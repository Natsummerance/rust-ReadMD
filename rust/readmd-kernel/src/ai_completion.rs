//! Bounded, ephemeral fill-in-the-middle completion. Never reads files or saves
//! chats: only the caller's explicit prefix/suffix enter the selected AI route.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use crate::error::{ApiError, ApiResult};

/// No text or model timing is persisted. Each App owns its own bounded state.
pub struct Runtime {
    metrics: Mutex<HashMap<String, Timing>>,
    cache: Mutex<VecDeque<(String, String, Instant)>>,
    catalogs: Mutex<HashMap<String,(Vec<String>,Instant)>>,
    pub catalog_discovery: Mutex<()>,
    pub transport: crate::ai_providers::ureq_transport::PooledTransport,
    pub fast_transport: crate::ai_providers::ureq_transport::PooledTransport,
}
struct Timing { millis: f64, successes: u32, failed_until: Option<Instant>, touched: Instant }
impl Default for Runtime {
    fn default() -> Self {
        Self { metrics: Mutex::new(HashMap::new()), cache: Mutex::new(VecDeque::new()), catalogs: Mutex::new(HashMap::new()), catalog_discovery: Mutex::new(()),
            transport: crate::ai_providers::ureq_transport::UreqTransport {
                json_timeout: Duration::from_secs(20), stream_timeout: Duration::from_secs(20),
            }.pooled(), fast_transport: crate::ai_providers::ureq_transport::UreqTransport {
                json_timeout: Duration::from_secs(8), stream_timeout: Duration::from_secs(8),
            }.pooled() }
    }
}
impl Runtime {
    pub fn catalog(&self, scope: &str) -> Option<Vec<String>> {
        let catalogs=self.catalogs.lock().unwrap_or_else(|e| e.into_inner());
        catalogs.get(scope).filter(|(models,time)| time.elapsed() < Duration::from_secs(if models.is_empty(){60}else{300})).map(|(models,_)| models.clone())
    }
    pub fn remember_catalog(&self, scope: String, models: Vec<String>) {
        let mut catalogs=self.catalogs.lock().unwrap_or_else(|e| e.into_inner());
        catalogs.retain(|_,(_,time)| time.elapsed()<Duration::from_secs(300));
        if catalogs.len()>=16 { if let Some(oldest)=catalogs.iter().min_by_key(|(_,(_,time))| *time).map(|(k,_)| k.clone()){catalogs.remove(&oldest);} }
        catalogs.insert(scope,(models.into_iter().filter(|s| !s.is_empty() && s.len()<=256).take(256).collect(),Instant::now()));
    }
    pub fn cached(&self, key: &str) -> Option<String> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|item| item.2.elapsed() < Duration::from_secs(60));
        let index = cache.iter().position(|item| item.0 == key)?;
        let item = cache.remove(index)?;
        let text = item.1.clone(); cache.push_back(item); Some(text)
    }
    pub fn remember(&self, key: String, text: String) {
        if text.is_empty() { return; }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|item| item.0 != key && item.2.elapsed() < Duration::from_secs(60));
        cache.push_back((key, text, Instant::now()));
        while cache.len() > 32 { cache.pop_front(); }
    }
    pub fn observe(&self, scope: &str, model: &str, elapsed: Duration, useful: bool) {
        let mut metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
        let key = format!("{scope}:{model}");
        let now = Instant::now();
        let value = metrics.entry(key).or_insert(Timing { millis: elapsed.as_secs_f64()*1000., successes: 0, failed_until: None, touched: now });
        if useful { value.millis = 0.65*value.millis + 0.35*elapsed.as_secs_f64()*1000.; value.failed_until = None; value.successes=value.successes.saturating_add(1); }
        else { value.failed_until = Some(now + Duration::from_secs(300)); }
        value.touched = now;
        while metrics.len() > 128 {
            if let Some(oldest) = metrics.iter().min_by_key(|(_,v)| v.touched).map(|(k,_)| k.clone()) { metrics.remove(&oldest); }
            else { break; }
        }
    }
    pub fn plan(&self, provider: &Value, current: &str, body: &Value, scope: &str) -> Plan {
        let settings = normalize_settings(provider.get("autocomplete"));
        let explicit = body.get("model").and_then(Value::as_str).filter(|s| !s.trim().is_empty() && s.len() <= 256);
        if let Some(model) = explicit { return Plan { models: vec![model.into()], automatic: false, selection: "fixed" }; }
        if settings["mode"] == "model" { return Plan { models: vec![settings["model"].as_str().unwrap_or("").into()], automatic: false, selection: "fixed" }; }
        if settings["mode"] == "current" { return Plan { models: vec![current.into()], automatic: false, selection: "current" }; }
        let metrics = self.metrics.lock().unwrap_or_else(|e| e.into_inner());
        let mut candidates = Vec::new();
        for model in provider.get("models").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).chain(std::iter::once(current)) {
            if model.is_empty() || model.len() > 256 || candidates.iter().any(|(m,_,_)| m == model) { continue; }
            let Some(prior) = model_prior(model, model == current) else { continue };
            let timing = metrics.get(&format!("{scope}:{model}"));
            if timing.is_some_and(|t| t.failed_until.is_some_and(|until| until > Instant::now())) { continue; }
            // Successful observations outrank name-only guesses: a slightly
            // slower useful model is preferable to an unverified retired alias.
            let measured = timing.filter(|t| t.successes>0);
            candidates.push((model.to_owned(), measured.map_or(prior, |t| t.millis), measured.is_some()));
        }
        candidates.sort_by(|a,b| b.2.cmp(&a.2).then_with(|| a.1.total_cmp(&b.1)).then_with(|| a.0.cmp(&b.0)));
        if candidates.first().is_some_and(|(model,_,_)| model!=current)
            && candidates.get(1).is_some_and(|(_,_,measured)| !measured) {
            if let Some(index)=candidates.iter().position(|(model,_,_)| model==current) {
                let saved=candidates.remove(index);candidates.insert(1,saved);
            }
        }
        Plan { models: candidates.into_iter().map(|(m,_,_)| m).take(3).collect(), automatic: true, selection: "automatic" }
    }
}
pub struct Plan { pub models: Vec<String>, pub automatic: bool, pub selection: &'static str }

/// Only inference preferences; never an alternative key, address or provider.
pub fn normalize_settings(value: Option<&Value>) -> Value {
    let mode = value.and_then(|v| v.get("mode")).and_then(Value::as_str).unwrap_or("auto");
    let model = value.and_then(|v| v.get("model")).and_then(Value::as_str).unwrap_or("").trim();
    if mode == "current" { json!({"mode":"current"}) }
    else if mode == "model" && !model.is_empty() && model.len() <= 256 && !model.chars().any(char::is_control) { json!({"mode":"model","model":model}) }
    else { json!({"mode":"auto"}) }
}

/// Name-based cold-start priors are heuristics, NOT measured speed or quality.
/// Only text families with a useful general-writing baseline enter automatic
/// exploration; unknown models remain available through explicit selection.
fn model_prior(model: &str, current: bool) -> Option<f64> {
    let m = model.to_lowercase();
    if ["embedding","rerank","whisper","tts","image","imagen","dall-e","veo","audio","moderation"].iter().any(|s| m.contains(s)) { return None; }
    let mut score = if m.contains("haiku") { 1300. }
        else if m.contains("flash") && !m.contains("lite") { 1700. }
        else if m.contains("mini") && (m.contains("gpt") || m.contains("mistral")) { 1900. }
        else if m.contains("codestral") || m.contains("qwen") && m.contains("coder") { 2000. }
        else if m.contains("sonnet") { 1500. }
        else if current { 4000. } else { return None; };
    if ["thinking","reasoner","-high","tiered","opus","-pro"].iter().any(|s| m.contains(s)) { score += 7000.; }
    if m.contains("lite") || m.contains("nano") { score += 4000.; }
    Some(score)
}

pub fn fingerprint(value: &Value) -> String { format!("{:x}", Sha256::digest(value.to_string().as_bytes())) }

pub fn usable(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    !lower.is_empty() && !["model has been retired", "model is retired", "model is no longer available", "model not found", "模型已下线", "模型已停用", "模型已退役", "模型不存在"].iter().any(|s| lower.contains(s))
        && !( ["gemini", "claude", "gpt"].iter().any(|s| lower.contains(s))
            && ["retired", "no longer available", "discontinued"].iter().any(|s| lower.contains(s)) )
}

/// Some OpenAI-compatible relays return an HTTP 200 *notice* instead of a
/// completion. Keep this narrow so normal discussion of retired models works.
pub fn retired_model_notice(text:&str)->bool {
    let lower=text.trim().to_lowercase();
    lower.len()<512 && ["gemini ","claude ","gpt-"].iter().any(|p|lower.starts_with(p))
        && ["no longer available","has been retired","is retired"].iter().any(|p|lower.contains(p))
        && lower.contains("please switch")
}

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
pub struct Permit;
impl Permit {
    pub fn acquire() -> ApiResult<Self> {
        ACTIVE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < 2).then_some(n + 1))
            .map_err(|_| ApiError::new(429, "ai_completion_busy"))?;
        Ok(Self)
    }
}
impl Drop for Permit { fn drop(&mut self) { ACTIVE.fetch_sub(1, Ordering::AcqRel); } }

