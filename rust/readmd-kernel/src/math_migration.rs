//! Read-only migration planning. Only the guarded save service materializes a
//! plan; opening, previewing and exporting a document never changes its source.
use crate::{document_transaction::{self as tx, ContentTransaction, TextEdit}, math_document::{self, XmlNode, XmlPart}};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::ops::Range;

const PREFIX: &str = "<!--readmd:math:v1:";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recovery {
    pub from: usize, pub to: usize, pub code: String, pub content: String,
    pub formula_id: String,
}
#[derive(Debug, Serialize)]
pub struct Migration {
    pub content: String, pub edits: Vec<TextEdit>, pub recoveries: Vec<Recovery>,
    pub diagnostics: Vec<serde_json::Value>, pub revision: String,
}
fn clean_xml(node: &mut XmlNode) {
    if !matches!(node.local(), "t" | "delText") {
        node.content.retain(|p| !matches!(p, XmlPart::Text(t) if t.trim().is_empty()));
    }
    for part in &mut node.content { if let XmlPart::Element(child) = part { clean_xml(child); } }
}
pub(crate) fn readable(xml: &str) -> Result<String, &'static str> {
    let mut original = math_document::parse_xml(xml)?;
    clean_xml(&mut original);
    let source = crate::math_dsl::decompile(&original)?;
    let (compiled, _) = crate::math_dsl::compile(&source, false)?;
    if math_document::xml_semantics(&original) != math_document::xml_semantics(&compiled) {
        return Err("math_migration_loss");
    }
    Ok(source)
}
/// Smallest contiguous edit with Unicode-safe boundaries. Used to acknowledge
/// composed asset + migration writes in the original editor coordinate space.
pub fn saved_edits(original: &str, saved: &str) -> Vec<TextEdit> {
    if original == saved { return vec![]; }
    let prefix = original.chars().zip(saved.chars()).take_while(|(a,b)| a == b).map(|(c,_)| c.len_utf8()).sum::<usize>();
    let suffix = original[prefix..].chars().rev().zip(saved[prefix..].chars().rev()).take_while(|(a,b)| a == b).map(|(c,_)| c.len_utf8()).sum::<usize>();
    vec![TextEdit {from:original[..prefix].encode_utf16().count(),to:original[..original.len()-suffix].encode_utf16().count(),insert:saved[prefix..saved.len()-suffix].into()}]
}
fn markdown(source: &str, display: bool, eol: &str) -> String {
    if display { format!("```readmd-math v1{eol}{source}{eol}```") }
    else { format!(":math[{source}]") }
}

