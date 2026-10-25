//! Typed DOCX/Rust PDF body and local paragraph line spacing.
//! Unsupported consumers reject requests before resources or output staging.
use serde_json::Value;
#[path = "publication_tracking.rs"]
pub(crate) mod tracking;
#[path = "publication_baseline.rs"]
pub(crate) mod baseline;
#[path = "publication_decoration.rs"]
pub(crate) mod decoration;

// Reuse the existing invalid-style code; a distinct capability error keeps
// unsupported output from being mistaken for a lossy successful projection.
pub const INVALID: &str = "publication_local_style_invalid";
pub const UNSUPPORTED: &str = crate::api_codes::PUBLICATION_LINE_SPACING_UNSUPPORTED;
const LEGACY: &str = "_publicationLineSpacingLegacy";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineSpacing {
    Multiple(f64),
    ExactPt(f64),
    AtLeastPt(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    GlobalBody,
    LocalBlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocxLine {
    pub line: u32,
    pub rule: &'static str,
}

impl LineSpacing {
    /// Word's auto value is 240ths of a line; exact/atLeast use 1/20pt.
    /// Validated positive finite inputs fit u32; never silently clamp.
    pub fn docx(self) -> DocxLine {
        match self {
            Self::Multiple(factor) => DocxLine { line: (factor * 240.).round() as u32, rule: "auto" },
            Self::ExactPt(pt) => DocxLine { line: (pt * 20.).round() as u32, rule: "exact" },
            Self::AtLeastPt(pt) => DocxLine { line: (pt * 20.).round() as u32, rule: "atLeast" },
        }
    }
}

fn number(value: &Value, min: f64, max: f64) -> Result<f64, &'static str> {
    value.as_f64().filter(|n| n.is_finite() && *n >= min && *n <= max).ok_or(INVALID)
}

pub fn parse_line_spacing(value: &Value, scope: Scope) -> Result<LineSpacing, &'static str> {
    let object = value.as_object().ok_or(INVALID)?;
    let mode = object.get("mode").and_then(Value::as_str).ok_or(INVALID)?;
    let field = match mode { "multiple" => "multiple", "exact" | "atLeast" => "pt", _ => return Err(INVALID) };
    if object.len() != 2 || !object.contains_key(field) {
        return Err(INVALID);
    }
    let value = object.get(field).ok_or(INVALID)?;
    match mode {
        "multiple" => Ok(LineSpacing::Multiple(number(value, 1., if scope == Scope::GlobalBody { 2.5 } else { 3. })?)),
        "exact" => Ok(LineSpacing::ExactPt(number(value, 6., 144.)?)),
        "atLeast" => Ok(LineSpacing::AtLeastPt(number(value, 6., 144.)?)),
        _ => Err(INVALID),
    }
}

/// Validate the *sparse original request*, before defaults inject lineHeight.
/// Missing/null removes this layer's typed override. Source declarations do
/// not persist null; publication_blocks::apply already removes patched keys.
pub fn validate_request(options: &Value) -> Result<(), &'static str> {
    tracking::validate_request(options)?;
    baseline::validate_request(options)?;
    // Reserved former draft key is never accepted from any JSON channel.
    if options.get(LEGACY).is_some() { return Err(INVALID); }
    let typography = &options["typography"];
    if let Some(raw) = typography.get("lineSpacing").filter(|v| !v.is_null()) {
        parse_line_spacing(raw, Scope::GlobalBody)?;
        if typography.get("lineHeight").is_some_and(|v| !v.is_null()) {
            return Err(INVALID);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LineContext { legacy_multiple: f64 }
impl Default for LineContext { fn default() -> Self { Self { legacy_multiple: 1.6 } } }
impl LineContext { pub(crate) fn legacy_multiple(self) -> f64 { self.legacy_multiple } }

/// Internal normalization structure: provenance never travels in client JSON.
/// Canonical fields are still validated at every consumer; this is not a
/// validation bypass or an authority flag.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedOptions { canonical: Value, line: LineContext }
impl ResolvedOptions {
    pub(crate) fn from_request(raw: &Value) -> Result<Self, &'static str> {
        let empty=serde_json::json!({});
        let raw=if raw.is_null(){&empty}else{raw};
        validate_request(raw)?;
        crate::publication_note_policies::parse_layer(raw).map_err(|e| e.code)?;
        let complete = crate::export_styles::sanitize(raw);
        let multiple = complete["typography"]["lineHeight"].as_f64().unwrap_or(1.6);
        Ok(Self { canonical: raw.clone(), line: LineContext { legacy_multiple: multiple } })
    }
    pub(crate) fn values(&self) -> &Value { &self.canonical }
    pub(crate) fn line_context(&self) -> LineContext { self.line }
    pub(crate) fn with_completed_values(mut self, values: Value) -> Result<Self, &'static str> {
        validate_request(&values)?;
        self.canonical = values; Ok(self)
    }
    pub(crate) fn merge(&self, sparse: &Value) -> Result<Self, crate::publication_metadata::MetadataError> {
        let canonical = crate::publication_metadata::merge_options(&self.canonical, sparse)?;
        let line = if sparse.pointer("/typography/lineHeight").is_some_and(|v| !v.is_null()) {
            Self::from_request(sparse).map_err(|code| crate::publication_metadata::MetadataError {
                code: code.into(), path: request_error_field(sparse).into(), from: 0, to: 0,
            })?.line
        } else { self.line };
        Ok(Self { canonical, line })
    }
}

