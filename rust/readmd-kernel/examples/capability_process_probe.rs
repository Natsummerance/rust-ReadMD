//! Real isolated pack/API/font export and cross-process lifecycle QA.
//! Never shipped as an application component; the writer barrier only schedules
//! a race at an actual installer validation log, without replacing any service.
use readmd_kernel::{capability_packs,mdexport,paths,publication_fonts,server::Request,App};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,io::Write,path::{Path,PathBuf},process::{Child,Command,Stdio},sync::{Arc,OnceLock,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
const CONTENT:&str="# 证据与推理\n\n这份合成材料验证独立进程共用 ReadMD 的出版资源，不读取用户文件。\n\n## 方法与不确定性\n\n先记录观察，再比较解释。式中 $E=mc^2$，另一项估计 :math[frac(σ^2, n)]。\n\n| 分组 | 数量 | 平均得分 |\n|---|---:|---:|\n| 实验组 | 48 | 0.84 |\n| 对照组 | 52 | 0.76 |\n\n## 校订\n\n字体用于真实 PDF 输出期间，另一进程不能修复或卸载它。取消不会覆盖已经保存的结果。\n";
fn hash(file:&Path)->String { format!("{:x}",Sha256::digest(fs::read(file).unwrap())) }
fn boot(root:&Path,assets:&Path)->Arc<App> {
    Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&root.join("profile"),&root.join("documents"),assets)).unwrap())
}
fn api(app:&Arc<App>,body:Value)->Value {
    match capability_packs::endpoint(app,&Request{method:"POST".into(),path:"/api/capabilities/packs".into(),query:Default::default(),headers:Default::default(),body:serde_json::to_vec(&body).unwrap()}) {
        Ok(response)=>{assert_eq!(response.status,200);serde_json::from_slice(&response.body).unwrap()},
        Err(error)=>json!({"ok":false,"status":error.status,"error_code":error.code}),
    }
}
fn wait_file(root:&Path,name:&str) {
    let until=Instant::now()+Duration::from_secs(30);
    while !root.join(name).is_file() {assert!(Instant::now()<until,"Timed out waiting for {name}");std::thread::sleep(Duration::from_millis(10));}
}
struct OwnedChild(Child);
impl Drop for OwnedChild { fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();} }
fn child(root:&Path,assets:&Path,mode:&str,pack:&Path)->OwnedChild {
    let mut command=Command::new(std::env::current_exe().unwrap());
    command.args(["--child",mode]).arg(root).arg(assets).arg(pack).stdin(Stdio::null());
    #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
    let mut child=OwnedChild(command.spawn().unwrap());
    let until=Instant::now()+Duration::from_secs(30);
    while !root.join(format!("{mode}.ready")).is_file() {
        if let Some(status)=child.0.try_wait().unwrap(){panic!("Child {mode} exited before ready: {status}");}
        assert!(Instant::now()<until,"Child {mode} did not become ready");std::thread::sleep(Duration::from_millis(10));
    }
    child
}
fn export(root:&Path,assets:&Path,name:&str)->Result<mdexport::ExportResult,String> {
    mdexport::export_document_with_data("pdf",CONTENT,root.join("documents").to_str().unwrap(),root.join(format!("{name}.pdf")).to_str().unwrap(),&json!({"pdf":{"engine":"rust"}}),"研究说明.md",assets,&root.join("profile"))
}
static WRITER_ROOT:OnceLock<PathBuf>=OnceLock::new();
static PAUSED:AtomicBool=AtomicBool::new(false);
struct WriterBarrier;
impl log::Log for WriterBarrier {
    fn enabled(&self,metadata:&log::Metadata)->bool {metadata.target()=="readmd::capability"}
    fn log(&self,record:&log::Record) {
        if self.enabled(record.metadata())&&record.args().to_string()=="validation begins"&&!PAUSED.swap(true,Ordering::AcqRel) {
            let root=WRITER_ROOT.get().unwrap();fs::write(root.join("writer.ready"),b"actual-install-validation").unwrap();wait_file(root,"writer.stop");
        }
    }
    fn flush(&self){}
}
static BARRIER:WriterBarrier=WriterBarrier;
fn child_main(mode:&str,root:&Path,assets:&Path,pack:&Path) {
    let app=boot(root,assets);
    if mode=="writer" {
        WRITER_ROOT.set(root.to_path_buf()).unwrap();log::set_logger(&BARRIER).unwrap();log::set_max_level(log::LevelFilter::Debug);
        let result=api(&app,json!({"action":"install","id":"fonts","path":pack}));assert_eq!(result["ok"],true);assert_eq!(result["unchanged"],true);
        fs::write(root.join("writer-result.json"),serde_json::to_vec_pretty(&result).unwrap()).unwrap();return;
    }
    assert!(matches!(mode,"reader-one"|"reader-two"));
    let _fonts=publication_fonts::scope(&root.join("profile"),assets);assert!(publication_fonts::current().is_some());
    let result=export(root,assets,mode).unwrap();assert!(result.ok);
    fs::write(root.join(format!("{mode}.ready")),b"actual-fonts-and-pdf").unwrap();wait_file(root,&format!("{mode}.stop"));
}
fn blocked(app:&Arc<App>,pack:&Path) {
    for body in [json!({"action":"uninstall","id":"fonts"}),json!({"action":"install","id":"fonts","path":pack}),
        json!({"action":"import","id":"fonts","path":pack}),
        json!({"action":"upload_start","id":"fonts","total":fs::metadata(pack).unwrap().len(),"format":"json-v1","request_key":uuid::Uuid::new_v4().to_string()})] {
        let result=api(app,body);assert_eq!(result["status"],409,"{result}");assert_eq!(result["error_code"],"capability_install_busy");
    }
}
#[cfg(windows)]
fn junction(link:&Path,target:&Path) {
    use std::os::windows::process::CommandExt;
    let program=PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child=Command::new(program).args(["-NoProfile","-NonInteractive","-Command",
        "$ErrorActionPreference='Stop'; $p=[Console]::In.ReadToEnd()|ConvertFrom-Json; New-Item -ItemType Junction -Path $p.link -Target $p.target | Out-Null"])
        .creation_flags(0x08000000).stdin(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(serde_json::to_string(&json!({"link":link,"target":target})).unwrap().as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());
}
#[cfg(unix)] fn junction(link:&Path,target:&Path){std::os::unix::fs::symlink(target,link).unwrap();}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let args=std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a|a=="--child") {child_main(args[1].to_str().unwrap(),Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4]));return Ok(());}
    let assets=PathBuf::from(args.first().ok_or("Assets required")?).canonicalize()?;
    let input=PathBuf::from(args.get(1).ok_or("Reviewed font pack required")?).canonicalize()?;
    let parent=PathBuf::from(args.get(2).ok_or("Owned output parent required")?).canonicalize()?;
    let root=parent.join(format!("capability-process-native-{}",uuid::Uuid::new_v4()));fs::create_dir(&root)?;fs::create_dir(root.join("documents"))?;
    let pack=root.join("fonts.readmd-pack");fs::hard_link(input,&pack)?;
    fs::write(root.join("documents/研究说明.md"),CONTENT)?;
    let app=boot(&root,&assets);let mut checks=Vec::new();
    let result=api(&app,json!({"action":"install","id":"fonts","path":pack}));assert_eq!(result["ok"],true,"{result}");
    let ready=api(&app,json!({"action":"status","id":"fonts"}));assert_eq!(ready["ready"],true);
    checks.push(json!({"name":"real-reviewed-pack-install-and-minimum-font-operation","passed":true}));
    let mut first=child(&root,&assets,"reader-one",&pack);let mut second=child(&root,&assets,"reader-two",&pack);
    let font=root.join("profile/capabilities/managed-v2/fonts/2026.10.10/fonts/ReadMDSansSC-Regular.ttf");let before=hash(&font);
    let busy=api(&app,json!({"action":"status","id":"fonts"}));assert_eq!(busy["installed"],true);assert_eq!(busy["ready"],false);assert_eq!(busy["busy"],true);
    blocked(&app,&pack);assert_eq!(hash(&font),before);
    checks.push(json!({"name":"two-independent-real-font-pdf-readers-block-repair-uninstall-import-and-upload","passed":true}));
    let other=api(&app,json!({"action":"status","id":"ocr"}));assert_eq!(other["busy"],false);assert_eq!(other["installed"],false);
    checks.push(json!({"name":"unrelated-capability-status-remains-available","passed":true}));
    first.0.kill()?;assert!(!first.0.wait()?.success());drop(first);blocked(&app,&pack);
    checks.push(json!({"name":"abrupt-reader-exit-does-not-release-other-process-reader","passed":true}));
    second.0.kill()?;assert!(!second.0.wait()?.success());drop(second);
    assert_eq!(api(&app,json!({"action":"status","id":"fonts"}))["ready"],true);assert_eq!(hash(&font),before);
    checks.push(json!({"name":"last-reader-abrupt-exit-releases-os-lock-without-pid-cleanup","passed":true}));
    let mut writer=child(&root,&assets,"writer",&pack);
    let busy=api(&app,json!({"action":"status","id":"fonts"}));assert_eq!(busy["busy"],true);assert_eq!(busy["installed"],false);assert_eq!(busy["ready"],false);
    blocked(&app,&pack);assert_eq!(hash(&font),before);
    checks.push(json!({"name":"actual-install-validation-exclusive-lock-blocks-other-writers-and-does-not-probe","passed":true,"schedulingBarrier":true}));
    let cancelled=AtomicBool::new(false);fs::write(root.join("cancelled.pdf"),b"Synthetic prior saved PDF")?;
    let (stop_ms,other_ms)=std::thread::scope(|scope| {
        let job=scope.spawn(||readmd_kernel::cancel::with_external(Some(&cancelled),||export(&root,&assets,"cancelled")));
        std::thread::sleep(Duration::from_millis(30));
        let (send,receive)=std::sync::mpsc::channel();
        let other_app=app.clone();
        let other=scope.spawn(move||{let started=Instant::now();let result=api(&other_app,json!({"action":"status","id":"ocr"}));let _=send.send((result,started.elapsed().as_millis()));});
        let available=receive.recv_timeout(Duration::from_millis(200));
        let started=Instant::now();cancelled.store(true,Ordering::Release);
        assert_eq!(job.join().unwrap().unwrap_err(),"cancelled");let stop_ms=started.elapsed().as_millis();other.join().unwrap();
        let (result,other_ms)=available.expect("An unrelated capability must not wait behind the external font writer");
        assert_eq!(result["busy"],false);assert_eq!(result["installed"],false);(stop_ms,other_ms)
    });
    assert!(stop_ms<200,"Waiting export cancellation took {stop_ms}ms");assert_eq!(fs::read(root.join("cancelled.pdf"))?,b"Synthetic prior saved PDF");
    checks.push(json!({"name":"real-export-waiting-on-writer-cancels-and-preserves-prior-output","passed":true,"stopMs":stop_ms}));
    checks.push(json!({"name":"unrelated-capability-progresses-during-real-export-wait-for-external-writer","passed":true,"statusMs":other_ms}));
    fs::write(root.join("writer.stop"),b"stop")?;assert!(writer.0.wait()?.success());drop(writer);
    assert_eq!(api(&app,json!({"action":"status","id":"fonts"}))["ready"],true);assert_eq!(hash(&font),before);
    checks.push(json!({"name":"real-writer-finishes-and-shared-capability-becomes-ready","passed":true}));
    let unknown=font.parent().unwrap().parent().unwrap().join("user-note.txt");fs::write(&unknown,b"Synthetic user-owned extra")?;
    let result=api(&app,json!({"action":"uninstall","id":"fonts"}));assert_eq!(result["ok"],true);assert_eq!(result["unknown_files_preserved"],true);
    assert_eq!(fs::read(&unknown)?,b"Synthetic user-owned extra");assert!(!font.exists());
    let lock=root.join("profile/capabilities/managed-v2/.locks/fonts.lock");assert_eq!(fs::metadata(&lock)?.len(),0);
    checks.push(json!({"name":"idle-uninstall-preserves-unknown-files-and-stable-empty-lock","passed":true}));
    fs::write(&lock,b"Synthetic unexpected lock contents")?;
    let invalid=api(&app,json!({"action":"uninstall","id":"fonts"}));assert_eq!(invalid["error_code"],"capability_pack_invalid");assert_eq!(fs::read(&lock)?,b"Synthetic unexpected lock contents");
    checks.push(json!({"name":"invalid-lock-never-truncates-or-deletes-existing-bytes","passed":true}));
    let malformed=root.join("redirected");fs::create_dir_all(malformed.join("documents"))?;let bad=boot(&malformed,&assets);
    let outside=root.join("outside-synthetic");fs::create_dir(&outside)?;fs::write(outside.join("fonts.lock"),b"Synthetic outside sentinel")?;
    fs::create_dir_all(malformed.join("profile/capabilities"))?;junction(&malformed.join("profile/capabilities/managed-v2/.locks"),&outside);
    let invalid=api(&bad,json!({"action":"status","id":"fonts"}));assert_eq!(invalid["error_code"],"capability_pack_invalid");
    assert_eq!(fs::read(outside.join("fonts.lock"))?,b"Synthetic outside sentinel");assert_eq!(fs::read_dir(&outside)?.count(),1);
    checks.push(json!({"name":"real-redirected-lock-directory-rejected-without-outside-write","passed":true}));
    assert_eq!(fs::read_to_string(root.join("documents/研究说明.md"))?,CONTENT);assert!(!root.join("documents/readmd-assets").exists());
    let outputs=["reader-one.pdf","reader-two.pdf"].into_iter().map(|name|json!({"path":name,"sha256":hash(&root.join(name))})).collect::<Vec<_>>();
    let report=json!({"passed":true,"platform":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),"checks":checks,
        "realReviewedPack":true,"corePdfOutputs":outputs,"sourceUnchanged":true,"userInstallationReplaced":false,"writerSchedulingBarrier":true,
        "scope":"Real independent process font scopes, core PDF and production pack services. Writer race is explicitly scheduled at validation. Not all engine-child crashes, global process budgets, clean-machine setup or other-platform certification."});
    fs::write(root.join("report.json"),serde_json::to_vec_pretty(&report)?)?;
    println!("{}",json!({"passed":true,"checks":checks.len(),"root":root}));Ok(())
}
