//! Independent font-table values and actual painted geometry, not snapshots
//! of the layout implementation. The oracle is generated with FontTools.
use super::*;
use serde_json::Value;

fn oracle() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/math-font-geometry-stix.json")).unwrap()
}
fn constant(data: &Value, name: &str) -> f64 {
    data["constants"][name].as_f64().unwrap() / data["unitsPerEm"].as_f64().unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "actual {actual}, expected {expected}");
}
fn run(text: &str) -> String { format!("<m:r><m:t>{text}</m:t></m:r>") }
fn root(body: &str) -> Node { omml::parse(&format!("<m:oMath>{body}</m:oMath>")).unwrap() }
fn radical(degree: &str, properties: &str) -> Node {
    root(&format!("<m:rad><m:radPr>{properties}</m:radPr><m:deg>{degree}</m:deg><m:e>{}</m:e></m:rad>", run("x")))
}
fn part<'a>(b: &'a MBox, name: &str) -> &'a Region {
    b.regions.iter().find(|r| r.part && r.kind == name).unwrap()
}
fn record<'a>(data: &'a Value, gid: u16) -> &'a Value {
    data["glyphs"].as_object().unwrap().values().find(|g| g["gid"] == gid).unwrap()
}
fn oracle_vertical_ink(data: &Value, items: &[Item]) -> (f64, f64) {
    let upem = data["unitsPerEm"].as_f64().unwrap();
    let bounds = items.iter().filter_map(|i| if let Item::Glyph { gid, y, scale, .. } = *i {
        let r = record(data, gid);
        Some((y + r["fontBounds"][1].as_f64().unwrap() * scale / upem, y + r["fontBounds"][3].as_f64().unwrap() * scale / upem))
    } else { None }).collect::<Vec<_>>();
    assert!(!bounds.is_empty());
    (bounds.iter().map(|b|b.0).fold(f64::INFINITY, f64::min), bounds.iter().map(|b|b.1).fold(f64::NEG_INFINITY, f64::max))
}
fn glyphs(b: &Formula) -> Vec<(u16, f64, f64, f64)> {
    b.items.iter().filter_map(|i| match *i {
        Item::Glyph { gid, x, y, scale, .. } => Some((gid, x, y, scale)), _ => None,
    }).collect()
}

#[test]
fn math_font_geometry_constants_match_independent_fonttools() {
    use sha2::{Digest, Sha256};
    let data = oracle(); let f = fonts(false).unwrap(); let c = load_constants(&f.fonts[0]);
    assert_eq!(format!("{:x}", Sha256::digest(&*f.fonts[0].data)), data["fontSha256"].as_str().unwrap());
    assert_eq!(f.fonts[0].upem, data["unitsPerEm"].as_f64().unwrap());
    for (value, name) in [
        (c.rad_before, "RadicalKernBeforeDegree"), (c.rad_after, "RadicalKernAfterDegree"),
        (c.overbar_gap, "OverbarVerticalGap"), (c.overbar_rule, "OverbarRuleThickness"),
        (c.overbar_extra, "OverbarExtraAscender"), (c.underbar_gap, "UnderbarVerticalGap"),
        (c.underbar_rule, "UnderbarRuleThickness"), (c.underbar_extra, "UnderbarExtraDescender"),
        (c.accent_base, "AccentBaseHeight"),
    ] { near(value, constant(&data, name)); }
    near(c.rad_degree_raise, data["constants"]["RadicalDegreeBottomRaisePercent"].as_f64().unwrap() / 100.);
}

