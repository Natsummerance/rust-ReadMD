//! Bounded deterministic protocol/storage tests. These are not real-provider evidence.
use super::*;
use crate::ai_providers::{RouteArgs, RouteTarget};
use std::{collections::VecDeque, sync::Mutex};

struct MockTransport { replies:Mutex<VecDeque<Value>>, streams:Mutex<VecDeque<Vec<String>>>, requests:Mutex<Vec<Value>> }
impl MockTransport {
    fn replies(replies:Vec<Value>)->Self {Self{replies:Mutex::new(replies.into()),streams:Mutex::new(VecDeque::new()),requests:Mutex::new(Vec::new())}}
    fn streams(streams:Vec<Vec<String>>)->Self {Self{replies:Mutex::new(VecDeque::new()),streams:Mutex::new(streams.into()),requests:Mutex::new(Vec::new())}}
    fn body(&self,index:usize)->Value {self.requests.lock().unwrap()[index].clone()}
}
struct Lines {lines:VecDeque<String>}
impl LineStream for Lines {fn next_line(&mut self)->Result<Option<String>,String>{Ok(self.lines.pop_front())}}
impl Transport for MockTransport {
    fn post_json(&self,_:&str,_:&Headers,body:&str)->Result<String,HttpError>{self.requests.lock().unwrap().push(serde_json::from_str(body).unwrap());Ok(self.replies.lock().unwrap().pop_front().expect("extra native request").to_string())}
    fn open_stream(&self,_:&str,_:&Headers,body:&str)->Result<Box<dyn LineStream>,HttpError>{self.requests.lock().unwrap().push(serde_json::from_str(body).unwrap());Ok(Box::new(Lines{lines:self.streams.lock().unwrap().pop_front().expect("extra native stream").into()}))}
}
fn route(protocol:&'static str)->ResolvedRoute {
    ResolvedRoute{target:if protocol=="anthropic"{RouteTarget::Anthropic}else{RouteTarget::Chat},tool_protocol:Some(protocol),args:RouteArgs{
        base_url:"https://synthetic.invalid/v1".into(),api_key:"SYNTHETIC_SECRET_NEVER_EVENT".into(),model:json!("synthetic-model"),messages:vec![json!({"role":"user","content":"PRIOR_CHAT_NEVER_SENT"})],
        temperature:json!(0.2),stream:false,endpoint_mode:"prefix".into(),custom_headers:json!({})}}
}
fn snapshot(review:&review::Service)->Value {
    let source="A original Z\n";
    review.handle(&json!({"action":"open","path":"C:/synthetic-harness.md","content":source,"disk_content":source,
        "disk_revision":crate::document_transaction::revision(source),"encoding":"utf-8","line_ending":"lf"})).unwrap()
}
fn request(snapshot:&Value)->TaskRequest {
    TaskRequest{start_call_id:"start-once".into(),document_id:snapshot["document_id"].as_str().unwrap().into(),instruction:"Polish the selected word, then inspect and complete.".into(),
        connection:ConnectionRef{provider_id:"saved-provider-id".into(),credential_id:Some("saved-credential-id".into()),model:"saved-model-ref".into()},stream:false,caller_digest:"0".repeat(64),
        allowed_ranges:Some(vec![[2,10]]),budget:Budget::default(),template:None}
}
fn args(snapshot:&Value)->Value {json!({"document_id":snapshot["document_id"]})}
fn proposal(snapshot:&Value,generation:u64,revision:u64,insert:&str)->Value {
    json!({"document_id":snapshot["document_id"],"generation":snapshot["generation"].as_u64().unwrap()+generation,"review_revision":revision,"edits":[{"from":2,"to":10,"insert":insert,"original":"original"}]})
}
fn completion(snapshot:&Value,generation:u64,revision:u64)->Value {json!({"document_id":snapshot["document_id"],"generation":snapshot["generation"].as_u64().unwrap()+generation,"review_revision":revision})}
fn call(protocol:&str,id:&str,name:&str,arguments:Value)->Value {
    match protocol {
        "chat_completions"=>json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":arguments.to_string()}}]}}]}),
        "anthropic"=>json!({"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":id,"name":name,"input":arguments}]}),
        "gemini"=>json!({"candidates":[{"finishReason":"STOP","content":{"role":"model","parts":[{"functionCall":{"id":id,"name":name,"args":arguments},"thoughtSignature":"preserved-native-signature"}]}}]}),_=>panic!(),
    }
}
fn script(protocol:&str,snapshot:&Value)->Vec<Value> {
    vec![call(protocol,"r1","read_document",args(snapshot)),call(protocol,"p1","propose_review",proposal(snapshot,0,0,"edited")),
        call(protocol,"r2","read_document",args(snapshot)),call(protocol,"i1","inspect_review",args(snapshot)),call(protocol,"c1","task_complete",completion(snapshot,1,1))]
}
fn read_current(review:&review::Service,snapshot:&Value)->Value {review.handle(&json!({"action":"read","document_id":snapshot["document_id"]})).unwrap()}

