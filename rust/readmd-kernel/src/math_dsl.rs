//! Readable, bounded mathematical syntax. Standard Office properties are plain
//! text; this adapter never embeds compressed XML in the user's expression.
use crate::math_document::{self, SourceNode, SourceSlot, XmlNode, XmlPart, MATH_NS, WORD_NS, DRAWING_NS};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_SOURCE: usize = 64 * 1024;
const MAX_DEPTH: usize = 96;
const MAX_TOKENS: usize = 12000;
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Debug)]
struct Token {
    text: String,
    from: usize,
    to: usize,
    quoted: bool,
}
#[derive(Clone, Debug)]
struct Expr {
    from: usize,
    to: usize,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    Atom(String, bool),
    Call(String, Vec<Expr>, BTreeMap<String, Expr>),
    List(Vec<Expr>),
    Group(Box<Expr>),
    Binary(Token, Box<Expr>, Box<Expr>),
    Unary(Token, Box<Expr>),
}

fn lex(source: &str) -> Result<Vec<Token>> {
    if source.len() > MAX_SOURCE {
        return Err("math_budget");
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let from = i;
        let c = source[i..].chars().next().unwrap();
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if source[i..].starts_with("//") || c == '#' {
            i += source[i..].find('\n').unwrap_or(source.len() - i);
            continue;
        }
        let quoted = c == '"';
        let text = if quoted {
            i += 1;
            let mut escaped = false;
            let mut closed = false;
            while i < source.len() {
                let c = source[i..].chars().next().unwrap();
                i += c.len_utf8();
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("math_invalid_source");
            }
            serde_json::from_str::<String>(&source[from..i]).map_err(|_| "math_invalid_source")?
        } else if c.is_alphanumeric() || c == '_' {
            i += c.len_utf8();
            while i < source.len() {
                let c = source[i..].chars().next().unwrap();
                if !(c.is_alphanumeric() || c == '_' || c == '.') {
                    break;
                }
                i += c.len_utf8();
            }
            source[from..i].into()
        } else {
            i += c.len_utf8();
            if matches!(c, '<' | '>' | '!') && source[i..].starts_with('=') {
                i += 1;
            }
            source[from..i].into()
        };
        out.push(Token {
            text,
            from,
            to: i,
            quoted,
        });
        if out.len() > MAX_TOKENS {
            return Err("math_budget");
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}
impl Parser {
    fn is(&self, s: &str) -> bool {
        self.tokens
            .get(self.at)
            .is_some_and(|t| !t.quoted && t.text == s)
    }
    fn take(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<()> {
        if self.take(s) {
            Ok(())
        } else {
            Err("math_invalid_source")
        }
    }
    fn expression(&mut self, min: u8, depth: usize) -> Result<Expr> {
        if depth > MAX_DEPTH {
            return Err("math_budget");
        }
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or("math_invalid_source")?;
        self.at += 1;
        let mut left = if !token.quoted && matches!(token.text.as_str(), "+" | "-" | "−") {
            let value = self.expression(25, depth + 1)?;
            Expr {
                from: token.from,
                to: value.to,
                kind: Kind::Unary(token, Box::new(value)),
            }
        } else if !token.quoted && token.text == "(" {
            let value = self.expression(0, depth + 1)?;
            self.expect(")")?;
            Expr {from:token.from,to:self.tokens[self.at-1].to,kind:Kind::Group(Box::new(value))}
        } else if !token.quoted && token.text == "[" {
            let mut items = Vec::new();
            if !self.is("]") {
                loop {
                    items.push(self.expression(0, depth + 1)?);
                    if !self.take(",") {
                        break;
                    }
                }
            }
            self.expect("]")?;
            Expr {
                from: token.from,
                to: self.tokens[self.at - 1].to,
                kind: Kind::List(items),
            }
        } else if !token.quoted && self.take("(") {
            if !token.text.chars().next().is_some_and(char::is_alphabetic) {
                return Err("math_invalid_source");
            }
            let mut args = Vec::new();
            let mut named = BTreeMap::new();
            if !self.is(")") {
                loop {
                    if self
                        .tokens
                        .get(self.at + 1)
                        .is_some_and(|t| !t.quoted && t.text == ":")
                    {
                        let key = self.tokens[self.at].text.clone();
                        self.at += 2;
                        if named.insert(key, self.expression(0, depth + 1)?).is_some() {
                            return Err("math_invalid_property");
                        }
                    } else {
                        if !named.is_empty() {
                            return Err("math_invalid_source");
                        }
                        args.push(self.expression(0, depth + 1)?);
                    }
                    if !self.take(",") {
                        break;
                    }
                }
            }
            self.expect(")")?;
            Expr {
                from: token.from,
                to: self.tokens[self.at - 1].to,
                kind: Kind::Call(token.text, args, named),
            }
        } else {
            if !token.quoted
                && matches!(
                    token.text.as_str(),
                    ")" | "]" | "," | ":" | "/" | "*" | "^" | "="
                )
            {
                return Err("math_invalid_source");
            }
            Expr {
                from: token.from,
                to: token.to,
                kind: Kind::Atom(token.text, token.quoted),
            }
        };
        loop {
            let Some(token) = self.tokens.get(self.at) else {
                break;
            };
            let power = if token.quoted {
                0
            } else {
                match token.text.as_str() {
                    "=" | "<" | ">" | "<=" | ">=" | "!=" | "≤" | "≥" | "≠" => 5,
                    "+" | "-" | "−" => 10,
                    "*" | "/" | "×" | "·" | "÷" => 20,
                    "^" => 30,
                    _ => 0,
                }
            };
            if power == 0 || power < min {
                break;
            }
            let operator = token.clone();
            self.at += 1;
            let right =
                self.expression(if operator.text == "^" { power } else { power + 1 }, depth + 1)?;
            left = Expr {
                from: left.from,
                to: right.to,
                kind: Kind::Binary(operator, Box::new(left), Box::new(right)),
            };
        }
        Ok(left)
    }
}

fn node(name: &str, content: Vec<XmlNode>) -> XmlNode {
    XmlNode {
        id: String::new(),
        name: format!("m:{name}"),
        namespace: MATH_NS.into(),
        attributes: BTreeMap::new(),
        attribute_namespaces: BTreeMap::new(),
        content: content.into_iter().map(XmlPart::Element).collect(),
    }
}
fn run(text: &str) -> XmlNode {
    let mut t = node("t", vec![]);
    t.content.push(XmlPart::Text(text.into()));
    node("r", vec![t])
}
fn mark_at(mut n: XmlNode, from: usize, to: usize) -> XmlNode {
    n.attributes.insert("dslFrom".into(), from.to_string());
    n.attributes.insert("dslTo".into(), to.to_string());
    n
}
fn mark(n: XmlNode, e: &Expr) -> XmlNode { mark_at(n,e.from,e.to) }
fn scalar(e: &Expr) -> Result<String> {
    match &e.kind {
        Kind::Atom(s, _) => Ok(s.clone()),
        Kind::Group(n) => scalar(n),
        Kind::Unary(s, n) if matches!(s.text.as_str(),"-"|"−") => Ok(format!("-{}", scalar(n)?)),
        _ => Err("math_invalid_property"),
    }
}
fn property(name: &str, value: &str) -> Result<XmlNode> {
    math_document::validate_value(name, value)?;
    let mut n = node(name, vec![]);
    n.attributes.insert("m:val".into(), value.into());
    Ok(n)
}
fn slot(name: &str, e: &Expr) -> Result<XmlNode> {
    Ok(mark(node(name, compile_expr(e)?), e))
}
fn definition(name: &str) -> Option<&serde_json::Value> {
    math_document::schema()["elements"]
        .as_array()?
        .iter()
        .find(|e| e["element"] == name && e["abstract"] != true)
}

/// Validate the pinned SDK's particle grammar, including namespaces, order,
/// choices and cardinality. Occurs absent means 1..1; an explicit empty Occurs
/// object means 0..unbounded (the SDK model's zero-valued Min/Max).
/// See the pinned Particle/ParticleVersion models linked in math-dsl-v1.md.
struct ParticleMatcher<'a> {
    children: Vec<&'a XmlNode>,
    memo: BTreeMap<(usize, usize), BTreeSet<usize>>,
    budget: usize,
}
impl ParticleMatcher<'_> {
    fn matches(&mut self, particle: &serde_json::Value, at: usize) -> Result<BTreeSet<usize>> {
        let key = (particle as *const _ as usize, at);
        if let Some(found) = self.memo.get(&key) {
            return Ok(found.clone());
        }
        self.budget = self.budget.checked_sub(1).ok_or("math_budget")?;
        let occurs = particle["Occurs"].as_array().and_then(|a| a.first());
        let min = occurs.map_or(1, |o| o["Min"].as_u64().unwrap_or(0) as usize);
        let max = occurs.map_or(1, |o| o["Max"].as_u64().unwrap_or(0) as usize);
        let max = if max == 0 {
            self.children.len() + min + 1
        } else {
            max
        };
        let mut frontier = BTreeSet::from([at]);
        let mut ends = if min == 0 {
            frontier.clone()
        } else {
            BTreeSet::new()
        };
        for count in 1..=max {
            let mut next = BTreeSet::new();
            for position in &frontier {
                next.extend(self.once(particle, *position)?);
            }
            if next.is_empty() {
                break;
            }
            if count >= min {
                ends.extend(&next);
            }
            if count >= min && next == frontier {
                break;
            }
            frontier = next;
        }
        self.memo.insert(key, ends.clone());
        Ok(ends)
    }
    fn once(&mut self, p: &serde_json::Value, at: usize) -> Result<BTreeSet<usize>> {
        if let Some(name) = p["Name"].as_str() {
            let qualified = name.rsplit('/').next().unwrap_or(name);
            let (prefix, local) = qualified.split_once(':').ok_or("math_invalid_property")?;
            let namespace = match prefix {
                "m" => MATH_NS,
                "w" => WORD_NS,
                _ => return Ok(BTreeSet::new()),
            };
            return Ok(
                if self
                    .children
                    .get(at)
                    .is_some_and(|n| n.local() == local && n.namespace == namespace
                        ||namespace==MATH_NS&&local=="r"&&crate::math_spacing::is_spacing(n))
                {
                    BTreeSet::from([at + 1])
                } else {
                    BTreeSet::new()
                },
            );
        }
        let items = p["Items"].as_array().ok_or("math_invalid_property")?;
        match p["Kind"].as_str() {
            Some("Choice") => {
                let mut ends = BTreeSet::new();
                for child in items {
                    ends.extend(self.matches(child, at)?);
                }
                Ok(ends)
            }
            Some("Sequence" | "Group") => {
                let mut ends = BTreeSet::from([at]);
                for child in items {
                    let mut next = BTreeSet::new();
                    for position in ends {
                        next.extend(self.matches(child, position)?);
                    }
                    ends = next;
                    if ends.is_empty() {
                        break;
                    }
                }
                Ok(ends)
            }
            _ => Err("math_invalid_property"),
        }
    }
}
fn validate_tree(n: &XmlNode) -> Result<()> {
    if n.namespace==crate::math_spacing::MATHML_NS{crate::math_spacing::Spacing::from_xml(n)?;}
    if n.namespace == MATH_NS {
        let def = definition(n.local()).ok_or("math_invalid_property")?;
        for attr in def["attributes"].as_array().into_iter().flatten() {
            let key = attr["name"].as_str().ok_or("math_invalid_property")?;
            let Some(value) = n.attributes.get(key) else {
                if attr["required"] == true && math_document::office_implicit_value(n.local(),key).is_none() {
                    return Err("math_invalid_property");
                }
                continue;
            };
            math_document::validate_constraints(attr, value)?;
            if key == "m:val" {
                math_document::validate_value(n.local(), value)?;
            }
        }
        let children: Vec<_> = n.children().collect();
        if let Some(particle) = def.get("particle").filter(|p| !p.is_null()) {
            let length = children.len();
            let mut matcher = ParticleMatcher {
                children,
                memo: BTreeMap::new(),
                budget: 250_000,
            };
            if !matcher.matches(particle, 0)?.contains(&length) {
                return Err("math_invalid_property");
            }
        } else if !children.is_empty() {
            return Err("math_invalid_property");
        }
    }
    for child in n.children() {
        validate_tree(child)?;
    }
    Ok(())
}
fn alias(text: &str) -> String {
    let name = format!("\\{text}");
    crate::math_catalog::symbol(&name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| text.into())
}
fn compile_expr(e: &Expr) -> Result<Vec<XmlNode>> {
    let n = match &e.kind {
        Kind::Atom(s, quoted) => {
            if !quoted {
                if let Some((base, sub)) = s
                    .split_once('_')
                    .filter(|(a, b)| !a.is_empty() && !b.is_empty())
                {
                    return Ok(vec![mark(
                        node(
                            "sSub",
                            vec![
                                mark_at(node("e", vec![mark_at(run(&alias(base)),e.from,e.from+base.len())]),e.from,e.from+base.len()),
                                mark_at(node("sub", vec![mark_at(run(&alias(sub)),e.from+base.len()+1,e.to)]),e.from+base.len()+1,e.to),
                            ],
                        ),
                        e,
                    )]);
                }
            }
            mark(run(&if *quoted { s.clone() } else { alias(s) }), e)
        }
        Kind::Unary(sign, value) => {
            let mut out = vec![mark_at(run(&sign.text),sign.from,sign.to)];
            out.extend(compile_expr(value)?);
            return Ok(out);
        }
        Kind::Binary(sign, left, right) => match sign.text.as_str() {
            "/" => mark(node("f", vec![slot("num", left)?, slot("den", right)?]), e),
            "^" => mark(node("sSup", vec![slot("e", left)?, slot("sup", right)?]), e),
            _ => {
                let mut out = compile_expr(left)?;
                out.push(mark_at(run(match sign.text.as_str() {
                    "*" => "·",
                    "<=" => "≤",
                    ">=" => "≥",
                    "!=" => "≠",
                    s => s,
                }),sign.from,sign.to));
                out.extend(compile_expr(right)?);
                return Ok(out);
            }
        },
        Kind::Group(value) => mark(node("d",vec![slot("e",value)?]),e),
        Kind::List(_) => return Err("math_invalid_source"),
        Kind::Call(name, args, named) if name == "empty" || name == "row" => {
            if !named.is_empty() || (name == "empty" && !args.is_empty()) { return Err("math_invalid_property"); }
            return args.iter().try_fold(Vec::new(), |mut out, arg| { out.extend(compile_expr(arg)?); Ok(out) });
        }
        Kind::Call(name,args,named) if name=="space"=>{
            if !args.is_empty(){return Err("math_invalid_property");}
            let mut attributes=Vec::new();
            for(key,value)in named{
                if !matches!(key.as_str(),"width"|"height"|"depth"|"size"){return Err("math_invalid_property");}
                let Kind::Atom(value,true)=&value.kind else{return Err("math_invalid_property");};
                attributes.push((if key=="size"{"mathsize"}else{key.as_str()},value.as_str()));
            }
            mark(crate::math_spacing::Spacing::from_attributes(attributes)?.node(),e)
        }
        Kind::Call(name, args, named)
            if name.starts_with("office.") || name.starts_with("word.") || name.starts_with("drawing.") =>
        {
            let (prefix, local) = name.split_once('.').unwrap();
            let definition = if prefix == "office" {
                Some(definition(local).ok_or("math_invalid_property")?)
            } else {
                None
            };
            if !local.chars().all(|c| c.is_ascii_alphanumeric()) || local.is_empty() {
                return Err("math_invalid_property");
            }
            let ns = match prefix {"office"=>MATH_NS,"word"=>WORD_NS,_=>DRAWING_NS};
            let prefix = match prefix {"office"=>"m","word"=>"w",_=>"a"};
            let mut n = node(local, vec![]);
            n.name = format!("{prefix}:{local}");
            n.namespace = ns.into();
            if ns==DRAWING_NS{n.attributes.insert("xmlns:a".into(),DRAWING_NS.into());}
            for arg in args {
                if matches!(local, "t" | "delText") {
                    if let Kind::Atom(s, true) = &arg.kind {
                        n.content.push(XmlPart::Text(s.clone()));
                    } else {
                        return Err("math_invalid_source");
                    }
                } else {
                    n.content
                        .extend(compile_expr(arg)?.into_iter().map(XmlPart::Element));
                }
            }
            for (key, value) in named {
                let key = if key.contains(':') || ns==DRAWING_NS {
                    key.clone()
                } else {
                    format!("{prefix}:{key}")
                };
                let value = scalar(value)?;
                if let Some(definition) = definition {
                    let attr = definition["attributes"]
                        .as_array()
                        .and_then(|attrs| attrs.iter().find(|a| a["name"] == key))
                        .ok_or("math_invalid_property")?;
                    math_document::validate_constraints(attr, &value)?;
                    if key == "m:val" {
                        math_document::validate_value(local, &value)?;
                    }
                }
                n.attributes.insert(key, value);
            }
            if let Some(definition) = definition {
                let allowed = definition["children"].as_array();
                for child in n.children().filter(|n| n.namespace == MATH_NS) {
                    if !allowed.is_some_and(|a| a.iter().any(|name| name == child.local())) {
                        return Err("math_invalid_property");
                    }
                }
                let required: &[&str] = match local {
                    "f" => &["num", "den"],
                    "rad" => &["deg", "e"],
                    "acc" | "bar" | "box" | "borderBox" | "groupChr" | "phant" => &["e"],
                    "func" => &["fName", "e"],
                    "limLow" | "limUpp" => &["e", "lim"],
                    "sSub" => &["e", "sub"],
                    "sSup" => &["e", "sup"],
                    "sPre" | "sSubSup" => &["e", "sub", "sup"],
                    "nary" => &["e", "sub", "sup"],
                    "eqArr" | "d" => &["e"],
                    "m" => &["mr"],
                    _ => &[],
                };
                if required.iter().any(|role| {
                    !n.children()
                        .any(|child| child.namespace == MATH_NS && child.local() == *role)
                }) {
                    return Err("math_missing_argument");
                }
            }
            mark(n, e)
        }
        Kind::Call(name, args, named) => compile_call(e, name, args, named)?,
    };
    Ok(vec![n])
}

fn compile_call(
    e: &Expr,
    name: &str,
    args: &[Expr],
    named: &BTreeMap<String, Expr>,
) -> Result<XmlNode> {
    if name == "matrix" {
        if args.is_empty() || args.len() > 256 {
            return Err("math_irregular_matrix");
        }
        let mut rows = Vec::new();
        let mut columns = None;
        for row in args {
            let Kind::List(cells) = &row.kind else {
                return Err("math_irregular_matrix");
            };
            if cells.is_empty() || cells.len() > 64 || columns.is_some_and(|n| n != cells.len()) {
                return Err("math_irregular_matrix");
            }
            columns = Some(cells.len());
            rows.push(node(
                "mr",
                cells
                    .iter()
                    .map(|cell| slot("e", cell))
                    .collect::<Result<_>>()?,
            ));
        }
        let mut properties = Vec::new();
        if let Some(align) = named.get("align") {
            let Kind::List(values) = &align.kind else {
                return Err("math_invalid_property");
            };
            if values.len() != columns.unwrap() {
                return Err("math_irregular_matrix");
            }
            properties.push(node(
                "mcs",
                values
                    .iter()
                    .map(|v| {
                        Ok(node(
                            "mc",
                            vec![node(
                                "mcPr",
                                vec![property("count", "1")?, property("mcJc", &scalar(v)?)?],
                            )],
                        ))
                    })
                    .collect::<Result<_>>()?,
            ));
        }
        if let Some(gap) = named.get("column_gap") {
            let gap = scalar(gap)?;
            let (amount, unit) = if let Some(v) = gap.strip_suffix("em") {
                (v, "em")
            } else if let Some(v) = gap.strip_suffix("pt") {
                (v, "pt")
            } else {
                return Err("math_invalid_property");
            };
            let amount = amount.parse::<f64>().map_err(|_| "math_invalid_property")?
                * if unit == "em" { 2.0 } else { 20.0 };
            if !amount.is_finite() || amount < 0.0 || amount > 65535.0 || amount.fract() != 0.0 {
                return Err("math_invalid_property");
            }
            properties.push(property("cGp", &(amount as u16).to_string())?);
            properties.push(property("cGpRule", if unit == "em" { "4" } else { "3" })?);
        }
        for key in named.keys() {
            if !matches!(key.as_str(), "align" | "column_gap" | "properties") {
                return Err("math_invalid_property");
            }
        }
        if let Some(extra) = named.get("properties") {
            for p in compile_expr(extra)? {
                if p.local() != "mPr" {
                    return Err("math_invalid_property");
                }
                properties.extend(p.content.into_iter().filter_map(|p| {
                    if let XmlPart::Element(n) = p {
                        Some(n)
                    } else {
                        None
                    }
                }));
            }
        }
        if !properties.is_empty() || named.contains_key("properties") {
            if let Some(order) = definition("mPr").and_then(|e| e["children"].as_array()) {
                properties.sort_by_key(|p| {
                    order
                        .iter()
                        .position(|n| n == p.local())
                        .unwrap_or(usize::MAX)
                });
            }
            rows.insert(0, node("mPr", properties));
        }
        return Ok(mark(node("m", rows), e));
    }
    let (object, roles): (&str, &[&str]) = match name {
        "frac" => ("f", &["num", "den"]),
        "sqrt" => ("rad", &["e"]),
        "root" => ("rad", &["deg", "e"]),
        "accent" => ("acc", &["e"]),
        "bar" => ("bar", &["e"]),
        "box" => ("box", &["e"]),
        "border_box" => ("borderBox", &["e"]),
        "delimiter" => ("d", &[]),
        "equations" => ("eqArr", &[]),
        "function" => ("func", &["fName", "e"]),
        "group_char" => ("groupChr", &["e"]),
        "lower_limit" => ("limLow", &["e", "lim"]),
        "upper_limit" => ("limUpp", &["e", "lim"]),
        "sum" | "product" | "integral" | "nary" => ("nary", &["e"]),
        "phantom" => ("phant", &["e"]),
        "prescript" => ("sPre", &["sub", "sup", "e"]),
        "subscript" => ("sSub", &["e", "sub"]),
        "subsup" => ("sSubSup", &["e", "sub", "sup"]),
        "superscript" => ("sSup", &["e", "sup"]),
        _ => return Err("math_unknown_macro"),
    };
    if (!roles.is_empty() && args.len() != roles.len()) || (roles.is_empty() && args.is_empty()) {
        return Err("math_missing_argument");
    }
    let mut content = Vec::new();
    let mut properties = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        content.push(slot(roles.get(i).unwrap_or(&"e"), arg)?);
    }
    if name == "sqrt" {
        properties.push(property("degHide", "1")?);
        content.insert(0, node("deg", vec![]));
    }
    if matches!(name, "sum" | "product" | "integral") {
        properties.push(property(
            "chr",
            match name {
                "sum" => "∑",
                "product" => "∏",
                _ => "∫",
            },
        )?);
    }
    for (key, value) in named {
        if key == "properties" {
            let extras = compile_expr(value)?;
            for extra in extras {
                if extra.local() != format!("{object}Pr") {
                    return Err("math_invalid_property");
                }
                properties.extend(extra.content.into_iter().filter_map(|p| {
                    if let XmlPart::Element(n) = p {
                        Some(n)
                    } else {
                        None
                    }
                }));
            }
            continue;
        }
        if object == "nary" && matches!(key.as_str(), "from" | "to") {
            content.push(slot(if key == "from" { "sub" } else { "sup" }, value)?);
            continue;
        }
        let property_name = match (name, key.as_str()) {
            ("frac", "style") => "type",
            (_, "character") => "chr",
            (_, "position") => "pos",
            (_, "limits") if object == "nary" => "limLoc",
            ("delimiter", "open") => "begChr",
            ("delimiter", "close") => "endChr",
            ("delimiter", "separator") => "sepChr",
            (_, key) => key,
        };
        let mut value = scalar(value)?;
        value = match (property_name, value.as_str()) {
            ("type", "stacked") => "bar",
            ("type", "skewed") => "skw",
            ("type", "linear") => "lin",
            ("type", "no_bar") => "noBar",
            ("limLoc", "above_below") => "undOvr",
            ("limLoc", "sub_sup") => "subSup",
            ("pos", "above") => "top",
            ("pos", "below") => "bot",
            _ => &value,
        }
        .into();
        if !definition(&format!("{object}Pr"))
            .and_then(|p| p["children"].as_array())
            .is_some_and(|a| a.iter().any(|v| v == property_name))
        {
            return Err("math_invalid_property");
        }
        properties.push(property(property_name, &value)?);
    }
    if object == "nary" {
        for role in ["sub", "sup"] {
            if !content.iter().any(|n| n.local() == role) {
                content.push(node(role, vec![]));
            }
        }
    }
    if let Some(order) = definition(object).and_then(|e| e["children"].as_array()) {
        content.sort_by_key(|p| {
            order
                .iter()
                .position(|n| n == p.local())
                .unwrap_or(usize::MAX)
        });
    }
    if !properties.is_empty() || named.contains_key("properties") {
        if let Some(order) =
            definition(&format!("{object}Pr")).and_then(|e| e["children"].as_array())
        {
            properties.sort_by_key(|p| {
                order
                    .iter()
                    .position(|n| n == p.local())
                    .unwrap_or(usize::MAX)
            });
        }
        content.insert(0, node(&format!("{object}Pr"), properties));
    }
    Ok(mark(node(object, content), e))
}

