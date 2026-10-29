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
    io::Read,
    path::{Component, Path, PathBuf},
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
    let value = parse_value(source)?;
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

fn parse_value(source: &str) -> Result<Value> {
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
    let value = root.ok_or(INVALID)?;
    if !stack.is_empty() || documents != 1 {
        return Err(INVALID);
    }
    Ok(value)
}

/// All bytes come from a retained, already-verified handle. Size/mtime changes
/// during the read invalidate the snapshot rather than returning mixed bytes.
fn read_source(file: &fs::File) -> Result<String> {
    let before = file.metadata().map_err(|_| INVALID)?;
    if !before.is_file() || source_reparse(&before) {
        return Err("transaction_conflict");
    }
    if before.len() > MAX_SOURCE as u64 {
        return Err(BUDGET);
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| INVALID)?;
    if bytes.len() > MAX_SOURCE {
        return Err(BUDGET);
    }
    let after = file.metadata().map_err(|_| "transaction_conflict")?;
    if before.len() != after.len()
        || bytes.len() as u64 != before.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err("transaction_conflict");
    }
    String::from_utf8(bytes).map_err(|_| INVALID)
}

fn source_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

struct OpenedSource {
    file: fs::File,
    _ancestors: Vec<fs::File>,
}
struct SourceReader {
    directory: PathBuf,
    opened: OpenedSource,
}
impl SourceReader {
    fn new(directory: &Path) -> Result<Self> {
        let opened = open_source_absolute(directory)?;
        if !opened.file.metadata().map_err(|_| INVALID)?.is_dir() {
            return Err(INVALID);
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            opened,
        })
    }
    fn open(&self, path: &Path) -> Result<OpenedSource> {
        if !crate::paths::path_starts_with(path, &self.directory) {
            return Err(INVALID);
        }
        let relative = path
            .components()
            .skip(self.directory.components().count())
            .collect::<PathBuf>();
        if relative.as_os_str().is_empty() {
            return Err(INVALID);
        }
        #[cfg(unix)]
        {
            open_source_relative(&self.opened.file, &relative)
        }
        #[cfg(windows)]
        {
            // `opened` pins the original root and every ancestor with write and
            // delete sharing denied. Each remaining component is independently
            // opened as a reparse point and checked before descending.
            let _root = &self.opened;
            open_source_absolute(&self.directory.join(relative))
        }
        #[cfg(not(any(windows, unix)))]
        {
            Err(INVALID)
        }
    }
    fn read(&self, path: &Path) -> Result<String> {
        let opened = self.open(path)?;
        read_source(&opened.file)
    }
}

#[cfg(windows)]
fn open_source_absolute(path: &Path) -> Result<OpenedSource> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    };
    if !path.is_absolute() {
        return Err(INVALID);
    }
    let mut current = PathBuf::new();
    let mut ancestors = Vec::new();
    let parts = path.components().collect::<Vec<_>>();
    let open = |path: &Path| {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .map_err(|_| INVALID)
    };
    for (index, part) in parts.iter().enumerate() {
        match part {
            Component::Prefix(_) | Component::RootDir => current.push(part.as_os_str()),
            Component::Normal(_) => {
                current.push(part.as_os_str());
                let file = open(&current)?;
                let metadata = file.metadata().map_err(|_| INVALID)?;
                if source_reparse(&metadata) || index + 1 < parts.len() && !metadata.is_dir() {
                    return Err("transaction_conflict");
                }
                if index + 1 == parts.len() {
                    return Ok(OpenedSource {
                        file,
                        _ancestors: ancestors,
                    });
                }
                ancestors.push(file);
            }
            _ => return Err(INVALID),
        }
    }
    let file = open(&current)?;
    if source_reparse(&file.metadata().map_err(|_| INVALID)?) {
        return Err("transaction_conflict");
    }
    Ok(OpenedSource {
        file,
        _ancestors: ancestors,
    })
}

#[cfg(unix)]
fn open_source_absolute(path: &Path) -> Result<OpenedSource> {
    use std::os::fd::FromRawFd;
    if !path.is_absolute() {
        return Err(INVALID);
    }
    let root = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if root < 0 {
        return Err(INVALID);
    }
    let root = unsafe { fs::File::from_raw_fd(root) };
    let relative = path.strip_prefix("/").map_err(|_| INVALID)?;
    if relative.as_os_str().is_empty() {
        return Ok(OpenedSource {
            file: root,
            _ancestors: vec![],
        });
    }
    open_source_relative(&root, relative)
}

