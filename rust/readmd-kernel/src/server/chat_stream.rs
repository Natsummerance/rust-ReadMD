//! Accept-negotiated chat. Provider deltas stay deltas; JSON/MCP stays JSON.
use super::*;
use crate::ai_providers::{Headers, HttpError, LineStream, Transport};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

pub(super) struct Peer {
    pub(super) lost: Arc<AtomicBool>, finished: Arc<AtomicBool>, worker: Option<std::thread::JoinHandle<()>>,
}
impl Peer {
    pub(super) fn watch(socket: &TcpStream) -> ApiResult<Self> {
        let socket=socket.try_clone().map_err(|_|ApiError::new(409,"cancelled"))?;
        socket.set_nonblocking(false).and_then(|_|socket.set_read_timeout(Some(Duration::from_millis(50))))
            .map_err(|_|ApiError::new(409,"cancelled"))?;
        let lost=Arc::new(AtomicBool::new(false));let finished=Arc::new(AtomicBool::new(false));
        let lost_flag=lost.clone();let stop=finished.clone();
        let worker=std::thread::Builder::new().name("readmd-chat-peer".into()).spawn(move||{
            let mut byte=[0u8;1];
            while !stop.load(Ordering::Acquire) {
                match socket.peek(&mut byte) {
                    Ok(0) => { lost_flag.store(true,Ordering::Release);break; },
                    Ok(_) => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut|std::io::ErrorKind::Interrupted) => (),
                    Err(_) => { lost_flag.store(true,Ordering::Release);break; },
                }
            }
        }).map_err(|_|ApiError::new(503,"review_ai_budget"))?;
        Ok(Self{lost,finished,worker:Some(worker)})
    }
}
impl Drop for Peer {
    fn drop(&mut self) { self.finished.store(true,Ordering::Release);if let Some(worker)=self.worker.take(){let _=worker.join();} }
}

pub(super) fn write_event_bytes(writer:&mut TcpStream,mut bytes:&[u8],cancelled:&dyn Fn()->bool,budget:Duration)->ApiResult<()> {
    let deadline=Instant::now()+budget;
    writer.set_write_timeout(Some(Duration::from_millis(50))).map_err(|_|ApiError::new(409,"cancelled"))?;
    while !bytes.is_empty() {
        if cancelled()||Instant::now()>=deadline{return Err(ApiError::new(409,"cancelled"));}
        match writer.write(bytes) {
            Ok(0)=>return Err(ApiError::new(409,"cancelled")),
            Ok(count)=>bytes=&bytes[count..],
            Err(error)if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut|std::io::ErrorKind::Interrupted)=>(),
            Err(_)=>return Err(ApiError::new(409,"cancelled")),
        }
    }
    Ok(())
}

pub(super) fn serve(app:&Arc<App>,req:&Request,writer:&mut TcpStream) {
    let peer=match Peer::watch(writer) {Ok(peer)=>peer,Err(error)=>{let _=write_response(writer,&error_response(error),false,false);return;}};
    let started=std::cell::Cell::new(false);
    let outcome;
    {
        let mut emit=|value:Value,cancelled:&dyn Fn()->bool|->ApiResult<()> {
            let budget=if value["type"]=="error"{Duration::from_millis(50)}else{Duration::from_secs(5)};
            if !started.get() {
                write_event_bytes(writer,b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-store\r\nX-Accel-Buffering: no\r\nConnection: close\r\n\r\n",cancelled,budget)?;started.set(true);
            }
            write_event_bytes(writer,format!("data: {value}\n\n").as_bytes(),cancelled,budget)
        };
        outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let result=run(app,req,peer.lost.clone(),&mut emit);
            match result {
                Ok(())=>Ok(()),
                Err(error) if started.get()=>{
                    let mut value:Value=serde_json::from_slice(&error_response(error).body).unwrap_or_else(|_|json!({"ok":false,"error_code":"internal_error"}));
                    value["type"]=json!("error");emit(value,&||peer.lost.load(Ordering::Acquire))
                },
                Err(error)=>Err(error),
            }
        }));
    }
    match outcome {
        Ok(Err(error)) if !started.get()&&error.code!="cancelled"=>{let _=writer.set_nonblocking(true);let _=write_response(writer,&error_response(error),false,false);},
        Err(_) if !started.get()=>{let _=writer.set_nonblocking(true);let _=write_response(writer,&internal_error_response(),false,false);},
        Err(_)=>{let _=write_event_bytes(writer,b"data: {\"type\":\"error\",\"ok\":false,\"error_code\":\"internal_error\"}\n\n",&||peer.lost.load(Ordering::Acquire),Duration::from_millis(100));},
        _=>(),
    }
}

