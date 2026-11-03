//! Source-bound code output uses the existing durable editor transaction.
//! Execution, insertion and saving are separate outcomes. No file is written here.
use crate::{document_transaction::TextEdit, review_api, ApiResult, App};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{ops::Range, sync::Arc};

const START: &str = "<!-- code_chunk_output -->";
const END: &str = "<!-- /code_chunk_output -->";
const INVALID: &str = "review_invalid_request";
const CONFLICT: &str = "review_conflict";
const BUDGET: &str = "review_budget";

#[cfg(test)]
#[path = "code_chunk_output_tests.rs"]
mod tests;
#[path = "code_chunk_batch.rs"]
mod batch;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Target {
    document_id: String,
    generation: u64,
    review_revision: u64,
    content_revision: String,
    node_id: String,
    source_line: usize,
    from: usize,
    to: usize,
    code: String,
}
fn integer(value: &Value) -> Result<u64, &'static str> {
    value.as_u64().ok_or(INVALID)
}
fn text(value: &Value) -> Result<&str, &'static str> {
    value.as_str().ok_or(INVALID)
}
fn revision(source: &str) -> String {
    crate::document_transaction::revision(source)
}

// Parser events, rather than raw substring searches, keep literal markers in
// code examples inert. A complete generated output is never executable source.
fn output_ranges(content: &str) -> Result<Vec<Range<usize>>, &'static str> {
    if content.len() > 4 * 1024 * 1024 {
        return Err(BUDGET);
    }
    let mut start = None;
    let mut ranges = Vec::new();
    for (index, (event, span)) in Parser::new_ext(content, crate::md_ast::options())
        .into_offset_iter()
        .enumerate()
    {
        if index >= 262_144 {
            return Err(BUDGET);
        }
        if index % 1024 == 0 && crate::speech::cancelled() {
            return Err("cancelled");
        }
        if !matches!(event, Event::Html(_) | Event::InlineHtml(_)) {
            continue;
        }
        match content[span.clone()].trim_end_matches('\n') {
            START => {
                if start.replace(span.start).is_some() {
                    return Err(CONFLICT);
                }
            }
            END => ranges.push(start.take().ok_or(CONFLICT)?..span.end),
            _ => (),
        }
    }
    if start.is_some() {
        return Err(CONFLICT);
    }
    Ok(ranges)
}

