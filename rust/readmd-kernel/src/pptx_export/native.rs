//! Compile-time, sanitized native template packages. Their artwork, masters,
//! images and safe artwork animation remain OOXML; Markdown stays editable.
use super::*;
use crate::math_document::{XmlNode, XmlPart};
use serde::Deserialize;
use quick_xml::{events::Event, Reader, XmlVersion};
use std::{collections::BTreeSet, sync::OnceLock};

mod previews {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/pptx/previews.rs"));
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Template { Pearl, Field, Chrome, Confluence, Lightfield }

impl Template {
    pub(super) fn resolve(id: &str) -> Option<Self> {
        Some(match id {
            "pptx-pearl" => Self::Pearl, "pptx-field" => Self::Field,
            "pptx-chrome" => Self::Chrome, "pptx-confluence" => Self::Confluence,
            "pptx-lightfield" => Self::Lightfield, _ => return None,
        })
    }
    pub(super) fn id(self) -> &'static str {
        match self { Self::Pearl=>"pptx-pearl",Self::Field=>"pptx-field",Self::Chrome=>"pptx-chrome",Self::Confluence=>"pptx-confluence",Self::Lightfield=>"pptx-lightfield" }
    }
    pub(super) fn palette(self) -> (&'static str,&'static str,&'static str) {
        match self {
            Self::Pearl => ("FFFFFF","282830","A78D70"),
            Self::Field => ("F3F5F1","18251C","C6F34A"),
            Self::Chrome => ("080A0E","F2F4FF","ADA5F5"),
            Self::Confluence => ("F7F6F2","292D29","BF492F"),
            Self::Lightfield => ("07101F","F3F7FF","7CCBFF"),
        }
    }
    fn metadata_source(self) -> &'static str {
        match self {
            Self::Pearl => include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/pearl.json")),
            Self::Field => include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/field.json")),
            Self::Chrome => include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/chrome.json")),
            Self::Confluence => include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/confluence.json")),
            Self::Lightfield => include_str!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/lightfield.json")),
        }
    }
    fn seed(self) -> &'static [u8] {
        match self {
            Self::Pearl => include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/pearl.pptx")),
            Self::Field => include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/field.pptx")),
            Self::Chrome => include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/chrome.pptx")),
            Self::Confluence => include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/confluence.pptx")),
            Self::Lightfield => include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/templates/pptx/lightfield.pptx")),
        }
    }
    pub(super) fn metadata(self) -> Result<&'static Metadata> {
        static PEARL:OnceLock<Result<Metadata>>=OnceLock::new();
        static FIELD:OnceLock<Result<Metadata>>=OnceLock::new();
        static CHROME:OnceLock<Result<Metadata>>=OnceLock::new();
        static CONFLUENCE:OnceLock<Result<Metadata>>=OnceLock::new();
        static LIGHTFIELD:OnceLock<Result<Metadata>>=OnceLock::new();
        let cache=match self {Self::Pearl=>&PEARL,Self::Field=>&FIELD,Self::Chrome=>&CHROME,Self::Confluence=>&CONFLUENCE,Self::Lightfield=>&LIGHTFIELD};
        cache.get_or_init(|| {
            let mut metadata:Metadata=serde_json::from_str(self.metadata_source()).map_err(|_|INVALID.to_string())?;
            metadata.roles.append(&mut metadata.content_variants);
            if metadata.schema!=1 || metadata.id!=self.id() || (metadata.width-W).abs()>0.01 || (metadata.height-H).abs()>0.01
                || metadata.width_emu<=0 || metadata.height_emu<=0 || metadata.roles.is_empty() || !color(&metadata.accent)
                || !["cover","section","content","image","table","closing"].iter().all(|role| metadata.roles.iter().any(|frame|frame.role==*role))
                || metadata.roles.iter().any(|frame|!frame.valid()) {return Err(INVALID.into());}
            Ok(metadata)
        }).as_ref().map_err(Clone::clone)
    }
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub(super) struct Metadata {
    schema:u32, id:String, width:f64, height:f64, width_emu:i64, height_emu:i64,
    pub(super) accent:String,
    pub(super) roles:Vec<Frame>,
    #[serde(default)] content_variants:Vec<Frame>,
}

impl Metadata {
    pub(super) fn select(&self,role:&str,page:usize)->usize {
        let candidates=self.roles.iter().enumerate().filter(|(_,frame)|frame.role==role).map(|(index,_)|index).collect::<Vec<_>>();
        candidates.get(page%candidates.len().max(1)).copied().unwrap_or_else(||self.roles.iter().position(|frame|frame.role=="content").unwrap_or(0))
    }
}

