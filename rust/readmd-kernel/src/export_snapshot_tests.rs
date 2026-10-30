use super::*;
use base64::Engine;
use tempfile::TempDir;

#[test]
fn localized_unused_callout_titles_do_not_require_a_font_pack_but_visible_titles_do(){
    let options=json!({"calloutTitles":{"note":"注意","warning":"警告"},"pdf":{"engine":"rust"}});
    for format in ["pdf","docx"]{
        assert!(!needs_fonts(format,"# English\n\nA Latin document.",&options));
        assert!(!needs_fonts(format,"```md\n> [!NOTE]\n> literal example\n```",&options));
        assert!(!needs_fonts(format,"> [!NOTE] Authored title\n> Body.",&options));
        assert!(needs_fonts(format,"> [!NOTE]\n> Body.",&options));
        assert!(needs_fonts(format,"# 中文正文",&options));
        assert!(needs_fonts(format,"English body.",&json!({"header":{"text":"页眉"}})));
    }
}

struct Fixture { directory: TempDir, data: PathBuf, assets: PathBuf, source: PathBuf, review: review::Service, service: Service, document: Value }
impl Fixture {
    fn new(content: &str) -> Self {
        let directory = TempDir::new().unwrap();
        let data = directory.path().join("data"); let assets = directory.path().join("assets");
        let source = directory.path().join("document/review.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(assets.join("mathjax")).unwrap();
        fs::write(assets.join("marked.min.js"), "window.marked={parse: x=>x};").unwrap();
        fs::write(assets.join("mathjax/tex-svg.js"), "window.MathJax={};").unwrap();
        fs::write(&source, content).unwrap();
        let review = review::Service::new(&data);
        let document = review.handle(&json!({"action":"open","path":source,"content":content,"disk_content":content,
            "disk_revision":crate::document_transaction::revision(content),"encoding":"utf-8","line_ending":"lf"})).unwrap();
        let service = Service::new(&data);
        Self { directory, data, assets, source, review, service, document }
    }
    fn prepare(&self, call: &str, format: &str, options: Value) -> Value {
        self.service.prepare(&self.review, &self.request(call, format, options), &self.assets).unwrap()
    }
    fn request(&self, call: &str, format: &str, options: Value) -> Value {
        json!({"document_id":self.document["document_id"],"generation":self.document["generation"],
            "review_revision":self.document["review_revision"],"format":format,"options":options,
            "suggested_name":format!("review.{format}"),"call_id":call})
    }
    fn change(&self, document: &Value, action: &str, call: &str, extra: Value) -> Value {
        let mut request = json!({"document_id":document["document_id"],"generation":document["generation"],
            "review_revision":document["review_revision"],"action":action,"call_id":call});
        for (key,value) in extra.as_object().unwrap() { request[key] = value.clone(); }
        self.review.handle(&request).unwrap()
    }
    fn read(&self) -> Value { self.review.handle(&json!({"action":"read","document_id":self.document["document_id"]})).unwrap() }
}
fn id(value: &Value) -> &str { value["snapshot_id"].as_str().unwrap() }

#[test]
fn presentation_freezes_images_and_runtime_and_still_renders_after_original_image_changes(){
    let content="# First slide\n\n![Red pixel](red.png)\n\n---\n\n## Second slide\n\nFinal body.\n";
    let mut fixture=Fixture::new(content);
    fixture.assets=fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")).unwrap();
    let mut png=std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(2,2,image::Rgba([255,0,0,255]))).write_to(&mut png,image::ImageFormat::Png).unwrap();
    let original=png.into_inner();let source=fixture.source.parent().unwrap().join("red.png");fs::write(&source,&original).unwrap();
    let prepared=fixture.prepare("presentation-freeze","presentation",json!({"theme":"white"}));
    fs::write(&source,b"Changed after preview preparation").unwrap();
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(fs::read(frozen.base_dir.join("red.png")).unwrap(),original);
    let (html,warnings)=crate::mdexport::presentation_with_images(&frozen.content,frozen.base_dir.to_str().unwrap(),"Slides",&frozen.options,true,&frozen.assets_dir).unwrap();
    assert!(warnings.is_empty());assert!(html.contains("data:image/png;base64,"));assert!(html.contains("Final body."));assert!(html.contains("new Reveal"));
    assert!(!html.contains("src=\"red.png\""));assert!(!html.contains("Changed after preview preparation"));
    fs::write(frozen.assets_dir.join("vendor/reveal/dist/readmd-boot.js"),b"replaced runtime").unwrap();
    assert_eq!(fixture.service.load(id(&prepared)).err().unwrap(),"export_snapshot_resource_changed");
}

#[test]
fn construction_is_lazy_and_prepare_is_idempotent_with_exact_private_options() {
    let directory = TempDir::new().unwrap(); let data = directory.path().join("unused");
    let _ = Service::new(&data); assert!(!data.exists());
    let fixture = Fixture::new("# Frozen\n");
    let options = json!({"typography":{"lineSpacing":{"mode":"exact","pt":17}},"meta":{"title":"Frozen"}});
    let mut request = fixture.request("prepare-once", "pdf", options.clone());
    request["resolved_options"] = json!({"canonical":options,"line":{"legacy_multiple":1.8}});
    let prepared = fixture.service.prepare(&fixture.review, &request, &fixture.assets).unwrap();
    let frozen = fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(frozen.content, "# Frozen\n");
    assert_eq!(frozen.options, options);
    assert_eq!(frozen.resolved_options, request["resolved_options"]);
    assert_eq!(fixture.service.prepare(&fixture.review, &request, &fixture.assets).unwrap(), prepared);
    request["options"]["meta"]["title"] = json!("Other");
    request["resolved_options"]["canonical"] = request["options"].clone();
    assert_eq!(fixture.service.prepare(&fixture.review, &request, &fixture.assets).unwrap_err(), "export_snapshot_call_conflict");
    let public = prepared.to_string();
    for hidden in ["legacy_multiple", "provider", "api_key", "Frozen"] { assert!(!public.contains(hidden)); }
    assert_eq!(prepared["pending_count"], 0);
    assert!(Uuid::parse_str(id(&prepared)).is_ok());
}

#[test]
fn public_prepare_identity_replays_before_live_resolution_and_survives_input_compaction() {
    let directory=TempDir::new().unwrap();let empty=Service::new(&directory.path().join("absent"));
    let identity=digest(b"canonical public request");
    assert!(empty.lookup_prepare("first",&identity).unwrap().is_none());
    assert!(!directory.path().join("absent").exists());
    let fixture=Fixture::new("# Original source");
    let mut request=fixture.request("public-identity","pdf",json!({"meta":{"title":"Original"}}));
    request["prepare_identity"]=json!(identity);
    let first=fixture.service.prepare(&fixture.review,&request,&fixture.assets).unwrap();
    assert_eq!(fixture.service.lookup_prepare("public-identity",&identity).unwrap().unwrap(),first);
    let _later=fixture.change(&fixture.document,"edit","new-source",json!({"edits":[{"from":0,"to":0,"insert":"---\ntitle: New title\n---\n"}]}));
    request["options"]["meta"]["title"]=json!("Live re-resolution changed");
    assert_eq!(fixture.service.prepare(&fixture.review,&request,&fixture.assets).unwrap(),first);
    assert_eq!(fixture.service.lookup_prepare("public-identity",&digest(b"changed public parameters")).unwrap_err(),"export_snapshot_call_conflict");
    fixture.service.discard(id(&first)).unwrap();
    let retry=fixture.service.lookup_prepare("public-identity",&identity).unwrap().unwrap();
    assert_eq!(retry["snapshot_id"],first["snapshot_id"]);assert_eq!(retry["discarded"],true);
    assert_eq!(retry["state"],"cancelled");
}

