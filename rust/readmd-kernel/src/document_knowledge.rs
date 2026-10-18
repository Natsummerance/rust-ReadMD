//! Local Markdown navigation and completion. Shares the inspector's path,
//! byte and parser boundaries; never writes a document or escapes a workspace.
use super::*;

fn context(root: &Path, file: Option<&Path>) -> Result<(PathBuf, PathBuf), String> {
    let root = workspace(root)?;
    let base = if let Some(file) = file {
        if !file.is_absolute() { return Err("file_path_must_be_absolute".into()); }
        let parent = file.parent().ok_or("file_not_found")?;
        let parent = crate::paths::canonical_existing(parent).map_err(|_| "file_not_found")?;
        let parent = crate::paths::canonicalize_or_clean(&parent);
        if !parent.starts_with(&root) { return Err("file_outside_workspace".into()); }
        parent
    } else { root.clone() };
    Ok((root, base))
}

fn files(root: &Path, images: bool) -> (Vec<PathBuf>, bool) {
    let mut found = Vec::new();
    let mut truncated = false;
    for (n, entry) in walkdir::WalkDir::new(root).follow_links(false).max_depth(24)
        .sort_by_file_name().into_iter().filter_entry(visible).enumerate() {
        if n >= 20_000 || found.len() >= MAX_FILES { truncated = true; break; }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let image = matches!(content::ext_of(path).as_str(), "png"|"jpg"|"jpeg"|"gif"|"webp"|"svg"|"avif");
        if (images && image) || (!images && markdown(path)) { found.push(path.to_path_buf()); }
    }
    (found, truncated)
}

fn relative(base: &Path, target: &Path) -> String {
    let a: Vec<_> = base.components().collect();
    let b: Vec<_> = target.components().collect();
    let common = a.iter().zip(&b).take_while(|(x,y)| x == y).count();
    let mut parts = vec!["..".to_string(); a.len() - common];
    parts.extend(b[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

fn resolve_file(root: &Path, base: &Path, target: &str) -> Result<PathBuf, String> {
    if target.len() > 4096 { return Err("invalid_link_target".into()); }
    for name in [target.to_string(), format!("{target}.md"), format!("{target}.markdown")] {
        if let Some(path) = local_path(base, &name, root).filter(|p| p.is_file() && markdown(p)) {
            return Ok(path);
        }
    }
    // Bare wiki names may refer to a note elsewhere in the workspace. A
    // collision is an error, never an arbitrary first match.
    if target.contains(['/', '\\', ':']) || target == ".." { return Err("link_not_found".into()); }
    let decoded = percent_encoding::percent_decode_str(target).decode_utf8().map_err(|_| "invalid_link_target")?;
    let (candidates, truncated) = files(root, false);
    let matches: Vec<_> = candidates.into_iter().filter(|p| {
        p.file_stem().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&decoded))
            || p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&decoded))
    }).collect();
    if matches.len() > 1 { return Err("ambiguous_link_target".into()); }
    if truncated { return Err("workspace_scan_limit".into()); }
    matches.into_iter().next().ok_or_else(|| "link_not_found".into())
}

pub fn resolve_knowledge_link(root: &Path, file: Option<&Path>, target: &str) -> Result<Value, String> {
    let (root, base) = context(root, file)?;
    let (name, fragment) = target.split_once('#').unwrap_or((target, ""));
    let path = if name.trim().is_empty() {
        absolute_file(file.ok_or("file_not_found")?)?
    } else { resolve_file(&root, &base, name.trim())? };
    if !path.starts_with(&root) { return Err("file_outside_workspace".into()); }
    let mut result = json!({"path":path,"heading":null});
    if !fragment.is_empty() {
        let text = crate::text_encoding::detect_and_decode(&bytes_bounded(&path)?).0;
        let fragment = percent_encoding::percent_decode_str(fragment).decode_utf8().map_err(|_| "invalid_link_target")?;
        let rows = heading_data(&text, AnchorStyle::Desktop);
        let heading = rows.iter().find(|h| h["id"].as_str() == Some(fragment.as_ref()))
            .or_else(|| rows.iter().find(|h| h["text"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(&fragment))))
            .ok_or("heading_not_found")?;
        result["heading"] = heading.clone();
    }
    Ok(result)
}

