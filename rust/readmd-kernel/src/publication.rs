//! Read-only publication preparation shared by preview, exporters and MCP.
//! Source citations stay in the Markdown file. CSL output is an ephemeral view,
//! so changing a style never rewrites the author's prose or creates side files.
use crate::{
    academic::{self, CiteMode, Request},
    ApiError, ApiResult,
};
use pulldown_cmark::{Event, Options, Parser};
use serde_json::{json, Value};
use std::{fs, path::Path};
use crate::publication_rich::{Projection, ProjectedCitation, RichBibliography, RichBibliographyItem, SourceSpan};

#[path = "publication_citations.rs"]
pub mod citations;

pub struct Prepared {
    pub content: String,
    pub data: Value,
    pub warnings: Vec<String>,
    pub projection: Projection,
}
fn invalid(code: &'static str) -> ApiError {
    ApiError::new(422, code)
}
fn read_resource(
    root: &Path,
    name: &str,
    extension: &[&str],
    budget: &mut usize,
) -> ApiResult<String> {
    if name.len() > 1024 {
        return Err(invalid("academic_resource"));
    }
    let path = root
        .join(name)
        .canonicalize()
        .map_err(|_| invalid("academic_resource"))?;
    if !path.starts_with(root)
        || !path.is_file()
        || !path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            extension
                .iter()
                .any(|allowed| e.eq_ignore_ascii_case(allowed))
        })
    {
        return Err(invalid("academic_resource"));
    }
    let size = fs::metadata(&path)
        .map_err(|_| invalid("academic_resource"))?
        .len();
    if size > 2 * 1024 * 1024 || budget.saturating_add(size as usize) > 2 * 1024 * 1024 {
        return Err(ApiError::new(413, "academic_budget"));
    }
    // Bounded even if another process grows a file after metadata was read.
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|_| invalid("academic_resource"))?
        .take((2 * 1024 * 1024 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("academic_resource"))?;
    *budget = budget.saturating_add(bytes.len());
    if *budget > 2 * 1024 * 1024 {
        return Err(ApiError::new(413, "academic_budget"));
    }
    Ok(crate::text_encoding::detect_and_decode(&bytes).0)
}

fn embedded_resources(source: &str) -> ApiResult<Option<crate::publication_docx_citations::Resources>> {
    crate::publication_docx_citations::Resources::from_markdown(source)
        .map_err(|_|invalid(crate::publication_docx_citations::RESOURCE_INVALID))
}
fn read_citation_resource(root: Option<&Path>, embedded: Option<&crate::publication_docx_citations::Resources>, name: &str, extensions: &[&str], budget: &mut usize) -> ApiResult<String> {
    if let Some(resources) = embedded {
        if !Path::new(name).extension().and_then(|e|e.to_str()).is_some_and(|e|extensions.iter().any(|allowed|e.eq_ignore_ascii_case(allowed))) {
            return Err(invalid("academic_resource"));
        }
        // An embedded document is self-contained. A missing or renamed resource
        // never grants access to an old path on the importing computer.
        let content = resources.read(name).ok_or_else(||invalid("academic_resource"))?;
        *budget = budget.saturating_add(content.len());
        if *budget > crate::publication_docx_citations::MAX_RESOURCES { return Err(ApiError::new(413,"academic_budget")); }
        Ok(content.into())
    } else {
        read_resource(root.ok_or_else(||invalid("academic_resource"))?,name,extensions,budget)
    }
}

