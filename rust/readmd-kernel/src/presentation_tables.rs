//! Author table geometry inside Reveal's existing Markdown slide structure.
use crate::html_footnotes::tables::project_with_spans;
use crate::table_geometry::{self, TableTracks};
use pulldown_cmark::Event;
use serde_json::Value;
use std::borrow::Cow;
use std::fmt::Write;
use std::ops::Range;

const TABLE_WIDTH: f64 = 900.0;
const MARKER_SUFFIX: char = 'Z';

pub(super) struct Projected<'a> {
    pub(super) source: Cow<'a, str>,
    marker: String,
    tables: Vec<String>,
}

pub(super) fn project(source: &str) -> Result<Projected<'_>, String> {
    let mut result = Projected {
        source: Cow::Borrowed(source),
        marker: String::new(),
        tables: Vec::new(),
    };
    if !table_geometry::source_candidate(source) {
        return Ok(result);
    }
    table_geometry::validate_document(&crate::md_ast::parse(source), true)?;
    let events = crate::publication_inline_text::markdown_events(source)?;
    let (events, spans) = project_with_spans(source, events, table_open)?;
    if spans.is_empty() {
        return Ok(result);
    }
    let marker = format!("READMDPRESENTATIONTABLE{}X", uuid::Uuid::new_v4().simple());
    let mut markdown = String::with_capacity(source.len());
    let mut cursor = 0;
    for (index, span) in spans.into_iter().enumerate() {
        crate::publication_budget::check()?;
        let mut html = String::new();
        pulldown_cmark::html::push_html(
            &mut html,
            events[span.events]
                .iter()
                .cloned()
                .map(|event| match event {
                    // Leave TeX on Reveal's existing KaTeX path. The legacy Markdown
                    // slide renderer has no footnote numbering, so keep the label.
                    Event::InlineMath(text) => {
                        Event::Text(format!("{delimiter}{text}{delimiter}", delimiter = "$").into())
                    }
                    Event::DisplayMath(text) => Event::Text(
                        format!("{delimiter}{text}{delimiter}", delimiter = "$$").into(),
                    ),
                    Event::FootnoteReference(label) => Event::Text(format!("[^{label}]").into()),
                    event => event,
                }),
        );
        result.tables.push(
            super::sanitize_slide_html(&html)
                .replace('\r', "&#13;")
                .replace('\n', "&#10;"),
        );
        markdown.push_str(&source[cursor..span.source.start]);
        let _ = write!(markdown, "{marker}{index}{MARKER_SUFFIX}");
        let original = &source[span.source.clone()];
        markdown.push_str(&original[original.trim_end_matches(['\r', '\n']).len()..]);
        cursor = span.source.end;
    }
    markdown.push_str(&source[cursor..]);
    result.source = Cow::Owned(markdown);
    result.marker = marker;
    Ok(result)
}

impl Projected<'_> {
    pub(super) fn restore<'a>(&self, text: &'a str, inside_textarea: bool) -> Cow<'a, str> {
        if self.tables.is_empty() || !text.contains(&self.marker) {
            return Cow::Borrowed(text);
        }
        let mut pieces = text.split(&self.marker);
        let mut html = pieces.next().unwrap_or_default().to_owned();
        for piece in pieces {
            let table = piece.split_once(MARKER_SUFFIX).and_then(|(index, tail)| {
                Some((self.tables.get(index.parse::<usize>().ok()?)?, tail))
            });
            if let Some((table, tail)) = table {
                if inside_textarea {
                    // Reading the Markdown template's textContent consumes a
                    // layer of entity escapes before Reveal sees the table.
                    html.push_str(&table.replace('&', "&amp;"));
                } else {
                    html.push_str(table);
                }
                html.push_str(tail);
            } else {
                html.push_str(&self.marker);
                html.push_str(piece);
            }
        }
        Cow::Owned(html)
    }
}

fn table_open(tracks: &TableTracks) -> String {
    if !tracks.has_column_widths() {
        return "<table class=\"readmd-table-geometry\"><thead>".to_owned();
    }
    // Reveal's coordinate system is in CSS pixels, with the established
    // table width capped at 900. Use the same fixed/flexible tracks as the
    // paged backends, expressing ratios for a narrower actual viewport.
    let available = TABLE_WIDTH * table_geometry::PT_PER_CSS_PX;
    let widths = tracks.column_widths_pt(available);
    let total = widths.iter().sum::<f64>();
    let px = total / table_geometry::PT_PER_CSS_PX;
    let mut html = format!(
        "<table class=\"readmd-table-geometry readmd-table-geometry--columns\" style=\"width:{px:.4}px;max-width:100%\"><colgroup>"
    );
    for width in widths {
        let percent = width * 100.0 / total;
        let _ = write!(html, "<col style=\"width:{percent:.4}%\" />");
    }
    html.push_str("</colgroup><thead>");
    html
}

pub(super) fn style(options: &Value) -> String {
    let mut css = String::from(include_str!("../../../assets/css/table-geometry.css"));
    // A tall table is one intact HTML object. Flex centering can otherwise
    // place its top rows above scroll origin in a constrained slide.
    css.push_str("\n.reveal .slides section.readmd-table-slide{justify-content:flex-start!important;justify-content:safe center!important}\n\
        .reveal .slides section.readmd-table-slide>*{flex-shrink:0}\n");
    let style = crate::export_styles::sanitize(options);
    css.push_str(&crate::export_styles::scientific_table_css(
        &style,
        ".reveal .slides",
    ));
    if crate::export_styles::three_line_table(&style) {
        let defaults = crate::export_styles::default_style();
        if style["typography"]["color"] == defaults["typography"]["color"] {
            css.push_str(".reveal .slides th,.reveal .slides td{color:inherit}");
        }
        if style["table"]["borderColor"] == defaults["table"]["borderColor"] {
            css.push_str(
                ".reveal .slides table,.reveal .slides thead th{border-color:currentColor}",
            );
        }
    }
    css
}

/// Keep a parsed Markdown image's alternative text and inline title while
/// embedding its destination. Reference-form images become equivalent inline
/// images without changing the surrounding cell or consuming a neighboring link.
pub(super) fn embed_image(
    source: &str,
    range: Range<usize>,
    url: &str,
    data: &str,
) -> Result<String, String> {
    if let Some(destination) =
        crate::document_refactor_links::inline_destination(source, range.clone(), url)
    {
        return Ok(format!(
            "{}{data}{}",
            &source[range.start..destination.start],
            &source[destination.end..range.end]
        ));
    }
    let raw = &source[range];
    let alt = raw
        .strip_prefix("![")
        .ok_or("export_snapshot_resource_path")?;
    let mut level = 1usize;
    let mut escaped = false;
    for (offset, character) in alt.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '[' {
            level += 1;
        } else if character == ']' {
            level -= 1;
            if level == 0 {
                return Ok(format!("![{}]({data})", &alt[..offset]));
            }
        }
    }
    Err("export_snapshot_resource_path".into())
}

#[cfg(test)]
#[path = "presentation_table_tests.rs"]
mod tests;
