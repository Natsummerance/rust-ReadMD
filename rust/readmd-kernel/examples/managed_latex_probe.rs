//! Real managed-service fault checks using unpublished local QA resources.
//! Explicitly not distribution, licensing or cross-platform certification.
use readmd_kernel::{cancel, capability_packs, latex_compile, paths, server::Request, App};
use serde_json::json;
use std::{fs, path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    struct Diagnostic;
    impl log::Log for Diagnostic {
        fn enabled(&self,m:&log::Metadata)->bool { matches!(m.target(),"readmd::tex"|"readmd::capability") }
        fn log(&self,r:&log::Record) { if self.enabled(r.metadata()) { println!("{:?} {}",std::thread::current().id(),r.args()); } }
        fn flush(&self) {}
    }
    static DIAGNOSTIC:Diagnostic=Diagnostic;
    log::set_logger(&DIAGNOSTIC).map_err(|_|"Diagnostic logger setup failed")?;
    log::set_max_level(log::LevelFilter::Debug);
    if std::env::args_os().nth(1).as_deref()==Some(std::ffi::OsStr::new("--managed-debug")) {
        let profile=PathBuf::from(std::env::args_os().nth(2).ok_or("Owned QA profile required")?);
        let fixture=PathBuf::from(std::env::args_os().nth(3).ok_or("QA fixture required")?);
        let receipt:serde_json::Value=serde_json::from_slice(&fs::read(fixture.join("receipt.json"))?)?;
        assert_eq!(receipt["qaOnly"],true);assert_eq!(receipt["redistributionApproved"],false);
        let assets=PathBuf::from(receipt["assets"].as_str().unwrap());
        let task=profile.join(format!("managed-debug-{}",uuid::Uuid::new_v4()));fs::create_dir(&task)?;
        let source=task.join("document.tex");
        let options=json!({"tex":{"bibEngine":"bibtex","renderBibliography":false}});
        readmd_kernel::mdexport::export_tex(&fs::read_to_string(profile.join("documents/Research.md"))?,profile.join("documents").to_str().unwrap(),source.to_str().unwrap(),&options,"Research")?;
        let result=latex_compile::compile_managed(&fs::read_to_string(source)?,&task,"xelatex",Duration::from_secs(120),&profile,&assets);
        if let Ok(bytes)=&result { fs::write(task.join("result.pdf"),bytes)?; }
        let record=json!({"qaOnly":true,"passed":result.is_ok(),"error":result.err()});
        fs::write(task.join("result.json"),serde_json::to_vec_pretty(&record)?)?;println!("{record}");return Ok(());
    }
    if std::env::args_os().nth(1).as_deref()==Some(std::ffi::OsStr::new("--io-benchmark")) {
        use sha2::{Digest,Sha256};
        let runtime=PathBuf::from(std::env::args_os().nth(2).ok_or("Locked runtime required")?);
        let lock:serde_json::Value=serde_json::from_slice(&fs::read(std::env::args_os().nth(3).ok_or("Input lock required")?)?)?;
        let entries=lock["files"].as_array().ok_or("Files required")?;
        let selected=entries.iter().step_by((entries.len()/256).max(1)).take(256).collect::<Vec<_>>();
        let mut report=Vec::new();
        for mode in 0..4 {
            let start=Instant::now();let mut digest=Sha256::new();let mut bytes=0u64;
            for entry in &selected {
                let path=runtime.join(entry["path"].as_str().unwrap());
                if mode==0 { bytes+=fs::symlink_metadata(&path)?.len();continue; }
                let mut options=fs::OpenOptions::new();options.read(true);
                #[cfg(windows)] {
                    use std::os::windows::fs::OpenOptionsExt;
                    use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT,FILE_SHARE_READ};
                    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0).share_mode(FILE_SHARE_READ.0);
                }
                let mut file=options.open(&path)?;
                if mode==2 { assert_eq!(file.metadata()?.len(),entry["size"].as_u64().unwrap()); }
                #[cfg(windows)] if mode==3 {
                    use std::os::windows::io::AsRawHandle;
                    use windows::Win32::{Foundation::HANDLE,Storage::FileSystem::{GetFileInformationByHandle,BY_HANDLE_FILE_INFORMATION}};
                    let mut info=BY_HANDLE_FILE_INFORMATION::default();
                    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()),&mut info)?; }
                    assert_eq!(((info.nFileSizeHigh as u64)<<32)|info.nFileSizeLow as u64,entry["size"].as_u64().unwrap());
                }
                let mut buffer=[0u8;65536];
                loop { use std::io::Read;let n=file.read(&mut buffer)?;if n==0 {break;} bytes+=n as u64;digest.update(&buffer[..n]); }
            }
            let name=["pathMetadata","openReadHash","openMetadataReadHash","openSingleInfoReadHash"][mode];
            report.push(json!({"mode":name,"files":selected.len(),"bytes":bytes,"ms":start.elapsed().as_millis(),"sha256":format!("{:x}",digest.finalize())}));
        }
        println!("{}",serde_json::to_string(&report)?);return Ok(());
    }
    if std::env::args_os().nth(1).as_deref()==Some(std::ffi::OsStr::new("--hash-benchmark")) {
        use sha2::{Digest,Sha256};
        let input=PathBuf::from(std::env::args_os().nth(2).ok_or("Locked benchmark input required")?);
        let started=Instant::now(); let bytes=fs::read(input)?; let read=started.elapsed().as_millis();
        let started=Instant::now(); let digest=Sha256::digest(&bytes); let hash=started.elapsed().as_millis();
        println!("{}",json!({"bytes":bytes.len(),"readMs":read,"hashMs":hash,"sha256":format!("{digest:x}")}));
        return Ok(());
    }
    let mut args = std::env::args_os().skip(1);
    let fixture = PathBuf::from(args.next().ok_or("QA fixture required")?);
    let original = PathBuf::from(args.next().ok_or("Locked input runtime required")?);
    let parent = PathBuf::from(args.next().ok_or("Owned output parent required")?);
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(fixture.join("receipt.json"))?)?;
    assert_eq!(receipt["redistributionApproved"], false);
    assert_eq!(receipt["qaOnly"], true);
    let assets = PathBuf::from(receipt["assets"].as_str().unwrap());
    let lock: serde_json::Value = serde_json::from_slice(&fs::read(assets.join("capabilities/latex-2026.10-r1-windows-x64.lock.json"))?)?;
    assert_eq!(lock["review"]["redistributionApproved"], false);
    let root = parent.join(format!("tex-managed-faults-{}", uuid::Uuid::new_v4()));
    let data = root.join("profile");
    let runtime = data.join("capabilities/managed-v2/latex/2026.10-r1");
    fs::create_dir_all(&runtime)?;
    #[cfg(windows)]
    readmd_kernel::worker_sandbox::prepare_empty_runtime(&runtime)?;
    for file in lock["files"].as_array().unwrap() {
        let name = file["path"].as_str().unwrap();
        let target = runtime.join(name);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::hard_link(original.join(name), target)?;
    }
    let app = Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&data, &root.join("documents"), &assets))?);
    let uninstall = || capability_packs::endpoint(&app, &Request {
        method: "POST".into(), path: "/api/capabilities/packs".into(),
        query: Default::default(), headers: Default::default(),
        body: serde_json::to_vec(&json!({"action":"uninstall","id":"latex"})).unwrap(),
    });
    let mut checks = Vec::new();
    // Production feature entry checks readiness before starting tasks. This
    // fixture is assembled from locked files (not an installation), so run
    // that same actual validation + both native minimum operations once.
    // Cold/unprobed-tree deadline failures are retained in earlier reports;
    // this explicit setup does not certify cold-start performance.
    let ready=capability_packs::endpoint(&app,&Request {
        method:"POST".into(),path:"/api/capabilities/packs".into(),query:Default::default(),headers:Default::default(),
        body:serde_json::to_vec(&json!({"action":"status","id":"latex"})).unwrap(),
    })?;
    let ready:serde_json::Value=serde_json::from_slice(&ready.body)?;
    assert_eq!(ready["installed"],true);assert_eq!(ready["ready"],true);
    checks.push(json!({"name":"actual-entry-readiness-before-managed-tasks","passed":true,"coldStartPerformanceCertified":false}));
    let work = [root.join("first"), root.join("second")];
    for dir in &work { fs::create_dir(dir)?; }
    let rejected = root.join("rejected"); fs::create_dir(&rejected)?;
    let flags = [AtomicBool::new(false), AtomicBool::new(false)];
    let (busy_ms, stop_ms) = std::thread::scope(|scope| {
        let jobs = work.iter().zip(flags.iter()).map(|(dir, flag)| {
            let data = &data; let assets = &assets;
            scope.spawn(move || cancel::with_external(Some(flag), || {
                latex_compile::compile_managed(
                    "\\documentclass{article}\n\\begin{document}\n\\newwrite\\ready\n\\immediate\\openout\\ready=started.txt\n\\immediate\\write\\ready{running}\n\\immediate\\closeout\\ready\n\\loop\\iftrue\\repeat\n\\end{document}",
                    dir, "pdflatex", Duration::from_secs(120), data, assets)
            }))
        }).collect::<Vec<_>>();
        // Full validation is included in the real 120 s task budget. Leave
        // ten seconds for checking busy/leases and actually cancelling both.
        let until = Instant::now() + Duration::from_secs(110);
        while !work.iter().all(|dir| dir.join("started.txt").is_file()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ready = work.iter().all(|dir| dir.join("started.txt").is_file());
        let started = Instant::now();
        let busy = latex_compile::compile_managed("text", &rejected, "xelatex", Duration::from_secs(30), &data, &assets);
        let busy_ms = started.elapsed().as_millis();
        // Do not remove the diagnostic runtime if workers failed to start:
        // in that case no active lease remains to make uninstall refuse.
        let removal = ready.then(uninstall);
        let started = Instant::now();
        for flag in &flags { flag.store(true, Ordering::Release); }
        let results = jobs.into_iter().map(|job| job.join().unwrap()).collect::<Vec<_>>();
        let stop_ms = started.elapsed().as_millis();
        fs::write(root.join("worker-diagnostics.json"), serde_json::to_vec_pretty(&json!({
            "bothStarted":ready,"busyMs":busy_ms,"stopMs":stop_ms,
            "workers":results.iter().map(|result| result.as_ref().err().map(|error| error.split(": ").next().unwrap_or(error))).collect::<Vec<_>>()
        })).unwrap()).unwrap();
        assert!(ready, "Both real workers must enter their synthetic TeX loop");
        assert_eq!(busy.unwrap_err(), "latex_busy");
        assert!(busy_ms < 200);
        assert_eq!(fs::read_dir(&rejected).unwrap().count(), 0);
        assert_eq!(removal.unwrap().err().expect("Active runtime must reject uninstall").code, "latex_busy");
        for result in results { assert_eq!(result.unwrap_err(), "cancelled"); }
        assert!(stop_ms < 2000, "Managed workers must actually stop, not just dismiss the UI");
        (busy_ms, stop_ms)
    });
    checks.push(json!({"name":"two-managed-workers-third-rejected-before-validation","passed":true,"busyMs":busy_ms}));
    checks.push(json!({"name":"active-resource-lease-rejects-uninstall","passed":true}));
    checks.push(json!({"name":"both-native-process-trees-cancel","passed":true,"stopMs":stop_ms}));
    // A real readiness probe holds the install guard. Two new exports must
    // wait without stealing its process slots and falsely marking it absent.
    let waiting = [root.join("waiting-first"), root.join("waiting-second")];
    for dir in &waiting { fs::create_dir(dir)?; }
    std::thread::scope(|scope| {
        let probing = scope.spawn(|| capability_packs::endpoint(&app, &Request {
            method: "POST".into(), path: "/api/capabilities/packs".into(),
            query: Default::default(), headers: Default::default(),
            body: serde_json::to_vec(&json!({"action":"status","id":"latex"})).unwrap(),
        }));
        std::thread::sleep(Duration::from_millis(50));
        let jobs = waiting.iter().map(|dir| scope.spawn(|| latex_compile::compile_managed(
            "\\documentclass{article}\\begin{document}Concurrent managed publication.\\end{document}",
            dir, "pdflatex", Duration::from_secs(120), &data, &assets
        ))).collect::<Vec<_>>();
        let response = probing.join().unwrap().expect("Readiness probe failed");
        let status: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(status["installed"], true);
        assert_eq!(status["ready"], true);
        for job in jobs { assert!(job.join().unwrap().is_ok()); }
    });
    checks.push(json!({"name":"waiting-exports-do-not-starve-native-readiness-probe","passed":true,"workers":2}));
    let early = root.join("early"); fs::create_dir(&early)?;
    let timeout = latex_compile::compile_managed("text", &early, "xelatex", Duration::from_nanos(1), &data, &assets).unwrap_err();
    assert_eq!(timeout, "latex_timeout"); assert_eq!(fs::read_dir(&early)?.count(), 0);
    checks.push(json!({"name":"deadline-includes-lock-wait-and-resource-validation","passed":true}));
    let source_name = "texmf-dist/tex/latex/base/article.cls";
    let changed = runtime.join(source_name);
    assert!(changed.is_file());
    // Break the QA hard link BEFORE modifying: immutable source inputs stay intact.
    fs::remove_file(&changed)?; fs::write(&changed, b"synthetic corruption")?;
    let invalid = latex_compile::compile_managed("text", &early, "xelatex", Duration::from_secs(30), &data, &assets).unwrap_err();
    assert_eq!(invalid, "latex_engine_unavailable"); assert_eq!(fs::read_dir(&early)?.count(), 0);
    fs::remove_file(&changed)?; fs::hard_link(original.join(source_name), &changed)?;
    checks.push(json!({"name":"corrupt-package-file-rejected-before-execution","passed":true,"originalInputUntouched":true}));
    let outside = root.join("Outside.txt"); fs::write(&outside, "Synthetic protected boundary")?;
    let source = format!("\\documentclass{{article}}\n\\begin{{document}}\n\\input{{{}}}\n\\end{{document}}", outside.to_string_lossy().replace('\\', "/"));
    let denied = latex_compile::compile_managed(&source, &early, "xelatex", Duration::from_secs(60), &data, &assets).unwrap_err();
    assert!(denied.starts_with("latex_compilation_failed: "), "{denied}");
    assert_eq!(fs::read_to_string(&outside)?, "Synthetic protected boundary");
    checks.push(json!({"name":"OS-isolation-denies-outside-task-input","passed":true}));
    let unknown = runtime.join("user-notes.txt"); fs::write(&unknown, "Synthetic user file")?;
    uninstall()?;
    assert_eq!(fs::read_to_string(&unknown)?, "Synthetic user file");
    assert!(!runtime.join("bin/windows/xelatex.exe").exists());
    checks.push(json!({"name":"leases-release-and-uninstall-preserves-unknown-files","passed":true}));
    fs::write(root.join("report.json"), serde_json::to_vec_pretty(&json!({
        "passed":true,"platform":"windows-x64","qaOnlyRuntime":true,"redistributionApproved":false,
        "formalPackAcceptance":false,"checks":checks,"offline":true
    }))?)?;
    println!("{}", json!({"passed":true,"checks":checks.len(),"profile":root}));
    Ok(())
}
