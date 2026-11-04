//! Bounded, cancellable waiting for native review tool requests.
//! Network threads never receive document/store handles and cannot commit edits.
use crate::ai_providers::{Headers, HttpError, LineStream, Transport};
mod network;
pub(crate) use network::NetworkOwner;
use std::{
    io::{BufRead, BufReader, Read},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) struct ReviewTransport<'a> {
    cancelled: CancelCheck<'a>,
    deadline: Instant,
    request_timeout: Duration,
    response_limit: usize,
}
enum CancelCheck<'a> {
    Borrowed(&'a dyn Fn() -> bool),
    Owned(Arc<dyn Fn() -> bool + Send + Sync>),
}
impl<'a> ReviewTransport<'a> {
    pub(crate) fn new(cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            cancelled: CancelCheck::Borrowed(cancelled),
            deadline: Instant::now() + Duration::from_secs(180),
            request_timeout: Duration::from_secs(90), response_limit: RESPONSE_LIMIT,
        }
    }
    fn stopped(&self) -> bool {
        let stopped = match &self.cancelled { CancelCheck::Borrowed(check) => check(), CancelCheck::Owned(check) => check() };
        stopped || Instant::now() >= self.deadline
    }
}
impl ReviewTransport<'static> {
    pub(crate) fn new_owned(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { cancelled: CancelCheck::Owned(cancelled), deadline: Instant::now() + Duration::from_secs(7200), request_timeout: Duration::from_secs(90), response_limit: RESPONSE_LIMIT }
    }
    pub(crate) fn bounded_owned(cancelled:Arc<dyn Fn()->bool+Send+Sync>,timeout:Duration,deadline:Instant,response_limit:usize)->Self {
        Self { cancelled:CancelCheck::Owned(cancelled), deadline:deadline.min(Instant::now()+timeout), request_timeout:timeout.min(Duration::from_secs(90)), response_limit:response_limit.min(RESPONSE_LIMIT) }
    }
}

