//! Single-request TCP ownership under the existing ureq HTTP/TLS stack.
//! No document handles, TLS termination, global proxy or process manipulation.
use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

static DNS_ACTIVE: AtomicUsize = AtomicUsize::new(0);
const POLL: Duration = Duration::from_millis(10);

pub(crate) struct NetworkOwner(Arc<State>);
struct State {
    stopped: AtomicBool,
    sockets: Mutex<Vec<TcpStream>>,
    connections: AtomicUsize,
    workers: AtomicUsize,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    deadline: Instant,
    #[cfg(test)]
    test_stop_trace: std::sync::OnceLock<TestStopTrace>,
}
#[derive(Clone)]
pub(crate) struct Resolver(Arc<State>);
impl NetworkOwner {
    pub(super) fn new() -> Self {
        Self::with_cancel(Arc::new(||false))
    }
    pub(super) fn with_cancel(cancelled:Arc<dyn Fn()->bool+Send+Sync>) -> Self {
        Self::with_deadline(cancelled,Instant::now()+Duration::from_secs(90))
    }
    pub(crate) fn with_deadline(cancelled:Arc<dyn Fn()->bool+Send+Sync>,deadline:Instant) -> Self {
        Self(Arc::new(State { stopped: AtomicBool::new(false), sockets: Mutex::new(Vec::new()),
            #[cfg(test)]
            test_stop_trace: std::sync::OnceLock::new(),
            connections: AtomicUsize::new(0), workers: AtomicUsize::new(0), cancelled, deadline }))
    }
    pub(crate) fn resolver(&self) -> Resolver { Resolver(self.0.clone()) }
    pub(super) fn stop(&self) { self.0.stop(); }
}
impl Drop for NetworkOwner { fn drop(&mut self) { self.stop(); } }
impl State {
    fn stopped(&self) -> bool { self.stopped.load(Ordering::Acquire) || (self.cancelled)() || Instant::now() >= self.deadline }
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        #[cfg(test)]
        self.test_trace_event("stop-begin", None, None, None);
        // Lock registration too: a connection created concurrently with stop
        // is either captured here or rejected and closed by register().
        #[cfg(test)]
        let mut trace_socket_index = 0;
        for socket in self.sockets.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            #[cfg(not(test))]
            let _ = socket.shutdown(Shutdown::Both);
            #[cfg(test)]
            {
                let result = socket.shutdown(Shutdown::Both);
                self.test_trace_event("shutdown", Some(trace_socket_index), result.as_ref().err(), None);
                trace_socket_index += 1;
            }
        }
        #[cfg(test)]
        self.test_trace_event("stop-end", None, None, None);
    }
    fn register(&self, socket: &TcpStream) -> io::Result<()> {
        let mut sockets = self.sockets.lock().unwrap_or_else(|e| e.into_inner());
        #[cfg(not(test))]
        if self.stopped() { let _ = socket.shutdown(Shutdown::Both); return Err(stopped()); }
        #[cfg(test)]
        if self.stopped() {
            let result = socket.shutdown(Shutdown::Both);
            self.test_trace_event("register-rejected-shutdown", None, result.as_ref().err(), None);
            return Err(stopped());
        }
        if sockets.len() >= 8 { return Err(stopped()); }
        // Darwin inherits O_NONBLOCK from the listening socket. A timeout
        // alone does not clear it; pumps must not spin on an idle connection.
        socket.set_nonblocking(false)?;
        sockets.push(socket.try_clone()?);
        #[cfg(test)]
        if let Some(trace) = self.test_stop_trace.get() {
            trace.registered.store(sockets.len(), Ordering::Release);
            self.test_trace_event("registered", None, None, None);
        }
        socket.set_nodelay(true)?;
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.set_write_timeout(Some(Duration::from_millis(100)))
    }
}
fn stopped() -> io::Error { io::Error::new(io::ErrorKind::Interrupted, "AI request stopped") }
struct Worker(Arc<State>);
impl Worker {
    fn new(state: &Arc<State>) -> Self { state.workers.fetch_add(1, Ordering::AcqRel); Self(state.clone()) }
}
#[cfg(not(test))]
impl Drop for Worker { fn drop(&mut self) { self.0.workers.fetch_sub(1, Ordering::AcqRel); } }
#[cfg(test)]
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.workers.fetch_sub(1, Ordering::AcqRel);
        self.0.test_trace_event("worker-retired", None, None, None);
    }
}
impl ureq::Resolver for Resolver {
    fn resolve(&self, netloc: &str) -> io::Result<Vec<SocketAddr>> {
        if self.0.stopped() { return Err(stopped()); }
        self.0.connections.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < 4).then_some(n+1))
            .map_err(|_| stopped())?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let state = self.0.clone(); let netloc = netloc.to_owned();
        let worker = Worker::new(&state);
        std::thread::Builder::new().name("readmd-ai-relay".into()).spawn(move || {
            let _worker = worker;
            // Every path closes only this request's registered sockets.
            struct Close(Arc<State>); impl Drop for Close { fn drop(&mut self) { self.0.stop(); } }
            let _close = Close(state.clone());
            let _ = relay(listener, &netloc, state);
        })?;
        Ok(vec![address])
    }
}
fn resolve(netloc: &str, state: &State) -> io::Result<Vec<SocketAddr>> {
    if let Ok(address) = netloc.parse::<SocketAddr>() { return Ok(vec![address]); }
    if state.stopped() { return Err(stopped()); }
    DNS_ACTIVE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < 2).then_some(n+1))
        .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "AI resolver busy"))?;
    struct DnsPermit; impl Drop for DnsPermit { fn drop(&mut self) { DNS_ACTIVE.fetch_sub(1, Ordering::AcqRel); } }
    let permit = DnsPermit; let netloc = netloc.to_owned(); let (send, receive) = mpsc::sync_channel(1);
    // Only a hostname is captured. A cancelled, delayed resolver has neither
    // sockets nor credentials/body and cannot later connect or replay a call.
    std::thread::Builder::new().name("readmd-ai-dns".into()).spawn(move || {
        let _permit = permit;
        let answer = netloc.to_socket_addrs().map(|values| values.take(16).collect::<Vec<_>>());
        let _ = send.send(answer);
    })?;
    loop {
        if state.stopped() { return Err(stopped()); }
        match receive.recv_timeout(POLL) {
            Ok(answer) => return if state.stopped() { Err(stopped()) } else { answer },
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(_) => return Err(stopped()),
        }
    }
}
fn relay(listener: TcpListener, netloc: &str, state: Arc<State>) -> io::Result<()> {
    let local = loop {
        if state.stopped() { return Err(stopped()); }
        match listener.accept() {
            Ok((socket, peer)) if peer.ip().is_loopback() => break socket,
            Ok(_) => return Err(stopped()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(error) => return Err(error),
        }
    };
    drop(listener);
    state.register(&local)?;
    let addresses = resolve(netloc, &state)?;
    let mut remote = None;
    for address in addresses {
        if state.stopped() { return Err(stopped()); }
        if let Ok(socket) = TcpStream::connect_timeout(&address, Duration::from_secs(1)) {
            state.register(&socket)?; remote = Some(socket); break;
        }
    }
    let remote = remote.ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "AI connection failed"))?;
    if state.stopped() { return Err(stopped()); }
    let reverse_local = local.try_clone()?; let reverse_remote = remote.try_clone()?;
    let reverse_state = state.clone();
    let worker = Worker::new(&state);
    let reverse = std::thread::Builder::new().name("readmd-ai-receive".into()).spawn(move || {
        let _worker = worker;
        let result = pump(reverse_remote, reverse_local, &reverse_state);
        reverse_state.stop(); result
    })?;
    let result = pump(local, remote, &state);
    state.stop(); let _ = reverse.join(); result
}
fn retry(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted)
}
fn pump(mut source: TcpStream, mut target: TcpStream, state: &State) -> io::Result<()> {
    let mut bytes = [0u8; 65536];
    loop {
        if state.stopped() { return Err(stopped()); }
        let count = match source.read(&mut bytes) {
            Ok(0) => return Ok(()), Ok(n) => n,
            Err(error) if retry(&error) => continue,
            Err(error) => return Err(error),
        };
        let mut offset = 0;
        while offset < count {
            if state.stopped() { return Err(stopped()); }
            match target.write(&bytes[offset..count]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(n) => offset += n,
                Err(error) if retry(&error) => (),
                Err(error) => return Err(error),
            }
        }
    }
}

