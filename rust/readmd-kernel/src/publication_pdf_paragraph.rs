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

/// Check only the text the author-aware caption route will consume. This is
/// deliberately separate from inline_markup_ctx: that routine registers
/// fonts and can resolve images or lay out formulas before finding a later
/// unsupported text leaf. The same itemizer/error runs are safe to use here.
fn caption_author_text(block:&Value,ordinary:bool,options:&Value,visited:&mut usize)->Result<(),PdfError>{
    let attributes=layers(block.get("publicationStyle").unwrap_or(&Value::Null))?;
    let has_author=attributes.iter().try_fold(false,|present,attributes|
        crate::publication_blocks::author_style(attributes).map(|author|present||!author.is_empty()).map_err(invalid))?;
    if !has_author{return Ok(());}
    if block["type"]!="caption"{return Err(invalid(crate::publication_inline_text::INVALID));}
    if !ordinary{return Err(invalid(crate::publication_inline_text::UNSUPPORTED));}
    // style() is a pure layer resolver. Its actual effective pair is also
    // the title's block-base reset target, including legacy text.size/flags.
    let caption=style(block,0,PStyle::base("Helvetica",11.,17.6),options,&mut Vec::new())?;
    let base=caption.author_text.expect("nonempty caption author establishes a pair");
    #[derive(Clone,Copy)]
    struct Cascade{author:crate::publication_inline_text::ResolvedTextStyle,base:crate::publication_inline_text::ResolvedTextStyle,depth:u16}
    enum Pending<'a>{Nodes(&'a Value,Cascade),Node(&'a Value,Cascade),Normalized(&'a Value,Cascade)}
    let code_size=(plain_float(dig(options,&["code","size"]),9.5)-0.5).max(6.);
    let context=|cascade:Cascade,code:bool|InlineCtx{
        face:if code{FaceStyle::Mono}else{FaceStyle::Regular},color:Rgb::rgb(0.,0.,0.),href:None,size:cascade.author.size,
        base_face:Face::Helvetica,text_style:TextStyle{author:Some(cascade.author),author_base:Some(cascade.base),author_depth:cascade.depth,..TextStyle::default()}
    };
    let mut pending=vec![Pending::Nodes(&block["text"],Cascade{author:base,base,depth:0})];
    while let Some(task)=pending.pop(){
        if crate::speech::cancelled(){return Err(invalid("cancelled"));}
        let(node,cascade)=match task{
            Pending::Nodes(value,cascade)=>{
                match value{
                    Value::Array(values)=>{
                        if values.len().saturating_add(pending.len())>crate::publication_inline_text::MAX_NODES.saturating_sub(*visited){return Err(invalid(crate::publication_inline_text::BUDGET));}
                        pending.extend(values.iter().rev().map(|value|Pending::Node(value,cascade)));
                    },
                    Value::Null=>(),other=>pending.push(Pending::Node(other,cascade)),
                }
                continue;
            },
            Pending::Normalized(value,cascade)=>{
                if value.is_array(){pending.push(Pending::Nodes(value,cascade));continue;}
                // Use the renderer's exact scalar/null normalization. Objects
                // here become visible JSON text, rather than trusted nodes.
                let normalized=normalize_inline(value);
                for node in normalized.as_array().expect("normalization is an array"){
                    *visited+=1;if *visited>crate::publication_inline_text::MAX_NODES{return Err(invalid(crate::publication_inline_text::BUDGET));}
                    check_run_errors(&author_text_runs(&json_text(&node["v"]),&context(cascade,false),false,code_size))?;
                }
                continue;
            },
            Pending::Node(node,cascade)=>(node,cascade),
        };
        *visited+=1;if *visited>crate::publication_inline_text::MAX_NODES{return Err(invalid(crate::publication_inline_text::BUDGET));}
        match node.get("t").and_then(Value::as_str).unwrap_or(""){
            "text"|"code"=>{
                let code=node["t"]=="code";
                check_run_errors(&author_text_runs(&json_text(node.get("v").unwrap_or(&Value::Null)),&context(cascade,code),code,code_size))?;
            },
            "author-styled"=>{
                if usize::from(cascade.depth)>=crate::publication_inline_text::MAX_DEPTH{return Err(invalid(crate::publication_inline_text::BUDGET));}
                let author=crate::publication_inline_text::AuthorTextStyle::from_value(node.get("text").unwrap_or(&Value::Null)).map_err(invalid)?;
                let effective=author.resolve(&cascade.author,&cascade.base);
                if [effective.size,effective.size_cs].iter().any(|size|!size.is_finite()||*size<=0.||*size>crate::publication_text_runs::MAX_RENDER_PT){return Err(invalid(crate::publication_inline_text::UNSUPPORTED));}
                pending.push(Pending::Normalized(node.get("v").unwrap_or(&Value::Null),Cascade{author:effective,depth:cascade.depth+1,..cascade}));
            },
            "bold"|"italic"|"strike"|"styled"|"display"=>pending.push(Pending::Normalized(node.get("v").unwrap_or(&Value::Null),cascade)),
            "link"=>pending.push(Pending::Normalized(node.get("text").unwrap_or(&Value::Null),cascade)),
            "br"=>{
                let ctx=context(cascade,false);
                check_run_errors(&[author_atom_run(Run{content:RunContent::Break{hard:true},face:ctx.face,size:ctx.size,color:ctx.color,href:None,text_style:ctx.text_style})])?;
            },
            "math"=>return Err(invalid(crate::publication_inline_text::UNSUPPORTED)),
            // Image alt/src are metadata until an actual resolver succeeds.
            // Predicting a fallback here would fetch a resource or reject
            // text that the real renderer may never display. Unknown nodes
            // likewise retain the renderer's ignored-node behavior.
            _=>(),
        }
    }
    Ok(())
}

