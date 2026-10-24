//! Sparse, source-owned note policies. Native DOCX values never pass through
//! the legacy export UI's clamping sanitizer. A layer is expanded before the
//! next layer is applied, so a new explicit legacy field overrides old kinds.
use crate::md_ast::{Block, Document};
pub use crate::publication_layout::NoteRestart;
pub use crate::publication_typography::LineSpacing;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const INVALID: &str = "publication_note_policy_invalid";
pub const UNSUPPORTED: &str = "publication_note_policy_unsupported";
pub const BINDING_MISMATCH: &str = "publication_note_binding_mismatch";
pub const MAX_LABELS: usize = 32768;
pub const MAX_NOTICE: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyError {
    pub code: &'static str,
    pub path: String,
}
pub type Error = PolicyError;
impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.path)
    }
}
impl std::error::Error for PolicyError {}
fn invalid(field: impl Into<String>) -> PolicyError { PolicyError { code: INVALID, path: field.into() } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind { Footnote, Endnote }
impl NoteKind {
    pub const ALL: [Self; 2] = [Self::Footnote, Self::Endnote];
    pub fn as_str(self) -> &'static str { match self { Self::Footnote => "footnote", Self::Endnote => "endnote" } }
    pub fn from_end(end: bool) -> Self { if end { Self::Endnote } else { Self::Footnote } }
    pub fn is_end(self) -> bool { self == Self::Endnote }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotePosition { Page, Document }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotePlacement { PageBottom, BeneathText, DocEnd, SectEnd }
impl NotePlacement {
    pub fn word_value(self) -> &'static str { match self {
        Self::PageBottom => "pageBottom", Self::BeneathText => "beneathText", Self::DocEnd => "docEnd", Self::SectEnd => "sectEnd",
    }}
    pub fn valid_for(self, kind: NoteKind) -> bool {
        matches!((kind,self), (NoteKind::Footnote,Self::PageBottom|Self::BeneathText)|(NoteKind::Endnote,Self::DocEnd|Self::SectEnd))
    }
}

/// Only section-owned numbering is accepted here. Document typography,
/// separators and special stories cannot enter a section through this type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectionNoteNumberingPatch {
    pub number_format: Option<String>,
    pub number_pattern: Option<String>,
    pub start: Option<u16>,
    pub restart: Option<NoteRestart>,
}
impl SectionNoteNumberingPatch {
    pub fn is_empty(&self)->bool { self==&Self::default() }
    pub fn validate(&self,field:&str)->Result<(),Error>{
        if self.number_format.as_deref().is_some_and(|format|!WORD_NUMBER_FORMATS.contains(&format)){
            return Err(invalid(format!("{field}.numberFormat")));
        }
        if let Some(pattern)=&self.number_pattern{text(&json!(pattern),&format!("{field}.numberPattern"),1024)?;}
        Ok(())
    }
    pub fn to_value(&self)->Value {
        let mut out=Map::new();
        if let Some(format)=&self.number_format{out.insert("numberFormat".into(),json!(format));}
        if let Some(pattern)=&self.number_pattern{out.insert("numberPattern".into(),json!(pattern));}
        if let Some(start)=self.start{out.insert("start".into(),json!(start));}
        if let Some(restart)=self.restart{out.insert("restart".into(),json!(match restart{NoteRestart::Continuous=>"continuous",NoteRestart::Section=>"section",NoteRestart::Page=>"page"}));}
        Value::Object(out)
    }
}
impl Serialize for SectionNoteNumberingPatch {
    fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{self.to_value().serialize(serializer)}
}

/// The source section vocabulary remains narrower than document note policies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectionNotePolicies {
    pub footnote_placement: Option<NotePlacement>,
    pub footnote_numbering: SectionNoteNumberingPatch,
    pub endnote_numbering: SectionNoteNumberingPatch,
}
impl SectionNotePolicies {
    pub fn is_empty(&self)->bool { self.footnote_placement.is_none()&&self.footnote_numbering.is_empty()&&self.endnote_numbering.is_empty() }
    pub fn validate(&self)->Result<(),Error>{
        if self.footnote_placement.is_some_and(|value|!value.valid_for(NoteKind::Footnote)){
            return Err(invalid("section.footnotes.kinds.footnote.placement"));
        }
        self.footnote_numbering.validate("section.footnotes.kinds.footnote")?;
        self.endnote_numbering.validate("section.footnotes.kinds.endnote")
    }
    pub fn to_value(&self)->Value {
        let mut kinds=Map::new();
        let mut footnote=self.footnote_numbering.to_value().as_object().unwrap().clone();
        if let Some(placement)=self.footnote_placement{footnote.insert("placement".into(),json!(placement));}
        if !footnote.is_empty(){kinds.insert("footnote".into(),Value::Object(footnote));}
        if !self.endnote_numbering.is_empty(){kinds.insert("endnote".into(),self.endnote_numbering.to_value());}
        if kinds.is_empty(){json!({})}else{json!({"kinds":kinds})}
    }
}
impl Serialize for SectionNotePolicies {
    fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{self.to_value().serialize(serializer)}
}

