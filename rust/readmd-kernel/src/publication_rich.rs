//! Bounded citation output and source-bound publication slots.
//! Only this module knows the processor's element types; writers consume the
//! restricted tree below, never executable HTML or Typst source.
use crate::{ApiError, ApiResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_RICH_BYTES: usize = 64 * 1024 * 1024;
const MAX_RICH_DEPTH: usize = 16;
const MAX_RICH_NODES: usize = 2048;
const SLOT_PREFIX: &str = "<readmd-csl data-slot=\"";

macro_rules! format_enum {
    ($name:ident { $default:ident $(, $other:ident)* }) => {
        #[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name { #[default] $default, $($other),* }
    };
}
format_enum!(RichFontStyle { Normal, Italic, Oblique });
format_enum!(RichFontVariant { Normal, SmallCaps });
format_enum!(RichFontWeight { Normal, Bold, Light });
format_enum!(RichDecoration { None, Underline });
format_enum!(RichVerticalAlign { Baseline, Sup, Sub });

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RichFormat {
    pub font_style: RichFontStyle,
    pub font_variant: RichFontVariant,
    pub font_weight: RichFontWeight,
    pub text_decoration: RichDecoration,
    pub vertical_align: RichVerticalAlign,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RichDisplay { Block, Indent, LeftMargin, RightInline }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RichNode {
    Text { text: String, #[serde(default)] format: RichFormat },
    Link { href: String, children: Vec<RichNode> },
    Group {
        children: Vec<RichNode>,
        #[serde(default)] display: Option<RichDisplay>,
        #[serde(default)] entry_index: Option<usize>,
    },
}
impl RichNode {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into(), format: RichFormat::default() }
    }
}

/// A display-aware flow derived from the original restricted processor tree.
/// The widest actual left-cell measurement is the bibliography's preferred
/// label width. Each row limits its actual left column to 35% of its parent,
/// wraps labels and preserves a 0.75em gutter; nested containers may be narrower.
/// Group/Link retain entry identity and hyperlink ownership without adding a
/// paragraph boundary.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RichLayoutNode {
    Inline { children: Vec<RichNode> },
    Block { children: Vec<RichLayoutNode>, indent: bool },
    Columns { left: Vec<RichLayoutNode>, right: Vec<RichLayoutNode> },
    Group { children: Vec<RichLayoutNode>, entry_index: Option<usize> },
    Link { href: String, children: Vec<RichLayoutNode> },
}

struct DisplayPart<'a> {
    display: Option<RichDisplay>,
    node: &'a RichNode,
    ancestors: Vec<Option<usize>>,
}
impl DisplayPart<'_> {
    fn whitespace(&self) -> bool {
        fn empty(node: &RichNode) -> bool {
            match node {
                // NBSP and other visible Unicode spaces are content, rather
                // than HTML/CSL source indentation that may collapse away.
                RichNode::Text { text, .. } => text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0c')),
                RichNode::Group { children, .. } | RichNode::Link { children, .. } => children.iter().all(empty),
            }
        }
        empty(self.node)
    }
    fn cloned_nodes(&self) -> Vec<RichNode> {
        let mut nodes = vec![self.node.clone()];
        for entry_index in self.ancestors.iter().rev() {
            nodes = vec![RichNode::Group { children: nodes, display: None, entry_index: *entry_index }];
        }
        nodes
    }
}

// A processor-only Group does not create a layout boundary. Keep its identity
// on every resulting part rather than discarding it while finding siblings.
// Links are not duplicated: a long validated URL must not multiply per leaf.
fn display_parts(nodes: &[RichNode]) -> Vec<DisplayPart<'_>> {
    fn collect<'a>(nodes: &'a [RichNode], ancestors: &mut Vec<Option<usize>>, output: &mut Vec<DisplayPart<'a>>) {
        for node in nodes {
            if let RichNode::Group { children, display: None, entry_index } = node {
                if !children.is_empty() {
                    ancestors.push(*entry_index); collect(children, ancestors, output); ancestors.pop();
                    continue;
                }
            }
            let display = match node { RichNode::Group { display, .. } => *display, _ => None };
            output.push(DisplayPart { display, node, ancestors: ancestors.clone() });
        }
    }
    let mut output = Vec::new(); collect(nodes, &mut Vec::new(), &mut output);
    output
}

fn consume_display(nodes: &mut [RichNode], expected: RichDisplay) -> bool {
    for node in nodes {
        if let RichNode::Group { children, display, .. } = node {
            if *display == Some(expected) { *display = None; return true; }
            if display.is_none() && consume_display(children, expected) { return true; }
        }
    }
    false
}

/// Project a bounded rich tree without reducing multiple blocks or nested
/// columns to a single first-field/body pair. Ordinary nonempty inline output
/// and real block/indent nodes end a sibling column pair; whitespace does not.
pub fn layout(nodes: &[RichNode]) -> ApiResult<Vec<RichLayoutNode>> {
    validate(nodes)?;
    Ok(project_layout(nodes, None))
}

fn project_layout(nodes: &[RichNode], field_root: Option<RichDisplay>) -> Vec<RichLayoutNode> {
    fn atom(part: &DisplayPart<'_>, consumed: Option<RichDisplay>) -> Vec<RichLayoutNode> {
        let node = match part.node {
            RichNode::Text { .. } => RichLayoutNode::Inline { children: vec![part.node.clone()] },
            RichNode::Link { href, children } => RichLayoutNode::Link {
                href: href.clone(), children: project_layout(children, None),
            },
            RichNode::Group { children, display, entry_index } => {
                let mut children = project_layout(children, None);
                if display.is_some() && *display != consumed && !part.whitespace() {
                    children = vec![RichLayoutNode::Block {
                        children, indent: *display == Some(RichDisplay::Indent),
                    }];
                }
                RichLayoutNode::Group { children, entry_index: *entry_index }
            }
        };
        let mut output = vec![node];
        for entry_index in part.ancestors.iter().rev() {
            output = vec![RichLayoutNode::Group { children: output, entry_index: *entry_index }];
        }
        output
    }
    let parts = display_parts(nodes);
    if let Some(expected) = field_root {
        let mut visible = parts.iter().filter(|part| !part.whitespace());
        if visible.next().is_some_and(|part| part.display == Some(expected)) && visible.next().is_none() {
            return parts.iter().flat_map(|part| atom(part, Some(expected))).collect();
        }
    }
    let mut parts = parts.into_iter().peekable();
    let mut output = Vec::new();
    while let Some(part) = parts.next() {
        if part.display == Some(RichDisplay::LeftMargin) && !part.whitespace() {
            let mut trivia = Vec::new();
            while parts.peek().is_some_and(DisplayPart::whitespace) {
                trivia.extend(atom(&parts.next().unwrap(), None));
            }
            if parts.peek().is_some_and(|next| next.display == Some(RichDisplay::RightInline)) {
                let mut left = atom(&part, Some(RichDisplay::LeftMargin));
                left.extend(trivia);
                let mut right = Vec::new();
                loop {
                    let next = parts.next().unwrap();
                    right.push(RichLayoutNode::Block {
                        children: atom(&next, Some(RichDisplay::RightInline)), indent: false,
                    });
                    // Consecutive right-inline blocks are separate body
                    // paragraphs belonging to this one preceding label.
                    while parts.peek().is_some_and(DisplayPart::whitespace) {
                        right.extend(atom(&parts.next().unwrap(), None));
                    }
                    if !parts.peek().is_some_and(|next| next.display == Some(RichDisplay::RightInline)) { break; }
                }
                output.push(RichLayoutNode::Columns { left, right });
                continue;
            }
            output.extend(atom(&part, None)); output.extend(trivia);
        } else { output.extend(atom(&part, None)); }
    }
    output
}