/// Preserve the first failing sparse field in preset and merge diagnostics.
pub(crate) fn request_error_field(options: &Value) -> &'static str {
    if tracking::validate_request(options).is_err() { "typography.trackingPt" }
    else if baseline::validate_request(options).is_err() { "typography.baselineShiftPt" }
    else { "typography.lineSpacing" }
}
pub(crate) fn merge_error_field(base: &Value, over: &Value) -> &'static str {
    request_error_field(if validate_request(base).is_err() { base } else { over })
}

pub fn normalize_completed_style(style: &mut Value) {
    if style.pointer("/typography/lineSpacing").is_some_and(|v| !v.is_null()) {
        if let Some(typography) = style.get_mut("typography").and_then(Value::as_object_mut) { typography.remove("lineHeight"); }
    }
}

/// Validate each sparse layer before merging. Replace the complete tagged
/// union, not its keys; explicit old lineHeight cancels inherited typed mode.
/// A null new field inherits the lower layer rather than resurrecting defaults.
pub fn merge_layers(base: &Value, over: &Value) -> Result<(Value, Value), &'static str> {
    validate_request(base)?;
    validate_request(over)?;
    global_line_spacing(base)?;
    let mut base = base.clone(); let mut over = over.clone();
    tracking::inherit_null_overlay(&mut over);
    baseline::inherit_null_overlay(&mut over);
    if let Some(raw) = over.pointer("/typography/lineSpacing").cloned() {
        if raw.is_null() {
            if let Some(values) = over.get_mut("typography").and_then(Value::as_object_mut) { values.remove("lineSpacing"); }
        } else {
            if let Some(values) = base.get_mut("typography").and_then(Value::as_object_mut) { values.remove("lineSpacing"); values.remove("lineHeight"); }
        }
    }
    if over.pointer("/typography/lineHeight").is_some_and(|v| !v.is_null()) {
        if let Some(values) = base.get_mut("typography").and_then(Value::as_object_mut) { values.remove("lineSpacing"); }
    }
    Ok((base, over))
}

pub fn validate_input(source: &str, options: &Value) -> Result<bool, &'static str> {
    let (line_spacing, decoration) = validate_input_usage(source, options)?;
    Ok(line_spacing || decoration)
}

fn validate_input_usage(source: &str, options: &Value) -> Result<(bool, bool), &'static str> {
    validate_request(options)?;
    let mut line_spacing = global_line_spacing(options)?.is_some();
    let mut decoration = false;
    if source.contains(crate::publication_blocks::PREFIX) {
        crate::publication_blocks::validate_source(source)?;
        for node in &crate::publication_blocks::analyze(source)?.nodes {
            line_spacing |= node.style["paragraph"].get("lineSpacing").is_some();
            decoration |= decoration::parse(&node.style["paragraph"])?.is_some();
        }
    }
    Ok((line_spacing, decoration))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Consumer { Docx, RustPdf, Typst, Tex, Flow, Unsupported }

/// Actual consumer identity cannot be overridden by the client's engine key.
pub(crate) fn validate_consumer(consumer: Consumer, source: &str, options: &Value) -> Result<(), &'static str> {
    if consumer==Consumer::Docx {crate::publication_blocks::validate_author_consumer(source,true)?;}
    else if consumer==Consumer::Unsupported {crate::publication_blocks::validate_author_consumer(source,false)?;}
    else {crate::publication_blocks::validate_author_render_consumer(source)?;}
    tracking::validate_consumer(source, options, matches!(consumer, Consumer::Docx))?;
    baseline::validate_source(source, options, match consumer {
        Consumer::Docx => baseline::Consumer::Docx,
        Consumer::RustPdf => baseline::Consumer::RustPdf,
        Consumer::Typst => baseline::Consumer::Typst,
        _ => baseline::Consumer::Unsupported,
    })?;
    decoration::validate_source_consumer(match consumer {
        Consumer::Docx => decoration::Consumer::Docx,
        Consumer::RustPdf => decoration::Consumer::RustPdf,
        _ => decoration::Consumer::Unsupported,
    }, source)?;
    if !validate_input(source, options)? { return Ok(()); }
    match consumer {
        Consumer::Docx => Ok(()),
        Consumer::RustPdf => validate_rust_pdf_document(&crate::md_ast::parse(source)),
        Consumer::Typst => validate_typst_document(&crate::md_ast::parse(source), options),
        Consumer::Tex => validate_tex_document(&crate::md_ast::parse(source), options),
        Consumer::Flow | Consumer::Unsupported => Err(UNSUPPORTED),
    }
}

