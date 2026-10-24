//! Source-owned author text sizes. Processor/CSL formatting remains separate.
use crate::document_transaction::{self as tx, ContentTransaction, TransactionResult};
use crate::md_ast::{Block, Inline};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use std::ops::Range;

pub const INVALID: &str = "author_inline_text_invalid";
pub const UNSUPPORTED: &str = "author_inline_text_unsupported";
pub const TARGET: &str = "author_inline_text_target";
pub const BUDGET: &str = "author_inline_text_budget";
pub const MAX_NODES: usize = 32768;
pub const MAX_DEPTH: usize = 32;
pub const MAX_MARKER_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HalfPoints(u32);
impl HalfPoints {
    pub fn new(value:u32)->Result<Self,&'static str>{if value==0||value>i32::MAX as u32{Err(INVALID)}else{Ok(Self(value))}}
    pub const fn half_points(self)->u32{self.0}
    pub fn to_pt(self)->f64{self.0 as f64/2.}
    pub fn from_pt(value:f64)->Result<Self,&'static str>{
        let half=value*2.;if !half.is_finite()||half<=0.||half>i32::MAX as f64||half.fract()!=0.{return Err(INVALID);}Self::new(half as u32)
    }
}
impl Serialize for HalfPoints{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{if self.0%2==0{s.serialize_u32(self.0/2)}else{s.serialize_f64(self.to_pt())}}}
impl<'de> Deserialize<'de> for HalfPoints{fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{let value=f64::deserialize(d)?;Self::from_pt(value).map_err(serde::de::Error::custom)}}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CascadeSize{#[default] Inherit,Points(HalfPoints),BlockBase}
impl CascadeSize {
    pub fn from_value(value:&Value)->Result<Self,&'static str>{match value{Value::Null=>Ok(Self::Inherit),Value::String(v)if v=="base"=>Ok(Self::BlockBase),Value::Number(v)=>HalfPoints::from_pt(v.as_f64().ok_or(INVALID)?).map(Self::Points),_=>Err(INVALID)}}
    pub fn to_value(self)->Value{match self{Self::Inherit=>Value::Null,Self::Points(value)=>json!(value),Self::BlockBase=>json!("base")}}
}
impl Serialize for CascadeSize{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{self.to_value().serialize(s)}}
impl<'de> Deserialize<'de> for CascadeSize{fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{Self::from_value(&Value::deserialize(d)?).map_err(serde::de::Error::custom)}}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthorTextStyle{pub size:CascadeSize,pub size_cs:CascadeSize,pub cs:Option<bool>,pub rtl:Option<bool>}
#[derive(Deserialize,Default)]
#[serde(default,deny_unknown_fields,rename_all="camelCase")]
struct StyleWire{size:CascadeSize,size_cs:CascadeSize,cs:Option<bool>,rtl:Option<bool>}
impl Serialize for AuthorTextStyle{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{self.to_value().serialize(s)}}
impl<'de> Deserialize<'de> for AuthorTextStyle{fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
    struct ObjectOnly;
    impl<'de> serde::de::Visitor<'de> for ObjectOnly{
        type Value=AuthorTextStyle;
        fn expecting(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("an author text style object")}
        fn visit_map<A:serde::de::MapAccess<'de>>(self,map:A)->Result<Self::Value,A::Error>{let w=StyleWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;Ok(AuthorTextStyle{size:w.size,size_cs:w.size_cs,cs:w.cs,rtl:w.rtl})}
    }d.deserialize_map(ObjectOnly)
}}
impl AuthorTextStyle {
    pub fn is_empty(&self)->bool{self.size==CascadeSize::Inherit&&self.size_cs==CascadeSize::Inherit&&self.cs.is_none()&&self.rtl.is_none()}
    pub fn from_value(value:&Value)->Result<Self,&'static str>{if !value.is_object(){return Err(INVALID);}serde_json::from_value(value.clone()).map_err(|_|INVALID)}
    pub fn to_value(&self)->Value{
        let mut out=serde_json::Map::new();for(key,size)in[("size",self.size),("sizeCs",self.size_cs)]{if size!=CascadeSize::Inherit{out.insert(key.into(),size.to_value());}}
        if let Some(value)=self.cs{out.insert("cs".into(),json!(value));}if let Some(value)=self.rtl{out.insert("rtl".into(),json!(value));}Value::Object(out)
    }
    pub fn resolve(&self,parent:&ResolvedTextStyle,block_base:&ResolvedTextStyle)->ResolvedTextStyle{
        fn field(value:CascadeSize,parent:f64,origin:SizeOrigin,base:f64)->(f64,SizeOrigin){match value{CascadeSize::Inherit=>(parent,origin),CascadeSize::Points(value)=>(value.to_pt(),SizeOrigin::Author),CascadeSize::BlockBase=>(base,SizeOrigin::BlockBase)}}
        let(size,size_origin)=field(self.size,parent.size,parent.size_origin,block_base.size);let(size_cs,size_cs_origin)=field(self.size_cs,parent.size_cs,parent.size_cs_origin,block_base.size_cs);
        ResolvedTextStyle{size,size_cs,size_origin,size_cs_origin,cs:self.cs.or(parent.cs),rtl:self.rtl.or(parent.rtl)}
    }
    pub fn open_tag(&self)->String{
        let mut out=String::from("<span data-readmd-text=\"v1\"");for(key,size)in[("data-size-pt",self.size),("data-size-cs-pt",self.size_cs)]{match size{CascadeSize::Inherit=>(),CascadeSize::Points(value)=>out.push_str(&format!(" {key}=\"{}\"",value.to_pt())),CascadeSize::BlockBase=>out.push_str(&format!(" {key}=\"base\""))}}
        for(key,value)in[("data-cs",self.cs),("data-rtl",self.rtl)]{if let Some(value)=value{out.push_str(&format!(" {key}=\"{value}\""));}}out.push('>');out
    }
    pub fn wrap_source(&self,content:&str)->String{if self.is_empty(){content.into()}else{format!("{}{content}</span>",self.open_tag())}}
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all="camelCase")]
pub enum SizeOrigin{#[default] Inherited,Author,BlockBase}
impl SizeOrigin{pub fn is_explicit(self)->bool{self!=Self::Inherited}}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all="camelCase")]
pub struct ResolvedTextStyle{pub size:f64,pub size_cs:f64,pub size_origin:SizeOrigin,pub size_cs_origin:SizeOrigin,pub cs:Option<bool>,pub rtl:Option<bool>}
impl ResolvedTextStyle{pub fn base(size:f64,size_cs:f64)->Self{Self{size,size_cs,size_origin:SizeOrigin::Inherited,size_cs_origin:SizeOrigin::Inherited,cs:None,rtl:None}}}

