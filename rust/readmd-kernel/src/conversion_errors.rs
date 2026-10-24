//! Stable native import failures shared by conversion, HTTP, jobs and MCP.
//! Only known, bounded payloads cross this boundary as structured diagnostics.
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_ERROR_BYTES: usize = 8192;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeImportError {
    pub code: &'static str,
    pub details: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteDetails {
    part: String,
    path: String,
    detail: String,
}

fn bounded_text(text: &str, max_bytes: usize) -> bool {
    !text.is_empty() && text.len() <= max_bytes && !text.chars().any(char::is_control)
}

pub(crate) fn decode(raw: &str) -> Option<NativeImportError> {
    use crate::api_codes as code;
    if raw.len() > MAX_ERROR_BYTES { return None; }
    let (name, payload) = match raw.split_once(':') {
        Some((name, payload)) => (name, Some(payload)),
        None => (raw, None),
    };
    let exact = match name {
        code::DOCX_NOTES_INVALID => code::DOCX_NOTES_INVALID,
        code::DOCX_NESTED_FOOTNOTE => code::DOCX_NESTED_FOOTNOTE,
        code::DOCX_NUMBERING_BUDGET => code::DOCX_NUMBERING_BUDGET,
        _ => "",
    };
    if !exact.is_empty() {
        return payload.is_none().then_some(NativeImportError { code: exact, details: None });
    }
    let notes = match name {
        code::DOCX_NOTE_SETTINGS_INVALID => Some(code::DOCX_NOTE_SETTINGS_INVALID),
        code::DOCX_NOTE_SETTINGS_UNSUPPORTED => Some(code::DOCX_NOTE_SETTINGS_UNSUPPORTED),
        code::AUTHOR_INLINE_TEXT_INVALID => Some(code::AUTHOR_INLINE_TEXT_INVALID),
        code::AUTHOR_INLINE_TEXT_UNSUPPORTED => Some(code::AUTHOR_INLINE_TEXT_UNSUPPORTED),
        code::AUTHOR_INLINE_TEXT_BUDGET => Some(code::AUTHOR_INLINE_TEXT_BUDGET),
        _ => None,
    };
    if let Some(code) = notes {
        // Deserializing a fixed struct rejects duplicate/unknown fields as well
        // as wrong types, missing fields and any trailing non-JSON text.
        let detail: NoteDetails = serde_json::from_str(payload?).ok()?;
        if !bounded_text(&detail.part, 512) || !bounded_text(&detail.path, 2048)
            || !bounded_text(&detail.detail, 2048) { return None; }
        return Some(NativeImportError { code, details: Some(json!({
            "part": detail.part, "path": detail.path, "detail": detail.detail,
        })) });
    }
    let number = match name {
        code::DOCX_NUMBERING_INVALID => code::DOCX_NUMBERING_INVALID,
        code::DOCX_NUMBERING_UNREPRESENTABLE => code::DOCX_NUMBERING_UNREPRESENTABLE,
        _ => return None,
    };
    let reason = payload?.strip_prefix(' ')?;
    if reason.is_empty() || reason.len() > 128
        || !reason.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') { return None; }
    Some(NativeImportError { code: number, details: Some(json!({ "reason": reason })) })
}

/// Shared wire body; technical detail is separate from the localized code.
pub(crate) fn body(raw: &str) -> Value {
    if let Some(native) = decode(raw) {
        let mut body = json!({ "ok": false, "error_code": native.code, "error": native.code });
        if let Some(details) = native.details { body["error_details"] = details; }
        body
    } else {
        json!({ "ok": false, "error_code": crate::api_codes::CONVERSION_FAILED, "error": raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_notes_have_one_consistent_structured_body() {
        for code in [crate::api_codes::DOCX_NOTE_SETTINGS_INVALID, crate::api_codes::DOCX_NOTE_SETTINGS_UNSUPPORTED] {
            let details = json!({"part":"word/notes/作者.xml","path":"footnote:7:[0]","detail":"mixed run font sizes require an inline size model"});
            let raw = format!("{code}:{details}");
            assert_eq!(decode(&raw), Some(NativeImportError{code,details:Some(details.clone())}));
            assert_eq!(body(&raw), json!({"ok":false,"error_code":code,"error":code,"error_details":details}));
            assert!(!body(&raw).to_string().contains("文件可能已损坏"));
        }
    }

    #[test]
    fn unrecognized_or_forged_details_cannot_claim_a_native_error() {
        let valid=r#"{"part":"word/footnotes.xml","path":"/","detail":"unsupported"}"#;
        let prefix="docx_note_settings_unsupported:";
        for payload in [
            "", "null", "[]", "true", "42", r#"{"part":4,"path":"/","detail":"unsupported"}"#,
            r#"{"part":"word/footnotes.xml","path":"/"}"#,
            r#"{"part":"word/footnotes.xml","path":"/","detail":"unsupported","extra":1}"#,
            r#"{"part":"word/footnotes.xml","path":"/","detail":"unsupported","part":"other"}"#,
            r#"{"part":"word/footnotes.xml","path":"/","detail":"bad\nline"}"#,
            r#"{"part":"","path":"/","detail":"unsupported"}"#,
        ] {
            let raw=format!("{prefix}{payload}");
            assert!(decode(&raw).is_none(),"{raw}");
            assert_eq!(body(&raw)["error_code"],crate::api_codes::CONVERSION_FAILED);
        }
        for raw in [format!("x{prefix}{valid}"),format!(" {prefix}{valid}"),format!("{prefix}{valid} suffix"),format!("{prefix}{valid}{{}}"),format!("unknown:{valid}")] {
            assert!(decode(&raw).is_none(),"{raw}");
        }
        for (field, limit) in [("part",512),("path",2048),("detail",2048)] {
            let mut value:Value=serde_json::from_str(valid).unwrap();value[field]=json!("x".repeat(limit+1));
            assert!(decode(&format!("{prefix}{value}")).is_none());
        }
        assert!(decode(&"docx_notes_invalid".repeat(MAX_ERROR_BYTES)).is_none());
    }

    #[test]
    fn known_numbering_reasons_and_exact_legacy_codes_are_not_substrings() {
        for code in [crate::api_codes::DOCX_NOTES_INVALID, crate::api_codes::DOCX_NESTED_FOOTNOTE, crate::api_codes::DOCX_NUMBERING_BUDGET] {
            assert_eq!(decode(code), Some(NativeImportError{code,details:None}));
            assert!(decode(&format!("{code}: extra")).is_none());
            assert!(decode(&format!("prefix {code}")).is_none());
        }
        for code in [crate::api_codes::DOCX_NUMBERING_INVALID, crate::api_codes::DOCX_NUMBERING_UNREPRESENTABLE] {
            let raw=format!("{code}: duplicate_property");
            assert_eq!(body(&raw)["error_details"],json!({"reason":"duplicate_property"}));
            for bad in ["",":duplicate_property",": reason with spaces",": <xml>",": reason/path",": Uppercase"] {
                assert!(decode(&format!("{code}{bad}")).is_none());
            }
        }
        let generic="legacy_office_parse_failed: unreadable";
        assert_eq!(body(generic),json!({"ok":false,"error_code":"conversion_failed","error":generic}));
    }

    #[test]
    fn actual_native_error_survives_single_mcp_and_batch_conversion_without_sidecars() {
        use std::sync::Arc;
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("documents");
        std::fs::create_dir(&workspace).unwrap();
        let app = Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(
            &temporary.path().join("profile"), &workspace, &temporary.path().join("assets"),
        )).unwrap());
        let parsed = crate::md_ast::parse("Body[^n].\n\n[^n]: Authored note.\n");
        let mut package = crate::docx_writer::build(&parsed, &json!({"footnotes":{"size":10,"start":2}}), "owned.md", &|_|None);
        assert!(!package.parts.is_empty(),"{:?}",package.warns);
        let settings = &mut package.parts.iter_mut().find(|(name,_)|name=="word/settings.xml").unwrap().1;
        let native = String::from_utf8(settings.clone()).unwrap();
        assert!(native.contains("<w:footnotePr>"));
        *settings = native.replacen("<w:footnotePr>","<w:footnotePr><w:unsupportedNoteProperty/>",1).into_bytes();
        let entries = package.parts.into_iter().map(|(path,data)|crate::mdexport::ZipEntry{path,data,compress:true}).collect::<Vec<_>>();
        let bytes = crate::mdexport::write_zip(&entries).unwrap();
        let source = workspace.join("unsupported.docx");
        std::fs::write(&source,&bytes).unwrap();
        let old_output = workspace.join("unsupported.md");
        std::fs::write(&old_output,b"KEEP OLD OUTPUT").unwrap();
        let before = std::fs::read_dir(&workspace).unwrap().map(|row|row.unwrap().file_name()).collect::<std::collections::BTreeSet<_>>();
        let triple = crate::convert::convert_triple(source.to_str().unwrap(),true);
        assert!(triple.text.is_empty());
        let error = decode(triple.error.as_deref().unwrap()).unwrap();
        assert_eq!(error.code,crate::api_codes::DOCX_NOTE_SETTINGS_UNSUPPORTED);
        let details = error.details.unwrap();
        assert_eq!(details["part"],"word/settings.xml");
        assert!(!triple.error.unwrap().contains("文件可能已损坏"));
        let mcp = crate::mcp::call_tool(&app,"readmd_convert_to_markdown",&json!({"file_path":source}));
        assert_eq!(mcp["isError"],true);
        assert_eq!(mcp["structuredContent"],json!({"ok":false,"error_code":error.code,"error":error.code,"error_details":details}));
        assert_eq!(serde_json::from_str::<Value>(mcp["content"][0]["text"].as_str().unwrap()).unwrap(),mcp["structuredContent"]);
        let encoded = source.to_string_lossy().bytes().map(|byte|if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte){(byte as char).to_string()}else{format!("%{byte:02X}")}).collect::<String>();
        let (status,http) = crate::server::call_in_process(&app,"GET",&format!("/api/convert?p={encoded}&preview=1"),None);
        assert_eq!(status,422,"{http}");
        for key in ["error_code","error","error_details"] { assert_eq!(http[key],mcp["structuredContent"][key]); }
        let (status,job) = crate::server::call_in_process(&app,"POST","/api/convert/batch",Some(&json!({"confirm":true,"paths":[source],"overwrite":false})));
        assert_eq!(status,200,"{job}");
        let id = job["job"].as_str().unwrap();
        let began = std::time::Instant::now();
        loop {
            let (status,progress) = crate::server::call_in_process(&app,"GET",&format!("/api/convert/progress?job={id}"),None);
            assert_eq!(status,200,"{progress}");
            if progress["finished"]==true {
                assert_eq!(progress["done"],1);
                assert_eq!(progress["items"][0]["status"],"error");
                assert!(progress["items"][0].get("out").is_none());
                for key in ["error_code","error","error_details"] { assert_eq!(progress["items"][0][key],http[key]); }
                break;
            }
            assert!(began.elapsed()<std::time::Duration::from_secs(10),"batch conversion never completed");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(std::fs::read(&source).unwrap(),bytes);
        assert_eq!(std::fs::read(&old_output).unwrap(),b"KEEP OLD OUTPUT");
        assert_eq!(std::fs::read_dir(&workspace).unwrap().map(|row|row.unwrap().file_name()).collect::<std::collections::BTreeSet<_>>(),before);
    }
}
