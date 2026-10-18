//! Lossless Office Math storage and revision-checked mathematical edits.
//! XML namespace prefixes, explicit false values and Word run properties survive
//! editing. Source positions use UTF-16, matching the editor, never glyph text.
use base64::{engine::general_purpose::STANDARD, Engine};
use quick_xml::{events::Event, Reader, XmlVersion};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::OnceLock;
use crate::document_transaction::{self as transaction, ContentTransaction, TextEdit};

const MAX_XML: usize = 1024 * 1024;
const MAX_SOURCE: usize = 16000;
const MAX_NODES: usize = 12000;
const MAX_DEPTH: usize = 128;
pub(crate) const MAX_XML_NAME_BYTES: usize = 512;
pub(crate) const MAX_NAMESPACE_URI_BYTES: usize = 4096;
const MAX_NAMESPACE_DECLARATION_BYTES: usize = 256 * 1024;
const MAX_NAMESPACE_RESOLUTION_BYTES: usize = 2 * 1024 * 1024;
pub const MATH_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
pub const WORD_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub(crate) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
pub(crate) const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

pub(crate) fn xml_ncname(value:&str)->bool{
    fn start(c:char)->bool{matches!(c,'A'..='Z'|'a'..='z'|'_'|'\u{c0}'..='\u{d6}'|'\u{d8}'..='\u{f6}'|'\u{f8}'..='\u{2ff}'|'\u{370}'..='\u{37d}'|'\u{37f}'..='\u{1fff}'|'\u{200c}'..='\u{200d}'|'\u{2070}'..='\u{218f}'|'\u{2c00}'..='\u{2fef}'|'\u{3001}'..='\u{d7ff}'|'\u{f900}'..='\u{fdcf}'|'\u{fdf0}'..='\u{fffd}'|'\u{10000}'..='\u{effff}')}
    let mut chars=value.chars();value.len()<=MAX_XML_NAME_BYTES&&chars.next().is_some_and(start)&&chars.all(|c|start(c)||matches!(c,'-'|'.'|'0'..='9'|'\u{b7}'|'\u{300}'..='\u{36f}'|'\u{203f}'..='\u{2040}'))
}
pub(crate) fn xml_qname(value:&str)->bool{
    value.len()<=MAX_XML_NAME_BYTES&&match value.split_once(':'){Some((prefix,local))=>xml_ncname(prefix)&&xml_ncname(local),None=>xml_ncname(value)}
}
pub(crate) fn namespace_uri(value:&str)->bool{
    value.len()<=MAX_NAMESPACE_URI_BYTES&&!value.chars().any(|c|c.is_whitespace()||c.is_control()||matches!(c,'<'|'>'|'"'))
}
pub(crate) fn namespace_binding(key:&str,value:&str)->bool{
    let prefix=if key=="xmlns"{""}else{let Some(prefix)=key.strip_prefix("xmlns:")else{return false;};if !xml_ncname(prefix){return false;}prefix};
    namespace_uri(value)&&prefix!="xmlns"&&value!=XMLNS_NS&&if prefix=="xml"{value==XML_NS}else{value!=XML_NS&&(prefix.is_empty()||!value.is_empty())}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum XmlPart { Element(XmlNode), Text(String), Comment(String), Instruction(String) }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XmlNode {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub namespace: String,
    pub attributes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attribute_namespaces: BTreeMap<String, String>,
    pub content: Vec<XmlPart>,
}

impl XmlNode {
    fn new(name: &str) -> Self { Self { id: String::new(), name: name.into(), namespace: String::new(), attributes: BTreeMap::new(), attribute_namespaces:BTreeMap::new(),content: vec![] } }
    pub fn local(&self) -> &str { self.name.rsplit(':').next().unwrap_or(&self.name) }
    pub fn children(&self) -> impl Iterator<Item = &XmlNode> { self.content.iter().filter_map(|p| if let XmlPart::Element(n) = p { Some(n) } else { None }) }
    fn find(&self, id: &str) -> Option<&Self> { if self.id == id { Some(self) } else { self.children().find_map(|child| child.find(id)) } }
    pub fn text(&self) -> String { self.content.iter().map(|p| match p { XmlPart::Text(s) => s.clone(), XmlPart::Element(n) => n.text(), _ => String::new() }).collect() }
    fn find_mut(&mut self, id: &str) -> Option<&mut Self> {
        if self.id == id { return Some(self); }
        for part in &mut self.content { if let XmlPart::Element(n) = part { if let Some(found) = n.find_mut(id) { return Some(found); } } }
        None
    }
    fn assign_ids(&mut self, id: String) {
        self.id = id;
        let mut i = 0;
        for part in &mut self.content { if let XmlPart::Element(n) = part { n.assign_ids(format!("{}.{}", self.id, i)); i += 1; } }
    }
    pub fn xml(&self) -> String {
        let mut out = String::new(); self.emit(&mut out); out
    }
    fn emit(&self, out: &mut String) {
        if self.name != "#root" {
            out.push('<'); out.push_str(&self.name);
            for (key, value) in &self.attributes { out.push(' '); out.push_str(key); out.push_str("=\""); out.push_str(&escape(value)); out.push('"'); }
            if self.content.is_empty() { out.push_str("/>"); return; }
            out.push('>');
        }
        for part in &self.content { match part {
            XmlPart::Element(n) => n.emit(out), XmlPart::Text(s) => out.push_str(&escape(s)),
            XmlPart::Comment(s) => { out.push_str("<!--"); out.push_str(s); out.push_str("-->"); },
            XmlPart::Instruction(s) => { out.push_str("<?"); out.push_str(s); out.push_str("?>"); },
        } }
        if self.name != "#root" { out.push_str("</"); out.push_str(&self.name); out.push('>'); }
    }
}

fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\r', "&#13;") }
fn start_node(start: &quick_xml::events::BytesStart<'_>) -> Result<XmlNode, &'static str> {
    if start.name().as_ref().len()>MAX_XML_NAME_BYTES{return Err("math_budget");}
    if !xml_qname(start.name().as_ref()){return Err("math_invalid_xml");}
    let mut node = XmlNode::new(start.name().as_ref());
    for attr in start.attributes() {
        let attr = attr.map_err(|_| "math_invalid_xml")?;
        let key=attr.key.as_ref();if key.len()>MAX_XML_NAME_BYTES{return Err("math_budget");}if !xml_qname(key){return Err("math_invalid_xml");}
        let value=attr.normalized_value(XmlVersion::Implicit1_0).map_err(|_| "math_invalid_xml")?.into_owned();
        if key=="xmlns"||key.starts_with("xmlns:"){
            if value.len()>MAX_NAMESPACE_URI_BYTES{return Err("math_budget");}
            if !namespace_binding(key,&value){return Err("math_invalid_xml");}
        }
        node.attributes.insert(key.into(),value);
    }
    Ok(node)
}

fn resolve_namespace(node:&mut XmlNode,ancestors:&[XmlNode],remaining:&mut usize)->Result<(),&'static str>{
    let resolve=|prefix:&str|{
        let key=if prefix.is_empty(){"xmlns".into()}else{format!("xmlns:{prefix}")};
        node.attributes.get(&key).or_else(||ancestors.iter().rev().find_map(|parent|parent.attributes.get(&key)))
            .map(String::as_str).unwrap_or(match prefix{"m"=>MATH_NS,"w"=>WORD_NS,"xml"=>XML_NS,_=>""})
    };
    let namespace=resolve(node.name.split_once(':').map(|p|p.0).unwrap_or(""));
    *remaining=remaining.checked_sub(namespace.len()).ok_or("math_budget")?;
    let namespace=namespace.to_string();let mut attributes=BTreeMap::new();
    for key in node.attributes.keys(){if let Some((prefix,_))=key.split_once(':').filter(|(prefix,_)|*prefix!="xmlns"){
        let uri=resolve(prefix);*remaining=remaining.checked_sub(uri.len()).ok_or("math_budget")?;attributes.insert(key.clone(),uri.to_string());
    }}
    node.namespace=namespace;node.attribute_namespaces=attributes;Ok(())
}

pub fn parse_xml(source: &str) -> Result<XmlNode, &'static str> {
    if source.len() > MAX_XML { return Err("math_budget"); }
    if source.chars().any(|c|!matches!(c,'\t'|'\r'|'\n') && ((c as u32)<32 || matches!(c,'\u{fffe}'|'\u{ffff}'))) {return Err("math_invalid_xml");}
    let mut reader = Reader::from_str(source);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut stack = vec![XmlNode::new("#root")];
    let mut nodes = 0;
    let mut namespace_remaining=MAX_NAMESPACE_RESOLUTION_BYTES;
    let mut declaration_remaining=MAX_NAMESPACE_DECLARATION_BYTES;
    loop { match reader.read_event().map_err(|_| "math_invalid_xml")? {
        Event::Start(start) => {
            nodes += 1;
            if nodes > MAX_NODES || stack.len() >= MAX_DEPTH { return Err("math_budget"); }
            let mut node=start_node(&start)?;for(k,v)in &node.attributes{if k=="xmlns"||k.starts_with("xmlns:"){declaration_remaining=declaration_remaining.checked_sub(k.len()+v.len()).ok_or("math_budget")?;}}resolve_namespace(&mut node,&stack,&mut namespace_remaining)?;stack.push(node);
        }
        Event::Empty(start) => { nodes += 1; if nodes > MAX_NODES || stack.len() >= MAX_DEPTH { return Err("math_budget"); } let mut node=start_node(&start)?;for(k,v)in &node.attributes{if k=="xmlns"||k.starts_with("xmlns:"){declaration_remaining=declaration_remaining.checked_sub(k.len()+v.len()).ok_or("math_budget")?;}}resolve_namespace(&mut node,&stack,&mut namespace_remaining)?;stack.last_mut().unwrap().content.push(XmlPart::Element(node)); }
        Event::End(_) => {
            if stack.len() <= 1 { return Err("math_invalid_xml"); }
            let node = stack.pop().unwrap(); stack.last_mut().unwrap().content.push(XmlPart::Element(node));
        }
        Event::Text(t) => append_text(stack.last_mut().unwrap(), t.xml_content(XmlVersion::Implicit1_0).into_owned()),
        Event::CData(t) => append_text(stack.last_mut().unwrap(), t.xml_content(XmlVersion::Implicit1_0).into_owned()),
        Event::GeneralRef(t) => {
            let encoded = format!("&{};", t.as_ref());
            append_text(stack.last_mut().unwrap(), quick_xml::escape::unescape(&encoded).map_err(|_| "math_invalid_xml")?.into_owned());
        }
        Event::Comment(t) => stack.last_mut().unwrap().content.push(XmlPart::Comment(t.as_ref().into())),
        Event::PI(t) => stack.last_mut().unwrap().content.push(XmlPart::Instruction(t.as_ref().into())),
        Event::DocType(_) => return Err("math_unsafe_xml"),
        Event::Decl(_) => (),
        Event::Eof => break,
    } }
    if stack.len() != 1 { return Err("math_invalid_xml"); }
    let mut root = stack.pop().unwrap(); root.assign_ids("x".into()); Ok(root)
}