/// Must run before resources, capability leases, staging and native choosers.
pub fn validate_export(format: &str, source: &str, options: &Value) -> Result<(), &'static str> {
    let consumer = if format.eq_ignore_ascii_case("docx") { Consumer::Docx }
        else if format.eq_ignore_ascii_case("pdf") && matches!(options.pointer("/pdf/engine").and_then(Value::as_str), None | Some("rust")) { Consumer::RustPdf }
        else if format.eq_ignore_ascii_case("pdf") && options.pointer("/pdf/engine").and_then(Value::as_str)==Some("typst") { Consumer::Typst }
        else if matches!(format.to_ascii_lowercase().as_str(), "tex" | "latex")
            || (format.eq_ignore_ascii_case("pdf") && matches!(options.pointer("/pdf/engine").and_then(Value::as_str), Some("xelatex" | "pdflatex"))) { Consumer::Tex }
        else if matches!(format.to_ascii_lowercase().as_str(), "html" | "epub") { Consumer::Flow }
        else { Consumer::Unsupported };
    validate_consumer(consumer,source,options)
}

pub(crate) fn validate_pdf_preview(format: &str, source: &str, options: &Value) -> Result<(), &'static str> {
    if format == "pdf" && matches!(options.pointer("/pdf/engine").and_then(Value::as_str), None | Some("rust")) {
        validate_consumer(Consumer::RustPdf,source,options)
    } else if format == "pdf" && options.pointer("/pdf/engine").and_then(Value::as_str)==Some("typst") {
        validate_consumer(Consumer::Typst,source,options)
    } else if format == "pdf" && matches!(options.pointer("/pdf/engine").and_then(Value::as_str), Some("xelatex" | "pdflatex")) {
        validate_consumer(Consumer::Tex,source,options)
    } else { validate_preview(source,options) }
}

/// M2 adds ordinary paragraph/heading/caption consumers. New local spacing in
/// container stories is rejected before resources; a global body value does
/// not alter their existing special-story geometry.
pub(crate) fn validate_rust_pdf_document(doc: &crate::md_ast::Document) -> Result<(), &'static str> {
    crate::publication_blocks::validate_author_render_document(doc)?;
    use crate::md_ast::Block;
    let mut pending = doc.blocks.iter().map(|b|(b,true)).collect::<Vec<_>>();
    let mut count = 0;
    while let Some((block,ordinary)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match block {
            Block::Styled { attributes, block } => {
                crate::publication_blocks::validate_style(attributes)?;
                if attributes["paragraph"].get("lineSpacing").is_some() {
                    count += 1;
                    if count > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
                    if !ordinary || !matches!(block.unstyled(),Block::Paragraph(_) | Block::Heading{..} | Block::Caption{..}) { return Err(UNSUPPORTED); }
                    if matches!(block.unstyled(),Block::Paragraph(inlines) if inlines.len()==1&&matches!(inlines.first(),Some(crate::md_ast::Inline::Image{..}))) {
                        return Err(UNSUPPORTED);
                    }
                }
                pending.push((block.as_ref(),ordinary));
            }
            Block::Quote(blocks) | Block::FootnoteDef{blocks,..} => pending.extend(blocks.iter().map(|b|(b,false))),
            Block::List{items,..} => for item in items { pending.extend(item.blocks.iter().map(|b|(b,false))); },
            _ => (),
        }
    }
    Ok(())
}

/// Ordinary top-level body paragraphs only. Actual parents decide scope;
/// neither a JSON hint nor the selected engine may broaden this capability.
pub(crate) fn validate_typst_document(doc: &crate::md_ast::Document, options: &Value) -> Result<(), &'static str> {
    crate::publication_blocks::validate_author_render_document(doc)?;
    tracking::validate_document(doc, options, false)?;
    baseline::validate_document(doc, options, baseline::Consumer::Typst)?;
    use crate::md_ast::Block;
    validate_request(options)?;
    let global = global_line_spacing(options)?;
    let mut pending = doc.blocks.iter().map(|block|(block,true,None)).collect::<Vec<_>>();
    let mut count = 0usize;
    while let Some((block,ordinary,inherited)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match block {
            Block::Styled {attributes, block} => {
                crate::publication_blocks::validate_style(attributes)?;
                let paragraph = ordinary && matches!(block.unstyled(), Block::Paragraph(_));
                if attributes["paragraph"].get("lineSpacing").is_some() && !paragraph { return Err(UNSUPPORTED); }
                let base = inherited.or(if paragraph {global} else {None});
                let mode = resolve_line_spacing(base, &attributes["paragraph"], paragraph)?;
                pending.push((block.as_ref(),ordinary,mode));
            },
            Block::Paragraph(values) if ordinary && inherited.or(global).is_some() => {
                count += 1;
                if count > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
                validate_typst_inlines(values)?;
            },
            Block::Quote(blocks) | Block::FootnoteDef {blocks, ..} => pending.extend(blocks.iter().map(|block|(block,false,None))),
            Block::List {items, ..} => for item in items { pending.extend(item.blocks.iter().map(|block|(block,false,None))); },
            _ => (),
        }
    }
    Ok(())
}

