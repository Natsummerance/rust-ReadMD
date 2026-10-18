//! Genuine engines and the shared desktop/MCP service on owned long paths.
//! Explicit offline inputs only; no system engines or user files.
#[cfg(windows)]
mod native {
    use readmd_kernel::{paths, server, App};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        os::windows::{ffi::OsStrExt, process::CommandExt},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::Arc,
        time::{Duration, Instant},
    };
    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    const CONTENT:&str="# 深层资料中的研究记录\n\n保留原始证据，比较解释与结果。\n\n```readmd-math v1\nfrac(α+x^2,sqrt(y))\n```\n\n| 记录 | 核对 |\n|---|---|\n| 观察 | 保留原文 |\n| 解释 | 检查前提 |\n";
    fn hash(path: &Path) -> Result<String> {
        Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
    }
    fn boot(data: &Path, assets: &Path, docs: &Path) -> Result<Arc<App>> {
        Ok(Arc::new(App::bootstrap(paths::AppPaths::with_dirs(
            data, docs, assets,
        ))?))
    }
    fn call(app: &Arc<App>, method: &str, target: &str, body: Option<&Value>) -> Result<Value> {
        let (status, value) = server::call_in_process(app, method, target, body);
        if status != 200 || value["ok"] == false {
            return Err(format!("Actual service rejected {target}: {status} {value}").into());
        }
        Ok(value)
    }
    fn pack(app: &Arc<App>, action: &str, id: &str, location: Option<&Path>) -> Result<Value> {
        call(
            app,
            "POST",
            "/api/capabilities/packs",
            Some(&json!({"action":action,"id":id,"path":location})),
        )
    }
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn child(data: &Path, assets: &Path, docs: &Path, name: &str) -> Result<OwnedChild> {
        let log = fs::File::create(docs.join(format!("{name}.log")))?;
        let system = PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot required")?)
            .join("System32");
        Ok(OwnedChild(
            Command::new(std::env::current_exe()?)
                .arg("--validate")
                .arg(data)
                .arg(assets)
                .arg(docs)
                .arg(name)
                .env("PATH", system)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .creation_flags(0x08000000)
                .spawn()?,
        ))
    }
    pub fn run() -> Result<()> {
        let args = std::env::args_os().skip(1).collect::<Vec<_>>();
        if args.first().is_some_and(|a| a == "--validate") {
            if args.len() != 5 {
                return Err("Validation requires data, assets, documents, name".into());
            }
            let (data, assets, docs) = (
                PathBuf::from(&args[1]),
                PathBuf::from(&args[2]),
                PathBuf::from(&args[3]),
            );
            let name = args[4].to_str().ok_or("Task name required")?;
            let app = boot(&data, &assets, &docs)?;
            fs::write(
                docs.join(format!("{name}.ready")),
                b"real child service ready",
            )?;
            let until = Instant::now() + Duration::from_secs(30);
            while !docs.join("start.request").is_file() {
                if Instant::now() >= until {
                    return Err("Start barrier deadline".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let scan = docs
                .join("资料".repeat(80))
                .join("证据".repeat(35))
                .join("Scan.png");
            let ocr = call(
                &app,
                "GET",
                &format!(
                    "/api/ocr?p={}&details=1",
                    server::urlencode(scan.to_str().ok_or("Unicode source required")?)
                ),
                None,
            )?;
            if ocr["engine"] != "readmd-tesseract"
                || !ocr["content"]
                    .as_str()
                    .is_some_and(|s| s.contains("READMD") && s.contains("12345"))
            {
                return Err("Genuine OCR did not recognize the long-path source".into());
            }
            fs::write(
                docs.join(format!("{name}-ocr.json")),
                serde_json::to_vec_pretty(&ocr)?,
            )?;
            let exported = call(
                &app,
                "POST",
                "/api/export",
                Some(
                    &json!({"format":"pdf","content":CONTENT,"baseDir":docs,"out_path":docs.join(format!("{name}.pdf")),"options":{"pdf":{"engine":"typst"}}}),
                ),
            )?;
            let pdf = fs::read(docs.join(format!("{name}.pdf")))?;
            if !pdf.starts_with(b"%PDF-") {
                return Err("Genuine Typst output missing".into());
            }
            fs::write(
                docs.join(format!("{name}-result.json")),
                serde_json::to_vec_pretty(
                    &json!({"passed":true,"actualSharedService":true,"engine":"readmd-tesseract","pdf":exported,"sourceUtf16":scan.as_os_str().encode_wide().count(),"dataUtf16":data.as_os_str().encode_wide().count()}),
                )?,
            )?;
            return Ok(());
        }
        if args.len() != 5 {
            return Err(
                "Requires assets, fonts pack, Typst pack, OCR pack, owned evidence parent".into(),
            );
        }
        let assets = PathBuf::from(&args[0]).canonicalize()?;
        let packs = [
            ("fonts", PathBuf::from(&args[1]).canonicalize()?),
            ("typst", PathBuf::from(&args[2]).canonicalize()?),
            ("ocr", PathBuf::from(&args[3]).canonicalize()?),
        ];
        let parent = PathBuf::from(&args[4]).canonicalize()?;
        let root = parent.join(format!("long-resource-native-{}", uuid::Uuid::new_v4()));
        let docs = root.join("documents");
        fs::create_dir_all(&docs)?;
        let segment = "资料".repeat(35);
        let data = root
            .join(&segment)
            .join(&segment)
            .join(&segment)
            .join(&segment)
            .join("profile");
        if data.as_os_str().encode_wide().count() <= 300 {
            return Err("Owned profile must exceed 300 UTF16 units".into());
        }
        let scan = docs
            .join("资料".repeat(80))
            .join("证据".repeat(35))
            .join("Scan.png");
        fs::create_dir_all(scan.parent().ok_or("Scan parent required")?)?;
        fs::write(
            &scan,
            include_bytes!("../../../assets/engines/ocr-probe.png"),
        )?;
        fs::write(docs.join("Research.md"), CONTENT)?;
        let original = hash(&scan)?;
        let app = boot(&data, &assets, &docs)?;
        let mut checks = Vec::new();
        for (id, location) in &packs {
            let r = pack(&app, "install", id, Some(location))?;
            if r["installed"] != true {
                return Err("Real deep-path package installation failed".into());
            }
        }
        checks.push(
            json!({"name":"genuine-offline-install-and-probe-beyond-MAX_PATH","passed":true}),
        );
        drop(app);
        let mut first = child(&data, &assets, &docs, "First")?;
        let mut second = child(&data, &assets, &docs, "Second")?;
        let until = Instant::now() + Duration::from_secs(30);
        while !docs.join("First.ready").is_file() || !docs.join("Second.ready").is_file() {
            if first.0.try_wait()?.is_some() || second.0.try_wait()?.is_some() {
                return Err("Owned child exited before shared start".into());
            }
            if Instant::now() >= until {
                return Err("Both real service startup deadline".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        fs::write(docs.join("start.request"), b"start both real operations")?;
        for (name, process) in [("First", &mut first), ("Second", &mut second)] {
            let until = Instant::now() + Duration::from_secs(90);
            loop {
                if let Some(status) = process.0.try_wait()? {
                    if !status.success() {
                        return Err(
                            format!("{name} genuine operation failed; owned log retained").into(),
                        );
                    }
                    break;
                }
                if Instant::now() >= until {
                    return Err("Genuine operations deadline".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        checks.push(json!({"name":"two-fresh-native-processes-actual-OCR-and-Typst","passed":true,"sourceUtf16":scan.as_os_str().encode_wide().count(),"profileUtf16":data.as_os_str().encode_wide().count()}));
        if hash(&scan)? != original {
            return Err("Source scan changed".into());
        }
        let app = boot(&data, &assets, &docs)?;
        for (id, _) in &packs {
            if pack(&app, "status", id, None)?["ready"] != true {
                return Err("Fresh service readiness lost".into());
            }
        }
        let unknown = data.join("capabilities/managed-v2/ocr/5.5.3-r1/Preserve-user.txt");
        fs::write(&unknown, b"owned unknown file must remain")?;
        for (id, _) in &packs {
            pack(&app, "uninstall", id, None)?;
        }
        if fs::read(&unknown)? != b"owned unknown file must remain" {
            return Err("Unknown original lost".into());
        }
        checks.push(json!({"name":"fresh-status-normal-uninstall-unknown-and-input-preserved","passed":true}));
        let pack_identities = packs
            .iter()
            .map(|(id, p)| Ok(json!({"id":id,"sha256":hash(p)?})))
            .collect::<Result<Vec<_>>>()?;
        let report = json!({"passed":true,"platform":"windows-x64","checks":checks,"actualService":true,"systemPathRestricted":true,"binarySha256":hash(&std::env::current_exe()?)?,"packs":pack_identities,"scanSha256":original,"scope":"Actual desktop/MCP shared route service, genuine locked engines on >300 UTF16 owned profile and long Unicode input, two fresh process operations, status/restart/uninstall. Not native WebView storage, remote/FAT junction support, long task scratch, owner-crash, whole stress/platform/release certification."});
        fs::write(
            root.join("report.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("{}", json!({"passed":true,"checks":3,"root":root}));
        Ok(())
    }
}
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows native evidence required");
    std::process::exit(1);
}
