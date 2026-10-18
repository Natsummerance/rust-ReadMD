//! Offline OpenAPI → Markdown projections. Generated regions carry provenance
//! and a digest; updates are reviewable, revision-checked, and never write files.
use crate::document_transaction::{self, ContentTransaction, TextEdit};
use crate::{
    server::{ok_json, Request, Response},
    ApiError, ApiResult, App,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use pulldown_cmark::{Event as MdEvent, Parser as MdParser};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use yaml_rust2::{
    parser::{Event, Parser},
    scanner::TScalarStyle,
    Yaml,
};

const MAX_SOURCE: usize = 8 * 1024 * 1024;
const MAX_OUTPUT: usize = 16 * 1024 * 1024;
const INVALID: &str = "openapi_invalid";
const BUDGET: &str = "openapi_budget";
type Result<T> = std::result::Result<T, &'static str>;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
struct Admission;
impl Drop for Admission {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Frame {
    value: Value,
    key: Option<String>,
    anchor: usize,
}

/// Build a JSON-compatible tree from bounded YAML events. Alias expansion has
/// its own serialized-byte budget; a small exponential alias input cannot
/// allocate an unbounded tree. No application-defined tag is ever executed.
pub fn parse(source: &str) -> Result<Value> {
    if crate::speech::cancelled() {
        return Err("cancelled");
    }
    if source.len() > MAX_SOURCE {
        return Err(BUDGET);
    }
    let mut parser = Parser::new_from_str(source);
    let mut stack: Vec<Frame> = Vec::new();
    let mut anchors = HashMap::new();
    let mut root = None;
    let (mut documents, mut nodes, mut expanded) = (0, 0, 0usize);
    loop {
        let (event, _) = parser.next_token().map_err(|_| INVALID)?;
        nodes += 1;
        if nodes % 1024 == 0 && crate::speech::cancelled() {
            return Err("cancelled");
        }
        if nodes > 200_000 || stack.len() > 64 {
            return Err(BUDGET);
        }
        let (value, anchor) = match event {
            Event::StreamEnd => break,
            Event::DocumentStart => {
                documents += 1;
                if documents > 1 {
                    return Err(INVALID);
                }
                continue;
            }
            Event::MappingStart(anchor, tag) => {
                if tag.is_some() {
                    return Err(INVALID);
                }
                stack.push(Frame {
                    value: json!({}),
                    key: None,
                    anchor,
                });
                continue;
            }
            Event::SequenceStart(anchor, tag) => {
                if tag.is_some() {
                    return Err(INVALID);
                }
                stack.push(Frame {
                    value: json!([]),
                    key: None,
                    anchor,
                });
                continue;
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let frame = stack.pop().ok_or(INVALID)?;
                if frame.key.is_some() {
                    return Err(INVALID);
                }
                (frame.value, frame.anchor)
            }
            Event::Alias(id) => {
                let value: &Value = anchors.get(&id).ok_or(INVALID)?;
                expanded += serde_json::to_vec(value).map_err(|_| INVALID)?.len();
                if expanded > MAX_OUTPUT {
                    return Err(BUDGET);
                }
                (value.clone(), 0)
            }
            Event::Scalar(text, style, anchor, tag) => {
                // JSON object names (including numeric HTTP statuses) are strings.
                let key = stack
                    .last()
                    .is_some_and(|f| f.value.is_object() && f.key.is_none());
                let explicit = tag.as_ref().map(|t| (t.handle.as_str(), t.suffix.as_str()));
                let force_string = explicit == Some(("tag:yaml.org,2002:", "str"));
                if explicit.is_some_and(|(h, s)| {
                    h != "tag:yaml.org,2002:"
                        || !matches!(s, "str" | "int" | "float" | "bool" | "null")
                }) {
                    return Err(INVALID);
                }
                let value = if key || force_string || style != TScalarStyle::Plain {
                    json!(text)
                } else {
                    match Yaml::from_str(&text) {
                        Yaml::Null => Value::Null,
                        Yaml::Boolean(b) => json!(b),
                        Yaml::Integer(i) => json!(i),
                        Yaml::Real(n) => {
                            serde_json::Number::from_f64(n.parse().map_err(|_| INVALID)?)
                                .map(Value::Number)
                                .ok_or(INVALID)?
                        }
                        _ => json!(text),
                    }
                };
                (value, anchor)
            }
            _ => continue,
        };
        if anchor != 0 {
            expanded += serde_json::to_vec(&value).map_err(|_| INVALID)?.len();
            if expanded > MAX_OUTPUT || anchors.len() > 4096 {
                return Err(BUDGET);
            }
            anchors.insert(anchor, value.clone());
        }
        if let Some(parent) = stack.last_mut() {
            if let Some(array) = parent.value.as_array_mut() {
                array.push(value);
            } else if let Some(key) = parent.key.take() {
                if parent
                    .value
                    .as_object_mut()
                    .ok_or(INVALID)?
                    .insert(key, value)
                    .is_some()
                {
                    return Err(INVALID);
                }
            } else {
                parent.key = Some(value.as_str().ok_or(INVALID)?.to_owned());
            }
        } else if root.replace(value).is_some() {
            return Err(INVALID);
        }
    }
    let value = root.filter(Value::is_object).ok_or(INVALID)?;
    if !stack.is_empty() || documents != 1 {
        return Err(INVALID);
    }
    let version = value["openapi"].as_str().ok_or(INVALID)?;
    if !(version.starts_with("3.0.") || version.starts_with("3.1.")) {
        return Err("openapi_version");
    }
    if value["info"]["title"].as_str().is_none()
        || value["info"]["version"].as_str().is_none()
        || !value["paths"].is_object()
            && !(version.starts_with("3.1.") && value["webhooks"].is_object())
    {
        return Err(INVALID);
    }
    Ok(value)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "")
}
fn cell(text: &str) -> String {
    escape(text).replace('|', "&#124;").replace('\n', "<br>")
}
fn pointer(text: &str) -> String {
    text.replace('~', "~0").replace('/', "~1")
}
fn code(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let tick = "`".repeat(longest + 1);
    format!("{tick} {} {tick}", cell(text))
}
fn fenced(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}json\n{text}\n{fence}\n\n")
}
fn label<'a>(labels: &'a Value, key: &'a str) -> &'a str {
    labels[key].as_str().unwrap_or(key)
}

