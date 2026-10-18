//! Explicit crash scheduling at production commit boundaries using a reviewed
//! font pack, actual pack APIs, fresh owners and real PDF output. No fake engine.
use readmd_kernel::{capability_packs, mdexport, paths, server::Request, App};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, OnceLock},
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
        Err(error) => json!({"ok":false,"status":error.status,"error_code":error.code}),
    }
}
fn export(root: &Path, assets: &Path) -> std::result::Result<mdexport::ExportResult, String> {
    mdexport::export_document_with_data(
        "pdf",
        CONTENT,
        root.join("documents").to_str().unwrap(),
        root.join("Recovered.pdf").to_str().unwrap(),
        &json!({"pdf":{"engine":"rust"}}),
        "研究说明.md",
        assets,
        &root.join("profile"),
    )
}
fn boundary(mode: &str) -> &'static str {
    match mode {
        "remove-first" => "removal first file moved",
        "remove-commit" => "removal committed",
        "replace-previous" => "replacement previous moved",
        "replace-candidate" => "replacement candidate moved",
        "replace-commit" => "replacement committed",
        "replace-prepared" => "replacement prepared",
        "stage-control" => "staging control created",
        "stage-planned" => "staging planned",
        "stage-root" => "staging root created",
        "stage-owned" => "staging ownership recorded",
        "stage-payload" => "staging payload written",
        "stage-validation" => "staging validation",
        "stage-probed" => "staging probed",
        _ => panic!("Unknown crash mode"),
    }
}
static SCHEDULE: OnceLock<(PathBuf, String)> = OnceLock::new();
struct Barrier;
impl log::Log for Barrier {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target() == "readmd::capability::transaction"
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let (root, mode) = SCHEDULE.get().unwrap();
        if record.args().to_string() != boundary(mode) {
            return;
        }
        fs::write(root.join("boundary.ready"), boundary(mode)).unwrap();
        let until = Instant::now() + Duration::from_secs(30);
        while !root.join("boundary.release").exists() {
            assert!(
                Instant::now() < until,
                "Crash controller did not stop owner"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn flush(&self) {}
}
static BARRIER: Barrier = Barrier;
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "--child") {
        if args.len() != 5 {
            return Err("Child requires mode, owned profile, assets and reviewed pack".into());
        }
        let (mode, root, assets, pack) = (
            args[1].to_str().unwrap(),
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        );
        boundary(mode);
        SCHEDULE.set((root.to_path_buf(), mode.into())).unwrap();
        log::set_logger(&BARRIER).map_err(|_| "Logger setup failed")?;
        log::set_max_level(log::LevelFilter::Debug);
        let app = boot(root, assets);
        let result = api(
            &app,
            if mode.starts_with("remove-") {
                "uninstall"
            } else {
                "install"
            },
            pack,
        );
        return Err(format!("Owner returned before controller interruption: {result}").into());
    }
    if args.len() != 3 {
        return Err(
            "Requires application assets, reviewed font pack and owned evidence parent".into(),
        );
    }
    let (assets, pack, parent) = (
        PathBuf::from(&args[0]).canonicalize()?,
        PathBuf::from(&args[1]).canonicalize()?,
        PathBuf::from(&args[2]).canonicalize()?,
    );
    let lock: Value = serde_json::from_slice(&fs::read(
        assets.join("capabilities/fonts-2026.10.10-universal.lock.json"),
    )?)?;
    if lock["id"] != "fonts" || lock["version"] != "2026.10.10" {
        return Err("Reviewed application font lock required".into());
    }
    let root = parent.join(format!(
        "capability-transaction-native-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&root)?;
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
    ] {
        let case = root.join(mode);
        fs::create_dir(&case)?;
        fs::create_dir(case.join("documents"))?;
        let app = boot(&case, &assets);
        let installed = api(&app, "install", &pack);
        assert_eq!(installed["ok"], true, "{installed}");
        let runtime = case.join("profile/capabilities/managed-v2/fonts/2026.10.10");
        let managed = runtime.parent().unwrap();
        let unmarked = managed.join("install-unmarked-keep");
        fs::create_dir(&unmarked)?;
        fs::write(
            unmarked.join("Preserve-unmarked.txt"),
            b"Never infer ownership from a prefix",
        )?;
        let known = runtime.join(
            lock["files"][0]["path"]
                .as_str()
                .ok_or("First locked font resource missing")?,
        );
        let valid_hash = hash(fs::read(&known)?);
        fs::write(
            runtime.join("Preserve-user.txt"),
            b"Keep synthetic user data",
        )?;
        if mode.starts_with("replace-") {
            fs::write(&known, b"Damaged old resource for actual repair")?;
        }
        drop(app);
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--child")
            .arg(mode)
            .arg(&case)
            .arg(&assets)
            .arg(&pack)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = OwnedChild(command.spawn()?);
        let until = Instant::now() + Duration::from_secs(60);
        while !case.join("boundary.ready").is_file() {
            if let Some(status) = child.0.try_wait()? {
                return Err(format!("{mode}: owner exited before boundary: {status}").into());
            }
            if Instant::now() >= until {
                return Err(format!("{mode}: boundary deadline").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut staged = Vec::new();
        for entry in fs::read_dir(managed)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let control = entry.path().join("staging-v1.json");
            if !control.is_file() {
                continue;
            }
            let record: Value = serde_json::from_slice(&fs::read(control)?)?;
            let payload = managed.join(
                record["staging"]
                    .as_str()
                    .ok_or("Missing staging identity")?,
            );
            if payload.is_dir() {
                fs::write(
                    payload.join("Preserve-stage.txt"),
                    b"Keep unknown stage data",
                )?;
                staged.push((payload, record["phase"].clone()));
            }
        }
        let observing = boot(&case, &assets);
        let busy = api(&observing, "status", &pack);
        assert_eq!(busy["busy"], true, "{busy}");
        drop(observing);
        child.0.kill()?;
        assert!(!child.0.wait()?.success());
        drop(child);
        // First actual export drives lazy recovery; this is not a readiness-only test.
        let first = export(&case, &assets);
        let app = boot(&case, &assets);
        let status = api(&app, "status", &pack);
        if mode == "remove-commit" {
            assert!(first.is_err());
            assert_eq!(status["installed"], false, "{status}");
            assert_eq!(
                fs::read(runtime.join("Preserve-user.txt"))?,
                b"Keep synthetic user data"
            );
            assert_eq!(api(&app, "install", &pack)["ok"], true);
        } else if matches!(mode, "replace-previous" | "replace-prepared") {
            assert!(first.is_err());
            assert_eq!(status["ready"], false, "{status}");
            assert_eq!(fs::read(&known)?, b"Damaged old resource for actual repair");
            assert_eq!(
                fs::read(runtime.join("Preserve-user.txt"))?,
                b"Keep synthetic user data"
            );
            assert_eq!(api(&app, "install", &pack)["ok"], true);
        } else {
            assert!(first?.ok);
            assert_eq!(status["ready"], true, "{status}");
        }
        assert_eq!(hash(fs::read(&known)?), valid_hash);
        assert!(export(&case, &assets)?.ok);
        let bytes = fs::read(case.join("Recovered.pdf"))?;
        assert!(!lopdf::Document::load_mem(&bytes)?.get_pages().is_empty());
        let managed = case.join("profile/capabilities/managed-v2/fonts");
        let sentinels = fs::read_dir(&managed)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path().join("Preserve-user.txt"))
            .filter(|p| p.is_file())
            .collect::<Vec<_>>();
        assert_eq!(
            sentinels.len(),
            1,
            "Unknown original must survive exactly once: {mode}"
        );
        assert_eq!(fs::read(&sentinels[0])?, b"Keep synthetic user data");
        assert_eq!(
            fs::read(unmarked.join("Preserve-unmarked.txt"))?,
            b"Never infer ownership from a prefix"
        );
        for (payload, phase) in &staged {
            // A successful candidate move carries an unexpected file forward;
            // otherwise it remains in its original staging directory.
            let location = if payload.exists() {
                payload.as_path()
            } else {
                runtime.as_path()
            };
            assert_eq!(
                fs::read(location.join("Preserve-stage.txt"))?,
                b"Keep unknown stage data"
            );
            if phase == "owned" && payload.exists() {
                for file in lock["files"].as_array().ok_or("Missing locked files")? {
                    assert!(
                        !payload.join(file["path"].as_str().unwrap()).exists(),
                        "Owned payload was not reaped: {mode}"
                    );
                }
            }
        }
        assert!(!fs::read_dir(&managed)?.filter_map(|e| e.ok()).any(|e| e
            .path()
            .join("removal-v1.json")
            .exists()
            || e.path().join("replacement-v1.json").exists()
            || e.path().join("staging-v1.json").exists()));
        // Leave compact evidence, not thirteen duplicate 50 MiB font trees.
        // Exercise the normal installer API in this new QA profile only.
        assert_eq!(api(&app, "uninstall", &pack)["ok"], true);
        assert_eq!(api(&app, "status", &pack)["installed"], false);
        for file in lock["files"].as_array().unwrap() {
            assert!(!runtime.join(file["path"].as_str().unwrap()).exists());
        }
        assert_eq!(fs::read(&sentinels[0])?, b"Keep synthetic user data");
        assert_eq!(
            fs::read(unmarked.join("Preserve-unmarked.txt"))?,
            b"Never infer ownership from a prefix"
        );
        checks.push(json!({"mode":mode,"passed":true,"boundary":boundary(mode),"firstUseRecovery":true,
            "minimumPdf":format!("{mode}/Recovered.pdf"),"pdfSha256":hash(&bytes),"unknownDataPreserved":true,
            "unmarkedPreserved":true,"stagingRecordsRecovered":true,"observedStagingRoots":staged.len(),
            "qaRuntimeUninstalled":true}));
    }
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"checks":checks,
            "platform":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),"realReviewedFontPack":true,
            "driverSha256":hash(fs::read(std::env::current_exe()?)?),"packSha256":hash(fs::read(&pack)?),
            "lockSha256":hash(fs::read(assets.join("capabilities/fonts-2026.10.10-universal.lock.json"))?),
            "userInstallationReplaced":false,"explicitCrashScheduling":true,
            "scope":"Actual pack APIs, reviewed fonts, abrupt owner exit, first-use recovery, restart and core PDF. Not desktop transport, other engines, old clients, power loss, malicious journal authors or native Unix race certification."
        }))?,
    )?;
    println!(
        "{}",
        json!({"passed":true,"checks":checks.len(),"report":root.join("report.json")})
    );
    Ok(())
}
