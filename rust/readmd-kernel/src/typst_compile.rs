//! Managed, locked offline Typst worker. Generated sources and dependencies stay
//! inside an owned project root; only a complete validated PDF is committed.
use serde_json::Value;
use std::{fs,io::Read,path::Path,process::Command,sync::atomic::{AtomicUsize,Ordering},time::{Duration,Instant}};
static ACTIVE:AtomicUsize=AtomicUsize::new(0);
pub fn busy()->bool{ACTIVE.load(Ordering::Acquire)>0}
fn stop(child:&mut crate::worker_sandbox::ManagedWorker){let _=child.stop();}
fn check(deadline:Instant)->Result<(),&'static str>{
    if crate::speech::cancelled(){Err("cancelled")}else if Instant::now()>=deadline{Err("typst_timeout")}else{Ok(())}
}
fn nonconvergent_layout(diagnostics:&str)->bool{
    // Typst 0.15.1 emits this detached summary before its per-location
    // diagnostics, even when compilation exits successfully. Match only the
    // engine header, never quoted author text or a source-line excerpt.
    diagnostics.lines().any(|line|line=="warning: document did not converge within five attempts")
}
fn run(command:&mut Command,dir:&Path,deadline:Instant,pdf:Option<&fs::File>,geometry:Option<(&crate::typst_writer::GeometryContext,&Path)>)->Result<String,String>{
    check(deadline)?;
    let log=tempfile::NamedTempFile::new_in(dir).map_err(|_|"typst_workspace_failed")?;
    crate::task_workspace::track_created(log.path(),log.as_file()).map_err(|_|"typst_workspace_failed")?;
    let mut child=crate::worker_sandbox::spawn_managed(command,dir,pdf.unwrap_or(log.as_file()),log.as_file()).map_err(|_|"typst_engine_unavailable")?;
    let mut last_budget=Instant::now();
    let status=loop{
        let error=if crate::speech::cancelled(){Some("cancelled")}else if Instant::now()>=deadline{Some("typst_timeout")}
            else if log.as_file().metadata().map_or(true,|m|m.len()>4*1024*1024)||pdf.is_some_and(|f|f.metadata().map_or(true,|m|m.len()>32*1024*1024)){Some("typst_output_limit")}else{None};
        if let Some(error)=error{stop(&mut child);return Err(error.into());}
        if last_budget.elapsed()>=Duration::from_millis(200){
            let bytes=walkdir::WalkDir::new(dir).follow_links(false).into_iter().try_fold(0u64,|sum,item|{let item=item.map_err(|_|"typst_workspace_failed")?;Ok::<_,&str>(sum.saturating_add(if item.file_type().is_file(){item.metadata().map_err(|_|"typst_workspace_failed")?.len()}else{0}))});
            match bytes{Ok(bytes)if bytes<=256*1024*1024=>(),Ok(_)=>{stop(&mut child);return Err("typst_output_limit".into());},Err(error)=>{stop(&mut child);return Err(error.into());}}
            last_budget=Instant::now();
        }
        match child.try_wait(){Ok(Some(status))=>break status,Ok(None)=>std::thread::sleep(Duration::from_millis(15)),Err(_)=>{stop(&mut child);return Err("typst_process_failed".into());}}
    };
    child.stop().map_err(|_|"typst_process_failed")?;
    let mut bytes=Vec::new();log.reopen().map_err(|_|"typst_workspace_failed")?.take(4*1024*1024).read_to_end(&mut bytes).map_err(|_|"typst_invalid_output")?;
    let log=String::from_utf8_lossy(&bytes).into_owned();
    if status==0{if nonconvergent_layout(&log){return Err("publication_layout_geometry".into());}Ok(log)}else{if let Some(error)=geometry.and_then(|(context,source)|context.diagnostic(&log,source,dir)){return Err(error);}
        let message=log.lines().find(|s|s.contains("error:")).unwrap_or("error: compilation failed");Err(format!("typst_compilation_failed: {}",message.chars().filter(|c|!c.is_control()).take(320).collect::<String>()))}
}
pub(crate) fn compile_source(program:&Path,source:&str,dir:&Path,timeout:Duration)->Result<Vec<u8>,String>{
    let deadline=crate::publication_budget::deadline(timeout.min(Duration::from_secs(120)));
    check(deadline)?;
    let _slot=crate::capability_process::LocalWorker::acquire("typst",&ACTIVE).map_err(|e|e.code)?;
    compile_at(program,source,dir,deadline,None,false,None)
}
fn stage_fonts(dir:&Path,core_text_fallback:bool)->Result<std::path::PathBuf,String>{
    let fonts=dir.join("fonts");crate::task_workspace::create_dir(&fonts).map_err(|_|"typst_workspace_failed")?;
    crate::task_workspace::write(&fonts.join("STIXTwoMath-Regular.ttf"),crate::publication_fonts::MATH).map_err(|_|"typst_workspace_failed")?;
    crate::task_workspace::write(&fonts.join("Waree-Regular.otf"),crate::publication_fonts::CORE_THAI_FALLBACK).map_err(|_|"typst_workspace_failed")?;
    crate::task_workspace::write(&fonts.join("Waree.LICENSE.txt"),crate::publication_fonts::CORE_THAI_FALLBACK_LICENSE).map_err(|_|"typst_workspace_failed")?;
    if core_text_fallback{
        crate::task_workspace::write(&fonts.join("DejaVuSans.ttf"),crate::publication_fonts::CORE_TEXT_FALLBACK).map_err(|_|"typst_workspace_failed")?;
        crate::task_workspace::write(&fonts.join("DejaVuSans.LICENSE.txt"),crate::publication_fonts::CORE_TEXT_FALLBACK_LICENSE).map_err(|_|"typst_workspace_failed")?;
    }
    Ok(fonts)
}
fn compile_at(program:&Path,source:&str,dir:&Path,deadline:Instant,language:Option<&str>,core_text_fallback:bool,geometry:Option<&crate::typst_writer::GeometryContext>)->Result<Vec<u8>,String>{
    check(deadline)?;
    if source.len()>16*1024*1024{return Err("typst_output_limit".into());}
    let version=run(crate::silent_command(program).arg("--version"),dir,deadline,None,None)?;
    if !version.starts_with("typst 0.15.1 "){return Err("typst_engine_unavailable".into());}
    let generated_source=dir.join("document.typ");
    crate::task_workspace::write(&generated_source,source).map_err(|_|"typst_workspace_failed")?;
    let packages=dir.join("packages");crate::task_workspace::create_dir(&packages).map_err(|_|"typst_workspace_failed")?;
    let fonts=stage_fonts(dir,core_text_fallback)?;
    // Rust creates and identifies the output before launch. The engine writes
    // PDF bytes through an inherited handle instead of replacing a path.
    let output=tempfile::NamedTempFile::new_in(dir).map_err(|_|"typst_workspace_failed")?;
    crate::task_workspace::track_created(output.path(),output.as_file()).map_err(|_|"typst_workspace_failed")?;
    let mut command=crate::silent_command(program);
    command.args(["--color","never","compile","--ignore-system-fonts","--font-path"]).arg(&fonts);
    if let Some(resources)=crate::publication_fonts::current(){command.arg("--font-path").arg(resources.sans.parent().ok_or("publication_fonts_unavailable")?);}
    let diagnostics=run(command.arg("--root").arg(dir).arg("--package-path").arg(&packages).arg("--package-cache-path").arg(&packages)
        .args(["--creation-timestamp","0","--jobs","2","--diagnostic-format","short","--format","pdf","document.typ","-"])
        .env_remove("TYPST_FONT_PATHS").env_remove("TYPST_IGNORE_SYSTEM_FONTS").env_remove("TYPST_IGNORE_EMBEDDED_FONTS"),dir,deadline,Some(output.as_file()),geometry.map(|context|(context,generated_source.as_path())))?;
    if diagnostics.contains("was ignored") {return Err("typst_invalid_content".into());}
    if crate::speech::cancelled(){return Err("cancelled".into());}
    let mut bytes=Vec::new();output.reopen().map_err(|_|"typst_invalid_output")?.take(32*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"typst_invalid_output")?;
    if bytes.len()>32*1024*1024{return Err("typst_output_limit".into());}
    let document=lopdf::Document::load_mem(&bytes).map_err(|_|"typst_invalid_output")?;if document.get_pages().is_empty(){return Err("typst_invalid_output".into());}
    crate::pdf_text::validate_generated_glyphs(&document).map_err(str::to_string)?;
    let bytes=if let Some(language)=language{apply_publication_language(document,bytes,language)?}else{bytes};check(deadline)?;Ok(bytes)
}
// Typst 0.15 emits only the primary language in the PDF catalog. Preserve the
// complete author declaration with a catalog-only incremental update: the
// original pages, fonts, links and structure bytes remain unchanged.
fn apply_publication_language(document:lopdf::Document,bytes:Vec<u8>,language:&str)->Result<Vec<u8>,String>{
    let root=document.trailer.get(b"Root").and_then(lopdf::Object::as_reference).map_err(|_|"typst_invalid_output")?;
    let mut update=lopdf::IncrementalDocument::create_from(bytes,document);
    update.opt_clone_object_to_new_document(root).map_err(|_|"typst_invalid_output")?;
    update.new_document.get_object_mut(root).and_then(lopdf::Object::as_dict_mut).map_err(|_|"typst_invalid_output")?
        .set("Lang",lopdf::Object::string_literal(language));
    let mut out=Vec::new();update.save_to(&mut out).map_err(|_|"typst_invalid_output")?;
    if out.len()>32*1024*1024{return Err("typst_output_limit".into());}Ok(out)
}
pub(crate) fn probe_directory(directory:&Path,data:&Path)->Result<(),String>{
    let dir=crate::task_workspace::Workspace::create(data,"typst-probe").map_err(|_|"typst_workspace_failed")?;
    dir.scope(||{
    // Never execute a staged installation in place: on Windows the image
    // loader/scanner can still pin its directory when the atomic commit runs.
    // Probe identical reviewed bytes from the owned worker directory instead.
    let name=if cfg!(windows){"typst.exe"}else{"typst"};
    let program=dir.path().join(name);
    crate::task_workspace::copy(&directory.join("bin").join(name),&program).map_err(|_|"typst_engine_unavailable")?;
    compile_source(&program,"#set document(title:\"ReadMD engine probe\")\nReadMD offline typesetting.",dir.path(),Duration::from_secs(8)).map(|_|())
    })
}
struct RawArtifact {
    bytes:Vec<u8>, warnings:Vec<String>,
    baseline_links:Vec<crate::publication_pagination::BaselineLinkRequest>,
    baseline_protected:Vec<crate::publication_pagination::BaselineProtectedRequest>,
}
pub(crate) fn document_bytes(content:&str,base:&str,options:&Value,name:&str,data:&Path,assets:&Path,nonce:Option<&str>)->Result<(Vec<u8>,Vec<String>),String>{
    let profile=crate::publication_profile::resolve_for_format("pdf",content,options).map_err(|error|error.code)?;
    let options=profile.values();
    if nonce.is_none() && crate::publication_typography::baseline::nonzero_used(content,options).map_err(str::to_string)? {
        let (bytes,warnings,_)=paginated_document_bytes(content,base,options,name,data,assets)?;
        return Ok((bytes,warnings));
    }
    let artifact=raw_document_bytes(content,base,options,name,data,assets,nonce)?;
    // A raw mapped artifact is not a finished deliverable when native text
    // links need correction. Only the common pagination pipeline owns that.
    if !artifact.baseline_links.is_empty(){return Err(crate::publication_typography::baseline::UNSUPPORTED.into());}
    Ok((artifact.bytes,artifact.warnings))
}

