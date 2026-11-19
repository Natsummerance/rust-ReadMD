//! Bind Office findings to a bounded source snapshot. The report never reads
//! neighboring files and never prevents a supported converter from running.
use super::{fidelity, ConvertTriple};
use crate::document_operations::{self, Observation, ObservedFile};
use serde_json::{json, Value};
use std::path::Path;

pub(crate) type SourceStamp = ObservedFile;

fn inspected(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|ext| {
            ["docx", "pptx", "xlsx"]
                .iter()
                .any(|s| ext.eq_ignore_ascii_case(s))
        })
}

pub(crate) fn observe(path: &Path) -> Option<SourceStamp> {
    if !inspected(path) {
        return None;
    }
    let absolute = std::path::absolute(path).ok()?;
    match document_operations::observe(&absolute, document_operations::MAX_CAPTURE_BYTES) {
        Ok(Observation::Regular(file)) => Some(file),
        _ => None,
    }
}

pub(crate) fn report_if_current(
    path: &Path,
    result: &ConvertTriple,
    before: Option<&SourceStamp>,
) -> Value {
    if !inspected(path) {
        return fidelity::report_bytes(path, result, &[]);
    }
    let verified = before.filter(|file| {
        let fp = file.fingerprint();
        let Ok(absolute) = std::path::absolute(path) else {
            return false;
        };
        document_operations::matches_publication(
            &absolute,
            fp.identity.as_str(),
            &fp.revision,
            Some(file.bytes().len() as u64),
        )
        .unwrap_or(false)
    });
    if let Some(file) = verified {
        let mut report = fidelity::report_bytes(path, result, file.bytes());
        report["source_revision"] = json!(file.fingerprint().revision);
        report
    } else {
        // Preserve extraction facts only. A replaced path or an over-budget
        // file cannot support claims about the structure of this conversion.
        fidelity::unavailable(path, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mdexport, paths::AppPaths, server::call_in_process, App};
    use std::{fs, sync::Arc};

    fn revised_code_package() -> Vec<u8> {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr><w:r><w:t>let </w:t></w:r><w:del><w:r><w:delText>OLD</w:delText></w:r></w:del><w:ins><w:r><w:t>answer</w:t></w:r></w:ins><w:moveFrom><w:r><w:t>WRONG</w:t></w:r></w:moveFrom><w:moveTo><w:hyperlink w:anchor="local"><w:r><w:t> = 42;</w:t></w:r></w:hyperlink></w:moveTo></w:p></w:body></w:document>"#;
        let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let parts = [
            ("[Content_Types].xml", types),
            ("_rels/.rels", rels),
            ("word/document.xml", xml),
        ]
        .map(|(path, data)| mdexport::ZipEntry {
            path: path.into(),
            data: data.as_bytes().to_vec(),
            compress: true,
        });
        mdexport::write_zip(&parts).unwrap()
    }

    #[test]
    fn preview_and_mcp_report_same_revision_and_keep_visible_word_code() {
        let root = tempfile::tempdir().unwrap();
        let app = Arc::new(
            App::bootstrap(AppPaths::with_dirs(
                &root.path().join("profile"),
                root.path(),
                &root.path().join("assets"),
            ))
            .unwrap(),
        );
        let input = root.path().join("report.docx");
        let output = input.with_extension("md");
        let bytes = revised_code_package();
        fs::write(&input, &bytes).unwrap();
        fs::write(&output, "owned existing text").unwrap();
        let url = format!(
            "/api/convert?preview=1&p={}",
            super::super::py_url_quote(input.to_str().unwrap(), "")
        );
        let (status, result) = call_in_process(&app, "GET", &url, None);
        assert_eq!(status, 200, "{result}");
        let md = result["content"].as_str().unwrap();
        assert!(
            md.contains("let answer = 42;") && md.contains("```"),
            "{md}"
        );
        assert!(!md.contains("OLD") && !md.contains("WRONG"));
        assert_eq!(result["saved"], false);
        let report = &result["fidelity_report"];
        assert_eq!(
            report["source_revision"],
            crate::document_intelligence::revision(&bytes)
        );
        assert_eq!(report["schema"], 1);
        assert!(
            report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|finding| finding["code"] == "review_markup"),
            "{report}"
        );
        let args = json!({"file_path":input,"include_fidelity_report":true});
        let tool = crate::mcp::call_tool(&app, "readmd_convert_to_markdown", &args);
        assert_eq!(&tool["structuredContent"]["fidelity_report"], report);
        assert!(tool["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("let answer = 42;"));
        assert_eq!(fs::read(input).unwrap(), bytes);
        assert_eq!(fs::read_to_string(output).unwrap(), "owned existing text");
    }

    #[test]
    fn replaced_input_cannot_inherit_findings_from_an_earlier_file() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("report.docx");
        let bytes = revised_code_package();
        fs::write(&input, &bytes).unwrap();
        let stamp = observe(&input).unwrap();
        fs::rename(&input, root.path().join("original.docx")).unwrap();
        fs::write(&input, b"replacement, not a package").unwrap();
        let result = super::super::convert_triple_from_source(
            input.to_str().unwrap(), false, None, Some(stamp.bytes()),
        );
        assert!(result.error.is_none() && result.text.contains("let answer = 42;"));
        // Even an identical replacement does not preserve the original identity.
        fs::write(&input, bytes).unwrap();
        let report = report_if_current(&input, &ConvertTriple::ok("plain", "docx"), Some(&stamp));
        assert_eq!(report["inspection"], "unavailable");
        assert!(report.get("source_revision").is_none());
    }
}
