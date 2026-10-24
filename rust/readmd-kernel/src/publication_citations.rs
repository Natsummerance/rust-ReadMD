//! Source citation syntax, independent of document writers and CSL output.
use crate::academic::{Cite, CiteContext, CiteMode};
use crate::publication_rich::{self, RichNode, SourceCitation, SourceItem, SourceSpan};
use crate::{ApiError, ApiResult};
use pulldown_cmark::{Event, Options, Parser, Tag};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;

pub struct Scan { pub citations: Vec<SourceCitation>, pub diagnostics: Vec<Value> }
fn error(field: &str) -> ApiError { ApiError::new(422, "academic_context").noted("field", field) }
fn options() -> Options { Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_MATH | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_STRIKETHROUGH }
type SourceEvents<'a> = Box<dyn Iterator<Item=(Event<'a>,Range<usize>)>+'a>;
/// Author scopes are source syntax only after the shared parser validates
/// their exact delimiters. Keep ordinary HTML opaque, including an ordinary
/// span nested inside a validated author scope. The legacy path adds no author
/// parsing budget and keeps its original parser options and source offsets.
fn source_events(source:&str)->ApiResult<(SourceEvents<'_>,HashSet<(usize,usize)>)>{
    if !crate::publication_inline_text::has_owned_syntax(source){
        return Ok((Box::new(Parser::new_ext(source,options()).into_offset_iter()),HashSet::new()));
    }
    let document=crate::publication_inline_text::consumer_document(source).map_err(|code|ApiError::new(422,code))?;
    let edges=document.nodes.into_iter().flat_map(|node|[node.opening,node.closing].into_iter().flatten())
        .map(|range|(range.start,range.end)).collect();
    let events=crate::publication_inline_text::markdown_events(source).map_err(|code|ApiError::new(422,code))?;
    Ok((Box::new(events.into_iter()),edges))
}
fn escaped(source: &str, at: usize) -> bool { source[..at].bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1 }
fn trim(source: &str, range: Range<usize>) -> Range<usize> {
    let text = &source[range.clone()]; let start = range.start + text.len() - text.trim_start().len();
    start..start + text.trim().len()
}
fn enclosed(source: &str, at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize; let mut braces = 0usize;
    for (offset, c) in source[at..].char_indices() {
        let index = at + offset;
        if index - at > 65536 || source[index..].starts_with("\n\n") || source[index..].starts_with("\r\n\r\n") { return None; }
        if escaped(source, index) { continue; }
        if open == '[' {
            if c == '{' { braces += 1; if braces > 16 { return None; } }
            else if c == '}' { braces = braces.saturating_sub(1); }
            if braces > 0 || c == '}' { continue; }
        }
        if c == open { depth += 1; if depth > 16 { return None; } }
        else if c == close { depth = depth.checked_sub(1)?; if depth == 0 { return Some(index); } }
    }
    None
}
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start,range.end));
    let mut out: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = out.last_mut().filter(|last| last.end >= range.start) { last.end=last.end.max(range.end); }
        else { out.push(range); }
    }
    out
}
fn covering(ranges: &[Range<usize>], at: usize) -> Option<&Range<usize>> {
    ranges.get(ranges.partition_point(|range| range.end <= at)).filter(|range| range.contains(&at))
}
fn overlaps(ranges: &[Range<usize>], range: Range<usize>) -> bool {
    ranges.get(ranges.partition_point(|candidate| candidate.end <= range.start)).is_some_and(|candidate| candidate.start < range.end)
}
fn field(error: &ApiError) -> &str {
    match &error.shape {
        crate::error::ErrorShape::ApiCode { extras, .. } => extras.iter().find(|(key,_)| key == "field").and_then(|(_,value)| value.as_str()).unwrap_or("citation"),
        _ => "citation",
    }
}
fn boundary(source: &str, at: usize) -> bool {
    source[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && !"_@/\\".contains(c))
}
fn key(source: &str, at: usize) -> ApiResult<(String, Range<usize>, usize)> {
    let start = at + 1;
    if source[start..].starts_with('{') {
        let end = enclosed(source, start, '{', '}').ok_or_else(|| error("key"))?;
        let value = source[start+1..end].to_string();
        if value.is_empty() || value.len() > 1024 { return Err(error("key")); }
        return Ok((value, start..end+1, end+1));
    }
    let mut end = start; let mut word = false;
    for (offset, c) in source[start..].char_indices() {
        let index = start + offset;
        if c.is_alphanumeric() || c == '_' { end = index + c.len_utf8(); word = true; }
        else if word && ":.#$%&-+?<>~/".contains(c)
            && source[index+c.len_utf8()..].chars().next().is_some_and(|next| next.is_alphanumeric() || next == '_') {
            end = index + c.len_utf8(); word = false;
        } else { break; }
        if end - start > 1024 { return Err(error("key")); }
    }
    if end == start { return Err(error("key")); }
    Ok((source[start..end].into(), start..end, end))
}
fn at_sign(source: &str, range: Range<usize>) -> Option<usize> {
    let mut braces = 0usize; let mut brackets = 0usize;
    for (offset, c) in source[range.clone()].char_indices() {
        let index = range.start + offset;
        if escaped(source, index) { continue; }
        match c {
            '{' => braces += 1, '}' => braces = braces.saturating_sub(1),
            '[' => brackets += 1, ']' => brackets = brackets.saturating_sub(1),
            '@' if braces == 0 && brackets == 0 => {
                let start = if index > range.start && source.as_bytes()[index-1] == b'-' { index-1 } else { index };
                if boundary(source, start) { return Some(index); }
            }
            _ => {}
        }
    }
    None
}
// An HTML character reference is literal affix content, not a citation item
// separator. Use the same Markdown decoder as the rich-affix parser, without
// decoding the source itself or changing its byte/UTF-16 coordinates.
fn entity_terminator(source: &str, start: usize, semicolon: usize) -> bool {
    let mut floor=semicolon.saturating_sub(64).max(start);
    while !source.is_char_boundary(floor){floor+=1;}
    let Some(offset)=source[floor..semicolon].rfind('&')else{return false;};
    let amp=floor+offset;
    if escaped(source,amp){return false;}
    let token=&source[amp+1..semicolon];
    if token.is_empty()||!token.bytes().all(|byte|byte.is_ascii_alphanumeric()||byte==b'#'){return false;}
    let raw=&source[amp..semicolon+1];let mut parser=Parser::new(raw);
    matches!(parser.next(),Some(Event::Start(Tag::Paragraph)))
        &&matches!(parser.next(),Some(Event::Text(value)) if value.as_ref()!=raw)
        &&matches!(parser.next(),Some(Event::End(pulldown_cmark::TagEnd::Paragraph)))
        &&parser.next().is_none()
}
fn parts(source: &str, range: Range<usize>) -> ApiResult<Vec<Range<usize>>> {
    let mut stack = Vec::new(); let mut start = range.start; let mut out = Vec::new();
    for (offset, c) in source[range.clone()].char_indices() {
        let index = range.start + offset;
        if escaped(source, index) { continue; }
        match c {
            '{' | '[' | '(' => { stack.push(c); if stack.len() > 16 { return Err(error("citation")); } },
            '}' | ']' | ')' => { let expected = match c { '}' => '{', ']' => '[', _ => '(' }; if stack.pop() != Some(expected) { return Err(error("citation")); } },
            ';' if stack.is_empty()&&!entity_terminator(source,start,index) => { out.push(trim(source, start..index)); start = index+1; },
            _ => {}
        }
    }
    if !stack.is_empty() { return Err(error("citation")); }
    out.push(trim(source, start..range.end));
    if out.len() > 100 || out.iter().any(Range::is_empty) { return Err(error("citation")); }
    Ok(out)
}

