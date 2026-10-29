//! Lossless source edits for a same-directory Markdown document rename.
//!
//! This module is deliberately pure: it does not enumerate, open, canonicalize,
//! or write files. The caller supplies authorized, canonical absolute paths and
//! must separately prove that reference resolution has no symlink/junction or
//! case-sensitive-Windows-directory aliases. Its transaction must refuse a plan
//! with diagnostics, check the original byte revision, and apply edits backwards.
//! Normal Markdown is never rendered and reserialized.

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
use std::{
    ops::Range,
    path::{Component, Path, PathBuf},
};

const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_REFERENCES: usize = 32_768;
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown"];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LinkSourceEdit {
    /// Inclusive UTF-8 byte offset into the original source.
    pub start: usize,
    /// Exclusive UTF-8 byte offset into the original source.
    pub end: usize,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LinkDiagnostic {
    /// Stable machine-readable reason; never includes source or filesystem data.
    pub code: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LinkLocalReference {
    /// Absolute, lexically normalized path within the authorized root. This is
    /// an audit candidate, not a claim that the file exists or has this spelling.
    pub path: PathBuf,
    /// Original destination byte span, including query/fragment bytes, but
    /// excluding angle/wiki delimiters and explicit wiki aliases. For a parser
    /// destination whose source slice cannot be isolated, this conservatively
    /// covers the parser-authorized link construct instead.
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct LinkEditPlan {
    /// Sorted, non-overlapping byte edits. All other source bytes are preserved.
    pub edits: Vec<LinkSourceEdit>,
    /// Every diagnostic is a blocker, even when some other edits are available.
    pub diagnostics: Vec<LinkDiagnostic>,
    /// All safely decoded local paths, including paths unrelated to old_target.
    /// The caller must check physical identity before commit: Windows short-name
    /// and Unicode case aliases cannot be proven from source spelling alone.
    /// Wiki destinations also include Markdown-extension completion candidates.
    pub local_references: Vec<LinkLocalReference>,
}

/// Plan relative inline-link, image, reference-definition and explicit wikilink
/// destination updates for an authorized same-directory `.md`, `.markdown`, or
/// `.mdown` rename. The old and new target must have the same extension.
///
/// The five paths use the host platform's absolute-path syntax. Components are
/// normalized lexically, with exact case on Unix and ASCII-insensitive case on
/// Windows. Possible non-ASCII Windows case aliases are diagnosed, not guessed.
/// Only the filename changes: directory spelling, escapes, existing percent
/// encodings, angle brackets, labels, titles, fragments, queries and line endings
/// retain their original bytes. New filename characters are escaped/encoded as
/// necessary for their existing destination syntax.
///
/// `..` may traverse within the root. References outside the root, into private
/// components, to external/protocol URLs, or in code/math/metadata are untouched.
/// Root-relative links and HTML attributes referring to the renamed file are
/// diagnosed because they are outside the supported rewrite scope. Extensionless
/// or global-basename wikilinks that could name it are likewise blockers. Parser
/// uncertainty about a relevant destination is never an authorization to edit it.
///
/// The caller MUST refuse commit unless `diagnostics.is_empty()`. It must prove
/// physical path identity and absence of reparse aliases separately; a pure text
/// planner cannot discover those from path strings. No filesystem IO occurs.
pub fn plan_link_updates(
    source: &str,
    source_path: &Path,
    old_target: &Path,
    new_target: &Path,
    workspace_root: &Path,
) -> Result<LinkEditPlan, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("link_source_too_large".into());
    }
    let context = Context::new(source_path, old_target, new_target, workspace_root)?;
    let bom = if source.starts_with('\u{feff}') { 3 } else { 0 };
    let parser = Parser::new_ext(&source[bom..], options());
    let definitions: Vec<_> = parser
        .reference_definitions()
        .iter()
        .map(|(_, definition)| {
            (
                definition.span.start + bom..definition.span.end + bom,
                definition.dest.to_string(),
            )
        })
        .collect();
    let mut plan = LinkEditPlan::default();
    // Inline HTML is opaque until its matching closing tag. This also protects
    // Markdown-looking examples inside <code>, <pre>, <script>, or custom tags.
    let mut html_stack = Vec::<String>::new();
    let mut html_start = None;
    let mut html_regions = Vec::<Range<usize>>::new();
    for (event, span) in parser.into_offset_iter() {
        let span = span.start + bom..span.end + bom;
        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            })
            | Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) => {
                match link_type {
                    LinkType::Inline => {
                        if let Some(destination) =
                            inline_destination(source, span.clone(), &dest_url)
                        {
                            process_destination(
                                source,
                                destination,
                                Syntax::Markdown,
                                !html_stack.is_empty(),
                                &context,
                                &mut plan,
                            );
                        } else {
                            diagnose_unlocated(&dest_url, span, &context, &mut plan);
                        }
                    }
                    LinkType::WikiLink { .. } => {
                        if let Some(destination) = wiki_destination(source, span.clone()) {
                            process_destination(
                                source,
                                destination,
                                Syntax::Wiki,
                                !html_stack.is_empty(),
                                &context,
                                &mut plan,
                            );
                        } else {
                            diagnose_unlocated(&dest_url, span, &context, &mut plan);
                        }
                    }
                    // The actual definition, including an unused definition,
                    // is updated once. Reference calls retain their exact form.
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => (),
                    LinkType::Autolink | LinkType::Email => (),
                    _ => diagnose_unlocated(&dest_url, span, &context, &mut plan),
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                let before = html_stack.is_empty();
                inspect_html(source, span.clone(), &context, &mut plan, &mut html_stack);
                if before && !html_stack.is_empty() {
                    html_start = Some(span.start);
                } else if !before && html_stack.is_empty() {
                    if let Some(start) = html_start.take() {
                        html_regions.push(start..span.end);
                    }
                }
            }
            _ => (),
        }
        if plan.edits.len() + plan.diagnostics.len() > MAX_REFERENCES
            || plan.local_references.len() > MAX_REFERENCES * 4
        {
            return Err("link_reference_limit".into());
        }
    }
    if let Some(start) = html_start {
        html_regions.push(start..source.len());
    }
    for (span, expected) in definitions {
        if let Some(destination) = definition_destination(source, span.clone(), &expected) {
            let preceding = html_regions.partition_point(|region| region.start <= span.start);
            let in_html = preceding > 0 && span.start < html_regions[preceding - 1].end;
            process_destination(
                source,
                destination,
                Syntax::Markdown,
                in_html,
                &context,
                &mut plan,
            );
        } else {
            diagnose_unlocated(&expected, span, &context, &mut plan);
        }
        if plan.edits.len() + plan.diagnostics.len() > MAX_REFERENCES
            || plan.local_references.len() > MAX_REFERENCES * 4
        {
            return Err("link_reference_limit".into());
        }
    }
    plan.edits.sort_by_key(|edit| (edit.start, edit.end));
    plan.edits.dedup();
    if let Some(pair) = plan
        .edits
        .windows(2)
        .find(|pair| pair[0].end > pair[1].start)
    {
        let span = pair[0].start..pair[1].end;
        plan.edits.clear();
        diagnostic(&mut plan, "overlapping_link_edits", span);
    }
    plan.diagnostics
        .sort_by(|a, b| (a.start, a.end, &a.code).cmp(&(b.start, b.end, &b.code)));
    plan.diagnostics.dedup();
    plan.local_references
        .sort_by(|a, b| (a.start, a.end, &a.path).cmp(&(b.start, b.end, &b.path)));
    plan.local_references.dedup();
    Ok(plan)
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_MATH
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