#[test]
fn actual_image_reference_bibliography_style_and_cover_bytes_are_owned() {
    let source = "---\nbibliography: refs/library.bib\ncsl: refs/style.csl\ncover: images/cover.png\n---\n# Evidence\n\n![plot][sample]\n\n[sample]: images/plot.png\n\n<img src='images/extra.png'>\n";
    let fixture = Fixture::new(source);
    fs::create_dir_all(fixture.source.parent().unwrap().join("images")).unwrap();
    fs::create_dir_all(fixture.source.parent().unwrap().join("refs")).unwrap();
    for (path, bytes) in [("images/cover.png", b"cover".as_slice()), ("images/plot.png", b"plot"), ("images/extra.png", b"extra"),
        ("refs/library.bib", b"@book{a,title={Original}}"), ("refs/style.csl", b"<style/>")] {
        fs::write(fixture.source.parent().unwrap().join(path), bytes).unwrap();
    }
    let prepared = fixture.prepare("all-local", "epub", json!({}));
    let frozen = fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(prepared["resource_count"], 5);
    fs::write(fixture.source.parent().unwrap().join("images/plot.png"), "changed").unwrap();
    fs::write(fixture.source.parent().unwrap().join("refs/library.bib"), "changed").unwrap();
    assert_eq!(fs::read(frozen.base_dir.join("images/plot.png")).unwrap(), b"plot");
    assert_eq!(fs::read(frozen.base_dir.join("refs/library.bib")).unwrap(), b"@book{a,title={Original}}");
    assert_eq!(fs::read(frozen.base_dir.join("images/cover.png")).unwrap(), b"cover");
    assert_eq!(frozen.review["content"], source);
}

#[test]
fn changing_source_assets_after_prepare_cannot_change_exported_html_bytes() {
    let fixture = Fixture::new("# Figure\n\n![fixed](image.png)\n");
    let old = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jA1cAAAAASUVORK5CYII=").unwrap();
    fs::write(fixture.source.parent().unwrap().join("image.png"), &old).unwrap();
    let prepared = fixture.prepare("old-pixels", "html", json!({}));
    fs::write(fixture.source.parent().unwrap().join("image.png"), "new bytes").unwrap();
    fs::write(fixture.assets.join("marked.min.js"), "changed runtime").unwrap();
    let frozen = fixture.service.load(id(&prepared)).unwrap();
    let output = fixture.directory.path().join("frozen.html");
    let result = crate::mdexport::export_document_with_data("html", &frozen.content, &frozen.base_dir.to_string_lossy(),
        &output.to_string_lossy(), &frozen.options, &frozen.name, &frozen.assets_dir, &fixture.data).unwrap();
    assert!(result.ok);
    let html = fs::read_to_string(output).unwrap();
    assert!(html.contains(&base64::engine::general_purpose::STANDARD.encode(&old)));
    assert!(html.contains("window.marked={parse: x=>x};"));
    assert!(!html.contains("changed runtime"));
}

#[test]
fn html_reference_images_raw_images_and_css_embed_owned_bytes_in_the_actual_output() {
    let content="# Frozen media\n\n![reference image][photo]\n\n[photo]: pictures/original.png \"retained title\"\n\n<img src='pictures/original.png' alt='raw'>\n\n<div style=\"background-image:linear-gradient(red,blue),url('pictures/original.png')\">CSS</div>\n";
    let fixture=Fixture::new(content);let images=fixture.source.parent().unwrap().join("pictures");fs::create_dir(&images).unwrap();
    let bytes=b"frozen original PNG bytes";fs::write(images.join("original.png"),bytes).unwrap();
    let prepared=fixture.prepare("html-all-image-syntaxes","html",json!({}));
    fs::write(images.join("original.png"),b"changed source pixels").unwrap();
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    let uri=format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes));
    assert!(frozen.content.contains(&format!("![reference image]({uri} \"retained title\")")));
    assert!(frozen.content.contains(&format!("src='{uri}'")));
    assert!(frozen.content.contains(&format!("url('{uri}')")));
    assert_eq!(frozen.review["content"],content);
    let output=fixture.directory.path().join("all-image-syntaxes.html");
    let result=crate::mdexport::export_document_with_data("html",&frozen.content,&frozen.base_dir.to_string_lossy(),&output.to_string_lossy(),
        &frozen.options,&frozen.name,&frozen.assets_dir,&fixture.data).unwrap();assert!(result.ok);
    let html=fs::read_to_string(&output).unwrap();
    assert!(html.matches(&uri).count()>=3);
    assert!(html.contains("linear-gradient(red,blue)"));
    assert!(!html.contains(&base64::engine::general_purpose::STANDARD.encode(b"changed source pixels")));
    // Publishing beside an unrelated same-name file cannot affect any of the
    // three rendered image references: all three now carry owned data bytes.
    fs::create_dir(fixture.directory.path().join("pictures")).unwrap();
    fs::write(fixture.directory.path().join("pictures/original.png"),b"destination pixels").unwrap();
    assert_eq!(fs::read_to_string(&output).unwrap(),html);
}

#[test]
fn unsupported_css_image_constructors_fail_before_the_html_exporter_can_write_an_artifact() {
    for constructor in ["image-set(\"plot.png\" 1x)","-webkit-image-set(\"plot.png\" 1x)","image(\"plot.png\")",
        "cross-fade(url(plot.png),url(plot.png),50%)","paint(private-resource)","-moz-element(#private-resource)","-WEBKIT-IMAGE-SET (\"plot.png\" 1x)"] {
        for content in [format!("<div style='background-image:{constructor}'>Frozen</div>\n"),format!("<style>.plot{{background-image:{constructor}}}</style>\n")] {
            let fixture=Fixture::new(&content);let output=fixture.directory.path().join("rejected.html");
            fs::write(fixture.source.parent().unwrap().join("plot.png"),b"live image bytes").unwrap();
            let exported=(||->Result<(),&'static str> {
                let prepared=fixture.service.prepare(&fixture.review,&fixture.request("unsupported-css","html",json!({})),&fixture.assets)?;
                let frozen=fixture.service.load(id(&prepared))?;
                let result=crate::mdexport::export_document_with_data("html",&frozen.content,&frozen.base_dir.to_string_lossy(),&output.to_string_lossy(),
                    &frozen.options,&frozen.name,&frozen.assets_dir,&fixture.data).map_err(|_|"export_snapshot_render_failed")?;
                if result.ok {Ok(())}else{Err("export_snapshot_render_failed")}
            })();
            assert_eq!(exported.unwrap_err(),"export_snapshot_dynamic_resource","{content}");
            assert!(!output.exists());
            assert!(!fs::read_dir(fixture.service.root()).unwrap().filter_map(Result::ok).any(|entry|Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok()));
        }
    }
}

