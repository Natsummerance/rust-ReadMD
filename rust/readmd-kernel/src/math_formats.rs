//! Presentation adapters for the shared lossless Office Math tree. An adapter
//! reports unsupported constructs instead of silently claiming equivalence.
use crate::math_document::{XmlNode,XmlPart,MATH_NS,WORD_NS};

fn is_office(n:&XmlNode)->bool{n.namespace.is_empty()||n.namespace==MATH_NS}
fn child<'a>(n:&'a XmlNode,name:&str)->Option<&'a XmlNode>{n.children().find(|c|c.local()==name&&is_office(c))}
fn prop<'a>(n:&'a XmlNode,container:&str,key:&str)->Option<&'a str>{let property=child(child(n,container)?,key)?;property.attributes.iter().find(|(k,_)|k.rsplit(':').next()==Some("val")&&property.attribute_namespaces.get(*k).is_none_or(|ns|ns.is_empty()||ns==MATH_NS)).map(|(_,v)|v.as_str())}
fn enabled(n:&XmlNode,container:&str,key:&str)->bool{child(n,container).and_then(|p|child(p,key)).is_some()&&!matches!(prop(n,container,key),Some("0"|"false"|"off"))}
fn character<'a>(n:&'a XmlNode,container:&str,key:&str,default:&'a str)->&'a str {
    if child(n,container).and_then(|p|child(p,key)).is_some(){prop(n,container,key).unwrap_or("")}else{default}
}
fn escaped(text:&str)->String{text.chars().map(|c|match c{'{'|'}'|'%'|'#'|'&'|'_'|'$'=>format!("\\{c}"),'\\'=>"\\textbackslash ".into(),c=>c.to_string()}).collect()}
fn escaped_atom(text:&str)->String{text.chars().map(|c|match c{'\\'=>"\\textbackslash ".into(),'^'=>"\\textasciicircum ".into(),'~'=>"\\textasciitilde ".into(),_=>escaped(&c.to_string())}).collect()}
fn styled_tex(n:&XmlNode,mut result:String)->String{
    if let Some(command)=match prop(n,"rPr","sty"){Some("p")=>Some("\\mathrm"),Some("i")=>Some("\\mathit"),Some("b")=>Some("\\mathbf"),Some("bi")=>Some("\\boldsymbol"),_=>None}{result=format!("{command}{{{result}}}");}
    if let Some(command)=match prop(n,"rPr","scr"){Some("double-struck")=>Some("\\mathbb"),Some("script")=>Some("\\mathcal"),Some("fraktur")=>Some("\\mathfrak"),Some("sans-serif")=>Some("\\mathsf"),Some("monospace")=>Some("\\mathtt"),_=>None}{result=format!("{command}{{{result}}}");}
    result
}

/// A plain-TeX projection is for editing and fallback only. Complete properties
/// remain in the adjacent metadata and are used directly by DOCX/PDF/EPUB.
pub fn tex(n:&XmlNode)->String {
    fn has_emulator(n:&XmlNode)->bool{(is_office(n)&&n.local()=="box"&&enabled(n,"boxPr","opEmu")&&!enabled(n,"boxPr","diff"))||n.children().any(has_emulator)}
    let emulator=has_emulator(n);tex_context(n,emulator,emulator)
}

/// Checked editing projection for the bounded native operator-emulator subset.
/// Other adapters still preserve the full original XML/DSL; this entry never
/// certifies break/style properties which the two standard atom macros lose.
pub fn tex_checked(n:&XmlNode)->Result<String,&'static str>{
    fn check(n:&XmlNode)->Result<(),&'static str>{
        if is_office(n)&&n.local()=="box"&&enabled(n,"boxPr","opEmu"){
            validate_emulator(n)?;
        }
        for c in n.children(){check(c)?;}
        Ok(())
    }
    check(n)?;Ok(tex(n))
}

fn namespace_attributes_only(n:&XmlNode)->bool{
    n.attributes.keys().all(|key|key=="xmlns"||key.starts_with("xmlns:"))
}
fn value_property_only(n:&XmlNode)->bool{
    n.children().next().is_none()&&n.text().trim().is_empty()&&n.attributes.keys().all(|key|
        key=="xmlns"||key.starts_with("xmlns:")||(key.rsplit(':').next()==Some("val")
            &&n.attribute_namespaces.get(key).is_none_or(|ns|ns.is_empty()||ns==MATH_NS)))
}
fn validate_emulator(n:&XmlNode)->Result<(),&'static str>{
    const UNSUPPORTED:&str="math_unsupported_extension";
    if !namespace_attributes_only(n)||!enabled(n,"boxPr","noBreak")||enabled(n,"boxPr","diff"){
        return Err(UNSUPPORTED);
    }
    let Some(pr)=child(n,"boxPr")else{return Err(UNSUPPORTED)};
    if !namespace_attributes_only(pr)||pr.children().any(|p|!is_office(p)||!value_property_only(p)||!matches!(p.local(),"opEmu"|"noBreak")){
        return Err(UNSUPPORTED);
    }
    if n.children().any(|c|!is_office(c)||!matches!(c.local(),"boxPr"|"e")){
        return Err(UNSUPPORTED);
    }
    let Some(body)=child(n,"e")else{return Err(UNSUPPORTED)};
    validate_emulator_body(body)
}
fn validate_emulator_body(n:&XmlNode)->Result<(),&'static str>{
    const UNSUPPORTED:&str="math_unsupported_extension";
    // Our editable projection uses the existing bounded decimal-mu model for
    // original run spaces. Accept that same model on subsequent generations;
    // this does not relax native Office/TeX export's mixed-tree prohibition.
    if crate::math_spacing::is_spacing(n){
        let space=crate::math_spacing::Spacing::from_xml(n)?;
        if space.height.is_none()&&space.depth.is_none()&&space.math_size.is_none()
            &&space.width.as_ref().is_some_and(|w|matches!(w.unit,crate::math_spacing::SpaceUnit::Em)){
            return Ok(());
        }
        return Err(UNSUPPORTED);
    }
    if !is_office(n)||!namespace_attributes_only(n){return Err(UNSUPPORTED);}
    if n.content.iter().any(|p|matches!(p,XmlPart::Text(t) if !t.trim().is_empty())){return Err(UNSUPPORTED);}
    match n.local(){
        "e"=>{for c in n.children(){validate_emulator_body(c)?;}},
        "r"=>{
            if n.children().any(|c|!is_office(c)||!matches!(c.local(),"rPr"|"t")){return Err(UNSUPPORTED);}
            if enabled(n,"rPr","nor"){
                // Literal '-' projects as \text{-}; that exact text atom is
                // representable without admitting general normal/Word text.
                let hyphen=n.children().filter(|c|c.local()=="t"&&is_office(c)).map(XmlNode::text).collect::<String>()=="-";
                if !hyphen||child(n,"rPr").is_some_and(|pr|pr.children().any(|p|p.local()!="nor")){
                    return Err(UNSUPPORTED);
                }
            }
            if let Some(pr)=child(n,"rPr"){
                if !namespace_attributes_only(pr){return Err(UNSUPPORTED);}
                for p in pr.children(){
                    if !is_office(p)||!value_property_only(p)||!matches!(p.local(),"lit"|"nor"|"sty"){return Err(UNSUPPORTED);}
                    if p.local()=="sty"&&!matches!(prop(n,"rPr","sty"),None|Some("p"|"i"|"b"|"bi")){return Err(UNSUPPORTED);}
                }
            }
            for t in n.children().filter(|c|c.local()=="t"){
                if !namespace_attributes_only(t)||t.children().next().is_some()
                    ||t.text().chars().any(|c|c.is_whitespace()&&!matches!(c,' '|'\u{2009}'|'\u{2005}'|'\u{2003}')){
                    return Err(UNSUPPORTED);
                }
            }
        },
        "box"=>{
            if enabled(n,"boxPr","opEmu"){validate_emulator(n)?;}
            else{
                let Some(pr)=child(n,"boxPr")else{return Err(UNSUPPORTED)};
                if !enabled(n,"boxPr","noBreak")||!namespace_attributes_only(pr)
                    ||pr.children().any(|c|!is_office(c)||!value_property_only(c)||c.local()!="noBreak")
                    ||n.children().any(|c|!is_office(c)||!matches!(c.local(),"boxPr"|"e")){
                    return Err(UNSUPPORTED);
                }
                let Some(body)=child(n,"e")else{return Err(UNSUPPORTED)};
                validate_emulator_body(body)?;
            }
        },
        _=>return Err(UNSUPPORTED),
    }
    Ok(())
}
fn emulator_command(n:&XmlNode)->&'static str{
    let relation=crate::omml::parse(&n.xml()).ok().and_then(|root|root.child("box").map(|n|
        crate::omml::emulated_operator_class(n)==crate::omml::Class::Rel)).unwrap_or(false);
    if relation{"\\mathrel"}else{"\\mathbin"}
}
fn emulator_run_tex(n:&XmlNode)->String{
    if enabled(n,"rPr","lit")||enabled(n,"rPr","nor"){return tex_context(n,true,false);}
    let text=n.children().filter(|c|c.local()=="t"&&is_office(c)).map(XmlNode::text).collect::<String>();
    let split=text.chars().nth(1).is_some();
    let body=text.chars().map(|c|{
        // These are the existing nonliteral run-space policies, not font
        // constants. Splitting a run must not introduce TeX atom spacing.
        let space=match c{' '=>Some(3.),'\u{2009}'|'\u{2005}'=>Some(3.06),'\u{2003}'=>Some(18.),_=>None};
        if let Some(mu)=space{return format!("\\mkern{mu}mu");}
        let token=escaped_atom(&(if c=='-'&&split{'−'}else{c}).to_string());
        if split{format!("\\mathord{{{token}}}")}else{token}
    }).collect::<String>();
    let explicit=child(n,"rPr").and_then(|pr|child(pr,"sty")).is_some();
    let mut body=styled_tex(n,body);
    if explicit&&prop(n,"rPr","sty").is_none(){body=format!("\\mathit{{{body}}}");}
    else if !explicit&&crate::omml::is_word(&text){body=format!("\\mathrm{{{body}}}");}
    body
}
fn emulator_body_tex(n:&XmlNode)->String{
    match n.local(){
        "e"=>n.children().map(emulator_body_tex).collect(),
        "r"=>emulator_run_tex(n),
        "box" if enabled(n,"boxPr","opEmu")&&!enabled(n,"boxPr","diff")=>format!("{}{{{}}}",emulator_command(n),child(n,"e").map(emulator_body_tex).unwrap_or_default()),
        "box" if enabled(n,"boxPr","noBreak")&&!enabled(n,"boxPr","diff")=>format!("\\mathord{{{}}}",child(n,"e").map(emulator_body_tex).unwrap_or_default()),
        _=>tex_context(n,true,true),
    }
}
fn tex_context(n:&XmlNode,preserve_style:bool,preserve_runs:bool)->String {
    let arg=|name:&str|child(n,name).map(|c|tex_context(c,preserve_style,preserve_runs)).unwrap_or_default();
    if crate::math_spacing::is_spacing(n){return crate::math_spacing::Spacing::from_xml(n).map(|space|{
        if space.height.is_none()&&space.depth.is_none()&&space.math_size.is_none(){
            if let Some(width)=&space.width{if matches!(width.unit,crate::math_spacing::SpaceUnit::Em){return format!("\\mkern{:.8}mu",width.value*18.);}}
        }
        space.tex_annotation()
    }).unwrap_or_else(|_|"\\mspaceUnsupported".into());}
    let children=||n.children().filter(|c|!c.local().ends_with("Pr")).map(|c|tex_context(c,preserve_style,preserve_runs)).collect::<String>();
    if !is_office(n){return children();}
    match n.local(){
        "t"=>escaped(&n.text()),"r"=>{
            let literal=enabled(n,"rPr","lit");
            let normal=enabled(n,"rPr","nor");
            if literal&&!normal{
                let result=n.children().filter(|c|c.local()=="t"&&is_office(c)).flat_map(|c|c.text().chars().collect::<Vec<_>>()).map(|c|{
                    if let Some(space)=crate::math_layout::literal_space_advance(c){format!("\\mkern{:.8}mu",space*18.)}
                    else{let symbol=match c{c if c.is_whitespace()=>format!("\\text{{{c}}}"),'-'=>"\\text{-}".into(),'\\'=>"\\textbackslash".into(),'^'=>"\\textasciicircum".into(),'~'=>"\\textasciitilde".into(),_=>escaped(&c.to_string())};format!("\\mathord{{{symbol}}}")}
                }).collect::<String>();
                styled_tex(n,result)
            }else if preserve_runs{emulator_run_tex(n)}else if preserve_style{let text=n.children().filter(|c|c.local()=="t"&&is_office(c)).map(|c|escaped_atom(&c.text())).collect::<String>();if normal{format!("\\text{{{text}}}")}else{styled_tex(n,text)}}else{n.children().filter(|c|c.local()=="t"&&is_office(c)).map(|c|tex_context(c,preserve_style,preserve_runs)).collect()}
        },
        "oMath"=>{
            let mut lines=vec![String::new()];
            for c in n.children().filter(|c|!c.local().ends_with("Pr")){
                let pr=match c.local(){"r"=>"rPr","box"=>"boxPr",_=>""};
                if !pr.is_empty()&&child(c,pr).and_then(|p|child(p,"brk")).is_some()&&!lines.last().unwrap().is_empty(){lines.push(String::new());}
                lines.last_mut().unwrap().push_str(&tex_context(c,preserve_style,preserve_runs));
            }
            if lines.len()>1{format!("\\begin{{aligned}}&{}\\end{{aligned}}",lines.join(" \\\\ &"))}else{lines.pop().unwrap_or_default()}
        },
        "oMathPara"=>{
            let equations:Vec<_>=n.children().filter(|c|c.local()=="oMath"&&is_office(c)).map(|c|tex_context(c,preserve_style,preserve_runs)).collect();
            if equations.len()<2{equations.into_iter().collect()}else{
                let alignment=prop(n,"oMathParaPr","jc").unwrap_or("centerGroup");
                if alignment=="center"{format!("\\begin{{gathered}}{}\\end{{gathered}}",equations.join(" \\\\ "))}
                else{let marker=if alignment=="right"{""}else{"&"};format!("\\begin{{aligned}}{marker}{}\\end{{aligned}}",equations.join(&format!(" \\\\ {marker}")))}
            }
        },
        "f"=>match prop(n,"fPr","type"){Some("noBar")=>format!("\\binom{{{}}}{{{}}}",arg("num"),arg("den")),Some("lin")=>format!("{{{}}}/{{{}}}",arg("num"),arg("den")),_=>format!("\\frac{{{}}}{{{}}}",arg("num"),arg("den"))},
        "rad"=>{let d=arg("deg");if d.is_empty()||matches!(prop(n,"radPr","degHide"),Some("1"|"true"|"on")){format!("\\sqrt{{{}}}",arg("e"))}else{format!("\\sqrt[{d}]{{{}}}",arg("e"))}},
        // The legacy parser paints standalone braces. On the emulator
        // editing path use its existing braced Ord atom to group a base.
        // Keep old plain-TeX projection bytes unchanged.
        "sSup"=>if preserve_runs{format!("\\mathord{{{}}}^{{{}}}",arg("e"),arg("sup"))}else{format!("{{{}}}^{{{}}}",arg("e"),arg("sup"))},
        "sSub"=>if preserve_runs{format!("\\mathord{{{}}}_{{{}}}",arg("e"),arg("sub"))}else{format!("{{{}}}_{{{}}}",arg("e"),arg("sub"))},
        "sSubSup"=>if preserve_runs{format!("\\mathord{{{}}}_{{{}}}^{{{}}}",arg("e"),arg("sub"),arg("sup"))}else{format!("{{{}}}_{{{}}}^{{{}}}",arg("e"),arg("sub"),arg("sup"))},
        "sPre"=>format!("{{}}_{{{}}}^{{{}}}{{{}}}",arg("sub"),arg("sup"),arg("e")),
        "nary"=>{let c=character(n,"naryPr","chr","∫");let mut out=c.to_string();for(role,sign,hide)in[("sub","_","subHide"),("sup","^","supHide")]{if !matches!(prop(n,"naryPr",hide),Some("1"|"true"|"on")){let a=arg(role);if !a.is_empty(){out+=&format!("{sign}{{{a}}}");}}}out+&format!("{{{}}}",arg("e"))},
        "d"=>{
            let delimiter=|s:&str|match s{""=>".".into(),"{"|"}"=>format!("\\{s}"),"│"=>"|".into(),s=>s.into()};
            let start=delimiter(character(n,"dPr","begChr","("));
            let end=delimiter(character(n,"dPr","endChr",")"));
            let sep=character(n,"dPr","sepChr","│");
            let grow=!matches!(prop(n,"dPr","grow"),Some("0"|"false"|"off"));
            let join=if sep.is_empty(){String::new()}else if grow{format!("\\middle{}",delimiter(sep))}else{delimiter(sep)};
            let body=n.children().filter(|c|c.local()=="e"&&is_office(c)).map(|c|tex_context(c,preserve_style,preserve_runs)).collect::<Vec<_>>().join(&join);
            if grow{format!("\\left{start} {body} \\right{end}")}
            else{format!("{}{body}{}",if start=="."{""}else{&start},if end=="."{""}else{&end})}
        },
        "m"=>format!("\\begin{{matrix}}{}\\end{{matrix}}",n.children().filter(|c|c.local()=="mr").map(|r|r.children().filter(|c|c.local()=="e").map(|c|tex_context(c,preserve_style,preserve_runs)).collect::<Vec<_>>().join(" & ")).collect::<Vec<_>>().join(" \\\\ ")),
        "eqArr"=>format!("\\begin{{aligned}}{}\\end{{aligned}}",n.children().filter(|c|c.local()=="e").map(|c|tex_context(c,preserve_style,preserve_runs)).collect::<Vec<_>>().join(" \\\\ ")),
        "limLow"=>format!("\\underset{{{}}}{{{}}}",arg("lim"),arg("e")),"limUpp"=>format!("\\overset{{{}}}{{{}}}",arg("lim"),arg("e")),
        "func"=>format!("\\operatorname{{{}}}{{{}}}",arg("fName"),arg("e")),
        "acc"=>{let sign=character(n,"accPr","chr","̂");if sign.is_empty(){arg("e")}else{format!("\\overset{{{}}}{{{}}}",escaped(sign),arg("e"))}},
        "bar"=>format!("\\{}{{{}}}",if prop(n,"barPr","pos")==Some("bot"){"underline"}else{"overline"},arg("e")),
        "groupChr"=>{
            let sign=character(n,"groupChrPr","chr","⏟");let body=arg("e");
            if sign.is_empty(){return body;}
            let bottom=prop(n,"groupChrPr","pos").unwrap_or("bot")!="top";
            let baseline_bottom=child(n,"groupChrPr").and_then(|p|child(p,"vertJc")).is_some()&&prop(n,"groupChrPr","vertJc").unwrap_or("bot")=="bot";
            if bottom==baseline_bottom {format!("\\{}{{{body}}}{{{}}}",if bottom{"overset"}else{"underset"},escaped(sign))}
            else if (bottom&&sign=="⏟")||(!bottom&&sign=="⏞"){format!("\\{}{{{body}}}",if bottom{"underbrace"}else{"overbrace"})}
            else{format!("\\{}{{{}}}{{{body}}}",if bottom{"underset"}else{"overset"},escaped(sign))}
        },
        "borderBox"=>format!("\\boxed{{{}}}",arg("e")),"box"=>if enabled(n,"boxPr","opEmu")&&!enabled(n,"boxPr","diff"){format!("{}{{{}}}",emulator_command(n),child(n,"e").map(emulator_body_tex).unwrap_or_default())}else if enabled(n,"boxPr","noBreak")&&!enabled(n,"boxPr","diff"){format!("\\mathord{{{}}}",child(n,"e").map(|c|tex_context(c,true,preserve_runs)).unwrap_or_default())}else{arg("e")},
        "phant"=>format!("\\phantom{{{}}}",arg("e")),
        name if name.ends_with("Pr")=>String::new(),_=>children(),
    }
}
fn node(name:&str,content:Vec<XmlNode>)->XmlNode{
    // MathML rows are grouping nodes; Office arguments already provide that
    // grouping. Only generated rows are flattened here, never imported OMML.
    let argument=matches!(name,"oMath"|"e"|"num"|"den"|"sub"|"sup"|"deg"|"lim"|"fName");
    let content=content.into_iter().flat_map(|child|{
        if argument&&child.local()=="e"&&child.attributes.is_empty(){child.content}
        else{vec![XmlPart::Element(child)]}
    }).collect();
    XmlNode{id:String::new(),attribute_namespaces:Default::default(),namespace:MATH_NS.into(),name:format!("m:{name}"),attributes:Default::default(),content}
}
fn run(text:&str)->XmlNode{let mut t=node("t",vec![]);t.content.push(XmlPart::Text(text.into()));node("r",vec![t])}
fn attr(n:&XmlNode,name:&str)->Option<String>{n.attributes.get(name).cloned()}
fn property(container:&str,key:&str,value:&str)->XmlNode{let mut p=node(key,vec![]);p.attributes.insert("m:val".into(),value.into());node(container,vec![p])}

