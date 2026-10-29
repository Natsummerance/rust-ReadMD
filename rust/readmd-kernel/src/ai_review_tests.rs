//! Protocol and rejection tests only. Mock replies do not certify a real model
//! or the persistent A/B/C save-and-reopen acceptance workflow.
use super::*;
use crate::ai_providers::{LineStream, ProviderDirectory, RouteArgs, RouteTarget, SkillError, SkillService, VarMap};
use std::collections::VecDeque;
use std::sync::Mutex;

struct MockTransport {
    replies: Mutex<VecDeque<Result<String, HttpError>>>,
    requests: Mutex<Vec<(String, Headers, Value)>>,
}

impl MockTransport {
    fn new(replies: Vec<Value>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().map(|reply| Ok(reply.to_string())).collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn count(&self) -> usize { self.requests.lock().unwrap().len() }

    fn body(&self, index: usize) -> Value { self.requests.lock().unwrap()[index].2.clone() }
}

impl Transport for MockTransport {
    fn post_json(&self, url: &str, headers: &Headers, body: &str) -> Result<String, HttpError> {
        self.requests.lock().unwrap().push((url.into(), headers.clone(), serde_json::from_str(body).unwrap()));
        self.replies.lock().unwrap().pop_front().expect("unexpected extra request")
    }

    fn open_stream(&self, _: &str, _: &Headers, _: &str) -> Result<Box<dyn LineStream>, HttpError> {
        panic!("the first review bridge must use bounded non-stream native tools")
    }
}

fn route(protocol: &'static str) -> ResolvedRoute {
    let prior_chat = "PRIOR_CHAT_MUST_NOT_BE_SENT";
    ResolvedRoute {
        target: if protocol == "anthropic" { RouteTarget::Anthropic } else { RouteTarget::Chat },
        tool_protocol: Some(protocol),
        args: RouteArgs {
            base_url: "https://provider.invalid/v1".into(),
            api_key: "synthetic-key".into(),
            model: json!("synthetic-model"),
            messages: vec![json!({"role":"system","content":prior_chat})],
            temperature: json!(0.4),
            stream: true,
            endpoint_mode: "prefix".into(),
            custom_headers: json!({"X-Synthetic":"test"}),
        },
    }
}

fn snapshot() -> Value {
    // UTF-16: first original [4,12), SESSION_ONLY [14,26),
    // second identical original [32,40). Each emoji occupies two units.
    let content = "甲😀 original\n\nSESSION_ONLY\n\n甲😀 original\n";
    json!({
        "document_id":"session-one", "content":content,
        "generation":7, "review_revision":3,
        "revision":crate::document_transaction::revision(content),
        "private_side_data":"MUST_NOT_BE_SENT",
    })
}

fn read_args() -> Value { json!({"document_id":"session-one"}) }

fn edits() -> Value {
    json!([
        {"from":4,"to":12,"insert":"reviewed-A","original":"original"},
        {"from":32,"to":40,"insert":"reviewed-B","original":"original"},
    ])
}

fn proposal(edits: Value) -> Value {
    json!({"document_id":"session-one","generation":7,"review_revision":3,"edits":edits})
}

fn chat_call(id: &str, name: &str, arguments: Value) -> Value {
    json!({"choices":[{"finish_reason":"tool_calls","message":{
        "role":"assistant","content":null,
        "tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":arguments.to_string()}}],
    }}]})
}

fn chat_script(arguments: Value) -> MockTransport {
    MockTransport::new(vec![chat_call("native-read", "read_document", read_args()), chat_call("native-propose", "propose_review", arguments)])
}

fn run(http: &MockTransport) -> Result<Value, AiError> {
    propose(&route("chat_completions"), http, &snapshot(), "Review the two original prose words.", &|| false)
}

