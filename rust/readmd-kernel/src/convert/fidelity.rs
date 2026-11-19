//! Read-only extraction diagnostics, not a renderer or a second converter.
//! Counts are structural occurrences/parts, never a percentage of visual fidelity.
use crate::convert::ConvertTriple;
use quick_xml::{events::Event, name::ResolveResult, NsReader, XmlVersion};
use serde_json::{json, Value};
#[cfg(test)]
use std::{fs, io::Read};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

const MAX_INPUT: usize = 32 * 1024 * 1024;
const MAX_PART: usize = 4 * 1024 * 1024;
const MAX_XML: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_PARTS: usize = 256;
const MAX_NODES: usize = 250_000;
const MAX_RESULT: usize = 1024 * 1024;
type R<T> = Result<T, &'static str>;

struct Budget {
    deadline: Instant,
    nodes: usize,
    bytes: usize,
}
impl Budget {
    fn check(&self) -> R<()> {
        if crate::speech::cancelled() {
            Err("cancelled")
        } else if Instant::now() >= self.deadline {
            Err("budget")
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct Counts(BTreeMap<&'static str, u64>);
impl Counts {
    fn add(&mut self, code: &'static str, count: u64) {
        if count > 0 {
            *self.0.entry(code).or_default() += count;
        }
    }
    fn merge(&mut self, other: Self) {
        for (code, count) in other.0 {
            self.add(code, count);
        }
    }
}
use crate::office_package::Member;
#[cfg(test)]
use crate::office_package::{u16_at, u32_at};
fn directory(bytes: &[u8], budget: &Budget) -> R<(Vec<Member>, usize)> {
    crate::office_package::directory(bytes, MAX_INPUT, MAX_ENTRIES, &||budget.check())
}
fn part(bytes: &[u8], member: &Member, central: usize, budget: &mut Budget) -> R<Vec<u8>> {
    budget.check()?;
    if member.size > MAX_PART || budget.bytes.checked_add(member.size).is_none_or(|n| n > MAX_XML) { return Err("budget"); }
    let decoded = crate::office_package::part(bytes, member, central, MAX_PART, &||budget.check())?;
    budget.bytes += decoded.len(); budget.check()?; Ok(decoded)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ns {
    Word,
    Math,
    Presentation,
    Sheet,
    WordDrawing,
    SheetDrawing,
    Chart,
    Other,
}
fn namespace(value: ResolveResult<'_>) -> R<Ns> {
    let uri = match value {
        ResolveResult::Bound(v) => v,
        ResolveResult::Unbound => return Ok(Ns::Other),
        ResolveResult::Unknown(_) => return Err("invalid_xml"),
    };
    Ok(match uri.as_ref() {
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
        | "http://purl.oclc.org/ooxml/wordprocessingml/main" => Ns::Word,
        "http://schemas.openxmlformats.org/officeDocument/2006/math"
        | "http://purl.oclc.org/ooxml/officeDocument/math" => Ns::Math,
        "http://schemas.openxmlformats.org/presentationml/2006/main"
        | "http://purl.oclc.org/ooxml/presentationml/main" => Ns::Presentation,
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
        | "http://purl.oclc.org/ooxml/spreadsheetml/main" => Ns::Sheet,
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
        | "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing" => Ns::WordDrawing,
        "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
        | "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing" => Ns::SheetDrawing,
        "http://schemas.openxmlformats.org/drawingml/2006/chart"
        | "http://purl.oclc.org/ooxml/drawingml/chart" => Ns::Chart,
        _ => Ns::Other,
    })
}
struct Cell {
    depth: usize,
    formula: bool,
    cached: bool,
    value_depth: Option<usize>,
    inline: bool,
    string_cache: bool,
}
fn scan_xml(data: &[u8], required: Option<(Ns, &str)>, budget: &mut Budget) -> R<Counts> {
    let source = std::str::from_utf8(data).map_err(|_| "invalid_xml")?;
    let mut reader = NsReader::from_str(source);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut stack = Vec::<(Ns, String)>::new();
    let mut roots = 0;
    let mut out = Counts::default();
    let mut cell: Option<Cell> = None;
    let mut header = false;
    let mut header_content = false;
    loop {
        budget.check()?;
        let event = reader.read_event().map_err(|_| "invalid_xml")?;
        match event {
            Event::Start(ref node) | Event::Empty(ref node) => {
                budget.nodes += 1;
                if budget.nodes > MAX_NODES || stack.len() >= 64 || node.name().as_ref().len() > 256
                {
                    return Err("budget");
                }
                let ns = namespace(reader.resolver().resolve_element(node.name()).0)?;
                let local = node.local_name().as_ref().to_owned();
                let name = local.as_str();
                let mut attrs = BTreeMap::new();
                let mut attr_count = 0;
                for attr in node.attributes() {
                    let attr = attr.map_err(|_| "invalid_xml")?;
                    attr_count += 1;
                    if attr_count > 64 || attr.value.len() > 4096 {
                        return Err("budget");
                    }
                    if attr.key.as_ref() == "xmlns" || attr.key.as_ref().starts_with("xmlns:") {
                        continue;
                    }
                    let resolved = reader.resolver().resolve_attribute(attr.key).0;
                    if matches!(resolved, ResolveResult::Unknown(_)) {
                        return Err("invalid_xml");
                    }
                    let value = attr
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|_| "invalid_xml")?
                        .into_owned();
                    if matches!(resolved, ResolveResult::Unbound) {
                        attrs.insert(attr.key.local_name().as_ref().to_string(), value);
                    }
                }
                if stack.is_empty() {
                    roots += 1;
                    if roots != 1
                        || required.is_some_and(|(expected, local)| ns != expected || name != local)
                    {
                        return Err("invalid_xml");
                    }
                    header = ns == Ns::Word && matches!(name, "hdr" | "ftr");
                    if ns == Ns::Chart && name == "chartSpace" {
                        out.add("charts", 1);
                    }
                }
                match ns {
                    Ns::WordDrawing if name == "anchor" => out.add("floating_objects", 1),
                    Ns::SheetDrawing
                        if matches!(name, "oneCellAnchor" | "twoCellAnchor" | "absoluteAnchor") =>
                    {
                        out.add("floating_objects", 1)
                    }
                    Ns::Word => match name {
                        "txbxContent"
                            if !stack
                                .iter()
                                .any(|(ns, n)| *ns == Ns::WordDrawing && n == "anchor") =>
                        {
                            out.add("floating_objects", 1)
                        }
                        "ins" | "del" | "moveFrom" | "moveTo" | "comment" => {
                            out.add("review_markup", 1)
                        }
                        "drawing" | "object" if header => header_content = true,
                        _ => (),
                    },
                    Ns::Presentation if matches!(name, "timing" | "transition") => {
                        out.add("presentation_effects", 1)
                    }
                    Ns::Math if name == "oMath" => {
                        out.add("native_math", 1);
                        if header {
                            header_content = true;
                        }
                    }
                    Ns::Sheet => {
                        let hidden = attrs
                            .get("hidden")
                            .is_some_and(|v| matches!(v.as_str(), "1" | "true"));
                        if name == "sheet"
                            && attrs
                                .get("state")
                                .is_some_and(|v| matches!(v.as_str(), "hidden" | "veryHidden"))
                            || matches!(name, "row" | "col") && hidden
                        {
                            out.add("hidden_content", 1);
                        }
                        if matches!(
                            name,
                            "mergeCell"
                                | "conditionalFormatting"
                                | "dataValidation"
                                | "pageSetup"
                                | "autoFilter"
                        ) || name == "row" && attrs.contains_key("ht")
                            || name == "col" && attrs.contains_key("width")
                        {
                            out.add("sheet_layout", 1);
                        }
                        if name == "c" {
                            if cell.is_some() {
                                return Err("invalid_xml");
                            }
                            cell = Some(Cell {
                                depth: stack.len() + 1,
                                formula: false,
                                cached: false,
                                value_depth: None,
                                inline: attrs.get("t").is_some_and(|v| v == "inlineStr"),
                                string_cache: attrs.get("t").is_some_and(|v| v == "str"),
                            });
                        }
                        if let Some(cell) = cell.as_mut() {
                            if name == "f" {
                                cell.formula = true;
                            }
                            if name == "v" || name == "is" && cell.inline {
                                cell.value_depth = Some(stack.len() + 1);
                                // A cached string result can legitimately be empty.
                                if name == "v" && cell.string_cache {
                                    cell.cached = true;
                                }
                            }
                        }
                    }
                    _ => (),
                }
                if matches!(event, Event::Start(_)) {
                    stack.push((ns, name.into()));
                } else if cell.as_ref().is_some_and(|c| c.depth == stack.len() + 1) {
                    cell = None;
                }
            }
            Event::End(_) => {
                if let Some(c) = cell.as_mut() {
                    if c.value_depth == Some(stack.len()) {
                        c.value_depth = None;
                    }
                    if c.depth == stack.len() {
                        if c.formula {
                            out.add("spreadsheet_formulas", 1);
                            if !c.cached {
                                out.add("formula_cache_missing", 1);
                            }
                        }
                        cell = None;
                    }
                }
                stack.pop().ok_or("invalid_xml")?;
            }
            Event::Text(ref text) => {
                let value = text.xml_content(XmlVersion::Implicit1_0);
                if stack.is_empty() && !value.trim().is_empty() {
                    return Err("invalid_xml");
                }
                if !value.trim().is_empty() {
                    if let Some(c) = cell.as_mut() {
                        if c.value_depth.is_some() {
                            c.cached = true;
                        }
                    }
                    if header && stack.iter().any(|(ns, n)| *ns == Ns::Word && n == "t") {
                        header_content = true;
                    }
                }
            }
            Event::CData(ref text) => {
                let value = text.xml_content(XmlVersion::Implicit1_0);
                if stack.is_empty() {
                    return Err("invalid_xml");
                }
                if let Some(c) = cell.as_mut() {
                    if c.value_depth.is_some() && !value.trim().is_empty() {
                        c.cached = true;
                    }
                }
                if header
                    && !value.trim().is_empty()
                    && stack.iter().any(|(ns, n)| *ns == Ns::Word && n == "t")
                {
                    header_content = true;
                }
            }
            Event::GeneralRef(ref reference) => {
                let value = quick_xml::escape::unescape(&format!("&{};", reference.as_ref()))
                    .map_err(|_| "invalid_xml")?
                    .into_owned();
                if stack.is_empty() {
                    return Err("invalid_xml");
                }
                if let Some(c) = cell.as_mut() {
                    if c.value_depth.is_some() && !value.trim().is_empty() {
                        c.cached = true;
                    }
                }
                if header
                    && !value.trim().is_empty()
                    && stack.iter().any(|(ns, n)| *ns == Ns::Word && n == "t")
                {
                    header_content = true;
                }
            }
            Event::DocType(_) | Event::PI(_) => return Err("invalid_xml"),
            Event::Eof => break,
            _ => (),
        }
    }
    if !stack.is_empty() || roots != 1 {
        return Err("invalid_xml");
    }
    if header && header_content {
        out.add("header_footer", 1);
    }
    Ok(out)
}

fn format(path: &Path) -> String {
    path.extension()
        .and_then(|v| v.to_str())
        .filter(|s| s.len() <= 16 && s.bytes().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "unknown".into())
}
fn base(path: &Path, result: &ConvertTriple) -> Value {
    let engine = if !result.engine.is_empty()
        && result.engine.len() <= 80
        && result
            .engine
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    {
        result.engine.as_str()
    } else {
        "unknown"
    };
    let nonempty = !result.text.trim().is_empty();
    json!({"schema":1,"format":format(path),"engine":engine,"inspection":"unavailable","scope":"structural","renderedComparison":false,
        "extraction":{"status":match(result.error.is_some(),nonempty){(true,true)=>"partial",(true,false)=>"failed",(false,false)=>"empty",_=>"complete"},"characters":result.text.chars().count(),"assets":result.assets.len()},
        "findings":[],"coverage":{"partsInspected":0,"partsSkipped":0,"xmlBytes":0,"resultInspected":false,"countUnit":"structural_occurrences"},
        "notChecked":["visual_rendering","binary_payloads","relationship_targets","source_result_equivalence"],
        "limits":{"inputBytes":MAX_INPUT,"partBytes":MAX_PART,"xmlBytes":MAX_XML,"entries":MAX_ENTRIES,"parts":MAX_PARTS,"nodes":MAX_NODES}})
}
fn finding(code: &str, count: u64, status: &str, severity: &str) -> Value {
    json!({"code":code,"count":count,"status":status,"severity":severity})
}
fn incomplete(report: &mut Value, reason: &str, count: usize) {
    report["reason"] = json!(reason);
    report["findings"].as_array_mut().unwrap().push(finding(
        "inspection_incomplete",
        count.max(1) as u64,
        "not_checked",
        "info",
    ));
}
fn math_result(text: &str, budget: &Budget) -> R<(usize, usize)> {
    if text.len() > MAX_RESULT {
        return Err("result_budget");
    }
    let mut equations = 0;
    let mut native = 0;
    let mut metadata = 0;
    for (event, range) in
        pulldown_cmark::Parser::new_ext(text, crate::md_ast::options()).into_offset_iter()
    {
        budget.check()?;
        if let pulldown_cmark::Event::InlineMath(tex) | pulldown_cmark::Event::DisplayMath(tex) =
            event
        {
            equations += 1;
            if equations > 128 {
                return Err("result_budget");
            }
            let tail = text[range.end..].trim_start_matches(char::is_whitespace);
            if tail.starts_with("<!--readmd:math:v1:") {
                let end = tail.find("-->").ok_or("result_budget")? + 3;
                metadata += 1;
                if metadata > 8 || end > 64 * 1024 {
                    return Err("result_budget");
                }
                if let Ok((model, true)) = crate::math_document::read_metadata(&tail[..end], &tex) {
                    // Fresh metadata is native output evidence only. It does
                    // not establish identity with any source expression.
                    let mut b = Budget {
                        deadline: budget.deadline,
                        nodes: 0,
                        bytes: 0,
                    };
                    if let Ok(counts) = scan_xml(model.omml.as_bytes(), None, &mut b) {
                        native += counts.0.get("native_math").copied().unwrap_or(0) as usize;
                    }
                }
            }
        }
    }
    Ok((equations, native))
}

/// Production entry: inspect only the caller's fenced immutable snapshot.
/// Only DOCX/PPTX/XLSX consume bytes; other formats need no second file copy.
pub(crate) fn report_bytes(path: &Path, result: &ConvertTriple, bytes: &[u8]) -> Value {
    let mut report = base(path, result);
    let format = report["format"].as_str().unwrap().to_string();
    let (prefix, main, namespace, root) = match format.as_str() {
        "docx" => ("word/", "word/document.xml", Ns::Word, "document"),
        "pptx" => (
            "ppt/",
            "ppt/presentation.xml",
            Ns::Presentation,
            "presentation",
        ),
        "xlsx" => ("xl/", "xl/workbook.xml", Ns::Sheet, "workbook"),
        _ => {
            if format == "pdf" && !result.text.trim().is_empty() {
                report["findings"].as_array_mut().unwrap().push(finding(
                    "original_layout",
                    1,
                    "review",
                    "info",
                ));
            }
            incomplete(&mut report, "format_not_inspected", 1);
            return report;
        }
    };
    let mut budget = Budget {
        deadline: Instant::now() + Duration::from_millis(250),
        nodes: 0,
        bytes: 0,
    };
    let (members, central) = match directory(bytes, &budget) {
        Ok(v) => v,
        Err(reason) => {
            incomplete(&mut report, reason, 1);
            return report;
        }
    };
    let Some(main_part) = members.iter().find(|m| m.name == main) else {
        incomplete(&mut report, "missing_main_part", 1);
        return report;
    };
    let mut counts = match part(bytes, main_part, central, &mut budget)
        .and_then(|data| scan_xml(&data, Some((namespace, root)), &mut budget))
    {
        Ok(c) => c,
        Err(reason) => {
            incomplete(&mut report, reason, 1);
            return report;
        }
    };
    let candidates = members
        .iter()
        .filter(|m| m.name.starts_with(prefix) && m.name.ends_with(".xml") && m.name != main)
        .collect::<Vec<_>>();
    let mut inspected = 1;
    let mut skipped = 0;
    let mut reason = None;
    for (index, member) in candidates.iter().enumerate() {
        if inspected >= MAX_PARTS || budget.check().is_err() {
            skipped += candidates.len() - index;
            reason = Some(if crate::speech::cancelled() {
                "cancelled"
            } else {
                "budget"
            });
            break;
        }
        match part(bytes, member, central, &mut budget)
            .and_then(|data| scan_xml(&data, None, &mut budget))
        {
            Ok(c) => {
                counts.merge(c);
                inspected += 1;
            }
            Err(error) => {
                skipped += 1;
                reason.get_or_insert(error);
            }
        }
    }
    for member in &members {
        if member.name.starts_with(prefix)
            && ((member.name.contains("/embeddings/") && !member.name.ends_with('/'))
                || member.name.ends_with("/vbaProject.bin")
                || member.name.contains("/diagrams/data") && member.name.ends_with(".xml"))
        {
            counts.add("embedded_objects", 1);
        }
    }
    report["coverage"] = json!({"partsInspected":inspected,"partsSkipped":skipped,"xmlBytes":budget.bytes,"resultInspected":true,"countUnit":"structural_occurrences"});
    report["inspection"] = json!(if skipped == 0 { "complete" } else { "partial" });
    let native_engine =
        result.error.is_none() && !result.text.trim().is_empty() && result.engine == format;
    let mut findings = vec![finding("original_layout", 1, "review", "info")];
    for (code, count) in counts.0 {
        let (status, severity) = if code == "native_math" {
            match math_result(&result.text, &budget) {
                Ok((extracted, native)) => {
                    report["extraction"]["mathExpressions"] = json!(extracted);
                    report["extraction"]["nativeMathModels"] = json!(native);
                    (
                        if extracted > 0 || native > 0 {
                            "extracted"
                        } else {
                            "review"
                        },
                        "info",
                    )
                }
                Err(error) => {
                    report["coverage"]["resultInspected"] = json!(false);
                    report["inspection"] = json!("partial");
                    reason.get_or_insert(error);
                    ("review", "info")
                }
            }
        } else if native_engine
            && matches!(
                code,
                "embedded_objects" | "presentation_effects" | "sheet_layout"
            )
        {
            ("not_preserved", "warning")
        } else {
            ("review", "warning")
        };
        findings.push(finding(code, count, status, severity));
    }
    report["findings"] = json!(findings);
    if let Some(reason) = reason {
        incomplete(&mut report, reason, skipped);
    }
    report
}

/// Source observation failed before a safe snapshot could be supplied.
pub(crate) fn unavailable(path: &Path, result: &ConvertTriple) -> Value {
    let mut report = base(path, result);
    incomplete(&mut report, "source_unavailable", 1);
    report
}

/// Convenience/testing wrapper. Production uses report_bytes and the caller's
/// identity/SHA fence; this function deliberately does not authorize a source.
#[cfg(test)]
pub(crate) fn report(path: &Path, result: &ConvertTriple) -> Value {
    if !matches!(format(path).as_str(), "docx" | "pptx" | "xlsx") {
        return report_bytes(path, result, &[]);
    }
    let read = (|| -> std::io::Result<Vec<u8>> {
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let mut data = Vec::new();
        fs::File::open(path)?
            .take(MAX_INPUT as u64 + 1)
            .read_to_end(&mut data)?;
        Ok(data)
    })();
    match read {
        Ok(bytes) => report_bytes(path, result, &bytes),
        Err(_) => unavailable(path, result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdexport::{write_zip, ZipEntry};

    const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const MATH: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
    const SHEET: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const PRESENTATION: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";

    fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        write_zip(
            &parts
                .iter()
                .map(|(path, data)| ZipEntry {
                    path: (*path).into(),
                    data: data.as_bytes().to_vec(),
                    compress: true,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn word(body: &str) -> String {
        format!(
            r#"<w:document xmlns:w="{WORD}" xmlns:m="{MATH}"><w:body>{body}</w:body></w:document>"#
        )
    }
    fn equation() -> String {
        format!(r#"<m:oMath xmlns:m="{MATH}"><m:r><m:t>x</m:t></m:r></m:oMath>"#)
    }
    fn find<'a>(report: &'a Value, code: &str) -> &'a Value {
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["code"] == code)
            .unwrap_or_else(|| panic!("missing {code}: {report}"))
    }
    fn absent(report: &Value, code: &str) {
        assert!(
            !report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["code"] == code),
            "{report}"
        );
    }
    fn office(format: &str, bytes: &[u8], text: &str) -> Value {
        // This path deliberately does not exist: production reads only bytes.
        report_bytes(
            Path::new(&format!("/not-read/private-source.{format}")),
            &ConvertTriple::ok(text, format),
            bytes,
        )
    }
    fn central_at(bytes: &[u8], member: &str) -> usize {
        let mut at = u32_at(bytes, bytes.len() - 22 + 16).unwrap();
        while bytes[at..].starts_with(b"PK\x01\x02") {
            let n = u16_at(bytes, at + 28).unwrap() as usize;
            if &bytes[at + 46..at + 46 + n] == member.as_bytes() {
                return at;
            }
            at += 46
                + n
                + u16_at(bytes, at + 30).unwrap() as usize
                + u16_at(bytes, at + 32).unwrap() as usize;
        }
        panic!("missing test member");
    }

    #[test]
    fn snapshot_scan_is_structural_and_does_not_invent_losses() {
        let main = word("<w:p><w:r><w:t>Only plain text</w:t></w:r></w:p>");
        let bytes = package(&[("word/document.xml", &main)]);
        let r = office("docx", &bytes, "Only plain text");
        assert_eq!(r["inspection"], "complete");
        assert_eq!(r["scope"], "structural");
        assert_eq!(r["renderedComparison"], false);
        assert_eq!(r["coverage"]["partsInspected"], 1);
        assert_eq!(r["findings"].as_array().unwrap().len(), 1);
        assert_eq!(find(&r, "original_layout")["status"], "review");
        assert!(r["notChecked"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "visual_rendering"));
        assert_eq!(r["limits"]["inputBytes"], 32 * 1024 * 1024);
    }

    #[test]
    fn word_counts_real_namespaces_and_review_history_without_claiming_body_loss() {
        let main = word(
            r#"<w:p><w:ins><w:r><w:t>kept</w:t></w:r></w:ins><w:del><w:r><w:delText>past</w:delText></w:r></w:del><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:txbxContent/></wp:anchor></w:drawing><w:txbxContent/></w:p>"#,
        );
        let header = format!(
            r#"<w:hdr xmlns:w="{WORD}"><w:p><w:r><w:t><![CDATA[Heading]]></w:t></w:r></w:p></w:hdr>"#
        );
        let notes = format!(
            r#"<w:footnotes xmlns:w="{WORD}"><w:footnote><w:p><w:r><w:t>Kept note</w:t></w:r></w:p></w:footnote></w:footnotes>"#
        );
        let bytes = package(&[
            ("word/document.xml", &main),
            ("word/header1.xml", &header),
            ("word/footnotes.xml", &notes),
        ]);
        let r = office("docx", &bytes, "kept\n\nKept note");
        assert_eq!(find(&r, "review_markup")["count"], 2);
        assert_eq!(find(&r, "review_markup")["status"], "review");
        assert_eq!(find(&r, "floating_objects")["count"], 2);
        assert_eq!(find(&r, "header_footer")["status"], "review");
        assert_eq!(r["inspection"], "complete");
        assert_eq!(r["coverage"]["partsInspected"], 3);
    }

    #[test]
    fn native_omml_payload_counts_never_claim_source_expression_equivalence() {
        let math = equation();
        let main = word(&format!("<w:p>{math}</w:p>"));
        let bytes = package(&[("word/document.xml", &main)]);
        let meta = crate::math_document::metadata("x", &math, "eq-test").unwrap();
        let kept = office("docx", &bytes, &format!("$x${meta}"));
        assert_eq!(find(&kept, "native_math")["status"], "extracted", "{kept}");
        assert_eq!(kept["extraction"]["nativeMathModels"], 1);
        let different =
            crate::math_document::metadata("x", &math.replace(">x<", ">z<"), "eq-other").unwrap();
        let unmatched = office("docx", &bytes, &format!("$x${different}"));
        assert_eq!(unmatched["extraction"]["nativeMathModels"], 1);
        assert_eq!(find(&unmatched, "native_math")["status"], "extracted");
        let plain = office("docx", &bytes, "$x$");
        assert_eq!(find(&plain, "native_math")["status"], "extracted");
        let stale = office("docx", &bytes, &format!("$y${meta}"));
        assert_eq!(find(&stale, "native_math")["status"], "extracted");
        let literal = office("docx", &bytes, &format!("```\n$x${meta}\n```"));
        assert_eq!(find(&literal, "native_math")["status"], "review");
        assert_ne!(find(&literal, "native_math")["status"], "not_preserved");
    }

    #[test]
    fn spreadsheet_distinguishes_missing_results_from_zero_and_cached_empty_strings() {
        let workbook = format!(
            r#"<s:workbook xmlns:s="{SHEET}"><s:sheets><s:sheet state="hidden"/></s:sheets></s:workbook>"#
        );
        let sheet = format!(
            r#"<s:worksheet xmlns:s="{SHEET}" xmlns:q="urn:unrelated"><s:cols><s:col hidden="true" width="10"/></s:cols><s:sheetData><s:row hidden="1" ht="20"><s:c><s:f>1+1</s:f><s:v>2</s:v></s:c><s:c><s:f>3+1</s:f></s:c><s:c><s:f>1-1</s:f><s:v>0</s:v></s:c><s:c><s:f t="shared" si="0"/></s:c><s:c><s:f>4+1</s:f><s:v/></s:c><s:c t="str"><s:f>IF(1,"",0)</s:f><s:v/></s:c></s:row><s:row q:hidden="1"/></s:sheetData><s:mergeCells><s:mergeCell ref="A1:B1"/></s:mergeCells><s:conditionalFormatting/><s:dataValidation/><s:pageSetup/><s:autoFilter/></s:worksheet>"#
        );
        let bytes = package(&[
            ("xl/workbook.xml", &workbook),
            ("xl/worksheets/sheet1.xml", &sheet),
        ]);
        let r = office("xlsx", &bytes, "| 2 | `=3+1` | 0 | | `=4+1` | | ");
        assert_eq!(find(&r, "spreadsheet_formulas")["count"], 6);
        assert_eq!(find(&r, "formula_cache_missing")["count"], 3);
        assert_eq!(find(&r, "formula_cache_missing")["status"], "review");
        assert_eq!(find(&r, "hidden_content")["count"], 3);
        assert_eq!(find(&r, "hidden_content")["status"], "review");
        assert_eq!(find(&r, "sheet_layout")["count"], 7);
        assert_eq!(find(&r, "sheet_layout")["status"], "not_preserved");
    }

    #[test]
    fn presentation_effects_and_objects_are_engine_specific_not_blanket_chart_loss() {
        let main = format!(r#"<p:presentation xmlns:p="{PRESENTATION}"/>"#);
        let slide =
            format!(r#"<p:sld xmlns:p="{PRESENTATION}"><p:timing/><p:transition/></p:sld>"#);
        let chart = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart/></c:chartSpace>"#;
        let bytes = package(&[
            ("ppt/presentation.xml", &main),
            ("ppt/slides/slide1.xml", &slide),
            ("ppt/charts/chart1.xml", chart),
            ("ppt/embeddings/oleObject1.bin", "opaque input"),
        ]);
        let r = office("pptx", &bytes, "Slide\n\n| Cached chart values |");
        assert_eq!(find(&r, "presentation_effects")["count"], 2);
        assert_eq!(find(&r, "presentation_effects")["status"], "not_preserved");
        assert_eq!(find(&r, "embedded_objects")["status"], "not_preserved");
        assert_eq!(find(&r, "charts")["status"], "review");
        let fallback = report_bytes(
            Path::new("doc.pptx"),
            &ConvertTriple::ok("fallback", "markitdown"),
            &bytes,
        );
        assert_eq!(find(&fallback, "presentation_effects")["status"], "review");
        assert_eq!(find(&fallback, "embedded_objects")["status"], "review");
    }

    #[test]
    fn literal_markup_and_foreign_namespaces_are_not_source_features() {
        let main = word(
            r#"<!-- <w:ins/><m:oMath/> --><w:p xmlns:q="urn:unrelated"><q:ins/><q:oMath/><w:r><w:t><![CDATA[<w:del/><m:oMath/>]]></w:t></w:r></w:p>"#,
        );
        let r = office(
            "docx",
            &package(&[("word/document.xml", &main)]),
            "Literal markup",
        );
        assert_eq!(r["inspection"], "complete");
        absent(&r, "review_markup");
        absent(&r, "native_math");
        let spoof = package(&[("word/document.xml", r#"<w:document xmlns:w="urn:fake"/>"#)]);
        assert_eq!(office("docx", &spoof, "text")["inspection"], "unavailable");
    }

    #[test]
    fn partial_scans_keep_valid_findings_and_bound_oversized_parts_before_inflating() {
        let main = word("<w:ins/>");
        let extra = format!(r#"<w:comments xmlns:w="{WORD}"><w:comment/></w:comments>"#);
        let mut bytes = package(&[("word/document.xml", &main), ("word/comments.xml", &extra)]);
        let at = central_at(&bytes, "word/comments.xml");
        bytes[at + 24..at + 28].copy_from_slice(&((MAX_PART + 1) as u32).to_le_bytes());
        let r = office("docx", &bytes, "text");
        assert_eq!(r["inspection"], "partial");
        assert_eq!(r["reason"], "budget");
        assert_eq!(r["coverage"]["partsInspected"], 1);
        assert_eq!(r["coverage"]["partsSkipped"], 1);
        assert_eq!(find(&r, "review_markup")["count"], 1);
        assert_eq!(find(&r, "inspection_incomplete")["status"], "not_checked");
        assert!(r["coverage"]["xmlBytes"].as_u64().unwrap() < MAX_PART as u64);
    }

    #[test]
    fn invalid_archives_and_xml_are_not_reported_as_complete() {
        let main = word("<w:p/>");
        let bytes = package(&[("word/document.xml", &main)]);
        let mut bad_crc = bytes.clone();
        let at = central_at(&bad_crc, "word/document.xml");
        bad_crc[at + 16] ^= 1;
        assert_eq!(
            office("docx", &bad_crc, "text")["reason"],
            "invalid_archive"
        );
        let mut too_many = bytes.clone();
        let end = too_many.len() - 22;
        let n = ((MAX_ENTRIES + 1) as u16).to_le_bytes();
        too_many[end + 8..end + 10].copy_from_slice(&n);
        too_many[end + 10..end + 12].copy_from_slice(&n);
        assert_eq!(office("docx", &too_many, "text")["reason"], "budget");
        let duplicate = package(&[("word/document.xml", &main), ("word/document.xml", &main)]);
        assert_eq!(
            office("docx", &duplicate, "text")["reason"],
            "invalid_archive"
        );
        let dtd = format!(
            r#"<!DOCTYPE document [<!ENTITY x "untrusted">]><w:document xmlns:w="{WORD}"><w:body>&x;</w:body></w:document>"#
        );
        let r = office("docx", &package(&[("word/document.xml", &dtd)]), "text");
        assert_eq!(r["inspection"], "unavailable");
        assert_eq!(r["reason"], "invalid_xml");
        absent(&r, "original_layout");
    }

    #[test]
    fn unavailable_formats_and_extraction_outcomes_do_not_leak_source_or_errors() {
        let path = Path::new("/private/SECRET-NAME.txt");
        let mut result = ConvertTriple::ok("SECRET-CONTENT", "private engine /SECRET");
        result.error = Some("SECRET-ERROR".into());
        let r = report_bytes(path, &result, b"not opened or interpreted");
        assert_eq!(r["inspection"], "unavailable");
        assert_eq!(r["engine"], "unknown");
        assert_eq!(r["extraction"]["status"], "partial");
        assert!(!r.to_string().contains("SECRET"));
        result.text.clear();
        assert_eq!(
            report_bytes(path, &result, &[])["extraction"]["status"],
            "failed"
        );
        result.error = None;
        assert_eq!(
            report_bytes(path, &result, &[])["extraction"]["status"],
            "empty"
        );
        let missing = report(Path::new("/not-present/source.docx"), &result);
        assert_eq!(missing["reason"], "source_unavailable");
        assert_eq!(missing["inspection"], "unavailable");
        let pdf = report_bytes(
            Path::new("source.pdf"),
            &ConvertTriple::ok("page text", "pdfium"),
            b"ignored",
        );
        assert_eq!(pdf["inspection"], "unavailable");
        assert_eq!(find(&pdf, "original_layout")["status"], "review");
    }

    #[test]
    fn result_math_budget_retains_honest_source_findings() {
        let main = word(&equation());
        let r = office(
            "docx",
            &package(&[("word/document.xml", &main)]),
            &"x".repeat(MAX_RESULT + 1),
        );
        assert_eq!(r["inspection"], "partial");
        assert_eq!(r["reason"], "result_budget");
        assert_eq!(r["coverage"]["resultInspected"], false);
        assert_eq!(find(&r, "native_math")["status"], "review");
    }
}