/// Materialize only supported MathML defaults before translation. Attribute
/// maps are scoped per branch; an inner mstyle/token overrides its parent.
/// We do not propagate scriptlevel (relative changes must occur exactly once).
fn mathml_inheritance(n:&mut XmlNode,defaults:&std::collections::BTreeMap<String,String>,color:Option<&str>){
    let mut defaults=defaults.clone();
    if matches!(n.local(),"math"|"mstyle") {
        for key in ["mathvariant","mathsize","linethickness","bevelled","accent","accentunder","stretchy","open","close","separators","columnalign","columnspacing","rowspacing"] {
            if let Some(value)=attr(n,key){defaults.insert(key.into(),value);}
        }
    }
    let color=attr(n,"mathcolor").or_else(||color.map(str::to_string));
    if n.local()!="mspace"{if let Some(color)=&color{n.attributes.insert("mathcolor".into(),color.clone());}}
    let keys:&[&str]=match n.local(){
        "mi"|"mn"|"mo"|"mtext"|"ms"=>&["mathvariant","mathsize"],"mspace"=>&["mathsize"],
        "mfrac"=>&["mathsize","linethickness","bevelled"],
        "munder"=>&["mathsize","accentunder"],"mover"=>&["mathsize","accent"],"munderover"=>&["mathsize","accent","accentunder"],
        "mfenced"=>&["mathsize","open","close","separators"],
        "mtable"=>&["mathsize","columnalign","columnspacing","rowspacing"],
        "msqrt"|"mroot"|"menclose"|"mphantom"=>&["mathsize"],_=>&[],
    };
    for key in keys{if !n.attributes.contains_key(*key){if let Some(value)=defaults.get(*key){n.attributes.insert((*key).into(),value.clone());}}}
    if n.local()=="mo"{for key in ["accent","stretchy"]{if !n.attributes.contains_key(key){if let Some(value)=defaults.get(key){n.attributes.insert(key.into(),value.clone());}}}}
    for part in &mut n.content{if let XmlPart::Element(child)=part{mathml_inheritance(child,&defaults,color.as_deref());}}
}
fn mathml_color(value:&str)->Option<String>{
    let value=value.trim();
    if let Some(hex)=value.strip_prefix('#'){
        if hex.len()==6&&hex.bytes().all(|b|b.is_ascii_hexdigit()){return Some(hex.to_ascii_uppercase());}
        if hex.len()==3&&hex.bytes().all(|b|b.is_ascii_hexdigit()){return Some(hex.chars().flat_map(|c|[c,c]).collect::<String>().to_ascii_uppercase());}
    }
    Some(match value.to_ascii_lowercase().as_str(){"canvastext"=>"auto","black"=>"000000","silver"=>"C0C0C0","gray"=>"808080","white"=>"FFFFFF","maroon"=>"800000","red"=>"FF0000","purple"=>"800080","fuchsia"=>"FF00FF","green"=>"008000","lime"=>"00FF00","olive"=>"808000","yellow"=>"FFFF00","navy"=>"000080","blue"=>"0000FF","teal"=>"008080","aqua"=>"00FFFF",_=>return None}.into())
}
fn mathml_half_points(value:&str)->Option<u32>{
    let value=value.trim();
    let (number,factor)=if let Some(v)=value.strip_suffix("pt"){(v,2.)}else if let Some(v)=value.strip_suffix("px"){(v,1.5)}else if let Some(v)=value.strip_suffix("mm"){(v,144./25.4)}else if let Some(v)=value.strip_suffix("cm"){(v,144./2.54)}else if let Some(v)=value.strip_suffix("in"){(v,144.)}else{return None;};
    let number=number.parse::<f64>().ok()?*factor;
    if !number.is_finite()||!(1.0..=3276.).contains(&number)||(number-number.round()).abs()>1e-7{return None;}
    Some(number.round() as u32)
}
fn word_property(key:&str,value:&str)->XmlNode{
    let mut p=node(key,vec![]);p.name=format!("w:{key}");p.namespace=WORD_NS.into();
    p.attributes.insert("xmlns:w".into(),WORD_NS.into());p.attributes.insert("w:val".into(),value.into());p.attribute_namespaces.insert("w:val".into(),WORD_NS.into());p
}
fn word_run_properties(properties:Vec<XmlNode>)->XmlNode{
    let mut pr=node("rPr",properties);pr.name="w:rPr".into();pr.namespace=WORD_NS.into();pr.attributes.insert("xmlns:w".into(),WORD_NS.into());pr
}
fn merge_mathml_control(target:&mut XmlNode,properties:Vec<XmlNode>){
    fn ensure(n:&mut XmlNode,child:XmlNode)->&mut XmlNode{
        let at=n.content.iter().position(|part|matches!(part,XmlPart::Element(c) if c.local()==child.local()&&c.namespace==child.namespace)).unwrap_or_else(||{n.content.push(XmlPart::Element(child));n.content.len()-1});
        let XmlPart::Element(child)=&mut n.content[at]else{unreachable!()};child
    }
    let name=format!("{}Pr",target.local());
    if !target.children().any(|n|n.local()==name&&is_office(n)){target.content.insert(0,XmlPart::Element(node(&name,vec![])));}
    let pr=ensure(target,node(&name,vec![]));
    let control=ensure(pr,node("ctrlPr",vec![]));
    let word=ensure(control,word_run_properties(vec![]));
    // The translated inner row/operator owns explicit overrides. A transparent
    // outer mstyle supplies only missing defaults, never a second ctrlPr.
    for property in properties{
        if !word.children().any(|n|n.local()==property.local()&&n.namespace==property.namespace){word.content.push(XmlPart::Element(property));}
    }
}
fn apply_mathml_appearance(source:&XmlNode,target:&mut XmlNode,diagnostics:&mut Vec<String>){
    let mut properties=vec![];
    if matches!(target.local(),"func"|"d")&&source.local()=="mo"{
        if let Some(variant)=attr(source,"mathvariant"){
            let bold=matches!(variant.as_str(),"bold"|"bold-italic");let italic=matches!(variant.as_str(),"italic"|"bold-italic");
            properties.extend([word_property("b",if bold{"1"}else{"0"}),word_property("i",if italic{"1"}else{"0"})]);
        }
    }
    if let Some(value)=attr(source,"mathcolor"){
        if let Some(value)=mathml_color(&value){properties.push(word_property("color",&value));}else{diagnostics.push("mathml_style_projection".into());}
    }
    if let Some(value)=attr(source,"mathsize"){
        if let Some(value)=mathml_half_points(&value){properties.push(word_property("sz",&value.to_string()));}else{diagnostics.push("mathml_style_projection".into());}
    }
    if attr(source,"mathbackground").is_some_and(|s|s!="transparent"){diagnostics.push("mathml_style_projection".into());}
    if properties.is_empty(){return;}
    if target.local()=="r"{
        if let Some(pr)=target.content.iter_mut().find_map(|p|if let XmlPart::Element(n)=p{(n.namespace==WORD_NS&&n.local()=="rPr").then_some(n)}else{None}){pr.content.extend(properties.into_iter().map(XmlPart::Element));}
        else{
            // CT_R requires mathematical rPr before Word rPr before text.
            let at=target.content.iter().position(|p|matches!(p,XmlPart::Element(n) if n.namespace==MATH_NS&&n.local()=="rPr")).map_or(0,|i|i+1);
            target.content.insert(at,XmlPart::Element(word_run_properties(properties)));
        }
    }else if matches!(target.local(),"func"|"f"|"rad"|"nary"|"d"|"m"|"eqArr"|"acc"|"bar"|"limLow"|"limUpp"|"borderBox"|"groupChr"|"sSup"|"sSub"|"sSubSup"|"sPre"){
        merge_mathml_control(target,properties);
    }
}
// OMML has one gap rule for the entire matrix. Columns use twips and rows
// whole points; relative lengths use half-em/half-line precision.
// Keep unrepresentable or unequal MathML gaps explicit in diagnostics.
fn mathml_gap(value:&str,points_factor:f64)->Option<(u8,u16)> {
    let mut result=None;
    for length in value.split_whitespace(){
        let (number,factor,rule)=if let Some(v)=length.strip_suffix("em"){(v,2.0,4)}
            else if let Some(v)=length.strip_suffix("pt"){(v,points_factor,3)}
            else if length=="0"{("0",2.0,4)}else{return None;};
        let scaled=number.parse::<f64>().ok()?*factor;
        if !scaled.is_finite()||!(0.0..=u16::MAX as f64).contains(&scaled)||(scaled-scaled.round()).abs()>1e-7{return None;}
        let gap=if rule==4 {match scaled as u16{2=>(0,0),3=>(1,0),4=>(2,0),v=>(4,v)}}else{(rule,scaled as u16)};
        if result.is_some_and(|previous|previous!=gap){return None;}result=Some(gap);
    }
    result
}
fn mathml_column_gap(value:&str)->Option<(u8,u16)> {mathml_gap(value,20.0)}
fn multiscripts(n:&XmlNode,diagnostics:&mut Vec<String>)->XmlNode {
    let children:Vec<_>=n.children().collect();
    let mut base=mathml_node(children[0],diagnostics);
    let split=children.iter().position(|n|n.local()=="mprescripts").unwrap_or(children.len());
    let posts=&children[1..split];
    let pres=if split<children.len(){&children[split+1..]}else{&[]};
    // Office Math has one pair per script object. Further tensor columns are
    // retained as nested objects, with an explicit layout projection warning.
    if posts.len()>2||pres.len()>2{diagnostics.push("mathml_multiscript_projection".into());}
    for pair in posts.chunks_exact(2){
        let sub=pair[0].local()!="none";let sup=pair[1].local()!="none";
        if !sub&&!sup{continue;}
        let mut slots=vec![node("e",vec![base])];
        if sub{slots.push(node("sub",vec![mathml_node(pair[0],diagnostics)]));}
        if sup{slots.push(node("sup",vec![mathml_node(pair[1],diagnostics)]));}
        base=node(if sub&&sup{"sSubSup"}else if sub{"sSub"}else{"sSup"},slots);
    }
    for pair in pres.chunks_exact(2){
        if pair.iter().all(|n|n.local()=="none"){continue;}
        let script=|n:&XmlNode,d:&mut Vec<String>|if n.local()=="none"{vec![]}else{vec![mathml_node(n,d)]};
        // CT_SPre requires sub, sup, e in that order (unlike CT_SSubSup).
        base=node("sPre",vec![node("sub",script(pair[0],diagnostics)),node("sup",script(pair[1],diagnostics)),node("e",vec![base])]);
    }
    base
}

