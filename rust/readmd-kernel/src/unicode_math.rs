//! UnicodeMath import through the shared readable-math compiler. Never use a
//! TeX string as an intermediate: that loses Office structure and source ranges.
//! Ranges refer to the original input, including UTF-16 supplementary symbols.
use crate::math_document::{Diagnostic, SourceNode, XmlNode};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, &'static str>;
const MAX_SOURCE: usize = 16_000;
const MAX_DEPTH: usize = 64;
const MAX_ITEMS: usize = 4_000;

#[derive(Clone, Debug)]
struct Expr {
    from: usize,
    to: usize,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    Text(String),
    Row(Vec<Expr>),
    Call(&'static str, Vec<Expr>, Vec<(&'static str, Expr)>),
    List(Vec<Expr>),
    Group(char, Box<Expr>, char),
}
impl Expr {
    fn text(from: usize, to: usize, value: impl Into<String>) -> Self {
        Self {
            from,
            to,
            kind: Kind::Text(value.into()),
        }
    }
    fn call(from: usize, to: usize, name: &'static str, args: Vec<Self>) -> Self {
        Self {
            from,
            to,
            kind: Kind::Call(name, args, vec![]),
        }
    }
    fn row(from: usize, to: usize, args: Vec<Self>) -> Self {
        if args.len() == 1 {
            args.into_iter().next().unwrap()
        } else {
            Self {
                from,
                to,
                kind: Kind::Row(args),
            }
        }
    }
    fn option(mut self, name: &'static str, value: Self) -> Self {
        if let Kind::Call(_, _, options) = &mut self.kind {
            options.push((name, value));
        }
        self
    }
    fn ungroup(self) -> Self {
        match self.kind {
            Kind::Group('(', body, ')') | Kind::Group('〖', body, '〗') => *body,
            _ => self,
        }
    }
}