struct Projection<'a> {
    root: &'a Value,
    labels: &'a Value,
    warnings: Vec<Value>,
    maps: Vec<Value>,
    operations: BTreeMap<String, String>,
}
impl Projection<'_> {
    fn warning(&mut self, code: &str, pointer: &str) {
        if self.warnings.len() < 256 {
            self.warnings.push(json!({"code":code,"pointer":pointer}));
        }
    }
    fn resolve(&mut self, value: &Value, location: &str) -> Value {
        let mut value = value.clone();
        let mut seen = HashSet::new();
        for _ in 0..16 {
            let Some(target) = value["$ref"].as_str().map(str::to_owned) else {
                return value;
            };
            if !target.starts_with("#/") {
                self.warning("external_reference", location);
                return value;
            }
            if !seen.insert(target.clone()) {
                self.warning("recursive_reference", location);
                return value;
            }
            let decoded = percent_encoding::percent_decode_str(&target[1..]).decode_utf8();
            let Some(resolved) = decoded.ok().and_then(|p| self.root.pointer(&p)).cloned() else {
                self.warning("missing_reference", location);
                return value;
            };
            if self.root["openapi"]
                .as_str()
                .unwrap_or("")
                .starts_with("3.1.")
            {
                let siblings = value.as_object().cloned().unwrap_or_default();
                value = resolved;
                if let Some(object) = value.as_object_mut() {
                    for key in ["summary", "description"] {
                        if let Some(v) = siblings.get(key) {
                            object.insert(key.into(), v.clone());
                        }
                    }
                }
            } else {
                value = resolved;
            }
        }
        self.warning("recursive_reference", location);
        value
    }
    fn section(&self, key: &str, value: &Value) -> String {
        if value.is_null()
            || value.as_array().is_some_and(Vec::is_empty)
            || value.as_object().is_some_and(Map::is_empty)
        {
            return String::new();
        }
        format!("#### {}\n\n{}", label(self.labels, key), fenced(value))
    }
    fn generate(&mut self) -> Result<String> {
        let mut out = format!(
            "# {}\n\n{} · {}\n\n",
            escape(self.root["info"]["title"].as_str().unwrap_or("")),
            escape(self.root["info"]["version"].as_str().unwrap_or("")),
            escape(self.root["openapi"].as_str().unwrap_or(""))
        );
        if let Some(description) = self.root["info"]["description"].as_str() {
            out.push_str(&escape(description));
            out.push_str("\n\n");
        }
        out.push_str(&self.section("openapi.servers", &self.root["servers"]));
        let mut paths: Vec<_> = self.root["paths"]
            .as_object()
            .into_iter()
            .flatten()
            .collect();
        paths.sort_by_key(|(p, _)| *p);
        let mut count = 0;
        for (path, item) in paths {
            if path.starts_with("x-") {
                continue;
            }
            if !path.starts_with('/') {
                return Err(INVALID);
            }
            let path_pointer = format!("/paths/{}", pointer(path));
            let item = self.resolve(item, &path_pointer);
            for method in [
                "get", "post", "put", "patch", "delete", "head", "options", "trace",
            ] {
                if crate::speech::cancelled() {
                    return Err("cancelled");
                }
                let Some(operation) = item.get(method) else {
                    continue;
                };
                if !operation.is_object() {
                    return Err(INVALID);
                }
                count += 1;
                if count > 2048 {
                    return Err(BUDGET);
                }
                let location = format!("{path_pointer}/{method}");
                let start_byte = out.len();
                let start = out.encode_utf16().count();
                let summary = operation["summary"].as_str().unwrap_or("");
                out.push_str(&format!(
                    "## {} {}{}\n\n",
                    method.to_uppercase(),
                    code(path),
                    if summary.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", cell(summary))
                    }
                ));
                if let Some(id) = operation["operationId"].as_str() {
                    out.push_str(&format!("{}\n\n", code(id)));
                }
                if operation["deprecated"] == true {
                    out.push_str(&format!(
                        "**{}**\n\n",
                        label(self.labels, "openapi.deprecated")
                    ));
                }
                if let Some(description) = operation["description"].as_str() {
                    out.push_str(&escape(description));
                    out.push_str("\n\n");
                }
                let servers = operation.get("servers").or_else(|| item.get("servers"));
                if let Some(servers) = servers {
                    out.push_str(&self.section("openapi.servers", servers));
                }
                let mut parameters: BTreeMap<(String, String), Value> = BTreeMap::new();
                for group in [&item["parameters"], &operation["parameters"]] {
                    if !group.is_null() && !group.is_array() {
                        return Err(INVALID);
                    }
                    for raw in group.as_array().into_iter().flatten() {
                        let p = self.resolve(raw, &format!("{location}/parameters"));
                        let (Some(name), Some(position)) = (p["name"].as_str(), p["in"].as_str())
                        else {
                            self.warning("missing_reference", &location);
                            continue;
                        };
                        parameters.insert((position.into(), name.into()), p);
                    }
                }
                if !parameters.is_empty() {
                    out.push_str(&format!(
                        "### {}\n\n| {} | {} | {} | {} | {} |\n| --- | --- | --- | --- | --- |\n",
                        label(self.labels, "openapi.parameters"),
                        label(self.labels, "openapi.name"),
                        label(self.labels, "openapi.location"),
                        label(self.labels, "openapi.required"),
                        label(self.labels, "openapi.schema"),
                        label(self.labels, "openapi.description")
                    ));
                    for ((position, name), p) in parameters {
                        out.push_str(&format!(
                            "| {} | {} | {} | {} | {} |\n",
                            code(&name),
                            code(&position),
                            label(
                                self.labels,
                                if position == "path" || p["required"] == true {
                                    "openapi.yes"
                                } else {
                                    "openapi.no"
                                }
                            ),
                            cell(&serde_json::to_string(&p["schema"]).unwrap_or_default()),
                            cell(p["description"].as_str().unwrap_or(""))
                        ));
                        // Preserve serialization, examples and content properties too.
                        if p.get("content").is_some()
                            || p.get("examples").is_some()
                            || p.get("style").is_some()
                        {
                            out.push_str("\n");
                            out.push_str(&fenced(&p));
                        }
                    }
                    out.push('\n');
                }
                let body = self.resolve(
                    &operation["requestBody"],
                    &format!("{location}/requestBody"),
                );
                out.push_str(&self.section("openapi.requestBody", &body));
                if !operation["responses"].is_object() {
                    return Err(INVALID);
                }
                let mut responses = Map::new();
                for (status, response) in operation["responses"].as_object().unwrap() {
                    responses.insert(
                        status.clone(),
                        self.resolve(
                            response,
                            &format!("{location}/responses/{}", pointer(status)),
                        ),
                    );
                }
                out.push_str(&self.section("openapi.responses", &Value::Object(responses)));
                let security = operation.get("security").unwrap_or(&self.root["security"]);
                // Explicit [] clears inherited security; keep this visible.
                if !security.is_null() {
                    out.push_str(&format!(
                        "#### {}\n\n{}",
                        label(self.labels, "openapi.security"),
                        fenced(security)
                    ));
                }
                for key in ["callbacks", "externalDocs", "tags"] {
                    if let Some(v) = operation.get(key) {
                        out.push_str(&self.section(&format!("openapi.{key}"), v));
                    }
                }
                self.operations.insert(
                    location.clone(),
                    document_transaction::revision(&out[start_byte..]),
                );
                self.maps.push(json!({"pointer":location,"method":method,"path":path,"operation_id":operation["operationId"],"from":start,"to":out.encode_utf16().count()}));
                if out.len() > MAX_OUTPUT {
                    return Err(BUDGET);
                }
            }
        }
        for (key, label_key) in [
            ("schemas", "openapi.schemas"),
            ("securitySchemes", "openapi.securitySchemes"),
        ] {
            out.push_str(&self.section(label_key, &self.root["components"][key]));
        }
        out.push_str(&self.section("openapi.webhooks", &self.root["webhooks"]));
        if out.len() > MAX_OUTPUT {
            return Err(BUDGET);
        }
        Ok(out)
    }
}