fn run(app:&Arc<App>,req:&Request,lost:Arc<AtomicBool>,emit:&mut dyn FnMut(Value,&dyn Fn()->bool)->ApiResult<()>)->ApiResult<()> {
    let (payload,route)=prepare_chat_route(app,req).map_err(safe_error)?;
    let task_id=match payload.get("task_id") {
        None=>uuid::Uuid::new_v4().to_string(),
        Some(Value::String(id)) if crate::cancel::valid_id(id)=>id.clone(),
        _=>return Err(ApiError::bad_request("invalid_task_id")),
    };
    let task=crate::cancel::register(&task_id).ok_or_else(||ApiError::bad_request("invalid_task_id"))?;
    let task_cancelled=task.checker();
    let cancelled:Arc<dyn Fn()->bool+Send+Sync>=Arc::new(move||lost.load(Ordering::Acquire)||task_cancelled());
    let transport=crate::review_transport::ReviewTransport::new_owned(cancelled.clone());
    let strict=StrictTransport(&transport);
    emit(json!({"type":"start","task_id":task_id}),cancelled.as_ref())?;
    let stream=ai_providers::dispatch_chat(&route,&strict).map_err(|error|provider_error(error,cancelled()))?;
    let mut content=String::new();let mut usage=None;let mut units=0usize;
    for event in stream {
        if cancelled(){return Err(ApiError::new(409,"cancelled"));}
        match event.map_err(|error|provider_error(error,cancelled()))? {
            ai_providers::ChatEvent::Delta(Value::String(delta))=>{
                units=units.saturating_add(delta.encode_utf16().count());
                if units>1_000_000||content.len().saturating_add(delta.len())>8*1024*1024{return Err(ApiError::new(422,"review_ai_budget"));}
                content.push_str(&delta);emit(json!({"type":"delta","task_id":task_id,"delta":delta}),cancelled.as_ref())?;
            },
            ai_providers::ChatEvent::Usage(value) if value.truthy()=>usage=Some(value.to_json()),
            _=>(),
        }
    }
    if cancelled(){return Err(ApiError::new(409,"cancelled"));}
    if crate::ai_completion::retired_model_notice(&content){return Err(ApiError::new(502,"provider_model_unavailable"));}
    if !task.try_commit(){return Err(ApiError::new(409,"cancelled"));}
    if cancelled(){return Err(ApiError::new(409,"cancelled"));}
    let incognito=payload.get("incognito").and_then(Value::as_bool).unwrap_or(false);
    if !incognito {
        let session=payload.get("session").and_then(value_to_string).unwrap_or_else(||"default".into());
        app.store.remember_chat_turn(&session,&chat_user_text(&payload),&content,&ai_providers::py_str(&route.args.model))
            .map_err(|_|ApiError::internal("ai_session_save_failed"))?;
    }
    if let Some(usage)=usage {emit(json!({"type":"usage","task_id":task_id,"usage":usage}),cancelled.as_ref())?;}
    emit(json!({"type":"done","task_id":task_id,"ok":true,"persisted":!incognito}),cancelled.as_ref())
}

