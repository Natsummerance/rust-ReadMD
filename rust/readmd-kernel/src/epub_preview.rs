//! Reflow preview of the actual EPUB spine and stylesheet, never A4 HTML.
//! No extraction folder: package resources remain in memory until the response
//! is delivered. Chapter IDs are scoped only in this combined viewing document.
use base64::Engine;
use quick_xml::{events::{BytesStart,Event},Reader,Writer,XmlVersion};
use serde_json::{json,Value};
use std::collections::HashMap;

fn attrs(e:&BytesStart<'_>)->Result<HashMap<String,String>,String>{
    e.attributes().map(|a|{
        let a=a.map_err(|_|"preview_generation_failed")?;
        Ok((a.key.as_ref().to_string(),a.normalized_value(XmlVersion::Implicit1_0).map_err(|_|"preview_generation_failed")?.into_owned()))
    }).collect()
}
fn scope(file:&str)->String {format!("epub-{}-",file.trim_end_matches(".xhtml").replace('.',"-"))}
// The combined reflow document cannot copy cover html/body rules into every
// chapter. Its cover instead has one scoped, ratio-sized block: no body font
// metrics, inline SVG baseline or viewport-height overflow enters that block.
const COVER_CSS:&str="article.epub-preview-cover{margin:0;padding:0;line-height:0;font-size:0}article.epub-preview-cover>section{display:block;margin:0;padding:0;width:100%;height:auto}article.epub-preview-cover>section>svg{display:block;margin:0;padding:0;width:100%;height:auto}";
fn body(source:&str,file:&str,resources:&HashMap<String,String>,spine:&[String])->Result<String,String>{
    let source=source.split_once("<body>").and_then(|(_,s)|s.rsplit_once("</body>").map(|(body,_)|body)).ok_or("preview_generation_failed")?;
    let mut reader=Reader::from_str(source);let mut writer=Writer::new(Vec::new());
    loop{
        let event=reader.read_event().map_err(|_|"preview_generation_failed")?;
        match event {
            Event::Eof=>break,
            event @ (Event::Start(_)|Event::Empty(_))=>{
                let empty=matches!(&event,Event::Empty(_));let e=match event{Event::Start(e)|Event::Empty(e)=>e,_=>unreachable!()};
                let name=e.name().as_ref().to_string();let image=e.local_name().as_ref()=="image";let mut node=BytesStart::new(name);
                for (key,mut value) in attrs(&e)? {
                    if key=="id"{value=format!("{}{value}",scope(file));}
                    if matches!(key.as_str(),"aria-labelledby"|"aria-describedby"){
                        value=value.split_ascii_whitespace().map(|id|format!("{}{id}",scope(file))).collect::<Vec<_>>().join(" ");
                    }
                    if key=="src" || (image&&matches!(key.as_str(),"href"|"xlink:href")){
                        // srcdoc inherits the desktop URL. Never leave an image
                        // path for the browser to resolve against that host:
                        // both HTML and SVG images use the in-memory manifest.
                        value=resources.get(&value).cloned().ok_or("preview_generation_failed")?;
                    }else if matches!(key.as_str(),"href"|"xlink:href") {
                        if value.starts_with('#'){value=format!("#{}{}",scope(file),&value[1..]);}
                        else {let (target,anchor)=value.split_once('#').unwrap_or((&value,""));if spine.iter().any(|p|p==target){value=format!("#{}{}",scope(target),anchor);}}
                    }
                    node.push_attribute((key.as_str(),value.as_str()));
                }
                writer.write_event(if empty{Event::Empty(node)}else{Event::Start(node)}).map_err(|_|"preview_generation_failed")?;
            },
            event=>writer.write_event(event).map_err(|_|"preview_generation_failed")?,
        }
    }
    String::from_utf8(writer.into_inner()).map_err(|_|"preview_generation_failed".into())
}
pub fn prepare(content:&str,base:&str,options:&Value,name:&str)->Result<Value,String>{
    crate::publication_typography::validate_preview(content, options).map_err(str::to_string)?;
    let (entries,warnings)=crate::mdexport::epub_build_entries(&json!(content),base,options,name,"ReadMD Author","zh-CN","preview","2026-01-01T00:00:00Z")?;
    if entries.iter().map(|e|e.data.len()).sum::<usize>()>32*1024*1024{return Err("preview_generation_failed".into());}
    let parts:HashMap<String,Vec<u8>>=entries.into_iter().map(|e|(e.path,e.data)).collect();
    let opf=std::str::from_utf8(&parts["OEBPS/content.opf"]).map_err(|_|"preview_generation_failed")?;
    let mut reader=Reader::from_str(opf);let mut manifest=HashMap::new();let mut spine=Vec::new();let mut resources=HashMap::new();
    loop{match reader.read_event().map_err(|_|"preview_generation_failed")?{
        Event::Empty(e) if e.local_name().as_ref()=="item"=>{
            let a=attrs(&e)?;if let (Some(id),Some(href))=(a.get("id"),a.get("href")){
                manifest.insert(id.clone(),href.clone());
                if let Some(mime)=a.get("media-type").filter(|m|m.starts_with("image/")){
                    let data=parts.get(&format!("OEBPS/{href}")).ok_or("preview_generation_failed")?;
                    resources.insert(href.clone(),format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(data)));
                }
            }
        },
        Event::Empty(e) if e.local_name().as_ref()=="itemref"=>{let a=attrs(&e)?;if let Some(id)=a.get("idref"){spine.push(manifest.get(id).ok_or("preview_generation_failed")?.clone());}},
        Event::Eof=>break,_=>(),
    }}
    let css=std::str::from_utf8(&parts["OEBPS/style.css"]).map_err(|_|"preview_generation_failed")?.replace('<',"\\3c ");
    let first=std::str::from_utf8(&parts[&format!("OEBPS/{}",spine.first().ok_or("preview_generation_failed")?)]).map_err(|_|"preview_generation_failed")?;
    let root=first.split_once("<head>").ok_or("preview_generation_failed")?.0;
    let cover=manifest.get("cover");
    let cover_css=if cover.is_some(){COVER_CSS}else{""};
    let mut html=format!("{root}<head><title>{}</title><style>{css}{cover_css}</style></head><body>",crate::epub_writer::esc(name));
    for file in &spine{
        if crate::speech::cancelled(){return Err("cancelled".into());}
        let source=std::str::from_utf8(&parts[&format!("OEBPS/{file}")]).map_err(|_|"preview_generation_failed")?;
        let cover_class=if cover.is_some_and(|cover|cover==file){" class=\"epub-preview-cover\""}else{""};
        html.push_str(&format!("<article id=\"{}\"{cover_class}>{}</article>",scope(file),body(source,file,&resources,&spine)?));
    }
    html.push_str("</body></html>");
    Ok(json!({"ok":true,"format":"epub","mode":"epub-reflow","html":html,"documents":spine,"warnings":warnings}))
}

