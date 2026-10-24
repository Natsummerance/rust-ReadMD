//! Current native note properties, independent of export provenance. The
//! parser borrows validated note stories, caches style inheritance once, and
//! returns the same sparse policies used by source profiles and DOCX output.
use super::{DocxRel, XmlNode, docx_notes};
use crate::publication_note_policies::{self as policy, LineSpacing, LongNote, NoteKind, NotePlacement, NotePolicies, NotePolicyPatch, NoteRestart, NoteSeparator};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest,Sha256};
use std::{borrow::Cow, collections::{BTreeMap, HashMap, HashSet}, ops::Range};

pub(super) const INVALID:&str="docx_note_settings_invalid";
pub(super) const UNSUPPORTED:&str="docx_note_settings_unsupported";
const REL:&str="http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_REL:&str="http://purl.oclc.org/ooxml/officeDocument/relationships";
const MAX_WORK:usize=1_200_000;
const MAX_PARAGRAPHS:usize=65536;

#[derive(Debug,Clone,PartialEq,Eq,Serialize)]
pub(super) struct Error {
    pub code:&'static str,
    pub part:String,
    pub path:String,
    pub detail:String,
}
impl std::fmt::Display for Error {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        write!(f,"{}:{}",self.code,json!({"part":self.part,"path":self.path,"detail":self.detail}))
    }
}
impl std::error::Error for Error {}
type Result<T>=std::result::Result<T,Error>;
fn error(part:&str,path:&str,detail:&str)->Error{Error{code:INVALID,part:part.into(),path:path.into(),detail:detail.chars().take(500).collect()}}
fn unsupported(part:&str,path:&str,detail:&str)->Error{Error{code:UNSUPPORTED,..error(part,path,detail)}}

#[derive(Debug,Clone,Default,Serialize)]
pub(super) struct ParseStats {
    pub parsed_parts:usize,
    pub borrowed_note_parts:usize,
    pub resolved_styles:usize,
    pub paragraphs:usize,
    pub runs:usize,
    pub work:usize,
}
impl ParseStats { pub(super) fn step(&mut self,part:&str,path:&str)->Result<()>{self.work+=1;if self.work>MAX_WORK{return Err(error(part,path,"note settings budget"));}Ok(())} }

#[derive(Debug,Clone,Serialize)]
pub(super) struct SectionNotePatch {
    pub ordinal:usize,
    /// Main-body child indices, including the paragraph terminating this
    /// section. This range describes native XML, not guessed Markdown offsets.
    pub body_range:Range<usize>,
    pub terminator_path:Vec<usize>,
    pub footnote:NotePolicyPatch,
    pub endnote:NotePolicyPatch,
    pub suppress_endnotes:Option<bool>,
}
impl SectionNotePatch {
    pub fn to_value(&self)->Value {
        let mut kinds=Map::new();
        if !self.footnote.is_empty(){kinds.insert("footnote".into(),self.footnote.to_value());}
        if !self.endnote.is_empty(){kinds.insert("endnote".into(),self.endnote.to_value());}
        let mut out=Map::new();if !kinds.is_empty(){out.insert("kinds".into(),Value::Object(kinds));}
        if let Some(suppress)=self.suppress_endnotes{out.insert("suppressEndnotes".into(),json!(suppress));}Value::Object(out)
    }
}
#[derive(Debug,Clone,Serialize)]
pub(super) struct NoteLocalPatch {
    pub kind:NoteKind,
    pub native_id:i32,
    pub label:String,
    /// Child indices from the note element to its actual paragraph.
    pub paragraph_path:Vec<usize>,
    /// Existing readable local-layout shape; never a copied XML payload.
    pub style:Value,
}
#[derive(Debug,Clone,Default,Serialize)]
pub(super) struct ImportedNoteSettings {
    pub policies:NotePolicies,
    pub sections:Vec<SectionNotePatch>,
    pub local:Vec<NoteLocalPatch>,
    pub diagnostics:Vec<Value>,
    pub stats:ParseStats,
}
#[derive(Debug,Clone)]
pub(super) struct NoteSectionDeclaration{
    /// Identity of the actual child visited by docx_word_children(body).
    /// None inserts at the end of the rendered main story, before note defs.
    pub before_node:Option<usize>,
    pub section:crate::publication_sections::Section,
    pub comment:String,
}
#[derive(Debug,Clone)]
pub(super) struct NoteSectionSourcePlan{
    pub declarations:Vec<NoteSectionDeclaration>,
    pub anchors:HashMap<String,String>,
    /// Use only after every declaration has been inserted at its node boundary.
    pub note_options:Value,
}
impl ImportedNoteSettings {
    pub fn to_options(&self)->Value {
        let value=self.policies.to_value();if value.as_object().is_some_and(|v|v.is_empty()){json!({})}else{json!({"footnotes":value})}
    }
    /// A caller which has not installed the section/local integration must
    /// refuse the conversion, rather than silently dropping native patches.
    pub fn require_global_only(&self)->Result<Value>{
        if !self.sections.is_empty(){return Err(unsupported("word/document.xml","sectPr","native section note settings require source section integration"));}
        if let Some(local)=self.local.first(){return Err(unsupported("note story",&format!("{}:{:?}",local.label,local.paragraph_path),"native note paragraph settings require source local-layout integration"));}
        Ok(self.to_options())
    }
    pub fn require_no_sections(&self)->Result<Value>{
        if !self.sections.is_empty(){return Err(unsupported("word/document.xml","sectPr","native section note settings require source section integration"));}Ok(self.to_options())
    }
    /// Restore real section scopes with their sparse native numbering and
    /// supported positions. Unsupported fields are never discarded by the plan.
    pub fn plan_source_sections(&self,body:&XmlNode)->Result<NoteSectionSourcePlan>{
        if self.sections.is_empty(){return Ok(NoteSectionSourcePlan{declarations:Vec::new(),anchors:HashMap::new(),note_options:self.to_options()});}
        if self.sections.len()>crate::publication_sections::MAX_SECTIONS{return Err(unsupported("word/document.xml","sectPr","source section count outside bounds"));}
        for patch in &self.sections{
            for(kind,original)in[("footnotePr",&patch.footnote),("endnotePr",&patch.endnote)]{
                let mut remaining=original.clone();
                remaining.number_format=None;remaining.number_pattern=None;remaining.start=None;remaining.restart=None;
                if kind=="footnotePr"{remaining.placement=None;}
                if !remaining.is_empty(){
                    let value=remaining.to_value();let key=value.as_object().and_then(|value|value.keys().next()).map(String::as_str).unwrap_or("unknown");
                    return Err(unsupported("word/document.xml",&format!("/w:body/{:?}/w:sectPr/w:{kind}/w:{key}",patch.terminator_path),"native section note setting has no equivalent source section field"));
                }
            }
            if patch.suppress_endnotes.is_some(){return Err(unsupported("word/document.xml",&format!("/w:body/{:?}/w:sectPr/w:suppressEndnotes",patch.terminator_path),"different native section note settings require per-section policy integration for endnote suppression"));}
        }
        let children=super::docx_word_children(body).into_iter().filter(|node|matches!(node.name.as_str(),"w:p"|"w:tbl")||super::docx_citations::is_control(node)).collect::<Vec<_>>();
        let positions=children.iter().enumerate().map(|(index,node)|(*node as *const XmlNode as usize,index)).collect::<HashMap<_,_>>();
        let mut declarations=Vec::new();let mut anchors=HashMap::new();let mut begin=0;let mut used_ids=HashSet::new();
        for(index,patch)in self.sections.iter().enumerate(){
            let properties=if patch.terminator_path.is_empty(){None}else{Some(section_path(body,&patch.terminator_path).ok_or_else(||error("word/document.xml","sectPr","native section node moved"))?)};
            let end=if let Some(properties)=properties{
                if properties.name!="w:sectPr"{return Err(error("word/document.xml","sectPr","native section node changed"));}
                if patch.terminator_path.len()==1{
                    if index+1!=self.sections.len(){return Err(error("word/document.xml","sectPr","body section properties must terminate the main story"));}children.len()
                }else{
                    let paragraph_path=patch.terminator_path.get(..patch.terminator_path.len().saturating_sub(2)).ok_or_else(||error("word/document.xml","sectPr","invalid native section path"))?;
                    let paragraph=section_path(body,paragraph_path).filter(|node|node.name=="w:p").ok_or_else(||unsupported("word/document.xml","sectPr","section terminator has no source paragraph boundary"))?;
                    positions.get(&(paragraph as *const XmlNode as usize)).copied().ok_or_else(||unsupported("word/document.xml","sectPr","section terminator is inside an indivisible source container"))?.saturating_add(1)
                }
            }else{children.len()};
            if end<begin{return Err(error("word/document.xml","sectPr","out-of-order native section terminators"));}
            let break_kind=section_break(properties)?;
            let id=source_section_id(children.get(begin).copied(),index,&mut used_ids)?;
            for name in source_section_names(children.get(begin).copied()){
                let target=format!("readmd-section-{id}");
                if anchors.insert(name.into(),target.clone()).is_some_and(|old|old!=target){return Err(unsupported("word/document.xml","sectPr/bookmark","ambiguous native section hyperlink target"));}
            }
            let numbering=|patch:&NotePolicyPatch|policy::SectionNoteNumberingPatch{
                number_format:patch.number_format.clone(),number_pattern:patch.number_pattern.clone(),start:patch.start,restart:patch.restart,
            };
            let notes=policy::SectionNotePolicies{footnote_placement:patch.footnote.placement,
                footnote_numbering:numbering(&patch.footnote),endnote_numbering:numbering(&patch.endnote)};
            let footnotes=(!notes.is_empty()).then_some(notes);
            let section=crate::publication_sections::Section{id,break_kind,page:Default::default(),numbering:Default::default(),header:Default::default(),footer:Default::default(),footnotes};
            let comment=crate::publication_sections::comment(&section).map_err(|_|unsupported("word/document.xml","sectPr","native section has no valid source declaration"))?;
            declarations.push(NoteSectionDeclaration{before_node:children.get(begin).map(|node|*node as *const XmlNode as usize),section,comment});begin=end;
        }
        if begin!=children.len(){return Err(error("word/document.xml","sectPr","unconsumed native main-story section"));}
        Ok(NoteSectionSourcePlan{declarations,anchors,note_options:self.to_options()})
    }
}