pub(crate) fn validate_typst_inlines(values: &[crate::md_ast::Inline]) -> Result<(), &'static str> {
    use crate::md_ast::Inline;
    use crate::publication_rich::{RichDecoration, RichVerticalAlign};
    let mut pending = values.iter().map(|value|(value,false)).collect::<Vec<_>>();
    while let Some((value, transformed)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match value {
            Inline::Text(_) | Inline::Code(_) | Inline::SoftBreak | Inline::HardBreak => (),
            Inline::Html(value) if value.trim().is_empty() || value.trim().starts_with("<!--") => (),
            Inline::Math(_) | Inline::OfficeMath {..} if !transformed => (),
            Inline::Strong(children) | Inline::Emph(children) | Inline::Strike(children)
                | Inline::AuthorStyled {children, ..} | Inline::Citation {children, ..} | Inline::Link {children, ..} => pending.extend(children.iter().map(|value|(value,transformed))),
            Inline::Styled {format, children} => {
                let transformed = transformed || format.vertical_align != RichVerticalAlign::Baseline
                    || format.text_decoration == RichDecoration::Underline;
                pending.extend(children.iter().map(|value|(value,transformed)));
            },
            _ => return Err(UNSUPPORTED),
        }
    }
    Ok(())
}

/// TeX's opt-in ordinary-body lane keeps native paragraph pagination.
/// The pure metric pass may inspect actual native line boxes, but cannot run
/// author commands, images, footnotes, native citation counters or other stories.
pub(crate) fn validate_tex_document(doc: &crate::md_ast::Document, options: &Value) -> Result<(), &'static str> {
    validate_tex_document_with_front(doc, options, 0)
}

pub(crate) fn validate_tex_document_with_front(doc: &crate::md_ast::Document, options: &Value, front_count: usize) -> Result<(), &'static str> {
    crate::publication_blocks::validate_author_render_document(doc)?;
    use crate::md_ast::Block;
    tracking::validate_document(doc, options, false)?;
    baseline::validate_document(doc, options, baseline::Consumer::Unsupported)?;
    validate_request(options)?;
    let global = global_line_spacing(options)?;
    let native_citations = options.pointer("/tex/bibEngine").or_else(||options.pointer("/latex/bibEngine"))
        .and_then(Value::as_str).is_some_and(|s| !s.is_empty())
        && options.pointer("/tex/renderBibliography").or_else(||options.pointer("/latex/renderBibliography")).and_then(Value::as_bool) != Some(false);
    // An external bibliography engine may execute author bibliography macros.
    // It cannot share a trusted generated paragraph-diagnostic protocol yet.
    if native_citations { return Err(UNSUPPORTED); }
    let mut pending = doc.blocks.iter().enumerate().map(|(i,b)|(b,i>=front_count,None)).collect::<Vec<_>>();
    let mut count = 0;
    while let Some((block, ordinary, inherited)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match block {
            Block::Styled {attributes,block} => {
                crate::publication_blocks::validate_style(attributes)?;
                let paragraph = ordinary && matches!(block.unstyled(),Block::Paragraph(_));
                if attributes["paragraph"].get("lineSpacing").is_some() && !paragraph { return Err(UNSUPPORTED); }
                let mode = resolve_line_spacing(inherited.or(if paragraph {global} else {None}), &attributes["paragraph"], paragraph)?;
                pending.push((block,ordinary,mode));
            },
            Block::Paragraph(values) => {
                validate_tex_metric_commands(values)?;
                if ordinary && inherited.or(global).is_some() {
                    count += 1;
                    if count > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
                    validate_tex_inlines(values,native_citations)?;
                }
            },
            Block::Heading {inlines,..}|Block::Caption {inlines,..} => validate_tex_metric_commands(inlines)?,
            Block::Table {header,rows,..} => {
                for row in std::iter::once(header).chain(rows) { for cell in row { validate_tex_metric_commands(cell)?; } }
            },
            Block::Math(value) if !tex_metric_math_is_pure(value) => return Err(UNSUPPORTED),
            Block::OfficeMath {..} => return Err(UNSUPPORTED),
            Block::Quote(blocks) | Block::FootnoteDef {blocks,..} => pending.extend(blocks.iter().map(|b|(b,false,None))),
            Block::List {items,..} => for item in items { pending.extend(item.blocks.iter().map(|b|(b,false,None))); },
            _ => (),
        }
    }
    Ok(())
}