#[derive(Clone,Deserialize)]
#[serde(rename_all="camelCase")]
pub(super) struct Frame {
    pub(super) role:String,
    pub(super) slide:usize,
    pub(super) title:Slot,
    pub(super) body:Slot,
    pub(super) background:String,
    #[serde(default)] foreground_shape_ids:Vec<usize>,
    #[serde(default)] pub(super) image_rect:Option<Rect>,
}
impl Frame {
    fn valid(&self)->bool {self.slide>0 && color(&self.background) && self.title.valid() && self.body.valid() && self.image_rect.is_none_or(valid_rect)}
}
fn valid_rect([x,y,w,h]:Rect)->bool {[x,y,w,h].iter().all(|v|v.is_finite())&&x>=0.&&y>=0.&&w>=40.&&h>=20.&&x+w<=W+0.01&&y+h<=H+0.01}

#[derive(Clone,Deserialize)]
#[serde(rename_all="camelCase")]
pub(super) struct Slot {
    pub(super) rect:Rect,
    pub(super) font_size:f64,
    color:String,
    font_face:String,
    #[serde(default)] align:String,
}
impl Slot {
    fn valid(&self)->bool {
        let [x,y,w,h]=self.rect;
        self.rect.iter().all(|v|v.is_finite()) && x>=0. && y>=0. && w>=40. && h>=20.
            && x+w<=W+0.01 && y+h<=H+0.01 && (14. ..=100.).contains(&self.font_size)
            && color(&self.color) && !self.font_face.is_empty() && self.font_face.len()<=200
            && !self.font_face.chars().any(char::is_control)
    }
    pub(super) fn center(&self)->bool {self.align=="center"}
    pub(super) fn style(&self)->TextStyle {TextStyle{color:self.color.clone(),font:self.font_face.clone()}}
}
fn color(value:&str)->bool {value.len()==6&&value.bytes().all(|b|b.is_ascii_hexdigit())}

pub(super) fn fit_text(values:&Line,slot:&Slot,minimum:f64)->Result<(Vec<Line>,f64)> {
    let mut size=slot.font_size;
    loop {
        let lines=wrap(values,size,slot.rect[2])?;
        if lines.len() as f64*size*1.25+2. <= slot.rect[3] {return Ok((lines,size));}
        if size<=minimum {return Err(BUDGET.into());}
        size=(size-1.).max(minimum);
    }
}

// The same sparse palette powers the export controls and direct API requests.
// Presence wins over defaults: choosing even the stock blue is an explicit
// user choice and must never be inferred from a color's hue or value.
pub(super) fn table_options(options:&Value,template:Template)->Value {
    let mut output=options.clone();
    let Some(defaults)=crate::export_profiles::preset(template.id()).and_then(|preset|preset.get("table").cloned()) else{return output;};
    let Some(object)=output.as_object_mut()else{return output;};
    let table=object.entry("table").or_insert_with(||json!({}));
    if let (Some(table),Some(defaults))=(table.as_object_mut(),defaults.as_object()) {
        for (field,value) in defaults {table.entry(field.clone()).or_insert_with(||value.clone());}
    }
    output
}

pub(super) fn role_for(blocks:&[Block],last:bool)->&'static str {
    if blocks.is_empty() {return if last {"closing"}else{"section"};}
    fn has_table(block:&Block)->bool {match block {Block::Table{..}=>true,Block::Quote(blocks)=>blocks.iter().any(has_table),_=>false}}
    fn image_inline(value:&Inline)->bool {match value {Inline::Image{..}=>true,Inline::Strong(v)|Inline::Emph(v)|Inline::Strike(v)=>v.iter().any(image_inline),Inline::Link{children,..}|Inline::Citation{children,..}=>children.iter().any(image_inline),_=>false}}
    if blocks.iter().any(has_table) {return "table";}
    if blocks.iter().any(|block|match block {Block::Paragraph(v)|Block::Caption{inlines:v,..}=>v.iter().any(image_inline),_=>false}) {return "image";}
    "content"
}

// The browser's 800-character heuristic predates physical PowerPoint slots.
// A None notes owner explicitly marks an automatic chunk; explicit authored
// slide/subslide boundaries always carry Some notes (including empty notes).
// Rejoin only those automatic chunks and let the native geometry paginate.
pub(super) fn coalesce_automatic_chunks(matrix:Vec<Vec<(String,Option<String>)>>)->Vec<Vec<(String,Option<String>)>> {
    matrix.into_iter().map(|group| {
        let mut output:Vec<(String,Option<String>)>=Vec::new();
        for (content,notes) in group {
            if notes.is_none() {
                if let Some((previous,_))=output.last_mut() {previous.push_str("\n\n");previous.push_str(&content);continue;}
            }
            output.push((content,notes));
        }
        output
    }).collect()
}

pub(super) fn preview_css(deck:&Deck)->Result<String> {
    let (Layout::Native(template),Some(metadata))=(deck.layout,deck.native)else{return Ok(String::new());};
    let mut css=String::new();
    for frame in deck.slides.iter().map(|slide|slide.frame).collect::<BTreeSet<_>>() {
        check()?;
        let spec=&metadata.roles[frame];
        let bytes=previews::preview(template.id(),spec.slide).ok_or(INVALID)?;
        if css.len().saturating_add(bytes.len().saturating_mul(4)/3+512)>LIMIT{return Err(BUDGET.into());}
        css.push_str(&format!(".frame-{frame}{{background-color:#{};background-image:url(data:image/png;base64,{});background-size:100% 100%;color:#{};}}",spec.background,base64::engine::general_purpose::STANDARD.encode(bytes),spec.body.color));
    }
    Ok(css)
}

