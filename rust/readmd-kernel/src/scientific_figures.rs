//! Publication figures use the original data/specification, never a screenshot
//! of the reader. Defaults are applied beneath explicit author configuration.
use serde_json::{json,Value};
use pulldown_cmark::{CodeBlockKind,Event,Options,Parser,Tag,TagEnd};
use sha2::{Digest,Sha256};
use std::{collections::HashMap,time::{Duration,Instant}};

pub fn catalog()->Value { serde_json::from_str(include_str!("../../../assets/data/scientific-figures.json")).expect("scientific figure catalog") }
fn merge(target:&mut Value,other:&Value) {
    if let (Some(dst),Some(src))=(target.as_object_mut(),other.as_object()) {
        for (key,value) in src { if let Some(old)=dst.get_mut(key) { merge(old,value); } else { dst.insert(key.clone(),value.clone()); } }
    } else { *target=other.clone(); }
}
fn number(value:&Value,fallback:f64,min:f64,max:f64)->f64 { value.as_f64().filter(|n|n.is_finite()).unwrap_or(fallback).clamp(min,max) }
pub fn profile(spec:&Value,options:&Value)->Value {
    let mut profile=spec.get("readmd").filter(|v|v.is_object()).cloned().unwrap_or_else(||json!({}));
    // "Original" respects the author's embedded profile. An explicit output
    // publication profile controls print dimensions across the whole paper.
    if options["style"]=="research" {merge(&mut profile,options);}
    profile
}
pub fn prepare_spec(mut spec:Value,options:&Value)->Result<Value,String> {
    if !spec.is_object(){return Err("diagram_invalid_input".into());}
    let settings=profile(&spec,options);spec.as_object_mut().unwrap().remove("readmd");
    if settings["style"]!="research" {return Ok(spec);}
    let mut defaults=catalog()["config"].clone();
    let size=number(&settings["fontSizePt"],7.,5.,18.)*96./72.;
    for section in ["axis","legend"] { for key in ["labelFontSize","titleFontSize"] {defaults[section][key]=json!(size);} }
    defaults["text"]["fontSize"]=json!(size);defaults["title"]["fontSize"]=json!(size+1.);
    if let Some(author)=spec.get("config"){merge(&mut defaults,author);}
    spec["config"]=defaults;
    if spec.get("background").is_none(){spec["background"]=json!("white");}
    // Fit the complete figure (axes and legends included) to the print width.
    // Composite figures retain their own independent view sizes.
    if !["hconcat","vconcat","concat","facet","repeat"].iter().any(|k|spec.get(*k).is_some()) {
        if spec.get("width").is_none(){spec["width"]=json!(number(&settings["widthMm"],89.,45.,300.)*96./25.4);}
        if spec.get("height").is_none(){spec["height"]=json!(220);}
        if spec.get("autosize").is_none(){spec["autosize"]=json!({"type":"fit","contains":"padding","resize":false});}
    }
    Ok(spec)
}
pub fn print_width(bytes:&[u8])->Option<f64> {
    let mut reader=quick_xml::Reader::from_reader(bytes);
    loop{match reader.read_event().ok()?{
        quick_xml::events::Event::Start(e)|quick_xml::events::Event::Empty(e)=>{
            if e.local_name().as_ref()!="svg"{return None;}
            for a in e.attributes().flatten(){if a.key.as_ref()=="data-readmd-print-width-mm" {
                return a.value.parse::<f64>().ok().filter(|n|n.is_finite()&&*n>=45.&&*n<=300.);
            }}return None;
        },quick_xml::events::Event::Eof=>return None,_=>(),
    }}
}
pub fn size_svg(svg:String,settings:&Value)->String {
    if settings["style"]!="research" {return svg;}
    let width=number(&settings["widthMm"],89.,45.,300.);
    svg.replacen("<svg ",&format!("<svg data-readmd-print-width-mm=\"{width}\" "),1)
}

