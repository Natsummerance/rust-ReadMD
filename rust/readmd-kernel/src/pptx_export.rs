//! Revision-bound Markdown -> editable PresentationML, without a host Office.
//! One bounded deck model owns pagination and geometry for both preview and ZIP.
use crate::md_ast::{Align, Block, Inline};
use crate::mdexport::{ExportResult, ZipEntry};
use base64::Engine;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

const W: f64 = 960.;
const H: f64 = 540.;
const LEFT: f64 = 54.;
const WIDTH: f64 = W - 2. * LEFT;
const TOP: f64 = 112.;
const BOTTOM: f64 = 492.;
const LIMIT: usize = crate::export_snapshot::MAX_SNAPSHOT_BYTES;
const BUDGET: &str = "export_snapshot_budget";
const INVALID: &str = "export_snapshot_render_failed";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
type Result<T> = std::result::Result<T, String>;
type Rect = [f64; 4];

#[derive(Clone, Debug, Default, PartialEq)]
struct Run { text: String, bold: bool, italic: bool, strike: bool, code: bool, href: Option<String> }
type Line = Vec<Run>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableAlign { Left, Center, Right, Justify }
impl TableAlign {
    fn option(value:&str) -> Self {match value {"center"=>Self::Center,"right"=>Self::Right,"justify"=>Self::Justify,_=>Self::Left}}
    fn column(value:Align, fallback:Self) -> Self {match value {Align::Left=>Self::Left,Align::Center=>Self::Center,Align::Right=>Self::Right,Align::None=>fallback}}
    fn css(self) -> &'static str {match self {Self::Left=>"left",Self::Center=>"center",Self::Right=>"right",Self::Justify=>"justify"}}
    fn xml(self) -> &'static str {match self {Self::Left=>"l",Self::Center=>"ctr",Self::Right=>"r",Self::Justify=>"just"}}
}
#[derive(Clone, Debug)]
struct TableStyle { three_line:bool, border_color:String, border_width:f64, default_align:TableAlign }
#[derive(Clone, Copy, Debug, PartialEq)]
struct TableBorders { left:f64, right:f64, top:f64, bottom:f64 }
impl TableStyle {
    fn from_options(options:&Value) -> Self {
        let style=crate::export_styles::sanitize(options);
        let three_line=crate::export_styles::three_line_table(&style);
        let border_color=style["table"]["borderColor"].as_str().unwrap_or("#c8cdd4").trim().trim_start_matches('#').to_ascii_uppercase();
        let width=style["table"]["borderWidth"].as_f64().unwrap_or(0.75);
        Self{three_line,border_color,border_width:if three_line{width.clamp(0.25,3.)}else{width},default_align:TableAlign::option(style["table"]["align"].as_str().unwrap_or("left"))}
    }
    fn borders(&self, row:usize, rows:usize) -> TableBorders {
        let width=self.border_width;
        if !self.three_line {return TableBorders{left:width,right:width,top:width,bottom:width};}
        TableBorders{left:0.,right:0.,top:if row==0{width*2.}else{0.},bottom:if row+1==rows{width*2.}else if row==0{width}else{0.}}
    }
    fn border_css(&self, borders:TableBorders) -> String {
        [("left",borders.left),("right",borders.right),("top",borders.top),("bottom",borders.bottom)].iter().map(|(side,width)|{
            if *width==0.{format!("border-{side}:0;")}else{format!("border-{side}:{width}pt solid #{};",self.border_color)}
        }).collect()
    }
    fn border_xml(&self, borders:TableBorders) -> String {
        [("L",borders.left),("R",borders.right),("T",borders.top),("B",borders.bottom)].iter().map(|(side,width)|{
            if *width==0.{format!("<a:ln{side}><a:noFill/></a:ln{side}>")}else{format!("<a:ln{side} w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln{side}>",emu(*width),self.border_color)}
        }).collect()
    }
}
#[derive(Clone, Debug)]
enum Shape {
    Text { rect: Rect, lines: Vec<Line>, size: f64, bullet: Option<String>, center: bool, code: bool },
    Image { rect: Rect, media: usize, alt: String },
    Accent { rect: Rect, color: &'static str, rounded: bool },
    Table { rect: Rect, rows: Vec<Vec<Vec<Line>>>, heights: Vec<f64>, columns: usize, aligns:Vec<TableAlign>, style:TableStyle },
}
#[derive(Default)]
struct Slide { shapes: Vec<Shape>, notes: String, cover: bool }
struct Media { bytes: Vec<u8>, extension: String, width: f64, height: f64, svg: Option<Vec<u8>> }
#[derive(Clone, Copy)]
enum Layout { Plain, Defense, Lab, Launch }

impl Layout {
    fn resolve(options: &Value) -> Result<Self> {
        Ok(match options["pptxLayout"].as_str().unwrap_or("") {
            "" => Self::Plain,
            "defense" => Self::Defense,
            "lab" => Self::Lab,
            "launch" => Self::Launch,
            _ => return Err(INVALID.into()),
        })
    }