#[test]
fn chat_native_tool_round_trip_reads_actual_unsaved_session() {
    let http = chat_script(proposal(edits()));
    let result = run(&http).unwrap();
    assert_eq!(http.count(), 2);
    let first = http.body(0);
    assert_eq!(first["stream"], false);
    assert_eq!(first["tools"][0]["type"], "function");
    assert_eq!(first["tools"][0]["function"]["name"], "read_document");
    assert_eq!(first["tools"][1]["function"]["name"], "propose_review");
    assert_eq!(first["tool_choice"]["function"]["name"], "read_document");
    assert_eq!(first["parallel_tool_calls"], false);
    assert!(!first.to_string().contains("SESSION_ONLY"));
    assert!(!first.to_string().contains("PRIOR_CHAT_MUST_NOT_BE_SENT"));
    let second = http.body(1);
    assert_eq!(second["messages"][2]["tool_calls"][0]["id"], "native-read");
    assert_eq!(second["messages"][3]["role"], "tool");
    assert_eq!(second["messages"][3]["tool_call_id"], "native-read");
    let actual: Value = serde_json::from_str(second["messages"][3]["content"].as_str().unwrap()).unwrap();
    assert_eq!(actual["content"], snapshot()["content"]);
    assert_eq!(actual["generation"], 7);
    assert_eq!(actual["review_revision"], 3);
    assert!(actual.get("private_side_data").is_none());
    assert_eq!(result["edits"], json!([
        {"from":4,"to":12,"insert":"reviewed-A"},
        {"from":32,"to":40,"insert":"reviewed-B"},
    ]));
    assert_eq!(result["source_generation"], 7);
    assert_eq!(result["source_review_revision"], 3);
    assert!(result["call_id"].as_str().unwrap().starts_with("review-ai:"));
    assert!(!result.to_string().contains("synthetic"));
    let again = run(&chat_script(proposal(edits()))).unwrap();
    assert_ne!(result["call_id"], again["call_id"], "upstream IDs are not globally unique receipt identities");
}

#[test]
fn anthropic_uses_tool_use_and_tool_result_and_preserves_signed_blocks() {
    let read_content = json!([
        {"type":"thinking","thinking":"synthetic reasoning","signature":"synthetic-signature"},
        {"type":"tool_use","id":"anthropic-read","name":"read_document","input":read_args()},
    ]);
    let http = MockTransport::new(vec![
        json!({"role":"assistant","content":read_content,"stop_reason":"tool_use"}),
        json!({"role":"assistant","content":[{"type":"tool_use","id":"anthropic-propose","name":"propose_review","input":proposal(edits())}],"stop_reason":"tool_use"}),
    ]);
    propose(&route("anthropic"), &http, &snapshot(), "Review two words.", &|| false).unwrap();
    let first = http.body(0);
    assert_eq!(first["tools"][0]["name"], "read_document");
    assert_eq!(first["tools"][0]["input_schema"]["additionalProperties"], false);
    assert_eq!(first["tool_choice"]["type"], "tool");
    assert_eq!(first["tool_choice"]["disable_parallel_tool_use"], true);
    let second = http.body(1);
    assert_eq!(second["messages"][1]["content"], read_content);
    assert_eq!(second["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(second["messages"][2]["content"][0]["tool_use_id"], "anthropic-read");
    let returned: Value = serde_json::from_str(second["messages"][2]["content"][0]["content"].as_str().unwrap()).unwrap();
    assert_eq!(returned["content"], snapshot()["content"]);
    let requests = http.requests.lock().unwrap();
    assert!(requests[0].0.ends_with("/v1/messages"));
    assert!(requests[0].1.contains(&("x-api-key".into(),"synthetic-key".into())));
}

#[test]
fn gemini_uses_function_declarations_responses_and_original_thought_signature() {
    let read_content = json!({"role":"model","parts":[{
        "functionCall":{"id":"gemini-read","name":"read_document","args":read_args()},
        "thoughtSignature":"synthetic-signature",
    }]});
    let http = MockTransport::new(vec![
        json!({"candidates":[{"finishReason":"STOP","content":read_content}]}),
        json!({"candidates":[{"finishReason":"STOP","content":{"role":"model","parts":[{"functionCall":{
            "id":"gemini-propose","name":"propose_review","args":proposal(edits()),
        }}]}}]}),
    ]);
    let mut gemini = route("gemini");
    gemini.args.base_url = "https://provider.invalid/v1beta".into();
    gemini.args.custom_headers["X-Goog-Api-Key"] = json!("must-not-override-vault-key");
    propose(&gemini, &http, &snapshot(), "Review two words.", &|| false).unwrap();
    let first = http.body(0);
    assert!(first.get("messages").is_none());
    assert_eq!(first["tools"][0]["functionDeclarations"][0]["name"], "read_document");
    assert_eq!(first["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"], json!(["read_document"]));
    let second = http.body(1);
    assert_eq!(second["contents"][1], read_content);
    let response = &second["contents"][2]["parts"][0]["functionResponse"];
    assert_eq!(response["name"], "read_document");
    assert_eq!(response["id"], "gemini-read");
    assert_eq!(response["response"]["content"], snapshot()["content"]);
    let requests = http.requests.lock().unwrap();
    assert!(requests[0].0.ends_with(":generateContent"));
    assert!(requests[0].1.contains(&("x-goog-api-key".into(), "synthetic-key".into())));
    assert!(!requests[0].1.iter().any(|(_, value)| value == "must-not-override-vault-key"));
}

#[test]
fn gemini_without_provider_call_ids_uses_native_name_pairing() {
    let http = MockTransport::new(vec![
        json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"read_document","args":read_args()}}]}}]}),
        json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"propose_review","args":proposal(edits())}}]}}]}),
    ]);
    propose(&route("gemini"), &http, &snapshot(), "Review.", &|| false).unwrap();
    assert!(http.body(1)["contents"][2]["parts"][0]["functionResponse"].get("id").is_none());
}

