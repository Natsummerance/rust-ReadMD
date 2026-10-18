//! `readmd --mcp`: a Model Context Protocol server on stdio (JSON-RPC 2.0, one
//! message per line), replacing `packages/mcp-server/readmd_mcp_server.py`.
//!
//! Every tool runs the kernel's own code — the same converters, exporters,
//! OCR, Skill registry and AI client the desktop UI uses — either through a
//! library call or, for routes that carry gating/validation, through
//! [`server::call_in_process`].  stdout carries protocol messages only; logs
//! go to stderr.
//!
//! Side effects are explicit: exports, network fetches, code execution and
//! PDF rollback require a literal `confirm: true`, output paths must be
//! absolute, never symlinks, and an existing file is only replaced with
//! `overwrite: true`.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::{server, App};

pub const PROTOCOL_VERSIONS: &[&str] = &["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const MODERN_PROTOCOL: &str = "2026-07-28";
const LEGACY_PROTOCOL: &str = "2025-11-25";
const PROTOCOL_META: &str = "io.modelcontextprotocol/protocolVersion";
const MAX_CONCURRENT_TOOLS: usize = 8;
const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;

/// Tools that write files, touch the network or run code.
const CONFIRM_REQUIRED: &[&str] = &[
    "readmd_pdf_document",
    "readmd_web_to_markdown",
    "readmd_export_document",
    "readmd_export_presentation",
    "readmd_export_epub",
    "readmd_run_code_chunk",
    "readmd_pdf_rollback",
    "readmd_ai_models",
    "readmd_render_diagram",
];

/// One-release aliases for the former workflow ids (`prompts/get`, `readmd_ai_assistant`).
const LEGACY_WORKFLOW_ALIASES: &[(&str, &str)] = &[
    ("quick_read", "readmd-quick-read"),
    ("polish", "readmd-polish"),
    ("modify", "readmd-format-fix"),
    ("expand", "readmd-polish"),
    ("continue", "readmd-continue"),
    ("translate", "readmd-translate"),
    ("ask", "readmd-ask"),
    ("summary", "readmd-summary"),
    ("outline", "readmd-outline"),
    ("weekly", "readmd-weekly"),
    ("to_english", "readmd-translate"),
    ("code_review", "readmd-code-review"),
];

fn resolve_skill_id(raw: &str) -> String {
    let t = raw.trim();
    LEGACY_WORKFLOW_ALIASES.iter().find(|(k, _)| *k == t).map(|(_, v)| v.to_string()).unwrap_or_else(|| t.to_string())
}

// ---------------------------------------------------------------- tool list

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn math_input_schema() -> Value {
    let mut props=json!({"operation":{"type":"string","enum":["parse","patch","serialize","migrate","schema"]},
        "source":{"type":"string"},"content":{"type":"string"},
        "dialect":{"type":"string","enum":["dsl","latex","omml","mathml","unicodemath"]},
        "display":{"type":"boolean"},"expected_revision":{"type":"string"},"patch":{"type":"object"},
        "target":{"type":"string"},"latex":{"type":"string"},"id":{"type":"string"}});
    props["font_size"]=json!({"type":"number","minimum":4,"maximum":512,
        "description":"Formula font size in physical points; defaults to 12."});
    props["layout_width"]=json!({"type":"number","minimum":1,"maximum":20000,
        "description":"Available content width in physical points. Omit for intrinsic layout."});
    props["metadata"]=json!({"type":"string",
        "description":"Optional legacy math metadata for read-only identity and stale-state inspection."});
    schema(props,&["operation"])
}

fn confirm_prop() -> Value {
    json!({ "type": "boolean", "const": true, "description": "Explicit confirmation for this side effect" })
}

fn overwrite_prop() -> Value {
    json!({ "type": "boolean", "default": false, "description": "Replace the target if it already exists" })
}

fn pdf_document_schema()->Value{
    let mut props=json!({"action":{"type":"string","enum":["probe","open","render","edit","undo","redo","save","close","drafts","resume","history","restore_history"]},"path":{"type":"string"},"session":{"type":"string"},"revision":{"type":"integer"},"page":{"type":"integer"},"width":{"type":"integer"},"operation":{"type":"object"},"draft":{"type":"string"}});
    props.as_object_mut().unwrap().extend(json!({"checkpoint":{"type":"string"},"expected_source_revision":{"type":"string"},"password":{"type":"string"},"allow_signatures":{"type":"boolean"},"discard":{"type":"boolean"},"confirm":confirm_prop()}).as_object().unwrap().clone());
    schema(props,&["action","confirm"])
}
pub fn tools() -> Value {
    // Keep nested option schemas outside the large tool-list macro.
    let epub_navigation_schema=json!({"anyOf":[crate::publication_navigation::request_schema(),{"type":"null"}]});
    let epub_export_options=json!({"type":"object",
        "description":"The same publication metadata, book layout and citation overrides as the desktop export panel.",
        "properties":{"epub":{"type":"object","properties":{"navigation":epub_navigation_schema}}}});

    let mut list = json!([
        { "name":"readmd_publication_metadata", "description":"Read structured publication metadata or plan a revision-checked, source-preserving metadata edit and inverse. Authors, institutions, funding and book data share the desktop/export model. Never writes files.",
          "inputSchema":crate::publication_metadata::request_schema() },
        { "name":"readmd_publication_blocks", "description":"List exact UTF-16 publication block ranges or return a revision-checked local-formatting transaction and its inverse. Readable Markdown metadata supports Word, Typst PDF and EPUB. Never writes files; unsupported targets and properties are rejected.",
          "inputSchema":schema(json!({"action":{"type":"string","enum":["list","apply"]},"content":{"type":"string"},"base_revision":{"type":"string"},"id":{"type":"string"},"patch":{"type":"object"}}), &["content"]) },
        { "name":"readmd_openapi_document", "description":"Generate or update a provenance-tracked Markdown region from local OpenAPI 3.0/3.1 JSON or YAML. Returns a revision-checked transaction, operation changes, source maps and unresolved-reference diagnostics. Preserves surrounding prose and refuses to overwrite manual edits. Never writes files or fetches references.",
          "inputSchema":schema(json!({"source":{"type":"string"},"source_name":{"type":"string"},"source_id":{"type":"string"},"file_path":{"type":"string"},"content":{"type":"string"},"base_revision":{"type":"string"},"expected_source_revision":{"type":"string"},"locale":{"type":"string"}}), &["content","base_revision"]) },
        { "name":"readmd_pdf_document", "description":"Use the desktop PDF object editor and managed offline PDFium worker. Open/render/edit/undo/redo/save/recovery share revision-guarded drafts. Only save modifies the original PDF. Requires the matching installed capability pack and explicit confirmation; never returns passwords.",
          "inputSchema":pdf_document_schema() },
        { "name":"readmd_academic_document", "description":"Prepare Markdown citations and a bibliography with the shared offline CSL processor. bibliography/csl resources must stay under baseDir. Returns an ephemeral publication view and diagnostics; never writes source files.",
          "inputSchema":schema(json!({"content":{"type":"string"},"baseDir":{"type":"string"},"options":{"type":"object","description":"Publication metadata overrides; meta.language controls CSL localization independently of the UI language."}}), &["content","baseDir"]) },
        { "name":"readmd_content_transaction", "description":"Apply revision-checked, nonoverlapping UTF-16 edits in memory. Returns new content and an inverse transaction; does not write a file.",
          "inputSchema":schema(json!({"content":{"type":"string"},"transaction":{"type":"object"}}), &["content","transaction"]) },
        { "name":"readmd_math", "description":"Use the desktop's mathematical model to parse, patch, serialize or plan migration to readable DSL. Patches require expected_revision; migration plans never write files. Unknown macros and invalid properties are diagnosed.",
          "inputSchema":math_input_schema() },
        { "name": "readmd_fix_markdown",
          "description": "Diagnose and repair Markdown syntax problems (broken formulas, misaligned tables, unclosed code fences, CJK/Latin spacing and punctuation).",
          "inputSchema": schema(json!({ "content": { "type": "string", "description": "Markdown to repair" } }), &["content"]) },
        { "name": "readmd_convert_to_markdown",
          "description": "Convert a local document (docx, doc, pdf, pptx, ppt, xlsx, xls, csv, epub, mobi, azw3, html, tex, txt, rtf, odt, ipynb, zip …) to Markdown.",
          "inputSchema": schema(json!({ "file_path": { "type": "string", "description": "Absolute path of the source document" } }), &["file_path"]) },
        { "name": "readmd_web_to_markdown",
          "description": "Fetch an http(s) URL and extract its main content as clean Markdown.",
          "inputSchema": schema(json!({ "url": { "type": "string" }, "confirm": confirm_prop() }), &["url", "confirm"]) },
        { "name": "readmd_ocr_to_markdown",
          "description": "Read text from a local image or scanned PDF with ReadMD's managed offline OCR engine and model pack. Never searches for host OCR or requires Office.",
          "inputSchema": schema(json!({ "file_path": { "type": "string", "description": "Absolute path of the image or PDF" } }), &["file_path"]) },
        { "name": "readmd_export_document",
          "description": "Export Markdown to PDF (vector formulas, embedded fonts), Word .docx (native equations), HTML or LaTeX.",
          "inputSchema": schema(json!({
              "markdown_content": { "type": "string" },
              "output_path": { "type": "string", "description": "Absolute target path; the extension must match output_format" },
              "output_format": { "type": "string", "enum": ["pdf", "docx", "html", "tex"] },
              "style_preset": { "type": "string", "description": "Style preset (default minimal)" },
              "options": { "type":"object", "description":"Override desktop export parameters; pdf.engine supports rust, typst, xelatex or pdflatex. Heavy engines use application-locked capability packs, with no host PATH or Office requirement. Unavailable packs return an explicit error." },
              "title": { "type": "string" },
              "base_dir": { "type": "string", "description": "Directory relative images resolve against (default: the output directory)" },
              "overwrite": overwrite_prop(),
              "confirm": confirm_prop() }), &["markdown_content", "output_path", "output_format", "confirm"]) },
        { "name": "readmd_export_presets",
          "description": "List the desktop app's current built-in and user-created export presets with their style options.",
          "inputSchema": schema(json!({}), &[]) },
        { "name": "readmd_latex_to_md",
          "description": "Convert LaTeX source (papers or formulas) to Markdown.",
          "inputSchema": schema(json!({ "latex_content": { "type": "string" } }), &["latex_content"]) },
        { "name": "readmd_md_to_latex",
          "description": "Compile Markdown into standalone LaTeX source for pdflatex / xelatex.",
          "inputSchema": schema(json!({ "markdown_content": { "type": "string" }, "doc_title": { "type": "string" } }), &["markdown_content"]) },
        { "name": "readmd_parse_bibtex",
          "description": "Parse a BibTeX file (or the .bib files beside a document) into structured entries.",
          "inputSchema": schema(json!({ "bib_file_path": { "type": "string", "description": ".bib file, or a document / directory to search" } }), &["bib_file_path"]) },
        { "name": "readmd_latex_to_omml",
          "description": "Convert a LaTeX formula to Office Math (OMML) XML for Word.",
          "inputSchema": schema(json!({ "latex_formula": { "type": "string" }, "display": { "type": "boolean", "default": false } }), &["latex_formula"]) },
        { "name": "readmd_ai_assistant",
          "description": "Render a ReadMD Skill into a ready-to-send system prompt for the given document.",
          "inputSchema": schema(json!({
              "skill_id": { "type": "string" },
              "workflow_id": { "type": "string", "description": "Legacy alias of skill_id" },
              "markdown_content": { "type": "string" },
              "request": { "type": "string" },
              "language": { "type": "string" } }), &[]) },
        { "name": "readmd_ai_providers",
          "description": "List the AI providers configured in ReadMD and their connection state. Never returns API keys.",
          "inputSchema": schema(json!({}), &[]) },
        { "name": "readmd_ai_models",
          "description": "Discover models from a saved ReadMD connection. Uses the credential vault; never accepts or returns raw API keys.",
          "inputSchema": schema(json!({ "provider": { "type": "string" }, "credential_id": { "type": "string" }, "confirm": confirm_prop() }), &["provider", "confirm"]) },
        { "name": "readmd_ai_complete",
          "description": "Return a bounded Markdown insertion at the cursor using the saved AI connection. Ephemeral: no file write or conversation history. Caller must discard obsolete contexts.",
          "inputSchema": schema(json!({"prefix":{"type":"string"},"suffix":{"type":"string"},"provider":{"type":"string"},"model":{"type":"string"},"protocol":{"type":"string","enum":["chat","qwen","starcoder","deepseek"]}}), &["prefix"]) },
        { "name": "readmd_ai_chat",
          "description": "Run one AI document task through a ReadMD Skill using a saved credential (credential_id, never a raw key).",
          "inputSchema": schema(json!({
              "provider": { "type": "string" }, "credential_id": { "type": "string" }, "model": { "type": "string" },
              "skill_id": { "type": "string" }, "markdown_content": { "type": "string" },
              "request": { "type": "string" }, "language": { "type": "string" } }), &["provider", "model", "skill_id", "markdown_content"]) },
        { "name": "readmd_render_diagram",
          "description": "Render PlantUML using locally installed Java/PlantUML, or basic WSD/D2/Ditaa diagrams. Never uploads diagram source.",
          "inputSchema": schema(json!({ "engine": { "type": "string", "enum": ["plantuml", "puml", "wsd", "d2", "ditaa"] }, "code": { "type": "string" }, "confirm": confirm_prop() }), &["engine", "code", "confirm"]) },
        { "name": "readmd_process_imports",
          "description": "Flatten @import directives (nested Markdown, CSV as tables, code line ranges).",
          "inputSchema": schema(json!({ "markdown_content": { "type": "string" }, "base_dir": { "type": "string" } }), &["markdown_content"]) },
        { "name": "readmd_generate_toc",
          "description": "Build a linked table of contents from the document's headings.",
          "inputSchema": schema(json!({
              "markdown_content": { "type": "string" },
              "depth_from": { "type": "integer", "default": 1 }, "depth_to": { "type": "integer", "default": 6 },
              "ordered_list": { "type": "boolean", "default": false } }), &["markdown_content"]) },
        { "name": "readmd_export_presentation",
          "description": "Export Markdown slides (--- or <!-- slide --> separated) to a single offline Reveal.js HTML file.",
          "inputSchema": schema(json!({
              "markdown_content": { "type": "string" }, "output_path": { "type": "string" }, "title": { "type": "string" },
              "theme": { "type": "string" }, "transition": { "type": "string" }, "base_dir": { "type": "string" },
              "overwrite": overwrite_prop(), "confirm": confirm_prop() }), &["markdown_content", "output_path", "confirm"]) },
        { "name": "readmd_export_epub",
          "description": "Package Markdown as an EPUB 3 e-book (local images embedded).",
          "inputSchema": schema(json!({
              "markdown_content": { "type": "string" }, "output_path": { "type": "string" }, "title": { "type": "string" },
              "author": { "type": "string" }, "language": { "type": "string" }, "base_dir": { "type": "string" },
              "options": epub_export_options,
              "overwrite": overwrite_prop(), "confirm": confirm_prop() }), &["markdown_content", "output_path", "confirm"]) },
        { "name": "readmd_run_code_chunk",
          "description": "Run a code block (python, javascript, bash, powershell, r, rust …) with the locally installed runtime and capture its output.",
          "inputSchema": schema(json!({
              "code": { "type": "string" }, "language": { "type": "string", "default": "python" },
              "capture_plot": { "type": "boolean", "default": true }, "confirm": confirm_prop() }), &["code", "confirm"]) },
        { "name": "readmd_pdf_audit",
          "description": "Inspect a PDF: page count, page sizes, rotation, text layer, read-only flag and processes holding it open.",
          "inputSchema": schema(json!({ "pdf_path": { "type": "string" } }), &["pdf_path"]) },
        { "name": "readmd_analyze_document",
          "description": "Inspect current Markdown without changing it: AST headings, local/wiki links, anchors, task counts and UTF-16 diagnostic locations. Optional workspace checks never fetch URLs.",
          "inputSchema": schema(json!({"content":{"type":"string"},"file_path":{"type":"string"},"workspace_root":{"type":"string"}}), &["content"]) },
        { "name": "readmd_search_workspace",
          "description": "Read-only, bounded search of current local Markdown files. Combine quoted phrases, path:, title:, tag:, and -exclusions. Ignores hidden and generated directories; reports skipped/truncated results.",
          "inputSchema": schema(json!({"workspace_root":{"type":"string"},"query":{"type":"string"},"limit":{"type":"integer","default":20}}), &["workspace_root","query"]) },
        { "name": "readmd_suggest_links",
          "description": "Read-only local file/image, heading, footnote and bibliography-key suggestions. Uses the supplied unsaved draft for current-document headings. Bounded workspace scan; never fetches URLs.",
          "inputSchema": schema(json!({"workspace_root":{"type":"string"},"file_path":{"type":"string"},"content":{"type":"string"},"kind":{"type":"string","enum":["wiki","link","image","footnote","citation"]},"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":60,"default":40}}), &["workspace_root","kind","query"]) },
        { "name": "readmd_resolve_link",
          "description": "Resolve a local wiki note and optional heading to its real path and UTF-16 source location. Rejects ambiguous note names and paths outside the workspace. Never creates a missing note.",
          "inputSchema": schema(json!({"workspace_root":{"type":"string"},"file_path":{"type":"string"},"target":{"type":"string"}}), &["workspace_root","target"]) },
        { "name": "readmd_read_document",
          "description": "Read at most 1000 lines of a local text document with its byte SHA-256 revision, encoding and EOL. Limited to 2 MiB; use the revision for conflict-protected edits.",
          "inputSchema": schema(json!({"file_path":{"type":"string"},"start_line":{"type":"integer","default":1},"end_line":{"type":"integer","default":1000}}), &["file_path"]) },
        { "name": "readmd_edit_document",
          "description": "Preview a literal replacement by default. Commit only with dry_run:false, confirm:true and a matching byte revision. Ambiguous matches require replace_all. Preserves encoding/EOL; records recoverable history outside the document folder.",
          "inputSchema": schema(json!({"file_path":{"type":"string"},"expected_revision":{"type":"string"},
            "old_text":{"type":"string"},"new_text":{"type":"string"},"replace_all":{"type":"boolean","default":false},
            "dry_run":{"type":"boolean","default":true},"confirm":confirm_prop()}), &["file_path","expected_revision","old_text","new_text"]) },
        { "name": "readmd_document_history",
          "description": "List or read bounded recovery checkpoints belonging to one local document. Read-only; never restores or overwrites automatically.",
          "inputSchema": schema(json!({"file_path":{"type":"string"},"operation":{"type":"string","enum":["list","read"],"default":"list"},"checkpoint_id":{"type":"string"}}), &["file_path"]) },
        { "name": "readmd_pdf_rollback",
          "description": "Restore a PDF from its .bak backup written by a previous ReadMD edit.",
          "inputSchema": schema(json!({ "pdf_path": { "type": "string" }, "confirm": confirm_prop() }), &["pdf_path", "confirm"]) },
    ]);
    for tool in list.as_array_mut().unwrap() {
        let name = tool["name"].as_str().unwrap_or("");
        let read_only = matches!(name, "readmd_fix_markdown" | "readmd_generate_toc" | "readmd_latex_to_md" | "readmd_md_to_latex" | "readmd_latex_to_omml" | "readmd_parse_bibtex" | "readmd_process_imports" | "readmd_ai_assistant" | "readmd_ai_providers" | "readmd_export_presets" | "readmd_pdf_audit");
        let read_only = read_only || matches!(name, "readmd_analyze_document" | "readmd_search_workspace" | "readmd_read_document" | "readmd_document_history");
        let read_only = read_only || matches!(name, "readmd_ai_complete" | "readmd_suggest_links" | "readmd_resolve_link");
        let read_only = read_only || matches!(name, "readmd_math" | "readmd_content_transaction" | "readmd_academic_document" | "readmd_openapi_document" | "readmd_publication_blocks" | "readmd_publication_metadata");
        let open_world = matches!(name, "readmd_web_to_markdown" | "readmd_ai_chat" | "readmd_ai_complete" | "readmd_ai_models" | "readmd_run_code_chunk");
        tool["annotations"] = json!({ "readOnlyHint": read_only, "destructiveHint": !read_only, "idempotentHint": read_only && name != "readmd_ai_complete", "openWorldHint": open_world });
    }
    list
}

