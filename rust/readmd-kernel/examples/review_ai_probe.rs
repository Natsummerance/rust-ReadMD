//! Opt-in private-provider evidence. Never copies a configuration/profile or
//! prints a provider identity, model, address, key, document text or raw error.
//! READMD_REVIEW_AI_CONFIG selects an existing schema-3 ai.json read in place;
//! READMD_REVIEW_EVIDENCE selects an owned report path. No GUI is launched.
use readmd_kernel::{paths, server, App};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

fn invoke(app: &Arc<App>, route: &str, body: &Value) -> Result<Value, String> {
    let (status, result) = server::call_in_process(app, "POST", route, Some(body));
    if status != 200 {
        return Err(result["error_code"]
            .as_str()
            .unwrap_or("unexpected_status")
            .to_string());
    }
    Ok(result)
}
fn change(app: &Arc<App>, state: &Value, action: &str, extra: Value) -> Result<Value, String> {
    let mut body = json!({"action":action,"document_id":state["document_id"],"generation":state["generation"],
        "review_revision":state["review_revision"],"call_id":uuid::Uuid::new_v4().to_string()});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    invoke(app, "/api/review", &body)
}
fn require(condition: bool) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err("independent_expectation_failed".into())
    }
}
fn connection_rejection(cfg: &Value, selectors: &Value) -> Result<Value, String> {
    use readmd_kernel::ai_providers::{self, Headers, HttpError, LineStream, Transport};
    use std::cell::RefCell;
    struct Inspect {
        transport: ai_providers::ureq_transport::UreqTransport,
        safe_cause: RefCell<Value>,
    }
    impl Transport for Inspect {
        fn post_json(&self, url: &str, headers: &Headers, body: &str) -> Result<String, HttpError> {
            let result = self.transport.post_json(url, headers, body);
            if let Err(HttpError::Status { code, body }) = &result {
                let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let message = parsed.pointer("/error/message").and_then(Value::as_str)
                    .or_else(|| parsed["error"].as_str()).or_else(|| parsed["message"].as_str()).unwrap_or(body);
                let lower = message.to_ascii_lowercase();
                let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
                let categories = [
                    ("client_upgrade_required", has(&["upgrade", "update", "no longer supported", "client version"])),
                    ("model_rejected", has(&["model_not_found", "model not found", "unknown model", "invalid model", "model is not", "model unavailable", "模型不存在", "模型不可用"])),
                    ("credential_rejected", has(&["invalid api key", "unauthorized", "unauthenticated", "invalid token", "permission denied"])),
                    ("quota_or_rate_limit", has(&["quota", "rate limit", "too many requests", "credit", "余额", "额度"])),
                    ("parameter_rejected", has(&["invalid argument", "invalid_argument", "unsupported parameter", "invalid_request", "bad request"])),
                    ("provider_unavailable", has(&["unavailable", "overloaded", "no available", "not configured", "未配置", "无可用"])),
                    ("region_restricted", has(&["user location is not supported", "not available in your country", "unsupported region", "regional restriction", "地区不支持", "区域限制"])),
                    ("upstream_account_unavailable", has(&["account", "no_available", "capacity", "账号", "账户", "渠道"])),
                    ("request_shape_rejected", has(&["invalid", "malformed", "json payload", "required", "unsupported", "不支持", "参数", "缺少"])),
                ].into_iter().filter_map(|(name, yes)| yes.then_some(name)).collect::<Vec<_>>();
                let parameter = parsed.pointer("/error/param").and_then(Value::as_str)
                    .filter(|name| ["model", "messages", "temperature", "tools", "tool_choice", "stream", "max_tokens"].contains(name));
                // Only fixed labels and numeric status leave this private buffer.
                *self.safe_cause.borrow_mut() = json!({"upstreamHttpStatus":code,"categories":categories,"parameter":parameter,"responseBodyBytes":body.len(),"structuredResponse":parsed.is_object(),"rawBodyLogged":false});
            }
            result
        }
        fn open_stream(&self, _: &str, _: &Headers, _: &str) -> Result<Box<dyn LineStream>, HttpError> {
            Err(HttpError::Network("diagnostic_nonstream_only".into()))
        }
    }
    let name = selectors["provider"].as_str().or_else(|| cfg["current"]["provider_id"].as_str()).ok_or("private_configuration_unavailable")?;
    let provider = cfg["providers"].as_array().and_then(|items| items.iter().find(|p| p["id"] == name)).ok_or("private_configuration_unavailable")?;
    require(provider["format"] == "openai")?;
    let args = ai_providers::RouteArgs {
        base_url: provider["base_url"].as_str().ok_or("private_configuration_unavailable")?.into(),
        api_key: readmd_kernel::ai::AiSettings::at_default_location().credentials().resolve_key(provider),
        model: selectors["model"].clone(),
        messages: vec![json!({"role":"user","content":"Reply only OK."})],
        temperature: json!(0.4), stream: false,
        endpoint_mode: provider["endpoint_mode"].as_str().unwrap_or("prefix").into(),
        custom_headers: provider["headers"].clone(),
    };
    let transport = Inspect { transport: ai_providers::ureq_transport::UreqTransport::new(), safe_cause: RefCell::new(Value::Null) };
    let accepted = ai_providers::chat_openai(&args, &transport).is_ok();
    let mut result = transport.safe_cause.into_inner();
    if !result.is_object() { result = json!({}); }
    result["adapterRequestAccepted"] = json!(accepted);
    Ok(result)
}
fn multi_round(app:&Arc<App>,request:&Value)->Result<(Value,Value),String>{
    let mut body=request.clone();body["action"]=json!("start");body["budget"]=json!({"max_rounds":16,"max_elapsed_ms":600000});
    body["request"]=json!("Use native tools to make two separate verified steps on ordinary prose. First read the actual unsaved document and propose exactly one minimal edit changing only colour to color in A. Read the resulting document again. In a later proposal change only colour to color in B, as a second separate edit. Keep C and its actual unsaved marker, heading, labels, punctuation and blank lines unchanged. Include exact original text for each edit. Finally read the current document, call inspect_review and call task_complete only after confirming both edits. Do not combine the two edits into one proposal or replace the whole document.");
    let first=invoke(app,"/api/ai/tasks",&body)?;let id=first["task"]["id"].as_str().ok_or("task_start_failed")?;
    let started=Instant::now();let mut events=Vec::new();let mut after=0u64;
    let final_status=loop{
        let value=invoke(app,"/api/ai/tasks",&json!({"action":"status","task_id":id,"after_event_id":after}))?;
        if let Some(batch)=value["events"].as_array(){for event in batch{after=after.max(event["id"].as_u64().unwrap_or(0));events.push(event.clone());}}
        match value["task"]["state"].as_str(){Some("completed")=>break value,Some("running"|"queued")=>{},_=>return Err(value["task"]["error_code"].as_str().unwrap_or("task_incomplete").to_string())}
        if started.elapsed().as_secs()>620{let _=invoke(app,"/api/ai/tasks",&json!({"action":"pause","task_id":id}));return Err("task_wait_timeout".into());}
        std::thread::sleep(std::time::Duration::from_millis(250));
    };
    let proposals=events.iter().filter(|e|e["type"]=="tool_result"&&e["tool"]=="propose_review").count();require(proposals>=2)?;
    require(events.iter().any(|e|e["tool"]=="inspect_review"&&e["type"]=="tool_result"))?;
    require(events.iter().any(|e|e["tool"]=="task_complete"&&e["type"]=="completed"))?;
    let repeated=invoke(app,"/api/ai/tasks",&body)?;require(repeated["task"]["id"]==id&&repeated["task"]["rounds"]==final_status["task"]["rounds"])?;
    Ok((final_status["snapshot"].clone(),json!({"taskId":id,"nativeToolProposals":proposals,"rounds":final_status["task"]["rounds"],"streamRequested":true,"nativeInspection":true,"nativeCompletion":true,"durableStartReplay":true})))
}
fn content_quality(app:&Arc<App>,selectors:&Value)->Result<Value,String>{
    use base64::Engine;
    let mut request=selectors.clone();request["stream"]=json!(false);
    request["prefix"]=json!("# Research notes\n\nThe observations must be compared with ");request["suffix"]=json!(" before drawing a conclusion.\n");
    let completion=invoke(app,"/api/ai/complete",&request)?;require(completion["completion"].as_str().is_some_and(|text|!text.trim().is_empty()))?;
    // Deliberately simple, owned pixel evidence; no private document/image.
    let mut pixels=image::RgbImage::from_pixel(128,128,image::Rgb([255,255,255]));
    for y in 20..108{for x in 20..108{pixels.put_pixel(x,y,image::Rgb([230,30,40]));}}
    let mut bytes=std::io::Cursor::new(Vec::new());image::DynamicImage::ImageRgb8(pixels).write_to(&mut bytes,image::ImageFormat::Png).map_err(|_|"fixture_unavailable")?;
    let mut vision=selectors.clone();vision["stream"]=json!(false);vision["session"]=json!("owned-batch2-image-quality");
    vision["messages"]=json!([{"role":"user","content":[{"type":"text","text":"Identify the dominant color of the central square in this image. Reply only the English color name."},{"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()))}}]}]);
    let answer=invoke(app,"/api/ai/chat",&vision)?;require(answer["content"].as_str().is_some_and(|text|text.to_ascii_lowercase().contains("red")))?;
    Ok(json!({"actualCompletion":true,"completionCharacters":completion["completion"].as_str().unwrap().chars().count(),"actualMultimodalImage":true,"imageExpectationMatched":true,"syntheticMaterialsOnly":true}))
}
fn run(config: &Path, report: &Path) -> Result<Value, String> {
    use std::io::Write;
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache")
        .canonicalize()
        .map_err(|_| "report_unavailable")?;
    let report_parent = report
        .parent()
        .ok_or("report_unavailable")?
        .canonicalize()
        .map_err(|_| "report_unavailable")?;
    require(report_parent.starts_with(&cache) && !report.exists())?;
    require(config.file_name().is_some_and(|name| name == "ai.json"))?;
    let before = std::fs::read(config).map_err(|_| "private_configuration_unavailable")?;
    let cfg: Value =
        serde_json::from_slice(&before).map_err(|_| "private_configuration_unavailable")?;
    require(cfg["schema_version"] == readmd_kernel::ai::CONFIG_SCHEMA_VERSION)?;
    require(
        cfg["current"]["provider"].as_str().unwrap_or("").is_empty()
            || !cfg["current"]["provider_id"]
                .as_str()
                .unwrap_or("")
                .is_empty(),
    )?;
    let private = config.parent().ok_or("private_configuration_unavailable")?;
    require(
        !private.join("credentials.vault").is_file() || private.join("encryption.key").is_file(),
    )?;
    require(cfg["providers"].as_array().is_some_and(|providers| {
        providers.iter().all(|provider| {
            provider["api_key"].as_str().unwrap_or("").is_empty()
                && provider["id"].as_str().is_some_and(|id| !id.is_empty())
        })
    }))?;
    let profile = tempfile::tempdir().map_err(|_| "fixture_unavailable")?;
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut runtime = App::bootstrap(paths::AppPaths::with_dirs(
        &profile.path().join("data"),
        profile.path(),
        &assets,
    ))
    .map_err(|_| "bootstrap_failed")?;
    // Only credential resolution consults the existing private data directory.
    // Document state, SQLite, workspace and settings were initialized privately.
    let private = config.parent().ok_or("private_configuration_unavailable")?;
    runtime.paths.data_dir = private.into();
    std::env::set_var("READMD_DATA_DIR", private);
    let mut app = Arc::new(runtime);
    let mut selectors = json!({});
    if let Ok(provider) = std::env::var("READMD_REVIEW_AI_PROVIDER") {
        selectors["provider"] = json!(provider);
    }
    let discovery = invoke(&app, "/api/ai/models", &selectors)?;
    let models = discovery["models"]
        .as_array()
        .ok_or("model_discovery_failed")?;
    if let Ok(model) = std::env::var("READMD_REVIEW_AI_MODEL") {
        require(models.iter().any(|candidate| candidate == &model))?;
        selectors["model"] = json!(model);
    } else if let Ok(words) = std::env::var("READMD_REVIEW_AI_KEYWORDS") {
        let words = words
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        let mut matching = models
            .iter()
            .filter_map(Value::as_str)
            .filter(|model| {
                words
                    .iter()
                    .all(|word| model.to_ascii_lowercase().contains(word))
            })
            .collect::<Vec<_>>();
        matching.sort_unstable();
        selectors["model"] = json!(matching.last().ok_or("requested_model_unavailable")?);
    }
    if std::env::var("READMD_PROBE_CONNECTION_ONLY").is_ok_and(|value| value == "1") {
        let mut request = selectors.clone();
        request["stream"] = json!(false);
        request["messages"] = json!([{"role":"user","content":"Reply only OK."}]);
        let started = Instant::now();
        let (status, response) =
            server::call_in_process(&app, "POST", "/api/ai/chat", Some(&request));
        // Do not record upstream bodies, provider identities or model names.
        // A numeric HTTP code is sufficient to distinguish a service refusal
        // from a local tool orchestration error.
        let upstream = response["status"]
            .as_str()
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|code| (100..=599).contains(code));
        let cause = if status == 200 { Value::Null } else { connection_rejection(&cfg, &selectors)? };
        let result = json!({
            "passed":status == 200,
            "scope":"real configured provider minimal connection diagnostic",
            "modelDiscoverySucceeded":true,
            "apiStatus":status,
            "upstreamHttpStatus":upstream,
            "diagnostic":cause,
            "elapsedMs":started.elapsed().as_millis(),
            "configurationUnchanged":std::fs::read(config).map_err(|_|"private_configuration_unavailable")? == before,
            "providerResponseLogged":false,
            "configurationCopied":false
        });
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(report)
            .map_err(|_| "report_unavailable")?
            .write_all(&serde_json::to_vec_pretty(&result).unwrap())
            .map_err(|_| "report_unavailable")?;
        return Ok(result);
    }
    let batch2=std::env::var("READMD_PROBE_BATCH2").is_ok_and(|value|value=="1");
    let mut baseline = "# Review\n\nA: colour.\n\nB: colour.\n\nC: original.\n".to_string();
    if batch2{
        for chapter in 1..=64{
            baseline.push_str(&format!("\n## Evidence section {chapter}\n\n"));
            for _ in 0..4{baseline.push_str("The measurement is 13.7 mg with a confidence interval of 12.4 to 14.9 mg. Observations and interpretations remain separate. Repeat the experiment before accepting a claim; preserve every numeric value and its units. Reference [protocol](https://example.invalid/protocol) and citation [@evidence2026].\n\n");}
            baseline.push_str("The equation is $E=mc^2$.\n\n```rust\nfn sum(values: &[u32]) -> u32 { values.iter().sum() }\n```\n");
        }
    }
    let initial = invoke(
        &app,
        "/api/review",
        &json!({"action":"open","path":"draft:real_provider_probe","content":baseline}),
    )?;
    let c = baseline.find("original").unwrap();
    let working = change(
        &app,
        &initial,
        "edit",
        json!({"edits":[{"from":c,"to":c+8,"insert":"UNSAVED_MARKER"}],"origin":"editor"}),
    )?;
    let mut request = json!({"document_id":working["document_id"],"generation":working["generation"],"review_revision":working["review_revision"],
        "call_id":uuid::Uuid::new_v4().to_string(),"request":"Use native read_document then propose_review. Read the actual unsaved document first. Replace only the word colour with color in both A and B, as exactly two separate minimal edits. Keep the heading, labels, punctuation, blank lines and C unchanged, including its actual unsaved marker. Include original in each edit. Do not rewrite the whole document."});
    request
        .as_object_mut()
        .unwrap()
        .extend(selectors.as_object().unwrap().clone());
    let template_selected = if let Ok(skill) = std::env::var("READMD_REVIEW_AI_SKILL") {
        request["skill_id"] = json!(skill);
        true
    } else {
        false
    };
    let started = Instant::now();
    let (proposed,task_evidence)=if batch2{multi_round(&app,&request)?}else{(invoke(&app,"/api/review/ai",&request)?,Value::Null)};
    let elapsed = started.elapsed().as_millis();
    let expected=baseline.replacen("colour.","color.",2).replacen("C: original.","C: UNSAVED_MARKER.",1);
    require(proposed["content"] == expected)?;
    let changes = proposed["changes"].as_array().ok_or("invalid_receipt")?;
    require(changes.len() == 2)?;
    require(changes.iter().all(|change| {
        change["original"] == "colour"
            && change["replacement"] == "color"
            && change["decision"] == "pending"
    }))?;
    if !batch2{let repeated=invoke(&app,"/api/review/ai",&request)?;require(repeated==proposed)?;}
    let ids = changes
        .iter()
        .map(|change| change["id"].clone())
        .collect::<Vec<_>>();
    let marker = proposed["content"]
        .as_str()
        .unwrap()
        .find("UNSAVED_MARKER")
        .unwrap();
    let hand_edited = change(
        &app,
        &proposed,
        "edit",
        json!({"edits":[{"from":marker,"to":marker+14,"insert":"USER_FINAL"}],"origin":"editor"}),
    )?;
    let accepted = change(
        &app,
        &hand_edited,
        "resolve",
        json!({"ids":[ids[0]],"decision":"accept"}),
    )?;
    let quality=if batch2{content_quality(&app,&selectors)?}else{Value::Null};
    // From here no request uses private provider configuration. Save/history
    // go back to the owned temporary profile; private files stay untouched.
    let released=Instant::now();while Arc::strong_count(&app)>1&&released.elapsed().as_secs()<3{std::thread::sleep(std::time::Duration::from_millis(20));}
    Arc::get_mut(&mut app).ok_or("fixture_busy")?.paths.data_dir = profile.path().join("data");
    let target = profile.path().join("result.md");
    std::fs::write(&target, "").map_err(|_| "fixture_unavailable")?;
    let (status, _) = server::call_in_process(
        &app,
        "GET",
        &format!(
            "/api/file?p={}",
            percent_encoding::utf8_percent_encode(
                &target.to_string_lossy(),
                percent_encoding::NON_ALPHANUMERIC
            )
        ),
        None,
    );
    require(status == 200)?;
    let body = json!({"path":target.to_string_lossy(),"content":accepted["content"],"encoding":"utf-8",
        "review_session":{"document_id":accepted["document_id"],"generation":accepted["generation"],"review_revision":accepted["review_revision"],"call_id":uuid::Uuid::new_v4().to_string()}});
    let saved = invoke(&app, "/api/save", &body)?;
    require(saved["review"]["changes"][1]["decision"] == "pending")?;
    drop(app);
    let restarted = Arc::new(
        App::bootstrap(paths::AppPaths::with_dirs(
            &profile.path().join("data"),
            profile.path(),
            &assets,
        ))
        .map_err(|_| "bootstrap_failed")?,
    );
    let (status, _) = server::call_in_process(
        &restarted,
        "GET",
        &format!(
            "/api/file?p={}",
            percent_encoding::utf8_percent_encode(
                &target.to_string_lossy(),
                percent_encoding::NON_ALPHANUMERIC
            )
        ),
        None,
    );
    require(status == 200)?;
    let mut reopened = invoke(
        &restarted,
        "/api/review",
        &json!({"action":"open","path":target.to_string_lossy()}),
    )?;
    let mut exported_formats=Vec::new();
    if batch2{
        let task_id=task_evidence["taskId"].as_str().ok_or("invalid_receipt")?;
        let restored=invoke(&restarted,"/api/ai/tasks",&json!({"action":"status","task_id":task_id}))?;
        require(restored["task"]["state"]=="completed"&&restored["task"]["rounds"]==task_evidence["rounds"])?;
        for format in ["html","pdf","docx","epub","tex"]{
            let frozen=invoke(&restarted,"/api/export/snapshot",&json!({"action":"prepare","document_id":reopened["document_id"],"generation":reopened["generation"],"review_revision":reopened["review_revision"],"call_id":uuid::Uuid::new_v4().to_string(),"format":format,"suggestedName":"Review.md","options":{}}))?;
            let output=profile.path().join(format!("result.{format}"));
            let done=invoke(&restarted,"/api/export/snapshot",&json!({"action":"commit","snapshot_id":frozen["snapshot_id"],"accept_pending":true,"out_path":output,"task_id":uuid::Uuid::new_v4().to_string(),"call_id":uuid::Uuid::new_v4().to_string()}))?;
            require(done["ok"]==true&&done["finalization_pending"]==false)?;
            let bytes=std::fs::read(&output).map_err(|_|"artifact_missing")?;require(!bytes.is_empty())?;
            match format{"pdf"=>{require(lopdf::Document::load_mem(&bytes).is_ok())?;},"docx"|"epub"=>require(bytes.starts_with(b"PK"))?,"html"=>require(String::from_utf8_lossy(&bytes).contains("USER_FINAL"))?,_=>{}}
            reopened=done["review"].clone();exported_formats.push(format);
        }
    }
    let withdrawn = change(
        &restarted,
        &reopened,
        "resolve",
        json!({"ids":[ids[1]],"decision":"withdraw"}),
    )?;
    require(withdrawn["content"] == baseline.replacen("A: colour.","A: color.",1).replacen("C: original.","C: USER_FINAL.",1))?;
    require(std::fs::read(config).map_err(|_| "private_configuration_unavailable")? == before)?;
    let mut result = json!({"passed":true,"scope":"real configured provider native tools plus actual save/reopen/withdraw",
        "nativeTools":["read_document","propose_review"],"appliedBlocks":2,"elapsedMs":elapsed,
        "unsavedSourceRead":true,"idempotentReplay":true,"savedThenReopened":true,"singleBlockWithdrawn":true,
        "acceptedAndManualTextPreserved":true,"selectedTemplateRequested":template_selected,"configurationUnchanged":true,"profileCopied":false,"secretsLogged":false});
    if batch2{let mut details=task_evidence;details.as_object_mut().unwrap().remove("taskId");result["scope"]=json!("real configured provider continuing native streamed tools, durable restart, five snapshot export formats, post-export withdrawal");result["multiRound"]=details;result["exportedFormats"]=json!(exported_formats);result["exportAcceptanceAfterArtifact"]=json!(true);result["quality"]=quality;result["longDocumentBytes"]=json!(baseline.len());result["numericCodeMathCitationLinkBytesPreserved"]=json!(true);}
    if let Some(parent) = report.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "report_unavailable")?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)
        .map_err(|_| "report_unavailable")?
        .write_all(&serde_json::to_vec_pretty(&result).unwrap())
        .map_err(|_| "report_unavailable")?;
    Ok(result)
}
fn main() {
    let config = std::env::var_os("READMD_REVIEW_AI_CONFIG").map(PathBuf::from);
    let report = std::env::var_os("READMD_REVIEW_EVIDENCE").map(PathBuf::from);
    let result = match (config, report) {
        (Some(config), Some(report)) => run(&config, &report),
        _ => Err("explicit_private_configuration_and_evidence_required".into()),
    };
    match result {
        Ok(report) => {
            println!("{}", report);
            if report["passed"] != true {
                std::process::exit(1);
            }
        }
        Err(code) => {
            eprintln!("review_ai_probe_failed: {}", code);
            std::process::exit(1);
        }
    }
}
