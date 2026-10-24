//! Apply the shared math policy to the legacy PDF JSON without rebuilding or
//! dropping its publication, layout, accessibility or future extension fields.
use std::borrow::Cow;
use serde_json::Value;
use crate::md_ast::{Block, Document, Inline};

const MAX_DEPTH: usize = 128;
const MAX_VALUES: usize = 1_000_000;

#[derive(Default)]
struct Budget { values: usize, bytes: usize }
impl Budget {
    fn visit(&mut self, value: &Value, depth: usize) -> Result<(), String> {
        self.values = self.values.checked_add(1).ok_or("math_budget")?;
        if depth > MAX_DEPTH || self.values > MAX_VALUES { return Err("math_budget".into()); }
        if self.values % 128 == 1 && crate::speech::cancelled() { return Err("cancelled".into()); }
        let bytes = match value {
            Value::String(text) => text.len(),
            Value::Object(map) => map.keys().try_fold(0usize, |sum, key| sum.checked_add(key.len()).ok_or("math_budget"))?,
            _ => 8,
        };
        self.bytes = self.bytes.checked_add(bytes).ok_or("math_budget")?;
        if self.bytes > crate::document_transaction::MAX_DOCUMENT_BYTES * 2 { return Err("math_budget".into()); }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Role { Blocks, Block, Inlines, Inline, InlineArray, ListItems, ListItem, Cells, Rows, BibliographyItems, BibliographyItem, LocalList, LocalNode }

#[derive(Clone, Copy)]
enum Traversal { Owned, Diagnostic }

#[derive(Clone, Copy)]
enum Step { Field(&'static str), Index(usize) }

struct Child<'a> { value: &'a Value, role: Role, step: Step }
enum Children<'a> {
    None,
    Array { values: std::iter::Enumerate<std::slice::Iter<'a, Value>>, role: Role },
    Fields { value: &'a Value, fields: std::slice::Iter<'static, (&'static str, Role)> },
}
impl<'a> Children<'a> {
    fn array(value: &'a Value, role: Role) -> Self {
        value.as_array().map_or(Self::None, |values| Self::Array { values: values.iter().enumerate(), role })
    }
    fn fields(value: &'a Value, fields: &'static [(&'static str, Role)]) -> Self { Self::Fields { value, fields: fields.iter() } }
    fn next(&mut self) -> Option<Child<'a>> {
        match self {
            Self::None => None,
            Self::Array { values, role } => values.next().map(|(index, value)| Child { value, role: *role, step: Step::Index(index) }),
            Self::Fields { value, fields } => {
                let parent = *value;
                for (field, role) in fields { if let Some(value) = parent.get(*field) { return Some(Child { value, role: *role, step: Step::Field(*field) }); } }
                None
            },
        }
    }
}

/// This is the PDF renderer's owned child grammar, not a search through JSON.
/// Extension fields, style dictionaries, links' href/title and math leaves are
/// opaque even when their values happen to contain a math discriminator.
fn children(value: &Value, role: Role, traversal: Traversal) -> Children<'_> {
    use Role::*;
    match role {
        Blocks => Children::array(value, Block),
        LocalList => Children::array(value, LocalNode),
        LocalNode => Children::None,
        Inlines if value.is_array() => Children::array(value, Inline),
        InlineArray => Children::array(value, Inline),
        ListItems => Children::array(value, ListItem),
        Cells => Children::array(value, Inlines),
        Rows => Children::array(value, Cells),
        BibliographyItems => Children::array(value, BibliographyItem),
        Block => match value.get("type").and_then(Value::as_str) {
            Some("paragraph" | "heading" | "caption") => Children::fields(value, &[("text", Inlines)]),
            Some("quote") => Children::fields(value, &[("title", Inlines), ("blocks", Blocks)]),
            Some("list") => Children::fields(value, &[("items", ListItems)]),
            Some("table") => Children::fields(value, &[("header", Cells), ("rows", Rows)]),
            Some("bibliography") => Children::fields(value, &[("items", BibliographyItems)]),
            _ if matches!(traversal, Traversal::Diagnostic) => reserved_content(value),
            _ => Children::None,
        },
        // The renderer normalizes wrapper contents: non-arrays become a text
        // node, so a bare object in v/text is not an inline-math child there.
        Inline | Inlines => match value.get("t").and_then(Value::as_str) {
            Some("bold" | "italic" | "strike" | "styled" | "author-styled" | "display") => Children::fields(value, &[("v", InlineArray)]),
            Some("link") => Children::fields(value, &[("text", InlineArray)]),
            Some("text" | "code" | "image" | "br") => Children::None,
            _ if matches!(traversal, Traversal::Diagnostic) => reserved_content(value),
            _ => Children::None,
        },
        ListItem => Children::fields(value, &[("text", Inlines)]),
        BibliographyItem => Children::fields(value, &[("firstField", Inlines), ("text", Inlines)]),
    }
}

/// An unsupported content wrapper must not silently hide vector-only math.
/// These arrays are diagnostic candidates, never author formula inventory.
/// No vendor/metadata/style field, or bare object in a reserved field, is read.
fn reserved_content(value: &Value) -> Children<'_> {
    Children::fields(value, &[("v", Role::LocalList), ("text", Role::LocalList), ("blocks", Role::LocalList)])
}

fn math_kind(value: &Value, role: Role) -> Option<bool> {
    // The actual consumer position determines display mode. Older callers
    // legitimately retain both type=math and t=math, or a stale display hint.
    match role {
        Role::Block if value.get("type").and_then(Value::as_str) == Some("math") => Some(true),
        Role::Inline | Role::Inlines if value.get("t").and_then(Value::as_str) == Some("math") => Some(false),
        _ => None,
    }
}

fn block_nested(value: &Value) -> bool {
    matches!(value.get("type").and_then(Value::as_str), Some("quote" | "list" | "table"))
        || value.get("publicationStyle").is_some() || value.get("publicationNoteLabel").is_some()
        || value.get("publicationContext").and_then(Value::as_str).is_some_and(|context| context != "body")
}

/// Iterators retain only the nesting spine. No math-specific cap is imposed
/// before finding an owned math site; unknown subtrees are never traversed.
fn walk_owned(value: &Value, root_role: Role, traversal: Traversal, mut visit: impl FnMut(&Value, bool, bool, &[Step]) -> Result<bool, String>) -> Result<bool, String> {
    struct Frame<'a> { children: Children<'a>, path_len: usize, nested: bool }
    let mut frames = Vec::<Frame<'_>>::new(); let mut path = Vec::new();
    let mut current = Some((value, root_role, false)); let mut visited = 0usize;
    loop {
        if let Some((value, role, inherited)) = current.take() {
            visited = visited.saturating_add(1);
            if visited % 128 == 1 && crate::speech::cancelled() { return Err("cancelled".into()); }
            // Local predicates receive an actual renderer-owned block/inline
            // node (or its list). Type wins only at that supplied local root;
            // its children immediately return to the strict owned grammar.
            let role = if matches!(role, Role::LocalNode) {
                if value.get("type").and_then(Value::as_str).is_some() { Role::Block } else { Role::Inline }
            } else { role };
            let nested = inherited || matches!(role, Role::Block) && block_nested(value);
            if let Some(display) = math_kind(value, role) {
                if visit(value, display, nested, &path)? { return Ok(true); }
            } else { frames.push(Frame { children: children(value, role, traversal), path_len: path.len(), nested }); }
        }
        let Some(frame) = frames.last_mut() else { return Ok(false); };
        if let Some(child) = frame.children.next() {
            path.truncate(frame.path_len); path.push(child.step);
            current = Some((child.value, child.role, frame.nested));
        } else { frames.pop(); }
    }
}

/// Shared with the PDF Word-paragraph validator so an ignored future field
/// cannot be reinterpreted as a formula by a later preflight.
pub(crate) fn visit_math_sites(value: &Value, mut visit: impl FnMut(&Value, bool, bool) -> Result<(), String>) -> Result<(), String> {
    walk_owned(value, Role::Blocks, Traversal::Owned, |value, display, nested, _| { visit(value, display, nested)?; Ok(false) })?;
    Ok(())
}

/// Local read-only predicates start at a consumer-owned inline/block node or
/// list. They do not get a rendering mode: complete policy analysis must use
/// visit_math_sites so an arbitrary root cannot become a document formula.
/// Unsupported wrappers' reserved content arrays are also checked solely to
/// preserve the consumer's existing reject-before-flattening diagnostics.
pub(crate) fn visit_math_subtree_sites(value: &Value, mut visit: impl FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    let role = if value.is_array() { Role::LocalList } else { Role::LocalNode };
    walk_owned(value, role, Traversal::Diagnostic, |value, _, _, _| { visit(value)?; Ok(false) })?;
    Ok(())
}

fn validate_clone_budget(value: &Value) -> Result<(), String> {
    enum AllChildren<'a> { None, Array(std::slice::Iter<'a, Value>), Object(serde_json::map::Values<'a>) }
    impl<'a> AllChildren<'a> {
        fn new(value: &'a Value) -> Self { match value { Value::Array(values) => Self::Array(values.iter()), Value::Object(values) => Self::Object(values.values()), _ => Self::None } }
        fn next(&mut self) -> Option<&'a Value> { match self { Self::None => None, Self::Array(values) => values.next(), Self::Object(values) => values.next() } }
    }
    // Even opaque payloads are charged before cloning. This bounded size-only
    // walk never interprets their keys or values as mathematical declarations.
    let mut budget = Budget::default(); budget.visit(value, 0)?;
    let mut frames = vec![AllChildren::new(value)];
    while let Some(frame) = frames.last_mut() {
        if let Some(child) = frame.next() { budget.visit(child, frames.len())?; frames.push(AllChildren::new(child)); }
        else { frames.pop(); }
    }
    Ok(())
}

struct Site { path: Vec<Step>, ordinary: bool }
fn collect(value: &Value, blocks: &mut Vec<Block>) -> Result<Vec<Site>, String> {
    let mut sites = Vec::new();
    walk_owned(value, Role::Blocks, Traversal::Owned, |value, display, _, path| {
        if blocks.len() >= crate::document_transaction::MAX_OPERATIONS { return Err("math_budget".into()); }
        let xml = value.get("omml").map(|xml| xml.as_str().ok_or("math_invalid_source")).transpose()?;
        let latex = match value.get("latex") {
            Some(Value::String(latex)) => latex.clone(),
            None if xml.is_some() => String::new(),
            _ => return Err("math_invalid_source".into()),
        };
        blocks.push(match (display, xml) {
            (true, Some(xml)) => Block::OfficeMath { latex, omml: xml.into() },
            (true, None) => Block::Math(latex),
            (false, Some(xml)) => Block::Paragraph(vec![Inline::OfficeMath { latex, omml: xml.into() }]),
            (false, None) => Block::Paragraph(vec![Inline::Math(latex)]),
        });
        sites.push(Site { path: path.to_vec(), ordinary: xml.is_none() }); Ok(false)
    })?;
    Ok(sites)
}

fn at_path_mut<'a>(mut value: &'a mut Value, path: &[Step]) -> Result<&'a mut Value, String> {
    for step in path { value = match step { Step::Field(field) => value.get_mut(*field), Step::Index(index) => value.get_mut(*index) }.ok_or("transaction_conflict")?; }
    Ok(value)
}

pub(crate) fn resolve(value: &Value) -> Result<Cow<'_, Value>, String> {
    // Do not impose new math-specific depth/size limits on documents which
    // never use mathematics. An iterative borrowed scan avoids recursion.
    let has_math = walk_owned(value, Role::Blocks, Traversal::Owned, |_, _, _, _| Ok(true))?;
    if !has_math{return Ok(Cow::Borrowed(value));}
    validate_clone_budget(value)?;
    let mut author = Document { front_matter: None, blocks: Vec::new() };
    let sites = collect(value, &mut author.blocks)?;
    let plan = crate::math_document_defaults::DocumentDefaultsPlan::analyze(&author).map_err(|error| error.message())?;
    if !plan.is_active() { return Ok(Cow::Borrowed(value)); }
    let rendered = plan.render_document(&author).map_err(|error| error.message())?;
    if sites.len() != rendered.blocks.len() { return Err("transaction_conflict".into()); }
    let mut resolved = value.clone();
    for (site, block) in sites.iter().zip(&rendered.blocks) {
        if crate::speech::cancelled() { return Err("cancelled".into()); }
        if !site.ordinary { continue; }
        let xml = match block {
            Block::OfficeMath { omml, .. } => Some(omml.as_str()),
            Block::Paragraph(inlines) => match inlines.first() { Some(Inline::OfficeMath { omml, .. }) => Some(omml.as_str()), _ => None },
            _ => None,
        }.ok_or("transaction_conflict")?;
        at_path_mut(&mut resolved, &site.path)?.as_object_mut().ok_or("transaction_conflict")?.insert("omml".into(), Value::String(xml.into()));
    }
    Ok(Cow::Owned(resolved))
}

#[cfg(test)]
#[path = "math_render_json_tests.rs"]
mod tests;