#[test]
fn percent_encoded_resource_names_are_decoded_once_and_cannot_alias_parent_directories() {
    let fixture=Fixture::new("![literal](%252e%252e.png)");
    fs::write(fixture.source.parent().unwrap().join("%2e%2e.png"),b"literal percent filename").unwrap();
    let prepared=fixture.prepare("literal-percent","pdf",json!({}));
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(fs::read(frozen.base_dir.join("%2e%2e.png")).unwrap(),b"literal percent filename");
}

#[test]
fn application_asset_discovery_normalizes_checked_ancestors_and_keeps_resource_paths_strict() {
    let fixture=Fixture::new("# Ordinary text\n");let nested=fixture.assets.join("dev/bin");fs::create_dir_all(&nested).unwrap();
    let expected=dunce::canonicalize(&fixture.assets).unwrap();
    let lexical=dunce::canonicalize(&nested).unwrap().join("..").join("..");
    assert!(lexical.components().any(|part|matches!(part,Component::ParentDir)));
    assert!(open_absolute(&lexical).is_err());
    let canonical_join=fs::canonicalize(&nested).unwrap().join("..").join("..");
    // Windows normalizes parent steps while joining to a verbatim prefix.
    // The strict opener receives an already-normalized path in that case.
    #[cfg(windows)] {
        assert!(!canonical_join.components().any(|part|matches!(part,Component::ParentDir)));
        assert!(open_absolute(&canonical_join).is_ok());
    }
    #[cfg(not(windows))] {assert!(open_absolute(&canonical_join).is_err());}
    for candidate in [lexical,canonical_join] {
        let directory=application_assets_directory(&candidate).unwrap();assert_eq!(directory,expected);
        assert_eq!(read_absolute(&directory.join("marked.min.js"),MAX_RESOURCE_BYTES).unwrap(),b"window.marked={parse: x=>x};");
    }
    assert_eq!(resource_path("../marked.min.js").unwrap_err(),"export_snapshot_resource_path");
    assert_eq!(resource_path("%2e%2e/marked.min.js").unwrap_err(),"export_snapshot_resource_path");
    let prepared=fixture.prepare("ordinary-discovered-runtime","pdf",json!({}));
    fixture.service.validate_runtime(&fixture.service.load(id(&prepared)).unwrap(),&fixture.assets).unwrap();
}

#[cfg(unix)]
#[test]
fn application_asset_discovery_cannot_hide_a_symlink_before_parent_steps() {
    let fixture=Fixture::new("# Safe\n");let foreign=fixture.directory.path().join("foreign");fs::create_dir(&foreign).unwrap();
    let link=fixture.assets.join("linked");std::os::unix::fs::symlink(&foreign,&link).unwrap();
    assert_eq!(application_assets_directory(&link.join("..")).unwrap_err(),"export_snapshot_resource_path");
}

#[cfg(windows)]
#[test]
fn application_asset_discovery_cannot_hide_a_reparse_directory_before_parent_steps() {
    let fixture=Fixture::new("# Safe\n");let foreign=fixture.directory.path().join("foreign");fs::create_dir(&foreign).unwrap();
    let link=fixture.assets.join("linked");
    if std::os::windows::fs::symlink_dir(&foreign,&link).is_err() {return;}
    assert_eq!(application_assets_directory(&link.join("..")).unwrap_err(),"export_snapshot_resource_path");
}

#[test]
fn missing_escaping_remote_and_dynamic_resources_fail_before_any_snapshot_is_acknowledged() {
    for source in ["![x](missing.png)", "![x](../outside.png)", "![x](%2e%2e/outside.png)",
        "![x](file:///secret.png)", "![x](https://example.test/image.png)", "<img src='/secret.png'>",
        "<iframe src='local.html'></iframe>", "<link href='style.css' rel='stylesheet'>", "```vega-lite\n{\"data\":{\"url\":\"https://example.test/data.csv\"}}\n```"] {
        let fixture = Fixture::new(source);
        fs::write(fixture.directory.path().join("outside.png"), b"private").unwrap();
        assert!(fixture.service.prepare(&fixture.review, &fixture.request("reject", "pdf", json!({})), &fixture.assets).is_err(), "{source}");
        assert!(!fs::read_dir(fixture.service.root()).unwrap().filter_map(Result::ok).any(|entry| Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok()));
    }
    let fixture = Fixture::new("---\nbibliography: missing.bib\n---\n[@a]");
    assert_eq!(fixture.service.prepare(&fixture.review, &fixture.request("missing-bib", "pdf", json!({})), &fixture.assets).unwrap_err(), "export_snapshot_resource_missing");
    let fixture = Fixture::new("# Cover");
    assert_eq!(fixture.service.prepare(&fixture.review, &fixture.request("missing-cover", "epub", json!({"epub":{"cover":"missing.png"}})), &fixture.assets).unwrap_err(), "export_snapshot_resource_missing");
    let unsafe_svg=base64::engine::general_purpose::STANDARD.encode("<svg><image href='https://example.test/tracking.png'/></svg>");
    let fixture=Fixture::new(&format!("![x](data:image/svg+xml;base64,{unsafe_svg})"));
    assert_eq!(fixture.service.prepare(&fixture.review,&fixture.request("unsafe-inline-svg","pdf",json!({})),&fixture.assets).unwrap_err(),"svg_unsafe");
    let fixture=Fixture::new("```vega-lite\n{\"data\":{\"u\\u0072l\":\"remote.csv\"}}\n```");
    assert_eq!(fixture.service.prepare(&fixture.review,&fixture.request("escaped-vega-url","pdf",json!({})),&fixture.assets).unwrap_err(),"export_snapshot_dynamic_resource");
}

#[cfg(unix)]
#[test]
fn source_symlink_and_directory_symlink_are_never_followed() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new("![x](escape.png)");
    let private = fixture.directory.path().join("private.png"); fs::write(&private, "private").unwrap();
    symlink(&private, fixture.source.parent().unwrap().join("escape.png")).unwrap();
    assert!(fixture.service.prepare(&fixture.review, &fixture.request("symlink-file", "pdf", json!({})), &fixture.assets).is_err());
    let fixture = Fixture::new("![x](linked/private.png)");
    fs::write(fixture.directory.path().join("private.png"), "private").unwrap();
    symlink(fixture.directory.path(), fixture.source.parent().unwrap().join("linked")).unwrap();
    assert!(fixture.service.prepare(&fixture.review, &fixture.request("symlink-dir", "pdf", json!({})), &fixture.assets).is_err());
}