// ---------------------------------------------------------------- results

fn text_result(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }] })
}

fn json_result(v: &Value) -> Value {
    let mut result = text_result(serde_json::to_string_pretty(v).unwrap_or_default());
    // Keep text for older clients; structuredContent is an object in both eras.
    if v.is_object() { result["structuredContent"] = v.clone(); }
    result
}

fn error_result(code: &str, message: Option<&str>) -> Value {
    let mut body = json!({ "ok": false, "error_code": code });
    if let Some(m) = message {
        body["error"] = json!(m);
    }
    json!({ "isError": true, "structuredContent": body, "content": [{ "type": "text", "text": body.to_string() }] })
}

fn arg_str(args: &Value, key: &str, fallback: &str) -> String {
    match args.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => fallback.to_string(),
        Some(v) => v.to_string(),
    }
}

fn arg_bool(args: &Value, key: &str, fallback: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn arg_i64(args: &Value, key: &str, fallback: i64) -> i64 {
    args.get(key).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(fallback)
}

fn existing_file(raw: &str) -> Result<PathBuf, Value> {
    let p = Path::new(raw);
    if raw.is_empty() || !p.is_absolute() {
        return Err(error_result("path_must_be_absolute", None));
    }
    if !p.is_file() {
        return Err(error_result("file_not_found", Some(&format!("文件不存在: {raw}"))));
    }
    Ok(p.to_path_buf())
}

/// Validate an explicit output target before any write: absolute, the right
/// extension, an existing parent directory, no symlink anywhere on the path,
/// and no silent replacement.
pub fn output_target(raw: &str, suffix: &str, overwrite: bool) -> Result<PathBuf, &'static str> {
    if raw.is_empty() || raw.contains('\0') || !Path::new(raw).is_absolute() {
        return Err("output_path_must_be_absolute");
    }
    let path = PathBuf::from(raw);
    for anc in path.ancestors() {
        if anc.as_os_str().is_empty() {
            break;
        }
        if std::fs::symlink_metadata(anc).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
            return Err("output_target_symlink_denied");
        }
    }
    if !raw.to_lowercase().ends_with(&suffix.to_lowercase()) {
        return Err("invalid_output_extension");
    }
    match path.parent() {
        Some(parent) if parent.is_dir() => {}
        _ => return Err("output_directory_not_found"),
    }
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !meta.is_file() {
            return Err("output_target_not_regular");
        }
        if !overwrite {
            return Err("output_exists");
        }
    }
    Ok(path)
}

