//! Shared-service regression coverage; every filesystem input is fixture-owned.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    area: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let area = std::env::temp_dir().join(format!(
            "readmd-openapi-local-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        let root = area.join("api");
        fs::create_dir_all(&root).unwrap();
        Self { area, root }
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    fn snapshot(&self, document: &Value) -> SourceSnapshot {
        let source = serde_json::to_string(document).unwrap();
        let path = self.write("api.json", &source);
        LocalReferences::snapshot(&path, &source).unwrap()
    }
    fn project(&self, document: &Value, content: &str) -> Value {
        project_snapshot(
            &self.snapshot(document),
            "api.json",
            "fixture/api.json",
            content,
            &document_transaction::revision(content),
            &json!({}),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.area.starts_with(std::env::temp_dir()));
        assert!(self
            .area
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("readmd-openapi-local-"));
        let _ = fs::remove_dir_all(&self.area);
    }
}
fn document(reference: &str) -> Value {
    json!({"openapi":"3.1.1","info":{"title":"Local API 😀","version":"1"},
        "paths":{"/notes/{id}":{"get":{"responses":{"200":{"$ref":reference}}}}}})
}
fn warning(snapshot: &SourceSnapshot, code: &str) -> bool {
    snapshot
        .warnings
        .iter()
        .any(|warning| warning["code"] == code)
}

#[test]
fn mixed_json_yaml_refs_are_relative_to_their_own_file() {
    let fixture = Fixture::new();
    fixture.write("defs/response.yaml", "description: From YAML\ncontent:\n  application/json:\n    schema: {$ref: '../schemas/note.json#/$defs/Note'}\n");
    fixture.write("schemas/note.json", r##"{"$defs":{"Note":{"type":"object","properties":{"x-field":{"$ref":"../defs/string.yaml"},"allowed":{"$ref":"../defs/boolean.yaml"}}}}}"##);
    fixture.write("defs/string.yaml", "type: string\nminLength: 2\n");
    fixture.write("defs/boolean.yaml", "false\n");
    let source = document("defs/response.yaml");
    let snapshot = fixture.snapshot(&source);
    let schema = &snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["content"]
        ["application/json"]["schema"];
    assert_eq!(schema["properties"]["x-field"]["minLength"], 2);
    assert_eq!(schema["properties"]["allowed"], false);
    assert_eq!(snapshot.references.len(), 4);
    assert!(snapshot.warnings.is_empty());
    let out = fixture.project(&source, "# Author notes 😀\r\n");
    let text = out["result"]["content"].as_str().unwrap();
    assert!(text.contains("From YAML"));
    assert!(text.contains("minLength"));
    assert!(!text.contains(fixture.area.to_str().unwrap()));
    for map in out["source_map"].as_array().unwrap() {
        let offset =
            document_transaction::byte_offset(text, map["from"].as_u64().unwrap() as usize)
                .unwrap();
        assert!(text[offset..].starts_with("## GET"));
        assert_eq!(map["pointer"], "/paths/~1notes~1{id}/get");
    }
}

#[test]
fn escaped_unicode_names_and_json_pointer_tokens_are_decoded_once() {
    let fixture = Fixture::new();
    fixture.write(
        "defs/研究 response.yaml",
        "'a/b~c': {description: Decoded pointer}\n",
    );
    fixture.write(
        "defs/literal%20name.json",
        r#"{"description":"One decode only"}"#,
    );
    let snapshot = fixture.snapshot(&document(
        "defs/%E7%A0%94%E7%A9%B6%20response.yaml#/a~1b~0c",
    ));
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["description"],
        "Decoded pointer"
    );
    let snapshot = fixture.snapshot(&document("defs/literal%2520name.json"));
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["description"],
        "One decode only"
    );
    for invalid in [
        "defs/literal%QQ.json",
        "defs/literal%FF.json",
        "defs/literal%20name.json#/bad~2escape",
        "defs/研究%20response.yaml#anchor",
    ] {
        let snapshot = fixture.snapshot(&document(invalid));
        assert!(warning(&snapshot, "external_reference"));
        assert!(snapshot.references.is_empty());
    }
}