#[derive(Clone, Debug)]
struct NormalPath {
    anchor: String,
    parts: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Same,
    Different,
    Uncertain,
}

fn compare_component(a: &str, b: &str) -> Comparison {
    if a == b || (cfg!(windows) && a.eq_ignore_ascii_case(b)) {
        return Comparison::Same;
    }
    if cfg!(windows)
        && (a.to_lowercase() == b.to_lowercase() || a.to_uppercase() == b.to_uppercase())
    {
        return Comparison::Uncertain;
    }
    Comparison::Different
}

fn compare_parts(a: &[String], b: &[String]) -> Comparison {
    if a.len() != b.len() {
        return Comparison::Different;
    }
    let mut uncertain = false;
    for (a, b) in a.iter().zip(b) {
        match compare_component(a, b) {
            Comparison::Different => return Comparison::Different,
            Comparison::Uncertain => uncertain = true,
            Comparison::Same => (),
        }
    }
    if uncertain {
        Comparison::Uncertain
    } else {
        Comparison::Same
    }
}

impl NormalPath {
    fn absolute(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("link_path_must_be_absolute".into());
        }
        let mut result = Self {
            anchor: String::new(),
            parts: Vec::new(),
        };
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => {
                    let prefix = prefix.as_os_str().to_str().ok_or("link_path_not_utf8")?;
                    result.anchor = prefix.replace('\\', "/");
                    if let Some(prefix) = result.anchor.strip_prefix("//?/") {
                        result.anchor = if let Some(unc) = prefix.strip_prefix("UNC/") {
                            format!("//{unc}")
                        } else {
                            prefix.to_owned()
                        };
                    }
                }
                Component::RootDir | Component::CurDir => (),
                Component::ParentDir => {
                    if result.parts.pop().is_none() {
                        return Err("invalid_link_path".into());
                    }
                }
                Component::Normal(part) => {
                    result
                        .parts
                        .push(part.to_str().ok_or("link_path_not_utf8")?.to_owned());
                }
            }
        }
        Ok(result)
    }

    fn compare(&self, other: &Self) -> Comparison {
        let anchor = compare_component(&self.anchor, &other.anchor);
        if anchor == Comparison::Different {
            return anchor;
        }
        let parts = compare_parts(&self.parts, &other.parts);
        if parts == Comparison::Different {
            parts
        } else if anchor == Comparison::Uncertain || parts == Comparison::Uncertain {
            Comparison::Uncertain
        } else {
            Comparison::Same
        }
    }

    fn within(&self, root: &Self) -> bool {
        self.parts.len() >= root.parts.len()
            && compare_component(&self.anchor, &root.anchor) == Comparison::Same
            && compare_parts(&self.parts[..root.parts.len()], &root.parts) == Comparison::Same
    }

    fn to_path_buf(&self) -> PathBuf {
        let mut path = if cfg!(windows) {
            PathBuf::from(format!("{}\\", self.anchor.replace('/', "\\")))
        } else {
            PathBuf::from("/")
        };
        for part in &self.parts {
            path.push(part);
        }
        path
    }
}