/// Source and profile identity prevent reusing an unrelated browser rendering
/// after the user edits code or switches output presets.
pub fn identity(engine:&str,code:&str,profile:&Value)->String {
    let mut h=Sha256::new();for bytes in [engine.as_bytes(),code.as_bytes(),profile.to_string().as_bytes()] {h.update((bytes.len()as u64).to_le_bytes());h.update(bytes);}
    format!("{:x}",h.finalize())
}
pub fn is_diagram(engine:&str)->bool { matches!(engine,"vega"|"vega-lite"|"mermaid"|"chart"|"chartjs"|"chart.js"|"graphviz"|"viz"|"dot"|"tikz"|"plantuml"|"puml"|"wavedrom"|"bitfield"|"wsd"|"d2"|"ditaa") }
pub fn render(engine:&str,code:&str,options:&Value)->Result<String,String> {
    let svg=match engine {
        "vega"|"vega-lite"=>crate::diagrams::render_vega_with_profile(code,engine,Duration::from_secs(12),options),
        "wsd"|"d2"|"ditaa"=>crate::native_diagrams::render(engine,code),
        "graphviz"|"viz"|"dot"=>crate::diagrams::render_graphviz_svg(code,options),
        "plantuml"|"puml"=>crate::diagrams::render_plantuml_svg(code,Duration::from_secs(15)),
        _=>return Err("diagram_client_renderer_required".into()),
    }.map_err(|e|e.code().to_string())?;
    crate::svg_render::validate(svg.as_bytes()).map_err(str::to_string)?;
    let settings=if matches!(engine,"vega"|"vega-lite"){profile(&serde_json::from_str(code).map_err(|_|"diagram_invalid_input")?,options)}else{options.clone()};
    Ok(size_svg(svg,&settings))
}

pub fn fences(source:&str,profile:&Value)->Vec<Value> {
    let mut engine=None;let mut code=String::new();let mut all=Vec::new();
    for event in Parser::new_ext(source,Options::all()) {match event {
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))=>{let lang=info.split_whitespace().next().unwrap_or("").to_lowercase();if is_diagram(&lang){engine=Some(lang);code.clear();}},
        Event::Text(text) if engine.is_some()=>code.push_str(&text),
        Event::End(TagEnd::CodeBlock) if engine.is_some()=>{let lang=engine.take().unwrap();all.push(json!({"engine":lang,"code":code,"key":identity(&lang,&code,profile)}));},_=>(),
    }}all
}