#[derive(Debug,Clone,PartialEq)]
enum SpanToken{Open(Option<AuthorTextStyle>),Close,Other}
fn span_token(raw:&str)->Result<SpanToken,&'static str>{
    let text=raw.trim();if !text.starts_with('<')||text.starts_with("<!")||text.starts_with("<?"){return Ok(SpanToken::Other);}
    if text.len()>MAX_MARKER_BYTES&&text.to_ascii_lowercase().contains("data-readmd-text"){return Err(BUDGET);}
    let lower=text.to_ascii_lowercase();if lower.starts_with("</"){
        return Ok(if lower[2..].trim_end_matches('>').trim()=="span"&&lower.ends_with('>'){SpanToken::Close}else{SpanToken::Other});
    }
    let reserved=lower.contains("data-readmd-text");
    let parse=||->Result<(String,Vec<(String,String)>,bool),&'static str>{
        let bytes=text.as_bytes();let mut at=1;while at<bytes.len()&&bytes[at].is_ascii_alphanumeric(){at+=1;}if at==1{return Err(INVALID);}let tag=text[1..at].to_ascii_lowercase();let mut attrs=Vec::new();let mut end=false;let mut self_closing=false;
        while at<bytes.len(){
            while at<bytes.len()&&bytes[at].is_ascii_whitespace(){at+=1;}if at==bytes.len(){break;}
            if bytes[at]==b'>'{at+=1;end=true;break;}if bytes[at]==b'/'&&bytes.get(at+1)==Some(&b'>'){at+=2;end=true;self_closing=true;break;}
            let start=at;while at<bytes.len()&&(bytes[at].is_ascii_alphanumeric()||b"_:-".contains(&bytes[at])){at+=1;}if start==at{return Err(INVALID);}let key=text[start..at].to_ascii_lowercase();
            while at<bytes.len()&&bytes[at].is_ascii_whitespace(){at+=1;}if bytes.get(at)!=Some(&b'='){return Err(INVALID);}at+=1;while at<bytes.len()&&bytes[at].is_ascii_whitespace(){at+=1;}
            let quote=*bytes.get(at).ok_or(INVALID)?;if !matches!(quote,b'\''|b'"'){return Err(INVALID);}at+=1;let start=at;while at<bytes.len()&&bytes[at]!=quote{at+=1;}if at==bytes.len(){return Err(INVALID);}let value=text[start..at].to_string();at+=1;if bytes.get(at).is_some_and(|byte|!byte.is_ascii_whitespace()&&*byte!=b'>'&&*byte!=b'/'){return Err(INVALID);}attrs.push((key,value));
        }if !end||!text[at..].trim().is_empty(){return Err(INVALID);}Ok((tag,attrs,self_closing))
    };
    let(tag,attrs,self_closing)=match parse(){Ok(value)=>value,Err(code)=>return if reserved{Err(code)}else{Ok(if lower.strip_prefix("<span").is_some_and(|tail|tail.starts_with(char::is_whitespace)||tail.starts_with('>'))&&!lower.ends_with("/>"){SpanToken::Open(None)}else{SpanToken::Other})}};
    let owns=attrs.iter().any(|(key,_)|key=="data-readmd-text");if !owns{return Ok(if tag=="span"&&!self_closing{SpanToken::Open(None)}else{SpanToken::Other});}
    if tag!="span"||self_closing{return Err(INVALID);}let mut seen=std::collections::HashSet::new();let mut style=AuthorTextStyle::default();
    for(key,value)in attrs{if !seen.insert(key.clone()){return Err(INVALID);}match key.as_str(){
        "data-readmd-text" if value=="v1"=>(),
        "data-size-pt"|"data-size-cs-pt"=>{let size=if value=="base"{CascadeSize::BlockBase}else{CascadeSize::Points(HalfPoints::from_pt(value.parse::<f64>().map_err(|_|INVALID)?)?)};if key=="data-size-pt"{style.size=size}else{style.size_cs=size}},
        "data-cs"|"data-rtl"=>{let value=match value.as_str(){"true"=>true,"false"=>false,_=>return Err(INVALID)};if key=="data-cs"{style.cs=Some(value)}else{style.rtl=Some(value)}},
        _=>return Err(INVALID),
    }}Ok(SpanToken::Open(Some(style)))
}
pub fn read_open_tag(raw:&str)->Result<Option<AuthorTextStyle>,&'static str>{match span_token(raw)?{SpanToken::Open(style)=>Ok(style),_=>Ok(None)}}

