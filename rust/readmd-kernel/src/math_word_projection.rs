//! Pure consumer checks for the associated Word paragraph. Import/source
//! preservation is deliberately independent from a target's rendering support.
use crate::{md_ast::{Block,Document,Inline},omml};

pub(crate) fn validate_formula(xml:&str,display:bool,width:Option<f64>,format:&str)->Result<(),String>{
    validate_formula_in_host(xml,display,width,format,false)
}

fn validate_formula_in_host(xml:&str,display:bool,width:Option<f64>,format:&str,nested:bool)->Result<(),String>{
    let root=omml::parse(xml)?;
    omml::validate_display_context(&root,display,width).map_err(str::to_string)?;
    if display&&nested&&root.word_paragraph.as_ref().is_some_and(|context|
        context.effective.alignment.is_some()||!context.effective.indent.is_empty()){
        // Generic host style application can overwrite this native paragraph,
        // and quote/list/note geometry is not a top-level body box. Reject the
        // unproven combination before resources rather than silently resetting
        // Word axes. Ordinary formulas and valid inline oMath keep their paths.
        return Err("math_unsupported_extension".into());
    }
    if display&&format=="docx"{omml::native_word_paragraph(&root).map_err(str::to_string)?;}
    if display&&matches!(format,"tex"|"latex"|"xelatex"|"pdflatex") {
        let check=|paragraph:Option<&omml::Node>|->Result<(),String>{
            if let Some(word)=omml::word_display_paragraph(&root,paragraph).map_err(str::to_string)? {
                if word.alignment.is_some()||word.left.is_some()||word.right.is_some(){
                    // The current TeX writer is a LaTeX editing projection; it
                    // does not yet consume Office paragraph geometry.
                    return Err("math_unsupported_extension".into());
                }
            }
            Ok(())
        };
        for paragraph in omml::display_scopes(&root){check(paragraph)?;}
    }
    Ok(())
}

pub(crate) fn validate_document(doc:&Document,format:&str)->Result<(),String>{
    validate_document_at_width(doc,format,None)
}
pub(crate) fn validate_document_at_width(doc:&Document,format:&str,width:Option<f64>)->Result<(),String>{
    let resolved = crate::math_document_defaults::DocumentDefaultsPlan::analyze(doc)
        .and_then(|plan| plan.render_document(doc)).map_err(|error| error.message())?;
    validate_render_document_at_width(resolved.as_ref(),format,width)
}

pub(crate) fn validate_render_document_at_width(doc:&Document,format:&str,width:Option<f64>)->Result<(),String>{
    validate_inlines(doc,format)?;
    validate_blocks(&doc.blocks,format,width,false)
}

fn validate_inlines(doc:&Document,format:&str)->Result<(),String>{
    let mut result=Ok(());
    crate::md_ast::walk_inlines(&doc.blocks,&mut |inline|{
        if result.is_ok(){if let Inline::OfficeMath{omml,..}=inline{result=validate_formula(omml,false,None,format);}}
    });
    result
}

fn validate_blocks(values:&[Block],format:&str,width:Option<f64>,nested:bool)->Result<(),String>{
    for block in values {match block {
        Block::OfficeMath{omml,..}=>validate_formula_in_host(omml,true,width,format,nested)?,
        Block::Styled{block,..}=>validate_blocks(std::slice::from_ref(block.as_ref()),format,width,true)?,
        Block::List{items,..}=>for item in items{validate_blocks(&item.blocks,format,width,true)?;},
        Block::Quote(inner)|Block::FootnoteDef{blocks:inner,..}=>validate_blocks(inner,format,width,true)?,
        _=>(),
    }}
    Ok(())
}

/// Paged consumers must use the formula's actual section, not a global width
/// that could admit a negative inner box (or reject a valid wider section).
pub(crate) fn validate_document_with_style(doc:&Document,format:&str,style:&serde_json::Value)->Result<(),String>{
    let resolved = crate::math_document_defaults::DocumentDefaultsPlan::analyze(doc)
        .and_then(|plan| plan.render_document(doc)).map_err(|error| error.message())?;
    validate_render_document_with_style(resolved.as_ref(),format,style)
}

/// The caller owns the checked author-to-render plan. Re-analyzing this derived
/// tree would lose which formulas were unconfigured in the author's document.
pub(crate) fn validate_render_document_with_style(doc:&Document,format:&str,style:&serde_json::Value)->Result<(),String>{
    if matches!(format,"docx"|"typst") {
        validate_inlines(doc,format)?;
        for section in crate::publication_sections::resolve(doc,style).map_err(str::to_string)? {
            let width=crate::publication_layout::Layout::from_style(&section.style).column_width(&section.style)*72./25.4;
            validate_blocks(&doc.blocks[section.block_range],format,Some(width),false)?;
        }
        Ok(())
    }else{
        let width=(format=="rust").then(||crate::pdf_render::DocGeometry::from_style(style).avail);
        validate_render_document_at_width(doc,format,width)
    }
}

pub(crate) fn validate_source(source:&str,format:&str)->Result<(),String>{
    validate_source_at_width(source,format,None)
}
pub(crate) fn validate_source_at_width(source:&str,format:&str,width:Option<f64>)->Result<(),String>{
    // Expansion owns the real Markdown ranges, old carriers and diagnostics.
    // No image, reference, font, worker lease or destination is touched here.
    let expanded=crate::math_dsl::expand_markdown(source).map_err(str::to_string)?;
    validate_document_at_width(&crate::md_ast::parse(&expanded),format,width)
}

pub(crate) fn validate_source_with_style(source:&str,format:&str,style:&serde_json::Value)->Result<(),String>{
    let expanded=crate::math_dsl::expand_markdown(source).map_err(str::to_string)?;
    validate_document_with_style(&crate::md_ast::parse(&expanded),format,style)
}
pub(crate) fn validate_json_at_width(value:&serde_json::Value,format:&str,width:Option<f64>)->Result<(),String>{
    let resolved=crate::math_render_json::resolve(value)?;
    validate_render_json_at_width(resolved.as_ref(),format,width)
}

pub(crate) fn validate_render_json_at_width(value:&serde_json::Value,format:&str,width:Option<f64>)->Result<(),String>{
    crate::math_render_json::visit_math_sites(value,|node,display,nested|{
        if let Some(xml)=node.get("omml").and_then(serde_json::Value::as_str){
            validate_formula_in_host(xml,display,if display{width}else{None},format,nested)?;
        }
        Ok(())
    })
}

pub(crate) fn uses_word_geometry(xml:&str)->Result<bool,String>{
    let root=omml::parse(xml)?;
    omml::validate_display_context(&root,true,None).map_err(str::to_string)?;
    for paragraph in omml::display_scopes(&root){
        if let Some(word)=omml::word_display_paragraph(&root,paragraph).map_err(str::to_string)? {
            if word.alignment.is_some()||word.left.is_some()||word.right.is_some(){return Ok(true);}
        }
    }
    Ok(false)
}

pub(crate) fn static_code(code:&str)->&'static str{
    if code.starts_with("math_document_defaults_conflict"){return "math_document_defaults_conflict";}
    match code {
        "math_invalid_property"=>"math_invalid_property","math_invalid_xml"=>"math_invalid_xml",
        "math_budget"=>"math_budget","math_unsupported_extension"=>"math_unsupported_extension",
        _=>"math_invalid_source",
    }
}

#[cfg(test)]
#[path="math_word_projection_tests.rs"]
mod tests;
