//! Source-authored GFM cell attributes and shared table geometry.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::md_ast::{Align, Block, Document, Inline};

const UNSUPPORTED: &str = crate::api_codes::PUBLICATION_TABLE_UNSUPPORTED;
const MAX_COLSPAN: usize = 16;
const MAX_ROWSPAN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

impl VerticalAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Middle => "middle",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CellAttributes {
    colspan: Option<usize>,
    rowspan: Option<usize>,
    align: Option<Align>,
    valign: Option<VerticalAlign>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CellPlacement {
    row: usize,
    column: usize,
    attributes: CellAttributes,
}

/// Sparse declarations. Every authored cell stays in its original AST slot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TableGeometry {
    placements: Vec<CellPlacement>,
    invalid: bool,
}

pub(crate) struct CellSuffix<'a> {
    pub(crate) brace: usize,
    pub(crate) tail: &'a str,
    pub(crate) attributes: Option<CellAttributes>,
}

pub(crate) fn source_candidate(source: &str) -> bool {
    source.contains('{') && source.contains('}') && ["colspan=", "rowspan=", "align=", "valign="].iter().any(|key| source.contains(key))
}

fn span(value: &str, limit: usize) -> Option<usize> {
    if value.starts_with('0') || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|n| (1..=limit).contains(n))
}

fn attributes(source: &str) -> Option<CellAttributes> {
    if source.starts_with(' ') || source.ends_with(' ') {
        return None;
    }
    let mut cell = CellAttributes::default();
    for assignment in source.split(' ').filter(|part| !part.is_empty()) {
        let (key, value) = assignment.split_once('=')?;
        match key {
            "colspan" if cell.colspan.is_none() => cell.colspan = Some(span(value, MAX_COLSPAN)?),
            "rowspan" if cell.rowspan.is_none() => cell.rowspan = Some(span(value, MAX_ROWSPAN)?),
            "align" if cell.align.is_none() => cell.align = Some(match value {
                "left" => Align::Left,
                "center" => Align::Center,
                "right" => Align::Right,
                _ => return None,
            }),
            "valign" if cell.valign.is_none() => cell.valign = Some(match value {
                "top" => VerticalAlign::Top,
                "middle" => VerticalAlign::Middle,
                "bottom" => VerticalAlign::Bottom,
                _ => return None,
            }),
            _ => return None,
        }
    }
    Some(cell)
}

/// The raw slice comes from a GFM table cell event. The caller verifies the
/// opening brace belongs to a top-level text event before using the suffix.
pub(crate) fn cell_suffix(source: &str) -> Option<CellSuffix<'_>> {
    let text = source.trim_matches([' ', '\t']);
    let before_close = text.strip_suffix('}')?;
    if before_close.bytes().rev().take_while(|byte| *byte == b'\\').count() % 2 != 0 {
        return None;
    }
    let open = before_close.rfind('{')?;
    let body = &before_close[open + 1..];
    if open > 0 && text.as_bytes()[open - 1] != b' ' || body.contains(['}', '\n', '\r']) {
        return None;
    }
    if !body.split([' ', '\t']).any(|word| {
        word.split_once('=').is_some_and(|(key, _)| matches!(key, "colspan" | "rowspan" | "align" | "valign"))
    }) {
        return None;
    }
    let leading = source.len() - source.trim_start_matches([' ', '\t']).len();
    Some(CellSuffix {
        brace: leading + open,
        tail: &text[open.saturating_sub(1)..],
        attributes: attributes(body),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellGeometry {
    pub origin_row: usize,
    pub origin_col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub align: Option<Align>,
    pub valign: Option<VerticalAlign>,
}

pub struct TableSlot<'a> {
    pub origin_row: usize,
    pub origin_col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub align: Align,
    pub valign: Option<VerticalAlign>,
    /// The origin owns every visible source segment, in row-major order.
    pub contents: Vec<&'a [Inline]>,
}

pub struct TableLayout<'a> {
    pub columns: usize,
    /// Includes the header as the first row.
    pub rows: Vec<Vec<TableSlot<'a>>>,
}

type Owners = HashMap<(usize, usize), CellPlacement>;

