//! Real in-process services and persistence, without a native chooser/engine.
use serde_json::{json,Value};
use std::{path::Path,sync::Arc};
fn app(root:&Path)->Arc<crate::App>{
    let assets=root.join("assets");std::fs::create_dir_all(&assets).unwrap();
    Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("profile"),root,&assets)).unwrap())
}
#[test]
fn publication_profile_invalid_preset_and_last_leave_previous_file_byte_identical(){
    let root=tempfile::tempdir().unwrap();let app=app(root.path());
    let file=app.paths.data_dir.join("export_presets.json");
    let original=b"{\"custom\":{\"Saved\":{\"footnotes\":{\"start\":4}}},\"last\":null}\n";
    std::fs::write(&file,original).unwrap();
    for body in [json!({"custom":{"Next":{"footnotes":{"numberFormat":"nativeMadeUp"}}}}),
        json!({"last":{"fmt":"docx","options":{"footnotes":{"start":65536}}}})] {
        let(status,result)=crate::server::call_in_process(&app,"POST","/api/export/presets",Some(&body));
        assert_eq!(status,422,"{result}");assert_eq!(result["error_code"],"publication_note_policy_invalid");
        assert_eq!(std::fs::read(&file).unwrap(),original);
    }
}
#[test]
fn publication_profile_presets_keep_legal_native_values_without_clamping_or_truncating(){
    let root=tempfile::tempdir().unwrap();let app=app(root.path());
    let notice="N".repeat(400);
    let notes=json!({"start":65535,"size":30,"numberFormat":"chicago","continuationNotice":notice});
    let body=json!({"custom":{"Native":{"footnotes":notes}},"last":{"fmt":"docx","options":{"footnotes":notes}}});
    let(status,result)=crate::server::call_in_process(&app,"POST","/api/export/presets",Some(&body));
    assert_eq!(status,200,"{result}");assert_eq!(result["ok"],true);
    let stored:Value=serde_json::from_slice(&std::fs::read(app.paths.data_dir.join("export_presets.json")).unwrap()).unwrap();
    assert_eq!(stored["custom"]["Native"]["footnotes"],notes);assert_eq!(stored["last"]["options"]["footnotes"],notes);
}
#[test]
fn publication_profile_get_default_and_source_patch_share_http_and_mcp(){
    let root=tempfile::tempdir().unwrap();let app=app(root.path());let source="Text\n";
    let(status,get)=crate::server::call_in_process(&app,"POST","/api/publication/profile",Some(&json!({"content":source})));
    assert_eq!(status,200,"{get}");assert_eq!(get["options"],json!({}));
    let result=crate::mcp::call_tool(&app,"readmd_publication_profile",&json!({"action":"patch","content":source,"base_revision":get["revision"],"patch":{"footnotes":{"start":4}}}));
    assert_ne!(result["isError"],true,"{result}");
    let changed=&result["structuredContent"];
    assert_eq!(changed["sourceValues"]["footnotes"]["start"],4);assert_eq!(changed["changed"],true);
    assert!(!app.paths.data_dir.join("export_presets.json").exists());
}

#[test]
fn section_only_notes_reach_preflight_before_resources_or_output_are_committed(){
    let temporary=tempfile::tempdir().unwrap();let app=app(temporary.path());
    let source="<!-- readmd-section:v1 {\"id\":\"first\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"beneathText\"}}}} -->\n\n# First\n\nText[^n].\n\n<!-- readmd-section:v1 {\"id\":\"second\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"pageBottom\"}}}} -->\n\n# Second\n\nRepeated[^n].\n\n[^n]: Authored note.\n";
    let options=json!({"pdf":{"engine":"rust"},"cover":{"enabled":false},"toc":{"enabled":false}});
    assert!(options.get("footnotes").is_none());
    for format in ["html","pdf","tex","epub"]{
        let out=temporary.path().join(format!("protected.{format}"));std::fs::write(&out,b"OLD OUTPUT").unwrap();
        let(status,result)=crate::server::call_in_process(&app,"POST","/api/export",Some(&json!({"format":format,"content":source,"baseDir":temporary.path(),"out_path":out,"options":options})));
        assert_eq!(status,200,"{format}: {result}");assert_eq!(result["ok"],false,"{format}: {result}");
        assert_eq!(result["error_code"],crate::publication_note_policies::UNSUPPORTED,"{format}: {result}");
        assert_eq!(std::fs::read(&out).unwrap(),b"OLD OUTPUT");
    }
    let preview_args=json!({"format":"pdf","content":source,"baseDir":temporary.path(),"suggestedName":"Section note proof","options":options});
    let key=crate::export_preview::key_with_data("pdf",source,temporary.path().to_str().unwrap(),&options,"Section note proof",&app.paths.data_dir,&app.paths.assets_dir);
    let(status,preview)=crate::server::call_in_process(&app,"POST","/api/export/preview",Some(&preview_args));
    assert_eq!(status,200,"{preview}");assert_eq!(preview["error_code"],crate::publication_note_policies::UNSUPPORTED,"{preview}");
    assert!(crate::export_preview::cached(&key).is_none());
    let word=temporary.path().join("sections.docx");
    let(status,result)=crate::server::call_in_process(&app,"POST","/api/export",Some(&json!({"format":"docx","content":source,"baseDir":temporary.path(),"out_path":word,"options":options})));
    assert_eq!(status,200,"{result}");assert_eq!(result["ok"],true,"{result}");
    assert!(std::fs::read(word).unwrap().starts_with(b"PK"));
    let equivalent=source.replace("beneathText","pageBottom");
    let html=temporary.path().join("equivalent.html");
    let(status,result)=crate::server::call_in_process(&app,"POST","/api/export",Some(&json!({"format":"html","content":equivalent,"baseDir":temporary.path(),"out_path":html,"options":options})));
    assert_eq!(status,200,"{result}");assert_eq!(result["ok"],true,"{result}");
    assert!(std::fs::read_to_string(html).unwrap().contains("Authored note"));
    assert!(!temporary.path().join("readmd-assets").exists());
}
