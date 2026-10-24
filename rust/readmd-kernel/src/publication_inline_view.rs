//! Trusted author-size projection for the reader, editor preview and HTML.
//! Source stays human-readable. Only live parser ranges produce view markers;
//! code samples, image alt text and arbitrary HTML cannot authorize CSS.
use crate::publication_inline_text::{self as model, ResolvedTextStyle, SizeOrigin, INVALID};
use crate::publication_text_runs::{self, SizeKind};
use pulldown_cmark::{Event, Tag, TagEnd};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct ViewRun {
    pub key: String,
    pub size: Option<f64>,
    pub base: Option<f64>,
    pub complex: bool,
    pub block_base: bool,
}
#[derive(Debug, Serialize)]
pub struct View { pub revision:String, pub nonce:String, pub runs:Vec<ViewRun>, #[serde(skip_serializing)] pub content:String }

#[derive(Clone,Copy)]
struct CaptionStyle { from:usize, to:usize, effective:ResolvedTextStyle }

pub(crate) fn requires_projection(source:&str)->bool{
    model::has_owned_syntax(source)||source.contains(crate::publication_blocks::PREFIX)
        &&crate::publication_blocks::analyze(source).is_ok_and(|document|document.nodes.iter().any(|node|
            node.kind=="caption"&&crate::publication_blocks::author_style(&node.style).is_ok_and(|author|!author.is_empty())))
}

// Numeric character references protect literal Markdown punctuation after
// decoding an entity/backslash. A second Markdown parser cannot reinterpret it.
fn text(value:&str)->String {
    let mut out=String::new();
    for c in value.chars() { match c {
        '&'=>out.push_str("&amp;"), '<'=>out.push_str("&lt;"), '>'=>out.push_str("&gt;"), '"'=>out.push_str("&quot;"),
        c if c.is_ascii()&&"\\`*_{}[]()!#|$~".contains(c)=>out.push_str(&format!("&#{};",c as u32)), _=>out.push(c),
    }}out
}

// PDC excludes the introducing backslash from some Text event offsets.
// Consume only that unclaimed syntax byte, never a slash owned by another
// leaf (for example the preceding half of an escaped-backslash pair).
fn text_edit_range(source:&str,range:std::ops::Range<usize>,value:&str,owner_start:usize,occupied:&[std::ops::Range<usize>])->std::ops::Range<usize>{
    if range.start<=owner_start{return range;}
    let at=range.start-1;
    let first=source[range.clone()].chars().next();
    if source.as_bytes()[at]!=b'\\'||!first.is_some_and(|c|c.is_ascii_punctuation())||value.chars().next()!=first{return range;}
    let owner=occupied.partition_point(|entry|entry.end<=at);
    if occupied.get(owner).is_some_and(|entry|entry.contains(&at)){return range;}
    at..range.end
}

pub fn project(source:&str,options:&Value)->Result<View,String>{
    if source.contains(":label[")&&source.contains(crate::publication_blocks::PREFIX)
        &&crate::publication_blocks::analyze(source)?.nodes.iter().any(|node|node.kind=="caption"
            &&crate::publication_blocks::author_style(&node.style).is_ok_and(|author|!author.is_empty())){
        let prepared=crate::cross_references::project(source);
        if prepared.content!=source{
            let mut view=project_current(&prepared.content,options)?;
            view.revision=crate::document_transaction::revision(source);return Ok(view);
        }
    }
    project_current(source,options)
}
fn project_current(source:&str,options:&Value)->Result<View,String>{
    let revision=crate::document_transaction::revision(source);
    let mut view=View{revision,nonce:uuid::Uuid::new_v4().simple().to_string(),runs:Vec::new(),content:source.into()};
    if !model::has_owned_syntax(source)&&!source.contains(crate::publication_blocks::PREFIX){return Ok(view);}
    let blocks=crate::publication_blocks::analyze(source)?;
    let mut captions=Vec::new();
    for node in blocks.nodes.iter().filter(|node|node.kind=="caption"){
        let author=crate::publication_blocks::author_style(&node.style)?;
        if author.is_empty(){continue;}
        // Legacy local text.size establishes the structural caption pair.
        // Author axes then override it independently. That effective pair is
        // also the reset target for every title-local `base` declaration.
        let size=node.style.pointer("/text/size").and_then(Value::as_f64).unwrap_or(10.);
        let base=ResolvedTextStyle::base(size,size);
        captions.push(CaptionStyle{from:node.bytes.start,to:node.bytes.end,effective:author.resolve(&base,&base)});
    }
    if !captions.is_empty(){crate::publication_blocks::validate_author_consumer(source,true)?;}
    let document=model::consumer_document(source)?;
    if captions.is_empty()&&!document.nodes.iter().any(|n|n.kind=="span"){return Ok(view);}
    let caption_at=|from,to|{
        let at=captions.partition_point(|caption|caption.from<=from);
        at.checked_sub(1).and_then(|index|captions.get(index)).filter(|caption|to<=caption.to).copied()
    };
    let base=ResolvedTextStyle::base(12.,12.);
    let mut styles=HashMap::<String,ResolvedTextStyle>::new();
    let mut active=HashMap::<String,bool>::new();
    let owned_starts=document.nodes.iter().filter(|node|node.kind=="span").map(|node|(node.id.as_str(),node.content_bytes.start)).collect::<HashMap<_,_>>();
    let occupied=document.nodes.iter().filter(|node|node.kind!="span").map(|node|node.bytes.clone()).collect::<Vec<_>>();
    let mut leaves=HashMap::new();let mut edits=Vec::new();
    for node in &document.nodes {
        if crate::speech::cancelled(){return Err("cancelled".into());}
        let caption=caption_at(node.bytes.start,node.bytes.end);
        let inherited=caption.map(|caption|caption.effective).unwrap_or(base);
        let parent=node.parent.as_ref().and_then(|id|styles.get(id)).copied().unwrap_or(inherited);
        let parent_active=node.parent.as_ref().and_then(|id|active.get(id)).copied().unwrap_or(caption.is_some());
        if node.kind=="span" {
            styles.insert(node.id.clone(),node.style.resolve(&parent,&inherited));
            active.insert(node.id.clone(),parent_active||!node.style.is_empty());
            // Keep only the canonical human-readable author attributes. They
            // carry no CSS authority; nonce-bound leaf rules remain separate.
            // Keeping the owned opener also lets later Markdown consumers use
            // the shared first-run/HTML-block parser without reinterpreting it.
            if let Some(open)=&node.opening {edits.push((open.clone(),node.style.open_tag()));}
        }else if parent_active {leaves.insert((node.bytes.start,node.bytes.end),(parent,node.kind=="atom",node.parent.as_ref().and_then(|id|owned_starts.get(id.as_str())).copied().unwrap_or(caption.map(|caption|caption.from).unwrap_or(node.bytes.start)),caption.map(|caption|caption.effective)));}
    }
    let profile=crate::publication_profile::resolve(source,options).map_err(|e|e.code)?;
    let policies=crate::publication_note_policies::parse_layer(profile.values()).map_err(|e|e.code)?;
    let mut style_blocks=blocks.nodes.iter().filter_map(|node|node.style.pointer("/text/size").and_then(Value::as_f64).map(|pt|(node.bytes.clone(),pt))).collect::<Vec<_>>();
    style_blocks.sort_by_key(|(range,_)|(range.start,std::cmp::Reverse(range.end)));
    let mut style_at=0usize;let mut active_blocks=Vec::new();
    let mut note:Option<String>=None;let mut blocked=0usize;let mut image=0usize;
    for (event,range) in model::markdown_events(source)? {
        match event {
            Event::Start(Tag::FootnoteDefinition(label))=>note=Some(label.into_string()),
            Event::End(TagEnd::FootnoteDefinition)=>note=None,
            Event::Start(Tag::CodeBlock(_)|Tag::MetadataBlock(_))=>blocked+=1,
            Event::End(TagEnd::CodeBlock|TagEnd::MetadataBlock(_))=>blocked=blocked.saturating_sub(1),
            Event::Start(Tag::Image{..})=>image+=1,
            Event::End(TagEnd::Image)=>image=image.saturating_sub(1),
            event @ (Event::Text(_)|Event::Code(_)|Event::InlineMath(_)|Event::FootnoteReference(_)) if blocked==0&&image==0=>{
                let(value,code)=match event{Event::Code(value)=>(value,true),Event::Text(value)|Event::InlineMath(value)|Event::FootnoteReference(value)=>(value,false),_=>unreachable!()};
                let Some((style,atom,owner_start,caption_base))=leaves.get(&(range.start,range.end))else{continue};
                // Word's rtl run property selectively reclassifies weak and
                // neutral characters. CSS embedding/override is not equivalent.
                if style.rtl==Some(true){return Err(model::UNSUPPORTED.into());}
                let math=*atom&&!code&&source[range.clone()].starts_with('$');
                if math&&caption_base.is_some(){return Err(model::UNSUPPORTED.into());}
                let reference=*atom&&source[range.clone()].starts_with("[^");
                let inherited=note.as_ref().map(|label|policies.paragraph_policy(policies.kind_for_label(label)));
                while style_at<style_blocks.len()&&style_blocks[style_at].0.start<=range.start{active_blocks.push(style_at);style_at+=1;}
                active_blocks.retain(|index|style_blocks[*index].0.end>=range.end);
                let local=active_blocks.iter().filter_map(|index|{let(bounds,pt)=&style_blocks[*index];(bounds.start<=range.start&&range.end<=bounds.end).then_some((bounds.end-bounds.start,*pt))}).min_by_key(|(width,_)|*width).map(|(_,pt)|pt);
                // Itemize script/origin before checking the consumed numeric
                // axis. A legal native value on an unused axis is not a
                // rendering request. Routing values never reach CSS/model.
                let routing=ResolvedTextStyle{size:1.,size_cs:1.,..*style};
                let pieces=publication_text_runs::split(if math||reference{"x"}else{&value},&routing)?;
                let mut replacement=String::new();
                for segment in pieces {
                    if view.runs.len()>=model::MAX_NODES{return Err(model::BUDGET.into());}
                    let key=format!("{}-{}",view.nonce,view.runs.len());
                    let complex=segment.kind==SizeKind::Complex;
                    let axis_size=if complex{style.size_cs}else{style.size};
                    let size=(segment.origin==SizeOrigin::Author).then_some(axis_size);
                    let base_pt=if size.is_some(){None}else if let Some(caption)=caption_base{Some(if complex{caption.size_cs}else{caption.size})}else{local.or_else(||inherited.as_ref().and_then(|policy|if complex{policy.size_cs.or(policy.size)}else{policy.size}))};
                    if size.or(base_pt).is_some_and(|points|!points.is_finite()||points<=0.||points>publication_text_runs::MAX_RENDER_PT){return Err(model::UNSUPPORTED.into());}
                    let inline_css=size.or(base_pt).map(|pt|format!(" style=\"font-size:{pt}pt\"")).unwrap_or_default();
                    replacement.push_str(&format!("<span data-readmd-author-run=\"{key}\"{inline_css}>"));
                    if math||reference {replacement.push_str(&source[range.clone()]);}
                    else if code {replacement.push_str("<code>");replacement.push_str(&text(&value[segment.from..segment.to]));replacement.push_str("</code>");}
                    else {replacement.push_str(&text(&value[segment.from..segment.to]));}
                    replacement.push_str("</span>");
                    view.runs.push(ViewRun{key,size,base:base_pt,complex,block_base:segment.origin==SizeOrigin::BlockBase});
                }
                let edit_range=if *atom{range}else{text_edit_range(source,range,&value,*owner_start,&occupied)};
                edits.push((edit_range,replacement));
            },_=>(),
        }
    }
    edits.sort_by_key(|(range,_)|(range.start,range.end));
    let mut cursor=0;let mut content=String::new();
    for(range,replacement)in edits {
        if range.start<cursor{return Err(INVALID.into());}
        content.push_str(source.get(cursor..range.start).ok_or(INVALID)?);content.push_str(&replacement);cursor=range.end;
        if content.len()>crate::publication_rich::MAX_RICH_BYTES{return Err(model::BUDGET.into());}
    }
    content.push_str(&source[cursor..]);
    if content.len()>crate::publication_rich::MAX_RICH_BYTES{return Err(model::BUDGET.into());}
    view.content=content;Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math_document::XmlNode;
    use serde_json::json;

    fn typed_html(source:&str)->XmlNode{
        let html=crate::html_footnotes::render(source).unwrap().expect("fixture must select the actual typed note HTML renderer");
        crate::math_document::parse_xml(&html).unwrap()
    }
    fn descendants<'a>(node:&'a XmlNode,out:&mut Vec<&'a XmlNode>){out.push(node);for child in node.children(){descendants(child,out);}}
    fn actual_view_html(source:&str)->(View,XmlNode){
        let before=serde_json::to_value(model::get(source).unwrap()).unwrap();
        let baseline=typed_html(source);let view=project(source,&json!({})).unwrap();let actual=typed_html(&view.content);
        assert_eq!(actual.text(),baseline.text(),"projection must preserve the actual typed-renderer visible text");
        assert!(!actual.text().contains(&view.nonce),"a trusted internal tag became visible text");
        assert_eq!(serde_json::to_value(model::get(source).unwrap()).unwrap(),before,"projection must not change model ranges or source revision");
        let mut nodes=Vec::new();descendants(&actual,&mut nodes);
        for run in &view.runs{
            let rendered=nodes.iter().filter(|node|node.attributes.get("data-readmd-author-run")==Some(&run.key)).collect::<Vec<_>>();
            assert_eq!(rendered.len(),1,"trusted run must be a real, unique HTML element: {}",run.key);
            let points=run.size.or(run.base).expect("fixture author runs have a numeric point size");
            assert_eq!(rendered[0].attributes.get("style"),Some(&format!("font-size:{points}pt")),"actual run CSS must use the resolved numeric points");
        }
        (view,actual)
    }
    fn note_base_source(size:f64,size_cs:f64,attributes:&str)->String{
        let profile=json!({"version":1,"footnotes":{"kinds":{"footnote":{"size":size,"sizeCs":size_cs}}}});
        format!("---\nreadmd:\n  publication: {profile}\n---\n\nBody[^n].\n\n[^n]: <span data-readmd-text=\"v1\" {attributes}>NOTE_BASE_LIMIT</span>\n")
    }
    #[test]
    fn trusted_ranges_preserve_nesting_entities_links_and_literal_markdown(){
        let source="A <span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"10\">**BIG** [e\\*x](https://example.test) &amp; 中文 العربية <span data-readmd-text=\"v1\" data-size-pt=\"base\">base</span></span> Z";
        let view=project(source,&json!({})).unwrap();
        assert!(view.content.contains("**<span")&&view.content.contains("https://example.test"));
        assert!(view.content.contains("&#42;")&&view.content.contains("&amp;"));
        assert!(view.runs.iter().any(|r|r.complex&&r.size==Some(10.)));
        assert!(view.runs.iter().any(|r|!r.complex&&r.size==Some(30.)));
        assert!(view.runs.iter().any(|r|r.block_base&&r.size.is_none()));
        assert!(view.content.contains("data-readmd-text=\"v1\""));
        assert!(!view.content.contains("data-size-pt=\"30;"));
        assert_eq!(view.revision,crate::document_transaction::revision(source));
    }
    #[test]
    fn examples_and_alt_text_never_become_css_authority(){
        let source="`<span data-readmd-text=\"v1\" data-size-pt=\"30\">x</span>`\n\n```html\n<span data-readmd-text=\"v1\" data-size-pt=\"30\">x</span>\n```\n";
        let view=project(source,&json!({})).unwrap();assert!(view.runs.is_empty());assert_eq!(view.content,source);
        let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\">![KEEP_ALT](image.png) text</span>";
        let view=project(source,&json!({})).unwrap();assert!(view.content.contains("![KEEP_ALT](image.png)"));
    }
    #[test]
    fn invalid_active_markup_is_rejected_and_code_remains_literal(){
        assert_eq!(project("A <span data-readmd-text=\"v1\" data-size-pt=\"30;position:fixed\">x</span>",&json!({})).unwrap_err(),INVALID);
        let source="A <span data-readmd-text=\"v1\" data-size-pt=\"30\">`a*b`</span>";
        let view=project(source,&json!({})).unwrap();assert!(view.content.contains("<code>a&#42;b</code>"));
    }
    #[test]
    fn author_text_never_escapes_live_formulas_or_claims_their_size(){
        for prefix in ["","A "]{for formula in [":math[x^2]","$x^2$","$$x^2$$"]{
            let source=format!("{prefix}<span data-readmd-text=\"v1\" data-size-pt=\"30\">{formula}</span>");
            let original=source.clone();let revision=crate::document_transaction::revision(&source);
            assert_eq!(project(&source,&json!({})).unwrap_err(),model::UNSUPPORTED,"{source}");
            assert_eq!(source,original);assert_eq!(crate::document_transaction::revision(&source),revision);
            assert!(model::get(&source).is_ok());
        }}
        let source="A <span data-readmd-text=\"v1\" data-size-pt=\"30\">`:math[x^2]`</span>";
        let view=project(source,&json!({})).unwrap();assert!(view.content.contains("<code>"));
    }
    #[test]
    fn unstyled_math_outside_author_scopes_and_literal_examples_remain_available(){
        let source="<span data-readmd-text=\"v1\">$x^2$</span>";
        let view=project(source,&json!({})).unwrap();assert_eq!(view.content,source);assert!(view.runs.is_empty());
        let source="$x^2$ :math[y^2] <span data-readmd-text=\"v1\" data-size-pt=\"30\">text</span>";
        let view=project(source,&json!({})).unwrap();assert!(view.content.starts_with("$x^2$ :math[y^2] "));assert!(view.runs.iter().any(|r|r.size==Some(30.)));
        let source="A <span data-readmd-text=\"v1\" data-size-pt=\"30\">&#58;math[x^2] \\:math[x^2] `$x$` ![literal $x$](owned.png)</span>";
        let view=project(source,&json!({})).unwrap();assert!(view.content.contains("![literal $x$](owned.png)"));assert!(view.content.contains("<code>"));
    }

    #[test]
    fn escaped_literal_text_keeps_the_actual_note_html_meaning_and_author_points(){
        let source="A <span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"10\">**BODY_BOLD** *BODY_ITALIC* `BODY_CODE` [BODY_LINK](https://example.test/inline) &amp; &lt;literal&gt; \\* literal</span>\n\nTrigger[^n].\n\n[^n]: note\n";
        let(view,actual)=actual_view_html(source);let mut nodes=Vec::new();descendants(&actual,&mut nodes);
        let body=nodes.iter().find(|node|node.local()=="p"&&node.text().contains("BODY_BOLD")).unwrap();
        assert_eq!(body.text(),"A BODY_BOLD BODY_ITALIC BODY_CODE BODY_LINK & <literal> * literal");
        let literal=nodes.iter().find(|node|node.attributes.contains_key("data-readmd-author-run")&&node.text()=="* literal").unwrap();
        assert_eq!(literal.attributes.get("style").map(String::as_str),Some("font-size:30pt"));
        assert!(view.runs.iter().any(|run|Some(&run.key)==literal.attributes.get("data-readmd-author-run")&&run.size==Some(30.)));
        assert!(!view.content.contains(&format!("\\<span data-readmd-author-run=\"{}",view.nonce)));
    }

    #[test]
    fn escape_ownership_preserves_pairs_entities_and_non_ascii_non_escapes(){
        for raw in [r"\* literal",r"\\* literal",r"\\\* literal",r"\&amp; literal",r"\\&amp; literal",r"\\\&amp; literal",r"\\",r"\<literal>",r"\α literal"]{
            for prefix in ["","A "]{
                let source=format!("{prefix}<span data-readmd-text=\"v1\" data-size-pt=\"30\">{raw}</span>\r\n\r\nTrigger[^n].\r\n\r\n[^n]: note\r\n");
                let(view,_)=actual_view_html(&source);assert!(!view.runs.is_empty(),"{raw}");
            }
        }
        let raw=format!("{}&amp; tail","\\".repeat(1024));
        let source=format!("<span data-readmd-text=\"v1\" data-size-pt=\"30\">{raw}</span>\n\nTrigger[^n].\n\n[^n]: note\n");
        actual_view_html(&source);
    }

    #[test]
    fn escaped_code_atoms_and_image_alt_keep_their_opaque_meaning(){
        let source="A <span data-readmd-text=\"v1\" data-size-pt=\"30\">`\\* CODE \\&amp;` [^n] ![ALT \\* literal $x$](owned.png) tail</span>\n\n[^n]: note\n";
        let(view,actual)=actual_view_html(source);assert!(view.content.contains("![ALT \\* literal $x$](owned.png)"));
        let mut nodes=Vec::new();descendants(&actual,&mut nodes);
        let code=nodes.iter().find(|node|node.local()=="code").unwrap();assert_eq!(code.text(),"\\* CODE \\&amp;");
        let image=nodes.iter().find(|node|node.local()=="img").unwrap();assert!(!image.attributes.contains_key("data-readmd-author-run"));
        let baseline=typed_html(source);let mut originals=Vec::new();descendants(&baseline,&mut originals);
        let original_image=originals.iter().find(|node|node.local()=="img").unwrap();assert_eq!(image.attributes,original_image.attributes);
        for source in ["```html\n<span data-readmd-text=\"v1\" data-size-pt=\"30\">\\* CODE</span>\n```\n\nTrigger[^n].\n\n[^n]: note\n","<pre><span data-readmd-text=\"v1\" data-size-pt=\"30\">\\* HTML</span></pre>\n\nTrigger[^n].\n\n[^n]: note\n"]{
            let(view,_)=actual_view_html(source);assert!(view.runs.is_empty());assert_eq!(view.content,source);
        }
    }

    #[test]
    fn literal_backticks_remain_ordinary_text_and_real_inline_code_keeps_its_tag(){
        for raw in [r"\`LITERAL_TICK\` `REAL_CODE`",r"`LITERAL_OPEN"]{
            let source=format!("A <span data-readmd-text=\"v1\" data-size-pt=\"30\">{raw}</span>\n\nTrigger[^n].\n\n[^n]: note\n");
            let(view,actual)=actual_view_html(&source);let mut nodes=Vec::new();descendants(&actual,&mut nodes);
            let code=nodes.iter().filter(|node|node.local()=="code").map(|node|node.text()).collect::<Vec<_>>();
            assert_eq!(code,if raw.contains("REAL_CODE"){vec!["REAL_CODE".to_string()]}else{Vec::new()},"literal backticks must not create code semantics");
            let body=nodes.iter().find(|node|node.local()=="p"&&node.text().contains("LITERAL_")).unwrap();
            assert_eq!(body.text(),if raw.contains("REAL_CODE"){"A `LITERAL_TICK` REAL_CODE"}else{"A `LITERAL_OPEN"});
            let literals=nodes.iter().filter(|node|node.attributes.contains_key("data-readmd-author-run")&&node.text().contains('`')).collect::<Vec<_>>();
            assert!(!literals.is_empty());
            for literal in literals{
                assert_eq!(literal.attributes.get("style").map(String::as_str),Some("font-size:30pt"));
                let mut children=Vec::new();descendants(literal,&mut children);assert!(!children.iter().any(|child|child.local()=="code"));
                assert!(view.runs.iter().any(|run|Some(&run.key)==literal.attributes.get("data-readmd-author-run")&&run.size==Some(30.)));
            }
        }
    }

    #[test]
    fn consumed_note_base_points_obey_the_shared_limit_without_narrowing_native_source(){
        for points in [publication_text_runs::MAX_RENDER_PT,publication_text_runs::MAX_RENDER_PT+0.5]{
            for complex in [false,true]{
                let(size,size_cs)=if complex{(9.,points)}else{(points,9.)};
                let attributes=format!("data-size-pt=\"base\" data-size-cs-pt=\"base\" data-cs=\"{complex}\"");
                let source=note_base_source(size,size_cs,&attributes);let before=serde_json::to_value(model::get(&source).unwrap()).unwrap();
                let native=crate::publication_profile::from_source(&source).unwrap();
                assert_eq!(native.options.pointer("/footnotes/kinds/footnote/size").and_then(Value::as_f64),Some(size));
                assert_eq!(native.options.pointer("/footnotes/kinds/footnote/sizeCs").and_then(Value::as_f64),Some(size_cs));
                if points<=publication_text_runs::MAX_RENDER_PT{
                    let(view,_)=actual_view_html(&source);assert_eq!(view.runs.len(),1);
                    let run=&view.runs[0];assert_eq!(run.size,None);assert_eq!(run.base,Some(points));assert_eq!(run.complex,complex);assert!(run.block_base);
                }else{assert_eq!(project(&source,&json!({})).unwrap_err(),model::UNSUPPORTED);}
                assert_eq!(serde_json::to_value(model::get(&source).unwrap()).unwrap(),before,"rejecting a projection must not rewrite or clamp the source domain");
            }
        }
        for points in [publication_text_runs::MAX_RENDER_PT,publication_text_runs::MAX_RENDER_PT+0.5]{for complex in [false,true]{
            let attributes=if complex{format!("data-size-cs-pt=\"{points}\" data-cs=\"true\"")}else{format!("data-size-pt=\"{points}\"")};
            let source=note_base_source(9.,9.,&attributes);assert!(model::get(&source).is_ok());
            if points<=publication_text_runs::MAX_RENDER_PT{
                let(view,_)=actual_view_html(&source);assert_eq!(view.runs.len(),1);assert_eq!(view.runs[0].size,Some(points));assert_eq!(view.runs[0].base,None);assert_eq!(view.runs[0].complex,complex);
            }else{assert_eq!(project(&source,&json!({})).unwrap_err(),model::UNSUPPORTED);}
        }}
    }

    #[test]
    fn local_and_author_points_do_not_consume_unused_oversized_note_or_style_axes(){
        for (size,size_cs,attributes,expected_size,expected_base) in [
            (8192.,8192.,"data-size-pt=\"30\" data-size-cs-pt=\"30\"",Some(30.),None),
            (9.,8192.,"data-size-pt=\"base\" data-size-cs-pt=\"base\"",None,Some(9.)),
            (8192.,9.,"data-size-pt=\"base\" data-size-cs-pt=\"base\" data-cs=\"true\"",None,Some(9.)),
            (9.,9.,"data-size-pt=\"30\" data-size-cs-pt=\"8192\"",Some(30.),None),
            (9.,9.,"data-size-pt=\"8192\" data-size-cs-pt=\"10\" data-cs=\"true\"",Some(10.),None),
            (9.,9.,"data-size-cs-pt=\"8192\"",None,Some(9.)),
        ]{
            let source=note_base_source(size,size_cs,attributes);let(view,_)=actual_view_html(&source);assert_eq!(view.runs.len(),1);
            assert_eq!(view.runs[0].size,expected_size);assert_eq!(view.runs[0].base,expected_base);
        }
        for complex in [false,true]{
            let source=note_base_source(8192.,8192.,&format!("data-size-pt=\"base\" data-size-cs-pt=\"base\" data-cs=\"{complex}\""));
            let marker_start=source.find("NOTE_BASE_LIMIT").unwrap();let marker_end=marker_start+"NOTE_BASE_LIMIT".len();
            let blocks=crate::publication_blocks::analyze(&source).unwrap();
            let notes=blocks.nodes.iter().filter(|node|node.nested&&node.editable&&node.kind=="paragraph"&&node.bytes.start<=marker_start&&marker_end<=node.bytes.end).collect::<Vec<_>>();
            assert_eq!(notes.len(),1,"the fixture marker must identify exactly one real editable note paragraph: {:#?}",blocks.nodes);
            let note=notes[0];assert!(note.inline_content.start<=marker_start&&marker_end<=note.inline_content.end);
            let edited=crate::publication_blocks::apply(&source,&blocks.revision,&note.id,&json!({"text":{"size":11.5}})).unwrap();
            let local=edited["result"]["content"].as_str().unwrap();
            let current=crate::publication_blocks::analyze(local).unwrap();let changed=current.nodes.iter().find(|node|node.id==note.id).expect("the local transaction must retain the selected note paragraph identity");
            assert!(changed.nested&&changed.editable&&changed.kind=="paragraph");assert_eq!(changed.style["text"]["size"],json!(11.5));
            let(view,_)=actual_view_html(local);assert_eq!(view.runs.len(),1);
            assert_eq!(view.runs[0].size,None);assert_eq!(view.runs[0].base,Some(11.5));assert!(view.runs[0].block_base);assert_eq!(view.runs[0].complex,complex);
        }
    }

    fn caption_source(author:Value,local:Option<f64>,title:&str)->String{
        let mut style=json!({"id":"caption-view","text":{"author":author}});if let Some(size)=local{style["text"]["size"]=json!(size);}
        format!("---\nlang: en-US\n---\n\nDUPLICATE outside.\n\n| A |\n|---|\n| Cell |\n\n{}\n\n:label[tbl:view] {title}\n\nDUPLICATE following.[^n]\n\n[^n]: Plain note.\n",crate::publication_blocks::comment(&style).unwrap())
    }
    fn caption_paragraph<'a>(actual:&'a XmlNode,id:&str)->&'a XmlNode{
        let mut nodes=Vec::new();descendants(actual,&mut nodes);nodes.into_iter().find(|node|node.local()=="p"&&node.children().any(|child|child.attributes.get("id").map(String::as_str)==Some(id))).expect("actual caption anchor paragraph")
    }
    fn assert_run_points(view:&View,actual:&XmlNode,needle:&str,points:f64){
        let mut nodes=Vec::new();descendants(actual,&mut nodes);let matching=nodes.into_iter().filter(|node|node.attributes.contains_key("data-readmd-author-run")&&node.text().contains(needle)).collect::<Vec<_>>();assert!(!matching.is_empty(),"missing actual author run {needle}");
        for node in matching{assert_eq!(node.attributes.get("style"),Some(&format!("font-size:{points}pt")),"{needle}");let key=&node.attributes["data-readmd-author-run"];assert!(view.runs.iter().any(|run|&run.key==key&&run.size.or(run.base)==Some(points)));}
    }
    #[test]
    fn caption_author_projection_styles_label_title_and_title_local_base_on_both_axes(){
        let source=caption_source(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}),None,"**DUPLICATE** العربية <span data-readmd-text=\"v1\" data-size-pt=\"45\">INLINE45 <span data-readmd-text=\"v1\" data-size-pt=\"base\" data-size-cs-pt=\"base\">RESET_WEST עברית</span></span> [VISIBLE_LINK](https://example.test/caption)");let original=source.clone();
        let prepared=crate::cross_references::project(&source).content;assert!(requires_projection(&prepared));let(view,actual)=actual_view_html(&prepared);let caption=caption_paragraph(&actual,"tbl:view");assert!(caption.text().starts_with("Table 1 — DUPLICATE العربية "));
        for marker in ["Table 1","DUPLICATE","VISIBLE_LINK","RESET_WEST"]{assert_run_points(&view,caption,marker,30.);}assert_run_points(&view,caption,"INLINE45",45.);assert_run_points(&view,caption,"العربية",10.);assert_run_points(&view,caption,"עברית",10.);
        let mut nodes=Vec::new();descendants(caption,&mut nodes);assert!(nodes.iter().any(|node|node.local()=="a"&&node.attributes.get("href").map(String::as_str)==Some("https://example.test/caption")&&node.text()=="VISIBLE_LINK"));
        let mut nodes=Vec::new();descendants(&actual,&mut nodes);for paragraph in nodes.iter().filter(|node|node.local()=="p"&&node.text().starts_with("DUPLICATE ")){let mut children=Vec::new();descendants(paragraph,&mut children);assert!(!children.iter().any(|node|node.attributes.contains_key("data-readmd-author-run")));}
        let reparsed=crate::md_ast::parse(&view.content);assert!(reparsed.blocks.iter().any(|block|matches!(block.unstyled(),crate::md_ast::Block::Caption{label,..}if label.id=="tbl:view")));assert_eq!(source,original);
    }
    #[test]
    fn caption_only_projection_keeps_legacy_pair_and_does_not_activate_empty_author_or_examples(){
        let source=caption_source(json!({"size":30}),Some(17.5),"DUPLICATE العربية");assert!(!model::has_owned_syntax(&source));let direct=project(&source,&json!({})).unwrap();assert!(!direct.runs.is_empty());assert_eq!(direct.revision,crate::document_transaction::revision(&source));assert!(caption_paragraph(&typed_html(&direct.content),"tbl:view").text().starts_with("Table 1 — "));let prepared=crate::cross_references::project(&source).content;let(view,actual)=actual_view_html(&prepared);let caption=caption_paragraph(&actual,"tbl:view");assert_run_points(&view,caption,"Table 1",30.);assert_run_points(&view,caption,"DUPLICATE",30.);assert_run_points(&view,caption,"العربية",17.5);
        let empty=crate::cross_references::project(&caption_source(json!({}),None,"DUPLICATE")).content;assert!(!requires_projection(&empty));let view=project(&empty,&json!({})).unwrap();assert!(view.runs.is_empty());assert_eq!(view.content,empty);
        for example in [format!("```md\n{source}\n```\n"),format!("<pre>\n{source}\n</pre>\n")]{assert!(!requires_projection(&example));let view=project(&example,&json!({})).unwrap();assert!(view.runs.is_empty());assert_eq!(view.content,example);}
    }
    #[test]
    fn mixed_script_caption_prefix_stays_typed_after_script_runs_and_layout_projection(){
        let source=caption_source(json!({"size":30,"sizeCs":10}),None,"DUPLICATE title").replacen("---\nlang: en-US\n---\n","---\nlang: en-US\n---\n\n<!-- readmd-numbering:v1 {\"captions\":{\"table\":{\"prefix\":\"Table العربية\",\"format\":\"upperLetter\",\"start\":27}}} -->\n",1);
        let projected=crate::cross_references::project(&source);assert!(projected.diagnostics.is_empty(),"{:?}",projected.diagnostics);let(view,actual)=actual_view_html(&projected.content);let caption=caption_paragraph(&actual,"tbl:view");assert!(caption.text().starts_with("Table العربية AA — DUPLICATE"));assert_run_points(&view,caption,"Table",30.);assert_run_points(&view,caption,"العربية",10.);assert_run_points(&view,caption,"AA",30.);
        let doc=crate::md_ast::parse(&view.content);let typed=doc.blocks.iter().find(|block|matches!(block.unstyled(),crate::md_ast::Block::Caption{..})).expect("mixed label runs must retain their typed caption");let crate::md_ast::Block::Caption{label,inlines}=typed.unstyled()else{unreachable!()};assert_eq!(label.id,"tbl:view");assert_eq!(label.text(),"Table العربية AA");assert_eq!(crate::md_ast::inline_plain(inlines)," — DUPLICATE title");
        let (display,layout)=crate::publication_blocks::view(&view.content).unwrap();assert_eq!(layout.as_object().unwrap().len(),1);let actual=typed_html(&display);let caption=caption_paragraph(&actual,"tbl:view");assert!(caption.text().starts_with("Table العربية AA — DUPLICATE"));
    }
    #[test]
    fn caption_render_checks_real_axes_direction_and_math_without_clamping_source(){
        let unused=caption_source(json!({"size":30,"sizeCs":8192}),None,"WESTERN_ONLY");let prepared=crate::cross_references::project(&unused).content;let view=project(&prepared,&json!({})).unwrap();assert!(!view.runs.is_empty());assert!(view.runs.iter().all(|run|run.size.or(run.base)==Some(30.)));
        for(author,title)in[(json!({"size":8192}),"WESTERN_ONLY"),(json!({"size":30,"sizeCs":8192}),"العربية"),(json!({"size":30,"rtl":true}),"123"),(json!({"size":30}),"$x$")]{let source=caption_source(author,None,title);let before=source.clone();let prepared=crate::cross_references::project(&source).content;assert_eq!(project(&prepared,&json!({})).unwrap_err(),model::UNSUPPORTED);assert_eq!(source,before);}
    }
}
