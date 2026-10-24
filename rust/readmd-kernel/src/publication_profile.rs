//! Source-owned publication parameters. Never persists client preset choices,
//! reads host files or injects completed defaults into the Markdown source.
use crate::{document_transaction as transaction, publication_metadata::{self as metadata, MetadataError}};
use serde_json::{json, Map, Value};

type Result<T> = std::result::Result<T, MetadataError>;
const PATH: &[&str] = &["readmd", "publication"];
fn error(code: &str, path: impl Into<String>) -> MetadataError {
    MetadataError { code: code.into(), path: path.into(), from: 0, to: 0 }
}

#[derive(Clone, Debug)]
pub struct SourceProfile {
    pub source_values: Value,
    pub options: Value,
    pub from: usize,
    pub to: usize,
    pub diagnostics: Vec<MetadataError>,
}

// The bounded YAML parser intentionally keeps scalar numbers lexical. Only
// typed numeric fields in this version are interpreted here; labels, unknown
// values, ISBNs and neighboring front matter remain exactly as authored.
fn numeric_member(path: &[String]) -> bool {
    if path == ["version"] { return true }
    if path == ["typography", "size"] { return true }
    let Some(tail) = path.strip_prefix(&["footnotes".to_string()]) else { return false };
    let tail = if tail.len() >= 3 && tail[0] == "kinds" && matches!(tail[1].as_str(), "footnote" | "endnote") { &tail[2..] } else { tail };
    match tail {
        [field] => matches!(field.as_str(), "size" | "sizeCs" | "start" | "lineHeight"),
        [group, field] => group == "lineSpacing" && matches!(field.as_str(), "pt" | "multiple"),
        _ => false,
    }
}
fn typed_source(value: &Value, path: &mut Vec<String>) -> Value {
    match value {
        Value::Object(values) => Value::Object(values.iter().map(|(k, v)| {
            path.push(k.clone()); let v = typed_source(v, path); path.pop(); (k.clone(), v)
        }).collect()),
        Value::String(s) if numeric_member(path) => serde_json::from_str::<Value>(s).ok()
            .filter(Value::is_number).unwrap_or_else(|| value.clone()),
        other => other.clone(),
    }
}

// The source-owned global size must survive the established style sanitizer
// exactly. This does not narrow the independent author-run half-point model or
// change the compatibility contract of legacy export-options requests.
fn validate_source_size(value: &Value, path: &str) -> Result<()> {
    let points = value.as_f64().ok_or_else(|| error("publication_metadata_type", path))?;
    if !(8.0..=20.0).contains(&points)
        || crate::publication_inline_text::HalfPoints::from_pt(points).is_err()
    {
        return Err(error("publication_metadata_field", path));
    }
    Ok(())
}

pub fn from_source(content: &str) -> Result<SourceProfile> {
    let Some((raw, from, to)) = metadata::bounded_source_member(content, PATH)? else {
        return Ok(SourceProfile { source_values: json!({}), options: json!({}), from: 0, to: 0, diagnostics: Vec::new() });
    };
    if raw.is_null() { return Ok(SourceProfile { source_values: json!({}), options: json!({}), from, to, diagnostics: Vec::new() }); }
    let value = typed_source(&raw, &mut Vec::new());
    let object = value.as_object().ok_or_else(|| MetadataError { from, to, ..error("publication_metadata_type", "source.readmd.publication") })?;
    if object.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(MetadataError { from, to, ..error("publication_metadata_field", "source.readmd.publication.version") });
    }
    let mut diagnostics = Vec::new();
    for key in object.keys().filter(|key| !matches!(key.as_str(), "version" | "footnotes" | "typography")) {
        diagnostics.push(MetadataError { from, to, ..error("publication_metadata_field", format!("source.readmd.publication.{key}")) });
    }
    let mut options = json!({});
    if let Some(notes) = object.get("footnotes").filter(|v| !v.is_null()) { options["footnotes"] = notes.clone(); }
    if let Some(typography) = object.get("typography").filter(|v| !v.is_null()) {
        let typography = typography.as_object().ok_or_else(|| MetadataError { from, to,
            ..error("publication_metadata_type", "source.readmd.publication.typography") })?;
        for key in typography.keys().filter(|key| key.as_str() != "size") {
            diagnostics.push(MetadataError { from, to,
                ..error("publication_metadata_field", format!("source.readmd.publication.typography.{key}")) });
        }
        if let Some(size) = typography.get("size").filter(|v| !v.is_null()) {
            validate_source_size(size, "source.readmd.publication.typography.size")
                .map_err(|e| MetadataError { from, to, ..e })?;
            options["typography"] = json!({"size":size});
        }
    }
    validate_options(&options).map_err(|mut e| { e.from = from; e.to = to; e.path = format!("source.readmd.publication.{}", e.path.trim_start_matches("options.")); e })?;
    Ok(SourceProfile { source_values: value, options, from, to, diagnostics })
}

