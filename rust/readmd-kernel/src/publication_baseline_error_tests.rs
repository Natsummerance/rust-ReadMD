//! The actual paint proof's bounded error crosses HTTP, preview and MCP via
//! the shared error adapter; it never becomes a user-visible engine string.
use super::*;
use serde_json::json;

fn details()->Value{json!({"coordinateSpace":"pdfBaselineInk","blockId":"paragraph-2","page":2,
    "pageObject":8,"reason":"ink-collision","ink":[36.,41.,150.,55.],"otherBlockId":"paragraph-1"})}
fn body(value:&Value)->Value{error_body(&format!("publication_layout_geometry: {value}"))}

#[test]
fn p04_actual_ink_error_preserves_source_identity_and_real_page_without_leaking_engine_text(){
    for reason in ["ink-collision","clipped-ink","paper-edge"]{
        let mut value=details();value["reason"]=json!(reason);
        assert_eq!(body(&value),json!({"ok":false,"error_code":"publication_layout_geometry","details":value}));
        value["blockId"]=Value::Null;value["otherBlockId"]=Value::Null;
        assert_eq!(body(&value)["details"],value);
    }
}

#[test]
fn p04_actual_ink_error_rejects_unbounded_ambiguous_and_wrong_coordinate_payloads(){
    for (key,value)in[("coordinateSpace",json!("source")),("page",json!(0)),("page",json!(100001)),
        ("page",json!(1.5)),("page",json!("2")),("pageObject",json!(0)),("pageObject",json!(4294967296_u64)),
        ("blockId",json!("")),("blockId",json!("line\ncontrol")),("otherBlockId",json!(false)),
        ("reason",json!("unknown")),("ink",json!([1,2,3])),("ink",json!([1,2,3,4,5])),
        ("ink",json!([1,2,"3",4])),("ink",json!([1,2,1e10,4])),("ink",json!([3,2,1,4])),
        ("unexpected",json!(true))] {
        let mut bad=details();bad[key]=value;assert!(body(&bad)["details"].is_string(),"{bad}");
    }
    for key in ["coordinateSpace","blockId","page","pageObject","reason","ink","otherBlockId"] {
        let mut bad=details();bad.as_object_mut().unwrap().remove(key);assert!(body(&bad)["details"].is_string(),"{key}");
    }
    let valid=details().to_string();let duplicate=valid.replacen("\"page\":2","\"page\":2,\"page\":2",1);assert_ne!(duplicate,valid);
    for raw in [duplicate,"[]".into(),"null".into(),format!("{valid} trailing"),format!("{}{}",valid," ".repeat(4097))] {
        assert!(error_body(&format!("publication_layout_geometry: {raw}"))["details"].is_string());
    }
}