#[test]
fn reference_object_siblings_and_schema_siblings_keep_version_semantics() {
    let fixture = Fixture::new();
    fixture.write(
        "response.yaml",
        "description: Original\ncontent:\n  application/json:\n    schema: {type: string}\n",
    );
    fixture.write(
        "schema.yaml",
        "type: object\nproperties: {name: {type: string}}\n",
    );
    let mut source = document("response.yaml");
    source["paths"]["/notes/{id}"]["get"]["responses"]["200"]["description"] = json!("Override");
    let snapshot = fixture.snapshot(&source);
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["description"],
        "Override"
    );
    source["paths"]["/notes/{id}"]["get"]["requestBody"] = json!({"content":{"application/json":{"schema":{
        "$ref":"schema.yaml","unevaluatedProperties":false,"allOf":[{"minProperties":1}],"description":"Sibling"
    }}}});
    let snapshot = fixture.snapshot(&source);
    let schema = &snapshot.document["paths"]["/notes/{id}"]["get"]["requestBody"]["content"]
        ["application/json"]["schema"];
    assert_eq!(schema["allOf"][0]["type"], "object");
    assert_eq!(schema["allOf"][1]["minProperties"], 1);
    assert_eq!(schema["unevaluatedProperties"], false);
    assert_eq!(schema["description"], "Sibling");
    source["openapi"] = json!("3.0.4");
    let snapshot = fixture.snapshot(&source);
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["description"],
        "Original"
    );
    let schema = &snapshot.document["paths"]["/notes/{id}"]["get"]["requestBody"]["content"]
        ["application/json"]["schema"];
    assert_eq!(schema["type"], "object");
    assert!(schema.get("unevaluatedProperties").is_none());
}

#[test]
fn external_path_items_report_both_callsite_and_definition() {
    let fixture = Fixture::new();
    fixture.write("paths/note.yaml", "get:\n  summary: Read from dependency\n  parameters: [{$ref: '../params.yaml#/id'}]\n  responses: {'200': {description: Works}}\n");
    fixture.write(
        "params.yaml",
        "id: {name: id, in: path, required: true, schema: {type: integer}}\n",
    );
    let mut source = document("unused.yaml");
    source["paths"]["/notes/{id}"] = json!({"$ref":"paths/note.yaml","post":{"responses":{"204":{"description":"Saved locally"}}}});
    let out = fixture.project(&source, "# Manual 😀\r\n");
    assert_eq!(out["source_map"].as_array().unwrap().len(), 2);
    let get = &out["source_map"][0];
    assert_eq!(get["definition"]["source"], "paths/note.yaml");
    assert_eq!(get["definition"]["pointer"], "/get");
    // The POST was authored in the main file, not in the referenced Path Item.
    assert!(out["result"]["content"]
        .as_str()
        .unwrap()
        .contains("Saved locally"));
    assert_eq!(out["source_map"][1]["definition"]["source"], "api.json");
    assert_eq!(
        out["source_map"][1]["definition"]["pointer"],
        "/paths/~1notes~1{id}/post"
    );
    assert!(out["reference_map"]
        .as_array()
        .unwrap()
        .iter()
        .any(|map| map["source"] == "params.yaml" && map["target"] == "/id"));
}

#[test]
fn overlapping_external_path_item_operations_are_preserved_and_diagnosed() {
    let fixture = Fixture::new();
    fixture.write(
        "path.yaml",
        "get: {responses: {'200': {description: Dependency}}}\n",
    );
    let mut source = document("unused.yaml");
    source["paths"]["/notes/{id}"]["$ref"] = json!("path.yaml");
    let snapshot = fixture.snapshot(&source);
    assert!(warning(&snapshot, "external_reference"));
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["$ref"],
        "path.yaml"
    );
}