#[cfg(windows)]
#[test]
fn windows_reparse_resource_is_rejected_when_symlink_creation_is_available() {
    use std::os::windows::fs::symlink_file;
    let fixture = Fixture::new("![x](escape.png)");
    let private = fixture.directory.path().join("private.png"); fs::write(&private, "private").unwrap();
    if symlink_file(&private, fixture.source.parent().unwrap().join("escape.png")).is_err() { return; }
    assert!(fixture.service.prepare(&fixture.review, &fixture.request("symlink-file", "pdf", json!({})), &fixture.assets).is_err());
}

#[test]
fn snapshot_budget_is_aggregate_and_never_evicts_an_output_intent() {
    let fixture = Fixture::new("# Budget\n");
    let first = fixture.prepare("first-budget", "pdf", json!({}));
    let used = fixture.service.run(|tx| record_bytes(&get(tx, id(&first))?)).unwrap();
    let small = Service::with_limit(&fixture.data, used + 64);
    assert_eq!(small.prepare(&fixture.review, &fixture.request("over-budget", "pdf", json!({})), &fixture.assets).unwrap_err(), BUDGET);
    assert_eq!(fixture.service.get(id(&first)).unwrap(), first);
    let tiny = Service::with_limit(&fixture.data, 1);
    assert_eq!(tiny.prepare(&fixture.review, &fixture.request("too-small", "pdf", json!({})), &fixture.assets).unwrap_err(), BUDGET);
}

#[test]
fn completing_figures_checks_the_combined_budget_and_rolls_back_an_over_budget_completion() {
    let fixture=Fixture::new("```mermaid\ngraph TD\nA-->B\n```\n");
    let first=fixture.prepare("figures-budget-first","pdf",json!({"figures":{"style":"research"}}));
    let second=fixture.prepare("figures-budget-second","pdf",json!({"figures":{"style":"research"}}));
    let (first_record,second_record)=fixture.service.run(|tx|Ok((get(tx,id(&first))?,get(tx,id(&second))?))).unwrap();
    let fences=crate::scientific_figures::fences(&first_record.content,&first_record.options["figures"]);
    let rendered=json!([{"key":fences[0]["key"],"svg":format!("<svg><text>{}</text></svg>","x".repeat(4096))}]);
    let mut completed_first=first_record.clone();assign_figures(&mut completed_first,&rendered).unwrap();
    let mut completed_second=second_record.clone();assign_figures(&mut completed_second,&rendered).unwrap();
    let limit=record_bytes(&completed_first).unwrap()+record_bytes(&second_record).unwrap();
    assert!(record_bytes(&completed_first).unwrap()<=limit&&record_bytes(&completed_second).unwrap()<=limit);
    assert!(record_bytes(&completed_first).unwrap()+record_bytes(&completed_second).unwrap()>limit);
    let limited=Service::with_limit(&fixture.data,limit);
    assert_eq!(limited.complete_figures(id(&first),&rendered).unwrap()["figures_complete"],true);
    assert_eq!(limited.complete_figures(id(&second),&rendered).unwrap_err(),BUDGET);
    assert_eq!(fixture.service.get(id(&second)).unwrap(),second);
    assert!(fixture.service.load(id(&second)).unwrap().options.pointer("/figures/rendered").is_none());
    assert_eq!(fixture.service.complete_figures(id(&second),&rendered).unwrap()["figures_complete"],true);
}

#[test]
fn cancelled_or_failed_output_does_not_accept_and_cannot_later_commit() {
    let mut fixture = Fixture::new("A");
    fixture.document = fixture.change(&fixture.document, "propose", "pending", json!({"edits":[{"from":0,"to":1,"insert":"a"}]}));
    let cancelled = fixture.prepare("cancel-this", "pdf", json!({}));
    fixture.service.cancel(id(&cancelled)).unwrap();
    assert_eq!(fixture.service.load(id(&cancelled)).unwrap_err(), "export_snapshot_unavailable");
    let target = fixture.directory.path().join("cancelled.pdf");
    assert_eq!(fixture.service.begin_commit(id(&cancelled), &target, None, &digest(b"pdf")).unwrap_err(), "export_snapshot_unavailable");
    let failed = fixture.prepare("fail-this", "pdf", json!({}));
    fixture.service.fail(id(&failed), "render_failed").unwrap();
    assert_eq!(fixture.read()["changes"][0]["decision"], "pending");
    assert!(!target.exists());
}

#[test]
fn reopen_unknown_intent_only_reconciles_expected_artifact_and_preserves_later_change() {
    let mut fixture = Fixture::new("A B C");
    fixture.document = fixture.change(&fixture.document, "propose", "ab", json!({"origin":"ai","task_id":"original-owner",
        "edits":[{"from":0,"to":1,"insert":"a"},{"from":2,"to":3,"insert":"b"}]}));
    let prepared = fixture.prepare("durable-intent", "pdf", json!({}));
    let target = fixture.directory.path().join("verified.pdf"); let hash = digest(b"verified artifact");
    let intent = fixture.service.begin_commit(id(&prepared), &target, None, &hash).unwrap();
    assert_eq!(fixture.service.begin_commit(id(&prepared), &target, None, &hash).unwrap(), intent);
    assert_eq!(fixture.service.begin_commit(id(&prepared), &target.with_file_name("other.pdf"), None, &hash).unwrap_err(), "export_snapshot_commit_conflict");
    assert_eq!(fixture.service.cancel(id(&prepared)).unwrap_err(), "export_snapshot_commit_in_progress");
    assert_eq!(fixture.service.fail(id(&prepared), "unknown").unwrap_err(), "export_snapshot_commit_in_progress");
    let reopened = Service::new(&fixture.data);
    assert_eq!(reopened.finish(&fixture.review, id(&prepared)).unwrap()["state"], "needs_recovery");
    fs::write(&target, "wrong artifact").unwrap();
    assert_eq!(reopened.reconcile(&fixture.review, id(&prepared)).unwrap()["state"], "needs_recovery");
    assert_eq!(fs::read_to_string(&target).unwrap(), "wrong artifact");
    assert_eq!(fixture.read()["changes"][0]["decision"], "pending");
    let later = fixture.change(&fixture.document, "propose", "c", json!({"origin":"ai","task_id":"new-owner",
        "edits":[{"from":4,"to":5,"insert":"c"}]}));
    fs::write(&target, b"verified artifact").unwrap();
    let finished = reopened.finish(&fixture.review, id(&prepared)).unwrap();
    assert_eq!(finished["state"], "finished"); assert_eq!(finished["receipt"]["accepted_count"], 2);
    assert_eq!(reopened.reconcile(&fixture.review, id(&prepared)).unwrap(), finished);
    let accepted = fixture.read();
    assert_eq!(accepted["content"], later["content"]);
    assert_eq!(accepted["changes"][0]["decision"], "accepted");
    assert_eq!(accepted["changes"][1]["decision"], "accepted");
    assert_eq!(accepted["changes"][2]["decision"], "pending");
}

