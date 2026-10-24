//! Only uninterpreted XML lives in this carrier. Standard Office semantics
//! remain in the readable DSL. A carrier can be restored only onto its exact
//! formula identity, source digest and structural map.
use crate::{document_transaction as tx, math_document::{self, MathDocument, XmlNode, XmlPart, MATH_NS, WORD_NS, DRAWING_NS}};
use base64::{engine::general_purpose::STANDARD, Engine};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, io::{Read, Write},ops::Range};

pub const PREFIX: &str = "<!--readmd:math:v2:";
const MAX_PAYLOAD: usize = 1024 * 1024;
const STALE: &str = "math_metadata_stale";
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub schema: u8,
    pub id: String,
    pub source_revision: String,
    pub structure_revision: String,
    pub anchors: Vec<Anchor>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub node_id: String,
    pub namespace: String,
    pub local: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String,String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<Attribute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<OpaquePart>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribute { pub name: String, pub namespace: String, pub value: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpaquePart {
    /// Number of preceding standard element children, or UTF-16 text units
    /// for an Office text leaf. Text-leaf edits with attachments are guarded.
    pub offset: usize,
    pub text: bool,
    pub xml: String,
}
#[derive(Debug, Clone)]
pub struct Projection { pub source: String, pub metadata: Option<String>, pub preserved: usize }
pub struct Resolved { pub document: MathDocument, pub xml: String, pub payload: Payload }

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|c|c.is_ascii_alphanumeric() || matches!(c,b'-'|b'_'))
}
fn digest(value: &str) -> bool { value.len()==64 && value.bytes().all(|b|b.is_ascii_hexdigit()) }
fn namespace_valid(node:&XmlNode)->bool{
    (node.local()=="#root"||math_document::xml_qname(&node.name))&&(!node.name.contains(':')||!node.namespace.is_empty())
        &&node.attributes.iter().all(|(key,value)|if key=="xmlns"||key.starts_with("xmlns:"){math_document::namespace_binding(key,value)}else{math_document::xml_qname(key)&&(!key.contains(':')||node.attribute_namespaces.get(key).is_some_and(|ns|!ns.is_empty()))})
        &&node.children().all(namespace_valid)
}
fn standard(n: &XmlNode) -> bool {
    n.local()=="#root" || matches!(n.namespace.as_str(),WORD_NS|DRAWING_NS)
        || n.namespace==MATH_NS&&math_document::schema()["elements"].as_array().into_iter().flatten().any(|e|e["element"]==n.local())
        || crate::math_spacing::is_spacing(n)
}
fn namespaces(n: &XmlNode, inherited: &BTreeMap<String,String>) -> BTreeMap<String,String> {
    let mut out=inherited.clone();
    out.extend(n.attributes.iter().filter(|(k,_)|*k=="xmlns"||k.starts_with("xmlns:")).map(|(k,v)|(k.clone(),v.clone())));
    out
}
fn escape(s:&str)->String{s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\r',"&#13;")}

/// Capture exact opaque element bytes before the ordinary XML tree normalizes
/// quote style, attribute order or CDATA. No external entity is ever accepted.
fn raw_elements(xml:&str)->Result<BTreeMap<String,Range<usize>>> {
    let mut reader=Reader::from_str(xml);let mut stack=vec![("x".to_string(),0usize,0usize)];let mut out=BTreeMap::new();
    loop {
        let from=reader.buffer_position() as usize;
        match reader.read_event().map_err(|_|"math_invalid_xml")? {
            Event::Start(_)=>{let parent=stack.last_mut().unwrap();let id=format!("{}.{}",parent.0,parent.1);parent.1+=1;stack.push((id,0,from));},
            Event::Empty(_)=>{let parent=stack.last_mut().unwrap();let id=format!("{}.{}",parent.0,parent.1);parent.1+=1;out.insert(id,from..reader.buffer_position() as usize);},
            Event::End(_)=>{let(id,_,start)=stack.pop().ok_or("math_invalid_xml")?;if stack.is_empty(){return Err("math_invalid_xml");}out.insert(id,start..reader.buffer_position() as usize);},
            Event::DocType(_)=>return Err("math_unsafe_xml"),Event::Eof=>break,_=>(),
        }
    }
    Ok(out)
}
fn spend(remaining:&mut usize,bytes:usize)->Result<()>{*remaining=remaining.checked_sub(bytes).ok_or("math_budget")?;Ok(())}
fn escaped_len(value:&str)->usize{value.chars().map(|c|match c{'&'=>5,'<'|'>'=>4,'"'=>6,'\r'=>5,c=>c.len_utf8()}).sum()}
fn self_contained(raw:&str,n:&XmlNode,ns:&BTreeMap<String,String>,remaining:&mut usize)->Result<String>{
    let at=raw.find(|c:char|c.is_whitespace()||c=='>'||c=='/').ok_or("math_invalid_xml")?;
    let bytes=ns.iter().filter(|(key,_)|!n.attributes.contains_key(*key)).try_fold(raw.len(),|size,(key,value)|size.checked_add(key.len()+escaped_len(value)+4).ok_or("math_budget"))?;
    spend(remaining,bytes)?;
    let mut declarations=String::new();
    for(key,value)in ns {if !n.attributes.contains_key(key){declarations.push_str(&format!(" {key}=\"{}\"",escape(value)));}}
    Ok(format!("{}{}{}",&raw[..at],declarations,&raw[at..]))
}
fn supported_attribute(node:&XmlNode,key:&str)->bool{
    let ns=node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
    supported_attribute_namespace(node,key,ns)
}
fn supported_attribute_namespace(node:&XmlNode,key:&str,ns:&str)->bool{
    if key=="xmlns"||key.starts_with("xmlns:"){return true;}
    if key.starts_with("xml:"){return ns=="http://www.w3.org/XML/1998/namespace";}
    if node.namespace==MATH_NS {
        if ns!=MATH_NS{return false;}
        return math_document::schema()["elements"].as_array().into_iter().flatten().find(|e|e["element"]==node.local())
            .and_then(|e|e["attributes"].as_array()).is_some_and(|attrs|attrs.iter().any(|a|a["name"].as_str().and_then(|n|n.rsplit(':').next())==key.rsplit(':').next()));
    }
    ns.is_empty()||ns==node.namespace
}
fn project_node(node:&mut XmlNode,id:String,ns:&BTreeMap<String,String>,xml:&str,raw:&BTreeMap<String,Range<usize>>,anchors:&mut Vec<Anchor>,remaining:&mut usize)->Result<()> {
    let local_bindings;let inherited=if node.attributes.keys().any(|key|key=="xmlns"||key.starts_with("xmlns:")){local_bindings=namespaces(node,ns);&local_bindings}else{ns};let text=matches!(node.local(),"t"|"delText");
    let mut anchor=Anchor{node_id:id.clone(),namespace:node.namespace.clone(),local:node.local().into(),bindings:BTreeMap::new(),attributes:vec![],parts:vec![]};
    let attrs=node.attributes.keys().cloned().collect::<Vec<_>>();
    for key in attrs {
        if key=="xmlns"||key.starts_with("xmlns:"){node.attributes.remove(&key);continue;}
        if !supported_attribute(node,&key){
            spend(remaining,key.len()+node.attribute_namespaces.get(&key).map_or(0,String::len)+node.attributes.get(&key).map_or(0,String::len)+64)?;
            anchor.attributes.push(Attribute{name:key.clone(),namespace:node.attribute_namespaces.remove(&key).unwrap_or_default(),value:node.attributes.remove(&key).unwrap()});
        }
    }
    let mut content=Vec::new();let mut children=0;let mut units=0;
    for part in std::mem::take(&mut node.content){match part{
        XmlPart::Element(mut child) if standard(&child)=>{
            project_node(&mut child,format!("{id}.{children}"),inherited,xml,raw,anchors,remaining)?;children+=1;content.push(XmlPart::Element(child));
        },
        XmlPart::Element(child)=>{
            let range=raw.get(&child.id).ok_or("math_invalid_xml")?;
            anchor.parts.push(OpaquePart{offset:if text{units}else{children},text,xml:self_contained(&xml[range.clone()],&child,inherited,remaining)?});
        },
        XmlPart::Comment(value)=>{spend(remaining,value.len()+7)?;anchor.parts.push(OpaquePart{offset:if text{units}else{children},text,xml:format!("<!--{value}-->")});},
        XmlPart::Instruction(value)=>{spend(remaining,value.len()+4)?;anchor.parts.push(OpaquePart{offset:if text{units}else{children},text,xml:format!("<?{value}?>")});},
        XmlPart::Text(value) if text=>{units+=value.encode_utf16().count();if let Some(XmlPart::Text(previous))=content.last_mut(){previous.push_str(&value);}else{content.push(XmlPart::Text(value));}},
        XmlPart::Text(value) if value.trim().is_empty()=>(),
        XmlPart::Text(_)=>return Err("math_migration_loss"),
    }}
    node.content=content;node.id=id;
    if !anchor.attributes.is_empty(){spend(remaining,inherited.iter().map(|(key,value)|key.len()+value.len()+8).sum())?;anchor.bindings=inherited.clone();}
    if !anchor.parts.is_empty()||!anchor.attributes.is_empty(){spend(remaining,anchor.node_id.len()+anchor.local.len()+anchor.namespace.len()+128)?;anchors.push(anchor);}
    Ok(())
}
fn structure(root:&XmlNode)->String{
    fn shape(n:&XmlNode)->serde_json::Value{serde_json::json!([n.namespace,n.local(),n.children().map(shape).collect::<Vec<_>>()])}
    tx::revision(&shape(root).to_string())
}
fn clean_standard_trivia(node:&mut XmlNode){
    if !standard(node){return;}
    if !matches!(node.local(),"t"|"delText"){node.content.retain(|p|!matches!(p,XmlPart::Text(t) if t.trim().is_empty()));}
    for part in &mut node.content{if let XmlPart::Element(child)=part{clean_standard_trivia(child);}}
}
/// This verification does not trust attachment keys or the emitter's structure
/// digest. Reparse the emitted XML, remove uninterpreted data, then compare every
/// remaining standard element, attribute and text against the author's DSL.
fn strip_opaque(node:&mut XmlNode)->Result<()>{
    let keys=node.attributes.keys().cloned().collect::<Vec<_>>();
    for key in keys{if key=="xmlns"||key.starts_with("xmlns:")||!supported_attribute(node,&key){node.attributes.remove(&key);node.attribute_namespaces.remove(&key);}}
    let leaf=matches!(node.local(),"t"|"delText");let mut out=Vec::new();
    for part in std::mem::take(&mut node.content){match part{
        XmlPart::Element(mut child) if standard(&child)=>{strip_opaque(&mut child)?;out.push(XmlPart::Element(child));},
        XmlPart::Text(value) if leaf=>{if let Some(XmlPart::Text(previous))=out.last_mut(){previous.push_str(&value);}else{out.push(XmlPart::Text(value));}},
        XmlPart::Text(value) if !value.trim().is_empty()=>return Err("math_metadata_invalid"),
        _=>(),
    }}node.content=out;Ok(())
}
fn validate_projection(xml:&str,expected:&XmlNode)->Result<()>{
    let mut reconstructed=math_document::parse_xml(xml)?;if !namespace_valid(&reconstructed){return Err("math_metadata_invalid");}strip_opaque(&mut reconstructed)?;
    if math_document::xml_semantics(&reconstructed)!=math_document::xml_semantics(expected){return Err("math_metadata_invalid");}Ok(())
}
pub fn encode(payload:&Payload)->Result<String>{
    if !valid_id(&payload.id){return Err("math_metadata_invalid");}
    let bytes=serde_json::to_vec(payload).map_err(|_|"math_metadata_invalid")?;
    if bytes.len()>MAX_PAYLOAD{return Err("math_budget");}
    let mut encoder=flate2::write::GzEncoder::new(Vec::new(),flate2::Compression::fast());encoder.write_all(&bytes).map_err(|_|"math_metadata_invalid")?;
    Ok(format!("{PREFIX}{}:{}-->",payload.id,STANDARD.encode(encoder.finish().map_err(|_|"math_metadata_invalid")?)))
}
pub fn decode(comment:&str)->Result<Payload>{
    let(id,data)=comment.strip_prefix(PREFIX).and_then(|s|s.strip_suffix("-->")).and_then(|s|s.split_once(':')).ok_or("math_metadata_invalid")?;
    if !valid_id(id)||data.len()>MAX_PAYLOAD*2{return Err("math_metadata_invalid");}
    let bytes=STANDARD.decode(data).map_err(|_|"math_metadata_invalid")?;let mut decoded=Vec::new();
    flate2::read::GzDecoder::new(bytes.as_slice()).take((MAX_PAYLOAD+1)as u64).read_to_end(&mut decoded).map_err(|_|"math_metadata_invalid")?;
    if decoded.len()>MAX_PAYLOAD{return Err("math_budget");}
    let value:Payload=serde_json::from_slice(&decoded).map_err(|_|"math_metadata_invalid")?;
    if value.schema!=2||value.id!=id||!digest(&value.source_revision)||!digest(&value.structure_revision)||value.anchors.is_empty()||value.anchors.len()>12000{return Err("math_metadata_invalid");}
    let mut ids=BTreeSet::new();
    for anchor in &value.anchors{
        if !ids.insert(&anchor.node_id)||anchor.node_id.len()>512||anchor.parts.len()+anchor.attributes.len()>12000||!(anchor.local=="#root"||math_document::xml_ncname(&anchor.local))||!math_document::namespace_uri(&anchor.namespace){return Err("math_metadata_invalid");}
        if anchor.bindings.iter().any(|(key,value)|!math_document::namespace_binding(key,value)){return Err("math_metadata_invalid");}
        for attribute in &anchor.attributes{
            if !math_document::xml_qname(&attribute.name)||attribute.name=="xmlns"||attribute.name.starts_with("xmlns:")||!math_document::namespace_uri(&attribute.namespace)||attribute.namespace==math_document::XMLNS_NS{return Err("math_metadata_invalid");}
            if let Some((prefix,_))=attribute.name.split_once(':'){
                if attribute.namespace.is_empty()||prefix=="xmlns"||prefix=="xml"&&attribute.namespace!=math_document::XML_NS||prefix!="xml"&&attribute.namespace==math_document::XML_NS{return Err("math_metadata_invalid");}
            }else if !attribute.namespace.is_empty(){return Err("math_metadata_invalid");}
        }
        if anchor.parts.is_empty()&&anchor.attributes.is_empty(){return Err("math_metadata_invalid");}
        for p in &anchor.parts{
            let raw=math_document::parse_xml(&p.xml)?;
            if !namespace_valid(&raw)||raw.content.len()!=1||raw.content.iter().any(|part|matches!(part,XmlPart::Text(_)))||raw.children().any(standard){return Err("math_metadata_invalid");}
        }
    }
    Ok(value)
}
pub fn capture(xml:&str,id:&str,display:bool)->Result<Projection>{
    if !valid_id(id){return Err("math_metadata_invalid");}
    let mut root=math_document::parse_xml(xml)?;if !namespace_valid(&root){return Err("math_invalid_xml");}let mut original=root.clone();let raw=raw_elements(xml)?;let mut anchors=Vec::new();
    let defaults=BTreeMap::from([("xmlns:m".into(),MATH_NS.into()),("xmlns:w".into(),WORD_NS.into())]);
    let mut remaining=MAX_PAYLOAD;project_node(&mut root,"x".into(),&defaults,xml,&raw,&mut anchors,&mut remaining)?;
    let source=crate::math_dsl::decompile(&root)?;let compiled=math_document::parse(&source,"dsl",false)?;
    if math_document::xml_semantics(&root)!=math_document::xml_semantics(&compiled.root){return Err("math_migration_loss");}
    let displayed=math_document::parse(&source,"dsl",display)?;
    let display_wrapper=structure(&displayed.root)!=structure(&compiled.root);
    if display_wrapper{
        // The DSL compiler supplies the display paragraph wrapper. Original
        // attachment IDs remain relative to the original oMath inside it.
        if compiled.root.children().count()!=1||compiled.root.children().next().is_none_or(|n|n.local()!="oMath"){return Err("math_migration_loss");}
        for anchor in &mut anchors{if anchor.node_id=="x.0"||anchor.node_id.starts_with("x.0."){anchor.node_id=format!("x.0.0{}",&anchor.node_id[3..]);}}
    }
    let preserved=anchors.iter().map(|a|a.parts.len()+a.attributes.len()).sum();
    let metadata=if anchors.is_empty(){None}else{
        let payload=Payload{schema:2,id:id.into(),source_revision:tx::revision(&source),structure_revision:structure(&displayed.root),anchors};
        let comment=encode(&payload)?;
        // Validate the complete reconstruction before exposing a migration.
        let restored=resolve(&source,&comment,Some(id),display)?;let mut roundtrip=math_document::parse_xml(&restored.xml)?;
        if display_wrapper{
            for part in &mut roundtrip.content{if let XmlPart::Element(node)=part{if node.namespace==MATH_NS&&node.local()=="oMathPara"{
                let child=node.children().next().cloned().ok_or("math_migration_loss")?;*node=child;
            }}}
        }
        clean_standard_trivia(&mut original);clean_standard_trivia(&mut roundtrip);
        if math_document::xml_semantics(&original)!=math_document::xml_semantics(&roundtrip){return Err("math_migration_loss");}
        Some(comment)
    };
    Ok(Projection{source,metadata,preserved})
}
fn find<'a>(node:&'a XmlNode,id:&str)->Option<&'a XmlNode>{if node.id==id{Some(node)}else{node.children().find_map(|c|find(c,id))}}
fn add_attributes(node:&XmlNode,anchor:Option<&Anchor>)->Result<BTreeMap<String,String>>{
    let mut attributes=node.attributes.clone();
    if let Some(anchor)=anchor{for(key,value)in &anchor.bindings{
        if !math_document::namespace_binding(key,value){return Err("math_metadata_invalid");}
        if matches!(key.as_str(),"xmlns:m"|"xmlns:w"|"xmlns:a")&&value!=match key.as_str(){"xmlns:m"=>MATH_NS,"xmlns:w"=>WORD_NS,_=>DRAWING_NS}{return Err("math_unsupported_extension");}
        if attributes.get(key).is_some_and(|existing|existing!=value){return Err("math_unsupported_extension");}attributes.insert(key.clone(),value.clone());
    }}
    if let Some(anchor)=anchor{for attr in &anchor.attributes{
        let mut name=attr.name.clone();
        if let Some((prefix,local))=attr.name.split_once(':'){
            if attr.namespace.is_empty(){return Err("math_metadata_invalid");}
            let declaration=format!("xmlns:{prefix}");
            if attributes.get(&declaration).is_some_and(|ns|ns!=&attr.namespace)
                || matches!(prefix,"m"|"w"|"a") && attr.namespace!=match prefix{"m"=>MATH_NS,"w"=>WORD_NS,_=>DRAWING_NS}{
                let mut i=0;while attributes.contains_key(&format!("xmlns:readmdext{i}")){i+=1;}
                let prefix=format!("readmdext{i}");name=format!("{prefix}:{local}");attributes.insert(format!("xmlns:{prefix}"),attr.namespace.clone());
            }else{attributes.insert(declaration,attr.namespace.clone());}
        }
        if attributes.insert(name,attr.value.clone()).is_some(){return Err(STALE);}
    }}
    Ok(attributes)
}
fn emit(node:&XmlNode,anchors:&BTreeMap<&str,&Anchor>,out:&mut String)->Result<()> {
    let anchor=anchors.get(node.id.as_str()).copied();
    if node.name!="#root"{out.push('<');out.push_str(&node.name);for(k,v)in add_attributes(node,anchor)?{out.push_str(&format!(" {k}=\"{}\"",escape(&v)));}out.push('>');}
    let text_leaf=matches!(node.local(),"t"|"delText");
    if text_leaf{
        let value=node.text();let mut from=0;
        for part in anchor.into_iter().flat_map(|a|a.parts.iter()){
            if !part.text||part.offset<from{return Err(STALE);}let to=tx::byte_offset(&value,part.offset)?;let start=tx::byte_offset(&value,from)?;
            out.push_str(&escape(&value[start..to]));out.push_str(&part.xml);from=part.offset;
        }
        out.push_str(&escape(&value[tx::byte_offset(&value,from)?..]));
    }else{
        let children=node.children().collect::<Vec<_>>();
        if node.content.iter().any(|p|!matches!(p,XmlPart::Element(_))){return Err(STALE);}
        for index in 0..=children.len(){
            for part in anchor.into_iter().flat_map(|a|a.parts.iter()).filter(|p|p.offset==index){if part.text{return Err(STALE);}out.push_str(&part.xml);}
            if let Some(child)=children.get(index){emit(child,anchors,out)?;}
        }
        if anchor.is_some_and(|a|a.parts.iter().any(|p|p.offset>children.len())){return Err(STALE);}
    }
    if node.name!="#root"{out.push_str("</");out.push_str(&node.name);out.push('>');}Ok(())
}
pub fn resolve(source:&str,comment:&str,expected_id:Option<&str>,display:bool)->Result<Resolved>{
    let payload=decode(comment)?;
    if expected_id.is_some_and(|id|id!=payload.id)||payload.source_revision!=tx::revision(source){return Err(STALE);}
    let document=math_document::parse(source,"dsl",display)?;
    if structure(&document.root)!=payload.structure_revision{return Err(STALE);}
    for anchor in &payload.anchors{
        let node=find(&document.root,&anchor.node_id).ok_or(STALE)?;
        if node.namespace!=anchor.namespace||node.local()!=anchor.local{return Err(STALE);}
        if node.local()=="#root"&&!anchor.attributes.is_empty(){return Err("math_metadata_invalid");}
        for attribute in &anchor.attributes{
            if supported_attribute_namespace(node,&attribute.name,&attribute.namespace){return Err("math_metadata_invalid");}
        }
    }
    let map=payload.anchors.iter().map(|a|(a.node_id.as_str(),a)).collect();let mut xml=String::new();emit(&document.root,&map,&mut xml)?;
    validate_projection(&xml,&document.root)?;
    Ok(Resolved{document,xml,payload})
}
/// A successful text/property edit must retain the complete structural map.
/// If an operation changes that map, keep the draft and ask for recovery review
/// instead of guessing where an uninterpreted object should move.
pub fn rebind(source:&str,comment:&str,expected_id:Option<&str>,changed:&MathDocument,operation:&math_document::MathPatch,display:bool)->Result<String>{
    let original=resolve(source,comment,expected_id,display)?;let mut payload=original.payload;
    if structure(&changed.root)!=payload.structure_revision{return Err("math_unsupported_extension");}
    let text_id=match operation{math_document::MathPatch::ReplaceText{node_id,..}|math_document::MathPatch::ReplaceTextRange{node_id,..}=>Some(node_id),_=>None};
    if text_id.is_some_and(|id|payload.anchors.iter().any(|a|a.node_id==*id&&!a.parts.is_empty())){return Err("math_unsupported_extension");}
    // A source/content replacement can replace equal-looking siblings. Unlike
    // an explicit leaf edit, equality of shape cannot establish their identity.
    let target=match operation{math_document::MathPatch::ReplaceSource{node_id,..}|math_document::MathPatch::ReplaceSlot{node_id,..}|math_document::MathPatch::ReplaceContent{node_id,..}|math_document::MathPatch::ReplaceDslContent{node_id,..}=>Some(node_id),_=>None};
    if target.is_some_and(|id|{
        let scalar=find(&original.document.root,id).is_some_and(|n|matches!(n.local(),"r"|"t"|"delText"));
        payload.anchors.iter().any(|a|(a.node_id==*id||a.node_id.starts_with(&format!("{id}.")))
            &&(!scalar||a.parts.iter().any(|p|p.text)))
    }){return Err("math_unsupported_extension");}
    payload.source_revision=changed.revision.clone();let metadata=encode(&payload)?;resolve(&changed.source,&metadata,expected_id,display)?;Ok(metadata)
}