#[test]
fn all_native_protocols_continue_after_durable_proposal_and_map_only_selected_range() {
    for protocol in ["chat_completions","anthropic","gemini"] {
        let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);
        let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
        let mut replies=script(protocol,&initial);
        let signed=json!({"type":"thinking","thinking":"private synthetic reasoning","signature":"preserved-native-signature"});
        if protocol=="anthropic" {replies[0]["content"].as_array_mut().unwrap().insert(0,signed.clone());}
        let transport=MockTransport::replies(replies);let permit=harness.start(&task.id).unwrap();
        let completed=harness.run(&permit,&review,&route(protocol),&transport,None,&|_|{}).unwrap();
        assert_eq!(completed.state,"completed");assert_eq!(completed.rounds,5);
        assert_eq!(completed.checkpoint.allowed_ranges,Some(vec![[2,8]]));
        let current=read_current(&review,&initial);assert_eq!(current["content"],"A edited Z\n");assert_eq!(current["changes"].as_array().unwrap().len(),1);
        let original=harness.lookup_start("start-once").unwrap().unwrap().initial_snapshot();
        assert_eq!(original["content"],initial["content"]);assert_eq!(original["generation"],1);
        assert_eq!(harness.create(&original,request(&initial)).unwrap().id,task.id);
        let second=transport.body(1);
        assert!(!transport.body(0).to_string().contains("PRIOR_CHAT_NEVER_SENT"));
        match protocol {
            "chat_completions"=>{assert_eq!(second["messages"][2]["tool_calls"][0]["id"],"r1");let result:Value=serde_json::from_str(second["messages"][3]["content"].as_str().unwrap()).unwrap();assert_eq!(result["allowed_ranges"],json!([[2,10]]));},
            "anthropic"=>assert_eq!(second["messages"][1]["content"][0],signed),
            "gemini"=>assert_eq!(second["contents"][1]["parts"][0]["thoughtSignature"],"preserved-native-signature"),_=>{},
        }
        let events=harness.events(&task.id,0).unwrap();assert!(events.iter().any(|event|event["type"]=="completed"));
        let public=format!("{}{}",completed.public_view(),serde_json::to_string(&events).unwrap());
        for private in ["SYNTHETIC_SECRET_NEVER_EVENT","synthetic.invalid","private synthetic reasoning","saved-credential-id","original"] {assert!(!public.contains(private));}
    }
}

#[test]
fn task_complete_requires_latest_inspection_and_native_call() {
    for replies_kind in [0,1] {
        let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
        let end=if replies_kind==0 {call("chat_completions","c","task_complete",completion(&initial,0,0))} else {json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Done","tool_calls":[]}}]})};
        let transport=MockTransport::replies(vec![call("chat_completions","r","read_document",args(&initial)),end]);let permit=harness.start(&task.id).unwrap();
        let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.state,"failed");assert_eq!(result.error_code.as_deref(),Some("review_ai_invalid_tool"));
    }
}

#[test]
fn changed_payload_with_same_call_id_is_rejected_without_second_mutation() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
    let mut replies=script("chat_completions",&initial);replies.truncate(3);
    replies.push(call("chat_completions","p1","propose_review",json!({"document_id":initial["document_id"],"generation":2,"review_revision":1,"edits":[{"from":2,"to":8,"insert":"changed"}]})));
    let transport=MockTransport::replies(replies);let permit=harness.start(&task.id).unwrap();let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();
    assert_eq!(result.error_code.as_deref(),Some("review_call_conflict"));assert_eq!(read_current(&review,&initial)["content"],"A edited Z\n");
}

