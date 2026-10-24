use super::*;
use serde_json::json;

fn native() -> Value {
    json!({"type":"math","display":true,"latex":"n","omml":concat!(
        "<m:mathPr xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:dispDef m:val=\"0\"/>",
        "<m:preSp m:val=\"120\"/><m:postSp m:val=\"240\"/><m:interSp m:val=\"40\"/><m:intraSp m:val=\"60\"/></m:mathPr>",
        "<m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath><m:r><m:t>n</m:t></m:r></m:oMath></m:oMathPara>"
    )})
}

fn inline(source: &str) -> Value { json!({"t":"math","latex":source}) }
fn block(source: &str) -> Value { json!({"type":"math","latex":source}) }
fn assert_policy(value: &Value, spacings: [u16; 4]) {
    let xml=value["omml"].as_str().expect("An owned ordinary formula must receive the render policy");
    let state=crate::math_defaults::MathDefaultsState::from_xml(&crate::math_document::parse_xml(xml).unwrap()).unwrap();
    assert!(!state.effective.display_defaults);
    assert_eq!([state.effective.pre_spacing,state.effective.post_spacing,state.effective.inter_spacing,state.effective.intra_spacing],spacings);
}

fn remove_derived(value: &mut Value, pointers: &[&str]) {
    for pointer in pointers { assert!(value.pointer_mut(pointer).unwrap().as_object_mut().unwrap().remove("omml").is_some()); }
}

#[test]
fn plain_json_stays_borrowed_and_arbitrary_fields_survive() {
    let input=json!([{"type":"paragraph","text":[{"t":"math","display":false,"latex":"x^2"}],"future":{"keep":[1,2]}}]);
    assert!(matches!(resolve(&input).unwrap(),Cow::Borrowed(_)));
}

#[test]
fn mixed_json_inherits_without_rebuilding_the_non_math_tree() {
    for reverse in [false,true] {
        let plain=json!({"type":"math","display":true,"latex":"x^2","future":{"keep":["a",false]},"publicationContext":"body"});
        let mut values=vec![plain.clone(),native()];
        if reverse {values.reverse();}
        let input=Value::Array(values);
        let output=resolve(&input).unwrap();
        let index=usize::from(reverse);
        let xml=output[index]["omml"].as_str().unwrap();
        let state=crate::math_defaults::MathDefaultsState::from_xml(&crate::math_document::parse_xml(xml).unwrap()).unwrap();
        assert!(!state.effective.display_defaults);
        assert_eq!((state.effective.pre_spacing,state.effective.post_spacing,state.effective.inter_spacing,state.effective.intra_spacing),(120,240,40,60));
        let mut restored=output.into_owned();restored[index].as_object_mut().unwrap().remove("omml");
        assert_eq!(restored,input);
        assert_eq!(input[index],plain);
    }
}

#[test]
fn json_rejects_malformed_owned_sources_and_deep_clone_inputs() {
    for input in [json!([{"type":"math"}]),json!([{"type":"math","latex":"x","omml":false}]),json!([{"type":"paragraph","text":[{"t":"math","latex":false}]}])] {
        assert_eq!(resolve(&input).unwrap_err(),"math_invalid_source");
    }
    let mut input=json!({"type":"paragraph","text":[inline("x")]});
    for _ in 0..130 {input=json!({"type":"quote","blocks":[input]});}
    let input=json!([input]);
    assert_eq!(resolve(&input).unwrap_err(),"math_budget");
}

#[test]
fn math_json_limits_do_not_restrict_documents_without_math() {
    let mut input=json!({"type":"paragraph","text":[{"t":"text","v":"ordinary"}]});
    for _ in 0..130 {input=json!({"type":"quote","blocks":[input]});}
    let input=json!([input]);
    assert!(matches!(resolve(&input).unwrap(),Cow::Borrowed(_)));
}

