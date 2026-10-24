use super::*;

fn source() -> String {format!("😀\r\n\r\n$$x$$\r\n\r\n```readmd-math v1\nrow(office.mathPr(office.dispDef(val: \"0\"), office.preSp(val: \"120\")), office.oMathPara(office.oMath(x)))\n```\r\n\r\n$$x$$")}
fn request(context:&Value,ordinal:usize)->Value {
    let o=&context["occurrences"][ordinal];json!({"source":o["source"],"dialect":o["dialect"],"display":o["display"],
        "document_context":{"id":context["id"],"revision":context["revision"],"fingerprint":context["fingerprint"],"from":o["from"],"to":o["to"]}})
}

#[test]
fn snapshots_bind_duplicate_sources_and_keep_author_source_sparse() {
    let runtime=Runtime::default();let source=source();let r=runtime.register(&source,&tx::revision(&source)).unwrap();let context=&r["context"];
    let a=runtime.bind(&request(context,0)).unwrap().unwrap();let b=runtime.bind(&request(context,2)).unwrap().unwrap();
    assert_ne!(a.response()["from"],b.response()["from"]);assert_eq!(a.document().source,"x");
    let before=serde_json::to_value(a.document()).unwrap();let rendered=a.render(a.document()).unwrap();
    assert_ne!(rendered.root.xml(),a.document().root.xml());assert_eq!(serde_json::to_value(a.document()).unwrap(),before);
    assert_eq!(a.entry.source,source);assert_eq!(a.response()["revision"],tx::revision(&source));
}

#[test]
fn client_cannot_replace_document_identity_revision_fingerprint_source_mode_or_range() {
    let runtime=Runtime::default();let source=source();let context=runtime.register(&source,&tx::revision(&source)).unwrap()["context"].clone();
    for key in ["id","revision","fingerprint"] {let mut q=request(&context,0);q["document_context"][key]=json!("forged");assert!(runtime.bind(&q).is_err());}
    let mut q=request(&context,0);q["source"]=json!("y");assert!(runtime.bind(&q).is_err());
    let mut q=request(&context,0);q["display"]=json!(false);assert!(runtime.bind(&q).is_err());
    let mut q=request(&context,0);q["document_context"]["from"]=json!(1);assert!(runtime.bind(&q).is_err());
    let mut q=request(&context,0);q["document_context"]["to"]=q["document_context"]["from"].clone();assert!(runtime.bind(&q).is_err());
    let mut q=request(&context,0);q["document_context"]["mathPr"]=json!({"dispDef":false});assert!(runtime.bind(&q).is_err());
    assert!(runtime.register(&source,"old-revision").is_err());
}

#[test]
fn supplied_invalid_context_never_falls_back_to_standalone() {
    let runtime=Runtime::default();assert!(runtime.bind(&json!({"source":"x"})).unwrap().is_none());
    for context in [Value::Null,json!({}),json!("handle"),json!({"id":"x","revision":"x","fingerprint":"x","from":-1,"to":1})] {
        assert!(runtime.bind(&json!({"source":"x","document_context":context})).is_err());
    }
}

#[test]
fn eight_context_lru_evicts_only_unpinned_snapshots() {
    let runtime=Runtime::default();let mut contexts=Vec::new();
    for i in 0..8 {let s=format!("$$x_{i}$$");contexts.push(runtime.register(&s,&tx::revision(&s)).unwrap()["context"].clone());}
    let pinned=runtime.bind(&request(&contexts[0],0)).unwrap().unwrap();
    let s="$$last$$";runtime.register(s,&tx::revision(s)).unwrap();
    assert!(runtime.current(&pinned).is_ok());assert!(runtime.bind(&request(&contexts[1],0)).is_err());
    assert_eq!(runtime.state.lock().unwrap().entries.len(),MAX_CONTEXTS);
}

#[test]
fn expired_handle_and_inflight_publishing_are_rejected_without_sleeping() {
    let runtime=Runtime::default();let s="$$x$$";let c=runtime.register(s,&tx::revision(s)).unwrap()["context"].clone();
    {let mut state=runtime.state.lock().unwrap();Arc::get_mut(state.entries.front_mut().unwrap()).unwrap().expires=Instant::now()-Duration::from_secs(1);}
    assert!(runtime.bind(&request(&c,0)).is_err());assert_eq!(runtime.state.lock().unwrap().entries.len(),0);
}