#[test]
fn model_arguments_cannot_widen_source_authority() {
    for bad in [json!({"document_id":"other-document"}),json!({"document_id":"placeholder","path":"C:/outside.md"})] {
        let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
        let transport=MockTransport::replies(vec![call("chat_completions","r","read_document",bad)]);let permit=harness.start(&task.id).unwrap();let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.error_code.as_deref(),Some("review_ai_invalid_tool"));
    }
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
    let transport=MockTransport::replies(vec![call("chat_completions","r","read_document",args(&initial)),call("chat_completions","p","propose_review",json!({"document_id":initial["document_id"],"generation":1,"review_revision":0,"edits":[{"from":0,"to":1,"insert":"OUTSIDE"}]}))]);
    let permit=harness.start(&task.id).unwrap();let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.error_code.as_deref(),Some("review_ai_invalid_tool"));assert_eq!(read_current(&review,&initial)["content"],initial["content"]);
}

#[test]
fn rounds_and_elapsed_budget_survive_pause_and_resume() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let mut input=request(&initial);input.budget.max_rounds=2;let task=harness.create(&initial,input).unwrap();
    let transport=MockTransport::replies(vec![call("chat_completions","r1","read_document",args(&initial)),call("chat_completions","r2","read_document",args(&initial))]);let permit=harness.start(&task.id).unwrap();
    let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.state,"budget_exhausted");assert_eq!(result.rounds,2);assert_eq!(harness.start(&task.id).err().unwrap(),"review_ai_budget");
    let second=Service::new(directory.path());assert_eq!(second.get(&task.id).unwrap().rounds,2);
    second.store.run(|tx|{let mut task=db::load(tx,&task.id)?;task.elapsed_ms=task.request.budget.max_elapsed_ms;db::put(tx,&task)}).unwrap();assert_eq!(second.start(&task.id).err().unwrap(),"review_ai_budget");
}

struct PauseOnReply<'a>{harness:&'a Service,id:String,reply:Value}
impl Transport for PauseOnReply<'_> {
    fn post_json(&self,_:&str,_:&Headers,_:&str)->Result<String,HttpError>{self.harness.pause(&self.id).unwrap();Ok(self.reply.to_string())}
    fn open_stream(&self,_:&str,_:&Headers,_:&str)->Result<Box<dyn LineStream>,HttpError>{panic!()}
}
#[test]
fn pause_wins_over_late_network_reply_and_does_not_commit_tool() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();let permit=harness.start(&task.id).unwrap();
    let transport=PauseOnReply{harness:&harness,id:task.id.clone(),reply:call("chat_completions","r","read_document",args(&initial))};
    let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.state,"paused");assert!((permit.checker())());assert!(result.calls.is_empty());assert_eq!(read_current(&review,&initial)["generation"],1);
}

#[test]
fn manual_change_stops_before_another_network_request_and_preserves_human_source() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
    review.handle(&json!({"action":"edit","document_id":initial["document_id"],"generation":1,"review_revision":0,"call_id":"human","edits":[{"from":0,"to":1,"insert":"HUMAN"}]})).unwrap();
    let transport=MockTransport::replies(vec![]);let permit=harness.start(&task.id).unwrap();let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.state,"needs_user_resume");assert_eq!(result.error_code.as_deref(),Some("review_stale_generation"));assert!(transport.requests.lock().unwrap().is_empty());assert_eq!(read_current(&review,&initial)["content"],"HUMAN original Z\n");
}