fn profile_diagnostics(content: &str, source: &SourceProfile) -> Vec<MetadataError> {
    let mut diagnostics = source.diagnostics.clone();
    if let Err(error) = crate::publication_note_policies::parse_layer(&source.options)
        .and_then(|policies| crate::publication_note_policies::validate_bindings(&crate::md_ast::parse(content), &policies))
    {
        diagnostics.push(MetadataError { from: source.from, to: source.to,
            ..self::error(error.code, format!("source.readmd.publication.{}", error.path)) });
    }
    diagnostics
}

/// Reader preparation reports authored settings without passing unknown or
/// invalid source values to a writer. Export resolution remains strict.
pub(crate) fn source_diagnostics(content: &str) -> Vec<MetadataError> {
    match from_source(content) {
        Ok(source) => profile_diagnostics(content, &source),
        Err(error) => vec![error],
    }
}

pub fn validate_options(options: &Value) -> Result<()> {
    if options.is_null(){return Ok(());}
    crate::publication_note_policies::parse_layer(options).map(|_| ()).map_err(|e| {
        error(e.code, e.path)
    })
}

/// Resolve original sparse layers before any consumer completes its defaults.
/// Omitted preset/fallback is below source; an explicitly selected preset and
/// this operation's explicit options are above it. No client authority flags.
pub(crate) fn resolve_with_preset(content: &str, fallback: &Value, explicit_preset: Option<&Value>, options: &Value)
    -> Result<crate::publication_typography::ResolvedOptions>
{
    let source = from_source(content)?;
    if let Some(first) = source.diagnostics.first() { return Err(first.clone()); }
    validate_options(fallback)?;
    if let Some(preset)=explicit_preset { validate_options(preset)?; }
    // A direct legacy writer request with no inherited layer keeps its
    // established sanitizer contract (for example a scalar display title).
    // Do not force it through the stricter metadata-overlay API just to merge
    // two empty objects. Source diagnostics and typed note validation still run.
    if (fallback.is_null() || fallback.as_object().is_some_and(Map::is_empty))
        && source.options.as_object().is_some_and(Map::is_empty) && explicit_preset.is_none()
    {
        return crate::publication_typography::ResolvedOptions::from_request(options)
            .map_err(|code| error(code, crate::publication_typography::request_error_field(options)));
    }
    let mut resolved = crate::publication_typography::ResolvedOptions::from_request(fallback)
        .map_err(|code| error(code, crate::publication_typography::request_error_field(fallback)))?;
    resolved = resolved.merge(&source.options)?;
    if let Some(preset) = explicit_preset { resolved = resolved.merge(preset)?; }
    resolved.merge(options)
}
pub(crate) fn resolve(content: &str, options: &Value) -> Result<crate::publication_typography::ResolvedOptions> {
    resolve_with_preset(content, &json!({}), None, options)
}
pub(crate) fn for_format(format:&str, content:&str, resolved:crate::publication_typography::ResolvedOptions)
    -> Result<crate::publication_typography::ResolvedOptions>
{
    if format.eq_ignore_ascii_case("docx") { return Ok(resolved); }
    if resolved.values().get("footnotes").is_none_or(Value::is_null) && !content.contains("readmd-section") { return Ok(resolved); }
    let doc=crate::md_ast::parse(content);
    if format.eq_ignore_ascii_case("html") {
        crate::publication_note_policies::validate_html(&doc,resolved.values()).map_err(|e|error(e.code,e.path))?;
    }
    if matches!(format.to_ascii_lowercase().as_str(),"tex"|"latex")
        || format.eq_ignore_ascii_case("pdf")&&matches!(resolved.values().pointer("/pdf/engine").and_then(Value::as_str),Some("xelatex"|"pdflatex")) {
        crate::publication_note_policies::validate_default_section_numbering(&doc,resolved.values()).map_err(|e|error(e.code,e.path))?;
    }
    let projected = crate::publication_note_policies::project_legacy(&doc,resolved.values())
        .map_err(|e|error(e.code,e.path))?;
    resolved.with_completed_values(projected).map_err(|code|error(code,"options"))
}
pub(crate) fn resolve_for_format(format:&str, content:&str, options:&Value)->Result<crate::publication_typography::ResolvedOptions>{
    for_format(format,content,resolve(content,options)?)
}
pub(crate) fn preflight(format: &str, content: &str, options: &Value) -> Result<()> {
    if options.get("footnotes").is_some_and(|v| !v.is_null()) || content.contains("readmd-section") {
        let consumer = if format.eq_ignore_ascii_case("docx") { crate::publication_note_policies::Consumer::Docx }
            else if format.eq_ignore_ascii_case("html") { crate::publication_note_policies::Consumer::Html }
            else { crate::publication_note_policies::Consumer::Other };
        if matches!(format.to_ascii_lowercase().as_str(),"tex"|"latex")
            || format.eq_ignore_ascii_case("pdf")&&matches!(options.pointer("/pdf/engine").and_then(Value::as_str),Some("xelatex"|"pdflatex")) {
            crate::publication_note_policies::validate_default_section_numbering(&crate::md_ast::parse(content),options).map_err(|e|error(e.code,e.path))?;
        }
        crate::publication_note_policies::validate_consumer(consumer, &crate::md_ast::parse(content), options)
            .map_err(|e| error(e.code, e.path))?;
    }
    Ok(())
}

