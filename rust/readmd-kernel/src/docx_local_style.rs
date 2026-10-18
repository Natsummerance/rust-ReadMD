//! Native Word paragraph/run attributes for a single publication block.
use crate::{
    math_document::{parse_xml, XmlNode, XmlPart},
    publication_blocks::Resolved,
};
use serde_json::Value;
const P_ORDER: &[&str] = &[
    "pStyle",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "widowControl",
    "numPr",
    "pBdr",
    "shd",
    "tabs",
    "suppressAutoHyphens",
    "snapToGrid",
    "spacing",
    "ind",
    "contextualSpacing",
    "mirrorIndents",
    "jc",
    "textDirection",
    "textAlignment",
    "outlineLvl",
    "sectPr",
    "rPr",
];
const R_ORDER: &[&str] = &[
    "rStyle",
    "rFonts",
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "dstrike",
    "outline",
    "shadow",
    "emboss",
    "imprint",
    "noProof",
    "snapToGrid",
    "vanish",
    "webHidden",
    "color",
    "spacing",
    "w",
    "kern",
    "position",
    "sz",
    "szCs",
    "highlight",
    "u",
    "effect",
    "bdr",
    "shd",
    "fitText",
    "vertAlign",
    "rtl",
    "cs",
    "em",
    "lang",
    "eastAsianLayout",
    "specVanish",
    "oMath",
];
fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn merge(node: &mut XmlNode, updates: &str, order: &[&str]) -> Result<(), &'static str> {
    let parsed = parse_xml(&format!("<w:properties>{updates}</w:properties>"))
        .map_err(|_| "publication_local_style_invalid")?;
    let additions = parsed
        .children()
        .next()
        .ok_or("publication_local_style_invalid")?;
    for child in additions.children() {
        node.content
            .retain(|p| !matches!(p,XmlPart::Element(n) if n.name==child.name));
        node.content.push(XmlPart::Element(child.clone()));
    }
    node.content.sort_by_key(|p| match p {
        XmlPart::Element(n) => order
            .iter()
            .position(|s| *s == n.local())
            .unwrap_or(order.len()),
        _ => order.len(),
    });
    Ok(())
}
fn properties(
    node: &mut XmlNode,
    name: &str,
    updates: &str,
    order: &[&str],
) -> Result<(), &'static str> {
    if let Some(XmlPart::Element(p)) = node
        .content
        .iter_mut()
        .find(|p| matches!(p,XmlPart::Element(n) if n.name==name))
    {
        merge(p, updates, order)?;
    } else {
        let parsed = parse_xml(&format!("<{name}>{updates}</{name}>"))
            .map_err(|_| "publication_local_style_invalid")?;
        let at = if node.name == "m:r" && name == "w:rPr" {
            node.content
                .iter()
                .take_while(|p| matches!(p,XmlPart::Element(n) if n.name=="m:rPr"))
                .count()
        } else {
            0
        };
        node.content.insert(
            at,
            XmlPart::Element(
                parsed
                    .children()
                    .next()
                    .ok_or("publication_local_style_invalid")?
                    .clone(),
            ),
        );
    }
    Ok(())
}
pub fn apply(fragment: &str, style: &Resolved, attributes: &Value, global: &Value, inherit_body: bool) -> Result<String, &'static str> {
    // Physical offsets are consumed at the ordinary-run boundary by the
    // writer, before code/CSL/OMML lose their identities in an XML fragment.
    // A direct caller must use that boundary too; never silently drop it.
    if crate::publication_typography::baseline::local(&attributes["text"])?.is_some() {
        return Err(crate::publication_typography::baseline::UNSUPPORTED);
    }
    let mut root = parse_xml(fragment).map_err(|_| "publication_local_style_invalid")?;
    let typed = crate::publication_typography::resolve_line_spacing(crate::publication_typography::global_line_spacing(global)?, &attributes["paragraph"], inherit_body)?;
    let native = typed.map(|value| value.docx()).unwrap_or(crate::publication_typography::DocxLine { line: (style.line_height * 240.).round() as u32, rule: "auto" });
    let line = native.line; let line_rule = native.rule;
    let grid = if typed.is_some() { "<w:snapToGrid w:val=\"0\"/>" } else { "" };
    let p=format!("<w:keepNext w:val=\"{}\"/><w:keepLines w:val=\"{}\"/><w:pageBreakBefore w:val=\"{}\"/><w:widowControl w:val=\"{}\"/>{grid}<w:spacing w:before=\"{}\" w:after=\"{}\" w:line=\"{line}\" w:lineRule=\"{line_rule}\"/><w:ind w:left=\"{}\" w:right=\"{}\" {}=\"{}\"/><w:jc w:val=\"{}\"/>",
        style.keep_next as u8,style.keep_lines as u8,style.page_break as u8,style.widow as u8,(style.before*20.).round()as i64,(style.after*20.).round()as i64,(style.left*1440./25.4).round()as i64,(style.right*1440./25.4).round()as i64,if style.hanging>0.{"w:hanging"}else{"w:firstLine"},(style.first.max(style.hanging)*1440./25.4).round()as i64,if style.align=="justify"{"both"}else{&style.align});
    let mut r = String::new();
    let text = &attributes["text"];
    if text.get("font").is_some() {
        let font = escaped(crate::publication_blocks::font_family(&style.font));
        r.push_str(&format!("<w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:eastAsia=\"{font}\" w:cs=\"{font}\"/>"));
    }
    if text.get("size").is_some() {
        let size = (style.size * 2.).round() as i64;
        r.push_str(&format!(
            "<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
        ));
    }
    if text.get("color").is_some() {
        r.push_str(&format!("<w:color w:val=\"{}\"/>", &style.color[1..]));
    }
    // Explicit strong/emphasis still win inside a locally normal paragraph.
    fn runs(node: &mut XmlNode, r: &str, text: &Value) -> Result<(), &'static str> {
        if node.name == "w:r" || node.name == "m:r" {
            let existing = node.children().find(|n| n.name == "w:rPr");
            let has = |key: &str| existing.is_some_and(|p| p.children().any(|n| n.local() == key));
            let mut extra = r.to_string();
            for (key, tag) in [("bold", "b"), ("italic", "i")] {
                if let Some(value) = text[key].as_bool() {
                    if !has(tag) {
                        extra.push_str(&format!(
                            "<w:{tag} w:val=\"{}\"/><w:{tag}Cs w:val=\"{}\"/>",
                            value as u8, value as u8
                        ));
                    }
                }
            }
            if !extra.is_empty() {
                properties(node, "w:rPr", &extra, R_ORDER)?;
            }
            return Ok(());
        }
        for part in &mut node.content {
            if let XmlPart::Element(n) = part {
                runs(n, r, text)?;
            }
        }
        Ok(())
    }
    for part in &mut root.content {
        if let XmlPart::Element(paragraph) = part {
            if paragraph.name == "w:p" {
                // A text-only override must not reset list/quote indentation.
                // Keep numbering-derived indentation and task hanging indents
                // unless the author explicitly changes an indentation field.
                let scoped = paragraph.children().find(|n| n.name == "w:pPr")
                    .is_some_and(|pr| pr.children().any(|n| n.local() == "numPr"
                        || n.local() == "pStyle" && n.xml().contains("Quote")
                        || n.local() == "pStyle" && n.xml().contains("ListParagraph")));
                let explicit_indent = ["left", "right", "firstLine", "hanging"].iter()
                    .any(|key| attributes["paragraph"].get(*key).is_some());
                let mut updates = p.clone();
                if scoped && !explicit_indent {
                    if let Some(start) = updates.find("<w:ind ") {
                        if let Some(end) = updates[start..].find("/>") {
                            updates.replace_range(start..start + end + 2, "");
                        }
                    }
                }
                properties(paragraph, "w:pPr", &updates, P_ORDER)?;
                runs(paragraph, &r, text)?;
            }
        }
    }
    Ok(root.xml())
}