/// Parse owned inline spans even when a first-run span would otherwise become
/// a CommonMark type-7 HTML block. The temporary prefix never reaches source,
/// text events or their ranges; genuine code/HTML examples remain opaque.
/// All returned byte ranges include the original BOM, when present.
pub fn has_owned_syntax(source:&str)->bool{
    const MARKER:&[u8]=b"data-readmd-text";
    source.as_bytes().windows(MARKER.len()).any(|bytes|bytes.eq_ignore_ascii_case(MARKER))
}
pub fn markdown_events<'a>(source:&'a str)->Result<Vec<(Event<'a>,Range<usize>)>,&'static str>{
    if source.len()>tx::MAX_DOCUMENT_BYTES{return Err(BUDGET);}
    let bom=usize::from(source.starts_with('\u{feff}'))*3;
    fn unchanged<'a>(source:&'a str,bom:usize)->Result<Vec<(Event<'a>,Range<usize>)>,&'static str>{
        let mut events=Vec::new();for(event,range)in Parser::new_ext(&source[bom..],crate::md_ast::options()).into_offset_iter(){if crate::speech::cancelled(){return Err("cancelled");}if events.len()>=MAX_NODES*16{return Err(BUDGET);}events.push((event,range.start+bom..range.end+bom));}Ok(events)
    }
    if !has_owned_syntax(source){return unchanged(source,bom);}
    let mut first_html=false;let mut starts=Vec::new();let mut count=0usize;
    for(event,range)in Parser::new_ext(&source[bom..],crate::md_ast::options()).into_offset_iter(){
        if crate::speech::cancelled(){return Err("cancelled");}count+=1;if count>MAX_NODES*16{return Err(BUDGET);}
        match event{
            Event::Start(Tag::HtmlBlock)=>first_html=true,
            Event::End(TagEnd::HtmlBlock)=>first_html=false,
            Event::Html(raw)if first_html=>{
                first_html=false;let text=raw.trim_start();let lower=text.to_ascii_lowercase();
                if !lower.strip_prefix("<span").is_some_and(|tail|tail.starts_with(char::is_whitespace)||tail.starts_with('>')){continue;}
                // Read one tag only: the HTML block event may include its body
                // and closing tag. Quoted '>' characters do not close a tag.
                let mut quote=None;let mut end=None;
                for(at,byte)in text.bytes().enumerate(){
                    if let Some(current)=quote{if byte==current{quote=None;}}
                    else if matches!(byte,b'\''|b'"'){quote=Some(byte);}
                    else if byte==b'>'{end=Some(at+1);break;}
                }
                let end=end.unwrap_or(text.len());
                if !text[..end].to_ascii_lowercase().contains("data-readmd-text"){continue;}
                if read_open_tag(&text[..end])?.is_some(){
                    starts.push(range.start+bom+(raw.len()-text.len()));
                    if starts.len()>MAX_NODES{return Err(BUDGET);}
                }
            },_=>(),
        }
    }
    if starts.is_empty(){return unchanged(source,bom);}
    const PREFIX:&str="\u{2060}";
    starts.sort_unstable();starts.dedup();let mut instrumented=String::with_capacity(source.len()+starts.len()*PREFIX.len());let mut inserted=Vec::with_capacity(starts.len());let mut cursor=0;
    for at in starts{instrumented.push_str(&source[cursor..at]);let start=instrumented.len();instrumented.push_str(PREFIX);inserted.push((at,start..instrumented.len()));cursor=at;}instrumented.push_str(&source[cursor..]);
    let original=|at:usize|{let done=inserted.partition_point(|(_,range)|range.end<=at);if let Some((source_at,range))=inserted.get(done){if range.start<=at{return *source_at;}}at-done*PREFIX.len()};
    let mut events=Vec::new();
    for(event,range)in Parser::new_ext(&instrumented[bom..],crate::md_ast::options()).into_offset_iter(){
        if crate::speech::cancelled(){return Err("cancelled");}if events.len()>MAX_NODES*16{return Err(BUDGET);}
        let range=range.start+bom..range.end+bom;
        let event=if let Event::Text(value)=event{
            let mut value=value.into_string();let first=inserted.partition_point(|(_,insertion)|insertion.end<=range.start);
            for(_,insertion)in inserted[first..].iter().take_while(|(_,insertion)|insertion.start<range.end){
                if insertion.start<range.start||insertion.end>range.end||!value.ends_with(PREFIX){return Err(INVALID);}
                value.truncate(value.len()-PREFIX.len());
            }
            if value.is_empty(){continue;}Event::Text(value.into())
        }else{event.into_static()};
        let range=original(range.start)..original(range.end);if range.is_empty()&&matches!(event,Event::Text(_)){continue;}events.push((event,range));
    }Ok(events)
}

#[derive(Debug,Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SourceNode{
    pub id:String,pub kind:String,pub from:usize,pub to:usize,pub content_from:usize,pub content_to:usize,pub style:AuthorTextStyle,pub parent:Option<String>,pub digest:String,
    #[serde(skip)]pub bytes:Range<usize>,#[serde(skip)]pub content_bytes:Range<usize>,#[serde(skip)]pub opening:Option<Range<usize>>,#[serde(skip)]pub closing:Option<Range<usize>>,
}
#[derive(Debug,Clone,Serialize)]
pub struct SourceDocument{pub revision:String,pub nodes:Vec<SourceNode>}
#[derive(Debug,Clone,Serialize)]
pub struct EditResult{pub transaction:ContentTransaction,pub result:TransactionResult}
#[derive(Clone)]
struct SpanRange{open:Range<usize>,close:Range<usize>,style:AuthorTextStyle,container:usize}
#[derive(Clone)]
struct Leaf{bytes:Range<usize>,container:usize,atomic:bool,math:bool}
struct Analysis{document:SourceDocument,spans:Vec<SpanRange>,leaves:Vec<Leaf>,edges:Vec<(Range<usize>,usize)>}
struct OpenSpan{open:Range<usize>,style:Option<AuthorTextStyle>,container:usize,formats:Vec<usize>}
fn utf16(source:&str,byte:usize)->usize{source[..byte].encode_utf16().count()}
fn range_node(source:&str,offsets:&std::collections::HashMap<usize,usize>,kind:&str,bytes:Range<usize>,content:Range<usize>,style:AuthorTextStyle,opening:Option<Range<usize>>,closing:Option<Range<usize>>)->SourceNode{
    let digest=tx::revision(&source[bytes.clone()]);SourceNode{id:format!("inline-{}",&tx::revision(&format!("{kind}:{}:{}:{digest}",bytes.start,bytes.end))[..24]),kind:kind.into(),from:offsets[&bytes.start],to:offsets[&bytes.end],content_from:offsets[&content.start],content_to:offsets[&content.end],style,parent:None,digest,bytes,content_bytes:content,opening,closing}
}
fn analyze(source:&str)->Result<Analysis,&'static str>{
    if source.len()>tx::MAX_DOCUMENT_BYTES{return Err(BUDGET);}
    let mut spans=Vec::new();let mut leaves=Vec::new();let mut stack=Vec::<OpenSpan>::new();let mut blocks=Vec::<(TagEnd,usize)>::new();let mut formats=Vec::<usize>::new();let mut serial=0usize;let mut blocked=0usize;let mut html_block=false;let mut html_pending=false;let mut image_depth=0usize;let mut steps=0usize;
    let mut eligible=Vec::<Range<usize>>::new();let mut recognized=std::collections::HashSet::new();let mut edges=Vec::new();
    for(event,range)in markdown_events(source)?{
        if crate::speech::cancelled(){return Err("cancelled");}steps+=1;if steps>MAX_NODES*16{return Err(BUDGET);}
        let container=blocks.last().map(|(_,id)|*id).unwrap_or(0);
        match event{
            Event::Start(tag)=>{
                serial=serial.checked_add(1).ok_or(BUDGET)?;
                match &tag{
                    Tag::CodeBlock(_)|Tag::MetadataBlock(_)=>blocked+=1,
                    Tag::HtmlBlock=>{html_block=true;html_pending=true;},
                    Tag::Paragraph|Tag::Heading{..}|Tag::TableCell|Tag::Item|Tag::FootnoteDefinition(_)=>blocks.push((tag.to_end(),serial)),
                    Tag::Emphasis|Tag::Strong|Tag::Strikethrough|Tag::Link{..}|Tag::Image{..}=>{if matches!(&tag,Tag::Image{..}){image_depth+=1;}formats.push(serial);if container!=0&&blocked==0{edges.push((range.clone(),container));}},_=>(),
                }
            },
            Event::End(end)=>{
                if matches!(end,TagEnd::CodeBlock|TagEnd::MetadataBlock(_)){blocked=blocked.saturating_sub(1);}
                if end==TagEnd::HtmlBlock{html_block=false;}
                if matches!(end,TagEnd::Emphasis|TagEnd::Strong|TagEnd::Strikethrough|TagEnd::Link|TagEnd::Image){formats.pop();}
                if end==TagEnd::Image{image_depth=image_depth.saturating_sub(1);}
                if blocks.last().is_some_and(|(tag,_)|*tag==end){let(_,id)=blocks.pop().unwrap();if stack.iter().any(|span|span.container==id&&span.style.is_some()){return Err(INVALID);}stack.retain(|span|span.container!=id);}
            },
            Event::InlineHtml(raw)|Event::Html(raw)if blocked==0=>{
                if html_block{if html_pending{html_pending=false;let raw=raw.trim_start().to_ascii_lowercase();if raw.starts_with("<span")&&raw.contains("data-readmd-text"){return Err(INVALID);}}continue;}
                if raw.trim().starts_with("<!--"){continue;}
                let token=span_token(&raw)?;if image_depth>0&&matches!(token,SpanToken::Open(Some(_))){return Err(UNSUPPORTED);}if html_block||container==0{
                    if matches!(token,SpanToken::Open(Some(_))){return Err(INVALID);}continue;
                }
                eligible.push(range.clone());match token{
                    SpanToken::Open(style)=>{if stack.len()>=MAX_DEPTH{return Err(BUDGET);}if style.is_some(){recognized.insert(range.start);}stack.push(OpenSpan{open:range,style,container,formats:formats.clone()});},
                    SpanToken::Close=>{if let Some(open)=stack.pop(){if open.style.is_some()&&(open.container!=container||open.formats!=formats){return Err(INVALID);}if let Some(style)=open.style{spans.push(SpanRange{open:open.open,close:range,style,container});}}},SpanToken::Other=>(),
                }
            },
            Event::Text(_)if blocked==0&&!html_block&&image_depth==0&&container!=0=>{eligible.push(range.clone());leaves.push(Leaf{bytes:range,container,atomic:false,math:false});},
            Event::InlineMath(_)|Event::DisplayMath(_)if blocked==0&&!html_block&&image_depth==0&&container!=0=>leaves.push(Leaf{bytes:range,container,atomic:true,math:true}),
            Event::Code(_)|Event::FootnoteReference(_)if blocked==0&&!html_block&&image_depth==0&&container!=0=>leaves.push(Leaf{bytes:range,container,atomic:true,math:false}),
            _=>(),
        }
        if spans.len()+leaves.len()>MAX_NODES{return Err(BUDGET);}
    }
    if stack.iter().any(|span|span.style.is_some()){return Err(INVALID);}
    // A malformed reserved opener may have been emitted as plain text by the
    // Markdown parser. Check only live text/HTML ranges; code, comments and
    // escaped examples never authorize a marker.
    eligible.sort_by_key(|range|range.start);let lower=source.to_ascii_lowercase();let bytes=lower.as_bytes();
    let mut markers=lower.match_indices("data-readmd-text").peekable();let mut eligible_at=0usize;let mut delimiter_end=0usize;let mut scan_steps=0usize;
    let mut scan_step=||->Result<(),&'static str>{scan_steps+=1;if scan_steps>MAX_NODES*16{return Err(BUDGET);}if scan_steps%256==1&&crate::speech::cancelled(){return Err("cancelled");}Ok(())};
    for(at,_)in lower.match_indices("<span"){
        scan_step()?;
        while eligible_at<eligible.len()&&eligible[eligible_at].start<=at{eligible_at+=1;}
        if eligible_at==0||!eligible[eligible_at-1].contains(&at){continue;}let slashes=source[..at].bytes().rev().take_while(|byte|*byte==b'\\').count();if slashes%2!=0{continue;}
        while markers.peek().is_some_and(|(position,_)|*position<at){markers.next();scan_step()?;}
        let Some(&(marker_at,_))=markers.peek()else{break;};
        // Every delimiter segment is scanned once, even if it contains many
        // malformed unreserved openers. Marker positions also move only ahead.
        // Byte cursors preserve late markers without slicing a UTF-8 scalar.
        if delimiter_end<=at{
            delimiter_end=at;
            while delimiter_end<bytes.len()&&!matches!(bytes[delimiter_end],b'>'|b'\r'|b'\n'){
                delimiter_end+=1;if delimiter_end%16384==0&&crate::speech::cancelled(){return Err("cancelled");}
            }
        }
        if marker_at+"data-readmd-text".len()<=delimiter_end&&!recognized.contains(&at){return Err(if delimiter_end-at>MAX_MARKER_BYTES{BUDGET}else{INVALID});}
    }
    spans.sort_by_key(|span|(span.open.start,std::cmp::Reverse(span.close.end)));
    let mut targets=std::collections::HashSet::new();for span in &spans{targets.extend([span.open.start,span.open.end,span.close.start,span.close.end]);}for leaf in &leaves{targets.extend([leaf.bytes.start,leaf.bytes.end]);}
    let mut offsets=std::collections::HashMap::with_capacity(targets.len());let mut units=0;for(at,ch)in source.char_indices(){if targets.contains(&at){offsets.insert(at,units);}units+=ch.len_utf16();}offsets.insert(source.len(),units);
    let mut nodes=Vec::new();for span in &spans{nodes.push(range_node(source,&offsets,"span",span.open.start..span.close.end,span.open.end..span.close.start,span.style,Some(span.open.clone()),Some(span.close.clone())));}
    for leaf in &leaves{nodes.push(range_node(source,&offsets,if leaf.atomic{"atom"}else{"text"},leaf.bytes.clone(),leaf.bytes.clone(),AuthorTextStyle::default(),None,None));}
    nodes.sort_by_key(|node|(node.bytes.start,std::cmp::Reverse(node.bytes.end)));
    let mut parents=Vec::<usize>::new();for at in 0..nodes.len(){let range=nodes[at].bytes.clone();while parents.last().is_some_and(|index|nodes[*index].content_bytes.start>range.start||range.end>nodes[*index].content_bytes.end){parents.pop();}nodes[at].parent=parents.last().map(|index|nodes[*index].id.clone());if nodes[at].kind=="span"{parents.push(at);}}
    Ok(Analysis{document:SourceDocument{revision:tx::revision(source),nodes},spans,leaves,edges})
}
pub fn get(source:&str)->Result<SourceDocument,&'static str>{analyze(source).map(|analysis|analysis.document)}
/// Consumer validation is deliberately separate from structural `get`: users
/// must still be able to inspect and clear an unsupported source combination.
pub(crate) fn consumer_document(source:&str)->Result<SourceDocument,&'static str>{
    let analysis=analyze(source)?;
    // Every nonempty ancestor is significant, including an outer property
    // inherited through an otherwise empty owned span. Merge these real
    // parser ranges so a large nested document stays bounded and linear here.
    let mut scopes=Vec::<Range<usize>>::new();
    for span in analysis.spans.iter().filter(|span|!span.style.is_empty()){
        let range=span.open.end..span.close.start;
        if let Some(last)=scopes.last_mut().filter(|last|range.start<=last.end){last.end=last.end.max(range.end);}else{scopes.push(range);}
    }
    if scopes.is_empty(){return Ok(analysis.document);}
    let overlaps=|range:&Range<usize>|{
        let at=scopes.partition_point(|scope|scope.end<=range.start);
        scopes.get(at).is_some_and(|scope|scope.start<range.end&&range.start<scope.end)
    };
    // These are the same typed leaves admitted by analyze: code, raw HTML and
    // image alt text never become mathematical source atoms. A current OMML
    // capsule has a real dollar carrier and is protected by that atom as well.
    if analysis.leaves.iter().any(|leaf|leaf.math&&overlaps(&leaf.bytes)){return Err(UNSUPPORTED);}
    // Live DSL discovery has its own source-aware exclusions for examples,
    // links, images and HTML. Do not infer a formula by scanning its spelling.
    for math in crate::math_dsl::markdown_ranges(source)?{
        if crate::speech::cancelled(){return Err("cancelled");}
        if overlaps(&(math.from..math.to)){return Err(UNSUPPORTED);}
    }
    // A first owned span is live inline source according to markdown_events,
    // although the ordinary DSL parser sees a type-7 HTML block. Give only
    // real owned openers a temporary running-text prefix, keeping the full
    // surrounding link/code/HTML context. Map discovery back to raw Text
    // leaves; prefixes and decoded entities never authorize a formula.
    let text_leaves=analysis.leaves.iter().filter(|leaf|!leaf.atomic).collect::<Vec<_>>();
    let mut syntax=String::with_capacity(source.len()+analysis.spans.len()*2);
    let mut inserted=Vec::with_capacity(analysis.spans.len());let mut cursor=0;
    for span in &analysis.spans{
        if crate::speech::cancelled(){return Err("cancelled");}
        syntax.push_str(&source[cursor..span.open.start]);syntax.push_str("A ");inserted.push(syntax.len());cursor=span.open.start;
    }
    syntax.push_str(&source[cursor..]);
    let original=|at:usize|at-inserted.partition_point(|end|*end<=at)*2;
    for math in crate::math_dsl::markdown_ranges(&syntax)?{
        if crate::speech::cancelled(){return Err("cancelled");}
        let from=original(math.from);let to=original(math.to);
        let at=text_leaves.partition_point(|leaf|leaf.bytes.end<=from);
        if text_leaves.get(at).is_some_and(|leaf|leaf.bytes.contains(&from))&&overlaps(&(from..to)){return Err(UNSUPPORTED);}
    }
    Ok(analysis.document)
}
pub fn validate_source(source:&str)->Result<(),&'static str>{if !has_owned_syntax(source){return Ok(());}consumer_document(source).map(|_|())}