struct NativeStream {
    receive: mpsc::Receiver<Result<Option<String>, String>>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    stopped: Arc<AtomicBool>,
    content_type: String,
    deadline: Instant,
    network: NetworkOwner,
}
impl NativeStream {
    fn stop(&self) { self.stopped.store(true, Ordering::Release); self.network.stop(); }
}
impl Drop for NativeStream { fn drop(&mut self) { self.stop(); } }
impl LineStream for NativeStream {
    fn content_type(&self) -> Option<&str> { Some(&self.content_type) }
    fn next_line(&mut self) -> Result<Option<String>, String> {
        loop {
            if (self.cancelled)() { self.stop(); return Err("cancelled".into()); }
            if Instant::now() >= self.deadline { self.stop(); return Err("ai_network_error".into()); }
            match self.receive.recv_timeout(Duration::from_millis(40)) {
                Ok(value) => {
                    if (self.cancelled)() { self.stop(); return Err("cancelled".into()); }
                    if !matches!(value, Ok(Some(_))) { self.stop(); }
                    return value;
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(_) => { self.stop(); return Err("ai_network_error".into()); },
            }
        }
    }
}
impl Transport for ReviewTransport<'_> {
    fn post_json(&self, url: &str, headers: &Headers, body: &str) -> Result<String, HttpError> {
        if self.stopped() {
            return Err(HttpError::Network("cancelled".into()));
        }
        if body.len() > 32 * 1024 * 1024 {
            return Err(HttpError::Network("review_ai_budget".into()));
        }
        ACTIVE
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < 2).then_some(n + 1)
            })
            .map_err(|_| HttpError::Network("review_ai_budget".into()))?;
        let permit = Permit;
        let url = url.to_owned();
        let headers = headers.clone();
        let body = body.to_owned();
        let (send, receive) = mpsc::sync_channel(1);
        let network = match &self.cancelled {CancelCheck::Owned(check)=>NetworkOwner::with_deadline(check.clone(),self.deadline.min(Instant::now()+self.request_timeout)),_=>NetworkOwner::new()};
        let resolver = network.resolver();
        let timeout=self.request_timeout;let response_limit=self.response_limit;
        std::thread::Builder::new()
            .name("readmd-review-request".into())
            .spawn(move || {
                let _permit = permit;
                let result = (|| {
                    let agent = ureq::AgentBuilder::new()
                        .resolver(resolver)
                        .timeout(timeout)
                        .timeout_connect(timeout.min(Duration::from_secs(10)))
                        .timeout_read(timeout)
                        .timeout_write(timeout.min(Duration::from_secs(10)))
                        .redirects(0)
                        .build();
                    let mut request = agent.post(&url);
                    for (name, value) in &headers {
                        request = request.set(name, value);
                    }
                    let response = match request.send_string(&body) {
                        Ok(response) => response,
                        Err(ureq::Error::Status(code, _)) => {
                            return Err(HttpError::Status {
                                code,
                                body: String::new(),
                            })
                        }
                        Err(_) => return Err(HttpError::Network("ai_network_error".into())),
                    };
                    let mut data = Vec::new();
                    response
                        .into_reader()
                        .take((response_limit + 1) as u64)
                        .read_to_end(&mut data)
                        .map_err(|_| HttpError::Network("ai_network_error".into()))?;
                    if data.len() > response_limit {
                        return Err(HttpError::Network("review_ai_budget".into()));
                    }
                    String::from_utf8(data)
                        .map_err(|_| HttpError::Network("ai_invalid_response".into()))
                })();
                let _ = send.send(result);
            })
            .map_err(|_| HttpError::Network("review_ai_budget".into()))?;
        loop {
            if self.stopped() {
                return Err(HttpError::Network("cancelled".into()));
            }
            match receive.recv_timeout(Duration::from_millis(40)) {
                Ok(value) => {
                    if self.stopped() {
                        return Err(HttpError::Network("cancelled".into()));
                    }
                    return value;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(HttpError::Network("ai_network_error".into())),
            }
        }
    }
    fn open_stream(&self, url: &str, headers: &Headers, body: &str) -> Result<Box<dyn LineStream>, HttpError> {
        let CancelCheck::Owned(check) = &self.cancelled else { return Err(HttpError::Network("review_ai_unsupported".into())); };
        if self.stopped() { return Err(HttpError::Network("cancelled".into())); }
        if body.len() > 32 * 1024 * 1024 { return Err(HttpError::Network("review_ai_budget".into())); }
        ACTIVE.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|n|(n<2).then_some(n+1))
            .map_err(|_|HttpError::Network("review_ai_budget".into()))?;
        let permit=Permit;
        let (opened_tx,opened_rx)=mpsc::sync_channel(1);
        let (send,receive)=mpsc::sync_channel(4);
        let stopped=Arc::new(AtomicBool::new(false)); let worker_stopped=stopped.clone();
        let network=NetworkOwner::with_deadline(check.clone(),self.deadline.min(Instant::now()+self.request_timeout)); let resolver=network.resolver();
        let worker_cancelled=check.clone();let worker_deadline=self.deadline.min(Instant::now()+self.request_timeout);
        let timeout=self.request_timeout;let response_limit=self.response_limit;
        let url=url.to_owned();let headers=headers.clone();let body=body.to_owned();
        std::thread::Builder::new().name("readmd-task-stream".into()).spawn(move||{
            let _permit=permit;
            let should_stop=||worker_stopped.load(Ordering::Acquire)||worker_cancelled()||Instant::now()>=worker_deadline;
            let agent=ureq::AgentBuilder::new().resolver(resolver).timeout(timeout).timeout_connect(timeout.min(Duration::from_secs(10)))
                .timeout_read(timeout).timeout_write(timeout.min(Duration::from_secs(10))).redirects(0).build();
            let mut request=agent.post(&url);for(name,value)in &headers{request=request.set(name,value);}
            let response=match request.send_string(&body){
                Ok(value)=>value,
                Err(ureq::Error::Status(code,_))=>{let _=opened_tx.send(Err(HttpError::Status{code,body:String::new()}));return;},
                Err(_)=>{let _=opened_tx.send(Err(HttpError::Network("ai_network_error".into())));return;},
            };
            let content_type=response.header("Content-Type").unwrap_or("").to_string();
            if opened_tx.send(Ok(content_type)).is_err(){return;}
            let mut reader=BufReader::new(response.into_reader().take((response_limit+1)as u64));let mut total=0usize;
            loop{
                if should_stop(){return;}
                let mut bytes=Vec::new();
                match reader.read_until(b'\n',&mut bytes){
                    Ok(0)=>{send_line(&send,&should_stop,Ok(None));return;},
                    Ok(n)=>{total=total.saturating_add(n);if total>response_limit{send_line(&send,&should_stop,Err("review_ai_budget".into()));return;}},
                    Err(_)=>{send_line(&send,&should_stop,Err("ai_network_error".into()));return;},
                }
                let text=match String::from_utf8(bytes){Ok(value)=>value,Err(_)=>{send_line(&send,&should_stop,Err("ai_invalid_response".into()));return;}};
                if !send_line(&send,&should_stop,Ok(Some(text))){return;}
            }
        }).map_err(|_|HttpError::Network("review_ai_budget".into()))?;
        loop{
            if self.stopped(){stopped.store(true,Ordering::Release);network.stop();return Err(HttpError::Network("cancelled".into()));}
            match opened_rx.recv_timeout(Duration::from_millis(40)){
                Ok(Ok(content_type))=>return Ok(Box::new(NativeStream{receive,cancelled:check.clone(),stopped,content_type,deadline:self.deadline.min(Instant::now()+Duration::from_secs(180)),network})),
                Ok(Err(error))=>return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout)=>{},
                Err(_)=>return Err(HttpError::Network("ai_network_error".into())),
            }
        }
    }
}

