//! Source-aware DSL lowering for the standalone HTML exporter. Markdown still
//! owns surrounding content; formulas share the native Word/PDF math model.
use std::{borrow::Cow, collections::BTreeSet, ops::Range};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

pub(crate) struct Projection<'a> {
    pub markdown:Cow<'a,str>,
    pub warnings:Vec<String>,
    pub formulas:usize,
}

pub(crate) const CSS:&str=".readmd-export-math-block { text-align:center; margin:1em 0; break-inside:avoid; } .readmd-export-math svg { max-width:100%; }";

pub(crate) fn lower_dsl(source:&str,size:f64)->Result<Projection<'_>,String>{
    // Direct HTML export uses this path without the dispatcher preflight.
    // A stale or unmapped carrier must fail before any destination is written.
    crate::math_dsl::expand_markdown(source).map_err(str::to_string)?;
    if !crate::math_dsl::version_diagnostics(source).is_empty(){return Err("math_unsupported_version".into());}
    let mut formulas=Vec::<(Range<usize>,String,bool)>::new();
    let mut block=None::<(Range<usize>,String)>;
    // Event text removes quote/list indentation. The source span still points
    // to the original fence so the surrounding container prefixes survive.
    for (event,range) in Parser::new_ext(source,Options::ENABLE_YAML_STYLE_METADATA_BLOCKS|Options::ENABLE_FOOTNOTES).into_offset_iter(){
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim()=="readmd-math v1"=>block=Some((range,String::new())),
            Event::Text(text)=>if let Some((_,body))=&mut block{if body.len()+text.len()>crate::math_dsl::MAX_SOURCE{return Err("math_budget".into());}body.push_str(&text);},
            Event::End(TagEnd::CodeBlock)=>if let Some((range,body))=block.take(){formulas.push((range,body,true));},
            _=>(),
        }
    }
    for r in crate::math_dsl::markdown_ranges(source).into_iter().filter(|r|!r.display){
        formulas.push((r.from..r.to,source[r.body_from..r.body_to].to_string(),false));
    }
    if formulas.is_empty(){return Ok(Projection{markdown:Cow::Borrowed(source),warnings:Vec::new(),formulas:0});}
    if source.len()>crate::document_transaction::MAX_DOCUMENT_BYTES||formulas.len()>crate::document_transaction::MAX_OPERATIONS{return Err("math_budget".into());}
    formulas.sort_by_key(|(range,_,_)|range.start);
    let count=formulas.len();let mut out=String::with_capacity(source.len());let mut cursor=0;let mut warnings=BTreeSet::new();
    for (range,body,display) in formulas {
        if range.start<cursor||range.end>source.len(){return Err("math_invalid_source".into());}
        let (root,_)=crate::math_dsl::compile(body.trim_end_matches(['\r','\n']),display).map_err(str::to_string)?;
        let tex=crate::math_formats::tex(&root);let xml=root.xml();
        let parsed=crate::omml::parse(&xml).map_err(|_|"math_invalid_source".to_string())?;
        if !crate::math_layout::font_warnings(&parsed).is_empty(){warnings.insert("math_font_substituted".to_string());}
        let (drawing,label)=if let Some(formula)=crate::math_layout::layout_source(&tex,Some(&xml),display,size){
            (formula.publication_svg(),formula.copy_text())
        }else{
            warnings.insert("math_font_substituted".to_string());
            warnings.extend(crate::omml::mathml_warnings(&parsed));
            (crate::omml::to_mathml(&parsed,display,&tex),tex)
        };
        let (tag,class)=if display{("div","readmd-export-math readmd-export-math-block")}else{("span","readmd-export-math")};
        out.push_str(&source[cursor..range.start]);
        let semantics=if label.is_empty(){"aria-hidden=\"true\"".to_string()}else{format!("role=\"math\" aria-label=\"{}\"",crate::epub_writer::esc(&label))};
        out.push_str(&format!("<{tag} class=\"{class}\" {semantics}>{drawing}</{tag}>"));
        if display{out.push('\n');}
        if out.len()>64*1024*1024{return Err("math_budget".into());}
        cursor=range.end;
    }
    out.push_str(&source[cursor..]);
    if out.len()>64*1024*1024{return Err("math_budget".into());}
    Ok(Projection{markdown:Cow::Owned(out),warnings:warnings.into_iter().collect(),formulas:count})
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn html_dsl_lowering_renders_each_formula_but_leaves_literal_examples_and_latex(){
        let source="First :math[frac(a,b)] and :math[frac(a,b)].\n\n```readmd-math v1\nx^2 + α\n```\n\n`:math[frac(a,b)]`\n\n````text\n```readmd-math v1\nfrac(a,b)\n```\n````\n\n<div>:math[raw]</div>\n\n$x^2$\n";
        let p=lower_dsl(source,12.).unwrap();assert_eq!(p.formulas,3);assert_eq!(p.markdown.matches("<svg ").count(),3);assert_eq!(p.markdown.matches("role=\"math\"").count(),3);
        for literal in ["`:math[frac(a,b)]`","````text\n```readmd-math v1\nfrac(a,b)\n```\n````","<div>:math[raw]</div>","$x^2$"]{assert!(p.markdown.contains(literal),"{literal}");}
        assert!(p.warnings.is_empty());assert!(p.markdown.starts_with("First <span"));assert!(!p.markdown.contains("<!--readmd:math:v1:"));
        let plain=lower_dsl("# Plain\n\n$x^2$",12.).unwrap();assert!(matches!(plain.markdown,Cow::Borrowed(_)));
    }
    #[test]fn html_dsl_lowering_preserves_nested_quote_and_list_containers(){
        let source="> Quoted formula\n>\n> ```readmd-math v1\n> frac(a,b)\n> ```\n\n- List formula\n\n  ```readmd-math v1\n  x^2\n  ```\n\nEnd.\n";
        let p=lower_dsl(source,12.).unwrap();assert_eq!(p.formulas,2);assert_eq!(p.markdown.matches("<svg ").count(),2);
        let mut html=String::new();pulldown_cmark::html::push_html(&mut html,Parser::new(&p.markdown));assert!(html.contains("<blockquote>"));assert!(html.contains("<ul>"));
        let quote=html.split("</blockquote>").next().unwrap();assert!(quote.contains("<svg "));let list=html.split("<ul>").nth(1).unwrap().split("</ul>").next().unwrap();assert!(list.contains("<svg "));assert!(html.contains("End."));
    }
    #[test]fn html_dsl_lowering_reports_missing_vector_glyphs_instead_of_exporting_empty_math(){
        let source=format!(":math[\"{}\"]",'\u{10fffd}');let p=lower_dsl(&source,12.).unwrap();assert_eq!(p.formulas,1);assert!(p.markdown.contains("<math"));assert!(p.warnings.contains(&"math_font_substituted".into()));assert!(!p.markdown.contains(":math["));
    }
}
