//! Tiny OMML (Office Math Markup) tree reader plus a MathML writer.
//!
//! `latex2omml::latex_to_omml` is the kernel's single LaTeX math parser; DOCX
//! embeds its output directly.  This module parses that output back into a
//! tree so the other exporters agree with DOCX on every formula: EPUB gets
//! MathML ([`to_mathml`]) and the PDF renderer lays the tree out as vector
//! boxes (`math_layout`).
//!
//! Parsing reuses the bounded lossless XML reader from `math_document`. The
//! rendering view resolves namespace aliases, preserves character data and
//! distinguishes mathematical properties from embedded Word run properties.
//! Rendering is bounded by [`MAX_DEPTH`]; imports also obey the shared budgets.

/// Deepest element nesting accepted; anything deeper is reported as an error
/// so callers can fall back to plain text instead of recursing without bound.
pub const MAX_DEPTH: usize = 200;

/// Document settings are inherited by the rendering view, never written back
/// into the lossless source tree. None keeps the ordinary style-dependent rule.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MathDefaults {
    pub integral_limits: Option<bool>,
    pub nary_limits: Option<bool>,
    pub small_fraction: Option<bool>,
}

/// Shared Office script/display state for vector and MathML projections.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MathStyle { pub display: bool, pub level: u8, pub cramped: bool }
impl MathStyle {
    pub fn sup(self) -> Self { Self { display:false, level:(self.level+1).min(2), ..self } }
    pub fn sub(self) -> Self { Self { cramped:true, ..self.sup() } }
    pub fn frac_part(self, den:bool, small:bool) -> Self {
        Self { display:false, level:if self.display&&!small{self.level}else{(self.level+1).min(2)}, cramped:den||self.cramped }
    }
    pub fn argument(self, n:&Node) -> Self { Self { level:(self.level as i8-argument_size(n)).clamp(0,2) as u8, ..self } }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MathMlFormat { pub points:f64, pub color:Option<[u8;3]> }

/// Group characters have two independent axes: placement and the part that
/// owns the baseline. Office defaults differ for an absent/empty vertJc.
pub(crate) fn group_character(n: &Node) -> (bool, bool, &str) {
    let bottom = n.prop("groupChrPr", "pos").unwrap_or("bot") != "top";
    let baseline_bottom = n.child("groupChrPr").and_then(|p| p.child("vertJc"))
        .is_some_and(|p| p.attr("val").unwrap_or("bot") == "bot");
    (bottom, bottom == baseline_bottom, n.character("groupChrPr", "chr", "⏟"))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    /// Local name without the namespace prefix (`f`, `sSup`, `r`, `t`, …).
    pub name: String,
    /// Original prefix is diagnostic only; semantics use the resolved URI.
    pub prefix: String,
    pub namespace: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// Character data directly inside this element (only `m:t` has any).
    pub text: String,
    pub math_defaults: MathDefaults,
    /// Temporary output context, never serialized into author OMML/DSL.
    pub mathml_format: Option<MathMlFormat>,
    /// Effective script level for explicit semantic spacing, output-only.
    pub mathml_script_level: u8,
    /// Source-preserving equation-array annotations, never author XML fields.
    pub(crate) equation_markers: Vec<crate::math_equation_array::RunMarker>,
    pub(crate) equation_word: Option<bool>,
    pub(crate) equation_context: Option<SpacingContext>,
    pub(crate) equation_glue: Option<(Option<Class>, Option<Class>)>,
    pub(crate) equation_source: Option<(String, usize)>,
    /// Many fragments share one immutable original run; do not clone a long
    /// text/property subtree for every alignment marker.
    pub(crate) equation_origin: Option<std::sync::Arc<Node>>,
    /// Associated Word paragraph cascade. Output-only, never author XML/DSL.
    pub(crate) word_paragraph: Option<std::sync::Arc<crate::math_word::WordParagraphContext>>,
}

impl Node {
    pub fn is_math(&self) -> bool {
        self.namespace.is_empty() || self.namespace == crate::math_document::MATH_NS
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name && c.is_math())
    }

    /// `m:val` of the named property child (`<m:chr m:val="∑"/>`).
    pub fn prop(&self, pr: &str, key: &str) -> Option<&str> {
        self.child(pr)?.child(key)?.attr("val")
    }

    /// Character properties distinguish an omitted element (use its Office
    /// default) from an attribute-free/empty element (no character).
    pub fn character<'a>(&'a self, pr: &str, key: &str, default: &'a str) -> &'a str {
        self.child(pr).and_then(|p| p.child(key))
            .map(|p| p.attr("val").unwrap_or(""))
            .unwrap_or(default)
    }

    pub fn enabled(&self, pr: &str, key: &str, default: bool) -> bool {
        self.child(pr).and_then(|p| p.child(key)).map(|p| on_off(p.attr("val"), true)).unwrap_or(default)
    }

    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Is this a property container (`fPr`, `rPr`, `naryPr`, …)?
    pub fn is_props(&self) -> bool {
        self.name.ends_with("Pr")
    }

    /// Concatenated `m:t` text of a run.
    pub fn run_text(&self) -> String {
        let mut s = String::new();
        for c in &self.children {
            if c.name == "t" && c.is_math() {
                s.push_str(&c.text);
            }
        }
        s
    }

    pub(crate) fn equation_visible_text(&self) -> String {
        let text = self.run_text();
        if self.equation_markers.is_empty() { return text; }
        let mut offset = 0;
        let mut markers = self.equation_markers.iter().peekable();
        text.chars().filter(|c| {
            while markers.peek().is_some_and(|m|m.offset<offset) {markers.next();}
            let mut consume=false;
            while markers.peek().is_some_and(|m|m.offset==offset) {consume|=markers.next().unwrap().consumes_character;}
            offset += c.len_utf16(); !consume
        }).collect()
    }

    /// Nesting depth of the subtree (a leaf is 1).
    pub fn depth(&self) -> usize {
        let mut max = 0usize;
        let mut stack: Vec<(&Node, usize)> = vec![(self, 1)];
        while let Some((n, d)) = stack.pop() {
            max = max.max(d);
            for c in &n.children {
                stack.push((c, d + 1));
            }
        }
        max
    }
}

pub fn on_off(value: Option<&str>, default: bool) -> bool {
    match value { Some("0" | "false" | "off") => false, Some("1" | "true" | "on") => true, _ => default }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MathJustification { Left, Right, Center, #[default] CenterGroup }
impl MathJustification {
    pub fn parse(value: &str) -> Self { match value { "left"=>Self::Left, "right"=>Self::Right, "center"=>Self::Center, _=>Self::CenterGroup } }
    pub fn as_str(self) -> &'static str { match self {Self::Left=>"left",Self::Right=>"right",Self::Center=>"center",Self::CenterGroup=>"centerGroup"} }
    pub fn column(self) -> &'static str { match self {Self::Left|Self::CenterGroup=>"left",Self::Right=>"right",Self::Center=>"center"} }
    pub fn offset(self, available: f64, content: f64) -> f64 {
        match self {Self::Left=>0.0,Self::Right=>(available-content).max(0.0),_=>((available-content)/2.0).max(0.0)}
    }
}

/// Effective display paragraph settings, in points. The lossless XML retains
/// omitted/default/off states; this projection never rewrites the source.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MathParagraphStyle {
    pub alignment: MathJustification,
    pub explicit_alignment: bool,
    pub before: f64,
    pub after: f64,
    pub inter: f64,
    pub intra: f64,
    pub left: f64,
    pub right: f64,
    pub explicit_margins: bool,
    /// A Word paragraph frame does not use Office Math's margin fallback.
    pub(crate) word_margins: bool,
}
impl MathParagraphStyle {
    /// Office ignores the left margin when both exceed the available width;
    /// an oversized right margin falls back to one inch (1440 twips).
    pub fn margins(self,width:Option<f64>)->(f64,f64){
        let(mut left,mut right)=(self.left,self.right);
        if self.word_margins { return (left,right); }
        if let Some(width)=width{if right>width{right=72.0;}if left+right>width{left=0.0;}}
        (left,right)
    }
}
pub fn math_paragraph_style(root: &Node, paragraph: Option<&Node>) -> MathParagraphStyle {
    let pr=root.child("mathPr");
    let enabled=pr.and_then(|n|n.child("dispDef")).map(|n|on_off(n.attr("val"),true)).unwrap_or(true);
    // dispDef controls the five paragraph-override settings, not the standard
    // pre/post/inter/intra spacing properties (MS-OE376 2.1.1820).
    let points=|key| pr.and_then(|n|n.child(key)).and_then(|n|n.attr("val"))
        .and_then(crate::math_defaults::bounded_twips).map_or(0.0,|v|f64::from(v)/20.0);
    let local=paragraph.and_then(|n|n.child("oMathParaPr")).and_then(|n|n.child("jc"));
    let alignment=local.or_else(||if enabled {pr.and_then(|n|n.child("defJc"))}else{None});
    let margin=|key|if enabled{pr.and_then(|n|n.child(key)).map_or(0.0,|n|n.attr("val").and_then(crate::math_defaults::bounded_twips).map_or(72.0,|v|f64::from(v)/20.0))}else{0.0};
    let mut style=MathParagraphStyle {alignment:alignment.map(|n|MathJustification::parse(n.attr("val").unwrap_or("centerGroup"))).unwrap_or_default(),
        explicit_alignment:alignment.is_some(),before:points("preSp"),after:points("postSp"),inter:points("interSp"),intra:points("intraSp"),
        left:margin("lMargin"),right:margin("rMargin"),explicit_margins:enabled&&pr.is_some_and(|n|n.child("lMargin").is_some()||n.child("rMargin").is_some()),word_margins:false};
    // This is a projection, not a capability probe. Product consumers call
    // validate_display_context before formatting or opening any resources.
    if let Ok(Some(word))=word_display_paragraph(root,paragraph) {
        if let Some(value)=word.alignment {style.alignment=word_justification(value.value);style.explicit_alignment=true;}
        style.left=word.left.as_ref().map_or(0.,|value|value.value.points());
        style.right=word.right.as_ref().map_or(0.,|value|value.value.points());
        // An explicit Word jc also needs the real constrained paragraph box,
        // even when neither physical indent axis was declared. The zero
        // offsets belong only to this output frame; native/source axes stay
        // Missing. An authored local math jc may shadow Word alignment, but
        // it must still be positioned inside that same paragraph frame.
        style.word_margins=word.left.is_some()||word.right.is_some()
            ||root.word_paragraph.as_ref().is_some_and(|context|context.effective.alignment.is_some());
        style.explicit_margins=style.word_margins;
    }
    style
}

fn word_justification(value:crate::math_word::WordParagraphJustification)->MathJustification {
    use crate::math_word::WordParagraphJustification as W;
    match value {W::Left=>MathJustification::Left,W::Right=>MathJustification::Right,W::Center=>MathJustification::Center,_=>unreachable!("validated finite Word justification")}
}

pub(crate) fn word_display_paragraph(root:&Node,paragraph:Option<&Node>)
    ->Result<Option<crate::math_word::WordFiniteParagraph>,&'static str>
{
    let enabled=root.child("mathPr").and_then(|pr|pr.child("dispDef"))
        .map(|node|on_off(node.attr("val"),true)).unwrap_or(true);
    let Some(context)=root.word_paragraph.as_ref() else{return Ok(None);};
    if !context.host_display_supported(){return Err("math_unsupported_extension");}
    if enabled {return Ok(None);}
    let local=paragraph.and_then(|node|node.child("oMathParaPr")).and_then(|pr|pr.child("jc")).is_some();
    context.finite_display(local).map(Some).map_err(|_|"math_unsupported_extension")
}

/// Pure capability/geometry preflight. Parsing and saving may retain valid
/// unsupported Word properties; no renderer is allowed to silently ignore them.
pub(crate) fn validate_display_context(root:&Node,display:bool,width:Option<f64>)->Result<(),&'static str>{
    if !display {
        fn has_paragraph(node:&Node)->bool {
            (node.is_math()&&node.name=="oMathPara")||node.children.iter().any(has_paragraph)
        }
        // An inline carrier cannot erase a display paragraph by rendering it
        // as a row. This is the existing DOCX carrier rule for every consumer.
        return if has_paragraph(root){Err("math_invalid_source")}else{Ok(())};
    }
    let check=|paragraph:Option<&Node>|->Result<(),&'static str>{
        if let Some(word)=word_display_paragraph(root,paragraph)? {
            if let Some(width)=width {word.inner_width(width).map_err(|_|"math_unsupported_extension")?;}
        }
        let disabled=root.child("mathPr").and_then(|pr|pr.child("dispDef")).is_some_and(|node|!on_off(node.attr("val"),true));
        let local=paragraph.and_then(|node|node.child("oMathParaPr")).and_then(|pr|pr.child("jc")).is_some();
        let word_alignment=root.word_paragraph.as_ref().is_some_and(|context|context.effective.alignment.is_some());
        let count=paragraph.unwrap_or(root).children.iter().filter(|node|node.is_math()&&node.name=="oMath").count();
        // The legacy centerGroup basis and Word's unaligned paragraph are not
        // interchangeable for unequal multiple equations. Require an authored
        // alignment until native round-trip semantics have been established.
        if disabled&&!local&&!word_alignment&&count>1{return Err("math_unsupported_extension");}
        Ok(())
    };
    for paragraph in display_scopes(root){check(paragraph)?;}
    Ok(())
}

/// Explicit paragraphs and adjacent bare display fragments share the same
/// associated Word context, but only actual oMathPara has a local math jc.
pub(crate) fn display_scopes(root:&Node)->Vec<Option<&Node>>{
    let mut scopes=root.children.iter().filter(|node|node.is_math()&&node.name=="oMathPara").map(Some).collect::<Vec<_>>();
    if scopes.is_empty()||root.children.iter().any(|node|node.is_math()&&!node.is_props()&&node.name!="oMathPara"){scopes.push(None);}
    scopes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MathBinaryBreak { #[default] Before, After, Repeat }

/// Operator policy is document-wide. dispDef only disables the display
/// paragraph's indent/alignment, not brkBin or brkBinSub.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MathWrapStyle {
    pub binary: MathBinaryBreak,
    pub subtraction: &'static str,
    pub indent: f64,
    pub right: bool,
}
pub fn math_wrap_style(root: &Node) -> MathWrapStyle {
    let pr=root.child("mathPr");
    let value=|key|pr.and_then(|n|n.child(key)).and_then(|n|n.attr("val"));
    let enabled=pr.and_then(|n|n.child("dispDef")).map(|n|on_off(n.attr("val"),true)).unwrap_or(true);
    MathWrapStyle {
        binary:match value("brkBin"){Some("after")=>MathBinaryBreak::After,Some("repeat")=>MathBinaryBreak::Repeat,_=>MathBinaryBreak::Before},
        subtraction:match value("brkBinSub"){Some("-+")=>"-+",Some("+-")=>"+-",_=>"--"},
        indent:if enabled{value("wrapIndent").and_then(crate::math_defaults::bounded_twips).map_or(72.0,|v|f64::from(v)/20.0)}else{0.0},
        right:enabled&&pr.and_then(|n|n.child("wrapRight")).is_some_and(|n|on_off(n.attr("val"),true)),
    }
}

/// A manual break belongs to a run or operator box, and aligns to a 1-based
/// operator index on the previous line. No index means that line's beginning.
pub fn manual_break(n: &Node) -> Option<usize> {
    if !n.is_math(){return None;}
    let pr=match n.name.as_str(){"r"=>"rPr","box"=>"boxPr",_=>return None};
    n.child(pr)?.child("brk").map(|n|n.attr("alnAt").and_then(|v|v.parse::<u8>().ok()).map_or(0,usize::from))
}

pub fn manual_math_lines(nodes: &[Node]) -> Vec<(&[Node], Option<usize>)> {
    let mut result=Vec::new();let mut start=0;let mut alignment=None;
    for (index,node) in nodes.iter().enumerate(){if let Some(next)=manual_break(node){
        if nodes[start..index].iter().any(|n|!n.is_props()) {result.push((&nodes[start..index],alignment));start=index;}
        alignment=Some(next);
    }}
    if start<nodes.len(){result.push((&nodes[start..],alignment));}result
}

/// Repeated matrix column groups are shared by MathML and vector PDF layout.
/// Missing groups/values use Office Math's centered-column default.
pub fn matrix_column_alignments(n:&Node,columns:usize)->Vec<&str>{
    let mut result=Vec::with_capacity(columns);
    if let Some(groups)=n.child("mPr").and_then(|p|p.child("mcs")){
        for group in groups.children.iter().filter(|n|n.name=="mc" && n.is_math()){
            let count=group.prop("mcPr","count").and_then(|v|v.parse::<usize>().ok()).unwrap_or(1).min(columns.saturating_sub(result.len()));
            let alignment=match group.prop("mcPr","mcJc"){Some("left")=>"left",Some("right")=>"right",_=>"center"};
            result.extend(std::iter::repeat_n(alignment,count));
        }
    }
    result.resize(columns,"center");result
}

/// Office Math distinguishes font-relative gaps from absolute point lengths.
/// cGpRule 3 stores twips; rule 4 stores half-em increments. The default is 1em.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MathLength {
    Em(f64),
    Points(f64),
}
impl MathLength {
    pub fn half(self) -> Self {
        match self { Self::Em(v) => Self::Em(v/2.0), Self::Points(v) => Self::Points(v/2.0) }
    }
    pub fn in_em(self, base_points:f64, local_scale:f64)->f64 {
        match self { Self::Em(value)=>value*local_scale, Self::Points(value)=>value/base_points }
    }
    pub fn mathml(self)->String {
        match self {Self::Em(value)=>format!("{value}em"),Self::Points(value)=>format!("{value}pt")}
    }
}
pub fn matrix_column_gap(n:&Node)->MathLength {
    let custom=||n.prop("mPr","cGp").and_then(|value|value.parse::<u16>().ok()).unwrap_or(0) as f64;
    match n.prop("mPr","cGpRule") {
        Some("1")=>MathLength::Em(1.5),
        Some("2")=>MathLength::Em(2.0),
        Some("3")=>MathLength::Points(custom()/20.0),
        Some("4")=>MathLength::Em(custom()/2.0),
        _=>MathLength::Em(1.0),
    }
}