#[test]
fn every_owned_block_and_inline_child_path_receives_the_policy_and_keeps_other_fields() {
    let wrappers=["bold","italic","strike","styled","author-styled","display"].into_iter()
        .map(|kind|json!({"t":kind,"v":[inline("x")],"format":{"keep":"format"},"text":{"keep":"author"},"display":"block","future":{"t":"math","omml":false}}))
        .chain(std::iter::once(json!({"t":"link","text":[inline("x")],"href":"https://example.invalid","title":"literal","future":{"type":"math","omml":false}}))).collect::<Vec<_>>();
    let input=json!([
        native(),
        {"type":"paragraph","text":[inline("p")],"publicationContext":"body"},
        {"type":"heading","text":[inline("h")],"level":2},
        {"type":"caption","text":[inline("c")],"label":{"keep":true}},
        {"type":"quote","kind":"note","title":[inline("q")],"blocks":[{"type":"paragraph","text":[inline("q")]}]},
        {"type":"list","items":[{"text":[inline("i")],"task":false,"number":7}]},
        {"type":"table","header":[[inline("h")]],"rows":[[[inline("r")]]],"aligns":["right"]},
        {"type":"bibliography","items":[{"firstField":[inline("f")],"text":[inline("b")],"layout":{"future":"opaque"}}]},
        {"type":"paragraph","text":wrappers},
        block("x")
    ]);
    let pointers=["/1/text/0","/2/text/0","/3/text/0","/4/title/0","/4/blocks/0/text/0","/5/items/0/text/0","/6/header/0/0","/6/rows/0/0/0","/7/items/0/firstField/0","/7/items/0/text/0","/8/text/0/v/0","/8/text/1/v/0","/8/text/2/v/0","/8/text/3/v/0","/8/text/4/v/0","/8/text/5/v/0","/8/text/6/text/0","/9"];
    let original=input.clone(); let output=resolve(&input).unwrap();
    for pointer in pointers { assert_policy(output.pointer(pointer).unwrap(),[120,240,40,60]); }
    assert_eq!(output[0],input[0]);
    let mut restored=output.into_owned();remove_derived(&mut restored,&pointers);assert_eq!(restored,input);assert_eq!(input,original);
}

#[test]
fn unknown_math_discriminators_never_supply_policy_or_change_during_projection() {
    let fake=json!({"type":"math","t":"math","latex":"extension literal","omml":false});
    let unknown=json!({"type":"future-block","text":[fake.clone()],"blocks":[native()]});
    let literal=json!({"type":"paragraph","text":[{"t":"future-inline","v":[fake.clone()]},{"t":"text","v":fake.clone()}],"title":fake.clone(),"future":native()});
    let inactive=json!([unknown.clone(),literal.clone(),block("x")]);
    assert!(matches!(resolve(&inactive).unwrap(),Cow::Borrowed(_)));
    let mut ordinary=block("x");ordinary["future"]=fake.clone();ordinary["metadata"]=json!({"nested":fake});
    let input=json!([native(),ordinary,unknown,literal]);let original=input.clone();
    let output=resolve(&input).unwrap();assert_policy(&output[1],[120,240,40,60]);
    let mut restored=output.into_owned();remove_derived(&mut restored,&["/1"]);assert_eq!(restored,input);assert_eq!(input,original);
}

#[test]
fn unknown_only_and_unconsumed_root_shapes_remain_borrowed_even_with_invalid_pseudo_math() {
    let fake=json!({"t":"math","type":"math","omml":false});
    for input in [fake.clone(),json!({"blocks":[native(),fake.clone()]}),json!([{"type":"future","text":[fake.clone()]}]),json!([{"type":"paragraph","text":[{"t":"text","v":fake.clone()}],"future":fake}])] {
        let before=input.clone();assert!(matches!(resolve(&input).unwrap(),Cow::Borrowed(_)));assert_eq!(input,before);
    }
}

#[test]
fn wrapper_scalars_are_text_and_do_not_own_a_bare_math_object() {
    let fake=json!({"t":"math","omml":false});
    let input=json!([native(),{"type":"paragraph","text":[{"t":"bold","v":fake.clone()},{"t":"link","text":fake.clone()},{"t":"strike","v":fake}]},block("x")]);
    let output=resolve(&input).unwrap();assert_policy(&output[2],[120,240,40,60]);
    let mut restored=output.into_owned();remove_derived(&mut restored,&["/2"]);assert_eq!(restored,input);
}

#[test]
fn native_json_can_omit_latex_without_inventing_it_in_the_output() {
    for display in [false,true] {
        let mut source=native();source.as_object_mut().unwrap().remove("latex");
        let input=if display {json!([source.clone(),block("x")])}else{
            source.as_object_mut().unwrap().remove("type");source["t"]=json!("math");source["display"]=json!(false);
            source["omml"]=json!(source["omml"].as_str().unwrap().replace("<m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath>","<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\">").replace("</m:oMath></m:oMathPara>","</m:oMath>"));
            json!([{"type":"paragraph","text":[source.clone()]},block("x")])
        };
        let output=resolve(&input).unwrap();let native_output=if display {&output[0]}else{&output[0]["text"][0]};
        assert_eq!(native_output,&source);assert!(native_output.get("latex").is_none());
        assert_policy(&output[1],if display {[120,240,40,60]}else{[0;4]});
        let mut restored=output.into_owned();remove_derived(&mut restored,&["/1"]);assert_eq!(restored,input);
    }
    let mut source=native();source.as_object_mut().unwrap().remove("latex");
    assert!(matches!(resolve(&json!([source])).unwrap(),Cow::Borrowed(_)));
}