fn fence(content: &str, line: usize, expected: &str) -> Result<Range<usize>, &'static str> {
    if content.len() > 4 * 1024 * 1024
        || expected.chars().count() > crate::code_chunk_runner::parity_code::MAX_CODE_CHARS
    {
        return Err(BUDGET);
    }
    if line == 0 {
        return Err(INVALID);
    }
    // Resolve the requested source line once. Counting every preceding newline
    // for every fence makes a long file with many blocks quadratic to prepare.
    let line_start = if line == 1 {
        0
    } else {
        content
            .match_indices('\n')
            .nth(line - 2)
            .map(|(at, _)| at + 1)
            .ok_or(CONFLICT)?
    };
    let line_end = content[line_start..]
        .find('\n')
        .map_or(content.len(), |at| line_start + at + 1);
    if output_ranges(content)?
        .iter()
        .any(|range| range.start <= line_start && line_start < range.end)
    {
        return Err(CONFLICT);
    }
    let mut containers = 0usize;
    let mut selected: Option<(Range<usize>, String)> = None;
    for (index, (event, span)) in Parser::new_ext(content, crate::md_ast::options())
        .into_offset_iter()
        .enumerate()
    {
        if index >= 262_144 {
            return Err(BUDGET);
        }
        if index % 1024 == 0 && crate::speech::cancelled() {
            return Err("cancelled");
        }
        match event {
            Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_),
            ) => containers += 1,
            Event::End(
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition,
            ) => containers = containers.saturating_sub(1),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
                if containers == 0 && span.start >= line_start && span.start < line_end =>
            {
                selected = Some((span, String::new()))
            }
            Event::Text(value) => {
                if let Some((_, code)) = selected.as_mut() {
                    code.push_str(&value);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((range, code)) = selected.take() {
                    if code.strip_suffix('\n').unwrap_or(&code) != expected {
                        return Err(CONFLICT);
                    }
                    let source = &content[range.clone()];
                    let opening = source
                        .lines()
                        .next()
                        .ok_or(CONFLICT)?
                        .trim_start_matches(' ');
                    let delimiter = opening.as_bytes().first().copied().ok_or(CONFLICT)?;
                    let width = opening.bytes().take_while(|b| *b == delimiter).count();
                    let closing = source
                        .strip_suffix('\n')
                        .unwrap_or(source)
                        .rsplit('\n')
                        .next()
                        .ok_or(CONFLICT)?
                        .trim_matches(' ');
                    if width < 3
                        || !matches!(delimiter, b'`' | b'~')
                        || closing.len() < width
                        || !closing.bytes().all(|b| b == delimiter)
                        || source.lines().count() < 2
                    {
                        return Err(CONFLICT);
                    }
                    return Ok(range);
                }
            }
            _ => (),
        }
    }
    Err(CONFLICT)
}
fn target(source: &Value, line: usize, code: &str) -> Result<Target, &'static str> {
    let content = text(&source["content"])?;
    let range = fence(content, line, code)?;
    let document_id = text(&source["document_id"])?.to_owned();
    let generation = integer(&source["generation"])?;
    let review_revision = integer(&source["review_revision"])?;
    let from = content[..range.start].encode_utf16().count();
    let to = content[..range.end].encode_utf16().count();
    let content_revision = revision(content);
    let node_id = revision(&format!("readmd-code-v1:{document_id}:{generation}:{review_revision}:{from}:{to}:{content_revision}"));
    Ok(Target {
        document_id,
        generation,
        review_revision,
        content_revision,
        node_id,
        source_line: line,
        from,
        to,
        code: code.into(),
    })
}
fn plan(content: &str, target: &Target, output: &str) -> Result<Vec<TextEdit>, &'static str> {
    if output.chars().count() > crate::code_chunk_runner::MAX_OUTPUT_CHARS * 2 {
        return Err(BUDGET);
    }
    if output.contains(START) || output.contains(END) {
        return Err(CONFLICT);
    }
    let mut from = crate::document_transaction::byte_offset(content, target.to)?;
    if content.as_bytes().get(from) == Some(&b'\n') {
        from += 1;
    }
    let mut cursor = from;
    for _ in 0..3 {
        let tail = &content[cursor..];
        let line = tail.split('\n').next().unwrap_or("");
        if !line.trim_matches(' ').is_empty() || cursor == content.len() {
            break;
        }
        cursor += line.len() + usize::from(tail.contains('\n'));
    }
    let mut to = from;
    if content[cursor..].split('\n').next() == Some(START) {
        let mut found = false;
        let begin = cursor + START.len() + 1;
        if begin > content.len() {
            return Err(CONFLICT);
        }
        let mut at = begin;
        for line in content[begin..].split_inclusive('\n') {
            if line.trim_end_matches('\n') == START {
                return Err(CONFLICT);
            }
            at += line.len();
            if line.trim_end_matches('\n') == END {
                to = at;
                found = true;
                break;
            }
        }
        if !found {
            return Err(CONFLICT);
        }
        // Only the structurally adjacent marker block is eligible. Never find
        // a marker elsewhere in the document or across a paragraph boundary.
    }
    let prefix = if from > 0 && content.as_bytes()[from - 1] != b'\n' {
        "\n\n"
    } else {
        "\n"
    };
    let insert = format!("{prefix}{START}\n\n{output}\n\n{END}\n");
    if content[from..to] == insert {
        return Ok(vec![]);
    }
    Ok(vec![TextEdit {
        from: content[..from].encode_utf16().count(),
        to: content[..to].encode_utf16().count(),
        insert,
    }])
}

