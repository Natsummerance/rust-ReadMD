//! Conservative inline delimiter completions for the reading projection.

use super::{
    is_escaped, is_punct_or_space, publication_literal_ranges, py_isspace, ReaderFixDiagnostic,
    LINE_ENDING_RE,
};
use pulldown_cmark::{Event, Options, Parser, Tag};
use regex::Regex;
use std::{collections::HashMap, ops::Range};

lazy_static::lazy_static! {
    static ref LEGACY_MATH_RE: Regex = Regex::new(r"(?s)\\\(.*?\\\)|\\\[.*?\\\]|\\begin\{[A-Za-z*]+\}.*?\\end\{[A-Za-z*]+\}").unwrap();
    static ref INLINE_LITERAL_HTML_RE: Regex = Regex::new(r"(?is)<(?:code|kbd|pre|samp|script|style|textarea)\b[^>]*>.*?(?:</(?:code|kbd|pre|samp|script|style|textarea)\s*>|\z)").unwrap();
    static ref MATH_BODY_RE: Regex = Regex::new(r"^(?:\\[A-Za-z]+|[A-Za-z\p{Greek}0-9 \t^_{}().,=+*/<>±×÷≤≥≠≈−-])+$").unwrap();
    static ref MATH_WORD_RE: Regex = Regex::new(r"\\[A-Za-z]+|[A-Za-z\p{Greek}]+").unwrap();
    static ref MATH_EVIDENCE_RE: Regex = Regex::new(r"\\[A-Za-z]+|[=+*/<>^_±×÷≤≥≠≈−-]").unwrap();
    static ref CJK_RE: Regex = Regex::new(r"[\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}]").unwrap();
}

fn options() -> Options {
    crate::md_ast::options() | Options::ENABLE_WIKILINKS
}

