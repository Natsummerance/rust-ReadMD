//! Plan a sibling-section move against the exact review generation. The caller
//! submits it through the normal manual-edit FIFO, including undo and recovery.
use crate::{document_transaction, md_ast, publication_sections, review_api, ApiResult, App};
use pulldown_cmark::{Event, Parser, Tag};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

struct Heading {
    level: u8,
    start: usize,
    title_start: usize,
}

// The Markdown parser handles LF and CRLF; converting only bare CR keeps every
// byte offset valid for slicing the original source, including mixed endings.
fn parser_source(text: &str) -> std::borrow::Cow<'_, str> {
    let bare_cr = |i: usize| text.as_bytes().get(i + 1) != Some(&b'\n');
    if text.match_indices('\r').any(|(i, _)| bare_cr(i)) {
        text.char_indices()
            .map(|(i, c)| if c == '\r' && bare_cr(i) { '\n' } else { c })
            .collect::<String>()
            .into()
    } else {
        text.into()
    }
}

fn headings(text: &str) -> Result<Vec<Heading>, &'static str> {
    let normalized = parser_source(text);
    let text = normalized.as_ref();
    let mut out = Vec::new();
    let mut depth = 0;
    let declarations = publication_sections::analyze(text)?;
    for (event, range) in Parser::new_ext(text, md_ast::options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Tag::Heading { level, .. } = tag {
                    if depth == 0 {
                        let title_start =
                            text[..range.start].rfind(['\n', '\r']).map_or(0, |n| n + 1);
                        let mut start = title_start;
                        // A section declaration belongs to the following content.
                        let preceding =
                            declarations.partition_point(|d| d.bytes.end <= title_start);
                        for declaration in declarations[..preceding].iter().rev() {
                            if text[declaration.bytes.end..start].trim().is_empty() {
                                start = declaration.bytes.start;
                            } else {
                                break;
                            }
                        }
                        out.push(Heading {
                            level: level as u8,
                            start,
                            title_start,
                        });
                    }
                }
                depth += 1;
            }
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    Ok(out)
}

/// Read and select use the same section ownership as a chapter move.
pub(crate) struct Chapter {
    pub bytes: std::ops::Range<usize>,
    pub title_start: usize,
    pub level: u8,
}

pub(crate) fn chapter_at(text: &str, byte: usize) -> Result<Option<Chapter>, &'static str> {
    if !text.is_char_boundary(byte) {
        return Err("review_invalid_request");
    }
    let headings = headings(text)?;
    let Some(i) = headings.partition_point(|h| h.start <= byte).checked_sub(1) else {
        return Ok(None);
    };
    let h = &headings[i];
    let end = headings[i + 1..]
        .iter()
        .find(|next| next.level <= h.level)
        .map_or(text.len(), |next| next.start);
    Ok(Some(Chapter {
        bytes: h.start..end,
        title_start: h.title_start,
        level: h.level,
    }))
}

/// Lines keep their original separators in the returned slice, including CR-only files.
pub(crate) fn chapter_page(
    text: &str,
    at: usize,
    first: usize,
    last: usize,
) -> Result<Value, &'static str> {
    if at == 0 || first == 0 || last < first || last - first >= 1000 {
        return Err("invalid_line_range");
    }
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    for (i, &b) in bytes.iter().enumerate() {
        if i + 1 < bytes.len() && (b == b'\n' || (b == b'\r' && bytes[i + 1] != b'\n')) {
            starts.push(i + 1);
        }
    }
    let byte = *starts.get(at - 1).ok_or("line_out_of_range")?;
    let chapter = chapter_at(text, byte)?.ok_or("chapter_not_found")?;
    let start_line = starts.partition_point(|n| *n <= chapter.bytes.start);
    let title_line = starts.partition_point(|n| *n <= chapter.title_start);
    let end_line = starts.partition_point(|n| *n < chapter.bytes.end);
    let count = end_line - start_line + 1;
    if first > count {
        return Err("line_out_of_range");
    }
    let start_index = (start_line - 1) + (first - 1);
    let end_index = start_line - 1 + last.min(count);
    let end_byte = starts.get(end_index).copied().unwrap_or(text.len());
    Ok(
        json!({"chapter_start_line":start_line,"title_line":title_line,"chapter_end_line":end_line,
        "level":chapter.level,"start_line":start_index+1,"end_line":end_index,
        "total_lines":starts.len(),"next_start_line":(last<count).then(||last+1),
        "content":&text[starts[start_index]..end_byte]}),
    )
}

// Moving the end of a file to its middle can swallow a heading in an open
// fence, turn two paragraphs into one or change a reference-style link's URL.
// Compare parsed events in their intended order before offering the edit.
fn preserves_blocks(before: &str, after: &str, bounds: [usize; 3]) -> Result<bool, &'static str> {
    let before = parser_source(before);
    let after = parser_source(after);
    let mut groups = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for (count, (event, span)) in Parser::new_ext(&before, md_ast::options())
        .into_offset_iter()
        .enumerate()
    {
        if count >= 200_000 {
            return Err("review_budget");
        }
        groups[bounds.partition_point(|&at| at <= span.start)].push(event);
    }
    let [prefix, left, right, suffix] = groups;
    Ok(prefix
        .into_iter()
        .chain(right)
        .chain(left)
        .chain(suffix)
        .eq(Parser::new_ext(&after, md_ast::options())))
}

