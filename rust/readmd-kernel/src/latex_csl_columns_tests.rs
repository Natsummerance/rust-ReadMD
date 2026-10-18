//! Original synthetic source tests. Actual managed TeX/PDF checks remain separate.
use super::*;
use crate::publication_rich::{RichBibliography, RichBibliographyItem, RichDisplay, RichFormat,
    RichFontStyle, RichFontVariant, RichFontWeight, RichLayoutNode as L, RichNode};

fn writer() -> Writer<'static> {
    Writer { out: String::new(), images: &|_| None, footnotes: HashMap::new(), fn_stack: Vec::new(),
        bib_engine: String::new(), chapter_headings: false, publication_style: None,
        core_text_fallback: false, core_text_used: false, csl_columns_text: false,
        paragraphs: paragraphs::State::default() }
}
fn inline(text: &str) -> L { L::Inline { children: vec![RichNode::text(text)] } }
fn group(display: RichDisplay, children: Vec<RichNode>) -> RichNode {
    RichNode::Group { children, display: Some(display), entry_index: Some(0) }
}

#[test]
fn csl_columns_emergency_wrap_is_conditional_and_preserves_width_conservation() {
    let source = CSL_COLUMN_MACROS;
    assert!(source.contains("\\ifdim\\wd\\readmdcslwordbox>\\linewidth\\relax"));
    assert!(source.contains("\\PackageError{ReadMD}{publication_layout_geometry}{A CSL character exceeds its column}"));
    let minimum = source.find("\\ifdim\\readmdcslleftwidth<.5em\\readmdcslleftwidth=.5em").unwrap();
    let cap = source.find("\\ifdim\\readmdcslleftwidth>.35\\linewidth").unwrap();
    let remainder = source.find("\\readmdcslrightwidth=\\dimexpr\\linewidth-\\readmdcslleftwidth-.75em\\relax").unwrap();
    assert!(minimum < cap && cap < remainder, "Actual right width must use the final, capped left width");
    assert!(source.contains("CSL label column is too narrow"));
    assert!(source.contains("\\hsize=\\readmdcslleftwidth\\linewidth=\\hsize"));
    assert!(source.contains("\\hsize=\\readmdcslrightwidth\\linewidth=\\hsize"));
    assert!(source.contains("\\vsplit\\readmdcslleft") && source.contains("\\vsplit\\readmdcslright"));
    assert!(!source.contains("\\resizebox") && !source.contains("\\clip"));
}

#[test]
fn csl_columns_nested_left_and_right_enable_wrap_and_restore_outer_context() {
    let flow = vec![L::Columns {
        left: vec![L::Columns { left: vec![inline("NLEFT100")], right: vec![inline("LEFTBODY100")] }],
        right: vec![L::Columns { left: vec![inline("NRIGHT100")], right: vec![inline("BODYB ordinary words ENDB")] }],
    }];
    let original = flow.clone(); let mut writer = writer();
    let source = writer.rich_flow(&flow, None, false);
    assert_eq!(source.matches("\\readmdcslcolumns{").count(), 3);
    for token in ["NLEFT100", "LEFTBODY100", "NRIGHT100", "BODYB", "ENDB"] {
        assert!(source.contains(&format!("\\readmdcslword{{{token}}}")), "{token}");
    }
    assert!(!writer.csl_columns_text, "Columns must restore the caller's text context");
    assert_eq!(writer.inlines(&[Inline::Text("AFTER BODY ordinary words".into())]), "AFTER BODY ordinary words");
    assert_eq!(flow, original, "Layout/source identity must not be mutated for wrapping");
}

#[test]
fn csl_columns_long_ascii_keeps_complete_natural_token_and_invisible_breaks() {
    let token = "LONGUNSPACEDLABEL".repeat(8) + "100";
    let mut writer = writer(); writer.csl_columns_text = true;
    let source = writer.inline_text(&token);
    assert!(source.starts_with(&format!("\\readmdcslword{{{token}}}{{")));
    assert_eq!(source.matches("\\readmdcslchar{").count(), token.len());
    assert_eq!(source.matches("\\allowbreak{}").count(), token.len() - 1);
    assert!(!source.contains("\\discretionary{-}") && !source.contains("\\scalebox"));
    assert!(source.ends_with("\\penalty1000\\relax "));
    // A one-character unit that cannot physically fit must fail rather than
    // escape the cell; it still reaches the measured glyph guard.
    assert!(writer.inline_text("W").contains("\\readmdcslword{W}{\\readmdcslchar{W}}"));
}

