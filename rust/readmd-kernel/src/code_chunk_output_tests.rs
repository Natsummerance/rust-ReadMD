use super::*;
fn projected(source: &str, line: usize, code: &str, output: &str) -> Result<String, &'static str> {
    let head =
        json!({"document_id":"owned-doc","generation":1,"review_revision":0,"content":source});
    let selected = target(&head, line, code)?;
    let edits = plan(source, &selected, output)?;
    Ok(crate::document_transaction::apply(
        source,
        &crate::document_transaction::ContentTransaction {
            base_revision: revision(source),
            origin: "editor".into(),
            edits,
        },
    )?
    .content)
}
#[test]
fn fenced_geometry_handles_long_backticks_tildes_unicode_and_eof() {
    assert_eq!(projected("😀\n\n````text\n```\n````",3,"```","42").unwrap(), "😀\n\n````text\n```\n````\n\n<!-- code_chunk_output -->\n\n42\n\n<!-- /code_chunk_output -->\n");
    assert!(
        projected("~~~sql\nSELECT 42;\n~~~\n", 1, "SELECT 42;", "42")
            .unwrap()
            .ends_with("<!-- /code_chunk_output -->\n")
    );
    assert_eq!(
        projected("```sql\nSELECT 42;", 1, "SELECT 42;", "42").unwrap_err(),
        CONFLICT
    );
}
#[test]
fn adjacent_output_replaces_only_its_complete_block_and_identical_is_noop() {
    let source="```sql\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\n\nold\n\n<!-- /code_chunk_output -->\n\nTail.\n";
    let expected="```sql\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\n\n42\n\n<!-- /code_chunk_output -->\n\nTail.\n";
    assert_eq!(projected(source, 1, "SELECT 42;", "42").unwrap(), expected);
    assert_eq!(
        projected(expected, 1, "SELECT 42;", "42").unwrap(),
        expected
    );
    assert_eq!(
        projected(
            "```sql\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\nold\n",
            1,
            "SELECT 42;",
            "42"
        )
        .unwrap_err(),
        CONFLICT
    );
}
#[test]
fn markers_after_a_paragraph_are_never_selected_and_nesting_is_explicitly_refused() {
    let source="```sql\nSELECT 42;\n```\n\nKeep.\n<!-- code_chunk_output -->\nUnrelated.\n<!-- /code_chunk_output -->\n";
    let next = projected(source, 1, "SELECT 42;", "42").unwrap();
    assert!(next
        .ends_with("Keep.\n<!-- code_chunk_output -->\nUnrelated.\n<!-- /code_chunk_output -->\n"));
    assert_eq!(next.matches(START).count(), 2);
    assert_eq!(
        projected("> ```sql\n> SELECT 42;\n> ```\n", 1, "SELECT 42;", "42").unwrap_err(),
        CONFLICT
    );
    assert_eq!(
        projected("```sql\nSELECT 99;\n```\n", 1, "SELECT 42;", "42").unwrap_err(),
        CONFLICT
    );
}

#[test]
fn many_fences_resolve_the_requested_line_and_its_utf16_range() {
    let prefix = "😀\n\n```text\nkeep\n```\n\n".repeat(2000);
    let content = format!("{prefix}   ~~~~sql\nSELECT 42;\n   ~~~~\n");
    let source =
        json!({"document_id":"many-fences","generation":3,"review_revision":2,"content":content});
    let target_ = target(&source, 12001, "SELECT 42;").unwrap();
    assert_eq!(target_.from, prefix.encode_utf16().count() + 3);
    assert_eq!(target_.source_line, 12001);
    assert_eq!(target(&source, 12000, "SELECT 42;").unwrap_err(), CONFLICT);
    assert_eq!(
        target(&source, usize::MAX, "SELECT 42;").unwrap_err(),
        CONFLICT
    );
}
