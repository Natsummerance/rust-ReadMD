//! Rust-PDF-only note identity. The author AST and public legacy JSON stay intact.
use crate::md_ast::{Block, Document, Inline};
use serde_json::{json, Value};
use std::{cell::{Cell, RefCell}, collections::{BTreeMap, BTreeSet, HashMap}};

pub(crate) const INVALID: &str = "publication_note_binding_mismatch";
pub(crate) const UNSUPPORTED: &str = "publication_note_policy_unsupported";
const BUDGET: &str = "publication_numbering_budget";
const MAX_VALUES: usize = 1_000_000;
const MAX_NOTES: usize = crate::publication_note_policies::MAX_LABELS;
const MAX_DEPTH: usize = 128;
// At most 32768 first-use notes and a start of 9999 produce index 42766.
// Word's repeated-letter format needs ceil(42766 / 26) = 1645 bytes;
// decimal and Roman are shorter. The operation still owns the 32 MiB cap.
const MAX_NUMBER_BYTES:usize=2048;
const PREFIX: &str = "readmd:rust-note:";

#[derive(Default)]
struct Work { values: usize, bytes: usize }
impl Work {
    fn step(&mut self, depth: usize, bytes: usize) -> Result<(), String> {
        self.values = self.values.checked_add(1).ok_or(BUDGET)?;
        self.bytes = self.bytes.checked_add(bytes).ok_or(BUDGET)?;
        if depth > MAX_DEPTH || self.values > MAX_VALUES || self.bytes > crate::document_transaction::MAX_DOCUMENT_BYTES { return Err(BUDGET.into()); }
        if self.values % 128 == 1 { crate::publication_budget::check()?; }
        Ok(())
    }
}
#[derive(Debug)]
struct Note { label: String, number: String, ordinal: usize, anchor: String, references: Vec<String>, used: Cell<usize> }

