//! Author-defined independent font metrics plus the ISO literal-plus example.
//! These tests do not use rendered output as an expected value or certify Word.
use super::*;
use crate::math_document::{self, MathPatch, XmlNode};
use serde_json::{json, Value};

fn esc(text: &str) -> String { text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }
fn oracle() -> Value { serde_json::from_str(include_str!("../tests/fixtures/math-literal-stix.json")).unwrap() }
fn record(data: &Value, c: char) -> &Value { &data["characters"][c.to_string()] }
fn advance(data: &Value, c: char) -> f64 { record(data,c)["advance"].as_f64().unwrap()/data["unitsPerEm"].as_f64().unwrap() }
fn near(actual: f64, expected: f64) { assert!((actual-expected).abs()<1e-8,"actual={actual}, independent expected={expected}"); }
fn run(text: &str, properties: &str, points: Option<u16>) -> String {
    format!("<m:r><m:rPr>{properties}</m:rPr>{}<m:t>{}</m:t></m:r>",points.map(|s|format!("<w:rPr><w:sz w:val=\"{}\"/></w:rPr>",s*2)).unwrap_or_default(),esc(text))
}
fn xml(body: &str) -> String { format!("<m:oMath xmlns:m=\"{}\" xmlns:w=\"{}\">{body}</m:oMath>",math_document::MATH_NS,math_document::WORD_NS) }
fn formula(source: &str,size:f64) -> Formula { layout_source("",Some(source),false,size).unwrap() }
fn glyphs(f:&Formula) -> Vec<(u8,u16,f64,f64,f64)> {
    f.items.iter().filter_map(|item|if let Item::Glyph{font,gid,x,y,scale,..}=*item{Some((font,gid,x*f.size,y*f.size,scale*f.size))}else{None}).collect()
}
fn same_glyph_geometry(left: &Formula, right: &Formula) {
    let a = glyphs(left); let b = glyphs(right); assert_eq!(a.len(), b.len());
    for (a,b) in a.into_iter().zip(b) {
        assert_eq!((a.0,a.1),(b.0,b.1));
        for (actual,expected) in [(a.2,b.2),(a.3,b.3),(a.4,b.4)] { near(actual,expected); }
    }
}
fn nodes<'a>(root:&'a XmlNode,name:&str) -> Vec<&'a XmlNode> {
    let mut out=Vec::new();if root.local()==name{out.push(root);}for child in root.children(){out.extend(nodes(child,name));}out
}

#[test]
fn math_literal_normative_plus_changes_spacing_only_at_actual_sizes() {
    use sha2::{Digest,Sha256};
    let data=oracle();let font=fonts(false).unwrap();
    assert_eq!(format!("{:x}",Sha256::digest(&*font.fonts[0].data)),data["fontSha256"].as_str().unwrap());
    for base in [12.,24.] {for physical in [None,Some(18)] {
        let points=physical.map(f64::from).unwrap_or(base);
        let source=|literal:&str|xml(&(run("a","<m:sty m:val=\"p\"/>",physical)+&run("+",literal,physical)+&run("b","<m:sty m:val=\"p\"/>",physical)));
        let ordinary=formula(&source(""),base);
        let raw=(advance(&data,'a')+advance(&data,'+')+advance(&data,'b'))*points;
        near(ordinary.width,raw+8./18.*points);
        for switch in ["<m:lit/>","<m:lit m:val=\"1\"/>","<m:lit m:val=\"true\"/>","<m:lit m:val=\"on\"/>"] {
            let f=formula(&source(switch),base);near(f.width,raw);
            let paint=glyphs(&f);let old=glyphs(&ordinary);assert_eq!(paint.len(),3);
            for (index,c) in ['a','+','b'].into_iter().enumerate(){
                assert_eq!(paint[index].0,0);assert_eq!(paint[index].1,record(&data,c)["gid"].as_u64().unwrap()as u16);
                near(paint[index].4,points);assert_eq!((paint[index].0,paint[index].1,paint[index].3,paint[index].4),(old[index].0,old[index].1,old[index].3,old[index].4));
            }
            near(paint[1].2,advance(&data,'a')*points);near(paint[2].2,(advance(&data,'a')+advance(&data,'+'))*points);
            near(f.height,ordinary.height);near(f.depth,ordinary.depth);
        }
        for switch in ["<m:lit m:val=\"0\"/>","<m:lit m:val=\"false\"/>","<m:lit m:val=\"off\"/>"] {let f=formula(&source(switch),base);near(f.width,ordinary.width);assert_eq!(glyphs(&f),glyphs(&ordinary));}
    }}
}