fn append_text(node: &mut XmlNode, text: String) {
    if let Some(XmlPart::Text(previous)) = node.content.last_mut() { previous.push_str(&text); }
    else { node.content.push(XmlPart::Text(text)); }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSlot { pub role: String, pub from: usize, pub to: usize }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceNode { pub id: String, pub kind: String, pub from: usize, pub to: usize, pub tex: String, pub slots: Vec<SourceSlot> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic { pub code: String, pub from: usize, pub to: usize }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathDocument {
    pub dialect: String, pub source: String, pub revision: String,
    pub root: XmlNode, pub nodes: Vec<SourceNode>, pub diagnostics: Vec<Diagnostic>,
}

pub(crate) fn group(source: &str, start: usize, open: char, close: char) -> Option<(usize, usize, usize)> {
    let from = start + source[start..].len() - source[start..].trim_start().len();
    if !source[from..].starts_with(open) { return None; }
    let body = from + open.len_utf8(); let mut depth = 1; let mut escaped = false;
    for (offset, ch) in source[body..].char_indices() {
        if escaped { escaped = false; continue; }
        if ch == '\\' { escaped = true; continue; }
        if ch == open { depth += 1; } else if ch == close { depth -= 1; if depth == 0 { return Some((body, body + offset, body + offset + ch.len_utf8())); } }
    }
    None
}
fn pos(s: &str, byte: usize) -> usize { s[..byte].encode_utf16().count() }

/// Source nodes include structure and argument spans, not just painted glyphs.
pub fn source_nodes(source: &str) -> Result<(Vec<SourceNode>, Vec<Diagnostic>), &'static str> {
    if source.len() > MAX_SOURCE { return Err("math_budget"); }
    let mut nodes = Vec::new(); let mut diagnostics = Vec::new(); let mut i = 0; let mut braces = Vec::new();
    while i < source.len() {
        let from = i; let ch = source[i..].chars().next().unwrap(); i += ch.len_utf8();
        if ch == '{' { braces.push(from); continue; }
        if ch == '}' { if braces.pop().is_none() { diagnostics.push(Diagnostic { code: "math_unbalanced".into(), from: pos(source,from), to: pos(source,i) }); } continue; }
        if ch.is_whitespace() || matches!(ch, '&' | '[' | ']') { continue; }
        let mut kind = "symbol".to_string(); let mut end = i; let mut slots = vec![];
        if ch == '\\' {
            while i < source.len() && source.as_bytes()[i].is_ascii_alphabetic() { i += 1; }
            if i == from + 1 && i < source.len() { i += source[i..].chars().next().unwrap().len_utf8(); }
            end = i; let command = &source[from..end];
            let roles: &[&str] = match command {
                "\\frac" | "\\dfrac" | "\\tfrac" | "\\binom" => { kind = "fraction".into(); &["num", "den"] },
                "\\sqrt" => { kind = "radical".into(); &["e"] },
                "\\overset" | "\\underset" => { kind = "limit".into(); &["lim", "e"] },
                "\\hat" | "\\widehat" | "\\vec" | "\\bar" | "\\overline" | "\\underline" | "\\dot" | "\\ddot" | "\\tilde" | "\\widetilde" | "\\overbrace" | "\\underbrace" | "\\boxed" | "\\phantom" | "\\hphantom" | "\\vphantom" => { kind = "structure".into(); &["e"] },
                "\\mathord" | "\\mathbin" | "\\mathrel" => { kind = "structure".into(); &["e"] },
                "\\mathrm" | "\\mathbf" | "\\mathit" | "\\mathbb" | "\\mathcal" | "\\mathscr" | "\\mathtt" | "\\mathsf" | "\\mathfrak" | "\\boldsymbol" | "\\text" | "\\operatorname" => { kind = "style".into(); &["e"] },
                "\\begin" | "\\end" => { kind = "environment".into(); &["name"] },
                _ => &[],
            };
            if command == "\\sqrt" { if let Some((a,b,c)) = group(source,end,'[',']') { slots.push(SourceSlot { role:"deg".into(),from:pos(source,a),to:pos(source,b) }); end=c; } }
            for role in roles { if let Some((a,b,c))=group(source,end,'{','}') { slots.push(SourceSlot { role:(*role).into(),from:pos(source,a),to:pos(source,b) }); end=c; } else { diagnostics.push(Diagnostic { code:"math_missing_argument".into(),from:pos(source,from),to:pos(source,end) }); break; } }
            if matches!(command,"\\mathord"|"\\mathbin"|"\\mathrel") && slots.is_empty() {
                return Err(if source[i..].trim_start().starts_with('{') { "math_invalid_source" } else { "math_unsupported_extension" });
            }
            if command == "\\mkern" {
                let kern = crate::latex2omml::math_kern(source, end)?;
                kind = "spacing".into(); end = kern.to; i = end;
                slots.push(SourceSlot { role:"width".into(),from:pos(source,kern.from),to:pos(source,kern.to) });
            }
            // These arguments are literal text in the existing compiler, so
            // apparent control sequences inside them are not math extensions.
            if matches!(command, "\\text" | "\\operatorname") { i = end; }
            // Recognize unsupported commands without converting them into words.
            let known = !roles.is_empty() || command == "\\mkern" || matches!(command,"\\textbackslash"|"\\textasciicircum"|"\\textasciitilde"|"\\$") || crate::math_catalog::symbol(command).is_some() || matches!(command,
                "\\left" | "\\right" | "\\middle" | "\\sum" | "\\prod" | "\\coprod" | "\\int" | "\\iint" | "\\iiint" | "\\oint" | "\\bigcap" | "\\bigcup" | "\\lim" | "\\sin" | "\\cos" | "\\tan" | "\\cot" | "\\sec" | "\\csc" | "\\arcsin" | "\\arccos" | "\\arctan" | "\\sinh" | "\\cosh" | "\\tanh" | "\\ln" | "\\log" | "\\lg" | "\\exp" | "\\det" | "\\dim" | "\\ker" | "\\deg" | "\\gcd" | "\\hom" | "\\inf" | "\\sup" | "\\max" | "\\min" | "\\quad" | "\\qquad" | "\\," | "\\;" | "\\!" | "\\:" | "\\ " | "\\\\" | "\\%" | "\\_" | "\\&" | "\\#" | "\\{" | "\\}" | "\\limits" | "\\nolimits");
            if !known { diagnostics.push(Diagnostic {code:"math_unknown_macro".into(),from:pos(source,from),to:pos(source,i)}); }
        } else if ch == '^' || ch == '_' {
            kind = if ch == '^' { "superscript" } else { "subscript" }.into();
            if let Some((a,b,c)) = group(source,end,'{','}') { slots.push(SourceSlot {role:"e".into(),from:pos(source,a),to:pos(source,b)}); end=c; }
            else if end < source.len() { let c=source[end..].chars().next().unwrap(); slots.push(SourceSlot {role:"e".into(),from:pos(source,end),to:pos(source,end+c.len_utf8())}); end+=c.len_utf8(); }
            else { diagnostics.push(Diagnostic {code:"math_missing_argument".into(),from:pos(source,from),to:pos(source,end)}); }
        } else if ch.is_ascii_digit() {
            while i < source.len() && (source.as_bytes()[i].is_ascii_digit() || source.as_bytes()[i] == b'.') { i+=1; } end=i; kind="number".into();
        }
        nodes.push(SourceNode {id:format!("s{}",nodes.len()),kind,from:pos(source,from),to:pos(source,end),tex:source[from..end].into(),slots});
    }
    for byte in braces { diagnostics.push(Diagnostic {code:"math_unbalanced".into(),from:pos(source,byte),to:pos(source,byte+1)}); }
    Ok((nodes,diagnostics))
}

pub fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(include_str!("../../../docs/design/office-math-coverage.json")).expect("checked-in Office Math schema"))
}

fn marked_root(doc:&MathDocument)->XmlNode{
    fn mark(node:&mut XmlNode){
        if !node.local().ends_with("Pr") && node.local()!="#root" && node.local()!="t" {
            node.attributes.insert("readmdId".into(),format!("rd-formula-token-omml-{}",node.id.replace('.',"-")));
        }
        for part in &mut node.content{if let XmlPart::Element(n)=part{mark(n);}}
    }
    let mut root=doc.root.clone();mark(&mut root);root
}

pub fn vector_preview(doc:&MathDocument,display:bool,size:f64)->Option<String>{
    vector_preview_constrained(doc,display,size,None)
}
pub fn vector_preview_constrained(doc:&MathDocument,display:bool,size:f64,width:Option<f64>)->Option<String>{
    vector_preview_with_diagnostics(doc,display,size,width).map(|result|result.svg)
}

#[derive(Debug,Clone,Serialize)]
pub struct VectorPreviewResult {
    pub svg:String,
    pub diagnostics:Vec<crate::math_equation_array::EquationArrayDiagnostic>,
}

/// The caller can retain a natural, unclipped preview and report exact array
/// overflow/unsupported source context instead of hiding it behind Option.
pub fn vector_preview_with_diagnostics(doc:&MathDocument,display:bool,size:f64,width:Option<f64>)->Option<VectorPreviewResult>{
    let root=marked_root(doc);
    crate::math_layout::layout_source_constrained(&crate::math_formats::tex(&doc.root),Some(&root.xml()),display,size,width)
        .map(|f|VectorPreviewResult{svg:f.interactive_svg(),diagnostics:f.equation_array_diagnostics().to_vec()})
}

pub fn preview(doc:&MathDocument,display:bool)->Result<String,&'static str>{
    preview_at_size(doc,display,12.0)
}
pub fn preview_at_size(doc:&MathDocument,display:bool,size:f64)->Result<String,&'static str>{
    let root=marked_root(doc);
    let parsed=crate::omml::parse(&root.xml()).map_err(|_|"math_invalid_xml")?;
    Ok(crate::omml::to_mathml_with_context(&parsed,display,&crate::math_formats::tex(&doc.root),size,None))
}

/// An explicit physical parent enables the bounded native-MathML array
/// profile. Source XML/IDs/revision remain the existing lossless document.
pub(crate) fn preview_constrained(doc:&MathDocument,display:bool,size:f64,width:Option<f64>)
    ->Result<crate::omml::MathmlProjection,&'static str>{
    let root=marked_root(doc);
    let parsed=crate::omml::parse(&root.xml()).map_err(|_|"math_invalid_xml")?;
    crate::omml::to_mathml_constrained(&parsed,display,&crate::math_formats::tex(&doc.root),size,width)
}

pub fn parse(source: &str, dialect: &str, display: bool) -> Result<MathDocument, &'static str> {
    let (root,nodes,diagnostics) = match dialect {
        "dsl" => { let (root, nodes) = crate::math_dsl::compile(source, display)?; (root, nodes, vec![]) },
        "latex" => {
            let (nodes,diagnostics)=source_nodes(source)?;
            (parse_xml(&crate::latex2omml::latex_to_omml_checked(source,display)?)?,nodes,diagnostics)
        },
        "unicodemath" => crate::unicode_math::compile(source, display)?,
        "mathml" => {let result=crate::math_formats::import_mathml(source)?;let diagnostics=result.diagnostics.into_iter().map(|code|Diagnostic{code,from:0,to:source.encode_utf16().count()}).collect();(result.root,result.nodes,diagnostics)},
        "omml" => { let root=parse_xml(source)?; if !root.children().any(|n| is_math(n)&&matches!(n.local(),"oMath"|"oMathPara"|"mathPr")) { return Err("math_invalid_xml"); } (root,vec![],vec![]) },
        _ => return Err("math_unsupported_dialect"),
    };
    Ok(MathDocument {dialect:dialect.into(),source:source.into(),revision:transaction::revision(source),root,nodes,diagnostics})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag="operation",rename_all="snake_case")]