/// Preview and offset export use the same mapped compile, ownership proof,
/// private-annotation cleanup and physical link correction. No second compile
/// is used to infer identity or object numbers.
pub(crate) fn paginated_document_bytes(content:&str,base:&str,options:&Value,name:&str,data:&Path,assets:&Path)
    ->Result<(Vec<u8>,Vec<String>,crate::publication_pagination::PageMap),String>
{
    let mapped=crate::publication_pagination::instrument(content)?;
    let (prepared,notes)=crate::mdexport::prepare_export_source("pdf",&mapped.content,base,options)?;
    let raw=raw_document_bytes(&prepared,base,options,name,data,assets,Some(&mapped.nonce))?;
    let (bytes,map)=crate::publication_pagination::extract_and_clean_with_baseline_and_paint(&raw.bytes,&mapped,&raw.baseline_links,&raw.baseline_protected)?;
    let metadata=crate::publication_metadata::from_source(content,options,name).map_err(|error|error.code)?;
    let bytes=crate::publication_metadata_formats::pdf_metadata(&bytes,&metadata)?;
    if crate::speech::cancelled(){return Err("cancelled".into());}
    let mut warnings=notes;warnings.extend(raw.warnings);
    Ok((bytes,warnings,map))
}

fn raw_document_bytes(content:&str,base:&str,options:&Value,name:&str,data:&Path,assets:&Path,nonce:Option<&str>)->Result<RawArtifact,String>{
    let profile=crate::publication_profile::resolve_for_format("pdf",content,options).map_err(|e|e.code)?;
    let options=profile.values();
    crate::publication_profile::preflight("pdf",content,options).map_err(|e|e.code)?;
    crate::publication_typography::validate_consumer(crate::publication_typography::Consumer::Typst, content, options).map_err(str::to_string)?;
    let deadline=crate::publication_budget::deadline(Duration::from_secs(120));
    let budget=crate::publication_budget::capture();
    let checked=||{budget.api()?;check(deadline).map_err(|code|crate::ApiError::new(422,code))};
    checked().map_err(|e|e.code)?;
    let capacity=crate::capability_process::check_worker_capacity(data,"typst");
    checked().map_err(|e|e.code)?;
    capacity.map_err(|e| {
        if e.code=="typst_busy" {e.code} else {"typst_engine_unavailable".into()}
    })?;
    crate::publication_blocks::validate_source(content).map_err(str::to_string)?;
    crate::publication_blocks::validate_geometry(content,&crate::export_styles::sanitize(options)).map_err(str::to_string)?;
    let (engine,_slot)=crate::capability_packs::lease_reserved(data,assets,"typst",&checked,||crate::capability_process::reserve_worker(data,"typst",&ACTIVE))
        .map_err(|e|if budget.interrupted(&e.code)||e.code=="typst_busy"{e.code}else{"typst_engine_unavailable".into()})?;
    let program=engine.directory.join(if cfg!(windows){"bin/typst.exe"}else{"bin/typst"});
    let _fonts=crate::publication_fonts::scope_checked(data,assets,&checked).map_err(|e|e.code)?;
    crate::publication_fonts::require_for_export("pdf",content,options)?;
    let dir=crate::task_workspace::Workspace::create(data,"typst-export").map_err(|_|"typst_workspace_failed")?;
    dir.scope(||{
    let (doc,citation_warnings)=crate::mdexport::publication_document(content,base,options)?;let source=if let Some(nonce)=nonce{crate::typst_writer::render_mapped(&doc,options,base,dir.path(),name,nonce)?}else{crate::typst_writer::render(&doc,options,base,dir.path(),name)?};
    // A raw nonzero artifact may only be produced for the mapped pipeline.
    // That caller must verify actual MCID/glyph/path paint and protected
    // source witnesses, then fully clean before Rect-only correction. Raw
    // artifacts remain forbidden as deliverables in document_bytes above.
    if !source.baseline_links.is_empty()&&nonce.is_none(){return Err(crate::publication_typography::baseline::UNSUPPORTED.into());}
    if source.baseline_protected.iter().any(|p|!source.baseline_links.iter().any(|r|r.block_id==p.block_id)){
        return Err("typst_invalid_output".into());
    }
    let metadata=crate::publication_metadata::resolve(doc.front_matter.as_deref(),options,name).map_err(|e|e.code)?;
    // Pagination removes private source-map annotations. Its cleaned artifact
    // must be the base of the final metadata revision, not a later rewrite of it.
    let bytes=compile_at(&program,&source.text,dir.path(),deadline,if nonce.is_some(){None}else{metadata.language.as_deref()},source.requires_core_text_fallback,source.geometry.as_ref())?;
    let bytes=if nonce.is_none(){crate::publication_metadata_formats::pdf_metadata(&bytes,&metadata)?}else{bytes};
    check(deadline)?;
    let mut warnings=source.warnings;
    warnings.extend(citation_warnings);
    warnings.extend(crate::publication_metadata_formats::warnings(&metadata));
    let requested=options.pointer("/typography/font").and_then(Value::as_str).unwrap_or("MicrosoftYaHei");
    if requested!=crate::publication_fonts::family(requested){warnings.push("publication_font_substituted".into());}
    Ok(RawArtifact{bytes,warnings,baseline_links:source.baseline_links,baseline_protected:source.baseline_protected})
    })
}
pub fn export_pdf(content:&str,base:&str,target:&str,options:&Value,name:&str,data:&Path,assets:&Path)->Result<crate::mdexport::ExportResult,String>{
    let (bytes,warnings)=document_bytes(content,base,options,name,data,assets,None)?;
    crate::publication_budget::check()?;
    crate::content::write_bytes_atomic(Path::new(target),&bytes).map_err(|_|"typst_save_failed")?;
    Ok(crate::mdexport::ExportResult{ok:true,path:Some(target.into()),size:Some(bytes.len()as u64),warns:Some(warnings),error:None,canceled:Some(false)})
}