#[cfg(unix)]
fn open_source_relative(root: &fs::File, path: &Path) -> Result<OpenedSource> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    let mut current = root.try_clone().map_err(|_| INVALID)?;
    let mut ancestors = Vec::new();
    let parts = path.components().collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(INVALID);
    }
    for (index, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            return Err(INVALID);
        };
        if !current.metadata().map_err(|_| INVALID)?.is_dir() {
            return Err(INVALID);
        }
        let name = CString::new(name.as_bytes()).map_err(|_| INVALID)?;
        let descriptor = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            let code = std::io::Error::last_os_error().raw_os_error();
            return Err(if matches!(code, Some(libc::ELOOP | libc::ENOTDIR)) {
                "transaction_conflict"
            } else {
                INVALID
            });
        }
        let next = unsafe { fs::File::from_raw_fd(descriptor) };
        let metadata = next.metadata().map_err(|_| INVALID)?;
        if source_reparse(&metadata) || index + 1 < parts.len() && !metadata.is_dir() {
            return Err("transaction_conflict");
        }
        ancestors.push(current);
        current = next;
    }
    Ok(OpenedSource {
        file: current,
        _ancestors: ancestors,
    })
}

#[cfg(not(any(windows, unix)))]
fn open_source_absolute(_path: &Path) -> Result<OpenedSource> {
    Err(INVALID)
}

struct SourceSnapshot {
    document: Value,
    revision: String,
    warnings: Vec<Value>,
    references: Vec<Value>,
    reference_map: Vec<Value>,
}

/// A vocabulary-aware walk keeps literal `$ref` values in examples and
/// extensions opaque. Only schema children carry JSON Schema semantics.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Document,
    Components,
    PathItems,
    PathItem,
    Operation,
    Parameters,
    Parameter,
    RequestBodies,
    RequestBody,
    Responses,
    Response,
    Content,
    MediaType,
    Schemas,
    Schema,
    SchemaArray,
    Headers,
    Header,
    Examples,
    Example,
    Callbacks,
    Callback,
    SecuritySchemes,
    SecurityScheme,
    Links,
    Link,
    Encodings,
    Encoding,
    Opaque,
}
impl NodeKind {
    fn reference(self) -> bool {
        matches!(
            self,
            Self::PathItem
                | Self::Parameter
                | Self::RequestBody
                | Self::Response
                | Self::Header
                | Self::Example
                | Self::Callback
                | Self::SecurityScheme
                | Self::Link
                | Self::Schema
        )
    }
    fn child(self, key: &str) -> Self {
        use NodeKind::*;
        if key.starts_with("x-")
            && !matches!(
                self,
                Schemas
                    | Parameters
                    | RequestBodies
                    | Headers
                    | Examples
                    | Callbacks
                    | SecuritySchemes
                    | Links
                    | Content
                    | Encodings
            )
        {
            return Opaque;
        }
        match (self, key) {
            (Document, "paths" | "webhooks") => PathItems,
            (Document, "components") => Components,
            (Components, "schemas") => Schemas,
            (Components, "parameters") => Parameters,
            (Components, "responses") => Responses,
            (Components, "requestBodies") => RequestBodies,
            (Components, "headers") => Headers,
            (Components, "examples") => Examples,
            (Components, "callbacks") => Callbacks,
            (Components, "securitySchemes") => SecuritySchemes,
            (Components, "links") => Links,
            (Components, "pathItems") => PathItems,
            (PathItems, _) => PathItem,
            (
                PathItem,
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace",
            ) => Operation,
            (PathItem | Operation, "parameters") => Parameters,
            (Operation, "requestBody") => RequestBody,
            (Operation, "responses") => Responses,
            (Operation, "callbacks") => Callbacks,
            (Parameters, _) => Parameter,
            (RequestBodies, _) => RequestBody,
            (Responses, _) => Response,
            (Parameter | Header | MediaType, "schema") => Schema,
            (Parameter | Header | RequestBody | Response, "content") => Content,
            (Parameter | Header | MediaType, "examples") => Examples,
            (Content, _) => MediaType,
            (MediaType, "encoding") => Encodings,
            (Encodings, _) => Encoding,
            (Encoding | Response, "headers") => Headers,
            (Response, "links") => Links,
            (Headers, _) => Header,
            (Examples, _) => Example,
            (Callbacks, _) => Callback,
            (Callback, _) => PathItem,
            (SecuritySchemes, _) => SecurityScheme,
            (Links, _) => Link,
            (Schemas, _) | (SchemaArray, _) => Schema,
            (
                Schema,
                "properties" | "patternProperties" | "$defs" | "definitions" | "dependentSchemas",
            ) => Schemas,
            (
                Schema,
                "items"
                | "additionalProperties"
                | "not"
                | "contains"
                | "propertyNames"
                | "if"
                | "then"
                | "else"
                | "unevaluatedProperties"
                | "unevaluatedItems"
                | "contentSchema",
            ) => Schema,
            (Schema, "allOf" | "anyOf" | "oneOf" | "prefixItems") => SchemaArray,
            _ => Opaque,
        }
    }
}

