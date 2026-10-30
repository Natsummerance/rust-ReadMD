use super::*;
fn app(root:&Path)->Arc<App>{
    let assets=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").canonicalize().unwrap();
    Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("data"),root,&assets)).unwrap())
}
fn api(app:&Arc<App>,route:&str,body:&Value)->(u16,Value){crate::server::call_in_process(app,"POST",route,Some(body))}
fn open(app:&Arc<App>,path:&Path)->Value{
    let route=format!("/api/file?p={}",percent_encoding::utf8_percent_encode(&path.to_string_lossy(),percent_encoding::NON_ALPHANUMERIC));
    assert_eq!(crate::server::call_in_process(app,"GET",&route,None).0,200);
    let(code,value)=api(app,"/api/review",&json!({"action":"open","path":path}));assert_eq!(code,200,"{value}");value
}
fn propose(app:&Arc<App>,s:&Value,call:&str,from:usize,to:usize,insert:&str)->Value{
    let(code,next)=api(app,"/api/review",&json!({"action":"propose","document_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"call_id":call,"origin":"ai","edits":[{"from":from,"to":to,"insert":insert}]}));assert_eq!(code,200,"{next}");next
}
fn prepare(app:&Arc<App>,s:&Value,format:&str,call:&str)->Value{
    let(code,result)=api(app,"/api/export/snapshot",&json!({"action":"prepare","doc_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"format":format,"suggestedName":"Research.md","options":{},"call_id":call}));
    assert_eq!(code,200,"{result}");result
}
fn commit_to(app:&Arc<App>,s:&Value,path:&Path,accept:bool)->(u16,Value){api(app,"/api/export/snapshot",&json!({"action":"commit","snapshot_id":s["snapshot_id"],"accept_pending":accept,"out_path":path,"call_id":"export-commit","task_id":uuid::Uuid::new_v4().to_string()}))}

#[test]
fn actual_html_export_freezes_body_and_picture_and_accepts_only_old_change(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Research.md");
    fs::write(&path,"Old prose.\n\nC original.\n\n![Figure](figure.svg)\n").unwrap();
    let original_svg="<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><text y=\"15\">ORIGINAL</text></svg>";
    fs::write(dir.path().join("figure.svg"),original_svg).unwrap();let opened=open(&runtime,&path);
    let first=propose(&runtime,&opened,"first",0,3,"New");let frozen=prepare(&runtime,&first,"html","snapshot-first");
    assert_eq!(frozen["pending_count"],1);
    let later=propose(&runtime,&first,"second",12,13,"CHANGED");
    fs::write(dir.path().join("figure.svg"),original_svg.replace("ORIGINAL","LATER")).unwrap();
    let(code,preview)=api(&runtime,"/api/export/preview",&json!({"snapshot_id":frozen["snapshot_id"],"task_id":uuid::Uuid::new_v4().to_string()}));
    assert_eq!(code,200,"{preview}");assert_eq!(preview["format"],"html");assert_eq!(preview["mode"],"projection");
    use base64::Engine;
    let projected=preview["content"].as_str().unwrap();
    assert!(projected.contains("data:image/svg+xml;base64,"));assert!(projected.contains(&base64::engine::general_purpose::STANDARD.encode(original_svg)));
    assert!(projected.contains("New prose."));assert!(!projected.contains("CHANGED"));
    assert_eq!(runtime.review.handle(&json!({"action":"read","document_id":later["document_id"]})).unwrap()["changes"][0]["decision"],"pending");
    let output=dir.path().join("result.html");let(code,result)=commit_to(&runtime,&frozen,&output,true);assert_eq!(code,200,"{result}");assert_eq!(result["ok"],true,"{result}");
    let html=fs::read_to_string(&output).unwrap();assert!(html.contains("New prose."));assert!(!html.contains("CHANGED"));
    assert!(html.contains(&base64::engine::general_purpose::STANDARD.encode(original_svg)));
    let now=runtime.review.handle(&json!({"action":"read","document_id":later["document_id"]})).unwrap();
    assert_eq!(now["changes"][0]["decision"],"accepted");assert_eq!(now["changes"][1]["decision"],"pending");assert!(now["content"].as_str().unwrap().contains("CHANGED"));
    assert_eq!(fs::read_to_string(&path).unwrap(),"Old prose.\n\nC original.\n\n![Figure](figure.svg)\n");
    let(_,again)=commit_to(&runtime,&frozen,&output,true);assert_eq!(again["ok"],true);assert_eq!(again["review"]["review_revision"],now["review_revision"]);
}

#[test]
fn actual_pptx_preview_commit_reopen_and_consent_use_frozen_body_and_image(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let source=dir.path().join("研究.md");
    let original="# Research\n\nOld conclusion.\n\n---\n\n## Data\n\n| Item | Result |\n| --- | --- |\n| Sample | 42 |\n\n![Frozen](figure.png)\n";
    fs::write(&source,original).unwrap();let mut png=std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(4,2,image::Rgba([220,30,10,255]))).write_to(&mut png,image::ImageFormat::Png).unwrap();let png=png.into_inner();fs::write(dir.path().join("figure.png"),&png).unwrap();
    let opened=open(&runtime,&source);let offset=original.find("Old").unwrap();let changed=propose(&runtime,&opened,"pptx-proposal",offset,offset+3,"New");let frozen=prepare(&runtime,&changed,"pptx","pptx-prepare");
    let (status,preview)=api(&runtime,"/api/export/preview",&json!({"snapshot_id":frozen["snapshot_id"]}));assert_eq!(status,200,"{preview}");assert_eq!(preview["ok"],true);assert_eq!(preview["format"],"pptx");assert!(preview["html"].as_str().unwrap().contains("New conclusion."));
    assert_eq!(runtime.review.handle(&json!({"action":"read","document_id":opened["document_id"]})).unwrap()["changes"][0]["decision"],"pending");
    fs::write(dir.path().join("figure.png"),b"changed after freeze").unwrap();let target=dir.path().join("研究.pptx");
    let (status,denied)=commit_to(&runtime,&frozen,&target,false);assert_eq!(status,409,"{denied}");assert!(!target.exists());
    let wrong=dir.path().join("fake.html");let(status,_)=commit_to(&runtime,&frozen,&wrong,true);assert_ne!(status,200);assert!(!wrong.exists());
    let(status,saved)=commit_to(&runtime,&frozen,&target,true);assert_eq!(status,200,"{saved}");assert_eq!(saved["ok"],true);assert_eq!(saved["review"]["changes"][0]["decision"],"accepted");
    let bytes=fs::read(&target).unwrap();assert_eq!(&bytes[..4],b"PK\x03\x04");let entries=crate::convert::parse_zip_entries(&bytes).unwrap();assert!(entries.iter().any(|(name,data)|name.starts_with("ppt/media/")&&data==&png));assert!(entries.iter().any(|(name,data)|name.starts_with("ppt/slides/slide")&&String::from_utf8_lossy(data).contains("New conclusion.")));
    let imported=crate::convert::convert_verbose(target.to_str().unwrap(),true).unwrap();assert!(imported.success);let body=imported.content.unwrap();assert!(body.contains("New conclusion."));assert!(body.contains("Sample"));assert!(body.contains("42"));
    let(status,repeated)=commit_to(&runtime,&frozen,&target,true);assert_eq!(status,200,"{repeated}");assert_eq!(fs::read(&target).unwrap(),bytes);assert_eq!(fs::read_to_string(source).unwrap(),original);
}
#[test]
fn snapshot_requires_consent_and_failure_preserves_target_and_review(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"Old.").unwrap();
    let s=propose(&runtime,&open(&runtime,&path),"proposal",0,3,"New");let f=prepare(&runtime,&s,"html","prepare");let target=dir.path().join("target.html");fs::write(&target,b"OLD TARGET").unwrap();
    let(code,result)=commit_to(&runtime,&f,&target,false);assert_eq!(code,409);assert_eq!(result["error_code"],"export_snapshot_consent_required");assert_eq!(fs::read(&target).unwrap(),b"OLD TARGET");
    let invalid=dir.path().join("settings.json");assert_eq!(commit_to(&runtime,&f,&invalid,true).0,422);assert!(!invalid.exists());
    let current=runtime.review.handle(&json!({"action":"read","document_id":s["document_id"]})).unwrap();assert_eq!(current["changes"][0]["decision"],"pending");
}
#[test]
fn four_formats_export_real_artifacts_and_same_pending_block_stays_withdrawable(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"# Research\n\nOld body.\n").unwrap();
    let open=open(&runtime,&path);let start=open["content"].as_str().unwrap().find("Old").unwrap();let proposed=propose(&runtime,&open,"proposal",start,start+3,"New");
    let mut state=proposed;
    for fmt in ["html","pdf","docx","epub"]{
        let f=prepare(&runtime,&state,fmt,&format!("prepare-{fmt}"));let out=dir.path().join(format!("result.{fmt}"));let(code,result)=commit_to(&runtime,&f,&out,true);
        assert_eq!(code,200,"{fmt}: {result}");assert_eq!(result["ok"],true,"{fmt}: {result}");let bytes=fs::read(&out).unwrap();
        match fmt{"pdf"=>{let pdf=lopdf::Document::load_mem(&bytes).unwrap();assert!(!pdf.get_pages().is_empty());},"docx"|"epub"=>assert!(bytes.starts_with(b"PK")),_=>assert!(String::from_utf8(bytes).unwrap().contains("New body."))}
        state=result["review"].clone();
    }
    let(code,withdrawn)=api(&runtime,"/api/review",&json!({"action":"resolve","document_id":state["document_id"],"generation":state["generation"],"review_revision":state["review_revision"],"call_id":"withdraw-exported","decision":"withdraw","ids":[state["changes"][0]["id"]]}));assert_eq!(code,200,"{withdrawn}");assert!(withdrawn["content"].as_str().unwrap().contains("Old body."));
}
#[test]
fn legacy_export_of_known_pending_document_cannot_bypass_snapshot(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"Old.").unwrap();let s=propose(&runtime,&open(&runtime,&path),"proposal",0,3,"New");
    let(code,result)=api(&runtime,"/api/export",&json!({"format":"html","content":s["content"],"baseDir":dir.path(),"suggestedName":"Source.md","out_path":dir.path().join("x.html")}));assert_eq!(code,409,"{result}");assert!(!dir.path().join("x.html").exists());
}
#[cfg(windows)]
#[test]
fn actual_locked_destination_never_accepts_or_overwrites(){
    use std::os::windows::fs::OpenOptionsExt;
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"Old.").unwrap();let s=propose(&runtime,&open(&runtime,&path),"proposal",0,3,"New");let f=prepare(&runtime,&s,"html","prepare");let target=dir.path().join("locked.html");fs::write(&target,b"PROTECTED").unwrap();
    let lock=fs::OpenOptions::new().read(true).share_mode(0).open(&target).unwrap();let(code,_)=commit_to(&runtime,&f,&target,true);assert_ne!(code,200);drop(lock);assert_eq!(fs::read(&target).unwrap(),b"PROTECTED");
    assert_eq!(runtime.review.handle(&json!({"action":"read","document_id":s["document_id"]})).unwrap()["changes"][0]["decision"],"pending");
}

#[test]
fn prepare_retry_keeps_frozen_source_and_survives_explicit_discard(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"# Original heading\n").unwrap();let s=open(&runtime,&path);
    let body=json!({"action":"prepare","doc_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"format":"html","options":{"meta":{"title":"Stable"}},"call_id":"stable-prepare"});
    let(code,first)=api(&runtime,"/api/export/snapshot",&body);assert_eq!(code,200,"{first}");
    let(_,later)=api(&runtime,"/api/review",&json!({"action":"edit","document_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"call_id":"new-live-profile","origin":"editor","edits":[{"from":0,"to":0,"insert":"---\ntitle: Later live title\n---\n"}]}));assert!(later["content"].as_str().unwrap().contains("Later"));
    let(code,repeated)=api(&runtime,"/api/export/snapshot",&body);assert_eq!(code,200);assert_eq!(repeated["snapshot_id"],first["snapshot_id"]);assert_eq!(repeated["content"],first["content"]);
    let(_,discarded)=api(&runtime,"/api/export/snapshot",&json!({"action":"discard","snapshot_id":first["snapshot_id"]}));assert_eq!(discarded["discarded"],true);
    let(code,repeated)=api(&runtime,"/api/export/snapshot",&body);assert_eq!(code,200);assert_eq!(repeated["snapshot_id"],first["snapshot_id"]);assert_eq!(repeated["state"],"cancelled");assert!(repeated.get("content").is_none());
}

#[test]
fn tex_snapshot_publishes_an_actual_frozen_resource_bundle_and_reconciles_it(){
    use base64::Engine;
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"# Research\n\nOld prose.\n\n![Figure](figure.png)\n").unwrap();
    let pixels=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jA1cAAAAASUVORK5CYII=").unwrap();fs::write(dir.path().join("figure.png"),&pixels).unwrap();let s=open(&runtime,&path);let start=s["content"].as_str().unwrap().find("Old").unwrap();let s=propose(&runtime,&s,"tex-proposal",start,start+3,"New");let f=prepare(&runtime,&s,"tex","tex-prepare");
    fs::write(dir.path().join("figure.png"),b"later invalid image").unwrap();let target=dir.path().join("Research.tex");let(code,result)=commit_to(&runtime,&f,&target,true);assert_eq!(code,200,"{result}");assert_eq!(result["ok"],true,"{result}");
    let text=fs::read_to_string(&target).unwrap();assert!(text.contains("New prose."));assert!(text.contains("readmd-assets/export-"));
    let store=service(&runtime);let meta=store.get(f["snapshot_id"].as_str().unwrap()).unwrap();let dependencies=meta["output"]["dependencies"].as_array().unwrap();assert!(!dependencies.is_empty());assert!(dependencies.iter().any(|dependency|fs::read(dependency["path"].as_str().unwrap()).unwrap()==pixels));
    let(code,repeated)=api(&runtime,"/api/export/snapshot",&json!({"action":"commit","snapshot_id":f["snapshot_id"],"out_path":target,"accept_pending":false,"overwrite":false}));assert_eq!(code,200);assert_eq!(repeated["ok"],true);
    fs::write(dependencies[0]["path"].as_str().unwrap(),b"external change").unwrap();let(_,repeated)=api(&runtime,"/api/export/snapshot",&json!({"action":"reconcile","snapshot_id":f["snapshot_id"]}));assert_eq!(repeated["ok"],false);assert_eq!(fs::read_to_string(&target).unwrap(),text);
}

#[test]
fn preview_cache_identity_changes_when_late_bound_figures_change(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"# Preview\n\nBody.\n").unwrap();let s=open(&runtime,&path);let meta=prepare(&runtime,&s,"pdf","preview-prepare");let store=service(&runtime);
    let before=store.load(meta["snapshot_id"].as_str().unwrap()).unwrap();let mut after=store.load(meta["snapshot_id"].as_str().unwrap()).unwrap();assert_eq!(crate::export_preview::snapshot_key(&before),crate::export_preview::snapshot_key(&after));
    after.options["figures"]=json!({"rendered":[{"key":"owned","svg":"<svg xmlns=\"http://www.w3.org/2000/svg\"><text>New label</text></svg>"}]});after.resolved_options["canonical"]=after.options.clone();assert_ne!(crate::export_preview::snapshot_key(&before),crate::export_preview::snapshot_key(&after));
}

#[test]
fn committed_pdf_is_the_exact_actual_snapshot_preview_artifact(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let path=dir.path().join("Source.md");fs::write(&path,"# Preview\n\nOld body.\n").unwrap();let s=open(&runtime,&path);let from=s["content"].as_str().unwrap().find("Old").unwrap();let s=propose(&runtime,&s,"preview-proposal",from,from+3,"New");let meta=prepare(&runtime,&s,"pdf","pdf-preview-prepare");let frozen=service(&runtime).load(meta["snapshot_id"].as_str().unwrap()).unwrap();
    let preview=crate::export_preview::prepare_snapshot(&frozen,&runtime.paths.data_dir).unwrap();let preview_bytes=fs::read(&preview.file).unwrap();assert!(!lopdf::Document::load_mem(&preview_bytes).unwrap().get_pages().is_empty());
    let target=dir.path().join("Preview.pdf");let(code,result)=commit_to(&runtime,&meta,&target,true);assert_eq!(code,200,"{result}");assert_eq!(result["ok"],true);assert_eq!(fs::read(&target).unwrap(),preview_bytes);
}

#[test]
fn html_export_recovers_poisoned_save_lock_in_an_isolated_process() {
    use std::{process::{Command, Stdio}, time::{Duration, Instant}};
    // Poisoning the shared test process would change unrelated test results.
    // A marker also proves that the exact child test ran, rather than a typo
    // in its name silently matching zero tests and exiting successfully.
    let root=tempfile::Builder::new().prefix("readmd-export-poison-probe-").tempdir().unwrap();
    let token=uuid::Uuid::new_v4().to_string();
    fs::write(root.path().join("probe.marker"),token.as_bytes()).unwrap();
    let mut child=Command::new(std::env::current_exe().unwrap())
        .arg("--exact").arg("export_snapshot_api::tests::save_lock_poison_html_export_child")
        .arg("--ignored").arg("--test-threads").arg("1")
        .env("READMD_EXPORT_POISON_PROBE_ROOT",root.path())
        .env("READMD_EXPORT_POISON_PROBE_TOKEN",&token)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let started=Instant::now();
    let status=loop {
        match child.try_wait() {
            Ok(Some(status))=>break status,
            Ok(None)=>(),
            Err(_)=>{
                let _=child.kill();let _=child.wait();
                panic!("Could not inspect the owned synthetic export probe");
            }
        }
        if started.elapsed()>Duration::from_secs(60) {
            let _=child.kill();let _=child.wait();
            panic!("Synthetic export poison probe timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(),"Synthetic export poison probe failed: {status}");
    assert_eq!(fs::read(root.path().join("probe.completed")).unwrap(),token.as_bytes());
}

#[test]
#[ignore = "Spawned only by the owned synthetic export poison test"]
fn save_lock_poison_html_export_child() {
    let Some(root)=std::env::var_os("READMD_EXPORT_POISON_PROBE_ROOT").map(PathBuf::from) else {return;};
    let token=std::env::var("READMD_EXPORT_POISON_PROBE_TOKEN").unwrap();
    assert!(root.is_absolute());
    assert!(root.file_name().unwrap().to_string_lossy().starts_with("readmd-export-poison-probe-"));
    assert!(uuid::Uuid::parse_str(&token).is_ok());
    assert_eq!(fs::read(root.join("probe.marker")).unwrap(),token.as_bytes());
    let scenario=root.join("scenario");fs::create_dir(&scenario).unwrap();
    let runtime=app(&scenario);let source=scenario.join("Source.md");
    fs::write(&source,"Old synthetic body.\n").unwrap();
    let opened=open(&runtime,&source);
    let proposed=propose(&runtime,&opened,"poison-proposal",0,3,"New");
    let change_id=proposed["changes"][0]["id"].clone();
    let frozen=prepare(&runtime,&proposed,"html","poison-prepare");
    assert_eq!(frozen["pending_count"],1);

    let poison=std::thread::spawn(|| {
        let _saving=crate::document_history::SAVE_LOCK.lock().unwrap();
        panic!("Controlled synthetic save-lock poison");
    });
    assert!(poison.join().is_err());
    assert!(crate::document_history::SAVE_LOCK.is_poisoned());

    let output=scenario.join("Export.html");
    let (status,result)=commit_to(&runtime,&frozen,&output,true);
    assert_eq!(status,200,"{result}");assert_eq!(result["ok"],true);
    assert_eq!(result["snapshot_state"],"finished");
    let exported=fs::read(&output).unwrap();
    assert!(String::from_utf8(exported.clone()).unwrap().contains("New synthetic body."));
    assert_eq!(fs::read_to_string(&source).unwrap(),"Old synthetic body.\n");
    let accepted=runtime.review.handle(&json!({"action":"read","document_id":proposed["document_id"]})).unwrap();
    assert_eq!(accepted["changes"].as_array().unwrap().len(),1);
    assert_eq!(accepted["changes"][0]["id"],change_id);
    assert_eq!(accepted["changes"][0]["decision"],"accepted");
    assert_eq!(accepted["content"],"New synthetic body.\n");
    let reconciled=service(&runtime).reconcile(&runtime.review,frozen["snapshot_id"].as_str().unwrap()).unwrap();
    assert_eq!(reconciled["state"],"finished");assert_eq!(reconciled["artifact_current"],true);

    let (status,repeated)=commit_to(&runtime,&frozen,&output,true);
    assert_eq!(status,200,"{repeated}");assert_eq!(repeated["ok"],true);
    assert_eq!(repeated["review"]["review_revision"],accepted["review_revision"]);
    assert_eq!(fs::read(&output).unwrap(),exported);
    let (status,withdrawn)=api(&runtime,"/api/review",&json!({"action":"resolve","document_id":accepted["document_id"],
        "generation":accepted["generation"],"review_revision":accepted["review_revision"],"call_id":"poison-withdraw",
        "decision":"withdraw","ids":[change_id]}));
    assert_eq!(status,200,"{withdrawn}");assert_eq!(withdrawn["content"],"Old synthetic body.\n");
    assert_eq!(fs::read(&output).unwrap(),exported);
    assert_eq!(fs::read_to_string(&source).unwrap(),"Old synthetic body.\n");
    fs::write(root.join("probe.completed"),token.as_bytes()).unwrap();
}
