//! EPUB 3 chapter bodies from the shared Markdown AST ([`crate::md_ast`]).
//!
//! Replaces the line parser the EPUB export used to share with the retired
//! Python port: nested and ordered lists, task items, table alignment, fenced
//! code with syntax colour, footnotes as EPUB pop-up notes, MathML formulas
//! (through the same LaTeX parser DOCX uses) and packaged local images.
//! Output is well-formed XHTML: raw HTML from the source is reduced to its
//! text, never copied through.

use crate::md_ast::{inline_plain, Align, Block, Document, Inline};
use std::collections::HashMap;

#[cfg(test)]
#[path = "epub_numeric_inheritance_tests.rs"]
mod numeric_inheritance_tests;

/// EPUB overrides are sparse. Missing/null values inherit body typography or
/// the reflow margin defaults; a numeric zero is an explicit margin choice.
pub(crate) const NUMERIC_INVALID: &str = "epub_invalid_options";
const NUMERIC_FIELDS: [(&str, f64, f64); 4] = [
    ("fontSize", 6.0, 72.0), ("lineHeight", 1.0, 5.0),
    ("marginV", 0.0, 40.0), ("marginH", 0.0, 40.0),
];

pub(crate) fn numeric_error_field(options: &serde_json::Value) -> &'static str {
    use serde_json::Value;
    let epub = &options["epub"];
    for (key, min, max) in NUMERIC_FIELDS {
        if epub.get(key).filter(|value| !value.is_null()).is_some_and(|value|
            !value.as_f64().is_some_and(|n| n.is_finite() && n >= min && n <= max)) {
            return match key { "fontSize" => "epub.fontSize", "lineHeight" => "epub.lineHeight", "marginV" => "epub.marginV", _ => "epub.marginH" };
        }
    }
    if !epub.is_null() && !matches!(epub, Value::Object(_)) { "epub" } else { "" }
}

/// Validate sparse layers before sanitization or preset persistence. Never
/// coerce strings, booleans or malformed values into successful CSS.
pub(crate) fn validate_numeric_request(options: &serde_json::Value) -> Result<(), &'static str> {
    if numeric_error_field(options).is_empty() { Ok(()) } else { Err(NUMERIC_INVALID) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NumericProfile {
    pub font_size: f64, pub line_height: f64, pub margin_v: f64, pub margin_h: f64,
}

pub(crate) fn numeric_profile(options: &serde_json::Value) -> Result<NumericProfile, &'static str> {
    validate_numeric_request(options)?;
    let style = crate::export_styles::sanitize(options);
    let value = |key: &str, inherited: f64| options["epub"].get(key).and_then(serde_json::Value::as_f64).unwrap_or(inherited);
    Ok(NumericProfile {
        font_size: value("fontSize", style["typography"]["size"].as_f64().unwrap_or(11.0)),
        line_height: value("lineHeight", style["typography"]["lineHeight"].as_f64().unwrap_or(1.6)),
        margin_v: value("marginV", 5.0), margin_h: value("marginH", 8.0),
    })
}

/// Native glyph layout resolves operator-indexed breaks. MathML readers do not
/// share those metrics; report the projection gap rather than silently certify
/// a data attribute as functional alignment.
pub fn mathml_alignment_warnings(doc:&Document)->Vec<String>{
    fn math(xml:&str,out:&mut std::collections::BTreeSet<String>){
        if let Ok(root)=crate::omml::parse(xml){out.extend(crate::omml::mathml_warnings(&root));}
    }
    fn blocks(values:&[Block],out:&mut std::collections::BTreeSet<String>){
        for b in values{match b.unstyled(){
            Block::OfficeMath{omml,..}=>math(omml,out),
            Block::Quote(v)|Block::FootnoteDef{blocks:v,..}=>blocks(v,out),
            Block::List{items,..}=>{for i in items{blocks(&i.blocks,out);}},_=>{},
        }}
    }
    let mut found=std::collections::BTreeSet::new();blocks(&doc.blocks,&mut found);
    crate::md_ast::walk_inlines(&doc.blocks,&mut |i|{if let Inline::OfficeMath{omml,..}=i{math(omml,&mut found);}});
    found.into_iter().collect()
}

/// One EPUB chapter.
pub struct Chapter {
    pub title: String,
    pub body: String,
    pub headings: Vec<NavHeading>,
    /// The body contains MathML (the OPF item needs `properties="mathml"`).
    pub has_math: bool,
    pub math_fonts: std::collections::BTreeSet<(String, bool)>,
    pub math_font_warnings: bool,
}

/// Headings retain the same target identity used by the XHTML body.
#[derive(Debug, Clone)]
pub struct NavHeading { pub level:u8, pub title:String, pub id:String }
#[derive(Debug)]
pub struct NavItem { pub level:u8, pub title:String, pub href:String, pub children:Vec<NavItem> }

/// Chapter entries are always reachable, even when the requested heading
/// depth is shallower than the split level. Nested headings never require
/// synthetic/empty list items when an author skips levels.
pub fn navigation(chapters:&[Chapter], depth:u8)->Vec<NavItem>{
    fn insert(items:&mut Vec<NavItem>,item:NavItem){
        if let Some(last)=items.last_mut(){if last.level<item.level{insert(&mut last.children,item);return;}}
        items.push(item);
    }
    chapters.iter().enumerate().map(|(index,chapter)|{
        let file=format!("chapter_{}.xhtml",index+1);
        let first=chapter.headings.first().filter(|h|h.title==chapter.title);
        let mut root=NavItem{level:first.map_or(0,|h|h.level),title:chapter.title.clone(),href:first.map_or_else(||file.clone(),|h|format!("{file}#{}",xml_id(&h.id))),children:Vec::new()};
        for (i,h) in chapter.headings.iter().enumerate(){
            if h.level>depth.clamp(1,6)||i==0&&first.is_some(){continue;}
            insert(&mut root.children,NavItem{level:h.level,title:h.title.clone(),href:format!("{file}#{}",xml_id(&h.id)),children:Vec::new()});
        }
        root
    }).collect()
}
pub fn navigation_xhtml(items:&[NavItem])->String{
    items.iter().map(|item|{
        let children=if item.children.is_empty(){String::new()}else{format!("<ol>{}</ol>",navigation_xhtml(&item.children))};
        format!("<li><a href=\"{}\">{}</a>{children}</li>",esc(&item.href),esc(&item.title))
    }).collect::<Vec<_>>().join("\n")
}
pub fn navigation_ncx(items:&[NavItem])->(String,usize){
    fn write(items:&[NavItem],next:&mut usize,level:usize,max:&mut usize)->String{
        let mut out=String::new();
        for item in items{
            *next+=1;*max=(*max).max(level);let id=*next;
            let children=write(&item.children,next,level+1,max);
            out.push_str(&format!("<navPoint id=\"navPoint-{id}\" playOrder=\"{id}\"><navLabel><text>{}</text></navLabel><content src=\"{}\"/>{children}</navPoint>\n",esc(&item.title),esc(&item.href)));
        }
        out
    }
    let mut next=0;let mut max=0;let out=write(items,&mut next,1,&mut max);(out,max)
}
fn navigation_headings(block:&Block,out:&mut Vec<NavHeading>){
    match block.unstyled(){
        Block::Heading{level,id,inlines}=>out.push(NavHeading{level:*level,id:id.clone(),title:inline_plain(inlines).trim().to_string()}),
        Block::Quote(inner)=>{for b in inner{navigation_headings(b,out);}},
        Block::List{items,..}=>{for b in items.iter().flat_map(|i|&i.blocks){navigation_headings(b,out);}},
        _=>{}
    }
}

/// Maps a Markdown image `src` to its package-relative href (`images/img_1.png`),
/// or `None` when the image cannot be packaged (the caller records why).
pub type ImageMap<'a> = &'a mut dyn FnMut(&str) -> Option<String>;

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            // Characters XML 1.0 forbids.
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => {}
            _ => o.push(c),
        }
    }
    o
}

