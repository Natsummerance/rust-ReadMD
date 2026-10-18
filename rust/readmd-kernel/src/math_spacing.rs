//! A bounded, nonpainting MathML space inside the shared lossless tree.
//! This is not an Office element. Native OMML/TeX projection must explicitly
//! reject it instead of substituting a character or exporting foreign XML.
use crate::math_document::{XmlNode, XmlPart};
use std::collections::BTreeMap;

pub const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";
/// Physical limits shared by mathematical preview and output guards.
pub const MAX_SPACE_POINTS: f64 = 20_000.;
pub(crate) const SOURCE_ID: &str = "readmdMathmlSourceId";
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceUnit {
    Em,
    Ex,
    Points,
    Pixels,
    Inches,
    Centimeters,
    Millimeters,
    Picas,
    Zero,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpaceLength {
    /// Keep lexical units, explicit zero and surrounding author whitespace.
    pub source: String,
    pub value: f64,
    pub unit: SpaceUnit,
}
impl SpaceLength {
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > 128 || source.chars().any(char::is_control) {
            return Err("math_invalid_property");
        }
        let value = source.trim();
        let bytes = value.as_bytes();
        let mut end = 0;
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            end += 1;
        }
        let first = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        let mut digits = end - first;
        if bytes.get(end) == Some(&b'.') {
            end += 1;
            let first = end;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end == first {
                return Err("math_invalid_property");
            }
            digits += end - first;
        }
        if digits == 0 {
            // Recognized CSS/legacy MathML expressions are outside this
            // bounded unit subset, not fabricated invalid numeric input.
            if value.starts_with("calc(")
                || value.starts_with("var(")
                || value.ends_with("mathspace")
            {
                return Err("math_unsupported_extension");
            }
            return Err("math_invalid_property");
        }
        if matches!(bytes.get(end), Some(b'e' | b'E')) {
            let mut exponent = end + 1;
            if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
                exponent += 1;
            }
            let first = exponent;
            while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                exponent += 1;
            }
            if exponent > first {
                end = exponent;
            }
        }
        let number = value[..end]
            .parse::<f64>()
            .map_err(|_| "math_invalid_property")?;
        if !number.is_finite() {
            return Err("math_invalid_property");
        }
        let unit = match value[end..].to_ascii_lowercase().as_str() {
            "em" => SpaceUnit::Em,
            "ex" => SpaceUnit::Ex,
            "pt" => SpaceUnit::Points,
            "px" => SpaceUnit::Pixels,
            "in" => SpaceUnit::Inches,
            "cm" => SpaceUnit::Centimeters,
            "mm" => SpaceUnit::Millimeters,
            "pc" => SpaceUnit::Picas,
            "" if number == 0. => SpaceUnit::Zero,
            "%" | "rem" | "ch" | "cap" | "ic" | "lh" | "rlh" | "vw" | "vh" | "vi" | "vb"
            | "vmin" | "vmax" | "q" => return Err("math_unsupported_extension"),
            "" => return Err("math_unsupported_extension"),
            _ => return Err("math_invalid_property"),
        };
        // Signed lengths are syntactically valid in MathML. Core defines
        // additional ascent/total-height rules which this first unit does not
        // yet implement; report unsupported, never silently clamp or abs().
        if number < 0. {
            return Err("math_unsupported_extension");
        }
        if number > 4096. {
            return Err("math_budget");
        }
        Ok(Self {
            source: source.into(),
            value: number,
            unit,
        })
    }
    pub fn points(&self, em_points: f64, x_height_em: f64) -> Result<f64> {
        if !em_points.is_finite()
            || em_points <= 0.
            || !x_height_em.is_finite()
            || x_height_em <= 0.
        {
            return Err("math_invalid_property");
        }
        let factor = match self.unit {
            SpaceUnit::Em => em_points,
            SpaceUnit::Ex => em_points * x_height_em,
            SpaceUnit::Points => 1.,
            SpaceUnit::Pixels => 0.75,
            SpaceUnit::Inches => 72.,
            SpaceUnit::Centimeters => 72. / 2.54,
            SpaceUnit::Millimeters => 72. / 25.4,
            SpaceUnit::Picas => 12.,
            SpaceUnit::Zero => 0.,
        };
        let result = self.value * factor;
        if !result.is_finite() || result > MAX_SPACE_POINTS {
            return Err("math_budget");
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spacing {
    pub width: Option<SpaceLength>,
    pub height: Option<SpaceLength>,
    pub depth: Option<SpaceLength>,
    pub math_size: Option<SpaceLength>,
}
impl Spacing {
    pub fn from_attributes<'a>(
        attributes: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self> {
        let mut result = Self::default();
        for (key, value) in attributes {
            let target = match key {
                "width" => &mut result.width,
                "height" => &mut result.height,
                "depth" => &mut result.depth,
                "mathsize" => &mut result.math_size,
                "xmlns" | "readmdId" => continue,
                key if key.starts_with("xmlns:") => continue,
                _ => return Err("math_unsupported_extension"),
            };
            *target = Some(SpaceLength::parse(value)?);
        }
        // Zero CSS font size is valid but needs a separate zero-font geometry
        // path; this bounded subset does not mislabel it as invalid input.
        if result
            .math_size
            .as_ref()
            .is_some_and(|size| size.value == 0.)
        {
            return Err("math_unsupported_extension");
        }
        // Absolute lengths already have a context-independent physical size.
        // Reject them before a serializer could feed an enormous CSS box to
        // a browser; relative lengths are checked at the actual render size.
        for length in [
            &result.width,
            &result.height,
            &result.depth,
            &result.math_size,
        ]
        .into_iter()
        .flatten()
        {
            if !matches!(length.unit, SpaceUnit::Em | SpaceUnit::Ex) {
                length.points(1., 0.5)?;
            }
        }
        let physical = |value: &Option<SpaceLength>| match value {
            None => Some(0.),
            Some(value) if !matches!(value.unit, SpaceUnit::Em | SpaceUnit::Ex) => {
                value.points(1., 0.5).ok()
            }
            _ => None,
        };
        if physical(&result.height)
            .zip(physical(&result.depth))
            .is_some_and(|(height, depth)| height + depth > MAX_SPACE_POINTS)
        {
            return Err("math_budget");
        }
        Ok(result)
    }
    pub fn from_xml(node: &XmlNode) -> Result<Self> {
        if !is_spacing(node) {
            return Err("math_unsupported_extension");
        }
        if node
            .content
            .iter()
            .any(|part| !matches!(part,XmlPart::Text(value) if value.trim().is_empty()))
        {
            return Err("math_unsupported_extension");
        }
        for (key, ns) in &node.attribute_namespaces {
            if !key.starts_with("xmlns") && !ns.is_empty() {
                return Err("math_unsupported_extension");
            }
        }
        Self::from_attributes(
            node.attributes
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
    }
    pub fn node(&self) -> XmlNode {
        let mut attributes = BTreeMap::from([("xmlns:readmdMathml".into(), MATHML_NS.into())]);
        for (key, value) in self.attributes() {
            attributes.insert(key.into(), value.into());
        }
        XmlNode {
            id: String::new(),
            name: "readmdMathml:mspace".into(),
            namespace: MATHML_NS.into(),
            attributes,
            attribute_namespaces: BTreeMap::new(),
            content: Vec::new(),
        }
    }
    pub fn attributes(&self) -> Vec<(&'static str, &str)> {
        [
            ("width", &self.width),
            ("height", &self.height),
            ("depth", &self.depth),
            ("mathsize", &self.math_size),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.as_ref().map(|length| (key, length.source.as_str())))
        .collect()
    }
    pub fn dimensions(&self, em_points: f64, x_height_em: f64) -> Result<(f64, f64, f64)> {
        let em = match &self.math_size {
            Some(size) => size.points(em_points, x_height_em)?,
            None => em_points,
        };
        if em <= 0. {
            return Err("math_invalid_property");
        }
        let resolve = |length: &Option<SpaceLength>| {
            length
                .as_ref()
                .map_or(Ok(0.), |value| value.points(em, x_height_em))
        };
        let (width, height, depth) = (
            resolve(&self.width)?,
            resolve(&self.height)?,
            resolve(&self.depth)?,
        );
        if height + depth > MAX_SPACE_POINTS {
            return Err("math_budget");
        }
        Ok((width, height, depth))
    }
    pub fn dsl(&self) -> String {
        format!(
            "space({})",
            self.attributes()
                .into_iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    if key == "mathsize" { "size" } else { key },
                    serde_json::to_string(value).unwrap()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
    /// Explicit dimension syntax for internal annotation/edit projections.
    /// Portable TeX exporters reject this node until that backend is measured;
    /// it must never degenerate into a visible text space on a failed render.
    pub fn tex_annotation(&self) -> String {
        let tex = |length: &Option<SpaceLength>| {
            length
                .as_ref()
                .map(|value| {
                    let number = match value.unit {
                        SpaceUnit::Pixels => value.value * 0.75,
                        SpaceUnit::Points => value.value,
                        _ => value.value,
                    };
                    let unit = match value.unit {
                        SpaceUnit::Em => "em",
                        SpaceUnit::Ex => "ex",
                        SpaceUnit::Points | SpaceUnit::Pixels => "bp",
                        SpaceUnit::Inches => "in",
                        SpaceUnit::Centimeters => "cm",
                        SpaceUnit::Millimeters => "mm",
                        SpaceUnit::Picas => "pc",
                        SpaceUnit::Zero => "pt",
                    };
                    format!("{number}{unit}")
                })
                .unwrap_or_else(|| "0pt".into())
        };
        format!(
            "\\phantom{{\\rule[-{}]{{{}}}{{\\dimexpr {}+{}\\relax}}}}",
            tex(&self.depth),
            tex(&self.width),
            tex(&self.height),
            tex(&self.depth)
        )
    }
}
pub fn is_spacing(node: &XmlNode) -> bool {
    node.namespace == MATHML_NS && node.local() == "mspace"
}
pub fn contains_spacing(node: &XmlNode) -> bool {
    is_spacing(node) || node.children().any(contains_spacing)
}

/// New LaTeX kern atoms enter exactly the existing bounded spacing model.
/// Token matching excludes escaped commands and literal text arguments; plain
/// historical LaTeX retains its original conversion and fallback contract.
pub(crate) fn latex_spacing_xml(source: &str, display: bool) -> Result<Option<String>> {
    if !crate::latex2omml::has_math_extensions(source) { return Ok(None); }
    let xml = crate::latex2omml::latex_to_omml_checked(source, display)?;
    let root = crate::math_document::parse_xml(&xml)?;
    Ok(contains_spacing(&root).then_some(xml))
}
pub(crate) fn validate_tree(node: &XmlNode) -> Result<()> {
    if node.namespace == MATHML_NS {
        Spacing::from_xml(node)?;
    }
    for child in node.children() {
        validate_tree(child)?;
    }
    Ok(())
}
pub(crate) fn native_office(node: &XmlNode) -> Result<()> {
    if contains_spacing(node) {
        Err("math_unsupported_extension")
    } else {
        Ok(())
    }
}

/// Both display and inline math, including tables, notes and styled blocks.
pub(crate) fn document_has_spacing(doc: &crate::md_ast::Document) -> Result<bool> {
    use crate::md_ast::{Block, Inline};
    fn xml(value: &str) -> Result<bool> {
        if !value.contains(MATHML_NS) {
            return Ok(false);
        }
        Ok(contains_spacing(&crate::math_document::parse_xml(value)?))
    }
    let mut result = Ok(false);
    crate::md_ast::walk_inlines(&doc.blocks, &mut |inline| {
        if result.is_ok() {
            if let Inline::OfficeMath { omml, .. } = inline {
                match xml(omml) {
                    Ok(true) => result = Ok(true),
                    Err(error) => result = Err(error),
                    _ => (),
                }
            } else if let Inline::Math(latex) = inline {
                match latex_spacing_xml(latex, false) {
                    Ok(Some(_)) => result = Ok(true),
                    Err(error) => result = Err(error),
                    _ => (),
                }
            }
        }
    });
    fn blocks(values: &[Block]) -> Result<bool> {
        for block in values {
            let found = match block {
                Block::OfficeMath { omml, .. } => xml(omml)?,
                Block::Math(latex) => latex_spacing_xml(latex, true)?.is_some(),
                Block::Styled { block, .. } => blocks(std::slice::from_ref(block.as_ref()))?,
                Block::List { items, .. } => {
                    let mut found = false;
                    for item in items {
                        found |= blocks(&item.blocks)?;
                    }
                    found
                }
                Block::Quote(inner) | Block::FootnoteDef { blocks: inner, .. } => blocks(inner)?,
                _ => false,
            };
            if found {
                return Ok(true);
            }
        }
        Ok(false)
    }
    Ok(result? || blocks(&doc.blocks)?)
}

/// The legacy PDF pipeline can print source text when a formula has no
/// vector result. A new spacing tree must never take that lossy branch.
/// Font/glyph availability is independent of this probe's point size; the
/// actual inherited size is still resolved by the normal renderer.
pub(crate) fn require_vector_for_spacing(value: &serde_json::Value) -> Result<()> {
    require_vector_for_spacing_at_size(value, 12.)
}

/// Call at the actual inherited point size before forming a PDF flowable.
/// A failure must be carried through the renderer's typed error channel.
pub(crate) fn require_vector_for_spacing_at_size(
    value: &serde_json::Value,
    size: f64,
) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(source) = map.get("latex").and_then(serde_json::Value::as_str)
                .filter(|_| !map.get("omml").is_some_and(serde_json::Value::is_string)) {
                let display = map.get("display").and_then(serde_json::Value::as_bool).unwrap_or(false);
                if let Some(xml) = latex_spacing_xml(source, display)? {
                    crate::math_layout::spacing_budget(&xml, display, size)?;
                    if crate::math_layout::layout_source(source, Some(&xml), display, size).is_none() {
                        return Err("math_unsupported_extension");
                    }
                }
            }
            if let Some(xml) = map
                .get("omml")
                .and_then(serde_json::Value::as_str)
                .filter(|xml| xml.contains(MATHML_NS))
            {
                let root = crate::math_document::parse_xml(xml)?;
                if contains_spacing(&root) {
                    let display = map
                        .get("display")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    crate::math_layout::spacing_budget(xml, display, size)?;
                    if crate::math_layout::layout_source("", Some(xml), display, size).is_none() {
                        return Err("math_unsupported_extension");
                    }
                }
            }
            for child in map.values() {
                require_vector_for_spacing_at_size(child, size)?;
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                require_vector_for_spacing_at_size(child, size)?;
            }
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mathml_space_units_preserve_spelling_and_have_independent_physical_values() {
        for (source, expected) in [
            ("1em", 12.),
            ("2ex", 10.8),
            ("12pt", 12.),
            ("16px", 12.),
            ("1in", 72.),
            ("2.54cm", 72.),
            ("25.4mm", 72.),
            ("1pc", 12.),
            ("0", 0.),
            ("+1.25EM", 15.),
            ("1e1pt", 10.),
        ] {
            let value = SpaceLength::parse(source).unwrap();
            assert_eq!(value.source, source);
            assert!(
                (value.points(12., 0.45).unwrap() - expected).abs() < 1e-9,
                "{source}"
            );
        }
    }
    #[test]
    fn mathml_space_invalid_and_unsupported_values_are_distinct_and_never_coerced() {
        for value in [
            "NaNpt",
            "infem",
            "2fake",
            "1.em",
            "",
            "em",
            "1 pt",
            "0pt;display:none",
        ] {
            assert_eq!(
                SpaceLength::parse(value).unwrap_err(),
                "math_invalid_property",
                "{value}"
            );
        }
        for value in [
            "-1em",
            "-0.2pt",
            "50%",
            "2rem",
            "1ch",
            "thinmathspace",
            "calc(1em + 2pt)",
        ] {
            assert_eq!(
                SpaceLength::parse(value).unwrap_err(),
                "math_unsupported_extension",
                "{value}"
            );
        }
        assert_eq!(SpaceLength::parse("4097em").unwrap_err(), "math_budget");
        assert_eq!(
            SpaceLength::parse("1e999pt").unwrap_err(),
            "math_invalid_property"
        );
    }
    #[test]
    fn mathml_space_omission_explicit_zero_and_size_are_different_source_states() {
        let empty = Spacing::default();
        let zero = Spacing::from_attributes([("width", "0px"), ("depth", "0ex")]).unwrap();
        assert_ne!(empty, zero);
        assert_ne!(empty.node().xml(), zero.node().xml());
        assert_eq!(zero.dimensions(12., 0.45).unwrap(), (0., 0., 0.));
        let size = Spacing::from_attributes([
            ("width", "1em"),
            ("height", "2ex"),
            ("depth", "3px"),
            ("mathsize", "18pt"),
        ])
        .unwrap();
        let actual = size.dimensions(8.4, 0.45).unwrap();
        assert!(
            (actual.0 - 18.).abs() < 1e-9
                && (actual.1 - 16.2).abs() < 1e-9
                && (actual.2 - 2.25).abs() < 1e-9
        );
        assert_eq!(Spacing::from_xml(&size.node()).unwrap(), size);
    }
    #[test]
    fn mathml_space_namespace_and_attribute_whitelist_cannot_open_foreign_xml() {
        for source in [
            "<mspace xmlns='urn:other' width='1em'/>",
            "<mrow xmlns='http://www.w3.org/1998/Math/MathML'/>",
            "<mspace xmlns='http://www.w3.org/1998/Math/MathML' style='width:2em'/>",
            "<mspace xmlns='http://www.w3.org/1998/Math/MathML'><mi>x</mi></mspace>",
        ] {
            let root = crate::math_document::parse_xml(source).unwrap();
            assert_eq!(
                Spacing::from_xml(root.children().next().unwrap()).unwrap_err(),
                "math_unsupported_extension"
            );
        }
        assert!(Spacing::default().tex_annotation().contains("\\phantom"));
        assert!(!Spacing::default().tex_annotation().contains("\\text"));
    }
}

#[cfg(test)]
#[path = "math_spacing_tests.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "math_spacing_property_controls_tests.rs"]
mod property_controls_tests;
