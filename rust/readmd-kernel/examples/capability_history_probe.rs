//! Upgrade recovery with genuine, unchanged font resources: previous service
//! writes schema-1 journals, current service trusts the exact schema-2 encoding
//! of the same package and a pinned application-owned historical lock.
use base64::{engine::general_purpose::STANDARD, Engine};
use readmd_kernel::{capability_archive, capability_packs, mdexport, paths, server::Request, App};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CONTENT:&str="# 恢复后的出版结果\n\n保留原文和用户资料，验证实际安装服务在异常终止之后恢复。\n\n$E=mc^2$ 与 :math[frac(α + x^2, sqrt(y))]。\n";
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn boot(root: &Path, assets: &Path) -> Arc<App> {
    Arc::new(
        App::bootstrap(paths::AppPaths::with_dirs(
            &root.join("profile"),
            &root.join("documents"),
            assets,
        ))
        .unwrap(),
    )
}
fn api(app: &Arc<App>, action: &str, pack: &Path) -> Value {
    match capability_packs::endpoint(
        app,
        &Request {
            method: "POST".into(),
            path: "/api/capabilities/packs".into(),
            query: Default::default(),
            headers: Default::default(),
            body: serde_json::to_vec(&json!({"action":action,"id":"fonts","path":pack})).unwrap(),
        },
    ) {
        Ok(response) => {
            assert_eq!(response.status, 200);
            serde_json::from_slice(&response.body).unwrap()
        }
        Err(e) => json!({"ok":false,"status":e.status,"error_code":e.code}),
    }
}
fn export(root: &Path, assets: &Path) -> Result<()> {
    let result = mdexport::export_document_with_data(
        "pdf",
        CONTENT,
        root.join("documents").to_str().unwrap(),
        root.join("Recovered.pdf").to_str().unwrap(),
        &json!({"pdf":{"engine":"rust"}}),
        "研究说明.md",
        assets,
        &root.join("profile"),
    )?;
    assert!(result.ok);
    assert!(!lopdf::Document::load(root.join("Recovered.pdf"))?
        .get_pages()
        .is_empty());
    Ok(())
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn previous_owner(
    binary: &Path,
    mode: &str,
    root: &Path,
    assets: &Path,
    pack: &Path,
) -> Result<OwnedChild> {
    let mut command = Command::new(binary);
    command
        .arg("--child")
        .arg(mode)
        .arg(root)
        .arg(assets)
        .arg(pack)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(OwnedChild(command.spawn()?))
}
fn stream_pack(original: &Path, output: &Path) -> Result<(Value, Value)> {
    let pack: Value = serde_json::from_slice(&fs::read(original)?)?;
    let old = pack["manifest"].clone();
    assert_eq!(old["schema"], 1);
    assert_eq!(old["id"], "fonts");
    let mut current = old.clone();
    current["schema"] = json!(2);
    for file in current["files"]
        .as_array_mut()
        .ok_or("Missing locked files")?
    {
        let name = file["path"].as_str().unwrap();
        let payload = &pack["payload"][name];
        assert_eq!(payload["compression"], "gzip");
        let bytes = STANDARD.decode(payload["data"].as_str().ok_or("Missing reviewed payload")?)?;
        file["compressedSize"] = json!(bytes.len());
    }
    let header = serde_json::to_vec(&current)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(capability_archive::MAGIC)?;
    file.write_all(&(header.len() as u32).to_le_bytes())?;
    file.write_all(&header)?;
    for entry in current["files"].as_array().unwrap() {
        let bytes = STANDARD.decode(
            pack["payload"][entry["path"].as_str().unwrap()]["data"]
                .as_str()
                .unwrap(),
        )?;
        file.write_all(&bytes)?;
    }
    file.sync_all()?;
    Ok((old, current))
}
fn assets(root: &Path, old: &Value, current: &Value, pack: &Path) -> Result<PathBuf> {
    let assets = root.join("current-assets");
    let caps = assets.join("capabilities");
    fs::create_dir_all(caps.join("history"))?;
    let fingerprint = hash(serde_json::to_vec(old)?);
    fs::write(
        caps.join(format!(
            "fonts-{}-universal.lock.json",
            current["version"].as_str().unwrap()
        )),
        serde_json::to_vec(current)?,
    )?;
    fs::write(
        caps.join(format!("history/{fingerprint}.lock.json")),
        serde_json::to_vec(old)?,
    )?;
    fs::write(
        caps.join("recovery-locks.json"),
        serde_json::to_vec(
            &json!({"schema":1,"manifests":[{"id":"fonts","platform":"universal","fingerprint":fingerprint}]}),
        )?,
    )?;
    // A schema-2 installation must retain the production catalog/digest gate.
    // This local QA encoding is not published or substituted into real assets.
    let installed_bytes = current["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["size"].as_u64().unwrap())
        .sum::<u64>();
    fs::write(
        caps.join("catalog.json"),
        serde_json::to_vec(
            &json!({"schema":1,"repository":"Natsummerance/tool-ReadMD","release":"owned-history-qa","qaOnly":true,"packs":[{
                "id":"fonts","version":current["version"],"platform":"universal","format":"stream-v2",
                "downloadBytes":fs::metadata(pack)?.len(),"installedBytes":installed_bytes,
                "sha256":hash(fs::read(pack)?),"manifestSha256":hash(serde_json::to_vec(current)?),
                "url":"https://github.com/Natsummerance/tool-ReadMD/releases/download/owned-history-qa/readmd-fonts-2026.10.10-universal.readmd-pack",
                "license":"OFL-1.1; genuine unchanged fonts; QA encoding only"
            }]}),
        )?,
    )?;
    Ok(assets)
}
fn capture_records(case: &Path, parent: &Path) -> Result<Vec<Value>> {
    let mut records = Vec::new();
    let capture = case.join("before-crash");
    fs::create_dir(&capture)?;
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        for name in ["removal-v1.json", "replacement-v1.json", "staging-v1.json"] {
            let source = entry.path().join(name);
            if !source.is_file() {
                continue;
            }
            let bytes = fs::read(&source)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            let destination = capture.join(entry.file_name());
            fs::create_dir_all(&destination)?;
            fs::write(destination.join(name), &bytes)?;
            records.push(json!({"kind":name,"manifest":value["manifest"],"phase":value["phase"],"sha256":hash(bytes)}));
        }
    }
    Ok(records)
}
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("Requires assets, genuine v1 fonts, previous transaction probe and owned evidence parent".into());
    }
    let source_assets = PathBuf::from(&args[0]).canonicalize()?;
    let original_pack = PathBuf::from(&args[1]).canonicalize()?;
    let previous = PathBuf::from(&args[2]).canonicalize()?;
    let parent = PathBuf::from(&args[3]).canonicalize()?;
    let root = parent.join(format!(
        "capability-history-native-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&root)?;
    let current_pack = root.join("CurrentFonts.readmd-pack");
    let (old, current) = stream_pack(&original_pack, &current_pack)?;
    let old_fingerprint = hash(serde_json::to_vec(&old)?);
    let current_fingerprint = hash(serde_json::to_vec(&current)?);
    assert_ne!(old_fingerprint, current_fingerprint);
    let genuine_lock: Value = serde_json::from_slice(&fs::read(source_assets.join(format!(
        "capabilities/fonts-{}-universal.lock.json",
        old["version"].as_str().unwrap()
    )))?)?;
    assert_eq!(old, genuine_lock);
    let mut checks = Vec::new();
    for mode in [
        "remove-first",
        "remove-commit",
        "replace-previous",
        "replace-candidate",
        "replace-commit",
        "replace-prepared",
        "stage-control",
        "stage-planned",
        "stage-root",
        "stage-owned",
        "stage-payload",
        "stage-validation",
        "stage-probed",
        "unindexed-history",
        "tampered-history",
    ] {
        let case = root.join(mode);
        fs::create_dir(&case)?;
        fs::create_dir(case.join("documents"))?;
        let current_assets = assets(&case, &old, &current, &current_pack)?;
        let initial = boot(&case, &source_assets);
        let installed = api(&initial, "install", &original_pack);
        assert_eq!(installed["ok"], true, "{installed}");
        drop(initial);
        let runtime = case
            .join("profile/capabilities/managed-v2/fonts")
            .join(old["version"].as_str().unwrap());
        let managed = runtime.parent().unwrap();
        fs::write(
            runtime.join("Preserve-user.txt"),
            b"Keep genuine upgrade originals",
        )?;
        let first = old["files"][0]["path"].as_str().unwrap();
        let valid = old["files"][0]["sha256"].as_str().unwrap();
        if mode.starts_with("replace-") {
            fs::write(
                runtime.join(first),
                b"Damaged prior version for real repair",
            )?;
        }
        let previous_mode = if matches!(mode, "unindexed-history" | "tampered-history") {
            "remove-first"
        } else {
            mode
        };
        let mut child = previous_owner(
            &previous,
            previous_mode,
            &case,
            &source_assets,
            &original_pack,
        )?;
        let until = Instant::now() + Duration::from_secs(60);
        while !case.join("boundary.ready").is_file() {
            if let Some(status) = child.0.try_wait()? {
                return Err(format!("{mode}: previous owner exited: {status}").into());
            }
            if Instant::now() >= until {
                return Err(format!("{mode}: owner boundary timeout").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let records = capture_records(&case, managed)?;
        assert!(records.iter().all(|r| r["manifest"] == old_fingerprint));
        let observer = boot(&case, &current_assets);
        let busy = api(&observer, "status", &current_pack);
        assert_eq!(busy["busy"], true, "{busy}");
        drop(observer);
        child.0.kill()?;
        assert!(!child.0.wait()?.success());
        drop(child);
        if matches!(mode, "unindexed-history" | "tampered-history") {
            let catalog = current_assets.join("capabilities/recovery-locks.json");
            let original_catalog = fs::read(&catalog)?;
            let history =
                current_assets.join(format!("capabilities/history/{old_fingerprint}.lock.json"));
            let original_history = fs::read(&history)?;
            if mode == "unindexed-history" {
                fs::remove_file(&catalog)?;
            } else {
                fs::write(&history, b"{\"schema\":1,\"files\":[\"private-file\"]}")?;
            }
            assert!(export(&case, &current_assets).is_err());
            let app = boot(&case, &current_assets);
            assert_eq!(api(&app, "status", &current_pack)["ok"], false);
            drop(app);
            for entry in fs::read_dir(case.join("before-crash"))? {
                let entry = entry?;
                for record in fs::read_dir(entry.path())? {
                    let record = record?;
                    assert_eq!(
                        fs::read(record.path())?,
                        fs::read(managed.join(entry.file_name()).join(record.file_name()))?,
                        "Rejected history changed a journal"
                    );
                }
            }
            let journals = fs::read_dir(managed)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().join("removal-v1.json").is_file())
                .count();
            assert_eq!(journals, 1);
            fs::write(&catalog, original_catalog)?;
            fs::write(&history, original_history)?;
        }
        let first_export = export(&case, &current_assets);
        let app = boot(&case, &current_assets);
        if matches!(
            mode,
            "remove-commit" | "replace-previous" | "replace-prepared"
        ) {
            assert!(first_export.is_err());
            assert_eq!(api(&app, "status", &current_pack)["installed"], false);
            let repaired = api(&app, "install", &current_pack);
            assert_eq!(repaired["ok"], true, "{repaired}");
        } else {
            first_export?;
        }
        export(&case, &current_assets)?;
        assert_eq!(api(&app, "status", &current_pack)["ready"], true);
        assert_eq!(hash(fs::read(runtime.join(first))?), valid);
        let mut sentinels = Vec::new();
        for entry in fs::read_dir(managed)? {
            let entry = entry?;
            let file = entry.path().join("Preserve-user.txt");
            if file.is_file() {
                sentinels.push(file);
            }
        }
        assert_eq!(sentinels.len(), 1);
        assert_eq!(fs::read(&sentinels[0])?, b"Keep genuine upgrade originals");
        for entry in fs::read_dir(managed)? {
            let entry = entry?;
            for name in ["removal-v1.json", "replacement-v1.json", "staging-v1.json"] {
                assert!(
                    !entry.path().join(name).exists(),
                    "{mode}: known old journal not retired"
                );
            }
        }
        assert_eq!(api(&app, "uninstall", &current_pack)["ok"], true);
        drop(app);
        checks.push(json!({"mode":mode,"passed":true,"previousBoundary":fs::read_to_string(case.join("boundary.ready"))?,"oldRecords":records,"minimumPdf":format!("{mode}/Recovered.pdf")}));
    }
    let report = json!({"passed":true,"checks":checks,"platform":std::env::consts::OS,"oldManifestFingerprint":old_fingerprint,"currentManifestFingerprint":current_fingerprint,"previousProbeSha256":hash(fs::read(previous)?),"probeSha256":hash(fs::read(std::env::current_exe()?)?),"v1PackSha256":hash(fs::read(original_pack)?),"v2PackSha256":hash(fs::read(&current_pack)?),"scope":"Actual previous Rust service journals at thirteen crash boundaries, genuine schema1-to-schema2 font package upgrade, two actual rejection/restoration cases, first PDF export and normal cleanup; not all historical releases, engines, platforms or power-loss certification."});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", json!({"root":root,"passed":true,"checks":15}));
    Ok(())
}