fn parse(bytes:&[u8])->Result<XmlNode> {
    // Some genuine vector artwork has tens of thousands of DrawingML nodes.
    // The formula editor's 1MB parser budget is intentionally too small for it.
    // These inputs are compiled into ReadMD, never paths or user-supplied XML.
    if bytes.len()>16*1024*1024 {return Err(BUDGET.into());}
    let source=std::str::from_utf8(bytes).map_err(|_|INVALID)?;
    let node=|name:String|XmlNode{id:String::new(),name,namespace:String::new(),attributes:BTreeMap::new(),attribute_namespaces:BTreeMap::new(),content:Vec::new()};
    let mut reader=Reader::from_str(source);
    reader.config_mut().check_end_names=true;
    let mut stack=vec![node("#root".into())];
    let mut nodes=0usize;
    loop {
        let event=reader.read_event().map_err(|_|INVALID)?;
        let empty=matches!(&event,Event::Empty(_));
        match event {
            Event::Start(start)|Event::Empty(start)=>{
                nodes+=1;if nodes%256==0 {check()?;}if nodes>250000||stack.len()>128 {return Err(BUDGET.into());}
                let mut child=node(start.name().as_ref().into());
                for attribute in start.attributes() {
                    let attribute=attribute.map_err(|_|INVALID)?;
                    child.attributes.insert(attribute.key.as_ref().into(),attribute.normalized_value(XmlVersion::Implicit1_0).map_err(|_|INVALID)?.into_owned());
                }
                if empty {stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Element(child));}else{stack.push(child);}
            },
            Event::End(_)=>{if stack.len()<=1{return Err(INVALID.into());}let child=stack.pop().ok_or(INVALID)?;stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Element(child));},
            Event::Text(value)=>stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Text(value.xml_content(XmlVersion::Implicit1_0).into_owned())),
            Event::CData(value)=>stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Text(value.xml_content(XmlVersion::Implicit1_0).into_owned())),
            Event::GeneralRef(value)=>{let raw=format!("&{};",value.as_ref());stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Text(quick_xml::escape::unescape(&raw).map_err(|_|INVALID)?.into_owned()));},
            Event::Comment(value)=>stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Comment(value.as_ref().into())),
            Event::PI(value)=>stack.last_mut().ok_or(INVALID)?.content.push(XmlPart::Instruction(value.as_ref().into())),
            Event::DocType(_)=>return Err(INVALID.into()),Event::Decl(_)=>(),Event::Eof=>break,
        }
    }
    if stack.len()!=1 {return Err(INVALID.into());}stack.pop().ok_or(INVALID.into())
}
fn child_mut<'a>(node:&'a mut XmlNode,local:&str)->Option<&'a mut XmlNode> {
    node.content.iter_mut().find_map(|part|match part {XmlPart::Element(child) if child.local()==local=>Some(child),_=>None})
}
fn descendant_mut<'a>(node:&'a mut XmlNode,local:&str)->Option<&'a mut XmlNode> {
    if node.local()==local {return Some(node);}
    node.content.iter_mut().find_map(|part|if let XmlPart::Element(child)=part {descendant_mut(child,local)}else{None})
}
fn single(bytes:&[u8])->Result<XmlNode> {parse(bytes)?.content.into_iter().find_map(|part|if let XmlPart::Element(node)=part{Some(node)}else{None}).ok_or(INVALID.into())}
fn element(xml:&str)->Result<XmlNode> {single(xml.as_bytes())}
fn part<'a>(entries:&'a BTreeMap<String,Vec<u8>>,name:&str)->Result<&'a [u8]> {entries.get(name).map(Vec::as_slice).ok_or(INVALID.into())}
fn output_xml(node:&XmlNode)->Vec<u8> {format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>{}",node.xml()).into_bytes()}
fn attr<'a>(node:&'a XmlNode,key:&str)->&'a str {node.attributes.get(key).map(String::as_str).unwrap_or("")}
fn is_relationship(node:&XmlNode,kind:&str)->bool {attr(node,"Type").ends_with(&format!("/{kind}"))}
fn retain_elements(node:&mut XmlNode,mut keep:impl FnMut(&XmlNode)->bool) {node.content.retain(|part|if let XmlPart::Element(child)=part {keep(child)}else{true});}
fn add_relationship(node:&mut XmlNode,id:&str,kind:&str,target:&str)->Result<()> {
    node.content.push(XmlPart::Element(element(&relation(id,kind,target,false))?));Ok(())
}