fn send_line(send:&mpsc::SyncSender<Result<Option<String>,String>>, stopped:&dyn Fn()->bool, mut value:Result<Option<String>,String>)->bool {
    loop {
        if stopped() { return false; }
        match send.try_send(value) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
            Err(mpsc::TrySendError::Full(pending)) => { value=pending; std::thread::sleep(Duration::from_millis(10)); },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{atomic::AtomicBool, Arc, Mutex},
    };
    use super::TEST_SERIAL as SERIAL;
    fn accept(listener: TcpListener) -> TcpStream {
        listener.set_nonblocking(true).unwrap(); let deadline=Instant::now()+Duration::from_secs(5);
        loop { match listener.accept() {
            Ok((socket,_)) => {socket.set_nonblocking(false).unwrap();socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();return socket;},
            Err(error) if error.kind()==std::io::ErrorKind::WouldBlock => {assert!(Instant::now()<deadline,"owned fixture accept timed out");std::thread::sleep(Duration::from_millis(5));},
            Err(error) => panic!("owned fixture accept: {error}"),
        }}
    }
    fn request(socket:&mut TcpStream)->Vec<u8> {
        let mut bytes=Vec::new();let mut buffer=[0u8;4096];
        loop {
            let n=socket.read(&mut buffer).unwrap();assert_ne!(n,0);bytes.extend_from_slice(&buffer[..n]);assert!(bytes.len()<65536);
            if let Some(end)=bytes.windows(4).position(|b|b==b"\r\n\r\n") {
                let header=std::str::from_utf8(&bytes[..end]).unwrap();
                let length=header.lines().find_map(|line|line.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).and_then(|(_,v)|v.trim().parse::<usize>().ok())).unwrap();
                if bytes.len()>=end+4+length {return bytes;}
            }
        }
    }
    fn closed(socket:&mut TcpStream) { let mut byte=[0u8;1];assert_eq!(socket.read(&mut byte).unwrap(),0,"owned request socket must close"); }
    fn released() {
        let deadline=Instant::now()+Duration::from_secs(2);
        while ACTIVE.load(Ordering::SeqCst)!=0 {assert!(Instant::now()<deadline,"cancelled network worker retained its request slot");std::thread::sleep(Duration::from_millis(5));}
    }
    #[test]
    fn cancellation_closes_stalled_headers_and_releases_the_request_slot() {
        let _serial=SERIAL.lock().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/tool", listener.local_addr().unwrap());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (closed_tx, closed_rx) = mpsc::channel();
        let host = std::thread::spawn(move || {
            let mut socket = accept(listener);request(&mut socket);
            entered_tx.send(()).unwrap();
            closed(&mut socket);closed_tx.send(()).unwrap();
        });
        let flag = Arc::new(AtomicBool::new(false));
        let flip = flag.clone();
        let sent=Arc::new(Mutex::new(None));let sent_at=sent.clone();
        let cancel = std::thread::spawn(move || {
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            *sent_at.lock().unwrap()=Some(Instant::now());
            flip.store(true, Ordering::SeqCst);
        });
        let is_cancelled = || flag.load(Ordering::SeqCst);
        let transport = ReviewTransport::new(&is_cancelled);
        let result = transport.post_json(&url, &vec![], "{}");
        assert!(matches!(result, Err(HttpError::Network(reason)) if reason=="cancelled"));
        assert!(sent.lock().unwrap().unwrap().elapsed() < Duration::from_millis(200));
        closed_rx.recv_timeout(Duration::from_secs(2)).unwrap();released();
        cancel.join().unwrap();
        host.join().unwrap();
    }
    #[test]
    fn a_stalled_stream_stops_without_waiting_for_the_consumer_to_drop() {
        let _serial=SERIAL.lock().unwrap();let listener=TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}/stream",listener.local_addr().unwrap());
        let (closed_tx,closed_rx)=mpsc::channel();
        let host=std::thread::spawn(move||{let mut socket=accept(listener);request(&mut socket);socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"first\":true}\n\n").unwrap();closed(&mut socket);closed_tx.send(()).unwrap();});
        let flag=Arc::new(AtomicBool::new(false));let check=flag.clone();let transport=ReviewTransport::new_owned(Arc::new(move||check.load(Ordering::Acquire)));
        let mut stream=transport.open_stream(&url,&vec![],"{}").unwrap();assert_eq!(stream.next_line().unwrap().unwrap(),"data: {\"first\":true}\n");
        flag.store(true,Ordering::Release);let at=Instant::now();assert_eq!(stream.next_line().unwrap_err(),"cancelled");assert!(at.elapsed()<Duration::from_millis(200));
        closed_rx.recv_timeout(Duration::from_secs(2)).unwrap();released();drop(stream);host.join().unwrap();
    }
    #[test]
    fn dropping_a_full_stream_queue_does_not_retain_the_network_worker() {
        let _serial=SERIAL.lock().unwrap();let listener=TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}/stream",listener.local_addr().unwrap());
        let (written_tx,written_rx)=mpsc::channel();let (closed_tx,closed_rx)=mpsc::channel();
        let host=std::thread::spawn(move||{let mut socket=accept(listener);request(&mut socket);socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();for _ in 0..32 {socket.write_all(b"data: {}\n\n").unwrap();}written_tx.send(()).unwrap();closed(&mut socket);closed_tx.send(()).unwrap();});
        let transport=ReviewTransport::new_owned(Arc::new(||false));let stream=transport.open_stream(&url,&vec![],"{}").unwrap();
        written_rx.recv_timeout(Duration::from_secs(3)).unwrap();std::thread::sleep(Duration::from_millis(80));let at=Instant::now();drop(stream);assert!(at.elapsed()<Duration::from_millis(200));
        closed_rx.recv_timeout(Duration::from_secs(2)).unwrap();released();host.join().unwrap();
    }
    #[test]
    fn cancellation_stops_a_full_queue_even_while_the_consumer_is_not_polling() {
        let _serial=SERIAL.lock().unwrap();let listener=TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}/stream",listener.local_addr().unwrap());
        let (written_tx,written_rx)=mpsc::channel();let (closed_tx,closed_rx)=mpsc::channel();
        let host=std::thread::spawn(move||{let mut socket=accept(listener);request(&mut socket);socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();for _ in 0..32 {socket.write_all(b"data: {}\n\n").unwrap();}written_tx.send(()).unwrap();closed(&mut socket);let _=closed_tx.send(());});
        let flag=Arc::new(AtomicBool::new(false));let check=flag.clone();let transport=ReviewTransport::new_owned(Arc::new(move||check.load(Ordering::Acquire)));
        let stream=transport.open_stream(&url,&vec![],"{}").unwrap();written_rx.recv_timeout(Duration::from_secs(3)).unwrap();std::thread::sleep(Duration::from_millis(80));
        let deadline=Instant::now()+Duration::from_secs(2);flag.store(true,Ordering::Release);
        let closed_before_drop=closed_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).is_ok();
        // Socket EOF and the request worker's Permit drop are independent
        // observations. Both must finish within the original two-second
        // budget, while the consumer is still deliberately not polling.
        while ACTIVE.load(Ordering::SeqCst)!=0&&Instant::now()<deadline {std::thread::sleep(Duration::from_millis(5));}
        let active_before_drop=ACTIVE.load(Ordering::SeqCst);
        // Always retire the explicit fixture before asserting the contract.
        drop(stream);released();host.join().unwrap();
        assert!(closed_before_drop,"the owned upstream outlived cancellation while its consumer was blocked");
        assert_eq!(active_before_drop,0,"the full queue retained a cancelled request slot");
    }
    #[test]
    fn normal_json_retains_the_original_host_header_and_exact_unicode_body() {
        let _serial=SERIAL.lock().unwrap();let listener=TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let host=std::thread::spawn(move||{let mut socket=accept(listener);let body=request(&mut socket);let head=std::str::from_utf8(&body).unwrap();assert!(head.contains(&format!("Host: {address}\r\n")));assert!(head.ends_with("{}"));let response="{\"text\":\"中文😀\"}";write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();});
        let transport=ReviewTransport::new_owned(Arc::new(||false));assert_eq!(transport.post_json(&format!("http://{address}/tool"),&vec![],"{}").unwrap(),"{\"text\":\"中文😀\"}");released();host.join().unwrap();
    }
}
