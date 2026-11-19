use std::path::Path;

pub(super) fn relocate_relative_links(
    markdown: String,
    source: &Path,
    output_dir: &Path,
) -> String {
    use crate::document_refactor_links::{definition_destination, inline_destination};
    use pulldown_cmark::{Event, LinkType, Parser, Tag};
    if !markdown.contains("](") && !markdown.contains("]:") {
        return markdown;
    }
    let Some(source_dir) = source.parent() else {
        return markdown;
    };
    let shared = source_dir
        .components()
        .zip(output_dir.components())
        .take_while(|(left, right)| left == right)
        .count();
    let prefix = if shared == 0 {
        let absolute = source_dir.to_string_lossy().replace('\\', "/");
        let root = if absolute.starts_with("//") {
            "//"
        } else {
            "/"
        };
        format!(
            "{root}{}/",
            crate::convert::py_url_quote(absolute.trim_matches('/'), "/:")
        )
    } else {
        let mut prefix = "../".repeat(output_dir.components().count() - shared);
        for component in source_dir.components().skip(shared) {
            prefix.push_str(&crate::convert::py_url_quote(
                &component.as_os_str().to_string_lossy(),
                "",
            ));
            prefix.push('/');
        }
        prefix
    };
    if prefix.is_empty() {
        return markdown;
    }

    let relative = |url: &str| {
        !url.is_empty()
            && !url.starts_with(['/', '\\', '#', '?'])
            && !url.split('/').next().unwrap_or("").contains(':')
    };
    let bom = if markdown.starts_with('\u{feff}') {
        3
    } else {
        0
    };
    let parser = Parser::new_ext(&markdown[bom..], crate::md_ast::options());
    let mut offsets = parser
        .reference_definitions()
        .iter()
        .filter_map(|(_, reference)| {
            if !relative(&reference.dest) {
                return None;
            }
            let span = reference.span.start + bom..reference.span.end + bom;
            definition_destination(&markdown, span, &reference.dest).map(|range| range.start)
        })
        .collect::<Vec<_>>();
    // Insert only at parser-verified URL offsets. Code, labels and titles
    // retain their bytes, and staged image URLs continue to materialize.
    for (event, span) in parser.into_offset_iter() {
        if let Event::Start(
            Tag::Link {
                link_type: LinkType::Inline,
                dest_url,
                ..
            }
            | Tag::Image {
                link_type: LinkType::Inline,
                dest_url,
                ..
            },
        ) = event
        {
            if relative(&dest_url) {
                let span = span.start + bom..span.end + bom;
                if let Some(range) = inline_destination(&markdown, span, &dest_url) {
                    offsets.push(range.start);
                }
            }
        }
    }
    if offsets.is_empty() {
        return markdown;
    }
    offsets.sort_unstable();
    offsets.dedup();
    let mut result = String::with_capacity(markdown.len() + offsets.len() * prefix.len());
    let mut previous = 0;
    for offset in offsets {
        result.push_str(&markdown[previous..offset]);
        result.push_str(&prefix);
        previous = offset;
    }
    result.push_str(&markdown[previous..]);
    result
}
