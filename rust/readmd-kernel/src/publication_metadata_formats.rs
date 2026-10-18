//! Format projections of validated publication metadata. No source mutation.
use crate::md_ast::Document;
use crate::publication_metadata::{self, PublicationMetadata};
use serde_json::{json, Value};

pub const NS: &str = "urn:readmd:publication:v1:";
pub fn pdf_metadata(bytes: &[u8], metadata: &PublicationMetadata) -> Result<Vec<u8>, String> {
    crate::publication_pdf_metadata::apply(bytes, metadata)
}
pub fn warnings(metadata: &PublicationMetadata) -> Vec<String> {
    metadata
        .warning_items
        .iter()
        .filter_map(|item| item["code"].as_str().map(str::to_string))
        .collect()
}
/// Recover a field diagnostic only when an export failed metadata validation.
/// Writer/engine failures retain their original code and are never relabelled.
pub fn error_body(source: &str, options: &Value, name: &str, format: &str, code: &str) -> Option<Value> {
    if code.starts_with("publication_navigation_") {
        return crate::publication_navigation::preflight_export(format,source,options).err().filter(|e|e.code==code).map(|e|e.response_body());
    }
    if !code.starts_with("publication_metadata_") { return None; }
    let diagnostic = match format_options(options, format) {
        Ok(effective) => publication_metadata::from_source(source, &effective, name).err(),
        Err(error) => publication_metadata::from_source(source, options, name).err().or_else(|| {
            Some(publication_metadata::MetadataError {code: error, path: format.into(), from: 0, to: 0})
        }),
    }?;
    (diagnostic.code == code).then(|| diagnostic.response_body())
}
fn xml(s: &str) -> String {
    crate::docx_writer::xml_escape(s)
}
fn element(name: &str, value: &str) -> String {
    format!("<{name}>{}</{name}>", xml(value))
}
fn optional(name: &str, value: Option<&str>) -> String {
    value.map(|s| element(name, s)).unwrap_or_default()
}

/// The W3C/ISO calendar forms used by XMP Date. Unknown time zones remain
/// unknown (allowed by XMP); precision and the original lexical value stay intact.
fn standard_date(value: &str) -> bool {
    fn digits(bytes: &[u8]) -> Option<u32> {
        if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(bytes.iter().fold(0, |n, b| n * 10 + u32::from(*b - b'0')))
    }
    let b = value.as_bytes();
    if !value.is_ascii() || b.len() < 4 || digits(&b[..4]).is_none() {
        return false;
    }
    if b.len() == 4 {
        return true;
    }
    if b.len() < 7 || b[4] != b'-' {
        return false;
    }
    let Some(month) = digits(&b[5..7]).filter(|n| (1..=12).contains(n)) else {
        return false;
    };
    if b.len() == 7 {
        return true;
    }
    if b.len() < 10 || b[7] != b'-' {
        return false;
    }
    let Some(day) = digits(&b[8..10]) else {
        return false;
    };
    let year = digits(&b[..4]).unwrap() as i32;
    if time::Date::from_calendar_date(year, time::Month::try_from(month as u8).unwrap(), day as u8)
        .is_err()
    {
        return false;
    }
    if b.len() == 10 {
        return true;
    }
    if b.len() < 16
        || b[10] != b'T'
        || b[13] != b':'
        || digits(&b[11..13]).is_none_or(|n| n > 23)
        || digits(&b[14..16]).is_none_or(|n| n > 59)
    {
        return false;
    }
    let mut end = 16;
    if b.get(end) == Some(&b':') {
        if b.len() < end + 3 || digits(&b[end + 1..end + 3]).is_none_or(|n| n > 59) {
            return false;
        }
        end += 3;
        if b.get(end) == Some(&b'.') {
            end += 1;
            let start = end;
            while b.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end == start {
                return false;
            }
        }
    }
    let zone = &b[end..];
    zone.is_empty()
        || zone == b"Z"
        || (zone.len() == 6
            && matches!(zone[0], b'+' | b'-')
            && zone[3] == b':'
            && digits(&zone[1..3]).is_some_and(|n| n <= 23)
            && digits(&zone[4..6]).is_some_and(|n| n <= 59))
}

