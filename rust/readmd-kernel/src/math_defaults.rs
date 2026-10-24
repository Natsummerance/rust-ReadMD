//! Author declarations and effective Office Math defaults are different views.
//!
//! The declaration view never rewrites OMML/DSL. DOCX has only one mathPr, so
//! output combines effective document-wide requirements and localizes only
//! properties with an exact native equivalent (paragraph jc and operator
//! limLoc, supplied by omml::docx_math_fragment). A conflict is an error before
//! the writer exposes package parts, rather than a warning on a changed file.

use crate::math_document::{self, XmlNode, MATH_NS};
use crate::omml::{MathBinaryBreak, MathJustification};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const PROPERTIES: [&str; 16] = [
    "mathFont", "brkBin", "brkBinSub", "smallFrac", "dispDef", "lMargin",
    "rMargin", "defJc", "preSp", "postSp", "interSp", "intraSp",
    "wrapIndent", "wrapRight", "intLim", "naryLim",
];

/// An empty element is not an absent element. Explicit strings retain boolean
/// spellings and numeric source representation; only the effective view folds
/// them. The existing lossless tree remains the authoring source of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Declared<T> {
    Missing,
    Implicit,
    Explicit(T),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectiveDefaults {
    // No font name is invented for Office's implicit choice. A named font and
    // an unspecified native font cannot be assumed equivalent across apps.
    pub math_font: Option<String>,
    pub integral_limits: bool,
    pub nary_limits: bool,
    pub small_fraction: bool,
    // False retains paragraph inheritance. The following disabled margin/wrap
    // numbers are the existing renderer's neutral projection, not a claim
    // that an unknown native paragraph actually has explicit zero settings.
    pub display_defaults: bool,
    pub alignment: MathJustification,
    pub pre_spacing: u16,
    pub post_spacing: u16,
    pub inter_spacing: u16,
    pub intra_spacing: u16,
    pub left_margin: u16,
    pub right_margin: u16,
    pub wrap_indent: u16,
    pub wrap_right: bool,
    pub binary_break: MathBinaryBreak,
    pub subtraction: &'static str,
}

impl EffectiveDefaults {
    /// Word has one policy for all instances of mathematical text, including
    /// inline wrapping. dispDef=false is paragraph inheritance, not an explicit
    /// all-zero layout. Keep that inheritance mode distinct during merging.
    /// See the BreakBinary/WrapIndent/WrapRight SDK remarks and MS-OE376
    /// 2.1.1820; none establishes an inline exemption for these policies.
    pub(crate) fn global_difference(&self, other: &Self) -> Option<&'static str> {
        if self.math_font != other.math_font { return Some("mathFont"); }
        if self.display_defaults != other.display_defaults { return Some("dispDef"); }
        for (property, first, second) in [
            ("lMargin", self.left_margin, other.left_margin),
            ("rMargin", self.right_margin, other.right_margin),
            ("wrapIndent", self.wrap_indent, other.wrap_indent),
        ] {
            if first != second { return Some(property); }
        }
        if self.wrap_right != other.wrap_right { return Some("wrapRight"); }
        if self.binary_break != other.binary_break { return Some("brkBin"); }
        if self.subtraction != other.subtraction { return Some("brkBinSub"); }
        None
    }

    /// These properties explicitly govern display math/paragraphs. The native
    /// per-paragraph jc and per-operator limLoc need no common global value.
    pub(crate) fn display_difference(&self, other: &Self) -> Option<&'static str> {
        if self.small_fraction != other.small_fraction { return Some("smallFrac"); }
        for (property, first, second) in [
            ("preSp", self.pre_spacing, other.pre_spacing),
            ("postSp", self.post_spacing, other.post_spacing),
            ("interSp", self.inter_spacing, other.inter_spacing),
            ("intraSp", self.intra_spacing, other.intra_spacing),
        ] {
            if first != second { return Some(property); }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MathDefaultsState {
    pub present: bool,
    pub declared: BTreeMap<&'static str, Declared<String>>,
    pub effective: EffectiveDefaults,
}

fn math(node: &XmlNode, name: &str) -> bool {
    node.namespace == MATH_NS && node.local() == name
}

fn checked_value(node: &XmlNode) -> Result<Option<&str>, &'static str> {
    let definition = math_document::schema()["elements"].as_array()
        .and_then(|elements| elements.iter().find(|element| element["element"] == node.local()))
        .ok_or("math_invalid_property")?;
    let mut expanded = BTreeSet::new();
    let mut result = None;
    for (name, value) in &node.attributes {
        if name == "xmlns" || name.starts_with("xmlns:") { continue; }
        let local = name.rsplit(':').next().unwrap_or(name);
        let namespace = node.attribute_namespaces.get(name).map(String::as_str).unwrap_or("");
        if namespace != MATH_NS || !name.contains(':') {
            // A val in no namespace or a forged namespace is never m:val.
            // Opaque foreign data remains in author source, but cannot be
            // promoted to one shared native mathPr with unverified semantics.
            return Err(if namespace.is_empty() || local == "val" {
                "math_invalid_property"
            } else { "math_unsupported_extension" });
        }
        if !expanded.insert((namespace, local)) { return Err("math_invalid_property"); }
        let attribute = definition["attributes"].as_array()
            .and_then(|attributes| attributes.iter().find(|attribute|
                attribute["name"].as_str().and_then(|name| name.strip_prefix("m:")) == Some(local)))
            .ok_or("math_invalid_property")?;
        math_document::validate_constraints(attribute, value)?;
        if local == "val" {
            math_document::validate_value(node.local(), value)?;
            result = Some(value.as_str());
        }
    }
    Ok(result)
}