#[test]
fn crash_after_review_accept_before_snapshot_receipt_replays_the_same_acceptance_once() {
    let mut fixture = Fixture::new("A");
    fixture.document = fixture.change(&fixture.document, "propose", "candidate", json!({"edits":[{"from":0,"to":1,"insert":"a"}]}));
    let prepared = fixture.prepare("crash-window", "pdf", json!({}));
    let frozen = fixture.service.load(id(&prepared)).unwrap();
    let target = fixture.directory.path().join("crash.pdf");
    fixture.service.begin_commit(id(&prepared), &target, None, &digest(b"artifact")).unwrap();
    fs::write(&target, "artifact").unwrap();
    let accepted = fixture.review.accept_export_snapshot(&frozen.review, &format!("export-snapshot:{}", id(&prepared))).unwrap();
    let reopened = Service::new(&fixture.data);
    let finished = reopened.finish(&fixture.review, id(&prepared)).unwrap();
    assert_eq!(finished["state"], "finished");
    assert_eq!(fixture.read()["last_event_id"], accepted["last_event_id"]);
    assert_eq!(fixture.read()["review_revision"], accepted["review_revision"]);
}

#[test]
fn touched_candidate_conflicts_after_verified_output_without_accepting_new_text() {
    let mut fixture = Fixture::new("A D");
    fixture.document = fixture.change(&fixture.document, "propose", "ad", json!({"edits":[{"from":0,"to":1,"insert":"a"},{"from":2,"to":3,"insert":"d"}]}));
    let prepared = fixture.prepare("conflicted-output", "pdf", json!({}));
    let changed = fixture.change(&fixture.document, "edit", "touch-d", json!({"edits":[{"from":2,"to":3,"insert":"D"}]}));
    let target = fixture.directory.path().join("conflict.pdf");
    fixture.service.begin_commit(id(&prepared), &target, None, &digest(b"old artifact")).unwrap();
    fs::write(&target, b"old artifact").unwrap();
    let finished = fixture.service.finish(&fixture.review, id(&prepared)).unwrap();
    assert_eq!(finished["state"], "acceptance_conflict");
    assert_eq!(finished["receipt"]["accepted_count"], 1); assert_eq!(finished["receipt"]["conflict_count"], 1);
    assert_eq!(fixture.read()["content"], changed["content"]);
    assert_eq!(fixture.read()["changes"][1]["decision"], "conflict");
    assert_eq!(fs::read(&target).unwrap(), b"old artifact");
}

#[test]
fn tex_bundle_verifies_every_sidecar_and_never_accepts_partial_publication_or_source_overwrite() {
    let fixture = Fixture::new("# Bundle\n");
    let prepared = fixture.prepare("tex-bundle", "tex", json!({}));
    let target = fixture.directory.path().join("bundle.tex"); let sidecar = fixture.directory.path().join("figure.svg");
    let dependencies = vec![(sidecar.clone(), digest(b"figure"))];
    fixture.service.begin_commit_bundle(id(&prepared), &target, None, &digest(b"tex"), &dependencies).unwrap();
    fs::write(&target, "tex").unwrap();
    assert_eq!(fixture.service.finish(&fixture.review, id(&prepared)).unwrap()["state"], "needs_recovery");
    fs::write(&sidecar, "figure").unwrap();
    assert_eq!(fixture.service.finish(&fixture.review, id(&prepared)).unwrap()["state"], "finished");
    let other = fixture.prepare("source-overwrite", "tex", json!({}));
    assert_eq!(fixture.service.begin_commit(id(&other), &fixture.source, None, &digest(b"tex")).unwrap_err(), "export_snapshot_output_is_source");
}

#[test]
fn bundle_intent_accepts_safe_missing_parents_without_creating_them_and_rejects_parent_escape() {
    let fixture=Fixture::new("# Future bundle");let prepared=fixture.prepare("future-bundle","tex",json!({}));
    let target=fixture.directory.path().join("future.tex");
    let folder=fixture.directory.path().join("readmd-assets/export-owned");let sidecar=folder.join("figure.svg");
    fixture.service.begin_commit_bundle(id(&prepared),&target,None,&digest(b"tex"),&[(sidecar.clone(),digest(b"figure"))]).unwrap();
    assert!(!folder.exists());assert!(!fixture.directory.path().join("readmd-assets").exists());
    fs::create_dir_all(&folder).unwrap();fs::write(&sidecar,b"figure").unwrap();fs::write(&target,b"tex").unwrap();
    assert_eq!(fixture.service.finish(&fixture.review,id(&prepared)).unwrap()["state"],"finished");
    let other=fixture.prepare("escape-bundle","tex",json!({}));
    let escape=fixture.directory.path().join("missing/../escape.svg");
    assert_eq!(fixture.service.begin_commit_bundle(id(&other),&target,None,&digest(b"tex"),&[(escape,digest(b"figure"))]).unwrap_err(),INVALID);
}

#[test]
fn finished_receipt_is_historical_when_target_or_bundle_dependency_later_changes() {
    let mut fixture=Fixture::new("A");
    fixture.document=fixture.change(&fixture.document,"propose","historical-candidate",json!({"edits":[{"from":0,"to":1,"insert":"a"}]}));
    let prepared=fixture.prepare("historical-output","tex",json!({}));let target=fixture.directory.path().join("historical.tex");
    let sidecar=fixture.directory.path().join("historical.svg");
    fixture.service.begin_commit_bundle(id(&prepared),&target,None,&digest(b"tex"),&[(sidecar.clone(),digest(b"svg"))]).unwrap();
    fs::write(&target,b"tex").unwrap();fs::write(&sidecar,b"svg").unwrap();
    let finished=fixture.service.finish(&fixture.review,id(&prepared)).unwrap();assert_eq!(finished["artifact_current"],true);
    let accepted=fixture.read();fs::write(&target,b"later target").unwrap();
    let replay=fixture.service.reconcile(&fixture.review,id(&prepared)).unwrap();
    assert_eq!(replay["state"],"finished");assert_eq!(replay["artifact_current"],false);assert_eq!(replay["receipt"],finished["receipt"]);
    assert_eq!(fixture.read(),accepted);assert_eq!(fs::read(&target).unwrap(),b"later target");
    fs::write(&target,b"tex").unwrap();fs::write(&sidecar,b"later sidecar").unwrap();
    assert_eq!(fixture.service.reconcile(&fixture.review,id(&prepared)).unwrap()["artifact_current"],false);
    assert_eq!(fixture.read(),accepted);assert_eq!(fs::read(&sidecar).unwrap(),b"later sidecar");
}