pub enum MathPatch {
    ReplaceSource {node_id:String,text:String},
    ReplaceSlot {node_id:String,role:String,text:String},
    ReplaceText {node_id:String,text:String},
    ReplaceTextRange {node_id:String,from:usize,to:usize,text:String},
    ReplaceContent {node_id:String,latex:String},
    ReplaceDslContent {node_id:String,source:String},
    SetProperty {node_id:String,container:String,property:String,value:Option<String>},
    SetAttribute {node_id:String,name:String,value:Option<String>},
    MatrixAxis {node_id:String,axis:String,index:usize,insert:bool},
    MatrixColumnAlignment {node_id:String,index:usize,alignment:String},
    ConvertScript {node_id:String,kind:String},
}

fn element(name: &str) -> Option<&Value> { schema()["elements"].as_array()?.iter().find(|e| e["element"] == name) }
fn allowed_child(parent: &str,child: &str)->bool { element(parent).and_then(|e|e["children"].as_array()).is_some_and(|a|a.iter().any(|v|v==child)) }
/// Documented Office legacy defaults, separate from the unchanged SDK schema.
pub(crate) fn office_implicit_value(property:&str,attribute:&str)->Option<&'static str>{
    (attribute=="m:val"&&matches!(property,"lMargin"|"rMargin"|"wrapIndent")).then_some("1440")
}
pub(crate) fn validate_value(property:&str,value:&str)->Result<(),&'static str> {
    if value.len()>4096 || value.chars().any(|c| c=='\0' || c=='\u{b}' || c=='\u{c}') {return Err("math_invalid_property");}
    let Some(el)=element(property) else {return Err("math_invalid_property");};
    if let Some(attrs)=el["attributes"].as_array() { for attr in attrs {
        if attr["name"] != "m:val" {continue;}
        let typ=attr["type"].as_str().unwrap_or("");
        if let Some(name)=typ.strip_prefix("EnumValue<DocumentFormat.OpenXml.Math.").and_then(|s|s.strip_suffix('>')) {
            let enumeration=schema()["enumerations"].as_array().and_then(|a|a.iter().find(|e|e["name"]==name)).ok_or("math_invalid_property")?;
            if !enumeration["facets"].as_array().is_some_and(|a|a.iter().any(|f|f["Value"]==value)) {return Err("math_invalid_property");}
        } else if typ.contains("Int") && value.parse::<i64>().is_err() { return Err("math_invalid_property"); }
        validate_constraints(attr,value)?;
    } }
    Ok(())
}

pub(crate) fn validate_constraints(attr:&Value,value:&str)->Result<(),&'static str>{
    let integer=value.parse::<i64>().ok();
    let typ=attr["type"].as_str().unwrap_or("");
    if typ=="IntegerValue"&&!integer.is_some_and(|v|i32::try_from(v).is_ok()){return Err("math_invalid_property");}
    if typ=="UInt16Value"&&!integer.is_some_and(|v|u16::try_from(v).is_ok()){return Err("math_invalid_property");}
    if typ=="UInt32Value"&&!integer.is_some_and(|v|u32::try_from(v).is_ok()){return Err("math_invalid_property");}
    if typ=="EnumValue<DocumentFormat.OpenXml.SpaceProcessingModeValues>"&&!matches!(value,"default"|"preserve"){return Err("math_invalid_property");}
    if typ.starts_with("UInt")&&!integer.is_some_and(|v|v>=0){return Err("math_invalid_property");}
    for validator in attr["validators"].as_array().into_iter().flatten(){
        for argument in validator["Arguments"].as_array().into_iter().flatten(){
            let n=argument["Value"].as_str().and_then(|v|v.parse::<i64>().ok()).unwrap_or(0);
            let good=match argument["Name"].as_str().unwrap_or(""){
                "MinInclusive"=>integer.is_some_and(|v|v>=n),"MaxInclusive"=>integer.is_some_and(|v|v<=n),
                "MinLength"=>value.encode_utf16().count()>=n as usize,"MaxLength"=>value.encode_utf16().count()<=n as usize,
                _=>true,
            };if !good{return Err("math_invalid_property");}
        }
    }Ok(())
}

fn blank_slot(prefix:&str)->XmlNode {XmlNode::new(&format!("{prefix}:e"))}
fn is_math(node:&XmlNode)->bool{node.namespace==MATH_NS||node.namespace.is_empty()}
fn math_prefix(node:&mut XmlNode)->String{
    if let Some((prefix,_))=node.name.split_once(':'){return prefix.into();}
    if let Some((key,_))=node.attributes.iter().find(|(key,value)|key.starts_with("xmlns:")&&*value==MATH_NS){return key[6..].into();}
    let source=node.xml();let mut index=0;
    loop{
        let prefix=if index==0{"readmdMath".into()}else{format!("readmdMath{index}")};
        if !source.contains(&format!("{prefix}:"))&&!source.contains(&format!("xmlns:{prefix}")){
            node.attributes.insert(format!("xmlns:{prefix}"),MATH_NS.into());return prefix;
        }
        index+=1;
    }
}
fn ensure_child<'a>(parent:&'a mut XmlNode,name:&str,prefix:&str)->&'a mut XmlNode{
    let index=parent.content.iter().position(|p|matches!(p,XmlPart::Element(n) if n.local()==name&&is_math(n))).unwrap_or_else(||{
        parent.content.push(XmlPart::Element(XmlNode::new(&format!("{prefix}:{name}"))));parent.content.len()-1
    });
    if let XmlPart::Element(node)=&mut parent.content[index]{node}else{unreachable!()}
}
fn matrix_column_alignment(node:&mut XmlNode,index:usize,alignment:&str)->Result<(),&'static str>{
    if node.local()!="m"||!is_math(node){return Err("math_invalid_property");}validate_value("mcJc",alignment)?;
    let rows:Vec<_>=node.children().filter(|n|n.local()=="mr").collect();
    let columns=rows.first().map(|n|n.children().filter(|n|n.local()=="e").count()).unwrap_or(0);
    if columns>64{return Err("math_budget");}
    if index>=columns||rows.iter().any(|r|r.children().filter(|n|n.local()=="e").count()!=columns){return Err("math_irregular_matrix");}
    let prefix=math_prefix(node);
    let properties=ensure_child(node,"mPr",&prefix);let groups=ensure_child(properties,"mcs",&prefix);
    let mut expanded=Vec::new();
    for column in groups.children().filter(|n|n.local()=="mc"){
        let count=column.children().find(|n|n.local()=="mcPr").and_then(|p|p.children().find(|n|n.local()=="count"))
            .and_then(|n|n.attributes.iter().find(|(k,_)|k.rsplit(':').next()==Some("val"))).and_then(|(_,v)|v.parse::<usize>().ok()).unwrap_or(1);
        if count==0||count>columns||expanded.len()+count>columns{return Err("math_irregular_matrix");}
        expanded.extend(std::iter::repeat_n(column.clone(),count));
    }
    if !expanded.is_empty()&&expanded.len()!=columns{return Err("math_irregular_matrix");}
    expanded.resize_with(columns,||XmlNode::new(&format!("{prefix}:mc")));
    for (at,column) in expanded.iter_mut().enumerate(){
        let pr=ensure_child(column,"mcPr",&prefix);
        ensure_child(pr,"count",&prefix).attributes.insert(format!("{prefix}:val"),"1".into());
        if at==index{ensure_child(pr,"mcJc",&prefix).attributes.insert(format!("{prefix}:val"),alignment.into());}
        pr.content.sort_by_key(|p|match p{XmlPart::Element(n) if n.local()=="count"=>0,XmlPart::Element(n) if n.local()=="mcJc"=>1,_=>2});
    }
    let extensions:Vec<_>=groups.content.iter().filter(|p|!matches!(p,XmlPart::Element(n) if n.local()=="mc")).cloned().collect();
    groups.content=expanded.into_iter().map(XmlPart::Element).chain(extensions).collect();
    if let Some(order)=element("mPr").and_then(|e|e["children"].as_array()){
        properties.content.sort_by_key(|p|if let XmlPart::Element(n)=p{order.iter().position(|v|v==n.local()).unwrap_or(usize::MAX)}else{usize::MAX});
    }
    // mPr must precede all matrix rows, including when first created here.
    node.content.sort_by_key(|p|if matches!(p,XmlPart::Element(n) if n.local()=="mPr"){0}else{1});
    Ok(())
}
// Generated fragments use m:. Imported equations may use another prefix, or a
// default namespace. Bind the new fragment without changing imported siblings.
fn bind_fragment(parts:&mut [XmlPart],prefix:&str){
    for part in parts{if let XmlPart::Element(node)=part{
        if let Some(local)=node.name.strip_prefix("m:"){node.name=format!("{prefix}:{local}");}
        node.attributes=std::mem::take(&mut node.attributes).into_iter().map(|(key,value)|{
            (if let Some(local)=key.strip_prefix("m:"){format!("{prefix}:{local}")}else{key},value)
        }).collect();
        bind_fragment(&mut node.content,prefix);
    }}
}
fn inherit_run_style(parts:&mut [XmlPart],original:&XmlNode,prefix:&str){
    for part in parts{if let XmlPart::Element(node)=part{
        if node.name==format!("{prefix}:r"){
            for (key,value) in &original.attributes{node.attributes.entry(key.clone()).or_insert_with(||value.clone());}
            for properties in original.children().filter(|n|n.local()=="rPr"){
                let existing=node.content.iter_mut().find_map(|part|if let XmlPart::Element(n)=part{(n.local()=="rPr"&&n.namespace==properties.namespace).then_some(n)}else{None});
                if let Some(existing)=existing{
                    for(key,value)in &properties.attributes{existing.attributes.entry(key.clone()).or_insert_with(||value.clone());}
                    for property in properties.children(){
                        let conflict=is_math(properties)&&((property.local()=="nor"&&existing.children().any(|n|matches!(n.local(),"scr"|"sty")))||(matches!(property.local(),"scr"|"sty")&&existing.children().any(|n|n.local()=="nor")));
                        if !conflict&&!existing.children().any(|n|n.local()==property.local()&&n.namespace==property.namespace){existing.content.push(XmlPart::Element(property.clone()));}
                    }
                    if is_math(properties){let order=["lit","nor","scr","sty","brk","aln"];existing.content.sort_by_key(|part|if let XmlPart::Element(n)=part{order.iter().position(|v|*v==n.local()).unwrap_or(usize::MAX)}else{usize::MAX});}
                }else{node.content.push(XmlPart::Element(properties.clone()));}
            }
            node.content.sort_by_key(|part|match part{XmlPart::Element(n) if n.name==format!("{prefix}:rPr")=>0,XmlPart::Element(n) if n.local()=="rPr"=>1,_=>2});
        }else{inherit_run_style(&mut node.content,original,prefix);}
    }}
}
fn replace_run(parent:&mut XmlNode,id:&str,replacement:&[XmlPart],fallback_prefix:&str)->Result<bool,&'static str>{
    for i in 0..parent.content.len(){
        let XmlPart::Element(child)=&mut parent.content[i] else{continue;};
        if child.id==id{
            if child.local()!="r"||!is_math(child){return Err("math_invalid_property");}
            let original=child.clone();let prefix=original.name.split_once(':').map(|p|p.0).unwrap_or(fallback_prefix);
            let mut content=replacement.to_vec();bind_fragment(&mut content,prefix);inherit_run_style(&mut content,&original,prefix);
            for part in &mut content{if let XmlPart::Element(n)=part{
                // The original run can itself carry namespace declarations.
                for (key,value) in original.attributes.iter().filter(|(k,_)|*k=="xmlns"||k.starts_with("xmlns:")){n.attributes.entry(key.clone()).or_insert_with(||value.clone());}
                n.attributes.insert(format!("xmlns:{prefix}"),MATH_NS.into());
            }}
            parent.content.splice(i..=i,content);return Ok(true);
        }
        if replace_run(child,id,replacement,fallback_prefix)?{return Ok(true);}
    }Ok(false)
}
fn matrix_axis(node:&mut XmlNode,axis:&str,index:usize,insert:bool)->Result<(),&'static str>{
    if node.local()!="m" || !is_math(node) || !matches!(axis,"row"|"column"){return Err("math_invalid_property");}
    let prefix=math_prefix(node);
    let rows:Vec<_>=node.content.iter().enumerate().filter_map(|(i,p)|if matches!(p,XmlPart::Element(e) if e.local()=="mr"){Some(i)}else{None}).collect();
    let columns=rows.first().and_then(|i|if let XmlPart::Element(r)=&node.content[*i]{Some(r.children().filter(|c|c.local()=="e").count())}else{None}).unwrap_or(0);
    let count=if axis=="row"{rows.len()}else{columns};
    let maximum=if axis=="row"{256}else{64};
    if columns>64 || rows.len()>256 || index>count || (!insert&&index==count) || (!insert&&count<=1) || (insert&&count>=maximum){return Err("math_invalid_property");}
    if axis=="row" {
        if insert {let mut row=XmlNode::new(&format!("{prefix}:mr"));row.content=(0..columns.max(1)).map(|_|XmlPart::Element(blank_slot(&prefix))).collect();let at=rows.get(index).copied().unwrap_or(node.content.len());node.content.insert(at,XmlPart::Element(row));}
        else{node.content.remove(rows[index]);}
    }else{
        for row_index in rows {if let XmlPart::Element(row)=&mut node.content[row_index]{let cells:Vec<_>=row.content.iter().enumerate().filter_map(|(i,p)|if matches!(p,XmlPart::Element(e) if e.local()=="e"){Some(i)}else{None}).collect();if cells.len()!=columns{return Err("math_irregular_matrix");}if insert{let at=cells.get(index).copied().unwrap_or(row.content.len());row.content.insert(at,XmlPart::Element(blank_slot(&prefix)));}else{row.content.remove(cells[index]);}}}
        // Expand repeated column groups before inserting/removing a column.
        // Existing alignment and width properties stay with their own columns.
        if let Some(properties)=node.content.iter_mut().find_map(|p|if let XmlPart::Element(e)=p{if e.local()=="mPr"{Some(e)}else{None}}else{None}){
            if let Some(groups)=properties.content.iter_mut().find_map(|p|if let XmlPart::Element(e)=p{if e.local()=="mcs"{Some(e)}else{None}}else{None}){
                let mut expanded=Vec::new();
                for column in groups.children().filter(|n|n.local()=="mc"){
                    let count=column.children().find(|n|n.local()=="mcPr").and_then(|pr|pr.children().find(|n|n.local()=="count")).and_then(|c|c.attributes.values().next()).and_then(|s|s.parse::<usize>().ok()).unwrap_or(1);
                    if count>64||expanded.len()+count>64{return Err("math_budget");}
                    for _ in 0..count{let mut c=column.clone();if let Some(pr)=c.content.iter_mut().find_map(|p|if let XmlPart::Element(e)=p{if e.local()=="mcPr"{Some(e)}else{None}}else{None}){pr.content.retain(|p|!matches!(p,XmlPart::Element(e) if e.local()=="count"));let mut one=XmlNode::new(&format!("{prefix}:count"));one.attributes.insert(format!("{prefix}:val"),"1".into());pr.content.insert(0,XmlPart::Element(one));}expanded.push(c);}
                }
                if expanded.len()==columns {
                    if insert{let new=expanded.get(index.min(columns.saturating_sub(1))).cloned().unwrap_or_else(||XmlNode::new(&format!("{prefix}:mc")));expanded.insert(index,new);}else{expanded.remove(index);}
                    let extensions:Vec<_>=groups.content.iter().filter(|p|!matches!(p,XmlPart::Element(e) if e.local()=="mc")).cloned().collect();
                    groups.content=expanded.into_iter().map(XmlPart::Element).chain(extensions).collect();
                }else{return Err("math_irregular_matrix");}
            }
        }
    }
    Ok(())
}

