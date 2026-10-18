//! Offline symbol spelling shared with the formula editor. OfficeMath uses
//! Unicode, so a literal mathematical scalar does not need a TeX macro alias.
//! This catalogue is not a substitute for OMML structural/property coverage.
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref ALIASES: HashMap<String, String> = {
        let data: serde_json::Value = serde_json::from_str(include_str!("../../../assets/math-catalog.json"))
            .expect("checked-in math catalogue is valid JSON");
        let mut out = HashMap::new();
        for item in data["symbols"].as_array().expect("symbols array") {
            for name in item["aliases"].as_array().expect("alias array") {
                out.insert(format!("\\{}", name.as_str().expect("alias string")), item["symbol"].as_str().expect("symbol string").to_owned());
            }
        }
        out
    };
}

pub fn symbol(command: &str) -> Option<&'static str> {
    ALIASES.get(command).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_catalogue_preserves_unicode_and_extended_aliases_in_office_math() {
        let data: serde_json::Value = serde_json::from_str(include_str!("../../../assets/math-catalog.json")).unwrap();
        assert!(data["symbols"].as_array().unwrap().len() >= 2400);
        for item in data["symbols"].as_array().unwrap() {
            let glyph = item["symbol"].as_str().unwrap();
            let xml = crate::latex2omml::latex_to_omml(glyph, false);
            let tree = crate::omml::parse(&xml).unwrap();
            let mut pending = vec![&tree]; let mut text = String::new();
            while let Some(n) = pending.pop() { if n.name == "t" { text.push_str(&n.text); } for child in n.children.iter().rev() { pending.push(child); } }
            // ASCII TeX syntax punctuation requires an escaped source spelling.
            if !"^_{}&".contains(glyph) { assert!(text.contains(glyph), "{}: {xml}", item["id"]); }
        }
        for command in [r"\therefore", r"\nexists", r"\leftrightharpoons", r"\digamma", r"\varkappa"] {
            let glyph = symbol(command).unwrap();
            let xml = crate::latex2omml::latex_to_omml(command, false);
            assert!(xml.contains(glyph), "{command}: {xml}");
        }
        // A shared catalogue is meaningful only when every command spelling
        // actually produces the same scalar as the visual picker. XML escaping
        // is significant for relation signs and ampersands.
        for (command, glyph) in ALIASES.iter() {
            let xml = crate::latex2omml::latex_to_omml(command, false);
            let escaped = glyph.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            assert!(xml.contains(&escaped), "{command} should retain {glyph}: {xml}");
        }
    }
}