/// Exclusive create, or atomic replace with `overwrite`.
fn write_output(path: &Path, bytes: &[u8], overwrite: bool) -> Result<(), &'static str> {
    if !overwrite {
        let mut file = tempfile::Builder::new().prefix(".readmd-mcp-").tempfile_in(path.parent().ok_or("output_directory_not_found")?).map_err(|_| "write_failed")?;
        file.write_all(bytes).and_then(|_| file.as_file().sync_all()).map_err(|_| "write_failed")?;
        return file.persist_noclobber(path).map(|_| ()).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists { "output_exists" } else { "write_failed" }
        });
    }
    crate::content::write_bytes_atomic(path, bytes).map_err(|_| "write_failed")
}

/// Unwrap an in-process API answer: `Ok(body)` on 2xx with `ok != false`.
fn api(app: &Arc<App>, method: &str, target: &str, body: Option<&Value>) -> Result<Value, Value> {
    let (status, v) = server::call_in_process(app, method, target, body);
    if (200..300).contains(&status) && v.get("ok") != Some(&Value::Bool(false)) {
        return Ok(v);
    }
    let code = v.get("error_code").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
        if status == 404 { "not_found".into() } else { format!("http_{status}") }
    });
    let msg = v.get("error").and_then(Value::as_str).map(str::to_string).or_else(|| v.as_str().map(str::to_string));
    Err(error_result(&code, msg.as_deref()))
}

/// Read-only view of the AI provider config with every secret-like field removed.
fn scrub(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| {
                    let k = k.to_lowercase();
                    k != "key" && !["api_key", "api-key", "apikey", "secret", "password", "access_token", "authorization", "cookie"].iter().any(|marker| k.contains(marker)) && k != "token"
                })
                .map(|(k, v)| {
                    let value = if k.to_lowercase().ends_with("url") {
                        v.as_str().map(|url| json!(public_url(url))).unwrap_or_else(|| scrub(v))
                    } else { scrub(v) };
                    (k.clone(), value)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(scrub).collect()),
        other => other.clone(),
    }
}

fn providers_view(app: &Arc<App>) -> Result<Value, Value> {
    let cfg = api(app, "GET", "/api/ai/config", None)?;
    let mut providers = Vec::new();
    // `providers` is the legacy alias of `presets` in this payload.
    for key in ["presets", "custom"] {
        if let Some(Value::Array(list)) = cfg.get(key) {
            providers.extend(list.iter().cloned());
        }
    }
    Ok(scrub(&json!({
        "schema_version": cfg.get("schema_version").cloned().unwrap_or(Value::Null),
        "providers": providers,
        "current": cfg.get("current").cloned().unwrap_or(json!({})),
    })))
}

fn skill_variables(args: &Value) -> Value {
    let doc = arg_str(args, "markdown_content", "");
    json!({
        "document": if doc.is_empty() { "(no document supplied)".to_string() } else { doc },
        "selection": "",
        "request": arg_str(args, "request", ""),
        "language": arg_str(args, "language", "en"),
        "context": "",
        "output_format": "Markdown",
    })
}

// ---------------------------------------------------------------- tools/call

pub fn call_tool(app: &Arc<App>, name: &str, args: &Value) -> Value {
    call_tool_cancellable(app, name, args, None)
}

fn public_url(url: &str) -> String {
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let Some((scheme, rest)) = clean.split_once("://") else { return clean.to_owned() };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    format!("{scheme}://{authority}{tail}")
}

fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), Value> {
    if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) { Err(error_result("cancelled", None)) } else { Ok(()) }
}

fn call_tool_cancellable(app: &Arc<App>, name: &str, args: &Value, cancel: Option<&AtomicBool>) -> Value {
    if CONFIRM_REQUIRED.contains(&name) && args.get("confirm") != Some(&Value::Bool(true)) {
        return error_result("confirmation_required", None);
    }
    if name == "readmd_ai_chat" && (args.get("api_key").is_some() || args.get("key").is_some()) {
        return error_result("raw_key_rejected", None);
    }
    if let Err(code) = validate_tool_arguments(name, args) {
        return error_result(code, None);
    }
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { check_cancel(cancel)?; dispatch_tool(app, name, args, cancel) }));
    match run {
        Ok(Ok(v)) | Ok(Err(v)) => v,
        Err(_) => error_result("internal_error", None),
    }
}

/// Validate advertised types before dispatch; never silently turn an object or
/// boolean into empty document text or a default file path.
fn validate_tool_arguments(name: &str, args: &Value) -> Result<(), &'static str> {
    let catalog = tools();
    let tool = catalog.as_array().unwrap().iter().find(|t| t["name"] == name).ok_or("unknown_tool")?;
    let schema = &tool["inputSchema"];
    if !matches_tool_schema(args,schema,0){return Err("invalid_arguments");}
    if name == "readmd_generate_toc" {
        let from = arg_i64(args, "depth_from", 1);
        let to = arg_i64(args, "depth_to", 6);
        if !(1..=6).contains(&from) || !(from..=6).contains(&to) { return Err("invalid_arguments"); }
    }
    Ok(())
}

fn matches_tool_schema(value:&Value,schema:&Value,depth:usize)->bool{
    if depth>32||schema.get("const").is_some_and(|expected|expected!=value)||schema.get("enum").and_then(Value::as_array).is_some_and(|allowed|!allowed.contains(value)){return false;}
    if let Some(allowed)=schema["type"].as_array(){
        return allowed.iter().any(|kind|{
            if !kind.is_string(){return false;}
            let mut branch=schema.clone();branch["type"]=kind.clone();
            matches_tool_schema(value,&branch,depth+1)
        });
    }
    if schema["anyOf"].as_array().is_some_and(|branches|!branches.iter().any(|branch|matches_tool_schema(value,branch,depth+1))){return false;}
    if schema["allOf"].as_array().is_some_and(|branches|!branches.iter().all(|branch|matches_tool_schema(value,branch,depth+1))){return false;}
    if schema["oneOf"].as_array().is_some_and(|branches|branches.iter().filter(|branch|matches_tool_schema(value,branch,depth+1)).count()!=1){return false;}
    match schema["type"].as_str().unwrap_or(""){
        "string"=>value.as_str().is_some_and(|s|schema["minLength"].as_u64().is_none_or(|n|s.chars().count() as u64>=n)&&schema["maxLength"].as_u64().is_none_or(|n|s.chars().count() as u64<=n)),
        "boolean"=>value.is_boolean(),
        "integer"|"number"=>{
            if schema["type"]=="integer"&&!value.is_i64()&&!value.is_u64(){return false;}
            value.as_f64().is_some_and(|n|schema["minimum"].as_f64().is_none_or(|min|n>=min)&&schema["maximum"].as_f64().is_none_or(|max|n<=max))
        },
        "object"=>value.as_object().is_some_and(|object|{
            if schema["required"].as_array().is_some_and(|required|required.iter().any(|key|key.as_str().is_none_or(|key|!object.contains_key(key)))){return false;}
            object.iter().all(|(key,value)|match schema["properties"].get(key){
                Some(prop)=>matches_tool_schema(value,prop,depth+1),
                None=>match schema.get("additionalProperties"){Some(Value::Bool(false))=>false,Some(prop)if prop.is_object()=>matches_tool_schema(value,prop,depth+1),_=>true},
            })
        }),
        "array"=>value.as_array().is_some_and(|array|{
            schema["minItems"].as_u64().is_none_or(|n|array.len() as u64>=n)&&schema["maxItems"].as_u64().is_none_or(|n|array.len() as u64<=n)&&schema.get("items").is_none_or(|items|array.iter().all(|v|matches_tool_schema(v,items,depth+1)))&&(!schema["uniqueItems"].as_bool().unwrap_or(false)||array.iter().enumerate().all(|(i,item)|!array[..i].contains(item)))
        }),
        "null"=>value.is_null(),
        ""=>schema.get("type").is_none()&&schema.is_object(),
        _=>false,
    }
}