struct Context {
    root: NormalPath,
    base: NormalPath,
    old: NormalPath,
    new_name: String,
}

impl Context {
    fn new(source: &Path, old: &Path, new: &Path, root: &Path) -> Result<Self, String> {
        let root = NormalPath::absolute(root)?;
        let source = NormalPath::absolute(source)?;
        let old = NormalPath::absolute(old)?;
        let new = NormalPath::absolute(new)?;
        for path in [&source, &old, &new] {
            if !path.within(&root) || path.parts.len() <= root.parts.len() {
                return Err("link_path_outside_workspace".into());
            }
            if path.parts[root.parts.len()..]
                .iter()
                .any(|part| private_component(part))
            {
                return Err("private_link_path".into());
            }
            if cfg!(windows)
                && path
                    .parts
                    .iter()
                    .any(|part| invalid_windows_component(part))
            {
                return Err("ambiguous_windows_path".into());
            }
        }
        if compare_component(&old.anchor, &new.anchor) != Comparison::Same
            || compare_parts(
                &old.parts[..old.parts.len() - 1],
                &new.parts[..new.parts.len() - 1],
            ) != Comparison::Same
        {
            return Err("link_rename_must_stay_in_directory".into());
        }
        let old_name = old.parts.last().ok_or("invalid_link_path")?;
        let new_name = new.parts.last().ok_or("invalid_link_path")?.clone();
        let old_extension = old_name.rsplit_once('.').map(|(_, ext)| ext);
        let new_extension = new_name.rsplit_once('.').map(|(_, ext)| ext);
        if !old_extension.is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        }) || !old_extension
            .zip(new_extension)
            .is_some_and(|(old, new)| old.eq_ignore_ascii_case(new))
        {
            return Err("link_rename_requires_markdown".into());
        }
        let mut base = source;
        base.parts.pop();
        Ok(Self {
            root,
            base,
            old,
            new_name,
        })
    }
}

fn private_component(part: &str) -> bool {
    part.starts_with('.') && part != "." && part != ".."
        || matches!(
            if cfg!(windows) {
                part.to_ascii_lowercase()
            } else {
                part.to_owned()
            }
            .as_str(),
            "node_modules" | "target" | "dist" | "build" | "vendor" | "venv" | "__pycache__"
        )
}

fn invalid_windows_component(part: &str) -> bool {
    part.ends_with(['.', ' '])
        || part
            .chars()
            .any(|c| c.is_control() || "<>:\"\\|?*".contains(c))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Markdown,
    Wiki,
    Html,
    Semantic,
}

#[derive(Clone, Debug)]
struct Atom {
    value: char,
    start: usize,
    end: usize,
}

fn punctuation(byte: u8) -> bool {
    byte.is_ascii_punctuation()
}