fn locator_terms(locale: &str) -> Vec<(String, String)> {
    use hayagriva::citationberg::taxonomy::Term;
    let language = locale.split('-').next().unwrap_or("en");
    let mut terms = Vec::new();
    for file in hayagriva::archive::locales() {
        if !file.lang.as_ref().is_some_and(|code| code.0 == locale || code.0.split('-').next() == Some(language) || code.0 == "en-US") { continue; }
        for term in file.terms.into_iter().flat_map(|terms| terms.terms) {
            if let Term::Locator(kind) = term.name {
                let label = serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(str::to_owned));
                if let Some(label) = label { for value in [term.single(), term.multiple()].into_iter().flatten() {
                    if !value.trim().is_empty() { terms.push((value.trim().to_string(), label.clone())); }
                }}
            }
        }
    }
    // Conventional abbreviations not present in every locale archive.
    for (value, label) in [("pp.","page"),("p.","page"),("chap.","chapter"),("ch.","chapter"),("sec.","section"),("§§","section"),("§","section"),("para.","paragraph"),("¶¶","paragraph"),("¶","paragraph"),("vol.","volume"),("fig.","figure"),("table","table")] {
        terms.push((value.into(),label.into()));
    }
    terms.sort_by(|a,b| b.0.len().cmp(&a.0.len()).then(a.cmp(b))); terms.dedup(); terms
}
fn term_prefix<'a>(value: &str, terms: &'a [(String,String)]) -> Option<(usize, &'a str)> {
    for (term, kind) in terms {
        if value.get(..term.len()).is_some_and(|prefix| prefix.to_lowercase() == term.to_lowercase()) {
            let rest = &value[term.len()..];
            if rest.chars().next().is_none_or(|c| c.is_whitespace() || c.is_ascii_digit() || term.ends_with('.') || !term.chars().last().unwrap().is_alphanumeric()) { return Some((term.len(),kind)); }
        }
    }
    None
}
fn locator_token(token: &str) -> bool {
    let token = token.trim_matches(|c:char| ",.;:()[]".contains(c));
    !token.is_empty() && (token.chars().any(|c| c.is_ascii_digit())
        || token.chars().filter(|c| c.is_alphabetic()).all(|c| "ivxlcdmIVXLCDM".contains(c)) && token.chars().any(|c| c.is_alphabetic()))
}
fn affix(source: &str, range: Range<usize>) -> ApiResult<Option<(String, Vec<RichNode>)>> {
    if range.is_empty() { return Ok(None); }
    let raw = &source[range]; if raw.trim().is_empty() { return Ok(None); }
    let mut rich = publication_rich::markdown_affix(raw.trim())?;
    if raw.starts_with(char::is_whitespace) { rich.insert(0, RichNode::text(" ")); }
    if raw.ends_with(char::is_whitespace) { rich.push(RichNode::text(" ")); }
    Ok(Some((raw.into(), rich)))
}
fn tail(source: &str, range: Range<usize>, terms: &[(String,String)], cite: &mut Cite, fields: &mut BTreeMap<String,SourceSpan>, comma_required: bool) -> ApiResult<()> {
    let mut start = range.start;
    while start < range.end && source[start..].starts_with(char::is_whitespace) { start += source[start..].chars().next().unwrap().len_utf8(); }
    if start == range.end { return Ok(()); }
    let forced = source[start..].starts_with('{');
    if !forced && source[start..].starts_with(',') { start += 1; while start < range.end && source[start..].starts_with(char::is_whitespace) { start += source[start..].chars().next().unwrap().len_utf8(); } }
    else if !forced && comma_required {
        if let Some((raw,rich)) = affix(source, range.clone())? { cite.suffix=Some(raw);cite.suffix_rich=Some(rich);fields.insert("suffix".into(),SourceSpan::new(source,range)?); }
        return Ok(());
    }
    let value_start = start;
    let forced_end = if source[start..range.end].starts_with('{') { Some(enclosed(source,start,'{','}').filter(|end|*end<range.end).ok_or_else(||error("locator"))?) } else { None };
    let forced_range = forced_end.map(|end| start+1..end);
    let content = forced_range.clone().unwrap_or(start..range.end);
    let content = trim(source,content);
    let value = &source[content.clone()];
    let term = term_prefix(value,terms);
    let mut locator_start = content.start;
    if let Some((length,kind)) = term {
        cite.kind=Some(kind.into()); fields.insert("kind".into(),SourceSpan::new(source,content.start..content.start+length)?);
        locator_start += length;
        while locator_start < content.end && source[locator_start..].starts_with(char::is_whitespace) { locator_start += source[locator_start..].chars().next().unwrap().len_utf8(); }
    }
    let mut locator_end = locator_start;
    if forced_end.is_some() { locator_end=content.end; }
    else {
        let mut first=true;
        for token in source[locator_start..content.end].split_inclusive(char::is_whitespace) {
            if locator_token(token.trim()) || first && term.is_some() && !token.trim().is_empty() { locator_end += token.len(); first=false; }
            else { break; }
        }
        while locator_end > locator_start && source[..locator_end].chars().next_back().is_some_and(|c| c.is_whitespace() || c==',') { locator_end -= source[..locator_end].chars().next_back().unwrap().len_utf8(); }
    }
    if locator_end > locator_start {
        cite.locator=Some(source[locator_start..locator_end].into());
        fields.insert("locator".into(),SourceSpan::new(source,locator_start..locator_end)?);
    } else if term.is_some() && forced_end.is_none() { return Err(error("locator")); }
    let suffix_start = if let Some(end)=forced_end {end+1} else if cite.locator.is_some() {locator_end} else {range.start};
    if let Some((raw,rich))=affix(source,suffix_start..range.end)? {
        cite.suffix=Some(raw);cite.suffix_rich=Some(rich);fields.insert("suffix".into(),SourceSpan::new(source,suffix_start..range.end)?);
    }
    // An explicit empty {} is a source field even though it disables locator
    // heuristics rather than asking the processor to print an empty locator.
    if forced_end.is_some() && cite.locator.is_none() { fields.insert("locator".into(),SourceSpan::new(source,value_start..forced_end.unwrap()+1)?); }
    Ok(())
}
fn item(source: &str, range: Range<usize>, terms: &[(String,String)], mode: CiteMode, mode_span: Option<Range<usize>>) -> ApiResult<SourceItem> {
    let at=at_sign(source,range.clone()).ok_or_else(||error("key"))?;
    let suppress=at>range.start && source.as_bytes()[at-1]==b'-';
    if suppress && mode!=CiteMode::Normal {return Err(error("mode"));}
    let (name,key_range,end)=key(source,at)?;
    if end>range.end{return Err(error("key"));}
    let mut cite=Cite::new(name);cite.mode=if suppress{CiteMode::SuppressAuthor}else{mode};
    let mut fields=BTreeMap::new();fields.insert("key".into(),SourceSpan::new(source,key_range)?);
    fields.insert("mode".into(),SourceSpan::new(source,mode_span.unwrap_or(if suppress{at-1..at+1}else{at..at+1}))?);
    let prefix_end=if suppress{at-1}else{at};
    if let Some((raw,rich))=affix(source,range.start..prefix_end)? {cite.prefix=Some(raw);cite.prefix_rich=Some(rich);fields.insert("prefix".into(),SourceSpan::new(source,range.start..prefix_end)?);}
    tail(source,end..range.end,terms,&mut cite,&mut fields,true)?;
    cite.validate()?;
    Ok(SourceItem{cite,span:SourceSpan::new(source,range)?,fields})
}