fn prefix_relationships(node:&mut XmlNode) {
    for (key,value) in &mut node.attributes {if matches!(key.as_str(),"r:id"|"r:embed"|"r:link") {*value=format!("readmd_{value}");}}
    for part in &mut node.content {if let XmlPart::Element(child)=part {prefix_relationships(child);}}
}

// The template's retained animation IDs and targets remain untouched. Generated
// title effects use a separate ID range and join its existing root timeline.
fn add_title_fade(slide:&mut XmlNode,target:usize)->Result<()> {
    fn renumber(node:&mut XmlNode) {
        if node.local()=="cTn" {if let Some(id)=node.attributes.get_mut("id") {if let Ok(value)=id.parse::<usize>() {*id=(value+100000).to_string();}}}
        if node.local()=="tn" {if let Some(id)=node.attributes.get_mut("val") {if let Ok(value)=id.parse::<usize>() {*id=(value+100000).to_string();}}}
        for part in &mut node.content {if let XmlPart::Element(child)=part {renumber(child);}}
    }
    let mut timing=element(&design::title_fade(target))?;
    renumber(&mut timing);
    if let Some(existing)=child_mut(slide,"timing") {
        let mut incoming=descendant_mut(&mut timing,"childTnLst").ok_or(INVALID)?.content.clone();
        // A slide owns one main sequence. This independent automatic title
        // sequence participates in the existing root without declaring another.
        fn secondary(node:&mut XmlNode) {
            if node.attributes.get("nodeType").is_some_and(|value|value=="mainSeq") {node.attributes.remove("nodeType");}
            for part in &mut node.content {if let XmlPart::Element(child)=part {secondary(child);}}
        }
        for part in &mut incoming {if let XmlPart::Element(child)=part {secondary(child);}}
        if let Some(children)=descendant_mut(existing,"childTnLst") {children.content.extend(incoming);}
        else {return Err(INVALID.into());}
    } else {
        let at=slide.content.iter().position(|part|matches!(part,XmlPart::Element(node) if node.local()=="extLst")).unwrap_or(slide.content.len());
        slide.content.insert(at,XmlPart::Element(timing));
    }
    Ok(())
}

fn runtime_shapes(deck:&Deck,slide:&Slide,metadata:&Metadata,links:&BTreeMap<String,String>)->Result<XmlNode> {
    let shapes=slide.shapes.iter().enumerate().map(|(index,shape)|shape_xml(shape,index+10000,deck,links)).collect::<String>();
    let mut group=element(&format!("<p:grpSp xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\"><p:nvGrpSpPr><p:cNvPr id=\"9999\" name=\"ReadMD editable content\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm></p:grpSpPr>{shapes}</p:grpSp>",metadata.width_emu,metadata.height_emu,emu(W),emu(H)))?;
    prefix_relationships(&mut group);
    Ok(group)
}

fn shape_id(node:&XmlNode)->Option<usize> {
    if node.local()=="cNvPr" {return attr(node,"id").parse().ok();}
    node.children().find_map(shape_id)
}

fn relationship_path(owner:&str)->String {
    if owner.is_empty() {return "_rels/.rels".into();}
    let (directory,name)=owner.rsplit_once('/').unwrap_or(("",owner));
    if directory.is_empty() {format!("_rels/{name}.rels")}else{format!("{directory}/_rels/{name}.rels")}
}
fn relationship_target(owner:&str,target:&str)->Result<String> {
    let target=percent_encoding::percent_decode_str(target.split('#').next().unwrap_or(target)).decode_utf8().map_err(|_|INVALID)?;
    let path=if target.starts_with('/') {target.trim_start_matches('/').to_string()}else{
        let parent=owner.rsplit_once('/').map(|(parent,_)|parent).unwrap_or("");
        if parent.is_empty() {target.to_string()}else{format!("{parent}/{target}")}
    };
    let mut normalized=Vec::new();
    for piece in path.split('/') {match piece {""|"."=>(),".."=>{if normalized.pop().is_none(){return Err(INVALID.into());}},value=>normalized.push(value)}}
    Ok(normalized.join("/"))
}

// Duplicating a subset of a template's roles must not retain media belonging
// only to unused source pages. Walk actual OPC relationships, including master
// and layout cycles, so no authored image or embedded native dependency is lost.
fn prune_unreferenced(output:&mut BTreeMap<String,Vec<u8>>)->Result<()> {
    let mut reachable=BTreeSet::from(["[Content_Types].xml".to_string()]);
    let mut pending=vec![String::new()];
    let mut visited=BTreeSet::new();
    while let Some(owner)=pending.pop() {
        check()?;
        if !visited.insert(owner.clone()) {continue;}
        if !owner.is_empty() {if !output.contains_key(&owner){return Err(INVALID.into());}reachable.insert(owner.clone());}
        let relations=relationship_path(&owner);
        if let Some(bytes)=output.get(&relations) {
            reachable.insert(relations);
            let root=single(bytes)?;
            for relation in root.children().filter(|node|attr(node,"TargetMode")!="External") {
                let target=relationship_target(&owner,attr(relation,"Target"))?;
                if target.is_empty(){return Err(INVALID.into());}
                pending.push(target);
            }
        }
    }
    output.retain(|name,_|reachable.contains(name));
    let mut types=single(part(output,"[Content_Types].xml")?)?;
    retain_elements(&mut types,|node|node.local()!="Override"||output.contains_key(attr(node,"PartName").trim_start_matches('/')));
    output.insert("[Content_Types].xml".into(),output_xml(&types));
    Ok(())
}