    fn accent_shapes(self, cover: bool) -> Vec<Shape> {
        let accent = |rect, color, rounded| Shape::Accent { rect, color, rounded };
        match (self, cover) {
            (Self::Plain, _) => Vec::new(),
            (Self::Defense, true) => vec![accent([0., 0., 10., H], "365BB0", false), accent([84., 78., 88., 4.], "365BB0", false)],
            (Self::Defense, false) => vec![accent([LEFT, 26., 78., 4.], "365BB0", false), accent([LEFT, 512., WIDTH, 1.], "CBD8EC", false)],
            (Self::Lab, true) => vec![accent([104., 54., 752., 432.], "E6F4EE", true), accent([432., 70., 96., 4.], "107E71", true)],
            (Self::Lab, false) => vec![accent([35., 29., 4., 70.], "107E71", true), accent([LEFT, 512., WIDTH, 2.], "B7DED1", false)],
            (Self::Launch, true) => vec![accent([0., 0., W, 7.], "A7BAFF", false), accent([84., 76., 104., 4.], "A7BAFF", true), accent([0., 504., W, 36.], "202943", false)],
            (Self::Launch, false) => vec![accent([0., 0., W, 7.], "A7BAFF", false), accent([LEFT, 513., WIDTH, 1.], "455574", false)],
        }
    }
}

struct Deck { slides: Vec<Slide>, media: Vec<Media>, warnings: Vec<String>, background: &'static str, foreground: &'static str, accent: &'static str, transition: String, layout: Layout }

fn escaped(value: &str) -> String {
    crate::mdexport::html_escape(&value.chars().filter(|&c| c=='\t'||c=='\n'||c=='\r'||c>=' ').collect::<String>(), true)
}
fn check() -> Result<()> { crate::publication_budget::check() }
fn text(value: impl Into<String>) -> Line { vec![Run { text: value.into(), ..Default::default() }] }
#[cfg(test)]
fn plain(values: &[Run]) -> String { values.iter().map(|v| v.text.as_str()).collect() }
fn comments_only(mut raw:&str)->bool {
    loop {raw=raw.trim();if raw.is_empty(){return true;}let Some(body)=raw.strip_prefix("<!--")else{return false;};let Some((_,tail))=body.split_once("-->")else{return false;};raw=tail;}
}

fn inline_runs(values: &[Inline], inherited: &Run, output: &mut Line) -> Result<()> {
    for value in values {
        check()?;
        let mut style = inherited.clone(); style.text.clear();
        match value {
            Inline::Text(t) => { style.text=t.clone(); output.push(style); },
            Inline::Code(t) => { style.text=t.clone(); style.code=true; output.push(style); },
            Inline::Strong(children) => { style.bold=true; inline_runs(children,&style,output)?; },
            Inline::Emph(children) => { style.italic=true; inline_runs(children,&style,output)?; },
            Inline::Strike(children) => { style.strike=true; inline_runs(children,&style,output)?; },
            Inline::Link{href,children,..} => { if safe_link(href) {style.href=Some(href.clone());} inline_runs(children,&style,output)?; },
            Inline::Citation{children,..} => inline_runs(children,&style,output)?,
            // Unsupported authored text axes must not silently disappear.
            Inline::Styled{..}|Inline::AuthorStyled{..}|Inline::Display{..} => return Err("academic_rich_unsupported".into()),
            Inline::SoftBreak => { style.text=" ".into(); output.push(style); },
            Inline::HardBreak => { style.text="\n".into(); output.push(style); },
            Inline::FootnoteRef(label) => {style.text=format!("[{label}]");output.push(style);},
            Inline::Html(raw) if comments_only(raw) => (),
            Inline::Math(_) | Inline::OfficeMath{..} | Inline::Image{..} | Inline::Html(_) => return Err(INVALID.into()),
        }
    }
    Ok(())
}
fn safe_link(value: &str) -> bool {
    !value.chars().any(char::is_control) && ["https://","http://","mailto:"].iter().any(|prefix|value.to_ascii_lowercase().starts_with(prefix))
}
fn advance(c: char, size: f64, code: bool) -> f64 {
    if matches!(c as u32,0x0300..=0x036f|0x200b..=0x200f|0xfe00..=0xfe0f) {return 0.;}
    if c=='\t' {return size*2.4;}
    if c as u32>=0x2e80 {return size;}
    if code {return size*0.62;}
    static FACE: std::sync::OnceLock<ttf_parser::Face<'static>> = std::sync::OnceLock::new();
    let face=FACE.get_or_init(||ttf_parser::Face::parse(crate::publication_fonts::CORE_TEXT_FALLBACK,0).expect("locked core font"));
    face.glyph_index(c).and_then(|id|face.glyph_hor_advance(id)).map(|width|size*width as f64/face.units_per_em() as f64).unwrap_or(size*0.62)
}
// Explicit shared line breaks keep the preview and editable text boxes aligned.
// A small metric reserve accommodates substitute Office fonts without clipping.
fn wrap(runs: &[Run], size: f64, width: f64) -> Result<Vec<Line>> {
    let mut lines = Vec::new();
    let mut line = Line::new();
    let mut used = 0.;
    let limit = width * 0.94;
    let mut glyphs = runs.iter().flat_map(|run| run.text.chars().map(move |c| (run, c)));
    let word_char = |c: char| !c.is_whitespace() && (c as u32) < 0x2e80;
    let glyph_width = |run: &Run, c| advance(c, size, run.code) * if run.bold { 1.06 } else { 1. };
    let mut within_word = false;
    let mut characters = 0usize;
    while let Some((run, c)) = glyphs.next() {
        characters += 1;
        if characters % 256 == 0 { check()?; }
        let next = glyph_width(run, c);
        let word = word_char(c);
        let mut whole_word = next;
        if word && !within_word && used + next <= limit && !line.is_empty() {
            for (i, (style, character)) in glyphs.clone().take_while(|&(_, ch)| word_char(ch)).enumerate() {
                if i % 256 == 0 { check()?; }
                whole_word += glyph_width(style, character);
                if whole_word > limit { break; }
            }
        }
        let move_word = whole_word <= limit && used + whole_word > limit;
        if c == '\n' || (!line.is_empty() && (used + next > limit || move_word)) {
            if lines.len() >= 5000 { return Err(BUDGET.into()); }
            lines.push(std::mem::take(&mut line));
            used = 0.;
        }
        within_word = word;
        if c == '\n' { continue; }
        if let Some(last) = line.last_mut().filter(|last| last.bold == run.bold && last.italic == run.italic
            && last.strike == run.strike && last.code == run.code && last.href == run.href) {
            last.text.push(c);
        } else {
            let mut chunk = run.clone();
            chunk.text = c.to_string();
            line.push(chunk);
        }
        used += next;
    }
    if !line.is_empty() || lines.is_empty() { lines.push(line); }
    Ok(lines)
}

struct Builder { deck: Deck, page: Slide, y: f64, title: Line, base: String, shapes: usize, bytes: usize, table_style:TableStyle }
impl Builder {
    fn new(base:&str, options:&Value) -> Result<Self> {
        let theme=options["theme"].as_str().unwrap_or("white");
        let layout = Layout::resolve(options)?;
        let legacy = match theme {
            "black"=>("191919","FFFFFF","74A9FF"),"league"=>("28313A","FFFFFF","80CFFF"),
            "night"=>("111111","F2EEE2","E7AD52"),"moon"=>("002B36","E5F2F4","52B4C8"),
            "blood"=>("222222","F7F2F1","D96F6F"),"beige"=>("F7F3DE","333333","785728"),
            "solarized"=>("FDF6E3","586E75","268BD2"),"sky"=>("EAF6FC","243645","176FB3"),
            "white"|"simple"|"serif"=>("FFFFFF","24292F","2169C5"),_=>return Err(INVALID.into()),
        };
        let (background,foreground,accent) = match layout {
            Layout::Plain => legacy,
            Layout::Defense => ("F9FBFF", "172846", "365BB0"),
            Layout::Lab => ("F8FBFA", "183E38", "107E71"),
            Layout::Launch => ("131B31", "F1F5FF", "A7BAFF"),
        };
        let transition=options["transition"].as_str().unwrap_or(if matches!(layout, Layout::Plain) {"slide"} else {"none"});
        if !matches!(transition,"slide"|"fade"|"zoom"|"none"){return Err(INVALID.into());}
        Ok(Self{deck:Deck{slides:Vec::new(),media:Vec::new(),warnings:Vec::new(),background,foreground,accent,transition:transition.into(),layout},page:Slide::default(),y:TOP,title:Vec::new(),base:base.into(),shapes:0,bytes:0,table_style:TableStyle::from_options(options)})
    }
    fn push(&mut self, shape:Shape) -> Result<()> {
        let text_bytes=match &shape {
            Shape::Text{lines,..}=>lines.iter().flatten().map(|run|run.text.len()+run.href.as_ref().map_or(0,String::len)+256).sum::<usize>(),
            Shape::Table{rows,..}=>rows.iter().flatten().flatten().flatten().map(|run|run.text.len()+run.href.as_ref().map_or(0,String::len)+256).sum(),
            Shape::Image{alt,..}=>alt.len()+512,
            Shape::Accent{..}=>128,
        };
        self.bytes=self.bytes.checked_add(text_bytes).ok_or(BUDGET)?;
        self.shapes+=1;if self.shapes>30000||self.bytes>LIMIT{return Err(BUDGET.into());}check()?;self.page.shapes.push(shape);Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        if self.page.shapes.is_empty(){return Ok(());}
        if self.deck.slides.len()>=300{return Err(BUDGET.into());}
        self.bytes=self.bytes.checked_add(self.page.notes.len()).ok_or(BUDGET)?;if self.bytes>LIMIT{return Err(BUDGET.into());}
        let mut page = std::mem::take(&mut self.page);
        let mut accents = self.deck.layout.accent_shapes(page.cover);
        accents.append(&mut page.shapes);
        page.shapes = accents;
        self.deck.slides.push(page);self.y=TOP;Ok(())
    }
    fn heading(&mut self, title:Line) -> Result<()> {
        self.title=title.clone();let lines=wrap(&title,32.,WIDTH)?;
        if lines.len()>3{return Err(BUDGET.into());}
        let height=lines.len() as f64*39.;self.y=(48.+height+22.).max(TOP);
        self.push(Shape::Text{rect:[LEFT,42.,WIDTH,height+4.],lines,size:32.,bullet:None,center:false,code:false})
    }
    fn cover(&mut self,title:&Line,blocks:&[Block])->Result<bool>{
        fn ordinary(blocks:&[Block],out:&mut Vec<Line>)->Result<bool>{
            for block in blocks {match block {
                Block::Paragraph(values)=>{let mut runs=Vec::new();if inline_runs(values,&Run::default(),&mut runs).is_err(){return Ok(false);}out.push(runs);},
                Block::Quote(inner)=>if !ordinary(inner,out)?{return Ok(false);},
                _=>return Ok(false),
            }}Ok(true)
        }
        if !self.deck.slides.is_empty()||blocks.len()>2{return Ok(false);}
        let mut paragraphs=Vec::new();if !ordinary(blocks,&mut paragraphs)?{return Ok(false);}
        let (width, title_size, body_size, centered) = match self.deck.layout {
            Layout::Plain => (WIDTH, 44., 24., true),
            Layout::Defense => (704., 47., 22., false),
            Layout::Lab => (660., 45., 22., true),
            Layout::Launch => (736., 57., 23., false),
        };
        let title_lines = wrap(title, title_size, width)?;
        if title_lines.len() > 2 { return Ok(false); }
        let mut body = Vec::new();
        for values in paragraphs {
            if !body.is_empty() { body.push(Vec::new()); }
            body.extend(wrap(&values, body_size, width)?);
        }
        if body.len() > 5 { return Ok(false); }
        let title_height = title_lines.len() as f64 * title_size * 1.25 + 8.;
        let body_height = body.len() as f64 * body_size * 1.25 + 8.;
        let gap = if body.is_empty() { 0. } else { 22. };
        let y = (H - title_height - body_height - gap) / 2.;
        let x = if centered { (W - width) / 2. } else { 84. };
        self.title = title.clone();
        self.page.cover = true;
        self.push(Shape::Text { rect: [x, y, width, title_height], lines: title_lines,
            size: title_size, bullet: None, center: centered, code: false })?;
        if !body.is_empty() {
            self.push(Shape::Text { rect: [x, y + title_height + gap, width, body_height],
                lines: body, size: body_size, bullet: None, center: centered, code: false })?;
        }
        Ok(true)
    }
    fn continuation(&mut self) -> Result<()> {let notes=self.page.notes.clone();self.finish()?;self.page.notes=notes;self.heading(self.title.clone())}
    fn paragraph(&mut self, values:Line,size:f64,bullet:Option<String>,code:bool) -> Result<()> {
        let indent=if bullet.is_some(){26.}else{0.};let lines=wrap(&values,size,WIDTH-indent-if code{20.}else{0.})?;
        let mut start=0;let line_height=size*1.25;
        while start<lines.len() {
            check()?;let capacity=((BOTTOM-self.y-12.)/line_height).floor().max(0.) as usize;
            if capacity==0 {self.continuation()?;continue;}
            let end=(start+capacity).min(lines.len());let height=(end-start) as f64*line_height+8.;
            self.push(Shape::Text{rect:[LEFT+indent,self.y,WIDTH-indent,height],lines:lines[start..end].to_vec(),size,bullet:if start==0{bullet.clone()}else{None},center:false,code})?;
            self.y+=height+10.;start=end;if start<lines.len(){self.continuation()?;}
        }
        Ok(())
    }
    fn add_media(&mut self,bytes:Vec<u8>,extension:String) -> Result<usize> {
        if bytes.len()>32*1024*1024{return Err(BUDGET.into());}
        if let Some(index)=self.deck.media.iter().position(|media|(media.extension==extension&&media.bytes==bytes)||(extension=="svg"&&media.svg.as_deref()==Some(bytes.as_slice()))){return Ok(index);}
        let svg=(extension=="svg").then(||bytes.clone());
        let raster=if extension=="svg" {crate::svg_render::rasterize(&bytes).map_err(str::to_string)?} else {bytes};
        let extension=if extension=="svg" {"png".into()}else{extension};
        let mut reader=image::ImageReader::new(std::io::Cursor::new(&raster)).with_guessed_format().map_err(|_|INVALID)?;
        let mut limits=image::Limits::default();limits.max_image_width=Some(12000);limits.max_image_height=Some(12000);limits.max_alloc=Some(128*1024*1024);reader.limits(limits);
        let image=reader.decode().map_err(|_|INVALID)?;
        let (bytes,extension)=if matches!(extension.as_str(),"png"|"jpg"){(raster,extension)}else{
            let mut buffer=std::io::Cursor::new(Vec::new());image.write_to(&mut buffer,image::ImageFormat::Png).map_err(|_|INVALID)?;(buffer.into_inner(),"png".into())
        };
        self.bytes=self.bytes.checked_add(bytes.len()+svg.as_ref().map_or(0,Vec::len)).ok_or(BUDGET)?;if self.bytes>LIMIT{return Err(BUDGET.into());}
        let id=self.deck.media.len();self.deck.media.push(Media{bytes,extension,width:image.width() as f64,height:image.height() as f64,svg});Ok(id)
    }
    fn image(&mut self,src:&str,alt:&str) -> Result<()> {
        let (bytes,extension)=crate::mdexport::publication_image(&self.base,src,&mut self.deck.warnings)?.ok_or("export_snapshot_resource_missing")?;
        let media=self.add_media(bytes,extension)?;self.image_id(media,alt)
    }
    fn image_id(&mut self,media:usize,alt:&str) -> Result<()> {
        if BOTTOM-self.y<160. {self.continuation()?;}
        let image=&self.deck.media[media];let scale=(WIDTH/image.width).min((BOTTOM-self.y-8.)/image.height);
        let width=image.width*scale;let height=image.height*scale;
        self.push(Shape::Image{rect:[LEFT+(WIDTH-width)/2.,self.y,width,height],media,alt:alt.into()})?;self.y+=height+16.;Ok(())
    }
    fn math(&mut self,latex:&str,omml:Option<&str>) -> Result<()> {
        let formula=crate::math_layout::layout_source_constrained(latex,omml,true,28.,Some(WIDTH)).ok_or("math_unsupported_extension")?;
        let natural_width=formula.width.max(1.);let natural_height=(formula.height+formula.depth).max(1.);
        let svg=formula.svg(&format!("#{}",self.deck.foreground));let media=self.add_media(svg.into_bytes(),"svg".into())?;
        if BOTTOM-self.y<natural_height.min(200.)+16. {self.continuation()?;}
        let scale=(WIDTH/natural_width).min((BOTTOM-self.y-16.)/natural_height).min(1.);
        let width=natural_width*scale;let height=natural_height*scale;
        self.page.notes.push_str(&format!("\n\n$$\n{latex}\n$$"));
        if !self.deck.warnings.iter().any(|value|value=="pptx_math_image"){self.deck.warnings.push("pptx_math_image".into());}
        self.push(Shape::Image{rect:[LEFT+(WIDTH-width)/2.,self.y,width,height],media,alt:latex.into()})?;self.y+=height+18.;Ok(())
    }
    fn inline_paragraph(&mut self,values:&[Inline],size:f64,bullet:Option<String>) -> Result<()> {
        let mut pending=Vec::new();
        for inline in values {
            match inline {
                Inline::Image{src,alt,..}=>{if !pending.is_empty(){self.paragraph(std::mem::take(&mut pending),size,bullet.clone(),false)?;}self.image(src,alt)?;},
                Inline::Math(latex)|Inline::OfficeMath{latex,..}=>{
                    if !pending.is_empty(){self.paragraph(std::mem::take(&mut pending),size,bullet.clone(),false)?;}
                    let xml=if let Inline::OfficeMath{omml,..}=inline {Some(omml.as_str())}else{None};self.math(latex,xml)?;
                },
                _=>inline_runs(std::slice::from_ref(inline),&Run::default(),&mut pending)?,
            }
        }
        if !pending.is_empty(){self.paragraph(pending,size,bullet,false)?;}Ok(())
    }
    fn table(&mut self,aligns:&[Align],header:&[Vec<Inline>],rows:&[Vec<Vec<Inline>>]) -> Result<()> {
        let columns=header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns==0{return Ok(());}if columns>16{return Err(BUDGET.into());}
        let aligns=(0..columns).map(|column|TableAlign::column(aligns.get(column).copied().unwrap_or(Align::None),self.table_style.default_align)).collect::<Vec<_>>();
        let cell_width=WIDTH/columns as f64-20.;
        let cell=|values:&[Vec<Inline>]| -> Result<Vec<Vec<Line>>> {
            let mut output=Vec::new();for col in 0..columns{let mut runs=Vec::new();if let Some(values)=values.get(col){inline_runs(values,&Run::default(),&mut runs)?;}output.push(wrap(&runs,20.,cell_width)?);}Ok(output)
        };
        let header=cell(header)?;let header_height=header.iter().map(Vec::len).max().unwrap_or(1) as f64*25.+16.;
        if header_height>120.{return Err(BUDGET.into());}
        if BOTTOM-self.y<header_height+42. {self.continuation()?;}
        let mut table_rows=vec![header.clone()];let mut heights=vec![header_height];let mut used=header_height;
        for source in rows {
            let cells=cell(source)?;let total=cells.iter().map(Vec::len).max().unwrap_or(1);let mut start=0;
            while start<total {
                if BOTTOM-self.y-used<42. {
                    if table_rows.len()>1 {self.push(Shape::Table{rect:[LEFT,self.y,WIDTH,used],rows:std::mem::replace(&mut table_rows,vec![header.clone()]),heights:std::mem::replace(&mut heights,vec![header_height]),columns,aligns:aligns.clone(),style:self.table_style.clone()})?;}
                    self.continuation()?;used=header_height;
                }
                let capacity=((BOTTOM-self.y-used-16.)/25.).floor().max(0.) as usize;
                if capacity==0{return Err(BUDGET.into());}
                let end=(start+capacity).min(total);let height=(end-start) as f64*25.+16.;
                table_rows.push(cells.iter().map(|lines|lines[start.min(lines.len())..end.min(lines.len())].to_vec()).collect());heights.push(height);used+=height;start=end;
            }
        }
        if !table_rows.is_empty(){self.push(Shape::Table{rect:[LEFT,self.y,WIDTH,used],rows:table_rows,heights,columns,aligns,style:self.table_style.clone()})?;self.y+=used+18.;}Ok(())
    }
    fn blocks(&mut self,blocks:&[Block],bullet:Option<String>,depth:usize) -> Result<()> {
        if depth>16{return Err(BUDGET.into());}
        for block in blocks {
            check()?;
            match block {
                Block::Heading{inlines,..}=>self.inline_paragraph(inlines,28.,None)?,
                Block::Paragraph(values)|Block::Caption{inlines:values,..}=>self.inline_paragraph(values,24.,bullet.clone())?,
                Block::List{ordered,start,items}=>for (index,item) in items.iter().enumerate(){
                    let marker=if let Some(checked)=item.task{if checked{"☑".into()}else{"☐".into()}}else if *ordered{format!("{}.",start.saturating_add(index as u64))}else{"•".into()};
                    self.blocks(&item.blocks,Some(marker),depth+1)?;
                },
                Block::Quote(values)=>self.blocks(values,bullet.clone(),depth+1)?,
                Block::Code{text:source,..}=>{let mut run=Run{text:source.clone(),..Default::default()};run.code=true;self.paragraph(vec![run],18.,None,true)?;},
                Block::Math(latex)=>self.math(latex,None)?,Block::OfficeMath{latex,omml}=>self.math(latex,Some(omml))?,
                Block::Table{aligns,header,rows}=>self.table(aligns,header,rows)?,
                Block::Hr|Block::PageBreak=>self.continuation()?,
                Block::FootnoteDef{label,blocks}=>{self.paragraph(text(format!("[{label}]")),20.,None,false)?;self.blocks(blocks,None,depth+1)?;},
                Block::Html(raw) if comments_only(raw)=>(),
                Block::Bibliography(bib)=>{self.paragraph(text(&bib.title),28.,None,false)?;for item in &bib.items {self.inline_paragraph(&crate::md_ast::rich_inlines(&item.rich),20.,None)?;}},
                Block::Styled{..}|Block::SectionStart(_)|Block::Html(_)=>return Err(INVALID.into()),
            }
        }
        Ok(())
    }
}

fn build(content:&str,base:&str,title:&str,options:&Value) -> Result<Deck> {
    if content.len()>8*1024*1024{return Err(BUDGET.into());}check()?;
    let (content,_)=crate::mdexport::prepare_export_source("pptx",content,base,options)?;
    let content=crate::math_dsl::expand_markdown(&content).map_err(str::to_string)?;
    // The legacy browser slide sanitizer may drop unsupported HTML. Reject it
    // on the original parser events before splitting, so it cannot vanish from
    // an apparently successful PowerPoint export.
    for event in pulldown_cmark::Parser::new_ext(&content,crate::md_ast::options()) {
        check()?;
        if let pulldown_cmark::Event::Html(raw)|pulldown_cmark::Event::InlineHtml(raw)=event {if !comments_only(&raw){return Err(INVALID.into());}}
    }
    // Preserve actual Markdown block ranges while reusing the established slide
    // separator/notes convention. Literal headings or `---` in fences never
    // become slide boundaries, including tilde and longer-backtick fences.
    let mut protected=content.clone();let mut replacements=Vec::new();let mut code=false;
    for (event,range) in pulldown_cmark::Parser::new_ext(&content,crate::md_ast::options()).into_offset_iter() {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_))=>{
                code=true;let key=format!("READMDPPTXBLOCK{}X{}",uuid::Uuid::new_v4().simple(),replacements.len());replacements.push((range,key));
            },
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::CodeBlock)=>code=false,
            pulldown_cmark::Event::DisplayMath(_) if !code=>{let key=format!("READMDPPTXBLOCK{}X{}",uuid::Uuid::new_v4().simple(),replacements.len());replacements.push((range,key));},
            _=>(),
        }
    }
    for (range,key) in replacements.iter().rev(){protected.replace_range(range.clone(),key);}
    let matrix=crate::mdexport::split_slides_markdown(&protected);let mut builder=Builder::new(base,options)?;
    for group in matrix {
    let mut section_heading=text(title.trim_end_matches(".md"));
    for (chunk,notes) in group {
        let mut chunk=chunk;let mut notes=notes;
        for (range,key) in &replacements {chunk=chunk.replace(key,&content[range.clone()]);notes=notes.replace(key,&content[range.clone()]);}
        check()?;builder.finish()?;
        let (doc,warnings)=crate::mdexport::publication_document(&chunk,base,options)?;builder.deck.warnings.extend(warnings);
        if notes.len()>256*1024{return Err(BUDGET.into());}
        builder.page.notes=notes;let mut blocks=doc.blocks.as_slice();
        let heading=if let Some(Block::Heading{inlines,..})=blocks.first() {blocks=&blocks[1..];let mut values=Vec::new();inline_runs(inlines,&Run::default(),&mut values)?;values}else{section_heading.clone()};
        section_heading=heading.clone();
        if !builder.cover(&heading,blocks)?{builder.heading(heading)?;builder.blocks(blocks,None,0)?;}
    }
    }
    builder.finish()?;if builder.deck.slides.is_empty(){builder.heading(text(title))?;builder.finish()?;}
    Ok(builder.deck)
}

