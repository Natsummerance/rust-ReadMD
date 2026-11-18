//! Table styling shared by the editable slides and their preview.
use super::emu;
use crate::md_ast::Align;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TableAlign {
    Left,
    Center,
    Right,
    Justify,
}

impl TableAlign {
    fn option(value: &str) -> Self {
        match value {
            "center" => Self::Center,
            "right" => Self::Right,
            "justify" => Self::Justify,
            _ => Self::Left,
        }
    }

    pub(super) fn column(value: Align, fallback: Self) -> Self {
        match value {
            Align::Left => Self::Left,
            Align::Center => Self::Center,
            Align::Right => Self::Right,
            Align::None => fallback,
        }
    }

    pub(super) fn css(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Justify => "justify",
        }
    }

    pub(super) fn xml(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "ctr",
            Self::Right => "r",
            Self::Justify => "just",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TableStyle {
    three_line: bool,
    border_color: String,
    border_width: f64,
    pub(super) default_align: TableAlign,
    pub(super) width_pct: f64,
    pub(super) header_bold: bool,
    header: Option<TablePaint>,
    band: Option<TablePaint>,
}

#[derive(Clone, Debug)]
pub(super) struct TablePaint {
    pub(super) foreground: String,
    pub(super) background: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TableRowKind {
    Header,
    Body,
    Band,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TableBorders {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

fn srgb_luminance(color: &str) -> f64 {
    let rgb = u32::from_str_radix(color, 16).expect("sanitized table or slide color");
    let linear = |shift: u32| {
        let channel = f64::from((rgb >> shift) & 0xff) / 255.;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(16) + 0.7152 * linear(8) + 0.0722 * linear(0)
}

fn band_foreground<'a>(background: &str, slide_foreground: &'a str) -> &'a str {
    let background = srgb_luminance(background) + 0.05;
    let foreground = srgb_luminance(slide_foreground) + 0.05;
    if background.max(foreground) / background.min(foreground) >= 4.5 {
        slide_foreground
    } else if background / 0.05 >= 1.05 / background {
        "000000"
    } else {
        "FFFFFF"
    }
}

impl TableStyle {
    pub(super) fn from_options(options: &Value, slide_foreground: &str) -> Self {
        let style = crate::export_styles::sanitize(options);
        let three_line = crate::export_styles::three_line_table(&style);
        let table = &style["table"];
        let color = |key: &str, fallback: &str| {
            table[key]
                .as_str()
                .unwrap_or(fallback)
                .trim()
                .trim_start_matches('#')
                .to_ascii_uppercase()
        };
        let width = table["borderWidth"].as_f64().unwrap_or(0.75);
        let header = (!three_line).then(|| TablePaint {
            foreground: color("headerColor", "#ffffff"),
            background: color("headerBg", "#3b6ef5"),
        });
        let band = (!three_line && table["banded"].as_bool().unwrap_or(true)).then(|| {
            let background = color("bandColor", "#f3f5f9");
            let foreground = band_foreground(&background, slide_foreground).to_owned();
            TablePaint {
                foreground,
                background,
            }
        });
        Self {
            three_line,
            border_color: color("borderColor", "#c8cdd4"),
            border_width: if three_line {
                width.clamp(0.25, 3.)
            } else {
                width
            },
            default_align: TableAlign::option(table["align"].as_str().unwrap_or("left")),
            width_pct: table["widthPct"].as_f64().unwrap_or(100.),
            header_bold: table["headerBold"].as_bool().unwrap_or(true),
            header,
            band,
        }
    }

    pub(super) fn row_paint(&self, kind: TableRowKind) -> Option<&TablePaint> {
        match kind {
            TableRowKind::Header => self.header.as_ref(),
            TableRowKind::Body => None,
            TableRowKind::Band => self.band.as_ref(),
        }
    }

    pub(super) fn borders(&self, first_row: usize, last_row: usize, rows: usize) -> TableBorders {
        let width = self.border_width;
        if !self.three_line {
            return TableBorders {
                left: width,
                right: width,
                top: width,
                bottom: width,
            };
        }
        TableBorders {
            left: 0.,
            right: 0.,
            top: if first_row == 0 { width * 2. } else { 0. },
            bottom: if last_row + 1 == rows {
                width * 2.
            } else if last_row == 0 {
                width
            } else {
                0.
            },
        }
    }

    pub(super) fn border_css(&self, borders: TableBorders) -> String {
        [
            ("left", borders.left),
            ("right", borders.right),
            ("top", borders.top),
            ("bottom", borders.bottom),
        ]
        .iter()
        .map(|(side, width)| {
            if *width == 0. {
                format!("border-{side}:0;")
            } else {
                format!("border-{side}:{width}pt solid #{};", self.border_color)
            }
        })
        .collect()
    }

    pub(super) fn border_xml(&self, borders: TableBorders) -> String {
        [
            ("L", borders.left),
            ("R", borders.right),
            ("T", borders.top),
            ("B", borders.bottom),
        ]
        .iter()
        .map(|(side, width)| {
            if *width == 0. {
                format!("<a:ln{side}><a:noFill/></a:ln{side}>")
            } else {
                format!(
                    "<a:ln{side} w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln{side}>",
                    emu(*width),
                    self.border_color,
                )
            }
        })
        .collect()
    }
}
