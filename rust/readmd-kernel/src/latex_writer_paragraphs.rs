//! Child of the existing TeX writer; only opt-in ordinary body paragraphs.
use super::*;
use crate::publication_typography::{self as typography, LineSpacing};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) const MACROS: &str = include_str!("latex_writer_paragraphs.tex");

#[derive(Clone, Debug)]
pub(crate) struct ParagraphRequest {
    pub index: usize,
    pub id: Option<String>,
    pub mode: LineSpacing,
    pub requested: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct GeometryContext {
    marker: String,
    paragraphs: Vec<ParagraphRequest>,
    source_hash: Option<[u8; 32]>,
    worker_directory: Option<std::path::PathBuf>,
    worker_directory_alias: Option<std::path::PathBuf>,
}

impl GeometryContext {
    fn new() -> Self {
        Self {
            marker: format!("readmd-p04-tex-{}:", uuid::Uuid::new_v4().simple()),
            paragraphs: Vec::new(),
            source_hash: None,
            worker_directory: None,
            worker_directory_alias: None,
        }
    }

    pub(super) fn bind_source(&mut self, source: &str) {
        self.source_hash = Some(Sha256::digest(source.as_bytes()).into());
    }

    pub(super) fn marker_for_source(&self) -> &str { &self.marker }

    pub(crate) fn bind_worker_directory(&mut self, directory: &Path) -> Result<(), String> {
        if !directory.is_absolute() { return Err("latex_workspace_failed".into()); }
        let canonical = dunce::canonicalize(directory).map_err(|_|"latex_workspace_failed")?;
        self.worker_directory = Some(canonical);
        self.worker_directory_alias = Some(directory.to_owned());
        Ok(())
    }

