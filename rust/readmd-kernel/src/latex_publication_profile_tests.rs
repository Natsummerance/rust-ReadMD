//! Actual standalone TeX files and native source contracts. Engine pagination,
//! physical glyphs and Word/reader behavior require separate native evidence.
use super::*;
use serde_json::{json, Value};
use std::fs;

fn options() -> Value {
    json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},
        "typography":{"font":"Arial","size":11,"lineHeight":1.1,"spacing":0,"firstLineIndent":0,"align":"left"},
        "page":{"size":"A4","marginTop":20,"marginRight":18,"marginBottom":20,"marginLeft":18},
        "header":{"distance":8,"size":8},"footer":{"distance":8,"size":9,"align":"right","pageNumbers":true}})
}

fn export(source: &str, options: &Value) -> (tempfile::TempDir, String, crate::mdexport::ExportResult) {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("Source.md");
    let target = directory.path().join("Paper.tex");
    fs::write(&input, source).unwrap();
    let before = options.clone();
    let result = crate::mdexport::export_tex(source, directory.path().to_str().unwrap(),
        target.to_str().unwrap(), options, "Source").unwrap();
    assert!(result.ok);
    assert_eq!(fs::read(input).unwrap(), source.as_bytes(), "Projection must not rewrite the source");
    assert_eq!(options, &before, "Projection must not rewrite export options");
    let tex = fs::read_to_string(target).unwrap();
    (directory, tex, result)
}

fn visible(tex: &str) -> &str {
    tex.split_once("\\begin{document}\n").unwrap().1
}

#[test]
fn shared_tex_front_none_hides_metadata_but_preserves_native_section_order() {
    let source = "---\r\ntitle: Hidden title\r\nauthor: Hidden author\r\n---\r\n\r\n<!-- readmd-section:v1 {\"id\":\"front\",\"numbering\":{\"format\":\"lowerRoman\",\"start\":1}} -->\r\n\r\n# Preface\r\n\r\nFRONT_ONLY\r\n\r\n<!-- readmd-section:v1 {\"id\":\"main\",\"page\":{\"columns\":2},\"numbering\":{\"format\":\"decimal\",\"start\":1}} -->\r\n\r\n# Main\r\n\r\nMAIN_ONLY\r\n\r\n<!-- readmd-section:v1 {\"id\":\"wide\",\"page\":{\"orientation\":\"landscape\",\"columns\":1}} -->\r\n\r\nWIDE_ONLY\r\n\r\n<!-- readmd-section:v1 {\"id\":\"odd\",\"break\":\"oddPage\",\"page\":{\"orientation\":\"portrait\"},\"numbering\":{\"format\":\"upperLetter\",\"start\":1}} -->\r\n\r\nODD_ONLY\r\n\r\n<!-- readmd-section:v1 {\"id\":\"even\",\"break\":\"evenPage\",\"numbering\":{\"format\":\"decimal\",\"start\":7}} -->\r\n\r\nEVEN_ONLY\r\n";
    let (_directory, tex, _) = export(source, &options());
    let body = visible(&tex);
    assert!(!body.contains("Hidden title") && !body.contains("Hidden author"));
    assert!(!tex.contains("\\maketitle") && !tex.contains("\\today") && !tex.contains("\\begin{titlepage}"));
    let mut previous = 0;
    for marker in ["FRONT\\_ONLY", "MAIN\\_ONLY", "WIDE\\_ONLY", "ODD\\_ONLY", "EVEN\\_ONLY"] {
        assert_eq!(body.matches(marker).count(), 1);
        let position = body.find(marker).unwrap();
        assert!(position > previous); previous = position;
    }
    assert!(body.find("\\newgeometry").unwrap() < body.find("FRONT\\_ONLY").unwrap());
    assert!(body.find("\\newgeometry").unwrap() < body.find("\\color[HTML]").unwrap(),
        "No colour whatsit may precede the first geometry clearpage");
    for boundary in body.split("\\clearpage\n").skip(1) {
        if let (Some(geometry),Some(colour))=(boundary.find("\\newgeometry"),boundary.find("\\color[HTML]")) {
            assert!(geometry<colour,"The next section must set geometry before emitting colour");
        }
    }
    assert_eq!(body.matches("\\begin{multicols}{2}").count(), 1);
    assert!(body.contains("layoutwidth=297mm,layoutheight=210mm"));
    assert!(body.contains("\\renewcommand{\\thepage}{\\roman{page}}"));
    assert!(body.contains("\\renewcommand{\\thepage}{\\readmdAlph{\\value{page}}}"));
    assert!(body.contains("\\ifodd\\value{readmdphysicalpage}\\else\\hbox{}\\thispagestyle{empty}\\newpage\\fi"));
    assert!(body.contains("\\ifodd\\value{readmdphysicalpage}\\hbox{}\\thispagestyle{empty}\\newpage\\fi"));
    assert!(body.contains("\\setcounter{page}{7}"));
    assert!(!body.contains("readmd-section:v1"));
}

