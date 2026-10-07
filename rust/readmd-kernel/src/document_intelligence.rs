//! Shared, bounded document inspection and local workspace operations.
//! No network, third-party runtime, hidden document copies, or UI dependency.
use crate::{content, App};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILES: usize = 5000;
const MAX_SCAN_BYTES: usize = 64 * 1024 * 1024;
const MAX_ITEMS: usize = 2000;

pub fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn cancelled(cancel: Option<&AtomicBool>) -> Result<(), String> {
    if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}
fn absolute_file(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("file_path_must_be_absolute".into());
    }
    if fs::symlink_metadata(path)
        .map_err(|_| "file_not_found")?
        .file_type()
        .is_symlink()
    {
        return Err("symlink_not_allowed".into());
    }
    let canonical = fs::canonicalize(path).map_err(|_| "file_not_found")?;
    if !canonical.is_file() {
        return Err("file_not_found".into());
    }
    if !content::is_readable(&canonical) {
        return Err("unsupported_text_file".into());
    }
    Ok(crate::paths::canonicalize_or_clean(&canonical))
}
fn bytes_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|_| "file_read_failed")?;
    if file.metadata().map_err(|_| "file_read_failed")?.len() > MAX_DOCUMENT_BYTES as u64 {
        return Err("document_too_large".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file_read_failed")?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("document_too_large".into());
    }
    Ok(bytes)
}
fn markdown(path: &Path) -> bool {
    matches!(
        content::ext_of(path).as_str(),
        "md" | "markdown" | "mdown" | "mkd" | "mdx" | "txt"
    )
}
fn visible(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !name.starts_with('.')
        && !matches!(
            name.as_ref(),
            "node_modules" | "target" | "dist" | "build" | "vendor" | "venv" | "__pycache__"
        )
}
fn workspace(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err("workspace_must_be_absolute".into());
    }
    let root = fs::canonicalize(root).map_err(|_| "workspace_not_found")?;
    if !root.is_dir() {
        return Err("workspace_not_found".into());
    }
    Ok(crate::paths::canonicalize_or_clean(&root))
}
fn local_path(base: &Path, raw: &str, root: &Path) -> Option<PathBuf> {
    if raw.contains('\0') || raw.starts_with("//") || raw.starts_with("\\\\") {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()?;
    if decoded.contains('\0') || decoded.contains('\\') || decoded.contains(':') {
        return None;
    }
    let (start, relative) = if decoded.starts_with('/') {
        (root, decoded.trim_start_matches('/'))
    } else {
        (base, decoded.as_ref())
    };
    let mut candidate = start.to_path_buf();
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(p) => candidate.push(p),
            Component::CurDir => (),
            Component::ParentDir => {
                candidate.pop();
            }
            _ => return None,
        }
    }
    if !candidate.starts_with(root) {
        return None;
    }
    let resolved = crate::paths::canonical_existing(&candidate).ok()?;
    let resolved = crate::paths::canonicalize_or_clean(&resolved);
    if resolved.starts_with(root) {
        Some(resolved)
    } else {
        None
    }
}
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(n, _)| n + 1))
        .collect()
}
fn position(text: &str, starts: &[usize], offset: usize) -> Value {
    let offset = offset.min(text.len());
    let row = starts.partition_point(|n| *n <= offset).saturating_sub(1);
    json!({"line": row + 1, "column": text[starts[row]..offset].encode_utf16().count() + 1})
}
#[derive(Clone, Copy)]
enum AnchorStyle {
    Standard,
    Desktop,
}
fn heading_slug(title: &str, index: usize, style: AnchorStyle) -> String {
    let filtered: String = title
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| {
            c.is_whitespace()
                || *c == '_'
                || *c == '-'
                || match style {
                    AnchorStyle::Standard => c.is_alphanumeric(),
                    AnchorStyle::Desktop => {
                        c.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c)
                    }
                }
        })
        .collect();
    match style {
        AnchorStyle::Standard => filtered
            .chars()
            .map(|c| if c.is_whitespace() { '-' } else { c })
            .collect(),
        AnchorStyle::Desktop => {
            let slug = filtered.split_whitespace().collect::<Vec<_>>().join("-");
            if slug.is_empty() {
                format!("toc-h-{index}")
            } else {
                slug
            }
        }
    }
}
fn heading_data(text: &str, style: AnchorStyle) -> Vec<Value> {
    let starts = line_starts(text);
    let mut current: Option<(u8, usize, String, Option<String>)> = None;
    let mut headings = Vec::new();
    let mut used: HashMap<String, usize> = HashMap::new();
    for (event, span) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, id, .. }) => {
                current = Some((
                    level as u8,
                    span.start,
                    String::new(),
                    id.map(|s| s.to_string()),
                ))
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(h) = current.as_mut() {
                    h.2.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(h) = current.as_mut() {
                    h.2.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, offset, title, explicit)) = current.take() {
                    let base =
                        explicit.unwrap_or_else(|| heading_slug(&title, headings.len(), style));
                    let count = used.entry(base.clone()).or_insert(0);
                    let suffix = *count
                        + if matches!(style, AnchorStyle::Desktop) {
                            1
                        } else {
                            0
                        };
                    let id = if *count == 0 {
                        base
                    } else {
                        format!("{base}-{suffix}")
                    };
                    *count += 1;
                    headings.push(json!({"level":level,"text":title,"id":id,"position":position(text,&starts,offset)}));
                    if headings.len() >= MAX_ITEMS {
                        break;
                    }
                }
            }
            _ => (),
        }
    }
    headings
}

