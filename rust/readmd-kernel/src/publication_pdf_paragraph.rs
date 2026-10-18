//! Typed paragraph spacing for the built-in PDF writer. This module is a
//! child of pdf_render so it shares the existing atoms, fonts and drawing.
use super::*;
use crate::publication_typography::{self as typography, LineSpacing};

fn invalid(code: &str) -> PdfError { PdfError::new(PdfErrorKind::Invalid, code) }

fn layers(value: &Value) -> Result<Vec<&Value>, PdfError> {
    let values = match value { Value::Array(values) => values.iter().collect(),
        Value::Null => Vec::new(), value => vec![value] };
    if values.len() > crate::publication_blocks::MAX_NODES { return Err(invalid(typography::INVALID)); }
    for value in &values {
        crate::publication_blocks::validate_style(value).map_err(invalid)?;
    }
    Ok(values)
}

/// Direct JSON callers do not get to bypass the same scope/capability checks
/// used by Markdown export. Context is an internal projection hint, never a
/// validation exemption: an unsupported story is still rejected.
pub(super) fn validate(blocks: &Value, style: &Value) -> Result<(), PdfError> {
    typography::validate_request(style).map_err(invalid)?;
    typography::global_line_spacing(style).map_err(invalid)?;
    if typography::tracking::global(style).map_err(invalid)?.is_some() { return Err(invalid(typography::tracking::UNSUPPORTED)); }
    let mut declarations = 0usize;
    let mut pending=vec![(blocks,true)];
    while let Some((block,parent_ordinary))=pending.pop() {
        if crate::speech::cancelled() { return Err(invalid("cancelled")); }
        let container=matches!(block.get("type").and_then(Value::as_str),Some("quote"|"list"|"table"|"code"|"footnote"|"bibliography"));
        let ordinary=parent_ordinary&&block.get("publicationContext").and_then(Value::as_str).is_none_or(|s|s=="body");
        match block {
            Value::Array(values) => { pending.extend(values.iter().map(|value|(value,parent_ordinary))); continue; },
            Value::Object(values) => pending.extend(values.iter()
                .filter(|(key,_)| key.as_str()!="publicationStyle").map(|(_,value)|(value,ordinary&&!container))),
            _=>continue,
        }
        if block.get("publicationContext").is_some_and(|v|
            !matches!(v.as_str(),Some("body"|"list"|"quote"|"footnote"|"frontMatter"))) {
            return Err(invalid(typography::INVALID));
        }
        for attributes in layers(block.get("publicationStyle").unwrap_or(&Value::Null))? {
            if typography::tracking::local(&attributes["text"]).map_err(invalid)?.is_some() {
                return Err(invalid(typography::tracking::UNSUPPORTED));
            }
            if typography::baseline::local(&attributes["text"]).map_err(invalid)?.is_some() {
                declarations += 1;
                if declarations > crate::publication_blocks::MAX_NODES { return Err(invalid(typography::INVALID)); }
                if matches!(block["type"].as_str(), Some("math")) { return Err(invalid(typography::INVALID)); }
                if !ordinary || !matches!(block["type"].as_str(), Some("paragraph"|"heading"|"caption")) {
                    return Err(invalid(typography::baseline::UNSUPPORTED));
                }
                if block["type"]=="paragraph" && block["text"].as_array().is_some_and(|text|
                    text.len()==1&&text[0]["t"]=="image") {return Err(invalid(typography::baseline::UNSUPPORTED));}
            }
            if attributes["paragraph"].get("lineSpacing").is_some() {
                declarations += 1;
                if declarations > crate::publication_blocks::MAX_NODES { return Err(invalid(typography::INVALID)); }
                if !matches!(block["type"].as_str(), Some("paragraph" | "heading" | "caption"))
                    || !ordinary {
                    return Err(invalid(typography::UNSUPPORTED));
                }
                if block["type"]=="paragraph" && block["text"].as_array().is_some_and(|text|
                    text.len()==1&&text[0]["t"]=="image") {return Err(invalid(typography::UNSUPPORTED));}
            }
        }
    }
    Ok(())
}