fn attach_values(values:&mut Vec<Inline>)->Result<(),&'static str>{
    let mut input=values.clone();for value in &mut input{match value{Inline::Strong(children)|Inline::Emph(children)|Inline::Strike(children)|Inline::Styled{children,..}|Inline::Display{children,..}|Inline::Citation{children,..}|Inline::AuthorStyled{children,..}|Inline::Link{children,..}=>attach_values(children)?,_=>(),}}
    let mut out=Vec::new();let mut at=0;while at<input.len(){
        let style=if let Inline::Html(raw)=&input[at]{read_open_tag(raw)?}else{None};
        if let Some(style)=style{
            let mut depth=1usize;let mut end=at+1;for value in &input[at+1..]{if let Inline::Html(raw)=value{match span_token(raw)?{SpanToken::Open(_)=>depth+=1,SpanToken::Close=>{depth-=1;if depth==0{break;}},_=>(),}}end+=1;}
            if end==input.len()||depth!=0{return Err(INVALID);}let mut children=input[at+1..end].to_vec();attach_values(&mut children)?;out.push(Inline::AuthorStyled{style,children});at=end+1;
        }else{out.push(input[at].clone());at+=1;}
    }*values=out;Ok(())
}
pub fn attach(blocks:&mut [Block])->Result<(),&'static str>{
    for block in blocks{match block{
        Block::Styled{block,..}=>attach(std::slice::from_mut(block.as_mut()))?,
        Block::Paragraph(values)|Block::Heading{inlines:values,..}|Block::Caption{inlines:values,..}=>attach_values(values)?,
        Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>attach(blocks)?,Block::List{items,..}=>for item in items{attach(&mut item.blocks)?;},
        Block::Table{header,rows,..}=>for row in std::iter::once(header).chain(rows){for cell in row{attach_values(cell)?;}},_=>(),
    }}Ok(())
}
pub fn has_author_styles(blocks:&[Block])->bool{let mut found=false;crate::md_ast::walk_inlines(blocks,&mut|inline|if matches!(inline,Inline::AuthorStyled{..}){found=true;});found}