fn format_warning_items(metadata: &PublicationMetadata, format: &str) -> Vec<Value> {
    let mut warnings = Vec::new();
    if matches!(format, "pdf" | "epub")
        && metadata
            .date
            .as_deref()
            .is_some_and(|date| !standard_date(date))
    {
        warnings.push(json!({"code":"publication_metadata_date_custom_only", "path":"date", "format":format, "preserved":"custom-and-front-matter"}));
    }
    if format == "epub"
        && metadata
            .series
            .as_ref()
            .and_then(|series| series.number.as_deref())
            .is_some_and(|number| !group_position(number))
    {
        warnings.push(json!({"code":"publication_metadata_series_position_custom_only", "path":"series.number", "format":format, "preserved":"custom-and-front-matter"}));
    }
    warnings
}

/// The legacy EPUB-specific fields are a final, format-local layer.
pub fn format_options(options: &Value, format: &str) -> Result<Value, String> {
    if format != "epub" {
        return Ok(options.clone());
    }
    let mut layer = serde_json::Map::new();
    if let Some(epub) = options.get("epub").and_then(Value::as_object) {
        for (old, new) in [
            ("title", "title"),
            ("author", "authors"),
            ("publisher", "publisher"),
            ("isbn", "isbn"),
            ("language", "language"),
            ("description", "abstracts"),
            ("rights", "rights"),
            ("date", "date"),
            ("keywords", "keywords"),
        ] {
            if let Some(value) = epub
                .get(old)
                .filter(|v| !v.is_null() && v.as_str().is_none_or(|s| !s.trim().is_empty()))
            {
                // Invalid language keeps the established P02 fallback rule.
                if new == "language"
                    && value
                        .as_str()
                        .and_then(crate::md_ast::publication_language_tag)
                        .is_none()
                {
                    continue;
                }
                layer.insert(new.into(), value.clone());
            }
        }
    }
    publication_metadata::merge_options(options, &json!({"meta":layer})).map_err(|e| e.code)
}

pub fn prepare(doc:&Document,options:&Value,name:&str,format:&str)->Result<(Document,Value,PublicationMetadata),String>{
    let (doc,style,metadata,_)=prepare_with_front_count(doc,options,name,format)?;Ok((doc,style,metadata))
}
pub(crate) fn prepare_with_front_count(doc:&Document,options:&Value,name:&str,format:&str)->Result<(Document,Value,PublicationMetadata,usize),String>{
    let options = format_options(options, format)?;
    let mut metadata = publication_metadata::resolve(doc.front_matter.as_deref(), &options, name)
        .map_err(|e| e.code)?;
    let mut format_warnings = format_warning_items(&metadata, format);
    publication_metadata::annotate_warning_ranges(
        &mut format_warnings,
        doc.front_matter.as_deref(),
        &options,
    )
    .map_err(|error| error.code)?;
    metadata.warning_items.extend(format_warnings);
    let mut style = crate::export_styles::sanitize(&options);
    if metadata.declared {
        style["meta"]["title"] = json!(metadata.title);
        style["meta"]["author"] = json!(metadata.author_text());
        style["meta"]["subject"] = json!(metadata.subject.as_deref().unwrap_or(""));
        style["meta"]["language"] = json!(metadata.language.as_deref().unwrap_or("en"));
        style["_publication"] =
            serde_json::to_value(&metadata).map_err(|_| "publication_metadata_type")?;
    }
    let mut prepared = doc.clone();
    let mut front = publication_metadata::front_matter_blocks(&metadata, &options);
    let front_count=front.len();
    if !front.is_empty() {
        // The opt-in typed front matter owns the title page; never duplicate it.
        style["cover"]["enabled"] = json!(false);
        front.append(&mut prepared.blocks);
        prepared.blocks = front;
    }
    Ok((prepared, style, metadata,front_count))
}

pub fn custom_properties(metadata: &PublicationMetadata) -> String {
    let value = serde_json::to_string(metadata).expect("metadata serialization");
    format!(concat!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\" ",
        "xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">",
        "<property fmtid=\"{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}\" pid=\"2\" name=\"ReadMD.Publication.v1\">",
        "<vt:lpwstr>{}</vt:lpwstr></property></Properties>"), xml(&value))
}

pub fn core_properties(metadata: &PublicationMetadata, stamp: &str) -> String {
    let description = metadata
        .abstracts
        .iter()
        .map(|a| a.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut body = element("dc:title", &metadata.title);
    body.push_str(&element("dc:creator", &metadata.author_text()));
    body.push_str(&optional("dc:subject", metadata.subject.as_deref()));
    body.push_str(&optional("dc:language", metadata.language.as_deref()));
    if !description.is_empty() {
        body.push_str(&element("dc:description", &description));
    }
    if !metadata.keywords.is_empty() {
        body.push_str(&element("cp:keywords", &metadata.keywords.join("; ")));
    }
    if let Some(identifier) = metadata.primary_identifier() {
        body.push_str(&element("dc:identifier", &identifier.value));
    }
    format!(concat!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" ",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" ",
        "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">{}",
        "<dcterms:created xsi:type=\"dcterms:W3CDTF\">{}</dcterms:created>",
        "<dcterms:modified xsi:type=\"dcterms:W3CDTF\">{}</dcterms:modified></cp:coreProperties>"), body, xml(stamp), xml(stamp))
}