/// The fixed schema's unsigned value space includes equivalent integer
/// spellings such as +0 and -0. Keep the spelling in source, compare twips.
pub(crate) fn bounded_twips(value: &str) -> Option<u16> {
    value.parse::<i64>().ok().filter(|value| (0..=31680).contains(value)).map(|value| value as u16)
}

/// Documented empty-element defaults are emitted explicitly where the pinned
/// SDK requires a native value. This is not a global relaxation of required
/// attributes: mathFont and all four spacing fields have no empty default.
pub(crate) fn native_implicit_value(property: &str) -> Option<&'static str> {
    math_document::office_implicit_value(property, "m:val")
}

impl MathDefaultsState {
    pub(crate) fn from_xml(root: &XmlNode) -> Result<Self, &'static str> {
        // Only the transparent formula fragment may carry global mathPr.
        // A direct mathPr is also valid for inspecting extracted settings.
        // Legacy/native XML does not pass the DSL particle validator, so do
        // not let a nested declaration bypass its URI/value/schema guard.
        let fragment = root.name == "#root" && root.namespace.is_empty();
        let mut pending = vec![(root, 0usize)];
        while let Some((node, depth)) = pending.pop() {
            if node.local() == "mathPr"
                && (node.namespace != MATH_NS || !(depth == 0 || fragment && depth == 1))
            { return Err("math_invalid_property"); }
            pending.extend(node.children().map(|child| (child, depth + 1)));
        }
        let mut containers = root.children().filter(|node| math(node, "mathPr"));
        let container = if math(root, "mathPr") { Some(root) } else { containers.next() };
        if containers.next().is_some() { return Err("math_invalid_property"); }
        let mut declared = PROPERTIES.into_iter().map(|key| (key, Declared::Missing))
            .collect::<BTreeMap<_, _>>();
        if let Some(container) = container {
            checked_value(container)?;
            if !container.text().trim().is_empty() { return Err("math_invalid_property"); }
            for node in container.children() {
                if node.namespace != MATH_NS {
                    return Err(if PROPERTIES.contains(&node.local()) { "math_invalid_property" }
                        else { "math_unsupported_extension" });
                }
                let key = PROPERTIES.into_iter().find(|key| *key == node.local())
                    .ok_or("math_invalid_property")?;
                if !matches!(declared[key], Declared::Missing) { return Err("math_invalid_property"); }
                if node.children().next().is_some() || !node.text().trim().is_empty() {
                    return Err("math_invalid_property");
                }
                let state = if let Some(value) = checked_value(node)? {
                    Declared::Explicit(value.to_string())
                } else {
                    if matches!(key, "mathFont" | "preSp" | "postSp" | "interSp" | "intraSp") {
                        return Err("math_invalid_property");
                    }
                    Declared::Implicit
                };
                declared.insert(key, state);
            }
            // Reuse the fixed schema's existing particle/required validator.
            // It expects canonical attribute spellings; the authoring tree and
            // its real namespace aliases remain untouched by this copy.
            let mut validation = container.clone();
            for part in &mut validation.content {
                if let crate::math_document::XmlPart::Element(node) = part {
                    let value = checked_value(node)?.map(str::to_owned);
                    node.attributes.retain(|name, _| name == "xmlns" || name.starts_with("xmlns:"));
                    node.attribute_namespaces.clear();
                    if let Some(value) = value {
                        node.attributes.insert("m:val".into(), value);
                        node.attribute_namespaces.insert("m:val".into(), MATH_NS.into());
                    }
                }
            }
            crate::math_dsl::validate_tree(&validation)?;
        }
        let explicit = |key| match &declared[key] {
            Declared::Explicit(value) => Some(value.as_str()), _ => None,
        };
        let boolean = |key, missing| match &declared[key] {
            Declared::Missing => missing,
            Declared::Implicit => true,
            Declared::Explicit(value) => crate::omml::on_off(Some(value), true),
        };
        let twips = |key, missing, implicit| match &declared[key] {
            Declared::Missing => missing,
            Declared::Implicit => implicit,
            Declared::Explicit(value) => bounded_twips(value).expect("validated bounded twips"),
        };
        let enabled = boolean("dispDef", true);
        let display_twips = |key, missing, implicit| if enabled { twips(key, missing, implicit) } else { 0 };
        let effective = EffectiveDefaults {
            math_font: explicit("mathFont").map(str::to_owned),
            integral_limits: explicit("intLim").unwrap_or("subSup") == "undOvr",
            nary_limits: explicit("naryLim").unwrap_or("undOvr") == "undOvr",
            small_fraction: boolean("smallFrac", false),
            display_defaults: enabled,
            alignment: if enabled { MathJustification::parse(explicit("defJc").unwrap_or("centerGroup")) }
                else { MathJustification::CenterGroup },
            // MS-OE376 2.1.1820 lists exactly five dispDef-gated elements:
            // defJc, lMargin, rMargin, wrapRight, wrapIndent. Not spacing.
            pre_spacing: twips("preSp", 0, 0),
            post_spacing: twips("postSp", 0, 0),
            inter_spacing: twips("interSp", 0, 0),
            intra_spacing: twips("intraSp", 0, 0),
            left_margin: display_twips("lMargin", 0, 1440),
            right_margin: display_twips("rMargin", 0, 1440),
            wrap_indent: display_twips("wrapIndent", 1440, 1440),
            wrap_right: enabled && boolean("wrapRight", false),
            binary_break: match explicit("brkBin") {
                Some("after") => MathBinaryBreak::After,
                Some("repeat") => MathBinaryBreak::Repeat,
                _ => MathBinaryBreak::Before,
            },
            subtraction: match explicit("brkBinSub") {
                Some("-+") => "-+", Some("+-") => "+-", _ => "--",
            },
        };
        Ok(Self { present: container.is_some(), declared, effective })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DefaultsError {
    Source(String),
    Conflict { property: &'static str, first_formula: usize, formula: usize },
}

impl DefaultsError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Source(code) => code.clone(),
            Self::Conflict { property, first_formula, formula } => format!(
                "math_document_defaults_conflict: {}",
                serde_json::json!({"property":property,"firstFormula":first_formula,"formula":formula}),
            ),
        }
    }
}