#[test]
fn schema_resource_ids_anchors_and_dynamic_scopes_are_preserved() {
    let fixture = Fixture::new();
    fixture.write("scope.yaml", "$id: 'https://example.invalid/schema'\n$defs:\n  Note: {type: object, properties: {value: {$ref: 'relative.yaml'}}}\n");
    fixture.write(
        "dynamic.yaml",
        "type: object\nproperties: {self: {$dynamicRef: '#node'}}\n",
    );
    for reference in ["scope.yaml#/$defs/Note", "dynamic.yaml"] {
        let mut source = document("unused.yaml");
        source["paths"]["/notes/{id}"]["get"]["responses"]["200"] = json!({"description":"Schema","content":{"application/json":{"schema":{"$ref":reference}}}});
        let snapshot = fixture.snapshot(&source);
        assert!(warning(&snapshot, "external_reference"));
        assert_eq!(
            snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            reference
        );
        assert_eq!(snapshot.references.len(), 1);
    }
}

#[test]
fn examples_defaults_and_extensions_cannot_trigger_file_reads() {
    let fixture = Fixture::new();
    fixture.write("payload.yaml", "description: MUST NOT RESOLVE\n");
    let mut source = document("response.yaml");
    fixture.write("response.yaml", "description: Response\ncontent:\n  application/json:\n    example: {$ref: 'payload.yaml'}\n    examples:\n      literal: {value: {$ref: 'payload.yaml'}}\n    schema:\n      type: object\n      default: {$ref: 'payload.yaml'}\n      examples: [{$ref: 'payload.yaml'}]\n      x-example: {$ref: 'payload.yaml'}\n");
    source["x-example"] = json!({"$ref":"payload.yaml"});
    let snapshot = fixture.snapshot(&source);
    assert_eq!(snapshot.references.len(), 1);
    assert_eq!(snapshot.references[0]["source"], "response.yaml");
    let text = serde_json::to_string(&snapshot.document).unwrap();
    assert!(text.contains("payload.yaml"));
    assert!(!text.contains("MUST NOT RESOLVE"));
}

#[test]
fn every_supported_reusable_component_uses_the_same_local_resolver() {
    let fixture = Fixture::new();
    fixture.write("components.yaml", "parameter: {name: q, in: query, schema: {type: string}}\nbody: {content: {application/json: {schema: {type: string}}}}\nresponse: {description: Shared}\nheader: {schema: {type: integer}}\nexample: {value: {$ref: literal}}\nlink: {operationId: readNote}\nsecurity: {type: http, scheme: bearer}\ncallback:\n  '{$request.body#/url}':\n    post: {responses: {'204': {description: Notified}}}\n");
    let mut source = document("components.yaml#/response");
    for (section, pointer) in [
        ("parameters", "parameter"),
        ("requestBodies", "body"),
        ("responses", "response"),
        ("headers", "header"),
        ("examples", "example"),
        ("links", "link"),
        ("securitySchemes", "security"),
        ("callbacks", "callback"),
    ] {
        source["components"][section]["Reusable"] =
            json!({"$ref":format!("components.yaml#/{pointer}")});
    }
    let snapshot = fixture.snapshot(&source);
    assert!(snapshot.warnings.is_empty());
    assert_eq!(snapshot.references.len(), 1);
    assert_eq!(
        snapshot.document["components"]["parameters"]["Reusable"]["name"],
        "q"
    );
    assert_eq!(
        snapshot.document["components"]["examples"]["Reusable"]["value"]["$ref"],
        "literal"
    );
    assert_eq!(
        snapshot.document["components"]["securitySchemes"]["Reusable"]["scheme"],
        "bearer"
    );
}

#[test]
fn cycles_and_missing_targets_are_bounded_and_remain_qualified() {
    let fixture = Fixture::new();
    fixture.write("defs/cycle.yaml", "A: {$ref: '#/B'}\nB: {$ref: '#/A'}\n");
    let snapshot = fixture.snapshot(&document("defs/cycle.yaml#/A"));
    assert!(warning(&snapshot, "recursive_reference"));
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["$ref"],
        "defs/cycle.yaml#/A"
    );
    fixture.write("defs/missing.yaml", "description: Preserved response\ncontent: {application/json: {schema: {$ref: '#/$defs/Absent'}}}\n");
    let snapshot = fixture.snapshot(&document("defs/missing.yaml"));
    assert!(warning(&snapshot, "missing_reference"));
    assert_eq!(
        snapshot.document["paths"]["/notes/{id}"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"]["$ref"],
        "defs/missing.yaml#/$defs/Absent"
    );
}