/// Parsing keeps the original text, comments and exact UTF-16 locations. XML IDs
/// identify rendered symbols and argument slots without matching their text.
pub fn compile(source: &str, display: bool) -> Result<(XmlNode, Vec<SourceNode>)> {
    let mut parser = Parser {
        tokens: lex(source)?,
        at: 0,
    };
    let expr = parser.expression(0, 0)?;
    if parser.at != parser.tokens.len() {
        return Err("math_invalid_source");
    }
    let parts = compile_expr(&expr)?;
    let fragment = parts.len() > 1 && parts.iter().all(|n|
        n.namespace == MATH_NS && matches!(n.local(), "mathPr" | "oMath" | "oMathPara")
        || n.namespace == WORD_NS && matches!(n.local(), "styles" | "pPr" | "clrSchemeMapping" | "themeFontLang")
        || n.namespace == DRAWING_NS && matches!(n.local(),"clrScheme"|"fontScheme"));
    if fragment && parts.iter().filter(|n| n.local() == "mathPr").count() > 1 { return Err("math_invalid_property"); }
    if fragment && (!parts.iter().any(|n|n.namespace==MATH_NS && matches!(n.local(),"oMath"|"oMathPara"))
        && parts.iter().any(|n|n.namespace==WORD_NS||n.namespace==DRAWING_NS)) { return Err("math_invalid_property"); }
    let mut root =
        if parts.len() == 1 && matches!(parts[0].local(), "oMath" | "oMathPara" | "mathPr") {
            parts.into_iter().next().unwrap()
        } else if fragment {
            let mut root = node("#root", parts);
            root.name = "#root".into(); root.namespace.clear(); root
        } else {
            node("oMath", parts)
        };
    root.attributes.insert("xmlns:m".into(), MATH_NS.into());
    root.attributes.insert("xmlns:w".into(), WORD_NS.into());
    if display && root.local() == "oMath" {
        root = node("oMathPara", vec![root]);
        root.attributes.insert("xmlns:m".into(), MATH_NS.into());
    }
    if root.local() == "#root" {
        for part in &mut root.content {
            if let XmlPart::Element(child) = part {
                child.attributes.insert("xmlns:m".into(), MATH_NS.into());
                child.attributes.insert("xmlns:w".into(), WORD_NS.into());
            }
        }
    }
    let mut root = math_document::parse_xml(&root.xml())?;
    { let mut view = root.clone(); crate::math_word::materialize(&mut view)?; }
    let mut nodes = Vec::new();
    fn collect(n: &mut XmlNode, source: &str, out: &mut Vec<SourceNode>) {
        if let (Some(from), Some(to)) =
            (n.attributes.remove("dslFrom"), n.attributes.remove("dslTo"))
        {
            let from = from.parse::<usize>().unwrap();
            let to = to.parse::<usize>().unwrap();
            let slots = n
                .children()
                .filter_map(|c| {
                    Some(SourceSlot {
                        role: c.local().into(),
                        from: source[..c.attributes.get("dslFrom")?.parse::<usize>().ok()?]
                            .encode_utf16()
                            .count(),
                        to: source[..c.attributes.get("dslTo")?.parse::<usize>().ok()?]
                            .encode_utf16()
                            .count(),
                    })
                })
                .collect();
            out.push(SourceNode {
                id: n.id.clone(),
                kind: n.local().into(),
                from: source[..from].encode_utf16().count(),
                to: source[..to].encode_utf16().count(),
                tex: source[from..to].into(),
                slots,
            });
        }
        for p in &mut n.content {
            if let XmlPart::Element(c) = p {
                collect(c, source, out);
            }
        }
    }
    collect(&mut root, source, &mut nodes);
    validate_tree(&root)?;
    Ok((root, nodes))
}