pub fn patch(source:&str,dialect:&str,display:bool,expected:&str,operation:&MathPatch)->Result<MathDocument,&'static str> {
    if transaction::revision(source)!=expected {return Err("transaction_conflict");}
    let mut doc=parse(source,dialect,display)?;
    if dialect == "dsl" {
        if let MathPatch::SetProperty { node_id, container, property, value: Some(value) } = operation {
            if let Some(target) = doc.nodes.iter().find(|n| n.id == *node_id) {
                if let Some(rewritten) = crate::math_dsl::rewrite_property(source, target, container, property, value)? { return parse(&rewritten, dialect, display); }
            }
        }
    }
    match operation {
        MathPatch::ReplaceContent{node_id,latex}|MathPatch::ReplaceDslContent{node_id,source:latex}=>{
            let replacement_dialect=if matches!(operation,MathPatch::ReplaceDslContent{..}){"dsl"}else{"latex"};
            let replacement=parse(latex,replacement_dialect,false)?;
            if !replacement.diagnostics.is_empty(){return Err("math_invalid_source");}
            let math=replacement.root.children().next().ok_or("math_invalid_source")?;
            if replacement.root.children().count()!=1||!is_math(math)||math.local()!="oMath"{return Err("math_invalid_source");}
            let mut content=math.content.clone();
            let prefix=format!("readmdMath_{}",&doc.revision[..16]);
            if source.contains(&format!("{prefix}:"))||source.contains(&format!("xmlns:{prefix}")){return Err("math_budget");}
            if doc.root.find_mut(node_id).ok_or("math_node_missing")?.local()=="r"{
                if !replace_run(&mut doc.root,node_id,&content,&prefix)?{return Err("math_node_missing");}
                return finish_patch(doc, source, dialect, display, operation);
            }
            let node=doc.root.find_mut(node_id).ok_or("math_node_missing")?;
            if !is_math(node)||!matches!(node.local(),"num"|"den"|"e"|"deg"|"sub"|"sup"|"lim"|"fName"){return Err("math_invalid_property");}
            let prefix=node.name.split_once(':').map(|p|p.0).unwrap_or(&prefix).to_string();bind_fragment(&mut content,&prefix);
            // A visual replacement of one formatted run keeps that run's
            // format. Mixed runs/structures have no single format to inherit;
            // never copy an arbitrary first run over a mixed selection.
            let originals=node.children().filter(|n|!n.local().ends_with("Pr")).collect::<Vec<_>>();
            if originals.len()==1&&is_math(originals[0])&&originals[0].local()=="r"{inherit_run_style(&mut content,originals[0],&prefix);}
            for part in &mut content{if let XmlPart::Element(n)=part{n.attributes.insert(format!("xmlns:{prefix}"),MATH_NS.into());}}
            node.content.retain(|p|matches!(p,XmlPart::Element(e) if e.local().ends_with("Pr")));
            node.content.extend(content);
        },
        MathPatch::MatrixAxis{node_id,axis,index,insert}=>matrix_axis(doc.root.find_mut(node_id).ok_or("math_node_missing")?,axis,*index,*insert)?,
        MathPatch::MatrixColumnAlignment{node_id,index,alignment}=>matrix_column_alignment(doc.root.find_mut(node_id).ok_or("math_node_missing")?,*index,alignment)?,
        MathPatch::SetAttribute{node_id,name,value}=>{
            let node=doc.root.find_mut(node_id).ok_or("math_node_missing")?;
            if crate::math_spacing::is_spacing(node){
                if !matches!(name.as_str(),"width"|"height"|"depth"|"mathsize"){return Err("math_invalid_property");}
                if let Some(value)=value{node.attributes.insert(name.clone(),value.clone());}else{node.attributes.remove(name);}
                let spacing=crate::math_spacing::Spacing::from_xml(node)?;
                if dialect=="mathml"{
                    // Replace only the source-owned element. Untouched XML,
                    // comments, siblings and lexical dimension units remain.
                    let target=doc.nodes.iter().find(|target|target.id==*node_id).ok_or("math_invalid_source")?;
                    let result=transaction::apply(source,&ContentTransaction{base_revision:expected.into(),origin:"formula".into(),edits:vec![TextEdit{from:target.from,to:target.to,insert:spacing.node().xml()}]})?;
                    return parse(&result.content,"mathml",display);
                }
                return finish_patch(doc,source,dialect,display,operation);
            }
            if !is_math(node){return Err("math_invalid_property");}
            let definition=element(node.local()).ok_or("math_invalid_property")?;
            let attr=definition["attributes"].as_array().and_then(|attrs|attrs.iter().find(|a|a["name"]==*name)).ok_or("math_invalid_property")?;
            if value.is_none()&&attr["required"]==true&&office_implicit_value(node.local(),name).is_none(){return Err("math_invalid_property");}
            if let Some(value)=value {
                if name=="m:val"{validate_value(node.local(),value)?;}else {validate_constraints(attr,value)?;}
                if value.chars().any(|c|(c as u32)<32){return Err("math_invalid_property");}
            }
            let actual=if let Some(suffix)=name.strip_prefix("m:"){format!("{}:{suffix}",math_prefix(node))}else{name.clone()};
            node.attributes.retain(|key,_|{
                if key==&actual{return false;}
                let Some(suffix)=name.strip_prefix("m:")else{return true;};
                let Some((_,local))=key.split_once(':')else{return true;};
                !(local==suffix&&node.attribute_namespaces.get(key).is_some_and(|uri|uri==MATH_NS))
            });
            node.attribute_namespaces.retain(|key,_|node.attributes.contains_key(key));
            if let Some(value)=value{
                // A newly added attribute has no namespace entry to inherit.
                // Keep the namespace model in sync so DSL decompilation treats
                // it as a standard Office attribute, including an empty switch.
                if name.starts_with("m:"){node.attribute_namespaces.insert(actual.clone(),MATH_NS.into());}
                node.attributes.insert(actual,value.clone());
            }
        },
        MathPatch::ConvertScript{node_id,kind}=>{
            if !matches!(kind.as_str(),"sSup"|"sSub"|"sSubSup"|"sPre"){return Err("math_invalid_property");}
            let node=doc.root.find_mut(node_id).ok_or("math_node_missing")?;
            if !is_math(node)||!matches!(node.local(),"sSup"|"sSub"|"sSubSup"|"sPre"){return Err("math_invalid_property");}
            let prefix=math_prefix(node);
            let old_pr=format!("{}Pr",node.local());let new_pr=format!("{kind}Pr");node.name=format!("{prefix}:{kind}");
            for part in &mut node.content{if let XmlPart::Element(n)=part{if n.local()==old_pr{
                // Never rename an incompatible property into invalid Office
                // XML. The user can first clear that property in this draft.
                if n.children().any(|c|!allowed_child(&new_pr,c.local())){return Err("math_invalid_property");}
                n.name=format!("{prefix}:{new_pr}");
            }}}
            let roles: &[&str]=match kind.as_str(){"sSup"=>&["e","sup"],"sSub"=>&["e","sub"],"sPre"=>&["sub","sup","e"],_=>&["e","sub","sup"]};
            node.content.retain(|p|!matches!(p,XmlPart::Element(n) if matches!(n.local(),"sub"|"sup")&&!roles.contains(&n.local())));
            for role in roles{if !node.children().any(|n|n.local()==*role){node.content.push(XmlPart::Element(XmlNode::new(&format!("{prefix}:{role}"))));}}
            node.content.sort_by_key(|part|match part{
                XmlPart::Element(n) if n.local()==new_pr=>0,
                XmlPart::Element(n)=>roles.iter().position(|r|*r==n.local()).map(|i|i+1).unwrap_or(usize::MAX),
                _=>usize::MAX,
            });
        },
        MathPatch::ReplaceSource {node_id,text} | MathPatch::ReplaceSlot {node_id,text,..} => {
            let node=doc.nodes.iter().find(|n|n.id==*node_id).ok_or("math_node_missing")?;
            if dialect=="dsl"&&matches!(operation,MathPatch::ReplaceSource{..}){crate::math_dsl::validate_source_replacement(node,text)?;}
            let (from,to)=if let MathPatch::ReplaceSlot {role,..}=operation {let slot=node.slots.iter().find(|s|s.role==*role).ok_or("math_node_missing")?;(slot.from,slot.to)} else {(node.from,node.to)};
            let insert=if dialect=="unicodemath"&&matches!(operation,MathPatch::ReplaceSlot{..})&&node.kind!="nary" {
                crate::unicode_math::slot_replacement(source,from,to,text)?
            }else{text.clone()};
            let result=transaction::apply(source,&ContentTransaction {base_revision:expected.into(),origin:"formula".into(),edits:vec![TextEdit {from,to,insert}]})?;
            return parse(&result.content,dialect,display);
        },
        MathPatch::ReplaceText {node_id,text} => {
            let node=doc.root.find_mut(node_id).ok_or("math_node_missing")?;
            if !is_math(node)||!matches!(node.local(),"t"|"delText") || text.len()>MAX_SOURCE {return Err("math_invalid_property");}
            node.content=vec![XmlPart::Text(text.clone())];
        },
        MathPatch::ReplaceTextRange {node_id,from,to,text} => {
            let node=doc.root.find_mut(node_id).ok_or("math_node_missing")?;
            if !is_math(node)||!matches!(node.local(),"t"|"delText")||text.len()>MAX_SOURCE||from>to||node.children().next().is_some(){return Err("math_invalid_property");}
            let original=node.text();
            let start=transaction::byte_offset(&original,*from)?;
            let end=transaction::byte_offset(&original,*to)?;
            if original.len()-(end-start)+text.len()>MAX_SOURCE{return Err("math_budget");}
            let mut offset=0;let mut inserted=false;let mut content=Vec::new();
            for part in &node.content {match part {
                XmlPart::Text(value)=>{
                    let next=offset+value.len();let a=start.saturating_sub(offset).min(value.len());let b=end.saturating_sub(offset).min(value.len());
                    let mut result=value[..a].to_string();
                    if !inserted&&offset<=start&&start<=next{result.push_str(text);inserted=true;}
                    result.push_str(&value[b..]);if !result.is_empty(){content.push(XmlPart::Text(result));}offset=next;
                },
                other=>content.push(other.clone()),
            }}
            if !inserted&&!text.is_empty(){content.push(XmlPart::Text(text.clone()));}
            node.content=content;
        },
        MathPatch::SetProperty {node_id,container,property,value} => {
            let node=doc.root.find_mut(node_id).ok_or("math_node_missing")?;
            if !is_math(node){return Err("math_invalid_property");}
            if !allowed_child(node.local(),container) || !allowed_child(container,property) {return Err("math_invalid_property");}
            if let Some(value)=value {validate_value(property,value)?;}
            let prefix=math_prefix(node);
            let qualified=format!("{prefix}:{container}");
            let index=node.content.iter().position(|p|matches!(p,XmlPart::Element(e) if e.local()==container&&is_math(e)));
            let index=match index {Some(i)=>i,None=>{node.content.insert(0,XmlPart::Element(XmlNode::new(&qualified)));0}};
            if let XmlPart::Element(properties)=&mut node.content[index] {
                let qualified=format!("{prefix}:{property}");
                properties.content.retain(|p|!matches!(p,XmlPart::Element(e) if e.local()==property&&is_math(e)));
                if let Some(value)=value {
                    // CT_RPr chooses either normal text or mathematical
                    // script/style; even an explicit nor=0 is in that choice.
                    if container=="rPr"{
                        properties.content.retain(|p|!matches!(p,XmlPart::Element(e) if
                            (property=="nor"&&matches!(e.local(),"scr"|"sty"))||
                            (matches!(property.as_str(),"scr"|"sty")&&e.local()=="nor")));
                    }
                    let mut p=XmlNode::new(&qualified);p.namespace=MATH_NS.into();
                    let attribute=format!("{prefix}:val");p.attribute_namespaces.insert(attribute.clone(),MATH_NS.into());
                    p.attributes.insert(attribute,value.clone());properties.content.push(XmlPart::Element(p));
                }
                if let Some(order)=element(container).and_then(|e|e["children"].as_array()){
                    properties.content.sort_by_key(|part|if let XmlPart::Element(n)=part{order.iter().position(|v|v==n.local()).unwrap_or(usize::MAX)}else{usize::MAX});
                }
            }
        },
    }
    finish_patch(doc, source, dialect, display, operation)
}

