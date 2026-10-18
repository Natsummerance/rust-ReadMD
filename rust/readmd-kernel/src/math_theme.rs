//! Word mathematical theme colors: retained readable context, effective output
//! copies, no host settings, network resources or mutation of author properties.
use crate::math_document::{self,XmlNode,XmlPart,WORD_NS,DRAWING_NS};
use std::collections::BTreeSet;
type Result<T>=std::result::Result<T,&'static str>;
fn drawing(n:&XmlNode,name:&str)->bool{n.namespace==DRAWING_NS&&n.local()==name}
fn word(n:&XmlNode,name:&str)->bool{n.namespace==WORD_NS&&n.local()==name}
fn wattr<'a>(n:&'a XmlNode,name:&str)->Option<&'a str>{n.attributes.iter().find(|(k,_)|k.rsplit(':').next()==Some(name)&&n.attribute_namespaces.get(*k).is_some_and(|uri|uri==WORD_NS)).map(|(_,v)|v.as_str())}
fn single<'a>(nodes:impl Iterator<Item=&'a XmlNode>)->Result<Option<&'a XmlNode>>{let mut nodes=nodes;let first=nodes.next();if nodes.next().is_some(){Err("math_invalid_property")}else{Ok(first)}}
pub(crate) fn is_context(n:&XmlNode)->bool{drawing(n,"clrScheme")||word(n,"clrSchemeMapping")}
fn slot(name:&str)->Option<&str>{Some(match name{"dark1"=>"dk1","light1"=>"lt1","dark2"=>"dk2","light2"=>"lt2","hyperlink"=>"hlink","followedHyperlink"=>"folHlink","accent1"|"accent2"|"accent3"|"accent4"|"accent5"|"accent6"=>name,_=>return None})}
fn role(name:&str)->Option<(&str,&str)>{Some(match name{"background1"=>("bg1","light1"),"background2"=>("bg2","light2"),"text1"=>("t1","dark1"),"text2"=>("t2","dark2"),"accent1"|"accent2"|"accent3"|"accent4"|"accent5"|"accent6"|"hyperlink"|"followedHyperlink"=>(name,name),_=>return None})}
fn rgb(s:&str)->Result<[u8;3]>{if s.len()!=6||!s.bytes().all(|c|c.is_ascii_hexdigit()){return Err("math_invalid_property");}Ok([u8::from_str_radix(&s[..2],16).unwrap(),u8::from_str_radix(&s[2..4],16).unwrap(),u8::from_str_radix(&s[4..],16).unwrap()])}
fn fraction(s:Option<&str>)->Result<Option<f64>>{s.map(|s|if s.len()==2&&s.bytes().all(|c|c.is_ascii_hexdigit()){Ok(f64::from(u8::from_str_radix(s,16).unwrap())/255.)}else{Err("math_invalid_property")}).transpose()}
// MS-OI29500 17.3.2.6: use HSL luminance, and tint wins over shade.
fn adjust(color:[u8;3],tint:Option<f64>,shade:Option<f64>)->[u8;3]{
 if tint.is_none()&&shade.is_none(){return color;}
 let [r,g,b]=color.map(|c|f64::from(c)/255.);let hi=r.max(g).max(b);let lo=r.min(g).min(b);let l=(hi+lo)/2.;let delta=hi-lo;
 let saturation=if delta==0.{0.}else{delta/(1.-(2.*l-1.).abs())};
 let hue=if delta==0.{0.}else if hi==r{((g-b)/delta).rem_euclid(6.)}else if hi==g{(b-r)/delta+2.}else{(r-g)/delta+4.};
 let l=if let Some(t)=tint{l*t+1.-t}else{l*shade.unwrap()};let c=(1.-(2.*l-1.).abs())*saturation;let x=c*(1.-(hue.rem_euclid(2.)-1.).abs());let m=l-c/2.;
 let channels=match hue as u8{0=>[c,x,0.],1=>[x,c,0.],2=>[0.,c,x],3=>[0.,x,c],4=>[x,0.,c],_=>[c,0.,x]};channels.map(|v|((v+m).clamp(0.,1.)*255.).round() as u8)
}
#[derive(Default,Clone)]
pub(crate) struct ThemeColors{scheme:Option<XmlNode>,mapping:Option<XmlNode>}
impl ThemeColors{
 pub(crate) fn from_parts(theme:Option<&str>,settings:Option<&str>)->Result<Self>{
  let scheme=if let Some(source)=theme{let root=math_document::parse_xml(source)?;let theme=single(root.children())?.filter(|n|drawing(n,"theme")).ok_or("math_invalid_xml")?;let elements=single(theme.children().filter(|n|drawing(n,"themeElements")))?.ok_or("math_invalid_xml")?;Some(single(elements.children().filter(|n|drawing(n,"clrScheme")))?.ok_or("math_invalid_xml")?.clone())}else{None};
  let mapping=if let Some(source)=settings{let root=math_document::parse_xml(source)?;let settings=single(root.children())?.filter(|n|word(n,"settings")).ok_or("math_invalid_xml")?;single(settings.children().filter(|n|word(n,"clrSchemeMapping")))?.cloned()}else{None};
  Self::new(scheme,mapping)
 }
 pub(crate) fn from_context(root:&XmlNode)->Result<Self>{
  fn placement(n:&XmlNode,depth:usize)->Result<()>{if depth>1&&is_context(n){return Err("math_invalid_property");}for c in n.children(){placement(c,depth+1)?;}Ok(())}placement(root,0)?;
  Self::new(single(root.children().filter(|n|drawing(n,"clrScheme")))?.cloned(),single(root.children().filter(|n|word(n,"clrSchemeMapping")))?.cloned())
 }
 fn new(mut scheme:Option<XmlNode>,mut mapping:Option<XmlNode>)->Result<Self>{
  if let Some(s)=&mut scheme{let mut seen=BTreeSet::new();for n in s.children().filter(|n|n.namespace==DRAWING_NS){if !seen.insert(n.local().to_string()){return Err("math_invalid_property");}}fn clean(n:&mut XmlNode){n.content.retain(|p|!matches!(p,XmlPart::Text(s) if s.trim().is_empty()));for p in &mut n.content{if let XmlPart::Element(c)=p{clean(c);}}}clean(s);crate::math_word::bind_used_namespaces(s);}
  if let Some(m)=&mut mapping{for(k,v)in &m.attributes{if m.attribute_namespaces.get(k).is_some_and(|uri|uri==WORD_NS)&&slot(v).is_none(){return Err("math_invalid_property");}}crate::math_word::bind_used_namespaces(m);}
  Ok(Self{scheme,mapping})
 }
 pub(crate) fn is_empty(&self)->bool{self.scheme.is_none()&&self.mapping.is_none()}
 fn mapped<'a>(&'a self,name:&'a str)->Result<&'a str>{
  if let Some((key,default))=role(name){return slot(self.mapping.as_ref().and_then(|m|wattr(m,key)).unwrap_or(default)).ok_or("math_invalid_property");}
  slot(name).ok_or("math_invalid_property")
 }
 /// Capture only colors referenced by the formula and its retained style chain.
 pub(crate) fn capture(&self,nodes:&[&XmlNode])->Result<Vec<XmlNode>>{
  fn refs<'a>(n:&'a XmlNode,out:&mut BTreeSet<&'a str>){if word(n,"color"){if let Some(name)=wattr(n,"themeColor").filter(|s|*s!="none"){out.insert(name);}}for c in n.children(){refs(c,out);}}
  let mut names=BTreeSet::new();for n in nodes{refs(n,&mut names);}if names.is_empty(){return Ok(vec![]);}
  let wanted=names.iter().map(|n|self.mapped(n)).collect::<Result<BTreeSet<_>>>()?;let mut context=vec![];
  if let Some(s)=&self.scheme{let mut s=s.clone();s.content.retain(|p|matches!(p,XmlPart::Element(n) if n.namespace==DRAWING_NS&&wanted.contains(n.local())));context.push(s);}
  if let Some(m)=&self.mapping{context.push(m.clone());}Ok(context)
 }
 pub(crate) fn resolve_run_color(&self,n:&mut XmlNode)->Result<()>{
  let Some(name)=wattr(n,"themeColor").filter(|s|*s!="none")else{return Ok(());};let name=self.mapped(name)?;
  let tint=fraction(wattr(n,"themeTint"))?;let shade=fraction(wattr(n,"themeShade"))?;
  // Without a theme part, val is already the producer's resolved fallback.
  // Never apply a tint again to that fallback or consult an OS theme.
  let Some(scheme)=&self.scheme else{return Ok(());};
  let entry=single(scheme.children().filter(|c|drawing(c,name)))?.ok_or("math_invalid_property")?;
  let color=single(entry.children().filter(|c|c.namespace==DRAWING_NS))?.ok_or("math_invalid_property")?;
  if color.children().next().is_some(){return Err("math_unsupported_extension");}
  let value=if drawing(color,"srgbClr"){color.attributes.get("val")}else if drawing(color,"sysClr"){color.attributes.get("lastClr")}else{return Err("math_unsupported_extension");};
  let color=adjust(rgb(value.ok_or("math_invalid_property")?)?,tint,shade);
  let keys=n.attributes.keys().filter(|k|n.attribute_namespaces.get(*k).is_some_and(|uri|uri==WORD_NS)&&matches!(k.rsplit(':').next(),Some("val"|"themeColor"|"themeTint"|"themeShade"))).cloned().collect::<Vec<_>>();
  for k in keys{n.attributes.remove(&k);n.attribute_namespaces.remove(&k);}
  // Bind a fresh prefix rather than reusing an unrelated source w binding.
  let mut prefix="readmdTheme".to_string();while n.attributes.get(&format!("xmlns:{prefix}")).is_some_and(|uri|uri!=WORD_NS){prefix.push('_');}
  n.attributes.insert(format!("xmlns:{prefix}"),WORD_NS.into());let key=format!("{prefix}:val");n.attributes.insert(key.clone(),format!("{:02X}{:02X}{:02X}",color[0],color[1],color[2]));n.attribute_namespaces.insert(key,WORD_NS.into());Ok(())
 }
}

