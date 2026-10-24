use super::*;
use crate::math_document::{self,XmlNode,WORD_NS,MATH_NS};

fn xml(jc:&str,left:i32,right:i32,enabled:bool,local:&str)->String{
    format!("<w:pPr><w:ind w:left=\"{left}\" w:right=\"{right}\"/><w:jc w:val=\"{jc}\"/></w:pPr><m:mathPr><m:dispDef m:val=\"{}\"/></m:mathPr><m:oMathPara>{local}<m:oMath><m:f><m:num><m:r><m:t>x</m:t></m:r></m:num><m:den><m:r><m:t>y</m:t></m:r></m:den></m:f></m:oMath></m:oMathPara>",u8::from(enabled))
}
fn unit_xml(jc:&str,left:&str,right:&str,enabled:bool,local:&str)->String{
    xml(jc,0,0,enabled,local).replace(r#"w:left="0" w:right="0""#,&format!(r#"w:left="{left}" w:right="{right}""#))
}
fn laid(source:&str,width:Option<f64>)->Formula{layout_source_constrained("",Some(source),true,12.,width).unwrap()}
fn near(a:f64,b:f64){assert!((a-b).abs()<1e-7,"{a} != {b}");}
fn moved(a:&Formula,b:&Formula,delta:f64){
    assert_eq!(a.items.len(),b.items.len());assert!(!a.items.is_empty());
    for (a,b) in a.items.iter().zip(&b.items){match (a,b){
        (Item::Glyph{font:af,gid:ag,x:ax,y:ay,scale:asc,..},Item::Glyph{font:bf,gid:bg,x:bx,y:by,scale:bsc,..})=>{
            assert_eq!((af,ag),(bf,bg));near((bx-ax)*12.,delta);near(*ay,*by);near(*asc,*bsc);
        },
        (Item::Rule{x:ax,y:ay,w:aw,h:ah,..},Item::Rule{x:bx,y:by,w:bw,h:bh,..})=>{near((bx-ax)*12.,delta);near(*ay,*by);near(*aw,*bw);near(*ah,*bh);},
        _=>panic!("Changed formula shape"),
    }}
    near(a.width,b.width);near(a.height,b.height);near(a.depth,b.depth);
}
fn word<'a>(node:&'a XmlNode,name:&str)->Option<&'a XmlNode>{node.children().find(|node|node.namespace==WORD_NS&&node.local()==name)}
fn attr<'a>(node:&'a XmlNode,name:&str)->Option<&'a str>{node.attributes.iter().find(|(key,_)|key.rsplit(':').next()==Some(name)&&node.attribute_namespaces.get(*key).is_some_and(|uri|uri==WORD_NS)).map(|(_,value)|value.as_str())}

