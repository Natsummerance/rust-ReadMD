//! Editable pressure drawings. Only the validated model produces visible SVG.
//! Opening an externally changed SVG never lets stale metadata replace its art.
use crate::{
    document_assets,
    server::{Request, Response},
    ApiError, ApiResult, App,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fmt::Write, sync::Arc};

const INVALID: &str = "invalid_parameter";
const LIMIT: &str = "review_budget";
// Bound drawings to the shared SVG rasterizer’s input budget.
const MAX_SVG: usize = 4 * 1024 * 1024;
const META: &str = "<metadata id=\"readmd-ink-v1\">";

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Drawing {
    schema: u8,
    width: u16,
    height: u16,
    layers: Vec<Layer>,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Layer {
    name: String,
    visible: bool,
    opacity: u8,
    strokes: Vec<Stroke>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    mask: Vec<Stroke>,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Stroke {
    color: String,
    width: f32,
    points: Vec<[f32; 3]>,
}

impl Drawing {
    fn validate(&self) -> Result<(), &'static str> {
        if self.schema != 1
            || !(1..=8192).contains(&self.width)
            || !(1..=8192).contains(&self.height)
            || !(1..=32).contains(&self.layers.len())
        {
            return Err(INVALID);
        }
        let mut points = 0;
        let mut strokes = 0;
        for layer in &self.layers {
            if layer.name.trim().is_empty()
                || layer.name.chars().count() > 100
                || layer.opacity > 100
                || layer
                    .name
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}'))
            {
                return Err(INVALID);
            }
            for stroke in layer.strokes.iter().chain(&layer.mask) {
                strokes += 1;
                points += stroke.points.len();
                if points > 48_000 || strokes > 4096 {
                    return Err(LIMIT);
                }
                if stroke.points.is_empty()
                    || !(0.25..=128.0).contains(&stroke.width)
                    || stroke.color.len() != 7
                    || !stroke.color.starts_with('#')
                    || !stroke.color[1..].bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return Err(INVALID);
                }
                for &[x, y, p] in &stroke.points {
                    if !(0.0..=f32::from(self.width)).contains(&x)
                        || !(0.0..=f32::from(self.height)).contains(&y)
                        || !(0.0..=1.0).contains(&p)
                    {
                        return Err(INVALID);
                    }
                }
            }
        }
        Ok(())
    }

    fn svg(&self) -> Result<String, &'static str> {
        self.validate()?;
        let (w, h) = (self.width, self.height);
        let state = STANDARD.encode(serde_json::to_vec(self).map_err(|_| INVALID)?);
        let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n{META}{state}</metadata>\n");
        for (i, layer) in self.layers.iter().enumerate() {
            if !layer.mask.is_empty() {
                writeln!(out,"<defs><mask id=\"cut{i}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>").unwrap();
                for erase in &layer.mask {
                    erase.paint(&mut out, "#000000");
                }
                out.push_str("</mask></defs>\n");
            }
            write!(
                out,
                "<g id=\"layer{i}\" opacity=\"{:.2}\"",
                f32::from(layer.opacity) / 100.
            )
            .unwrap();
            if !layer.visible {
                out.push_str(" display=\"none\"");
            }
            if !layer.mask.is_empty() {
                write!(out, " mask=\"url(#cut{i})\"").unwrap();
            }
            writeln!(
                out,
                "><title>{}</title>",
                quick_xml::escape::escape(&layer.name)
            )
            .unwrap();
            for stroke in &layer.strokes {
                stroke.paint(&mut out, &stroke.color);
            }
            out.push_str("</g>\n");
            if out.len() > MAX_SVG {
                return Err(LIMIT);
            }
        }
        out.push_str("</svg>\n");
        if out.len() > MAX_SVG {
            return Err(LIMIT);
        }
        Ok(out)
    }

    fn open(svg: &str) -> Result<Self, &'static str> {
        if svg.len() > MAX_SVG {
            return Err(LIMIT);
        }
        let encoded = svg
            .split_once(META)
            .and_then(|(_, tail)| tail.split_once("</metadata>"))
            .map(|(v, _)| v)
            .ok_or("unsupported_format")?;
        let bytes = STANDARD.decode(encoded).map_err(|_| INVALID)?;
        let doc: Self = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
        // A foreign path, image, script or changed geometry also fails here.
        if doc.svg()? != svg {
            return Err("review_conflict");
        }
        Ok(doc)
    }
}