pub fn suggest_knowledge_links(root: &Path, file: Option<&Path>, draft: &str, kind: &str, query: &str, limit: usize) -> Result<Value, String> {
    if draft.len() > MAX_DOCUMENT_BYTES { return Err("document_too_large".into()); }
    if query.len() > 4096 || !(1..=60).contains(&limit) { return Err("invalid_request".into()); }
    let (root, base) = context(root, file)?;
    let mut result = Vec::new();
    let mut truncated = false;
    let needle = query.to_lowercase();
    match kind {
        "wiki"|"link"|"image" if query.contains('#') => {
            let (name, fragment) = query.split_once('#').unwrap();
            let text = if name.is_empty() { draft.to_string() } else {
                let path = resolve_file(&root, &base, name)?;
                if file.is_some_and(|f| crate::paths::canonicalize_or_clean(f) == path) { draft.to_string() }
                else { crate::text_encoding::detect_and_decode(&bytes_bounded(&path)?).0 }
            };
            for row in heading_data(&text, AnchorStyle::Desktop) {
                let title = row["text"].as_str().unwrap_or("");
                let id = row["id"].as_str().unwrap_or("");
                if title.to_lowercase().contains(&fragment.to_lowercase()) || id.contains(&fragment.to_lowercase()) {
                    result.push(json!({"label":title,"insert":format!("{name}#{id}"),"detail":format!("H{} · {name}",row["level"]),"type":"text"}));
                }
            }
        }
        "wiki"|"link"|"image" => {
            let (candidates, cut) = files(&root, kind == "image"); truncated = cut;
            for path in candidates {
                let rel = relative(&base, &path);
                if !rel.to_lowercase().contains(&needle) { continue; }
                let insert = if kind == "wiki" {
                    rel.strip_suffix(".md").unwrap_or(&rel).to_string()
                } else {
                    rel.split('/').map(|s| percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()).collect::<Vec<_>>().join("/")
                };
                result.push(json!({"label":rel,"insert":insert,"detail":"","type":if kind=="image" {"text"} else {"namespace"}}));
            }
            result.sort_by_key(|v| { let s=v["label"].as_str().unwrap_or(""); (s.starts_with("../"), !s.to_lowercase().starts_with(&needle), s.len(), s.to_string()) });
        }
        "footnote" => {
            let mut seen = HashSet::new();
            for event in Parser::new_ext(draft, options()) {
                if let Event::Start(Tag::FootnoteDefinition(key)) = event {
                    if key.to_lowercase().contains(&needle) && seen.insert(key.to_string()) {
                        result.push(json!({"label":key.as_ref(),"insert":key.as_ref(),"detail":"","type":"variable"}));
                    }
                }
            }
        }
        "citation" => {
            let names=crate::publication_metadata::bibliography_sources(draft).map_err(|error|error.code.to_string())?;
            if names.len()>8 { return Err("academic_budget".into()); }
            // A leading brace is completion context, not part of the key. The
            // editor retains an explicit @{...} pair; otherwise non-plain
            // Pandoc keys are inserted with their required braces.
            let braced=query.starts_with('{');
            let needle=query.strip_prefix('{').unwrap_or(query).to_lowercase();
            let plain_key=regex::Regex::new(r"^[\p{L}\p{N}_]+(?:[:.#$%&\-+?<>~/][\p{L}\p{N}_]+)*$").expect("citation key grammar");
            let mut budget = 0;
            let mut format=None;
            let mut data=String::new();
            let mut json_items=Vec::<Value>::new();
            for name in names {
                let Some(path)=local_path(&base,&name,&root).filter(|p|matches!(content::ext_of(p).as_str(),"bib"|"json")) else {continue};
                let json_format=content::ext_of(&path)=="json";
                if format.is_some_and(|previous|previous!=json_format){return Err("academic_mixed_bibliography".into());}
                format=Some(json_format);
                let bytes=bytes_bounded(&path)?; budget += bytes.len();
                if budget > MAX_DOCUMENT_BYTES { return Err("document_too_large".into()); }
                let text=crate::text_encoding::detect_and_decode(&bytes).0;
                if json_format{json_items.extend(serde_json::from_str::<Vec<Value>>(&text).map_err(|_|"academic_bibliography")?);}
                else{data.push_str(&text);data.push('\n');}
            }
            if let Some(json_format)=format{
                if json_format{data=serde_json::to_string(&json_items).map_err(|_|"academic_bibliography")?;}
                for entry in crate::academic::reference_summaries(if json_format{"csl-json"}else{"biblatex"},&data).map_err(|error|error.code)? {
                    if entry.key.to_lowercase().contains(&needle) {
                        let insert=if braced||plain_key.is_match(&entry.key){entry.key.clone()}else{format!("{{{}}}",entry.key)};
                        result.push(json!({"label":entry.key,"insert":insert,"detail":entry.title,"type":"variable"}));
                    }
                }
            }
            result.sort_by_key(|v|v["label"].as_str().unwrap_or("").to_string());
        }
        _ => return Err("invalid_completion_kind".into()),
    }
    truncated |= result.len() > limit;
    result.truncate(limit);
    Ok(json!({"items":result,"truncated":truncated}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> tempfile::TempDir {
        let t=tempfile::tempdir().unwrap();
        fs::create_dir(t.path().join("notes")).unwrap();
        fs::write(t.path().join("index.md"),"# Home").unwrap();
        fs::write(t.path().join("notes/认识.md"),"# 认识\n\n## 证据与判断\n\n## 证据与判断\n").unwrap();
        t
    }
    #[test] fn resolves_nested_names_fragments_and_duplicates() {
        let t=setup(); let file=t.path().join("index.md");
        let v=resolve_knowledge_link(t.path(),Some(&file),"认识#证据与判断-1").unwrap();
        assert_eq!(v["heading"]["position"]["line"],5);
        fs::write(t.path().join("认识.md"),"# Another").unwrap();
        // Exact sibling wins; two non-sibling names never resolve arbitrarily.
        assert!(resolve_knowledge_link(t.path(),Some(&file),"认识").is_ok());
        fs::create_dir(t.path().join("other")).unwrap();
        fs::write(t.path().join("other/认识.md"),"# Other").unwrap();
        let sub=t.path().join("other/start.md"); fs::write(&sub,"# Start").unwrap();
        fs::create_dir(t.path().join("third")).unwrap();
        let third=t.path().join("third/start.md");fs::write(&third,"# Start").unwrap();
        assert_eq!(resolve_knowledge_link(t.path(),Some(&third),"认识").unwrap_err(),"ambiguous_link_target");
    }
    #[test] fn completions_use_unsaved_draft_and_ignore_code_headings() {
        let t=setup(); let file=t.path().join("index.md");
        let v=suggest_knowledge_links(t.path(),Some(&file),"# New\n~~~md\n# Fake\n~~~\n## New", "wiki","#",60).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(),2);
        assert_eq!(v["items"][1]["insert"],"#new-1");
        let v=suggest_knowledge_links(t.path(),Some(&file),"", "wiki","认识#证据",60).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(),2);
    }
    #[test] fn paths_footnotes_citations_and_boundaries() {
        let t=setup(); let file=t.path().join("index.md");
        fs::write(t.path().join("references.bib"),"@book{work2026,title={Evidence},author={A},year={2026}}").unwrap();
        let draft="---\nbibliography: references.bib\n---\n[^scope]: Valid\n\n```md\n[^fake]: Example\n```";
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"citation","work",60).unwrap();
        assert_eq!(v["items"][0]["insert"],"work2026");
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"footnote","",60).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(),1);
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"wiki","认识",60).unwrap();
        assert_eq!(v["items"][0]["insert"],"notes/认识");
        assert!(resolve_knowledge_link(t.path(),Some(&file),"../../escape").is_err());
        assert!(resolve_knowledge_link(t.path(),Some(&file),"https://example.test").is_err());
        assert!(suggest_knowledge_links(t.path(),Some(&file),&"x".repeat(MAX_DOCUMENT_BYTES+1),"wiki","",60).is_err());
        assert!(suggest_knowledge_links(t.path(),Some(&file),"","invalid","",60).is_err());
    }
    #[test] fn citation_completion_uses_shared_biblatex_and_json_entries_and_preserves_braced_keys() {
        let t=setup();let file=t.path().join("index.md");
        fs::write(t.path().join("reference, set.json"),r#"[{"id":"study2026","type":"book","title":"Nested {literal} title"},{"id":"https://example.test/bib?name=work&year=2026","type":"webpage","title":"URL identifier"},{"id":"末尾.","type":"book","title":"Unicode evidence"}]"#).unwrap();
        let draft="---\nbibliography:\n  - 'reference, set.json'\n---\n[see @stu";
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"citation","study",60).unwrap();assert_eq!(v["items"][0]["insert"],"study2026");assert_eq!(v["items"][0]["detail"],"Nested {literal} title");
        let url="https://example.test/bib?name=work&year=2026";
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"citation","https",60).unwrap();assert_eq!(v["items"][0]["insert"],format!("{{{url}}}"));
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"citation","{https",60).unwrap();assert_eq!(v["items"][0]["insert"],url);
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"citation","末尾",60).unwrap();assert_eq!(v["items"][0]["insert"],"{末尾.}");
        fs::write(t.path().join("refs.bib"),"@string{series = {Shared Macro}}\n@book{entry,title=series,author={Hopper, Grace},year={1952}}").unwrap();
        let draft="---\nbibliography: refs.bib\n---\n@ent";
        let v=suggest_knowledge_links(t.path(),Some(&file),draft,"citation","ent",60).unwrap();assert_eq!(v["items"][0]["detail"],"Shared Macro");
        fs::write(t.path().join("bad.json"),r#"[{"id":"duplicate","title":"A"},{"id":"duplicate","title":"B"}]"#).unwrap();
        assert_eq!(suggest_knowledge_links(t.path(),Some(&file),"---\nbibliography: bad.json\n---","citation","",60).unwrap_err(),"academic_duplicate_reference");
        fs::write(t.path().join("bad.json"),"{not a bibliography}").unwrap();
        assert_eq!(suggest_knowledge_links(t.path(),Some(&file),"---\nbibliography: bad.json\n---","citation","",60).unwrap_err(),"academic_bibliography");
        let escaped="---\nbibliography: ../../outside.json\n---";
        assert!(suggest_knowledge_links(t.path(),Some(&file),escaped,"citation","",60).unwrap()["items"].as_array().unwrap().is_empty());
    }
}