/// Text content of a raw HTML fragment (tags and comments removed, entities
/// kept as written but re-escaped).
fn html_text(raw: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut rest = raw;
    while !rest.is_empty() {
        if !in_tag {
            if let Some(stripped) = rest.strip_prefix("<!--") {
                rest = match stripped.find("-->") {
                    Some(e) => &stripped[e + 3..],
                    None => "",
                };
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        rest = &rest[c.len_utf8()..];
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    crate::mdexport::html_unescape(&out)
}

pub(crate) fn xml_id(slug: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut s = String::from("h-");
    let mut encoded = false;
    for c in slug.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
            s.push(c);
        } else {
            s.push_str(&format!("-u{:x}-", c as u32)); encoded = true;
        }
    }
    if encoded { s.push('-'); s.push_str(&format!("{:x}", Sha256::digest(slug.as_bytes()))[..16]); }
    s
}

fn safe_href(href: &str) -> bool {
    let l = href.trim().to_ascii_lowercase();
    !(l.starts_with("javascript:") || l.starts_with("vbscript:") || l.starts_with("data:"))
}

/// A resolution-independent, self-contained cover. Text stays searchable in
/// the corresponding XHTML page; no screenshot or temporary image is created.
pub fn cover_svg(title:&str,subtitle:&str,author:&str,date:&str,align:&str)->String {
    let (x,anchor)=match align{"left"=>(240,"start"),"right"=>(2160,"end"),_=>(1200,"middle")};
    let mut lines=Vec::new();let mut line=String::new();
    for c in title.chars().take(512){
        if line.chars().count()>=18 || c=='\n' {lines.push(std::mem::take(&mut line));}
        if c!='\n'{line.push(c);}
    }
    if !line.is_empty(){lines.push(line);}
    let size=(1100.0/(lines.len().max(1) as f64)).clamp(32.0,150.0);
    let text=lines.iter().enumerate().map(|(n,line)|format!("<tspan x=\"{x}\" y=\"{}\">{}</tspan>",940.0+n as f64*size*1.4,esc(line))).collect::<String>();
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="2400" height="3200" viewBox="0 0 2400 3200"><rect width="2400" height="3200" fill="#f4efe5"/><rect x="180" y="180" width="2040" height="2840" fill="none" stroke="#c8bba4" stroke-width="3"/><path d="M240 570H2160" stroke="#a9926f" stroke-width="4"/><g text-anchor="{anchor}" font-family="serif" fill="#202b32"><text x="{x}" y="450" font-size="44" letter-spacing="8">{}</text><text font-size="{size}" font-weight="600">{text}</text><text x="{x}" y="2380" font-size="58">{}</text><text x="{x}" y="2670" font-size="48">{}</text><text x="{x}" y="2820" font-size="34" fill="#6b6a64">{}</text></g></svg>"##,esc(subtitle),esc(author),esc(date),esc(title))
}

/// Shared print/Word style parameters projected into a reflowable book. Page
/// geometry stays with paged formats; the EPUB-specific type size is independent
/// so a reader can still choose a comfortable reading size.
pub fn profile_css(options:&serde_json::Value)->String {
    // Public export entry points validate first; direct style projections use
    // the same effective body size rather than a second inheritance policy.
    let body=numeric_profile(options).map(|profile|profile.font_size).unwrap_or(11.0);
    let style=crate::export_styles::sanitize(options);
    let n=|section:&str,key:&str|style[section][key].as_f64().unwrap_or(0.0);
    let s=|section:&str,key:&str|style[section][key].as_str().unwrap_or("");
    let font=match s("typography","font"){
        "SimSun"=>"\"SimSun\",\"Songti SC\",serif","KaiTi"=>"\"KaiTi\",\"Kaiti SC\",serif",
        "SimHei"=>"\"SimHei\",sans-serif","DengXian"=>"\"DengXian\",sans-serif","Arial"=>"Arial,sans-serif",
        _=>"\"Microsoft YaHei\",\"PingFang SC\",sans-serif",
    };
    let mut css=format!("\nbody{{font-family:{font};color:{}}}\np{{margin:0 0 {}pt;text-align:{};text-indent:{}mm}}\na{{color:{}}}\nfigure img{{max-width:{}%}}\n",
        s("typography","color"),n("typography","spacing"),s("typography","align"),n("typography","firstLineIndent"),s("link","color"),n("images","widthPct"));
    css.push_str(&crate::publication_layout::paragraph_flow_css(&style));
    let defaults=crate::export_styles::default_style();
    for level in 1..=6{
        let key=format!("h{level}");let h=&style["headings"][&key];
        // Default ink must follow an EPUB reader's light/night foreground.
        // Explicit non-default author colours remain part of the profile.
        let heading_color=if h["color"]==defaults["headings"][&key]["color"]{"inherit"}else{h["color"].as_str().unwrap_or("inherit")};
        css.push_str(&format!("h{level}{{font-family:inherit;font-size:{}em;color:{};font-weight:{};text-align:{};margin-top:{}pt;margin-bottom:{}pt;text-indent:0;{} }}\n",
            h["size"].as_f64().unwrap_or(12.0)/body,heading_color,if h["bold"]==true{"700"}else{"400"},h["align"].as_str().unwrap_or("left"),h["before"],h["after"],if h["pageBreakBefore"]==true{"break-before:page;page-break-before:always;"}else{""}));
        for (key,property) in [("keepNext","after"),("keepLines","inside")]{
            if let Some(enabled)=h[key].as_bool(){let value=if enabled{"avoid"}else{"auto"};
                css.push_str(&format!("h{level}{{break-{property}:{value};page-break-{property}:{value}}}\n"));
            }
        }
    }
    css.push_str(&format!("table{{width:{}%}}th,td{{border:{}px solid {};padding:{}px;text-align:{}}}th{{background:{};color:{};font-weight:{}}}\npre{{background:{};color:{};border:{}px solid {};border-radius:{}px}}pre,code{{font-family:\"{}\",monospace}}blockquote{{background:{};color:{};border-left-color:{}}}\n",
        n("table","widthPct"),n("table","borderWidth"),s("table","borderColor"),n("table","cellPadding"),s("table","align"),s("table","headerBg"),s("table","headerColor"),if style["table"]["headerBold"]==true{"700"}else{"400"},
        s("code","bg"),s("code","color"),n("code","borderWidth"),s("code","borderColor"),if style["code"]["rounded"]==true{6}else{0},s("code","font"),s("quote","bg"),s("quote","color"),s("quote","barColor")));
    if style["table"]["banded"]==true{
        // A reader can change the foreground independently of author CSS.
        // An opaque default light stripe then makes its night-mode text almost
        // invisible. A subtle neutral overlay works on both reader surfaces,
        // without relying on a particular reader's dark-mode class/media hint.
        // Keep explicitly customised palette values and print exports intact.
        let band=if style["table"]["bandColor"]==defaults["table"]["bandColor"]{
            "rgba(127,127,127,0.10)"
        }else{s("table","bandColor")};
        css.push_str(&format!("table:not(.bibliography-columns) tbody tr:nth-child(even){{background:{band}}}\n"));
    }
    css.push_str(&format!("table{{font-size:{}em}}pre{{font-size:{}em}}pre code{{font-size:1em;background:transparent}}hr{{border:0;border-top:1px solid {}}}\n",n("table","cellSize")/body,n("code","size")/body,s("hr","color")));
    css.push_str("li p,blockquote p,.footnotes p,figcaption,.publication-caption{ text-indent:0 }\n");
    let notes=crate::publication_layout::Layout::from_style(&style);
    css.push_str(&format!(".footnotes{{font-size:{}em;line-height:{}}}\n",notes.note_size/body,notes.note_line_height));
    css.push_str(&crate::export_styles::scientific_table_css(&style,"body"));
    css
}

struct Writer<'a, 'm> {
    style: &'a serde_json::Value,
    images: ImageMap<'m>,
    /// slug → chapter file for cross-chapter `#anchor` links.
    anchors: &'a HashMap<String, String>,
    footnote_defs: &'a HashMap<String, Vec<Block>>,
    /// Footnotes referenced in the current chapter, in order.
    used_notes: Vec<String>,
    /// Per-note occurrence IDs: repeated references must never duplicate IDs.
    note_refs: HashMap<String, Vec<String>>,
    /// A document-wide note chapter uses global IDs and qualified backlinks.
    note_file:Option<String>,file:String,
    in_note: bool,
    has_math: bool,
    math_fonts: std::collections::BTreeSet<(String, bool)>,
    math_font_warnings: bool,
}

impl<'a, 'm> Writer<'a, 'm> {
    fn rich_flow(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],hanging:bool)->String{
        use crate::publication_rich::RichLayoutNode as L;
        fn flush(out:&mut String,pending:&mut String,hanging:bool){
            if !pending.bytes().all(|b|b.is_ascii_whitespace()){out.push_str(&format!("<div class=\"readmd-csl-layout-paragraph{}\">{}</div>",if hanging{" readmd-csl-layout-hanging"}else{""},pending));}pending.clear();
        }
        fn walk(writer:&mut Writer<'_, '_>,nodes:&[L],hanging:bool,href:Option<&str>,out:&mut String,pending:&mut String){
            for node in nodes{match node{
                L::Inline{children}=>{let body=writer.inlines(&crate::md_ast::rich_inlines(children));pending.push_str(&if let Some(href)=href{format!("<a href=\"{}\">{body}</a>",esc(href))}else{body});},
                L::Group{children,..}=>walk(writer,children,hanging,href,out,pending),
                L::Link{href,children}=>walk(writer,children,hanging,Some(href),out,pending),
                L::Block{children,indent}=>{flush(out,pending,hanging);let mut body=String::new();let mut text=String::new();walk(writer,children,hanging,href,&mut body,&mut text);flush(&mut body,&mut text,hanging);out.push_str(&format!("<div class=\"readmd-csl-layout-block{}\">{body}</div>",if *indent{" readmd-csl-layout-indent"}else{""}));},
                L::Columns{left,right}=>{flush(out,pending,hanging);let mut cells=Vec::new();for cell in [left,right]{let mut body=String::new();let mut text=String::new();walk(writer,cell,false,href,&mut body,&mut text);flush(&mut body,&mut text,false);cells.push(body);}out.push_str(&format!("<div class=\"readmd-csl-layout-columns\"><div class=\"readmd-csl-layout-left\">{}</div><div class=\"readmd-csl-layout-right\">{}</div></div>",cells[0],cells[1]));},
            }}
        }
        let mut out=String::new();let mut pending=String::new();walk(self,nodes,hanging,None,&mut out,&mut pending);flush(&mut out,&mut pending,hanging);out
    }
    fn rich_inline(&mut self,nodes:&[crate::publication_rich::RichLayoutNode])->String{
        use crate::publication_rich::RichLayoutNode as L;
        fn walk(writer:&mut Writer<'_, '_>,nodes:&[L],href:Option<&str>)->String{
            nodes.iter().map(|node|match node{
                L::Inline{children}=>{let body=writer.inlines(&crate::md_ast::rich_inlines(children));if let Some(href)=href{format!("<a href=\"{}\">{body}</a>",esc(href))}else{body}},
                L::Group{children,..}=>walk(writer,children,href),
                L::Link{href,children}=>walk(writer,children,Some(href)),
                _=>String::new(),
            }).collect()
        }
        walk(self,nodes,None)
    }
    fn rich_rows(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],hanging:bool,rows:&mut Vec<(Vec<String>,bool)>,inline:&mut String){
        use crate::publication_rich::RichLayoutNode as L;
        fn flush(rows:&mut Vec<(Vec<String>,bool)>,inline:&mut String){if !inline.trim().is_empty(){rows.push((vec![std::mem::take(inline)],true));}else{inline.clear();}}
        for node in nodes{match node{
            L::Inline{..}=>inline.push_str(&self.rich_inline(std::slice::from_ref(node))),
            L::Group{children,..}=>self.rich_rows(children,hanging,rows,inline),
            L::Link{..} if crate::publication_rich::layout_is_inline(std::slice::from_ref(node))=>inline.push_str(&self.rich_inline(std::slice::from_ref(node))),
            L::Columns{left,right}=>{flush(rows,inline);rows.push((vec![self.rich_flow(left,false),self.rich_flow(right,false)],false));},
            _=>{flush(rows,inline);rows.push((vec![self.rich_flow(std::slice::from_ref(node),hanging)],crate::publication_rich::layout_is_inline(std::slice::from_ref(node))));},
        }}
    }
    fn rich_bibliography(&mut self,bib:&crate::publication_rich::RichBibliography)->String{
        let Ok(layouts)=bib.items.iter().map(|item|item.layout()).collect::<crate::ApiResult<Vec<_>>>()else{return String::new();};
        let columns=layouts.iter().any(|layout|crate::publication_rich::layout_has_columns(layout));
        let mut out=format!("<section role=\"doc-bibliography\" class=\"bibliography bibliography-complex\" style=\"line-height:{}\"><h2>{}</h2><div class=\"bibliography-flow{}\">",bib.line_spacing,esc(&bib.title),if columns{" bibliography-flow-columns"}else{""});
        for (index,layout) in layouts.iter().enumerate(){
            let mut rows=Vec::new();let mut inline=String::new();self.rich_rows(layout,bib.hanging_indent,&mut rows,&mut inline);if !inline.trim().is_empty(){rows.push((vec![inline],true));}
            let last=rows.len().saturating_sub(1);for (row,(cells,inline_row)) in rows.into_iter().enumerate(){
                let gap=if row==last&&index+1<layouts.len(){bib.entry_gap_em()}else{0};
                let id=if row==0{format!(" id=\"{}\"",xml_id(&format!("bib-{}",bib.items[index].key)))}else{String::new()};
                if cells.len()==2{out.push_str(&format!("<div{id} class=\"readmd-csl-layout-left\" style=\"padding-bottom:{gap}em\">{}</div><div class=\"readmd-csl-layout-right\" style=\"padding-bottom:{gap}em\">{}</div>",cells[0],cells[1]));}
                else{out.push_str(&format!("<div{id} class=\"bibliography-flow-full{}\" style=\"padding-bottom:{gap}em\">{}</div>",if bib.hanging_indent&&inline_row{" bibliography-flow-hanging"}else{""},cells[0]));}
            }
        }
        out.push_str("</div></section>");out
    }
    fn office_math(&mut self,latex:&str,xml:&str,display:bool)->String {
        self.has_math=true;
        let size=if self.in_note{crate::publication_layout::Layout::from_style(self.style).note_size}else{self.style["typography"]["size"].as_f64().unwrap_or(12.0)};
        let color=self.style["typography"]["color"].as_str().and_then(|s|s.strip_prefix('#')).filter(|s|s.len()==6&&s.bytes().all(|c|c.is_ascii_hexdigit())).map(|s|[u8::from_str_radix(&s[..2],16).unwrap(),u8::from_str_radix(&s[2..4],16).unwrap(),u8::from_str_radix(&s[4..],16).unwrap()]);
        crate::omml::parse(xml).map(|root|{
            self.math_fonts.extend(crate::math_layout::mathml_fonts(&root));
            self.math_font_warnings|=!crate::math_layout::font_warnings(&root).is_empty();
            crate::omml::to_mathml_with_context(&root,display,latex,size,color)
        }).unwrap_or_else(|_|esc(latex))
    }
    fn inlines(&mut self, v: &[Inline]) -> String {
        let mut out = String::new();
        for (idx, i) in v.iter().enumerate() {
            match i {
                Inline::Text(t) => out.push_str(&esc(t)),
                Inline::Code(t) => out.push_str(&format!("<code>{}</code>", esc(t))),
                Inline::Strong(c) => out.push_str(&format!("<strong>{}</strong>", self.inlines(c))),
                Inline::Emph(c) => out.push_str(&format!("<em>{}</em>", self.inlines(c))),
                Inline::Strike(c) => out.push_str(&format!("<del>{}</del>", self.inlines(c))),
                Inline::Styled { format, children } => {
                    use crate::publication_rich::{RichFormat, RichFontStyle, RichFontWeight};
                    let css=crate::md_ast::rich_format_css(format);
                    if css.is_empty(){out.push_str(&self.inlines(children));}
                    else if *format == (RichFormat{font_style:RichFontStyle::Italic,..Default::default()}) {
                        out.push_str(&format!("<em>{}</em>",self.inlines(children)));
                    }
                    else if *format == (RichFormat{font_weight:RichFontWeight::Bold,..Default::default()}) {
                        out.push_str(&format!("<strong>{}</strong>",self.inlines(children)));
                    }
                    else{out.push_str(&format!("<span style=\"{css}\">{}</span>",self.inlines(children)));}
                },
                Inline::Display { display, children } => {
                    let css = match display { crate::publication_rich::RichDisplay::Block => "display:block;", crate::publication_rich::RichDisplay::Indent => "display:block;padding-left:4em;", crate::publication_rich::RichDisplay::LeftMargin => "display:inline-block;", crate::publication_rich::RichDisplay::RightInline => "display:inline;" };
                    out.push_str(&format!("<span style=\"{css}\">{}</span>", self.inlines(children)));
                }
                Inline::Citation { children, .. } => out.push_str(&self.inlines(children)),
                Inline::Link { href, title, children } => {
                    let inner = self.inlines(children);
                    let inner = if inner.is_empty() { esc(href) } else { inner };
                    if !safe_href(href) || href.is_empty() {
                        out.push_str(&inner);
                        continue;
                    }
                    let target = match href.strip_prefix('#') {
                        Some(anchor) => {
                            let id = percent_encoding::percent_decode_str(anchor).decode_utf8_lossy();
                            match self.anchors.get(id.as_ref()) {
                                Some(file) => format!("{file}#{}", xml_id(&id)),
                                None => format!("#{}", xml_id(&id)),
                            }
                        }
                        None => href.clone(),
                    };
                    let title_attr = if title.is_empty() { String::new() } else { format!(" title=\"{}\"", esc(title)) };
                    out.push_str(&format!("<a href=\"{}\"{title_attr}>{inner}</a>", esc(&target)));
                }
                Inline::Image { src, alt, .. } => match (self.images)(src) {
                    Some(href) => out.push_str(&format!("<img src=\"{}\" alt=\"{}\"/>", esc(&href), esc(alt))),
                    None => {
                        if !alt.is_empty() {
                            out.push_str(&format!("<span class=\"img-missing\">[{}]</span>", esc(alt)));
                        }
                    }
                },
                Inline::Math(m) => out.push_str(&self.math(m, false)),
                Inline::OfficeMath {latex,omml} => out.push_str(&self.office_math(latex,omml,false)),
                Inline::SoftBreak => {
                    // A source line break between two CJK characters is not a space.
                    let prev = crate::md_ast::inline_plain(&v[..idx]).chars().last();
                    let next = crate::md_ast::inline_plain(&v[idx + 1..]).chars().next();
                    let cjk = matches!((prev, next), (Some(a), Some(b)) if crate::latex_writer::is_cjk(a) && crate::latex_writer::is_cjk(b));
                    if !cjk {
                        out.push(' ');
                    }
                }
                Inline::HardBreak => out.push_str("<br/>"),
                Inline::FootnoteRef(label) => {
                    if self.footnote_defs.contains_key(label) {
                        let n = match self.used_notes.iter().position(|l| l == label) {
                            Some(p) => p + 1,
                            None => {
                                self.used_notes.push(label.clone());
                                self.used_notes.len()
                            }
                        };
                        let refs=self.note_refs.entry(label.clone()).or_default();
                        let id=if refs.is_empty(){format!("fnref-{n}")}else{format!("fnref-{n}-{}",refs.len()+1)};
                        refs.push(if self.note_file.is_some(){format!("{}#{id}",self.file)}else{id.clone()});
                        let destination=self.note_file.as_ref().map_or_else(||format!("#fn-{n}"),|file|format!("{file}#fn-{n}"));
                        let number=crate::publication_layout::note_number(self.style["footnotes"]["numberFormat"].as_str().unwrap_or("decimal"),crate::publication_layout::note_index(self.style,n));
                        out.push_str(&format!(
                            "<a epub:type=\"noteref\" role=\"doc-noteref\" href=\"{destination}\" id=\"{id}\" class=\"noteref\"><sup>{number}</sup></a>"
                        ));
                    } else {
                        out.push_str(&format!("[{}]", esc(label)));
                    }
                }
                Inline::Html(h) => {
                    if let Ok(Some(anchor))=crate::publication_navigation::anchor_html(h){out.push_str(&anchor);}else{out.push_str(&esc(&html_text(h)));}
                },
            }
        }
        out
    }

    fn math(&mut self, latex: &str, display: bool) -> String {
        let latex = latex.trim();
        if latex.is_empty() {
            return String::new();
        }
        if let Ok(root)=crate::omml::from_latex(latex,display){
            self.math_fonts.extend(crate::math_layout::mathml_fonts(&root));
            self.math_font_warnings|=!crate::math_layout::font_warnings(&root).is_empty();
        }
        match crate::omml::latex_to_mathml(latex, display) {
            Some(m) => {
                self.has_math = true;
                if display {
                    format!("<div class=\"math-display\">{m}</div>")
                } else {
                    m
                }
            }
            None => {
                let cls = if display { "math-display math-src" } else { "math-src" };
                format!("<span class=\"{cls}\">{}</span>", esc(latex))
            }
        }
    }

    fn code(&self, lang: &str, text: &str) -> String {
        let mut body = String::new();
        for (kind, tok) in crate::code_highlight::tokenize(text, lang) {
            let cls = match kind {
                crate::code_highlight::Kind::Plain => None,
                crate::code_highlight::Kind::Keyword => Some("k"),
                crate::code_highlight::Kind::String => Some("s"),
                crate::code_highlight::Kind::Comment => Some("c"),
                crate::code_highlight::Kind::Number => Some("n"),
                crate::code_highlight::Kind::Type => Some("t"),
            };
            match cls {
                Some(c) => body.push_str(&format!("<span class=\"{c}\">{}</span>", esc(tok))),
                None => body.push_str(&esc(tok)),
            }
        }
        let label = if lang.is_empty() {
            String::new()
        } else {
            format!("<span class=\"code-lang\">{}</span>", esc(lang))
        };
        let cls = if lang.is_empty() { String::new() } else { format!(" class=\"language-{}\"", esc(lang)) };
        format!("<div class=\"code\">{label}<pre><code{cls}>{body}</code></pre></div>")
    }

    fn blocks(&mut self, blocks: &[Block], tight: bool) -> String {
        let mut out = String::new();
        for b in blocks {
            out.push_str(&self.block(b, tight));
            out.push('\n');
        }
        out
    }

    fn block(&mut self, b: &Block, tight: bool) -> String {
        match b {
            Block::Styled{attributes,block}=>{
                let style=crate::publication_blocks::Resolved::for_context(self.style,attributes,block,self.in_note);
                let mut html=self.block(block,tight);
                if let Some(end)=html.find('>'){html.insert_str(end,&format!(" style=\"{}\"",esc(&style.css())));}html
            }
            Block::Bibliography(bibliography) => {
                if bibliography.items.iter().any(|item|item.layout().map(|flow|crate::publication_rich::layout_requires_flow(&flow)).unwrap_or(true)){return self.rich_bibliography(bibliography);}
                let mut body = format!("<section role=\"doc-bibliography\" class=\"bibliography\"><h2>{}</h2>", esc(&bibliography.title));
                let columns = bibliography.items.iter().any(|item| !item.first_field.is_empty());
                if columns {
                    body.push_str(&format!("<table role=\"presentation\" class=\"bibliography-columns\" style=\"line-height:{}\"><tbody>", bibliography.line_spacing.max(1)));
                }
                for (index, item) in bibliography.items.iter().enumerate() {
                    let rich = self.inlines(&crate::md_ast::rich_inlines(&item.rich));
                    let gap = if index+1<bibliography.items.len() { bibliography.entry_gap_em() } else { 0 };
                    let hanging = if bibliography.hanging_indent { "padding-left:2em;text-indent:-2em;" } else { "" };
                    if columns {
                        body.push_str(&format!("<tr id=\"{}\" class=\"bibliography-entry\">", xml_id(&format!("bib-{}", item.key))));
                        if item.first_field.is_empty() {
                            body.push_str(&format!("<td colspan=\"2\" style=\"padding-bottom:{gap}em;{hanging}\">{rich}</td>"));
                        } else {
                            let first = self.inlines(&crate::md_ast::rich_inlines(&item.first_field));
                            body.push_str(&format!("<td class=\"bibliography-first-field\" style=\"padding-bottom:{gap}em\">{first}</td><td class=\"bibliography-content\" style=\"padding-bottom:{gap}em\">{rich}</td>"));
                        }
                        body.push_str("</tr>");
                    } else {
                        body.push_str(&format!("<div id=\"{}\" class=\"bibliography-entry\" style=\"line-height:{};margin-bottom:{gap}em;{hanging}\">{rich}</div>", xml_id(&format!("bib-{}", item.key)), bibliography.line_spacing.max(1)));
                    }
                }
                if columns { body.push_str("</tbody></table>"); }
                body.push_str("</section>"); body
            }
            Block::Heading { level, id, inlines } => {
                let lv = (*level).clamp(1, 6);
                format!("<h{lv} id=\"{}\">{}</h{lv}>", xml_id(id), self.inlines(inlines))
            }
            Block::Caption { label, inlines } => format!("<p class=\"publication-caption\" id=\"{}\">{}{}</p>", xml_id(&label.id), esc(&label.text()), self.inlines(inlines)),
            Block::Paragraph(inl) => {
                // A paragraph that is only an image becomes a figure.
                let meaningful: Vec<&Inline> = inl
                    .iter()
                    .filter(|i| !matches!(i, Inline::SoftBreak) && !matches!(i, Inline::Text(t) if t.trim().is_empty()))
                    .collect();
                if let [Inline::Image { alt, .. }] = meaningful.as_slice() {
                    let img = self.inlines(inl);
                    if img.starts_with("<img") {
                        let cap = if alt.is_empty() { String::new() } else { format!("<figcaption>{}</figcaption>", esc(alt)) };
                        return format!("<figure>{}{cap}</figure>", img.trim());
                    }
                }
                let body = self.inlines(inl);
                if tight {
                    body
                } else {
                    format!("<p>{body}</p>")
                }
            }
            Block::List { ordered, start, items } => {
                let tag = if *ordered { "ol" } else { "ul" };
                let start_attr = if *ordered && *start != 1 { format!(" start=\"{start}\"") } else { String::new() };
                let has_task = items.iter().any(|i| i.task.is_some());
                let cls = if has_task { " class=\"task-list\"" } else { "" };
                let mut s = format!("<{tag}{start_attr}{cls}>");
                for it in items {
                    let single = it.blocks.len() <= 2 && matches!(it.blocks.first(), Some(Block::Paragraph(_)));
                    let mark = match it.task {
                        Some(true) => "<span class=\"task done\">☑</span> ",
                        Some(false) => "<span class=\"task\">☐</span> ",
                        None => "",
                    };
                    s.push_str(&format!("<li>{mark}{}</li>", self.blocks(&it.blocks, single).trim_end()));
                }
                s.push_str(&format!("</{tag}>"));
                s
            }
            Block::Quote(inner) => format!("<blockquote>{}</blockquote>", self.blocks(inner, false)),
            Block::Code { lang, text } => self.code(lang, text),
            Block::Math(m) => self.math(m, true),
            Block::OfficeMath {latex,omml} => self.office_math(latex,omml,true),
            Block::Table { aligns, header, rows } => {
                let style = |ci: usize| -> String {
                    match aligns.get(ci).copied().unwrap_or(Align::None) {
                        Align::None => String::new(),
                        a => format!(" style=\"text-align:{}\"", a.as_str()),
                    }
                };
                let mut s = String::from("<table><thead><tr>");
                for (ci, c) in header.iter().enumerate() {
                    s.push_str(&format!("<th{}>{}</th>", style(ci), self.inlines(c)));
                }
                s.push_str("</tr></thead><tbody>");
                for r in rows {
                    s.push_str("<tr>");
                    for (ci, c) in r.iter().enumerate() {
                        s.push_str(&format!("<td{}>{}</td>", style(ci), self.inlines(c)));
                    }
                    s.push_str("</tr>");
                }
                s.push_str("</tbody></table>");
                s
            }
            Block::Hr => "<hr/>".to_string(),
            Block::PageBreak => "<div class=\"pagebreak\"></div>".to_string(),
            // Reflowable books retain the semantic target, not a pretend
            // physical paper/page-number implementation. The export entry
            // reports the print-layout projection limit separately.
            Block::SectionStart(section)=>format!("<span class=\"publication-section-boundary\" id=\"{}\"></span>",xml_id(&format!("readmd-section-{}",section.id))),
            Block::Html(h) => {
                if let Ok(Some(anchor))=crate::publication_navigation::anchor_html(h){return anchor;}
                let t = html_text(h);
                if t.trim().is_empty() {
                    String::new()
                } else {
                    format!("<p>{}</p>", esc(t.trim()))
                }
            }
            // Definitions are emitted as notes at the end of the chapter that cites them.
            Block::FootnoteDef { .. } => String::new(),
        }
    }

    fn notes(&mut self) -> String {
        if self.used_notes.is_empty() {
            return String::new();
        }
        // Render all definitions first: a later note can reference an earlier
        // one, so its backlink list is only final after this bounded walk.
        let mut bodies=Vec::new();
        let mut i = 0usize;
        // Notes may cite further notes; the list grows while we walk it.
        while i < self.used_notes.len() {
            let label = self.used_notes[i].clone();
            i += 1;
            let def = self.footnote_defs.get(&label).cloned().unwrap_or_default();
            self.in_note = true;
            let body = self.blocks(&def, false);
            self.in_note = false;
            bodies.push((label,body));
        }
        let end=self.note_file.is_some();
        let mut s = if end{format!("<section class=\"footnotes\" epub:type=\"endnotes\" role=\"doc-endnotes\" aria-label=\"{}\"><ol class=\"endnote-list\" role=\"list\">",esc(&crate::publication_layout::note_title(self.style)))}else{String::from("<section class=\"footnotes\" epub:type=\"footnotes\"><hr/>")};
        for (index,(label,body)) in bodies.into_iter().enumerate(){
            let i=index+1;
            let backs=self.note_refs.get(&label).into_iter().flatten().enumerate().map(|(at,id)|{
                let number=crate::publication_layout::note_number(self.style["footnotes"]["numberFormat"].as_str().unwrap_or("decimal"),crate::publication_layout::note_index(self.style,i));
                let text=if at==0{format!("{number}.")}else{format!("↩{}",at+1)};
                let href=if end{id.clone()}else{format!("#{id}")};
                format!("<a epub:type=\"backlink\" role=\"doc-backlink\" href=\"{href}\" class=\"fn-back\">{text}</a>")
            }).collect::<Vec<_>>().join(" ");
            let kind=if end{"endnote"}else{"footnote"};
            // DPUB-ARIA 1.1 deprecates doc-endnote; EPUB semantics remain
            // explicit, while native list items describe each entry.
            if end{s.push_str(&format!("<li epub:type=\"{kind}\" id=\"fn-{i}\">{backs} {body}</li>"));}
            else{s.push_str(&format!("<aside epub:type=\"{kind}\" role=\"doc-footnote\" id=\"fn-{i}\">{backs} {body}</aside>"));}
        }
        if end{s.push_str("</ol>");}s.push_str("</section>");
        s
    }
}