fn validate_mathml(n:&XmlNode)->Result<(),&'static str>{
    if !n.namespace.is_empty()&&n.namespace!="http://www.w3.org/1998/Math/MathML"{return Err("math_invalid_xml");}
    // This finite output profile is not yet a reverse-import syntax. Never
    // flatten its allocation spacers into authored mspace/matrix cells and
    // claim an editable equation-array round trip or recover hidden source.
    if n.attributes.contains_key("data-readmd-mathml-profile"){return Err("math_unsupported_extension");}
    if n.local()=="mtd"&&mathml_matrix_placeholder(n).is_some(){return Ok(());}
    if n.local()=="mspace"{
        // readmdId is an internal interactive-view attribute, not author
        // MathML. Do not silently drop an untrusted source identity field.
        if n.attributes.contains_key("readmdId"){return Err("math_unsupported_extension");}
        let mut space=n.clone();space.namespace=crate::math_spacing::MATHML_NS.into();crate::math_spacing::Spacing::from_xml(&space)?;
    }
    // Inheritable defaults are copied into descendants. Bound each value
    // before materializing the cascade so a small XML input cannot amplify
    // one enormous attribute into gigabytes of cloned strings.
    for key in ["mathvariant","mathsize","mathcolor","linethickness","bevelled","accent","accentunder","stretchy","open","close","separators","columnalign","columnspacing","rowspacing"]{
        if n.attributes.get(key).is_some_and(|value|value.len()>256){return Err("math_invalid_property");}
    }
    let children:Vec<_>=n.children().collect();
    if matches!(n.local(),"math"|"mrow"|"mtd"|"mstyle"|"semantics"){
        let presentation=children.iter().copied().filter(|n|!matches!(n.local(),"annotation"|"annotation-xml")).collect::<Vec<_>>();
        for (i,_) in presentation.iter().enumerate().filter(|(_,n)|mathml_function_operator(n)){
            if i==0||i+1==presentation.len()||mathml_function_empty(presentation[i-1])||mathml_function_empty(presentation[i+1])
                ||mathml_function_operator(presentation[i-1])||mathml_function_operator(presentation[i+1]){
                return Err("math_missing_argument");
            }
        }
    }
    let arity=match n.local(){"mfrac"|"mroot"|"msub"|"msup"|"munder"|"mover"=>Some(2),"msubsup"|"munderover"=>Some(3),_=>None};
    if arity.is_some_and(|count|children.len()!=count){return Err("math_invalid_source");}
    if n.local()=="mmultiscripts"{
        if children.is_empty()||matches!(children[0].local(),"none"|"mprescripts"){return Err("math_invalid_source");}
        let markers:Vec<_>=children.iter().enumerate().filter(|(_,n)|n.local()=="mprescripts").map(|(i,_)|i).collect();
        if markers.len()>1{return Err("math_invalid_source");}
        let split=markers.first().copied().unwrap_or(children.len());
        if (split-1)%2!=0||(split<children.len()&&(children.len()-split-1)%2!=0){return Err("math_invalid_source");}
    }
    for child in children{
        if matches!(child.local(),"none"|"mprescripts")&&n.local()!="mmultiscripts"{return Err("math_invalid_source");}
        validate_mathml(child)?;
    }
    Ok(())
}
/// MathML 3, sections 3.5.1/2/4: a row list overrides the table list,
/// and a cell's single value overrides its row. Office Math can retain that
/// cascade exactly only when all authored cells in a column agree.
fn mathml_table_column_alignments(n:&XmlNode,rows:&[&XmlNode],columns:usize,diagnostics:&mut Vec<String>)->Option<Vec<&'static str>>{
    fn values(n:&XmlNode,single:bool,explicit:&mut bool,projection:&mut bool)->Option<Vec<&'static str>>{
        let value=n.attributes.get("columnalign")?;*explicit=true;
        let values=value.split_whitespace().map(|value|match value{"left"=>Some("left"),"center"=>Some("center"),"right"=>Some("right"),_=>None}).collect::<Option<Vec<_>>>();
        if let Some(values)=values{if !values.is_empty()&&(!single||values.len()==1){return Some(values);}}
        *projection=true;None
    }
    let mut explicit=false;let mut projection=false;
    let table=values(n,false,&mut explicit,&mut projection).unwrap_or_else(||vec!["center"]);
    let at=|values:&[&'static str],column:usize|values.get(column).or_else(||values.last()).copied().unwrap_or("center");
    let mut result=(0..columns).map(|column|at(&table,column)).collect::<Vec<_>>();
    // A span changes the logical column coordinates. Keep the existing table
    // projection and diagnose it rather than folding physical child indices.
    let spans=rows.iter().filter(|row|row.local()=="mtr").flat_map(|row|row.children()).any(|cell|
        ["rowspan","columnspan"].iter().any(|key|cell.attributes.get(*key).is_some_and(|value|value.trim().parse::<u32>().ok()!=Some(1))));
    projection|=spans;
    let mut effective=vec![None;columns];let mut conflicts=vec![false;columns];
    for row in rows.iter().filter(|row|row.local()=="mtr"){
        let row_values=values(row,false,&mut explicit,&mut projection);
        for(column,cell)in row.children().enumerate(){
            let cell_values=values(cell,true,&mut explicit,&mut projection);
            if spans{continue;}
            let alignment=cell_values.as_ref().map(|v|v[0]).or_else(||row_values.as_ref().map(|v|at(v,column))).unwrap_or(result[column]);
            conflicts[column]|=effective[column].is_some_and(|previous|previous!=alignment);
            effective[column]=Some(alignment);
        }
    }
    for(column,alignment)in effective.into_iter().enumerate(){
        if conflicts[column]{projection=true;}else if let Some(alignment)=alignment{result[column]=alignment;}
    }
    if projection{diagnostics.push("mathml_table_alignment_projection".into());}
    explicit.then_some(result)
}
fn mathml_table(n:&XmlNode,diagnostics:&mut Vec<String>)->XmlNode{
    let rows:Vec<_>=n.children().collect();
    let columns=rows.iter().map(|r|r.children().count()).max().unwrap_or(0);
    let mut properties=node("mPr",vec![]);
    if let Some(alignment)=attr(n,"align"){
        let base=match alignment.trim(){"axis"|"axis 0"=>Some("center"),"baseline"|"baseline 1"=>Some("top"),"baseline -1"=>Some("bot"),_=>{diagnostics.push("mathml_table_alignment_projection".into());None}};
        if let Some(base)=base{properties.content.extend(property("mPr","baseJc",base).content);}
    }
    let column_alignments=mathml_table_column_alignments(n,&rows,columns,diagnostics);
    if let Some(alignments)=&column_alignments{
        let mut groups=vec![];
        for alignment in alignments{
            let mut p=property("mcPr","count","1");p.content.extend(property("mcPr","mcJc",alignment).content);
            groups.push(node("mc",vec![p]));
        }
        properties.content.push(XmlPart::Element(node("mcs",groups)));
    }
    if let Some(value)=attr(n,"columnspacing"){
        if let Some((rule,value))=mathml_column_gap(&value){
            properties.content.extend(property("mPr","cGpRule",&rule.to_string()).content);
            if rule>=3{properties.content.extend(property("mPr","cGp",&value.to_string()).content);}
        }else{diagnostics.push("mathml_table_spacing_projection".into());}
    }
    if let Some(value)=attr(n,"rowspacing"){
        if let Some((rule,value))=mathml_gap(&value,1.0){
            properties.content.extend(property("mPr","rSpRule",&rule.to_string()).content);
            if rule>=3{properties.content.extend(property("mPr","rSp",&value.to_string()).content);}
        }else{diagnostics.push("mathml_table_spacing_projection".into());}
    }
    if ["rowalign","columnwidth","equalrows","equalcolumns","frame","rowlines","columnlines","width"].iter().any(|key|attr(n,key).is_some()){
        diagnostics.push("mathml_table_spacing_projection".into());
    }
    // Office's uniform minimum is a CSS minimum, never MathML's fixed
    // columnwidth. Nonuniform/imported CSS cannot be silently flattened.
    let cells:Vec<_>=rows.iter().flat_map(|r|r.children().enumerate()).collect();
    let spacing_view=crate::omml::parse(&format!("<m:m>{}</m:m>",properties.xml())).ok();
    let half_gap=spacing_view.as_ref().and_then(|v|v.children.first()).map(crate::omml::matrix_column_gap).unwrap_or(crate::omml::MathLength::Em(1.0)).half().mathml();
    let minimum=|column:usize,cell:&XmlNode|->Option<u16>{
        let style=attr(cell,"style")?;let declarations=style.split(';').filter(|v|!v.trim().is_empty()).collect::<Vec<_>>();
        let mut css=std::collections::BTreeMap::new();
        for declaration in declarations {let(key,value)=declaration.split_once(':')?;if css.insert(key.trim(),value.trim()).is_some(){return None;}}
        if css.keys().any(|k|!matches!(*k,"box-sizing"|"min-width"|"padding-left"|"padding-right"|"text-align")){return None;}
        if css.len()!=1 {
            if css.get("box-sizing")!=Some(&"content-box")||css.get("padding-left")!=Some(&if column==0{"0pt"}else{half_gap.as_str()})||css.get("padding-right")!=Some(&if column+1==columns{"0pt"}else{half_gap.as_str()}){return None;}
            let expected=column_alignments.as_ref().and_then(|alignments|alignments.get(column)).copied().unwrap_or("center");
            if css.get("text-align").is_some_and(|v|*v!=expected){return None;}
        }
        let value=css.get("min-width").copied().unwrap_or("0pt").strip_suffix("pt")?.parse::<f64>().ok()?*20.;
        if value.is_finite()&&(0.0..=31680.0).contains(&value)&&value.fract()==0.0{Some(value as u16)}else{None}
    };
    if cells.iter().any(|(_,c)|attr(c,"style").is_some()) {
        let first=cells.first().and_then(|(i,c)|minimum(*i,c));
        if first.is_some()&&cells.iter().all(|(i,c)|minimum(*i,c)==first){
            if cells.iter().any(|(_,c)|attr(c,"style").is_some_and(|v|v.split(';').any(|part|part.split_once(':').is_some_and(|(key,_)|key.trim()=="min-width")))){properties.content.extend(property("mPr","cSp",&first.unwrap().to_string()).content);}
        }
        else{diagnostics.push("mathml_table_spacing_projection".into());}
    }
    let mut placeholder_visibility=None;
    let mut mixed_visibility=false;
    let contents_rows=rows.into_iter().map(|r|{
        if r.local()!="mtr"{return mathml_node(r,diagnostics);}
        node("mr",r.children().map(|cell|{
            if let Some((visible,size))=mathml_matrix_placeholder(cell){
                mixed_visibility|=placeholder_visibility.is_some_and(|old|old!=visible);
                placeholder_visibility=Some(visible);
                return node("e",if size==0{vec![]}else{vec![property("argPr","argSz",&size.to_string())]});
            }
            node("e",vec![mathml_node(cell,diagnostics)])
        }).collect())
    }).collect::<Vec<_>>();
    if mixed_visibility{diagnostics.push("mathml_table_spacing_projection".into());}
    if placeholder_visibility==Some(false){properties.content.extend(property("mPr","plcHide","1").content);}
    let mut contents=vec![];if !properties.content.is_empty(){contents.push(properties);}
    contents.extend(contents_rows);
    node("m",contents)
}

