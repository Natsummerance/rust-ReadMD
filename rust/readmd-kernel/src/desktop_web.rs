//! Isolated, task-scoped system WebViews for dynamic extraction and sign-in.
//! Untrusted pages receive only a capture callback, never the reader bridge.
use serde_json::{json,Value};
use std::collections::{HashMap,VecDeque};
use std::sync::{Arc,Mutex};
use std::path::{Path,PathBuf};
use std::time::{Duration,Instant};
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window,WindowBuilder,WindowId};
use wry::{WebContext,WebView,WebViewBuilder};

pub enum Message { Reader(Value), Capture(u64,Value) }
pub static QUEUE: Mutex<VecDeque<Message>> = Mutex::new(VecDeque::new());

pub fn origin(url:&str) -> Option<String> {
    let uri=url.parse::<wry::http::Uri>().ok()?;
    let scheme=uri.scheme_str()?;let authority=uri.authority()?.as_str();
    if !matches!(scheme,"http"|"https") || authority.contains('@') || url.chars().any(char::is_control) || url.len()>4096 {return None;}
    Some(format!("{scheme}://{}",authority.to_ascii_lowercase()))
}

fn is_private(url:&str) -> bool {
    let Some(uri)=url.parse::<wry::http::Uri>().ok() else {return true;};
    let host=uri.host().unwrap_or("").trim_matches(['[',']']).to_ascii_lowercase();
    if host.is_empty() || host=="localhost" || host.ends_with(".local") || host.ends_with(".internal") || host.ends_with(".localhost") {return true;}
    if let Ok(ip)=host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(ip)=>ip.is_private()||ip.is_loopback()||ip.is_link_local()||ip.is_unspecified()||ip.is_broadcast(),
            std::net::IpAddr::V6(ip)=>ip.is_loopback()||ip.is_unspecified()||ip.is_unique_local()||ip.is_unicast_link_local()||ip.to_ipv4_mapped().is_some_and(|ip|ip.is_private()||ip.is_loopback()||ip.is_link_local()),
        };
    }
    false
}

// Keep the authorized browser session alive until revocation. Session cookies
// disappear when its last WebView closes, even if the profile directory survives.
struct Grant { task:String,origin:String,profile:Arc<tempfile::TempDir>,expires:Instant,_session:Job }
struct Job {
    // Close view and window before deleting the isolated profile.
    _view:WebView,window:Window,_context:WebContext,profile:Arc<tempfile::TempDir>,
    request:u64,task:String,origin:String,authorization:bool,private:bool,deadline:Instant,
}
pub struct Manager { jobs:HashMap<u64,Job>,grants:HashMap<String,Grant>,counter:u64,reader_origin:String,profile_root:PathBuf }

