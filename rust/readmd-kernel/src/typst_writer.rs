//! Markdown publication projection into Typst. Author text is always quoted,
//! never evaluated as Typst code. Only explicitly referenced images are staged.
use crate::md_ast::{Block, Document, Inline};
use crate::publication_inline_text::{AuthorTextStyle, ResolvedTextStyle};
use serde_json::Value;
use std::{collections::{HashMap,HashSet},path::Path};
#[cfg(test)]use std::fs;

#[path="typst_writer_paragraphs.rs"]
mod paragraphs;
#[path="typst_writer_baseline.rs"]
mod baseline;
pub(crate) use paragraphs::GeometryContext;
pub struct Source { pub text:String, pub warnings:Vec<String>, pub requires_core_text_fallback:bool,
    pub(crate) geometry:Option<GeometryContext>,
    pub(crate) baseline_links:Vec<crate::publication_pagination::BaselineLinkRequest>,
    pub(crate) baseline_protected:Vec<crate::publication_pagination::BaselineProtectedRequest> }
pub(crate) fn string(value:&str)->String{
    let mut out=String::from("\"");for c in value.chars(){match c{
        '\\'=>out.push_str("\\\\"),'"'=>out.push_str("\\\""),'\n'=>out.push_str("\\n"),'\r'=>out.push_str("\\r"),'\t'=>out.push_str("\\t"),
        c if c.is_control()=>out.push_str(&format!("\\u{{{:x}}}",c as u32)),c=>out.push(c),
    }}out.push('"');out
}
/// Word's Latin letter formats repeat one letter (27=AA, 28=BB, 53=AAA).
/// Use the same quotient/remainder rule for native heading, page and note
/// counters; Typst's built-in a/A pattern alone uses a different sequence.
fn number_format(format:&str)->String{
    let pattern=string(crate::publication_layout::typst_number(format));
    if matches!(format,"lowerLetter"|"upperLetter"){
        format!("(readmd-n => numbering({pattern},calc.rem(readmd-n - 1,26) + 1) * (calc.quo(readmd-n - 1,26) + 1))")
    }else{pattern}
}
fn n(v:&Value,default:f64,min:f64,max:f64)->f64{v.as_f64().filter(|n|n.is_finite()).unwrap_or(default).clamp(min,max)}
fn color(v:&Value,default:&str)->String{let s=v.as_str().unwrap_or(default);if s.len()==7&&s.starts_with('#')&&s[1..].bytes().all(|b|b.is_ascii_hexdigit()){format!("rgb({})",string(s))}else{format!("rgb({})",string(default))}}
fn font_with_resources(name:&str,managed:bool)->&str{
    let selected=crate::publication_fonts::family(name);
    // The pinned CLI embeds Libertinus Serif, New Computer Modern and
    // DejaVu Sans Mono. Libertinus Sans is not one of its embedded families.
    if selected.eq_ignore_ascii_case("Libertinus Sans")
        ||(!managed&&matches!(selected,"ReadMD Sans SC"|"ReadMD Serif SC")){
        "Libertinus Serif"
    }else{selected}
}
fn font_faces(name:&str,managed:bool,latin_variant:bool)->Vec<&str>{
    let requested=crate::publication_fonts::family(name);
    let base=font_with_resources(name,managed);
    // The portable SC faces provide real regular/bold outlines, but no
    // italic/oblique or smcp face. Put the reviewed Latin face first so its
    // real italic/bold/smcp outlines win over an upright CJK Latin glyph.
    let selected=if latin_variant&&matches!(base,"ReadMD Sans SC"|"ReadMD Serif SC"){"Libertinus Serif"}else{base};
    let mut faces=vec![selected];
    if managed{
        let cjk=match requested{
            "ReadMD Sans SC"|"ReadMD Serif SC"=>Some(requested),
            other if other.eq_ignore_ascii_case("Libertinus Sans")=>Some("ReadMD Sans SC"),
            _ if selected=="Libertinus Serif"=>Some("ReadMD Serif SC"),
            _=>None,
        };
        if let Some(cjk)=cjk{if !faces.contains(&cjk){faces.push(cjk);}}
    }
    faces
}
fn font_expression(name:&str,latin_variant:bool,fallback:bool,warnings:&mut Vec<String>)->String{
    let mut faces=font_faces(name,crate::publication_fonts::current().is_some(),latin_variant);
    if faces[0]!=crate::publication_fonts::family(name){
        let warning=format!("font_fallback: {}",faces[0]);
        if !warnings.contains(&warning){warnings.push(warning);}
    }
    // CJK and supplemental fallback retain the author's Unicode and genuine
    // glyphs. Their upright faces are not certified as italic or small caps.
    if fallback&&!faces.contains(&"DejaVu Sans"){faces.push("DejaVu Sans");}
    if faces.len()==1{string(faces[0])}else{format!("({})",faces.into_iter().map(string).collect::<Vec<_>>().join(","))}
}
fn align(v:&Value)->&str{match v.as_str(){Some("center")=>"center",Some("right")=>"right",_=>"left"}}
fn text(value:&str)->String{format!("#text({})",string(value))}
fn anchor(value:&str)->String{let mut id=String::from("readmd-");for byte in value.bytes(){id.push_str(&format!("{byte:02x}"));}id}
// Public anchors contain only hex after `readmd-`; an internal kind is outside
// that alphabet, so no author-supplied ID can shadow a note or source section.
fn private_anchor(kind:&str,value:&str)->String{format!("readmd-internal-{kind}-{}",anchor(value).trim_start_matches("readmd-"))}

/// A counter update in the actual page header runs on every physical page,
/// including automatic overflow. It is independent of heading/chapter breaks.
fn note_page_header(style:&Value)->String{
    let layout=crate::publication_layout::Layout::from_style(style);
    if layout.note_restart==crate::publication_layout::NoteRestart::Page&&!crate::publication_layout::notes_at_end(style){
        format!("context {{ counter(footnote).update({}) }}",layout.note_start-1)
    }else{"none".into()}
}

/// Feed source-owned starts/restarts into Typst's real heading counter. The
/// numbering callback formats its native counter arguments, never author runs.
fn heading_numbering(level:u8,numbering:&crate::cross_references::HeadingNumbering)->Result<(String,String),String>{
    let depth=level as usize;
    if !(1..=6).contains(&depth)||numbering.numbers.len()!=depth||numbering.rules.len()<depth
        ||numbering.numbers.iter().any(|number|*number>crate::cross_references::MAX_NUMBER)
        ||![" ","\t",""].contains(&numbering.suffix.as_str()) {
        return Err("publication_numbering_invalid".into());
    }
    // Typst counters cannot be negative. A native zero-start heading steps
    // from zero to one; its own formatter subtracts that one at the location.
    // Every subsequent heading receives the resolved source counter state.
    let mut before=numbering.numbers.clone();before[depth-1]=before[depth-1].saturating_sub(1);
    let update=format!("#counter(heading).update(({},))\n",before.iter().map(usize::to_string).collect::<Vec<_>>().join(","));
    let mut body=String::new();
    for part in crate::cross_references::heading_text_parts(&numbering.rules[depth-1].text,depth){
        let index=match part{crate::cross_references::HeadingTextPart::Literal(literal)=>{body.push_str(&text(literal));continue;},crate::cross_references::HeadingTextPart::Counter(index)=>index};
        let selected=if numbering.rules[depth-1].is_legal{"decimal"}else{numbering.rules[index].format.as_str()};
        if !crate::publication_layout::NUMBER_FORMATS.contains(&selected){return Err("publication_numbering_invalid".into());}
        if numbering.numbers[index]==0{
            let value=if index+1==depth{format!("readmd-levels.pos().at({index}) - 1")}else{format!("readmd-levels.pos().at({index})")};
            body.push_str(&format!("#{{ let readmd-value = {value}; if readmd-value == 0 {{ text({}) }} else {{ numbering({},readmd-value) }} }}",string(if selected=="decimal"{"0"}else{""}),number_format(selected)));
        }else{body.push_str(&format!("#numbering({},readmd-levels.pos().at({index}))",number_format(selected)));}
    }
    body.push_str(match numbering.suffix.as_str(){" "=>"#text(\" \")","\t"=>"#h(2em)",_=>""});
    Ok((update,format!("(..readmd-levels) => [{body}]")))
}

/// Section running matter is selected from actual physical page locations.
/// A continuous boundary does not replace the header of an already started
/// page; it takes effect on the next physical page. Counter resets are kept
/// separate from page parity (which is always physical).
fn section_running(style:&Value,index:usize)->String{
    let (width,_)=crate::export_styles::page_dimensions(style);
    let layout=crate::publication_layout::Layout::from_style(style);
    let inside=n(&style["page"]["marginLeft"],18.,0.,60.)+layout.gutter;
    let outside=n(&style["page"]["marginRight"],18.,0.,60.);
    let body_width=width-inside-outside;
    let running_text=|part:&str|{
        let odd=text(style[part]["text"].as_str().unwrap_or(""));
        let regular=if layout.different_odd_even{format!("#if calc.even(here().page()) [{}] else [{odd}]",text(style[part]["evenText"].as_str().unwrap_or("")))}else{odd};
        if layout.hide_first{format!("#if first [{}] else [{regular}]",text(style[part]["firstText"].as_str().unwrap_or("")))}else{regular}
    };
    let number=if style["footer"]["pageNumbers"].as_bool().unwrap_or(true){
        let current=format!("#counter(page).display({})",number_format(&layout.page_format));
        let regular=if layout.page_total{format!("{current} / #str(query(<readmd-publication-document-end>).last().location().page())")}else{current};
        if layout.hide_first{format!("#if not first [{regular}]")}else{regular}
    }else{String::new()};
    format!("context {{ let start = query(<readmd-publication-section-boundary>).find(mark => mark.value.index == {index}); let first = start != none and here().page() == start.location().page(); let running-left=if {} and calc.even(here().page()) {{{outside}mm}} else {{{inside}mm}};\nplace(top+left,dx:running-left,dy:{}mm)[#block(width:{body_width}mm)[#set text(size:{}pt,fill:rgb(\"#808080\"))\n#set par(first-line-indent:0pt)\n#align({})[{}]]]\nplace(bottom+left,dx:running-left,dy:-{}mm)[#block(width:{body_width}mm)[#set text(size:{}pt,fill:rgb(\"#808080\"))\n#set par(first-line-indent:0pt)\n#align({})[{} {number}]]]\n}}",layout.mirror,layout.header_distance,layout.header_size,align(&style["header"]["align"]),running_text("header"),layout.footer_distance,layout.footer_size,layout.footer_align,running_text("footer"))
}
fn section_page(style:&Value)->String{
    let (width,height)=crate::export_styles::page_dimensions(style);
    let layout=crate::publication_layout::Layout::from_style(style);
    let mt=n(&style["page"]["marginTop"],20.,0.,60.);let mb=n(&style["page"]["marginBottom"],20.,0.,60.);
    let inside=n(&style["page"]["marginLeft"],18.,0.,60.)+layout.gutter;
    let outside=n(&style["page"]["marginRight"],18.,0.,60.);
    let margin=if layout.mirror{format!("top:{mt}mm,bottom:{mb}mm,inside:{inside}mm,outside:{outside}mm")}else{format!("top:{mt}mm,bottom:{mb}mm,left:{inside}mm,right:{outside}mm")};
    let note_header=note_page_header(style);
    format!("#set page(width:{width}mm,height:{height}mm,binding:left,margin:({margin}),header:{note_header},footer:none,foreground:context {{ let current=here().page(); let boundaries=query(<readmd-publication-section-boundary>); let padding=query(<readmd-publication-parity-before>).any(mark => {{ let next=boundaries.find(start => start.value.index == mark.value); next != none and current > mark.location().page() and current < next.location().page() }}); let starts=boundaries.filter(mark => mark.location().page() < current or (mark.location().page() == current and mark.value.starts-page)); if not padding and starts.len() > 0 {{readmd-section-running.at(starts.last().value.index)}} }})\n")
}