struct LocalReferences {
    root: PathBuf,
    directory: PathBuf,
    reader: SourceReader,
    documents: HashMap<PathBuf, Option<Arc<Value>>>,
    revisions: BTreeMap<String, String>,
    warnings: Vec<Value>,
    reference_map: Vec<Value>,
    stack: HashSet<(PathBuf, String)>,
    version31: bool,
    input_bytes: usize,
    nodes: usize,
    output_bytes: usize,
}

/// URI decoding is strict and performed exactly once before filesystem access.
fn uri_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            if at + 2 >= bytes.len() || !bytes[at + 1..at + 3].iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            at += 3;
        } else {
            at += 1;
        }
    }
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()
        .map(|s| s.into_owned())
}

impl LocalReferences {
    #[cfg(test)]
    fn snapshot(path: &Path, source: &str) -> Result<SourceSnapshot> {
        let root = crate::paths::strip_verbatim(fs::canonicalize(path).map_err(|_| INVALID)?);
        let reader = SourceReader::new(root.parent().ok_or(INVALID)?)?;
        Self::new(root, source, reader)?.complete(source)
    }
    fn file_snapshot(path: &Path) -> Result<SourceSnapshot> {
        let root = crate::paths::strip_verbatim(fs::canonicalize(path).map_err(|_| INVALID)?);
        let reader = SourceReader::new(root.parent().ok_or(INVALID)?)?;
        let source = reader.read(&root)?;
        Self::new(root, &source, reader)?.complete(&source)
    }
    fn new(root: PathBuf, source: &str, reader: SourceReader) -> Result<Self> {
        let document = parse(source)?;
        Ok(Self {
            version31: document["openapi"]
                .as_str()
                .unwrap_or("")
                .starts_with("3.1."),
            directory: reader.directory.clone(),
            reader,
            root: root.clone(),
            documents: HashMap::from([(root.clone(), Some(Arc::new(document)))]),
            revisions: BTreeMap::new(),
            warnings: vec![],
            reference_map: vec![],
            stack: HashSet::new(),
            input_bytes: source.len(),
            nodes: 0,
            output_bytes: 0,
        })
    }
    fn complete(mut self, source: &str) -> Result<SourceSnapshot> {
        let root = self.root.clone();
        let document = self
            .documents
            .get(&root)
            .and_then(Option::as_ref)
            .cloned()
            .ok_or(INVALID)?;
        let document = self.walk(&document, NodeKind::Document, &root, "", "", 0)?;
        let references: Vec<Value> = self
            .revisions
            .iter()
            .map(|(name, digest)| json!({"source":name,"revision":digest}))
            .collect();
        let root_revision = document_transaction::revision(source);
        let revision = if references.is_empty() {
            root_revision
        } else {
            document_transaction::revision(
                &serde_json::to_string(&json!({
                    "root":root_revision,"references":references
                }))
                .map_err(|_| INVALID)?,
            )
        };
        Ok(SourceSnapshot {
            document,
            revision,
            warnings: self.warnings,
            references,
            reference_map: self.reference_map,
        })
    }
    fn name(&self, path: &Path) -> String {
        // Containment is checked before this function; component counting also
        // works when Windows canonical paths differ only in letter case.
        path.components()
            .skip(self.directory.components().count())
            .collect::<PathBuf>()
            .to_string_lossy()
            .replace('\\', "/")
    }
    fn warning(&mut self, code: &str, location: &str) {
        if self.warnings.len() < 256 {
            let warning = json!({"code":code,"pointer":location});
            if !self.warnings.contains(&warning) {
                self.warnings.push(warning);
            }
        }
    }
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.output_bytes = self.output_bytes.saturating_add(bytes);
        if self.output_bytes > MAX_OUTPUT {
            return Err(BUDGET);
        }
        Ok(())
    }
    fn target(&self, source: &Path, raw: &str) -> Option<(PathBuf, String)> {
        let (file, fragment) = raw.split_once('#').unwrap_or((raw, ""));
        let fragment = uri_decode(fragment)?;
        if !fragment.is_empty() && !fragment.starts_with('/') {
            return None;
        }
        for token in fragment.split('/') {
            let mut chars = token.chars();
            while let Some(ch) = chars.next() {
                if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
                    return None;
                }
            }
        }
        let file = uri_decode(file)?;
        if file.is_empty() {
            return Some((source.to_path_buf(), fragment));
        }
        if file.starts_with('/')
            || file.contains(['\\', ':', '?', '#'])
            || file.chars().any(char::is_control)
        {
            return None;
        }
        let joined = source.parent()?.join(file);
        let target = crate::paths::canonical_existing(&joined).ok()?;
        if !crate::paths::path_starts_with(&target, &self.directory) {
            return None;
        }
        let extension = target.extension()?.to_str()?.to_ascii_lowercase();
        if !matches!(extension.as_str(), "json" | "yaml" | "yml") {
            return None;
        }
        Some((target, fragment))
    }
    fn load(&mut self, target: &Path) -> Result<Option<Arc<Value>>> {
        if let Some(document) = self.documents.get(target) {
            return Ok(document.clone());
        }
        // The root document does not consume a dependency slot.
        if self.documents.len() > 64 {
            return Err(BUDGET);
        }
        let name = self.name(target);
        let document = match self.reader.read(target) {
            Ok(source) => {
                self.input_bytes = self.input_bytes.saturating_add(source.len());
                if self.input_bytes > MAX_OUTPUT {
                    return Err(BUDGET);
                }
                self.revisions
                    .insert(name, document_transaction::revision(&source));
                match parse_value(&source) {
                    Ok(value) => Some(Arc::new(value)),
                    Err(code @ (BUDGET | "cancelled")) => return Err(code),
                    Err(_) => None,
                }
            }
            Err(code @ (BUDGET | "transaction_conflict")) => return Err(code),
            Err(_) => {
                self.revisions.insert(name, "missing".into());
                None
            }
        };
        self.documents
            .insert(target.to_path_buf(), document.clone());
        Ok(document)
    }
    fn unresolved(&self, value: &Value, source: &Path) -> Value {
        let mut value = value.clone();
        if source != self.root {
            if let Some(raw) = value["$ref"].as_str() {
                if let Some((target, fragment)) = self.target(source, raw) {
                    value["$ref"] = json!(format!("{}#{fragment}", self.name(&target)));
                }
            }
        }
        value
    }
    fn walk(
        &mut self,
        value: &Value,
        kind: NodeKind,
        source: &Path,
        location: &str,
        source_pointer: &str,
        depth: usize,
    ) -> Result<Value> {
        self.nodes += 1;
        if self.nodes % 1024 == 0 && crate::speech::cancelled() {
            return Err("cancelled");
        }
        if depth > 64 || self.nodes > 200_000 {
            return Err(BUDGET);
        }
        self.charge(2)?;
        if kind == NodeKind::Schema && value.get("$id").is_some() {
            // Moving a resource with its own base URI requires a resource
            // registry; do not silently resolve against a filesystem directory.
            if source != self.root || schema_has_external_reference(value) {
                self.warning("external_reference", location);
            }
            self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
            return Ok(self.unresolved(value, source));
        }
        if let Some(raw) = value
            .get("$ref")
            .and_then(Value::as_str)
            .filter(|_| kind.reference())
        {
            // Root-local references keep their established projection and
            // generated-region format. Their component children are walked too.
            if !raw.starts_with('#') || source != self.root {
                let Some((target, fragment)) = self.target(source, raw) else {
                    self.warning("external_reference", location);
                    self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
                    return Ok(self.unresolved(value, source));
                };
                let key = (target.clone(), fragment.clone());
                if self.stack.len() >= 32 {
                    return Err(BUDGET);
                }
                if !self.stack.insert(key.clone()) {
                    self.warning("recursive_reference", location);
                    self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
                    return Ok(self.unresolved(value, source));
                }
                let document = self.load(&target)?;
                let scoped = kind == NodeKind::Schema
                    && document.as_ref().is_some_and(|doc| {
                        fragment.match_indices('/').any(|(end, _)| {
                            doc.pointer(&fragment[..end])
                                .is_some_and(|parent| parent.get("$id").is_some())
                        })
                    });
                let found = document.and_then(|doc| doc.pointer(&fragment).cloned());
                let result = if let Some(found) = found {
                    if scoped || kind == NodeKind::Schema && schema_has_uri_scope(&found) {
                        self.warning("external_reference", location);
                        self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
                        self.unresolved(value, source)
                    } else {
                        let mapping_start = self.reference_map.len();
                        let mut resolved =
                            self.walk(&found, kind, &target, location, &fragment, depth + 1)?;
                        let mut siblings = value.as_object().cloned().unwrap_or_default();
                        siblings.remove("$ref");
                        if kind == NodeKind::Schema && self.version31 && !siblings.is_empty() {
                            let mut siblings = self.walk(
                                &Value::Object(siblings),
                                kind,
                                source,
                                location,
                                source_pointer,
                                depth + 1,
                            )?;
                            // Keep sibling keywords at the original level so
                            // unevaluatedProperties/Items see the annotations
                            // of the referenced schema through allOf.
                            if let Some(all_of) = siblings.get_mut("allOf") {
                                if let Some(array) = all_of.as_array_mut() {
                                    array.insert(0, resolved);
                                    resolved = siblings;
                                } else {
                                    self.warning("external_reference", location);
                                    resolved = self.unresolved(value, source);
                                }
                            } else {
                                siblings["allOf"] = json!([resolved]);
                                resolved = siblings;
                            }
                        } else if kind == NodeKind::PathItem {
                            // Distinct local Path Item fields are preserved.
                            // Conflicting operations are undefined by OAS: keep
                            // the original reference and report the boundary.
                            if siblings.keys().any(|key| {
                                !matches!(key.as_str(), "summary" | "description")
                                    && resolved.get(key).is_some()
                            }) {
                                self.warning("external_reference", location);
                                self.reference_map.truncate(mapping_start);
                                self.stack.remove(&key);
                                self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
                                return Ok(self.unresolved(value, source));
                            } else if let Some(object) = resolved.as_object_mut() {
                                for (key, child) in siblings {
                                    let child_location = format!("{location}/{}", pointer(&key));
                                    let child_pointer =
                                        format!("{source_pointer}/{}", pointer(&key));
                                    let child = self.walk(
                                        &child,
                                        kind.child(&key),
                                        source,
                                        &child_location,
                                        &child_pointer,
                                        depth + 1,
                                    )?;
                                    if kind.child(&key) == NodeKind::Operation
                                        && self.reference_map.len() < 4096
                                    {
                                        self.reference_map.push(json!({"pointer":child_location,"source":self.name(source),"target":child_pointer}));
                                    }
                                    object.insert(key, child);
                                }
                            }
                        } else if self.version31 {
                            if let Some(object) = resolved.as_object_mut() {
                                for key in ["summary", "description"] {
                                    if let Some(child) = siblings.get(key) {
                                        let child = self.walk(
                                            child,
                                            NodeKind::Opaque,
                                            source,
                                            &format!("{location}/{key}"),
                                            &format!("{source_pointer}/{key}"),
                                            depth + 1,
                                        )?;
                                        object.insert(key.into(), child);
                                    }
                                }
                            }
                        }
                        if self.reference_map.len() < 4096
                            && !self
                                .reference_map
                                .iter()
                                .any(|origin| origin["pointer"] == location)
                        {
                            self.reference_map.push(json!({"pointer":location,"source":self.name(&target),"target":fragment}));
                        }
                        resolved
                    }
                } else {
                    self.warning("missing_reference", location);
                    self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
                    self.unresolved(value, source)
                };
                self.stack.remove(&key);
                return Ok(result);
            }
        }
        match value {
            Value::Object(object) => {
                let mut result = Map::new();
                for (key, child) in object {
                    self.charge(serde_json::to_string(key).map_err(|_| INVALID)?.len() + 2)?;
                    let child = self.walk(
                        child,
                        kind.child(key),
                        source,
                        &format!("{location}/{}", pointer(key)),
                        &format!("{source_pointer}/{}", pointer(key)),
                        depth + 1,
                    )?;
                    result.insert(key.clone(), child);
                }
                Ok(Value::Object(result))
            }
            Value::Array(array) => array
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    self.walk(
                        child,
                        kind.child(""),
                        source,
                        &format!("{location}/{index}"),
                        &format!("{source_pointer}/{index}"),
                        depth + 1,
                    )
                })
                .collect::<Result<Vec<_>>>()
                .map(Value::Array),
            _ => {
                self.charge(serde_json::to_vec(value).map_err(|_| INVALID)?.len())?;
                Ok(value.clone())
            }
        }
    }
}