#[test]
fn selected_template_reaches_all_three_native_protocols_only_after_authoritative_read() {
    let template = TaskTemplate {
        skill_id:"synthetic-polish".into(),
        rendered:"TASK_TEMPLATE_MARKER: Polish in French. Source: read_document.content. Selection: allowed_ranges. Legacy output instruction: output the complete document.".into(),
    };
    for protocol in ["chat_completions","anthropic","gemini"] {
        let replies = match protocol {
            "chat_completions" => vec![chat_call("read","read_document",read_args()),chat_call("propose","propose_review",proposal(edits()))],
            "anthropic" => vec![
                json!({"role":"assistant","content":[{"type":"tool_use","id":"read","name":"read_document","input":read_args()}],"stop_reason":"tool_use"}),
                json!({"role":"assistant","content":[{"type":"tool_use","id":"propose","name":"propose_review","input":proposal(edits())}],"stop_reason":"tool_use"}),
            ],
            _ => vec![
                json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"read_document","args":read_args()}}]}}]}),
                json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"propose_review","args":proposal(edits())}}]}}]}),
            ],
        };
        let http = MockTransport::new(replies);
        let mut scoped=snapshot();scoped["allowed_ranges"]=json!([[4,12],[32,40]]);
        let result=propose_with_template(&route(protocol),&http,&scoped,"Polish the selected prose.",Some(&template),&|| false).unwrap();
        assert_eq!(result["edits"].as_array().unwrap().len(),2);
        let first=http.body(0).to_string();
        assert!(!first.contains("TASK_TEMPLATE_MARKER"));
        assert!(!first.contains("SESSION_ONLY"));
        let second=http.body(1);
        let task=match protocol {
            "chat_completions" => {
                assert_eq!(second["messages"][0]["content"],SYSTEM);
                assert_eq!(second["messages"][3]["role"],"tool");
                assert_eq!(second["messages"][4]["role"],"user");
                second["messages"][4]["content"].as_str().unwrap()
            }
            "anthropic" => {
                assert_eq!(second["system"],SYSTEM);
                assert_eq!(second["messages"][2]["content"][0]["type"],"tool_result");
                assert_eq!(second["messages"][2]["content"][1]["type"],"text");
                second["messages"][2]["content"][1]["text"].as_str().unwrap()
            }
            _ => {
                assert_eq!(second["systemInstruction"]["parts"][0]["text"],SYSTEM);
                assert_eq!(second["contents"][2]["parts"][0]["functionResponse"]["name"],"read_document");
                second["contents"][2]["parts"][1]["text"].as_str().unwrap()
            }
        };
        let task:Value=serde_json::from_str(task).unwrap();
        assert_eq!(task["selected_task_template"]["skill_id"],"synthetic-polish");
        assert_eq!(task["selected_task_template"]["instructions"],template.rendered);
        let wire=second.to_string();
        assert_eq!(wire.matches("SESSION_ONLY").count(),1,"source appears only in the native read result");
        assert_eq!(wire.matches("TASK_TEMPLATE_MARKER").count(),1);
        assert!(!wire.contains("PRIOR_CHAT_MUST_NOT_BE_SENT"));
        assert!(SYSTEM.contains("take priority over its"));
    }
}