fn section_path<'a>(mut node:&'a XmlNode,path:&[usize])->Option<&'a XmlNode>{for index in path{node=node.children.get(*index)?;}Some(node)}
fn section_break(properties:Option<&XmlNode>)->Result<crate::publication_sections::SectionBreak>{
    use crate::publication_sections::SectionBreak;
    let Some(properties)=properties else{return Ok(SectionBreak::NextPage)};
    let Some(kind)=unique_child(properties,"w:type","word/document.xml","sectPr/type")?else{return Ok(SectionBreak::NextPage)};
    leaf(kind,&["w:val"],"word/document.xml","sectPr/type")?;
    match val(kind,"word/document.xml","sectPr/type")?{"continuous"=>Ok(SectionBreak::Continuous),"nextPage"=>Ok(SectionBreak::NextPage),"oddPage"=>Ok(SectionBreak::OddPage),"evenPage"=>Ok(SectionBreak::EvenPage),_=>Err(unsupported("word/document.xml","sectPr/type","native section break has no equivalent source section type"))}
}
fn recover_section_id(name:&str)->Option<String>{
    let (stem,hash)=name.strip_prefix("_ssection3a")?.rsplit_once('_')?;
    if hash.len()!=16||!hash.bytes().all(|b|b.is_ascii_hexdigit())||stem.is_empty()||!stem.as_bytes()[0].is_ascii_alphabetic()||!stem.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){return None;}
    let separators=stem.bytes().enumerate().filter_map(|(index,b)|(b==b'_').then_some(index)).collect::<Vec<_>>();
    if separators.len()>6{return None;}
    for mask in 0..(1usize<<separators.len()){
        let mut candidate=stem.as_bytes().to_vec();for(bit,index)in separators.iter().enumerate(){if mask&(1<<bit)!=0{candidate[*index]=b'-';}}
        let candidate=String::from_utf8(candidate).ok()?;
        if crate::docx_writer::section_bookmark_name(&candidate)==name{return Some(candidate);}
    }None
}
fn source_section_names(first:Option<&XmlNode>)->Vec<&str>{
    let mut names=Vec::new();
    if let Some(first)=first{for node in std::iter::once(first).chain(first.descendants()){
        if node.name=="w:bookmarkStart"{if let Some(name)=node.attr("w:name").filter(|name|name.starts_with("_ssection3a")){names.push(name);}}
    }}names
}
fn source_section_id(first:Option<&XmlNode>,ordinal:usize,used:&mut HashSet<String>)->Result<String>{
    let names=source_section_names(first);
    let restored=names.iter().filter_map(|name|recover_section_id(name)).collect::<HashSet<_>>();
    if restored.len()>1{return Err(unsupported("word/document.xml","sectPr/bookmark","ambiguous source section identity"));}
    if let Some(id)=restored.into_iter().next(){if !used.insert(id.clone()){return Err(unsupported("word/document.xml","sectPr/bookmark","duplicate source section identity"));}return Ok(id);}
    let seed=names.first().map(|name|(*name).to_string()).unwrap_or_else(||format!("native-section:{ordinal}"));
    for nonce in 0..=crate::publication_sections::MAX_SECTIONS{
        // Short enough for the Word bookmark to retain its complete stem, so
        // the next conversion recovers this generated identity without drift.
        let id=format!("s{}",&format!("{:x}",Sha256::digest(format!("{seed}:{nonce}").as_bytes()))[..10]);
        if used.insert(id.clone()){return Ok(id);}
    }Err(error("word/document.xml","sectPr/bookmark","source section identity budget"))
}
/// Can be computed before citation rendering, without loading note stories or
/// resources. Only native section markers receive the restored section target.
pub(super) fn source_section_anchors(body:&XmlNode)->Result<HashMap<String,String>>{
    let mut policies=NotePolicies::default();let mut diagnostics=Vec::new();let mut stats=ParseStats::default();
    let sections=section_patches(body,&mut policies,&mut diagnostics,&mut stats)?;
    ImportedNoteSettings{sections,..Default::default()}.plan_source_sections(body).map(|plan|plan.anchors)
}