// Keep the legacy conversion parser unchanged. Accept the common YAML tags
// list without evaluating YAML directives, aliases or custom types.
fn metadata(text: &str) -> Value {
    let mut result = content::front_matter(text);
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next() != Some("---") {
        return result;
    }
    let mut tags = Vec::new();
    let mut in_tags = false;
    let mut size = 0;
    for line in lines {
        size += line.len();
        if size > 16 * 1024 || line == "---" || line == "..." {
            break;
        }
        if line.trim() == "tags:" {
            in_tags = true;
            continue;
        }
        if in_tags {
            if let Some(tag) = line.trim().strip_prefix("- ") {
                let tag = tag.trim().trim_matches(|c| c == '"' || c == '\'');
                if !tag.is_empty() {
                    tags.push(json!(tag));
                }
            } else if !line.trim().is_empty() {
                in_tags = false;
            }
        }
    }
    if !tags.is_empty() {
        result["tags"] = json!(tags);
    }
    result
}

/// AST-based inspection. Filesystem checking is explicit and stays in root.
pub fn analyze(text: &str, file: Option<&Path>, root: Option<&Path>) -> Result<Value, String> {
    analyze_with_style(text, file, root, AnchorStyle::Standard)
}
pub fn analyze_for_reader(
    text: &str,
    file: Option<&Path>,
    root: Option<&Path>,
) -> Result<Value, String> {
    analyze_with_style(text, file, root, AnchorStyle::Desktop)
}
fn analyze_with_style(
    text: &str,
    file: Option<&Path>,
    root: Option<&Path>,
    style: AnchorStyle,
) -> Result<Value, String> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err("document_too_large".into());
    }
    let owned_root = root.map(workspace).transpose()?;
    let file = file.map(|p| crate::paths::canonicalize_or_clean(p));
    let base = file
        .as_deref()
        .and_then(Path::parent)
        .or(owned_root.as_deref());
    let root = owned_root.as_deref().or(base);
    let starts = line_starts(text);
    let headings = heading_data(text, style);
    let own_ids: HashSet<String> = headings
        .iter()
        .filter_map(|h| h["id"].as_str().map(str::to_string))
        .collect();
    let mut links = Vec::new();
    let mut diagnostics = Vec::new();
    let mut tasks = (0usize, 0usize);
    let mut code_depth = 0usize;
    let mut footnotes = HashSet::new();
    let mut foot_refs = Vec::new();
    let mut target_headings: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();
    let mut truncated = headings.len() == MAX_ITEMS;
    let mut wiki_files: Option<HashMap<String, Vec<PathBuf>>> = None;
    let mut add = |code: &str, offset: usize, target: &str| {
        if diagnostics.len() < 200 {
            diagnostics.push(json!({"code":code,"severity":"warning",
            "target":target,"position":position(text,&starts,offset)}));
        }
    };
    for (event, span) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_depth += 1;
            }
            Event::End(TagEnd::CodeBlock) => {
                code_depth = code_depth.saturating_sub(1);
            }
            Event::TaskListMarker(done) => {
                tasks.0 += 1;
                if done {
                    tasks.1 += 1;
                }
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                footnotes.insert(label.to_string());
            }
            Event::FootnoteReference(label) => foot_refs.push((label.to_string(), span.start)),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            })
            | Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) if code_depth == 0 => {
                if links.len() >= MAX_ITEMS {
                    truncated = true;
                    continue;
                }
                let raw = dest_url.to_string();
                let wiki = matches!(link_type, LinkType::WikiLink { .. });
                let external = raw.starts_with("//")
                    || raw.split_once(':').is_some_and(|(scheme, _)| {
                        !scheme.is_empty()
                            && scheme
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                    });
                let mut link = json!({"target":raw,"kind":if wiki {"wiki"} else {"markdown"},
                    "external":external,"position":position(text,&starts,span.start)});
                if !external {
                    let (path_part, anchor) = raw
                        .split_once('#')
                        .map(|(p, a)| (p, Some(a)))
                        .unwrap_or((&raw, None));
                    let path_part = path_part.split('?').next().unwrap_or(path_part);
                    if path_part.is_empty() {
                        if wiki {
                            if let Some(file) = file.as_ref() {
                                link["resolved_path"] = json!(file);
                            }
                        }
                        if let Some(anchor) = anchor {
                            let anchor =
                                percent_encoding::percent_decode_str(anchor).decode_utf8_lossy();
                            let normalized = heading_slug(&anchor, 0, style);
                            if !anchor.is_empty()
                                && !own_ids.contains(anchor.as_ref())
                                && !(wiki && own_ids.contains(&normalized))
                            {
                                add("missing_anchor", span.start, &raw);
                            } else if wiki {
                                link["resolved_anchor"] =
                                    json!(if own_ids.contains(anchor.as_ref()) {
                                        anchor.into_owned()
                                    } else {
                                        normalized
                                    });
                            }
                        }
                    } else if let (Some(base), Some(root)) = (base, root) {
                        if let Some(mut target) = local_path(base, path_part, root) {
                            if wiki && !target.is_file() && target.extension().is_none() {
                                target.set_extension("md");
                            }
                            if wiki && !target.is_file() && !path_part.contains('/') {
                                let files = wiki_files.get_or_insert_with(|| {
                                    let mut map: HashMap<String, Vec<PathBuf>> = HashMap::new();
                                    for e in walkdir::WalkDir::new(root)
                                        .follow_links(false)
                                        .sort_by_file_name()
                                        .into_iter()
                                        .filter_entry(visible)
                                        .filter_map(Result::ok)
                                        .take(MAX_FILES)
                                    {
                                        if e.file_type().is_file() && markdown(e.path()) {
                                            if let Ok(p) = fs::canonicalize(e.path())
                                                .map(|p| crate::paths::canonicalize_or_clean(&p))
                                            {
                                                if p.starts_with(root) {
                                                    map.entry(
                                                        e.path()
                                                            .file_stem()
                                                            .unwrap_or_default()
                                                            .to_string_lossy()
                                                            .to_lowercase(),
                                                    )
                                                    .or_default()
                                                    .push(p);
                                                }
                                            }
                                        }
                                    }
                                    map
                                });
                                let key = Path::new(path_part)
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_lowercase();
                                if let Some(found) = files.get(&key) {
                                    if found.len() == 1 {
                                        target = found[0].clone();
                                    } else {
                                        add("ambiguous_link", span.start, &raw);
                                        link["ambiguous"] = json!(true);
                                    }
                                }
                            }
                            if target.is_file() {
                                link["resolved_path"] = json!(target);
                                if let Some(anchor) = anchor.filter(|a| !a.is_empty()) {
                                    if markdown(&target)
                                        && (target_headings.contains_key(&target)
                                            || target_headings.len() < 32)
                                    {
                                        let ids = target_headings
                                            .entry(target.clone())
                                            .or_insert_with(|| {
                                                bytes_bounded(&target).ok().map(|b| {
                                                    heading_data(
                                                        &crate::text_encoding::detect_and_decode(
                                                            &b,
                                                        )
                                                        .0,
                                                        style,
                                                    )
                                                    .iter()
                                                    .filter_map(|h| {
                                                        h["id"].as_str().map(str::to_string)
                                                    })
                                                    .collect()
                                                })
                                            });
                                        let anchor = percent_encoding::percent_decode_str(anchor)
                                            .decode_utf8_lossy();
                                        if let Some(ids) = ids {
                                            if !ids.contains(anchor.as_ref())
                                                && !(wiki
                                                    && ids
                                                        .contains(&heading_slug(&anchor, 0, style)))
                                            {
                                                add("missing_anchor", span.start, &raw);
                                            } else if wiki {
                                                link["resolved_anchor"] =
                                                    json!(if ids.contains(anchor.as_ref()) {
                                                        anchor.into_owned()
                                                    } else {
                                                        heading_slug(&anchor, 0, style)
                                                    });
                                            }
                                        } else {
                                            truncated = true;
                                        }
                                    } else if markdown(&target) {
                                        truncated = true;
                                    }
                                }
                            } else if link.get("ambiguous").is_none() {
                                add("missing_file", span.start, &raw);
                            }
                        } else {
                            add("outside_workspace", span.start, &raw);
                        }
                    }
                }
                links.push(link);
            }
            _ => (),
        }
    }
    for (label, offset) in foot_refs {
        if !footnotes.contains(&label) {
            add("missing_footnote", offset, &label);
        }
    }
    Ok(
        json!({"ok":true,"headings":headings,"links":links,"diagnostics":diagnostics,
        "stats":{"words":content::count_words(text),"lines":starts.len(),"tasks":tasks.0,"completed_tasks":tasks.1},
        "metadata":metadata(text),"truncated":truncated || diagnostics.len() == 200,
        "limits":{"document_bytes":MAX_DOCUMENT_BYTES,"links":MAX_ITEMS,"diagnostics":200,"anchor_files":32}}),
    )
}