fn validate_tex_metric_commands(values: &[crate::md_ast::Inline]) -> Result<(), &'static str> {
    use crate::md_ast::Inline;
    let mut pending=values.iter().collect::<Vec<_>>();let mut count=0;
    while let Some(value)=pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        count+=1;if count>crate::publication_blocks::MAX_NODES { return Err(INVALID); }
        match value {
            Inline::Math(value) if !tex_metric_math_is_pure(value) => return Err(UNSUPPORTED),
            Inline::OfficeMath {..} => return Err(UNSUPPORTED),
            Inline::Strong(children)|Inline::Emph(children)|Inline::Strike(children)|Inline::Styled{children,..}
                |Inline::Display{children,..}|Inline::Citation{children,..}|Inline::Link{children,..}=>pending.extend(children),
            _=>(),
        }
    }
    Ok(())
}

pub(crate) fn validate_tex_inlines(values: &[crate::md_ast::Inline], native_citations: bool) -> Result<(), &'static str> {
    use crate::md_ast::Inline;
    let mut pending = values.iter().collect::<Vec<_>>();
    let mut count = 0;
    while let Some(value) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        count += 1;
        if count > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
        match value {
            Inline::Text(_) | Inline::Code(_) | Inline::SoftBreak | Inline::HardBreak => (),
            Inline::Html(value) if value.trim().is_empty() || value.trim().starts_with("<!--") => (),
            Inline::Math(value) if tex_metric_math_is_pure(value) => (),
            // TeX's legacy OMML translation does not yet preserve explicit
            // physical run sizes. Do not certify that lane through a new mode.
            Inline::OfficeMath {..} => return Err(UNSUPPORTED),
            Inline::Citation {children,..} if !native_citations => pending.extend(children),
            Inline::Strong(children) | Inline::Emph(children) | Inline::Strike(children)
                | Inline::AuthorStyled {children,..} | Inline::Styled {children,..} | Inline::Link {children,..} => pending.extend(children),
            _ => return Err(UNSUPPORTED),
        }
    }
    Ok(())
}

/// A closed pure-math vocabulary prevents the metric pass from repeating
/// writes/counters/catcode changes or forging the task-bound error protocol.
/// Unknown commands remain available to the old untyped TeX export lane.
fn tex_metric_math_is_pure(value: &str) -> bool {
    if value.len() > 65536 || value.contains("^^") { return false; }
    let mut chars = value.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut command = String::new();
                while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) { command.push(chars.next().unwrap()); }
                if command.is_empty() {
                    if !chars.next().is_some_and(|c| "{}%#$&_ ,;:!|".contains(c)) { return false; }
                } else if !matches!(command.as_str(),
                    "frac"|"dfrac"|"tfrac"|"cfrac"|"sqrt"|"binom"|"dbinom"|"tbinom"|"genfrac"|
                    "overline"|"underline"|"widehat"|"widetilde"|"hat"|"tilde"|"bar"|"vec"|"dot"|"ddot"|
                    "acute"|"grave"|"breve"|"check"|"overbrace"|"underbrace"|"overset"|"underset"|"stackrel"|
                    "left"|"right"|"middle"|"big"|"Big"|"bigg"|"Bigg"|"bigl"|"bigr"|"Bigl"|"Bigr"|"biggl"|"biggr"|"Biggl"|"Biggr"|
                    "langle"|"rangle"|"lceil"|"rceil"|"lfloor"|"rfloor"|"lbrace"|"rbrace"|"vert"|"Vert"|
                    "sum"|"prod"|"coprod"|"int"|"iint"|"iiint"|"oint"|"lim"|"inf"|"sup"|"min"|"max"|"log"|"ln"|"exp"|
                    "sin"|"cos"|"tan"|"cot"|"sec"|"csc"|"sinh"|"cosh"|"tanh"|"arcsin"|"arccos"|"arctan"|
                    "alpha"|"beta"|"gamma"|"delta"|"epsilon"|"varepsilon"|"zeta"|"eta"|"theta"|"vartheta"|
                    "iota"|"kappa"|"lambda"|"mu"|"nu"|"xi"|"pi"|"varpi"|"rho"|"varrho"|"sigma"|"varsigma"|"tau"|"upsilon"|"phi"|"varphi"|"chi"|"psi"|"omega"|
                    "Gamma"|"Delta"|"Theta"|"Lambda"|"Xi"|"Pi"|"Sigma"|"Upsilon"|"Phi"|"Psi"|"Omega"|
                    "infty"|"partial"|"nabla"|"ell"|"hbar"|"imath"|"jmath"|"Re"|"Im"|"aleph"|
                    "times"|"cdot"|"div"|"pm"|"mp"|"le"|"leq"|"ge"|"geq"|"ne"|"neq"|"approx"|"sim"|"simeq"|"equiv"|"propto"|
                    "in"|"notin"|"subset"|"supset"|"subseteq"|"supseteq"|"cup"|"cap"|"setminus"|"emptyset"|"varnothing"|
                    "to"|"mapsto"|"rightarrow"|"leftarrow"|"leftrightarrow"|"Rightarrow"|"Leftarrow"|"Leftrightarrow"|
                    "forall"|"exists"|"neg"|"land"|"lor"|"wedge"|"vee"|"oplus"|"otimes"|"perp"|"parallel"|
                    "ldots"|"cdots"|"vdots"|"ddots"|"dots"|"quad"|"qquad"|"enspace"|"thinspace"|"medspace"|"thickspace"|"negthinspace"|
                    "displaystyle"|"textstyle"|"scriptstyle"|"scriptscriptstyle"|"limits"|"nolimits"|
                    "mathrm"|"mathbf"|"mathit"|"mathsf"|"mathtt"|"mathbb"|"mathcal"|"mathfrak"|"boldsymbol"|
                    "text"|"textrm"|"textit"|"textbf"|"operatorname") { return false; }
            },
            '{' => { depth += 1; if depth > 64 { return false; } },
            '}' => { let Some(n) = depth.checked_sub(1) else { return false; }; depth = n; },
            '%'|'#'|'$'|'&' => return false,
            c if c.is_control() && !c.is_whitespace() => return false,
            _ => (),
        }
    }
    depth == 0
}