#[test]
fn invalid_or_oversized_selected_template_is_rejected_before_transport() {
    for template in [
        TaskTemplate{skill_id:String::new(),rendered:"task".into()},
        TaskTemplate{skill_id:"fixture".into(),rendered:"  ".into()},
        TaskTemplate{skill_id:"fixture".into(),rendered:"x".repeat(MAX_TEMPLATE_BYTES+1)},
    ] {
        let http=MockTransport::new(vec![]);
        let expected=if template.rendered.len()>MAX_TEMPLATE_BYTES{"review_ai_budget"}else{"review_invalid_request"};
        assert_eq!(propose_with_template(&route("chat_completions"),&http,&snapshot(),"Review.",Some(&template),&|| false).unwrap_err().message,expected);
        assert_eq!(http.count(),0);
    }
}

#[test]
fn proposals_cannot_run_before_a_separate_read_round() {
    let direct = MockTransport::new(vec![chat_call("propose-first", "propose_review", proposal(edits()))]);
    assert_eq!(run(&direct).unwrap_err().message, "review_ai_invalid_tool");
    assert_eq!(direct.count(), 1);
    let mut mixed = chat_call("read", "read_document", read_args());
    mixed["choices"][0]["message"]["tool_calls"].as_array_mut().unwrap().push(
        chat_call("propose", "propose_review", proposal(edits()))["choices"][0]["message"]["tool_calls"][0].clone());
    let http = MockTransport::new(vec![mixed]);
    assert_eq!(run(&http).unwrap_err().message, "review_ai_invalid_tool");
    assert_eq!(http.count(), 1);
}

#[test]
fn read_authority_and_tool_schema_cannot_be_expanded_by_model_arguments() {
    for args in [json!({"document_id":"other-session"}), json!({"document_id":"session-one","path":"private.md"}), json!({})] {
        let http = MockTransport::new(vec![chat_call("read", "read_document", args)]);
        assert_eq!(run(&http).unwrap_err().message, "review_ai_invalid_tool");
        assert_eq!(http.count(), 1);
    }
    for name in ["read_file", "export_snapshot", "write_document"] {
        let http = MockTransport::new(vec![chat_call("read", name, read_args())]);
        assert_eq!(run(&http).unwrap_err().message, "review_ai_invalid_tool");
    }
}

#[test]
fn proposal_requires_exact_authorized_document_and_both_revisions() {
    for (key, value) in [
        ("document_id",json!("other-session")), ("generation",json!(6)),
        ("review_revision",json!(2)), ("generation",json!(7.0)), ("confirm",json!(true)),
    ] {
        let mut args = proposal(edits());
        args[key] = value;
        let http = chat_script(args);
        assert_eq!(run(&http).unwrap_err().message, "review_ai_invalid_tool", "field {key}");
    }
}

