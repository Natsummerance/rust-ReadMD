//! TeX workers share bounded output, cancellation and PDF validation. Portable
//! Windows execution additionally uses OS isolation and explicit PDF driving.
//! Production resolves only application-locked resources, never host PATH.
use crate::mdexport::ExportResult;
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;
pub(crate) const VERSION: &str = "2026.10-r1";
const LOG_LIMIT: u64 = 4 * 1024 * 1024;
const WORKSPACE_ENTRIES: usize = 16_384;
static ACTIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub(crate) fn busy() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Acquire) > 0
}
// Reserve before creating profiles, task files or validating a large runtime.
// Requests rejected as busy must remain cheap and leave no setup artefacts.
fn preflight(timeout: Duration) -> Result<(crate::capability_process::LocalWorker, Instant), String> {
    if crate::speech::cancelled() {
        return Err("cancelled".into());
    }
    if timeout.is_zero() {
        return Err("latex_timeout".into());
    }
    crate::publication_budget::check()?;
    Ok((
        crate::capability_process::LocalWorker::acquire("latex", &ACTIVE).map_err(|e|e.code)?,
        crate::publication_budget::deadline(timeout.min(Duration::from_secs(120))),
    ))
}
const CODES: &[&str] = &[
    "latex_engine_unavailable",
    "latex_invalid_engine",
    "latex_invalid_options",
    "latex_workspace_failed",
    "latex_timeout",
    "latex_output_limit",
    "latex_process_failed",
    "latex_invalid_output",
    "latex_compilation_failed",
    "latex_references_unsettled",
    "latex_save_failed",
    "latex_busy",
];

pub fn error_body(error: &str) -> Value {
    if error == "preview_expired" {
        return serde_json::json!({"ok":false,"error_code":"preview_expired"});
    }
    let (code, details) = error.split_once(": ").unwrap_or((error, ""));
    if code==crate::api_codes::AUTHOR_INLINE_TEXT_UNSUPPORTED && details.len()<=2048
        && serde_json::from_str::<serde_json::Map<String,Value>>(details).is_ok() {
        #[derive(serde::Deserialize,serde::Serialize)]
        #[serde(deny_unknown_fields,rename_all="camelCase")]
        struct TexUnsupported { consumer:String, reason:String, coordinate_space:String, from:usize, to:usize }
        // The map gate rejects sequence-shaped structs; decode the original
        // bytes again so duplicate keys are still rejected by the typed model.
        if let Ok(value)=serde_json::from_str::<TexUnsupported>(details){
            if value.consumer=="tex" && value.reason=="rtl" && value.coordinate_space=="textUtf8Bytes"
                && value.from<value.to && value.to<=crate::document_transaction::MAX_DOCUMENT_BYTES {
                return serde_json::json!({"ok":false,"error_code":code,"details":value});
            }
        }
    }
    if code==crate::api_codes::AUTHOR_INLINE_TEXT_PDF_SHAPING_UNSUPPORTED&&details.len()<=2048
        && serde_json::from_str::<serde_json::Map<String,Value>>(details).is_ok() {
        #[derive(serde::Deserialize,serde::Serialize)]
        #[serde(deny_unknown_fields,rename_all="camelCase")]
        struct Shaping { consumer:String, coordinate_space:String, from:usize, to:usize }
        if let Ok(value)=serde_json::from_str::<Shaping>(details){
            if value.consumer=="rustPdf"&&value.coordinate_space=="textUtf8Bytes"&&value.from<value.to&&value.to<=crate::document_transaction::MAX_DOCUMENT_BYTES{
                return serde_json::json!({"ok":false,"error_code":code,"details":value});
            }
        }
    }
    if code==crate::api_codes::MATH_DOCUMENT_DEFAULTS_CONFLICT && details.len()<=2048
        && serde_json::from_str::<serde_json::Map<String,Value>>(details).is_ok() {
        #[derive(serde::Deserialize,serde::Serialize)]
        #[serde(deny_unknown_fields,rename_all="camelCase")]
        struct Conflict { property:String, first_formula:u64, formula:u64 }
        if let Ok(value)=serde_json::from_str::<Conflict>(details) {
            if matches!(value.property.as_str(),"mathFont"|"brkBin"|"brkBinSub"|"smallFrac"|"dispDef"|"lMargin"|"rMargin"|"defJc"|"preSp"|"postSp"|"interSp"|"intraSp"|"wrapIndent"|"wrapRight"|"intLim"|"naryLim")
                &&value.first_formula>0 && value.first_formula<value.formula && value.formula<=1_000_000 {
                return serde_json::json!({"ok":false,"error_code":code,"details":value});
            }
        }
    }
    if code==crate::api_codes::PUBLICATION_NOTE_LAYOUT && details.len()<=4096 {
        if let Ok(value)=serde_json::from_str::<Value>(details){
            if let Some(object)=value.as_object(){
                const FIELDS:&[&str]=&["coordinateSpace","noteIndex","definitionPage","referencePage"];
                if object.len()==FIELDS.len() && object.keys().all(|key|FIELDS.contains(&key.as_str()))
                    &&value["coordinateSpace"]=="pdfFootnoteStartPage"
                    &&value["noteIndex"].as_u64().is_some_and(|n|(1..=32768).contains(&n))
                    &&["definitionPage","referencePage"].iter().all(|key|value[*key].as_u64().is_some_and(|n|(1..=100000).contains(&n)))
                    &&value["definitionPage"]!=value["referencePage"] {
                    return serde_json::json!({"ok":false,"error_code":code,"details":value});
                }
            }
        }
    }
    if code==crate::api_codes::PDF_MISSING_GLYPH&&details.len()<=4096 {
        if let Ok(value)=serde_json::from_str::<Value>(details){
            if let Some(object)=value.as_object(){
                const FIELDS:&[&str]=&["codepoint","page","item","span","coordinateSpace","from","to","x","y"];
                if object.keys().all(|key|FIELDS.contains(&key.as_str()))
                    &&value["coordinateSpace"]=="pdfSpanUtf16"
                    &&value["codepoint"].as_str().is_some_and(|code|code.starts_with("U+")&&code.len()<=8&&code[2..].bytes().all(|byte|byte.is_ascii_hexdigit()))
                    &&["page","item","span","from","to"].iter().all(|key|value[*key].as_u64().is_some())
                    &&value["from"].as_u64()<value["to"].as_u64()
                    &&["x","y"].iter().all(|key|value[*key].as_f64().is_some_and(f64::is_finite)) {
                    return serde_json::json!({"ok":false,"error_code":code,"details":value});
                }
            }
        }
    }
    if code==crate::api_codes::PUBLICATION_LAYOUT_GEOMETRY && details.len()<=4096 {
        const FIELDS:&[&str]=&["coordinateSpace","blockId","page","pageObject","reason","ink","otherBlockId"];
        #[derive(serde::Deserialize,serde::Serialize)]
        #[serde(deny_unknown_fields,rename_all="camelCase")]
        struct BaselineInk { coordinate_space:String, block_id:Option<String>, page:u32, page_object:u32,
            reason:String, ink:[f64;4], other_block_id:Option<String> }
        // Decode the original bytes twice: the object gate requires all
        // fields, while the typed struct rejects duplicate keys and arrays.
        let object=serde_json::from_str::<serde_json::Map<String,Value>>(details).ok();
        if object.as_ref().is_some_and(|v|v.len()==FIELDS.len()&&v.keys().all(|key|FIELDS.contains(&key.as_str()))) {
            if let Ok(value)=serde_json::from_str::<BaselineInk>(details) {
                let identity=|id:&Option<String>|id.as_ref().is_none_or(|id|!id.is_empty()&&id.len()<=1024&&!id.chars().any(char::is_control));
                if value.coordinate_space=="pdfBaselineInk"&&(1..=100000).contains(&value.page)&&value.page_object>0
                    &&matches!(value.reason.as_str(),"clipped-ink"|"paper-edge"|"ink-collision")
                    &&identity(&value.block_id)&&identity(&value.other_block_id)
                    &&value.ink.iter().all(|n|n.is_finite()&&n.abs()<=1e9)
                    &&value.ink[0]<=value.ink[2]&&value.ink[1]<=value.ink[3] {
                    return serde_json::json!({"ok":false,"error_code":code,"details":value});
                }
            }
        }
    }
    if code=="publication_layout_geometry" && details.len()<=4096 {
        if let Ok(value)=serde_json::from_str::<Value>(details) {
            if let Some(object)=value.as_object() {
                const FIELDS:&[&str]=&["coordinateSpace","blockIndex","blockId","line","mode","requestedPt","requiredPt","availableWidthPt","availableHeightPt","baselineShiftPt"];
                if object.keys().all(|key|FIELDS.contains(&key.as_str()))
                    && value["coordinateSpace"]=="pdfParagraphLine"
                    && ["blockIndex","line"].iter().all(|key|value[*key].as_u64().is_some_and(|n|n>0))
                    && value.get("blockId").is_none_or(|id|id.as_str().is_some_and(|s|s.len()<=1024&&!s.chars().any(char::is_control)))
                    && (matches!(value["mode"].as_str(),Some("multiple"|"exact"|"atLeast"))
                        || value["mode"]=="legacy"&&value.get("baselineShiftPt").is_some())
                    && value.get("baselineShiftPt").is_none_or(|v|v.as_f64().is_some_and(|n|n.is_finite()&&(-72.0..=72.0).contains(&n)))
                    && ["requestedPt","requiredPt","availableWidthPt","availableHeightPt"].iter()
                        .all(|key|value[*key].as_f64().is_some_and(|n|n.is_finite()&&n>=0.)) {
                    return serde_json::json!({"ok":false,"error_code":code,"details":value});
                }
            }
        }
    }
    if CODES.contains(&code)
        || crate::api_codes::ERROR_CODES.contains(&code)
        || matches!(code, "cancelled" | "math_unsupported_version" | "math_invalid_source" | "math_invalid_property"
            | "math_budget" | "math_invalid_xml" | "math_unsupported_extension")
        || code.starts_with("diagram_")
    {
        serde_json::json!({"ok":false,"error_code":code,"details":details})
    } else {
        serde_json::json!({"ok":false,"error":error})
    }
}