#[test]
fn denied_uris_and_parent_escape_never_read_or_echo_outside_content() {
    let fixture = Fixture::new();
    fs::write(
        fixture.area.join("outside.yaml"),
        "description: OUTSIDE FIXTURE CONTENT\n",
    )
    .unwrap();
    for reference in [
        "../outside.yaml",
        "%2e%2e/outside.yaml",
        "https://example.invalid/schema.yaml",
        "file:///outside.yaml",
        "C:/outside.yaml",
        "//host/share.yaml",
        "\\\\host\\share.yaml",
        "response.yaml?query=1",
        "response.txt",
        "response%00.yaml",
    ] {
        let snapshot = fixture.snapshot(&document(reference));
        assert!(warning(&snapshot, "external_reference"));
        assert!(snapshot.references.is_empty());
        assert!(!serde_json::to_string(&snapshot.document)
            .unwrap()
            .contains("OUTSIDE FIXTURE CONTENT"));
        assert!(!serde_json::to_string(&snapshot.reference_map)
            .unwrap()
            .contains(fixture.area.to_str().unwrap()));
    }
}

#[test]
fn native_directory_link_cannot_escape_reference_root() {
    let fixture = Fixture::new();
    let outside = fixture.area.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(
        outside.join("secret.yaml"),
        "description: LINK TARGET MUST STAY PRIVATE\n",
    )
    .unwrap();
    let link = fixture.root.join("linked");
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Native junction fixture must be available; do not silently skip"
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let snapshot = fixture.snapshot(&document("linked/secret.yaml"));
    assert!(warning(&snapshot, "external_reference"));
    assert!(snapshot.references.is_empty());
    assert!(!serde_json::to_string(&snapshot.document)
        .unwrap()
        .contains("LINK TARGET MUST STAY PRIVATE"));
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    #[cfg(unix)]
    fs::remove_file(&link).unwrap();
    assert_eq!(
        fs::read_to_string(outside.join("secret.yaml")).unwrap(),
        "description: LINK TARGET MUST STAY PRIVATE\n"
    );
}

#[test]
fn root_and_dependencies_have_stable_revision_receipts() {
    let fixture = Fixture::new();
    let source = document("response.yaml");
    fixture.write("response.yaml", "description: First\n");
    let first = fixture.snapshot(&source);
    assert_eq!(first.revision, fixture.snapshot(&source).revision);
    fixture.write("unrelated.yaml", "description: Unrelated\n");
    assert_eq!(first.revision, fixture.snapshot(&source).revision);
    fixture.write("response.yaml", "description: Second\n");
    assert_ne!(first.revision, fixture.snapshot(&source).revision);
    fs::remove_file(fixture.root.join("response.yaml")).unwrap();
    let missing = fixture.snapshot(&source);
    assert!(warning(&missing, "missing_reference"));
    assert_ne!(first.revision, missing.revision);
    fixture.write("response.yaml", "description: First\n");
    assert_eq!(first.revision, fixture.snapshot(&source).revision);
    assert_ne!(missing.revision, fixture.snapshot(&source).revision);
}