#[test]
fn letter_page_counters_share_the_expandable_office_formatter_without_numbered_headings() {
    for (format,helper) in [("upperLetter","readmdAlph"),("lowerLetter","readmdalph")] {
        let source=format!("<!-- readmd-section:v1 {{\"id\":\"letters52\",\"numbering\":{{\"format\":\"{format}\",\"start\":52}}}} -->\n\n# Ordinary heading\n\nFIRSTLETTERPAGE\n\n<!-- readmd-section:v1 {{\"id\":\"letters53\",\"break\":\"nextPage\",\"numbering\":{{\"format\":\"{format}\",\"start\":53}}}} -->\n\nSECONDLETTERPAGE\n");
        let (_directory,tex,_)=export(&source,&options());let (preamble,body)=tex.split_once("\\begin{document}\n").unwrap();
        assert_eq!(preamble.matches("\\cs_new:Npn \\readmdAlph").count(),1);
        assert!(preamble.contains("\\prg_replicate:nn")&&preamble.contains("\\int_div_truncate:nn")&&preamble.contains("\\int_mod:nn"));
        assert!(!preamble.contains("\\newcounter{readmdh1}"));
        assert!(body.contains(&format!("\\renewcommand{{\\thepage}}{{\\{helper}{{\\value{{page}}}}}}")));
        assert!(body.contains("\\setcounter{page}{52}")&&body.contains("\\setcounter{page}{53}"));
        assert!(body.contains("\\thepage")&&body.contains("FIRSTLETTERPAGE")&&body.contains("SECONDLETTERPAGE"));
        assert!(!body.contains("readmdnumberedheadingmark"));
    }
}

#[test]
fn shared_tex_paper_and_book_project_the_shared_front_matter_once() {
    let source = "---\ntitle: Shared title\nsubtitle: Shared subtitle\nlanguage: en-US\nauthors:\n  - id: ada\n    name: Ada Example\n    affiliationIds: [lab]\ninstitutions:\n  - id: lab\n    name: Example Institute\nabstracts:\n  - text: Exact abstract text.\n    language: en-US\npublisher: Example Publisher\n---\n\nBODY_SENTINEL\n";
    for mode in ["paper", "book"] {
        let mut style = options();
        style["frontMatter"]["mode"] = json!(mode);
        style["cover"] = json!({"enabled":true,"title":"Must not duplicate shared title"});
        let (_directory, tex, _) = export(source, &style);
        let body = visible(&tex);
        for value in ["Shared title", "Shared subtitle", "Ada Example", "Example Institute", "BODY\\_SENTINEL"] {
            assert_eq!(body.matches(value).count(), 1, "{mode}: {value}");
        }
        assert!(!tex.contains("\\maketitle") && !tex.contains("\\today"));
        assert!(!body.contains("\\begin{titlepage}") && !body.contains("Must not duplicate shared title"));
        if mode == "paper" { assert!(body.contains("Exact abstract text.")); }
        else { assert!(body.contains("Example Publisher")); }
    }
}