/// Recognize only the generated, text-free empty-cell shape. Arbitrary mspace
/// dimensions/content remain authored spacing, never an inferred empty slot.
fn mathml_matrix_placeholder(cell:&XmlNode)->Option<(bool,i8)>{
    let mut n=cell;let mut size=0i8;
    loop {
        if !n.namespace.is_empty()&&n.namespace!=crate::math_spacing::MATHML_NS{return None;}
        if n.local()=="mspace"{break;}
        if !matches!(n.local(),"mtd"|"mrow"|"mstyle")||!n.text().trim().is_empty(){return None;}
        if n.content.iter().any(|part|matches!(part,XmlPart::Comment(_)|XmlPart::Instruction(_))){return None;}
        if n.attribute_namespaces.iter().any(|(key,ns)|!key.starts_with("xmlns")&&!ns.is_empty()){return None;}
        if n.attributes.keys().any(|key|!matches!(key.as_str(),"xmlns"|"mathcolor")&&!key.starts_with("xmlns:")&&match n.local(){"mtd"=>key!="style","mrow"=>key!="id","mstyle"=>key!="scriptlevel",_=>true}){return None;}
        if n.local()=="mstyle"{
            let level=attr(n,"scriptlevel")?;
            if !level.starts_with(['+','-']){return None;}
            let relative=level.parse::<i8>().ok()?;
            size=size.checked_sub(relative)?;
            if !(-2..=2).contains(&size){return None;}
        }
        let children=n.children().collect::<Vec<_>>();if children.len()!=1{return None;}n=children[0];
    }
    if n.content.iter().any(|part|!matches!(part,XmlPart::Text(value) if value.trim().is_empty())){return None;}
    if attr(n,"data-readmd-matrix-placeholder").as_deref()!=Some("true"){return None;}
    if n.attributes.keys().any(|key|!matches!(key.as_str(),"width"|"height"|"depth"|"style"|"mathsize"|"data-readmd-matrix-placeholder"|"xmlns")&&!key.starts_with("xmlns:")){return None;}
    if n.attribute_namespaces.iter().any(|(key,ns)|!key.starts_with("xmlns")&&!ns.is_empty()){return None;}
    let(w,h,d,stroke)=crate::omml::MATRIX_PLACEHOLDER;
    for(key,value)in[("width",w),("height",h),("depth",d)]{if attr(n,key).as_deref()!=Some(&format!("{value}em")){return None;}}
    let style=attr(n,"style");
    let shown=format!("outline:{stroke}em solid currentColor;outline-offset:-{stroke}em");
    if style.as_deref().is_some_and(|v|v!=shown){return None;}
    Some((style.is_some(),size))
}
fn mathml_run(n:&XmlNode,diagnostics:&mut Vec<String>)->XmlNode{
    let mut r=run(&n.text());let mut pr=node("rPr",vec![]);
    let text=n.local()=="mtext";
    if text{pr.content.extend(property("rPr","nor","1").content);}
    let default=if n.local()=="mi"&&n.text().trim().chars().count()==1{"italic"}else{"normal"};
    if let Some(variant)=attr(n,"mathvariant").or_else(||Some(default.into())){
        let(style,script)=match variant.as_str(){
            "normal"=>(Some("p"),None),"bold"=>(Some("b"),None),"bold-italic"=>(Some("bi"),None),"italic"=>(Some("i"),None),
            "double-struck"=>(Some("p"),Some("double-struck")),"script"=>(Some("p"),Some("script")),"bold-script"=>(Some("b"),Some("script")),
            "fraktur"=>(Some("p"),Some("fraktur")),"bold-fraktur"=>(Some("b"),Some("fraktur")),
            "sans-serif"=>(Some("p"),Some("sans-serif")),"bold-sans-serif"=>(Some("b"),Some("sans-serif")),
            "sans-serif-italic"=>(Some("i"),Some("sans-serif")),"sans-serif-bold-italic"=>(Some("bi"),Some("sans-serif")),
            "monospace"=>(Some("p"),Some("monospace")),_=>{diagnostics.push("mathml_unsupported_variant".into());(None,None)}
        };
        if text{
            // nor and scr/sty are mutually exclusive. Literal text bold/italic
            // belongs to w:rPr, not the mathematical alphabet properties.
            if matches!(variant.as_str(),"normal"|"bold"|"italic"|"bold-italic")&&attr(n,"mathvariant").is_some(){
                let bold=matches!(variant.as_str(),"bold"|"bold-italic");let italic=matches!(variant.as_str(),"italic"|"bold-italic");
                r.content.insert(0,XmlPart::Element(word_run_properties(vec![word_property("b",if bold{"1"}else{"0"}),word_property("i",if italic{"1"}else{"0"})])));
            }else if variant!="normal"{diagnostics.push("mathml_text_style_projection".into());}
        }else{
            if let Some(script)=script{pr.content.extend(property("rPr","scr",script).content);}
            if let Some(style)=style{pr.content.extend(property("rPr","sty",style).content);}
        }
    }
    if !pr.content.is_empty(){r.content.insert(0,XmlPart::Element(pr));}
    r
}

fn core_operator(n: &XmlNode) -> Option<&XmlNode> {
    if n.local()=="mo" { return Some(n); }
    if matches!(n.local(),"mrow"|"mstyle"|"semantics") {
        let mut children=n.children().filter(|n|!matches!(n.local(),"annotation"|"annotation-xml"));
        let first=children.next()?;
        if children.next().is_none() { return core_operator(first); }
    }
    None
}

/// Accent/limit are different Office objects, not interchangeable layout
/// aliases. Explicit false always wins over the operator's dictionary default.
fn mathml_attachment(base:XmlNode, mark:&XmlNode, under:bool, explicit:Option<&str>, diagnostics:&mut Vec<String>)->XmlNode {
    let operator=core_operator(mark);
    let text=operator.map(XmlNode::text).unwrap_or_default();
    let character={let mut chars=text.trim().chars();chars.next().filter(|_|chars.next().is_none())};
    let dictionary=character.is_some_and(|c|matches!(c,
        '\u{0300}'..='\u{036f}'|'\u{20d0}'..='\u{20ff}'|'^'|'~'|'¯'|'‾'|'_'|'˙'|'¨'|'ˇ'|'˘'|'´'|'`'|
        '→'|'←'|'↔'|'⏞'|'⏟'|'⎴'|'⎵'|'⏜'|'⏝'));
    let boolean=|value:&str|if value.eq_ignore_ascii_case("true"){Some(true)}else if value.eq_ignore_ascii_case("false"){Some(false)}else{None};
    let accent=explicit.and_then(boolean).or_else(||operator.and_then(|n|attr(n,"accent")).as_deref().and_then(boolean)).unwrap_or(dictionary);
    if explicit.is_some_and(|v|boolean(v).is_none()) { diagnostics.push("mathml_style_projection".into()); }
    if accent && operator.is_some_and(|n|attr(n,"stretchy").as_deref()!=Some("false")) {
        if let Some(ch)=character {
            let (kind,container,properties)=if (!under&&matches!(ch,'¯'|'‾'|'\u{0304}'|'\u{0305}'))||(under&&matches!(ch,'_'|'\u{0332}')) {
                ("bar","barPr",vec![("pos",if under{"bot"}else{"top"}.to_string())])
            }else if under||matches!(ch,'⏞'|'⏟'|'⎴'|'⎵'|'⏜'|'⏝') {
                ("groupChr","groupChrPr",vec![("chr",ch.to_string()),("pos",if under{"bot"}else{"top"}.to_string()),("vertJc",if under{"top"}else{"bot"}.to_string())])
            }else{("acc","accPr",vec![("chr",ch.to_string())])};
            let mut pr=node(container,vec![]);
            for (key,value) in properties { pr.content.extend(property(container,key,&value).content); }
            return node(kind,vec![pr,node("e",vec![base])]);
        }
    }
    if accent { diagnostics.push("mathml_style_projection".into()); }
    node(if under{"limLow"}else{"limUpp"},vec![node("e",vec![base]),node("lim",vec![mathml_node(mark,diagnostics)])])
}
/// A zero-width non-slanted trailing atom explicitly aligns a script pair.
/// Accept only the exact, harmless wrapper emitted by our MathML adapter.
fn mathml_aligned_base(n:&XmlNode)->Option<&XmlNode>{
    if n.local()!="mrow" { return None; }
    let c=n.children().collect::<Vec<_>>();
    if c.len()!=2||c[0].local()!="mrow"||c[1].local()!="mspace"||attr(c[1],"width").as_deref()!=Some("0em")||c[1].children().next().is_some()||!c[1].text().trim().is_empty(){return None;}
    let allowed=|n:&XmlNode,space:bool|n.attributes.keys().all(|k|matches!(k.as_str(),"xmlns"|"mathcolor"|"mathvariant"|"mathsize")||k.starts_with("xmlns:")||space&&k=="width");
    if !allowed(n,false)||!allowed(c[1],true){return None;}Some(c[0])
}
fn mathml_function_operator(n:&XmlNode)->bool{
    (n.namespace.is_empty()||n.namespace==crate::math_spacing::MATHML_NS)&&n.local()=="mo"&&n.children().next().is_none()&&n.text().trim()=="\u{2061}"
}
fn mathml_function_empty(n:&XmlNode)->bool{
    if n.local()=="mo"&&(attr(n,"fence").as_deref()==Some("true")||attr(n,"separator").as_deref()==Some("true")){return true;}
    if matches!(n.local(),"mi"|"mn"|"mo"|"mtext"|"ms"){return n.text().trim().is_empty();}
    n.children().filter(|n|!matches!(n.local(),"annotation"|"annotation-xml")).all(mathml_function_empty)
}
fn mathml_row_attributes(n:&XmlNode)->bool{
    (n.namespace.is_empty()||n.namespace==crate::math_spacing::MATHML_NS)
        &&n.attributes.keys().all(|key|matches!(key.as_str(),"xmlns"|"id"|"mathcolor")||key.starts_with("xmlns:"))
        &&n.attribute_namespaces.iter().all(|(key,ns)|key.starts_with("xmlns")||ns.is_empty())
}
/// The single-slot delimiter emitted by ReadMD has explicit matching fence
/// flags. Bare parenthesis characters or unrelated mo rows are never inferred.
fn mathml_generated_delimiter(n:&XmlNode)->Option<[&XmlNode;3]>{
    if n.local()!="mrow"||!mathml_row_attributes(n)||n.content.iter().any(|part|!matches!(part,XmlPart::Element(_))&&!matches!(part,XmlPart::Text(value) if value.trim().is_empty())){return None;}
    let children=n.children().collect::<Vec<_>>();if children.len()!=3||children[1].local()!="mrow"{return None;}
    for fence in [children[0],children[2]]{
        if !fence.namespace.is_empty()&&fence.namespace!=crate::math_spacing::MATHML_NS{return None;}
        if fence.local()!="mo"||fence.children().next().is_some()||fence.text().chars().count()!=1||attr(fence,"fence").as_deref()!=Some("true"){return None;}
        if !matches!(attr(fence,"stretchy").as_deref(),Some("true"|"false"))||!matches!(attr(fence,"symmetric").as_deref(),Some("true"|"false")){return None;}
        if fence.content.iter().any(|part|!matches!(part,XmlPart::Text(_)))||fence.attributes.iter().any(|(key,value)|!matches!(key.as_str(),"xmlns"|"id"|"mathcolor"|"mathsize"|"fence"|"stretchy"|"symmetric")&&!key.starts_with("xmlns:")&&!(key=="mathvariant"&&matches!(value.as_str(),"normal"|"bold"|"italic"|"bold-italic"))){return None;}
        if fence.attribute_namespaces.iter().any(|(key,ns)|!key.starts_with("xmlns")&&!ns.is_empty()){return None;}
    }
    if !mathml_row_attributes(children[1]){return None;}
    for key in ["stretchy","symmetric","mathcolor","mathsize","mathvariant"]{if attr(children[0],key)!=attr(children[2],key){return None;}}
    Some([children[0],children[1],children[2]])
}
/// Recognize the explicit three-part row emitted for an Office function. Do
/// not infer application from names, juxtaposition, or ungrouped sibling rows.
fn mathml_function_row(n:&XmlNode,diagnostics:&mut Vec<String>)->XmlNode{
    let children=n.children().filter(|n|!matches!(n.local(),"annotation"|"annotation-xml")).collect::<Vec<_>>();
    let content_only=n.content.iter().all(|part|matches!(part,XmlPart::Element(_))||matches!(part,XmlPart::Text(value) if value.trim().is_empty()));
    let row_attributes=n.local()!="mrow"||mathml_row_attributes(n);
    if let Some([begin,body,end])=mathml_generated_delimiter(n){
        let mut pr=property("dPr","begChr",&begin.text());pr.content.extend(property("dPr","endChr",&end.text()).content);
        pr.content.extend(property("dPr","grow",if attr(begin,"stretchy").as_deref()==Some("true"){"1"}else{"0"}).content);
        pr.content.extend(property("dPr","shp",if attr(begin,"symmetric").as_deref()==Some("true"){"centered"}else{"match"}).content);
        let mut delimiter=node("d",vec![pr,node("e",vec![mathml_node(body,diagnostics)])]);
        apply_mathml_appearance(begin,&mut delimiter,diagnostics);return delimiter;
    }
    let operator=children.get(1).copied();
    let supported=operator.is_some_and(|operator|mathml_function_operator(operator)
        &&operator.content.iter().all(|part|matches!(part,XmlPart::Text(_)))
        &&operator.attributes.iter().all(|(key,value)|matches!(key.as_str(),"xmlns"|"mathcolor"|"mathsize")||key.starts_with("xmlns:")
            ||key=="mathvariant"&&matches!(value.as_str(),"normal"|"bold"|"italic"|"bold-italic")||key=="form"&&value=="infix")
        &&operator.attribute_namespaces.iter().all(|(key,ns)|key.starts_with("xmlns")||ns.is_empty()));
    if children.len()==3&&content_only&&row_attributes&&supported{
        let mut function=node("func",vec![node("fName",vec![mathml_node(children[0],diagnostics)]),node("e",vec![mathml_node(children[2],diagnostics)])]);
        apply_mathml_appearance(operator.unwrap(),&mut function,diagnostics);return function;
    }
    let mut content=children.into_iter().map(|n|mathml_node(n,diagnostics)).collect::<Vec<_>>();
    if content_only&&content.len()==1&&content[0].local()=="func"{
        if row_attributes{return content.remove(0);}
        diagnostics.push("mathml_function_application_projection".into());
    }
    node("e",content)
}
fn mathml_node(n:&XmlNode,diagnostics:&mut Vec<String>)->XmlNode {
    let c:Vec<_>=n.children().collect();let translated=|i:usize,d:&mut Vec<String>|c.get(i).map(|n|mathml_node(n,d)).unwrap_or_else(||run(""));
    let slot=|name:&str,i:usize,d:&mut Vec<String>|node(name,vec![translated(i,d)]);
    let all=|d:&mut Vec<String>|c.iter().map(|n|mathml_node(n,d)).collect();
    let mut result=match n.local(){
        "math"=>node("oMath",all(diagnostics)),"mrow"|"mtd"=>mathml_function_row(n,diagnostics),
        "semantics"=>{let presentation=c.iter().filter(|n|!matches!(n.local(),"annotation"|"annotation-xml")).collect::<Vec<_>>();if presentation.len()==1{mathml_node(presentation[0],diagnostics)}else{node("e",presentation.into_iter().map(|n|mathml_node(n,diagnostics)).collect())}},
        "mi"|"mn"|"mo"|"mtext"|"ms"=>{if n.local()=="mo"&&n.text().contains('\u{2061}'){diagnostics.push("mathml_function_application_projection".into());}if n.local()=="mo"&&(attr(n,"fence").as_deref()==Some("true")||attr(n,"separator").as_deref()==Some("true")){diagnostics.push("mathml_delimiter_projection".into());}mathml_run(n,diagnostics)},
        "mfrac"=>{let mut f=node("f",vec![slot("num",0,diagnostics),slot("den",1,diagnostics)]);if attr(n,"linethickness").is_some_and(|v|matches!(v.as_str(),"0"|"0px"|"0pt")){f.content.insert(0,XmlPart::Element(property("fPr","type","noBar")));}else if attr(n,"bevelled").as_deref()==Some("true"){f.content.insert(0,XmlPart::Element(property("fPr","type","skw")));}f},
        "msqrt"=>node("rad",vec![property("radPr","degHide","1"),node("deg",vec![]),node("e",all(diagnostics))]),
        "mroot"=>node("rad",vec![property("radPr","degHide","0"),slot("deg",1,diagnostics),slot("e",0,diagnostics)]),
        "msub"=>node("sSub",vec![slot("e",0,diagnostics),slot("sub",1,diagnostics)]),
        "msup"=>node("sSup",vec![slot("e",0,diagnostics),slot("sup",1,diagnostics)]),
        "msubsup"=>{let aligned=mathml_aligned_base(c[0]);let mut parts=vec![];if let Some(base)=aligned{parts.push(property("sSubSupPr","alnScr","1"));parts.push(node("e",vec![mathml_node(base,diagnostics)]));}else{parts.push(slot("e",0,diagnostics));}parts.extend([slot("sub",1,diagnostics),slot("sup",2,diagnostics)]);node("sSubSup",parts)},
        "mmultiscripts"=>multiscripts(n,diagnostics),
        "mstyle"=>{
            let size=attr(n,"scriptlevel").and_then(|value|{
                // Signed MathML values are relative; unsigned values are
                // absolute and need a context-aware conversion, so report them.
                if !value.starts_with('+')&&!value.starts_with('-'){return None;}
                value.parse::<i8>().ok().filter(|v|(-2..=2).contains(v)).map(|v|-v)
            });
            if attr(n,"scriptlevel").is_some()&&size.is_none(){diagnostics.push("mathml_scriptlevel_projection".into());}
            if n.attributes.keys().any(|key|!matches!(key.as_str(),"scriptlevel"|"xmlns"|"mathvariant"|"mathsize"|"mathcolor"|"mathbackground"|"linethickness"|"bevelled"|"accent"|"accentunder"|"stretchy"|"open"|"close"|"separators"|"columnalign"|"columnspacing"|"rowspacing")&&!key.starts_with("xmlns:")){diagnostics.push("mathml_style_projection".into());}
            let mut content=if c.len()==3&&mathml_function_operator(c[1]){vec![mathml_function_row(n,diagnostics)]}else{all(diagnostics)};
            if attr(n,"scriptlevel").is_none()&&content.len()==1&&content[0].local()=="func"{content.remove(0)}
            else{if let Some(size)=size{content.insert(0,property("argPr","argSz",&size.to_string()));}node("box",vec![node("e",content)])}
        },
        "munder"|"mover"=>{let under=n.local()=="munder";mathml_attachment(translated(0,diagnostics),c[1],under,attr(n,if under{"accentunder"}else{"accent"}).as_deref(),diagnostics)},
        "munderover"=>{let base=mathml_attachment(translated(0,diagnostics),c[1],true,attr(n,"accentunder").as_deref(),diagnostics);mathml_attachment(base,c[2],false,attr(n,"accent").as_deref(),diagnostics)},
        "mtable"=>mathml_table(n,diagnostics),"mtr"|"mlabeledtr"=>{if n.local()=="mlabeledtr"{diagnostics.push("mathml_labeled_row".into());}node("mr",c.iter().map(|cell|node("e",vec![mathml_node(cell,diagnostics)])).collect())},
        "mfenced"=>{let mut pr=property("dPr","begChr",attr(n,"open").as_deref().unwrap_or("("));if let XmlPart::Element(p)=&mut pr.content[0]{p.name="m:begChr".into();}let mut end=property("dPr","endChr",attr(n,"close").as_deref().unwrap_or(")"));pr.content.append(&mut end.content);let mut sep=property("dPr","sepChr",attr(n,"separators").as_deref().unwrap_or(","));pr.content.append(&mut sep.content);let mut parts=vec![pr];parts.extend(c.iter().map(|n|node("e",vec![mathml_node(n,diagnostics)])));node("d",parts)},
        "mphantom"=>node("phant",vec![property("phantPr","show","0"),node("e",all(diagnostics))]),
        "menclose"=>{let notation=attr(n,"notation").unwrap_or_else(||"longdiv".into());let mut pr=node("borderBoxPr",vec![]);for(key,side)in[("hideTop","top"),("hideBot","bottom"),("hideLeft","left"),("hideRight","right")]{let visible=notation.split_whitespace().any(|v|v==side||v=="box");let mut p=property("borderBoxPr",key,if visible{"0"}else{"1"});pr.content.append(&mut p.content);}for(key,name)in[("strikeH","horizontalstrike"),("strikeV","verticalstrike"),("strikeBLTR","updiagonalstrike"),("strikeTLBR","downdiagonalstrike")]{if notation.split_whitespace().any(|v|v==name){let mut p=property("borderBoxPr",key,"1");pr.content.append(&mut p.content);}}if notation.split_whitespace().any(|v|!matches!(v,"box"|"top"|"bottom"|"left"|"right"|"horizontalstrike"|"verticalstrike"|"updiagonalstrike"|"downdiagonalstrike")){diagnostics.push("mathml_unsupported_enclosure".into());}node("borderBox",vec![pr,node("e",all(diagnostics))])},
        "mspace"=>{
            let space=crate::math_spacing::Spacing::from_attributes(n.attributes.iter().map(|(key,value)|(key.as_str(),value.as_str()))).expect("validated bounded MathML space");
            let mut result=space.node();result.attributes.insert(crate::math_spacing::SOURCE_ID.into(),n.id.clone());result
        },"none"=>node("e",vec![]),
        _=>{diagnostics.push(format!("mathml_unsupported_{}",n.local()));node("e",all(diagnostics))},
    };
    if n.local()!="mspace"{apply_mathml_appearance(n,&mut result,diagnostics);}result
}
pub struct MathMlImport {pub root:XmlNode,pub nodes:Vec<crate::math_document::SourceNode>,pub diagnostics:Vec<String>}

