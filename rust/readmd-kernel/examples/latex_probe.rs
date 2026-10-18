//! Opt-in real offline engine verification. Requires both installed engines.
use readmd_kernel::{latex_compile,mdexport,speech};
use serde_json::json;
use std::{fs,path::PathBuf,time::{Duration,Instant}};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let root=std::env::args_os().nth(1).map(PathBuf::from).ok_or("An owned output directory is required")?;fs::create_dir_all(&root)?;
    let mut checks=Vec::new();
    for engine in ["xelatex","pdflatex"]{
        let dir=tempfile::tempdir()?;let start=Instant::now();
        let source="\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\section{Evidence}\\label{sec:evidence}\nThe result in Section~\\ref{sec:evidence}: $E=mc^2$.\n\\end{document}\n";
        let bytes=latex_compile::compile_source(source,dir.path(),engine,Duration::from_secs(60))?;
        let text=pdf_extract::extract_text_from_mem(&bytes)?;assert!(text.contains("Evidence")&&text.contains("Section 1"));
        fs::write(root.join(format!("{engine}.pdf")),bytes)?;checks.push(json!({"name":engine,"passed":true,"ms":start.elapsed().as_millis(),"referencesSettled":true}));
        let failed=tempfile::tempdir()?;let error=latex_compile::compile_source("\\documentclass{article}\n\\begin{document}\n\\ReadMDUnknownCommand\n\\end{document}",failed.path(),engine,Duration::from_secs(60)).unwrap_err();assert!(error.starts_with("latex_compilation_failed: "),"{error}");assert!(error.contains("document.tex:3:"),"{error}");
        let timeout=tempfile::tempdir()?;assert_eq!(latex_compile::compile_source(source,timeout.path(),engine,Duration::ZERO).unwrap_err(),"latex_timeout");
        let cancel=tempfile::tempdir()?;let started=Instant::now();let result=speech::with_cancel(std::sync::Arc::new(||true),||latex_compile::compile_source(source,cancel.path(),engine,Duration::from_secs(30)));assert_eq!(result.unwrap_err(),"cancelled");assert!(started.elapsed()<Duration::from_secs(2));
        checks.push(json!({"name":format!("{engine}-errors-timeout-cancel"),"passed":true,"locatedGeneratedLine":3}));
        let running=tempfile::tempdir()?;let flag=std::sync::atomic::AtomicBool::new(false);let started=Instant::now();
        let result=std::thread::scope(|scope|{
            scope.spawn(||{std::thread::sleep(Duration::from_millis(700));flag.store(true,std::sync::atomic::Ordering::Release);});
            readmd_kernel::cancel::with_external(Some(&flag),||latex_compile::compile_source("\\documentclass{article}\n\\begin{document}\n\\loop\\iftrue\\repeat\n\\end{document}",running.path(),engine,Duration::from_secs(30)))
        });assert_eq!(result.unwrap_err(),"cancelled");assert!(started.elapsed()<Duration::from_secs(3));
        checks.push(json!({"name":format!("{engine}-active-process-cancellation"),"passed":true,"totalMs":started.elapsed().as_millis(),"cancellationAfterMs":700}));
    }
    let source="---\ntitle: 证据与推理\nbibliography: refs.bib\ncsl: apa\nlang: zh-CN\n---\n# 从观察到结论\n\n核对原始论证 [@kant, p. 42]。\n\n$$E=mc^2$$\n\n| 观察 | 说明 |\n|---|---|\n| 证据 | 保留原文 |\n";
    fs::write(root.join("refs.bib"),"@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781}}")?;fs::write(root.join("Research.md"),source)?;
    let output=root.join("Research.pdf");let options=json!({"pdf":{"engine":"xelatex"},"tex":{"margin":"2cm","fontSize":"12pt"}});
    let result=mdexport::export_document("pdf",source,root.to_str().unwrap(),output.to_str().unwrap(),&options,"Research",&root)?;assert!(result.ok);
    let text=pdf_extract::extract_text_from_mem(&fs::read(&output)?)?;assert!(text.contains("1781")&&text.contains("42")&&text.contains("Critique"));assert_eq!(fs::read_to_string(root.join("Research.md"))?,source);
    assert!(!root.join("document.tex").exists()&&!root.join("document.assets").exists());
    checks.push(json!({"name":"CJK Markdown CSL table and formula to native TeX PDF","passed":true,"sourcePreserved":true,"noSiblingBuildFiles":true}));
    fs::write(root.join("report.json"),serde_json::to_vec_pretty(&json!({"passed":true,"platform":std::env::consts::OS,"offline":true,"checks":checks}))?)?;
    println!("Real offline TeX engine checks passed: {}",checks.len());Ok(())
}