/// Discover real math events, never examples, metadata, HTML, link destinations
/// or escaped dollars. Offset ranges refer to the exact original UTF-8 source.
fn formulas(source: &str) -> Vec<(Range<usize>, String, bool)> {
    let options = Options::ENABLE_MATH | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_FOOTNOTES;
    let mut out = Vec::new();
    let mut excluded = 0usize;
    let mut protected = Vec::new();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::HtmlBlock | Tag::Link {..} | Tag::Image {..}) => { excluded += 1; protected.push(range); },
            Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_) | TagEnd::HtmlBlock | TagEnd::Link | TagEnd::Image) => excluded = excluded.saturating_sub(1),
            Event::InlineMath(body) if excluded == 0 => out.push((range, body.into_string(), false)),
            Event::DisplayMath(body) if excluded == 0 => out.push((range, body.into_string(), true)),
            Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) => protected.push(range),
            _ => (),
        }
        if out.len() >= tx::MAX_OPERATIONS { break; }
    }
    protected.extend(out.iter().map(|(r,_,_)|r.clone()));
    protected.sort_by_key(|r|r.start);
    let mut i=0;let mut at=0;
    while i<source.len() && out.len()<tx::MAX_OPERATIONS {
        while at<protected.len() && protected[at].end<=i { at+=1; }
        if at<protected.len() && protected[at].start<=i { i=protected[at].end;continue; }
        if source[i..].starts_with("\\(") || source[i..].starts_with("\\[") {
            let display=source[i..].starts_with("\\[");let close=if display{"\\]"}else{"\\)"};let mut end=i+2;
            let limit=protected.get(at).map(|r|r.start).unwrap_or(source.len()).min(source.len());
            let mut found=false;
            while end<limit {
                if source[end..].starts_with(close) { out.push((i..end+2,source[i+2..end].into(),display));i=end+2;found=true;break; }
                if !display && source.as_bytes()[end]==b'\n' { break; }
                end+=if source.as_bytes()[end]==b'\\' { 1+source[end+1..].chars().next().map(char::len_utf8).unwrap_or(0) } else { source[end..].chars().next().unwrap().len_utf8() };
            }
            if found { continue; }
            i=end;
            if i>=source.len() { break; }
        }
        i+=if source.as_bytes()[i]==b'\\' { 1+source[i+1..].chars().next().map(char::len_utf8).unwrap_or(0) } else { source[i..].chars().next().unwrap().len_utf8() };
    }
    out.sort_by_key(|(r,_,_)|r.start);
    out
}
pub fn plan(source: &str) -> Result<Migration, &'static str> {
    if source.len() > tx::MAX_DOCUMENT_BYTES { return Err("transaction_budget"); }
    let revision = tx::revision(source);
    let mut edits = Vec::new(); let mut recoveries = Vec::new(); let mut diagnostics = Vec::new();
    if !source.contains(PREFIX) && !source.contains(crate::math_extension::PREFIX) { return Ok(Migration {content:source.into(), edits, recoveries, diagnostics, revision}); }
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    // The carrier is occurrence-bound. Duplicate IDs are ambiguous even when
    // two author expressions happen to be byte-for-byte equal.
    let mut carriers=Vec::new();let mut identities=std::collections::BTreeMap::<String,usize>::new();
    for range in crate::math_dsl::markdown_ranges(source)?{
        if let Some((comment_from,to,comment))=crate::math_extension::adjacent(source,range.to){
            if let Ok(payload)=crate::math_extension::decode(comment){*identities.entry(payload.id.clone()).or_default()+=1;}
            carriers.push((range,comment_from,to,comment));
        }
    }
    for (range,comment_from,to,comment) in carriers{
        let from=source[..range.from].encode_utf16().count();let comment_from_utf16=source[..comment_from].encode_utf16().count();let to_utf16=source[..to].encode_utf16().count();
        match crate::math_extension::decode(comment){
            Ok(payload)=>{
                let current=identities.get(&payload.id)==Some(&1)&&crate::math_extension::resolve(&source[range.body_from..range.body_to],comment,Some(&payload.id),range.display).is_ok();
                if !current{
                    recoveries.push(Recovery{from,to:to_utf16,code:"math_metadata_stale".into(),content:source[range.from..to].into(),formula_id:payload.id});
                    diagnostics.push(serde_json::json!({"code":"math_metadata_stale","from":comment_from_utf16,"to":to_utf16}));
                    edits.push(TextEdit{from:comment_from_utf16,to:to_utf16,insert:String::new()});
                }
            },
            Err(code)=>diagnostics.push(serde_json::json!({"code":code,"from":comment_from_utf16,"to":to_utf16})),
        }
    }
    for (range, tex, display) in formulas(source) {
        let tail = &source[range.end..];
        let gap = tail.len() - tail.trim_start_matches(char::is_whitespace).len();
        let tail = &tail[gap..];
        if !tail.starts_with(PREFIX) { continue; }
        let Some(end) = tail.find("-->").map(|n| n + 3) else { continue; };
        let comment = &tail[..end];
        let from = source[..range.start].encode_utf16().count();
        let comment_from = source[..range.end + gap].encode_utf16().count();
        let to = comment_from + comment.encode_utf16().count();
        let (metadata, current) = match math_document::read_metadata(comment, &tex) {
            Ok(value) => value,
            Err(code) => { diagnostics.push(serde_json::json!({"code":code,"from":comment_from,"to":to})); continue; }
        };
        let current = current || metadata.source_revision == tx::revision(tex.trim());
        let dsl = match readable(&metadata.omml){
            Ok(source)=>Ok(crate::math_extension::Projection{source,metadata:None,preserved:0}),
            Err("math_unsupported_extension")=>{
                // Legacy IDs were not required to be unique. Establish a fresh
                // occurrence identity when migrating an opaque extension.
                let id=format!("ext-{}-{from}",&tx::revision(&metadata.id)[..16]);
                crate::math_extension::capture(&metadata.omml,&id,display)
            },
            Err(code)=>Err(code),
        };
        if !current {
            let recovery = dsl.as_ref().map(|s| markdown(&s.source, display, eol)+&s.metadata.as_ref().map(|m|format!("{}{m}",if display{eol}else{""})).unwrap_or_default())
                .unwrap_or_else(|_| source[range.clone()].to_string() + &source[range.end..range.end + gap + end]);
            recoveries.push(Recovery {from,to,code:"math_metadata_stale".into(),content:recovery,formula_id:metadata.id});
            diagnostics.push(serde_json::json!({"code":"math_metadata_stale","from":comment_from,"to":to}));
            // Detach only after the save service has persisted the old properties.
            // The new author text is authoritative, never the stale Office XML.
            edits.push(TextEdit {from:comment_from,to,insert:String::new()});
            continue;
        }
        match dsl {
            Ok(dsl) => {
                let mut insert = markdown(&dsl.source, display, eol);
                if let Some(carrier)=dsl.metadata{if display{insert.push_str(eol);}insert.push_str(&carrier);}
                if display {
                    // A display expression embedded in a prose line still needs
                    // complete block boundaries for a valid Markdown fence.
                    let prefix = source[..range.start].rsplit('\n').next().unwrap_or("");
                    static CONTAINER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
                    let container = CONTAINER.get_or_init(||regex::Regex::new(r"^(?: {0,3}> ?)*[ \t]*(?:(?:[-+*]|[0-9]+[.)])[ \t]+)?$").unwrap());
                    static NOTE_START: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
                    let note_start = NOTE_START.get_or_init(||regex::Regex::new(r"^((?: {0,3}> ?)*[ \t]*)\[\^[^\]\r\n]+\]:[ \t]*$").unwrap());
                    if let Some(note) = note_start.captures(prefix) {
                        let continuation=note[1].chars().map(|c|if c=='>'{c}else{' '}).collect::<String>()+"    ";
                        insert=insert.replace(eol,&format!("{eol}{continuation}"));
                    } else if container.is_match(prefix) {
                        let continuation=prefix.chars().map(|c|if c=='>'{c}else{' '}).collect::<String>();
                        insert=insert.replace(eol,&format!("{eol}{continuation}"));
                    } else {
                        // Prose containing a display formula is separated by a
                        // true block boundary. Container/prose mixtures stay
                        // lossless until their complete prefix can be retained.
                        if prefix.contains('>') { diagnostics.push(serde_json::json!({"code":"math_migration_loss","from":comment_from,"to":to}));continue; }
                        insert.insert_str(0, &format!("{eol}{eol}"));
                    }
                    if source[range.end + gap + end..].chars().next().is_some_and(|c| c != '\r' && c != '\n') { insert.push_str(&format!("{eol}{eol}")); }
                }
                edits.push(TextEdit {from,to,insert});
            }
            Err(code) => diagnostics.push(serde_json::json!({"code":code,"from":comment_from,"to":to})),
        }
    }
    let result = tx::apply(source, &ContentTransaction {base_revision:revision.clone(),origin:"formula".into(),edits:edits.clone()})?;
    Ok(Migration {content:result.content,edits,recoveries,diagnostics,revision})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rich(tex: &str) -> String {
        let xml = format!(r#"<m:oMath xmlns:m="{}"><m:f><m:fPr><m:type m:val="skw"/></m:fPr><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>"#, math_document::MATH_NS);
        math_document::metadata(tex, &xml, "eq-one").unwrap()
    }
    #[test]
    fn migrates_only_current_metadata_and_is_idempotent_invertible() {
        let source = format!("😀 Result $a/b${}.\r\n\r\n$$a/b$${}\r\n", rich("a/b"), rich("a/b"));
        let p = plan(&source).unwrap();
        assert_eq!(p.edits.len(),2); assert!(p.recoveries.is_empty()); assert!(p.diagnostics.is_empty(),"{:?}",p.diagnostics);
        assert!(!p.content.contains(PREFIX)); assert!(p.content.contains(":math[")); assert!(p.content.contains("```readmd-math v1\r\n"));
        assert!(p.content.contains("office.type(val: \"skw\")"));
        assert_eq!(plan(&p.content).unwrap().content,p.content);
        let tx = ContentTransaction {base_revision:p.revision,origin:"formula".into(),edits:p.edits};
        let result = tx::apply(&source,&tx).unwrap();
        assert_eq!(tx::apply(&result.content,&ContentTransaction {base_revision:result.revision,origin:"formula".into(),edits:result.inverse}).unwrap().content,source);
    }
    #[test]
    fn stale_properties_never_overwrite_new_source_and_have_readable_recovery() {
        let source = format!("$z+1${}",rich("a/b")); let p=plan(&source).unwrap();
        assert_eq!(p.content,"$z+1$"); assert_eq!(p.recoveries.len(),1);
        assert!(p.recoveries[0].content.contains("skw")); assert_eq!(p.recoveries[0].code,"math_metadata_stale");
    }
    #[test]
    fn fenced_inline_indented_html_and_yaml_examples_are_unchanged() {
        let example=format!("$a/b${}",rich("a/b"));
        for source in [format!("```markdown\n{example}\n```"),format!("`{example}`"),format!("    {example}"),format!("<pre>{example}</pre>"),format!("---\nexample: '{example}'\n---\n# Note")] {
            assert_eq!(plan(&source).unwrap().content,source);
        }
    }
    #[test]
    fn invalid_and_unknown_properties_stay_recoverable_and_do_not_block_plain_save() {
        let source="$x$<!--readmd:math:v1:invalid-->";
        assert_eq!(plan(source).unwrap().content,source);
        assert_eq!(plan(source).unwrap().diagnostics.len(),1);
    }
    #[test]
    fn missing_character_attributes_remain_lossless_without_relaxing_the_pinned_schema() {
        // Office's rendering defaults describe these malformed/legacy inputs,
        // but the pinned Open XML schema requires an explicit m:val. Render
        // them tolerantly; never migrate them into invalid new DSL/Word output.
        for properties in ["<m:begChr/>","<m:sepChr/>","<m:endChr/>"] {
            let xml=format!("<m:oMath><m:d><m:dPr>{properties}</m:dPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:d></m:oMath>");
            assert_eq!(readable(&xml).unwrap_err(),"math_invalid_property");
            let metadata=math_document::metadata("x",&xml,"legacy-char").unwrap();
            let source=format!("$x${metadata}");let result=plan(&source).unwrap();
            assert_eq!(result.content,source);
            assert!(result.edits.is_empty());
            assert_eq!(result.diagnostics.len(),1);
            assert_eq!(result.diagnostics[0]["code"],"math_invalid_property");
            let stale=format!("$new${metadata}");let recovery=plan(&stale).unwrap();
            assert_eq!(recovery.content,"$new$");
            assert_eq!(recovery.recoveries.len(),1);
            assert_eq!(recovery.recoveries[0].content,stale);
        }
        for source in ["delimiter(x, properties: office.dPr(office.endChr()))","nary(x, from: empty(), to: empty(), properties: office.naryPr(office.chr()))"] {
            assert_eq!(crate::math_dsl::compile(source,false).unwrap_err(),"math_invalid_property");
        }
    }
    #[test]
    fn compatibility_delimiters_and_nested_display_containers_keep_their_scope() {
        let source=format!("\\(a/b\\){}\n\n> $$a/b$${}\n\n- $$a/b$${}\n",rich("a/b"),rich("a/b"),rich("a/b"));
        let plan=plan(&source).unwrap();assert_eq!(plan.edits.len(),3);
        let doc=crate::md_ast::parse(&plan.content);
        assert!(matches!(&doc.blocks[1],crate::md_ast::Block::Quote(blocks) if matches!(&blocks[0],crate::md_ast::Block::OfficeMath{..})),"{}",plan.content);
        assert!(matches!(&doc.blocks[2],crate::md_ast::Block::List{..}),"{}",plan.content);
        let example=format!("`\\(a/b\\){}`",rich("a/b"));assert_eq!(super::plan(&example).unwrap().content,example);
    }

    #[test]
    fn footnote_math_migration_keeps_definition_scope_and_code_examples() {
        for (formula,display) in [("$a/b$",false),("$$a/b$$",true)] {
            for first in [false,true] {
                let body=if first {format!("[^evidence]: {formula}{}\r\n",rich("a/b"))}else{format!("[^evidence]: Author evidence.\r\n\r\n    {formula}{}\r\n",rich("a/b"))};
                let source=format!("Claim[^evidence].\r\n\r\n{body}");
                let migrated=plan(&source).unwrap();assert_eq!(migrated.edits.len(),1,"{source}");
                assert!(migrated.diagnostics.is_empty(),"{:?}",migrated.diagnostics);
                assert!(!migrated.content.contains(PREFIX));
                assert_eq!(plan(&migrated.content).unwrap().content,migrated.content);
                let document=crate::md_ast::parse(&migrated.content);
                let notes=document.blocks.iter().filter_map(|b|if let crate::md_ast::Block::FootnoteDef{blocks,..}=b{Some(blocks)}else{None}).collect::<Vec<_>>();
                assert_eq!(notes.len(),1,"{}",migrated.content);
                if display {assert!(notes[0].iter().any(|b|matches!(b,crate::md_ast::Block::OfficeMath{..})),"{}",migrated.content);}
                else {let mut formulas=0;crate::md_ast::walk_inlines(notes[0],&mut |inline|if matches!(inline,crate::md_ast::Inline::OfficeMath{..}){formulas+=1;});assert_eq!(formulas,1,"{}",migrated.content);}
                assert!(!document.blocks.iter().any(|b|matches!(b,crate::md_ast::Block::OfficeMath{..})),"{}",migrated.content);
            }
        }
        let code=format!("Claim[^evidence].\n\n[^evidence]: Example.\n\n    ```markdown\n    $a/b${}\n    ```\n",rich("a/b"));
        assert_eq!(plan(&code).unwrap().content,code);
    }
}