fn dispatch_tool(app: &Arc<App>, name: &str, args: &Value, cancel: Option<&AtomicBool>) -> Result<Value, Value> {
    crate::ocr_portable::with_paths(&app.paths,||dispatch_tool_with_profile(app,name,args,cancel))
}
fn dispatch_tool_with_profile(app: &Arc<App>, name: &str, args: &Value, cancel: Option<&AtomicBool>) -> Result<Value, Value> {
    match name {
        "readmd_publication_metadata" => {
            let (status,body)=server::call_in_process(app,"POST","/api/publication/metadata",Some(args));
            if (200..300).contains(&status)&&body["ok"]!=false {Ok(json_result(&body))}
            else {Err(metadata_error_result(body))}
        },
        "readmd_publication_blocks" => Ok(json_result(&api(app,"POST","/api/publication/blocks",Some(args))?)),
        "readmd_pdf_document" => crate::cancel::with_external(cancel,||Ok(json_result(&api(app,"POST","/api/pdf/workspace",Some(args))?))),
        "readmd_academic_document" => Ok(json_result(&api(app,"POST","/api/academic/document",Some(args))?)),
        "readmd_openapi_document" => crate::cancel::with_external(cancel,||Ok(json_result(&api(app,"POST","/api/developer/openapi",Some(args))?))),
        "readmd_content_transaction" => Ok(json_result(&api(app,"POST","/api/documents/transaction",Some(args))?)),
        "readmd_math" => {
            let operation=args["operation"].as_str().unwrap_or("parse");
            let target=match operation {"parse"=>"/api/math/parse","patch"=>"/api/math/patch","serialize"=>"/api/math/serialize","migrate"=>"/api/math/migrate","schema"=>"/api/math/schema",_=>return Err(error_result("invalid_arguments",None))};
            Ok(json_result(&api(app,if operation=="schema"{"GET"}else{"POST"},target,if operation=="schema"{None}else{Some(args)})?))
        },
        "readmd_suggest_links" | "readmd_resolve_link" => {
            if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) { return Err(error_result("cancelled", None)); }
            let root=arg_str(args,"workspace_root",""); let file=arg_str(args,"file_path","");
            let file=(!file.is_empty()).then(||Path::new(&file));
            let result=if name == "readmd_resolve_link" {
                crate::document_intelligence::resolve_knowledge_link(Path::new(&root),file,&arg_str(args,"target",""))
            } else {
                crate::document_intelligence::suggest_knowledge_links(Path::new(&root),file,&arg_str(args,"content",""),&arg_str(args,"kind",""),&arg_str(args,"query",""),arg_i64(args,"limit",40).try_into().unwrap_or(0))
            };
            if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) { return Err(error_result("cancelled", None)); }
            result.map(|v|json_result(&v)).map_err(|code|error_result(&code,None))
        }
        "readmd_analyze_document" => {
            let file = arg_str(args, "file_path", ""); let root = arg_str(args, "workspace_root", "");
            if (!file.is_empty() && !Path::new(&file).is_absolute()) || (!root.is_empty() && !Path::new(&root).is_absolute()) {
                return Err(error_result("file_path_must_be_absolute", None));
            }
            crate::document_intelligence::analyze(&arg_str(args,"content",""),
                (!file.is_empty()).then(||Path::new(&file)), (!root.is_empty()).then(||Path::new(&root)))
                .map(|v|json_result(&v)).map_err(|code|error_result(&code,None))
        }
        "readmd_search_workspace" => crate::document_intelligence::search_workspace(
            Path::new(&arg_str(args,"workspace_root","")), &arg_str(args,"query",""),
            arg_i64(args,"limit",20).try_into().unwrap_or(0),cancel)
            .map(|v|json_result(&v)).map_err(|code|error_result(&code,None)),
        "readmd_read_document" => crate::document_intelligence::read_document(
            Path::new(&arg_str(args,"file_path","")),arg_i64(args,"start_line",1).try_into().unwrap_or(0),
            arg_i64(args,"end_line",1000).try_into().unwrap_or(0))
            .map(|v|json_result(&v)).map_err(|code|error_result(&code,None)),
        "readmd_edit_document" => crate::document_intelligence::edit_document(app,
            Path::new(&arg_str(args,"file_path","")), &arg_str(args,"expected_revision",""),
            &arg_str(args,"old_text",""),&arg_str(args,"new_text",""),arg_bool(args,"replace_all",false),
            arg_bool(args,"dry_run",true),arg_bool(args,"confirm",false),cancel)
            .map(|v|json_result(&v)).map_err(|code|error_result(&code,None)),
        "readmd_document_history" => {
            let raw=arg_str(args,"file_path","");
            let source=Path::new(&raw);
            if !source.is_absolute() { return Err(error_result("file_path_must_be_absolute",None)); }
            let path=crate::paths::canonicalize_or_clean(source);
            if path.is_dir() || !crate::content::is_readable(&path) { return Err(error_result("unsupported_text_file",None)); }
            let key=crate::paths::canonicalize_or_clean(&path).to_string_lossy().into_owned();
            if arg_str(args,"operation","list")=="read" {
                let (entry,text)=crate::document_history::read(&app.paths.data_dir,&arg_str(args,"checkpoint_id",""))
                    .map_err(|_|error_result("history_not_found",None))?;
                if entry.doc_key!=key { return Err(error_result("history_document_mismatch",None)); }
                Ok(json_result(&json!({"ok":true,"entry":entry,"content":text})))
            } else {
                let entries=crate::document_history::list(&app.paths.data_dir,Some(&key)).map_err(|_|error_result("history_read_failed",None))?;
                Ok(json_result(&json!({"ok":true,"entries":entries})))
            }
        }
        "readmd_fix_markdown" => {
            let res = crate::readmd_fix::fix_markdown(&arg_str(args, "content", ""));
            Ok(json_result(&json!({
                "ok": true, "repaired_content": res.text, "fixes_count": res.fixes.len(),
                "fixes_details": res.fixes, "stats": res.stats,
            })))
        }
        "readmd_convert_to_markdown" => {
            let path = existing_file(&arg_str(args, "file_path", ""))?;
            let t = crate::plugin_manager::convert_document(&crate::plugin_manager::Sandbox::for_app(app), &path.to_string_lossy(), true, None);
            if let Some(err) = t.error {
                return Err(error_result("conversion_failed", Some(&err)));
            }
            if t.text.trim().is_empty() {
                return Err(error_result("convert_no_text", Some("未提取到文字；如果是扫描件，可使用 readmd_ocr_to_markdown")));
            }
            Ok(text_result(t.text))
        }
        "readmd_web_to_markdown" => {
            let url = arg_str(args, "url", "").trim().to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(error_result("invalid_url", Some("URL 必须以 http:// 或 https:// 开头")));
            }
            let doc = crate::parity_web::fetch_document(&url, "smart", 30, "")
                .map_err(|e| error_result(&e.code, Some(&e.message)))?;
            if doc.get("ok") != Some(&Value::Bool(true)) {
                let code = doc.get("error_code").and_then(Value::as_str).unwrap_or("web_extract_failed");
                return Err(error_result(code, doc.get("error").and_then(Value::as_str)));
            }
            Ok(json_result(&json!({
                "ok": true,
                "title": doc.pointer("/meta/title").cloned().unwrap_or(json!("")),
                "markdown": doc.get("content").cloned().unwrap_or(json!("")),
                "url": url,
                "engine": doc.get("engine").cloned().unwrap_or(Value::Null),
                "warnings": doc.get("warnings").cloned().unwrap_or(json!([])),
            })))
        }
        "readmd_ocr_to_markdown" => {
            let path = existing_file(&arg_str(args, "file_path", ""))?;
            let text = crate::cancel::with_external(cancel,||crate::ocr::ocr_any(&path.to_string_lossy())).map_err(|e| {
                let code = serde_json::to_value(&e.error_code).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "ocr_failed".into());
                error_result(&code, Some(&e.code))
            })?;
            Ok(text_result(text))
        }
        "readmd_export_document" => export_document(app, args, cancel),
        "readmd_latex_to_md" => Ok(text_result(crate::texmd::latex_to_markdown(&arg_str(args, "latex_content", ""), ""))),
        "readmd_md_to_latex" => {
            let title = arg_str(args, "doc_title", "ReadMD Document");
            let tex = crate::texmd::md_to_latex(&arg_str(args, "markdown_content", ""), &title, "", true, &crate::texmd::OptVal::Null);
            Ok(text_result(tex))
        }
        "readmd_parse_bibtex" => {
            let raw = arg_str(args, "bib_file_path", "");
            let p = Path::new(&raw);
            if raw.is_empty() || !p.exists() {
                return Err(error_result("file_not_found", Some(&format!("文件不存在: {raw}"))));
            }
            let entries: Vec<Value> = if p.is_file() && raw.to_lowercase().ends_with(".bib") {
                let text = crate::content::read_text(p).map_err(|e| error_result("bibtex_failed", Some(&e.to_string())))?;
                server::parse_bibtex(&text)
            } else {
                let mut all = Vec::new();
                let dir = if p.is_dir() { p.to_path_buf() } else { p.parent().map(Path::to_path_buf).unwrap_or_default() };
                if let Ok(read) = std::fs::read_dir(&dir) {
                    let mut bibs: Vec<PathBuf> = read.flatten().map(|e| e.path())
                        .filter(|q| q.extension().map(|x| x.eq_ignore_ascii_case("bib")).unwrap_or(false)).collect();
                    bibs.sort();
                    for b in bibs {
                        if let Ok(text) = crate::content::read_text(&b) {
                            all.extend(server::parse_bibtex(&text));
                        }
                    }
                }
                all
            };
            Ok(json_result(&json!({ "ok": true, "count": entries.len(), "entries": entries })))
        }
        "readmd_latex_to_omml" => crate::latex2omml::latex_to_native_omml(
            &arg_str(args, "latex_formula", ""),
            arg_bool(args, "display", false),
        ).map(|xml|text_result(xml)).map_err(|code|error_result(code,None)),
        "readmd_ai_assistant" => {
            let wf = arg_str(args, "workflow_id", "");
            let requested = [arg_str(args, "skill_id", ""), wf.clone()].into_iter().find(|s| !s.trim().is_empty()).unwrap_or_else(|| "readmd-quick-read".into());
            let id = resolve_skill_id(&requested);
            let vars = skill_variables(args);
            let prompt = server::skill_render(app, &id, &vars).map_err(|e| error_result("skill_not_found", Some(&e)))?;
            let desc = server::skill_list(app).into_iter().find(|s| s.id == id).map(|s| s.description).unwrap_or_default();
            Ok(json_result(&json!({
                "workflow_id": wf, "skill_id": id, "workflow_name": id, "description": desc,
                "system_prompt": prompt, "user_payload": vars["document"],
            })))
        }
        "readmd_ai_providers" => Ok(json_result(&providers_view(app)?)),
        "readmd_export_presets" => Ok(json_result(&api(app, "GET", "/api/export/presets", None)?)),
        "readmd_ai_models" => Ok(json_result(&api(app, "POST", "/api/ai/models", Some(args))?)),
        "readmd_render_diagram" => Ok(json_result(&api(app, "POST", "/api/diagram/render", Some(&json!({ "engine": args["engine"], "code": args["code"], "allow_remote": false })))?)),
        "readmd_ai_complete" => Ok(json_result(&api(app, "POST", "/api/ai/complete", Some(args))?)),
        "readmd_ai_chat" => {
            if args.get("api_key").is_some() || args.get("key").is_some() {
                return Err(error_result("raw_key_rejected", Some("AI Chat 只接受 credential_id，不接受 API Key")));
            }
            let (provider, skill_id, doc) = (arg_str(args, "provider", ""), arg_str(args, "skill_id", ""), arg_str(args, "markdown_content", ""));
            if provider.is_empty() || skill_id.is_empty() || doc.is_empty() {
                return Err(error_result("invalid_request", Some("provider、skill_id、markdown_content 均为必填")));
            }
            let body = json!({
                "provider": provider,
                "credential_id": arg_str(args, "credential_id", ""),
                "model": arg_str(args, "model", ""),
                "skill_id": resolve_skill_id(&skill_id),
                "skill_variables": skill_variables(args),
                "messages": [{ "role": "user", "content": doc }],
                "stream": false,
            });
            let v = api(app, "POST", "/api/ai/chat", Some(&body))?;
            Ok(json_result(&json!({
                "ok": true,
                "content": v.get("content").cloned().unwrap_or(json!("")),
                "usage": v.get("usage").cloned().unwrap_or(Value::Null),
                "skill_id": skill_id,
            })))
        }
        "readmd_process_imports" => {
            let base = arg_str(args, "base_dir", "");
            let base = if base.is_empty() { std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default() } else { base };
            let out = crate::import_processor::process_markdown_imports(&arg_str(args, "markdown_content", ""), &base, None, None, None)
                .map_err(|e| {
                    let code = serde_json::to_value(&e.error_code).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "import_failed".into());
                    error_result(&code, Some(&e.code))
                })?;
            Ok(text_result(out))
        }
        "readmd_generate_toc" => {
            let headings = crate::toc_engine::extract_headings(&arg_str(args, "markdown_content", ""));
            Ok(text_result(crate::toc_engine::generate_toc_markdown(
                &headings,
                arg_i64(args, "depth_from", 1),
                arg_i64(args, "depth_to", 6),
                arg_bool(args, "ordered_list", false),
                None,
            )))
        }
        "readmd_export_presentation" => {
            let overwrite = arg_bool(args, "overwrite", false);
            let out = output_target(&arg_str(args, "output_path", ""), ".html", overwrite).map_err(|c| error_result(c, None))?;
            let base = arg_str(args, "base_dir", &out.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
            let mut warnings = Vec::new();
            let markdown = crate::mdexport::embed_images_for_html(&arg_str(args, "markdown_content", ""), &base, &mut warnings);
            let html = crate::mdexport::render_presentation_html(
                &markdown,
                &arg_str(args, "title", "ReadMD Presentation"),
                &arg_str(args, "theme", "black"),
                &arg_str(args, "transition", "slide"),
                true,
                &app.paths.assets_dir,
            )
            .map_err(|e| error_result("presentation_export_failed", Some(&e)))?;
            check_cancel(cancel)?;
            write_output(&out, html.as_bytes(), overwrite).map_err(|c| error_result(c, None))?;
            Ok(json_result(&json!({ "ok": true, "output_path": out, "file_size": html.len(), "warnings": warnings })))
        }
        "readmd_export_epub" => {
            let overwrite = arg_bool(args, "overwrite", false);
            let out = output_target(&arg_str(args, "output_path", ""), ".epub", overwrite).map_err(|c| error_result(c, None))?;
            let base = arg_str(args, "base_dir", &out.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
            let content=arg_str(args,"markdown_content","");
            let options=args.get("options").unwrap_or(&Value::Null);
            let title=arg_str(args,"title","ReadMD 电子书");
            let (bytes, warns) = crate::mdexport::epub_build_bytes(
                &json!(content),
                &base,
                options,
                &title,
                &arg_str(args, "author", "ReadMD Author"),
                &arg_str(args, "language", "zh-CN"),
                &uuid::Uuid::new_v4().to_string(),
                &crate::mdexport::now_iso_z(),
            )
            .map_err(|e|crate::publication_metadata_formats::error_body(&content,options,&title,"epub",&e).map(metadata_error_result).unwrap_or_else(||export_error(Some(&e))))?;
            check_cancel(cancel)?;
            write_output(&out, &bytes, overwrite).map_err(|c| error_result(c, None))?;
            Ok(json_result(&json!({ "ok": true, "output_path": out, "file_size": bytes.len(), "warnings": warns })))
        }
        "readmd_run_code_chunk" => {
            let body = json!({
                "code": arg_str(args, "code", ""),
                "lang": arg_str(args, "language", "python"),
                "capture_plot": arg_bool(args, "capture_plot", true),
                "confirm": true,
            });
            Ok(json_result(&api(app, "POST", "/api/code/run", Some(&body))?))
        }
        "readmd_pdf_audit" => {
            let path = existing_file(&arg_str(args, "pdf_path", ""))?;
            let v = crate::pdf_editor::audit(&path.to_string_lossy()).map_err(|e| error_result("pdf_audit_failed", Some(&e.message)))?;
            Ok(json_result(&v))
        }
        "readmd_pdf_rollback" => {
            let raw = arg_str(args, "pdf_path", "");
            if raw.is_empty() || !Path::new(&raw).is_absolute() {
                return Err(error_result("path_must_be_absolute", None));
            }
            let v = crate::pdf_editor::rollback(&raw).map_err(|e| error_result("pdf_rollback_failed", Some(&e.message)))?;
            Ok(json_result(&v))
        }
        _ => Err(error_result("unknown_tool", Some(&format!("未知的工具名称: {name}")))),
    }
}