fn schema_has_uri_scope(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if ["$id", "$anchor", "$dynamicAnchor", "$dynamicRef"]
        .iter()
        .any(|key| object.contains_key(*key))
    {
        return true;
    }
    object
        .iter()
        .any(|(key, child)| match NodeKind::Schema.child(key) {
            NodeKind::Schema => schema_has_uri_scope(child),
            NodeKind::Schemas => child
                .as_object()
                .is_some_and(|map| map.values().any(schema_has_uri_scope)),
            NodeKind::SchemaArray => child
                .as_array()
                .is_some_and(|array| array.iter().any(schema_has_uri_scope)),
            _ => false,
        })
}

fn schema_has_external_reference(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object
        .get("$ref")
        .and_then(Value::as_str)
        .is_some_and(|reference| !reference.starts_with('#'))
    {
        return true;
    }
    object
        .iter()
        .any(|(key, child)| match NodeKind::Schema.child(key) {
            NodeKind::Schema => schema_has_external_reference(child),
            NodeKind::Schemas => child
                .as_object()
                .is_some_and(|map| map.values().any(schema_has_external_reference)),
            NodeKind::SchemaArray => child
                .as_array()
                .is_some_and(|array| array.iter().any(schema_has_external_reference)),
            _ => false,
        })
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
    project_snapshot(
        &SourceSnapshot {
            document: parse(source)?,
            revision: document_transaction::revision(source),
            warnings: vec![],
            references: vec![],
            reference_map: vec![],
        },
        source_name,
        identity,
        content,
        base_revision,
        labels,
    )
}

