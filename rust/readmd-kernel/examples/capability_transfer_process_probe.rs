//! Real isolated capability API transfers in independent owner processes.
//! Declarations test reservation limits; this does not send a 2 GiB payload.
use readmd_kernel::{capability_delivery, capability_packs, mdexport, paths, server::Request, App};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const HALF: u64 = 1024 * 1024 * 1024;
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn boot(root: &Path, assets: &Path) -> Result<Arc<App>> {
    Ok(Arc::new(App::bootstrap(paths::AppPaths::with_dirs(
        &root.join("profile"),
        &root.join("documents"),
        assets,
    ))?))
}
fn api(app: &Arc<App>, body: Value) -> Value {
    match capability_delivery::endpoint(app, &body) {
        Ok(response) => serde_json::from_slice(&response.body).unwrap(),
        Err(error) => json!({"ok":false,"status":error.status,"error_code":error.code}),
    }
}
fn start(app: &Arc<App>, total: u64) -> Value {
    api(
        app,
        json!({"action":"upload_start","id":"fonts","format":"stream-v2","total":total,"request_key":uuid::Uuid::new_v4().to_string()}),
    )
}
fn holders(root: &Path) -> usize {
    fs::read_dir(root.join("profile/capabilities/managed-v2/transfers"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("upload-"))
        .count()
}
fn rejected(app: &Arc<App>, body: Value) -> Result<u128> {
    let start = Instant::now();
    let response = api(app, body);
    let ms = start.elapsed().as_millis();
    if response["error_code"] != "capability_install_busy" || response["status"] != 409 || ms >= 200
    {
        return Err(format!(
            "Expected bounded transfer rejection, elapsed={ms}ms, error={}, phase={}",
            response["error_code"], response["phase"]
        )
        .into());
    }
    Ok(ms)
}
fn launch(root: &Path, assets: &Path, name: &str, total: u64) -> Result<OwnedChild> {
    let log = fs::File::create(root.join(format!("{name}.log")))?;
    let mut child = OwnedChild(
        Command::new(std::env::current_exe()?)
            .arg("--holder")
            .arg(root)
            .arg(assets)
            .arg(name)
            .arg(total.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    );
    let until = Instant::now() + Duration::from_secs(30);
    while !root.join(format!("{name}.ready")).is_file() {
        if let Some(status) = child.0.try_wait()? {
            return Err(format!("Transfer owner exited before ready: {status}").into());
        }
        if Instant::now() >= until {
            return Err("Transfer owner readiness deadline".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(child)
}
fn stop(root: &Path, name: &str, child: &mut OwnedChild) -> Result<u128> {
    let start = Instant::now();
    fs::write(root.join(format!("{name}.stop")), b"cancel")?;
    loop {
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                return Err("Transfer owner cancellation failed".into());
            }
            return Ok(start.elapsed().as_millis());
        }
        if start.elapsed() >= Duration::from_secs(2) {
            return Err("Transfer owner cancellation exceeded two seconds".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn terminal(app: &Arc<App>, job: &Value) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        let result = api(app, json!({"action":"download_status","job":job["job"]}));
        match result["phase"].as_str() {
            Some("ready") => return Ok(result),
            Some("error" | "cancelled") => {
                return Err(format!("Real offline installation failed: {result}").into())
            }
            _ => (),
        }
        if Instant::now() >= until {
            return Err("Actual offline installation deadline".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|v| v == "--holder") {
        if args.len() != 5 {
            return Err("Holder requires isolated root, assets, name, total".into());
        }
        let root = PathBuf::from(&args[1]);
        let assets = PathBuf::from(&args[2]);
        let name = args[3].to_str().ok_or("Invalid holder name")?;
        if !matches!(
            name,
            "bytes-one" | "bytes-two" | "count-one" | "count-two" | "count-three" | "count-four"
        ) {
            return Err("Invalid holder name".into());
        }
        let total = args[4].to_str().ok_or("Invalid total")?.parse::<u64>()?;
        let app = boot(&root, &assets)?;
        let job = start(&app, total);
        if job["phase"] != "uploading" {
            return Err(format!("Actual upload_start failed: {}", job["error_code"]).into());
        }
        fs::write(
            root.join(format!("{name}.ready")),
            serde_json::to_vec(&json!({"declaredTotal":total,"actualUploadStarted":true}))?,
        )?;
        let until = Instant::now() + Duration::from_secs(120);
        while !root.join(format!("{name}.stop")).is_file() {
            if Instant::now() >= until {
                return Err("Parent did not release transfer owner".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let cancelled = api(&app, json!({"action":"cancel_download","job":job["job"]}));
        if cancelled["phase"] != "cancelled" {
            return Err("Actual upload cancellation failed".into());
        }
        return Ok(());
    }
    if args.len() != 3 {
        return Err("Requires real assets, reviewed fonts pack, owned evidence parent".into());
    }
    let assets = PathBuf::from(&args[0]).canonicalize()?;
    let pack = PathBuf::from(&args[1]).canonicalize()?;
    let parent = PathBuf::from(&args[2]).canonicalize()?;
    let root = parent.join(format!(
        "capability-transfer-native-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&root)?;
    fs::create_dir(root.join("documents"))?;
    let app = boot(&root, &assets)?;
    let mut checks = Vec::new();
    let mut busy = Vec::new();
    let mut first = launch(&root, &assets, "bytes-one", HALF)?;
    let mut second = launch(&root, &assets, "bytes-two", HALF)?;
    for body in [
        json!({"action":"upload_start","id":"fonts","format":"stream-v2","total":7,"request_key":uuid::Uuid::new_v4().to_string()}),
        json!({"action":"download","id":"typst"}),
        json!({"action":"import","id":"fonts","path":pack}),
    ] {
        busy.push(rejected(&app, body)?);
    }
    if holders(&root) != 2 {
        return Err("Rejected transfers created upload directories".into());
    }
    checks.push(json!({"name":"two-real-upload-owners-fill-two-gib-declaration-budget","passed":true,"actualPayloadBytes":0}));
    first.0.kill()?;
    if first.0.wait()?.success() {
        return Err("Expected abrupt transfer owner exit".into());
    }
    busy.push(rejected(&app,json!({"action":"upload_start","id":"fonts","format":"stream-v2","total":2*HALF,"request_key":uuid::Uuid::new_v4().to_string()}))?);
    checks.push(json!({"name":"crash-releases-only-owner-reservation-other-owner-remains-protected","passed":true}));
    let imported = api(&app, json!({"action":"import","id":"fonts","path":pack}));
    if imported["phase"] != "extracting" {
        return Err(format!("Real import could not resume: {imported}").into());
    }
    terminal(&app, &imported)?;
    let pdf = root.join("Recovered.pdf");
    let output = mdexport::export_document_with_data(
        "pdf",
        "# 证据与推理\n\n真实上传拥有者退出后继续安装并导出。\n\n:math[frac(α + x^2, sqrt(y))]",
        root.join("documents").to_str().unwrap(),
        pdf.to_str().unwrap(),
        &json!({"pdf":{"engine":"rust"}}),
        "Research.md",
        &assets,
        &root.join("profile"),
    )?;
    if !output.ok || lopdf::Document::load(&pdf)?.get_pages().is_empty() {
        return Err("Actual recovered PDF failed".into());
    }
    let reopened = boot(&root, &assets)?;
    let response = capability_packs::endpoint(
        &reopened,
        &Request {
            method: "POST".into(),
            path: "/api/capabilities/packs".into(),
            query: Default::default(),
            headers: Default::default(),
            body: serde_json::to_vec(&json!({"action":"status","id":"fonts"}))?,
        },
    )?;
    let status: Value = serde_json::from_slice(&response.body)?;
    if status["ready"] != true {
        return Err("Installed fonts did not survive service restart".into());
    }
    checks.push(json!({"name":"actual-reviewed-offline-import-pdf-and-reopen-while-other-upload-lives","passed":true}));
    let byte_cancel = stop(&root, "bytes-two", &mut second)?;
    let mut count = Vec::new();
    for name in ["count-one", "count-two", "count-three", "count-four"] {
        count.push((name, launch(&root, &assets, name, 7)?));
    }
    busy.push(rejected(&app,json!({"action":"upload_start","id":"fonts","format":"stream-v2","total":7,"request_key":uuid::Uuid::new_v4().to_string()}))?);
    busy.push(rejected(&app, json!({"action":"download","id":"typst"}))?);
    busy.push(rejected(
        &app,
        json!({"action":"import","id":"fonts","path":pack}),
    )?);
    let raw = Request {
        method: "POST".into(),
        path: "/api/capabilities/upload".into(),
        query: std::collections::HashMap::from([("id".into(), "fonts".into())]),
        headers: Default::default(),
        body: b"payload".to_vec(),
    };
    let failure = capability_delivery::upload(&app, &raw)
        .err()
        .ok_or("Raw upload unexpectedly admitted")?;
    if failure.code != "capability_install_busy" || holders(&root) != 4 {
        return Err("Count budget or raw-upload cleanup failed".into());
    }
    checks.push(json!({"name":"four-independent-active-uploads-block-all-new-transport-routes","passed":true}));
    count[0].1 .0.kill()?;
    let _ = count[0].1 .0.wait()?;
    let admitted = start(&app, 7);
    if admitted["phase"] != "uploading" {
        return Err("Owner crash did not release one count".into());
    }
    let request = Request {
        method: "POST".into(),
        path: "/api/capabilities/upload/chunk".into(),
        query: std::collections::HashMap::from([
            ("job".into(), admitted["job"].as_str().unwrap().into()),
            ("offset".into(), "0".into()),
        ]),
        headers: std::collections::HashMap::from([
            (
                "x-readmd-upload-token".into(),
                admitted["upload_token"].as_str().unwrap().into(),
            ),
            (
                "x-readmd-chunk-sha256".into(),
                format!("{:x}", Sha256::digest(b"payload")),
            ),
        ]),
        body: b"payload".to_vec(),
    };
    for _ in 0..2 {
        let response = capability_delivery::upload_chunk(&app, &request)?;
        let value: Value = serde_json::from_slice(&response.body)?;
        if value["received"] != 7 {
            return Err("Actual chunk/duplicate failed".into());
        }
    }
    let cancelled = api(
        &app,
        json!({"action":"cancel_download","job":admitted["job"]}),
    );
    if cancelled["phase"] != "cancelled" {
        return Err("Parent upload did not cancel".into());
    }
    let after = start(&app, 7);
    if after["phase"] != "uploading" {
        return Err("Cancellation did not release reservation".into());
    }
    api(&app, json!({"action":"cancel_download","job":after["job"]}));
    let mut cancels = vec![byte_cancel];
    for (name, child) in count.iter_mut().skip(1) {
        cancels.push(stop(&root, name, child)?);
    }
    checks.push(json!({"name":"real-chunk-idempotency-cancellation-and-subsequent-upload","passed":true,"ownerCancellationMs":cancels}));
    let report = json!({"passed":true,"platform":std::env::consts::OS,"checks":checks,"busyMs":busy,"compressedDeclarationLimitBytes":2*HALF,"reservationQuantumBytes":64*1024*1024,"driverSha256":format!("{:x}",Sha256::digest(fs::read(std::env::current_exe()?)?)),"packSha256":format!("{:x}",Sha256::digest(fs::read(&pack)?)),"userInstallationReplaced":false,"scope":"Independent OS owners call actual Rust upload/download/import/raw/chunk APIs; reviewed offline installation, PDF and restart. Reservation capacity, not 2 GiB transferred bytes, desktop/MCP transport, old clients, expanded-disk budget or complete B3 certification."});
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        json!({"passed":true,"checks":checks.len(),"report":root.join("report.json")})
    );
    Ok(())
}