fn export_error(message:Option<&str>)->Value {
    let Some(message)=message else{return error_result("export_failed",None)};
    let body=crate::latex_compile::error_body(message);
    if matches!(body["error_code"].as_str(),Some(crate::api_codes::PDF_MISSING_GLYPH | crate::api_codes::PUBLICATION_LAYOUT_GEOMETRY))
        &&body["details"].is_object(){return metadata_error_result(body);}
    error_result(body["error_code"].as_str().unwrap_or("export_failed"),Some(message))
}
fn metadata_error_result(body: Value) -> Value {
    let mut result=json_result(&body);
    result["isError"]=json!(true);
    result
}

fn export_document(app: &Arc<App>, args: &Value, cancel: Option<&AtomicBool>) -> Result<Value, Value> {
    let fmt = arg_str(args, "output_format", "pdf").to_lowercase();
    if !matches!(fmt.as_str(), "pdf" | "docx" | "html" | "tex") {
        return Err(error_result("unsupported_output_format", None));
    }
    let overwrite = arg_bool(args, "overwrite", false);
    let out = output_target(&arg_str(args, "output_path", ""), &format!(".{fmt}"), overwrite).map_err(|c| error_result(c, None))?;
    let base = arg_str(args, "base_dir", &out.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
    let preset = arg_str(args, "style_preset", "minimal");
    let catalog = api(app, "GET", "/api/export/presets", None)?;
    let preset_options = catalog["presets"].get(&preset).or_else(|| catalog["custom"].get(&preset))
        .ok_or_else(|| error_result("unknown_style_preset", None))?;
    let inherited = crate::publication_typography::ResolvedOptions::from_request(preset_options).map_err(|code| export_error(Some(code)))?;
    let mut options=crate::export_styles::sanitize(preset_options);
    if let Some(meta)=preset_options.get("meta"){options["meta"]=meta.clone();}
    if args.get("options").is_some_and(|v|!v.is_object()){return Err(error_result("invalid_options",None));}
    let resolved = inherited.with_completed_values(options).map_err(|code| export_error(Some(code)))?
        .merge(args.get("options").unwrap_or(&json!({})))
        .map_err(|error|metadata_error_result(error.response_body()))?;
    let options = resolved.values();
    let title = arg_str(args, "title", "ReadMD Document");
    let content=arg_str(args,"markdown_content","");
    crate::publication_typography::validate_export(&fmt, &content, &options).map_err(|code| export_error(Some(code)))?;
    // TeX has a companion assets directory and commits when rendering begins.
    // Single-file formats are rendered in isolation before atomic publication.
    check_cancel(cancel)?;
    let stage = if fmt == "tex" { None } else {
        Some(crate::task_workspace::Workspace::create(&app.paths.data_dir, "mcp-export")
            .map_err(|_| error_result("write_failed", None))?)
    };
    let write_to = stage.as_ref().map(|dir| dir.path().join(out.file_name().unwrap())).unwrap_or_else(|| out.clone());
    let out_s = write_to.to_string_lossy().into_owned();
    let render = || crate::cancel::with_external(cancel,||crate::mdexport::export_document_with_resolved_options(&fmt, &content, &base, &out_s, &resolved, &title, &app.paths.assets_dir,&app.paths.data_dir));
    let res = stage.as_ref().map_or_else(render, |stage| stage.scope(render))
        .map_err(|e|crate::publication_metadata_formats::error_body(&content,&options,&title,&fmt,&e).map(metadata_error_result).unwrap_or_else(||export_error(Some(&e))))?;
    if !res.ok {
        return Err(export_error(res.error.as_deref()));
    }
    if stage.is_some() {
        let bytes = std::fs::read(&write_to).map_err(|_| error_result("export_failed", None))?;
        check_cancel(cancel)?;
        stage.as_ref().unwrap().finish().map_err(|_|error_result("write_failed",None))?;
        write_output(&out, &bytes, overwrite).map_err(|code| error_result(code, None))?;
    }
    let warns = res.warns.unwrap_or_default();
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    Ok(json_result(&json!({
        "ok": true, "format": fmt, "output_path": out, "file_size": size,
        "warnings": warns, "warn_items": crate::api_codes::warn_items(&warns),
    })))
}

// ---------------------------------------------------------------- resources / prompts

fn resources(app: &Arc<App>) -> Value {
    let mut out: Vec<Value> = server::skill_list(app)
        .into_iter()
        .map(|s| json!({ "uri": format!("readmd://skills/{}", s.id), "name": s.id, "description": s.description, "mimeType": "text/markdown" }))
        .collect();
    out.push(json!({ "uri": "readmd://providers", "name": "ReadMD AI providers",
        "description": "Provider catalog and connection state; secrets are omitted.", "mimeType": "application/json" }));
    out.push(json!({ "uri": "readmd://sessions", "name": "ReadMD AI sessions",
        "description": "Local AI chat history (read-only).", "mimeType": "application/json" }));
    json!({ "resources": out })
}

fn read_resource(app: &Arc<App>, uri: &str) -> Result<Value, (i64, String)> {
    let one = |mime: &str, text: String| json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] });
    match uri {
        "readmd://providers" => {
            let v = providers_view(app).map_err(|_| (-32603, "provider config unavailable".to_string()))?;
            Ok(one("application/json", v.to_string()))
        }
        "readmd://sessions" => {
            let (_, v) = server::call_in_process(app, "GET", "/api/ai/history", None);
            Ok(one("application/json", scrub(&v).to_string()))
        }
        _ => {
            let id = uri.strip_prefix("readmd://skills/").unwrap_or("");
            if id.is_empty() || id.contains('/') || id.contains('\\') {
                return Err((-32602, format!("unknown resource: {uri}")));
            }
            let skill = server::skill_list(app).into_iter().find(|s| s.id == id).ok_or((-32602, format!("Skill not found: {id}")))?;
            Ok(one("text/markdown", skill.instructions))
        }
    }
}