/// cSp is a minimum content width, not a second gap. Unlike cGpRule 4 it
/// stays physical when the formula/control font size changes (twips = pt/20).
pub fn matrix_column_minimum(n: &Node) -> MathLength {
    MathLength::Points(n.prop("mPr", "cSp").and_then(|v| v.parse::<u16>().ok()).unwrap_or(0) as f64 / 20.0)
}

pub fn matrix_placeholders_visible(n: &Node) -> bool {
    !n.enabled("mPr", "plcHide", false)
}

/// A zero-width phantom or a structured expression is not an empty argument.
/// Preserve deliberate spaces too: this is semantic emptiness, not ink bounds.
pub fn empty_argument(n: &Node) -> bool {
    n.is_math() && matches!(n.name.as_str(), "e" | "r" | "t")
        && if n.name == "t" { n.text.is_empty() } else { n.text.trim().is_empty() }
        && n.children.iter().filter(|c| !c.is_props()).all(empty_argument)
}

/// Width, ascent, descent and inset stroke, in local em. The Office standard
/// leaves the shape unspecified; both projections use an unlabelled thin box.
pub(crate) const MATRIX_PLACEHOLDER: (f64, f64, f64, f64) = (0.45, 0.65, 0.05, 0.04);

/// Row gaps use points (not the column property's twips) for rule 3 and
/// half-lines for rule 4. Keep the units until the final layout size is known.
/// Both matrices and equation arrays use the same Office spacing properties.
pub fn table_row_gap(n: &Node) -> MathLength {
    let container = if n.name == "eqArr" { "eqArrPr" } else { "mPr" };
    let value = || n.prop(container, "rSp").and_then(|v| v.parse::<u16>().ok()).unwrap_or(0) as f64;
    match n.prop(container, "rSpRule") {
        Some("1") => MathLength::Em(1.5),
        Some("2") => MathLength::Em(2.0),
        Some("3") => MathLength::Points(value()),
        Some("4") => MathLength::Em(value() / 2.0),
        _ => MathLength::Em(1.0),
    }
}

pub fn table_base_justification(n: &Node) -> Option<&str> {
    n.prop(if n.name == "eqArr" { "eqArrPr" } else { "mPr" }, "baseJc")
}

/// Explicit limLoc is authoritative even in inline/script style. An omitted
/// property is movable: integral-family operators use side scripts, while
/// other operators use above/below limits only in display style. An explicit
/// document default takes precedence over that fallback, but not over limLoc.
pub fn nary_limits(n: &Node, display: bool) -> bool {
    if let Some(location) = n.child("naryPr").and_then(|p| p.child("limLoc")) {
        return location.attr("val").unwrap_or("undOvr") == "undOvr";
    }
    nary_document_limits(n).unwrap_or_else(||
        display && !is_integral_operator(n.character("naryPr", "chr", "∫")))
}

pub fn nary_document_limits(n: &Node) -> Option<bool> {
    if is_integral_operator(n.character("naryPr", "chr", "∫")) {
        n.math_defaults.integral_limits
    } else {
        n.math_defaults.nary_limits
    }
}

pub fn is_integral_operator(character: &str) -> bool {
    character.chars().next().is_some_and(|c|
        matches!(c, '\u{222b}'..='\u{2233}' | '\u{2a0b}'..='\u{2a1c}'))
}

/// Build a bounded rendering view from the shared lossless XML parser. Namespace
/// aliases resolve by URI, so Word properties never depend on the spelling w:.
/// Bare m:/w: fragments remain compatible with the existing internal adapters.
pub fn parse(xml: &str) -> Result<Node, String> {
    use crate::math_document::{XmlNode,XmlPart};
    fn convert(source:&XmlNode)->Node {
        let (prefix,name)=source.name.split_once(':').unwrap_or(("",&source.name));
        let attrs=source.attributes.iter().filter(|(key,_)|*key!="xmlns"&&!key.starts_with("xmlns:")&&source.attribute_namespaces.get(*key).is_none_or(|uri|uri.is_empty()||uri==&source.namespace||uri=="http://www.w3.org/XML/1998/namespace"))
            .map(|(key,value)|(key.rsplit(':').next().unwrap_or(key).into(),value.clone())).collect();
        let mut node=Node{name:name.into(),prefix:prefix.into(),namespace:source.namespace.clone(),attrs,..Default::default()};
        for part in &source.content {match part {
            XmlPart::Element(child)=>node.children.push(convert(child)),
            XmlPart::Text(text)=>node.text.push_str(text),_=>(),
        }}
        node
    }
    let mut source=crate::math_document::parse_xml(xml).map_err(str::to_string)?;
    crate::math_spacing::validate_tree(&source).map_err(str::to_string)?;
    let paragraph=crate::math_word::word_paragraph_context(&source).map_err(str::to_string)?;
    let associated=source.children().any(|node|node.namespace==crate::math_document::WORD_NS&&matches!(node.local(),"styles"|"pPr"));
    crate::math_word::materialize(&mut source).map_err(str::to_string)?;
    let mut root = convert(&source);
    if associated {root.word_paragraph=Some(std::sync::Arc::new(paragraph));}
    let defaults = root.child("mathPr").map(|pr| MathDefaults {
        integral_limits: pr.child("intLim").map(|p| p.attr("val").unwrap_or("subSup") == "undOvr"),
        nary_limits: pr.child("naryLim").map(|p| p.attr("val").unwrap_or("undOvr") == "undOvr"),
        small_fraction: pr.child("smallFrac").map(|p| on_off(p.attr("val"), true)),
    }).unwrap_or_default();
    fn inherit(node: &mut Node, defaults: MathDefaults) {
        node.math_defaults = defaults;
        for child in &mut node.children { inherit(child, defaults); }
    }
    inherit(&mut root, defaults);
    Ok(root)
}

/// Word has one mathPr in settings.xml. Keep the originating settings separate
/// from paragraph content and fix effective operator/paragraph placement
/// locally. The export-only projection never changes the author's DSL/XML.
pub(crate) fn docx_math_fragment(xml: &str, display: bool)
    -> Result<(String, Option<crate::math_document::XmlNode>), String>
{
    let source = crate::math_document::parse_xml(xml).map_err(str::to_string)?;
    docx_math_fragment_tree(source, display)
}

/// Reuse the merger's already bounded parse rather than parsing every full
/// formula again merely to obtain its declaration state and output copy.
pub(crate) fn docx_math_fragment_tree(source: crate::math_document::XmlNode, display: bool)
    -> Result<(String, Option<crate::math_document::XmlNode>), String>
{
    let projected=docx_math_projection_tree(source,display)?;
    Ok((projected.content,projected.settings))
}

#[derive(Debug,Clone,PartialEq,Default)]
pub(crate) struct DocxMathParagraph {
    pub alignment:Option<String>,
    pub left:Option<String>,
    pub right:Option<String>,
}
pub(crate) struct DocxMathProjection {
    pub content:String,
    pub settings:Option<crate::math_document::XmlNode>,
    pub paragraph:Option<DocxMathParagraph>,
    pub paragraph_source:Option<crate::math_word::NativeParagraphSource>,
}
pub(crate) fn native_word_paragraph(root:&Node)->Result<Option<DocxMathParagraph>,&'static str>{
    let Some(context)=root.word_paragraph.as_ref() else{return Ok(None);};
    // Word's stored paragraph axes remain meaningful when math defaults
    // temporarily override their paint. Never drop them merely because they
    // are inactive. Native jc supports every validated Word enumeration;
    // physical indents remain in this unit's finite, proven consumer domain.
    context.finite_display(true).map_err(|_|"math_unsupported_extension")?;
    Ok(Some(DocxMathParagraph {
        alignment:context.effective.alignment.as_ref().map(|value|value.value.as_str().to_string()),
        left:context.effective.indent.get("left").map(|value|value.value.raw().to_string()),
        right:context.effective.indent.get("right").map(|value|value.value.raw().to_string()),
    }))
}
pub(crate) fn docx_math_projection_tree(mut source:crate::math_document::XmlNode,display:bool)
    ->Result<DocxMathProjection,String>
{
    use crate::math_document::{XmlNode, XmlPart, MATH_NS};
    crate::math_spacing::native_office(&source).map_err(str::to_string)?;
    // Parse the context-bearing source before materialize removes its pPr and
    // style catalogue. The typed view, source and native pPr stay separate.
    let mut view=parse(&source.xml())?;
    if view.word_paragraph.is_some(){
        // Attribute serialization is not the source of native measure words:
        // literal tabs/LFs would normalize on the rendering view's reparse.
        // Keep the original, equally validated cascade for lexical export.
        view.word_paragraph=Some(std::sync::Arc::new(crate::math_word::word_paragraph_context(&source).map_err(str::to_string)?));
    }
    validate_display_context(&view,display,None).map_err(str::to_string)?;
    let paragraph=if display {native_word_paragraph(&view).map_err(str::to_string)?}else{None};
    let paragraph_source=if display {
        crate::math_word::capture_native_paragraph(&source).map_err(str::to_string)?
    }else{None};
    crate::math_word::materialize(&mut source).map_err(str::to_string)?;
    fn element(name: &str) -> XmlNode {
        XmlNode { id: String::new(), name: format!("m:{name}"), namespace: MATH_NS.into(),
            attributes: [("xmlns:m".into(), MATH_NS.into())].into(),
            attribute_namespaces: Default::default(), content: vec![] }
    }
    fn paragraph_alignment(source: &mut XmlNode, alignment: MathJustification) {
        let index = source.content.iter().position(|part|
            matches!(part, XmlPart::Element(node) if node.namespace == MATH_NS && node.local() == "oMathParaPr"))
            .unwrap_or_else(|| {
                source.content.insert(0, XmlPart::Element(element("oMathParaPr")));
                0
            });
        let XmlPart::Element(properties) = &mut source.content[index] else { unreachable!() };
        if let Some(jc) = properties.content.iter_mut().find_map(|part| match part {
            XmlPart::Element(node) if node.namespace == MATH_NS && node.local() == "jc" => Some(node),
            _ => None,
        }) {
            // An explicitly empty local jc means centerGroup. Materialize the
            // required native value in this copy, retaining source omission.
            if !jc.attributes.keys().any(|key| key.rsplit(':').next() == Some("val")
                && jc.attribute_namespaces.get(key).is_none_or(|uri| uri.is_empty() || uri == MATH_NS))
            {
                jc.attributes.insert("m:val".into(), alignment.as_str().into());
                jc.attribute_namespaces.insert("m:val".into(), MATH_NS.into());
                jc.attributes.insert("xmlns:m".into(), MATH_NS.into());
            }
            return;
        }
        let mut jc = element("jc");
        jc.attributes.insert("m:val".into(), alignment.as_str().into());
        jc.attribute_namespaces.insert("m:val".into(), MATH_NS.into());
        properties.content.insert(0, XmlPart::Element(jc));
    }
    fn resolve(source: &mut XmlNode, view: &Node, root: &Node, display: bool) {
        // Script/argument styles agree with the shared vector and MathML model.
        let parent = view.name.as_str();
        for (child, child_view) in source.content.iter_mut().filter_map(|p|
            if let XmlPart::Element(n) = p { Some(n) } else { None }).zip(&view.children)
        {
            let reduced = view.is_math() && child_view.is_math() && match parent {
                "f" => matches!(child_view.name.as_str(), "num" | "den"),
                "rad" => child_view.name == "deg",
                "sSup" | "sSub" | "sSubSup" | "sPre" | "nary" => matches!(child_view.name.as_str(), "sub" | "sup"),
                "limLow" | "limUpp" => child_view.name == "lim",
                "m" | "eqArr" => true,
                _ => false,
            };
            resolve(child, child_view, root, display && !reduced);
        }
        let display_defaults = root.child("mathPr").and_then(|pr| pr.child("dispDef"))
            .map(|node| on_off(node.attr("val"), true)).unwrap_or(true);
        let local_jc = view.child("oMathParaPr").and_then(|properties| properties.child("jc")).is_some();
        if view.is_math() && parent == "oMathPara" && display && (display_defaults || local_jc) {
            paragraph_alignment(source, math_paragraph_style(root, Some(view)).alignment);
        }
        if !view.is_math() || parent != "nary" || view.child("naryPr").and_then(|p| p.child("limLoc")).is_some() { return; }
        let mut location = element("limLoc");
        location.attributes.insert("m:val".into(), if nary_limits(view, display) {"undOvr"} else {"subSup"}.into());
        location.attribute_namespaces.insert("m:val".into(), MATH_NS.into());
        let index = source.content.iter().position(|p| matches!(p, XmlPart::Element(n) if n.namespace == MATH_NS && n.local() == "naryPr"));
        if let Some(index) = index {
            let XmlPart::Element(pr) = &mut source.content[index] else { unreachable!() };
            let at = pr.content.iter().position(|p| matches!(p, XmlPart::Element(n) if n.namespace == MATH_NS && matches!(n.local(), "grow" | "subHide" | "supHide" | "ctrlPr"))).unwrap_or(pr.content.len());
            pr.content.insert(at, XmlPart::Element(location));
        } else {
            let mut pr = element("naryPr"); pr.content.push(XmlPart::Element(location));
            source.content.insert(0, XmlPart::Element(pr));
        }
    }
    resolve(&mut source, &view, &view, display);
    let mut settings = source.children().find(|n| n.namespace == MATH_NS && n.local() == "mathPr").cloned();
    // Keep missing values in the author's DSL; materialize only in DOCX so
    // its settings obey the pinned schema and retain Office's effective value.
    if let Some(pr)=&mut settings{for part in &mut pr.content{if let XmlPart::Element(n)=part{
        if n.namespace!=MATH_NS{continue;}
        if let Some(value)=crate::math_defaults::native_implicit_value(n.local()){
            let present=n.attributes.keys().any(|k|k.rsplit(':').next()==Some("val")&&n.attribute_namespaces.get(k).is_none_or(|uri|uri.is_empty()||uri==MATH_NS));
            if !present{n.attributes.insert("m:val".into(),value.into());n.attribute_namespaces.insert("m:val".into(),MATH_NS.into());n.attributes.insert("xmlns:m".into(),MATH_NS.into());}
        }
    }}}
    source.content.retain(|p| !matches!(p, XmlPart::Element(n) if n.namespace == MATH_NS && n.local() == "mathPr"));
    if display {
        // A top-level row(mathPr, oMath, ...) fragment is a display paragraph
        // even though its authoring wrapper is the transparent #root. Group
        // consecutive equations without flattening existing paragraph groups.
        fn flush(target: &mut Vec<XmlPart>, pending: &mut Vec<XmlPart>, alignment: Option<MathJustification>) {
            if pending.is_empty() { return; }
            let mut paragraph = element("oMathPara");
            paragraph.content = std::mem::take(pending);
            if let Some(alignment) = alignment { paragraph_alignment(&mut paragraph, alignment); }
            target.push(XmlPart::Element(paragraph));
        }
        let enabled = view.child("mathPr").and_then(|properties| properties.child("dispDef"))
            .map(|node| on_off(node.attr("val"), true)).unwrap_or(true);
        let alignment = enabled.then(||math_paragraph_style(&view,None).alignment);
        let mut content = Vec::new();
        let mut pending = Vec::new();
        for part in std::mem::take(&mut source.content) {
            let equation = matches!(&part, XmlPart::Element(node) if node.namespace == MATH_NS && node.local() == "oMath");
            let invisible = matches!(&part, XmlPart::Comment(_) | XmlPart::Instruction(_))
                || matches!(&part, XmlPart::Text(text) if text.trim().is_empty());
            if equation || (!pending.is_empty() && invisible) { pending.push(part); }
            else { flush(&mut content, &mut pending, alignment); content.push(part); }
        }
        flush(&mut content, &mut pending, alignment);
        source.content = content;
    }
    Ok(DocxMathProjection{content:source.xml(),settings,paragraph,paragraph_source})
}

/// Parse LaTeX through the shared converter; `Err` when the OMML is unusable.
pub fn from_latex(latex: &str, display: bool) -> Result<Node, String> {
    let xml = crate::latex2omml::latex_to_omml_checked(latex, display).map_err(str::to_owned)?;
    parse(&xml)
}

// ------------------------------------------------------------------ MathML

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}

/// Normalise a delimiter as `latex2omml` writes it (`\{`, `\|`, `\langle` …).
pub fn delimiter_char(raw: &str) -> String {
    let t = raw.trim();
    let t = match t {
        "\\{" => "{",
        "\\}" => "}",
        "\\|" | "\\Vert" | "\\lVert" | "\\rVert" | "‖" => "‖",
        "\\vert" | "\\lvert" | "\\rvert" => "|",
        "\\langle" => "⟨",
        "\\rangle" => "⟩",
        "\\lceil" => "⌈",
        "\\rceil" => "⌉",
        "\\lfloor" => "⌊",
        "\\rfloor" => "⌋",
        "." => "",
        other => other.strip_prefix('\\').unwrap_or(other),
    };
    t.to_string()
}

