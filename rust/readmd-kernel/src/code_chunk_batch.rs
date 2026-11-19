//! Bounded, source-frozen batches. The first consumer is pure in-memory SQL;
//! host runtimes need managed readiness and durable side-effect checkpoints.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
    time::{Duration, Instant},
};

const MAX_BLOCKS: usize = 32;
const MAX_CODE: usize = 200_000;
const MAX_OUTPUT: usize = 400_000;
const DEADLINE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Item {
    target: Target,
    lang: String,
    output: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Batch {
    items: Vec<Item>,
}

fn metadata(info: &str) -> Option<(String, bool)> {
    static TOKENS: OnceLock<regex::Regex> = OnceLock::new();
    let tokens =
        TOKENS.get_or_init(|| regex::Regex::new(r#"(?:[^\s"']+|"[^"]*"|'[^']*')+"#).unwrap());
    let info = info.replace(['{', '}'], " ");
    let mut words = tokens.find_iter(&info).map(|m| m.as_str());
    let language = words.next()?;
    let mut attrs = BTreeMap::new();
    let mut flags = BTreeSet::new();
    for word in words {
        let Some((key, value)) = word.split_once('=') else {
            flags.insert(word.to_ascii_lowercase());
            continue;
        };
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            &value[1..value.len().saturating_sub(1)]
        } else {
            value
        };
        attrs.insert(key.to_ascii_lowercase(), value.to_owned());
    }
    let truthy = |value: &str| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        )
    };
    let boolean = |name: &str| {
        attrs
            .get(name)
            .map_or_else(|| flags.contains(name), |value| truthy(value))
    };
    let command = attrs.get("cmd").map(String::as_str).unwrap_or("").trim();
    if matches!(
        command.to_ascii_lowercase().as_str(),
        "false" | "0" | "no" | "off"
    ) || !(boolean("cmd") || !command.is_empty() || flags.contains("cmd"))
    {
        return None;
    }
    let lang = if command.is_empty() || truthy(command) {
        language
    } else {
        command
    };
    let output = boolean("output")
        || attrs.get("output").is_some_and(|v| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on" | "text" | "markdown" | "html" | "json" | "png"
            )
        });
    Some((lang.to_owned(), output))
}

fn prepare(source: &Value) -> Result<Batch, &'static str> {
    let content = text(&source["content"])?;
    let outputs = output_ranges(content)?;
    let mut items = Vec::new();
    let mut containers = 0usize;
    let mut selected: Option<(Range<usize>, String, bool, String)> = None;
    let mut code_chars = 0usize;
    let line_starts: Vec<_> = std::iter::once(0)
        .chain(content.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
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
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                if outputs.iter().any(|range| range.contains(&span.start)) {
                    continue;
                }
                if let Some((lang, output)) = metadata(&info) {
                    if lang.len() > 128 {
                        return Err(BUDGET);
                    }
                    if containers != 0 {
                        return Err(CONFLICT);
                    }
                    selected = Some((span, lang, output, String::new()));
                }
            }
            Event::Text(code) => {
                // Capture the parser's exact normalized code, without looking
                // up a duplicate string or trusting card datasets.
                if let Some((_, _, _, text)) = selected.as_mut() {
                    text.push_str(&code);
                    if text.chars().count() > MAX_CODE {
                        return Err(BUDGET);
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((range, lang, output, code)) = selected.take() {
                    let code = code.strip_suffix('\n').unwrap_or(&code);
                    code_chars = code_chars.checked_add(code.chars().count()).ok_or(BUDGET)?;
                    if items.len() >= MAX_BLOCKS || code_chars > MAX_CODE {
                        return Err(BUDGET);
                    }
                    let line = line_starts.partition_point(|at| *at <= range.start);
                    items.push(Item {
                        target: target(source, line, code)?,
                        lang,
                        output,
                    });
                }
            }
            _ => (),
        }
    }
    Ok(Batch { items })
}

fn check_source(source: &Value, value: &Value) -> ApiResult<()> {
    if value["generation"] != source["generation"] {
        return Err(review_api::error("review_stale_generation"));
    }
    if value["review_revision"] != source["review_revision"] {
        return Err(review_api::error("review_stale_revision"));
    }
    if source.get("rename").is_some() {
        return Err(review_api::error("rename_recovery_pending"));
    }
    Ok(())
}
fn supplied(value: &Value) -> ApiResult<Batch> {
    let batch: Batch =
        serde_json::from_value(value["batch"].clone()).map_err(|_| review_api::error(INVALID))?;
    if batch.items.is_empty() || batch.items.len() > MAX_BLOCKS {
        return Err(review_api::error(BUDGET));
    }
    Ok(batch)
}
fn verify(source: &Value, batch: &Batch) -> ApiResult<()> {
    if *batch != prepare(source).map_err(review_api::error)? {
        return Err(review_api::error(CONFLICT));
    }
    Ok(())
}