fn collect_defs(blocks: &[Block], into: &mut HashMap<String, Vec<Block>>) {
    for b in blocks {
        match b {
            Block::FootnoteDef { label, blocks } => {
                into.entry(label.clone()).or_insert_with(|| blocks.clone());
            }
            Block::Quote(inner) => collect_defs(inner, into),
            Block::List { items, .. } => {
                for it in items {
                    collect_defs(&it.blocks, into);
                }
            }
            _ => {}
        }
    }
}

/// Split the document into chapters at `h1` (or `h1`/`h2` for `split_level`
/// `"h2"`) and render each one.  Content before the first split heading
/// becomes a "序言" chapter; empty chapters are dropped.
pub fn render_chapters(doc: &Document, split_level: &str, images: ImageMap) -> Vec<Chapter> {
    render_chapters_with_options(doc,split_level,images,&crate::export_styles::default_style())
}
pub fn render_chapters_with_options(doc:&Document,split_level:&str,images:ImageMap,style:&serde_json::Value)->Vec<Chapter>{
    render_chapters_core(doc,split_level,images,style,None)
}
pub(crate) fn render_chapters_with_navigation(doc:&Document,split_level:&str,images:ImageMap,style:&serde_json::Value,front_count:usize,request:&crate::publication_navigation::Request)->crate::publication_navigation::Result<Vec<Chapter>>{
    crate::publication_navigation::validate_document(doc,request)?;
    if front_count>doc.blocks.len(){return Err(crate::publication_navigation::Error{code:crate::publication_navigation::INVALID,path:"epub.navigation.bodymatter".into(),from:0,to:0});}
    let first_body=doc.blocks[front_count..].iter().find(|b|!matches!(b.unstyled(),Block::Html(_)|Block::FootnoteDef{..}|Block::SectionStart(_)));
    if first_body.is_none()&&!request.anchors.is_empty(){return Err(crate::publication_navigation::Error{code:crate::publication_navigation::REFERENCE,path:"epub.navigation.bodymatter".into(),from:0,to:0});}
    Ok(render_chapters_core(doc,split_level,images,style,first_body))
}
fn navigation_boundary(block:&Block)->bool{
    matches!(block.unstyled(),Block::SectionStart(_))||matches!(block.unstyled(),Block::Html(raw) if crate::publication_navigation::parse_html(raw).is_ok_and(|v|!v.is_empty()))
}
fn render_chapters_core(doc:&Document,split_level:&str,images:ImageMap,style:&serde_json::Value,first_body:Option<&Block>)->Vec<Chapter>{
    let max_level: u8 = match split_level { "none" => 0, "h2" => 2, _ => 1 };
    let mut groups: Vec<(String, Vec<&Block>)> = Vec::new();
    let mut cur_title = "序言".to_string();
    let mut cur: Vec<&Block> = Vec::new();
    for b in &doc.blocks {
        if let Block::Heading { level, inlines, .. } = b.unstyled() {
            if *level <= max_level {
                let trailing=cur.iter().rposition(|block|!matches!(block.unstyled(),Block::FootnoteDef{..})&&!navigation_boundary(block)).map_or(0,|index|index+1);
                let mut boundaries=cur.split_off(trailing);
                boundaries.retain(|block|navigation_boundary(block));
                if cur.iter().any(|x| !matches!(x.unstyled(), Block::FootnoteDef { .. })&&!navigation_boundary(x)) {
                    groups.push((std::mem::take(&mut cur_title), std::mem::take(&mut cur)));
                }
                // Keep a leading boundary with its heading instead of
                // manufacturing an empty preface chapter or losing its ID.
                cur=boundaries;
                cur_title = inline_plain(inlines).trim().to_string();
            }
        }
        cur.push(b);
    }
    if cur.iter().any(|x| !matches!(x.unstyled(), Block::FootnoteDef { .. })&&!navigation_boundary(x)) {
        groups.push((cur_title, std::mem::take(&mut cur)));
    }

    // A final marker belongs to the last real chapter; never manufacture an
    // otherwise empty spine item solely for a static reference page.
    if !cur.is_empty() {if let Some((_,last))=groups.last_mut(){last.extend(cur);}}
    // Anchor → chapter file, so `[see](#intro)` works across chapters.
    let mut anchors: HashMap<String, String> = HashMap::new();
    for (n, (_, blocks)) in groups.iter().enumerate() {
        let file = format!("chapter_{}.xhtml", n + 1);
        fn inline_anchors(values:&[Inline],file:&str,anchors:&mut HashMap<String,String>){
            for value in values{match value{
                Inline::Html(raw)=>{if let Ok(items)=crate::publication_navigation::parse_html(raw){for item in items{anchors.entry(format!("readmd-epub-{}",item.id)).or_insert_with(||file.to_string());}}},
                Inline::Strong(c)|Inline::Emph(c)|Inline::Strike(c)|Inline::Styled{children:c,..}|Inline::Display{children:c,..}|Inline::Citation{children:c,..}|Inline::Link{children:c,..}=>inline_anchors(c,file,anchors),_=>(),
            }}
        }
        fn walk(b: &Block, file: &str, anchors: &mut HashMap<String, String>) {
            match b.unstyled(){Block::Paragraph(v)|Block::Heading{inlines:v,..}|Block::Caption{inlines:v,..}=>inline_anchors(v,file,anchors),_=>(),}
            match b.unstyled() {
                Block::Heading { id, .. } => {
                    anchors.entry(id.clone()).or_insert_with(|| file.to_string());
                }
                Block::Caption { label, .. } => { anchors.entry(label.id.clone()).or_insert_with(|| file.to_string()); }
                Block::Html(raw)=>{if let Ok(items)=crate::publication_navigation::parse_html(raw){for item in items{anchors.entry(format!("readmd-epub-{}",item.id)).or_insert_with(||file.to_string());}}},
                Block::SectionStart(section)=>{anchors.entry(format!("readmd-section-{}",section.id)).or_insert_with(||file.to_string());}
                Block::Quote(inner) => inner.iter().for_each(|x| walk(x, file, anchors)),
                Block::List { items, .. } => {
                    items.iter().flat_map(|i| i.blocks.iter()).for_each(|x| walk(x, file, anchors))
                }
                _ => {}
            }
        }
        for b in blocks {
            walk(b, &file, &mut anchors);
        }
    }
    let mut defs = HashMap::new();
    collect_defs(&doc.blocks, &mut defs);

    let mut out = Vec::new();let end=crate::publication_layout::notes_at_end(style);
    let note_file=end.then(||format!("chapter_{}.xhtml",groups.len()+1));
    let mut w = Writer { style, images: &mut *images, anchors: &anchors, footnote_defs: &defs, used_notes: Vec::new(), note_refs:HashMap::new(), note_file:note_file.clone(),file:String::new(),in_note:false, has_math: false, math_fonts:Default::default(), math_font_warnings:false };
    for (index,(title, blocks)) in groups.into_iter().enumerate() {
        w.file=format!("chapter_{}.xhtml",index+1);
        let mut body = String::new();
        let mut headings=Vec::new();
        for b in blocks {
            if first_body.is_some_and(|first|std::ptr::eq(first,b)){body.push_str(&format!("<span id=\"{}\" data-readmd-epub-anchor=\"bodymatter\"></span>",xml_id(crate::publication_navigation::BODY_ID)));}
            navigation_headings(b,&mut headings);
            body.push_str(&w.block(b, false));
            body.push('\n');
        }
        if !end{body.push_str(&w.notes());w.used_notes.clear();w.note_refs.clear();}
        let title = if title.is_empty() { "章节".to_string() } else { title };
        out.push(Chapter { title, body, headings, has_math: std::mem::take(&mut w.has_math), math_fonts:std::mem::take(&mut w.math_fonts), math_font_warnings:std::mem::take(&mut w.math_font_warnings) });
    }
    if end&&!w.used_notes.is_empty(){
        w.file=note_file.unwrap();let title=crate::publication_layout::note_title(style);let body=format!("<h1>{}</h1>{}",esc(&title),w.notes());
        out.push(Chapter {title,body,headings:Vec::new(),has_math:w.has_math,math_fonts:w.math_fonts,math_font_warnings:w.math_font_warnings});
    }
    out
}