/// Operator class used for spacing (PDF) and `<mo>` classification (MathML).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Ord,
    Bin,
    Rel,
    Differential,
    Punct,
    Open,
    Close,
}

/// Spacing context is independent of a phantom's painted ink and dimensions.
/// A transparent phantom must not normalize its leading binary operator as
/// unary before its enclosing row has supplied the left operand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpacingContext {
    pub left: Option<Class>,
    pub right: Option<Class>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpacingPlan {
    pub context: SpacingContext,
    pub first: Option<Class>,
    pub last: Option<Class>,
}

fn spacing_classes(n: &Node, classes: &mut Vec<Class>) {
    if n.is_props() { return; }
    if n.is_math() && n.name == "phant" {
        if n.enabled("phantPr", "transp", false) {
            if let Some(e) = n.child("e") { for child in &e.children { spacing_classes(child, classes); } }
        } else { classes.push(Class::Ord); }
    } else if n.is_math() && n.name == "box" {
        classes.push(if n.enabled("boxPr", "diff", false) { Class::Differential }
            else if n.enabled("boxPr", "opEmu", false) { emulated_operator_class(n) }
            else { Class::Ord });
    } else if n.is_math() && n.name == "r" {
        let text = n.equation_visible_text();
        let mut chars = text.chars();
        if let Some(c) = chars.next() {
            classes.push(if chars.next().is_none() && !run_style(n).normal_text && !literal_run(n) { classify(c) } else { Class::Ord });
        }
    } else if !n.is_math() || matches!(n.name.as_str(), "root" | "oMath" | "oMathPara" | "e") {
        for child in &n.children { spacing_classes(child, classes); }
    } else { classes.push(Class::Ord); }
}

/// Raw category of the existing native emulator model. Do not use a standalone
/// spacing_plan here: it would normalize a leading/trailing binary as unary.
/// This intentionally retains the current inference, including run boundaries.
pub(crate) fn emulated_operator_class(n: &Node) -> Class {
    let mut inner = Vec::new();
    if let Some(e) = n.child("e") {
        for child in &e.children { spacing_classes(child, &mut inner); }
    }
    if inner.contains(&Class::Rel) { Class::Rel } else { Class::Bin }
}

/// Linear in the number of spacing atoms, including transparent phantoms.
/// Office specifies a differential class, not an exact cross-font advance.
pub fn spacing_plans(nodes: &[Node], context: SpacingContext) -> Vec<SpacingPlan> {
    let mut classes = Vec::new();
    let ranges: Vec<_> = nodes.iter().map(|n| { let start = classes.len(); spacing_classes(n, &mut classes); start..classes.len() }).collect();
    let mut normalized = classes.clone();
    let mut previous = context.left;
    for (index, class) in normalized.iter_mut().enumerate() {
        let next = classes.get(index + 1).copied().or(context.right);
        if *class == Class::Bin && (previous.is_none() || matches!(previous, Some(Class::Bin | Class::Rel | Class::Open | Class::Punct))
            || next.is_none() || matches!(next, Some(Class::Rel | Class::Close | Class::Punct))) { *class = Class::Ord; }
        previous = Some(*class);
    }
    ranges.into_iter().map(|range| SpacingPlan {
        context: SpacingContext {
            left: range.start.checked_sub(1).and_then(|i| normalized.get(i).copied()).or(context.left),
            right: classes.get(range.end).copied().or(context.right),
        },
        first: (!range.is_empty()).then(|| normalized[range.start]),
        last: (!range.is_empty()).then(|| normalized[range.end - 1]),
    }).collect()
}

pub fn spacing_gap(left: Option<Class>, right: Option<Class>, scripts: bool) -> f64 {
    use Class::*;
    match (left, right) {
        // A 3/18 em thin space is our explicit typographic policy. Relation and
        // binary spacing take precedence; beginning/open delimiters add none.
        (Some(Ord | Close | Differential), Some(Differential)) => 3.0 / 18.0,
        (Some(_), Some(Rel)) | (Some(Rel), Some(_)) if !scripts => 5.0 / 18.0,
        (Some(_), Some(Bin)) | (Some(Bin), Some(_)) if !scripts => 4.0 / 18.0,
        (Some(Punct), Some(_)) if !scripts => 3.0 / 18.0,
        _ => 0.0,
    }
}

fn explicit_spacing(n: &Node) -> bool {
    !n.is_props() && ((n.is_math() && ((n.name == "box" && (n.enabled("boxPr", "diff", false) || n.enabled("boxPr", "opEmu", false) || n.enabled("boxPr", "noBreak", false))) || n.name == "phant" || literal_run(n)))
        || ((!n.is_math() || matches!(n.name.as_str(), "root" | "oMath" | "oMathPara" | "e")) && n.children.iter().any(explicit_spacing)))
}
pub fn classify(c: char) -> Class {
    match c {
        '+' | '−' | '-' | '±' | '∓' | '×' | '÷' | '·' | '⋅' | '∗' | '⋆' | '∘' | '∙' | '⊗' | '⊕' | '⊙' | '∩' | '∪'
        | '∖' | '∧' | '∨' => Class::Bin,
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≈' | '≡' | '∼' | '≃' | '≅' | '∝' | '≪' | '≫' | '≺' | '≻' | '→'
        | '←' | '⇒' | '⇐' | '↔' | '⇔' | '↦' | '↑' | '↓' | '∈' | '∉' | '⊂' | '⊆' | '⊃' | '⊇' | '∣' | '⊥'
        | '∥' | ':' => Class::Rel,
        ',' | ';' => Class::Punct,
        '(' | '[' | '{' | '⟨' | '⌈' | '⌊' => Class::Open,
        ')' | ']' | '}' | '⟩' | '⌉' | '⌋' => Class::Close,
        _ => Class::Ord,
    }
}

/// Run style flags read from `m:rPr`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunStyle {
    /// `m:nor` — ordinary text (`\text{}`).
    pub normal_text: bool,
    pub bold: bool,
    /// `m:i m:val="off"` — upright letters (`\mathrm{}`).
    pub upright: bool,
    pub double_struck: bool,
    pub script: bool,
    pub fraktur: bool,
    pub sans_serif: bool,
    pub monospace: bool,
    pub explicit_style: bool,
    /// Word font size in half-points; independent of CSS em/Office math spacing.
    pub half_points: Option<u32>,
    pub color: Option<[u8;3]>,
    pub word_italic: Option<bool>,
}

pub fn run_style(r: &Node) -> RunStyle {
    let r=r.equation_origin.as_deref().unwrap_or(r);
    let mut s = RunStyle::default();
    if let Some(pr)=r.children.iter().find(|n|n.name=="rPr"&&n.namespace==crate::math_document::WORD_NS){
        for c in pr.children.iter().filter(|c|c.namespace==crate::math_document::WORD_NS){match c.name.as_str(){
            "b"=>s.bold=on_off(c.attr("val"),true),
            "i"=>{let italic=on_off(c.attr("val"),true);s.word_italic=Some(italic);s.upright=!italic;s.explicit_style=true;},
            "sz"=>s.half_points=c.attr("val").and_then(|v|v.parse::<u32>().ok()).filter(|v|*v>0 && *v<=3276),
            "color"=>{if let Some(v)=c.attr("val").filter(|v|v.len()==6&&v.bytes().all(|c|c.is_ascii_hexdigit())){s.color=Some([u8::from_str_radix(&v[..2],16).unwrap(),u8::from_str_radix(&v[2..4],16).unwrap(),u8::from_str_radix(&v[4..],16).unwrap()]);}},
            _=>()
        }}
    }
    if let Some(pr) = r.child("rPr") {
        for c in &pr.children {
            match c.name.as_str() {
                "nor" => s.normal_text = on_off(c.attr("val"), true),
                "b" => s.bold = on_off(c.attr("val"), true),
                "i" => s.upright = !on_off(c.attr("val"), true),
                "sty" => {s.explicit_style=true;match c.attr("val").unwrap_or("i") {
                    "p" => {s.bold=false;s.upright=true;},
                    "b" => { s.bold = true; s.upright = true; },
                    "bi" => {s.bold=true;s.upright=false;},
                    _ => {s.bold=false;s.upright=false;},
                }},
                "scr" => match c.attr("val") {
                    Some("double-struck") => s.double_struck = true,
                    Some("script") => s.script = true,
                    Some("fraktur") => s.fraktur = true,
                    Some("sans-serif") => s.sans_serif = true,
                    Some("monospace") => s.monospace = true,
                    _ => {}
                },
                _ => {}
            }
        }
    }
    s
}

/// Office chooses a typeface per character, not just the first rFonts field.
/// Theme tokens are resolved by math_word before this rendering view exists.
#[derive(Debug,Clone,Copy,Default)]
pub(crate) struct RunFonts<'a>{pub ascii:Option<&'a str>,pub high_ansi:Option<&'a str>,pub east_asian:Option<&'a str>,pub complex:Option<&'a str>,pub east_hint:bool,pub chinese:bool,pub complex_run:bool}
pub(crate) fn run_fonts(r:&Node)->RunFonts<'_>{
    let r=r.equation_origin.as_deref().unwrap_or(r);
    let Some(pr)=r.children.iter().find(|n|n.namespace==crate::math_document::WORD_NS&&n.name=="rPr")else{return RunFonts::default();};
    let get=|key:&str|pr.children.iter().find(|n|n.namespace==crate::math_document::WORD_NS&&n.name==key);
    let fonts=get("rFonts");let lang=get("lang").and_then(|n|n.attr("eastAsia"));
    RunFonts{ascii:fonts.and_then(|n|n.attr("ascii")),high_ansi:fonts.and_then(|n|n.attr("hAnsi")),east_asian:fonts.and_then(|n|n.attr("eastAsia")),complex:fonts.and_then(|n|n.attr("cs")),east_hint:fonts.is_some_and(|n|n.attr("hint")==Some("eastAsia")),chinese:lang.is_some_and(|s|s.split('-').next().is_some_and(|v|v.eq_ignore_ascii_case("zh"))),complex_run:["cs","rtl"].iter().any(|key|get(key).is_some_and(|n|on_off(n.attr("val"),true)))}
}
impl<'a> RunFonts<'a>{
    pub(crate) fn family(self,c:char)->Option<&'a str>{
        if self.complex_run{return self.complex;}
        if self.east_asian==Some("Times New Roman")&&self.ascii.is_some()&&self.ascii==self.high_ansi{return self.ascii;}
        let value=c as u32;
        let east=self.east_hint&&(matches!(value,0x00a1|0x00a4|0x00a7..=0x00a8|0x00aa|0x00ad|0x00af|0x00b0..=0x00b4|0x00b6..=0x00ba|0x00bc..=0x00bf|0x00d7|0x00f7|0x02b0..=0x04ff|0x2000..=0x27bf|0x2e80..=0x2eff|0xe000..=0xf8ff|0xfb00..=0xfb1c)
            ||self.chinese&&matches!(value,0x00e0..=0x00e1|0x00e8..=0x00ea|0x00ec..=0x00ed|0x00f2..=0x00f3|0x00f9..=0x00fa|0x00fc|0x0100..=0x02af|0x1e00..=0x1eff));
        if east||matches!(value,0x1100..=0x11ff|0x2f00..=0x2fdf|0x2ff0..=0x319f|0x3200..=0x4dbf|0x4e00..=0x9faf|0xa000..=0xa4cf|0xac00..=0xd7af|0xf900..=0xfaff|0xfe30..=0xfe6f|0xff00..=0xffef|0x10000..=0x10ffff){return self.east_asian;}
        if matches!(value,0..=0x007f|0x0590..=0x07bf|0xfb1d..=0xfb4f|0xfb50..=0xfdff|0xfe70..=0xfefe){self.ascii}else{self.high_ansi}
    }
}

/// Office control characters (fraction rules, radicals, fences, large
/// operators, etc.) have their own formatting; it does not style the operands.
/// A present ctrlPr inherits the paragraph cascade materialized by math_word.
/// When omitted, Office uses the first character of the object instead.
pub fn control_style(n: &Node) -> RunStyle {
    let n=n.equation_origin.as_deref().unwrap_or(n);
    let properties = format!("{}Pr", n.name);
    if let Some(control) = n.child(&properties).and_then(|pr| pr.child("ctrlPr")) {
        return run_style(control);
    }
    fn first_run(n: &Node) -> Option<&Node> {
        if !n.is_math() || n.is_props() { return None; }
        if n.name == "r" && n.children.iter().any(|c| c.is_math() && c.name == "t" && !c.text.is_empty()) { return Some(n); }
        n.children.iter().find_map(first_run)
    }
    first_run(n).map(run_style).unwrap_or_default()
}

/// A run of several letters (a function name such as `sin`, or an unknown
/// command's name) is set upright, as TeX does for `\sin` / `\operatorname`.
pub fn is_word(text: &str) -> bool {
    text.trim().chars().filter(|c| c.is_alphabetic()).count() > 1
}

/// ISO 29500 lit is independent of nor and mathematical typeface. Fragmented
/// equation-array runs read the original run's property, never a guessed token
/// class or the presence of a rendered operator character.
pub fn literal_run(run: &Node) -> bool {
    let run = run.equation_origin.as_deref().unwrap_or(run);
    run.is_math() && run.name == "r" && run.enabled("rPr", "lit", false)
}

/// Convert a parsed OMML root into a MathML `<math>` element.
/// Reader-dependent spacing is reported separately from lossless structure
/// preservation. A valid MathML tree does not certify an Office layout match.
pub fn mathml_warnings(root: &Node) -> Vec<String> {
    let (mut alignment,mut scripts)=(false,false);
    let mut array_codes=Vec::new();
    let mut pending=vec![root];
    while let Some(n)=pending.pop(){
        alignment|=manual_break(n).is_some_and(|v|v>0);
        scripts|=n.is_math()&&n.name=="sSubSup"&&!n.enabled("sSubSupPr","alnScr",false);
        if n.is_math()&&n.name=="eqArr" {
            let plans=n.children.iter().filter(|r|r.is_math()&&r.name=="e").map(|r|crate::math_equation_array::plan_row(r,true)).collect::<Vec<_>>();
            if plans.iter().map(|p|p.marker_count).sum::<usize>()>crate::math_equation_array::MAX_MARKERS
                &&!array_codes.contains(&"math_equation_array_marker_budget") {array_codes.push("math_equation_array_marker_budget");}
            for issue in plans.iter().flat_map(|p|&p.diagnostics) {if !array_codes.contains(&issue.code){array_codes.push(issue.code);}}
            // Standalone Core MathML has no ambient width API. Its native
            // alignment columns are real; maximum distribution remains a
            // reported projection limitation instead of a guessed page size.
            if n.enabled("eqArrPr","maxDist",false)&&plans.iter().any(|p|p.has_markers) {
                if !array_codes.contains(&"math_equation_array_mathml_distribution") {array_codes.push("math_equation_array_mathml_distribution");}
            }
        }
        let equations=n.children.iter().filter(|c|c.is_math()&&c.name=="oMath").collect::<Vec<_>>();
        if !equations.is_empty() {
            let plans=equations.iter().map(|r|crate::math_equation_array::plan_row(r,false)).collect::<Vec<_>>();
            for issue in plans.iter().flat_map(|p|&p.diagnostics) {if !array_codes.contains(&issue.code){array_codes.push(issue.code);}}
            if plans.iter().any(|p|p.has_markers)&&equations.iter().any(|r|manual_math_lines(&r.children).len()>1)
                &&!array_codes.contains(&"math_equation_array_manual_alignment_scope") {array_codes.push("math_equation_array_manual_alignment_scope");}
        }
        pending.extend(&n.children);
    }
    let mut out=Vec::new();
    if alignment{out.push(crate::api_codes::WARN_MATHML_ALIGNMENT.into());}
    if scripts{out.push(crate::api_codes::WARN_MATHML_SCRIPT_SPACING.into());}
    out.extend(array_codes.into_iter().map(str::to_owned));
    out
}

/// The third argument is kept for source compatibility. Alternative text is
/// generated from visible presentation, not from that source/fallback payload.
pub fn to_mathml(root: &Node, display: bool, alt: &str) -> String {
    to_mathml_with_context(root,display,alt,12.0,None)
}

/// A finite projection is conditional on this exact core font and an explicit
/// physical container width. It is not a general reflow or Word certificate.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MathmlProjectionProfile {
    pub name: &'static str,
    pub font_family: &'static str,
    pub font_size: f64,
    pub available_points: f64,
    pub allocated_points: f64,
    pub natural_points: f64,
    pub overflow: bool,
}

pub struct MathmlProjection {
    pub output: String,
    pub profile: Option<MathmlProjectionProfile>,
    pub warnings: Vec<String>,
}

