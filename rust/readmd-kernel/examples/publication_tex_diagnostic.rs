//! Dev-only, persistent diagnostic for the original synthetic section fixture.
//! Uses the actual exporter and managed sandbox; never a direct TeX process.
use readmd_kernel::{latex_compile, mdexport, publication_fonts};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}, time::{Duration, Instant}};

const SOURCE_SHA: &str = "d6155e59de41a1f48dc51f82087d7b89283e51e37589698d6068eb14dcdeee64";
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn create_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file=fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;file.sync_all()
}
fn absolute_existing(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if !path.is_absolute() { return Err("Explicit absolute input path required".into()); }
    let canonical=dunce::canonicalize(path)?;
    if canonical!=path { return Err("Redirected input paths are not accepted".into()); }
    Ok(canonical)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args:Vec<PathBuf>=std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len()!=5 { return Err("Expected original-Sections.md options.json NEW-output-directory owned-font-profile assets-directory".into()); }
    let repo=dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))?;
    let cache=dunce::canonicalize(repo.join(".cache/capability-upgrade"))?;
    let source_path=absolute_existing(&args[0])?;let options_path=absolute_existing(&args[1])?;
    let data=absolute_existing(&args[3])?;let assets=absolute_existing(&args[4])?;
    if !source_path.starts_with(&cache)||source_path.file_name().and_then(|s|s.to_str())!=Some("Sections.md")
        ||!options_path.starts_with(&cache)||data!=cache.join("publication-engines-native-n2e7Ug")
        ||!assets.starts_with(&repo)||!assets.is_dir() {
        return Err("Only owned synthetic section inputs, fixed managed profile and repository assets are accepted".into());
    }
    let output=&args[2];
    if !output.is_absolute()||fs::symlink_metadata(output).is_ok(){return Err("Output must be a new absolute owned directory".into());}
    let parent=absolute_existing(output.parent().ok_or("Missing output parent")?)?;
    if !parent.starts_with(&cache){return Err("Output parent must be inside the owned QA cache".into());}
    let source_bytes=fs::read(&source_path)?;
    if digest(&source_bytes)!=SOURCE_SHA{return Err("The original section fixture must remain byte-identical".into());}
    let source=std::str::from_utf8(&source_bytes)?;
    let option_bytes=fs::read(&options_path)?;
    let options:serde_json::Value=serde_json::from_slice(&option_bytes)?;
    if options.pointer("/pdf/engine").and_then(serde_json::Value::as_str)!=Some("xelatex"){
        return Err("Diagnostic options must identify the actual XeLaTeX request".into());
    }
    fs::create_dir(output)?;
    let _font_scope=publication_fonts::scope(&data,&assets);
    if publication_fonts::current().is_none(){return Err("Explicit managed fonts unavailable".into());}
    let mut effective=options.clone();
    // Same managed-PDF projection controls as export_pdf_with_data.
    effective["tex"]["renderBibliography"]=json!(false);effective["tex"]["bibEngine"]=json!("bibtex");
    create_new(&output.join("inputs.json"),&serde_json::to_vec_pretty(&json!({
        "devOnly":true,"acceptance":false,"sourcePath":source_path,"sourceSha256":SOURCE_SHA,
        "optionsPath":options_path,"optionsSha256":digest(&option_bytes),"options":options,"effectiveOptions":effective,
        "data":data,"assets":assets,"exampleSha256":digest(include_bytes!("publication_tex_diagnostic.rs")),
        "managedBudgetMs":120000,"managedInvocations":1,"noReadinessRequest":true,"noDirectEngineInvocation":true
    }))?)?;
    let target=output.join("document.tex");let started=Instant::now();
    let exported=mdexport::export_tex(source,source_path.parent().ok_or("Missing source parent")?.to_str().ok_or("Invalid source parent")?,
        target.to_str().ok_or("Invalid output path")?,&effective,"Sections")?;
    let projection_ms=started.elapsed().as_millis();let tex=fs::read_to_string(&target)?;
    let started=Instant::now();
    let result=latex_compile::compile_managed(&tex,output,"xelatex",Duration::from_secs(120),&data,&assets);
    let elapsed_ms=started.elapsed().as_millis();
    let (compiled,error)=match result {Ok(bytes)=>{create_new(&output.join("result.pdf"),&bytes)?;(true,None)},Err(error)=>(false,Some(error))};
    let mut artifacts=Vec::new();
    for name in ["document.tex","document.log","document.worker.log","document.pdf","document.xdv","document.aux","document.out","document.toc","result.pdf"]{
        let path=output.join(name);if path.is_file(){let bytes=fs::read(path)?;artifacts.push(json!({"file":name,"bytes":bytes.len(),"sha256":digest(&bytes)}));}
    }
    let unchanged=fs::read(&source_path)?==source_bytes&&fs::read(&options_path)?==option_bytes;
    create_new(&output.join("report.json"),&serde_json::to_vec_pretty(&json!({
        "scope":"managed-source-projection-diagnostic-only","acceptance":false,"compiled":compiled,"error":error,
        "projectionMs":projection_ms,"compileMs":elapsed_ms,"inputsUnchanged":unchanged,"warnings":exported.warns,"artifacts":artifacts
    }))?)?;
    println!("Diagnostic report: {} (compiled={compiled}, acceptance=false)",output.join("report.json").display());
    if !unchanged{return Err("Original input bytes changed".into());}
    if let Some(error)=error{return Err(error.into());}Ok(())
}
