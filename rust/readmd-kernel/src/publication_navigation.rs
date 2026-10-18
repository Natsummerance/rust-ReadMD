//! Source-owned EPUB 3.3 navigation. Comments remain transparent to other
//! stories; only the EPUB writer projects these typed anchors into XHTML.
use crate::md_ast::{Block, Document, Inline};
use pulldown_cmark::{Event, Parser, Tag};
use std::ops::Range;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const PREFIX: &str = "<!-- readmd:epub";
pub const MAX_MARKERS: usize = crate::publication_blocks::MAX_NODES;
pub const MAX_DECLARATION_BYTES: usize = 2048;
pub const INVALID: &str = "publication_navigation_invalid";
pub const REFERENCE: &str = "publication_navigation_reference";
pub const BUDGET: &str = "publication_navigation_budget";
pub const BODY_ID: &str = "readmd-epub-bodymatter";
const ROLES: &[&str] = &["cover", "toc", "bodymatter", "titlepage", "preface", "bibliography", "glossary", "index"];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    pub path: String,
    pub from: usize,
    pub to: usize,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    fn new(code: &'static str, path: impl Into<String>) -> Self {
        Self {code, path:path.into(), from:0, to:0}
    }
    pub fn response_body(&self) -> Value {
        json!({"ok":false,"error_code":self.code,"path":self.path,"field":self.path,"from":self.from,"to":self.to,"units":"UTF-16"})
    }
    pub fn api_error(self) -> crate::ApiError {
        crate::ApiError::new(422,self.code).noted("field",self.path).noted_json("from",json!(self.from)).noted_json("to",json!(self.to)).noted("units","UTF-16")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landmark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip)]
    pub from: usize,
    #[serde(skip)]
    pub to: usize,
}
impl Anchor {
    pub fn target_id(&self) -> String { crate::epub_writer::xml_id(&format!("readmd-epub-{}",self.id)) }
    fn error(&self, code: &'static str, field: &str) -> Error {
        Error {code,path:format!("source.epub.{}.{}",self.id,field),from:self.from,to:self.to}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Landmark {
    #[serde(rename="type")]
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub label: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaginationSource {
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub description: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LandmarkMode { Auto, Disabled, Explicit(Vec<Landmark>) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub anchors: Vec<Anchor>,
    pub landmarks: LandmarkMode,
    pub page_list: bool,
    pub pagination_source: Option<PaginationSource>,
    pub labels: BTreeMap<String,String>,
    landmarks_path:String,
    landmarks_from:usize,
    landmarks_to:usize,
}
impl Default for Request {
    fn default() -> Self { Self {anchors:Vec::new(),landmarks:LandmarkMode::Auto,page_list:true,pagination_source:None,labels:BTreeMap::new(),landmarks_path:"options.epub.navigation.landmarks".into(),landmarks_from:0,landmarks_to:0} }
}
impl Request {
    fn landmark_error(&self,index:usize)->Error{Error{code:REFERENCE,path:format!("{}[{index}].target",self.landmarks_path),from:self.landmarks_from,to:self.landmarks_to}}
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Link {pub href:String, pub label:String, #[serde(skip_serializing_if="Option::is_none")] pub kind:Option<String>}
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Resolved { pub landmarks:Vec<Link>, pub pages:Vec<Link> }

fn clean_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count()<=max
        && !text.chars().any(|c|c.is_control()||matches!(c,'\u{fffe}'|'\u{ffff}'))
}
fn valid_id(id:&str)->bool {
    !id.is_empty() && id.len()<=80 && id.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.'))
        && id!="bodymatter" // reserved generated XHTML anchor identity
}
fn validate_anchor(anchor:&Anchor)->Result<()> {
    if !valid_id(&anchor.id) {return Err(anchor.error(INVALID,"id"));}
    if anchor.page.is_none() && anchor.landmark.is_none() {return Err(anchor.error(INVALID,"page"));}
    if anchor.page.as_ref().is_some_and(|p|!clean_text(p,32)) {return Err(anchor.error(INVALID,"page"));}
    if anchor.landmark.as_ref().is_some_and(|r|!ROLES.contains(&r.as_str())) {return Err(anchor.error(INVALID,"landmark"));}
    if anchor.label.as_ref().is_some_and(|l|anchor.landmark.is_none()||!clean_text(l,256)) {return Err(anchor.error(INVALID,"label"));}
    Ok(())
}

/// Reparse only an already recognized declaration carrier as ordinary inline
/// Markdown. The removed prefix keeps a leading comment from swallowing its
/// same-line body as a raw HTML block; all returned ranges address the carrier.
fn carrier_events(fragment:&str)->Vec<(Event<'static>,Range<usize>)> {
    let content=fragment.trim_start();let leading=fragment.len()-content.len();
    let input=format!("x {content}");
    Parser::new_ext(&input,crate::md_ast::options()).into_offset_iter().filter_map(|(event,range)|{
        let event=match event {
            Event::Text(text) if range.start==0=>{
                let text=text.strip_prefix("x ").unwrap_or(&text);
                if text.is_empty(){return None;}Event::Text(text.to_string().into())
            },
            other=>other.into_static(),
        };
        Some((event,leading+range.start.saturating_sub(2)..leading+range.end.saturating_sub(2)))
    }).collect()
}

/// Recognize only parsed HTML comments. Same-line body text and Markdown retain
/// their own parser events; every declaration owns its exact local byte range.
pub fn parse_html(fragment:&str)->Result<Vec<Anchor>> {
    let content=fragment.trim_start();let leading=fragment.len()-content.len();let mut out=Vec::new();
    if !content.starts_with(PREFIX){return Ok(out);}
    for (event,range) in carrier_events(fragment){
        if !matches!(event,Event::Html(_)|Event::InlineHtml(_)){continue;}
        let raw=&fragment[range.clone()];if !raw.starts_with(PREFIX){continue;}
        let parsed=(||{
            let body=raw.strip_prefix("<!-- readmd:epub v1 ").ok_or_else(||Error::new(INVALID,"source.epub"))?;
            let body=body.strip_suffix("-->").ok_or_else(||Error::new(INVALID,"source.epub"))?;
            if raw.len()>MAX_DECLARATION_BYTES{return Err(Error::new(BUDGET,"source.epub"));}
            let mut anchor:Anchor=serde_json::from_str(body).map_err(|_|Error::new(INVALID,"source.epub"))?;
            anchor.from=range.start;anchor.to=range.end;validate_anchor(&anchor)?;Ok(anchor)
        })().map_err(|mut error|{error.from=range.start;error.to=range.end;error})?;
        out.push(parsed);if out.len()>MAX_MARKERS{return Err(Error{code:BUDGET,path:"source.epub".into(),from:range.start,to:range.end});}
    }
    if out.first().is_none_or(|anchor|anchor.from!=leading){return Err(Error{code:INVALID,path:"source.epub".into(),from:leading,to:fragment.trim_end().len()});}
    Ok(out)
}

/// Expand only a validated navigation HTML block that also contains body
/// content. The shared AST builder then retains strong/link/math/code semantics.
pub(crate) fn body_carrier_events(fragment:&str)->Option<Vec<Event<'static>>> {
    let anchors=parse_html(fragment).ok()?;if anchors.is_empty(){return None;}
    let mut previous=0;let mut body=false;
    for anchor in anchors{body|=!fragment[previous..anchor.from].trim().is_empty();previous=anchor.to;}
    if !body&&fragment[previous..].trim().is_empty(){return None;}
    Some(carrier_events(fragment).into_iter().map(|(event,_)|event).collect())
}

fn nav_value<'a>(options:&'a Value)->Result<Option<&'a Value>> {
    let Some(value)=options.pointer("/epub/navigation").filter(|v|!v.is_null()) else{return Ok(None);};
    if !value.is_object(){return Err(Error::new(INVALID,"options.epub.navigation"));}Ok(Some(value))
}
fn apply_layer(request:&mut Request,value:&Value,path:&str)->Result<()> {
    if value.is_null(){return Ok(());}let values=value.as_object().ok_or_else(||Error::new(INVALID,path))?;
    for (key,value) in values {
        let field=format!("{path}.{key}");
        if !matches!(key.as_str(),"landmarks"|"pageList"|"paginationSource"|"labels") {return Err(Error::new(INVALID,field));}
        if value.is_null(){continue;}
        match key.as_str() {
            "landmarks"=>request.landmarks=match value {
                Value::String(s) if s=="auto"=>LandmarkMode::Auto,
                Value::Bool(false)=>LandmarkMode::Disabled,
                Value::Array(values)=>{
                    if values.len()>MAX_MARKERS {return Err(Error::new(BUDGET,&field));}
                    let mut entries=Vec::new();let mut seen=BTreeSet::new();
                    for (index,value) in values.iter().enumerate(){let at=format!("{field}[{index}]");
                        let entry:Landmark=serde_json::from_value(value.clone()).map_err(|_|Error::new(INVALID,&at))?;
                        if !ROLES.contains(&entry.kind.as_str())||!(valid_id(&entry.target)||matches!(entry.target.as_str(),"@cover"|"@toc"|"@bodymatter"))
                            ||entry.label.as_ref().is_some_and(|s|!clean_text(s,256)){return Err(Error::new(INVALID,at));}
                        if !seen.insert((entry.kind.clone(),entry.target.clone())){return Err(Error::new(REFERENCE,at));}entries.push(entry);
                    }LandmarkMode::Explicit(entries)
                },_=>return Err(Error::new(INVALID,field)),
            },
            "pageList"=>request.page_list=match value{Value::String(s) if s=="auto"=>true,Value::Bool(false)=>false,_=>return Err(Error::new(INVALID,field))},
            "paginationSource"=>{
                if value==&Value::Bool(false){request.pagination_source=None;continue;}
                let source:PaginationSource=serde_json::from_value(value.clone()).map_err(|_|Error::new(INVALID,&field))?;
                if source.identifier.is_none()&&source.description.is_none()
                    ||source.identifier.as_ref().is_some_and(|s|!clean_text(s,512))||source.description.as_ref().is_some_and(|s|!clean_text(s,1024)) {return Err(Error::new(INVALID,field));}
                request.pagination_source=Some(source);
            },
            "labels"=>{
                let values=value.as_object().ok_or_else(||Error::new(INVALID,&field))?;
                for (key,value)in values{let text=value.as_str().filter(|s|clean_text(s,256)).ok_or_else(||Error::new(INVALID,format!("{field}.{key}")))?;
                    if !matches!(key.as_str(),"landmarks"|"pageList")&&!ROLES.contains(&key.as_str()){return Err(Error::new(INVALID,format!("{field}.{key}")));}
                    request.labels.insert(key.clone(),text.into());
                }
            },_=>unreachable!(),
        }
    }Ok(())
}
pub fn validate_options(options:&Value)->Result<()> {
    if let Some(value)=nav_value(options)?{apply_layer(&mut Request::default(),value,"options.epub.navigation")?;}Ok(())
}

/// Preset overlay null means inherit; arrays stay atomic, never concatenated.
pub fn inherit_null_overlay(over:&mut Value) {
    if let Some(epub)=over.get_mut("epub").and_then(Value::as_object_mut){
        if epub.get("navigation").is_some_and(Value::is_null){epub.remove("navigation");return;}
        if let Some(nav)=epub.get_mut("navigation").and_then(Value::as_object_mut){nav.retain(|_,v|!v.is_null());}
    }
}

pub fn from_source(source:&str,options:&Value)->Result<Request> {
    validate_options(options)?;let mut request=Request::default();let stripped=source.strip_prefix('\u{feff}').unwrap_or(source);let bom=source.len()-stripped.len();
    let mut restricted=Vec::new();let mut front_range:Option<Range<usize>>=None;let mut in_front=false;let mut seen=BTreeSet::new();
    for (event,range) in Parser::new_ext(stripped,crate::md_ast::options()).into_offset_iter(){
        if crate::speech::cancelled(){return Err(Error::new("cancelled","source.epub"));}
        match event {
            Event::Start(tag)=>{
                if matches!(&tag,Tag::MetadataBlock(_)){in_front=true;}
                let blocked=matches!(tag,Tag::BlockQuote(_)|Tag::List(_)|Tag::Item|Tag::Table(_)|Tag::TableHead|Tag::TableRow|Tag::TableCell|Tag::FootnoteDefinition(_)|Tag::Image{..});
                restricted.push(blocked);
            },
            Event::End(_)=>{restricted.pop();if in_front{in_front=false;}},
            Event::Text(_) if in_front=>{
                let front=front_range.get_or_insert(bom+range.start..bom+range.end);front.end=bom+range.end;
                if front.len()>65536{return Err(Error::new(BUDGET,"source.frontMatter.epub.navigation"));}
            },
            Event::Html(_)|Event::InlineHtml(_)=>{
                let base=bom+range.start;
                let mut anchors=parse_html(&stripped[range]).map_err(|mut error|{error.from=source[..base+error.from].encode_utf16().count();error.to=source[..base+error.to].encode_utf16().count();error})?;
                if restricted.iter().any(|blocked|*blocked){if let Some(anchor)=anchors.first(){return Err(Error{code:INVALID,path:"source.epub.story".into(),from:source[..base+anchor.from].encode_utf16().count(),to:source[..base+anchor.to].encode_utf16().count()});}}
                for anchor in &mut anchors {
                    // Retain byte endpoints while parsing. Convert all distinct
                    // endpoints in one ordered sweep instead of rescanning the
                    // full prefix for every inline declaration.
                    anchor.from+=base;anchor.to+=base;
                    if !seen.insert(anchor.id.clone()){
                        anchor.from=source[..anchor.from].encode_utf16().count();anchor.to=source[..anchor.to].encode_utf16().count();
                        return Err(anchor.error(REFERENCE,"id"));
                    }
                }request.anchors.extend(anchors);if request.anchors.len()>MAX_MARKERS{return Err(Error::new(BUDGET,"source.epub"));}
            },_=>(),
        }
    }
    let endpoints=request.anchors.iter().flat_map(|anchor|[anchor.from,anchor.to]).collect::<BTreeSet<_>>();
    let mut offsets=BTreeMap::new();let(mut previous,mut units)=(0,0);
    for byte in endpoints{
        if crate::speech::cancelled(){return Err(Error::new("cancelled","source.epub"));}
        units+=source[previous..byte].encode_utf16().count();offsets.insert(byte,units);previous=byte;
    }
    for anchor in &mut request.anchors{anchor.from=offsets[&anchor.from];anchor.to=offsets[&anchor.to];}
    if let Some(front_range)=front_range{
        let front_start=front_range.start;let front=&source[front_range];
        let layer=crate::publication_metadata::bounded_front_matter_member(front,&["epub","navigation"])
            .map_err(|e|Error{code:if e.code=="publication_metadata_budget"{BUDGET}else{INVALID},path:"source.frontMatter.epub.navigation".into(),from:source[..front_start].encode_utf16().count()+e.from,to:source[..front_start].encode_utf16().count()+e.to})?;
        if let Some((value,from,to))=layer {
            let start=source[..front_start].encode_utf16().count();
            apply_layer(&mut request,&value,"source.frontMatter.epub.navigation").map_err(|mut error|{error.from=start+from;error.to=start+to;error})?;
            if value.get("landmarks").is_some_and(|v|!v.is_null()){request.landmarks_path="source.frontMatter.epub.navigation.landmarks".into();request.landmarks_from=start+from;request.landmarks_to=start+to;}
        }
    }
    if let Some(value)=nav_value(options)?{
        apply_layer(&mut request,value,"options.epub.navigation")?;
        if value.get("landmarks").is_some_and(|v|!v.is_null()){request.landmarks_path="options.epub.navigation.landmarks".into();request.landmarks_from=0;request.landmarks_to=0;}
    }
    let doc=crate::md_ast::parse(source);validate_document(&doc,&request)?;Ok(request)
}

pub fn validate_document(doc:&Document,request:&Request)->Result<()> {
    fn inlines(values:&[Inline],blocked:bool,out:&mut Vec<Anchor>)->Result<()> {
        for item in values{match item{
            Inline::Html(raw)=>{let anchors=parse_html(raw)?;if blocked&&!anchors.is_empty(){return Err(Error::new(INVALID,"source.epub.story"));}out.extend(anchors);},
            Inline::Strong(c)|Inline::Emph(c)|Inline::Strike(c)|Inline::Styled{children:c,..}|Inline::Display{children:c,..}|Inline::Citation{children:c,..}|Inline::Link{children:c,..}=>inlines(c,blocked,out)?,_=>(),
        }}Ok(())
    }
    fn blocks(values:&[Block],blocked:bool,out:&mut Vec<Anchor>)->Result<()> {
        for block in values{match block.unstyled(){
            Block::Html(raw)=>{let anchors=parse_html(raw)?;if blocked&&!anchors.is_empty(){return Err(Error::new(INVALID,"source.epub.story"));}out.extend(anchors);},
            Block::Paragraph(v)|Block::Heading{inlines:v,..}|Block::Caption{inlines:v,..}=>inlines(v,blocked,out)?,
            Block::Quote(v)|Block::FootnoteDef{blocks:v,..}=>blocks(v,true,out)?,
            Block::List{items,..}=>for item in items{blocks(&item.blocks,true,out)?;},
            Block::Table{header,rows,..}=>for row in std::iter::once(header).chain(rows){for cell in row{inlines(cell,true,out)?;}},_=>(),
        }}Ok(())
    }
    let mut actual=Vec::new();blocks(&doc.blocks,false,&mut actual)?;
    let actual_len=actual.len();let actual:BTreeMap<_,_>=actual.into_iter().map(|a|(a.id.clone(),a)).collect();
    if actual.len()!=actual_len{return Err(Error::new(REFERENCE,"source.epub.id"));}
    if actual.len()!=request.anchors.len(){return Err(Error::new(INVALID,"source.epub.carrier"));}
    for expected in &request.anchors{
        if actual.get(&expected.id).is_none_or(|actual|actual.page!=expected.page||actual.landmark!=expected.landmark||actual.label!=expected.label){return Err(expected.error(INVALID,"carrier"));}
    }
    if let LandmarkMode::Explicit(entries)=&request.landmarks{
        for (i,entry)in entries.iter().enumerate(){if !entry.target.starts_with('@')&&!actual.contains_key(&entry.target){return Err(request.landmark_error(i));}}
    }Ok(())
}
/// Virtual destinations are validated before a chooser, images or staging.
/// Existing non-EPUB consumers transparently skip the source-only comments.
pub fn preflight_export(format:&str,source:&str,options:&Value)->Result<Request> {
    let request=from_source(source,options)?;
    if format.eq_ignore_ascii_case("epub") {
        let doc=crate::md_ast::parse(source);
        let cover=options.pointer("/epub/cover").and_then(Value::as_str).is_some_and(|s|!s.trim().is_empty())
            ||options.pointer("/cover/enabled").is_some_and(crate::mdexport::py_truthy)||doc.meta("cover").is_some();
        let body=doc.blocks.iter().any(|b|!matches!(b.unstyled(),Block::Html(_)|Block::FootnoteDef{..}|Block::SectionStart(_)));
        if !body&&!request.anchors.is_empty(){return Err(Error::new(REFERENCE,"epub.navigation.bodymatter"));}
        if let LandmarkMode::Explicit(entries)=&request.landmarks{for (i,entry)in entries.iter().enumerate(){
            if entry.target=="@cover"&&!cover||entry.target=="@bodymatter"&&!body{return Err(request.landmark_error(i));}
        }}
    }Ok(request)
}

pub fn anchor_html(fragment:&str)->Result<Option<String>> {
    let anchors=parse_html(fragment)?;if anchors.is_empty(){return Ok(None);}Ok(Some(anchors.iter().map(|anchor|{
        let page=anchor.page.as_ref().map(|label|format!(" epub:type=\"pagebreak\" role=\"doc-pagebreak\" aria-label=\"{}\"",crate::epub_writer::esc(label))).unwrap_or_default();
        format!("<span id=\"{}\" data-readmd-epub-anchor=\"source\"{page}></span>",anchor.target_id())
    }).collect()))
}

/// Scan actual generated XML. Only real id attributes define destinations;
/// strings in script/text/attributes and ordinary epub:type values do not.
pub fn resolve(request:&Request,chapters:&[crate::epub_writer::Chapter],cover:bool)->Result<Resolved> {
    use quick_xml::{events::Event as XmlEvent,Reader,XmlVersion};
    let mut ids=BTreeMap::<String,Vec<(usize,String,String,Option<String>)>>::new();let mut order=0usize;
    for (index,chapter)in chapters.iter().enumerate(){let file=format!("chapter_{}.xhtml",index+1);let mut reader=Reader::from_str(&chapter.body);let mut events=0usize;
        loop{if crate::speech::cancelled(){return Err(Error::new("cancelled","epub.navigation"));}events+=1;if events>2_000_000{return Err(Error::new(BUDGET,"epub.navigation.xml"));}
            match reader.read_event().map_err(|_|Error::new(INVALID,"epub.navigation.xml"))?{
                XmlEvent::Start(e)|XmlEvent::Empty(e)=>{
                    let mut id=None;let mut owned=None;
                    for a in e.attributes(){let a=a.map_err(|_|Error::new(INVALID,"epub.navigation.xml"))?;
                        if matches!(a.key.as_ref(),"id"|"data-readmd-epub-anchor"){
                            let value=a.normalized_value(XmlVersion::Implicit1_0).map_err(|_|Error::new(INVALID,"epub.navigation.xml"))?.into_owned();
                            if a.key.as_ref()=="id"{id=Some(value);}else{owned=Some(value);}
                        }
                    }
                    if let Some(id)=id{ids.entry(id.clone()).or_default().push((order,format!("{file}#{id}"),e.name().as_ref().to_string(),owned));order+=1;}
                },XmlEvent::Eof=>break,_=>(),
            }
        }
    }
    let target=|id:&str,owner:&str|->Result<(usize,String)>{
        let values=ids.get(id).filter(|v|v.len()==1).ok_or_else(||Error::new(REFERENCE,format!("epub.navigation.target.{id}")))?;
        let (order,href,tag,actual_owner)=&values[0];
        if tag!="span"||actual_owner.as_deref()!=Some(owner){return Err(Error::new(REFERENCE,format!("epub.navigation.target.{id}")));}
        Ok((*order,href.clone()))
    };
    let body=if ids.contains_key(&crate::epub_writer::xml_id(BODY_ID)){Some(target(&crate::epub_writer::xml_id(BODY_ID),"bodymatter")?.1)}else{None};
    let mut pages=Vec::new();let mut source_landmarks=Vec::new();
    for anchor in &request.anchors{let (order,href)=target(&anchor.target_id(),"source").map_err(|_|anchor.error(REFERENCE,"id"))?;
        if request.page_list{if let Some(label)=&anchor.page{pages.push((order,Link{href:href.clone(),label:label.clone(),kind:None}));}}
        if let Some(kind)=&anchor.landmark{source_landmarks.push(Link{href,label:anchor.label.clone().unwrap_or_else(||label(request,kind)),kind:Some(kind.clone())});}
    }pages.sort_by_key(|(order,_)|*order);
    let mut landmarks=match &request.landmarks{
        LandmarkMode::Disabled=>Vec::new(),
        LandmarkMode::Auto=>{
            let explicit_roles=source_landmarks.iter().filter_map(|l|l.kind.as_deref()).collect::<BTreeSet<_>>();let mut result=Vec::new();
            if cover&&!explicit_roles.contains("cover"){result.push(Link{href:"cover.xhtml".into(),label:label(request,"cover"),kind:Some("cover".into())});}
            if !explicit_roles.contains("toc"){result.push(Link{href:"nav.xhtml#toc".into(),label:label(request,"toc"),kind:Some("toc".into())});}
            if let Some(href)=&body{if !explicit_roles.contains("bodymatter"){result.push(Link{href:href.clone(),label:label(request,"bodymatter"),kind:Some("bodymatter".into())});}}
            result.extend(source_landmarks);result
        },
        LandmarkMode::Explicit(entries)=>{let mut result=Vec::new();for (i,entry)in entries.iter().enumerate(){let href=match entry.target.as_str(){
            "@cover" if cover=>"cover.xhtml".into(),"@toc"=>"nav.xhtml#toc".into(),"@bodymatter"=>body.clone().ok_or_else(||request.landmark_error(i))?,
            value if !value.starts_with('@')=>target(&crate::epub_writer::xml_id(&format!("readmd-epub-{value}")),"source")?.1,_=>return Err(request.landmark_error(i)),
        };result.push(Link{href,label:entry.label.clone().unwrap_or_else(||label(request,&entry.kind)),kind:Some(entry.kind.clone())});}result},
    };
    // EPUB 3.3 forbids duplicate types to the same resource, including
    // different fragments in that resource. Distinct documents may repeat.
    let mut seen=BTreeSet::new();for entry in &landmarks{if !seen.insert((entry.kind.clone(),entry.href.split('#').next().unwrap_or("").to_string())){return Err(Error::new(REFERENCE,"epub.navigation.landmarks"));}}
    // These are semantic entry points, not a second table of contents.
    landmarks.shrink_to_fit();Ok(Resolved{landmarks,pages:pages.into_iter().map(|(_,link)|link).collect()})
}

pub fn label(request:&Request,key:&str)->String {
    request.labels.get(key).cloned().unwrap_or_else(||match key{"landmarks"=>"Landmarks","pageList"=>"Pages","cover"=>"Cover","toc"=>"Contents","bodymatter"=>"Start of content","titlepage"=>"Title page","preface"=>"Preface","bibliography"=>"References","glossary"=>"Glossary","index"=>"Index",_=>key}.into())
}
/// The caller supplies effective publication language, never application UI
/// language. Explicit source/export labels win over the shared locale provider.
pub fn localize(request:&mut Request,language:&str)->Option<&'static str> {
    static LABELS:std::sync::OnceLock<Value>=std::sync::OnceLock::new();
    let asset=LABELS.get_or_init(||serde_json::from_str(include_str!("../../../assets/data/publication-navigation-i18n.json")).unwrap_or(Value::Null));
    let labels=&asset["contentLabels"];
    let supported=crate::publication_metadata::select_content_labels(labels,language);
    let selected=supported.or_else(||labels.get("en"));
    let explicit=std::mem::take(&mut request.labels);
    if let Some(values)=selected.and_then(Value::as_object){request.labels=values.iter().filter_map(|(k,v)|v.as_str().map(|s|(k.clone(),s.into()))).collect();}
    request.labels.extend(explicit);
    supported.is_none().then_some("publication_metadata_label_fallback")
}
pub fn navigation_xhtml(request:&Request,resolved:&Resolved)->String {
    fn nav(kind:&str,title:&str,items:&[Link])->String{
        if items.is_empty(){return String::new();}let entries=items.iter().map(|item|format!("<li><a{} href=\"{}\">{}</a></li>",item.kind.as_ref().map(|kind|format!(" epub:type=\"{kind}\"")).unwrap_or_default(),crate::epub_writer::esc(&item.href),crate::epub_writer::esc(&item.label))).collect::<String>();
        format!("<nav epub:type=\"{kind}\" id=\"readmd-{kind}\" hidden=\"hidden\"><h2>{}</h2><ol>{entries}</ol></nav>",crate::epub_writer::esc(title))
    }
    format!("{}{}",nav("landmarks",&label(request,"landmarks"),&resolved.landmarks),nav("page-list",&label(request,"pageList"),&resolved.pages))
}

/// Called after P01's final metadata replacement, inside the same ZIP staging.
/// Package identifier/author fields are untouched. No experimental vocabulary.
pub fn pagination_metadata(request:&Request,resolved:&Resolved)->String {
    if resolved.pages.is_empty(){return String::new();}let Some(source)=&request.pagination_source else{return String::new();};
    let value=source.identifier.as_deref().or(source.description.as_deref()).unwrap_or("");
    let description=if source.identifier.is_some(){source.description.as_ref().map(|s|format!("<meta property=\"readmd:pagination-source-description\">{}</meta>",crate::epub_writer::esc(s))).unwrap_or_default()}else{String::new()};
    format!("<dc:source id=\"readmd-pagination-source\">{}</dc:source><meta refines=\"#readmd-pagination-source\" property=\"source-of\">pagination</meta>{description}",crate::epub_writer::esc(value))
}

pub fn request_schema()->Value {
    let nullable=|value:Value|json!({"anyOf":[value,{"type":"null"}]});
    let entry=json!({"type":"object","additionalProperties":false,"required":["type","target"],"properties":{"type":{"enum":ROLES},"target":{"type":"string","minLength":1,"maxLength":81},"label":{"type":"string","minLength":1,"maxLength":256}}});
    json!({"type":"object","additionalProperties":false,"properties":{
        "landmarks":nullable(json!({"anyOf":[{"const":"auto"},{"const":false},{"type":"array","maxItems":MAX_MARKERS,"items":entry}]})),
        "pageList":nullable(json!({"enum":["auto",false]})),
        "paginationSource":nullable(json!({"anyOf":[{"const":false},{"type":"object","additionalProperties":false,"properties":{"identifier":{"type":"string","minLength":1,"maxLength":512},"description":{"type":"string","minLength":1,"maxLength":1024}},"anyOf":[{"type":"object","required":["identifier"]},{"type":"object","required":["description"]}]}]})),
        "labels":nullable(json!({"type":"object","additionalProperties":{"type":"string","minLength":1,"maxLength":256},"propertyNames":{"enum":["landmarks","pageList","cover","toc","bodymatter","titlepage","preface","bibliography","glossary","index"]}}))
    }})
}

#[cfg(test)]
#[path="publication_navigation_tests.rs"]
mod tests;