/// Preserve the intrinsic compatibility output unless a trustworthy finite
/// parent is supplied. Core table semantics and baselines stay native: unlike
/// CSS Grid/display:contents, finite mpadded/mspace boxes retain MathML cells.
pub fn to_mathml_constrained(
    root: &Node,
    display: bool,
    alt: &str,
    size: f64,
    width: Option<f64>,
) -> Result<MathmlProjection, &'static str> {
    use crate::math_document::{XmlNode, XmlPart};
    if !size.is_finite() || !(4.0..=512.0).contains(&size)
        || width.is_some_and(|v| !v.is_finite() || !(1.0..=20000.0).contains(&v))
    {
        return Err("math_invalid_property");
    }
    validate_display_context(root,display,width)?;
    let output = to_mathml_with_context(root, display, alt, size, None);
    let mut result = MathmlProjection { output, profile: None, warnings: mathml_warnings(root) };
    let Some(width) = width.filter(|_| display) else { return Ok(result); };
    fn sole_array(n: &Node) -> Option<&Node> {
        if !n.is_math() || !n.text.trim().is_empty() { return None; }
        if n.name == "eqArr" { return Some(n); }
        if !matches!(n.name.as_str(), "#root" | "oMath" | "oMathPara") { return None; }
        let mut children = n.children.iter().filter(|n| !n.is_props());
        let child = children.next()?;
        if children.next().is_some() { return None; }
        sole_array(child)
    }
    let Some(array) = sole_array(root) else { return Ok(result); };
    // Existing paragraph margins have a separate compatibility projection.
    // Do not silently treat their outer frame as this array's content width.
    let paragraph = root.children.iter().find(|n| n.is_math() && n.name == "oMathPara");
    if math_paragraph_style(root, paragraph).explicit_margins { return Ok(result); }
    let Some(geometry) = crate::math_layout::mathml_equation_array_geometry(array, size, width)? else { return Ok(result); };
    if geometry.solution.width > 20000.0 { return Err("math_budget"); }
    let formatted = mathml_formatted_tree(root, display, size, None);
    let Some(formatted_array) = sole_array(formatted.as_ref().unwrap_or(root)) else { return Ok(result); };
    let replacement = finite_equation_array_mathml(formatted_array, &geometry);
    let replacement = crate::math_document::parse_xml(&format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{replacement}</math>"))?;
    let replacement = replacement.children().next().and_then(|n| n.children().next()).cloned().ok_or("math_invalid_xml")?;
    fn replace(n: &mut XmlNode, replacement: &XmlNode) -> bool {
        if n.local() == "mtable" { *n = replacement.clone(); return true; }
        // Only the generated transparent wrappers on the sole-array path may
        // be traversed. No arbitrary string search or source payload is used.
        if !matches!(n.local(), "#root" | "math" | "mrow" | "mstyle" | "mpadded") { return false; }
        let mut children = n.content.iter_mut().filter_map(|part| if let XmlPart::Element(n) = part { Some(n) } else { None });
        let Some(child) = children.next() else { return false; };
        if children.next().is_some() { return false; }
        replace(child, replacement)
    }
    let mut tree = crate::math_document::parse_xml(&result.output)?;
    if !replace(&mut tree, &replacement) { return Ok(result); }
    let Some(math) = tree.content.iter_mut().find_map(|part| if let XmlPart::Element(n) = part { Some(n) } else { None }) else { return Err("math_invalid_xml"); };
    let mut css = math.attributes.get("style").cloned().unwrap_or_default();
    if !css.is_empty() { css.push(';'); }
    css.push_str(&format!("font-family:'STIX Two Math';font-size:{size}pt;font-weight:normal;font-style:normal"));
    math.attributes.insert("style".into(), css);
    math.attributes.insert("data-readmd-mathml-profile".into(), "eqarr-core-runs-finite-v1".into());
    result.output = tree.xml();
    result.profile = Some(MathmlProjectionProfile {
        name: "eqarr-core-runs-finite-v1", font_family: "STIX Two Math", font_size: size,
        available_points: width, allocated_points: geometry.solution.width,
        natural_points: geometry.solution.natural_width, overflow: geometry.solution.overflow,
    });
    result.warnings.retain(|code| code != "math_equation_array_mathml_distribution");
    if geometry.solution.overflow { result.warnings.push("math_equation_array_overflow".into()); }
    Ok(result)
}

fn finite_equation_array_mathml(n: &Node, geometry: &crate::math_layout::MathmlEquationArrayGeometry) -> String {
    let plans = n.children.iter().filter(|n| n.is_math() && n.name == "e")
        .map(|n| crate::math_equation_array::plan_row(n, true)).collect::<Vec<_>>();
    let columns = plans.iter().map(crate::math_equation_array::node_columns).collect::<Vec<_>>();
    let solution = &geometry.solution;
    let base = match table_base_justification(n) { Some("top") => "baseline 1", Some("bot") => "baseline -1", _ => "axis" };
    let mut out = format!("<mtable align=\"{base}\" rowspacing=\"{}\" style=\"border-collapse:collapse;border-spacing:0;width:{}pt\">", table_row_gap(n).mathml(), solution.width);
    let spacer = |out: &mut String, width: f64| {
        out.push_str(&format!("<mtd data-readmd-equation-gap=\"true\" style=\"padding:0\"><mspace width=\"{}pt\" height=\"0pt\" depth=\"0pt\"/></mtd>", width.max(0.0)));
    };
    for (i, (row, plan)) in columns.iter().zip(&plans).enumerate() {
        let id = plan.view.attr("readmdId").map(|id| format!(" id=\"{}\"", esc(id))).unwrap_or_default();
        out.push_str(&format!("<mtr{id}>"));
        spacer(&mut out, solution.columns[0].origin);
        for (j, column) in row.iter().enumerate() {
            let position = solution.columns[j];
            let left = position.anchor - position.origin;
            for (side, nodes, measured, cell_width) in [
                ("before", &column.before, geometry.cells[i][j][0], left),
                ("after", &column.after, geometry.cells[i][j][1], position.width - left),
            ] {
                let shift = if side == "before" { (cell_width - measured).max(0.0) } else { 0.0 };
                out.push_str(&format!("<mtd data-readmd-equation-column=\"{j}\" data-readmd-equation-side=\"{side}\" style=\"padding:0;text-align:left\"><mpadded width=\"{cell_width}pt\" lspace=\"{shift}pt\"><mrow>"));
                emit_equation_atoms(nodes, &mut out, 1, false);
                out.push_str("</mrow></mpadded></mtd>");
            }
            let edge = position.origin + position.width;
            let next = solution.columns.get(j + 1).map_or(solution.width, |next| next.origin);
            spacer(&mut out, next - edge);
        }
        out.push_str("</mtr>");
    }
    out.push_str("</mtable>");
    out
}

#[cfg(test)]
#[path = "mathml_equation_array_tests.rs"]
mod mathml_equation_array_tests;

/// Export/profile context supplies the ambient size and automatic color.
/// Plain formulas keep native, reflowable MathML; Office-formatted formulas
/// need explicit operand resets because MathML control color/size inherits.
pub(crate) fn to_mathml_with_context(root:&Node,display:bool,_alt:&str,size:f64,automatic_color:Option<[u8;3]>) -> String {
    let formatted=mathml_formatted_tree(root,display,size,automatic_color);
    let root=formatted.as_ref().unwrap_or(root);
    let mut body = String::new();
    if display {emit_display_root(root,&mut body);}else{emit_seq(&root.children, &mut body, 0, display);}
    let paragraph=root.children.iter().find(|n|n.is_math()&&n.name=="oMathPara");
    let style=math_paragraph_style(root,paragraph);
    let mut css=Vec::new();
    if display&&style.explicit_alignment{css.push(format!("text-align:{}",if style.alignment==MathJustification::CenterGroup{"center"}else{style.alignment.as_str()}));}
    if display&&style.explicit_margins{css.push(format!("margin-left:{}pt;margin-right:{}pt",style.left,style.right));}
    let alignment=if css.is_empty(){String::new()}else{format!(" style=\"{}\"",css.join(";"))};
    // The caller's compatibility argument may be raw OMML/DSL. Source
    // payload belongs to MathDocument.source/revision, never alternative text.
    let alternative=mathml_linear_description(&body).map(|text|format!(" alttext=\"{}\"",esc(&text))).unwrap_or_default();
    format!(
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"{}\"{alternative}{alignment}><mrow>{}</mrow></math>",
        if display { "block" } else { "inline" },
        body
    )
}

pub(crate) fn to_mathml_checked_with_context(root:&Node,display:bool,alt:&str,size:f64,automatic_color:Option<[u8;3]>)
    ->Result<String,&'static str>
{
    validate_display_context(root,display,None)?;
    Ok(to_mathml_with_context(root,display,alt,size,automatic_color))
}

/// Bounded linear mathematical fallback, not a speech/navigation engine.
/// It follows the actual presentation tree (so alignment markers, hidden
/// phantoms, Word properties and annotation payload cannot become speech).
/// Omit an unavailable/incomplete alternative instead of exposing source.
fn mathml_linear_description(body:&str)->Option<String>{
    use crate::math_document::{XmlNode,XmlPart};
    if body.len()>64*1024{return None;}
    let root=crate::math_document::parse_xml(&format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{body}</math>")).ok()?;
    // Structural punctuation is not visible mathematical content. Keep empty
    // matrix cells in a nonempty matrix, but do not describe an entirely
    // empty/hidden tree as a misleading row of commas and brackets.
    fn has_visible_token(node:&XmlNode)->bool{
        if matches!(node.local(),"annotation"|"annotation-xml"|"mphantom"|"mspace"|"none"|"mprescripts"){return false;}
        if matches!(node.local(),"mi"|"mn"|"mo"|"mtext"|"ms"){
            return node.content.iter().any(|part|matches!(part,XmlPart::Text(value) if value.chars().any(|c|!c.is_whitespace()&&c!='\u{2061}')));
        }
        node.children().any(has_visible_token)
    }
    if !has_visible_token(&root){return None;}
    fn text(node:&XmlNode)->Option<String>{
        if node.namespace!="http://www.w3.org/1998/Math/MathML"&&node.local()!="#root"{return None;}
        if matches!(node.local(),"annotation"|"annotation-xml"|"mphantom"|"mspace"|"none"|"mprescripts"){return Some(String::new());}
        if matches!(node.local(),"mi"|"mn"|"mo"|"mtext"|"ms"){
            if node.children().next().is_some(){return None;}
            let value=node.content.iter().filter_map(|part|if let XmlPart::Text(value)=part{Some(value.as_str())}else{None}).collect::<String>();
            return Some(value.chars().map(|c|match c{'\u{2061}'=>' ','\u{2062}'=>'·','\u{2063}'=>',','\u{2064}'=>'+',c=>c}).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "));
        }
        let children=node.children().map(text).collect::<Option<Vec<_>>>()?;
        let join=|separator:&str|children.iter().filter(|value|!value.is_empty()).cloned().collect::<Vec<_>>().join(separator);
        let pair=|index:usize|children.get(index).map(String::as_str);
        let result=match node.local(){
            "mfrac"=>{
                // A barless stack need not mean division or a binomial.
                if node.attributes.get("linethickness").is_some_and(|value|matches!(value.trim(),"0"|"0pt"|"0px"|"0em")){return None;}
                if children.len()!=2{return None;}format!("({})/({})",pair(0)?,pair(1)?)
            },
            "msqrt"=>format!("√({})",join(" ")),
            "mroot"=>{if children.len()!=2{return None;}format!("√[{}]({})",pair(1)?,pair(0)?)},
            "msub"|"munder"=>{if children.len()!=2{return None;}format!("({})_({})",pair(0)?,pair(1)?)},
            "msup"|"mover"=>{if children.len()!=2{return None;}format!("({})^({})",pair(0)?,pair(1)?)},
            "msubsup"|"munderover"=>{if children.len()!=3{return None;}format!("({})_({})^({})",pair(0)?,pair(1)?,pair(2)?)},
            // Preserve every row/cell. This is a readable coordinate form;
            // it does not claim spoken matrix/array navigation.
            "mtable"=>format!("[{}]",join("; ")),
            "mtr"=>children.join(", "),
            "semantics"=>pair(0)?.into(),
            "#root"|"math"|"mrow"|"mstyle"|"mpadded"|"menclose"|"mtd"=>join(" "),
            // Multi-script ordering needs a separate accessibility contract.
            _=>return None,
        };
        (result.len()<=4096).then_some(result)
    }
    let result=text(&root)?;(!result.is_empty()).then_some(result)
}

#[cfg(test)]
mod alternative_text_tests{
    use super::*;
    fn alternative(xml:&str,argument:&str)->(Option<String>,String){
        let root=parse(xml).unwrap();let result=to_mathml(&root,true,argument);
        let tree=crate::math_document::parse_xml(&result).unwrap();let value=tree.children().next().unwrap().attributes.get("alttext").cloned();(value,result)
    }
    #[test]fn math_alttext_describes_visible_fraction_root_and_scripts_without_xml_payload(){
        let xml="<m:oMath><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f><m:rad><m:radPr><m:degHide/></m:radPr><m:deg/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad><m:sSubSup><m:e><m:r><m:t>y</m:t></m:r></m:e><m:sub><m:r><m:t>i</m:t></m:r></m:sub><m:sup><m:r><m:t>n</m:t></m:r></m:sup></m:sSubSup></m:oMath>";
        let (value,_)=alternative(xml,xml);assert_eq!(value.as_deref(),Some("(a)/(b) √(x) (y)_(i)^(n)"));
        let (same,_)=alternative(xml,"row(raw_payload, \"private fallback\")");assert_eq!(same,value);
    }
    #[test]fn math_alttext_excludes_hidden_phantoms_and_source_annotations(){
        let xml="<m:oMath><m:r><m:t>x</m:t></m:r><m:phant><m:e><m:r><m:t>hidden value</m:t></m:r></m:e></m:phant></m:oMath>";
        let (value,rendered)=alternative(xml,"<source>private</source>");assert_eq!(value.as_deref(),Some("x"));assert!(rendered.contains("hidden value"));
        assert_eq!(mathml_linear_description("<semantics><mi>x</mi><annotation encoding='text/plain'>private</annotation></semantics>").as_deref(),Some("x"));
    }
    #[test]fn math_alttext_retains_unicode_rows_and_safe_literal_operators(){
        assert_eq!(mathml_linear_description("<mtable><mtr><mtd><mi>𝑎</mi></mtd><mtd><mo>&amp;</mo><mtext>中文 😀</mtext></mtd></mtr><mtr><mtd><mi>β</mi></mtd><mtd><mn>2</mn></mtd></mtr></mtable>").as_deref(),Some("[𝑎, & 中文 😀; β, 2]"));
        let (value,rendered)=alternative("<m:oMath><m:r><m:t>&lt;&amp;&quot;</m:t></m:r></m:oMath>","bad source");assert!(value.unwrap().contains('&'));assert!(rendered.contains("&amp;"));assert!(!rendered.contains("bad source"));
    }
    #[test]fn math_alttext_omits_empty_unsupported_and_overbudget_instead_of_source_fallback(){
        for body in ["<mspace width='1em'/>","<mphantom><mi>hidden</mi></mphantom>","<mmultiscripts><mi>x</mi><none/><none/></mmultiscripts>","<mfrac linethickness='0'><mi>n</mi><mi>k</mi></mfrac>","<mi xmlns='urn:foreign'>bad</mi>"]{assert_eq!(mathml_linear_description(body),None,"{body}");}
        assert!(mathml_linear_description(&format!("<mtext>{}</mtext>","x".repeat(4097))).is_none());
        let (value,rendered)=alternative("<m:oMath/>","<private xml='source'/>");assert!(value.is_none());assert!(!rendered.contains("alttext="));
    }
    #[test]fn math_alttext_preserves_backend_source_revision_and_presentation(){
        let source="<m:oMath><!-- author --><m:r><m:t>x</m:t></m:r></m:oMath>";
        let doc=crate::math_document::parse(source,"omml",false).unwrap();let revision=doc.revision.clone();let before=doc.root.xml();
        let (first,_)=alternative(source,source);let (second,_)=alternative(source,"different payload");assert_eq!(first,second);
        assert_eq!(doc.source,source);assert_eq!(doc.revision,revision);assert_eq!(doc.root.xml(),before);
        let root=parse(source).unwrap();let mut body=String::new();emit_seq(&root.children,&mut body,0,false);
        let expected=crate::math_document::parse_xml(&format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mrow>{body}</mrow></math>")).unwrap();
        let actual=crate::math_document::parse_xml(&to_mathml(&root,false,source)).unwrap();
        assert_eq!(actual.children().next().unwrap().children().next().unwrap().xml(),expected.children().next().unwrap().children().next().unwrap().xml());
    }
    #[test]fn math_alttext_retains_empty_matrix_coordinates_but_omits_empty_trees(){
        let diagonal="<mtable><mtr><mtd><mi>a</mi></mtd><mtd><mspace width='1em'/></mtd></mtr><mtr><mtd/><mtd><mi>b</mi></mtd></mtr></mtable>";
        assert_eq!(mathml_linear_description(diagonal).as_deref(),Some("[a, ; , b]"));
        for empty in ["<mtable><mtr><mtd/><mtd/></mtr><mtr><mtd/><mtd/></mtr></mtable>","<mfrac><mrow/><mrow/></mfrac>","<mtable><mtr><mtd><mphantom><mi>x</mi></mphantom></mtd><mtd><mtext> </mtext></mtd></mtr></mtable>"]{
            assert_eq!(mathml_linear_description(empty),None);
        }
    }
    #[test]fn math_alttext_removes_alignment_syntax_but_preserves_literal_ampersands(){
        let xml="<m:oMath><m:eqArr><m:e><m:r><m:t>a&amp;=b</m:t></m:r></m:e><m:e><m:r><m:t>c&amp;=d</m:t></m:r></m:e></m:eqArr></m:oMath>";
        let (value,_)=alternative(xml,xml);let value=value.unwrap();assert!(!value.contains('&'));assert!(value.contains(';'));for token in ['a','b','c','d','=']{assert!(value.contains(token));}
        let literal="<m:oMath><m:r><m:rPr><m:lit/></m:rPr><m:t>a&amp;b</m:t></m:r></m:oMath>";
        assert!(alternative(literal,literal).0.unwrap().contains('&'));
    }
}