#[derive(Serialize, Debug)]
struct Move {
    from: usize,
    to: usize,
    insert: String,
    anchor: usize,
}

fn plan(text: &str, caret: usize, down: bool) -> Result<Option<Move>, &'static str> {
    let cursor = document_transaction::byte_offset(text, caret)?;
    let headings = headings(text)?;
    let Some(current) = headings
        .partition_point(|h| h.start <= cursor)
        .checked_sub(1)
    else {
        return Ok(None);
    };
    let level = headings[current].level;
    let next = |from: usize| (from + 1..headings.len()).find(|&i| headings[i].level <= level);
    let adjacent = if down {
        next(current)
    } else {
        (0..current).rev().find(|&i| headings[i].level <= level)
    };
    let Some(adjacent) = adjacent.filter(|&i| headings[i].level == level) else {
        return Ok(None);
    };
    let (left, right) = (current.min(adjacent), current.max(adjacent));
    let start = headings[left].start;
    let middle = headings[right].start;
    let end = next(right).map_or(text.len(), |i| headings[i].start);
    let first = &text[start..middle];
    let second = &text[middle..end];
    // Keep the separators at their positions, including a missing final LF.
    let first_body = first.trim_end_matches(['\r', '\n']);
    let second_body = second.trim_end_matches(['\r', '\n']);
    let middle_break = &first[first_body.len()..];
    let end_break = &second[second_body.len()..];
    let insert = [second_body, middle_break, first_body, end_break].concat();
    let after = [&text[..start], &insert, &text[end..]].concat();
    if !preserves_blocks(text, &after, [start, middle, end])? {
        return Err("author_inline_text_unsupported");
    }
    let from = text[..start].encode_utf16().count();
    let to = from + text[start..end].encode_utf16().count();
    let moved_heading = if down {
        second_body.len() + middle_break.len()
    } else {
        0
    } + headings[current].title_start
        - headings[current].start;
    let anchor = from + insert[..moved_heading].encode_utf16().count();
    Ok(Some(Move {
        from,
        to,
        insert,
        anchor,
    }))
}

