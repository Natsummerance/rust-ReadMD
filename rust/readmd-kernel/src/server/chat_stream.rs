//! Accept-negotiated chat. Provider deltas stay deltas; JSON/MCP stays JSON.
use super::*;
use crate::ai_providers::{Headers, HttpError, LineStream, Transport};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

pub(super) struct Peer {
    pub(super) lost: Arc<AtomicBool>, finished: Arc<AtomicBool>, worker: Option<std::thread::JoinHandle<()>>,
    restore_blocking: Option<TcpStream>,
}
impl Peer {
    pub(super) fn watch(socket: &TcpStream) -> ApiResult<Self> {
        Self::watch_mode(socket,false)
    }
    pub(super) fn watch_stream(socket: &TcpStream) -> ApiResult<Self> {
        Self::watch_mode(socket,true)
    }
    fn watch_mode(socket: &TcpStream,nonblocking:bool) -> ApiResult<Self> {
        let socket=socket.try_clone().map_err(|_|ApiError::new(409,"cancelled"))?;
        let restore_blocking=if nonblocking {Some(socket.try_clone().map_err(|_|ApiError::new(409,"cancelled"))?)} else {None};
        socket.set_nonblocking(nonblocking).map_err(|_|ApiError::new(409,"cancelled"))?;
        if !nonblocking {
            socket.set_read_timeout(Some(Duration::from_millis(50))).map_err(|_|ApiError::new(409,"cancelled"))?;
        }
        let lost=Arc::new(AtomicBool::new(false));let finished=Arc::new(AtomicBool::new(false));
        let lost_flag=lost.clone();let stop=finished.clone();
        let worker=std::thread::Builder::new().name("readmd-chat-peer".into()).spawn(move||{
            let mut byte=[0u8;1];
            while !stop.load(Ordering::Acquire) {
                match socket.peek(&mut byte) {
                    Ok(0) => { lost_flag.store(true,Ordering::Release);break; },
                    Ok(_) => std::thread::park_timeout(Duration::from_millis(10)),
                    Err(error) if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut|std::io::ErrorKind::Interrupted) => {
                        if nonblocking {std::thread::park_timeout(Duration::from_millis(5));}
                    },
                    Err(_) => { lost_flag.store(true,Ordering::Release);break; },
                }
            }
        });
        match worker {
            Ok(worker)=>Ok(Self{lost,finished,worker:Some(worker),restore_blocking}),
            Err(_)=>{
                if let Some(socket)=restore_blocking {let _=socket.set_nonblocking(false);}
                Err(ApiError::new(503,"review_ai_budget"))
            },
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.finished.store(true,Ordering::Release);
        if let Some(worker)=self.worker.take(){worker.thread().unpark();let _=worker.join();}
        if let Some(socket)=self.restore_blocking.take(){let _=socket.set_nonblocking(false);}
    }
}

pub(super) fn write_event_bytes(writer:&mut TcpStream,mut bytes:&[u8],cancelled:&dyn Fn()->bool,budget:Duration)->ApiResult<()> {
    let deadline=Instant::now()+budget;
    // Stream peers share this mode with their watcher and restore blocking on
    // drop. Nonblocking attempts keep cancellation checks independent of
    // socket timeout granularity.
    writer.set_nonblocking(true).map_err(|_|ApiError::new(409,"cancelled"))?;
    while !bytes.is_empty() {
        if cancelled()||Instant::now()>=deadline{return Err(ApiError::new(409,"cancelled"));}
        match writer.write(bytes) {
            Ok(0)=>return Err(ApiError::new(409,"cancelled")),
            Ok(count)=>bytes=&bytes[count..],
            Err(error)if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut|std::io::ErrorKind::Interrupted)=>{
                #[cfg(test)]
                if error.kind()!=std::io::ErrorKind::Interrupted { tests::note_blocked_write(); }
                std::thread::park_timeout(Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())));
            },
            Err(_)=>return Err(ApiError::new(409,"cancelled")),
        }
    }
    Ok(())
}