/// Adjacent whitespace is part of the consuming document transaction, never
/// found by a global text replacement. Ranges here are UTF-8 byte boundaries.
pub fn adjacent(source:&str,after:usize)->Option<(usize,usize,&str)>{
    let tail=source.get(after..)?;let gap=tail.len()-tail.trim_start_matches(char::is_whitespace).len();let from=after+gap;
    let value=source.get(from..)?;if !value.starts_with(PREFIX){return None;}let to=from+value.find("-->")?+3;Some((from,to,&source[from..to]))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn xml()->String{format!(r#"<q:oMath xmlns:q="{MATH_NS}" xmlns:z="urn:vendor" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><q:f z:flag="keep"><q:num><q:r><q:t>a</q:t></q:r><!-- preserved --><z:settings b='2' a='1'><![CDATA[x<y]]></z:settings></q:num><q:den><q:r><q:t>b</q:t></q:r></q:den></q:f><mc:AlternateContent><mc:Choice Requires="z"><z:item/></mc:Choice><mc:Fallback><q:r><q:t>fallback</q:t></q:r></mc:Fallback></mc:AlternateContent></q:oMath>"#)}
    #[test]fn extension_carrier_contains_no_standard_tree_and_restores_opaque_bytes(){
        let original=xml();let p=capture(&original,"equation-two",false).unwrap();assert!(p.source.contains("frac("));assert!(!p.source.contains("urn:vendor"));assert_eq!(p.preserved,4);
        let comment=p.metadata.unwrap();let payload=decode(&comment).unwrap();let serialized=serde_json::to_string(&payload).unwrap();assert!(!serialized.contains("<q:f"));assert!(!serialized.contains("<q:den"));assert!(!serialized.contains("omml"));
        let restored=resolve(&p.source,&comment,Some("equation-two"),false).unwrap();assert!(restored.xml.contains("b='2' a='1'><![CDATA[x<y]]>"));assert!(restored.xml.contains("<!-- preserved -->"));assert!(restored.xml.contains("mc:AlternateContent"));assert!(restored.xml.contains("z:flag=\"keep\""));
        let left=math_document::parse_xml(&original).unwrap();let right=math_document::parse_xml(&restored.xml).unwrap();assert_eq!(math_document::xml_semantics(&left),math_document::xml_semantics(&right));
    }
    #[test]fn neighbor_text_edits_rebind_but_stale_identity_source_and_structure_never_restore(){
        let p=capture(&xml(),"second",false).unwrap();let metadata=p.metadata.unwrap();let doc=math_document::parse(&p.source,"dsl",false).unwrap();
        fn leaf(n:&XmlNode)->Option<&XmlNode>{if n.local()=="t"&&n.text()=="b"{Some(n)}else{n.children().find_map(leaf)}}let id=leaf(&doc.root).unwrap().id.clone();
        let operation=math_document::MathPatch::ReplaceText{node_id:id,text:"β".into()};let changed=math_document::patch(&p.source,"dsl",false,&doc.revision,&operation).unwrap();let next=rebind(&p.source,&metadata,Some("second"),&changed,&operation,false).unwrap();
        assert!(resolve(&changed.source,&next,Some("second"),false).unwrap().xml.contains("β"));assert!(resolve(&changed.source,&metadata,Some("second"),false).is_err());assert!(resolve(&p.source,&metadata,Some("first"),false).is_err());
        let mut payload=decode(&metadata).unwrap();payload.structure_revision="0".repeat(64);assert!(resolve(&p.source,&encode(&payload).unwrap(),Some("second"),false).is_err());
    }
    #[test]fn display_wrappers_and_qname_bindings_remain_bound_to_their_formula(){
        let original=format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:z="urn:vendor" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="z"><m:r><m:t>a</m:t></m:r><z:custom/></m:oMath>"#);
        let captured=capture(&original,"display",true).unwrap();let metadata=captured.metadata.unwrap();let restored=resolve(&captured.source,&metadata,Some("display"),true).unwrap();
        assert!(restored.xml.contains("oMathPara"));assert!(restored.xml.contains("xmlns:z=\"urn:vendor\""));assert!(restored.xml.contains("mc:Ignorable=\"z\""));
        assert!(resolve(&captured.source,&metadata,Some("display"),false).is_err());
    }
    #[test]fn repeated_nested_source_edits_keep_comments_and_utf16_spans_without_global_replacement(){
        let source="// 😀 Author note\r\nrow(frac( x , root(3, β) ), frac( x , root(3, β) ), frac( x , root(3, β) )) // End\r\n";
        let doc=math_document::parse(source,"dsl",false).unwrap();let second=doc.nodes.iter().filter(|n|n.kind=="rad").nth(1).unwrap();
        let operation=math_document::MathPatch::ReplaceSlot{node_id:second.id.clone(),role:"deg".into(),text:"𝟜".into()};let changed=math_document::patch(source,"dsl",false,&doc.revision,&operation).unwrap();
        assert_eq!(changed.source,"// 😀 Author note\r\nrow(frac( x , root(3, β) ), frac( x , root(𝟜, β) ), frac( x , root(3, β) )) // End\r\n");
        assert_eq!(math_document::patch(source,"dsl",false,"stale",&operation).unwrap_err(),"transaction_conflict");
    }
    #[test]fn legacy_unknown_extensions_migrate_once_and_stale_or_duplicate_carriers_go_to_recovery(){
        let legacy=math_document::metadata("a/b",&xml(),"duplicate").unwrap();
        let source=format!("\u{feff}# 😀 Author\r\n\r\nFirst $a/b${legacy}. Second $a/b${legacy}. Third $a/b${legacy}.\r\n");
        let migrated=crate::math_migration::plan(&source).unwrap();assert_eq!(migrated.edits.len(),3);assert!(migrated.recoveries.is_empty());assert!(migrated.diagnostics.is_empty(),"{:?}",migrated.diagnostics);
        let ranges=crate::math_dsl::markdown_ranges(&migrated.content).unwrap();assert_eq!(ranges.len(),3);let mut ids=BTreeSet::new();
        for range in &ranges{let(_,_,comment)=adjacent(&migrated.content,range.to).unwrap();assert!(ids.insert(decode(comment).unwrap().id));}
        assert_eq!(crate::math_migration::plan(&migrated.content).unwrap().content,migrated.content);
        let range=&ranges[1];let changed=tx::apply(&migrated.content,&tx::ContentTransaction{base_revision:tx::revision(&migrated.content),origin:"formula".into(),edits:vec![tx::TextEdit{from:migrated.content[..range.body_from].encode_utf16().count(),to:migrated.content[..range.body_to].encode_utf16().count(),insert:"z + 1".into()}]}).unwrap();
        let stale=crate::math_migration::plan(&changed.content).unwrap();assert_eq!(stale.recoveries.len(),1);assert!(stale.content.contains(":math[z + 1]"));assert_eq!(stale.content.matches(PREFIX).count(),2);assert!(stale.recoveries[0].content.contains(PREFIX));
        assert_eq!(crate::math_dsl::expand_markdown(&changed.content).unwrap_err(),STALE);
        let captured=capture(&xml(),"same",false).unwrap();let formula=format!(":math[{}]{}",captured.source,captured.metadata.unwrap());let duplicate=format!("{formula} and {formula}");
        assert_eq!(crate::math_dsl::expand_markdown(&duplicate).unwrap_err(),STALE);let review=crate::math_migration::plan(&duplicate).unwrap();assert_eq!(review.recoveries.len(),2);assert!(!review.content.contains(PREFIX));
    }
    #[test]fn attached_text_comments_cannot_be_repositioned_by_an_ambiguous_text_edit(){
        let original="<m:oMath><m:r><m:t>A<!-- exact -->B</m:t></m:r></m:oMath>";let captured=capture(original,"comment",false).unwrap();let metadata=captured.metadata.unwrap();
        let restored=resolve(&captured.source,&metadata,None,false).unwrap();assert!(restored.xml.contains("A<!-- exact -->B"));
        let operation=math_document::MathPatch::ReplaceText{node_id:"x.0.0.0".into(),text:"XYZ".into()};let changed=math_document::patch(&captured.source,"dsl",false,&restored.document.revision,&operation).unwrap();
        assert_eq!(rebind(&captured.source,&metadata,None,&changed,&operation,false).unwrap_err(),"math_unsupported_extension");
    }
    #[test]fn standard_invalid_properties_are_never_hidden_as_opaque_extensions(){
        for xml in ["<m:oMath><m:d><m:dPr><m:endChr/></m:dPr><m:e/></m:d></m:oMath>","<m:oMath><m:f><m:den/></m:f></m:oMath>"]{assert!(capture(xml,"eq",false).is_err());}
        let plain=capture("<m:oMath><m:r><m:t>a</m:t></m:r></m:oMath>","plain",false).unwrap();assert!(plain.metadata.is_none());assert_eq!(plain.preserved,0);
    }
    #[test]fn corrupt_payloads_cannot_hide_standard_content_or_use_undeclared_prefixes(){
        let captured=capture(&xml(),"guard",false).unwrap();let comment=captured.metadata.unwrap();let original=decode(&comment).unwrap();
        for raw in ["<m:r><m:t>hidden standard</m:t></m:r>","<undeclared:future/>","<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///private'>]><x>&e;</x>"]{
            let mut payload=original.clone();payload.anchors.iter_mut().find(|a|!a.parts.is_empty()).unwrap().parts[0].xml=raw.into();assert!(decode(&encode(&payload).unwrap()).is_err());
        }
        let mut payload=original;payload.id="different".into();let tampered=encode(&payload).unwrap().replacen(":different:",":guard:",1);assert_eq!(decode(&tampered).unwrap_err(),"math_metadata_invalid");
    }
    #[test]fn carrier_binding_and_attribute_names_cannot_inject_xml_or_rebind_reserved_namespaces(){
        let captured=capture("<m:oMath><m:r><m:t>a</m:t><z:custom xmlns:z=\"urn:z\"/></m:r></m:oMath>","injection",false).unwrap();let comment=captured.metadata.unwrap();let payload=decode(&comment).unwrap();
        let malicious=["xmlns:zz=\"urn:z\"></m:r><m:r><m:t>INJECTED</m:t></m:r><m:r injected","xmlns:bad key","xmlns:bad\tkey","xmlns:bad\nkey","xmlns:bad'key","xmlns:bad\"key","xmlns:bad<key","xmlns:bad>key","xmlns:bad:key","xmlns:"];
        for key in malicious{
            let mut forged=payload.clone();forged.anchors[0].bindings.insert(key.into(),"ok".into());let encoded=encode(&forged).unwrap();assert_eq!(resolve(&captured.source,&encoded,None,false).err(),Some("math_metadata_invalid"),"{key}");
        }
        for(key,value)in [("xmlns:xml","urn:evil"),("xmlns:xmlns",math_document::XMLNS_NS),("xmlns:z",math_document::XMLNS_NS),("xmlns:z",math_document::XML_NS),("xmlns",math_document::XML_NS),("xmlns:z",""),("xmlns:z","urn:bad uri"),("xmlns:z","urn:bad\"uri"),("xmlns:z","urn:bad<uri")]{
            let mut forged=payload.clone();forged.anchors[0].bindings.insert(key.into(),value.into());assert!(resolve(&captured.source,&encode(&forged).unwrap(),None,false).is_err(),"{key}: {value}");
        }
        for name in ["bad key","bad\tkey","bad\nkey","bad'key","bad\"key","bad<key","bad>key","z:bad:key","xmlns:z"]{
            let mut forged=payload.clone();forged.anchors[0].attributes.push(Attribute{name:name.into(),namespace:String::new(),value:"keep".into()});assert!(resolve(&captured.source,&encode(&forged).unwrap(),None,false).is_err(),"{name}");
        }
        for raw in ["<z:custom xmlns:z=\"urn:z\" xmlns:xml=\"urn:evil\"/>","<z:custom xmlns:z=\"urn:z\" xmlns:xmlns=\"urn:evil\"/>","<z:custom xmlns:z=\"http://www.w3.org/2000/xmlns/\"/>"]{
            let mut forged=payload.clone();forged.anchors[0].parts[0].xml=raw.into();assert!(resolve(&captured.source,&encode(&forged).unwrap(),None,false).is_err());assert!(capture(&format!("<m:oMath>{raw}</m:oMath>"),"bad",false).is_err());
        }
        let restored=resolve(&captured.source,&comment,None,false).unwrap();let injected=restored.xml.replacen("</m:r>","</m:r><m:r><m:t>INJECTED</m:t></m:r>",1);
        assert!(math_document::parse_xml(&injected).is_ok());assert_eq!(validate_projection(&injected,&restored.document.root),Err("math_metadata_invalid"));
    }
    #[test]fn namespace_and_opaque_expansion_budgets_are_checked_before_amplifying_input(){
        let max=math_document::MAX_NAMESPACE_URI_BYTES;let uri=format!("urn:{}","a".repeat(max-4));
        let bounded=format!("<m:oMath xmlns:z=\"{uri}\"><m:r><m:t>a</m:t><z:future/></m:r></m:oMath>");assert!(capture(&bounded,"bounded",false).is_ok());
        let oversized=format!("<m:oMath xmlns:z=\"{uri}a\"><z:future/></m:oMath>");assert_eq!(math_document::parse_xml(&oversized).unwrap_err(),"math_budget");
        let repeated=format!("<m:oMath xmlns:z=\"{uri}\">{}</m:oMath>","<z:x/>".repeat(600));assert!(repeated.len()<32*1024);assert_eq!(math_document::parse_xml(&repeated).unwrap_err(),"math_budget");
        let declarations=format!("<m:oMath>{}</m:oMath>",format!("<m:r xmlns:z=\"{uri}\"/>").repeat(70));assert!(declarations.len()<300*1024);assert_eq!(math_document::parse_xml(&declarations).unwrap_err(),"math_budget");
        let raw="<z:custom/>";let parsed=math_document::parse_xml("<m:oMath xmlns:z=\"urn:z\"><z:custom/></m:oMath>").unwrap();let node=parsed.children().next().unwrap().children().next().unwrap();
        let bindings=BTreeMap::from([("xmlns:z".into(),"urn:z".into()),("xmlns:extra".into(),"urn:extra&more".into())]);let mut budget=1024;let standalone=self_contained(raw,node,&bindings,&mut budget).unwrap();let exact=standalone.len();assert_eq!(1024-budget,exact);
        let mut too_small=exact-1;assert_eq!(self_contained(raw,node,&bindings,&mut too_small),Err("math_budget"));assert_eq!(too_small,exact-1);
        let mut exact_budget=exact;assert_eq!(self_contained(raw,node,&bindings,&mut exact_budget).unwrap(),standalone);assert_eq!(exact_budget,0);
    }
    #[test]fn all_direct_export_entries_preserve_old_targets_when_carriers_are_stale_invalid_or_unmapped(){
        let directory=tempfile::tempdir().unwrap();let root=directory.path();let base=root.to_str().unwrap();
        let original="<m:oMath><m:r><m:t>a</m:t><z:custom xmlns:z=\"urn:z\"/></m:r></m:oMath>";
        let captured=capture(original,"direct",false).unwrap();let comment=captured.metadata.unwrap();let current=format!(":math[{}]{comment}",captured.source);
        let displayed=capture(original,"nested",true).unwrap();let nested=format!("> ```readmd-math v1\n> {}\n> ```\n\n{}",displayed.source,displayed.metadata.unwrap());
        let cases=[(format!(":math[b]{comment}"),"math_metadata_stale"),(format!("{current} and {current}"),"math_metadata_stale"),(":math[a]<!--readmd:math:v2:invalid:bm90LWd6aXA=-->".into(),"math_metadata_invalid"),(nested,"math_unsupported_extension")];
        let options=serde_json::json!({"pdf":{"engine":"rust"},"cover":{"enabled":false},"toc":{"enabled":false},"footer":{"pageNumbers":false}});
        for (source,code) in cases{
            for format in ["docx","pdf","tex","epub","html"]{
                let target=root.join(format!("protected.{format}"));std::fs::write(&target,b"OWNED PREVIOUS OUTPUT").unwrap();let output=target.to_str().unwrap();
                let result=match format{
                    "docx"=>crate::mdexport::export_docx(&source,base,output,&options,"guard"),
                    "pdf"=>crate::mdexport::export_pdf(&source,base,output,&options,"guard",root),
                    "tex"=>crate::mdexport::export_tex(&source,base,output,&options,"guard"),
                    "epub"=>crate::mdexport::export_epub(&source,base,output,&options,"guard"),
                    _=>crate::mdexport::export_html(&source,base,output,&options,"guard",root),
                };
                assert_eq!(result.unwrap_err(),code,"{format}");assert_eq!(std::fs::read(&target).unwrap(),b"OWNED PREVIOUS OUTPUT");
            }
        }
        let mut names=std::fs::read_dir(root).unwrap().map(|entry|entry.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();names.sort();assert_eq!(names,["protected.docx","protected.epub","protected.html","protected.pdf","protected.tex"]);
    }
}