struct Writer<'a>{
    style:&'a Value,base:&'a str,dir:&'a Path,notes:HashMap<String,Vec<Block>>,seen_notes:HashSet<String>,
    end_notes:Vec<String>,end_ids:HashMap<String,usize>,end_refs:HashMap<String,Vec<String>>,
    note_numbers:HashMap<String,usize>,section_ids:HashSet<String>,
    note_depth:usize,images:HashMap<String,(String,Option<f64>)>,resources:usize,bytes:usize,warnings:Vec<String>,local:Option<crate::publication_blocks::Resolved>,mapping_nonce:Option<&'a str>,
    ordinary_body:bool,local_line:Option<crate::publication_typography::LineSpacing>,local_id:Option<String>,
    paragraph_index:usize,inline_object_bounds:Option<(f64,f64)>,geometry:Option<GeometryContext>,front_blocks:HashSet<usize>,
    author:Option<ResolvedTextStyle>,author_base:ResolvedTextStyle,local_pair:Option<ResolvedTextStyle>,
    baseline:Option<crate::publication_typography::baseline::BaselineShiftPt>,
    local_baseline:Option<crate::publication_typography::baseline::BaselineShiftPt>,
    source_id:Option<String>,baseline_links:Vec<crate::publication_pagination::BaselineLinkRequest>,
    baseline_protected:Vec<crate::publication_pagination::BaselineProtectedRequest>,
}
struct NativeBlock {
    source: String,
    flow: bool,
    keep_next: bool,
    safe: bool,
}
fn safe_column_inlines(values:&[Inline])->bool{
    values.iter().all(|value|match value{
        Inline::FootnoteRef(_)=>false,
        Inline::Strong(children)|Inline::Emph(children)|Inline::Strike(children)
        |Inline::Styled{children,..}|Inline::AuthorStyled{children,..}|Inline::Citation{children,..}
        |Inline::Link{children,..}=>safe_column_inlines(children),
        // Inline display containers can introduce nested block/grid flow.
        // They are outside this helper's calibrated positive-flow subset.
        Inline::Display{..}=>false,
        Inline::Html(source)=>source.trim().is_empty()||source.trim().starts_with("<!--"),
        Inline::Text(_)|Inline::Code(_)|Inline::Image{..}|Inline::Math(_)
        |Inline::OfficeMath{..}|Inline::SoftBreak|Inline::HardBreak=>true,
    })
}
fn safe_column_block(style:&Value,block:&Block)->bool{
    match block{
        Block::Styled{attributes,block}=>{
            let resolved=crate::publication_blocks::Resolved::for_context(style,attributes,block,false);
            !resolved.page_break&&safe_column_block(style,block)
        },
        Block::Heading{level,inlines,..}=>!style["headings"][format!("h{level}")]["pageBreakBefore"].as_bool().unwrap_or(false)&&safe_column_inlines(inlines),
        Block::Paragraph(values)|Block::Caption{inlines:values,..}=>safe_column_inlines(values),
        Block::Quote(blocks)=>blocks.iter().all(|block|safe_column_block(style,block)),
        Block::List{items,..}=>items.iter().all(|item|item.blocks.iter().all(|block|safe_column_block(style,block))),
        Block::Table{header,rows,..}=>header.iter().chain(rows.iter().flatten()).all(|cell|safe_column_inlines(cell)),
        // Bibliography can contain nested layout contexts, counters and large
        // breakable grids. Preserve its native path until independently proven.
        Block::Bibliography(_)|Block::PageBreak|Block::FootnoteDef{..}|Block::SectionStart(_)=>false,
        Block::Html(source)=>source.trim().is_empty()||source.trim().starts_with("<!--"),
        Block::Code{..}|Block::Math(_)|Block::OfficeMath{..}|Block::Hr=>true,
    }
}
fn column_keep_next(style:&Value,block:&Block)->bool{
    match block{
        Block::Styled{attributes,block}=>{
            let resolved=crate::publication_blocks::Resolved::for_context(style,attributes,block,false);
            if attributes.pointer("/paragraph/keepNext").is_some(){resolved.keep_next}else{resolved.keep_next||column_keep_next(style,block)}
        },
        Block::Heading{level,..}=>style["headings"][format!("h{level}")]["keepNext"].as_bool().unwrap_or(true),
        _=>false,
    }
}
fn group_column_parts(blocks:Vec<NativeBlock>)->(Vec<String>,bool){
    let mut parts=Vec::<String>::new();let mut current=String::new();let mut flow=false;let mut safe=true;
    for block in blocks{
        safe&=block.safe;current.push_str(&block.source);flow|=block.flow;
        if block.flow&&!block.keep_next{parts.push(std::mem::take(&mut current));flow=false;}
    }
    if !current.is_empty(){
        if flow||parts.is_empty(){parts.push(current);}else{parts.last_mut().unwrap().push_str(&current);}
    }
    (parts,safe)
}
fn collect_notes(blocks:&[Block],notes:&mut HashMap<String,Vec<Block>>){for block in blocks{match block{
    Block::FootnoteDef{label,blocks}=>{notes.entry(label.clone()).or_insert_with(||blocks.clone());},
    Block::Quote(blocks)=>collect_notes(blocks,notes),Block::List{items,..}=>for item in items{collect_notes(&item.blocks,notes)},_=>(),
}}}
impl Writer<'_>{
    fn text_pair(&self,block:&Block)->ResolvedTextStyle{
        if let Some(pair)=self.local_pair{return pair;}
        let body=n(&self.style["typography"]["size"],11.,6.,72.);
        if self.note_depth>0{let size=crate::publication_layout::Layout::from_style(self.style).note_size;return ResolvedTextStyle::base(size,self.style["footnotes"]["sizeCs"].as_f64().unwrap_or(size));}
        let size=match block.unstyled(){Block::Heading{level,..}=>self.style["headings"][format!("h{level}")]["size"].as_f64().unwrap_or(body),Block::Table{..}=>self.style["table"]["cellSize"].as_f64().unwrap_or(10.),Block::Caption{..}=>10.,_=>body};ResolvedTextStyle::base(size,size)
    }
    fn author_text(&self,value:&str,code:bool)->Result<String,String>{
        let Some(style)=self.author else{return Ok(if code{format!("#raw({})",string(value))}else{text(value)});};
        if !value.is_empty(){self.author_direction()?;}
        if [style.size,style.size_cs].into_iter().any(|size|!size.is_finite()||size<=0.||size>4096.){return Err(crate::publication_inline_text::UNSUPPORTED.into());}
        let mut out=String::new();for segment in crate::publication_text_runs::split(value,&style).map_err(str::to_string)?{
            let body=value.get(segment.from..segment.to).ok_or(crate::publication_inline_text::INVALID)?;
            // Raw's native 0.8em default remains for inherited/ordinary code.
            // An author size (including an explicit block-base reset) is an
            // absolute metric and must also override that inner raw default.
            let body=if code&&segment.origin.is_explicit(){format!("#{{ show raw: set text(size:{}pt); raw({}) }}",segment.size,string(body))}else if code{format!("#raw({})",string(body))}else{text(body)};
            // Natural script direction follows the engine's Unicode bidi
            // algorithm. Word's explicit rtl run property is not an embedding.
            out.push_str(&format!("#text(size:{}pt)[{body}]",segment.size));
        }Ok(out)
    }
    fn author_direction(&self)->Result<(),String>{if self.author.is_some_and(|style|style.rtl==Some(true)){Err(crate::publication_inline_text::UNSUPPORTED.into())}else{Ok(())}}
    fn author_atom(&self,body:String)->Result<String,String>{if !body.is_empty(){self.author_direction()?;}Ok(body)}
    fn author_inlines(&mut self,style:&AuthorTextStyle,children:&[Inline])->Result<String,String>{let previous=self.author;self.author=Some(style.resolve(&previous.unwrap_or(self.author_base),&self.author_base));let result=if self.author.is_some_and(|style|[style.size,style.size_cs].into_iter().any(|size|!size.is_finite()||size<=0.||size>4096.)){Err(crate::publication_inline_text::UNSUPPORTED.into())}else{self.inlines(children)};self.author=previous;result}
    fn rich_measure(&mut self,nodes:&[crate::publication_rich::RichLayoutNode])->Result<String,String>{
        use crate::publication_rich::RichLayoutNode as L;
        let mut out=String::new();for node in nodes{match node{
            L::Inline{children}=>out.push_str(&self.inlines(&crate::md_ast::rich_inlines(children))?),
            L::Group{children,..}=>out.push_str(&self.rich_measure(children)?),
            L::Link{href,children}=>out.push_str(&format!("#link({})[{}]",string(href),self.rich_measure(children)?)),
            L::Block{children,indent}=>{if *indent{out.push_str("#h(4em)");}out.push_str(&self.rich_measure(children)?);out.push_str("#linebreak()");},
            L::Columns{left,right}=>out.push_str(&format!("#grid(columns:(auto,auto),column-gutter:0.75em,align:(left,top),[{}],[{}])",self.rich_measure(left)?,self.rich_measure(right)?)),
        }}Ok(out)
    }
    fn rich_labels(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],out:&mut Vec<String>)->Result<(),String>{
        use crate::publication_rich::RichLayoutNode as L;
        for node in nodes{match node{
            L::Columns{left,right}=>{out.push(self.rich_measure(left)?);self.rich_labels(left,out)?;self.rich_labels(right,out)?;},
            L::Block{children,..}|L::Group{children,..}|L::Link{children,..}=>self.rich_labels(children,out)?,_=>(),
        }}Ok(())
    }
    fn rich_flow(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],hanging:bool)->Result<String,String>{
        use crate::publication_rich::RichLayoutNode as L;
        let mut out=String::new();for node in nodes{match node{
            L::Inline{children}=>out.push_str(&self.inlines(&crate::md_ast::rich_inlines(children))?),
            L::Group{children,..}=>out.push_str(&self.rich_flow(children,hanging)?),
            L::Link{href,children}=>out.push_str(&format!("#link({})[{}]",string(href),self.rich_flow(children,hanging)?)),
            L::Block{children,indent}=>out.push_str(&format!("#block(width:100%,breakable:true,above:0pt,below:0pt)[#set par(first-line-indent:0pt,hanging-indent:{}em,spacing:0pt)\n{}{}{}]\n",if hanging{2}else{0},if *indent{"#pad(left:4em)["}else{""},self.rich_flow(children,hanging)?,if *indent{"]"}else{""})),
            L::Columns{left,right}=>out.push_str(&format!("#layout(size => grid(columns:(calc.min(readmd-csl-label-width,size.width*0.35),1fr),column-gutter:0.75em,align:(left,top),[#set par(first-line-indent:0pt,hanging-indent:0pt)\n#readmd-csl-cell[{}]],[#set par(first-line-indent:0pt,hanging-indent:0pt)\n#readmd-csl-cell[{}]]))\n",self.rich_flow(left,false)?,self.rich_flow(right,false)?)),
        }}Ok(out)
    }
    fn rich_rows(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],columns:bool,hanging:bool,rows:&mut Vec<String>,inline:&mut String)->Result<(),String>{
        use crate::publication_rich::RichLayoutNode as L;
        fn flush(columns:bool,rows:&mut Vec<String>,inline:&mut String){if !inline.is_empty(){let body=std::mem::take(inline);rows.push(if columns{format!("grid.cell(colspan:2)[{body}]")}else{format!("[{body}]" )});}}
        for node in nodes{match node{
            L::Inline{children}=>{if !inline.is_empty()||!crate::publication_rich::plain(children).bytes().all(|b|b.is_ascii_whitespace()){inline.push_str(&self.rich_flow(std::slice::from_ref(node),hanging)?);}},
            L::Group{children,..}=>self.rich_rows(children,columns,hanging,rows,inline)?,
            L::Columns{left,right}=>{flush(columns,rows,inline);rows.push(format!("[#set par(first-line-indent:0pt,hanging-indent:0pt)\n#readmd-csl-cell[{}]],[#set par(first-line-indent:0pt,hanging-indent:0pt)\n#readmd-csl-cell[{}]]",self.rich_flow(left,false)?,self.rich_flow(right,false)?));},
            _=>{flush(columns,rows,inline);let body=self.rich_flow(std::slice::from_ref(node),hanging)?;rows.push(if columns{format!("grid.cell(colspan:2)[{body}]")}else{format!("[{body}]" )});},
        }}Ok(())
    }
    fn rich_bibliography(&mut self,bib:&crate::publication_rich::RichBibliography)->Result<String,String>{
        let layouts=bib.items.iter().map(|item|item.layout().map_err(|error|error.code)).collect::<Result<Vec<_>,_>>()?;
        let columns=layouts.iter().any(|layout|crate::publication_rich::layout_has_columns(layout));
        let mut labels=Vec::new();for layout in &layouts{self.rich_labels(layout,&mut labels)?;}
        let measurement=labels.iter().map(|label|format!("measure([{label}]).width")).collect::<Vec<_>>().join(",");
        let mut cells=Vec::new();let mut gaps=Vec::new();
        for (index,layout) in layouts.iter().enumerate(){
            let mut rows=Vec::new();let mut inline=String::new();self.rich_rows(layout,columns,bib.hanging_indent,&mut rows,&mut inline)?;
            if !inline.is_empty(){rows.push(if columns{format!("grid.cell(colspan:2)[{inline}]")}else{format!("[{inline}]" )});}
            if rows.is_empty(){continue;}
            for (row,value) in rows.iter().enumerate(){
                cells.push(value.clone());
                let entry_gap=if row+1==rows.len()&&index+1<layouts.len(){bib.entry_gap_em()}else{0};
                gaps.push(format!("{}em",entry_gap+u64::from(bib.line_spacing.saturating_sub(1))));
            }
        }
        if !gaps.is_empty(){gaps.pop();}
        let gaps=if gaps.is_empty(){"0pt".into()}else{format!("({},)",gaps.join(","))};
        let grid=if columns{format!("#layout(size => grid(columns:(calc.min(readmd-csl-label-width,size.width*0.35),1fr),column-gutter:0.75em,row-gutter:{gaps},align:(left,top),{}))",cells.join(","))}else{format!("#grid(columns:(1fr,),row-gutter:{gaps},{})",cells.join(","))};
        let cell_helper=if columns{include_str!("publication_typst_cell.typ")}else{""};
        Ok(format!("#heading(level:2)[{}]\n#context {{ let readmd-csl-label-width=calc.max(0pt{}{}); [#block(width:100%,breakable:true)[{cell_helper}\n#set text(top-edge:0.8em,bottom-edge:-0.2em)\n#set par(first-line-indent:0pt,spacing:0pt,hanging-indent:{}em,leading:{}em)\n{grid}\n]] }}\n",text(&bib.title),if measurement.is_empty(){""}else{","},measurement,if bib.hanging_indent{2}else{0},bib.line_spacing.saturating_sub(1)))
    }
    fn resource(&mut self,bytes:&[u8],extension:&str)->Result<String,String>{
        self.resources+=1;self.bytes=self.bytes.saturating_add(bytes.len());
        if self.resources>4096||self.bytes>128*1024*1024{return Err("typst_output_limit".into());}
        let name=format!("resource-{}.{extension}",self.resources);crate::task_workspace::write(&self.dir.join(&name),bytes).map_err(|_|"typst_workspace_failed")?;Ok(name)
    }
    fn math(&mut self,source:&str,xml:Option<&str>,display:bool)->Result<String,String>{
        let size=self.local.as_ref().map_or_else(||if self.note_depth>0{crate::publication_layout::Layout::from_style(self.style).note_size}else{n(&self.style["typography"]["size"],11.,6.,72.)},|s|s.size);
        let available=crate::publication_layout::Layout::from_style(self.style).column_width(self.style)*72.0/25.4-self.local.as_ref().map_or(0.0,|s|(s.left+s.right)*72.0/25.4);
        if let Some(xml)=xml{crate::math_word_projection::validate_formula(xml,display,display.then_some(available),"typst")?;}
        let formula=crate::math_layout::layout_source_constrained(source,xml,display,size,display.then_some(available)).ok_or("typst_formula_failed")?;
        if !display {
            if let Some((upper,lower))=&mut self.inline_object_bounds {
                *upper=upper.max(formula.height);*lower=lower.max(formula.depth);
            }
        }
        let svg=formula.svg(self.local.as_ref().map(|s|s.color.as_str()).unwrap_or_else(||self.style["typography"]["color"].as_str().unwrap_or("#262626")));
        crate::svg_render::validate(svg.as_bytes()).map_err(|_|"typst_formula_failed")?;
        let name=self.resource(svg.as_bytes(),"svg")?;
        let image=format!("#image({},width:{}pt,alt:{})",string(&name),formula.width.max(0.01),string(source));
        if display{Ok(format!("#align({})[{image}]\n",formula.alignment.map(|a|if a==crate::omml::MathJustification::CenterGroup{"center"}else{a.as_str()}).or_else(||self.local.as_ref().map(|s|s.align.as_str()).filter(|s|*s!="justify")).unwrap_or("center")))}else{Ok(format!("#box(baseline:{}pt)[{image}]",formula.depth))}
    }
    fn image(&mut self,src:&str,alt:&str)->Result<String,String>{
        let (name,print_width)=if let Some(entry)=self.images.get(src){entry.clone()}else{
            let Some((bytes,extension))=crate::mdexport::publication_image(self.base,src,&mut self.warnings)? else{return Ok(text(alt));};
            let print_width=crate::scientific_figures::print_width(&bytes);
            let name=self.resource(&bytes,&extension)?;let entry=(name,print_width);self.images.insert(src.into(),entry.clone());entry
        };
        // Typst images are block content; a raw image inside par() is ignored
        // with only a compiler warning. An inline box owns the image's region.
        let width=n(&self.style["images"]["widthPct"],92.,5.,100.);
        // A zero-leading note can put punctuation ink beyond the nominal
        // font descender. Give its inline figures a small optical clearance,
        // without changing the author's text line-height or figure width.
        let clearance=if self.note_depth>0{",inset:(top:0.15em,bottom:0.15em)"}else{""};
        if let Some(mm)=print_width {Ok(format!("#box(width:100%{clearance})[#layout(size => image({},width:calc.min(size.width,{mm}mm),alt:{}))]",string(&name),string(alt)))}
        else{Ok(format!("#box(width:{width}%{clearance})[#image({},width:100%,alt:{})]",string(&name),string(alt)))}
    }
    fn styled_inline(&mut self,format:&crate::publication_rich::RichFormat,mut body:String)->Result<String,String>{
                use crate::publication_rich::*;
                // The reviewed portable faces expose regular/bold, not Light.
                // A successful output must not silently discard that request.
                if format.font_weight==RichFontWeight::Light{return Err("academic_rich_unsupported".into());}

                let requested=self.local.as_ref().map(|s|s.font.as_str()).unwrap_or_else(||self.style["typography"]["font"].as_str().unwrap_or("MicrosoftYaHei"));
                let faces=font_expression(requested,format.font_variant==RichFontVariant::SmallCaps||format.font_style!=RichFontStyle::Normal,crate::publication_fonts::needs_core_text_fallback(&body),&mut self.warnings);
                if format.font_variant==RichFontVariant::SmallCaps{body=format!("#smallcaps[{body}]");}
                // Apply routing to normal/bold leaves too: an explicit absent
                // family must never bypass the real-face selection above.
                body=format!("#text(font:{faces})[{body}]");
                if format.text_decoration == RichDecoration::Underline { body = format!("#underline[{body}]"); }
                match format.vertical_align { RichVerticalAlign::Baseline => {}, RichVerticalAlign::Sup => body = format!("#super[{body}]"), RichVerticalAlign::Sub => body = format!("#sub[{body}]") }
                Ok(format!("#text(style:{},weight:{})[{body}]", string(match format.font_style { RichFontStyle::Normal=>"normal", RichFontStyle::Italic=>"italic", RichFontStyle::Oblique=>"oblique" }), match format.font_weight { RichFontWeight::Normal=>400, RichFontWeight::Bold=>700, RichFontWeight::Light=>300 }))
    }
    fn inlines(&mut self,values:&[Inline])->Result<String,String>{
        let mut out=String::new();for value in values{out.push_str(&match value{
            Inline::Text(s)=>self.ordinary_text(s)?,Inline::Code(s)=>{let body=self.author_text(s,true)?;self.protected_baseline_object(body)?},
            Inline::Strong(values)=>format!("#strong[{}]",self.inlines(values)?),Inline::Emph(values)=>format!("#emph[{}]",self.inlines(values)?),Inline::Strike(values)=>format!("#strike[{}]",self.inlines(values)?),
            Inline::Styled { format, children } => {
                let body = self.inlines(children)?;
                self.styled_inline(format, body)?
            },
            Inline::AuthorStyled{style,children}=>self.author_inlines(style,children)?,
            Inline::Display { display, children } => {
                let body=self.inlines(children)?;
                match display { crate::publication_rich::RichDisplay::Block=>format!("#block[{body}]"),crate::publication_rich::RichDisplay::Indent=>format!("#pad(left:4em)[{body}]"),crate::publication_rich::RichDisplay::LeftMargin=>format!("#box[{body}]"),crate::publication_rich::RichDisplay::RightInline=>body }
            },
            Inline::Citation { children, .. } => self.with_baseline(None,|writer|writer.inlines(children))?,
            Inline::SoftBreak=>self.author_atom(" ".into())?,Inline::HardBreak=>self.author_atom("#linebreak()".into())?,
            Inline::Link{href,children,..}=>{
                let destination=if let Some(id)=href.strip_prefix('#'){
                    let target=id.strip_prefix("readmd-section-").filter(|id|self.section_ids.contains(*id)).map(|id|private_anchor("section",id)).unwrap_or_else(||anchor(id));
                    format!("label({})",string(&target))
                }else{string(href)};
                format!("#link({destination})[{}]",self.inlines(children)?)
            },
            Inline::Image{src,alt,..}=>{let body=self.image(src,alt)?;self.author_atom(body)?},
            Inline::Math(s)=>{let body=self.math(s,None,false)?;let body=self.author_atom(body)?;self.protected_baseline_object(body)?},
            Inline::OfficeMath{latex,omml}=>{let body=self.math(latex,Some(omml),false)?;let body=self.author_atom(body)?;self.protected_baseline_object(body)?},
            Inline::FootnoteRef(label)=>{
                self.author_direction()?;
                let id=private_anchor("note",label);
                        if self.note_depth>0{return Err("typst_invalid_content".into());}
                if crate::publication_layout::notes_at_end(self.style){
                    if !self.notes.contains_key(label){return Err("typst_invalid_content".into());}
                    let index=if let Some(index)=self.end_ids.get(label){*index}else{self.end_notes.push(label.clone());let index=self.end_notes.len();self.end_ids.insert(label.clone(),index);index};
                    let refs=self.end_refs.entry(label.clone()).or_default();let reference=private_anchor("note-ref",&format!("{label}:{}",refs.len()+1));refs.push(reference.clone());
                    let layout=crate::publication_layout::Layout::from_style(self.style);
                    let index=self.note_numbers.get(label).copied().unwrap_or_else(||crate::publication_layout::note_index(self.style,index));
                    format!("#link(label({}))[#super[#numbering({},{index})]]#label({})",string(&id),number_format(&layout.note_format),string(&reference))
                }else if self.seen_notes.contains(label){format!("#footnote(label({}))",string(&id))}else{
                    let blocks=self.notes.get(label).cloned().ok_or("typst_invalid_content")?;self.seen_notes.insert(label.clone());self.note_depth+=1;let previous=self.local.take();let old_pair=self.local_pair.take();let old_author=self.author.take();let old_base=self.author_base;self.author_base=self.text_pair(&Block::Paragraph(Vec::new()));
                    // The first paragraph follows the note marker inline;
                    // wrapping it in par() would orphan the marker on a line.
                    let keep=crate::publication_layout::Layout::from_style(self.style).note_keep_paragraphs;
                    let body=if !keep{if let Some((Block::Paragraph(values),rest))=blocks.split_first(){format!("{}\n\n{}",self.inlines(values)?,self.blocks(rest)?)}else{self.blocks(&blocks)?}}else{self.blocks(&blocks)?};
                    self.local=previous;self.local_pair=old_pair;self.author=old_author;self.author_base=old_base;self.note_depth-=1;format!("#footnote[{body}]#label({})",string(&id))
                }
            },
            Inline::Html(s) if s.trim().is_empty()||s.trim().starts_with("<!--")=>String::new(),
            Inline::Html(_)=>return Err("typst_invalid_content".into()),
        });}Ok(out)
    }
    fn document_notes(&mut self)->Result<String,String>{
        if self.end_notes.is_empty(){return Ok(String::new());}
        let layout=crate::publication_layout::Layout::from_style(self.style);
        let mut out=format!("\n#pagebreak(weak:true)\n#heading(level:1,numbering:none,outlined:false)[{}]\n#block[\n#set text(size:{}pt,top-edge:\"ascender\",bottom-edge:\"descender\")\n#set par(first-line-indent:0pt,leading:{}pt,spacing:0pt)\n",text(&crate::publication_layout::note_title(self.style)),layout.note_size,(layout.note_line_height-1.)*layout.note_size);
        if layout.note_separator&&self.style["footnotes"].get("separator").is_some(){out.push_str("#line(length:30%,stroke:0.5pt)\n");}
        for (at,label)in self.end_notes.clone().into_iter().enumerate(){
            let blocks=self.notes.get(&label).cloned().ok_or("typst_invalid_content")?;self.note_depth+=1;let previous=self.local.take();let old_pair=self.local_pair.take();
            let body=self.blocks(&blocks);self.local=previous;self.local_pair=old_pair;self.note_depth-=1;let body=body?;
            let refs=&self.end_refs[&label];let index=self.note_numbers.get(&label).copied().unwrap_or_else(||crate::publication_layout::note_index(self.style,at+1));
            let backs=refs.iter().enumerate().skip(1).map(|(n,id)|format!("#h(0.5em)#link(label({}))[{}]",string(id),text(&format!("↩{}",n+1)))).collect::<String>();
            out.push_str(&format!("#block(width:100%,above:6pt,breakable:true)[#grid(columns:(auto,1fr),column-gutter:0.7em,align:(right,top),[#link(label({}))[#numbering({},{index})]],[{body}\n{backs}])]#label({})\n",string(&refs[0]),number_format(&layout.note_format),string(&private_anchor("note",&label))));
        }
        out.push_str("]\n");Ok(out)
    }
    fn blocks(&mut self,blocks:&[Block])->Result<String,String>{
        Ok(self.rendered_blocks(blocks)?.into_iter().map(|block|block.source).collect())
    }
    fn rendered_blocks(&mut self,blocks:&[Block])->Result<Vec<NativeBlock>,String>{
        let mut out=Vec::new();let mut bytes=0usize;let mut target=None;for block in blocks{if crate::speech::cancelled(){return Err("cancelled".into());}
            let previous_author=self.author.take();let previous_base=self.author_base;self.author_base=self.text_pair(block);
            if matches!(block.unstyled(),Block::Caption{..}){self.author=previous_author;}
            let original_body=self.ordinary_body;
            if self.front_blocks.contains(&(block as *const Block as usize)){self.ordinary_body=false;}
            if let (Some(nonce),Block::Html(comment))=(self.mapping_nonce,block){if let Some(id)=crate::publication_pagination::marker_id(comment,nonce){target=Some(id.to_string());self.author=previous_author;self.author_base=previous_base;self.ordinary_body=original_body;continue;}}
            let inline_target=match block {
                Block::Paragraph(values)|Block::Heading{inlines:values,..}=>self.mapping_nonce.and_then(|nonce|values.iter().find_map(|value|if let Inline::Html(s)=value{crate::publication_pagination::marker_id(s,nonce).map(str::to_string)}else{None})),
                _=>None,
            };
            let mapped=target.take().or(inline_target);
            let previous_source_id=self.source_id.clone();
            if let Some(id)=mapped.as_ref(){self.source_id=Some(id.clone());}
            let rendered=match block{
                Block::Styled{attributes,block}=>{
                    let old_pair=self.local_pair;let base=self.text_pair(block);self.local_pair=Some(if attributes.pointer("/text/size").is_some(){let size=attributes["text"]["size"].as_f64().unwrap_or(base.size);ResolvedTextStyle::base(size,size)}else{base});self.author_base=self.local_pair.unwrap();
                    if matches!(block.unstyled(),Block::Caption{..}){
                        let author=crate::publication_blocks::author_style(attributes).map_err(str::to_string)?;
                        // A title's base reset returns to the effective
                        // caption pair rather than the body or raw default.
                        if !author.is_empty(){let caption=author.resolve(&self.author_base,&self.author_base);self.local_pair=Some(caption);self.author_base=caption;self.author=Some(caption);}
                    }
                    let previous_line=self.local_line;let previous_id=self.local_id.clone();
                    let ordinary=self.ordinary_body&&self.note_depth==0&&matches!(block.unstyled(),Block::Paragraph(_));
                    let inherited=previous_line.or(if ordinary{crate::publication_typography::global_line_spacing(self.style)?}else{None});
                    self.local_line=crate::publication_typography::resolve_line_spacing(inherited,&attributes["paragraph"],ordinary)?;
                    let next_baseline=crate::publication_typography::baseline::resolve(
                        self.local_baseline.or(if ordinary{crate::publication_typography::baseline::global(self.style)?}else{None}),
                        &attributes["text"],ordinary)?;
                    let previous_baseline=std::mem::replace(&mut self.local_baseline,next_baseline);
                    if let Some(id)=attributes["id"].as_str(){self.local_id=Some(id.into());}
                    let s=crate::publication_blocks::Resolved::for_context(self.style,attributes,block,self.note_depth>0);
                    let width=crate::publication_layout::Layout::from_style(self.style).column_width(self.style);if width-s.left-s.right<20.{return Err("publication_layout_geometry".into());}
                    let previous=self.local.replace(s.clone());
                    let body=match block.as_ref(){
                        Block::Caption{label,inlines}=>Ok(format!("{} {}#label({})\n",self.author_text(&label.text(),false)?,self.inlines(inlines)?,string(&anchor(&label.id)))),
                        _=>self.blocks(std::slice::from_ref(block.as_ref())),
                    };self.local=previous;self.local_pair=old_pair;self.local_line=previous_line;self.local_id=previous_id;self.local_baseline=previous_baseline;let body=body?;
                    format!("{}#block(width:100%,above:{}pt,below:{}pt,sticky:{},breakable:{})[#set text(font:{},size:{}pt,fill:{},weight:{},style:{},costs:(widow:{}%,orphan:{}%))\n#set par(leading:{}pt,spacing:0pt,first-line-indent:{}mm,hanging-indent:{}mm,justify:{})\n{}#pad(left:{}mm,right:{}mm)[#block(width:100%)[#set align({})\n{}]]]\n",
                        if s.page_break{"#pagebreak(weak:true)\n"}else{""},s.before,s.after,s.keep_next,!s.keep_lines,font_expression(&s.font,s.italic,crate::publication_fonts::needs_core_text_fallback(&body),&mut self.warnings),s.size,color(&serde_json::json!(s.color),"#262626"),string(if s.bold{"bold"}else{"regular"}),string(if s.italic{"italic"}else{"normal"}),if s.widow{100}else{0},if s.widow{100}else{0},(s.line_height-1.)*s.size,s.first,s.hanging,s.align=="justify",match block.as_ref(){Block::Heading{numbering:Some(_),..}=>"#show heading: it => context { counter(heading).display(it.numbering) + it.body }\n",Block::Heading{..}=>"#show heading: it => it.body\n",_=>""},s.left,s.right,if s.align=="justify"{"left"}else{&s.align},body)
                },
                Block::Heading{level,id,inlines,numbering}=>{
                    let body=self.inlines(inlines)?;
                    if let Some(numbering)=numbering{
                        if numbering.suffix=="\t"&&!self.warnings.iter().any(|warning|warning=="publication_numbering_tab_unsupported"){self.warnings.push("publication_numbering_tab_unsupported".into());}
                        let(update,native)=heading_numbering(*level,numbering)?;
                        format!("{update}#heading(level:{level},numbering:{native})[{body}]#label({})\n",string(&anchor(id)))
                    }else{format!("#heading(level:{level})[{body}]#label({})\n",string(&anchor(id)))}
                },
                Block::Caption{label,inlines}=>format!("#block(width:100%,above:4pt,below:8pt)[#set text(size:10pt)\n#set align(center)\n#set par(first-line-indent:0pt)\n{} {}]#label({})\n",self.author_text(&label.text(),false)?,self.inlines(inlines)?,string(&anchor(&label.id))),
                Block::Paragraph(values)=>{
                    let baseline=if self.ordinary_body&&self.note_depth==0{
                        self.local_baseline.or(crate::publication_typography::baseline::global(self.style)?)
                    }else{None};
                    self.register_baseline_links(values,baseline)?;
                    self.with_baseline(baseline,|writer|writer.body_paragraph(values))?
                },
                Block::Math(s)=>self.math(s,None,true)?,Block::OfficeMath{latex,omml}=>self.math(latex,Some(omml),true)?,
                Block::PageBreak=>"#pagebreak(weak:true)\n".into(),Block::Hr=>format!("#line(length:100%,stroke:0.5pt+{})\n",color(&self.style["hr"]["color"],"#d8dce2")),
                Block::Code{lang,text}=>format!("#block(width:100%,breakable:true,inset:8pt,fill:{},radius:{}pt,stroke:{}pt+{})[#set text(font:{},size:{}pt,fill:{})\n#raw({},block:true,lang:{})]\n",
                    color(&self.style["code"]["bg"],"#f5f6f8"),if self.style["code"]["rounded"].as_bool().unwrap_or(true){4.}else{0.},n(&self.style["code"]["borderWidth"],0.5,0.,3.),color(&self.style["code"]["borderColor"],"#dfe3e8"),font_expression(self.style["code"]["font"].as_str().unwrap_or("Consolas"),false,crate::publication_fonts::needs_core_text_fallback(text),&mut self.warnings),n(&self.style["code"]["size"],9.5,6.,36.),color(&self.style["code"]["color"],"#2f3b4a"),string(text),string(lang)),
                Block::Quote(values)=>format!("#block(width:100%,breakable:true,inset:8pt,fill:{},stroke:(left:2pt+{}))[#set text(fill:{})\n#set par(first-line-indent:0pt)\n{}]\n",color(&self.style["quote"]["bg"],"#f3f6ff"),color(&self.style["quote"]["barColor"],"#3b6ef5"),color(&self.style["quote"]["color"],"#4a5568"),self.special_blocks(values)?),
                Block::List{ordered,start,items}=>{
                    let mut values=Vec::new();for item in items{let marker=match item.task{Some(true)=>text("☑ "),Some(false)=>text("☐ "),None=>String::new()};values.push(format!("[{marker}{}]",self.special_blocks(&item.blocks)?));}
                    format!("#{}({}{})\n",if *ordered{"enum"}else{"list"},if *ordered{format!("start:{start},")}else{String::new()},values.join(","))
                },
                Block::Table{header,rows,aligns}=>{
                    let scholarly=crate::export_styles::three_line_table(self.style);
                    let mut cells=Vec::new();for row in rows{for cell in row{cells.push(format!("[{}]",self.inlines(cell)?));}}
                    let mut heads=Vec::new();for cell in header{heads.push(format!("[#text(fill:{},weight:{})[{}]]",color(if scholarly{&self.style["typography"]["color"]}else{&self.style["table"]["headerColor"]},if scholarly{"#111111"}else{"#ffffff"}),string(if self.style["table"]["headerBold"].as_bool().unwrap_or(true){"bold"}else{"regular"}),self.inlines(cell)?));}
                    let column_count=header.len().max(1);let alignments=(0..column_count).map(|i|match aligns.get(i){Some(crate::md_ast::Align::Center)=>"center",Some(crate::md_ast::Align::Right)=>"right",_=>"left"}).collect::<Vec<_>>().join(",");
                    let band=if self.style["table"]["banded"].as_bool().unwrap_or(true){color(&self.style["table"]["bandColor"],"#f3f5f9")}else{"none".into()};
                    if scholarly {
                        let width=n(&self.style["table"]["borderWidth"],0.75,0.25,3.);let border=color(&self.style["table"]["borderColor"],"#111111");
                        let mut entries=vec![format!("table.header(table.hline(stroke:{}pt+{border}),{},table.hline(stroke:{width}pt+{border}))",width*2.,heads.join(","))];
                        entries.extend(cells);entries.push(format!("table.hline(stroke:{}pt+{border})",width*2.));
                        format!("#block(width:{}%)[#set text(size:{}pt)\n#table(columns:{column_count},align:({alignments},),inset:{}pt,stroke:none,fill:none,{})]\n",n(&self.style["table"]["widthPct"],100.,20.,100.),n(&self.style["table"]["cellSize"],10.,6.,36.),n(&self.style["table"]["cellPadding"],6.,0.,30.),entries.join(","))
                    } else {
                    format!("#block(width:{}%)[#set text(size:{}pt)\n#table(columns:{column_count},align:({alignments},),inset:{}pt,stroke:{}pt+{},fill:(x,y)=>if y==0 {{{}}} else if calc.rem(y,2)==0 {{{band}}} else {{none}},table.header({}),{})]\n",n(&self.style["table"]["widthPct"],100.,20.,100.),n(&self.style["table"]["cellSize"],10.,6.,36.),n(&self.style["table"]["cellPadding"],6.,0.,30.),n(&self.style["table"]["borderWidth"],0.75,0.,3.),color(&self.style["table"]["borderColor"],"#c8cdd4"),color(&self.style["table"]["headerBg"],"#3b6ef5"),heads.join(","),cells.join(","))
                    }
                },
                Block::Bibliography(bibliography) => {
                    if bibliography.items.iter().any(|item|!item.first_field.is_empty()||item.layout().map(|flow|crate::publication_rich::layout_requires_flow(&flow)).unwrap_or(true)){
                        self.rich_bibliography(bibliography)?
                    }else{
                    let mut body=format!("#heading(level:2)[{}]\n",text(&bibliography.title));
                    let columns=bibliography.items.iter().any(|item|!item.first_field.is_empty());
                    let gap=f64::from(bibliography.entry_spacing)*f64::from(bibliography.line_spacing);
                    // Typst leading measures edge-to-edge, not baseline-to-
                    // baseline. Fix the bibliography strut to one em; include
                    // the missing inter-line leading between separate grid
                    // rows as well as the CSL additional entry-spacing.
                    let inter_entry=gap+f64::from(bibliography.line_spacing.saturating_sub(1));
                    body.push_str(&format!("#block[\n#set text(top-edge:0.8em,bottom-edge:-0.2em)\n#set par(first-line-indent:0pt,spacing:0pt,hanging-indent:{}em,leading:{}em)\n",if bibliography.hanging_indent&&!columns{2}else{0},bibliography.line_spacing.saturating_sub(1)));
                    let mut cells=Vec::new();
                    for item in &bibliography.items {
                        let entry=self.inlines(&crate::md_ast::rich_inlines(&item.rich))?;
                        if columns {
                            if item.first_field.is_empty(){cells.push(format!("grid.cell(colspan:2)[{entry}]"));}
                            else{cells.push(format!("[{}]",self.inlines(&crate::md_ast::rich_inlines(&item.first_field))?));cells.push(format!("[{entry}]"));}
                        }else{cells.push(format!("[{entry}]"));}
                    }
                    body.push_str(&format!("#grid(columns:{},column-gutter:0.75em,row-gutter:{inter_entry}em,{})\n]\n",if columns{"(auto,1fr)"}else{"(1fr,)"},cells.join(",")));
                    body
                    }
                },
                Block::SectionStart(_)|Block::FootnoteDef{..}=>String::new(),Block::Html(s) if s.trim().is_empty()||s.trim().starts_with("<!--")=>String::new(),
                Block::Html(_)=>return Err("typst_invalid_content".into()),
            };let source=if let (Some(id),Some(nonce))=(mapped,self.mapping_nonce){
                let uri=string(&format!("{}{}",crate::publication_pagination::prefix(nonce),id));
                // Temporary annotations give exact line/image geometry, even
                // across pages and columns. Preserve inherited hyphenation.
                format!("#context {{ let inherited-hyphenation=text.hyphenate; show link.where(dest:{uri}): set text(hyphenate:inherited-hyphenation); link({uri})[{rendered}] }}\n")
            }else{rendered};
            self.source_id=previous_source_id;
            self.ordinary_body=original_body;
            self.author=previous_author;self.author_base=previous_base;
            bytes=bytes.saturating_add(source.len());if bytes>16*1024*1024{return Err("typst_output_limit".into());}
            let flow=!matches!(block,Block::Html(_)|Block::FootnoteDef{..}|Block::SectionStart(_));
            out.push(NativeBlock{source,flow,keep_next:column_keep_next(self.style,block),safe:safe_column_block(self.style,block)});
        }Ok(out)
    }
}
pub fn render(doc:&Document,options:&Value,base:&str,dir:&Path,name:&str)->Result<Source,String>{
    render_inner(doc,options,base,dir,name,None)
}
pub(crate) fn render_mapped(doc:&Document,options:&Value,base:&str,dir:&Path,name:&str,nonce:&str)->Result<Source,String>{
    render_inner(doc,options,base,dir,name,Some(nonce))
}
fn render_inner(doc:&Document,options:&Value,base:&str,dir:&Path,name:&str,nonce:Option<&str>)->Result<Source,String>{
    let resolved_math = crate::math_document_defaults::DocumentDefaultsPlan::analyze(doc)
        .and_then(|plan| plan.render_document(doc)).map_err(|error| error.message())?;
    let doc = resolved_math.as_ref();
    let math_style=crate::export_styles::sanitize(options);
    crate::math_word_projection::validate_render_document_with_style(doc,"typst",&math_style)?;
    crate::publication_blocks::validate_author_render_document(doc).map_err(str::to_string)?;
    let options=crate::publication_note_policies::project_legacy(doc,options).map_err(|error|error.to_string())?;let options=&options;
    crate::publication_typography::validate_typst_document(doc,options)?;
    let (prepared,style,metadata)=crate::publication_metadata_formats::prepare(doc,options,&crate::docx_writer::doc_title(doc,options,name),"pdf")?;
    let front_len=prepared.blocks.len().saturating_sub(doc.blocks.len());
    let doc=&prepared;let (width,height)=crate::export_styles::page_dimensions(&style);
    let sections=crate::publication_sections::resolve(doc,&style).map_err(str::to_string)?;
    let has_sections=sections.iter().any(|section|section.explicit);
    let has_numbered_headings=doc.blocks.iter().any(|block|matches!(block.unstyled(),Block::Heading{numbering:Some(_),..}));
    let layout=crate::publication_layout::Layout::from_style(&style);layout.validate(&style)?;
    let mut notes=HashMap::new();collect_notes(&doc.blocks,&mut notes);
    let mut diagnostic_options=style.clone();diagnostic_options["pdf"]=serde_json::json!({"engine":"typst"});
    let note_warnings=crate::publication_layout::note_warnings("pdf",&diagnostic_options);
    let mut writer=Writer{style:&style,base,dir,notes,seen_notes:HashSet::new(),end_notes:Vec::new(),end_ids:HashMap::new(),end_refs:HashMap::new(),note_numbers:crate::publication_layout::note_numbers(doc,&style),section_ids:sections.iter().filter(|section|section.explicit).map(|section|section.id.clone()).collect(),note_depth:0,images:HashMap::new(),resources:0,bytes:0,warnings:note_warnings,local:None,mapping_nonce:nonce,ordinary_body:true,local_line:None,local_id:None,paragraph_index:0,inline_object_bounds:None,geometry:None,front_blocks:doc.blocks.iter().take(front_len).map(|block|block as *const Block as usize).collect(),author:None,author_base:ResolvedTextStyle::base(n(&style["typography"]["size"],11.,6.,72.),n(&style["typography"]["size"],11.,6.,72.)),local_pair:None,baseline:None,local_baseline:None,source_id:None,baseline_links:Vec::new(),baseline_protected:Vec::new()};
    let title=options.pointer("/meta/title").and_then(Value::as_str).filter(|s|!s.is_empty()).map(str::to_owned).unwrap_or_else(||crate::docx_writer::doc_title(doc,&style,name));
    let author=metadata.author_text();
    let lang=metadata.language.clone().unwrap_or_else(||"en".into());
    let region=crate::md_ast::publication_language_region(&lang);
    let size=n(&style["typography"]["size"],11.,6.,72.);let leading=(n(&style["typography"]["lineHeight"],1.6,1.,3.)-1.)*size;
    let mt=n(&style["page"]["marginTop"],20.,0.,60.);let mb=n(&style["page"]["marginBottom"],20.,0.,60.);
    let ml=n(&style["page"]["marginLeft"],18.,0.,60.);let mr=n(&style["page"]["marginRight"],18.,0.,60.);
    let inside=ml+layout.gutter;let outside=mr;let body_width=width-inside-outside;
    let margin=if layout.mirror{format!("top:{mt}mm,bottom:{mb}mm,inside:{inside}mm,outside:{outside}mm")}else{format!("top:{mt}mm,bottom:{mb}mm,left:{inside}mm,right:{outside}mm")};
    let numbers=if style["footer"]["pageNumbers"].as_bool().unwrap_or(true) {
        let current=format!("#counter(page).display({})",number_format(&layout.page_format));
        if layout.page_total{format!("{current} / #str(counter(page).final().first()-{}+1)",layout.page_start)}else{current}
    }else{String::new()};
    // Position running text from the physical paper edges, independently of
    // body margins. Physical page parity is not affected by custom numbering.
    let running_text=|section:&str|{
        let odd=text(style[section]["text"].as_str().unwrap_or(""));
        if layout.different_odd_even {format!("#if calc.even(here().page()) [{}] else [{odd}]",text(style[section]["evenText"].as_str().unwrap_or("")))}else{odd}
    };
    let running=format!("context {{ if {} or here().page()!=1 {{ let running-left=if {} and calc.even(here().page()) {{{outside}mm}} else {{{inside}mm}};\nplace(top+left,dx:running-left,dy:{}mm)[#block(width:{body_width}mm)[#set text(size:{}pt,fill:rgb(\"#808080\"))\n#set par(first-line-indent:0pt)\n#align({})[{}]]]\nplace(bottom+left,dx:running-left,dy:-{}mm)[#block(width:{body_width}mm)[#set text(size:{}pt,fill:rgb(\"#808080\"))\n#set par(first-line-indent:0pt)\n#align({})[{} {numbers}]]]\n}} }}",
        !layout.hide_first,layout.mirror,layout.header_distance,layout.header_size,align(&style["header"]["align"]),running_text("header"),layout.footer_distance,layout.footer_size,layout.footer_align,running_text("footer"));
    let body=if has_sections{
        let mut body=format!("#let readmd-section-running = ({},)\n",sections.iter().enumerate().map(|(index,section)|section_running(&section.style,index)).collect::<Vec<_>>().join(","));
        for (index,section) in sections.iter().enumerate(){
            if index>0{match section.break_kind{
                crate::publication_sections::SectionBreak::Continuous=>(),
                crate::publication_sections::SectionBreak::NextPage=>body.push_str("#pagebreak(weak:true)\n"),
                crate::publication_sections::SectionBreak::OddPage=>body.push_str(&format!("#metadata({index})<readmd-publication-parity-before>\n#pagebreak(weak:true,to:\"odd\")\n")),
                crate::publication_sections::SectionBreak::EvenPage=>body.push_str(&format!("#metadata({index})<readmd-publication-parity-before>\n#pagebreak(weak:true,to:\"even\")\n")),
            }}
            if index==0||section.break_kind!=crate::publication_sections::SectionBreak::Continuous{body.push_str(&section_page(&section.style));}
            body.push_str(&format!("#metadata((index:{index},starts-page:{}))<readmd-publication-section-boundary>\n",index==0||section.break_kind!=crate::publication_sections::SectionBreak::Continuous));
            body.push_str(&format!("#metadata(none)#label({})\n",string(&private_anchor("section",&section.id))));
            if let Some(start)=section.page_start{body.push_str(&format!("#counter(page).update({start})\n"));}
            writer.style=&section.style;
            let section_layout=crate::publication_layout::Layout::from_style(&section.style);
            if section_layout.note_restart==crate::publication_layout::NoteRestart::Section&&!crate::publication_layout::notes_at_end(&section.style){
                body.push_str(&format!("#counter(footnote).update({})\n",section_layout.note_start-1));
            }
            let blocks=&doc.blocks[section.block_range.clone()];
            if section_layout.columns>1{
                // One traversal owns all note/resource effects and pending
                // source-map markers. Group its rendered bytes afterwards;
                // never call blocks() again to build the measurement parts.
                let (parts,safe)=group_column_parts(writer.rendered_blocks(blocks)?);
                let (section_width,section_height)=crate::export_styles::page_dimensions(&section.style);
                let section_inside=n(&section.style["page"]["marginLeft"],18.,0.,60.)+section_layout.gutter;
                let section_outside=n(&section.style["page"]["marginRight"],18.,0.,60.);
                let section_bottom=section_height-n(&section.style["page"]["marginBottom"],20.,0.,60.);
                let section_size=n(&section.style["typography"]["size"],11.,6.,72.);
                let section_spacing=(n(&section.style["typography"]["lineHeight"],1.6,1.,3.)-1.)*section_size+n(&section.style["typography"]["spacing"],6.,0.,60.);
                let parts=parts.into_iter().map(|part|format!("[{part}\n]")).collect::<Vec<_>>();
                let tuple=if parts.is_empty(){"()".to_string()}else{format!("({},)",parts.join(","))};
                body.push_str(&format!("#readmd-section-columns({tuple},{}mm,{section_bottom}mm,count:{},gutter:{}mm,after:{section_spacing}pt,safe:{safe},id:{})\n",section_width-section_inside-section_outside,section_layout.columns,section_layout.column_gap,string(&section.id)));
                let continuous=section.break_kind==crate::publication_sections::SectionBreak::Continuous
                    ||sections.get(index+1).is_some_and(|next|next.break_kind==crate::publication_sections::SectionBreak::Continuous);
                if continuous&&!writer.warnings.iter().any(|warning|warning=="publication_continuous_columns_partial"){
                    writer.warnings.push("publication_continuous_columns_partial".into());
                }
            }else{body.push_str(&writer.blocks(blocks)?);}
        }
        body
    }else{writer.blocks(&doc.blocks)?};
    let document_notes=writer.document_notes()?;
    let requires_core_text_fallback=crate::publication_fonts::needs_core_text_fallback(&body)
        ||crate::publication_fonts::needs_core_text_fallback(&document_notes)
        ||crate::publication_fonts::needs_core_text_fallback(&running)
        ||(style["cover"]["enabled"].as_bool().unwrap_or(false)
            &&[style["cover"]["title"].as_str().filter(|value|!value.is_empty()).unwrap_or(&title),style["cover"]["subtitle"].as_str().unwrap_or(""),&author,style["cover"]["date"].as_str().unwrap_or("")].iter().any(|value|crate::publication_fonts::needs_core_text_fallback(value)));
    // Typst spacing is edge-to-edge, including between distinct paragraphs.
    // A one-em strut and the extra leading keep a declared lineHeight of 1.1
    // at 11pt at 12.1pt rather than the font cap-height (about 8pt).
    let note_header=note_page_header(&style);let note_separator=if layout.note_separator{"line(length:30%,stroke:0.5pt)"}else{"none"};
    let mut source=format!("// ReadMD generated publication. No author code is executed.\n#set document(title:{},author:{})\n#set page(width:{width}mm,height:{height}mm,binding:left,margin:({margin}),header:{note_header},footer:none,foreground:{running})\n#set text(font:{},size:{size}pt,fill:{},lang:{},top-edge:0.8em,bottom-edge:-0.2em,costs:(widow:{}%,orphan:{}%))\n#set par(leading:{leading}pt,spacing:{}pt,first-line-indent:{}mm,justify:{},hanging-indent:0pt)\n#set align({})\n#show link: set text(fill:{})\n#show raw: set text(font:{})\n#set footnote(numbering:{})\n#set footnote.entry(separator:{note_separator},gap:0.6em)\n#show footnote.entry: set par(first-line-indent:0pt,leading:{}pt)\n#show footnote.entry: set text(size:{}pt,top-edge:\"ascender\",bottom-edge:\"descender\")\n#show heading: set block(sticky:false,breakable:true)\n#counter(page).update({})\n",
        string(&title),format!("({}{})",metadata.authors.iter().map(|person|string(&person.name)).collect::<Vec<_>>().join(","),if metadata.authors.is_empty(){""}else{","}),font_expression(style["typography"]["font"].as_str().unwrap_or("MicrosoftYaHei"),false,requires_core_text_fallback,&mut writer.warnings),color(&style["typography"]["color"],"#262626"),string(lang.split(['-','_']).next().unwrap_or("en")),if layout.widow_control{100}else{0},if layout.widow_control{100}else{0},leading+n(&style["typography"]["spacing"],6.,0.,60.),n(&style["typography"]["firstLineIndent"],0.,0.,30.),style["typography"]["align"]=="justify",align(&style["typography"]["align"]),color(&style["link"]["color"],"#2b6cb0"),font_expression(style["code"]["font"].as_str().unwrap_or("Consolas"),false,requires_core_text_fallback,&mut writer.warnings),number_format(&layout.note_format),(layout.note_line_height-1.)*layout.note_size,layout.note_size,layout.page_start);
    if writer.geometry.is_some(){source.push_str(include_str!("publication_typst_paragraph.typ"));source.push('\n');}
    if layout.note_start!=1 {source.push_str(&format!("#counter(footnote).update({})\n",layout.note_start-1));}
    if let Some(region)=region {source.push_str(&format!("#set text(region:{})\n",string(region)));}
    // Independent columns keep the marker beside the first content block,
    // including locally formatted paragraphs, quotes and tables. The body
    // retains normal pagination and the marker links back to its declaration.
    source.push_str("#show footnote.entry: entry => {\n let note = entry.note\n let mark = counter(footnote).display(at:note.location(),note.numbering)\n grid(columns:(auto,1fr),column-gutter:0.2em,align:(left,top),link(note.location(),super(mark)),note.body)\n}\n");
    let heading_body=if has_numbered_headings{"#if it.numbering == none { it } else { context { counter(heading).display(it.numbering) + it.body } }"}else{"#it"};
    for level in 1..=6{let h=&style["headings"][format!("h{level}")];source.push_str(&format!("#show heading.where(level:{level}): it => {{ {}block(above:{}pt,below:{}pt,sticky:{},breakable:{})[#set text(size:{}pt,fill:{},weight:{})\n#set par(first-line-indent:0pt)\n#align({})[{heading_body}]] }}\n",if h["pageBreakBefore"].as_bool().unwrap_or(false){"pagebreak(weak:true); "}else{""},n(&h["before"],12.,0.,100.),n(&h["after"],8.,0.,100.),h["keepNext"].as_bool().unwrap_or(true),!h["keepLines"].as_bool().unwrap_or(true),n(&h["size"],14.,6.,72.),color(&h["color"],"#262626"),string(if h["bold"].as_bool().unwrap_or(true){"bold"}else{"regular"}),align(&h["align"])));}
    if has_sections&&sections.iter().any(|section|crate::publication_layout::Layout::from_style(&section.style).columns>1){
        source.push_str(include_str!("publication_typst_columns.typ"));source.push('\n');
    }
    if style["cover"]["enabled"].as_bool().unwrap_or(false){
        let cover=style["cover"]["title"].as_str().filter(|s|!s.is_empty()).unwrap_or(&title);
        source.push_str(&format!("#align({}+horizon)[#text(size:28pt,weight:\"bold\")[{}]\n#v(12pt)\n{}\n#v(12pt)\n{}\n#v(12pt)\n{}]\n#pagebreak()\n",align(&style["cover"]["align"]),text(cover),text(style["cover"]["subtitle"].as_str().unwrap_or("")),text(&author),text(style["cover"]["date"].as_str().unwrap_or(""))));
    }
    if style["toc"]["enabled"].as_bool().unwrap_or(false){
        let depth=crate::publication_layout::toc_depth(&style);
        let fill=match crate::publication_layout::toc_leader(&style){"none"=>"none","hyphen"=>"repeat[–]","underscore"=>"repeat[\\_]",_=>"repeat[.]"};
        // fill belongs to outline.entry, not outline (pinned Typst 0.15.1).
        source.push_str(&format!("#set outline.entry(fill:{fill})\n#outline(depth:{depth})\n#pagebreak()\n"));
    }
    if let Some(nonce)=nonce{
        source=source.replace(&format!("#show link: set text(fill:{})",color(&style["link"]["color"],"#2b6cb0")),&format!("#show link: it => if type(it.dest)==str and it.dest.starts-with({}) {{it}} else {{text(fill:{},it)}}",string(&crate::publication_pagination::prefix(nonce)),color(&style["link"]["color"],"#2b6cb0")));
    }
    if !has_sections&&layout.columns>1{source.push_str(&format!("#columns({},gutter:{}mm)[{body}]",layout.columns,layout.column_gap));}else{source.push_str(&body);}
    source.push_str(&document_notes);
    if has_sections{source.push_str("\n#metadata(none)<readmd-publication-document-end>\n");}
    if !writer.seen_notes.is_empty(){
        if writer.seen_notes.len()>crate::publication_blocks::MAX_NODES{return Err("typst_output_limit".into());}
        let geometry=writer.geometry.get_or_insert_with(GeometryContext::new);
        source.push_str(&geometry.note_guard(writer.seen_notes.len()));
    }
    Ok(Source{text:source,warnings:writer.warnings,requires_core_text_fallback,geometry:writer.geometry,baseline_links:writer.baseline_links,baseline_protected:writer.baseline_protected})
}