/// A single compilation owns its private destinations and exact occurrence list.
pub(crate) struct Projection {
    notes: HashMap<String, Note>, references: Vec<String>, cursor: Cell<usize>, error: RefCell<Option<String>>,
}
impl Projection {
    pub(crate) fn analyze(doc: &Document, style: &Value) -> Result<Option<Self>, String> {
        let mut work = Work::default();
        let mut definitions = BTreeSet::new(); let mut references = Vec::new(); let mut author_ids = BTreeSet::new();
        fn inlines(values: &[Inline], inside_note: bool, depth: usize, work: &mut Work, refs: &mut Vec<String>) -> Result<(), String> {
            for value in values {
                let bytes = match value { Inline::Text(s)|Inline::Code(s)|Inline::Math(s)|Inline::Html(s)|Inline::FootnoteRef(s)=>s.len(), Inline::OfficeMath{latex,omml}=>latex.len().checked_add(omml.len()).ok_or(BUDGET)?, _=>0 };
                work.step(depth, bytes)?;
                match value {
                    Inline::FootnoteRef(label) => { if inside_note { return Err(UNSUPPORTED.into()); } valid_label(label)?; if refs.len() >= MAX_VALUES { return Err(BUDGET.into()); } refs.push(label.clone()); },
                    Inline::Strong(c)|Inline::Emph(c)|Inline::Strike(c)|Inline::Styled{children:c,..}|Inline::AuthorStyled{children:c,..}|Inline::Display{children:c,..}|Inline::Citation{children:c,..}|Inline::Link{children:c,..}=>inlines(c,inside_note,depth+1,work,refs)?,
                    _=>(),
                }
            } Ok(())
        }
        fn blocks(values: &[Block], inside_note: bool, depth: usize, work: &mut Work, defs: &mut BTreeSet<String>, refs: &mut Vec<String>, ids: &mut BTreeSet<String>) -> Result<(),String> {
            for value in values {
                let bytes=match value{Block::Code{lang,text}=>lang.len().checked_add(text.len()).ok_or(BUDGET)?,Block::Html(s)|Block::Math(s)=>s.len(),Block::OfficeMath{latex,omml}=>latex.len().checked_add(omml.len()).ok_or(BUDGET)?,_=>0};
                work.step(depth,bytes)?; match value {
                Block::Styled{block,..}=>blocks(std::slice::from_ref(block.as_ref()),inside_note,depth+1,work,defs,refs,ids)?,
                Block::FootnoteDef{label,blocks:children}=>{ valid_label(label)?; work.step(depth,label.len())?; if inside_note || !defs.insert(label.clone()) { return Err(INVALID.into()); } if defs.len()>MAX_NOTES{return Err(BUDGET.into());} blocks(children,true,depth+1,work,defs,refs,ids)?; },
                Block::Paragraph(v)=>inlines(v,inside_note,depth+1,work,refs)?,
                Block::Heading{inlines:v,id,..}=>{ids.insert(id.clone());inlines(v,inside_note,depth+1,work,refs)?;},
                Block::Caption{label,inlines:v}=>{ids.insert(label.id.clone());inlines(v,inside_note,depth+1,work,refs)?;},
                Block::Quote(v)=>blocks(v,inside_note,depth+1,work,defs,refs,ids)?,
                Block::List{items,..}=>for item in items{blocks(&item.blocks,inside_note,depth+1,work,defs,refs,ids)?;},
                Block::Table{header,rows,..}=>{for cell in header{inlines(cell,inside_note,depth+1,work,refs)?;}for row in rows{for cell in row{inlines(cell,inside_note,depth+1,work,refs)?;}}},
                _=>(),
            }} Ok(())
        }
        blocks(&doc.blocks,false,0,&mut work,&mut definitions,&mut references,&mut author_ids)?;
        if definitions.is_empty() && references.is_empty(){return Ok(None);}
        if references.iter().any(|label|!definitions.contains(label)){return Err(INVALID.into());}
        // An unreferenced definition has no first-use number. Preserve the
        // source and fail before publication instead of dropping its content.
        let referenced=references.iter().collect::<BTreeSet<_>>();
        if definitions.iter().any(|label|!referenced.contains(label)){return Err(INVALID.into());}
        if crate::publication_sections::has_sections(doc){return Err("publication_sections_require_paged_engine".into());}
        let policies=crate::publication_note_policies::parse_layer(style).map_err(|e|e.code.to_string())?;
        crate::publication_note_policies::validate_consumer(crate::publication_note_policies::Consumer::Other,doc,style).map_err(|e|e.code.to_string())?;
        let layout=supported_style(style)?;
        if crate::publication_note_policies::used_kinds(doc,&policies).len()>1{return Err(UNSUPPORTED.into());}
        prepay_projection(&references,&layout,&mut work)?;
        let numbers=crate::publication_layout::note_numbers(doc,style);
        let nonce=uuid::Uuid::new_v4().simple().to_string(); let mut notes=HashMap::new();
        for (index,label) in references.iter().enumerate(){
            let ordinal=notes.len()+1;
            if !notes.contains_key(label){
                let number=*numbers.get(label).ok_or(INVALID)?;
                let anchor=format!("{PREFIX}{nonce}:d:{ordinal}"); if author_ids.contains(&anchor){return Err(INVALID.into());}
                let number=crate::publication_layout::note_number(&layout.note_format,number);if number.len()>MAX_NUMBER_BYTES{return Err(BUDGET.into());}
                notes.insert(label.clone(),Note{label:label.clone(),number,ordinal,anchor,references:Vec::new(),used:Cell::new(0)});
            }
            let anchor=format!("{PREFIX}{nonce}:r:{}",index+1);if author_ids.contains(&anchor){return Err(INVALID.into());}
            notes.get_mut(label).ok_or(INVALID)?.references.push(anchor);
        }
        Ok(Some(Self{notes,references,cursor:Cell::new(0),error:RefCell::new(None)}))
    }
    fn fail(&self)->Value{*self.error.borrow_mut()=Some(INVALID.into());Value::Null}
    pub(crate) fn reference(&self,label:&str)->Value{
        let cursor=self.cursor.get();if self.references.get(cursor).map(String::as_str)!=Some(label){return self.fail();}
        let Some(note)=self.notes.get(label)else{return self.fail()};
        let occurrence=note.used.get()+1;note.used.set(occurrence);
        self.cursor.set(cursor+1);self.marker(note,"reference",occurrence)
    }
    fn marker(&self,note:&Note,role:&str,occurrence:usize)->Value{
        let Some(reference)=note.references.get(occurrence-1)else{return self.fail()};
        json!({"t":"note-marker","role":role,"label":note.label,"number":note.number,"ordinal":note.ordinal,"occurrence":occurrence,"note":note.anchor,"reference":reference})
    }
    pub(crate) fn definition(&self,label:&str)->Value{self.notes.get(label).map(|n|self.marker(n,"definition",1)).unwrap_or_else(||self.fail())}
    pub(crate) fn backlinks(&self,label:&str)->Vec<Value>{let Some(note)=self.notes.get(label)else{self.fail();return Vec::new()};(2..=note.references.len()).map(|n|self.marker(note,"backlink",n)).collect()}
    pub(crate) fn finish(&self)->Result<(),String>{if let Some(error)=self.error.borrow().as_ref(){return Err(error.clone());}if self.cursor.get()!=self.references.len(){return Err(INVALID.into());}Ok(())}
}
fn valid_label(label:&str)->Result<(),String>{if label.is_empty()||label.len()>1024||label.chars().any(char::is_control){Err(INVALID.into())}else{Ok(())}}
// Each logical reference creates one reference marker and, across the whole
// graph, one definition/backlink marker. Charge a conservative compact JSON
// upper bound before creating UUID destinations, per-note vectors or JSON.
// 256 covers fixed keys, punctuation, roles and integer fields; each anchor
// is below 96 bytes. JSON escaping cannot exceed six bytes per label byte.
fn prepay_projection(references:&[String],layout:&crate::publication_layout::Layout,work:&mut Work)->Result<(),String>{
    let mut number_bytes:HashMap<&str,usize>=HashMap::new();
    for label in references{
        let bytes=if let Some(bytes)=number_bytes.get(label.as_str()){*bytes}else{
            if number_bytes.len()>=MAX_NOTES{return Err(BUDGET.into());}
            let index=number_bytes.len().checked_add(layout.note_start as usize).ok_or(BUDGET)?;
            let bytes=crate::publication_layout::note_number(&layout.note_format,index).len();
            if bytes>MAX_NUMBER_BYTES{return Err(BUDGET.into());}number_bytes.insert(label.as_str(),bytes);bytes
        };
        let escaped=label.len().checked_mul(6).ok_or(BUDGET)?;
        let marker_bytes=escaped.checked_add(bytes).and_then(|n|n.checked_add(256+2*96)).ok_or(BUDGET)?;
        work.step(0,marker_bytes)?;work.step(0,marker_bytes)?;
    }Ok(())
}
fn supported_style(style:&Value)->Result<crate::publication_layout::Layout,String>{
    for(key,valid)in[
        ("numberFormat",style["footnotes"].get("numberFormat").filter(|v|!v.is_null()).is_none_or(|v|v.as_str().is_some_and(|s|["decimal","lowerRoman","upperRoman","lowerLetter","upperLetter"].contains(&s)))),
        ("restart",style["footnotes"].get("restart").filter(|v|!v.is_null()).is_none_or(|v|v.as_str()==Some("continuous"))),
        ("start",style["footnotes"].get("start").filter(|v|!v.is_null()).is_none_or(|v|v.as_u64().is_some_and(|n|(1..=9999).contains(&n)))),
    ]{let _=key;if !valid{return Err(UNSUPPORTED.into());}}
    Ok(crate::publication_layout::Layout::from_style(style))
}

