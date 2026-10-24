//! Standalone HTML footnotes, parsed before marked can treat their definitions
//! as link definitions. Only real Markdown events authorize a note or callout.
//! Definitions are emitted once, so trusted author-run nonce keys stay unique.
use pulldown_cmark::{Event, Parser, RefDefs, Tag, TagEnd};
use std::fmt::{self, Write};

const INVALID: &str = "publication_note_policy_invalid";
const BUDGET: &str = crate::publication_inline_text::BUDGET;
const MAX_DEFINITIONS: usize = 4096;
const MAX_REFERENCES: usize = 32768;
const MAX_HTML_BYTES: usize = 64 * 1024 * 1024;

pub(crate) const CSS: &str = ".readmd-footnotes { margin-top:2em; border-top:1px solid currentColor; padding-top:.5em; } .readmd-footnotes > ol { padding-inline-start:1.8em; } .readmd-footnotes li { margin:.5em 0; } .readmd-footnote-backlinks { white-space:nowrap; } .readmd-footnote-ref { line-height:0; }";

struct Definition<'a> {
    events: Vec<Event<'a>>,
}

struct BoundedHtml {
    value: String,
    limit: usize,
}

impl fmt::Write for BoundedHtml {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.value.len().checked_add(value.len()).is_none_or(|size| size > self.limit)
            || crate::speech::cancelled()
        {
            return Err(fmt::Error);
        }
        self.value.push_str(value);
        Ok(())
    }
}

struct Notes {
    prefix: String,
    numbers: Vec<usize>,
    order: Vec<usize>,
    references: Vec<Vec<String>>,
    reference_count: usize,
    max_references: usize,
}

impl Notes {
    fn number(&mut self, index: usize) -> usize {
        if self.numbers[index] == 0 {
            self.order.push(index);
            self.numbers[index] = self.order.len();
        }
        self.numbers[index]
    }

    fn reference(&mut self, index: usize) -> Result<String, String> {
        if self.reference_count >= self.max_references {
            return Err(BUDGET.into());
        }
        self.reference_count += 1;
        let number = self.number(index);
        let target = format!("{}-{number}", self.prefix);
        let id = format!("{target}-ref-{}", self.references[index].len() + 1);
        let html = format!("<sup class=\"readmd-footnote-ref\"><a id=\"{id}\" href=\"#{target}\" role=\"doc-noteref\">{number}</a></sup>");
        self.references[index].push(id);
        Ok(html)
    }
}

fn render_events<'a>(
    events: Vec<Event<'a>>,
    definitions: &RefDefs<'_>,
    notes: &mut Notes,
    limit: usize,
) -> Result<String, String> {
    let mut rendered = Vec::with_capacity(events.len());
    for event in events {
        if crate::speech::cancelled() { return Err("cancelled".into()); }
        rendered.push(match event {
            Event::FootnoteReference(label) => {
                let index = definitions.get(label.as_ref()).and_then(|definition|definition.dest.strip_prefix("readmd-note:")).and_then(|value|value.parse::<usize>().ok());
                if let Some(index) = index.filter(|index|*index < notes.numbers.len()) {
                    Event::InlineHtml(notes.reference(index)?.into())
                } else {
                    // Never invent a binding for an unresolved label.
                    Event::Text(format!("[^{label}]").into())
                }
            }
            // Match the legacy marked {breaks:true} setting. Code blocks have
            // Text events, so literal line breaks in examples stay untouched.
            Event::SoftBreak => Event::HardBreak,
            // Keep ordinary TeX on the existing MathJax path. Text events let
            // the renderer escape HTML without reinterpreting TeX as markup.
            Event::InlineMath(value) => Event::Text(format!("${value}$").into()),
            Event::DisplayMath(value) => Event::Text(format!("$${value}$$").into()),
            event => event,
        });
    }
    let mut output = BoundedHtml { value: String::new(), limit };
    pulldown_cmark::html::write_html_fmt(&mut output, rendered.into_iter())
        .map_err(|_| if crate::speech::cancelled() { "cancelled" } else { BUDGET }.to_string())?;
    Ok(output.value)
}

/// `None` retains the original standalone HTML template byte for byte. This
/// includes references and definitions that occur only in code or raw HTML.
pub(crate) fn render(source: &str) -> Result<Option<String>, String> {
    render_bounded(source, MAX_DEFINITIONS, MAX_REFERENCES, MAX_HTML_BYTES)
}

