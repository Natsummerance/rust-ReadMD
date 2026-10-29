//! Resolve a review connection without the legacy cross-provider credential
//! adoption used by text chat. All secrets/destinations stay inside the route;
//! failures contain only public localization codes.
use crate::ai_providers::{
    self, AiError, ProviderDirectory, ResolvedRoute, SkillService,
};
use serde_json::{json, Value};

const FROZEN_PROVIDER: &str = "__readmd_review_bound_provider";

/// Bind a supplied credential to the explicit/current connection. When no
/// connection is selected, a credential handle may select its entire owner
/// record, including URL, protocol, models and headers. Request fields cannot
/// override saved destinations or authentication.
pub(crate) fn resolve(
    payload: &Value,
    directory: &dyn ProviderDirectory,
    skills: &dyn SkillService,
) -> Result<ResolvedRoute, AiError> {
    let object = payload.as_object().ok_or_else(denied)?;
    const ALLOWED: &[&str] = &["provider", "credential_id", "model", "messages", "stream", "temperature"];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(denied());
    }
    let credential = match payload.get("credential_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if id.trim().is_empty() => None,
        Some(Value::String(id)) => Some(id.trim()),
        _ => return Err(denied()),
    };
    let Some(credential) = credential else {
        // The normal configured path keeps existing provider/model selection.
        return ai_providers::resolve_chat(payload, directory, skills)
            .map_err(|_| AiError::new("ai_not_configured"));
    };
    if credential.len() > 128 || !crate::ai::is_valid_credential_id(credential) {
        return Err(denied());
    }
    let (selected, current_model) = match payload.get("provider") {
        Some(Value::String(name)) if !name.is_empty() => (Some(name.clone()), None),
        Some(value) if ai_providers::py_truthy(value) => return Err(denied()),
        _ => {
            let current = directory.current_config();
            let selected = current.get("provider_id").filter(|value| ai_providers::py_truthy(value))
                .or_else(|| current.get("provider").filter(|value| ai_providers::py_truthy(value)));
            let name = match selected {
                None => None,
                Some(Value::String(name)) => {
                    let name = ai_providers::py_strip(name);
                    (!name.is_empty()).then_some(name)
                }
                _ => return Err(denied()),
            };
            (name, current.get("model").filter(|value| ai_providers::py_truthy(value)).cloned())
        }
    };
    let using_current = selected.is_some()
        && !payload.get("provider").is_some_and(ai_providers::py_truthy);
    let provider = if let Some(name) = selected {
        directory.find_provider(&name)
    } else {
        directory.find_provider_by_credential(credential).ok_or_else(denied)?
    };
    // A missing credential field cannot authorize adoption of some other
    // connection's secret, even if that connection uses the same wire format.
    if !provider.is_object() || provider.get("credential_id").and_then(Value::as_str) != Some(credential) {
        return Err(denied());
    }
    let frozen = FrozenDirectory { directory, provider, credential };
    let mut bound = payload.clone();
    bound["provider"] = json!(FROZEN_PROVIDER);
    bound["credential_id"] = json!(credential);
    // Making the frozen provider explicit must not lose a valid current model
    // fallback when the caller selected the current connection implicitly.
    if using_current && !payload.get("model").is_some_and(ai_providers::py_truthy) {
        if let Some(model) = current_model { bound["model"] = model; }
    }
    ai_providers::resolve_chat(&bound, &frozen, skills)
        .map_err(|_| AiError::new("ai_not_configured"))
}

fn denied() -> AiError { AiError::new("review_not_authorized") }

struct FrozenDirectory<'a> {
    directory: &'a dyn ProviderDirectory,
    provider: Value,
    credential: &'a str,
}