#[test]
fn shared_endpoint_rejects_dependency_changes_before_accepting_old_preview() {
    let fixture = Fixture::new();
    fixture.write("response.yaml", "description: First\n");
    let root_source = serde_json::to_string(&document("response.yaml")).unwrap();
    let source = fixture.write("api.json", &root_source);
    let profile = fixture.area.join("profile");
    fs::create_dir(&profile).unwrap();
    let app = Arc::new(
        App::bootstrap(crate::paths::AppPaths::with_dirs(
            &profile,
            &fixture.root,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        ))
        .unwrap(),
    );
    let args = json!({"file_path":source,"content":"# Author 😀\n","base_revision":document_transaction::revision("# Author 😀\n"),"locale":"en"});
    let preview: Value = serde_json::from_slice(&handle(&app, &args).unwrap().body).unwrap();
    let mut checked = args.clone();
    checked["expected_source_revision"] = preview["source_revision"].clone();
    assert_eq!(handle(&app, &checked).unwrap().status, 200);
    fixture.write("response.yaml", "description: Changed after review\n");
    let failure = handle(&app, &checked).err().unwrap();
    assert_eq!(
        (failure.status, failure.code.as_str()),
        (409, "transaction_conflict")
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), root_source);
    fs::remove_file(fixture.root.join("response.yaml")).unwrap();
    assert_eq!(
        handle(&app, &checked).err().unwrap().code,
        "transaction_conflict"
    );
}

#[test]
fn update_is_reversible_idempotent_and_keeps_author_and_local_edits() {
    let fixture = Fixture::new();
    let source = document("response.yaml");
    fixture.write("response.yaml", "description: First dependency\n");
    let first = fixture.project(&source, "# Author 😀\r\n");
    let text = format!(
        "{}\r\n\r\n## Author tail\r\nKeep this.\r\n",
        first["result"]["content"].as_str().unwrap()
    );
    assert_eq!(fixture.project(&source, &text)["result"]["content"], text);
    fixture.write("response.yaml", "description: Updated dependency\n");
    let updated = fixture.project(&source, &text);
    assert_eq!(
        updated["changes"],
        json!([{"kind":"changed","pointer":"/paths/~1notes~1{id}/get"}])
    );
    let updated_text = updated["result"]["content"].as_str().unwrap();
    assert!(updated_text.ends_with("## Author tail\r\nKeep this.\r\n"));
    let inverse: Vec<TextEdit> =
        serde_json::from_value(updated["result"]["inverse"].clone()).unwrap();
    let undo = document_transaction::apply(
        updated_text,
        &ContentTransaction {
            origin: "openapi".into(),
            base_revision: document_transaction::revision(updated_text),
            edits: inverse,
        },
    )
    .unwrap();
    assert_eq!(undo.content, text);
    let manual = text.replace("First dependency", "Manual change");
    assert_eq!(
        project_snapshot(
            &fixture.snapshot(&source),
            "api.json",
            "fixture/api.json",
            &manual,
            &document_transaction::revision(&manual),
            &json!({})
        )
        .unwrap_err(),
        "openapi_local_changes"
    );
}

#[test]
fn single_file_projection_and_revision_remain_compatible() {
    let fixture = Fixture::new();
    let mut source = document("#/components/responses/Ok");
    source["components"]["responses"]["Ok"] = json!({"description":"Internal reference"});
    source["components"]["schemas"]["Local"] = json!({"$id":"urn:single-file","type":"string"});
    let text = serde_json::to_string(&source).unwrap();
    let snapshot = fixture.snapshot(&source);
    assert_eq!(snapshot.revision, document_transaction::revision(&text));
    assert!(snapshot.references.is_empty());
    let plain = project_with_identity(
        &text,
        "api.json",
        "fixture/api.json",
        "Author 😀\n",
        &document_transaction::revision("Author 😀\n"),
        &json!({}),
    )
    .unwrap();
    assert_eq!(fixture.project(&source, "Author 😀\n"), plain);
}