fn leave_intent(harness:&Service,permit:&RunPermit,initial:&Value)->Value {
    let mut call=make_call(Some("p-crash".into()),"propose_review".into(),proposal(initial,0,0,"edited"),1,0).unwrap();
    call.review_call_id=format!("harness:{}:{}",permit.task_id,digest(&json!(call.key)));call.phase="intent".into();
    let request=json!({"action":"propose","document_id":initial["document_id"],"generation":1,"review_revision":0,"call_id":call.review_call_id,"origin":"ai","task_id":permit.task_id,"edits":[{"from":2,"to":10,"insert":"edited"}],"dependencies":[]});
    call.review_request=Some(request.clone());
    call.reserved_bytes=review::MAX_DOCUMENT_BYTES+64*1024;
    harness.guarded(permit,|_,task|{task.pending=Some(PendingBatch{calls:vec![call.clone()],next:0});task.calls.insert(call.key.clone(),CallReceipt{name:call.name.clone(),digest:call.digest.clone(),completed:false});Ok(())}).unwrap();request
}
#[test]
fn restart_mid_tool_queries_durable_receipt_and_never_repeats_proposal() {
    for committed in [true,false] {
        let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let first=Service::new(directory.path());let task=first.create(&initial,request(&initial)).unwrap();let permit=first.start(&task.id).unwrap();let intent=leave_intent(&first,&permit,&initial);
        assert!(first.get(&task.id).unwrap().pending.as_ref().unwrap().calls[0].reserved_bytes>review::MAX_DOCUMENT_BYTES);
        if committed {review.propose_task(&intent).unwrap();}
        drop(permit);drop(first);
        let restarted=Service::new(directory.path());assert_eq!(restarted.recover_running().unwrap(),1);assert_eq!(restarted.get(&task.id).unwrap().state,"needs_user_resume");
        let replies=if committed {vec![call("chat_completions","r-new","read_document",args(&initial)),call("chat_completions","i-new","inspect_review",args(&initial)),call("chat_completions","c-new","task_complete",completion(&initial,1,1))]}else{vec![]};
        let transport=MockTransport::replies(replies);let new_permit=restarted.start(&task.id).unwrap();let result=restarted.run(&new_permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();
        if committed {assert_eq!(result.state,"completed");assert!(result.pending.is_none());assert_eq!(read_current(&review,&initial)["changes"].as_array().unwrap().len(),1);}else{assert_eq!(result.state,"needs_user_resume");assert!(result.pending.as_ref().unwrap().calls[0].reserved_bytes>0);assert_eq!(result.error_code.as_deref(),Some("review_ai_interrupted"));assert!(transport.requests.lock().unwrap().is_empty());assert_eq!(read_current(&review,&initial)["generation"],1);}
    }
}

#[test]
fn create_is_idempotent_conflicting_start_and_store_budget_are_not_success() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let mut harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();assert_eq!(harness.create(&initial,request(&initial)).unwrap().id,task.id);
    let mut changed=request(&initial);changed.instruction.push('!');assert_eq!(harness.create(&initial,changed).unwrap_err(),"review_call_conflict");
    harness.store.byte_limit=1;let mut input=request(&initial);input.start_call_id="start-two".into();assert_eq!(harness.create(&initial,input).unwrap_err(),"review_ai_budget");
}

fn sse(values:Vec<Value>,done:bool)->Vec<String> {let mut lines=Vec::new();for value in values{lines.push(format!("data: {value}"));lines.push(String::new());}if done{lines.push("data: [DONE]".into());lines.push(String::new());}lines}
fn chat_stream(id:&str,name:&str,arguments:&Value)->Vec<String> {
    let arguments=arguments.to_string();let split=arguments.char_indices().nth(arguments.chars().count()/2).map(|(byte,_)|byte).unwrap_or(0);
    sse(vec![json!({"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":name,"arguments":&arguments[..split]}}]},"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":&arguments[split..]}}]},"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})],true)
}
#[test]
fn actual_chat_stream_reassembles_split_tool_json_and_completes_multiple_rounds() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();let permit=harness.start(&task.id).unwrap();
    let transport=MockTransport::streams(vec![chat_stream("r1","read_document",&args(&initial)),chat_stream("p1","propose_review",&proposal(&initial,0,0,"edited")),chat_stream("r2","read_document",&args(&initial)),chat_stream("i1","inspect_review",&args(&initial)),chat_stream("c1","task_complete",&completion(&initial,1,1))]);
    let mut selected=route("chat_completions");selected.args.stream=true;let result=harness.run(&permit,&review,&selected,&transport,None,&|_|{}).unwrap();assert_eq!(result.state,"completed");assert_eq!(transport.body(0)["stream"],true);
}

fn assemble(protocol:Protocol,lines:Vec<String>)->Result<Value,AiError>{let mut stream=Lines{lines:lines.into()};let mut total=0;assemble_stream(protocol,&mut stream,&||Ok(()),&mut total)}
#[test]
fn streamed_anthropic_json_and_signed_thinking_survive_native_round_trip() {
    let args=json!({"document_id":"document"});let encoded=args.to_string();let split=encoded.len()/2;
    let events=vec![json!({"type":"message_start","message":{"role":"assistant"}}),json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"reason"}}),json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed"}}),json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"r","name":"read_document","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":&encoded[..split]}}),json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":&encoded[split..]}}),
        json!({"type":"content_block_stop","index":1}),json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),json!({"type":"message_stop"})];
    let value=assemble(Protocol::Anthropic,sse(events,false)).unwrap();let parsed=parse_response(Protocol::Anthropic,&value,1).unwrap();assert_eq!(parsed.calls[0].arguments,args);assert_eq!(parsed.assistant["content"][0]["signature"],"signed");
}