/// Reader-only CSS is a different consumer from exported HTML or a physical
/// PDF preview. It permits bounded local ordinary-body tracking and paragraph
/// decoration, never a global Word request or typed physical line spacing.
pub fn validate_reader_view(source: &str, options: &Value) -> Result<(), &'static str> {
    crate::publication_blocks::validate_author_render_consumer(source)?;
    tracking::validate_reader(source, options)?;
    baseline::validate_source(source, options, baseline::Consumer::Reader)?;
    decoration::validate_source_consumer(decoration::Consumer::Reader, source)?;
    if validate_input_usage(source, options)?.0 { return Err(UNSUPPORTED); }
    Ok(())
}

/// Even DOCX preview uses an unimplemented PDF projection. It cannot certify
/// native Word exact/atLeast spacing, nor reuse a prior approximate artifact.
pub fn validate_preview(source: &str, options: &Value) -> Result<(), &'static str> {
    crate::publication_blocks::validate_author_render_consumer(source)?;
    tracking::validate_consumer(source, options, false)?;
    baseline::validate_source(source, options, baseline::Consumer::Unsupported)?;
    // HTML-flow previews cannot carry paragraph decoration; refuse with the
    // dedicated code rather than dropping the keys into a success page.
    if decoration::used(source)? { return Err(decoration::UNSUPPORTED); }
    if validate_input(source, options)? { return Err(UNSUPPORTED); }
    Ok(())
}

/// Read a completed style without mistaking defaulted lineHeight for an
/// explicitly conflicting request. The caller must first validate_request on
/// the raw request. Malformed typed values remain errors after completion.
pub fn global_line_spacing(style: &Value) -> Result<Option<LineSpacing>, &'static str> {
    style["typography"].get("lineSpacing").filter(|v| !v.is_null())
        .map(|v| parse_line_spacing(v, Scope::GlobalBody)).transpose()
}

pub fn validate_local(paragraph: &Value) -> Result<(), &'static str> {
    if let Some(raw) = paragraph.get("lineSpacing") {
        parse_line_spacing(raw, Scope::LocalBlock)?;
        if paragraph.get("lineHeight").is_some() {
            return Err(INVALID);
        }
    }
    Ok(())
}

/// Local legacy lineHeight is an explicit override of an inherited typed mode.
/// None preserves the existing writer path and its byte-level old defaults.
/// Global lineSpacing applies only to ordinary BodyText paragraphs. Headings,
/// notes, lists, quotes, tables and CSL keep their existing story defaults.
pub fn resolve_line_spacing(global: Option<LineSpacing>, paragraph: &Value, inherit_body: bool)
    -> Result<Option<LineSpacing>, &'static str>
{
    validate_local(paragraph)?;
    if let Some(raw) = paragraph.get("lineSpacing") {
        return parse_line_spacing(raw, Scope::LocalBlock).map(Some);
    }
    if inherit_body && global.is_some() {
        if let Some(raw) = paragraph.get("lineHeight") {
            return Ok(Some(LineSpacing::Multiple(number(raw, 1., 3.)?)));
        }
        return Ok(global);
    }
    Ok(None)
}