/// Apply the defaults of the footnote story after the shared block writer.
/// Explicit local/run attributes win. Preserve structural paragraph styles,
/// list numbering, table cells, drawings and math rather than flattening them.
pub fn footnote(fragment: &str, style: &Value) -> Result<String, &'static str> {
    const INVALID: &str = "publication_local_style_invalid";
    let mut root = parse_xml(fragment).map_err(|_| INVALID)?;
    let size = (style["footnotes"]["size"].as_f64().unwrap_or(9.) * 2.).round() as i64;
    let line = (style["footnotes"]["lineHeight"].as_f64().unwrap_or(1.2) * 240.).round() as i64;
    let end=crate::publication_layout::notes_at_end(style);
    let paragraph_style=if end{"EndnoteText"}else{"FootnoteText"};
    fn defaults(node: &mut XmlNode, size: i64, line: i64, note_style:&str) -> Result<(), &'static str> {
        if node.name == "w:p" {
            let pr = node.children().find(|n| n.name == "w:pPr");
            let paragraph_style = pr.and_then(|p| p.children().find(|n| n.name == "w:pStyle"));
            let body_style = paragraph_style.map_or(true, |p| p.xml().contains("\"BodyText\""));
            let spacing = pr.is_some_and(|p| p.children().any(|n| n.name == "w:spacing"));
            if body_style { properties(node, "w:pPr", &format!("<w:pStyle w:val=\"{note_style}\"/>"), P_ORDER)?; }
            if !spacing {
                properties(node, "w:pPr", &format!("<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{line}\" w:lineRule=\"auto\"/>"), P_ORDER)?;
            }
        }
        if node.name == "w:r" {
            let explicit = node.children().find(|n| n.name == "w:rPr")
                .is_some_and(|p| p.children().any(|n| n.name == "w:sz"));
            if !explicit { properties(node, "w:rPr", &format!("<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"), R_ORDER)?; }
            return Ok(());
        }
        for part in &mut node.content {
            if let XmlPart::Element(child) = part { defaults(child, size, line, note_style)?; }
        }
        Ok(())
    }
    defaults(&mut root, size, line, paragraph_style)?;
    let (reference,kind)=if end{("EndnoteReference","endnote")}else{("FootnoteReference","footnote")};
    let mark = format!("<w:r><w:rPr><w:rStyle w:val=\"{reference}\"/></w:rPr><w:{kind}Ref/></w:r><w:r><w:t xml:space=\"preserve\"> </w:t></w:r>");
    let marker = parse_xml(&mark).map_err(|_| INVALID)?.content;
    let first = root.content.iter().position(|p| matches!(p, XmlPart::Element(_)));
    if let Some(XmlPart::Element(paragraph)) = first.and_then(|i| root.content.get_mut(i)).filter(|p| matches!(p,XmlPart::Element(n) if n.name == "w:p")) {
        let at = paragraph.content.iter().position(|p| matches!(p,XmlPart::Element(n) if n.name == "w:pPr")).map_or(0, |i| i+1);
        paragraph.content.splice(at..at, marker);
    } else {
        let paragraph = parse_xml(&format!("<w:p><w:pPr><w:pStyle w:val=\"{paragraph_style}\"/></w:pPr>{mark}</w:p>")).map_err(|_| INVALID)?;
        root.content.splice(0..0, paragraph.content);
    }
    // A trailing paragraph gives editors a valid insertion point after tables.
    if root.children().last().is_some_and(|n| n.name == "w:tbl") {
        root.content.extend(parse_xml(&format!("<w:p><w:pPr><w:pStyle w:val=\"{paragraph_style}\"/></w:pPr></w:p>")).map_err(|_| INVALID)?.content);
    }
    Ok(root.xml())
}