pub(super) fn handle(
    app: &Arc<App>,
    value: &Value,
    source: Value,
    task: Option<&crate::cancel::TaskGuard>,
) -> ApiResult<crate::server::Response> {
    let action = text(&value["action"]).map_err(review_api::error)?;
    if action == "code_batch_target" {
        check_source(&source, value)?;
        let batch = prepare(&source).map_err(review_api::error)?;
        commit(task)?;
        let mut result = source;
        result["code_batch"] =
            serde_json::to_value(batch).map_err(|_| review_api::error(INVALID))?;
        return Ok(crate::server::Response::json(&result));
    }
    let batch = supplied(value)?;
    if action == "code_batch_run" {
        if value["confirm"] != true {
            return Err(review_api::error("confirmation_required"));
        }
        check_source(&source, value)?;
        verify(&source, &batch)?;
        // Check every runtime before executing any input. PATH discovery is
        // deliberately not presented as managed readiness or used as fallback.
        if batch.items.iter().any(|item| {
            !matches!(
                item.lang
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
                    .as_str(),
                "sql" | "sqlite" | "sqlite3"
            )
        }) {
            return Err(review_api::error("capability_pack_unavailable"));
        }
        return crate::code_chunk_runner::with_owned_execution(|| run(app, value, batch, task))
            .unwrap_or_else(|| Err(review_api::error("cancelled")));
    }
    let outputs: Vec<String> =
        serde_json::from_value(value["outputs"].clone()).map_err(|_| review_api::error(INVALID))?;
    if outputs.len() != batch.items.len()
        || outputs.iter().map(|v| v.chars().count()).sum::<usize>() > MAX_OUTPUT
    {
        return Err(review_api::error(BUDGET));
    }
    let outputs: Vec<_> = outputs
        .into_iter()
        .map(|s| s.replace("\r\n", "\n").replace('\r', "\n"))
        .collect();
    let call = text(&value["call_id"]).map_err(review_api::error)?;
    if call.is_empty() || call.len() > 128 {
        return Err(review_api::error(INVALID));
    }
    let intent = revision(&json!({"action":action,"document_id":value["document_id"],"generation":value["generation"],"review_revision":value["review_revision"],"batch":batch,"outputs":outputs}).to_string());
    let receipt = app
        .review
        .handle(&json!({"action":"receipt","document_id":source["document_id"],"call_id":call}))
        .map_err(review_api::error)?;
    if receipt["found"] == true {
        if receipt["request"]["code_output_intent_digest"] != intent {
            return Err(review_api::error("review_call_conflict"));
        }
        return Ok(crate::server::Response::json(&receipt["snapshot"]));
    }
    check_source(&source, value)?;
    verify(&source, &batch)?;
    let content = text(&source["content"]).map_err(review_api::error)?;
    let mut edits = Vec::new();
    for (item, output) in batch.items.iter().zip(outputs) {
        if item.output {
            edits.extend(plan(content, &item.target, &output).map_err(review_api::error)?);
        }
    }
    commit(task)?;
    if edits.is_empty() {
        let mut result = source;
        result["code_output_unchanged"] = json!(true);
        return Ok(crate::server::Response::json(&result));
    }
    let request = json!({"action":"edit","document_id":value["document_id"],"generation":value["generation"],"review_revision":value["review_revision"],"call_id":call,"origin":"editor","edits":edits,"code_output_intent_digest":intent});
    Ok(crate::server::Response::json(
        &app.review.handle(&request).map_err(review_api::error)?,
    ))
}

fn run(
    app: &Arc<App>,
    value: &Value,
    batch: Batch,
    task: Option<&crate::cancel::TaskGuard>,
) -> ApiResult<crate::server::Response> {
    let started = Instant::now();
    let outer = crate::speech::cancellation_check();
    let deadline = outer.clone();
    crate::speech::with_cancel(
        Arc::new(move || deadline() || started.elapsed() >= DEADLINE),
        || {
            let mut results = Vec::new();
            let mut output_chars = 0usize;
            // Actual built-in operation, rather than a route/executable check.
            if crate::speech::cancelled() {
                return Err(review_api::error("cancelled"));
            }
            let probe = crate::code_chunk_runner::execute_code_chunk_dict(
                "SELECT 1;",
                "sql",
                false,
                1,
                None,
            )
            .map_err(|_| review_api::error("capability_pack_unavailable"))?;
            if probe.get("ok") != Some(&Value::Bool(true)) {
                return Err(review_api::error(if crate::speech::cancelled() {
                    "cancelled"
                } else {
                    "capability_pack_unavailable"
                }));
            }
            for item in &batch.items {
                check_source(&review_api::snapshot(app, value)?, value)?;
                let result = crate::code_chunk_runner::execute_code_chunk_dict(
                    &item.target.code,
                    &item.lang,
                    false,
                    10,
                    None,
                )
                .map_err(|_| review_api::error("execution_failed"))?;
                if result.get("ok") != Some(&Value::Bool(true)) {
                    let code = if started.elapsed() >= DEADLINE {
                        "execution_timeout"
                    } else if outer() {
                        "execution_cancelled"
                    } else {
                        result
                            .get("error_code")
                            .and_then(Value::as_str)
                            .or_else(|| {
                                result.get("error").and_then(Value::as_str).filter(|code| {
                                    matches!(
                                        *code,
                                        "network_not_allowed"
                                            | "path_access_not_allowed"
                                            | "cwd_not_found"
                                            | "cwd_not_allowed"
                                    )
                                })
                            })
                            .unwrap_or("execution_failed")
                    };
                    return Ok(crate::server::Response::json(
                        &json!({"ok":false,"error_code":code,"results":results,"failed_index":results.len(),"source_written":false}),
                    ));
                }
                output_chars += result
                    .get("stdout")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .count();
                if result
                    .get("stderr")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
                {
                    output_chars += 1;
                }
                output_chars += result
                    .get("stderr")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .count();
                if output_chars > MAX_OUTPUT {
                    return Err(review_api::error(BUDGET));
                }
                results.push(result);
            }
            check_source(&review_api::snapshot(app, value)?, value)?;
            if started.elapsed() >= DEADLINE {
                return Err(review_api::error("execution_timeout"));
            }
            commit(task)?;
            Ok(crate::server::Response::json(
                &json!({"ok":true,"results":results,"source_written":false}),
            ))
        },
    )
}

#[cfg(test)]
#[path = "code_chunk_batch_tests.rs"]
mod tests;