#[test]
fn math_radical_degree_uses_font_kerns_and_bottom_raise() {
    let data = oracle(); let f = fonts(false).unwrap();
    for degree in ["3", "123123", "j", "+", "̇"] { for display in [false, true] { for level in [0, 1, 2] {
        let source = radical(&run(degree), ""); let n = source.child("oMath").unwrap().child("rad").unwrap();
        let st = Style { display, level, cramped: false }; let mut ctx = Ctx::new(&f, 12.);
        let b = ctx.radical(n, st); let sign = part(&b, "radical-sign"); let index = part(&b, "radical-degree");
        let s = st.scale(&f.c);
        let before = (constant(&data, "RadicalKernBeforeDegree") * s).max(0.);
        let after = (constant(&data, "RadicalKernAfterDegree") * s).max(-index.w);
        near(index.x, before); near(sign.x, before + index.w + after);
        let (sign_bottom, sign_top) = oracle_vertical_ink(&data, &b.items[sign.from..sign.to]);
        let (index_bottom, _) = oracle_vertical_ink(&data, &b.items[index.from..index.to]);
        near(index_bottom, sign_bottom + (sign_top - sign_bottom) * data["constants"]["RadicalDegreeBottomRaisePercent"].as_f64().unwrap() / 100.);
        assert!(sign.x >= 0. && index.x >= 0. && b.w >= sign.x + sign.w);
    }}}
}

#[test]
fn math_radical_zero_raise_and_signed_kerns_are_not_replaced_by_defaults() {
    let mut f = (*fonts(false).unwrap()).clone();
    f.c.rad_before = -0.4; f.c.rad_after = -20.; f.c.rad_degree_raise = 0.;
    let source = radical(&run("j"), ""); let n = source.child("oMath").unwrap().child("rad").unwrap();
    let b = Ctx::new(&f, 12.).radical(n, Style { display: true, level: 0, cramped: false });
    let sign = part(&b, "radical-sign"); let index = part(&b, "radical-degree");
    near(index.x, 0.); near(sign.x, 0.); near(index.y - index.d, -sign.d);
}

#[test]
fn math_radical_hidden_and_property_only_degrees_leave_no_spacing() {
    let f = fonts(false).unwrap();
    for display in [false, true] {
        let st = Style { display, level: 0, cramped: false };
        let draw = |degree: &str, props: &str| {
            let source = radical(degree, props); let n = source.child("oMath").unwrap().child("rad").unwrap();
            Ctx::new(&f, 12.).radical(n, st)
        };
        let empty = draw("", "");
        for b in [draw(&run("12345"), "<m:degHide/>"), draw("<m:argPr><m:argSz m:val=\"-1\"/></m:argPr>", ""), draw(&run(""), "")] {
            assert_eq!(empty.items, b.items); near(empty.w, b.w); near(empty.h, b.h); near(empty.d, b.d);
            assert!(!b.regions.iter().any(|r| r.kind == "radical-degree"));
        }
    }
}

#[test]
fn math_bar_constants_control_ink_and_reserved_space_independently() {
    let mut f = (*fonts(false).unwrap()).clone();
    // Deliberately distinguish the two bars and the unrelated fraction rule.
    f.c.rule = 0.777; f.c.overbar_gap = 0.18; f.c.overbar_rule = 0.031; f.c.overbar_extra = 0.06;
    f.c.underbar_gap = 0.29; f.c.underbar_rule = 0.087; f.c.underbar_extra = 0.11;
    let data = oracle(); let j = &data["glyphs"][data["characters"]["𝑗"].as_str().unwrap()]; let upem = f.fonts[0].upem;
    for level in [0, 1, 2] { for bottom in [false, true] {
        let source = root(&format!("<m:bar><m:barPr><m:pos m:val=\"{}\"/></m:barPr><m:e>{}</m:e></m:bar>", if bottom { "bot" } else { "top" }, run("j")));
        let st = Style { display: false, level, cramped: false }; let s = st.scale(&f.c);
        let b = Ctx::new(&f, 12.).bar(source.child("oMath").unwrap().child("bar").unwrap(), st);
        let (y, t) = b.items.iter().find_map(|i| if let Item::Rule { y, h, .. } = *i { Some((y, h)) } else { None }).unwrap();
        if bottom {
            near(t, 0.087 * s); near(y, j["fontBounds"][1].as_f64().unwrap() / upem * s - 0.29 * s - t);
            near(b.d, -y + 0.11 * s);
        } else {
            near(t, 0.031 * s); near(y, j["fontBounds"][3].as_f64().unwrap() / upem * s + 0.18 * s);
            near(b.h, y + t + 0.06 * s);
        }
    }}
}