fn mathml_formatted_tree(root:&Node,display:bool,size:f64,automatic_color:Option<[u8;3]>) -> Option<Node> {
    fn formatted(n:&Node)->bool {
        n.namespace==crate::math_document::WORD_NS&&n.name=="rPr"&&!n.children.is_empty() || n.children.iter().any(formatted)
    }
    // Native MathML keeps operator-dictionary margins in compact scripts.
    // Annotate unformatted scripts too, so adding a transparent phantom does
    // not switch the surrounding row to a different spacing policy. This pass
    // supplies levels only; plain script sizes remain native and reflowable.
    fn contextual(n:&Node)->bool {
        explicit_spacing(n) || (n.is_math() && (argument_size(n) != 0 || matches!(n.name.as_str(),
            "f" | "rad" | "sSup" | "sSub" | "sSubSup" | "sPre" | "nary" | "limLow" | "limUpp" | "groupChr")))
            || n.children.iter().any(contextual)
    }
    let apply_format=formatted(root);
    if !apply_format && !contextual(root){return None;}
    fn walk(n:&mut Node,st:MathStyle,base:f64,automatic_color:Option<[u8;3]>,scales:&[f64;3],apply_format:bool) {
        if !n.is_math()||n.is_props(){return;}
        let st=if matches!(n.name.as_str(),"e"|"num"|"den"|"sub"|"sup"|"deg"|"lim"|"fName"){st.argument(n)}else{st};
        n.mathml_script_level=st.level;
        if apply_format && matches!(n.name.as_str(),"r"|"f"|"sSup"|"sSub"|"sSubSup"|"sPre"|"rad"|"nary"|"d"|"m"|"eqArr"|"acc"|"bar"|"limLow"|"limUpp"|"borderBox"|"box"|"func"|"phant"|"groupChr") {
            let format=if n.name=="r"{run_style(n)}else{control_style(n)};
            n.mathml_format=Some(MathMlFormat{points:format.half_points.map(|v|f64::from(v)/2.0).unwrap_or(base)*scales[st.level as usize],color:format.color.or(automatic_color)});
        }
        let kind=n.name.clone();let small=n.math_defaults.small_fraction.unwrap_or(false);
        let group_sign_baseline=kind=="groupChr"&&group_character(n).1;
        for child in &mut n.children {
            let child_style=match(kind.as_str(),child.name.as_str()){
                ("f","num")=>st.frac_part(false,small),("f","den")=>st.frac_part(true,small),
                ("rad","deg")=>MathStyle{display:false,level:2,cramped:false},
                ("rad"|"acc","e")=>MathStyle{cramped:true,..st},
                ("sSup"|"sSubSup"|"sPre"|"nary","sup")=>st.sup(),
                ("sSub"|"sSubSup"|"sPre"|"nary","sub")=>st.sub(),
                ("limLow"|"limUpp","lim")=>st.sub(),
                ("groupChr","e") if group_sign_baseline=>st.sub(),
                ("m"|"eqArr",_)=>MathStyle{display:false,..st},
                _=>st,
            };walk(child,child_style,base,automatic_color,scales,apply_format);
        }
    }
    let mut copy=root.clone();let base=if size.is_finite()&&size>0.0{size}else{12.0};
    let scales=std::array::from_fn(|i|crate::math_layout::script_scale(i as u8));
    walk(&mut copy,MathStyle{display,level:0,cramped:false},base,automatic_color,&scales,apply_format);Some(copy)
}

fn source_row_start(n:&Node,out:&mut String){
    if let Some(id)=n.attr("readmdId"){out.push_str(&format!("<mrow id=\"{}\">",esc(id)));}else{out.push_str("<mrow>");}
}
fn emit_display_equation(n:&Node,out:&mut String,depth:usize){
    let lines=manual_math_lines(&n.children);
    if lines.len()<2{emit(n,out,depth,true);return;}
    source_row_start(n,out);
    out.push_str("<mtable displaystyle=\"true\" align=\"baseline 1\" columnalign=\"left\" rowspacing=\"0pt\">");
    for (nodes,alignment) in lines{
        // Keep the exact alignment index with the MathML projection; native
        // vector layout resolves it against real glyph positions. Independent
        // MathML reader alignment support is certified separately.
        if let Some(index)=alignment.filter(|index|*index>0){out.push_str(&format!("<mtr data-readmd-align-at=\"{index}\"><mtd>"));}else{out.push_str("<mtr><mtd>");}
        out.push_str("<mrow>");emit_seq(nodes,out,depth+1,true);out.push_str("</mrow></mtd></mtr>");
    }
    out.push_str("</mtable></mrow>");
}
fn emit_display_paragraph(root:&Node,n:&Node,out:&mut String,depth:usize){
    let style=math_paragraph_style(root,Some(n));
    let equations:Vec<_>=n.children.iter().filter(|n|n.is_math()&&n.name=="oMath").collect();
    if style.before!=0.0||style.after!=0.0{out.push_str(&format!("<mpadded height=\"+{}pt\" depth=\"+{}pt\">",style.before,style.after));}
    if equations.is_empty(){emit_display_equation(n,out,depth);}else{
        source_row_start(n,out);
        if emit_aligned_equations(&equations,out,depth,style.inter) {
            // The actual left/right cell widths establish shared anchors.
        }else if equations.len()>1{
            out.push_str(&format!("<mtable displaystyle=\"true\" align=\"baseline 1\" columnalign=\"{}\" rowspacing=\"{}pt\">",style.alignment.column(),style.inter));
            for n in equations{out.push_str("<mtr><mtd>");emit_display_equation(n,out,depth+1);out.push_str("</mtd></mtr>");}
            out.push_str("</mtable>");
        }else{emit_display_equation(equations[0],out,depth+1);}
        out.push_str("</mrow>");
    }
    if style.before!=0.0||style.after!=0.0{out.push_str("</mpadded>");}
}
fn emit_display_root(root:&Node,out:&mut String){
    let paragraphs:Vec<_>=root.children.iter().filter(|n|!n.is_props()).collect();
    let multiple=paragraphs.len()>1;
    if paragraphs.iter().any(|n|n.is_math()&&n.name=="oMathPara"){
        let style=math_paragraph_style(root,None);
        if multiple{out.push_str(&format!("<mtable displaystyle=\"true\" align=\"baseline 1\" columnalign=\"{}\" rowspacing=\"{}pt\">",style.alignment.column(),style.intra));}
        for n in paragraphs{if multiple{out.push_str("<mtr><mtd>");}emit_display_paragraph(root,n,out,0);if multiple{out.push_str("</mtd></mtr>");}}
        if multiple{out.push_str("</mtable>");}
    }else{emit_display_paragraph(root,root,out,0);}
}

fn emit_children(n: &Node, out: &mut String, depth: usize, display: bool) {
    let size = argument_size(n);
    if size != 0 { out.push_str(&format!("<mstyle scriptlevel=\"{:+}\">", -size)); }
    emit_seq(&n.children, out, depth + 1, display);
    if size != 0 { out.push_str("</mstyle>"); }
}

/// OMML argSz is relative to the argument's starting size, not an absolute
/// font size. Positive values undo script reductions; negative values add them.
/// See ISO/IEC 29500-1 22.1.2.6 / Microsoft Open XML ArgumentSize remarks.
pub fn argument_size(n: &Node) -> i8 {
    n.prop("argPr", "argSz").and_then(|v| v.parse::<i8>().ok())
        .filter(|v| (-2..=2).contains(v)).unwrap_or(0)
}

fn is_numeric_run(n: &Node) -> bool {
    if !n.is_math() || n.name != "r" || run_style(n).normal_text || literal_run(n) || n.attr("readmdId").is_some() {
        return false;
    }
    let t = n.run_text();
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Emit siblings, joining consecutive digit runs (`3`, `.`, `1`, `4`) into one `<mn>`.
fn emit_seq(children: &[Node], out: &mut String, depth: usize, display: bool) {
    if children.iter().any(|n| explicit_spacing(n) || n.mathml_script_level > 0) {
        emit_spacing_seq(children, out, depth, display, SpacingContext::default());
        return;
    }
    let mut i = 0usize;
    while i < children.len() {
        if is_numeric_run(&children[i]) {
            let mut num = String::new();
            let mut j = i;
            while j < children.len() && is_numeric_run(&children[j]) {
                num.push_str(&children[j].run_text());
                j += 1;
            }
            // A lone trailing dot is punctuation, not part of the number.
            let tail_dot = num.ends_with('.') && num.len() > 1;
            let digits = if tail_dot { &num[..num.len() - 1] } else { &num[..] };
            if digits == "." {
                out.push_str("<mo>.</mo>");
            } else {
                out.push_str(&format!("<mn>{}</mn>", esc(digits)));
            }
            if tail_dot {
                out.push_str("<mo>.</mo>");
            }
            i = j;
            continue;
        }
        emit(&children[i], out, depth, display);
        i += 1;
    }
}

/// Explicit boundary glue avoids a browser treating an opaque phantom as an
/// embellished operator. Empty mrows terminate operator-embellishment; they
/// contribute neither ink nor copy text. Internal structural layout stays native.
fn emit_spacing_seq(children: &[Node], out: &mut String, depth: usize, display: bool, context: SpacingContext) {
    if depth > MAX_DEPTH { return; }
    let plans = spacing_plans(children, context);
    let mut previous = None;
    for (n, plan) in children.iter().zip(plans).filter(|(n, _)| !n.is_props()) {
        let gap = spacing_gap(previous, plan.first, n.mathml_script_level > 0);
        if gap != 0.0 {
            let width = n.mathml_format.map(|f| format!("{:.8}pt", f.points * gap)).unwrap_or_else(|| format!("{gap:.8}em"));
            out.push_str(&format!("<mspace width=\"{width}\"/>"));
        }
        out.push_str("<mrow><mrow/>");
        emit_context(n, out, depth, display, Some(plan.context));
        out.push_str("<mrow/></mrow>");
        if plan.last.is_some() { previous = plan.last; }
    }
}

fn mrow_context(n: Option<&Node>, out: &mut String, depth: usize, display: bool, context: SpacingContext) {
    if let Some(n) = n {
        source_row_start(n, out);
        let size = argument_size(n);
        if size != 0 { out.push_str(&format!("<mstyle scriptlevel=\"{:+}\">", -size)); }
        emit_spacing_seq(&n.children, out, depth + 1, display, context);
        if size != 0 { out.push_str("</mstyle>"); }
        out.push_str("</mrow>");
    } else { out.push_str("<mrow/>"); }
}
fn mrow(n: Option<&Node>, out: &mut String, depth: usize, display: bool) {
    if let Some(id) = n.and_then(|n| n.attr("readmdId")) { out.push_str(&format!("<mrow id=\"{}\">", esc(id))); }
    else { out.push_str("<mrow>"); }
    if let Some(n) = n {
        emit_children(n, out, depth, display);
    }
    out.push_str("</mrow>");
}

/// mfrac already reduces inline children. Display smallFrac needs one extra
/// script step, applied to the arguments only (never to the fraction rule).
/// A linear fraction has no mfrac, so its argument style is explicit.
fn fraction_argument(n: Option<&Node>, out: &mut String, depth: usize, display: bool, small: bool, linear: bool) {
    let reduced = (display && small) || (!display && linear);
    if reduced || linear {
        out.push_str("<mstyle displaystyle=\"false\"");
        if reduced { out.push_str(" scriptlevel=\"+1\""); }
        out.push('>');
    }
    mrow(n, out, depth, false);
    if reduced || linear { out.push_str("</mstyle>"); }
}

fn run_mathml(r: &Node, out: &mut String) {
    run_mathml_inner(r,out);
}
fn run_mathml_inner(r: &Node, out: &mut String) {
    let text = r.run_text();
    let st = run_style(r);
    if text.is_empty() {
        return;
    }
    if st.normal_text {
        let variant=match(st.bold,st.word_italic.unwrap_or(false)){(true,true)=>" mathvariant=\"bold-italic\"",(true,false)=>" mathvariant=\"bold\"",(false,true)=>" mathvariant=\"italic\"",_=>""};
        let fonts=run_fonts(r);let mut segments=Vec::<(Option<&str>,String)>::new();
        for c in text.chars(){let family=crate::publication_fonts::math_text_family(fonts.family(c),c);if segments.last().is_some_and(|(f,_)|*f==family){segments.last_mut().unwrap().1.push(c);}else{segments.push((family,c.to_string()));}}
        for(family,text)in segments{
            let family=family.map(|f|{let mut css=String::from("font-family:'");for c in f.chars(){if c.is_ascii_alphanumeric()||c==' '||c=='-' {css.push(c);}else{css.push_str(&format!("\\{:X} ",c as u32));}}css.push_str("';");format!(" style=\"{}\"",esc(&css))}).unwrap_or_default();
            out.push_str(&format!("<mtext{variant}{family}>{}</mtext>", esc(&text)));
        }
        return;
    }
    let variant = if st.monospace {
        " mathvariant=\"monospace\""
    } else if st.sans_serif {
        match (st.bold,!st.upright&&st.explicit_style){
            (true,true)=>" mathvariant=\"sans-serif-bold-italic\"",
            (true,false)=>" mathvariant=\"bold-sans-serif\"",
            (false,true)=>" mathvariant=\"sans-serif-italic\"",
            _=>" mathvariant=\"sans-serif\"",
        }
    } else if st.fraktur {
        if st.bold{" mathvariant=\"bold-fraktur\""}else{" mathvariant=\"fraktur\""}
    } else if st.double_struck {
        " mathvariant=\"double-struck\""
    } else if st.script {
        if st.bold{" mathvariant=\"bold-script\""}else{" mathvariant=\"script\""}
    } else if st.bold {
        if !st.upright&&st.explicit_style{" mathvariant=\"bold-italic\""}else{" mathvariant=\"bold\""}
    } else if st.upright {
        " mathvariant=\"normal\""
    } else if st.explicit_style {
        " mathvariant=\"italic\""
    } else {
        ""
    };
    if literal_run(r) {
        for c in text.chars() {
            if let Some(width) = crate::math_layout::literal_space_advance(c) {
                out.push_str(&format!("<mspace width=\"{width:.8}em\"/>"));
            } else if c.is_whitespace() {
                // Unsupported whitespace remains visible source content. It
                // must not disappear through trim()/function-name inference.
                out.push_str(&format!("<mtext xml:space=\"preserve\" style=\"white-space:pre\">{}</mtext>", esc(&c.to_string())));
            } else if c.is_ascii_digit() {
                out.push_str(&format!("<mn{variant}>{c}</mn>"));
            } else if c.is_alphabetic() {
                out.push_str(&format!("<mi{variant}>{}</mi>", esc(&c.to_string())));
            } else {
                // Literal punctuation is an ordinary identifier, not an mo
                // which could regain dictionary spacing/stretching/limits.
                out.push_str(&format!("<mi mathvariant=\"normal\">{}</mi>", esc(&c.to_string())));
            }
        }
        return;
    }
    if r.equation_word.unwrap_or_else(||is_word(&text)) {
        let v = if variant.is_empty() { " mathvariant=\"normal\"" } else { variant };
        out.push_str(&format!("<mi{v}>{}</mi>", esc(text.trim())));
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            let num: String = chars[i..j].iter().collect();
            out.push_str(&format!("<mn{variant}>{}</mn>", esc(&num)));
            i = j;
            continue;
        }
        if c.is_alphabetic() || matches!(c, '∞' | '∂' | '∇' | 'ℏ' | 'ℓ' | '∅' | '′') {
            out.push_str(&format!("<mi{variant}>{}</mi>", esc(&c.to_string())));
        } else {
            out.push_str(&format!("<mo>{}</mo>", esc(&c.to_string())));
        }
        i += 1;
    }
}

fn emit(n: &Node, out: &mut String, depth: usize, display: bool) {
    emit_context(n, out, depth, display, None);
}
fn emit_context(n: &Node, out: &mut String, depth: usize, display: bool, context: Option<SpacingContext>) {
    if let Some(format)=n.mathml_format {
        let color=format.color.map(|[r,g,b]|format!("#{r:02X}{g:02X}{b:02X}")).unwrap_or_else(||"CanvasText".into());
        let size=format!("{:.6}",format.points);let size=size.trim_end_matches('0').trim_end_matches('.');
        out.push_str(&format!("<mstyle mathcolor=\"{color}\" mathsize=\"{size}pt\">"));
    }
    if let Some(id)=n.attr("readmdId") {
        out.push_str(&format!("<mrow id=\"{}\">",esc(id)));emit_inner(n, out, depth, display, context);out.push_str("</mrow>");
    }else{emit_inner(n, out, depth, display, context);}
    if n.mathml_format.is_some(){out.push_str("</mstyle>");}
}
fn emit_equation_atoms(nodes:&[Node],out:&mut String,depth:usize,display:bool) {
    for n in nodes {
        let gap=n.equation_glue.map_or(0.0,|(a,b)|spacing_gap(a,b,n.mathml_script_level>0));
        if gap!=0.0 {
            let width=n.mathml_format.map(|f|format!("{:.8}pt",f.points*gap)).unwrap_or_else(||format!("{gap:.8}em"));
            out.push_str(&format!("<mspace width=\"{width}\"/>"));
        }
        out.push_str("<mrow><mrow/>");
        emit_context(n,out,depth,display,n.equation_context.or(Some(SpacingContext::default())));
        out.push_str("<mrow/></mrow>");
    }
}