#[cfg(test)]
#[path = "publication_baseline_error_tests.rs"]
mod baseline_error_tests;

#[cfg(windows)]
enum Process {
    Isolated(crate::worker_sandbox::Worker),
}
#[cfg(windows)]
impl Process {
    fn finish(&mut self)->std::io::Result<()> {
        match self {Self::Isolated(child)=>child.stop()}
    }
    fn stop(&mut self) {
        match self {
            Self::Isolated(child) => {
                let _ = child.stop();
            }
        }
    }
    fn try_wait(&mut self) -> std::io::Result<Option<u32>> {
        match self {
            Self::Isolated(child) => child.try_wait(),
        }
    }
}

#[cfg(windows)]
fn monitor(
    mut child: Process,
    capture: tempfile::NamedTempFile,
    dir: &Path,
    deadline: Instant,
) -> Result<(bool, String), String> {
    let mut budget_check = Instant::now();
    let status = loop {
        let error = if crate::speech::cancelled() {
            Some("cancelled")
        } else if Instant::now() >= deadline {
            Some("latex_timeout")
        } else if capture
            .as_file()
            .metadata()
            .map(|m| m.len() > LOG_LIMIT)
            .unwrap_or(true)
        {
            Some("latex_output_limit")
        } else {
            None
        };
        if let Some(error) = error {
            child.stop();
            return Err(error.into());
        }
        if budget_check.elapsed() >= Duration::from_millis(200) {
            let total = workspace_size(dir, deadline, WORKSPACE_ENTRIES);
            let total = match total {
                Ok(total) => total,
                Err(error) => {
                    child.stop();
                    return Err(error);
                }
            };
            if total > 128 * 1024 * 1024 {
                child.stop();
                return Err("latex_output_limit".into());
            }
            budget_check = Instant::now();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                child.stop();
                return Err("latex_process_failed".into());
            }
        }
    };
    child.finish().map_err(|_|"latex_process_failed")?;
    let mut log = Vec::new();
    capture
        .reopen()
        .map_err(|_| "latex_workspace_failed")?
        .take(LOG_LIMIT)
        .read_to_end(&mut log)
        .map_err(|_| "latex_invalid_output")?;
    let mut log = String::from_utf8_lossy(&log).into_owned();
    if status != 0 && log.trim().is_empty() {
        log = format!("Worker exited with status {status:#x}");
    }
    if status != 0 {
        let _ = fs::write(dir.join("document.worker.log"), &log);
    }
    Ok((status == 0, log))
}

fn workspace_size(dir: &Path, deadline: Instant, entry_limit: usize) -> Result<u64, String> {
    let mut total = 0u64;
    for (index, item) in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if index >= entry_limit {
            return Err("latex_output_limit".into());
        }
        if index % 64 == 0 {
            if crate::speech::cancelled() {
                return Err("cancelled".into());
            }
            if Instant::now() >= deadline {
                return Err("latex_timeout".into());
            }
        }
        let item = match item {
            Ok(item) => item,
            // TeX/fontconfig may remove or atomically rename temporary files
            // during enumeration. A vanished child is no longer output; root
            // loss, permission failures and other I/O errors must still fail.
            Err(error)
                if error.depth() > 0
                    && error
                        .io_error()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                continue
            }
            Err(_) => return Err("latex_workspace_failed".into()),
        };
        if item.file_type().is_file() {
            match item.metadata() {
                Ok(metadata) => total = total.saturating_add(metadata.len()),
                Err(error)
                    if error
                        .io_error()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    continue
                }
                Err(_) => return Err("latex_workspace_failed".into()),
            }
        }
        if total > 128 * 1024 * 1024 {
            return Err("latex_output_limit".into());
        }
    }
    Ok(total)
}