fn prompts(app: &Arc<App>) -> Value {
    let args = json!([
        { "name": "markdown_content", "description": "Document text", "required": false },
        { "name": "request", "description": "User request", "required": false },
        { "name": "language", "description": "Output language", "required": false },
    ]);
    let list: Vec<Value> = server::skill_list(app)
        .into_iter()
        .map(|s| json!({ "name": s.id, "description": s.description, "arguments": args }))
        .collect();
    json!({ "prompts": list })
}

fn get_prompt(app: &Arc<App>, params: &Value) -> Result<Value, (i64, String)> {
    let id = resolve_skill_id(&arg_str(params, "name", "readmd-quick-read"));
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let text = server::skill_render(app, &id, &skill_variables(&args)).map_err(|e| (-32602, e))?;
    Ok(json!({ "description": id, "messages": [{ "role": "user", "content": { "type": "text", "text": text } }] }))
}

// ---------------------------------------------------------------- JSON-RPC loop

/// Serialised writer: responses from worker threads never interleave.
#[derive(Clone)]
pub struct Out(Arc<Mutex<Box<dyn Write + Send>>>, Option<String>);

impl Out {
    pub fn new(w: Box<dyn Write + Send>) -> Out {
        Out(Arc::new(Mutex::new(w)), None)
    }
    fn send(&self, v: &Value) {
        let mut w = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(w, "{v}");
        let _ = w.flush();
    }
}

fn reply(out: &Out, id: &Value, mut result: Value) {
    if let Some(method) = &out.1 {
        result["resultType"] = json!("complete");
        result["_meta"] = json!({ "io.modelcontextprotocol/serverInfo": { "name": "readmd", "title": "ReadMD", "version": server::VERSION } });
        if matches!(method.as_str(), "server/discover" | "tools/list" | "prompts/list" | "resources/list" | "resources/read" | "resources/templates/list") {
            // Resource/Skill catalogs depend on the local user's current files.
            result["ttlMs"] = json!(0);
            result["cacheScope"] = json!("private");
        }
    }
    out.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
}

fn reply_err(out: &Out, id: &Value, code: i64, message: &str) {
    out.send(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
}

/// Handle one incoming line.  `tools/call` runs on a worker thread so the loop
/// keeps reading (`ping`, further calls) while a long export or OCR runs.
pub fn handle_line(app: &Arc<App>, out: &Out, busy: &Arc<AtomicUsize>, cancelled: &Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return reply_err(out, &Value::Null, -32700, "Parse error"),
    };
    let Some(obj) = msg.as_object() else {
        return reply_err(out, &Value::Null, -32600, "Invalid Request");
    };
    let method = obj.get("method").and_then(Value::as_str).unwrap_or("");
    let params = obj.get("params").cloned().unwrap_or(json!({}));
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_empty() ||
        obj.get("id").is_some_and(|id| !(id.is_string() || id.is_i64() || id.is_u64())) {
        return reply_err(out, &Value::Null, -32600, "Invalid Request");
    }
    let Some(id) = obj.get("id").cloned() else {
        // Notification: never answered.
        if method == "notifications/cancelled" {
            if let Some(rid) = params.get("requestId") {
                if let Some(flag) = cancelled.lock().unwrap_or_else(|e| e.into_inner()).get(&rid.to_string()) {
                    flag.store(true, Ordering::SeqCst);
                }
            }
        }
        return;
    };
    if !params.is_object() { return reply_err(out, &id, -32602, "Invalid params"); }
    let requested = params.get("_meta").and_then(|m| m.get(PROTOCOL_META));
    if let Some(version) = requested {
        if !version.as_str().is_some_and(|v| PROTOCOL_VERSIONS.contains(&v)) {
            out.send(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32022,
                "message": "Unsupported protocol version", "data": { "supported": PROTOCOL_VERSIONS, "requested": version } } }));
            return;
        }
    }
    let modern = requested.and_then(Value::as_str) == Some(MODERN_PROTOCOL);
    if (modern && !params["_meta"]["io.modelcontextprotocol/clientCapabilities"].is_object()) ||
        (method == "server/discover" && !modern) {
        return reply_err(out, &id, -32602, "Per-request protocol metadata and client capabilities required");
    }
    let response_out = Out(out.0.clone(), modern.then(|| method.to_owned()));
    let out = &response_out;
    if matches!(method, "tools/list" | "resources/list" | "prompts/list" | "resources/templates/list") && params.get("cursor").is_some() {
        return reply_err(out, &id, -32602, "Invalid cursor");
    }
    match method {
        "server/discover" => reply(out, &id, json!({ "supportedVersions": PROTOCOL_VERSIONS,
            "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
            "instructions": "Local ReadMD kernel. File writes, URL fetches and code execution require confirm: true. Credentials are referenced by id, never supplied as raw keys." })),
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = if asked != MODERN_PROTOCOL && PROTOCOL_VERSIONS.contains(&asked) { asked } else { LEGACY_PROTOCOL };
            reply(out, &id, json!({
                "protocolVersion": version,
                "serverInfo": { "name": "readmd", "title": "ReadMD", "version": server::VERSION },
                "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
                "instructions": "ReadMD converts documents to Markdown, repairs Markdown, and exports PDF/DOCX/HTML/LaTeX/EPUB/slides. Tools that write files, fetch URLs or run code need confirm: true.",
            }));
        }
        "ping" => reply(out, &id, json!({})),
        "tools/list" => reply(out, &id, json!({ "tools": tools() })),
        "resources/list" => reply(out, &id, resources(app)),
        "resources/templates/list" => reply(out, &id, json!({ "resourceTemplates": [] })),
        "resources/read" => match read_resource(app, &arg_str(&params, "uri", "")) {
            Ok(v) => reply(out, &id, v),
            Err((c, m)) => reply_err(out, &id, c, &m),
        },
        "prompts/list" => reply(out, &id, prompts(app)),
        "prompts/get" => match get_prompt(app, &params) {
            Ok(v) => reply(out, &id, v),
            Err((c, m)) => reply_err(out, &id, c, &m),
        },
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str).map(str::to_string) else {
                return reply_err(out, &id, -32602, "Invalid params");
            };
            let args = match params.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(v @ Value::Object(_)) => v.clone(),
                Some(_) => return reply_err(out, &id, -32602, "Invalid params"),
            };
            let mut requests = cancelled.lock().unwrap_or_else(|e| e.into_inner());
            if requests.contains_key(&id.to_string()) { return reply_err(out, &id, -32600, "Duplicate in-flight request id"); }
            if busy.fetch_add(1, Ordering::SeqCst) >= MAX_CONCURRENT_TOOLS {
                busy.fetch_sub(1, Ordering::SeqCst);
                return reply_err(out, &id, -32001, "server_busy");
            }
            let flag = Arc::new(AtomicBool::new(false));
            requests.insert(id.to_string(), flag.clone());
            drop(requests);
            let progress_token = params.get("_meta").and_then(|m| m.get("progressToken")).filter(|token| token.is_string() || token.is_i64() || token.is_u64()).cloned();
            let spawn_id = id.clone();
            let (app, out, busy, cancelled) = (app.clone(), out.clone(), busy.clone(), cancelled.clone());
            let (failed_out, failed_busy, failed_cancelled) = (out.clone(), busy.clone(), cancelled.clone());
            if std::thread::Builder::new()
                .name("mcp-tool".into())
                .spawn(move || {
                    // Keep clients' inactivity timers alive during synchronous
                    // OCR/export/provider requests. These are status messages,
                    // never AI text deltas. Disconnecting the channel stops it
                    // immediately when work finishes.
                    let (done, receiver) = std::sync::mpsc::channel::<()>();
                    let activity = progress_token.and_then(|token| {
                        let (out, flag) = (out.clone(), flag.clone());
                        std::thread::Builder::new().name("mcp-progress".into()).spawn(move || {
                            let mut progress: u64 = 0;
                            while receiver.recv_timeout(std::time::Duration::from_secs(10)) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
                                if flag.load(Ordering::SeqCst) { break; }
                                progress += 1;
                                out.send(&json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": {
                                    "progressToken": token, "progress": progress, "message": "Working" } }));
                            }
                        }).ok()
                    });
                    let result = call_tool_cancellable(&app, &name, &args, Some(&flag));
                    drop(done);
                    if let Some(activity) = activity { let _ = activity.join(); }
                    // A cancelled request gets no response (MCP spec).
                    if !flag.load(Ordering::SeqCst) {
                        reply(&out, &id, result);
                    }
                    cancelled.lock().unwrap_or_else(|e| e.into_inner()).remove(&id.to_string());
                    busy.fetch_sub(1, Ordering::SeqCst);
                }).is_err() {
                failed_cancelled.lock().unwrap_or_else(|e| e.into_inner()).remove(&spawn_id.to_string());
                failed_busy.fetch_sub(1, Ordering::SeqCst);
                reply_err(&failed_out, &spawn_id, -32603, "Worker unavailable");
            }
        }
        _ => reply_err(out, &id, -32601, &format!("Method not found: {method}")),
    }
}