/// Decode CommonMark punctuation escapes and entities, retaining exact source
/// ownership for every resulting scalar. Wiki names are raw in pulldown-cmark.
fn atoms(raw: &str, syntax: Syntax) -> Vec<Atom> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let start = i;
        if syntax == Syntax::Markdown
            && raw.as_bytes()[i] == b'\\'
            && raw
                .as_bytes()
                .get(i + 1)
                .is_some_and(|byte| punctuation(*byte))
        {
            result.push(Atom {
                value: raw.as_bytes()[i + 1] as char,
                start,
                end: i + 2,
            });
            i += 2;
            continue;
        }
        if matches!(syntax, Syntax::Markdown | Syntax::Html) && raw.as_bytes()[i] == b'&' {
            if let Some(end) = raw[i..].find(';').filter(|end| *end <= 48) {
                let end = i + end + 1;
                let candidate = &raw[i..end];
                if candidate
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"&#;".contains(&byte))
                {
                    let decoded: String = Parser::new(candidate)
                        .filter_map(|event| {
                            if let Event::Text(value) = event {
                                Some(value.into_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !decoded.is_empty() && decoded != candidate {
                        for value in decoded.chars() {
                            result.push(Atom { value, start, end });
                        }
                        i = end;
                        continue;
                    }
                }
            }
        }
        let value = raw[i..].chars().next().expect("UTF-8 source boundary");
        i += value.len_utf8();
        result.push(Atom {
            value,
            start,
            end: i,
        });
    }
    result
}

fn atom_text(atoms: &[Atom]) -> String {
    atoms.iter().map(|atom| atom.value).collect()
}

fn hex(c: char) -> Option<u8> {
    c.to_digit(16).filter(|_| c.is_ascii()).map(|n| n as u8)
}

fn percent_decode(input: &[Atom]) -> Result<Vec<Atom>, &'static str> {
    let mut bytes = Vec::new();
    let mut ownership = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i].value == '%' {
            let a = input.get(i + 1).and_then(|atom| hex(atom.value));
            let b = input.get(i + 2).and_then(|atom| hex(atom.value));
            let (Some(a), Some(b)) = (a, b) else {
                return Err("invalid_percent_encoding");
            };
            bytes.push(a * 16 + b);
            ownership.push((input[i].start, input[i + 2].end));
            i += 3;
        } else {
            let mut buffer = [0; 4];
            let encoded = input[i].value.encode_utf8(&mut buffer);
            bytes.extend_from_slice(encoded.as_bytes());
            ownership.extend(std::iter::repeat_n(
                (input[i].start, input[i].end),
                encoded.len(),
            ));
            i += 1;
        }
    }
    let decoded = String::from_utf8(bytes).map_err(|_| "invalid_percent_encoding")?;
    Ok(decoded
        .char_indices()
        .map(|(i, value)| Atom {
            value,
            start: ownership[i].0,
            end: ownership[i + value.len_utf8() - 1].1,
        })
        .collect())
}

fn external(value: &str) -> bool {
    if value.starts_with("//") {
        return true;
    }
    value.split_once(':').is_some_and(|(scheme, _)| {
        scheme
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
            && scheme
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(&byte))
    })
}

fn windows_drive_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    cfg!(windows)
        && bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