fn safe_error(error:ApiError)->ApiError {
    match error.code.as_str() {
        "invalid_request"|"ai_not_configured"|"invalid_task_id"=>error,
        _=>ApiError::new(502,"ai_invalid_response"),
    }
}
pub(super) fn provider_error(error:ai_providers::AiError,cancelled:bool)->ApiError {
    if cancelled{return ApiError::new(409,"cancelled");}
    if let Some(rest)=error.message.strip_prefix("HTTP ") {
        let status=rest.chars().take_while(char::is_ascii_digit).collect::<String>();
        let mut safe=ApiError::new(502,"ai_http_error");
        if status.parse::<u16>().is_ok_and(|code|(100..600).contains(&code)){safe=safe.noted("status",status);}
        return safe;
    }
    let code=if error.message.contains("review_ai_budget"){"review_ai_budget"}
        else if error.message.contains("review_ai_unsupported"){"review_ai_unsupported"}
        else if error.message.starts_with("网络错误")||error.message.contains("ai_network_error"){"ai_network_error"}
        else{"ai_invalid_response"};
    ApiError::new(502,code)
}

pub(super) struct StrictTransport<'a>(pub(super) &'a dyn Transport);
impl Transport for StrictTransport<'_> {
    fn post_json(&self,url:&str,headers:&Headers,body:&str)->Result<String,HttpError>{
        let text=self.0.post_json(url,headers,body)?;
        validate_json(&text).map_err(HttpError::Network)?;
        Ok(text)
    }
    fn open_stream(&self,url:&str,headers:&Headers,body:&str)->Result<Box<dyn LineStream>,HttpError>{
        let mut inner=self.0.open_stream(url,headers,body)?;
        let mime=inner.content_type().unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        let json=mime=="application/json"||mime.ends_with("+json");
        if json {
            // JSON fallback is a single complete reply. Validate it before
            // the provider's generic JSON reader can erase the failure code.
            let mut text=String::new();
            while let Some(line)=inner.next_line().map_err(HttpError::Network)? {
                if text.len().saturating_add(line.len())>8*1024*1024{return Err(HttpError::Network("review_ai_budget".into()));}
                text.push_str(&line);
            }
            validate_json(&text).map_err(HttpError::Network)?;
            return Ok(Box::new(JsonReply(Some(text))));
        }
        if !mime.is_empty()&&mime!="text/event-stream"{return Err(HttpError::Network("ai_invalid_response".into()));}
        Ok(Box::new(StrictLines{inner,terminal:false,buffer:String::new(),has_data:false,ended:false}))
    }
}
struct JsonReply(Option<String>);
impl LineStream for JsonReply {
    fn content_type(&self)->Option<&str>{Some("application/json")}
    fn next_line(&mut self)->Result<Option<String>,String>{Ok(self.0.take())}
}
struct StrictLines {inner:Box<dyn LineStream>,terminal:bool,buffer:String,has_data:bool,ended:bool}
impl LineStream for StrictLines {
    fn content_type(&self)->Option<&str>{self.inner.content_type()}
    fn next_line(&mut self)->Result<Option<String>,String>{
        if self.ended{return Ok(None);}
        loop {
            let Some(raw)=self.inner.next_line()? else {
                self.ended=true;
                // SSE dispatches only at an empty line. A terminal-looking
                // partial event at EOF must not authorize a successful turn.
                return if self.terminal&&!self.has_data{Ok(None)}else{Err("ai_invalid_response".into())};
            };
            let line=raw.strip_suffix('\n').unwrap_or(&raw);let line=line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                if !self.has_data{continue;}
                self.has_data=false;let data=std::mem::take(&mut self.buffer);
                let data=data.trim();if data.is_empty(){continue;}
                if data=="[DONE]"{self.terminal=true;}
                else {
                    let value:Value=serde_json::from_str(data).map_err(|_|"ai_invalid_response")?;
                    self.terminal|=validate_value(&value)?;
                }
                // The provider parser consumes one data string at a time;
                // join an SSE event's multiple data fields before passing it.
                return Ok(Some(format!("data: {data}\n")));
            }
            let data=if line=="data"{Some("")}else{line.strip_prefix("data:").map(|s|s.strip_prefix(' ').unwrap_or(s))};
            if let Some(data)=data {
                if self.buffer.len().saturating_add(data.len()).saturating_add(1)>8*1024*1024{return Err("review_ai_budget".into());}
                if self.has_data{self.buffer.push('\n');}self.buffer.push_str(data);self.has_data=true;
            }
        }
    }
}
fn validate_json(text:&str)->Result<(),String>{
    let value:Value=serde_json::from_str(text).map_err(|_|"ai_invalid_response")?;
    if matches!(value["status"].as_str(),Some("queued"|"in_progress")){return Err("ai_invalid_response".into());}
    validate_value(&value).map(|_|())
}
fn validate_value(value:&Value)->Result<bool,String>{
    if !value.is_object()||value.get("error").is_some_and(|v|!v.is_null())||value["type"]=="error"{return Err("ai_invalid_response".into());}
    if let Some(response)=value.get("response").filter(|v|v.is_object()){terminal(response)?;}
    terminal(value)
}
fn terminal(value:&Value)->Result<bool,String>{
    let mut done=matches!(value["type"].as_str(),Some("message_stop"|"response.completed"));
    if value["type"]=="response.failed"||matches!(value["status"].as_str(),Some("failed"|"cancelled")){return Err("ai_invalid_response".into());}
    if value["type"]=="response.incomplete"||value["status"]=="incomplete"{
        return Err(if value["incomplete_details"]["reason"]=="max_output_tokens"{"review_ai_budget"}else{"ai_invalid_response"}.into());
    }
    let reasons=value["choices"].as_array().into_iter().flatten().filter_map(|choice|choice["finish_reason"].as_str())
        .chain(value["candidates"].as_array().into_iter().flatten().filter_map(|candidate|candidate["finishReason"].as_str()))
        .chain(value["delta"]["stop_reason"].as_str()).chain(value["stop_reason"].as_str());
    for reason in reasons {match reason {
        "stop"|"STOP"|"end_turn"|"stop_sequence"=>done=true,
        "length"|"MAX_TOKENS"|"max_tokens"=>return Err("review_ai_budget".into()),
        "tool_calls"|"tool_use"=>return Err("review_ai_unsupported".into()),
        _=>return Err("ai_invalid_response".into()),
    }}
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Lines(std::collections::VecDeque<String>);
    impl LineStream for Lines {fn next_line(&mut self)->Result<Option<String>,String>{Ok(self.0.pop_front())}}
    fn lines(values:&[&str])->StrictLines{StrictLines{inner:Box::new(Lines(values.iter().flat_map(|s|[format!("{}\n",s.trim_end_matches('\n')),"\n".into()]).collect())),terminal:false,buffer:String::new(),has_data:false,ended:false}}
    #[test]
    fn a_delta_followed_by_bare_eof_is_not_success(){let mut stream=lines(&["data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n"]);assert!(stream.next_line().unwrap().is_some());assert_eq!(stream.next_line().unwrap_err(),"ai_invalid_response");}
    #[test]
    fn normal_finish_or_done_allows_eof_but_token_exhaustion_does_not(){
        for data in ["data: [DONE]","data: {\"choices\":[{\"finish_reason\":\"stop\"}]}","data: {\"type\":\"message_stop\"}","data: {\"candidates\":[{\"finishReason\":\"STOP\"}]}"]{let mut stream=lines(&[data]);assert!(stream.next_line().unwrap().is_some());assert!(stream.next_line().unwrap().is_none());}
        let mut stream=lines(&["data: {\"choices\":[{\"finish_reason\":\"length\"}]}"]);assert_eq!(stream.next_line().unwrap_err(),"review_ai_budget");
    }
    #[test]
    fn malformed_upstream_frames_are_not_silently_skipped(){let mut stream=lines(&["data: {broken}"]);assert_eq!(stream.next_line().unwrap_err(),"ai_invalid_response");}

    fn socket_response(mime:&str,body:&str)->(String,std::thread::JoinHandle<()>) {
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let response=format!("HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
        let host=std::thread::spawn(move||{
            listener.set_nonblocking(true).unwrap();let deadline=Instant::now()+Duration::from_secs(3);
            let mut socket=loop{match listener.accept(){Ok((socket,_))=>break socket,Err(error)if error.kind()==std::io::ErrorKind::WouldBlock=>{assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));},Err(error)=>panic!("fixture accept: {error}")}};
            socket.set_nonblocking(false).unwrap();socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request=Vec::new();let mut bytes=[0u8;4096];
            loop {
                let count=std::io::Read::read(&mut socket,&mut bytes).unwrap();assert_ne!(count,0);request.extend_from_slice(&bytes[..count]);assert!(request.len()<65536);
                if let Some(end)=request.windows(4).position(|v|v==b"\r\n\r\n") {
                    let head=std::str::from_utf8(&request[..end]).unwrap();let length=head.lines().find_map(|line|line.split_once(':').filter(|(name,_)|name.eq_ignore_ascii_case("content-length")).and_then(|(_,value)|value.trim().parse::<usize>().ok())).unwrap();
                    if request.len()>=end+4+length{break;}
                }
            }
            socket.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}/chat"),host)
    }
    fn socket_lines(mime:&str,body:&str)->Result<Box<dyn LineStream>,HttpError>{
        let(url,host)=socket_response(mime,body);let transport=crate::review_transport::ReviewTransport::new_owned(Arc::new(||false));
        let stream=StrictTransport(&transport).open_stream(&url,&vec![],"{}");
        host.join().unwrap();stream
    }
    #[test]
    fn an_unterminated_sse_terminal_frame_is_not_success_over_a_real_socket(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        for body in ["data: [DONE]", "data: [DONE]\n", "data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n"]{
            let mut stream=socket_lines("text/event-stream",body).unwrap();
            assert!(stream.next_line().is_err(),"an SSE frame without its blank line was accepted: {body:?}");
        }
    }
    #[test]
    fn json_fallback_rejects_explicit_unsuccessful_finish_over_a_real_socket(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        for body in [
            r#"{"choices":[{"message":{"content":"partial"},"finish_reason":"length"}]}"#,
            r#"{"content":[{"type":"text","text":"partial"}],"stop_reason":"max_tokens"}"#,
            r#"{"output_text":"partial","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}"#,
            r#"{"choices":[{"message":{"content":"partial"},"finish_reason":"tool_calls"}]}"#,
            r#"{"output_text":"partial","status":"in_progress"}"#,
        ]{
            assert!(socket_lines("application/json",body).is_err(),"an explicitly incomplete JSON reply was accepted: {body}");
        }
    }
    #[test]
    fn non_stream_json_rejects_an_explicit_failure_without_repeating_the_request(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let(url,host)=socket_response("application/json",r#"{"choices":[{"message":{"content":"partial"},"finish_reason":"length"}]}"#);
        let transport=crate::review_transport::ReviewTransport::new_owned(Arc::new(||false));
        assert!(matches!(StrictTransport(&transport).post_json(&url,&vec![],"{}"),Err(HttpError::Network(reason))if reason=="review_ai_budget"));
        host.join().unwrap();
    }
    #[test]
    fn normal_json_with_null_error_and_case_insensitive_mime_is_preserved(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let body="{\n\"output_text\":\"中文😀\",\"status\":\"completed\",\"error\":null\n}";
        let mut stream=socket_lines("Application/JSON; Charset=UTF-8",body).unwrap();assert_eq!(stream.content_type(),Some("application/json"));
        let mut received=String::new();while let Some(line)=stream.next_line().unwrap(){received.push_str(&line);}assert_eq!(received,body);
    }
    #[test]
    fn complete_crlf_events_join_data_fields_before_provider_parsing(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let mut stream=socket_lines("text/event-stream","data: {\"choices\":[\r\ndata: {\"delta\":{\"content\":\"中文😀\"}}]}\r\n\r\ndata: {\"choices\":[{\"finish_reason\":\"stop\"}]}\r\n\r\n").unwrap();
        let first=stream.next_line().unwrap().unwrap();let frame:Value=serde_json::from_str(first.trim().strip_prefix("data:").unwrap()).unwrap();
        assert_eq!(frame["choices"][0]["delta"]["content"],"中文😀");assert!(stream.next_line().unwrap().is_some());assert!(stream.next_line().unwrap().is_none());assert!(stream.next_line().unwrap().is_none());
    }
    fn socket_chat(mime:&str,body:&str,stream:bool)->Result<ai_providers::ChatStream,ai_providers::AiError>{
        let(url,host)=socket_response(mime,body);let transport=crate::review_transport::ReviewTransport::new_owned(Arc::new(||false));
        let args=ai_providers::RouteArgs{base_url:url,api_key:"fixture".into(),model:json!("fixture"),messages:vec![json!({"role":"user","content":"hi"})],temperature:json!(0.5),stream,endpoint_mode:"full_url".into(),custom_headers:Value::Null};
        let result=ai_providers::chat_openai(&args,&StrictTransport(&transport));host.join().unwrap();result
    }
    #[test]
    fn real_provider_json_path_keeps_budget_errors_and_one_normal_reply(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        for stream in [false,true] {
            let error=socket_chat("application/json",r#"{"choices":[{"message":{"content":"partial"},"finish_reason":"length"}]}"#,stream).err().unwrap();
            assert_eq!(provider_error(error,false).code,"review_ai_budget");
        }
        let mut events=socket_chat("Application/JSON",r#"{"choices":[{"message":{"content":"完整答复"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}"#,true).unwrap();
        assert!(matches!(events.next().unwrap().unwrap(),ai_providers::ChatEvent::Delta(value)if value=="完整答复"));
        assert!(matches!(events.next().unwrap().unwrap(),ai_providers::ChatEvent::Usage(_)));assert!(events.next().is_none());
    }
    #[test]
    fn cancellation_retires_a_slow_downstream_writer_without_waiting_for_socket_close(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let dir=std::env::temp_dir().join(format!("readmd-chat-backpressure-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();std::fs::write(dir.join("assets/index.html"),"<head><title>fixture</title></head>").unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&dir.join("data"),&dir,&dir.join("assets"))).unwrap());
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let client=TcpStream::connect(listener.local_addr().unwrap()).unwrap();let(mut writer,_)=listener.accept().unwrap();
        writer.set_write_timeout(Some(Duration::from_millis(100))).unwrap();let bytes=[b'x';65536];let mut sent=0;
        // Fill real TCP buffers while the client deliberately does not read.
        loop {match writer.write(&bytes){Ok(n)=>{sent+=n;assert!(sent<64*1024*1024);},Err(error)if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut)=>break,Err(error)=>panic!("fixture fill: {error}")}}
        writer.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let id=format!("chat-backpressure-{}",uuid::Uuid::new_v4());
        let body=json!({"base_url":"http://127.0.0.1:1/v1","api_key":"fixture","model":"fixture","messages":[{"role":"user","content":"hi"}],"task_id":id});
        let req=Request{method:"POST".into(),path:"/api/ai/chat".into(),query:HashMap::new(),headers:HashMap::new(),body:serde_json::to_vec(&body).unwrap()};
        let(finished_tx,finished_rx)=std::sync::mpsc::channel();let worker=std::thread::spawn(move||{serve(&app,&req,&mut writer);finished_tx.send(()).unwrap();});
        let deadline=std::time::Instant::now()+Duration::from_secs(3);
        while crate::cancel::cancel(&id)==crate::cancel::CancelState::Unknown{assert!(std::time::Instant::now()<deadline);std::thread::sleep(Duration::from_millis(5));}
        let retired_before_close=finished_rx.recv_timeout(Duration::from_millis(200)).is_ok();
        drop(client);worker.join().unwrap();std::fs::remove_dir_all(dir).unwrap();
        assert!(retired_before_close,"a cancelled SSE write stayed blocked on a downstream socket for more than 200ms");
    }
}
