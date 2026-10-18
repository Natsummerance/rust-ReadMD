//! Dev-only managed geometry-packet diagnostic. A successful probe expects a
//! failed compilation, preserves the actual worker log and certifies no output.
use readmd_kernel::{latex_compile, mdexport};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}, time::{Duration, Instant}};

fn sha(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn fresh(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?; file.sync_all()
}
fn existing(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if !path.is_absolute() { return Err("Explicit absolute input required".into()); }
    let canonical = dunce::canonicalize(path)?;
    if canonical != path { return Err("Redirected input rejected".into()); }
    Ok(canonical)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 3 { return Err("Expected fixed-owned-profile assets NEW-owned-output-directory".into()); }
    let repo = dunce::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))?;
    let cache = dunce::canonicalize(repo.join(".cache/capability-upgrade"))?;
    let data = existing(&args[0])?; let assets = existing(&args[1])?;
    if data != cache.join("publication-engines-native-n2e7Ug") || !assets.starts_with(&cache) || !assets.is_dir() {
        return Err("Only fixed QA managed profile and its owned asset overlay are accepted".into());
    }
    let out = &args[2];
    if !out.is_absolute() || fs::symlink_metadata(out).is_ok() { return Err("Output must be a new absolute directory".into()); }
    if !existing(out.parent().ok_or("Missing output parent")?)?.starts_with(&cache) { return Err("Output must remain in owned cache".into()); }
    fs::create_dir(out)?;
    // The unchanged first rejected native fixture: actual hard breaks + tall math.
    let markdown = "\u{feff}FIRST  \r\nSECOND $\\dfrac{X}{\\dfrac{Y}{Z}}$  \r\nTHIRD\r\n";
    let options = json!({"cover":{"enabled":false},"toc":{"enabled":false},"frontMatter":{"mode":"none"},
        "page":{"size":"A4","marginTop":20,"marginBottom":20,"marginLeft":18,"marginRight":18},
        "typography":{"font":"Arial","size":11,"spacing":0,"align":"left","firstLineIndent":0,"keepLines":false,"lineSpacing":{"mode":"exact","pt":18}},
        "footer":{"pageNumbers":false,"text":""},"header":{"text":""},"pdf":{"engine":"xelatex"},
        "tex":{"renderBibliography":false,"bibEngine":"bibtex"}});
    fresh(&out.join("Tall-original.md"), markdown.as_bytes())?;
    fresh(&out.join("options.json"), &serde_json::to_vec_pretty(&options)?)?;
    let target = out.join("document.tex");
    mdexport::export_tex(markdown, out.to_str().ok_or("Invalid directory")?, target.to_str().ok_or("Invalid target")?, &options, "Geometry")?;
    let original = fs::read_to_string(&target)?;
    fresh(&out.join("writer-original.tex"), original.as_bytes())?;
    let marker = original.lines().find_map(|line| line.strip_prefix(r"\def\readmdpmarker{").and_then(|line| line.strip_suffix('}'))).ok_or("Missing generated marker")?;
    // Two lexical controls in the same real managed invocation. They are not
    // trusted packets and the public compile helper has no GeometryContext.
    let contrasts = "\\begin{document}\n\\typeout{readmd-p04-probe-old:\\readmdpmarker BEGIN}\n\\typeout{readmd-p04-probe-new:\\readmdpmarker\\space BEGIN}";
    if original.matches(r"\begin{document}").count() != 1 { return Err("Generated document boundary must be unique".into()); }
    let source = original.replacen(r"\begin{document}", contrasts, 1);
    let started = Instant::now();
    let result = latex_compile::compile_managed(&source, out, "xelatex", Duration::from_secs(120), &data, &assets);
    let elapsed = started.elapsed().as_millis();
    let log = fs::read_to_string(out.join("document.worker.log")).unwrap_or_default();
    let old = format!("readmd-p04-probe-old:{marker}BEGIN");
    let new = format!("readmd-p04-probe-new:{marker} BEGIN");
    let packet: Vec<_> = log.lines().filter_map(|line| line.strip_prefix(&format!("{marker} "))).collect();
    let keys = ["index=", "mode=", "requested=", "required=", "width=", "height=", "reserved="];
    let frame = packet.len() == 9 && packet[0] == "BEGIN" && packet[8] == "END"
        && keys.iter().enumerate().all(|(index, key)| packet[index + 1].strip_prefix(key).is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())));
    let error = result.as_ref().err().cloned();
    let expected = result.is_err() && log.lines().any(|line| line == old) && log.lines().any(|line| line == new) && frame
        && log.lines().any(|line| line.ends_with(": Package ReadMD Error: readmd-p04-geometry."));
    let mut artifacts = Vec::new();
    for name in ["Tall-original.md", "options.json", "writer-original.tex", "document.tex", "document.log", "document.worker.log", "document.xdv", "document.pdf"] {
        let file = out.join(name); if file.is_file() { let bytes = fs::read(file)?; artifacts.push(json!({"file":name,"bytes":bytes.len(),"sha256":sha(&bytes)})); }
    }
    fresh(&out.join("report.json"), &serde_json::to_vec_pretty(&json!({
        "scope":"managed-producer-lexical-diagnostic-only","acceptance":false,"expectedCompileSuccess":false,
        "compilerError":error,"allMatchedExpected":expected,"compileMs":elapsed,"managedBudgetMs":120000,
        "managedInvocations":1,"noDirectEngineInvocation":true,"cwd":out,"data":data,"assets":assets,
        "oldSourceSpaceWasElided":log.lines().any(|line|line==old),"explicitSpaceWasPreserved":log.lines().any(|line|line==new),
        "geometryPacket":packet,"packetComplete":frame,"sourceSha256":sha(source.as_bytes()),
        "exampleSha256":sha(include_bytes!("publication_tex_geometry_probe.rs")),"artifacts":artifacts
    }))?)?;
    println!("Probe report: {} (matched={expected}, acceptance=false)", out.join("report.json").display());
    if !expected { return Err("Managed geometry framing controls did not match; inspect preserved raw log".into()); }
    Ok(())
}