/// Serve MCP on stdin/stdout until stdin closes; waits for running tools.
pub fn run_stdio(app: Arc<App>) {
    let out = Out::new(Box::new(std::io::stdout()));
    let busy = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(Mutex::new(HashMap::new()));
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        match read_message(&mut input, MAX_MESSAGE_BYTES) {
            Ok(None) => break,
            Ok(Some(Ok(line))) => handle_line(&app, &out, &busy, &cancelled, &line),
            Ok(Some(Err(code))) => reply_err(&out, &Value::Null, code, "Invalid or oversized message"),
            Err(_) => break,
        }
    }
    while busy.load(Ordering::SeqCst) > 0 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn managed_export_keeps_retryable_busy_code_and_original_output() {
        let root=tempfile::tempdir().unwrap();let data=root.path().join("profile");
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&data,root.path(),&root.path().join("assets"))).unwrap());
        let output=root.path().join("Protected.pdf");std::fs::write(&output,b"KEEP ORIGINAL").unwrap();
        static ACTIVE:AtomicUsize=AtomicUsize::new(0);
        let _first=crate::capability_process::reserve_worker(&data,"typst",&ACTIVE).unwrap();
        let _second=crate::capability_process::reserve_worker(&data,"typst",&ACTIVE).unwrap();
        let result=call_tool(&app,"readmd_export_document",&json!({"markdown_content":"```readmd-math v999\nx\n```","output_path":output,"output_format":"pdf","options":{"pdf":{"engine":"typst"}},"overwrite":true,"confirm":true}));
        assert_eq!(result["isError"],true);assert_eq!(result["structuredContent"]["error_code"],"typst_busy");
        assert_eq!(std::fs::read(&output).unwrap(),b"KEEP ORIGINAL");
        assert!(!std::fs::read_dir(root.path()).unwrap().any(|e|e.unwrap().file_name().to_string_lossy().starts_with(".readmd-export-")));
        assert_eq!(export_error(Some("cancelled"))["structuredContent"]["error_code"],"cancelled");
        assert_eq!(export_error(Some("typst_timeout"))["structuredContent"]["error_code"],"typst_timeout");
        assert_eq!(export_error(Some("unrecognized render failure"))["structuredContent"]["error_code"],"export_failed");
    }

    #[test]
    fn math_layout_parameters_share_desktop_bounds_and_preserve_legacy_calls() {
        assert!(validate_tool_arguments("readmd_math", &json!({"operation":"parse","source":"x","dialect":"latex"})).is_ok());
        for (size,width) in [(4,1),(12,96),(512,20000)] {
            assert!(validate_tool_arguments("readmd_math", &json!({"operation":"parse","source":"x","font_size":size,"layout_width":width})).is_ok());
        }
        for value in [json!(0),json!(513),json!("12"),json!(null)] {
            assert!(validate_tool_arguments("readmd_math", &json!({"operation":"parse","source":"x","font_size":value})).is_err());
        }
        for value in [json!(0),json!(20001),json!("96"),json!(null)] {
            assert!(validate_tool_arguments("readmd_math", &json!({"operation":"parse","source":"x","layout_width":value})).is_err());
        }
        assert!(validate_tool_arguments("readmd_math", &json!({"operation":"parse","source":"x","metadata":"old source-associated payload"})).is_ok());
        assert!(validate_tool_arguments("readmd_math", &json!({"operation":"parse","source":"x","metadata":{}})).is_err());
    }
    #[test]
    fn structured_parameters_validate_against_the_declared_tool_schema(){
        assert!(validate_tool_arguments("readmd_export_document",&json!({"markdown_content":"# test","output_path":"fixture.pdf","output_format":"pdf","options":{"pdf":{"engine":"xelatex"}},"confirm":true})).is_ok());
        assert!(validate_tool_arguments("readmd_pdf_document",&json!({"action":"edit","session":"id","revision":1,"operation":{"kind":"text","text":"hello"},"confirm":true})).is_ok());
        assert!(validate_tool_arguments("readmd_math",&json!({"operation":"patch","patch":{"op":"replace","node":"r1"}})).is_ok());
        assert!(validate_tool_arguments("readmd_pdf_document",&json!({"action":"edit","operation":[],"confirm":true})).is_err());
        assert!(validate_tool_arguments("readmd_pdf_document",&json!({"action":"edit","unexpected":true,"confirm":true})).is_err());
        assert!(validate_tool_arguments("readmd_pdf_document",&json!({"action":"edit","confirm":"true"})).is_err());
        assert!(validate_tool_arguments("readmd_pdf_document",&json!({"action":"unknown","confirm":true})).is_err());
        let schema=json!({"type":"object","properties":{"items":{"type":"array","items":{"type":"integer","minimum":1},"maxItems":2}},"required":["items"],"additionalProperties":false});
        assert!(matches_tool_schema(&json!({"items":[1,2]}),&schema,0));assert!(!matches_tool_schema(&json!({"items":[0]}),&schema,0));assert!(!matches_tool_schema(&json!({"items":[1,2,3]}),&schema,0));
    }
    #[test]
    fn every_advertised_tool_has_a_callable_object_contract() {
        for tool in tools().as_array().unwrap() {
            assert_eq!(tool["inputSchema"]["type"],"object","{} exposes a model descriptor instead of a request schema",tool["name"]);
        }
        for args in [
            json!({"content":"Body","options":{"meta":{"subtitle":null,"authors":null,"series":{"title":"Series","number":null}}}}),
            json!({"action":"patch","content":"Body","base_revision":"0".repeat(64),"patch":{"subtitle":null,"bibliography":["references.json"]}}),
        ] {assert!(validate_tool_arguments("readmd_publication_metadata",&args).is_ok(),"{args}");}
        for args in [json!({"content":false}),json!({"content":"Body","unexpected":true}),json!({"content":"Body","patch":{"keywords":[false]}}),json!({"content":"Body","options":{"meta":{"clear":["subtitle","subtitle"]}}})] {
            assert_eq!(validate_tool_arguments("readmd_publication_metadata",&args),Err("invalid_arguments"),"{args}");
        }
    }
    #[test]
    fn metadata_tool_dispatches_the_real_contract_and_keeps_inverse_and_error_ranges() {
        let root=tempfile::tempdir().unwrap();let assets=root.path().join("assets");std::fs::create_dir(&assets).unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&root.path().join("profile"),root.path(),&assets)).unwrap());
        let source="\u{feff}---\r\ntitle: Study\r\nauthors:\r\n  - id: ada\r\n    name: \"Ada, Example\" # keep this comment\r\nunknown: {value: preserved}\r\n---\r\nBody 😀 unchanged\r\n";
        let read=call_tool(&app,"readmd_publication_metadata",&json!({"action":"get","content":source,"source_name":"Study.md"}));
        assert_ne!(read["isError"],true,"{read}");let metadata=&read["structuredContent"];
        assert_eq!(metadata["metadata"]["authors"][0]["name"],"Ada, Example");
        let edited=call_tool(&app,"readmd_publication_metadata",&json!({"action":"patch","content":source,"base_revision":metadata["revision"],"patch":{"authors":[{"id":"ada","name":"Ada revised"}]}}));
        assert_ne!(edited["isError"],true,"{edited}");let body=&edited["structuredContent"];
        assert_eq!(body["content"],source.replace("\"Ada, Example\"","\"Ada revised\""));
        assert_eq!(body["transaction"]["edits"].as_array().unwrap().len(),1);
        let inverse=call_tool(&app,"readmd_content_transaction",&json!({"content":body["content"],"transaction":{"base_revision":body["revision"],"origin":"editor","edits":body["inverse"]}}));
        assert_ne!(inverse["isError"],true,"{inverse}");assert_eq!(inverse["structuredContent"]["result"]["content"],source);
        let stale=call_tool(&app,"readmd_publication_metadata",&json!({"action":"patch","content":body["content"],"base_revision":metadata["revision"],"patch":{"subtitle":"stale"}}));
        assert_eq!(stale["isError"],true);assert_eq!(stale["structuredContent"]["error_code"],"transaction_conflict");
        assert!(stale["structuredContent"]["path"].is_string());assert!(stale["structuredContent"]["from"].is_u64());
        let invalid=source.replace("name: \"Ada, Example\" # keep this comment","name: \"Ada, Example\" # keep this comment\r\n    orcid: \"0000-0000-0000-0000\"");
        let rejected=call_tool(&app,"readmd_publication_metadata",&json!({"content":invalid}));
        assert_eq!(rejected["isError"],true);assert_eq!(rejected["structuredContent"]["error_code"],"publication_metadata_orcid");
        assert_eq!(rejected["structuredContent"]["path"],"authors[0].orcid");assert!(rejected["structuredContent"]["to"].as_u64()>rejected["structuredContent"]["from"].as_u64());
    }
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn bounded_transport_drains_bad_messages_and_recovers() {
        let mut input = std::io::BufReader::with_capacity(3, std::io::Cursor::new(b"123456789\nok\n\xff\nlast"));
        assert_eq!(read_message(&mut input, 4).unwrap(), Some(Err(-32600)));
        assert_eq!(read_message(&mut input, 4).unwrap(), Some(Ok("ok".into())));
        assert_eq!(read_message(&mut input, 4).unwrap(), Some(Err(-32700)));
        assert_eq!(read_message(&mut input, 4).unwrap(), Some(Ok("last".into())));
        assert_eq!(read_message(&mut input, 4).unwrap(), None);
    }

    #[test]
    fn atomic_publication_and_cancelled_exports_preserve_destination() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("result.html");
        std::fs::write(&output, b"KEEP").unwrap();
        assert_eq!(write_output(&output, b"REPLACEMENT", false), Err("output_exists"));
        assert_eq!(std::fs::read(&output).unwrap(), b"KEEP");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        let (app, _, _, _, _) = harness("cancelled-export");
        let cancelled = AtomicBool::new(true);
        let result = call_tool_cancellable(&app, "readmd_export_document", &json!({
            "output_path": output, "output_format":"html", "markdown_content":"# Replaced", "overwrite":true, "confirm":true
        }), Some(&cancelled));
        assert_eq!(result["structuredContent"]["error_code"], "cancelled");
        assert_eq!(std::fs::read(&output).unwrap(), b"KEEP");
    }

    #[test]
    fn provider_metadata_does_not_echo_header_or_url_credentials() {
        let result = scrub(&json!({"credential_id":"cred:handle", "base_url":"https://name:private@example.invalid/v1?api_key=private#private", "headers":{"X-API-Key":"private", "Proxy-Authorization":"private", "X-Region":"test"}}));
        assert_eq!(result["credential_id"], "cred:handle");
        assert_eq!(result["base_url"], "https://example.invalid/v1");
        assert_eq!(result["headers"], json!({"X-Region":"test"}));
        assert!(!result.to_string().contains("private"));
    }

    struct Chan(mpsc::Sender<String>, Vec<u8>);
    impl Write for Chan {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.1.extend_from_slice(b);
            while let Some(i) = self.1.iter().position(|&c| c == b'\n') {
                let line: Vec<u8> = self.1.drain(..=i).collect();
                let _ = self.0.send(String::from_utf8_lossy(&line).trim().to_string());
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn harness(tag: &str) -> (Arc<App>, Out, mpsc::Receiver<String>, Arc<AtomicUsize>, Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>) {
        let dir = std::env::temp_dir().join(format!("readmd-mcp-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        let paths = crate::paths::AppPaths::with_dirs(&dir.join("data"), &dir, &dir.join("assets"));
        let app = Arc::new(App::bootstrap(paths).unwrap());
        let (tx, rx) = mpsc::channel();
        (app, Out::new(Box::new(Chan(tx, Vec::new()))), rx, Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(HashMap::new())))
    }

    fn roundtrip(tag: &str, lines: &[Value]) -> Vec<Value> {
        let (app, out, rx, busy, cancelled) = harness(tag);
        for l in lines {
            handle_line(&app, &out, &busy, &cancelled, &l.to_string());
        }
        let want = lines.iter().filter(|l| l.get("id").is_some()).count();
        (0..want).map(|_| serde_json::from_str(&rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap()).unwrap()).collect()
    }

    fn by_id(res: &[Value], id: i64) -> Value {
        res.iter().find(|r| r["id"] == json!(id)).cloned().unwrap_or(Value::Null)
    }

    #[test]
    fn handshake_lists_tools_and_answers_ping() {
        let r = roundtrip("hs", &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":4,"method":"nope"}),
        ]);
        assert_eq!(by_id(&r, 1)["result"]["protocolVersion"], "2024-11-05");
        let listed = by_id(&r, 2);
        let names: Vec<&str> = listed["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"readmd_convert_to_markdown") && names.contains(&"readmd_export_document"));
        assert_eq!(by_id(&r, 3)["result"], json!({}));
        assert_eq!(by_id(&r, 4)["error"]["code"], -32601);
    }

    #[test]
    fn tools_run_and_side_effects_need_confirmation() {
        let dir = std::env::temp_dir().join(format!("readmd-mcp-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("a.html");
        let r = roundtrip("tools", &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"readmd_fix_markdown","arguments":{"content":"# t\n\n**x"}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"readmd_export_document","arguments":{"markdown_content":"# t","output_path":out,"output_format":"html"}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"readmd_generate_toc","arguments":{"markdown_content":"# A\n## B\n"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"readmd_export_document","arguments":{"markdown_content":"x","output_path":"rel.pdf","output_format":"pdf","confirm":true}}}),
        ]);
        let fixed: Value = serde_json::from_str(by_id(&r, 1)["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(fixed["ok"], true);
        assert_eq!(by_id(&r, 2)["result"]["isError"], true);
        assert!(by_id(&r, 2)["result"]["content"][0]["text"].as_str().unwrap().contains("confirmation_required"));
        assert!(by_id(&r, 3)["result"]["content"][0]["text"].as_str().unwrap().contains("[B](#b)"));
        assert!(by_id(&r, 4)["result"]["content"][0]["text"].as_str().unwrap().contains("output_path_must_be_absolute"));
        assert!(!out.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_never_replaces_without_overwrite() {
        let dir = std::env::temp_dir().join(format!("readmd-mcp-ow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("doc.tex");
        std::fs::write(&out, "keep").unwrap();
        let call = |id: i64, ow: bool| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"readmd_export_document","arguments":{"markdown_content":"# Hi","output_path":out,"output_format":"tex","overwrite":ow,"confirm":true}}});
        let r = roundtrip("ow", &[call(1, false)]);
        assert!(by_id(&r, 1)["result"]["content"][0]["text"].as_str().unwrap().contains("output_exists"), "{r:?}");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "keep");
        let r = roundtrip("ow2", &[call(2, true)]);
        assert_eq!(by_id(&r, 2)["result"].get("isError"), None, "{r:?}");
        assert!(std::fs::read_to_string(&out).unwrap().contains("Hi"));
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".mcp-")).collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_errors_and_bad_params_are_reported() {
        let (app, out, rx, busy, cancelled) = harness("bad");
        handle_line(&app, &out, &busy, &cancelled, "{not json");
        let v: Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        assert_eq!(v["error"]["code"], -32700);
        handle_line(&app, &out, &busy, &cancelled, r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"x","arguments":[1]}}"#);
        let v: Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        assert_eq!(v["error"]["code"], -32602);
    }

    #[test]
    fn modern_discovery_is_self_contained_and_legacy_handshake_stays_legacy() {
        let meta = json!({ PROTOCOL_META: MODERN_PROTOCOL, "io.modelcontextprotocol/clientCapabilities": {} });
        let r = roundtrip("modern", &[
            json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":meta}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":meta}}),
            json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":MODERN_PROTOCOL}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"_meta":meta,"name":"readmd_fix_markdown","arguments":{"content":"# Heading"}}}),
        ]);
        let discovery = by_id(&r, 1);
        assert_eq!(discovery["result"]["resultType"], "complete");
        assert!(discovery["result"]["supportedVersions"].as_array().unwrap().contains(&json!(MODERN_PROTOCOL)));
        assert_eq!(discovery["result"]["cacheScope"], "private");
        assert_eq!(discovery["result"]["ttlMs"], 0);
        assert_eq!(discovery["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "readmd");
        assert_eq!(by_id(&r, 2)["result"]["resultType"], "complete");
        assert_eq!(by_id(&r, 3)["result"]["protocolVersion"], LEGACY_PROTOCOL);
        assert!(by_id(&r, 3)["result"].get("resultType").is_none());
        let tool = by_id(&r, 4);
        assert_eq!(tool["result"]["structuredContent"]["ok"], true);
        assert_eq!(tool["result"]["resultType"], "complete");
    }

    #[test]
    fn invalid_protocol_and_parameter_types_are_not_silently_accepted() {
        let r = roundtrip("validate", &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{PROTOCOL_META:"1900-01-01"}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{PROTOCOL_META:MODERN_PROTOCOL}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"readmd_fix_markdown","arguments":{"content":false}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"readmd_generate_toc","arguments":{"markdown_content":"# hi","depth_from":7}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/list","params":{"cursor":"made-up"}}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/list","params":[]}),
        ]);
        assert_eq!(by_id(&r, 1)["error"]["code"], -32022);
        assert_eq!(by_id(&r, 1)["error"]["data"]["requested"], "1900-01-01");
        assert_eq!(by_id(&r, 2)["error"]["code"], -32602);
        assert_eq!(by_id(&r, 3)["result"]["structuredContent"]["error_code"], "invalid_arguments");
        assert_eq!(by_id(&r, 4)["result"]["isError"], true);
        assert_eq!(by_id(&r, 5)["error"]["code"], -32602);
        assert_eq!(by_id(&r, 6)["error"]["code"], -32602);
    }
}