#[test]
fn consumer_position_owns_display_mode_with_both_legacy_discriminators_and_hints() {
    let mut source=native();source["t"]=json!("math");source["display"]=json!(false);
    let input=json!([source.clone(),{"type":"math","t":"math","display":false,"latex":"x"},{"type":"paragraph","text":[{"t":"math","type":"math","display":true,"latex":"x"}]}]);
    let output=resolve(&input).unwrap();assert_eq!(output[0],source);assert_policy(&output[1],[120,240,40,60]);assert_policy(&output[2]["text"][0],[120,240,40,60]);
    let mut modes=Vec::new();visit_math_sites(&input,|_,display,_|{modes.push(display);Ok(())}).unwrap();assert_eq!(modes,[true,true,false]);
    let mut restored=output.into_owned();remove_derived(&mut restored,&["/1","/2/text/0"]);assert_eq!(restored,input);
}

#[test]
fn shared_site_visitor_preserves_consumer_order_and_nested_host_protection() {
    let input=json!([
        block("a"),{"type":"paragraph","text":[inline("b")]},
        {"type":"quote","kind":"note","title":[inline("c")],"blocks":[{"type":"paragraph","text":[inline("d")]}]},
        {"type":"list","items":[{"text":[inline("e")]}]},
        {"type":"table","header":[[inline("f")]],"rows":[[[inline("g")]]]},
        {"type":"bibliography","items":[{"firstField":[inline("h")],"text":[inline("i")]}]},
        {"type":"paragraph","publicationStyle":{},"text":[inline("j")]},
        {"type":"paragraph","publicationNoteLabel":null,"text":[inline("k")]},
        {"type":"paragraph","publicationContext":"note","text":[inline("l")]},
        {"type":"paragraph","publicationContext":"body","text":[inline("m")],"future":{"t":"math","latex":"ignored"}}
    ]);
    let mut sites=Vec::new();visit_math_sites(&input,|value,display,nested|{sites.push((value["latex"].as_str().unwrap().to_owned(),display,nested));Ok(())}).unwrap();
    let expected=[("a",true,false),("b",false,false),("c",false,true),("d",false,true),("e",false,true),("f",false,true),("g",false,true),("h",false,false),("i",false,false),("j",false,true),("k",false,true),("l",false,true),("m",false,false)].into_iter().map(|(text,display,nested)|(text.to_owned(),display,nested)).collect::<Vec<_>>();
    assert_eq!(sites,expected);
    assert_eq!(visit_math_sites(&input,|_,_,_|Err("stop-owned-visitor".into())).unwrap_err(),"stop-owned-visitor");
}

#[test]
fn local_predicate_visitor_uses_the_same_owned_paths_without_classifying_extensions() {
    let input=json!({"type":"quote","title":[{"t":"styled","v":[inline("a")],"format":{"t":"math","omml":false}}],"blocks":[{"type":"paragraph","text":[inline("b")]}],"future":{"type":"math","omml":false}});
    let mut sites=Vec::new();visit_math_subtree_sites(&input,|value|{sites.push(value["latex"].as_str().unwrap().to_owned());Ok(())}).unwrap();assert_eq!(sites,["a","b"]);
    let input=json!([{"t":"link","text":[inline("c")],"href":{"t":"math","omml":false}},inline("d")]);
    let mut sites=Vec::new();visit_math_subtree_sites(&input,|value|{sites.push(value["latex"].as_str().unwrap().to_owned());Ok(())}).unwrap();assert_eq!(sites,["c","d"]);
    let mut count=0;visit_math_subtree_sites(&native(),|_|{count+=1;Ok(())}).unwrap();assert_eq!(count,1);
    let mut count=0;visit_math_sites(&input,|_,_,_|{count+=1;Ok(())}).unwrap();assert_eq!(count,0);
    assert_eq!(visit_math_subtree_sites(&input,|_|Err("stop-local-visitor".into())).unwrap_err(),"stop-local-visitor");
}

