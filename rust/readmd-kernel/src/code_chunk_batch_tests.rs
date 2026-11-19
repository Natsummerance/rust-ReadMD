use super::*;
fn source(content: &str) -> Value {
    json!({"document_id":"batch-doc","generation":2,"review_revision":1,"content":content})
}
#[test]
fn command_attributes_keep_the_existing_renderer_flag_precedence() {
    assert_eq!(
        metadata("sql cmd=javascript cmd output"),
        Some(("javascript".into(), true))
    );
    assert_eq!(
        metadata("sql {cmd cmd=\"\" output}"),
        Some(("sql".into(), true))
    );
    assert_eq!(
        metadata("text cmd='sqlite' output=false output"),
        Some(("sqlite".into(), false))
    );
    assert_eq!(metadata("sql cmd=false cmd output=true"), None);
    assert_eq!(metadata("sql output=html"), None);
}
#[test]
fn batch_source_order_attributes_and_generated_code_provenance() {
    let content = "😀\n\n```sql {cmd output='true'}\nSELECT 1;\n```\n\n<!-- code_chunk_output -->\n\n```sql cmd output=true\nSELECT 999;\n```\n\n<!-- /code_chunk_output -->\n\n~~~text cmd=sqlite output=none\nSELECT 1;\n~~~\n\n```sql cmd=false\nSELECT 888;\n```\n";
    let batch = prepare(&source(content)).unwrap();
    assert_eq!(batch.items.len(), 2);
    assert_eq!(
        batch
            .items
            .iter()
            .map(|i| i.target.source_line)
            .collect::<Vec<_>>(),
        vec![3, 15]
    );
    assert_eq!(batch.items[0].target.code, "SELECT 1;");
    assert!(batch.items[0].output);
    assert_eq!(batch.items[1].lang, "sqlite");
    assert!(!batch.items[1].output);
    assert_eq!(
        target(&source(content), 9, "SELECT 999;").unwrap_err(),
        CONFLICT
    );
}
#[test]
fn literal_markers_are_code_and_broken_output_never_becomes_input() {
    let literal = "````sql cmd\nSELECT '<!-- code_chunk_output -->';\n````\n";
    assert_eq!(prepare(&source(literal)).unwrap().items.len(), 1);
    for content in [
        "<!-- code_chunk_output -->\n\n```sql cmd\nSELECT 1;\n```\n",
        "<!-- /code_chunk_output -->\n\n```sql cmd\nSELECT 1;\n```\n",
        "<!-- code_chunk_output -->\n\n<!-- code_chunk_output -->\n\n<!-- /code_chunk_output -->\n",
        "> ```sql cmd\n> SELECT 1;\n> ```\n",
        "```sql cmd\nSELECT 1;",
    ] {
        assert_eq!(prepare(&source(content)).unwrap_err(), CONFLICT);
    }
}
#[test]
fn backend_batch_budget_rejects_before_any_execution() {
    assert_eq!(
        prepare(&source(&"```sql cmd\nSELECT 1;\n```\n\n".repeat(33))).unwrap_err(),
        BUDGET
    );
    let large = format!(
        "```sql cmd\n{}\n```\n\n```sql cmd\nSELECT 2;\n```\n",
        "x".repeat(MAX_CODE)
    );
    assert_eq!(prepare(&source(&large)).unwrap_err(), BUDGET);
}
