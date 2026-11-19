//! Local data-only upgrades for the export preset catalog.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, BTreeSet}, fs, path::Path, sync::Mutex};

const INVALID: &str = "invalid_presets";
const LIMIT: usize = 256 * 1024;
static INSTALL: Mutex<()> = Mutex::new(());

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    schema: u32,
    id: String,
    revision: u32,
    presets: Vec<Preset>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    id: String,
    name: String,
    format: String,
    options: Value,
}

fn id_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn follows_defaults(value: &Value, default: &Value) -> bool {
    match (value, default) {
        (Value::Object(fields), Value::Object(schema)) => fields.iter().all(|(key, value)|
            schema.get(key).is_some_and(|default| follows_defaults(value, default))),
        (Value::String(text), Value::String(_)) => text.chars().count() <= 256 && !text.chars().any(char::is_control),
        (Value::Bool(_), Value::Bool(_)) | (Value::Number(_), Value::Number(_)) => true,
        _ => false,
    }
}

fn validate(pack: &Pack) -> Result<(), &'static str> {
    if pack.schema != 1 || pack.revision == 0 || !id_valid(&pack.id)
        || pack.presets.is_empty() || pack.presets.len() > 24 { return Err(INVALID); }
    let defaults = crate::export_styles::default_style();
    let mut ids = BTreeSet::new();
    for preset in &pack.presets {
        let fmt = preset.format.as_str();
        if !id_valid(&preset.id) || !ids.insert(&preset.id) || preset.name.trim().is_empty()
            || preset.name.chars().count() > 48 || preset.name.chars().any(char::is_control)
            || !matches!(fmt, "pdf" | "docx" | "html" | "tex" | "epub" | "pptx" | "png" | "png-pages") { return Err(INVALID); }
        for (key, value) in preset.options.as_object().ok_or(INVALID)? {
            let allowed = match key.as_str() {
                "htmlLayout" => fmt == "html" && matches!(value.as_str(), Some("reading" | "docs" | "magazine")),
                "pptxLayout" => fmt == "pptx" && matches!(value.as_str(), Some("defense" | "lab" | "launch")),
                "pptxTitleFade" => fmt == "pptx" && value.is_boolean(),
                "transition" => fmt == "pptx" && matches!(value.as_str(), Some("none" | "fade" | "slide" | "zoom")),
                "htmlTheme" => fmt == "html" && matches!(value.as_str(), Some("light" | "dark" | "sepia")),
                "page" | "typography" | "headings" | "table" | "images" | "quote" | "code"
                | "link" | "hr" | "header" | "footer" | "cover" | "toc" =>
                    follows_defaults(value, &defaults[key]),
                _ => false,
            };
            if !allowed { return Err(INVALID); }
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<BTreeMap<String, Pack>, &'static str> {
    let bytes = match fs::read(path) {
        Ok(bytes) if bytes.len() <= LIMIT * 4 => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        _ => return Err(INVALID),
    };
    let packs: BTreeMap<String, Pack> = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
    for (id, pack) in &packs {
        if id != &pack.id { return Err(INVALID); }
        validate(pack)?;
    }
    Ok(packs)
}

pub(crate) fn install(data: &Path, input: &Value) -> Result<usize, &'static str> {
    if serde_json::to_vec(input).map_err(|_| INVALID)?.len() > LIMIT { return Err(INVALID); }
    let pack: Pack = serde_json::from_value(input.clone()).map_err(|_| INVALID)?;
    validate(&pack)?;
    let _guard = INSTALL.lock().map_err(|_| INVALID)?;
    let path = data.join("export_preset_packs.json");
    let mut packs = load(&path)?;
    let count = pack.presets.len();
    if let Some(old) = packs.get(&pack.id) {
        if old == &pack { return Ok(count); }
        if old.revision >= pack.revision { return Err("preset_name_conflict"); }
    }
    packs.insert(pack.id.clone(), pack);
    if packs.values().map(|pack| pack.presets.len()).sum::<usize>() > 100 { return Err(INVALID); }
    let bytes = serde_json::to_vec(&packs).map_err(|_| INVALID)?;
    if bytes.len() > LIMIT * 4 { return Err(INVALID); }
    crate::content::write_bytes_atomic(&path, &bytes).map_err(|_| INVALID)?;
    Ok(count)
}

pub(crate) fn catalog(data: &Path) -> Vec<Value> {
    load(&data.join("export_preset_packs.json")).unwrap_or_default().into_values().flat_map(|pack|
        pack.presets.into_iter().map(move |preset| json!({
            "id": format!("pack:{}:{}", pack.id, preset.id), "category": "pack",
            "names": {"en": preset.name}, "formats": [preset.format], "options": preset.options
        }))).collect()
}
