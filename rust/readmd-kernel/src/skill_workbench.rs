//! The Skill workbench and legacy prompt editor share the executable registry.
use crate::{
    content,
    error::{ApiError, ApiResult},
    server::{ok_json, Request, Response},
    App,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

static EDIT_LOCK: Mutex<()> = Mutex::new(());
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && id.as_bytes()[0] != b'-'
}
fn user_dir(app: &App, id: &str) -> ApiResult<PathBuf> {
    if !valid_id(id) {
        return Err(ApiError::bad_request("skill_id_invalid"));
    }
    Ok(app.paths.data_dir.join("skills").join(id))
}
fn atomic_json(p: &Path, v: &Value) -> ApiResult<()> {
    content::write_bytes_atomic(
        p,
        &serde_json::to_vec_pretty(v).map_err(|_| ApiError::internal("skill_write_failed"))?,
    )
    .map_err(|_| ApiError::internal("skill_write_failed"))
}
fn frontmatter(s: &str) -> Option<(BTreeMap<String, String>, String)> {
    let mut lines = s.lines();
    if lines.next()?.trim_start_matches('\u{feff}') != "---" {
        return None;
    }
    let mut fields = BTreeMap::new();
    let mut closed = false;
    let mut body = vec![];
    for line in lines {
        if closed {
            body.push(line);
        } else if line.trim() == "---" {
            closed = true;
        } else if let Some((k, v)) = line.split_once(':') {
            fields.insert(k.trim().into(), v.trim().trim_matches(['\"', '\'']).into());
        }
    }
    closed.then(|| (fields, body.join("\n").trim().into()))
}
fn load_one(folder: &Path, scope: &str) -> Option<Value> {
    let source = std::fs::read_to_string(folder.join("SKILL.md")).ok()?;
    let (fields, instructions) = frontmatter(&source)?;
    let id = fields.get("name")?;
    if !valid_id(id) || fields.get("description").is_none_or(|s| s.is_empty()) {
        return None;
    }
    let metadata = match std::fs::read(folder.join("readmd.skill.json")) {
        Ok(b) => serde_json::from_slice::<Value>(&b).ok()?,
        Err(_) => json!({}),
    };
    Some(
        json!({"id":id,"name":metadata.get("name").and_then(Value::as_str).unwrap_or(id),"description":fields.get("description"),"instructions":instructions,"scope":scope,"metadata":metadata,"enabled":metadata.get("enabled")!=Some(&json!(false)),"installed":true,"source":"local","file":folder.join("SKILL.md"),"variables":crate::ai_providers::template_variable_names(&instructions)}),
    )
}
pub fn list(app: &App) -> Vec<Value> {
    let mut out = BTreeMap::new();
    for (root, scope) in [
        (app.paths.assets_dir.join("skills"), "builtin"),
        (app.paths.data_dir.join("skills"), "user"),
        (app.paths.workspace.join(".readmd/skills"), "project"),
    ] {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(s) = load_one(&entry.path(), scope) {
                        out.insert(text(&s, "id").to_owned(), s);
                    }
                }
            }
        }
    }
    out.into_values().collect()
}
fn validate(id: &str, source: &str) -> ApiResult<()> {
    if !valid_id(id) || source.len() > 512 * 1024 {
        return Err(ApiError::bad_request("skill_invalid"));
    }
    let (f, body) =
        frontmatter(source).ok_or_else(|| ApiError::bad_request("skill_frontmatter_invalid"))?;
    if f.get("name").map(String::as_str) != Some(id)
        || f.get("description").is_none_or(|s| s.is_empty())
        || body.is_empty()
    {
        return Err(ApiError::bad_request("skill_frontmatter_invalid"));
    }
    if crate::ai_providers::template_variable_names(&body)
        .iter()
        .any(|x| {
            ![
                "context",
                "document",
                "language",
                "output_format",
                "request",
                "selection",
            ]
            .contains(&x.as_str())
        })
    {
        return Err(ApiError::bad_request("skill_variable_invalid"));
    }
    Ok(())
}
fn save(app: &App, id: &str, source: &str, metadata: &Value) -> ApiResult<()> {
    validate(id, source)?;
    let dir = user_dir(app, id)?;
    // Built-in entries must be copied before editing or publishing.
    if app.paths.assets_dir.join("skills").join(id).exists() {
        return Err(ApiError::bad_request("skill_builtin_readonly"));
    }
    std::fs::create_dir_all(&dir).map_err(|_| ApiError::internal("skill_write_failed"))?;
    content::write_bytes_atomic(&dir.join("SKILL.md"), source.as_bytes())
        .map_err(|_| ApiError::internal("skill_write_failed"))?;
    atomic_json(&dir.join("readmd.skill.json"), metadata)
}
fn token(app: &App, id: &str, source: &str, metadata: &Value) -> String {
    let mut h = Sha256::new();
    h.update(app.app_token.as_bytes());
    h.update(id.as_bytes());
    h.update(source.as_bytes());
    h.update(metadata.to_string());
    format!("{:x}", h.finalize())
}
pub fn handle(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method == "GET" {
        let skills = list(app);
        return ok_json(json!({"ok":true,"count":skills.len(),"skills":skills,"engine":"rust"}));
    }
    let p = req.json()?;
    let action = text(&p, "action");
    let id = text(&p, "id");
    if action == "generate" {
        let request = text(&p, "request");
        if request.trim().is_empty() {
            return Err(ApiError::bad_request("skill_request_required"));
        }
        let prompt=format!("Create an original reading Skill. Return only a JSON object with id (lowercase ASCII slug), name, description and instructions. Use only {{{{document}}}}, {{{{request}}}}, {{{{selection}}}}, {{{{language}}}}, {{{{context}}}} and {{{{output_format}}}} as optional variables. Keep instructions under 1200 characters. User request: {}\nReading material:\n{}",request,text(&p,"document").chars().take(12000).collect::<String>());
        let payload = json!({"provider":p.get("provider"),"credential_id":p.get("credential_id"),"model":p.get("model"),"stream":false,"messages":[{"role":"user","content":prompt}]});
        let (status, response) =
            crate::server::call_in_process(app, "POST", "/api/ai/chat", Some(&payload));
        if status != 200 || response.get("ok") == Some(&json!(false)) {
            return Err(ApiError::internal("skill_generation_failed"));
        }
        let raw = text(&response, "content");
        let start = raw
            .find('{')
            .ok_or_else(|| ApiError::bad_request("skill_draft_invalid"))?;
        let end = raw
            .rfind('}')
            .ok_or_else(|| ApiError::bad_request("skill_draft_invalid"))?;
        let mut draft: Value = serde_json::from_str(&raw[start..=end])
            .map_err(|_| ApiError::bad_request("skill_draft_invalid"))?;
        let source = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            text(&draft, "id"),
            text(&draft, "description").replace(['\n', '\r'], " "),
            text(&draft, "instructions")
        );
        validate(text(&draft, "id"), &source)?;
        draft["metadata"] = json!({"enabled":false,"scripts_allowed":false,"source":"ai-draft"});
        return ok_json(json!({"ok":true,"draft":draft}));
    }
    let _guard = EDIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match action {
        "evaluate" | "publish" => {
            let source = text(&p, "content");
            validate(id, source)?;
            let metadata = p.get("metadata").cloned().unwrap_or_else(|| json!({}));
            // Bind confirmation to exactly the evaluated content. Enabled changes on publish.
            let mut bound = metadata.clone();
            bound["enabled"] = json!(false);
            if action == "evaluate" {
                return ok_json(
                    json!({"ok":true,"evaluation_token":token(app,id,source,&bound),"evaluation":{"valid":true,"scripts_allowed":false}}),
                );
            }
            if p.get("confirm") != Some(&json!(true))
                || text(&p, "evaluation_token") != token(app, id, source, &bound)
            {
                return Err(ApiError::bad_request("skill_evaluation_required"));
            }
            let mut safe = metadata;
            safe["enabled"] = json!(true);
            safe["scripts_allowed"] = json!(false);
            save(app, id, source, &safe)?;
        }
        "enable" | "disable" => {
            let dir = user_dir(app, id)?;
            let s =
                load_one(&dir, "user").ok_or_else(|| ApiError::bad_request("skill_not_found"))?;
            let mut m = s["metadata"].clone();
            m["enabled"] = json!(action == "enable");
            atomic_json(&dir.join("readmd.skill.json"), &m)?;
        }
        "export" => {
            let dir = user_dir(app, id)?;
            let source = std::fs::read_to_string(dir.join("SKILL.md"))
                .map_err(|_| ApiError::bad_request("skill_not_found"))?;
            return ok_json(json!({"ok":true,"content":source}));
        }
        _ => return Err(ApiError::bad_request("skill_action_invalid")),
    }
    ok_json(json!({"ok":true,"id":id,"skills":list(app)}))
}
pub fn prompt_templates(app: &App) -> Vec<Value> {
    list(app).into_iter().map(|s|{let metadata=&s["metadata"];json!({"id":s["id"],"skill_id":s["id"],"name":s["name"],"system":metadata.get("system").cloned().unwrap_or(s["instructions"].clone()),"user":metadata.get("user").cloned().unwrap_or(json!("")),"action":metadata.get("action").cloned().unwrap_or(json!("custom")),"builtin":s["scope"]=="builtin","metadata":metadata})}).collect()
}
pub fn prompts(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method == "GET" {
        let templates = prompt_templates(app);
        return ok_json(
            json!({"ok":true,"templates":templates,"prompts":app.setting("aiPrompts")}),
        );
    }
    let p = req.json()?;
    let action = text(&p, "action");
    if action.is_empty() {
        let prompts = p.get("prompts").cloned().unwrap_or(p);
        app.update_settings(&json!({"aiPrompts":prompts}));
        return ok_json(json!({"ok":true,"prompts":prompts,"saved":true}));
    }
    let _guard = EDIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut saved_id = String::new();
    match action {
        "save" | "batch_save" => {
            let items = if action == "save" {
                vec![p.get("template").cloned().unwrap_or(Value::Null)]
            } else {
                p.get("templates")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| ApiError::bad_request("templates_required"))?
            };
            let mut validated = vec![];
            for t in items {
                if text(&t, "name").trim().is_empty() {
                    return Err(ApiError::bad_request("template_name_required"));
                }
                let id = if text(&t, "id").is_empty() {
                    format!("custom-{}", uuid::Uuid::new_v4().simple())
                } else {
                    text(&t, "id").to_owned()
                };
                let system = text(&t, "system");
                let body = frontmatter(system).map(|(_, body)| body).unwrap_or_else(||system.to_owned());
                let user = text(&t, "user");
                let instructions = if user.is_empty() {
                    body
                } else {
                    format!(
                        "{}\n\n{}",
                        body,
                        user.replace("{doc}", "{{document}}")
                            .replace("{prompt}", "{{request}}")
                    )
                };
                let source = format!(
                    "---\nname: {}\ndescription: {}\n---\n\n{}",
                    id,
                    text(&t, "name").replace(['\r', '\n'], " "),
                    instructions
                );
                validate(&id, &source)?;
                if app.paths.assets_dir.join("skills").join(&id).exists() {
                    return Err(ApiError::bad_request("skill_builtin_readonly"));
                }
                validated.push((id,source,json!({"name":t["name"],"system":system,"user":user,"action":t.get("action").cloned().unwrap_or(json!("custom")),"enabled":true,"scripts_allowed":false,"source":"prompt-editor"})));
            }
            for (id, source, metadata) in validated {
                save(app, &id, &source, &metadata)?;
                saved_id = id;
            }
        }
        "delete" => {
            let id = text(&p, "id");
            let dir = user_dir(app, id)?;
            if !dir.is_dir() {
                return Err(ApiError::bad_request("skill_not_found"));
            }
            let trash = app.paths.data_dir.join("skill-trash");
            std::fs::create_dir_all(&trash)
                .map_err(|_| ApiError::internal("skill_write_failed"))?;
            std::fs::rename(
                dir,
                trash.join(format!("{}-{}", id, uuid::Uuid::new_v4().simple())),
            )
            .map_err(|_| ApiError::internal("skill_write_failed"))?;
        }
        _ => return Err(ApiError::bad_request("template_action_invalid")),
    }
    ok_json(json!({"ok":true,"saved_id":saved_id,"templates":prompt_templates(app)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paths::AppPaths, server::call_in_process};
    #[test]
    fn workbench_survives_reload_and_drives_the_runtime_registry() {
        let temp = tempfile::tempdir().unwrap();
        let assets = temp.path().join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        let app = Arc::new(
            App::bootstrap(AppPaths::with_dirs(
                &temp.path().join("data"),
                temp.path(),
                &assets,
            ))
            .unwrap(),
        );
        let template = json!({"id":"original-reasoning","name":"原创论证分析","system":"区分事实与解释。","user":"{doc}\n{prompt}"});
        let (status, saved) = call_in_process(
            &app,
            "POST",
            "/api/ai/prompts",
            Some(&json!({"action":"save","template":template})),
        );
        assert_eq!(status, 200);
        assert_eq!(saved["saved_id"], "original-reasoning");
        let rendered = crate::server::skill_render(
            &app,
            "original-reasoning",
            &json!({"document":"实际材料","request":"检查论证"}),
        )
        .unwrap();
        assert!(rendered.contains("实际材料"));
        assert!(rendered.contains("检查论证"));
        let reloaded = Arc::new(App::bootstrap(app.paths.clone()).unwrap());
        assert_eq!(
            prompt_templates(&reloaded)
                .iter()
                .find(|s| s["id"] == "original-reasoning")
                .unwrap()["name"],
            "原创论证分析"
        );
        let source = std::fs::read_to_string(
            app.paths
                .data_dir
                .join("skills/original-reasoning/SKILL.md"),
        )
        .unwrap();
        let metadata = json!({"id":"original-reasoning","source":"skill-workbench","enabled":false,"scripts_allowed":false});
        let (_, evaluation) = call_in_process(
            &app,
            "POST",
            "/api/skills",
            Some(
                &json!({"action":"evaluate","id":"original-reasoning","content":source,"metadata":metadata}),
            ),
        );
        let mut publication = json!({"action":"publish","confirm":true,"id":"original-reasoning","content":source,"metadata":metadata,"evaluation_token":evaluation["evaluation_token"]});
        publication["content"] = json!(format!("{}\nChanged", source));
        assert_eq!(
            call_in_process(&app, "POST", "/api/skills", Some(&publication)).0,
            400
        );
        publication["content"] = json!(source);
        publication["metadata"]["enabled"] = json!(true);
        assert_eq!(
            call_in_process(&app, "POST", "/api/skills", Some(&publication)).0,
            200
        );
        assert_eq!(
            call_in_process(
                &app,
                "POST",
                "/api/skills",
                Some(&json!({"action":"disable","id":"original-reasoning"}))
            )
            .0,
            200
        );
        assert!(crate::server::skill_render(
            &app,
            "original-reasoning",
            &json!({"document":"材料"})
        )
        .is_err());
        assert_eq!(
            call_in_process(
                &app,
                "POST",
                "/api/ai/prompts",
                Some(&json!({"action":"delete","id":"../escape"}))
            )
            .0,
            400
        );
        assert_eq!(
            call_in_process(
                &app,
                "POST",
                "/api/ai/prompts",
                Some(&json!({"action":"delete","id":"original-reasoning"}))
            )
            .0,
            200
        );
        assert!(!app
            .paths
            .data_dir
            .join("skills/original-reasoning")
            .exists());
        assert_eq!(
            std::fs::read_dir(app.paths.data_dir.join("skill-trash"))
                .unwrap()
                .count(),
            1
        );
    }
}