pub fn plan_patch(content: &str, base_revision: &str, patch: &Value) -> Result<transaction::ContentTransaction> {
    let object = patch.as_object().ok_or_else(|| error("publication_metadata_type", "patch"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "version" | "footnotes" | "typography") { return Err(error("publication_metadata_field", format!("patch.{key}"))); }
    }
    if object.get("version").is_some_and(|v| v.as_u64() != Some(1)) {
        return Err(error("publication_metadata_field", "patch.version"));
    }
    if let Some(typography) = object.get("typography").filter(|v| !v.is_null()) {
        let typography = typography.as_object().ok_or_else(|| error("publication_metadata_type", "patch.typography"))?;
        for key in typography.keys().filter(|key| key.as_str() != "size") {
            return Err(error("publication_metadata_field", format!("patch.typography.{key}")));
        }
        if let Some(size) = typography.get("size").filter(|v| !v.is_null()) {
            validate_source_size(size, "patch.typography.size")?;
        }
    }
    let mut patch = patch.clone();
    fn has_value(v:&Value)->bool {match v {Value::Null=>false,Value::Object(m)=>m.values().any(has_value),_=>true}}
    if has_value(&patch) { patch["version"] = json!(1); }
    let tx = metadata::patch_source_member(content, base_revision, PATH, &patch)?;
    let applied = transaction::apply(content, &tx).map_err(|code| error(code, "patch"))?;
    let profile=from_source(&applied.content)?;
    let policies=crate::publication_note_policies::parse_layer(&profile.options).map_err(|e|error(e.code,e.path))?;
    if !policies.kind_by_label.is_empty(){
        crate::publication_note_policies::validate_bindings(&crate::md_ast::parse(&applied.content),&policies)
            .map_err(|e|error(e.code,e.path))?;
    }
    Ok(tx)
}