fn mathml_element_spans(source:&str)->Result<std::collections::BTreeMap<String,(usize,usize)>,&'static str>{
    use quick_xml::{Reader,events::Event};
    let mut reader=Reader::from_str(source);
    let mut stack=vec![("x".to_string(),0usize,0usize)];let mut result=std::collections::BTreeMap::new();
    loop{
        let from=reader.buffer_position() as usize;
        match reader.read_event().map_err(|_|"math_invalid_xml")?{
            Event::Start(_)=>{let parent=stack.last_mut().ok_or("math_invalid_xml")?;let id=format!("{}.{}",parent.0,parent.1);parent.1+=1;stack.push((id,0,from));},
            Event::Empty(_)=>{let parent=stack.last_mut().ok_or("math_invalid_xml")?;let id=format!("{}.{}",parent.0,parent.1);parent.1+=1;result.insert(id,(from,reader.buffer_position() as usize));},
            Event::End(_)=>{if stack.len()<=1{return Err("math_invalid_xml");}let(id,_,start)=stack.pop().unwrap();result.insert(id,(start,reader.buffer_position() as usize));},
            Event::Eof=>break,_=>(),
        }
    }
    Ok(result)
}
fn mathml_space_source_slots(source:&str,from:usize,to:usize)->Vec<crate::math_document::SourceSlot>{
    let opening=&source[from..to];let bytes=opening.as_bytes();let mut at=1;let mut result=Vec::new();
    while at<bytes.len()&&!bytes[at].is_ascii_whitespace()&&!matches!(bytes[at],b'>'|b'/'){at+=1;}
    loop{
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace){at+=1;}
        if at>=bytes.len()||matches!(bytes[at],b'>'|b'/'){break;}
        let start=at;while at<bytes.len()&&!bytes[at].is_ascii_whitespace()&&bytes[at]!=b'='{at+=1;}
        let key=&opening[start..at];while bytes.get(at).is_some_and(u8::is_ascii_whitespace){at+=1;}
        if bytes.get(at)!=Some(&b'='){break;}at+=1;while bytes.get(at).is_some_and(u8::is_ascii_whitespace){at+=1;}
        let Some(quote @ (b'\''|b'"'))=bytes.get(at).copied() else{break;};at+=1;let start=at;
        while at<bytes.len()&&bytes[at]!=quote{at+=1;}if at>=bytes.len(){break;}
        if matches!(key,"width"|"height"|"depth"|"mathsize"){
            result.push(crate::math_document::SourceSlot{role:key.into(),from:source[..from+start].encode_utf16().count(),to:source[..from+at].encode_utf16().count()});
        }at+=1;
    }result
}

pub fn import_mathml(source:&str)->Result<MathMlImport,&'static str>{
    let parsed=crate::math_document::parse_xml(source)?;let roots:Vec<_>=parsed.children().collect();
    if roots.len()!=1||roots[0].local()!="math"{return Err("math_invalid_xml");}
    validate_mathml(roots[0])?;
    let mut normalized=roots[0].clone();mathml_inheritance(&mut normalized,&Default::default(),None);
    // Inherited sizes are input too: reject an unsupported/invalid cascade
    // before a validated space is converted into the shared tree.
    validate_mathml(&normalized)?;
    let mut diagnostics=vec![];let mut root=mathml_node(&normalized,&mut diagnostics);
    // Office property containers are ordered sequences. In particular dPr
    // requires begChr, sepChr, endChr, and mPr puts spacing before mcs. Sort
    // generated properties by the fixed schema before both export and DSL
    // decompilation; never normalize or rewrite imported author OMML here.
    fn order(n:&mut XmlNode){
        if n.namespace==MATH_NS&&n.local().ends_with("Pr"){
            if let Some(children)=crate::math_document::schema()["elements"].as_array().and_then(|elements|elements.iter().find(|e|e["element"]==n.local())).and_then(|e|e["children"].as_array()){
                n.content.sort_by_key(|part|if let XmlPart::Element(child)=part{children.iter().position(|name|name.as_str()==Some(child.local())).unwrap_or(usize::MAX)}else{usize::MAX});
            }
        }
        for part in &mut n.content{if let XmlPart::Element(child)=part{order(child);}}
    }
    order(&mut root);
    root.attributes.insert("xmlns:m".into(),MATH_NS.into());
    let spans=mathml_element_spans(source)?;
    let mut root=crate::math_document::parse_xml(&root.xml())?;let mut nodes=Vec::new();
    fn collect(node:&mut XmlNode,source:&str,spans:&std::collections::BTreeMap<String,(usize,usize)>,nodes:&mut Vec<crate::math_document::SourceNode>)->Result<(),&'static str>{
        if let Some(id)=node.attributes.remove(crate::math_spacing::SOURCE_ID){
            let &(from,to)=spans.get(&id).ok_or("math_invalid_source")?;
            nodes.push(crate::math_document::SourceNode{id:node.id.clone(),kind:"mspace".into(),
                from:source[..from].encode_utf16().count(),to:source[..to].encode_utf16().count(),tex:source[from..to].into(),slots:mathml_space_source_slots(source,from,to)});
        }
        for part in &mut node.content{if let XmlPart::Element(child)=part{collect(child,source,spans,nodes)?;}}Ok(())
    }
    collect(&mut root,source,&spans,&mut nodes)?;crate::math_spacing::validate_tree(&root)?;
    Ok(MathMlImport{root,nodes,diagnostics})
}
pub fn from_mathml(source:&str)->Result<(String,Vec<String>),&'static str>{
    let result=import_mathml(source)?;Ok((result.root.xml(),result.diagnostics))
}

/// Compatibility TeX projection. Structural consumers must use the shared
/// UnicodeMath compiler, whose diagnostics and ranges belong to the input.
pub fn from_unicode(source:&str)->Result<String,&'static str>{
    let (root, _, diagnostics) = crate::unicode_math::compile(source, false)?;
    if !diagnostics.is_empty() { return Err("math_unknown_macro"); }
    Ok(tex(&root))
}

