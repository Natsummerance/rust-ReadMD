//! Shared, bounded OOXML ZIP reader and offline resource vetting.
//! Extraction diagnostics and original-layout rendering use the same central
//! directory, local-header, decompression, range, and CRC validation.
use flate2::read::DeflateDecoder;
use quick_xml::{events::Event, Reader, XmlVersion};
use std::{collections::HashSet, io::Read, sync::OnceLock, time::Instant};
const INVALID: &str = "office_preview_invalid";
const TOO_LARGE: &str = "office_preview_too_large";
const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_PART: usize = 32 * 1024 * 1024;
const MAX_EXPANDED: usize = 192 * 1024 * 1024;
const MAX_FILES: usize = 8192;
type Result<T> = std::result::Result<T, &'static str>;
fn checkpoint(deadline: Instant) -> Result<()> {
    if crate::speech::cancelled() { Err("office_preview_cancelled") }
    else if Instant::now() >= deadline { Err("office_preview_timeout") } else { Ok(()) }
}
pub(crate) struct Member { pub(crate) name: String, flags: u16, method: u16, crc: u32, compressed: usize, pub(crate) size: usize, offset: usize }
pub(crate) fn u16_at(bytes: &[u8], at: usize) -> Result<u16> { Ok(u16::from_le_bytes(bytes.get(at..at + 2).ok_or("invalid_archive")?.try_into().unwrap())) }
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Result<usize> { Ok(u32::from_le_bytes(bytes.get(at..at + 4).ok_or("invalid_archive")?.try_into().unwrap()) as usize) }
fn crc32(bytes: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, value) in table.iter_mut().enumerate() {
            let mut v = index as u32; for _ in 0..8 { v = if v & 1 != 0 { (v >> 1) ^ 0xedb88320 } else { v >> 1 }; } *value = v;
        } table
    });
    !bytes.iter().fold(!0u32, |crc, &b| table[((crc ^ b as u32) & 255) as usize] ^ (crc >> 8))
}
pub(crate) fn directory(bytes: &[u8], max_input: usize, max_entries: usize, check: &dyn Fn() -> Result<()>) -> Result<(Vec<Member>, usize)> {
    if bytes.len() < 22 { return Err("invalid_archive"); }
    if bytes.len() > max_input { return Err("budget"); }
    let end = (bytes.len().saturating_sub(65557)..=bytes.len() - 22).rev().find(|&at|
        bytes[at..].starts_with(b"PK\x05\x06") && u16_at(bytes, at + 20).is_ok_and(|n| at + 22 + n as usize == bytes.len())).ok_or("invalid_archive")?;
    let count = u16_at(bytes, end + 10)? as usize;
    if count == 65535 { return Err("unsupported_archive"); }
    if count > max_entries { return Err("budget"); }
    if u16_at(bytes, end + 4)? != 0 || u16_at(bytes, end + 6)? != 0 || u16_at(bytes, end + 8)? as usize != count { return Err("invalid_archive"); }
    let central = u32_at(bytes, end + 16)?;
    if central.checked_add(u32_at(bytes, end + 12)?) != Some(end) { return Err("invalid_archive"); }
    let mut at = central; let mut names = HashSet::new(); let mut members = Vec::with_capacity(count);
    for _ in 0..count {
        check()?;
        let header = bytes.get(at..at.checked_add(46).ok_or("invalid_archive")?).ok_or("invalid_archive")?;
        if !header.starts_with(b"PK\x01\x02") { return Err("invalid_archive"); }
        let length = u16_at(header, 28)? as usize;
        if length == 0 || length > 1024 { return Err("invalid_archive"); }
        let name = std::str::from_utf8(bytes.get(at + 46..at + 46 + length).ok_or("invalid_archive")?).map_err(|_| "invalid_archive")?.to_owned();
        if name.contains(['\\', ':', '\0']) || name.starts_with('/') || name.trim_end_matches('/').split('/').any(|p| p.is_empty() || matches!(p, "." | "..")) || !names.insert(name.to_ascii_lowercase()) { return Err("invalid_archive"); }
        let flags = u16_at(header, 8)?; let method = u16_at(header, 10)?;
        let size = u32_at(header, 24)?;
        if flags & (1 | 0x40 | 0x2000) != 0 || !matches!(method, 0 | 8) || u16_at(header, 34)? != 0 { return Err("invalid_archive"); }
        // A ZIP symlink is not an Office part, even though nothing is unpacked.
        let attributes = u32_at(header, 38)? as u32;
        if (attributes >> 16) & 0xf000 == 0xa000 { return Err("invalid_archive"); }
        members.push(Member { name, flags, method, crc: u32_at(header, 16)? as u32,
            compressed: u32_at(header, 20)?, size, offset: u32_at(header, 42)? });
        at = at.checked_add(46 + length + u16_at(header, 30)? as usize + u16_at(header, 32)? as usize).ok_or("invalid_archive")?;
        if at > end { return Err("invalid_archive"); }
    }
    if at != end { return Err("invalid_archive"); }
    let mut ranges = members.iter().map(|m| local_range(bytes, m, central).map(|(a, _, b)| (a, b))).collect::<Result<Vec<_>>>()?;
    ranges.sort_unstable();
    if ranges.windows(2).any(|p| p[0].1 > p[1].0) { return Err("invalid_archive"); }
    Ok((members, central))
}
pub(crate) fn part(bytes: &[u8], member: &Member, central: usize, max_size: usize, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
    check()?;
    if member.size > max_size { return Err("budget"); }
    let (_, start, end) = local_range(bytes, member, central)?;
    if member.flags & 8 == 0 && u32_at(bytes, member.offset + 22)? != member.size { return Err("invalid_archive"); }
    let raw = bytes.get(start..end).ok_or("invalid_archive")?;
    let mut decoded = Vec::new();
    if member.method == 0 { if raw.len() != member.size { return Err("invalid_archive"); } decoded.extend_from_slice(raw); }
    else { let mut decoder = DeflateDecoder::new(raw); decoder.by_ref().take(member.size as u64 + 1).read_to_end(&mut decoded).map_err(|_| "invalid_archive")?; if decoder.total_in() != member.compressed as u64 { return Err("invalid_archive"); } }
    if decoded.len() != member.size || crc32(&decoded) != member.crc { return Err("invalid_archive"); }
    check()?; Ok(decoded)
}
fn local_range(bytes: &[u8], member: &Member, central: usize) -> Result<(usize, usize, usize)> {
    let head = bytes.get(member.offset..member.offset.checked_add(30).ok_or("invalid_archive")?).ok_or("invalid_archive")?;
    if !head.starts_with(b"PK\x03\x04") || u16_at(head, 6)? != member.flags || u16_at(head, 8)? != member.method { return Err("invalid_archive"); }
    let length = u16_at(head, 26)? as usize;
    let name = bytes.get(member.offset + 30..member.offset + 30 + length).ok_or("invalid_archive")?;
    if name != member.name.as_bytes() { return Err("invalid_archive"); }
    if member.flags & 8 == 0 && (u32_at(head, 14)? as u32 != member.crc || u32_at(head, 18)? != member.compressed) { return Err("invalid_archive"); }
    let start = member.offset.checked_add(30 + length + u16_at(head, 28)? as usize).ok_or("invalid_archive")?;
    let end = start.checked_add(member.compressed).filter(|&end| end <= central).ok_or("invalid_archive")?;
    Ok((member.offset, start, end))
}