#[test]
fn math_literal_letters_keep_math_typefaces_and_do_not_infer_a_function() {
    let data=oracle();
    for size in [12.,24.] {
        let ordinary=formula(&xml(&run("sin","",None)),size);let literal=formula(&xml(&run("sin","<m:lit/>",None)),size);
        for ((_,gid,_,_,scale),c) in glyphs(&literal).into_iter().zip(['𝑠','𝑖','𝑛']) {assert_eq!(gid,record(&data,c)["gid"].as_u64().unwrap()as u16);near(scale,size);}
        near(literal.width,['𝑠','𝑖','𝑛'].into_iter().map(|c|advance(&data,c)).sum::<f64>()*size);
        assert_ne!(glyphs(&literal).iter().map(|g|g.1).collect::<Vec<_>>(),glyphs(&ordinary).iter().map(|g|g.1).collect::<Vec<_>>());
        for style in ["p","i","b","bi"] {
            let properties=format!("<m:sty m:val=\"{style}\"/>");
            let off=formula(&xml(&run("sin",&properties,None)),size);let on=formula(&xml(&run("sin",&format!("<m:lit/>{properties}"),None)),size);
            assert_eq!(glyphs(&off),glyphs(&on));near(off.width,on.width);
        }
        let nor="<m:nor/>";
        let off=formula(&xml(&run("sin + a",nor,None)),size);let on=formula(&xml(&run("sin + a",&format!("<m:lit/>{nor}"),None)),size);
        assert_eq!(glyphs(&off),glyphs(&on));near(off.width,on.width);
        let ml=omml::to_mathml(&omml::parse(&xml(&run("sin + a",&format!("<m:lit/>{nor}"),None))).unwrap(),false,"");assert!(ml.contains("<mtext>sin + a</mtext>"));
    }
}

#[test]
fn math_literal_spaces_hyphen_and_mathml_tokens_preserve_original_content() {
    let data=oracle();
    for size in [12.,24.] {for space in [' ','\u{2009}','\u{2005}','\u{2003}'] {
        let text=format!("a{space}b");let source=xml(&run(&text,"<m:lit/><m:sty m:val=\"p\"/>",None));let f=formula(&source,size);
        near(f.width,(advance(&data,'a')+advance(&data,space)+advance(&data,'b'))*size);assert_eq!(f.items.len(),2,"Whitespace reserves advance without painted glyphs");
        let ml=omml::to_mathml(&omml::parse(&source).unwrap(),false,"");let tree=math_document::parse_xml(&ml).unwrap();let spaces=nodes(&tree,"mspace");assert_eq!(spaces.len(),1);
        near(spaces[0].attributes["width"].trim_end_matches("em").parse::<f64>().unwrap(),advance(&data,space));assert!(nodes(&tree,"mo").is_empty());
    }}
    let f=formula(&xml(&run("-","<m:lit/>",None)),12.);assert_eq!(glyphs(&f)[0].1,record(&data,'-')["gid"].as_u64().unwrap()as u16);near(f.width,advance(&data,'-')*12.);
    let source=xml(&run("12+/-=","<m:lit/>",None));let ml=omml::to_mathml(&omml::parse(&source).unwrap(),false,"");let tree=math_document::parse_xml(&ml).unwrap();
    assert!(nodes(&tree,"mo").is_empty());assert_eq!(nodes(&tree,"mn").len(),2);assert_eq!(nodes(&tree,"mi").iter().map(|n|n.text()).collect::<String>(),"+/-=");
}

