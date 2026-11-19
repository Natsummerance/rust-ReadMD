use super::*;

fn batch_request(state: &Value, action: &str, batch: &Value) -> Value {
    json!({"action":action,"document_id":state["document_id"],"generation":state["generation"],"review_revision":state["review_revision"],"batch":batch,"confirm":true})
}

#[test]
fn code_batch_mcp_uses_the_same_confirmation_and_bounded_sql_engine() {
    let root = tempfile::tempdir().unwrap(); let runtime = app(root.path());
    let file = root.path().join("batch-mcp.md"); std::fs::write(&file, "```sql cmd output=true\nSELECT 42;\n```\n").unwrap();
    let head = open(&runtime, &file);
    let mut args = json!({"action":"code_batch_target","document_id":head["document_id"],"generation":head["generation"],"review_revision":head["review_revision"],"confirm":true});
    let prepared = crate::mcp::call_tool(&runtime, "readmd_review_document", &args);
    assert_ne!(prepared["isError"], true, "{prepared}");
    args["action"] = json!("code_batch_run"); args["batch"] = prepared["structuredContent"]["code_batch"].clone();
    let result = crate::mcp::call_tool(&runtime, "readmd_review_document", &args);
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["source_written"], false);
    assert_eq!(result["structuredContent"]["results"][0]["stdout"], "42\n--\n42");
    args["confirm"] = json!(false);
    assert_eq!(crate::mcp::call_tool(&runtime, "readmd_review_document", &args)["isError"], true);
}

#[test]
fn durable_code_batch_is_one_transaction_across_save_reopen_and_receipt_retry() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("批量 中文😀.md");
    let code = "```sql cmd output=true\nSELECT 42;\n```\n";
    let content = format!("# 😀\n\n{code}\nSame.\n\n{code}");
    std::fs::write(&file, format!("\u{feff}{}", content.replace('\n', "\r\n"))).unwrap();
    let first = open(&runtime, &file);
    let before = mutate(&runtime, &first, "batch-pending", "propose", json!({"origin":"ai","edits":[source_edit(&first,"Same.","Pending.",0)]}));
    let prepared = api(&runtime, "/api/review", &batch_request(&before, "code_batch_target", &Value::Null)).1;
    assert_eq!(prepared["code_batch"]["items"].as_array().unwrap().len(), 2);
    let run = api(&runtime, "/api/review", &batch_request(&before, "code_batch_run", &prepared["code_batch"])).1;
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["source_written"], false);
    let outputs = run["results"].as_array().unwrap().iter().map(|r| r["stdout"].clone()).collect::<Vec<_>>();
    assert_eq!(outputs, vec![json!("42\n--\n42"), json!("42\n--\n42")]);
    let mut insert = batch_request(&before, "code_batch_output", &prepared["code_batch"]);
    insert["call_id"] = json!("batch-one-commit"); insert["outputs"] = json!(outputs);
    let (status, result) = api(&runtime, "/api/review", &insert); assert_eq!(status, 200, "{result}");
    let each = format!("{code}\n<!-- code_chunk_output -->\n\n42\n--\n42\n\n<!-- /code_chunk_output -->\n");
    let expected = format!("# 😀\n\n{each}\nPending.\n\n{each}");
    assert_eq!(result["content"], expected);
    assert_eq!(result["generation"].as_u64(), Some(before["generation"].as_u64().unwrap() + 1));
    assert_eq!(api(&runtime, "/api/review", &insert).1, result);
    let mut conflict = insert.clone(); conflict["outputs"][0] = json!("other");
    assert_eq!(api(&runtime, "/api/review", &conflict).1["error_code"], "review_call_conflict");
    assert_eq!(api(&runtime, "/api/save", &save_body(&file, &result)).0, 200);
    drop(runtime);
    let runtime = app(root.path()); let reopened = open(&runtime, &file);
    assert_eq!(reopened["content"], expected);
    let undone = mutate(&runtime, &reopened, "batch-single-undo", "undo", json!({}));
    assert_eq!(undone["content"], before["content"]);
    assert_eq!(undone["changes"][0]["original"], "Same.");
    assert_eq!(undone["changes"][0]["decision"], "pending");
    assert_eq!(api(&runtime, "/api/review", &insert).1, result);
    assert_eq!(api(&runtime, "/api/review", &json!({"action":"read","document_id":undone["document_id"]})).1["content"], before["content"]);
    let redone = mutate(&runtime, &undone, "batch-single-redo", "redo", json!({}));
    assert_eq!(redone["content"], expected);
    assert_eq!(api(&runtime, "/api/save", &save_body(&file, &redone)).0, 200);
    assert_eq!(std::fs::read(file).unwrap(), format!("\u{feff}{}", expected.replace('\n', "\r\n")).into_bytes());
}