/// Replace only genuine diagram fences, preserving every other source byte.
/// The result is an ephemeral export input; no resource folder is created.
pub fn materialize(source:&str,options:&Value)->Result<String,String> {
    let mut start=None;let mut code=String::new();let mut edits=Vec::new();let mut cache:HashMap<String,String>=HashMap::new();let mut bytes=0usize;
    let started=Instant::now();let profile=options.get("figures").cloned().unwrap_or_else(||json!({}));
    let snapshots=profile.get("rendered").and_then(Value::as_array);
    for (event,range) in Parser::new_ext(source,Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))=>{
                let engine=info.split_whitespace().next().unwrap_or("").to_lowercase();
                if is_diagram(&engine){start=Some((range.start,engine));code.clear();}
            },
            Event::Text(text) if start.is_some()=>code.push_str(&text),
            Event::End(TagEnd::CodeBlock) if start.is_some()=>{
                if crate::speech::cancelled(){return Err("cancelled".into());}
                if edits.len()>=128||started.elapsed()>Duration::from_secs(120){return Err("diagram_input_too_large".into());}
                let (from,engine)=start.take().unwrap();let mut clean_profile=profile.clone();if let Some(obj)=clean_profile.as_object_mut(){obj.remove("rendered");}
                let id=identity(&engine,&code,&clean_profile);
                let svg=if let Some(svg)=cache.get(&id){svg.clone()}else{
                    let svg=if let Some(snapshot)=snapshots.and_then(|all|all.iter().find(|v|v["key"]==id)).and_then(|v|v["svg"].as_str()) {
                        crate::svg_render::validate(snapshot.as_bytes()).map_err(str::to_string)?;size_svg(snapshot.to_string(),&clean_profile)
                    }else{match render(&engine,&code,&clean_profile){
                        Ok(svg)=>svg,
                        Err(error) if error=="diagram_client_renderer_required"&&profile["style"]!="research"=>continue,
                        Err(error)=>return Err(error),
                    }};
                    bytes+=svg.len();if bytes>16*1024*1024{return Err("diagram_input_too_large".into());}cache.insert(id,svg.clone());svg
                };
                use base64::Engine;let uri=base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
                edits.push((from..range.end,format!("![{}](data:image/svg+xml;base64,{uri})\n",engine)));
            },_=>(),
        }
    }
    let mut output=source.to_string();for (range,text) in edits.into_iter().rev(){output.replace_range(range,&text);}Ok(output)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn print_profile_overrides_dimensions_without_destroying_author_settings(){
        let original=catalog()["templates"][0]["spec"].clone();
        let settings=json!({"style":"research","widthMm":183,"fontSizePt":8});
        let prepared=prepare_spec(original.clone(),&settings).unwrap();
        assert_eq!(prepared["data"],original["data"]);assert_eq!(prepared["layer"],original["layer"]);
        assert_eq!(prepared["config"]["axis"]["labelFontSize"],8.*96./72.);
        assert_eq!(profile(&original,&json!({"style":"original"}))["widthMm"],89);
        assert_eq!(profile(&original,&settings)["widthMm"],183);
        let sized=size_svg("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"200\"/>".into(),&settings);
        assert_eq!(print_width(sized.as_bytes()),Some(183.));assert_eq!(print_width(b"<svg data-readmd-print-width-mm=\"NaN\"/>"),None);
    }
    #[test]fn materialized_vector_reaches_the_real_publication_image_pipeline(){
        use base64::Engine;
        let svg="<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"60\"><text x=\"10\" y=\"30\">Evidence</text></svg>";
        let uri=format!("data:image/svg+xml;base64,{}",base64::engine::general_purpose::STANDARD.encode(svg));
        let mut warnings=Vec::new();let image=crate::mdexport::publication_image("",&uri,&mut warnings).unwrap().expect("valid inline vector");
        assert_eq!(image.0,svg.as_bytes());assert_eq!(image.1,"svg");assert!(warnings.is_empty());
        let unsafe_uri=format!("data:image/svg+xml;base64,{}",base64::engine::general_purpose::STANDARD.encode("<svg><script>bad</script></svg>"));
        assert!(crate::mdexport::publication_image("",&unsafe_uri,&mut warnings).unwrap().is_none());
    }
    #[test]fn styles_never_change_measurements_or_statistical_definitions(){
        let input=json!({"readmd":{"style":"research"},"data":{"values":[{"x":1,"y":2,"lo":1.5,"hi":2.5}]},"mark":{"type":"errorbar","extent":"stderr"},"encoding":{"x":{"field":"x","type":"quantitative","scale":{"zero":false}},"y":{"field":"y","type":"quantitative"}},"config":{"axis":{"labelFontSize":12,"grid":true}}});
        let prepared=prepare_spec(input.clone(),&json!({})).unwrap();
        for key in ["data","mark","encoding"]{assert_eq!(input[key],prepared[key]);}
        assert_eq!(prepared["config"]["axis"]["labelFontSize"],12);assert_eq!(prepared["config"]["axis"]["grid"],true);assert!(prepared.get("readmd").is_none());assert_eq!(input["readmd"]["style"],"research");
    }
    #[test]fn snapshots_are_source_bound_and_program_code_is_not_rendered(){
        let code="graph TD\nA-->B\n";let profile=json!({"style":"research"});let key=identity("mermaid",code,&profile);
        let svg=r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><path d="M0 0L80 40"/></svg>"##;
        let options=json!({"figures":{"style":"research","rendered":[{"key":key,"svg":svg}]}});
        let source=format!("```rust\nprintln!(\"keep me\");\n```\n\n```mermaid\n{code}```\n");let prepared=materialize(&source,&options).unwrap();
        assert!(prepared.contains("```rust"));assert!(!prepared.contains("```mermaid"));assert!(prepared.contains("data:image/svg+xml;base64,"));
        assert_eq!(materialize(&source.replace("A-->B","A-->C"),&options).unwrap_err(),"diagram_client_renderer_required");
        let unsafe_options=json!({"figures":{"style":"research","rendered":[{"key":identity("mermaid",code,&profile),"svg":"<svg><script>bad</script></svg>"}]}});
        assert_eq!(materialize(&source,&unsafe_options).unwrap_err(),"svg_unsafe");
    }
}
