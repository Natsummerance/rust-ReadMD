//! Bounded extension edits share the metadata parser, but deliberately do not
//! use bibliographic empty-value normalization: 0, false and "" are values.
use super::*;

pub(crate) fn bounded_source_member(content: &str, keys: &[&str]) -> Result<Option<(Value, usize, usize)>> {
    let Some(fm) = front(content)? else { return Ok(None) };
    bounded_front_matter_member(fm.body, keys).map(|found| found.map(|(v, from, to)| {
        let base = utf16(content, fm.body_from); (v, base + from, base + to)
    })).map_err(|mut error| { let base = utf16(content, fm.body_from); error.from += base; error.to += base; error })
}

fn cleaned(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(values.iter().filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.clone(), cleaned(v))).collect()),
        other => other.clone(),
    }
}
fn has_value(value: &Value) -> bool {
    match value { Value::Null => false, Value::Object(values) => values.values().any(has_value), _ => true }
}
fn block_member(key:&str,value:&Value,indent:usize,eol:&str)->String {
    let reserved = matches!(key.to_ascii_lowercase().as_str(),"null"|"true"|"false"|"yes"|"no"|"on"|"off"|"nan"|"inf");
    let key = if !reserved && !key.is_empty() && key.bytes().all(|c|c.is_ascii_alphabetic()||matches!(c,b'_'|b'-')) {key.to_string()} else {yaml(&json!(key))};
    let pad=" ".repeat(indent);
    if let Some(values)=value.as_object().filter(|values|!values.is_empty()) {
        format!("{pad}{key}:{eol}{}",values.iter().map(|(k,v)|block_member(k,v,indent+2,eol)).collect::<String>())
    } else {format!("{pad}{key}: {}{eol}",yaml(value))}
}
fn insert_extension(source:&str,node:&Node,values:&[(String,Value)],eol:&str,edits:&mut Vec<ByteEdit>){
    if values.is_empty(){return;}
    if node.flow {object_insert(source,node,values,eol,edits);return;}
    let Kind::Object(entries)=&node.kind else{return};
    let column=entries.first().map(|e|e.key_start-line_start(source,e.key_start)).unwrap_or(0);
    let at=if entries.is_empty(){source.len()}else{line_end(source,node.end)};
    let prefix=if at>0&&!source[..at].ends_with('\n'){eol}else{""};
    edits.push(ByteEdit{from:at,to:at,insert:format!("{prefix}{}",values.iter().map(|(k,v)|block_member(k,v,column,eol)).collect::<String>())});
}

fn trivia_without_commas(source: &str) -> String {
    let mut comment = false;
    source.chars().filter(|c| {
        if matches!(c, '\n' | '\r') { comment = false; }
        else if *c == '#' { comment = true; }
        *c != ',' || comment
    }).collect()
}

fn patch_extension(source: &str, node: &Node, patch: &Value, eol: &str, edits: &mut Vec<ByteEdit>) -> Result<()> {
    if patch.as_object().is_some_and(Map::is_empty) { return Ok(()) }
    let (Kind::Object(entries), Value::Object(values)) = (&node.kind, patch) else {
        let value = cleaned(patch);
        // Comparison is lexical-safe: a request for a number deliberately
        // changes a quoted numeric scalar to an actual numeric scalar.
        if node.value != value { edits.push(ByteEdit { from: node.start, to: node.end, insert: yaml(&value) }); }
        return Ok(());
    };
    if values.is_empty() { return Ok(()) }
    if node.flow && values.iter().any(|(k, v)| v.is_null() && entries.iter().any(|e| &e.key == k)) {
        // Rebuild only this flow mapping, retaining every surviving lexical
        // entry and every gap/comment. Independent comma deletion overlaps
        // when two adjacent members are removed together.
        let mut output = String::from("{");
        let first = entries.first().map_or(node.end - 1, |e| e.key_start);
        output.push_str(&trivia_without_commas(&source[node.start + 1..first]));
        let additions = values.iter().filter(|(k, v)| has_value(v) && !entries.iter().any(|e| &e.key == *k)).collect::<Vec<_>>();
        for (index, entry) in entries.iter().enumerate() {
            let survives = !values.get(&entry.key).is_some_and(Value::is_null);
            if survives {
                let mut local = Vec::new();
                if let Some(value) = values.get(&entry.key) { patch_extension(source, &entry.node, value, eol, &mut local)?; }
                output.push_str(&apply_bytes(source, &local, entry.key_start, entry.node.end)?);
                if !additions.is_empty() || entries[index + 1..].iter().any(|e| !values.get(&e.key).is_some_and(Value::is_null)) { output.push(','); }
            }
            let next = entries.get(index + 1).map_or(node.end - 1, |e| e.key_start);
            output.push_str(&trivia_without_commas(&source[entry.node.end..next]));
        }
        for (index, (key, value)) in additions.iter().enumerate() {
            if index > 0 { output.push_str(", "); }
            output.push_str(&format!("{}: {}", yaml(&json!(key)), yaml(&cleaned(value))));
        }
        output.push('}');
        edits.push(ByteEdit { from: node.start, to: node.end, insert: output });
        return Ok(());
    }
    let mut additions = Vec::new();
    for (key, value) in values {
        if let Some(entry) = entries.iter().find(|e| &e.key == key) {
            if value.is_null() {
                let from = line_start(source, entry.key_start);
                if !source[from..entry.key_start].trim().is_empty() { return Err(err("publication_metadata_yaml_feature", key)); }
                let to = line_end(source, entry.node.end);
                let tail = &source[entry.node.end..to];
                let insert = if tail.trim_start().starts_with('#') {
                    format!("{}{}", &source[from..entry.key_start], tail.trim_start_matches([' ', '\t']))
                } else { String::new() };
                edits.push(ByteEdit { from, to, insert });
            } else { patch_extension(source, &entry.node, value, eol, edits)?; }
        } else if has_value(value) { additions.push((key.clone(), cleaned(value))); }
    }
    insert_extension(source,node,&additions,eol,edits);
    Ok(())
}

