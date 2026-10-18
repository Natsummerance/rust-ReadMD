//! Bounded, shared publication options. Missing optional keys stay missing so
//! old presets retain their identity; writers use the same explicit defaults.
use serde_json::{json, Value};
pub const NUMBER_FORMATS: &[&str] = &["decimal", "lowerRoman", "upperRoman", "lowerLetter", "upperLetter"];

pub const TOC_LEADERS: &[&str] = &["dot", "hyphen", "underscore", "none"];
/// Shared bounded depth. Direct writer APIs may receive unsanitized profiles.
pub fn toc_depth(style: &Value) -> u8 {
    style["toc"]["depth"].as_f64().filter(|v| v.is_finite())
        .unwrap_or(3.).round().clamp(1., 6.) as u8
}
pub fn toc_leader(style: &Value) -> &str {
    style["toc"]["leader"].as_str().filter(|v| TOC_LEADERS.contains(v)).unwrap_or("dot")
}

pub fn sanitize_optional(style: &mut Value) {
    for (section,key,default,min,max,integral) in [
        ("page","columns",1.,1.,3.,true),("page","columnGap",8.,1.,30.,false),("page","gutter",0.,0.,40.,false),
        ("header","distance",10.,0.,50.,false),("footer","distance",10.,0.,50.,false),
        ("header","size",9.,6.,24.,false),("footer","size",9.,6.,24.,false),("footer","start",1.,1.,9999.,true),
        ("footnotes","size",9.,6.,24.,false),("footnotes","lineHeight",1.,1.,3.,false),
        ("footnotes","start",1.,1.,9999.,true),
        ("toc","depth",3.,1.,6.,true),
    ] { if let Some(value)=style.get_mut(section).and_then(|s|s.get_mut(key)) {
        let n=value.as_f64().filter(|n|n.is_finite()).unwrap_or(default).clamp(min,max);
        *value=if integral{json!(n.round() as u32)}else{json!(n)};
    }}
    for (section,key,default) in [
        ("page","mirrorMargins",false),("page","differentFirstPage",false),("page","differentOddEven",false),("footer","showTotal",false),
        ("typography","widowControl",true),("typography","keepLines",false),
    ] { if let Some(value)=style.get_mut(section).and_then(|s|s.get_mut(key)) {*value=json!(value.as_bool().unwrap_or(default));} }
    if let Some(value)=style.get_mut("toc").and_then(|s|s.get_mut("leader")) {
        if !value.as_str().is_some_and(|s| TOC_LEADERS.contains(&s)){*value=json!("dot");}
    }
    for section in ["header","footer"]{if let Some(value)=style.get_mut(section).and_then(|s|s.get_mut("evenText")){
        *value=json!(value.as_str().unwrap_or("").chars().filter(|c|!c.is_control()).take(500).collect::<String>());
    }}
    for section in ["footer","footnotes"] {
        if let Some(value)=style.get_mut(section).and_then(|s|s.get_mut("numberFormat")) {
            if !value.as_str().is_some_and(|s|NUMBER_FORMATS.contains(&s)){*value=json!("decimal");}
        }
    }
    if let Some(value)=style.get_mut("footer").and_then(|s|s.get_mut("align")) {
        if !matches!(value.as_str(),Some("left"|"center"|"right")){*value=json!("center");}
    }
    if let Some(value)=style.get_mut("footnotes").and_then(|s|s.get_mut("position")) {
        if !matches!(value.as_str(),Some("page"|"document")){*value=json!("page");}
    }
    if let Some(value)=style.get_mut("footnotes").and_then(|s|s.get_mut("title")) {
        *value=json!(value.as_str().unwrap_or("").chars().filter(|c|!c.is_control()).take(120).collect::<String>());
    }
    if let Some(headings)=style.get_mut("headings").and_then(Value::as_object_mut) {
        for (_,heading) in headings {for key in ["keepNext","keepLines"] {
            if let Some(value)=heading.get_mut(key){*value=json!(value.as_bool().unwrap_or(true));}
        }}
    }
}

