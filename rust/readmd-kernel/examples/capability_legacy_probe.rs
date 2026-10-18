//! Real old storage-protocol application versus current Rust service, using
//! reviewed fonts, actual HTTP installation, private migration and PDF output.
//! The old executable is identified in the report; this is not a claim that
//! every historically released client has been tested.
use readmd_kernel::{capability_packs, mdexport, paths, publication_fonts, server::Request, App};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, OnceLock},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const VERSION: &str = "2026.10.10";
const CONTENT: &str = "# 新旧客户端隔离\n\n保留旧资料，验证中文正文与数学输出。\n\n:math[frac(α + x^2, sqrt(y))] 与 $E=mc^2$。\n";
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
fn export(root: &Path, assets: &Path, name: &str) -> Result<()> {
    let out = root.join(format!("{name}.pdf"));
    let result = mdexport::export_document_with_data(
        "pdf",
        CONTENT,
        root.join("documents").to_str().unwrap(),
        out.to_str().unwrap(),
        &json!({"pdf":{"engine":"rust"}}),
        "隔离验证.md",
        assets,
        &root.join("profile"),
    )?;
    assert!(result.ok);
    let pdf = lopdf::Document::load(out)?;
    assert!(!pdf.get_pages().is_empty());
    Ok(())
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn spawn_hidden(command: &mut Command) -> Result<OwnedChild> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(OwnedChild(command.spawn()?))
}
struct OldClient {
    _process: OwnedChild,
    url: String,
    agent: ureq::Agent,
}
impl OldClient {
    fn start(binary: &Path, root: &Path, assets: &Path) -> Result<Self> {
        let mut command = Command::new(binary);
        command
            .args(["--no-window", "--port", "-1"])
            .arg("--data-dir")
            .arg(root.join("profile"))
            .arg("--workspace")
            .arg(root.join("documents"))
            .arg("--assets")
            .arg(assets)
            .env_remove("READMD_REQUIRE_TOKEN")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut process = spawn_hidden(&mut command)?;
        let stdout = process
            .0
            .stdout
            .take()
            .ok_or("Missing old application stdout")?;
        let (tx, rx) = mpsc::channel();
        // Drain stdout continuously. Do not log boot directories or secrets.
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                if let Some((_, url)) = line.split_once(" listening on ") {
                    let _ = tx.send(url.trim().to_string());
                }
            }
        });
        let url = rx.recv_timeout(Duration::from_secs(30))?;
        if !url.starts_with("http://127.0.0.1:") {
            return Err("Nonlocal old application address".into());
        }
        Ok(Self {
            _process: process,
            url,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(120))
                .build(),
        })
    }
    fn api(&self, action: &str, pack: &Path) -> Result<Value> {
        let response = self
            .agent
            .post(&format!("{}/api/capabilities/packs", self.url))
            .set("Origin", &self.url)
            .send_json(json!({"action":action,"id":"fonts","path":pack}));
        match response {
            Ok(response) => Ok(response.into_json()?),
            Err(ureq::Error::Status(code, response)) => {
                let mut value: Value = response.into_json()?;
                value["status"] = json!(code);
                Ok(value)
            }
            Err(error) => Err(error.into()),
        }
    }
}
fn runtime(root: &Path, old: bool) -> PathBuf {
    let base = root.join("profile/capabilities");
    (if old { base } else { base.join("managed-v2") })
        .join("fonts")
        .join(VERSION)
}
fn identities(root: &Path, lock: &Value) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for f in lock["files"].as_array().ok_or("Missing locked files")? {
        let name = f["path"].as_str().ok_or("Missing locked path")?;
        let bytes = fs::read(root.join(name))?;
        assert_eq!(bytes.len() as u64, f["size"].as_u64().unwrap());
        assert_eq!(hash(&bytes), f["sha256"].as_str().unwrap());
        result.insert(name.into(), json!(hash(&bytes)));
    }
    Ok(Value::Object(result))
}
fn seed(old: &OldClient, root: &Path, pack: &Path, lock: &Value) -> Result<Value> {
    let installed = old.api("install", pack)?;
    assert_eq!(installed["ok"], true, "{installed}");
    assert!(runtime(root, true).is_dir());
    assert!(
        !runtime(root, false).exists(),
        "Fixture executable must use old storage protocol"
    );
    fs::write(
        runtime(root, true).join("Preserve-original.txt"),
        b"old user extra",
    )?;
    fs::write(
        runtime(root, true)
            .parent()
            .unwrap()
            .join("removal-unknown-v0.json"),
        b"unknown old journal",
    )?;
    identities(&runtime(root, true), lock)
}
fn assert_originals(root: &Path, lock: &Value, expected: &Value) -> Result<()> {
    assert_eq!(&identities(&runtime(root, true), lock)?, expected);
    // The real old application may repair its own tree, retaining extras in
    // its own replacement backup. Migration must preserve them; it cannot
    // dictate where a non-cooperating legacy writer keeps its own originals.
    let mut originals = Vec::new();
    for entry in fs::read_dir(runtime(root, true).parent().unwrap())? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path().join("Preserve-original.txt");
            if path.is_file() {
                originals.push(path);
            }
        }
    }
    assert_eq!(
        originals.len(),
        1,
        "Legacy user extra must survive exactly once"
    );
    assert_eq!(fs::read(&originals[0])?, b"old user extra");
    assert_eq!(
        fs::read(
            runtime(root, true)
                .parent()
                .unwrap()
                .join("removal-unknown-v0.json")
        )?,
        b"unknown old journal"
    );
    assert!(!runtime(root, false).join("Preserve-original.txt").exists());
    Ok(())
}
fn boundary(mode: &str) -> &'static str {
    match mode {
        "legacy-copy" => "legacy migration first file copied",
        "legacy-probed" => "legacy migration probed",
        "legacy-prepared" => "replacement prepared",
        "legacy-candidate" => "replacement candidate moved",
        "legacy-commit" => "replacement committed",
        _ => panic!("Unknown migration crash boundary"),
    }
}
static SCHEDULE: OnceLock<(PathBuf, String)> = OnceLock::new();
struct Barrier;
impl log::Log for Barrier {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target() == "readmd::capability::transaction"
    }
    fn log(&self, record: &log::Record) {
        let (root, mode) = SCHEDULE.get().unwrap();
        if !self.enabled(record.metadata()) || record.args().to_string() != boundary(mode) {
            return;
        }
        fs::write(root.join("boundary.ready"), boundary(mode)).unwrap();
        let until = Instant::now() + Duration::from_secs(30);
        while !root.join("boundary.release").exists() {
            assert!(
                Instant::now() < until,
                "Migration owner was not interrupted"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn flush(&self) {}
}
static BARRIER: Barrier = Barrier;
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "--child") {
        if args.len() != 5 {
            return Err("Child requires boundary, owned profile, assets and pack".into());
        }
        let root = Path::new(&args[2]);
        SCHEDULE
            .set((root.into(), args[1].to_str().unwrap().into()))
            .unwrap();
        log::set_logger(&BARRIER).map_err(|_| "Migration logger setup failed")?;
        log::set_max_level(log::LevelFilter::Debug);
        let result = api(
            &boot(root, Path::new(&args[3])),
            "status",
            Path::new(&args[4]),
        );
        return Err(format!("Owner returned before interruption: {result}").into());
    }
    if args.len() != 4 {
        return Err(
            "Requires assets, reviewed fonts, old protocol executable and owned evidence parent"
                .into(),
        );
    }
    let assets = PathBuf::from(&args[0]).canonicalize()?;
    let pack = PathBuf::from(&args[1]).canonicalize()?;
    let old_binary = PathBuf::from(&args[2]).canonicalize()?;
    let parent = PathBuf::from(&args[3]).canonicalize()?;
    let lock: Value = serde_json::from_slice(&fs::read(
        assets.join(format!("capabilities/fonts-{VERSION}-universal.lock.json")),
    )?)?;
    assert_eq!(lock["id"], "fonts");
    let root = parent.join(format!("capability-legacy-native-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)?;
    let mut checks = Vec::new();
    let case = root.join("coexistence");
    fs::create_dir(&case)?;
    fs::create_dir(case.join("documents"))?;
    let old = OldClient::start(&old_binary, &case, &assets)?;
    let originals = seed(&old, &case, &pack, &lock)?;
    // First real export, rather than a status route, initiates migration.
    export(&case, &assets, "First-use")?;
    let app = boot(&case, &assets);
    assert_eq!(api(&app, "status", &pack)["ready"], true);
    assert_eq!(identities(&runtime(&case, false), &lock)?, originals);
    assert_originals(&case, &lock, &originals)?;
    let known = lock["files"][0]["path"].as_str().unwrap();
    fs::write(
        runtime(&case, true).join(known),
        b"old client changed its copy",
    )?;
    assert_eq!(identities(&runtime(&case, false), &lock)?, originals);
    export(&case, &assets, "After-legacy-write")?;
    assert_eq!(old.api("install", &pack)?["ok"], true);
    assert_originals(&case, &lock, &originals)?;
    checks.push(json!({"name":"real-old-client-install-private-first-use-migration-no-hardlinks","passed":true}));
    {
        let _current_reader = publication_fonts::scope(&case.join("profile"), &assets);
        assert!(publication_fonts::current().is_some());
        assert_eq!(api(&app, "uninstall", &pack)["status"], 409);
        assert_eq!(old.api("uninstall", &pack)?["ok"], true);
        assert_eq!(identities(&runtime(&case, false), &lock)?, originals);
        export(&case, &assets, "During-current-lease")?;
    }
    checks.push(
        json!({"name":"old-uninstall-does-not-damage-current-read-lease-or-PDF","passed":true}),
    );
    assert_eq!(old.api("install", &pack)?["ok"], true);
    assert_eq!(api(&app, "uninstall", &pack)["ok"], true);
    assert!(!runtime(&case, false).exists());
    assert_eq!(old.api("status", &pack)?["ready"], true);
    drop(app);
    let app = boot(&case, &assets);
    assert_eq!(api(&app, "status", &pack)["installed"], false);
    assert!(!runtime(&case, false).exists());
    assert_originals(&case, &lock, &originals)?;
    assert_eq!(api(&app, "install", &pack)["ok"], true);
    export(&case, &assets, "Explicit-reinstall")?;
    assert_eq!(api(&app, "uninstall", &pack)["ok"], true);
    assert_eq!(old.api("uninstall", &pack)?["ok"], true);
    drop(old);
    drop(app);
    checks.push(json!({"name":"uninstall-opt-out-survives-restart-explicit-reinstall-remains-available","passed":true}));
    let case = root.join("damaged");
    fs::create_dir(&case)?;
    fs::create_dir(case.join("documents"))?;
    let old = OldClient::start(&old_binary, &case, &assets)?;
    seed(&old, &case, &pack, &lock)?;
    fs::write(runtime(&case, true).join(known), b"corrupt old runtime")?;
    let app = boot(&case, &assets);
    let missing = api(&app, "status", &pack);
    assert_eq!(missing["ok"], true, "{missing}");
    assert_eq!(missing["installed"], false);
    assert_eq!(missing["legacy_migration_error"], "capability_pack_invalid");
    assert!(!runtime(&case, false).exists());
    assert_eq!(api(&app, "install", &pack)["ok"], true);
    export(&case, &assets, "Manual-repair")?;
    assert_eq!(
        fs::read(runtime(&case, true).join(known))?,
        b"corrupt old runtime"
    );
    assert_eq!(api(&app, "uninstall", &pack)["ok"], true);
    assert_eq!(old.api("uninstall", &pack)?["ok"], true);
    drop(old);
    drop(app);
    checks.push(json!({"name":"damaged-legacy-keeps-real-offline-install-flow-and-original-bytes","passed":true}));
    for mode in [
        "legacy-copy",
        "legacy-probed",
        "legacy-prepared",
        "legacy-candidate",
        "legacy-commit",
    ] {
        let case = root.join(mode);
        fs::create_dir(&case)?;
        fs::create_dir(case.join("documents"))?;
        let old = OldClient::start(&old_binary, &case, &assets)?;
        let originals = seed(&old, &case, &pack, &lock)?;
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
        let mut owner = spawn_hidden(&mut command)?;
        let until = Instant::now() + Duration::from_secs(60);
        while !case.join("boundary.ready").is_file() {
            if let Some(status) = owner.0.try_wait()? {
                return Err(format!("{mode}: owner exited before boundary: {status}").into());
            }
            if Instant::now() >= until {
                return Err(format!("{mode}: boundary timeout").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let observer = boot(&case, &assets);
        assert_eq!(api(&observer, "status", &pack)["busy"], true);
        assert_eq!(old.api("status", &pack)?["ready"], true);
        assert_originals(&case, &lock, &originals)?;
        owner.0.kill()?;
        assert!(!owner.0.wait()?.success());
        drop(owner);
        export(&case, &assets, "Recovered")?;
        assert_eq!(api(&observer, "status", &pack)["ready"], true);
        assert_eq!(identities(&runtime(&case, false), &lock)?, originals);
        assert_originals(&case, &lock, &originals)?;
        assert_eq!(api(&observer, "uninstall", &pack)["ok"], true);
        assert_eq!(old.api("uninstall", &pack)?["ok"], true);
        drop(observer);
        drop(old);
        checks.push(json!({"name":mode,"boundary":boundary(mode),"passed":true}));
    }
    let report = json!({"passed":true,"platform":std::env::consts::OS,
        "oldExecutableSha256":hash(fs::read(old_binary)?),
        "probeExecutableSha256":hash(fs::read(std::env::current_exe()?)?),
        "fontPackSha256":hash(fs::read(pack)?),"fontManifestSha256":hash(serde_json::to_vec(&lock)?),
        "oldScope":"previous local dev snapshot, old storage protocol; not published V0.0.4",
        "checks":checks,"cleanup":"Both clients used normal uninstall APIs; user sentinels and PDFs retained"});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        json!({"root":root,"passed":true,"checks":report["checks"].as_array().unwrap().len()})
    );
    Ok(())
}