#[derive(Clone,Debug)]
pub(crate) struct Marker { pub role:String,pub label:String,pub number:String,pub ordinal:usize,pub occurrence:usize,pub note:String,pub reference:String }
impl Marker {
    pub(crate) fn parse(value:&Value)->Result<Self,String>{
        let object=value.as_object().ok_or(INVALID)?;
        const KEYS:[&str;8]=["t","role","label","number","ordinal","occurrence","note","reference"];
        if object.len()!=KEYS.len()||KEYS.iter().any(|k|!object.contains_key(*k))||value["t"]!="note-marker"{return Err(INVALID.into());}
        let string=|key:&str|->Result<String,String>{let text=value[key].as_str().ok_or(INVALID)?;
            let max=match key{"label"=>1024,"number"=>MAX_NUMBER_BYTES,"role"=>16,_=>96};if text.len()>max{return Err(BUDGET.into());}Ok(text.to_owned())};
        let integer=|key:&str|value[key].as_u64().filter(|n|*n>0&&*n<=MAX_VALUES as u64).map(|n|n as usize).ok_or_else(||INVALID.to_string());
        let marker=Self{role:string("role")?,label:string("label")?,number:string("number")?,ordinal:integer("ordinal")?,occurrence:integer("occurrence")?,note:string("note")?,reference:string("reference")?};
        valid_label(&marker.label)?;
        if !["reference","definition","backlink"].contains(&marker.role.as_str())||marker.number.is_empty()||!marker.number.bytes().all(|b|b.is_ascii_alphanumeric()){return Err(INVALID.into());}
        let (nonce,ordinal)=anchor(&marker.note,"d")?;let (other,_)=anchor(&marker.reference,"r")?;
        if nonce!=other||ordinal!=marker.ordinal||marker.role=="definition"&&marker.occurrence!=1||marker.role=="backlink"&&marker.occurrence<2{return Err(INVALID.into());}
        Ok(marker)
    }
    pub(crate) fn text(&self)->String{if self.role=="backlink"{format!("[{}]",self.occurrence)}else{self.number.clone()}}
    pub(crate) fn destination(&self)->Option<String>{match self.role.as_str(){"reference"=>Some(self.reference.clone()),"definition"=>Some(self.note.clone()),_=>None}}
    pub(crate) fn href(&self)->String{format!("#{}",if self.role=="reference"{&self.note}else{&self.reference})}
}
fn anchor(value:&str,role:&str)->Result<(String,usize),String>{
    let tail=value.strip_prefix(PREFIX).ok_or(INVALID)?;let pieces=tail.split(':').collect::<Vec<_>>();
    if pieces.len()!=3||pieces[0].len()!=32||!pieces[0].bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase())||pieces[1]!=role{return Err(INVALID.into());}
    let ordinal=pieces[2].parse::<usize>().ok().filter(|n|*n>0&&*n<=MAX_VALUES).ok_or(INVALID)?;if ordinal.to_string()!=pieces[2]{return Err(INVALID.into());}Ok((pieces[0].into(),ordinal))
}