#[test]
fn code_batch_preflights_all_runtimes_and_stops_on_first_sql_failure() {
    let root = tempfile::tempdir().unwrap(); let runtime = app(root.path());
    for (name, content, expected) in [
        ("mixed", "```sql cmd\nINVALID SQL;\n```\n\n```js cmd\nconsole.log(1);\n```\n", "capability_pack_unavailable"),
        ("failure", "```sql cmd\nINVALID SQL;\n```\n\n```sql cmd\nSELECT 999;\n```\n", "execution_failed"),
        ("partial", "```sql cmd output=true\nSELECT 42;\n```\n\n```sql cmd output=true\nINVALID SQL;\n```\n", "execution_failed"),
    ] {
        let file = root.path().join(format!("{name}.md")); std::fs::write(&file, content).unwrap(); let before = open(&runtime, &file);
        let prepared = api(&runtime, "/api/review", &batch_request(&before, "code_batch_target", &Value::Null)).1;
        let mut request = batch_request(&before, "code_batch_run", &prepared["code_batch"]);
        request["confirm"] = json!(false);
        assert_eq!(api(&runtime, "/api/review", &request).1["error_code"], "confirmation_required");
        request["confirm"] = json!(true);
        let result = api(&runtime, "/api/review", &request).1; assert_eq!(result["error_code"], expected, "{result}");
        if name == "failure" { assert_eq!(result["failed_index"], 0); assert_eq!(result["results"], json!([])); }
        if name == "partial" { assert_eq!(result["failed_index"], 1); assert_eq!(result["results"][0]["stdout"], "42\n--\n42"); }
        assert_eq!(api(&runtime, "/api/review", &json!({"action":"read","document_id":before["document_id"]})).1["generation"], before["generation"]);
        assert_eq!(std::fs::read_to_string(file).unwrap(), content);
    }
}

#[test]
fn code_batch_rejects_tampered_stale_cross_document_and_oversized_outputs() {
    let root = tempfile::tempdir().unwrap(); let runtime = app(root.path());
    let content = "```sql cmd output=true\nSELECT 1;\n```\n";
    let file = root.path().join("batch.md"); std::fs::write(&file, content).unwrap(); let first = open(&runtime, &file);
    let other = root.path().join("other.md"); std::fs::write(&other, content).unwrap(); let second = open(&runtime, &other);
    let prepared = api(&runtime, "/api/review", &batch_request(&first, "code_batch_target", &Value::Null)).1;
    let mut write = batch_request(&first, "code_batch_output", &prepared["code_batch"]); write["call_id"] = json!("batch-protection"); write["outputs"] = json!(["1"]);
    let mut invalid = write.clone(); invalid["batch"]["items"][0]["output"] = json!(false);
    assert_eq!(api(&runtime, "/api/review", &invalid).1["error_code"], "review_conflict");
    invalid = write.clone(); invalid["document_id"] = second["document_id"].clone();
    assert_eq!(api(&runtime, "/api/review", &invalid).1["error_code"], "review_conflict");
    invalid = write.clone(); invalid["outputs"] = json!(["x".repeat(400001)]);
    assert_eq!(api(&runtime, "/api/review", &invalid).1["error_code"], "review_budget");
    let edited = mutate(&runtime, &first, "batch-edit", "edit", json!({"origin":"editor","edits":[{"from":0,"to":0,"insert":"Manual.\n"}]}));
    let undone = mutate(&runtime, &edited, "batch-edit-undo", "undo", json!({})); assert_eq!(undone["content"], content);
    assert_eq!(api(&runtime, "/api/review", &write).1["error_code"], "review_stale_generation");
    let run = batch_request(&first, "code_batch_run", &prepared["code_batch"]);
    assert_eq!(api(&runtime, "/api/review", &run).1["error_code"], "review_stale_generation");
}

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