/// DOCX import uses the very same source transaction as an authored change.
/// Conversion still returns an in-memory result; only Save materializes it.
pub(crate) fn embed_imported(content: &str, options: &Value) -> Result<String> {
    let mut patch = json!({});
    if let Some(notes) = options.get("footnotes").filter(|v| !v.is_null() && v.as_object().is_none_or(|m| !m.is_empty())) {
        patch["footnotes"] = notes.clone();
    }
    // The native adapter supplies only a common size that actually influenced
    // its import. Keep 11pt too: the MCP's omitted preset defaults to 10.5pt.
    if let Some(typography) = options.get("typography").filter(|v| !v.is_null()) {
        let typography = typography.as_object().ok_or_else(|| error("publication_metadata_type", "options.typography"))?;
        if let Some(size) = typography.get("size").filter(|v| !v.is_null()) {
            patch["typography"] = json!({"size":size});
        }
    }
    if patch.as_object().unwrap().is_empty() { return Ok(content.into()); }
    let tx = plan_patch(content, &transaction::revision(content), &patch)?;
    transaction::apply(content, &tx).map(|r| r.content).map_err(|code| error(code, "patch"))
}

pub fn handle(request: &Value) -> Result<Value> {
    let content = request.get("content").and_then(Value::as_str).ok_or_else(|| error("publication_metadata_type", "content"))?;
    let options = request.get("options").unwrap_or(&Value::Null);
    match request.get("action").and_then(Value::as_str).unwrap_or("get") {
        "patch" => {
            let revision = request.get("base_revision").and_then(Value::as_str).ok_or_else(|| error("transaction_conflict", "base_revision"))?;
            let tx = plan_patch(content, revision, request.get("patch").unwrap_or(&Value::Null))?;
            let applied = transaction::apply(content, &tx).map_err(|code| error(code, "patch"))?;
            let mut result = handle(&json!({"content":applied.content,"options":options}))?;
            result["content"] = json!(applied.content); result["transaction"] = json!(tx);
            result["inverse"] = json!(applied.inverse); result["changed"] = json!(applied.changed);
            Ok(result)
        },
        "get" => {
            let source = from_source(content)?;
            let diagnostics = profile_diagnostics(content, &source);
            let mut ranges = Map::new();
            ranges.insert("readmd.publication".into(), json!({"from":source.from,"to":source.to}));
            let resolved = crate::publication_typography::ResolvedOptions::from_request(&source.options)
                .map_err(|code| error(code, "source.readmd.publication"))?.merge(options)?;
            Ok(json!({"ok":true,"revision":transaction::revision(content),"sourceValues":source.source_values,
                "ranges":ranges,"options":resolved.values(),"diagnostics":diagnostics}))
        },
        _ => Err(error("publication_metadata_action", "action")),
    }
}
pub fn request_schema() -> Value {
    json!({"type":"object","required":["content"],"additionalProperties":false,
        "properties":{"action":{"enum":["get","patch"]},"content":{"type":"string","maxLength":transaction::MAX_DOCUMENT_BYTES},
        "base_revision":{"type":"string","minLength":64,"maxLength":64},"patch":{"type":"object"},"options":{"type":"object"}}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ordinary_documents_do_not_gain_profiles_or_defaults() {
        assert_eq!(from_source("# ordinary").unwrap().options, json!({}));
        assert_eq!(embed_imported("# ordinary", &json!({})).unwrap(), "# ordinary");
        for request in [json!({"content":"Text"}),json!({"content":"Text","options":null})] {
            let result=handle(&request).unwrap();assert_eq!(result["sourceValues"],json!({}));assert_eq!(result["options"],json!({}));
        }
    }
    #[test] fn numeric_fields_are_typed_without_touching_labels_or_neighboring_ids() {
        let s = "---\nisbn: '00123'\nreadmd:\n  publication:\n    version: 1\n    footnotes:\n      kinds:\n        endnote: {start: 4}\n      kindByLabel: {'00123': endnote}\n---\nText[^00123]\n\n[^00123]: A note\n";
        let p = from_source(s).unwrap();
        assert_eq!(p.options.pointer("/footnotes/kinds/endnote/start"), Some(&json!(4)));
        assert_eq!(p.options.pointer("/footnotes/kindByLabel/00123"), Some(&json!("endnote")));
        assert!(s.contains("isbn: '00123'"));
    }
    #[test] fn source_inherits_and_explicit_layers_win_without_becoming_source() {
        let s = "---\nreadmd: {publication: {version: 1, footnotes: {size: 10, start: 4}}}\n---\nT[^a]\n\n[^a]: A\n";
        let source = resolve_with_preset(s, &json!({"footnotes":{"size":8}}), None, &json!({})).unwrap();
        assert_eq!(source.values()["footnotes"]["size"], 10);
        let chosen = resolve_with_preset(s, &json!({}), Some(&json!({"footnotes":{"size":9}})), &json!({"footnotes":{"start":null}})).unwrap();
        assert_eq!(chosen.values()["footnotes"]["size"], 9);
        assert_eq!(chosen.values()["footnotes"]["start"], 4);
        assert_eq!(from_source(s).unwrap().options["footnotes"]["size"], 10);
    }
    #[test] fn patch_returns_one_reversible_revision_checked_change() {
        let s = "\u{feff}---\r\nisbn: '00123' # keep\r\n---\r\n# 𝛂\r\n";
        let result = handle(&json!({"action":"patch","content":s,"base_revision":transaction::revision(s),"patch":{"footnotes":{"start":4}}})).unwrap();
        let out = result["content"].as_str().unwrap();
        assert!(out.contains("isbn: '00123' # keep\r\n"));
        let back = transaction::ContentTransaction {base_revision:transaction::revision(out), origin:"editor".into(), edits:serde_json::from_value(result["inverse"].clone()).unwrap()};
        assert_eq!(transaction::apply(out, &back).unwrap().content, s);
        assert_eq!(handle(&json!({"action":"patch","content":out,"base_revision":transaction::revision(s),"patch":{}})).unwrap_err().code,"transaction_conflict");
    }
    #[test] fn unknown_source_parameters_are_retained_and_diagnosed_before_export() {
        let s = "---\nreadmd: {publication: {version: 1, future: '007'}}\n---\nText\n";
        let profile = from_source(s).unwrap(); assert_eq!(profile.source_values["future"], "007");
        assert_eq!(profile.diagnostics[0].path, "source.readmd.publication.future");
        assert!(resolve(s, &json!({})).is_err());
    }
    #[test] fn dangling_note_identity_is_diagnosed_and_cannot_be_committed_as_a_source_patch() {
        let source="Text[^present]\n\n[^present]: A note\n";
        assert_eq!(plan_patch(source,&transaction::revision(source),&json!({"footnotes":{"kindByLabel":{"absent":"endnote"}}})).unwrap_err().code,"publication_note_binding_mismatch");
        let invalid="---\nreadmd: {publication: {version: 1, footnotes: {kindByLabel: {absent: endnote}}}}\n---\nText\n";
        let response=handle(&json!({"content":invalid})).unwrap();
        assert_eq!(response["diagnostics"][0]["code"],"publication_note_binding_mismatch");
    }
    #[test] fn source_global_size_is_exact_bounded_and_separate_from_author_run_sizes() {
        for size in [8.,8.5,10.5,11.,14.,20.] {
            let source=format!("---\ncatalog_id: '00123'\nreadmd: {{publication: {{version: 1, typography: {{size: {size}}}}}}}\n---\nText\n");
            let profile=from_source(&source).unwrap();
            assert_eq!(profile.options.pointer("/typography/size").and_then(Value::as_f64),Some(size));
            assert!(profile.diagnostics.is_empty());assert!(source.contains("catalog_id: '00123'"));
        }
        for size in [json!(7.5),json!(20.5),json!(8.25),json!(true),json!("base"),json!({}),json!([])] {
            let profile=json!({"version":1,"typography":{"size":size}});
            let source=format!("---\nreadmd: {{publication: {profile}}}\n---\nText\n");
            let before=source.clone();let error=from_source(&source).unwrap_err();
            assert!(matches!(error.code.as_str(),"publication_metadata_field"|"publication_metadata_type"));
            assert_eq!(error.path,"source.readmd.publication.typography.size");assert_eq!(source,before);
            assert!(plan_patch("Text",&transaction::revision("Text"),&json!({"typography":{"size":size}})).is_err());
            assert!(embed_imported("Text",&json!({"typography":{"size":size}})).is_err());
        }
        let ordinary="A <span data-readmd-text=\"v1\" data-size-pt=\"8192\">Native run</span>";
        assert!(crate::publication_inline_text::get(ordinary).is_ok());
        assert_eq!(from_source(ordinary).unwrap().options,json!({}));
        let legacy=resolve("Text",&json!({"typography":{"size":30}})).unwrap();
        assert_eq!(legacy.values()["typography"]["size"],30);
        assert_eq!(crate::export_styles::sanitize(legacy.values())["typography"]["size"],20.,"the existing unrelated export-options sanitizer contract is unchanged");
    }
    #[test] fn global_size_patch_is_reversible_and_preserves_source_boundaries() {
        let source="\u{feff}---\r\ncatalog_id: '00123' # keep\r\nreadmd:\r\n  ownedNeighbor: '007' # keep nested\r\n---\r\n# 𝛂\r\n\r\nBody[^n].\r\n\r\n[^n]: Note\r\n";
        let result=handle(&json!({"action":"patch","content":source,"base_revision":transaction::revision(source),"patch":{"typography":{"size":11},"footnotes":{"start":4}}})).unwrap();
        let edited=result["content"].as_str().unwrap();assert!(result["changed"].as_bool().unwrap());
        assert!(edited.starts_with('\u{feff}'));assert!(!edited.replace("\r\n","").contains('\n'));
        for unchanged in ["catalog_id: '00123' # keep\r\n","  ownedNeighbor: '007' # keep nested\r\n","# 𝛂\r\n\r\nBody[^n].\r\n\r\n[^n]: Note\r\n"]{assert!(edited.contains(unchanged),"{edited}");}
        assert_eq!(from_source(edited).unwrap().options,json!({"typography":{"size":11},"footnotes":{"start":4}}));
        let inverse=transaction::ContentTransaction{base_revision:transaction::revision(edited),origin:"editor".into(),edits:serde_json::from_value(result["inverse"].clone()).unwrap()};
        assert_eq!(transaction::apply(edited,&inverse).unwrap().content,source);
        assert_eq!(plan_patch(edited,&transaction::revision(source),&json!({"typography":{"size":12}})).unwrap_err().code,"transaction_conflict");
    }
    #[test] fn source_global_size_inherits_over_minimal_and_explicit_layers_still_win() {
        let source="---\nreadmd: {publication: {version: 1, typography: {size: 11}}}\n---\nText\n";
        let minimal=crate::export_styles::preset("minimal").unwrap();assert_eq!(minimal["typography"]["size"],10.5);
        for options in [json!({}),Value::Null] {
            let inherited=resolve_with_preset(source,&minimal,None,&options).unwrap();
            assert_eq!(inherited.values()["typography"]["size"],11,"MCP's omitted preset must not replace imported 11pt with minimal 10.5pt");
        }
        let selected=resolve_with_preset(source,&minimal,Some(&json!({"typography":{"size":12}})),&json!({})).unwrap();
        assert_eq!(selected.values()["typography"]["size"],12);
        let explicit=resolve_with_preset(source,&minimal,Some(&json!({"typography":{"size":12}})),&json!({"typography":{"size":14}})).unwrap();
        assert_eq!(explicit.values()["typography"]["size"],14);assert_eq!(from_source(source).unwrap().options["typography"]["size"],11);
    }
    #[test] fn imported_global_size_is_embedded_without_notes_including_eleven_points() {
        let source="# Imported\n\nNative text.\n";
        for size in [10.5,11.,14.] {
            let imported=embed_imported(source,&json!({"typography":{"size":size,"font":"Never persist"},"page":{"size":"A5"}})).unwrap();
            let profile=from_source(&imported).unwrap();assert_eq!(profile.options,json!({"typography":{"size":size}}));
            assert_eq!(profile.source_values,json!({"version":1,"typography":{"size":size}}));
            assert!(!imported.contains("Never persist"));assert!(!imported.contains("A5"));assert!(imported.ends_with(source));
        }
        let notes="Body[^n].\n\n[^n]: Note\n";
        let imported=embed_imported(notes,&json!({"typography":{"size":11},"footnotes":{"start":4,"kindByLabel":{"n":"endnote"}}})).unwrap();
        assert_eq!(from_source(&imported).unwrap().options,json!({"typography":{"size":11},"footnotes":{"start":4,"kindByLabel":{"n":"endnote"}}}));
        assert_eq!(embed_imported(source,&json!({"typography":{},"footnotes":{}})).unwrap(),source);
        assert_eq!(embed_imported(source,&json!({"typography":{"size":null}})).unwrap(),source);
    }
    #[test] fn unknown_typography_members_are_visible_to_get_and_cannot_reach_export() {
        let source="---\nreadmd: {publication: {version: 1, typography: {size: 11, future: '007'}}}\n---\nText\n";
        let profile=from_source(source).unwrap();assert_eq!(profile.source_values["typography"]["future"],"007");
        assert_eq!(profile.options,json!({"typography":{"size":11}}));assert_eq!(profile.diagnostics.len(),1);
        assert_eq!(profile.diagnostics[0].path,"source.readmd.publication.typography.future");assert!(profile.diagnostics[0].to>profile.diagnostics[0].from);
        let readable=handle(&json!({"content":source})).unwrap();assert_eq!(readable["sourceValues"]["typography"]["future"],"007");
        assert_eq!(readable["diagnostics"][0]["path"],"source.readmd.publication.typography.future");assert!(resolve(source,&json!({})).is_err());
        let error=plan_patch("Text",&transaction::revision("Text"),&json!({"typography":{"font":"No new profile field"}})).unwrap_err();
        assert_eq!(error.code,"publication_metadata_field");assert_eq!(error.path,"patch.typography.font");
        for typography in [json!([]),json!(true),json!(12)] {
            assert!(plan_patch("Text",&transaction::revision("Text"),&json!({"typography":typography})).is_err());
            assert!(embed_imported("Text",&json!({"typography":typography})).is_err());
        }
    }
    #[test] fn global_size_clear_and_empty_patch_keep_existing_transaction_semantics() {
        let source="---\nreadmd: {publication: {version: 1, typography: {size: 11}}}\n---\nText\n";
        let unchanged=handle(&json!({"action":"patch","content":source,"base_revision":transaction::revision(source),"patch":{"typography":{}}})).unwrap();
        assert_eq!(unchanged["content"],source);assert_eq!(unchanged["changed"],false);
        for patch in [json!({"typography":{"size":null}}),json!({"typography":null})] {
            let cleared=handle(&json!({"action":"patch","content":source,"base_revision":transaction::revision(source),"patch":patch})).unwrap();
            let content=cleared["content"].as_str().unwrap();assert_eq!(from_source(content).unwrap().options,json!({}));
            let inverse=transaction::ContentTransaction{base_revision:transaction::revision(content),origin:"editor".into(),edits:serde_json::from_value(cleared["inverse"].clone()).unwrap()};
            assert_eq!(transaction::apply(content,&inverse).unwrap().content,source);
        }
    }
}