/// Decode compatible paths represented by one native, constant-width InkList.
pub(crate) fn pdf_stroke(svg: &str) -> Result<serde_json::Value, &'static str> {
    let drawing = Drawing::open(svg)?;
    let unsupported = "pdf_unsupported_object";
    let mut paths = Vec::new();
    let mut style: Option<(&str, f32)> = None;
    let mut points = 0;
    for layer in drawing.layers.iter().filter(|l| l.visible && l.opacity > 0 && !l.strokes.is_empty()) {
        if !layer.mask.is_empty() || layer.opacity != 100 { return Err(unsupported); }
        for stroke in &layer.strokes {
            let pressure = stroke.points[0][2];
            let diameter = 2.0 * stroke.radius(pressure);
            if stroke.points.len() < 2 || stroke.points.iter().any(|p| p[2] != pressure) { return Err(unsupported); }
            if let Some((color, width)) = style {
                if !color.eq_ignore_ascii_case(&stroke.color) || diameter != width { return Err(unsupported); }
            } else { style = Some((&stroke.color, diameter)); }
            points += stroke.points.len();
            if paths.len() >= 128 || points > 4096 { return Err("pdf_limit"); }
            paths.push(stroke.points.iter().map(|p| [p[0], p[1]]).collect::<Vec<_>>());
        }
    }
    let (color, diameter) = style.ok_or(unsupported)?;
    Ok(
        json!({"width":drawing.width,"height":drawing.height,"color":color,
        "strokeWidth":diameter,"strokes":paths}),
    )
}

impl Stroke {
    fn radius(&self, pressure: f32) -> f32 {
        self.width * (0.15 + 0.85 * pressure) * 0.5
    }

    // Union of round dabs and their connecting trapezoids. One filled path
    // gives overlapping joins a single opacity and covers reversals/corners.
    fn paint(&self, out: &mut String, color: &str) {
        write!(out, "<path fill=\"{color}\" d=\"").unwrap();
        let mut previous = None;
        for &[x, y, p] in &self.points {
            let r = self.radius(p);
            // Clockwise circle and strip winding keep overlaps filled.
            write!(
                out,
                "M{:.2} {y:.2}a{r:.2} {r:.2} 0 1 1 {:.2} 0a{r:.2} {r:.2} 0 1 1 {:.2} 0Z",
                x - r,
                2. * r,
                -2. * r
            )
            .unwrap();
            if let Some((ax, ay, ar)) = previous {
                let (dx, dy): (f32, f32) = (x - ax, y - ay);
                let length = dx.hypot(dy);
                if length > 0.0001 {
                    let (nx, ny) = (-dy / length, dx / length);
                    write!(
                        out,
                        "M{:.2} {:.2}L{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}Z",
                        ax + nx * ar,
                        ay + ny * ar,
                        ax - nx * ar,
                        ay - ny * ar,
                        x - nx * r,
                        y - ny * r,
                        x + nx * r,
                        y + ny * r
                    )
                    .unwrap();
                }
            }
            previous = Some((x, y, r));
        }
        out.push_str("\"/>\n");
    }
}