#[cfg(test)]
#[path="typst_writer_columns_tests.rs"]
mod columns_tests;

#[cfg(test)]
#[path="typst_notes_layout_tests.rs"]
mod notes_layout_tests;

#[cfg(test)]
#[path="typst_writer_author_inline_tests.rs"]
mod author_inline_tests;

#[cfg(test)]
#[path="typst_heading_numbering_tests.rs"]
mod heading_numbering_tests;

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn supplemental_fallback_uses_visible_content_without_changing_hidden_metadata_or_latin_faces(){
        let dir=tempfile::tempdir().unwrap();
        let plain=render(&crate::md_ast::parse("Ordinary Latin and α."),&serde_json::json!({"meta":{"title":"Hidden 😀"},"frontMatter":{"mode":"none"},"cover":{"enabled":false}}),"",dir.path(),"Paper").unwrap();
        assert!(!plain.requires_core_text_fallback);assert!(!plain.text.contains("\"DejaVu Sans\""));
        let mut doc=crate::md_ast::parse("Body 😀 and `😀`.\n\n```text\n😀\n```\n");
        doc.blocks.push(Block::Paragraph(vec![Inline::Styled{format:crate::publication_rich::RichFormat{font_variant:crate::publication_rich::RichFontVariant::SmallCaps,..Default::default()},children:vec![Inline::Text("Abc😀".into())]}]));
        let rich=render(&doc,&serde_json::json!({"cover":{"enabled":false}}),"",dir.path(),"Paper").unwrap();
        assert!(rich.requires_core_text_fallback);assert!(rich.text.contains("font:(\"Libertinus Serif\",\"DejaVu Sans\")"));
        assert!(rich.text.contains("#text(font:(\"Libertinus Serif\",\"DejaVu Sans\"))[#smallcaps["));
        assert!(rich.text.contains("#text(\"Abc😀\")"));
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none(),"Writer alone must not materialize fonts or sidecars");
    }
    #[test]fn running_and_local_typography_keep_supplemental_glyph_fallback(){
        let dir=tempfile::tempdir().unwrap();
        let mut doc=crate::md_ast::parse("Local 😀.");
        let block=doc.blocks.remove(0);doc.blocks.push(Block::Styled{attributes:serde_json::json!({"text":{"font":"Arial"}}),block:Box::new(block)});
        let source=render(&doc,&serde_json::json!({"header":{"text":"Header 😀"}}),"",dir.path(),"Paper").unwrap();
        assert!(source.requires_core_text_fallback);assert!(source.text.contains("DejaVu Sans"));assert!(source.text.contains("#text(\"Header 😀\")"));
    }
    #[test]fn pinned_font_routes_cover_rich_axes_and_keep_managed_sans_regular_bold(){
        use crate::publication_rich::*;
        // Expected families come from the locked engine's actual face audit:
        // Serif has R/B/I/BI and smcp; SC has only R/B. No Libertinus Sans
        // face exists, including for regular or bold-normal requests.
        let formats=[
            RichFormat::default(),
            RichFormat{font_style:RichFontStyle::Italic,..Default::default()},
            RichFormat{font_style:RichFontStyle::Oblique,..Default::default()},
            RichFormat{font_weight:RichFontWeight::Bold,..Default::default()},
            RichFormat{font_style:RichFontStyle::Italic,font_weight:RichFontWeight::Bold,..Default::default()},
            RichFormat{font_variant:RichFontVariant::SmallCaps,..Default::default()},
            RichFormat{font_style:RichFontStyle::Italic,font_weight:RichFontWeight::Bold,font_variant:RichFontVariant::SmallCaps,..Default::default()},
        ];
        for managed in [false,true]{for (requested,cjk) in [("MicrosoftYaHei","ReadMD Sans SC"),("Arial","ReadMD Sans SC"),("SimSun","ReadMD Serif SC"),("Libertinus Sans","ReadMD Sans SC"),("libertinus sans","ReadMD Sans SC")]{for format in &formats{
            let latin=format.font_style!=RichFontStyle::Normal||format.font_variant==RichFontVariant::SmallCaps;
            let actual=font_faces(requested,managed,latin);
            let expected=if !managed||latin||requested.eq_ignore_ascii_case("Libertinus Sans"){vec!["Libertinus Serif"]}else{vec![cjk]};
            assert_eq!(actual[0],expected[0],"managed={managed}, requested={requested}, format={format:?}");
            if managed&&actual[0]=="Libertinus Serif"{assert_eq!(actual,vec!["Libertinus Serif",cjk]);}else{assert_eq!(actual,expected);}
            assert!(!actual.iter().any(|name|name.eq_ignore_ascii_case("Libertinus Sans")));
        }}}
        assert_eq!(font_faces("Libertinus Serif",true,true),vec!["Libertinus Serif","ReadMD Serif SC"]);
        assert_eq!(font_faces("Consolas",true,false),vec!["DejaVu Sans Mono"]);
    }
    #[test]fn rich_font_axes_use_pinned_serif_preserve_unicode_and_report_fallback_once(){
        use crate::publication_rich::*;
        let dir=tempfile::tempdir().unwrap();
        let formats=[
            (RichFormat::default(),"normal",400,false),
            (RichFormat{font_style:RichFontStyle::Italic,..Default::default()},"italic",400,false),
            (RichFormat{font_style:RichFontStyle::Oblique,..Default::default()},"oblique",400,false),
            (RichFormat{font_weight:RichFontWeight::Bold,..Default::default()},"normal",700,false),
            (RichFormat{font_style:RichFontStyle::Italic,font_weight:RichFontWeight::Bold,..Default::default()},"italic",700,false),
            (RichFormat{font_variant:RichFontVariant::SmallCaps,..Default::default()},"normal",400,true),
            (RichFormat{font_style:RichFontStyle::Italic,font_weight:RichFontWeight::Bold,font_variant:RichFontVariant::SmallCaps,..Default::default()},"italic",700,true),
        ];
        for requested in [None,Some("Arial"),Some("SimSun"),Some("Libertinus Sans")]{for (format,style,weight,smallcaps) in &formats{
            let leaf=Inline::Styled{format:format.clone(),children:vec![Inline::Text("Abc 中文 😀".into())]};
            let doc=Document{blocks:vec![Block::Paragraph(vec![leaf.clone(),leaf])],..Default::default()};
            let mut options=serde_json::json!({"cover":{"enabled":false}});if let Some(requested)=requested{options["typography"]=serde_json::json!({"font":requested});}
            let source=render(&doc,&options,"",dir.path(),"Rich").unwrap();
            assert!(source.text.contains("#text(font:(\"Libertinus Serif\",\"DejaVu Sans\"))"),"{requested:?} {format:?}: {}",source.text);
            assert!(source.text.contains(&format!("#text(style:{},weight:{weight})",string(style))));
            assert_eq!(source.text.matches("#smallcaps[").count(),if *smallcaps{2}else{0});
            assert_eq!(source.text.matches("#text(\"Abc 中文 😀\")").count(),2);
            assert!(!source.text.contains("ABC 中文"));assert!(!source.text.contains("Libertinus Sans"));
            assert_eq!(source.warnings,vec!["font_fallback: Libertinus Serif"],"{requested:?} {format:?}");
            assert!(source.requires_core_text_fallback);
        }}
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }
    #[test]fn global_local_and_rich_absent_sans_routes_share_one_real_font_fallback(){
        use crate::publication_rich::*;
        let dir=tempfile::tempdir().unwrap();
        for (style,weight,variant) in [(RichFontStyle::Normal,RichFontWeight::Normal,RichFontVariant::Normal),(RichFontStyle::Italic,RichFontWeight::Normal,RichFontVariant::Normal),(RichFontStyle::Oblique,RichFontWeight::Normal,RichFontVariant::Normal),(RichFontStyle::Normal,RichFontWeight::Bold,RichFontVariant::Normal),(RichFontStyle::Italic,RichFontWeight::Bold,RichFontVariant::Normal),(RichFontStyle::Normal,RichFontWeight::Normal,RichFontVariant::SmallCaps),(RichFontStyle::Italic,RichFontWeight::Bold,RichFontVariant::SmallCaps)]{
            let doc=Document{blocks:vec![Block::Styled{attributes:serde_json::json!({"text":{"font":"Libertinus Sans","bold":weight==RichFontWeight::Bold,"italic":style!=RichFontStyle::Normal}}),block:Box::new(Block::Paragraph(vec![Inline::Text("Local 中文 😀 ".into()),Inline::Styled{format:RichFormat{font_style:style,font_weight:weight,font_variant:variant,..Default::default()},children:vec![Inline::Text("Abc 中文 😀".into())]}]))}],..Default::default()};
            let source=render(&doc,&serde_json::json!({"typography":{"font":"Libertinus Sans"},"code":{"font":"Libertinus Sans"},"cover":{"enabled":false}}),"",dir.path(),"Local").unwrap();
            assert!(!source.text.contains("Libertinus Sans"));
            assert!(source.text.contains("#set text(font:(\"Libertinus Serif\",\"DejaVu Sans\")"));
            assert!(source.text.contains("#text(\"Local 中文 😀 \")"));assert!(source.text.contains("#text(\"Abc 中文 😀\")"));
            assert_eq!(source.warnings,vec!["font_fallback: Libertinus Serif"]);
        }
    }
    #[test]fn rich_light_is_still_rejected_for_default_explicit_and_local_absent_sans(){
        use crate::publication_rich::*;
        let dir=tempfile::tempdir().unwrap();
        let doc=Document{blocks:vec![Block::Paragraph(vec![Inline::Styled{format:RichFormat{font_weight:RichFontWeight::Light,..Default::default()},children:vec![Inline::Text("Light".into())]}])],..Default::default()};
        assert!(matches!(render(&doc,&serde_json::json!({}),"",dir.path(),"Rich"),Err(code)if code=="academic_rich_unsupported"));
        assert!(matches!(render(&doc,&serde_json::json!({"typography":{"font":"Libertinus Sans"}}),"",dir.path(),"Rich"),Err(code)if code=="academic_rich_unsupported"));
        let local=Document{blocks:vec![Block::Styled{attributes:serde_json::json!({"text":{"font":"Libertinus Sans"}}),block:Box::new(doc.blocks[0].clone())}],..Default::default()};
        assert!(matches!(render(&local,&serde_json::json!({}),"",dir.path(),"Rich"),Err(code)if code=="academic_rich_unsupported"));
    }
    #[test]fn bibliography_uses_one_shared_column_grid_and_adjusted_line_height_gap(){
        use crate::publication_rich::*;
        let dir=tempfile::tempdir().unwrap();
        for (line,entry,gap) in [(1,0,"0"),(1,2,"2"),(3,0,"2"),(3,2,"8")]{
            let doc=Document{blocks:vec![Block::Bibliography(RichBibliography{title:"References".into(),items:vec![RichBibliographyItem{key:"a".into(),first_field:vec![RichNode::text("[9]")],rich:vec![RichNode::text("First long entry")]},RichBibliographyItem{key:"b".into(),first_field:vec![RichNode::text("[100]")],rich:vec![RichNode::text("Second long entry")]}],hanging_indent:false,line_spacing:line,entry_spacing:entry,source_span:None})],..Default::default()};
            let source=render(&doc,&serde_json::json!({"cover":{"enabled":false}}),"",dir.path(),"Rich").unwrap();
            assert_eq!(source.text.matches("#layout(size => grid(columns:(calc.min(readmd-csl-label-width,size.width*0.35),1fr)").count(),1);
            assert!(source.text.contains(&format!("row-gutter:({gap}em,)")));
            assert!(source.text.contains("#set text(top-edge:0.8em,bottom-edge:-0.2em)"));
            assert!(source.text.contains(&format!("leading:{}em",line-1)));
            assert!(!source.text.contains("#block(below:"));
        }
    }
    #[test]fn publication_language_and_region_reach_actual_engine_source() {
        let dir=tempfile::tempdir().unwrap();
        for (tag,primary,region) in [("de_DE","de",Some("DE")),("zh-Hant-TW","zh",Some("TW")),("es-419","es",None),("en-u-ca-gregory-x-US","en",None)] {
            let doc=crate::md_ast::parse(&format!("---\nlang: {tag}\n---\n# Title\nBody"));let source=render(&doc,&serde_json::json!({}),"",dir.path(),"Paper").unwrap().text;
            assert!(source.contains(&format!("lang:\"{primary}\"")));assert_eq!(source.contains("#set text(region:"),region.is_some());if let Some(region)=region{assert!(source.contains(&format!("#set text(region:\"{region}\")")));}
        }
        let doc=crate::md_ast::parse("---\nlang: de-DE\n---\nBody");let source=render(&doc,&serde_json::json!({"meta":{"language":"fr-CA"}}),"",dir.path(),"Paper").unwrap().text;assert!(source.contains("lang:\"fr\""));assert!(source.contains("region:\"CA\""));
    }
    #[test]fn toc_parameters_reach_real_typst_outline_without_raw_code(){
        let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("# One\n\n## Two\n\n###### Six");
        for (leader,fill) in [("dot","repeat[.]"),("hyphen","repeat[–]"),("underscore","repeat[\\_]"),("none","none")]{
            let style=crate::export_styles::sanitize(&serde_json::json!({"toc":{"enabled":true,"depth":6,"leader":leader}}));
            let source=render(&doc,&style,"",dir.path(),"Outline").unwrap().text;
            assert!(source.contains(&format!("#set outline.entry(fill:{fill})\n#outline(depth:6)")));
            assert!(!source.contains("#outline(depth:6,fill:"));
        }
        let style=serde_json::json!({"toc":{"enabled":true,"depth":999,"leader":"#read(\"private\")"}});
        let source=render(&doc,&style,"",dir.path(),"Outline").unwrap().text;
        assert!(source.contains("#set outline.entry(fill:repeat[.])\n#outline(depth:6)"));assert!(!source.contains("private"));
    }
    #[test]fn running_pages_use_physical_parity_and_keep_first_page_independent(){
        let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("# Book\n\nText.");
        let style=crate::export_styles::sanitize(&serde_json::json!({"page":{"differentOddEven":true,"differentFirstPage":true},"header":{"text":"Odd","evenText":"Even #read(\"private\")"},"footer":{"text":"Odd footer","evenText":"","start":8}}));
        let source=render(&doc,&style,"",dir.path(),"Book").unwrap().text;assert!(source.contains("if false or here().page()!=1"));assert!(source.contains("#if calc.even(here().page())"));assert!(source.contains("#counter(page).update(8)"));assert!(source.contains(&text("Even #read(\"private\")")));assert!(source.contains(&format!("else [{}]",text("Odd footer"))));
    }
    #[test]fn note_start_offsets_native_counter_and_shared_endnote_labels() {
        let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("Body[^a] repeat[^a], other[^b].\n\n[^a]: A.\n\n[^b]: B.\n");
        for position in ["page","document"] {
            let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":position,"start":27,"numberFormat":"upperLetter"}}));
            let source=render(&doc,&style,"",dir.path(),"Notes").unwrap().text;
            assert!(source.contains("#counter(footnote).update(26)"));
            if position=="document" {
                assert_eq!(source.matches(&format!("#numbering({},27)",number_format("upperLetter"))).count(),3);
                assert_eq!(source.matches(&format!("#numbering({},28)",number_format("upperLetter"))).count(),2);
            }else{
                assert_eq!(source.matches("#footnote[").count(),2);
                assert_eq!(source.matches("#footnote(label(").count(),1);
            }
        }
    }
    #[test]fn document_notes_follow_body_reuse_identity_and_link_every_reference(){
        let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("# First\n\nBody[^a] and repeat[^a].\n\n# Last\n\nLast body[^b].\n\n[^a]: First note.\n\n    > Quoted note.\n\n    $x^2$\n\n[^b]: Last note.\n");
        let source=render(&doc,&serde_json::json!({"footnotes":{"position":"document","title":"Notes #read(\"private\")","numberFormat":"lowerRoman"}}),"",dir.path(),"Book").unwrap().text;
        assert!(!source.contains("#footnote["));assert!(!source.contains("#footnote(label("));
        assert_eq!(source.matches("#text(\"First note.\")").count(),1);assert!(source.find("#text(\"Last body\")").unwrap()<source.find("#text(\"First note.\")").unwrap());
        for (kind,id) in [("note","a"),("note","b"),("note-ref","a:1"),("note-ref","a:2"),("note-ref","b:1")]{assert!(source.contains(&format!("#label({})",string(&private_anchor(kind,id)))));}
        assert!(source.contains("#numbering(\"i\",1)"));assert!(source.contains("#text(\"Notes #read(\\\"private\\\")\")"));assert!(source.contains("#image("));
    }
    #[test]fn endnote_mode_without_references_adds_no_empty_notes_page(){
        let dir=tempfile::tempdir().unwrap();let source=render(&crate::md_ast::parse("Just text."),&serde_json::json!({"footnotes":{"position":"document"}}),"",dir.path(),"Book").unwrap().text;
        assert!(!source.contains("#pagebreak(weak:true)"));assert!(!source.contains("outlined:false"));
        assert_eq!(render(&crate::md_ast::parse("Text[^a].\n\n[^a]: Nested[^b].\n\n[^b]: B."),&serde_json::json!({"footnotes":{"position":"document"}}),"",dir.path(),"Book").err().unwrap(),"typst_invalid_content");
    }
    #[test]fn author_text_never_becomes_executable_typst(){
        let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("# Unsafe #read(\"private\")\n\nUse [links](https://example.test) and `#import`.");
        let source=render(&doc,&serde_json::json!({}),"",dir.path(),"Test").unwrap().text;
        assert!(source.contains("#text(\"Unsafe #read(\\\"private\\\")\")"));assert!(!source.contains("#import \"@preview"));
        assert_eq!(string("\u{0}\n\\\""),"\"\\u{0}\\n\\\\\\\"\"");
    }
    #[test]fn shared_styles_notes_and_links_are_real_engine_operations(){
        let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("# Evidence {#chapter}\n\nOne[^n] and again[^n], [jump](#chapter).\n\n[^n]: A real footnote.\n\n|A|B|\n|-|-|\n|1|2|");
        let source=render(&doc,&serde_json::json!({"page":{"columns":2,"columnGap":7},"typography":{"firstLineIndent":8}}),"",dir.path(),"Test").unwrap().text;
        assert!(source.contains("#columns(2,gutter:7mm)"));assert!(source.contains("first-line-indent:8mm"));assert_eq!(source.matches("#footnote[").count(),1);assert_eq!(source.matches("#footnote(label(").count(),1);assert!(source.contains("table.header("));assert!(source.contains("#link(label("));
    }
    #[test]fn captioned_svg_survives_publication_preparation_and_is_staged_losslessly(){
        let input=tempfile::tempdir().unwrap();let output=tempfile::tempdir().unwrap();
        let svg=r#"<svg xmlns="http://www.w3.org/2000/svg" width="2400" height="800"><rect width="2400" height="800" fill="blue"/></svg>"#;
        fs::write(input.path().join("route.svg"),svg).unwrap();
        let prepared=crate::publication::prepare("# Reading\n\n```readmd-math v1\nfrac(a+b,c)\n```\n\n:label[eq:estimate] Estimate\n\n![Route](route.svg)\n\n:label[fig:route] Evidence map\n\nSee :ref[fig:route].",input.path().to_str().unwrap()).unwrap();
        let doc=crate::md_ast::parse(&prepared.content);
        let source=render(&doc,&serde_json::json!({}),input.path().to_str().unwrap(),output.path(),"Test").unwrap();
        assert!(source.text.contains("#image("),"Image lost: {}",source.text);
        assert_eq!(source.warnings,vec!["font_fallback: Libertinus Serif"]);
        assert_eq!(fs::read_to_string(output.path().join("resource-2.svg")).unwrap(),svg);
    }
    #[test]fn notes_citations_and_figure_survive_together(){
        let input=tempfile::tempdir().unwrap();let output=tempfile::tempdir().unwrap();
        fs::write(input.path().join("references.bib"),"@book{kant,title={Critique},author={Kant, Immanuel},year={1781}}").unwrap();
        fs::write(input.path().join("route.svg"),r##"<svg xmlns="http://www.w3.org/2000/svg" width="2400" height="800"><rect width="2400" height="800" fill="#3048ac"/></svg>"##).unwrap();
        let prepared=crate::publication::prepare("---\ntitle: Evidence\nbibliography: references.bib\ncsl: chicago-notes\n---\n# Evidence\n\nReading [@kant, p. 42] and note[^author].\n\n![Route](route.svg)\n\n:label[fig:route] Evidence\n\n[^author]: Author note.\n",input.path().to_str().unwrap()).unwrap();
        let source=render(&crate::md_ast::parse(&prepared.content),&serde_json::json!({}),input.path().to_str().unwrap(),output.path(),"Test").unwrap();
        assert_eq!(source.warnings,vec!["font_fallback: Libertinus Serif"]);
        assert!(source.text.contains("#image("),"Image lost: {}\n{}",prepared.content,source.text);
        assert!(source.text.contains("#footnote["));
    }
}

#[cfg(test)]
#[path="typst_writer_paragraphs_tests.rs"]
mod paragraph_tests;