#[test]
fn shared_tex_cover_is_explicit_and_does_not_invent_a_date() {
    let mut style = options();
    style["cover"] = json!({"enabled":true,"title":"Cover & title","subtitle":"Cover subtitle","date":"","align":"right"});
    style["page"] = json!({"size":"Letter","marginTop":21,"marginRight":19,"marginBottom":22,"marginLeft":20});
    let (_directory, tex, _) = export("---\ntitle: Metadata title\nauthor: Cover Author\n---\n\nBODY_SENTINEL\n", &style);
    let body = visible(&tex);
    assert_eq!(body.matches("\\begin{titlepage}").count(), 1);
    assert_eq!(body.matches("Cover \\& title").count(), 1);
    let start=body.find("\\begin{titlepage}").unwrap();
    assert!(!body[..start].contains("\\color[HTML]"),"No colour whatsit may precede titlepage's own transition");
    let cover=&body[start..body.find("\\end{titlepage}").unwrap()];
    assert!(cover.contains("\\thispagestyle{empty}"));
    assert!(cover.contains("\\raggedleft\n\\vspace*{\\fill}"));
    assert!(body.contains("Cover subtitle") && body.contains("Cover Author"));
    assert!(!tex.contains("\\today") && !tex.contains("\\maketitle"));
    assert!(body.find("\\end{titlepage}").unwrap() < body.find("BODY\\_SENTINEL").unwrap());
    assert!(tex.contains("paperwidth=215.9mm,paperheight=279.4mm,left=20mm,right=19mm,top=21mm,bottom=22mm"));
}

#[test]
fn shared_tex_typography_and_unnumbered_heading_metrics_are_source_owned() {
    let mut style = options();
    style["typography"] = json!({"font":"Arial","size":12,"lineHeight":1.25,"spacing":4,"firstLineIndent":3,"align":"justify","color":"#123456","keepLines":true,"widowControl":false});
    style["headings"]["h2"] = json!({"size":17,"before":9,"after":5,"align":"right","bold":false,"color":"#654321"});
    style["tex"] = json!({"docClass":"book"});
    let (_directory, tex, result) = export("# Title\n\nFIRST_PARAGRAPH\n\n## Second\n\nSECOND_PARAGRAPH\n", &style);
    assert!(tex.contains("\\fontsize{12}{15}\\selectfont\\color[HTML]{123456}"));
    assert!(tex.contains("\\setlength{\\parindent}{3mm}\\setlength{\\parskip}{4pt}"));
    assert!(tex.contains("\\clubpenalty=0\\widowpenalty=0\\interlinepenalty=10000"));
    assert!(tex.contains("\\setcounter{secnumdepth}{-1}"));
    assert!(tex.contains("\\@startsection{subsection}{2}{0pt}{9pt}{5pt}{\\normalfont\\fontsize{17}{21.25}\\selectfont\\mdseries\\color[HTML]{654321}\\raggedleft}"));
    let body = visible(&tex);
    assert!(body.contains("\\csname readmdheading1\\endcsname{Title}"));
    assert!(body.contains("\\csname readmdheading2\\endcsname{Second}"));
    assert!(!body.contains("\\chapter{") && !body.contains("\\section{"));
    assert!(body.find("FIRST\\_PARAGRAPH").unwrap() < body.find("SECOND\\_PARAGRAPH").unwrap());
    let warnings = result.warns.unwrap();
    assert_eq!(warnings.iter().filter(|warning|warning.as_str()=="publication_font_substituted").count(), 1);
    assert!(!tex.contains("\\setmainfont{Arial}"), "Unavailable host fonts must not be certified as applied");
}

#[test]
fn shared_tex_heading_zero_gap_stays_a_block_and_page_break_is_explicit() {
    let mut style = options();
    style["headings"]["h4"] = json!({"after":0,"before":0,"pageBreakBefore":true,"keepNext":false});
    let (_directory, tex, _) = export("First paragraph.\n\n#### Fourth\n\nAfter fourth.\n", &style);
    assert!(tex.contains("\\@startsection{paragraph}{4}{0pt}{0pt}{1sp}"));
    assert!(visible(&tex).contains("\\newpage\n\\csname readmdheading4\\endcsname{Fourth}\\label{sec:fourth}\n\\readmdheadingfree\n"));
    assert!(visible(&tex).contains("After fourth."));
}

