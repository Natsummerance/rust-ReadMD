//! Frozen, offline HTML article. Preview and standalone export consume the same
//! source projection, typography, colors, author runs and footnote markup.
use super::*;

pub(super) struct Article {
    pub markdown: String,
    pub css: String,
    pub footnotes: Option<String>,
    pub author_text: crate::publication_inline_view::View,
    pub metadata: crate::publication_metadata::PublicationMetadata,
    pub sized_tables: bool,
    pub warnings: Vec<String>,
    pub options: Value,
}

pub(super) fn prepare(content: &str, base_dir: &str, options: &Value, source_name: &str) -> Result<Article, String> {
    let resolved = crate::publication_profile::resolve_for_format("html",content,options).map_err(|e|e.code)?;
    let options = resolved.values();
    crate::publication_profile::preflight("html",content,options).map_err(|e|e.code)?;
    crate::math_word_projection::validate_source(content,"html")?;
    crate::publication_typography::validate_export("html", content, options).map_err(str::to_string)?;
    crate::publication_blocks::validate_source(content).map_err(str::to_string)?;
    crate::publication_blocks::validate_geometry(content,&crate::export_styles::sanitize(options)).map_err(str::to_string)?;
    let mut warns: Vec<String> = Vec::new();
    if crate::publication_blocks::has_overrides(content){
        let unsupported=crate::publication_blocks::analyze(content)?.nodes.iter().any(|node|{
            let caption_author=node.kind=="caption"&&crate::publication_blocks::author_style(&node.style).is_ok_and(|author|!author.is_empty());
            let consumed=caption_author&&node.style.as_object().is_some_and(|style|style.keys().all(|key|matches!(key.as_str(),"id"|"text")))
                &&node.style["text"].as_object().is_some_and(|text|text.keys().all(|key|matches!(key.as_str(),"author"|"size")));
            !consumed&&node.style.as_object().is_some_and(|style|style.keys().any(|key|key!="id"))
        });
        if unsupported{warns.push("publication_local_style_unsupported".into());}
    }
    // `__init__.py:131` runs before `html_render.render`, so image warnings
    // precede the asset warnings.
    let style=crate::export_styles::sanitize(options);
    // Legacy HTML options coerced flat title/author/subject values. Keep that
    // boundary compatible; new structured metadata and document declarations
    // still use the strict shared model without stringifying people or IDs.
    let source_declared=crate::publication_metadata::from_source(content,&Value::Null,source_name).map_err(|e|e.code)?.declared;
    let legacy_html=!source_declared&&options.get("meta").and_then(Value::as_object).is_none_or(|meta|meta.keys().all(|key|matches!(key.as_str(),"title"|"author"|"subject")))&&options.pointer("/frontMatter/mode").and_then(Value::as_str).is_none_or(|mode|mode=="none");
    let mut publication_options=options.clone();if legacy_html&&publication_options.is_object(){publication_options["meta"]=serde_json::json!({});}
    let metadata=crate::publication_metadata::from_source(content,&publication_options,source_name).map_err(|e|e.code)?;
    warns.extend(crate::publication_metadata_formats::warnings(&metadata));
    let publication=crate::publication::prepare_with_options(content,base_dir,&publication_options).map_err(|e|e.code.to_string())?;
    warns.extend(publication.warnings);
    let reader_content=publication.projection.reader_content(content).map_err(|e|e.code.to_string())?;
    let reader_content=crate::cross_references::project(&reader_content).content;
    let author_text=crate::publication_inline_view::project(&reader_content,options)?;
    let reader_content=&author_text.content;
    let reader_content=section_flow_source(&reader_content,&style)?;
    let source_document = crate::md_ast::parse(content);
    if crate::publication_sections::has_sections(&source_document){warns.push("publication_layout_paged_unsupported".into());}
    let sized_tables = crate::table_geometry::document_has_track_sizes(&source_document);
    let math=crate::html_math::lower_dsl(crate::md_ast::source_without_front_matter(&reader_content),style["typography"]["size"].as_f64().unwrap_or(12.0))?;
    warns.extend(math.warnings.iter().cloned());
    let front=crate::publication_metadata::front_matter_blocks(&metadata,options);
    let front=if front.is_empty(){String::new()}else{
        crate::epub_writer::render_chapters(&crate::md_ast::Document{front_matter:None,blocks:front},"none",&mut |_|None).into_iter().map(|chapter|chapter.body).collect::<String>()
    };
    let html_markdown=if front.is_empty(){math.markdown.to_string()}else{format!("{front}\n\n{}",math.markdown)};
    let embedded_md = embed_images_for_html(&html_markdown, base_dir, &mut warns);
    let footnotes = crate::html_footnotes::render(&embedded_md)?;

    let display_options=crate::export_styles::html_display_options(options);
    let layout = options["htmlLayout"].as_str().filter(|value| matches!(*value, "reading" | "docs" | "magazine"));
    let mut css = String::new();
    if layout.is_some() {
        css.push_str(include_str!("../../../../assets/css/tokens.css"));
        css.push('\n');
    }
    css.push_str(&build_export_css(&display_options));
    if crate::table_geometry::source_candidate(&embedded_md) { css.push_str(include_str!("../../../../assets/css/table-geometry.css")); }
    if layout.is_some() {
        css.push('\n');
        css.push_str(include_str!("../../../../assets/css/export-html-layouts.css"));
    }
    if options["htmlTheme"].as_str()==Some("dark") {css.push_str("\n:not(pre) > code { color:#ff99b0; }");}
    if math.formulas>0{css.push('\n');css.push_str(crate::html_math::CSS);}
    if footnotes.is_some(){css.push('\n');css.push_str(crate::html_footnotes::CSS);}
    // A named light preset owns its text ink. Dark/sepia keep the
    // contrast chosen for that surface by the theme.
    if layout.is_some() && !matches!(options["htmlTheme"].as_str(), Some("dark" | "sepia")) {
        let ink = style_hex(style.pointer("/typography/color"), "#262626");
        css.push_str(&format!("\nbody {{ color:{}; }}", ink));
    }
    Ok(Article { markdown: embedded_md, css, footnotes, author_text, metadata, sized_tables, warnings: warns, options: options.clone() })
}