impl Manager {
    pub fn new(reader_url:&str,data_dir:&Path) -> Self {Self{jobs:HashMap::new(),grants:HashMap::new(),counter:0,reader_origin:origin(reader_url).unwrap_or_default(),profile_root:data_dir.join("web-capture")}}
    pub fn active(&self)->bool { !self.jobs.is_empty() }
    fn reply(reader:&WebView,id:u64,result:Value) {
        let payload=json!({"id":id,"result":result});
        let _=reader.evaluate_script(&format!("window.__readmdWebResolve && window.__readmdWebResolve({payload});"));
    }
    fn finish(&mut self,key:u64,result:Value,reader:&WebView) {
        if let Some(job)=self.jobs.remove(&key) {Self::reply(reader,job.request,result);}
    }
    pub fn close_window(&mut self,id:WindowId,reader:&WebView)->bool {
        if let Some(key)=self.jobs.iter().find(|(_,j)|j.window.id()==id).map(|(key,_)|*key) {
            self.finish(key,json!({"ok":false,"code":"cancelled"}),reader);return true;
        }false
    }
    pub fn tick<T:'static>(&mut self,target:&EventLoopWindowTarget<T>,reader:&WebView,wake:Arc<dyn Fn()+Send+Sync>) {
        let expired:Vec<_>=self.jobs.iter().filter(|(_,j)|Instant::now()>=j.deadline).map(|(k,_)|*k).collect();
        for key in expired {self.finish(key,json!({"ok":false,"code":"render_timeout"}),reader);}
        self.grants.retain(|_,g|g.expires>Instant::now());
        loop {
            let next=QUEUE.lock().unwrap_or_else(|e|e.into_inner()).pop_front();
            let Some(message)=next else {break;};
            match message {
                Message::Capture(key,value)=>{
                    let Some(job)=self.jobs.get(&key) else {continue;};
                    let final_url=value.get("final_url").and_then(Value::as_str).unwrap_or("");
                    let final_origin=origin(final_url);
                    if final_origin.as_deref()==Some(&self.reader_origin) || final_origin.is_none() || (job.private && final_origin.as_deref()!=Some(&job.origin)) {
                        self.finish(key,json!({"ok":false,"code":"render_navigation_blocked"}),reader);continue;
                    }
                    if job.authorization {
                        if final_origin.as_deref()!=Some(&job.origin) {continue;}
                        let token=uuid::Uuid::new_v4().to_string();
                        let job=self.jobs.remove(&key).expect("authorized job exists");
                        job.window.set_visible(false);
                        Self::reply(reader,job.request,json!({"ok":true,"grant":token}));
                        self.grants.insert(token,Grant{task:job.task.clone(),origin:job.origin.clone(),profile:job.profile.clone(),expires:Instant::now()+Duration::from_secs(600),_session:job});
                    } else {
                        let html=value.get("html").and_then(Value::as_str).unwrap_or("");
                        if html.is_empty() || html.len()>8*1024*1024 {self.finish(key,json!({"ok":false,"code":"render_document_invalid"}),reader);continue;}
                        self.finish(key,json!({"ok":true,"html":html,"final_url":final_url,"engine":"system-webview"}),reader);
                    }
                }
                Message::Reader(value)=>{
                    let id=value.get("id").and_then(Value::as_u64).unwrap_or(0);
                    let task=value.get("task").and_then(Value::as_str).unwrap_or("");
                    match value.get("op").and_then(Value::as_str).unwrap_or("") {
                        "cancel"|"revoke"=>{
                            let keys:Vec<_>=self.jobs.iter().filter(|(_,j)|j.task==task).map(|(k,_)|*k).collect();
                            for key in keys {self.finish(key,json!({"ok":false,"code":"cancelled"}),reader);}
                            if value.get("op").and_then(Value::as_str)==Some("revoke") {
                                self.grants.retain(|token,g|g.task!=task && token!=task);
                            }
                            Self::reply(reader,id,json!({"ok":true}));
                        }
                        "authorize"|"render"=>{
                            if let Err(code)=self.start(target,&value,wake.clone()) {Self::reply(reader,id,json!({"ok":false,"code":code}));}
                        }
                        _=>Self::reply(reader,id,json!({"ok":false,"code":"invalid_render_request"})),
                    }
                }
            }
        }
    }
    fn start<T:'static>(&mut self,target:&EventLoopWindowTarget<T>,value:&Value,wake:Arc<dyn Fn()+Send+Sync>)->Result<(),String> {
        if self.jobs.len()>=4 {return Err("renderer_busy".into());}
        let url=value.get("url").and_then(Value::as_str).unwrap_or("");
        let expected_origin=origin(url).ok_or("invalid_url")?;
        if expected_origin==self.reader_origin {return Err("render_navigation_blocked".into());}
        let task=value.get("task").and_then(Value::as_str).unwrap_or("");
        if task.is_empty() || task.len()>128 {return Err("invalid_task_id".into());}
        let authorization=value.get("op").and_then(Value::as_str)==Some("authorize");
        let token=value.get("grant").and_then(Value::as_str).unwrap_or("");
        let granted=if !token.is_empty() {
            Some(self.grants.get(token).filter(|g|g.task==task && g.origin==expected_origin && g.expires>Instant::now()).ok_or("private_grant_invalid")?)
        }else{None};
        if !authorization && granted.is_none() && is_private(url) {return Err("private_authorization_required".into());}
        let private=authorization||granted.is_some();
        std::fs::create_dir_all(&self.profile_root).map_err(|e|e.to_string())?;
        let profile=match granted {Some(g)=>g.profile.clone(),None=>Arc::new(tempfile::Builder::new().prefix("task-").tempdir_in(&self.profile_root).map_err(|e|e.to_string())?)};
        if self.jobs.values().any(|j|j.task==task) {return Err("renderer_busy".into());}
        self.counter+=1;let key=self.counter;
        let interactive=authorization||value.get("interactive").and_then(Value::as_bool).unwrap_or(false);
        let timeout=value.get("timeout").and_then(Value::as_u64).unwrap_or(if authorization{300000}else{25000}).clamp(1000,300000);
        let title=if authorization{"ReadMD · 登录并授权此站点"}else{"ReadMD · 网页提取"};
        let window=WindowBuilder::new().with_title(title).with_visible(interactive).with_inner_size(tao::dpi::LogicalSize::new(1000.0,720.0)).build(target).map_err(|e|e.to_string())?;
        let mut context=WebContext::new(Some(profile.path().to_path_buf()));
        let reader_origin=self.reader_origin.clone();let bound_origin=expected_origin.clone();
        let script=capture_script(interactive,authorization,value.get("label").and_then(Value::as_str).unwrap_or(if authorization{"授权此站点"}else{"提取此页"}));
        let builder=WebViewBuilder::new_with_web_context(&mut context)
            .with_url(url)
            .with_initialization_script(script)
            .with_navigation_handler(move |url| {
                let Some(current)=origin(&url) else{return url=="about:blank";};
                current!=reader_origin && (!is_private(&url) || private && current==bound_origin)
            })
            .with_new_window_req_handler(|_,_|wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if request.body().len()>8*1024*1024 {return;}
                if let Ok(value)=serde_json::from_str::<Value>(request.body()) {
                    if value.get("readmd_capture").and_then(Value::as_bool)==Some(true) {
                        // Bind capture to the actual sending frame's origin.
                        if origin(&request.uri().to_string())!=value.get("final_url").and_then(Value::as_str).and_then(origin) {return;}
                        QUEUE.lock().unwrap_or_else(|e|e.into_inner()).push_back(Message::Capture(key,value));wake();
                    }
                }
            });
        #[cfg(not(target_os="linux"))]
        let view=builder.build(&window).map_err(|e|e.to_string())?;
        #[cfg(target_os="linux")]
        let view={use tao::platform::unix::WindowExtUnix;use wry::WebViewBuilderExtUnix;builder.build_gtk(window.default_vbox().ok_or("webview_container_unavailable")?).map_err(|e|e.to_string())?};
        if interactive {window.set_focus();}
        self.jobs.insert(key,Job{_view:view,window,_context:context,profile,request:value.get("id").and_then(Value::as_u64).unwrap_or(0),task:task.into(),origin:expected_origin,authorization,private,deadline:Instant::now()+Duration::from_millis(timeout)});
        Ok(())
    }
}