#[test]
fn shared_tex_implicit_first_section_consumes_global_page_and_running_style() {
    let mut style = options();
    style["page"] = json!({"size":"A5","orientation":"landscape","marginTop":20,"marginRight":16,"marginBottom":20,"marginLeft":15,"columns":2,"columnGap":6});
    style["header"]["text"] = json!("SHARED_HEADER");
    style["footer"]["numberFormat"] = json!("upperRoman");
    style["footer"]["start"] = json!(4);
    let (_directory, tex, _) = export("First body.\n\nSecond body.\n", &style);
    assert!(tex.contains("paperwidth=210mm,paperheight=148mm,left=15mm,right=16mm"));
    let body = visible(&tex);
    assert!(body.contains("layoutwidth=210mm,layoutheight=148mm"));
    assert!(body.contains("\\begin{multicols}{2}") && body.contains("\\setlength{\\columnsep}{6mm}"));
    assert!(body.contains("SHARED\\_HEADER"));
    assert!(body.contains("\\renewcommand{\\thepage}{\\Roman{page}}") && body.contains("\\setcounter{page}{4}"));
    assert_eq!(body.matches("\\hypertarget{readmd-section-").count(), 1);
    assert!(body.find("\\pagestyle{readmdsection0}").unwrap() < body.find("First body.").unwrap());
}

#[test]
fn shared_tex_continuous_columns_do_not_force_a_page_or_rewrite_current_running_style() {
    let source = "<!-- readmd-section:v1 {\"id\":\"main\",\"page\":{\"columns\":2}} -->\n\nFIRST_BODY\n\n<!-- readmd-section:v1 {\"id\":\"full\",\"break\":\"continuous\",\"page\":{\"columns\":1},\"header\":{\"linkPrevious\":false,\"text\":\"NEXT_HEADER\"}} -->\n\nFULL_BODY\n\n<!-- readmd-section:v1 {\"id\":\"resume\",\"break\":\"continuous\",\"page\":{\"columns\":2}} -->\n\nRESUMED_BODY\n";
    let (_directory, tex, _) = export(source, &options());
    let body = visible(&tex);
    let continuous = &body[body.find("FIRST\\_BODY").unwrap()..];
    assert!(!continuous.contains("\\newgeometry") && !continuous.contains("\\clearpage") && !continuous.contains("\\newpage"));
    assert_eq!(continuous.matches("\\AddToHookNext{shipout/after}").count(), 2);
    for index in [1, 2] {
        assert!(continuous.contains(&format!("\\ifdim\\pagetotal>0pt\\relax\\AddToHookNext{{shipout/after}}{{\\readmdapplypagestyle{{readmdsection{index}}}}}\\else\\pagestyle{{readmdsection{index}}}\\fi")));
    }
    assert!(!tex.contains("\\globaldefs"));
    assert!(continuous.contains("\\end{multicols}") && continuous.contains("\\begin{multicols}{2}"));
    assert_eq!(body.matches("FIRST\\_BODY").count(), 1);
    assert_eq!(body.matches("FULL\\_BODY").count(), 1);
    assert_eq!(body.matches("RESUMED\\_BODY").count(), 1);
}

#[test]
fn shared_tex_continuous_running_style_publishes_only_header_footer_macros() {
    let source = "FIRST_BODY\n\n<!-- readmd-section:v1 {\"id\":\"next\",\"break\":\"continuous\",\"header\":{\"linkPrevious\":false,\"text\":\"NEXT_HEADER\",\"evenText\":\"EVEN_HEADER\"},\"footer\":{\"linkPrevious\":false,\"text\":\"NEXT_FOOTER\",\"evenText\":\"EVEN_FOOTER\"}} -->\n\nNEXT_BODY\n";
    let mut style = options();
    style["page"]["differentOddEven"] = json!(true);
    let (_directory, tex, _) = export(source, &style);
    let (_, helper) = tex.split_once("\\newcommand{\\readmdapplypagestyle}[1]{%\n").unwrap();
    let (helper, _) = helper.split_once("\\endgroup\n}\n").unwrap();
    assert!(helper.starts_with("\\begingroup\n\\pagestyle{#1}%\n"));
    let published = helper.lines().filter(|line| line.contains("\\global")).collect::<Vec<_>>();
    let expected = [
        "@oddhead", "@evenhead", "@oddfoot", "@evenfoot", "f@nch@oddhead", "f@nch@oddfoot",
        "f@nch@olh", "f@nch@och", "f@nch@orh", "f@nch@elh", "f@nch@ech", "f@nch@erh",
        "f@nch@olf", "f@nch@ocf", "f@nch@orf", "f@nch@elf", "f@nch@ecf", "f@nch@erf",
        "headrulewidth", "footrulewidth",
    ].map(|name| format!("\\global\\let\\{name}\\{name}"));
    assert_eq!(published, expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "Only the allowlisted running-matter definitions may escape shipout scope");
    for temporary in ["\\globaldefs", "\\escapechar", "\\catcode", "\\global\\def", "\\global\\let\\temp", "\\global\\let\\@auxout"] {
        assert!(!helper.contains(temporary), "Temporary TeX state must remain local: {temporary}");
    }
    assert!(tex.contains("\\fancyhead[L]{\\fontsize{8}{9.6}\\selectfont "));
    for value in ["NEXT\\_HEADER", "EVEN\\_HEADER", "NEXT\\_FOOTER", "EVEN\\_FOOTER"] {
        assert!(tex.contains(value), "Running matter is preserved: {value}");
    }
    let body = visible(&tex);
    assert_eq!(body.matches("FIRST\\_BODY").count(), 1);
    assert_eq!(body.matches("NEXT\\_BODY").count(), 1);
}