#[cfg(test)]mod tests{
    #[test]fn unstable_success_is_not_accepted_and_author_source_excerpts_are_not_engine_headers(){
        let actual="warning: document did not converge within five attempts\r\ndocument.typ:5:25: warning: page number of the footnote did not stabilize\r\n";
        assert!(super::nonconvergent_layout(actual));
        for text in ["", "warning: unknown font family", "document.typ:5:25: error: author text", "5 | warning: document did not converge within five attempts", "Author wrote warning: document did not converge within five attempts", "warning: document did not converge within five attempts (quoted)"]{
            assert!(!super::nonconvergent_layout(text),"{text}");
        }
    }
    #[test]fn catalog_language_update_keeps_original_pdf_objects_and_bytes(){
        let mut pdf=lopdf::Document::with_version("1.7");let pages=pdf.new_object_id();let stream=pdf.add_object(lopdf::Stream::new(lopdf::dictionary!{},b"BT /F1 12 Tf (Original content) Tj ET".to_vec()));let page=pdf.add_object(lopdf::dictionary!{"Type"=>"Page","Parent"=>pages,"Contents"=>stream,"MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()]});pdf.objects.insert(pages,lopdf::dictionary!{"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into());let root=pdf.add_object(lopdf::dictionary!{"Type"=>"Catalog","Pages"=>pages,"Lang"=>lopdf::Object::string_literal("de"),"PageMode"=>"UseOutlines"});pdf.trailer.set("Root",root);let mut before=Vec::new();pdf.save_to(&mut before).unwrap();let document=lopdf::Document::load_mem(&before).unwrap();let out=super::apply_publication_language(document,before.clone(),"de-DE").unwrap();assert!(out.starts_with(&before));let result=lopdf::Document::load_mem(&out).unwrap();let catalog=result.catalog().unwrap();assert_eq!(catalog.get(b"Lang").unwrap().as_str().unwrap(),b"de-DE");assert_eq!(catalog.get(b"PageMode").unwrap().as_name().unwrap(),b"UseOutlines");assert_eq!(result.get_pages().len(),1);assert_eq!(result.get_object(stream).unwrap().as_stream().unwrap().content,b"BT /F1 12 Tf (Original content) Tj ET");
    }
    use super::*;
    #[test]fn worker_fonts_stage_reviewed_fallback_and_complete_license_only_on_demand(){
        let plain=tempfile::tempdir().unwrap();let fonts=stage_fonts(plain.path(),false).unwrap();
        assert!(fonts.join("STIXTwoMath-Regular.ttf").is_file());
        assert_eq!(fs::read(fonts.join("Waree-Regular.otf")).unwrap(),crate::publication_fonts::CORE_THAI_FALLBACK);
        assert_eq!(fs::read(fonts.join("Waree.LICENSE.txt")).unwrap(),crate::publication_fonts::CORE_THAI_FALLBACK_LICENSE);
        for unbundled in ["Waree-Bold.otf","Waree-Oblique.otf","Waree-BoldOblique.otf"]{assert!(!fonts.join(unbundled).exists());}
        assert!(!fonts.join("DejaVuSans.ttf").exists());
        assert!(!fonts.join("DejaVuSans.LICENSE.txt").exists());
        let rich=tempfile::tempdir().unwrap();let fonts=stage_fonts(rich.path(),true).unwrap();
        assert_eq!(fs::read(fonts.join("Waree-Regular.otf")).unwrap(),crate::publication_fonts::CORE_THAI_FALLBACK);
        assert_eq!(fs::read(fonts.join("Waree.LICENSE.txt")).unwrap(),crate::publication_fonts::CORE_THAI_FALLBACK_LICENSE);
        assert_eq!(fs::read(fonts.join("DejaVuSans.ttf")).unwrap(),crate::publication_fonts::CORE_TEXT_FALLBACK);
        assert_eq!(fs::read(fonts.join("DejaVuSans.LICENSE.txt")).unwrap(),crate::publication_fonts::CORE_TEXT_FALLBACK_LICENSE);
    }
    #[test]fn elapsed_deadline_prevents_process_and_task_file_creation(){
        let root=tempfile::tempdir().unwrap();let program=root.path().join("unavailable-engine");
        assert_eq!(compile_source(&program,"ReadMD",root.path(),Duration::ZERO).unwrap_err(),"typst_timeout");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(),0);
        crate::speech::with_cancel(std::sync::Arc::new(||true),||{
            assert_eq!(compile_source(&program,"ReadMD",root.path(),Duration::ZERO).unwrap_err(),"cancelled");
        });
        assert_eq!(fs::read_dir(root.path()).unwrap().count(),0);
    }
    #[test]fn global_busy_precedes_runtime_validation_and_preserves_previous_pdf(){
        let data=tempfile::tempdir().unwrap();let documents=tempfile::tempdir().unwrap();
        let assets=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let first=crate::capability_process::try_worker_slot(data.path(),"typst").unwrap().unwrap();
        let second=crate::capability_process::try_worker_slot(data.path(),"typst").unwrap().unwrap();
        let target=documents.path().join("previous.pdf");fs::write(&target,b"Unchanged previous output").unwrap();
        let result=export_pdf("# ReadMD\n\nText.",documents.path().to_str().unwrap(),target.to_str().unwrap(),&serde_json::json!({}),"ReadMD",data.path(),&assets);
        assert_eq!(result.unwrap_err(),"typst_busy");
        assert_eq!(fs::read(&target).unwrap(),b"Unchanged previous output");
        assert_eq!(fs::read_dir(documents.path()).unwrap().count(),1);
        assert!(!data.path().join("capabilities/managed-v2/typst").exists());drop((first,second));
    }
}
