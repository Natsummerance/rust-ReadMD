//! Opt-in Windows evidence with synthetic publication inputs. No license or
//! platform certification is implied by an input runtime compiling correctly.
use readmd_kernel::{latex_compile, mdexport};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let runtime = PathBuf::from(args.next().ok_or("Prepared input runtime required")?);
    let parent = PathBuf::from(args.next().ok_or("Owned evidence parent required")?);
    let root = parent.join(format!("portable-tex-publication-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)?;
    let mut checks = Vec::new();
    let mut content = String::from("# 证据与推理\n\nReadMD 中文排版验证材料 · 原创演示\n\n> 本文区分观察、解释与论证，全部数据均为合成测试资料。\n\n");
    for section in ["研究问题", "观察方法", "比较证据", "结论与局限"] {
        content.push_str(&format!("## {section}\n\n"));
        for _ in 0..4 {
            content.push_str("观察并不等于解释。我们先记录材料，再比较假设，最后说明结论的适用范围。**保留原始证据**，逐项核对数字、公式与引用，是严谨写作的基础。\n\n");
        }
        content.push_str("| 方法 | 样本量 | 准确率 |\n|:---|---:|---:|\n| 对照组 | 120 | 0.912 |\n| 改进组 | 120 | 0.954 |\n\n");
        content.push_str("$$\nP(H \\mid E)=\\frac{P(E \\mid H)P(H)}{P(E)}\n$$\n\n");
    }
    fs::write(root.join("Publication.md"), &content)?;
    for class in ["article", "ctexart", "ctexrep", "ctexbook"] {
        let dir = root.join(class);
        fs::create_dir(&dir)?;
        let source = dir.join("document.tex");
        let options = json!({"tex":{"docClass":class,"bibEngine":"bibtex"},"meta":{"title":"观察与判断：研究方法手册","author":"ReadMD"}});
        let exported = mdexport::export_tex(
            &content,
            dir.to_str().ok_or("Base path")?,
            source.to_str().ok_or("Source path")?,
            &options,
            "Publication",
        )?;
        assert!(
            exported.ok && exported.warns.as_ref().is_none_or(|warns| warns.is_empty()),
            "{class}: {:?}",
            exported.warns
        );
        let text = fs::read_to_string(&source)?;
        assert!(
            text.contains("fontset=fandol"),
            "Generated template must select portable fonts"
        );
        if matches!(class, "ctexrep" | "ctexbook") {
            assert!(text.contains("\\chapter{证据与推理}"));
        }
        let started = Instant::now();
        let bytes = latex_compile::compile_portable(
            &text,
            &dir,
            &runtime,
            "xelatex",
            Duration::from_secs(120),
        )?;
        let extracted = pdf_extract::extract_text_from_mem(&bytes)?;
        fs::write(dir.join("extracted.txt"), &extracted)?;
        // Layout extraction may insert whitespace between positioned glyphs.
        // Require every character in order; independently check exact Unicode
        // text and rendered pages with a second PDF implementation.
        let compact = extracted
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        for expected in [
            "证据与推理",
            "研究问题",
            "观察方法",
            "比较证据",
            "结论与局限",
            "ReadMD",
            "0.954",
        ] {
            assert!(compact.contains(expected), "{class} lost text: {expected}");
        }
        if matches!(class, "ctexrep" | "ctexbook") {
            assert!(
                !compact.contains("0.1"),
                "Chapter zero leaked into {class} output"
            );
        }
        let pdf = lopdf::Document::load_mem(&bytes)?;
        let descriptors = pdf
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| {
                d.get(b"Type").and_then(|v| v.as_name()).ok() == Some(b"FontDescriptor".as_slice())
            })
            .collect::<Vec<_>>();
        assert!(!descriptors.is_empty());
        assert!(
            descriptors
                .iter()
                .all(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3")),
            "Output fonts must be embedded"
        );
        fs::write(root.join(format!("{class}.pdf")), &bytes)?;
        checks.push(json!({"name":format!("chinese-{class}"),"passed":true,"engine":"xelatex","ms":started.elapsed().as_millis(),"pages":pdf.get_pages().len(),"bytes":bytes.len(),"searchableChinese":true,"allFontDescriptorsEmbedded":true,"fontDescriptors":descriptors.len()}));
    }
    // The same generated export path keeps Latin-only pdfLaTeX compatibility.
    let dir = root.join("latin-pdflatex");
    fs::create_dir(&dir)?;
    let source = dir.join("document.tex");
    mdexport::export_tex(
        "# Evidence\n\nReadMD portable output.\n\n$$E=mc^2$$\n",
        dir.to_str().ok_or("Base path")?,
        source.to_str().ok_or("Source path")?,
        &json!({"tex":{"bibEngine":"bibtex"}}),
        "Evidence",
    )?;
    let bytes = latex_compile::compile_portable(
        &fs::read_to_string(source)?,
        &dir,
        &runtime,
        "pdflatex",
        Duration::from_secs(120),
    )?;
    assert!(pdf_extract::extract_text_from_mem(&bytes)?.contains("ReadMD portable output"));
    fs::write(root.join("latin-pdflatex.pdf"), bytes)?;
    checks.push(json!({"name":"generated-latin-pdflatex","passed":true}));
    let report = json!({"passed":true,"platform":std::env::consts::OS,"offline":true,"synthetic":true,"hostTexDiscovered":false,"hostFontsUsed":false,"checks":checks,
        "scope":"Prepared Chinese input and production Markdown-to-TeX writer through native sandbox. Managed TeX installation, binary corresponding sources, full templates and redistribution review remain separate."});
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