#[test]
fn streamed_gemini_preserves_native_function_and_thought_signature() {
    let native=json!({"functionCall":{"id":"read","name":"read_document","args":{"document_id":"document"}},"thoughtSignature":"gemini-signed"});
    let value=assemble(Protocol::Gemini,sse(vec![json!({"candidates":[{"index":0,"content":{"role":"model","parts":[native]}}]}),json!({"candidates":[{"index":0,"finishReason":"STOP"}]})],false)).unwrap();
    let parsed=parse_response(Protocol::Gemini,&value,1).unwrap();assert_eq!(parsed.assistant["parts"][0]["thoughtSignature"],"gemini-signed");
}

#[test]
fn streamed_gemini_partial_arguments_are_joined_before_any_tool_executes() {
    let events=vec![json!({"candidates":[{"index":0,"content":{"role":"model","parts":[{"functionCall":{"id":"r","name":"read_document","partialArgs":[{"jsonPath":"$.document_id","stringValue":"docu"}],"willContinue":true},"thoughtSignature":"signed"}]}}]}),
        json!({"candidates":[{"index":0,"content":{"role":"model","parts":[{"functionCall":{"id":"r","partialArgs":[{"jsonPath":"$.document_id","stringValue":"ment"}],"willContinue":false}}]},"finishReason":"STOP"}]})];
    let value=assemble(Protocol::Gemini,sse(events,false)).unwrap();let parsed=parse_response(Protocol::Gemini,&value,1).unwrap();assert_eq!(parsed.calls.len(),1);assert_eq!(parsed.calls[0].arguments,json!({"document_id":"document"}));assert_eq!(parsed.assistant["parts"][0]["thoughtSignature"],"signed");
    let unfinished=vec![json!({"candidates":[{"index":0,"content":{"role":"model","parts":[{"functionCall":{"id":"r","name":"read_document","partialArgs":[{"jsonPath":"$.document_id","stringValue":"doc"}],"willContinue":true}}]},"finishReason":"STOP"}]})];
    assert_eq!(assemble(Protocol::Gemini,sse(unfinished,false)).unwrap_err().message,"ai_invalid_response");
}

#[test]
fn stream_rejects_truncation_malformed_json_final_stop_and_oversize() {
    let mut missing_done=chat_stream("r","read_document",&json!({"document_id":"document"}));missing_done.truncate(missing_done.len()-2);assert_eq!(assemble(Protocol::Chat,missing_done).unwrap_err().message,"ai_invalid_response");
    assert_eq!(assemble(Protocol::Chat,vec!["data: {bad".into(),String::new()]).unwrap_err().message,"ai_invalid_response");
    let bad=sse(vec![json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"r","type":"function","function":{"name":"read_document","arguments":"{bad"}}]},"finish_reason":"tool_calls"}]})],true);
    let value=assemble(Protocol::Chat,bad).unwrap();assert_eq!(parse_response(Protocol::Chat,&value,1).err().unwrap().message,"review_ai_invalid_tool");
    assert_eq!(assemble(Protocol::Chat,vec![format!("data: {}","x".repeat(MAX_MESSAGE+1))]).unwrap_err().message,"review_ai_budget");
    let mut stream=Lines{lines:chat_stream("r","read_document",&json!({"document_id":"d"})).into()};let mut total=0;
    assert_eq!(assemble_stream(Protocol::Chat,&mut stream,&||Err(err("cancelled")),&mut total).unwrap_err().message,"cancelled");
}

#[test]
fn parallel_native_calls_have_one_assistant_and_ordered_matching_results() {
    let response=json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[
        {"id":"r","type":"function","function":{"name":"read_document","arguments":"{\"document_id\":\"d\"}"}},
        {"id":"i","type":"function","function":{"name":"inspect_review","arguments":"{\"document_id\":\"d\"}"}}]}}]});
    let batch=parse_response(Protocol::Chat,&response,1).unwrap();let mut history=vec![];append_results(Protocol::Chat,&mut history,&batch,&[json!({"read":true}),json!({"inspect":true})]);
    assert_eq!(history.len(),3);assert_eq!(history[1]["tool_call_id"],"r");assert_eq!(history[2]["tool_call_id"],"i");
}