pub(super) fn package(deck:&Deck,title:&str,template:Template)->Result<Vec<u8>> {
    let metadata=template.metadata()?;
    let seed=crate::convert::parse_zip_entries(template.seed())?.into_iter().collect::<BTreeMap<_,_>>();
    // Reuse the lossless notes/image writer; only its authored notes and media
    // are copied. Native artwork and all design dependencies come from the seed.
    let generated=crate::convert::parse_zip_entries(&package_legacy(deck,title)?)?.into_iter().collect::<BTreeMap<_,_>>();
    let mut output=seed.iter().filter(|(path,_)|!path.starts_with("ppt/slides/")&&!path.starts_with("ppt/notesSlides/")&&!path.starts_with("ppt/notesMasters/")&&!matches!(path.as_str(),"ppt/presentation.xml"|"ppt/_rels/presentation.xml.rels"|"[Content_Types].xml"|"_rels/.rels"|"docProps/core.xml"|"docProps/app.xml")).map(|(path,bytes)|(path.clone(),bytes.clone())).collect::<BTreeMap<_,_>>();
    let mut types=single(part(&seed,"[Content_Types].xml")?)?;
    retain_elements(&mut types,|node| {
        let name=attr(node,"PartName");
        !name.starts_with("/ppt/slides/")&&!name.starts_with("/ppt/notesSlides/")&&!name.starts_with("/ppt/notesMasters/")
    });
    let generated_types=single(part(&generated,"[Content_Types].xml")?)?;
    for node in generated_types.children() {
        let name=attr(node,"PartName");
        let copy=node.local()=="Default"||name.starts_with("/ppt/notesSlides/")||name.starts_with("/ppt/slides/")||name=="/docProps/core.xml"||name=="/docProps/app.xml";
        if copy && !types.children().any(|existing|existing.local()==node.local()&&if node.local()=="Default" {attr(existing,"Extension")==attr(node,"Extension")}else{attr(existing,"PartName")==name}) {types.content.push(XmlPart::Element(node.clone()));}
    }
    for (name,kind) in [("notesMasters/readmdNotesMaster.xml","application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"),("theme/readmdNotesTheme.xml","application/vnd.openxmlformats-officedocument.theme+xml")] {
        types.content.push(XmlPart::Element(element(&format!("<Override PartName=\"/ppt/{name}\" ContentType=\"{kind}\"/>"))?));
    }

    let mut presentation=single(part(&seed,"ppt/presentation.xml")?)?;
    retain_elements(&mut presentation,|node|!matches!(node.local(),"sldIdLst"|"notesMasterIdLst"|"custShowLst"|"extLst"));
    let mut relations=single(part(&seed,"ppt/_rels/presentation.xml.rels")?)?;
    retain_elements(&mut relations,|node|!is_relationship(node,"slide")&&!is_relationship(node,"notesMaster"));
    add_relationship(&mut relations,"readmd_notes","notesMaster","notesMasters/readmdNotesMaster.xml")?;
    let mut slide_ids=String::new();

    for (index,slide) in deck.slides.iter().enumerate() {
        check()?;
        let number=index+1;
        let frame=&metadata.roles[slide.frame];
        let mut native=single(part(&seed,&format!("ppt/slides/slide{}.xml",frame.slide))?)?;
        let mut rels=single(part(&seed,&format!("ppt/slides/_rels/slide{}.xml.rels",frame.slide))?)?;
        retain_elements(&mut rels,|node|!is_relationship(node,"notesSlide")&&!is_relationship(node,"slide"));
        let mut links=BTreeMap::new();
        let mut visit=|line:&Line| {for run in line {if let Some(href)=&run.href {let id=format!("link{}",links.len());links.entry(href.clone()).or_insert(id);}}};
        for shape in &slide.shapes {match shape {Shape::Text{lines,..}=>for line in lines {visit(line);},Shape::Table{rows,..}=>for line in table::lines(rows){visit(line);},_=>()}}
        let group=runtime_shapes(deck,slide,metadata,&links)?;
        // All seed shape IDs are below this reserved range, including grouped
        // descendants. A conflict fails before writing the destination.
        fn ids(node:&XmlNode,out:&mut BTreeSet<usize>)->bool {
            if node.local()=="cNvPr" {let Ok(id)=attr(node,"id").parse::<usize>()else{return false;};if !out.insert(id) {return false;}}
            node.children().all(|child|ids(child,out))
        }
        let mut existing=BTreeSet::new();
        if !ids(&native,&mut existing)||existing.iter().any(|id|*id>=9999) {return Err(INVALID.into());}
        // Body text/fonts already came from the selected role's slots.
        let tree=descendant_mut(&mut native,"spTree").ok_or(INVALID)?;
        let at=tree.content.iter().position(|part|matches!(part,XmlPart::Element(node) if shape_id(node).is_some_and(|id|frame.foreground_shape_ids.contains(&id)))).unwrap_or(tree.content.len());
        // Namespace declarations are retained on the group, so SVG extensions
        // and source-specific namespace prefixes survive this composition.
        tree.content.insert(at,XmlPart::Element(group));
        if deck.title_fade {
            if let Some(target)=slide.shapes.iter().position(|shape|matches!(shape,Shape::Text{code:false,..})) {add_title_fade(&mut native,target+10000)?;}
        }
        // Native artwork animation and transitions are part of the chosen
        // template. An explicit exporter transition can replace the transition.
        if deck.transition_explicit {
            retain_elements(&mut native,|node|node.local()!="transition");
            let kind=match deck.transition.as_str(){"fade"=>"<p:fade/>","slide"=>"<p:push dir=\"l\"/>","zoom"=>"<p:zoom dir=\"in\"/>",_=>""};
            if !kind.is_empty() {
                let at=native.content.iter().position(|part|matches!(part,XmlPart::Element(node) if matches!(node.local(),"timing"|"extLst"))).unwrap_or(native.content.len());
                native.content.insert(at,XmlPart::Element(element(&format!("<p:transition spd=\"med\">{kind}</p:transition>"))?));
            }
        }
        let gen_rels=single(part(&generated,&format!("ppt/slides/_rels/slide{number}.xml.rels"))?)?;
        for node in gen_rels.children().filter(|node|!is_relationship(node,"slideLayout")) {
            let mut node=node.clone();
            if let Some(id)=node.attributes.get_mut("Id") {*id=format!("readmd_{id}");}
            if is_relationship(&node,"image") {if let Some(target)=node.attributes.get_mut("Target") {*target=target.replace("../media/","../media/readmd-");}}
            rels.content.push(XmlPart::Element(node));
        }
        output.insert(format!("ppt/slides/slide{number}.xml"),output_xml(&native));
        output.insert(format!("ppt/slides/_rels/slide{number}.xml.rels"),output_xml(&rels));
        slide_ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"readmd_slide{number}\"/>",256+index));
        add_relationship(&mut relations,&format!("readmd_slide{number}"),"slide",&format!("slides/slide{number}.xml"))?;
    }
    // ECMA child order: masters, notes masters, handout masters, slide list.
    let at=presentation.content.iter().position(|part|matches!(part,XmlPart::Element(node) if !matches!(node.local(),"sldMasterIdLst"))).unwrap_or(presentation.content.len());
    presentation.content.insert(at,XmlPart::Element(element("<p:notesMasterIdLst><p:notesMasterId r:id=\"readmd_notes\"/></p:notesMasterIdLst>")?));
    let at=presentation.content.iter().position(|part|matches!(part,XmlPart::Element(node) if !matches!(node.local(),"sldMasterIdLst"|"notesMasterIdLst"|"handoutMasterIdLst"))).unwrap_or(presentation.content.len());
    presentation.content.insert(at,XmlPart::Element(element(&format!("<p:sldIdLst>{slide_ids}</p:sldIdLst>"))?));
    output.insert("ppt/presentation.xml".into(),output_xml(&presentation));
    output.insert("ppt/_rels/presentation.xml.rels".into(),output_xml(&relations));
    output.insert("[Content_Types].xml".into(),output_xml(&types));
    for (path,bytes) in &generated {
        if path.starts_with("ppt/notesSlides/") {
            let bytes=if path.ends_with(".rels") {String::from_utf8(bytes.clone()).map_err(|_|INVALID)?.replace("../notesMasters/notesMaster1.xml","../notesMasters/readmdNotesMaster.xml").into_bytes()}else{bytes.clone()};
            output.insert(path.clone(),bytes);
        } else if let Some(name)=path.strip_prefix("ppt/media/") {
            output.insert(format!("ppt/media/readmd-{name}"),bytes.clone());
        } else if matches!(path.as_str(),"_rels/.rels"|"docProps/core.xml"|"docProps/app.xml") {
            output.insert(path.clone(),bytes.clone());
        }
    }
    output.insert("ppt/notesMasters/readmdNotesMaster.xml".into(),part(&generated,"ppt/notesMasters/notesMaster1.xml")?.to_vec());
    output.insert("ppt/notesMasters/_rels/readmdNotesMaster.xml.rels".into(),String::from_utf8(part(&generated,"ppt/notesMasters/_rels/notesMaster1.xml.rels")?.to_vec()).map_err(|_|INVALID)?.replace("../theme/theme1.xml","../theme/readmdNotesTheme.xml").into_bytes());
    output.insert("ppt/theme/readmdNotesTheme.xml".into(),part(&generated,"ppt/theme/theme1.xml")?.to_vec());
    prune_unreferenced(&mut output)?;
    let mut total=0usize;
    let entries=output.into_iter().map(|(path,data)| {
        total=total.checked_add(data.len()).ok_or(BUDGET)?;
        if total>LIMIT{return Err(BUDGET.into());}check()?;
        Ok(ZipEntry{path,data,compress:true})
    }).collect::<Result<Vec<_>>>()?;
    let bytes=crate::mdexport::write_zip(&entries)?;
    if bytes.len()>LIMIT {return Err(BUDGET.into());}check()?;Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATES:[Template;5]=[Template::Pearl,Template::Field,Template::Chrome,Template::Confluence,Template::Lightfield];
    fn authored(deck:&Deck)->String {
        deck.slides.iter().flat_map(|slide|&slide.shapes).map(|shape|match shape {
            Shape::Text{lines,..}=>lines.iter().map(|line|plain(line)).collect::<String>(),
            Shape::Table{rows,..}=>table::lines(rows).map(|line|plain(line)).collect::<String>(),
            Shape::Image{alt,..}=>alt.clone(),Shape::Accent{..}=>String::new(),
        }).collect()
    }
    fn walk<'a>(node:&'a XmlNode,local:&str,out:&mut Vec<&'a XmlNode>) {
        if node.local()==local {out.push(node);}
        for child in node.children() {walk(child,local,out);}
    }
    fn nodes<'a>(node:&'a XmlNode,local:&str)->Vec<&'a XmlNode> {let mut out=Vec::new();walk(node,local,&mut out);out}

    #[test]
    fn native_table_defaults_share_catalog_and_explicit_stock_blue_remains_a_user_choice() {
        let source="# Palette\n\n| Header | Value |\n|---|---|\n| Body | 42 |\n";
        for template in TEMPLATES {
            let preset=crate::export_profiles::preset(template.id()).unwrap();
            let deck=build(source,"","Palette",&json!({"pptxLayout":template.id()})).unwrap();
            let xml=deck.slides.iter().flat_map(|slide|&slide.shapes).find_map(|shape|if matches!(shape,Shape::Table{..}){Some(shape_xml(shape,10001,&deck,&BTreeMap::new()))}else{None}).unwrap();
            assert!(xml.contains(&format!("val=\"{}\"",preset["table"]["headerBg"].as_str().unwrap().trim_start_matches('#').to_ascii_uppercase())));
            let explicit=json!({"pptxLayout":template.id(),"table":{"headerBg":"#3b6ef5","headerColor":"#ffffff","borderColor":"#123456"}});
            let merged=table_options(&explicit,template);
            assert_eq!(merged["table"]["headerBg"],"#3b6ef5");
            assert_eq!(merged["table"]["headerColor"],"#ffffff");
            assert_eq!(merged["table"]["borderColor"],"#123456");
            assert_eq!(merged["table"]["bandColor"],preset["table"]["bandColor"]);
            assert_eq!(explicit["table"].as_object().unwrap().len(),3,"original options mutated");
            let customized=build(source,"","Palette",&explicit).unwrap();
            let xml=customized.slides.iter().flat_map(|slide|&slide.shapes).find_map(|shape|if matches!(shape,Shape::Table{..}){Some(shape_xml(shape,10001,&customized,&BTreeMap::new()))}else{None}).unwrap();
            assert!(xml.contains("val=\"3B6EF5\""));
            assert!(xml.contains("val=\"123456\""));
        }
    }

    #[test]
    fn all_native_templates_fill_real_artwork_roles_with_editable_markdown() {
        let source=concat!("# 原生模板测试\n\nIntro subtitle.\n\n<!-- slide -->\n",
            "## Section only\n\n<!-- slide -->\n## Evidence\n\n",
            "Editable **body** and [source](https://example.test/evidence).\n\n",
            "| Key | Value |\n|---|---|\n| Preserved cell | 42 |\n\n",
            "<!-- note -->\nSpeaker note retained.\n\n<!-- slide -->\n## Closing\n");
        for template in TEMPLATES {
            let options=json!({"pptxLayout":template.id(),"pptxTitleFade":true});
            let deck=build(source,"","Fallback filename",&options).unwrap();
            let metadata=template.metadata().unwrap();
            assert_eq!(deck.slides.len(),4,"{}",template.id());
            assert_eq!(metadata.roles[deck.slides[0].frame].role,"cover");
            assert_eq!(metadata.roles[deck.slides[1].frame].role,"section");
            assert_eq!(metadata.roles[deck.slides[2].frame].role,"table");
            assert_eq!(metadata.roles[deck.slides[3].frame].role,"closing");
            let expected=authored(&deck);
            for text in ["原生模板测试","Section only","Editable ","Preserved cell","42","Closing"] {assert!(expected.contains(text),"{}: {text}",template.id());}
            let html=preview_html(&deck).unwrap();
            assert!(html.contains("background-image:url(data:image/png;base64,"));
            assert!(html.contains("Preserved cell"));
            assert!(!html.contains("Speaker note retained"));
            let entries=crate::convert::parse_zip_entries(&super::package(&deck,"Native editable test",template).unwrap()).unwrap().into_iter().collect::<BTreeMap<_,_>>();
            let presentation=single(part(&entries,"ppt/presentation.xml").unwrap()).unwrap();
            assert_eq!(nodes(&presentation,"sldId").len(),4);
            let native=single(part(&entries,"ppt/slides/slide3.xml").unwrap()).unwrap();
            assert_eq!(nodes(&native,"tbl").len(),1);
            assert!(native.text().contains("Preserved cell"));
            assert!(!nodes(&native,"spTgt").is_empty());
            assert!(nodes(&native,"cNvPr").iter().any(|node|attr(node,"id").parse::<usize>().is_ok_and(|id|id<9999)));
            assert!(nodes(&native,"cNvPr").iter().any(|node|attr(node,"id")=="10000"));
            let notes=single(part(&entries,"ppt/notesSlides/notesSlide3.xml").unwrap()).unwrap();
            assert!(notes.text().contains("Speaker note retained."));
            let seed=crate::convert::parse_zip_entries(template.seed()).unwrap();
            for (name,data) in seed.iter().filter(|(name,_)|name.starts_with("ppt/media/")||name.starts_with("ppt/slideMasters/")||name.starts_with("ppt/slideLayouts/")||name.starts_with("ppt/theme/")) {
                if let Some(retained)=entries.get(name) {assert_eq!(retained,data,"{}: native dependency changed {name}",template.id());}
            }
            let mut pruned=entries.clone();prune_unreferenced(&mut pruned).unwrap();assert_eq!(pruned,entries,"package contained unreachable source dependencies");
            for (name,data) in entries.iter().filter(|(name,_)|name.ends_with(".xml")||name.ends_with(".rels")) {parse(data).unwrap_or_else(|error|panic!("{} {name}: {error}",template.id()));}
        }
    }

    #[test]
    fn native_pagination_keeps_every_table_cell_and_repeated_title_in_its_slots() {
        let mut source="# Long native table\n\n| Key | Detail |\n|---|---|\n".to_string();
        for index in 0..28 {source.push_str(&format!("| unique-{index:03} | detail-{index:03} |\n"));}
        for template in TEMPLATES {
            let deck=build(&source,"","Deck",&json!({"pptxLayout":template.id()})).unwrap();
            assert!(deck.slides.len()>2);
            for index in 0..28 {assert_eq!(authored(&deck).matches(&format!("unique-{index:03}")).count(),1);}
            for slide in &deck.slides {
                let frame=&template.metadata().unwrap().roles[slide.frame];
                assert_eq!(frame.role,"table");
                let mut titles=0;
                for shape in &slide.shapes {
                    let rect=match shape {Shape::Text{rect,lines,..}=>{if lines.iter().any(|line|plain(line).contains("Long native table")){titles+=1;}rect},Shape::Table{rect,rows,..}=>{assert_eq!(plain(&rows[0].cells[0].lines[0]),"Key");rect},Shape::Image{rect,..}|Shape::Accent{rect,..}=>rect};
                    assert!(rect[0]>=0.&&rect[1]>=0.&&rect[0]+rect[2]<=W+0.01&&rect[1]+rect[3]<=H+0.01);
                }
                assert_eq!(titles,1);
            }
        }
    }

    #[test]
    fn native_image_role_preserves_original_bytes_aspect_and_source_image_slot() {
        let mut data=std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(32,16,image::Rgba([45,95,150,255]))).write_to(&mut data,image::ImageFormat::Png).unwrap();
        let bytes=data.into_inner();
        let encoded=base64::engine::general_purpose::STANDARD.encode(&bytes);
        let source=format!("# Image evidence\n\nCaption remains editable.\n\n![Original image](data:image/png;base64,{encoded})");
        for template in TEMPLATES {
            let deck=build(&source,"","Image",&json!({"pptxLayout":template.id()})).unwrap();
            assert_eq!(deck.media[0].bytes,bytes);
            assert_eq!(deck.slides.len(),1);
            let frame=&template.metadata().unwrap().roles[deck.slides[0].frame];
            assert_eq!(frame.role,"image");
            let rect=deck.slides[0].shapes.iter().find_map(|shape|if let Shape::Image{rect,..}=shape{Some(rect)}else{None}).unwrap();
            assert!((rect[2]/rect[3]-2.).abs()<0.0001);
            if let Some(slot)=frame.image_rect {assert!(rect[0]>=slot[0]&&rect[1]>=slot[1]&&rect[0]+rect[2]<=slot[0]+slot[2]+0.01&&rect[1]+rect[3]<=slot[1]+slot[3]+0.01);}
        }
    }
}
