//! Dev-only fontspec token calibration through the real managed compiler.
//! No native suite, publication acceptance, system font or direct TeX launch.
use readmd_kernel::latex_compile;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}, time::{Duration, Instant}};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSans.ttf");
const LICENSE: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSans.LICENSE.txt");
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn create_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn existing_directory(path: PathBuf) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if !path.is_absolute() || !path.is_dir() || dunce::canonicalize(&path)? != path {
        return Err("Explicit absolute directory without redirected ancestors required".into());
    }
    Ok(path)
}

fn probe_termes_smallcaps(data: &Path, assets: &Path, workspace: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let directory = workspace.join("termes-smallcaps");
    fs::create_dir(&directory)?;
    let mut source = String::from(concat!(
        "\\documentclass[11pt,a4paper]{article}\n\\usepackage{iftex}\n",
        "\\usepackage[UTF8,fontset=fandol]{ctex}\n\\usepackage[margin=2.5cm]{geometry}\n",
        "\\usepackage{amsmath,amssymb,amsfonts,mathtools}\n",
        "\\usepackage{unicode-math}\n\\setmathfont{latinmodern-math.otf}\n",
        "\\usepackage{graphicx}\n\\usepackage[export]{adjustbox}\n",
        "\\usepackage{booktabs,tabularx,longtable}\n\\usepackage[normalem]{ulem}\n",
        "\\usepackage{xcolor,listings,hyperref}\n",
        "\\hypersetup{colorlinks=true,linkcolor=blue!60!black,urlcolor=blue!60!black,citecolor=blue!60!black}\n",
        "\\lstset{basicstyle=\\ttfamily\\small,breaklines=true,columns=fullflexible,keepspaces=true,frame=single,rulecolor=\\color{black!15},backgroundcolor=\\color{black!3},keywordstyle=\\color{blue!70!black}\\bfseries,commentstyle=\\color{green!45!black}\\itshape,stringstyle=\\color{red!60!black},showstringspaces=false,upquote=true}\n",
        "\\setlength{\\parskip}{0.5em}\n",
        "\\providecommand{\\tightlist}{\\setlength{\\itemsep}{0pt}\\setlength{\\parskip}{0pt}}\n",
        "\\newfontfamily\\readmdtermesfont[NFSSFamily=readmdtermes,BoldFont={texgyretermes-bold.otf},ItalicFont={texgyretermes-italic.otf},BoldItalicFont={texgyretermes-bolditalic.otf}]{texgyretermes-regular.otf}\n",
        "\\newfontfamily\\readmdtermessmallcapsfont[NFSSFamily=readmdtermessmallcaps,Letters=SmallCaps,BoldFont={texgyretermes-bold.otf},ItalicFont={texgyretermes-italic.otf},BoldItalicFont={texgyretermes-bolditalic.otf}]{texgyretermes-regular.otf}\n",
        "\\newcommand{\\readmdtermesblockfont}{\\renewcommand{\\rmdefault}{readmdtermes}\\renewcommand{\\familydefault}{readmdtermes}\\readmdtermesfont}\n",
        "\\newcommand{\\readmdbaselinecaps}[1]{{\\readmdtermesfont\\textsc{#1}}}\n",
        "\\newcommand{\\readmdfeaturecaps}[1]{{\\readmdtermessmallcapsfont #1}}\n",
        "\\newcommand{\\readmdlocalcaps}[1]{{\\XeTeXgenerateactualtext=1\\typeout{READMD-PROBE LOCAL-INNER-ACTUALTEXT: \\the\\XeTeXgenerateactualtext}\\readmdfeaturecaps{#1}}}\n",
        "\\newcommand{\\readmdprobelog}[1]{\\typeout{READMD-PROBE #1: \\fontname\\font}}\n",
        "\\XeTeXgenerateactualtext=0\n\\begin{document}\n"
    ));
    for (group, caps, actual_text) in [("BASE", "readmdbaselinecaps", 0),
        ("FEATURE", "readmdfeaturecaps", 0), ("ACTUAL", "readmdfeaturecaps", 1),
        ("LOCAL", "readmdlocalcaps", 0)] {
        // XeTeX may consult the parameter at shipout. Each page keeps its
        // explicit value through clearpage; endgroup then restores the old one.
        source.push_str(&format!("\\begingroup\n\\XeTeXgenerateactualtext={actual_text}\n\\readmdtermesblockfont\n\\section*{{{group}}}\n{group} BEFORE Abcxyz\\par\n"));
        for (face, bold, italic) in [("R", false, false), ("B", true, false),
            ("I", false, true), ("BI", true, true)] {
            for mode in ["NORMAL", "SMALL", "UPPER"] {
                let marker = format!("{group}_{face}_{mode}");
                let word = if mode == "UPPER" { "ABCXYZ" } else { "Abcxyz" };
                let mut body = format!("\\readmdprobelog{{{marker}}}{} {word}", marker.replace('_', "\\_"));
                if italic { body = format!("\\textit{{{body}}}"); }
                if bold { body = format!("\\textbf{{{body}}}"); }
                if mode == "SMALL" { body = format!("\\{caps}{{{body}}}"); }
                source.push_str(&format!("\\noindent {body}\\par\n"));
            }
        }
        source.push_str(&format!("{group} AFTER Abcxyz\\par\n中文 \\(\\alpha+\\int_0^1 x^2\\,dx\\)\\par\n"));
        if group == "LOCAL" {
            // Test native-node capture independently of page-level state: the
            // run and family groups have both ended before this page ships.
            source.push_str("\\typeout{READMD-PROBE LOCAL-PAGE-BEFORE-END: \\the\\XeTeXgenerateactualtext}\n\\endgroup\n\\typeout{READMD-PROBE LOCAL-PAGE-AFTER-END: \\the\\XeTeXgenerateactualtext}\n\\clearpage\n");
        } else {
            source.push_str("\\clearpage\n\\endgroup\n");
        }
    }
    source.push_str("\\end{document}\n");
    let input = json!({"devOnly":true,"acceptance":false,"requestedCase":"termes-smallcaps",
        "sourceVersion":"termes-smallcaps-calibration-v4","sourceExampleSha256":digest(include_bytes!("publication_tex_font_probe.rs")),
        "data":data,"assets":assets,"managedBudgetMs":120000,"managedInvocations":1,
        "noReadinessRequest":true,"noDirectEngineInvocation":true,"fontSidecarsCreated":false,
        "groups":[{"name":"BASE","smallcaps":"current textsc","actualText":0},
            {"name":"FEATURE","smallcaps":"independent four-face NFSS family plus Letters=SmallCaps","actualText":0},
            {"name":"ACTUAL","smallcaps":"same independent four-face NFSS family","actualText":1},
            {"name":"LOCAL","smallcaps":"same independent four-face NFSS family","actualText":0,"smallcapsRunActualText":1}],
        "actualTextScope":"ACTUAL retains page-level 1 through clearpage. LOCAL enables 1 only inside each SMALL run; page/family groups end with 0 before clearpage. Independent output inspection must determine both effects.",
        "scope":"Single real managed PDF for separate glyph, Unicode, marked-content and before/after leakage inspection. Compile success is not semantic acceptance."});
    create_new(&workspace.join("inputs.json"), &serde_json::to_vec_pretty(&input)?)?;
    create_new(&directory.join("document.tex"), source.as_bytes())?;
    let started = Instant::now();
    let actual = latex_compile::compile_managed(&source, &directory, "xelatex",
        Duration::from_secs(120), data, assets);
    let elapsed_ms = started.elapsed().as_millis();
    let (accepted, raw_error) = match actual {
        Ok(bytes) => { create_new(&directory.join("result.pdf"), &bytes)?; (true, None) },
        Err(error) => (false, Some(error)),
    };
    let mut artifacts = Vec::new();
    for filename in ["document.tex", "document.log", "document.worker.log", "document.pdf", "result.pdf"] {
        let file = directory.join(filename);
        if file.is_file() {
            let bytes = fs::read(file)?;
            artifacts.push(json!({"file":filename,"bytes":bytes.len(),"sha256":digest(&bytes)}));
        }
    }
    let report = json!({"devOnly":true,"acceptance":false,"requestedCase":"termes-smallcaps",
        "sourceVersion":"termes-smallcaps-calibration-v4","executedManagedCompiles":1,
        "actualAccepted":accepted,"elapsedMs":elapsed_ms,"rawError":raw_error,"artifacts":artifacts,
        "physicalGlyphAssertionsExecuted":false,"unicodeParserAssertionsExecuted":false,
        "scope":"Only compile result and original artifacts; independent 4-face GSUB/outlines, point size, MuPDF/pypdf Unicode and ActualText/ToUnicode checks remain required."});
    create_new(&workspace.join("report.json"), &serde_json::to_vec_pretty(&report)?)?;
    println!("{}", json!({"report":workspace.join("report.json"),"actualAccepted":accepted,"acceptance":false}));
    if !accepted { return Err("Managed calibration compile failed; original source/log and raw error retained".into()); }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let data = existing_directory(args.next().map(PathBuf::from).ok_or("Owned data path required")?)?;
    let assets = existing_directory(args.next().map(PathBuf::from).ok_or("Owned assets path required")?)?;
    let workspace = args.next().map(PathBuf::from).ok_or("New owned workspace path required")?;
    let selector = args.next().map(|value| value.into_string()
        .map_err(|_| "Invalid case selector")).transpose()?;
    if args.next().is_some() || selector.as_deref().is_some_and(|value| !matches!(value, "ascii11-nnv-family" | "termes-smallcaps"))
        || !workspace.is_absolute() || workspace.exists() {
        return Err("Use data, assets, new absolute workspace, and optionally ascii11-nnv-family or termes-smallcaps; never reuse outputs".into());
    }
    existing_directory(workspace.parent().ok_or("Workspace parent required")?.to_owned())?;
    fs::create_dir(&workspace)?;
    if selector.as_deref() == Some("termes-smallcaps") { return probe_termes_smallcaps(&data, &assets, &workspace); }
    let font_sha = digest(FONT);
    if font_sha != "7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954" {
        return Err("Core font bytes changed".into());
    }
    let relative = format!("document.assets/readmd-core-{font_sha}.ttf");
    let input = json!({"devOnly":true,"acceptance":false,"data":data,"assets":assets,
        "fontSha256":font_sha,"fontBytes":FONT.len(),"licenseSha256":digest(LICENSE),
        "licenseBytes":LICENSE.len(),"managedBudgetMs":120000,"noReadinessRequest":true,
        "sourceVersion":"nfss-family-calibration-v2","requestedCase":selector,
        "originalThreeCaseSourceSha256":"6fdbd36258a10855d36aaf103d1d1d6c141b323d677275d8eecb5aa9b015006a",
        "sourceExampleSha256":digest(include_bytes!("publication_tex_font_probe.rs"))});
    create_new(&workspace.join("inputs.json"), &serde_json::to_vec_pretty(&input)?)?;
    let mut cases = Vec::new();
    for (name, ascii_letters, nnv, nfss_family, expected_accepted) in [
        ("all12-current", false, false, false, false),
        ("all12-nnv", false, true, false, false),
        ("ascii11-nnv", true, true, false, true),
        ("ascii11-nnv-family", true, true, true, true),
    ] {
        if selector.as_deref().is_some_and(|value| value != name) { continue; }
        let directory = workspace.join(name);
        fs::create_dir(&directory)?;
        fs::create_dir(directory.join("document.assets"))?;
        create_new(&directory.join(&relative), FONT)?;
        create_new(&directory.join("document.assets/DejaVuSans.LICENSE.txt"), LICENSE)?;
        let tokens = relative.chars().map(|character| {
            let category = if ascii_letters && character.is_ascii_alphabetic() { 11 } else { 12 };
            format!("\\char_generate:nn {{{}}}{{{category}}}", character as u32)
        }).collect::<String>();
        let invocation = if nfss_family {
            "\\exp_args:NNV \\newfontface \\readmdcorefallback \\readmdcorefontfile [NFSSFamily=readmdcorefallback]\n\\ExplSyntaxOff\n"
        } else if nnv {
            "\\exp_args:NNV \\newfontface \\readmdcorefallback \\readmdcorefontfile\n\\ExplSyntaxOff\n"
        } else {
            "\\ExplSyntaxOff\n\\newfontface\\readmdcorefallback{\\readmdcorefontfile}\n"
        };
        let source = format!("\\documentclass{{article}}\n\\usepackage{{fontspec}}\n\\ExplSyntaxOn\n\\cs_new:Npx \\readmdcorefontfile {{ {tokens} }}\n{invocation}\\begin{{document}}\nCore fallback: {{\\readmdcorefallback 😀}}.\n\\end{{document}}\n");
        create_new(&directory.join("document.tex"), source.as_bytes())?;
        let started = Instant::now();
        let actual = latex_compile::compile_managed(&source, &directory, "xelatex",
            Duration::from_secs(120), &data, &assets);
        let elapsed_ms = started.elapsed().as_millis();
        let (accepted, raw_error, pdf_sha, pdf_bytes) = match actual {
            Ok(bytes) => {
                create_new(&directory.join("result.pdf"), &bytes)?;
                (true, None, Some(digest(&bytes)), Some(bytes.len()))
            }
            Err(error) => (false, Some(error), None, None),
        };
        let mut artifacts = Vec::new();
        for filename in ["document.tex", "document.log", "document.worker.log", "document.pdf", "result.pdf"] {
            let file = directory.join(filename);
            if file.is_file() {
                let bytes = fs::read(file)?;
                artifacts.push(json!({"file":filename,"bytes":bytes.len(),"sha256":digest(&bytes)}));
            }
        }
        let fontspec_error = fs::read(directory.join("document.log")).ok()
            .is_some_and(|bytes| String::from_utf8_lossy(&bytes).contains("Package fontspec Error"));
        let matches = accepted == expected_accepted && (expected_accepted || fontspec_error);
        let case = json!({"case":name,"expectedAccepted":expected_accepted,"actualAccepted":accepted,
            "matchesExpected":matches,"fontspecErrorObserved":fontspec_error,"elapsedMs":elapsed_ms,"rawError":raw_error,
            "pdfSha256":pdf_sha,"pdfBytes":pdf_bytes,"artifacts":artifacts});
        create_new(&directory.join("result.json"), &serde_json::to_vec_pretty(&case)?)?;
        println!("{case}");
        cases.push(case);
    }
    let matched = cases.iter().all(|case| case["matchesExpected"] == true);
    let summary = json!({"devOnly":true,"acceptance":false,"cases":cases,"allMatchedExpected":matched,
        "sourceVersion":"nfss-family-calibration-v2","requestedCase":selector,"executedCases":cases.len(),
        "scope":"Only the requested case, or all four when no selector is given, through unchanged managed120s compilation, sandbox and complete lock validation. Original third-case hypothesis remains unchanged. No product/native-suite certification."});
    create_new(&workspace.join("report.json"), &serde_json::to_vec_pretty(&summary)?)?;
    println!("{}", json!({"report":workspace.join("report.json"),"allMatchedExpected":matched,"acceptance":false}));
    if !matched { return Err("Observed results differ from the calibration hypothesis; all actual results were retained".into()); }
    Ok(())
}