/// TeX diagnostic locations refer to the generated source, never guessed MD lines.
pub fn diagnostic(log: &str) -> String {
    let line = log
        .lines()
        .find(|s| {
            s.starts_with("./document.tex:")
                || s.starts_with("document.tex:")
                || s.starts_with("./document.aux:")
                || s.starts_with("document.aux:")
                || s.starts_with('!')
                || s.contains(":fatal:")
                || s.starts_with("Missing character:")
                || s.starts_with("Worker exited with status")
        })
        .unwrap_or("TeX did not produce a valid PDF.");
    line.chars().filter(|c| !c.is_control()).take(320).collect()
}

pub fn compile_source(
    source: &str,
    dir: &Path,
    engine: &str,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    compile_managed(source, dir, engine, timeout, &crate::paths::data_dir(), &crate::paths::assets_dir())
}

pub fn compile_managed(
    source: &str,
    dir: &Path,
    engine: &str,
    timeout: Duration,
    data: &Path,
    assets: &Path,
) -> Result<Vec<u8>, String> {
    compile_managed_with_geometry(source,dir,engine,timeout,data,assets,None)
}

fn compile_managed_with_geometry(
    source:&str, dir:&Path, engine:&str, timeout:Duration, data:&Path, assets:&Path,
    geometry:Option<&crate::latex_writer::ParagraphGeometry>,
) -> Result<Vec<u8>,String> {
    if !matches!(engine, "xelatex" | "pdflatex") {
        return Err("latex_invalid_engine".into());
    }
    if source.len() > 16 * 1024 * 1024 {
        return Err("latex_output_limit".into());
    }
    if crate::speech::cancelled() { return Err("cancelled".into()); }
    if timeout.is_zero() { return Err("latex_timeout".into()); }
    if ACTIVE.load(std::sync::atomic::Ordering::Acquire) >= 2 { return Err("latex_busy".into()); }
    let deadline = crate::publication_budget::deadline(timeout.min(Duration::from_secs(120)));
    let budget=crate::publication_budget::capture();budget.check()?;
    let capacity=crate::capability_process::check_worker_capacity(data,"latex");
    if crate::speech::cancelled() { return Err("cancelled".into()); }
    if Instant::now()>=deadline { return Err("latex_timeout".into()); }
    capacity.map_err(|e| {
        if e.code=="latex_busy" {e.code} else {"latex_engine_unavailable".into()}
    })?;
    // The lease is held through rendering and CMap completion. Repair and
    // uninstall cannot remove DLLs, fonts or packages underneath the worker.
    // Reserve after acquiring the resource guard but before hashing. A task
    // waiting for installation must not consume the slots its probe needs.
    let (runtime, _slot) = crate::capability_packs::lease_reserved(data, assets, "latex", &|| {
        budget.api()?;
        if crate::speech::cancelled() { Err(crate::ApiError::new(422, "cancelled")) }
        else if Instant::now() >= deadline { Err(crate::ApiError::new(422, "latex_timeout")) }
        else { Ok(()) }
    }, || crate::capability_process::reserve_worker(data,"latex",&ACTIVE)).map_err(|e| {
        if budget.interrupted(&e.code)||e.code=="latex_busy"{e.code}else{"latex_engine_unavailable".to_string()}
    })?;
    log::debug!(target:"readmd::tex", "managed lease validated");
    #[cfg(windows)]
    { compile_portable_at_with_geometry(source, dir, &runtime.directory, engine, deadline, Some(data), geometry) }
    #[cfg(not(windows))]
    { let _ = (runtime,geometry); Err("latex_engine_unavailable".into()) }
}

pub(crate) fn probe_directory(runtime: &Path, data: &Path) -> Result<(), String> {
    for (engine, source) in [
        ("xelatex", "\\documentclass[fontset=fandol]{ctexart}\n\\begin{document}ReadMD 离线排版 $x^2$.\\end{document}"),
        ("pdflatex", "\\documentclass{article}\\usepackage{lmodern}\n\\begin{document}ReadMD offline $x^2$.\\end{document}"),
    ] {
        let dir = tempfile::Builder::new().prefix("readmd-tex-probe-").tempdir().map_err(|_| "latex_workspace_failed")?;
        #[cfg(windows)] {
            let (_slot, deadline) = preflight(Duration::from_secs(30))?;
            compile_portable_at(source, dir.path(), runtime, engine, deadline, Some(data))?;
        }
        #[cfg(not(windows))] {
            let _ = data;
            compile_portable(source, dir.path(), runtime, engine, Duration::from_secs(30))?;
        }
    }
    Ok(())
}

fn compile_with(
    source: &str, dir: &Path, engine: &str, deadline: Instant,
    invoke: impl FnMut(&[&str], Instant) -> Result<(bool, String), String>,
) -> Result<Vec<u8>, String> {
    compile_with_geometry(source,dir,engine,deadline,None,invoke)
}

fn compile_with_geometry(
    source: &str, dir: &Path, engine: &str, deadline: Instant,
    geometry: Option<&crate::latex_writer::ParagraphGeometry>,
    mut invoke: impl FnMut(&[&str], Instant) -> Result<(bool, String), String>,
) -> Result<Vec<u8>, String> {
    if !matches!(engine, "xelatex" | "pdflatex") {
        return Err("latex_invalid_engine".into());
    }
    if source.len() > 16 * 1024 * 1024 {
        return Err("latex_output_limit".into());
    }
    let (ok, version) = invoke(&["--version"], deadline)?;
    if !ok {
        return Err(format!(
            "latex_engine_unavailable: {}",
            version
                .chars()
                .filter(|c| !c.is_control())
                .take(320)
                .collect::<String>()
        ));
    }
    fs::write(dir.join("document.tex"), source).map_err(|_| "latex_workspace_failed")?;
    for pass in 0..4 {
        let mut args = vec![
            "-no-shell-escape",
            "-halt-on-error",
            "-interaction=nonstopmode",
            "-file-line-error",
            "-no-parse-first-line",
            "-jobname=document",
        ];
        if version.contains("MiKTeX") {
            args.push("--disable-installer");
        }
        args.push("document.tex");
        let (ok, log) = invoke(&args, deadline)?;
        if !ok {
            if let Some(error) = geometry.and_then(|g|g.diagnostic(&log,source,dir)) { return Err(error); }
            return Err(format!("latex_compilation_failed: {}", diagnostic(&log)));
        }
        if log
            .lines()
            .any(|line| line.starts_with("Missing character:"))
        {
            return Err(format!("latex_compilation_failed: {}", diagnostic(&log)));
        }
        if pass >= 1 && !log.contains("Rerun to get") && !log.contains("Label(s) may have changed")
        {
            break;
        }
        if pass == 3 {
            return Err("latex_references_unsettled".into());
        }
    }
    if crate::speech::cancelled() {
        return Err("cancelled".into());
    }
    let target = dir.join("document.pdf");
    if fs::metadata(&target)
        .map_err(|_| "latex_invalid_output")?
        .len()
        > OUTPUT_LIMIT
    {
        return Err("latex_output_limit".into());
    }
    let bytes = fs::read(target).map_err(|_| "latex_invalid_output")?;
    let pdf = lopdf::Document::load_mem(&bytes).map_err(|_| "latex_invalid_output")?;
    if pdf.get_pages().is_empty() {
        return Err("latex_invalid_output".into());
    }
    Ok(bytes)
}