// Section notes have no sequence-valued fields. A dedicated map visitor keeps
// both arrays and duplicate keys (including null followed by a value) from
// becoming an apparently empty/default policy before the narrow parser runs.
struct SectionPolicyValue(Value);
impl<'de> Deserialize<'de> for SectionPolicyValue {
    fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error>{
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value=SectionPolicyValue;
            fn expecting(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("a section note object or scalar field")}
            fn visit_map<A:serde::de::MapAccess<'de>>(self,mut map:A)->Result<Self::Value,A::Error>{
                let mut out=Map::new();
                while let Some(key)=map.next_key::<String>()?{
                    if out.contains_key(&key){return Err(serde::de::Error::custom(format!("duplicate field `{key}`")));}
                    out.insert(key,map.next_value::<SectionPolicyValue>()?.0);
                }
                Ok(SectionPolicyValue(Value::Object(out)))
            }
            fn visit_unit<E:serde::de::Error>(self)->Result<Self::Value,E>{Ok(SectionPolicyValue(Value::Null))}
            fn visit_none<E:serde::de::Error>(self)->Result<Self::Value,E>{self.visit_unit()}
            fn visit_bool<E:serde::de::Error>(self,value:bool)->Result<Self::Value,E>{Ok(SectionPolicyValue(json!(value)))}
            fn visit_i64<E:serde::de::Error>(self,value:i64)->Result<Self::Value,E>{Ok(SectionPolicyValue(json!(value)))}
            fn visit_u64<E:serde::de::Error>(self,value:u64)->Result<Self::Value,E>{Ok(SectionPolicyValue(json!(value)))}
            fn visit_f64<E:serde::de::Error>(self,value:f64)->Result<Self::Value,E>{
                serde_json::Number::from_f64(value).map(|number|SectionPolicyValue(Value::Number(number))).ok_or_else(||E::custom("non-finite section note value"))
            }
            fn visit_str<E:serde::de::Error>(self,value:&str)->Result<Self::Value,E>{Ok(SectionPolicyValue(json!(value)))}
            fn visit_string<E:serde::de::Error>(self,value:String)->Result<Self::Value,E>{Ok(SectionPolicyValue(Value::String(value)))}
        }
        deserializer.deserialize_any(Visitor)
    }
}
impl<'de> Deserialize<'de> for SectionNotePolicies {
    fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error>{
        let value=SectionPolicyValue::deserialize(deserializer)?;
        parse_section(&value.0).map_err(serde::de::Error::custom)
    }
}
pub fn parse_section(value:&Value)->Result<SectionNotePolicies,Error>{
    let mut out=SectionNotePolicies::default();
    if value.is_null(){return Ok(out);}
    let root=value.as_object().ok_or_else(||invalid("section.footnotes"))?;
    for key in root.keys(){if key!="kinds"{return Err(invalid(format!("section.footnotes.{key}")));}}
    let Some(kinds)=root.get("kinds").filter(|value|!value.is_null())else{return Ok(out)};
    let kinds=kinds.as_object().ok_or_else(||invalid("section.footnotes.kinds"))?;
    for(name,value)in kinds{
        let field=format!("section.footnotes.kinds.{name}");
        let kind=match name.as_str(){"footnote"=>NoteKind::Footnote,"endnote"=>NoteKind::Endnote,_=>return Err(invalid(field))};
        if value.is_null(){continue;}
        let object=value.as_object().ok_or_else(||invalid(&field))?;
        let numbering=if kind==NoteKind::Footnote{&mut out.footnote_numbering}else{&mut out.endnote_numbering};
        for(key,value)in object{
            let path=format!("{field}.{key}");
            if !["numberFormat","numberPattern","start","restart"].contains(&key.as_str())&&!(kind==NoteKind::Footnote&&key=="placement"){return Err(invalid(path));}
            if value.is_null(){continue;}
            match key.as_str(){
                "numberFormat"=>numbering.number_format=Some(string(value,&path)?.into()),
                "numberPattern"=>numbering.number_pattern=Some(text(value,&path,1024)?),
                "start"=>numbering.start=Some(value.as_u64().filter(|n|*n<=u16::MAX as u64).ok_or_else(||invalid(&path))? as u16),
                "restart"=>numbering.restart=Some(match string(value,&path)?{"continuous"=>NoteRestart::Continuous,"section"=>NoteRestart::Section,"page"=>NoteRestart::Page,_=>return Err(invalid(path))}),
                "placement"=>out.footnote_placement=Some(match string(value,&path)?{"pageBottom"=>NotePlacement::PageBottom,"beneathText"=>NotePlacement::BeneathText,_=>return Err(invalid(path))}),
                _=>unreachable!(),
            }
        }
    }
    out.validate()?;Ok(out)
}
fn section_kind_object(options:&mut Value,kind:NoteKind)->&mut Map<String,Value>{
    if !options["footnotes"].is_object(){options["footnotes"]=json!({});}
    if !options["footnotes"]["kinds"].is_object(){options["footnotes"]["kinds"]=json!({});}
    if !options["footnotes"]["kinds"][kind.as_str()].is_object(){options["footnotes"]["kinds"][kind.as_str()]=json!({});}
    options["footnotes"]["kinds"][kind.as_str()].as_object_mut().unwrap()
}
/// Overlay this section on the document's original options, never the prior
/// section. Other note fields, label identities and raw numeric values survive.
pub fn resolve_section(global_options:&Value,patch:&SectionNotePolicies)->Result<Value,Error>{
    let global=parse_layer(global_options)?;patch.validate()?;
    let mut out=global_options.clone();
    let numbered=[(NoteKind::Footnote,&patch.footnote_numbering),(NoteKind::Endnote,&patch.endnote_numbering)];
    // Removing only a kind's field would expose a common inherited pattern.
    // Move that common value to the other kind before clearing this kind.
    if numbered.iter().any(|(_,numbering)|numbering.number_format.is_some()&&numbering.number_pattern.is_none()){
        if let Some(notes)=out.get_mut("footnotes").and_then(Value::as_object_mut){notes.remove("numberPattern");}
        for(kind,numbering)in &numbered{
            let inherited=if numbering.number_format.is_none()||numbering.number_pattern.is_some(){global.policy(*kind).number_pattern}else{None};
            if let Some(pattern)=inherited{section_kind_object(&mut out,*kind).insert("numberPattern".into(),json!(pattern));}
            else if let Some(object)=out.get_mut("footnotes").and_then(|notes|notes.get_mut("kinds")).and_then(|kinds|kinds.get_mut(kind.as_str())).and_then(Value::as_object_mut){
                object.remove("numberPattern");
            }
        }
    }
    for(kind,numbering)in numbered{
        if numbering.is_empty(){continue;}
        let object=section_kind_object(&mut out,kind);
        object.extend(numbering.to_value().as_object().unwrap().clone());
        if numbering.number_pattern.is_some()&&numbering.number_format.is_none()&&global.policy(kind).number_format.is_none(){
            object.insert("numberFormat".into(),json!(if kind==NoteKind::Endnote{"lowerRoman"}else{"decimal"}));
        }
    }
    if let Some(placement)=patch.footnote_placement{section_kind_object(&mut out,NoteKind::Footnote).insert("placement".into(),json!(placement));}
    Ok(out)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteSeparator { Line, None }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LongNote { Split, KeepParagraphs }

/// ST_NumberFormat, including native formats which only the DOCX consumer can
/// preserve. Flow consumers must not silently substitute decimal for these.
pub const WORD_NUMBER_FORMATS: &[&str] = &[
    "decimal","upperRoman","lowerRoman","upperLetter","lowerLetter","ordinal","cardinalText","ordinalText","hex","chicago",
    "ideographDigital","japaneseCounting","aiueo","iroha","decimalFullWidth","decimalHalfWidth","japaneseLegal","japaneseDigitalTenThousand",
    "decimalEnclosedCircle","decimalFullWidth2","aiueoFullWidth","irohaFullWidth","decimalZero","bullet","ganada","chosung",
    "decimalEnclosedFullstop","decimalEnclosedParen","decimalEnclosedCircleChinese","ideographEnclosedCircle","ideographTraditional",
    "ideographZodiac","ideographZodiacTraditional","taiwaneseCounting","ideographLegalTraditional","taiwaneseCountingThousand",
    "taiwaneseDigital","chineseCounting","chineseLegalSimplified","chineseCountingThousand","koreanDigital","koreanCounting","koreanLegal",
    "koreanDigital2","vietnameseCounting","russianLower","russianUpper","none","numberInDash","hebrew1","hebrew2","arabicAlpha","arabicAbjad",
    "hindiVowels","hindiConsonants","hindiNumbers","hindiCounting","thaiLetters","thaiNumbers","thaiCounting","bahtText","dollarText","custom",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NotePolicyPatch {
    pub size: Option<f64>,
    pub size_cs: Option<f64>,
    pub line_spacing: Option<LineSpacing>,
    pub number_format: Option<String>,
    /// XSLT number pattern, when the native w:numFmt carries w:format.
    pub number_pattern: Option<String>,
    pub start: Option<u16>,
    pub restart: Option<NoteRestart>,
    pub placement: Option<NotePlacement>,
    pub separator: Option<NoteSeparator>,
    pub continuation_separator: Option<NoteSeparator>,
    pub continuation_notice: Option<String>,
    pub long_note: Option<LongNote>,
}
pub fn line_spacing_value(spacing: LineSpacing) -> Value { match spacing {
    LineSpacing::Multiple(multiple) => json!({"mode":"multiple","multiple":multiple}),
    LineSpacing::ExactPt(pt) => json!({"mode":"exact","pt":pt}),
    LineSpacing::AtLeastPt(pt) => json!({"mode":"atLeast","pt":pt}),
}}
impl NotePolicyPatch {
    pub fn is_empty(&self) -> bool { self == &Self::default() }
    pub fn overlay(&mut self, over: &Self) {
        macro_rules! copy { ($($field:ident),* $(,)?) => { $(if over.$field.is_some() { self.$field = over.$field.clone(); })* }; }
        copy!(size,size_cs,line_spacing,number_format,number_pattern,start,restart,placement,separator,continuation_separator,continuation_notice,long_note);
    }
    pub fn to_value(&self) -> Value {
        let mut out = Map::new();
        macro_rules! value { ($field:ident,$key:literal) => { if let Some(value) = &self.$field { out.insert($key.into(),json!(value)); } }; }
        value!(size,"size"); value!(size_cs,"sizeCs"); value!(number_format,"numberFormat"); value!(number_pattern,"numberPattern"); value!(start,"start");
        value!(placement,"placement"); value!(separator,"separator"); value!(continuation_separator,"continuationSeparator"); value!(continuation_notice,"continuationNotice"); value!(long_note,"longNote");
        if let Some(spacing) = self.line_spacing { out.insert("lineSpacing".into(),line_spacing_value(spacing)); }
        if let Some(restart) = self.restart { out.insert("restart".into(),json!(match restart { NoteRestart::Continuous=>"continuous",NoteRestart::Section=>"section",NoteRestart::Page=>"page" })); }
        Value::Object(out)
    }
}
impl Serialize for NotePolicyPatch {
    fn serialize<S: Serializer>(&self, serializer:S)->Result<S::Ok,S::Error>{ self.to_value().serialize(serializer) }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NotePolicies {
    pub common: NotePolicyPatch,
    /// Already expanded for this layer: a value here overrides common.
    pub kinds: BTreeMap<NoteKind,NotePolicyPatch>,
    pub kind_by_label: BTreeMap<String,NoteKind>,
    pub position: Option<NotePosition>,
    pub title: Option<String>,
    pub declared: bool,
    /// True only for an actual new field, not the internally expanded legacy
    /// template. Old callers keep their existing consumer behavior.
    pub extended: bool,
}
impl NotePolicies {
    pub fn to_options(&self)->Value { json!({"footnotes":self.to_value()}) }
    pub fn kind_for_label(&self, label:&str)->NoteKind {
        self.kind_by_label.get(label).copied().unwrap_or(if self.position==Some(NotePosition::Document){NoteKind::Endnote}else{NoteKind::Footnote})
    }
    pub fn policy(&self, kind:NoteKind)->NotePolicyPatch {
        let mut out=self.common.clone();if let Some(patch)=self.kinds.get(&kind){out.overlay(patch);}out
    }
    /// Per-paragraph consumption never clones a potentially long continuation
    /// notice or a complete label map once per note.
    pub fn paragraph_policy(&self,kind:NoteKind)->NotePolicyPatch{
        let specific=self.kinds.get(&kind);
        NotePolicyPatch{
            size:specific.and_then(|p|p.size).or(self.common.size),
            size_cs:specific.and_then(|p|p.size_cs).or(self.common.size_cs),
            line_spacing:specific.and_then(|p|p.line_spacing).or(self.common.line_spacing),
            long_note:specific.and_then(|p|p.long_note).or(self.common.long_note),
            ..Default::default()
        }
    }
    pub fn overlay(&mut self, over:&Self) {
        self.common.overlay(&over.common);
        // Expansion happens per layer, not after the merged common template.
        for kind in NoteKind::ALL {
            if !over.common.is_empty() || over.kinds.contains_key(&kind) {
                let mut patch=over.common.clone();if let Some(specific)=over.kinds.get(&kind){patch.overlay(specific);}
                self.kinds.entry(kind).or_default().overlay(&patch);
            }
        }
        self.kind_by_label.extend(over.kind_by_label.clone());
        if over.position.is_some(){self.position=over.position;}
        if over.title.is_some(){self.title=over.title.clone();}
        self.declared|=over.declared;self.extended|=over.extended;
    }
    /// A readable footnotes object, independent of its front-matter carrier.
    pub fn to_value(&self)->Value {
        let mut out=self.common.to_value().as_object().unwrap().clone();
        if let Some(position)=self.position{out.insert("position".into(),json!(position));}
        if let Some(title)=&self.title{out.insert("title".into(),json!(title));}
        if !self.kinds.is_empty(){out.insert("kinds".into(),json!(self.kinds.iter().filter(|(_,v)|!v.is_empty()).map(|(k,v)|(k.as_str(),v.to_value())).collect::<BTreeMap<_,_>>()));}
        if !self.kind_by_label.is_empty(){out.insert("kindByLabel".into(),json!(self.kind_by_label));}
        Value::Object(out)
    }
}
impl Serialize for NotePolicies {
    fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{self.to_value().serialize(serializer)}
}

const POLICY_FIELDS:&[&str]=&["size","sizeCs","lineHeight","lineSpacing","numberFormat","numberPattern","start","restart","placement","separator","continuationSeparator","continuationNotice","longNote"];
fn string<'a>(value:&'a Value,field:&str)->Result<&'a str,PolicyError>{value.as_str().ok_or_else(||invalid(field))}
fn finite(value:&Value,field:&str,positive:bool)->Result<f64,PolicyError>{
    value.as_f64().filter(|n|n.is_finite() && if positive{*n>0.}else{*n>=0.}).ok_or_else(||invalid(field))
}
fn text(value:&Value,field:&str,max:usize)->Result<String,PolicyError>{
    let value=string(value,field)?;
    if value.chars().count()>max || value.chars().any(|c|c.is_control()&&!matches!(c,'\t'|'\n'|'\r')){return Err(invalid(field));}
    Ok(value.into())
}
/// Native measures are bounded by the DOCX integer representation, not by UI
/// slider bounds. Reject unrepresentable fractions instead of rounding them.
fn measure(value:&Value,field:&str,units:f64,positive:bool)->Result<f64,PolicyError>{
    let number=finite(value,field,positive)?;let raw=number*units;
    if raw>i32::MAX as f64 || (raw-raw.round()).abs()>0.000001{return Err(invalid(field));}Ok(number)
}
pub fn parse_line_spacing(value:&Value,field:&str)->Result<LineSpacing,PolicyError>{
    let object=value.as_object().ok_or_else(||invalid(field))?;
    let mode=object.get("mode").and_then(Value::as_str).ok_or_else(||invalid(format!("{field}.mode")))?;
    let key=match mode{"multiple"=>"multiple","exact"|"atLeast"=>"pt",_=>return Err(invalid(format!("{field}.mode")))};
    if object.len()!=2 || !object.contains_key(key){return Err(invalid(field));}
    let n=measure(&object[key],&format!("{field}.{key}"),if mode=="multiple"{240.}else{20.},false)?;
    Ok(match mode{"multiple"=>LineSpacing::Multiple(n),"exact"=>LineSpacing::ExactPt(n),_=>LineSpacing::AtLeastPt(n)})
}
pub fn parse_patch(value:&Value,field:&str,kind:Option<NoteKind>)->Result<NotePolicyPatch,PolicyError>{
    let object=value.as_object().ok_or_else(||invalid(field))?;let mut out=NotePolicyPatch::default();
    for(key,value)in object{
        let path=format!("{field}.{key}");
        if !POLICY_FIELDS.contains(&key.as_str()){return Err(invalid(path));}
        if value.is_null(){continue;}
        match key.as_str(){
            "size"=>out.size=Some(measure(value,&path,2.,true)?),
            "sizeCs"=>out.size_cs=Some(measure(value,&path,2.,true)?),
            "lineHeight"=>{
                if object.get("lineSpacing").is_some_and(|v|!v.is_null()){return Err(invalid(path));}
                out.line_spacing=Some(LineSpacing::Multiple(measure(value,&path,240.,false)?));
            },
            "lineSpacing"=>out.line_spacing=Some(parse_line_spacing(value,&path)?),
            "numberFormat"=>{let format=string(value,&path)?;if !WORD_NUMBER_FORMATS.contains(&format){return Err(invalid(path));}out.number_format=Some(format.into());},
            "numberPattern"=>out.number_pattern=Some(text(value,&path,1024)?),
            "start"=>out.start=Some(value.as_u64().filter(|n|*n<=u16::MAX as u64).ok_or_else(||invalid(&path))? as u16),
            "restart"=>out.restart=Some(match string(value,&path)?{"continuous"=>NoteRestart::Continuous,"section"=>NoteRestart::Section,"page"=>NoteRestart::Page,_=>return Err(invalid(path))}),
            "placement"=>{
                let placement=match string(value,&path)?{"pageBottom"=>NotePlacement::PageBottom,"beneathText"=>NotePlacement::BeneathText,"docEnd"=>NotePlacement::DocEnd,"sectEnd"=>NotePlacement::SectEnd,_=>return Err(invalid(path))};
                if kind.is_none_or(|kind|!placement.valid_for(kind)){return Err(invalid(path));}out.placement=Some(placement);
            },
            "separator"|"continuationSeparator"=>{
                let separator=match string(value,&path)?{"line"=>NoteSeparator::Line,"none"=>NoteSeparator::None,_=>return Err(invalid(path))};
                if key=="separator"{out.separator=Some(separator)}else{out.continuation_separator=Some(separator)}
            },
            "continuationNotice"=>out.continuation_notice=Some(text(value,&path,MAX_NOTICE)?),
            "longNote"=>out.long_note=Some(match string(value,&path)?{"split"=>LongNote::Split,"keepParagraphs"=>LongNote::KeepParagraphs,_=>return Err(invalid(path))}),
            _=>unreachable!(),
        }
    }
    Ok(out)
}
pub fn parse_footnotes(value:&Value)->Result<NotePolicies,PolicyError>{
    if value.is_null(){return Ok(NotePolicies::default());}
    let object=value.as_object().ok_or_else(||invalid("footnotes"))?;
    let mut common=Map::new();let mut out=NotePolicies{declared:true,..Default::default()};
    for(key,value)in object{
        let field=format!("footnotes.{key}");
        match key.as_str(){
            "position" if !value.is_null()=>out.position=Some(match string(value,&field)?{"page"=>NotePosition::Page,"document"=>NotePosition::Document,_=>return Err(invalid(field))}),
            "title" if !value.is_null()=>out.title=Some(text(value,&field,MAX_NOTICE)?),
            "kinds" if !value.is_null()=>{
                for(name,patch)in value.as_object().ok_or_else(||invalid(&field))?{
                    let kind=match name.as_str(){"footnote"=>NoteKind::Footnote,"endnote"=>NoteKind::Endnote,_=>return Err(invalid(format!("{field}.{name}")))};
                    if !patch.is_null(){let patch=parse_patch(patch,&format!("{field}.{name}"),Some(kind))?;out.extended|=!patch.is_empty();out.kinds.insert(kind,patch);}
                }
            },
            "kindByLabel" if !value.is_null()=>{
                let labels=value.as_object().ok_or_else(||invalid(&field))?;
                if labels.len()>MAX_LABELS{return Err(invalid(field));}
                for(label,value)in labels{
                    let path=format!("{field}[{}]",json!(label));
                    if label.is_empty()||label.len()>1024||label.chars().any(char::is_control){return Err(invalid(path));}
                    if value.is_null(){continue;}
                    out.extended=true;
                    out.kind_by_label.insert(label.clone(),match string(value,&path)?{"footnote"=>NoteKind::Footnote,"endnote"=>NoteKind::Endnote,_=>return Err(invalid(path))});
                }
            },
            "position"|"title"|"kinds"|"kindByLabel"=>(),
            _ if POLICY_FIELDS.contains(&key.as_str())=>{
                if !value.is_null()&&matches!(key.as_str(),"sizeCs"|"lineSpacing"|"numberPattern"|"placement"){out.extended=true;}
                common.insert(key.clone(),value.clone());
            },
            _=>return Err(invalid(field)),
        }
    }
    out.common=parse_patch(&Value::Object(common),"footnotes",None)?;
    out.extended|=out.common.size.is_some_and(|v|!(6. ..=24.).contains(&v))
        ||out.common.start.is_some_and(|v|v==0||v>9999)
        ||out.common.number_format.as_deref().is_some_and(|v|!crate::publication_layout::NUMBER_FORMATS.contains(&v))
        ||out.common.line_spacing.is_some_and(|v|matches!(v,LineSpacing::Multiple(n)if !(1. ..=3.).contains(&n)))
        ||out.common.continuation_notice.as_ref().is_some_and(|v|v.chars().count()>120||v.chars().any(char::is_control))
        ||out.title.as_ref().is_some_and(|v|v.chars().count()>120||v.chars().any(char::is_control));
    for kind in NoteKind::ALL{
        if !out.common.is_empty(){let mut patch=out.common.clone();if let Some(specific)=out.kinds.get(&kind){patch.overlay(specific);}out.kinds.insert(kind,patch);}
    }
    Ok(out)
}
pub fn parse_layer(options:&Value)->Result<NotePolicies,PolicyError>{
    if !options.is_object(){return Err(invalid("options"));}
    options.get("footnotes").map(parse_footnotes).unwrap_or_else(||Ok(NotePolicies::default()))
}

fn discard_nulls(value:&mut Value){
    if let Some(object)=value.as_object_mut(){object.retain(|_,v|!v.is_null());for value in object.values_mut(){discard_nulls(value);}}
}
fn clear_spacing(object:&mut Map<String,Value>,key:&str){
    if key=="lineSpacing"||key=="lineHeight"{object.remove("lineSpacing");object.remove("lineHeight");}else{object.remove(key);}
}
/// Prepare two sparse option objects for the existing recursive merge. Neither
/// unrelated publication fields nor presets are owned by this module.
pub fn merge_layers(base:&Value,over:&Value)->Result<(Value,Value),PolicyError>{
    parse_layer(base)?;parse_layer(over)?;
    let mut base=base.clone();let mut over=over.clone();
    if let Some(notes)=over.get_mut("footnotes"){
        if notes.is_null(){over.as_object_mut().unwrap().remove("footnotes");return Ok((base,over));}
        discard_nulls(notes);
        let overlay=notes.as_object().unwrap();
        if let Some(old)=base.get_mut("footnotes").and_then(Value::as_object_mut){
            for key in POLICY_FIELDS{
                if overlay.contains_key(*key){
                    if *key=="lineSpacing"||*key=="lineHeight"{old.remove("lineSpacing");old.remove("lineHeight");}
                    if let Some(kinds)=old.get_mut("kinds").and_then(Value::as_object_mut){
                        for patch in kinds.values_mut().filter_map(Value::as_object_mut){clear_spacing(patch,key);}
                    }
                }
            }
            if let Some(kinds)=overlay.get("kinds").and_then(Value::as_object){
                for(kind,patch)in kinds{
                    if let Some(patch)=patch.as_object(){
                        if patch.contains_key("lineSpacing")||patch.contains_key("lineHeight"){
                            if let Some(old)=old.get_mut("kinds").and_then(|v|v.get_mut(kind)).and_then(Value::as_object_mut){old.remove("lineSpacing");old.remove("lineHeight");}
                        }
                    }
                }
            }
        }
    }
    Ok((base,over))
}

pub fn validate_bindings(doc:&Document,policies:&NotePolicies)->Result<(),PolicyError>{
    fn definitions(blocks:&[Block],out:&mut BTreeMap<String,usize>){for block in blocks{match block.unstyled(){
        Block::FootnoteDef{label,blocks}=>{*out.entry(label.clone()).or_default()+=1;definitions(blocks,out);},
        Block::Quote(blocks)=>definitions(blocks,out),Block::List{items,..}=>for item in items{definitions(&item.blocks,out)},_=>(),
    }}}
    if policies.kind_by_label.is_empty(){return Ok(());}
    let mut found=BTreeMap::new();definitions(&doc.blocks,&mut found);
    for label in policies.kind_by_label.keys(){if found.get(label)!=Some(&1){return Err(PolicyError{code:BINDING_MISMATCH,path:format!("footnotes.kindByLabel[{}]",json!(label))});}}
    Ok(())
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Consumer { Docx, Html, Other }
pub fn has_section_policies(doc:&Document)->bool{
    fn walk(blocks:&[Block])->bool{blocks.iter().any(|block|match block.unstyled(){
        Block::SectionStart(section)=>section.footnotes.is_some(),
        Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>walk(blocks),
        Block::List{items,..}=>items.iter().any(|item|walk(&item.blocks)),_=>false,
    })}walk(&doc.blocks)
}
fn section_numbering_kinds(doc:&Document)->BTreeSet<NoteKind>{
    fn walk(blocks:&[Block],out:&mut BTreeSet<NoteKind>){for block in blocks{match block.unstyled(){
        Block::SectionStart(section)=>if let Some(notes)=&section.footnotes{
            if !notes.footnote_numbering.is_empty(){out.insert(NoteKind::Footnote);}
            if !notes.endnote_numbering.is_empty(){out.insert(NoteKind::Endnote);}
        },
        Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>walk(blocks,out),
        Block::List{items,..}=>for item in items{walk(&item.blocks,out)},_=>(),
    }}}
    let mut out=BTreeSet::new();walk(&doc.blocks,&mut out);out
}
/// Only actual P06-C fields select the new numbering contract. Existing
/// placement-only sections and null/inherited fields retain their old behavior.
pub fn has_section_numbering(doc:&Document)->bool{!section_numbering_kinds(doc).is_empty()}
/// Prove a consumer projection before any writer reads only flat fields.
pub fn validate_consumer(consumer:Consumer,doc:&Document,options:&Value)->Result<(),PolicyError>{
    let policies=parse_layer(options)?;validate_bindings(doc,&policies)?;
    match consumer{
        Consumer::Other=>{project_legacy(doc,options)?;},
        Consumer::Html=>validate_html(doc,options)?,
        Consumer::Docx=>{
            crate::publication_sections::validate_document(doc).map_err(|code|PolicyError{code,path:"sections".into()})?;
            if has_section_policies(doc){crate::publication_sections::resolve(doc,options).map_err(|code|PolicyError{code,path:"sections".into()})?;}
        },
    }
    Ok(())
}

pub fn used_kinds(doc:&Document,policies:&NotePolicies)->BTreeSet<NoteKind>{
    fn definitions<'a>(blocks:&'a[Block],out:&mut BTreeMap<&'a str,&'a[Block]>){for block in blocks{match block.unstyled(){
        Block::FootnoteDef{label,blocks}=>{out.entry(label.as_str()).or_insert(blocks.as_slice());definitions(blocks,out);},
        Block::Quote(blocks)=>definitions(blocks,out),Block::List{items,..}=>for item in items{definitions(&item.blocks,out)},_=>(),
    }}}
    fn refs(blocks:&[Block],defs:&BTreeMap<&str,&[Block]>,seen:&mut BTreeSet<String>,order:&mut Vec<String>){for block in blocks{match block.unstyled(){
        Block::FootnoteDef{..}=>(),Block::Quote(blocks)=>refs(blocks,defs,seen,order),Block::List{items,..}=>for item in items{refs(&item.blocks,defs,seen,order)},
        other=>crate::md_ast::walk_inlines(std::slice::from_ref(other),&mut|inline|{if let crate::md_ast::Inline::FootnoteRef(label)=inline{if defs.contains_key(label.as_str())&&seen.insert(label.clone()){order.push(label.clone());}}}),
    }}}
    let mut defs=BTreeMap::new();definitions(&doc.blocks,&mut defs);let mut seen=BTreeSet::new();let mut order=Vec::new();refs(&doc.blocks,&defs,&mut seen,&mut order);
    let mut at=0;while at<order.len(){let label=order[at].clone();at+=1;if let Some(blocks)=defs.get(label.as_str()){refs(blocks,&defs,&mut seen,&mut order);}}
    order.iter().map(|label|policies.kind_for_label(label)).collect()
}

#[derive(Debug,Clone,PartialEq,Eq)]
struct NumberingTuple {
    format:String,
    pattern:Option<String>,
    start:u16,
    restart:NoteRestart,
}
impl NumberingTuple {
    fn from_policy(kind:NoteKind,policy:&NotePolicyPatch)->Self{
        // P06-C compares native effective scopes. An omitted endnote format is
        // lowerRoman, even when this document has never passed through import.
        // Old global-only requests never enter this new section projection.
        Self{format:policy.number_format.clone().unwrap_or_else(||if kind.is_end(){"lowerRoman".into()}else{"decimal".into()}),pattern:policy.number_pattern.clone(),start:policy.start.unwrap_or(1),restart:policy.restart.unwrap_or(NoteRestart::Continuous)}
    }
    fn difference(&self,other:&Self)->Option<&'static str>{
        if self.format!=other.format{Some("numberFormat")}
        else if self.pattern!=other.pattern{Some("numberPattern")}
        else if self.start!=other.start{Some("start")}
        else if self.restart!=other.restart{Some("restart")}else{None}
    }
    fn unsupported_legacy_field(&self)->Option<&'static str>{
        if !crate::publication_layout::NUMBER_FORMATS.contains(&self.format.as_str()){Some("numberFormat")}
        else if self.pattern.is_some(){Some("numberPattern")}
        else if self.start==0||self.start>9999{Some("start")}else{None}
    }
    fn nondefault_field(&self)->Option<&'static str>{
        if self.format!="decimal"{Some("numberFormat")}
        else if self.pattern.is_some(){Some("numberPattern")}
        else if self.start!=1{Some("start")}
        else if self.restart!=NoteRestart::Continuous{Some("restart")}else{None}
    }
}
fn section_policy_error(id:&str,kind:NoteKind,key:&str)->PolicyError{
    PolicyError{code:UNSUPPORTED,path:format!("sections[{}].footnotes.kinds.{}.{key}",json!(id),kind.as_str())}
}
fn selected_legacy_kind(doc:&Document,policies:&NotePolicies)->Result<NoteKind,Error>{
    let kinds=used_kinds(doc,policies);
    if kinds.len()>1{return Err(PolicyError{code:UNSUPPORTED,path:"footnotes.kindByLabel".into()});}
    let fallback=policies.kind_for_label("");let kind=kinds.first().copied().unwrap_or(fallback);
    // Preflight can precede the CSL projection. Changing this fallback could
    // silently move its future generated notes into the other native story.
    if kind!=fallback{return Err(PolicyError{code:UNSUPPORTED,path:"footnotes.position".into()});}
    Ok(kind)
}
/// The decimal-only flow consumers can use this before flattening. Inspect the
/// original section provenance so an explicit default still selects P06-C, and
/// an old global-only request retains its established warnings contract.
pub fn validate_default_section_numbering(doc:&Document,options:&Value)->Result<(),Error>{
    let mut kinds=section_numbering_kinds(doc);
    if kinds.is_empty(){return Ok(());}
    let policies=parse_layer(options)?;
    kinds.insert(selected_legacy_kind(doc,&policies)?);
    let sections=crate::publication_sections::resolve(doc,options).map_err(|code|PolicyError{code,path:"sections".into()})?;
    for section in sections{
        let policies=parse_layer(&section.style)?;
        for kind in &kinds{
            if let Some(key)=NumberingTuple::from_policy(*kind,&policies.policy(*kind)).nondefault_field(){return Err(section_policy_error(&section.id,*kind,key));}
        }
    }
    Ok(())
}
/// Standalone HTML still numbers notes as first-use decimal order. Generic
/// legacy equivalence does not certify its renderer's format/start/restart.
pub fn validate_html(doc:&Document,options:&Value)->Result<(),Error>{
    project_legacy(doc,options)?;
    validate_default_section_numbering(doc,options)
}