fn emit_aligned_equations(equations:&[&Node],out:&mut String,depth:usize,gap:f64)->bool {
    let plans=equations.iter().map(|n|crate::math_equation_array::plan_row(n,false)).collect::<Vec<_>>();
    if !plans.iter().any(|p|p.has_markers)||plans.iter().any(|p|!p.diagnostics.is_empty())
        ||equations.iter().any(|n|manual_math_lines(&n.children).len()>1) {return false;}
    let columns=plans.iter().map(crate::math_equation_array::node_columns).collect::<Vec<_>>();
    out.push_str(&format!("<mtable displaystyle=\"true\" align=\"baseline 1\" rowspacing=\"{gap}pt\" style=\"border-collapse:collapse;border-spacing:0\">"));
    for row in columns {
        let c=&row[0];out.push_str("<mtr>");
        if c.aligned {
            for (nodes,align) in [(&c.before,"right"),(&c.after,"left")] {
                out.push_str(&format!("<mtd style=\"padding:0;text-align:{align}\"><mrow>"));
                emit_equation_atoms(nodes,out,depth+1,true);out.push_str("</mrow></mtd>");
            }
        }else{
            out.push_str("<mtd columnspan=\"2\" style=\"padding:0;text-align:left\"><mrow>");
            emit_equation_atoms(&c.before,out,depth+1,true);out.push_str("</mrow></mtd>");
        }
        out.push_str("</mtr>");
    }
    out.push_str("</mtable>");true
}

fn emit_equation_array(n:&Node,out:&mut String,depth:usize)->bool {
    let plans=n.children.iter().filter(|c|c.is_math()&&c.name=="e").map(|r|crate::math_equation_array::plan_row(r,true)).collect::<Vec<_>>();
    if plans.iter().any(|p|!p.diagnostics.is_empty())||!plans.iter().any(|p|p.has_markers)
        ||plans.iter().map(|p|p.marker_count).sum::<usize>()>crate::math_equation_array::MAX_MARKERS {return false;}
    let columns=plans.iter().map(crate::math_equation_array::node_columns).collect::<Vec<_>>();
    let count=columns.iter().map(Vec::len).max().unwrap_or(0);
    let base=match table_base_justification(n){Some("top")=>"baseline 1",Some("bot")=>"baseline -1",_=>"axis"};
    out.push_str(&format!("<mtable align=\"{base}\" rowspacing=\"{}\" style=\"border-collapse:collapse;border-spacing:0\">",table_row_gap(n).mathml()));
    for (row,plan) in columns.iter().zip(&plans) {
        let id=plan.view.attr("readmdId").map(|id|format!(" id=\"{}\"",esc(id))).unwrap_or_default();
        out.push_str(&format!("<mtr{id}>"));
        for j in 0..count {
            let column=row.get(j);
            if let Some(column)=column.filter(|c|!c.aligned) {
                out.push_str("<mtd columnspan=\"2\" style=\"padding:0;text-align:left\"><mrow>");
                let size=argument_size(&plan.view);
                if size!=0{out.push_str(&format!("<mstyle scriptlevel=\"{:+}\">",-size));}
                emit_equation_atoms(&column.before,out,depth+1,false);
                if size!=0{out.push_str("</mstyle>");}out.push_str("</mrow></mtd>");continue;
            }
            for right in [false,true] {
                // Core uses CSS table cells. No malignmark/columnspacing
                // extension is required for these intrinsic alignment groups.
                out.push_str(&format!("<mtd style=\"padding:0;text-align:{}\"><mrow>",if right{"left"}else{"right"}));
                let size=argument_size(&plan.view);
                if size!=0 {out.push_str(&format!("<mstyle scriptlevel=\"{:+}\">",-size));}
                if let Some(c)=column {
                    if !right {emit_equation_atoms(&c.before,out,depth+1,false);}
                    else {emit_equation_atoms(&c.after,out,depth+1,false);}
                }
                if size!=0 {out.push_str("</mstyle>");}
                out.push_str("</mrow></mtd>");
            }
        }
        out.push_str("</mtr>");
    }
    out.push_str("</mtable>");true
}

fn emit_inner(n: &Node, out: &mut String, depth: usize, display: bool, context: Option<SpacingContext>) {
    if depth > MAX_DEPTH || n.is_props() {
        return;
    }
    if n.namespace==crate::math_spacing::MATHML_NS&&n.name=="mspace"{
        let space=crate::math_spacing::Spacing::from_attributes(n.attrs.iter().map(|(key,value)|(key.as_str(),value.as_str()))).expect("validated MathML space");
        out.push_str("<mspace");for(key,value)in space.attributes(){out.push_str(&format!(" {key}=\"{}\"",esc(value)));}out.push_str("/>");return;
    }
    if !n.is_math() { emit_children(n, out, depth, display); return; }
    match n.name.as_str() {
        "r" => {
            if context.is_some() {
                let mut run = String::new();
                run_mathml(n, &mut run);
                out.push_str(&run.replace("<mo>", "<mo lspace=\"0em\" rspace=\"0em\">"));
            } else { run_mathml(n, out); }
        },
        "f" => {
            let nobar = n.prop("fPr", "type") == Some("noBar");
            let ty = n.prop("fPr", "type").unwrap_or("bar");
            let small = n.math_defaults.small_fraction.unwrap_or(false);
            if ty == "lin" {
                out.push_str("<mrow>");
                fraction_argument(n.child("num"), out, depth, display, small, true);
                out.push_str("<mo>/</mo>");
                fraction_argument(n.child("den"), out, depth, display, small, true);
                out.push_str("</mrow>");return;
            }
            out.push_str(if nobar { "<mfrac linethickness=\"0\">" } else if ty == "skw" { "<mfrac bevelled=\"true\">" } else { "<mfrac>" });
            fraction_argument(n.child("num"), out, depth, display, small, false);
            fraction_argument(n.child("den"), out, depth, display, small, false);
            out.push_str("</mfrac>");
        }
        "sSup" => {
            out.push_str("<msup>");
            mrow(n.child("e"), out, depth, display);
            mrow(n.child("sup"), out, depth, false);
            out.push_str("</msup>");
        }
        "sSub" => {
            out.push_str("<msub>");
            mrow(n.child("e"), out, depth, display);
            mrow(n.child("sub"), out, depth, false);
            out.push_str("</msub>");
        }
        "sSubSup" => {
            out.push_str("<msubsup>");
            // A zero-width, non-slanted final atom absorbs the base's italic
            // correction and leaves both scripts at one common start column.
            let aligned=n.enabled("sSubSupPr","alnScr",false);
            if aligned { out.push_str("<mrow>"); }
            mrow(n.child("e"), out, depth, display);
            if aligned { out.push_str("<mspace width=\"0em\"/></mrow>"); }
            mrow(n.child("sub"), out, depth, false);
            mrow(n.child("sup"), out, depth, false);
            out.push_str("</msubsup>");
        }
        "rad" => {
            let hide = n.enabled("radPr", "degHide", false);
            let deg = n.child("deg").filter(|d| !d.children.is_empty());
            if hide || deg.is_none() {
                out.push_str("<msqrt>");
                mrow(n.child("e"), out, depth, display);
                out.push_str("</msqrt>");
            } else {
                out.push_str("<mroot>");
                mrow(n.child("e"), out, depth, display);
                mrow(deg, out, depth, false);
                out.push_str("</mroot>");
            }
        }
        "nary" => {
            let chr = n.character("naryPr", "chr", "∫");
            let und_ovr = nary_limits(n, display);
            let movable = n.child("naryPr").and_then(|p| p.child("limLoc")).is_none() && nary_document_limits(n).is_none();
            let sub = n.child("sub").filter(|s| !s.children.is_empty() && !n.enabled("naryPr","subHide",false));
            let sup = n.child("sup").filter(|s| !s.children.is_empty() && !n.enabled("naryPr","supHide",false));
            // A growing operator must stretch against its own operand, rather
            // than unrelated siblings in the containing formula.
            out.push_str("<mrow>");
            let op = format!("<mo largeop=\"true\" movablelimits=\"{movable}\" stretchy=\"{}\">{}</mo>",n.enabled("naryPr","grow",false),esc(chr));
            let (tag_both, tag_sub, tag_sup) =
                if und_ovr { ("munderover", "munder", "mover") } else { ("msubsup", "msub", "msup") };
            match (sub, sup) {
                (Some(b), Some(p)) => {
                    out.push_str(&format!("<{tag_both}>{op}"));
                    mrow(Some(b), out, depth, false);
                    mrow(Some(p), out, depth, false);
                    out.push_str(&format!("</{tag_both}>"));
                }
                (Some(b), None) => {
                    out.push_str(&format!("<{tag_sub}>{op}"));
                    mrow(Some(b), out, depth, false);
                    out.push_str(&format!("</{tag_sub}>"));
                }
                (None, Some(p)) => {
                    out.push_str(&format!("<{tag_sup}>{op}"));
                    mrow(Some(p), out, depth, false);
                    out.push_str(&format!("</{tag_sup}>"));
                }
                (None, None) => out.push_str(&op),
            }
            if let Some(e) = n.child("e") {
                mrow(Some(e), out, depth, display);
            }
            out.push_str("</mrow>");
        }
        "d" => {
            let beg = delimiter_char(n.character("dPr", "begChr", "("));
            let end = delimiter_char(n.character("dPr", "endChr", ")"));
            let separator = delimiter_char(n.character("dPr", "sepChr", "│"));
            let grow = n.enabled("dPr", "grow", true);
            let symmetric = n.prop("dPr", "shp") != Some("match");
            out.push_str("<mrow>");
            if !beg.is_empty() {
                out.push_str(&format!("<mo fence=\"true\" stretchy=\"{grow}\" symmetric=\"{symmetric}\">{}</mo>", esc(&beg)));
            }
            for (index,e) in n.children.iter().filter(|c| c.name == "e" && c.is_math()).enumerate() {
                if index>0 && !separator.is_empty() {out.push_str(&format!("<mo separator=\"true\" stretchy=\"{grow}\" symmetric=\"{symmetric}\">{}</mo>",esc(&separator)));}
                mrow(Some(e), out, depth, display);
            }
            if !end.is_empty() {
                out.push_str(&format!("<mo fence=\"true\" stretchy=\"{grow}\" symmetric=\"{symmetric}\">{}</mo>", esc(&end)));
            }
            out.push_str("</mrow>");
        }
        "m" => {
            let columns=n.children.iter().filter(|r|r.name=="mr" && r.is_math()).map(|r|r.children.iter().filter(|c|c.name=="e" && c.is_math()).count()).max().unwrap_or(0);
            let alignments=matrix_column_alignments(n,columns);
            let base=match table_base_justification(n){Some("top")=>"baseline 1",Some("bot")=>"baseline -1",_=>"axis"};
            out.push_str(&format!("<mtable columnalign=\"{}\" align=\"{base}\" columnspacing=\"{}\" rowspacing=\"{}\">",alignments.join(" "),matrix_column_gap(n).mathml(),table_row_gap(n).mathml()));
            let minimum = matrix_column_minimum(n);
            let minimum_style = match minimum {
                MathLength::Points(v) if v > 0.0 => format!("min-width:{v}pt;"),
                _ => String::new(),
            };
            let half_gap = matrix_column_gap(n).half().mathml();
            for row in n.children.iter().filter(|c| c.name == "mr" && c.is_math()) {
                out.push_str("<mtr>");
                for (column,cell) in row.children.iter().filter(|c| c.name == "e" && c.is_math()).enumerate() {
                    // MathML Core omits columnspacing. Explicit half-padding
                    // supplies only the interior gap, and content-box prevents
                    // app-wide border-box CSS swallowing it into cSp.
                    let left=if column==0 {"0pt"} else {&half_gap};
                    let right=if column+1==columns {"0pt"} else {&half_gap};
                    out.push_str(&format!("<mtd style=\"box-sizing:content-box;{minimum_style}padding-left:{left};padding-right:{right};text-align:{}\">",alignments[column]));
                    if empty_argument(cell) {
                        let (w, h, d, stroke) = MATRIX_PLACEHOLDER;
                        let outline = if matrix_placeholders_visible(n) { format!(" style=\"outline:{stroke}em solid currentColor;outline-offset:-{stroke}em\"") } else { String::new() };
                        let id = cell.attr("readmdId").map(|id| format!(" id=\"{}\"", esc(id))).unwrap_or_default();
                        out.push_str(&format!("<mrow{id}>"));
                        let size = argument_size(cell);
                        if size != 0 { out.push_str(&format!("<mstyle scriptlevel=\"{:+}\">", -size)); }
                        out.push_str(&format!("<mspace width=\"{w}em\" height=\"{h}em\" depth=\"{d}em\" data-readmd-matrix-placeholder=\"true\"{outline}/>"));
                        if size != 0 { out.push_str("</mstyle>"); }
                        out.push_str("</mrow>");
                    } else {
                        mrow(Some(cell), out, depth, false);
                    }
                    out.push_str("</mtd>");
                }
                out.push_str("</mtr>");
            }
            out.push_str("</mtable>");
        }
        "eqArr" => {
            if emit_equation_array(n,out,depth) {return;}
            let base=match table_base_justification(n){Some("top")=>"baseline 1",Some("bot")=>"baseline -1",_=>"axis"};
            out.push_str(&format!("<mtable columnalign=\"left\" align=\"{base}\" rowspacing=\"{}\">",table_row_gap(n).mathml()));
            for row in n.children.iter().filter(|c| c.name == "e" && c.is_math()) {
                out.push_str("<mtr><mtd>");
                mrow(Some(row), out, depth, false);
                out.push_str("</mtd></mtr>");
            }
            out.push_str("</mtable>");
        }
        "acc" => {
            let chr = n.character("accPr", "chr", "^");
            if chr.is_empty() { mrow(n.child("e"), out, depth, display); return; }
            out.push_str("<mover accent=\"true\">");
            mrow(n.child("e"), out, depth, display);
            out.push_str(&format!("<mo>{}</mo></mover>", esc(chr)));
        }
        "bar" => {
            let bottom = n.prop("barPr", "pos") == Some("bot");
            if bottom {
                out.push_str("<munder accentunder=\"true\">");
                mrow(n.child("e"), out, depth, display);
                out.push_str("<mo stretchy=\"true\">_</mo></munder>");
            } else {
                out.push_str("<mover accent=\"true\">");
                mrow(n.child("e"), out, depth, display);
                out.push_str("<mo stretchy=\"true\">¯</mo></mover>");
            }
        }
        "limLow" => {
            out.push_str("<munder accentunder=\"false\">");
            mrow(n.child("e"), out, depth, display);
            mrow(n.child("lim"), out, depth, false);
            out.push_str("</munder>");
        }
        "limUpp" => {
            out.push_str("<mover accent=\"false\">");
            mrow(n.child("e"), out, depth, display);
            mrow(n.child("lim"), out, depth, false);
            out.push_str("</mover>");
        }
        "borderBox" => {
            let mut notation=Vec::new();
            for (property,name) in [("hideTop","top"),("hideBot","bottom"),("hideLeft","left"),("hideRight","right")] {if !n.enabled("borderBoxPr",property,false) {notation.push(name);}}
            for (property,name) in [("strikeH","horizontalstrike"),("strikeV","verticalstrike"),("strikeBLTR","updiagonalstrike"),("strikeTLBR","downdiagonalstrike")] {if n.enabled("borderBoxPr",property,false) {notation.push(name);}}
            out.push_str(&format!("<menclose notation=\"{}\">",notation.join(" ")));
            mrow(n.child("e"), out, depth, display);
            out.push_str("</menclose>");
        }
        "sPre" => {out.push_str("<mmultiscripts>");mrow(n.child("e"), out, depth, display);out.push_str("<mprescripts/>");mrow(n.child("sub"), out, depth, false);mrow(n.child("sup"), out, depth, false);out.push_str("</mmultiscripts>");}
        "func" => {out.push_str("<mrow>");mrow(n.child("fName"), out, depth, display);out.push_str("<mo>&#x2061;</mo>");mrow(n.child("e"), out, depth, display);out.push_str("</mrow>");}
        "box" => {
            if n.enabled("boxPr","noBreak",false) {
                out.push_str("<mrow><mrow/>");mrow_context(n.child("e"),out,depth,display,SpacingContext::default());out.push_str("<mrow/></mrow>");
            } else {out.push_str("<mrow>");mrow(n.child("e"), out, depth, display);out.push_str("</mrow>");}
        }
        "phant" => {
            out.push_str("<mpadded");for (property,attribute) in [("zeroWid","width"),("zeroAsc","height"),("zeroDesc","depth")] {if n.enabled("phantPr",property,false) {out.push_str(&format!(" {attribute}=\"0\""));}}out.push('>');
            let hidden = !n.enabled("phantPr", "show", false);
            if hidden { out.push_str("<mphantom>"); }
            let inherited = if n.enabled("phantPr", "transp", false) { context.unwrap_or_default() } else { SpacingContext::default() };
            mrow_context(n.child("e"), out, depth, display, inherited);
            if hidden { out.push_str("</mphantom>"); }
            out.push_str("</mpadded>");
        }
        "groupChr" => {
            let (bottom, sign_baseline, chr) = group_character(n);
            if chr.is_empty() { mrow(n.child("e"), out, depth, display); return; }
            let tag = if bottom != sign_baseline { "munder" } else { "mover" };
            let accent = if tag == "munder" { "accentunder" } else { "accent" };
            // When the character owns the baseline, the expression is the
            // limit; otherwise the expression stays at its normal size.
            out.push_str(&format!("<{tag} {accent}=\"{}\">", !sign_baseline));
            if sign_baseline { out.push_str(&format!("<mo stretchy=\"true\">{}</mo>", esc(chr))); }
            mrow(n.child("e"), out, depth, if sign_baseline { false } else { display });
            if !sign_baseline { out.push_str(&format!("<mo stretchy=\"true\">{}</mo>", esc(chr))); }
            out.push_str(&format!("</{tag}>"));
        }
        // Containers (`oMath`, `oMathPara`, `e`, `num`, …) and anything unknown:
        // keep the content so nothing the author wrote is lost.
        _ => emit_children(n, out, depth, display),
    }
}