#[test]
fn expression_patch_reanalyzes_complete_candidate_and_provides_a_new_bound_snapshot() {
    let runtime=Runtime::default();let source=source();let c=runtime.register(&source,&tx::revision(&source)).unwrap()["context"].clone();
    let old=runtime.bind(&request(&c,0)).unwrap().unwrap();let changed=math_document::parse("y","latex",true).unwrap();
    let new=runtime.patch_binding(&old,&changed).unwrap();assert_eq!(new.document().source,"y");
    assert_ne!(old.response()["id"],new.response()["id"]);assert_eq!(old.document().source,"x");
    assert!(new.entry.source.contains("$$y$$"));assert!(new.entry.source.ends_with("$$x$$"));
    assert_eq!(new.entry.analysis.occurrences[2].source,"x");new.render(&changed).unwrap();
}

#[test]
fn provider_mathpr_patch_cannot_reuse_a_previously_verified_plan() {
    let runtime=Runtime::default();let source=source();let c=runtime.register(&source,&tx::revision(&source)).unwrap()["context"].clone();
    let provider=runtime.bind(&request(&c,1)).unwrap().unwrap();let changed=math_document::parse(&provider.document().source.replace("120","240"),"dsl",true).unwrap();
    assert!(runtime.patch_binding(&provider,&changed).is_err());assert_eq!(provider.entry.source,source);
}

#[test]
fn memory_charge_includes_author_tree_plan_and_models() {
    let runtime=Runtime::default();let source=source();let c=runtime.register(&source,&tx::revision(&source)).unwrap()["context"].clone();
    let binding=runtime.bind(&request(&c,0)).unwrap().unwrap();assert!(binding.entry.charge>3*source.len());
    assert!(runtime.state.lock().unwrap().bytes<=MAX_BYTES);
}

#[test]
fn projected_view_tokens_are_original_occurrence_bound_and_preserve_every_line_ending() {
    let runtime=Runtime::default();let source=source();let view=format!("# Projected prose\r\n\r\n{source}\r\n");
    let r=runtime.register_view(&source,&tx::revision(&source),Some(&view)).unwrap();let c=&r["context"];
    let output=r["view_content"].as_str().unwrap();let nonce=c["nonce"].as_str().unwrap();
    assert_eq!(output.matches("<!--readmd:math-slot:v1:").count(),3);
    for ordinal in 0..3 {assert!(output.contains(&format!("<!--readmd:math-slot:v1:{nonce}:{ordinal}-->")));}
    assert_eq!(view.chars().filter(|c|matches!(c,'\r'|'\n')).collect::<String>(),output.chars().filter(|c|matches!(c,'\r'|'\n')).collect::<String>());
    assert_eq!(c["revision"],tx::revision(&source));
    let binding=runtime.bind(&request(c,2)).unwrap().unwrap();assert_eq!(binding.entry.source,source);
}

#[test]
fn view_cannot_add_remove_change_or_reorder_mathematical_sources() {
    let runtime=Runtime::default();let original=source();
    for view in [original.replace("$$x$$","$$y$$"),format!("{original}\n\n$$new$$"),original.replacen("$$x$$","",1),original.replacen("$$x$$","```readmd-math v1\nx\n```",1)] {
        assert!(runtime.register_view(&original,&tx::revision(&original),Some(&view)).is_err());
    }
}

#[test]
fn decoded_container_fences_cannot_be_patched_as_a_contiguous_body() {
    let runtime=Runtime::default();let s="$$y$$\n\n> ```readmd-math v1\n> row(office.mathPr(office.dispDef(val: \"0\")),\n> office.oMathPara(office.oMath(x)))\n> ```";
    let c=runtime.register(s,&tx::revision(s)).unwrap()["context"].clone();let bound=runtime.bind(&request(&c,1)).unwrap().unwrap();
    let changed=math_document::parse(&bound.document().source.replace("oMath(x)","oMath(z)"),"dsl",true).unwrap();
    assert!(runtime.patch_binding(&bound,&changed).is_err());bound.render(bound.document()).unwrap();
}