#[test]
fn ranges_originals_and_edit_shapes_are_validated_atomically() {
    let invalid = [
        json!({"from":2,"to":3,"insert":"X"}), // inside first emoji
        json!({"from":4,"to":999,"insert":"X"}),
        json!({"from":-1,"to":4,"insert":"X"}),
        json!({"from":4,"to":3,"insert":"X"}),
        json!({"from":4.0,"to":12,"insert":"X"}),
        json!({"from":4,"to":12,"insert":"X","original":"wrong"}),
        json!({"from":4,"to":12,"insert":"X","original":null}),
        json!({"from":4,"to":12,"insert":4}),
        json!({"from":4,"to":12,"insert":"X\r\n"}),
        json!({"from":4,"to":12,"insert":"X","extra":"forbidden"}),
    ];
    for invalid_edit in invalid {
        let http = chat_script(proposal(json!([
            {"from":32,"to":40,"insert":"valid-B"}, invalid_edit,
        ])));
        assert_eq!(run(&http).unwrap_err().message, "review_ai_invalid_tool");
    }
    for overlapping in [
        json!([{"from":4,"to":12,"insert":"X"},{"from":11,"to":13,"insert":"Y"}]),
        json!([{"from":4,"to":12,"insert":"X"},{"from":4,"to":4,"insert":"Y"}]),
    ] {
        assert_eq!(run(&chat_script(proposal(overlapping))).unwrap_err().message, "review_ai_invalid_tool");
    }
}

#[test]
fn valid_surrogate_pair_and_repeated_paragraphs_keep_exact_source_offsets() {
    let http = chat_script(proposal(json!([
        {"from":32,"to":40,"insert":"second","original":"original"},
        {"from":1,"to":3,"insert":"😀😀","original":"😀"},
    ])));
    let result = run(&http).unwrap();
    assert_eq!(result["edits"], json!([
        {"from":1,"to":3,"insert":"😀😀"},
        {"from":32,"to":40,"insert":"second"},
    ]));
}

#[test]
fn allowed_mutation_ranges_are_returned_and_enforced_separately_from_read_context() {
    let mut source = snapshot();
    source["allowed_ranges"] = json!([[4,12]]);
    let http = chat_script(proposal(json!([{"from":4,"to":12,"insert":"permitted"}])));
    propose(&route("chat_completions"), &http, &source, "Review selection.", &|| false).unwrap();
    let returned: Value = serde_json::from_str(http.body(1)["messages"][3]["content"].as_str().unwrap()).unwrap();
    assert_eq!(returned["allowed_ranges"], json!([[4,12]]));
    assert_eq!(returned["content"], source["content"]);
    let invalid = chat_script(proposal(edits()));
    assert_eq!(propose(&route("chat_completions"), &invalid, &source, "Review selection.", &|| false).unwrap_err().message, "review_ai_invalid_tool");
    source["allowed_ranges"] = json!([]);
    assert_eq!(propose(&route("chat_completions"), &chat_script(proposal(edits())), &source, "Review selection.", &|| false).unwrap_err().message, "review_ai_invalid_tool");
    source["allowed_ranges"] = json!([[2,12]]);
    let never_called = MockTransport::new(vec![]);
    assert_eq!(propose(&route("chat_completions"), &never_called, &source, "Review selection.", &|| false).unwrap_err().message, "review_ai_invalid_tool");
    assert_eq!(never_called.count(), 0);
}

#[test]
fn full_source_edit_requires_explicit_selection_and_empty_document_can_be_written() {
    let mut source = snapshot();
    source["content"] = json!("short");
    source["revision"] = json!(crate::document_transaction::revision("short"));
    let args = proposal(json!([{"from":0,"to":5,"insert":"polished"}]));
    assert_eq!(propose(&route("chat_completions"), &chat_script(args.clone()), &source, "Polish.", &|| false).unwrap_err().message, "review_ai_invalid_tool");
    source["allowed_ranges"] = json!([[0,5]]);
    assert!(propose(&route("chat_completions"), &chat_script(args), &source, "Polish selection.", &|| false).is_ok());
    source["content"] = json!("");
    source["revision"] = json!(crate::document_transaction::revision(""));
    source["allowed_ranges"] = json!([[0,0]]);
    let insertion = proposal(json!([{"from":0,"to":0,"insert":"new text"}]));
    assert!(propose(&route("chat_completions"), &chat_script(insertion), &source, "Write.", &|| false).is_ok());
}

#[test]
fn no_changes_and_textual_json_never_become_a_proposal() {
    for edits in [json!([]), json!([{"from":4,"to":12,"insert":"original"}])] {
        assert_eq!(run(&chat_script(proposal(edits))).unwrap_err().message, "review_ai_no_changes");
    }
    let text = json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":proposal(edits()).to_string()}}]});
    assert_eq!(run(&MockTransport::new(vec![text])).unwrap_err().message, "review_ai_no_changes");
    let mut bad_json = chat_call("read", "read_document", read_args());
    bad_json["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"] = json!("{bad-json");
    assert_eq!(run(&MockTransport::new(vec![bad_json])).unwrap_err().message, "review_ai_invalid_tool");
}