/// Closed grammar for the new tokens; extension/metadata dictionaries stay
/// opaque. The graph is checked before font selection, resources or painting.
pub(crate) fn validate_json(value:&Value,style:&Value)->Result<(),String>{
    validated_destinations(value,style).map(|_|())
}
pub(crate) fn validated_destinations(value:&Value,style:&Value)->Result<BTreeSet<String>,String>{
    #[derive(Clone,Copy)]enum Role{Blocks,Block,Inlines,Inline,Items,Item,Cells,Rows,Bibs,Bib}
    let mut stack=vec![(value,Role::Blocks,0)];let mut work=Work::default();let mut markers=Vec::new();let mut author_ids=BTreeSet::new();
    while let Some((value,role,depth))=stack.pop(){work.step(depth,0)?;
        if !matches!(role,Role::Inline|Role::Inlines)&&(value["t"]=="note-marker"||value["type"]=="note-marker"){return Err(INVALID.into());}
        let mut field=|name:&str,next:Role|{if let Some(child)=value.get(name){stack.push((child,next,depth+1));}};
        match role{
            Role::Blocks|Role::Inlines|Role::Items|Role::Cells|Role::Rows|Role::Bibs if value.is_array()=>{let next=match role{Role::Blocks=>Role::Block,Role::Items=>Role::Item,Role::Cells=>Role::Inlines,Role::Rows=>Role::Cells,Role::Bibs=>Role::Bib,_=>Role::Inline};for item in value.as_array().unwrap().iter().rev(){stack.push((item,next,depth+1));}},
            Role::Block=>{
                if value.get("t").and_then(Value::as_str)==Some("note-marker"){return Err(INVALID.into());}
                match value.get("type").and_then(Value::as_str){Some("paragraph"|"heading"|"caption")=>{if let Some(id)=value.get("id").and_then(Value::as_str){author_ids.insert(id.to_owned());}field("text",Role::Inlines)},Some("quote")=>{field("blocks",Role::Blocks);field("title",Role::Inlines);},Some("list")=>field("items",Role::Items),Some("table")=>{field("rows",Role::Rows);field("header",Role::Cells);},Some("bibliography")=>field("items",Role::Bibs),Some("note-marker")=>return Err(INVALID.into()),_=>{if reserved_marker(value)?{return Err(INVALID.into());}}}
            },
            Role::Inline|Role::Inlines=>match value.get("t").and_then(Value::as_str){Some("note-marker")=>{let marker=Marker::parse(value)?;work.step(depth,marker.label.len()+marker.number.len()+marker.note.len()+marker.reference.len())?;markers.push(marker);},Some("bold"|"italic"|"strike"|"styled"|"author-styled"|"display")=>field("v",Role::Inlines),Some("link")=>field("text",Role::Inlines),_=>{if reserved_marker(value)?{return Err(INVALID.into());}}},
            Role::Item=>field("text",Role::Inlines),Role::Bib=>{field("text",Role::Inlines);field("firstField",Role::Inlines);},_=>{if reserved_marker(value)?{return Err(INVALID.into());}},
        }
    }
    if markers.is_empty(){return Ok(BTreeSet::new());}
    let layout=supported_style(style)?;
    #[derive(Default)]struct Graph{label:String,number:String,ordinal:usize,refs:BTreeMap<usize,String>,definition:Option<String>,backs:BTreeMap<usize,String>}
    let mut graphs:BTreeMap<String,Graph>=BTreeMap::new();let mut ref_ids=BTreeSet::new();let mut first_seen=Vec::new();let mut nonce=None;
    for m in markers{
        let (scope,_)=anchor(&m.note,"d")?;if nonce.as_ref().is_some_and(|n|n!=&scope){return Err(INVALID.into());}nonce=Some(scope);
        if !graphs.contains_key(&m.note)&&graphs.len()>=MAX_NOTES{return Err(BUDGET.into());}
        let graph=graphs.entry(m.note).or_default();if graph.label.is_empty(){graph.label=m.label.clone();graph.number=m.number.clone();graph.ordinal=m.ordinal;}
        if graph.label!=m.label||graph.number!=m.number||graph.ordinal!=m.ordinal{return Err(INVALID.into());}
        match m.role.as_str(){
            "reference"=>{if anchor(&m.reference,"r")?.1!=ref_ids.len()+1||m.occurrence!=graph.refs.len()+1||!ref_ids.insert(m.reference.clone())||graph.refs.insert(m.occurrence,m.reference).is_some(){return Err(INVALID.into());}if m.occurrence==1{first_seen.push(m.ordinal);}},
            "definition"=>{if graph.definition.replace(m.reference).is_some(){return Err(INVALID.into());}},
            "backlink"=>{if graph.backs.insert(m.occurrence,m.reference).is_some(){return Err(INVALID.into());}},_=>return Err(INVALID.into()),
        }
    }
    if first_seen!=(1..=graphs.len()).collect::<Vec<_>>(){return Err(INVALID.into());}
    let mut labels=BTreeSet::new();for graph in graphs.values(){
        let index=crate::publication_layout::note_index(style,graph.ordinal);
        if graph.number!=crate::publication_layout::note_number(&layout.note_format,index)||!labels.insert(&graph.label)||graph.refs.is_empty()||graph.definition.as_ref()!=graph.refs.get(&1)||graph.backs.len()+1!=graph.refs.len(){return Err(INVALID.into());}for (n,reference)in graph.refs.iter().skip(1){if graph.backs.get(n)!=Some(reference){return Err(INVALID.into());}}}
    let mut destinations=graphs.keys().cloned().collect::<BTreeSet<_>>();destinations.extend(ref_ids);if !destinations.is_disjoint(&author_ids){return Err(INVALID.into());}Ok(destinations)
}