/// LaTeX → MathML through the shared OMML converter.  `None` when the formula
/// cannot be represented (the caller keeps the LaTeX source as text).
pub fn latex_to_mathml(latex: &str, display: bool) -> Option<String> {
    let root = from_latex(latex, display).ok()?;
    Some(to_mathml(&root, display, latex))
}

#[cfg(test)]
mod tests {

    #[test]
    fn differential_and_phantom_spacing_plans_preserve_explicit_switches() {
        use super::*;
        let sample = |props: &str, body: &str| parse(&format!("<m:oMath><m:r><m:t>x</m:t></m:r><m:phant><m:phantPr>{props}</m:phantPr><m:e>{body}</m:e></m:phant><m:r><m:t>z</m:t></m:r></m:oMath>")).unwrap();
        for (props, expected) in [("", Class::Ord), ("<m:transp/>", Class::Bin), ("<m:transp m:val=\"true\"/>", Class::Bin), ("<m:transp m:val=\"false\"/>", Class::Ord)] {
            let root = sample(props, "<m:r><m:t>+</m:t></m:r>");
            let plans = spacing_plans(&root.child("oMath").unwrap().children, SpacingContext::default());
            assert_eq!(plans[1].first, Some(expected));
            assert_eq!(plans[1].last, Some(expected));
            assert_eq!(plans[1].context.left, Some(Class::Ord));
        }
        let root = sample("<m:transp/>", "<m:box><m:boxPr><m:diff/></m:boxPr><m:e><m:r><m:t>dx</m:t></m:r></m:e></m:box>");
        let plans = spacing_plans(&root.child("oMath").unwrap().children, SpacingContext::default());
        assert_eq!(plans[1].first, Some(Class::Differential));
        assert_eq!(spacing_gap(plans[0].last, plans[1].first, false), 1. / 6.);
        assert_eq!(spacing_gap(None, plans[1].first, false), 0.);
    }

    #[test]
    fn mathml_phantom_and_differential_emit_real_spacing_without_fake_text() {
        use super::*;
        let source = |transp: &str| format!("<m:oMath><m:r><m:t>x</m:t></m:r><m:phant><m:phantPr><m:show/>{transp}</m:phantPr><m:e><m:r><m:t>+</m:t></m:r></m:e></m:phant><m:r><m:t>z</m:t></m:r></m:oMath>");
        let on = to_mathml(&parse(&source("<m:transp/>")).unwrap(), true, "");
        let off = to_mathml(&parse(&source("<m:transp m:val=\"false\"/>")).unwrap(), true, "");
        assert_eq!(on.matches("width=\"0.22222222em\"").count(), 2);
        assert!(!off.contains("<mspace"));
        assert!(on.contains("<mo lspace=\"0em\" rspace=\"0em\">+</mo>"));
        assert!(!on.contains("mphantom"));
        let diff = parse("<m:oMath><m:r><m:t>x</m:t></m:r><m:box><m:boxPr><m:diff/></m:boxPr><m:e><m:r><m:t>dx</m:t></m:r></m:e></m:box></m:oMath>").unwrap();
        let result = to_mathml(&diff, true, "");
        assert_eq!(result.matches("width=\"0.16666667em\"").count(), 1);
        assert!(!result.contains("&#x2063;"));
    }