fn relationship_target(part: &str, target: &str) -> Result<String> {
    let target = target.split('#').next().unwrap_or("");
    let decoded = percent_encoding::percent_decode_str(target).decode_utf8().map_err(|_| INVALID)?;
    if decoded.contains(['\\', ':', '\0', '?']) || decoded.starts_with("//") { return Err(INVALID); }
    let directory = part.rsplit_once("/_rels/").map(|(p, _)| p).unwrap_or("");
    let mut parts: Vec<&str> = if decoded.starts_with('/') { Vec::new() } else { directory.split('/').filter(|s| !s.is_empty()).collect() };
    for item in decoded.split('/') {
        match item { "" | "." => (), ".." => { parts.pop().ok_or(INVALID)?; }, other => parts.push(other) }
    }
    Ok(parts.join("/"))
}
pub(crate) fn validate(ext: &str, bytes: &[u8], deadline: Instant) -> Result<()> {
    let mut budget = PackageBudget { expanded: MAX_EXPANDED, entries: MAX_FILES, nodes: 0, deadline };
    validate_package(Some(ext), bytes, &mut budget, 0)
}
struct PackageBudget { expanded: usize, entries: usize, nodes: usize, deadline: Instant }
fn archive_error(error: &'static str) -> &'static str { if error == "budget" { TOO_LARGE } else if error.starts_with("office_preview_") { error } else { INVALID } }
fn validate_package(ext: Option<&str>, bytes: &[u8], budget: &mut PackageBudget, depth: usize) -> Result<()> {
    if depth > 3 { return Err(TOO_LARGE); }
    let deadline = budget.deadline;
    let (members, central) = directory(bytes, MAX_INPUT, budget.entries, &||checkpoint(deadline)).map_err(archive_error)?;
    budget.entries = budget.entries.checked_sub(members.len()).ok_or(TOO_LARGE)?;
    for member in &members { budget.expanded = budget.expanded.checked_sub(member.size).ok_or(TOO_LARGE)?; }
    let names: HashSet<&str> = members.iter().map(|m| m.name.as_str()).collect();
    if !names.contains("[Content_Types].xml") || !names.contains("_rels/.rels") { return Err(INVALID); }
    if let Some(ext) = ext { if !names.contains(match ext { "docx" => "word/document.xml", "pptx" => "ppt/presentation.xml", "xlsx" => "xl/workbook.xml", _ => return Err(INVALID) }) { return Err(INVALID); } }
    for member in &members {
        checkpoint(deadline)?;
        let decoded = part(bytes, member, central, MAX_PART, &||checkpoint(deadline)).map_err(archive_error)?;
        let lower = member.name.to_ascii_lowercase();
        if lower.contains("/externallinks/") || lower.ends_with("/connections.xml") || lower.contains("/querytables/") { return Err(INVALID); }
        if decoded.starts_with(b"PK\x03\x04") {
            let nested_ext = ["xlsx", "docx", "pptx"].iter().find(|ext| lower.ends_with(&format!(".{ext}"))).copied();
            validate_package(nested_ext, &decoded, budget, depth + 1)?;
        } else if decoded.starts_with(&[0xd0,0xcf,0x11,0xe0,0xa1,0xb1,0x1a,0xe1]) { return Err(INVALID); }
        if lower.ends_with(".svg") { crate::svg_render::validate(&decoded).map_err(|_| INVALID)?; }
        if lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".vml") { validate_xml(&member.name, &decoded, &names, &mut budget.nodes, deadline)?; }
        // Imported web parts, macros and embedded executables are not needed
        // to lay out OOXML; no linked-content interpreter is started.
        if [".html", ".htm", ".mht", ".mhtml", ".xhtml", ".exe", ".dll", ".js", ".vbs"].iter().any(|s| lower.ends_with(s))
            || lower.ends_with("vbaproject.bin") { return Err(INVALID); }
    }
    Ok(())
}
fn validate_xml(part: &str, bytes: &[u8], names: &HashSet<&str>, nodes: &mut usize, deadline: Instant) -> Result<()> {
    let utf8 = if bytes.starts_with(&[0xff, 0xfe]) { encoding_rs::UTF_16LE.decode_without_bom_handling(&bytes[2..]).0.into_owned() }
        else if bytes.starts_with(&[0xfe, 0xff]) { encoding_rs::UTF_16BE.decode_without_bom_handling(&bytes[2..]).0.into_owned() }
        else { std::str::from_utf8(bytes).map_err(|_| INVALID)?.to_owned() };
    let mut reader = Reader::from_str(&utf8); reader.config_mut().check_end_names = true;
    let mut depth = 0usize; let mut roots = 0usize; let mut instruction_depth = None; let mut instructions = String::new();
    let mut formula_depth = None; let mut formulas = String::new();
    loop {
        checkpoint(deadline)?;
        match reader.read_event().map_err(|_| INVALID)? {
            event @ (Event::Start(_) | Event::Empty(_)) => {
                let empty = matches!(&event, Event::Empty(_));
                let element = match event { Event::Start(e) | Event::Empty(e) => e, _ => unreachable!() };
                *nodes += 1; if *nodes > 1_000_000 || depth > 128 { return Err(TOO_LARGE); }
                if depth == 0 { roots += 1; if roots > 1 { return Err(INVALID); } }
                let local = element.local_name(); let name = local.as_ref();
                if name == "instrText" && !empty { instruction_depth = Some(depth + 1); }
                if name == "f" && !empty { formula_depth = Some(depth + 1); }
                if name == "fldChar" { instructions.push('\n'); }
                let mut kind = None; let mut target = None; let mut mode = None;
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| INVALID)?;
                    let key = attribute.key.local_name();
                    let value = attribute.normalized_value(XmlVersion::Implicit1_0).map_err(|_| INVALID)?.into_owned();
                    match key.as_ref() {
                        "Type" => kind = Some(value), "Target" => target = Some(value), "TargetMode" => mode = Some(value),
                        "instr" if name == "fldSimple" => { instructions.push_str(&value); instructions.push(' '); },
                        // Direct resource URIs in VML/XML bypass relationships.
                        "src" | "href" if !value.starts_with('#') && !value.starts_with("data:image/") => return Err(INVALID),
                        "base" => return Err(INVALID),
                        "link" if !value.is_empty() => return Err(INVALID),
                        _ => (),
                    }
                }
                if part.ends_with(".rels") && name == "Relationship" {
                    let kind = kind.ok_or(INVALID)?; let target = target.ok_or(INVALID)?;
                    match mode.as_deref().unwrap_or("Internal") {
                        "External" if kind.ends_with("/hyperlink") => {
                            let lower = target.to_ascii_lowercase();
                            if target.chars().any(char::is_control) || !["http://", "https://", "mailto:"].iter().any(|s| lower.starts_with(s)) { return Err(INVALID); }
                        }
                        "Internal" => { let target = relationship_target(part, &target)?; if !names.contains(target.as_str()) { return Err(INVALID); } }
                        _ => return Err(INVALID),
                    }
                }
                if !empty { depth += 1; }
            }
            Event::End(_) => {
                if instruction_depth == Some(depth) { instruction_depth = None; }
                if formula_depth == Some(depth) { formula_depth = None; formulas.push('\n'); }
                depth = depth.checked_sub(1).ok_or(INVALID)?;
            }
            Event::Text(text) => {
                let text = text.xml_content(XmlVersion::Implicit1_0);
                if depth == 0 && !text.trim().is_empty() { return Err(INVALID); }
                if instruction_depth.is_some() { instructions.push_str(&text); }
                if formula_depth.is_some() { formulas.push_str(&text); }
            }
            Event::CData(text) => {
                if depth == 0 { return Err(INVALID); }
                let text = text.xml_content(XmlVersion::Implicit1_0);
                if instruction_depth.is_some() { instructions.push_str(&text); }
                if formula_depth.is_some() { formulas.push_str(&text); }
            }
            Event::GeneralRef(reference) => {
                let value = quick_xml::escape::unescape(&format!("&{};", reference.as_ref())).map_err(|_| INVALID)?.into_owned();
                if depth == 0 { return Err(INVALID); } if instruction_depth.is_some() { instructions.push_str(&value); }
                if formula_depth.is_some() { formulas.push_str(&value); }
            }
            Event::DocType(_) | Event::PI(_) => return Err(INVALID),
            Event::Eof => break,
            _ => (),
        }
    }
    if depth != 0 || roots != 1 { return Err(INVALID); }
    if instructions.split(|c: char| !c.is_ascii_alphabetic()).any(|s| ["INCLUDETEXT", "INCLUDEPICTURE", "LINK", "DDE", "DDEAUTO", "DATABASE"].iter().any(|v| s.eq_ignore_ascii_case(v))) { return Err(INVALID); }
    static NETWORK_FORMULA: OnceLock<regex::Regex> = OnceLock::new();
    if NETWORK_FORMULA.get_or_init(|| regex::Regex::new(r"(?i)\b(?:IMAGE|WEBSERVICE|RTD|CUBEVALUE|CUBEMEMBER)\s*\(").unwrap()).is_match(&formulas) { return Err(INVALID); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn package(extra: &[(&str, &str)]) -> Vec<u8> {
        let mut parts = vec![
            ("[Content_Types].xml".to_owned(), b"<Types/>".to_vec()),
            ("_rels/.rels".to_owned(), b"<Relationships><Relationship Id=\"r\" Type=\"urn:officeDocument\" Target=\"word/document.xml\"/></Relationships>".to_vec()),
            ("word/document.xml".to_owned(), b"<document><body><p><t>ReadMD test</t></p></body></document>".to_vec()),
        ];
        for (name, content) in extra { parts.retain(|(n, _)| n != name); parts.push((name.to_string(), content.as_bytes().to_vec())); }
        let entries: Vec<_> = parts.into_iter().map(|(path, data)| crate::mdexport::ZipEntry { path, data, compress: false }).collect();
        crate::mdexport::write_zip_at(&entries, 0, 0).unwrap()
    }
    fn checked(bytes: &[u8]) -> Result<()> { validate("docx", bytes, Instant::now() + Duration::from_secs(5)) }
    #[test]
    fn references_remain_inert_but_linked_images_and_templates_fail_closed() {
        let rel = |kind: &str, target: &str| format!("<Relationships><Relationship Id=\"r\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\" TargetMode=\"External\"/></Relationships>");
        assert!(checked(&package(&[("word/_rels/document.xml.rels", &rel("hyperlink", "https://example.test/reference"))])).is_ok());
        for kind in ["image", "attachedTemplate", "oleObject", "externalLink"] {
            assert_eq!(checked(&package(&[("word/_rels/document.xml.rels", &rel(kind, "https://example.test/resource"))])), Err(INVALID));
        }
        assert_eq!(checked(&package(&[("word/_rels/document.xml.rels", &rel("hyperlink", "file:///private/document"))])), Err(INVALID));
    }
    #[test]
    fn rejects_import_fields_dtd_and_encoded_resource_traversal() {
        for xml in ["<!DOCTYPE x SYSTEM 'file:///private/document'><document/>",
            "<document><instrText>INCLUDE</instrText><instrText>TEXT file:///private/document</instrText></document>",
            "<document><fldSimple instr=\"DDEAUTO ignored\"/></document>",
            "<document><imagedata src=\"file:///private/document\"/></document>"] {
            assert_eq!(checked(&package(&[("word/document.xml", xml)])), Err(INVALID));
        }
        assert_eq!(checked(&package(&[("word/_rels/document.xml.rels", "<Relationships><Relationship Id=\"r\" Type=\"urn:image\" Target=\"%2e%2e/%2e%2e/private/document\"/></Relationships>")])), Err(INVALID));
    }
    #[test]
    fn validates_all_members_and_rejects_crc_corruption_and_size_bombs() {
        let good = package(&[]); assert!(checked(&good).is_ok());
        let mut corrupt = good.clone(); let at = corrupt.windows(11).position(|s| s == b"ReadMD test").unwrap(); corrupt[at] ^= 1;
        assert_eq!(checked(&corrupt), Err(INVALID));
        let mut bomb = good; let central = bomb.windows(4).position(|s| s == b"PK\x01\x02").unwrap(); bomb[central + 24..central + 28].copy_from_slice(&(MAX_PART as u32 + 1).to_le_bytes());
        assert_eq!(checked(&bomb), Err(TOO_LARGE));
    }
    #[test]
    fn package_root_relationships_directories_and_internal_chart_workbooks_are_allowed() {
        let outer = package(&[("word/media/", ""), ("word/_rels/document.xml.rels", "<Relationships><Relationship Id=\"r\" Type=\"urn:internal\" Target=\"/word/document.xml\"/></Relationships>")]);
        assert!(checked(&outer).is_ok());
        let workbook = |external: bool| {
            let mut entries = vec![
                ("[Content_Types].xml", "<Types/>"),
                ("_rels/.rels", "<Relationships><Relationship Id=\"r\" Type=\"urn:officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"),
                ("xl/workbook.xml", "<workbook/>"),
            ];
            if external { entries.push(("xl/_rels/workbook.xml.rels", "<Relationships><Relationship Id=\"r\" Type=\"urn:externalLink\" Target=\"https://example.test/workbook.xlsx\" TargetMode=\"External\"/></Relationships>")); }
            let entries: Vec<_> = entries.into_iter().map(|(path, data)| crate::mdexport::ZipEntry { path: path.into(), data: data.as_bytes().to_vec(), compress: true }).collect();
            crate::mdexport::write_zip_at(&entries, 0, 0).unwrap()
        };
        for external in [false, true] {
            let mut entries: Vec<_> = crate::convert::parse_zip_entries(&outer).unwrap().into_iter()
                .map(|(path, data)| crate::mdexport::ZipEntry { path, data, compress: true }).collect();
            entries.push(crate::mdexport::ZipEntry { path: "word/embeddings/chart.xlsx".into(), data: workbook(external), compress: true });
            let nested = crate::mdexport::write_zip_at(&entries, 0, 0).unwrap();
            assert_eq!(checked(&nested), if external { Err(INVALID) } else { Ok(()) });
        }
    }
}
