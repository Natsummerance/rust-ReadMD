//! Preview and export share generated artifacts keyed by identical inputs.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::VecDeque,path::{Path,PathBuf},sync::{Arc,Mutex,OnceLock},time::{Duration,Instant}};
pub struct Artifact {
    pub key:String,pub file:PathBuf,pub pdf:PathBuf,pub format:String,pub warnings:Vec<String>,pub projection_engine:String,pub source_map:Option<crate::publication_pagination::PageMap>,
    _directory:tempfile::TempDir,created:Instant,
}
fn cache()-> &'static Mutex<VecDeque<Arc<Artifact>>> {
    static CACHE:OnceLock<Mutex<VecDeque<Arc<Artifact>>>>=OnceLock::new();CACHE.get_or_init(||Mutex::new(VecDeque::new()))
}
pub fn key(format:&str,content:&str,base:&str,options:&Value,name:&str)->String {
    key_with_data(format,content,base,options,name,&crate::paths::data_dir(),&crate::paths::assets_dir())
}
pub fn key_with_data(format:&str,content:&str,base:&str,options:&Value,name:&str,data:&Path,assets:&Path)->String{
    key_with_data_checked(format,content,base,options,name,data,assets)
        .unwrap_or_else(|_|format!("uncached:{}",uuid::Uuid::new_v4()))
}
pub(crate) fn key_with_data_checked(format:&str,content:&str,base:&str,options:&Value,name:&str,data:&Path,assets:&Path)->Result<String,String>{
    crate::publication_budget::run(format,options,||key_checked(format,content,base,options,name,data,assets))
}
fn key_checked(format:&str,content:&str,base:&str,options:&Value,name:&str,data:&Path,assets:&Path)->Result<String,String>{
    let budget=crate::publication_budget::capture();let checked=||budget.api();checked().map_err(|error|error.code)?;
    let mut digest=Sha256::new();let options=serde_json::to_vec(options).unwrap_or_default();
    for value in [format.as_bytes(),content.as_bytes(),base.as_bytes(),options.as_slice(),name.as_bytes()] {
        digest.update((value.len() as u64).to_le_bytes());digest.update(value);
    }
    let selected:Value=serde_json::from_slice(&options).unwrap_or(Value::Null);
    if matches!(format,"pdf"|"docx"){
        let fonts=match crate::capability_packs::leased_font_resources_checked(data,assets,&checked){
            Ok((resources,_lease))=>resources.fingerprint,
            Err(error) if budget.interrupted(&error.code)=>return Err(error.code),
            Err(_)=>"core-fonts-only".into(),
        };
        digest.update(crate::publication_fonts::MATH);digest.update(crate::publication_fonts::CORE_TEXT_FALLBACK);digest.update(fonts.as_bytes());
    }
    if format!="docx"&&selected.pointer("/pdf/engine").and_then(Value::as_str)==Some("typst"){
        let engine=match crate::capability_packs::typst_engine_checked(data,assets,&checked){
            Ok((_,digest))=>digest,
            Err(error) if budget.interrupted(&error.code)=>return Err(error.code),
            Err(_)=>uuid::Uuid::new_v4().to_string(),
        };
        digest.update(data.to_string_lossy().as_bytes());digest.update(engine.as_bytes());
    }
    if format=="pdf"&&matches!(selected.pointer("/pdf/engine").and_then(Value::as_str),Some("xelatex"|"pdflatex")){
        let engine=match crate::capability_packs::runtime_revision_checked(data,assets,"latex",&checked){
            Ok(digest)=>digest,
            Err(error) if budget.interrupted(&error.code)=>return Err(error.code),
            Err(_)=>uuid::Uuid::new_v4().to_string(),
        };
        digest.update(data.to_string_lossy().as_bytes());digest.update(engine.as_bytes());
    }
    if format=="docx" {
        // A Word preview is a labelled layout projection, not a Word renderer.
        // It must not inherit an unavailable TeX engine from the PDF tab.
        let engine=match crate::capability_packs::typst_engine_checked(data,assets,&checked){
            Ok((_,digest))=>digest,
            Err(error) if budget.interrupted(&error.code)=>return Err(error.code),
            Err(_)=>"rust-layout-projection".into(),
        };
        digest.update(data.to_string_lossy().as_bytes());digest.update(engine.as_bytes());
    }
    if !matches!(format,"tex"|"latex"){
        let revision=crate::publication::resource_revision_with_options(content,base,&selected).unwrap_or_else(|e|format!("error:{}",e.code));digest.update(revision.as_bytes());
    }
    let images=crate::mdexport::image_resource_revision(content,base);
    budget.check()?;
    let reusable=images.is_some();
    digest.update(images.unwrap_or_else(||uuid::Uuid::new_v4().to_string()).as_bytes());
    if !reusable{return Ok(format!("uncached:{:x}",digest.finalize()));}
    Ok(format!("{:x}",digest.finalize()))
}
pub fn cached(key:&str)->Option<Arc<Artifact>> {
    let mut items=cache().lock().unwrap_or_else(|e|e.into_inner());
    items.retain(|item|item.created.elapsed()<Duration::from_secs(600));
    items.iter().find(|item|item.key==key).cloned()
}
pub fn prepare(format:&str,content:&str,base:&str,options:&Value,name:&str,assets:&Path)->Result<Arc<Artifact>,String> {
    prepare_with_data(format,content,base,options,name,assets,&crate::paths::data_dir())
}
pub fn prepare_with_data(format:&str,content:&str,base:&str,options:&Value,name:&str,assets:&Path,data:&Path)->Result<Arc<Artifact>,String>{
    crate::publication_budget::run(format,options,||prepare_inner(format,content,base,options,name,assets,data))
}
fn prepare_inner(format:&str,content:&str,base:&str,options:&Value,name:&str,assets:&Path,data:&Path)->Result<Arc<Artifact>,String>{
    let resolved = crate::publication_profile::resolve_for_format(format,content, options).map_err(|e|e.code)?;
    let options = resolved.values();
    crate::publication_profile::preflight(format,content,options).map_err(|e|e.code)?;
    crate::publication_typography::validate_pdf_preview(format, content, options).map_err(str::to_string)?;
    crate::mdexport::validate_caption_author_export(format,content,options)?;
    if !matches!(format,"pdf"|"docx") {return Err("unsupported_preview_format".into());}
    let budget=crate::publication_budget::capture();let checked=||budget.api();
    let projection_engine=if format=="docx"{
        match crate::capability_packs::typst_engine_checked(data,assets,&checked){
            Ok(_)=>"typst",Err(error)if budget.interrupted(&error.code)=>return Err(error.code),Err(_)=>"rust",
        }.to_string()
    }else{options.pointer("/pdf/engine").and_then(Value::as_str).unwrap_or("rust").to_string()};
    if format=="docx"&&projection_engine=="rust"{crate::pdf_render::validate_caption_author_source(content,options)?;}
    let key=key_with_data_checked(format,content,base,options,name,data,assets)?;budget.check()?;if let Some(artifact)=cached(&key) {return Ok(artifact);}
    let directory=tempfile::Builder::new().prefix("readmd-preview-").tempdir().map_err(|e|e.to_string())?;
    let file=directory.path().join(format!("document.{format}"));
    let pdf=if format=="pdf" {file.clone()}else{directory.path().join("layout.pdf")};
    let mut warnings=Vec::new();let mut source_map=None;
    if format=="docx" {
        let result=crate::mdexport::export_document_with_data(format,content,base,&file.to_string_lossy(),options,name,assets,data)?;
        if !result.ok {return Err(result.error.unwrap_or_else(||"preview_generation_failed".into()));}warnings.extend(result.warns.unwrap_or_default());
    }
    let projection=if format=="docx"{let mut value=options.as_object().cloned().unwrap_or_default();value.insert("pdf".into(),json!({"engine":projection_engine}));Value::Object(value)}else{options.clone()};
    if projection_engine=="typst"{
        let (bytes,engine_notes,map)=crate::typst_compile::paginated_document_bytes(content,base,&projection,name,data,assets)?;
        if crate::speech::cancelled(){return Err("cancelled".into());}
        std::fs::write(&pdf,bytes).map_err(|_|"typst_workspace_failed")?;
        warnings.extend(engine_notes);warnings.extend(crate::publication_layout::warnings("pdf",&projection));source_map=Some(map);
    }else{
        let layout=crate::mdexport::export_document_with_data("pdf",content,base,&pdf.to_string_lossy(),&projection,name,assets,data)?;
        if !layout.ok {return Err(layout.error.unwrap_or_else(||"preview_generation_failed".into()));}
        warnings.extend(layout.warns.unwrap_or_default());
    }
    if !key.starts_with("uncached:")&&key!=key_with_data_checked(format,content,base,options,name,data,assets)?{return Err("preview_expired".into());}
    budget.check()?;
    let artifact=Arc::new(Artifact{key,file,pdf,format:format.into(),warnings,projection_engine,source_map,_directory:directory,created:Instant::now()});
    let mut items=cache().lock().unwrap_or_else(|e|e.into_inner());
    if let Some(existing)=items.iter().find(|a|a.key==artifact.key) {return Ok(existing.clone());}
    while items.len()>=6 {items.pop_front();}items.push_back(artifact.clone());Ok(artifact)
}
pub fn payload(artifact:&Artifact)->Result<Value,String> {
    use base64::Engine;
    let budget=crate::publication_budget::capture();budget.check()?;
    let pdf=std::fs::read(&artifact.pdf).map_err(|e|e.to_string())?;
    budget.check()?;
    if pdf.len()>32*1024*1024 {return Err("preview_too_large".into());}
    let document=lopdf::Document::load_mem(&pdf).map_err(|e|e.to_string())?;
    budget.check()?;
    let pages=document.get_pages().len();
    let dimensions=document.get_pages().values().next().and_then(|id|document.get_object(*id).ok()).and_then(|p|p.as_dict().ok())
        .and_then(|p|p.get(b"MediaBox").ok()).and_then(|p|p.as_array().ok())
        .map(|a|a.iter().filter_map(|v|v.as_float().ok().map(|v|v as f64).or_else(||v.as_i64().ok().map(|v|v as f64))).collect::<Vec<_>>()).unwrap_or_default();
    let text=crate::pdf_text::extract(&pdf).unwrap_or_default();
    budget.check()?;
    let rendered=crate::ocr_winrt::render_pdf_page_png(&pdf,0);budget.check()?;
    let png=match rendered{Ok(bytes)=>Some(base64::engine::general_purpose::STANDARD.encode(bytes)),Err(error)if budget.interrupted(&error)=>return Err(error),Err(_)=>None};
    let value=json!({"ok":true,"key":artifact.key,"format":artifact.format,"pdf":base64::engine::general_purpose::STANDARD.encode(pdf),"firstPage":png,"pages":pages,"dimensions":dimensions,"text":text,"mode":if artifact.format=="pdf"{"artifact"}else{"shared-layout"},"previewEngine":artifact.projection_engine,"sourceMap":artifact.source_map,"warnings":artifact.warnings});
    budget.check()?;Ok(value)
}
pub fn page_payload(artifact:&Artifact,page:usize)->Result<Value,String> {
    crate::publication_budget::run("pdf",&json!({"pdf":{"engine":artifact.projection_engine.as_str()}}),||page_payload_inner(artifact,page))
}
fn page_payload_inner(artifact:&Artifact,page:usize)->Result<Value,String>{
    use base64::Engine;
    let budget=crate::publication_budget::capture();budget.check()?;
    if page==0 {return Err("invalid_page_range".into());}
    let bytes=std::fs::read(&artifact.pdf).map_err(|e|e.to_string())?;
    budget.check()?;
    let png=crate::ocr_winrt::render_pdf_page_png(&bytes,page-1)?;
    budget.check()?;
    let value=json!({"ok":true,"key":artifact.key,"page":page,"png":base64::engine::general_purpose::STANDARD.encode(png)});budget.check()?;Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn cancelled_payload_and_key_page_do_not_touch_missing_artifact_files(){
        let directory=tempfile::tempdir().unwrap();let artifact=Artifact{key:"owned-control".into(),file:directory.path().join("missing.pdf"),pdf:directory.path().join("missing.pdf"),format:"pdf".into(),warnings:Vec::new(),projection_engine:"typst".into(),source_map:None,_directory:directory,created:Instant::now()};
        crate::speech::with_cancel(Arc::new(||true),||{assert_eq!(payload(&artifact).unwrap_err(),"cancelled");assert_eq!(page_payload(&artifact,1).unwrap_err(),"cancelled");});
    }
    #[test]
    fn word_preview_does_not_require_the_last_selected_pdf_engine() {
        let dir=tempfile::tempdir().unwrap();let data=tempfile::tempdir().unwrap();
        for engine in ["xelatex","pdflatex","typst"] {
            let options=json!({"pdf":{"engine":engine},"page":{"columns":2}});
            let first=prepare_with_data("docx","# Independent Word\n\nBody.","",&options,"Test",dir.path(),data.path()).unwrap();
            assert_eq!(first.projection_engine,"rust");assert!(first.warnings.contains(&"publication_layout_paged_unsupported".into()));
            let again=prepare_with_data("docx","# Independent Word\n\nBody.","",&options,"Test",dir.path(),data.path()).unwrap();
            assert!(Arc::ptr_eq(&first,&again));assert_eq!(payload(&first).unwrap()["mode"],"shared-layout");
        }
    }
    #[test]
    fn preview_artifact_is_reused_and_changes_invalidate_it() {
        let dir=tempfile::tempdir().unwrap();let options=json!({"typography":{"size":14},"page":{"size":"A5"}});
        let first=prepare("pdf","# Shared preview\n\nReadable body.","",&options,"test",dir.path()).unwrap();
        let again=prepare("pdf","# Shared preview\n\nReadable body.","",&options,"test",dir.path()).unwrap();
        assert!(Arc::ptr_eq(&first,&again));assert_eq!(first.file,first.pdf);
        let payload=payload(&first).unwrap();assert!(payload["pages"].as_u64().unwrap()>0);assert_eq!(payload["mode"],"artifact");
        assert_ne!(first.key,key("pdf","Changed content","",&options,"test"));
        assert_ne!(first.key,key("pdf","# Shared preview\n\nReadable body.","",&json!({}),"test"));
    }
    #[test]
    fn editing_bibliography_invalidates_the_actual_cached_export(){
        let dir=tempfile::tempdir().unwrap();let base=dir.path().to_str().unwrap();let source="---\nbibliography: refs.bib\ncsl: apa\n---\n# Research\n\n[@kant]";let options=json!({});
        std::fs::write(dir.path().join("refs.bib"),"@book{kant,title={First edition},author={Kant, Immanuel},year={1781}}").unwrap();
        let first=prepare("docx",source,base,&options,"Research",dir.path()).unwrap();let bytes=std::fs::read(&first.file).unwrap();
        std::fs::write(dir.path().join("refs.bib"),"@book{kant,title={Revised edition},author={Kant, Immanuel},year={1787}}").unwrap();
        let second=prepare("docx",source,base,&options,"Research",dir.path()).unwrap();assert_ne!(first.key,second.key);assert_ne!(bytes,std::fs::read(&second.file).unwrap());
        assert!(payload(&second).unwrap()["text"].as_str().unwrap().contains("1787"));
    }
    #[test]
    fn replacing_local_images_invalidates_artifacts_even_with_the_same_source_and_size(){
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("plot.png");let source="![Plot](plot.png)\n\n```md\n![Example](missing.png)\n```";let options=json!({});let base=dir.path().to_str().unwrap();
        std::fs::write(&path,b"first image").unwrap();let first=key("pdf",source,base,&options,"Research");
        assert_eq!(first,key("pdf",source,base,&options,"Research"));
        std::fs::write(&path,b"other image").unwrap();assert_ne!(first,key("pdf",source,base,&options,"Research"));
        std::fs::remove_file(path).unwrap();assert_ne!(first,key("pdf",source,base,&options,"Research"));
    }
}
