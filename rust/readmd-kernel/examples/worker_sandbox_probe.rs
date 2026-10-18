//! Opt-in native isolation checks using only synthetic owned files.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use readmd_kernel::worker_sandbox::Sandbox;
    use std::{
        ffi::OsString,
        fs,
        net::{TcpListener, TcpStream},
        path::PathBuf,
        time::{Duration, Instant},
    };
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "--descendant") {
        std::thread::sleep(Duration::from_secs(120));
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--child") {
        let outside = PathBuf::from(&args[2]);
        let readonly = PathBuf::from(&args[3]);
        let address = args[4].to_string_lossy();
        let own = fs::read_to_string("Input.txt")?;
        let outside_denied = fs::read_to_string(outside).is_err();
        let write_denied = fs::write(readonly, "overwritten").is_err();
        let app_container = unsafe {
            use windows::Win32::{
                Foundation::CloseHandle,
                Security::*,
                System::Threading::{GetCurrentProcess, OpenProcessToken},
            };
            let mut token = windows::Win32::Foundation::HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
            let (mut value, mut returned) = (0u32, 0u32);
            let result = GetTokenInformation(
                token,
                TokenIsAppContainer,
                Some((&mut value as *mut u32).cast()),
                4,
                &mut returned,
            );
            let _ = CloseHandle(token);
            result?;
            value == 1
        };
        let network_denied =
            TcpStream::connect_timeout(&address.parse()?, Duration::from_millis(400)).is_err();
        let child = std::process::Command::new(std::env::current_exe()?)
            .arg("--descendant")
            .spawn()?;
        fs::write(
            "Ready.json",
            serde_json::to_vec(
                &serde_json::json!({"appContainer":app_container,"ownedRead":own=="Owned task input","outsideDenied":outside_denied,"runtimeWriteDenied":write_denied,"networkDenied":network_denied,"childPid":child.id(),"argument":args[5].to_str().ok_or("Invalid synthetic argument")?}),
            )?,
        )?;
        std::thread::sleep(Duration::from_secs(120));
        return Ok(());
    }
    let parent = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("An owned evidence root is required")?;
    let root = parent.join(format!("tex-sandbox-native-空 格-{}", uuid::Uuid::new_v4()));
    let runtime = root.join("runtime");
    let workspace = root.join("task");
    fs::create_dir_all(&runtime)?;
    fs::create_dir(&workspace)?;
    let program = runtime.join("Probe.exe");
    fs::copy(std::env::current_exe()?, &program)?;
    for name in ["vcruntime140.dll", "vcruntime140_1.dll"] {
        fs::copy(
            parent.join("publication-fonts-final-bin").join(name),
            runtime.join(name),
        )?;
    }
    fs::write(runtime.join("Read-only.txt"), "Unchanged runtime")?;
    fs::write(root.join("Outside.txt"), "Synthetic outside boundary")?;
    fs::write(workspace.join("Input.txt"), "Owned task input")?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?.to_string();
    let sandbox = Sandbox::new(&runtime, &workspace)?;
    let capture = fs::File::create(workspace.join("stdout.log"))?;
    let environment = sandbox.environment()?;
    let child_args = vec![
        OsString::from("--child"),
        fs::canonicalize(root.join("Outside.txt"))?.into_os_string(),
        fs::canonicalize(runtime.join("Read-only.txt"))?.into_os_string(),
        address.into(),
        OsString::from("空 格𝑥 \"quote\" trailing\\"),
    ];
    let mut worker = sandbox.spawn(&program, &child_args, &environment, &capture)?;
    let started = Instant::now();
    while !workspace.join("Ready.json").exists() {
        if let Some(code) = worker.try_wait()? {
            return Err(format!(
                "Worker exited before its checks: {code:#x}; {}",
                fs::read_to_string(workspace.join("stdout.log"))?
            )
            .into());
        }
        if started.elapsed() > Duration::from_secs(15) {
            return Err("Worker readiness deadline".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(workspace.join("Ready.json"))?)?;
    for key in [
        "appContainer",
        "ownedRead",
        "outsideDenied",
        "runtimeWriteDenied",
        "networkDenied",
    ] {
        assert_eq!(report[key], true, "{key}");
    }
    assert_eq!(report["argument"], "空 格𝑥 \"quote\" trailing\\");
    assert_eq!(
        fs::read_to_string(runtime.join("Read-only.txt"))?,
        "Unchanged runtime"
    );
    let stop = Instant::now();
    worker.stop()?;
    assert!(stop.elapsed() < Duration::from_secs(2));
    report["stopMs"] = serde_json::json!(stop.elapsed().as_millis());
    let runtime_view = sandbox.runtime_view().to_path_buf();
    let task_view = sandbox.workspace_view().to_path_buf();
    drop(sandbox);
    assert!(runtime_view.is_dir() && task_view.is_dir());
    drop(worker);
    assert!(!runtime_view.exists() && !task_view.exists());
    report["viewsRemoved"] = serde_json::json!(true);
    report["unicodeAndSpaces"] = serde_json::json!(true);
    report["passed"] = serde_json::json!(true);
    report["platform"] = serde_json::json!("windows-x64");
    report["synthetic"] = serde_json::json!(true);
    report["scope"] = serde_json::json!(
        "Native AppContainer and job isolation probe; portable TeX integration remains separate."
    );
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        serde_json::json!({"passed":true,"report":root.join("report.json")})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Native Windows evidence requires Windows");
    std::process::exit(1);
}
