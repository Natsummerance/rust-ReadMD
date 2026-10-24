use super::*;
use crate::math_document::MathPatch;
use crate::omml;
use serde_json::{json, Value};

fn fragment(properties: Option<&str>, body: &str) -> String {
    let settings = properties.map(|properties| format!("<m:mathPr>{properties}</m:mathPr>")).unwrap_or_default();
    format!("{settings}<m:oMath>{body}</m:oMath>")
}
fn run(text: &str) -> String { format!("<m:r><m:t>{text}</m:t></m:r>") }
fn fraction() -> String { format!("<m:f><m:num>{}</m:num><m:den>{}</m:den></m:f>", run("a"), run("b")) }
fn state(xml: &str) -> MathDefaultsState {
    MathDefaultsState::from_xml(&math_document::parse_xml(xml).unwrap()).unwrap()
}
fn part(package: &crate::docx_writer::DocxPackage, name: &str) -> String {
    String::from_utf8(package.parts.iter().find(|(path, _)| path == name).unwrap_or_else(|| panic!("Missing {name}: {:?}", package.warns)).1.clone()).unwrap()
}
fn markdown(properties: &str, content: &str) -> String {
    format!("```readmd-math v1\nrow(office.mathPr({properties}), office.oMath({content}))\n```\n\n")
}
fn package(source: &str) -> crate::docx_writer::DocxPackage {
    crate::docx_writer::build(&crate::md_ast::parse(source), &json!({"cover":{"enabled":false},"toc":{"enabled":false}}), "math-defaults", &|_| None)
}
fn native_view(merge: &DocxDefaultsMerge, content: &str) -> omml::Node {
    omml::parse(&(merge.settings().map(XmlNode::xml).unwrap_or_default() + content)).unwrap()
}
fn nodes<'a>(root: &'a omml::Node, name: &str) -> Vec<&'a omml::Node> {
    let mut result = Vec::new();
    if root.is_math() && root.name == name { result.push(root); }
    for child in &root.children { result.extend(nodes(child, name)); }
    result
}

#[test]
fn three_state_booleans_retain_spelling_and_agree_with_existing_consumers() {
    for (key, missing) in [("smallFrac", false), ("dispDef", true), ("wrapRight", false)] {
        for (property, declaration, effective) in [
            (String::new(), Declared::Missing, missing),
            (format!("<m:{key}/>"), Declared::Implicit, true),
            (format!("<m:{key} m:val=\"0\"/>"), Declared::Explicit("0".into()), false),
            (format!("<m:{key} m:val=\"false\"/>"), Declared::Explicit("false".into()), false),
            (format!("<m:{key} m:val=\"off\"/>"), Declared::Explicit("off".into()), false),
            (format!("<m:{key} m:val=\"1\"/>"), Declared::Explicit("1".into()), true),
            (format!("<m:{key} m:val=\"true\"/>"), Declared::Explicit("true".into()), true),
            (format!("<m:{key} m:val=\"on\"/>"), Declared::Explicit("on".into()), true),
        ] {
            let source = fragment(Some(&property), &fraction());
            let before = source.clone();
            let defaults = state(&source);
            assert_eq!(defaults.declared[key], declaration, "{key}: {property}");
            let actual = match key {
                "smallFrac" => defaults.effective.small_fraction,
                "dispDef" => defaults.effective.display_defaults,
                _ => defaults.effective.wrap_right,
            };
            assert_eq!(actual, effective, "{key}: {property}");
            let view = omml::parse(&source).unwrap();
            assert_eq!(defaults.effective.small_fraction, view.math_defaults.small_fraction.unwrap_or(false));
            assert_eq!(defaults.effective.wrap_right, omml::math_wrap_style(&view).right);
            assert_eq!(source, before, "Resolving defaults changed author source");
        }
    }
    assert!(!state(&fragment(None, &run("x"))).present);
    assert!(state(&fragment(Some(""), &run("x"))).present);
}

#[test]
fn margins_and_wrap_indent_distinguish_missing_empty_zero_and_exact_units() {
    for (key, missing) in [("lMargin", 0), ("rMargin", 0), ("wrapIndent", 1440)] {
        for (property, expected, declaration) in [
            (String::new(), missing, Declared::Missing),
            (format!("<m:{key}/>"), 1440, Declared::Implicit),
            (format!("<m:{key} m:val=\"0\"/>"), 0, Declared::Explicit("0".into())),
            (format!("<m:{key} m:val=\"-0\"/>"), 0, Declared::Explicit("-0".into())),
            (format!("<m:{key} m:val=\"+000\"/>"), 0, Declared::Explicit("+000".into())),
            (format!("<m:{key} m:val=\"1440\"/>"), 1440, Declared::Explicit("1440".into())),
            (format!("<m:{key} m:val=\"31680\"/>"), 31680, Declared::Explicit("31680".into())),
        ] {
            let source = fragment(Some(&property), &run("x"));
            let defaults = state(&source);
            let view = omml::parse(&source).unwrap();
            let paragraph = omml::math_paragraph_style(&view, None);
            let wrap = omml::math_wrap_style(&view);
            let (twips, points) = match key {
                "lMargin" => (defaults.effective.left_margin, paragraph.left),
                "rMargin" => (defaults.effective.right_margin, paragraph.right),
                _ => (defaults.effective.wrap_indent, wrap.indent),
            };
            assert_eq!(twips, expected);
            assert_eq!(points, f64::from(expected) / 20.0);
            assert_eq!(defaults.declared[key], declaration);
            let (_, native) = omml::docx_math_fragment(&source, true).unwrap();
            if property == format!("<m:{key}/>") { assert!(native.unwrap().xml().contains("m:val=\"1440\"")); }
            assert_eq!(state(&source), defaults, "Output must not normalize source state");
        }
    }
}

#[test]
fn known_schema_values_are_validated_before_default_gating() {
    for (key, value) in [
        ("smallFrac", "maybe"), ("wrapRight", "FALSE"), ("dispDef", "2"),
        ("lMargin", "-1"), ("rMargin", "31681"), ("wrapIndent", "1.0"),
        ("preSp", "31681"), ("postSp", "1e2"), ("defJc", "justify"),
        ("brkBin", "left"), ("brkBinSub", "++"), ("intLim", "above"),
        ("naryLim", "below"), ("mathFont", "This font name is longer than 31 code units"),
    ] {
        let gate = if key == "dispDef" { "" } else { "<m:dispDef m:val=\"0\"/>" };
        let source = fragment(Some(&format!("{gate}<m:{key} m:val=\"{value}\"/>")), &run("x"));
        assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(&source).unwrap()).unwrap_err(), "math_invalid_property", "{key}={value}");
    }
    for invalid in [
        "<m:mathPr><m:smallFrac/><m:smallFrac/></m:mathPr><m:oMath/>",
        "<m:mathPr/><m:mathPr/><m:oMath/>",
        "<m:mathPr><m:unknown/></m:mathPr><m:oMath/>",
        "<m:mathPr><m:mathFont/></m:mathPr><m:oMath/>",
        "<m:mathPr><m:preSp/></m:mathPr><m:oMath/>",
        "<m:mathPr><m:postSp/></m:mathPr><m:oMath/>",
        "<m:mathPr><m:interSp/></m:mathPr><m:oMath/>",
        "<m:mathPr><m:intraSp/></m:mathPr><m:oMath/>",
    ] {
        assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(invalid).unwrap()).unwrap_err(), "math_invalid_property");
    }
}