pub(super) fn warnings(blocks:&Value,warns:&mut Vec<String>) {
    let mut pending=vec![(blocks,true)];
    while let Some((value,parent_ordinary))=pending.pop() {
        let ordinary=parent_ordinary&&value.get("publicationContext").and_then(Value::as_str).is_none_or(|s|s=="body");
        let container=matches!(value.get("type").and_then(Value::as_str),Some("quote"|"list"|"table"|"code"|"footnote"|"bibliography"));
        match value {
            Value::Array(values)=>pending.extend(values.iter().map(|value|(value,parent_ordinary))),
            Value::Object(values)=>{
                pending.extend(values.values().map(|value|(value,ordinary&&!container)));
                if value.get("publicationStyle").is_some()
                    && (!matches!(value["type"].as_str(),Some("paragraph"|"heading"|"caption"))
                        || !ordinary)
                    && !warns.iter().any(|s|s=="publication_local_style_unsupported") {
                    warns.push("publication_local_style_unsupported".into());
                }
            }, _=>{},
        }
    }
}

pub(super) fn style(block: &Value, index: usize, mut st: PStyle, options: &Value,
    warns: &mut Vec<String>) -> Result<PStyle, PdfError>
{
    let body = block["type"] == "paragraph"
        && block.get("publicationContext").and_then(Value::as_str).is_none_or(|s| s == "body");
    st.line_spacing = if body { typography::global_line_spacing(options).map_err(invalid)? } else { None };
    st.baseline_shift = if body { typography::baseline::global(options).map_err(invalid)? } else { None };
    if st.line_spacing.is_none() && st.baseline_shift.is_none() && block.get("publicationStyle").is_none() { return Ok(st); }
    st.publication_block = Some((index + 1, block.get("id").and_then(Value::as_str).map(str::to_owned)));
    for attributes in layers(block.get("publicationStyle").unwrap_or(&Value::Null))? {
        if let Some(id) = attributes.get("id").and_then(Value::as_str) {
            st.publication_block = Some((index + 1, Some(id.to_owned())));
        }
        let text = &attributes["text"];
        let paragraph = &attributes["paragraph"];
        st.baseline_shift = typography::baseline::resolve(st.baseline_shift, text, true).map_err(invalid)?;
        if let Some(size) = text["size"].as_f64() { st.leading *= size/st.size; st.size = size; }
        if let Some(color) = text["color"].as_str().and_then(hex_parse) { st.color = color; }
        let bold = text["bold"].as_bool().unwrap_or(matches!(st.face, FaceStyle::Bold | FaceStyle::BoldItalic));
        let italic = text["italic"].as_bool().unwrap_or(matches!(st.face, FaceStyle::Italic | FaceStyle::BoldItalic));
        st.face = match (bold, italic) { (true,true) => FaceStyle::BoldItalic,
            (true,false) => FaceStyle::Bold, (false,true) => FaceStyle::Italic, _ => FaceStyle::Regular };
        if italic && inline_text(&block["text"]).chars().any(|c| !c.is_whitespace()&&winansi_code(c).is_none())
            && !warns.iter().any(|code| code=="publication_pdf_italic_font_unavailable") {
            warns.push("publication_pdf_italic_font_unavailable".into());
        }
        if let Some(align) = paragraph.get("align") { st.align = ta_of(align, st.align); }
        if let Some(value) = paragraph["before"].as_f64() { st.space_before = value; }
        if let Some(value) = paragraph["after"].as_f64() { st.space_after = value; }
        if let Some(value) = paragraph["left"].as_f64() { st.left_indent = value * MM; }
        if let Some(value) = paragraph["right"].as_f64() { st.right_indent = value * MM; }
        if let Some(value) = paragraph["firstLine"].as_f64() { st.first_line_indent = value * MM; }
        if let Some(value) = paragraph["hanging"].as_f64() { st.first_line_indent = -value * MM; }
        if let Some(value) = paragraph["keepNext"].as_bool() { st.keep_with_next = value; }
        if let Some(value) = paragraph["pageBreakBefore"].as_bool() { st.page_break_before = value; }
        st.line_spacing = typography::resolve_line_spacing(st.line_spacing, paragraph, true).map_err(invalid)?;
        if st.line_spacing.is_none() {
            if let Some(value) = paragraph["lineHeight"].as_f64() { st.leading = st.size * value; }
        }
        if text.get("font").is_some() || paragraph.get("keepLines").is_some()
            || paragraph.get("widowControl").is_some() {
            if !warns.iter().any(|code| code == "publication_local_style_unsupported") {
                warns.push("publication_local_style_unsupported".into());
            }
        }
    }
    Ok(st)
}