#[cfg(test)]mod tests{
    use super::*;
    use crate::math_document::{parse_xml,XmlNode};
    fn find<'a>(node:&'a XmlNode,local:&str)->Option<&'a XmlNode>{
        if node.local()==local{Some(node)}else{node.children().find_map(|child|find(child,local))}
    }
    #[test]fn real_spine_styles_and_note_anchors_are_used_without_creating_files(){
        let dir=tempfile::tempdir().unwrap();let source="# One\n\nA[^n] twice[^n] and [two](#two).\n\n# Two\n\nB[^n].\n\n[^n]: Note\n";
        let data=prepare(source,dir.path().to_str().unwrap(),&json!({"cover":{"enabled":true},"epub":{"fontSize":17}}),"Book").unwrap();
        let html=data["html"].as_str().unwrap();assert!(html.contains("17px")||html.contains("17pt"));assert!(html.contains("data:image/svg+xml;base64,"));
        assert!(html.contains("href=\"#epub-chapter_2-h-two\""));assert!(html.contains("id=\"epub-chapter_1-fnref-1-2\""));assert!(html.contains("href=\"#epub-chapter_2-fnref-1\""));
        assert_eq!(fs_count(dir.path()),0);assert_eq!(data["documents"].as_array().unwrap().len(),4);
        let document=parse_xml(html).unwrap();
        let svg=find(&document,"svg").unwrap();
        assert_eq!(svg.attributes["viewBox"],"0 0 2400 3200");
        assert_eq!(svg.attributes["preserveAspectRatio"],"xMidYMid meet");
        let title=find(svg,"title").unwrap();
        assert_eq!(svg.attributes["aria-labelledby"],title.attributes["id"]);
        assert_eq!(title.attributes["id"],"epub-cover-cover-image-title");
        assert_eq!(title.text(),"Book");
        let resource=&find(svg,"image").unwrap().attributes["xlink:href"];
        let encoded=resource.strip_prefix("data:image/svg+xml;base64,").unwrap();
        let bytes=base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        crate::svg_render::validate(&bytes).unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("viewBox=\"0 0 2400 3200\""));
        assert!(html.contains("id=\"epub-cover-\" class=\"epub-preview-cover\""));
        assert!(html.contains(COVER_CSS));
    }

    #[test]
    fn cover_and_body_images_share_manifest_bytes_and_keep_source_files_unchanged(){
        use image::{DynamicImage,ImageFormat};
        let dir=tempfile::tempdir().unwrap();
        let image=dir.path().join("source.png");
        let mut encoded=std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(36,52).write_to(&mut encoded,ImageFormat::Png).unwrap();
        let bytes=encoded.into_inner();
        std::fs::write(&image,&bytes).unwrap();
        let title="Cover <literal> & \"quoted\" 😀";
        let source="# Body\n\n![Shared source](source.png)\n";
        let data=prepare(source,dir.path().to_str().unwrap(),&json!({"epub":{"cover":"source.png","lineHeight":4.5},"cover":{"title":title}}),"Book").unwrap();
        let html=data["html"].as_str().unwrap();
        let document=parse_xml(html).unwrap();
        let svg=find(&document,"svg").unwrap();
        assert_eq!(svg.attributes["viewBox"],"0 0 36 52");
        assert_eq!(find(svg,"title").unwrap().text(),title);
        let svg_image=find(svg,"image").unwrap();
        let html_image=find(&document,"img").unwrap();
        assert_eq!(svg_image.attributes["xlink:href"],html_image.attributes["src"]);
        let resource=svg_image.attributes["xlink:href"].strip_prefix("data:image/png;base64,").unwrap();
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(resource).unwrap(),bytes);
        assert_eq!(std::fs::read(image).unwrap(),bytes);
        assert_eq!(fs_count(dir.path()),1,"preview creates no extraction or output files");
        assert_eq!(html.matches("class=\"epub-preview-cover\"").count(),1);
        assert!(html.contains("line-height: 4.5;"),"body stylesheet remains in the same preview");
        assert!(html.contains("article.epub-preview-cover>section>svg{display:block;margin:0;padding:0;width:100%;height:auto}"));
        assert!(!html.contains("xlink:href=\"images/")&&!html.contains("src=\"images/"));
    }

    #[test]
    fn svg_href_variants_and_accessible_local_references_use_the_same_scoped_document(){
        let resources=HashMap::from([("images/local.png".into(),"data:image/png;base64,cGl4ZWxz".into())]);
        let source=r##"<html><body><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" aria-labelledby="title extra" aria-describedby="description"><title id="title">Title</title><desc id="description">Description</desc><g id="extra"/><g id="shape"/><use href="#shape"/><use xlink:href="#shape"/><image href="images/local.png"/><image xlink:href="images/local.png"/></svg><img src="images/local.png" alt="local"/></body></html>"##;
        for file in ["chapter_1.xhtml","chapter_2.xhtml"]{
            let output=body(source,file,&resources,&[]).unwrap();
            parse_xml(&output).unwrap();
            let prefix=scope(file);
            assert!(output.contains(&format!("aria-labelledby=\"{prefix}title {prefix}extra\"")));
            assert!(output.contains(&format!("aria-describedby=\"{prefix}description\"")));
            assert!(output.contains(&format!("id=\"{prefix}title\"")));
            assert!(output.contains(&format!("id=\"{prefix}description\"")));
            assert!(output.contains(&format!("href=\"#{prefix}shape\"")));
            assert!(output.contains(&format!("xlink:href=\"#{prefix}shape\"")));
            assert_eq!(output.matches("data:image/png;base64,cGl4ZWxz").count(),3);
            assert!(!output.contains("=\"images/local.png\""));
        }
    }

    #[test]
    fn unresolved_image_resources_fail_closed_while_links_keep_their_navigation(){
        let resources=HashMap::new();
        for target in ["missing.png","https://example.test/private.png","file:///private.png","javascript:alert(1)","data:image/svg+xml;base64,PHN2Zz48c2NyaXB0Lz48L3N2Zz4="]{
            for image in [format!(r#"<img src="{target}"/>"#),format!(r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="{target}"/></svg>"#),format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="{target}"/></svg>"#)]{
                assert_eq!(body(&format!("<html><body>{image}</body></html>"),"chapter_1.xhtml",&resources,&[]).unwrap_err(),"preview_generation_failed");
            }
        }
        let source=r##"<html><body><a href="https://example.test/reference">External reference</a><a href="#local">Local</a><a href="chapter_2.xhtml#target">Next chapter</a><p id="local">Target</p></body></html>"##;
        let output=body(source,"chapter_1.xhtml",&resources,&["chapter_1.xhtml".into(),"chapter_2.xhtml".into()]).unwrap();
        assert!(output.contains("href=\"https://example.test/reference\""));
        assert!(output.contains("href=\"#epub-chapter_1-local\""));
        assert!(output.contains("href=\"#epub-chapter_2-target\""));
        assert!(output.contains("id=\"epub-chapter_1-local\""));
        parse_xml(&output).unwrap();
    }
    fn fs_count(path:&std::path::Path)->usize{std::fs::read_dir(path).unwrap().count()}
}