/// Whether a recursive flow contributes labels to the bibliography-wide
/// shared left-column measurement. No source text is searched or rewritten.
pub fn layout_has_columns(nodes: &[RichLayoutNode]) -> bool {
    nodes.iter().any(|node| match node {
        RichLayoutNode::Columns { .. } => true,
        RichLayoutNode::Block { children, .. } | RichLayoutNode::Group { children, .. }
        | RichLayoutNode::Link { children, .. } => layout_has_columns(children),
        RichLayoutNode::Inline { .. } => false,
    })
}

/// True only when a flow has no paragraph/column boundaries of its own.
pub fn layout_is_inline(nodes: &[RichLayoutNode]) -> bool {
    nodes.iter().all(|node| match node {
        RichLayoutNode::Inline { .. } => true,
        RichLayoutNode::Group { children, .. } | RichLayoutNode::Link { children, .. } => layout_is_inline(children),
        RichLayoutNode::Block { .. } | RichLayoutNode::Columns { .. } => false,
    })
}

/// Whether a validated layout needs the recursive writer instead of the
/// historical first-field/full-width paragraph adapter. Transparent identities
/// do not add paragraphs. One inline column pair or one unindented inline block
/// stays compatible, as does ordinary inline output. Links around structural
/// flow stay recursive: the old first_field adapter does not split their owner.
/// Validation is separate; callers must not treat this predicate as validation.
pub fn layout_requires_flow(nodes: &[RichLayoutNode]) -> bool {
    fn trivia(nodes: &[RichLayoutNode]) -> bool {
        nodes.iter().all(|node| match node {
            RichLayoutNode::Inline { children } => children.iter().all(|child| matches!(child,
                RichNode::Text { text, .. } if text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0c')))),
            RichLayoutNode::Group { children, .. } | RichLayoutNode::Link { children, .. } => trivia(children),
            RichLayoutNode::Block { .. } | RichLayoutNode::Columns { .. } => false,
        })
    }
    fn single<'a>(nodes: &'a [RichLayoutNode], candidate: &mut Option<&'a RichLayoutNode>) -> bool {
        for node in nodes {
            match node {
                RichLayoutNode::Group { children, .. } => if !single(children, candidate) { return false; },
                RichLayoutNode::Inline { .. } | RichLayoutNode::Link { .. } => {
                    if !trivia(std::slice::from_ref(node)) { return false; }
                }
                RichLayoutNode::Block { .. } | RichLayoutNode::Columns { .. } => {
                    if candidate.replace(node).is_some() { return false; }
                }
            }
        }
        true
    }
    if layout_is_inline(nodes) { return false; }
    let mut candidate = None;
    if !single(nodes, &mut candidate) { return true; }
    match candidate {
        Some(RichLayoutNode::Columns { left, right }) => !layout_is_inline(left) || !layout_is_inline(right),
        Some(RichLayoutNode::Block { children, indent: false }) => !layout_is_inline(children),
        None => false,
        _ => true,
    }
}

fn rich_error(code: &'static str) -> ApiError { ApiError::new(422, code) }
pub fn safe_link(href: &str) -> bool {
    !href.is_empty() && !href.chars().any(char::is_control)
        && (href.starts_with("https://") || href.starts_with("http://") || href.starts_with("mailto:"))
}

pub fn validate(nodes: &[RichNode]) -> ApiResult<()> {
    fn walk(nodes: &[RichNode], depth: usize, count: &mut usize, bytes: &mut usize) -> ApiResult<()> {
        if depth > MAX_RICH_DEPTH { return Err(rich_error("academic_rich_budget")); }
        for node in nodes {
            *count += 1;
            if *count > MAX_RICH_NODES { return Err(rich_error("academic_rich_budget")); }
            match node {
                RichNode::Text { text, .. } => *bytes = bytes.saturating_add(text.len()),
                RichNode::Link { href, children } => {
                    if !safe_link(href) { return Err(rich_error("academic_rich_link")); }
                    *bytes = bytes.saturating_add(href.len());
                    walk(children, depth + 1, count, bytes)?;
                }
                RichNode::Group { children, .. } => walk(children, depth + 1, count, bytes)?,
            }
            if *bytes > MAX_RICH_BYTES { return Err(rich_error("academic_rich_budget")); }
        }
        Ok(())
    }
    walk(nodes, 0, &mut 0, &mut 0)
}

pub fn from_elements(children: &hayagriva::ElemChildren) -> ApiResult<Vec<RichNode>> {
    use hayagriva::citationberg::{Display, FontStyle, FontVariant, FontWeight, TextDecoration, VerticalAlign};
    fn format(f: hayagriva::Formatting) -> RichFormat {
        RichFormat {
            font_style: match f.font_style { FontStyle::Normal => RichFontStyle::Normal, FontStyle::Italic => RichFontStyle::Italic },
            font_variant: if f.font_variant == FontVariant::SmallCaps { RichFontVariant::SmallCaps } else { RichFontVariant::Normal },
            font_weight: match f.font_weight { FontWeight::Normal => RichFontWeight::Normal, FontWeight::Bold => RichFontWeight::Bold, FontWeight::Light => RichFontWeight::Light },
            text_decoration: if f.text_decoration == TextDecoration::Underline { RichDecoration::Underline } else { RichDecoration::None },
            vertical_align: match f.vertical_align { VerticalAlign::Sup => RichVerticalAlign::Sup, VerticalAlign::Sub => RichVerticalAlign::Sub, VerticalAlign::None | VerticalAlign::Baseline => RichVerticalAlign::Baseline },
        }
    }
    fn convert(children: &hayagriva::ElemChildren, depth: usize, count: &mut usize) -> ApiResult<Vec<RichNode>> {
        use hayagriva::{ElemChild, ElemMeta};
        if depth > MAX_RICH_DEPTH { return Err(rich_error("academic_rich_budget")); }
        let mut nodes = Vec::new();
        for child in &children.0 {
            *count += 1;
            if *count > MAX_RICH_NODES { return Err(rich_error("academic_rich_budget")); }
            nodes.push(match child {
                ElemChild::Text(text) => RichNode::Text { text: text.text.clone(), format: format(text.formatting) },
                ElemChild::Link { text, url } => RichNode::Link {
                    href: url.clone(), children: vec![RichNode::Text { text: text.text.clone(), format: format(text.formatting) }],
                },
                ElemChild::Elem(elem) => RichNode::Group {
                    children: convert(&elem.children, depth + 1, count)?,
                    display: elem.display.map(|display| match display {
                        Display::Block => RichDisplay::Block, Display::Indent => RichDisplay::Indent,
                        Display::LeftMargin => RichDisplay::LeftMargin, Display::RightInline => RichDisplay::RightInline,
                    }),
                    entry_index: match &elem.meta { Some(ElemMeta::Entry(index)) => Some(*index), _ => None },
                },
                ElemChild::Markup(_) | ElemChild::Transparent { .. } => return Err(rich_error("academic_rich_unsupported")),
            });
        }
        Ok(nodes)
    }
    let nodes = convert(children, 0, &mut 0)?;
    validate(&nodes)?;
    Ok(nodes)
}

/// The inverse adapter is used only for validated per-item affixes. It cannot
/// produce Markup or Transparent elements, including from a hostile API call.
pub fn to_elements(nodes: &[RichNode]) -> ApiResult<hayagriva::ElemChildren> {
    use hayagriva::{Elem, ElemChild, ElemChildren, Formatted, Formatting};
    use hayagriva::citationberg::{Display, FontStyle, FontVariant, FontWeight, TextDecoration, VerticalAlign};
    validate(nodes)?;
    let mut output = Vec::new();
    for node in nodes {
        output.push(match node {
            RichNode::Text { text, format } => ElemChild::Text(Formatted { text: text.clone(), formatting: Formatting {
                // citationberg 0.7.0 supports normal/italic only. The output
                // AST can retain oblique, but an affix cannot silently lose it
                // while being converted into this locked processor's format.
                font_style: match format.font_style { RichFontStyle::Normal => FontStyle::Normal, RichFontStyle::Italic => FontStyle::Italic, RichFontStyle::Oblique => return Err(rich_error("academic_rich_unsupported").noted("field","font_style").noted("value","oblique")) },
                font_variant: if format.font_variant == RichFontVariant::SmallCaps { FontVariant::SmallCaps } else { FontVariant::Normal },
                font_weight: match format.font_weight { RichFontWeight::Normal => FontWeight::Normal, RichFontWeight::Bold => FontWeight::Bold, RichFontWeight::Light => FontWeight::Light },
                text_decoration: if format.text_decoration == RichDecoration::Underline { TextDecoration::Underline } else { TextDecoration::None },
                vertical_align: match format.vertical_align { RichVerticalAlign::Baseline => VerticalAlign::Baseline, RichVerticalAlign::Sup => VerticalAlign::Sup, RichVerticalAlign::Sub => VerticalAlign::Sub },
            }}),
            RichNode::Group { children, display, .. } => ElemChild::Elem(Elem {
                children: to_elements(children)?, meta: None,
                display: display.map(|display| match display { RichDisplay::Block => Display::Block, RichDisplay::Indent => Display::Indent, RichDisplay::LeftMargin => Display::LeftMargin, RichDisplay::RightInline => Display::RightInline }),
            }),
            RichNode::Link { href, children } => {
                let children = to_elements(children)?;
                let mut linked = Vec::new();
                fn link(children: ElemChildren, href: &str, out: &mut Vec<ElemChild>) {
                    for child in children.0 { match child {
                        ElemChild::Text(text) => out.push(ElemChild::Link { text, url: href.into() }),
                        ElemChild::Elem(mut elem) => { let mut nested = Vec::new(); link(elem.children, href, &mut nested); elem.children = ElemChildren(nested); out.push(ElemChild::Elem(elem)); },
                        other => out.push(other),
                    }}
                }
                link(children, href, &mut linked);
                ElemChild::Elem(Elem { children: ElemChildren(linked), display: None, meta: None })
            }
        });
    }
    Ok(ElemChildren(output))
}

pub fn plain(nodes: &[RichNode]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            RichNode::Text { text: value, .. } => text.push_str(value),
            RichNode::Link { children, .. } => text.push_str(&plain(children)),
            RichNode::Group { children, display, .. } => {
                if *display == Some(RichDisplay::Block) && !text.is_empty() { text.push('\n'); }
                text.push_str(&plain(children));
                if *display == Some(RichDisplay::Block) { text.push('\n'); }
            }
        }
    }
    text
}
fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
fn literal_html_text(value: &str) -> String {
    // This safe fragment also passes through a Markdown reader. Entities keep
    // literal CSL punctuation and line breaks from becoming Markdown syntax.
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"), '\'' => out.push_str("&#39;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '$' | '~' | '\r' | '\n' => out.push_str(&format!("&#{};", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// Only constant CSS properties and escaped literal content can be emitted.
pub fn html(nodes: &[RichNode]) -> ApiResult<String> {
    validate(nodes)?;
    fn render(nodes: &[RichNode], out: &mut String) {
        for node in nodes {
            match node {
                RichNode::Text { text, format } => {
                    let mut css = String::new();
                    match format.font_style { RichFontStyle::Normal => {}, RichFontStyle::Italic => css.push_str("font-style:italic;"), RichFontStyle::Oblique => css.push_str("font-style:oblique;") }
                    if format.font_variant == RichFontVariant::SmallCaps { css.push_str("font-variant:small-caps;"); }
                    match format.font_weight { RichFontWeight::Normal => {}, RichFontWeight::Bold => css.push_str("font-weight:bold;"), RichFontWeight::Light => css.push_str("font-weight:300;") }
                    if format.text_decoration == RichDecoration::Underline { css.push_str("text-decoration:underline;"); }
                    match format.vertical_align { RichVerticalAlign::Baseline => {}, RichVerticalAlign::Sup => css.push_str("vertical-align:super;font-size:smaller;"), RichVerticalAlign::Sub => css.push_str("vertical-align:sub;font-size:smaller;") }
                    if css.is_empty() { out.push_str(&literal_html_text(text)); }
                    else { out.push_str(&format!("<span class=\"{}\" style=\"{css}\">{}</span>", crate::md_ast::rich_format_classes(format), literal_html_text(text))); }
                }
                RichNode::Link { href, children } => {
                    out.push_str(&format!("<a href=\"{}\">", escape(href))); render(children, out); out.push_str("</a>");
                }
                RichNode::Group { children, display, .. } => {
                    let css = match display { None => "", Some(RichDisplay::Block) => "display:block;", Some(RichDisplay::Indent) => "display:block;padding-left:4em;", Some(RichDisplay::LeftMargin) => "display:inline-block;", Some(RichDisplay::RightInline) => "display:inline;" };
                    let class = match display { None => "", Some(RichDisplay::Block) => "readmd-csl-display-block", Some(RichDisplay::Indent) => "readmd-csl-display-indent", Some(RichDisplay::LeftMargin) => "readmd-csl-display-left-margin", Some(RichDisplay::RightInline) => "readmd-csl-display-right-inline" };
                    if !css.is_empty() { out.push_str(&format!("<span class=\"{class}\" style=\"{css}\">")); }
                    render(children, out);
                    if !css.is_empty() { out.push_str("</span>"); }
                }
            }
        }
    }
    let mut output = String::new(); render(nodes, &mut output); Ok(output)
}

/// Validate a derived flow before a writer accepts externally transported JSON.
/// The original source budget remains authoritative; this allows only the
/// bounded expansion of its transparent ancestor identities.
pub fn validate_layout(nodes: &[RichLayoutNode]) -> ApiResult<()> {
    fn validate_flow(nodes: &[RichLayoutNode], depth: usize, count: &mut usize, bytes: &mut usize) -> ApiResult<()> {
        // Transparent ancestor identities can be present on both cells; this
        // derived bound accommodates that expansion of a validated source tree.
        if depth > MAX_RICH_DEPTH * 2 + 4 { return Err(rich_error("academic_rich_budget")); }
        for node in nodes {
            *count += 1;
            if *count > MAX_RICH_NODES * (MAX_RICH_DEPTH + 2) { return Err(rich_error("academic_rich_budget")); }
            match node {
                RichLayoutNode::Inline { children } => {
                    for child in children {
                        let RichNode::Text { text, .. } = child else {
                            return Err(rich_error("academic_rich_unsupported").noted("field", "layout.inline"));
                        };
                        *count += 1; *bytes = bytes.saturating_add(text.len());
                        if *bytes > MAX_RICH_BYTES || *count > MAX_RICH_NODES * (MAX_RICH_DEPTH + 2) {
                            return Err(rich_error("academic_rich_budget"));
                        }
                    }
                }
                RichLayoutNode::Columns { left, right } => {
                    validate_flow(left, depth + 1, count, bytes)?;
                    validate_flow(right, depth + 1, count, bytes)?;
                }
                RichLayoutNode::Block { children, .. } | RichLayoutNode::Group { children, .. } => validate_flow(children, depth + 1, count, bytes)?,
                RichLayoutNode::Link { href, children } => {
                    if !safe_link(href) { return Err(rich_error("academic_rich_link")); }
                    *bytes = bytes.saturating_add(href.len());
                    validate_flow(children, depth + 1, count, bytes)?;
                }
            }
            if *bytes > MAX_RICH_BYTES || *count > MAX_RICH_NODES * (MAX_RICH_DEPTH + 2) {
                return Err(rich_error("academic_rich_budget"));
            }
        }
        Ok(())
    }
    validate_flow(nodes, 0, &mut 0, &mut 0)
}

/// Safe recursive display HTML. The initial CSS uses each container's
/// fit-content(35%) intrinsic label width. The reader measures all labels in
/// one bibliography and sets a numeric preferred-width variable plus the fixed
/// column-template variable before declaring its complex layout ready.
pub fn layout_html(nodes: &[RichLayoutNode]) -> ApiResult<String> {
    layout_html_with_hanging(nodes, false)
}

/// Apply hanging independently to full-width paragraphs. A column cell and
/// every descendant keep their own left edge, even if the same entry also has
/// full-width paragraphs before or after its columns.
pub fn layout_html_with_hanging(nodes: &[RichLayoutNode], hanging: bool) -> ApiResult<String> {
    fn flush(pending: &mut String, visible: &mut bool, output: &mut String, hanging: bool) {
        if *visible {
            if hanging {
                output.push_str("<span class=\"readmd-csl-layout-paragraph readmd-csl-layout-hanging\" style=\"display:block;min-width:0;max-width:100%;box-sizing:border-box;overflow-wrap:anywhere;padding-left:2em;text-indent:-2em;\">");
            }
            output.push_str(pending);
            if hanging { output.push_str("</span>"); }
        }
        pending.clear(); *visible = false;
    }
    fn render_nodes(nodes: &[RichLayoutNode], output: &mut String, pending: &mut String, visible: &mut bool, hanging: bool, href: Option<&str>) -> ApiResult<()> {
        for node in nodes {
            match node {
                RichLayoutNode::Inline { children } => {
                    if let Some(href) = href { pending.push_str(&format!("<a href=\"{}\">", escape(href))); }
                    pending.push_str(&html(children)?);
                    if href.is_some() { pending.push_str("</a>"); }
                    *visible |= children.iter().any(|child| matches!(child, RichNode::Text { text, .. } if text.chars().any(|c| !matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0c'))));
                }
                RichLayoutNode::Group { children, .. } => render_nodes(children, output, pending, visible, hanging, href)?,
                RichLayoutNode::Link { href, children } => render_nodes(children, output, pending, visible, hanging, Some(href))?,
                RichLayoutNode::Block { children, indent } => {
                    flush(pending, visible, output, hanging);
                    let indent_class = if *indent { " readmd-csl-layout-indent" } else { "" };
                    let indent_css = if *indent { "padding-left:4em;" } else { "" };
                    output.push_str(&format!("<span class=\"readmd-csl-layout-block{indent_class}\" style=\"display:block;min-width:0;max-width:100%;box-sizing:border-box;overflow-wrap:anywhere;{indent_css}\">"));
                    render(children, output, hanging, href)?; output.push_str("</span>");
                }
                RichLayoutNode::Columns { left, right } => {
                    flush(pending, visible, output, hanging);
                    output.push_str("<span class=\"readmd-csl-layout-columns\" style=\"display:grid;grid-template-columns:var(--readmd-csl-column-template,fit-content(35%) minmax(0,1fr));column-gap:.75em;min-width:0;max-width:100%;text-indent:0;\"><span class=\"readmd-csl-layout-left\" style=\"min-width:0;max-width:100%;white-space:normal;overflow-wrap:anywhere;\">");
                    render(left, output, false, href)?;
                    output.push_str("</span><span class=\"readmd-csl-layout-right\" style=\"min-width:0;max-width:100%;white-space:normal;overflow-wrap:anywhere;\">");
                    render(right, output, false, href)?; output.push_str("</span></span>");
                }
            }
            if output.len().saturating_add(pending.len()) > MAX_RICH_BYTES { return Err(rich_error("academic_rich_budget")); }
        }
        Ok(())
    }
    fn render(nodes: &[RichLayoutNode], output: &mut String, hanging: bool, href: Option<&str>) -> ApiResult<()> {
        let mut pending = String::new(); let mut visible = false;
        render_nodes(nodes, output, &mut pending, &mut visible, hanging, href)?;
        flush(&mut pending, &mut visible, output, hanging);
        if output.len() > MAX_RICH_BYTES { return Err(rich_error("academic_rich_budget")); }
        Ok(())
    }
    validate_layout(nodes)?;
    let mut output = String::new(); render(nodes, &mut output, hanging, None)?; Ok(output)
}

/// A restricted Markdown affix. No HTML, code, math, image or nested citation
/// becomes an executable processor fragment. Emphasis and safe links survive.
pub fn markdown_affix(source: &str) -> ApiResult<Vec<RichNode>> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    if source.len() > 4096 { return Err(rich_error("academic_context_budget")); }
    let mut stack: Vec<(Vec<RichNode>, RichFormat, Option<String>)> = vec![(Vec::new(), RichFormat::default(), None)];
    for event in Parser::new(source) {
        match event {
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => {},
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Link { .. }) => {
                let mut format = stack.last().unwrap().1.clone();
                let href = match event {
                    Event::Start(Tag::Emphasis) => { format.font_style = RichFontStyle::Italic; None },
                    Event::Start(Tag::Strong) => { format.font_weight = RichFontWeight::Bold; None },
                    Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.into_string()),
                    _ => unreachable!(),
                };
                if stack.len() >= MAX_RICH_DEPTH { return Err(rich_error("academic_rich_budget")); }
                stack.push((Vec::new(), format, href));
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Link) => {
                let (children, _, href) = stack.pop().ok_or_else(|| rich_error("academic_context"))?;
                let parent = stack.last_mut().ok_or_else(|| rich_error("academic_context"))?;
                if let Some(href) = href { parent.0.push(RichNode::Link { href, children }); }
                else { parent.0.extend(children); }
            }
            Event::Text(value) => {
                let (nodes, format, _) = stack.last_mut().unwrap();
                nodes.push(RichNode::Text { text: value.into_string(), format: format.clone() });
            }
            Event::SoftBreak | Event::HardBreak => stack.last_mut().unwrap().0.push(RichNode::text(" ")),
            _ => return Err(rich_error("academic_affix_unsupported")),
        }
    }
    if stack.len() != 1 { return Err(rich_error("academic_context")); }
    let nodes = stack.pop().unwrap().0; validate(&nodes)?; Ok(nodes)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub byte_from: usize, pub byte_to: usize,
    pub from: usize, pub to: usize,
    pub source: String,
}
impl SourceSpan {
    pub fn new(source: &str, range: std::ops::Range<usize>) -> ApiResult<Self> {
        let Some(raw) = source.get(range.clone()) else { return Err(rich_error("academic_source_range")); };
        Ok(Self { byte_from: range.start, byte_to: range.end, from: source[..range.start].encode_utf16().count(), to: source[..range.end].encode_utf16().count(), source: raw.into() })
    }
    fn matches(&self, source: &str) -> bool {
        source.get(self.byte_from..self.byte_to) == Some(self.source.as_str())
            && source[..self.byte_from].encode_utf16().count() == self.from
            && source[..self.byte_to].encode_utf16().count() == self.to
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceItem {
    pub cite: crate::academic::Cite,
    pub span: SourceSpan,
    pub fields: BTreeMap<String, SourceSpan>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceCitation {
    pub span: SourceSpan,
    pub items: Vec<SourceItem>,
    pub context: crate::academic::CiteContext,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectedCitation {
    pub source: SourceCitation,
    pub rich: Vec<RichNode>,
    #[serde(default)] pub inline_rich: Vec<RichNode>,
    #[serde(default)] pub note_rich: Vec<RichNode>,
    pub note_label: Option<String>,
    pub rendered: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RichBibliographyItem {
    pub key: String, pub rich: Vec<RichNode>, pub first_field: Vec<RichNode>,
}
impl RichBibliographyItem {
    /// Derive the complete recursive flow. A processor-provided first field
    /// owns the outer column split; its internal display layouts remain local
    /// to the appropriate cell instead of replacing or duplicating that label.
    pub fn layout(&self) -> ApiResult<Vec<RichLayoutNode>> {
        validate(&self.rich)?; validate(&self.first_field)?;
        if self.first_field.is_empty() { return layout(&self.rich); }
        // These optional field-root markers describe the already-known split.
        // Never erase a nested marker through a real block/indent boundary.
        Ok(vec![RichLayoutNode::Columns {
            left: project_layout(&self.first_field, Some(RichDisplay::LeftMargin)),
            right: project_layout(&self.rich, Some(RichDisplay::RightInline)),
        }])
    }

    /// Keep the historical simple first_field/body adapter, without making it
    /// the representation of complex display. The original tree remains
    /// untouched for multiple rows, extra blocks or nested column layouts.
    pub fn normalize_display_columns(&mut self) -> ApiResult<()> {
        fn nested_display(nodes: &[RichNode], outer: bool) -> bool {
            nodes.iter().any(|node| match node {
                RichNode::Group { children, display, .. } => {
                    (outer && display.is_some()) || nested_display(children, outer || display.is_some())
                }
                RichNode::Link { children, .. } => nested_display(children, outer),
                RichNode::Text { .. } => false,
            })
        }
        fn full_width(nodes: &mut [RichNode]) {
            for node in nodes {
                match node {
                    RichNode::Group { children, display, .. } => {
                        if matches!(display, Some(RichDisplay::LeftMargin | RichDisplay::RightInline)) { *display = Some(RichDisplay::Block); }
                        full_width(children);
                    }
                    RichNode::Link { children, .. } => full_width(children),
                    _ => {},
                }
            }
        }
        let projected = self.layout()?;
        if !self.first_field.is_empty() { return Ok(()); }
        if !layout_has_columns(&projected) { full_width(&mut self.rich); return Ok(()); }
        let parts = display_parts(&self.rich);
        let visible: Vec<_> = parts.iter().enumerate().filter(|(_, part)| !part.whitespace()).collect();
        if visible.len() != 2 || visible[0].1.display != Some(RichDisplay::LeftMargin)
            || visible[1].1.display != Some(RichDisplay::RightInline)
            || parts.iter().any(|part| nested_display(std::slice::from_ref(part.node), false)) {
            return Ok(());
        }
        let left = visible[0].0; let right = visible[1].0;
        let mut first = Vec::new(); let mut body = Vec::new();
        for (index, part) in parts.into_iter().enumerate() {
            let mut nodes = part.cloned_nodes();
            if index == left { consume_display(&mut nodes, RichDisplay::LeftMargin); }
            if index == right { consume_display(&mut nodes, RichDisplay::RightInline); }
            if index < right { first.extend(nodes); } else { body.extend(nodes); }
        }
        validate(&first)?; validate(&body)?;
        self.first_field = first; self.rich = body;
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RichBibliography {
    pub title: String,
    pub items: Vec<RichBibliographyItem>,
    pub hanging_indent: bool,
    pub line_spacing: u32,
    pub entry_spacing: u32,
    pub source_span: Option<SourceSpan>,
}
impl RichBibliography {
    pub fn normalize_layout(&mut self) -> ApiResult<()> {
        if self.line_spacing == 0 {
            return Err(rich_error("academic_rich_spacing").noted("field", "bibliography.line_spacing"));
        }
        for item in &mut self.items { item.normalize_display_columns()?; }
        Ok(())
    }
    /// CSL entry-spacing counts additional line heights after line-spacing.
    pub fn entry_gap_em(&self) -> u64 {
        u64::from(self.entry_spacing) * u64::from(self.line_spacing)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Projection {
    pub revision: String, pub nonce: String,
    pub source_bytes: usize, pub source_utf16: usize,
    pub citations: Vec<ProjectedCitation>,
    pub bibliography: Option<RichBibliography>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionSlot { Citation(usize), Bibliography }
impl Projection {
    pub fn new(source: &str) -> Self {
        Self { revision: crate::document_transaction::revision(source), nonce: uuid::Uuid::new_v4().simple().to_string(), source_bytes: source.len(), source_utf16: source.encode_utf16().count(), citations: Vec::new(), bibliography: None }
    }
    pub fn verify_source(&self, source: &str) -> ApiResult<()> {
        if self.source_bytes != source.len() || self.source_utf16 != source.encode_utf16().count()
            || self.revision != crate::document_transaction::revision(source)
            || self.nonce.len() != 32 || !self.nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(rich_error("academic_source_revision"));
        }
        Ok(())
    }
    pub fn token(&self, slot: ProjectionSlot) -> String {
        let id = match slot { ProjectionSlot::Citation(index) => format!("c{index}"), ProjectionSlot::Bibliography => "b".into() };
        format!("{SLOT_PREFIX}{} {id}\"/>", self.nonce)
    }
    pub fn is_slot_marker(value: &str) -> bool { value.trim_start().starts_with(SLOT_PREFIX) }
    pub fn slot(&self, value: &str) -> Option<ProjectionSlot> {
        let body = value.trim().strip_prefix(SLOT_PREFIX)?.strip_suffix("\"/>")?;
        let (nonce, id) = body.split_once(' ')?;
        if nonce != self.nonce { return None; }
        if id == "b" { return self.bibliography.as_ref().filter(|b| !b.items.is_empty()).map(|_| ProjectionSlot::Bibliography); }
        let index = id.strip_prefix('c')?.parse::<usize>().ok()?;
        (id == format!("c{index}") && self.citations.get(index)?.rendered).then_some(ProjectionSlot::Citation(index))
    }
    pub fn slot_count(&self) -> usize {
        self.citations.iter().filter(|c| c.rendered).count() + usize::from(self.bibliography.as_ref().is_some_and(|b| !b.items.is_empty()))
    }
    fn verify_citation(&self, citation: &ProjectedCitation, source: &str) -> ApiResult<()> {
        if !citation.source.span.matches(source) { return Err(rich_error("academic_source_range")); }
        let parent = &citation.source.span;
        for item in &citation.source.items {
            if !item.span.matches(source) || item.span.byte_from < parent.byte_from || item.span.byte_to > parent.byte_to {
                return Err(rich_error("academic_source_range"));
            }
            item.cite.validate()?;
            for span in item.fields.values() {
                if !span.matches(source) || span.byte_from < parent.byte_from || span.byte_to > parent.byte_to {
                    return Err(rich_error("academic_source_range"));
                }
            }
        }
        validate(&citation.rich)?;
        validate(&citation.inline_rich)?;
        validate(&citation.note_rich)?;
        Ok(())
    }
    fn apply_edits(source: &str, mut edits: Vec<(std::ops::Range<usize>, String)>) -> ApiResult<String> {
        edits.sort_by_key(|(range, _)| (range.start, range.end));
        let mut cursor = 0; let mut output = String::new();
        for (range, replacement) in edits {
            if range.start < cursor { return Err(rich_error("academic_source_overlap")); }
            output.push_str(source.get(cursor..range.start).ok_or_else(|| rich_error("academic_source_range"))?);
            output.push_str(&replacement); cursor = range.end;
            if output.len() > MAX_RICH_BYTES { return Err(rich_error("academic_rich_budget")); }
        }
        output.push_str(source.get(cursor..).ok_or_else(|| rich_error("academic_source_range"))?);
        if output.len() > MAX_RICH_BYTES { return Err(rich_error("academic_rich_budget")); }
        Ok(output)
    }

    /// A source-bound reader view: prose remains Markdown and only owned
    /// citation spans become safe rich HTML. No nonce token leaves this view.
    pub fn reader_content(&self, source: &str) -> ApiResult<String> {
        self.verify_source(source)?;
        let mut edits = Vec::new(); let mut notes = String::new();
        for (index, citation) in self.citations.iter().enumerate() {
            self.verify_citation(citation, source)?;
            if !citation.rendered { continue; }
            let mut body = if citation.note_label.is_some() && !citation.source.context.in_note {
                html(&citation.inline_rich)?
            } else { html(&citation.rich)? };
            if let Some(label) = citation.note_label.as_ref().filter(|_| !citation.source.context.in_note) {
                if label.is_empty() || label.len() > 128 || !label.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c,b'-'|b'_')) {
                    return Err(rich_error("academic_source_note"));
                }
                body.push_str(&format!("[^{label}]"));
                let note = if citation.note_rich.is_empty() { &citation.rich } else { &citation.note_rich };
                notes.push_str(&format!("\n\n[^{label}]: {}\n", html(note)?));
            }
            let span = &citation.source.span;
            let replacement = format!("<span class=\"readmd-citation\" data-readmd-citation-index=\"{index}\" data-readmd-source-from=\"{}\" data-readmd-source-to=\"{}\" data-readmd-source-revision=\"{}\">{body}</span>",span.from,span.to,escape(&self.revision));
            edits.push((span.byte_from..span.byte_to,replacement));
        }
        if !notes.is_empty() { edits.push((source.len()..source.len(),notes)); }
        if let Some(bib) = self.bibliography.as_ref().filter(|b| !b.items.is_empty()) {
            if bib.line_spacing==0 {
                return Err(rich_error("academic_rich_spacing").noted("field","bibliography.line_spacing"));
            }
            let line_class=if bib.line_spacing<=10{format!(" readmd-bibliography-line-{}",bib.line_spacing)}else{String::new()};
            let entry_class=if bib.entry_spacing<=10{format!(" readmd-bibliography-entry-{}",bib.entry_spacing)}else{String::new()};
            let layouts: Vec<_> = bib.items.iter().map(RichBibliographyItem::layout).collect::<ApiResult<_>>()?;
            let complex = layouts.iter().any(|flow| layout_requires_flow(flow));
            let complex_columns = layouts.iter().any(|flow| layout_has_columns(flow) && layout_requires_flow(flow));
            let complex_class = if complex { " readmd-bibliography-complex" } else { "" };
            let complex_marker = if complex { " data-csl-layout=\"complex\"" } else { "" };
            let mut content = format!("\n\n<section class=\"readmd-bibliography{line_class}{entry_class}{complex_class}\" data-csl-line-spacing=\"{}\" data-csl-entry-spacing=\"{}\"{complex_marker} style=\"line-height:{};\"><h2>{}</h2>",bib.line_spacing,bib.entry_spacing,bib.line_spacing,html(&[RichNode::text(&bib.title)])?);
            // Preserve the old one-grid markup for simple field pairs. A
            // recursive row needs local grids within its actual parent cells;
            // the reader supplies the bibliography-wide measured preference.
            let shared_columns = !complex_columns && bib.items.iter().any(|item| !item.first_field.is_empty());
            let shared_class = if shared_columns { " readmd-bibliography-shared-columns" } else { "" };
            let shared_css = if shared_columns { "display:grid;grid-template-columns:max-content minmax(0,1fr);column-gap:.75em;" } else { "" };
            content.push_str(&format!("<div class=\"readmd-bibliography-items{shared_class}\" style=\"{shared_css}\">"));
            for (index, item) in bib.items.iter().enumerate() {
                let flow = &layouts[index];
                let flow_entry = complex_columns || layout_requires_flow(flow);
                let hanging = if !flow_entry && bib.hanging_indent && item.first_field.is_empty() { "padding-left:2em;text-indent:-2em;" } else { "" };
                let hanging_class=if !flow_entry && bib.hanging_indent{" readmd-bibliography-hanging"}else{""};
                let flow_class=if flow_entry{" readmd-bibliography-flow-entry"}else{""};
                let columns_class=if flow_entry || item.first_field.is_empty(){""}else{" readmd-bibliography-has-first-field"};
                let columns=if flow_entry || item.first_field.is_empty(){""}else{"display:grid;grid-template-columns:subgrid;column-gap:.75em;padding-left:0;text-indent:0;"};
                let row=if shared_columns{"grid-column:1/-1;"}else{""};
                let gap=if index+1<bib.items.len(){bib.entry_gap_em()}else{0};
                content.push_str(&format!("<div class=\"readmd-bibliography-entry{hanging_class}{columns_class}{flow_class}\" style=\"{row}{hanging}{columns}margin-bottom:{gap}em;\">"));
                if flow_entry {
                    content.push_str(&format!("<span class=\"readmd-bibliography-content\">{}</span></div>",layout_html_with_hanging(flow,bib.hanging_indent)?));
                } else {
                    if !item.first_field.is_empty(){content.push_str(&format!("<span class=\"readmd-bibliography-first-field\">{}</span>",html(&item.first_field)?));}
                    content.push_str(&format!("<span class=\"readmd-bibliography-content\">{}</span></div>",html(&item.rich)?));
                }
            }
            content.push_str("</div></section>\n\n");
            let range = if let Some(span) = &bib.source_span {
                if !span.matches(source) { return Err(rich_error("academic_source_range")); }
                span.byte_from..span.byte_to
            } else { source.len()..source.len() };
            edits.push((range,content));
        }
        Self::apply_edits(source,edits)
    }
    /// Exact source edits before the normal Markdown transformations. The AST
    /// consumer must consume each authorized slot exactly once, then create
    /// typed notes/bibliography; no marker belongs in the user's source/output.
    pub fn instrument(&self, source: &str) -> ApiResult<String> {
        self.verify_source(source)?;
        let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        for (index, citation) in self.citations.iter().enumerate() {
            self.verify_citation(citation,source)?;
            if citation.rendered {
                edits.push((citation.source.span.byte_from..citation.source.span.byte_to, self.token(ProjectionSlot::Citation(index))));
            }
        }
        if let Some(bib) = self.bibliography.as_ref().filter(|b| !b.items.is_empty()) {
            for item in &bib.items { validate(&item.rich)?; validate(&item.first_field)?; }
            let token = format!("\n\n{}\n\n", self.token(ProjectionSlot::Bibliography));
            if let Some(span) = &bib.source_span {
                if !span.matches(source) { return Err(rich_error("academic_source_range")); }
                edits.push((span.byte_from..span.byte_to, token));
            } else { edits.push((source.len()..source.len(), token)); }
        }
        Self::apply_edits(source,edits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn display_part(display: RichDisplay, text: &str) -> RichNode {
        RichNode::Group { children: vec![RichNode::text(text)], display: Some(display), entry_index: Some(17) }
    }
    #[test]
    fn bibliography_display_pair_uses_first_field_without_losing_wrappers_format_links_or_metadata() {
        let left = RichNode::Group { children: vec![RichNode::Text { text: "[100]".into(), format: RichFormat { font_weight: RichFontWeight::Bold, ..Default::default() } }], display: Some(RichDisplay::LeftMargin), entry_index: Some(4) };
        let right = RichNode::Group { children: vec![RichNode::Link { href: "https://example.test/evidence".into(), children: vec![RichNode::Text { text: "Retained body".into(), format: RichFormat { font_style: RichFontStyle::Italic, ..Default::default() } }] }], display: Some(RichDisplay::RightInline), entry_index: Some(5) };
        let mut item = RichBibliographyItem { key: "stable-key".into(), first_field: vec![], rich: vec![RichNode::Group { children: vec![RichNode::text(" "), left, RichNode::text(" "), right, RichNode::text("\n")], display: None, entry_index: Some(7) }] };
        let original_text = plain(&item.rich);
        item.normalize_display_columns().unwrap();
        assert_eq!(item.key, "stable-key");
        assert_eq!(plain(&item.first_field), " [100] ");
        assert_eq!(plain(&item.rich), "Retained body\n");
        assert_eq!(format!("{}{}", plain(&item.first_field), plain(&item.rich)), original_text);
        let RichNode::Group { children, display: None, entry_index: Some(7) } = &item.first_field[1] else { panic!("transparent source metadata lost") };
        assert!(matches!(&children[0], RichNode::Group { display: None, entry_index: Some(4), .. }));
        let RichNode::Group { children, display: None, entry_index: Some(7) } = &item.rich[0] else { panic!("body source metadata lost") };
        assert!(matches!(&children[0], RichNode::Group { display: None, entry_index: Some(5), .. }));
        assert!(html(&item.first_field).unwrap().contains("font-weight:bold"));
        let body = html(&item.rich).unwrap(); assert!(body.contains("font-style:italic") && body.contains("https://example.test/evidence"));
        let normalized = item.clone(); item.normalize_display_columns().unwrap(); assert_eq!(item, normalized);
    }
    #[test]
    fn unpaired_display_columns_keep_full_width_block_semantics_and_source_metadata() {
        for nodes in [vec![display_part(RichDisplay::LeftMargin, "Left alone")], vec![display_part(RichDisplay::RightInline, "Right alone")], vec![display_part(RichDisplay::RightInline, "Earlier right"), display_part(RichDisplay::LeftMargin, "Later left")]] {
            let mut item = RichBibliographyItem { key: "orphan".into(), rich: nodes, first_field: vec![] };
            item.normalize_display_columns().unwrap(); assert!(item.first_field.is_empty());
            for node in &item.rich { assert!(matches!(node, RichNode::Group { display: Some(RichDisplay::Block), entry_index: Some(17), .. })); }
            let output = html(&item.rich).unwrap(); assert_eq!(output.matches("display:block;").count(), item.rich.len());
            assert!(!output.contains("display:inline"));
        }
        let mut existing = RichBibliographyItem { key: "processor-split".into(), first_field: vec![display_part(RichDisplay::LeftMargin, "[10]")], rich: vec![display_part(RichDisplay::RightInline, "Existing body")] };
        let original = existing.clone(); existing.normalize_display_columns().unwrap();
        assert_eq!(existing, original, "second-field-align already supplies the shared columns");
    }
    #[test]
    fn complex_display_columns_preserve_rows_without_flattening_or_duplicating_the_label() {
        let pair = || vec![display_part(RichDisplay::LeftMargin, "[1]"), display_part(RichDisplay::RightInline, "Body")];
        let mut extra = pair(); extra.push(display_part(RichDisplay::Block, "Extra block"));
        let mut twice = pair(); twice.extend(pair());
        let nested = vec![RichNode::Group { children: pair(), display: Some(RichDisplay::Block), entry_index: Some(8) }];
        let linked = vec![RichNode::Link { href: "https://example.test".into(), children: pair() }];
        for nodes in [extra, twice, nested, linked] {
            let mut item = RichBibliographyItem { key: "complex".into(), rich: nodes, first_field: vec![] };
            let original = item.clone();
            item.normalize_display_columns().unwrap();
            assert_eq!(item, original, "complex processor structure must remain intact");
            assert!(layout_has_columns(&item.layout().unwrap()));
        }
        let mut duplicate = RichBibliographyItem { key: "already-split".into(), rich: pair(), first_field: vec![RichNode::text("Existing label")] };
        let original = duplicate.clone(); duplicate.normalize_display_columns().unwrap();
        assert_eq!(duplicate, original);
        let mut nested_label = RichBibliographyItem { key: "label-pair".into(), rich: vec![RichNode::text("Body")], first_field: pair() };
        let original = nested_label.clone(); nested_label.normalize_display_columns().unwrap();
        assert_eq!(nested_label, original);
    }
    #[test]
    fn reader_bibliography_shares_columns_and_counts_only_inter_entry_line_heights() {
        let source = "Before\n\n<!--readmd:bibliography-->\n\nAfter\n";
        let start = source.find("<!--readmd:bibliography-->").unwrap();
        for line in [1, 3] { for entry in [0, 2] {
            let span = SourceSpan::new(source, start..start+"<!--readmd:bibliography-->".len()).unwrap();
            let mut bib = RichBibliography { title: "References".into(), hanging_indent: true, line_spacing: line, entry_spacing: entry, source_span: Some(span.clone()), items: vec![
                RichBibliographyItem { key: "short".into(), first_field: vec![RichNode::text("[9]")], rich: vec![RichNode::text("First body")] },
                RichBibliographyItem { key: "wide".into(), first_field: vec![RichNode::text("[100]")], rich: vec![RichNode::text("Second body")] },
                RichBibliographyItem { key: "full".into(), first_field: vec![], rich: vec![display_part(RichDisplay::RightInline, "Full width")] },
            ] };
            bib.normalize_layout().unwrap(); assert_eq!(bib.source_span, Some(span));
            let gap = u64::from(line)*u64::from(entry); assert_eq!(bib.entry_gap_em(), gap);
            let mut projection = Projection::new(source); projection.bibliography = Some(bib);
            let output = projection.reader_content(source).unwrap();
            assert_eq!(output.matches("readmd-bibliography-items readmd-bibliography-shared-columns").count(), 1);
            assert_eq!(output.matches("grid-template-columns:max-content minmax(0,1fr)").count(), 1);
            assert_eq!(output.matches("grid-template-columns:subgrid").count(), 2);
            assert_eq!(output.matches("grid-column:1/-1").count(), 3);
            assert_eq!(output.matches("padding-left:2em;text-indent:-2em;").count(), 1);
            assert_eq!(output.matches(&format!("margin-bottom:{gap}em;")).count(), if gap==0 {3} else {2});
            assert_eq!(output.matches("margin-bottom:0em;").count(), if gap==0 {3} else {1});
            assert!(output.contains(&format!("data-csl-line-spacing=\"{line}\" data-csl-entry-spacing=\"{entry}\"")));
            assert!(output.starts_with("Before\n\n") && output.ends_with("\n\nAfter\n"));
        }}
        let mut bib = RichBibliography { title: String::new(), items: vec![], hanging_indent: false, line_spacing: 11, entry_spacing: u32::MAX, source_span: None };
        bib.normalize_layout().unwrap(); assert_eq!(bib.entry_gap_em(), 11*u64::from(u32::MAX));
        bib.line_spacing=0; assert_eq!(bib.normalize_layout().unwrap_err().code, "academic_rich_spacing");
    }
    #[test]
    fn rich_output_retains_all_format_axes_links_and_display_without_executable_markup() {
        let format = RichFormat { font_style: RichFontStyle::Oblique, font_variant: RichFontVariant::SmallCaps, font_weight: RichFontWeight::Light, text_decoration: RichDecoration::Underline, vertical_align: RichVerticalAlign::Sup };
        let nodes = vec![RichNode::Group { children: vec![RichNode::Link { href: "https://example.test/\"<x>".into(), children: vec![RichNode::Text { text: "<script>& cited".into(), format }] }], display: Some(RichDisplay::Block), entry_index: Some(2) }];
        let output = html(&nodes).unwrap();
        for property in ["font-style:oblique", "font-variant:small-caps", "font-weight:300", "text-decoration:underline", "vertical-align:super", "display:block"] { assert!(output.contains(property)); }
        assert!(!output.contains("<script>")); assert!(output.contains("&lt;script&gt;&amp;")); assert!(output.contains("&quot;&lt;x&gt;"));
        let invalid = vec![RichNode::Link { href: "javascript:alert(1)".into(), children: vec![RichNode::text("x")] }];
        assert_eq!(html(&invalid).unwrap_err().code, "academic_rich_link");
        assert_eq!(serde_json::from_value::<Vec<RichNode>>(serde_json::to_value(&nodes).unwrap()).unwrap(), nodes);
    }
    #[test]
    fn rich_markdown_affixes_keep_emphasis_and_safe_links_and_reject_raw_markup() {
        let nodes = markdown_affix("see **also** and *passim* [evidence](https://example.test)").unwrap();
        let output = html(&nodes).unwrap();
        assert!(output.contains("font-weight:bold")); assert!(output.contains("font-style:italic")); assert!(output.contains("href=\"https://example.test\""));
        assert_eq!(plain(&nodes), "see also and passim evidence");
        assert!(markdown_affix("<script>unsafe()</script>").is_err());
    }
    #[test]
    fn locked_csl_affix_adapter_rejects_oblique_without_losing_independent_rich_display() {
        let nodes=vec![RichNode::Text{text:"oblique affix".into(),format:RichFormat{font_style:RichFontStyle::Oblique,..Default::default()}}];
        assert!(html(&nodes).unwrap().contains("font-style:oblique"));
        assert_eq!(to_elements(&nodes).unwrap_err().code,"academic_rich_unsupported");
        let cite=crate::academic::Cite{prefix_rich:Some(nodes),..crate::academic::Cite::new("a")};
        let error=cite.validate().unwrap_err();assert_eq!(error.code,"academic_rich_unsupported");
        let crate::error::ErrorShape::ApiCode{extras}=error.shape else{panic!("missing field")};
        assert!(extras.iter().any(|(key,value)|key=="field"&&value=="prefix_rich"));
        for font_style in [RichFontStyle::Normal,RichFontStyle::Italic]{
            let original=vec![RichNode::Text{text:"retained".into(),format:RichFormat{font_style,font_variant:RichFontVariant::SmallCaps,font_weight:RichFontWeight::Bold,text_decoration:RichDecoration::Underline,vertical_align:RichVerticalAlign::Sub}}];
            assert_eq!(from_elements(&to_elements(&original).unwrap()).unwrap(),original);
        }
    }
    #[test]
    fn citation_projection_requires_exact_utf16_source_and_owned_unique_slots() {
        let source = "😀 first [@a] and [@a]\r\n";
        let mut projection = Projection::new(source);
        for (byte_from, _) in source.match_indices("[@a]") {
            projection.citations.push(ProjectedCitation { source: SourceCitation { span: SourceSpan::new(source, byte_from..byte_from+4).unwrap(), items: Vec::new(), context: Default::default() }, rich: vec![RichNode::text("(A)")], inline_rich: Vec::new(), note_rich: Vec::new(), note_label: None, rendered: true });
        }
        assert_eq!(projection.citations[0].source.span.from, 9);
        let output = projection.instrument(source).unwrap();
        assert!(output.contains(&projection.token(ProjectionSlot::Citation(0)))); assert!(output.contains(&projection.token(ProjectionSlot::Citation(1))));
        assert_eq!(projection.slot_count(), 2);
        assert_eq!(projection.slot(&projection.token(ProjectionSlot::Citation(1))), Some(ProjectionSlot::Citation(1)));
        assert!(projection.slot(&projection.token(ProjectionSlot::Citation(9))).is_none());
        assert!(projection.slot(&projection.token(ProjectionSlot::Citation(0)).replace(&projection.nonce, "00000000000000000000000000000000")).is_none());
        assert!(projection.instrument(&source.replace("first", "changed")).is_err());
        projection.citations.push(projection.citations[0].clone()); assert_eq!(projection.instrument(source).unwrap_err().code, "academic_source_overlap");
    }
    #[test]
    fn citation_slot_tags_preserve_line_start_text_and_adjacent_citations() {
        use pulldown_cmark::{Event,Parser};
        for source in ["[@a] following text","[@a][@a] following text","[@a]\n\nfollowing text"] {
            let mut projection=Projection::new(source);
            for(byte_from,_)in source.match_indices("[@a]"){
                projection.citations.push(ProjectedCitation{source:SourceCitation{span:SourceSpan::new(source,byte_from..byte_from+4).unwrap(),items:Vec::new(),context:Default::default()},rich:vec![RichNode::text("A")],inline_rich:Vec::new(),note_rich:Vec::new(),note_label:None,rendered:true});
            }
            let instrumented=projection.instrument(source).unwrap();
            assert!(!instrumented.contains("<!--"));assert!(instrumented.contains("following text"));
            let events=Parser::new(&instrumented).collect::<Vec<_>>();
            assert!(events.iter().any(|event|matches!(event,Event::Text(text)if text.contains("following text"))),"{events:?}");
            let markers=events.iter().filter_map(|event|match event{Event::Html(value)|Event::InlineHtml(value)=>projection.slot(value),_=>None}).count();
            assert_eq!(markers,projection.slot_count(),"{events:?}");
        }
    }
    #[test]
    fn citation_reader_view_keeps_literal_text_split_notes_and_safe_bibliography_classes() {
        let source="😀 @a and note[^manual].\n\n[^manual]: Existing [@a].\n\n<!--readmd:bibliography-->\n";
        let mut projection=Projection::new(source);
        let first=source.find("@a").unwrap();let second=source.find("[@a]").unwrap();
        let author=vec![RichNode::Text{text:"A *literal* [name]".into(),format:RichFormat{font_variant:RichFontVariant::SmallCaps,..Default::default()}}];
        projection.citations.push(ProjectedCitation{source:SourceCitation{span:SourceSpan::new(source,first..first+2).unwrap(),items:Vec::new(),context:Default::default()},rich:vec![RichNode::text("full")],inline_rich:author,note_rich:vec![RichNode::text("1843, p. 17")],note_label:Some("readmd-citation-1".into()),rendered:true});
        projection.citations.push(ProjectedCitation{source:SourceCitation{span:SourceSpan::new(source,second..second+4).unwrap(),items:Vec::new(),context:crate::academic::CiteContext{note_number:Some(2),in_note:true,note_label:Some("manual".into())}},rich:vec![RichNode::text("Existing source")],inline_rich:Vec::new(),note_rich:Vec::new(),note_label:None,rendered:true});
        let marker=source.find("<!--readmd:bibliography-->").unwrap();
        projection.bibliography=Some(RichBibliography{title:"References <literal>".into(),items:vec![RichBibliographyItem{key:"a".into(),first_field:vec![RichNode::text("1.")],rich:vec![RichNode::text("Study <script>")]}],hanging_indent:true,line_spacing:2,entry_spacing:1,source_span:Some(SourceSpan::new(source,marker..marker+"<!--readmd:bibliography-->".len()).unwrap())});
        let content=projection.reader_content(source).unwrap();
        assert!(content.contains("readmd-citation\" data-readmd-citation-index=\"0\""));assert!(content.contains(&format!("data-readmd-source-revision=\"{}\"",projection.revision)));
        assert!(content.contains("readmd-csl-smallcaps")&&content.contains("&#42;literal&#42; &#91;name&#93;"));
        assert_eq!(content.matches("[^readmd-citation-1]:").count(),1);assert!(content.contains("[^readmd-citation-1]</span>"));assert!(content.contains("[^manual]: Existing <span"));
        assert!(content.contains("readmd-bibliography-line-2 readmd-bibliography-entry-1"));assert!(content.contains("readmd-bibliography-first-field")&&content.contains("readmd-bibliography-hanging"));
        assert!(content.contains("Study &lt;script&gt;"));assert!(!content.contains("data-slot=")&&!content.contains("<script>"));
        assert!(projection.reader_content(&source.replace("Existing","Changed")).is_err());
        projection.bibliography.as_mut().unwrap().line_spacing=11;assert!(projection.reader_content(source).unwrap().contains("data-csl-line-spacing=\"11\""));
    }
}

#[cfg(test)]
#[path = "publication_rich_display_tests.rs"]
mod display_tests;
