//! Local readiness is based on an engine operation, never route membership.
//! Network reachability is deliberately a separate, request-specific result.
use crate::App;
use std::sync::Arc;

pub fn start(app: &Arc<App>, name: &str, retry: bool) {
    {
        let mut registry = app.modules.lock().unwrap_or_else(|e| e.into_inner());
        match registry.state(name).as_deref() {
            Some("idle") => (),
            Some("error") if retry => (),
            _ => return,
        }
        registry.set(name, "loading");
    }
    let app = app.clone(); let name = name.to_string();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::ocr_portable::with_paths(&app.paths,||if name=="pdf_editor"{crate::pdf_workspace::probe(&app)}else{probe(&name)})));
        let mut registry = app.modules.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(Ok(())) => registry.set(&name,"ready"),
            Ok(Err(code)) => registry.set_error(&name,code),
            Err(_) => registry.set_error(&name,"module_probe_failed"),
        }
    });
}

pub fn probe(name: &str) -> Result<(), &'static str> {
    match name {
        "convert" => {
            let doc=crate::md_ast::parse("# ReadMD\n\nEngine probe α\n");
            let package=crate::docx_writer::build(&doc,&serde_json::json!({}),"probe.md",&|_|None);
            if package.parts.iter().any(|(n,b)|n=="word/document.xml" && String::from_utf8_lossy(b).contains("Engine probe α")) {Ok(())} else {Err("convert_probe_failed")}
        },
        "web" => {
            let result=crate::parity_web::extract_html("https://readmd.invalid/probe","<html><body><article><h1>ReadMD</h1><p>ReadMD engine probe content.</p></article></body></html>","auto",None,None,false).map_err(|_|"web_probe_failed")?;
            if result.to_string().contains("ReadMD engine probe content") {Ok(())} else {Err("web_probe_failed")}
        },
        "ocr" => {
            match crate::ocr_portable::probe_managed(){
                Ok(())=>return Ok(()),
                Err(code) if code=="ocr_no_engine"=>(),
                Err(code)=>return Err(match code.as_str(){"cancelled"=>"cancelled","ocr_busy"=>"ocr_busy","ocr_timeout"=>"ocr_timeout",_=>"ocr_probe_failed"}),
            }
            if !crate::ocr_winrt::available(){return Err("ocr_no_engine");}
            // An owned 64×64 white BMP exercises image decoding and OCR without
            // inspecting any personal document or persisting a probe file.
            let size=54+64*64*3;let mut bmp=vec![255u8;size];bmp[..54].fill(0);
            bmp[..2].copy_from_slice(b"BM");bmp[2..6].copy_from_slice(&(size as u32).to_le_bytes());
            bmp[10..14].copy_from_slice(&54u32.to_le_bytes());bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
            bmp[18..22].copy_from_slice(&64i32.to_le_bytes());bmp[22..26].copy_from_slice(&64i32.to_le_bytes());
            bmp[26..28].copy_from_slice(&1u16.to_le_bytes());bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
            crate::ocr_winrt::ocr_image_bytes(&bmp).map(|_|()).map_err(|_|"ocr_probe_failed")
        },
        "ai" => {
            // This is transport initialization, NOT a claim that credentials,
            // a remote endpoint or a particular model have been verified.
            let endpoint=crate::ai_providers::normalize_base_url("http://127.0.0.1:1/v1", "/chat/completions");
            if endpoint.is_empty(){Err("ai_engine_unavailable")}else{Ok(())}
        },
        "transcribe" => if crate::speech::capabilities()["ok"]==true {Ok(())} else {Err("speech_engine_unavailable")},
        "pdf_editor" => if crate::pdf_editor::raster_available(){Ok(())}else{Err("pdf_render_engine_unavailable")},
        _=>Err("module_invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_requires_a_real_operation_and_never_a_route() {
        assert!(probe("convert").is_ok()); assert!(probe("web").is_ok());
        assert_eq!(probe("not-a-module"),Err("module_invalid"));
        assert_eq!(probe("pdf_editor").is_ok(),crate::pdf_editor::raster_available());
    }
}