struct Parser<'a> {
    source: &'a str,
    at: usize,
    items: usize,
    diagnostics: Vec<Diagnostic>,
}
fn is_function(value: &str) -> bool {
    matches!(
        value,
        "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "sinh"
            | "cosh"
            | "tanh"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "log"
            | "ln"
            | "exp"
            | "lim"
            | "max"
            | "min"
            | "det"
            | "gcd"
            | "sup"
            | "inf"
    )
}
fn accent(value: &str) -> Option<&'static str> {
    Some(match value {
        "hat" => "̂",
        "vec" => "⃗",
        "bar" => "̅",
        "dot" => "̇",
        "ddot" => "̈",
        "tilde" => "̃",
        "breve" => "̆",
        "check" => "̌",
        "acute" => "́",
        "grave" => "̀",
        _ => return None,
    })
}
impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.at..].chars().next()
    }
    fn next(&mut self) -> Result<char> {
        self.items += 1;
        if self.items > MAX_ITEMS {
            return Err("math_budget");
        }
        let c = self.peek().ok_or("math_missing_argument")?;
        self.at += c.len_utf8();
        Ok(c)
    }
    fn spaces(&mut self) -> Result<()> {
        while self.peek().is_some_and(char::is_whitespace) {
            self.next()?;
        }
        Ok(())
    }
    fn expression(&mut self, stops: &[char], depth: usize) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        let from = self.at;
        let mut terms = Vec::<Expr>::new();
        let mut fractions = 0;
        while let Some(mut c) = self.peek() {
            let fraction_alias = if c == '\\' {
                [("\\sdiv", '⁄'), ("\\ldiv", '∕'), ("\\atop", '¦')]
                    .into_iter()
                    .find(|(word, _)| {
                        self.source[self.at..].starts_with(word)
                            && !self.source[self.at + word.len()..]
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_alphabetic())
                    })
            } else {
                None
            };
            if let Some((_, glyph)) = fraction_alias {
                c = glyph;
            }
            if stops.contains(&c) {
                break;
            }
            if c.is_whitespace() {
                self.spaces()?;
                continue;
            }
            if matches!(c, ')' | ']' | '}' | '〗') {
                return Err("math_unbalanced");
            }
            if matches!(c, '/' | '⁄' | '∕' | '¦') {
                fractions += 1;
                if fractions > MAX_DEPTH {
                    return Err("math_budget");
                }
                let slash = self.at;
                if let Some((word, _)) = fraction_alias {
                    for _ in 0..word.len() {
                        self.next()?;
                    }
                } else {
                    self.next()?;
                }
                let slash_end = self.at;
                let left = terms.pop().ok_or("math_missing_argument")?;
                if matches!(&left.kind, Kind::Text(t) if t.chars().all(|c| "+-=<>:,;".contains(c)))
                {
                    return Err("math_missing_argument");
                }
                self.spaces()?;
                let right = self.operand(depth + 1)?;
                if matches!(&right.kind, Kind::Text(t) if t.chars().all(|c| "+-=<>:,;".contains(c)))
                {
                    return Err("math_missing_argument");
                }
                let (start, end) = (left.from, right.to);
                let mut fraction = Expr::call(
                    start,
                    end,
                    "frac",
                    vec![
                        if c == '∕' { left } else { left.ungroup() },
                        if c == '∕' { right } else { right.ungroup() },
                    ],
                );
                let style = match c {
                    '⁄' => "skewed",
                    '∕' => "linear",
                    '¦' => "no_bar",
                    _ => "stacked",
                };
                fraction = fraction.option("style", Expr::text(slash, slash_end, style));
                terms.push(fraction);
                continue;
            }
            if c == '\\' {
                let word = self.source[self.at + 1..]
                    .chars()
                    .take_while(char::is_ascii_alphabetic)
                    .collect::<String>();
                if let Some(mark) = accent(&word).filter(|_| !terms.is_empty()) {
                    let from = self.at;
                    for _ in 0..=word.len() {
                        self.next()?;
                    }
                    let base = terms.pop().unwrap();
                    terms.push(
                        Expr::call(base.from, self.at, "accent", vec![base.ungroup()])
                            .option("character", Expr::text(from, self.at, mark)),
                    );
                    continue;
                }
            }
            terms.push(self.operand(depth + 1)?);
        }
        Ok(Expr::row(from, self.at, terms))
    }
    fn operand(&mut self, depth: usize) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        if matches!(self.peek(), Some('_' | '^')) {
            let from = self.at;
            let mut sub = Expr::row(from, from, vec![]);
            let mut sup = sub.clone();
            while let Some(c @ ('_' | '^')) = self.peek() {
                self.next()?;
                let arg = self.script_argument(c, depth + 1)?;
                if c == '_' {
                    sub = arg;
                } else {
                    sup = arg;
                }
            }
            self.spaces()?;
            let base = self.atom(depth + 1)?;
            return Ok(Expr::call(from, base.to, "prescript", vec![sub, sup, base]));
        }
        let atom = self.atom(depth + 1)?;
        self.scripts(atom, None, depth + 1)
    }
    fn script_argument(&mut self, marker: char, depth: usize) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        self.spaces()?;
        let from = self.at;
        let atom = if matches!(self.peek(), Some('+' | '-' | '−')) {
            let c = self.next()?;
            let sign = Expr::text(from, self.at, c.to_string());
            let value = self.atom(depth + 1)?;
            Expr::row(from, value.to, vec![sign, value])
        } else {
            self.atom(depth + 1)?
        };
        let atom = atom.ungroup();
        // Repeated scripts associate right; opposite scripts share one base.
        self.scripts(atom, Some(marker), depth + 1)
    }
    fn scripts(&mut self, mut base: Expr, only: Option<char>, depth: usize) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        let mut sub = None;
        let mut sup = None;
        let mut prefix = None;
        if matches!(self.peek(), Some('^' | '_')) {
            if let Kind::Text(value) = &base.kind {
                if !is_function(value)
                    && value.chars().count() > 1
                    && value.chars().all(char::is_alphanumeric)
                {
                    let (offset, last) = value.char_indices().last().unwrap();
                    prefix = Some(Expr::text(base.from, base.from + offset, &value[..offset]));
                    base = Expr::text(base.from + offset, base.to, last.to_string());
                }
            }
        }
        while let Some(c @ ('^' | '_')) = self.peek() {
            if only.is_some_and(|m| m != c) {
                break;
            }
            self.next()?;
            let value = self.script_argument(c, depth + 1)?;
            if c == '^' {
                sup = Some(value);
            } else {
                sub = Some(value);
            }
        }
        let from = base.from;
        let to = self.at.max(base.to);
        let limit = matches!(&base.kind, Kind::Text(name) if name == "lim" || name == "max" || name == "min" || name == "sup" || name == "inf");
        let value = match (sub, sup) {
            (Some(sub), Some(sup)) => {
                Expr::call(from, to, "subsup", vec![base.ungroup(), sub, sup])
            }
            (Some(sub), None) => Expr::call(
                from,
                to,
                if limit { "lower_limit" } else { "subscript" },
                vec![base.ungroup(), sub],
            ),
            (None, Some(sup)) => Expr::call(from, to, "superscript", vec![base.ungroup(), sup]),
            _ => base,
        };
        Ok(if let Some(prefix) = prefix {
            Expr::row(prefix.from, to, vec![prefix, value])
        } else {
            value
        })
    }
    fn group(&mut self, open: char, depth: usize) -> Result<Expr> {
        let from = self.at;
        self.next()?;
        let close = match open {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            _ => '〗',
        };
        let body = self.expression(&[close], depth + 1)?;
        if self.peek() != Some(close) {
            return Err("math_unbalanced");
        }
        self.next()?;
        Ok(Expr {
            from,
            to: self.at,
            kind: Kind::Group(open, Box::new(body), close),
        })
    }
    fn matrix(
        &mut self,
        from: usize,
        bracket: Option<(char, char)>,
        equations: bool,
        depth: usize,
    ) -> Result<Expr> {
        self.spaces()?;
        if self.next()? != '(' {
            return Err("math_missing_argument");
        }
        let mut rows = Vec::<Vec<Expr>>::new();
        let mut row = Vec::new();
        loop {
            if rows.len() >= 256 || row.len() >= 256 {
                return Err("math_budget");
            }
            row.push(self.expression(&['&', '@', ')'], depth + 1)?);
            match self.peek() {
                Some('&') => {
                    self.next()?;
                }
                Some('@') => {
                    self.next()?;
                    rows.push(std::mem::take(&mut row));
                }
                Some(')') => {
                    self.next()?;
                    rows.push(row);
                    break;
                }
                _ => return Err("math_unbalanced"),
            }
        }
        let to = self.at;
        let columns = rows.iter().map(Vec::len).max().unwrap_or(1);
        let args = rows
            .into_iter()
            .map(|mut cells| {
                if equations {
                    let mut aligned = Vec::new();
                    for (i, cell) in cells.into_iter().enumerate() {
                        if i > 0 {
                            let at = cell.from;
                            aligned.push(Expr::call(
                                at,
                                at,
                                "office.r",
                                vec![
                                    Expr::call(
                                        at,
                                        at,
                                        "office.rPr",
                                        vec![Expr::call(at, at, "office.aln", vec![])],
                                    ),
                                    Expr::call(at, at, "office.t", vec![Expr::text(at, at, "")]),
                                ],
                            ));
                        }
                        aligned.push(cell);
                    }
                    Expr::row(from, to, aligned)
                } else {
                    while cells.len() < columns {
                        cells.push(Expr::row(to - 1, to - 1, vec![]));
                    }
                    Expr {
                        from,
                        to,
                        kind: Kind::List(cells),
                    }
                }
            })
            .collect();
        let matrix = Expr::call(
            from,
            to,
            if equations { "equations" } else { "matrix" },
            args,
        );
        Ok(if let Some((open, close)) = bracket {
            Expr::call(from, to, "delimiter", vec![matrix])
                .option("open", Expr::text(from, from, open.to_string()))
                .option("close", Expr::text(to, to, close.to_string()))
        } else {
            matrix
        })
    }
    fn radical(&mut self, from: usize, depth: usize) -> Result<Expr> {
        self.spaces()?;
        if self.peek() == Some('(') {
            self.next()?;
            let first = self.expression(&['&', ')'], depth + 1)?;
            let args = if self.peek() == Some('&') {
                self.next()?;
                vec![first, self.expression(&[')'], depth + 1)?]
            } else {
                vec![first]
            };
            if self.next()? != ')' {
                return Err("math_unbalanced");
            }
            Ok(Expr::call(
                from,
                self.at,
                if args.len() == 2 { "root" } else { "sqrt" },
                args,
            ))
        } else {
            let arg = self.operand(depth + 1)?;
            Ok(Expr::call(from, arg.to, "sqrt", vec![arg]))
        }
    }
    fn large_operator(&mut self, from: usize, character: char, depth: usize) -> Result<Expr> {
        let mut sub = None;
        let mut sup = None;
        while let Some(c @ ('_' | '^')) = self.peek() {
            self.next()?;
            let arg = self.script_argument(c, depth + 1)?;
            if c == '_' {
                sub = Some(arg);
            } else {
                sup = Some(arg);
            }
        }
        self.spaces()?;
        if self.peek() == Some('▒') {
            self.next()?;
            self.spaces()?;
        }
        let body = self.expression(&[')', ']', '}', '〗', '&', '@'], depth + 1)?;
        let mut value = Expr::call(from, self.at, "nary", vec![body]).option(
            "character",
            Expr::text(
                from,
                from + character.len_utf8().min(self.source.len() - from),
                character.to_string(),
            ),
        );
        if let Some(sub) = sub {
            value = value.option("from", sub);
        }
        if let Some(sup) = sup {
            value = value.option("to", sup);
        }
        Ok(value)
    }
    fn atom(&mut self, depth: usize) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        let from = self.at;
        let c = self.peek().ok_or("math_missing_argument")?;
        if matches!(c, '(' | '[' | '{' | '〖') {
            return self.group(c, depth + 1);
        }
        if matches!(c, ')' | ']' | '}' | '〗' | '/' | '^' | '_' | '@' | '&') {
            return Err("math_missing_argument");
        }
        self.next()?;
        if c == '"' {
            let mut value = String::new();
            loop {
                match self.next()? {
                    '"' => break,
                    '\\' => value.push(self.next()?),
                    c => value.push(c),
                }
            }
            return Ok(Expr::text(from, self.at, value));
        }
        if c == '√' {
            return self.radical(from, depth + 1);
        }
        if c == '■' {
            return self.matrix(from, None, false, depth + 1);
        }
        if matches!(
            c,
            '∑' | '∏' | '∐' | '∫' | '∬' | '∭' | '∮' | '∯' | '∰' | '⋃' | '⋂'
        ) {
            return self.large_operator(from, c, depth + 1);
        }
        if c == '\\' {
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                self.next()?;
            }
            let word = &self.source[from + 1..self.at];
            match word {
                "matrix" | "bmatrix" | "pmatrix" | "vmatrix" | "Bmatrix" | "Vmatrix"
                | "eqarray" => {
                    let bracket = match word {
                        "bmatrix" => Some(('[', ']')),
                        "pmatrix" => Some(('(', ')')),
                        "vmatrix" => Some(('|', '|')),
                        "Bmatrix" => Some(('{', '}')),
                        "Vmatrix" => Some(('‖', '‖')),
                        _ => None,
                    };
                    return self.matrix(from, bracket, word == "eqarray", depth + 1);
                }
                "sqrt" => return self.radical(from, depth + 1),
                "rect" | "overbar" | "underbar" | "overbrace" | "underbrace" => {
                    self.spaces()?;
                    let arg = self.operand(depth + 1)?.ungroup();
                    let (name, position, character) = match word {
                        "rect" => ("border_box", None, None),
                        "overbar" => ("bar", Some("above"), None),
                        "underbar" => ("bar", Some("below"), None),
                        "overbrace" => ("group_char", Some("above"), Some("⏞")),
                        _ => ("group_char", Some("below"), Some("⏟")),
                    };
                    let mut value = Expr::call(from, self.at, name, vec![arg]);
                    if let Some(position) = position {
                        value = value.option("position", Expr::text(from, from, position));
                    }
                    if let Some(character) = character {
                        value = value.option("character", Expr::text(from, from, character));
                    }
                    return Ok(value);
                }
                _ => {}
            }
            if let Some(mark) = accent(word) {
                self.spaces()?;
                let value = self.operand(depth + 1)?.ungroup();
                return Ok(Expr::call(from, self.at, "accent", vec![value])
                    .option("character", Expr::text(from, from, mark)));
            }
            let alias = match word {
                "infty" => Some("∞"),
                "to" => Some("→"),
                "sdiv" => Some("⁄"),
                "ldiv" => Some("∕"),
                _ => None,
            };
            let key = format!("\\{word}");
            if let Some(glyph) = alias.or_else(|| crate::math_catalog::symbol(&key)) {
                if let Some(op) = glyph
                    .chars()
                    .next()
                    .filter(|_| glyph.chars().count() == 1)
                    .filter(|c| matches!(c, '∑' | '∏' | '∫' | '∬' | '∭' | '∮'))
                {
                    return self.large_operator(from, op, depth + 1);
                }
                return Ok(Expr::text(from, self.at, glyph));
            }
            if word.is_empty() {
                let escaped = self.next()?;
                return Ok(Expr::text(from, self.at, escaped.to_string()));
            }
            self.diagnostics.push(Diagnostic {
                code: "math_unknown_macro".into(),
                from: self.source[..from].encode_utf16().count(),
                to: self.source[..self.at].encode_utf16().count(),
            });
            return Ok(Expr::text(from, self.at, &self.source[from..self.at]));
        }
        if c.is_alphanumeric() {
            while self
                .peek()
                .is_some_and(|c| c.is_alphanumeric() || c == '\u{2063}')
            {
                self.next()?;
            }
            // Decimal punctuation is part of a numeric operand only between digits.
            while matches!(self.peek(), Some('.' | ','))
                && self.source[..self.at]
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_digit())
                && self.source[self.at + 1..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                self.next()?;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.next()?;
                }
            }
        }
        Ok(Expr::text(from, self.at, &self.source[from..self.at]))
    }
}

