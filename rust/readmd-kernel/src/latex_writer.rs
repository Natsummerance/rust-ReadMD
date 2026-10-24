//! Markdown → standalone LaTeX, built on [`crate::md_ast`].
//!
//! Replaces the line-by-line converter that used to live in `mdexport` (which
//! sliced UTF-8 strings by character index and panicked on `中文**粗**`, had no
//! CJK support and dropped images, ordered lists, tables and links).  Output is
//! meant for XeLaTeX (`% !TEX program = xelatex`) but stays compilable with
//! pdfLaTeX for pure-ASCII documents thanks to the `iftex` guard.

use crate::md_ast::{inline_plain, Align, Block, Document, Inline};
use std::collections::{BTreeMap, BTreeSet, HashMap};
#[path = "latex_writer_paragraphs.rs"]
mod paragraphs;
pub(crate) use paragraphs::GeometryContext as ParagraphGeometry;

#[cfg(test)]
#[path="latex_writer_author_inline_tests.rs"]
mod author_inline_tests;

/// Document-level knobs, read from the export panel's `tex`/`meta` options.
#[derive(Debug, Clone)]
pub struct LatexOptions {
    pub title: String,
    pub author: String,
    /// Literal date text. None uses `\today` only in the legacy title lane.
    pub date: Option<String>,
    pub doc_class: String,
    pub font_size: String,
    pub paper: String,
    pub margin: String,
    /// `None` → enable `ctex` iff the body contains CJK.
    pub use_ctex: Option<bool>,
    pub toc: bool,
    /// Export panel selection: biblatex / natbib / bibtex.
    pub bib_engine: String,
    /// Relative, copied .bib resources; never raw user-supplied LaTeX.
    pub bibliography: Vec<String>,
    /// Application-generated relative font resource, materialized by the exporter
    /// only when a visible scalar needs the core SMP fallback.
    pub core_text_font: Option<String>,
    /// Shared, validated section ranges. Empty preserves legacy output.
    pub publication_sections: Vec<crate::publication_sections::ResolvedSection>,
    /// The already validated publication projection. None keeps the direct
    /// standalone writer's legacy title and class-default layout contract.
    pub publication_style: Option<serde_json::Value>,
    /// Internal projection boundary; never read from user JSON. Shared metadata
    /// front blocks keep their own story and do not inherit ordinary-body mode.
    pub publication_front_block_count: usize,
    /// Internal sparse-layer provenance, separate from client style JSON.
    pub publication_legacy_line_height: f64,
}

impl Default for LatexOptions {
    fn default() -> Self {
        LatexOptions {
            title: String::new(),
            author: String::new(),
            date: None,
            doc_class: "article".into(),
            font_size: "11pt".into(),
            paper: "a4paper".into(),
            margin: "2.5cm".into(),
            use_ctex: None,
            toc: false,
            bib_engine: "biblatex".into(),
            bibliography: Vec::new(),
            core_text_font: None,
            publication_sections: Vec::new(),
            publication_style: None,
            publication_front_block_count: 0,
            publication_legacy_line_height: 1.6,
        }
    }
}

/// Image resolution callback: markdown `src` → path to use in `\includegraphics`
/// (already relative to the `.tex` file), or `None` if it cannot be embedded.
pub type ImageMap<'a> = &'a dyn Fn(&str) -> Option<String>;

// Split both column lists at real TeX line/page breakpoints. A long citation
// is never trapped in one unbreakable table row or minipage. Nested calls use
// local box assignments and retain their split points in their parent list.
const CSL_COLUMN_MACROS:&str=r#"
\newbox\readmdcslleft
\newbox\readmdcslright
\newbox\readmdcslleftpart
\newbox\readmdcslrightpart
\newdimen\readmdcslleftwidth
\newdimen\readmdcslrightwidth
\newdimen\readmdcslavailable
\newdimen\readmdcslheight
\newif\ifreadmdcslmore
\newbox\readmdcslwordbox
\newcommand{\readmdcslchar}[1]{%
  \setbox\readmdcslwordbox=\hbox{#1}%
  \ifdim\wd\readmdcslwordbox>\linewidth\relax
    \PackageError{ReadMD}{publication_layout_geometry}{A CSL character exceeds its column}%
  \fi
  #1%
}
\newcommand{\readmdcslword}[2]{%
  \begingroup
  \setbox\readmdcslwordbox=\hbox{#1}%
  \ifdim\wd\readmdcslwordbox>\linewidth\relax
    #2%
  \else
    #1%
  \fi
  \endgroup
}
\newcommand{\readmdcslcolumns}[2]{%
  \par\begingroup
  \readmdcslleftwidth=\readmdbiblabelwidth
  \ifdim\readmdcslleftwidth<.5em\readmdcslleftwidth=.5em\fi
  \ifdim\readmdcslleftwidth>.35\linewidth\readmdcslleftwidth=.35\linewidth\fi
  \readmdcslrightwidth=\dimexpr\linewidth-\readmdcslleftwidth-.75em\relax
  \ifdim\readmdcslleftwidth<.5em\PackageError{ReadMD}{publication_layout_geometry}{CSL label column is too narrow}\fi
  \ifdim\readmdcslrightwidth<1em\PackageError{ReadMD}{publication_layout_geometry}{CSL columns are too narrow}\fi
  % A short non-final line needs stretch: emergency penalties alone can
  % otherwise force an overfull line into the reserved structural gutter.
  \setbox\readmdcslleft=\vbox{\hsize=\readmdcslleftwidth\linewidth=\hsize\leftskip=0pt\rightskip=0pt plus1fil\parfillskip=0pt plus1fil\parindent=0pt\parskip=0pt\noindent#1\par}%
  \setbox\readmdcslright=\vbox{\hsize=\readmdcslrightwidth\linewidth=\hsize\leftskip=0pt\rightskip=0pt plus1fil\parfillskip=0pt plus1fil\parindent=0pt\parskip=0pt\noindent#2\par}%
  \loop
    \ifinner\readmdcslavailable=\textheight\else
      \readmdcslavailable=\dimexpr\pagegoal-\pagetotal\relax
      \ifdim\readmdcslavailable>\textheight\readmdcslavailable=\textheight\fi
      \ifdim\readmdcslavailable<2\baselineskip\newpage\readmdcslavailable=\textheight\fi
    \fi
    \advance\readmdcslavailable by-\baselineskip
    \splittopskip=\topskip
    \readmdcslheight=\dimexpr\ht\readmdcslleft+\dp\readmdcslleft\relax
    \ifdim\readmdcslheight>\readmdcslavailable
      \setbox\readmdcslleftpart=\vsplit\readmdcslleft to\readmdcslavailable
    \else\setbox\readmdcslleftpart=\box\readmdcslleft\fi
    \readmdcslheight=\dimexpr\ht\readmdcslright+\dp\readmdcslright\relax
    \ifdim\readmdcslheight>\readmdcslavailable
      \setbox\readmdcslrightpart=\vsplit\readmdcslright to\readmdcslavailable
    \else\setbox\readmdcslrightpart=\box\readmdcslright\fi
    \ifdim\ht\readmdcslleftpart>\dimexpr\readmdcslavailable+1pt\relax\PackageError{ReadMD}{publication_layout_geometry}{An unbreakable CSL object exceeds the page}\fi
    \ifdim\ht\readmdcslrightpart>\dimexpr\readmdcslavailable+1pt\relax\PackageError{ReadMD}{publication_layout_geometry}{An unbreakable CSL object exceeds the page}\fi
    \noindent\hbox to\linewidth{\hbox to\readmdcslleftwidth{\vtop{\unvbox\readmdcslleftpart}\hss}\hspace{.75em}\hbox to\readmdcslrightwidth{\vtop{\unvbox\readmdcslrightpart}\hss}\hss}\par
    \readmdcslmorefalse
    \ifvoid\readmdcslleft\else\readmdcslmoretrue\fi
    \ifvoid\readmdcslright\else\readmdcslmoretrue\fi
    \ifreadmdcslmore\ifinner\penalty-10000\else\newpage\fi\fi
  \ifreadmdcslmore\repeat
  \par\endgroup
}
"#;

/// True for CJK ideographs, kana, hangul and CJK punctuation / fullwidth forms.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x2FDF | 0x3000..=0x30FF | 0x3100..=0x31FF |
        0x3200..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF |
        0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// Escape running text for LaTeX (text mode).
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\textbackslash{}"),
            '&' => out.push_str(r"\&"),
            '%' => out.push_str(r"\%"),
            '$' => out.push_str(r"\$"),
            '#' => out.push_str(r"\#"),
            '_' => out.push_str(r"\_"),
            '{' => out.push_str(r"\{"),
            '}' => out.push_str(r"\}"),
            '~' => out.push_str(r"\textasciitilde{}"),
            '^' => out.push_str(r"\textasciicircum{}"),
            '<' => out.push_str(r"\textless{}"),
            '>' => out.push_str(r"\textgreater{}"),
            '|' => out.push_str(r"\textbar{}"),
            '\u{00A0}' => out.push('~'),
            _ => out.push(c),
        }
    }
    out
}

fn escape_visible_text(s: &str, fallback: bool, used: &mut bool) -> String {
    if !fallback || !crate::publication_fonts::needs_core_text_fallback(s) {
        return escape_text(s);
    }
    let mut out = String::with_capacity(s.len() + 24);
    let mut start = 0;
    for (index, character) in s.char_indices() {
        if character as u32 <= 0xffff {
            continue;
        }
        let mut encoded = [0; 4];
        if !crate::publication_fonts::needs_core_text_fallback(character.encode_utf8(&mut encoded)) {
            continue;
        }
        out.push_str(&escape_text(&s[start..index]));
        out.push_str("\\readmdunicode{");
        out.push(character);
        out.push('}');
        start = index + character.len_utf8();
        *used = true;
    }
    out.push_str(&escape_text(&s[start..]));
    out
}

// Encode the path as character tokens, never raw TeX. fontspec's extension
// detection compares tokens: ASCII letters must retain category 11 so .ttf
// and .otf match its declared extensions. All other characters remain literal
// category 12, including spaces, braces, percent signs and Unicode scalars.
fn font_file_tokens(path: &str) -> String {
    path.chars().map(|character| {
        let category = if character.is_ascii_alphabetic() { 11 } else { 12 };
        format!("\\char_generate:nn {{{}}}{{{category}}}", character as u32)
    }).collect()
}

/// Escape a URL for `\href{…}` / `\url{…}`.
fn escape_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '#' => out.push_str(r"\#"),
            '%' => out.push_str(r"\%"),
            '{' => out.push_str("%7B"),
            '}' => out.push_str("%7D"),
            _ => out.push(c),
        }
    }
    out
}

/// A `\label` key: ASCII-safe, deterministic, identical for heading and link.
// Dots delimit escaped scalars and are themselves escaped. This preserves
// existing plain ASCII keys while keeping α/u3b1 and "."/u2e distinct.
pub fn label_key(slug: &str) -> String {
    let mut out = String::from("sec:");
    for c in slug.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push_str(&format!(".{:x}.", c as u32));
        }
    }
    out
}

/// `listings` only knows a fixed set of languages; anything else would be a
/// compile error, so unknown fences fall back to no highlighting.
fn listings_language(lang: &str) -> Option<&'static str> {
    Some(match lang.to_ascii_lowercase().as_str() {
        "python" | "py" | "python3" => "Python",
        "c" | "h" => "C",
        "cpp" | "c++" | "cc" | "cxx" | "hpp" => "C++",
        "java" => "Java",
        "bash" | "sh" | "shell" | "zsh" => "bash",
        "sql" => "SQL",
        "html" | "htm" => "HTML",
        "xml" | "svg" => "XML",
        "tex" | "latex" => "TeX",
        "ruby" | "rb" => "Ruby",
        "perl" | "pl" => "Perl",
        "php" => "PHP",
        "r" => "R",
        "matlab" | "m" => "Matlab",
        "fortran" | "f90" => "Fortran",
        "pascal" => "Pascal",
        "lisp" | "elisp" => "Lisp",
        "haskell" | "hs" => "Haskell",
        "make" | "makefile" => "make",
        _ => return None,
    })
}

struct Writer<'a> {
    out: String,
    images: ImageMap<'a>,
    footnotes: HashMap<String, Vec<Block>>,
    /// Footnote labels currently being expanded (guards self-reference).
    fn_stack: Vec<String>,
    bib_engine: String,
    chapter_headings: bool,
    heading_numbering: bool,
    toc_numbers: TocNumbers,
    publication_style: Option<serde_json::Value>,
    core_text_fallback: bool,
    core_text_used: bool,
    /// Emergency ASCII breaks are allowed only inside recursive CSL columns.
    csl_columns_text: bool,
    paragraphs: paragraphs::State,
    author: Option<crate::publication_inline_text::ResolvedTextStyle>,
    author_base: crate::publication_inline_text::ResolvedTextStyle,
    local_pair: Option<crate::publication_inline_text::ResolvedTextStyle>,
}

type TocNumbers = BTreeMap<&'static str, BTreeSet<String>>;

fn has_heading_numbering(blocks:&[Block])->bool{
    blocks.iter().any(|block|match block.unstyled(){
        Block::Heading{numbering:Some(_),..}=>true,
        Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>has_heading_numbering(blocks),
        Block::List{items,..}=>items.iter().any(|item|has_heading_numbering(&item.blocks)),_=>false,
    })
}

// Native article/report classes stop at subparagraph (level 5). Shared H6
// uses one further named level; its reader and bookmark metadata must exist
// before the first tableofcontents consumes a previous pass's .toc file.
const SIXTH_TOC_LEVEL:&str=r#"
\makeatletter
\providecommand{\l@readmdsubparagraph}{\@dottedtocline{6}{10em}{5em}}
\providecommand{\toclevel@readmdsubparagraph}{6}
\makeatother
"#;