#[cfg(test)]mod tests{
 use super::*;
 fn theme()->String{format!(r#"<a:theme xmlns:a="{DRAWING_NS}"><a:themeElements><a:clrScheme name="Test"><a:accent1><a:srgbClr val="4F81BD"/></a:accent1><a:accent2><a:srgbClr val="C0504D"/></a:accent2><a:dk1><a:sysClr val="windowText" lastClr="123456"/></a:dk1></a:clrScheme></a:themeElements></a:theme>"#)}
 fn color(pr:&str)->XmlNode{math_document::parse_xml(&format!(r#"<q:color xmlns:q="{WORD_NS}" {pr}/>"#)).unwrap().children().next().unwrap().clone()}
 fn val(n:&XmlNode)->&str{wattr(n,"val").unwrap()}
 #[test]fn math_theme_hsl_matches_explicit_algorithm_official_tint_and_tint_priority(){
  let t=ThemeColors::from_parts(Some(&theme()),None).unwrap();
  // The illustrative shade RGB in MS-OI29500 disagrees with its explicit HSL
  // formula. Python colorsys independently gives 953735 for C0504D * BF/FF.
  // Keep exact formula behavior; actual Word pixel parity remains unverified.
  for(pr,expected)in [(r#"q:val="FF0000" q:themeColor="accent1" q:themeTint="99""#,"95B3D7"),(r#"q:themeColor="accent2" q:themeShade="BF""#,"953735"),(r#"q:themeColor="accent1" q:themeTint="99" q:themeShade="00""#,"95B3D7"),(r#"q:themeColor="text1""#,"123456")]{let mut n=color(pr);t.resolve_run_color(&mut n).unwrap();assert_eq!(val(&n),expected);assert!(wattr(&n,"themeColor").is_none());}
  assert_eq!(adjust([0,0,0],Some(0.),None),[255,255,255]);assert_eq!(adjust([255,255,255],None,Some(0.)),[0,0,0]);assert_eq!(adjust([128,128,128],Some(1.),None),[128,128,128]);
 }
 #[test]fn math_theme_mapping_is_one_lookup_and_capture_is_minimal(){
  let settings=format!(r#"<w:settings xmlns:w="{WORD_NS}"><w:clrSchemeMapping w:t1="accent2" w:accent1="accent2" w:accent2="accent1"/></w:settings>"#);let t=ThemeColors::from_parts(Some(&theme()),Some(&settings)).unwrap();let n=color(r#"q:themeColor="text1""#);let context=t.capture(&[&n]).unwrap();assert_eq!(context.len(),2);assert_eq!(context[0].children().map(|n|n.local()).collect::<Vec<_>>(),["accent2"]);
  let mut n=n.clone();t.resolve_run_color(&mut n).unwrap();assert_eq!(val(&n),"C0504D");let mut n=color(r#"q:themeColor="accent2""#);t.resolve_run_color(&mut n).unwrap();assert_eq!(val(&n),"4F81BD");assert!(t.capture(&[&color(r#"q:val="123456""#)]).unwrap().is_empty());
 }
 #[test]fn math_theme_invalid_or_ambiguous_data_cannot_silently_use_stale_rgb(){
  let t=ThemeColors::from_parts(Some(&theme()),None).unwrap();for pr in [r#"q:themeColor="nonsense""#,r#"q:themeColor="accent3" q:val="123456""#,r#"q:themeColor="accent1" q:themeTint="zz""#,r#"q:themeColor="accent1" q:themeShade="123""#]{assert!(t.resolve_run_color(&mut color(pr)).is_err());}
  let duplicate=theme().replace("</a:clrScheme>","<a:accent1><a:srgbClr val=\"000000\"/></a:accent1></a:clrScheme>");assert!(ThemeColors::from_parts(Some(&duplicate),None).is_err());
  let mut fallback=color(r#"q:val="95B3D7" q:themeColor="accent1" q:themeTint="99""#);ThemeColors::default().resolve_run_color(&mut fallback).unwrap();assert_eq!(val(&fallback),"95B3D7");assert_eq!(wattr(&fallback,"themeTint"),Some("99"));
 }
 #[test]fn math_theme_readable_round_trip_keeps_original_theme_and_materializes_only_output(){
  let scheme=ThemeColors::from_parts(Some(&theme()),None).unwrap().scheme.unwrap().xml();
  let xml=format!(r#"{scheme}<m:oMath xmlns:m="{}" xmlns:w="{WORD_NS}"><m:r><w:rPr><w:color w:val="FF0000" w:themeColor="accent1" w:themeTint="99"/></w:rPr><m:t>x</m:t></m:r><m:r><w:rPr><w:color w:themeColor="text1"/></w:rPr><m:t>y</m:t></m:r></m:oMath>"#,crate::math_document::MATH_NS);
  let original=math_document::parse_xml(&xml).unwrap();let readable=crate::math_dsl::decompile(&original).unwrap();assert!(readable.contains("drawing.clrScheme("));assert!(readable.contains("themeColor: \"accent1\""));assert!(readable.contains("val: \"FF0000\""));
  let(compiled,_)=crate::math_dsl::compile(&readable,false).unwrap();assert_eq!(math_document::xml_semantics(&original),math_document::xml_semantics(&compiled));
  let parsed=crate::omml::parse(&compiled.xml()).unwrap();let math=parsed.children.iter().find(|n|n.name=="oMath").unwrap();let runs=math.children.iter().filter(|n|n.name=="r").collect::<Vec<_>>();assert_eq!(crate::omml::run_style(runs[0]).color,Some([0x95,0xB3,0xD7]));assert_eq!(crate::omml::run_style(runs[1]).color,Some([0x12,0x34,0x56]));
  let(exported,_)=crate::omml::docx_math_fragment(&compiled.xml(),false).unwrap();assert!(!exported.contains("themeColor"));assert!(!exported.contains("clrScheme"));assert!(exported.contains("95B3D7"));assert!(compiled.xml().contains("FF0000"));assert!(crate::omml::to_mathml(&parsed,false,"").contains("#95B3D7"));
 }
 #[test]fn math_theme_context_must_not_be_nested_in_formula_slots_or_duplicated(){
  for source in ["frac(drawing.clrScheme(), x)","drawing.clrScheme()","row(drawing.clrScheme(), drawing.clrScheme(), office.oMath(x))","row(drawing.clrScheme(), word.clrSchemeMapping(), word.clrSchemeMapping(), office.oMath(x))"]{assert!(crate::math_dsl::compile(source,false).is_err(),"{source}");}
  let source="row(drawing.clrScheme(drawing.accent1(drawing.srgbClr(val: \"336699\")), name: \"Local\"), office.oMath(office.r(word.rPr(word.color(themeColor: \"accent1\")), office.t(\"x\"))))";let(compiled,_)=crate::math_dsl::compile(source,false).unwrap();assert!(crate::omml::to_mathml(&crate::omml::parse(&compiled.xml()).unwrap(),false,"").contains("#336699"));
 }
}