#[cfg(test)]
#[path = "openapi_local_refs_tests.rs"]
mod local_refs_tests;
fn project_snapshot(
    snapshot: &SourceSnapshot,
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
    let mut projection = Projection {
        root: &snapshot.document,
        labels,
        warnings: snapshot.warnings.clone(),
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
    let mut metadata = json!({"schema":1,"id":id,"source":source_name,"source_sha256":snapshot.revision,"body_sha256":document_transaction::revision(&body),"operations":projection.operations});
    if !snapshot.references.is_empty() {
        metadata["references"] = json!(snapshot.references);
    }
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
        if let Some(location) = entry["pointer"].as_str() {
            // Keep the original root call site for existing consumers and add
            // the actual definition location for cross-file Path Items.
            if let Some(origin) = snapshot
                .reference_map
                .iter()
                .filter(|origin| {
                    let pointer = origin["pointer"].as_str().unwrap_or("");
                    !pointer.is_empty()
                        && (location == pointer
                            || location
                                .strip_prefix(pointer)
                                .is_some_and(|tail| tail.starts_with('/')))
                })
                .max_by_key(|origin| origin["pointer"].as_str().unwrap_or("").len())
            {
                let pointer = origin["pointer"].as_str().unwrap();
                entry["definition"] = json!({"source":origin["source"],"pointer":format!("{}{}",origin["target"].as_str().unwrap_or(""),&location[pointer.len()..])});
            }
        }
    }
    Ok(
        json!({"ok":true,"result":result,"transaction":tx,"source_map":projection.maps,"changes":changes,"warnings":projection.warnings,"source_name":source_name,"source_revision":metadata["source_sha256"],"references":snapshot.references,"reference_map":snapshot.reference_map}),
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
    let snapshot = if let Some(file) = v["file_path"].as_str() {
        let path = app.paths.resolve_doc(file)?;
        if fs::metadata(&path)
            .map_err(|_| ApiError::new(404, "file_not_found"))?
            .len()
            > MAX_SOURCE as u64
        {
            return Err(ApiError::new(422, BUDGET));
        }
        LocalReferences::file_snapshot(&path)
    } else {
        let source = v["source"]
            .as_str()
            .ok_or_else(|| ApiError::new(422, INVALID))?;
        parse(source).map(|document| SourceSnapshot {
            document,
            revision: document_transaction::revision(source),
            warnings: vec![],
            references: vec![],
            reference_map: vec![],
        })
    }
    .map_err(|code| {
        ApiError::new(
            match code {
                "cancelled" => 499,
                "transaction_conflict" => 409,
                _ => 422,
            },
            code,
        )
    })?;
    if v["expected_source_revision"]
        .as_str()
        .is_some_and(|expected| expected != snapshot.revision)
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
    let output = project_snapshot(
        &snapshot,
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
