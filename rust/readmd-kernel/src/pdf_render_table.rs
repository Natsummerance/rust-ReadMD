//! Native measured tables with merged cells and page-level repeated headers.

use std::ops::Range;

use crate::{
    api_codes::{PUBLICATION_LAYOUT_GEOMETRY, PUBLICATION_TABLE_UNSUPPORTED},
    table_geometry::{TableGeometry, VerticalAlign, PT_PER_CSS_PX},
};

use super::{
    atoms_from_runs, bool_or, dig, face_for_logical, hex_d, inline_markup_at_size, line_height,
    paragraph_lines, plain_float, publication_paragraph, retarget_runs, ta_get,
    wrap_two_phase_with_min_width, CellGeometry, DocGeometry, DrawItem, EnhancedTableFlow,
    Flowable, Line, PStyle, PdfError, PdfErrorKind, Rgb, Run, RunContent, Sheet, Styles, TableCell,
    TableFlow, TextStyle, Value, FRAME_PAD, TA_LEFT,
};

const EPS: f64 = 1e-6;
const AFTER_TABLE: f64 = 10.0;

fn invalid(code: &str) -> PdfError {
    PdfError::new(PdfErrorKind::Invalid, code)
}

fn rule_radius(table: &TableFlow) -> f64 {
    if table.three_line {
        table.border_w.max(0.25)
    } else {
        table.border_w / 2.0
    }
}