enum Destination {
    Unrelated,
    Target { filename: Vec<Atom> },
    Block(&'static str),
}

fn classify(
    raw: &str,
    syntax: Syntax,
    context: &Context,
    mut local_paths: Option<&mut Vec<PathBuf>>,
) -> Destination {
    let decoded = atoms(raw, syntax);
    let end = decoded
        .iter()
        .position(|atom| matches!(atom.value, '#' | '?'))
        .unwrap_or(decoded.len());
    let path_atoms = &decoded[..end];
    let path = atom_text(path_atoms);
    if path.is_empty() || external(&path) && !windows_drive_absolute(&path) {
        return Destination::Unrelated;
    }
    let decoded = match percent_decode(path_atoms) {
        Ok(decoded) => decoded,
        Err(code) => return Destination::Block(code),
    };
    let path = atom_text(&decoded);
    if windows_drive_absolute(&path) {
        // An authorized workspace root may itself be below a dot directory.
        // Only that already-authorized prefix is exempt from the private rule;
        // private intermediate components in a reference are still excluded,
        // including components later canceled by `..`.
        let slash_path = path.replace('\\', "/");
        let mut authorized_prefix = true;
        if slash_path
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .skip(1)
            .enumerate()
            .any(|(index, part)| {
                let authorized_component = authorized_prefix
                    && context.root.parts.get(index).is_some_and(|root_part| {
                        compare_component(part, root_part) == Comparison::Same
                    });
                authorized_prefix = authorized_component;
                private_component(part) && !authorized_component
            })
        {
            return Destination::Unrelated;
        }
        return match NormalPath::absolute(Path::new(&path)) {
            Ok(absolute)
                if absolute.within(&context.root)
                    && !absolute
                        .parts
                        .iter()
                        .any(|part| invalid_windows_component(part)) =>
            {
                if let Some(paths) = local_paths {
                    paths.push(absolute.to_path_buf());
                }
                if absolute.compare(&context.old) != Comparison::Different {
                    Destination::Block("unsupported_absolute_reference")
                } else {
                    Destination::Unrelated
                }
            }
            _ => Destination::Unrelated,
        };
    }
    if path.split('/').any(private_component) {
        return Destination::Unrelated;
    }
    if path.chars().any(|c| c.is_control()) {
        return Destination::Block("invalid_local_destination");
    }
    if path.contains('\\') {
        return Destination::Block("unsupported_backslash_reference");
    }
    if path.contains(':') {
        return Destination::Block("unsupported_local_destination");
    }
    let rooted = path.starts_with('/');
    let mut resolved = if rooted {
        context.root.clone()
    } else {
        context.base.clone()
    };
    for part in path.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                if resolved.parts.len() <= context.root.parts.len() {
                    return Destination::Unrelated;
                }
                resolved.parts.pop();
            }
            part => {
                if cfg!(windows) && invalid_windows_component(part) {
                    return Destination::Block("ambiguous_windows_path");
                }
                resolved.parts.push(part.to_owned());
            }
        }
    }
    if !resolved.within(&context.root) {
        return Destination::Unrelated;
    }
    if let Some(paths) = local_paths.as_mut() {
        paths.push(resolved.to_path_buf());
        if syntax == Syntax::Wiki
            && !path.ends_with('/')
            && !matches!(path.rsplit('/').next(), Some("." | ".."))
        {
            for extension in MARKDOWN_EXTENSIONS {
                let mut completed = resolved.clone();
                if let Some(last) = completed.parts.last_mut() {
                    last.push('.');
                    last.push_str(extension);
                    paths.push(completed.to_path_buf());
                }
            }
        }
    }
    let comparison = resolved.compare(&context.old);
    if comparison == Comparison::Uncertain {
        return Destination::Block("ambiguous_windows_case");
    }
    if comparison != Comparison::Same {
        // Reader/knowledge navigation also completes Markdown extensions. Bare wiki names
        // may fall back to a global basename search, which this pure API cannot
        // prove unique. Neither is silently changed into an explicit path.
        for extension in MARKDOWN_EXTENSIONS {
            let mut completed = resolved.clone();
            if let Some(last) = completed.parts.last_mut() {
                last.push('.');
                last.push_str(extension);
            }
            if completed.compare(&context.old) != Comparison::Different {
                return Destination::Block(if syntax == Syntax::Wiki {
                    "ambiguous_wiki_target"
                } else {
                    "unsupported_extensionless_target"
                });
            }
        }
        if syntax == Syntax::Wiki && !path.contains('/') {
            let name = context.old.parts.last().expect("validated old filename");
            let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
            if compare_component(&path, name) != Comparison::Different
                || compare_component(&path, stem) != Comparison::Different
            {
                return Destination::Block("ambiguous_wiki_target");
            }
        }
        return Destination::Unrelated;
    }
    if rooted {
        return Destination::Block("unsupported_root_relative_reference");
    }
    let start = decoded
        .iter()
        .rposition(|atom| atom.value == '/')
        .map(|i| i + 1)
        .unwrap_or(0);
    let filename = decoded[start..].to_vec();
    let name = atom_text(&filename);
    if filename.is_empty()
        || compare_component(&name, context.old.parts.last().unwrap()) != Comparison::Same
    {
        return Destination::Block("unsupported_path_suffix");
    }
    Destination::Target { filename }
}

fn diagnostic(plan: &mut LinkEditPlan, code: &str, span: Range<usize>) {
    plan.diagnostics.push(LinkDiagnostic {
        code: code.into(),
        start: span.start,
        end: span.end,
    });
}

fn diagnose_unlocated(
    expected: &str,
    span: Range<usize>,
    context: &Context,
    plan: &mut LinkEditPlan,
) {
    let mut paths = Vec::new();
    let decision = classify(expected, Syntax::Semantic, context, Some(&mut paths));
    for path in paths {
        plan.local_references.push(LinkLocalReference {
            path,
            start: span.start,
            end: span.end,
        });
    }
    if !matches!(decision, Destination::Unrelated) {
        diagnostic(plan, "unsupported_destination_syntax", span);
    }
}