#[test]
fn diagnostic_reserved_arrays_preserve_spacing_rejection_in_the_actual_pdf_consumer() {
    let xml=crate::math_document::parse("<math><mspace width='1em'/><mi>x</mi></math>","mathml",false).unwrap().root.xml();
    let math=json!({"t":"math","omml":xml,"latex":"x","fallback":true});
    let display=json!({"type":"math","omml":math["omml"],"latex":"x"});
    let mut inputs=Vec::new();
    for wrapper in [
        json!({"t":"strike","v":[math.clone()]}),
        json!({"t":"unknown","v":[math.clone()]}),json!({"v":[math.clone()]}),
        json!({"t":"unknown","text":[math.clone()]}),
        json!({"t":"unknown","blocks":[display.clone()]}),
    ] { inputs.push(json!([{"type":"paragraph","text":[wrapper]}])); }
    inputs.extend([
        json!([{"type":"unknown","text":[math.clone()]}]),json!([{"text":[math.clone()]}]),
        json!([{"type":"unknown","blocks":[display]}]),
    ]);
    let style=crate::export_styles::default_style();
    for input in inputs {
        let before=input.clone();
        let error=crate::pdf_render::build(&input,&style,"",&|_|None,&mut Vec::new()).unwrap_err();
        assert_eq!(error.message,"math_unsupported_extension","{input}");assert_eq!(input,before);
    }
}

#[test]
fn diagnostic_candidates_never_supply_or_receive_the_document_render_policy() {
    for unknown in [
        json!({"type":"unknown","text":[native()]}),
        json!({"type":"unknown","blocks":[native()]}),
        json!({"type":"paragraph","text":[{"t":"unknown","v":[native()]}]}),
        json!({"type":"paragraph","text":[{"v":[native()]}]}),
    ] {
        let input=json!([unknown.clone(),block("x")]);
        let mut owned=0;visit_math_sites(&input,|_,_,_|{owned+=1;Ok(())}).unwrap();assert_eq!(owned,1);
        let mut diagnosed=0;visit_math_subtree_sites(&input,|_|{diagnosed+=1;Ok(())}).unwrap();assert_eq!(diagnosed,2);
        assert!(matches!(resolve(&input).unwrap(),Cow::Borrowed(_)));
        let active=json!([native(),unknown,block("x")]);let output=resolve(&active).unwrap();
        assert_eq!(output[1],active[1]);assert_policy(&output[2],[120,240,40,60]);
        let mut restored=output.into_owned();remove_derived(&mut restored,&["/2"]);assert_eq!(restored,active);
    }
}

#[test]
fn diagnostic_scanning_keeps_future_metadata_and_non_array_reserved_payloads_opaque() {
    let fake=json!({"type":"math","t":"math","omml":false});
    for wrapper in [
        json!({"t":"unknown","v":fake.clone(),"text":fake.clone(),"blocks":fake.clone()}),
        json!({"type":"unknown","v":[],"text":[],"blocks":[],"future":[fake.clone()],"metadata":{"v":[fake.clone()]},"format":{"text":[fake.clone()]}}),
        json!({"t":"unknown","future":{"blocks":[native()]},"metadata":[fake.clone()]}),
        json!({"t":"text","v":[fake.clone()]}),
    ] {
        let mut diagnosed=0;visit_math_subtree_sites(&wrapper,|_|{diagnosed+=1;Ok(())}).unwrap();assert_eq!(diagnosed,0);
    }
    let mut math=native();math["future"]=json!({"v":[fake]});
    let mut diagnosed=0;visit_math_subtree_sites(&math,|_|{diagnosed+=1;Ok(())}).unwrap();assert_eq!(diagnosed,1);
}

#[test]
fn active_projection_charges_opaque_clone_payloads_without_recognizing_them_as_math() {
    let mut opaque=json!({"type":"math","omml":false});for _ in 0..130 {opaque=json!([opaque]);}
    let inactive=json!([{"type":"paragraph","text":"ordinary","future":opaque.clone()}]);
    assert!(matches!(resolve(&inactive).unwrap(),Cow::Borrowed(_)));
    let input=json!([native(),block("x"),{"type":"paragraph","text":"ordinary","future":opaque}]);
    assert_eq!(resolve(&input).unwrap_err(),"math_budget");
}

#[test]
fn cancellation_interrupts_both_site_visitors_and_a_long_no_math_scan() {
    use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
    let input=json!([native(),block("x")]);
    crate::speech::with_cancel(Arc::new(||true),||{
        assert_eq!(resolve(&input).unwrap_err(),"cancelled");
        assert_eq!(visit_math_sites(&input,|_,_,_|Ok(())).unwrap_err(),"cancelled");
        assert_eq!(visit_math_subtree_sites(&input,|_|Ok(())).unwrap_err(),"cancelled");
    });
    let input=Value::Array((0..512).map(|_|json!({"type":"paragraph","text":[{"t":"text","v":"ordinary"}]})).collect());
    let count=Arc::new(AtomicUsize::new(0));let check=count.clone();
    crate::speech::with_cancel(Arc::new(move||check.fetch_add(1,Ordering::Relaxed)>1),||assert_eq!(resolve(&input).unwrap_err(),"cancelled"));
    assert!(matches!(resolve(&input).unwrap(),Cow::Borrowed(_)));
}