pub(super) fn build_flow(
    block: &Value,
    style: &Value,
    tmpdir: &str,
    resolve: &dyn Fn(&str) -> Option<String>,
    styles: &Styles,
    geom: &DocGeometry,
) -> Result<Flowable, PdfError> {
    let geometry = block
        .get("tableGeometry")
        .map(TableGeometry::from_render_json)
        .transpose()
        .map_err(invalid)?;
    if geometry.is_some()
        && (!block["header"].is_array()
            || !block["rows"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(Value::is_array)))
    {
        return Err(invalid(PUBLICATION_TABLE_UNSUPPORTED));
    }
    let style_table = &style["table"];
    let number = |key, default| plain_float(&style_table[key], default);
    let cell_size = number("cellSize", 10.0);
    let row_values = std::iter::once(&block["header"])
        .chain(block["rows"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    let columns = row_values
        .iter()
        .filter_map(|row| row.as_array().map(Vec::len))
        .max()
        .unwrap_or(0)
        .max(1);
    let grid = geometry
        .as_ref()
        .map(|geometry| geometry.grid(columns, row_values.len()))
        .transpose()
        .map_err(invalid)?;
    let tracks = geometry.filter(TableGeometry::has_track_sizes)
        .map(|geometry| geometry.tracks(columns, row_values.len()));
    let mut data = Vec::with_capacity(row_values.len());
    for row in row_values {
        crate::publication_budget::check().map_err(|code| invalid(&code))?;
        let mut runs = row
            .as_array()
            .into_iter()
            .flatten()
            .map(|cell| inline_markup_at_size(cell, style, tmpdir, resolve, cell_size))
            .collect::<Vec<_>>();
        runs.resize_with(columns, Vec::new);
        data.push(runs);
    }

    let three_line = crate::export_styles::three_line_table(style);
    let body_color = hex_d(dig(style, &["typography", "color"]));
    let header_color = if three_line {
        body_color
    } else {
        hex_d(&style_table["headerColor"])
    };
    let default_align = style_table["align"]
        .as_str()
        .and_then(ta_get)
        .unwrap_or(TA_LEFT);
    let aligns = block["aligns"].as_array();
    let mut rows = Vec::with_capacity(data.len());
    for ri in 0..data.len() {
        let mut row = Vec::with_capacity(columns);
        for ci in 0..columns {
            let cell = grid.as_ref().map(|grid| grid[ri][ci]);
            let column_align = aligns.and_then(|aligns| aligns.get(ci));
            let mut text_style = PStyle::child(&styles.body, "TableCell");
            text_style.size = cell_size;
            text_style.leading = cell_size * 1.35;
            text_style.first_line_indent = 0.0;
            text_style.space_after = 0.0;
            text_style.color = if ri == 0 { header_color } else { body_color };
            text_style.align = if let Some(cell) = cell {
                cell.align
                    .map(|align| align.as_str())
                    .or_else(|| column_align.and_then(Value::as_str))
                    .and_then(ta_get)
                    .unwrap_or(default_align)
            } else {
                column_align.map_or(default_align, |value| {
                    value.as_str().and_then(ta_get).unwrap_or(TA_LEFT)
                })
            };
            let runs = match cell {
                Some(cell) if (ri, ci) != (cell.origin_row, cell.origin_col) => Vec::new(),
                Some(cell) if cell.rowspan > 1 || cell.colspan > 1 => {
                    merged_runs(&mut data, cell, &text_style)
                }
                _ => std::mem::take(&mut data[ri][ci]),
            };
            row.push(TableCell {
                runs: retarget_runs(&runs, text_style.size, text_style.color),
                st: text_style,
            });
        }
        rows.push(row);
    }

    let available_width = geom.avail * number("widthPct", 100.0) / 100.0;
    let table = TableFlow {
        three_line,
        rows,
        col_w: match &tracks {
            Some(tracks) => tracks.column_widths_pt(available_width),
            None => vec![available_width / columns as f64; columns],
        },
        repeat_rows: 1,
        header_bg: hex_d(&style_table["headerBg"]),
        border_w: number("borderWidth", 0.75),
        border_color: hex_d(&style_table["borderColor"]),
        pad_top: number("cellPadding", 6.0),
        pad_bottom: number("cellPadding", 6.0),
        pad_left: 6.0,
        pad_right: 6.0,
        banded: bool_or(&style_table["banded"], true),
        band_color: hex_d(&style_table["bandColor"]),
        all_bg: None,
        line_before: None,
    };
    Ok(match grid {
        Some(grid) => Flowable::EnhancedTable(EnhancedTableFlow {
            table,
            grid,
            row_heights_px: tracks.map_or_else(Vec::new, |tracks| tracks.row_heights),
        }),
        None => Flowable::Table(table),
    })
}

fn merged_runs(data: &mut [Vec<Vec<Run>>], cell: CellGeometry, st: &PStyle) -> Vec<Run> {
    let mut merged = Vec::new();
    for row in &mut data[cell.origin_row..cell.origin_row + cell.rowspan] {
        for runs in &mut row[cell.origin_col..cell.origin_col + cell.colspan] {
            if runs
                .iter()
                .all(|run| matches!(&run.content, RunContent::Text(text) if text.trim().is_empty()))
            {
                continue;
            }
            if !merged.is_empty() {
                merged.push(Run {
                    content: RunContent::Break { hard: true },
                    face: st.face,
                    size: st.size,
                    color: st.color,
                    href: None,
                    text_style: TextStyle::default(),
                });
            }
            merged.append(runs);
        }
    }
    merged
}

struct CellLine {
    line: Line,
    /// Downward coordinates from the top of the logical table.
    top: f64,
    height: f64,
    ascent: f64,
    indent: f64,
    ink: publication_paragraph::Extent,
}

impl CellLine {
    fn bottom(&self) -> f64 {
        self.top + self.height
    }

    fn visible(&self) -> bool {
        !self.line.atoms.is_empty()
    }
}

struct MeasuredCell<'a> {
    rows: Range<usize>,
    columns: Range<usize>,
    style: &'a PStyle,
    valign: VerticalAlign,
    height: f64,
    lines: Vec<CellLine>,
}

impl<'a> MeasuredCell<'a> {
    fn new(
        cell: &'a TableCell,
        geometry: CellGeometry,
        inner_width: f64,
        table: &TableFlow,
        enhanced: bool,
    ) -> Result<Self, PdfError> {
        let st = &cell.st;
        let mut lines = Vec::new();
        if enhanced {
            let width = inner_width - st.left_indent - st.right_indent;
            wrap_two_phase_with_min_width(
                &atoms_from_runs(&cell.runs),
                width - st.first_line_indent,
                width,
                &mut lines,
                0.0,
            );
        } else {
            lines = paragraph_lines(&cell.runs, st, inner_width.max(1.0));
        }
        let face_ascent = st.size * face_for_logical(&st.font_name).asc1000() / 1000.0;
        let ink_margin = rule_radius(table) + 0.25;
        let mut y = st.space_before;
        let lines = lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let first_indent = if index == 0 {
                    st.first_line_indent
                } else {
                    0.0
                };
                let indent = st.left_indent + first_indent + line.indent;
                let available = inner_width - indent - st.right_indent;
                if (enhanced || line.image_h > 0.0)
                    && !line.atoms.is_empty()
                    && (!available.is_finite() || available <= 0.0 || line.width > available + EPS)
                {
                    return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
                }
                let mut ascent = face_ascent;
                let mut height = line_height(&line, st);
                let ink = publication_paragraph::paint_extent(&line.atoms);
                if line
                    .atoms
                    .iter()
                    .any(|a| a.image.is_some() || a.rise != 0.0 || a.author_size)
                {
                    ascent = ascent.max(ink.upper);
                    height = height.max(ascent + ink.lower);
                }
                if !line.atoms.is_empty() {
                    // Keep the ordinary baseline when the authored padding
                    // already encloses the ink, even with a thick rule.
                    ascent = ascent.max(ink.upper + ink_margin - table.pad_top);
                    height = height.max(ascent + ink.lower + ink_margin - table.pad_bottom);
                }
                if !height.is_finite() || height <= EPS {
                    return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
                }
                let measured = CellLine {
                    line,
                    top: y,
                    height,
                    ascent,
                    indent,
                    ink,
                };
                y += height;
                Ok(measured)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            rows: geometry.origin_row..geometry.origin_row + geometry.rowspan,
            columns: geometry.origin_col..geometry.origin_col + geometry.colspan,
            style: st,
            valign: geometry.valign.unwrap_or(VerticalAlign::Top),
            height: (y + st.space_after).max(st.leading),
            lines,
        })
    }

    fn line_index(&self, bottom: f64) -> usize {
        self.lines
            .partition_point(|line| line.bottom() <= bottom + EPS)
    }
}

