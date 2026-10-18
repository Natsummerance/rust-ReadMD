//! Explicit Windows native format probe with the application's reviewed Typst
//! bytes. This is not an install/catalog/desktop UI certification.
use readmd_kernel::capability_archive;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("This opt-in native fixture is Windows x64 only".into());
    }
    let mut args = std::env::args_os().skip(1);
    let manifest_path = PathBuf::from(args.next().ok_or("Reviewed v2 manifest required")?);
    let pack = PathBuf::from(args.next().ok_or("Local v2 archive required")?);
    let parent = PathBuf::from(args.next().ok_or("Owned evidence parent required")?);
    if fs::metadata(&manifest_path)?.len() > capability_archive::MAX_MANIFEST as u64
        || fs::metadata(&pack)?.len() > capability_archive::MAX_DOWNLOAD
    {
        return Err("Fixture budget exceeded".into());
    }
    let manifest_bytes = fs::read(&manifest_path)?;
    let expected: Value = serde_json::from_slice(&manifest_bytes)?;
    let known: Value = serde_json::from_str(include_str!(
        "../../../assets/capabilities/typst-0.15.1-windows-x64.lock.json"
    ))?;
    assert_eq!(expected["id"], known["id"]);
    assert_eq!(expected["version"], known["version"]);
    assert_eq!(expected["platform"], known["platform"]);
    assert_eq!(
        expected["files"].as_array().unwrap().len(),
        known["files"].as_array().unwrap().len()
    );
    for item in known["files"].as_array().unwrap() {
        let actual = expected["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == item["path"])
            .ok_or("Missing pinned file")?;
        assert_eq!(actual["size"], item["size"]);
        assert_eq!(actual["sha256"], item["sha256"]);
    }
    let root = parent.join(format!("stream-archive-native-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    let staging_parent = root.join("staging");
    fs::create_dir(&staging_parent)?;
    let started = Instant::now();
    let mut progress = None;
    let staged = capability_archive::stage(
        fs::File::open(&pack)?,
        &expected,
        &staging_parent,
        || false,
        |p| progress = Some(p),
    )?;
    let stage_ms = started.elapsed().as_millis();
    let progress = progress.ok_or("No extraction progress")?;
    assert_eq!(
        progress.files_done,
        known["files"].as_array().unwrap().len()
    );
    assert_eq!(progress.expanded_bytes, progress.expanded_total);
    let work = root.join("render");
    fs::create_dir(&work)?;
    // Avoid executing in a staging directory which must be removable/committable.
    let program = work.join("typst.exe");
    fs::copy(staged.path().join("bin/typst.exe"), &program)?;
    drop(staged);
    assert_eq!(fs::read_dir(&staging_parent)?.count(), 0);
    fs::create_dir(work.join("packages"))?;
    fs::write(work.join("document.typ"), "#set document(title: \"ReadMD v2 stream evidence\")\nReadMD stream archive evidence. $E=m c^2$.")?;
    let log = fs::File::create(work.join("compile.log"))?;
    let mut command = Command::new(&program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.env_clear();
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system);
    }
    for key in ["TEMP", "TMP", "HOME", "USERPROFILE"] {
        command.env(key, &work);
    }
    command
        .current_dir(&work)
        .args(["compile", "--ignore-system-fonts", "--root"])
        .arg(&work)
        .arg("--package-path")
        .arg(work.join("packages"))
        .arg("--package-cache-path")
        .arg(work.join("packages"))
        .args([
            "--creation-timestamp",
            "0",
            "--jobs",
            "1",
            "document.typ",
            "document.pdf",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Native engine fixture timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "Native compilation must succeed");
    let pdf = fs::read(work.join("document.pdf"))?;
    assert!(pdf_extract::extract_text_from_mem(&pdf)?.contains("ReadMD stream archive evidence"));
    assert!(!lopdf::Document::load_mem(&pdf)?.get_pages().is_empty());
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut input = fs::File::open(&pack)?;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"platform":"windows-x64","offline":true,"syntheticDocument":true,"format":"stream-v2","files":progress.files_done,"expandedBytes":progress.expanded_bytes,"archiveBytes":fs::metadata(&pack)?.len(),"archiveSha256":format!("{:x}",digest.finalize()),"manifestSha256":format!("{:x}",Sha256::digest(&manifest_bytes)),"stageMs":stage_ms,"stagingRemoved":true,"actualSearchablePdf":true,"hostFontsIgnored":true,"scope":"Native streamed extraction and actual compilation with the pinned Typst engine; not production v2 installer, catalog, UI or full large-TeX certification."}),
        )?,
    )?;
    println!(
        "{}",
        json!({"passed":true,"report":root.join("report.json")})
    );
    Ok(())
}
