//! Authorized snapshot/export adapter. Rendering never reads live source assets.
use crate::{server::{Request,Response}, ApiError, ApiResult, App};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,io::Read,path::{Path,PathBuf},sync::Arc};

fn error(code:&'static str)->ApiError{
    let status=if code.contains("not_authorized"){403}else if code.contains("persistence"){507}
        else if code.contains("budget"){413}else if code.contains("conflict")||code.contains("stale")||code.contains("recovery")||code.contains("unavailable")||code.contains("required"){409}else{422};
    ApiError::new(status,code)
}
fn service(app:&App)->crate::export_snapshot::Service{crate::export_snapshot::Service::new(&app.paths.data_dir)}
fn read(app:&Arc<App>,id:&Value)->ApiResult<Value>{
    let snapshot=app.review.handle(&json!({"action":"read","document_id":id})).map_err(error)?;
    let path=snapshot["path"].as_str().ok_or_else(||error("review_invalid_request"))?;
    if !path.starts_with("draft:")&&crate::server::review_authorized_path(path).is_none(){return Err(error("review_not_authorized"));}
    Ok(snapshot)
}
fn authorized_snapshot(app:&Arc<App>,id:&str)->ApiResult<Value>{
    let meta=service(app).get(id).map_err(error)?;read(app,&meta["document_id"])?;Ok(meta)
}
pub fn handle(app:&Arc<App>,req:&Request)->ApiResult<Response>{
    if req.method!="POST"{return Err(ApiError::new(405,"method_not_allowed"));}
    let body=req.json()?;Ok(Response::json(&dispatch(app,&body)?))
}
pub(crate) fn dispatch(app:&Arc<App>,body:&Value)->ApiResult<Value>{
    match body["action"].as_str().unwrap_or(""){
        "prepare"=>{
            if body.as_object().is_none_or(|object|object.keys().any(|key|!["action","document_id","doc_id","generation","review_revision","format","options","suggestedName","suggested_name","call_id"].contains(&key.as_str()))){return Err(error("review_not_authorized"));}
            let document=body.get("document_id").or_else(||body.get("doc_id")).ok_or_else(||error("review_invalid_request"))?;
            if body.get("document_id").is_some()&&body.get("doc_id").is_some()&&body["document_id"]!=body["doc_id"]{return Err(error("review_not_authorized"));}
            let format=body["format"].as_str().unwrap_or("");
            let options=body.get("options").cloned().unwrap_or_else(||json!({}));
            let name=body.get("suggestedName").or_else(||body.get("suggested_name")).and_then(Value::as_str).unwrap_or("document.md");
            let public=json!({"document_id":document,"generation":body["generation"],"review_revision":body["review_revision"],"format":format,"options":options,"suggested_name":name});
            let identity=hash(&serde_json::to_vec(&canonical(&public)).map_err(|_|error("review_invalid_request"))?);
            let call=body["call_id"].as_str().ok_or_else(||error("review_invalid_request"))?;
            if let Some(mut meta)=service(app).lookup_prepare(call,&identity).map_err(error)?{
                read(app,&meta["document_id"])?;
                if meta["state"]=="prepared"&&meta["discarded"]!=true{
                    let frozen=service(app).load(meta["snapshot_id"].as_str().ok_or_else(||error("review_invalid_request"))?).map_err(error)?;
                    meta["content"]=json!(frozen.content);meta["options"]=frozen.options;
                }
                return Ok(meta);
            }
            let snapshot=read(app,document)?;
            let content=snapshot["content"].as_str().ok_or_else(||error("review_invalid_request"))?;
            let resolved=crate::publication_profile::resolve_for_format(format,content,&options).map_err(|e|ApiError::new(422,&e.code))?;
            crate::publication_profile::preflight(format,content,resolved.values()).map_err(|e|ApiError::new(422,&e.code))?;
            let request=json!({"document_id":snapshot["document_id"],"generation":body["generation"],"review_revision":body["review_revision"],
                "call_id":body["call_id"],"format":format,"suggested_name":name,"prepare_identity":identity,"options":resolved.values(),"resolved_options":serde_json::to_value(resolved.clone()).map_err(|_|error("review_invalid_request"))?});
            let mut meta=service(app).prepare(&app.review,&request,&app.paths.assets_dir).map_err(error)?;
            let frozen=service(app).load(meta["snapshot_id"].as_str().ok_or_else(||error("review_invalid_request"))?).map_err(error)?;
            meta["content"]=json!(frozen.content);meta["options"]=frozen.options;Ok(meta)
        },
        "status"=>authorized_snapshot(app,identifier(body)?),
        "runtime"=>{let id=identifier(body)?;authorized_snapshot(app,id)?;service(app).bind_runtime(id,&app.paths.assets_dir).map_err(error)},
        "figures"=>{let id=identifier(body)?;authorized_snapshot(app,id)?;service(app).complete_figures(id,&body["rendered"]).map_err(error)},
        "discard"=>{let id=identifier(body)?;authorized_snapshot(app,id)?;service(app).discard(id).map_err(error)},
        "reconcile"=>{let id=identifier(body)?;authorized_snapshot(app,id)?;let meta=service(app).reconcile(&app.review,id).map_err(error)?;finish_response(app,meta,None)},
        "commit"=>commit(app,body,crate::native_dialogs::run),
        _=>Err(error("review_invalid_request")),
    }
}
fn identifier(body:&Value)->ApiResult<&str>{body["snapshot_id"].as_str().filter(|s|!s.is_empty()).ok_or_else(||error("review_invalid_request"))}
pub(crate) fn preview(app:&Arc<App>,body:&Value)->ApiResult<Value>{
    let id=identifier(body)?;let meta=authorized_snapshot(app,id)?;if meta["figures_complete"]!=true{return Err(error("diagram_client_renderer_required"));}
    let store=service(app);let frozen=store.load(id).map_err(error)?;store.validate_runtime(&frozen,&app.paths.assets_dir).map_err(error)?;
    let result=if frozen.format=="epub"{
        crate::epub_preview::prepare(&frozen.content,&frozen.base_dir.to_string_lossy(),&frozen.options,&frozen.name)
    }else if frozen.format=="tex"{
        crate::export_preview::source_snapshot(&frozen,&app.paths.data_dir)
    }else if frozen.format=="presentation"{
        crate::export_preview::presentation_snapshot(&frozen)
    }else{
        crate::export_preview::prepare_snapshot(&frozen,&app.paths.data_dir).and_then(|artifact|crate::export_preview::payload(&artifact))
    }.map_err(|code|ApiError::new(422,crate::latex_compile::error_body(&code)["error_code"].as_str().unwrap_or("export_snapshot_render_failed")))?;
    store.validate_runtime(&frozen,&app.paths.assets_dir).map_err(error)?;Ok(result)
}
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn canonical(value:&Value)->Value{
    match value{
        Value::Object(object)=>{let sorted=object.iter().map(|(key,value)|(key.clone(),canonical(value))).collect::<std::collections::BTreeMap<_,_>>();serde_json::to_value(sorted).unwrap_or(Value::Null)},
        Value::Array(items)=>Value::Array(items.iter().map(canonical).collect()),
        _=>value.clone(),
    }
}
fn file_hash(path:&Path)->ApiResult<Option<String>>{
    match fs::File::open(path){
        Ok(file)=>{if !file.metadata().map_err(|_|error("export_snapshot_output_unverified"))?.is_file(){return Err(error("export_snapshot_output_unverified"));}
            let mut data=Vec::new();file.take((crate::export_snapshot::MAX_SNAPSHOT_BYTES+1)as u64).read_to_end(&mut data).map_err(|_|error("export_snapshot_output_unverified"))?;
            if data.len()>crate::export_snapshot::MAX_SNAPSHOT_BYTES{return Err(error("export_snapshot_budget"));}Ok(Some(hash(&data)))},
        Err(e)if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),
        Err(_)=>Err(error("export_snapshot_output_unverified")),
    }
}
fn target(app:&App,raw:&str,format:&str)->ApiResult<PathBuf>{
    let path=Path::new(raw);if !path.is_absolute(){return Err(error("review_not_authorized"));}
    let extension=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    let extension_format=if format=="presentation"{"html"}else{format};
    if extension!=extension_format&&!(extension_format=="html"&&extension=="htm"){return Err(error("export_snapshot_invalid_request"));}
    let parent=dunce::canonicalize(path.parent().ok_or_else(||error("review_not_authorized"))?).map_err(|_|error("export_snapshot_output_unverified"))?;
    let output=parent.join(path.file_name().ok_or_else(||error("review_not_authorized"))?);
    let inside=|root:&Path|{let root=dunce::canonicalize(root).unwrap_or_else(|_|dunce::simplified(root).to_path_buf());if cfg!(windows){let child=output.to_string_lossy().to_ascii_lowercase();let root=root.to_string_lossy().trim_end_matches(['\\','/']).to_ascii_lowercase();child==root||child.starts_with(&(root+"\\"))}else{output.starts_with(root)}};
    if inside(&app.paths.data_dir)||inside(&app.paths.assets_dir){return Err(error("review_not_authorized"));}
    if let Ok(meta)=fs::symlink_metadata(&output){if meta.file_type().is_symlink()||!meta.is_file(){return Err(error("review_not_authorized"));}}
    Ok(output)
}
pub(crate) fn commit(app:&Arc<App>,body:&Value,pick:impl FnOnce(&crate::native_dialogs::Request)->crate::native_dialogs::DialogOutcome)->ApiResult<Value>{
    let id=identifier(body)?;let meta=authorized_snapshot(app,id)?;
    if meta["state"]=="prepared"&&meta["pending_count"].as_u64().unwrap_or(0)>0&&body["accept_pending"]!=true{return Err(error("export_snapshot_consent_required"));}
    let store=service(app);
    if meta["state"]!="prepared"{
        if let Some(requested)=body.get("out_path").or_else(||body.get("outPath")).and_then(Value::as_str).filter(|s|!s.is_empty()){
            let recorded=meta.pointer("/output/target").and_then(Value::as_str).ok_or_else(||error("export_snapshot_commit_conflict"))?;
            let normalized=target(app,requested,meta["format"].as_str().unwrap_or(""))?;
            let equal=if cfg!(windows){normalized.to_string_lossy().eq_ignore_ascii_case(&dunce::simplified(Path::new(recorded)).to_string_lossy())}else{normalized==Path::new(recorded)};
            if !equal{return Err(error("export_snapshot_commit_conflict"));}
        }
        let done=store.reconcile(&app.review,id).map_err(error)?;return finish_response(app,done,None);
    }
    if meta["runtime_ready"]==false{store.bind_runtime(id,&app.paths.assets_dir).map_err(error)?;}
    let frozen=store.load(id).map_err(error)?;store.validate_runtime(&frozen,&app.paths.assets_dir).map_err(error)?;
    let task_id=body["task_id"].as_str().filter(|s|crate::cancel::valid_id(s)).map(str::to_string).unwrap_or_else(||uuid::Uuid::new_v4().to_string());
    let guard=crate::cancel::register(&task_id).ok_or_else(||error("review_invalid_request"))?;
    let raw=body.get("out_path").or_else(||body.get("outPath")).and_then(Value::as_str).unwrap_or("");
    let raw=if raw.is_empty(){
        use crate::native_dialogs::{DialogOutcome,DialogShape,Request as Dialog};
        let request=Dialog::new(DialogShape::Export).name(&frozen.name).export(&frozen.format);
        match pick(&request){DialogOutcome::Picked(paths)if !paths.is_empty()=>paths[0].clone(),DialogOutcome::Unavailable=>return Err(error("dialog_unavailable")),_=>return Ok(json!({"ok":false,"canceled":true}))}
    }else{raw.to_string()};
    if guard.is_cancelled(){return Ok(json!({"ok":false,"error_code":"cancelled","canceled":true}));}
    let output=target(app,&raw,&frozen.format)?;let before=file_hash(&output)?;
    if body["overwrite"]==false&&before.is_some(){return Err(error("export_snapshot_commit_conflict"));}
    let work=tempfile::Builder::new().prefix("export-render-").tempdir_in(&app.paths.data_dir).map_err(|_|error("export_snapshot_persistence_failed"))?;
    let artifact=work.path().join(output.file_name().ok_or_else(||error("export_snapshot_invalid_request"))?);
    let resolved:crate::publication_typography::ResolvedOptions=serde_json::from_value(frozen.resolved_options.clone()).map_err(|_|error("export_snapshot_invalid_request"))?;
    if resolved.values()!=&frozen.options{return Err(error("export_snapshot_invalid_request"));}
    let cached=crate::export_preview::cached(&crate::export_preview::snapshot_key(&frozen));
    let rendered=if let Some(cached)=cached{
        fs::copy(&cached.file,&artifact).map_err(|_|"export_snapshot_render_failed".to_string()).map(|size|crate::mdexport::ExportResult{ok:true,path:Some(artifact.to_string_lossy().into_owned()),size:Some(size),warns:Some(cached.warnings.clone()),error:None,canceled:Some(false)})
    }else{crate::speech::with_cancel(guard.checker(),||crate::mdexport::export_document_with_resolved_options(&frozen.format,&frozen.content,&frozen.base_dir.to_string_lossy(),&artifact.to_string_lossy(),&resolved,&frozen.name,&frozen.assets_dir,&app.paths.data_dir))};
    let rendered=match rendered{Ok(result)if result.ok=>result,Ok(_)=>return Err(error("export_snapshot_render_failed")),Err(message)=>{
        let code=crate::latex_compile::error_body(&message)["error_code"].as_str().unwrap_or("export_snapshot_render_failed").to_string();
        return Ok(json!({"ok":false,"error_code":code}));
    }};
    let mut bytes=fs::read(&artifact).map_err(|_|error("export_snapshot_render_failed"))?;
    if bytes.len()>crate::export_snapshot::MAX_SNAPSHOT_BYTES{return Err(error("export_snapshot_budget"));}
    store.validate_runtime(&frozen,&app.paths.assets_dir).map_err(error)?;
    let mut dependencies=Vec::<(PathBuf,String)>::new();let mut dependency_bytes=Vec::new();let mut bundle_bytes=bytes.len();let mut resource_destination=None;
    if frozen.format=="tex"{
        let stem=output.file_stem().and_then(|s|s.to_str()).ok_or_else(||error("export_snapshot_invalid_request"))?;
        let folder=work.path().join(format!("{stem}.assets"));
        if folder.is_dir(){
            let resource_root=output.parent().unwrap().join("readmd-assets");
            if fs::symlink_metadata(&resource_root).is_ok_and(|meta|meta.file_type().is_symlink()){return Err(error("review_not_authorized"));}
            let destination=resource_root.join(format!("export-{id}"));resource_destination=Some(destination.clone());
            // TeX writer produces flat, owned resources. Reject unexpected
            // directories rather than silently publishing only half a bundle.
            for entry in fs::read_dir(&folder).map_err(|_|error("export_snapshot_render_failed"))?{
                let entry=entry.map_err(|_|error("export_snapshot_render_failed"))?;if !entry.file_type().map_err(|_|error("export_snapshot_render_failed"))?.is_file(){return Err(error("export_snapshot_render_failed"));}
                let data=fs::read(entry.path()).map_err(|_|error("export_snapshot_render_failed"))?;
                bundle_bytes=bundle_bytes.checked_add(data.len()).ok_or_else(||error("export_snapshot_budget"))?;
                if bundle_bytes>crate::export_snapshot::MAX_SNAPSHOT_BYTES{return Err(error("export_snapshot_budget"));}
                if data.len()>16*1024*1024||dependencies.len()>=256{return Err(error("export_snapshot_budget"));}
                let path=destination.join(entry.file_name());dependencies.push((path.clone(),hash(&data)));dependency_bytes.push((path,data));
            }
            let text=String::from_utf8(bytes).map_err(|_|error("export_snapshot_render_failed"))?;
            bytes=text.replace(&format!("{stem}.assets/"),&format!("readmd-assets/export-{id}/")).into_bytes();
        }
    }
    // This mutex only orders writers. Recover its unit guard like the other
    // save services; the journal and target checks below validate disk state.
    let _saving=crate::document_history::SAVE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    if file_hash(&output)?!=before{return Err(error("export_snapshot_commit_conflict"));}
    if guard.is_cancelled(){return Ok(json!({"ok":false,"canceled":true,"error_code":"cancelled"}));}
    // Cancel is finalized before recording an output intent. Afterwards the
    // journal precedes every write and survives any following I/O failure.
    if !guard.try_commit(){return Ok(json!({"ok":false,"canceled":true,"error_code":"cancelled"}));}
    store.begin_commit_bundle(id,&output,before.as_deref(),&hash(&bytes),&dependencies).map_err(error)?;
    if file_hash(&output)?!=before{return Err(error("export_snapshot_commit_conflict"));}
    if let Some(destination)=resource_destination{
        let parent=destination.parent().unwrap();fs::create_dir_all(parent).map_err(|_|error("export_snapshot_persistence_failed"))?;
        if fs::symlink_metadata(parent).map_err(|_|error("export_snapshot_persistence_failed"))?.file_type().is_symlink(){return Err(error("review_not_authorized"));}
        fs::create_dir(&destination).map_err(|_|error("export_snapshot_commit_conflict"))?;
    }
    for(path,data)in dependency_bytes{
        let mut file=fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|_|error("export_snapshot_persistence_failed"))?;
        use std::io::Write;file.write_all(&data).and_then(|_|file.sync_all()).map_err(|_|error("export_snapshot_persistence_failed"))?;
    }
    if before.is_none(){
        use std::io::Write;let mut temp=tempfile::Builder::new().prefix(".readmd-export-").tempfile_in(output.parent().unwrap()).map_err(|_|error("export_snapshot_persistence_failed"))?;
        temp.write_all(&bytes).and_then(|_|temp.as_file().sync_all()).map_err(|_|error("export_snapshot_persistence_failed"))?;
        temp.persist_noclobber(&output).map_err(|_|error("export_snapshot_output_unverified"))?;
    }else{crate::content::write_bytes_atomic(&output,&bytes).map_err(|_|error("export_snapshot_output_unverified"))?;}
    finish_response(app,store.finish(&app.review,id).map_err(error)?,rendered.warns)
}
fn finish_response(app:&Arc<App>,meta:Value,warnings:Option<Vec<String>>)->ApiResult<Value>{
    let finished=matches!(meta["state"].as_str(),Some("finished"|"acceptance_conflict"));
    let target=meta.pointer("/output/target").cloned().unwrap_or(Value::Null);
    let snapshot=read(app,&meta["document_id"])?;
    let size=target.as_str().and_then(|p|fs::metadata(p).ok()).map(|m|m.len());
    // The store checks all paths without following redirected/reparse entries.
    // A same-hash file behind a replacement symlink is not this output.
    let output_verified=meta["artifact_current"]==true;
    let mut result=json!({"ok":output_verified,"path":target,"size":size,"snapshot_id":meta["snapshot_id"],"snapshot_state":meta["state"],
        "warns":warnings.unwrap_or_default(),"review":snapshot,"review_conflicts":meta.pointer("/receipt/conflict_ids").cloned().unwrap_or_else(||json!([])),
        "finalization_pending":!finished&&output_verified,"review_receipt":meta.get("receipt"),"error_code":meta.get("error_code")});
    if output_verified {result.as_object_mut().unwrap().remove("error_code");}else{result["error_code"]=json!("export_snapshot_output_unverified");}
    crate::api_codes::attach_warn_items(&mut result);Ok(result)
}

