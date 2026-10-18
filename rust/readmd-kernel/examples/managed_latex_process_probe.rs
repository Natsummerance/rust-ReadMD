//! Opt-in real TeX owner-crash and cross-process budget checks.
//! Uses locked, unpublished QA inputs; never production distribution approval.
#[cfg(windows)]
mod native {
    use readmd_kernel::{cancel, capability_packs, latex_compile, paths, pet_launcher, server::Request, App};
    use serde_json::{json, Value};
    use std::{fs, path::{Path, PathBuf}, process::{Child, Command, Stdio}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    const LOOP:&str="\\documentclass{article}\n\\begin{document}\n\\newwrite\\ready\n\\immediate\\openout\\ready=started.txt\n\\immediate\\write\\ready{running}\n\\immediate\\closeout\\ready\n\\loop\\iftrue\\repeat\n\\end{document}";

    struct OwnedChild(Child);
    impl Drop for OwnedChild { fn drop(&mut self) { let _=self.0.kill();let _=self.0.wait(); } }
    struct ObservedProcess(windows::Win32::Foundation::HANDLE);
    impl Drop for ObservedProcess { fn drop(&mut self) { unsafe { let _=windows::Win32::Foundation::CloseHandle(self.0); } } }
    impl ObservedProcess {
        fn open(pid:u32)->Result<Self> {
            use windows::Win32::System::Threading::{OpenProcess,PROCESS_ACCESS_RIGHTS};
            Ok(Self(unsafe { OpenProcess(PROCESS_ACCESS_RIGHTS(0x0010_0000),false,pid)? }))
        }
        fn stopped(&self,timeout:Duration)->bool {
            use windows::Win32::{Foundation::WAIT_OBJECT_0,System::Threading::WaitForSingleObject};
            unsafe { WaitForSingleObject(self.0,timeout.as_millis().min(u32::MAX as u128) as u32)==WAIT_OBJECT_0 }
        }
    }
    fn launch(data:&Path,assets:&Path,task:&Path)->Result<OwnedChild> {
        fs::create_dir(task)?;
        let output=fs::File::create(task.join("owner.log"))?;
        let system=PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot required")?).join("System32");
        let child=Command::new(std::env::current_exe()?).arg("--worker").arg(data).arg(assets).arg(task)
            .env("PATH",system).stdin(Stdio::null()).stdout(Stdio::from(output.try_clone()?)).stderr(Stdio::from(output)).spawn()?;
        Ok(OwnedChild(child))
    }
    fn wait_started(child:&mut OwnedChild,task:&Path)->Result<()> {
        let until=Instant::now()+Duration::from_secs(110);
        while !task.join("started.txt").is_file() {
            if let Some(status)=child.0.try_wait()? { return Err(format!("Owner exited before real TeX loop: {status}").into()); }
            if Instant::now()>=until { return Err("Real TeX loop start deadline".into()); }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
    fn engine(child:&OwnedChild)->Result<ObservedProcess> {
        let table=pet_launcher::list_processes().ok_or("Process table unavailable")?;
        let descendants=pet_launcher::descendants(&table,child.0.id());
        let processes=table.iter().filter(|p|descendants.contains(&p.pid)&&p.name.eq_ignore_ascii_case("pdflatex.exe")).collect::<Vec<_>>();
        if processes.len()!=1 { return Err("Exactly one real pdflatex worker must be active per owner".into()); }
        ObservedProcess::open(processes[0].pid)
    }
    fn cancel_worker(child:&mut OwnedChild,task:&Path,engine:&ObservedProcess)->Result<u128> {
        let start=Instant::now();fs::write(task.join("cancel.request"),b"cancel")?;
        loop {
            if let Some(status)=child.0.try_wait()? {
                if !status.success() { return Err("Owner cancellation failed".into()); }
                break;
            }
            if start.elapsed()>=Duration::from_secs(2) { return Err("Owner cancellation exceeded two seconds".into()); }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !engine.stopped(Duration::ZERO) { return Err("Owner completed but engine remains alive".into()); }
        let result:Value=serde_json::from_slice(&fs::read(task.join("result.json"))?)?;
        if result["error"]!="cancelled" { return Err("Expected actual cancelled result".into()); }
        Ok(start.elapsed().as_millis())
    }
    fn rejected(data:&Path,assets:&Path,task:&Path)->Result<u128> {
        fs::create_dir(task)?;
        let start=Instant::now();
        let result=latex_compile::compile_managed("text",task,"pdflatex",Duration::from_secs(30),data,assets);
        let ms=start.elapsed().as_millis();
        let files=fs::read_dir(task)?.map(|e|e.map(|e|e.file_name().to_string_lossy().into_owned())).collect::<std::result::Result<Vec<_>,_>>()?;
        let diagnostic=json!({"busyMs":ms,"error":result.as_ref().err(),"unexpectedSuccess":result.is_ok(),"taskFiles":files});
        let name=task.file_name().and_then(|n|n.to_str()).ok_or("Owned busy task name missing")?;
        fs::write(task.parent().ok_or("Owned evidence parent missing")?.join(format!("{name}-busy.json")),serde_json::to_vec_pretty(&diagnostic)?)?;
        if result.as_ref().err().map(String::as_str)!=Some("latex_busy") { return Err(format!("Third task did not reject as busy: {diagnostic}").into()); }
        if ms>=200||!files.is_empty() { return Err(format!("Busy request exceeded budget or wrote task files: {diagnostic}").into()); }
        Ok(ms)
    }
    fn api(app:&Arc<App>,action:&str,pack:Option<&Path>)->Result<Value> {
        let response=capability_packs::endpoint(app,&Request{method:"POST".into(),path:"/api/capabilities/packs".into(),query:Default::default(),headers:Default::default(),body:serde_json::to_vec(&json!({"action":action,"id":"latex","path":pack}))?})?;
        Ok(serde_json::from_slice(&response.body)?)
    }
    pub fn run()->Result<()> {
        let args=std::env::args_os().skip(1).collect::<Vec<_>>();
        if args.first().is_some_and(|v|v=="--worker") {
            if args.len()!=4 { return Err("Worker requires profile, assets and task".into()); }
            let (data,assets,task)=(PathBuf::from(&args[1]),PathBuf::from(&args[2]),PathBuf::from(&args[3]));
            let flag=AtomicBool::new(false);let finished=AtomicBool::new(false);
            let result=std::thread::scope(|scope| {
                scope.spawn(|| { while !finished.load(Ordering::Acquire) {
                    if task.join("cancel.request").is_file() { flag.store(true,Ordering::Release);break; }
                    std::thread::sleep(Duration::from_millis(10));
                } });
                let result=cancel::with_external(Some(&flag),||latex_compile::compile_managed(LOOP,&task,"pdflatex",Duration::from_secs(120),&data,&assets));
                finished.store(true,Ordering::Release);result
            });
            fs::write(task.join("result.json"),serde_json::to_vec_pretty(&json!({"error":result.as_ref().err(),"unexpectedSuccess":result.is_ok()}))?)?;
            return if result.as_ref().err().map(String::as_str)==Some("cancelled") { Ok(()) } else { Err("Real worker failed before cancellation".into()) };
        }
        if args.len()!=3 { return Err("Requires QA fixture, locked runtime and owned evidence parent".into()); }
        let (fixture,original,parent)=(PathBuf::from(&args[0]),PathBuf::from(&args[1]),PathBuf::from(&args[2]));
        let receipt:Value=serde_json::from_slice(&fs::read(fixture.join("receipt.json"))?)?;
        if receipt["qaOnly"]!=true||receipt["redistributionApproved"]!=false { return Err("Unpublished QA fixture required".into()); }
        let assets=PathBuf::from(receipt["assets"].as_str().ok_or("Fixture assets missing")?);
        let lock:Value=serde_json::from_slice(&fs::read(assets.join("capabilities/latex-2026.10-r1-windows-x64.lock.json"))?)?;
        if lock["review"]["redistributionApproved"]!=false { return Err("Do not substitute a distribution-approved lock".into()); }
        let root=parent.join(format!("tex-process-native-{}",uuid::Uuid::new_v4()));
        let data=root.join("profile");let runtime=data.join("capabilities/managed-v2/latex/2026.10-r1");
        if !original.is_dir() { return Err("Original audited runtime missing".into()); }
        fs::create_dir_all(root.join("documents"))?;
        let app=Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&data,&root.join("documents"),&assets))?);
        // Hard links preserve the source ACL rather than inheriting the new
        // sandbox permissions. Use the actual stream installer and its probes.
        let pack=PathBuf::from(receipt["pack"].as_str().ok_or("QA pack missing")?);
        println!("{}",json!({"phase":"actual-offline-install","qaOnly":true,"root":root}));
        let installation=api(&app,"install",Some(&pack))?;
        fs::write(root.join("installation.json"),serde_json::to_vec_pretty(&installation)?)?;
        if installation["installed"]!=true { return Err(format!("Actual installation failed: {installation}").into()); }
        let readiness=api(&app,"status",None)?;
        fs::write(root.join("readiness.json"),serde_json::to_vec_pretty(&readiness)?)?;
        if readiness["ready"]!=true { return Err("Actual entry readiness failed".into()); }
        println!("{}",json!({"phase":"actual-install-and-entry-ready"}));
        // Explicitly warmed by the real entry probe, never cold-performance evidence.
        let mut checks=vec![json!({"name":"actual-stream-install-and-entry-readiness","passed":true,"coldPerformanceCertified":false})];
        let first_task=root.join("first");let second_task=root.join("second");
        let mut first=launch(&data,&assets,&first_task)?;let mut second=launch(&data,&assets,&second_task)?;
        wait_started(&mut first,&first_task)?;wait_started(&mut second,&second_task)?;
        let first_engine=engine(&first)?;let second_engine=engine(&second)?;
        let busy_ms=rejected(&data,&assets,&root.join("third"))?;
        checks.push(json!({"name":"two-independent-owners-third-refused-before-hashing","passed":true,"busyMs":busy_ms}));
        let stop=Instant::now();first.0.kill()?;let status=first.0.wait()?;
        if status.success()||!first_engine.stopped(Duration::from_secs(2)) { return Err("Killed owner left its actual engine alive".into()); }
        let crash_ms=stop.elapsed().as_millis();
        if crash_ms>=2000||second_engine.stopped(Duration::ZERO) { return Err("Crash deadline failed or other owner was stopped".into()); }
        checks.push(json!({"name":"owner-abrupt-exit-stops-only-its-real-tex-engine","passed":true,"stopMs":crash_ms}));
        let replacement_task=root.join("replacement");let mut replacement=launch(&data,&assets,&replacement_task)?;
        wait_started(&mut replacement,&replacement_task)?;let replacement_engine=engine(&replacement)?;
        let replacement_busy_ms=rejected(&data,&assets,&root.join("fourth"))?;
        checks.push(json!({"name":"one-slot-recovered-other-owner-remains-protected","passed":true,"busyMs":replacement_busy_ms}));
        let second_stop=cancel_worker(&mut second,&second_task,&second_engine)?;
        let replacement_stop=cancel_worker(&mut replacement,&replacement_task,&replacement_engine)?;
        checks.push(json!({"name":"actual-engine-cancellation-and-no-orphans","passed":true,"stopMs":[second_stop,replacement_stop]}));
        let final_task=root.join("final");fs::create_dir(&final_task)?;
        let bytes=latex_compile::compile_managed("\\documentclass{article}\n\\begin{document}Recovered $x^2$.\\end{document}",&final_task,"pdflatex",Duration::from_secs(120),&data,&assets)?;
        let pdf=lopdf::Document::load_mem(&bytes)?;if pdf.get_pages().is_empty() { return Err("Recovered operation produced no PDF pages".into()); }
        fs::write(root.join("Recovered.pdf"),bytes)?;
        checks.push(json!({"name":"real-pdf-after-owner-crash-and-cancellation","passed":true}));
        fs::write(runtime.join("Preserve-user.txt"),b"Synthetic unknown file")?;
        api(&app,"uninstall",None)?;
        if fs::read(runtime.join("Preserve-user.txt"))?!=b"Synthetic unknown file" { return Err("Unknown file not preserved".into()); }
        checks.push(json!({"name":"idle-uninstall-works-and-preserves-unknown-files","passed":true}));
        let recovery=readmd_kernel::task_workspace::recover(&data,128)?;
        if recovery.failed!=0||recovery.running!=0||recovery.preserved!=0 { return Err(format!("Native sandbox resource recovery failed: {recovery:?}").into()); }
        checks.push(json!({"name":"persisted-native-sandbox-resources-recover-after-real-tex-owner-crash","passed":true,"recovery":recovery}));
        let report=json!({"passed":true,"platform":"windows-x64","qaOnly":true,"redistributionApproved":false,"checks":checks,"scope":"Real managed Rust TeX service in independent owner processes, OS engine handles, sandbox jobs and acknowledged native-resource recovery. Not desktop/MCP transport, other engines, pre-acknowledgement windows, generated TeX file cleanup, old-client, cold performance, material licensing or full B3 certification."});
        fs::write(root.join("report.json"),serde_json::to_vec_pretty(&report)?)?;
        println!("{}",json!({"passed":true,"report":root.join("report.json")}));Ok(())
    }
}
#[cfg(windows)]
fn main()->Result<(),Box<dyn std::error::Error>> { native::run() }
#[cfg(not(windows))]
fn main() { eprintln!("Native evidence requires Windows");std::process::exit(1); }