/// Compile with an explicitly supplied, application-owned portable distribution.
/// This entry never discovers engines, packages or fonts on the host. The caller
/// must first validate the managed distribution against its application lock.
#[cfg(windows)]
pub fn compile_portable(
    source: &str,
    dir: &Path,
    runtime: &Path,
    engine: &str,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    if !matches!(engine, "xelatex" | "pdflatex") {
        return Err("latex_invalid_engine".into());
    }
    if source.len() > 16 * 1024 * 1024 {
        return Err("latex_output_limit".into());
    }
    let (_slot, deadline) = preflight(timeout)?;
    compile_portable_at(source, dir, runtime, engine, deadline, None)
}

#[cfg(windows)]
fn compile_portable_at(source: &str, dir: &Path, runtime: &Path, engine: &str, deadline: Instant, data: Option<&Path>) -> Result<Vec<u8>, String> {
    compile_portable_at_with_geometry(source,dir,runtime,engine,deadline,data,None)
}

#[cfg(windows)]
fn compile_portable_at_with_geometry(source:&str, dir:&Path, runtime:&Path, engine:&str, deadline:Instant, data:Option<&Path>,
    geometry:Option<&crate::latex_writer::ParagraphGeometry>) -> Result<Vec<u8>,String> {
    use std::ffi::OsString;
    let runtime = dunce::canonicalize(runtime).map_err(|_| "latex_engine_unavailable")?;
    let dir = dunce::canonicalize(dir).map_err(|_| "latex_workspace_failed")?;
    log::debug!(target:"readmd::tex", "sandbox setup begins");
    let sandbox = if let Some(data) = data {
        let task = crate::task_workspace::Workspace::create(data, "latex-sandbox")
            .map_err(|_| "latex_workspace_failed")?;
        crate::worker_sandbox::Sandbox::new_for_task(&runtime, &dir, task)
    } else {
        crate::worker_sandbox::Sandbox::new(&runtime, &dir)
    }
        .map_err(|e| format!("latex_engine_unavailable: isolation setup: {e}"))?;
    log::debug!(target:"readmd::tex", "sandbox setup finished");
    let mut environment = sandbox
        .environment()
        .map_err(|_| "latex_engine_unavailable")?;
    // The executable view avoids parent enumeration during kpathsea startup.
    // kpathsea's resource file I/O expects ordinary filesystem paths instead.
    let bin = sandbox.runtime_view().join("bin/windows");
    let resources = sandbox.resource_view();
    let texpath = |path: &Path| path.to_string_lossy().replace('\\', "/");
    for (key, path) in [
        ("TEXMFROOT", resources.to_path_buf()),
        ("TEXMFDIST", resources.join("texmf-dist")),
        ("TEXMFSYSVAR", resources.join("texmf-var")),
        ("TEXMFSYSCONFIG", resources.join("texmf-config")),
        ("TEXMFHOME", dir.join("tex-home")),
        ("TEXMFCONFIG", dir.join("tex-config")),
        ("TEXMFVAR", dir.join("tex-cache")),
        ("TEXMFCACHE", dir.join("tex-cache")),
    ] {
        if path.starts_with(&dir) {
            fs::create_dir_all(&path).map_err(|_| "latex_workspace_failed")?;
        }
        environment.push((key.into(), texpath(&path).into()));
    }
    let font_cache = dir.join("font-cache");
    fs::create_dir(&font_cache).map_err(|_| "latex_workspace_failed")?;
    let xml = |path: &Path| {
        texpath(path)
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let config = dir.join("fonts.conf");
    fs::write(&config,format!("<?xml version=\"1.0\"?><fontconfig><dir>{}</dir><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",xml(&resources.join("texmf-dist/fonts/opentype")),xml(&resources.join("texmf-dist/fonts/truetype")),xml(&font_cache))).map_err(|_|"latex_workspace_failed")?;
    let system = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("latex_engine_unavailable")?
        .join("System32");
    environment.extend([
        (
            "PATH".into(),
            std::env::join_paths([&bin, &system]).map_err(|_| "latex_engine_unavailable")?,
        ),
        ("COMSPEC".into(), system.join("cmd.exe").into_os_string()),
        ("FONTCONFIG_FILE".into(), config.into_os_string()),
        ("FONTCONFIG_PATH".into(), dir.as_os_str().into()),
        (
            "OSFONTDIR".into(),
            OsString::from(texpath(&resources.join("texmf-dist/fonts")) + "//"),
        ),
        (
            "TEXMFCNF".into(),
            std::env::join_paths([resources.to_path_buf(), resources.join("texmf-dist/web2c")])
                .map_err(|_| "latex_engine_unavailable")?,
        ),
        (
            "TEXMF".into(),
            "{$TEXMFCONFIG,$TEXMFVAR,$TEXMFHOME,!!$TEXMFSYSCONFIG,!!$TEXMFSYSVAR,!!$TEXMFDIST}"
                .into(),
        ),
        ("shell_escape".into(), "0".into()),
        ("openout_any".into(), "p".into()),
        ("MKTEXFMT".into(), "0".into()),
        ("MKTEXPK".into(), "0".into()),
        ("MKTEXTFM".into(), "0".into()),
        ("TEXLIVE_WINDOWS_TRY_EXTERNAL_PERL".into(), "0".into()),
        ("SOURCE_DATE_EPOCH".into(), "0".into()),
    ]);
    let invoke = |program: &str, args: Vec<OsString>, deadline: Instant| {
        if crate::speech::cancelled() {
            return Err("cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("latex_timeout".into());
        }
        let capture =
            tempfile::NamedTempFile::new_in(&dir).map_err(|_| "latex_workspace_failed")?;
        let executable = runtime.join("bin/windows").join(format!("{program}.exe"));
        let worker = sandbox
            .spawn(&executable, &args, &environment, capture.as_file())
            .map_err(|e| format!("latex_engine_unavailable: isolation launch: {e}"))?;
        monitor(Process::Isolated(worker), capture, &dir, deadline)
    };
    let bytes = compile_with_geometry(source, &dir, engine, deadline, geometry, |args, deadline| {
        let source_pass = args.last() == Some(&"document.tex");
        // XeTeX normally asks the system shell to launch its PDF driver. Keep
        // both workers explicit, confined and under the same time/output budget.
        let mut argv = Vec::new();
        if source_pass && engine == "xelatex" {
            argv.push(OsString::from("-no-pdf"));
        }
        argv.extend(args.iter().map(OsString::from));
        let (ok, mut log) = invoke(engine, argv, deadline)?;
        if ok && source_pass && engine == "xelatex" {
            let (ok, driver) = invoke(
                "xdvipdfmx",
                ["-q", "-o", "document.pdf", "document.xdv"]
                    .into_iter()
                    .map(OsString::from)
                    .collect(),
                deadline,
            )?;
            log.push('\n');
            log.push_str(&driver);
            return Ok((ok, log));
        }
        Ok((ok, log))
    })?;
    complete_standard_cmaps(&bytes, &runtime, deadline)
}

/// xdvipdfmx deliberately omits ToUnicode for Adobe's standard CID collections.
/// Embed the reviewed runtime mapping so consumers need no external CMap set.
/// This changes text semantics only, preserving glyph IDs and page operators.
#[cfg(any(windows, test))]
fn complete_standard_cmaps(
    bytes: &[u8],
    runtime: &Path,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    if crate::speech::cancelled() {
        return Err("cancelled".into());
    }
    if Instant::now() >= deadline {
        return Err("latex_timeout".into());
    }
    let mut doc = lopdf::Document::load_mem(bytes).map_err(|_| "latex_invalid_output")?;
    let fonts = doc
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let font = object.as_dict().ok()?;
            if font.has(b"ToUnicode")
                || font.get(b"Subtype").ok()?.as_name().ok()? != b"Type0"
                || !matches!(
                    font.get(b"Encoding").ok()?.as_name().ok()?,
                    b"Identity-H" | b"Identity-V"
                )
            {
                return None;
            }
            let descendant = font
                .get(b"DescendantFonts")
                .ok()?
                .as_array()
                .ok()?
                .first()?;
            let descendant = doc.dereference(descendant).ok()?.1.as_dict().ok()?;
            let info = doc
                .dereference(descendant.get(b"CIDSystemInfo").ok()?)
                .ok()?
                .1
                .as_dict()
                .ok()?;
            if info.get(b"Registry").ok()?.as_str().ok()? != b"Adobe" {
                return None;
            }
            let collection = match info.get(b"Ordering").ok()?.as_str().ok()? {
                b"GB1" => "GB1",
                b"CNS1" => "CNS1",
                b"Japan1" => "Japan1",
                b"Korea1" => "Korea1",
                _ => return None,
            };
            Some((*id, collection))
        })
        .collect::<Vec<_>>();
    if fonts.is_empty() {
        return Ok(bytes.to_vec());
    }
    // Keep root and children in one namespace. `dunce` may simplify a short
    // Windows root but retain the verbatim prefix on its >MAX_PATH children,
    // which would falsely reject an owned CMap during the containment check.
    let runtime = fs::canonicalize(runtime).map_err(|_| "latex_engine_unavailable")?;
    let mut mappings = std::collections::HashMap::new();
    for (id, collection) in fonts {
        if crate::speech::cancelled() {
            return Err("cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("latex_timeout".into());
        }
        let map_id = if let Some(id) = mappings.get(collection) {
            *id
        } else {
            let file = runtime
                .join("texmf-dist/fonts/cmap/adobemapping/mapping-resources-pdf/pdf2unicode")
                .join(format!("Adobe-{collection}-UCS2"));
            let file = fs::canonicalize(file).map_err(|_| "latex_invalid_output")?;
            if !file.starts_with(&runtime) || !file.is_file() {
                return Err("latex_invalid_output".into());
            }
            let mut mapping = Vec::new();
            fs::File::open(file)
                .map_err(|_| "latex_invalid_output")?
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut mapping)
                .map_err(|_| "latex_invalid_output")?;
            if mapping.len() > 8 * 1024 * 1024 {
                return Err("latex_output_limit".into());
            }
            let mut stream = lopdf::Stream::new(lopdf::Dictionary::new(), mapping);
            stream.compress().map_err(|_| "latex_invalid_output")?;
            let id = doc.add_object(stream);
            mappings.insert(collection, id);
            id
        };
        doc.get_object_mut(id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|_| "latex_invalid_output")?
            .set("ToUnicode", map_id);
    }
    let mut output = Vec::new();
    doc.save_to(&mut output)
        .map_err(|_| "latex_invalid_output")?;
    if output.len() as u64 > OUTPUT_LIMIT {
        return Err("latex_output_limit".into());
    }
    if crate::speech::cancelled() {
        return Err("cancelled".into());
    }
    if Instant::now() >= deadline {
        return Err("latex_timeout".into());
    }
    Ok(output)
}
#[cfg(not(windows))]
pub fn compile_portable(
    _source: &str,
    _dir: &Path,
    _runtime: &Path,
    _engine: &str,
    _timeout: Duration,
) -> Result<Vec<u8>, String> {
    // No silent unrestricted fallback on a platform without a native sandbox.
    Err("latex_engine_unavailable".into())
}