fn process_destination(
    source: &str,
    span: Range<usize>,
    syntax: Syntax,
    in_html: bool,
    context: &Context,
    plan: &mut LinkEditPlan,
) {
    let raw = &source[span.clone()];
    let mut paths = Vec::new();
    let decision = classify(raw, syntax, context, Some(&mut paths));
    for path in paths {
        plan.local_references.push(LinkLocalReference {
            path,
            start: span.start,
            end: span.end,
        });
    }
    match decision {
        Destination::Unrelated => (),
        Destination::Block(code) => diagnostic(plan, code, span),
        Destination::Target { filename } => {
            if syntax == Syntax::Html || in_html {
                diagnostic(plan, "unsupported_html_reference", span);
            } else {
                let angle = syntax == Syntax::Markdown
                    && span.start > 0
                    && source.as_bytes()[span.start - 1] == b'<'
                    && source.as_bytes().get(span.end) == Some(&b'>');
                match filename_edit(raw, span.start, &filename, &context.new_name, syntax, angle) {
                    Ok(Some(edit)) => plan.edits.push(edit),
                    Ok(None) => (),
                    Err(code) => diagnostic(plan, code, span),
                }
            }
        }
    }
}

fn filename_edit(
    raw: &str,
    offset: usize,
    old: &[Atom],
    new: &str,
    syntax: Syntax,
    angle: bool,
) -> Result<Option<LinkSourceEdit>, &'static str> {
    let new_chars: Vec<_> = new.chars().collect();
    let mut prefix = old
        .iter()
        .zip(&new_chars)
        .take_while(|(a, b)| a.value == **b)
        .count();
    if prefix == old.len() && prefix == new_chars.len() {
        return Ok(None);
    }
    let mut suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new_chars[prefix..].iter().rev())
        .take_while(|(a, b)| a.value == **b)
        .count();
    // An entity may own several Unicode scalars. A source edit must never cut
    // that source token in half, even if only one of its scalars differs.
    while prefix > 0 && prefix < old.len() && old[prefix - 1].end > old[prefix].start {
        prefix -= 1;
    }
    while suffix > 0
        && old.len() > suffix
        && old[old.len() - suffix - 1].end > old[old.len() - suffix].start
    {
        suffix -= 1;
    }
    let start = old
        .get(prefix)
        .map(|atom| atom.start)
        .unwrap_or_else(|| old.last().unwrap().end);
    let end = if suffix == 0 {
        old.last().unwrap().end
    } else {
        old[old.len() - suffix].start
    };
    let middle: String = new_chars[prefix..new_chars.len() - suffix].iter().collect();
    let old_middle = &raw[start..end];
    let old_name = &raw[old.first().unwrap().start..old.last().unwrap().end];
    let replacement = encode_filename_middle(&middle, old_middle, old_name, syntax, angle);
    let filename_start = old.first().unwrap().start;
    let filename_end = old.last().unwrap().end;
    let new_raw_name = format!(
        "{}{}{}",
        &raw[filename_start..start],
        replacement,
        &raw[end..filename_end]
    );
    if atom_text(&percent_decode(&atoms(&new_raw_name, syntax))?) != new {
        return Err("unsupported_filename_encoding");
    }
    let updated = format!("{}{}{}", &raw[..start], replacement, &raw[end..]);
    if syntax == Syntax::Markdown {
        let checked = if angle {
            format!("<{updated}>")
        } else {
            updated
        };
        if !scan_destination(&checked, 0, checked.len())
            .is_some_and(|(_, end)| end == checked.len())
        {
            return Err("unsupported_filename_encoding");
        }
    }
    Ok(Some(LinkSourceEdit {
        start: offset + start,
        end: offset + end,
        replacement,
    }))
}