pub struct Context { pub prefix: String, pub suffix: String, pub revision: String }
impl Context {
    pub fn parse(body: &Value) -> ApiResult<Self> {
        let prefix = body.get("prefix").and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("invalid_completion_context"))?;
        let suffix = body.get("suffix").and_then(Value::as_str).unwrap_or("");
        if prefix.chars().count() > 8000 || suffix.chars().count() > 2000 || prefix.contains('\0') || suffix.contains('\0') {
            return Err(ApiError::bad_request("completion_context_too_large"));
        }
        if prefix.trim().chars().count() < 4 { return Err(ApiError::bad_request("completion_context_too_short")); }
        let revision = format!("{:x}", Sha256::digest(format!("{}:{prefix}{suffix}", prefix.len()).as_bytes()));
        Ok(Self { prefix: prefix.into(), suffix: suffix.into(), revision })
    }
    pub fn payload(&self, body: &Value, instructions: &str) -> Value {
        let mut payload = json!({"stream": false, "temperature": 0.2, "messages": [
            {"role":"system", "content":instructions},
            {"role":"user", "content": serde_json::to_string(&json!({"prefix":self.prefix,"suffix":self.suffix})).unwrap()}
        ]});
        // No raw key, URL, custom headers, Skill or session accepted here.
        for key in ["provider", "model"] { if let Some(v) = body.get(key).and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 256) { payload[key] = json!(v); } }
        payload
    }
    /// Keep the immediate writing context; avoid retransmitting long chapters.
    pub fn compact(&self) -> Self {
        Self { prefix: self.prefix.chars().rev().take(3000).collect::<Vec<_>>().into_iter().rev().collect(),
            suffix: self.suffix.chars().take(750).collect(), revision: self.revision.clone() }
    }
    pub fn preview(&self, raw: &str) -> String {
        let start = raw.trim_start();
        if start.is_empty() || self.prefix.trim_start().starts_with(start)
            || self.prefix.rsplit('\n').next().unwrap_or("").trim_start().starts_with(start)
            || ["<think>","<|fim_middle|>","<fim_middle>"].iter().any(|token| token.starts_with(start)) {
            return String::new();
        }
        let text = self.clean(raw);
        if usable(&text) { text } else { String::new() }
    }
    /// A complete short unit can be returned before an upstream stream ends.
    /// Preserve Markdown delimiters and do not cut a code expression midway.
    pub fn ready(&self, raw: &str) -> Option<String> {
        if raw.trim().is_empty() || self.prefix.trim_start().starts_with(raw.trim_start())
            || self.prefix.rsplit('\n').next().unwrap_or("").trim_start().starts_with(raw.trim_start()) { return None; }
        let text = self.clean(raw);
        if !usable(&text) || text.chars().count() < 12 { return None; }
        let code = self.prefix.matches("```").count() % 2 != 0;
        for (i,c) in text.char_indices() {
            let end = i+c.len_utf8();
            let sentence = !code && matches!(c, '。'|'！'|'？'|'!'|'?')
                || !code && c == '.' && text[..end].chars().count() >= 40 && text[end..].chars().next().is_none_or(char::is_whitespace);
            let unit = &text[..end];
            if sentence && unit.chars().count() >= 12 && unit.matches('[').count() == unit.matches(']').count()
                && unit.matches('(').count() == unit.matches(')').count() && unit.matches('`').count() % 2 == 0 { return Some(unit.into()); }
        }
        None
    }
    pub fn fim_prompt(&self, protocol: &str) -> ApiResult<String> {
        match protocol {
            "starcoder" => Ok(format!("<fim_prefix>{}<fim_suffix>{}<fim_middle>", self.prefix, self.suffix)),
            "qwen" => Ok(format!("<|fim_prefix|>{}<|fim_suffix|>{}<|fim_middle|>", self.prefix, self.suffix)),
            "deepseek" => Ok(format!("<｜fim▁begin｜>{}<｜fim▁hole｜>{}<｜fim▁end｜>", self.prefix, self.suffix)),
            _ => Err(ApiError::bad_request("unsupported_completion_protocol")),
        }
    }
    pub fn clean(&self, raw: &str) -> String {
        let mut text = raw.replace("\r\n", "\n");
        // Reasoning is never displayed as a suggestion, including an incomplete
        // reasoning block. Ordinary Markdown fences inside code remain intact.
        if let Some(start) = text.find("<think>") {
            if let Some(end) = text[start..].find("</think>") { text.replace_range(start..start + end + 8, ""); }
            else { return String::new(); }
        }
        for token in ["<|endoftext|>", "<|fim_middle|>", "<fim_middle>", "<|im_end|>", "<｜end▁of▁sentence｜>"] {
            if let Some(i) = text.find(token) { text.truncate(i); }
        }
        if let Some(rest) = text.strip_prefix(&self.prefix) { text = rest.to_owned(); }
        if text.starts_with("```markdown\n") && text.trim_end().ends_with("```") {
            text = text[12..text.trim_end().len()-3].trim_end_matches('\n').into();
        }
        // Chat models sometimes echo just the cursor's current line rather
        // than the entire context. Strip a substantial exact echo, preserving
        // short intentional repetitions and UTF-8 boundaries.
        let line = self.prefix.rsplit('\n').next().unwrap_or("").trim_start();
        if line.chars().count() >= 8 {
            if let Some(rest) = text.trim_start().strip_prefix(line) { text = rest.to_owned(); }
        }
        // Remove overlap with right context, on Unicode character boundaries.
        for (i, _) in text.char_indices() {
            if !self.suffix.is_empty() && self.suffix.starts_with(&text[i..]) { text.truncate(i); break; }
        }
        text = text.lines().take(6).collect::<Vec<_>>().join("\n").chars().take(1000).collect();
        if text.trim().is_empty() || text.contains('\0') { return String::new(); }
        let compact = text.trim();
        if compact.chars().count() >= 8 && self.prefix.trim_end().ends_with(compact) { return String::new(); }
        text
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn c(p: &str, s: &str) -> Context { Context::parse(&json!({"prefix":p,"suffix":s})).unwrap() }
    #[test] fn bounds_are_unicode_safe() {
        assert!(Context::parse(&json!({"prefix":"文".repeat(8000)})).is_ok());
        assert!(Context::parse(&json!({"prefix":"文".repeat(8001)})).is_err());
        assert!(Context::parse(&json!({"prefix":"abc"})).is_err());
        assert!(Context::parse(&json!({"prefix":"hello","suffix":"x".repeat(2001)})).is_err());
    }
    #[test] fn unicode_overlap_and_echo() {
        let x = c("哲学的方法：", "，再审视结论。");
        assert_eq!(x.clean("提出问题，再审视结论。"), "提出问题");
        assert_eq!(x.clean("哲学的方法：提出问题"), "提出问题");
        assert_eq!(c("# 札记\n\n好的研究从观察开始，我们应该", "").clean("好的研究从观察开始，我们应该检验证据。"), "检验证据。");
        assert_ne!(c("abcd", "ef").revision, c("abcde", "f").revision);
    }
    #[test] fn reasoning_and_repetition_filtered() {
        let x = c("We examine evidence. ", "");
        assert_eq!(x.clean("<think>secret reasoning"), "");
        assert_eq!(x.clean("<think>reason</think>Ask why.<|im_end|>junk"), "Ask why.");
        assert_eq!(x.clean("We examine evidence."), "");
        assert_eq!(x.clean("```markdown\nAsk why.\n```"), "Ask why.");
    }
    #[test] fn no_request_privileges_or_history() {
        let x = c("Some prose", "");
        let p = x.payload(&json!({"api_key":"secret","base_url":"https://bad","session":"private","skill":"bad","model":"test"}), "Insert only");
        for key in ["api_key","base_url","session","skill"] { assert!(p.get(key).is_none()); }
        assert_eq!(p["model"], "test");
        for scheme in ["qwen","starcoder","deepseek"] { assert!(x.fim_prompt(scheme).unwrap().contains("Some prose")); }
        assert!(x.fim_prompt("other").is_err());
    }
    #[test] fn automatic_routing_learns_speed_without_overriding_fixed_models() {
        let runtime = Runtime::default();
        let provider = json!({"models":["gemini-3.8-flash-high","claude-sonnet-4-6","claude-haiku-4-5","text-embedding-3-small","image-fast"]});
        let plan = runtime.plan(&provider,"gemini-3.8-flash-high",&json!({}),"connection-a");
        assert_eq!(plan.models[0],"claude-haiku-4-5");
        assert!(!plan.models.iter().any(|m| m.contains("embedding") || m.contains("image")));
        runtime.observe("connection-a","claude-haiku-4-5",Duration::from_secs(10),true);
        runtime.observe("connection-a","claude-sonnet-4-6",Duration::from_millis(650),true);
        assert_eq!(runtime.plan(&provider,"gemini-3.8-flash-high",&json!({}),"connection-a").models[0],"claude-sonnet-4-6");
        runtime.observe("connection-a","claude-sonnet-4-6",Duration::from_secs(1),false);
        assert!(!runtime.plan(&provider,"gemini-3.8-flash-high",&json!({}),"connection-a").models.contains(&"claude-sonnet-4-6".into()));
        assert_eq!(runtime.plan(&provider,"gemini-3.8-flash-high",&json!({"model":"claude-sonnet-4-6"}),"connection-a").models,vec!["claude-sonnet-4-6"]);
        // A second connection has independent observations and no borrowed key.
        assert_eq!(runtime.plan(&provider,"gemini-3.8-flash-high",&json!({}),"connection-b").models[0],"claude-haiku-4-5");
        let writing=json!({"models":["gemini-2.5-flash","claude-sonnet-4-6","gemini-3.8-flash-tiered"]});
        assert_eq!(runtime.plan(&writing,"gemini-3.8-flash-tiered",&json!({}),"relay").models[..2],["claude-sonnet-4-6","gemini-3.8-flash-tiered"]);
        runtime.observe("relay","claude-sonnet-4-6",Duration::from_millis(6225),true);
        assert_eq!(runtime.plan(&writing,"gemini-3.8-flash-tiered",&json!({}),"relay").models[0],"claude-sonnet-4-6");
        assert_eq!(runtime.plan(&writing,"gemini-3.8-flash-tiered",&json!({}),"relay").models[1],"gemini-3.8-flash-tiered");
    }
    #[test] fn saved_policy_and_unknown_model_remain_explicit() {
        let runtime = Runtime::default();
        assert_eq!(runtime.plan(&json!({"autocomplete":{"mode":"current"}}),"slow-model",&json!({}),"p").models,vec!["slow-model"]);
        assert_eq!(runtime.plan(&json!({"autocomplete":{"mode":"model","model":"private-model"}}),"chat-model",&json!({}),"p").models,vec!["private-model"]);
        assert_eq!(normalize_settings(Some(&json!({"mode":"model","model":"m","api_key":"bad","url":"bad"}))),json!({"mode":"model","model":"m"}));
        assert_eq!(normalize_settings(Some(&json!({"mode":"model","model":"\n"}))),json!({"mode":"auto"}));
    }
    #[test] fn early_stream_unit_preserves_context_and_markdown_boundaries() {
        let context = c("# Notes\n\nWe examine the evidence, then ","\n\n## Results");
        assert!(context.ready("We examine the evidence, then ").is_none());
        assert_eq!(context.ready("we test each explanation against a counterexample. More text"),Some("we test each explanation against a counterexample.".into()));
        assert!(context.ready("检查这条[尚未完成的 Markdown 链接。 ").is_none());
        assert!(context.ready("检查这条[跨句子的 Markdown 链接。](notes.md)").is_none());
        assert_eq!(context.preview("<thi"),"");
        assert!(context.ready("<think>unfinished reasoning").is_none());
        assert!(!usable("Gemini 3.5 Flash has been retired. Please switch models."));
        assert!(retired_model_notice("Gemini 3.5 Flash is no longer available. Please switch to Gemini 3.7 Flash in the latest version of Antigravity."));
        assert!(!retired_model_notice("The article discusses why Gemini models have been retired and how users switch providers."));
        assert!(!retired_model_notice("Gemini is useful for editing research notes."));
        let long = c(&"文".repeat(8000),&"后".repeat(2000)).compact();
        assert_eq!(long.prefix.chars().count(),3000);assert_eq!(long.suffix.chars().count(),750);
    }
    #[test] fn runtime_cache_is_bounded_ephemeral_and_isolated() {
        let runtime=Runtime::default();
        for i in 0..40 { runtime.remember(i.to_string(),"useful candidate".into()); }
        assert!(runtime.cached("0").is_none());assert_eq!(runtime.cached("39"),Some("useful candidate".into()));
        assert!(Runtime::default().cached("39").is_none());
    }
}