/// Equivalent, consumer-only lowering. The readable source profile remains
/// untouched. Unsupported values fail before a legacy sanitizer can clamp or
/// ignore them; legacy warnings still describe its existing engine limits.
pub fn project_legacy(doc:&Document,options:&Value)->Result<Value,Error>{
    // A reserved but malformed section remains an HTML block in the forgiving
    // Markdown AST. It must not bypass the policy guard by failing to attach.
    crate::publication_sections::validate_document(doc).map_err(|code|PolicyError{code,path:"sections".into()})?;
    let original_policies=parse_layer(options)?;validate_bindings(doc,&original_policies)?;
    let numbering_kinds=section_numbering_kinds(doc);
    let has_numbering=!numbering_kinds.is_empty();
    let mut selected=None;
    let mut uniform_numbering=None;
    let projected=if has_section_policies(doc){
        let sections=crate::publication_sections::resolve(doc,options).map_err(|code|PolicyError{code,path:"sections".into()})?;
        let mut scoped_numbering=Vec::with_capacity(sections.len());
        for section in &sections{
            let policies=parse_layer(&section.style)?;
            let placement=policies.policy(NoteKind::Footnote).placement.unwrap_or(NotePlacement::PageBottom);
            if placement!=NotePlacement::PageBottom{return Err(PolicyError{code:UNSUPPORTED,path:format!("sections[{}].footnotes.kinds.footnote.placement",json!(section.id))});}
            // Retain only the small tuples, never a second label map or long
            // continuation notice for every resolved section.
            if has_numbering{scoped_numbering.push(NoteKind::ALL.map(|kind|(kind,NumberingTuple::from_policy(kind,&policies.policy(kind)))));}
        }
        if has_numbering{
            let kind=selected_legacy_kind(doc,&original_policies)?;
            selected=Some(kind);
            // A native eachPage value can be preserved in DOCX. No new flow
            // consumer has independently proved this section-owned behavior.
            // Equal effective values and unused notes do not erase provenance.
            for section in &sections{
                if let Some(overrides)=&section.note_overrides{
                    for(kind,numbering)in[(NoteKind::Footnote,&overrides.footnote_numbering),(NoteKind::Endnote,&overrides.endnote_numbering)]{
                        if numbering.restart==Some(NoteRestart::Page){return Err(section_policy_error(&section.id,kind,"restart"));}
                    }
                }
            }
            let uniform=&scoped_numbering[0];
            // Keep implicit prefixes and explicit empty sections in this proof.
            // Both kinds matter: an unused endnote override must not disappear
            // simply because today's body only references footnotes.
            for(section,actual)in sections.iter().zip(&scoped_numbering){
                for((kind,expected),(_,actual))in uniform.iter().zip(actual){
                    if let Some(key)=expected.difference(actual){return Err(section_policy_error(&section.id,*kind,key));}
                }
            }
            for(other,numbering)in uniform{
                if let Some(key)=numbering.unsupported_legacy_field(){return Err(section_policy_error(&sections[0].id,*other,key));}
                if *other!=kind{
                    if numbering_kinds.contains(other){
                        let origin=sections.iter().find(|section|section.note_overrides.as_ref().is_some_and(|notes|if other.is_end(){!notes.endnote_numbering.is_empty()}else{!notes.footnote_numbering.is_empty()})).unwrap_or(&sections[0]);
                        let overrides=origin.note_overrides.as_ref().unwrap();
                        let numbering=if other.is_end(){&overrides.endnote_numbering}else{&overrides.footnote_numbering};
                        let key=if numbering.number_format.is_some(){"numberFormat"}else if numbering.number_pattern.is_some(){"numberPattern"}else if numbering.start.is_some(){"start"}else{"restart"};
                        // A single flat kind cannot carry another independently
                        // explicit section policy, even when its current value
                        // equals a global default. Reject before it is dropped
                        // or re-evaluated against a projected common template.
                        return Err(section_policy_error(&origin.id,*other,key));
                    }
                }else{uniform_numbering=Some(numbering.clone());}
            }
            // Copy only note policy values. Page geometry, running content and
            // page numbering retain their existing independent section path.
            let mut projected=options.clone();
            if let Some(notes)=sections[0].style.get("footnotes"){projected["footnotes"]=notes.clone();}
            else if let Some(object)=projected.as_object_mut(){object.remove("footnotes");}
            projected
        }else{
            // Only after proving every scope, including an implicit prefix, can
            // a beneathText template be lowered to equivalent pageBottom.
            if original_policies.policy(NoteKind::Footnote).placement==Some(NotePlacement::BeneathText){
                resolve_section(options,&SectionNotePolicies{footnote_placement:Some(NotePlacement::PageBottom),..Default::default()})?
            }else{options.clone()}
        }
    }else{options.clone()};
    let options=&projected;
    let policies=parse_layer(options)?;
    if !policies.extended&&!has_numbering{return Ok(options.clone());}
    let kind=match selected{Some(kind)=>kind,None=>selected_legacy_kind(doc,&policies)?};
    let mut patch=policies.policy(kind);
    if let Some(numbering)=uniform_numbering{
        patch.number_format=Some(numbering.format);patch.number_pattern=numbering.pattern;
        patch.start=Some(numbering.start);patch.restart=Some(numbering.restart);
    }
    let path=|key:&str|if options["footnotes"].get("kinds").and_then(|v|v.get(kind.as_str())).and_then(|v|v.get(key)).is_some(){format!("footnotes.kinds.{}.{key}",kind.as_str())}else{format!("footnotes.{key}")};
    let fail=|key:&str|PolicyError{code:UNSUPPORTED,path:path(key)};
    if patch.size.is_some_and(|size|!(6.0..=24.0).contains(&size)){return Err(fail("size"));}
    if patch.size_cs.is_some_and(|size|size!=patch.size.unwrap_or(9.)){return Err(fail("sizeCs"));}
    if patch.start.is_some_and(|start|start==0||start>9999){return Err(fail("start"));}
    if patch.number_format.as_deref().is_some_and(|format|!crate::publication_layout::NUMBER_FORMATS.contains(&format)){return Err(fail("numberFormat"));}
    if patch.number_pattern.is_some(){return Err(fail("numberPattern"));}
    if let Some(placement)=patch.placement{if !matches!((kind,placement),(NoteKind::Footnote,NotePlacement::PageBottom)|(NoteKind::Endnote,NotePlacement::DocEnd)){return Err(fail("placement"));}}
    if patch.continuation_notice.as_ref().is_some_and(|text|text.chars().count()>120||text.chars().any(char::is_control)){return Err(fail("continuationNotice"));}
    if policies.title.as_ref().is_some_and(|text|text.chars().count()>120||text.chars().any(char::is_control)){return Err(fail("title"));}
    let mut flat=patch.to_value().as_object().unwrap().clone();
    if let Some(spacing)=patch.line_spacing{
        let LineSpacing::Multiple(multiple)=spacing else{return Err(fail("lineSpacing"))};
        if !(1.0..=3.0).contains(&multiple){return Err(fail("lineSpacing"));}flat.insert("lineHeight".into(),json!(multiple));
    }
    for key in ["sizeCs","lineSpacing","numberPattern","placement"]{flat.remove(key);}
    flat.insert("position".into(),json!(if kind.is_end(){"document"}else{"page"}));
    if let Some(title)=&policies.title{flat.insert("title".into(),json!(title));}
    let mut result=options.clone();result.as_object_mut().unwrap().insert("footnotes".into(),Value::Object(flat));Ok(result)
}

#[cfg(test)]
#[path = "publication_note_policies_tests.rs"]
mod tests;