pub fn project(
    source: &str,
    source_name: &str,
    content: &str,
    base_revision: &str,
    labels: &Value,
) -> Result<Value> {
    project_with_identity(
        source,
        source_name,
        source_name,
        content,
        base_revision,
        labels,
    )
}
fn project_with_identity(
    source: &str,
    source_name: &str,
    identity: &str,
    content: &str,
    base_revision: &str,
    labels: &Value,
) -> Result<Value> {
    if content.len() > document_transaction::MAX_DOCUMENT_BYTES {
        return Err(BUDGET);
    }
    if document_transaction::revision(content) != base_revision {
        return Err("transaction_conflict");
    }
    let source_name = Path::new(source_name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or(INVALID)?;
    if identity.len() > 4096 || identity.chars().any(char::is_control) {
        return Err(INVALID);
    }
    let id = document_transaction::revision(identity);
    let doc = parse(source)?;
    let mut projection = Projection {
        root: &doc,
        labels,
        warnings: vec![],
        maps: vec![],
        operations: BTreeMap::new(),
    };
    let body = projection.generate()?;
    let marker = "<!-- readmd:openapi:start ";
    let end_marker = format!("<!-- readmd:openapi:end {id} -->");
    let mut start = None;
    let mut end = None;
    for (event, range) in MdParser::new(content).into_offset_iter() {
        let MdEvent::Html(raw) = event else { continue };
        for (offset, line) in raw.split_inclusive('\n').scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line))
        }) {
            let at = range.start + offset;
            if at > 0 && content.as_bytes()[at - 1] != b'\n' {
                continue;
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(encoded) = line
                .strip_prefix(marker)
                .and_then(|s| s.strip_suffix(" -->"))
            {
                if encoded.len() > 1024 * 1024 {
                    return Err(BUDGET);
                }
                let metadata: Value = serde_json::from_slice(
                    &URL_SAFE_NO_PAD
                        .decode(encoded)
                        .map_err(|_| "openapi_region_invalid")?,
                )
                .map_err(|_| "openapi_region_invalid")?;
                if metadata["id"] == id {
                    if start.is_some() || metadata["schema"] != 1 {
                        return Err("openapi_region_invalid");
                    }
                    let after = content[at..]
                        .find('\n')
                        .map(|n| at + n + 1)
                        .ok_or("openapi_region_invalid")?;
                    start = Some((at, after, metadata));
                }
            }
            if line == end_marker {
                if end.replace((at, at + line.len())).is_some() {
                    return Err("openapi_region_invalid");
                }
            }
        }
    }
    let (from, to, previous) = match (start, end) {
        (Some((from, body_start, meta)), Some((body_end, to))) if body_end >= body_start => {
            if meta["body_sha256"]
                != document_transaction::revision(
                    &content[body_start..body_end]
                        .replace("\r\n", "\n")
                        .replace('\r', "\n"),
                )
            {
                return Err("openapi_local_changes");
            }
            (from, to, meta["operations"].clone())
        }
        (None, None) => (content.len(), content.len(), json!({})),
        _ => return Err("openapi_region_invalid"),
    };
    let mut changes = vec![];
    for (pointer, digest) in &projection.operations {
        let kind = if previous.get(pointer).is_none() {
            "added"
        } else if previous[pointer] != *digest {
            "changed"
        } else {
            continue;
        };
        changes.push(json!({"pointer":pointer,"kind":kind}));
    }
    for key in previous.as_object().into_iter().flatten().map(|(k, _)| k) {
        if !projection.operations.contains_key(key) {
            changes.push(json!({"pointer":key,"kind":"removed"}));
        }
    }
    let metadata = json!({"schema":1,"id":id,"source":source_name,"source_sha256":document_transaction::revision(source),"body_sha256":document_transaction::revision(&body),"operations":projection.operations});
    let prefix = if from == content.len() && !content.is_empty() {
        "\n\n"
    } else {
        ""
    };
    let mut insert = format!(
        "{prefix}{marker}{} -->\n{body}{end_marker}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&metadata).map_err(|_| INVALID)?)
    );
    let crlf = content.contains("\r\n") && !content.replace("\r\n", "").contains('\n');
    if crlf {
        insert = insert.replace('\n', "\r\n");
    }
    let tx = ContentTransaction {
        base_revision: base_revision.into(),
        origin: "openapi".into(),
        edits: vec![TextEdit {
            from: content[..from].encode_utf16().count(),
            to: content[..to].encode_utf16().count(),
            insert,
        }],
    };
    let result = document_transaction::apply(content, &tx)?;
    let map_offset = content[..from].encode_utf16().count()
        + prefix.len()
        + marker.len()
        + URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&metadata).unwrap())
            .len()
        + 5
        + if crlf {
            prefix.bytes().filter(|b| *b == b'\n').count() + 1
        } else {
            0
        };
    for entry in &mut projection.maps {
        for key in ["from", "to"] {
            let position = entry[key].as_u64().unwrap() as usize;
            let newline_offset = if crlf {
                body[..document_transaction::byte_offset(&body, position)?]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
            } else {
                0
            };
            entry[key] = json!(position + map_offset + newline_offset);
        }
        entry["source"] = json!(source_name);
    }
    Ok(
        json!({"ok":true,"result":result,"transaction":tx,"source_map":projection.maps,"changes":changes,"warnings":projection.warnings,"source_name":source_name,"source_revision":metadata["source_sha256"]}),
    )
}