// Opt-in, passive test diagnostics. No address, body or error Display text.
#[cfg(test)]
struct TestStopTrace {
    started: Instant,
    registered: AtomicUsize,
    dumped: AtomicBool,
    events: Mutex<Vec<TestStopEvent>>,
}
#[cfg(test)]
#[derive(Clone, Copy)]
struct TestStopEvent {
    elapsed_us: u128,
    phase: &'static str,
    registered: usize,
    stopped: bool,
    workers: usize,
    socket: Option<usize>,
    error_kind: Option<io::ErrorKind>,
    os_code: Option<i32>,
    bytes: Option<usize>,
}
#[cfg(test)]
impl TestStopTrace {
    fn new() -> Self {
        Self { started: Instant::now(), registered: AtomicUsize::new(0),
            dumped: AtomicBool::new(false), events: Mutex::new(Vec::with_capacity(64)) }
    }
}
#[cfg(test)]
impl State {
    fn test_trace_event(&self, phase: &'static str, socket: Option<usize>, error: Option<&io::Error>, bytes: Option<usize>) {
        let Some(trace) = self.test_stop_trace.get() else { return; };
        let event = TestStopEvent { elapsed_us: trace.started.elapsed().as_micros(), phase,
            registered: trace.registered.load(Ordering::Acquire),
            stopped: self.stopped.load(Ordering::Acquire), workers: self.workers.load(Ordering::Acquire),
            socket, error_kind: error.map(io::Error::kind), os_code: error.and_then(io::Error::raw_os_error), bytes };
        trace.events.lock().unwrap_or_else(|error| error.into_inner()).push(event);
    }
    fn test_trace_dump(&self, reason: &'static str) {
        let Some(trace) = self.test_stop_trace.get() else { return; };
        if trace.dumped.swap(true, Ordering::AcqRel) { return; }
        let stderr = io::stderr(); let mut output = stderr.lock();
        let _ = writeln!(output, "READMD_RELAY_STOP_TRACE reason={} elapsed_us={} registered={} stopped={} workers={}",
            reason, trace.started.elapsed().as_micros(), trace.registered.load(Ordering::Acquire),
            self.stopped.load(Ordering::Acquire), self.workers.load(Ordering::Acquire));
        let events = match trace.events.try_lock() {
            Ok(events) => events.clone(),
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner().clone(),
            Err(std::sync::TryLockError::WouldBlock) => {
                let _ = writeln!(output, "READMD_RELAY_STOP_TRACE events_unavailable=busy"); return;
            }
        };
        for (index, event) in events.iter().enumerate() {
            let _ = writeln!(output, "READMD_RELAY_STOP_TRACE event={} elapsed_us={} phase={} registered={} stopped={} workers={} socket={:?} error_kind={:?} os_code={:?} bytes={:?}",
                index, event.elapsed_us, event.phase, event.registered, event.stopped,
                event.workers, event.socket, event.error_kind, event.os_code, event.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TraceOnExit<'a>(&'a State);
    impl Drop for TraceOnExit<'_> {
        fn drop(&mut self) {
            let reason = if std::thread::panicking() { "test-unwind" } else { "test-end" };
            self.0.test_trace_event(reason, None, None, None); self.0.test_trace_dump(reason);
        }
    }
    fn observed_read(state: &State, phase: &'static str, result: io::Result<usize>) -> io::Result<usize> {
        state.test_trace_event(phase, None, result.as_ref().err(), result.as_ref().ok().copied());
        if result.is_err() { state.test_trace_dump("peer-read-error"); }
        result
    }
    #[test]
    fn cancelling_an_actual_stalled_request_retires_both_relay_workers() {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();let address=listener.local_addr().unwrap();
        let owner=NetworkOwner::new();let state=owner.0.clone();let resolver=owner.resolver();
        let _ = state.test_stop_trace.set(TestStopTrace::new());
        let _trace_on_exit = TraceOnExit(&state);
        state.test_trace_event("test-start", None, None, None);
        let worker=std::thread::spawn(move||ureq::AgentBuilder::new().resolver(resolver).timeout(Duration::from_secs(5)).build().post(&format!("http://{address}/tool")).send_string("{}").is_err());
        let deadline=Instant::now()+Duration::from_secs(3);let mut peer=loop {match listener.accept(){Ok((socket,_))=>break socket,Err(error)if error.kind()==io::ErrorKind::WouldBlock=>{assert!(Instant::now()<deadline);std::thread::sleep(POLL);},Err(error)=>panic!("fixture accept: {error}")}};
        peer.set_nonblocking(false).unwrap();peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut buffer=[0u8;4096];assert!(observed_read(&state, "initial-peer-read", peer.read(&mut buffer)).unwrap()>0);assert!(state.workers.load(Ordering::Acquire)>0);
        state.test_trace_event("before-owner-drop", None, None, None);
        let at=Instant::now();drop(owner);assert!(at.elapsed()<Duration::from_millis(200));
        state.test_trace_event("after-owner-drop-assert", None, None, None);
        // Drain any already-sent request body; no response is sent by fixture.
        while observed_read(&state, "drain-peer-read", peer.read(&mut buffer)).unwrap()!=0 {}
        assert!(worker.join().unwrap());
        while state.workers.load(Ordering::Acquire)!=0 {assert!(at.elapsed()<Duration::from_secs(2));std::thread::sleep(POLL);}
        assert!(at.elapsed()<Duration::from_secs(2));
        state.test_trace_event("all-original-assertions-passed", None, None, None);
    }
    #[test]
    fn a_stopped_resolver_cannot_open_a_late_connection() {
        use ureq::Resolver as _;
        let owner=NetworkOwner::new();let resolver=owner.resolver();owner.stop();
        assert!(resolver.resolve("127.0.0.1:1").is_err());assert_eq!(owner.0.connections.load(Ordering::Acquire),0);assert_eq!(owner.0.workers.load(Ordering::Acquire),0);
    }
}