    #[test]
    fn mathml_plain_and_transparent_phantom_scripts_share_operator_spacing() {
        let run = |text: &str| format!("<m:r><m:t>{text}</m:t></m:r>");
        for operator in ["+", "="] {
            for script in ["sSup", "sSub"] {
                let argument = if script == "sSup" { "sup" } else { "sub" };
                for wrapped in [false, true] {
                    let middle = if wrapped {
                        format!("<m:phant><m:phantPr><m:show/><m:transp/></m:phantPr><m:e>{}</m:e></m:phant>", run(operator))
                    } else { run(operator) };
                    let root = parse(&format!("<m:oMath><m:{script}><m:e>{}</m:e><m:{argument}>{}{}{}</m:{argument}></m:{script}></m:oMath>", run("x"), run("a"), middle, run("b"))).unwrap();
                    let output = to_mathml(&root, true, "");
                    assert!(output.contains(&format!("<mo lspace=\"0em\" rspace=\"0em\">{operator}</mo>")), "{script}, {wrapped}: {output}");
                    assert!(!output.contains("<mspace"), "Binary/relation gaps vanish at script level: {output}");
                    assert!(!output.contains("mathsize="), "Metadata must not force absolute script sizes: {output}");
                }
            }
            let plain = parse(&format!("<m:oMath>{}{}{}</m:oMath>", run("a"), run(operator), run("b"))).unwrap();
            let output = to_mathml(&plain, true, "");
            assert!(output.contains(&format!("<mo>{operator}</mo>")));
            assert!(!output.contains("lspace="), "Unformatted non-script math retains its native path");
        }
    }
    #[test]
    fn matrix_minimum_width_and_placeholder_switches_follow_office_defaults() {
        use super::*;
        for (pr, expected) in [("",0.0),("<m:cSp/>",0.0),(r#"<m:cSp m:val="0"/>"#,0.0),(r#"<m:cSp m:val="720"/>"#,36.0),(r#"<m:cSp m:val="31680"/>"#,1584.0)] {
            let root=parse(&format!("<m:m><m:mPr>{pr}</m:mPr></m:m>")).unwrap();
            assert_eq!(matrix_column_minimum(&root.children[0]),MathLength::Points(expected));
        }
        for (value,visible) in [(None,true),(Some(""),false),(Some("1"),false),(Some("true"),false),(Some("on"),false),(Some("0"),true),(Some("false"),true),(Some("off"),true)] {
            let pr=value.map(|v|if v.is_empty(){"<m:plcHide/>".into()}else{format!(r#"<m:plcHide m:val="{v}"/>"#)}).unwrap_or_default();
            let root=parse(&format!("<m:m><m:mPr>{pr}</m:mPr></m:m>")).unwrap();
            assert_eq!(matrix_placeholders_visible(&root.children[0]),visible,"{pr}");
        }
        let foreign=parse(r#"<m:m xmlns:x="urn:impostor"><x:mPr><x:cSp m:val="720"/><x:plcHide/></x:mPr></m:m>"#).unwrap();
        assert_eq!(matrix_column_minimum(&foreign.children[0]),MathLength::Points(0.0));
        assert!(matrix_placeholders_visible(&foreign.children[0]));
    }

    #[test]
    fn matrix_mathml_uses_minimum_css_and_empty_slots_without_adding_text() {
        use super::*;
        let xml=|hide:&str|format!(r#"<m:oMath><m:m><m:mPr><m:cSp m:val="720"/>{hide}</m:mPr><m:mr><m:e readmdId="rd-formula-token-omml-0-1"><m:argPr><m:argSz m:val="-1"/></m:argPr></m:e><m:e><m:r><m:t> </m:t></m:r></m:e><m:e><m:phant><m:phantPr><m:zeroWid/></m:phantPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:phant></m:e></m:mr></m:m></m:oMath>"#);
        let root=parse(&xml("")).unwrap();let output=to_mathml(&root,true,"");
        assert_eq!(output.matches("min-width:36pt").count(),3);
        assert!(output.contains("box-sizing:content-box;min-width:36pt;padding-left:0pt;padding-right:0.5em;text-align:center"));
        assert!(!output.contains("columnwidth="));
        assert_eq!(output.matches("outline:0.04em").count(),1);
        assert!(output.contains(r#"<mrow id="rd-formula-token-omml-0-1"><mstyle scriptlevel="+1"><mspace"#));
        let hidden=to_mathml(&parse(&xml("<m:plcHide/>")).unwrap(),true,"");
        assert!(!hidden.contains("outline:"));assert_eq!(hidden.matches("<mspace ").count(),1);
        // Empty boxes remain structural targets; spaces and invisible content
        // must not be replaced by a visible placeholder.
        let row=root.children[0].child("m").unwrap().child("mr").unwrap();
        assert!(empty_argument(&row.children[0]));assert!(!empty_argument(&row.children[1]));assert!(!empty_argument(&row.children[2]));
        let pretty=parse("<m:e>\n <m:r>\n <m:t/>\n </m:r>\n</m:e>").unwrap();
        assert!(empty_argument(&pretty.children[0]));
    }
    #[test]
    fn group_character_defaults_and_vertical_baselines_reach_mathml() {
        let xml=|pr:&str|format!("<m:oMath><m:groupChr>{pr}<m:e><m:r><m:t>x</m:t></m:r></m:e></m:groupChr></m:oMath>");
        let output=super::to_mathml(&super::parse(&xml("")).unwrap(),true,"");
        assert!(output.contains("<munder accentunder=\"true\">"));assert!(output.contains(">⏟</mo>"));
        for (pos,jc,tag,first_sign) in [("top","top","munder",true),("top","bot","mover",false),("bot","top","munder",false),("bot","bot","mover",true)] {
            let pr=format!(r#"<m:groupChrPr><m:chr m:val="⏞"/><m:pos m:val="{pos}"/><m:vertJc m:val="{jc}"/></m:groupChrPr>"#);
            let output=super::to_mathml(&super::parse(&xml(&pr)).unwrap(),true,"");
            assert!(output.contains(&format!("<{tag} ")));assert_eq!(output.find(">⏞</mo>").unwrap()<output.find(">x</mi>").unwrap(),first_sign);
        }
        for object in ["groupChr","acc"] {
            let tree=super::parse(&format!("<m:oMath><m:{object}><m:{object}Pr><m:chr/></m:{object}Pr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:{object}></m:oMath>")).unwrap();
            let output=super::to_mathml(&tree,true,"");assert!(output.contains(">x</mi>"));assert!(!output.contains("<mo>"));
        }
    }
    #[test]
    fn mathml_office_controls_reset_operands_and_share_physical_script_sizes() {
        let xml=r#"<m:oMath><m:f><m:fPr><m:ctrlPr><w:rPr><w:sz w:val="48"/><w:color w:val="CC3366"/></w:rPr></m:ctrlPr></m:fPr><m:num><m:r><w:rPr><w:sz w:val="32"/><w:color w:val="336699"/></w:rPr><m:t>x</m:t></m:r></m:num><m:den><m:r><w:rPr><w:sz w:val="32"/><w:color w:val="auto"/></w:rPr><m:t>y</m:t></m:r></m:den></m:f></m:oMath>"#;
        let root=parse(xml).unwrap();let original=root.clone();
        for display in [false,true] {
            let formatted=mathml_formatted_tree(&root,display,12.,None).unwrap();let fraction=formatted.child("oMath").unwrap().child("f").unwrap();
            assert_eq!(fraction.mathml_format,Some(MathMlFormat{points:24.,color:Some([0xCC,0x33,0x66])}));
            let points=16.*crate::math_layout::script_scale(if display{0}else{1});
            assert_eq!(fraction.child("num").unwrap().child("r").unwrap().mathml_format,Some(MathMlFormat{points,color:Some([0x33,0x66,0x99])}));
            assert_eq!(fraction.child("den").unwrap().child("r").unwrap().mathml_format,Some(MathMlFormat{points,color:None}));
            let rendered=to_mathml(&root,display,"");assert!(rendered.contains("mathcolor=\"#CC3366\" mathsize=\"24pt\"><mfrac>"));assert!(rendered.contains("mathcolor=\"CanvasText\""));
        }
        assert_eq!(root,original); // formatting never mutates author/render input
    }

    #[test]
    fn mathml_office_nested_fractions_roots_and_arguments_match_script_levels() {
        let run=|text:&str|format!(r#"<m:r><w:rPr><w:sz w:val="32"/></w:rPr><m:t>{text}</m:t></m:r>"#);
        let xml=format!("<m:oMath><m:f><m:num><m:f><m:num>{}</m:num><m:den>{}</m:den></m:f></m:num><m:den><m:argPr><m:argSz m:val=\"-1\"/></m:argPr>{}</m:den></m:f><m:rad><m:deg>{}</m:deg><m:e>{}</m:e></m:rad></m:oMath>",run("a"),run("b"),run("c"),run("d"),run("e"));
        let root=parse(&xml).unwrap();let formatted=mathml_formatted_tree(&root,false,12.,Some([1,2,3])).unwrap();
        fn runs(n:&Node,out:&mut Vec<(String,MathMlFormat)>){if n.name=="r"{out.push((n.run_text(),n.mathml_format.unwrap()));}for c in &n.children{if c.is_math()&&!c.is_props(){runs(c,out);}}}
        let mut actual=vec![];runs(&formatted,&mut actual);assert_eq!(actual.iter().map(|(s,_)|s.as_str()).collect::<Vec<_>>(),vec!["a","b","c","d","e"]);
        for (index,(_,format)) in actual.iter().enumerate(){assert_eq!(format.points,16.*crate::math_layout::script_scale(if index<4{2}else{0}));assert_eq!(format.color,Some([1,2,3]));}
        assert!(to_mathml(&root,false,"").contains("<mroot>"));
    }

    #[test]
    fn mathml_unformatted_math_keeps_native_reflow_and_context_defaults_are_bounded() {
        let plain=parse("<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>").unwrap();assert!(mathml_formatted_tree(&plain,false,12.,None).is_none());
        assert!(!to_mathml(&plain,false,"").contains("mathsize="));
        let mixed=parse(r#"<m:oMath><m:r><w:rPr><w:color w:val="CC3366"/></w:rPr><m:t>x</m:t></m:r><m:r><m:t>y</m:t></m:r></m:oMath>"#).unwrap();
        let output=to_mathml_with_context(&mixed,false,"",18.,Some([0x12,0x34,0x56]));assert!(output.contains("mathcolor=\"#CC3366\" mathsize=\"18pt\""));assert!(output.contains("mathcolor=\"#123456\" mathsize=\"18pt\""));
        for size in [f64::NAN,f64::INFINITY,-1.,0.]{assert!(to_mathml_with_context(&mixed,false,"",size,None).contains("mathsize=\"12pt\""));}
    }

    #[test]
    fn math_wrapping_defaults_switches_units_and_display_off_remain_distinct(){
        let get=|props:&str|math_wrap_style(&parse(&format!("<m:mathPr>{props}</m:mathPr><m:oMath/>" )).unwrap());
        let default=get("");assert_eq!(default.binary,MathBinaryBreak::Before);assert_eq!(default.subtraction,"--");assert_eq!(default.indent,72.);assert!(!default.right);
        assert_eq!(get("<m:brkBin/><m:brkBinSub/><m:wrapIndent/>"),default);
        let custom=get(r#"<m:brkBin m:val="repeat"/><m:brkBinSub m:val="-+"/><m:wrapIndent m:val="240"/>"#);assert_eq!(custom.binary,MathBinaryBreak::Repeat);assert_eq!(custom.subtraction,"-+");assert_eq!(custom.indent,12.);
        assert!(get("<m:wrapRight/>").right);assert!(!get(r#"<m:wrapRight m:val="0"/>"#).right);
        let off=get(r#"<m:brkBin m:val="after"/><m:dispDef m:val="0"/><m:wrapRight/>"#);assert_eq!(off.binary,MathBinaryBreak::After);assert_eq!(off.indent,0.);assert!(!off.right);
    }
    #[test]
    fn math_paragraph_defaults_units_local_precedence_and_explicit_off() {
        let ns=crate::math_document::MATH_NS;
        for (display_defaults,enabled) in [("",true),("<q:dispDef/>",true),(r#"<q:dispDef q:val="0"/>"#,false)]{
            let root=parse(&format!(r#"<q:mathPr xmlns:q="{ns}">{display_defaults}<q:defJc q:val="left"/><q:preSp q:val="120"/><q:postSp q:val="240"/><q:interSp q:val="360"/><q:intraSp q:val="480"/></q:mathPr><q:oMathPara xmlns:q="{ns}"><q:oMathParaPr><q:jc q:val="right"/></q:oMathParaPr><q:oMath><q:r><q:t>x</q:t></q:r></q:oMath></q:oMathPara>"#)).unwrap();
            let global=math_paragraph_style(&root,None);
            assert_eq!((global.before,global.after,global.inter,global.intra),(6.,12.,18.,24.));
            assert_eq!(global.alignment,if enabled{MathJustification::Left}else{MathJustification::CenterGroup});
            let local=math_paragraph_style(&root,root.child("oMathPara"));assert_eq!(local.alignment,MathJustification::Right);assert!(local.explicit_alignment);
            assert!(to_mathml(&root,true,"").contains("text-align:right"));
        }
        let root=parse("<m:oMathPara><m:oMathParaPr><m:jc/></m:oMathParaPr><m:oMath/></m:oMathPara>").unwrap();
        assert_eq!(math_paragraph_style(&root,root.child("oMathPara")).alignment,MathJustification::CenterGroup);
    }

    #[test]
    fn math_manual_breaks_are_top_level_namespace_aware_and_project_as_real_rows() {
        let ns=crate::math_document::MATH_NS;
        let root=parse(&format!(r#"<q:oMath xmlns:q="{ns}" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><q:r><q:rPr><q:brk/></q:rPr><q:t>a</q:t></q:r><q:r><w:rPr><w:brk/></w:rPr><q:t>b</q:t></q:r><q:box><q:boxPr><q:brk q:alnAt="2"/></q:boxPr><q:e><q:r><q:t>==</q:t></q:r></q:e></q:box></q:oMath>"#)).unwrap();
        let equation=root.child("oMath").unwrap();let lines=manual_math_lines(&equation.children);
        assert_eq!(lines.len(),2);assert_eq!(lines[0].0.len(),2);assert_eq!(lines[1].1,Some(2));
        let output=to_mathml(&root,true,"");assert_eq!(output.matches("<mtr").count(),2);assert!(output.contains("data-readmd-align-at=\"2\""));assert!(output.contains("displaystyle=\"true\""));
    }
    #[test]
    fn math_small_fraction_defaults_apply_only_to_display_arguments() {
        let ns = crate::math_document::MATH_NS;
        for (property, enabled) in [
            ("", false), ("<q:smallFrac/>", true),
            (r#"<q:smallFrac q:val="1"/>"#, true),
            (r#"<q:smallFrac q:val="true"/>"#, true),
            (r#"<q:smallFrac q:val="on"/>"#, true),
            (r#"<q:smallFrac q:val="0"/>"#, false),
            (r#"<q:smallFrac q:val="false"/>"#, false),
            (r#"<q:smallFrac q:val="off"/>"#, false),
        ] {
            for kind in ["bar", "noBar", "skw", "lin"] {
                let xml = format!(r#"<q:mathPr xmlns:q="{ns}">{property}</q:mathPr><q:oMath xmlns:q="{ns}"><q:f><q:fPr><q:type q:val="{kind}"/></q:fPr><q:num><q:r><q:t>a</q:t></q:r></q:num><q:den><q:r><q:t>b</q:t></q:r></q:den></q:f></q:oMath>"#);
                let root = parse(&xml).unwrap();
                for display in [false, true] {
                    let output = to_mathml(&root, display, "a/b");
                    let expected = if (display && enabled) || (!display && kind == "lin") { 2 } else { 0 };
                    assert_eq!(output.matches("scriptlevel=\"+1\"").count(), expected, "{kind} {display} {property}: {output}");
                    assert!(!output.contains("mathPr"));
                    let (word, settings) = docx_math_fragment(&xml, display).unwrap();
                    assert!(!word.contains("mathPr"));
                    assert!(settings.unwrap().xml().contains(property));
                }
                assert_eq!(root.math_defaults.small_fraction, if property.is_empty() { None } else { Some(enabled) });
            }
        }
    }

    #[test]
    fn math_nested_fraction_and_limits_use_their_actual_mathml_style() {
        let inner = "<m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f>";
        let xml = format!("<m:mathPr><m:smallFrac/></m:mathPr><m:oMath><m:f><m:num>{inner}</m:num><m:den><m:r><m:t>c</m:t></m:r></m:den></m:f></m:oMath>");
        let root = parse(&xml).unwrap();
        assert_eq!(to_mathml(&root, true, "").matches("scriptlevel=\"+1\"").count(), 2);
        assert_eq!(to_mathml(&root, false, "").matches("scriptlevel=\"+1\"").count(), 0);
        let sum = "<m:nary><m:naryPr><m:chr m:val=\"∑\"/></m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary>";
        let root = parse(&format!("<m:oMath><m:f><m:num>{sum}</m:num><m:den>{sum}</m:den></m:f>{sum}</m:oMath>")).unwrap();
        let output = to_mathml(&root, true, "");
        assert_eq!(output.matches("<msubsup>").count(), 2);
        assert_eq!(output.matches("<munderover>").count(), 1);
    }

    #[test]
    fn math_document_limit_defaults_are_inherited_and_local_overrides_win() {
        use super::*;
        for (character, key, default) in [("∫", "intLim", false), ("∑", "naryLim", true)] {
            for (property, expected) in [(format!("<q:{key} q:val=\"undOvr\"/>"), true), (format!("<q:{key} q:val=\"subSup\"/>"), false), (format!("<q:{key}/>"), default)] {
                for local in [None, Some(true), Some(false)] {
                    let local_property = local.map(|above| format!("<q:limLoc q:val=\"{}\"/>", if above {"undOvr"} else {"subSup"})).unwrap_or_default();
                    let xml = format!(r#"<q:mathPr xmlns:q="{ns}">{property}</q:mathPr><q:oMath xmlns:q="{ns}"><q:nary><q:naryPr><q:chr q:val="{character}"/>{local_property}</q:naryPr><q:sub><q:r><q:t>0</q:t></q:r></q:sub><q:sup><q:r><q:t>1</q:t></q:r></q:sup><q:e><q:r><q:t>x</q:t></q:r></q:e></q:nary></q:oMath>"#, ns=crate::math_document::MATH_NS);
                    let root = parse(&xml).unwrap();
                    let nary = root.child("oMath").unwrap().child("nary").unwrap();
                    for display in [false, true] {
                        let effective = local.unwrap_or(expected);
                        assert_eq!(nary_limits(nary, display), effective, "{xml}");
                        let output = to_mathml(&root, display, "");
                        assert!(output.contains(if effective {"<munderover>"} else {"<msubsup>"}), "{output}");
                        assert!(output.contains("movablelimits=\"false\""));
                        let (word, settings) = docx_math_fragment(&xml, display).unwrap();
                        assert!(!word.contains("mathPr")); assert!(settings.unwrap().xml().contains(key));
                        let standalone = parse(&word).unwrap();
                        let paragraph = standalone.child("oMathPara").unwrap_or(&standalone);
                        assert_eq!(nary_limits(paragraph.child("oMath").unwrap().child("nary").unwrap(), display), effective);
                    }
                    // Resolving display/export must not normalize the source.
                    assert!(crate::math_document::parse_xml(&xml).unwrap().xml().contains(&local_property));
                }
            }
        }
    }
    use super::*;

    #[test]
    fn nary_explicit_limits_override_inline_style_and_defaults_distinguish_integrals() {
        for (character,property,inline,display,movable) in [
            ("∑","",false,true,true),("∏","",false,true,true),
            ("∫","",false,false,true),("∮","",false,false,true),
            ("⨜","",false,false,true),("⋃","",false,true,true),
            ("∑",r#"<m:limLoc m:val="subSup"/>"#,false,false,false),
            ("∑",r#"<m:limLoc m:val="undOvr"/>"#,true,true,false),
            ("∫",r#"<m:limLoc m:val="undOvr"/>"#,true,true,false),
            ("∑","<m:limLoc/>",true,true,false),
        ] {
            let root=parse(&format!(r#"<m:oMath><m:nary><m:naryPr><m:chr m:val="{character}"/>{property}</m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#)).unwrap();
            let n=root.child("oMath").unwrap().child("nary").unwrap();
            assert_eq!(nary_limits(n,false),inline,"{character} {property}");
            assert_eq!(nary_limits(n,true),display,"{character} {property}");
            for (style, expected) in [(false, inline), (true, display)] {
                let mathml=to_mathml(&root,style,"");
                assert!(mathml.contains(if expected{"<munderover>"}else{"<msubsup>"}),"{mathml}");
                assert!(mathml.contains(&format!("movablelimits=\"{movable}\"")));
            }
        }
    }

    #[test]
    fn matrices_and_equation_arrays_share_row_gap_units_and_baseline_properties() {
        for (object,container,rows) in [
            ("m","mPr","<m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr>"),
            ("eqArr","eqArrPr","<m:e><m:r><m:t>x</m:t></m:r></m:e>"),
        ] {
            for (properties,expected) in [
                ("","1em"),(r#"<m:rSp m:val="42"/>"#,"1em"),
                (r#"<m:rSpRule m:val="1"/>"#,"1.5em"),(r#"<m:rSpRule m:val="2"/>"#,"2em"),
                (r#"<m:rSpRule m:val="3"/><m:rSp m:val="12"/>"#,"12pt"),
                (r#"<m:rSpRule m:val="4"/><m:rSp m:val="3"/>"#,"1.5em"),
                (r#"<m:rSpRule m:val="3"/>"#,"0pt"),(r#"<m:rSpRule m:val="4"/>"#,"0em"),
            ] {
                let root=parse(&format!(r#"<m:oMath><m:{object}><m:{container}><m:baseJc m:val="top"/>{properties}</m:{container}>{rows}</m:{object}></m:oMath>"#)).unwrap();
                let output=to_mathml(&root,true,"");
                assert!(output.contains(&format!("rowspacing=\"{expected}\"")),"{output}");
                assert!(output.contains("align=\"baseline 1\""));
            }
        }
    }

    #[test]
    fn delimiter_growth_shape_and_explicit_empty_characters_reach_mathml() {
        let render=|properties:&str| {
            let xml=format!(r#"<m:oMath><m:d><m:dPr>{properties}</m:dPr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:d></m:oMath>"#);
            to_mathml(&parse(&xml).unwrap(),true,"")
        };
        let omitted=render("");
        assert!(omitted.contains("stretchy=\"true\" symmetric=\"true\">("));
        assert!(omitted.contains("symmetric=\"true\">│"));
        for off in ["0","false","off"] {
            let fixed=render(&format!(r#"<m:grow m:val="{off}"/><m:shp m:val="match"/>"#));
            assert_eq!(fixed.matches("stretchy=\"false\" symmetric=\"false\"").count(),3);
        }
        assert_eq!(render(r#"<m:grow/><m:shp m:val="match"/>"#).matches("stretchy=\"true\" symmetric=\"false\"").count(),3);
        for empty in [r#"<m:begChr/><m:sepChr/><m:endChr/>"#,r#"<m:begChr m:val=""/><m:sepChr m:val=""/><m:endChr m:val=""/>"#] {
            let none=render(empty);
            assert!(!none.contains("<mo"));
            assert!(none.contains("<mi>x</mi>")&&none.contains("<mi>y</mi>"));
        }
    }

    #[test]
    fn nary_growth_has_its_own_default_and_never_reintroduces_empty_operators() {
        for (properties,grow,operator) in [
            ("",false,"∫"),("<m:grow/>",true,"∫"),
            (r#"<m:grow m:val="0"/>"#,false,"∫"),
            (r#"<m:grow m:val="true"/>"#,true,"∫"),
            ("<m:chr/>",false,""),(r#"<m:chr m:val=""/>"#,false,""),
        ] {
            let root=parse(&format!("<m:oMath><m:nary><m:naryPr>{properties}</m:naryPr><m:sub/><m:sup/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>")).unwrap();
            let output=to_mathml(&root,true,"");
            assert!(output.contains(&format!("stretchy=\"{grow}\">{operator}</mo>")));
            assert!(output.contains("<mrow><mo"));
            if operator.is_empty(){assert!(!output.contains('∫'));}
        }
    }

    #[test]
    fn matrix_column_gaps_follow_office_rules_in_mathml_and_preserve_units(){
        for (props,expected) in [
            ("","1em"),(r#"<m:cGp m:val="600"/>"#,"1em"),
            (r#"<m:cGpRule m:val="0"/><m:cGp m:val="600"/>"#,"1em"),
            (r#"<m:cGpRule m:val="1"/>"#,"1.5em"),(r#"<m:cGpRule m:val="2"/>"#,"2em"),
            (r#"<m:cGpRule m:val="3"/><m:cGp m:val="25"/>"#,"1.25pt"),
            (r#"<m:cGpRule m:val="4"/><m:cGp m:val="6"/>"#,"3em"),
            (r#"<m:cGpRule m:val="3"/>"#,"0pt"),(r#"<m:cGpRule m:val="4"/>"#,"0em"),
        ] {
            let root=parse(&format!(r#"<m:oMath><m:m><m:mPr>{props}</m:mPr><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#)).unwrap();
            assert!(to_mathml(&root,true,"").contains(&format!("columnspacing=\"{expected}\"")),"{props}");
        }
    }

    #[test]
    fn parses_latex2omml_output() {
        let root = from_latex(r"\frac{a}{b} + x^2", false).unwrap();
        let math = root.child("oMath").unwrap();
        assert!(math.child("f").is_some());
        assert!(math.child("sSup").is_some());
    }

    #[test]
    fn entities_and_attributes() {
        let root = parse("<m:r><m:t>a &amp; b &lt; &#x3b1;</m:t></m:r><m:chr m:val=\"&amp;\"/>").unwrap();
        assert_eq!(root.children[0].run_text(), "a & b < α");
        assert_eq!(root.children[1].attr("val"), Some("&"));
    }

    #[test]
    fn argument_sizes_project_as_relative_script_levels_without_namespace_impostors() {
        let xml = format!(r#"<m:oMath xmlns:m="{}" xmlns:x="urn:extension"><m:box><x:boxPr/><m:e><m:argPr><x:argSz x:val="2"/><m:argSz m:val="-1"/></m:argPr><m:r><x:t>hidden-extension</x:t><m:t>x</m:t></m:r></m:e></m:box></m:oMath>"#,crate::math_document::MATH_NS);
        let root=parse(&xml).unwrap();
        let e=root.child("oMath").unwrap().child("box").unwrap().child("e").unwrap();
        assert_eq!(argument_size(e),-1);
        assert_eq!(e.child("r").unwrap().run_text(),"x");
        let output=to_mathml(&root,false,"x");
        assert!(output.contains("<mstyle scriptlevel=\"+1\">"));
        assert!(!output.contains("hidden-extension"));
        for value in [-2,-1,0,1,2] {
            let changed=parse(&xml.replace("m:val=\"-1\"",&format!("m:val=\"{value}\""))).unwrap();
            let output=to_mathml(&changed,false,"x");
            if value==0 { assert!(!output.contains("scriptlevel=")); }
            else { assert!(output.contains(&format!("scriptlevel=\"{:+}\"",-value))); }
        }
    }

    #[test]
    fn namespace_aliases_and_shadowed_prefixes_do_not_change_run_semantics() {
        use crate::math_document::{MATH_NS,WORD_NS};
        for (math,word) in [("m","w"),("equation","font"),("w","m")] {
            let xml=format!("<{math}:oMath xmlns:{math}=\"{MATH_NS}\" xmlns:{word}=\"{WORD_NS}\"><{math}:r><{word}:rPr><{word}:b/><{word}:i {word}:val=\"0\"/></{word}:rPr><{math}:rPr><{math}:sty {math}:val=\"bi\"/></{math}:rPr><{math}:t>x</{math}:t></{math}:r></{math}:oMath>");
            let root=parse(&xml).unwrap();let run=root.child("oMath").unwrap().child("r").unwrap();
            let style=run_style(run);assert!(style.bold&&!style.upright);assert_eq!(run.child("rPr").unwrap().namespace,MATH_NS);
            assert!(to_mathml(&root,false,"x").contains("mathvariant=\"bold-italic\""));
        }
        let xml=format!("<oMath xmlns=\"{MATH_NS}\"><r><rPr xmlns=\"{WORD_NS}\"><b/><i val=\"0\"/></rPr><t>x</t></r></oMath>");
        let root=parse(&xml).unwrap();let run=root.child("oMath").unwrap().child("r").unwrap();
        assert!(run_style(run).bold&&run_style(run).upright);assert!(run.child("rPr").is_none());
    }

    #[test]
    fn shared_xml_reader_preserves_cdata_quoted_greater_than_and_rejects_entities() {
        let root=parse("<m:r label=\"a > b\"><m:t><![CDATA[x < y & z]]>&#x1d6c2;</m:t></m:r>").unwrap();
        assert_eq!(root.children[0].attr("label"),Some("a > b"));
        assert_eq!(root.children[0].run_text(),"x < y & z𝛂");
        assert!(parse("<m:r><m:t>&unknown;</m:t></m:r>").is_err());
        assert!(parse("<!DOCTYPE r [<!ENTITY x SYSTEM 'private'>]><m:r/>").is_err());
        assert!(parse("<m:r><q:t>x</m:t></m:r>").is_err());
    }

    #[test]
    fn rejects_broken_xml() {
        assert!(parse("<m:r><m:t>x</m:r>").is_err());
        assert!(parse("<m:r>").is_err());
        assert!(parse("</m:r>").is_err());
        let deep = "<m:e>".repeat(MAX_DEPTH + 5) + &"</m:e>".repeat(MAX_DEPTH + 5);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn mathml_covers_common_constructs() {
        let m = latex_to_mathml(r"\sum_{i=1}^{n} \frac{\sqrt{x}}{\alpha_i} \le \left( \begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix} \right)", true).unwrap();
        for tag in ["<munderover>", "<mfrac>", "<msqrt>", "<msub>", "<mtable ", "<mo>≤</mo>", "display=\"block\""] {
            assert!(m.contains(tag), "{tag} missing in {m}");
        }
        let m = latex_to_mathml(r"\sin x + \text{中文} + 3.14", false).unwrap();
        assert!(m.contains("<mi mathvariant=\"normal\">sin</mi>"), "{m}");
        assert!(m.contains("<mtext style=\"font-family:'STIX Two Math';\">中文</mtext>"), "{m}");
        assert!(m.contains("<mn>3.14</mn>"), "{m}");
    }

    #[test]
    fn delimiters_normalise() {
        assert_eq!(delimiter_char("\\{"), "{");
        assert_eq!(delimiter_char("\\|"), "‖");
        assert_eq!(delimiter_char("."), "");
        assert_eq!(delimiter_char("\\langle"), "⟨");
    }

    #[test]
    fn never_panics_on_odd_formulas() {
        for s in ["", "{", "}", "^", "_", "\\frac", "\\sqrt[", "\\left(", "\\begin{matrix}", "a^^b", "\\\\", "&&", "中文"] {
            let _ = latex_to_mathml(s, false);
            let _ = latex_to_mathml(s, true);
        }
    }
}