fn extra_literals(source: &str, ranges: &mut Vec<Range<usize>>) {
    // These are ReadMD's renderer extensions outside CommonMark inline math.
    ranges.extend(
        LEGACY_MATH_RE
            .find_iter(source)
            .filter(|m| !is_escaped(source, m.start()))
            .map(|m| m.range()),
    );
    ranges.extend(INLINE_LITERAL_HTML_RE.find_iter(source).map(|m| m.range()));
    if source.contains(":math[") {
        match crate::math_dsl::markdown_ranges(source) {
            Ok(math) => ranges.extend(math.into_iter().map(|span| span.from..span.to)),
            Err(_) => ranges.push(0..source.len()),
        }
    }
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut().filter(|last| range.start < last.end) {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

/// Shield parsed literal ranges before the existing heading and table passes.
pub(super) fn structural_literals(
    source: &str,
    mut ranges: Vec<Range<usize>>,
) -> Vec<Range<usize>> {
    extra_literals(source, &mut ranges);
    let parser = Parser::new_ext(source, options());
    ranges.extend(
        parser
            .reference_definitions()
            .iter()
            .map(|(_, definition)| definition.span.clone()),
    );
    for (event, span) in parser.into_offset_iter() {
        if matches!(
            event,
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Link { .. } | Tag::Image { .. })
                | Event::Code(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
        ) {
            ranges.push(span);
        }
    }
    merge_ranges(ranges)
}

fn find_completions(source: &str, literals: &[Range<usize>]) -> Vec<Completion> {
    let mut completions = Vec::new();
    let mut consider = |span| {
        if let Some(edit) = completion(source, span) {
            let next = literals.partition_point(|literal| literal.end <= edit.range.start);
            if !literals
                .get(next)
                .is_some_and(|literal| literal.start < edit.range.end)
            {
                completions.push(edit);
            }
        }
    };
    let mut run: Option<Range<usize>> = None;
    let mut blocked_until = 0;
    for (event, span) in Parser::new_ext(source, options()).into_offset_iter() {
        match event {
            Event::Start(
                Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::MetadataBlock(_)
                | Tag::Link { .. }
                | Tag::Image { .. }
                | Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough,
            ) => blocked_until = blocked_until.max(span.end),
            Event::Text(_) if span.start >= blocked_until => {
                if let Some(previous) = run.as_mut().filter(|previous| previous.end == span.start) {
                    previous.end = span.end;
                } else if let Some(previous) = run.replace(span) {
                    consider(previous);
                }
            }
            _ => (),
        }
    }
    if let Some(run) = run {
        consider(run);
    }
    completions
}

fn looks_like_math(body: &str) -> bool {
    // A bare dollar followed by an amount or a variable is ambiguous. Require
    // an operator or TeX command and keep surrounding prose out of the repair.
    if !MATH_EVIDENCE_RE.is_match(body) || !math_terms(body) {
        return false;
    }
    if body.starts_with(|ch: char| ch.is_ascii_digit())
        && !body.contains(['=', '^'])
        && ![" + ", " - ", " * ", " / ", " × ", " ÷ "]
            .iter()
            .any(|op| body.contains(op))
    {
        return false;
    }
    true
}

fn math_terms(body: &str) -> bool {
    MATH_BODY_RE.is_match(body)
        && MATH_WORD_RE.find_iter(body).all(|term| {
            let word = term.as_str();
            word.starts_with('\\')
                || word.chars().count() == 1
                || (word.len() == 2 && body[term.end()..].starts_with('^'))
                || matches!(
                    word,
                    "sin" | "cos" | "tan" | "log" | "ln" | "exp" | "lim" | "min" | "max"
                )
        })
}

fn has_multiline_dollar_close(tail: &str) -> bool {
    let tail = tail.trim_start_matches([' ', '\t']);
    let Some(continuation) = tail
        .strip_prefix("\r\n")
        .or_else(|| tail.strip_prefix(['\r', '\n']))
    else {
        return false;
    };
    // A continuation such as ` + 1$` can be parsed as a Markdown list item.
    // Treat the first dollar on uninterrupted equation lines as a possible
    // closer. A blank line or non-formula text ends the search.
    for line in LINE_ENDING_RE.split(continuation) {
        if let Some((body, after)) = line.split_once('$') {
            if body.trim().is_empty() {
                return after.trim().is_empty();
            }
            return math_terms(body.trim_start())
                && !body.ends_with(py_isspace)
                && !is_escaped(line, body.len())
                && !after.starts_with(|ch: char| ch == '$' || ch.is_ascii_digit());
        }
        if !math_terms(line.trim()) {
            break;
        }
    }
    false
}

fn cjk(ch: char) -> bool {
    !ch.is_ascii() && CJK_RE.is_match(ch.encode_utf8(&mut [0; 4]))
}

struct Completion {
    range: Range<usize>,
    marker: &'static str,
}

fn completion(source: &str, span: Range<usize>) -> Option<Completion> {
    // A parsed link, code span, or other inline may interrupt a text run. End
    // at a real line/inline boundary, including heading and table punctuation.
    let tail = source[span.end..].trim_start_matches([' ', '\t']);
    if !tail.is_empty()
        && !tail.starts_with(['\r', '\n', '|'])
        && !tail.starts_with("\\\n")
        && !tail.starts_with("\\\r")
        && !tail.starts_with('#')
    {
        return None;
    }
    let raw = &source[span.clone()];
    let mut opening = None;
    for (offset, delimiter) in raw.match_indices(['*', '$']) {
        let start = span.start + offset;
        let glyph = delimiter.as_bytes()[0];
        if start
            .checked_sub(1)
            .is_some_and(|before| source.as_bytes()[before] == glyph)
            || is_escaped(source, start)
        {
            continue;
        }
        let run = source[start..]
            .bytes()
            .take_while(|byte| *byte == glyph)
            .count();
        let marker = match (glyph, run) {
            (b'*', 1) => "*",
            (b'*', 2) => "**",
            (b'$', 1) => "$",
            _ => continue,
        };
        let before = source[..start].chars().next_back();
        let after = source[start + run..].chars().next();
        if before.is_some_and(|ch| {
            matches!(ch, '/' | '\\' | '.' | '=' | '@' | '$' | '*')
                || (!is_punct_or_space(ch)
                    && !(cjk(ch) && (marker == "$" || after.is_some_and(cjk))))
        }) || !after.is_some_and(|ch| {
            ch.is_alphabetic()
                || (marker != "*" && ch.is_ascii_digit())
                || (marker == "$" && ch == '\\')
        }) {
            continue;
        }
        if opening.replace((start, marker)).is_some() {
            return None;
        }
    }
    let (start, marker) = opening?;
    let body = source
        .get(start + marker.len()..span.end)?
        .trim_end_matches(py_isspace);
    let body = if marker == "$" {
        body.trim_end_matches([
            '.', ',', ';', ':', '!', '?', '。', '，', '；', '：', '！', '？',
        ])
        .trim_end_matches(py_isspace)
    } else {
        body
    };
    let end = start + marker.len() + body.len();
    if body.is_empty() || body.contains(['\r', '\n']) || is_escaped(source, end) {
        return None;
    }
    if marker == "$" {
        if !looks_like_math(body) || has_multiline_dollar_close(&source[end..]) {
            return None;
        }
    } else {
        let first = body
            .split_whitespace()
            .next()?
            .trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if body.contains(['*', '$', '`', '|', '=']) || first.contains(['.', '/', '\\', '@']) {
            return None;
        }
        // In loose multiplication, an omitted space after the operator does
        // not turn it into a Markdown opener (e.g. 2 *x).
        if marker == "*"
            && source[..start]
                .trim_end_matches([' ', '\t'])
                .ends_with(|ch: char| ch.is_ascii_digit())
        {
            return None;
        }
    }
    Some(Completion {
        range: start..end,
        marker,
    })
}

pub(super) fn close_inline(
    source: String,
    fixes: &mut Vec<String>,
    stats: &mut HashMap<&'static str, usize>,
    diagnostics: &mut Vec<ReaderFixDiagnostic>,
) -> String {
    if !source.contains(['*', '$']) {
        return source;
    }
    let mut literals = publication_literal_ranges(&source);
    extra_literals(&source, &mut literals);
    let completions = find_completions(&source, &merge_ranges(literals));
    if completions.is_empty() {
        return source;
    }

    let mut output = String::with_capacity(source.len());
    let mut expected = Vec::with_capacity(completions.len());
    let mut cursor = 0;
    for edit in &completions {
        let start = output.len() + edit.range.start - cursor;
        output.push_str(&source[cursor..edit.range.end]);
        output.push_str(edit.marker);
        cursor = edit.range.end;
        expected.push((start..output.len(), edit.marker));
    }
    output.push_str(&source[cursor..]);

    // Confirm the new closer actually pairs with its intended opener in the
    // full source. Delimiter interactions can invalidate an isolated guess.
    let mut expected = expected.into_iter().peekable();
    for (event, span) in Parser::new_ext(&output, options()).into_offset_iter() {
        let marker = match event {
            Event::Start(Tag::Strong) => "**",
            Event::Start(Tag::Emphasis) => "*",
            Event::InlineMath(_) => "$",
            _ => continue,
        };
        if expected.peek() == Some(&(span, marker)) {
            expected.next();
        }
    }
    if expected.next().is_some() {
        return source;
    }

    let mut line = 1;
    let mut endings = LINE_ENDING_RE.find_iter(&source).peekable();
    for edit in completions {
        while endings
            .peek()
            .is_some_and(|ending| ending.end() <= edit.range.start)
        {
            endings.next();
            line += 1;
        }
        if edit.marker == "$" {
            fixes.push(format!("[公式] 第 {line} 行：补全未闭合的行内公式 $"));
            *stats.entry("math").or_insert(0) += 1;
            diagnostics.push(ReaderFixDiagnostic::InlineMath { line });
        } else {
            fixes.push(format!("[加粗] 第 {line} 行：补全未闭合的 {}", edit.marker));
            *stats.entry("bold").or_insert(0) += 1;
            diagnostics.push(ReaderFixDiagnostic::InlineDelimiter {
                line,
                delimiter: edit.marker,
            });
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use crate::readmd_fix::fix_markdown_for_reader;

    #[test]
    fn reader_closes_visible_openers_without_rewriting_source_bytes() {
        let source = concat!(
            "\u{feff}正文**明显漏掉加粗  \r\n\r\n",
            "* item with *missing italic\n\n",
            "公式$E=mc^2。\r\n",
            "TeX $\\frac{a}{b}\r",
            "Equation $x_1",
        );
        let expected = concat!(
            "\u{feff}正文**明显漏掉加粗**  \r\n\r\n",
            "* item with *missing italic*\n\n",
            "公式$E=mc^2$。\r\n",
            "TeX $\\frac{a}{b}$\r",
            "Equation $x_1$",
        );
        let result = fix_markdown_for_reader(source);
        assert_eq!(result.text, expected);
        assert_eq!(result.stats["bold"], 2);
        assert_eq!(result.stats["math"], 3);
        for (fix, line) in result.fixes.iter().zip([1, 3, 5, 6, 7]) {
            assert!(fix.contains(&format!("第 {line} 行")), "{fix}");
        }
        let repeated = fix_markdown_for_reader(&result.text);
        assert_eq!(repeated.text, expected);
        assert!(repeated.fixes.is_empty(), "{:?}", repeated.fixes);
    }

    #[test]
    fn reader_keeps_literal_tokens_and_recognized_markdown() {
        for source in [
            "**[title]** and **valid** and *valid*\n\n**multi\nline** and *more\nlines*",
            "2 * 3 = 6, 2 *x, and selector *.md\n\nUse *glob.txt as an example.",
            "* * *\n\n* item\n+ item\n- item\n\n> * quoted item",
            "a $ b and only $5\n\nUS$15.00 and $20-30\n\n$HOME or $folder_name\n\n$10 for C:\\path_to_file",
            "\\*literal and \\**escaped; \\$x^2\n\ntrailing closer* and **",
            "[link](https://example.org/$abc*path)\n\n[x]: https://example.org/**example\n\n![image](./$x^2)",
            "`**literal $x^2\n*multiline`\n\n    **indented $x^2\n\n- item\n\n  ```md\n  #Literal **example $x^2\n  ```",
            "ordinary <code>*literal\n$x^2</code>\n\n<div>\n#Literal **example\n</div>",
            "valid $x^2 + 1$ and $x^2\n + 1$\n\n$$\nx_1\n$$\n\n$$\ny_1\n$$",
            "\\(a *x\\)\n\n\\[\n*x\n\\]\n\n:math[frac(2 *x, y)]",
            "---\ntitle: '#Literal **example $x^2'\n---\n\n[see **word @paper]",
        ] {
            let result = fix_markdown_for_reader(source);
            assert_eq!(result.text, source, "{:?}", result.fixes);
            assert!(result.fixes.is_empty(), "{source:?}: {:?}", result.fixes);
        }
    }

    #[test]
    fn reader_checks_for_math_continuations_within_the_paragraph_boundary() {
        let source = "$x^2\r\n + 1\r\n - 2$\r\n\r\nEquation $a = b\n\nA literal $";
        let result = fix_markdown_for_reader(source);
        assert_eq!(
            result.text,
            "$x^2\r\n + 1\r\n - 2$\r\n\r\nEquation $a = b$\n\nA literal $"
        );
        assert_eq!(result.stats["math"], 1);
        assert_eq!(result.fixes, ["[公式] 第 5 行：补全未闭合的行内公式 $"]);
    }

    #[test]
    fn reader_keeps_existing_table_and_heading_repairs_with_local_line_endings() {
        let source = "#Heading\r\n\n| Column |\r\n| Value |\r\n\r\n**unclosed\n";
        let result = fix_markdown_for_reader(source);
        assert_eq!(
            result.text,
            "# Heading\r\n\n| Column |\r\n| --- |\r\n| Value |\r\n\r\n**unclosed**\n"
        );
        assert_eq!(result.stats["heading"], 1);
        assert_eq!(result.stats["table"], 1);
        assert_eq!(result.stats["bold"], 1);
    }
}
