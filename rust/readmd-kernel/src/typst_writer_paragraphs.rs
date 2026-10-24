//! Child of the existing writer: native paragraph spacing, not a new renderer.
use super::*;
use crate::publication_typography::{self as typography, LineSpacing};

#[derive(Clone, Debug)]
pub(crate) struct ParagraphRequest {
    pub index: usize,
    pub id: Option<String>,
    pub requested: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct GeometryContext {
    pub marker: String,
    pub paragraphs: Vec<ParagraphRequest>,
    pub note_count: usize,
}
impl GeometryContext {
    pub(super) fn new() -> Self {
        Self { marker: format!("readmd-p04-{}:", uuid::Uuid::new_v4().simple()), paragraphs: Vec::new(), note_count: 0 }
    }

    /// Observe final native entry locations without changing their layout.
    /// Typst 0.15.1 can retain an insertion when an orphan group moves; an
    /// entry may then precede its callout. Long entries may continue on later
    /// pages, but their first page must match the original native declaration.
    pub(super) fn note_guard(&mut self, count: usize) -> String {
        self.note_count = count;
        format!("\n#context {{\n for (index,entry) in query(footnote.entry).enumerate() {{\n let actual=entry.location().page()\n let expected=entry.note.location().page()\n if actual != expected {{\n panic({} + json.encode((coordinateSpace:\"pdfFootnoteStartPage\",noteIndex:index+1,definitionPage:actual,referencePage:expected),pretty:false))\n }}\n }}\n}}\n", string(&self.marker))
    }

    /// Only an exact, generated diagnostic with this render's random marker
    /// and an existing paragraph request becomes a structured geometry error.
    /// Unknown warnings, source excerpts and malformed messages stay generic.
    pub(crate) fn diagnostic(&self, log: &str, generated_source: &Path, worker_dir: &Path) -> Option<String> {
        if !generated_source.is_absolute() || !worker_dir.is_absolute() { return None; }
        let normalize_path = |value: &str| {
            let value = value.strip_prefix(r"\\?\").unwrap_or(value).replace('\\', "/");
            if cfg!(windows) { value.to_ascii_lowercase() } else { value }
        };
        let expected_path = normalize_path(generated_source.to_str()?);
        // The caller supplies the same owned working directory passed to
        // spawn_managed. Accept a relative diagnostic only for its exact
        // generated filename, never by basename or path-prefix matching.
        if expected_path != normalize_path(worker_dir.join("document.typ").to_str()?) { return None; }
        let mut diagnostic = None;
        for line in log.lines() {
            // Typst 0.15.1 short diagnostics use an absolute source path or
            // precisely document.typ when run in the generated worker root,
            // print a string panic without JSON-quoting the complete message.
            // An unrelated or additional error prevents structured promotion.
            if !line.contains(": error: ") {
                if line.starts_with("error:") { return None; }
                continue;
            }
            if line.len() > 8192 || diagnostic.is_some() { return None; }
            let (location, message) = line.split_once(": error: ")?;
            let (location, source_column) = location.rsplit_once(':')?;
            let (source_path, source_line) = location.rsplit_once(':')?;
            if !(source_path == "document.typ" || normalize_path(source_path) == expected_path)
                || source_line.parse::<u32>().ok().filter(|n| *n > 0).is_none()
                || source_column.parse::<u32>().ok().filter(|n| *n > 0).is_none() { return None; }
            let json = message.strip_prefix("panicked with: ")?.strip_prefix(&self.marker)?;
            if json.len() > 4096 { return None; }
            let value = serde_json::from_str::<Value>(json).ok()?;
            if value["coordinateSpace"] == "pdfFootnoteStartPage" {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct NoteDetails {
                    coordinate_space: String, note_index: usize, definition_page: u32, reference_page: u32,
                }
                let details = serde_json::from_str::<NoteDetails>(json).ok()?;
                if details.coordinate_space != "pdfFootnoteStartPage"
                    || self.note_count == 0 || self.note_count > crate::publication_blocks::MAX_NODES
                    || details.note_index == 0 || details.note_index > self.note_count
                    || !(1..=100_000).contains(&details.definition_page)
                    || !(1..=100_000).contains(&details.reference_page)
                    || details.definition_page == details.reference_page { return None; }
                diagnostic = Some(format!("publication_note_layout: {}", serde_json::to_string(&value).ok()?));
                continue;
            }
            // A typed struct also rejects duplicate fields, unknown keys and
            // floats/strings where an index is required.
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Details {
                coordinate_space: String, block_index: usize, block_id: Option<String>, line: usize,
                mode: String, requested_pt: f64, required_pt: f64,
                available_width_pt: f64, available_height_pt: f64,
            }
            let details = serde_json::from_str::<Details>(json).ok()?;
            if details.coordinate_space != "pdfParagraphLine" || details.mode != "exact"
                || details.line != 1 { return None; }
            let request = self.paragraphs.iter().find(|p| p.index == details.block_index)?;
            if value.get("blockId").and_then(Value::as_str) != request.id.as_deref()
                || details.block_id.as_deref() != request.id.as_deref()
                || value.get("blockId").is_some_and(|v| !v.is_string()) { return None; }
            let requested = details.requested_pt;
            let required = details.required_pt;
            let width = details.available_width_pt;
            let height = details.available_height_pt;
            if ![requested, required, width, height].iter().all(|n| n.is_finite() && *n >= 0. && *n <= 20000.)
                || (requested - request.requested).abs() > 0.000001 || required <= requested
                || width <= 0. || height <= 0. { return None; }
            diagnostic = Some(format!("publication_layout_geometry: {}", serde_json::to_string(&value).ok()?));
        }
        diagnostic
    }
}

impl Writer<'_> {
    pub(super) fn special_blocks(&mut self, values: &[Block]) -> Result<String, String> {
        let ordinary = self.ordinary_body;
        let line = self.local_line;
        let baseline = self.baseline.take();
        let local_baseline = self.local_baseline.take();
        self.ordinary_body = false; self.local_line = None;
        let result = self.blocks(values);
        self.ordinary_body = ordinary; self.local_line = line;
        self.baseline = baseline; self.local_baseline = local_baseline;
        result
    }