/// Direct DOCX callers also reject malformed typed attributes before invoking
/// any image loader. Route-level source validation is still mandatory because
/// invalid readable declarations may deliberately remain raw HTML in the AST.
pub fn validate_docx_document(doc: &crate::md_ast::Document, style: &Value) -> Result<(), &'static str> {
    crate::publication_blocks::validate_author_document(doc, true)?;
    tracking::validate_document(doc, style, true)?;
    baseline::validate_document(doc, style, baseline::Consumer::Docx)?;
    validate_request(style)?;
    global_line_spacing(style)?;
    use crate::md_ast::Block;
    // Avoid introducing a new recursive depth limit for legacy documents.
    // Only the new declarations consume the existing authoring-node budget.
    let mut pending = doc.blocks.iter().collect::<Vec<_>>();
    let mut declarations = 0;
    while let Some(block) = pending.pop() {
        match block {
            Block::Styled { attributes, block } => {
                validate_local(&attributes["paragraph"])?;
                if attributes["paragraph"].get("lineSpacing").is_some() {
                    declarations += 1;
                    if declarations > crate::publication_blocks::MAX_NODES
                        || matches!(block.unstyled(), Block::Math(_) | Block::OfficeMath { .. }) {
                        return Err(INVALID);
                    }
                }
                pending.push(block.as_ref());
            }
            Block::Quote(inner) | Block::FootnoteDef { blocks: inner, .. } => pending.extend(inner),
            Block::List { items, .. } => for item in items { pending.extend(&item.blocks); },
            _ => (),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn p04_line_union_is_strict_and_preserves_scope_bounds() {
        assert_eq!(parse_line_spacing(&json!({"mode":"multiple","multiple":2.5}), Scope::GlobalBody), Ok(LineSpacing::Multiple(2.5)));
        assert_eq!(parse_line_spacing(&json!({"mode":"multiple","multiple":3}), Scope::LocalBlock), Ok(LineSpacing::Multiple(3.)));
        for scope in [Scope::GlobalBody, Scope::LocalBlock] {
            for pt in [6., 14.33, 144.] {
                assert_eq!(parse_line_spacing(&json!({"mode":"exact","pt":pt}), scope), Ok(LineSpacing::ExactPt(pt)));
                assert_eq!(parse_line_spacing(&json!({"mode":"atLeast","pt":pt}), scope), Ok(LineSpacing::AtLeastPt(pt)));
            }
        }
        for raw in [Value::Null, json!(14), json!({}), json!({"mode":"exact","pt":"14"}),
            json!({"mode":"atleast","pt":14}), json!({"mode":"exact","factor":1}),
            json!({"mode":"exact","pt":14,"factor":1}), json!({"mode":"exact","pt":14,"unknown":true}),
            json!({"mode":"exact","pt":5.99}), json!({"mode":"atLeast","pt":144.01}),
            json!({"mode":"multiple","multiple":0.99}), json!({"mode":"multiple","multiple":3.01})] {
            assert_eq!(parse_line_spacing(&raw, Scope::LocalBlock), Err(INVALID), "{raw}");
        }
        assert_eq!(parse_line_spacing(&json!({"mode":"multiple","multiple":3}), Scope::GlobalBody), Err(INVALID));
    }

    #[test]
    fn p04_raw_conflict_check_precedes_completed_style_defaults() {
        let request = json!({"typography":{"lineSpacing":{"mode":"exact","pt":14}}});
        assert!(validate_request(&request).is_ok());
        let mut completed = request.clone(); completed["typography"]["lineHeight"] = json!(1.6);
        assert_eq!(validate_request(&completed), Err(INVALID));
        assert_eq!(global_line_spacing(&completed), Ok(Some(LineSpacing::ExactPt(14.))));
        assert!(validate_request(&json!({"typography":{"lineSpacing":null,"lineHeight":1.6}})).is_ok());
        assert_eq!(global_line_spacing(&json!({"typography":{"lineSpacing":null}})), Ok(None));
        assert_eq!(validate_local(&json!({"lineSpacing":null})), Err(INVALID));
    }

    #[test]
    fn p04_local_legacy_multiplier_overrides_inherited_exact_without_flattening_contexts() {
        let global = Some(LineSpacing::ExactPt(18.));
        assert_eq!(resolve_line_spacing(global, &json!({}), true), Ok(global));
        assert_eq!(resolve_line_spacing(global, &json!({"lineHeight":2}), true), Ok(Some(LineSpacing::Multiple(2.))));
        assert_eq!(resolve_line_spacing(global, &json!({}), false), Ok(None));
        assert_eq!(resolve_line_spacing(None, &json!({"lineHeight":2}), true), Ok(None));
        assert_eq!(resolve_line_spacing(global, &json!({"lineSpacing":{"mode":"atLeast","pt":20}}), false), Ok(Some(LineSpacing::AtLeastPt(20.))));
        assert_eq!(resolve_line_spacing(global, &json!({"lineSpacing":{"mode":"exact","pt":14},"lineHeight":1}), true), Err(INVALID));
    }

    #[test]
    fn p04_docx_native_line_units_do_not_confuse_twips_and_line_multiples() {
        for (value, line, rule) in [(LineSpacing::Multiple(1.25),300,"auto"),
            (LineSpacing::ExactPt(18.),360,"exact"),(LineSpacing::AtLeastPt(14.33),287,"atLeast")] {
            assert_eq!(value.docx(), DocxLine { line, rule });
        }
    }

    #[test]
    fn p04_sparse_layers_replace_union_and_preserve_internal_legacy_context() {
        let base = ResolvedOptions::from_request(&json!({"typography":{"lineHeight":1.55},"meta":{"publisher":"Parent"}})).unwrap();
        let exact = base.merge(&json!({"typography":{"lineSpacing":{"mode":"exact","pt":18}}})).unwrap();
        assert_eq!(exact.line_context().legacy_multiple(), 1.55);
        assert!(exact.values()["typography"].get("lineHeight").is_none());
        assert_eq!(global_line_spacing(exact.values()), Ok(Some(LineSpacing::ExactPt(18.))));
        let complete = crate::export_styles::sanitize(exact.values());
        assert!(validate_request(&complete).is_ok(), "Default fill must not manufacture a conflict");
        assert!(complete["typography"].get("lineHeight").is_none());
        let multiple = exact.merge(&json!({"typography":{"lineSpacing":{"mode":"multiple","multiple":2}}})).unwrap();
        assert!(multiple.values()["typography"]["lineSpacing"].get("pt").is_none());
        assert_eq!(global_line_spacing(multiple.values()), Ok(Some(LineSpacing::Multiple(2.))));
        let inherited = multiple.merge(&json!({"typography":{"lineSpacing":null},"meta":{"clear":["publisher"]}})).unwrap();
        assert_eq!(inherited.values()["typography"], multiple.values()["typography"]);
        assert_eq!(inherited.values()["meta"]["clear"], json!(["publisher"]));
        let old = exact.merge(&json!({"typography":{"lineHeight":2}})).unwrap();
        assert_eq!(global_line_spacing(old.values()), Ok(None));
        assert_eq!(old.line_context().legacy_multiple(), 2.);
        assert_eq!(base.values()["typography"]["lineHeight"], 1.55);
    }

    #[test]
    fn p04_untrusted_json_cannot_supply_provenance_or_hide_raw_conflicts() {
        for value in [Value::Null, json!(1.55), json!(true), json!({"trusted":true})] {
            let raw = json!({"_publicationLineSpacingLegacy":value,"typography":{"lineSpacing":{"mode":"exact","pt":18}}});
            assert_eq!(validate_request(&raw), Err(INVALID));
            assert!(ResolvedOptions::from_request(&raw).is_err());
            assert_eq!(validate_export("docx", "Body", &raw), Err(INVALID));
        }
        let conflict = json!({"typography":{"lineHeight":1.55,"lineSpacing":{"mode":"exact","pt":18}}});
        assert!(ResolvedOptions::from_request(&conflict).is_err());
        assert!(crate::publication_metadata::merge_options(&json!({}), &conflict).is_err());
        assert!(ResolvedOptions::from_request(&json!({"typography":{"lineHeight":1.55}})).unwrap().merge(&conflict).is_err());
    }

    #[test]
    fn p04_source_detection_uses_parsed_declarations_not_code_or_comment_substrings() {
        let mode = json!({"frontMatter":{"mode":"none"}});
        for source in ["```md\n<!-- readmd:layout v1 {\"id\":\"sample\",\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":18}}} -->\n```", "A paragraph mentions lineSpacing without a declaration."] {
            assert_eq!(validate_input(source, &mode), Ok(false));
            assert!(validate_export("pdf", source, &mode).is_ok());
        }
        let source = "<!-- readmd:layout v1 {\"id\":\"sample\",\"paragraph\":{\"\\u006cineSpacing\":{\"mode\":\"exact\",\"pt\":18}}} -->\n\nBody\n";
        assert_eq!(validate_input(source, &mode), Ok(true));
        assert_eq!(validate_export("epub", source, &mode), Err(UNSUPPORTED));
        assert!(validate_export("docx", source, &mode).is_ok());
        assert_eq!(validate_preview(source, &mode), Err(UNSUPPORTED));
    }
}
