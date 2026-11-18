//! Shared table geometry for editable slides and their HTML preview.
use std::collections::BTreeMap;

use crate::api_codes::PUBLICATION_TABLE_UNSUPPORTED;
use crate::md_ast::{Align, Inline};
use crate::table_geometry::{TableGeometry, TableLayout, TableSlot, VerticalAlign};

use super::table_style::{TableAlign, TableRowKind, TableStyle};
use super::{advance, aligned_text_body, check, emu, inline_runs, line_html, position, wrap};
use super::{Builder, Line, Rect, Result, Run, Shape, BOTTOM, BUDGET, MAX_SLIDES, TOP, W, WIDTH};

const FONT_SIZE: f64 = 20.;
const LINE_HEIGHT: f64 = 25.;
const PADDING_X: f64 = 10.;
const PADDING_Y: f64 = 8.;
const MIN_ROW_HEIGHT: f64 = LINE_HEIGHT + 2. * PADDING_Y;

#[derive(Clone, Debug)]
pub(super) struct TableRow {
    pub(super) cells: Vec<TableCell>,
    height: f64,
    kind: TableRowKind,
}

#[derive(Clone, Debug)]
pub(super) struct TableCell {
    pub(super) lines: Vec<Line>,
    align: TableAlign,
    valign: VerticalAlign,
    rowspan: usize,
    colspan: usize,
    row_offset: usize,
    col_offset: usize,
}

impl TableCell {
    fn covered(&self) -> bool {
        self.row_offset != 0 || self.col_offset != 0
    }

    fn merge_attributes(&self) -> String {
        // DrawingML repeats the height on the top edge and the width on the left edge.
        let mut attributes = String::new();
        if self.row_offset == 0 && self.rowspan > 1 {
            attributes.push_str(&format!(" rowSpan=\"{}\"", self.rowspan));
        }
        if self.col_offset == 0 && self.colspan > 1 {
            attributes.push_str(&format!(" gridSpan=\"{}\"", self.colspan));
        }
        if self.col_offset != 0 {
            attributes.push_str(" hMerge=\"1\"");
        }
        if self.row_offset != 0 {
            attributes.push_str(" vMerge=\"1\"");
        }
        attributes
    }
}

fn padded_height(lines: usize) -> f64 {
    lines.max(1) as f64 * LINE_HEIGHT + 2. * PADDING_Y
}