// Measure in the original TOC reader, after it has selected its real font,
// size and series. One discarded row computes a cached maximum per kind;
// visible rows keep the native indent, leaders, page box and depth checks.
const MEASURED_TOC_NUMBERS:&str=r#"
\makeatletter
\newbox\readmdtocrowbox
\newbox\readmdtocnumberbox
\newdimen\readmdtocnumberwidth
\newcommand{\readmdtocmeasure}[1]{%
  \setbox\readmdtocnumberbox=\hbox{#1\hskip\fontdimen2\font}%
  \ifdim\wd\readmdtocnumberbox>\readmdtocnumberwidth
    \readmdtocnumberwidth=\wd\readmdtocnumberbox
  \fi
}
\newcommand{\readmdtocentry}[3]{%
  \begingroup
  \ifcsname readmdtocwidth@#1\endcsname\else
    \setbox\readmdtocrowbox=\vbox{%
      \def\numberline##1{%
        \readmdtocnumberwidth=\@tempdima
        \csname readmdtoclabels@#1\endcsname
        \expandafter\xdef\csname readmdtocwidth@#1\endcsname{\the\readmdtocnumberwidth}%
      }%
      \csname readmdtocoriginal@#1\endcsname{\numberline{}}{}%
    }%
  \fi
  \ifcsname readmdtocwidth@#1\endcsname
    \let\readmdtocsetlength\setlength
    \def\setlength##1##2{%
      \ifx##1\@tempdima
        \readmdtocsetlength{##1}{\csname readmdtocwidth@#1\endcsname}%
      \else\readmdtocsetlength{##1}{##2}\fi
    }%
    \let\readmdtocdottedline\@dottedtocline
    \def\@dottedtocline##1##2##3{%
      \readmdtocdottedline{##1}{##2}{\csname readmdtocwidth@#1\endcsname}%
    }%
  \fi
  \csname readmdtocoriginal@#1\endcsname{#2}{#3}%
  \endgroup
}
\newcommand{\readmdtocsetup}[2]{%
  \expandafter\def\csname readmdtoclabels@#1\endcsname{#2}%
  \expandafter\let\expandafter\readmdtocoriginal\csname l@#1\endcsname
  \expandafter\let\csname readmdtocoriginal@#1\endcsname\readmdtocoriginal
  \expandafter\def\csname l@#1\endcsname##1##2{\readmdtocentry{#1}{##1}{##2}}%
}
\makeatother
"#;

fn measured_toc_settings(numbers:&TocNumbers,fallback:bool,used:&mut bool)->String{
    let mut out=String::new();
    for(kind,labels)in numbers{
        out.push_str(&format!("\\readmdtocsetup{{{kind}}}{{"));
        for label in labels{
            out.push_str(&format!("\\readmdtocmeasure{{{}}}",escape_visible_text(label,fallback,used)));
        }
        out.push_str("}\n");
    }
    out
}

/// Independent native counters avoid class-dependent automatic prefixes.
/// Expandable repeated-letter formatters match Word's native numbering after
/// Z (AA, BB, ..., ZZ, AAA) without TeX's built-in 26-letter counter limit.
const ALPHABET_NUMBER_MACROS:&str=r#"
\ExplSyntaxOn
\cs_new:Npn \readmdalph #1 { \int_compare:nNnT {#1} > {0} { \prg_replicate:nn { \int_div_truncate:nn {#1-1} {26} + 1 } { \int_to_alph:n { \int_mod:nn {#1-1} {26} + 1 } } } }
\cs_new:Npn \readmdAlph #1 { \int_compare:nNnT {#1} > {0} { \prg_replicate:nn { \int_div_truncate:nn {#1-1} {26} + 1 } { \int_to_Alph:n { \int_mod:nn {#1-1} {26} + 1 } } } }
\ExplSyntaxOff
"#;
const HEADING_COUNTER_MACROS:&str=r#"
\newcounter{readmdhanchor}
\newcounter{readmdh1}
\newcounter{readmdh2}
\newcounter{readmdh3}
\newcounter{readmdh4}
\newcounter{readmdh5}
\newcounter{readmdh6}
\makeatletter
\expandafter\def\csname theHreadmdh1\endcsname{\arabic{readmdhanchor}}
\expandafter\def\csname theHreadmdh2\endcsname{\arabic{readmdhanchor}}
\expandafter\def\csname theHreadmdh3\endcsname{\arabic{readmdhanchor}}
\expandafter\def\csname theHreadmdh4\endcsname{\arabic{readmdhanchor}}
\expandafter\def\csname theHreadmdh5\endcsname{\arabic{readmdhanchor}}
\expandafter\def\csname theHreadmdh6\endcsname{\arabic{readmdhanchor}}
\newcommand{\readmdnumberedheadingmark}[2]{%
  \stepcounter{readmdhanchor}%
  \refstepcounter{readmdh#1}%
  \def\@currentlabelname{#2}%
  \xdef\readmdheadinghref{\@currentHref}%
}
\newcommand{\readmdnumberedheadingtoc}[3]{%
  \begingroup
  \let\@currentHref\readmdheadinghref
  \addcontentsline{toc}{#1}{\protect\numberline{\csname thereadmdh#2\endcsname}#3}%
  \endgroup
}
\makeatother
"#;

fn heading_counter_settings(level:u8,numbering:&crate::cross_references::HeadingNumbering,literal:&mut dyn FnMut(&str)->String)->Result<String,&'static str>{
    let depth=level as usize;
    if !(1..=6).contains(&depth)||numbering.numbers.len()!=depth||numbering.rules.len()<depth
        ||numbering.numbers.iter().any(|number|*number>crate::cross_references::MAX_NUMBER)
        ||![" ","\t",""].contains(&numbering.suffix.as_str()){return Err("publication_numbering_invalid");}
    let mut out=String::new();
    for(index,number)in numbering.numbers.iter().enumerate(){
        let before=if index+1==depth{*number as i64-1}else{*number as i64};
        out.push_str(&format!("\\setcounter{{readmdh{}}}{{{before}}}\n",index+1));
    }
    let mut format=String::new();
    for part in crate::cross_references::heading_text_parts(&numbering.rules[depth-1].text,depth){
        let index=match part{crate::cross_references::HeadingTextPart::Literal(text)=>{format.push_str(&literal(text));continue;},crate::cross_references::HeadingTextPart::Counter(index)=>index};
        let counter=format!("readmdh{}",index+1);
        let number_format=if numbering.rules[depth-1].is_legal{"decimal"}else{numbering.rules[index].format.as_str()};
        format.push_str(&match number_format{
            "decimal"=>format!("\\arabic{{{counter}}}"),"lowerRoman"=>format!("\\roman{{{counter}}}"),
            "upperRoman"=>format!("\\Roman{{{counter}}}"),"lowerLetter"=>format!("\\readmdalph{{\\value{{{counter}}}}}"),
            "upperLetter"=>format!("\\readmdAlph{{\\value{{{counter}}}}}"),_=>return Err("publication_numbering_invalid"),
        });
    }
    out.push_str(&format!("\\expandafter\\def\\csname thereadmdh{level}\\endcsname{{{format}}}\n"));Ok(out)
}

fn local_termes_font(attributes: &serde_json::Value) -> bool {
    crate::publication_blocks::validate_style(attributes).is_ok()
        && attributes.pointer("/text/font").and_then(serde_json::Value::as_str)
            == Some("TeX Gyre Termes")
}

fn any_local_style(blocks: &[Block], predicate: &dyn Fn(&serde_json::Value) -> bool) -> bool {
    blocks.iter().any(|block| match block {
        Block::Styled { attributes, block } => {
            predicate(attributes) || any_local_style(std::slice::from_ref(block.as_ref()), predicate)
        }
        Block::List { items, .. } => items.iter().any(|item| any_local_style(&item.blocks, predicate)),
        Block::Quote(blocks) | Block::FootnoteDef { blocks, .. } => any_local_style(blocks, predicate),
        _ => false,
    })
}

/// TeX implements one fixed, managed local family. Every other visual request
/// still needs the exporter warning; identities and empty sections do not.
pub(crate) fn has_unsupported_local_styles(doc: &Document) -> bool {
    has_unsupported_local_styles_with_spacing(doc,None)
}

pub(crate) fn has_unsupported_local_styles_with_spacing(doc:&Document, style:Option<&serde_json::Value>)->bool {
    use crate::publication_typography as t;
    let global = style.and_then(|style|t::global_line_spacing(style).ok().flatten());
    fn walk(block:&Block,ordinary:bool,inherited:Option<t::LineSpacing>,global:Option<t::LineSpacing>)->bool {
        match block {
            Block::Styled {attributes,block} => {
                if crate::publication_blocks::validate_style(attributes).is_err() { return true; }
                let body = ordinary && matches!(block.unstyled(),Block::Paragraph(_));
                let mode = t::resolve_line_spacing(inherited.or(if body{global}else{None}),&attributes["paragraph"],body).ok().flatten();
                let caption_author=matches!(block.unstyled(),Block::Caption{..})&&crate::publication_blocks::author_style(attributes).is_ok_and(|author|!author.is_empty());
                let unsupported = attributes.as_object().is_some_and(|root|root.iter().any(|(section,value)|match section.as_str(){
                    "id"=>false,
                    "text"=>value.as_object().is_some_and(|text|text.iter().any(|(key,value)|
                        !(key=="font"&&value.as_str()==Some("TeX Gyre Termes") || key=="size"&&(body&&mode.is_some()||caption_author)
                            || key=="author"&&matches!(block.unstyled(),Block::Caption{..})&&crate::publication_blocks::author_style(attributes).is_ok()))),
                    "paragraph"=>value.as_object().is_some_and(|p|p.keys().any(|key|
                        !(body&&mode.is_some()&&matches!(key.as_str(),"lineSpacing"|"lineHeight")))),
                    _=>true,
                }));
                unsupported || walk(block,ordinary,mode,global)
            },
            Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>blocks.iter().any(|b|walk(b,false,None,global)),
            Block::List{items,..}=>items.iter().any(|item|item.blocks.iter().any(|b|walk(b,false,None,global))),
            _=>false,
        }
    }
    doc.blocks.iter().any(|block|walk(block,true,None,global))
}

pub(crate) fn has_small_caps(doc: &Document) -> bool {
    let mut found=false;
    crate::md_ast::walk_inlines(&doc.blocks,&mut |inline|{
        if matches!(inline,Inline::Styled{format,..} if format.font_variant==crate::publication_rich::RichFontVariant::SmallCaps){found=true;}
    });
    fn rich(nodes:&[crate::publication_rich::RichNode])->bool {
        use crate::publication_rich::{RichFontVariant,RichNode};
        nodes.iter().any(|node|match node {
            RichNode::Text{format,..}=>format.font_variant==RichFontVariant::SmallCaps,
            RichNode::Link{children,..}|RichNode::Group{children,..}=>rich(children),
        })
    }
    fn bibliography(blocks:&[Block])->bool {
        blocks.iter().any(|block|match block.unstyled(){
            Block::Bibliography(bib)=>bib.items.iter().any(|item|rich(&item.first_field)||rich(&item.rich)),
            Block::List{items,..}=>items.iter().any(|item|bibliography(&item.blocks)),
            Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>bibliography(blocks),
            _=>false,
        })
    }
    found||bibliography(&doc.blocks)
}
fn has_bibliography_columns(blocks:&[Block])->bool {
    blocks.iter().any(|block|match block.unstyled(){
        Block::Bibliography(bib)=>bib.items.iter().any(|item|!item.first_field.is_empty()||item.layout().is_ok_and(|layout|crate::publication_rich::layout_has_columns(&layout))),
        Block::List{items,..}=>items.iter().any(|item|has_bibliography_columns(&item.blocks)),
        Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>has_bibliography_columns(blocks),
        _=>false,
    })
}
/// Render the full standalone `.tex` document.
pub fn render_document(doc: &Document, opts: &LatexOptions, images: ImageMap) -> String {
    render_document_with_resources(doc, opts, images).0
}

/// Return whether the actual visible projection requires the staged font.
/// Image captions and referenced notes are observed while rendering, so unused
/// source metadata and unused notes cannot create extra resource files.
pub(crate) fn render_document_with_resources(doc: &Document, opts: &LatexOptions, images: ImageMap) -> (String, bool) {
    match render_document_with_geometry(doc, opts, images) {
        Ok((source, used, _)) => (source, used),
        Err(error) => (format!("\\documentclass{{article}}\n\\begin{{document}}\n\\PackageError{{ReadMD}}{{{}}}{{Invalid publication paragraph}}\n\\end{{document}}\n", escape_text(&error)), false),
    }
}

/// Preflight precedes image/bibliography resource staging in actual exporters.
pub(crate) fn validate_paragraphs(doc:&Document, opts:&LatexOptions)->Result<(),String> {
    crate::math_word_projection::validate_document(doc,"tex")?;
    crate::publication_blocks::validate_author_render_document(doc).map_err(str::to_string)?;
    let original=opts.publication_style.clone().unwrap_or_else(||serde_json::json!({}));
    crate::publication_note_policies::validate_default_section_numbering(doc,&original).map_err(|error|error.to_string())?;
    let mut checked=crate::publication_note_policies::project_legacy(doc,&original).map_err(|error|error.to_string())?;
    crate::publication_typography::tracking::validate_document(doc, &checked, false)?;
    crate::publication_typography::baseline::validate_document(doc, &checked, crate::publication_typography::baseline::Consumer::Unsupported)?;
    checked["tex"] = serde_json::json!({"bibEngine":opts.bib_engine,"renderBibliography":!opts.bib_engine.is_empty()});
    if crate::publication_typography::global_line_spacing(&checked)?.is_some()
        || any_local_style(&doc.blocks,&|attributes| attributes["paragraph"].get("lineSpacing").is_some()) {
        crate::publication_typography::validate_tex_document_with_front(doc,
            &checked, opts.publication_front_block_count)?;
    }
    Ok(())
}

/// The actual exporter consumes the typed error/context, not a TeX string or
/// warning parsed back into a control message. Legacy public formatting keeps
/// its String signature but emits an explicit noncompilable error document.
pub(crate) fn render_document_with_geometry(doc: &Document, opts: &LatexOptions, images: ImageMap)
    -> Result<(String, bool, Option<ParagraphGeometry>), String>
{
    let mut projected_opts=opts.clone();let original=opts.publication_style.clone().unwrap_or_else(||serde_json::json!({}));
    crate::publication_note_policies::validate_default_section_numbering(doc,&original).map_err(|error|error.to_string())?;
    let projected=crate::publication_note_policies::project_legacy(doc,&original).map_err(|error|error.to_string())?;
    if projected!=original{if let Some(notes)=projected.get("footnotes"){for section in &mut projected_opts.publication_sections{section.style["footnotes"]=notes.clone();}}}
    if opts.publication_style.is_some()||projected!=original{projected_opts.publication_style=Some(projected);}
    let opts=&projected_opts;
    validate_paragraphs(doc,opts)?;
    let fallback = opts.core_text_font.is_some();
    let (body, mut core_text_used, mut paragraph_state, toc_numbers) = if opts.publication_sections.is_empty(){
        render_body_with_bibliography(doc, images, &opts.bib_engine,
            matches!(opts.doc_class.as_str(), "report" | "book" | "ctexrep" | "ctexbook"), fallback, opts.publication_style.as_ref(), opts.publication_front_block_count)
    }else{render_section_body(doc,opts,images,fallback)};
    if let Some(error) = paragraph_state.error.take() { return Err(error); }
    let toc_number_settings=measured_toc_settings(&toc_numbers,fallback,&mut core_text_used);
    let legacy_title = opts.publication_style.is_none() && !opts.title.trim().is_empty();
    let cover = opts.publication_style.as_ref().filter(|style|style["cover"]["enabled"].as_bool()==Some(true));
    // Hidden metadata remains literal metadata, not a visible glyph/resource
    // requirement. Typed paper/book front matter is already in the shared AST.
    let title = if legacy_title {escape_visible_text(&opts.title, fallback, &mut core_text_used)}else{escape_text(&opts.title)};
    let author = escape_visible_text(&opts.author, fallback && legacy_title, &mut core_text_used);
    let date = opts.date.as_ref().map(|date| escape_visible_text(date, fallback && legacy_title, &mut core_text_used));
    let cover_source = cover.map(|style|{
        let c=&style["cover"];
        let requested=c["title"].as_str().filter(|value|!value.trim().is_empty()).unwrap_or(&opts.title);
        let title=escape_visible_text(requested,fallback,&mut core_text_used);
        let subtitle=escape_visible_text(c["subtitle"].as_str().unwrap_or(""),fallback,&mut core_text_used);
        let author=escape_visible_text(&opts.author,fallback,&mut core_text_used);
        let date=escape_visible_text(c["date"].as_str().unwrap_or(""),fallback,&mut core_text_used);
        let alignment=match c["align"].as_str(){Some("left")=>"\\raggedright",Some("right")=>"\\raggedleft",_=>"\\centering"};
        let settings=shared_text_settings(style,opts.publication_legacy_line_height);
        // titlepage itself starts with a page transition. Colour commands must
        // run inside it, after that transition, never on the preceding empty
        // vertical list.
        format!("\\begin{{titlepage}}\n\\thispagestyle{{empty}}\n{settings}{alignment}\n\\vspace*{{\\fill}}\n{{\\fontsize{{28}}{{34}}\\selectfont\\bfseries {title}\\par}}\n\\vspace{{12pt}}\n{subtitle}\\par\n\\vspace{{12pt}}\n{author}\\par\n\\vspace{{12pt}}\n{date}\\par\n\\vspace*{{\\fill}}\n\\end{{titlepage}}\n")
    }).unwrap_or_default();
    let wants_ctex = opts.use_ctex.unwrap_or_else(|| {
        body.chars().any(is_cjk) || cover_source.chars().any(is_cjk)
            || (legacy_title && (opts.title.chars().any(is_cjk) || opts.author.chars().any(is_cjk)))
    });
    let mut s = String::new();
    s.push_str("% !TEX program = xelatex\n");
    s.push_str("% Generated by ReadMD. Compile with XeLaTeX (recommended) or LuaLaTeX.\n");
    // CTeX otherwise auto-selects SimSun on Windows / native fonts on macOS.
    // Generated documents use the same portable font family on every host.
    // A hand-authored/imported TeX template remains free to select its fonts.
    let class_fonts = if opts.doc_class.starts_with("ctex") {
        ",fontset=fandol"
    } else {
        ""
    };
    s.push_str(&format!(
        "\\documentclass[{},{}{}]{{{}}}\n\n",
        opts.font_size, opts.paper, class_fonts, opts.doc_class
    ));
    // T1 Computer Modern otherwise falls back to EC bitmap fonts and mktexpk.
    // Latin Modern ships ready-made outlines in the portable input runtime.
    s.push_str("\\usepackage{iftex}\n\\ifPDFTeX\n  \\usepackage[utf8]{inputenc}\n  \\usepackage[T1]{fontenc}\n  \\usepackage{lmodern}\n\\fi\n");
    if wants_ctex && !opts.doc_class.starts_with("ctex") {
        s.push_str("\\usepackage[UTF8,fontset=fandol]{ctex}\n");
    }
    if let Some(style)=&opts.publication_style{
        // The explicit cover, TOC and first body section share the same paper
        // profile; class defaults and TeX's legacy margin knob cannot silently
        // change the shared projection before the first section marker.
        let (width,height)=crate::export_styles::page_dimensions(style);
        let layout=crate::publication_layout::Layout::from_style(style);
        let n=|key:&str,d:f64|style["page"][key].as_f64().filter(|v|v.is_finite()).unwrap_or(d);
        s.push_str(&format!("\\usepackage[paperwidth={width}mm,paperheight={height}mm,left={}mm,right={}mm,top={}mm,bottom={}mm]{{geometry}}\n",n("marginLeft",18.)+layout.gutter,n("marginRight",18.),n("marginTop",20.),n("marginBottom",20.)));
    }else{s.push_str(&format!("\\usepackage[margin={}]{{geometry}}\n", opts.margin));}
    if !opts.publication_sections.is_empty(){
        s.push_str("\\usepackage{multicol}\n\\usepackage{fancyhdr}\n\\newcounter{readmdphysicalpage}\n\\setcounter{readmdphysicalpage}{1}\n");
        s.push_str(r#"\providecommand{\readmdtotalpages}{?}
\makeatletter
\newcommand{\readmdapplypagestyle}[1]{%
\begingroup
\pagestyle{#1}%
\global\let\@oddhead\@oddhead
\global\let\@evenhead\@evenhead
\global\let\@oddfoot\@oddfoot
\global\let\@evenfoot\@evenfoot
\global\let\f@nch@oddhead\f@nch@oddhead
\global\let\f@nch@oddfoot\f@nch@oddfoot
\global\let\f@nch@olh\f@nch@olh
\global\let\f@nch@och\f@nch@och
\global\let\f@nch@orh\f@nch@orh
\global\let\f@nch@elh\f@nch@elh
\global\let\f@nch@ech\f@nch@ech
\global\let\f@nch@erh\f@nch@erh
\global\let\f@nch@olf\f@nch@olf
\global\let\f@nch@ocf\f@nch@ocf
\global\let\f@nch@orf\f@nch@orf
\global\let\f@nch@elf\f@nch@elf
\global\let\f@nch@ecf\f@nch@ecf
\global\let\f@nch@erf\f@nch@erf
\global\let\headrulewidth\headrulewidth
\global\let\footrulewidth\footrulewidth
\endgroup
}
\AddToHook{shipout/after}{\immediate\write\@auxout{\string\gdef\string\readmdtotalpages{\number\value{readmdphysicalpage}}}\stepcounter{readmdphysicalpage}}
\makeatother
"#);
    }
    match opts.bib_engine.as_str() {
        "natbib" => s.push_str("\\usepackage[round,authoryear]{natbib}\n"),
        "bibtex" => s.push_str("% Bibliography engine: BibTeX\n"),
        ""=>(),
        _ => {
            s.push_str("\\usepackage[backend=biber]{biblatex}\n");
            for path in &opts.bibliography {
                s.push_str(&format!("\\addbibresource{{{}}}\n", escape_url(path)));
            }
        }
    }
    s.push_str(concat!(
        "\\usepackage{amsmath,amssymb,amsfonts,mathtools}\n",
        // DSL/OMML equations contain real Unicode math, including Greek and
        // mathematical alphabet scalars. Legacy Computer Modern math slots
        // cannot consume those scalars even when XeTeX handles body Unicode.
        // Use the pack's OpenType math font; pdfTeX keeps its ASCII math lane.
        "\\ifPDFTeX\\else\n  \\usepackage{unicode-math}\n  \\setmathfont{latinmodern-math.otf}\n\\fi\n",
        "\\usepackage{graphicx}\n",
        "\\usepackage[export]{adjustbox}\n",
        "\\usepackage{booktabs}\n",
        "\\usepackage{tabularx}\n",
        "\\usepackage{longtable}\n",
        "\\usepackage[normalem]{ulem}\n",
        "\\usepackage{xcolor}\n",
        "\\usepackage{listings}\n",
        "\\usepackage{hyperref}\n",
        "\n",
        "\\hypersetup{colorlinks=true, linkcolor=blue!60!black, urlcolor=blue!60!black, citecolor=blue!60!black}\n",
        "\\lstset{basicstyle=\\ttfamily\\small, breaklines=true, columns=fullflexible, keepspaces=true,\n",
        "  frame=single, rulecolor=\\color{black!15}, backgroundcolor=\\color{black!3},\n",
        "  keywordstyle=\\color{blue!70!black}\\bfseries, commentstyle=\\color{green!45!black}\\itshape,\n",
        "  stringstyle=\\color{red!60!black}, showstringspaces=false, upquote=true}\n",
        "\\setlength{\\parskip}{0.5em}\n",
        "\\providecommand{\\tightlist}{\\setlength{\\itemsep}{0pt}\\setlength{\\parskip}{0pt}}\n",
        "\n",
    ));
    if opts.bib_engine.is_empty() && has_bibliography_columns(&doc.blocks) {
        // Allocate once; each bibliography changes the dimensions only inside
        // its local spacing group. Ordinary paragraphs remain breakable.
        s.push_str("\\newlength{\\readmdbiblabelwidth}\n\\newlength{\\readmdbibmeasure}\n");
        s.push_str(CSL_COLUMN_MACROS);
    }
    let small_caps = has_small_caps(doc);
    if small_caps || any_local_style(&doc.blocks, &local_termes_font) {
        // The same locked four physical faces serve ordinary text and small caps.
        // Local defaults also cover headings/notes that call \normalfont. The
        // fixed NFSS name is independent of any user font or file-name string.
        s.push_str(concat!(
            "\\ifPDFTeX\n",
            "  \\newcommand{\\readmdtermesfont}{\\fontfamily{qtm}\\selectfont}\n",
            "  \\newcommand{\\readmdtermesblockfont}{\\renewcommand{\\rmdefault}{qtm}\\renewcommand{\\familydefault}{qtm}\\readmdtermesfont}\n",
            "\\else\n",
            "  \\usepackage{fontspec}\n",
            "  \\newfontfamily\\readmdtermesfont[NFSSFamily=readmdtermes,BoldFont={texgyretermes-bold.otf},ItalicFont={texgyretermes-italic.otf},BoldItalicFont={texgyretermes-bolditalic.otf}]{texgyretermes-regular.otf}\n",
            "  \\newcommand{\\readmdtermesblockfont}{\\renewcommand{\\rmdefault}{readmdtermes}\\renewcommand{\\familydefault}{readmdtermes}\\readmdtermesfont}\n",
            "\\fi\n"
        ));
        if small_caps {
            // Keep smcp in a separate NFSS family: fontspec's bx/sc alias can
            // otherwise select ordinary bold. Select only the family so the
            // inherited weight, shape and size survive. XeTeX records ActualText
            // on each native word while this local group is active; restoring
            // the parameter before shipout does not lose original Unicode.
            s.push_str(concat!(
                "\\ifPDFTeX\n",
                "  \\newcommand{\\readmdsmallcaps}[1]{{\\readmdtermesfont\\textsc{#1}}}\n",
                "\\else\n",
                "  \\newfontfamily\\readmdtermessmallcapsfont[NFSSFamily=readmdtermessmallcaps,Letters=SmallCaps,BoldFont={texgyretermes-bold.otf},ItalicFont={texgyretermes-italic.otf},BoldItalicFont={texgyretermes-bolditalic.otf}]{texgyretermes-regular.otf}\n",
                "  \\ifXeTeX\n",
                "    \\newcommand{\\readmdsmallcaps}[1]{{\\XeTeXgenerateactualtext=1\\relax\\readmdtermessmallcapsfont #1}}\n",
                "  \\else\n",
                "    \\newcommand{\\readmdsmallcaps}[1]{{\\readmdtermessmallcapsfont #1}}\n",
                "  \\fi\n\\fi\n"
            ));
        }
    }
    if core_text_used {
        // A single real face is enough for the covered scalars. Reset series
        // and shape only inside this character's group; surrounding Termes
        // small caps, emphasis, size, colour and links retain their settings.
        s.push_str("\\ifPDFTeX\n  \\PackageError{ReadMD}{Core Unicode text fallback requires XeLaTeX}{Select XeLaTeX for this document.}\n\\else\n  \\usepackage{fontspec}\n  \\ExplSyntaxOn\n");
        s.push_str(&format!("  \\cs_new:Npx \\readmdcorefontfile {{ {} }}\n", font_file_tokens(opts.core_text_font.as_deref().unwrap_or_default())));
        s.push_str("  \\exp_args:NNV \\newfontface \\readmdcorefallback \\readmdcorefontfile [NFSSFamily=readmdcorefallback]\n  \\ExplSyntaxOff\n  \\DeclareRobustCommand{\\readmdunicode}[1]{{\\normalfont\\readmdcorefallback #1}}\n\\fi\n\\pdfstringdefDisableCommands{\\def\\readmdunicode#1{#1}}\n");
    }
    if let Some(geometry) = &paragraph_state.geometry {
        s.push_str(paragraphs::MACROS);
        s.push_str(&format!("\\def\\readmdpmarker{{{}}}\n", geometry.marker_for_source()));
    }
    let numbered_headings=has_heading_numbering(&doc.blocks);
    let alphabetic_pages=opts.publication_style.as_ref().is_some_and(|style|matches!(crate::publication_layout::Layout::from_style(style).page_format.as_str(),"lowerLetter"|"upperLetter"))
        ||opts.publication_sections.iter().any(|section|matches!(crate::publication_layout::Layout::from_style(&section.style).page_format.as_str(),"lowerLetter"|"upperLetter"));
    if numbered_headings||alphabetic_pages{s.push_str(ALPHABET_NUMBER_MACROS);}
    if numbered_headings{s.push_str(HEADING_COUNTER_MACROS);}
    if numbered_headings||opts.publication_style.is_some(){s.push_str(SIXTH_TOC_LEVEL);}
    if numbered_headings{s.push_str(MEASURED_TOC_NUMBERS);s.push_str(&toc_number_settings);}
    s.push_str(&format!("\\title{{{title}}}\n"));
    s.push_str(&format!("\\author{{{author}}}\n"));
    match &date {
        Some(d) => s.push_str(&format!("\\date{{{d}}}\n")),
        None => s.push_str(if opts.publication_style.is_some(){"\\date{}\n"}else{"\\date{\\today}\n"}),
    }
    s.push_str("\n\\begin{document}\n");
    if legacy_title {
        s.push_str("\\maketitle\n");
    }
    s.push_str(&cover_source);
    if opts.toc {
        // Shared H1..H6 always use section=1..readmdsubparagraph=6, including
        // book/report. A legacy chapter lane instead uses levels 0..5; when
        // no shared profile is present its existing class defaults stay intact.
        if let Some(style)=&opts.publication_style{
            s.push_str(&format!("\\setcounter{{tocdepth}}{{{}}}\n",crate::publication_layout::toc_depth(style)));
        }
        s.push_str("\\tableofcontents\n\\newpage\n");
    }
    if opts.publication_sections.is_empty(){
        if let Some(style)=&opts.publication_style{s.push_str(&shared_text_settings(style,opts.publication_legacy_line_height));}
    }
    s.push('\n');
    s.push_str(body.trim_end());
    if !opts.bib_engine.is_empty() && !opts.bibliography.is_empty() {
        if matches!(opts.bib_engine.as_str(), "natbib" | "bibtex") {
            s.push_str("\n\\bibliographystyle{plain");
            if opts.bib_engine == "natbib" { s.push_str("nat"); }
            s.push_str("}\n\\bibliography{");
            s.push_str(&opts.bibliography.iter().map(|p| escape_url(p.trim_end_matches(".bib"))).collect::<Vec<_>>().join(","));
            s.push_str("}\n");
        } else {
            s.push_str("\n\\printbibliography\n");
        }
    }
    s.push_str("\n\n\\end{document}\n");
    if let Some(geometry) = &mut paragraph_state.geometry { geometry.bind_source(&s); }
    Ok((s, core_text_used, paragraph_state.geometry))
}

/// Render only the body (no preamble).
pub fn render_body(doc: &Document, images: ImageMap) -> String {
    if let Err(code)=crate::math_word_projection::validate_document(doc,"tex"){return format!("\\PackageError{{ReadMD}}{{{}}}{{Unsupported formula paragraph properties}}\n",escape_text(&code));}
    if let Err(error)=crate::publication_blocks::validate_author_render_document(doc){return format!("\\PackageError{{ReadMD}}{{{}}}{{Unsupported publication author text}}\n",escape_text(error));}
    let original=serde_json::json!({});
    let projected=crate::publication_note_policies::validate_default_section_numbering(doc,&original)
        .and_then(|_|crate::publication_note_policies::project_legacy(doc,&original));
    let projected=match projected{Ok(style)=>style,Err(error)=>return format!("\\PackageError{{ReadMD}}{{{}}}{{Unsupported publication note policy}}\n",escape_text(&error.to_string()))};
    let (body,_,_,toc_numbers)=render_body_with_bibliography(doc,images,"biblatex",false,false,if projected!=original{Some(&projected)}else{None},0);
    if has_heading_numbering(&doc.blocks){
        let mut unused=false;
        let settings=measured_toc_settings(&toc_numbers,false,&mut unused);
        format!("{ALPHABET_NUMBER_MACROS}{HEADING_COUNTER_MACROS}{SIXTH_TOC_LEVEL}{MEASURED_TOC_NUMBERS}{settings}{body}")
    }else{body}
}

fn render_body_with_bibliography(doc: &Document, images: ImageMap, bib_engine: &str, chapter_headings: bool, core_text_fallback: bool, publication_style: Option<&serde_json::Value>, front_count: usize) -> (String, bool, paragraphs::State, TocNumbers) {
    let mut footnotes = HashMap::new();
    collect_footnotes(&doc.blocks, &mut footnotes);
    let mut w = Writer { out: String::new(), images, footnotes, fn_stack: Vec::new(), bib_engine: bib_engine.into(), chapter_headings, heading_numbering:has_heading_numbering(&doc.blocks),toc_numbers:TocNumbers::new(),publication_style: publication_style.cloned(), core_text_fallback, core_text_used: false, csl_columns_text: false, paragraphs: paragraphs::State::new(doc, front_count),author:None,author_base:crate::publication_inline_text::ResolvedTextStyle::base(11.,11.),local_pair:None };
    w.blocks(&doc.blocks, 0);
    (w.out, w.core_text_used, w.paragraphs, w.toc_numbers)
}

fn shared_text_settings(style:&serde_json::Value,legacy_line_height:f64)->String{
    let number=|value:&serde_json::Value,default:f64|value.as_f64().filter(|v|v.is_finite()).unwrap_or(default);
    let color=|value:&serde_json::Value|{let value=value.as_str().unwrap_or("#262626").trim();if value.len()==7&&value.starts_with('#')&&value[1..].bytes().all(|b|b.is_ascii_hexdigit()){value[1..].to_string()}else{"262626".into()}};
    let body=&style["typography"];let size=number(&body["size"],11.);let line=number(&body["lineHeight"],legacy_line_height);
    let alignment=match body["align"].as_str(){Some("center")=>"\\centering",Some("right")=>"\\raggedleft",Some("justify")=>"\\leftskip=0pt\\rightskip=0pt\\parfillskip=0pt plus1fil",_=>"\\raggedright"};
    let widow=if body["widowControl"].as_bool().unwrap_or(true){10000}else{0};
    let mut out=format!("% Shared publication typography, independent of class defaults.\n\\linespread{{1}}{alignment}\n\\fontsize{{{size}}}{{{}}}\\selectfont\\color[HTML]{{{}}}\n\\setlength{{\\parindent}}{{{}mm}}\\setlength{{\\parskip}}{{{}pt}}\n\\clubpenalty={widow}\\widowpenalty={widow}\\interlinepenalty={}\\relax\n\\setcounter{{secnumdepth}}{{-1}}\n\\makeatletter\n\\def\\readmdheadingfree{{\\global\\@nobreakfalse\\everypar{{}}\\penalty0 }}\n",size*line,color(&body["color"]),number(&body["firstLineIndent"],0.),number(&body["spacing"],6.),if body["keepLines"].as_bool().unwrap_or(false){10000}else{0});
    for (index,name) in ["section","subsection","subsubsection","paragraph","subparagraph","readmdsubparagraph"].iter().enumerate(){
        let level=index+1;let h=&style["headings"][format!("h{level}")];let size=number(&h["size"],14.);
        let alignment=match h["align"].as_str(){Some("center")=>"\\centering",Some("right")=>"\\raggedleft",_=>"\\raggedright"};
        // Native @startsection interprets zero after-space as a run-in heading.
        // One scaled point preserves a separate block at the requested zero
        // visible gap (1/65536 TeX point), rather than merging the next paragraph.
        let after=number(&h["after"],8.);let after=if after==0.{"1sp".into()}else{format!("{after}pt")};
        let leading=size*line;
        let bold=if h["bold"].as_bool().unwrap_or(true){"\\bfseries"}else{"\\mdseries"};
        let heading_color=color(&h["color"]);
        out.push_str(&format!("\\expandafter\\def\\csname readmdheading{level}\\endcsname{{"));
        out.push_str(&format!("\\@startsection{{{name}}}{{{level}}}{{0pt}}{{{}pt}}{{{after}}}",number(&h["before"],12.)));
        out.push('{');
        out.push_str(&format!("\\normalfont\\fontsize{{{size}}}{{{leading}}}\\selectfont{bold}\\color[HTML]{{{heading_color}}}{alignment}"));
        out.push_str("}}\n");
    }
    out.push_str("\\makeatother\n");out
}

fn render_section_body(doc:&Document,opts:&LatexOptions,images:ImageMap,fallback:bool)->(String,bool,paragraphs::State,TocNumbers){
    let mut footnotes=HashMap::new();collect_footnotes(&doc.blocks,&mut footnotes);
    let mut w=Writer{out:String::new(),images,footnotes,fn_stack:Vec::new(),bib_engine:opts.bib_engine.clone(),chapter_headings:matches!(opts.doc_class.as_str(),"report"|"book"|"ctexrep"|"ctexbook"),heading_numbering:has_heading_numbering(&doc.blocks),toc_numbers:TocNumbers::new(),publication_style:opts.publication_style.clone(),core_text_fallback:fallback,core_text_used:false,csl_columns_text:false,paragraphs:paragraphs::State::new(doc,opts.publication_front_block_count),author:None,author_base:crate::publication_inline_text::ResolvedTextStyle::base(11.,11.),local_pair:None};
    for (index,section) in opts.publication_sections.iter().enumerate(){
        let Some(blocks)=doc.blocks.get(section.block_range.clone())else{
            w.out.push_str("\\PackageError{ReadMD}{publication_section_invalid}{Invalid section range}\n");return(w.out,w.core_text_used,w.paragraphs,w.toc_numbers);
        };
        if index>0{match section.break_kind{
            crate::publication_sections::SectionBreak::Continuous=>w.out.push_str("\\par\n"),
            crate::publication_sections::SectionBreak::NextPage=>w.out.push_str("\\clearpage\n"),
            crate::publication_sections::SectionBreak::OddPage=>w.out.push_str("\\clearpage\n\\ifodd\\value{readmdphysicalpage}\\else\\hbox{}\\thispagestyle{empty}\\newpage\\fi\n"),
            crate::publication_sections::SectionBreak::EvenPage=>w.out.push_str("\\clearpage\n\\ifodd\\value{readmdphysicalpage}\\hbox{}\\thispagestyle{empty}\\newpage\\fi\n"),
        }}
        let style=&section.style;let layout=crate::publication_layout::Layout::from_style(style);
        if index==0||section.break_kind!=crate::publication_sections::SectionBreak::Continuous{
            let (width,height)=crate::export_styles::page_dimensions(style);
            let num=|key:&str,default:f64|style["page"][key].as_f64().filter(|v|v.is_finite()).unwrap_or(default);
            let left=num("marginLeft",18.)+layout.gutter;let right=num("marginRight",18.);
            let top=num("marginTop",20.);let bottom=num("marginBottom",20.);
            // geometry's newgeometry intentionally ignores paper=... . Use
            // its public layout dimensions, then update the physical paper
            // registers and the selected engine's page size explicitly.
            // XeTeX reads these dimensions at every shipout. A one-page
            // papersize special alone leaves subsequent pages (including
            // parity padding) at the preamble's original paper dimensions.
            w.out.push_str(&format!("\\newgeometry{{layoutwidth={width}mm,layoutheight={height}mm,left={left}mm,right={right}mm,top={top}mm,bottom={bottom}mm,headheight={}pt,headsep={}mm,footskip={}mm}}\n\\setlength{{\\paperwidth}}{{{width}mm}}\\setlength{{\\paperheight}}{{{height}mm}}\n\\ifPDFTeX\\pdfpagewidth=\\paperwidth\\pdfpageheight=\\paperheight\\else\\ifLuaTeX\\pagewidth=\\paperwidth\\pageheight=\\paperheight\\else\\ifXeTeX\\pdfpagewidth=\\paperwidth\\pdfpageheight=\\paperheight\\else\\special{{papersize={width}mm,{height}mm}}\\fi\\fi\\fi\n",layout.header_size*1.3,(top-layout.header_distance-layout.header_size*1.3*25.4/72.).max(0.),(bottom-layout.footer_distance).max(0.)));
        }
        // newgeometry clears the current page. Apply any colour/font settings
        // only afterwards, so a colour whatsit cannot create an undeclared
        // leading page (or an extra page after a real section boundary).
        if opts.publication_style.is_some(){w.publication_style=Some(style.clone());w.out.push_str(&shared_text_settings(style,opts.publication_legacy_line_height));}
        let page_number=match layout.page_format.as_str(){"lowerLetter"=>"\\readmdalph{\\value{page}}","upperLetter"=>"\\readmdAlph{\\value{page}}","lowerRoman"=>"\\roman{page}","upperRoman"=>"\\Roman{page}",_=>"\\arabic{page}"};
        w.out.push_str(&format!("\\renewcommand{{\\thepage}}{{{page_number}}}\n"));
        if let Some(start)=section.page_start{w.out.push_str(&format!("\\setcounter{{page}}{{{start}}}\n"));}
        let position=|value:&str|match value{"left"=>"L","right"=>"R",_=>"C"};
        let mut running=String::from("\\fancyhf{}\\renewcommand{\\headrulewidth}{0pt}\\renewcommand{\\footrulewidth}{0pt}\\setlength{\\headwidth}{\\textwidth}\n");
        for part in ["header","footer"]{
            let odd=w.text(style[part]["text"].as_str().unwrap_or(""));
            let even=w.text(style[part]["evenText"].as_str().unwrap_or(""));
            let text=if layout.different_odd_even{format!("\\ifodd\\value{{readmdphysicalpage}} {odd}\\else {even}\\fi")}else{odd};
            let number=if part=="footer"&&style["footer"]["pageNumbers"].as_bool().unwrap_or(true){if layout.page_total{" \\thepage{} / \\readmdtotalpages"}else{" \\thepage"}}else{""};
            let command=if part=="header"{"fancyhead"}else{"fancyfoot"};
            let size=if part=="header"{layout.header_size}else{layout.footer_size};
            running.push_str(&format!("\\{command}[{}]{{\\fontsize{{{size}}}{{{}}}\\selectfont {text}{number}}}\n",position(style[part]["align"].as_str().unwrap_or(if part=="header"{"left"}else{"center"})),size*1.2));
        }
        w.out.push_str(&format!("\\fancypagestyle{{readmdsection{index}}}{{{running}}}\n"));
        if index>0&&section.break_kind==crate::publication_sections::SectionBreak::Continuous{
            // A continuous boundary cannot rewrite the running matter of the
            // physical page already in progress. Last boundary on that page
            // wins for the next page; native shipout remains the authority.
            // Publish only the running-matter macros from the hook's group.
            // Globalizing all pagestyle assignments leaks TeX's temporary
            // escapechar state and corrupts every subsequent AUX command.
            w.out.push_str(&format!("\\ifdim\\pagetotal>0pt\\relax\\AddToHookNext{{shipout/after}}{{\\readmdapplypagestyle{{readmdsection{index}}}}}\\else\\pagestyle{{readmdsection{index}}}\\fi\n"));
        }else{w.out.push_str(&format!("\\pagestyle{{readmdsection{index}}}\n"));}
        if layout.hide_first{
            let header=w.text(style["header"]["firstText"].as_str().unwrap_or(""));
            let footer=w.text(style["footer"]["firstText"].as_str().unwrap_or(""));
            let first=format!("\\fancyhf{{}}\\renewcommand{{\\headrulewidth}}{{0pt}}\\renewcommand{{\\footrulewidth}}{{0pt}}\\setlength{{\\headwidth}}{{\\textwidth}}\\fancyhead[{}]{{\\fontsize{{{}}}{{{}}}\\selectfont {header}}}\\fancyfoot[{}]{{\\fontsize{{{}}}{{{}}}\\selectfont {footer}}}",position(style["header"]["align"].as_str().unwrap_or("left")),layout.header_size,layout.header_size*1.2,position(&layout.footer_align),layout.footer_size,layout.footer_size*1.2);
            w.out.push_str(&format!("\\fancypagestyle{{readmdsection{index}first}}{{{first}}}\n\\thispagestyle{{readmdsection{index}first}}\n"));
        }
        w.out.push_str(&format!("\\hypertarget{{readmd-section-{}}}{{}}\n",section.id));
        if layout.columns>1{w.out.push_str(&format!("\\setlength{{\\columnsep}}{{{}mm}}\n\\begin{{multicols}}{{{}}}\n",layout.column_gap,layout.columns));}
        w.blocks(blocks,0);
        if layout.columns>1{w.out.push_str("\\end{multicols}\n");}
    }
    (w.out,w.core_text_used,w.paragraphs,w.toc_numbers)
}

fn collect_footnotes(blocks: &[Block], into: &mut HashMap<String, Vec<Block>>) {
    for b in blocks {
        match b {
            Block::Styled { block, .. } => collect_footnotes(std::slice::from_ref(block.as_ref()), into),
            Block::FootnoteDef { label, blocks } => {
                into.insert(label.clone(), blocks.clone());
            }
            Block::Quote(inner) => collect_footnotes(inner, into),
            Block::List { items, .. } => {
                for it in items {
                    collect_footnotes(&it.blocks, into);
                }
            }
            _ => {}
        }
    }
}

impl<'a> Writer<'a> {
    fn rich_flow(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],href:Option<&str>,hanging:bool)->String{
        use crate::publication_rich::RichLayoutNode as L;
        let mut out=String::new();for node in nodes{match node{
            L::Inline{children}=>{let value=self.inlines(&crate::md_ast::rich_inlines(children));out.push_str(&if let Some(href)=href{format!("\\href{{{}}}{{{value}}}",escape_url(href))}else{value});},
            L::Group{children,..}=>out.push_str(&self.rich_flow(children,href,hanging)),
            L::Link{href,children}=>out.push_str(&self.rich_flow(children,Some(href),hanging)),
            L::Block{children,indent}=>out.push_str(&format!("\\par\\begingroup\\parindent=0pt\\hangindent={}em\\hangafter=1{}\n{}\\par\\endgroup\n",if hanging{2}else{0},if *indent{"\\advance\\leftskip by4em\\advance\\linewidth by-4em"}else{""},self.rich_flow(children,href,hanging))),
            L::Columns{left,right}=>{
                let previous = self.csl_columns_text; self.csl_columns_text = true;
                let left = self.rich_flow(left,href,false); let right = self.rich_flow(right,href,false);
                self.csl_columns_text = previous;
                out.push_str(&format!("\\readmdcslcolumns{{{left}}}{{{right}}}\n"));
            },
        }}out
    }
    fn rich_label_lines(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],out:&mut Vec<String>,line:&mut String){
        use crate::publication_rich::RichLayoutNode as L;
        fn flush(out:&mut Vec<String>,line:&mut String){if !line.is_empty(){out.push(std::mem::take(line));}}
        for node in nodes{match node{
            L::Inline{children}=>line.push_str(&self.inlines(&crate::md_ast::rich_inlines(children))),
            L::Group{children,..}|L::Link{children,..}=>self.rich_label_lines(children,out,line),
            L::Block{children,..}=>{flush(out,line);self.rich_label_lines(children,out,line);flush(out,line);},
            L::Columns{left,right}=>{flush(out,line);self.rich_label_lines(left,out,line);flush(out,line);self.rich_label_lines(right,out,line);flush(out,line);},
        }}
    }
    fn rich_labels(&mut self,nodes:&[crate::publication_rich::RichLayoutNode],out:&mut Vec<String>){
        use crate::publication_rich::RichLayoutNode as L;
        for node in nodes{match node{
            L::Columns{left,right}=>{let mut line=String::new();self.rich_label_lines(left,out,&mut line);if !line.is_empty(){out.push(line);}self.rich_labels(left,out);self.rich_labels(right,out);},
            L::Block{children,..}|L::Group{children,..}|L::Link{children,..}=>self.rich_labels(children,out),_=>(),
        }}
    }
    fn rich_bibliography(&mut self,bib:&crate::publication_rich::RichBibliography){
        let layouts=match bib.items.iter().map(|item|item.layout()).collect::<crate::ApiResult<Vec<_>>>(){
            Ok(layouts)=>layouts,Err(_)=>{self.out.push_str("\\PackageError{ReadMD}{academic_rich_budget}{Invalid citation layout}\n");return;}
        };
        self.para_break();let title=self.text(&bib.title);self.out.push_str(&format!("\\section*{{{title}}}\n\\begingroup\\linespread{{{}}}\\selectfont\\setlength{{\\parskip}}{{0pt}}\n",bib.line_spacing));
        let mut labels=Vec::new();for layout in &layouts{self.rich_labels(layout,&mut labels);}
        if !labels.is_empty(){
            self.out.push_str("\\setlength{\\readmdbiblabelwidth}{0pt}\n");
            for label in labels{self.out.push_str(&format!("\\settowidth{{\\readmdbibmeasure}}{{{label}}}\\ifdim\\readmdbibmeasure>\\readmdbiblabelwidth\\setlength{{\\readmdbiblabelwidth}}{{\\readmdbibmeasure}}\\fi\n"));}
        }
        for (index,layout) in layouts.iter().enumerate(){
            let body=self.rich_flow(layout,None,bib.hanging_indent);self.out.push_str(&format!("\\begingroup\\parindent=0pt\\hangindent={}em\\hangafter=1\n{body}\\par\\endgroup\n",if bib.hanging_indent{2}else{0}));
            if index+1<layouts.len()&&bib.entry_spacing>0{self.out.push_str(&format!("\\vspace{{{}\\baselineskip}}\n",bib.entry_spacing));}
        }
        self.out.push_str("\\par\\endgroup\n");
    }
    fn text(&mut self, text: &str) -> String {
        escape_visible_text(text, self.core_text_fallback, &mut self.core_text_used)
    }
    fn inline_text(&mut self, text: &str) -> String {
        if self.author.is_some(){return self.author_text(text,false);}
        self.inline_text_legacy(text)
    }
    fn author_text(&mut self,text:&str,code:bool)->String{
        let Some(style)=self.author else{return if code{self.text(text).replace(' ',"\\ ")}else{self.inline_text_legacy(text)};};
        if [style.size,style.size_cs].into_iter().any(|size|!size.is_finite()||size<=0.||size>4096.){self.paragraphs.fail(crate::publication_inline_text::UNSUPPORTED);return String::new();}
        let segments=match crate::publication_text_runs::split(text,&style){Ok(segments)=>segments,Err(error)=>{self.paragraphs.fail(error);return String::new();}};
        let mut out=String::new();for segment in segments{
            // This preamble does not install a bidi engine. A scoped size is
            // not evidence that native RTL order has been implemented.
            if segment.rtl{self.paragraphs.fail(format!("{}: {{\"consumer\":\"tex\",\"reason\":\"rtl\",\"coordinateSpace\":\"textUtf8Bytes\",\"from\":{},\"to\":{}}}",crate::publication_inline_text::UNSUPPORTED,segment.from,segment.to));return String::new();}
            let Some(part)=text.get(segment.from..segment.to)else{self.paragraphs.fail(crate::publication_inline_text::INVALID);return String::new();};
            let body=if code{self.text(part).replace(' ',"\\ ")}else{self.inline_text_legacy(part)};
            // Terminate the control word before the author's own text.
            // A delimiter space would swallow a whitespace-only leaf (or
            // leading space) at a link, size-axis or nested-span boundary.
            // An empty group emits no glyph, glue or extra source space.
            out.push_str(&format!("{{\\fontsize{{{}bp}}{{\\baselineskip}}\\selectfont{{}}{body}}}",segment.size));
        }out
    }
    fn author_atom(&mut self,visible:bool)->bool{
        if visible&&self.author.is_some_and(|style|style.rtl==Some(true)){
            self.paragraphs.fail(crate::publication_inline_text::UNSUPPORTED);false
        }else{true}
    }
    fn inline_text_legacy(&mut self, text: &str) -> String {
        if !self.csl_columns_text || !self.bib_engine.is_empty() {
            return citation_text(text, &self.bib_engine, self.core_text_fallback, &mut self.core_text_used);
        }
        let mut out = String::new();
        // Keep a complete non-ASCII token intact: never split combining marks,
        // surrogate scalars, emoji sequences or a font's Unicode shaping unit.
        for segment in text.split_inclusive(char::is_whitespace) {
            let token = segment.trim_end_matches(char::is_whitespace);
            if token.is_ascii() && !token.is_empty() {
                let normal = self.text(token);
                let pieces = token.chars().map(|c| format!("\\readmdcslchar{{{}}}", self.text(&c.to_string())))
                    .collect::<Vec<_>>().join("\\allowbreak{}");
                out.push_str(&format!("\\readmdcslword{{{normal}}}{{{pieces}}}"));
            } else { out.push_str(&self.text(token)); }
            out.push_str(&self.text(&segment[token.len()..]));
        }
        // Separate rich leaves may be parts of one word. A high-penalty,
        // invisible leaf boundary permits emergency wrapping without forcing
        // a glyph/space or changing links and format ownership.
        if text.chars().last().is_some_and(|c| c.is_ascii() && !c.is_ascii_whitespace()) {
            out.push_str("\\penalty1000\\relax ");
        }
        out
    }
    fn blocks(&mut self, blocks: &[Block], list_depth: usize) {
        for b in blocks {
            if crate::speech::cancelled() { self.paragraphs.fail("cancelled"); break; }
            let ordinary = self.paragraphs.ordinary;
            if self.paragraphs.is_front(b) { self.paragraphs.ordinary = false; }
            self.block(b, list_depth);
            self.paragraphs.ordinary = ordinary;
            if self.paragraphs.error.is_some() { break; }
        }
    }

    fn para_break(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            if self.out.ends_with('\n') {
                self.out.push('\n');
            } else {
                self.out.push_str("\n\n");
            }
        }
    }

    fn block(&mut self, b: &Block, list_depth: usize) {
        self.block_with_author(b,list_depth,None);
    }
    fn block_with_author(&mut self,b:&Block,list_depth:usize,caption_author:Option<crate::publication_inline_text::ResolvedTextStyle>){
        let author=self.author.take();let base=self.author_base;
        self.author_base=self.text_pair(b);
        self.author=caption_author;
        if !matches!(b,Block::Styled{..})&&crate::publication_inline_text::has_author_styles(std::slice::from_ref(b)){self.author=Some(self.author.unwrap_or(self.author_base));}
        self.block_inner(b,list_depth);
        self.author=author;self.author_base=base;
    }
    fn text_pair(&self,block:&Block)->crate::publication_inline_text::ResolvedTextStyle{
        use crate::publication_inline_text::ResolvedTextStyle;
        if let Some(pair)=self.local_pair{return pair;}
        let style=self.publication_style.as_ref();let body=style.and_then(|s|s["typography"]["size"].as_f64()).unwrap_or(11.);
        if let Some(label)=self.fn_stack.last(){let default=style.map(|s|crate::publication_layout::Layout::from_style(s).note_size).unwrap_or(9.);let policy=style.and_then(|s|crate::publication_note_policies::parse_layer(s).ok()).map(|p|p.paragraph_policy(p.kind_for_label(label))).unwrap_or_default();let size=policy.size.unwrap_or(default);return ResolvedTextStyle::base(size,policy.size_cs.unwrap_or(size));}
        let size=match block.unstyled(){Block::Heading{level,..}=>style.and_then(|s|s["headings"][format!("h{level}")]["size"].as_f64()).unwrap_or(body),Block::Table{..}=>style.and_then(|s|s["table"]["cellSize"].as_f64()).unwrap_or(10.),Block::Caption{..}=>10.,_=>body};ResolvedTextStyle::base(size,size)
    }
    fn block_inner(&mut self, b: &Block, list_depth: usize) {
        match b {
            Block::SectionStart(_)=>(),
            Block::Styled { attributes, block } => {
                let old_pair=self.local_pair;let base=self.text_pair(block);self.local_pair=Some(if let Some(size)=attributes.pointer("/text/size").and_then(serde_json::Value::as_f64){crate::publication_inline_text::ResolvedTextStyle::base(size,size)}else{base});
                if matches!(block.unstyled(),Block::Caption{..}){
                    match crate::publication_blocks::author_style(attributes){
                        // Keep independent caption axes as the reset target
                        // while all visible leaves use physical bp metrics.
                        Ok(author)if !author.is_empty()=>{let base=self.local_pair.unwrap();let caption=author.resolve(&base,&base);self.local_pair=Some(caption);self.author=Some(caption);},
                        Ok(_)=>(),Err(error)=>{self.paragraphs.fail(error);return;},
                    }
                }
                let previous = (self.paragraphs.line, self.paragraphs.size, self.paragraphs.id.clone());
                if self.paragraphs.ordinary && matches!(block.unstyled(),Block::Paragraph(_)) {
                    let global = self.publication_style.as_ref()
                        .map(crate::publication_typography::global_line_spacing).transpose();
                    let mode = global.and_then(|global| crate::publication_typography::resolve_line_spacing(
                        self.paragraphs.line.or(global.flatten()), &attributes["paragraph"], true));
                    match mode {
                        Ok(mode) => {
                            self.paragraphs.line = mode;
                            if mode.is_some() {
                                self.paragraphs.size = attributes.pointer("/text/size").and_then(serde_json::Value::as_f64).or(self.paragraphs.size);
                                self.paragraphs.id = attributes["id"].as_str().map(str::to_owned).or(self.paragraphs.id.clone());
                            }
                        },
                        Err(error) => { self.paragraphs.fail(error); return; },
                    }
                }
                if local_termes_font(attributes) {
                    self.para_break();
                    self.out.push_str("\\begingroup\\readmdtermesblockfont\n");
                    self.block_with_author(block,list_depth,if matches!(block.unstyled(),Block::Caption{..}){self.author}else{None});
                    // Paragraph metrics must be evaluated before the local
                    // family is restored, including the final list/quote line.
                    self.out.push_str("\\par\\endgroup\n\n");
                } else {
                    self.block_with_author(block,list_depth,if matches!(block.unstyled(),Block::Caption{..}){self.author}else{None});
                }
                (self.paragraphs.line, self.paragraphs.size, self.paragraphs.id) = previous;
                self.local_pair=old_pair;
            }
            Block::Heading { level, id, inlines, numbering } => {
                self.para_break();
                let heading_style = self.publication_style.as_ref().map(|style| style["headings"][format!("h{level}")].clone());
                let toc_kind=match (*level,heading_style.is_none()&&self.chapter_headings){
                    (1,true)=>"chapter",(2,true)|(1,false)=>"section",(3,true)|(2,false)=>"subsection",
                    (4,true)|(3,false)=>"subsubsection",(5,true)|(4,false)=>"paragraph",
                    (6,true)|(5,false)=>"subparagraph",_=>"readmdsubparagraph",
                };
                let cmd = if heading_style.is_some() {
                    // A Markdown heading is an unnumbered block in the shared
                    // profile, including book/report classes. A native chapter
                    // command would introduce an undeclared page break.
                    format!("\\csname readmdheading{level}\\endcsname")
                } else {
                    format!("\\{}", match (*level, self.chapter_headings) {
                        (1, true) => "chapter",
                        (2, true) | (1, false) => "section",
                        (3, true) | (2, false) => "subsection",
                        (4, true) | (3, false) => "subsubsection",
                        (5, true) | (4, false) => "paragraph",
                        _ => "subparagraph",
                    })
                };
                if heading_style.as_ref().is_some_and(|h|h["pageBreakBefore"].as_bool()==Some(true)){
                    self.out.push_str("\\newpage\n");
                }
                let text = self.inlines(inlines);
                // Moving arguments must not contain fragile commands: give the
                // TOC/bookmark a plain-text version.
                let plain_text = inline_plain(inlines);
                let plain = self.text(&plain_text);
                let pdf_plain = escape_text(&plain_text);
                if let Some(numbering)=numbering{
                    self.toc_numbers.entry(toc_kind).or_default().insert(numbering.text.clone());
                    let settings=match heading_counter_settings(*level,numbering,&mut |literal|self.text(literal)){
                        Ok(settings)=>settings,Err(error)=>{self.paragraphs.fail(error);return;},
                    };
                    self.out.push_str(&settings);
                    let suffix=match numbering.suffix.as_str(){" "=>"\\ ","\t"=>"\\hspace{2em}",_=>""};
                    let pdf_numbered=escape_text(&format!("{}{}",numbering.full_text(),plain_text));
                    // Stepping inside the starred title occurs after a book
                    // class's chapter page break. The label therefore records
                    // this native counter and its unique actual destination.
                    self.out.push_str(&format!("{cmd}*{{\\texorpdfstring{{\\readmdnumberedheadingmark{{{level}}}{{{plain}}}\\label{{{}}}\\csname thereadmdh{level}\\endcsname{{}}{suffix}{text}}}{{{pdf_numbered}}}}}\n\\readmdnumberedheadingtoc{{{toc_kind}}}{{{level}}}{{{plain}}}\n",label_key(id)));
                } else if self.heading_numbering {
                    // Explicitly unconfigured levels stay unnumbered even
                    // when the standalone document class enables secnumdepth.
                    self.out.push_str(&format!("{cmd}*{{\\texorpdfstring{{{text}}}{{{pdf_plain}}}}}\\label{{{}}}\n\\addcontentsline{{toc}}{{{toc_kind}}}{{{plain}}}\n",label_key(id)));
                } else if plain == text {
                    self.out.push_str(&format!("{cmd}{{{text}}}\\label{{{}}}\n", label_key(id)));
                } else {
                    self.out.push_str(&format!(
                        "{cmd}[{plain}]{{\\texorpdfstring{{{text}}}{{{pdf_plain}}}}}\\label{{{}}}\n",
                        label_key(id)
                    ));
                }
                if heading_style.as_ref().is_some_and(|h|h["keepNext"].as_bool()==Some(false)){
                    self.out.push_str("\\readmdheadingfree\n");
                }
                self.out.push('\n');
            }
            Block::Paragraph(inl) => {
                let global = if self.paragraphs.ordinary { self.publication_style.as_ref()
                    .map(crate::publication_typography::global_line_spacing).transpose() } else { Ok(None) };
                let mode = match global { Ok(global) => self.paragraphs.line.or(global.flatten()),
                    Err(error) => { self.paragraphs.fail(error); return; } };
                if let Some(mode) = mode {
                    self.para_break();
                    match self.typed_paragraph(inl,mode) { Ok(value) => self.out.push_str(&value),
                        Err(error) => self.paragraphs.fail(error), }
                    return;
                }
                self.para_break();
                if let [Inline::Image { src, alt, .. }] = inl.as_slice() {
                    self.figure(src, alt);
                } else {
                    let t = self.inlines(inl);
                    self.out.push_str(t.trim());
                    self.out.push_str("\n\n");
                }
            }
            Block::List { ordered, start, items } => {
                let ordinary = self.paragraphs.ordinary; let line = self.paragraphs.line;
                self.paragraphs.ordinary = false; self.paragraphs.line = None;
                self.para_break();
                if list_depth >= 4 {
                    // LaTeX nests lists at most four deep; flatten deeper ones.
                    for (n, it) in items.iter().enumerate() {
                        let mark = if *ordered { format!("{}.", start + n as u64) } else { "--".into() };
                        self.out.push_str(&format!("\\noindent\\hspace*{{{}em}}{mark} ", list_depth * 2));
                        self.item_body(&it.blocks, list_depth);
                    }
                    self.paragraphs.ordinary = ordinary; self.paragraphs.line = line;
                    return;
                }
                let env = if *ordered { "enumerate" } else { "itemize" };
                self.out.push_str(&format!("\\begin{{{env}}}\n"));
                if *ordered && *start != 1 {
                    let counter = ["enumi", "enumii", "enumiii", "enumiv"][list_depth.min(3)];
                    self.out.push_str(&format!("\\setcounter{{{counter}}}{{{}}}\n", start.saturating_sub(1)));
                }
                for it in items {
                    match it.task {
                        Some(true) => self.out.push_str("\\item[$\\boxtimes$] "),
                        Some(false) => self.out.push_str("\\item[$\\square$] "),
                        None => self.out.push_str("\\item "),
                    }
                    self.item_body(&it.blocks, list_depth + 1);
                }
                self.out.push_str(&format!("\\end{{{env}}}\n\n"));
                self.paragraphs.ordinary = ordinary; self.paragraphs.line = line;
            }
            Block::Quote(inner) => {
                let ordinary = self.paragraphs.ordinary; let line = self.paragraphs.line;
                self.paragraphs.ordinary = false; self.paragraphs.line = None;
                self.para_break();
                self.out.push_str("\\begin{quote}\n");
                self.blocks(inner, list_depth);
                trim_trailing_blank(&mut self.out);
                self.out.push_str("\n\\end{quote}\n\n");
                self.paragraphs.ordinary = ordinary; self.paragraphs.line = line;
            }
            Block::Code { lang, text } => {
                self.para_break();
                let safe = text.replace("\\end{lstlisting}", "\\end {lstlisting}");
                match listings_language(lang) {
                    Some(l) => self.out.push_str(&format!("\\begin{{lstlisting}}[language={l}]\n")),
                    None => self.out.push_str("\\begin{lstlisting}\n"),
                }
                self.out.push_str(&safe);
                self.out.push_str("\n\\end{lstlisting}\n\n");
            }
            Block::Math(m) | Block::OfficeMath {latex:m,..} => {
                self.para_break();
                self.out.push_str(&display_math(m));
                self.out.push_str("\n\n");
            }
            Block::Caption { label, inlines } => {
                self.para_break();
                let rest = self.inlines(inlines);
                let text = if self.author.is_some(){self.author_text(&label.text(),false)}else{self.text(&label.text())};
                self.out.push_str(&format!("\\phantomsection\\label{{{}}}\n\\begin{{center}}{}{}\\end{{center}}\n\n", label_key(&label.id), text, rest));
            }
            Block::Table { aligns, header, rows } => {
                self.para_break();
                self.table(aligns, header, rows);
            }
            Block::Hr => {
                self.para_break();
                self.out.push_str("\\noindent\\rule{\\linewidth}{0.4pt}\n\n");
            }
            Block::PageBreak => {
                self.para_break();
                self.out.push_str("\\newpage\n\n");
            }
            Block::Bibliography(bibliography) => {
                if !self.bib_engine.is_empty() { return; }
                if bibliography.items.iter().any(|item| !item.first_field.is_empty()
                    || item.layout().map(|flow| crate::publication_rich::layout_requires_flow(&flow)).unwrap_or(true)) {
                    // Simple CSL first-field labels use the same bounded,
                    // breakable columns as nested displays. An unbounded
                    // makebox must not consume the complete text width.
                    self.rich_bibliography(bibliography); return;
                }
                self.para_break();
                let title = self.text(&bibliography.title);
                self.out.push_str(&format!("\\section*{{{title}}}\n"));
                // CSL spacing is relative to the selected font's ordinary line
                // height. Keep the stretch and paragraph glue local, and measure
                // the extra entry gap with that same effective baselineskip.
                self.out.push_str(&format!("\\begingroup\n\\linespread{{{}}}\\selectfont\n\\setlength{{\\parskip}}{{0pt}}\n", bibliography.line_spacing));
                let entries=bibliography.items.iter().map(|item|
                    self.inlines(&crate::md_ast::rich_inlines(&item.rich))
                ).collect::<Vec<_>>();
                for (index,body) in entries.iter().enumerate() {
                    self.out.push_str(&format!("\\begingroup\\parindent=0pt\\hangindent={}em\\hangafter=1\\noindent {body}\\par\\endgroup\n",if bibliography.hanging_indent{2}else{0}));
                    if index + 1 < entries.len() && bibliography.entry_spacing > 0 {
                        self.out.push_str(&format!("\\vspace{{{}\\baselineskip}}\n", bibliography.entry_spacing));
                    }
                }
                self.out.push_str("\\par\\endgroup\n");
            }
            Block::Html(h) => {
                if crate::publication_navigation::parse_html(h).is_ok_and(|items|!items.is_empty()){return;}
                self.para_break();
                for line in h.lines() {
                    self.out.push_str("% ");
                    self.out.push_str(line);
                    self.out.push('\n');
                }
                self.out.push('\n');
            }
            // Emitted inline at their reference point.
            Block::FootnoteDef { .. } => {}
        }
    }

    fn item_body(&mut self, blocks: &[Block], depth: usize) {
        let mut first = true;
        for b in blocks {
            if first {
                if let Block::Paragraph(inl) = b {
                    let t = self.inlines(inl);
                    self.out.push_str(t.trim());
                    self.out.push('\n');
                    first = false;
                    continue;
                }
                self.out.push('\n');
            }
            first = false;
            match b {
                Block::Paragraph(inl) => {
                    let t = self.inlines(inl);
                    self.out.push('\n');
                    self.out.push_str(t.trim());
                    self.out.push('\n');
                }
                other => {
                    let before = self.out.len();
                    self.block(other, depth);
                    // keep items compact
                    if self.out.len() > before {
                        trim_trailing_blank(&mut self.out);
                        self.out.push('\n');
                    }
                }
            }
        }
        if blocks.is_empty() || !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn figure(&mut self, src: &str, alt: &str) {
        match (self.images)(src) {
            Some(path) => {
                self.out.push_str("\\begin{figure}[htbp]\n\\centering\n");
                self.out.push_str(&format!(
                    "\\includegraphics[max width=\\linewidth,max height=0.7\\textheight]{{{path}}}\n"
                ));
                if !alt.trim().is_empty() {
                    let text = self.text(alt);
                    self.out.push_str(&format!("\\caption{{{text}}}\n"));
                }
                self.out.push_str("\\end{figure}\n\n");
            }
            None => {
                let text = self.text(if alt.is_empty() { src } else { alt });
                self.out.push_str(&format!("\\textit{{[{text}]}}\n\n"));
            }
        }
    }

    fn table(&mut self, aligns: &[Align], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) {
        let ncols = header.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0)).max(1);
        let mut spec = String::from("@{}");
        for i in 0..ncols {
            let a = aligns.get(i).copied().unwrap_or(Align::None);
            spec.push_str(match a {
                Align::Center => ">{\\centering\\arraybackslash}X",
                Align::Right => ">{\\raggedleft\\arraybackslash}X",
                _ => ">{\\raggedright\\arraybackslash}X",
            });
        }
        spec.push_str("@{}");
        self.out.push_str("\\begin{center}\n");
        self.out.push_str(&format!("\\begin{{tabularx}}{{\\linewidth}}{{{spec}}}\n\\toprule\n"));
        let row_tex = |w: &mut Writer, cells: &[Vec<Inline>], bold: bool| -> String {
            let mut parts: Vec<String> = Vec::with_capacity(ncols);
            for i in 0..ncols {
                let c = cells.get(i).map(|c| w.inlines_in_cell(c)).unwrap_or_default();
                parts.push(if bold && !c.is_empty() { format!("\\textbf{{{c}}}") } else { c });
            }
            format!("{} \\\\\n", parts.join(" & "))
        };
        let h = row_tex(self, header, true);
        self.out.push_str(&h);
        self.out.push_str("\\midrule\n");
        for r in rows {
            let t = row_tex(self, r, false);
            self.out.push_str(&t);
        }
        self.out.push_str("\\bottomrule\n\\end{tabularx}\n\\end{center}\n\n");
    }

    fn inlines_in_cell(&mut self, v: &[Inline]) -> String {
        self.inlines(v).replace("\\\\\n", "\\newline ").replace('\n', " ").trim().to_string()
    }

    fn footnote_paragraph(&mut self, block: &Block) -> Option<String> {
        let previous=self.author_base;self.author_base=self.text_pair(block);
        let body=self.footnote_paragraph_inner(block);self.author_base=previous;body
    }
    fn footnote_paragraph_inner(&mut self, block: &Block) -> Option<String> {
        match block {
            Block::Paragraph(inlines) => Some(self.inlines(inlines).trim().to_string()),
            Block::Styled { attributes, block } => {
                let old_pair=self.local_pair;let old_base=self.author_base;let old_author=self.author;
                let base=self.text_pair(block);self.local_pair=Some(if let Some(size)=attributes.pointer("/text/size").and_then(serde_json::Value::as_f64){crate::publication_inline_text::ResolvedTextStyle::base(size,size)}else{base});self.author_base=self.local_pair.unwrap();if old_author.is_some(){self.author=Some(self.author_base);}
                let body=self.footnote_paragraph(block);self.local_pair=old_pair;self.author_base=old_base;self.author=old_author;let body=body?;
                if local_termes_font(attributes) {
                    Some(format!("\\begingroup\\readmdtermesblockfont {body}\\par\\endgroup"))
                } else {
                    Some(body)
                }
            }
            _ => None,
        }
    }

    fn inlines(&mut self, v: &[Inline]) -> String {
        let mut s = String::new();
        let mut text = String::new();
        for i in v {
            if let Inline::Text(t) = i { text.push_str(t); continue; }
            s.push_str(&self.inline_text(&text));
            text.clear();
            self.inline(i, &mut s);
        }
        s.push_str(&self.inline_text(&text));
        s
    }

    fn inline(&mut self, i: &Inline, s: &mut String) {
        match i {
            Inline::Text(t) => s.push_str(&self.inline_text(t)),
            Inline::Code(t) => {
                s.push_str("\\texttt{");
                s.push_str(&self.author_text(t,true));
                s.push('}');
            }
            Inline::Strong(c) => {
                let t = self.inlines(c);
                s.push_str(&format!("\\textbf{{{t}}}"));
            }
            Inline::Emph(c) => {
                let t = self.inlines(c);
                s.push_str(&format!("\\emph{{{t}}}"));
            }
            Inline::Strike(c) => {
                let t = self.inlines(c);
                s.push_str(&format!("\\sout{{{t}}}"));
            }
            Inline::Styled { format, children } => {
                use crate::publication_rich::*;
                let mut body=self.inlines(children);
                if format.font_style!=RichFontStyle::Normal { body=format!("\\textit{{{body}}}"); }
                if format.font_weight==RichFontWeight::Bold { body=format!("\\textbf{{{body}}}"); }
                if format.font_variant==RichFontVariant::SmallCaps { body=format!("\\readmdsmallcaps{{{body}}}"); }
                if format.text_decoration==RichDecoration::Underline { body=format!("\\uline{{{body}}}"); }
                match format.vertical_align { RichVerticalAlign::Baseline=>(),RichVerticalAlign::Sup=>body=format!("\\textsuperscript{{{body}}}"),RichVerticalAlign::Sub=>body=format!("\\textsubscript{{{body}}}") }
                s.push_str(&body);
            }
            Inline::AuthorStyled{style,children}=>{
                let previous=self.author;self.author=Some(style.resolve(&previous.unwrap_or(self.author_base),&self.author_base));
                if self.author.is_some_and(|style|[style.size,style.size_cs].into_iter().any(|size|!size.is_finite()||size<=0.||size>4096.)){self.paragraphs.fail(crate::publication_inline_text::UNSUPPORTED);}else{s.push_str(&self.inlines(children));}self.author=previous;
            },
            Inline::Display { display, children } => {
                let body=self.inlines(children);
                match display { crate::publication_rich::RichDisplay::Block=>s.push_str(&format!("\\par {body}\\par ")),crate::publication_rich::RichDisplay::Indent=>s.push_str(&format!("\\par\\begingroup\\leftskip=4em {body}\\par\\endgroup ")),_=>s.push_str(&body) }
            }
            Inline::Citation { source, children } => {
                if self.bib_engine.is_empty() { s.push_str(&self.inlines(children)); }
                else {
                    let command=native_citation(source,&self.bib_engine,self.core_text_fallback,&mut self.core_text_used);
                    if let Some(author)=self.author{if author.size!=author.size_cs||author.rtl==Some(true){self.paragraphs.fail(crate::publication_inline_text::UNSUPPORTED);}else{s.push_str(&format!("{{\\fontsize{{{}bp}}{{\\baselineskip}}\\selectfont {command}}}",author.size));}}else{s.push_str(&command);}
                }
            }
            Inline::Link { href, children, .. } => {
                let t = self.inlines(children);
                if let Some(anchor) = href.strip_prefix('#') {
                    let slug = percent_encoding::percent_decode_str(anchor).decode_utf8_lossy();
                    s.push_str(&format!("\\hyperref[{}]{{{t}}}", label_key(&slug)));
                } else if self.csl_columns_text || t.contains("\\fontsize{") {
                    s.push_str(&format!("\\href{{{}}}{{{t}}}", escape_url(href)));
                } else if t.is_empty()&&self.author.is_some(){
                    // An empty label displays the URL itself. That visible
                    // fallback must use the effective author style too.
                    let visible=self.author_text(href,false);
                    s.push_str(&format!("\\href{{{}}}{{{visible}}}",escape_url(href)));
                } else if t.is_empty() || inline_plain(children) == *href {
                    if self.core_text_fallback && crate::publication_fonts::needs_core_text_fallback(href) {
                        let visible = self.text(href);
                        s.push_str(&format!("\\href{{{}}}{{\\texttt{{{visible}}}}}", escape_url(href)));
                    } else {
                        s.push_str(&format!("\\url{{{}}}", escape_url(href)));
                    }
                } else {
                    s.push_str(&format!("\\href{{{}}}{{{t}}}", escape_url(href)));
                }
            }
            Inline::Image { src, alt, .. } => {
                if !self.author_atom(true){return;}
                match (self.images)(src) {
                    Some(path) => s.push_str(&format!("\\includegraphics[height=1.2em]{{{path}}}")),
                    None => {
                        let fallback=if alt.is_empty(){src}else{alt};
                        let visible=if self.author.is_some(){self.author_text(fallback,false)}else{self.text(fallback)};
                        s.push_str(&format!("\\textit{{[{visible}]}}"));
                    },
                }
            },
            Inline::Math(m) | Inline::OfficeMath {latex:m,..} => {
                if !self.author_atom(!m.trim().is_empty()){return;}
                s.push_str("\\(");
                s.push_str(m.trim());
                s.push_str("\\)");
            }
            Inline::SoftBreak => {if self.author_atom(true){s.push('\n');}},
            Inline::HardBreak => {if self.author_atom(true){s.push_str("\\\\\n");}},
            Inline::FootnoteRef(label) => {
                if self.fn_stack.contains(label) {
                    return;
                }
                if !self.author_atom(true){return;}
                match self.footnotes.get(label).cloned() {
                    Some(blocks) => {
                        self.fn_stack.push(label.clone());
                        let author=self.author.take();let base=self.author_base;let local=self.local_pair.take();self.author_base=self.text_pair(&Block::Paragraph(Vec::new()));self.local_pair=Some(self.author_base);
                        if crate::publication_inline_text::has_author_styles(&blocks){self.author=Some(self.author_base);}
                        let mut parts = Vec::new();
                        for b in &blocks {
                            if let Some(body) = self.footnote_paragraph(b) {
                                parts.push(body);
                            }
                        }
                        self.fn_stack.pop();
                        self.author=author;self.author_base=base;self.local_pair=local;
                        s.push_str(&format!("\\footnote{{{}}}", parts.join(" \\par ")));
                    }
                    None => s.push_str(&format!("\\textsuperscript{{[{}]}}", self.text(label))),
                }
            }
            // Raw inline HTML has no safe LaTeX equivalent; its text children
            // arrive as ordinary `Text` nodes and are kept.
            Inline::Html(_) => {}
        }
    }
}

/// Pandoc-style bracket citations. Only safe BibTeX keys become commands;
/// ordinary brackets and code spans remain ordinary text.
/// Native engines have different cluster contracts. Reject unrepresentable
/// combinations before any resource is copied, instead of dropping an affix.
pub fn validate_native(doc:&Document,engine:&str)->Result<(),String>{
    fn valid_spacing(blocks:&[Block])->bool {
        blocks.iter().all(|block|match block.unstyled(){
            Block::Bibliography(bibliography)=>bibliography.line_spacing>0,
            Block::List{items,..}=>items.iter().all(|item|valid_spacing(&item.blocks)),
            Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>valid_spacing(blocks),
            _=>true,
        })
    }
    if engine.is_empty() && !valid_spacing(&doc.blocks) { return Err("academic_rich_spacing".into()); }
    if engine.is_empty(){return Ok(());}
    let mut error=false;
    crate::md_ast::walk_inlines(&doc.blocks,&mut |inline|{if let Inline::Citation{source,..}=inline{
        let first=source.items.first().map(|item|&item.cite);
        if source.items.iter().any(|item|!item.cite.key.chars().all(|c|c.is_alphanumeric()||"_:.+/-".contains(c))){error=true;}
        if source.items.iter().any(|item|first.is_some_and(|f|f.mode!=item.cite.mode)){error=true;}
        // Suppression differs between style-specific starred commands. Without
        // an audited native style it cannot stand in for CSL suppress-author.
        if source.items.iter().any(|item|item.cite.mode==crate::academic::CiteMode::SuppressAuthor){error=true;}
        // Only ordinary/prose BibLaTeX clusters have an audited per-item
        // multi-command path. A shared command must not discard another item's
        // affix, including distinct formatting of otherwise identical text.
        let shared_command=engine!="biblatex"||first.is_some_and(|f|f.mode==crate::academic::CiteMode::AuthorOnly);
        if shared_command&&source.items.len()>1&&source.items.iter().any(|item|first.is_some_and(|f|f.prefix!=item.cite.prefix||f.suffix!=item.cite.suffix||f.prefix_rich!=item.cite.prefix_rich||f.suffix_rich!=item.cite.suffix_rich||engine!="biblatex"&&(f.locator!=item.cite.locator||f.kind!=item.cite.kind))){error=true;}
        if engine=="bibtex"&&source.items.iter().any(|item|item.cite.mode!=crate::academic::CiteMode::Normal||item.cite.prefix.is_some()){error=true;}
    }});
    if error{Err("academic_tex_context".into())}else{Ok(())}
}
fn native_citation(source:&crate::publication_rich::SourceCitation,engine:&str,fallback:bool,used:&mut bool)->String{
    use crate::academic::CiteMode;
    fn affix(rich:&Option<Vec<crate::publication_rich::RichNode>>,plain:&Option<String>,fallback:bool,used:&mut bool)->String{
        if let Some(nodes)=rich {
            let (text,rich_used,_,_)=render_body_with_bibliography(&Document{blocks:vec![Block::Paragraph(crate::md_ast::rich_inlines(nodes))],..Default::default()},&|_|None,"biblatex",false,fallback,None,0);
            *used |= rich_used;
            text.trim().to_string()
        }else{escape_visible_text(plain.as_deref().unwrap_or(""),fallback,used)}
    }
    fn params(cite:&crate::academic::Cite,engine:&str,fallback:bool,used:&mut bool)->String{
        let prefix=affix(&cite.prefix_rich,&cite.prefix,fallback,used);
        let mut post=String::new();
        if cite.mode!=CiteMode::AuthorOnly {if let Some(locator)=&cite.locator {let kind=cite.kind.as_deref().unwrap_or("page");if kind!="page"{post.push_str(&escape_visible_text(kind,fallback,used));post.push(' ');}post.push_str(&escape_visible_text(locator,fallback,used));}}
        let suffix=affix(&cite.suffix_rich,&cite.suffix,fallback,used);if !suffix.is_empty(){if !post.is_empty(){post.push(' ');}post.push_str(&suffix);}
        if prefix.is_empty()&&post.is_empty(){String::new()}else if engine=="bibtex"{format!("[{{{post}}}]")}else{format!("[{{{prefix}}}][{{{post}}}]")}
    }
    let Some(first)=source.items.first().map(|item|&item.cite)else{return String::new();};
    let command=match (engine,first.mode){("biblatex",CiteMode::Prose)=>"textcite",(_,CiteMode::AuthorOnly)=>"citeauthor",("natbib",CiteMode::Prose)=>"citet",("natbib",_)=>"citep",("bibtex",_)=>"cite",_=>"autocite"};
    if engine=="biblatex"&&source.items.len()>1&&first.mode!=CiteMode::AuthorOnly {
        let args=source.items.iter().map(|item|format!("{}{{{}}}",params(&item.cite,engine,fallback,used),item.cite.key)).collect::<String>();format!("\\{command}s{args}")
    }else{
        let keys=source.items.iter().map(|item|item.cite.key.as_str()).collect::<Vec<_>>().join(",");
        format!("\\{command}{}{{{keys}}}",params(first,engine,fallback,used))
    }
}

fn citation_text(text: &str, engine: &str, fallback: bool, used: &mut bool) -> String {
    let re = regex::Regex::new(r"\[\s*@([A-Za-z0-9_:.+/-]+)(?:\s*;\s*@[A-Za-z0-9_:.+/-]+)*\s*\]").expect("citation regex");
    let command = match engine { "natbib" => "citep", "bibtex" => "cite", _ => "autocite" };
    let mut out = String::new(); let mut cursor = 0;
    for m in re.find_iter(text) {
        out.push_str(&escape_visible_text(&text[cursor..m.start()],fallback,used));
        let keys = m.as_str().trim_matches(|c| c == '[' || c == ']').split(';')
            .map(|key| key.trim().trim_start_matches('@')).collect::<Vec<_>>().join(",");
        out.push_str(&format!("\\{command}{{{keys}}}")); cursor = m.end();
    }
    out.push_str(&escape_visible_text(&text[cursor..],fallback,used)); out
}

/// Display math: environments pass through, everything else goes in `\[ \]`.
fn display_math(m: &str) -> String {
    let t = m.trim();
    if t.starts_with("\\begin{") {
        t.to_string()
    } else {
        format!("\\[\n{t}\n\\]")
    }
}

fn trim_trailing_blank(s: &mut String) {
    while s.ends_with('\n') {
        s.pop();
    }
}

#[cfg(test)]
#[path = "latex_publication_profile_tests.rs"]
mod publication_profile_tests;

#[cfg(test)]
#[path = "latex_heading_numbering_tests.rs"]
mod heading_numbering_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_ast::parse;

    fn termes_block(block: Block) -> Block {
        Block::Styled {
            attributes: serde_json::json!({"text":{"font":"TeX Gyre Termes"}}),
            block: Box::new(block),
        }
    }

    #[test]
    fn local_termes_family_is_shared_by_plain_uppercase_and_small_caps() {
        use crate::publication_rich::{RichFontStyle, RichFontVariant, RichFontWeight, RichFormat};
        let mut doc = Document::default();
        for (weight, style) in [
            (RichFontWeight::Normal, RichFontStyle::Normal),
            (RichFontWeight::Bold, RichFontStyle::Normal),
            (RichFontWeight::Normal, RichFontStyle::Italic),
            (RichFontWeight::Bold, RichFontStyle::Italic),
        ] {
            for (text, variant) in [("abc", RichFontVariant::Normal),
                ("ABC", RichFontVariant::Normal), ("abc", RichFontVariant::SmallCaps)] {
                doc.blocks.push(termes_block(Block::Paragraph(vec![Inline::Styled {
                    format: RichFormat { font_weight: weight, font_style: style,
                        font_variant: variant, ..Default::default() },
                    children: vec![Inline::Text(text.into())],
                }])));
            }
        }
        assert!(!has_unsupported_local_styles(&doc));
        let tex = render_document(&doc, &LatexOptions::default(), &|_| None);
        assert_eq!(tex.matches("\\newfontfamily\\readmdtermesfont[").count(), 1);
        assert!(tex.contains("NFSSFamily=readmdtermes"));
        for face in ["regular", "bold", "italic", "bolditalic"] {
            assert_eq!(tex.matches(&format!("texgyretermes-{face}.otf")).count(), 2);
        }
        assert!(tex.contains("\\fontfamily{qtm}\\selectfont"));
        assert!(tex.contains("\\newcommand{\\readmdsmallcaps}[1]{{\\readmdtermesfont\\textsc{#1}}}"));
        assert_eq!(tex.matches("NFSSFamily=readmdtermessmallcaps,").count(), 1);
        assert!(tex.contains("Letters=SmallCaps"));
        assert!(tex.contains("\\newcommand{\\readmdsmallcaps}[1]{{\\XeTeXgenerateactualtext=1\\relax\\readmdtermessmallcapsfont #1}}"));
        assert_eq!(tex.matches("\\XeTeXgenerateactualtext=1").count(), 1,
            "Original-copy state must be scoped to the small-caps run");
        let body = tex.split_once("\\begin{document}").unwrap().1;
        assert_eq!(body.matches("\\begingroup\\readmdtermesblockfont").count(), 12);
        assert_eq!(body.matches("\\readmdsmallcaps{").count(), 4);
        assert!(body.contains("\\textbf{\\textit{ABC}}"));
        assert!(body.contains("\\readmdsmallcaps{\\textbf{\\textit{abc}}}"));
        assert!(!body.contains("\\normalfont") && !body.contains("\\fontspec"),
            "Selecting the family must preserve the run's weight, shape and size");
        let plain = render_document(&parse("Ordinary text."), &LatexOptions::default(), &|_| None);
        assert!(!plain.contains("readmdtermes") && !plain.contains("texgyretermes"));
    }

    #[test]
    fn local_termes_scope_covers_headings_lists_quotes_and_restores_after_paragraphs() {
        let mut doc = parse("Before.\n\n# Heading\n\n> Quoted text.\n\n- Item text.\n\nAfter.\n");
        assert_eq!(doc.blocks.len(), 5);
        for index in 1..4 {
            doc.blocks[index] = termes_block(doc.blocks[index].clone());
        }
        if let Block::Styled { block, .. } = &mut doc.blocks[2] {
            if let Block::Quote(blocks) = block.as_mut() {
                blocks[0] = termes_block(blocks[0].clone());
            }
        }
        let tex = render_document(&doc, &LatexOptions::default(), &|_| None);
        assert!(tex.contains("\\renewcommand{\\rmdefault}{readmdtermes}\\renewcommand{\\familydefault}{readmdtermes}\\readmdtermesfont"),
            "Section/footnote normalfont must use the local family, not the document default");
        let body = tex.split_once("\\begin{document}").unwrap().1;
        assert!(body.contains("\\begingroup\\readmdtermesblockfont\n\n\\section{Heading}"));
        assert!(body.contains("\\begin{quote}"));
        assert!(body.contains("\\begin{itemize}\n\\item Item text."));
        assert_eq!(body.matches("\\begingroup\\readmdtermesblockfont").count(), 4);
        assert_eq!(body.matches("\\par\\endgroup").count(), 4);
        let mut depth = 0i32;
        for token in regex::Regex::new(r"\\(begingroup|endgroup)\b|Before\.|After\.").unwrap().find_iter(body) {
            match token.as_str() {
                "\\begingroup" => depth += 1,
                "\\endgroup" => depth -= 1,
                "Before." | "After." => assert_eq!(depth, 0, "Surrounding paragraphs must retain their font"),
                _ => unreachable!(),
            }
            assert!(depth >= 0);
        }
        assert_eq!(depth, 0);
        assert!(!body.contains("\\setmainfont") && !body.contains("\\normalfont"));
    }

    #[test]
    fn local_termes_footnotes_keep_styled_content_cjk_smp_and_recursion_guard() {
        let style = crate::publication_blocks::comment(&serde_json::json!({"text":{"font":"TeX Gyre Termes"}})).unwrap();
        let source = format!("Before[^n].\n\n[^n]: *Note 中文 😀* [^n] {style}\n\nAfter.\n");
        let doc = parse(&source);
        let note = doc.blocks.iter().find_map(|block| match block {
            Block::FootnoteDef { blocks, .. } => Some(blocks), _ => None,
        }).unwrap();
        assert!(matches!(&note[0], Block::Styled { .. }), "The source directive must reach the typed note paragraph");
        let options = LatexOptions { bib_engine: String::new(), core_text_font: Some("paper.assets/core.ttf".into()), ..Default::default() };
        let (tex, used) = render_document_with_resources(&doc, &options, &|_| None);
        assert!(used);
        assert!(tex.contains("\\footnote{\\begingroup\\readmdtermesblockfont \\emph{Note 中文 \\readmdunicode{😀}}\\par\\endgroup}"), "{tex}");
        assert_eq!(tex.matches("\\footnote{").count(), 1);
        assert!(tex.contains("\\DeclareRobustCommand{\\readmdunicode}[1]{{\\normalfont\\readmdcorefallback #1}}"));
        assert!(tex.contains("\\usepackage[UTF8,fontset=fandol]{ctex}"));
        assert!(!has_unsupported_local_styles(&doc));

        let unsupported = source.replace("TeX Gyre Termes", "Other Font");
        let unsupported = parse(&unsupported);
        assert!(has_unsupported_local_styles(&unsupported));
        let (tex, _) = render_document_with_resources(&unsupported, &options, &|_| None);
        assert!(tex.contains("\\footnote{\\emph{Note 中文 \\readmdunicode{😀}}}"),
            "An unsupported font must not erase a wrapped footnote's content");
        assert!(!tex.contains("Other Font") && !tex.contains("readmdtermesblockfont"));
    }

    #[test]
    fn local_termes_support_reports_mixed_properties_and_never_interpolates_font_names() {
        for (attributes, unsupported, applied) in [
            (serde_json::json!({"id":"identity","text":{},"paragraph":{}}), false, false),
            (serde_json::json!({"text":{"font":"TeX Gyre Termes"}}), false, true),
            (serde_json::json!({"text":{"font":"TeX Gyre Termes","italic":false}}), true, true),
            (serde_json::json!({"text":{"font":"TeX Gyre Termes"},"paragraph":{"before":0}}), true, true),
            (serde_json::json!({"text":{"font":"Other Font"}}), true, false),
            (serde_json::json!({"text":{"font":"TeX Gyre Termes}\\input{injected}"}}), true, false),
            (serde_json::json!({"text":{"font":"TeX Gyre Termes","unknown":true}}), true, false),
        ] {
            let doc = Document { blocks: vec![Block::Quote(vec![Block::Styled {
                attributes, block: Box::new(Block::Paragraph(vec![Inline::Text("Preserved text.".into())])),
            }])], ..Default::default() };
            assert_eq!(has_unsupported_local_styles(&doc), unsupported);
            let tex = render_document(&doc, &LatexOptions::default(), &|_| None);
            assert_eq!(tex.contains("\\begingroup\\readmdtermesblockfont"), applied);
            assert!(tex.contains("Preserved text."));
            assert!(!tex.contains("Other Font") && !tex.contains("injected"));
        }
    }

    #[test]
    fn native_author_only_clusters_reject_distinct_plain_and_rich_affixes() {
        use crate::publication_rich::{RichFontStyle, RichNode, SourceCitation};
        fn typed(source: SourceCitation) -> Document {
            Document { blocks: vec![Block::Paragraph(vec![Inline::Citation {
                source, children: vec![Inline::Text("author display".into())],
            }])], ..Default::default() }
        }
        let scan = crate::publication::citations::scan(
            ":cite[author-only cf. @alpha after; cf. @beta after]", "en-US").unwrap();
        assert!(scan.diagnostics.is_empty(), "{:?}", scan.diagnostics);
        let source = scan.citations.into_iter().next().unwrap();
        assert_eq!(source.items.len(), 2);
        assert!(source.items.iter().all(|item| item.cite.mode == crate::academic::CiteMode::AuthorOnly));
        validate_native(&typed(source.clone()), "biblatex").unwrap();
        for prefix in [true, false] {
            let mut different = source.clone();
            let cite = &mut different.items[1].cite;
            if prefix { cite.prefix = Some("different".into()); }
            else { cite.suffix = Some("different".into()); }
            assert_eq!(validate_native(&typed(different), "biblatex").unwrap_err(), "academic_tex_context");

            let mut formatted = source.clone();
            let cite = &mut formatted.items[1].cite;
            let rich = if prefix { &mut cite.prefix_rich } else { &mut cite.suffix_rich };
            let rich = rich.as_mut().unwrap();
            let original_text = crate::publication_rich::plain(rich);
            for node in rich.iter_mut() {
                if let RichNode::Text { format, .. } = node {
                    format.font_style = RichFontStyle::Italic;
                    break;
                }
            }
            assert_eq!(crate::publication_rich::plain(rich), original_text);
            assert_eq!(formatted.items[0].cite.prefix, formatted.items[1].cite.prefix);
            assert_eq!(formatted.items[0].cite.suffix, formatted.items[1].cite.suffix);
            for engine in ["biblatex", "natbib"] {
                assert_eq!(validate_native(&typed(formatted.clone()), engine).unwrap_err(), "academic_tex_context");
            }
        }
        // Ordinary/prose BibLaTeX has a real per-item parameter path and remains
        // valid when each citation has a different affix.
        for mode in [crate::academic::CiteMode::Normal, crate::academic::CiteMode::Prose] {
            let mut multi = source.clone();
            for item in &mut multi.items { item.cite.mode = mode; }
            multi.items[1].cite.prefix = Some("see".into());
            multi.items[1].cite.prefix_rich = Some(vec![RichNode::text("see")]);
            let doc = typed(multi);
            validate_native(&doc, "biblatex").unwrap();
            let tex = render_document(&doc, &LatexOptions::default(), &|_| None);
            assert!(tex.contains(if mode == crate::academic::CiteMode::Normal { "\\autocites" } else { "\\textcites" }));
            assert!(tex.contains("[{see}][{after}]{beta}"), "{tex}");
        }
    }

    #[test]
    fn native_author_only_rejection_preserves_existing_tex_and_foreign_assets() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("Paper.tex");
        let assets = directory.path().join("Paper.assets");
        std::fs::write(&target, b"Previous complete TeX").unwrap();
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("foreign.bin"), b"Unrelated existing resource").unwrap();
        for citation in [
            ":cite[author-only cf. @alpha; see @beta]",
            ":cite[author-only *cf.* @alpha; **cf.** @beta]",
        ] {
            let source = format!("Visible 😀. ![image](image.png)\n\n{citation}\n");
            let error = crate::mdexport::export_tex(&source, directory.path().to_str().unwrap(),
                target.to_str().unwrap(), &serde_json::json!({"tex":{"bibEngine":"biblatex"}}), "Paper").unwrap_err();
            assert_eq!(error, "academic_tex_context");
            assert_eq!(std::fs::read(&target).unwrap(), b"Previous complete TeX");
            assert_eq!(std::fs::read(assets.join("foreign.bin")).unwrap(), b"Unrelated existing resource");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
            assert_eq!(std::fs::read_dir(&assets).unwrap().count(), 1, "No image/font/license may be staged before validation");
        }
    }

    #[test]
    fn csl_bibliography_line_and_entry_spacing_are_local_effective_line_heights() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("refs.json"), r#"[{"id":"a","type":"book","title":"First bibliography title"},{"id":"b","type":"book","title":"Second bibliography title"}]"#).unwrap();
        let source = "---\nbibliography: refs.json\ncsl: spacing.csl\n---\nBefore [@a; @b].\n\n<!--readmd:bibliography-->\n\nAfter bibliography text.\n";
        for (line, entry) in [(1, 0), (3, 2), (11, 2)] {
            let csl = format!(r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD spacing regression</title><id>https://readmd.test/spacing</id></info><citation><layout delimiter="; "><text variable="citation-number"/></layout></citation><bibliography hanging-indent="true" line-spacing="{line}" entry-spacing="{entry}"><layout><text variable="title" font-style="italic"/></layout></bibliography></style>"#);
            std::fs::write(directory.path().join("spacing.csl"), csl).unwrap();
            let (doc, _) = crate::mdexport::publication_document(source, directory.path().to_str().unwrap(), &serde_json::json!({})).unwrap();
            let bibliography = doc.blocks.iter().find_map(|block| match block.unstyled() {
                Block::Bibliography(bibliography) => Some(bibliography), _ => None,
            }).unwrap();
            assert_eq!((bibliography.line_spacing, bibliography.entry_spacing), (line, entry));
            assert_eq!(bibliography.items.len(), 2);
            validate_native(&doc, "").unwrap();
            let tex = render_document(&doc, &LatexOptions { bib_engine: String::new(), ..Default::default() }, &|_| None);
            let start = tex.find(&format!("\\begingroup\n\\linespread{{{line}}}\\selectfont\n")).unwrap();
            let after = tex.find("After bibliography text.").unwrap();
            let body = &tex[start..after];
            assert!(body.contains("\\setlength{\\parskip}{0pt}"));
            assert!(body.contains("\\hangindent=2em"));
            assert!(body.contains("\\textit{First bibliography title}"));
            assert!(body.contains("\\textit{Second bibliography title}"));
            assert!(!tex.contains("\\printbibliography"));
            let gap = format!("\\vspace{{{entry}\\baselineskip}}");
            assert_eq!(body.matches(&gap).count(), usize::from(entry > 0));
            let mut depth = 0i32;
            for part in regex::Regex::new(r"\\(begingroup|endgroup)\b").unwrap().captures_iter(body) {
                depth += if &part[1] == "begingroup" { 1 } else { -1 };
                assert!(depth >= 0, "A bibliography group cannot close its caller's scope");
            }
            assert_eq!(depth, 0, "Line stretch must not affect the following paragraph");
            if entry > 0 {
                let gap_at = body.find(&gap).unwrap();
                assert!(gap_at > body.find("First bibliography title").unwrap());
                assert!(gap_at < body.find("Second bibliography title").unwrap());
                let gap_depth = regex::Regex::new(r"\\(begingroup|endgroup)\b").unwrap()
                    .captures_iter(&body[..gap_at]).fold(0i32, |depth, part| depth + if &part[1] == "begingroup" { 1 } else { -1 });
                assert_eq!(gap_depth, 1, "Entry gap must use the still-active bibliography line stretch");
            }
        }
    }

    #[test]
    fn csl_display_bibliography_columns_measure_the_widest_styled_label_and_keep_full_width_rows() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("refs.json"), r#"[{"id":"a","type":"book","volume":"9","title":"First long bibliography entry"},{"id":"b","type":"book","volume":"100","title":"Second long bibliography entry"},{"id":"c","type":"book","title":"Full width bibliography entry"}]"#).unwrap();
        std::fs::write(directory.path().join("columns.csl"), r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original display column regression</title><id>https://readmd.test/columns</id></info><citation><layout delimiter="; "><text variable="citation-number"/></layout></citation><bibliography hanging-indent="true" line-spacing="3" entry-spacing="2"><layout><text variable="volume" prefix="[" suffix="]" font-weight="bold" display="left-margin"/><group display="right-inline"><text variable="title" font-variant="small-caps"/></group></layout></bibliography></style>"#).unwrap();
        let source = "---\nbibliography: refs.json\ncsl: columns.csl\n---\nBefore [@a; @b; @c].\n\n<!--readmd:bibliography-->\n\nAfter bibliography.\n";
        let (doc, _) = crate::mdexport::publication_document(source, directory.path().to_str().unwrap(), &serde_json::json!({})).unwrap();
        let bibliography = doc.blocks.iter().find_map(|block| match block.unstyled() {
            Block::Bibliography(bib) => Some(bib), _ => None,
        }).unwrap();
        assert_eq!(bibliography.items.iter().map(|item| crate::publication_rich::plain(&item.first_field)).collect::<Vec<_>>(), ["[9]", "[100]", ""]);
        assert!(has_small_caps(&doc), "Bibliography-only small caps must define the font macro used in labels and body");
        let options = LatexOptions { bib_engine: String::new(), ..Default::default() };
        let tex = render_document(&doc, &options, &|_| None);
        assert_eq!(tex.matches("\\newlength{\\readmdbiblabelwidth}").count(), 1);
        assert_eq!(tex.matches("\\newlength{\\readmdbibmeasure}").count(), 1);
        assert!(tex.contains("\\newcommand{\\readmdsmallcaps}"));
        let measurements = tex.lines().filter(|line| line.starts_with("\\settowidth{\\readmdbibmeasure}")).collect::<Vec<_>>();
        assert_eq!(measurements.len(), 2);
        assert!(measurements[0].contains('9') && measurements[1].contains("100"));
        assert!(measurements.iter().all(|line| line.contains("\\textbf{")), "Measure the actual styled label, not its plain Unicode length");
        assert_eq!(tex.matches("\\ifdim\\readmdbibmeasure>\\readmdbiblabelwidth").count(), 2);
        let body = tex.split_once("\\begin{document}").unwrap().1;
        assert_eq!(body.matches("\\readmdcslcolumns{").count(), 2);
        assert!(!body.contains("\\makebox[\\readmdbiblabelwidth]"));
        let full_width = body.lines().find(|line| line.contains("Full width bibliography entry")).unwrap();
        assert!(!full_width.contains("\\readmdcslcolumns") && !full_width.contains("\\readmdbibmeasure"));
        assert!(body[..body.find(full_width).unwrap()].ends_with("\\begingroup\\parindent=0pt\\hangindent=2em\\hangafter=1\n"));
        assert!(!tex.contains("\\begin{tabular"), "Long bibliography paragraphs must remain able to wrap and paginate");
        assert!(body.find("\\settowidth").unwrap() < body.find("\\readmdcslcolumns").unwrap());

        let mut unlabelled = doc.clone();
        for block in &mut unlabelled.blocks {
            if let Block::Bibliography(bib) = block {
                for item in &mut bib.items { item.first_field.clear(); }
                bib.hanging_indent = false;
            }
        }
        let tex = render_document(&unlabelled, &options, &|_| None);
        assert!(!tex.contains("\\newlength{\\readmdbiblabelwidth}") && !tex.contains("\\settowidth"));
        let rows = tex.lines().filter(|line| line.contains("bibliography entry")).collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|line| line.contains("\\hangindent=0em") && !line.contains("\\makebox")));
    }

    #[test]
    fn static_bibliography_rejects_zero_line_spacing_in_nested_blocks() {
        let doc = Document { blocks: vec![Block::Quote(vec![Block::Bibliography(
            crate::publication_rich::RichBibliography { title: "References".into(), items: Vec::new(),
                hanging_indent: false, line_spacing: 0, entry_spacing: 0, source_span: None },
        )])], ..Default::default() };
        assert_eq!(validate_native(&doc, "").unwrap_err(), "academic_rich_spacing");
    }

    #[test]
    fn smp_text_fallback_is_cmap_driven_and_preserves_tex_escaping() {
        let face = ttf_parser::Face::parse(crate::publication_fonts::CORE_TEXT_FALLBACK, 0).unwrap();
        let second = (0x10000..0x20000).filter_map(char::from_u32)
            .find(|character| *character != '😀' && face.glyph_index(*character).is_some_and(|glyph| glyph.0 != 0)).unwrap();
        let source = format!("A & %_\\{{}} α 中文 😀{second}\u{10ffff}");
        let mut used = false;
        let rendered = escape_visible_text(&source, true, &mut used);
        assert!(used);
        assert!(rendered.starts_with("A \\& \\%\\_\\textbackslash{}\\{\\} α 中文 "));
        assert!(rendered.contains("\\readmdunicode{😀}"));
        assert!(rendered.contains(&format!("\\readmdunicode{{{second}}}")));
        assert!(rendered.ends_with('\u{10ffff}'));
        assert_eq!(rendered.matches("\\readmdunicode{").count(), 2);
        let mut unused = false;
        assert_eq!(escape_visible_text(&source, false, &mut unused), escape_text(&source));
        assert!(!unused);
        assert_eq!(escape_visible_text("Latin α 中文 \u{10ffff}", true, &mut unused), "Latin α 中文 \u{10ffff}");
        assert!(!unused);
    }

    #[test]
    fn smp_font_keeps_heading_bookmarks_links_and_small_caps_local() {
        use crate::publication_rich::{RichFontVariant, RichFormat};
        let mut doc = parse("# **Head 😀**\n\n[Shown 😀](https://example.test/a%20b#target).\n");
        doc.blocks.push(Block::Paragraph(vec![Inline::Styled {
            format: RichFormat { font_variant: RichFontVariant::SmallCaps, ..Default::default() },
            children: vec![Inline::Text("Alpha 😀 Beta".into())],
        }]));
        let path = "paper %{} space.assets/readmd-core.ttf";
        let options = LatexOptions { title: "Paper 😀".into(), bib_engine: String::new(), core_text_font: Some(path.into()), ..Default::default() };
        let (tex, used) = render_document_with_resources(&doc, &options, &|_| None);
        assert!(used);
        assert!(tex.contains("\\section[Head \\readmdunicode{😀}]{\\texorpdfstring{\\textbf{Head \\readmdunicode{😀}}}{Head 😀}}"), "{tex}");
        assert!(tex.contains("\\title{Paper \\readmdunicode{😀}}"));
        assert!(tex.contains("\\href{https://example.test/a\\%20b\\#target}{Shown \\readmdunicode{😀}}"));
        assert!(tex.contains("\\readmdsmallcaps{Alpha \\readmdunicode{😀} Beta}"));
        assert!(tex.contains("texgyretermes-bold.otf") && tex.contains("texgyretermes-italic.otf") && tex.contains("texgyretermes-bolditalic.otf"));
        assert!(tex.contains("\\DeclareRobustCommand{\\readmdunicode}[1]{{\\normalfont\\readmdcorefallback #1}}"));
        assert!(tex.contains("\\pdfstringdefDisableCommands{\\def\\readmdunicode#1{#1}}"));
        assert!(!tex.contains(path), "The resource path must not become raw TeX syntax");
        assert!(tex.contains("\\exp_args:NNV \\newfontface \\readmdcorefallback \\readmdcorefontfile [NFSSFamily=readmdcorefallback]\n  \\ExplSyntaxOff"));
        assert!(!tex.contains("\\newfontface\\readmdcorefallback{\\readmdcorefontfile}"));
        let pattern = regex::Regex::new(r"\\char_generate:nn \{(\d+)\}\{(?:11|12)\}").unwrap();
        let decoded: String = pattern.captures_iter(&font_file_tokens(path)).map(|capture|
            char::from_u32(capture[1].parse().unwrap()).unwrap()).collect();
        assert_eq!(decoded, path);
    }

    #[test]
    fn core_font_filename_tokens_preserve_extension_categories_and_literal_specials() {
        let path = "Az %{}#_$&^~\\中文😀.ttf";
        let encoded = font_file_tokens(path);
        let pattern = regex::Regex::new(r"\\char_generate:nn \{(\d+)\}\{(\d+)\}").unwrap();
        let tokens: Vec<(u32, u8)> = pattern.captures_iter(&encoded).map(|capture|
            (capture[1].parse().unwrap(), capture[2].parse().unwrap())).collect();
        assert_eq!(tokens, vec![
            (65, 11), (122, 11), (32, 12), (37, 12), (123, 12), (125, 12),
            (35, 12), (95, 12), (36, 12), (38, 12), (94, 12), (126, 12),
            (92, 12), (20013, 12),
            (25991, 12), (128512, 12), (46, 12), (116, 11), (116, 11), (102, 11),
        ]);
        let decoded: String = tokens.iter().map(|(codepoint, _)| char::from_u32(*codepoint).unwrap()).collect();
        assert_eq!(decoded, path);
        assert_eq!(pattern.find_iter(&encoded).map(|hit| hit.as_str()).collect::<String>(), encoded,
            "No file-name character may escape the generated character-token sequence");
    }

    #[test]
    fn smp_font_resources_follow_visible_images_and_referenced_notes() {
        let options = LatexOptions { bib_engine: String::new(), core_text_font: Some("paper.assets/core.ttf".into()), ..Default::default() };
        let doc = parse("---\nunused: 😀\n---\n\nPlain text. ![😀](image.png)\n\n[^unused]: 😀\n\n```text\n😀\n```\n");
        let (tex, used) = render_document_with_resources(&doc, &options, &|_| Some("image.png".into()));
        assert!(!used, "Unprinted metadata, an unused note, successful inline image alt and raw listings need no fallback resource");
        assert!(!tex.contains("readmdcorefallback"));
        assert!(tex.contains("\\begin{lstlisting}\n😀\n"), "Raw code must not be rewritten as TeX commands");
        assert!(render_document_with_resources(&doc, &options, &|_| None).1, "A failed inline image visibly prints its alt");
        let referenced = parse("Plain[^used].\n\n[^used]: 😀\n");
        assert!(render_document_with_resources(&referenced, &options, &|_| None).1);
        let hidden_author = LatexOptions { author: "😀".into(), ..options.clone() };
        assert!(!render_document_with_resources(&parse("Plain"), &hidden_author, &|_| None).1);
    }

    #[test]
    fn native_citation_smp_affixes_keep_keys_and_typed_formatting() {
        use crate::publication_rich::{RichFontStyle, RichFormat, RichNode, SourceCitation, SourceItem, SourceSpan};
        let raw = "[@alpha_1]";
        let span = SourceSpan::new(raw, 0..raw.len()).unwrap();
        let mut cite = crate::academic::Cite::new("alpha_1");
        cite.mode = crate::academic::CiteMode::Prose;
        cite.locator = Some("17".into());
        cite.prefix_rich = Some(vec![RichNode::Text { text: "see 😀 &".into(), format: RichFormat { font_style: RichFontStyle::Italic, ..Default::default() } }]);
        cite.suffix = Some("after".into());
        let source = SourceCitation { span: span.clone(), items: vec![SourceItem { cite, span, fields: Default::default() }], context: Default::default() };
        let doc = Document { blocks: vec![Block::Paragraph(vec![Inline::Citation { source, children: vec![Inline::Text("unused display".into())] }])], ..Default::default() };
        let options = LatexOptions { core_text_font: Some("paper.assets/core.ttf".into()), ..Default::default() };
        let (tex, used) = render_document_with_resources(&doc, &options, &|_| None);
        assert!(used);
        assert!(tex.contains("\\textcite[{\\textit{see \\readmdunicode{😀} \\&}}][{17 after}]{alpha_1}"), "{tex}");
        assert!(!tex.contains("unused display"));
    }

    #[test]
    fn selected_bibliography_engines_and_citations_are_real_output() {
        let doc=parse("---\nbibliography:\n  - refs.bib\n---\nCitation [@alpha; @beta] and `[@literal]`.\n");
        assert_eq!(doc.meta_list("bibliography"),vec!["refs.bib"]);
        for (engine,command) in [("biblatex","autocite"),("natbib","citep"),("bibtex","cite")] {
            let tex=render_document(&doc,&LatexOptions{bib_engine:engine.into(),bibliography:vec!["refs.bib".into()],..Default::default()},&|_|None);
            assert!(tex.contains(&format!("\\{command}{{alpha,beta}}")),"{tex}");
            assert!(tex.contains("\\texttt{[@literal]}"));
            if engine=="biblatex" {assert!(tex.contains("\\addbibresource{refs.bib}"));assert!(tex.contains("\\printbibliography"));}
            else {assert!(tex.contains("\\bibliography{refs}"));}
        }
    }

    fn body(md: &str) -> String {
        render_body(&parse(md), &|_| None)
    }

    fn doc(md: &str) -> String {
        render_document(&parse(md), &LatexOptions { title: "T".into(), ..Default::default() }, &|_| None)
    }

    #[test]
    fn cjk_bold_and_code_do_not_panic() {
        let b = body("中文**粗体**与`代码`，然后*斜体*");
        assert!(b.contains("中文\\textbf{粗体}与\\texttt{代码}"), "{b}");
        assert!(b.contains("\\emph{斜体}"));
    }

    #[test]
    fn preamble_has_xelatex_and_ctex_for_cjk() {
        let d = doc("你好");
        assert!(d.starts_with("% !TEX program = xelatex"));
        assert!(d.contains("\\usepackage[UTF8,fontset=fandol]{ctex}"));
        assert!(!doc("hello").contains("{ctex}"));
    }

    #[test]
    fn ctex_classes_use_portable_fonts_without_duplicate_package_loading() {
        for class in ["ctexart", "ctexrep", "ctexbook"] {
            let options = LatexOptions { doc_class: class.into(), ..Default::default() };
            let source = render_document(&parse("# 中文书稿\n\n## 方法"), &options, &|_| None);
            assert!(source.contains(&format!("\\documentclass[11pt,a4paper,fontset=fandol]{{{class}}}")));
            assert!(!source.contains("\\usepackage[UTF8,fontset=fandol]{ctex}"));
            if class == "ctexart" {
                assert!(source.contains("\\section{中文书稿}"));
                assert!(source.contains("\\subsection{方法}"));
            } else {
                assert!(source.contains("\\chapter{中文书稿}"));
                assert!(source.contains("\\section{方法}"));
            }
        }
    }

    #[test]
    fn special_characters_are_escaped() {
        let b = body("100% & $5 # a_b {x} ~ ^");
        assert!(b.contains(r"100\% \& \$5 \# a\_b \{x\} \textasciitilde{} \textasciicircum{}"), "{b}");
    }

    #[test]
    fn lists_nest_and_keep_start_and_tasks() {
        let b = body("3. a\n4. b\n   - [x] done\n   - [ ] todo\n");
        assert!(b.contains("\\begin{enumerate}\n\\setcounter{enumi}{2}"), "{b}");
        assert!(b.contains("\\item[$\\boxtimes$] done"));
        assert!(b.contains("\\item[$\\square$] todo"));
        assert_eq!(b.matches("\\begin{itemize}").count(), 1);
    }

    #[test]
    fn table_uses_alignment() {
        let b = body("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n");
        assert!(b.contains("\\raggedright\\arraybackslash}X>{\\centering\\arraybackslash}X>{\\raggedleft\\arraybackslash}X"), "{b}");
        assert!(b.contains("\\textbf{a} & \\textbf{b} & \\textbf{c} \\\\"));
        assert!(b.contains("1 & 2 & 3 \\\\"));
    }

    #[test]
    fn links_images_math_footnotes() {
        let b = render_body(
            &parse("See [x](https://a.b/c#d) and [intro](#intro).\n\n![cap](img/a.png)\n\n$$\\frac{a}{b}$$\n\nNote[^1].\n\n[^1]: body\n"),
            &|src| if src == "img/a.png" { Some("doc.assets/img1.png".into()) } else { None },
        );
        assert!(b.contains("\\href{https://a.b/c\\#d}{x}"), "{b}");
        assert!(b.contains("\\hyperref[sec:intro]{intro}"));
        assert!(b.contains("\\includegraphics[max width=\\linewidth,max height=0.7\\textheight]{doc.assets/img1.png}"));
        assert!(b.contains("\\caption{cap}"));
        assert!(b.contains("\\[\n\\frac{a}{b}\n\\]"));
        assert!(b.contains("Note\\footnote{body}."));
    }

    #[test]
    fn unknown_code_language_has_no_language_key() {
        assert!(body("```rust\nfn x(){}\n```").contains("\\begin{lstlisting}\nfn x(){}"));
        assert!(body("```python\nx=1\n```").contains("[language=Python]"));
    }

    #[test]
    fn every_used_package_is_loaded() {
        let d = doc("~~s~~ ![i](x.png) | a |\n|---|\n| b |\n\n```py\nx\n```\n[l](http://x)");
        for (cmd, pkg) in [("\\sout", "ulem"), ("tabularx}", "tabularx"), ("\\toprule", "booktabs"), ("lstlisting", "listings"), ("\\href", "hyperref")] {
            if d.contains(cmd) {
                assert!(d.contains(&format!("{pkg}}}")), "{cmd} needs {pkg}");
            }
        }
    }

    /// Randomized: inline markup over multi-byte alphabets never panics and
    /// keeps braces balanced.
    #[test]
    fn randomized_inline_is_safe() {
        let alphabet: Vec<&str> = vec![
            "中", "文", "😀", "é", "\u{200b}", "a", " ", "*", "**", "_", "`", "$", "~~", "[", "]", "(", ")", "\\", "#", "%", "{", "}", "\n",
        ];
        let mut state: u64 = 0x5eed_1234_abcd_0001;
        let iters: usize = std::env::var("READMD_PBT_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(500);
        for _ in 0..iters {
            let mut s = String::new();
            state = splitmix64(state);
            let len = (state % 24) as usize;
            for _ in 0..len {
                state = splitmix64(state);
                s.push_str(alphabet[(state as usize) % alphabet.len()]);
            }
            let src = s.clone();
            let r = std::panic::catch_unwind(move || body(&src));
            assert!(r.is_ok(), "panic on {s:?}");
        }
    }

    /// Writes a sample `.tex` for a real XeLaTeX compile when
    /// `READMD_TEX_SAMPLE=<path>` is set.
    #[test]
    fn write_sample_when_requested() {
        let Ok(path) = std::env::var("READMD_TEX_SAMPLE") else { return };
        let md = "---\ntitle: 测试报告\nauthor: ReadMD\n---\n\n# 第一章 概述\n\n中文**粗体**、*斜体*、~~删除~~、`code_x`、100% & $5 # a_b {x}。\n见 [第一章](#第一章-概述) 与 [链接](https://example.com/a_b#c)。\n\n## 列表\n\n3. 三\n4. 四\n   - 嵌套\n   - [x] 完成\n   - [ ] 待办\n\n> 引用段落\n\n| 左 | 中 | 右 |\n|:--|:-:|--:|\n| 文本很长很长很长 | 2 | 3.14 |\n\n```python\nprint(\"你好\")\n```\n\n```rust\nfn main() {}\n```\n\n$$\n\\int_0^1 x^2\\,dx = \\frac{1}{3}\n$$\n\n行内 $e^{i\\pi}+1=0$ 与脚注[^a]。\n\n---\n\n[^a]: 这是脚注。\n";
        let tex = render_document(
            &parse(md),
            &LatexOptions { title: "测试报告".into(), author: "ReadMD".into(), toc: true, ..Default::default() },
            &|_| None,
        );
        std::fs::write(path, tex).unwrap();
    }

    fn splitmix64(mut z: u64) -> u64 {
        z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
#[path = "latex_csl_columns_tests.rs"]
mod csl_columns_tests;
