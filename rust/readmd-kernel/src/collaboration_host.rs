//! A deliberately separate LAN/self-hosted listener. Only session-owned data
//! and an explicit client asset allowlist are reachable; never the desktop API.
use crate::{
    collaboration::{Role, Service},
    server::{Request, Response},
    ApiError, ApiResult, App,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[derive(Default)]
pub struct Runtime {
    service: Mutex<Option<Arc<Service>>>,
    host: Mutex<Option<Host>>,
}
pub struct Host {
    pub port: u16,
    stop: Arc<AtomicBool>,
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Runtime {
    pub fn service(&self, app: &App) -> ApiResult<Arc<Service>> {
        let mut state = self.service.lock().unwrap_or_else(|e| e.into_inner());
        if state.is_none() {
            *state = Some(Arc::new(Service::open(
                &app.paths.data_dir.join("collaboration.sqlite"),
            )?));
        }
        Ok(state.as_ref().unwrap().clone())
    }
    fn start(&self, app: &App) -> ApiResult<u16> {
        let mut state = self.host.lock().unwrap_or_else(|e| e.into_inner());
        if state.is_none() {
            // Keep LAN URLs and browser IndexedDB origins stable across restarts.
            // If another process owns the previous port, never take it over.
            let file=app.paths.data_dir.join("collaboration-port");
            let previous=std::fs::metadata(&file).ok().filter(|m|m.len()<=16)
                .and_then(|_|std::fs::read_to_string(&file).ok())
                .and_then(|s|s.parse::<u16>().ok()).filter(|p|*p>=1024);
            let launch=|port|Host::start(self.service(app)?,app.paths.assets_dir.clone(),&format!("0.0.0.0:{port}"),"".into(),None);
            let host=if let Some(port)=previous {launch(port).or_else(|_|launch(0))?} else {launch(0)?};
            crate::content::write_text_atomic(&file,&host.port.to_string()).map_err(|_|ApiError::new(500,"collaboration_database"))?;
            *state = Some(host);
        }
        Ok(state.as_ref().unwrap().port)
    }
}
fn bad(code: &'static str) -> ApiError {
    ApiError::new(422, code)
}
fn string<'a>(v: &'a Value, key: &str) -> ApiResult<&'a str> {
    v[key].as_str().ok_or_else(|| bad("collaboration_invalid"))
}
fn role(v: &Value) -> ApiResult<Role> {
    serde_json::from_value(v.clone()).map_err(|_| bad("collaboration_invalid"))
}
pub fn session_request(
    service: &Service,
    room: &str,
    operation: &str,
    secret: &str,
    v: Value,
) -> ApiResult<Value> {
    if uuid::Uuid::parse_str(room).is_err() {
        return Err(bad("collaboration_invalid"));
    }
    match operation {
        "sync" => service.sync(
            room,
            secret,
            string(&v, "vector")?,
            v["update"].as_str(),
            v.get("presence").filter(|v| !v.is_null()).cloned(),
        ),
        "snapshot" => service.snapshot(room, secret),
        "invite" => Ok(
            json!({"ok":true,"credential":service.invite(room,secret,role(&v["role"])?,v["name"].as_str().unwrap_or(""))?}),
        ),
        "permission" => {
            service.permission(
                room,
                secret,
                string(&v, "member")?,
                if v["role"].is_null() {
                    None
                } else {
                    Some(role(&v["role"])?)
                },
            )?;
            Ok(json!({"ok":true}))
        }
        "review" => Ok(json!({"ok":true,"id":service.review(room,secret,v)?})),
        "reviews" => service.reviews(room, secret),
        "attachments" => service.attachments(room, secret),
        "members" => service.members(room, secret),
        "resolve" => {
            service.resolve(
                room,
                secret,
                string(&v, "id")?,
                v["accept"]
                    .as_bool()
                    .ok_or_else(|| bad("collaboration_invalid"))?,
            )?;
            Ok(json!({"ok":true}))
        }
        "close" => {
            service.close(room, secret)?;
            Ok(json!({"ok":true}))
        }
        _ => Err(ApiError::new(404, "collaboration_not_found")),
    }
}
pub fn desktop(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let v = req.json()?;
    let service = app.collaboration.service(app)?;
    if req.path == "/api/collaboration/create" || req.path == "/api/collaboration/resume" {
        // Authenticate a resumed room before exposing or starting its listener.
        let resumed = if req.path.ends_with("/resume") {
            Some(service.snapshot(string(&v, "room")?, req.header("x-readmd-collaboration-token").unwrap_or(""))?)
        } else {
            None
        };
        let port = app.collaboration.start(app)?;
        let credential = if resumed.is_none() {Some(service.create(
            v["title"].as_str().unwrap_or(""),
            string(&v, "content")?,
            v["name"].as_str().unwrap_or(""),
        )?)} else {None};
        let ip = if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            let _ = socket.connect("8.8.8.8:443");
            socket
                .local_addr()
                .ok()
                .map(|a| a.ip().to_string())
                .filter(|a| a != "0.0.0.0")
                .unwrap_or_else(|| "127.0.0.1".into())
        } else {
            "127.0.0.1".into()
        };
        return Ok(Response::json(
            &json!({"ok":true,"credential":credential,"snapshot":resumed,"url":format!("http://{ip}:{port}/"),"port":port}),
        ));
    }
    let room = string(&v, "room")?.to_string();
    let operation = string(&v, "operation")?.to_string();
    Ok(Response::json(&session_request(
        &service,
        &room,
        &operation,
        req.header("x-readmd-collaboration-token").unwrap_or(""),
        v,
    )?))
}
impl Host {
    pub fn start(
        service: Arc<Service>,
        assets: PathBuf,
        address: &str,
        public_origin: String,
        admin: Option<String>,
    ) -> ApiResult<Self> {
        let listener =
            TcpListener::bind(address).map_err(|_| ApiError::new(503, "collaboration_listen"))?;
        let port = listener
            .local_addr()
            .map_err(|_| ApiError::new(503, "collaboration_listen"))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|_| ApiError::new(503, "collaboration_listen"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let active = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        // Winsock may inherit the listener's nonblocking mode.
                        // Request workers use bounded blocking reads instead.
                        if socket.set_nonblocking(false).is_err() {
                            continue;
                        }
                        if active.fetch_add(1, Ordering::AcqRel) >= 64 {
                            active.fetch_sub(1, Ordering::AcqRel);
                            continue;
                        }
                        let (service, assets, active, origin, admin) = (
                            service.clone(),
                            assets.clone(),
                            active.clone(),
                            public_origin.clone(),
                            admin.clone(),
                        );
                        std::thread::spawn(move || {
                            struct Count(Arc<AtomicUsize>);
                            impl Drop for Count {
                                fn drop(&mut self) {
                                    self.0.fetch_sub(1, Ordering::AcqRel);
                                }
                            }
                            let _count = Count(active);
                            let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                            let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                            let response = read(&mut socket).and_then(|r| {
                                dispatch(&service, &assets, &origin, admin.as_deref(), r)
                            });
                            let r = match response {
                                Ok(r) => r,
                                Err(e) => Response::json_status(
                                    e.status,
                                    &json!({"ok":false,"error_code":e.code}),
                                ),
                            };
                            let _ = write(&mut socket, r);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(15))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self { port, stop })
    }
}
fn read(socket: &mut TcpStream) -> ApiResult<Request> {
    let mut reader = BufReader::new(socket);
    let mut head = Vec::new();
    // Bounded even when a malicious peer never sends a newline.
    while head.len() < 16 * 1024 {
        let mut b = [0];
        reader
            .read_exact(&mut b)
            .map_err(|_| bad("collaboration_http"))?;
        head.push(b[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    if !head.ends_with(b"\r\n\r\n") {
        return Err(ApiError::new(431, "collaboration_http"));
    }
    let text = std::str::from_utf8(&head).map_err(|_| bad("collaboration_http"))?;
    let mut lines = text.split("\r\n");
    let parts: Vec<_> = lines.next().unwrap_or("").split(' ').collect();
    if parts.len() != 3
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with('/')
        || parts[1].contains(['?', '#'])
    {
        return Err(bad("collaboration_http"));
    }
    let mut headers = HashMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| bad("collaboration_http"))?;
        if headers
            .insert(k.to_ascii_lowercase(), v.trim().to_string())
            .is_some()
        {
            return Err(bad("collaboration_http"));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err(bad("collaboration_http"));
    }
    let size: usize = match headers.get("content-length") {
        Some(v) => v.parse().map_err(|_| bad("collaboration_http"))?,
        None => 0,
    };
    let max = if parts[1].ends_with("/attachment") {
        16 * 1024 * 1024
    } else {
        8 * 1024 * 1024
    };
    if size > max {
        return Err(ApiError::new(413, "collaboration_budget"));
    }
    let mut body = vec![0; size];
    reader
        .read_exact(&mut body)
        .map_err(|_| bad("collaboration_http"))?;
    Ok(Request {
        method: parts[0].into(),
        path: parts[1].into(),
        headers,
        query: HashMap::new(),
        body,
    })
}
fn dispatch(
    service: &Service,
    assets: &std::path::Path,
    public_origin: &str,
    admin: Option<&str>,
    req: Request,
) -> ApiResult<Response> {
    let host = req
        .header("host")
        .ok_or_else(|| bad("collaboration_http"))?;
    let actual = format!("http://{host}");
    let configured = !public_origin.is_empty()
        && public_origin
            .trim_end_matches('/')
            .strip_prefix("https://")
            .or_else(|| public_origin.trim_end_matches('/').strip_prefix("http://"))
            .is_some_and(|value| value == host);
    let literal = host.parse::<std::net::SocketAddr>().is_ok()
        || host
            .strip_prefix("localhost:")
            .is_some_and(|p| p.parse::<u16>().is_ok());
    if !configured && !literal {
        return Err(ApiError::new(403, "collaboration_origin"));
    }
    if req
        .header("origin")
        .is_some_and(|o| o != actual && o != public_origin.trim_end_matches('/'))
    {
        return Err(ApiError::new(403, "collaboration_origin"));
    }
    if req.method == "GET" {
        let (file, mime) = match req.path.as_str() {
            "/" => ("collaboration/index.html", "text/html; charset=utf-8"),
            "/client.js" => ("collaboration/client.js", "text/javascript; charset=utf-8"),
            "/session.js" => ("collaboration/session.js", "text/javascript; charset=utf-8"),
            "/client.css" => ("collaboration/client.css", "text/css; charset=utf-8"),
            "/tokens.css" => ("css/tokens.css", "text/css; charset=utf-8"),
            "/yjs.js" => ("vendor/yjs.bundle.js", "text/javascript; charset=utf-8"),
            p if p.starts_with("/i18n/")
                && p.ends_with(".json")
                && p.len() < 32
                && p[6..p.len() - 5]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-') =>
            {
                (p.trim_start_matches('/'), "application/json")
            }
            _ => return Err(ApiError::new(404, "collaboration_not_found")),
        };
        return Ok(Response::raw_bytes(200,mime,std::fs::read(assets.join(file)).map_err(|_|ApiError::new(404,"collaboration_not_found"))?)
            .header("Content-Security-Policy","default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'")
            .header("Referrer-Policy","no-referrer").header("X-Content-Type-Options","nosniff"));
    }
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let secret = req
        .header("authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("");
    if req.path == "/sessions" {
        let expected = admin.ok_or_else(|| ApiError::new(403, "collaboration_forbidden"))?;
        if crate::document_transaction::revision(secret)
            != crate::document_transaction::revision(expected)
        {
            return Err(ApiError::new(403, "collaboration_forbidden"));
        }
        let v = req.json()?;
        return Ok(Response::json(
            &json!({"ok":true,"credential":service.create(v["title"].as_str().unwrap_or(""),string(&v,"content")?,v["name"].as_str().unwrap_or(""))?}),
        ));
    }
    let (room, operation) = req
        .path
        .strip_prefix("/sessions/")
        .and_then(|s| s.split_once('/'))
        .ok_or_else(|| ApiError::new(404, "collaboration_not_found"))?;
    if operation == "attachment" {
        let encoded = req.header("x-asset-name-base64");
        let name = if let Some(encoded) = encoded {
            let bytes = crate::server::base64_decode(encoded)
                .ok_or_else(|| bad("collaboration_invalid"))?;
            String::from_utf8(bytes).map_err(|_| bad("collaboration_invalid"))?
        } else {
            req.header("x-asset-name")
                .unwrap_or("attachment")
                .to_string()
        };
        let mime = req
            .header("content-type")
            .unwrap_or("application/octet-stream");
        return Ok(Response::json(
            &json!({"ok":true,"id":service.attachment(room,secret,&name,mime,&req.body)?}),
        ));
    }
    if operation == "download" {
        let v = req.json()?;
        return Ok(Response::raw_bytes(
            200,
            "application/octet-stream",
            service.read_attachment(room, secret, string(&v, "id")?)?,
        )
        .header("Content-Disposition", "attachment"));
    }
    Ok(Response::json(&session_request(
        service,
        room,
        operation,
        secret,
        req.json()?,
    )?))
}
fn write(socket: &mut TcpStream, res: Response) -> std::io::Result<()> {
    write!(socket,"HTTP/1.1 {} Response\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",res.status,res.body.len())?;
    for (k, v) in res.headers {
        write!(socket, "{k}: {v}\r\n")?;
    }
    socket.write_all(b"\r\n")?;
    socket.write_all(&res.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_listener_rejects_desktop_routes_without_connection_resets() {
        let data = tempfile::tempdir().unwrap();
        let service = Arc::new(Service::open(&data.path().join("db")).unwrap());
        let host = Host::start(
            service.clone(),
            data.path().into(),
            "127.0.0.1:0",
            "".into(),
            None,
        )
        .unwrap();
        for _ in 0..100 {
            match ureq::get(&format!("http://127.0.0.1:{}/api/file", host.port)).call() {
                Err(ureq::Error::Status(404, response)) => assert!(response
                    .into_string()
                    .unwrap()
                    .contains("collaboration_not_found")),
                other => panic!("Expected a complete 404 response, got {other:?}"),
            }
        }
        drop(host);
        drop(service);
        std::thread::sleep(Duration::from_millis(60));
    }
    #[test]
    fn origin_and_filesystem_asset_boundaries_are_enforced() {
        let data = tempfile::tempdir().unwrap();
        let service = Service::open(&data.path().join("db")).unwrap();
        for (path, host, origin, status) in [
            ("/api/file", "127.0.0.1:1000", None, 404),
            ("/../../settings.json", "127.0.0.1:1000", None, 404),
            ("/", "evil.example:1000", None, 403),
            (
                "/sessions",
                "127.0.0.1:1000",
                Some("https://evil.example"),
                403,
            ),
        ] {
            let mut headers = HashMap::from([("host".into(), host.into())]);
            if let Some(origin) = origin {
                headers.insert("origin".into(), origin.into());
            }
            let req = Request {
                method: "GET".into(),
                path: path.into(),
                headers,
                query: HashMap::new(),
                body: vec![],
            };
            assert_eq!(
                dispatch(&service, data.path(), "", None, req)
                    .err()
                    .unwrap()
                    .status,
                status
            );
        }
    }
}