#[test]
fn math_accents_align_real_font_attachment_and_preserve_positive_ink_bottom() {
    let data = oracle(); let upem = data["unitsPerEm"].as_f64().unwrap();
    for text in ["f", "x", "A"] { for accent in ["̂", "̇", "⃗"] { for display in [false, true] { for size in [12., 24.] {
        let xml = format!("<m:oMath><m:acc><m:accPr><m:chr m:val=\"{accent}\"/></m:accPr><m:e>{}</m:e></m:acc></m:oMath>", run(text));
        let b = layout_source("", Some(&xml), display, size).unwrap(); let g = glyphs(&b); assert_eq!(g.len(), 2);
        let base = record(&data, g[0].0); let mark = record(&data, g[1].0);
        let attach = |record: &Value| record["topAccentAttachment"].as_f64().unwrap_or_else(|| record["advance"].as_f64().unwrap() / 2.);
        near(g[0].1 + attach(base) * g[0].3 / upem, g[1].1 + attach(mark) * g[1].3 / upem);
        let base_top = g[0].2 + base["fontBounds"][3].as_f64().unwrap() * g[0].3 / upem;
        let mark_bottom = mark["fontBounds"][1].as_f64().unwrap() * g[1].3 / upem;
        let expected_y = (base_top - constant(&data, "AccentBaseHeight")).max(0.).max(base_top - mark_bottom);
        near(g[1].2, expected_y);
        let ink = b.ink_bounds().unwrap(); assert!(ink.0 >= -1e-9 && ink.2 <= b.width / size + 1e-9, "{text}/{accent}/{display}/{size}: ink {ink:?}, advance {}", b.width / size);
        assert!(mark_bottom + g[1].2 >= base_top);
    }}}}
}

#[test]
fn math_wide_accents_reserve_ink_and_keep_source_regions() {
    let id = "rd-formula-token-omml-geometry-0";
    let xml = format!("<m:oMath><m:acc readmdId=\"{id}\"><m:accPr><m:chr m:val=\"̂\"/></m:accPr><m:e readmdId=\"{id}-0\">{}</m:e></m:acc></m:oMath>", run(&"𝑥".repeat(16)));
    let b = layout_source("", Some(&xml), true, 12.).unwrap(); let ink = b.ink_bounds().unwrap();
    // This font deliberately has finite hat variants with no assembly.
    // The longest outline must cover the base without also growing taller.
    let data = oracle(); assert!(data["variants"]["horizontal"]["̂"]["assembly"].is_null());
    let last = data["variants"]["horizontal"]["̂"]["variants"].as_array().unwrap().last().unwrap();
    let variant = &data["glyphs"][last["glyph"].as_str().unwrap()];
    let upem = data["unitsPerEm"].as_f64().unwrap();
    let (gid, scale, stretch) = b.items.iter().find_map(|i| if let Item::Glyph { gid, scale, x_scale, .. } = *i {
        (gid as u64 == variant["gid"].as_u64().unwrap()).then_some((gid,scale,x_scale))
    } else { None }).unwrap();
    assert_eq!(gid as u64, variant["gid"].as_u64().unwrap()); near(scale, 1.);
    let variant_width = (variant["fontBounds"][2].as_f64().unwrap() - variant["fontBounds"][0].as_f64().unwrap()) / upem;
    let x = &data["glyphs"][data["characters"]["𝑥"].as_str().unwrap()];
    near(variant_width * stretch, 16. * x["advance"].as_f64().unwrap() / upem);
    assert!(stretch > 1.); assert!(b.interactive_svg().contains("scale("));
    assert!(ink.0 >= -1e-9 && ink.2 <= b.width / b.size + 1e-9);
    assert!(b.regions.iter().any(|r| r.id == id)); assert!(b.regions.iter().any(|r| r.id == format!("{id}-0")));
    assert!(b.interactive_svg().contains("data-math-parts=\"accent-glyph\""));
    assert!(b.draw(0., 0., "0 0 0").contains("m\n"));
}