/// A generated operator edits one operator token, not an arbitrary statement.
/// Accepting a comment here could make a valid parse silently consume the
/// operands after it. Operand/structural slots still accept full expressions.
pub(crate) fn validate_source_replacement(node:&SourceNode,text:&str)->Result<()> {
    let operator=|s:&str|matches!(s,"+"|"-"|"−"|"*"|"×"|"·"|"÷"|"/"|"^"|"="|"<"|">"|"<="|">="|"!="|"≤"|"≥"|"≠");
    if node.kind=="r"&&operator(&node.tex)&&!operator(text.trim()){return Err("math_invalid_source");}
    Ok(())
}

/// A common property change rewrites only its value (or adds one argument).
/// Comments, surrounding formatting and identical sibling formulas survive.
pub fn rewrite_property(
    source: &str,
    target: &SourceNode,
    container: &str,
    property_name: &str,
    value: &str,
) -> Result<Option<String>> {
    if container != format!("{}Pr", target.kind) {
        return Ok(None);
    }
    if !definition(container)
        .and_then(|p| p["children"].as_array())
        .is_some_and(|a| a.iter().any(|v| v == property_name))
    {
        return Err("math_invalid_property");
    }
    math_document::validate_value(property_name, value)?;
    let from = crate::document_transaction::byte_offset(source, target.from)?;
    let to = crate::document_transaction::byte_offset(source, target.to)?;
    let fragment = &source[from..to];
    let mut parser = Parser {
        tokens: lex(fragment)?,
        at: 0,
    };
    let expr = parser.expression(0, 0)?;
    let Kind::Call(name, _, named) = expr.kind else {
        return Ok(None);
    };
    if !fragment.starts_with(&format!("{name}(")) {
        return Ok(None);
    }
    // Matrix's concise syntax accepts align/column_gap, not raw mPr names.
    // Use the exact subtree compiler rather than append an invalid argument.
    if name == "matrix" || name.starts_with("office.") || name.starts_with("word.") || named.contains_key("properties")
    {
        return Ok(None);
    }
    let key = match (target.kind.as_str(), property_name) {
        ("f", "type") => "style",
        ("nary", "limLoc") => "limits",
        (_, "chr") => "character",
        (_, "pos") => "position",
        ("d", "begChr") => "open",
        ("d", "endChr") => "close",
        ("d", "sepChr") => "separator",
        _ => property_name,
    };
    let value = match (property_name, value) {
        ("type", "skw") => "skewed",
        ("type", "bar") => "stacked",
        ("type", "lin") => "linear",
        ("type", "noBar") => "no_bar",
        ("limLoc", "undOvr") => "above_below",
        ("limLoc", "subSup") => "sub_sup",
        ("pos", "top") => "above",
        ("pos", "bot") => "below",
        (_, v) => v,
    };
    let value = if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !value.is_empty()
    {
        value.into()
    } else {
        quote(value)
    };
    let mut out = source.to_string();
    if let Some(previous) = named.get(key).or_else(|| named.get(property_name)) {
        out.replace_range(from + previous.from..from + previous.to, &value);
    } else {
        out.insert_str(to - 1, &format!(", {key}: {value}"));
    }
    Ok(Some(out))
}