#[test]
fn repeated_reads_have_four_actual_rounds_and_duplicate_native_ids_are_rejected() {
    let replies = (0..MAX_ROUNDS).map(|i| chat_call(&format!("read-{i}"), "read_document", read_args())).collect();
    let http = MockTransport::new(replies);
    assert_eq!(run(&http).unwrap_err().message, "review_ai_budget");
    assert_eq!(http.count(), 4);
    let duplicate = MockTransport::new(vec![chat_call("same", "read_document", read_args()),chat_call("same", "propose_review", proposal(edits()))]);
    assert_eq!(run(&duplicate).unwrap_err().message, "review_ai_invalid_tool");
}

#[test]
fn cancellation_before_or_after_any_request_prevents_proposal_return() {
    let never_called = MockTransport::new(vec![]);
    assert_eq!(propose(&route("chat_completions"), &never_called, &snapshot(), "Review.", &|| true).unwrap_err().message, "cancelled");
    assert_eq!(never_called.count(), 0);
    for stop_after in [1,2] {
        let http = chat_script(proposal(edits()));
        let result = propose(&route("chat_completions"), &http, &snapshot(), "Review.", &|| http.count() >= stop_after);
        assert_eq!(result.unwrap_err().message, "cancelled");
        assert_eq!(http.count(), stop_after);
    }
}

#[test]
fn content_instruction_response_and_operation_budgets_are_bounded() {
    let never_called = MockTransport::new(vec![]);
    let mut too_large = snapshot();
    too_large["content"] = json!("x".repeat(MAX_DOCUMENT_BYTES + 1));
    assert_eq!(propose(&route("chat_completions"), &never_called, &too_large, "Review.", &|| false).unwrap_err().message, "review_ai_budget");
    assert_eq!(propose(&route("chat_completions"), &never_called, &snapshot(), &"x".repeat(MAX_INSTRUCTION_BYTES + 1), &|| false).unwrap_err().message, "review_ai_budget");
    assert_eq!(never_called.count(), 0);
    let large_reply = MockTransport::new(vec![]);
    large_reply.replies.lock().unwrap().push_back(Ok(" ".repeat(MAX_RESPONSE_BYTES + 1)));
    assert_eq!(run(&large_reply).unwrap_err().message, "review_ai_budget");
    let excessive_edits = json!(vec![json!({"from":4,"to":12,"insert":"X"}); MAX_EDITS + 1]);
    assert_eq!(run(&chat_script(proposal(excessive_edits))).unwrap_err().message, "review_ai_budget");
    let mut aggregate = MAX_WIRE_BYTES - 1;
    assert_eq!(add_budget(&mut aggregate, 2, MAX_REQUEST_BYTES).unwrap_err().message, "review_ai_budget");
}

#[test]
fn provider_failures_and_malformed_replies_never_echo_connection_data() {
    for (failure, code) in [
        (HttpError::Status { code:429, body:"https://private.invalid synthetic-key synthetic-model".into() }, "ai_http_error"),
        (HttpError::Network("https://private.invalid synthetic-key synthetic-model".into()), "ai_network_error"),
    ] {
        let http = MockTransport::new(vec![]);
        http.replies.lock().unwrap().push_back(Err(failure));
        assert_eq!(run(&http).unwrap_err().message, code);
    }
    for malformed in [json!({"error":{"message":"synthetic-key synthetic-model"}}),json!({"choices":[]}),json!("private response")] {
        assert_eq!(run(&MockTransport::new(vec![malformed])).unwrap_err().message, "ai_invalid_response");
    }
    let http = MockTransport::new(vec![]);
    http.replies.lock().unwrap().push_back(Ok("synthetic-key invalid JSON".into()));
    assert_eq!(run(&http).unwrap_err().message, "ai_invalid_response");
}