#[test]
fn a_followup_updates_own_candidate_versions_and_records_dependencies() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();
    let replies=vec![call("chat_completions","r1","read_document",args(&initial)),call("chat_completions","p1","propose_review",proposal(&initial,0,0,"edited")),call("chat_completions","r2","read_document",args(&initial)),
        call("chat_completions","p2","propose_review",json!({"document_id":initial["document_id"],"generation":2,"review_revision":1,"edits":[{"from":2,"to":8,"insert":"polished","original":"edited"}]})),
        call("chat_completions","r3","read_document",args(&initial)),call("chat_completions","i","inspect_review",args(&initial)),call("chat_completions","c","task_complete",completion(&initial,2,2))];
    let transport=MockTransport::replies(replies);let permit=harness.start(&task.id).unwrap();let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();
    assert_eq!(result.state,"completed");assert_eq!(result.checkpoint.allowed_ranges,Some(vec![[2,10]]));let current=read_current(&review,&initial);assert_eq!(current["content"],"A polished Z\n");
    assert_eq!(current["changes"][0]["taken_over"],false);assert!(current["changes"][0]["candidate_versions"].as_array().unwrap().contains(&json!("edited")));
    assert_eq!(current["changes"].as_array().unwrap().len(),1);assert_eq!(current["changes"][0]["original"],"original");assert_eq!(current["changes"][0]["origin"],"ai");
    let old=review.handle(&json!({"action":"receipt","document_id":initial["document_id"],"call_id":format!("harness:{}:{}",task.id,digest(&json!("p1")))})).unwrap();
    assert_eq!(current["changes"][0]["id"],old["snapshot"]["changes"][0]["id"]);
}

#[test]
fn a_manual_takeover_between_rounds_cannot_be_overwritten_by_a_late_candidate() {
    struct HumanOnSecondRequest<'a>{review:&'a review::Service,initial:Value,count:std::cell::Cell<usize>}
    impl Transport for HumanOnSecondRequest<'_>{
        fn post_json(&self,_:&str,_:&Headers,_:&str)->Result<String,HttpError>{let count=self.count.get();self.count.set(count+1);if count==0{return Ok(call("chat_completions","r","read_document",args(&self.initial)).to_string());}
            self.review.handle(&json!({"action":"edit","document_id":self.initial["document_id"],"generation":1,"review_revision":0,"call_id":"human-race","edits":[{"from":2,"to":10,"insert":"HUMAN"}]})).unwrap();
            Ok(call("chat_completions","p","propose_review",proposal(&self.initial,0,0,"STALE_AI")).to_string())}
        fn open_stream(&self,_:&str,_:&Headers,_:&str)->Result<Box<dyn LineStream>,HttpError>{panic!()}
    }
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();let permit=harness.start(&task.id).unwrap();
    let transport=HumanOnSecondRequest{review:&review,initial:initial.clone(),count:std::cell::Cell::new(0)};let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();
    assert_eq!(result.state,"needs_user_resume");assert_eq!(read_current(&review,&initial)["content"],"A HUMAN Z\n");assert!(read_current(&review,&initial)["changes"].as_array().unwrap().is_empty());
}

#[test]
fn source_scope_does_not_absorb_another_tasks_candidate() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);
    let existing=review.handle(&json!({"action":"propose","document_id":initial["document_id"],"generation":1,"review_revision":0,"call_id":"other-task","task_id":"other-task","edits":[{"from":2,"to":10,"insert":"existing"}]})).unwrap();
    let harness=Service::new(directory.path());let task=harness.create(&existing,request(&existing)).unwrap();let permit=harness.start(&task.id).unwrap();
    let transport=MockTransport::replies(vec![call("chat_completions","r","read_document",args(&existing)),call("chat_completions","p","propose_review",json!({"document_id":existing["document_id"],"generation":2,"review_revision":1,"edits":[{"from":2,"to":10,"insert":"overwrite"}]}))]);
    let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();assert_eq!(result.state,"needs_user_resume");assert_eq!(read_current(&review,&initial)["content"],"A existing Z\n");assert_eq!(read_current(&review,&initial)["changes"][0]["taken_over"],false);
}

#[test]
fn recovery_charges_bounded_unknown_work_without_counting_days_offline() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let first=Service::new(directory.path());let task=first.create(&initial,request(&initial)).unwrap();let _permit=first.start(&task.id).unwrap();
    first.store.run(|tx|{let mut task=db::load(tx,&task.id)?;task.elapsed_ms=1234;task.active_since_ms=Some(db::now_ms().saturating_sub(7*24*60*60*1000));db::put(tx,&task)}).unwrap();
    drop(_permit);drop(first);
    let restarted=Service::new(directory.path());assert_eq!(restarted.recover_running().unwrap(),1);let recovered=restarted.get(&task.id).unwrap();assert_eq!(recovered.elapsed_ms,181234);assert_eq!(recovered.state,"needs_user_resume");assert!(recovered.owner.is_none());
}

