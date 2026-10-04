//! Preview and export share generated artifacts keyed by identical inputs.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::VecDeque,path::{Path,PathBuf},sync::{Arc,Mutex,OnceLock},time::{Duration,Instant}};
pub struct Artifact {
    pub key:String,pub file:PathBuf,pub pdf:PathBuf,pub format:String,pub warnings:Vec<String>,
    _directory:tempfile::TempDir,created:Instant,
}
fn cache()-> &'static Mutex<VecDeque<Arc<Artifact>>> {
    static CACHE:OnceLock<Mutex<VecDeque<Arc<Artifact>>>>=OnceLock::new();CACHE.get_or_init(||Mutex::new(VecDeque::new()))
}
pub fn key(format:&str,content:&str,base:&str,options:&Value,name:&str)->String {
    let mut digest=Sha256::new();let options=serde_json::to_vec(options).unwrap_or_default();
    for value in [format.as_bytes(),content.as_bytes(),base.as_bytes(),options.as_slice(),name.as_bytes()] {
        digest.update((value.len() as u64).to_le_bytes());digest.update(value);
    }
    format!("{:x}",digest.finalize())
}
pub fn cached(key:&str)->Option<Arc<Artifact>> {
    let mut items=cache().lock().unwrap_or_else(|e|e.into_inner());
    items.retain(|item|item.created.elapsed()<Duration::from_secs(600));
    items.iter().find(|item|item.key==key).cloned()
}
pub fn prepare(format:&str,content:&str,base:&str,options:&Value,name:&str,assets:&Path)->Result<Arc<Artifact>,String> {
    if !matches!(format,"pdf"|"docx") {return Err("unsupported_preview_format".into());}
    let key=key(format,content,base,options,name);if let Some(artifact)=cached(&key) {return Ok(artifact);}
    let directory=tempfile::Builder::new().prefix("readmd-preview-").tempdir().map_err(|e|e.to_string())?;
    let file=directory.path().join(format!("document.{format}"));
    let result=crate::mdexport::export_document(format,content,base,&file.to_string_lossy(),options,name,assets)?;
    if !result.ok {return Err(result.error.unwrap_or_else(||"preview_generation_failed".into()));}
    let pdf=if format=="pdf" {file.clone()}else{directory.path().join("layout.pdf")};
    let mut warnings=result.warns.unwrap_or_default();
    if format=="docx" {
        let layout=crate::mdexport::export_pdf(content,base,&pdf.to_string_lossy(),options,name,assets)?;
        if !layout.ok {return Err(layout.error.unwrap_or_else(||"preview_generation_failed".into()));}
        warnings.extend(layout.warns.unwrap_or_default());
    }
    let artifact=Arc::new(Artifact{key,file,pdf,format:format.into(),warnings,_directory:directory,created:Instant::now()});
    let mut items=cache().lock().unwrap_or_else(|e|e.into_inner());
    if let Some(existing)=items.iter().find(|a|a.key==artifact.key) {return Ok(existing.clone());}
    while items.len()>=6 {items.pop_front();}items.push_back(artifact.clone());Ok(artifact)
}
pub fn payload(artifact:&Artifact)->Result<Value,String> {
    use base64::Engine;
    let pdf=std::fs::read(&artifact.pdf).map_err(|e|e.to_string())?;
    if pdf.len()>32*1024*1024 {return Err("preview_too_large".into());}
    let document=lopdf::Document::load_mem(&pdf).map_err(|e|e.to_string())?;
    let pages=document.get_pages().len();
    let dimensions=document.get_pages().values().next().and_then(|id|document.get_object(*id).ok()).and_then(|p|p.as_dict().ok())
        .and_then(|p|p.get(b"MediaBox").ok()).and_then(|p|p.as_array().ok())
        .map(|a|a.iter().filter_map(|v|v.as_float().ok().map(|v|v as f64).or_else(||v.as_i64().ok().map(|v|v as f64))).collect::<Vec<_>>()).unwrap_or_default();
    let text=pdf_extract::extract_text_from_mem(&pdf).unwrap_or_default();
    let png=crate::ocr_winrt::render_pdf_page_png(&pdf,0).ok().map(|b|base64::engine::general_purpose::STANDARD.encode(b));
    Ok(json!({"ok":true,"key":artifact.key,"format":artifact.format,"pdf":base64::engine::general_purpose::STANDARD.encode(pdf),"firstPage":png,"pages":pages,"dimensions":dimensions,"text":text,"mode":if artifact.format=="pdf"{"artifact"}else{"shared-layout"},"warnings":artifact.warnings}))
}
pub fn page_payload(artifact:&Artifact,page:usize)->Result<Value,String> {
    use base64::Engine;
    if page==0 {return Err("invalid_page_range".into());}
    let bytes=std::fs::read(&artifact.pdf).map_err(|e|e.to_string())?;
    let png=crate::ocr_winrt::render_pdf_page_png(&bytes,page-1)?;
    Ok(json!({"ok":true,"key":artifact.key,"page":page,"png":base64::engine::general_purpose::STANDARD.encode(png)}))
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