    /// A text-only projection follows the same actual style routing. It does
    /// not emit a second SVG, note, anchor, counter or resource operation.
    fn paragraph_metric(&mut self, values: &[Inline]) -> Result<String, String> {
        let mut out = String::new();
        for value in values {
            if crate::speech::cancelled() { return Err("cancelled".into()); }
            let body = match value {
                Inline::Text(value) => self.author_text(value,false)?,
                Inline::Code(value) => self.author_text(value,true)?,
                Inline::Strong(values) => format!("#strong[{}]", self.paragraph_metric(values)?),
                Inline::Emph(values) => format!("#emph[{}]", self.paragraph_metric(values)?),
                Inline::Strike(values) => format!("#strike[{}]", self.paragraph_metric(values)?),
                Inline::Styled {format, children} => {
                    let body = self.paragraph_metric(children)?;
                    self.styled_inline(format, body)?
                },
                Inline::AuthorStyled{style,children}=>{let previous=self.author;self.author=Some(style.resolve(&previous.unwrap_or(self.author_base),&self.author_base));let body=self.paragraph_metric(children);self.author=previous;body?},
                Inline::Citation {children, ..} | Inline::Link {children, ..} => self.paragraph_metric(children)?,
                Inline::SoftBreak | Inline::HardBreak | Inline::Math(_) | Inline::OfficeMath {..} => text(" "),
                Inline::Html(value) if value.trim().is_empty() || value.trim().starts_with("<!--") => String::new(),
                _ => return Err(typography::UNSUPPORTED.into()),
            };
            out.push_str(&body);
            if out.len() > 16 * 1024 * 1024 { return Err("typst_output_limit".into()); }
        }
        Ok(out)
    }

    pub(super) fn typed_paragraph(&mut self, values: &[Inline], mode: LineSpacing) -> Result<String, String> {
        typography::validate_typst_inlines(values)?;
        self.paragraph_index = self.paragraph_index.checked_add(1).ok_or("typst_output_limit")?;
        if self.paragraph_index > crate::publication_blocks::MAX_NODES { return Err(typography::INVALID.into()); }
        // Collect actual SVG extents during the only visible rendering pass.
        self.inline_object_bounds = Some((0., 0.));
        let visible = self.inlines(values);
        let (top, bottom) = self.inline_object_bounds.take().unwrap_or((0., 0.));
        let visible = visible?;
        let metric = self.paragraph_metric(values)?;
        let size = self.local.as_ref().map_or_else(|| n(&self.style["typography"]["size"], 11., 6., 72.), |local| local.size);
        let (name, requested) = match mode {
            LineSpacing::Multiple(multiple) => ("multiple", multiple * size),
            LineSpacing::ExactPt(pt) => ("exact", pt),
            LineSpacing::AtLeastPt(pt) => ("atLeast", pt),
        };
        let (_, height) = crate::export_styles::page_dimensions(self.style);
        let body_height = (height - n(&self.style["page"]["marginTop"], 20., 0., 60.)
            - n(&self.style["page"]["marginBottom"], 20., 0., 60.)) * 72. / 25.4;
        let after = if self.local.is_some() { 0. } else { n(&self.style["typography"]["spacing"], 6., 0., 60.) };
        let geometry = self.geometry.get_or_insert_with(GeometryContext::new);
        if matches!(mode, LineSpacing::ExactPt(_)) {
            geometry.paragraphs.push(ParagraphRequest { index: self.paragraph_index, id: self.local_id.clone(), requested });
        }
        Ok(format!("#readmd-typed-paragraph(metric:[{metric}],mode:{},requested:{requested}pt,base-size:{size}pt,object-top:{top}pt,object-bottom:{bottom}pt,after:{after}pt,marker:{},block-index:{},block-id:{},body-height:{body_height}pt)[{visible}]\n",
            string(name), string(&geometry.marker), self.paragraph_index,
            self.local_id.as_deref().map(string).unwrap_or_else(|| "none".into())))
    }
}