fn columns(header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> usize {
    rows.iter().map(Vec::len).fold(header.len(), usize::max).max(1)
}

impl TableGeometry {
    pub fn is_enhanced(&self) -> bool {
        self.invalid || !self.placements.is_empty()
    }

    pub(crate) fn add(&mut self, row: usize, column: usize, attributes: Option<CellAttributes>) {
        match attributes {
            Some(attributes) => self.placements.push(CellPlacement { row, column, attributes }),
            None => self.invalid = true,
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.invalid = true;
    }

    pub(crate) fn from_render_json(value: &serde_json::Value) -> Result<Self, &'static str> {
        let placements = value.get("placements").and_then(serde_json::Value::as_array).ok_or(UNSUPPORTED)?;
        if placements.iter().any(|cell| !cell.get("attributes").is_some_and(serde_json::Value::is_object)) {
            return Err(UNSUPPORTED);
        }
        Self::deserialize(value).map_err(|_| UNSUPPORTED)
    }

    fn owners(&self, columns: usize, rows: usize) -> Result<Owners, &'static str> {
        if self.invalid {
            return Err(UNSUPPORTED);
        }
        let mut owners = Owners::new();
        let mut previous = None;
        for &placement in &self.placements {
            let CellPlacement { row, column, attributes } = placement;
            let rowspan = attributes.rowspan.unwrap_or(1);
            let colspan = attributes.colspan.unwrap_or(1);
            let position = (row, column);
            if previous.is_some_and(|p| p >= position)
                || attributes == CellAttributes::default()
                || attributes.align == Some(Align::None)
                || !(1..=MAX_ROWSPAN).contains(&rowspan)
                || !(1..=MAX_COLSPAN).contains(&colspan)
                || row >= rows
                || column >= columns
                || rowspan > rows - row
                || colspan > columns - column
                || (row == 0 && rowspan > 1)
            {
                return Err(UNSUPPORTED);
            }
            previous = Some(position);
            if rowspan == 1 && colspan == 1 {
                continue;
            }
            for r in row..row + rowspan {
                for c in column..column + colspan {
                    if owners.insert((r, c), placement).is_some() {
                        return Err(UNSUPPORTED);
                    }
                }
            }
        }
        Ok(owners)
    }

    fn grid_rows(
        &self,
        columns: usize,
        rows: usize,
    ) -> Result<impl Iterator<Item = Vec<CellGeometry>> + '_, &'static str> {
        let owners = self.owners(columns, rows)?;
        let mut placements = self.placements.iter().copied().peekable();
        Ok((0..rows).map(move |row| {
            (0..columns).map(|column| {
                let authored = placements.next_if(|cell| (cell.row, cell.column) == (row, column));
                let placement = owners.get(&(row, column)).copied().or(authored).unwrap_or(CellPlacement {
                    row,
                    column,
                    attributes: CellAttributes::default(),
                });
                CellGeometry {
                    origin_row: placement.row,
                    origin_col: placement.column,
                    rowspan: placement.attributes.rowspan.unwrap_or(1),
                    colspan: placement.attributes.colspan.unwrap_or(1),
                    align: placement.attributes.align,
                    valign: placement.attributes.valign,
                }
            }).collect()
        }))
    }

    /// Resolve the origins and spans for a rectangular grid including the header.
    pub(crate) fn grid(&self, columns: usize, rows: usize) -> Result<Vec<Vec<CellGeometry>>, &'static str> {
        Ok(self.grid_rows(columns, rows)?.collect())
    }

    pub fn layout<'a>(&self, header: &'a [Vec<Inline>], rows: &'a [Vec<Vec<Inline>>], aligns: &[Align]) -> crate::ApiResult<TableLayout<'a>> {
        let columns = columns(header, rows);
        let geometry = self.grid_rows(columns, rows.len() + 1).map_err(|code| crate::ApiError::new(422, code))?;
        let get_row = |row: usize| if row == 0 { header } else { rows[row - 1].as_slice() };
        let mut grid = Vec::with_capacity(rows.len() + 1);
        for (row, cells) in geometry.enumerate() {
            let mut slots = Vec::with_capacity(columns);
            for (column, cell) in cells.into_iter().enumerate() {
                let mut contents = Vec::new();
                if (row, column) == (cell.origin_row, cell.origin_col) {
                    for r in row..row + cell.rowspan {
                        let source_row = get_row(r);
                        for source_cell in source_row.iter().skip(column).take(cell.colspan) {
                            if source_cell.iter().any(|value| !matches!(value, Inline::Text(text) if text.trim().is_empty())) {
                                contents.push(source_cell.as_slice());
                            }
                        }
                    }
                }
                slots.push(TableSlot {
                    origin_row: cell.origin_row,
                    origin_col: cell.origin_col,
                    rowspan: cell.rowspan,
                    colspan: cell.colspan,
                    align: cell.align.unwrap_or_else(|| aligns.get(cell.origin_col).copied().unwrap_or(Align::None)),
                    valign: cell.valign,
                    contents,
                });
            }
            grid.push(slots);
        }
        Ok(TableLayout { columns, rows: grid })
    }
}

#[derive(Clone, Copy)]
enum GeometrySupport {
    None,
    All,
    RustPdf,
}

