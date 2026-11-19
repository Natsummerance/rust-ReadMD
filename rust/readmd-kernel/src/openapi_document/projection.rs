use super::*;

// Preserve source-owned fields outside the compact, human-readable summary.
// User extensions stay opaque and never become executable markup.
fn other_fields(subject: &Value, consumed: &[&str], name: &str) -> String {
    let Some(object) = subject.as_object() else {
        return String::new();
    };
    let fields = object
        .iter()
        .filter(|(key, _)| !consumed.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<_, _>>();
    if fields.is_empty() {
        return String::new();
    }
    format!("#### {}\n\n{}", code(name), fenced(&Value::Object(fields)))
}

impl Projection<'_> {
    fn warning(&mut self, code: &str, pointer: &str) {
        if self.warnings.len() < 256 {
            self.warnings.push(json!({"code":code,"pointer":pointer}));
        }
    }
    fn resolve(&mut self, value: &Value, location: &str) -> Value {
        self.resolve_with_origins(value, location, false).0
    }
    fn resolve_with_origins(
        &mut self,
        original: &Value,
        location: &str,
        path_item: bool,
    ) -> (Value, BTreeMap<String, String>) {
        let mut value = original.clone();
        let mut origins = BTreeMap::new();
        let mut resolved_any = false;
        if path_item {
            if let Some(object) = value.as_object() {
                origins.extend(
                    object
                        .keys()
                        .map(|key| (key.clone(), format!("{location}/{}", pointer(key)))),
                );
            }
        }
        let mut seen = HashSet::new();
        for depth in 0..=16 {
            let Some(target) = value["$ref"].as_str().map(str::to_owned) else {
                return (
                    value,
                    if resolved_any {
                        origins
                    } else {
                        BTreeMap::new()
                    },
                );
            };
            if depth == 16 {
                self.warning("recursive_reference", location);
                break;
            }
            if !target.starts_with('#') {
                self.warning("external_reference", location);
                break;
            }
            let Some(decoded) = pointer_fragment(&target[1..]).filter(|p| !p.is_empty()) else {
                self.warning("missing_reference", location);
                break;
            };
            if !seen.insert(decoded.clone()) {
                self.warning("recursive_reference", location);
                break;
            }
            let Some(mut resolved) = self.root.pointer(&decoded).cloned() else {
                self.warning("missing_reference", location);
                break;
            };
            let mut siblings = value.as_object().cloned().unwrap_or_default();
            siblings.remove("$ref");
            if path_item {
                // Path Item siblings are fields, including independent methods,
                // in both OAS versions. Collisions have undefined semantics.
                if siblings.keys().any(|key| {
                    !matches!(key.as_str(), "summary" | "description")
                        && resolved.get(key).is_some()
                }) {
                    self.warning("external_reference", location);
                    return (original.clone(), BTreeMap::new());
                }
                if let Some(object) = resolved.as_object_mut() {
                    let mut next_origins: BTreeMap<_, _> = object
                        .keys()
                        .map(|key| (key.clone(), format!("{decoded}/{}", pointer(key))))
                        .collect();
                    for (key, child) in siblings {
                        if let Some(origin) = origins.get(&key) {
                            next_origins.insert(key.clone(), origin.clone());
                        }
                        object.insert(key, child);
                    }
                    origins = next_origins;
                }
                value = resolved;
            } else if self.root["openapi"]
                .as_str()
                .unwrap_or("")
                .starts_with("3.1.")
            {
                value = resolved;
                if let Some(object) = value.as_object_mut() {
                    for key in ["summary", "description"] {
                        if let Some(v) = siblings.get(key) {
                            object.insert(key.into(), v.clone());
                        }
                    }
                }
            } else {
                value = resolved;
            }
            resolved_any = true;
        }
        if path_item {
            (original.clone(), BTreeMap::new())
        } else {
            (value, BTreeMap::new())
        }
    }
    fn section(&self, key: &str, value: &Value) -> String {
        if value.is_null()
            || value.as_array().is_some_and(Vec::is_empty)
            || value.as_object().is_some_and(Map::is_empty)
        {
            return String::new();
        }
        format!("#### {}\n\n{}", label(self.labels, key), fenced(value))
    }
    pub(super) fn generate(&mut self) -> Result<String> {
        let mut out = format!(
            "# {}\n\n{} · {}\n\n",
            escape(self.root["info"]["title"].as_str().unwrap_or("")),
            escape(self.root["info"]["version"].as_str().unwrap_or("")),
            escape(self.root["openapi"].as_str().unwrap_or(""))
        );
        if let Some(description) = self.root["info"]["description"].as_str() {
            out.push_str(&escape(description));
            out.push_str("\n\n");
        }
        out.push_str(&self.section("openapi.servers", &self.root["servers"]));
        out.push_str(&self.section("openapi.tags", &self.root["tags"]));
        out.push_str(&self.section("openapi.externalDocs", &self.root["externalDocs"]));
        out.push_str(&other_fields(
            &self.root["info"],
            &["title", "version", "description"],
            "info",
        ));
        out.push_str(&other_fields(
            self.root,
            &[
                "info",
                "openapi",
                "paths",
                "webhooks",
                "components",
                "tags",
                "servers",
                "externalDocs",
            ],
            "OpenAPI",
        ));
        let v31 = self.root["openapi"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.1."));
        let mut items = Vec::new();
        for group in ["paths", "webhooks"] {
            if group == "webhooks" && !v31 {
                continue;
            }
            if let Some(value) = self.root.get(group) {
                let entries = value.as_object().ok_or(INVALID)?;
                items.extend(entries.iter().map(|(name, value)| (group, name, value)));
            }
        }
        items.sort_by_key(|(group, name, _)| (*group == "webhooks", *name));
        let mut count = 0;
        let mut last_end = (0, 0);
        for (group, path, item) in items {
            if group == "paths" && path.starts_with("x-") {
                out.push_str(&format!(
                    "#### {}\n\n{}",
                    code(&format!("paths.{path}")),
                    fenced(item)
                ));
                continue;
            }
            if !item.is_object() || group == "paths" && !path.starts_with('/') {
                return Err(INVALID);
            }
            let path_pointer = format!("/{group}/{}", pointer(path));
            let (item, origins) = self.resolve_with_origins(item, &path_pointer, true);
            const METHODS: [&str; 8] = [
                "get", "post", "put", "patch", "delete", "head", "options", "trace",
            ];
            let has_operations = METHODS.iter().any(|method| item.get(method).is_some());
            let metadata: Map<_, _> = item
                .as_object()
                .ok_or(INVALID)?
                .iter()
                .filter(|(key, _)| {
                    !METHODS.contains(&key.as_str())
                        && !(has_operations && matches!(key.as_str(), "servers" | "parameters"))
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            if !metadata.is_empty() || !has_operations {
                out.push_str(&format!(
                    "### {}\n\n{}",
                    code(path),
                    fenced(&Value::Object(metadata))
                ));
            }
            for method in METHODS {
                if crate::speech::cancelled() {
                    return Err("cancelled");
                }
                let Some(operation) = item.get(method) else {
                    continue;
                };
                if !operation.is_object() {
                    return Err(INVALID);
                }
                count += 1;
                if count > 2048 {
                    return Err(BUDGET);
                }
                let location = format!("{path_pointer}/{method}");
                let start_byte = out.len();
                let start = last_end.1 + out[last_end.0..].encode_utf16().count();
                let summary = operation["summary"].as_str().unwrap_or("");
                out.push_str(&format!(
                    "## {}{} {}{}\n\n",
                    if group == "webhooks" {
                        format!("{} · ", label(self.labels, "openapi.webhooks"))
                    } else {
                        String::new()
                    },
                    method.to_uppercase(),
                    code(path),
                    if summary.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", cell(summary))
                    }
                ));
                if let Some(id) = operation["operationId"].as_str() {
                    out.push_str(&format!("{}\n\n", code(id)));
                }
                if operation["deprecated"] == true {
                    out.push_str(&format!(
                        "**{}**\n\n",
                        label(self.labels, "openapi.deprecated")
                    ));
                }
                if let Some(description) = operation["description"].as_str() {
                    out.push_str(&escape(description));
                    out.push_str("\n\n");
                }
                let servers = operation.get("servers").or_else(|| item.get("servers"));
                if let Some(servers) = servers {
                    out.push_str(&self.section("openapi.servers", servers));
                }
                let mut parameters: BTreeMap<(String, String), Value> = BTreeMap::new();
                let mut unresolved = Vec::new();
                for (group, owner) in [
                    (&item["parameters"], &path_pointer),
                    (&operation["parameters"], &location),
                ] {
                    if !group.is_null() && !group.is_array() {
                        return Err(INVALID);
                    }
                    for (index, raw) in group.as_array().into_iter().flatten().enumerate() {
                        let parameter_pointer = format!("{owner}/parameters/{index}");
                        let warning_count = self.warnings.len();
                        let p = self.resolve(raw, &parameter_pointer);
                        let (Some(name), Some(position)) = (p["name"].as_str(), p["in"].as_str())
                        else {
                            if self.warnings.len() == warning_count {
                                self.warning("missing_reference", &parameter_pointer);
                            }
                            unresolved.push(raw.clone());
                            continue;
                        };
                        parameters.insert((position.into(), name.into()), p);
                    }
                }
                if !parameters.is_empty() {
                    out.push_str(&format!(
                        "### {}\n\n| {} | {} | {} | {} | {} |\n| --- | --- | --- | --- | --- |\n",
                        label(self.labels, "openapi.parameters"),
                        label(self.labels, "openapi.name"),
                        label(self.labels, "openapi.location"),
                        label(self.labels, "openapi.required"),
                        label(self.labels, "openapi.schema"),
                        label(self.labels, "openapi.description")
                    ));
                    let mut details = Vec::new();
                    for ((position, name), p) in parameters {
                        out.push_str(&format!(
                            "| {} | {} | {} | {} | {} |\n",
                            code(&name),
                            code(&position),
                            label(
                                self.labels,
                                if position == "path" || p["required"] == true {
                                    "openapi.yes"
                                } else {
                                    "openapi.no"
                                }
                            ),
                            cell(&serde_json::to_string(&p["schema"]).unwrap_or_default()),
                            cell(p["description"].as_str().unwrap_or(""))
                        ));
                        // Preserve serialization, examples and content properties too.
                        if p.as_object().is_some_and(|obj| {
                            obj.keys().any(|k| {
                                !matches!(
                                    k.as_str(),
                                    "name" | "in" | "description" | "required" | "schema"
                                )
                            })
                        }) {
                            details.push(p);
                        }
                    }
                    out.push('\n');
                    for detail in details {
                        out.push_str(&fenced(&detail));
                    }
                }
                if !unresolved.is_empty() {
                    out.push_str(&self.section("openapi.parameters", &Value::Array(unresolved)));
                }
                let body = self.resolve(
                    &operation["requestBody"],
                    &format!("{location}/requestBody"),
                );
                out.push_str(&self.section("openapi.requestBody", &body));
                let response_items = match operation.get("responses") {
                    Some(value) => Some(value.as_object().ok_or(INVALID)?),
                    None if v31 => None,
                    None => return Err(INVALID),
                };
                let mut responses = Map::new();
                for (status, response) in response_items.into_iter().flatten() {
                    responses.insert(
                        status.clone(),
                        self.resolve(
                            response,
                            &format!("{location}/responses/{}", pointer(status)),
                        ),
                    );
                }
                out.push_str(&self.section("openapi.responses", &Value::Object(responses)));
                let security = operation.get("security").unwrap_or(&self.root["security"]);
                // Explicit [] clears inherited security; keep this visible.
                if !security.is_null() {
                    out.push_str(&format!(
                        "#### {}\n\n{}",
                        label(self.labels, "openapi.security"),
                        fenced(security)
                    ));
                }
                for key in ["callbacks", "externalDocs", "tags"] {
                    if let Some(v) = operation.get(key) {
                        out.push_str(&self.section(&format!("openapi.{key}"), v));
                    }
                }
                out.push_str(&other_fields(
                    operation,
                    &[
                        "summary",
                        "operationId",
                        "description",
                        "servers",
                        "parameters",
                        "requestBody",
                        "responses",
                        "security",
                        "callbacks",
                        "externalDocs",
                        "tags",
                        "deprecated",
                    ],
                    "operation",
                ));
                self.operations.insert(
                    location.clone(),
                    document_transaction::revision(&out[start_byte..]),
                );
                let end = start + out[start_byte..].encode_utf16().count();
                let mut mapping = json!({"pointer":location,"method":method,"path":path,"operation_id":operation["operationId"],"from":start,"to":end});
                if let Some(origin) = origins.get(method) {
                    mapping["definition"] = json!({"pointer":origin});
                }
                self.maps.push(mapping);
                last_end = (out.len(), end);
                if out.len() > MAX_OUTPUT {
                    return Err(BUDGET);
                }
            }
        }
        for (key, label_key) in [
            ("schemas", "openapi.schemas"),
            ("securitySchemes", "openapi.securitySchemes"),
        ] {
            out.push_str(&self.section(label_key, &self.root["components"][key]));
        }
        out.push_str(&other_fields(
            &self.root["components"],
            &[
                "schemas",
                "securitySchemes",
                "parameters",
                "responses",
                "requestBodies",
                "examples",
                "headers",
                "links",
                "callbacks",
                "pathItems",
            ],
            "components",
        ));
        // Referenced components keep their source names even when no operation
        // uses them. These keys are OpenAPI vocabulary in every UI language.
        for key in [
            "parameters",
            "responses",
            "requestBodies",
            "examples",
            "headers",
            "links",
            "callbacks",
            "pathItems",
        ] {
            if let Some(entries) = self.root["components"]
                .get(key)
                .filter(|v| !v.as_object().is_some_and(Map::is_empty))
            {
                out.push_str(&format!(
                    "#### {}\n\n{}",
                    code(&format!("components.{key}")),
                    fenced(entries)
                ));
            }
        }
        if out.len() > MAX_OUTPUT {
            return Err(BUDGET);
        }
        Ok(out)
    }
}