pub(super) fn inline(nodes: &Value, options: &Value, tmpdir: &str,
    resolve: &dyn Fn(&str)->Option<String>, st: &PStyle) -> Vec<Run>
{
    let context = InlineCtx { face: st.face, color: st.color, href: None, size: st.size,
        base_face: face_for_logical(&st.font_name), text_style: TextStyle {
            baseline_shift_pt: st.baseline_shift.map(|value| value.requested_pt()).unwrap_or(0.), ..TextStyle::default() } };
    inline_markup_ctx(nodes, options, tmpdir, resolve, &context)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Extent { pub upper: f64, pub lower: f64 }

/// Outward-rounded whole-font bounds for MuPDF 1.28.2's locked builtin
/// standard-14 substitutes; audit/font-buffer hashes accompany this patch.
/// These are deliberately conservative, not purported per-glyph AFM data or
/// a promise about arbitrary viewer-supplied substitute fonts. Exact may
/// reject an interval that the particular letters could otherwise fit.
fn base14_bounds(face:Face,size:f64)->Extent {
    let (upper,lower)=match face {
        Face::Helvetica=>(1.077,0.300), Face::HelveticaBold=>(1.072,0.309),
        Face::HelveticaOblique=>(1.072,0.286), Face::HelveticaBoldOblique=>(1.074,0.310),
        Face::TimesRoman=>(1.054,0.282), Face::TimesBold=>(1.046,0.342),
        Face::TimesItalic=>(0.952,0.272), Face::TimesBoldItalic=>(0.974,0.326),
        Face::Courier=>(0.933,0.319), Face::CourierBold=>(1.008,0.395),
        Face::CourierOblique=>(0.922,0.319), Face::CourierBoldOblique=>(0.998,0.395),
        Face::Cjk|Face::Embedded(_)=>(face.asc1000()/1000.,face.desc1000()/1000.),
    };
    Extent{upper:upper*size,lower:lower*size}
}

fn extent(line: &Line, st: &PStyle) -> Extent {
    let face = face_for_logical(&st.font_name);
    // A text strut is intentional. Actual glyphs may enlarge it, never erase
    // the font's ordinary ascent/descent or a formula's logical phantom size.
    expand_extent(&line.atoms, Extent { upper: st.size * face.asc1000()/1000.,
        lower: st.size * face.desc1000()/1000. })
}
pub(super) fn paint_extent(atoms: &[Atom]) -> Extent {
    expand_extent(atoms, Extent {upper:0.,lower:0.})
}
fn expand_extent(atoms:&[Atom],mut bounds:Extent)->Extent {
    for atom in atoms {
        if let Some(image) = &atom.image {
            let (upper, lower) = if let Some(formula) = &image.math {
                (formula.height, formula.depth)
            } else { (image.height - atom.size * 0.2, atom.size * 0.2) };
            bounds.upper = bounds.upper.max(upper + atom.rise);
            bounds.lower = bounds.lower.max(lower - atom.rise);
        } else {
            let mut upper = atom.size * atom.base.asc1000()/1000.;
            let mut lower = atom.size * atom.base.desc1000()/1000.;
            for scalar in atom.paint_text.chars() {
                if winansi_code(scalar).is_some() {
                    let owned = atom.face.owned_proportional(atom.base).then(||
                        crate::pdf_fonts::proportional_vertical_bounds(scalar,
                            matches!(atom.face, FaceStyle::Bold), atom.size)).flatten();
                    if let Some((top, bottom)) = owned {
                        upper=upper.max(top); lower=lower.max(bottom);
                    } else if !scalar.is_whitespace() {
                        let conservative=base14_bounds(atom.face.face(atom.base),atom.size);
                        upper=upper.max(conservative.upper); lower=lower.max(conservative.lower);
                    }
                } else if !scalar.is_whitespace() {
                    if let Some((top, bottom)) = crate::pdf_fonts::vertical_bounds(scalar,
                        matches!(atom.face, FaceStyle::Bold | FaceStyle::BoldItalic), atom.size) {
                        upper = upper.max(top); lower = lower.max(bottom);
                    }
                }
            }
            bounds.upper = bounds.upper.max(upper + atom.rise);
            bounds.lower = bounds.lower.max(lower - atom.rise);
        }
    }
    bounds
}

#[derive(Debug)]
pub(super) struct Plan {
    pub lines: Vec<Line>,
    pub bounds: Vec<Extent>,
    /// Distance from the first baseline, independent of page origin.
    pub offsets: Vec<f64>,
    pub tail: f64,
    pub height: f64,
    /// Physical rise must not be cancelled by moving the paragraph baseline.
    pub anchor_uppers: Vec<f64>,
}

fn geometry(st: &PStyle, line: usize, required: f64, width: f64, height: f64) -> PdfError {
    let (mode, requested) = match st.line_spacing {
        Some(LineSpacing::Multiple(value)) => ("multiple", value * st.size),
        Some(LineSpacing::ExactPt(value)) => ("exact", value),
        Some(LineSpacing::AtLeastPt(value)) => ("atLeast", value),
        None => ("legacy", st.leading),
    };
    let (block_index, block_id) = st.publication_block.as_ref().map(|(i,id)| (*i,id.clone())).unwrap_or((1,None));
    let mut details = serde_json::json!({"coordinateSpace":"pdfParagraphLine","blockIndex":block_index,
        "line":line,"mode":mode,"requestedPt":requested,"requiredPt":required,
        "availableWidthPt":width,"availableHeightPt":height});
    if let Some(id) = block_id { details["blockId"] = Value::String(id); }
    if let Some(value) = st.baseline_shift {
        details["baselineShiftPt"] = serde_json::json!(value.requested_pt());
    }
    PdfError::new(PdfErrorKind::Invalid, format!("publication_layout_geometry: {details}"))
}

pub(super) fn plan(runs: &[Run], st: &PStyle, width: f64, height: f64) -> Result<Plan, PdfError> {
    let physical = st.baseline_shift.is_some_and(|value| value.requested_pt()!=0.);
    if st.line_spacing.is_none() && !physical { return Err(invalid(typography::INVALID)); }
    let spacing = st.line_spacing.unwrap_or(LineSpacing::Multiple(st.leading/st.size));
    let request = match spacing { LineSpacing::Multiple(v) => st.size * v,
        LineSpacing::ExactPt(v) | LineSpacing::AtLeastPt(v) => v };
    let available = width - st.left_indent - st.right_indent;
    if !available.is_finite() || available <= 0. || available - st.first_line_indent <= 0. {
        return Err(geometry(st,1,st.left_indent+st.right_indent+st.first_line_indent.max(0.),width,height));
    }
    let lines = paragraph_lines(runs, st, width);
    if lines.len() > 32768 { return Err(invalid("transaction_budget")); }
    let mut bounds = Vec::with_capacity(lines.len());
    let mut offsets = Vec::with_capacity(lines.len());
    let mut distance = 0.;
    let nominal_upper = st.size * face_for_logical(&st.font_name).asc1000()/1000.;
    let mut previous: Option<Extent> = None;
    let mut anchor_uppers = Vec::with_capacity(lines.len());
    for (index,line) in lines.iter().enumerate() {
        if crate::speech::cancelled() { return Err(invalid("cancelled")); }
        let bound = extent(line,st);
        let mut control = line.clone();
        for atom in &mut control.atoms { atom.rise -= atom.baseline_shift_pt; atom.baseline_shift_pt=0.; }
        anchor_uppers.push(if !physical { bound.upper }
                else if st.line_spacing.is_some() { extent(&control, st).upper }
                else { nominal_upper + line_height(&control,st)-st.leading });
        let indent = st.left_indent + line.indent + if index==0 {st.first_line_indent} else {0.};
        if indent < -1e-7 {return Err(geometry(st,index+1,-indent,width,height));}
        // wrap_atoms can keep a long indivisible glyph/word. Do not silently
        // accept an atom outside the page merely because a mode is valid.
        if line.width > width - st.right_indent - indent + 1e-7 {
            return Err(geometry(st,index+1,line.width+indent+st.right_indent,width,height));
        }
        if bound.upper+bound.lower > height+1e-7 {
            return Err(geometry(st,index+1,bound.upper+bound.lower,width,height));
        }
        if index==0 && st.space_before+bound.upper+bound.lower>height+1e-7 {
            return Err(geometry(st,1,st.space_before+bound.upper+bound.lower,width,height));
        }
        if let Some(last) = previous {
            let minimum = last.lower+bound.upper;
            if matches!(spacing,LineSpacing::ExactPt(_)) && minimum > request+1e-7 {
                return Err(geometry(st,index+1,minimum,width,height));
            }
            let nominal = if st.line_spacing.is_none() { line_height(&control,st) } else { request };
            distance += if matches!(spacing,LineSpacing::ExactPt(_)) {request} else {nominal.max(minimum)};
        }
        offsets.push(distance); bounds.push(bound); previous=Some(bound);
    }
    let tail = bounds.last().map(|last| last.lower.max(request-nominal_upper)).unwrap_or(0.);
    if bounds.last().is_some_and(|last| last.upper+tail+if lines.len()==1 {st.space_before}else{0.} > height+1e-7) {
        return Err(geometry(st,lines.len(),bounds.last().unwrap().upper+tail,width,height));
    }
    let height = anchor_uppers.first().copied().unwrap_or(0.)+distance+tail;
    Ok(Plan {lines,bounds,offsets,tail,height,anchor_uppers})
}

impl Sheet<'_> {
    pub(super) fn add_typed_paragraph(&mut self, runs: &[Run], st: &PStyle,
        keep_min: f64, anchor: Option<&str>)
        ->Result<(),PdfError>
    {
        // Geometry and cancellation are checked again against this actual
        // sheet. Neither a prior plan nor an export option is a bypass.
        let plan = plan(runs,st,self.avail(),self.top()-self.limit())?;
        self.begin();
        if st.keep_with_next && keep_min>0. && self.room()+1e-9 < st.space_before+plan.height+st.space_after+keep_min {
            self.end(); self.begin();
        }
        if let Some(first) = plan.bounds.first() {
            if self.room()+1e-9 < st.space_before+first.upper+first.lower && self.placed>0 {
                self.end(); self.begin();
            }
        }
        self.y -= st.space_before;
        let mut last_baseline: Option<f64> = None;
        let mut first_page=self.doc.current_page;
        for (index,line) in plan.lines.iter().enumerate() {
            if crate::speech::cancelled() {return Err(invalid("cancelled"));}
            let bound=plan.bounds[index];
            let mut baseline = if let Some(last)=last_baseline { last-(plan.offsets[index]-plan.offsets[index-1]) }
                else {self.y-plan.anchor_uppers[index]};
            let tail = if index+1==plan.lines.len() {plan.tail} else {bound.lower};
            if baseline-tail < self.limit()-1e-9 && (self.placed>0 || index>0) {
                self.end(); self.begin(); baseline=self.y-plan.anchor_uppers[index];
            }
            if st.baseline_shift.is_some_and(|value|value.requested_pt()!=0.) {
                if let Some(floor)=self.paint_floor {
                    let overlap=baseline+bound.upper-floor;
                    if overlap>1e-7 {
                        return Err(geometry(st,index+1,st.leading+overlap,self.avail(),self.top()-self.limit()));
                    }
                }
            }
            // An intentional rise can enter the line gap or paper margin;
            // it must never put any real glyph/formula beyond the paper.
            if baseline+bound.upper>self.geom.page_h+1e-7 || baseline-bound.lower < -1e-7 {
                return Err(geometry(st,index+1,bound.upper+bound.lower,self.avail(),self.geom.page_h));
            }
            if index==0 {
                first_page=self.doc.current_page;
                if let Some(name)=anchor {self.items.push(DrawItem::Destination{name:name.to_owned(),top:self.y});}
                if st.name.strip_prefix('H').and_then(|s|s.parse::<u8>().ok()).is_some_and(|n|n>0) {
                    self.items.push(DrawItem::Destination{name:format!("readmd:toc:{}",self.doc.outlines.len()),top:self.y});
                }
            }
            let indent=st.left_indent+line.indent+if index==0 {st.first_line_indent} else {0.};
            self.draw_line(&line.atoms,self.x0()+indent,self.avail()-st.right_indent-indent,st.align,baseline);
            self.texts.push(line.text.clone()); self.y=baseline-tail;
            last_baseline=Some(baseline);
        }
        self.y-=st.space_after; self.placed+=1;
        self.doc.after_flowable_at(st,&runs_plain_text(runs),first_page);
        Ok(())
    }
}