fn validate_options(options: &Value) -> Result<(), String> {
    if !options.is_object() {
        return Err("latex_invalid_options".into());
    }
    for (key, allowed) in [
        (
            "docClass",
            &[
                "article", "report", "book", "beamer", "ctexart", "ctexrep", "ctexbook",
            ][..],
        ),
        ("fontSize", &["10pt", "11pt", "12pt"][..]),
        (
            "paperSize",
            &["a4paper", "a5paper", "b5paper", "letterpaper", "legalpaper"][..],
        ),
    ] {
        if let Some(value) = options
            .pointer(&format!("/tex/{key}"))
            .or_else(|| options.pointer(&format!("/latex/{key}")))
        {
            if !value.as_str().is_some_and(|s| allowed.contains(&s)) {
                return Err("latex_invalid_options".into());
            }
        }
    }
    if let Some(value) = options
        .pointer("/tex/margin")
        .or_else(|| options.pointer("/latex/margin"))
    {
        let value = value.as_str().ok_or("latex_invalid_options")?;
        let number = value
            .strip_suffix("cm")
            .or_else(|| value.strip_suffix("mm"))
            .or_else(|| value.strip_suffix("in"))
            .ok_or("latex_invalid_options")?;
        if !number
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && v > 0.0 && v <= 60.0)
        {
            return Err("latex_invalid_options".into());
        }
    }
    Ok(())
}

pub fn export_pdf(
    content: &str,
    base: &str,
    target: &str,
    options: &Value,
    name: &str,
) -> Result<ExportResult, String> {
    export_pdf_with_data(content, base, target, options, name, &crate::paths::data_dir(), &crate::paths::assets_dir())
}

pub fn export_pdf_with_data(
    content: &str, base: &str, target: &str, options: &Value, name: &str, data: &Path, assets: &Path,
) -> Result<ExportResult, String> {
    let resolved=crate::publication_profile::resolve_for_format("pdf",content,options).map_err(|e|e.code)?;
    export_pdf_with_lines(content,base,target,resolved.values(),name,data,assets,resolved.line_context())
}