/// One merge instance belongs to one DOCX build. Inline formulas do not impose
/// display-only requirements. Document-wide wrapping and font policies still
/// cannot be silently replaced merely because this renderer fits an inline run.
#[derive(Debug, Default)]
pub(crate) struct DocxDefaultsMerge {
    count: usize,
    global: Option<(EffectiveDefaults, usize)>,
    display: Option<(EffectiveDefaults, usize)>,
    // The outer Option distinguishes the first unconfigured source from a
    // not-yet-seen source. The first display source supersedes inline-only
    // settings, since inline math never consumed their paragraph defaults.
    settings: Option<Option<XmlNode>>,
}

impl DocxDefaultsMerge {
    pub(crate) fn accept(&mut self, xml: &str, display: bool) -> Result<String, DefaultsError> {
        self.accept_projected(xml,display).map(|(content,_)|content)
    }
    pub(crate) fn accept_projected(&mut self,xml:&str,display:bool)
        ->Result<(String,Option<crate::omml::DocxMathParagraph>),DefaultsError>
    {
        self.accept_native_projected(xml,display).map(|(content,paragraph,_)|(content,paragraph))
    }
    pub(crate) fn accept_native_projected(&mut self,xml:&str,display:bool)
        ->Result<(String,Option<crate::omml::DocxMathParagraph>,Option<crate::math_word::NativeParagraphSource>),DefaultsError>
    {
        let source = math_document::parse_xml(xml).map_err(|code| DefaultsError::Source(code.into()))?;
        if !display {
            let mut pending = vec![&source];
            while let Some(node) = pending.pop() {
                if math(node, "oMathPara") { return Err(DefaultsError::Source("math_invalid_source".into())); }
                pending.extend(node.children());
            }
        }
        let state = MathDefaultsState::from_xml(&source).map_err(|code| DefaultsError::Source(code.into()))?;
        let projected=crate::omml::docx_math_projection_tree(source,display).map_err(DefaultsError::Source)?;
        let crate::omml::DocxMathProjection{content,settings,paragraph,paragraph_source}=projected;
        let formula = self.count.checked_add(1).ok_or_else(|| DefaultsError::Source("math_budget".into()))?;
        if let Some((first, first_formula)) = &self.global {
            if let Some(property) = first.global_difference(&state.effective) {
                return Err(DefaultsError::Conflict { property, first_formula: *first_formula, formula });
            }
        }
        if display {
            if let Some((first, first_formula)) = &self.display {
                if let Some(property) = first.display_difference(&state.effective) {
                    return Err(DefaultsError::Conflict { property, first_formula: *first_formula, formula });
                }
            }
        }
        if self.settings.is_none() || (display && self.display.is_none()) {
            self.settings = Some(settings);
        }
        self.global.get_or_insert_with(|| (state.effective.clone(), formula));
        if display { self.display.get_or_insert_with(|| (state.effective, formula)); }
        self.count = formula;
        Ok((content,paragraph,paragraph_source))
    }

    pub(crate) fn settings(&self) -> Option<&XmlNode> {
        self.settings.as_ref().and_then(Option::as_ref)
    }
}

#[cfg(test)]
#[path = "math_defaults_tests.rs"]
mod tests;