/// Human slot input is separate from the lossless advanced source. Word run
/// formatting stays in the model and is retained by visual content patches.
pub(crate) fn visual_projections(root:&XmlNode)->serde_json::Value {
    fn input(n:&XmlNode)->Result<String>{
        let mut copy=n.clone();
        fn clean(n:&mut XmlNode){
            n.attributes.retain(|key,_|key!="xmlns"&&!key.starts_with("xmlns:"));
            n.attribute_namespaces.retain(|key,_|n.attributes.contains_key(key));
            n.content.retain(|p|!matches!(p,XmlPart::Element(c) if c.namespace==WORD_NS&&c.local()=="rPr"));
            for p in &mut n.content{if let XmlPart::Element(c)=p{clean(c);}}
        }
        clean(&mut copy);
        if n.local()!="r"{copy.name="#root".into();copy.namespace.clear();copy.attributes.clear();copy.attribute_namespaces.clear();copy.content.retain(|p|!matches!(p,XmlPart::Element(c) if c.namespace==MATH_NS&&c.local()=="argPr"));}
        let source=decompile(&copy)?;
        if let Ok(text)=serde_json::from_str::<String>(&source){
            if text.chars().count()==1||!text.is_empty()&&text.chars().all(|c|c.is_ascii_digit()){return Ok(text);}
        }
        Ok(source)
    }
    fn visit(n:&XmlNode,map:&mut serde_json::Map<String,serde_json::Value>,budget:&mut usize){
        if n.namespace==MATH_NS&&matches!(n.local(),"r"|"e"|"num"|"den"|"sub"|"sup"|"deg"|"lim"|"fName")&&*budget<2*1024*1024{
            fn has_word_format(n:&XmlNode)->bool{(n.namespace==WORD_NS&&n.local()=="rPr")||n.children().any(has_word_format)}
            // Plain source keeps its author's exact spelling and comments.
            // Only hidden Word formatting needs a separate visual input.
            if has_word_format(n){if let Ok(value)=input(n){if value.len()<=2*1024*1024-*budget{*budget+=value.len();map.insert(n.id.clone(),serde_json::Value::String(value));}}}
        }
        for c in n.children(){visit(c,map,budget);}
    }
    let mut map=serde_json::Map::new();visit(root,&mut map,&mut 0);serde_json::Value::Object(map)
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("string")
}
/// Canonical readable Office form is the escape hatch for advanced standard
/// properties. Namespace aliases are normalized by URI, never by XML prefix.
pub fn decompile(root: &XmlNode) -> Result<String> {
    fn expression(slot: &XmlNode) -> Result<Option<String>> {
        if !slot.attributes.is_empty() || slot.content.iter().any(|p| !matches!(p, XmlPart::Element(_))) { return Ok(None); }
        let children = slot.children().map(emit).collect::<Result<Vec<_>>>()?;
        Ok(Some(match children.len() { 0 => "empty()".into(), 1 => children.into_iter().next().unwrap(), _ => format!("row({})", children.join(", ")) }))
    }
    fn emit(n: &XmlNode) -> Result<String> {
        if crate::math_spacing::is_spacing(n){return Ok(crate::math_spacing::Spacing::from_xml(n)?.dsl());}
        if n.local() == "#root" {
            let parts = n.children().map(emit).collect::<Result<Vec<_>>>()?;
            return Ok(if parts.len() <= 1 { parts.join("") } else { format!("row({})", parts.join(", ")) });
        }
        let prefix = match n.namespace.as_str() {
            MATH_NS | "" => "office",
            WORD_NS => "word",
            DRAWING_NS => "drawing",
            _ => return Err("math_unsupported_extension"),
        };
        if prefix == "office" && n.attributes.is_empty() {
            if matches!(n.local(), "m" | "eqArr" | "d" | "nary") && n.content.iter().all(|p| matches!(p,XmlPart::Element(_))) {
                let properties_name = format!("{}Pr", n.local());
                let properties = n.children().filter(|c| c.local()==properties_name).collect::<Vec<_>>();
                let mut args = Vec::new(); let mut exact = properties.len() <= 1;
                for child in n.children().filter(|c| c.local()!=properties_name) {
                    if child.namespace != MATH_NS { exact=false; break; }
                    if n.local()=="m" && child.local()=="mr" && child.attributes.is_empty() {
                        let mut cells=vec![];
                        for cell in child.children() { if cell.local()!="e" { exact=false; break; } if let Some(value)=expression(cell)? { cells.push(value); } else { exact=false; break; } }
                        if cells.is_empty() || child.content.len()!=cells.len() { exact=false; }
                        args.push(format!("[{}]",cells.join(", ")));
                    } else if (matches!(n.local(),"d"|"eqArr") && child.local()=="e") || (n.local()=="nary" && matches!(child.local(),"e"|"sub"|"sup")) {
                        if let Some(value)=expression(child)? { args.push(match child.local() { "sub"=>format!("from: {value}"), "sup"=>format!("to: {value}"), _=>value }); } else { exact=false; break; }
                    } else { exact=false; break; }
                }
                if exact && !args.is_empty() {
                    // nary's semantic argument is first; bounds are named.
                    if n.local()=="nary" { args.sort_by_key(|v| v.starts_with("from: ") || v.starts_with("to: ")); }
                    if let Some(properties)=properties.first() { args.push(format!("properties: {}",emit(properties)?)); }
                    let name=match n.local(){"m"=>"matrix","eqArr"=>"equations","d"=>"delimiter",_=>"nary"};
                    return Ok(format!("{name}({})",args.join(", ")));
                }
            }
            if n.local() == "r" && n.content.len() == 1 {
                if let Some(t) = n.children().next().filter(|t| {
                    t.local() == "t"
                        && t.namespace == MATH_NS
                        && t.attributes.is_empty()
                        && t.content.iter().all(|p| matches!(p, XmlPart::Text(_)))
                }) {
                    return Ok(quote(&t.text()));
                }
            }
            let signature: Option<(&str, &[&str])> = match n.local() {
                "f" => Some(("frac", &["num", "den"])),
                "rad" => Some(("root", &["deg", "e"])),
                "acc" => Some(("accent", &["e"])),
                "bar" => Some(("bar", &["e"])),
                "box" => Some(("box", &["e"])),
                "borderBox" => Some(("border_box", &["e"])),
                "func" => Some(("function", &["fName", "e"])),
                "groupChr" => Some(("group_char", &["e"])),
                "limLow" => Some(("lower_limit", &["e", "lim"])),
                "limUpp" => Some(("upper_limit", &["e", "lim"])),
                "phant" => Some(("phantom", &["e"])),
                "sPre" => Some(("prescript", &["sub", "sup", "e"])),
                "sSub" => Some(("subscript", &["e", "sub"])),
                "sSubSup" => Some(("subsup", &["e", "sub", "sup"])),
                "sSup" => Some(("superscript", &["e", "sup"])),
                _ => None,
            };
            if let Some((name, roles)) = signature {
                let properties_name = format!("{}Pr", n.local());
                let children: Vec<_> = n.children().collect();
                if n.content.len() == children.len()
                    && children.iter().all(|c| {
                        c.namespace == MATH_NS
                            && (c.local() == properties_name || roles.contains(&c.local()))
                    })
                    && roles
                        .iter()
                        .all(|role| children.iter().filter(|c| c.local() == *role).count() == 1)
                {
                    let mut args = Vec::new();
                    let mut exact = true;
                    for role in roles {
                        let value = children.iter().find(|c| c.local() == *role).unwrap();
                        if let Some(value)=expression(value)? { args.push(value); } else { exact=false; break; }
                    }
                    let properties: Vec<_> = children
                        .iter()
                        .filter(|c| c.local() == properties_name)
                        .collect();
                    if exact && properties.len() <= 1 {
                        if let Some(p) = properties.first() {
                            args.push(format!("properties: {}", emit(p)?));
                        }
                        return Ok(format!("{name}({})", args.join(", ")));
                    }
                }
            }
        }
        let mut args = Vec::new();
        for p in &n.content {
            match p {
                XmlPart::Element(c) => args.push(emit(c)?),
                XmlPart::Text(s) => args.push(quote(s)),
                _ => return Err("math_unsupported_extension"),
            }
        }
        for (key, value) in &n.attributes {
            if key == "xmlns" || key.starts_with("xmlns:") {
                continue;
            }
            let (attr_prefix, local) = key.split_once(':').unwrap_or(("", key));
            let namespace = n
                .attribute_namespaces
                .get(key)
                .map(String::as_str)
                .unwrap_or("");
            let key = if namespace == MATH_NS && prefix == "office"
                || namespace == WORD_NS && prefix == "word"
            {
                local.into()
            } else if attr_prefix == "xml" {
                "xml:".to_string() + local
            } else if attr_prefix.is_empty() {
                key.clone()
            } else {
                return Err("math_unsupported_extension");
            };
            args.push(format!(
                "{}: {}",
                if key.contains(':') { quote(&key) } else { key },
                quote(value)
            ));
        }
        Ok(format!("{prefix}.{}({})", n.local(), args.join(", ")))
    }
    emit(root)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownRange {
    pub from: usize,
    pub to: usize,
    pub body_from: usize,
    pub body_to: usize,
    pub display: bool,
}

/// Unsupported syntax is preserved for reading and saving, but cannot be
/// silently exported as a successfully rendered equation. The Markdown parser
/// distinguishes actual math fences from examples, indentation and raw HTML.
pub fn version_diagnostics(source: &str) -> Vec<serde_json::Value> {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
    Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_FOOTNOTES)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event else { return None; };
            if info.split_whitespace().next() != Some("readmd-math") || info.trim() == "readmd-math v1" { return None; }
            let to = range.start + source[range.clone()].find('\n').unwrap_or(range.len());
            Some(serde_json::json!({"code":"math_unsupported_version","key":info.as_ref(),"from":source[..range.start].encode_utf16().count(),"to":source[..to].encode_utf16().count()}))
        }).take(4000).collect()
}

