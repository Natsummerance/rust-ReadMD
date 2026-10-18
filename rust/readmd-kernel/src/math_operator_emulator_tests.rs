//! Independently authored operator labels and the existing semantic gap policy.
//! Font advances/GIDs come from the locked FontTools fixture, never this renderer.
use super::*;
use crate::math_document::{self, MathPatch, XmlNode};
use serde_json::{json, Value};

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }
fn run(s: &str, pr: &str) -> String { format!("<m:r><m:rPr>{pr}</m:rPr><m:t>{}</m:t></m:r>", esc(s)) }
fn emulator(s: &str, pr: &str) -> String { format!("<m:box><m:boxPr>{pr}</m:boxPr><m:e>{s}</m:e></m:box>") }
fn xml(s: &str) -> String { format!("<m:oMath xmlns:m=\"{}\" xmlns:w=\"{}\">{s}</m:oMath>", math_document::MATH_NS, math_document::WORD_NS) }
fn source(body: &str) -> String { xml(&(run("a", "") + &emulator(body, "<m:opEmu/><m:noBreak/>") + &run("a", ""))) }
fn formula(s: &str, size: f64) -> Formula { layout_source("", Some(s), false, size).expect("native formula") }
fn near(a: f64, b: f64) { assert!((a-b).abs()<1e-8, "actual={a}, independent expected={b}"); }
fn data() -> Value { serde_json::from_str(include_str!("../tests/fixtures/math-literal-stix.json")).unwrap() }
fn metric<'a>(v: &'a Value, c: char) -> &'a Value { &v["characters"][c.to_string()] }
fn advance(v: &Value, c: char) -> f64 { metric(v,c)["advance"].as_f64().unwrap()/v["unitsPerEm"].as_f64().unwrap() }
fn glyphs(f: &Formula) -> Vec<(u8,u16,f64,f64,f64)> {
    f.items.iter().filter_map(|item| if let Item::Glyph{font,gid,x,y,scale,..}=*item {
        Some((font,gid,x*f.size,y*f.size,scale*f.size))
    } else {None}).collect()
}
fn same(a: &Formula, b: &Formula) {
    near(a.width,b.width); near(a.height,b.height); near(a.depth,b.depth);
    let a=glyphs(a); let b=glyphs(b); assert_eq!(a.len(),b.len());
    for (a,b) in a.into_iter().zip(b) {
        assert_eq!((a.0,a.1),(b.0,b.1));
        for (a,b) in [(a.2,b.2),(a.3,b.3),(a.4,b.4)] {near(a,b);}
    }
}
fn nodes<'a>(n: &'a XmlNode, name: &str) -> Vec<&'a XmlNode> {
    let mut out=Vec::new(); if n.local()==name {out.push(n);} for c in n.children(){out.extend(nodes(c,name));} out
}
fn generations(s: &str) {
    let original=math_document::parse(s,"omml",false).unwrap(); let mut document=original.clone();
    for generation in 1..=3 {
        let tex=crate::math_formats::tex_checked(&document.root).unwrap_or_else(|code|
            panic!("generation={generation} source={s} previous={} checked error={code}",document.root.xml()));
        eprintln!("emulator generation={generation} source={s} tex={tex}");
        document=math_document::parse(&tex,"latex",false).unwrap();
        assert!(document.diagnostics.is_empty(),"generation={generation} tex={tex}");
        for size in [12.,24.] {same(&formula(s,size),&formula(&document.root.xml(),size));}
    }
}

#[test]
fn math_operator_emulator_letter_projection_keeps_independent_binary_gaps() {
    use sha2::{Digest,Sha256};
    let oracle=data();let font=fonts(false).unwrap();
    assert_eq!(format!("{:x}",Sha256::digest(&*font.fonts[0].data)),oracle["fontSha256"].as_str().unwrap());
    let source=source(&run("b","<m:sty m:val=\"p\"/>"));
    let doc=math_document::parse(&source,"omml",false).unwrap();
    assert_eq!(crate::math_formats::tex_checked(&doc.root).unwrap(),r"a\mathbin{\mathrm{b}}a");
    for size in [12.,24.] {
        let f=formula(&source,size);let expected=(2.*advance(&oracle,'𝑎')+advance(&oracle,'b')+8./18.)*size;
        near(f.width,expected);let g=glyphs(&f);assert_eq!(g.len(),3);
        for (item,c) in g.iter().zip(['𝑎','b','𝑎']) {assert_eq!(item.0,0);assert_eq!(item.1,metric(&oracle,c)["gid"].as_u64().unwrap()as u16);near(item.4,size);}
        near(g[1].2,(advance(&oracle,'𝑎')+4./18.)*size);
        near(g[2].2,(advance(&oracle,'𝑎')+advance(&oracle,'b')+8./18.)*size);
        assert!(layout("aba",false,size).unwrap().width<f.width,"The old plain-letter projection lost spacing");
    }
    generations(&source);
}