#[test]fn word_paragraph_lcr_moves_real_glyphs_and_rules_without_resizing(){
    for (jc,left,right,delta) in [("left",720,0,36.),("right",0,360,-18.),("center",480,240,6.)]{
        let a=laid(&xml(jc,0,0,false,""),Some(420.));let b=laid(&xml(jc,left,right,false,""),Some(420.));
        moved(&a,&b,delta);near(a.width,420.);near(b.width,420.);
    }
}
#[test]fn word_paragraph_units_move_every_real_glyph_and_rule_without_twip_rounding(){
    for (left,right,l,r) in [("1mm","1cm",360./127.,3600./127.),("0.1cm","0.5in",360./127.,36.),("0.5in","18.125pt",36.,18.125),("18.125pt","2pc",18.125,24.),("2pc","1.5pi",24.,18.),("1.5pi","1mm",18.,360./127.)] {
        for jc in ["left","start","center","right","end"] {
            let source=unit_xml(jc,left,right,false,"");let authored=math_document::parse_xml(&source).unwrap();let before=authored.xml();
            let root=omml::parse(&source).unwrap();omml::validate_display_context(&root,true,Some(420.)).unwrap();
            let a=laid(&xml(jc,0,0,false,""),Some(420.));let b=laid(&source,Some(420.));
            moved(&a,&b,match jc{"left"|"start"=>l,"right"|"end"=>-r,_=>(l-r)/2.});near(b.width,420.);
            let style=omml::math_paragraph_style(&root,root.child("oMathPara"));assert!(style.word_margins);near(style.left,l);near(style.right,r);
            let mathml=omml::to_mathml_constrained(&root,true,"",12.,Some(420.)).unwrap().output;
            assert!(mathml.contains(&format!("margin-left:{l}pt;margin-right:{r}pt")),"{mathml}");
            let native=omml::docx_math_projection_tree(authored.clone(),true).unwrap().paragraph.unwrap();
            assert_eq!((native.left.as_deref(),native.right.as_deref()),(Some(left),Some(right)));assert_eq!(native.alignment.as_deref(),Some(jc));
            assert_eq!(authored.xml(),before);
        }
    }
    let exact=laid(&unit_xml("left","1mm","0",false,""),Some(420.));
    let rounded=laid(&xml("left",57,0,false,""),Some(420.));
    let first=|value:&Formula|value.items.iter().find_map(|item|if let Item::Glyph{x,..}=item{Some(x*12.)}else{None}).unwrap();
    near(first(&rounded)-first(&exact),57./20.-360./127.);assert!((first(&rounded)-first(&exact)).abs()>0.01);
}
#[test]fn word_paragraph_unit_margins_keep_math_priorities_and_cannot_bypass_width_or_host_guards(){
    for enabled in [false,true] {for local in ["",r#"<m:oMathParaPr><m:jc m:val="center"/></m:oMathParaPr>"#] {
        let source=unit_xml("right","1mm","18.125pt",enabled,local)
            .replace("</m:mathPr>",r#"<m:lMargin m:val="120"/><m:rMargin m:val="240"/><m:defJc m:val="left"/></m:mathPr>"#);
        let root=omml::parse(&source).unwrap();omml::validate_display_context(&root,true,Some(420.)).unwrap();
        let style=omml::math_paragraph_style(&root,root.child("oMathPara"));
        assert_eq!(style.alignment,if !local.is_empty(){omml::MathJustification::Center}else if enabled{omml::MathJustification::Left}else{omml::MathJustification::Right});
        assert_eq!(style.word_margins,!enabled);near(style.left,if enabled{6.}else{360./127.});near(style.right,if enabled{12.}else{18.125});
        let native=omml::native_word_paragraph(&root).unwrap().unwrap();assert_eq!((native.left.as_deref(),native.right.as_deref()),(Some("1mm"),Some("18.125pt")));
        for property in ["<w:bidi/>","<w:mirrorIndents/>",r#"<w:textDirection w:val="tbRl"/>"#] {
            let blocked=if property.starts_with("<w:bidi"){source.replace("<w:ind",&format!("{property}<w:ind"))}else if property.starts_with("<w:mirror"){source.replace("<w:jc",&format!("{property}<w:jc"))}else{source.replace("</w:pPr>",&format!("{property}</w:pPr>"))};
            assert_eq!(omml::validate_display_context(&omml::parse(&blocked).unwrap(),true,Some(420.)),Err("math_unsupported_extension"));
        }
        if !enabled {
            let width=360./127.+18.125-0.01;
            assert_eq!(omml::validate_display_context(&root,true,Some(width)),Err("math_unsupported_extension"));
            assert!(layout_source_constrained("",Some(&source),true,12.,Some(width)).is_none());
        }
    }}
    let sparse=unit_xml("left","-0.000mm","0",false,"").replace(r#" w:right="0""#,"");
    let root=omml::parse(&sparse).unwrap();let native=omml::native_word_paragraph(&root).unwrap().unwrap();
    assert_eq!(native.left.as_deref(),Some("-0.000mm"));assert!(native.right.is_none());
    moved(&laid(&xml("left",0,0,false,""),Some(420.)),&laid(&sparse,Some(420.)),0.);
}
#[test]fn word_paragraph_logical_ltr_matches_physical_glyphs_svg_mathml_and_keeps_native_values(){
    for (logical,physical,expected) in [("start","left",omml::MathJustification::Left),("end","right",omml::MathJustification::Right)] {
        for (left,right) in [(0,0),(720,360),(480,240)] {for direction in ["","lrTb"] {
            let with_direction=|source:String|if direction.is_empty(){source}else{source.replace("</w:pPr>",&format!(r#"<w:textDirection w:val="{direction}"/></w:pPr>"#))};
            let source=with_direction(xml(logical,left,right,false,""));
            let control=with_direction(xml(physical,left,right,false,""));
            let authored=math_document::parse_xml(&source).unwrap();let before=authored.xml();
            let root=omml::parse(&source).unwrap();let raw_context=root.word_paragraph.as_ref().unwrap().as_ref().clone();
            omml::validate_display_context(&root,true,Some(420.)).unwrap();
            let projected=omml::word_display_paragraph(&root,root.child("oMathPara")).unwrap().unwrap();
            assert_eq!(projected.alignment.as_ref().unwrap().value.as_str(),physical);
            assert_eq!(projected.alignment.unwrap().origin,crate::math_word::WordParagraphOrigin::Direct);
            let a=laid(&control,Some(420.));let b=laid(&source,Some(420.));
            moved(&a,&b,0.);assert_eq!(a.items,b.items);assert_eq!(b.alignment,Some(expected));
            assert_eq!(a.svg("#123456"),b.svg("#123456"));
            let logical_mathml=omml::to_mathml_constrained(&root,true,"",12.,Some(420.)).unwrap();
            let physical_mathml=omml::to_mathml_constrained(&omml::parse(&control).unwrap(),true,"",12.,Some(420.)).unwrap();
            assert_eq!(logical_mathml.output,physical_mathml.output);assert_eq!(logical_mathml.profile,physical_mathml.profile);
            assert!(logical_mathml.output.contains(&format!("text-align:{physical}")));
            assert!(!logical_mathml.output.contains("text-align:start")&&!logical_mathml.output.contains("text-align:end"));
            let native=omml::docx_math_projection_tree(authored.clone(),true).unwrap().paragraph.unwrap();
            assert_eq!(native.alignment.as_deref(),Some(logical));assert_eq!((native.left,native.right),(Some(left.to_string()),Some(right.to_string())));
            assert_eq!(root.word_paragraph.as_ref().unwrap().as_ref(),&raw_context);
            assert_eq!(raw_context.effective.alignment.as_ref().unwrap().value.as_str(),logical);assert_eq!(authored.xml(),before);
        }}
    }
}
#[test]fn word_paragraph_logical_jc_math_priority_and_tb_qualification_are_not_global_rewrites(){
    for (logical,default) in [("start","right"),("end","left")] {for enabled in [false,true] {for local in [false,true] {for direction in ["lrTb","tb"] {
        let local_xml=if local{r#"<m:oMathParaPr><m:jc m:val="center"/></m:oMathParaPr>"#}else{""};
        let source=xml(logical,720,360,enabled,local_xml)
            .replace("</w:pPr>",&format!(r#"<w:textDirection w:val="{direction}"/></w:pPr>"#))
            .replace("</m:mathPr>",&format!(r#"<m:defJc m:val="{default}"/></m:mathPr>"#));
        let root=omml::parse(&source).unwrap();
        if !enabled&&!local&&direction=="tb" {
            assert_eq!(omml::validate_display_context(&root,true,Some(420.)),Err("math_unsupported_extension"));
            assert!(layout_source_constrained("",Some(&source),true,12.,Some(420.)).is_none());continue;
        }
        omml::validate_display_context(&root,true,Some(420.)).unwrap();
        let style=omml::math_paragraph_style(&root,root.child("oMathPara"));
        assert_eq!(style.alignment,if local{omml::MathJustification::Center}else if enabled{omml::MathJustification::parse(default)}else if logical=="start"{omml::MathJustification::Left}else{omml::MathJustification::Right});
        if !enabled{assert_eq!((style.left,style.right),(36.,18.));assert!(style.word_margins);}
        let native=omml::native_word_paragraph(&root).unwrap().unwrap();assert_eq!(native.alignment.as_deref(),Some(logical));
        assert_eq!(root.word_paragraph.as_ref().unwrap().effective.alignment.as_ref().unwrap().value.as_str(),logical);
    }}}}
}
#[test]fn word_paragraph_math_override_disables_word_axes_but_not_four_spacings(){
    let a=laid(&xml("right",0,0,true,""),Some(420.));let b=laid(&xml("left",720,360,true,""),Some(420.));moved(&a,&b,0.);
    let source=xml("left",0,0,false,"");let spaced=source.replace("</m:mathPr>","<m:preSp m:val=\"120\"/><m:postSp m:val=\"240\"/><m:interSp m:val=\"360\"/><m:intraSp m:val=\"480\"/></m:mathPr>");
    let root=omml::parse(&spaced).unwrap();let style=omml::math_paragraph_style(&root,root.child("oMathPara"));
    assert_eq!((style.before,style.after,style.inter,style.intra),(6.,12.,18.,24.));
    let plain=laid(&source,Some(420.));let with=laid(&spaced,Some(420.));near(with.height-plain.height,6.);near(with.depth-plain.depth,12.);
}
#[test]fn word_paragraph_jc_without_ind_still_uses_the_actual_frame_without_inventing_source_axes(){
    for (jc,local) in [("left",""),("center",""),("right",""),("start",""),("end",""),("left","<m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr>"),("start","<m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr>")]{
        let explicit=xml(jc,0,0,false,local);
        let missing=explicit.replace("<w:ind w:left=\"0\" w:right=\"0\"/>","");
        let a=laid(&missing,Some(420.));let b=laid(&explicit,Some(420.));
        moved(&a,&b,0.);near(a.width,420.);
        let source=math_document::parse_xml(&missing).unwrap();
        let native=omml::docx_math_projection_tree(source.clone(),true).unwrap().paragraph.unwrap();
        assert_eq!(native.alignment.as_deref(),Some(jc));assert_eq!((native.left,native.right),(None,None));
        assert_eq!(source.xml(),math_document::parse_xml(&missing).unwrap().xml());
    }
}
#[test]fn word_paragraph_local_math_jc_wins_without_erasing_native_word_jc(){
    for jc in ["left","start","end"] {
    let source=xml(jc,720,360,false,"<m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr>");
    let root=omml::parse(&source).unwrap();let style=omml::math_paragraph_style(&root,root.child("oMathPara"));
    assert_eq!(style.alignment,omml::MathJustification::Right);assert!(style.word_margins);assert_eq!((style.left,style.right),(36.,18.));
    let projected=omml::docx_math_projection_tree(math_document::parse_xml(&source).unwrap(),true).unwrap();
    let paragraph=projected.paragraph.unwrap();assert_eq!(paragraph.alignment.as_deref(),Some(jc));assert!(projected.content.contains("m:val=\"right\""));
    }
}
#[test]fn word_paragraph_native_inheritance_does_not_create_sticky_math_override(){
    let source=xml("left",0,0,false,"");let projected=omml::docx_math_projection_tree(math_document::parse_xml(&source).unwrap(),true).unwrap();
    assert!(!projected.content.contains("<m:jc"));assert_eq!(projected.paragraph.unwrap().alignment.as_deref(),Some("left"));
    let empty="<w:pPr/><m:mathPr><m:dispDef m:val=\"0\"/></m:mathPr><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>";
    let projected=omml::docx_math_projection_tree(math_document::parse_xml(empty).unwrap(),true).unwrap();assert!(!projected.content.contains("<m:jc"));assert_eq!(projected.paragraph.unwrap().alignment,None);
}
#[test]fn word_paragraph_context_survives_formatted_clone_without_touching_source(){
    let source=xml("left",0,0,false,"").replace("<m:r><m:t>x", "<m:r><w:rPr><w:b/></w:rPr><m:t>x");
    let before=math_document::parse_xml(&source).unwrap();let root=omml::parse(&source).unwrap();
    let cloned=root.clone();assert!(Arc::ptr_eq(root.word_paragraph.as_ref().unwrap(),cloned.word_paragraph.as_ref().unwrap()));
    let mathml=omml::to_mathml_checked_with_context(&root,true,"",12.,None).unwrap();assert!(mathml.contains("text-align:left"));assert!(mathml.contains("mathvariant=\"bold"));
    assert_eq!(before.xml(),math_document::parse_xml(&source).unwrap().xml());
    assert!(omml::parse("<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>").unwrap().word_paragraph.is_none());
}
#[test]fn word_paragraph_zero_is_a_real_axis_and_does_not_use_math_margin_fallback(){
    let root=omml::parse(&xml("left",0,0,false,"")).unwrap();let style=omml::math_paragraph_style(&root,root.child("oMathPara"));
    assert!(style.explicit_margins&&style.word_margins);assert_eq!(style.margins(Some(1.)),(0.,0.));
    let root=omml::parse(&xml("left",31680,0,false,"")).unwrap();assert_eq!(omml::validate_display_context(&root,true,Some(420.)).unwrap_err(),"math_unsupported_extension");
    assert!(layout_source_constrained("",Some(&xml("left",31680,0,false,"")),true,12.,Some(420.)).is_none());
}
#[test]fn word_paragraph_mixed_implicit_and_explicit_display_scopes_are_both_checked(){
    let source=xml("both",0,0,false,"<m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr>")
        .replace("<m:oMathPara>","<m:oMath><m:r><m:t>a</m:t></m:r></m:oMath><m:oMathPara>");
    let root=omml::parse(&source).unwrap();assert_eq!(omml::display_scopes(&root).len(),2);
    assert_eq!(omml::validate_display_context(&root,true,None),Err("math_unsupported_extension"));
    let valid=source.replace("w:val=\"both\"","w:val=\"left\"");assert!(omml::validate_display_context(&omml::parse(&valid).unwrap(),true,None).is_ok());
    let a=laid(&valid,Some(420.));let b=laid(&valid.replace("w:left=\"0\"","w:left=\"720\""),Some(420.));
    let first=|f:&Formula|f.items.iter().find_map(|item|if let Item::Glyph{x,..}=item{Some(*x*12.)}else{None}).unwrap();near(first(&b)-first(&a),36.);
}
#[test]fn word_paragraph_direction_guards_apply_even_when_math_defaults_are_enabled(){
    for jc in ["left","start","end"] {for enabled in [true,false]{for property in ["<w:bidi/>","<w:mirrorIndents/>","<w:textDirection w:val=\"tbRl\"/>","<w:framePr/>"]{
        let source=if property.starts_with("<w:bidi")||property.starts_with("<w:framePr"){
            xml(jc,0,0,enabled,"").replace("<w:ind",&format!("{property}<w:ind"))
        }else if property.starts_with("<w:mirror"){
            xml(jc,0,0,enabled,"").replace("<w:jc",&format!("{property}<w:jc"))
        }else{xml(jc,0,0,enabled,"").replace("</w:pPr>",&format!("{property}</w:pPr>"))};
        let document=math_document::parse(&source,"omml",true).unwrap();assert_eq!(document.source,source);
        let root=omml::parse(&source).unwrap();assert_eq!(omml::validate_display_context(&root,true,None),Err("math_unsupported_extension"));
        assert!(math_document::preview(&document,true).is_err());
    }}}
}
#[test]fn word_paragraph_native_output_has_one_ordered_outer_patch_and_real_math(){
    for jc in ["right","start","end"] {
    for context in ["", "<w:styles/>", r#"<w:styles><w:style w:type="paragraph" w:styleId="Equation" w:default="1"><w:rPr><w:b/></w:rPr></w:style></w:styles>"#] {
    let source=format!("{context}{}",xml(jc,720,180,false,""));
    let parsed=math_document::parse_xml(&source).unwrap();let before=parsed.xml();
    let projection=omml::docx_math_projection_tree(parsed.clone(),true).unwrap();assert_eq!(parsed.xml(),before);
    let fragment=math_document::parse_xml(&projection.content).unwrap();assert!(!fragment.children().any(|n|n.namespace==WORD_NS&&matches!(n.local(),"pPr"|"styles")));
    let doc=crate::md_ast::Document{blocks:vec![crate::md_ast::Block::OfficeMath{latex:"x/y".into(),omml:source}],..Default::default()};
    let out=crate::docx_writer::build(&doc,&crate::export_styles::default_style(),"Math",&|_|panic!("unexpected image access"));assert!(!out.parts.is_empty(),"{:?}",out.warns);
    let part=String::from_utf8(out.parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1.clone()).unwrap();
    let root=math_document::parse_xml(&part).unwrap();let document=root.children().next().unwrap();let body=word(document,"body").unwrap();
    let p=body.children().find(|node|node.namespace==WORD_NS&&node.local()=="p"&&node.children().any(|child|child.namespace==MATH_NS&&child.local()=="oMathPara")).unwrap();
    assert_eq!(p.children().filter(|node|node.namespace==WORD_NS&&node.local()=="pPr").count(),1,"{part}");
    assert_eq!(p.children().map(XmlNode::local).collect::<Vec<_>>(),["pPr","oMathPara"]);
    let ppr=word(p,"pPr").unwrap();let names=ppr.children().map(XmlNode::local).collect::<Vec<_>>();assert_eq!(names,vec!["pStyle","ind","jc"]);
    assert_eq!(attr(word(ppr,"ind").unwrap(),"left"),Some("720"));assert_eq!(attr(word(ppr,"ind").unwrap(),"right"),Some("180"));assert_eq!(attr(word(ppr,"jc").unwrap(),"val"),Some(jc));
    let math=p.children().find(|node|node.namespace==MATH_NS&&node.local()=="oMathPara").unwrap();assert!(!math.xml().contains("<m:jc"));assert!(!part.contains("word_paragraph"));assert!(!part.contains("<w:drawing"));
    }
    }
}