fn render_bounded(source: &str, max_definitions: usize, max_references: usize, max_html: usize) -> Result<Option<String>, String> {
    if !source.contains("[^") { return Ok(None); }
    let mut body = Vec::new();
    let mut definitions = Vec::<Definition<'_>>::new();
    let mut labels = String::new();
    let mut active = Vec::new();
    for (event, _) in crate::publication_inline_text::markdown_events(source)? {
        match event {
            Event::Start(Tag::FootnoteDefinition(label)) => {
                if definitions.len() >= max_definitions { return Err(BUDGET.into()); }
                let index = definitions.len();
                // A duplicate definition keeps its own visible body. Only the
                // parser's first binding receives references; no body is lost.
                // The parser's public reference map supplies exactly the same
                // Unicode label folding; an ASCII lowercase approximation does
                // not bind labels such as STRASSE and Straße correctly. Labels
                // already come from parsed bracket syntax, not arbitrary text.
                writeln!(labels, "[{label}]: readmd-note:{index}").map_err(|_|INVALID)?;
                definitions.push(Definition { events: Vec::new() });
                active.push(index);
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                if active.pop().is_none() { return Err(INVALID.into()); }
            }
            event => {
                if let Some(&index) = active.last() { definitions[index].events.push(event); }
                else { body.push(event); }
            }
        }
    }
    if !active.is_empty() { return Err(INVALID.into()); }
    if definitions.is_empty() { return Ok(None); }
    let label_parser = Parser::new(&labels);
    let labels = label_parser.reference_definitions();
    let digest = crate::document_transaction::revision(source);
    let mut notes = Notes {
        prefix: format!("readmd-fn-{}", &digest[..16]),
        numbers: vec![0; definitions.len()],
        order: Vec::with_capacity(definitions.len()),
        references: vec![Vec::new(); definitions.len()],
        reference_count: 0,
        max_references,
    };
    let body = render_events(body, labels, &mut notes, max_html)?;
    // Preserve unreferenced definitions too, including their author runs.
    // Numbers used in the main text come first; remaining notes keep source order.
    for index in 0..definitions.len() { notes.number(index); }
    let mut note_html = Vec::with_capacity(definitions.len());
    let mut used_bytes = body.len();
    for index in notes.order.clone() {
        let html = render_events(std::mem::take(&mut definitions[index].events), labels, &mut notes, max_html.saturating_sub(used_bytes))?;
        used_bytes = used_bytes.checked_add(html.len()).ok_or(BUDGET)?;
        note_html.push((index, html));
    }
    // References inside definitions are now known, including forward and
    // cyclic references. Append every backlink without recursively copying a note.
    let mut output = BoundedHtml { value: body, limit: max_html };
    output.write_str("<section class=\"readmd-footnotes\" role=\"doc-endnotes\"><ol>\n").map_err(|_|BUDGET)?;
    for (index, html) in note_html {
        let number = notes.numbers[index];
        write!(output, "<li id=\"{}-{number}\">{html}", notes.prefix).map_err(|_|BUDGET)?;
        if !notes.references[index].is_empty() {
            output.write_str("<span class=\"readmd-footnote-backlinks\">").map_err(|_|BUDGET)?;
            for (reference, id) in notes.references[index].iter().enumerate() {
                write!(output, " <a href=\"#{id}\" role=\"doc-backlink\" aria-label=\"↩ {}\">↩</a>", reference + 1).map_err(|_|BUDGET)?;
            }
            output.write_str("</span>").map_err(|_|BUDGET)?;
        }
        output.write_str("</li>\n").map_err(|_|BUDGET)?;
    }
    output.write_str("</ol></section>\n").map_err(|_|BUDGET)?;
    if crate::speech::cancelled() { return Err("cancelled".into()); }
    Ok(Some(output.value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_references_render_one_complete_note_with_all_backlinks() {
        let source = "First[^f], again[^f]. End.\n\n[^f]: **VISIBLE_NOTE** [evidence](https://example.test/note)\n\n    Second line.\n";
        let html = render(source).unwrap().unwrap();
        assert_eq!(html.matches("VISIBLE_NOTE").count(), 1);
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), 2);
        assert_eq!(html.matches("role=\"doc-backlink\"").count(), 2);
        assert_eq!(html.matches("<li id=").count(), 1);
        assert!(html.contains("<strong>VISIBLE_NOTE</strong>"));
        assert!(html.contains("href=\"https://example.test/note\""));
        assert!(html.contains("Second line.") && html.contains("End."));
        let prefix = format!("readmd-fn-{}-1", &crate::document_transaction::revision(source)[..16]);
        for reference in 1..=2 { assert!(html.contains(&format!("href=\"#{prefix}-ref-{reference}\""))); }
    }

    #[test]
    fn authored_first_run_math_links_and_nonce_keys_survive_exactly_once() {
        let source = "Body[^f].\n\n[^f]: <span data-readmd-text=\"v1\" data-size-pt=\"30\">NOTE_BIG <span data-readmd-text=\"v1\" data-size-pt=\"base\">NOTE_RESET</span></span> $a<b$ [NOTE_LINK](https://example.test/)\n\n<!-- SOURCE_COMMENT -->\n";
        let view = crate::publication_inline_view::project(source, &serde_json::json!({"footnotes":{"size":10,"sizeCs":10}})).unwrap();
        assert!(!view.runs.is_empty());
        let html = render(&view.content).unwrap().unwrap();
        for marker in ["NOTE_BIG", "NOTE_RESET", "NOTE_LINK", "SOURCE_COMMENT"] { assert_eq!(html.matches(marker).count(), 1, "{marker}: {html}"); }
        assert!(html.contains("$a&lt;b$") && html.contains("href=\"https://example.test/\""));
        for run in &view.runs { assert_eq!(html.matches(&format!("data-readmd-author-run=\"{}\"", run.key)).count(), 1); }
        assert!(view.runs.iter().any(|run|run.size==Some(30.)));
        assert!(view.runs.iter().any(|run|run.block_base && run.base==Some(10.)));
    }

    #[test]
    fn examples_unknown_references_and_ordinary_documents_keep_legacy_path() {
        for source in ["# Ordinary\n\nText $x$.", "`[^f]: EXAMPLE` and `[^f]`\n\n```md\n[^f]: CODE_NOTE\n```", "<!-- [^f]: COMMENT_NOTE -->\n\nMissing[^missing].", "<pre>[^f]: HTML_EXAMPLE</pre>\n"] {
            assert!(render(source).unwrap().is_none(), "{source}");
        }
    }

    #[test]
    fn actual_notes_do_not_activate_examples_and_keep_softbreaks_and_math() {
        let source = "Body[^f].\nNext line.\n\n`[^f]`\n\n```md\n[^fake]: CODE_NOTE\n```\n\n<!-- [^f] stays a comment -->\n\n[^f]: $a<b$\n\n    $$c<d$$\n";
        let html = render(source).unwrap().unwrap();
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), 1);
        assert!(html.contains("<br />\nNext line."));
        assert!(html.contains("<code>[^f]</code>") && html.contains("[^fake]: CODE_NOTE"));
        assert!(html.contains("<!-- [^f] stays a comment -->"));
        assert!(html.contains("$a&lt;b$") && html.contains("$$c&lt;d$$"));
    }

    #[test]
    fn forward_cyclic_and_unused_definitions_are_bounded_and_visible() {
        let source = "Body[^甲].\n\n[^甲]: FIRST_NOTE and other[^2].\n\n[^2]: SECOND_NOTE and back[^甲].\n\n[^unused]: UNUSED_NOTE\n";
        let html = render(source).unwrap().unwrap();
        for marker in ["FIRST_NOTE", "SECOND_NOTE", "UNUSED_NOTE"] { assert_eq!(html.matches(marker).count(), 1); }
        assert_eq!(html.matches("<li id=").count(), 3);
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), 3);
        assert_eq!(html.matches("role=\"doc-backlink\"").count(), 3);
        assert_eq!(render_bounded(source, 2, 10, 10000).unwrap_err(), BUDGET);
        assert_eq!(render_bounded(source, 10, 2, 10000).unwrap_err(), BUDGET);
        assert_eq!(render_bounded(source, 10, 10, 20).unwrap_err(), BUDGET);
    }

    #[test]
    fn unicode_casefolded_labels_and_duplicate_definitions_keep_visible_content() {
        let source = "Body[^STRASSE], repeated[^Straße].\n\n[^Straße]: FIRST_BODY\n\n[^Straße]: SECOND_BODY\n";
        let html = render(source).unwrap().unwrap();
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), 2);
        assert_eq!(html.matches("role=\"doc-backlink\"").count(), 2);
        for marker in ["FIRST_BODY", "SECOND_BODY"] { assert_eq!(html.matches(marker).count(), 1); }
        assert_eq!(html.matches("<li id=").count(), 2);
        assert!(!html.contains("[^STRASSE]"));
    }

    #[test]
    fn notes_inside_quotes_and_lists_retain_surrounding_visible_blocks() {
        let source = "> QUOTED_BODY[^q]\n>\n> [^q]: QUOTED_NOTE\n\n- LIST_BODY[^l]\n\n  [^l]: LIST_NOTE\n\nAFTER_CONTAINERS\n";
        let html = render(source).unwrap().unwrap();
        for marker in ["QUOTED_BODY", "QUOTED_NOTE", "LIST_BODY", "LIST_NOTE", "AFTER_CONTAINERS"] { assert_eq!(html.matches(marker).count(), 1); }
        assert!(html.contains("<blockquote>"));
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), 2);
        assert_eq!(html.matches("role=\"doc-backlink\"").count(), 2);
    }

    #[test]
    fn lowered_math_and_gfm_structures_stay_in_the_real_note_html() {
        let source = "Body[^f].\n\n| A | B |\n| - | - |\n| C | D |\n\n[^f]: :math[frac(a,b)] and **NOTE**\n\n    - LIST_ITEM\n\n        `CODE_LITERAL`\n";
        let lowered = crate::html_math::lower_dsl(source, 12.).unwrap();
        assert_eq!(lowered.formulas, 1);
        let html = render(&lowered.markdown).unwrap().unwrap();
        for fragment in ["<table>", "<svg ", "<strong>NOTE</strong>", "<code>CODE_LITERAL</code>"] { assert!(html.contains(fragment), "{fragment}: {html}"); }
        assert_eq!(html.matches("<svg ").count(), 1);
        // This is a loose list: its marker paragraph and following code
        // paragraph belong to the same li. Whitespace/tight-list serialization
        // is not part of the visible-content contract.
        let tree=crate::math_document::parse_xml(&html).unwrap();
        let mut pending=vec![(&tree,false)];let mut items=Vec::new();
        while let Some((node,in_list))=pending.pop(){
            if in_list&&node.local()=="li"&&node.children().any(|child|child.local()=="p"&&child.text()=="LIST_ITEM"){items.push(node);}
            pending.extend(node.children().map(|child|(child,node.local()=="ul")));
        }
        assert_eq!(items.len(),1,"one real unordered list item must own LIST_ITEM");
        let paragraphs=items[0].children().filter(|node|node.local()=="p").collect::<Vec<_>>();
        assert_eq!(paragraphs.len(),2);assert_eq!(paragraphs[0].text(),"LIST_ITEM");
        assert_eq!(paragraphs[1].text(),"CODE_LITERAL");
        let code=paragraphs[1].children().filter(|node|node.local()=="code").collect::<Vec<_>>();
        assert_eq!(code.len(),1);assert_eq!(code[0].text(),"CODE_LITERAL");
    }

    fn script_json(html: &str, id: &str) -> String {
        let start = format!("<script type=\"application/json\" id=\"{id}\">");
        let raw = html.split_once(&start).unwrap().1.split_once("</script>").unwrap().0;
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn exporter_injects_complete_notes_and_applies_nonce_before_mathjax() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("Notes.html");
        let source = "Body[^f], repeated[^f].\n\n[^f]: <span data-readmd-text=\"v1\" data-size-pt=\"30\">ACTUAL_NOTE</span> $x$\n";
        crate::mdexport::export_html(source, "", out.to_str().unwrap(), &serde_json::json!({}), "Notes", dir.path()).unwrap();
        let html = std::fs::read_to_string(out).unwrap();
        let body = script_json(&html, "readmd-footnote-html");
        assert_eq!(body.matches("ACTUAL_NOTE").count(), 1);
        assert_eq!(body.matches("role=\"doc-noteref\"").count(), 2);
        assert_eq!(body.matches("role=\"doc-backlink\"").count(), 2);
        assert!(body.contains("data-readmd-author-run="));
        let apply = html.find("window.ReadMDAuthorText.apply(root,").unwrap();
        let typeset = html.find("MathJax.typesetPromise()").unwrap();
        assert!(apply < typeset);
        assert!(html.contains("JSON.parse(document.getElementById('readmd-footnote-html').textContent)"));
        assert!(!html.contains("try { html = marked.parse(md,"));
    }

    #[test]
    fn exported_note_payload_is_json_text_and_cannot_close_its_script() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("Literal.html");
        let source = "Body[^f].\n\n[^f]: `</script><script>DO_NOT_EXECUTE</script>` and $a<b$\n";
        crate::mdexport::export_html(source, "", out.to_str().unwrap(), &serde_json::json!({}), "Notes", dir.path()).unwrap();
        let html = std::fs::read_to_string(out).unwrap();
        let body = script_json(&html, "readmd-footnote-html");
        assert!(body.contains("<code>&lt;/script&gt;&lt;script&gt;DO_NOT_EXECUTE&lt;/script&gt;</code>"));
        assert!(body.contains("$a&lt;b$"));
        let raw = html.split_once("id=\"readmd-footnote-html\">").unwrap().1.split_once("</script>").unwrap().0;
        assert!(!raw.contains('<'));
        assert!(!body.contains("<script>"));
    }

    #[test]
    fn exporter_rejects_invalid_note_style_without_replacing_existing_output() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("Protected.html");
        std::fs::write(&out, b"KEEP_ORIGINAL").unwrap();
        let source = "Body[^f].\n\n[^f]: <span data-readmd-text=\"v1\" data-size-pt=\"30;position:fixed\">BAD</span>\n";
        assert_eq!(crate::mdexport::export_html(source, "", out.to_str().unwrap(), &serde_json::json!({}), "Notes", dir.path()).unwrap_err(), crate::publication_inline_text::INVALID);
        assert_eq!(std::fs::read(&out).unwrap(), b"KEEP_ORIGINAL");
    }
}