fn span(run:&Run) -> String {
    let style=format!("font-weight:{};font-style:{};text-decoration:{};font-family:{}",if run.bold{"700"}else{"400"},if run.italic{"italic"}else{"normal"},if run.strike{"line-through"}else{"none"},if run.code{"monospace"}else{"Arial, sans-serif"});
    let inner=format!("<span style=\"{style}\">{}</span>",escaped(&run.text));
    if let Some(href)=&run.href {format!("<a href=\"{}\">{inner}</a>",escaped(href))}else{inner}
}
fn line_html(line:&Line) -> String {line.iter().map(span).collect()}
fn position([x,y,w,h]:Rect) -> String {format!("position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h}px;")}
fn preview_html(deck:&Deck) -> Result<String> {
    let mut slides=String::new();
    for (index,slide) in deck.slides.iter().enumerate() {
        check()?;slides.push_str(&format!("<section class=\"readmd-slide\" data-page=\"{}\"{}>",index+1,if index==0{""}else{" hidden"}));
        for shape in &slide.shapes {
            check()?;
            if slides.len()>LIMIT{return Err(BUDGET.into());}
            if let Shape::Image{media,..}=shape {if slides.len().saturating_add(deck.media[*media].svg.as_ref().unwrap_or(&deck.media[*media].bytes).len().saturating_mul(4)/3+512)>LIMIT{return Err(BUDGET.into());}}
            match shape {
            Shape::Text{rect,lines,size,bullet,center,code}=>{
                slides.push_str(&format!("<div style=\"{}font-size:{size}px;line-height:1.25;white-space:pre;text-align:{};{}\">",position(*rect),if *center{"center"}else{"left"},if *code{"background:#30343B;color:#FFFFFF;padding:0 8px;box-sizing:border-box;"}else{""}));
                for (n,line) in lines.iter().enumerate(){slides.push_str("<div>");if n==0 {if let Some(bullet)=bullet{slides.push_str(&format!("<span style=\"position:absolute;left:-26px\">{}</span>",escaped(bullet)));}}slides.push_str(&line_html(line));slides.push_str("</div>");}slides.push_str("</div>");
            },
            Shape::Accent{rect,color,rounded}=>{
                slides.push_str(&format!("<div aria-hidden=\"true\" style=\"{}background:#{color};border-radius:{};\"></div>",position(*rect),if *rounded {"8px"}else{"0"}));
            },
            Shape::Image{rect,media,alt}=>{let image=&deck.media[*media];slides.push_str(&format!("<img alt=\"{}\" style=\"{}\" src=\"data:image/{};base64,{}\">",escaped(alt),position(*rect),if image.svg.is_some(){"svg+xml"}else if image.extension=="jpg"{"jpeg"}else{&image.extension},base64::engine::general_purpose::STANDARD.encode(image.svg.as_ref().unwrap_or(&image.bytes))));},
            Shape::Table{rect,rows,heights,columns,aligns,style}=>{
                slides.push_str(&format!("<table style=\"{}table-layout:fixed;border-collapse:collapse;font-size:20px;line-height:25px;\">",position(*rect)));
                for (n,row) in rows.iter().enumerate(){slides.push_str(&format!("<tr style=\"height:{}px\">",heights[n]));for (column,cell) in row.iter().enumerate(){slides.push_str(&format!("<td style=\"width:{}px;padding:8px 10px;vertical-align:top;{}text-align:{};font-weight:{};box-sizing:border-box;\">",rect[2]/ *columns as f64,style.border_css(style.borders(n,rows.len())),aligns[column].css(),if n==0{"700"}else{"400"}));for line in cell{slides.push_str(&format!("<div style=\"white-space:pre\">{}</div>",line_html(line)));}slides.push_str("</td>");}slides.push_str("</tr>");}slides.push_str("</table>");
            },
        }}
        slides.push_str("</section>");if slides.len()>LIMIT{return Err(BUDGET.into());}
    }
    Ok(format!("<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>html,body{{margin:0;width:100%;height:100%;overflow:hidden;background:#{bg};color:#{fg};font-family:Arial,sans-serif}}a{{color:#{accent}}}#readmd-deck{{position:absolute;width:960px;height:540px;transform-origin:top left}}.readmd-slide{{position:relative;width:960px;height:540px;overflow:hidden}}.readmd-slide[hidden]{{display:none}}</style></head><body><main id=\"readmd-deck\">{slides}</main></body></html>",bg=deck.background,fg=deck.foreground,accent=deck.accent))
}