#[test]
fn math_literal_flow_classes_and_transparent_phantoms_keep_operator_scope() {
    let middle=|property:&str|xml(&(run("a","",None)+&format!("<m:phant><m:phantPr><m:show/><m:transp/></m:phantPr><m:e>{}</m:e></m:phant>",run("+",property,None))+&run("b","",None)));
    let ordinary=omml::parse(&middle("")).unwrap();let literal=omml::parse(&middle("<m:lit/>")).unwrap();
    for (root,class) in [(&ordinary,omml::Class::Bin),(&literal,omml::Class::Ord)] {let plans=omml::spacing_plans(&root.child("oMath").unwrap().children,omml::SpacingContext::default());assert_eq!(plans[1].first,Some(class));assert_eq!(plans[1].last,Some(class));}
    near(formula(&middle(""),12.).width-formula(&middle("<m:lit/>"),12.).width,8./18.*12.);
    let font=fonts(false).unwrap();let style=Style{display:false,level:0,cramped:false};
    for (property,kind) in [("","alignment-operator"),("<m:lit/>","flow-character")] {
        let source=omml::parse(&xml(&run("+",property,None))).unwrap();let mut context=Ctx::new(&font,12.);let b=context.run(source.child("oMath").unwrap().child("r").unwrap(),style);
        assert_eq!(b.regions.iter().filter(|r|r.part&&r.character==Some('+')).map(|r|r.kind.as_str()).collect::<Vec<_>>(),vec![kind]);
    }
    let manual=omml::parse(&xml(&(run("a","",None)+&run("+","<m:lit/><m:brk/>",None)+&run("b","",None)))).unwrap();
    assert_eq!(omml::manual_math_lines(&manual.child("oMath").unwrap().children).len(),2,"Literal disables inferred operator breaks, not the author's explicit break");
}

#[test]
fn math_literal_lossless_dsl_properties_repeated_runs_and_utf16_edits() {
    for lit in ["<m:lit/>","<m:lit m:val=\"false\"/>",""] {
        let source=xml(&(run("𝑎 +b",&format!("{lit}<m:sty m:val=\"p\"/>"),Some(18))+&run("𝑎 +b",&format!("{lit}<m:sty m:val=\"p\"/>"),Some(18))));
        let doc=math_document::parse(&source,"omml",false).unwrap();let dsl=crate::math_dsl::decompile(&doc.root).unwrap();let(back,_)=crate::math_dsl::compile(&dsl,false).unwrap();assert_eq!(math_document::xml_semantics(&doc.root),math_document::xml_semantics(&back));
        let text=nodes(&doc.root,"t")[1];let id=text.id.clone();
        let changed=math_document::patch(&source,"omml",false,&doc.revision,&MathPatch::ReplaceTextRange{node_id:id,from:3,to:4,text:"=".into()}).unwrap();
        assert_eq!(nodes(&changed.root,"t").iter().map(|n|n.text()).collect::<Vec<_>>(),["𝑎 +b","𝑎 =b"]);assert_eq!(doc.source,source);
        assert_eq!(math_document::patch(&changed.source,"omml",false,&doc.revision,&MathPatch::ReplaceTextRange{node_id:text.id.clone(),from:0,to:2,text:"x".into()}).unwrap_err(),"transaction_conflict");
    }
    let source=xml(&run("𝑎 +b","<m:lit/>",None));let doc=math_document::parse(&source,"omml",false).unwrap();
    let svg=math_document::vector_preview(&doc,false,12.).unwrap();let tree=math_document::parse_xml(&svg).unwrap();
    let spans=nodes(&tree,"g").iter().filter(|n|n.attributes.get("data-math-node").is_some_and(|v|v=="run-text")).map(|n|(n.attributes["data-math-text-from"].clone(),n.attributes["data-math-text-to"].clone())).collect::<Vec<_>>();
    assert!(spans.contains(&("0".into(),"2".into())));assert!(spans.contains(&("2".into(),"3".into())));assert!(spans.contains(&("3".into(),"4".into())));assert!(spans.contains(&("4".into(),"5".into())));
}