    /// A private render context cannot be supplied through options/Markdown.
    /// The source hash, random marker, exact generated filename, worker cwd,
    /// registered request and one complete packet are all required. Ordinary
    /// engine errors and user text never become trusted geometry diagnostics.
    pub(crate) fn diagnostic(&self, log: &str, source: &str, worker_dir: &Path) -> Option<String> {
        let actual_hash: [u8; 32] = Sha256::digest(source.as_bytes()).into();
        if self.source_hash != Some(actual_hash) || !worker_dir.is_absolute()
            || log.len() > 4 * 1024 * 1024 { return None; }
        let normalize = |s: &str| {
            let s = s.strip_prefix(r"\\?\").unwrap_or(s).replace('\\', "/");
            if cfg!(windows) { s.to_ascii_lowercase() } else { s }
        };
        let bound = self.worker_directory.as_ref()?;
        let actual_worker = dunce::canonicalize(worker_dir).ok()?;
        if normalize(actual_worker.to_str()?) != normalize(bound.to_str()?) { return None; }
        // Temp paths may be a junction, an 8.3 alias or /var on macOS. Only
        // accept the exact private spellings supplied at bind/call time and
        // their canonical directory, never resolve a path supplied by a log.
        let expected = [bound.as_path(), self.worker_directory_alias.as_ref()?.as_path(), worker_dir]
            .iter().map(|directory| directory.join("document.tex").to_str().map(&normalize))
            .collect::<Option<Vec<_>>>()?;
        let mut fields = Vec::new();
        let mut began = false;
        let mut ended = false;
        let mut error = false;
        for line in log.lines() {
            if line.starts_with("Missing character:") || line.starts_with('!') { return None; }
            if let Some(value) = line.strip_prefix(&self.marker) {
                let value = value.strip_prefix(' ')?;
                if value == "BEGIN" {
                    if began || ended { return None; }
                    began = true;
                } else if value == "END" {
                    if !began || ended || fields.len() != 7 { return None; }
                    ended = true;
                } else {
                    if !began || ended || value.len() > 64 { return None; }
                    fields.push(value);
                }
                continue;
            }
            // -file-line-error emits document.tex, ./document.tex, or the
            // absolute generated path, depending on the locked engine/cwd.
            if let Some((location, message)) = line.split_once(": Package ") {
                let (path, number) = location.rsplit_once(':')?;
                if !ended || error || number.parse::<usize>().ok().filter(|n| *n > 0).is_none()
                    || !(matches!(path, "document.tex" | "./document.tex") || expected.contains(&normalize(path)))
                    || message != "ReadMD Error: readmd-p04-geometry." { return None; }
                error = true;
            } else if line.contains(": LaTeX Error:") || line.contains(": Undefined control sequence.")
                || line.starts_with("Fatal error") { return None; }
        }
        if !began || !ended || !error { return None; }
        const KEYS: [&str; 7] = ["index", "mode", "requested", "required", "width", "height", "reserved"];
        // The packet has six dimensions/identity values; the seventh field is
        // an explicit protocol version. Ordered fields forbid duplicates.
        let mut numbers = [0u64; 7];
        for (i, field) in fields.iter().enumerate() {
            let (key, value) = field.split_once('=')?;
            if key != KEYS[i] || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) { return None; }
            numbers[i] = value.parse().ok()?;
        }
        if numbers[6] != 1 { return None; }
        let request = self.paragraphs.iter().find(|p| p.index as u64 == numbers[0])?;
        let mode = match request.mode { LineSpacing::Multiple(_) => 0, LineSpacing::ExactPt(_) => 1, LineSpacing::AtLeastPt(_) => 2 };
        if numbers[1] != mode { return None; }
        let bp = |sp: u64| sp as f64 / 65536. * 72. / 72.27;
        let requested = bp(numbers[2]);
        let required = bp(numbers[3]);
        let width = bp(numbers[4]);
        let height = bp(numbers[5]);
        if ![requested, required, width, height].iter().all(|n| n.is_finite() && *n > 0. && *n <= 20000.)
            || (requested - request.requested).abs() > 2. / 65536.
            || !(required > height || (mode == 1 && required > requested)) { return None; }
        let mut details = serde_json::json!({
            "coordinateSpace":"pdfParagraphLine", "blockIndex":request.index,
            "line":1, "mode":match mode {0=>"multiple",1=>"exact",_=>"atLeast"},
            "requestedPt":request.requested, "requiredPt":required,
            "availableWidthPt":width, "availableHeightPt":height,
        });
        if let Some(id) = &request.id { details["blockId"] = serde_json::Value::String(id.clone()); }
        Some(format!("publication_layout_geometry: {details}"))
    }
}

#[derive(Default)]
pub(super) struct State {
    pub error: Option<String>,
    pub geometry: Option<GeometryContext>,
    pub ordinary: bool,
    pub line: Option<LineSpacing>,
    pub size: Option<f64>,
    pub id: Option<String>,
    index: usize,
    front: std::collections::HashSet<usize>,
}
impl State {
    pub(super) fn new(doc: &Document, front_count: usize) -> Self {
        Self { ordinary: true, front: doc.blocks.iter().take(front_count)
            .map(|b| b as *const Block as usize).collect(), ..Self::default() }
    }
    pub(super) fn is_front(&self, block: &Block) -> bool { self.front.contains(&(block as *const Block as usize)) }
    pub(super) fn fail(&mut self, error: impl Into<String>) {
        if self.error.is_none() { self.error = Some(error.into()); }
    }
}

impl Writer<'_> {
    pub(super) fn typed_paragraph(&mut self, values: &[Inline], mode: LineSpacing) -> Result<String, String> {
        typography::validate_tex_inlines(values, !self.bib_engine.is_empty())?;
        self.paragraphs.index += 1;
        if self.paragraphs.index > crate::publication_blocks::MAX_NODES { return Err(typography::INVALID.into()); }
        let size = self.paragraphs.size.unwrap_or_else(|| self.publication_style.as_ref()
            .and_then(|v| v.pointer("/typography/size")).and_then(serde_json::Value::as_f64).unwrap_or(11.));
        let (kind, requested) = match mode {
            LineSpacing::Multiple(factor) => (0, factor * size),
            LineSpacing::ExactPt(pt) => (1, pt),
            LineSpacing::AtLeastPt(pt) => (2, pt),
        };
        let visible = self.inlines(values);
        // Rust renders the pure inline tree once, retaining adjacent-text
        // shaping, actual style, URL font and Unicode. The engine measures the
        // same token list in a discarded box; it is never shipped twice.
        if visible.len() > 16 * 1024 * 1024 { return Err("latex_output_limit".into()); }
        if crate::speech::cancelled() { return Err("cancelled".into()); }
        let metric = &visible;
        let geometry = self.paragraphs.geometry.get_or_insert_with(GeometryContext::new);
        geometry.paragraphs.push(ParagraphRequest {
            index: self.paragraphs.index, id: self.paragraphs.id.clone(), mode, requested,
        });
        Ok(format!("\\readmdtypedparagraph{{{}}}{{{kind}}}{{{requested:.8}}}{{{size:.8}}}{{{metric}}}{{{visible}}}\n\n", self.paragraphs.index))
    }
}

#[cfg(test)]
#[path = "latex_writer_paragraphs_tests.rs"]
mod tests;
