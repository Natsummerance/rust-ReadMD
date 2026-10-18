//! Original synthetic CSL group-suppression cases, independently checked
//! with the locked citeproc-js QA runtime. No upstream fixture is copied.
use serde_json::{Value,json};

#[test]
fn academic_empty_names_preserve_variable_attempts_in_groups_and_macros() {
    let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/citation-empty-names.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let request=crate::academic::parse_request(json!({"format":"csl-json",
            "data":case["items"].to_string(),"csl":case["csl"],"locale":"en-US",
            "clusters":[[{"key":"atlas"}]]})).unwrap();
        let output=crate::academic::citations(&request).unwrap();
        assert_eq!(output["citations"][0]["text"],case["expected"],"case {}",case["id"]);
        assert_eq!(output["bibliography"]["items"][0]["text"],"Atlas");
        assert_eq!(output["diagnostics"],json!([]));
    }
}