#[test]
fn frozen_font_identity_hashes_actual_core_bytes_and_runtime_changes_invalidate_it() {
    let fixture = Fixture::new("# Fonts\n"); let prepared = fixture.prepare("fonts", "pdf", json!({}));
    let mut frozen = fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(frozen.font_identity["core"][0]["sha256"], digest(crate::publication_fonts::MATH));
    assert_eq!(frozen.font_identity["core"][1]["sha256"], digest(crate::publication_fonts::CORE_TEXT_FALLBACK));
    fixture.service.validate_runtime(&frozen, &fixture.assets).unwrap();
    frozen.font_identity["core"][0]["sha256"] = json!(digest(b"different face"));
    assert_eq!(fixture.service.validate_runtime(&frozen, &fixture.assets).unwrap_err(), "export_snapshot_runtime_changed");
}

#[test]
fn docx_snapshot_retains_typst_manifest_and_binds_the_chosen_projection_engine() {
    let fixture=Fixture::new("# DOCX projection\n");
    let name=format!("typst-0.15.1-{}.lock.json",crate::capability_packs::platform());
    fs::create_dir_all(fixture.assets.join("capabilities")).unwrap();
    let manifest=b"{\"schema\":1,\"id\":\"typst\",\"version\":\"0.15.1\",\"files\":[]}";
    fs::write(fixture.assets.join("capabilities").join(&name),manifest).unwrap();
    let prepared=fixture.prepare("docx-projection","docx",json!({}));
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(fs::read(frozen.assets_dir.join("capabilities").join(name)).unwrap(),manifest);
    assert_eq!(frozen.engine_identity["projection"]["engine"],"rust");
    fixture.service.validate_runtime(&frozen,&fixture.assets).unwrap();
}

fn install_runtime_fixture(fixture:&Fixture,id:&str)->PathBuf {
    let version=if id=="fonts" {crate::publication_fonts::VERSION}else{"0.15.1"};
    let platform=if id=="fonts" {"universal"}else{crate::capability_packs::platform()};
    let directory=crate::capability_store::root(&fixture.data).join(id).join(version);
    let names=if id=="fonts" {vec!["fonts/ReadMDSansSC-Regular.ttf","fonts/ReadMDSansSC-Bold.ttf","fonts/ReadMDSerifSC-Regular.ttf","fonts/ReadMDSerifSC-Bold.ttf"]}
        else {vec![if cfg!(windows){"bin/typst.exe"}else{"bin/typst"},"LICENSE","NOTICE"]};
    let mut files=Vec::new();
    for name in names {
        let bytes=format!("owned {id} runtime fixture {name}");let path=directory.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(&path,bytes.as_bytes()).unwrap();
        files.push(json!({"path":name,"size":bytes.len(),"sha256":digest(bytes.as_bytes())}));
    }
    fs::create_dir_all(fixture.assets.join("capabilities")).unwrap();
    let lock=json!({"schema":1,"id":id,"version":version,"platform":platform,"files":files,"review":{"license":"test fixture only"}});
    fs::write(fixture.assets.join(format!("capabilities/{id}-{version}-{platform}.lock.json")),serde_json::to_vec(&lock).unwrap()).unwrap();
    directory
}

#[test]
fn missing_selected_engine_freezes_inputs_then_binds_once_without_rereading_live_images() {
    let fixture=Fixture::new("# Pending engine\n\n![x](image.png)\n");
    fs::write(fixture.source.parent().unwrap().join("image.png"),b"original frozen pixels").unwrap();
    let prepared=fixture.prepare("pending-engine","pdf",json!({"pdf":{"engine":"typst"}}));
    assert_eq!(prepared["runtime_ready"],false);assert_eq!(prepared["pending_engine"],true);
    let output=fixture.directory.path().join("pending.pdf");
    assert_eq!(fixture.service.begin_commit(id(&prepared),&output,None,&digest(b"artifact")).unwrap_err(),"export_snapshot_runtime_unavailable");
    assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap_err(),"typst_engine_unavailable");
    assert!(!output.exists());
    fs::write(&fixture.source,"changed source").unwrap();
    fs::write(fixture.source.parent().unwrap().join("image.png"),b"changed live pixels").unwrap();
    let installed=install_runtime_fixture(&fixture,"typst");
    let bound=fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap();
    assert_eq!(bound["runtime_ready"],true);assert_eq!(bound["pending_engine"],false);
    assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap(),bound);
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    assert!(frozen.content.contains("Pending engine"));
    assert_eq!(fs::read(frozen.base_dir.join("image.png")).unwrap(),b"original frozen pixels");
    fixture.service.validate_runtime(&frozen,&fixture.assets).unwrap();
    fs::write(installed.join(if cfg!(windows){"bin/typst.exe"}else{"bin/typst"}),b"changed engine").unwrap();
    assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap_err(),"export_snapshot_runtime_changed");
    assert!(!output.exists());
}

#[test]
fn missing_cjk_fonts_bind_after_install_for_pdf_and_docx_but_english_core_is_already_ready() {
    for format in ["pdf","docx"] {
        let fixture=Fixture::new("# 中文研究\n");
        let prepared=fixture.prepare(&format!("pending-fonts-{format}"),format,json!({}));
        assert_eq!(prepared["runtime_ready"],false);assert_eq!(prepared["pending_fonts"],true);assert_eq!(prepared["pending_engine"],false);
        assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap_err(),"publication_fonts_unavailable");
        install_runtime_fixture(&fixture,"fonts");
        let ready=fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap();
        assert_eq!(ready["runtime_ready"],true);
        let frozen=fixture.service.load(id(&prepared)).unwrap();
        assert_eq!(frozen.font_identity["managed"].as_array().unwrap().len(),4);
        fixture.service.validate_runtime(&frozen,&fixture.assets).unwrap();
        assert_eq!(frozen.review["content"],"# 中文研究\n");
    }
    let fixture=Fixture::new("# English core\n");let ready=fixture.prepare("english-ready","pdf",json!({}));
    assert_eq!(ready["runtime_ready"],true);assert_eq!(ready["pending_fonts"],false);
    assert_eq!(fixture.service.bind_runtime(id(&ready),&fixture.assets).unwrap(),ready);
}

#[test]
fn pending_font_binding_cannot_change_an_engine_that_was_available_at_prepare() {
    let fixture=Fixture::new("# 研究\n");let engine=install_runtime_fixture(&fixture,"typst");
    let prepared=fixture.prepare("preserve-available-engine","pdf",json!({"pdf":{"engine":"typst"}}));
    assert_eq!(prepared["pending_engine"],false);assert_eq!(prepared["pending_fonts"],true);
    install_runtime_fixture(&fixture,"fonts");
    fs::write(engine.join(if cfg!(windows){"bin/typst.exe"}else{"bin/typst"}),b"replacement engine").unwrap();
    assert!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).is_err());
    assert_eq!(fixture.service.get(id(&prepared)).unwrap()["runtime_ready"],false);
}