pub fn scan(source: &str, locale: &str) -> ApiResult<Scan> {
    if source.len()>crate::document_transaction::MAX_DOCUMENT_BYTES{return Err(ApiError::new(413,"academic_budget"));}
    let mut hard=Vec::new();let mut links=Vec::new();let mut html_stack:Vec<(String,usize)>=Vec::new();
    let(events,owned_edges)=source_events(source)?;
    for(event,range)in events{match event{
        Event::Start(Tag::CodeBlock(_)|Tag::MetadataBlock(_)|Tag::Image{..})|Event::Code(_)|Event::InlineMath(_)|Event::DisplayMath(_)|Event::Html(_)=>hard.push(range),
        Event::InlineHtml(raw)=>{
            if owned_edges.contains(&(range.start,range.end)){hard.push(range);continue;}
            let raw=raw.trim();let closing=raw.starts_with("</");
            let tag=raw.trim_start_matches('<').trim_start_matches('/').chars().take_while(|c|c.is_ascii_alphanumeric()||*c=='-').collect::<String>().to_ascii_lowercase();
            if !tag.is_empty(){
                if closing{if let Some(index)=html_stack.iter().rposition(|(name,_)|name==&tag){let (_,start)=html_stack[index].clone();html_stack.truncate(index);hard.push(start..range.end);}}
                else if !raw.ends_with("/>")&&!matches!(tag.as_str(),"area"|"base"|"br"|"col"|"embed"|"hr"|"img"|"input"|"link"|"meta"|"param"|"source"|"track"|"wbr"){html_stack.push((tag,range.start));}
            }
            hard.push(range);
        },
        Event::Start(Tag::Link{..})=>links.push(range),_=>()
    }}
    let hard=merged(hard);let links=merged(links);let protected=merged(hard.iter().chain(&links).cloned().collect());
    let terms=locator_terms(locale);let mut citations=Vec::new();let mut diagnostics=Vec::new();let mut pos=0;
    while pos<source.len(){
        if let Some(range)=covering(&protected,pos){pos=range.end;continue;}
        if citations.len()>10000{return Err(ApiError::new(413,"academic_budget"));}
        let extension=source[pos..].starts_with(":cite[")&&!escaped(source,pos);
        let bracket=source[pos..].starts_with('[')&&!escaped(source,pos);
        if extension||bracket{
            let open=if extension{pos+5}else{pos};
            if let Some(end)=enclosed(source,open,'[',']'){
                let mut inner=open+1..end;let mut mode=CiteMode::Normal;let mut mode_span=None;
                if extension{
                    let text=&source[inner.clone()];
                    if !text.starts_with("author-only")||!text[11..].starts_with(char::is_whitespace){
                        diagnostics.push(json!({"code":"academic_context","field":"mode","from":source[..pos].encode_utf16().count(),"to":source[..end+1].encode_utf16().count()}));pos=end+1;continue;
                    }
                    mode=CiteMode::AuthorOnly;mode_span=Some(inner.start..inner.start+11);inner.start+=11;inner=trim(source,inner);
                }
                if extension||at_sign(source,inner.clone()).is_some(){
                    let result=parts(source,inner).and_then(|parts|parts.into_iter().map(|part|item(source,part,&terms,mode,mode_span.clone())).collect::<ApiResult<Vec<_>>>());
                    let blocked=overlaps(&hard,pos..end+1);
                    match result{
                        Ok(items) if !blocked&&items.iter().all(|item|covering(&links,item.fields["key"].byte_from).is_none())=>citations.push(SourceCitation{span:SourceSpan::new(source,pos..end+1)?,items,context:Default::default()}),
                        Ok(_)=>(),Err(error)=>diagnostics.push(json!({"code":error.code,"field":field(&error),"from":source[..pos].encode_utf16().count(),"to":source[..end+1].encode_utf16().count(),"source":&source[pos..end+1]})),
                    }
                    pos=end+1;continue;
                }
            }
        }
        if source[pos..].starts_with('@')&&!escaped(source,pos)&&boundary(source,pos){
            if let Ok((name,key_range,end))=key(source,pos){
                let mut cite=Cite::new(name);cite.mode=CiteMode::Prose;let mut fields=BTreeMap::new();fields.insert("key".into(),SourceSpan::new(source,key_range)?);fields.insert("mode".into(),SourceSpan::new(source,pos..pos+1)?);
                let mut finish=end;let mut next=end;
                while next<source.len()&&source[next..].starts_with(char::is_whitespace)&&!source[next..].starts_with("\n\n")&&!source[next..].starts_with("\r\n\r\n"){next+=source[next..].chars().next().unwrap().len_utf8();}
                if source[next..].starts_with('[')&&covering(&links,next).is_none(){
                    if let Some(close)=enclosed(source,next,'[',']'){
                        if !overlaps(&hard,next..close+1){
                            if let Err(error)=tail(source,next+1..close,&terms,&mut cite,&mut fields,false){diagnostics.push(json!({"code":error.code,"field":field(&error),"from":source[..pos].encode_utf16().count(),"to":source[..close+1].encode_utf16().count(),"source":&source[pos..close+1]}));pos=close+1;continue;}
                            finish=close+1;
                        }
                    }
                }
                cite.validate()?;let span=SourceSpan::new(source,pos..finish)?;
                citations.push(SourceCitation{span:span.clone(),items:vec![SourceItem{cite,span,fields}],context:Default::default()});pos=finish;continue;
            }
        }
        pos+=source[pos..].chars().next().unwrap().len_utf8();
    }
    if citations.len()>10000{return Err(ApiError::new(413,"academic_budget"));}
    Ok(Scan{citations,diagnostics})
}