#[derive(Default)]
struct Emitter {
    output: String,
    positions: BTreeMap<(usize, usize), (usize, usize)>,
}
impl Emitter {
    fn emit(&mut self, expr: &Expr) {
        let start = self.output.encode_utf16().count();
        match &expr.kind {
            Kind::Text(value) => self.output.push_str(&serde_json::to_string(value).unwrap()),
            Kind::Row(args) => self.call(if args.is_empty() { "empty" } else { "row" }, args, &[]),
            Kind::Call(name, args, named) => self.call(name, args, named),
            Kind::List(args) => {
                self.output.push('[');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit(arg);
                }
                self.output.push(']');
            }
            Kind::Group('〖', body, '〗') => self.emit(body),
            Kind::Group(open, body, close) => {
                self.output.push_str("delimiter(");
                self.emit(body);
                self.output.push_str(&format!(
                    ", open: {}, close: {})",
                    serde_json::to_string(&open.to_string()).unwrap(),
                    serde_json::to_string(&close.to_string()).unwrap()
                ));
            }
        }
        self.positions.insert(
            (start, self.output.encode_utf16().count()),
            (expr.from, expr.to),
        );
    }
    fn call(&mut self, name: &str, args: &[Expr], named: &[(&str, Expr)]) {
        self.output.push_str(name);
        self.output.push('(');
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            self.emit(arg);
        }
        for (i, (key, value)) in named.iter().enumerate() {
            if !args.is_empty() || i > 0 {
                self.output.push_str(", ");
            }
            self.output.push_str(key);
            self.output.push_str(": ");
            self.emit(value);
        }
        self.output.push(')');
    }
}