fn validate_blocks(blocks: &[Block], support: GeometrySupport) -> Result<(), &'static str> {
    for block in blocks {
        match block.unstyled() {
            Block::Table { header, rows, geometry, .. } if geometry.is_enhanced() => {
                if matches!(support, GeometrySupport::None) {
                    return Err(UNSUPPORTED);
                }
                geometry.owners(columns(header, rows), rows.len() + 1)?;
            }
            Block::Quote(inner) => {
                // The Rust PDF quote flow has no nested table layout.
                let support = if matches!(support, GeometrySupport::RustPdf) { GeometrySupport::None } else { support };
                validate_blocks(inner, support)?;
            }
            Block::FootnoteDef { blocks: inner, .. } => validate_blocks(inner, support)?,
            Block::List { items, .. } => {
                for item in items {
                    validate_blocks(&item.blocks, support)?;
                }
            }
            _ => (),
        }
    }
    Ok(())
}

pub(crate) fn validate_document(doc: &Document, allow_enhanced: bool) -> Result<(), &'static str> {
    let support = if allow_enhanced { GeometrySupport::All } else { GeometrySupport::None };
    validate_blocks(&doc.blocks, support)
}

pub(crate) fn validate_rust_pdf_document(doc: &Document) -> Result<(), &'static str> {
    validate_blocks(&doc.blocks, GeometrySupport::RustPdf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_ast::{inline_plain, parse};
    use serde_json::json;

    #[test]
    fn source_cells_remain_distinct_while_layout_resolves_merged_content_and_alignment() {
        let source = concat!(
            "\u{feff}| Head A {colspan=2} | Head B | Head C |\r\n",
            "| :--- | :---: | ---: |\r\n",
            "| **One** {valign=bottom align=right rowspan=2 colspan=2} | *Two* | Five {align=left valign=top} |\r\n",
            "| [Three][ref] | `Four` {align=center} | Six |\r\n",
            "| {valign=middle} | escaped \\{colspan=2} | `{align=center}` |\r\n",
            "\r\n[ref]: https://example.test/readmd\r\n",
        );
        let doc = parse(source);
        assert!(validate_document(&doc, true).is_ok());
        assert_eq!(validate_document(&doc, false), Err(UNSUPPORTED));
        let Block::Table { aligns, header, rows, geometry } = &doc.blocks[0] else { panic!("GFM table") };
        assert_eq!(inline_plain(&rows[0][1]), "Two");
        assert_eq!(inline_plain(&rows[1][1]), "Four");
        assert!(matches!(&rows[1][0][0], Inline::Link { href, .. } if href == "https://example.test/readmd"));
        assert_eq!(inline_plain(&rows[2][1]), "escaped {colspan=2}");
        assert_eq!(rows[2][2], [Inline::Code("{align=center}".into())]);
        let grid = geometry.layout(header, rows, aligns).unwrap();
        assert_eq!(grid.columns, 3);
        let header = &grid.rows[0][0];
        assert_eq!(header.colspan, 2);
        assert_eq!(header.align, Align::Left);
        assert_eq!(header.contents.iter().map(|cell| inline_plain(cell)).collect::<Vec<_>>(), ["Head A", "Head B"]);
        let owner = &grid.rows[1][0];
        assert_eq!((owner.rowspan, owner.colspan, owner.align, owner.valign), (2, 2, Align::Right, Some(VerticalAlign::Bottom)));
        assert_eq!(owner.contents.iter().map(|cell| inline_plain(cell)).collect::<Vec<_>>(), ["One", "Two", "Three", "Four"]);
        let covered = &grid.rows[2][1];
        assert_eq!((covered.origin_row, covered.origin_col), (1, 0));
        assert!(covered.contents.is_empty());
        assert_eq!(grid.rows[1][2].align, Align::Left);
        assert_eq!(grid.rows[1][2].valign, Some(VerticalAlign::Top));
        assert_eq!(grid.rows[3][0].valign, Some(VerticalAlign::Middle));
        assert!(grid.rows[3][0].contents.is_empty());
    }

    #[test]
    fn invalid_enhanced_tables_are_rejected_without_extending_ordinary_gfm() {
        const HEAD: &str = "| A | B |\n|---|---|\n";
        for source in [
            "| A {rowspan=2} | B |\n|---|---|\n| a | b |".into(),
            format!("{HEAD}| a {{colspan=2 rowspan=2}} | b |\n| c {{colspan=2}} | d |"),
            format!("{HEAD}| a {{rowspan=2}} | b |"),
            format!("{HEAD}| a | b {{colspan=2}} |"),
            format!("{HEAD}| a {{colspan=02}} | b |"),
            format!("{HEAD}| a {{align=left align=right}} | b |"),
            format!("{HEAD}| a {{valign=top bad=1}} | b |"),
            format!("{HEAD}| a {{align=*left*}} | b |"),
            format!("{HEAD}| a {{align=left}} | b | extra |"),
        ] {
            let doc = parse(&source);
            assert_eq!(validate_document(&doc, true), Err(UNSUPPORTED), "{source:?}");
        }
        let source = format!("{HEAD}| escaped \\{{rowspan=2}} | `{{colspan=2}}` | unused |");
        let doc = parse(&source);
        assert!(validate_document(&doc, false).is_ok());
        let Block::Table { rows, geometry, .. } = &doc.blocks[0] else { panic!("GFM table") };
        assert!(!geometry.is_enhanced());
        assert_eq!(inline_plain(&rows[0][0]), "escaped {rowspan=2}");
        assert_eq!(inline_plain(&rows[0][1]), "{colspan=2}");
        let escaped_close = parse(&format!("{HEAD}| escaped {{align=left\\}} | literal |"));
        assert!(validate_document(&escaped_close, false).is_ok());
        let Block::Table { rows, .. } = &escaped_close.blocks[0] else { panic!("GFM table") };
        assert_eq!(inline_plain(&rows[0][0]), "escaped {align=left}");
    }

    #[test]
    fn render_json_resolves_geometry_and_preserves_each_source_cell_for_inline_visitors() {
        let doc = parse(concat!(
            "| Header A | Header B | Header C |\n",
            "| :--- | --- | ---: |\n",
            "| **Owner** {rowspan=2 colspan=2 align=right valign=bottom} | [Covered link](https://example.test/) | Left {align=left} |\n",
            "| $x^2$ | `cell code`[^note] | Footer {valign=middle} |\n\n",
            "[^note]: Definition.\n",
        ));
        let style = crate::export_styles::sanitize(&json!({}));
        let notes = crate::rust_pdf_notes::Projection::analyze(&doc, &style).unwrap().unwrap();
        let rendered = crate::md_ast::to_rust_note_render_json(&doc.blocks, &notes, 0, &style).unwrap();
        let table = &rendered[0];
        assert_eq!(table["aligns"], json!(["left", null, "right"]));
        assert_eq!(table["rows"][0][0], json!([{"t":"bold","v":[{"t":"text","v":"Owner"}]}]));
        assert_eq!(table["rows"][0][1][0]["href"], "https://example.test/");
        assert_eq!(table["rows"][1][0][0]["latex"], "x^2");
        assert_eq!(table["rows"][1][1][0], json!({"t":"code","v":"cell code"}));
        assert_eq!(table["rows"][1][1][1]["t"], "note-marker");

        let geometry = TableGeometry::from_render_json(&table["tableGeometry"]).unwrap();
        let grid = geometry.grid(3, 3).unwrap();
        assert_eq!(grid[1][0], CellGeometry {
            origin_row: 1, origin_col: 0, rowspan: 2, colspan: 2,
            align: Some(Align::Right), valign: Some(VerticalAlign::Bottom),
        });
        assert_eq!(grid[2][1], grid[1][0]);
        assert_eq!(grid[1][2].align, Some(Align::Left));
        assert_eq!(grid[2][2].align, None);
        assert_eq!(grid[2][2].valign, Some(VerticalAlign::Middle));
    }

    #[test]
    fn render_geometry_rejects_invalid_placements_at_the_json_boundary() {
        let valid = json!({
            "invalid": false,
            "placements": [{"row": 1, "column": 0, "attributes": {"rowspan": 2, "colspan": 2}}]
        });
        let check = |value: &serde_json::Value| TableGeometry::from_render_json(value).and_then(|geometry| geometry.grid(3, 3));
        assert!(check(&valid).is_ok());
        for (path, replacement) in [
            ("/invalid", json!(true)),
            ("/placements/0/row", json!(usize::MAX)),
            ("/placements/0/row", json!(0)),
            ("/placements/0/column", json!(2)),
            ("/placements/0/attributes/rowspan", json!(0)),
            ("/placements/0/attributes/rowspan", json!("2")),
            ("/placements/0/attributes/colspan", json!(17)),
            ("/placements/0/attributes", json!({"align": "none"})),
            ("/placements/0/attributes", json!({"valign": "center"})),
            ("/placements/0/attributes", json!([2, 2, null, null])),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(path).unwrap() = replacement;
            assert_eq!(check(&invalid), Err(UNSUPPORTED), "{invalid}");
        }
        for placement in [
            json!({"row": 1, "column": 0, "attributes": {"align": "left"}}),
            json!({"row": 2, "column": 1, "attributes": {"colspan": 2}}),
        ] {
            let mut invalid = valid.clone();
            invalid["placements"].as_array_mut().unwrap().push(placement);
            assert_eq!(check(&invalid), Err(UNSUPPORTED), "{invalid}");
        }
    }
}
