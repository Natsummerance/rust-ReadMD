use super::*;

#[test]
fn durable_code_output_mcp_reuses_the_receipt_and_identical_result_adds_no_undo() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("MCP-code.md");
    let source = "```sql cmd output=true\nSELECT 42;\n```\n";
    std::fs::write(&file, source).unwrap();
    let first = open(&runtime, &file);
    let context = |s: &Value, action: &str| json!({"action":action,"document_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"confirm":true});
    let call = |args: &Value| {
        let result = crate::mcp::call_tool(&runtime, "readmd_review_document", args);
        assert_ne!(result["isError"], true, "{result}");
        result["structuredContent"].clone()
    };
    let mut prepare = context(&first, "code_target");
    prepare["source_line"] = json!(1);
    prepare["code"] = json!("SELECT 42;");
    let target = call(&prepare)["code_target"].clone();
    let mut insert = context(&first, "code_output");
    insert["call_id"] = json!("mcp-code-insert");
    insert["target"] = target;
    insert["output"] = json!("42");
    let result = call(&insert);
    assert!(result["content"]
        .as_str()
        .unwrap()
        .contains("<!-- code_chunk_output -->\n\n42\n"));
    assert_eq!(call(&insert), result);
    prepare["generation"] = result["generation"].clone();
    prepare["review_revision"] = result["review_revision"].clone();
    let target = call(&prepare)["code_target"].clone();
    let mut identical = context(&result, "code_output");
    identical["call_id"] = json!("mcp-code-identical");
    identical["target"] = target;
    identical["output"] = json!("42");
    let unchanged = call(&identical);
    assert_eq!(unchanged["code_output_unchanged"], true);
    assert_eq!(unchanged["generation"], result["generation"]);
    let mut undo = context(&result, "undo");
    undo["call_id"] = json!("mcp-code-one-undo");
    let undone = call(&undo);
    assert_eq!(undone["content"], source);
    assert_eq!(
        call(&insert),
        result,
        "old receipt is reported without running or inserting again"
    );
    let current = call(&json!({"action":"read","document_id":first["document_id"],"confirm":true}));
    assert_eq!(current["content"], source);
    assert_eq!(current["generation"], undone["generation"]);
    assert_eq!(std::fs::read_to_string(file).unwrap(), source);
}

#[test]
fn durable_code_output_rejects_another_document_tampering_and_edit_then_undo() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let source = "😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nTail.\n";
    let a = root.path().join("A.md");
    let b = root.path().join("B.md");
    std::fs::write(&a, source).unwrap();
    std::fs::write(&b, source).unwrap();
    let first = open(&runtime, &a);
    let second = open(&runtime, &b);
    let context = |s: &Value, action: &str, call: &str| json!({"action":action,"document_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"call_id":call});
    let mut prepare = context(&first, "code_target", "unused");
    prepare["source_line"] = json!(3);
    prepare["code"] = json!("SELECT 42;");
    let (status, prepared) = api(&runtime, "/api/review", &prepare);
    assert_eq!(status, 200);
    let mut body = context(&first, "code_output", "fixed-result");
    body["target"] = prepared["code_target"].clone();
    body["output"] = json!("42");
    let mut swapped = body.clone();
    swapped["document_id"] = second["document_id"].clone();
    swapped["generation"] = second["generation"].clone();
    swapped["review_revision"] = second["review_revision"].clone();
    assert_eq!(
        api(&runtime, "/api/review", &swapped).1["error_code"],
        "review_conflict"
    );
    let mut tampered = body.clone();
    tampered["target"]["node_id"] = json!("0".repeat(64));
    assert_eq!(
        api(&runtime, "/api/review", &tampered).1["error_code"],
        "review_conflict"
    );
    let mut marker = body.clone();
    marker["output"] = json!("<!-- /code_chunk_output -->");
    assert_eq!(
        api(&runtime, "/api/review", &marker).1["error_code"],
        "review_conflict"
    );
    let cancelled =
        crate::speech::with_cancel(Arc::new(|| true), || api(&runtime, "/api/review", &body));
    assert_eq!(cancelled.1["error_code"], "cancelled");
    let edited = mutate(
        &runtime,
        &first,
        "code-hand-edit",
        "edit",
        json!({"origin":"editor","edits":[source_edit(&first,"Tail.","Manual.",0)]}),
    );
    let undone = mutate(&runtime, &edited, "code-hand-undo", "undo", json!({}));
    assert_eq!(undone["content"], first["content"]);
    assert_eq!(
        api(&runtime, "/api/review", &body).1["error_code"],
        "review_stale_generation"
    );
    let before = api(
        &runtime,
        "/api/review",
        &json!({"action":"read","document_id":undone["document_id"]}),
    )
    .1;
    assert_eq!(before["generation"], undone["generation"]);
    assert_eq!(before["content"], source);
    assert_eq!(std::fs::read_to_string(a).unwrap(), source);
    assert_eq!(std::fs::read_to_string(b).unwrap(), source);
}