// An unknown extension dictionary remains opaque. A typed note hidden inside
// renderer-owned but unsupported content arrays must instead fail closed.
fn reserved_marker(value:&Value)->Result<bool,String>{
    let mut stack=vec![(value,0)];let mut work=Work::default();
    while let Some((value,depth))=stack.pop(){work.step(depth,0)?;match value{
        Value::Array(values)=>for child in values{stack.push((child,depth+1));},
        Value::Object(_)=>{
            if value["t"]=="note-marker"||value["type"]=="note-marker"{return Ok(true);}
            for key in ["v","text","blocks","items","header","rows","firstField"]{if let Some(child)=value.get(key).filter(|child|child.is_array()){stack.push((child,depth+1));}}
        },_=>(),
    }}Ok(false)
}

pub(crate) fn private_destination(name:&str)->bool{name.starts_with(PREFIX)}

pub(crate) fn contains_marker(value:&Value)->bool{match value{
    Value::Array(values)=>values.iter().any(contains_marker),
    Value::Object(_)=>match value.get("t").and_then(Value::as_str){Some("note-marker")=>true,Some("bold"|"italic"|"strike"|"styled"|"author-styled"|"display")=>value.get("v").is_some_and(contains_marker),Some("link")=>value.get("text").is_some_and(contains_marker),_=>false},_=>false,
}}

#[cfg(test)]#[path="rust_pdf_notes_tests.rs"]mod tests;