#[test]
fn math_literal_tex_projection_is_editable_scoped_and_escape_safe() {
    for properties in ["<m:lit/>","<m:lit/><m:sty m:val=\"p\"/>","<m:lit/><m:sty m:val=\"i\"/>","<m:lit/><m:sty m:val=\"b\"/>"] {
        let source=xml(&run("sin +/=-{}%#&_\\^$~",properties,None));let doc=math_document::parse(&source,"omml",false).unwrap();let tex=crate::math_formats::tex(&doc.root);
        assert!(tex.contains("\\mathord{+}"));assert!(tex.contains("\\mkern"));let edited=math_document::parse(&tex,"latex",false).unwrap();assert!(edited.diagnostics.is_empty(),"{tex}: {:?}",edited.diagnostics);
        for size in [12.,24.] {let a=formula(&source,size);let b=layout(&tex,false,size).unwrap();assert!((a.width-b.width).abs()<1e-8,"first projection properties={properties} size={size} tex={tex} original={:?} projected={:?}",glyphs(&a),glyphs(&b));same_glyph_geometry(&a,&b);}
        let again=crate::math_formats::tex(&edited.root);let roundtrip=math_document::parse(&again,"latex",false).unwrap();
        let before=formula(&edited.root.xml(),12.);let after=formula(&roundtrip.root.xml(),12.);
        assert!((before.width-after.width).abs()<1e-8,"second projection properties={properties} tex={again} before_width={} after_width={} before={:?} after={:?}",before.width,after.width,glyphs(&before),glyphs(&after));same_glyph_geometry(&before,&after);
    }
    let source=r"a\mathord{+}b+c";let doc=math_document::parse(source,"latex",false).unwrap();let root=omml::parse(&doc.root.xml()).unwrap();let plans=omml::spacing_plans(&root.child("oMath").unwrap().children,omml::SpacingContext::default());
    assert_eq!(plans[1].first,Some(omml::Class::Ord));assert_eq!(plans[3].first,Some(omml::Class::Bin));
    let nested=layout(r"\mathord{a+b}",false,12.).unwrap();let normal=layout("a+b",false,12.).unwrap();near(nested.width,normal.width);
    let ml=omml::to_mathml(&root,false,"");assert!(ml.contains("<mo lspace=\"0em\" rspace=\"0em\">+</mo>"));
}