#[test]
fn csl_columns_ascii_safety_does_not_turn_tex_controls_into_executable_tokens() {
    let input = r"\write18{payload}&^~_%#";
    let mut writer = writer(); writer.csl_columns_text = true;
    let source = writer.inline_text(input);
    assert!(source.contains(&format!("\\readmdcslword{{{}}}", escape_text(input))));
    assert!(!source.contains(r"\write18{"));
    for escaped in [r"\textbackslash{}", r"\{", r"\}", r"\&", r"\textasciicircum{}", r"\textasciitilde{}", r"\_", r"\%", r"\#"] {
        assert!(source.contains(escaped), "Escaping changed for {escaped}");
    }
}

#[test]
fn csl_columns_non_ascii_tokens_combining_and_emoji_are_not_split_into_scalars() {
    let mut writer = writer(); writer.csl_columns_text = true;
    let input = "café 中文 👩\u{200d}🔬 A\u{301} 𝛼";
    assert_eq!(writer.inline_text(input), escape_text(input));
    assert!(!writer.inline_text(input).contains("\\allowbreak"));
    let math = writer.inlines(&[Inline::Math("\\frac{a}{b}".into())]);
    assert_eq!(math, "\\(\\frac{a}{b}\\)");
}

#[test]
fn csl_columns_format_and_url_ownership_survive_emergency_wrap() {
    let format = RichFormat { font_style: RichFontStyle::Italic, font_weight: RichFontWeight::Bold,
        font_variant: RichFontVariant::SmallCaps, ..Default::default() };
    let href = "https://example.invalid/original/unchanged?x=1&y=2";
    let flow = vec![L::Columns { left: vec![L::Inline { children: vec![RichNode::Text { text: "NLEFT100".into(), format }] }],
        right: vec![L::Link { href: href.into(), children: vec![inline("VISIBLELINKLONGWORD")] }] }];
    let mut writer = writer(); let source = writer.rich_flow(&flow, None, false);
    assert!(source.contains("\\readmdsmallcaps{\\textbf{\\textit{\\readmdcslword{NLEFT100}"));
    assert!(source.contains(&format!("\\href{{{}}}{{\\readmdcslword{{VISIBLELINKLONGWORD}}", escape_url(href))));
    writer.csl_columns_text = true;
    let automatic = writer.inlines(&[Inline::Link { href: href.into(), title: String::new(), children: vec![Inline::Text(href.into())] }]);
    assert!(automatic.starts_with(&format!("\\href{{{}}}", escape_url(href))));
    assert!(!automatic.starts_with("\\url{"), "Display wrapping must not change the link target or use an unbreakable URL shortcut");
}

#[test]
fn csl_columns_preferred_measurement_remains_natural_and_rendered_order_is_complete() {
    let rich = vec![group(RichDisplay::Block, vec![RichNode::text("NHEAD100")]),
        group(RichDisplay::LeftMargin, vec![group(RichDisplay::LeftMargin, vec![RichNode::text("NLEFT100")]),
            group(RichDisplay::RightInline, vec![RichNode::text("LEFTBODY100")])]),
        group(RichDisplay::RightInline, vec![group(RichDisplay::LeftMargin, vec![RichNode::text("NRIGHT100")]),
            group(RichDisplay::RightInline, vec![RichNode::text("BODYB ordinary words ENDB")])]),
        group(RichDisplay::Block, vec![RichNode::text("NTAIL100")])];
    let bib = RichBibliography { title: "References".into(), items: vec![RichBibliographyItem {key: "b".into(), rich,
        first_field: Vec::new()}], hanging_indent: false, line_spacing: 1, entry_spacing: 0, source_span: None };
    let doc = Document {blocks: vec![Block::Paragraph(vec![Inline::Text("BEFORE BODY".into())]), Block::Bibliography(bib),
        Block::Paragraph(vec![Inline::Text("AFTER BODY".into())])], ..Default::default()};
    let source = render_document(&doc, &LatexOptions {bib_engine: String::new(), ..Default::default()}, &|_| None);
    let measures = source.lines().filter(|line| line.starts_with("\\settowidth{\\readmdbibmeasure}")).collect::<Vec<_>>();
    assert!(!measures.is_empty());
    assert!(measures.iter().all(|line| !line.contains("\\readmdcslword") && !line.contains("\\allowbreak")));
    let body = source.split_once("\\begin{document}").unwrap().1;
    let mut cursor = 0;
    for token in ["BEFORE BODY", "NHEAD100", "NLEFT100", "LEFTBODY100", "NRIGHT100", "BODYB", "ENDB", "NTAIL100", "AFTER BODY"] {
        let found = body[cursor..].find(token).unwrap_or_else(|| panic!("Missing or reordered {token}")); cursor += found + token.len();
    }
    assert_eq!(body.matches("\\readmdcslcolumns{").count(), 3);
    assert!(!body.contains("\\readmdcslword{BEFORE") && !body.contains("\\readmdcslword{AFTER"));
}


