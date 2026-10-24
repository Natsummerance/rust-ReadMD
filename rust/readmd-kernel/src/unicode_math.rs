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
    fn is_empty(&self) -> bool {
        match &self.kind {
            Kind::Text(value) => value.is_empty(),
            Kind::Row(args) => args.iter().all(Self::is_empty),
            Kind::Group(_, body, _) => body.is_empty(),
            _ => false,
        }
    }
    fn is_operator(&self) -> bool {
        matches!(&self.kind, Kind::Text(value) if value.chars().all(|c| "+-−=<>:,;.*×·÷/⁄∕¦".contains(c)))
    }
}

struct Parser<'a> {
    source: &'a str,
    at: usize,
    items: usize,
    diagnostics: Vec<Diagnostic>,
    // In sin⁡^2 x, the name's contiguous source span also contains ⁡.
    // Preserve that operator when a name slot is replaced.
    function_markers: BTreeMap<(usize, usize), (usize, usize)>,
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
    fn function_marker(&self) -> Option<(usize, usize)> {
        let rest = &self.source[self.at..];
        let trimmed = rest.trim_start();
        let from = self.at + rest.len() - trimmed.len();
        if trimmed.starts_with('\u{2061}') {
            Some((from, from + '\u{2061}'.len_utf8()))
        } else {
            trimmed.strip_prefix("\\funcapply").filter(|tail| {
                !tail.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            })?;
            Some((from, from + "\\funcapply".len()))
        }
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
        if self.function_marker().is_some() {
            return Err("math_missing_argument");
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
        let name = self.scripts(atom, None, false, depth + 1)?;
        let Some((marker_from, marker_to)) = self.function_marker() else {
            return Ok(name);
        };
        if name.is_empty() || name.is_operator() {
            return Err("math_missing_argument");
        }
        self.spaces()?;
        while self.at < marker_to {
            self.next()?;
        }
        // UTN28 §3.5 moves scripts immediately following ⁡ onto the name.
        let name = self.scripts(name, None, true, depth + 1)?;
        if marker_from < name.to {
            self.function_markers
                .insert((name.from, name.to), (marker_from, marker_to));
        }
        self.spaces()?;
        let argument = self.operand(depth + 1)?;
        if argument.is_empty() || argument.is_operator() {
            return Err("math_missing_argument");
        }
        let (from, to) = (name.from, argument.to);
        // Visible argument parentheses are significant; only 〖〗 disappear.
        let argument = if matches!(&argument.kind, Kind::Group('〖', _, '〗')) {
            argument.ungroup()
        } else {
            argument
        };
        Ok(Expr::call(from, to, "function", vec![name, argument]))
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
        self.scripts(atom, Some(marker), false, depth + 1)
    }
    fn scripts(
        &mut self,
        mut base: Expr,
        only: Option<char>,
        whole_base: bool,
        depth: usize,
    ) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        let mut sub = None;
        let mut sup = None;
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
        let mut prefix = None;
        if !whole_base
            && (sub.is_some() || sup.is_some())
            && (only.is_some() || self.function_marker().is_none())
        {
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
        function_markers: BTreeMap::new(),
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
    if source.len() > MAX_SOURCE || text.len() > MAX_SOURCE {
        return Err("math_budget");
    }
    let mut parser = Parser {
        source: text,
        at: 0,
        items: 0,
        diagnostics: vec![],
        function_markers: BTreeMap::new(),
    };
    let expr = parser.expression(&[], 0)?;
    if !parser.diagnostics.is_empty() {
        return Err("math_unknown_macro");
    }
    let compound = matches!(
        expr.kind,
        Kind::Row(_) | Kind::Call("frac" | "function", _, _)
    );
    let from = crate::document_transaction::byte_offset(source, from)?;
    let to = crate::document_transaction::byte_offset(source, to)?;
    let already_grouped = matches!(
        (source[..from].chars().next_back(), source[to..].chars().next()),
        (Some('('), Some(')')) | (Some('〖'), Some('〗'))
    );
    let mut replacement = if compound && !already_grouped {
        format!("({text})")
    } else {
        text.into()
    };
    let mut original = Parser {
        source,
        at: 0,
        items: 0,
        diagnostics: vec![],
        function_markers: BTreeMap::new(),
    };
    original.expression(&[], 0)?;
    if let Some((marker_from, marker_to)) = original.function_markers.get(&(from, to)) {
        replacement.push_str(&source[*marker_from..*marker_to]);
    }
    Ok(replacement)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml(source: &str) -> String {
        compile(source, false).unwrap().0.xml()
    }
    fn find<'a>(node: &'a XmlNode, local: &str) -> Option<&'a XmlNode> {
        if node.local() == local {
            Some(node)
        } else {
            node.children().find_map(|child| find(child, local))
        }
    }
    #[test]
    fn explicit_functions_have_native_name_and_argument_roles() {
        for source in ["sin⁡(x+1)", r"sin\funcapply(x+1)", "sin ⁡ (x+1)"] {
            let (root, nodes, diagnostics) = compile(source, false).unwrap();
            assert!(diagnostics.is_empty(), "{source}");
            let function = find(&root, "func").unwrap();
            assert_eq!(find(function, "fName").unwrap().text(), "sin");
            let argument = function.children().find(|n| n.local() == "e").unwrap();
            assert_eq!(argument.text(), "x+1");
            assert_eq!(argument.children().next().unwrap().local(), "d");
            assert!(!root.text().contains('⁡'));
            let node = nodes.iter().find(|n| n.kind == "func").unwrap();
            assert_eq!(node.slots.iter().map(|s| s.role.as_str()).collect::<Vec<_>>(), ["fName", "e"]);
            let dsl = crate::math_dsl::decompile(&root).unwrap();
            assert!(dsl.contains("function("));
            assert_eq!(crate::math_dsl::compile(&dsl, false).unwrap().0.xml(), root.xml());
        }
    }
    #[test]
    fn function_name_scripts_bind_before_and_immediately_after_apply() {
        for source in ["sin^2⁡x", "sin⁡^2 x", r"sin\funcapply^2 x"] {
            let (root, _, diagnostics) = compile(source, false).unwrap();
            assert!(diagnostics.is_empty(), "{source}");
            let function = find(&root, "func").unwrap();
            let name = find(function, "fName").unwrap();
            let script = name.children().next().unwrap();
            assert_eq!(script.local(), "sSup");
            assert_eq!(script.children().find(|n| n.local() == "e").unwrap().text(), "sin");
            assert_eq!(script.children().find(|n| n.local() == "sup").unwrap().text(), "2");
            assert_eq!(function.children().find(|n| n.local() == "e").unwrap().text(), "x");
        }
        let (root, _, _) = compile("sin^ab^c⁡x", false).unwrap();
        let name = find(&root, "fName").unwrap();
        let exponent = name.children().next().unwrap().children().find(|n| n.local() == "sup").unwrap();
        let mut parts = exponent.children();
        assert_eq!(parts.next().unwrap().text(), "a");
        let nested = parts.next().unwrap();
        assert_eq!(nested.local(), "sSup");
        assert_eq!(nested.children().find(|n| n.local() == "e").unwrap().text(), "b");
    }
    #[test]
    fn functions_associate_right_and_bracketed_arguments_end_at_the_close() {
        let (root, _, _) = compile("f⁡g⁡x", false).unwrap();
        let outer = find(&root, "func").unwrap();
        assert_eq!(find(outer, "fName").unwrap().text(), "f");
        let argument = outer.children().find(|n| n.local() == "e").unwrap();
        let inner = argument.children().next().unwrap();
        assert_eq!(inner.local(), "func");
        assert_eq!(find(inner, "fName").unwrap().text(), "g");
        assert_eq!(inner.children().find(|n| n.local() == "e").unwrap().text(), "x");

        let (root, _, _) = compile("sin⁡(x)y", false).unwrap();
        let math = root.children().next().unwrap();
        let mut children = math.children();
        let function = children.next().unwrap();
        assert_eq!(function.local(), "func");
        assert_eq!(function.children().find(|n| n.local() == "e").unwrap().text(), "x");
        assert_eq!(children.next().unwrap().text(), "y");
        assert!(children.next().is_none());
    }
    #[test]
    fn repeated_functions_keep_utf16_slots_and_patch_only_the_selected_argument() {
        use crate::math_document::{parse, patch, MathPatch};
        let source = "𝛼+sin⁡(x+1)+sin⁡(x+1)";
        let doc = parse(source, "unicodemath", false).unwrap();
        let functions: Vec<_> = doc.nodes.iter().filter(|n| n.kind == "func").collect();
        assert_eq!(functions.len(), 2);
        assert_ne!(functions[0].id, functions[1].id);
        for (node, from, to, name_from, name_to, arg_from, arg_to) in [
            (functions[0], 3, 12, 3, 6, 7, 12),
            (functions[1], 13, 22, 13, 16, 17, 22),
        ] {
            assert_eq!((node.from, node.to), (from, to));
            let name = node.slots.iter().find(|s| s.role == "fName").unwrap();
            let argument = node.slots.iter().find(|s| s.role == "e").unwrap();
            assert_eq!((name.from, name.to), (name_from, name_to));
            assert_eq!((argument.from, argument.to), (arg_from, arg_to));
        }
        let changed = patch(source, "unicodemath", false, &doc.revision, &MathPatch::ReplaceSlot {
            node_id: functions[1].id.clone(), role: "e".into(), text: "y+2".into(),
        }).unwrap();
        assert_eq!(changed.source, "𝛼+sin⁡(x+1)+sin⁡(y+2)");
        assert_eq!(changed.nodes.iter().filter(|n| n.kind == "func").count(), 2);
        assert!(crate::math_document::preview(&changed, false).unwrap().contains("&#x2061;"));
    }
    #[test]
    fn function_argument_hidden_grouping_survives_slot_replacement() {
        use crate::math_document::{parse, patch, MathPatch};
        let source = "sin⁡〖x+1〗";
        let doc = parse(source, "unicodemath", false).unwrap();
        assert!(!doc.root.xml().contains("<m:d>"));
        let function = doc.nodes.iter().find(|n| n.kind == "func").unwrap();
        assert_eq!((function.from, function.to), (0, 9));
        let argument = function.slots.iter().find(|s| s.role == "e").unwrap();
        assert_eq!((argument.from, argument.to), (5, 8));
        let changed = patch(source, "unicodemath", false, &doc.revision, &MathPatch::ReplaceSlot {
            node_id: function.id.clone(), role: "e".into(), text: "y+2".into(),
        }).unwrap();
        assert_eq!(changed.source, "sin⁡〖y+2〗");
        assert!(!changed.root.xml().contains("<m:d>"));
    }
    #[test]
    fn replacing_a_name_keeps_the_apply_marker_that_precedes_its_script() {
        use crate::math_document::{parse, patch, MathPatch};
        for (source, expected) in [
            ("sin⁡^2 x", "cos⁡ x"),
            (r"sin\funcapply^2 x", r"cos\funcapply x"),
        ] {
            let doc = parse(source, "unicodemath", false).unwrap();
            let function = doc.nodes.iter().find(|n| n.kind == "func").unwrap();
            let changed = patch(source, "unicodemath", false, &doc.revision, &MathPatch::ReplaceSlot {
                node_id: function.id.clone(), role: "fName".into(), text: "cos".into(),
            }).unwrap();
            assert_eq!(changed.source, expected);
            let function = find(&changed.root, "func").unwrap();
            assert_eq!(find(function, "fName").unwrap().text(), "cos");
            assert_eq!(function.children().find(|n| n.local() == "e").unwrap().text(), "x");
        }
    }
    #[test]
    fn explicit_function_errors_literals_and_macro_boundaries_are_distinct() {
        for source in ["⁡x", r"\funcapply x", "f⁡", "f⁡()", "f⁡〖〗", "()⁡x", "f⁡)", "f⁡+x", "f⁡⁡x", "a+⁡x"] {
            assert_eq!(compile(source, false).unwrap_err(), "math_missing_argument", "{source}");
        }
        for source in ["sin x", r"f\⁡x", "\"sin⁡x\""] {
            assert!(!xml(source).contains("<m:func>"), "{source}");
        }
        let (_, nodes, diagnostics) = compile(r"sin\funcapplyExtra x", false).unwrap();
        assert!(nodes.iter().all(|n| n.kind != "func"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "math_unknown_macro");
        assert_eq!(compile(&format!("{}x", "f⁡".repeat(MAX_DEPTH + 1)), false).unwrap_err(), "math_budget");
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