#[test]
fn math_literal_escaped_style_groups_keep_braces_and_neighbours_on_reprojection() {
    // Each source character is independently literal. Reprojection nests
    // per-run font commands inside mathord, so their escaped brace is not a
    // group delimiter. The entire extension parse must retain this rule in
    // child frames that no longer contain a mathord token themselves.
    for style in ["p","i","b","bi"] {for text in ["{","}","{}","{a}","a{+}b",r"\{a}\",r"^$~"] {
        let properties=format!("<m:lit/><m:sty m:val=\"{style}\"/>");
        let source=xml(&run(text,&properties,None));let original=math_document::parse(&source,"omml",false).unwrap();
        let mut doc=original.clone();
        for generation in 1..=3 {
            let tex=crate::math_formats::tex(&doc.root);doc=math_document::parse(&tex,"latex",false).unwrap();
            assert!(doc.diagnostics.is_empty(),"style={style} text={text:?} generation={generation} tex={tex}");
            assert_eq!(nodes(&doc.root,"t").iter().map(|n|n.text()).collect::<String>(),text,"style={style} generation={generation} tex={tex}");
            for size in [12.,24.] {
                let a=formula(&original.root.xml(),size);let b=formula(&doc.root.xml(),size);
                assert!((a.width-b.width).abs()<1e-8,"style={style} text={text:?} generation={generation} size={size} tex={tex} original_width={} projected_width={} original={:?} projected={:?}",a.width,b.width,glyphs(&a),glyphs(&b));
                same_glyph_geometry(&a,&b);
            }
        }
    }}
    let source=xml(&format!("<m:f><m:num>{}</m:num><m:den>{}</m:den></m:f>{}{}",run("{","<m:lit/><m:sty m:val=\"p\"/>",None),run("}","<m:lit/><m:sty m:val=\"p\"/>",None),run("+","",None),run("b","",None)));
    let original=math_document::parse(&source,"omml",false).unwrap();let tex=crate::math_formats::tex(&original.root);
    let parsed=math_document::parse(&tex,"latex",false).unwrap();assert_eq!(nodes(&parsed.root,"f").len(),1);
    assert_eq!(nodes(&parsed.root,"t").iter().map(|n|n.text()).collect::<String>(),"{}+b");
    for size in [12.,24.] {let a=formula(&source,size);let b=formula(&parsed.root.xml(),size);near(a.width,b.width);same_glyph_geometry(&a,&b);}
    // Nonextended legacy inputs retain their historical adapter byte path.
    for source in [r"\mathrm{a}+b",r"\frac{x}{y}",r"\text{\{not interpreted\}}"] {
        assert_eq!(crate::latex2omml::latex_to_omml_checked(source,false).unwrap(),crate::latex2omml::latex_to_omml(source,false));
    }
}

#[test]
fn math_literal_mkern_units_scope_and_source_slots_are_bounded() {
    // A TeX control word consumes every adjacent letter; it is not a kern.
    let unknown = math_document::parse(r"\mkernNaNmu", "latex", false).unwrap();
    assert!(!crate::latex2omml::has_math_extensions(&unknown.source));
    assert!(unknown.diagnostics.iter().any(|d| d.code == "math_unknown_macro"));
    for (value,em) in [("0mu",0.),(".5mu",0.5/18.),("+18 MU",1.),("1.mu",1./18.),(" 36mu",2.)] {
        let source=format!("𝑎\\mkern{value}b");let doc=math_document::parse(&source,"latex",false).unwrap();assert!(doc.diagnostics.is_empty());
        let space=nodes(&doc.root,"mspace");assert_eq!(space.len(),1);near(crate::math_spacing::Spacing::from_xml(space[0]).unwrap().width.unwrap().value,em);
        let node=doc.nodes.iter().find(|n|n.kind=="spacing").unwrap();assert_eq!(node.from,2);assert_eq!(node.slots[0].role,"width");assert_eq!(node.to,source.encode_utf16().count()-1);
    }
    for (source,code) in [(r"\mkern-1mu","math_unsupported_extension"),(r"\mkern--1mu","math_unsupported_extension"),(r"\mkern\muskip0","math_unsupported_extension"),(r"\mkern1pt","math_invalid_property"),(r"\mkern1em","math_invalid_property"),(r"\mkern NaNmu","math_invalid_property"),(r"\mkern73729mu","math_budget"),(r"\mathord{\unknown{x}}","math_unknown_macro"),(r"\mathord+","math_unsupported_extension"),(r"\mathord{a","math_invalid_source")] {assert_eq!(math_document::parse(source,"latex",false).unwrap_err(),code,"{source}");assert!(omml::from_latex(source,false).is_err());}
    for source in [r"\\mkern-1mu",r"\text{\mkern-1mu}",r"\operatorname{\mkern-1mu}"] {assert!(!crate::latex2omml::has_math_extensions(source));assert!(crate::math_spacing::latex_spacing_xml(source,false).unwrap().is_none());}
    let source="𝑎\\mkern1mu b+\\mkern2mu c";let doc=math_document::parse(source,"latex",false).unwrap();let width=doc.nodes.iter().find(|n|n.kind=="spacing").unwrap();
    let changed=math_document::patch(source,"latex",false,&doc.revision,&MathPatch::ReplaceSlot{node_id:width.id.clone(),role:"width".into(),text:"3mu".into()}).unwrap();assert_eq!(changed.source,"𝑎\\mkern3mu b+\\mkern2mu c");
    let nested=math_document::parse(r"\frac{a\mkern18mu b}{\mathord{c+d}}+e","latex",false).unwrap();assert!(nested.diagnostics.is_empty());assert!(nodes(&nested.root,"f").len()==1);assert!(nodes(&nested.root,"box").len()==1);
    let too_deep=format!("{}a{}","\\mathord{".repeat(130),"}".repeat(130));assert_eq!(math_document::parse(&too_deep,"latex",false).unwrap_err(),"math_budget");
}