#[test]
fn csl_simple_first_field_uses_bounded_columns_without_losing_full_width_entries() {
    let label = "LONGUNSPACEDLABEL".repeat(8);
    let bib = RichBibliography { title: "References".into(), items: vec![
        RichBibliographyItem { key: "labelled".into(), first_field: vec![RichNode::text(&label)],
            rich: vec![RichNode::text("RIGHTBODY original text END")] },
        RichBibliographyItem { key: "plain".into(), first_field: vec![],
            rich: vec![RichNode::text("FULL WIDTH original entry")] },
    ], hanging_indent: true, line_spacing: 2, entry_spacing: 1, source_span: None };
    let doc = Document { blocks: vec![Block::Bibliography(bib),
        Block::Paragraph(vec![Inline::Text("AFTER ordinary paragraph".into())])], ..Default::default() };
    let source = render_document(&doc, &LatexOptions {bib_engine:String::new(), ..Default::default()}, &|_|None);
    let body = source.split_once("\\begin{document}").unwrap().1;
    assert_eq!(body.matches("\\readmdcslcolumns{").count(),1);
    assert!(!body.contains("\\makebox[\\readmdbiblabelwidth]"));
    assert!(body.contains(&format!("\\readmdcslword{{{label}}}")));
    assert!(body.contains("\\readmdcslword{RIGHTBODY}"));
    assert!(body.contains("FULL WIDTH original entry"));
    assert!(body.contains("\\vspace{1\\baselineskip}"));
    assert!(body.contains("AFTER ordinary paragraph"));
    assert!(!body.contains("\\readmdcslword{AFTER}"));
    assert_eq!(source.matches("\\newlength{\\readmdbiblabelwidth}").count(),1);
}


#[test]
fn csl_columns_ragged_cells_keep_emergency_breaks_out_of_the_structural_gutter() {
    // Both sides, including recursive calls, share the same bounded cell
    // macro. Native TeX must be able to accept a short non-final line.
    let cells = CSL_COLUMN_MACROS.lines().filter(|line| line.contains("=\\vbox{\\hsize=\\readmdcsl")).collect::<Vec<_>>();
    assert_eq!(cells.len(), 2);
    for cell in cells {
        assert!(cell.contains("\\rightskip=0pt plus1fil\\parfillskip=0pt plus1fil"));
        assert!(cell.contains("\\linewidth=\\hsize\\leftskip=0pt"));
        assert!(cell.ends_with("\\noindent#1\\par}%") || cell.ends_with("\\noindent#2\\par}%"));
    }
    assert!(CSL_COLUMN_MACROS.contains("\\hspace{.75em}"));
    assert!(CSL_COLUMN_MACROS.contains("\\vsplit\\readmdcslleft"));
    assert!(CSL_COLUMN_MACROS.contains("\\vsplit\\readmdcslright"));

    let split_label = vec![L::Group { entry_index: Some(7), children: vec![inline("NLEFT"), inline("100")] }];
    let flow = vec![L::Columns {
        left: vec![L::Columns { left: split_label.clone(), right: vec![inline("LEFTBODY100")] }],
        right: vec![L::Columns { left: vec![inline("NRIGHT100")], right: vec![inline("RIGHTBODY whole office ENDB")] }],
    }];
    let before = flow.clone(); let mut writer = writer();
    let output = writer.rich_flow(&flow, None, false);
    assert_eq!(output.matches("\\readmdcslcolumns{").count(), 3);
    assert!(output.contains("\\readmdcslword{NLEFT}"));
    assert!(output.contains("\\penalty1000\\relax \\readmdcslword{100}"));
    assert!(output.contains("\\readmdcslword{office}"));
    assert_eq!(flow, before);
    assert!(!writer.csl_columns_text);
    assert_eq!(writer.inline_text("After ordinary paragraph."), "After ordinary paragraph.");
}