/// Source-aware discovery excludes code examples, raw HTML and legacy LaTeX.
/// Byte offsets stay internal; editor endpoints use the compiler's UTF-16 map.
pub fn markdown_ranges(source: &str) -> Vec<MarkdownRange> {
    use pulldown_cmark::{CodeBlockKind,Event,Options,Parser,Tag};
    // Markdown owns container and indentation rules. The lightweight inline
    // scanner must not compile examples inside a nested quote/list code block,
    // nor a URL, frontmatter or raw HTML that happens to contain :math[...].
    let mut protected=Vec::<std::ops::Range<usize>>::new();
    for (event,range) in Parser::new_ext(source,Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_FOOTNOTES).into_offset_iter(){
        let exclude=match event{
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim()=="readmd-math v1"=>{
                let prefix=source[..range.start].rsplit('\n').next().unwrap_or("");
                prefix.len()>3||!prefix.bytes().all(|c|c==b' ')
            },
            Event::Start(Tag::CodeBlock(_)|Tag::MetadataBlock(_)|Tag::HtmlBlock|Tag::Link{..}|Tag::Image{..})|Event::Code(_)|Event::Html(_)|Event::InlineHtml(_)=>true,
            _=>false,
        };
        if exclude{protected.push(range);}
    }
    protected.sort_by_key(|r|r.start);
    let mut merged=Vec::<std::ops::Range<usize>>::new();
    for range in protected{if let Some(last)=merged.last_mut().filter(|last|range.start<=last.end){last.end=last.end.max(range.end);}else{merged.push(range);}}
    let mut protected_at=0;
    let mut ranges = Vec::new();
    let mut i = 0;
    let mut fence: Option<(u8, usize, usize, usize, bool)> = None;
    // Frontmatter is document metadata, not content. Match the reader's rule
    // without reinterpreting a horizontal rule or an ordinary first paragraph.
    let first = source.strip_prefix('\u{feff}').unwrap_or(source);
    if first.starts_with("---\n") || first.starts_with("---\r\n") {
        let mut offset = source.len() - first.len() + first.find('\n').unwrap() + 1;
        let mut has_field = false;
        for line in source[offset..].split_inclusive('\n') {
            if line.trim_end_matches(['\r', '\n']) == "---" {
                if has_field {
                    i = offset + line.len();
                }
                break;
            }
            let key = line.split_once(':').map(|p| p.0.trim());
            has_field |= key.is_some_and(|key| {
                !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            });
            offset += line.len();
        }
    }
    while i < source.len() {
        while protected_at<merged.len()&&merged[protected_at].end<=i{protected_at+=1;}
        if let Some(range)=merged.get(protected_at).filter(|range|range.start<=i){i=range.end;continue;}
        if i == 0 || source.as_bytes()[i - 1] == b'\n' {
            let end = source[i..]
                .find('\n')
                .map(|n| i + n + 1)
                .unwrap_or(source.len());
            let line = source[i..end].trim_end_matches(['\r', '\n']);
            let stripped = line.trim_start_matches(' ');
            let indent = line.len() - stripped.len();
            let marker = stripped.as_bytes().first().copied().unwrap_or(0);
            let count = stripped.bytes().take_while(|b| *b == marker).count();
            if indent <= 3 && matches!(marker, b'`' | b'~') && count >= 3 {
                if let Some((old, length, from, body_from, math)) = fence {
                    if marker == old && count >= length && stripped[count..].trim().is_empty() {
                        if math {
                            let body_to = source[..i].trim_end_matches(['\r', '\n']).len();
                            ranges.push(MarkdownRange {
                                from,
                                to: end,
                                body_from,
                                body_to,
                                display: true,
                            });
                        }
                        fence = None;
                    }
                } else {
                    fence = Some((
                        marker,
                        count,
                        i,
                        end,
                        stripped[count..].trim() == "readmd-math v1",
                    ));
                }
                i = end;
                continue;
            }
            if fence.is_some() {
                i = end;
                continue;
            }
        }
        if fence.is_some() {
            i += 1;
            continue;
        }
        if source[i..].starts_with("<!--") {
            i = source[i + 4..]
                .find("-->")
                .map(|n| i + 4 + n + 3)
                .unwrap_or(source.len());
            continue;
        }
        if source.as_bytes()[i] == b'`' {
            let count = source[i..].bytes().take_while(|b| *b == b'`').count();
            let marker = &source[i..i + count];
            i = source[i + count..]
                .find(marker)
                .map(|n| i + count + n + count)
                .unwrap_or(i + count);
            continue;
        }
        if source.as_bytes()[i] == b'<' {
            let rest = &source[i..];
            let mut skipped = false;
            for name in [
                "script",
                "style",
                "textarea",
                "pre",
                "code",
                "annotation",
                "annotation-xml",
            ] {
                let open = format!("<{name}");
                if rest
                    .get(..open.len())
                    .is_some_and(|s| s.eq_ignore_ascii_case(&open))
                {
                    let close = format!("</{name}>");
                    i += rest
                        .to_ascii_lowercase()
                        .find(&close)
                        .map(|n| n + close.len())
                        .unwrap_or(rest.len());
                    skipped = true;
                    break;
                }
            }
            if skipped {
                continue;
            }
            if let Some(end) = rest.find('>') {
                i += end + 1;
                continue;
            }
        }
        if source[i..].starts_with(":math[") {
            let from = i;
            let body_from = i + 6;
            let mut end = body_from;
            let mut depth = 1;
            let mut quote = false;
            let mut escaped = false;
            while end < source.len() {
                let c = source[end..].chars().next().unwrap();
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    quote = !quote;
                } else if !quote {
                    if c == '\n' {
                        break;
                    }
                    if c == '[' {
                        depth += 1;
                    } else if c == ']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                end += c.len_utf8();
            }
            if depth == 0 {
                ranges.push(MarkdownRange {
                    from,
                    to: end + 1,
                    body_from,
                    body_to: end,
                    display: false,
                });
                i = end + 1;
                continue;
            }
        }
        i += if source.as_bytes()[i] == b'\\' {
            source[i + 1..]
                .chars()
                .next()
                .map(|c| 1 + c.len_utf8())
                .unwrap_or(1)
        } else {
            source[i..].chars().next().unwrap().len_utf8()
        };
        if ranges.len() >= 4000 {
            break;
        }
    }
    if let Some((_, _, from, body_from, true)) = fence {
        ranges.push(MarkdownRange {
            from,
            to: source.len(),
            body_from,
            body_to: source.len(),
            display: true,
        });
    }
    ranges
}