#[test]
fn math_literal_ascii_backslash_keeps_the_existing_mathematical_alias() {
    assert_eq!(crate::math_catalog::symbol(r"\backslash"),Some("∖"));
    // The shared escaped() adapter is used for ordinary runs too. Both source
    // scalars retain their own glyph; only the mathematical command means ∖.
    for scalar in [r"\","∖"] {
        let source=xml(&run(scalar,"",None));let original=math_document::parse(&source,"omml",false).unwrap();let mut parsed=original.clone();
        for generation in 1..=3 {
            let tex=crate::math_formats::tex(&parsed.root);
            if scalar==r"\"{assert!(tex.contains(r"\textbackslash"));assert!(!tex.contains(r"\backslash"));}
            parsed=math_document::parse(&tex,"latex",false).unwrap();assert!(parsed.diagnostics.is_empty());
            assert_eq!(nodes(&parsed.root,"t").iter().map(|n|n.text()).collect::<String>(),scalar,"nonliteral generation={generation} tex={tex}");
            for size in [12.,24.] {let a=formula(&original.root.xml(),size);let b=formula(&parsed.root.xml(),size);near(a.width,b.width);same_glyph_geometry(&a,&b);}
        }
    }
    for source in [r"\backslash",r"a\backslash b",r"\mathord{a}\backslash b"] {
        let parsed=math_document::parse(source,"latex",false).unwrap();
        assert!(parsed.diagnostics.is_empty());
        let text=nodes(&parsed.root,"t").iter().map(|n|n.text()).collect::<String>();
        assert_eq!(text,if source==r"\backslash"{"∖"}else{"a∖b"});
        assert!(crate::latex2omml::latex_to_omml(source,false).contains('∖'));
    }
    for properties in ["<m:lit/>","<m:lit/><m:sty m:val=\"p\"/>","<m:lit/><m:sty m:val=\"b\"/>"] {
        let source=xml(&run(r"\∖\{+}",properties,None));
        let original=math_document::parse(&source,"omml",false).unwrap();let mut parsed=original.clone();
        for generation in 1..=3 {
            let tex=crate::math_formats::tex(&parsed.root);
            assert!(tex.contains(r"\textbackslash"),"generation={generation} tex={tex}");
            assert!(!tex.contains(r"\backslash"),"Literal U+005C cannot reuse the U+2216 alias: {tex}");
            parsed=math_document::parse(&tex,"latex",false).unwrap();assert!(parsed.diagnostics.is_empty());
            assert_eq!(nodes(&parsed.root,"t").iter().map(|n|n.text()).collect::<String>(),r"\∖\{+}");
            for size in [12.,24.] {let a=formula(&original.root.xml(),size);let b=formula(&parsed.root.xml(),size);near(a.width,b.width);same_glyph_geometry(&a,&b);}
        }
    }
    let unknown=math_document::parse(r"\mathord{\textbackslashBAD}","latex",false).unwrap_err();
    assert_eq!(unknown,"math_unknown_macro","Only the complete text command token is supported");
    // Source spellings inside the existing literal-text argument remain text.
    let text=math_document::parse(r"\text{\textbackslash}","latex",false).unwrap();
    assert_eq!(nodes(&text.root,"t")[0].text(),r"\textbackslash");
}