fn encode_filename_middle(
    value: &str,
    old_middle: &str,
    old_name: &str,
    syntax: Syntax,
    angle: bool,
) -> String {
    let percent_all = !old_middle.is_empty() && all_percent_encoded(old_middle);
    let percent_unicode = old_name.as_bytes().windows(3).any(|bytes| {
        bytes[0] == b'%'
            && hex(bytes[1] as char)
                .zip(hex(bytes[2] as char))
                .is_some_and(|(a, b)| a * 16 + b >= 0x80)
    });
    let lower_hex = old_name
        .as_bytes()
        .windows(3)
        .filter(|bytes| bytes[0] == b'%')
        .any(|bytes| bytes[1..].iter().any(u8::is_ascii_lowercase))
        && !old_name
            .as_bytes()
            .windows(3)
            .filter(|bytes| bytes[0] == b'%')
            .any(|bytes| bytes[1..].iter().any(u8::is_ascii_uppercase));
    let raw_parentheses = angle
        || old_name
            .match_indices(['(', ')'])
            .any(|(at, _)| !escaped(old_name, at));
    let mut result = String::new();
    for c in value.chars() {
        let encode = percent_all
            || c.is_control()
            || "?#%<>\\&".contains(c)
            || c == ' ' && (syntax != Syntax::Wiki || old_name.contains("%20"))
            || syntax == Syntax::Wiki && "|[]".contains(c)
            || !c.is_ascii() && percent_unicode;
        if encode {
            let mut buffer = [0; 4];
            for byte in c.encode_utf8(&mut buffer).bytes() {
                let digits = if lower_hex {
                    b"0123456789abcdef"
                } else {
                    b"0123456789ABCDEF"
                };
                result.push('%');
                result.push(digits[(byte >> 4) as usize] as char);
                result.push(digits[(byte & 15) as usize] as char);
            }
        } else {
            if syntax == Syntax::Markdown && !raw_parentheses && "()".contains(c) {
                result.push('\\');
            }
            result.push(c);
        }
    }
    result
}

fn all_percent_encoded(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len().is_multiple_of(3)
        && bytes.chunks_exact(3).all(|bytes| {
            bytes[0] == b'%' && bytes[1].is_ascii_hexdigit() && bytes[2].is_ascii_hexdigit()
        })
}