pub(super) struct MeasuredTable<'a> {
    source: &'a TableFlow,
    /// Start of each column/row plus the outer edge at the end.
    x: Vec<f64>,
    y: Vec<f64>,
    cells: Vec<MeasuredCell<'a>>,
    row_cells: Vec<Vec<usize>>,
    body_capacity: f64,
}

impl<'a> MeasuredTable<'a> {
    pub(super) fn for_flow(
        flow: &'a Flowable,
        geom: &DocGeometry,
    ) -> Result<Option<Self>, PdfError> {
        let (table, grid, row_heights_px) = match flow {
            Flowable::Table(table)
                if table.three_line && table.repeat_rows == 1 && !table.rows.is_empty() =>
            {
                (table, None, &[][..])
            }
            Flowable::EnhancedTable(flow) => (
                &flow.table,
                Some(flow.grid.as_slice()),
                flow.row_heights_px.as_slice(),
            ),
            _ => return Ok(None),
        };
        Self::measure(table, grid, row_heights_px, geom).map(Some)
    }

    fn measure(
        table: &'a TableFlow,
        grid: Option<&[Vec<CellGeometry>]>,
        row_heights_px: &[Option<usize>],
        geom: &DocGeometry,
    ) -> Result<Self, PdfError> {
        let frame = geom.height - 2.0 * FRAME_PAD;
        let x = edges(&table.col_w);
        let width = x.last().copied().unwrap_or(0.0);
        if !frame.is_finite()
            || frame <= EPS
            || !geom.avail.is_finite()
            || table.col_w.iter().any(|w| !w.is_finite() || *w <= 0.0)
            || width <= 0.0
            || width > geom.avail + EPS
            || [
                table.pad_top,
                table.pad_bottom,
                table.pad_left,
                table.pad_right,
                table.border_w,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
        }
        let padding = table.pad_top + table.pad_bottom;
        let mut heights = Vec::with_capacity(table.rows.len());
        let mut cells = Vec::new();
        let mut row_cells = vec![Vec::new(); table.rows.len()];
        for (ri, row) in table.rows.iter().enumerate() {
            crate::publication_budget::check().map_err(|code| invalid(&code))?;
            let strut = row.first().map(|cell| cell.st.leading).unwrap_or(0.0);
            if row.len() != table.col_w.len() || !strut.is_finite() || strut <= EPS {
                return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
            }
            let declared = row_heights_px.get(ri).copied().flatten()
                .map_or(0.0, |height| height as f64 * PT_PER_CSS_PX);
            heights.push((strut + padding).max(declared));
            for (ci, source_cell) in row.iter().enumerate() {
                let geometry = grid.map_or(
                    CellGeometry {
                        origin_row: ri,
                        origin_col: ci,
                        rowspan: 1,
                        colspan: 1,
                        align: None,
                        valign: None,
                    },
                    |grid| grid[ri][ci],
                );
                if (ri, ci) != (geometry.origin_row, geometry.origin_col) {
                    continue;
                }
                let inner = x[ci + geometry.colspan] - x[ci] - table.pad_left - table.pad_right;
                let cell = MeasuredCell::new(source_cell, geometry, inner, table, grid.is_some())?;
                if geometry.rowspan == 1 {
                    heights[ri] = heights[ri].max(cell.height + padding);
                }
                for row in &mut row_cells[cell.rows.clone()] {
                    row.push(cells.len());
                }
                cells.push(cell);
            }
        }
        for cell in cells.iter().filter(|cell| cell.rows.len() > 1) {
            let rows = &mut heights[cell.rows.clone()];
            let shortage = cell.height + padding - rows.iter().sum::<f64>();
            if shortage > 0.0 {
                let extra = shortage / rows.len() as f64;
                rows.iter_mut().for_each(|height| *height += extra);
            }
        }
        let y = edges(&heights);
        let body_capacity = frame - y[1];
        let has_body = table.rows.len() > 1;
        if y.last().is_some_and(|y| !y.is_finite())
            || body_capacity < -EPS
            || has_body && body_capacity <= EPS
        {
            return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
        }
        for cell in &mut cells {
            let height = y[cell.rows.end] - y[cell.rows.start];
            let free = (height - cell.height - padding).max(0.0);
            let shift = match cell.valign {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Middle => free / 2.0,
                VerticalAlign::Bottom => free,
            };
            let origin = y[cell.rows.start] + table.pad_top + shift;
            for line in &mut cell.lines {
                if cell.rows.start > 0 && line.height > body_capacity + EPS {
                    return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
                }
                line.top += origin;
            }
        }
        for row in &mut row_cells {
            row.sort_unstable_by_key(|&index| cells[index].columns.start);
        }
        Ok(Self {
            source: table,
            x,
            y,
            cells,
            row_cells,
            body_capacity,
        })
    }