pub fn compile(source: &str, display: bool) -> Result<(XmlNode, Vec<SourceNode>, Vec<Diagnostic>)> {
    if source.len() > MAX_SOURCE || source.contains('\0') {
        return Err("math_budget");
    }
    let mut parser = Parser {
        source,
        at: 0,
        items: 0,
        diagnostics: vec![],
    };
    let tree = parser.expression(&[], 0)?;
    let mut pending = vec![(&tree, 0)];
    while let Some((expr, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        match &expr.kind {
            Kind::Row(args) | Kind::List(args) => {
                pending.extend(args.iter().map(|arg| (arg, depth + 1)))
            }
            Kind::Call(_, args, named) => {
                pending.extend(args.iter().map(|arg| (arg, depth + 1)));
                pending.extend(named.iter().map(|(_, arg)| (arg, depth + 1)));
            }
            Kind::Group(_, body, _) => pending.push((body, depth + 1)),
            _ => {}
        }
    }
    let mut emitter = Emitter::default();
    emitter.emit(&tree);
    let (root, mut nodes) = crate::math_dsl::compile(&emitter.output, display)?;
    for node in &mut nodes {
        let (from, to) = *emitter
            .positions
            .get(&(node.from, node.to))
            .ok_or("math_invalid_source")?;
        node.tex = source[from..to].into();
        node.from = source[..from].encode_utf16().count();
        node.to = source[..to].encode_utf16().count();
        for slot in &mut node.slots {
            let (from, to) = *emitter
                .positions
                .get(&(slot.from, slot.to))
                .ok_or("math_invalid_source")?;
            slot.from = source[..from].encode_utf16().count();
            slot.to = source[..to].encode_utf16().count();
        }
    }
    Ok((root, nodes, parser.diagnostics))
}

/// A slot replacement is one operand, even when the user types an ungrouped
/// sum or fraction. Reuse existing syntactic parentheses without doubling them.
pub(crate) fn slot_replacement(source: &str, from: usize, to: usize, text: &str) -> Result<String> {
    if text.len() > MAX_SOURCE {
        return Err("math_budget");
    }
    let mut parser = Parser {
        source: text,
        at: 0,
        items: 0,
        diagnostics: vec![],
    };
    let expr = parser.expression(&[], 0)?;
    if !parser.diagnostics.is_empty() {
        return Err("math_unknown_macro");
    }
    let compound = matches!(expr.kind, Kind::Row(_) | Kind::Call("frac", _, _));
    let from = crate::document_transaction::byte_offset(source, from)?;
    let to = crate::document_transaction::byte_offset(source, to)?;
    let already_grouped =
        source[..from].chars().next_back() == Some('(') && source[to..].starts_with(')');
    Ok(if compound && !already_grouped {
        format!("({text})")
    } else {
        text.into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml(source: &str) -> String {
        compile(source, false).unwrap().0.xml()
    }
    #[test]
    fn fraction_styles_grouping_and_associativity() {
        for (source, style) in [
            ("a/b", "bar"),
            ("a⁄b", "skw"),
            ("a∕b", "lin"),
            ("a¦b", "noBar"),
        ] {
            assert!(xml(source).contains(&format!("m:val=\"{style}\"")));
        }
        assert_eq!(xml("a/b/c").matches("<m:f>").count(), 2);
        assert!(!xml("(a+b)/(c+d)").contains("<m:d>"));
        assert!(xml("((a+b))/c").contains("<m:d>"));
        assert!(xml("[a+b]/c").contains("<m:d>"));
        assert!(xml("(a+b)∕c").contains("<m:d>"));
        assert!(xml("(a+b)").contains("<m:d>"));
        assert!(xml("1/3.1416").contains(">3.1416<"));
        assert!(xml(r"a\sdiv b").contains("m:val=\"skw\""));
        assert!(xml(r"a\ldiv b").contains("m:val=\"lin\""));
        assert!(xml(r"a\atop b").contains("m:val=\"noBar\""));
    }
    #[test]
    fn scripts_bind_to_character_and_preserve_right_association() {
        let root = compile("ab^c_d", false).unwrap().0;
        let math = root.children().next().unwrap();
        assert_eq!(math.children().next().unwrap().text(), "a");
        let script = math.children().find(|n| n.local() == "sSubSup").unwrap();
        assert_eq!(
            script.children().find(|n| n.local() == "e").unwrap().text(),
            "b"
        );
        let value = xml("a^b^c");
        assert_eq!(value.matches("<m:sSup>").count(), 2);
        assert!(!xml("a^2 b").contains("<m:t>2b</m:t>"));
        assert!(xml("x^(-2)").contains("<m:sSup>"));
        assert!(!xml("x^(-2)").contains("<m:d>"));
    }
    #[test]
    fn matrices_keep_nested_boundaries_and_pad_empty_cells() {
        for source in [
            "■(a&b@c&d)",
            r"\matrix(a&b@c)",
            r"\pmatrix((a+b)&\sqrt(2)@c&d)",
        ] {
            let (root, _, _) = compile(source, false).unwrap();
            let output = root.xml();
            assert!(output.contains("<m:m>"));
            assert_eq!(output.matches("<m:mr>").count(), 2);
            fn matrix(node: &XmlNode) -> Option<&XmlNode> {
                if node.local() == "m" {
                    Some(node)
                } else {
                    node.children().find_map(matrix)
                }
            }
            let matrix = matrix(&root).unwrap();
            for row in matrix.children().filter(|n| n.local() == "mr") {
                assert_eq!(row.children().filter(|n| n.local() == "e").count(), 2);
            }
        }
        assert!(xml(r"\eqarray(x+1&=2@3/x&=6)").contains("<m:aln/>"));
    }
    #[test]
    fn office_roots_accents_bars_and_large_operators_are_native_objects() {
        for (source, tag) in [
            (r"\sqrt(5&a^2)", "rad"),
            (r"(abc)\vec", "acc"),
            (r"\overbar(abc)", "bar"),
            (r"\overbrace(a+b)", "groupChr"),
            (r"\rect(a/b)", "borderBox"),
            (r"\iint_(a=0)^\infty▒a", "nary"),
            ("∑_(i=1)^n▒x_i", "nary"),
            ("_a^b x", "sPre"),
            (r"lim_(n→\infty) n", "limLow"),
        ] {
            let output = xml(source);
            assert!(output.contains(&format!("<m:{tag}>")), "{source}: {output}");
        }
    }
    #[test]
    fn unknown_commands_keep_exact_original_utf16_diagnostics() {
        let source = "𝛼 + \\notReadMD(x)";
        let (root, nodes, diagnostics) = compile(source, false).unwrap();
        assert!(root.text().contains("\\notReadMD"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!((diagnostics[0].from, diagnostics[0].to), (5, 15));
        for node in nodes {
            assert_eq!(
                crate::document_transaction::byte_offset(source, node.from).is_ok(),
                true
            );
        }
        for source in ["a/(b", "a/", "a/+b", "a/=b", "x^", "■(a&b", "(a]", "\\"] {
            assert!(compile(source, false).is_err(), "{source}");
        }
        assert!(compile(&"(".repeat(100), false).is_err());
        assert!(compile(&"x".repeat(MAX_SOURCE + 1), false).is_err());
        assert!(compile(&format!("a{}", "/b".repeat(1000)), false).is_err());
    }
}
