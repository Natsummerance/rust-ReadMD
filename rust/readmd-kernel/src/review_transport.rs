//! Bounded, cancellable waiting for native review tool requests.
//! Network threads never receive document/store handles and cannot commit edits.
use crate::ai_providers::{Headers, HttpError, LineStream, Transport};
use std::{
    io::{BufRead, BufReader, Read},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
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
        }
    }
    fn stopped(&self) -> bool {
        let stopped = match &self.cancelled { CancelCheck::Borrowed(check) => check(), CancelCheck::Owned(check) => check() };
        stopped || Instant::now() >= self.deadline
    }
}
impl ReviewTransport<'static> {
    pub(crate) fn new_owned(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { cancelled: CancelCheck::Owned(cancelled), deadline: Instant::now() + Duration::from_secs(7200) }
    }
}

struct NativeStream {
    receive: mpsc::Receiver<Result<Option<String>, String>>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    stopped: Arc<AtomicBool>,
    content_type: String,
    deadline: Instant,
}
impl Drop for NativeStream { fn drop(&mut self) { self.stopped.store(true, Ordering::Release); } }
impl LineStream for NativeStream {
    fn content_type(&self) -> Option<&str> { Some(&self.content_type) }
    fn next_line(&mut self) -> Result<Option<String>, String> {
        loop {
            if (self.cancelled)() { return Err("cancelled".into()); }
            if Instant::now() >= self.deadline { return Err("ai_network_error".into()); }
            match self.receive.recv_timeout(Duration::from_millis(40)) {
                Ok(value) => return if (self.cancelled)() { Err("cancelled".into()) } else { value },
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(_) => return Err("ai_network_error".into()),
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
        std::thread::Builder::new()
            .name("readmd-review-request".into())
            .spawn(move || {
                let _permit = permit;
                let result = (|| {
                    let agent = ureq::AgentBuilder::new()
                        .timeout(Duration::from_secs(90))
                        .timeout_connect(Duration::from_secs(10))
                        .timeout_read(Duration::from_secs(90))
                        .timeout_write(Duration::from_secs(10))
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
                        .take((RESPONSE_LIMIT + 1) as u64)
                        .read_to_end(&mut data)
                        .map_err(|_| HttpError::Network("ai_network_error".into()))?;
                    if data.len() > RESPONSE_LIMIT {
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
        let url=url.to_owned();let headers=headers.clone();let body=body.to_owned();
        std::thread::Builder::new().name("readmd-task-stream".into()).spawn(move||{
            let _permit=permit;
            let agent=ureq::AgentBuilder::new().timeout(Duration::from_secs(90)).timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(90)).timeout_write(Duration::from_secs(10)).redirects(0).build();
            let mut request=agent.post(&url);for(name,value)in &headers{request=request.set(name,value);}
            let response=match request.send_string(&body){
                Ok(value)=>value,
                Err(ureq::Error::Status(code,_))=>{let _=opened_tx.send(Err(HttpError::Status{code,body:String::new()}));return;},
                Err(_)=>{let _=opened_tx.send(Err(HttpError::Network("ai_network_error".into())));return;},
            };
            let content_type=response.header("Content-Type").unwrap_or("").to_string();
            if opened_tx.send(Ok(content_type)).is_err(){return;}
            let mut reader=BufReader::new(response.into_reader().take((RESPONSE_LIMIT+1)as u64));let mut total=0usize;
            loop{
                if worker_stopped.load(Ordering::Acquire){return;}
                let mut bytes=Vec::new();
                match reader.read_until(b'\n',&mut bytes){
                    Ok(0)=>{let _=send.send(Ok(None));return;},
                    Ok(n)=>{total=total.saturating_add(n);if total>RESPONSE_LIMIT{let _=send.send(Err("review_ai_budget".into()));return;}},
                    Err(_)=>{let _=send.send(Err("ai_network_error".into()));return;},
                }
                let text=match String::from_utf8(bytes){Ok(value)=>value,Err(_)=>{let _=send.send(Err("ai_invalid_response".into()));return;}};
                if send.send(Ok(Some(text))).is_err(){return;}
            }
        }).map_err(|_|HttpError::Network("review_ai_budget".into()))?;
        loop{
            if self.stopped(){stopped.store(true,Ordering::Release);return Err(HttpError::Network("cancelled".into()));}
            match opened_rx.recv_timeout(Duration::from_millis(40)){
                Ok(Ok(content_type))=>return Ok(Box::new(NativeStream{receive,cancelled:check.clone(),stopped,content_type,deadline:self.deadline.min(Instant::now()+Duration::from_secs(180))})),
                Ok(Err(error))=>return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout)=>{},
                Err(_)=>return Err(HttpError::Network("ai_network_error".into())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{atomic::AtomicBool, Arc},
    };
    #[test]
    fn cancellation_returns_while_owned_network_request_cannot_apply_a_result() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/tool", listener.local_addr().unwrap());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let host = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = [0; 4096];
            let _ = socket.read(&mut bytes).unwrap();
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        });
        let flag = Arc::new(AtomicBool::new(false));
        let flip = flag.clone();
        let cancel = std::thread::spawn(move || {
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            flip.store(true, Ordering::SeqCst);
        });
        let is_cancelled = || flag.load(Ordering::SeqCst);
        let transport = ReviewTransport::new(&is_cancelled);
        let started = Instant::now();
        let result = transport.post_json(&url, &vec![], "{}");
        assert!(matches!(result, Err(HttpError::Network(_))));
        assert!(started.elapsed() < Duration::from_millis(200));
        release_tx.send(()).unwrap();
        cancel.join().unwrap();
        host.join().unwrap();
    }
}