impl ProviderDirectory for FrozenDirectory<'_> {
    fn current_config(&self) -> Value { json!({}) }
    fn find_provider(&self, name: &str) -> Value {
        if name == FROZEN_PROVIDER { self.provider.clone() } else { Value::Null }
    }
    fn find_provider_by_credential(&self, id: &str) -> Option<Value> {
        (id == self.credential).then(|| self.provider.clone())
    }
    fn load_credential(&self, id: &str) -> String {
        if id == self.credential { self.directory.load_credential(id) } else { String::new() }
    }
    fn decrypt_secret(&self, _: &str) -> String { String::new() }
    fn env_var(&self, name: &str) -> String { self.directory.env_var(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_providers::{Headers, HttpError, LineStream, RouteTarget, SkillError, Transport, VarMap};
    use std::cell::{Cell, RefCell};

    const A: &str = "cred:synthetic_a";
    const B: &str = "cred:synthetic_b";

    struct FakeSettingsDirectory {
        current: Value,
        providers: Vec<Value>,
        loaded: RefCell<Vec<String>>,
    }
    impl FakeSettingsDirectory {
        fn new() -> Self {
            Self {
                current: json!({"provider_id":"provider-a","model":"current-a"}),
                providers: vec![
                    json!({"id":"provider-a","base_url":"https://a.example.invalid/v1","format":"openai","credential_id":A,"models":["model-a"],"headers":{"X-Route":"a"}}),
                    json!({"id":"provider-b","base_url":"https://b.example.invalid/v1","format":"anthropic","credential_id":B,"models":["model-b"],"headers":{"X-Route":"b"}}),
                ],
                loaded: RefCell::new(Vec::new()),
            }
        }
    }
    impl ProviderDirectory for FakeSettingsDirectory {
        fn current_config(&self) -> Value { self.current.clone() }
        fn find_provider(&self, name: &str) -> Value {
            self.providers.iter().find(|provider| provider["id"] == name).cloned().unwrap_or(Value::Null)
        }
        fn find_provider_by_credential(&self, id: &str) -> Option<Value> {
            self.providers.iter().find(|provider| provider["credential_id"] == id).cloned()
        }
        fn load_credential(&self, id: &str) -> String {
            self.loaded.borrow_mut().push(id.into());
            match id { A => "synthetic-secret-a".into(), B => "synthetic-secret-b".into(), _ => String::new() }
        }
        fn decrypt_secret(&self, _: &str) -> String { panic!("review must use its bound credential") }
        fn env_var(&self, _: &str) -> String { String::new() }
    }
    struct NoSkills;
    impl SkillService for NoSkills {
        fn render_skill(&self, _: &str, _: &VarMap) -> Result<String, SkillError> { panic!("no skill in fixture") }
    }
    #[derive(Default)]
    struct NoTransport { calls: Cell<usize> }
    impl Transport for NoTransport {
        fn post_json(&self, _: &str, _: &Headers, _: &str) -> Result<String, HttpError> {
            self.calls.set(self.calls.get() + 1);
            Err(HttpError::Network("unexpected_synthetic_request".into()))
        }
        fn open_stream(&self, _: &str, _: &Headers, _: &str) -> Result<Box<dyn LineStream>, HttpError> {
            panic!("review does not use text streaming")
        }
    }
    fn denied_before_transport(payload: Value, directory: &FakeSettingsDirectory) {
        let transport = NoTransport::default();
        let result = resolve(&payload, directory, &NoSkills).and_then(|route| {
            crate::ai_review::propose(&route, &transport,
                &json!({"document_id":"fixture","content":"one two\n","generation":1,"review_revision":0,"revision":"fixture-revision"}),
                "Review.", &|| false)
        });
        assert_eq!(result.unwrap_err().message,"review_not_authorized");
        assert_eq!(transport.calls.get(),0);
        assert!(directory.loaded.borrow().is_empty(),"denial precedes secret access");
    }

    #[test]
    fn explicit_provider_credential_match_keeps_one_complete_connection() {
        let directory = FakeSettingsDirectory::new();
        let route = resolve(&json!({"provider":"provider-b","credential_id":B,"messages":[]}), &directory, &NoSkills).unwrap();
        assert_eq!(route.args.base_url,"https://b.example.invalid/v1");
        assert_eq!(route.args.api_key,"synthetic-secret-b");
        assert_eq!(route.args.model,"model-b");
        assert_eq!(route.args.custom_headers,json!({"X-Route":"b"}));
        assert_eq!(route.target,RouteTarget::Anthropic);
        assert_eq!(directory.loaded.borrow().as_slice(),&[B.to_string()]);
    }

    #[test]
    fn foreign_credential_and_provider_without_credential_are_denied_before_egress() {
        let directory = FakeSettingsDirectory::new();
        denied_before_transport(json!({"provider":"provider-a","credential_id":B,"messages":[]}),&directory);
        let mut no_credential = FakeSettingsDirectory::new();
        no_credential.providers[0].as_object_mut().unwrap().remove("credential_id");
        denied_before_transport(json!({"provider":"provider-a","credential_id":B,"messages":[]}),&no_credential);
    }

    #[test]
    fn current_provider_is_bound_and_matching_current_model_is_preserved() {
        let directory = FakeSettingsDirectory::new();
        denied_before_transport(json!({"credential_id":B,"messages":[]}),&directory);
        let route = resolve(&json!({"credential_id":A,"messages":[]}),&directory,&NoSkills).unwrap();
        assert_eq!(route.args.base_url,"https://a.example.invalid/v1");
        assert_eq!(route.args.api_key,"synthetic-secret-a");
        assert_eq!(route.args.model,"current-a");
    }

    #[test]
    fn credential_only_selects_the_entire_owner_when_no_provider_is_selected() {
        let mut directory = FakeSettingsDirectory::new();
        directory.current=json!({});
        let route=resolve(&json!({"credential_id":B,"messages":[]}),&directory,&NoSkills).unwrap();
        assert_eq!(route.args.base_url,"https://b.example.invalid/v1");
        assert_eq!(route.args.api_key,"synthetic-secret-b");
        assert_eq!(route.args.model,"model-b");
        assert_eq!(route.tool_protocol,Some("anthropic"));
        assert_eq!(route.args.custom_headers,json!({"X-Route":"b"}));
    }

    #[test]
    fn raw_endpoint_and_auth_overrides_cannot_repair_a_binding_or_redirect_a_key() {
        for (key,value) in [
            ("base_url",json!("https://foreign.example.invalid/v1")),
            ("headers",json!({"Authorization":"synthetic"})),
            ("api_key",json!("synthetic")), ("format",json!("anthropic")),
            ("mode",json!("messages")), ("endpoint_mode",json!("full_url")),
        ] {
            let directory=FakeSettingsDirectory::new();
            let mut request=json!({"provider":"provider-a","credential_id":A,"messages":[]});
            request[key]=value;
            denied_before_transport(request,&directory);
        }
    }

    #[test]
    fn unknown_credential_and_malformed_selectors_return_only_public_codes() {
        for request in [
            json!({"provider":"provider-a","credential_id":"private-looking-invalid-id","messages":[]}),
            json!({"provider":"provider-a","credential_id":17,"messages":[]}),
            json!({"provider":"unknown-private-provider","credential_id":A,"messages":[]}),
        ] {
            denied_before_transport(request,&FakeSettingsDirectory::new());
        }
        let mut directory=FakeSettingsDirectory::new();directory.current=json!({});
        denied_before_transport(json!({"credential_id":"cred:unknown_synthetic","messages":[]}),&directory);
    }

    #[test]
    fn normal_no_credential_selection_preserves_legacy_provider_and_model_defaults() {
        for request in [json!({"messages":[]}),json!({"provider":"provider-b","messages":[],"model":"chosen-model"})] {
            let actual=resolve(&request,&FakeSettingsDirectory::new(),&NoSkills).unwrap();
            let expected=ai_providers::resolve_chat(&request,&FakeSettingsDirectory::new(),&NoSkills).unwrap();
            assert_eq!(actual,expected);
        }
        let missing=resolve(&json!({"provider":"unknown-private-provider","messages":[]}),&FakeSettingsDirectory::new(),&NoSkills).unwrap_err();
        assert_eq!(missing.message,"ai_not_configured");
    }
}