#[test]
fn documented_implicit_values_materialize_only_in_the_native_copy() {
    let properties = "<m:brkBinSub/><m:lMargin/><m:rMargin/><m:defJc/><m:wrapIndent/><m:intLim/><m:naryLim/>";
    let source = fragment(Some(properties), &run("x"));
    let declarations = state(&source);
    let (_, settings) = omml::docx_math_fragment(&source, true).unwrap();
    let settings = settings.unwrap();
    let native = MathDefaultsState::from_xml(&settings).unwrap();
    assert_eq!(native.effective, declarations.effective);
    for key in ["brkBinSub", "defJc", "lMargin", "rMargin", "wrapIndent", "intLim", "naryLim"] {
        assert_eq!(declarations.declared[key], Declared::Implicit);
        assert_eq!(native.declared[key], Declared::Explicit(native_implicit_value(key).unwrap().into()));
    }
    assert!(source.contains(properties));
}

#[test]
fn dispdef_gates_only_five_overrides_not_spacing_operator_policy_or_local_jc() {
    for wrap in [r#"<m:wrapIndent m:val="240"/>"#, "<m:wrapRight/>"] {
        let properties = format!(r#"<m:brkBin m:val="repeat"/><m:brkBinSub m:val="+-"/><m:smallFrac/><m:dispDef m:val="0"/><m:lMargin/><m:rMargin/><m:defJc m:val="left"/><m:preSp m:val="120"/><m:postSp m:val="240"/><m:interSp m:val="360"/><m:intraSp m:val="480"/>{wrap}"#);
        let source = format!("<m:mathPr>{properties}</m:mathPr><m:oMathPara><m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr><m:oMath>{}</m:oMath></m:oMathPara>", fraction());
        let defaults = state(&source).effective;
        assert!(defaults.small_fraction);
        assert!(!defaults.display_defaults);
        assert!(!defaults.wrap_right);
        assert_eq!((defaults.pre_spacing, defaults.post_spacing, defaults.inter_spacing, defaults.intra_spacing), (120, 240, 360, 480));
        assert_eq!((defaults.left_margin, defaults.right_margin, defaults.wrap_indent), (0, 0, 0));
        assert_eq!(defaults.binary_break, MathBinaryBreak::Repeat);
        assert_eq!(defaults.subtraction, "+-");
        let mut merger = DocxDefaultsMerge::default();
        let output = merger.accept(&source, true).unwrap();
        let native = native_view(&merger, &output);
        assert_eq!(omml::math_paragraph_style(&native, Some(nodes(&native, "oMathPara")[0])).alignment, MathJustification::Right);
        assert_eq!(omml::math_wrap_style(&native).indent, 0.0);
        assert!(!omml::math_wrap_style(&native).right);
    }
}

#[test]
fn equivalent_xml_shapes_boolean_spellings_and_prefixes_do_not_conflict() {
    let equivalent = [
        fragment(None, &fraction()),
        fragment(Some(""), &fraction()),
        fragment(Some(r#"<m:brkBin m:val="before"/><m:brkBinSub m:val="--"/><m:smallFrac m:val="false"/><m:dispDef/><m:lMargin m:val="-0"/><m:rMargin m:val="+000"/><m:defJc m:val="centerGroup"/><m:preSp m:val="0"/><m:postSp m:val="0"/><m:interSp m:val="0"/><m:intraSp m:val="0"/><m:wrapIndent m:val="1440"/><m:intLim m:val="subSup"/><m:naryLim m:val="undOvr"/>"#), &fraction()),
        fragment(Some(r#"<m:smallFrac m:val="false"/><m:dispDef m:val="true"/><m:wrapRight m:val="off"/>"#), &fraction()),
        format!(r#"<q:mathPr xmlns:q="{MATH_NS}"><q:smallFrac q:val="0"/><q:dispDef q:val="on"/><q:wrapIndent/></q:mathPr><q:oMath xmlns:q="{MATH_NS}"><q:r><q:t>x</q:t></q:r></q:oMath>"#),
    ];
    for reverse in [false, true] {
        let mut inputs = equivalent.iter().collect::<Vec<_>>();
        if reverse { inputs.reverse(); }
        let mut merger = DocxDefaultsMerge::default();
        for source in inputs { merger.accept(source, true).unwrap(); }
    }
    let source = markdown("office.smallFrac(val: false), office.dispDef(val: true), office.lMargin(val: 0), office.rMargin(val: 0), office.wrapIndent(val: 1440)", "frac(a, b)") + "$$\\frac{c}{d}$$\n";
    let output = package(&source);
    assert!(!output.parts.is_empty(), "{:?}", output.warns);
    assert!(!output.warns.iter().any(|warning| warning.starts_with("math_document_defaults_conflict")));
    assert_eq!(nodes(&omml::parse(&part(&output, "word/document.xml")).unwrap(), "f").len(), 2);
}

#[test]
fn implicit_margins_and_explicit_1440_are_equivalent_without_normalizing_source() {
    let empty = fragment(Some("<m:lMargin/><m:rMargin/><m:wrapIndent/>"), &run("x"));
    let explicit = fragment(Some(r#"<m:lMargin m:val="1440"/><m:rMargin m:val="1440"/><m:wrapIndent m:val="1440"/>"#), &run("y"));
    assert_ne!(state(&empty).declared, state(&explicit).declared);
    assert_eq!(state(&empty).effective, state(&explicit).effective);
    for pair in [[&empty, &explicit], [&explicit, &empty]] {
        let mut merger = DocxDefaultsMerge::default();
        for source in pair { merger.accept(source, true).unwrap(); }
    }
    assert!(empty.contains("<m:lMargin/>"));
}

#[test]
fn native_paragraph_jc_localizes_different_global_alignment_including_bare_display() {
    let sources = [
        fragment(Some(r#"<m:defJc m:val="left"/>"#), &run("Left")),
        fragment(Some(r#"<m:defJc m:val="right"/>"#), &run("Right")),
        fragment(None, &run("Bare")),
    ];
    for reverse in [false, true] {
        let mut merger = DocxDefaultsMerge::default();
        let mut outputs = Vec::new();
        for index in if reverse { vec![2, 1, 0] } else { vec![0, 1, 2] } {
            outputs.push((index, merger.accept(&sources[index], true).unwrap()));
        }
        for (index, output) in outputs {
            let native = native_view(&merger, &output);
            let paragraphs = nodes(&native, "oMathPara");
            assert_eq!(paragraphs.len(), 1);
            let expected = [MathJustification::Left, MathJustification::Right, MathJustification::CenterGroup][index];
            assert_eq!(omml::math_paragraph_style(&native, Some(paragraphs[0])).alignment, expected);
            assert_eq!(nodes(&native, "jc").len(), 1);
        }
    }
}

#[test]
fn native_jc_preserves_existing_local_override_and_empty_local_default() {
    for (local, expected) in [("right", MathJustification::Right), ("", MathJustification::CenterGroup)] {
        let jc = if local.is_empty() { "<m:jc/>".into() } else { format!("<m:jc m:val=\"{local}\"/>") };
        let source = format!("<m:mathPr><m:defJc m:val=\"left\"/></m:mathPr><m:oMathPara><m:oMathParaPr>{jc}</m:oMathParaPr><m:oMath>{}</m:oMath></m:oMathPara>", run("x"));
        let before = source.clone();
        let mut merger = DocxDefaultsMerge::default();
        let native = merger.accept(&source, true).unwrap();
        let native = native_view(&merger, &native);
        assert_eq!(nodes(&native, "oMathParaPr").len(), 1);
        assert_eq!(nodes(&native, "jc").len(), 1);
        assert_eq!(omml::math_paragraph_style(&native, Some(nodes(&native, "oMathPara")[0])).alignment, expected);
        assert_eq!(source, before);
    }
}

#[test]
fn grouping_native_display_equations_keeps_existing_paragraph_boundaries() {
    let source = format!("<m:mathPr><m:defJc m:val=\"left\"/></m:mathPr><m:oMath>{}</m:oMath><!-- keep equation group --><m:oMath>{}</m:oMath><m:oMathPara><m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr><m:oMath>{}</m:oMath></m:oMathPara><m:oMath>{}</m:oMath>", run("a"), run("b"), run("c"), run("d"));
    let mut merger = DocxDefaultsMerge::default();
    let native = merger.accept(&source, true).unwrap();
    let view = native_view(&merger, &native);
    let paragraphs = nodes(&view, "oMathPara");
    assert_eq!(paragraphs.iter().map(|paragraph| nodes(paragraph, "oMath").len()).collect::<Vec<_>>(), [2, 1, 1]);
    assert_eq!(paragraphs.iter().map(|paragraph| omml::math_paragraph_style(&view, Some(paragraph)).alignment).collect::<Vec<_>>(), [MathJustification::Left, MathJustification::Right, MathJustification::Left]);
    assert!(native.contains("keep equation group"));
}

#[test]
fn genuinely_global_conflicts_reject_both_orders_with_bounded_property_identity() {
    for (property, xml) in [
        ("smallFrac", "<m:smallFrac/>"),
        ("preSp", r#"<m:preSp m:val="120"/>"#),
        ("postSp", r#"<m:postSp m:val="120"/>"#),
        ("interSp", r#"<m:interSp m:val="120"/>"#),
        ("intraSp", r#"<m:intraSp m:val="120"/>"#),
        ("lMargin", "<m:lMargin/>"),
        ("rMargin", "<m:rMargin/>"),
        ("wrapIndent", r#"<m:wrapIndent m:val="0"/>"#),
        ("wrapRight", "<m:wrapRight/>"),
        ("brkBin", r#"<m:brkBin m:val="repeat"/>"#),
        ("brkBinSub", r#"<m:brkBinSub m:val="+-"/>"#),
        ("mathFont", r#"<m:mathFont m:val="STIX Two Math"/>"#),
    ] {
        let bare = fragment(None, &fraction());
        let custom = fragment(Some(xml), &fraction());
        for pair in [[&bare, &custom], [&custom, &bare]] {
            let mut merger = DocxDefaultsMerge::default();
            merger.accept(pair[0], true).unwrap();
            let error = merger.accept(pair[1], true).unwrap_err();
            assert_eq!(error, DefaultsError::Conflict { property, first_formula: 1, formula: 2 });
            let details: Value = serde_json::from_str(error.message().strip_prefix("math_document_defaults_conflict: ").unwrap()).unwrap();
            assert_eq!(details, json!({"property":property,"firstFormula":1,"formula":2}));
        }
    }
}

#[test]
fn dispdef_paragraph_inheritance_is_not_equated_with_explicit_zero_overrides() {
    let off = fragment(Some(r#"<m:dispDef m:val="0"/><m:lMargin/><m:rMargin/><m:wrapIndent m:val="240"/>"#), &fraction());
    let zeros = fragment(Some(r#"<m:dispDef m:val="1"/><m:lMargin m:val="0"/><m:rMargin m:val="0"/><m:wrapIndent m:val="0"/>"#), &fraction());
    assert_ne!(state(&off).declared, state(&zeros).declared);
    for pair in [[&off, &zeros], [&zeros, &off]] {
        let mut merger = DocxDefaultsMerge::default();
        merger.accept(pair[0], true).unwrap();
        assert_eq!(merger.accept(pair[1], true).unwrap_err(), DefaultsError::Conflict { property: "dispDef", first_formula: 1, formula: 2 });
    }
}

#[test]
fn disabled_equivalent_settings_keep_inheritance_and_never_invent_local_jc() {
    let first = fragment(Some(r#"<m:dispDef m:val="0"/><m:lMargin/><m:rMargin/><m:defJc m:val="left"/><m:wrapIndent m:val="240"/>"#), &run("a"));
    let second = format!("<m:mathPr><m:dispDef m:val=\"false\"/><m:lMargin m:val=\"0\"/><m:defJc m:val=\"right\"/><m:wrapIndent m:val=\"0\"/></m:mathPr><m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara>", run("b"));
    for pair in [[&first, &second], [&second, &first]] {
        let mut merger = DocxDefaultsMerge::default();
        for source in pair {
            let content = merger.accept(source, true).unwrap();
            let view = omml::parse(&content).unwrap();
            assert_eq!(nodes(&view, "oMathPara").len(), 1);
            assert!(nodes(&view, "jc").is_empty(), "A synthetic jc must not override paragraph inheritance");
        }
    }
}

#[test]
fn inline_source_defaults_never_supply_a_later_bare_display_policy() {
    let inline = fragment(Some(r#"<m:smallFrac/><m:defJc m:val="right"/><m:preSp m:val="120"/><m:postSp m:val="240"/><m:interSp m:val="360"/><m:intraSp m:val="480"/>"#), &fraction());
    let bare = fragment(None, &fraction());
    for reverse in [false, true] {
        let inputs = if reverse { vec![(&bare, true), (&inline, false)] } else { vec![(&inline, false), (&bare, true)] };
        let mut merger = DocxDefaultsMerge::default();
        let mut display = String::new();
        for (source, is_display) in inputs {
            let content = merger.accept(source, is_display).unwrap();
            if is_display { display = content; }
        }
        assert!(merger.settings().is_none(), "The bare display source owns its missing global defaults");
        let view = native_view(&merger, &display);
        assert_eq!(view.math_defaults.small_fraction, None);
        assert_eq!(omml::math_wrap_style(&view).binary, MathBinaryBreak::Before);
        assert_eq!(omml::math_wrap_style(&view).indent, 72.0);
    }
}

#[test]
fn inline_font_conflicts_still_reject_even_without_display_formulas() {
    let named = fragment(Some(r#"<m:mathFont m:val="Cambria Math"/>"#), &run("x"));
    let bare = fragment(None, &run("y"));
    let mut merger = DocxDefaultsMerge::default();
    merger.accept(&named, false).unwrap();
    assert_eq!(merger.accept(&bare, false).unwrap_err(), DefaultsError::Conflict { property: "mathFont", first_formula: 1, formula: 2 });
}

#[test]
fn inline_wrapping_defaults_are_document_wide_even_if_the_renderer_fits_one_line() {
    for (property, settings) in [
        ("dispDef", r#"<m:dispDef m:val="0"/>"#),
        ("lMargin", "<m:lMargin/>"),
        ("rMargin", "<m:rMargin/>"),
        ("wrapIndent", r#"<m:wrapIndent m:val="0"/>"#),
        ("wrapRight", "<m:wrapRight/>"),
        ("brkBin", r#"<m:brkBin m:val="after"/>"#),
        ("brkBinSub", r#"<m:brkBinSub m:val="+-"/>"#),
    ] {
        let bare = fragment(None, &run("a+b-c+d-e+f"));
        let custom = fragment(Some(settings), &run("u+v-w+x-y+z"));
        for first_display in [false, true] {
            for reverse in [false, true] {
                let pair = if reverse { [&custom, &bare] } else { [&bare, &custom] };
                let mut merger = DocxDefaultsMerge::default();
                merger.accept(pair[0], first_display).unwrap();
                assert_eq!(merger.accept(pair[1], false).unwrap_err(), DefaultsError::Conflict { property, first_formula: 1, formula: 2 });
            }
        }
    }
}

#[test]
fn writer_rejects_instead_of_exposing_parts_for_bare_latex_and_unicodemath_conflicts() {
    let small = markdown("office.smallFrac()", "frac(a, b)");
    for bare in ["$$\\frac{c}{d}$$\n", "```readmd-math v1\nfrac(c, d)\n```\n"] {
        for source in [small.clone() + bare, bare.to_string() + "\n" + &small] {
            let output = package(&source);
            assert!(output.parts.is_empty(), "A changed-default DOCX escaped: {:?}", output.warns);
            assert_eq!(output.warns.len(), 1);
            let details: Value = serde_json::from_str(output.warns[0].strip_prefix("math_document_defaults_conflict: ").unwrap()).unwrap();
            assert_eq!(details["property"], "smallFrac");
        }
    }
    let unicode = math_document::parse("a/b", "unicodemath", true).unwrap();
    let small = math_document::parse("row(office.mathPr(office.smallFrac()), office.oMath(frac(c, d)))", "dsl", true).unwrap();
    let mut merger = DocxDefaultsMerge::default();
    merger.accept(&unicode.root.xml(), true).unwrap();
    assert_eq!(merger.accept(&small.root.xml(), true).unwrap_err(), DefaultsError::Conflict { property: "smallFrac", first_formula: 1, formula: 2 });
}

#[test]
fn writer_localizes_real_paragraphs_without_image_or_settings_inside_equations() {
    let source = markdown("office.defJc(val: left), office.naryLim(val: undOvr)", "sum(a, from: 0, to: 1)")
        + &markdown("office.defJc(val: right), office.naryLim(val: subSup)", "sum(b, from: 0, to: 1)")
        + "$$c$$\n";
    let output = package(&source);
    assert!(!output.parts.is_empty(), "{:?}", output.warns);
    let body = part(&output, "word/document.xml");
    let settings = part(&output, "word/settings.xml");
    assert!(!body.contains("mathPr"));
    assert!(!body.contains("<w:drawing"));
    assert_eq!(settings.matches("<m:mathPr").count(), 1);
    let document = omml::parse(&body).unwrap();
    let paragraphs = nodes(&document, "oMathPara");
    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs.iter().map(|paragraph| paragraph.child("oMathParaPr").unwrap().child("jc").unwrap().attr("val").unwrap()).collect::<Vec<_>>(), ["left", "right", "centerGroup"]);
    assert_eq!(nodes(&document, "limLoc").iter().map(|node| node.attr("val").unwrap()).collect::<Vec<_>>(), ["undOvr", "subSup"]);
    for (_, bytes) in &output.parts {
        if bytes.starts_with(b"<?xml") { math_document::parse_xml(std::str::from_utf8(bytes).unwrap()).unwrap(); }
    }
}

#[test]
fn per_formula_patch_keeps_three_states_comments_utf16_and_rejects_stale_revision() {
    let source = "// 保留 😀\r\nrow(office.mathPr(office.smallFrac(val: false), office.dispDef(), office.wrapRight(val: false)), office.oMath(frac(α, β)))";
    let original = math_document::parse(source, "dsl", true).unwrap();
    let node = original.nodes.iter().find(|node| node.kind == "smallFrac").unwrap();
    let empty = MathPatch::SetAttribute { node_id: node.id.clone(), name: "m:val".into(), value: None };
    let changed = math_document::patch(source, "dsl", true, &original.revision, &empty).unwrap();
    assert!(changed.source.starts_with("// 保留 😀\r\n"));
    assert!(changed.source.contains("office.smallFrac()"));
    assert!(changed.source.contains("office.dispDef()"));
    assert!(changed.source.contains("office.wrapRight(val: false)"));
    assert_eq!(state(&changed.root.xml()).declared["smallFrac"], Declared::Implicit);
    let stale = math_document::patch(&changed.source, "dsl", true, &original.revision, &empty).unwrap_err();
    assert_eq!(stale, "transaction_conflict");
    let current = changed.nodes.iter().find(|node| node.kind == "smallFrac").unwrap();
    let off = math_document::patch(&changed.source, "dsl", true, &changed.revision, &MathPatch::SetAttribute { node_id: current.id.clone(), name: "m:val".into(), value: Some("0".into()) }).unwrap();
    assert_eq!(state(&off.root.xml()).declared["smallFrac"], Declared::Explicit("0".into()));
    assert!(off.source.starts_with("// 保留 😀\r\n"));
    assert!(off.source.contains("frac(α, β)"));
}

#[test]
fn current_native_values_derive_new_defaults_without_an_old_source_snapshot() {
    let source = markdown("office.smallFrac(val: false), office.dispDef(), office.wrapRight(val: false)", "frac(a, b)");
    let output = package(&source);
    let original = part(&output, "word/settings.xml");
    assert!(original.contains("m:val=\"false\""));
    let changed = original.replacen("<m:smallFrac m:val=\"false\"", "<m:smallFrac m:val=\"true\"", 1);
    assert_ne!(original, changed);
    let root = math_document::parse_xml(&changed).unwrap();
    let settings = root.children().next().unwrap().children().find(|node| math(node, "mathPr")).unwrap();
    let current = MathDefaultsState::from_xml(settings).unwrap();
    assert!(current.effective.small_fraction);
    assert_eq!(current.declared["smallFrac"], Declared::Explicit("true".into()));
    assert!(!output.parts.iter().any(|(name, _)| name.contains("mathSource") || name.contains("mathDefaults")));
}

#[test]
fn small_fraction_and_dispdef_changes_reach_actual_vector_geometry() {
    let display = fragment(Some(r#"<m:smallFrac m:val="0"/>"#), &fraction());
    let small = fragment(Some("<m:smallFrac/>"), &fraction());
    let layout = |source: &str| crate::math_layout::layout_source_constrained("", Some(source), true, 12.0, Some(240.0)).unwrap();
    let a = layout(&display);
    let b = layout(&small);
    assert_ne!(a.svg("#000000"), b.svg("#000000"));
    assert_ne!(a.draw(0.0, 0.0, "0 0 0"), b.draw(0.0, 0.0, "0 0 0"));
    assert!(a.height + a.depth > b.height + b.depth);
    let spaced = fragment(Some(r#"<m:preSp m:val="120"/><m:postSp m:val="240"/>"#), &fraction());
    let disabled = fragment(Some(r#"<m:dispDef m:val="0"/><m:preSp m:val="120"/><m:postSp m:val="240"/>"#), &fraction());
    let spaced = layout(&spaced);
    let disabled = layout(&disabled);
    assert!((spaced.height - a.height - 6.0).abs() < 1e-8);
    assert!((spaced.depth - a.depth - 12.0).abs() < 1e-8);
    assert_eq!(spaced.height, disabled.height);
    assert_eq!(spaced.depth, disabled.depth);
    assert_eq!(spaced.width, disabled.width);
    assert_eq!(spaced.svg("#000000"), disabled.svg("#000000"));
    assert_eq!(spaced.draw(0.0, 0.0, "0 0 0"), disabled.draw(0.0, 0.0, "0 0 0"));
}

#[test]
fn foreign_val_cannot_impersonate_math_val_and_invalid_source_cannot_emit_parts() {
    let source = format!(r#"<m:mathPr xmlns:m="{MATH_NS}" xmlns:x="urn:foreign"><m:smallFrac x:val="0"/></m:mathPr><m:oMath xmlns:m="{MATH_NS}"/>"#);
    assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(&source).unwrap()).unwrap_err(), "math_invalid_property");
    // A directly supplied OfficeMath tree must fail rather than silently
    // supplying another default when a native property is out of range.
    let mut document = crate::md_ast::parse("Ordinary body");
    document.blocks.push(crate::md_ast::Block::OfficeMath { latex: "x".into(), omml: fragment(Some(r#"<m:wrapIndent m:val="31681"/>"#), &run("x")) });
    let direct = crate::docx_writer::build(&document, &json!({}), "invalid", &|_| None);
    assert!(direct.parts.is_empty());
    assert_eq!(direct.warns, ["math_invalid_property"]);
}

#[test]
fn inline_display_paragraph_is_rejected_through_the_actual_dsl_ast_in_both_orders() {
    let inline = ":math[row(office.mathPr(office.smallFrac()), office.oMathPara(office.oMath(frac(a, b))))]";
    for source in [format!("Before {inline} after.\n\n$$c$$\n"), format!("$$c$$\n\nBefore {inline} after.\n")] {
        let document = crate::md_ast::parse(&source);
        let mut displays = 0;
        crate::md_ast::walk_inlines(&document.blocks, &mut |inline| {
            if let crate::md_ast::Inline::OfficeMath { omml, .. } = inline {
                if !nodes(&omml::parse(omml).unwrap(), "oMathPara").is_empty() { displays += 1; }
            }
        });
        assert_eq!(displays, 1, "The regression must reach a real inline OfficeMath AST, not a code listing");
        let before = source.clone();
        let output = package(&source);
        assert!(output.parts.is_empty());
        assert_eq!(output.warns, ["math_invalid_source"]);
        assert_eq!(source, before);
    }
    let source = "```readmd-math v1\nrow(office.mathPr(office.smallFrac()), office.oMathPara(office.oMath(frac(a, b))))\n```\n";
    let output = package(source);
    assert!(!output.parts.is_empty(), "{:?}", output.warns);
    let view = omml::parse(&part(&output, "word/document.xml")).unwrap();
    assert_eq!(nodes(&view, "oMathPara").len(), 1);
    assert_eq!(nodes(&view, "f").len(), 1);
}

#[test]
fn legacy_inline_metadata_cannot_hide_a_display_paragraph_from_default_scope() {
    let xml = format!("<m:mathPr><m:smallFrac/></m:mathPr><m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara>", fraction());
    let latex = "\\frac{a}{b}";
    let capsule = math_document::metadata(latex, &xml, "scope-legacy").unwrap();
    for source in [format!("Before ${latex}${capsule} after.\n\n$$c$$\n"), format!("$$c$$\n\nBefore ${latex}${capsule} after.\n")] {
        let document = crate::md_ast::parse(&source);
        let mut inline_count = 0;
        crate::md_ast::walk_inlines(&document.blocks, &mut |inline| {
            if let crate::md_ast::Inline::OfficeMath { omml, .. } = inline { assert_eq!(omml, &xml); inline_count += 1; }
        });
        assert_eq!(inline_count, 1);
        let output = package(&source);
        assert!(output.parts.is_empty());
        assert_eq!(output.warns, ["math_invalid_source"]);
        assert!(source.contains(&capsule));
    }
}

#[test]
fn all_default_leaves_validate_standard_attributes_by_expanded_namespace() {
    for key in PROPERTIES {
        let value = match key {
            "mathFont" => "STIX Two Math", "brkBin" => "before", "brkBinSub" => "--",
            "defJc" => "centerGroup", "intLim" => "subSup", "naryLim" => "undOvr", _ => "0",
        };
        for attribute in [format!("val=\"{value}\""), format!("m:val=\"{value}\" m:bogus=\"x\""), format!("x:val=\"{value}\""), format!("m:val=\"{value}\" q:val=\"{value}\"")] {
            let xml = format!("<m:mathPr xmlns:m=\"{MATH_NS}\" xmlns:q=\"{MATH_NS}\" xmlns:x=\"urn:forged\"><m:{key} {attribute}/></m:mathPr><m:oMath>{}</m:oMath>", run("x"));
            assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(&xml).unwrap()).unwrap_err(), "math_invalid_property", "{key} {attribute}");
            let mut merger = DocxDefaultsMerge::default();
            assert_eq!(merger.accept(&xml, true).unwrap_err(), DefaultsError::Source("math_invalid_property".into()));
        }
    }
    for attribute in ["val=\"0\"", "m:val=\"0\"", "m:bogus=\"x\""] {
        let xml = format!("<m:mathPr {attribute}/><m:oMath/>");
        assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(&xml).unwrap()).unwrap_err(), "math_invalid_property");
    }
    let forged = format!("<x:mathPr xmlns:x=\"urn:forged\"><x:smallFrac x:val=\"0\"/></x:mathPr><m:oMath>{}</m:oMath>", run("x"));
    assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(&forged).unwrap()).unwrap_err(), "math_invalid_property");
}

#[test]
fn real_math_namespace_aliases_and_default_element_namespace_remain_supported() {
    let sources = [
        format!("<q:mathPr xmlns:q=\"{MATH_NS}\"><q:smallFrac q:val=\"0\"/><q:dispDef q:val=\"true\"/></q:mathPr><q:oMath xmlns:q=\"{MATH_NS}\"><q:r><q:t>x</q:t></q:r></q:oMath>"),
        format!("<mathPr xmlns=\"{MATH_NS}\" xmlns:v=\"{MATH_NS}\"><smallFrac v:val=\"false\"/><dispDef v:val=\"1\"/></mathPr><m:oMath>{}</m:oMath>", run("y")),
    ];
    let expected = state(&fragment(None, &run("z"))).effective;
    let mut merger = DocxDefaultsMerge::default();
    for source in sources {
        assert_eq!(state(&source).effective, expected);
        let output = merger.accept(&source, true).unwrap();
        assert_eq!(nodes(&omml::parse(&output).unwrap(), "oMathPara").len(), 1);
    }
    let source = format!("<q:mathPr xmlns:q=\"{MATH_NS}\"><q:preSp q:val=\"120\"/><q:postSp q:val=\"240\"/></q:mathPr><q:oMath xmlns:q=\"{MATH_NS}\"><q:r><q:t>x</q:t></q:r></q:oMath>");
    let before = source.clone();
    assert_eq!((state(&source).effective.pre_spacing, state(&source).effective.post_spacing), (120, 240));
    let mut merger = DocxDefaultsMerge::default();
    let output = merger.accept(&source, true).unwrap();
    assert_eq!(nodes(&omml::parse(&output).unwrap(), "oMathPara").len(), 1);
    assert_eq!(source, before, "Canonical validation must not rewrite real aliases in source");
}

#[test]
fn opaque_foreign_defaults_remain_in_source_and_never_become_a_native_global_package() {
    for properties in [r#"<m:smallFrac m:val="0" xmlns:x="urn:future" x:future="keep"/>"#, r#"<x:future xmlns:x="urn:future"/>"#] {
        let xml = fragment(Some(properties), &fraction());
        let before = xml.clone();
        let mut merger = DocxDefaultsMerge::default();
        assert_eq!(merger.accept(&xml, true).unwrap_err(), DefaultsError::Source("math_unsupported_extension".into()));
        let capsule = math_document::metadata("\\frac{a}{b}", &xml, "defaults-foreign").unwrap();
        let source = format!("$$\\frac{{a}}{{b}}$$\n\n{capsule}\n");
        let output = package(&source);
        assert!(output.parts.is_empty());
        assert_eq!(output.warns, ["math_unsupported_extension"]);
        assert_eq!(xml, before);
        assert_eq!(math_document::read_metadata(&capsule, "\\frac{a}{b}").unwrap().0.omml, xml);
    }
}

#[test]
fn mathpr_uses_the_fixed_schema_choice_and_order_before_native_parts_exist() {
    for properties in [
        "<m:wrapIndent/><m:wrapRight/>",
        "<m:wrapRight/><m:wrapIndent/>",
        "<m:dispDef/><m:smallFrac/>",
        "<m:defJc/><m:lMargin/>",
        "<m:smallFrac/><m:smallFrac/>",
        "visible default text<m:smallFrac/>",
    ] {
        let xml = fragment(Some(properties), &fraction());
        let mut merger = DocxDefaultsMerge::default();
        assert_eq!(merger.accept(&xml, true).unwrap_err(), DefaultsError::Source("math_invalid_property".into()), "{properties}");
        // Bypass Markdown's invalid-DSL-as-code fallback. The actual OfficeMath
        // AST must reach the same all-parts barrier as imported native XML.
        let mut document = crate::md_ast::parse("Before");
        document.blocks.push(crate::md_ast::Block::OfficeMath { latex: "\\frac{a}{b}".into(), omml: xml });
        let output = crate::docx_writer::build(&document, &json!({}), "invalid-particle", &|_| None);
        assert!(output.parts.is_empty());
        assert_eq!(output.warns, ["math_invalid_property"]);
    }
    for property in ["office.wrapIndent()", "office.wrapRight(val: false)"] {
        let source = markdown(property, "frac(a, b)");
        let document = crate::md_ast::parse(&source);
        assert_eq!(document.blocks.iter().filter(|block| matches!(block, crate::md_ast::Block::OfficeMath { .. })).count(), 1);
        let output = package(&source);
        assert!(!output.parts.is_empty(), "{:?}", output.warns);
        assert_eq!(nodes(&omml::parse(&part(&output, "word/document.xml")).unwrap(), "f").len(), 1);
    }
    let xml = fragment(Some(" \r\n<!-- keep default note --><m:smallFrac m:val=\"0\"/> \t\n"), &fraction());
    let before = xml.clone();
    let mut merger = DocxDefaultsMerge::default();
    let output = merger.accept(&xml, true).unwrap();
    assert_eq!(nodes(&omml::parse(&output).unwrap(), "f").len(), 1);
    assert_eq!(xml, before, "Element-only validation must allow and preserve source whitespace/comments");
}

#[test]
fn forged_namespace_declarations_cannot_impersonate_math_defaults() {
    for xml in [
        r#"<m:mathPr xmlns:m="urn:forged"><m:smallFrac m:val="0"/></m:mathPr><m:oMath/>"#.to_string(),
        format!(r#"<m:mathPr xmlns:m="{MATH_NS}"><m:smallFrac q:val="0"/></m:mathPr><m:oMath/>"#),
        format!(r#"<m:mathPr xmlns:m="{MATH_NS}" xmlns:q="urn:forged"><m:smallFrac q:val="0"/></m:mathPr><m:oMath/>"#),
    ] {
        let mut merger = DocxDefaultsMerge::default();
        assert_eq!(merger.accept(&xml, true).unwrap_err(), DefaultsError::Source("math_invalid_property".into()));
    }
    for xml in [
        format!(r#"<m:mathPr xmlns:q="{MATH_NS} "><m:smallFrac q:val="0"/></m:mathPr><m:oMath/>"#),
        r#"<m:mathPr xmlns:xml="urn:forged"><m:smallFrac/></m:mathPr><m:oMath/>"#.to_string(),
        r#"<m:mathPr xmlns:xmlns="urn:forged"><m:smallFrac/></m:mathPr><m:oMath/>"#.to_string(),
        r#"<m:mathPr><m:smallFrac m:val="0" m:val="1"/></m:mathPr><m:oMath/>"#.to_string(),
    ] {
        assert_eq!(math_document::parse_xml(&xml).unwrap_err(), "math_invalid_xml");
        let mut merger = DocxDefaultsMerge::default();
        assert_eq!(merger.accept(&xml, true).unwrap_err(), DefaultsError::Source("math_invalid_xml".into()));
    }
    for prefix in ["m", "x"] {
        for (local, code) in [("future", "math_unsupported_extension"), ("val", "math_invalid_property")] {
            for container in [false, true] {
                let (outer, inner) = if container { (format!("{prefix}:{local}=\"0\""), String::new()) }
                    else { (String::new(), format!("{prefix}:{local}=\"0\"")) };
                let xml = format!(r#"<q:mathPr xmlns:q="{MATH_NS}" xmlns:{prefix}="urn:foreign" {outer}><q:smallFrac {inner}/></q:mathPr><q:oMath xmlns:q="{MATH_NS}"/>"#);
                let mut merger = DocxDefaultsMerge::default();
                assert_eq!(merger.accept(&xml, true).unwrap_err(), DefaultsError::Source(code.into()), "A namespace alias must not change error classification: {prefix}:{local}");
            }
        }
    }
}

#[test]
fn four_documented_empty_enums_roundtrip_as_editable_three_state_source() {
    for (key, default, other) in [
        ("defJc", "centerGroup", "right"), ("intLim", "subSup", "undOvr"),
        ("naryLim", "undOvr", "subSup"), ("brkBinSub", "--", "+-"),
    ] {
        let xml = fragment(Some(&format!("<m:{key}/>")), &(fraction() + &fraction()));
        let readable = crate::math_migration::readable(&xml).unwrap();
        assert!(readable.contains(&format!("office.{key}()")), "{readable}");
        let source = format!("// keep 😀\r\n{readable}");
        let document = math_document::parse(&source, "dsl", true).unwrap();
        assert_eq!(state(&document.root.xml()).declared[key], Declared::Implicit);
        assert_eq!(state(&document.root.xml()).effective, state(&fragment(None, &fraction())).effective);
        let explicit_xml = fragment(Some(&format!("<m:{key} m:val=\"{default}\"/>")), &fraction());
        assert_ne!(state(&xml).declared[key], state(&explicit_xml).declared[key]);
        assert_eq!(state(&xml).effective, state(&explicit_xml).effective);
        assert_eq!(state(&fragment(None, &fraction())).declared[key], Declared::Missing);

        let target = document.nodes.iter().find(|node| node.kind == key).unwrap();
        let set = MathPatch::SetAttribute { node_id: target.id.clone(), name: "m:val".into(), value: Some(other.into()) };
        let explicit = math_document::patch(&source, "dsl", true, &document.revision, &set).unwrap();
        assert_eq!(state(&explicit.root.xml()).declared[key], Declared::Explicit(other.into()));
        assert!(explicit.source.starts_with("// keep 😀\r\n"));
        assert_eq!(nodes(&omml::parse(&explicit.root.xml()).unwrap(), "f").len(), 2);
        let target = explicit.nodes.iter().find(|node| node.kind == key).unwrap();
        let clear = MathPatch::SetAttribute { node_id: target.id.clone(), name: "m:val".into(), value: None };
        let restored = math_document::patch(&explicit.source, "dsl", true, &explicit.revision, &clear).unwrap();
        assert!(restored.source.starts_with("// keep 😀\r\n"));
        assert!(restored.source.contains(&format!("office.{key}()")));
        assert_eq!(state(&restored.root.xml()).declared[key], Declared::Implicit);
        assert_eq!(math_document::xml_semantics(&restored.root), math_document::xml_semantics(&document.root));
        assert_eq!(math_document::patch(&restored.source, "dsl", true, &explicit.revision, &clear).unwrap_err(), "transaction_conflict");

        let markdown = format!("```readmd-math v1\n{}\n```\n", restored.source);
        let output = package(&markdown);
        assert!(!output.parts.is_empty(), "{key}: {:?}", output.warns);
        assert_eq!(nodes(&omml::parse(&part(&output, "word/document.xml")).unwrap(), "f").len(), 2);
        let settings = math_document::parse_xml(&part(&output, "word/settings.xml")).unwrap();
        let math_pr = settings.children().next().unwrap().children().find(|node| math(node, "mathPr")).unwrap();
        assert_eq!(MathDefaultsState::from_xml(math_pr).unwrap().declared[key], Declared::Explicit(default.into()));
        assert!(restored.source.contains(&format!("office.{key}()")), "Native normalization changed editable source");
    }
}

#[test]
fn empty_enum_migration_preserves_occurrences_and_requires_exact_attribute_defaults() {
    let latex = "\\frac{a}{b}";
    let xml = fragment(Some("<m:brkBinSub/><m:defJc/><m:intLim/><m:naryLim/>"), &fraction());
    let first = math_document::metadata(latex, &xml, "empty-enum-first").unwrap();
    let second = math_document::metadata(latex, &xml, "empty-enum-second").unwrap();
    let source = format!("Before 😀\r\n\r\n$${latex}$$\r\n\r\n{first}\r\n\r\n$${latex}$$\r\n\r\n{second}\r\n");
    let migration = crate::math_migration::plan(&source).unwrap();
    assert_eq!(migration.edits.len(), 2);
    assert!(migration.recoveries.is_empty());
    assert!(migration.diagnostics.is_empty());
    assert!(migration.content.starts_with("Before 😀\r\n"));
    for key in ["brkBinSub", "defJc", "intLim", "naryLim"] {
        assert_eq!(migration.content.matches(&format!("office.{key}()")).count(), 2);
    }
    let output = package(&migration.content);
    assert!(!output.parts.is_empty(), "{:?}", output.warns);
    assert_eq!(nodes(&omml::parse(&part(&output, "word/document.xml")).unwrap(), "f").len(), 2);
    assert_eq!(crate::math_migration::plan(&migration.content).unwrap().content, migration.content);

    for key in ["defJc", "intLim", "naryLim", "brkBinSub"] {
        assert_eq!(math_document::office_implicit_value(key, "val"), None);
        assert_eq!(math_document::office_implicit_value(key, "w:val"), None);
        assert_eq!(math_document::office_implicit_value(key, "m:bogus"), None);
        assert_eq!(crate::math_dsl::compile(&format!("office.{key}(val: \"invalid\")"), true).unwrap_err(), "math_invalid_property");
    }
    for key in ["mathFont", "preSp", "postSp", "interSp", "intraSp", "argSz", "count"] {
        assert_eq!(math_document::office_implicit_value(key, "m:val"), None);
        let root = math_document::parse_xml(&format!("<m:{key}/>")).unwrap();
        assert_eq!(crate::math_dsl::validate_tree(&root).unwrap_err(), "math_invalid_property");
        assert_eq!(crate::math_dsl::compile(&format!("office.{key}()"), true).unwrap_err(), "math_invalid_property");
    }
    for key in ["mathFont", "preSp", "postSp", "interSp", "intraSp"] {
        let value = if key == "mathFont" { "STIX Two Math" } else { "120" };
        let source = format!("row(office.mathPr(office.{key}(val: \"{value}\")), office.oMath(frac(a, b)))");
        let document = math_document::parse(&source, "dsl", true).unwrap();
        let target = document.nodes.iter().find(|node| node.kind == key).unwrap();
        assert_eq!(math_document::patch(&source, "dsl", true, &document.revision, &MathPatch::SetAttribute {
            node_id: target.id.clone(), name: "m:val".into(), value: None,
        }).unwrap_err(), "math_invalid_property");
        assert_eq!(document.source, source, "Rejected removal must preserve the required source value");
    }
}

#[test]
fn native_display_capsules_keep_actual_display_ast_before_and_after_readable_save() {
    let xml = format!(r#"<w:styles xmlns:w="{}"><w:docDefaults><w:rPrDefault><w:rPr><w:b/><w:color w:val="336699"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r><m:r><w:rPr><w:i/><w:sz w:val="30"/></w:rPr><m:t>y</m:t></m:r></m:oMath></m:oMathPara>"#, math_document::WORD_NS);
    let capsule = math_document::metadata("xy", &xml, "native-display-carrier").unwrap();
    for gap in ["", " ", "\n", "\n\n"] {
        let source = format!("Before\n\n$$xy$${gap}{capsule}\n");
        let before = source.clone();
        let saved = crate::math_migration::plan(&source).unwrap();
        assert!(saved.recoveries.is_empty());
        assert!(saved.diagnostics.is_empty());
        assert!(saved.content.contains("word.styles("));
        for current in [&source, &saved.content] {
            let document = crate::md_ast::parse(current);
            let display = document.blocks.iter().filter_map(|block| if let crate::md_ast::Block::OfficeMath { omml, .. } = block { Some(omml) } else { None }).collect::<Vec<_>>();
            assert_eq!(display.len(), 1, "Native $$ display was demoted to an inline carrier: {current}");
            let mut inline = 0;
            crate::md_ast::walk_inlines(&document.blocks, &mut |node| {
                if matches!(node, crate::md_ast::Inline::OfficeMath { .. }) { inline += 1; }
            });
            assert_eq!(inline, 0);
            let output = package(current);
            assert!(!output.parts.is_empty(), "{:?}", output.warns);
            assert_eq!(output.warns,[crate::api_codes::WARN_MATH_RUN_STYLE_MATERIALIZED],"Nonempty source run defaults retain effective values with an exact inheritance warning");
            let view = omml::parse(&part(&output, "word/document.xml")).unwrap();
            assert_eq!(nodes(&view, "oMathPara").len(), 1);
            assert_eq!(nodes(&view, "oMath").len(), 1);
            let runs = nodes(&view, "r").iter().map(|node| omml::run_style(node)).collect::<Vec<_>>();
            assert_eq!(runs.len(), 2);
            assert!(runs.iter().all(|run| run.bold && run.color == Some([0x33, 0x66, 0x99])));
            assert_eq!(runs[0].half_points, Some(24));
            assert_eq!(runs[1].half_points, Some(30));
            assert_eq!(runs[1].word_italic, Some(true));
        }
        assert_eq!(source, before);
        assert_eq!(crate::math_migration::plan(&saved.content).unwrap().content, saved.content);
    }
    let multiline = format!("$$\nxy\n$${capsule}\n");
    assert!(matches!(crate::md_ast::parse(&multiline).blocks.as_slice(), [crate::md_ast::Block::OfficeMath { latex, .. }] if latex == "xy"));
    assert!(!package(&multiline).parts.is_empty());
}

#[test]
fn display_capsule_recognition_never_promotes_inline_math_or_swallows_visible_body() {
    let xml = format!("<m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara>", run("x"));
    let capsule = math_document::metadata("x", &xml, "native-inline-protection").unwrap();
    for source in [
        format!("$x${capsule}\n"), format!("$x$\n\n{capsule}\n"), format!("Before $$x$${capsule}\n"),
        format!("$$x$${capsule} after\n"), format!("$$x$${capsule} $$y$$\n"),
        format!("$$x$${capsule}{capsule}\n"),
    ] {
        let document = crate::md_ast::parse(&source);
        assert!(!document.blocks.iter().any(|block| matches!(block, crate::md_ast::Block::OfficeMath { .. })), "Mixed or inline carrier was promoted: {source}");
        let mut inline = 0;
        crate::md_ast::walk_inlines(&document.blocks, &mut |node| {
            if matches!(node, crate::md_ast::Inline::OfficeMath { .. }) { inline += 1; }
        });
        assert_eq!(inline, 1, "The inline rejection must reach its real OfficeMath AST");
        let output = package(&source);
        assert!(output.parts.is_empty());
        assert_eq!(output.warns, ["math_invalid_source"]);
    }
    for tail in [" visible", "<!-- unrelated -->", "<!--readmd:math:v1:broken-->"] {
        let source = format!("$$x$${tail}\n");
        let document = crate::md_ast::parse(&source);
        assert!(!document.blocks.iter().any(|block| matches!(block, crate::md_ast::Block::OfficeMath { .. })));
        assert!(matches!(document.blocks.as_slice(), [crate::md_ast::Block::Paragraph(_)]));
    }
    let xml = format!("<m:oMath>{}</m:oMath>", run("x"));
    let capsule = math_document::metadata("x", &xml, "native-inline-math").unwrap();
    for gap in ["", "\n\n"] {
        let source = format!("$x${gap}{capsule}\n");
        let document = crate::md_ast::parse(&source);
        assert!(matches!(document.blocks.as_slice(), [crate::md_ast::Block::Paragraph(inlines)] if matches!(inlines.as_slice(), [crate::md_ast::Inline::OfficeMath { .. }])));
        let output = package(&source);
        assert!(!output.parts.is_empty(), "{:?}", output.warns);
        let view = omml::parse(&part(&output, "word/document.xml")).unwrap();
        assert_eq!(nodes(&view, "oMath").len(), 1);
        assert!(nodes(&view, "oMathPara").is_empty(), "The native oMath inline mode must not become display");
    }
}


#[test]
fn nested_mathpr_is_rejected_in_native_and_legacy_carriers_by_uri_and_location() {
    for prefix in ["m", "q"] {
        let pr = format!("<{prefix}:mathPr><{prefix}:smallFrac {prefix}:val=\"bogus\"/></{prefix}:mathPr>");
        for xml in [
            format!("<{prefix}:oMath xmlns:{prefix}=\"{MATH_NS}\">{pr}<{prefix}:r><{prefix}:t>x</{prefix}:t></{prefix}:r></{prefix}:oMath>"),
            format!("<{prefix}:oMathPara xmlns:{prefix}=\"{MATH_NS}\">{pr}<{prefix}:oMath><{prefix}:r><{prefix}:t>x</{prefix}:t></{prefix}:r></{prefix}:oMath></{prefix}:oMathPara>"),
            format!("<{prefix}:oMath xmlns:{prefix}=\"{MATH_NS}\"><{prefix}:f><{prefix}:num>{pr}<{prefix}:r><{prefix}:t>x</{prefix}:t></{prefix}:r></{prefix}:num><{prefix}:den><{prefix}:r><{prefix}:t>y</{prefix}:t></{prefix}:r></{prefix}:den></{prefix}:f></{prefix}:oMath>"),
        ] {
            let root = math_document::parse_xml(&xml).unwrap();
            assert_eq!(MathDefaultsState::from_xml(&root).unwrap_err(), "math_invalid_property");
            assert_eq!(MathDefaultsState::from_xml(root.children().next().unwrap()).unwrap_err(), "math_invalid_property");
            let capsule = math_document::metadata("x", &xml, "nested-defaults").unwrap();
            assert!(math_document::read_metadata(&capsule, "x").unwrap().1);
            let source = format!("$$x$${capsule}\n");
            assert!(matches!(crate::md_ast::parse(&source).blocks.as_slice(), [crate::md_ast::Block::OfficeMath { .. }]));
            let output = package(&source);
            assert!(output.parts.is_empty());
            assert_eq!(output.warns, ["math_invalid_property"]);
            let mut native = crate::md_ast::parse("Before");
            native.blocks.push(crate::md_ast::Block::OfficeMath { latex: "x".into(), omml: xml.clone() });
            let output = crate::docx_writer::build(&native, &json!({}), "nested-native", &|_| None);
            assert!(output.parts.is_empty());
            assert_eq!(output.warns, ["math_invalid_property"]);
            assert_eq!(math_document::read_metadata(&capsule, "x").unwrap().0.omml, xml);
        }
    }
    let forged = "<m:oMath><x:mathPr xmlns:x=\"urn:forged\"><x:smallFrac/></x:mathPr><m:r><m:t>x</m:t></m:r></m:oMath>";
    assert_eq!(MathDefaultsState::from_xml(&math_document::parse_xml(forged).unwrap()).unwrap_err(), "math_invalid_property");
}

#[test]
fn top_level_default_fragments_and_direct_settings_roots_remain_legal_with_real_aliases() {
    for prefix in ["m", "q"] {
        let xml = format!("<{prefix}:mathPr xmlns:{prefix}=\"{MATH_NS}\"><{prefix}:smallFrac {prefix}:val=\"0\"/></{prefix}:mathPr><{prefix}:oMath xmlns:{prefix}=\"{MATH_NS}\"><{prefix}:r><{prefix}:t>x</{prefix}:t></{prefix}:r></{prefix}:oMath>");
        let root = math_document::parse_xml(&xml).unwrap();
        let direct = root.children().find(|node| math(node, "mathPr")).unwrap();
        let before = root.xml();
        assert_eq!(MathDefaultsState::from_xml(&root).unwrap(), MathDefaultsState::from_xml(direct).unwrap());
        let mut merger = DocxDefaultsMerge::default();
        let content = merger.accept(&xml, true).unwrap();
        assert_eq!(nodes(&omml::parse(&content).unwrap(), "oMathPara").len(), 1);
        assert_eq!(MathDefaultsState::from_xml(merger.settings().unwrap()).unwrap().declared["smallFrac"], Declared::Explicit("0".into()));
        assert_eq!(root.xml(), before);
    }
}

#[test]
fn stale_capsules_keep_new_body_math_mode_and_old_values_only_for_recovery() {
    let old = format!("<m:mathPr><m:smallFrac/></m:mathPr><m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara>", run("x"));
    let capsule = math_document::metadata("x", &old, "stale-default-mode").unwrap();
    assert!(!math_document::read_metadata(&capsule, "y").unwrap().1);
    let mut cases = Vec::new();
    for gap in ["", "\n\n"] {
        cases.push((format!("$$y$${gap}{capsule}\n"), true));
        cases.push((format!("$y${gap}{capsule}\n"), false));
    }
    cases.push((format!("Before $$y$${capsule} after\n"), false));
    cases.push((format!("Before $y${capsule} after\n"), false));
    for (source, display) in cases {
        let document = crate::md_ast::parse(&source);
        assert_eq!(document.blocks.iter().filter(|block| matches!(block, crate::md_ast::Block::Math(_))).count(), usize::from(display));
        assert!(!document.blocks.iter().any(|block| matches!(block, crate::md_ast::Block::OfficeMath { .. })));
        let mut carriers = document.blocks.iter().filter(|block| matches!(block, crate::md_ast::Block::Html(html) if html.trim() == capsule)).count();
        crate::md_ast::walk_inlines(&document.blocks, &mut |node| match node {
            crate::md_ast::Inline::OfficeMath { .. } => panic!("Stale native properties became active"),
            crate::md_ast::Inline::Html(html) if html.trim() == capsule => carriers += 1,
            _ => (),
        });
        assert_eq!(carriers, 1, "The stale source must remain available for recovery");
        let output = package(&source);
        assert!(!output.parts.is_empty(), "{:?}", output.warns);
        assert!(output.warns.is_empty());
        let view = omml::parse(&part(&output, "word/document.xml")).unwrap();
        assert_eq!(nodes(&view, "oMath").len(), 1);
        assert_eq!(nodes(&view, "oMathPara").len(), usize::from(display));
        assert_eq!(nodes(&view, "t").iter().map(|node| node.text.as_str()).collect::<String>(), "y");
        assert!(!part(&output, "word/settings.xml").contains("smallFrac"));
    }
    let source = format!("$$y$${capsule}\n\n$$z$$\n");
    let output = package(&source);
    assert!(!output.parts.is_empty(), "Old defaults must not conflict with a new bare formula: {:?}", output.warns);
    assert_eq!(nodes(&omml::parse(&part(&output, "word/document.xml")).unwrap(), "oMathPara").len(), 2);
    let saved = crate::math_migration::plan(&source).unwrap();
    assert_eq!(saved.recoveries.len(), 1);
    assert!(saved.recoveries[0].content.contains("office.smallFrac()"));
    assert!(saved.content.contains("$$y$$") && !saved.content.contains(&capsule));
    let output = package(&saved.content);
    assert!(!output.parts.is_empty());
    assert_eq!(nodes(&omml::parse(&part(&output, "word/document.xml")).unwrap(), "oMathPara").len(), 2);
    assert!(!part(&output, "word/settings.xml").contains("smallFrac"));

    let matched = format!("$$x$${capsule}\n");
    let output = package(&matched);
    assert!(!output.parts.is_empty());
    assert!(part(&output, "word/settings.xml").contains("smallFrac"));
    let output = package(&(matched + "\n$$z$$\n"));
    assert!(output.parts.is_empty());
    assert!(output.warns[0].starts_with("math_document_defaults_conflict: "));
    assert_eq!(serde_json::from_str::<Value>(output.warns[0].split_once(": ").unwrap().1).unwrap()["property"], "smallFrac");
}
