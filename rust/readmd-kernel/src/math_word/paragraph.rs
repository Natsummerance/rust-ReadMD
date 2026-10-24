//! Pure associated Word paragraph declarations and effective-value resolver.
//! Output contexts are internal; author XmlNode/MathDocument/source stay intact.
use super::{attr, word, Result, WordMathStyles};
use crate::math_document::{XmlNode, XmlPart, WORD_NS};
#[cfg(test)]
use crate::math_document::MATH_NS;
use std::collections::{BTreeMap, BTreeSet};
#[path = "paragraph_measure.rs"]
mod measure;
use measure::RationalPoints;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WordParagraphDeclaration<T> { Missing, Empty, Explicit(T) }
impl<T> Default for WordParagraphDeclaration<T> {
    fn default() -> Self { Self::Missing }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WordParagraphOrigin { DocDefaults, Style(String), Direct }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordParagraphValue<T> {
    pub value: T,
    pub origin: WordParagraphOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordParagraphJustification {
    Left, Start, Center, Right, End, Both, MediumKashida, Distribute,
    NumTab, HighKashida, LowKashida, ThaiDistribute,
}
impl WordParagraphJustification {
    pub(crate) fn as_str(self)->&'static str {
        match self {
            Self::Left=>"left",Self::Start=>"start",Self::Center=>"center",Self::Right=>"right",Self::End=>"end",Self::Both=>"both",
            Self::MediumKashida=>"mediumKashida",Self::Distribute=>"distribute",Self::NumTab=>"numTab",Self::HighKashida=>"highKashida",
            Self::LowKashida=>"lowKashida",Self::ThaiDistribute=>"thaiDistribute",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "left"=>Self::Left,"start"=>Self::Start,"center"=>Self::Center,
            "right"=>Self::Right,"end"=>Self::End,"both"=>Self::Both,
            "mediumKashida"=>Self::MediumKashida,"distribute"=>Self::Distribute,
            "numTab"=>Self::NumTab,"highKashida"=>Self::HighKashida,
            "lowKashida"=>Self::LowKashida,"thaiDistribute"=>Self::ThaiDistribute,
            _=>return Err("math_invalid_property"),
        })
    }
    fn finite(self) -> bool { matches!(self,Self::Left|Self::Center|Self::Right) }
}

