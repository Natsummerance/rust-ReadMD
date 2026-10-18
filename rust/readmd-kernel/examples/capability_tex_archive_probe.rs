//! Explicit offline Windows capacity QA of maintainer-pinned, unapproved TeX
//! input. This does not install a capability or confer redistribution approval.
use readmd_kernel::{capability_archive, latex_compile, mdexport};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    io::Read,
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Opt-in Windows x64 QA only".into());
    }
    let mut args = std::env::args_os().skip(1);
    let manifest_path = PathBuf::from(args.next().ok_or("Fixture manifest required")?);
    let archive = PathBuf::from(args.next().ok_or("Fixture archive required")?);
    let inventory = PathBuf::from(args.next().ok_or("Pinned runtime inventory required")?);
    let parent = PathBuf::from(args.next().ok_or("Owned QA parent required")?);
    if fs::metadata(&manifest_path)?.len() > capability_archive::MAX_MANIFEST as u64
        || fs::metadata(&inventory)?.len() > 8 * 1024 * 1024
        || fs::metadata(&archive)?.len() > capability_archive::MAX_DOWNLOAD
    {
        return Err("QA input budget exceeded".into());
    }
    let expected: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let inventory_bytes = fs::read(&inventory)?;
    let receipt: Value = serde_json::from_slice(&inventory_bytes)?;
    assert_eq!(expected["id"], "tex-review-fixture");
    assert_eq!(expected["version"], "2026.10");
    assert_eq!(expected["platform"], "windows-x64");
    assert_eq!(expected["review"]["redistributionApproved"], false);
    assert_eq!(receipt["redistributionApproved"], false);
    assert_eq!(
        expected["review"]["runtimeInventorySha256"],
        format!("{:x}", Sha256::digest(&inventory_bytes))
    );
    assert_eq!(
        receipt["lockSha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "../../../tools/tex-publication-inputs.lock.json"
            ))
        )
    );
    let files = expected["files"]
        .as_array()
        .ok_or("Invalid manifest files")?;
    let known = receipt["files"].as_array().ok_or("Invalid runtime files")?;
    assert_eq!(files.len(), 34822);
    assert_eq!(files.len(), known.len());
    for (actual, original) in files.iter().zip(known) {
        for key in ["path", "size", "sha256"] {
            assert_eq!(actual[key], original[key]);
        }
    }
    let root = parent.join(format!("tex-stream-native-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    let staging = root.join("staging");
    fs::create_dir(&staging)?;
    let previous = root.join("previous-version");
    fs::create_dir(&previous)?;
    fs::write(
        previous.join("keep.txt"),
        "old installed resource remains untouched",
    )?;
    let cancel = Cell::new(false);
    let started = Instant::now();
    let cancelled = capability_archive::stage(
        fs::File::open(&archive)?,
        &expected,
        &staging,
        || cancel.get(),
        |p| {
            if p.files_done >= 500 {
                cancel.set(true)
            }
        },
    );
    assert!(matches!(
        cancelled,
        Err(capability_archive::Error::Cancelled)
    ));
    assert_eq!(fs::read_dir(&staging)?.count(), 0);
    let cancelled_ms = started.elapsed().as_millis();
    let mut progress = None;
    let started = Instant::now();
    let staged = capability_archive::stage(
        fs::File::open(&archive)?,
        &expected,
        &staging,
        || false,
        |p| progress = Some(p),
    )?;
    let extraction_ms = started.elapsed().as_millis();
    let progress = progress.ok_or("Missing extraction progress")?;
    assert_eq!(progress.files_done, 34822);
    assert_eq!(
        progress.expanded_bytes,
        receipt["totalBytes"]
            .as_u64()
            .ok_or("Missing byte budget")?
    );
    assert_eq!(progress.expanded_bytes, progress.expanded_total);
    // The real native compiler sees only the staged runtime and its own task
    // directory. No program is copied onto PATH or installed on the host.
    let work = root.join("publication");
    fs::create_dir(&work)?;
    let source = work.join("document.tex");
    let content="# 离线书籍验证\n\n## 证据与推理\n\n安装完整运行库后，原文、公式与章节均可继续使用。\n\n| 方法 | 样本 |\n|:---|---:|\n| 对照 | 120 |\n\n$$E=mc^2$$\n";
    fs::write(root.join("Publication.md"), content)?;
    mdexport::export_tex(
        content,
        work.to_str().ok_or("Work path")?,
        source.to_str().ok_or("Source path")?,
        &json!({"tex":{"docClass":"ctexbook","bibEngine":"bibtex"},"meta":{"title":"ReadMD 独立运行库验证","author":"ReadMD"}}),
        "Publication",
    )?;
    let bytes = latex_compile::compile_portable(
        &fs::read_to_string(source)?,
        &work,
        staged.path(),
        "xelatex",
        Duration::from_secs(120),
    )?;
    let extracted = pdf_extract::extract_text_from_mem(&bytes)?;
    let text = extracted
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert!(text.contains("离线书籍验证") && text.contains("证据与推理") && text.contains("120"));
    assert!(!text.contains("0.1"));
    let pages = lopdf::Document::load_mem(&bytes)?.get_pages().len();
    fs::write(root.join("Publication.pdf"), &bytes)?;
    fs::write(root.join("extracted.txt"), &extracted)?;
    drop(staged);
    assert_eq!(fs::read_dir(&staging)?.count(), 0);
    assert_eq!(
        fs::read_to_string(previous.join("keep.txt"))?,
        "old installed resource remains untouched"
    );
    let mut digest = Sha256::new();
    let mut file = fs::File::open(&archive)?;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let report = json!({"passed":true,"platform":"windows-x64","offline":true,"checks":["cancel-after-500-files-and-clean-only-task-stage","complete-inventory-hashes-and-expanded-size","real-chinese-book-pdf-from-staged-runtime","finished-stage-cleaned-previous-resource-preserved"],"files":progress.files_done,"installedBytes":progress.expanded_bytes,"archiveBytes":fs::metadata(&archive)?.len(),"archiveSha256":format!("{:x}",digest.finalize()),"manifestSha256":format!("{:x}",Sha256::digest(fs::read(manifest_path)?)),"cancelledAttemptMs":cancelled_ms,"extractionMs":extraction_ms,"pages":pages,"pdfSha256":format!("{:x}",Sha256::digest(&bytes)),"redistributionApproved":false,"applicationInstalled":false,"published":false,"scope":"Full real TeX input through production streaming extractor and Windows isolated compiler. Not catalog/install/repair/uninstall UI certification or other platforms."});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        json!({"passed":true,"files":progress.files_done,"checks":4,"report":root.join("report.json")})
    );
    Ok(())
}