/// Returns actual reading order: a note's citations occur at its first real
/// reference, not at the later definition. Unknown clusters do not create a
/// synthetic note or influence following citation state.
pub fn reading_order(source:&str,citations:Vec<SourceCitation>,note_style:bool,known:&HashSet<String>)->ApiResult<Vec<SourceCitation>>{
    let mut definitions=HashMap::<String,Range<usize>>::new();let mut refs=Vec::new();
    let(events,_)=source_events(source)?;
    for(event,range)in events{match event{
        Event::Start(Tag::FootnoteDefinition(label))=>{definitions.insert(label.into_string(),range);},
        Event::FootnoteReference(label)=>refs.push((range.start,label.into_string())),_=>()
    }}
    #[derive(Clone)]enum Flow{Citation(usize),Note(String)}
    let inside=|at:usize|definitions.iter().find(|(_,range)|range.contains(&at)).map(|(label,_)|label.clone());
    let mut flow=Vec::new();
    for(index,citation)in citations.iter().enumerate(){if inside(citation.span.byte_from).is_none(){flow.push((citation.span.byte_from,Flow::Citation(index)));}}
    for(at,label)in refs{if inside(at).is_none(){flow.push((at,Flow::Note(label)));}}
    flow.sort_by_key(|(at,_)|*at);let mut seen=HashSet::new();let mut counter=0;let mut out=Vec::new();
    for(_,entry)in flow{match entry{
        Flow::Citation(index)=>{
            let mut citation=citations[index].clone();
            if note_style&&citation.items.iter().any(|item|item.cite.mode!=CiteMode::AuthorOnly)&&citation.items.iter().all(|item|known.contains(&item.cite.key)){counter+=1;citation.context.note_number=Some(counter);}
            out.push(citation);
        },
        Flow::Note(label)=>{
            if !seen.insert(label.clone()){continue;}
            let Some(range)=definitions.get(&label)else{continue};counter+=1;
            for citation in citations.iter().filter(|citation|range.contains(&citation.span.byte_from)){
                let mut citation=citation.clone();citation.context=CiteContext{note_number:Some(counter),in_note:true,note_label:Some(label.clone())};out.push(citation);
            }
        }
    }}
    Ok(out)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn literal_html_entities_keep_affixes_and_source_coordinates_without_eating_real_separators(){
        let source="😀 [ &lt;script&gt;window.__qaInjected=1&lt;/script&gt; &#x3B1; &amp; @study, suffix &copy;; -@other, p. 3]";
        let result=scan(source,"en-US").unwrap();assert!(result.diagnostics.is_empty(),"{:?}",result.diagnostics);
        assert_eq!(result.citations.len(),1);let cluster=&result.citations[0];assert_eq!(cluster.items.len(),2);
        assert_eq!(cluster.span.source,&source[cluster.span.byte_from..cluster.span.byte_to]);
        assert_eq!(cluster.items[0].cite.key,"study");assert_eq!(cluster.items[1].cite.mode,CiteMode::SuppressAuthor);
        let nodes=cluster.items[0].cite.prefix_rich.as_ref().unwrap();
        assert_eq!(publication_rich::plain(nodes),"<script>window.__qaInjected=1</script> α & ");
        let prefix=publication_rich::html(nodes).unwrap();
        // Escaping underscores as numeric HTML references is also safe. The
        // authoritative Unicode text above must remain exact in either form.
        assert!(prefix.contains("&lt;script&gt;window."),"{prefix}");assert!(prefix.contains("&lt;/script&gt;"));assert!(!prefix.contains("<script>"));
        assert!(prefix.contains('α'));assert!(prefix.contains("&amp;"));
        for item in &cluster.items{for field in item.fields.values(){assert_eq!(&source[field.byte_from..field.byte_to],field.source);assert_eq!(crate::document_transaction::byte_offset(source,field.from).unwrap(),field.byte_from);}}
        for raw in ["&notARealEntity;", "&amp; trailing;", "\\&amp;", "ordinary;"]{
            let index=raw.rfind(';').unwrap();assert!(!entity_terminator(raw,0,index),"{raw}");
        }
        for raw in ["&lt;","&amp;","&#945;","&#x1F600;","&CounterClockwiseContourIntegral;"]{assert!(entity_terminator(raw,0,raw.len()-1),"{raw}");}
        let invalid=scan("[unknown &notARealEntity; @study]","en-US").unwrap();assert!(invalid.citations.is_empty());assert!(!invalid.diagnostics.is_empty());
    }
    #[test]fn citation_parser_preserves_context_affixes_forced_locators_and_precise_unicode_ranges(){
        let source="😀 [see @doe, pp. 33-35 and *passim*; -@smith, chap. 1]. @doe [p. 42]. :cite[author-only @doe]. [@doe{}, 99 years later] [@doe{ii, A, D-Z}, suffix]";
        let scan=scan(source,"en-US").unwrap();assert!(scan.diagnostics.is_empty(),"{:?}",scan.diagnostics);assert_eq!(scan.citations.len(),5);
        let first=&scan.citations[0];assert_eq!(first.items[0].cite.locator.as_deref(),Some("33-35"));assert_eq!(first.items[0].cite.kind.as_deref(),Some("page"));assert_eq!(first.items[0].cite.prefix.as_deref(),Some("see "));assert_eq!(first.items[1].cite.mode,CiteMode::SuppressAuthor);
        assert!(publication_rich::html(first.items[0].cite.suffix_rich.as_ref().unwrap()).unwrap().contains("font-style:italic"));
        assert_eq!(scan.citations[1].items[0].cite.mode,CiteMode::Prose);assert_eq!(scan.citations[1].items[0].cite.locator.as_deref(),Some("42"));
        assert_eq!(scan.citations[2].items[0].cite.mode,CiteMode::AuthorOnly);assert!(scan.citations[3].items[0].cite.locator.is_none());assert_eq!(scan.citations[4].items[0].cite.locator.as_deref(),Some("ii, A, D-Z"));
        for citation in &scan.citations{assert_eq!(crate::document_transaction::byte_offset(source,citation.span.from).unwrap(),citation.span.byte_from);for item in &citation.items{for span in item.fields.values(){assert_eq!(&source[span.byte_from..span.byte_to],span.source);}}}
    }
    #[test]fn citation_parser_protects_literal_contexts_and_retains_general_pandoc_key_boundaries(){
        let source="---\nbibliography: refs.bib\nexample: '@fake'\n---\n`[@code]` $[@math]$ <span>@html</span> [@link](https://example.test/@path) user@example.test \\@escaped\n\n```md\n[@fence]\n```\n\n@Foo_bar.baz. [@{https://example.test/bib?name=work&year=2000}, p. 33]";
        let scan=scan(source,"en-US").unwrap();assert_eq!(scan.citations.len(),2,"{:?}",scan.citations);assert_eq!(scan.citations[0].items[0].cite.key,"Foo_bar.baz");assert_eq!(scan.citations[1].items[0].cite.key,"https://example.test/bib?name=work&year=2000");
    }
    #[test]fn citation_context_uses_first_note_reference_and_skips_missing_or_author_only_note_slots(){
        let mut source=String::from("[@a] :cite[author-only @a] [@missing] ");
        for index in 0..100{source.push_str(&format!("manual[^n{index}] "));}
        source.push_str("[@a]\n\n");for index in 0..100{source.push_str(&format!("[^n{index}]: Manual note {index}.\n\n"));}
        let known=HashSet::from(["a".into()]);let ordered=reading_order(&source,scan(&source,"en-US").unwrap().citations,true,&known).unwrap();
        assert_eq!(ordered.len(),4);assert_eq!(ordered[0].context.note_number,Some(1));assert_eq!(ordered[1].context.note_number,None);assert_eq!(ordered[2].context.note_number,None);assert_eq!(ordered[3].context.note_number,Some(102));
        let source="A[^n], then [@b]. Again[^n].\n\n[^n]: See [@a, p. 1].\n";let known=HashSet::from(["a".into(),"b".into()]);let ordered=reading_order(source,scan(source,"en-US").unwrap().citations,true,&known).unwrap();
        assert_eq!(ordered[0].items[0].cite.key,"a");assert!(ordered[0].context.in_note);assert_eq!(ordered[0].context.note_number,Some(1));assert_eq!(ordered[1].context.note_number,Some(2));
    }
}

#[cfg(test)]
#[path="publication_citations_author_tests.rs"]
mod author_tests;
