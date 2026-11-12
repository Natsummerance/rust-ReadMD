//! Mirror the reader's tight CJK quotation rule without changing source offsets.
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::{collections::BTreeMap, ops::Range};

const QUOTES: [(char, char); 8] = [
    ('“', '”'), ('‘', '’'), ('「', '」'), ('『', '』'),
    ('《', '》'), ('〈', '〉'), ('【', '】'), ('（', '）'),
];

fn east_asian(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff |
        0xac00..=0xd7af | 0xf900..=0xfaff | 0x20000..=0x323af)
}

pub(crate) fn candidate(source: &str) -> bool {
    QUOTES.iter().any(|(open, _)| source.contains(&format!("**{open}")))
}

pub(crate) fn events(source: &str, options: Options) -> Vec<(Event<'_>, Range<usize>)> {
    let plain = || Parser::new_ext(source, options).into_offset_iter();
    if !candidate(source) { return plain().collect(); }

    let original: Vec<_> = plain().collect();
    let mut protected: Vec<_> = original.iter().filter_map(|(event, range)| match event {
        Event::Code(_) | Event::InlineMath(_) | Event::DisplayMath(_) | Event::Html(_) | Event::InlineHtml(_) |
        Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_) | Tag::Link { .. } | Tag::Image { .. }) =>
            Some(range.clone()),
        _ => None,
    }).collect();
    protected.sort_unstable_by_key(|range| range.start);
    let mut disjoint: Vec<Range<usize>> = Vec::new();
    for range in protected {
        if let Some(last) = disjoint.last_mut().filter(|last| last.end >= range.start) {
            last.end = last.end.max(range.end);
        } else {
            disjoint.push(range);
        }
    }
    let is_protected = |at: usize| {
        let candidate = disjoint.partition_point(|range| range.end <= at);
        disjoint.get(candidate).is_some_and(|range| range.contains(&at))
    };
    let mut points = BTreeMap::new();
    for (start, _) in source.match_indices("**") {
        if source.as_bytes().get(start.wrapping_sub(1)) == Some(&b'*') { continue; }
        let escapes = source.as_bytes()[..start].iter().rev().take_while(|&&byte| byte == b'\\').count();
        if escapes % 2 != 0 { continue; }
        let rest = &source[start + 2..];
        let Some(open) = rest.chars().next() else { continue; };
        let Some((_, close)) = QUOTES.iter().find(|(value, _)| *value == open) else { continue; };
        if is_protected(start) { continue; }
        let mut depth = 1;
        for (offset, ch) in rest[open.len_utf8()..].char_indices().take_while(|(i, _)| *i < 4096) {
            if matches!(ch, '\n' | '\r') { break; }
            let position = start + 2 + open.len_utf8() + offset;
            if ch != open && ch != *close || is_protected(position) { continue; }
            if ch == open { depth += 1; continue; }
            depth -= 1;
            if depth != 0 { continue; }
            let after_quote = position + close.len_utf8();
            let tail = &source[after_quote..];
            if let Some(after) = tail.strip_prefix("**").filter(|tail| !tail.starts_with('*')) {
                if source[..start].chars().next_back().is_some_and(east_asian)
                    || after.chars().next().is_some_and(east_asian)
                {
                    points.insert(start + 2, open);
                    points.insert(position, *close);
                }
            }
            break;
        }
    }
    if points.is_empty() { return original; }

    // Private-use characters are ordinary non-punctuation to CommonMark and
    // are exactly three UTF-8 bytes. Avoid collisions with original literals
    // and decoded entities before repairing text from the temporary parse.
    let mut unused = (0xe000..=0xf8ff).filter_map(char::from_u32).filter(|&ch|
        !source.contains(ch) && !original.iter().any(|(event, _)| matches!(event, Event::Text(value) if value.contains(ch))));
    let mut markers = BTreeMap::new();
    let mut rewritten = source.to_string();
    for (at, quote) in points {
        let marker = match markers.get(&quote) {
            Some(&marker) => marker,
            None => {
                let Some(marker) = unused.next() else { return original; };
                markers.insert(quote, marker);
                marker
            }
        };
        let mut bytes = [0; 4];
        rewritten.replace_range(at..at + quote.len_utf8(), marker.encode_utf8(&mut bytes));
    }
    let originals: BTreeMap<_, _> = markers.into_iter().map(|(quote, marker)| (marker, quote)).collect();
    Parser::new_ext(&rewritten, options).into_offset_iter().map(|(event, range)| {
        let event = match event {
            Event::Text(text) if text.chars().any(|ch| originals.contains_key(&ch)) =>
                Event::Text(text.chars().map(|ch| originals.get(&ch).copied().unwrap_or(ch)).collect::<String>().into()),
            other => other.into_static(),
        };
        (event, range)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_quotes_close_before_the_next_phrase_without_touching_examples() {
        let source = "是**“功能性框架”**：中间**实质性论点槽位**；\n\n`是**“示例”**`";
        let parsed = events(source, Options::empty());
        for (event, range) in &parsed {
            if let Event::Start(Tag::Strong) = event {
                assert!(matches!(&source[range.clone()], "**“功能性框架”**" | "**实质性论点槽位**"));
            }
        }
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parsed.into_iter().map(|(event, _)| event));
        assert_eq!(html, "<p>是<strong>“功能性框架”</strong>：中间<strong>实质性论点槽位</strong>；</p>\n<p><code>是**“示例”**</code></p>\n");
    }
}
