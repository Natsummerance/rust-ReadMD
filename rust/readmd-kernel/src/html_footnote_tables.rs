//! Project table events before numbering notes so references, styled runs and
//! links from covered cells remain live in the standalone HTML.
use crate::md_ast::Align;
use crate::table_geometry::{cell_suffix, source_candidate, TableGeometry};
use pulldown_cmark::{Alignment, Event, Tag, TagEnd};
use std::ops::Range;

type SourceEvent<'a> = (Event<'a>, Range<usize>);

pub(super) fn project<'a>(source: &str, events: Vec<SourceEvent<'a>>) -> Result<Vec<Event<'a>>, String> {
    let mut out = Vec::with_capacity(events.len());
    let mut events = events.into_iter();
    while let Some((event, range)) = events.next() {
        if let Event::Start(Tag::Table(aligns)) = &event {
            if source_candidate(&source[range]) {
                let mut table = vec![event.clone()];
                let mut geometry = TableGeometry::default();
                let mut rows = Vec::new();
                let mut row = Vec::new();
                let mut cell = Vec::new();
                let mut candidate = None;
                for (ev, bytes) in events.by_ref() {
                    table.push(ev.clone());
                    match ev {
                        Event::Start(Tag::TableCell) => {
                            candidate = cell_suffix(&source[bytes.clone()]).map(|s| (bytes.start + s.brace, s));
                        }
                        Event::End(TagEnd::TableCell) => {
                            if let Some((brace, suffix)) = candidate.take() {
                                if let Some((Event::Text(text), last)) = cell.last_mut() {
                                    if Range::contains(last, &brace) && text.ends_with(suffix.tail) {
                                        geometry.add(rows.len(), row.len(), suffix.attributes);
                                        *text = text[..text.len() - suffix.tail.len()].to_owned().into();
                                    }
                                }
                            }
                            row.push(std::mem::take(&mut cell));
                        }
                        Event::End(TagEnd::TableHead | TagEnd::TableRow) => rows.push(std::mem::take(&mut row)),
                        Event::Start(Tag::TableHead | Tag::TableRow) => (),
                        Event::End(TagEnd::Table) => break,
                        _ => cell.push((ev, bytes)),
                    }
                }
                if !geometry.is_enhanced() { out.extend(table); continue; }
                let aligns = aligns.iter().map(|a| match a {
                    Alignment::None => Align::None, Alignment::Left => Align::Left,
                    Alignment::Center => Align::Center, Alignment::Right => Align::Right,
                }).collect::<Vec<_>>();
                let grid = rows.iter().map(|r| vec![Vec::new(); r.len()]).collect::<Vec<_>>();
                let Some((header, body)) = grid.split_first() else { return Err(crate::api_codes::PUBLICATION_TABLE_UNSUPPORTED.into()); };
                let layout = geometry.layout(header, body, &aligns).map_err(|e| e.code)?;
                out.push(Event::Html("<table class=\"readmd-table-geometry\"><thead>".into()));
                for (r, slots) in layout.rows.iter().enumerate() {
                    if r == 1 { out.push(Event::Html("</thead><tbody>".into())); }
                    out.push(Event::Html("<tr>".into()));
                    let tag = if r == 0 { "th" } else { "td" };
                    for (c, slot) in slots.iter().enumerate() {
                        if (r, c) != (slot.origin_row, slot.origin_col) { continue; }
                        let mut open = format!("<{tag}");
                        if slot.colspan > 1 { open.push_str(&format!(" colspan=\"{}\"", slot.colspan)); }
                        if slot.rowspan > 1 { open.push_str(&format!(" rowspan=\"{}\"", slot.rowspan)); }
                        if slot.align != Align::None { open.push_str(&format!(" align=\"{}\"", slot.align.as_str())); }
                        if let Some(v) = slot.valign { open.push_str(&format!(" valign=\"{}\"", v.as_str())); }
                        open.push('>');
                        out.push(Event::Html(open.into()));
                        let mut first = true;
                        for row in rows.iter_mut().skip(r).take(slot.rowspan) {
                            for cell in row.iter_mut().skip(c).take(slot.colspan) {
                                if cell.iter().all(|(e, _)| matches!(e, Event::Text(t) if t.trim().is_empty())) { continue; }
                                if !first { out.push(Event::HardBreak); }
                                first = false;
                                out.extend(cell.drain(..).map(|(e, _)| e));
                            }
                        }
                        out.push(Event::Html(format!("</{tag}>").into()));
                    }
                    out.push(Event::Html("</tr>".into()));
                }
                out.push(Event::Html(if body.is_empty() { "</thead></table>" } else { "</tbody></table>" }.into()));
                continue;
            }
        }
        out.push(event);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_geometry_preserves_footnote_targets_and_code_literals() {
        let md = "| H | I | J |\n|---|---|---|\n| **A** {colspan=2 rowspan=2 align=center valign=bottom} | B[^n] | X |\n| C | D | `{align=right}` |\n\n[^n]: Note body\n";
        let html = crate::html_footnotes::render(md).unwrap().unwrap();
        assert!(html.contains("colspan=\"2\" rowspan=\"2\" align=\"center\" valign=\"bottom\""), "{html}");
        assert!(html.contains("<strong>A</strong><br />\nB<sup"), "{html}");
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), 1);
        assert_eq!(html.matches("role=\"doc-backlink\"").count(), 1);
        assert!(html.contains("<code>{align=right}</code>"));
        assert!(html.contains("Note body"));
        assert!(!html.contains("{colspan"));
    }
}