/// Export-only lowering reuses the existing OfficeMath AST adapters. The
/// transient comment never replaces or gets written into the .md document.
pub fn expand_markdown(source: &str) -> Result<String> {
    if !version_diagnostics(source).is_empty() { return Err("math_unsupported_version"); }
    // The Markdown parser owns nested list/quote scopes. These fences are
    // consumed directly by the shared AST, but must pass the same preflight
    // before any exporter opens its target file.
    {
        use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
        let mut body = None::<String>;
        for event in Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_FOOTNOTES) {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim()=="readmd-math v1" => body=Some(String::new()),
                Event::Text(text) => if let Some(body)=&mut body { if body.len()+text.len()>MAX_SOURCE{return Err("math_budget");}body.push_str(&text); },
                Event::End(TagEnd::CodeBlock) => if let Some(body)=body.take() { compile(body.trim_end_matches(['\r','\n']),true)?; },
                _=>(),
            }
        }
    }
    let ranges = markdown_ranges(source);
    let mut out = source.to_string();
    let mut identities=BTreeSet::new();
    for r in ranges.into_iter().rev() {
        let body=&source[r.body_from..r.body_to];
        let (root, _) = compile(body, r.display)?;
        let (xml,to,id)=if let Some((_,to,comment))=crate::math_extension::adjacent(source,r.to){
            let restored=crate::math_extension::resolve(body,comment,None,r.display)?;
            if !identities.insert(restored.payload.id.clone()){return Err("math_metadata_stale");}
            (restored.xml,to,restored.payload.id)
        }else{(root.xml(),r.to,format!("dsl-{}",r.from))};
        let latex = crate::math_formats::tex(&root);
        let metadata = math_document::metadata(&latex, &xml, &id)?;
        let lowered = if r.display {
            format!("$$\n{latex}\n$$\n\n{metadata}\n")
        } else {
            format!("${latex}${metadata}")
        };
        out.replace_range(r.from..to, &lowered);
    }
    // A nested Markdown container without an exact carrier map must fail
    // preflight instead of exporting a projection which drops its extension.
    {
        use pulldown_cmark::{Event,Options,Parser};
        for event in Parser::new_ext(&out,Options::ENABLE_FOOTNOTES){
            if matches!(event,Event::Html(ref value)|Event::InlineHtml(ref value) if value.contains(crate::math_extension::PREFIX)){return Err("math_unsupported_extension");}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn math_visual_input_hides_only_word_run_format_and_preserves_literal_names() {
        use crate::math_document;
        let source="row(frac(office.r(word.rPr(word.color(val: \"336699\")), office.t(\"a\")), office.r(word.rPr(word.color(val: \"336699\")), office.t(\"alpha\"))), frac( x + 1, y ))";
        let doc=math_document::parse(source,"dsl",false).unwrap();let original=doc.root.xml();
        let inputs=super::visual_projections(&doc.root);
        let nums=doc.nodes.iter().filter(|n|n.kind=="num").collect::<Vec<_>>();
        assert_eq!(inputs[&nums[0].id],"a");assert!(inputs.get(&nums[1].id).is_none());
        let den=doc.nodes.iter().find(|n|n.kind=="den").unwrap();assert_eq!(inputs[&den.id],"\"alpha\"");
        assert_eq!(doc.root.xml(),original);assert!(!inputs.to_string().contains("336699"));
        assert!(super::visual_projections(&math_document::parse("frac( x + 1, y )","dsl",false).unwrap().root).as_object().unwrap().is_empty());
    }
    use super::*;
    #[test]
    fn nested_examples_and_links_never_compile_or_mutate_their_source() {
        let examples="😀\n\n> ```markdown\n> :math[frac(]\n> ```\n\n- ```text\n  :math[matrix(]\n  ```\n\nEnd of list.\n\n    :math[root(]\n\n`example :math[frac(]`\n\n[example](<https://example.test/:math[frac(]>)\n\n<div>\n:math[frac(]\n</div>\n\n";
        assert!(markdown_ranges(examples).is_empty(),"{:?}",markdown_ranges(examples));assert_eq!(expand_markdown(examples).unwrap(),examples);
        let actual=format!("{examples}:math[a / b]\n\n> ```readmd-math v1\n> frac(a, b)\n> ```\n");
        assert_eq!(markdown_ranges(&actual).len(),1);
        let expanded=expand_markdown(&actual).unwrap();assert!(expanded.starts_with(examples));assert!(expanded.contains("readmd:math:v1:"));
        let ast=crate::md_ast::parse(&expanded);assert!(matches!(ast.blocks.last(),Some(crate::md_ast::Block::Quote(blocks)) if matches!(blocks.first(),Some(crate::md_ast::Block::OfficeMath{..}))));
    }
    #[test]
    fn unknown_versions_are_locatable_preserved_and_never_exported_as_code() {
        let source = "😀\n\n```readmd-math v2\nx + 1\n```\n\n```readmd-math\na / b\n```\n\n````markdown\n```readmd-math v2\nx\n```\n````\n\n    ```readmd-math v2\n\n```readmd-math v1\na / b\n```\n";
        let d = version_diagnostics(source);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0]["from"], 4);
        assert_eq!(d[0]["key"], "readmd-math v2");
        assert_eq!(expand_markdown(source).unwrap_err(), "math_unsupported_version");
        let doc = crate::publication::prepare(source, "").unwrap();
        assert_eq!(doc.content, source);
        assert_eq!(doc.data["diagnostics"].as_array().unwrap().len(), 2);
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("untouched.docx");
        std::fs::write(&output, b"existing file").unwrap();
        assert_eq!(crate::mdexport::export_document("docx", source, "", output.to_str().unwrap(), &serde_json::json!({}), "test", dir.path()).unwrap_err(), "math_unsupported_version");
        assert_eq!(std::fs::read(output).unwrap(), b"existing file");
        assert!(crate::mdexport::epub_build_bytes(&serde_json::json!(source), "", &serde_json::json!({}), "test", "", "en", "test", "2026-10-09T00:00:00Z").unwrap_err().contains("math_unsupported_version"));
    }
    fn xml(source: &str) -> String {
        compile(source, false).unwrap().0.xml()
    }

    #[test]
    fn matrix_minimum_and_placeholder_properties_round_trip_without_hidden_payload() {
        for properties in ["office.cSp(val: 0)","office.cSp(val: 720), office.plcHide()","office.cSp(val: 31680), office.plcHide(val: false)"] {
            let source=format!("matrix([x, empty()], properties: office.mPr({properties}))");
            let original=xml(&source);
            let root=crate::math_document::parse_xml(&original).unwrap();
            let readable=decompile(&root).unwrap();
            assert_eq!(xml(&readable),original,"{readable}");
            assert!(!readable.contains("readmd:math"));
        }
        assert!(compile("matrix([x], properties: office.mPr(office.cSp(val: 31681)))",false).is_err());
    }

    #[test]
    fn math_operator_tokens_keep_exact_utf16_ranges_comments_and_aliases() {
        let source="// 😀 + - * <= are commentary\r\n−α\t + (β * gamma <= δ) - x_i + χ ÷ ψ";
        let (root,nodes)=compile(source,true).unwrap();
        for token in lex(source).unwrap().iter().filter(|t|matches!(t.text.as_str(),"−"|"+"|"*"|"<="|"-"|"÷")){
            let from=source[..token.from].encode_utf16().count();let to=source[..token.to].encode_utf16().count();
            let mapped:Vec<_>=nodes.iter().filter(|n|n.kind=="r"&&n.from==from&&n.to==to).collect();
            assert_eq!(mapped.len(),1,"{} at {from}..{to}",token.text);assert_eq!(mapped[0].tex,token.text);
        }
        fn runs<'a>(n:&'a XmlNode,out:&mut Vec<&'a XmlNode>){if n.local()=="r"{out.push(n);}for c in n.children(){runs(c,out);}}
        let mut leaves=Vec::new();runs(&root,&mut leaves);
        assert!(leaves.iter().all(|n|nodes.iter().any(|s|s.id==n.id&&s.kind=="r")));
        let encoded=root.xml();assert!(!encoded.contains("dslFrom")&&!encoded.contains("dslTo"));assert!(encoded.contains('·')&&encoded.contains('≤'));
    }

    #[test]
    fn math_operator_edit_changes_only_its_source_and_rejects_stale_or_invalid_edits() {
        use math_document::{parse,patch,MathPatch};
        let source="// 😀 keep + and <=\r\n-α + β\t+ γ <= δ";let doc=parse(source,"dsl",true).unwrap();
        let target=doc.nodes.iter().filter(|n|n.kind=="r"&&n.tex=="+").max_by_key(|n|n.from).unwrap();
        let edit=MathPatch::ReplaceSource{node_id:target.id.clone(),text:"−".into()};
        let changed=patch(source,"dsl",true,&doc.revision,&edit).unwrap();
        assert_eq!(changed.source,"// 😀 keep + and <=\r\n-α + β\t− γ <= δ");
        assert_eq!(patch(&changed.source,"dsl",true,&doc.revision,&edit).unwrap_err(),"transaction_conflict");
        for bad in ["", "* *", "// swallowed operand"]{
            assert!(patch(source,"dsl",true,&doc.revision,&MathPatch::ReplaceSource{node_id:target.id.clone(),text:bad.into()}).is_err());
        }
        assert_eq!(doc.source,source);
        let relation=doc.nodes.iter().find(|n|n.kind=="r"&&n.tex=="<=").unwrap();
        let changed=patch(source,"dsl",true,&doc.revision,&MathPatch::ReplaceSource{node_id:relation.id.clone(),text:"≥".into()}).unwrap();
        assert_eq!(changed.source,"// 😀 keep + and <=\r\n-α + β\t+ γ ≥ δ");
    }

    #[test]
    fn math_parentheses_survive_precedence_projection_and_keep_inner_source_ranges() {
        let source="(a + b) * c";let (root,nodes)=compile(source,true).unwrap();
        let d=nodes.iter().find(|n|n.kind=="d").unwrap();assert_eq!(d.tex,"(a + b)");
        assert!(nodes.iter().any(|n|n.kind=="r"&&n.tex=="a"&&n.from==1&&n.to==2));
        let tex=crate::math_formats::tex(&root);assert!(tex.contains("\\left( a+b \\right)·c"),"{tex}");
        let expected=math_document::parse_xml("<m:oMath><m:d><m:e><m:r><m:t>a</m:t></m:r><m:r><m:t>+</m:t></m:r><m:r><m:t>b</m:t></m:r></m:e></m:d><m:r><m:t>·</m:t></m:r><m:r><m:t>c</m:t></m:r></m:oMath>").unwrap();
        let unwrapped=compile(source,false).unwrap().0;
        assert_eq!(math_document::xml_semantics(&unwrapped),math_document::xml_semantics(&expected));
        let nested=compile("((a + b) * c) / (d - e)",false).unwrap().0.xml();assert_eq!(nested.matches("<m:d>").count(),3);assert!(nested.contains("<m:f>"));
    }

    #[test]
    fn math_natural_subscript_base_and_slot_edit_preserve_aliases_and_unicode() {
        use math_document::{parse,patch,MathPatch};
        let source="// 😀\nalpha_i + β_κ";let doc=parse(source,"dsl",true).unwrap();
        for (tex,kind) in [("alpha","e"),("i","sub"),("β","e"),("κ","sub")]{assert!(doc.nodes.iter().any(|n|n.kind==kind&&n.tex==tex));}
        let target=doc.nodes.iter().find(|n|n.kind=="r"&&n.tex=="κ").unwrap();assert_eq!(target.to-target.from,1);
        let changed=patch(source,"dsl",true,&doc.revision,&MathPatch::ReplaceSource{node_id:target.id.clone(),text:"λ".into()}).unwrap();
        assert_eq!(changed.source,"// 😀\nalpha_i + β_λ");assert!(changed.root.xml().contains("<m:t>α</m:t>"));
        let target=doc.nodes.iter().find(|n|n.kind=="r"&&n.tex=="alpha").unwrap();
        let changed=patch(source,"dsl",true,&doc.revision,&MathPatch::ReplaceSource{node_id:target.id.clone(),text:"gamma".into()}).unwrap();
        assert_eq!(changed.source,"// 😀\ngamma_i + β_κ");assert!(changed.root.xml().contains("<m:t>γ</m:t>"));
    }

    #[test]
    fn math_operator_edit_can_change_structure_without_storing_source_annotations() {
        use math_document::{parse,patch,MathPatch};
        let doc=parse("a + b","dsl",true).unwrap();let op=doc.nodes.iter().find(|n|n.kind=="r"&&n.tex=="+").unwrap();
        let changed=patch(&doc.source,"dsl",true,&doc.revision,&MathPatch::ReplaceSource{node_id:op.id.clone(),text:"/".into()}).unwrap();
        assert_eq!(changed.source,"a / b");let xml=changed.root.xml();assert!(xml.contains("<m:f>")&&!xml.contains("dslFrom"));
        assert!(changed.nodes.iter().any(|n|n.kind=="num"&&n.tex=="a"));assert!(changed.nodes.iter().any(|n|n.kind=="den"&&n.tex=="b"));
    }

    #[test]
    fn math_unicode_minus_division_and_quoted_text_retain_distinct_meanings() {
        let (root,nodes)=compile("−α ÷ β + \"a + b\"",true).unwrap();
        assert!(root.xml().contains("<m:t>−</m:t>")&&root.xml().contains("<m:t>÷</m:t>"));
        assert_eq!(nodes.iter().filter(|n|n.kind=="r"&&n.tex=="+").count(),1);
        assert!(nodes.iter().any(|n|n.kind=="r"&&n.tex=="\"a + b\""));
        let scalar=Parser{tokens:lex("(−1)").unwrap(),at:0}.expression(0,0).unwrap();assert_eq!(super::scalar(&scalar).unwrap(),"-1");
    }

    #[test]
    fn math_document_defaults_and_formula_fragment_roundtrip_with_source_maps() {
        let original = format!(r#"<q:mathPr xmlns:q="{MATH_NS}"><q:intLim q:val="undOvr"/><q:naryLim q:val="subSup"/></q:mathPr><q:oMath xmlns:q="{MATH_NS}"><q:nary><q:naryPr><q:chr q:val="∑"/></q:naryPr><q:sub><q:r><q:t>0</q:t></q:r></q:sub><q:sup><q:r><q:t>1</q:t></q:r></q:sup><q:e><q:r><q:t>α</q:t></q:r></q:e></q:nary></q:oMath>"#);
        let readable = crate::math_migration::readable(&original).unwrap();
        assert!(readable.starts_with("row(office.mathPr("), "{readable}");
        for display in [false, true] {
            let (compiled, nodes) = compile(&readable, display).unwrap();
            assert_eq!(math_document::xml_semantics(&compiled), math_document::xml_semantics(&math_document::parse_xml(&original).unwrap()));
            let alpha = nodes.iter().find(|n| n.tex == "\"α\"").unwrap();
            assert_eq!(&readable[crate::document_transaction::byte_offset(&readable, alpha.from).unwrap()..crate::document_transaction::byte_offset(&readable, alpha.to).unwrap()], "\"α\"");
            assert_eq!(decompile(&compiled).unwrap(), readable);
        }
        assert!(compile("row(office.mathPr(), office.mathPr(), office.oMath(x))", false).is_err());
        assert!(compile("office.oMath(office.mathPr(), x)", false).is_err());
        assert!(compile("row(office.mathPr(), x)", false).is_err());
    }
    #[test]
    fn readable_structures_preserve_empty_slots_and_explicit_empty_properties() {
        for source in [
            "matrix([a, empty()], [c, d], properties: office.mPr())",
            "equations(a = b, c = d, properties: office.eqArrPr())",
            "delimiter(row(a, b), empty(), properties: office.dPr())",
            "nary(row(a, b), from: empty(), to: n, properties: office.naryPr(office.chr(val: \"∑\")))",
            "frac(row(a, b), empty(), properties: office.fPr())",
        ] {
            let original=compile(source,false).unwrap().0;
            let canonical=decompile(&original).unwrap();
            let again=compile(&canonical,false).unwrap().0;
            assert_eq!(decompile(&again).unwrap(),canonical,"{source}");
            assert!(canonical.contains(source.split('(').next().unwrap()),"{canonical}");
            assert_eq!(original.xml(),again.xml(),"{source}");
        }
    }
    #[test]
    fn natural_syntax_precedence_units_and_exact_unicode_ranges() {
        let source = "// 注释保留\nfrac(α + β, x^2, style: skewed)";
        let (root, nodes) = compile(source, false).unwrap();
        assert!(root.xml().contains("m:val=\"skw\""));
        assert!(root.xml().contains("<m:sSup>"));
        let fraction = nodes.iter().find(|n| n.kind == "f").unwrap();
        assert_eq!(fraction.from, source.find("frac").unwrap() - 8); // UTF-8 Chinese comment versus UTF-16
        assert_eq!(fraction.slots.len(), 2);
        assert!(xml("a / b + x^2").contains("<m:f>"));
        let matrix = xml("matrix([a, b], [c, d], align: [left, right], column_gap: 2em)");
        assert!(matrix.contains("<m:cGp m:val=\"4\"/>"));
        assert!(matrix.contains("<m:mcJc m:val=\"right\"/>"));
        let sum = xml("sum(x_i, from: i = 1, to: n, limits: above_below)");
        assert!(sum.contains("<m:limLoc m:val=\"undOvr\"/>"));
        assert!(sum.contains("<m:sSub>"));
    }
    #[test]
    fn standard_tree_roundtrip_keeps_explicit_false_word_attributes_and_empty_properties() {
        let original = math_document::parse_xml(&format!("<m:oMath xmlns:m=\"{MATH_NS}\" xmlns:w=\"{WORD_NS}\"><m:f><m:fPr><m:type m:val=\"skw\"/><m:ctrlPr><w:rPr><w:b w:val=\"0\"/></w:rPr></m:ctrlPr></m:fPr><m:num><m:r><m:rPr><m:nor m:val=\"0\"/></m:rPr><m:t xml:space=\"preserve\"> α </m:t></m:r></m:num><m:den/></m:f></m:oMath>")).unwrap();
        let source = decompile(&original).unwrap();
        assert!(!source.contains("<!--"));
        assert!(source.contains("word.b(val: \"0\")"));
        let (compiled, _) = compile(&source, false).unwrap();
        assert_eq!(decompile(&compiled).unwrap(), source);
        assert!(compiled.xml().contains("xml:space=\"preserve\""));
    }
    #[test]
    fn malformed_inputs_never_drop_parameters_or_extensions() {
        for source in [
            "frac(a)",
            "a /",
            "frac(a, b, style: wrong)",
            "frac(a, b, style: skewed, style: stacked)",
            "matrix([a], [b, c])",
            "matrix([a], column_gap: 0.3em)",
            "office.type(val: \"invalid\")",
            "office.f()",
            "office.f(office.num(a), office.den(b), office.t(\"invalid child\"))",
            "a b",
            "\"unterminated",
            "office.unknown()",
        ] {
            assert!(compile(source, false).is_err(), "{source}");
        }
        assert!(compile(&"(".repeat(200), false).is_err());
        let unknown =
            math_document::parse_xml("<m:oMath xmlns:x=\"urn:external\"><x:extra/></m:oMath>")
                .unwrap();
        assert_eq!(
            decompile(&unknown).unwrap_err(),
            "math_unsupported_extension"
        );
    }
    #[test]
    fn particle_grammar_rejects_order_duplicates_missing_attributes_and_foreign_children() {
        for source in [
            "office.f(office.den(b), office.num(a))",
            "office.f(office.num(a), office.num(b), office.den(c))",
            "office.type()",
            "office.chr()",
            "office.f(office.num(a), office.den(b), word.b())",
            "office.r(office.t(\"x\"), office.rPr())",
            "frac(a, b, style: skewed, properties: office.fPr(office.type(val: \"bar\")))",
        ] {
            assert!(compile(source, false).is_err(), "{source}");
        }
        assert!(compile(
            "office.r(office.rPr(), word.rPr(word.b(val: \"0\")), office.t(\"x\"))",
            false
        )
        .is_ok());
        assert!(compile("office.r(office.rPr(office.nor()), office.t(\"x\"))", false).is_ok()); // omitted switch remains distinct from false
        let source = "---\ntitle: ':math[frac(a, b)]'\n---\n\n正文 :math[α]\n";
        assert_eq!(markdown_ranges(source).len(), 1);
        assert!(expand_markdown(source)
            .unwrap()
            .contains("title: ':math[frac(a, b)]'"));
    }
    #[test]
    fn office_implicit_margin_values_keep_source_state_and_export_schema_valid_defaults(){
        let source="row(office.mathPr(office.lMargin(), office.rMargin(), office.defJc(val: \"left\")), office.oMath(x))";
        let(root,_)=compile(source,true).unwrap();let xml=root.xml();assert!(!xml.contains("1440"));
        let doc=crate::math_document::parse(source,"dsl",true).unwrap();
        let margin=doc.root.children().find(|n|n.local()=="mathPr").unwrap().children().find(|n|n.local()=="lMargin").unwrap();
        let explicit=crate::math_document::patch(source,"dsl",true,&doc.revision,&crate::math_document::MathPatch::SetAttribute{node_id:margin.id.clone(),name:"m:val".into(),value:Some("240".into())}).unwrap();
        let margin=explicit.root.children().find(|n|n.local()=="mathPr").unwrap().children().find(|n|n.local()=="lMargin").unwrap();
        let restored=crate::math_document::patch(&explicit.source,"dsl",true,&explicit.revision,&crate::math_document::MathPatch::SetAttribute{node_id:margin.id.clone(),name:"m:val".into(),value:None}).unwrap();
        assert!(restored.source.contains("office.lMargin()"));
        let(_,settings)=crate::omml::docx_math_fragment(&xml,true).unwrap();assert_eq!(settings.unwrap().xml().matches("val=\"1440\"").count(),2);assert!(!root.xml().contains("1440"));
        for bad in ["office.mathPr(office.lMargin(val: -1))","office.mathPr(office.lMargin(val: 31681))","office.mathPr(office.defJc(val: \"left\"), office.lMargin())","office.mathPr(office.preSp())"]{assert!(compile(bad,true).is_err(),"{bad}");}
    }
    #[test]
    fn every_standard_math_enum_compiles_as_a_readable_property_and_roundtrips() {
        let mut tested = BTreeSet::new();
        for element in math_document::schema()["elements"].as_array().unwrap() {
            if element["abstract"] == true {
                continue;
            }
            for attr in element["attributes"].as_array().unwrap() {
                let Some(name) = attr["type"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("EnumValue<DocumentFormat.OpenXml.Math.")
                    .and_then(|s| s.strip_suffix('>'))
                else {
                    continue;
                };
                let enumeration = math_document::schema()["enumerations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["name"] == name)
                    .unwrap();
                tested.insert(name);
                for facet in enumeration["facets"].as_array().unwrap() {
                    let value = facet["Value"].as_str().unwrap();
                    let source = format!(
                        "office.{}(val: {})",
                        element["element"].as_str().unwrap(),
                        serde_json::to_string(value).unwrap()
                    );
                    let mut parser = Parser {
                        tokens: lex(&source).unwrap(),
                        at: 0,
                    };
                    let expr = parser.expression(0, 0).unwrap();
                    let mut compiled = compile_expr(&expr).unwrap().remove(0);
                    compiled.attributes.remove("dslFrom");
                    compiled.attributes.remove("dslTo");
                    validate_tree(&compiled).unwrap();
                    let normalized = math_document::parse_xml(&compiled.xml()).unwrap();
                    assert_eq!(
                        decompile(normalized.children().next().unwrap()).unwrap(),
                        source
                    );
                }
            }
        }
        assert_eq!(tested.len(), 12);
        for source in [
            "office.cGp(val: 65536)",
            "office.cSp(val: 4294967296)",
            "office.t(\"x\", \"xml:space\": \"wrong\")",
        ] {
            let mut parser = Parser {
                tokens: lex(source).unwrap(),
                at: 0,
            };
            let expr = parser.expression(0, 0).unwrap();
            assert!(compile_expr(&expr).is_err(), "{source}");
        }
    }
    #[test]
    fn delimiter_and_nary_defaults_and_explicit_overrides_survive_dsl_round_trip() {
        for properties in ["", "<m:grow/>",r#"<m:grow m:val="0"/>"#,r#"<m:grow m:val="1"/><m:shp m:val="match"/>"#,
            r#"<m:begChr m:val=""/><m:sepChr m:val=""/><m:endChr m:val=""/>"#] {
            let xml=format!("<m:oMath><m:d><m:dPr>{properties}</m:dPr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:d></m:oMath>");
            let original=math_document::parse_xml(&xml).unwrap();
            let readable=crate::math_migration::readable(&xml).unwrap_or_else(|error|panic!("{properties}: {error}"));
            let (compiled,_)=compile(&readable,false).unwrap();
            assert_eq!(math_document::xml_semantics(&original),math_document::xml_semantics(&compiled));
            let mathml=|root:&XmlNode|crate::omml::to_mathml(&crate::omml::parse(&root.xml()).unwrap(),true,"");
            assert_eq!(mathml(&original),mathml(&compiled));
        }
        for properties in ["","<m:grow/>",r#"<m:grow m:val="0"/>"#,r#"<m:chr m:val=""/>"#] {
            let xml=format!("<m:oMath><m:nary><m:naryPr>{properties}</m:naryPr><m:sub/><m:sup/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>");
            let original=math_document::parse_xml(&xml).unwrap();
            let readable=crate::math_migration::readable(&xml).unwrap_or_else(|error|panic!("{properties}: {error}"));
            let (compiled,_)=compile(&readable,false).unwrap();
            assert_eq!(math_document::xml_semantics(&original),math_document::xml_semantics(&compiled));
        }
        let source="// 保留我的括号 🧮\ndelimiter( x, grow: true, shp: centered )";
        let doc=math_document::parse(source,"dsl",false).unwrap();
        let id=doc.nodes.iter().find(|n|n.kind=="d").unwrap().id.clone();
        let changed=math_document::patch(source,"dsl",false,&doc.revision,&math_document::MathPatch::SetProperty{node_id:id,container:"dPr".into(),property:"grow".into(),value:Some("0".into())}).unwrap();
        assert_eq!(changed.source,source.replace("grow: true","grow: 0"));
    }
    #[test]
    fn markdown_lowering_ignores_examples_and_preserves_standard_office_properties() {
        let source = "正文 :math[x^2 + α]\n\n```readmd-math v1\nfrac(a, b, style: skewed)\n```\n\n```markdown\n:math[a / b]\n```\n\n`:math[c / d]`\n";
        assert_eq!(markdown_ranges(source).len(), 2);
        let lowered = expand_markdown(source).unwrap();
        assert!(lowered.contains("```markdown\n:math[a / b]"));
        let document = crate::md_ast::parse(source);
        assert!(
            matches!(&document.blocks[0], crate::md_ast::Block::Paragraph(v) if v.iter().any(|i| matches!(i, crate::md_ast::Inline::OfficeMath {..})))
        );
        assert!(document.blocks.iter().any(
            |b| matches!(b, crate::md_ast::Block::OfficeMath {omml,..} if omml.contains("skw"))
        ));
        assert!(expand_markdown("```readmd-math v1\nfrac(a)\n```\n").is_err());
    }
    #[test]
    fn all_nineteen_core_structures_compile_render_and_roundtrip_as_readable_text() {
        let examples = [
            "accent(x, character: \"̂\")",
            "bar(x, position: below)",
            "box(x, opEmu: false)",
            "border_box(x, hideTop: true)",
            "delimiter(a, b, open: \"{\", close: \"}\")",
            "equations(a = b, c = d)",
            "frac(a, b, style: no_bar)",
            "function(sin, x)",
            "group_char(x, position: below, character: \"⏟\")",
            "lower_limit(x, n)",
            "upper_limit(x, n)",
            "matrix([a, b], [c, d])",
            "sum(x_i, from: i = 1, to: n)",
            "phantom(x, show: false)",
            "root(3, x)",
            "prescript(i, j, x)",
            "subscript(x, i)",
            "subsup(x, i, j)",
            "superscript(x, 2)",
        ];
        let mut kinds = std::collections::BTreeSet::new();
        for example in examples {
            let (root, _) =
                compile(example, false).unwrap_or_else(|error| panic!("{example}: {error}"));
            kinds.insert(
                root.children()
                    .next()
                    .unwrap()
                    .children()
                    .next()
                    .unwrap()
                    .local()
                    .to_string(),
            );
            let canonical = decompile(&root).unwrap();
            let again = compile(&canonical, false).unwrap();
            assert_eq!(decompile(&again.0).unwrap(), canonical, "{example}");
            let view = crate::omml::parse(&root.xml()).unwrap();
            assert!(crate::omml::to_mathml(&view, false, example).contains("<math"));
        }
        assert_eq!(kinds.len(), 19);
    }
    #[test]
    fn property_and_slot_edits_preserve_comments_formatting_and_utf16_positions() {
        use crate::{document_transaction, math_document::MathPatch};
        let source = "// 数学 🧮\nfrac( α, β, // 子注释\n style: stacked )";
        let document = math_document::parse(source, "dsl", false).unwrap();
        let fraction = document.nodes.iter().find(|n| n.kind == "f").unwrap();
        let changed = math_document::patch(
            source,
            "dsl",
            false,
            &document_transaction::revision(source),
            &MathPatch::SetProperty {
                node_id: fraction.id.clone(),
                container: "fPr".into(),
                property: "type".into(),
                value: Some("skw".into()),
            },
        )
        .unwrap();
        assert_eq!(changed.source, source.replace("stacked", "skewed"));
        assert_eq!(changed.dialect, "dsl");
        let denominator = changed.nodes.iter().find(|n| n.kind == "den").unwrap();
        let edited = math_document::patch(
            &changed.source,
            "dsl",
            false,
            &changed.revision,
            &MathPatch::ReplaceSource {
                node_id: denominator.id.clone(),
                text: "θ + 1".into(),
            },
        )
        .unwrap();
        assert_eq!(edited.source, changed.source.replace('β', "θ + 1"));
        let document = math_document::parse("frac(12, x^2)", "dsl", false).unwrap();
        let preview = math_document::preview(&document, false).unwrap();
        for node in document
            .nodes
            .iter()
            .filter(|n| matches!(n.kind.as_str(), "r" | "num" | "den" | "sup"))
        {
            assert!(
                preview.contains(&format!(
                    "rd-formula-token-omml-{}",
                    node.id.replace('.', "-")
                )),
                "{} missing from rendered map",
                node.kind
            );
        }
    }
}