pub(super) fn serve(app:&Arc<App>,req:&Request,writer:&mut TcpStream) {
    let peer=match Peer::watch_stream(writer) {Ok(peer)=>peer,Err(error)=>{let _=write_response(writer,&error_response(error),false,false);return;}};
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
    let (task_id,task)=register_chat_task(&payload)?;
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
    let persisted=commit_chat_turn(app,&payload,&content,&ai_providers::py_str(&route.args.model),&task,cancelled.as_ref())?;
    if let Some(usage)=usage {emit(json!({"type":"usage","task_id":task_id,"usage":usage}),cancelled.as_ref())?;}
    emit(json!({"type":"done","task_id":task_id,"ok":true,"persisted":persisted}),cancelled.as_ref())
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
    thread_local! {
        static BLOCKED_WRITE: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> = const { std::cell::RefCell::new(None) };
    }
    pub(super) fn note_blocked_write() {
        BLOCKED_WRITE.with(|slot| {
            if let Some(sender)=slot.borrow_mut().take() { let _=sender.send(()); }
        });
    }
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
    fn pending_sse_response()->(String,std::sync::mpsc::Sender<()>,std::thread::JoinHandle<()>) {
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let(stop_tx,stop_rx)=std::sync::mpsc::channel();
        let host=std::thread::spawn(move||{
            let stopped=||!matches!(stop_rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty));
            let deadline=Instant::now()+Duration::from_secs(3);listener.set_nonblocking(true).unwrap();
            let mut socket=loop {
                if stopped(){return;}
                match listener.accept(){
                    Ok((socket,_))=>break socket,
                    Err(error)if error.kind()==std::io::ErrorKind::WouldBlock=>{
                        assert!(Instant::now()<deadline,"pending provider was never opened");
                        std::thread::sleep(Duration::from_millis(5));
                    },
                    Err(error)=>panic!("pending provider accept: {error}"),
                }
            };
            socket.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
            socket.set_write_timeout(Some(Duration::from_millis(50))).unwrap();
            let mut request=Vec::new();let mut bytes=[0u8;4096];
            loop {
                if stopped(){return;}
                assert!(Instant::now()<deadline,"pending provider request was incomplete");
                match std::io::Read::read(&mut socket,&mut bytes){
                    Ok(0)=>return,
                    Ok(count)=>request.extend_from_slice(&bytes[..count]),
                    Err(error)if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut|std::io::ErrorKind::Interrupted)=>continue,
                    Err(_)=>return,
                }
                assert!(request.len()<65536);
                if let Some(end)=request.windows(4).position(|value|value==b"\r\n\r\n"){
                    let head=std::str::from_utf8(&request[..end]).unwrap();
                    let length=head.lines().find_map(|line|line.split_once(':').filter(|(name,_)|name.eq_ignore_ascii_case("content-length")).and_then(|(_,value)|value.trim().parse::<usize>().ok())).unwrap();
                    if request.len()>=end+4+length{break;}
                }
            }
            let send=|socket:&mut TcpStream,mut bytes:&[u8]|{
                while !bytes.is_empty(){
                    if stopped(){return false;}
                    assert!(Instant::now()<deadline,"pending provider write did not settle");
                    match socket.write(bytes){
                        Ok(0)=>return false,
                        Ok(count)=>bytes=&bytes[count..],
                        Err(error)if matches!(error.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut|std::io::ErrorKind::Interrupted)=>(),
                        Err(_)=>return false,
                    }
                }
                true
            };
            if !send(&mut socket,b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"){return;}
            let delta=json!({"choices":[{"delta":{"content":"段".repeat(25_000)}}]});
            let frame=format!("data: {delta}\n\n");
            for _ in 0..36 {if !send(&mut socket,frame.as_bytes()){return;}}
            // No terminal frame or EOF may authorize commit while cancellation
            // is being measured. The test releases this endpoint only afterward.
            let _=stop_rx.recv_timeout(Duration::from_secs(3));
        });
        (format!("http://{address}/chat"),stop_tx,host)
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
    fn a_disconnect_after_the_sse_commit_gate_keeps_the_complete_turn(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let dir=tempfile::tempdir().unwrap();let assets=dir.path().join("assets");std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("index.html"),"<head><title>fixture</title></head>").unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&dir.path().join("data"),dir.path(),&assets)).unwrap());
        let(url,host)=socket_response("text/event-stream","data: {\"choices\":[{\"delta\":{\"content\":\"complete reply\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"total_tokens\":1}}\n\ndata: [DONE]\n\n");
        let id="sse-post-commit-disconnect";let session="owned-sse-post-commit";
        let body=json!({"base_url":url,"endpoint_mode":"full_url","api_key":"fixture","model":"fixture","stream":true,"task_id":id,"session":session,"messages":[{"role":"user","content":"question"}]});
        let req=Request{method:"POST".into(),path:"/api/ai/chat".into(),query:HashMap::new(),headers:HashMap::new(),body:serde_json::to_vec(&body).unwrap()};
        let lost=Arc::new(AtomicBool::new(false));let mut committed=false;
        let result=run(&app,&req,lost.clone(),&mut |event,cancelled|{
            if event["type"]=="usage" {
                assert_eq!(crate::cancel::cancel(id),crate::cancel::CancelState::Finished);
                assert_eq!(app.store.chat_history(session,10).unwrap().len(),2);committed=true;
                lost.store(true,Ordering::Release);assert!(cancelled());return Err(ApiError::new(409,"cancelled"));
            }
            Ok(())
        });
        host.join().unwrap();assert!(committed);assert_eq!(result.err().unwrap().code,"cancelled");
        let history=app.store.chat_history(session,10).unwrap();assert_eq!(history.len(),2);assert_eq!(history[1]["content"],"complete reply");
        assert_eq!(crate::cancel::cancel(id),crate::cancel::CancelState::Unknown);
    }
    #[test]
    fn stream_peer_restores_http_mode_and_json_requests_keep_alive(){
        use std::io::{BufRead,Read};
        let dir=tempfile::tempdir().unwrap();let assets=dir.path().join("assets");std::fs::create_dir(&assets).unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&dir.path().join("data"),dir.path(),&assets)).unwrap());
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let mut client=TcpStream::connect(address).unwrap();client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let(socket,_)=listener.accept().unwrap();let(ready_tx,ready_rx)=std::sync::mpsc::channel();
        let host=std::thread::spawn(move||{
            let peer=Peer::watch_stream(&socket).unwrap();drop(peer);
            ready_tx.send(()).unwrap();handle_connection(socket,app);
        });
        ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let mut reader=std::io::BufReader::new(client.try_clone().unwrap());
        let response=|reader:&mut std::io::BufReader<TcpStream>|{
            let mut headers=String::new();let mut length=None;
            loop{
                let mut line=String::new();assert_ne!(reader.read_line(&mut line).unwrap(),0);
                if let Some((name,value))=line.split_once(':'){
                    if name.eq_ignore_ascii_case("content-length"){length=Some(value.trim().parse::<usize>().unwrap());}
                }
                headers.push_str(&line);if line=="\r\n"{break;}
            }
            let length=length.unwrap();assert!(length<65536);let mut body=vec![0u8;length];reader.read_exact(&mut body).unwrap();headers
        };
        // The guarded JSON route creates its ordinary Peer even when auth
        // rejects this synthetic request; no provider or model is contacted.
        client.write_all(format!("POST /api/ai/chat HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\n{{}}").as_bytes()).unwrap();
        assert!(response(&mut reader).contains("Connection: keep-alive\r\n"));
        client.write_all(format!("GET /api/version HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
        assert!(response(&mut reader).contains("Connection: close\r\n"));host.join().unwrap();
    }
    #[test]
    fn cancellation_retires_a_slow_downstream_writer_without_waiting_for_socket_close(){
        let _serial=crate::review_transport::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let dir=std::env::temp_dir().join(format!("readmd-chat-backpressure-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();std::fs::write(dir.join("assets/index.html"),"<head><title>fixture</title></head>").unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&dir.join("data"),&dir,&dir.join("assets"))).unwrap());
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let client=TcpStream::connect(listener.local_addr().unwrap()).unwrap();let(mut writer,_)=listener.accept().unwrap();
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // Bound the fixture's buffers before traffic starts. A failed
            // large prefill did not prove that a later small start frame
            // would stay blocked while TCP acknowledgements reopened space.
            let capacity=4096i32;
            for (socket,option) in [(&client,libc::SO_RCVBUF),(&writer,libc::SO_SNDBUF)] {
                assert_eq!(unsafe {libc::setsockopt(socket.as_raw_fd(),libc::SOL_SOCKET,option,
                    (&capacity as *const i32).cast(),std::mem::size_of_val(&capacity) as libc::socklen_t)},0);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;
            // Test-only Winsock calls avoid adding a networking feature or
            // dependency to the application. SOCKET is an unsigned pointer.
            #[link(name="ws2_32")]
            unsafe extern "system" {
                fn setsockopt(socket:usize,level:i32,option:i32,value:*const i8,length:i32)->i32;
                fn getsockopt(socket:usize,level:i32,option:i32,value:*mut i8,length:*mut i32)->i32;
                fn WSAGetLastError()->i32;
            }
            const SOL_SOCKET:i32=0xffff;
            const SO_SNDBUF:i32=0x1001;
            const SO_RCVBUF:i32=0x1002;
            let capacity=4096i32;
            for (socket,option) in [(&client,SO_RCVBUF),(&writer,SO_SNDBUF)] {
                let socket=socket.as_raw_socket() as usize;
                assert_eq!(unsafe {setsockopt(socket,SOL_SOCKET,option,
                    (&capacity as *const i32).cast(),std::mem::size_of_val(&capacity) as i32)},0,
                    "Winsock buffer setup failed: {}",unsafe {WSAGetLastError()});
                let mut actual=0i32;let mut length=std::mem::size_of_val(&actual) as i32;
                assert_eq!(unsafe {getsockopt(socket,SOL_SOCKET,option,
                    (&mut actual as *mut i32).cast(),&mut length)},0,
                    "Winsock buffer readback failed: {}",unsafe {WSAGetLastError()});
                assert_eq!(length,std::mem::size_of_val(&actual) as i32);
                assert!(actual>0&&actual<=64*1024,"fixture socket buffer remained too large: {actual}");
            }
        }
        // The real SSE endpoint stays in progress even if Windows accepts an
        // entire first write. A blocked done frame after commit must never
        // masquerade as the cancellable downstream writer under test.
        let(url,stop_provider,provider)=pending_sse_response();
        let id=format!("chat-backpressure-{}",uuid::Uuid::new_v4());
        let session=format!("chat-backpressure-history-{}",uuid::Uuid::new_v4());
        let body=json!({"base_url":url,"endpoint_mode":"full_url","api_key":"fixture","model":"fixture","messages":[{"role":"user","content":"hi"}],"task_id":id,"session":session});
        let req=Request{method:"POST".into(),path:"/api/ai/chat".into(),query:HashMap::new(),headers:HashMap::new(),body:serde_json::to_vec(&body).unwrap()};
        let(blocked_tx,blocked_rx)=std::sync::mpsc::channel();
        let(finished_tx,finished_rx)=std::sync::mpsc::channel();let worker_app=app.clone();
        let worker=std::thread::spawn(move||{
            BLOCKED_WRITE.with(|slot|*slot.borrow_mut()=Some(blocked_tx));
            serve(&worker_app,&req,&mut writer);finished_tx.send(()).unwrap();
        });
        let blocked=blocked_rx.recv_timeout(Duration::from_secs(3)).is_ok();
        let cancellation=crate::cancel::cancel(&id);
        let retired_before_close=finished_rx.recv_timeout(Duration::from_millis(200)).is_ok();
        drop(client);worker.join().unwrap();let _=stop_provider.send(());provider.join().unwrap();
        let history=app.store.chat_history(&session,10).unwrap();drop(app);std::fs::remove_dir_all(dir).unwrap();
        assert!(blocked,"the fixture never observed an actual downstream write timeout; cancellation={cancellation:?}");
        assert_eq!(cancellation,crate::cancel::CancelState::Cancelling);
        assert!(history.is_empty(),"a cancelled downstream write persisted a chat turn");
        assert_eq!(crate::cancel::cancel(&id),crate::cancel::CancelState::Unknown);
        assert!(retired_before_close,"a cancelled SSE write stayed blocked on a downstream socket for more than 200ms");
    }
}