pub(crate) fn export_pdf_with_lines(
    content:&str,base:&str,target:&str,options:&Value,name:&str,data:&Path,assets:&Path,
    lines:crate::publication_typography::LineContext,
) -> Result<ExportResult,String> {
    let profile=crate::publication_profile::resolve_for_format("pdf",content,options).map_err(|e|e.code)?;
    let options=profile.values();
    crate::publication_profile::preflight("pdf",content,options).map_err(|e|e.code)?;
    crate::publication_typography::validate_consumer(crate::publication_typography::Consumer::Tex, content, options).map_err(str::to_string)?;
    validate_options(options)?;
    let engine = options
        .pointer("/pdf/engine")
        .and_then(Value::as_str)
        .unwrap_or("xelatex");
    let directory = tempfile::Builder::new()
        .prefix("readmd-tex-")
        .tempdir()
        .map_err(|_| "latex_workspace_failed")?;
    // Managed PDF uses the shared typed CSL projection, not a second processor.
    let mut options = options.clone();
    options["tex"]["renderBibliography"] = Value::Bool(false);
    options["tex"]["bibEngine"] = Value::String("bibtex".into());
    let source = directory.path().join("document.tex");
    let (exported, core_text_used, geometry) = crate::mdexport::export_tex_with_geometry_and_lines(
        content,
        base,
        source.to_str().ok_or("latex_workspace_failed")?,
        &options,
        name,
        lines,
    )?;
    if engine == "pdflatex" && core_text_used {
        // This comes from the actual visible projection, not a text search over
        // author source, PDF metadata, comments or generated TeX. Fail before
        // leasing/launching an engine and before committing the destination.
        return Err("latex_compilation_failed: Core Unicode text fallback requires XeLaTeX.".into());
    }
    let tex = fs::read_to_string(source).map_err(|_| "latex_workspace_failed")?;
    let bytes = compile_managed_with_geometry(&tex, directory.path(), engine, Duration::from_secs(120), data, assets, geometry.as_ref())?;
    let metadata=crate::publication_metadata::from_source(content,&options,name).map_err(|e|e.code)?;
    let bytes=crate::publication_metadata_formats::pdf_metadata(&bytes,&metadata)?;
    if crate::speech::cancelled() {
        return Err("cancelled".into());
    }
    crate::publication_budget::check()?;
    crate::content::write_bytes_atomic(Path::new(target), &bytes)
        .map_err(|_| "latex_save_failed")?;
    Ok(ExportResult {
        ok: true,
        path: Some(target.into()),
        size: Some(bytes.len() as u64),
        warns: exported.warns,
        error: None,
        canceled: Some(false),
    })
}