/// Decimal strings deliberately do not overflow an i64 during preservation.
/// Modern Word's unitless signed measure is not math's bounded u16. The finite
/// consumer does a separate range check without rewriting a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WordParagraphMeasure {
    Integer(String), UnitLiteral(String), CharacterUnits {raw:String,value:i32},
}
impl WordParagraphMeasure {
    pub(crate) fn raw(&self)->&str {match self {Self::Integer(raw)|Self::UnitLiteral(raw)|Self::CharacterUnits{raw,..}=>raw}}
    fn finite_points(&self)->std::result::Result<RationalPoints,&'static str> {
        match self {
            Self::Integer(raw)=>RationalPoints::twips(raw),
            Self::UnitLiteral(raw)=>RationalPoints::unit(raw),
            Self::CharacterUnits{..}=>Err("unsupported-indent-domain-or-axis"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WordParagraphProperties {
    pub alignment: WordParagraphDeclaration<WordParagraphJustification>,
    pub indent: WordParagraphDeclaration<BTreeMap<String,WordParagraphMeasure>>,
    pub guards: BTreeMap<String,WordParagraphDeclaration<bool>>,
    pub text_direction: WordParagraphDeclaration<String>,
    // This subset cannot prove these properties' larger cascades inactive.
    // Keep their origin/name; never silently report full paragraph support.
    pub unresolved: BTreeMap<String,String>,
    pub paragraph_mark_properties: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordParagraphLayer {
    pub origin: WordParagraphOrigin,
    pub declaration: WordParagraphDeclaration<WordParagraphProperties>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WordParagraphEffective {
    pub alignment: Option<WordParagraphValue<WordParagraphJustification>>,
    pub indent: BTreeMap<String,WordParagraphValue<WordParagraphMeasure>>,
    pub guards: BTreeMap<String,WordParagraphValue<bool>>,
    pub text_direction: Option<WordParagraphValue<String>>,
    pub unresolved: Vec<WordParagraphUnsupported>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WordParagraphContext {
    pub layers: Vec<WordParagraphLayer>,
    pub effective: WordParagraphEffective,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordParagraphUnsupported {
    pub field: String,
    pub origin: WordParagraphOrigin,
    pub reason: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WordFiniteParagraph {
    // None means undeclared, or local math jc takes priority. It is never an
    // invented center or explicit zero and cannot replace sparse source data.
    // A consumed horizontal-LTR start/end is physical only in this output
    // copy; effective author values and their declaration layers stay native.
    pub alignment: Option<WordParagraphValue<WordParagraphJustification>>,
    pub left: Option<WordParagraphValue<RationalPoints>>,
    pub right: Option<WordParagraphValue<RationalPoints>>,
}
impl WordFiniteParagraph {
    pub(crate) fn inner_width(&self,available_points:f64)->Result<f64> {
        let left=self.left.as_ref().map_or(0.,|v|v.value.points());
        let right=self.right.as_ref().map_or(0.,|v|v.value.points());
        let width=available_points-left-right;
        if !available_points.is_finite()||available_points<=0.||!width.is_finite()||width<=0. {
            return Err("math_unsupported_extension");
        }
        Ok(width)
    }
}

fn paragraph_attributes<'a>(node:&'a XmlNode,allowed:&[&str])->Result<BTreeMap<&'a str,&'a str>> {
    let mut out=BTreeMap::new();
    for (key,value) in &node.attributes {
        if key=="xmlns"||key.starts_with("xmlns:"){continue;}
        let local=key.rsplit(':').next().unwrap_or(key);
        if !node.attribute_namespaces.get(key).is_some_and(|uri|uri==WORD_NS)
            || !allowed.contains(&local)||out.insert(local,value.as_str()).is_some(){
            return Err("math_invalid_property");
        }
    }
    Ok(out)
}
fn paragraph_single_child<'a>(node:&'a XmlNode,local:&str)->Result<Option<&'a XmlNode>> {
    let mut selected=None;
    for child in node.children().filter(|child|child.local()==local) {
        if !word(child,local)||selected.replace(child).is_some(){return Err("math_invalid_property");}
    }
    Ok(selected)
}
fn paragraph_no_content(node:&XmlNode)->Result<()> {
    if node.content.iter().any(|part|match part {
        XmlPart::Element(_)=>true,XmlPart::Text(text)=>!text.trim().is_empty(),_=>false,
    }) {return Err("math_invalid_property");} Ok(())
}
fn paragraph_required_value(node:&XmlNode)->Result<&str> {
    paragraph_no_content(node)?;
    paragraph_attributes(node,&["val"])?.get("val").copied()
        .filter(|value|!value.is_empty()).ok_or("math_invalid_property")
}
fn paragraph_boolean(node:&XmlNode)->Result<WordParagraphDeclaration<bool>> {
    paragraph_no_content(node)?;
    Ok(match paragraph_attributes(node,&["val"])?.get("val").copied() {
        None=>WordParagraphDeclaration::Empty,
        Some("0"|"false"|"off")=>WordParagraphDeclaration::Explicit(false),
        Some("1"|"true"|"on")=>WordParagraphDeclaration::Explicit(true),
        _=>return Err("math_invalid_property"),
    })
}
fn paragraph_decimal(value:&str,signed:bool)->bool {
    let value=value.trim_matches([' ','\t','\r','\n']);
    let digits=value.strip_prefix(['+','-']).unwrap_or(value);
    !digits.is_empty()&&digits.bytes().all(|c|c.is_ascii_digit())
        &&(signed||!value.starts_with('-')||digits.bytes().all(|c|c==b'0'))
}
fn paragraph_measure(value:&str,signed:bool,characters:bool)->Result<WordParagraphMeasure> {
    if characters {
        if !paragraph_decimal(value,true){return Err("math_invalid_property");}
        let numeric=value.trim_matches([' ','\t','\r','\n']).parse::<i32>().map_err(|_|"math_invalid_property")?;
        return Ok(WordParagraphMeasure::CharacterUnits{raw:value.into(),value:numeric});
    }
    if paragraph_decimal(value,signed){return Ok(WordParagraphMeasure::Integer(value.into()));}
    let Some(number)=["mm","cm","in","pt","pc","pi"].iter().find_map(|unit|value.strip_suffix(*unit)) else{return Err("math_invalid_property");};
    let number=if signed{number.strip_prefix('-').unwrap_or(number)}else{number};
    let mut parts=number.split('.');
    let whole=parts.next().unwrap_or("");let fraction=parts.next();
    if whole.is_empty()||!whole.bytes().all(|c|c.is_ascii_digit())||parts.next().is_some()
        ||fraction.is_some_and(|part|part.is_empty()||!part.bytes().all(|c|c.is_ascii_digit())) {
        return Err("math_invalid_property");
    }
    Ok(WordParagraphMeasure::UnitLiteral(value.into()))
}

#[derive(Clone,Copy)]
enum WordParagraphContainer { Direct, Style, Defaults }
const WORD_PARAGRAPH_ORDER:&[&str]=&[
    "pStyle","keepNext","keepLines","pageBreakBefore","framePr","widowControl","numPr",
    "suppressLineNumbers","pBdr","shd","tabs","suppressAutoHyphens","kinsoku","wordWrap",
    "overflowPunct","topLinePunct","autoSpaceDE","autoSpaceDN","bidi","adjustRightInd",
    "snapToGrid","spacing","ind","contextualSpacing","mirrorIndents","suppressOverlap",
    "jc","textDirection","textAlignment","textboxTightWrap","outlineLvl","divId","cnfStyle",
    "rPr","sectPr","pPrChange",
];
fn paragraph_properties(node:Option<&XmlNode>,container:WordParagraphContainer)->Result<WordParagraphDeclaration<WordParagraphProperties>> {
    use WordParagraphDeclaration as D;
    let Some(node)=node else{return Ok(D::Missing);};
    if !word(node,"pPr"){return Err("math_invalid_property");}
    paragraph_attributes(node,&[])?;
    if node.content.iter().any(|part|matches!(part,XmlPart::Text(text)if !text.trim().is_empty())){return Err("math_invalid_property");}
    let mut out=WordParagraphProperties::default();let mut seen=BTreeSet::new();let mut previous=None;
    for property in node.children() {
        let local=property.local();
        let index=WORD_PARAGRAPH_ORDER.iter().position(|value|*value==local).ok_or("math_invalid_property")?;
        if property.namespace!=WORD_NS||!seen.insert(local)||previous.is_some_and(|last|index<=last){return Err("math_invalid_property");}
        previous=Some(index);
        if !matches!(container,WordParagraphContainer::Direct)
            &&matches!(local,"pStyle"|"divId"|"cnfStyle"|"rPr"|"sectPr"){return Err("math_invalid_property");}
        if matches!(container,WordParagraphContainer::Defaults)&&local=="pPrChange"{return Err("math_invalid_property");}
        match local {
            "pStyle"=>{paragraph_required_value(property)?;},
            "jc"=>out.alignment=D::Explicit(WordParagraphJustification::parse(paragraph_required_value(property)?)?),
            "ind"=>{
                paragraph_no_content(property)?;
                let attrs=paragraph_attributes(property,&["left","right","start","end","leftChars","rightChars","startChars","endChars","hanging","hangingChars","firstLine","firstLineChars"])?;
                let mut indent=BTreeMap::new();
                for (key,value) in attrs {
                    let characters=key.ends_with("Chars");
                    indent.insert(key.into(),paragraph_measure(value,matches!(key,"left"|"right"|"start"|"end")||characters,characters)?);
                }
                out.indent=if indent.is_empty(){D::Empty}else{D::Explicit(indent)};
            },
            "bidi"|"mirrorIndents"|"adjustRightInd"|"snapToGrid"=>{out.guards.insert(local.into(),paragraph_boolean(property)?);},
            "textDirection"=>{
                let value=paragraph_required_value(property)?;
                if !matches!(value,"lrTb"|"tb"|"tbRl"|"rl"|"btLr"|"lr"|"lrTbV"|"tbV"|"tbRlV"|"rlV"|"tbLrV"|"lrV"){return Err("math_invalid_property");}
                out.text_direction=D::Explicit(value.into());
            },
            // The paragraph mark does not set paragraph text or geometry.
            "rPr"=>out.paragraph_mark_properties=true,
            // The lossless source already owns this subtree. Store a bounded
            // QName diagnostic, not another opaque XML payload in the context.
            _=>{out.unresolved.insert(local.into(),format!("{{{}}}{local}",property.namespace));},
        }
    }
    Ok(if seen.is_empty(){D::Empty}else{D::Explicit(out)})
}

impl WordParagraphContext {
    /// Host flow/direction is not among dispDef's five math overrides.
    pub(crate) fn host_display_supported(&self)->bool {
        self.effective.unresolved.is_empty()
            &&self.effective.guards.values().all(|value|!value.value)
            &&self.effective.text_direction.as_ref().is_none_or(|value|matches!(value.value.as_str(),"lrTb"|"tb"))
    }
    fn push(&mut self,origin:WordParagraphOrigin,declaration:WordParagraphDeclaration<WordParagraphProperties>) {
        use WordParagraphDeclaration as D;
        if let D::Explicit(properties)=&declaration {
            if let D::Explicit(value)=properties.alignment{self.effective.alignment=Some(WordParagraphValue{value,origin:origin.clone()});}
            if let D::Explicit(indent)=&properties.indent {
                for (key,value) in indent{self.effective.indent.insert(key.clone(),WordParagraphValue{value:value.clone(),origin:origin.clone()});}
            }
            for (key,declared) in &properties.guards {
                let value=match declared{D::Missing=>continue,D::Empty=>true,D::Explicit(value)=>*value};
                self.effective.guards.insert(key.clone(),WordParagraphValue{value,origin:origin.clone()});
            }
            if let D::Explicit(value)=&properties.text_direction{self.effective.text_direction=Some(WordParagraphValue{value:value.clone(),origin:origin.clone()});}
            for (field,value) in &properties.unresolved {
                self.effective.unresolved.push(WordParagraphUnsupported{field:format!("word.pPr.{field}"),origin:origin.clone(),reason:"unimplemented-property-cascade",value:value.clone()});
            }
        }
        self.layers.push(WordParagraphLayer{origin,declaration});
    }
    /// Only call for the finite dispDef=false display lane. Local math jc can
    /// shadow Word alignment; it does not shadow Word ind/direction/guards.
    pub(crate) fn finite_display(&self,local_math_alignment:bool)->std::result::Result<WordFiniteParagraph,Vec<WordParagraphUnsupported>> {
        let mut issues=self.effective.unresolved.clone();let mut out=WordFiniteParagraph::default();
        let logical_alignment=!local_math_alignment&&self.effective.alignment.as_ref()
            .is_some_and(|value|matches!(value.value,WordParagraphJustification::Start|WordParagraphJustification::End));
        if !local_math_alignment {
            if let Some(value)=&self.effective.alignment {
                let physical=match value.value {
                    WordParagraphJustification::Start=>WordParagraphJustification::Left,
                    WordParagraphJustification::End=>WordParagraphJustification::Right,
                    other=>other,
                };
                if physical.finite(){out.alignment=Some(WordParagraphValue{value:physical,origin:value.origin.clone()});}else{
                    issues.push(WordParagraphUnsupported{field:"word.pPr.jc".into(),origin:value.origin.clone(),reason:"unsupported-justification",value:value.value.as_str().into()});
                }
            }
        }
        for (key,value) in &self.effective.indent {
            let reason=if matches!(key.as_str(),"left"|"right") {
                match value.value.finite_points() {
                    Ok(points)=>{
                        let target=if key=="left"{&mut out.left}else{&mut out.right};
                        *target=Some(WordParagraphValue{value:points,origin:value.origin.clone()});continue;
                    },
                    Err(reason)=>reason,
                }
            }else{"unsupported-indent-domain-or-axis"};
            issues.push(WordParagraphUnsupported{field:format!("word.pPr.ind.{key}"),origin:value.origin.clone(),reason,value:value.value.raw().into()});
        }
        for (key,value) in &self.effective.guards {
            if value.value{issues.push(WordParagraphUnsupported{field:format!("word.pPr.{key}"),origin:value.origin.clone(),reason:"unsupported-effective-paragraph-guard",value:"true".into()});}
        }
        if let Some(value)=&self.effective.text_direction {
            // Existing physical L/C/R keeps its historical tb alias. That
            // alias is not a proof of LTR for a consumed logical start/end.
            // A local math jc shadows Word jc, not the existing host guards.
            if !matches!(value.value.as_str(),"lrTb"|"tb")||(logical_alignment&&value.value=="tb"){issues.push(WordParagraphUnsupported{field:"word.pPr.textDirection".into(),origin:value.origin.clone(),reason:"unsupported-text-direction",value:value.value.clone()});}
        }
        if issues.is_empty(){Ok(out)}else{Err(issues)}
    }
}

/// Pure resolver. No mutation, rPr materialization, font/resource access,
/// private OMML attributes, serialization, I/O or public parse-signature change.
/// Call on a marker-free lossless tree, not compiler temporary dslFrom/dslTo.
pub(crate) fn word_paragraph_context(root:&XmlNode)->Result<WordParagraphContext> {
    let context=paragraph_single_child(root,"styles")?;
    let direct=paragraph_single_child(root,"pPr")?;
    let direct_properties=paragraph_properties(direct,WordParagraphContainer::Direct)?;
    let styles=context.map(WordMathStyles::from_node).transpose()?.unwrap_or_default();
    let mut out=WordParagraphContext::default();
    let defaults=if let Some(root)=context {
        let defaults=paragraph_single_child(root,"docDefaults")?;
        if let Some(defaults)=defaults {
            paragraph_attributes(defaults,&[])?;
            let mut previous=None;let mut seen=BTreeSet::new();
            for entry in defaults.children() {
                let index=match entry.local(){"rPrDefault"=>0,"pPrDefault"=>1,_=>return Err("math_invalid_property")};
                if entry.namespace!=WORD_NS||!seen.insert(index)||previous.is_some_and(|last|index<=last){return Err("math_invalid_property");}
                previous=Some(index);
            }
            if defaults.content.iter().any(|part|matches!(part,XmlPart::Text(text)if !text.trim().is_empty())){return Err("math_invalid_property");}
            let default_pr=paragraph_single_child(defaults,"pPrDefault")?;
            if let Some(default_pr)=default_pr{
                paragraph_attributes(default_pr,&[])?;
                if default_pr.children().any(|child|!word(child,"pPr"))
                    ||default_pr.content.iter().any(|part|matches!(part,XmlPart::Text(text)if !text.trim().is_empty())){return Err("math_invalid_property");}
                paragraph_single_child(default_pr,"pPr")?
            }else{None}
        }else{None}
    }else{None};
    out.push(WordParagraphOrigin::DocDefaults,paragraph_properties(defaults,WordParagraphContainer::Defaults)?);
    // Validate pStyle before paragraph_id's legacy missing-value fallback.
    if let Some(node)=direct {
        if let Some(reference)=paragraph_single_child(node,"pStyle")? {paragraph_required_value(reference)?;}
    }
    if let Some(id)=styles.paragraph_id(direct) {
        for style in styles.chain(id,"paragraph")? {
            let style_id=attr(style,"styleId").ok_or("math_invalid_property")?;
            if style_id.is_empty()||style_id.chars().count()>253{return Err("math_invalid_property");}
            if let Some(reference)=paragraph_single_child(style,"basedOn")? {
                if paragraph_required_value(reference)?.chars().count()>253{return Err("math_invalid_property");}
            }
            let properties=paragraph_single_child(style,"pPr")?;
            out.push(WordParagraphOrigin::Style(style_id.into()),paragraph_properties(properties,WordParagraphContainer::Style)?);
        }
    }
    out.push(WordParagraphOrigin::Direct,direct_properties);
    Ok(out)
}

#[cfg(test)]
#[path = "paragraph_tests.rs"]
mod tests;