pub(crate) fn request(app: &Arc<App>, body: &Value) -> ApiResult<Value> {
    let snapshot = review_api::snapshot(app, body)?;
    for (key, code) in [
        ("generation", "review_stale_generation"),
        ("review_revision", "review_stale_revision"),
    ] {
        if body[key] != snapshot[key] {
            return Err(review_api::error(code));
        }
    }
    let invalid = || review_api::error("review_invalid_request");
    let text = snapshot["content"].as_str().ok_or_else(invalid)?;
    let caret = body["caret"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(invalid)?;
    if body["action"] == "chapter_range" {
        let offset = document_transaction::byte_offset(text, caret).map_err(review_api::error)?;
        let chapter = chapter_at(text, offset)
            .map_err(review_api::error)?
            .map(|chapter| {
                let from = text[..chapter.bytes.start].encode_utf16().count();
                let to = from + text[chapter.bytes].encode_utf16().count();
                json!({"from":from,"to":to,"level":chapter.level})
            });
        return Ok(
            json!({"document_id":snapshot["document_id"],"generation":snapshot["generation"],
            "review_revision":snapshot["review_revision"],"chapter":chapter}),
        );
    }
    let down = match body["direction"].as_str() {
        Some("down") => true,
        Some("up") => false,
        _ => return Err(invalid()),
    };
    let edit = plan(text, caret, down).map_err(review_api::error)?;
    if let Some(edit) = &edit {
        let (from, to) = (edit.from as u64, edit.to as u64);
        for change in snapshot["changes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["decision"] != "withdrawn")
        {
            let (a, b) = (
                change["from"].as_u64().ok_or_else(invalid)?,
                change["to"].as_u64().ok_or_else(invalid)?,
            );
            if (a == b && (from..=to).contains(&a)) || (a < to && from < b) {
                return Err(review_api::error("review_conflict"));
            }
        }
    }
    Ok(
        json!({"document_id":snapshot["document_id"],"generation":snapshot["generation"],"review_revision":snapshot["review_revision"],"edit":edit}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apply(source: &str, m: &Move) -> String {
        let from = document_transaction::byte_offset(source, m.from).unwrap();
        let to = document_transaction::byte_offset(source, m.to).unwrap();
        [&source[..from], &m.insert, &source[to..]].concat()
    }

    #[test]
    fn duplicate_headings_move_by_utf16_with_children_and_without_final_newline() {
        let source = "---\ntitle: 试验\n---\n\n# 书📚\n\n## 章\n甲\n\n### 图\n> # 引文\n\n~~~\n# 示例\n~~~\n\n## 章\n乙";
        let m = plan(source, source.encode_utf16().count(), false)
            .unwrap()
            .unwrap();
        let moved = apply(source, &m);
        assert_eq!(moved, "---\ntitle: 试验\n---\n\n# 书📚\n\n## 章\n乙\n\n## 章\n甲\n\n### 图\n> # 引文\n\n~~~\n# 示例\n~~~");
        let undo_move = plan(&moved, m.anchor, true).unwrap().unwrap();
        assert_eq!(apply(&moved, &undo_move), source);
    }

    #[test]
    fn section_declarations_travel_with_their_heading() {
        let a = "<!-- readmd-section:v1 {\"id\":\"a\"} -->\n\n# A\nText";
        let b = "<!-- readmd-section:v1 {\"id\":\"b\"} -->\n\nB\n=====\nText";
        let source = format!("{a}\n\n{b}\n");
        let m = plan(&source, 0, true).unwrap().unwrap();
        assert_eq!(apply(&source, &m), format!("{b}\n\n{a}\n"));
    }

    #[test]
    fn refuses_missing_sibling_open_fence_and_changed_global_reference_resolution() {
        assert!(plan("# A\n## Only\n# B\n", 7, true).unwrap().is_none());
        assert!(plan("# A\n\n# B\n~~~\nstill open", 1, true).is_err());
        let source = "# A\n\n[x][ref]\n\n[ref]: /a\n\n# B\n\n[ref]: /b\n";
        assert!(plan(source, 1, true).is_err());
        assert!(plan("📚\n\n# A\n# B", 1, true).is_err());
    }

    #[test]
    fn chapter_pages_keep_exact_source_and_do_not_treat_nested_markup_as_chapters() {
        for eol in ["\n", "\r\n", "\r"] {
            let lines = [
                "---",
                "title: Example",
                "---",
                "# Same",
                "A",
                "# Same",
                "😀",
                "~~~",
                "# fenced",
                "~~~",
                "> # quoted",
                "## Child",
                "Body",
                "# End",
            ];
            let source = lines.join(eol);
            let page = chapter_page(&source, 9, 2, 5).unwrap();
            assert_eq!(page["title_line"], 6);
            assert_eq!(page["chapter_end_line"], 13);
            assert_eq!(page["next_start_line"], 6);
            assert_eq!(page["content"], format!("{}{eol}", lines[6..10].join(eol)));
            let child = chapter_page(&source, 13, 1, 1000).unwrap();
            assert_eq!(child["level"], 2);
            assert_eq!(child["content"], format!("## Child{eol}Body{eol}"));
            assert_eq!(chapter_page(&source, 14, 1, 1).unwrap()["content"], "# End");
            assert_eq!(
                chapter_page(&source, 2, 1, 2).unwrap_err(),
                "chapter_not_found"
            );
        }
    }

    #[test]
    fn http_plan_uses_current_working_revision_and_preserves_pending_review() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::AppPaths::with_dirs(
            &tmp.path().join("profile"),
            tmp.path(),
            &tmp.path().join("assets"),
        );
        let app = Arc::new(App::bootstrap(paths).unwrap());
        let call =
            |body: Value| crate::server::call_in_process(&app, "POST", "/api/review", Some(&body));
        let (status, opened) = call(
            json!({"action":"open","path":"draft:chapters","content":"# A\nfirst\n\n# B\nsecond"}),
        );
        assert_eq!(status, 200);
        let mut body = json!({"action":"chapter_move_plan","document_id":opened["document_id"],"generation":opened["generation"],"review_revision":opened["review_revision"],"direction":"up","caret":12});
        let (status, p) = call(body.clone());
        assert_eq!(status, 200);
        assert_eq!(p["edit"]["insert"], "# B\nsecond\n\n# A\nfirst");
        let (status, changed) = call(
            json!({"action":"propose","document_id":opened["document_id"],"generation":opened["generation"],"review_revision":opened["review_revision"],"call_id":"chapter-proposal","origin":"ai","edits":[{"from":4,"to":9,"insert":"new"}]}),
        );
        assert_eq!(status, 200);
        assert_eq!(call(body.clone()).0, 409);
        body["generation"] = changed["generation"].clone();
        body["review_revision"] = changed["review_revision"].clone();
        let mut read = body.clone();
        read["action"] = json!("chapter_range");
        let (status, selection) = call(read.clone());
        assert_eq!(status, 200);
        assert_eq!(selection["chapter"], json!({"from":9,"to":19,"level":1}));
        read["generation"] = json!(0);
        assert_eq!(call(read).0, 409);
        let (status, conflict) = call(body);
        assert_eq!(status, 409);
        assert_eq!(conflict["error_code"], "review_conflict");
        assert_eq!(
            call(json!({"action":"read","document_id":opened["document_id"]})).1["content"],
            "# A\nnew\n\n# B\nsecond"
        );
    }
}