#[test]
fn shared_tex_xetex_paper_dimensions_persist_until_after_odd_and_even_padding() {
    let source = "<!-- readmd-section:v1 {\"id\":\"wide\",\"page\":{\"orientation\":\"landscape\"}} -->\n\nWIDE_BODY\n\n<!-- readmd-section:v1 {\"id\":\"odd\",\"break\":\"oddPage\",\"page\":{\"orientation\":\"portrait\"}} -->\n\nODD_BODY\n\n<!-- readmd-section:v1 {\"id\":\"even\",\"break\":\"evenPage\",\"page\":{\"orientation\":\"landscape\"}} -->\n\nEVEN_BODY\n";
    let (_directory, tex, _) = export(source, &options());
    let body = visible(&tex);
    let engine_dimensions = "\\ifPDFTeX\\pdfpagewidth=\\paperwidth\\pdfpageheight=\\paperheight\\else\\ifLuaTeX\\pagewidth=\\paperwidth\\pageheight=\\paperheight\\else\\ifXeTeX\\pdfpagewidth=\\paperwidth\\pdfpageheight=\\paperheight\\else\\special{papersize=";
    assert_eq!(body.matches(engine_dimensions).count(), 3,
        "Every real section must update persistent XeTeX page dimensions, not only insert a one-page special");
    let wide = "\\setlength{\\paperwidth}{297mm}\\setlength{\\paperheight}{210mm}";
    let portrait = "\\setlength{\\paperwidth}{210mm}\\setlength{\\paperheight}{297mm}";
    assert_eq!(body.matches(wide).count(), 2);
    assert_eq!(body.matches(portrait).count(), 1);
    let odd = "\\clearpage\n\\ifodd\\value{readmdphysicalpage}\\else\\hbox{}\\thispagestyle{empty}\\newpage\\fi\n";
    let even = "\\clearpage\n\\ifodd\\value{readmdphysicalpage}\\hbox{}\\thispagestyle{empty}\\newpage\\fi\n";
    for (boundary, dimensions) in [(odd, portrait), (even, wide)] {
        let (_, after) = body.split_once(boundary).unwrap();
        assert!(after.starts_with("\\newgeometry{"));
        assert!(after.find(dimensions).unwrap() < after.find(engine_dimensions).unwrap());
    }
    assert!(body.find(wide).unwrap() < body.find(odd).unwrap());
    assert!(body.find(portrait).unwrap() < body.find(even).unwrap());
    for marker in ["WIDE\\_BODY", "ODD\\_BODY", "EVEN\\_BODY"] {
        assert_eq!(body.matches(marker).count(), 1);
    }
    assert!(!body.contains("\\AddToHookNext{shipout/before}"),
        "Paper dimensions belong to the section and must persist for arbitrary page counts");
}

#[test]
fn shared_tex_hidden_smp_metadata_does_not_materialize_a_font() {
    let source = "---\ntitle: Hidden 中文 😀\nauthor: Hidden author 中文 😀\n---\n\nVisible ASCII.\n";
    let (directory, tex, _) = export(source, &options());
    assert!(!tex.contains("readmdcorefallback") && !directory.path().join("Paper.assets").exists());
    assert!(!visible(&tex).contains('😀') && !visible(&tex).contains("中文"));
    assert!(!tex.contains("\\usepackage[UTF8,fontset=fandol]{ctex}"));
    let mut style = options(); style["frontMatter"]["mode"] = json!("paper");
    let (directory, tex, _) = export(source, &style);
    assert!(visible(&tex).contains("\\readmdunicode{😀}"));
    assert!(tex.contains("\\usepackage[UTF8,fontset=fandol]{ctex}"));
    assert!(tex.contains("[NFSSFamily=readmdcorefallback]"));
    assert!(fs::read_dir(directory.path().join("Paper.assets")).unwrap().count() >= 2);
}