pub(crate) fn handle(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    if req.body.len() > MAX_SVG + 1024 * 1024 {
        return Err(ApiError::new(413, LIMIT));
    }
    let mut body = req.json()?;
    let err = crate::review_api::error;
    if body["action"] == "open" {
        let svg = body["svg"].as_str().ok_or_else(|| err(INVALID))?;
        let doc = Drawing::open(svg).map_err(err)?;
        return Ok(Response::json(&json!({"ok":true,"document":doc})));
    }
    let stage = match body["action"].as_str() {
        Some("render") => false,
        Some("stage") => true,
        _ => return Err(err(INVALID)),
    };
    let drawing: Drawing =
        serde_json::from_value(body["document"].take()).map_err(|_| err(INVALID))?;
    let svg = drawing.svg().map_err(err)?;
    if !stage {
        return Ok(Response::json(&json!({"ok":true,"svg":svg})));
    }
    let asset = document_assets::stage(&app.paths.data_dir, svg.as_bytes(), "drawing", "svg")
        .map_err(|e| ApiError::new(422, e))?;
    Ok(Response::json(
        &json!({"ok":true,"asset":asset,"rel":asset.reference}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing() -> Drawing {
        serde_json::from_value(json!({
            "schema":1,"width":120,"height":80,
            "layers":[{"name":"蓝墨 & 蓝纸","visible":true,"opacity":100,
                "strokes":[{"width":20,"color":"#0033ff","points":[[10,40,0.2],[90,40,1]]}],
                "mask":[{"width":12,"color":"#ffffff","points":[[88,40,1]]}]}]
        }))
        .unwrap()
    }

    #[test]
    fn owned_svg_reopens_only_when_geometry_and_editable_state_agree() {
        let doc = drawing();
        let svg = doc.svg().unwrap();
        assert!(svg.contains("蓝墨 &amp; 蓝纸") && svg.contains("<mask"));
        crate::svg_render::validate(svg.as_bytes()).unwrap();
        assert_eq!(Drawing::open(&svg).unwrap().svg().unwrap(), svg);
        assert!(matches!(
            Drawing::open(&svg.replace("fill=\"#0033ff\"", "fill=\"red\"")),
            Err("review_conflict")
        ));
        let mut invalid = drawing();
        invalid.layers[0].strokes[0].points[0][0] = f32::NAN;
        assert!(invalid.svg().is_err());
        invalid = drawing();
        invalid.layers[0].strokes[0].color = "#abc\"/>".into();
        assert!(invalid.svg().is_err());
    }
    #[test]
    fn native_pdf_ink_preserves_width_and_rejects_unrepresentable_strokes() {
        let mut doc = drawing();
        assert!(pdf_stroke(&doc.svg().unwrap()).is_err());
        let layer = &mut doc.layers[0];
        layer.mask.clear();
        layer.opacity = 100;
        let stroke = &mut layer.strokes[0];
        for point in &mut stroke.points {
            point[2] = 0.5;
        }
        let diameter = stroke.radius(0.5) * 2.0;
        let data = pdf_stroke(&doc.svg().unwrap()).unwrap();
        assert_eq!(data["strokeWidth"], json!(diameter));
        assert_eq!(data["strokes"][0].as_array().unwrap().len(), 2);
        doc.layers[0].strokes[0].points[0][2] = 0.3;
        assert!(pdf_stroke(&doc.svg().unwrap()).is_err());
    }
    #[test]
    fn native_pdf_group_keeps_each_compatible_path_and_rejects_mixed_brushes() {
        let mut doc = drawing();
        doc.layers[0].mask.clear();
        for p in &mut doc.layers[0].strokes[0].points { p[2] = 1.; }
        let mut second: Stroke = serde_json::from_value(json!({"width":20,"color":"#0033FF","points":[[20,15,1],[70,15,1]]})).unwrap();
        doc.layers[0].strokes.push(second);
        let svg=doc.svg().unwrap();
        let group=pdf_stroke(&svg).unwrap();
        assert_eq!(group["strokes"],json!([[[10.,40.],[90.,40.]],[[20.,15.],[70.,15.]]]));
        // Compatible opaque layers have the same union while retaining their
        // order and original editable layer state inside the canonical SVG.
        second=doc.layers[0].strokes.pop().unwrap();
        doc.layers.push(Layer{name:"second".into(),visible:true,opacity:100,strokes:vec![second],mask:vec![]});
        assert_eq!(pdf_stroke(&doc.svg().unwrap()).unwrap(),group);
        for changed in ["color","width","pressure","opacity","mask"] {
            let mut bad:Drawing=Drawing::open(&doc.svg().unwrap()).unwrap();
            match changed {
                "color"=>bad.layers[1].strokes[0].color="#ff0000".into(),
                "width"=>bad.layers[1].strokes[0].width=21.,
                "pressure"=>bad.layers[1].strokes[0].points[0][2]=0.5,
                "opacity"=>bad.layers[1].opacity=99,
                _=>bad.layers[1].mask.push(serde_json::from_value(json!({"width":3,"color":"#ffffff","points":[[20,15,1]]})).unwrap()),
            }
            assert_eq!(pdf_stroke(&bad.svg().unwrap()).err(),Some("pdf_unsupported_object"));
        }
    }

    #[test]
    fn page_preview_and_open_make_no_staged_asset_and_save_deduplicates() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let assets = root.path().join("assets");
        let app = Arc::new(
            App::bootstrap(crate::paths::AppPaths::with_dirs(
                &profile,
                root.path(),
                &assets,
            ))
            .unwrap(),
        );
        let call = |body| crate::server::call_in_process(&app, "POST", "/api/drawing", Some(&body));
        let doc = drawing();
        let (status, p) = call(json!({"action":"render","document":doc}));
        assert_eq!(status, 200);
        let svg = p["svg"].as_str().unwrap();
        assert_eq!(
            call(json!({"action":"open","svg":svg})).1["document"],
            serde_json::to_value(&doc).unwrap()
        );
        let count = || {
            profile
                .join("resource-staging")
                .read_dir()
                .map(|r| r.count())
                .unwrap_or(0)
        };
        assert_eq!(count(), 0);
        let (status, s) = call(json!({"action":"stage","document":doc}));
        assert_eq!(status, 200);
        assert_eq!(count(), 1);
        assert_eq!(
            std::fs::read_to_string(s["asset"]["path"].as_str().unwrap()).unwrap(),
            svg
        );
        assert_eq!(
            call(json!({"action":"stage","document":doc})).1["rel"],
            s["rel"]
        );
        assert_eq!(count(), 1);
        assert!(!root.path().join("readmd-assets").exists());
    }

    #[test]
    fn locked_resvg_renders_pressure_mask_and_hidden_layer() {
        let mut doc = drawing();
        let svg = doc.svg().unwrap();
        let png = crate::svg_render::rasterize(svg.as_bytes()).unwrap();
        let image = image::load_from_memory(&png).unwrap().into_rgba8();
        let (w, h) = image.dimensions();
        let pixel = |x: u32, y: u32| image.get_pixel(x * w / 120, y * h / 80);
        assert_eq!(pixel(20, 40)[2], 255);
        assert_eq!(pixel(88, 40)[3], 0);
        assert_eq!(pixel(78, 45)[3], 255);
        assert_eq!(pixel(12, 46)[3], 0);
        doc.layers[0].mask.clear();
        doc.layers[0].visible = false;
        let hidden = crate::svg_render::rasterize(doc.svg().unwrap().as_bytes()).unwrap();
        assert!(image::load_from_memory(&hidden)
            .unwrap()
            .into_rgba8()
            .pixels()
            .all(|p| p[3] == 0));
    }
}