#[test]
fn math_nested_bars_do_not_count_inner_reserved_space_as_ink() {
    let f = fonts(false).unwrap();
    for bottom in [false, true] {
        let bar = |body: &str| format!("<m:bar><m:barPr><m:pos m:val=\"{}\"/></m:barPr><m:e>{body}</m:e></m:bar>", if bottom { "bot" } else { "top" });
        let xml = format!("<m:oMath>{}</m:oMath>", bar(&bar(&run("x"))));
        let b = layout_source("", Some(&xml), true, 12.).unwrap();
        let rules = b.items.iter().filter_map(|i| if let Item::Rule { y, h, .. } = *i { Some((y,h)) } else { None }).collect::<Vec<_>>();
        assert_eq!(rules.len(), 2);
        if bottom {
            near(rules[0].0 - rules[1].0 - rules[1].1, f.c.underbar_gap);
            near(b.depth / b.size, -rules[1].0 + f.c.underbar_extra);
        } else {
            near(rules[1].0 - rules[0].0 - rules[0].1, f.c.overbar_gap);
            near(b.height / b.size, rules[1].0 + rules[1].1 + f.c.overbar_extra);
        }
    }
}

#[test]
fn math_root_bar_and_accent_preserve_explicit_physical_office_size() {
    let defaults = "<w:styles><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"36\"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>";
    let bodies = [
        format!("<m:rad><m:deg>{}</m:deg><m:e>{}</m:e></m:rad>", run("j"), run("x")),
        format!("<m:bar><m:e>{}</m:e></m:bar>", run("j")),
        format!("<m:bar><m:barPr><m:pos m:val=\"bot\"/></m:barPr><m:e>{}</m:e></m:bar>", run("j")),
        format!("<m:acc><m:e>{}</m:e></m:acc>", run("f")),
    ];
    for body in bodies { for display in [false,true] {
        let xml = format!("{defaults}<m:oMath>{body}</m:oMath>");
        let a = layout_source("", Some(&xml), display, 12.).unwrap(); let b = layout_source("", Some(&xml), display, 24.).unwrap();
        near(a.width, b.width); near(a.height, b.height); near(a.depth, b.depth);
        let ga = glyphs(&a); let gb = glyphs(&b); assert_eq!(ga.len(), gb.len());
        for (left,right) in ga.iter().zip(&gb) {
            assert_eq!(left.0, right.0); near(left.1*a.size,right.1*b.size); near(left.2*a.size,right.2*b.size); near(left.3*a.size,right.3*b.size);
        }
    }}
}

#[test]
fn math_context_and_output_agree_on_slanted_and_rule_ink() {
    let f = fonts(false).unwrap(); let mut ctx = Ctx::new(&f, 12.);
    let mut b = ctx.glyph_box('𝑓', 1.3).shifted(0.2, -0.1);
    if let Item::Glyph { shear, x_scale, .. } = &mut b.items[0] { *shear = 0.2; *x_scale = 1.7; }
    b.items.push(Item::Rule { x:-0.1, y:1.2, w:0.6, h:0.05, color:Paint::Auto });
    b.items.push(Item::Line { x1:0.1, y1:-0.4, x2:0.7, y2:0.2, thickness:0.08, color:Paint::Auto });
    let mut output = layout_source("x", None, false, 12.).unwrap(); output.items = b.items.clone();
    assert_eq!(ctx.box_ink_bounds(&b), output.ink_bounds());
}