/// Patch exactly one extension subtree. Null removes a member; empty mappings
/// are no-ops. Validation of that extension's public schema remains its owner.
pub(crate) fn patch_source_member(content: &str, base_revision: &str, keys: &[&str], patch: &Value) -> Result<ContentTransaction> {
    if keys.is_empty() || keys.len() > MAX_DEPTH { return Err(err("publication_metadata_field", "patch")); }
    if base_revision != transaction::revision(content) { return Err(err("transaction_conflict", "base_revision")); }
    if serde_json::to_vec(patch).map_err(|_| err("publication_metadata_type", "patch"))?.len() > MAX_FRONT_MATTER {
        return Err(err("publication_metadata_budget", "patch"));
    }
    let mut wrapped = patch.clone();
    for key in keys.iter().rev() { wrapped = json!({*key:wrapped}); }
    let fm = front(content)?;
    let mut edits = Vec::new();
    if let Some(fm) = &fm {
        let root = parse_yaml(fm.body)?;
        let mut current=&root;
        for (index,key) in keys.iter().enumerate() {
            let Kind::Object(entries)=&current.kind else {break};
            let Some(entry)=entries.iter().find(|e|e.key==*key) else {break};
            if !entry.node.value.is_null() && !matches!(entry.node.kind,Kind::Object(_))
                && (index+1<keys.len()||patch.is_object()) {
                return Err(err("publication_metadata_type", &keys[..=index].join(".")));
            }
            current=&entry.node;
        }
        patch_extension(fm.body, &root, &wrapped, fm.eol, &mut edits)?;
        for edit in &mut edits { edit.from += fm.body_from; edit.to += fm.body_from; }
    } else if has_value(patch) {
        let eol = if content.contains("\r\n") { "\r\n" } else { "\n" };
        let at = if content.starts_with('\u{feff}') { 3 } else { 0 };
        edits.push(ByteEdit { from: at, to: at, insert: format!("---{eol}{}---{eol}", cleaned(&wrapped).as_object().unwrap().iter()
            .map(|(k,v)| block_member(k,v,0,eol)).collect::<String>()) });
    }
    edits.sort_by_key(|e| (e.from, e.to));
    let mut combined: Vec<ByteEdit> = Vec::new();
    for edit in edits {
        if let Some(last) = combined.last_mut().filter(|e| e.from == edit.from && e.to == edit.to && edit.from == edit.to) { last.insert.push_str(&edit.insert); }
        else { combined.push(edit); }
    }
    let tx = ContentTransaction { base_revision: base_revision.into(), origin: "editor".into(), edits: combined.into_iter()
        .map(|e| TextEdit { from: utf16(content, e.from), to: utf16(content, e.to), insert: e.insert }).collect() };
    let applied = transaction::apply(content, &tx).map_err(|code| err(code, "patch"))?;
    // Validate the actual patched YAML too. This also catches syntactically
    // incompatible external flow trivia before returning a source transaction.
    if let Some(fm) = front(&applied.content)? { parse_yaml(fm.body)?; }
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn patch(source: &str, value: Value) -> transaction::TransactionResult {
        let tx = patch_source_member(source, &transaction::revision(source), &["readmd", "publication"], &value).unwrap();
        let result = transaction::apply(source, &tx).unwrap();
        let inverse = ContentTransaction { base_revision: result.revision.clone(), origin: "editor".into(), edits: result.inverse.clone() };
        assert_eq!(transaction::apply(&result.content, &inverse).unwrap().content, source);
        result
    }
    #[test] fn preserves_bom_crlf_unknown_comments_and_falsy_values() {
        let source = "\u{feff}---\r\nisbn: '00123' # adjacent\r\nreadmd:\r\n  other: literal\r\n  publication:\r\n    version: 1\r\n    footnotes:\r\n      start: 4 # retained comment\r\n      untouched: '001'\r\n---\r\n# 𝛂\r\n";
        let result = patch(source, json!({"footnotes":{"start":null,"longNote":{"keepParagraphs":false},"size":0,"continuationNotice":""}}));
        assert!(result.content.contains("isbn: '00123' # adjacent\r\n"));
        assert!(result.content.contains("  other: literal\r\n"));
        assert!(result.content.contains("# retained comment\r\n"));
        assert!(result.content.contains("untouched: '001'\r\n"));
        let v = bounded_source_member(&result.content, &["readmd","publication","footnotes"]).unwrap().unwrap().0;
        assert!(v.get("start").is_none()); assert_eq!(v["longNote"]["keepParagraphs"], false);
        assert_eq!(v["size"], "0"); assert_eq!(v["continuationNotice"], "");
    }
    #[test] fn flow_deletions_preserve_surviving_lexemes_and_comments() {
        let source = "---\nreadmd: {other: '001', publication: {version: 1, footnotes: {start: 4, size: 9, # keep me\n unknown: '007', restart: continuous}}}\n---\nText\n";
        let result = patch(source, json!({"footnotes":{"start":null,"size":null,"restart":"page"}}));
        assert!(result.content.contains("other: '001'")); assert!(result.content.contains("# keep me"));
        assert!(result.content.contains("unknown: '007'"));
        let v = bounded_source_member(&result.content, &["readmd","publication","footnotes"]).unwrap().unwrap().0;
        assert!(v.get("start").is_none()); assert!(v.get("size").is_none()); assert_eq!(v["restart"], "page");
    }
    #[test] fn empty_patch_is_noop_and_stale_patch_is_rejected() {
        let source = "# Text\n";
        // An entirely absent extension needs no wrapper for an empty patch.
        assert!(!patch(source, json!({})).changed);
        assert_eq!(patch_source_member(source, "stale", &["readmd","publication"], &json!({})).unwrap_err().code, "transaction_conflict");
    }
    #[test] fn reserved_label_keys_remain_strings_and_nonmapping_namespaces_are_not_destroyed() {
        let source="# Notes\n";
        let result=patch(source,json!({"footnotes":{"kindByLabel":{"null":"endnote","Null":"footnote","NULL":"endnote","on":"footnote"}}}));
        let value=bounded_source_member(&result.content,&["readmd","publication","footnotes","kindByLabel"]).unwrap().unwrap().0;
        assert_eq!(value["null"],"endnote");assert_eq!(value["Null"],"footnote");assert_eq!(value["NULL"],"endnote");assert_eq!(value["on"],"footnote");
        for source in ["---\nreadmd: KEEP_ME\n---\nText", "---\nreadmd: {publication: KEEP_ME, other: 7}\n---\nText"] {
            assert_eq!(patch_source_member(source,&transaction::revision(source),&["readmd","publication"],&json!({"footnotes":{"start":4}})).unwrap_err().code,"publication_metadata_type");
        }
    }
    #[test] fn flow_trailing_commas_remain_valid_when_adding_a_member() {
        for mapping in ["{version: 1,}","{version: 1, # keep comma, in comment\n}","{version: 1 # comment, not separator\n}"] {
            let source=format!("---\nreadmd: {{publication: {mapping}}}\n---\nText\n");
            let result=patch(&source,json!({"footnotes":{"start":4}}));
            let value=bounded_source_member(&result.content,&["readmd","publication"]).unwrap().unwrap().0;
            assert_eq!(value["footnotes"]["start"],"4");
            if mapping.contains("# "){assert!(result.content.contains("# "));}
        }
    }
}