pub fn read_document(path: &Path, start: usize, end: usize) -> Result<Value, String> {
    if start == 0 || end < start || end - start >= 1000 {
        return Err("invalid_line_range".into());
    }
    let path = absolute_file(path)?;
    let bytes = bytes_bounded(&path)?;
    let (text, encoding) = crate::text_encoding::detect_and_decode(&bytes);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if start > lines.len().max(1) {
        return Err("line_out_of_range".into());
    }
    let eol = if text.contains("\r\n") { "crlf" } else { "lf" };
    let selected = lines
        .iter()
        .skip(start - 1)
        .take(end - start + 1)
        .copied()
        .collect::<String>();
    Ok(
        json!({"ok":true,"path":path,"revision":revision(&bytes),"encoding":encoding,"eol":eol,
        "start_line":start,"end_line":end.min(lines.len().max(1)),"total_lines":lines.len(),
        "content":selected,"truncated":start>1 || end<lines.len()}),
    )
}

#[derive(Debug)]
struct Term {
    field: String,
    value: String,
    exclude: bool,
}
fn query_terms(query: &str) -> Result<Vec<Term>, String> {
    if query.len() > 4096 {
        return Err("query_too_large".into());
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = false;
    for c in query.chars() {
        if c == '"' {
            quote = !quote;
        } else if c.is_whitespace() && !quote {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if quote {
        return Err("unclosed_search_quote".into());
    }
    if !word.is_empty() {
        words.push(word);
    }
    if words.is_empty() || words.len() > 16 {
        return Err("invalid_search_query".into());
    }
    Ok(words
        .into_iter()
        .map(|word| {
            let exclude = word.starts_with('-');
            let word = word.trim_start_matches('-');
            let (field, value) = word
                .split_once(':')
                .filter(|(f, _)| matches!(*f, "path" | "tag" | "title"))
                .unwrap_or(("text", word));
            Term {
                field: field.into(),
                value: value.to_lowercase(),
                exclude,
            }
        })
        .collect())
}
pub fn search_workspace(
    root: &Path,
    query: &str,
    limit: usize,
    cancel: Option<&AtomicBool>,
) -> Result<Value, String> {
    if !(1..=100).contains(&limit) {
        return Err("invalid_search_limit".into());
    }
    let root = workspace(root)?;
    let terms = query_terms(query)?;
    if terms.iter().any(|t| t.value.is_empty()) {
        return Err("invalid_search_query".into());
    }
    let mut hits = Vec::new();
    let mut files = 0;
    let mut scanned = 0;
    let mut skipped = 0;
    let mut truncated = false;
    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(visible)
    {
        cancelled(cancel)?;
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        if !entry.file_type().is_file() || !markdown(entry.path()) {
            continue;
        }
        if files >= MAX_FILES || scanned >= MAX_SCAN_BYTES {
            truncated = true;
            break;
        }
        files += 1;
        let Ok(path) =
            fs::canonicalize(entry.path()).map(|p| crate::paths::canonicalize_or_clean(&p))
        else {
            skipped += 1;
            continue;
        };
        if !path.starts_with(&root) {
            skipped += 1;
            continue;
        }
        let Ok(bytes) = bytes_bounded(&path) else {
            skipped += 1;
            continue;
        };
        if bytes.len() > MAX_SCAN_BYTES - scanned {
            truncated = true;
            break;
        }
        scanned += bytes.len();
        let (text, _) = crate::text_encoding::detect_and_decode(&bytes);
        let relative = entry
            .path()
            .strip_prefix(&root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        let title = content::title_of(&path, &text);
        let body = text.to_lowercase();
        let title_lower = title.to_lowercase();
        let metadata = metadata(&text);
        let tags = metadata
            .get("tags")
            .map(|v| {
                v.as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_lowercase)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| {
                        v.as_str()
                            .unwrap_or("")
                            .split(',')
                            .map(|s| s.trim().to_lowercase())
                            .collect()
                    })
            })
            .unwrap_or_default();
        if !terms.iter().all(|t| {
            let found = match t.field.as_str() {
                "path" => relative.to_lowercase().contains(&t.value),
                "title" => title_lower.contains(&t.value),
                "tag" => tags
                    .iter()
                    .any(|tag| tag.trim_start_matches('#') == t.value.trim_start_matches('#')),
                _ => body.contains(&t.value),
            };
            found != t.exclude
        }) {
            continue;
        }
        let line = text
            .lines()
            .position(|line| {
                terms
                    .iter()
                    .filter(|t| !t.exclude && t.field == "text")
                    .any(|t| line.to_lowercase().contains(&t.value))
            })
            .unwrap_or(0)
            + 1;
        let snippet = text
            .lines()
            .nth(line - 1)
            .unwrap_or("")
            .chars()
            .take(240)
            .collect::<String>();
        let score = terms
            .iter()
            .filter(|t| !t.exclude)
            .map(|t| {
                if title_lower.contains(&t.value) {
                    10
                } else {
                    1
                }
            })
            .sum::<usize>();
        hits.push(json!({"path":path,"relative_path":relative,"title":title,"line":line,"snippet":snippet,"score":score}));
    }
    hits.sort_by(|a, b| {
        b["score"].as_u64().cmp(&a["score"].as_u64()).then_with(|| {
            a["relative_path"]
                .as_str()
                .cmp(&b["relative_path"].as_str())
        })
    });
    let matched = hits.len();
    hits.truncate(limit);
    Ok(
        json!({"ok":true,"results":hits,"matched":matched,"truncated":truncated || matched>limit,
        "scanned_files":files,"scanned_bytes":scanned,"skipped_files":skipped,
        "limits":{"files":MAX_FILES,"total_bytes":MAX_SCAN_BYTES,"document_bytes":MAX_DOCUMENT_BYTES}}),
    )
}

/// Literal edits default to preview; committing requires a byte revision and
/// confirmation, preserves encoding/EOL, and records bounded recovery history.
pub fn edit_document(
    app: &App,
    path: &Path,
    expected: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    dry_run: bool,
    confirm: bool,
    cancel: Option<&AtomicBool>,
) -> Result<Value, String> {
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected_revision_required".into());
    }
    if old.is_empty() {
        return Err("empty_edit_match".into());
    }
    if new.len() > MAX_DOCUMENT_BYTES {
        return Err("document_too_large".into());
    }
    if !dry_run && !confirm {
        return Err("confirmation_required".into());
    }
    let _guard = crate::document_history::SAVE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cancelled(cancel)?;
    let path = absolute_file(path)?;
    let bytes = bytes_bounded(&path)?;
    if revision(&bytes) != expected.to_lowercase() {
        return Err("document_conflict".into());
    }
    let (text, encoding) = crate::text_encoding::detect_and_decode(&bytes);
    if crate::text_encoding::encode(&text, encoding).map_err(|_| "encoding_unrepresentable")?
        != bytes
    {
        return Err("encoding_not_lossless".into());
    }
    let old = old.replace("\r\n", "\n");
    let new = new.replace("\r\n", "\n");
    let normalized = text.replace("\r\n", "\n");
    let count = normalized.matches(&old).count();
    if count == 0 {
        return Err("edit_match_not_found".into());
    }
    if count > 1 && !replace_all {
        return Err("ambiguous_edit_match".into());
    }
    let replacements = if replace_all { count } else { 1 };
    let proposed = normalized
        .len()
        .checked_sub(old.len().saturating_mul(replacements))
        .and_then(|n| {
            new.len()
                .checked_mul(replacements)
                .and_then(|added| n.checked_add(added))
        });
    if proposed.is_none_or(|n| n > MAX_DOCUMENT_BYTES) {
        return Err("document_too_large".into());
    }
    // Map normalized match boundaries back to source bytes. Untouched text,
    // including mixed line endings, remains byte-for-byte equivalent.
    let mut offsets = Vec::with_capacity(normalized.len() + 1);
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' && i > 0 && text.as_bytes()[i - 1] == b'\r' {
            continue;
        }
        offsets.push(i);
    }
    offsets.push(text.len());
    let mut edited = String::with_capacity(text.len());
    let mut previous = 0;
    for (at, _) in normalized.match_indices(&old).take(replacements) {
        let begin = offsets[at];
        let end = offsets[at + old.len()];
        let crlf = text[begin..end].contains("\r\n")
            || (!text[begin..end].contains('\n') && text.contains("\r\n"));
        let replacement = if crlf {
            new.replace('\n', "\r\n")
        } else {
            new.clone()
        };
        if edited.len() + (begin - previous) + replacement.len() > MAX_DOCUMENT_BYTES {
            return Err("document_too_large".into());
        }
        edited.push_str(&text[previous..begin]);
        edited.push_str(&replacement);
        previous = end;
    }
    if edited.len() + text.len() - previous > MAX_DOCUMENT_BYTES {
        return Err("document_too_large".into());
    }
    edited.push_str(&text[previous..]);
    let encoded =
        crate::text_encoding::encode(&edited, encoding).map_err(|_| "encoding_unrepresentable")?;
    if encoded.len() > MAX_DOCUMENT_BYTES {
        return Err("document_too_large".into());
    }
    let changed = encoded != bytes;
    let mut checkpoint = Value::Null;
    if !dry_run && changed {
        cancelled(cancel)?;
        if revision(&bytes_bounded(&path)?) != expected.to_lowercase() {
            return Err("document_conflict".into());
        }
        checkpoint = json!(crate::document_history::checkpoint_file(
            &app.paths.data_dir,
            &path,
            "mcp_edit"
        )?
        .map(|entry| entry.id));
        cancelled(cancel)?;
        if revision(&bytes_bounded(&path)?) != expected.to_lowercase() {
            return Err("document_conflict".into());
        }
        content::write_bytes_atomic(&path, &encoded).map_err(|_| "file_write_failed")?;
        let _ = content::index(app, &path);
    }
    Ok(
        json!({"ok":true,"dry_run":dry_run,"changed":changed,"replacements":if replace_all{count}else{1},
        "revision":revision(if dry_run {&bytes}else{&encoded}),"proposed_revision":revision(&encoded),
        "encoding":encoding,"checkpoint_id":checkpoint,
        "preview":{"before":old.chars().take(1600).collect::<String>(),"after":new.chars().take(1600).collect::<String>(),
            "truncated":old.chars().count()>1600 || new.chars().count()>1600}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(root: &Path) -> App {
        App::bootstrap(crate::paths::AppPaths::with_dirs(
            &root.join("data"),
            root,
            &root.join("assets"),
        ))
        .unwrap()
    }
    #[test]
    fn each_host_checks_the_anchor_ids_it_actually_renders() {
        assert!(
            analyze("# Alpha title\n\n[[#Alpha title]]\n", None, None).unwrap()["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let source = "# Alpha\n\n# Alpha\n\n[duplicate](#alpha-1)\n";
        assert!(analyze(source, None, None).unwrap()["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            analyze_for_reader(source, None, None).unwrap()["diagnostics"][0]["code"],
            "missing_anchor"
        );
        assert!(
            analyze_for_reader(&source.replace("#alpha-1", "#alpha-2"), None, None).unwrap()
                ["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let unicode = "# Café\n\n[valid](#café)\n";
        assert!(analyze(unicode, None, None).unwrap()["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(
            analyze_for_reader(&unicode.replace("(#café)", "(#caf)"), None, None).unwrap()
                ["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn edits_require_revision_confirmation_and_keep_recovery_out_of_document_folder() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let file = dir.path().join("original.md");
        let original = b"# Notes\r\n\r\nOriginal text.\r\n";
        fs::write(&file, original).unwrap();
        let rev = revision(original);
        let preview = edit_document(
            &app, &file, &rev, "Original", "Revised", false, true, false, None,
        )
        .unwrap();
        assert_eq!(preview["changed"], true);
        assert_eq!(fs::read(&file).unwrap(), original);
        assert_eq!(
            edit_document(&app, &file, &rev, "Original", "Revised", false, false, false, None)
                .unwrap_err(),
            "confirmation_required"
        );
        let result = edit_document(
            &app, &file, &rev, "Original", "Revised", false, false, true, None,
        )
        .unwrap();
        assert_eq!(
            fs::read(&file).unwrap(),
            b"# Notes\r\n\r\nRevised text.\r\n"
        );
        let id = result["checkpoint_id"].as_str().unwrap();
        assert_eq!(
            crate::document_history::read(&app.paths.data_dir, id)
                .unwrap()
                .1,
            "# Notes\r\n\r\nOriginal text.\r\n"
        );
        assert!(!dir.path().join("original.md.bak").exists());
        assert_eq!(
            edit_document(&app, &file, &rev, "Revised", "Lost", false, false, true, None)
                .unwrap_err(),
            "document_conflict"
        );
    }
    #[test]
    fn editing_preserves_utf16_bom_mixed_eol_and_rejects_ambiguous_or_explosive_changes() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let file = dir.path().join("mixed.md");
        let text = "# Notes\r\nsame\nsame\r\n";
        let bytes = crate::text_encoding::encode(text, "utf-16le").unwrap();
        fs::write(&file, &bytes).unwrap();
        let rev = revision(&bytes);
        assert_eq!(
            edit_document(&app, &file, &rev, "same", "new", false, true, false, None).unwrap_err(),
            "ambiguous_edit_match"
        );
        let result =
            edit_document(&app, &file, &rev, "same", "new", true, false, true, None).unwrap();
        assert_eq!(result["encoding"], "utf-16-le");
        assert_eq!(
            fs::read(&file).unwrap(),
            crate::text_encoding::encode("# Notes\r\nnew\nnew\r\n", "utf-16le").unwrap()
        );
        fs::write(&file, "a".repeat(64 * 1024)).unwrap();
        assert_eq!(
            edit_document(
                &app,
                &file,
                &revision(&fs::read(&file).unwrap()),
                "a",
                &"b".repeat(64 * 1024),
                true,
                true,
                false,
                None
            )
            .unwrap_err(),
            "document_too_large"
        );
    }
    #[test]
    fn legacy_chinese_encoding_is_preserved_and_lossy_input_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let file = dir.path().join("reading.md");
        let bytes = crate::text_encoding::encode("# 阅读\n\n你好，世界。\n", "gb18030").unwrap();
        fs::write(&file, &bytes).unwrap();
        let changed = edit_document(
            &app,
            &file,
            &revision(&bytes),
            "世界",
            "哲学",
            false,
            false,
            true,
            None,
        )
        .unwrap();
        assert_eq!(changed["encoding"], "gb18030");
        assert_eq!(
            fs::read(&file).unwrap(),
            crate::text_encoding::encode("# 阅读\n\n你好，哲学。\n", "gb18030").unwrap()
        );
        let invalid = b"\xef\xbb\xbf\xff";
        fs::write(&file, invalid).unwrap();
        assert_eq!(
            edit_document(
                &app,
                &file,
                &revision(invalid),
                "x",
                "y",
                false,
                false,
                true,
                None
            )
            .unwrap_err(),
            "encoding_not_lossless"
        );
        assert_eq!(fs::read(&file).unwrap(), invalid);
    }
    #[test]
    fn cancellation_and_invalid_ranges_do_not_change_files() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let file = dir.path().join("notes.md");
        fs::write(&file, "# Notes\n").unwrap();
        let cancel = AtomicBool::new(true);
        assert_eq!(
            edit_document(
                &app,
                &file,
                &revision(b"# Notes\n"),
                "Notes",
                "Change",
                false,
                false,
                true,
                Some(&cancel)
            )
            .unwrap_err(),
            "cancelled"
        );
        assert_eq!(
            search_workspace(dir.path(), "Notes", 20, Some(&cancel)).unwrap_err(),
            "cancelled"
        );
        assert_eq!(read_document(&file, 5, 8).unwrap_err(), "line_out_of_range");
        assert_eq!(fs::read_to_string(&file).unwrap(), "# Notes\n");
    }
    #[test]
    fn block_tags_and_unchecked_large_anchor_targets_are_reported_honestly() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("index.md");
        fs::write(
            &file,
            "---\ntags:\n  - philosophy\n  - 'reading'\n---\n# Reading\n",
        )
        .unwrap();
        assert_eq!(
            search_workspace(dir.path(), "tag:reading", 20, None).unwrap()["matched"],
            1
        );
        fs::write(
            dir.path().join("large.md"),
            "x".repeat(MAX_DOCUMENT_BYTES + 1),
        )
        .unwrap();
        let result = analyze("[large](large.md#unknown)", Some(&file), Some(dir.path())).unwrap();
        assert_eq!(result["truncated"], true);
        assert!(result["diagnostics"].as_array().unwrap().is_empty());
    }
    #[test]
    fn inspection_uses_ast_and_reports_utf16_locations() {
        let report=analyze("# Alpha\n\n- [x] done\n- [ ] next\n\n[bad](#missing)\n\n~~~md\n[ignored](gone.md)\n~~~\n",None,None).unwrap();
        assert_eq!(report["links"].as_array().unwrap().len(), 1);
        assert_eq!(report["diagnostics"][0]["code"], "missing_anchor");
        assert_eq!(report["stats"]["completed_tasks"], 1);
        assert_eq!(report["headings"][0]["position"]["line"], 1);
    }
    #[test]
    fn workspace_search_combines_phrases_paths_tags_and_exclusions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("reading.md"),
            "---\ntags: [philosophy, reading]\n---\n# Plato\n\nA careful argument.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("other.md"),
            "# Other\nA careless argument.\n",
        )
        .unwrap();
        let root = crate::paths::canonicalize_or_clean(dir.path());
        let report = search_workspace(
            &root,
            "tag:philosophy \"careful argument\" -careless",
            10,
            None,
        )
        .unwrap();
        assert_eq!(report["matched"], 1);
        assert_eq!(report["results"][0]["relative_path"], "reading.md");
        assert!(search_workspace(&root, "\"unfinished", 10, None).is_err());
    }
    #[test]
    fn local_checks_do_not_read_outside_workspace_and_wiki_resolves_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::paths::canonicalize_or_clean(dir.path());
        fs::create_dir(root.join("notes")).unwrap();
        fs::write(root.join("notes/Plato.md"), "# Plato").unwrap();
        let file = root.join("index.md");
        let report = analyze(
            "[[Plato]]\n\n[escape](../secret.md)\n",
            Some(&file),
            Some(&root),
        )
        .unwrap();
        assert!(report["links"][0]["resolved_path"]
            .as_str()
            .unwrap()
            .ends_with("Plato.md"));
        assert_eq!(report["diagnostics"][0]["code"], "outside_workspace");
    }
}