#[test]
fn math_operator_emulator_relation_and_joined_run_categories_remain_distinct() {
    let oracle=data();
    for (body,kind,count,gaps) in [
        (run("=",""),r"\mathrel",1.,10./18.),
        (run("=","")+&run("=",""),r"\mathrel",2.,15./18.),
        (run("==",""),r"\mathbin",2.,8./18.),
    ] {
        let source=source(&body);let doc=math_document::parse(&source,"omml",false).unwrap();
        assert!(crate::math_formats::tex_checked(&doc.root).unwrap().contains(kind));
        for size in [12.,24.] {near(formula(&source,size).width,(2.*advance(&oracle,'𝑎')+count*advance(&oracle,'=')+gaps)*size);}
        generations(&source);
    }
}

#[test]
fn math_operator_emulator_nested_atoms_and_unary_contexts_keep_scope() {
    let ordinary=emulator(&run("=",""),"<m:noBreak/>");
    let relation=emulator(&run("=",""),"<m:opEmu/><m:noBreak/>");
    for body in [ordinary,relation] {generations(&source(&body));}
    let op=emulator(&run("b",""),"<m:opEmu/><m:noBreak/>");
    for body in [op.clone()+&run("a",""),run("a","")+&op,op.clone(),run("+","")+&op+&run("a","")] {
        generations(&xml(&body));
    }
    let tex=r"a\mathbin{b}+c";let doc=math_document::parse(tex,"latex",false).unwrap();
    assert_eq!(nodes(&doc.root,"box").len(),1);assert_eq!(nodes(&doc.root,"t").iter().map(|n|n.text()).collect::<String>(),"ab+c");
}

#[test]
fn math_operator_emulator_run_boundaries_styles_and_original_literal_text_roundtrip() {
    for style in ["","<m:sty/>","<m:sty m:val=\"p\"/>","<m:sty m:val=\"i\"/>","<m:sty m:val=\"b\"/>","<m:sty m:val=\"bi\"/>"] {
        for text in ["sin","a+b","a=b","{}%#&_\\^$~-"] {
            generations(&source(&run(text,style)));
        }
        generations(&source(&run("sin +/=-{}%#&_\\^$~",&format!("<m:lit/>{style}"))));
    }
    for text in ["a b","a\u{2009}b","a\u{2005}b","a\u{2003}b"] {generations(&source(&run(text,"")));}
    let outside=run("a","<m:sty m:val=\"b\"/>")+&emulator(&run("b",""),"<m:opEmu/><m:noBreak/>")+&run("sin","<m:sty m:val=\"i\"/>");
    generations(&xml(&outside));
}

#[test]
fn math_operator_emulator_scripts_use_the_existing_zero_gap_rule() {
    let oracle=data();let inside=run("a","")+&emulator(&run("+",""),"<m:opEmu/><m:noBreak/>")+&run("b","");
    let source=xml(&format!("<m:sSup><m:e>{}</m:e><m:sup>{inside}</m:sup></m:sSup>",run("a","")));
    generations(&source);
    for body in [
        format!("<m:sSub><m:e>{}</m:e><m:sub>{inside}</m:sub></m:sSub>",run("a","")),
        format!("<m:sSubSup><m:e>{}</m:e><m:sub>{inside}</m:sub><m:sup>{inside}</m:sup></m:sSubSup>",run("a","")),
    ]{generations(&xml(&body));}
    // Independently compare script glyph starts: operators add no 4/18 gap
    // at script level. Use the font's actual advances and observed script size.
    let f=formula(&source,12.);let g=glyphs(&f);assert_eq!(g.len(),4);
    near(g[2].2-g[1].2,advance(&oracle,'𝑎')*g[1].4);
    near(g[3].2-g[2].2,advance(&oracle,'+')*g[2].4);
}