#[test]
fn file_count_chain_depth_expansion_and_source_size_have_hard_budgets() {
    let fixture = Fixture::new();
    for index in 0..34 {
        fixture.write(
            &format!("chain{index}.yaml"),
            &format!("$ref: 'chain{}.yaml'\n", index + 1),
        );
    }
    let text = serde_json::to_string(&document("chain0.yaml")).unwrap();
    let root = fixture.write("api.json", &text);
    assert_eq!(
        LocalReferences::snapshot(&root, &text).err().unwrap(),
        BUDGET
    );
    fixture.write("large.yaml", &"x".repeat(MAX_SOURCE + 1));
    let text = serde_json::to_string(&document("large.yaml")).unwrap();
    let root = fixture.write("api.json", &text);
    assert_eq!(
        LocalReferences::snapshot(&root, &text).err().unwrap(),
        BUDGET
    );
    let mut source = document("unused.yaml");
    source["paths"]["/notes/{id}"]["get"]["responses"] = json!({});
    for index in 0..65 {
        fixture.write(&format!("file{index}.yaml"), "description: File\n");
        source["paths"]["/notes/{id}"]["get"]["responses"][format!("{}", 200 + index)] =
            json!({"$ref":format!("file{index}.yaml")});
    }
    let text = serde_json::to_string(&source).unwrap();
    let root = fixture.write("api.json", &text);
    assert_eq!(
        LocalReferences::snapshot(&root, &text).err().unwrap(),
        BUDGET
    );
    fixture.write(
        "expand.yaml",
        &format!("description: '{}'\n", "x".repeat(1024 * 1024)),
    );
    source["paths"]["/notes/{id}"]["get"]["responses"] = json!({});
    for index in 0..20 {
        source["paths"]["/notes/{id}"]["get"]["responses"][format!("{}", 200 + index)] =
            json!({"$ref":"expand.yaml"});
    }
    let text = serde_json::to_string(&source).unwrap();
    let root = fixture.write("api.json", &text);
    assert_eq!(
        LocalReferences::snapshot(&root, &text).err().unwrap(),
        BUDGET
    );

    let node_schema = json!({"type":"object","properties":(0..4000)
        .map(|index|(format!("p{index}"),json!({"type":"string"}))).collect::<Map<String,Value>>()});
    fixture.write("nodes.json", &serde_json::to_string(&node_schema).unwrap());
    source["paths"]["/notes/{id}"]["get"]["responses"] = json!({});
    for index in 0..30 {
        source["paths"]["/notes/{id}"]["get"]["responses"][format!("{}", 200 + index)] = json!({"description":"Nodes","content":{"application/json":{"schema":{"$ref":"nodes.json"}}}});
    }
    let text = serde_json::to_string(&source).unwrap();
    let root = fixture.write("api.json", &text);
    assert_eq!(
        LocalReferences::snapshot(&root, &text).err().unwrap(),
        BUDGET
    );
}

fn directory_link(link: &Path, outside: &Path) {
    #[cfg(windows)]
    {
        let result = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(outside)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "The native junction fixture must exist; no silent skip"
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside, link).unwrap();
}
fn remove_directory_link(link: &Path) {
    #[cfg(windows)]
    fs::remove_dir(link).unwrap();
    #[cfg(unix)]
    fs::remove_file(link).unwrap();
}

#[test]
fn dependency_parent_swap_between_lookup_and_open_is_blocked_or_refused() {
    let fixture = Fixture::new();
    let source = serde_json::to_string(&document("nested/response.yaml")).unwrap();
    let root = fixture.write("api.json", &source);
    fixture.write(
        "nested/response.yaml",
        "description: Original owned response\n",
    );
    let root = crate::paths::strip_verbatim(fs::canonicalize(root).unwrap());
    let nested = fixture.root.join("nested");
    let moved = fixture.root.join("nested-original");
    let outside = fixture.area.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(
        outside.join("response.yaml"),
        "description: COMPETING OUTSIDE CONTENT\n",
    )
    .unwrap();
    let reader = SourceReader::new(root.parent().unwrap()).unwrap();
    let mut references = LocalReferences::new(root.clone(), &source, reader).unwrap();
    let (resolved, _) = references.target(&root, "nested/response.yaml").unwrap();
    // Deterministic scheduling of the exact target() -> load() boundary.
    match fs::rename(&nested, &moved) {
        Ok(()) => {
            directory_link(&nested, &outside);
            assert_eq!(
                references.load(&resolved).err().unwrap(),
                "transaction_conflict"
            );
            remove_directory_link(&nested);
            fs::rename(&moved, &nested).unwrap();
        }
        Err(error) => {
            #[cfg(windows)]
            assert!(
                matches!(error.raw_os_error(), Some(5 | 32)),
                "Unexpected native rename error"
            );
            #[cfg(not(windows))]
            panic!("Unexpected rename error: {error}");
            assert_eq!(
                references.load(&resolved).unwrap().unwrap()["description"],
                "Original owned response"
            );
            drop(references);
            // The fixture is writable once our native guards are dropped.
            fs::rename(&nested, &moved).unwrap();
            fs::rename(&moved, &nested).unwrap();
        }
    }
    assert_eq!(
        fs::read_to_string(outside.join("response.yaml")).unwrap(),
        "description: COMPETING OUTSIDE CONTENT\n"
    );
}

