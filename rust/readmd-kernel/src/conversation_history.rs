//! Durable conversation documents for the AI history panel.
use crate::{
    content,
    error::{ApiError, ApiResult},
    server::{ok_json, Request, Response},
    App,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
static LOCK: Mutex<()> = Mutex::new(());
pub fn handle(app: &Arc<App>, req: &Request) -> Option<ApiResult<Response>> {
    let body = if req.method == "POST" {
        match req.json() {
            Ok(b) => b,
            Err(e) => return Some(Err(e)),
        }
    } else {
        json!({})
    };
    // Preserve the older raw-message interface when its explicit session is used.
    if req.q("session").is_some() || (req.method == "POST" && body.get("action").is_none()) {
        return None;
    }
    Some((|| {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let file = app.paths.data_dir.join("ai-history.json");
        let mut sessions: Vec<Value> = match std::fs::read(&file) {
            Ok(b) => {
                serde_json::from_slice(&b).map_err(|_| ApiError::internal("ai_history_corrupt"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(_) => return Err(ApiError::internal("ai_history_failed")),
        };
        if req.method == "GET" {
            if let Some(id) = req.q("id") {
                let session = sessions
                    .into_iter()
                    .find(|s| s["id"] == id)
                    .ok_or_else(|| ApiError::bad_request("ai_session_not_found"))?;
                return ok_json(json!({"ok":true,"session":session}));
            }
            sessions.sort_by(|a, b| b["updated"].as_u64().cmp(&a["updated"].as_u64()));
            let summaries:Vec<Value>=sessions.into_iter().map(|s|json!({"id":s["id"],"title":s["title"],"updated":s["updated"],"created":s["created"],"msgCount":s["messages"].as_array().map_or(0,|m|m.len()),"provider":s["provider"],"model":s["model"]})).collect();
            return ok_json(json!({"ok":true,"sessions":summaries}));
        }
        let action = body["action"].as_str().unwrap_or("");
        let mut saved = Value::Null;
        match action {
            "save" => {
                let s = body
                    .get("session")
                    .filter(|s| s.is_object())
                    .ok_or_else(|| ApiError::bad_request("ai_session_invalid"))?;
                let messages = s["messages"]
                    .as_array()
                    .ok_or_else(|| ApiError::bad_request("ai_session_invalid"))?;
                if messages.len() > 2000
                    || messages.iter().any(|m| {
                        !matches!(m["role"].as_str(), Some("user" | "assistant"))
                            || !m["content"].is_string()
                            || m["ephemeral"] == true
                    })
                {
                    return Err(ApiError::bad_request("ai_session_invalid"));
                }
                let id = s["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if id.len() > 128 {
                    return Err(ApiError::bad_request("ai_session_invalid"));
                }
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let created = sessions
                    .iter()
                    .find(|x| x["id"] == id)
                    .map(|x| x["created"].clone())
                    .unwrap_or(json!(now));
                saved = json!({"id":id,"title":s["title"].as_str().unwrap_or("Untitled").chars().take(300).collect::<String>(),"provider":s["provider"],"model":s["model"],"doc":s["doc"],"messages":messages,"usage":s["usage"],"created":created,"updated":now});
                sessions.retain(|x| x["id"] != id);
                sessions.push(saved.clone());
            }
            "delete" => {
                let id = body["id"]
                    .as_str()
                    .ok_or_else(|| ApiError::bad_request("ai_session_invalid"))?;
                sessions.retain(|s| s["id"] != id);
            }
            "clear" => sessions.clear(),
            _ => return Err(ApiError::bad_request("ai_history_action_invalid")),
        }
        let bytes = serde_json::to_vec_pretty(&sessions)
            .map_err(|_| ApiError::internal("ai_history_failed"))?;
        content::write_bytes_atomic(&file, &bytes)
            .map_err(|_| ApiError::internal("ai_history_failed"))?;
        ok_json(json!({"ok":true,"session":saved}))
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paths::AppPaths, server::call_in_process};
    #[test]
    fn conversations_reopen_export_rename_delete_and_exclude_private_messages() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        let app = Arc::new(
            App::bootstrap(AppPaths::with_dirs(
                &dir.path().join("data"),
                dir.path(),
                &assets,
            ))
            .unwrap(),
        );
        let mut session = json!({"title":"研究方法","messages":[{"role":"user","content":"区分事实与解释"},{"role":"assistant","content":"先描述证据，再检查推理。"}]});
        let (status, r) = call_in_process(
            &app,
            "POST",
            "/api/ai/history",
            Some(&json!({"action":"save","session":session})),
        );
        assert_eq!(status, 200);
        let id = r["session"]["id"].as_str().unwrap();
        let loaded = Arc::new(App::bootstrap(app.paths.clone()).unwrap());
        let (_, listed) = call_in_process(&loaded, "GET", "/api/ai/history", None);
        assert_eq!(listed["sessions"][0]["msgCount"], 2);
        let (_, full) = call_in_process(&loaded, "GET", &format!("/api/ai/history?id={id}"), None);
        assert_eq!(full["session"]["messages"], session["messages"]);
        session = full["session"].clone();
        session["title"] = json!("修订讨论");
        assert_eq!(
            call_in_process(
                &loaded,
                "POST",
                "/api/ai/history",
                Some(&json!({"action":"save","session":session}))
            )
            .0,
            200
        );
        session["messages"][0]["ephemeral"] = json!(true);
        assert_eq!(
            call_in_process(
                &loaded,
                "POST",
                "/api/ai/history",
                Some(&json!({"action":"save","session":session}))
            )
            .0,
            400
        );
        let (_, full) = call_in_process(&loaded, "GET", &format!("/api/ai/history?id={id}"), None);
        assert_eq!(full["session"]["title"], "修订讨论");
        assert_eq!(
            call_in_process(
                &loaded,
                "POST",
                "/api/ai/history",
                Some(&json!({"action":"delete","id":id}))
            )
            .0,
            200
        );
        assert_eq!(
            call_in_process(&loaded, "GET", &format!("/api/ai/history?id={id}"), None).0,
            400
        );
    }
}