#[test]fn word_paragraph_native_docx_keeps_effective_unit_lexical_values_from_each_origin(){
    for enabled in [false,true] {
        let math=xml("right",0,0,enabled,"").split_once("</w:pPr>").unwrap().1.to_string();
        let cases=[
            (unit_xml("right","0001.000mm","1.00cm",enabled,""),"0001.000mm","1.00cm",false),
            (format!(r#"<w:styles><w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:ind w:left="0.5in"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="Base"/><w:pPr><w:ind w:right="18.125pt"/></w:pPr></w:style></w:styles><w:pPr><w:pStyle w:val="Equation"/><w:jc w:val="right"/></w:pPr>{math}"#),"0.5in","18.125pt",true),
            (format!(r#"<w:styles><w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="2pc" w:right="1.5pi"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles><w:pPr><w:jc w:val="right"/></w:pPr>{math}"#),"2pc","1.5pi",false),
            (unit_xml("right"," &#9;+00057&#13;&#10; ","-0.000mm",enabled,"")," \t+00057\r\n ","-0.000mm",false),
        ];
        for (source,left,right,native_chain) in cases {
            let parsed=math_document::parse_xml(&source).unwrap();let before=parsed.xml();
            let projection=omml::docx_math_projection_tree(parsed.clone(),true).unwrap().paragraph.unwrap();
            assert_eq!((projection.left.as_deref(),projection.right.as_deref()),(Some(left),Some(right)));
            let doc=crate::md_ast::Document{blocks:vec![crate::md_ast::Block::OfficeMath{latex:"x/y".into(),omml:source}],..Default::default()};
            let out=crate::docx_writer::build(&doc,&crate::export_styles::default_style(),"Math",&|_|panic!("unit native export accessed image"));
            assert!(!out.parts.is_empty(),"{:?}",out.warns);
            let part=String::from_utf8(out.parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1.clone()).unwrap();
            let root=math_document::parse_xml(&part).unwrap();let body=word(root.children().next().unwrap(),"body").unwrap();
            let p=body.children().find(|node|node.namespace==WORD_NS&&node.local()=="p"&&node.children().any(|child|child.namespace==MATH_NS&&child.local()=="oMathPara")).unwrap();
            assert_eq!(p.children().map(XmlNode::local).collect::<Vec<_>>(),["pPr","oMathPara"]);
            let ppr=word(p,"pPr").unwrap();
            if native_chain {
                // Keep the old raw/effective contract, but require the newly
                // preserved sparse native chain instead of flattening it.
                assert_eq!(ppr.children().map(XmlNode::local).collect::<Vec<_>>(),["pStyle","jc"]);
                assert_eq!(attr(word(ppr,"pStyle").unwrap(),"val"),Some("Equation"));
                let styles_part=std::str::from_utf8(&out.parts.iter().find(|(name,_)|name=="word/styles.xml").unwrap().1).unwrap();
                let styles_tree=math_document::parse_xml(styles_part).unwrap();let styles=styles_tree.children().next().unwrap();
                let definition=|id:&str|styles.children().find(|node|node.namespace==WORD_NS&&node.local()=="style"&&attr(node,"styleId")==Some(id)).unwrap();
                let leaf=definition("Equation");assert_eq!(attr(word(leaf,"basedOn").unwrap(),"val"),Some("Base"));
                let base=definition("Base");assert!(word(base,"basedOn").is_none());
                let base_ind=word(word(base,"pPr").unwrap(),"ind").unwrap();let leaf_ind=word(word(leaf,"pPr").unwrap(),"ind").unwrap();
                assert_eq!(attr(base_ind,"left"),Some(left));assert!(attr(base_ind,"right").is_none());
                assert_eq!(attr(leaf_ind,"right"),Some(right));assert!(attr(leaf_ind,"left").is_none());
                assert!(!out.warns.iter().any(|code|code=="math_paragraph_style_materialized"));
            }else{
                assert_eq!(ppr.children().map(XmlNode::local).collect::<Vec<_>>(),["pStyle","ind","jc"]);
                assert_eq!(attr(word(ppr,"ind").unwrap(),"left"),Some(left));assert_eq!(attr(word(ppr,"ind").unwrap(),"right"),Some(right));
            }
            assert_eq!(attr(word(ppr,"jc").unwrap(),"val"),Some("right"));
            assert!(!part.contains("<w:drawing")&&!part.contains("word_paragraph"));assert_eq!(parsed.xml(),before);
            if left.contains('\t'){assert!(part.contains("&#9;")&&part.contains("&#13;")&&part.contains("&#10;"));}
        }
    }
}

#[test]fn word_paragraph_missing_multi_equation_alignment_is_not_faked_by_native_materialization(){
    let source="<w:pPr/><m:mathPr><m:dispDef m:val=\"0\"/></m:mathPr><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath><m:oMath><m:r><m:t>longer</m:t></m:r></m:oMath></m:oMathPara>";
    let document=math_document::parse(source,"omml",true).unwrap();assert_eq!(document.source,source);
    assert_eq!(omml::validate_display_context(&omml::parse(source).unwrap(),true,None),Err("math_unsupported_extension"));
    let explicit=source.replace("<m:oMathPara>","<m:oMathPara><m:oMathParaPr><m:jc m:val=\"centerGroup\"/></m:oMathParaPr>");
    assert!(omml::validate_display_context(&omml::parse(&explicit).unwrap(),true,None).is_ok());
}

#[test]fn word_paragraph_pdf_and_docx_fail_before_image_callbacks_or_replacing_target(){
    for jc in ["left","start","end"] {
    let source=xml(jc,31680,31680,false,"");let raw=serde_json::json!([{"type":"image","src":"not-readable"},{"type":"math","omml":source,"latex":"x/y","fallback":true}]);
    let style=crate::export_styles::default_style();let count=std::cell::Cell::new(0usize);let callback=|_:&str|{count.set(count.get()+1);None};
    assert!(crate::pdf_render::build(&raw,&style,"",&callback,&mut Vec::new()).is_err());assert_eq!(count.get(),0);
    let doc=crate::md_ast::Document{blocks:vec![crate::md_ast::Block::OfficeMath{latex:"x/y".into(),omml:xml(jc,31680,31680,false,"")}],..Default::default()};
    let out=crate::docx_writer::build(&doc,&style,"Math",&|_|panic!("unexpected image access"));assert!(out.parts.is_empty());assert_eq!(out.warns,["math_unsupported_extension"]);
    let dir=tempfile::tempdir().unwrap();let target=dir.path().join("old.docx");std::fs::write(&target,b"old owned output").unwrap();
    let dsl=crate::math_dsl::decompile(&math_document::parse_xml(&source).unwrap()).unwrap();let md=format!("![bad](missing.png)\n\n```readmd-math v1\n{dsl}\n```\n");
    let error=crate::mdexport::export_docx(&md,dir.path().to_str().unwrap(),target.to_str().unwrap(),&serde_json::json!({}),"Math").unwrap_err();assert_eq!(error,"math_unsupported_extension");assert_eq!(std::fs::read(target).unwrap(),b"old owned output");
    }
}