#[test]
fn tool_completion_capacity_is_admitted_before_review_can_mutate() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let mut harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();let permit=harness.start(&task.id).unwrap();
    let used=harness.store.run(db::used_bytes).unwrap();harness.store.byte_limit=used+256*1024;
    let transport=MockTransport::replies(vec![call("chat_completions","r","read_document",args(&initial)),call("chat_completions","p","propose_review",proposal(&initial,0,0,"edited"))]);
    let result=harness.run(&permit,&review,&route("chat_completions"),&transport,None,&|_|{}).unwrap();
    assert_eq!(result.state,"budget_exhausted");assert_eq!(result.error_code.as_deref(),Some("review_ai_budget"));assert_eq!(result.reserved_bytes,0);
    let current=read_current(&review,&initial);assert_eq!(current["content"],initial["content"]);assert_eq!(current["generation"],initial["generation"]);assert!(current["changes"].as_array().unwrap().is_empty());
    let pending=result.pending.as_ref().unwrap();assert_eq!(pending.calls[0].phase,"planned");assert_eq!(pending.calls[0].reserved_bytes,0);
}

#[test]
fn an_admitted_terminal_transition_can_pause_at_the_logical_quota_boundary() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let mut harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();let _permit=harness.start(&task.id).unwrap();
    harness.store.byte_limit=harness.store.run(db::used_bytes).unwrap();let paused=harness.pause(&task.id).unwrap();assert_eq!(paused.state,"paused");assert_eq!(paused.reserved_bytes,0);
    assert!(harness.store.run(db::used_bytes).unwrap()<harness.store.byte_limit);
}

#[test]
fn cumulative_deadline_during_blocked_stream_read_is_budget_exhaustion_not_user_pause() {
    struct ExpiresOnRead{control:Arc<Control>}
    impl LineStream for ExpiresOnRead{
        fn next_line(&mut self)->Result<Option<String>,String>{self.control.expires_at_ms.store(0,Ordering::Release);Err("cancelled".into())}
    }
    struct DeadlineTransport{control:Arc<Control>}
    impl Transport for DeadlineTransport{
        fn post_json(&self,_:&str,_:&Headers,_:&str)->Result<String,HttpError>{panic!()}
        fn open_stream(&self,_:&str,_:&Headers,_:&str)->Result<Box<dyn LineStream>,HttpError>{Ok(Box::new(ExpiresOnRead{control:self.control.clone()}))}
    }
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let harness=Service::new(directory.path());let task=harness.create(&initial,request(&initial)).unwrap();let permit=harness.start(&task.id).unwrap();
    let transport=DeadlineTransport{control:permit.control.clone()};let mut selected=route("chat_completions");selected.args.stream=true;
    let result=harness.run(&permit,&review,&selected,&transport,None,&|_|{}).unwrap();assert!(!permit.control.cancelled.load(Ordering::Acquire));assert_eq!(result.state,"budget_exhausted");assert_eq!(result.error_code.as_deref(),Some("review_ai_budget"));assert!(result.calls.is_empty());
}

#[test]
fn live_owner_survives_another_service_and_external_pause_reaches_its_checker() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let first=Service::new(directory.path());let second=Service::new(directory.path());
    let task=first.create(&initial,request(&initial)).unwrap();let permit=first.start(&task.id).unwrap();
    assert_eq!(second.recover_running().unwrap(),0);assert_eq!(second.get(&task.id).unwrap().state,"running");assert_eq!(second.start(&task.id).err().unwrap(),"review_conflict");
    second.pause(&task.id).unwrap();*permit.control.last_poll.lock().unwrap()=Instant::now()-Duration::from_millis(101);
    assert!((permit.checker())());assert!(!permit.control.cancelled.load(Ordering::Acquire));assert_eq!(permit.check_local().unwrap_err().message,"cancelled");drop(permit);
    let resumed=second.start(&task.id).unwrap();assert_eq!(second.get(&task.id).unwrap().state,"running");drop(resumed);
}

