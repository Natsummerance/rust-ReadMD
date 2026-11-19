//! Placement of the shared drawing model's native, constant-width subset.
//! No SVG path parser, brush renderer, rasterization or external assets.
use serde::Deserialize;
use serde_json::Value;

pub const MARKER: &str = "ReadMD uniform drawing stroke v1";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stroke {
    width: u16,
    height: u16,
    color: String,
    #[serde(rename = "strokeWidth")]
    stroke_width: f32,
    // `points` is the original one-stroke journal. Accept it unchanged when
    // replaying old recovery records; new operations carry separate strokes.
    points: Option<Vec<[f32; 2]>>,
    strokes: Option<Vec<Vec<[f32; 2]>>>,
}
pub struct Placement {
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub color: [u8; 3],
    pub width: f32,
    // left, bottom, right, top in unrotated PDF page coordinates.
    pub bounds: [f32; 4],
}
pub fn placement(operation: &Value) -> Result<Placement, &'static str> {
    let invalid = "pdf_invalid_operation";
    let stroke: Stroke =
        serde_json::from_value(operation["stroke"].clone()).map_err(|_| invalid)?;
    let matrix: [f32; 6] =
        serde_json::from_value(operation["transform"].clone()).map_err(|_| invalid)?;
    let paths = match (stroke.points, stroke.strokes) {
        (Some(points), None) => vec![points],
        (None, Some(paths)) => paths,
        _ => return Err(invalid),
    };
    if !(1..=8192).contains(&stroke.width)
        || !(1..=8192).contains(&stroke.height)
        || !(0.037..=128.).contains(&stroke.stroke_width)
        || !(1..=128).contains(&paths.len())
        || paths.iter().any(|p| p.len() < 2)
        || paths.iter().map(Vec::len).sum::<usize>() > 4096
        || stroke.color.len() != 7
        || !stroke.color.starts_with('#')
        || !stroke.color[1..].bytes().all(|c| c.is_ascii_hexdigit())
        || matrix.iter().any(|v| !v.is_finite() || v.abs() > 100_000.)
    {
        return Err(invalid);
    }
    let [ox, oy, xx, xy, yx, yy] = matrix;
    let sx = xx.hypot(xy);
    let sy = yx.hypot(yy);
    // Only uniform placement, including quarter-turn rotations, preserves the
    // native ink width. Shear/nonuniform scaling must not fake brush fidelity.
    if !(0.0001..=1000.).contains(&sx)
        || (sx - sy).abs() > sx * 0.001
        || (xx * yx + xy * yy).abs() > sx * sy * 0.001
    {
        return Err(invalid);
    }
    let radius = stroke.stroke_width * 0.5;
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut strokes = Vec::with_capacity(paths.len());
    for path in paths {
        let mut points = Vec::with_capacity(path.len());
        for [x, y] in path {
            // A native InkList has no SVG viewport clipping. Refuse edge-clipped
            // source art instead of extending it beyond its original canvas.
            if !(radius..=f32::from(stroke.width) - radius).contains(&x)
                || !(radius..=f32::from(stroke.height) - radius).contains(&y)
            {
                return Err("pdf_unsupported_object");
            }
            let p = [ox + xx * x + yx * y, oy + xy * x + yy * y];
            if p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
                return Err(invalid);
            }
            bounds[0] = bounds[0].min(p[0]);
            bounds[1] = bounds[1].min(p[1]);
            bounds[2] = bounds[2].max(p[0]);
            bounds[3] = bounds[3].max(p[1]);
            points.push(p);
        }
        if points.iter().all(|p| *p == points[0]) {
            return Err("pdf_unsupported_object");
        }
        strokes.push(points);
    }
    let width = stroke.stroke_width * sx;
    if !width.is_finite() || !(0.001..=1000.).contains(&width) {
        return Err(invalid);
    }
    bounds[0] -= width * 0.5;
    bounds[1] -= width * 0.5;
    bounds[2] += width * 0.5;
    bounds[3] += width * 0.5;
    let color = [
        u8::from_str_radix(&stroke.color[1..3], 16).map_err(|_| invalid)?,
        u8::from_str_radix(&stroke.color[3..5], 16).map_err(|_| invalid)?,
        u8::from_str_radix(&stroke.color[5..7], 16).map_err(|_| invalid)?,
    ];
    Ok(Placement {
        strokes,
        color,
        width,
        bounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn op() -> Value {
        json!({"stroke":{"width":100,"height":80,"color":"#1267fe","strokeWidth":8.,"points":[[10.,20.],[70.,40.]]},"transform":[50.,200.,2.,0.,0.,-2.]})
    }
    #[test]
    fn constant_width_placement_preserves_coordinates_through_all_page_rotations() {
        let transforms = [
            [50., 200., 2., 0., 0., -2.],
            [50., 200., 0., 2., 2., 0.],
            [50., 200., -2., 0., 0., 2.],
            [50., 200., 0., -2., -2., 0.],
        ];
        for matrix in transforms {
            let mut op = op();
            op["transform"] = json!(matrix);
            let placed = placement(&op).unwrap();
            assert_eq!(placed.width, 16.);
            assert_eq!(placed.color, [0x12, 0x67, 0xfe]);
            assert_eq!(
                placed.strokes[0][0],
                [
                    matrix[0] + 10. * matrix[2] + 20. * matrix[4],
                    matrix[1] + 10. * matrix[3] + 20. * matrix[5]
                ]
            );
            assert!(
                ((placed.strokes[0][1][0] - placed.strokes[0][0][0])
                    .hypot(placed.strokes[0][1][1] - placed.strokes[0][0][1])
                    - 2. * 60_f32.hypot(20.))
                .abs()
                    < 0.001
            );
        }
    }
    #[test]
    fn separate_paths_share_a_transform_and_legacy_journals_replay_unchanged() {
        let old = op();
        let legacy = placement(&old).unwrap();
        let mut group = old.clone();
        let points = group["stroke"]
            .as_object_mut()
            .unwrap()
            .remove("points")
            .unwrap();
        group["stroke"]["strokes"] = json!([points]);
        let single = placement(&group).unwrap();
        assert_eq!(legacy.strokes, single.strokes);
        assert_eq!(legacy.bounds, single.bounds);
        group["stroke"]["strokes"]
            .as_array_mut()
            .unwrap()
            .push(json!([[20., 60.], [60., 60.]]));
        let two = placement(&group).unwrap();
        assert_eq!(two.strokes.len(), 2);
        assert_eq!(two.strokes[0], legacy.strokes[0]);
        assert_eq!(two.strokes[1], vec![[90., 80.], [170., 80.]]);
        group["stroke"]["points"] = old["stroke"]["points"].clone();
        assert!(placement(&group).is_err());
        group["stroke"].as_object_mut().unwrap().remove("points");
        group["stroke"]["strokes"] = json!([[[10., 20.], [10., 20.]]]);
        assert_eq!(placement(&group).err(), Some("pdf_unsupported_object"));
        group["stroke"]["strokes"] = json!([]);
        assert!(placement(&group).is_err());
    }
    #[test]
    fn malformed_scaling_unbounded_points_and_clipped_source_art_are_rejected() {
        for transform in [
            json!([0., 0., 1., 0., 0., 2.]),
            json!([0., 0., 1., 0., 1., 1.]),
            json!([0., 0., 0., 0., 0., 0.]),
            json!([0., 0., 1.]),
        ] {
            let mut op = op();
            op["transform"] = transform;
            assert!(placement(&op).is_err());
        }
        let mut op = op();
        op["stroke"]["points"] = json!([[0., 20.], [70., 40.]]);
        assert_eq!(placement(&op).err(), Some("pdf_unsupported_object"));
        op["stroke"]["points"] = json!([[10., 20.], [10., 20.]]);
        assert!(placement(&op).is_err());
        op["stroke"]["points"] = json!([[10., 20.], [70., 40.]]);
        op["stroke"]["color"] = json!("#ffzz00");
        assert!(placement(&op).is_err());
    }
}