#[test]
fn only_exact_known_local_transport_failures_keep_their_typed_codes() {
    for (reason,expected) in [
        ("review_ai_budget","review_ai_budget"),
        ("ai_invalid_response","ai_invalid_response"),
        ("review_ai_budget: synthetic-key https://private.invalid","ai_network_error"),
    ] {
        let http = MockTransport::new(vec![]);
        http.replies.lock().unwrap().push_back(Err(HttpError::Network(reason.into())));
        assert_eq!(run(&http).unwrap_err().message,expected);
    }
}

#[test]
fn truncated_native_proposals_are_rejected_for_each_protocol() {
    let mut chat = chat_call("read", "read_document", read_args());
    chat["choices"][0]["finish_reason"] = json!("length");
    assert_eq!(run(&MockTransport::new(vec![chat])).unwrap_err().message, "review_ai_budget");
    let anthropic = MockTransport::new(vec![json!({"stop_reason":"max_tokens","content":[]})]);
    assert_eq!(propose(&route("anthropic"), &anthropic, &snapshot(), "Review.", &|| false).unwrap_err().message, "review_ai_budget");
    let gemini = MockTransport::new(vec![json!({"candidates":[{"finishReason":"MAX_TOKENS"}]})]);
    assert_eq!(propose(&route("gemini"), &gemini, &snapshot(), "Review.", &|| false).unwrap_err().message, "review_ai_budget");
}

struct Directory(Value);

impl ProviderDirectory for Directory {
    fn current_config(&self) -> Value { json!({"provider":"test-provider"}) }
    fn find_provider(&self, _: &str) -> Value { self.0.clone() }
    fn find_provider_by_credential(&self, _: &str) -> Option<Value> { None }
    fn load_credential(&self, _: &str) -> String { "synthetic-key".into() }
    fn decrypt_secret(&self, _: &str) -> String { "synthetic-key".into() }
    fn env_var(&self, _: &str) -> String { String::new() }
}

struct NoSkills;

impl SkillService for NoSkills {
    fn render_skill(&self, _: &str, _: &VarMap) -> Result<String, SkillError> { panic!("no skill in review route") }
}

#[test]
fn configured_api_protocol_is_strict_and_independent_of_model_names() {
    for (format, mode, expected) in [
        ("openai","auto",Some("chat_completions")),
        ("openai","chat",Some("chat_completions")),
        ("openai","messages",Some("anthropic")),
        ("anthropic","auto",Some("anthropic")),
        ("gemini","auto",Some("gemini")),
        ("openai","responses",None),
        ("openai","completion",None),
        ("openai","unknown",None),
        ("unknown","auto",None),
    ] {
        let directory = Directory(json!({"id":"test-provider","base_url":"https://provider.invalid/v1","api_key":"enc:synthetic","format":format,"mode":mode,"models":["gemini-synthetic"]}));
        let resolved = crate::ai_providers::resolve_chat(&json!({"messages":[]}), &directory, &NoSkills).unwrap();
        assert_eq!(resolved.tool_protocol, expected, "format={format}, mode={mode}");
        if expected.is_none() {
            let http = MockTransport::new(vec![]);
            assert_eq!(propose(&resolved, &http, &snapshot(), "Review.", &|| false).unwrap_err().message, "review_ai_unsupported");
            assert_eq!(http.count(), 0);
        }
    }
}

#[test]
fn gemini_endpoint_modes_keep_configured_prefixes_and_full_endpoints() {
    assert_eq!(gemini_url("https://provider.invalid/proxy/v1beta", "prefix", "models/example").unwrap(), "https://provider.invalid/proxy/v1beta/models/example:generateContent");
    assert_eq!(gemini_url("https://provider.invalid/proxy", "prefix", "example").unwrap(), "https://provider.invalid/proxy/v1beta/models/example:generateContent");
    assert_eq!(gemini_url("https://provider.invalid/custom", "full_url", "example").unwrap(), "https://provider.invalid/custom");
    assert_eq!(gemini_url("https://provider.invalid/v1beta/models/example:streamGenerateContent", "full_url", "example").unwrap_err().message, "review_ai_unsupported");
}