#[test]
fn math_literal_new_kern_budget_uses_real_size_and_frozen_retarget_fonts() {
    let source=r"a\mkern18000mu b";let f=layout(source,false,12.).unwrap();assert!(f.source_xml.is_some());
    let data=oracle();near(f.width,(advance(&data,'𝑎')+1000.+advance(&data,'𝑏'))*12.);
    let small=f.spacing_at_size(4.).unwrap().unwrap();assert!(Arc::ptr_eq(&small.fonts,&f.fonts));assert!(small.width<5000.);
    assert_eq!(f.spacing_at_size(24.).unwrap_err(),"math_budget");assert!(layout(source,false,24.).is_none());
    let value=json!({"t":"math","latex":source,"fallback":true});crate::math_spacing::require_vector_for_spacing_at_size(&value,12.).unwrap();assert_eq!(crate::math_spacing::require_vector_for_spacing_at_size(&value,24.).unwrap_err(),"math_budget");
    let office=xml(&run("a+b","",None));crate::math_spacing::require_vector_for_spacing_at_size(&json!({"omml":office,"latex":r"\mkern999999mu","display":false}),12.).unwrap();
    assert!(layout("a+b",false,12.).unwrap().source_xml.is_none());
}

#[test]
fn math_literal_new_kern_exports_reject_before_mutating_existing_targets() {
    let markdown=r"Synthetic $a\mkern18mu b$.";let doc=crate::md_ast::parse(markdown);assert!(crate::math_spacing::document_has_spacing(&doc).unwrap());
    let package=crate::docx_writer::build(&doc,&json!({}),"Literal",&|_|None);assert!(package.parts.is_empty());assert!(package.warns.contains(&"math_unsupported_extension".into()));
    let dir=tempfile::tempdir().unwrap();for extension in ["docx","tex"] {
        let target=dir.path().join(format!("owned.{extension}"));std::fs::write(&target,b"owned original").unwrap();
        let error=if extension=="docx"{crate::mdexport::export_docx(markdown,"",target.to_str().unwrap(),&json!({}),"Literal")}else{crate::mdexport::export_tex(markdown,"",target.to_str().unwrap(),&json!({}),"Literal")}.unwrap_err();
        assert_eq!(error,"math_unsupported_extension");assert_eq!(std::fs::read(target).unwrap(),b"owned original");
    }
    let target=dir.path().join("owned.pdf");std::fs::write(&target,b"owned original PDF").unwrap();
    let blocks=json!([{"type":"heading","level":1,"text":[{"t":"math","latex":r"a\mkern18000mu b","fallback":true}]}]);let style=json!({"typography":{"size":12},"headings":{"h1":{"size":24}}});let mut warns=vec![];
    let error=crate::pdf_render::render(&blocks,target.to_str().unwrap(),&style,dir.path().to_str().unwrap(),&|_|None,&mut warns).unwrap_err();assert_eq!(error.message,"math_budget");assert_eq!(std::fs::read(target).unwrap(),b"owned original PDF");assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),3);
}