fn relation<'a>(rels:&'a[DocxRel],kind:&str)->Result<Option<&'a DocxRel>>{
    let mut found=rels.iter().filter(|rel|rel.kind==format!("{REL}/{kind}")||rel.kind==format!("{STRICT_REL}/{kind}"));
    let Some(rel)=found.next()else{return Ok(None)};
    if found.next().is_some()||!matches!(rel.mode.as_str(),""|"Internal"){return Err(error("word/_rels/document.xml.rels",kind,"ambiguous or external note-settings relationship"));}Ok(Some(rel))
}
pub(super) fn load_part(entries:&[(String,Vec<u8>)],rels:&[DocxRel],kind:&str,root:&str,stats:&mut ParseStats)->Result<Option<(String,XmlNode)>>{
    let Some(rel)=relation(rels,kind)?else{return Ok(None)};
    let part=docx_notes::member("word",&rel.raw).map_err(|_|error("word/_rels/document.xml.rels",kind,"invalid package member"))?;
    let bytes=docx_notes::unique(entries,&part).map_err(|_|error(&part,"/","missing or duplicate package part"))?;
    let parsed=docx_notes::xml(bytes).map_err(|_|error(&part,"/","invalid bounded namespace-aware XML"))?;
    if parsed.name!=root{return Err(error(&part,"/","unexpected native root namespace"));}
    stats.parsed_parts+=1;Ok(Some((part,parsed)))
}
fn unique_child<'a>(node:&'a XmlNode,name:&str,part:&str,path:&str)->Result<Option<&'a XmlNode>>{
    let local=name.split_once(':').map(|(_,v)|v).unwrap_or(name);
    let mut found=node.children.iter().filter(|n|n.local==local);
    let Some(child)=found.next()else{return Ok(None)};
    if child.name!=name{return Err(unsupported(part,path,"foreign namespace on native note property"));}
    if found.next().is_some(){return Err(error(part,path,"duplicate native note property"));}Ok(Some(child))
}
fn attrs(node:&XmlNode,allowed:&[&str],part:&str,path:&str)->Result<()>{
    if node.attrs.iter().any(|(key,_)|key!="xmlns"&&!key.starts_with("xmlns:")&&!allowed.contains(&key.as_str())){return Err(unsupported(part,path,"unsupported attribute on native note property"));}Ok(())
}
fn leaf(node:&XmlNode,allowed:&[&str],part:&str,path:&str)->Result<()>{
    attrs(node,allowed,part,path)?;
    if !node.children.is_empty()||!node.text.trim().is_empty(){return Err(error(part,path,"native note property must be a leaf"));}Ok(())
}
fn val<'a>(node:&'a XmlNode,part:&str,path:&str)->Result<&'a str>{node.attr("w:val").ok_or_else(||error(part,path,"missing namespaced value"))}
fn unsigned(value:&str,max:u32,part:&str,path:&str)->Result<u32>{
    let value=value.trim();let raw=value.strip_prefix('+').unwrap_or(value);
    if raw.is_empty()||!raw.bytes().all(|b|b.is_ascii_digit()){return Err(error(part,path,"invalid native integer"));}
    raw.parse::<u32>().ok().filter(|v|*v<=max).ok_or_else(||error(part,path,"native integer outside representation range"))
}
fn boolean(node:&XmlNode,part:&str,path:&str)->Result<bool>{
    leaf(node,&["w:val"],part,path)?;
    match node.attr("w:val").unwrap_or("true"){"1"|"true"|"on"=>Ok(true),"0"|"false"|"off"=>Ok(false),_=>Err(error(part,path,"invalid native boolean"))}
}
fn parse_properties(node:&XmlNode,kind:NoteKind,part:&str,path:&str,section:bool,diagnostics:&mut Vec<Value>)->Result<NotePolicyPatch>{
    attrs(node,&[],part,path)?;
    if !node.text.trim().is_empty(){return Err(error(part,path,"native note properties may contain only property elements"));}
    let mut patch=NotePolicyPatch::default();let mut seen=HashSet::new();let mut previous=0;
    for child in &node.children{
        let child_path=format!("{path}/{}",child.name);
        let special_ref=!section&&child.name==format!("w:{}",kind.as_str());
        if !special_ref&&!seen.insert(child.local.as_str()){return Err(error(part,&child_path,"duplicate native note property"));}
        if !child.name.starts_with("w:"){return Err(unsupported(part,&child_path,"foreign namespace on native note property"));}
        // CT_FtnProps/CT_EdnProps and their document-wide counterparts share
        // this sequence; special story references belong only at the end of
        // document-wide properties. Parsing out of order would accept XML the
        // writer cannot faithfully round-trip under the fixed Word schema.
        if let Some(order)=match child.name.as_str(){"w:pos"=>Some(1),"w:numFmt"=>Some(2),"w:numStart"=>Some(3),"w:numRestart"=>Some(4),_ if special_ref=>Some(5),_=>None}{
            if order<previous{return Err(error(part,&child_path,"native note properties are out of schema order"));}previous=order;
        }
        match child.name.as_str(){
            "w:numFmt"=>{
                leaf(child,&["w:val","w:format"],part,&child_path)?;let format=val(child,part,&child_path)?;
                if !policy::WORD_NUMBER_FORMATS.contains(&format){return Err(error(part,&child_path,"unknown native number format"));}
                patch.number_format=Some(format.into());
                if let Some(pattern)=child.attr("w:format"){
                    if pattern.chars().count()>1024||pattern.chars().any(char::is_control){return Err(unsupported(part,&child_path,"custom number pattern outside source bounds"));}
                    patch.number_pattern=Some(pattern.into());
                }
            },
            "w:numStart"=>{leaf(child,&["w:val"],part,&child_path)?;patch.start=Some(unsigned(val(child,part,&child_path)?,u16::MAX as u32,part,&child_path)? as u16);},
            "w:numRestart"=>{leaf(child,&["w:val"],part,&child_path)?;patch.restart=Some(match val(child,part,&child_path)?{"continuous"=>NoteRestart::Continuous,"eachSect"=>NoteRestart::Section,"eachPage"=>NoteRestart::Page,_=>return Err(error(part,&child_path,"unknown native restart"))});},
            "w:pos"=>{
                leaf(child,&["w:val"],part,&child_path)?;let raw=val(child,part,&child_path)?;
                let placement=match raw{"pageBottom"=>NotePlacement::PageBottom,"beneathText"=>NotePlacement::BeneathText,"docEnd"=>NotePlacement::DocEnd,"sectEnd"=>NotePlacement::SectEnd,_=>return Err(error(part,&child_path,"unknown native placement"))};
                if !placement.valid_for(kind){return Err(unsupported(part,&child_path,"native placement is not equivalent for this Word note kind"));}
                if (section&&kind==NoteKind::Endnote)||(!section&&kind==NoteKind::Footnote){
                    diagnostics.push(json!({"code":"docx_note_position_ignored","part":part,"path":child_path,"value":raw,"kind":kind}));
                }else{patch.placement=Some(placement);}
            },
            "w:footnote" if !section&&kind==NoteKind::Footnote=>{leaf(child,&["w:id"],part,&child_path)?;},
            "w:endnote" if !section&&kind==NoteKind::Endnote=>{leaf(child,&["w:id"],part,&child_path)?;},
            _=>return Err(unsupported(part,&child_path,"native note property has no equivalent policy")),
        }
    }
    Ok(patch)
}