fn capture_script(interactive:bool,authorization:bool,label:&str)->String {
    let options=json!({"interactive":interactive,"authorization":authorization,"label":label});
    format!("const __readmdCaptureOptions={options};\n{}",r#"
(() => {
  if (window.top !== window.self) return;
  const options=__readmdCaptureOptions;
  let sent=false, lastChange=Date.now(), started=Date.now();
  function capture() {
    if(sent)return;
    const clone=document.documentElement.cloneNode(true);
    clone.querySelectorAll('#readmd-capture-bar,script,noscript').forEach(n=>n.remove());
    clone.querySelectorAll('input,textarea').forEach(n=>{n.removeAttribute('value');n.textContent='';});
    const html='<!doctype html>'+clone.outerHTML;
    if(html.length>8*1024*1024)return;
    sent=true;
    window.ipc.postMessage(JSON.stringify({readmd_capture:true,html,final_url:location.href}));
  }
  function mount() {
    if(!document.body)return;
    if(options.interactive && !document.getElementById('readmd-capture-bar')) {
      const bar=document.createElement('div');bar.id='readmd-capture-bar';
      bar.style.cssText='position:fixed;z-index:2147483647;right:20px;bottom:20px;background:#fff;border:1px solid #ddd;border-radius:14px;padding:10px;box-shadow:0 6px 28px #0003;';
      const button=document.createElement('button');button.textContent=options.label;
      button.style.cssText='min-height:44px;min-width:120px;background:#466b55;color:white;border:0;border-radius:10px;padding:10px 18px;font:15px system-ui;cursor:pointer;';
      button.onclick=capture;bar.appendChild(button);document.body.appendChild(bar);
    }
    new MutationObserver(()=>{lastChange=Date.now();}).observe(document.body,{childList:true,subtree:true,characterData:true});
    if(!options.interactive) {
      const timer=setInterval(()=>{
        if(Date.now()-started>4000 && Date.now()-lastChange>1000 || Date.now()-started>18000) {clearInterval(timer);capture();}
      },250);
    }
  }
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',mount,{once:true});else mount();
})();
"#)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_urls_are_origin_bound_and_never_file_or_credentials() {
        assert_eq!(origin("https://example.test/path?q=1"),Some("https://example.test".into()));
        for url in ["file:///C:/private","javascript:alert(1)","https://user:pass@example.test/"] {assert!(origin(url).is_none());}
        for url in ["http://localhost/","http://192.168.1.2/","http://[::1]/","http://[::ffff:127.0.0.1]/"] {assert!(is_private(url));}
        assert!(!is_private("https://example.test/"));
    }
}