fn patch_style(mut style:AuthorTextStyle,patch:&Value)->Result<AuthorTextStyle,&'static str>{
    let object=patch.as_object().ok_or(INVALID)?;for(key,value)in object{match key.as_str(){
        "size"=>style.size=CascadeSize::from_value(value)?,"sizeCs"=>style.size_cs=CascadeSize::from_value(value)?,
        "cs"=>style.cs=if value.is_null(){None}else{Some(value.as_bool().ok_or(INVALID)?)},
        "rtl"=>style.rtl=if value.is_null(){None}else{Some(value.as_bool().ok_or(INVALID)?)},_=>return Err(INVALID),
    }}Ok(style)
}
fn boundary(source:&str,leaf:&Leaf,at:usize)->bool{
    if at<leaf.bytes.start||at>leaf.bytes.end{return false;}if leaf.atomic{return at==leaf.bytes.start||at==leaf.bytes.end;}
    let raw=&source[leaf.bytes.clone()];let target=at-leaf.bytes.start;let mut index=0;
    while index<raw.len(){if index==target{return true;}let start=index;let ch=raw[index..].chars().next().unwrap();index+=ch.len_utf8();
        if ch=='\\'{if let Some(next)=raw[index..].chars().next().filter(|ch|ch.is_ascii_punctuation()){index+=next.len_utf8();}}
        else if ch=='&'{if let Some(end)=raw[index..].find(';').filter(|end|*end<=64){let name=&raw[index..index+end];if !name.is_empty()&&name.bytes().all(|byte|byte.is_ascii_alphanumeric()||byte==b'#'){index+=end+1;}}}
        if start<target&&target<index{return false;}
    }target==raw.len()
}
fn selection_container(source:&str,analysis:&Analysis,range:&Range<usize>)->Result<usize,&'static str>{
    if range.start>=range.end{return Err(TARGET);}for at in [range.start,range.end]{if at>0&&source.as_bytes().get(at-1)==Some(&b'\r')&&source.as_bytes().get(at)==Some(&b'\n'){return Err("transaction_invalid_range");}}
    let ends=|at|{let mut ids=std::collections::HashSet::new();for leaf in &analysis.leaves{if boundary(source,leaf,at){ids.insert(leaf.container);}}for(edge,id)in &analysis.edges{if at==edge.start||at==edge.end{ids.insert(*id);}}for span in &analysis.spans{if [span.open.start,span.open.end,span.close.start,span.close.end].contains(&at){ids.insert(span.container);}}ids};
    let start=ends(range.start);let end=ends(range.end);start.intersection(&end).copied().next().ok_or(TARGET)
}
fn strip_authors(blocks:&mut [Block]){
    fn normalize_values(values:&mut Vec<Inline>){let mut out=Vec::new();for mut inline in std::mem::take(values){match &mut inline{
        Inline::AuthorStyled{children,..}=>{normalize_values(children);for child in std::mem::take(children){append(&mut out,child);}continue;},
        Inline::Strong(children)|Inline::Emph(children)|Inline::Strike(children)|Inline::Styled{children,..}|Inline::Display{children,..}|Inline::Citation{children,..}|Inline::Link{children,..}=>normalize_values(children),_=>(),
    }append(&mut out,inline);}*values=out;}
    fn append(values:&mut Vec<Inline>,inline:Inline){if let Inline::Text(text)=&inline{if let Some(Inline::Text(previous))=values.last_mut(){previous.push_str(text);return;}}values.push(inline);}
    for block in blocks{match block{Block::Styled{block,..}=>strip_authors(std::slice::from_mut(block.as_mut())),Block::Paragraph(inlines)|Block::Heading{inlines,..}|Block::Caption{inlines,..}=>normalize_values(inlines),Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>strip_authors(blocks),Block::List{items,..}=>for item in items{strip_authors(&mut item.blocks);},Block::Table{header,rows,..}=>for row in std::iter::once(header).chain(rows){for cell in row{normalize_values(cell);}},_=>(),}}
}
fn semantic_source(source:&str)->crate::md_ast::Document{let mut doc=crate::md_ast::parse(source);strip_authors(&mut doc.blocks);doc}
fn transaction(source:&str,expected:&str,edits:Vec<tx::TextEdit>)->Result<ContentTransaction,&'static str>{
    let plan=ContentTransaction{base_revision:expected.into(),origin:"editor".into(),edits};let result=tx::apply(source,&plan)?;
    validate_source(&result.content).map_err(|_|TARGET)?;if result.changed&&semantic_source(source)!=semantic_source(&result.content){return Err(TARGET);}Ok(plan)
}
/// Source coordinates are UTF-16, never normalized display offsets. A whole
/// existing span (or its complete content) patches that level without touching
/// child overrides. Partial edits use a new nested level; null deletes a local
/// property by splitting the containing source span when representable.
pub fn plan_apply(source:&str,base_revision:&str,from:usize,to:usize,patch:&Value)->Result<ContentTransaction,&'static str>{
    if base_revision!=tx::revision(source){return Err("transaction_conflict");}let next=patch_style(AuthorTextStyle::default(),patch)?;let analysis=analyze(source)?;
    let start=tx::byte_offset(source,from)?;let end=tx::byte_offset(source,to)?;if start>=end{return Err(TARGET);}let selected=start..end;
    if let Some(span)=analysis.spans.iter().rev().find(|span|selected==(span.open.start..span.close.end)||selected==(span.open.end..span.close.start)){
        let updated=patch_style(span.style,patch)?;if updated==span.style{return transaction(source,base_revision,Vec::new());}
        let edits=vec![tx::TextEdit{from:utf16(source,span.open.start),to:utf16(source,span.open.end),insert:if updated.is_empty(){String::new()}else{updated.open_tag()}},tx::TextEdit{from:utf16(source,span.close.start),to:utf16(source,span.close.end),insert:if updated.is_empty(){String::new()}else{"</span>".into()}}];return transaction(source,base_revision,edits);
    }
    let container=selection_container(source,&analysis,&selected)?;
    if let Some(span)=analysis.spans.iter().rev().find(|span|span.container==container&&span.open.end<=start&&end<=span.close.start){
        let updated=patch_style(span.style,patch)?;if updated==span.style{return transaction(source,base_revision,Vec::new());}
        if patch.as_object().unwrap().values().any(Value::is_null){
            let mut replacement=String::new();for(content,style)in[(&source[span.open.end..start],span.style),(&source[start..end],updated),(&source[end..span.close.start],span.style)]{if !content.is_empty(){replacement.push_str(&style.wrap_source(content));}}
            return transaction(source,base_revision,vec![tx::TextEdit{from:utf16(source,span.open.start),to:utf16(source,span.close.end),insert:replacement}]);
        }
    }
    // A null patch must never report success while leaving selected authored
    // properties behind. A range crossing independent scopes needs an explicit
    // safe scope selection rather than a guessed Markdown reconstruction.
    if patch.as_object().unwrap().values().any(Value::is_null)&&analysis.spans.iter().any(|span|span.open.start<end&&start<span.close.end){return Err(TARGET);}
    if next.is_empty(){return transaction(source,base_revision,Vec::new());}
    transaction(source,base_revision,vec![tx::TextEdit{from,to,insert:next.wrap_source(&source[selected])}])
}
pub fn apply(source:&str,base_revision:&str,from:usize,to:usize,patch:&Value)->Result<EditResult,&'static str>{let transaction=plan_apply(source,base_revision,from,to,patch)?;let result=tx::apply(source,&transaction)?;Ok(EditResult{transaction,result})}
pub fn clear(source:&str,base_revision:&str,from:usize,to:usize,fields:&[&str],to_block_base:bool)->Result<EditResult,&'static str>{
    if fields.is_empty()||fields.len()>4{return Err(INVALID);}let mut patch=serde_json::Map::new();for field in fields{if !matches!(*field,"size"|"sizeCs"|"cs"|"rtl")||patch.contains_key(*field)||to_block_base&&matches!(*field,"cs"|"rtl"){return Err(INVALID);}patch.insert((*field).into(),if to_block_base{json!("base")}else{Value::Null});}apply(source,base_revision,from,to,&Value::Object(patch))
}
pub fn handle(request:&Value)->Result<Value,&'static str>{
    let object=request.as_object().ok_or(INVALID)?;let action=object.get("action").and_then(Value::as_str).ok_or(INVALID)?;let content=object.get("content").and_then(Value::as_str).ok_or(INVALID)?;
    let allowed:&[&str]=match action{"get"=>&["action","content"],"apply"=>&["action","content","base_revision","from","to","patch"],"clear"=>&["action","content","base_revision","from","to","fields","to_block_base"],_=>return Err(INVALID)};if object.keys().any(|key|!allowed.contains(&key.as_str())){return Err(INVALID);}
    if action=="get"{let document=get(content)?;return Ok(json!({"ok":true,"revision":document.revision,"nodes":document.nodes}));}
    let expected=object.get("base_revision").and_then(Value::as_str).ok_or(INVALID)?;let coordinate=|key:&str|object.get(key).and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).ok_or(INVALID);let from=coordinate("from")?;let to=coordinate("to")?;
    let edit=if action=="apply"{apply(content,expected,from,to,object.get("patch").ok_or(INVALID)?)?}else{
        let fields=if let Some(value)=object.get("fields"){value.as_array().ok_or(INVALID)?.iter().map(|value|value.as_str().ok_or(INVALID)).collect::<Result<Vec<_>,_>>()?}else{vec!["size","sizeCs"]};let base=object.get("to_block_base").map(|value|value.as_bool().ok_or(INVALID)).transpose()?.unwrap_or(false);clear(content,expected,from,to,&fields,base)?
    };Ok(json!({"ok":true,"transaction":edit.transaction,"result":edit.result}))
}
pub fn request_schema()->Value{
    let coordinates=json!({"content":{"type":"string"},"base_revision":{"type":"string"},"from":{"type":"integer","minimum":0},"to":{"type":"integer","minimum":0}});
    let size=json!({"anyOf":[{"type":"number","minimum":0.5,"maximum":i32::MAX as f64/2.,"multipleOf":0.5},{"type":"string","enum":["base"]},{"type":"null"}]});
    let mut apply=coordinates.as_object().unwrap().clone();apply.insert("action".into(),json!({"type":"string","const":"apply"}));apply.insert("patch".into(),json!({"type":"object","additionalProperties":false,"properties":{"size":size,"sizeCs":size,"cs":{"type":["boolean","null"]},"rtl":{"type":["boolean","null"]}}}));
    let mut clear=coordinates.as_object().unwrap().clone();clear.insert("action".into(),json!({"type":"string","const":"clear"}));clear.insert("fields".into(),json!({"type":"array","minItems":1,"maxItems":4,"uniqueItems":true,"items":{"type":"string","enum":["size","sizeCs","cs","rtl"]}}));clear.insert("to_block_base".into(),json!({"type":"boolean"}));
    json!({"type":"object","oneOf":[{"type":"object","additionalProperties":false,"required":["action","content"],"properties":{"action":{"type":"string","const":"get"},"content":{"type":"string"}}},{"type":"object","additionalProperties":false,"required":["action","content","base_revision","from","to","patch"],"properties":apply},{"type":"object","additionalProperties":false,"required":["action","content","base_revision","from","to"],"properties":clear}]})
}

#[cfg(test)]
#[path="publication_inline_text_tests.rs"]
mod tests;

#[cfg(test)]
#[path="publication_inline_parser_tests.rs"]
mod parser_tests;