#[test]
fn math_structure_placement_respects_phantom_visibility_and_zero_dimensions() {
    let font = fonts(false).unwrap(); let gid = font.fonts[0].face().unwrap().glyph_index('𝑏').unwrap().0;
    let hidden_body = |show: bool, zeros: &str, nested: bool| {
        let tall = "<m:r><w:rPr><w:sz w:val=\"96\"/></w:rPr><m:t>b</m:t></m:r>";
        let inner = if nested { format!("<m:phant><m:phantPr><m:show/></m:phantPr><m:e>{tall}</m:e></m:phant>") } else { tall.into() };
        format!("{}<m:phant><m:phantPr><m:show m:val=\"{show}\"/>{zeros}</m:phantPr><m:e>{inner}</m:e></m:phant>", run("x"))
    };
    for kind in ["rad", "bar", "acc"] { for zeros in ["", "<m:zeroAsc/>", "<m:zeroDesc/>", "<m:zeroAsc/><m:zeroDesc/>", "<m:zeroWid/>"] { for nested in [false,true] {
        let draw = |show| {
            let xml = format!("<m:oMath><m:{kind}><m:e>{}</m:e></m:{kind}></m:oMath>", hidden_body(show, zeros, nested));
            layout_source("", Some(&xml), true, 12.).unwrap()
        };
        let visible = draw(true); let hidden = draw(false);
        assert!((visible.width-hidden.width).abs()<1e-9 && (visible.height-hidden.height).abs()<1e-9 && (visible.depth-hidden.depth).abs()<1e-9,
            "{kind}, zero={zeros}, nested={nested}: visible {:?}, hidden {:?}", (visible.width,visible.height,visible.depth), (hidden.width,hidden.height,hidden.depth));
        let remaining = visible.items.iter().filter(|i| !matches!(i, Item::Glyph { gid:g, .. } if *g == gid)).cloned().collect::<Vec<_>>();
        assert_eq!(remaining, hidden.items, "{kind}, zero={zeros}, nested={nested}");
    }}}
}

#[test]
fn math_relayout_uses_frozen_source_and_fonts_for_relative_and_office_sizes() {
    let defaults = "<w:styles><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"36\"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>";
    let bodies = [
        format!("<m:f><m:fPr><m:type m:val=\"skw\"/></m:fPr><m:num>{}</m:num><m:den>{}</m:den></m:f>",run("f"),run("j")),
        format!("<m:rad><m:deg>{}</m:deg><m:e>{}</m:e></m:rad>",run("3"),run("x")),
        format!("<m:bar><m:e>{}</m:e></m:bar>",run("x")),
        format!("<m:acc><m:e>{}</m:e></m:acc>",run(&"𝑥".repeat(16))),
    ];
    for fixed in [false,true] { for display in [false,true] { for body in &bodies {
        let xml = format!("{}<m:oMath>{body}</m:oMath>",if fixed {defaults}else{""});
        let original = layout_source("",Some(&xml),display,12.).unwrap();
        let changed = original.at_size(24.).unwrap(); let direct = layout_source("",Some(&xml),display,24.).unwrap();
        assert_eq!(changed,direct); assert!(Arc::ptr_eq(&original.fonts,&changed.fonts));
        if fixed { near(original.width,changed.width); near(original.height,changed.height); near(original.depth,changed.depth); }
        else { near(original.width*2.,changed.width); near(original.height*2.,changed.height); near(original.depth*2.,changed.depth); }
        for invalid in [f64::NAN,f64::INFINITY,0.,-1.] { assert!(original.at_size(invalid).is_none()); }
    }}}
    let original = layout_source_constrained("x+x+x+x+x+x",None,true,12.,Some(55.)).unwrap();
    let changed = original.at_size(24.).unwrap();
    assert_eq!(changed,layout_source_constrained("x+x+x+x+x+x",None,true,24.,Some(55.)).unwrap());
    near(changed.available_width.unwrap(),55.);
}
