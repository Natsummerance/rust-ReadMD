//! Native author sizes are bound to immutable story/run nodes, not words.
//! The inherited cache is shared with note-policy import and parsed once.
use super::{docx_note_settings as notes, docx_notes, DocxRel, XmlNode};
use crate::publication_inline_text::{AuthorTextStyle, CascadeSize, HalfPoints};
use std::{cell::{Cell,RefCell}, collections::{HashMap,HashSet}};

pub(super) const INVALID: &str = crate::publication_inline_text::INVALID;
pub(super) const UNSUPPORTED: &str = crate::publication_inline_text::UNSUPPORTED;
pub(super) const BUDGET: &str = crate::publication_inline_text::BUDGET;
type Result<T> = std::result::Result<T, notes::Error>;
fn error(code: &'static str, part: &str, path: &str, detail: &str) -> notes::Error {
    notes::Error { code, part: part.into(), path: path.into(), detail: detail.chars().take(500).collect() }
}
fn author_error(mut error: notes::Error) -> notes::Error {
    error.code = if error.detail.contains("budget") { BUDGET }
        else if error.code == notes::UNSUPPORTED { UNSUPPORTED } else { INVALID };
    error
}
fn key(part: &str, node: &XmlNode) -> (String, usize) { (part.into(), node as *const XmlNode as usize) }
#[derive(Clone)]
struct RunRecord { effective: notes::Metrics, overrides: notes::Metrics, base: notes::Metrics, note_base: Option<(u32,u32)>, field: bool, table: bool, path: String }
pub(super) struct RunStyleIndex {
    pub cache: RefCell<notes::StyleCache>,
    pub stats: notes::ParseStats,
    paragraphs: HashMap<(String, usize), notes::Metrics>,
    runs: HashMap<(String, usize), RunRecord>,
    field_runs: HashMap<(String,usize),HashSet<usize>>,
    document_defaults: notes::Metrics,
    main_base: Option<u32>,
    main_base_consumed: Cell<bool>,
    pub block_styles: super::DocxStyles,
}
impl RunStyleIndex {
    pub fn build(entries: &[(String, Vec<u8>)], rels: &[DocxRel], body: &XmlNode, note_stories: &docx_notes::Notes) -> Result<Self> {
        let mut stats = notes::ParseStats::default();
        let styles = notes::load_part(entries, rels, "styles", "w:styles", &mut stats).map_err(author_error)?;
        let cache = notes::StyleCache::new(styles.as_ref().map(|(part, _)| part.as_str()).unwrap_or("styles"), styles.as_ref().map(|(_, root)| root)).map_err(author_error)?;
        let block_styles=styles.as_ref().map(|(_,root)|super::docx_styles_from_root(root)).unwrap_or_default();
        let document_defaults=cache.document_defaults();
        // The existing source typography.size leaf can reproduce only one
        // common, exact body size. Never invent a missing CS axis or clamp it.
        let main_base=document_defaults.size.filter(|size|Some(*size)==document_defaults.size_cs&&(16..=40).contains(size));
        let mut result = Self { cache: RefCell::new(cache), stats, paragraphs: HashMap::new(), runs: HashMap::new(), field_runs: HashMap::new(), document_defaults, main_base, main_base_consumed:Cell::new(false), block_styles };
        result.story("word/document.xml", body, false)?;
        for story in note_stories.settings_stories() {
            for (end, id) in note_stories.settings_used().iter().filter(|(end, _)| *end == story.end) {
                let _ = end;
                let node = &story.root.children[*story.definitions.get(id).ok_or_else(|| error(INVALID, story.part, "/", "missing referenced note"))?];
                result.story(story.part, node, true)?;
            }
        }
        Ok(result)
    }
    fn story(&mut self, part: &str, root: &XmlNode, note: bool) -> Result<()> {
        fn author_content(node:&XmlNode)->bool{
            if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:sdtPr"|"w:rPr"|"w:pPr"){return false;}
            if node.name=="w:r"&&node.children.iter().any(|child|child.name=="w:t"&&!child.text.is_empty()||matches!(child.name.as_str(),"w:tab"|"w:br"|"w:cr"|"w:footnoteReference"|"w:endnoteReference")){return true;}
            node.children.iter().any(author_content)
        }
        let code_base=if note{self.cache.borrow_mut().note_font_base(root.name=="w:endnote",&mut self.stats)?}else{crate::docx_writer::author_text_base(&serde_json::json!({}),crate::docx_writer::AuthorTextContext::Code)};
        fn walk(index: &mut RunStyleIndex, node: &XmlNode, part: &str, path: &mut Vec<usize>, paragraph: Option<(&XmlNode, notes::Metrics, bool, Option<(usize, usize)>)>, field: &mut Vec<usize>, simple: bool, note: bool, table: bool, run:Option<usize>,code_base:(u32,u32),code_issues:&mut Vec<(usize,String)>) -> Result<()> {
            if path.len()>256{return Err(error(BUDGET,part,"tree","author run tree depth"));}
            index.stats.step(part, &format!("{path:?}")).map_err(|e| if note { e } else { author_error(e) })?;
            if matches!(node.name.as_str(), "w:del" | "w:moveFrom" | "w:sdtPr" | "w:rPr" | "w:pPr") { return Ok(()); }
            let paragraph = if node.name == "w:p" {
                if index.paragraphs.len() >= 65536 { return Err(error(BUDGET, part, "paragraphs", "author paragraph budget")); }
                // Math-only main paragraphs belong to the established math
                // style resolver. No author run consumes this style chain.
                let base = if note||author_content(node){index.cache.borrow_mut().paragraph(node, part, &format!("{path:?}"), &mut index.stats).map_err(|e| if note { e } else { author_error(e) })?}else{notes::Metrics::default()};
                let code = index.cache.borrow().is_code(node);
                index.paragraphs.insert(key(part, node), base.clone());
                Some((node, base, code, notes::code_label_runs(node, code)))
            } else { paragraph };
            if node.name == "w:fldChar" {
                match node.attr("w:fldCharType").or_else(|| node.attr("fldCharType")) {
                    Some("begin") => field.push(node as *const XmlNode as usize), Some("end") => {field.pop();}, _ => (),
                }
            }
            if node.name == "w:r" {
                if let Some((paragraph_node, base, code, metadata)) = &paragraph {
                    let identity = node as *const XmlNode as usize;
                    let structural = metadata.is_some_and(|(label, separator)| identity == label || identity == separator);
                    let author = node.children.iter().any(|child| child.name == "w:t" && !child.text.is_empty() || matches!(child.name.as_str(), "w:tab" | "w:br" | "w:cr" | "w:footnoteReference" | "w:endnoteReference"));
                    let note_marker=note&&node.children.iter().any(|child|matches!(child.name.as_str(),"w:footnoteRef"|"w:endnoteRef"));
                    if (author||note_marker) && !structural {
                        let location = format!("{path:?}");
                        let overrides = index.cache.borrow_mut().run_overrides(node, part, &location, &mut index.stats).map_err(|e| if note { e } else { author_error(e) })?;
                        let mut effective = base.clone(); effective.overlay(&overrides);
                        let changed=effective.size.is_some_and(|value|value!=code_base.0)||effective.size_cs.is_some_and(|value|value!=code_base.1)||effective.cs==Some(true)||effective.rtl==Some(true);
                        if *code && changed {
                            if note{return Err(error(UNSUPPORTED, part, &location, "author run size or routing inside a fenced code block differs from its source baseline"));}
                            code_issues.push((*paragraph_node as *const XmlNode as usize,location.clone()));
                        }
                        let record = RunRecord { effective, overrides, base: base.clone(), note_base: note.then_some(code_base), field: simple || !field.is_empty() || node.children.iter().any(|child| child.name == "w:fldChar"), table, path: location };
                        index.runs.insert(key(part, node), record);
                    }
                }
            }
            let run=if node.name=="w:r"{Some(node as *const XmlNode as usize)}else{run};
            if matches!(node.name.as_str(),"w:t"|"w:tab"|"w:br"|"w:cr"|"w:footnoteReference"|"w:endnoteReference"){
                if let Some(run)=run.filter(|run|index.runs.contains_key(&(part.into(),*run))){for id in field.iter(){
                    index.stats.step(part,"field run identity").map_err(|e|if note{e}else{author_error(e)})?;
                    index.field_runs.entry((part.into(),*id)).or_default().insert(run);
                }}
            }
            let simple = simple || node.name == "w:fldSimple";
            if node.name=="w:fldSimple"{field.push(node as *const XmlNode as usize);}
            let table = table || node.name == "w:tbl";
            for (position, child) in node.children.iter().enumerate() {
                path.push(position); walk(index, child, part, path, paragraph.clone(), field, simple, note, table, run,code_base,code_issues)?; path.pop();
            }
            if node.name=="w:fldSimple"{field.pop();}
            Ok(())
        }
        let mut code_issues=Vec::new();
        walk(self, root, part, &mut Vec::new(), None, &mut Vec::new(), false, note, false, None,code_base,&mut code_issues)?;
        if !code_issues.is_empty(){
            let compatible=self.plain_default_code_paragraphs(part,root)?;
            if let Some((_,path))=code_issues.iter().find(|(paragraph,_)|!compatible.contains(paragraph)){
                return Err(error(UNSUPPORTED,part,path,"author run size or routing inside a fenced code block differs from its source baseline"));
            }
        }Ok(())
    }
    /// Legacy ordinary code has no editable local-size model. Keep that
    /// existing text/section conversion only for a whole uniform code group
    /// whose sizes come exclusively from docDefaults, never author overrides.
    fn plain_default_code_paragraphs(&mut self,part:&str,root:&XmlNode)->Result<HashSet<usize>>{
        fn axes(value:&notes::Metrics)->bool{value.size.is_some()||value.size_cs.is_some()||value.cs.is_some()||value.rtl.is_some()}
        fn finish(index:&mut RunStyleIndex,part:&str,group:&mut Vec<&XmlNode>,allowed:&mut HashSet<usize>)->Result<()>{
            if group.is_empty(){return Ok(());}
            let mut uniform=None;let mut valid=true;
            for paragraph in group.iter(){
                let declarations=index.cache.borrow_mut().paragraph_declarations(paragraph,part,"code paragraph",&mut index.stats).map_err(author_error)?;
                if axes(&declarations){valid=false;}
                for run in paragraph.descendants().into_iter().filter(|node|node.name=="w:r"){
                    let Some(record)=index.runs.get(&key(part,run))else{continue};
                    if record.field||record.table||axes(&record.overrides)||record.effective.cs==Some(true)||record.effective.rtl==Some(true){valid=false;}
                    let pair=(record.effective.size,record.effective.size_cs);
                    if uniform.is_some_and(|old|old!=pair){valid=false;}else{uniform=Some(pair);}
                }
            }
            if valid&&uniform.is_some(){allowed.extend(group.iter().map(|paragraph|*paragraph as *const XmlNode as usize));}
            group.clear();Ok(())
        }
        let mut allowed=HashSet::new();let mut group=Vec::new();
        for node in super::docx_word_children(root){
            let ordinary=node.name=="w:p"&&self.cache.borrow().is_code(node)
                &&!node.descendants().iter().any(|child|matches!(child.name.as_str(),"m:oMath"|"w:footnoteReference"|"w:endnoteReference"|"w:fldChar"|"w:fldSimple"));
            if !ordinary{finish(self,part,&mut group,&mut allowed)?;continue;}
            group.push(node);
            if node.children.iter().any(|child|child.name=="w:pPr"&&child.children.iter().any(|property|property.name=="w:sectPr")){
                finish(self,part,&mut group,&mut allowed)?;
            }
        }
        finish(self,part,&mut group,&mut allowed)?;Ok(allowed)
    }
    pub fn main_text_base(&self,context:crate::docx_writer::AuthorTextContext)->(u32,u32){
        if let Some(size)=self.main_base{
            match context{
                crate::docx_writer::AuthorTextContext::Body=>{self.main_base_consumed.set(true);return(size,size);},
                crate::docx_writer::AuthorTextContext::Caption=>{self.main_base_consumed.set(true);return(18,size);},
                _=>(),
            }
        }
        crate::docx_writer::author_text_base(&serde_json::json!({}),context)
    }
    pub fn source_body_size(&self)->Option<f64>{self.main_base.filter(|_|self.main_base_consumed.get()).map(|size|size as f64/2.)}
    fn semantic_run_ids(&self,part:&str,node:&XmlNode,fields_only:bool)->HashSet<usize>{
        let mut ids=HashSet::new();
        for node in std::iter::once(node).chain(node.descendants()){
            let key=key(part,node);
            if self.runs.get(&key).is_some_and(|record|!fields_only||record.field){ids.insert(key.1);}
            if let Some(runs)=self.field_runs.get(&key){ids.extend(runs);}
        }ids
    }
    fn uniform_semantic_style(&self,part:&str,ids:HashSet<usize>,expected:Option<(u32,u32)>,caption:bool)->Result<AuthorTextStyle>{
        let mut first=None;
        for id in ids{
            let Some(record)=self.runs.get(&(part.into(),id))else{continue};
            let axes=(record.effective.size,record.effective.size_cs,record.effective.cs,record.effective.rtl);
            if first.is_some_and(|(old,_,_)|old!=axes){return Err(error(UNSUPPORTED,part,&record.path,"mixed native semantic field author axes have no single editable source style"));}
            first=Some((axes,record.table,record.note_base));
        }
        let Some(((size,size_cs,cs,rtl),table,note_base))=first else{return Ok(AuthorTextStyle::default());};
        // Match style(): semantic note controls inherit the actual note
        // template unless the caller supplied a more specific source base.
        let expected=if table{Some(crate::docx_writer::author_text_base(&serde_json::json!({}),crate::docx_writer::AuthorTextContext::Table))}else{expected.or(note_base)};
        let mut style=AuthorTextStyle::default();
        for(value,base,fixed,target)in[(size,expected.map(|base|base.0),caption&&size==Some(18),&mut style.size),(size_cs,expected.map(|base|base.1),false,&mut style.size_cs)]{
            if let Some(value)=value.filter(|value|Some(*value)!=base||part=="word/document.xml"&&self.main_base.is_none()&&!fixed){
                *target=CascadeSize::Points(HalfPoints::new(value).map_err(|_|error(INVALID,part,"field","font size outside exact half-point model"))?);
            }
        }
        style.cs=cs;style.rtl=rtl;Ok(style)
    }
    /// Only callers which already identified a live source semantic object
    /// may use this. Arbitrary DATE/opaque fields retain the strict old guard.
    pub fn semantic_source(&self,part:&str,node:&XmlNode,expected:Option<(u32,u32)>,source:&str)->Result<String>{
        let style=self.uniform_semantic_style(part,self.semantic_run_ids(part,node,false),expected,false)?;
        Ok(style.wrap_source(source))
    }
    pub fn caption_style(&self,part:&str,node:&XmlNode,expected:Option<(u32,u32)>,generated:&[&XmlNode])->Result<AuthorTextStyle>{
        let mut ids=self.semantic_run_ids(part,node,true);
        for run in generated{if self.runs.contains_key(&key(part,run)){ids.insert(*run as *const XmlNode as usize);}}
        self.uniform_semantic_style(part,ids,expected,true)
    }
    fn unrepresented_field_defaults(&self,part:&str,record:&RunRecord,expected:(Option<u32>,Option<u32>))->bool{
        // A matching bare-writer22 is not a source baseline when the real
        // empty-request exporter may use minimal21. Reject only a known
        // document axis actually consumed by that field baseline; Caption's
        // fixed western18 does not consume an unrelated document western22.
        part=="word/document.xml"&&!record.table&&self.main_base.is_none()
            &&[(self.document_defaults.size,record.effective.size,expected.0),(self.document_defaults.size_cs,record.effective.size_cs,expected.1)]
                .iter().any(|(document,effective,base)|document.is_some()&&document==effective&&document==base)
    }
    /// Restoring a generated citation suppresses the actual native note. Its
    /// author axes/routes must therefore be reproducible by the current note
    /// template, not merely unchanged as XML. Explicit false routes still
    /// need editable source; an unstyled CSL note cannot carry that intent.
    pub fn generated_note_author_reproducible(&self,part:&str,note:&XmlNode)->Result<bool>{
        let marker_name=match note.name.as_str(){"w:footnote"=>"w:footnoteRef","w:endnote"=>"w:endnoteRef",_=>return Err(error(INVALID,part,"note","expected the actual native note node"))};
        fn marker_run(node:&XmlNode,marker_name:&str)->bool{
            node.name=="w:r"&&node.children.iter().any(|child|child.name==marker_name)
        }
        fn harmless_boundary(node:&XmlNode)->bool{
            matches!(node.name.as_str(),"w:bookmarkStart"|"w:bookmarkEnd"|"w:proofErr"|"w:lastRenderedPageBreak")&&node.children.is_empty()&&node.text.trim().is_empty()
        }
        fn pure_gap(node:&XmlNode)->bool{
            node.name=="w:r"&&node.children.iter().filter(|child|child.name=="w:t").count()==1&&node.children.iter().any(|child|child.name=="w:t"&&child.text==" ")&&node.children.iter().all(|child|matches!(child.name.as_str(),"w:rPr"|"w:t")||harmless_boundary(child))
        }
        // The note tree was already bounded and validated by Notes/load and
        // this index. Each generated identity is checked at its own nodes.
        fn walk(index:&RunStyleIndex,part:&str,node:&XmlNode,marker_name:&str,gap:bool)->Result<bool>{
            if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:sdtPr"|"w:rPr"|"w:pPr"){return Ok(true);}
            if node.name=="w:r"{
                // A real marker can follow bookmarks or another legitimate
                // boundary. Never infer its identity from the first child or
                // skip it merely because that native run has no text.
                let structural=marker_run(node,marker_name)||gap;
                let author=node.children.iter().any(|child|child.name=="w:t"&&!child.text.is_empty()||matches!(child.name.as_str(),"w:tab"|"w:br"|"w:cr"));
                if !author&&!structural{return Ok(true);}
                let Some(record)=index.runs.get(&key(part,node))else{return if structural{Err(error(UNSUPPORTED,part,"note marker","native generated note marker has no validated author-axis identity"))}else{Ok(false)}};
                let reproducible=record.note_base.is_some_and(|base|record.effective.size==Some(base.0)&&record.effective.size_cs==Some(base.1))&&record.effective.cs.is_none()&&record.effective.rtl.is_none();
                if structural&&!reproducible{return Err(error(UNSUPPORTED,part,&record.path,"generated note marker or gap author axes cannot be reproduced by the source note marker template"));}
                return Ok(reproducible);
            }
            // Do not stop at the first author mismatch: a later marker/gap
            // error must still propagate instead of pretending manual source
            // can retain unsupported structural formatting.
            let mut reproducible=true;let mut after_marker=false;
            for child in &node.children{
                let gap=after_marker&&pure_gap(child);
                reproducible&=walk(index,part,child,marker_name,gap)?;
                // Only exact native leaf boundaries are transparent here;
                // their subtrees/foreign lookalikes never hide a real run.
                if !harmless_boundary(child){after_marker=marker_run(child,marker_name);}
            }Ok(reproducible)
        }
        walk(self,part,note,marker_name,false)
    }
    /// A verified generated note may instead become an authored, single
    /// paragraph note. Only its one actual citation control gets an editable
    /// author span; the native marker and its bare separator remain structural.
    /// This is a qualification hook, never a source/signature lookup by text.
    pub fn generated_note_authored_style(&self,part:&str,note:&XmlNode,control:&XmlNode)->Result<Option<AuthorTextStyle>>{
        self.generated_note_author_reproducible(part,note)?;
        let marker=match (part,note.name.as_str()){
            ("word/footnotes.xml","w:footnote")=>"w:footnoteRef",
            ("word/endnotes.xml","w:endnote")=>"w:endnoteRef",
            _=>return Ok(None),
        };
        fn boundary(node:&XmlNode)->bool{
            matches!(node.name.as_str(),"w:bookmarkStart"|"w:bookmarkEnd"|"w:proofErr"|"w:lastRenderedPageBreak")
                &&node.children.is_empty()&&node.text.trim().is_empty()
        }
        if !note.text.trim().is_empty()||note.children.iter().any(|node|!node.tail.trim().is_empty()){return Ok(None);}
        let paragraphs=note.children.iter().filter(|node|!boundary(node)).collect::<Vec<_>>();
        let [paragraph]=paragraphs.as_slice()else{return Ok(None)};
        if paragraph.name!="w:p"||!paragraph.text.trim().is_empty()||paragraph.children.iter().any(|node|!node.tail.trim().is_empty())
            ||paragraph.children.iter().filter(|node|node.name=="w:pPr").count()>1{return Ok(None);}
        let children=paragraph.children.iter().filter(|node|node.name!="w:pPr"&&!boundary(node)).collect::<Vec<_>>();
        let [marker_run,gap,actual]=children.as_slice()else{return Ok(None)};
        if !std::ptr::eq(*actual,control)||marker_run.name!="w:r"||!marker_run.text.trim().is_empty()
            ||marker_run.children.iter().filter(|node|node.name==marker).count()!=1
            ||marker_run.children.iter().any(|node|node.name!="w:rPr"&&node.name!=marker&&!boundary(node)){return Ok(None);}
        // Match the existing note importer separator exactly. A formatted or
        // compound whitespace run is actual author content, not disposable.
        if gap.name!="w:r"||!gap.text.trim().is_empty()||gap.children.len()!=1||gap.children[0].name!="w:t"||gap.children[0].text!=" "{return Ok(None);}
        if control.name!="w:sdt"||control.children.iter().filter(|node|node.name=="w:sdtContent").count()!=1
            ||!control.text.trim().is_empty()||control.children.iter().any(|node|!node.tail.trim().is_empty()||!matches!(node.name.as_str(),"w:sdtPr"|"w:sdtContent")){return Ok(None);}
        fn supported(node:&XmlNode,depth:usize,work:&mut usize)->bool{
            *work+=1;
            if depth>256||*work>crate::publication_inline_text::MAX_NODES*16||!node.text.trim().is_empty()||node.children.iter().any(|child|!child.tail.trim().is_empty()){return false;}
            match node.name.as_str(){
                "w:sdtContent"|"w:hyperlink"=>node.children.iter().all(|child|supported(child,depth+1,work)),
                "w:r"=>node.children.iter().filter(|child|child.name=="w:rPr").count()<=1
                    &&node.children.iter().all(|child|child.name=="w:rPr"
                        ||matches!(child.name.as_str(),"w:t"|"w:tab"|"w:br"|"w:cr")&&child.children.is_empty()
                        ||boundary(child)),
                _=>boundary(node),
            }
        }
        let contents=control.children.iter().find(|node|node.name=="w:sdtContent").expect("unique citation content");
        if !supported(contents,0,&mut 0){return Ok(None);}
        let ids=self.semantic_run_ids(part,control,false);
        if ids.is_empty()||ids.iter().any(|id|self.runs.get(&(part.into(),*id)).is_none_or(|record|
            record.field||record.table||record.effective.size.is_none()||record.effective.size_cs.is_none())){return Ok(None);}
        match self.uniform_semantic_style(part,ids,None,false){
            Ok(style)=>Ok(Some(style)),
            Err(error)if error.code==UNSUPPORTED=>Ok(None),
            Err(error)=>Err(error),
        }
    }
    pub fn paragraph(&self, part: &str, node: &XmlNode) -> Option<notes::Metrics> { self.paragraphs.get(&key(part, node)).cloned() }
    pub fn run(&self, part: &str, node: &XmlNode) -> Option<notes::Metrics> { self.runs.get(&key(part, node)).map(|record| record.effective.clone()) }
    pub fn style(&self, part: &str, run: &XmlNode, expected: Option<(u32,u32)>, field_expected: Option<(u32,u32)>) -> Result<AuthorTextStyle> {
        let Some(record) = self.runs.get(&key(part, run)) else { return Ok(AuthorTextStyle::default()); };
        let mut style = AuthorTextStyle::default();
        let expected=if record.table{let value=crate::docx_writer::author_text_base(&serde_json::json!({}),crate::docx_writer::AuthorTextContext::Table);(Some(value.0),Some(value.1))}else{expected.or(record.note_base).map(|value|(Some(value.0),Some(value.1))).unwrap_or((record.base.size,record.base.size_cs))};
        let field_expected=if record.table{expected}else{field_expected.or(record.note_base).map(|value|(Some(value.0),Some(value.1))).unwrap_or((record.base.size,record.base.size_cs))};
        for (value, direct, base, document, target) in [
            (record.effective.size, record.overrides.size, expected.0, self.document_defaults.size, &mut style.size),
            (record.effective.size_cs, record.overrides.size_cs, expected.1, self.document_defaults.size_cs, &mut style.size_cs),
        ] {
            // Note templates already recreate equivalent direct default sizes
            // written by Word's story post-processing. Keep every distinct
            // run value; ordinary main-story direct declarations stay explicit.
            if let Some(value) = value.filter(|value| Some(*value) != base || part=="word/document.xml"&&(direct.is_some()||self.main_base.is_none()&&document.is_some())) {
                *target = CascadeSize::Points(HalfPoints::new(value).map_err(|_| error(INVALID, part, &record.path, "font size outside exact half-point model"))?);
            }
        }
        style.cs = record.effective.cs; style.rtl = record.effective.rtl;
        if record.field && (self.unrepresented_field_defaults(part,record,field_expected)||record.effective.size.is_some_and(|value|Some(value)!=field_expected.0)||record.effective.size_cs.is_some_and(|value|Some(value)!=field_expected.1)||record.effective.cs==Some(true)||record.effective.rtl==Some(true)) {
            return Err(error(UNSUPPORTED, part, &record.path, "author-sized live field result has no editable field-run size model"));
        }
        Ok(style)
    }
    pub fn paragraph_has_author_style(&self,part:&str,paragraph:&XmlNode,expected:Option<(u32,u32)>,field_expected:Option<(u32,u32)>)->Result<bool>{
        for run in paragraph.descendants().into_iter().filter(|node|node.name=="w:r"){
            if !self.style(part,run,expected,field_expected)?.is_empty(){return Ok(true);}
        }Ok(false)
    }
    pub fn guard_inline_code(&self,part:&str,run:&XmlNode,text:&str,expected:Option<(u32,u32)>,field_expected:Option<(u32,u32)>)->Result<()>{
        if text.contains(['\n','\r'])&&!self.style(part,run,expected,field_expected)?.is_empty(){
            let path=self.runs.get(&key(part,run)).map(|record|record.path.as_str()).unwrap_or("run");
            return Err(error(UNSUPPORTED,part,path,"multiline author-sized inline code has no exact source code-span representation"));
        }Ok(())
    }
    pub fn wrap(&self, part: &str, run: &XmlNode, content: &str, expected: Option<(u32,u32)>, field_expected: Option<(u32,u32)>) -> Result<String> {
        if content.is_empty() { return Ok(String::new()); }
        let style = self.style(part, run, expected, field_expected)?;
        if style.is_empty() { return Ok(content.into()); }
        let edge = |text: &str| text.chars().map(|c| match c { ' ' => "&#32;".into(), '\t' => "&#9;".into(), '\r' => "&#13;".into(), '\n' => "<br/>".into(), c => c.to_string() }).collect::<String>();
        let leading = content.len() - content.trim_start_matches([' ', '\t', '\r', '\n']).len();
        let trailing = content.len() - content.trim_end_matches([' ', '\t', '\r', '\n']).len();
        let split = content.len().saturating_sub(trailing).max(leading);
        let content = format!("{}{}{}", edge(&content[..leading]), content[leading..split].replace('\n', "<br/>"), edge(&content[split..]));
        Ok(style.wrap_source(&content))
    }
}

#[cfg(test)]
#[path = "docx_text_style_tests.rs"]
mod tests;