#[test]
fn rendered_cjk_labels_can_bind_only_previously_missing_fonts_without_changing_frozen_inputs() {
    for format in ["pdf","docx"] {
        let fixture=Fixture::new("```mermaid\ngraph TD\nA-->B\n```\n");
        let options=json!({"figures":{"style":"research"}});
        let mut request=fixture.request(&format!("rendered-cjk-{format}"),format,options.clone());
        request["resolved_options"]=json!({"canonical":options});
        let prepared=fixture.service.prepare(&fixture.review,&request,&fixture.assets).unwrap();
        assert_eq!(prepared["runtime_ready"],true);
        fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap();
        let original=fixture.service.load(id(&prepared)).unwrap();
        let fences=crate::scientific_figures::fences(&original.content,&original.options["figures"]);
        let rendered=json!([{"key":fences[0]["key"],"svg":"<svg xmlns='http://www.w3.org/2000/svg'><text>中文标签</text></svg>"}]);
        let completed=fixture.service.complete_figures(id(&prepared),&rendered).unwrap();
        assert_eq!(completed["figures_complete"],true);assert_eq!(completed["runtime_ready"],false);
        assert_eq!(completed["pending_fonts"],true);assert_eq!(completed["pending_engine"],false);
        assert_eq!(fixture.service.complete_figures(id(&prepared),&rendered).unwrap(),completed);
        let pending=fixture.service.load(id(&prepared)).unwrap();
        assert_eq!(pending.engine_identity,original.engine_identity);assert_eq!(pending.font_identity,original.font_identity);
        assert_eq!(pending.content,original.content);assert_eq!(pending.resolved_options["canonical"],pending.options);
        assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap_err(),"publication_fonts_unavailable");
        let target=fixture.directory.path().join(format!("pending.{format}"));
        assert_eq!(fixture.service.begin_commit(id(&prepared),&target,None,&digest(b"output")).unwrap_err(),"export_snapshot_runtime_unavailable");
        fs::write(&fixture.source,"changed live source").unwrap();install_runtime_fixture(&fixture,"fonts");
        let ready=fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap();
        assert_eq!(ready["runtime_ready"],true);assert_eq!(ready["pending_fonts"],false);
        let bound=fixture.service.load(id(&prepared)).unwrap();
        assert_eq!(bound.engine_identity,original.engine_identity);assert_eq!(bound.font_identity["core"],original.font_identity["core"]);
        assert_eq!(bound.font_identity["managed"].as_array().unwrap().len(),4);
        assert_eq!(bound.content,original.content);assert_eq!(bound.options["figures"]["rendered"],rendered);
        assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap(),ready);
        fixture.service.validate_runtime(&bound,&fixture.assets).unwrap();assert!(!target.exists());
    }
}

#[test]
fn rendered_cjk_labels_cannot_rebind_a_previously_available_font_pack() {
    let fixture=Fixture::new("```mermaid\ngraph TD\nA-->B\n```\n");let directory=install_runtime_fixture(&fixture,"fonts");
    let prepared=fixture.prepare("rendered-cjk-existing-fonts","pdf",json!({"figures":{"style":"research"}}));
    let original=fixture.service.load(id(&prepared)).unwrap();
    let fences=crate::scientific_figures::fences(&original.content,&original.options["figures"]);
    let completed=fixture.service.complete_figures(id(&prepared),&json!([{"key":fences[0]["key"],"svg":"<svg><text>中文标签</text></svg>"}])).unwrap();
    assert_eq!(completed["runtime_ready"],true);assert_eq!(completed["pending_fonts"],false);
    let name="fonts/ReadMDSansSC-Regular.ttf";let replacement=b"new valid managed font revision";
    fs::write(directory.join(name),replacement).unwrap();
    let lock_path=fixture.assets.join(format!("capabilities/fonts-{}-universal.lock.json",crate::publication_fonts::VERSION));
    let mut lock:Value=serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    let file=lock["files"].as_array_mut().unwrap().iter_mut().find(|file|file["path"]==name).unwrap();
    file["size"]=json!(replacement.len());file["sha256"]=json!(digest(replacement));
    fs::write(lock_path,serde_json::to_vec(&lock).unwrap()).unwrap();
    let (_,changed)=runtime_identity(&fixture.data,&fixture.assets,"pdf",&original.options).unwrap();
    assert_eq!(changed["managed"].as_array().unwrap().len(),4);assert_ne!(changed["managed"],original.font_identity["managed"]);
    assert_eq!(fixture.service.bind_runtime(id(&prepared),&fixture.assets).unwrap_err(),"export_snapshot_runtime_changed");
    assert_eq!(fixture.service.load(id(&prepared)).unwrap().font_identity,original.font_identity);
}

#[test]
fn figure_completion_is_source_bound_safe_complete_and_immutable() {
    let fixture = Fixture::new("```mermaid\ngraph TD\nA-->B\n```\n");
    let prepared = fixture.prepare("figures", "pdf", json!({"figures":{"style":"research"}}));
    let target = fixture.directory.path().join("diagram.pdf");
    assert_eq!(fixture.service.begin_commit(id(&prepared), &target, None, &digest(b"pdf")).unwrap_err(), "diagram_client_renderer_required");
    let fences = crate::scientific_figures::fences(&fixture.service.load(id(&prepared)).unwrap().content, &json!({"style":"research"}));
    let rendered = json!([{"key":fences[0]["key"],"svg":"<svg xmlns='http://www.w3.org/2000/svg' width='80' height='40'><path d='M0 0L80 40'/></svg>"}]);
    assert_eq!(fixture.service.complete_figures(id(&prepared), &json!([])).unwrap_err(), "diagram_client_renderer_required");
    let unsafe_svg = json!([{"key":fences[0]["key"],"svg":"<svg><script>bad</script></svg>"}]);
    assert_eq!(fixture.service.complete_figures(id(&prepared), &unsafe_svg).unwrap_err(), "svg_unsafe");
    let completed = fixture.service.complete_figures(id(&prepared), &rendered).unwrap();
    assert_eq!(completed["figures_complete"], true);
    assert_eq!(fixture.service.complete_figures(id(&prepared), &rendered).unwrap(), completed);
    let mut changed = rendered.clone(); changed[0]["svg"] = json!("<svg width='1' height='1'/>");
    assert_eq!(fixture.service.complete_figures(id(&prepared), &changed).unwrap_err(), "export_snapshot_figures_conflict");
    fixture.service.begin_commit(id(&prepared), &target, None, &digest(b"pdf")).unwrap();
    assert_eq!(fixture.service.complete_figures(id(&prepared), &rendered).unwrap_err(), "export_snapshot_commit_in_progress");
}