    pub(super) fn first_height(&self) -> f64 {
        self.y[1]
    }

    fn group_end(&self, start: usize) -> usize {
        // Overlapping rowspans in different columns can connect many rows.
        let mut end = start + 1;
        let mut row = start;
        while row < end {
            for &cell in &self.row_cells[row] {
                end = end.max(self.cells[cell].rows.end);
            }
            row += 1;
        }
        end
    }

    fn row_at(&self, y: f64) -> usize {
        self.y.partition_point(|edge| *edge <= y) - 1
    }

    fn carried_height(&self, start: f64) -> f64 {
        self.row_cells[self.row_at(start)]
            .iter()
            .filter_map(|&index| {
                let cell = &self.cells[index];
                cell.lines.get(cell.line_index(start))
            })
            .fold(0.0, |height, line| height.max(start - line.top))
    }

    fn minimum_body_height(&self, row: usize) -> f64 {
        let top = self.y[row];
        self.row_cells[row]
            .iter()
            .filter_map(|&index| self.cells[index].lines.iter().find(|line| line.visible()))
            .fold(self.y[row + 1] - top, |height, line| {
                height.min(line.bottom() - top)
            })
    }

    fn intersecting_cells(&self, span: &Range<f64>) -> impl Iterator<Item = &MeasuredCell<'_>> {
        let start = self.row_at(span.start);
        let end = self.y.partition_point(|edge| *edge < span.end);
        (start..end).flat_map(move |row| {
            self.row_cells[row].iter().filter_map(move |&index| {
                let cell = &self.cells[index];
                (row == start || cell.rows.start == row).then_some(cell)
            })
        })
    }

    fn has_visible_line(&self, span: &Range<f64>) -> bool {
        self.intersecting_cells(span).any(|cell| {
            cell.lines[cell.line_index(span.start)..cell.line_index(span.end)]
                .iter()
                .any(CellLine::visible)
        })
    }

    fn split_insets(&self, span: &Range<f64>) -> Result<(f64, f64), PdfError> {
        // The logical line boxes preserve the usual table leading. Use ink
        // bounds here because a split may put the ink next to a page rule
        // without the original cell padding on either side.
        let mut upper = 0.0f64;
        let mut lower = 0.0f64;
        let mut tallest = 0.0f64;
        for cell in self.intersecting_cells(span) {
            for line in &cell.lines {
                upper = upper.max(line.ink.upper - line.ascent);
                lower = lower.max(line.ascent + line.ink.lower - line.height);
                tallest = tallest.max(line.height);
            }
        }
        let rule = rule_radius(self.source);
        upper += rule + 0.25;
        lower += rule + 0.25;
        let room = self.body_capacity - tallest - upper - lower;
        if room < -EPS {
            return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
        }

        let upper_extra = (self.source.pad_top - upper).max(0.0);
        let lower_extra = (self.source.pad_bottom - lower).max(0.0);
        let extra = upper_extra + lower_extra;
        if extra > room && extra > 0.0 {
            let scale = room.max(0.0) / extra;
            Ok((upper + scale * upper_extra, lower + scale * lower_extra))
        } else {
            Ok((upper + upper_extra, lower + lower_extra))
        }
    }

    fn left(&self, sheet: &Sheet<'_>) -> f64 {
        sheet.x0() + ((sheet.avail() - self.x[self.x.len() - 1]) / 2.0).max(0.0)
    }

    fn background(&self, row: usize) -> Option<Rgb> {
        if self.source.three_line {
            None
        } else {
            self.source.all_bg.or_else(|| match row {
                0 => Some(self.source.header_bg),
                n if self.source.banded && n % 2 == 0 => Some(self.source.band_color),
                _ => None,
            })
        }
    }

    fn rule(&self, sheet: &mut Sheet<'_>, x1: f64, y1: f64, x2: f64, y2: f64, width: f64) {
        sheet.items.push(DrawItem::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            color: self.source.border_color,
        });
    }

    fn horizontal_rule(&self, sheet: &mut Sheet<'_>, y: f64, width: f64) {
        if width > 0.0 {
            let left = self.left(sheet);
            self.rule(sheet, left, y, left + self.x[self.x.len() - 1], y, width);
        }
    }

    fn paint_interval(
        &self,
        sheet: &mut Sheet<'_>,
        span: Range<f64>,
        inset_top: f64,
        inset_bottom: f64,
    ) {
        let origin_x = self.left(sheet);
        let frame_top = sheet.y;
        let origin_y = frame_top + span.start - inset_top;
        let table = self.source;
        let horizontal = |cell: &MeasuredCell<'_>| {
            (
                origin_x + self.x[cell.columns.start],
                origin_x + self.x[cell.columns.end],
            )
        };
        let vertical = |cell: &MeasuredCell<'_>| {
            let cell_top = self.y[cell.rows.start];
            let top = if cell_top <= span.start {
                frame_top
            } else {
                origin_y - cell_top
            };
            let logical_bottom = self.y[cell.rows.end];
            let bottom = if logical_bottom >= span.end {
                origin_y - span.end - inset_bottom
            } else {
                origin_y - logical_bottom
            };
            (bottom, top)
        };

        // Paint the uninterrupted owner background throughout a merged region.
        // At an internal row boundary only newly starting cells have a top edge.
        for cell in self.intersecting_cells(&span) {
            if let Some(color) = self.background(cell.rows.start) {
                let (left, right) = horizontal(cell);
                let (bottom, top) = vertical(cell);
                sheet.rect(left, bottom, right - left, top - bottom, color);
            }
        }
        for cell in self.intersecting_cells(&span) {
            let (left, right) = horizontal(cell);
            let width = right - left - table.pad_left - table.pad_right;
            for line in &cell.lines[cell.line_index(span.start)..cell.line_index(span.end)] {
                sheet.draw_line(
                    &line.line.atoms,
                    left + table.pad_left + line.indent,
                    (width - line.indent - cell.style.right_indent).max(0.0),
                    cell.style.align,
                    origin_y - line.top - line.ascent,
                );
                sheet.texts.push(line.line.text.clone());
            }
            if !table.three_line && table.border_w > 0.0 {
                let (bottom, top) = vertical(cell);
                self.rule(sheet, left, top, right, top, table.border_w);
                self.rule(sheet, left, bottom, left, top, table.border_w);
                if cell.columns.end == self.x.len() - 1 {
                    self.rule(sheet, right, bottom, right, top, table.border_w);
                }
            }
        }
        sheet.y = origin_y - span.end - inset_bottom;
        sheet.placed += 1;
    }

    fn paint_header(&self, sheet: &mut Sheet<'_>) {
        sheet.begin();
        let top = sheet.y;
        self.paint_interval(sheet, 0.0..self.y[1], 0.0, 0.0);
        if self.source.three_line {
            let width = self.source.border_w.max(0.25);
            self.horizontal_rule(sheet, top, 2.0 * width);
            if self.row_cells.len() > 1 {
                self.horizontal_rule(sheet, sheet.y, width);
            }
        }
    }

    fn finish_fragment(&self, sheet: &mut Sheet<'_>) {
        let width = 2.0 * rule_radius(self.source);
        self.horizontal_rule(sheet, sheet.y, width);
        sheet.note_paint_bottom(sheet.y - width / 2.0);
    }

    fn paint_oversized_group(
        &self,
        sheet: &mut Sheet<'_>,
        span: Range<f64>,
        top_inset: f64,
        bottom_inset: f64,
    ) -> Result<(), PdfError> {
        let mut start = span.start;
        loop {
            crate::publication_budget::check().map_err(|code| invalid(&code))?;
            // A page cut can cross lines in several active cells. Offset the
            // next page by the largest unfinished line's overhang so every
            // line is drawn once, in full, inside the same owner's rectangle.
            let top = if start == span.start {
                0.0
            } else {
                self.carried_height(start) + top_inset
            };
            let available_end = start + sheet.room() - top;
            if available_end + EPS >= span.end {
                self.paint_interval(sheet, start..span.end, top, 0.0);
                return Ok(());
            }
            let available_end = available_end - bottom_inset;
            if available_end <= start {
                return Err(invalid(PUBLICATION_LAYOUT_GEOMETRY));
            }
            // Prefer the last whole row that fits. When the current row is
            // itself taller than the frame, continue inside it.
            let row_edge = self.y[self.row_at(available_end + EPS)];
            let end = if row_edge > start && self.has_visible_line(&(start..row_edge)) {
                row_edge
            } else {
                available_end
            };
            self.paint_interval(sheet, start..end, top, bottom_inset);
            self.finish_fragment(sheet);
            sheet.end();
            self.paint_header(sheet);
            start = end;
        }
    }

    pub(super) fn paint(&self, sheet: &mut Sheet<'_>) -> Result<(), PdfError> {
        let mut header_on_page = false;
        let mut row = 1;
        while row < self.row_cells.len() {
            crate::publication_budget::check().map_err(|code| invalid(&code))?;
            let end = self.group_end(row);
            let span = self.y[row]..self.y[end];
            let height = span.end - span.start;
            let split = (height > self.body_capacity + EPS)
                .then(|| self.split_insets(&span))
                .transpose()?;
            let body_start = split.map_or(height, |(_, bottom)| {
                (self.minimum_body_height(row) + bottom).min(self.body_capacity)
            });
            let required = body_start + if header_on_page { 0.0 } else { self.y[1] };
            if sheet.room() + EPS < required {
                if header_on_page {
                    self.finish_fragment(sheet);
                }
                sheet.end();
                header_on_page = false;
            }
            if !header_on_page {
                self.paint_header(sheet);
                header_on_page = true;
            }
            if let Some((top, bottom)) = split {
                self.paint_oversized_group(sheet, span, top, bottom)?;
            } else {
                self.paint_interval(sheet, span, 0.0, 0.0);
            }
            row = end;
        }
        if !header_on_page {
            if sheet.room() + EPS < self.y[1] {
                sheet.end();
            }
            self.paint_header(sheet);
        }
        self.finish_fragment(sheet);
        // Trailing spacing uses the remainder of this frame without opening
        // a page on its own when the last row reaches the lower margin.
        sheet.y = (sheet.y - AFTER_TABLE).max(sheet.limit());
        Ok(())
    }
}