#[test]
fn profile_slots_are_shared_between_independent_service_instances() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);
    let first=Service::new(directory.path());let second=Service::new(directory.path());let third=Service::new(directory.path());
    let a=first.create(&initial,request(&initial)).unwrap();let mut b=request(&initial);b.start_call_id="second".into();let b=second.create(&initial,b).unwrap();let mut c=request(&initial);c.start_call_id="third".into();let c=third.create(&initial,c).unwrap();
    let pa=first.start(&a.id).unwrap();let pb=second.start(&b.id).unwrap();assert_eq!(third.start(&c.id).err().unwrap(),"review_ai_budget");
    drop(pa);assert_eq!(third.recover_running().unwrap(),1);assert_eq!(third.get(&a.id).unwrap().state,"needs_user_resume");assert_eq!(third.get(&b.id).unwrap().state,"running");
    let pc=third.start(&c.id).unwrap();drop(pb);drop(pc);
}

#[test]
fn exiting_one_service_pauses_only_its_owners_and_preserves_a_live_sibling() {
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let first=Service::new(directory.path());let second=Service::new(directory.path());
    let own=first.create(&initial,request(&initial)).unwrap();let mut other=request(&initial);other.start_call_id="other-service".into();let other=second.create(&initial,other).unwrap();
    let own_permit=first.start(&own.id).unwrap();let other_permit=second.start(&other.id).unwrap();assert_eq!(first.pause_owned().unwrap(),1);
    assert_eq!(first.get(&own.id).unwrap().state,"paused");assert!(own_permit.control.cancelled.load(Ordering::Acquire));
    assert_eq!(second.get(&other.id).unwrap().state,"running");assert!(!(other_permit.checker())());assert_eq!(first.pause_owned().unwrap(),0);
    // A stale local owner cannot pause a newer owner of the same durable ID.
    let stale=own_permit.control.clone();drop(own_permit);let resumed=second.start(&own.id).unwrap();first.active.lock().unwrap().insert(own.id.clone(),stale);
    assert_eq!(first.pause_owned().unwrap(),0);assert_eq!(second.get(&own.id).unwrap().state,"running");assert!(!(resumed.checker())());
    first.active.lock().unwrap().remove(&own.id);drop(resumed);drop(other_permit);
}

#[test]
fn task_os_lease_is_profile_wide_and_crash_releases_only_dead_owner() {
    const CHILD:&str="READMD_AI_TASK_LEASE_CHILD";
    if let Some(root)=std::env::var_os(CHILD) {
        let root=std::path::PathBuf::from(root);let service=Service::new(&root);let id=std::fs::read_to_string(root.join("child-task-id")).unwrap();let _permit=service.start(&id).unwrap();
        std::fs::write(root.join("child-ready"),b"ready").unwrap();let until=Instant::now()+Duration::from_secs(15);
        while Instant::now()<until {std::thread::sleep(Duration::from_millis(10));}
        panic!("Lease-test parent did not terminate its owned child");
    }
    struct Child(std::process::Child);
    impl Drop for Child{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let directory=tempfile::tempdir().unwrap();let review=review::Service::new(directory.path());let initial=snapshot(&review);let parent=Service::new(directory.path());let task=parent.create(&initial,request(&initial)).unwrap();
    std::fs::write(directory.path().join("child-task-id"),&task.id).unwrap();
    let mut child=Child(std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","ai_harness::tests::task_os_lease_is_profile_wide_and_crash_releases_only_dead_owner","--test-threads=1"])
        .env(CHILD,directory.path()).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap());
    let until=Instant::now()+Duration::from_secs(10);
    while !directory.path().join("child-ready").is_file(){assert!(child.0.try_wait().unwrap().is_none(),"Lease child exited before ready");assert!(Instant::now()<until,"Lease child readiness deadline");std::thread::sleep(Duration::from_millis(10));}
    let other=Service::new(directory.path());assert_eq!(other.recover_running().unwrap(),0);assert_eq!(other.start(&task.id).err().unwrap(),"review_conflict");
    let mut input=request(&initial);input.start_call_id="parent-sibling".into();let sibling=other.create(&initial,input).unwrap();let sibling_permit=other.start(&sibling.id).unwrap();
    let mut input=request(&initial);input.start_call_id="third-waits".into();let third=parent.create(&initial,input).unwrap();assert_eq!(parent.start(&third.id).err().unwrap(),"review_ai_budget");
    child.0.kill().unwrap();child.0.wait().unwrap();assert_eq!(parent.recover_running().unwrap(),1);assert_eq!(parent.get(&task.id).unwrap().state,"needs_user_resume");assert_eq!(parent.get(&sibling.id).unwrap().state,"running");
    let replacement=parent.start(&third.id).unwrap();drop(replacement);drop(sibling_permit);
}