/// Extra stylesheet rules for the constructs this writer emits.
/// Data tables must size against the current reflow column rather than their
/// unbroken cell text. Fixed layout still honours authored col/first-row widths;
/// no column width, font size or content is rewritten. CSL's presentation table
/// retains its shared intrinsic-width label column and nowrap label semantics.
pub(crate) const DATA_TABLE_REFLOW_CSS: &str = r#"
table:not(.bibliography-columns) { table-layout: fixed; max-width: 100%; }
table:not(.bibliography-columns) th, table:not(.bibliography-columns) td { word-wrap: break-word; overflow-wrap: anywhere; word-break: normal; }
table:not(.bibliography-columns) tr { break-inside: avoid; page-break-inside: avoid; }
"#;

/// Extra stylesheet rules for the constructs this writer emits.
pub const EXTRA_CSS: &str = r#"
figure { margin: 1.2em 0; text-align: center; }
figure img, p img { max-width: 100%; height: auto; }
.publication-caption { text-align: center; font-size: 0.9em; break-before: avoid; }
figcaption { font-size: 0.85em; color: inherit; margin-top: 0.4em; }
.bibliography-columns { width: 100%; border-collapse: collapse; border: 0; table-layout: auto; margin: 0; font: inherit; }
.bibliography-columns td { border: 0; padding: 0; vertical-align: baseline; }
.bibliography-columns .bibliography-first-field { width: 1%; white-space: nowrap; padding-right: .75em; }
.bibliography-columns .bibliography-content { overflow-wrap: anywhere; }
.bibliography-flow-columns, .readmd-csl-layout-columns { display:grid; grid-template-columns:fit-content(35%) minmax(0,1fr); column-gap:.75em; min-width:0; }
.readmd-csl-layout-left, .readmd-csl-layout-right { min-width:0; overflow-wrap:anywhere; }
.bibliography-flow-full { grid-column:1 / -1; min-width:0; }
.bibliography-flow-hanging { padding-left:2em; text-indent:-2em; }
.readmd-csl-layout-block { display:block; min-width:0; text-indent:0; }
.readmd-csl-layout-paragraph { min-width:0; text-indent:0; }
.readmd-csl-layout-paragraph.readmd-csl-layout-hanging { padding-inline-start:2em; text-indent:-2em; }
.readmd-csl-layout-indent { padding-left:4em; }
.code { margin: 1em 0; }
.code-lang { display: block; font-size: 0.72em; color: #94a3b8; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 0.2em; font-family: sans-serif; }
pre code { background: none; padding: 0; white-space: pre-wrap; word-wrap: break-word; }
p code, li code, td code { padding: 0.1em 0.3em; border-radius: 3px; }
.endnote-list { list-style: none; margin: 0; padding: 0; }
.endnote-list > li { margin: 0 0 1em; }
.k { color: #0033b3; font-weight: bold; }
.s { color: #067d17; }
.c { color: #8c8c8c; font-style: italic; }
.n { color: #1750eb; }
.t { color: #7a3e9d; }
ul.task-list { list-style: none; padding-left: 1.2em; }
.task { font-family: sans-serif; }
.math-display { text-align: center; margin: 1em 0; overflow-x: auto; }
.math-src { font-family: monospace; }
math[display="block"] { display: block; margin: 0.8em auto; }
.noteref { text-decoration: none; }
.footnotes { font-size: 0.88em; color: inherit; margin-top: 2em; }
.footnotes aside { margin: 0.4em 0; }
.fn-back { text-decoration: none; }
.pagebreak { page-break-after: always; break-after: page; }
.img-missing { color: #94a3b8; font-style: italic; }
del { color: inherit; }
"#;

#[cfg(test)]
#[path = "epub_table_reflow_tests.rs"]
mod table_reflow_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bibliography_display_columns_use_one_xhtml_table_and_additional_configured_line_heights() {
        use crate::publication_rich::{RichBibliography,RichBibliographyItem,RichDisplay,RichNode};
        fn descendants<'a>(node:&'a crate::math_document::XmlNode,name:&str,out:&mut Vec<&'a crate::math_document::XmlNode>){
            if node.local()==name {out.push(node);} for child in node.children(){descendants(child,name,out);}
        }
        let item=|key:&str,label:&str,body:&str|RichBibliographyItem{key:key.into(),first_field:vec![],rich:vec![
            RichNode::Group{children:vec![RichNode::text(label)],display:Some(RichDisplay::LeftMargin),entry_index:Some(1)},
            RichNode::Group{children:vec![RichNode::text(body)],display:Some(RichDisplay::RightInline),entry_index:Some(1)},
        ]};
        for line in [1,3] {for entry in [0,2] {
            let mut bibliography=RichBibliography{title:"References".into(),items:vec![item("short","[9]","First body"),item("wide","[100]","Second body"),RichBibliographyItem{key:"full".into(),first_field:vec![],rich:vec![RichNode::text("Full-width final entry")]}],hanging_indent:true,line_spacing:line,entry_spacing:entry,source_span:None};
            bibliography.normalize_layout().unwrap(); let gap=u64::from(line)*u64::from(entry);
            let doc=Document{front_matter:None,blocks:vec![Block::Bibliography(bibliography)]};
            let chapters=render_chapters(&doc,"none",&mut |_|None); assert_eq!(chapters.len(),1);
            let xml=crate::math_document::parse_xml(&chapters[0].body).unwrap();
            let mut tables=Vec::new();descendants(&xml,"table",&mut tables);assert_eq!(tables.len(),1,"Each row must share the same intrinsic-width table columns");
            assert_eq!(tables[0].attributes.get("role").unwrap(),"presentation");assert_eq!(tables[0].attributes.get("style").unwrap(),&format!("line-height:{line}"));
            let mut rows=Vec::new();descendants(tables[0],"tr",&mut rows);assert_eq!(rows.len(),3);
            for (index,row) in rows.iter().enumerate(){
                let cells=row.children().filter(|node|node.local()=="td").collect::<Vec<_>>();
                assert_eq!(cells.len(),if index<2{2}else{1}); let expected=if index<2{gap}else{0};
                for cell in &cells{assert!(cell.attributes.get("style").unwrap().contains(&format!("padding-bottom:{expected}em")));}
                if index<2 {
                    assert_eq!(cells[0].attributes.get("class").unwrap(),"bibliography-first-field");
                    assert_eq!(cells[0].text(),if index==0{"[9]"}else{"[100]"});
                    assert_eq!(cells[1].text(),if index==0{"First body"}else{"Second body"});
                    assert!(!cells.iter().any(|cell|cell.attributes.get("style").unwrap().contains("text-indent")));
                }else{assert_eq!(cells[0].attributes.get("colspan").unwrap(),"2");assert!(cells[0].attributes.get("style").unwrap().contains("text-indent:-2em"));}
            }
            assert!(!chapters[0].body.contains("grid-template-columns"));
            assert!(EXTRA_CSS.contains("white-space: nowrap")&&EXTRA_CSS.contains("padding-right: .75em"));
        }}
    }
    #[test]
    fn unpaired_bibliography_display_remains_full_width_and_has_no_final_entry_gap() {
        use crate::publication_rich::{RichBibliography,RichBibliographyItem,RichDisplay,RichNode};
        let mut bibliography=RichBibliography{title:"References".into(),items:[RichDisplay::LeftMargin,RichDisplay::RightInline].into_iter().enumerate().map(|(index,display)|RichBibliographyItem{key:index.to_string(),first_field:vec![],rich:vec![RichNode::Group{children:vec![RichNode::text("Full width")],display:Some(display),entry_index:None}]}).collect(),hanging_indent:false,line_spacing:3,entry_spacing:2,source_span:None};
        bibliography.normalize_layout().unwrap();
        let doc=Document{front_matter:None,blocks:vec![Block::Bibliography(bibliography)]};let chapters=render_chapters(&doc,"none",&mut |_|None);let body=&chapters[0].body;
        assert!(!body.contains("<table")&&!body.contains("display:inline"));
        assert_eq!(body.matches("display:block;").count(),2);
        assert_eq!(body.matches("margin-bottom:6em;").count(),1);assert_eq!(body.matches("margin-bottom:0em;").count(),1);
    }
    #[test]fn heading_css_preserves_legacy_defaults_and_applies_explicit_pagination_per_level(){
        let old=profile_css(&serde_json::json!({}));assert!(!old.contains("break-after:")&&!old.contains("break-inside:"));
        let options=serde_json::json!({"headings":{"h2":{"before":18,"after":7,"pageBreakBefore":true,"keepNext":false,"keepLines":false},"h3":{"keepNext":true,"keepLines":true}}});
        let css=profile_css(&options);assert!(css.contains("margin-top:18.0pt;margin-bottom:7.0pt"));
        for value in ["h2{break-after:auto;page-break-after:auto}","h2{break-inside:auto;page-break-inside:auto}","h3{break-after:avoid;page-break-after:avoid}","h3{break-inside:avoid;page-break-inside:avoid}","break-before:page;page-break-before:always;"]{assert!(css.contains(value),"{value}");}
    }

    #[test]
    fn mathml_script_spacing_warning_tracks_actual_office_semantics_and_deduplicates(){
        let formula=|properties:&str|{
            let xml=format!("<m:oMath><m:sSubSup>{properties}<m:e><m:r><m:t>f</m:t></m:r></m:e><m:sub><m:r><m:t>2</m:t></m:r></m:sub><m:sup><m:r><m:t>3</m:t></m:r></m:sup></m:sSubSup></m:oMath>");
            crate::math_migration::readable(&xml).unwrap()
        };
        for properties in ["","<m:sSubSupPr><m:alnScr m:val=\"0\"/></m:sSubSupPr>"]{
            let dsl=formula(properties);let source=format!("inline :math[{dsl}]\n\n```readmd-math v1\n{dsl}\n```");
            assert_eq!(mathml_alignment_warnings(&crate::md_ast::parse(&source)),vec![crate::api_codes::WARN_MATHML_SCRIPT_SPACING]);
        }
        for value in ["true","1","on"]{
            let source=format!("```readmd-math v1\n{}\n```",formula(&format!("<m:sSubSupPr><m:alnScr m:val=\"{value}\"/></m:sSubSupPr>")));
            assert!(mathml_alignment_warnings(&crate::md_ast::parse(&source)).is_empty());
        }
    }

    #[test]
    fn mathml_operator_alignment_loss_is_reported_in_nested_publication_content_once() {
        let source="```readmd-math v1\noffice.oMath(a, office.r(office.rPr(office.brk(alnAt: 1)), office.t(\"=b\")))\n```";
        let document=crate::md_ast::parse(source);assert_eq!(mathml_alignment_warnings(&document),vec![crate::api_codes::WARN_MATHML_ALIGNMENT]);
        let nested=Document{front_matter:None,blocks:vec![Block::Quote(document.blocks.clone()),Block::FootnoteDef{label:"n".into(),blocks:document.blocks}]};
        assert_eq!(mathml_alignment_warnings(&nested).len(),1);
        let output=render_chapters(&nested,"h1",&mut |_|None).into_iter().map(|c|c.body).collect::<String>();assert!(output.contains("<mtable"));assert!(output.contains("data-readmd-align-at=\"1\""));
        assert!(mathml_alignment_warnings(&crate::md_ast::parse("```readmd-math v1\nfrac(a,b)\n```")).is_empty());
    }

    fn render(md: &str) -> Vec<Chapter> {
        let doc = crate::md_ast::parse(md);
        let mut none = |_: &str| -> Option<String> { None };
        render_chapters(&doc, "h1", &mut none)
    }

    #[test]
    fn chapters_split_at_h1_with_preface() {
        let ch = render("intro\n\n# A\n\ntext\n\n## sub\n\n# B\n\nmore\n");
        let titles: Vec<&str> = ch.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["序言", "A", "B"]);
        assert!(ch[1].body.contains("<h2 id=\"h-sub\">sub</h2>"));
    }

    #[test]
    fn configured_note_start_changes_labels_and_never_backlink_targets() {
        let doc=crate::md_ast::parse("# A\n\nBody[^a] repeated[^a] then[^b].\n\n[^a]: A.\n\n[^b]: B.\n");
        for position in ["page","document"] {
            let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":position,"start":27,"numberFormat":"upperLetter"}}));
            let chapters=render_chapters_with_options(&doc,"h1",&mut |_|None,&style);
            assert_eq!(chapters[0].body.matches("<sup>AA</sup>").count(),2);
            assert_eq!(chapters[0].body.matches("<sup>AB</sup>").count(),1);
            let notes=&chapters.last().unwrap().body;
            assert!(notes.contains("id=\"fn-1\"")&&notes.contains("id=\"fn-2\""));
            assert!(notes.contains(">AA.</a>")&&notes.contains(">AB.</a>"));
            assert!(notes.contains("#fnref-1-2\""));assert!(!notes.contains("fn-27"));
        }
    }
    #[test]
    fn document_endnotes_share_definitions_across_chapters_and_resolve_all_backlinks() {
        let source="# First\n\nBody[^a] and again[^a].\n\n# Last\n\nLater[^a] and other[^b].\n\n[^a]: Shared note $x^2$.\n\n    > Quote.\n\n[^b]: Last note references[^a].\n";
        let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":"document","title":"Research & notes","numberFormat":"lowerRoman","size":8}}));
        let chapters=render_chapters_with_options(&crate::md_ast::parse(source),"h1",&mut |_|None,&style);assert_eq!(chapters.len(),3);assert_eq!(chapters[2].title,"Research & notes");
        assert!(chapters[..2].iter().all(|c|!c.body.contains("Shared note")&&!c.has_math));let last=&chapters[2];assert!(last.has_math);assert!(last.body.contains("epub:type=\"endnotes\""));assert_eq!(last.body.matches("epub:type=\"endnote\"").count(),2);assert_eq!(last.body.matches("Shared note").count(),1);
        assert!(chapters[0].body.contains("href=\"chapter_3.xhtml#fn-1\""));assert!(chapters[1].body.contains("href=\"chapter_3.xhtml#fn-1\""));assert!(chapters[1].body.contains("<sup>ii</sup>"));
        for href in ["chapter_1.xhtml#fnref-1","chapter_1.xhtml#fnref-1-2","chapter_2.xhtml#fnref-1-3","chapter_2.xhtml#fnref-2","chapter_3.xhtml#fnref-1-4"]{assert!(last.body.contains(&format!("href=\"{href}\"")),"missing backlink {href}");}
        assert!(last.body.contains("<blockquote>")&&last.body.contains("<math"));assert!(last.body.contains("Research &amp; notes"));
    }

    #[test]
    fn structure_survives() {
        let ch = render("# T\n\n3. a\n4. b\n   - [x] done\n\n| l | r |\n|:--|--:|\n| 1 | 2 |\n\n```rust\nlet x = 1;\n```\n");
        let b = &ch[0].body;
        assert!(b.contains("<ol start=\"3\">"), "{b}");
        assert!(b.contains("class=\"task-list\"") && b.contains("☑"), "{b}");
        assert!(b.contains("style=\"text-align:right\""), "{b}");
        assert!(b.contains("<span class=\"k\">let</span>"), "{b}");
    }

    #[test]
    fn math_becomes_mathml() {
        let ch = render("# M\n\nInline $x^2$.\n\n$$\n\\frac{a}{b}\n$$\n");
        assert!(ch[0].has_math);
        assert!(ch[0].body.contains("<msup>") && ch[0].body.contains("<mfrac>"), "{}", ch[0].body);
    }

    #[test]
    fn footnotes_are_epub_notes() {
        let ch = render("# F\n\nText[^a].\n\n[^a]: The note.\n");
        let b = &ch[0].body;
        assert!(b.contains("epub:type=\"noteref\""), "{b}");
        assert!(b.contains("epub:type=\"footnote\"") && b.contains("The note."), "{b}");
    }

    #[test]
    fn toc_navigation_preserves_hierarchy_skips_depth_and_targets_real_xhtml(){
        let doc=crate::md_ast::parse("# First\n\n### Skipped level\n\n## Sibling & detail\n\n###### Deep\n\n# Second\n\n## Sibling & detail\n");
        let chapters=render_chapters(&doc,"h1",&mut |_|None);
        let entries=navigation(&chapters,3);let nav=navigation_xhtml(&entries);
        assert_eq!(entries.len(),2);assert_eq!(entries[0].children.len(),2);
        assert!(!nav.contains(">Deep<"));assert!(nav.contains("Sibling &amp; detail"));
        assert_eq!(nav.matches("<li>").count(),5);assert_eq!(nav.matches("<ol>").count(),2);
        for (index,root) in entries.iter().enumerate(){
            let (file,id)=root.href.split_once('#').unwrap();assert_eq!(file,format!("chapter_{}.xhtml",index+1));
            assert!(chapters[index].body.contains(&format!("id=\"{id}\"")));
            for item in &root.children{
            let (file,id)=item.href.split_once('#').unwrap();let index=if file=="chapter_1.xhtml"{0}else{1};
            assert!(chapters[index].body.contains(&format!("id=\"{id}\"")));
        }}
        assert_ne!(entries[0].children[1].href,entries[1].children[0].href);
        let (ncx,depth)=navigation_ncx(&entries);assert_eq!(depth,2);assert_eq!(ncx.matches("<navPoint ").count(),5);
        assert!(ncx.contains("playOrder=\"5\""));
        assert_eq!(navigation_xhtml(&navigation(&chapters,1)).matches("<li>").count(),2);
        assert!(navigation_xhtml(&navigation(&chapters,6)).contains(">Deep<"));
        let single=render_chapters(&doc,"none",&mut |_|None);
        let single_nav=navigation_xhtml(&navigation(&single,3));
        assert!(single_nav.contains("chapter_1.xhtml#h-first")&&single_nav.contains("chapter_1.xhtml#h-second"));
        assert!(!single_nav.contains("chapter_2.xhtml"));
    }
    #[test]
    fn no_split_keeps_all_headings_links_and_notes_in_one_chapter() {
        let doc=crate::md_ast::parse("Introduction.\n\n# One\n\nA[^n] and [two](#two).\n\n## Subsection\n\n# Two\n\nB[^n].\n\n[^n]: Source note.\n");
        let mut images=|_:&str|None;
        let chapters=render_chapters(&doc,"none",&mut images);
        assert_eq!(chapters.len(),1);
        let body=&chapters[0].body;
        assert!(body.contains("Introduction.")&&body.contains("Subsection")&&body.contains("B<"));
        assert!(body.contains("href=\"chapter_1.xhtml#h-two\""));
        assert!(body.contains("id=\"fnref-1-2\"")&&body.contains("href=\"#fnref-1-2\""));
    }

    #[test]
    fn repeated_cyclic_and_cross_chapter_notes_have_unique_targets_and_all_backlinks() {
        let chapters=render("# One\n\nFirst[^a] and again[^a].\n\n# Two\n\nSecond[^a] and other[^b].\n\n[^a]: Note A with nested[^b].\n\n[^b]: Note B refers back[^a].\n");
        assert_eq!(chapters.len(),2);
        for chapter in chapters {
            let source=format!(r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">{}</html>"#,chapter.body);
            let doc=crate::math_document::parse_xml(&source).unwrap();
            fn visit(node:&crate::math_document::XmlNode,ids:&mut std::collections::HashSet<String>,hrefs:&mut Vec<String>){
                if let Some(id)=node.attributes.get("id"){assert!(ids.insert(id.clone()),"duplicate {id}");}
                if let Some(href)=node.attributes.get("href"){hrefs.push(href.clone());}
                for part in &node.content{if let crate::math_document::XmlPart::Element(child)=part{visit(child,ids,hrefs);}}
            }
            let mut ids=std::collections::HashSet::new();let mut hrefs=Vec::new();visit(&doc,&mut ids,&mut hrefs);
            assert!(hrefs.iter().all(|h|h.strip_prefix('#').is_none_or(|id|ids.contains(id))));
            assert_eq!(chapter.body.matches("role=\"doc-noteref\"").count(),chapter.body.matches("role=\"doc-backlink\"").count());
            assert_eq!(chapter.body.matches("epub:type=\"footnote\"").count(),2);
        }
    }

    #[test]
    fn raw_html_is_reduced_to_text_and_links_are_filtered() {
        let ch = render("# H\n\n<div onclick=\"x()\">hi <b>there</b></div>\n\n[x](javascript:alert(1)) <span>s</span>\n");
        let b = &ch[0].body;
        assert!(!b.contains("onclick") && !b.contains("<div onclick"), "{b}");
        assert!(b.contains("hi there"), "{b}");
        assert!(!b.contains("javascript:"), "{b}");
    }

    #[test]
    fn cross_chapter_anchor_links() {
        let ch = render("# One\n\nSee [two](#two).\n\n# Two\n\nhere\n");
        assert!(ch[0].body.contains("href=\"chapter_2.xhtml#h-two\""), "{}", ch[0].body);
    }

    #[test]
    fn images_use_the_package_map() {
        let doc = crate::md_ast::parse("# I\n\n![cap](a.png)\n");
        let mut map = |s: &str| -> Option<String> { (s == "a.png").then(|| "images/img_1.png".to_string()) };
        let ch = render_chapters(&doc, "h1", &mut map);
        assert!(ch[0].body.contains("<figure><img src=\"images/img_1.png\" alt=\"cap\"/><figcaption>cap</figcaption></figure>"), "{}", ch[0].body);
    }
}