#[derive(Debug,Clone)]
pub struct Layout {
    pub columns:u32,pub column_gap:f64,pub gutter:f64,pub mirror:bool,
    pub header_distance:f64,pub footer_distance:f64,pub header_size:f64,pub footer_size:f64,pub footer_align:String,
    pub page_format:String,pub page_start:u32,pub page_total:bool,pub hide_first:bool,pub different_odd_even:bool,
    pub widow_control:bool,pub keep_lines:bool,pub note_size:f64,pub note_line_height:f64,pub note_format:String,pub note_start:u32,
}
impl Layout {
    /// Direct writer APIs also accept unsanitized options.
    pub fn from_style(style:&Value)->Self {
        let n=|s:&str,k:&str,d:f64,min:f64,max:f64|style[s][k].as_f64().filter(|n|n.is_finite()).unwrap_or(d).clamp(min,max);
        let b=|s:&str,k:&str,d|style[s][k].as_bool().unwrap_or(d);
        let format=|s:&str|style[s]["numberFormat"].as_str().filter(|s|NUMBER_FORMATS.contains(s)).unwrap_or("decimal").to_string();
        Self {
            columns:n("page","columns",1.,1.,3.).round() as u32,column_gap:n("page","columnGap",8.,1.,30.),
            gutter:n("page","gutter",0.,0.,40.),mirror:b("page","mirrorMargins",false),
            header_distance:n("header","distance",10.,0.,50.),footer_distance:n("footer","distance",10.,0.,50.),
            header_size:n("header","size",9.,6.,24.),footer_size:n("footer","size",9.,6.,24.),
            footer_align:style["footer"]["align"].as_str().filter(|s|["left","center","right"].contains(s)).unwrap_or("center").into(),
            page_format:format("footer"),page_start:n("footer","start",1.,1.,9999.).round() as u32,
            page_total:b("footer","showTotal",false),hide_first:b("page","differentFirstPage",b("cover","enabled",false)),different_odd_even:b("page","differentOddEven",false),
            widow_control:b("typography","widowControl",true),keep_lines:b("typography","keepLines",false),
            note_size:n("footnotes","size",9.,6.,24.),note_line_height:n("footnotes","lineHeight",1.,1.,3.),note_format:format("footnotes"),
            note_start:n("footnotes","start",1.,1.,9999.).round() as u32,
        }
    }
    pub fn column_width(&self,style:&Value)->f64 {
        let (width,_)=crate::export_styles::page_dimensions(style);
        (width-style["page"]["marginLeft"].as_f64().unwrap_or(18.)-style["page"]["marginRight"].as_f64().unwrap_or(18.)
            -self.gutter-(self.columns-1) as f64*self.column_gap)/self.columns as f64
    }
    pub fn validate(&self,style:&Value)->Result<(),String> {
        if self.column_width(style)<20. {return Err("publication_layout_geometry".into());}
        // Explicit distances opt into validation. Preserve legacy zero-margin presets.
        for (section,margin,distance,size,active) in [
            ("header","marginTop",self.header_distance,self.header_size,style["header"]["text"].as_str().is_some_and(|s|!s.is_empty())||self.different_odd_even&&style["header"]["evenText"].as_str().is_some_and(|s|!s.is_empty())),
            ("footer","marginBottom",self.footer_distance,self.footer_size,style["footer"]["pageNumbers"].as_bool().unwrap_or(true)||style["footer"]["text"].as_str().is_some_and(|s|!s.is_empty())||self.different_odd_even&&style["footer"]["evenText"].as_str().is_some_and(|s|!s.is_empty())),
        ] {
            if style[section].get("distance").is_some()&&active&&distance+size*25.4/72.>style["page"][margin].as_f64().unwrap_or(20.) {
                return Err("publication_running_overlap".into());
            }
        }
        Ok(())
    }
}
/// Display numbering is separate from note identity and backlink IDs.
/// Repeated references keep the same ordinal and retain their link targets.
pub fn note_index(style:&Value,ordinal:usize)->usize {
    ordinal.max(1).saturating_add(Layout::from_style(style).note_start as usize-1)
}
pub fn typst_number(format:&str)->&'static str {match format {
    "lowerRoman"=>"i","upperRoman"=>"I","lowerLetter"=>"a","upperLetter"=>"A",_=>"1",
}}
pub fn notes_at_end(style:&Value)->bool {style["footnotes"]["position"].as_str()==Some("document")}
pub fn note_title(style:&Value)->String {
    style["footnotes"]["title"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or("Notes").chars().filter(|c|!c.is_control()).take(120).collect()
}
/// Literal labels for flow outputs. Word uses its native numbering fields.
pub fn note_number(format:&str,index:usize)->String {
    let mut value=index.max(1);
    let result=match format {
        "lowerLetter"|"upperLetter"=>{let mut chars=Vec::new();while value>0{value-=1;chars.push((b'A'+(value%26) as u8) as char);value/=26;}chars.into_iter().rev().collect::<String>()},
        "lowerRoman"|"upperRoman"=>{let mut text=String::new();for (n,s)in [(1000,"M"),(900,"CM"),(500,"D"),(400,"CD"),(100,"C"),(90,"XC"),(50,"L"),(40,"XL"),(10,"X"),(9,"IX"),(5,"V"),(4,"IV"),(1,"I")]{while value>=n{text.push_str(s);value-=n;}}text},
        _=>value.to_string(),
    };
    if matches!(format,"lowerLetter"|"lowerRoman"){result.to_lowercase()}else{result}
}

/// Shared flow policy for HTML and EPUB. Omitted options keep legacy output
/// unchanged; explicit off must override the writer/reader's default rules.
pub fn paragraph_flow_css(style:&Value)->String {
    let mut css=String::new();
    if let Some(enabled)=style["typography"]["widowControl"].as_bool(){
        let lines=if enabled{2}else{1};
        css.push_str(&format!("p {{ orphans:{lines}; widows:{lines}; }}\n"));
    }
    if let Some(enabled)=style["typography"]["keepLines"].as_bool(){
        let value=if enabled{"avoid"}else{"auto"};
        css.push_str(&format!("p {{ break-inside:{value}; page-break-inside:{value}; }}\n"));
    }
    css
}
/// Also covers presets supplied through MCP, outside the visible controls.
pub fn warnings(format:&str,options:&Value)->Vec<String> {
    if format=="docx"||format=="pdf"&&options.pointer("/pdf/engine").and_then(Value::as_str)==Some("typst"){return Vec::new();}
    let paged=["page.columns","page.columnGap","page.gutter","page.mirrorMargins","page.differentFirstPage","page.differentOddEven","header.distance","header.size","footer.distance","footer.size","footer.align","footer.numberFormat","footer.start","footer.showTotal"];
    let paragraph=["typography.widowControl","typography.keepLines"];
    let notes=["footnotes.size","footnotes.lineHeight","footnotes.numberFormat","footnotes.position","footnotes.start"];
    let differs=|keys:&[&str]|keys.iter().any(|key| {
        let pointer=format!("/{}",key.replace('.',"/"));let Some(value)=options.pointer(&pointer)else{return false;};
        let fallback=match *key {
            "page.columns"|"footer.start"|"footnotes.start"=>json!(1),"page.columnGap"=>json!(8),"page.gutter"=>json!(0),
            "header.distance"|"footer.distance"=>json!(10),"header.size"|"footer.size"|"footnotes.size"=>json!(9),
            "footnotes.lineHeight"=>json!(1),"footer.align"=>json!("center"),"footer.numberFormat"|"footnotes.numberFormat"=>json!("decimal"),
            "footnotes.position"=>json!("page"),
            "typography.widowControl"=>json!(true),_=>json!(false),
        };if value.is_number()&&fallback.is_number(){value.as_f64()!=fallback.as_f64()}else{value!=&fallback}
    });
    let mut out=Vec::new();
    if differs(&paged){out.push("publication_layout_paged_unsupported".into());}
    let toc_changed = options["toc"]["enabled"].as_bool().unwrap_or(false)
        && format!="epub" && (toc_depth(options)!=3 || toc_leader(options)!="dot")
        || format=="epub" && toc_leader(options)!="dot";
    if toc_changed && !out.iter().any(|s|s=="publication_layout_paged_unsupported") {
        out.push("publication_layout_paged_unsupported".into());
    }
    if !matches!(format,"html"|"epub")&&differs(&paragraph){out.push("publication_layout_paragraph_unsupported".into());}
    if format!="epub"&&differs(&notes){out.push("publication_layout_notes_unsupported".into());}
    if !matches!(format,"html"|"epub")&&options["headings"].as_object().is_some_and(|h|h.values().any(|v|v["keepNext"]==false||v["keepLines"]==false)){out.push("publication_layout_heading_unsupported".into());}
    out
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn toc_parameters_are_optional_bounded_and_engine_aware(){
        let old=crate::export_styles::sanitize(&json!({}));
        assert!(old["toc"].get("depth").is_none());assert!(old["toc"].get("leader").is_none());
        for (value,want) in [(json!(0),1),(json!(4.6),5),(json!(7),6),(json!("4"),3)]{
            let clean=crate::export_styles::sanitize(&json!({"toc":{"depth":value}}));
            assert_eq!(toc_depth(&clean),want);
        }
        assert_eq!(toc_leader(&json!({"toc":{"leader":"unknown"}})),"dot");
        let custom=json!({"toc":{"enabled":true,"depth":5,"leader":"hyphen"}});
        assert!(warnings("docx",&custom).is_empty());
        assert!(warnings("pdf",&custom).contains(&"publication_layout_paged_unsupported".into()));
        assert!(warnings("epub",&json!({"toc":{"depth":6}})).is_empty());
        assert!(warnings("epub",&custom).contains(&"publication_layout_paged_unsupported".into()));
    }
    #[test]fn paragraph_flow_policy_preserves_omission_and_both_explicit_switches(){
        assert!(paragraph_flow_css(&crate::export_styles::sanitize(&json!({}))).is_empty());
        for widow in [false,true]{for keep in [false,true]{
            let style=crate::export_styles::sanitize(&json!({"typography":{"widowControl":widow,"keepLines":keep}}));
            let css=paragraph_flow_css(&style);let count=if widow{2}else{1};let value=if keep{"avoid"}else{"auto"};
            assert!(css.contains(&format!("orphans:{count}; widows:{count};")));assert!(css.contains(&format!("break-inside:{value}; page-break-inside:{value};")));
        }}
        assert!(!paragraph_flow_css(&json!({"typography":{"keepLines":true}})).contains("orphans"));
        assert!(!paragraph_flow_css(&json!({"typography":{"widowControl":false}})).contains("break-inside"));
    }
    #[test]fn paragraph_flow_capability_matches_actual_output_engines(){
        let style=json!({"typography":{"widowControl":false,"keepLines":true}});
        for format in ["html","epub","docx"]{assert!(warnings(format,&style).is_empty(),"{format}");}
        assert!(warnings("pdf",&style).contains(&"publication_layout_paragraph_unsupported".into()));
        assert!(warnings("pdf",&json!({"typography":style["typography"],"pdf":{"engine":"typst"}})).is_empty());
    }
    #[test]fn heading_flow_properties_are_projected_by_css_but_not_claimed_for_native_pdf(){
        let options=json!({"headings":{"h2":{"keepNext":false,"keepLines":false,"pageBreakBefore":true}}});
        for format in ["html","epub","docx"]{assert!(warnings(format,&options).is_empty(),"{format}");}
        assert_eq!(warnings("pdf",&options),vec!["publication_layout_heading_unsupported"]);
        assert!(warnings("pdf",&json!({"pdf":{"engine":"typst"},"headings":options["headings"]})).is_empty());
    }
    #[test]fn dual_running_pages_are_optional_bounded_and_check_even_only_geometry(){
        let old=crate::export_styles::sanitize(&json!({}));assert!(old["page"].get("differentOddEven").is_none());assert!(old["header"].get("evenText").is_none());
        let style=crate::export_styles::sanitize(&json!({"page":{"differentOddEven":true,"marginTop":20},"header":{"text":"","evenText":"B\nook","distance":30}}));assert_eq!(style["header"]["evenText"],"Book");let layout=Layout::from_style(&style);assert!(layout.different_odd_even);assert_eq!(layout.validate(&style).unwrap_err(),"publication_running_overlap");
        assert!(warnings("epub",&style).contains(&"publication_layout_paged_unsupported".into()));assert!(warnings("pdf",&style).contains(&"publication_layout_paged_unsupported".into()));assert!(warnings("pdf",&json!({"page":{"differentOddEven":true},"pdf":{"engine":"typst"}})).is_empty());
        let invalid=crate::export_styles::sanitize(&json!({"page":{"differentOddEven":"true"},"header":{"evenText":true},"footer":{"evenText":"a".repeat(501)}}));assert!(!Layout::from_style(&invalid).different_odd_even);assert_eq!(invalid["header"]["evenText"],"");assert_eq!(invalid["footer"]["evenText"].as_str().unwrap().len(),500);
    }
    #[test]fn note_start_is_bounded_optional_and_distinct_from_link_identity(){
        let old=crate::export_styles::sanitize(&json!({}));
        assert!(old.get("footnotes").is_none());assert_eq!(note_index(&old,1),1);
        for (input,expected) in [(json!(0),1),(json!(27),27),(json!(4.8),5),(json!(10000),9999),(json!("27"),1)]{
            let style=crate::export_styles::sanitize(&json!({"footnotes":{"start":input}}));
            assert_eq!(Layout::from_style(&style).note_start,expected);
            assert_eq!(note_index(&style,2),expected as usize+1);
        }
        assert!(warnings("pdf",&json!({"footnotes":{"start":4}})).contains(&"publication_layout_notes_unsupported".into()));
        assert!(warnings("epub",&json!({"footnotes":{"start":4}})).is_empty());
    }
    #[test]fn endnotes_preserve_absent_options_and_report_unsupported_engines(){
        let old=crate::export_styles::sanitize(&json!({}));assert!(old.get("footnotes").is_none());assert!(!notes_at_end(&old));
        let style=crate::export_styles::sanitize(&json!({"footnotes":{"position":"document","title":"Research\nnotes","numberFormat":"lowerRoman"}}));assert!(notes_at_end(&style));assert_eq!(note_title(&style),"Researchnotes");
        assert!(warnings("docx",&style).is_empty());assert!(warnings("epub",&style).is_empty());assert!(warnings("pdf",&style).contains(&"publication_layout_notes_unsupported".into()));
        let invalid=crate::export_styles::sanitize(&json!({"footnotes":{"position":"chapter","title":false}}));assert!(!notes_at_end(&invalid));assert_eq!(note_title(&invalid),"Notes");
    }
    #[test]fn flow_note_numbering_handles_alphabet_boundaries_and_roman_subtraction(){
        for (n,alpha,roman)in [(1,"a","i"),(26,"z","xxvi"),(27,"aa","xxvii"),(49,"aw","xlix"),(99,"cu","xcix"),(999,"alk","cmxcix")]{assert_eq!(note_number("lowerLetter",n),alpha);assert_eq!(note_number("lowerRoman",n),roman);}
        assert_eq!(note_number("upperLetter",27),"AA");assert_eq!(note_number("upperRoman",49),"XLIX");assert_eq!(note_number("decimal",27),"27");
    }
    #[test]fn optional_values_preserve_legacy_identity_and_bound_untrusted_input(){
        let original=crate::export_styles::sanitize(&json!({}));let mut same=original.clone();sanitize_optional(&mut same);assert_eq!(same,original);
        let s=crate::export_styles::sanitize(&json!({"page":{"columns":2.7,"columnGap":300,"gutter":-1,"mirrorMargins":"false"},"footer":{"start":0,"numberFormat":"I\"#read"},"footnotes":{"size":"9pt"},"headings":{"h2":{"keepNext":false}}}));
        let l=Layout::from_style(&s);assert_eq!(l.columns,3);assert_eq!(l.column_gap,30.);assert_eq!(l.gutter,0.);assert!(!l.mirror);assert_eq!(l.page_format,"decimal");assert_eq!(l.page_start,1);assert_eq!(l.note_size,9.);assert_eq!(s["headings"]["h2"]["keepNext"],false);
    }
    #[test]fn cramped_columns_and_running_heads_are_rejected(){
        let s=crate::export_styles::sanitize(&json!({"page":{"size":"A5","columns":3,"columnGap":30,"gutter":40}}));assert_eq!(Layout::from_style(&s).validate(&s).unwrap_err(),"publication_layout_geometry");
        let s=crate::export_styles::sanitize(&json!({"footer":{"distance":30,"size":24}}));assert_eq!(Layout::from_style(&s).validate(&s).unwrap_err(),"publication_running_overlap");
    }
    #[test]fn cross_engine_presets_report_limitations(){
        let o=json!({"page":{"columns":2},"typography":{"keepLines":true},"footnotes":{"size":8}});assert_eq!(warnings("pdf",&o).len(),3);assert!(warnings("docx",&o).is_empty());
        assert!(warnings("html",&json!({"page":{"columns":1},"footer":{"start":1},"typography":{"keepLines":false}})).is_empty());
    }
}