pub(crate) fn xml_semantics(node: &XmlNode) -> Value {
    let mut attrs = BTreeMap::new();
    for (name, value) in &node.attributes {
        if name == "xmlns" || name.starts_with("xmlns:") { continue; }
        let ns = node.attribute_namespaces.get(name).map(String::as_str).unwrap_or_else(|| if name.starts_with("xml:") { "http://www.w3.org/XML/1998/namespace" } else if name.contains(':') { MATH_NS } else { "" });
        attrs.insert(format!("{ns}:{}", name.rsplit(':').next().unwrap_or(name)), value);
    }
    serde_json::json!([if node.namespace.is_empty(){MATH_NS}else{&node.namespace}, node.local(), attrs, node.content.iter().map(|part| match part {
        XmlPart::Element(child) => xml_semantics(child), XmlPart::Text(text) => serde_json::json!({"text":text}),
        XmlPart::Comment(text) => serde_json::json!({"comment":text}), XmlPart::Instruction(text) => serde_json::json!({"instruction":text}),
    }).collect::<Vec<_>>()])
}
fn finish_patch(doc: MathDocument, source: &str, dialect: &str, display: bool, operation: &MathPatch) -> Result<MathDocument, &'static str> {
    // Advanced edits stay readable for DSL documents. Legacy TeX continues to
    // retain its Office properties through the existing lossless adapter.
    if dialect == "unicodemath" || dialect=="mathml"&&crate::math_spacing::contains_spacing(&doc.root) {
        return parse(&crate::math_migration::readable(&doc.root.xml())?, "dsl", display);
    }
    if dialect != "dsl" { return parse(&doc.root.xml(), "omml", display); }
    let id = match operation {
        MathPatch::ReplaceSource {node_id,..} | MathPatch::ReplaceSlot {node_id,..}
        | MathPatch::ReplaceText {node_id,..} | MathPatch::ReplaceTextRange {node_id,..} | MathPatch::ReplaceContent {node_id,..} | MathPatch::ReplaceDslContent {node_id,..}
        | MathPatch::SetProperty {node_id,..} | MathPatch::SetAttribute {node_id,..}
        | MathPatch::MatrixAxis {node_id,..} | MathPatch::MatrixColumnAlignment {node_id,..}
        | MathPatch::ConvertScript {node_id,..} => node_id,
    };
    // Recompile only the smallest independently expressible changed subtree.
    // Outer comments and the exact spelling of untouched siblings survive.
    let mut candidates = doc.nodes.iter().filter(|node| node.id == *id || id.starts_with(&format!("{}.", node.id)))
        .filter(|node| !matches!(node.kind.as_str(), "num"|"den"|"e"|"deg"|"sub"|"sup"|"lim"|"fName"|"t"|"delText"))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|node| std::cmp::Reverse(node.id.len()));
    let expected = xml_semantics(&doc.root);
    for target in candidates {
        let Some(changed) = doc.root.find(&target.id) else { continue; };
        let Ok(fragment) = crate::math_dsl::decompile(changed) else { continue; };
        let from = transaction::byte_offset(source, target.from)?;
        let to = transaction::byte_offset(source, target.to)?;
        let mut candidate = source.to_string(); candidate.replace_range(from..to, &fragment);
        if let Ok(parsed) = parse(&candidate, "dsl", display) {
            if xml_semantics(&parsed.root) == expected { return Ok(parsed); }
        }
    }
    // No mapped structure can represent this operation without changing other
    // source. Keep the current draft rather than silently normalize the file.
    Err("math_invalid_source")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathMetadata {pub schema:u8,pub id:String,pub source_revision:String,pub omml:String}
pub fn metadata(tex:&str,omml:&str,id:&str)->Result<String,&'static str> {
    parse(omml,"omml",false)?;
    if id.len()>128 || !id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_') {return Err("math_metadata_invalid");}
    let value=MathMetadata {schema:1,id:id.into(),source_revision:transaction::revision(tex),omml:omml.into()};
    let bytes=serde_json::to_vec(&value).map_err(|_|"math_metadata_invalid")?;
    let mut encoder=flate2::write::GzEncoder::new(Vec::new(),flate2::Compression::fast());
    encoder.write_all(&bytes).map_err(|_|"math_metadata_invalid")?;
    Ok(format!("<!--readmd:math:v1:{}-->",STANDARD.encode(encoder.finish().map_err(|_|"math_metadata_invalid")?)))
}
pub fn read_metadata(comment:&str,tex:&str)->Result<(MathMetadata,bool),&'static str> {
    let data=comment.strip_prefix("<!--readmd:math:v1:").and_then(|s|s.strip_suffix("-->" )).ok_or("math_metadata_invalid")?;
    if data.len()>MAX_XML*2 {return Err("math_budget");}
    let bytes=STANDARD.decode(data).map_err(|_|"math_metadata_invalid")?;
    let mut decoded=Vec::new(); flate2::read::GzDecoder::new(bytes.as_slice()).take((MAX_XML+1) as u64).read_to_end(&mut decoded).map_err(|_|"math_metadata_invalid")?;
    if decoded.len()>MAX_XML {return Err("math_budget");}
    let value:MathMetadata=serde_json::from_slice(&decoded).map_err(|_|"math_metadata_invalid")?;
    if value.schema!=1 {return Err("math_metadata_invalid");}
    parse(&value.omml,"omml",false)?;
    let current=value.source_revision==transaction::revision(tex); Ok((value,current))
}