pub(crate) fn handle(app: &Arc<App>, value: &Value) -> ApiResult<crate::server::Response> {
    let task = match value.get("task_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if id.is_empty() => None,
        Some(Value::String(id)) => Some(
            crate::cancel::register(id)
                .ok_or_else(|| crate::ApiError::bad_request("invalid_task_id"))?,
        ),
        Some(_) => return Err(crate::ApiError::bad_request("invalid_task_id")),
    };
    let outer = crate::speech::cancellation_check();
    let checker = task.as_ref().map(crate::cancel::TaskGuard::checker);
    crate::speech::with_cancel(
        Arc::new(move || outer() || checker.as_ref().is_some_and(|check| check())),
        || handle_inner(app, value, task.as_ref()),
    )
}
fn commit(task: Option<&crate::cancel::TaskGuard>) -> ApiResult<()> {
    if crate::speech::cancelled() || task.is_some_and(|task| !task.try_commit()) {
        return Err(review_api::error("cancelled"));
    }
    Ok(())
}
fn handle_inner(
    app: &Arc<App>,
    value: &Value,
    task: Option<&crate::cancel::TaskGuard>,
) -> ApiResult<crate::server::Response> {
    let source = review_api::snapshot(app, value)?;
    let action = text(&value["action"]).map_err(review_api::error)?;
    if matches!(action, "code_batch_target" | "code_batch_run" | "code_batch_output") {
        return batch::handle(app, value, source, task);
    }
    let supplied = if action == "code_output" {
        Some(
            serde_json::from_value::<Target>(value["target"].clone())
                .map_err(|_| review_api::error(INVALID))?,
        )
    } else {
        None
    };
    let output = if supplied.is_some() {
        Some(
            text(&value["output"])
                .map_err(review_api::error)?
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
        )
    } else {
        None
    };
    let call = if supplied.is_some() {
        Some(
            text(&value["call_id"])
                .map_err(review_api::error)?
                .to_owned(),
        )
    } else {
        None
    };
    let intent = supplied.as_ref().map(|target| revision(&json!({"document_id":value["document_id"],"generation":value["generation"],"review_revision":value["review_revision"],"target":target,"output":output}).to_string()));
    if let Some(call) = &call {
        if call.is_empty() || call.len() > 128 {
            return Err(review_api::error(INVALID));
        }
        let receipt = app
            .review
            .handle(&json!({"action":"receipt","document_id":source["document_id"],"call_id":call}))
            .map_err(review_api::error)?;
        if receipt["found"] == true {
            if receipt["request"]["code_output_intent_digest"].as_str() != intent.as_deref() {
                return Err(review_api::error("review_call_conflict"));
            }
            return Ok(crate::server::Response::json(&receipt["snapshot"]));
        }
    }
    if value["generation"] != source["generation"] {
        return Err(review_api::error("review_stale_generation"));
    }
    if value["review_revision"] != source["review_revision"] {
        return Err(review_api::error("review_stale_revision"));
    }
    if source.get("rename").is_some() {
        return Err(review_api::error("rename_recovery_pending"));
    }
    let selected = if let Some(target_) = supplied.as_ref() {
        let current =
            target(&source, target_.source_line, &target_.code).map_err(review_api::error)?;
        if *target_ != current {
            return Err(review_api::error(CONFLICT));
        }
        current
    } else {
        let line = usize::try_from(integer(&value["source_line"]).map_err(review_api::error)?)
            .map_err(|_| review_api::error(INVALID))?;
        target(
            &source,
            line,
            text(&value["code"]).map_err(review_api::error)?,
        )
        .map_err(review_api::error)?
    };
    if action == "code_target" {
        commit(task)?;
        let mut result = source;
        result["code_target"] =
            serde_json::to_value(&selected).map_err(|_| review_api::error(INVALID))?;
        return Ok(crate::server::Response::json(&result));
    }
    let edits = plan(
        text(&source["content"]).map_err(review_api::error)?,
        &selected,
        output
            .as_deref()
            .ok_or_else(|| review_api::error(INVALID))?,
    )
    .map_err(review_api::error)?;
    if edits.is_empty() {
        commit(task)?;
        let mut result = source;
        result["code_output_unchanged"] = json!(true);
        return Ok(crate::server::Response::json(&result));
    }
    let request = json!({"action":"edit","document_id":value["document_id"],"generation":value["generation"],"review_revision":value["review_revision"],"call_id":call,
        "origin":"editor","edits":edits,"code_output_intent_digest":intent});
    commit(task)?;
    let result = app.review.handle(&request).map_err(review_api::error)?;
    Ok(crate::server::Response::json(&result))
}