impl TableRow {
    fn from_layout(
        index: usize,
        slots: &[TableSlot<'_>],
        col_width: f64,
        style: &TableStyle,
        enhanced: bool,
    ) -> Result<Self> {
        let kind = match index {
            0 => TableRowKind::Header,
            n if n % 2 == 0 => TableRowKind::Band,
            _ => TableRowKind::Body,
        };
        let bold = index == 0 && style.header_bold;
        let mut cells = Vec::with_capacity(slots.len());
        for (column, slot) in slots.iter().enumerate() {
            check()?;
            let content_width = col_width * slot.colspan as f64 - 2. * PADDING_X;
            let mut cell_lines = Vec::new();
            for segment in &slot.contents {
                let mut runs = Vec::new();
                inline_runs(
                    segment,
                    &Run {
                        bold,
                        ..Run::default()
                    },
                    &mut runs,
                )?;
                if enhanced
                    && runs.iter().any(|run| {
                        run.text.chars().filter(|&c| c != '\n').any(|c| {
                            advance(c, FONT_SIZE, run.code) * if run.bold { 1.06 } else { 1. }
                                > content_width
                        })
                    })
                {
                    return Err(PUBLICATION_TABLE_UNSUPPORTED.into());
                }
                // A new paragraph keeps every original nonempty cell distinguishable.
                cell_lines.extend(wrap(&runs, FONT_SIZE, content_width)?);
            }
            cells.push(TableCell {
                lines: cell_lines,
                align: TableAlign::column(slot.align, style.default_align),
                valign: slot.valign.unwrap_or(VerticalAlign::Top),
                rowspan: slot.rowspan,
                colspan: slot.colspan,
                row_offset: index - slot.origin_row,
                col_offset: column - slot.origin_col,
            });
        }
        let line_count = cells
            .iter()
            .filter(|cell| !cell.covered() && cell.rowspan == 1)
            .map(|cell| cell.lines.len())
            .max()
            .unwrap_or_default();
        Ok(Self {
            cells,
            height: padded_height(line_count),
            kind,
        })
    }

    fn fragment(&self, start: usize, end: usize) -> Self {
        let cells = self
            .cells
            .iter()
            .map(|cell| TableCell {
                lines: cell.lines[start.min(cell.lines.len())..end.min(cell.lines.len())].to_vec(),
                ..*cell
            })
            .collect();
        Self {
            cells,
            height: padded_height(end - start),
            kind: self.kind,
        }
    }
}

pub(super) fn lines(rows: &[TableRow]) -> impl Iterator<Item = &Line> {
    rows.iter()
        .flat_map(|row| &row.cells)
        .flat_map(|cell| &cell.lines)
}

fn row_group_end(grid: &[Vec<TableSlot<'_>>], start: usize) -> usize {
    let mut end = start + 1;
    let mut index = start;
    while index < end {
        for slot in &grid[index] {
            end = end.max(slot.origin_row + slot.rowspan);
        }
        index += 1;
    }
    end
}

fn fit_rowspans(rows: &mut [TableRow]) -> f64 {
    let mut required_at_end = vec![Vec::new(); rows.len()];
    for (index, row) in rows.iter().enumerate() {
        for cell in row
            .cells
            .iter()
            .filter(|cell| !cell.covered() && cell.rowspan > 1)
        {
            required_at_end[index + cell.rowspan - 1]
                .push((index, padded_height(cell.lines.len())));
        }
    }
    // Extend at the end of each span, so other spans sharing that row can use the extra height.
    let mut offsets = vec![0.; rows.len() + 1];
    for (index, (row, constraints)) in rows.iter_mut().zip(required_at_end).enumerate() {
        let base = offsets[index];
        let end = constraints
            .into_iter()
            .fold(base + row.height, |end, (start, height)| {
                end.max(offsets[start] + height)
            });
        row.height = end - base;
        offsets[index + 1] = end;
    }
    offsets[rows.len()]
}

impl Builder {
    fn push_table(
        &mut self,
        width: f64,
        columns: usize,
        rows: Vec<TableRow>,
        height: f64,
    ) -> Result<()> {
        self.push(Shape::Table {
            rect: [(W - width) / 2., self.y, width, height],
            rows,
            columns,
            style: self.table_style.clone(),
        })
    }

    pub(super) fn table(
        &mut self,
        geometry: &TableGeometry,
        aligns: &[Align],
        header: &[Vec<Inline>],
        body: &[Vec<Vec<Inline>>],
    ) -> Result<()> {
        let columns = body.iter().map(Vec::len).fold(header.len(), usize::max);
        if columns == 0 {
            return Ok(());
        }
        let max_body_rows = ((BOTTOM - TOP - MIN_ROW_HEIGHT) / MIN_ROW_HEIGHT) as usize;
        if columns > 16 || body.len() > MAX_SLIDES * max_body_rows {
            return Err(BUDGET.into());
        }
        let TableLayout {
            columns,
            rows: grid,
        } = geometry
            .layout(header, body, aligns)
            .map_err(|error| error.code)?;
        let enhanced = geometry.is_enhanced();
        let width = WIDTH * self.table_style.width_pct / 100.;
        let col_width = width / columns as f64;
        let header = TableRow::from_layout(0, &grid[0], col_width, &self.table_style, enhanced)?;
        let merged_header = header.cells.iter().any(|cell| cell.colspan > 1);
        let header_error = if merged_header {
            PUBLICATION_TABLE_UNSUPPORTED
        } else {
            BUDGET
        };
        if !merged_header && header.height > 120. {
            return Err(header_error.into());
        }
        if header.height > BOTTOM - self.y {
            self.continuation()?;
            if header.height > BOTTOM - self.y {
                return Err(header_error.into());
            }
        }
        let mut used = header.height;
        let mut rows = vec![header.clone()];
        let continue_table =
            |builder: &mut Self, rows: &mut Vec<TableRow>, height| -> Result<f64> {
                if rows.len() > 1 {
                    let finished = std::mem::replace(rows, vec![header.clone()]);
                    builder.push_table(width, columns, finished, height)?;
                }
                builder.continuation()?;
                Ok(header.height)
            };

        let mut start = 1;
        while start < grid.len() {
            check()?;
            let end = row_group_end(&grid, start);
            let has_merge = end > start + 1 || grid[start].iter().any(|slot| slot.colspan > 1);
            if has_merge && header.height + (end - start) as f64 * MIN_ROW_HEIGHT > BOTTOM - TOP {
                return Err(PUBLICATION_TABLE_UNSUPPORTED.into());
            }
            let mut group = (start..end)
                .map(|index| {
                    TableRow::from_layout(
                        index,
                        &grid[index],
                        col_width,
                        &self.table_style,
                        enhanced,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            start = end;
            if has_merge {
                let height = fit_rowspans(&mut group);
                if used + height > BOTTOM - self.y {
                    used = continue_table(self, &mut rows, used)?;
                    if used + height > BOTTOM - self.y {
                        return Err(PUBLICATION_TABLE_UNSUPPORTED.into());
                    }
                }
                used += height;
                rows.extend(group);
                continue;
            }

            let row = group.pop().expect("one unmerged row");
            let total = row
                .cells
                .iter()
                .map(|cell| cell.lines.len())
                .max()
                .unwrap_or_default()
                .max(1);
            let mut first_line = 0;
            while first_line < total {
                if BOTTOM - self.y - used < MIN_ROW_HEIGHT {
                    used = continue_table(self, &mut rows, used)?;
                }
                let capacity = ((BOTTOM - self.y - used - 2. * PADDING_Y) / LINE_HEIGHT)
                    .floor()
                    .max(0.) as usize;
                if capacity == 0 {
                    return Err(header_error.into());
                }
                let last_line = (first_line + capacity).min(total);
                let fragment = row.fragment(first_line, last_line);
                used += fragment.height;
                rows.push(fragment);
                first_line = last_line;
            }
        }
        self.push_table(width, columns, rows, used)?;
        self.y += used + 18.;
        Ok(())
    }
}

pub(super) fn preview(rect: Rect, rows: &[TableRow], columns: usize, style: &TableStyle) -> String {
    let mut html = format!(
        "<table style=\"{}table-layout:fixed;border-collapse:collapse;font-size:{FONT_SIZE}px;line-height:{LINE_HEIGHT}px;\"><colgroup>",
        position(rect),
    );
    let column = format!("<col style=\"width:{}%;\"/>", 100. / columns as f64);
    for _ in 0..columns {
        html.push_str(&column);
    }
    html.push_str("</colgroup>");
    for (index, row) in rows.iter().enumerate() {
        let colors = style
            .row_paint(row.kind)
            .map(|paint| {
                format!(
                    "background-color:#{};color:#{};",
                    paint.background, paint.foreground
                )
            })
            .unwrap_or_default();
        html.push_str(&format!("<tr style=\"height:{}px\">", row.height));
        for cell in row.cells.iter().filter(|cell| !cell.covered()) {
            let borders =
                style.border_css(style.borders(index, index + cell.rowspan - 1, rows.len()));
            html.push_str("<td");
            if cell.colspan > 1 {
                html.push_str(&format!(" colspan=\"{}\"", cell.colspan));
            }
            if cell.rowspan > 1 {
                html.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
            }
            html.push_str(&format!(
                " style=\"padding:{PADDING_Y}px {PADDING_X}px;vertical-align:{};{borders}text-align:{};box-sizing:border-box;{colors}\">",
                cell.valign.as_str(), cell.align.css(),
            ));
            for line in &cell.lines {
                html.push_str(&format!(
                    "<div style=\"white-space:pre;min-height:{LINE_HEIGHT}px\">{}</div>",
                    line_html(line)
                ));
            }
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    html
}

pub(super) fn xml(
    rect: Rect,
    rows: &[TableRow],
    columns: usize,
    style: &TableStyle,
    id: usize,
    default_color: &str,
    links: &BTreeMap<String, String>,
) -> String {
    let [x, y, w, h] = rect;
    let mut body = format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"Table {id}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"1\" bandRow=\"0\"/><a:tblGrid>",
        emu(x), emu(y), emu(w), emu(h),
    );
    for _ in 0..columns {
        body.push_str(&format!("<a:gridCol w=\"{}\"/>", emu(w / columns as f64)));
    }
    body.push_str("</a:tblGrid>");
    for (index, row) in rows.iter().enumerate() {
        body.push_str(&format!("<a:tr h=\"{}\">", emu(row.height)));
        for cell in &row.cells {
            let origin = index - cell.row_offset;
            let borders =
                style.border_xml(style.borders(origin, origin + cell.rowspan - 1, rows.len()));
            let paint = style.row_paint(rows[origin].kind);
            let foreground = paint.map_or(default_color, |paint| paint.foreground.as_str());
            let fill = paint
                .map(|paint| {
                    format!(
                        "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                        paint.background
                    )
                })
                .unwrap_or_else(|| "<a:noFill/>".to_owned());
            let anchor = match cell.valign {
                VerticalAlign::Top => "t",
                VerticalAlign::Middle => "ctr",
                VerticalAlign::Bottom => "b",
            };
            let contents = aligned_text_body(
                &cell.lines,
                FONT_SIZE,
                foreground,
                None,
                cell.align,
                anchor,
                links,
            );
            let attributes = cell.merge_attributes();
            let margin_x = emu(PADDING_X);
            let margin_y = emu(PADDING_Y);
            body.push_str(&format!(
                "<a:tc{attributes}><a:txBody>{contents}</a:txBody><a:tcPr marL=\"{margin_x}\" marR=\"{margin_x}\" marT=\"{margin_y}\" marB=\"{margin_y}\" anchor=\"{anchor}\">{borders}{fill}</a:tcPr></a:tc>",
            ));
        }
        body.push_str("</a:tr>");
    }
    body.push_str("</a:tbl></a:graphicData></a:graphic></p:graphicFrame>");
    body
}