#[cfg(test)]
#[path = "math_export_error_tests.rs"]
mod math_export_error_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn shaping_details_require_an_object_and_preserve_the_existing_range_contract(){
        let code=crate::api_codes::AUTHOR_INLINE_TEXT_PDF_SHAPING_UNSUPPORTED;
        let valid=serde_json::json!({"consumer":"rustPdf","coordinateSpace":"textUtf8Bytes","from":0,"to":8});
        assert_eq!(super::error_body(&format!("{code}: {valid}")),serde_json::json!({"ok":false,"error_code":code,"details":valid}));
        let mut maximum=valid.clone();maximum["to"]=serde_json::json!(crate::document_transaction::MAX_DOCUMENT_BYTES);
        assert_eq!(super::error_body(&format!("{code}: {maximum}"))["details"],maximum);
        let mut invalid=vec!["[\"rustPdf\",\"textUtf8Bytes\",0,8]".into(),"[]".into(),"null".into(),"{\"consumer\":\"rustPdf\",\"coordinateSpace\":\"textUtf8Bytes\",\"from\":0,\"from\":0,\"to\":8}".into(),format!("{valid}{}"," ".repeat(2049))];
        for (key,value) in [("from",serde_json::json!(-1)),("from",serde_json::json!(0.5)),("from",serde_json::json!("0")),("from",serde_json::json!(false)),("from",serde_json::json!(8)),("to",serde_json::json!(crate::document_transaction::MAX_DOCUMENT_BYTES+1))]{let mut raw=valid.clone();raw[key]=value;invalid.push(raw.to_string());}
        for text in invalid{assert_eq!(super::error_body(&format!("{code}: {text}")),serde_json::json!({"ok":false,"error_code":code,"details":text}));}
    }
    #[test]
    fn math_conflict_sequence_details_stay_untrusted_but_bounded_objects_remain_typed(){
        let code=crate::api_codes::MATH_DOCUMENT_DEFAULTS_CONFLICT;
        let valid=serde_json::json!({"property":"smallFrac","firstFormula":999999,"formula":1000000});
        assert_eq!(super::error_body(&format!("{code}: {valid}")),serde_json::json!({"ok":false,"error_code":code,"details":valid}));
        for text in ["[\"smallFrac\",1,2]".to_string(),"[]".into(),"null".into(),format!("{valid}{}"," ".repeat(2049))]{
            assert_eq!(super::error_body(&format!("{code}: {text}")),serde_json::json!({"ok":false,"error_code":code,"details":text}));
        }
    }
    #[test]
    fn tex_author_diagnostics_require_a_bounded_exact_object_and_utf8_byte_range(){
        let code=crate::api_codes::AUTHOR_INLINE_TEXT_UNSUPPORTED;
        let valid=serde_json::json!({"consumer":"tex","reason":"rtl","coordinateSpace":"textUtf8Bytes","from":0,"to":14});
        assert_eq!(super::error_body(&format!("{code}: {valid}")),serde_json::json!({"ok":false,"error_code":code,"details":valid}));
        let mut boundary=valid.clone();boundary["to"]=serde_json::json!(crate::document_transaction::MAX_DOCUMENT_BYTES);
        assert_eq!(super::error_body(&format!("{code}: {boundary}"))["details"],boundary);
        let mut invalid=Vec::new();
        for (key,value) in [("consumer",serde_json::json!("typst")),("reason",serde_json::json!("other")),("coordinateSpace",serde_json::json!("sourceUtf16")),("from",serde_json::json!(-1)),("from",serde_json::json!(0.5)),("from",serde_json::json!("0")),("from",serde_json::json!(false)),("from",serde_json::json!(14)),("from",serde_json::json!(15)),("to",serde_json::json!(0)),("to",serde_json::json!(crate::document_transaction::MAX_DOCUMENT_BYTES+1)),("unexpected",serde_json::json!(true))]{let mut value_=valid.clone();value_[key]=value;invalid.push(value_.to_string());}
        for key in ["consumer","reason","coordinateSpace","from","to"]{let mut value=valid.clone();value.as_object_mut().unwrap().remove(key);invalid.push(value.to_string());}
        invalid.extend(["[\"tex\",\"rtl\",\"textUtf8Bytes\",0,14]".into(),"[]".into(),"null".into(),"not-json".into(),format!("{valid} trailing"),"{\"consumer\":\"tex\",\"consumer\":\"tex\",\"reason\":\"rtl\",\"coordinateSpace\":\"textUtf8Bytes\",\"from\":0,\"to\":14}".into(),format!("{}{}",valid," ".repeat(2049))]);
        for text in invalid {let result=super::error_body(&format!("{code}: {text}"));assert_eq!(result["error_code"],code,"{text}");assert_eq!(result["details"],text,"Malformed details must remain untrusted text");}
        assert_eq!(super::error_body(&format!("{code}:{valid}"))["error"],format!("{code}:{valid}"));
    }
    #[test]
    fn note_layout_error_preserves_typed_pages_but_never_promotes_invalid_details(){
        let valid=serde_json::json!({"coordinateSpace":"pdfFootnoteStartPage","noteIndex":31,"definitionPage":2,"referencePage":3});
        let result=super::error_body(&format!("publication_note_layout: {valid}"));
        assert_eq!(result["error_code"],"publication_note_layout");assert_eq!(result["details"],valid);
        let mut invalid=Vec::new();
        for (key,value) in [("noteIndex",serde_json::json!(0)),("noteIndex",serde_json::json!(32769)),("definitionPage",serde_json::json!(-1)),("referencePage",serde_json::json!("3")),("referencePage",serde_json::json!(100001)),("referencePage",serde_json::json!(2)),("coordinateSpace",serde_json::json!("other")),("unexpected",serde_json::json!(true))]{let mut v=valid.clone();v[key]=value;invalid.push(v);}
        for value in invalid {let result=super::error_body(&format!("publication_note_layout: {value}"));assert_eq!(result["error_code"],"publication_note_layout");assert!(result["details"].is_string());}
    }
    use super::*;
    #[test]
    fn formula_export_errors_have_localizable_envelopes_without_reclassifying_plain_errors() {
        for code in ["math_invalid_source", "math_invalid_property", "math_unsupported_version", "math_metadata_stale", "math_metadata_invalid"] {
            assert_eq!(error_body(code), serde_json::json!({"ok":false,"error_code":code,"details":""}));
            assert_eq!(error_body(&format!("{code}: token 7")), serde_json::json!({"ok":false,"error_code":code,"details":"token 7"}));
        }
        assert_eq!(error_body("Unknown file: unchanged"), serde_json::json!({"ok":false,"error":"Unknown file: unchanged"}));
    }
    #[test]
    fn missing_pdf_glyph_details_keep_local_utf16_coordinates_at_all_export_boundaries(){
        let details=serde_json::json!({"codepoint":"U+10FFFF","page":1,"item":3,"span":1,"coordinateSpace":"pdfSpanUtf16","from":4,"to":6,"x":12.5,"y":31.0});
        let body=error_body(&format!("pdf_missing_glyph: {details}"));
        assert_eq!(body["error_code"],"pdf_missing_glyph");assert_eq!(body["details"],details);
        for text in ["not-json","{\"coordinateSpace\":\"source\"}","{\"codepoint\":\"U+1F600\",\"untrusted\":true}"]{
            assert_eq!(error_body(&format!("pdf_missing_glyph: {text}"))["details"],text);
        }
    }
    #[test]
    fn engine_parameters_cannot_inject_tex_commands() {
        assert!(validate_options(
            &serde_json::json!({"tex":{"margin":"2.5cm","docClass":"article"}})
        )
        .is_ok());
        for options in [
            serde_json::json!({"tex":{"margin":"1cm}\\input{private}"}}),
            serde_json::json!({"tex":{"docClass":"article}\\input{private}"}}),
            serde_json::json!({"tex":{"fontSize":"14pt"}}),
        ] {
            assert_eq!(
                validate_options(&options).unwrap_err(),
                "latex_invalid_options"
            );
        }
        assert_eq!(
            diagnostic("unrelated path\n./document.tex:16: Undefined control sequence.\nlog tail"),
            "./document.tex:16: Undefined control sequence."
        );
        assert_eq!(
            diagnostic(
                "worker output\n!pdfTeX error: font outline not found\n ==> Fatal error occurred"
            ),
            "!pdfTeX error: font outline not found"
        );
    }
    #[test]
    fn diagnostic_keeps_generated_aux_error_location_and_first_error_order() {
        let error = "./document.aux:12: LaTeX Error: Missing \\begin{document}.";
        let log = format!("(./document.aux\n{error}\nSee the LaTeX manual.\nl.12 g\n      defreadmdtotalpages{{3}}\n./document.tex:90: Secondary error.");
        assert_eq!(diagnostic(&log), error);
        assert_eq!(diagnostic("(document.aux)\ndocument.aux:15: Undefined control sequence.\nl.15 text"),
            "document.aux:15: Undefined control sequence.");
        let earlier = "./document.tex:8: Undefined control sequence.";
        assert_eq!(diagnostic(&format!("{earlier}\n{error}")), earlier);
    }

    #[test]
    fn diagnostic_does_not_guess_aux_locations_from_arbitrary_paths_or_mentions() {
        let log = "(./document.aux)\nA note mentions ./document.aux:12: text\n/private/document.aux:12: text\nC:/private/document.aux:12: text\n./source.md:12: text\n./document.auxiliary:12: text";
        assert_eq!(diagnostic(log), "TeX did not produce a valid PDF.");
    }

    #[test]
    fn diagnostic_aux_keeps_control_filter_and_character_budget() {
        let line = format!("./document.aux:12: LaTeX Error:\t{}\u{7f}", "坏".repeat(400));
        let result = diagnostic(&line);
        let clean = line.chars().filter(|c| !c.is_control()).take(320).collect::<String>();
        assert_eq!(result, clean);
        assert_eq!(result.chars().count(), 320);
        assert!(!result.chars().any(char::is_control));
    }

    #[test]
    fn unknown_engine_is_rejected_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            compile_source("text", dir.path(), "sh", Duration::from_secs(1)).unwrap_err(),
            "latex_invalid_engine"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn absent_managed_runtime_never_uses_host_or_creates_compile_files() {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let assets = tempfile::tempdir().unwrap();
        for engine in ["xelatex", "pdflatex"] {
            assert_eq!(compile_managed("\\documentclass{article}", dir.path(), engine,
                Duration::from_secs(5), data.path(), assets.path()).unwrap_err(), "latex_engine_unavailable");
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        assert_eq!(fs::read_dir(data.path()).unwrap().count(), 0);
        assert_eq!(compile_managed("text", dir.path(), "xelatex", Duration::ZERO,
            data.path(), assets.path()).unwrap_err(), "latex_timeout");
    }

    #[test]
    fn unavailable_runtime_keeps_existing_pdf_and_input_directory_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let assets = tempfile::tempdir().unwrap();
        let target = dir.path().join("paper.pdf");
        fs::write(&target, b"previous PDF stays intact").unwrap();
        let result = export_pdf_with_data("# Research\n\nText $x^2$.",
            dir.path().to_str().unwrap(), target.to_str().unwrap(),
            &serde_json::json!({"pdf":{"engine":"xelatex"}}), "Research", data.path(), assets.path());
        assert_eq!(result.unwrap_err(), "latex_engine_unavailable");
        assert_eq!(fs::read(&target).unwrap(), b"previous PDF stays intact");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn pdflatex_smp_text_is_rejected_before_runtime_lease_and_pdf_commit() {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let assets = tempfile::tempdir().unwrap();
        let target = dir.path().join("paper.pdf");
        fs::write(&target, b"Previous PDF remains intact").unwrap();
        let error = export_pdf_with_data("# Paper\n\nVisible 😀.", dir.path().to_str().unwrap(),
            target.to_str().unwrap(), &serde_json::json!({"pdf":{"engine":"pdflatex"}}),
            "Paper", data.path(), assets.path()).unwrap_err();
        assert_eq!(error, "latex_compilation_failed: Core Unicode text fallback requires XeLaTeX.");
        assert_eq!(error_body(&error)["error_code"], "latex_compilation_failed");
        assert_eq!(fs::read(&target).unwrap(), b"Previous PDF remains intact");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(fs::read_dir(data.path()).unwrap().count(), 0);
        assert_eq!(fs::read_dir(assets.path()).unwrap().count(), 0);
    }

    #[test]
    fn workspace_inventory_is_bounded_and_requires_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        assert_eq!(
            workspace_size(&dir.path().join("missing"), deadline, 16).unwrap_err(),
            "latex_workspace_failed"
        );
        for i in 0..20 {
            fs::write(dir.path().join(format!("output-{i}")), b"data").unwrap();
        }
        assert_eq!(
            workspace_size(dir.path(), deadline, 16).unwrap_err(),
            "latex_output_limit"
        );
        assert_eq!(
            workspace_size(dir.path(), Instant::now(), 64).unwrap_err(),
            "latex_timeout"
        );
        assert_eq!(workspace_size(dir.path(), deadline, 64).unwrap(), 80);
    }

    #[test]
    fn workspace_inventory_tolerates_real_temporary_file_churn() {
        let dir = tempfile::tempdir().unwrap();
        let done = std::sync::atomic::AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(10);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let a = dir.path().join("cache-new");
                let b = dir.path().join("cache-old");
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    fs::write(&a, b"temporary font cache").unwrap();
                    fs::rename(&a, &b).unwrap();
                    fs::remove_file(&b).unwrap();
                }
            });
            for _ in 0..500 {
                let result = workspace_size(dir.path(), deadline, 64);
                if result.is_err() {
                    done.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                assert!(result.is_ok(), "{result:?}");
            }
            done.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    #[test]
    fn standard_cid_maps_are_embedded_once_and_existing_maps_are_preserved() {
        use lopdf::{dictionary, Document, Object, Stream};
        let runtime = tempfile::tempdir().unwrap();
        let dir = runtime
            .path()
            .join("texmf-dist/fonts/cmap/adobemapping/mapping-resources-pdf/pdf2unicode");
        fs::create_dir_all(&dir).unwrap();
        let mapping = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CMapType 2 def\n1 begincodespacerange\n<0000> <ffff>\nendcodespacerange\n1 beginbfchar\n<0001> <7814>\nendbfchar\nendcmap\nend\nend\n";
        fs::write(dir.join("Adobe-GB1-UCS2"), mapping).unwrap();
        let mut doc = Document::with_version("1.7");
        let descendant = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"CIDFontType0","CIDSystemInfo"=>dictionary!{"Registry"=>Object::string_literal("Adobe"),"Ordering"=>Object::string_literal("GB1"),"Supplement"=>5}});
        let font = dictionary! {"Type"=>"Font","Subtype"=>"Type0","Encoding"=>"Identity-H","DescendantFonts"=>vec![Object::Reference(descendant)]};
        let a = doc.add_object(font.clone());
        let b = doc.add_object(font.clone());
        let original = doc.add_object(Stream::new(dictionary! {}, b"original mapping".to_vec()));
        let mut protected = font;
        protected.set("ToUnicode", original);
        let c = doc.add_object(protected);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let output = complete_standard_cmaps(&bytes, runtime.path(), deadline).unwrap();
        let parsed = Document::load_mem(&output).unwrap();
        let map_a = parsed
            .get_dictionary(a)
            .unwrap()
            .get(b"ToUnicode")
            .unwrap()
            .as_reference()
            .unwrap();
        let map_b = parsed
            .get_dictionary(b)
            .unwrap()
            .get(b"ToUnicode")
            .unwrap()
            .as_reference()
            .unwrap();
        assert_eq!(map_a, map_b);
        assert_eq!(
            parsed
                .get_dictionary(c)
                .unwrap()
                .get(b"ToUnicode")
                .unwrap()
                .as_reference()
                .unwrap(),
            original
        );
        assert_eq!(
            parsed
                .get_object(map_a)
                .unwrap()
                .as_stream()
                .unwrap()
                .decompressed_content()
                .unwrap(),
            mapping
        );
        assert_eq!(
            parsed
                .get_object(original)
                .unwrap()
                .as_stream()
                .unwrap()
                .content,
            b"original mapping"
        );
        fs::remove_file(dir.join("Adobe-GB1-UCS2")).unwrap();
        assert_eq!(
            complete_standard_cmaps(&bytes, runtime.path(), deadline).unwrap_err(),
            "latex_invalid_output"
        );
        assert_eq!(
            complete_standard_cmaps(&bytes, runtime.path(), Instant::now()).unwrap_err(),
            "latex_timeout"
        );
    }
    #[cfg(windows)]
    #[test]
    fn standard_cid_maps_accept_long_children_without_weakening_containment() {
        use lopdf::{dictionary, Document, Object};
        use std::os::windows::ffi::OsStrExt;
        let fixture = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(fixture.path()).unwrap();
        let root = base.join("r".repeat(235 - base.as_os_str().encode_wide().count() - 1));
        let directory = root.join("texmf-dist/fonts/cmap/adobemapping/mapping-resources-pdf/pdf2unicode");
        fs::create_dir_all(&directory).unwrap();
        let map = directory.join("Adobe-GB1-UCS2");
        let contents = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CMapType 2 def\n1 begincodespacerange\n<0000> <ffff>\nendcodespacerange\n1 beginbfchar\n<0001> <7814>\nendbfchar\nendcmap\nend\nend\n";
        fs::write(&map, contents).unwrap();
        assert!(root.as_os_str().encode_wide().count() < 248);
        assert!(map.as_os_str().encode_wide().count() > 260);
        // This is the exact namespace split that rejected a genuine installed
        // CMap: a simplified root and a verbatim long child are different paths.
        assert!(!dunce::canonicalize(&map).unwrap().starts_with(dunce::canonicalize(&root).unwrap()));
        let mut doc = Document::with_version("1.7");
        let cid = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"CIDFontType0","CIDSystemInfo"=>dictionary!{"Registry"=>Object::string_literal("Adobe"),"Ordering"=>Object::string_literal("GB1"),"Supplement"=>5}});
        let font = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type0","Encoding"=>"Identity-H","DescendantFonts"=>vec![Object::Reference(cid)]});
        let mut input = Vec::new(); doc.save_to(&mut input).unwrap();
        let output = complete_standard_cmaps(&input, &root, Instant::now()+Duration::from_secs(10)).unwrap();
        let parsed = Document::load_mem(&output).unwrap();
        let reference = parsed.get_dictionary(font).unwrap().get(b"ToUnicode").unwrap().as_reference().unwrap();
        assert_eq!(parsed.get_object(reference).unwrap().as_stream().unwrap().decompressed_content().unwrap(), contents);
    }
}