#[cfg(test)]
mod tests {
    #[test]
    fn math_text_ranges_preserve_run_properties_comments_and_utf16_identity() {
        use super::*;
        let xml=format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><m:rPr><m:nor m:val="0"/></m:rPr><w:rPr><w:color w:val="336699"/><w:b w:val="0"/></w:rPr><m:t xml:space="preserve">A𝛂<!-- author -->BA</m:t></m:r></m:oMath>"#);
        let original=parse(&xml,"omml",false).unwrap();let id=original.root.children().next().unwrap().children().next().unwrap().children().find(|n|n.local()=="t").unwrap().id.clone();
        let changed=patch(&xml,"omml",false,&original.revision,&MathPatch::ReplaceTextRange{node_id:id.clone(),from:1,to:4,text:"θ".into()}).unwrap();
        assert_eq!(changed.root.find(&id).unwrap().text(),"AθA");
        for value in ["<!-- author -->","w:val=\"336699\"","w:val=\"0\"","m:val=\"0\"","xml:space=\"preserve\""]{assert!(changed.source.contains(value),"{value}");}
        assert!(patch(&xml,"omml",false,&original.revision,&MathPatch::ReplaceTextRange{node_id:id.clone(),from:2,to:3,text:"x".into()}).is_err());
        assert!(patch(&xml,"omml",false,&original.revision,&MathPatch::ReplaceTextRange{node_id:id.clone(),from:4,to:1,text:"x".into()}).is_err());
        assert_eq!(patch(&xml,"omml",false,"stale",&MathPatch::ReplaceTextRange{node_id:id,from:0,to:1,text:"x".into()}).unwrap_err(),"transaction_conflict");
    }

    #[test]
    fn math_text_ranges_rewrite_only_selected_dsl_run_and_support_empty_text() {
        use super::*;
        let run=r#"office.r(word.rPr(word.color(val: "336699")), office.t("ABA𝛂Aé"))"#;
        let source=format!("// 😀 Keep author spelling.\nrow({run},  frac( x , y ), {run})");
        let original=parse(&source,"dsl",false).unwrap();let text=original.nodes.iter().filter(|n|n.kind=="t").last().unwrap();
        let changed=patch(&source,"dsl",false,&original.revision,&MathPatch::ReplaceTextRange{node_id:text.id.clone(),from:2,to:3,text:"β + γ".into()}).unwrap();
        assert!(changed.source.starts_with(&format!("// 😀 Keep author spelling.\nrow({run},  frac( x , y ), ")));
        assert_eq!(changed.root.find(&text.id).unwrap().text(),"ABβ + γ𝛂Aé");assert_eq!(changed.source.matches("336699").count(),2);
        let one="office.r(office.t(\"A\"))";let doc=parse(one,"dsl",false).unwrap();let id=doc.nodes.iter().find(|n|n.kind=="t").unwrap().id.clone();
        let empty=patch(one,"dsl",false,&doc.revision,&MathPatch::ReplaceTextRange{node_id:id,from:0,to:1,text:String::new()}).unwrap();
        assert!(empty.root.text().is_empty());
    }