fn whitespace(source: &str, mut at: usize, end: usize) -> usize {
    while at < end && source.as_bytes()[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// Return the content slice, excluding angle delimiters, and its syntax end.
fn scan_destination(source: &str, start: usize, end: usize) -> Option<(Range<usize>, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(start) == Some(&b'<') {
        let mut at = start + 1;
        while at < end {
            match bytes[at] {
                b'<' | b'\r' | b'\n' => return None,
                b'>' => return Some((start + 1..at, at + 1)),
                b'\\' if at + 1 < end && punctuation(bytes[at + 1]) => at += 1,
                _ => (),
            }
            at += 1;
        }
        return None;
    }
    let mut at = start;
    let mut depth = 0usize;
    while at < end {
        match bytes[at] {
            0..=32 => break,
            b'(' => {
                depth += 1;
                if depth > 32 {
                    return None;
                }
            }
            b')' if depth == 0 => break,
            b')' => depth -= 1,
            b'\\' if at + 1 < end && punctuation(bytes[at + 1]) => at += 1,
            _ => (),
        }
        at += 1;
    }
    (depth == 0).then_some((start..at, at))
}

fn inline_tail(source: &str, open: usize, end: usize) -> Option<Range<usize>> {
    let start = whitespace(source, open + 1, end);
    let (destination, after) = scan_destination(source, start, end)?;
    let mut at = whitespace(source, after, end);
    if source.as_bytes().get(at) == Some(&b')') && at + 1 == end {
        return Some(destination);
    }
    if after == at {
        return None;
    }
    let delimiter = *source.as_bytes().get(at)?;
    if !matches!(delimiter, b'\'' | b'"' | b'(') {
        return None;
    }
    let close = if delimiter == b'(' { b')' } else { delimiter };
    at += 1;
    while at < end {
        if source.as_bytes()[at] == b'\\' && at + 1 < end && punctuation(source.as_bytes()[at + 1])
        {
            at += 2;
        } else if source.as_bytes()[at] == close {
            at = whitespace(source, at + 1, end);
            return (source.as_bytes().get(at) == Some(&b')') && at + 1 == end)
                .then_some(destination);
        } else {
            at += 1;
        }
    }
    None
}

fn escaped(source: &str, at: usize) -> bool {
    source.as_bytes()[..at]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        % 2
        == 1
}

fn inline_destination(source: &str, span: Range<usize>, expected: &str) -> Option<Range<usize>> {
    let raw = &source[span.clone()];
    if !(raw.starts_with('[') || raw.starts_with("![")) {
        return None;
    }
    let candidates: Vec<_> = raw
        .match_indices("](")
        .map(|(i, _)| span.start + i)
        .collect();
    if candidates.len() > 64 {
        return None;
    }
    let mut found = None;
    for at in candidates {
        if escaped(source, at) {
            continue;
        }
        let Some(destination) = inline_tail(source, at + 1, span.end) else {
            continue;
        };
        if atom_text(&atoms(&source[destination.clone()], Syntax::Markdown)) != expected {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(destination);
    }
    found
}

fn definition_destination(
    source: &str,
    span: Range<usize>,
    expected: &str,
) -> Option<Range<usize>> {
    let raw = &source[span.clone()];
    let first = raw.find('[')? + span.start;
    let mut at = first + 1;
    while at + 1 < span.end {
        if source.as_bytes()[at] == b']'
            && source.as_bytes()[at + 1] == b':'
            && !escaped(source, at)
        {
            let start = whitespace(source, at + 2, span.end);
            let (destination, _) = scan_destination(source, start, span.end)?;
            return (atom_text(&atoms(&source[destination.clone()], Syntax::Markdown)) == expected)
                .then_some(destination);
        }
        at += 1;
    }
    None
}

fn wiki_destination(source: &str, span: Range<usize>) -> Option<Range<usize>> {
    let raw = &source[span.clone()];
    let prefix = if raw.starts_with("![[") {
        3
    } else if raw.starts_with("[[") {
        2
    } else {
        return None;
    };
    if !raw.ends_with("]]") {
        return None;
    }
    let end = raw[prefix..raw.len() - 2]
        .find('|')
        .map(|i| prefix + i)
        .unwrap_or(raw.len() - 2);
    let body = &raw[prefix..end];
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let leading = body.len() - body.trim_start().len();
    Some(span.start + prefix + leading..span.start + prefix + leading + trimmed.len())
}

fn html_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-_:".contains(&byte)
}

/// A conservative lexical attribute reader, never an HTML rewrite parser.
/// Only complete quoted/unquoted URL attributes authorize a blocker. Opaque
/// tags remain opaque even when their children look like Markdown.
fn inspect_html(
    source: &str,
    span: Range<usize>,
    context: &Context,
    plan: &mut LinkEditPlan,
    stack: &mut Vec<String>,
) {
    let bytes = source.as_bytes();
    let mut at = span.start;
    while at < span.end {
        if bytes[at] != b'<' {
            at += 1;
            continue;
        }
        if source[at..span.end].starts_with("<!--") {
            at = source[at + 4..span.end]
                .find("-->")
                .map(|i| at + 4 + i + 3)
                .unwrap_or(span.end);
            continue;
        }
        let mut i = at + 1;
        let closing = bytes.get(i) == Some(&b'/');
        if closing {
            i += 1;
        }
        let name_start = i;
        while i < span.end && html_name(bytes[i]) {
            i += 1;
        }
        if i == name_start {
            at += 1;
            continue;
        }
        let tag = source[name_start..i].to_ascii_lowercase();
        let mut complete = false;
        let mut self_closing = false;
        while i < span.end {
            i = whitespace(source, i, span.end);
            if bytes.get(i) == Some(&b'>') {
                complete = true;
                i += 1;
                break;
            }
            if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'>') {
                complete = true;
                self_closing = true;
                i += 2;
                break;
            }
            let start = i;
            while i < span.end && html_name(bytes[i]) {
                i += 1;
            }
            if i == start {
                i += 1;
                continue;
            }
            let attribute = source[start..i].to_ascii_lowercase();
            i = whitespace(source, i, span.end);
            if bytes.get(i) != Some(&b'=') {
                continue;
            }
            i = whitespace(source, i + 1, span.end);
            let quote = bytes.get(i).copied().filter(|b| matches!(*b, b'\'' | b'"'));
            if quote.is_some() {
                i += 1;
            }
            let value_start = i;
            while i < span.end
                && if let Some(quote) = quote {
                    bytes[i] != quote
                } else {
                    !bytes[i].is_ascii_whitespace() && bytes[i] != b'>'
                }
            {
                i += 1;
            }
            let value_end = i;
            if quote.is_some() && i == span.end {
                break;
            }
            if quote.is_some() {
                i += 1;
            }
            if matches!(
                attribute.as_str(),
                "href"
                    | "src"
                    | "xlink:href"
                    | "action"
                    | "formaction"
                    | "poster"
                    | "data"
                    | "cite"
                    | "longdesc"
                    | "background"
            ) {
                process_destination(
                    source,
                    value_start..value_end,
                    Syntax::Html,
                    true,
                    context,
                    plan,
                );
            }
        }
        if complete {
            if closing {
                if let Some(index) = stack.iter().rposition(|name| name == &tag) {
                    stack.truncate(index);
                }
            } else if !self_closing
                && !matches!(
                    tag.as_str(),
                    "area"
                        | "base"
                        | "br"
                        | "col"
                        | "embed"
                        | "hr"
                        | "img"
                        | "input"
                        | "link"
                        | "meta"
                        | "param"
                        | "source"
                        | "track"
                        | "wbr"
                )
            {
                stack.push(tag);
            }
        }
        at = i.max(at + 1);
    }
}

#[cfg(test)]
#[path = "document_refactor_links_tests.rs"]
mod tests;