// Drain oversized lines without retaining them, so the next request can recover.
fn read_message(input: &mut impl BufRead, limit: usize) -> std::io::Result<Option<Result<String, i64>>> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let chunk = input.fill_buf()?;
        if chunk.is_empty() {
            if bytes.is_empty() && !oversized { return Ok(None); }
            break;
        }
        let end = chunk.iter().position(|b| *b == b'\n');
        let count = end.map_or(chunk.len(), |at| at + 1);
        let payload = end.unwrap_or(count);
        if !oversized && bytes.len().saturating_add(payload) <= limit {
            bytes.extend_from_slice(&chunk[..payload]);
        } else { oversized = true; bytes.clear(); }
        input.consume(count);
        if end.is_some() { break; }
    }
    if oversized { return Ok(Some(Err(-32600))); }
    Ok(Some(String::from_utf8(bytes).map_err(|_| -32700)))
}

#[cfg(test)]
mod p13_navigation_schema_tests {
    use super::*;
    fn args(navigation:Value)->Value {
        json!({"markdown_content":"# Synthetic\n\nBODY","output_path":"Synthetic.epub",
            "confirm":true,"options":{"epub":{"navigation":navigation}}})
    }
    #[test]
    fn p13_mcp_navigation_null_inherits_and_field_false_or_empty_list_stay_valid() {
        for navigation in [
            Value::Null, json!({}),
            json!({"landmarks":null,"pageList":null,"paginationSource":null,"labels":null}),
            json!({"landmarks":false,"pageList":false,"paginationSource":false}),
            json!({"landmarks":[]}),
            json!({"paginationSource":{"description":"Synthetic printed edition"}}),
            json!({"paginationSource":{"identifier":"0013","description":"Synthetic printed edition"}}),
            json!({"landmarks":[{"type":"bodymatter","target":"@bodymatter"}],
                "paginationSource":{"identifier":"0013"},"labels":{"pageList":"Pages"}}),
        ] {
            assert_eq!(validate_tool_arguments("readmd_export_epub",&args(navigation.clone())),Ok(()),"{navigation}");
            assert!(crate::publication_navigation::validate_options(&json!({"epub":{"navigation":navigation}})).is_ok());
        }
    }
    #[test]
    fn p13_mcp_navigation_top_level_false_and_other_invalid_shapes_are_rejected() {
        for navigation in [
            json!(false),json!(true),json!([]),json!("auto"),
            json!({"pageList":true}),json!({"landmarks":true}),
            json!({"landmarks":[false]}),json!({"paginationSource":{}}),
            json!({"paginationSource":{"identifier":false}}),
            json!({"labels":{"landmarks":false}}),
            json!({"pageBreakSource":"unsupported"}),
        ] {
            assert_eq!(validate_tool_arguments("readmd_export_epub",&args(navigation.clone())),Err("invalid_arguments"),"{navigation}");
            assert!(crate::publication_navigation::validate_options(&json!({"epub":{"navigation":navigation}})).is_err());
        }
    }
}