/// Native XMP, shared by the Rust writer and bounded engine metadata updates.
pub fn xmp(metadata: &PublicationMetadata) -> String {
    fn alt(name: &str, value: &str) -> String {
        format!(
            "<{name}><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></{name}>",
            xml(value)
        )
    }
    fn array(name: &str, kind: &str, values: impl IntoIterator<Item = String>) -> String {
        let values = values
            .into_iter()
            .map(|s| element("rdf:li", &s))
            .collect::<String>();
        if values.is_empty() {
            String::new()
        } else {
            format!("<{name}><rdf:{kind}>{values}</rdf:{kind}></{name}>")
        }
    }
    let mut body = alt("dc:title", &metadata.title);
    body.push_str(&array(
        "dc:creator",
        "Seq",
        metadata.authors.iter().map(|p| p.name.clone()),
    ));
    body.push_str(&array(
        "dc:contributor",
        "Bag",
        metadata.translators.iter().map(|p| p.name.clone()),
    ));
    body.push_str(&array("dc:subject", "Bag", metadata.keywords.clone()));
    body.push_str(&array(
        "xmp:Identifier",
        "Bag",
        metadata.identifiers.iter().map(|p| p.value.clone()),
    ));
    if let Some(identifier) = metadata.primary_identifier() {
        body.push_str(&element("dc:identifier", &identifier.value));
    }
    if let Some(rights) = &metadata.rights {
        body.push_str(&alt("dc:rights", rights));
    }
    if let Some(language) = &metadata.language {
        body.push_str(&array("dc:language", "Bag", [language.clone()]));
    }
    if let Some(publisher) = &metadata.publisher {
        body.push_str(&array("dc:publisher", "Bag", [publisher.clone()]));
    }
    if let Some(date) = metadata.date.as_ref().filter(|date| standard_date(date)) {
        body.push_str(&array("dc:date", "Seq", [date.clone()]));
    }
    if !metadata.abstracts.is_empty() {
        // LangAlt has one item per language. Keep ordered abstracts in the
        // full model and join repeated language entries without losing text.
        let mut languages = Vec::<(&str, Vec<&str>)>::new();
        for value in &metadata.abstracts {
            let language = value.language.as_deref().unwrap_or("und");
            if let Some((_, texts)) = languages
                .iter_mut()
                .find(|(key, _)| key.eq_ignore_ascii_case(language))
            {
                texts.push(&value.text);
            } else {
                languages.push((language, vec![&value.text]));
            }
        }
        let default = languages
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("x-default"))
            .unwrap_or(&languages[0])
            .1
            .join("\n\n");
        let mut values = format!("<rdf:li xml:lang=\"x-default\">{}</rdf:li>", xml(&default));
        for (language, texts) in languages {
            if !language.eq_ignore_ascii_case("x-default") {
                values.push_str(&format!(
                    "<rdf:li xml:lang=\"{}\">{}</rdf:li>",
                    xml(language),
                    xml(&texts.join("\n\n"))
                ));
            }
        }
        body.push_str(&format!(
            "<dc:description><rdf:Alt>{values}</rdf:Alt></dc:description>"
        ));
    }
    body.push_str(&element(
        "readmd:metadata",
        &serde_json::to_string(metadata).expect("metadata serialization"),
    ));
    format!(concat!("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>",
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
        "<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ",
        "xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:readmd=\"urn:readmd:publication:v1:\">",
        "{}</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"), body)
}

fn group_position(value: &str) -> bool {
    value.split('.').all(|part| {
        !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) && part.parse::<u32>().is_ok()
    })
}