struct Story<'a>{kind:NoteKind,part:String,root:Cow<'a,XmlNode>,definitions:Cow<'a,HashMap<i32,usize>>}
fn story_index(root:&XmlNode,kind:NoteKind,part:&str)->Result<HashMap<i32,usize>>{
    let mut ids=HashSet::new();let mut definitions=HashMap::new();let name=format!("w:{}",kind.as_str());
    for(index,note)in root.children.iter().enumerate(){
        if note.name!=name{return Err(error(part,"/","unexpected note story child"));}
        let id=note.attr("w:id").ok_or_else(||error(part,"/","missing note id"))?.parse::<i32>().map_err(|_|error(part,"/","invalid note id"))?;
        if !ids.insert(id){return Err(error(part,"/","duplicate note id"));}
        match note.attr("w:type").unwrap_or("normal"){"normal"=>{definitions.insert(id,index);},"separator"|"continuationSeparator"|"continuationNotice"=>(),_=>return Err(error(part,"/","unknown note story type"))}
    }Ok(definitions)
}
fn special_text(note:&XmlNode,kind:NoteKind,part:&str,path:&str)->Result<(usize,String)>{
    // This is intentionally a structural test, not descendant text() flattening.
    // Rich stories are refused; their original package remains the source.
    attrs(note,&["w:id","w:type"],part,path)?;
    let mut markers=0;let mut text=String::new();let mut paragraphs=0;
    for p in &note.children{
        if p.name!="w:p"{return Err(unsupported(part,path,"complex special note story"));}paragraphs+=1;
        attrs(p,&["w:rsidR","w:rsidRPr","w:rsidP","w:rsidRDefault"],part,path)?;
        for run in &p.children{
            if run.name=="w:pPr"{
                for property in &run.children{
                    match property.name.as_str(){
                        "w:pStyle"=>{leaf(property,&["w:val"],part,path)?;if property.attr("w:val")!=Some(if kind==NoteKind::Endnote{"EndnoteText"}else{"FootnoteText"}){return Err(unsupported(part,path,"styled special note story"));}},
                        "w:spacing"=>{
                            leaf(property,&["w:after","w:before","w:line","w:lineRule"],part,path)?;
                            for(key,value)in &property.attrs{let valid=match key.as_str(){"w:after"|"w:before"=>value=="0","w:line"=>value=="240","w:lineRule"=>value=="auto",_=>key=="xmlns"||key.starts_with("xmlns:")};if !valid{return Err(unsupported(part,path,"formatted special note spacing"));}}
                        },
                        _=>return Err(unsupported(part,path,"formatted special note paragraph")),
                    }
                }continue;
            }
            if run.name!="w:r"{return Err(unsupported(part,path,"complex special note story"));}
            attrs(run,&["w:rsidR","w:rsidRPr","w:rsidDel"],part,path)?;
            for atom in &run.children{
                match atom.name.as_str(){
                    "w:separator"|"w:continuationSeparator"=>{leaf(atom,&[],part,path)?;markers+=1;},
                    "w:t"=>{
                        leaf_text(atom,part,path)?;text.push_str(&atom.text);
                        if text.chars().count()>policy::MAX_NOTICE{return Err(unsupported(part,path,"special note text outside source bounds"));}
                    },
                    "w:rPr" if atom.children.is_empty()&&atom.attrs.iter().all(|(k,_)|k=="xmlns"||k.starts_with("xmlns:"))=>(),
                    _=>return Err(unsupported(part,path,"formatted or executable special note story")),
                }
            }
        }
    }
    if paragraphs>1{return Err(unsupported(part,path,"multiple-paragraph special note story"));}Ok((markers,text))
}
fn leaf_text(node:&XmlNode,part:&str,path:&str)->Result<()>{
    attrs(node,&["xml:space"],part,path)?;
    if !node.children.is_empty()||node.attr("xml:space").is_some_and(|v|!matches!(v,"preserve"|"default")){return Err(unsupported(part,path,"complex special note text"));}Ok(())
}
fn specials(pr:Option<&XmlNode>,story:Option<&Story<'_>>,kind:NoteKind,patch:&mut NotePolicyPatch)->Result<()>{
    patch.separator=Some(NoteSeparator::None);patch.continuation_separator=Some(NoteSeparator::None);patch.continuation_notice=Some(String::new());
    let Some(pr)=pr else{return Ok(())};let ref_name=format!("w:{}",kind.as_str());let mut seen_ids=HashSet::new();let mut seen_types=HashSet::new();
    for reference in pr.children.iter().filter(|n|n.name==ref_name){
        let raw=reference.attr("w:id").ok_or_else(||error("settings",&ref_name,"missing special note id"))?;
        let id=raw.parse::<i32>().map_err(|_|error("settings",&ref_name,"invalid special note id"))?;
        if !seen_ids.insert(id){return Err(error("settings",&ref_name,"duplicate selected special id"));}
        let story=story.ok_or_else(||error("settings",&ref_name,"selected special story is missing"))?;
        let note=story.root.children.iter().find(|n|n.attr("w:id").and_then(|v|v.parse::<i32>().ok())==Some(id)).ok_or_else(||error(&story.part,&ref_name,"selected special id is missing"))?;
        let note_type=note.attr("w:type").unwrap_or("normal");
        if !seen_types.insert(note_type){return Err(error(&story.part,&ref_name,"ambiguous selected special type"));}
        let path=format!("/{}/{}[@w:id='{id}']",story.root.name,note.name);
        let(markers,text)=special_text(note,kind,&story.part,&path)?;
        match note_type{
            "separator"|"continuationSeparator"=>{
                let expected=if note_type=="separator"{"w:separator"}else{"w:continuationSeparator"};
                if markers>1||!text.is_empty()||note.children.iter().flat_map(|p|&p.children).flat_map(|r|&r.children).any(|n|matches!(n.name.as_str(),"w:separator"|"w:continuationSeparator")&&n.name!=expected){return Err(unsupported(&story.part,&path,"non-equivalent separator story"));}
                let value=if markers==1{NoteSeparator::Line}else{NoteSeparator::None};if note_type=="separator"{patch.separator=Some(value)}else{patch.continuation_separator=Some(value)}
            },
            "continuationNotice"=>{if markers!=0{return Err(unsupported(&story.part,&path,"non-text continuation notice"));}patch.continuation_notice=Some(text);},
            _=>return Err(error(&story.part,&path,"settings reference is not a special note")),
        }
    }Ok(())
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
enum Rule{Auto,Exact,AtLeast}
#[derive(Debug,Clone,Default,PartialEq)]
pub(super) struct Metrics{
    pub size:Option<u32>,pub size_cs:Option<u32>,pub cs:Option<bool>,pub rtl:Option<bool>,line:Option<u32>,rule:Option<Rule>,
    keep_lines:Option<bool>,keep_next:Option<bool>,widow:Option<bool>,
    widow_specific:bool,
}
impl Metrics{
    pub(super) fn overlay(&mut self,over:&Self){
        macro_rules! fields{($($field:ident),*)=>{$(if over.$field.is_some(){self.$field=over.$field;})*};}
        fields!(size,size_cs,cs,rtl,line,rule,keep_lines,keep_next,widow);
        if over.widow.is_some(){self.widow_specific=over.widow_specific;}
    }
    fn spacing(&self)->Option<LineSpacing>{self.line.map(|line|match self.rule.unwrap_or(Rule::Auto){
        Rule::Auto=>LineSpacing::Multiple(line as f64/240.),Rule::Exact=>LineSpacing::ExactPt(line as f64/20.),Rule::AtLeast=>LineSpacing::AtLeastPt(line as f64/20.),
    })}
}
fn native_measure(raw:&str,units_per_pt:f64,positive:bool,part:&str,path:&str)->Result<u32>{
    let raw=raw.trim();
    if raw.starts_with('-'){return Err(unsupported(part,path,"negative native measure has no equivalent note policy"));}
    let unit=raw.get(raw.len().saturating_sub(2)..).unwrap_or("");
    let factor=match unit{"pt"=>Some(1.),"in"=>Some(72.),"cm"=>Some(72./2.54),"mm"=>Some(72./25.4),"pc"|"pi"=>Some(12.),_=>None};
    let number=if let Some(factor)=factor{
        raw[..raw.len()-2].parse::<f64>().ok().map(|n|n*factor*units_per_pt).filter(|n|n.is_finite()&&*n>=0.).ok_or_else(||error(part,path,"invalid native universal measure"))?
    }else{return unsigned(raw,i32::MAX as u32,part,path).and_then(|n|if positive&&n==0{Err(error(part,path,"zero font size"))}else{Ok(n)})};
    if number>i32::MAX as f64||(number-number.round()).abs()>0.000001{return Err(unsupported(part,path,"native universal measure is not exactly representable in note units"));}
    if positive&&number==0.{return Err(error(part,path,"zero font size"));}Ok(number.round() as u32)
}
fn rpr(node:Option<&XmlNode>,part:&str,path:&str)->Result<Metrics>{
    let mut out=Metrics::default();let Some(node)=node else{return Ok(out)};
    for(name,complex)in[("w:sz",false),("w:szCs",true)]{
        if let Some(size)=unique_child(node,name,part,path)?{
            leaf(size,&["w:val"],part,path)?;let value=native_measure(val(size,part,path)?,2.,true,part,path)?;
            if complex{out.size_cs=Some(value)}else{out.size=Some(value)}
        }
    }
    for(name,target)in[("w:cs",&mut out.cs),("w:rtl",&mut out.rtl)]{
        if let Some(route)=unique_child(node,name,part,path)?{*target=Some(boolean(route,part,path)?);}
    }Ok(out)
}
fn ppr(node:Option<&XmlNode>,part:&str,path:&str)->Result<Metrics>{
    let mut out=Metrics::default();let Some(node)=node else{return Ok(out)};
    if let Some(spacing)=unique_child(node,"w:spacing",part,path)?{
        // Other spacing attributes belong to paragraph layout, not line-height.
        // They are retained by the ordinary local-layout importer when used.
        for(key,_)in &spacing.attrs{
            if key.split_once(':').is_some_and(|(_,local)|matches!(local,"line"|"lineRule"))&&!matches!(key.as_str(),"w:line"|"w:lineRule"){return Err(unsupported(part,path,"foreign namespace on line spacing"));}
        }
        if let Some(line)=spacing.attr("w:line"){out.line=Some(native_measure(line,20.,false,part,path)?);}
        if let Some(rule)=spacing.attr("w:lineRule"){out.rule=Some(match rule{"auto"=>Rule::Auto,"exact"=>Rule::Exact,"atLeast"=>Rule::AtLeast,_=>return Err(error(part,path,"unknown native line rule"))});}
    }
    for(name,target)in[("w:keepLines",&mut out.keep_lines),("w:keepNext",&mut out.keep_next),("w:widowControl",&mut out.widow)]{
        if let Some(node)=unique_child(node,name,part,path)?{*target=Some(boolean(node,part,path)?);}
    }out.widow_specific=out.widow.is_some();Ok(out)
}
#[derive(Clone)]
struct StyleDef{node:std::sync::Arc<XmlNode>,kind:String}
pub(super) struct StyleCache{
    part:String,index:HashMap<String,StyleDef>,duplicates:HashSet<String>,
    cache:HashMap<String,Metrics>,visiting:HashSet<String>,defaults:Metrics,
    default_paragraph:Option<String>,default_character:Option<String>,
}
impl StyleCache{
    pub(super) fn document_defaults(&self)->Metrics{self.defaults.clone()}
    pub(super) fn note_font_base(&mut self,end:bool,stats:&mut ParseStats)->Result<(u32,u32)>{
        let id=if end{"EndnoteText"}else{"FootnoteText"};
        if !self.index.contains_key(id){return Ok((18,18));}
        let mut values=self.defaults.clone();values.overlay(&self.resolve(id,"paragraph",stats)?);Ok((values.size.unwrap_or(18),values.size_cs.or(values.size).unwrap_or(18)))
    }
    pub(super) fn new(part:&str,root:Option<&XmlNode>)->Result<Self>{
        let mut out=Self{part:part.into(),index:HashMap::new(),duplicates:HashSet::new(),cache:HashMap::new(),visiting:HashSet::new(),defaults:Metrics::default(),default_paragraph:None,default_character:None};
        let Some(root)=root else{return Ok(out)};
        if let Some(defaults)=unique_child(root,"w:docDefaults",part,"/w:styles/w:docDefaults")?{
            if let Some(properties)=unique_child(defaults,"w:pPrDefault",part,"docDefaults/pPrDefault")?{out.defaults.overlay(&ppr(unique_child(properties,"w:pPr",part,"pPrDefault/pPr")?,part,"docDefaults/pPr")?);}
            if let Some(properties)=unique_child(defaults,"w:rPrDefault",part,"docDefaults/rPrDefault")?{out.defaults.overlay(&rpr(unique_child(properties,"w:rPr",part,"rPrDefault/rPr")?,part,"docDefaults/rPr")?);}
        }
        out.defaults.widow_specific=false;
        for style in root.children.iter().filter(|n|n.name=="w:style"){
            let Some(id)=style.attr("w:styleId")else{continue};
            if id.is_empty()||id.len()>1024{return Err(error(part,"/w:styles/w:style","invalid style identity"));}
            let kind=style.attr("w:type").unwrap_or("");
            if out.index.insert(id.into(),StyleDef{node:std::sync::Arc::new(style.clone()),kind:kind.into()}).is_some(){out.duplicates.insert(id.into());}
            if matches!(style.attr("w:default"),Some("1"|"true"|"on")){
                let slot=match kind{"paragraph"=>&mut out.default_paragraph,"character"=>&mut out.default_character,_=>continue};
                if slot.replace(id.into()).is_some(){return Err(error(part,"/w:styles/w:style","ambiguous default note style"));}
            }
        }Ok(out)
    }
    fn resolve(&mut self,id:&str,kind:&str,stats:&mut ParseStats)->Result<Metrics>{
        stats.step(&self.part,id)?;
        if self.duplicates.contains(id){return Err(error(&self.part,id,"duplicate effective style identity"));}
        let Some(style)=self.index.get(id).cloned()else{return Err(error(&self.part,id,"missing effective note style"))};
        if style.kind!=kind{return Err(error(&self.part,id,"note style has the wrong kind"));}
        if let Some(value)=self.cache.get(id){return Ok(value.clone());}
        if self.visiting.len()>=128||!self.visiting.insert(id.into()){return Err(error(&self.part,id,"cyclic or over-deep note style inheritance"));}
        let mut values=if let Some(base)=unique_child(&style.node,"w:basedOn",&self.part,id)?{
            leaf(base,&["w:val"],&self.part,id)?;let base=val(base,&self.part,id)?.to_string();self.resolve(&base,kind,stats)?
        }else{Metrics::default()};
        if kind=="paragraph"{
            let mut own=ppr(unique_child(&style.node,"w:pPr",&self.part,id)?,&self.part,id)?;
            if self.default_paragraph.as_deref()==Some(id){own.widow_specific=false;}
            values.overlay(&own);
        }
        values.overlay(&rpr(unique_child(&style.node,"w:rPr",&self.part,id)?,&self.part,id)?);
        self.visiting.remove(id);stats.resolved_styles+=1;self.cache.insert(id.into(),values.clone());Ok(values)
    }
    pub(super) fn paragraph(&mut self,p:&XmlNode,part:&str,path:&str,stats:&mut ParseStats)->Result<Metrics>{
        let mut values=self.defaults.clone();values.overlay(&self.paragraph_declarations(p,part,path,stats)?);Ok(values)
    }
    /// The paragraph-style chain without docDefaults. This distinguishes an
    /// inherited document baseline from a local author-sized code paragraph.
    pub(super) fn paragraph_declarations(&mut self,p:&XmlNode,part:&str,path:&str,stats:&mut ParseStats)->Result<Metrics>{
        let properties=unique_child(p,"w:pPr",part,path)?;let style=properties.map(|p|unique_child(p,"w:pStyle",part,path)).transpose()?.flatten();
        let id=if let Some(style)=style{leaf(style,&["w:val"],part,path)?;Some(val(style,part,path)?.into())}else{self.default_paragraph.clone()};
        // A missing referenced pStyle has no formatting effect (ISO 17.3.1.27).
        // An existing duplicate, broken inheritance chain or cycle still fails.
        let mut values=Metrics::default();if let Some(id)=id{if self.index.contains_key(id.as_str()){values.overlay(&self.resolve(&id,"paragraph",stats)?);}}
        values.overlay(&ppr(properties,part,path)?);Ok(values)
    }
    fn run(&mut self,run:&XmlNode,base:&Metrics,part:&str,path:&str,stats:&mut ParseStats)->Result<Metrics>{
        let mut values=base.clone();values.overlay(&self.run_overrides(run,part,path,stats)?);Ok(values)
    }
    pub(super) fn run_overrides(&mut self,run:&XmlNode,part:&str,path:&str,stats:&mut ParseStats)->Result<Metrics>{
        let properties=unique_child(run,"w:rPr",part,path)?;let style=properties.map(|p|unique_child(p,"w:rStyle",part,path)).transpose()?.flatten();
        let id=if let Some(style)=style{leaf(style,&["w:val"],part,path)?;Some(val(style,part,path)?.into())}else{self.default_character.clone()};
        // rStyle has the same missing-style rule (ISO 17.3.2.30).
        let mut values=Metrics::default();if let Some(id)=id{if self.index.contains_key(id.as_str()){values.overlay(&self.resolve(&id,"character",stats)?);}}
        values.overlay(&rpr(properties,part,path)?);Ok(values)
    }
    pub(super) fn is_code(&self,p:&XmlNode)->bool{
        let Some(id)=super::docx_pstyle_raw(p)else{return false};
        id=="Code"||self.index.get(id.as_str()).and_then(|s|s.node.children.iter().find(|n|n.name=="w:name")).and_then(|n|n.attr("w:val"))
            .is_some_and(|name|matches!(name.to_lowercase().as_str(),"code"|"source code"|"code block"))
    }
}

/// The existing code importer consumes this exact language-label signature as
/// fence metadata. Its smaller font is not author text and is not note sizing.
pub(super) fn code_label_runs(p:&XmlNode,code:bool)->Option<(usize,usize)>{
    if !code||p.descendants().iter().any(|n|matches!(n.name.as_str(),"m:oMath"|"w:footnoteReference"|"w:endnoteReference"|"w:fldChar")){return None;}
    let mut runs=p.children.iter().filter(|n|n.name=="w:r");let first=runs.next()?;let second=runs.next()?;
    let properties=first.children.iter().find(|n|n.name=="w:rPr")?;let label=super::docx_run_text(first);
    let property=|name:&str|properties.children.iter().find(|n|n.name==name).and_then(|n|n.attr("w:val"));
    (property("w:color")==Some("8A8F98")&&property("w:sz")==Some("15")&&super::docx_run_text(second)=="\n"
        &&!label.is_empty()&&label.len()<=40&&label.chars().all(|c|c.is_ascii_alphanumeric()||"_+.#-".contains(c)))
        .then_some((first as *const XmlNode as usize,second as *const XmlNode as usize))
}
fn paragraph_has_author_content(node:&XmlNode)->bool{
    if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:sdtPr"){return false;}
    if node.name=="w:t"&&!node.text.trim().is_empty(){return true;}
    if matches!(node.name.as_str(),"m:oMath"|"m:oMathPara"|"w:drawing"|"w:pict"|"w:object"){return true;}
    node.children.iter().any(paragraph_has_author_content)
}
struct ParagraphRecord{kind:NoteKind,id:i32,label:Option<String>,path:Vec<usize>,metrics:Metrics,structural:bool,layout_only:bool}
fn records(story:&Story<'_>,id:i32,label:Option<&str>,cache:&mut StyleCache,stats:&mut ParseStats,out:&mut Vec<ParagraphRecord>,text_styles:Option<&super::docx_text_style::RunStyleIndex>)->Result<()>{
    let index=*story.definitions.get(&id).ok_or_else(||error(&story.part,"/","missing referenced ordinary note"))?;
    let note=&story.root.children[index];
    fn runs(node:&XmlNode,base:&Metrics,cache:&mut StyleCache,part:&str,path:&str,stats:&mut ParseStats,out:&mut Vec<Metrics>,metadata:Option<(usize,usize)>,text_styles:Option<&super::docx_text_style::RunStyleIndex>)->Result<()>{
        stats.step(part,path)?;
        if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:pPr"|"w:sdtPr"){return Ok(());}
        if node.name=="w:r"{
            if metadata.is_some_and(|(label,separator)|{let identity=node as *const XmlNode as usize;identity==label||identity==separator}){return Ok(());}
            if node.children.iter().any(|n|(n.name=="w:t"&&!n.text.is_empty())||matches!(n.name.as_str(),"w:tab"|"w:br"|"w:cr")){
                stats.runs+=1;out.push(if let Some(index)=text_styles{index.run(part,node).ok_or_else(||error(part,path,"missing native run style identity"))?}else{cache.run(node,base,part,path,stats)?});
            }return Ok(());
        }
        for child in &node.children{runs(child,base,cache,part,path,stats,out,metadata,text_styles)?;}Ok(())
    }
    fn walk(node:&XmlNode,path:&mut Vec<usize>,story:&Story<'_>,id:i32,label:Option<&str>,cache:&mut StyleCache,stats:&mut ParseStats,out:&mut Vec<ParagraphRecord>,text_styles:Option<&super::docx_text_style::RunStyleIndex>)->Result<()>{
        stats.step(&story.part,"paragraphs")?;
        if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:sdtPr"){return Ok(());}
        if node.name=="w:p"{
            if out.len()>=MAX_PARAGRAPHS{return Err(error(&story.part,"paragraphs","note paragraph budget"));}
            stats.paragraphs+=1;let location=format!("{}:{id}:{path:?}",story.kind.as_str());let mut metrics=if let Some(index)=text_styles{index.paragraph(&story.part,node).ok_or_else(||error(&story.part,&location,"missing native paragraph style identity"))?}else{cache.paragraph(node,&story.part,&location,stats)?};
            let code=cache.is_code(node);let structural=code||super::docx_pstyle_raw(node).as_deref()==Some("TableText");
            let mut run_values=Vec::new();runs(node,&metrics,cache,&story.part,&location,stats,&mut run_values,code_label_runs(node,code),text_styles)?;
            if let Some(first)=run_values.first(){
                let mixed=run_values.iter().any(|m|m.size!=first.size||m.size_cs!=first.size_cs);
                if mixed&&(text_styles.is_none()||code){return Err(unsupported(&story.part,&location,"mixed run font sizes require an inline size model"));}
                if !mixed{metrics.size=first.size;metrics.size_cs=first.size_cs;}
            }else if let Some(properties)=unique_child(node,"w:pPr",&story.part,&location)?{
                metrics.overlay(&rpr(unique_child(properties,"w:rPr",&story.part,&location)?,&story.part,&location)?);
            }
            out.push(ParagraphRecord{kind:story.kind,id,label:label.map(str::to_string),path:path.clone(),metrics,structural,layout_only:!paragraph_has_author_content(node)});return Ok(());
        }
        for(index,child)in node.children.iter().enumerate(){path.push(index);walk(child,path,story,id,label,cache,stats,out,text_styles)?;path.pop();}Ok(())
    }
    walk(note,&mut Vec::new(),story,id,label,cache,stats,out,text_styles)
}

fn apply_metrics(kind:NoteKind,records:&[ParagraphRecord],policies:&mut NotePolicies,local:&mut Vec<NoteLocalPatch>){
    let records=records.iter().filter(|r|r.kind==kind).collect::<Vec<_>>();if records.is_empty(){return;}
    let template=records.iter().copied().filter(|r|!r.structural&&!r.layout_only).collect::<Vec<_>>();
    let patch=policies.kinds.entry(kind).or_default();
    if let Some(first)=template.first(){
        if template.iter().all(|r|r.metrics.size.is_some()){patch.size=first.metrics.size.map(|n|n as f64/2.);}
        if template.iter().all(|r|r.metrics.size_cs.is_some()){patch.size_cs=first.metrics.size_cs.map(|n|n as f64/2.);}
        if template.iter().all(|r|r.metrics.spacing().is_some()){patch.line_spacing=first.metrics.spacing();}
    }
    let keep=template.first().map(|r|r.metrics.keep_lines.unwrap_or(false)).unwrap_or(patch.long_note==Some(LongNote::KeepParagraphs));patch.long_note=Some(if keep{LongNote::KeepParagraphs}else{LongNote::Split});
    for record in records{
        let Some(label)=&record.label else{continue};let mut text=Map::new();let mut paragraph=Map::new();
        for(key,value,common)in[("size",record.metrics.size.map(|n|n as f64/2.),patch.size),("sizeCs",record.metrics.size_cs.map(|n|n as f64/2.),patch.size_cs)]{
            // Code and table text keep their existing structural conversion;
            // their independent fonts do not redefine FootnoteText/EndnoteText.
            if !record.structural&&!record.layout_only{if let Some(value)=value{if Some(value)!=common{text.insert(key.into(),json!(value));}}}
        }
        // The source's text.size controls both scripts. An equal native CS
        // size needs no extra field; a genuinely different CS value still does.
        if text.contains_key("size")&&record.metrics.size_cs.is_some()&&record.metrics.size_cs==record.metrics.size{text.remove("sizeCs");}
        // The existing local text.size applies to both scripts. A distinct
        // native complex-script value must not disappear behind that alias.
        if text.contains_key("size")&&record.metrics.size_cs.is_some()&&record.metrics.size_cs!=record.metrics.size{
            text.insert("sizeCs".into(),json!(record.metrics.size_cs.unwrap() as f64/2.));
        }
        if !record.structural&&!record.layout_only{if let Some(spacing)=record.metrics.spacing(){if Some(spacing)!=patch.line_spacing{paragraph.insert("lineSpacing".into(),policy::line_spacing_value(spacing));}}}
        if record.metrics.keep_lines.unwrap_or(false)!=keep{paragraph.insert("keepLines".into(),json!(!keep));}
        if let Some(value)=record.metrics.keep_next{paragraph.insert("keepNext".into(),json!(value));}
        // Normal/document-default true is the existing consumer default, not
        // a separate authored note override. Keep false and note/direct values.
        if let Some(value)=record.metrics.widow{if !value||record.metrics.widow_specific{paragraph.insert("widowControl".into(),json!(value));}}
        if text.is_empty()&&paragraph.is_empty(){continue;}
        let mut style=Map::new();
        let identity=format!("{}:{}:{:?}",kind.as_str(),record.id,record.path);
        style.insert("id".into(),json!(format!("note-{}-{:x}",if kind==NoteKind::Footnote{"f"}else{"e"},Sha256::digest(identity.as_bytes()))));
        if !text.is_empty(){style.insert("text".into(),Value::Object(text));}
        if !paragraph.is_empty(){style.insert("paragraph".into(),Value::Object(paragraph));}
        local.push(NoteLocalPatch{kind,native_id:record.id,label:label.clone(),paragraph_path:record.path.clone(),style:Value::Object(style)});
    }
}

fn section_patches(body:&XmlNode,policies:&mut NotePolicies,diagnostics:&mut Vec<Value>,stats:&mut ParseStats)->Result<Vec<SectionNotePatch>>{
    fn locate<'a>(node:&'a XmlNode,path:&mut Vec<usize>,out:&mut Vec<(Vec<usize>,&'a XmlNode)>,stats:&mut ParseStats)->Result<()>{
        stats.step("word/document.xml","sections")?;
        if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:sdtPr"|"w:tbl"|"w:pPrChange"|"w:sectPrChange"){return Ok(());}
        if node.name=="w:sectPr"{out.push((path.clone(),node));return Ok(());}
        for(index,child)in node.children.iter().enumerate(){path.push(index);locate(child,path,out,stats)?;path.pop();}Ok(())
    }
    let mut native=Vec::new();locate(body,&mut Vec::new(),&mut native,stats)?;
    let mut out=Vec::new();let mut start=0;let mut placements=Vec::new();
    for(path,node)in native{
        let path_text=format!("/w:body/{path:?}/w:sectPr");
        if node.children.iter().any(|c|c.local=="footnoteColumns"){return Err(unsupported("word/document.xml",&path_text,"independent footnote columns require a note column model"));}
        let foot=unique_child(node,"w:footnotePr","word/document.xml",&path_text)?.map(|n|parse_properties(n,NoteKind::Footnote,"word/document.xml",&format!("{path_text}/w:footnotePr"),true,diagnostics)).transpose()?.unwrap_or_default();
        let end=unique_child(node,"w:endnotePr","word/document.xml",&path_text)?.map(|n|parse_properties(n,NoteKind::Endnote,"word/document.xml",&format!("{path_text}/w:endnotePr"),true,diagnostics)).transpose()?.unwrap_or_default();
        let suppress=unique_child(node,"w:suppressEndnotes","word/document.xml",&path_text)?.map(|n|boolean(n,"word/document.xml",&path_text)).transpose()?;
        let end_child=path.first().copied().unwrap_or(body.children.len()).saturating_add(usize::from(path.len()>1)).min(body.children.len());
        placements.push(foot.placement.unwrap_or(NotePlacement::PageBottom));
        out.push(SectionNotePatch{ordinal:out.len(),body_range:start..end_child,terminator_path:path,footnote:foot,endnote:end,suppress_endnotes:suppress});start=end_child;
    }
    if out.is_empty()||start<body.children.len()&&!body.children.last().is_some_and(|n|n.name=="w:sectPr"){
        placements.push(NotePlacement::PageBottom);out.push(SectionNotePatch{ordinal:out.len(),body_range:start..body.children.len(),terminator_path:Vec::new(),footnote:NotePolicyPatch::default(),endnote:NotePolicyPatch::default(),suppress_endnotes:None});
    }
    let foot_used=policies.kinds.contains_key(&NoteKind::Footnote);
    let uniform=placements.first().copied().filter(|first|placements.iter().all(|p|p==first));
    if foot_used{
        policies.kinds.get_mut(&NoteKind::Footnote).unwrap().placement=Some(uniform.unwrap_or(NotePlacement::PageBottom));
        if uniform.is_some(){for section in &mut out{section.footnote.placement=None;}}
        else{for(section,placement)in out.iter_mut().zip(placements){section.footnote.placement=Some(placement);}}
    }else{
        // Numbering remains sparse even when no note currently uses it. In
        // particular, explicit native defaults are still future section
        // overrides and must not be folded into or pruned with global defaults.
        // Placement retains its existing section-scoped effective routing.
        let positions_differ=placements.iter().any(|placement|*placement==NotePlacement::BeneathText);
        for(section,placement)in out.iter_mut().zip(placements){section.footnote.placement=positions_differ.then_some(placement);}
    }
    let needs_boundaries=out.len()>1&&policies.kinds.values().any(|p|p.restart==Some(NoteRestart::Section)||p.placement==Some(NotePlacement::SectEnd));
    let source_boundaries=body.descendants().iter().any(|node|node.name=="w:bookmarkStart"&&node.attr("w:name").is_some_and(|name|name.starts_with("_ssection3a")));
    if !needs_boundaries&&!source_boundaries&&out.iter().all(|s|s.footnote.is_empty()&&s.endnote.is_empty()&&s.suppress_endnotes.is_none()){out.clear();}Ok(out)
}

fn reference_contract(node:&XmlNode,stats:&mut ParseStats)->Result<()>{
    stats.step("word/document.xml","note references")?;
    if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:sdtPr"){return Ok(());}
    if matches!(node.name.as_str(),"w:footnoteReference"|"w:endnoteReference"){
        leaf(node,&["w:id","w:customMarkFollows"],"word/document.xml",&node.name)?;
        if let Some(mark)=node.attr("w:customMarkFollows"){
            match mark{
                "0"|"false"|"off"=>(),
                "1"|"true"|"on"=>return Err(unsupported("word/document.xml",&node.name,"custom note marks do not participate in automatic numbering")),
                _=>return Err(error("word/document.xml",&node.name,"invalid custom note mark boolean")),
            }
        }
    }
    for child in &node.children{reference_contract(child,stats)?;}Ok(())
}
fn prune_unused(patch:&mut NotePolicyPatch){
    // These are the fixed defaults written by the old builder even when its
    // document has no notes. Their presence alone is not an authored profile.
    if patch.size==Some(9.){patch.size=None;}
    if patch.size_cs==Some(9.){patch.size_cs=None;}
    if patch.line_spacing==Some(LineSpacing::Multiple(1.)){patch.line_spacing=None;}
    if patch.number_format.as_deref()==Some("decimal"){patch.number_format=None;}
    if patch.start==Some(1){patch.start=None;}
    if patch.restart==Some(NoteRestart::Continuous){patch.restart=None;}
    if matches!(patch.placement,Some(NotePlacement::PageBottom|NotePlacement::DocEnd)){patch.placement=None;}
    if patch.separator==Some(NoteSeparator::Line){patch.separator=None;}
    if patch.continuation_separator==Some(NoteSeparator::Line){patch.continuation_separator=None;}
    if patch.continuation_notice.as_deref()==Some(""){patch.continuation_notice=None;}
    if patch.long_note==Some(LongNote::Split){patch.long_note=None;}
}

/// Parse only current native values. `notes` must be the same Notes instance
/// used by the converter and called after citation suppression has finished.
pub(super) fn parse(entries:&[(String,Vec<u8>)],body:&XmlNode,rels:&[DocxRel],notes:&docx_notes::Notes)->Result<ImportedNoteSettings>{
    parse_inner(entries,body,rels,notes,None)
}
pub(super) fn parse_with_text_styles(entries:&[(String,Vec<u8>)],body:&XmlNode,rels:&[DocxRel],notes:&docx_notes::Notes,index:&super::docx_text_style::RunStyleIndex)->Result<ImportedNoteSettings>{
    parse_inner(entries,body,rels,notes,Some(index))
}
fn parse_inner(entries:&[(String,Vec<u8>)],body:&XmlNode,rels:&[DocxRel],notes:&docx_notes::Notes,text_styles:Option<&super::docx_text_style::RunStyleIndex>)->Result<ImportedNoteSettings>{
    let mut out=ImportedNoteSettings::default();
    let settings=load_part(entries,rels,"settings","w:settings",&mut out.stats)?;
    let settings_part=settings.as_ref().map(|(p,_)|p.as_str()).unwrap_or("settings");
    let mut properties=BTreeMap::new();
    for kind in NoteKind::ALL{
        let node=settings.as_ref().map(|(_,root)|unique_child(root,&format!("w:{}Pr",kind.as_str()),settings_part,"/w:settings")).transpose()?.flatten();
        properties.insert(kind,node);
    }
    if notes.settings_used().is_empty()&&properties.values().all(Option::is_none){
        // Source section bookmarks remain meaningful after Word removes its
        // unrelated note defaults. Match the early hyperlink map without
        // loading note stories or inventing an inactive note profile.
        out.sections=section_patches(body,&mut out.policies,&mut out.diagnostics,&mut out.stats)?;
        return Ok(out);
    }
    reference_contract(body,&mut out.stats)?;
    let used=notes.settings_used().iter().map(|(end,_)|NoteKind::from_end(*end)).collect::<HashSet<_>>();
    let mut stories=BTreeMap::new();
    for borrowed in notes.settings_stories(){
        let kind=NoteKind::from_end(borrowed.end);out.stats.borrowed_note_parts+=1;
        stories.insert(kind,Story{kind,part:borrowed.part.into(),root:Cow::Borrowed(borrowed.root),definitions:Cow::Borrowed(borrowed.definitions)});
    }
    for kind in NoteKind::ALL{
        if stories.contains_key(&kind)||properties[&kind].is_none(){continue;}
        let name=if kind.is_end(){"endnotes"}else{"footnotes"};
        if let Some((part,root))=load_part(entries,rels,name,&format!("w:{name}"),&mut out.stats)?{
            let definitions=story_index(&root,kind,&part)?;
            stories.insert(kind,Story{kind,part,root:Cow::Owned(root),definitions:Cow::Owned(definitions)});
        }
    }
    for kind in NoteKind::ALL{
        let pr=properties[&kind];if pr.is_none()&&!used.contains(&kind){continue;}
        let mut patch=pr.map(|node|parse_properties(node,kind,settings_part,&format!("/w:settings/w:{}Pr",kind.as_str()),false,&mut out.diagnostics)).transpose()?.unwrap_or_default();
        if used.contains(&kind){
            if patch.number_format.is_none(){patch.number_format=Some(if kind.is_end(){"lowerRoman"}else{"decimal"}.into());}
            if patch.start.is_none(){patch.start=Some(1);}
            if patch.restart.is_none(){patch.restart=Some(NoteRestart::Continuous);}
            if kind.is_end()&&patch.placement.is_none(){patch.placement=Some(NotePlacement::DocEnd);}
        }
        specials(pr,stories.get(&kind),kind,&mut patch)?;
        out.policies.kinds.insert(kind,patch);
    }
    let bindings=notes.settings_bindings();
    let mut labels=HashMap::new();
    for(end,id,label)in bindings{
        let kind=NoteKind::from_end(end);labels.insert((kind,id),label.clone());out.policies.kind_by_label.insert(label,kind);
    }
    if used.len()>1{
        // Current readable citation source has no per-citation native story
        // kind. In a mixed document its unbound generated note routes through
        // the legacy page default, which is provably correct only for footnotes.
        if let Some((_,id))=notes.settings_used().iter().find(|(end,id)|*end&&!labels.contains_key(&(NoteKind::Endnote,*id))){
            let part=stories.get(&NoteKind::Endnote).map(|s|s.part.as_str()).unwrap_or("endnotes");
            return Err(unsupported(part,&format!("/w:endnotes/w:endnote[@w:id='{id}']"),"mixed generated endnote requires a source citation kind association; the current citation source cannot reconstruct this native kind"));
        }
    }
    if used.len()==1{
        out.policies.position=Some(if used.contains(&NoteKind::Endnote){policy::NotePosition::Document}else{policy::NotePosition::Page});
    }
    let styles=if text_styles.is_none(){load_part(entries,rels,"styles","w:styles",&mut out.stats)?}else{None};
    let mut own_cache=StyleCache::new(styles.as_ref().map(|(p,_)|p.as_str()).unwrap_or("styles"),styles.as_ref().map(|(_,root)|root))?;
    let mut shared_cache=text_styles.map(|index|index.cache.borrow_mut());
    let cache=if let Some(cache)=shared_cache.as_deref_mut(){cache}else{&mut own_cache};
    if let Some(index)=text_styles{out.stats.parsed_parts+=index.stats.parsed_parts;out.stats.resolved_styles+=index.stats.resolved_styles;}
    let mut paragraphs=Vec::new();
    for(end,id)in notes.settings_used(){
        let kind=NoteKind::from_end(*end);let story=stories.get(&kind).ok_or_else(||error("note story","/","missing referenced story"))?;
        records(story,*id,labels.get(&(kind,*id)).map(String::as_str),cache,&mut out.stats,&mut paragraphs,text_styles)?;
    }
    for kind in NoteKind::ALL{
        if out.policies.kinds.contains_key(&kind)&&(!used.contains(&kind)||paragraphs.iter().all(|r|r.kind!=kind||r.structural||r.layout_only)){
            let id=if kind.is_end(){"EndnoteText"}else{"FootnoteText"};
            if cache.index.contains_key(id){
                let mut metrics=cache.defaults.clone();metrics.overlay(&cache.resolve(id,"paragraph",&mut out.stats)?);
                let patch=out.policies.kinds.get_mut(&kind).unwrap();
                patch.size=metrics.size.map(|v|v as f64/2.);patch.size_cs=metrics.size_cs.map(|v|v as f64/2.);patch.line_spacing=metrics.spacing();
                if let Some(keep)=metrics.keep_lines{patch.long_note=Some(if keep{LongNote::KeepParagraphs}else{LongNote::Split});}
            }
        }
        if used.contains(&kind){apply_metrics(kind,&paragraphs,&mut out.policies,&mut out.local);}
    }
    out.sections=section_patches(body,&mut out.policies,&mut out.diagnostics,&mut out.stats)?;
    for kind in NoteKind::ALL{
        if !used.contains(&kind){
            if let Some(patch)=out.policies.kinds.get_mut(&kind){prune_unused(patch);if patch.is_empty(){out.policies.kinds.remove(&kind);}}
        }
    }
    out.policies.declared=!out.policies.kinds.is_empty()||!out.policies.kind_by_label.is_empty();out.policies.extended=out.policies.declared;
    // All produced values must pass the same source validator. Failing here
    // preserves the original DOCX instead of emitting an unusable declaration.
    policy::parse_layer(&out.to_options()).map_err(|e|unsupported(settings_part,&e.path,"native value has no exact source-policy representation"))?;
    Ok(out)
}

#[cfg(test)]
#[path="docx_note_settings_tests.rs"]
mod tests;