pub fn endpoint(app: &Arc<App>, request: &Request) -> ApiResult<Response> {
    if request.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let v = request.json()?;
    let task = v["task_id"].as_str().and_then(crate::cancel::register);
    let internal = task
        .as_ref()
        .map(|t| t.checker())
        .unwrap_or_else(|| Arc::new(|| false));
    let external = crate::speech::cancellation_check();
    crate::speech::with_cancel(Arc::new(move || internal() || external()), || {
        handle(app, &v)
    })
}
fn handle(app: &Arc<App>, v: &Value) -> ApiResult<Response> {
    ACTIVE
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 2).then_some(n + 1)
        })
        .map_err(|_| ApiError::new(429, "openapi_busy"))?;
    let _admission = Admission;
    let source = if let Some(file) = v["file_path"].as_str() {
        let path = app.paths.resolve_doc(file)?;
        if fs::metadata(&path)
            .map_err(|_| ApiError::new(404, "file_not_found"))?
            .len()
            > MAX_SOURCE as u64
        {
            return Err(ApiError::new(422, BUDGET));
        }
        fs::read_to_string(path).map_err(|_| ApiError::new(422, INVALID))?
    } else {
        v["source"]
            .as_str()
            .ok_or_else(|| ApiError::new(422, INVALID))?
            .into()
    };
    if v["expected_source_revision"]
        .as_str()
        .is_some_and(|expected| expected != document_transaction::revision(&source))
    {
        return Err(ApiError::new(409, "transaction_conflict"));
    }
    let locale = v["locale"].as_str().unwrap_or("en");
    if !locale
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || locale.len() > 16
    {
        return Err(ApiError::new(422, INVALID));
    }
    let labels: Value = fs::read(app.paths.assets_dir.join(format!("i18n/{locale}.json")))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| ApiError::new(422, INVALID))?;
    let name = v["source_name"]
        .as_str()
        .or_else(|| v["file_path"].as_str())
        .unwrap_or("openapi.json");
    let identity = v["source_id"]
        .as_str()
        .or_else(|| v["file_path"].as_str())
        .unwrap_or(name);
    let normalized = if cfg!(windows) {
        identity.replace('\\', "/").to_lowercase()
    } else {
        identity.into()
    };
    let output = project_with_identity(
        &source,
        name,
        &normalized,
        v["content"].as_str().unwrap_or(""),
        v["base_revision"]
            .as_str()
            .ok_or_else(|| ApiError::new(422, INVALID))?,
        &labels,
    )
    .map_err(|code| {
        ApiError::new(
            match code {
                "transaction_conflict" => 409,
                "cancelled" => 499,
                _ => 422,
            },
            code,
        )
    })?;
    if crate::speech::cancelled() {
        return Err(ApiError::new(499, "cancelled"));
    }
    ok_json(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> String {
        r##"{"openapi":"3.1.1","info":{"title":"研究 API","version":"1"},"security":[{"token":[]}],"paths":{"/notes/{id}":{"parameters":[{"name":"id","in":"path","required":true,"schema":{"type":"string"}}],"get":{"summary":"Read","parameters":[{"$ref":"#/components/parameters/query"}],"responses":{"200":{"$ref":"#/components/responses/ok"}}},"post":{"security":[],"responses":{"204":{"description":"Saved"}}}}},"components":{"parameters":{"query":{"name":"expand","in":"query","schema":{"type":"boolean"}}},"responses":{"ok":{"description":"Success","content":{"application/json":{"schema":{"$ref":"#/components/schemas/Note"}}}}},"schemas":{"Note":{"type":"object","properties":{"text":{"type":"string"}}}}}}"##.into()
    }
    fn run(source: &str, content: &str) -> Value {
        project(
            source,
            "api.json",
            content,
            &document_transaction::revision(content),
            &json!({}),
        )
        .unwrap()
    }
    #[test]
    fn repeated_update_preserves_prose_and_is_idempotent() {
        let source = fixture();
        let first = run(&source, "# 我的说明\n\n😀 notes");
        let original = first["result"]["content"].as_str().unwrap();
        let prose = format!("{original}\n\n## 自己的笔记\nNever replace");
        let second = run(&source, &prose);
        assert_eq!(second["result"]["content"], prose);
        assert_eq!(second["changes"], json!([]));
        let changed = run(&source.replace("\"Read\"", "\"Read revised\""), &prose);
        assert_eq!(changed["changes"].as_array().unwrap().len(), 1);
        assert!(changed["result"]["content"]
            .as_str()
            .unwrap()
            .ends_with("## 自己的笔记\nNever replace"));
        let edit = changed["transaction"]["edits"][0]["from"].as_u64().unwrap() as usize;
        assert!(document_transaction::byte_offset(&prose, edit).is_ok());
    }
    #[test]
    fn local_changes_and_stale_base_cannot_be_overwritten() {
        let source = fixture();
        let original = run(&source, "")["result"]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        let edited = original.replace("Read", "Manual change");
        assert_eq!(
            project(
                &source,
                "api.json",
                &edited,
                &document_transaction::revision(&edited),
                &json!({})
            )
            .unwrap_err(),
            "openapi_local_changes"
        );
        assert_eq!(
            project(&source, "api.json", &original, "stale", &json!({})).unwrap_err(),
            "transaction_conflict"
        );
        let quoted = format!("``````````md\n{original}\n``````````\n");
        assert!(run(&source, &quoted)["result"]["content"]
            .as_str()
            .unwrap()
            .starts_with(&quoted));
        let crlf = original.replace('\n', "\r\n");
        let updated = run(&source, &crlf);
        assert_eq!(updated["result"]["content"], crlf);
    }
    #[test]
    fn inherited_parameters_references_and_public_security() {
        let result = run(&fixture(), "");
        let content = result["result"]["content"].as_str().unwrap();
        assert!(content.contains("expand"));
        assert!(content.contains("Success"));
        assert!(content.contains("```json\n[]"));
        assert_eq!(result["source_map"].as_array().unwrap().len(), 2);
        for map in result["source_map"].as_array().unwrap() {
            let from =
                document_transaction::byte_offset(content, map["from"].as_u64().unwrap() as usize)
                    .unwrap();
            assert!(content[from..].starts_with("## "));
        }
        let result = run(
            &fixture().replace(
                "#/components/parameters/query",
                "https://example.test/schema",
            ),
            "",
        );
        assert_eq!(result["warnings"][0]["code"], "external_reference");
    }
    #[test]
    fn yaml_30_31_aliases_duplicates_tags_and_limits() {
        let source="openapi: 3.0.4\ninfo: {title: Demo, version: '1'}\npaths:\n  /x:\n    get:\n      responses:\n        200: &response {description: Works}\n        201: *response\n";
        let value = parse(source).unwrap();
        assert_eq!(
            value["paths"]["/x"]["get"]["responses"]["201"]["description"],
            "Works"
        );
        assert!(parse("openapi: 3.1.1\nopenapi: 3.0.4").is_err());
        assert!(parse("x: !!python/object hi").is_err());
        assert!(parse(&format!("{}x{}", "[".repeat(100), "]".repeat(100))).is_err());
        assert!(parse("---\na: 1\n---\nb: 2").is_err());
        let mut bomb = String::from("a: &a [abcdefghij,abcdefghij,abcdefghij,abcdefghij]\n");
        let mut prior = 'a';
        for next in 'b'..='m' {
            bomb.push_str(&format!(
                "{next}: &{next} [*{prior},*{prior},*{prior},*{prior}]\n"
            ));
            prior = next;
        }
        assert_eq!(parse(&bomb).unwrap_err(), BUDGET);
    }
    #[test]
    fn cancellation_uses_shared_task_context() {
        crate::speech::with_cancel(Arc::new(|| true), || {
            assert_eq!(parse(&fixture()).unwrap_err(), "cancelled")
        });
        assert!(parse(&fixture()).is_ok());
    }
}