    #[test]
    fn math_visual_dsl_slot_replacement_keeps_explicit_run_format_and_other_source() {
        use super::*;
        let source="// author 🧮\nrow(frac(a, b), frac(office.r(word.rPr(word.color(val: \"336699\"), word.sz(val: \"32\")), office.t(\"a\")), b))";
        let original=parse(source,"dsl",true).unwrap();
        let num=original.nodes.iter().filter(|n|n.kind=="num").nth(1).unwrap().id.clone();
        let changed=patch(source,"dsl",true,&original.revision,&MathPatch::ReplaceDslContent{node_id:num,source:"γ^2".into()}).unwrap();
        assert!(changed.source.starts_with("// author 🧮\nrow(frac(a, b), "));
        let numerator=changed.nodes.iter().filter(|n|n.kind=="num").nth(1).unwrap();
        let slot=changed.root.find(&numerator.id).unwrap();
        fn runs<'a>(n:&'a XmlNode,out:&mut Vec<&'a XmlNode>){if is_math(n)&&n.local()=="r"{out.push(n);}for c in n.children(){runs(c,out);}}
        let mut values=Vec::new();runs(slot,&mut values);assert_eq!(values.len(),2);
        for run in values{let p=run.children().find(|n|n.namespace==WORD_NS&&n.local()=="rPr").unwrap();assert!(p.xml().contains("336699"));assert!(p.xml().contains("32"));}
        assert_eq!(slot.text(),"γ2");
        let again=parse(&changed.source,"dsl",true).unwrap();assert_eq!(xml_semantics(&again.root),xml_semantics(&changed.root));
        assert!(preview(&again,true).unwrap().contains("#336699"));
        for forbidden in ["row(drawing.clrScheme(drawing.accent1(drawing.srgbClr(val: \"FFFFFF\"))), office.oMath(x))", "row(word.styles(), office.oMath(x))"] {
            assert!(patch(source,"dsl",true,&original.revision,&MathPatch::ReplaceDslContent{node_id:original.nodes.iter().filter(|n|n.kind=="num").nth(1).unwrap().id.clone(),source:forbidden.into()}).is_err());
        }
    }
    #[test]
    fn math_visual_dsl_replacement_respects_new_format_and_does_not_guess_mixed_style() {
        use super::*;
        let source="frac(office.r(word.rPr(word.color(val: \"336699\"), word.sz(val: \"32\")), office.t(\"a\")), b)";
        let original=parse(source,"dsl",false).unwrap();let num=original.nodes.iter().find(|n|n.kind=="num").unwrap().id.clone();
        let changed=patch(source,"dsl",false,&original.revision,&MathPatch::ReplaceDslContent{node_id:num,source:"office.r(word.rPr(word.color(val: \"FF0000\")), office.t(\"z\"))".into()}).unwrap();
        assert!(!changed.source.contains("336699"));assert!(changed.source.contains("FF0000"));assert!(changed.source.contains("32"));
        let mixed="frac(row(office.r(word.rPr(word.color(val: \"336699\")), office.t(\"a\")), office.r(word.rPr(word.color(val: \"FF0000\")), office.t(\"b\"))), c)";
        let original=parse(mixed,"dsl",false).unwrap();let num=original.nodes.iter().find(|n|n.kind=="num").unwrap().id.clone();
        let changed=patch(mixed,"dsl",false,&original.revision,&MathPatch::ReplaceDslContent{node_id:num,source:"z".into()}).unwrap();
        assert!(!changed.source.contains("336699")&&!changed.source.contains("FF0000"));
    }
    #[test]
    fn adding_and_clearing_an_empty_office_switch_retains_namespace_and_untouched_source() {
        use super::*;
        let source="// keep author note\nrow(office.mathPr(office.smallFrac()), office.oMath(frac(α, β)))";
        let original=parse(source,"dsl",true).unwrap();
        let id=original.nodes.iter().find(|n|n.kind=="smallFrac").unwrap().id.clone();
        let explicit=patch(source,"dsl",true,&original.revision,&MathPatch::SetAttribute{node_id:id.clone(),name:"m:val".into(),value:Some("0".into())}).unwrap();
        assert!(explicit.source.starts_with("// keep author note\n"));
        assert!(explicit.source.contains("office.smallFrac(val: \"0\")"));
        assert!(explicit.source.ends_with(", office.oMath(frac(α, β)))"));
        let node=explicit.root.find(&id).unwrap();
        assert_eq!(node.attribute_namespaces.get("m:val").map(String::as_str),Some(MATH_NS));
        let implicit=patch(&explicit.source,"dsl",true,&explicit.revision,&MathPatch::SetAttribute{node_id:id.clone(),name:"m:val".into(),value:None}).unwrap();
        assert!(implicit.source.contains("office.smallFrac()"));
        assert!(implicit.root.find(&id).unwrap().attribute_namespaces.is_empty());
        assert_eq!(xml_semantics(&implicit.root),xml_semantics(&original.root));
    }
    #[test]
    fn math_document_default_attribute_edit_preserves_other_formula_and_source_notes() {
        use super::*;
        let source = "// author note\nrow(office.mathPr(office.naryLim(val: subSup)), office.oMath(sum(α, from: 0, to: 1)))";
        let doc = parse(source, "dsl", false).unwrap();
        let id = doc.nodes.iter().find(|n| n.kind == "naryLim").unwrap().id.clone();
        let changed = patch(source, "dsl", false, &doc.revision, &MathPatch::SetAttribute {node_id:id,name:"m:val".into(),value:Some("undOvr".into())}).unwrap();
        assert!(changed.source.starts_with("// author note\n"));
        assert!(changed.source.ends_with(", office.oMath(sum(α, from: 0, to: 1)))"));
        let rendered = crate::omml::parse(&changed.root.xml()).unwrap();
        assert!(crate::omml::to_mathml(&rendered, false, "").contains("<munderover>"));
    }
    #[test]
    fn math_matrix_advanced_properties_remain_editable_in_readable_dsl() {
        for expression in ["matrix([x], [y])","matrix([x], [y], properties: office.mPr(office.rSpRule(val: 3), office.rSp(val: 12)))"] {
            let original=format!("// preserve author note\nrow({expression}, matrix([a], [b]))");
            let mut doc=parse(&original,"dsl",true).unwrap();
            for (property,value) in [("rSpRule","3"),("rSp","24"),("baseJc","top")] {
                let id=doc.nodes.iter().find(|n|n.kind=="m").unwrap().id.clone();
                doc=patch(&doc.source,"dsl",true,&doc.revision,&MathPatch::SetProperty{node_id:id,container:"mPr".into(),property:property.into(),value:Some(value.into())}).unwrap();
                assert!(doc.source.starts_with("// preserve author note\n"));assert!(doc.source.ends_with(", matrix([a], [b]))"));
                let root=crate::omml::parse(&doc.root.xml()).unwrap();
                let first=root.child("oMathPara").unwrap().child("oMath").unwrap().child("m").unwrap();
                assert_eq!(first.prop("mPr",property),Some(value));
            }
            let xml=crate::omml::parse(&doc.root.xml()).unwrap();let mml=crate::omml::to_mathml(&xml,true,"");
            assert!(mml.contains("rowspacing=\"24pt\""));assert!(mml.contains("align=\"baseline 1\""));
        }
    }
    use super::*;
    #[test]
    fn unicode_fraction_owns_scripted_numerator_and_original_source_slots() {
        let source = "𝛼 + x^2/y + x^2/y";
        let doc = parse(source, "unicodemath", false).unwrap();
        let mut fractions = Vec::new();
        fn collect<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
            if node.local() == "f" { out.push(node); }
            for child in node.children() { collect(child, out); }
        }
        collect(&doc.root, &mut fractions);
        assert_eq!(fractions.len(), 2);
        for fraction in fractions {
            let numerator = fraction.children().find(|n| n.local() == "num").unwrap();
            assert_eq!(numerator.text(), "x2");
            assert_eq!(numerator.children().next().unwrap().local(), "sSup");
        }
        let nodes: Vec<_> = doc.nodes.iter().filter(|n| n.kind == "f").collect();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].from, 5); // astral alpha occupies two UTF-16 units
        let changed = patch(source, "unicodemath", false, &doc.revision,
            &MathPatch::ReplaceSlot {node_id:nodes[1].id.clone(),role:"den".into(),text:"(z+1)".into()}).unwrap();
        assert_eq!(changed.source, "𝛼 + x^2/y + x^2/(z+1)");
        let changed = patch(source, "unicodemath", false, &doc.revision,
            &MathPatch::ReplaceSlot {node_id:nodes[1].id.clone(),role:"den".into(),text:"z+1".into()}).unwrap();
        assert_eq!(changed.source, "𝛼 + x^2/y + x^2/(z+1)");
        let next = changed.nodes.iter().filter(|n|n.kind=="f").nth(1).unwrap();
        let changed = patch(&changed.source, "unicodemath", false, &changed.revision,
            &MathPatch::ReplaceSlot {node_id:next.id.clone(),role:"den".into(),text:"w+2".into()}).unwrap();
        assert_eq!(changed.source, "𝛼 + x^2/y + x^2/(w+2)");
    }
    #[test]
    fn dsl_structural_changes_preserve_sibling_source_and_outer_comments() {
        let source = "// keep the explanation\n  matrix([a, b], [c, d]) + frac( x , y , style: skewed) // keep this too\n";
        let doc = parse(source, "dsl", false).unwrap();
        let id = doc.nodes.iter().find(|n| n.kind == "m").unwrap().id.clone();
        let changed = patch(source, "dsl", false, &doc.revision, &MathPatch::MatrixAxis {node_id:id,axis:"row".into(),index:1,insert:true}).unwrap();
        assert!(changed.source.starts_with("// keep the explanation\n  "));
        assert!(changed.source.ends_with(" + frac( x , y , style: skewed) // keep this too\n"));
        assert_eq!(changed.root.children().next().unwrap().children().find(|n| n.local()=="m").unwrap().children().filter(|n|n.local()=="mr").count(),3);
        let source = "// scripts\n x^2 + frac( a , b ) // unchanged sibling";
        let doc = parse(source,"dsl",false).unwrap(); let id = doc.nodes.iter().find(|n|n.kind=="sSup").unwrap().id.clone();
        let changed = patch(source,"dsl",false,&doc.revision,&MathPatch::ConvertScript {node_id:id,kind:"sSubSup".into()}).unwrap();
        assert!(changed.source.starts_with("// scripts\n "));
        assert!(changed.source.ends_with(" + frac( a , b ) // unchanged sibling"));
    }
    #[test]
    fn inherited_switches_integer_limits_and_all_enum_facets_are_validated(){
        assert!(validate_value("degHide","0").is_ok());assert!(validate_value("degHide","maybe").is_err());
        for v in ["-2","0","2"]{assert!(validate_value("argSz",v).is_ok());}
        for v in ["-3","3","2147483648"]{assert!(validate_value("argSz",v).is_err());}
        assert!(validate_value("mathFont",&"a".repeat(32)).is_err());
        for element in schema()["elements"].as_array().unwrap(){
            if element["abstract"]==true||element["element"]==""{continue;}
            for attr in element["attributes"].as_array().unwrap(){
                if attr["name"]!="m:val"{continue;}
                if let Some(name)=attr["type"].as_str().unwrap_or("").strip_prefix("EnumValue<DocumentFormat.OpenXml.Math.").and_then(|s|s.strip_suffix('>')){
                    let enumeration=schema()["enumerations"].as_array().unwrap().iter().find(|e|e["name"]==name).unwrap();
                    for facet in enumeration["facets"].as_array().unwrap(){assert!(validate_value(element["element"].as_str().unwrap(),facet["Value"].as_str().unwrap()).is_ok(),"{} {}",element["element"],facet["Value"]);}
                    assert!(validate_value(element["element"].as_str().unwrap(),"__invalid__").is_err());
                }
            }
        }
    }
    #[test]
    fn run_property_choices_and_incompatible_script_conversions_never_emit_invalid_xml(){
        let mut doc=parse(r"\text{note}","latex",false).unwrap();let id=doc.root.children().next().unwrap().children().find(|n|n.local()=="r").unwrap().id.clone();
        for(property,value)in[("scr","fraktur"),("sty","b"),("nor","0")]{
            doc=patch(&doc.source,&doc.dialect,false,&doc.revision,&MathPatch::SetProperty{node_id:id.clone(),container:"rPr".into(),property:property.into(),value:Some(value.into())}).unwrap();
            let r=doc.root.find_mut(&id).unwrap();let pr=r.children().find(|n|n.local()=="rPr").unwrap();
            assert!(!(pr.children().any(|n|n.local()=="nor")&&pr.children().any(|n|matches!(n.local(),"scr"|"sty"))));
        }
        let doc=parse(r"x_i^j","latex",false).unwrap();let id=doc.root.children().next().unwrap().children().find(|n|n.local()=="sSubSup").unwrap().id.clone();
        let configured=patch(&doc.source,"latex",false,&doc.revision,&MathPatch::SetProperty{node_id:id.clone(),container:"sSubSupPr".into(),property:"alnScr".into(),value:Some("1".into())}).unwrap();
        assert!(patch(&configured.source,"omml",false,&configured.revision,&MathPatch::ConvertScript{node_id:id,kind:"sPre".into()}).is_err());
    }
    #[test]
    fn script_conversion_follows_each_office_schema_sequence(){
        let mut doc=parse(r"x_i^j","latex",false).unwrap();
        let id=doc.root.children().next().unwrap().children().find(|n|n.local()=="sSubSup").unwrap().id.clone();
        for kind in ["sPre","sSubSup","sPre","sSub","sSup"]{
            doc=patch(&doc.source,&doc.dialect,false,&doc.revision,&MathPatch::ConvertScript{node_id:id.clone(),kind:kind.into()}).unwrap();
            let script=doc.root.find_mut(&id).unwrap();
            let roles:Vec<_>=script.children().filter(|n|!n.local().ends_with("Pr")).map(|n|n.local().to_string()).collect();
            let expected=match kind{"sPre"=>vec!["sub","sup","e"],"sSubSup"=>vec!["e","sub","sup"],"sSub"=>vec!["e","sub"],_=>vec!["e","sup"]};
            assert_eq!(roles,expected);
            assert!(crate::math_formats::tex(script).contains('x'));
        }
    }
    #[test]
    fn matrix_alignment_changes_only_selected_column_and_survives_axis_edits(){
        let source=r"\begin{matrix}a&b\\long&d\end{matrix}";
        let doc=parse(source,"latex",false).unwrap();let id=doc.root.children().next().unwrap().children().find(|n|n.local()=="m").unwrap().id.clone();
        let changed=patch(source,"latex",false,&doc.revision,&MathPatch::MatrixColumnAlignment{node_id:id.clone(),index:1,alignment:"right".into()}).unwrap();
        let parsed=crate::omml::parse(&changed.source).unwrap();let m=parsed.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(m,2),["center","right"]);
        assert_eq!(m.children[0].name,"mPr");assert!(changed.source.contains(">b<"));
        let expanded=patch(&changed.source,"omml",false,&changed.revision,&MathPatch::MatrixAxis{node_id:id.clone(),axis:"column".into(),index:0,insert:true}).unwrap();
        let parsed=crate::omml::parse(&expanded.source).unwrap();let m=parsed.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(m,3),["center","center","right"]);
        let edited=patch(&expanded.source,"omml",false,&expanded.revision,&MathPatch::MatrixColumnAlignment{node_id:id.clone(),index:1,alignment:"left".into()}).unwrap();
        let parsed=crate::omml::parse(&edited.source).unwrap();let m=parsed.child("oMath").unwrap().child("m").unwrap();
        assert_eq!(crate::omml::matrix_column_alignments(m,3),["center","left","right"]);
        assert!(patch(source,"latex",false,&doc.revision,&MathPatch::MatrixColumnAlignment{node_id:id.clone(),index:2,alignment:"right".into()}).is_err());
        assert!(patch(source,"latex",false,&doc.revision,&MathPatch::MatrixColumnAlignment{node_id:id,index:0,alignment:"justify".into()}).is_err());
    }
    #[test]
    fn matrix_axis_edits_keep_cell_content_and_column_alignment(){
        let xml=r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:m><m:mPr><m:mcs><m:mc><m:mcPr><m:count m:val="2"/><m:mcJc m:val="right"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e><m:r><m:t>a</m:t></m:r></m:e><m:e><m:r><m:t>b</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#;
        let doc=parse(xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().id.clone();
        let added=patch(xml,"omml",false,&doc.revision,&MathPatch::MatrixAxis{node_id:id.clone(),axis:"column".into(),index:1,insert:true}).unwrap();
        assert_eq!(added.source.matches("m:val=\"right\"").count(),3);assert!(added.source.contains(">a<"));assert!(added.source.contains(">b<"));
        let removed=patch(&added.source,"omml",false,&added.revision,&MathPatch::MatrixAxis{node_id:id,axis:"column".into(),index:1,insert:false}).unwrap();
        assert_eq!(removed.source.matches("m:val=\"right\"").count(),2);assert!(removed.source.contains(">b<"));
    }
    #[test]
    fn rich_run_replacement_parses_math_and_preserves_fonts_siblings_and_namespaces(){
        for prefix in ["m","q"]{
            let xml=format!("<{prefix}:oMath xmlns:{prefix}=\"{MATH_NS}\" xmlns:w=\"{WORD_NS}\"><{prefix}:f><{prefix}:num><{prefix}:r><{prefix}:rPr><{prefix}:sty {prefix}:val=\"bi\"/></{prefix}:rPr><w:rPr><w:color w:val=\"336699\"/></w:rPr><{prefix}:t>a</{prefix}:t></{prefix}:r><{prefix}:r><{prefix}:t>b</{prefix}:t></{prefix}:r></{prefix}:num><{prefix}:den><{prefix}:r><{prefix}:t>c</{prefix}:t></{prefix}:r></{prefix}:den></{prefix}:f></{prefix}:oMath>");
            let doc=parse(&xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().children().find(|n|n.local()=="num").unwrap().children().next().unwrap().id.clone();
            let updated=patch(&xml,"omml",false,&doc.revision,&MathPatch::ReplaceContent{node_id:id.clone(),latex:r"\theta+\frac{x}{y}".into()}).unwrap();
            assert!(updated.source.contains(">θ<"));assert!(!updated.source.contains(r"\theta"));
            assert!(updated.source.contains(&format!("<{prefix}:f>")));assert!(updated.source.contains(">b<"));assert!(updated.source.contains(">c<"));
            assert_eq!(updated.source.matches("336699").count(),4);assert_eq!(updated.source.matches(&format!("{prefix}:val=\"bi\"")).count(),4);
            let invalid=patch(&xml,"omml",false,&doc.revision,&MathPatch::ReplaceContent{node_id:id,latex:r"\notKnown{x}".into()});assert!(invalid.is_err());
            let mut reader=quick_xml::NsReader::from_str(&updated.source);
            loop{let (namespace,event)=reader.read_resolved_event().unwrap();match event{
                Event::Start(_) | Event::Empty(_)=>assert!(!matches!(namespace,quick_xml::name::ResolveResult::Unknown(_))),
                Event::Eof=>break,_=>(),
            }}
        }
        let xml=format!("<oMath xmlns=\"{MATH_NS}\"><r><t>a</t></r></oMath>");let doc=parse(&xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().id.clone();
        let updated=patch(&xml,"omml",false,&doc.revision,&MathPatch::ReplaceContent{node_id:id,latex:r"\frac{x}{y}".into()}).unwrap();
        assert!(updated.source.contains("xmlns:readmdMath_"));assert!(preview(&updated,false).unwrap().contains("mfrac"));
    }

    #[test]
    fn property_edits_resolve_aliases_preserve_word_properties_and_bind_default_namespace(){
        for xml in [
            format!("<a:oMath xmlns:a=\"{MATH_NS}\" xmlns:b=\"{MATH_NS}\" xmlns:font=\"{WORD_NS}\"><a:r><font:rPr><font:b/></font:rPr><b:rPr><b:sty b:val=\"bi\"/></b:rPr><a:t>x</a:t></a:r></a:oMath>"),
            format!("<oMath xmlns=\"{MATH_NS}\" xmlns:font=\"{WORD_NS}\"><r><font:rPr><font:b/></font:rPr><rPr><sty xmlns:m=\"{MATH_NS}\" m:val=\"bi\"/></rPr><t>x</t></r></oMath>"),
        ] {
            let doc=parse(&xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().id.clone();
            let updated=patch(&xml,"omml",false,&doc.revision,&MathPatch::SetProperty{node_id:id.clone(),container:"rPr".into(),property:"sty".into(),value:Some("p".into())}).unwrap();
            let run=updated.root.children().next().unwrap().children().next().unwrap();
            let properties:Vec<_>=run.children().filter(|n|n.local()=="rPr"&&n.namespace==MATH_NS).collect();assert_eq!(properties.len(),1);
            let styles:Vec<_>=properties[0].children().filter(|n|n.local()=="sty").collect();assert_eq!(styles.len(),1);assert!(styles[0].attributes.values().any(|v|v=="p"));
            assert!(updated.source.contains("<font:b/>")&&updated.source.contains("<font:rPr>"));assert!(preview(&updated,false).unwrap().contains("mathvariant=\"normal\""));
            let mut reader=quick_xml::NsReader::from_str(&updated.source);loop {let(ns,event)=reader.read_resolved_event().unwrap();match event{Event::Start(_)|Event::Empty(_)=>assert!(!matches!(ns,quick_xml::name::ResolveResult::Unknown(_))),Event::Eof=>break,_=>()}}
            let cleared=patch(&updated.source,"omml",false,&updated.revision,&MathPatch::SetProperty{node_id:id,container:"rPr".into(),property:"sty".into(),value:None}).unwrap();assert!(cleared.source.contains("<font:b/>")&&!cleared.source.contains(":sty"));
        }
        let xml=format!("<oMath xmlns=\"{MATH_NS}\"><m><mr><e><r><t>a</t></r></e></mr></m></oMath>");let doc=parse(&xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().id.clone();
        let updated=patch(&xml,"omml",false,&doc.revision,&MathPatch::MatrixColumnAlignment{node_id:id,index:0,alignment:"right".into()}).unwrap();
        assert!(preview(&updated,false).unwrap().contains("columnalign=\"right\""));assert!(updated.source.contains("xmlns:readmdMath="));
        let xml=format!("<a:oMath xmlns:a=\"{MATH_NS}\" xmlns:b=\"{MATH_NS}\" xmlns:font=\"{WORD_NS}\"><a:r><a:rPr><a:sty b:val=\"bi\" font:val=\"preserve\"/></a:rPr><a:t>x</a:t></a:r></a:oMath>");let doc=parse(&xml,"omml",false).unwrap();
        let id=doc.root.children().next().unwrap().children().next().unwrap().children().find(|n|n.local()=="rPr").unwrap().children().next().unwrap().id.clone();
        let updated=patch(&xml,"omml",false,&doc.revision,&MathPatch::SetAttribute{node_id:id,name:"m:val".into(),value:Some("p".into())}).unwrap();
        assert!(!updated.source.contains("b:val="));assert!(updated.source.contains("a:val=\"p\"")&&updated.source.contains("font:val=\"preserve\""));
        assert!(parse(&format!("<m:oMath xmlns:m=\"{WORD_NS}\"><m:r/></m:oMath>"),"omml",false).is_err());
    }

    #[test]
    fn explicit_replacement_fonts_preserve_other_run_controls(){
        let xml=format!("<m:oMath xmlns:m=\"{MATH_NS}\"><m:r><m:rPr><m:sty m:val=\"bi\"/><m:brk m:alnAt=\"2\"/><m:aln m:val=\"0\"/></m:rPr><m:t>a</m:t></m:r></m:oMath>");let doc=parse(&xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().id.clone();
        let updated=patch(&xml,"omml",false,&doc.revision,&MathPatch::ReplaceContent{node_id:id,latex:r"\mathrm{x}".into()}).unwrap();
        assert!(updated.source.contains("m:alnAt=\"2\"")&&updated.source.contains("m:val=\"0\""));
        assert!(updated.source.contains("m:sty m:val=\"p\"")&&!updated.source.contains("m:val=\"bi\""));
        let run=updated.root.children().next().unwrap().children().next().unwrap();let names:Vec<_>=run.children().find(|n|n.local()=="rPr").unwrap().children().map(XmlNode::local).collect();assert_eq!(names,["sty","brk","aln"]);
    }
    #[test]
    fn matrix_edits_respect_office_row_and_column_cardinalities(){
        for (rows,columns,axis,maximum) in [(1,63,"column",64),(255,1,"row",256)]{
            let row=format!("<m:mr>{}</m:mr>","<m:e/>".repeat(columns));
            let xml=format!("<m:oMath xmlns:m=\"{MATH_NS}\"><m:m>{}</m:m></m:oMath>",row.repeat(rows));
            let doc=parse(&xml,"omml",false).unwrap();let id=doc.root.children().next().unwrap().children().next().unwrap().id.clone();
            let count=if axis=="row"{rows}else{columns};
            let full=patch(&xml,"omml",false,&doc.revision,&MathPatch::MatrixAxis{node_id:id.clone(),axis:axis.into(),index:count,insert:true}).unwrap();
            let m=full.root.children().next().unwrap().children().next().unwrap();
            let actual=if axis=="row"{m.children().filter(|n|n.local()=="mr").count()}else{m.children().find(|n|n.local()=="mr").unwrap().children().filter(|n|n.local()=="e").count()};
            assert_eq!(actual,maximum);
            assert!(patch(&full.source,"omml",false,&full.revision,&MathPatch::MatrixAxis{node_id:id,axis:axis.into(),index:maximum,insert:true}).is_err());
        }
    }
    #[test]
    fn metadata_reaches_real_docx_epub_and_pdf_model_and_stale_data_is_not_used(){
        let tex=r"\frac{x}{y}";
        let xml=crate::latex2omml::latex_to_omml(tex,false).replace("<m:f>","<m:f><m:fPr><m:type m:val=\"skw\"/></m:fPr>");
        let metadata=metadata(tex,&xml,"sample").unwrap();let markdown=format!("# Study\n\nResult ${tex}${metadata}.\n");
        let document=crate::md_ast::parse(&markdown);
        let package=crate::docx_writer::build(&document,&serde_json::json!({}),"study.md",&|_|None);
        let word=String::from_utf8_lossy(&package.parts.iter().find(|(n,_)|n=="word/document.xml").unwrap().1);
        assert!(word.contains("m:val=\"skw\""),"{word}");
        let chapters=crate::epub_writer::render_chapters(&document,"none",&mut |_|None);
        assert!(chapters.iter().any(|c|c.body.contains("bevelled=\"true\"")));
        assert!(crate::md_ast::to_render_json(&document.blocks).to_string().contains("skw"));
        let stale=crate::md_ast::parse(&markdown.replace("$\\frac{x}{y}$","$\\frac{z}{y}$"));
        assert!(!crate::md_ast::to_render_json(&stale.blocks).to_string().contains("\\\"skw\\\""));
        assert!(parse_xml("<m:t>\u{1}</m:t>").is_err());
    }
    #[test]
    fn qualified_properties_mixed_text_and_entities_survive_roundtrip() {
        let xml=format!("<m:oMath xmlns:m=\"{MATH_NS}\" xmlns:w=\"{WORD_NS}\"><m:r><m:rPr><m:nor m:val=\"0\"/></m:rPr><w:rPr><w:b w:val=\"0\"/></w:rPr><m:t xml:space=\"preserve\"> α &amp; &#x1D6C2; </m:t></m:r></m:oMath>");
        let tree=parse_xml(&xml).unwrap();let output=tree.xml();
        assert!(output.contains("<m:rPr>"));assert!(output.contains("<w:rPr>"));assert!(output.contains("m:val=\"0\""));assert!(output.contains("w:val=\"0\""));
        assert_eq!(parse_xml(&output).unwrap(),tree);
        assert!(output.contains(" α &amp; 𝛂 "));
    }
    #[test]
    fn utf16_source_slots_replace_only_the_selected_argument() {
        let source=r"𝛂+\frac{x}{x}+x";
        let doc=parse(source,"latex",false).unwrap();let node=doc.nodes.iter().find(|n|n.kind=="fraction").unwrap();
        let result=patch(source,"latex",false,&doc.revision,&MathPatch::ReplaceSlot {node_id:node.id.clone(),role:"den".into(),text:"y+1".into()}).unwrap();
        assert_eq!(result.source,r"𝛂+\frac{x}{y+1}+x");
        assert_eq!(patch(source,"latex",false,"old",&MathPatch::ReplaceSource {node_id:node.id.clone(),text:"z".into()}).unwrap_err(),"transaction_conflict");
    }
    #[test]
    fn property_edits_preserve_explicit_false_and_validate_enumerations() {
        let doc=parse(r"\frac{a}{b}","latex",false).unwrap();let math=doc.root.children().next().unwrap();let f=math.children().find(|n|n.local()=="f").unwrap();
        let p=MathPatch::SetProperty {node_id:f.id.clone(),container:"fPr".into(),property:"type".into(),value:Some("skw".into())};
        let changed=patch(&doc.source,"latex",false,&doc.revision,&p).unwrap();assert!(changed.source.contains("m:val=\"skw\""));
        let p=MathPatch::SetProperty {node_id:f.id.clone(),container:"fPr".into(),property:"type".into(),value:Some("not-a-type".into())};
        assert_eq!(patch(&doc.source,"latex",false,&doc.revision,&p).unwrap_err(),"math_invalid_property");
    }
    #[test]
    fn metadata_keeps_complex_properties_and_detects_external_source_edits() {
        let omml=crate::latex2omml::latex_to_omml("x",false);let comment=metadata("x",&omml,"equation-1").unwrap();
        assert!(read_metadata(&comment,"x").unwrap().1);assert!(!read_metadata(&comment,"y").unwrap().1);
        assert_eq!(read_metadata(&comment,"y").unwrap().0.omml,omml);
    }
    #[test]
    fn hostile_and_unbounded_xml_and_unknown_macros_are_not_silently_accepted() {
        assert_eq!(parse_xml("<!DOCTYPE r [<!ENTITY x SYSTEM 'file:///secret'>]><r>&x;</r>").unwrap_err(),"math_unsafe_xml");
        assert!(parse_xml("<m:r><m:t>x</m:r>").is_err());
        assert!(parse_xml(&("<e>".repeat(200)+&"</e>".repeat(200))).is_err());
        let doc=parse(r"\unknownMacro{x}","latex",false).unwrap();assert_eq!(doc.diagnostics[0].code,"math_unknown_macro");assert_eq!(doc.source,r"\unknownMacro{x}");
    }
}