pub fn epub_metadata(metadata: &PublicationMetadata, uid: &str, language: &str) -> String {
    let mut body = format!("<dc:title id=\"publication-title\">{}</dc:title><meta refines=\"#publication-title\" property=\"title-type\">main</meta>", xml(&metadata.title));
    if let Some(subtitle) = &metadata.subtitle {
        body.push_str(&format!("<dc:title id=\"publication-subtitle\">{}</dc:title><meta refines=\"#publication-subtitle\" property=\"title-type\">subtitle</meta>", xml(subtitle)));
    }
    if let Some(edition) = &metadata.edition {
        body.push_str(&format!("<dc:title id=\"publication-edition\">{}</dc:title><meta refines=\"#publication-edition\" property=\"title-type\">edition</meta>", xml(edition)));
    }
    body.push_str(&format!(
        "<dc:identifier id=\"BookId\">{}</dc:identifier>",
        xml(metadata
            .primary_identifier()
            .map(|identifier| identifier.value.as_str())
            .unwrap_or(uid))
    ));
    body.push_str(&element("dc:language", language));
    for (index, person) in metadata.authors.iter().enumerate() {
        let id = format!("publication-author-{index}");
        body.push_str(&format!("<dc:creator id=\"{id}\">{}</dc:creator><meta refines=\"#{id}\" property=\"role\" scheme=\"marc:relators\">aut</meta>", xml(&person.name)));
        if let Some(file_as) = &person.file_as {
            body.push_str(&format!(
                "<meta refines=\"#{id}\" property=\"file-as\">{}</meta>",
                xml(file_as)
            ));
        }
    }
    for (index, person) in metadata.translators.iter().enumerate() {
        let id = format!("publication-translator-{index}");
        body.push_str(&format!("<dc:contributor id=\"{id}\">{}</dc:contributor><meta refines=\"#{id}\" property=\"role\" scheme=\"marc:relators\">trl</meta>", xml(&person.name)));
    }
    let primary = metadata.primary_identifier();
    for (index, identifier) in metadata.identifiers.iter().enumerate() {
        let is_primary = primary.is_some_and(|primary| std::ptr::eq(primary, identifier));
        let id = if is_primary {
            "BookId".to_string()
        } else {
            format!("publication-id-{index}")
        };
        if !is_primary {
            body.push_str(&format!(
                "<dc:identifier id=\"{id}\">{}</dc:identifier>",
                xml(&identifier.value)
            ));
        }
        if identifier.scheme.eq_ignore_ascii_case("isbn") {
            body.push_str(&format!("<meta refines=\"#{id}\" property=\"identifier-type\" scheme=\"onix:codelist5\">{}</meta>", if identifier.value.bytes().filter(u8::is_ascii_digit).count() == 13 { "15" } else { "02" }));
        }
    }
    body.push_str(&optional("dc:publisher", metadata.publisher.as_deref()));
    body.push_str(&optional(
        "dc:date",
        metadata.date.as_deref().filter(|date| standard_date(date)),
    ));
    body.push_str(&optional("dc:rights", metadata.rights.as_deref()));
    for value in &metadata.abstracts {
        body.push_str(&format!(
            "<dc:description{}>{}</dc:description>",
            value
                .language
                .as_deref()
                .map(|s| format!(" xml:lang=\"{}\"", xml(s)))
                .unwrap_or_default(),
            xml(&value.text)
        ));
    }
    for keyword in &metadata.keywords {
        body.push_str(&element("dc:subject", keyword));
    }
    if let Some(series) = &metadata.series {
        body.push_str(&format!("<meta property=\"belongs-to-collection\" id=\"publication-series\">{}</meta><meta refines=\"#publication-series\" property=\"collection-type\">series</meta>", xml(&series.title)));
        if let Some(number) = &series.number {
            if group_position(number) {
                body.push_str(&format!(
                    "<meta refines=\"#publication-series\" property=\"group-position\">{}</meta>",
                    xml(number)
                ));
            }
        }
    }
    body.push_str(&format!(
        "<meta property=\"readmd:metadata\">{}</meta>",
        xml(&serde_json::to_string(metadata).expect("metadata serialization"))
    ));
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn machine_dates_keep_exact_iso_precision_without_inventing_components() {
        for date in [
            "2026",
            "2026-10",
            "2024-02-29",
            "2026-10-13T09:30",
            "2026-10-13T09:30Z",
            "2026-10-13T09:30:12.123456789123+08:00",
        ] {
            let metadata = PublicationMetadata {
                date: Some(date.into()),
                ..Default::default()
            };
            assert!(standard_date(date), "{date}");
            assert!(xmp(&metadata).contains(&format!(
                "<dc:date><rdf:Seq><rdf:li>{date}</rdf:li></rdf:Seq></dc:date>"
            )));
            assert!(epub_metadata(&metadata, "urn:test", "en")
                .contains(&format!("<dc:date>{date}</dc:date>")));
            assert!(format_warning_items(&metadata, "pdf").is_empty());
            assert!(format_warning_items(&metadata, "epub").is_empty());
            assert_eq!(metadata.date.as_deref(), Some(date));
        }
        for date in [
            "Fall 2026",
            "2026/2027",
            "2023-02-29",
            "2026-13",
            "2026-00",
            "2026-04-31",
            "2026-10-13T24:00Z",
            "2026-10-13T12:60Z",
            "2026-10-13T12:00:60Z",
            "2026-10-13T12:00:00.Z",
            "2026-10-13T12:00+24:00",
        ] {
            assert!(!standard_date(date), "{date}");
        }
    }
    #[test]
    fn natural_date_and_nonlexical_series_warn_without_losing_custom_or_front_content() {
        let doc = crate::md_ast::parse("---\ntitle: Book\ndate: 'Fall 2026'\nseries: {title: Collected, number: '1e2'}\n---\nBody");
        let options = json!({"frontMatter":{"mode":"book"}});
        let (prepared, _, metadata) = prepare(&doc, &options, "Book", "epub").unwrap();
        let output = epub_metadata(&metadata, "urn:test", "en");
        assert!(!output.contains("<dc:date>"));
        assert!(!output.contains("property=\"group-position\""));
        assert!(!xmp(&metadata).contains("<dc:date>"));
        assert!(output.contains(&xml(&serde_json::to_string(&metadata).unwrap())));
        assert!(format!("{:?}", prepared.blocks).contains("Fall 2026"));
        assert!(format!("{:?}", prepared.blocks).contains("1e2"));
        assert!(warnings(&metadata).contains(&"publication_metadata_date_custom_only".to_string()));
        assert!(warnings(&metadata)
            .contains(&"publication_metadata_series_position_custom_only".to_string()));
        for (code, lexical) in [
            ("publication_metadata_date_custom_only", "'Fall 2026'"),
            ("publication_metadata_series_position_custom_only", "'1e2'"),
        ] {
            let warning = metadata
                .warning_items
                .iter()
                .find(|warning| warning["code"] == code)
                .unwrap();
            assert_eq!(warning["coordinateSpace"], "frontMatter");
            let source = doc.front_matter.as_deref().unwrap();
            assert_eq!(
                &source[warning["from"].as_u64().unwrap() as usize
                    ..warning["to"].as_u64().unwrap() as usize],
                lexical
            );
        }
        assert_eq!(metadata.date.as_deref(), Some("Fall 2026"));
        assert_eq!(
            metadata.series.as_ref().unwrap().number.as_deref(),
            Some("1e2")
        );
        let (_, _, pdf_metadata) = prepare(&doc, &options, "Book", "pdf").unwrap();
        assert!(
            warnings(&pdf_metadata).contains(&"publication_metadata_date_custom_only".to_string())
        );
        assert!(!warnings(&pdf_metadata)
            .contains(&"publication_metadata_series_position_custom_only".to_string()));
        let (_, _, docx_metadata) = prepare(&doc, &options, "Book", "docx").unwrap();
        assert!(!warnings(&docx_metadata)
            .contains(&"publication_metadata_date_custom_only".to_string()));
    }
    #[test]
    fn xmp_language_alternatives_are_unique_and_keep_all_abstracts() {
        let abstracts = vec![
            publication_metadata::Abstract {
                text: "English one".into(),
                language: Some("en".into()),
            },
            publication_metadata::Abstract {
                text: "English two".into(),
                language: Some("EN".into()),
            },
            publication_metadata::Abstract {
                text: "Unknown one".into(),
                language: None,
            },
            publication_metadata::Abstract {
                text: "Unknown two".into(),
                language: None,
            },
            publication_metadata::Abstract {
                text: "中文 & <文本>".into(),
                language: Some("zh-CN".into()),
            },
        ];
        let metadata = PublicationMetadata {
            abstracts: abstracts.clone(),
            ..Default::default()
        };
        let output = xmp(&metadata);
        let description = output
            .split_once("<dc:description>")
            .unwrap()
            .1
            .split_once("</dc:description>")
            .unwrap()
            .0;
        assert_eq!(description.matches("xml:lang=\"x-default\"").count(), 1);
        assert_eq!(description.matches("xml:lang=\"en\"").count(), 1);
        assert!(!description.contains("xml:lang=\"EN\""));
        assert_eq!(description.matches("xml:lang=\"und\"").count(), 1);
        assert!(description.contains("English one\n\nEnglish two"));
        assert!(description.contains("Unknown one\n\nUnknown two"));
        assert!(description.contains("中文 &amp; &lt;文本&gt;"));
        assert!(output.contains(&xml(&serde_json::to_string(&metadata).unwrap())));
        assert_eq!(metadata.abstracts, abstracts);
    }
    #[test]
    fn epub_primary_identifier_is_published_once_and_refined_as_book_id() {
        let metadata = PublicationMetadata {
            identifiers: vec![
                publication_metadata::Identifier {
                    scheme: "doi".into(),
                    value: "10.1234/example".into(),
                    primary: false,
                },
                publication_metadata::Identifier {
                    scheme: "isbn".into(),
                    value: "ISBN 978-0-306-40615-7".into(),
                    primary: true,
                },
            ],
            ..Default::default()
        };
        let output = epub_metadata(&metadata, "urn:fallback", "en");
        assert!(
            output.contains("<dc:identifier id=\"BookId\">ISBN 978-0-306-40615-7</dc:identifier>")
        );
        assert!(output
            .contains("<dc:identifier id=\"publication-id-0\">10.1234/example</dc:identifier>"));
        assert_eq!(output.matches("<dc:identifier ").count(), 2);
        assert!(!output.contains("id=\"publication-id-1\""));
        assert!(output.contains(
            "refines=\"#BookId\" property=\"identifier-type\" scheme=\"onix:codelist5\">15</meta>"
        ));
        assert!(!output.contains("urn:fallback"));
    }
    #[test]
    fn epub_group_position_uses_unsigned_integer_components_and_preserves_custom_values() {
        for number in ["0", "02", "2.2.1", "98.4", "4294967295"] {
            assert!(group_position(number), "valid {number}");
        }
        for number in [
            "",
            "+1",
            "-0",
            "1e2",
            "1.",
            ".1",
            "1..2",
            "1 2",
            "NaN",
            "4294967296",
        ] {
            assert!(!group_position(number), "invalid {number}");
        }
        for (number, standard) in [("2.2.1", true), ("1e2", false)] {
            let metadata = PublicationMetadata {
                series: Some(publication_metadata::Series {
                    title: "Series".into(),
                    number: Some(number.into()),
                }),
                ..Default::default()
            };
            let output = epub_metadata(&metadata, "urn:test", "en");
            assert_eq!(output.contains("property=\"group-position\""), standard);
            assert!(output.contains(&xml(&serde_json::to_string(&metadata).unwrap())));
            assert_eq!(metadata.series.unwrap().number.as_deref(), Some(number));
        }
    }
    #[test]
    fn metadata_projections_keep_order_roles_identifiers_and_escape_literal_content() {
        let source = "---\ntitle: 'Book & <title>'\nauthors: ['Lovelace, Ada', 'Grace Hopper']\ntranslator: 'Reader & Writer'\nkeywords: ['one, intact', two]\nrights: 'All <reserved>'\nisbn: '9780306406157'\n---\n# Body";
        let metadata = publication_metadata::from_source(source, &json!({}), "Book").unwrap();
        let epub = epub_metadata(&metadata, "urn:test:book", "en-US");
        assert_eq!(epub.matches("<dc:creator ").count(), 2);
        assert!(epub.find("Lovelace, Ada").unwrap() < epub.find("Grace Hopper").unwrap());
        assert!(epub.contains(">trl</meta>"));
        assert!(epub.contains("Book &amp; &lt;title&gt;"));
        assert!(epub.contains("9780306406157"));
        let xmp = xmp(&metadata);
        assert!(xmp.contains("<dc:creator><rdf:Seq>"));
        assert!(xmp.contains("<xmp:Identifier><rdf:Bag>"));
        assert!(xmp.contains("<dc:identifier>9780306406157</dc:identifier>"));
        assert!(!xmp.contains("<dc:identifier><rdf:Bag>"));
        let custom = custom_properties(&metadata);
        assert!(custom.contains("ReadMD.Publication.v1"));
        assert!(custom.contains("one, intact"));
        for value in [
            epub,
            xmp,
            custom,
            core_properties(&metadata, "2026-10-13T00:00:00Z"),
        ] {
            let wrapped =
                format!("<test xmlns:dc=\"http://purl.org/dc/elements/1.1/\">{value}</test>");
            let mut reader = quick_xml::Reader::from_str(&wrapped);
            loop {
                match reader.read_event() {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => (),
                    Err(error) => panic!("invalid projection XML: {error}"),
                }
            }
        }
    }
}