fn edges(sizes: &[f64]) -> Vec<f64> {
    let mut edges = Vec::with_capacity(sizes.len() + 1);
    edges.push(0.0);
    for &size in sizes {
        edges.push(edges.last().copied().unwrap_or(0.0) + size);
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_table_track_sizes_measure_source_columns_rows_and_repeated_headers() {
        let mut source = concat!(
            "| Head {width=160 height=120 colspan=2} | Pair {width=96} | Edge |\n",
            "| --- | --- | --- |\n",
            "| One {rowspan=2 colspan=2 height=80} | Two | A |\n",
            "| Three {height=112} | Four | B |\n",
            "| Short {height=20} | Untouched | C |\n",
        ).to_string();
        for index in 0..9 {
            source.push_str(&format!("| Body{index} {{height=112}} | D | E |\n"));
        }
        let style = crate::export_styles::sanitize(&serde_json::json!({
            "page": {"size":"A5", "marginTop":16, "marginBottom":16, "marginLeft":16, "marginRight":16},
            "table": {"style":"grid", "widthPct":100, "cellSize":10, "cellPadding":6},
            "cover": {"enabled":false}, "toc": {"enabled":false}
        }));
        let geom = DocGeometry::from_style(&style);
        let styles = Styles::build(&style, "Helvetica");
        let blocks = crate::md_ast::to_render_json(&crate::md_ast::parse(&source).blocks);
        let flow = build_flow(&blocks[0], &style, "", &|_| None, &styles, &geom).unwrap();
        let measured = MeasuredTable::for_flow(&flow, &geom).unwrap().unwrap();
        assert_eq!(&measured.x[..3], &[0., 120., 192.]);
        assert!((measured.x[3] - geom.avail).abs() < EPS);
        assert_eq!(&measured.y[..4], &[0., 90., 150., 234.]);
        assert!(measured.y[4] - measured.y[3] >= 25.5);

        let doc = super::super::build(&blocks, &style, "", &|_| None, &mut Vec::new()).unwrap();
        assert!(doc.pages.len() >= 2);
        let mut painted = Vec::new();
        for page in &doc.pages {
            let top = doc.geom.page_h - doc.geom.top - FRAME_PAD;
            let full_rule_at = |y: f64| page.items.iter().any(|item| matches!(item,
                DrawItem::Line { x1, y1, x2, y2, .. }
                    if (*y1 - y).abs() < EPS && (*y2 - y).abs() < EPS && (x2 - x1).abs() > EPS
            ));
            assert!(full_rule_at(top));
            assert!(full_rule_at(top - 90.0));
            let text: Vec<_> = page.items.iter().filter_map(|item| match item {
                DrawItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            }).collect();
            assert_eq!(text.iter().filter(|&&text| text == "Head").count(), 1);
            painted.extend(text);
        }
        assert_eq!(painted.iter().copied().filter(|text| matches!(*text, "One" | "Two" | "Three" | "Four")).collect::<Vec<_>>(), ["One", "Two", "Three", "Four"]);
        for index in 0..9 {
            let expected = format!("Body{index}");
            assert_eq!(painted.iter().filter(|&&text| text == expected.as_str()).count(), 1);
        }
    }
}