#[test]
fn math_operator_emulator_boolean_switches_and_breakable_source_keep_legacy_layout() {
    let body=run("b","");let ordinary=xml(&(run("a","")+&emulator(&body,"<m:noBreak/>")+&run("a","")));
    for value in ["0","false","off"] {
        let source=xml(&(run("a","")+&emulator(&body,&format!("<m:opEmu m:val=\"{value}\"/><m:noBreak/>"))+&run("a","")));
        for size in [12.,24.] {same(&formula(&source,size),&formula(&ordinary,size));}
        assert!(!crate::math_formats::tex_checked(&math_document::parse(&source,"omml",false).unwrap().root).unwrap().contains(r"\mathbin"));
    }
    for value in ["","1","true","on"] {
        let property=if value.is_empty(){"<m:opEmu/>".into()}else{format!("<m:opEmu m:val=\"{value}\"/>")};
        let source=xml(&(run("a","")+&emulator(&body,&(property+"<m:noBreak/>"))+&run("a","")));
        generations(&source);
    }
    let breakable=xml(&emulator(&(run("a","")+&run("+","")+&run("b","")),"<m:opEmu/><m:noBreak m:val=\"false\"/>"));
    let doc=math_document::parse(&breakable,"omml",false).unwrap();assert_eq!(doc.source,breakable);
    assert!(formula(&breakable,12.).width>0.);
    assert_eq!(crate::math_formats::tex_checked(&doc.root).unwrap_err(),"math_unsupported_extension");
    assert!(crate::math_dsl::decompile(&doc.root).unwrap().contains("noBreak"));
}

#[test]
fn math_operator_emulator_checked_projection_rejects_unrepresented_properties() {
    for property in ["<m:opEmu/>","<m:opEmu/><m:noBreak/><m:diff/>","<m:opEmu/><m:noBreak/><m:aln/>","<m:opEmu/><m:noBreak/><m:brk/>","<m:opEmu/><m:noBreak/><m:ctrlPr/>"] {
        let source=xml(&emulator(&run("b",""),property));let doc=math_document::parse(&source,"omml",false).unwrap();
        assert_eq!(crate::math_formats::tex_checked(&doc.root).unwrap_err(),"math_unsupported_extension");assert_eq!(doc.source,source);
    }
    for body in [run("sin","<m:nor/>"),run("--","<m:nor/>"),run("-","<m:nor/><m:sty m:val=\"p\"/>"),run("b","<m:scr m:val=\"sans-serif\"/>"),"<m:r><w:rPr><w:sz w:val=\"36\"/></w:rPr><m:t>b</m:t></m:r>".into(),"<m:f><m:num/><m:den/></m:f>".into()] {
        let source=source(&body);let doc=math_document::parse(&source,"omml",false).unwrap();
        assert_eq!(crate::math_formats::tex_checked(&doc.root).unwrap_err(),"math_unsupported_extension");
    }
    for attributes in ["width=\"1pt\"","width=\"1em\" height=\"0em\"","width=\"1em\" depth=\"0em\"","width=\"1em\" mathsize=\"18pt\""] {
        let space=format!("<mspace xmlns=\"{}\" {attributes}/>",crate::math_spacing::MATHML_NS);
        let doc=math_document::parse(&source(&space),"omml",false).unwrap();
        assert_eq!(crate::math_formats::tex_checked(&doc.root).unwrap_err(),"math_unsupported_extension");
    }
}

#[test]
fn math_operator_emulator_new_atom_commands_are_bounded_and_do_not_guess_categories() {
    for (source,code) in [(r"\mathrel{x}","math_unsupported_extension"),(r"\mathbin{=}","math_unsupported_extension"),(r"\mathbin x","math_unsupported_extension"),(r"\mathrel{=","math_invalid_source"),(r"\mathbin{\unknown{x}}","math_unknown_macro"),(r"\mathbin{a\mkern-1mu b}","math_unsupported_extension")] {
        assert_eq!(math_document::parse(source,"latex",false).unwrap_err(),code,"{source}");
    }
    for source in [r"\mathbin{x}",r"\mathbin{a+b}",r"\mathrel{=}",r"\mathrel{\mathrel{=}}"] {
        let doc=math_document::parse(source,"latex",false).unwrap();assert!(doc.diagnostics.is_empty());
        let native=crate::latex2omml::latex_to_native_omml(source,false).unwrap();assert!(!native.contains(crate::math_spacing::MATHML_NS));
    }
    for source in [r"\mathbinary{x}",r"\mathrelative{=}",r"\text{\mathbin{x}}",r"\operatorname{\mathrel{=}}"] {assert!(!crate::latex2omml::has_math_extensions(source),"Exact token only: {source}");}
    let nested=format!("{}x{}",r"\mathbin{".repeat(200),"}".repeat(200));
    assert_eq!(math_document::parse(&nested,"latex",false).unwrap_err(),"math_budget");
}