pub(crate) fn preview(content:&str,base:&str,title:&str,options:&Value) -> Result<Value> {
    crate::publication_budget::run("pptx",options,||{let deck=build(content,base,title,options)?;let html=preview_html(&deck)?;Ok(json!({"ok":true,"format":"pptx","html":html,"pages":deck.slides.len(),"width":W,"height":H,"warnings":deck.warnings}))})
}
fn emu(value:f64) -> i64 {(value*12700.).round() as i64}
fn xfrm([x,y,w,h]:Rect) -> String {format!("<a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",emu(x),emu(y),emu(w),emu(h))}
fn group() -> &'static str {"<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>"}
fn relation(id:&str,kind:&str,target:&str,external:bool) -> String {format!("<Relationship Id=\"{}\" Type=\"{R}/{}\" Target=\"{}\"{}/>",escaped(id),kind,escaped(target),if external{" TargetMode=\"External\""}else{""})}
fn relationships(body:&str) -> String {format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>")}
fn xml(root:&str,body:&str) -> String {format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:{root} xmlns:a=\"{A}\" xmlns:r=\"{R}\" xmlns:p=\"{P}\">{body}</p:{root}>")}
fn paragraph(line:&Line,size:f64,color:&str,bullet:Option<&str>,align:TableAlign,links:&BTreeMap<String,String>) -> String {
    let bullet=bullet.map(|value|format!("<a:buChar char=\"{}\"/>",escaped(value))).unwrap_or_else(||"<a:buNone/>".into());
    let mut body=format!("<a:p><a:pPr algn=\"{}\" marL=\"0\" indent=\"{}\"><a:lnSpc><a:spcPts val=\"{}\"/></a:lnSpc><a:spcBef><a:spcPts val=\"0\"/></a:spcBef><a:spcAft><a:spcPts val=\"0\"/></a:spcAft>{bullet}</a:pPr>",align.xml(),if bullet.contains("buChar"){-emu(24.)}else{0},(size*125.).round() as i64);
    for run in line {
        let link=run.href.as_ref().and_then(|href|links.get(href)).map(|id|format!("<a:hlinkClick r:id=\"{id}\"/>")).unwrap_or_default();
        body.push_str(&format!("<a:r><a:rPr lang=\"en-US\" sz=\"{}\" b=\"{}\" i=\"{}\" strike=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:latin typeface=\"{}\"/><a:ea typeface=\"Microsoft YaHei\"/>{link}</a:rPr><a:t xml:space=\"preserve\">{}</a:t></a:r>",(size*100.).round() as i64,u8::from(run.bold),u8::from(run.italic),if run.strike{"sngStrike"}else{"noStrike"},color,if run.code{"Consolas"}else{"Arial"},escaped(&run.text)));
    }
    body.push_str(&format!("<a:endParaRPr sz=\"{}\"/></a:p>",(size*100.).round() as i64));body
}
fn text_body(lines:&[Line],size:f64,color:&str,bullet:Option<&str>,center:bool,links:&BTreeMap<String,String>) -> String {
    aligned_text_body(lines,size,color,bullet,if center{TableAlign::Center}else{TableAlign::Left},links)
}
fn aligned_text_body(lines:&[Line],size:f64,color:&str,bullet:Option<&str>,align:TableAlign,links:&BTreeMap<String,String>) -> String {
    let mut body="<a:bodyPr wrap=\"none\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"t\"><a:noAutofit/></a:bodyPr><a:lstStyle/>".to_string();
    for (index,line) in lines.iter().enumerate(){body.push_str(&paragraph(line,size,color,if index==0{bullet}else{None},align,links));}body
}
fn shape_xml(shape:&Shape,id:usize,deck:&Deck,links:&BTreeMap<String,String>) -> String {
    match shape {
        Shape::Text{rect,lines,size,bullet,center,code}=>{
            let fill=if *code{"<a:solidFill><a:srgbClr val=\"30343B\"/></a:solidFill>"}else{"<a:noFill/>"};
            let mut rect=*rect;if *code{rect[0]+=8.;rect[2]-=16.;}
            format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{fill}<a:ln><a:noFill/></a:ln></p:spPr><p:txBody>{}</p:txBody></p:sp>",xfrm(rect),text_body(lines,*size,if *code{"FFFFFF"}else{deck.foreground},bullet.as_deref(),*center,links))
        },
        Shape::Accent{rect,color,rounded}=>{
            let geometry = if *rounded {"roundRect"} else {"rect"};
            format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Accent {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"{geometry}\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp>",xfrm(*rect))
        },
        Shape::Image{rect,media,alt}=>{
            let svg=if deck.media[*media].svg.is_some(){format!("<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"svg{media}\"/></a:ext></a:extLst>")}else{String::new()};
            format!("<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Image {id}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"image{media}\">{svg}</a:blip><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",escaped(alt),xfrm(*rect))
        },
        Shape::Table{rect,rows,heights,columns,aligns,style}=>{
            let [x,y,w,h]=*rect;let mut body=format!("<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Table {id}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"1\" bandRow=\"0\"/><a:tblGrid>",emu(x),emu(y),emu(w),emu(h));
            for _ in 0..*columns{body.push_str(&format!("<a:gridCol w=\"{}\"/>",emu(w/ *columns as f64)));}body.push_str("</a:tblGrid>");
            for (index,row) in rows.iter().enumerate(){body.push_str(&format!("<a:tr h=\"{}\">",emu(heights[index])));for (column,cell) in row.iter().enumerate(){
                let lines=if index==0 {cell.iter().map(|line|line.iter().map(|run|Run{bold:true,..run.clone()}).collect()).collect::<Vec<_>>()}else{cell.clone()};
                body.push_str(&format!("<a:tc><a:txBody>{}</a:txBody><a:tcPr marL=\"127000\" marR=\"127000\" marT=\"101600\" marB=\"101600\" anchor=\"t\">",aligned_text_body(&lines,20.,deck.foreground,None,aligns[column],links)));
                body.push_str(&style.border_xml(style.borders(index,rows.len())));body.push_str("<a:noFill/></a:tcPr></a:tc>");
            }body.push_str("</a:tr>");}body.push_str("</a:tbl></a:graphicData></a:graphic></p:graphicFrame>");body
        },
    }
}
fn theme(deck:&Deck) -> String {
    let colors=[("dk1","000000"),("lt1","FFFFFF"),("dk2",deck.foreground),("lt2",deck.background),("accent1",deck.accent),("accent2","7B9E87"),("accent3","D7AD6A"),("accent4","9475B5"),("accent5","5AA7B0"),("accent6","BC7979"),("hlink","2169C5"),("folHlink","7754A0")].iter().map(|(key,color)|format!("<a:{key}><a:srgbClr val=\"{color}\"/></a:{key}>")).collect::<String>();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><a:theme xmlns:a=\"{A}\" name=\"ReadMD\"><a:themeElements><a:clrScheme name=\"ReadMD\">{colors}</a:clrScheme><a:fontScheme name=\"ReadMD\"><a:majorFont><a:latin typeface=\"Arial\"/><a:ea typeface=\"Microsoft YaHei\"/><a:cs typeface=\"Arial\"/></a:majorFont><a:minorFont><a:latin typeface=\"Arial\"/><a:ea typeface=\"Microsoft YaHei\"/><a:cs typeface=\"Arial\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"ReadMD\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{line}{line}{line}</a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",fill="<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",line="<a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>")
}
fn package(deck:&Deck,title:&str) -> Result<Vec<u8>> {
    let mut entries=Vec::new();let mut bytes=0usize;
    let mut add=|path:&str,data:Vec<u8>| -> Result<()> {bytes=bytes.checked_add(data.len()).ok_or(BUDGET)?;if bytes>LIMIT{return Err(BUDGET.into());}check()?;entries.push(ZipEntry{path:path.into(),data,compress:true});Ok(())};
    let mut types=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>");
    for (path,kind) in [("slideMasters/slideMaster1.xml","slideMaster"),("slideLayouts/slideLayout1.xml","slideLayout"),("notesMasters/notesMaster1.xml","notesMaster"),("presProps.xml","presProps"),("viewProps.xml","viewProps")] {types.push_str(&format!("<Override PartName=\"/ppt/{path}\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.{kind}+xml\"/>"));}
    types.push_str("<Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>");
    let mut ids=String::new();let mut pres_rels=relation("master","slideMaster","slideMasters/slideMaster1.xml",false)+&relation("notes","notesMaster","notesMasters/notesMaster1.xml",false)+&relation("properties","presProps","presProps.xml",false)+&relation("view","viewProps","viewProps.xml",false);
    for (index,slide) in deck.slides.iter().enumerate() {
        check()?;let n=index+1;ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"slide{n}\"/>",256+index));pres_rels.push_str(&relation(&format!("slide{n}"),"slide",&format!("slides/slide{n}.xml"),false));
        for (directory,kind) in [("slides","slide"),("notesSlides","notesSlide")]{types.push_str(&format!("<Override PartName=\"/ppt/{directory}/{kind}{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.{kind}+xml\"/>"));}
        let mut links=BTreeMap::new();
        let mut visit=|line:&Line| {for run in line{if let Some(href)=&run.href{let id=format!("link{}",links.len());links.entry(href.clone()).or_insert(id);}}};
        for shape in &slide.shapes {match shape {Shape::Text{lines,..}=>for line in lines{visit(line);},Shape::Table{rows,..}=>for line in rows.iter().flatten().flatten(){visit(line);},_=>()}}
        let mut rels=relation("layout","slideLayout","../slideLayouts/slideLayout1.xml",false)+&relation("notes","notesSlide",&format!("../notesSlides/notesSlide{n}.xml"),false);
        for (href,id) in &links {rels.push_str(&relation(id,"hyperlink",href,true));}
        let mut media=std::collections::BTreeSet::new();for shape in &slide.shapes {if let Shape::Image{media:id,..}=shape{media.insert(*id);}}
        for id in media{rels.push_str(&relation(&format!("image{id}"),"image",&format!("../media/image{id}.{}",deck.media[id].extension),false));if deck.media[id].svg.is_some(){rels.push_str(&relation(&format!("svg{id}"),"image",&format!("../media/image{id}.svg"),false));}}
        let shapes=slide.shapes.iter().enumerate().map(|(i,shape)|shape_xml(shape,i+2,deck,&links)).collect::<String>();
        let transition=match deck.transition.as_str(){"fade"=>"<p:transition spd=\"med\"><p:fade/></p:transition>","slide"=>"<p:transition spd=\"med\"><p:push dir=\"l\"/></p:transition>","zoom"=>"<p:transition spd=\"med\"><p:zoom dir=\"in\"/></p:transition>",_=>""};
        add(&format!("ppt/slides/slide{n}.xml"),xml("sld",&format!("<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>{}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}",deck.background,group())).into_bytes())?;
        add(&format!("ppt/slides/_rels/slide{n}.xml.rels"),relationships(&rels).into_bytes())?;
        let note=Shape::Text{rect:[36.,100.,600.,600.],lines:slide.notes.lines().map(text).collect(),size:14.,bullet:None,center:false,code:false};
        let notes_shape=shape_xml(&note,2,deck,&BTreeMap::new())
            .replacen("<p:nvPr/>","<p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr>",1)
            .replace(&format!("val=\"{}\"",deck.foreground),"val=\"24292F\"");
        add(&format!("ppt/notesSlides/notesSlide{n}.xml"),xml("notes",&format!("<p:cSld><p:spTree>{}{notes_shape}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",group())).into_bytes())?;
        add(&format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),relationships(&(relation("master","notesMaster","../notesMasters/notesMaster1.xml",false)+&relation("slide","slide",&format!("../slides/slide{n}.xml"),false))).into_bytes())?;
    }
    types.push_str("<Default Extension=\"svg\" ContentType=\"image/svg+xml\"/></Types>");add("[Content_Types].xml",types.into_bytes())?;
    add("_rels/.rels",relationships(&(relation("document","officeDocument","ppt/presentation.xml",false)+&relation("core","metadata/core-properties","docProps/core.xml",false).replace(&format!("{R}/metadata/core-properties"),"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties")+&relation("app","extended-properties","docProps/app.xml",false))).into_bytes())?;
    add("ppt/presentation.xml",xml("presentation",&format!("<p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"master\"/></p:sldMasterIdLst><p:notesMasterIdLst><p:notesMasterId r:id=\"notes\"/></p:notesMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\" type=\"screen16x9\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/><p:defaultTextStyle/>",emu(W),emu(H))).into_bytes())?;
    add("ppt/_rels/presentation.xml.rels",relationships(&pres_rels).into_bytes())?;
    let map="<p:clrMap accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" bg1=\"lt1\" bg2=\"lt2\" folHlink=\"folHlink\" hlink=\"hlink\" tx1=\"dk1\" tx2=\"dk2\"/>";
    add("ppt/slideMasters/slideMaster1.xml",xml("sldMaster",&format!("<p:cSld><p:spTree>{}</p:spTree></p:cSld>{map}<p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"layout\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles>",group())).into_bytes())?;
    add("ppt/slideMasters/_rels/slideMaster1.xml.rels",relationships(&(relation("layout","slideLayout","../slideLayouts/slideLayout1.xml",false)+&relation("theme","theme","../theme/theme1.xml",false))).into_bytes())?;
    add("ppt/slideLayouts/slideLayout1.xml",xml("sldLayout",&format!("<p:cSld name=\"Blank\"><p:spTree>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>",group())).replacen("<p:sldLayout ","<p:sldLayout type=\"blank\" preserve=\"1\" ",1).into_bytes())?;
    add("ppt/slideLayouts/_rels/slideLayout1.xml.rels",relationships(&relation("master","slideMaster","../slideMasters/slideMaster1.xml",false)).into_bytes())?;
    add("ppt/notesMasters/notesMaster1.xml",xml("notesMaster",&format!("<p:cSld><p:spTree>{}</p:spTree></p:cSld>{map}<p:notesStyle/>",group())).into_bytes())?;
    add("ppt/notesMasters/_rels/notesMaster1.xml.rels",relationships(&relation("theme","theme","../theme/theme1.xml",false)).into_bytes())?;
    add("ppt/theme/theme1.xml",theme(deck).into_bytes())?;add("ppt/presProps.xml",xml("presentationPr","").into_bytes())?;add("ppt/viewProps.xml",xml("viewPr","<p:normalViewPr/>").into_bytes())?;
    add("docProps/core.xml",format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>ReadMD</dc:creator><cp:lastModifiedBy>ReadMD</cp:lastModifiedBy></cp:coreProperties>",escaped(title)).into_bytes())?;
    add("docProps/app.xml",format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>ReadMD</Application><PresentationFormat>Widescreen</PresentationFormat><Slides>{}</Slides><Notes>{}</Notes></Properties>",deck.slides.len(),deck.slides.len()).into_bytes())?;
    for (id,media) in deck.media.iter().enumerate(){add(&format!("ppt/media/image{id}.{}",media.extension),media.bytes.clone())?;if let Some(svg)=&media.svg{add(&format!("ppt/media/image{id}.svg"),svg.clone())?;}}
    let bytes=crate::mdexport::write_zip(&entries)?;if bytes.len()>LIMIT{return Err(BUDGET.into());}check()?;Ok(bytes)
}
pub(crate) fn export(content:&str,base:&str,out:&str,options:&Value,title:&str) -> Result<ExportResult> {
    let deck=build(content,base,title,options)?;let bytes=package(&deck,title)?;check()?;
    crate::content::write_bytes_atomic(Path::new(out),&bytes).map_err(|_|"export_snapshot_persistence_failed")?;
    Ok(ExportResult{ok:true,path:Some(out.into()),size:Some(bytes.len() as u64),warns:Some(deck.warnings),error:None,canceled:Some(false)})
}

#[cfg(test)]
#[path="pptx_export_tests.rs"]
mod tests;

#[cfg(test)]
#[path="pptx_export_table_tests.rs"]
mod table_style_tests;
