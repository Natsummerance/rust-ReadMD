use super::*;
use serde_json::json;

fn source(pt: f64, body: &str) -> String {
    format!("\u{feff}<!-- readmd:layout v1 {{\"id\":\"reader-track\",\"text\":{{\"trackingPt\":{pt}}}}} -->\r\n\r\n{body}\r\n")
}

#[test]
fn p04_tracking_reader_is_a_local_only_consumer_not_an_export_permission() {
    for pt in [-5., -0.025, 0., 0.025, 20.] {
        let source = source(pt, "Original 中文 e\u{301} 👩\u{200d}💻 **bold** `code` $x^2$ [@sample].");
        assert_eq!(crate::publication_typography::validate_reader_view(&source, &json!({})), Ok(()));
        assert_eq!(crate::publication_typography::validate_export("docx", &source, &json!({})), Ok(()));
        assert_eq!(crate::publication_typography::validate_preview(&source, &json!({})), Err(UNSUPPORTED));
        for (format, engine) in [("html", "rust"), ("epub", "rust"), ("tex", "xelatex"), ("pdf", "rust"), ("pdf", "typst"), ("pdf", "xelatex")] {
            let options = json!({"pdf":{"engine":engine}});
            assert_eq!(crate::publication_typography::validate_export(format, &source, &options), Err(UNSUPPORTED));
        }
        let options = json!({"typography":{"trackingPt":pt}});
        assert_eq!(crate::publication_typography::validate_reader_view("Body.", &options), Err(UNSUPPORTED));
    }
}

#[test]
fn p04_tracking_reader_keeps_exact_source_and_revision_specific_css() {
    for pt in [-0.025, 0., 1.25] {
        let body = "Original 中文 e\u{301} 👩\u{200d}💻 **bold** `code` $x^2$ [@sample].";
        let source = source(pt, body);
        let snapshot = source.clone();
        let before = crate::document_transaction::revision(&source);
        let (view, rules) = crate::publication_blocks::view(&source).unwrap();
        assert!(view.starts_with('\u{feff}'));
        assert!(view.ends_with(&format!("\r\n\r\n{body}\r\n")));
        assert!(!view.contains("trackingPt"));
        assert_eq!(rules.as_object().unwrap().len(), 1);
        assert_eq!(rules.as_object().unwrap().values().next().unwrap()["letter-spacing"], format!("{pt}pt"));
        assert_eq!(source, snapshot);
        assert_eq!(crate::document_transaction::revision(&source), before);
    }
    let (view, rules) = crate::publication_blocks::view("Body `code` $x^2$.\r\n").unwrap();
    assert_eq!(view, "Body `code` $x^2$.\r\n");
    assert!(rules.as_object().unwrap().is_empty());
}

#[test]
fn p04_tracking_reader_retains_shape_scope_and_physical_spacing_guards() {
    for body in ["# Heading", "![only](image.png)", "$$x^2$$"] {
        assert!(crate::publication_typography::validate_reader_view(&source(1., body), &json!({})).is_err());
    }
    for value in [json!(null), json!("1"), json!(20.001), json!(-5.001)] {
        let source = format!("<!-- readmd:layout v1 {} -->\n\nBody.\n", json!({"id":"bad","text":{"trackingPt":value}}));
        assert_eq!(crate::publication_typography::validate_reader_view(&source, &json!({})), Err(INVALID));
    }
    let typed = "<!-- readmd:layout v1 {\"id\":\"typed\",\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":18}}} -->\n\nBody.\n";
    assert_eq!(crate::publication_typography::validate_reader_view(typed, &json!({})), Err(crate::publication_typography::UNSUPPORTED));
    use crate::md_ast::{Block, Inline};
    let tracked = |block| Block::Styled { attributes:json!({"text":{"trackingPt":1}}), block:Box::new(block) };
    let paragraph = || Block::Paragraph(vec![Inline::Text("Body.".into())]);
    let mut doc = crate::md_ast::parse("Body.");
    doc.blocks = vec![tracked(tracked(paragraph()))];
    assert_eq!(validate_document_consumer(&doc, &json!({}), Consumer::Reader), Err(UNSUPPORTED));
    doc.blocks = vec![Block::Quote(vec![tracked(paragraph())])];
    assert_eq!(validate_document_consumer(&doc, &json!({}), Consumer::Reader), Err(UNSUPPORTED));
    doc.blocks = vec![Block::FootnoteDef { label:"note".into(), blocks:vec![tracked(paragraph())] }];
    assert_eq!(validate_document_consumer(&doc, &json!({}), Consumer::Reader), Err(UNSUPPORTED));
}

#[test]
fn p04_tracking_reader_cannot_bypass_escaped_field_or_invalidate_later_export() {
    let source = "<!-- readmd:layout v1 {\"id\":\"escaped\",\"text\":{\"tracking\\u0050t\":1}} -->\r\n\r\nBody.\r\n";
    let before = source.as_bytes().to_vec();
    assert_eq!(crate::publication_typography::validate_reader_view(source, &json!({})), Ok(()));
    assert!(crate::publication_blocks::view(source).is_ok());
    assert_eq!(validate_consumer(source, &json!({}), false), Err(UNSUPPORTED));
    assert_eq!(crate::publication_typography::validate_preview(source, &json!({})), Err(UNSUPPORTED));
    assert_eq!(source.as_bytes(), before);
}