#[test]
fn math_operator_emulator_source_slots_patch_only_the_selected_duplicate_after_astral() {
    let source=r"𝑎\mathbin{b}+\mathbin{b}";let doc=math_document::parse(source,"latex",false).unwrap();
    let boxes=doc.nodes.iter().filter(|n|n.tex.starts_with(r"\mathbin{")).collect::<Vec<_>>();assert_eq!(boxes.len(),2);
    assert_eq!(boxes[0].from,2);assert_eq!(boxes[0].kind,"structure");assert_eq!(boxes[1].slots[0].role,"e");
    let changed=math_document::patch(source,"latex",false,&doc.revision,&MathPatch::ReplaceSlot{node_id:boxes[1].id.clone(),role:"e".into(),text:"c".into()}).unwrap();
    assert_eq!(changed.source,r"𝑎\mathbin{b}+\mathbin{c}");assert_eq!(doc.source,source);
    assert_eq!(math_document::patch(&changed.source,"latex",false,&doc.revision,&MathPatch::ReplaceSlot{node_id:boxes[1].id.clone(),role:"e".into(),text:"d".into()}).unwrap_err(),"transaction_conflict");
    let dsl=crate::math_dsl::decompile(&changed.root).unwrap();let (back,_)=crate::math_dsl::compile(&dsl,false).unwrap();assert_eq!(math_document::xml_semantics(&back),math_document::xml_semantics(&changed.root));
}

#[test]
fn math_operator_emulator_invalid_class_and_kern_failures_preserve_existing_exports() {
    let dir=tempfile::tempdir().unwrap();
    for extension in ["pdf","docx","tex"] {
        for source in [r"a\mathrel{x}b",r"a\mathbin{x\mkern0mu}b"] {
            let target=dir.path().join(format!("owned.{extension}"));std::fs::write(&target,b"owned existing target").unwrap();let content=format!("${source}$");
            let error=match extension{
                "pdf" if source.contains("mkern")=>continue,
                "pdf"=>crate::mdexport::export_pdf(&content,"",target.to_str().unwrap(),&json!({}),"Operator",std::path::Path::new("")),
                "docx"=>crate::mdexport::export_docx(&content,"",target.to_str().unwrap(),&json!({}),"Operator"),
                _=>crate::mdexport::export_tex(&content,"",target.to_str().unwrap(),&json!({}),"Operator"),
            }.unwrap_err();
            assert_eq!(error,"math_unsupported_extension");assert_eq!(std::fs::read(&target).unwrap(),b"owned existing target");
        }
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),3);
    let inline=json!([{"type":"paragraph","text":[{"t":"math","latex":r"a\mathbin{x\mkern18000mu}b","fallback":true}]}]);
    let mut warns=vec![];let error=crate::pdf_render::build(&inline,&json!({"typography":{"size":24}}),"",&|_|None,&mut warns).unwrap_err();
    assert_eq!(error.message,"math_budget","The new class macro cannot bypass actual-size spacing guards");
}

#[test]
fn math_operator_emulator_plain_tex_and_finite_equation_array_boundaries_stay_explicit() {
    for text in ["a+b",r"\backslash",r"\frac{a}{b}",r"\sin x"] {
        assert!(!crate::latex2omml::has_math_extensions(text));let doc=math_document::parse(text,"latex",false).unwrap();assert!(doc.diagnostics.is_empty());
    }
    let source=xml(&format!("<m:eqArr><m:eqArrPr><m:maxDist/></m:eqArrPr><m:e>{}</m:e><m:e>{}</m:e></m:eqArr>",run("a&b&x&y",""),emulator(&run("a&b&x&y",""),"<m:opEmu/><m:noBreak/>")));
    let root=omml::parse(&source).unwrap();let projection=omml::to_mathml_constrained(&root,true,"",12.,Some(96.)).unwrap();
    assert!(projection.profile.is_none(),"Structured emulators are not part of the finite plain-token profile");
    assert_eq!(projection.warnings,vec!["math_equation_array_mathml_distribution"]);
    assert_eq!(projection.output,omml::to_mathml(&root,true,""));
    assert!(projection.output.contains("&amp;"));
}