/// Source preflight uses this same pure caption pass before publication/asset
/// preparation. The direct JSON renderer invokes it within validate() below.
pub(super) fn validate_caption_author(blocks:&Value,options:&Value)->Result<(),PdfError>{
    let mut visited=0;let mut pending=vec![(blocks,true)];
    while let Some((block,parent_ordinary))=pending.pop(){
        if crate::speech::cancelled(){return Err(invalid("cancelled"));}
        let container=matches!(block.get("type").and_then(Value::as_str),Some("quote"|"list"|"table"|"code"|"footnote"|"bibliography"));
        let ordinary=parent_ordinary&&block.get("publicationContext").and_then(Value::as_str).is_none_or(|value|value=="body");
        match block{
            Value::Array(values)=>pending.extend(values.iter().rev().map(|value|(value,parent_ordinary))),
            Value::Object(values)=>{
                caption_author_text(block,ordinary,options,&mut visited)?;
                pending.extend(values.iter().rev().filter(|(key,_)|key.as_str()!="publicationStyle").map(|(_,value)|(value,ordinary&&!container)));
            },_=>(),
        }
    }
    Ok(())
}

/// Direct JSON callers do not get to bypass the same scope/capability checks
/// used by Markdown export. Context is an internal projection hint, never a
/// validation exemption: an unsupported story is still rejected.
pub(super) fn validate(blocks: &Value, style: &Value) -> Result<(), PdfError> {
    typography::validate_request(style).map_err(invalid)?;
    typography::global_line_spacing(style).map_err(invalid)?;
    if typography::tracking::global(style).map_err(invalid)?.is_some() { return Err(invalid(typography::tracking::UNSUPPORTED)); }
    let mut declarations = 0usize;
    let mut author_text_nodes=0;
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
            let author=crate::publication_blocks::author_style(attributes).map_err(invalid)?;
            if !author.is_empty() {
                declarations+=1;if declarations>crate::publication_blocks::MAX_NODES{return Err(invalid(crate::publication_inline_text::BUDGET));}
                if block["type"]!="caption" {return Err(invalid(crate::publication_inline_text::INVALID));}
                if !ordinary||author.rtl==Some(true) {return Err(invalid(crate::publication_inline_text::UNSUPPORTED));}
                for axis in [author.size,author.size_cs]{if let crate::publication_inline_text::CascadeSize::Points(points)=axis{
                    if points.to_pt()>crate::publication_text_runs::MAX_RENDER_PT{return Err(invalid(crate::publication_inline_text::UNSUPPORTED));}
                }}
                let mut values=vec![&block["text"]];let mut count=0;
                while let Some(value)=values.pop(){
                    count+=1;if count>crate::publication_inline_text::MAX_NODES{return Err(invalid(crate::publication_inline_text::BUDGET));}
                    if crate::speech::cancelled(){return Err(invalid("cancelled"));}
                    match value{
                        Value::Object(map)=>{if map.get("t").is_some_and(|v|v=="math"){return Err(invalid(crate::publication_inline_text::UNSUPPORTED));}values.extend(map.values());},
                        Value::Array(children)=>values.extend(children),_=>(),
                    }
                }
            }
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
            // Direct-JSON decoration passes the same value/context contract as
            // a source declaration: values via the shared single parser, then
            // the ordinary-paragraph rule plus the image-only exclusion that
            // the story adapter cannot paint.
            if typography::decoration::parse(&attributes["paragraph"]).map_err(invalid)?.is_some() {
                declarations += 1;
                if declarations > crate::publication_blocks::MAX_NODES { return Err(invalid(typography::INVALID)); }
                if block["type"] != "paragraph" || !ordinary {
                    return Err(invalid(typography::decoration::UNSUPPORTED));
                }
                if block["text"].as_array().is_some_and(|text|
                    text.len()==1&&text[0]["t"]=="image") {return Err(invalid(typography::decoration::UNSUPPORTED));}
            }
        }
        caption_author_text(block,ordinary,style,&mut author_text_nodes)?;
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
    let attributes=layers(block.get("publicationStyle").unwrap_or(&Value::Null))?;
    let has_author=attributes.iter().try_fold(false,|present,attributes|crate::publication_blocks::author_style(attributes).map(|author|present||!author.is_empty()).map_err(invalid))?;
    if has_author {
        // Keep legacy captions unchanged. Only the new author vocabulary uses
        // the common structural caption base; each axis then resolves alone.
        st.leading *= 10./st.size;st.size=10.;
        st.author_text=Some(crate::publication_inline_text::ResolvedTextStyle::base(10.,10.));
    }
    for attributes in attributes {
        if let Some(id) = attributes.get("id").and_then(Value::as_str) {
            st.publication_block = Some((index + 1, Some(id.to_owned())));
        }
        let text = &attributes["text"];
        let paragraph = &attributes["paragraph"];
        st.baseline_shift = typography::baseline::resolve(st.baseline_shift, text, true).map_err(invalid)?;
        if let Some(size) = text["size"].as_f64() { st.leading *= size/st.size; st.size = size; }
        if has_author {
            let mut base=st.author_text.unwrap();
            if let Some(size)=text["size"].as_f64(){
                base.size=size;base.size_cs=size;
                base.size_origin=crate::publication_inline_text::SizeOrigin::Inherited;
                base.size_cs_origin=crate::publication_inline_text::SizeOrigin::Inherited;
            }
            let effective=crate::publication_blocks::author_style(attributes).map_err(invalid)?.resolve(&base,&base);
            if effective.rtl==Some(true)||[effective.size,effective.size_cs].iter().any(|size|!size.is_finite()||*size<=0.||*size>crate::publication_text_runs::MAX_RENDER_PT){return Err(invalid(crate::publication_inline_text::UNSUPPORTED));}
            st.leading *= effective.size/st.size;st.size=effective.size;st.author_text=Some(effective);
        }
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
        if let Some(decoration) = typography::decoration::parse(paragraph).map_err(invalid)? {
            st.decoration = Some(decoration);
        }
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
            author:st.author_text,author_base:st.author_text,
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
    // A decoration-only paragraph enters the typed lane for painting, with the
    // legacy leading/size multiple as its default spacing derivation.
    if st.line_spacing.is_none() && !physical && st.decoration.is_none() { return Err(invalid(typography::INVALID)); }
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
        // Decoration bookkeeping per page segment: (first item index, line-box
        // top, line-box bottom). Shapes are spliced in front of the segment so
        // the fill/border bars paint below the glyphs (paint_back precedent).
        let mut segment_at = self.items.len();
        let mut segment_top = 0.;
        let mut segment_bottom = 0.;
        let mut segment_fresh = true;
        for (index,line) in plan.lines.iter().enumerate() {
            if crate::speech::cancelled() {return Err(invalid("cancelled"));}
            let bound=plan.bounds[index];
            let mut baseline = if let Some(last)=last_baseline { last-(plan.offsets[index]-plan.offsets[index-1]) }
                else {self.y-plan.anchor_uppers[index]};
            let tail = if index+1==plan.lines.len() {plan.tail} else {bound.lower};
            if baseline-tail < self.limit()-1e-9 && (self.placed>0 || index>0) {
                self.paint_decoration(segment_at,segment_top,segment_bottom,st);
                self.end(); self.begin(); baseline=self.y-plan.anchor_uppers[index];
                segment_at = self.items.len();
                segment_fresh = true;
            }
            if segment_fresh { segment_top = baseline+bound.upper; segment_fresh = false; }
            segment_bottom = baseline-bound.lower;
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
        self.paint_decoration(segment_at,segment_top,segment_bottom,st);
        self.y-=st.space_after; self.placed+=1;
        self.doc.after_flowable_at(st,&runs_plain_text(runs),first_page);
        Ok(())
    }

    /// Paint one page segment's paragraph decoration: a fill rectangle over
    /// the whole line-box extent at the paragraph's available text width, plus
    /// one solid bar per declared edge (width = widthPt, offset spacePt outward
    /// from the text area, declared colour). Fill first, then borders, then
    /// the already-buffered text of the segment.
    fn paint_decoration(&mut self, at: usize, top: f64, bottom: f64, st: &PStyle) {
        let Some(decoration) = &st.decoration else { return };
        if bottom >= top { return; }
        let left = self.x0() + st.left_indent;
        let width = self.avail() - st.left_indent - st.right_indent;
        if width <= 0. { return; }
        let right = left + width;
        let height = top - bottom;
        let mut shapes: Vec<DrawItem> = Vec::new();
        if let Some(fill) = &decoration.fill {
            if let Some(color) = hex_parse(&format!("#{fill}")) {
                shapes.push(DrawItem::FillRect { x: left, y: bottom, w: width, h: height, color });
            }
        }
        for (side, edge) in
            [("top", &decoration.top), ("bottom", &decoration.bottom),
             ("left", &decoration.left), ("right", &decoration.right)]
        {
            let Some(edge) = edge else { continue };
            let Some(color) = hex_parse(&format!("#{}", edge.color)) else { continue };
            let (x, y, w, h) = match side {
                "top" => (left, top + edge.space_pt as f64, width, edge.width_pt),
                "bottom" => (left, bottom - edge.space_pt as f64 - edge.width_pt, width, edge.width_pt),
                "left" => (left - edge.space_pt as f64 - edge.width_pt, bottom, edge.width_pt, height),
                _ => (right + edge.space_pt as f64, bottom, edge.width_pt, height),
            };
            shapes.push(DrawItem::FillRect { x, y, w, h, color });
        }
        for (offset, shape) in shapes.into_iter().enumerate() {
            self.items.insert(at + offset, shape);
        }
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