#[test]
fn stale_canonical_path_is_rejected_before_reading_a_replacement_link() {
    let fixture = Fixture::new();
    let original = fixture.write(
        "nested/response.yaml",
        "description: Original owned response\n",
    );
    let previously_resolved = crate::paths::strip_verbatim(fs::canonicalize(original).unwrap());
    let nested = fixture.root.join("nested");
    fs::rename(&nested, fixture.root.join("nested-original")).unwrap();
    let outside = fixture.area.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(
        outside.join("response.yaml"),
        "description: STALE PATH MUST NOT READ THIS\n",
    )
    .unwrap();
    directory_link(&nested, &outside);
    let canonical_root = crate::paths::strip_verbatim(fs::canonicalize(&fixture.root).unwrap());
    let reader = SourceReader::new(&canonical_root).unwrap();
    assert_eq!(
        reader.read(&previously_resolved).err().unwrap(),
        "transaction_conflict"
    );
    drop(reader);
    remove_directory_link(&nested);
    assert_eq!(
        fs::read_to_string(outside.join("response.yaml")).unwrap(),
        "description: STALE PATH MUST NOT READ THIS\n"
    );
}

#[test]
fn already_opened_file_handle_cannot_be_redirected_by_a_later_parent_swap() {
    let fixture = Fixture::new();
    let source = fixture.write(
        "nested/response.yaml",
        "description: Opened owned response\n",
    );
    let canonical_root = crate::paths::strip_verbatim(fs::canonicalize(&fixture.root).unwrap());
    let source = crate::paths::strip_verbatim(fs::canonicalize(source).unwrap());
    let nested = fixture.root.join("nested");
    let moved = fixture.root.join("nested-original");
    let outside = fixture.area.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(
        outside.join("response.yaml"),
        "description: AFTER OPEN OUTSIDE CONTENT\n",
    )
    .unwrap();
    let reader = SourceReader::new(&canonical_root).unwrap();
    let opened = reader.open(&source).unwrap();
    let replaced = match fs::rename(&nested, &moved) {
        Ok(()) => {
            directory_link(&nested, &outside);
            true
        }
        Err(error) => {
            #[cfg(windows)]
            assert!(
                matches!(error.raw_os_error(), Some(5 | 32)),
                "Unexpected native rename error"
            );
            #[cfg(not(windows))]
            panic!("Unexpected rename error: {error}");
            false
        }
    };
    assert_eq!(
        read_source(&opened.file).unwrap(),
        "description: Opened owned response\n"
    );
    drop(opened);
    drop(reader);
    if replaced {
        remove_directory_link(&nested);
        fs::rename(&moved, &nested).unwrap();
    } else {
        fs::rename(&nested, &moved).unwrap();
        fs::rename(&moved, &nested).unwrap();
    }
    assert_eq!(
        fs::read_to_string(outside.join("response.yaml")).unwrap(),
        "description: AFTER OPEN OUTSIDE CONTENT\n"
    );
}

#[test]
fn local_reference_parsing_obeys_the_shared_cancellation_context() {
    let fixture = Fixture::new();
    let text = serde_json::to_string(&document("response.yaml")).unwrap();
    let root = fixture.write("api.json", &text);
    fixture.write("response.yaml", "description: Response\n");
    crate::speech::with_cancel(Arc::new(|| true), || {
        assert_eq!(
            LocalReferences::snapshot(&root, &text).err().unwrap(),
            "cancelled"
        )
    });
    assert!(LocalReferences::snapshot(&root, &text).is_ok());
}