#[cfg(test)]
mod p15_owned_line_bounds_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn p15_exact_collision_guard_uses_the_selected_owned_strut_and_restores_scope() {
        assert!(crate::publication_fonts::current().is_none());
        crate::pdf_fonts::begin("Arial");
        let runs=inline_markup(&json!([{"t":"text","v":"A"},{"t":"br"},{"t":"text","v":"g"}]),
            &json!({"typography":{"size":11}}),"",&|_|None);
        let mut style=PStyle::base("Helvetica",11.,17.6);
        // Original DejaVu hhea: (1901+483)/2048*11 =12.8046875pt.
        style.line_spacing=Some(LineSpacing::ExactPt(12.));
        let error=plan(&runs,&style,400.,700.).unwrap_err();
        assert!(error.message.starts_with("publication_layout_geometry:"));
        style.line_spacing=Some(LineSpacing::ExactPt(13.));
        let accepted=plan(&runs,&style,400.,700.).unwrap();assert_eq!(accepted.lines.len(),2);
        assert!((accepted.offsets[1]-13.).abs()<1e-9);
        for bound in accepted.bounds {assert!((bound.upper-1901.*11./2048.).abs()<1e-9);
            assert!((bound.lower-483.*11./2048.).abs()<1e-9);}
        let (_,warnings)=crate::pdf_fonts::finish().unwrap();assert!(warnings.is_empty());
        assert_eq!(crate::pdf_fonts::proportional_advance('A',false,11.),None);
    }
}
