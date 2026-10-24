use super::*;
use crate::document_transaction::{self as tx, ContentTransaction};
fn config() -> Value { json!({"headings":{"levels":[{"format":"upperRoman"},{"format":"lowerLetter"}],"separator":".","suffix":"space"}}) }
#[test]
fn source_numbering_service_is_reversible_and_preserves_bom_eol_identity() {
    let source="\u{feff}---\r\ntitle: 编号测试\r\n---\r\n# 第一章 {#stable}\r\n\r\n## 𝛂\r\n\r\n正文\r\n";
    let original=numbering_request(&json!({"content":source})).unwrap();
    assert!(original["config"].is_null());
    let result=numbering_request(&json!({"action":"apply","content":source,"base_revision":original["revision"],"config":config()})).unwrap();
    let changed=result["result"]["content"].as_str().unwrap();
    assert!(changed.starts_with("\u{feff}---\r\n"));
    assert!(!changed.replace("\r\n","").contains('\n'));
    assert!(changed.ends_with("## 𝛂\r\n\r\n正文\r\n"));
    assert_eq!(result["transaction"]["edits"].as_array().unwrap().len(),1);
    assert_eq!(result["plan"]["headings"][0]["id"],"stable");
    assert_eq!(result["plan"]["headings"][1]["numbering"]["text"],"I.a");
    let inverse=ContentTransaction{base_revision:tx::revision(changed),origin:"editor".into(),edits:serde_json::from_value(result["result"]["inverse"].clone()).unwrap()};
    assert_eq!(tx::apply(changed,&inverse).unwrap().content,source);
}
#[test]
fn source_numbering_service_requires_explicit_clear_and_current_revision() {
    let source="# First\n";
    let base=tx::revision(source);
    assert_eq!(numbering_request(&json!({"action":"apply","content":source,"base_revision":base})).unwrap_err(),"invalid_json");
    assert_eq!(numbering_request(&json!({"action":"apply","content":source,"base_revision":"stale","config":null})).unwrap_err(),"transaction_conflict");
    let unchanged=numbering_request(&json!({"action":"apply","content":source,"base_revision":base,"config":null})).unwrap();
    assert_eq!(unchanged["result"]["changed"],false);
    assert_eq!(unchanged["result"]["content"],source);
    assert_eq!(unchanged["transaction"]["edits"].as_array().unwrap().len(),0);
    assert_eq!(numbering_request(&json!({"action":"write","content":source})).unwrap_err(),"invalid_json");
    for action in [Value::Null,json!(true),json!(42),json!({}),json!([])] {
        assert_eq!(numbering_request(&json!({"action":action,"content":source})).unwrap_err(),"invalid_json");
    }
    assert_eq!(numbering_request(&json!({"content":source,"unexpected":true})).unwrap_err(),"invalid_json");
}
#[test]
fn source_numbering_service_rejects_unknown_properties_and_missing_ancestors() {
    let source="## Orphan\n";
    assert_eq!(numbering_request(&json!({"action":"apply","content":source,"base_revision":tx::revision(source),"config":config()})).unwrap_err(),NUMBERING_HIERARCHY);
    let source="# Good\n";let mut bad=config();bad["headings"]["hidden"]=json!(true);
    assert_eq!(numbering_request(&json!({"action":"apply","content":source,"base_revision":tx::revision(source),"config":bad})).unwrap_err(),NUMBERING_INVALID);
}
