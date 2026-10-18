//! Opt-in portable TeX integration evidence; uses only synthetic owned inputs.
//! This does not certify distribution licences or make a pack downloadable.
use readmd_kernel::{cancel, latex_compile};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let runtime = args
        .next()
        .map(PathBuf::from)
        .ok_or("Managed input runtime required")?;
    let parent = args
        .next()
        .map(PathBuf::from)
        .ok_or("Owned evidence parent required")?;
    let root = parent.join(format!("portable-tex-native-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)?;
    let outside = root.join("Outside.txt");
    fs::write(&outside, "Synthetic private boundary")?;
    let source="\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\section{Evidence}\\label{sec:evidence}\nSection~\\ref{sec:evidence}: $E=mc^2$.\n\\end{document}\n";
    let mut checks = Vec::new();
    for engine in ["xelatex", "pdflatex"] {
        let unopened = root.join(format!("{engine}-preflight"));
        fs::create_dir(&unopened)?;
        assert_eq!(
            latex_compile::compile_portable(source, &unopened, &runtime, engine, Duration::ZERO)
                .unwrap_err(),
            "latex_timeout"
        );
        assert_eq!(
            readmd_kernel::speech::with_cancel(std::sync::Arc::new(|| true), || {
                latex_compile::compile_portable(
                    source,
                    &unopened,
                    &runtime,
                    engine,
                    Duration::from_secs(30),
                )
            })
            .unwrap_err(),
            "cancelled"
        );
        assert_eq!(fs::read_dir(&unopened)?.count(), 0);
        checks.push(json!({"name":format!("{engine}-preflight-timeout-cancel"),"passed":true,"noTaskFiles":true}));
        let dir = root.join(engine);
        fs::create_dir(&dir)?;
        let started = Instant::now();
        let bytes = latex_compile::compile_portable(
            source,
            &dir,
            &runtime,
            engine,
            Duration::from_secs(60),
        )?;
        let text = pdf_extract::extract_text_from_mem(&bytes)?;
        assert!(
            text.contains("Evidence") && text.contains("Section 1"),
            "{engine} produced incomplete text"
        );
        fs::write(root.join(format!("{engine}.pdf")), &bytes)?;
        checks.push(json!({"name":engine,"passed":true,"ms":started.elapsed().as_millis(),"referencesSettled":true,"searchableText":true}));
        let failed = root.join(format!("{engine}-error"));
        fs::create_dir(&failed)?;
        let error = latex_compile::compile_portable(
            "\\documentclass{article}\n\\begin{document}\n\\ReadMDUnknownCommand\n\\end{document}",
            &failed,
            &runtime,
            engine,
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert!(
            error.starts_with("latex_compilation_failed: ") && error.contains("document.tex:3:"),
            "{error}"
        );
        let boundary = root.join(format!("{engine}-boundary"));
        fs::create_dir(&boundary)?;
        let path = dunce::canonicalize(&outside)?
            .to_str()
            .ok_or("Invalid synthetic path")?
            .replace('\\', "/");
        let input=format!("\\documentclass{{article}}\n\\begin{{document}}\n\\newread\\boundary\n\\openin\\boundary=\"{path}\"\n\\ifeof\\boundary Denied\\else\\errmessage{{Outside task file was readable}}\\fi\n\\end{{document}}");
        let bytes = latex_compile::compile_portable(
            &input,
            &boundary,
            &runtime,
            engine,
            Duration::from_secs(30),
        )?;
        assert!(pdf_extract::extract_text_from_mem(&bytes)?.contains("Denied"));
        checks.push(json!({"name":format!("{engine}-generated-line-and-outside-read"),"passed":true,"line":3,"osFileBoundary":true}));
        if engine == "xelatex" {
            let missing = root.join("xelatex-missing-glyph");
            fs::create_dir(&missing)?;
            let error = latex_compile::compile_portable(
                "\\documentclass{article}\n\\begin{document}\nLatin 研究\n\\end{document}",
                &missing,
                &runtime,
                engine,
                Duration::from_secs(30),
            )
            .unwrap_err();
            assert!(
                error.starts_with("latex_compilation_failed: Missing character:"),
                "{error}"
            );
            checks.push(
                json!({"name":"missing-glyph-is-an-error","passed":true,"silentLossRejected":true}),
            );
        }
        let running = root.join(format!("{engine}-cancel"));
        fs::create_dir(&running)?;
        let flag = std::sync::atomic::AtomicBool::new(false);
        let started = Instant::now();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(1000));
                flag.store(true, std::sync::atomic::Ordering::Release);
            });
            cancel::with_external(Some(&flag), || {
                latex_compile::compile_portable("\\documentclass{article}\n\\begin{document}\n\\loop\\iftrue\\repeat\n\\end{document}",&running,&runtime,engine,Duration::from_secs(30))
            })
        });
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(started.elapsed() < Duration::from_secs(3));
        checks.push(json!({"name":format!("{engine}-active-cancellation"),"passed":true,"totalMs":started.elapsed().as_millis(),"cancelAfterMs":1000}));
    }
    // Two real workers hold their slots. A third request must fail before
    // creating any sandbox/task files, and both running jobs must then stop.
    let concurrent = [root.join("parallel-a"), root.join("parallel-b")];
    for dir in &concurrent {
        fs::create_dir(dir)?;
    }
    let rejected = root.join("parallel-busy");
    fs::create_dir(&rejected)?;
    let flags = [
        std::sync::atomic::AtomicBool::new(false),
        std::sync::atomic::AtomicBool::new(false),
    ];
    let (busy_ms, stop_ms) = std::thread::scope(
        |scope| -> Result<(u128, u128), Box<dyn std::error::Error>> {
            let handles = concurrent.iter().zip(flags.iter()).map(|(dir,flag)| {
            let runtime = &runtime;
            scope.spawn(move || cancel::with_external(Some(flag), || {
                latex_compile::compile_portable("\\documentclass{article}\n\\begin{document}\n\\newwrite\\ready\n\\immediate\\openout\\ready=started.txt\n\\immediate\\write\\ready{running}\n\\immediate\\closeout\\ready\n\\loop\\iftrue\\repeat\n\\end{document}",dir,runtime,"pdflatex",Duration::from_secs(60))
            }))
        }).collect::<Vec<_>>();
            let deadline = Instant::now() + Duration::from_secs(30);
            while !concurrent
                .iter()
                .all(|dir| dir.join("started.txt").is_file())
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            let ready = concurrent
                .iter()
                .all(|dir| dir.join("started.txt").is_file());
            let started = Instant::now();
            let result = latex_compile::compile_portable(
                source,
                &rejected,
                &runtime,
                "xelatex",
                Duration::from_secs(30),
            );
            let busy_ms = started.elapsed().as_millis();
            let started = Instant::now();
            for flag in &flags {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
            let results = handles
                .into_iter()
                .map(|h| h.join().expect("portable worker panicked"))
                .collect::<Vec<_>>();
            let stop_ms = started.elapsed().as_millis();
            assert!(ready, "Both native workers must reach the TeX loop");
            assert_eq!(result.unwrap_err(), "latex_busy");
            assert!(busy_ms < 200, "Busy request performed expensive setup");
            assert_eq!(fs::read_dir(&rejected)?.count(), 0);
            for result in results {
                assert_eq!(result.unwrap_err(), "cancelled");
            }
            assert!(
                stop_ms < 2000,
                "Concurrent worker cancellation exceeded two seconds"
            );
            Ok((busy_ms, stop_ms))
        },
    )?;
    checks.push(json!({"name":"two-active-workers-third-busy-and-cancel","passed":true,"workers":2,"busyMs":busy_ms,"noRejectedTaskFiles":true,"cancelledJobs":2,"stopMs":stop_ms}));
    assert_eq!(fs::read_to_string(&outside)?, "Synthetic private boundary");
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"platform":std::env::consts::OS,"offline":true,"synthetic":true,"hostTexDiscovered":false,"scope":"Prepared portable input and OS sandbox integration. Pack distribution and Chinese template coverage remain separate.","checks":checks}),
        )?,
    )?;
    println!(
        "{}",
        json!({"passed":true,"checks":checks.len(),"report":root.join("report.json")})
    );
    Ok(())
}