#[cfg(test)]mod tests{
    fn native_function_nodes(n:&XmlNode)->Vec<&XmlNode>{
        let mut functions=vec![];if n.local()=="func"{functions.push(n);}for c in n.children(){functions.extend(native_function_nodes(c));}functions
    }
    fn native_find<'a>(n:&'a XmlNode,local:&str)->Option<&'a XmlNode>{if n.local()==local{Some(n)}else{n.children().find_map(|c|native_find(c,local))}}
    fn native_count(n:&XmlNode,local:&str)->usize{usize::from(n.local()==local)+n.children().map(|c|native_count(c,local)).sum::<usize>()}
    fn function_signatures(n:&XmlNode)->Vec<(String,String,usize,usize,usize)>{
        native_function_nodes(n).into_iter().map(|f|{
            let name=child(f,"fName").unwrap();let argument=child(f,"e").unwrap();
            (name.text(),argument.text(),native_count(name,"sSup"),native_count(name,"d"),native_count(argument,"d"))
        }).collect()
    }
    #[test]fn explicit_functions_keep_native_roles_through_real_mathml_and_readable_roundtrips(){
        for source in ["sin⁡(x+1)+sin⁡(y+2)","sin^2⁡x","sin⁡^2 x","sin⁡〖x+1〗","f⁡(g⁡x)","(f+g)⁡x"]{
            let original=crate::unicode_math::compile(source,false).unwrap().0;let expected=function_signatures(&original);
            let mathml=crate::omml::to_mathml(&crate::omml::parse(&original.xml()).unwrap(),false,"");
            let(xml,diagnostics)=from_mathml(&mathml).unwrap();assert!(diagnostics.is_empty(),"{source}: {mathml}: {diagnostics:?}");
            let imported=crate::math_document::parse_xml(&xml).unwrap();assert_eq!(function_signatures(&imported),expected,"{source}: {xml}");
            let readable=crate::math_dsl::decompile(&imported).unwrap();let(rebuilt,nodes)=crate::math_dsl::compile(&readable,false).unwrap();
            assert_eq!(nodes.iter().filter(|n|n.kind=="func").count(),expected.len());
            assert_eq!(crate::math_document::xml_semantics(&rebuilt),crate::math_document::xml_semantics(&imported));
            let again=crate::omml::to_mathml(&crate::omml::parse(&rebuilt.xml()).unwrap(),false,"");
            let(xml,diagnostics)=from_mathml(&again).unwrap();assert!(diagnostics.is_empty(),"{source}: {diagnostics:?}");
            assert_eq!(function_signatures(&crate::math_document::parse_xml(&xml).unwrap()),expected);
        }
    }
    #[test]fn explicit_function_mathml_keeps_token_variants_and_native_control_format(){
        let source=r#"<m:oMath xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><m:func><m:funcPr><m:ctrlPr><w:rPr><w:color w:val="336699"/><w:sz w:val="48"/></w:rPr></m:ctrlPr></m:funcPr><m:fName><m:r><m:rPr><m:scr m:val="script"/><m:sty m:val="b"/></m:rPr><w:rPr><w:color w:val="FF0000"/><w:sz w:val="24"/></w:rPr><m:t>F</m:t></m:r></m:fName><m:e><m:r><m:rPr><m:scr m:val="double-struck"/><m:sty m:val="p"/></m:rPr><w:rPr><w:color w:val="008000"/><w:sz w:val="32"/></w:rPr><m:t>x</m:t></m:r></m:e></m:func></m:oMath>"#;
        let mathml=crate::omml::to_mathml(&crate::omml::parse(source).unwrap(),false,"");
        let(xml,diagnostics)=from_mathml(&mathml).unwrap();assert!(diagnostics.is_empty(),"{mathml}: {diagnostics:?}");
        let imported=crate::math_document::parse_xml(&xml).unwrap();let functions=native_function_nodes(&imported);assert_eq!(functions.len(),1);
        let function=functions[0];assert_eq!(native_count(child(function,"funcPr").unwrap(),"ctrlPr"),1);
        let name=native_find(child(function,"fName").unwrap(),"r").unwrap();let argument=native_find(child(function,"e").unwrap(),"r").unwrap();
        assert_eq!(prop(name,"rPr","scr"),Some("script"));assert_eq!(prop(name,"rPr","sty"),Some("b"));assert_eq!(prop(argument,"rPr","scr"),Some("double-struck"));
        let tree=crate::omml::parse(&xml).unwrap();
        fn find<'a>(n:&'a crate::omml::Node,name:&str,text:Option<&str>)->Option<&'a crate::omml::Node>{if n.name==name&&text.is_none_or(|text|n.run_text()==text){Some(n)}else{n.children.iter().find_map(|n|find(n,name,text))}}
        let control=crate::omml::control_style(find(&tree,"func",None).unwrap());assert_eq!(control.color,Some([0x33,0x66,0x99]));assert_eq!(control.half_points,Some(48));
        let name=crate::omml::run_style(find(&tree,"r",Some("F")).unwrap());let argument=crate::omml::run_style(find(&tree,"r",Some("x")).unwrap());
        assert!(name.bold);assert_eq!((name.color,name.half_points),(Some([255,0,0]),Some(24)));assert_eq!((argument.color,argument.half_points),(Some([0,128,0]),Some(32)));
        let dsl=crate::math_dsl::decompile(&imported).unwrap();let(rebuilt,_)=crate::math_dsl::compile(&dsl,false).unwrap();assert_eq!(crate::math_document::xml_semantics(&rebuilt),crate::math_document::xml_semantics(&imported));
    }
    #[test]fn function_style_wrappers_merge_inner_overrides_once_and_keep_automatic_color(){
        let source=r#"<math><mstyle mathcolor="red" mathsize="18pt"><mstyle mathcolor="navy" mathsize="24pt"><mrow><mi mathvariant="bold-script">F</mi><mo>&#x2061;</mo><mi mathvariant="double-struck">x</mi></mrow></mstyle></mstyle></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
        let root=crate::math_document::parse_xml(&xml).unwrap();let function=native_function_nodes(&root)[0];let control=child(function,"funcPr").unwrap();assert_eq!(native_count(control,"ctrlPr"),1);
        assert_eq!(control.text(),"");let word=native_find(control,"rPr").unwrap();
        assert_eq!(word.children().filter(|n|n.local()=="color").count(),1);assert_eq!(word.children().filter(|n|n.local()=="sz").count(),1);
        let tree=crate::omml::parse(&xml).unwrap();let style=crate::omml::control_style(tree.child("oMath").unwrap().child("func").unwrap());assert_eq!((style.color,style.half_points),(Some([0,0,128]),Some(48)));
        let source=r#"<m:oMath xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><m:func><m:funcPr><m:ctrlPr><w:rPr><w:sz w:val="36"/></w:rPr></m:ctrlPr></m:funcPr><m:fName><m:r><m:t>sin</m:t></m:r></m:fName><m:e><m:r><m:t>x</m:t></m:r></m:e></m:func></m:oMath>"#;
        let mathml=crate::omml::to_mathml(&crate::omml::parse(source).unwrap(),false,"");assert!(mathml.contains("CanvasText"));
        let(xml,diagnostics)=from_mathml(&mathml).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");assert!(xml.contains("w:val=\"auto\""));
        assert_eq!(native_function_nodes(&crate::math_document::parse_xml(&xml).unwrap()).len(),1);
    }
    #[test]fn explicit_function_semantics_use_only_the_presentation_branch(){
        let source=r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><semantics><mrow><mrow><mi>f</mi><mo>+</mo><mi>g</mi></mrow><mo>&#x2061;</mo><mi>x</mi></mrow><annotation encoding="application/x-tex">f(x)</annotation><annotation-xml encoding="MathML-Content"><apply><sin/><ci>z</ci></apply></annotation-xml></semantics></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");let root=crate::math_document::parse_xml(&xml).unwrap();let functions=native_function_nodes(&root);assert_eq!(functions.len(),1);assert_eq!(child(functions[0],"fName").unwrap().text(),"f+g");assert_eq!(child(functions[0],"e").unwrap().text(),"x");assert!(!root.text().contains('z'));
        let(xml,diagnostics)=from_mathml("<math><mrow><mi>sin</mi><mi>x</mi></mrow></math>").unwrap();assert!(diagnostics.is_empty());assert!(native_function_nodes(&crate::math_document::parse_xml(&xml).unwrap()).is_empty());
    }
    #[test]fn generated_function_delimiters_keep_explicit_grow_and_shape_flags(){
        for (grow,symmetric) in [("true","true"),("false","false"),("true","false"),("false","true")]{
            let source=format!(r#"<math><mrow><mi>f</mi><mo>&#x2061;</mo><mrow><mo fence="true" stretchy="{grow}" symmetric="{symmetric}">[</mo><mrow><mi>x</mi></mrow><mo fence="true" stretchy="{grow}" symmetric="{symmetric}">]</mo></mrow></mrow></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");let root=crate::math_document::parse_xml(&xml).unwrap();let delimiter=native_find(&root,"d").unwrap();
            assert_eq!(prop(delimiter,"dPr","begChr"),Some("["));assert_eq!(prop(delimiter,"dPr","endChr"),Some("]"));assert_eq!(prop(delimiter,"dPr","grow"),Some(if grow=="true"{"1"}else{"0"}));assert_eq!(prop(delimiter,"dPr","shp"),Some(if symmetric=="true"{"centered"}else{"match"}));
            assert_eq!(child(native_function_nodes(&root)[0],"e").unwrap().text(),"x");
        }
        for end in [r#"<mo fence="true" stretchy="false" symmetric="true">)</mo>"#,r#"<mo fence="true" stretchy="true" symmetric="true" lspace="1em">)</mo>"#]{
            let source=format!(r#"<math><mrow><mi>f</mi><mo>&#x2061;</mo><mrow><mo fence="true" stretchy="true" symmetric="true">(</mo><mrow><mi>x</mi></mrow>{end}</mrow></mrow></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.contains(&"mathml_delimiter_projection".into()));let root=crate::math_document::parse_xml(&xml).unwrap();assert_eq!(native_count(&root,"d"),0);assert!(root.text().contains("(x)"));
        }
    }
    #[test]fn malformed_and_ambiguous_mathml_function_application_is_not_silently_flattened(){
        for body in ["<mi>f</mi><mo>⁡</mo>","<mo>⁡</mo><mi>x</mi>","<mi>f</mi><mo>⁡</mo><mrow/>","<mrow/><mo>⁡</mo><mi>x</mi>","<mi>f</mi><mo>⁡</mo><mo>⁡</mo><mi>x</mi>",r#"<mi>f</mi><mo>⁡</mo><mrow><mo fence="true" stretchy="true" symmetric="true">(</mo><mrow/><mo fence="true" stretchy="true" symmetric="true">)</mo></mrow>"#]{
            assert_eq!(from_mathml(&format!("<math><mrow>{body}</mrow></math>")).unwrap_err(),"math_missing_argument","{body}");
        }
        for source in ["<math><mrow><mi>f</mi><mo>⁡</mo><mi>x</mi><mi>y</mi></mrow></math>","<math><mrow><mi>f</mi><mo>⁡</mo><mi>g</mi><mo>⁡</mo><mi>x</mi></mrow></math>",r#"<math><mrow><mi>f</mi><mo lspace="1em">⁡</mo><mi>x</mi></mrow></math>"#,"<math><mi>f</mi><mo>⁡</mo><mi>x</mi></math>",r#"<math><mrow data-other="unknown"><mi>f</mi><mo>⁡</mo><mi>x</mi></mrow></math>"#]{
            let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.contains(&"mathml_function_application_projection".into()),"{source}: {diagnostics:?}");let root=crate::math_document::parse_xml(&xml).unwrap();assert!(native_function_nodes(&root).is_empty());assert!(root.text().contains('⁡'));
        }
        assert_eq!(from_mathml(r#"<math xmlns="http://www.w3.org/1998/Math/MathML" xmlns:f="urn:foreign"><mrow><mi>f</mi><f:mo>⁡</f:mo><mi>x</mi></mrow></math>"#).unwrap_err(),"math_invalid_xml");
    }
    #[test]
    fn mathml_matrix_generated_minimum_and_empty_cells_reimport_as_semantics() {
        for hide in ["", "<m:plcHide/>"] {
            let xml=format!(r#"<m:oMath><m:m><m:mPr><m:cSp m:val="720"/>{hide}</m:mPr><m:mr><m:e><m:argPr><m:argSz m:val="-1"/></m:argPr></m:e><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#);
            let source=crate::omml::to_mathml(&crate::omml::parse(&xml).unwrap(),true,"");
            let (result,diagnostics)=super::from_mathml(&source).unwrap();
            assert!(diagnostics.is_empty(),"{source}: {diagnostics:?}");
            assert!(result.contains(r#"<m:cSp m:val="720"/>"#));
            let tree=crate::omml::parse(&result).unwrap();
            fn matrix(n:&crate::omml::Node)->Option<&crate::omml::Node>{if n.name=="m"{Some(n)}else{n.children.iter().find_map(matrix)}}
            let m=matrix(&tree).unwrap();assert_eq!(crate::omml::matrix_placeholders_visible(m),hide.is_empty());
            let empty=&m.child("mr").unwrap().children[0];assert!(crate::omml::empty_argument(empty));assert_eq!(crate::omml::argument_size(empty),-1);
            let again=crate::omml::to_mathml(&tree,true,"");
            assert!(again.contains("min-width:36pt"));assert_eq!(again.contains("outline:"),hide.is_empty());
        }
    }

    #[test]
    fn mathml_matrix_nonuniform_minima_and_unrelated_spaces_report_projection() {
        let uniform=r#"<math><mtable><mtr><mtd style=" min-width : 36pt; "><mi>x</mi></mtd><mtd style="min-width:36pt"><mi>y</mi></mtd></mtr></mtable></math>"#;
        let(result,diagnostics)=super::from_mathml(uniform).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");assert!(result.contains(r#"<m:cSp m:val="720"/>"#));
        let source=r#"<math><mtable><mtr><mtd style="min-width:12pt"><mi>x</mi></mtd><mtd style="min-width:24pt"><mi>y</mi></mtd></mtr></mtable></math>"#;
        let(result,diagnostics)=super::from_mathml(source).unwrap();assert!(!result.contains("<m:cSp"));assert!(diagnostics.iter().any(|v|v=="mathml_table_spacing_projection"));
        let styled=r#"<math><mtable><mtr><mtd><mspace width="0.45em" height="0.65em" depth="0.05em" style="outline:2px solid red"/></mtd></mtr></mtable></math>"#;
        assert_eq!(super::from_mathml(styled).unwrap_err(),"math_unsupported_extension");
        let(result,diagnostics)=super::from_mathml(r#"<math><mtable><mtr><mtd><mspace width="1em"/></mtd></mtr></mtable></math>"#).unwrap();
        assert!(diagnostics.is_empty());assert!(result.contains("readmdMathml:mspace"));assert!(!result.contains("<m:t> </m:t>"));
    }
    #[test]fn aligned_scripts_survive_real_mathml_projection_without_creating_spacing_text(){
        for display in [false,true]{for value in ["1","true","on"]{
            let source=format!(r#"<m:oMath><m:sSubSup><m:sSubSupPr><m:alnScr m:val="{value}"/></m:sSubSupPr><m:e><m:r><m:t>f</m:t></m:r></m:e><m:sub><m:r><m:t>2</m:t></m:r></m:sub><m:sup><m:r><m:t>3</m:t></m:r></m:sup></m:sSubSup></m:oMath>"#);
            let view=crate::omml::parse(&source).unwrap();let mathml=crate::omml::to_mathml(&view,display,"");assert!(mathml.contains("<mspace width=\"0em\"/>"));
            let (xml,diagnostics)=from_mathml(&mathml).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");assert!(xml.contains("<m:alnScr m:val=\"1\"/>"));assert!(crate::math_migration::readable(&xml).is_ok());
            let new=crate::math_layout::layout_source("",Some(&xml),display,12.).unwrap();let old=crate::math_layout::layout_source("",Some(&source),display,12.).unwrap();assert_eq!((new.width,new.height,new.depth),(old.width,old.height,old.depth));
        }}
        for space in [r#"<mspace width="1em"/>"#,r#"<mspace width="0em" height="2em"/>"#]{
            let source=format!("<math><msubsup><mrow><mrow><mi>f</mi></mrow>{space}</mrow><mn>2</mn><mn>3</mn></msubsup></math>");let(xml,diagnostics)=from_mathml(&source).unwrap();assert!(!xml.contains("alnScr"));assert!(diagnostics.is_empty());assert!(xml.contains("readmdMathml:mspace"));
        }
    }
    #[test]fn mathml_scoped_styles_keep_token_defaults_and_nested_overrides(){
        let source=r##"<math><mstyle mathvariant="bold" mathcolor="#369" mathsize="24px"><mfrac><mi>x</mi><mi mathvariant="normal" mathcolor="red" mathsize="12pt">den</mi></mfrac><mstyle mathvariant="italic"><mi>y</mi></mstyle><mi>z</mi></mstyle><mi>ab</mi><mi>c</mi><mn>12</mn><mo>+</mo></math>"##;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
        let parsed=crate::omml::parse(&xml).unwrap();let mut runs=vec![];
        fn visit<'a>(n:&'a crate::omml::Node,runs:&mut Vec<&'a crate::omml::Node>){if n.is_math()&&n.name=="r"{runs.push(n);}for c in &n.children{visit(c,runs);}}
        visit(&parsed,&mut runs);assert_eq!(runs.len(),8);
        let styles=runs.iter().map(|n|crate::omml::run_style(n)).collect::<Vec<_>>();
        assert!(styles[0].bold);assert_eq!(styles[0].half_points,Some(36));assert_eq!(styles[0].color,Some([0x33,0x66,0x99]));
        assert!(styles[1].upright&&!styles[1].bold);assert_eq!(styles[1].half_points,Some(24));assert_eq!(styles[1].color,Some([255,0,0]));
        assert!(!styles[2].upright&&!styles[2].bold);assert!(styles[3].bold);
        assert!(styles[4].upright);assert!(!styles[5].upright);assert!(styles[6].upright&&styles[7].upright);
        for style in &styles[4..]{assert_eq!(style.color,None);assert_eq!(style.half_points,None);}
        fn fraction(n:&crate::omml::Node)->Option<&crate::omml::Node>{if n.name=="f"{Some(n)}else{n.children.iter().find_map(fraction)}}
        let control=crate::omml::control_style(fraction(&parsed).unwrap());
        assert_eq!(control.color,Some([0x33,0x66,0x99]));assert_eq!(control.half_points,Some(36));
        let dsl=crate::math_dsl::decompile(&crate::math_document::parse_xml(&xml).unwrap()).unwrap();let(rebuilt,_)=crate::math_dsl::compile(&dsl,false).unwrap_or_else(|e|panic!("{e}: {dsl}"));assert_eq!(crate::math_document::xml_semantics(&rebuilt),crate::math_document::xml_semantics(&crate::math_document::parse_xml(&xml).unwrap()));
    }
    #[test]fn mathml_inherited_defaults_and_text_styles_have_native_office_semantics(){
        let(xml,diagnostics)=from_mathml(r#"<math><mstyle linethickness="0" accent="false" open="[" close="]" columnalign="left right"><mfrac><mi>a</mi><mi>b</mi></mfrac><mover><mi>x</mi><mo>^</mo></mover><mfenced><mi>q</mi></mfenced><mtable><mtr><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd></mtr></mtable><mtext mathvariant="bold-italic" mathcolor="navy">Evidence</mtext></mstyle></math>"#).unwrap();
        assert!(diagnostics.is_empty(),"{diagnostics:?}");assert!(xml.contains("<m:type m:val=\"noBar\""));assert!(xml.contains("<m:limUpp>"));assert!(xml.contains("<m:begChr m:val=\"[\""));assert!(xml.contains("<m:mcJc m:val=\"right\""));
        assert!(crate::math_migration::readable(&xml).is_ok(),"Every supported inherited default must actually compile back to readable source: {xml}");
        let tree=crate::omml::parse(&xml).unwrap();fn text(n:&crate::omml::Node)->Option<&crate::omml::Node>{if n.name=="r"&&n.run_text()=="Evidence"{return Some(n);}n.children.iter().find_map(text)}
        let style=crate::omml::run_style(text(&tree).unwrap());assert!(style.normal_text&&style.bold&&style.word_italic==Some(true));assert_eq!(style.color,Some([0,0,128]));
    }
    #[test]fn mathml_physical_sizes_are_exact_bounded_and_diagnose_unrepresented_values(){
        for(value,expected)in[("18pt",36),("24px",36),("0.25in",36),("6.35mm",36),("0.635cm",36),("1638pt",3276)]{assert_eq!(mathml_half_points(value),Some(expected));}
        for value in ["18.1pt","20.1px","0pt","1639pt","NaNpt","Infinitypt","1em","120%","18pt; color:red"]{assert_eq!(mathml_half_points(value),None);}
        for source in [r#"<math><mi mathsize="18.1pt">x</mi></math>"#,r#"<math><mstyle mathcolor="url(private)" mathbackground="red"><mi>x</mi></mstyle></math>"#]{let(xml,diagnostics)=from_mathml(source).unwrap();assert!(xml.contains("<m:t>x</m:t>"));assert!(diagnostics.contains(&"mathml_style_projection".into()));assert!(!xml.contains("private"));}
    }
    #[test]fn inherited_mathml_defaults_are_bounded_before_branch_amplification(){
        for key in ["mathvariant","mathsize","mathcolor","separators","columnspacing"]{
            let source=format!("<math><mstyle {key}=\"{}\">{}</mstyle></math>","a".repeat(8192),"<mi>x</mi>".repeat(2000));
            assert_eq!(from_mathml(&source).unwrap_err(),"math_invalid_property");
        }
    }
    #[test]fn group_and_accent_plain_text_projections_keep_defaults_and_explicit_empty(){
        let run="<m:e><m:r><m:t>x</m:t></m:r></m:e>";
        let get=|body:String|tex(&crate::math_document::parse_xml(&format!("<m:oMath>{body}</m:oMath>")).unwrap());
        assert_eq!(get(format!("<m:groupChr>{run}</m:groupChr>")),"\\underbrace{x}");
        for kind in ["groupChr","acc"] {
            assert_eq!(get(format!("<m:{kind}><m:{kind}Pr><m:chr/></m:{kind}Pr>{run}</m:{kind}>")),"x");
        }
        assert_eq!(get(format!("<m:groupChr><m:groupChrPr><m:vertJc/></m:groupChrPr>{run}</m:groupChr>")),"\\overset{x}{⏟}");
        let foreign=format!(r#"<m:groupChr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><m:groupChrPr><m:vertJc w:val="top"/></m:groupChrPr>{run}</m:groupChr>"#);
        assert_eq!(get(foreign),"\\overset{x}{⏟}");
    }
    #[test]fn mathml_attachments_keep_accent_bar_group_and_limit_semantics_through_dsl(){
        for (attachment,kind,property) in [
            (r#"<mover><mi>x</mi><mo>^</mo></mover>"#,"acc","accPr"),
            (r#"<mover><mi>x</mi><mo>⃗</mo></mover>"#,"acc","accPr"),
            (r#"<mover accent="true"><mi>x</mi><mo>¯</mo></mover>"#,"bar","barPr"),
            (r#"<munder accentunder="true"><mi>x</mi><mo>_</mo></munder>"#,"bar","barPr"),
            (r#"<munder><mi>x</mi><mrow><mo>⏟</mo></mrow></munder>"#,"groupChr","groupChrPr"),
            (r#"<mover accent="true"><mi>x</mi><mo>⏞</mo></mover>"#,"groupChr","groupChrPr"),
            (r#"<mover accent="false"><mi>x</mi><mo>^</mo></mover>"#,"limUpp","lim"),
            (r#"<munder accentunder="false"><mi>x</mi><mo>_</mo></munder>"#,"limLow","lim"),
        ] {
            let source=format!("<math>{attachment}</math>");
            let (xml,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
            let parsed=crate::math_document::parse_xml(&xml).unwrap();
            let dsl=crate::math_dsl::decompile(&parsed).unwrap();
            let (rebuilt,_)=crate::math_dsl::compile(&dsl,false).unwrap();
            assert!(rebuilt.xml().contains(&format!("<m:{kind}")),"{dsl}");
            assert!(rebuilt.xml().contains(&format!("<m:{property}")),"{dsl}");
            let mml=crate::omml::to_mathml(&crate::omml::parse(&rebuilt.xml()).unwrap(),false,"");
            let (again,diagnostics)=from_mathml(&mml).unwrap();assert!(diagnostics.is_empty(),"{mml}: {diagnostics:?}");
            assert!(again.contains(&format!("<m:{kind}")),"{mml} -> {again}");
        }
    }
    #[test]fn combined_mathml_accents_respect_both_explicit_switches_and_report_projection(){
        let (xml,diagnostics)=from_mathml(r#"<math><munderover accentunder="true" accent="false"><mi>x</mi><mo>_</mo><mo>^</mo></munderover></math>"#).unwrap();
        assert!(diagnostics.is_empty());assert!(xml.contains("<m:limUpp>")&&xml.contains("<m:bar>"));
        for mark in [r#"<mo stretchy="false">^</mo>"#,r#"<mrow><mi>a</mi><mi>b</mi></mrow>"#] {
            let (xml,diagnostics)=from_mathml(&format!(r#"<math><mover accent="true"><mi>x</mi>{mark}</mover></math>"#)).unwrap();
            assert!(xml.contains("<m:limUpp>"));assert!(diagnostics.contains(&"mathml_style_projection".into()));
        }
        let (xml,diagnostics)=from_mathml(r#"<math><mover><mi>x</mi><mo accent="false">^</mo></mover></math>"#).unwrap();
        assert!(diagnostics.is_empty());assert!(xml.contains("<m:limUpp>"));
    }
    #[test]fn math_paragraph_readable_projection_preserves_rows_and_compiles_as_equations(){
        for alignment in ["left","right","center","centerGroup"]{
            let xml=format!(r#"<m:oMathPara xmlns:m="{MATH_NS}"><m:oMathParaPr><m:jc m:val="{alignment}"/></m:oMathParaPr><m:oMath><m:r><m:t>a</m:t></m:r></m:oMath><m:oMath><m:r><m:t>b</m:t></m:r></m:oMath></m:oMathPara>"#);
            let projection=tex(&crate::math_document::parse_xml(&xml).unwrap());
            assert!(projection.contains("\\\\"));
            let compiled=crate::latex2omml::latex_to_omml(&projection,true);
            let tree=crate::omml::parse(&compiled).unwrap();
            let output=crate::omml::to_mathml(&tree,true,"");
            assert_eq!(output.matches("<mtr>").count(),2,"{alignment}: {output}");
            assert!(output.contains(">a<")&&output.contains(">b<"));
        }
        let xml=format!(r#"<m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>a</m:t></m:r><m:r><m:rPr><m:brk m:alnAt="1"/></m:rPr><m:t>=b</m:t></m:r></m:oMath>"#);
        let projection=tex(&crate::math_document::parse_xml(&xml).unwrap());
        assert!(projection.contains("a \\\\ &"));
        let output=crate::latex2omml::latex_to_omml(&projection,true);
        assert!(output.contains("<m:eqArr>"));
    }
    use super::*;
    #[test]
    fn tex_projection_never_resurrects_empty_fences_separators_or_operators() {
        let original=crate::math_document::parse_xml("<m:oMath><m:d><m:dPr><m:begChr/><m:sepChr/><m:endChr/><m:grow m:val=\"0\"/></m:dPr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:d></m:oMath>").unwrap();
        assert_eq!(tex(&original),"xy");
        let xml="<m:oMath><m:nary><m:naryPr><m:chr/></m:naryPr><m:sub/><m:sup/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>";
        assert_eq!(tex(&crate::math_document::parse_xml(xml).unwrap()),"{x}");
        let doc=crate::math_document::parse("delimiter(x, y, grow: false, open: \"{\", close: \"}\", separator: \"\")","dsl",false).unwrap();
        assert_eq!(tex(&doc.root),"\\{xy\\}");
    }
    #[test]fn uniform_mathml_column_gaps_round_trip_without_losing_units_or_precision(){
        for (input,expected) in [("1em","1em"),("1.5em 1.5em","1.5em"),("2em","2em"),("3em","3em"),("1.25pt","1.25pt"),("0","0em")] {
            let source=format!(r#"<math><mtable columnspacing="{input}"><mtr><mtd><mi>x</mi></mtd><mtd><mi>y</mi></mtd></mtr></mtable></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.is_empty(),"{input}: {diagnostics:?}");
            let output=crate::omml::to_mathml(&crate::omml::parse(&xml).unwrap(),false,"");
            assert!(output.contains(&format!("columnspacing=\"{expected}\"")),"{output}");
            let(_,diagnostics)=from_mathml(&output).unwrap();assert!(diagnostics.is_empty());
        }
        for value in ["1em 2em","0.25em","1ex","NaNem","-1pt","4000pt","0.001pt"] {
            let source=format!(r#"<math><mtable columnspacing="{value}"><mtr><mtd><mi>x</mi></mtd></mtr></mtable></math>"#);
            let(_,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.contains(&"mathml_table_spacing_projection".into()),"{value}");
        }
        let(_,diagnostics)=from_mathml(r#"<math><mtable columnwidth="2em"><mtr><mtd><mi>x</mi></mtd></mtr></mtable></math>"#).unwrap();
        assert!(diagnostics.contains(&"mathml_table_spacing_projection".into()));
    }
    #[test]fn relative_mathml_argument_sizes_round_trip_and_foreign_symbols_never_replace_math(){
        for level in [-2,-1,1,2]{
            let source=format!(r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><msup><mi>x</mi><mstyle scriptlevel="{level:+}"><mi>y</mi></mstyle></msup></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.is_empty());
            assert!(xml.contains(&format!("m:val=\"{}\"",-level)));
            let output=crate::omml::to_mathml(&crate::omml::parse(&xml).unwrap(),false,"");
            assert!(output.contains(&format!("scriptlevel=\"{level:+}\"")));
            let(reopened,diagnostics)=from_mathml(&output).unwrap();assert!(diagnostics.is_empty());
            assert!(reopened.contains(&format!("m:val=\"{}\"",-level)));
        }
        let(_,diagnostics)=from_mathml("<math><mstyle scriptlevel=\"2\" mathsize=\"20.1px\"><mi>x</mi></mstyle></math>").unwrap();
        assert!(diagnostics.contains(&"mathml_scriptlevel_projection".into())&&diagnostics.contains(&"mathml_style_projection".into()));
        assert!(from_mathml("<math xmlns=\"urn:impostor\"><mi>x</mi></math>").is_err());
        assert!(from_mathml("<math><mi xmlns=\"urn:impostor\">x</mi></math>").is_err());
        let root=crate::math_document::parse_xml(&format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:x="urn:extension"><m:r><x:t>wrong</x:t><m:t>x</m:t></m:r></m:oMath>"#)).unwrap();
        assert_eq!(tex(&root),"x");
    }
    #[test]fn mathml_import_preserves_fraction_scripts_and_fenced_slots(){
        let (xml,diagnostics)=from_mathml("<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mfrac><msup><mi>α</mi><mn>2</mn></msup><mfenced><mi>x</mi><mi>y</mi></mfenced></mfrac></math>").unwrap();
        assert!(diagnostics.is_empty());assert!(xml.contains("<m:sSup>"));assert!(xml.contains("m:val=\",\""));
        let root=crate::math_document::parse_xml(&xml).unwrap();assert!(tex(&root).contains("\\frac"));
    }
    #[test]fn unicode_compact_fraction_radical_and_matrix(){
        assert_eq!(from_unicode("(a+b)/(c+d)").unwrap(),"\\frac{a+b}{c+d}");
        assert_eq!(from_unicode("√(x^2+1)").unwrap(),"\\sqrt{{x}^{2}+1}");
        assert!(from_unicode("■(a&b@c&d)").unwrap().contains("\\begin{matrix}a & b \\\\ c & d"));
        assert!(from_unicode("a/(b").is_err());
    }
    #[test]fn unknown_mathml_is_diagnosed_without_removing_its_content(){
        let (xml,d)=from_mathml("<math><maction><mi>x</mi></maction></math>").unwrap();assert!(xml.contains(">x<"));assert_eq!(d,["mathml_unsupported_maction"]);
    }
    #[test]fn mathml_prescripts_postscripts_and_empty_slots_round_trip(){
        let source="<math><mmultiscripts><mi>T</mi><mi>i</mi><none/><mprescripts/><none/><mn>2</mn></mmultiscripts></math>";
        let(xml,d)=from_mathml(source).unwrap();assert!(d.is_empty());
        let tree=crate::omml::parse(&xml).unwrap();
        let pre=tree.child("oMath").unwrap().child("sPre").unwrap();
        assert_eq!(pre.children.iter().map(|n|n.name.as_str()).collect::<Vec<_>>(),["sub","sup","e"]);
        assert!(pre.child("sub").unwrap().children.is_empty());
        assert_eq!(pre.child("sup").unwrap().child("r").unwrap().run_text(),"2");
        let post=pre.child("e").unwrap().child("sSub").unwrap();
        assert_eq!(post.child("e").unwrap().child("r").unwrap().run_text(),"T");
        assert_eq!(post.child("sub").unwrap().child("r").unwrap().run_text(),"i");
        let out=crate::omml::to_mathml(&tree,false,"");
        let(xml2,d)=from_mathml(&out).unwrap();assert!(d.is_empty());
        assert_eq!(tex(&crate::math_document::parse_xml(&xml2).unwrap()),tex(&crate::math_document::parse_xml(&xml).unwrap()));
        let vector=crate::math_layout::layout_source("",Some(&xml),false,12.0).unwrap();assert!(vector.width>0.0);
    }
    #[test]fn mathml_invalid_argument_counts_never_drop_extra_scripts(){
        for fragment in ["<mfrac><mi>x</mi></mfrac>","<msup><mi>x</mi><mn>2</mn><mn>3</mn></msup>","<mmultiscripts/>","<mmultiscripts><none/><mi>i</mi><mi>j</mi></mmultiscripts>","<mmultiscripts><mi>T</mi><mi>i</mi></mmultiscripts>","<mmultiscripts><mi>T</mi><mprescripts/><mi>i</mi></mmultiscripts>","<mmultiscripts><mi>T</mi><mprescripts/><mprescripts/></mmultiscripts>","<mrow><none/></mrow>"]{
            assert!(from_mathml(&format!("<math>{fragment}</math>")).is_err(),"{fragment}");
        }
        let(xml,d)=from_mathml("<math><mmultiscripts><mi>T</mi><mi>i</mi><mi>j</mi><mi>k</mi><mi>l</mi></mmultiscripts></math>").unwrap();
        assert_eq!(d,["mathml_multiscript_projection"]);
        for token in ["T","i","j","k","l"]{assert!(xml.contains(&format!(">{token}<")));}
    }
    #[test]fn representable_row_gaps_survive_mathml_roundtrip_without_losing_column_units(){
        for (input,expected) in [("1em","1em"),("1.5em","1.5em"),("2em","2em"),("3.5em","3.5em"),("12pt","12pt"),("0","0em"),("12pt 12pt","12pt")] {
            let source=format!(r#"<math><mtable rowspacing="{input}" columnspacing="1.25pt"><mtr><mtd><mi>x</mi></mtd></mtr><mtr><mtd><mi>y</mi></mtd></mtr></mtable></math>"#);
            let (xml,diagnostics)=from_mathml(&source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
            let output=crate::omml::to_mathml(&crate::omml::parse(&xml).unwrap(),true,"");
            assert!(output.contains(&format!("rowspacing=\"{expected}\"")),"{output}");
            assert!(output.contains("columnspacing=\"1.25pt\""));
        }
        for input in ["1.25pt","1.25em","-1pt","1em 2em","calc(1em)","NaNpt","65536pt"] {
            let (_,diagnostics)=from_mathml(&format!(r#"<math><mtable rowspacing="{input}"><mtr><mtd><mi>x</mi></mtd></mtr></mtable></math>"#)).unwrap();
            assert!(diagnostics.contains(&"mathml_table_spacing_projection".into()),"{input}");
        }
    }
    #[test]fn matrix_column_groups_and_baselines_survive_mathml_import_and_export(){
        let(xml,d)=from_mathml("<math><mtable columnalign=\"left right\" align=\"baseline -1\"><mtr><mtd><mi>x</mi></mtd><mtd><mn>12</mn></mtd><mtd><mn>3</mn></mtd></mtr></mtable></math>").unwrap();
        assert!(d.is_empty());
        let root=crate::omml::parse(&xml).unwrap();let matrix=root.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,3),["left","right","right"]);
        assert_eq!(matrix.prop("mPr","baseJc"),Some("bot"));
        let exported=crate::omml::to_mathml(&root,false,"");
        assert!(exported.contains("columnalign=\"left right right\""));assert!(exported.contains("align=\"baseline -1\""));
        let repeated=crate::omml::parse("<m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:count m:val=\"2\"/><m:mcJc m:val=\"right\"/></m:mcPr></m:mc></m:mcs></m:mPr></m:m>").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(repeated.child("m").unwrap(),3),["right","right","center"]);
    }
    #[test]fn mathml_table_row_and_cell_alignment_overrides_have_editable_column_semantics(){
        let source=r#"<math><mtable columnalign="center left" columnspacing="1.25pt" rowspacing="12pt" align="baseline -1"><mtr columnalign="left right"><mtd columnalign="center"><mi>a</mi></mtd><mtd><mi>wide</mi></mtd><mtd><mi>c</mi></mtd></mtr><mtr columnalign="center left"><mtd><mi>long</mi></mtd><mtd columnalign="right"><mi>e</mi></mtd><mtd columnalign="right"><mi>f</mi></mtd></mtr></mtable></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
        let tree=crate::omml::parse(&xml).unwrap();let matrix=tree.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,3),["center","right","right"]);
        assert_eq!(matrix.prop("mPr","baseJc"),Some("bot"));assert_eq!(matrix.prop("mPr","cGp"),Some("25"));assert_eq!(matrix.prop("mPr","rSp"),Some("12"));
        let source_tree=crate::math_document::parse_xml(&xml).unwrap();let dsl=crate::math_dsl::decompile(&source_tree).unwrap();
        let(rebuilt,_)=crate::math_dsl::compile(&dsl,false).unwrap();assert_eq!(crate::math_document::xml_semantics(&source_tree),crate::math_document::xml_semantics(&rebuilt));
        let exported=crate::omml::to_mathml(&tree,true,"");assert!(exported.contains("columnalign=\"center right right\""));
        let(again,diagnostics)=from_mathml(&exported).unwrap();assert!(diagnostics.is_empty(),"{exported}: {diagnostics:?}");
        let again=crate::omml::parse(&again).unwrap();let matrix=again.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,3),["center","right","right"]);assert_eq!(matrix.prop("mPr","cGp"),Some("25"));assert_eq!(matrix.prop("mPr","rSp"),Some("12"));
        let document=crate::math_document::parse(&dsl,"dsl",true).unwrap();let id=document.nodes.iter().find(|n|n.kind=="m").unwrap().id.clone();
        let changed=crate::math_document::patch(&dsl,"dsl",true,&document.revision,&crate::math_document::MathPatch::MatrixColumnAlignment{node_id:id,index:1,alignment:"left".into()}).unwrap();
        let changed=crate::omml::parse(&changed.root.xml()).unwrap();
        let display=changed.child("oMathPara").expect("display-mode patch must retain its equation paragraph");
        let matrix=display.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,3),["center","left","right"]);assert_eq!(matrix.prop("mPr","cGp"),Some("25"));assert_eq!(matrix.prop("mPr","rSp"),Some("12"));
    }
    #[test]fn mathml_table_alignment_cascade_preserves_empty_and_ragged_rows(){
        let source=r#"<math><mtable><mtr columnalign="left right"><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd><mtd><mi>c</mi></mtd></mtr><mtr columnalign="center"/><mtr columnalign="left"><mtd><mi>d</mi></mtd></mtr></mtable></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
        let tree=crate::omml::parse(&xml).unwrap();let matrix=tree.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,3),["left","right","right"]);
        assert_eq!(matrix.children.iter().filter(|n|n.name=="mr").map(|n|n.children.len()).collect::<Vec<_>>(),[3,0,1]);
    }
    #[test]fn mathml_table_alignment_overrides_combine_with_uniform_cell_minima(){
        let source=r#"<math><mtable columnalign="left" columnspacing="1.5em" rowspacing="12pt"><mtr columnalign="right center"><mtd style="box-sizing:content-box;min-width:36pt;padding-left:0pt;padding-right:0.75em;text-align:right"><mi>a</mi></mtd><mtd style="box-sizing:content-box;min-width:36pt;padding-left:0.75em;padding-right:0pt;text-align:center"><mi>b</mi></mtd></mtr><mtr columnalign="left center"><mtd columnalign="right" style="box-sizing:content-box;min-width:36pt;padding-left:0pt;padding-right:0.75em;text-align:right"><mi>c</mi></mtd><mtd style="box-sizing:content-box;min-width:36pt;padding-left:0.75em;padding-right:0pt;text-align:center"><mi>d</mi></mtd></mtr></mtable></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
        let tree=crate::omml::parse(&xml).unwrap();let matrix=tree.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,2),["right","center"]);assert_eq!(matrix.prop("mPr","cSp"),Some("720"));assert_eq!(matrix.prop("mPr","cGpRule"),Some("1"));assert_eq!(matrix.prop("mPr","rSp"),Some("12"));
        let exported=crate::omml::to_mathml(&tree,true,"");let(again,diagnostics)=from_mathml(&exported).unwrap();assert!(diagnostics.is_empty(),"{exported}: {diagnostics:?}");
        let tree=crate::omml::parse(&again).unwrap();let matrix=tree.child("oMath").unwrap().child("m").unwrap();assert_eq!(crate::omml::matrix_column_alignments(matrix,2),["right","center"]);assert_eq!(matrix.prop("mPr","cSp"),Some("720"));
    }
    #[test]fn mathml_table_alignment_overrides_do_not_escape_into_nested_tables(){
        let source=r#"<math><mstyle columnalign="left"><mtable columnalign="center"><mtr columnalign="center"><mtd columnalign="right"><mtable><mtr><mtd><mi>a</mi></mtd></mtr></mtable></mtd></mtr></mtable></mstyle><mtable><mtr><mtd><mi>b</mi></mtd></mtr></mtable></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");let tree=crate::omml::parse(&xml).unwrap();
        fn matrices(n:&crate::omml::Node,alignments:&mut Vec<Vec<String>>){if n.name=="m"{alignments.push(crate::omml::matrix_column_alignments(n,1).into_iter().map(str::to_string).collect());}for child in &n.children{matrices(child,alignments);}}
        let mut alignments=vec![];matrices(&tree,&mut alignments);assert_eq!(alignments,vec![vec!["right".to_string()],vec!["left".to_string()],vec!["center".to_string()]]);
    }
    #[test]fn mathml_table_conflicting_column_alignments_keep_explicit_projection(){
        let source=r#"<math><mtable columnalign="center left"><mtr columnalign="left right"><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd></mtr><mtr columnalign="right"><mtd><mi>c</mi></mtd><mtd><mi>d</mi></mtd></mtr></mtable></math>"#;
        let(xml,diagnostics)=from_mathml(source).unwrap();assert_eq!(diagnostics,["mathml_table_alignment_projection"]);
        let tree=crate::omml::parse(&xml).unwrap();let matrix=tree.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(matrix,2),["center","right"]);
        for token in ["a","b","c","d"]{assert!(xml.contains(&format!(">{token}<")));}
    }
    #[test]fn mathml_table_invalid_alignment_overrides_are_not_silently_accepted(){
        for value in ["", "left right", "decimalpoint", "LEFT"]{
            let source=format!(r#"<math><mtable columnalign="left"><mtr columnalign="right"><mtd columnalign="{value}"><mi>x</mi></mtd></mtr></mtable></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert_eq!(diagnostics,["mathml_table_alignment_projection"],"{value}");
            let tree=crate::omml::parse(&xml).unwrap();assert_eq!(crate::omml::matrix_column_alignments(tree.child("oMath").unwrap().child("m").unwrap(),1),["right"]);
        }
        for value in ["", "left decimalpoint", "RIGHT"]{
            let source=format!(r#"<math><mtable columnalign="left"><mtr columnalign="{value}"><mtd><mi>x</mi></mtd></mtr></mtable></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert_eq!(diagnostics,["mathml_table_alignment_projection"],"{value}");
            let tree=crate::omml::parse(&xml).unwrap();assert_eq!(crate::omml::matrix_column_alignments(tree.child("oMath").unwrap().child("m").unwrap(),1),["left"]);
        }
    }
    #[test]fn mathml_table_spans_remain_outside_column_alignment_folding(){
        for attribute in [r#"columnspan="2""#,r#"rowspan="2""#,r#"columnspan="0""#]{
            let source=format!(r#"<math><mtable columnalign="center"><mtr columnalign="left"><mtd {attribute}><mi>x</mi></mtd></mtr></mtable></math>"#);
            let(xml,diagnostics)=from_mathml(&source).unwrap();assert_eq!(diagnostics,["mathml_table_alignment_projection"],"{attribute}");
            let tree=crate::omml::parse(&xml).unwrap();assert_eq!(crate::omml::matrix_column_alignments(tree.child("oMath").unwrap().child("m").unwrap(),1),["center"]);
            assert!(xml.contains(">x<"));
        }
        let(_,diagnostics)=from_mathml(r#"<math><mtable><mtr columnalign="left"><mtd rowspan="1" columnspan="1"><mi>x</mi></mtd></mtr></mtable></math>"#).unwrap();assert!(diagnostics.is_empty(),"{diagnostics:?}");
    }
    #[test]fn all_representable_mathml_variants_share_one_valid_run_property_container(){
        for variant in ["normal","bold","italic","bold-italic","script","bold-script","fraktur","bold-fraktur","double-struck","sans-serif","bold-sans-serif","sans-serif-italic","sans-serif-bold-italic","monospace"]{
            let(xml,d)=from_mathml(&format!("<math><mi mathvariant=\"{variant}\">x</mi></math>")).unwrap();assert!(d.is_empty(),"{variant}: {d:?}");
            let root=crate::omml::parse(&xml).unwrap();let run=root.child("oMath").unwrap().child("r").unwrap();
            assert_eq!(run.children.iter().filter(|n|n.name=="rPr").count(),1);
            let output=crate::omml::to_mathml(&root,false,"");assert!(output.contains(&format!("mathvariant=\"{variant}\"")),"{variant}: {output}");
            let properties=run.child("rPr").unwrap();let names:Vec<_>=properties.children.iter().map(|n|n.name.as_str()).collect();
            assert!(names==["sty"]||names==["scr","sty"],"{names:?}");
        }
        let(xml,d)=from_mathml("<math><mtext mathvariant=\"bold\">hello</mtext></math>").unwrap();
        assert!(d.is_empty());assert!(!xml.contains("<m:sty"));assert!(xml.contains("<m:nor")&&xml.contains("<w:b")&&xml.contains("<w:i"));
    }
}