#[test]
fn unsaved_draft_exports_real_html_and_profile_owned_staged_images_without_source_sidecars() {
    let mut fixture = Fixture::new("Existing document");
    let asset = crate::document_assets::stage(&fixture.data, b"original draft pixels", "draft-image", "png").unwrap();
    let content = format!("# Unsaved draft\n\n![draft]({})\n",asset.reference);
    fixture.document = fixture.review.handle(&json!({"action":"open","path":"draft:new_note-123","content":content,"disk_content":"",
        "disk_revision":crate::document_transaction::revision(""),"encoding":"utf-8","line_ending":"lf"})).unwrap();
    let prepared = fixture.prepare("unsaved-export", "html", json!({}));
    let frozen = fixture.service.load(id(&prepared)).unwrap();
    assert_eq!(frozen.review["content"],content);
    assert!(frozen.content.contains(&format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(b"original draft pixels"))));
    assert!(!frozen.content.contains("/raw?p="));
    assert!(!fixture.source.parent().unwrap().join("readmd-assets").exists());
    assert_eq!(fs::read_to_string(&fixture.source).unwrap(),"Existing document");
    let output = fixture.directory.path().join("unsaved.html");
    let result = crate::mdexport::export_document_with_data("html",&frozen.content,&frozen.base_dir.to_string_lossy(),&output.to_string_lossy(),
        &frozen.options,&frozen.name,&frozen.assets_dir,&fixture.data).unwrap();
    assert!(result.ok);
    let html=fs::read_to_string(&output).unwrap();
    assert!(html.contains("Unsaved draft"));
    assert!(html.contains(&base64::engine::general_purpose::STANDARD.encode(b"original draft pixels")));
    assert_eq!(fixture.read()["path"],"draft:new_note-123");
}

#[test]
fn unsaved_draft_never_uses_profile_files_as_an_implicit_document_base() {
    let mut fixture=Fixture::new("Existing");
    fs::create_dir_all(&fixture.data).unwrap(); fs::write(fixture.data.join("private.png"),b"private").unwrap();
    fixture.document=fixture.review.handle(&json!({"action":"open","path":"draft:safe","content":"![x](private.png)","disk_content":"",
        "disk_revision":crate::document_transaction::revision(""),"encoding":"utf-8","line_ending":"lf"})).unwrap();
    assert_eq!(fixture.service.prepare(&fixture.review,&fixture.request("draft-private","pdf",json!({})),&fixture.assets).unwrap_err(),"export_snapshot_resource_path");
}

#[test]
fn discard_releases_only_known_owned_inputs_and_preserves_receipt_output_and_call_identity() {
    let fixture=Fixture::new("# Retained receipt\n\n![x](image.png)\n");
    fs::write(fixture.source.parent().unwrap().join("image.png"),b"input pixels").unwrap();
    let prepared=fixture.prepare("discard-finished","pdf",json!({}));
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    let output=fixture.directory.path().join("discard-output.pdf");
    fixture.service.begin_commit(id(&prepared),&output,None,&digest(b"finished output")).unwrap();
    fs::write(&output,b"finished output").unwrap();
    let finished=fixture.service.finish(&fixture.review,id(&prepared)).unwrap();
    let discarded=fixture.service.discard(id(&prepared)).unwrap();
    assert_eq!(discarded["discarded"],true); assert_eq!(discarded["state"],"finished");
    assert_eq!(discarded["receipt"],finished["receipt"]);
    assert_eq!(fixture.service.discard(id(&prepared)).unwrap(),discarded);
    assert_eq!(fixture.service.load(id(&prepared)).unwrap_err(),"export_snapshot_unavailable");
    assert!(!frozen.base_dir.join("image.png").exists());
    assert_eq!(fs::read(&output).unwrap(),b"finished output");
    assert_eq!(fs::read(fixture.source.parent().unwrap().join("image.png")).unwrap(),b"input pixels");
    let retry=fixture.service.prepare(&fixture.review,&fixture.request("discard-finished","pdf",json!({})),&fixture.assets).unwrap();
    assert_eq!(retry["snapshot_id"],prepared["snapshot_id"]);
    assert_eq!(retry["receipt"],finished["receipt"]);
}

#[test]
fn cleanup_compacts_old_terminal_inputs_and_never_retires_unverified_intents() {
    let fixture=Fixture::new("# Cleanup\n");
    let completed=fixture.prepare("old-finished","html",json!({}));
    let target=fixture.directory.path().join("old.html");
    fixture.service.begin_commit(id(&completed),&target,None,&digest(b"old output")).unwrap();
    fs::write(&target,b"old output").unwrap(); fixture.service.finish(&fixture.review,id(&completed)).unwrap();
    fixture.service.run(|tx| {let mut record=get(tx,id(&completed))?;record.created_at=now()-8*24*60*60;put(tx,&record)}).unwrap();
    let unresolved=fixture.prepare("unverified","html",json!({}));
    let target2=fixture.directory.path().join("unknown.html");
    fixture.service.begin_commit(id(&unresolved),&target2,None,&digest(b"unverified output")).unwrap();
    fixture.service.run(|tx| {let mut record=get(tx,id(&unresolved))?;record.created_at=now()-8*24*60*60;put(tx,&record)}).unwrap();
    fixture.service.cleanup_finished().unwrap();
    assert_eq!(fixture.service.get(id(&completed)).unwrap()["discarded"],true);
    assert_eq!(fixture.service.get(id(&unresolved)).unwrap()["discarded"],false);
    assert_eq!(fixture.service.get(id(&unresolved)).unwrap()["state"],"commit_intent");
    assert_eq!(fs::read(&target).unwrap(),b"old output");
    assert_eq!(fixture.service.discard(id(&unresolved)).unwrap_err(),"export_snapshot_commit_in_progress");
}

#[test]
fn cleanup_preserves_unknown_replacement_bytes_instead_of_deleting_them() {
    let fixture=Fixture::new("![x](image.png)");
    fs::write(fixture.source.parent().unwrap().join("image.png"),b"known").unwrap();
    let prepared=fixture.prepare("unknown-owned-path","pdf",json!({}));
    let frozen=fixture.service.load(id(&prepared)).unwrap();
    fs::write(frozen.base_dir.join("image.png"),b"unknown replacement").unwrap();
    assert_eq!(fixture.service.discard(id(&prepared)).unwrap_err(),"export_snapshot_resource_changed");
    assert_eq!(fs::read(frozen.base_dir.join("image.png")).unwrap(),b"unknown replacement");
}

#[test]
fn completed_input_cleanup_preserves_unregistered_hidden_files_and_the_published_output() {
    let fixture=Fixture::new("![x](image.png)");
    fs::write(fixture.source.parent().unwrap().join("image.png"),b"known image").unwrap();
    let prepared=fixture.prepare("hidden-input-cleanup","pdf",json!({}));let frozen=fixture.service.load(id(&prepared)).unwrap();
    let output=fixture.directory.path().join("preserved.pdf");fixture.service.begin_commit(id(&prepared),&output,None,&digest(b"published")).unwrap();
    fs::write(&output,b"published").unwrap();fixture.service.finish(&fixture.review,id(&prepared)).unwrap();
    let unknown=frozen.base_dir.join(".private-user-notes");fs::write(&unknown,b"unregistered private bytes").unwrap();
    fixture.service.discard(id(&prepared)).unwrap();
    assert_eq!(fs::read(&unknown).unwrap(),b"unregistered private bytes");
    assert_eq!(fs::read(&output).unwrap(),b"published");
    assert!(!frozen.base_dir.join("image.png").exists());
}