fn markdown_text(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c == '[' { out.push_str("&#91;"); continue; }
        if c == ']' { out.push_str("&#93;"); continue; }
        if "\\`*_{}<>!#|$".contains(c) {
            out.push('\\');
        }
        if c == '\n' || c == '\r' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn front_document(source: &str) -> crate::md_ast::Document {
    let beginning = source.strip_prefix('\u{feff}').unwrap_or(source);
    let front = if beginning.starts_with("---\n") || beginning.starts_with("---\r\n") {
        let mut offset = beginning.lines().next().unwrap().len() + 1;
        let mut end = 0;
        for line in beginning[offset..].split_inclusive('\n') {
            offset += line.len();
            if line.trim_end_matches(['\r', '\n']) == "---" {
                end = offset;
                break;
            }
            if offset > 64 * 1024 {
                break;
            }
        }
        &beginning[..end]
    } else {
        ""
    };
    crate::md_ast::parse(front)
}

/// Preview keys include resources, not merely the unchanged Markdown text.
/// Editing a .bib or .csl file can therefore never reuse a stale export artifact.
pub fn resource_revision(source: &str, base: &str) -> ApiResult<String> {
    resource_revision_with_options(source,base,&Value::Null)
}
pub fn resource_revision_with_options(source: &str, base: &str, options: &Value) -> ApiResult<String> {
    let doc = front_document(source);
    let embedded = embedded_resources(source)?;
    let mut names = if let Some(resources)=&embedded {resources.bibliography.clone()} else {crate::publication_metadata::bibliography_sources(source).map_err(|e|ApiError::new(422,e.code))?};
    if names.is_empty() {
        return Ok(String::new());
    }
    if names.len() > 8 {
        return Err(ApiError::new(413, "academic_budget"));
    }
    let style_override=options.pointer("/citations/style").map(|value|value.as_str().filter(|value|!value.is_empty()&&value.len()<=1024).ok_or_else(||invalid("academic_style"))).transpose()?;
    if let Some(csl) = style_override.map(str::to_string).or_else(||doc.meta("csl")).or_else(||embedded.as_ref().map(|r|r.style.clone()))
        .filter(|s| s.to_ascii_lowercase().ends_with(".csl"))
    {
        names.push(csl);
    }
    let root = if embedded.is_none() {Some(Path::new(base).canonicalize().map_err(|_|invalid("academic_resource"))?)} else {None};
    let mut budget = 0;
    let mut digest = sha2::Sha256::default();
    use sha2::Digest;
    if let Some(xml) = citation_locale_xml(options)?.or_else(||embedded.as_ref().and_then(|r|r.locale_xml.as_deref())) {
        digest.update(b"ReadMD CSL locale XML\0");
        digest.update((xml.len() as u64).to_le_bytes());
        digest.update(xml.as_bytes());
    }
    for name in names {
        let content = read_citation_resource(root.as_deref(), embedded.as_ref(), &name, &["bib", "json", "csl"], &mut budget)?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((content.len() as u64).to_le_bytes());
        digest.update(content.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn prepare(source: &str, base: &str) -> ApiResult<Prepared> {
    prepare_with_options(source, base, &Value::Null)
}

fn citation_locale_xml(options: &Value) -> ApiResult<Option<&str>> {
    let xml=options.pointer("/citations/locale_xml").filter(|value| !value.is_null())
        .map(|value| value.as_str().ok_or_else(|| ApiError::new(422,"academic_context").noted("field","options.citations.locale_xml"))).transpose()?;
    if xml.is_some_and(|xml| xml.len()>256*1024) { return Err(ApiError::new(413,"academic_budget")); }
    Ok(xml)
}

/// Preview/export metadata participates in citation localization without
/// rewriting the source declaration or borrowing the application UI language.
pub fn prepare_with_options(source: &str, base: &str, options: &Value) -> ApiResult<Prepared> {
    crate::publication_typography::validate_input(source, options).map_err(|code| ApiError::new(422, code))?;
    let source_sections=crate::publication_sections::analyze(source).map_err(|code|ApiError::new(422,code))?;
    if !source_sections.is_empty(){
        crate::publication_sections::resolve(&crate::md_ast::parse(source),&crate::export_styles::sanitize(options)).map_err(|code|ApiError::new(422,code))?;
    }
    let embedded = embedded_resources(source)?;
    let language = crate::publication_metadata::from_source(source, options, "").map_err(|e|ApiError::new(422,e.code))?.language
        .or_else(||embedded.as_ref().map(|r|r.locale.clone()));
    let style = options.pointer("/citations/style").map(|value| value.as_str().filter(|value| !value.is_empty() && value.len() <= 1024).ok_or_else(|| invalid("academic_style"))).transpose()?;
    let locale_xml = citation_locale_xml(options)?;
    if let Some(xml) = locale_xml { academic::custom_locale(xml, language.as_deref().unwrap_or("en-US"))
        .map_err(|error|error.noted("field","options.citations.locale_xml"))?; }
    let mut prepared=prepare_with_citation_context(source, base, language.as_deref(), style, locale_xml)?;
    if !source_sections.is_empty(){prepared.data["active"]=json!(true);prepared.data["source_sections"]=json!(source_sections);}
    Ok(prepared)
}

pub(crate) fn prepare_with_language(source: &str, base: &str, language: Option<&str>) -> ApiResult<Prepared> {
    prepare_with_language_and_style(source, base, language, None)
}

pub(crate) fn prepare_with_language_and_style(source: &str, base: &str, language: Option<&str>, style: Option<&str>) -> ApiResult<Prepared> {
    prepare_with_citation_context(source, base, language, style, None)
}

/// Only actual Markdown/HTML comments can carry import diagnostics. The
/// annotation is evidence for review, never an instruction to restore content.
fn numbering_import_diagnostics(source: &str) -> Vec<Value> {
    const FAMILY: &str = "<!--readmd-numbering-import:";
    const HEADER: &str = "<!--readmd-numbering-import:v1 ";
    const MAX_BYTES: usize = 256 * 1024;
    const MAX_DIAGNOSTICS: usize = 4096;
    fn text(value: &Value, limit: usize) -> bool {
        value.as_str().is_some_and(|text| text.len() <= limit && !text.chars().any(char::is_control))
    }
    fn valid(value: &Value) -> bool {
        let Some(object) = value.as_object() else { return false };
        object.keys().all(|key| matches!(key.as_str(), "code" | "reason" | "detail" | "paragraph" | "kind" | "source_id" | "visible_content_preserved"))
            && matches!(value["code"].as_str(), Some("docx_numbering_import_conflict" | "docx_numbering_invalid" | "docx_numbering_unrepresentable" | "docx_numbering_budget"))
            && text(&value["reason"], 128) && value["reason"].as_str().is_some_and(|reason| !reason.is_empty())
            && value["visible_content_preserved"].as_bool() == Some(true)
            && object.get("detail").is_none_or(|value| text(value, 8192))
            && object.get("source_id").is_none_or(|value| text(value, 1024))
            && object.get("kind").is_none_or(|value| matches!(value.as_str(), Some("figure" | "table" | "equation")))
            && object.get("paragraph").is_none_or(|value| value.as_u64().is_some_and(|n| n <= u32::MAX as u64))
    }
    let mut diagnostics = Vec::new();
    let mut bytes = 0usize;
    for (index, range) in crate::publication_docx_citations::source_comments(source, FAMILY, 9).into_iter().enumerate() {
        let annotation = &source[range.clone()];
        let from = source[..range.start].encode_utf16().count();
        let to = source[..range.end].encode_utf16().count();
        bytes = bytes.saturating_add(annotation.len());
        let budget = index >= 8 || bytes > MAX_BYTES;
        let values = (!budget).then(|| annotation.strip_prefix(HEADER).and_then(|raw| raw.strip_suffix("-->")))
            .flatten().filter(|raw| !raw.contains("<!--") && !raw.contains("-->"))
            .and_then(|raw| serde_json::from_str::<Vec<Value>>(raw).ok())
            .filter(|values| diagnostics.len().saturating_add(values.len()) <= MAX_DIAGNOSTICS && values.iter().all(valid));
        if let Some(values) = values {
            for mut value in values {
                value["from"] = json!(from); value["to"] = json!(to);
                value["path"] = json!("source.readmd-numbering-import");
                value["key"] = value.get("source_id").cloned().unwrap_or_else(|| value["reason"].clone());
                diagnostics.push(value);
            }
        } else {
            diagnostics.push(json!({"code":if budget {"docx_numbering_budget"} else {"docx_numbering_invalid"},
                "reason":if budget {"numbering_import_annotation_budget"} else {"numbering_import_annotation_invalid"},
                "visible_content_preserved":true,"source_preserved":true,"from":from,"to":to,
                "key":"readmd-numbering-import","path":"source.readmd-numbering-import"}));
        }
        if budget { break; }
    }
    diagnostics
}

fn prepare_with_citation_context(source: &str, base: &str, language: Option<&str>, style: Option<&str>, locale_xml: Option<&str>) -> ApiResult<Prepared> {
    crate::publication_inline_text::validate_source(source).map_err(|code|ApiError::new(422,code))?;
    crate::cross_references::validate_source(source).map_err(|code|ApiError::new(422,code))?;
    crate::publication_typography::validate_input(source, &Value::Null).map_err(|code| ApiError::new(422, code))?;
    crate::publication_navigation::from_source(source,&Value::Null).map_err(|e|e.api_error())?;
    crate::publication_sections::validate_source(source).map_err(|code| ApiError::new(422, code))?;
    let mut prepared = prepare_citations(source, base, language, style, locale_xml)?;
    let mut source_diagnostics = crate::publication_profile::source_diagnostics(source).into_iter().map(|diagnostic| {
        json!({"code":diagnostic.code,"key":diagnostic.path,"path":diagnostic.path,
            "from":diagnostic.from,"to":diagnostic.to,"visible_content_preserved":true,"source_preserved":true})
    }).collect::<Vec<_>>();
    source_diagnostics.extend(numbering_import_diagnostics(source));
    if !source_diagnostics.is_empty() {
        prepared.data["active"] = json!(true);
        let revision = crate::document_transaction::revision(source);
        for mut diagnostic in source_diagnostics {
            diagnostic["revision"] = json!(revision);
            prepared.warnings.push(format!("{}: {}", diagnostic["code"].as_str().unwrap_or("publication_metadata_field"), diagnostic["key"].as_str().unwrap_or("")));
            prepared.data["diagnostics"].as_array_mut().expect("diagnostics").push(diagnostic);
        }
    }
    let imported=crate::publication_docx_citations::import_diagnostics(source);
    if !imported.is_empty(){
        prepared.data["active"]=json!(true);
        let revision=crate::document_transaction::revision(source);
        for mut diagnostic in imported{
            diagnostic["revision"]=json!(revision);
            prepared.warnings.push(diagnostic["code"].as_str().unwrap_or(crate::publication_docx_citations::INVALID).into());
            prepared.data["diagnostics"].as_array_mut().expect("diagnostics").push(diagnostic);
        }
    }
    let versions = crate::math_dsl::version_diagnostics(source);
    if !versions.is_empty() {
        prepared.data["active"] = json!(true);
        prepared.warnings.extend(versions.iter().map(|d| format!("math_unsupported_version: {}", d["key"].as_str().unwrap_or(""))));
        prepared.data["diagnostics"].as_array_mut().expect("diagnostics").extend(versions);
    }
    let references = crate::cross_references::project(source);
    if let Some(plan)=&references.numbering {
        prepared.data["active"]=json!(true);
        prepared.data["source_numbering"]=json!(plan);
    }
    prepared.content = crate::cross_references::project(&prepared.content).content;
    if !references.labels.is_empty() || !references.diagnostics.is_empty() {
        prepared.data["active"] = json!(true);
        prepared.data["source_labels"] = json!(references.labels);
        let diagnostics = prepared.data["diagnostics"].as_array_mut().expect("diagnostics");
        prepared.warnings.extend(references.diagnostics.iter().map(|d| format!("{}: {}", d["code"].as_str().unwrap_or("academic_diagnostic"), d["key"].as_str().unwrap_or(""))));
        diagnostics.extend(references.diagnostics);
    }
    Ok(prepared)
}

fn prepare_citations(source: &str, base: &str, language: Option<&str>, style_override: Option<&str>, locale_xml: Option<&str>) -> ApiResult<Prepared> {
    if source.len() > crate::document_transaction::MAX_DOCUMENT_BYTES {
        return Err(ApiError::new(413, "academic_budget"));
    }
    let doc = front_document(source);
    let embedded = embedded_resources(source)?;
    let names = if let Some(resources)=&embedded {resources.bibliography.clone()} else {crate::publication_metadata::bibliography_sources(source).map_err(|e|ApiError::new(422,e.code))?};
    if names.is_empty() {
        return Ok(Prepared {
            content: source.into(),
            data: json!({"ok":true,"active":false,"citations":[],"diagnostics":[]}),
            warnings: vec![],
            projection: Projection::new(source),
        });
    }
    if names.len() > 8 {
        return Err(ApiError::new(413, "academic_budget"));
    }
    let root = if embedded.is_none() {Some(Path::new(base).canonicalize().map_err(|_|invalid("academic_resource"))?)} else {None};
    let json_format = names[0].to_ascii_lowercase().ends_with(".json");
    let mut data = String::new();
    let mut json_items = Vec::<Value>::new();
    let mut budget = 0;
    for name in names {
        if name.to_ascii_lowercase().ends_with(".json") != json_format {
            return Err(invalid("academic_mixed_bibliography"));
        }
        let text = read_citation_resource(
            root.as_deref(),embedded.as_ref(),
            &name,
            if json_format { &["json"] } else { &["bib"] },
            &mut budget,
        )?;
        if json_format {
            let items: Vec<Value> =
                serde_json::from_str(&text).map_err(|_| invalid("academic_bibliography"))?;
            json_items.extend(items);
        } else {
            data.push_str(&text);
            data.push('\n');
        }
    }
    if json_format {
        data = serde_json::to_string(&json_items).map_err(|_| invalid("academic_bibliography"))?;
    }
    let requested = style_override.map(str::to_string).or_else(||doc.meta("csl")).or_else(||embedded.as_ref().map(|r|r.style.clone())).unwrap_or_else(|| "american-psychological-association".into());
    let (style, csl) = if requested.to_ascii_lowercase().ends_with(".csl") {
        (
            "american-psychological-association".into(),
            Some(read_citation_resource(root.as_deref(),embedded.as_ref(), &requested, &["csl"], &mut budget)?),
        )
    } else {
        (requested, None)
    };
    let language = language.or_else(||embedded.as_ref().map(|r|r.locale.as_str()));
    let locale_xml = locale_xml.or_else(||embedded.as_ref().and_then(|r|r.locale_xml.as_deref()));
    let manual_bibliography = embedded.as_ref().is_some_and(|r|r.manual_bibliography);
    let format=if json_format { "csl-json" } else { "bibtex" };
    let known=academic::reference_keys(format,&data)?;
    let note_style=academic::is_note_style(&style,csl.as_deref())?;
    let scan=citations::scan(source,language.unwrap_or("en-US"))?;
    let spans=citations::reading_order(source,scan.citations,note_style,&known)?;
    let request = Request {
        format: if json_format { "csl-json" } else { "bibtex" }.into(),
        data,
        style,
        csl,
        locale: language.unwrap_or("en-US").into(),
        locale_xml: locale_xml.map(str::to_string),
        clusters: spans.iter().map(|span| span.items.iter().map(|item|item.cite.clone()).collect()).collect(),
        preceding_notes: Vec::new(),
        contexts: spans.iter().map(|span|span.context.clone()).collect(),
    };
    let mut rendered = academic::citations(&request)?;
    let mut projection = Projection::new(source);
    let revision = projection.revision.clone();
    let mut positions = Vec::new();
    let mut edits = Vec::new();
    let mut notes = String::new();
    for (index, span) in spans.iter().enumerate() {
        let result = &rendered["citations"][index];
        let complete = result["complete"].as_bool() == Some(true);
        let rich = serde_json::from_value(result["rich"].clone()).map_err(|_| invalid("academic_rich_unsupported"))?;
        let inline_rich = serde_json::from_value(result.get("inline_rich").cloned().unwrap_or_else(|| json!([]))).map_err(|_| invalid("academic_rich_unsupported"))?;
        let note_rich = serde_json::from_value(result.get("note_rich").cloned().unwrap_or_else(|| json!([]))).map_err(|_| invalid("academic_rich_unsupported"))?;
        let note_label = if complete && note_style && !span.context.in_note && span.items.iter().any(|item| item.cite.mode != CiteMode::AuthorOnly) {
            let mut label = format!("readmd-citation-{}", index + 1);
            while source.contains(&format!("[^{label}]")) { label.push('x'); }
            Some(label)
        } else { None };
        let projected = ProjectedCitation { source: span.clone(), rich, inline_rich, note_rich, note_label, rendered: complete };
        let replacement = if !complete {
            span.span.source.clone()
        } else if let Some(label) = &projected.note_label {
            if projected.note_rich.is_empty() {
                notes.push_str(&format!("\n\n[^{label}]: {}\n", result["markdown"].as_str().unwrap_or("")));
                format!("[^{label}]")
            } else {
                // The typed tree is authoritative; this field remains a
                // compatibility view for callers which still request Markdown.
                notes.push_str(&format!("\n\n[^{label}]: {}\n", crate::publication_rich::html(&projected.note_rich)?));
                format!("{}[^{label}]", crate::publication_rich::html(&projected.inline_rich)?)
            }
        } else {
            result["markdown"].as_str().unwrap_or(&span.span.source).to_string()
        };
        edits.push((span.span.byte_from..span.span.byte_to, replacement));
        positions.push(json!({"from":span.span.from,"to":span.span.to,"source":span.span.source,"rendered":result,"keys":span.items.iter().map(|item|&item.cite.key).collect::<Vec<_>>(),"items":span.items,"context":span.context,"revision":revision}));
        projection.citations.push(projected);
    }
    // Reading order differs from source order when a cited footnote is defined
    // later. Apply source edits independently of driver order.
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (range, replacement) in edits {
        if range.start < cursor { return Err(invalid("academic_source_overlap")); }
        output.push_str(&source[cursor..range.start]);
        output.push_str(&replacement);
        cursor = range.end;
    }
    output.push_str(&source[cursor..]);
    output.push_str(&notes);
    let mut bibliography = String::new();
    if let Some(items) = rendered["bibliography"]["items"]
        .as_array()
        .filter(|items| !items.is_empty() && !manual_bibliography)
    {
        let title = doc.meta("reference-section-title").unwrap_or_else(|| {
            if request.locale.starts_with("zh") {
                "参考文献".into()
            } else {
                "References".into()
            }
        });
        bibliography.push_str(&format!("\n\n## {}\n\n", markdown_text(&title)));
        let mut rich_items = Vec::new();
        for item in items {
            if let Some(label) = item["first_field"].as_str() {
                bibliography.push_str(&markdown_text(label));
                bibliography.push(' ');
            }
            bibliography.push_str(item["markdown"].as_str().unwrap_or(""));
            bibliography.push_str("\n\n");
            rich_items.push(RichBibliographyItem {
                key: item["key"].as_str().unwrap_or("").into(),
                rich: serde_json::from_value(item["rich"].clone()).map_err(|_| invalid("academic_rich_unsupported"))?,
                first_field: serde_json::from_value(item["first_field_rich"].clone()).map_err(|_| invalid("academic_rich_unsupported"))?,
            });
        }
        let source_span = Parser::new_ext(source, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
            .into_offset_iter().find_map(|(event, range)| match event {
                Event::Html(text) | Event::InlineHtml(text) if text.trim() == "<!--readmd:bibliography-->" => Some(SourceSpan::new(source, range)),
                _ => None,
            }).transpose()?;
        let mut rich_bibliography = RichBibliography {
            title, items: rich_items,
            hanging_indent: rendered["bibliography"]["hanging_indent"].as_bool().unwrap_or(false),
            line_spacing: rendered["bibliography"]["line_spacing"].as_u64().unwrap_or(1) as u32,
            entry_spacing: rendered["bibliography"]["entry_spacing"].as_u64().unwrap_or(0) as u32,
            source_span,
        };
        rich_bibliography.normalize_layout()?;
        projection.bibliography = Some(rich_bibliography);
    }
    let marker = "<!--readmd:bibliography-->";
    // Only replace a real HTML marker outside fenced code/examples.
    let markers = Parser::new_ext(&output, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Html(s) | Event::InlineHtml(s) if s.trim() == marker => Some(range),
            _ => None,
        })
        .collect::<Vec<_>>();
    if manual_bibliography {
        // The imported bibliography has author edits and remains ordinary
        // document content. Citation preparation may not append another copy.
    } else if let Some(range) = markers.first() {
        output.replace_range(range.clone(), &bibliography);
    } else {
        output.push_str(&bibliography);
    }
    let diagnostics = rendered["diagnostics"].as_array_mut().expect("diagnostics");
    for diagnostic in diagnostics.iter_mut() {
        let cluster = diagnostic["cluster"].as_u64().and_then(|index| spans.get(index as usize));
        let locations = if let Some(cluster) = cluster {
            let span = diagnostic["item"].as_u64().and_then(|index| cluster.items.get(index as usize))
                .and_then(|item| item.fields.get("key")).unwrap_or(&cluster.span);
            vec![span.clone()]
        } else {
            spans.iter().flat_map(|cluster| &cluster.items)
                .filter(|item| diagnostic["key"].as_str() == Some(item.cite.key.as_str()))
                .filter_map(|item| item.fields.get("key").cloned()).collect()
        };
        if let Some(first) = locations.first() { diagnostic["from"] = json!(first.from); diagnostic["to"] = json!(first.to); }
        diagnostic["locations"] = json!(locations);
        diagnostic["revision"] = json!(revision);
    }
    diagnostics.extend(scan.diagnostics.into_iter().map(|mut diagnostic| { diagnostic["revision"] = json!(revision); diagnostic }));
    let warnings: Vec<String> = rendered["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d["code"].as_str().unwrap_or("academic_diagnostic"),
                d["key"].as_str().unwrap_or("")
            )
        })
        .collect();
    rendered["active"] = json!(true);
    if manual_bibliography {rendered["manual_bibliography"]=json!(true);}
    rendered["revision"] = json!(revision);
    rendered["source_citations"] = json!(positions);
    Ok(Prepared {
        content: output,
        data: rendered,
        warnings,
        projection,
    })
}

#[cfg(test)]
#[path = "publication_profile_reader_tests.rs"]
mod profile_reader_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_display_columns_without_second_field_align_enter_the_shared_typed_bibliography() {
        let dir=tempfile::tempdir().unwrap(); let base=dir.path().to_str().unwrap();
        let references=r#"[{"id":"short","type":"book","title":"[9]","publisher":"First body"},{"id":"wide","type":"book","title":"[100]","publisher":"Second body"}]"#;
        fs::write(dir.path().join("refs.json"),references).unwrap();
        let source="---\nbibliography: refs.json\ncsl: columns.csl\n---\n😀 Proof [@short; @wide].\n\n<!--readmd:bibliography-->\n";
        for line in [1,3] { for entry in [0,2] {
            let style=format!(r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Explicit display column probe</title><id>urn:readmd:test:display-columns</id></info><citation><layout delimiter="; "><text variable="title"/></layout></citation><bibliography line-spacing="{line}" entry-spacing="{entry}"><layout><group display="left-margin"><text variable="title" font-weight="bold"/></group><group display="right-inline"><text variable="publisher" font-style="italic"/></group></layout></bibliography></style>"#);
            fs::write(dir.path().join("columns.csl"),&style).unwrap();
            let prepared=prepare(source,base).unwrap(); let bib=prepared.projection.bibliography.as_ref().unwrap();
            assert_eq!(bib.items.len(),2); assert_eq!((bib.line_spacing,bib.entry_spacing),(line,entry));
            let labels=bib.items.iter().map(|item|crate::publication_rich::plain(&item.first_field)).collect::<Vec<_>>();
            assert_eq!(labels,vec!["[9]","[100]"]);
            assert_eq!(bib.items.iter().map(|item|crate::publication_rich::plain(&item.rich)).collect::<Vec<_>>(),vec!["First body","Second body"]);
            for item in &bib.items {
                assert!(crate::publication_rich::html(&item.first_field).unwrap().contains("font-weight:bold"));
                assert!(crate::publication_rich::html(&item.rich).unwrap().contains("font-style:italic"));
            }
            let span=bib.source_span.as_ref().unwrap();
            assert_eq!(span.source.trim(),"<!--readmd:bibliography-->");
            assert_eq!(&source[span.byte_from..span.byte_to],span.source);
            assert_eq!(prepared.projection.revision,crate::document_transaction::revision(source));
            let view=prepared.projection.reader_content(source).unwrap(); assert!(view.contains("readmd-bibliography-shared-columns"));
            assert!(view.contains(&format!("margin-bottom:{}em;",u64::from(line)*u64::from(entry))));
            assert_eq!(fs::read_to_string(dir.path().join("columns.csl")).unwrap(),style);
        }}
        assert_eq!(fs::read_to_string(dir.path().join("refs.json")).unwrap(),references);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),2);
    }
    #[test]
    fn explicit_citation_locale_options_share_prepare_and_resource_identity() {
        let dir=tempfile::tempdir().unwrap();let base=dir.path().to_str().unwrap();
        let references=r#"[{"id":"translator","type":"book","title":"Locale options proof","translator":[{"family":"Reader"}]}]"#;
        let style=r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Independent locale options probe</title><id>urn:readmd:test:explicit-locale</id></info><citation><layout><names variable="translator"><name form="short"/><label form="short" prefix=" "/></names><text variable="locator" prefix=" "/></layout></citation></style>"#;
        fs::write(dir.path().join("refs.json"),references).unwrap();fs::write(dir.path().join("locale.csl"),style).unwrap();
        let source="---\nbibliography: refs.json\ncsl: locale.csl\nlanguage: en-US\n---\n\nProof [@translator, p. 8].\n";
        let xml=r#"<locale xmlns="http://purl.org/net/xbiblio/csl" version="1.0" xml:lang="en-US"><terms><term name="translator" form="short">qa-role.</term></terms></locale>"#;
        let options=json!({"citations":{"locale_xml":xml}});let result=prepare_with_options(source,base,&options).unwrap();
        assert_eq!(result.data["citations"][0]["text"],"Reader qa-role. 8");assert_eq!(result.projection.revision,crate::document_transaction::revision(source));
        let before=resource_revision_with_options(source,base,&json!({})).unwrap();let after=resource_revision_with_options(source,base,&options).unwrap();assert_ne!(before,after);
        assert_ne!(after,resource_revision_with_options(source,base,&json!({"citations":{"locale_xml":xml.replace("qa-role.","qa-revised.")}})).unwrap());
        assert_eq!(fs::read_to_string(dir.path().join("refs.json")).unwrap(),references);assert_eq!(fs::read_to_string(dir.path().join("locale.csl")).unwrap(),style);
        let error=prepare_with_options(source,base,&json!({"citations":{"locale_xml":17}})).err().unwrap();assert_eq!(error.code,"academic_context");
        assert!(matches!(error.shape,crate::error::ErrorShape::ApiCode{extras} if extras.iter().any(|(key,value)|key=="field"&&value=="options.citations.locale_xml")));
    }
    #[test]
    fn citation_language_uses_shared_metadata_without_rewriting_source() {
        let dir=tempfile::tempdir().unwrap();
        fs::write(dir.path().join("refs.json"),r#"[{"id":"study","type":"book","title":"Evidence","issued":{"date-parts":[[2026,1,13]]}}]"#).unwrap();
        fs::write(dir.path().join("locale.csl"),r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Locale probe</title><id>urn:readmd:test:locale</id><updated>2026-10-13T00:00:00Z</updated></info><citation><layout><date variable="issued"><date-part name="month" form="long"/></date></layout></citation></style>"#).unwrap();
        let source="---\nbibliography: refs.json\ncsl: locale.csl\nlanguage: de_DE\n---\n# Research\n\nSource [@study].\n";
        fs::write(dir.path().join("original.md"),source).unwrap();
        for (options,locale,month) in [(json!({}),"de-DE","Januar"),(json!({"meta":{"language":"fr_CA"}}),"fr-CA","janvier"),(json!({"language":"ar","meta":{"language":"invalid--tag"}}),"de-DE","Januar")] {
            let result=prepare_with_options(source,dir.path().to_str().unwrap(),&options).unwrap();
            assert_eq!(result.data["locale"],locale);assert!(result.content.contains(month),"{}",result.content);
            assert!(result.content.contains("language: de_DE"));
        }
        assert_eq!(prepare(source,dir.path().to_str().unwrap()).unwrap().data["locale"],"de-DE");
        assert_eq!(fs::read_to_string(dir.path().join("original.md")).unwrap(),source);assert_eq!(fs::read_dir(dir.path()).unwrap().count(),3);
        let no_language=source.replace("language: de_DE","nested:\n  language: de-DE");
        assert_eq!(prepare(&no_language,dir.path().to_str().unwrap()).unwrap().data["locale"],"en-US");
    }
    #[test]
    fn note_styles_create_real_footnotes_and_keep_manually_authored_notes() {
        let dir=tempfile::tempdir().unwrap();
        fs::write(dir.path().join("refs.bib"),"@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781}}").unwrap();
        let source="---\nbibliography: refs.bib\ncsl: chicago-notes-bibliography\n---\nA manual note[^author]. Then [@kant, p. 42]. Again [@kant, p. 43].\n\n[^author]: Author's unchanged note.\n";
        let prepared=prepare(source,dir.path().to_str().unwrap()).unwrap();
        assert_eq!(prepared.data["style_class"],"note");
        assert_eq!(prepared.data["citations"][0]["note_number"],2);
        assert_eq!(prepared.data["citations"][1]["note_number"],3);
        assert!(prepared.content.contains("Then [^readmd-citation-1]. Again [^readmd-citation-2]."));
        assert!(prepared.content.contains("[^author]: Author's unchanged note."));
        let doc=crate::md_ast::parse(&prepared.content);
        let package=crate::docx_writer::build(&doc,&json!({}),"notes",&|_|None);
        let notes=&package.parts.iter().find(|(n,_)|n=="word/footnotes.xml").unwrap().1;
        let notes=String::from_utf8_lossy(notes);
        assert!(notes.contains("Author's unchanged note.")&&notes.contains("Kant")&&notes.contains("42"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),1);
    }
    #[test]
    fn publication_resolves_clusters_and_locators_without_touching_examples_or_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let bib="@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781}}\n@book{hume,title={An Enquiry},author={Hume, David},year={1748}}";
        fs::write(dir.path().join("refs.bib"), bib).unwrap();
        let source="---\nbibliography: refs.bib\ncsl: apa\nlang: zh-CN\n---\n# 研究\n\n比较 [@kant, p. 42; @hume]。缺项 [@missing]。\n\n`[@kant]`\n\n```md\n[@hume]\n<!--readmd:bibliography-->\n```\n\n$link=[@kant]$\n\n<!--readmd:bibliography-->\n";
        let result = prepare(source, dir.path().to_str().unwrap()).unwrap();
        assert!(
            result.content.contains("Kant")
                && result.content.contains("42")
                && result.content.contains("Hume")
        );
        assert!(result.content.contains("缺项 [@missing]"));
        assert!(
            result.content.contains("`[@kant]`")
                && result
                    .content
                    .contains("```md\n[@hume]\n<!--readmd:bibliography-->\n```")
                && result.content.contains("$link=[@kant]$")
        );
        assert_eq!(result.data["source_citations"].as_array().unwrap().len(), 2);
        assert_eq!(result.content.matches("## 参考文献").count(), 1);
        assert_eq!(
            fs::read_to_string(dir.path().join("refs.bib")).unwrap(),
            bib
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(result.warnings.len(), 1);
    }
    #[test]
    fn bibliography_is_optional_and_resources_are_local_bounded_and_explicit() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            prepare("# Note\n\n[@example]", "missing").unwrap().content,
            "# Note\n\n[@example]"
        );
        let outside = dir
            .path()
            .parent()
            .unwrap()
            .join(format!("outside-{}.bib", uuid::Uuid::new_v4()));
        fs::write(&outside, "@book{x,title={Private}}").unwrap();
        let source = format!(
            "---\nbibliography: {}\n---\n[@x]",
            outside.to_string_lossy()
        );
        assert_eq!(
            prepare(&source, dir.path().to_str().unwrap())
                .err()
                .unwrap()
                .code,
            "academic_resource"
        );
        fs::remove_file(outside).unwrap();
    }
    #[test]
    fn publication_context_projection_preserves_reading_order_source_identity_and_style_override() {
        let dir=tempfile::tempdir().unwrap();
        let references=r#"[{"id":"study","type":"book","title":"Compiler Study","author":[{"family":"Hopper","given":"Grace"}],"issued":{"date-parts":[[1952]]}}]"#;
        fs::write(dir.path().join("refs.json"),references).unwrap();
        let body=r#"<citation><layout delimiter="; "><group delimiter=", "><names variable="author"><name form="short"/><substitute><text variable="title"/></substitute></names><date variable="issued"><date-part name="year"/></date><text variable="title"/><group delimiter=" "><label variable="locator" form="short"/><text variable="locator"/></group></group></layout></citation><bibliography hanging-indent="true" line-spacing="2" entry-spacing="1"><layout><text variable="title" font-style="italic"/></layout></bibliography>"#;
        let style=|class:&str|format!(r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="{class}"><info><title>ReadMD publication probe</title><id>urn:readmd:publication-context-probe</id><updated>2026-10-13T00:00:00Z</updated></info>{body}</style>"#);
        fs::write(dir.path().join("note.csl"),style("note")).unwrap();fs::write(dir.path().join("text.csl"),style("in-text")).unwrap();
        let source="---\nbibliography: refs.json\ncsl: note.csl\n---\n😀 @study [p. 22]. Manual[^author]. Then [@study].\n\n[^author]: Read [-@study, chap. 2].\n\n<!--readmd:bibliography-->\n";
        let prepared=prepare(source,dir.path().to_str().unwrap()).unwrap();
        assert_eq!(prepared.projection.citations.len(),3);assert_eq!(prepared.projection.revision,crate::document_transaction::revision(source));
        let citations=&prepared.projection.citations;
        assert_eq!(citations[0].source.items[0].cite.mode,CiteMode::Prose);assert_eq!(citations[0].source.context.note_number,Some(1));
        assert!(citations[1].source.context.in_note);assert_eq!(citations[1].source.context.note_number,Some(2));assert!(citations[1].note_label.is_none());
        assert_eq!(citations[2].source.context.note_number,Some(3));assert!(citations[1].source.span.byte_from>citations[2].source.span.byte_from);
        assert_eq!(crate::publication_rich::plain(&citations[0].inline_rich),"Hopper");assert!(crate::publication_rich::plain(&citations[0].note_rich).contains("22"));
        assert_eq!(prepared.data["source_citations"][1]["context"]["note_label"],"author");
        for(index,citation)in citations.iter().enumerate(){assert_eq!(prepared.data["source_citations"][index]["from"],citation.source.span.from);assert_eq!(&source[citation.source.span.byte_from..citation.source.span.byte_to],citation.source.span.source);}
        let view=prepared.projection.reader_content(source).unwrap();assert!(view.contains("[^author]: Read <span class=\"readmd-citation\""));assert_eq!(view.matches("[^readmd-citation-").count(),4);assert!(!view.contains("data-slot="));assert!(view.contains("readmd-csl-italic"));
        let tokens=prepared.projection.instrument(source).unwrap();assert_eq!(tokens.matches("data-slot=").count(),4);
        let overridden=prepare_with_options(source,dir.path().to_str().unwrap(),&json!({"citations":{"style":"text.csl"}})).unwrap();
        assert_eq!(overridden.data["style_class"],"in-text");assert!(overridden.projection.citations.iter().all(|citation|citation.note_label.is_none()));assert!(overridden.content.contains("csl: note.csl"));
        assert_eq!(fs::read_to_string(dir.path().join("refs.json")).unwrap(),references);assert_eq!(fs::read_dir(dir.path()).unwrap().count(),3);
    }
}