/// Legacy callers cannot export a known reviewed document behind the user's
/// consent. Explicitly unrelated raw Markdown remains a supported legacy input.
pub(crate) fn legacy_guard(app:&Arc<App>,body:&Value)->ApiResult<()>{
    if !app.paths.data_dir.join("review.sqlite3").is_file(){return Ok(());}
    let explicit=body.get("review_session").and_then(|s|s.get("document_id")).or_else(||body.get("document_id"));
    let source_path=body.get("source_path").and_then(Value::as_str);
    let found=if let Some(id)=explicit{Some(read(app,id)?)}else if let Some(path)=source_path{
        if !path.starts_with("draft:")&&crate::server::review_authorized_path(path).is_none(){return Err(error("review_not_authorized"));}
        let lookup=app.review.handle(&json!({"action":"lookup","path":path})).map_err(error)?;
        if lookup["found"]==true{return Err(error("export_snapshot_consent_required"));}None
    }else{
        let name=body.get("suggestedName").or_else(||body.get("suggested_name")).and_then(Value::as_str);
        let base=body.get("baseDir").or_else(||body.get("base_dir")).and_then(Value::as_str);
        if let(Some(name),Some(base))=(name,base){
            let path=Path::new(base).join(name);if let Some(path)=crate::server::review_authorized_path(&path.to_string_lossy()){
                let lookup=app.review.handle(&json!({"action":"lookup","path":path.to_string_lossy()})).map_err(error)?;
                if lookup["found"]==true{Some(read(app,&lookup["snapshot"]["document_id"])?) }else{None}
            }else{None}
        }else{None}
    };
    if found.is_some_and(|s|s["changes"].as_array().is_some_and(|all|all.iter().any(|c|c["decision"]=="pending"||c["decision"]=="conflict"))){return Err(error("export_snapshot_consent_required"));}
    Ok(())
}

#[cfg(test)]
#[path="export_snapshot_api_tests.rs"]
mod tests;