#[test]
fn math_literal_new_macros_take_actual_pdf_table_quote_and_note_paths() {
    let inline=json!([{"t":"math","latex":r"a\mkern27000mu b","fallback":true}]);
    for (blocks,style) in [
        (json!([{"type":"table","header":[inline.clone()],"rows":[]}]),json!({"typography":{"size":8},"table":{"cellSize":16}})),
        (json!([{"type":"quote","blocks":[{"type":"paragraph","text":inline.clone()}]}]),json!({"typography":{"size":16}})),
        (json!([{"type":"quote","blocks":[{"type":"heading","level":2,"text":inline.clone()}]}]),json!({"typography":{"size":16}})),
        (crate::md_ast::to_render_json(&crate::md_ast::parse("Text[^n].\n\n[^n]: $a\\mkern27000mu b$\n").blocks),json!({"typography":{"size":16}})),
    ] {let mut warns=vec![];let error=crate::pdf_render::build(&blocks,&style,"",&|_|None,&mut warns).unwrap_err();assert_eq!(error.message,"math_budget","{blocks}");}
    let source=r"a\mkern18mu b";let blocks=json!([{"type":"quote","blocks":[{"type":"heading","level":2,"text":[{"t":"math","latex":source,"fallback":true}]}]}]);let style=json!({"typography":{"size":16}});let mut warns=vec![];
    let document=crate::pdf_render::build(&blocks,&style,"",&|_|None,&mut warns).unwrap();let placed=document.pages.iter().flat_map(|p|&p.items).filter_map(|i|if let crate::pdf_render::DrawItem::Formula{f,..}=i{Some(f)}else{None}).collect::<Vec<_>>();assert_eq!(placed.len(),1);near(placed[0].width,layout(source,false,16.).unwrap().width);
    let low=json!([{"type":"paragraph","text":[{"t":"math","latex":r"a\mkern60000mu b","fallback":true}]}]);let low_style=json!({"typography":{"size":4}});let mut warns=vec![];
    crate::math_spacing::require_vector_for_spacing_at_size(&low,4.).unwrap();assert_eq!(crate::pdf_render::build(&low,&low_style,"",&|_|None,&mut warns).unwrap_err().message,"publication_layout_geometry");
}

#[test]
fn math_literal_native_office_adapter_and_plain_latex_compatibility_are_explicit() {
    for source in [r"a\mkern0mu b",r"a\mkern18mu b",r"\mkern-1mu"] {assert_eq!(crate::latex2omml::latex_to_native_omml(source,false).unwrap_err(),"math_unsupported_extension");}
    let source=r"a\mathord{+}b";let xml=crate::latex2omml::latex_to_native_omml(source,false).unwrap();assert!(xml.contains("<m:noBreak/>"));assert!(!crate::math_spacing::contains_spacing(&math_document::parse_xml(&xml).unwrap()));
    for source in ["a+b",r"\frac{x}{y}",r"\sqrt{x}+\sin x",r"\text{\mkern-1mu}"] {assert_eq!(crate::latex2omml::latex_to_omml_checked(source,false).unwrap(),crate::latex2omml::latex_to_omml(source,false));}
}

#[test]
fn math_literal_equation_array_ambiguity_keeps_source_and_rejects_finite_profile() {
    let source=xml(&format!("<m:eqArr><m:eqArrPr><m:maxDist/></m:eqArrPr><m:e>{}</m:e><m:e>{}</m:e></m:eqArr>",run("a&b&x&y","<m:lit/>",None),run("a&b&x&y","",None)));
    let root=omml::parse(&source).unwrap();let projection=omml::to_mathml_constrained(&root,true,"",12.,Some(96.)).unwrap();assert!(projection.profile.is_none());assert!(projection.warnings.contains(&"math_equation_array_literal_alignment".into()));
    let doc=math_document::parse(&source,"omml",true).unwrap();assert_eq!(doc.source,source);assert_eq!(nodes(&doc.root,"t")[0].text(),"a&b&x&y");assert!(projection.output.contains("&amp;"));
    let alias=source.replace("xmlns:m=","xmlns:q=").replace("m:","q:");let other=omml::parse(&alias).unwrap();assert!(omml::literal_run(other.child("oMath").unwrap().child("eqArr").unwrap().child("e").unwrap().child("r").unwrap()));
    let foreign=xml(&run("+","<m:lit xmlns:x=\"urn:foreign\" x:val=\"false\"/>",None));let parsed=omml::parse(&foreign).unwrap();assert!(omml::literal_run(parsed.child("oMath").unwrap().child("r").unwrap()));assert!(crate::math_formats::tex(&math_document::parse_xml(&foreign).unwrap()).contains("\\mathord{+}"));
}