#[test]
fn shared_tex_legacy_direct_writer_keeps_title_class_and_date_contract() {
    let doc = crate::md_ast::parse("# Chapter\n\nLegacy paragraph.\n");
    let tex = render_document(&doc, &LatexOptions {title:"Legacy title".into(),doc_class:"book".into(),..Default::default()}, &|_|None);
    assert!(tex.contains("\\maketitle") && tex.contains("\\date{\\today}"));
    assert!(tex.contains("\\chapter{Chapter}") && tex.contains("\\usepackage[margin=2.5cm]{geometry}"));
    assert!(!tex.contains("Shared publication typography") && !tex.contains("\\newgeometry"));
}

#[test]
fn shared_tex_explicit_legacy_knobs_fill_only_missing_canonical_fields() {
    let style=json!({"frontMatter":{"mode":"none"},"tex":{"fontSize":"12pt","paperSize":"letterpaper","margin":"1in","toc":true}});
    let (_directory,tex,_)=export("# Body\n\nVisible text.\n",&style);
    assert!(tex.contains("\\fontsize{12}{"));
    assert!(tex.contains("paperwidth=215.9mm,paperheight=279.4mm,left=25.4mm,right=25.4mm,top=25.4mm,bottom=25.4mm"));
    assert!(visible(&tex).contains("\\tableofcontents"));
    assert!(!tex.contains("\\maketitle")&&!tex.contains("\\today"));

    // Precedence is based on the sparse request, not presence in sanitize's
    // completed defaults. A single canonical margin does not mask the others.
    let style=json!({"frontMatter":{"mode":"none"},"typography":{"size":10},"page":{"size":"A5","marginLeft":16},"toc":{"enabled":false},
        "latex":{"fontSize":"12pt","paperSize":"letterpaper","margin":"2cm","toc":true}});
    let (_directory,tex,_)=export("Body without cover.\n",&style);
    assert!(tex.contains("\\fontsize{10}{"));
    assert!(tex.contains("paperwidth=148mm,paperheight=210mm,left=16mm,right=20mm,top=20mm,bottom=20mm"));
    assert!(!visible(&tex).contains("\\tableofcontents"));
}

#[test]
fn shared_tex_invalid_legacy_knobs_are_not_swallowed_or_written_as_tex() {
    let directory=tempfile::tempdir().unwrap();let target=directory.path().join("Paper.tex");
    fs::write(&target,b"Previous complete output").unwrap();
    let mut invalid=vec![json!({"tex":{"fontSize":"14pt"}}),json!({"latex":{"paperSize":"host-default"}})];
    for margin in ["0mm","61mm","6.1cm","1e1mm","NaNmm","1cm}\\input{private}","1pt"]{
        invalid.push(json!({"tex":{"margin":margin}}));
    }
    for style in invalid{
        let error=crate::mdexport::export_tex("Visible 😀.",directory.path().to_str().unwrap(),target.to_str().unwrap(),&style,"Source").unwrap_err();
        assert_eq!(error,"latex_invalid_options");
        assert_eq!(fs::read(&target).unwrap(),b"Previous complete output");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(),1,"Invalid values must fail before copying image/font resources");
    }
}

#[test]
fn shared_tex_invalid_section_preserves_existing_output_and_source() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("Paper.tex");
    let input = directory.path().join("Source.md");
    let source = "<!-- readmd-section:v1 {\"id\":\"main\",\"page\":{\"columns\":2}} -->\n\nVisible body.\n\n<!-- readmd-section:v1 {\"id\":\"bad\",\"break\":\"continuous\",\"page\":{\"orientation\":\"landscape\"}} -->\n\nMust not replace previous result.\n";
    fs::write(&target,b"Previous complete output").unwrap(); fs::write(&input,source).unwrap();
    let error = crate::mdexport::export_tex(source,directory.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Source").unwrap_err();
    assert_eq!(error,"publication_section_continuous_page");
    assert_eq!(fs::read(target).unwrap(),b"Previous complete output");
    assert_eq!(fs::read(input).unwrap(),source.as_bytes());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(),2);
}
