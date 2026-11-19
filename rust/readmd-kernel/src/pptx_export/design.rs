//! Editable presentation ornaments shared by the preview and DrawingML writer.
use super::{Layout, Rect, Shape, H, LEFT, W, WIDTH};

#[derive(Clone, Copy, Debug)]
pub(super) enum Geometry {
    Rectangle,
    Rounded,
    Ellipse,
}

impl Geometry {
    pub(super) fn css(self) -> &'static str {
        match self { Self::Rectangle => "0", Self::Rounded => "12px", Self::Ellipse => "50%" }
    }

    pub(super) fn xml(self) -> &'static str {
        match self { Self::Rectangle => "rect", Self::Rounded => "roundRect", Self::Ellipse => "ellipse" }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Paint {
    Solid(&'static str),
    Gradient(&'static str, &'static str),
    Outline(&'static str),
}

impl Paint {
    pub(super) fn css(self) -> String {
        match self {
            Self::Solid(color) => format!("background:#{color};"),
            Self::Gradient(start, end) => format!("background:linear-gradient(90deg,#{start},#{end});"),
            Self::Outline(color) => format!("border:1px solid #{color};box-sizing:border-box;"),
        }
    }

    pub(super) fn xml(self) -> String {
        match self {
            Self::Solid(color) => format!(r#"<a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:ln><a:noFill/></a:ln>"#),
            Self::Gradient(start, end) => format!(r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="{start}"/></a:gs><a:gs pos="100000"><a:srgbClr val="{end}"/></a:gs></a:gsLst><a:lin ang="0" scaled="0"/></a:gradFill><a:ln><a:noFill/></a:ln>"#),
            Self::Outline(color) => format!(r#"<a:noFill/><a:ln w="12700"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:ln>"#),
        }
    }
}

pub(super) fn ornaments(layout: Layout, cover: bool) -> Vec<Shape> {
    use Geometry::{Ellipse, Rectangle, Rounded};
    use Paint::{Gradient, Outline, Solid};
    let mark = |rect: Rect, geometry, paint| Shape::Accent { rect, geometry, paint };
    let rule = |rect, color| mark(rect, Rectangle, Solid(color));
    match (layout, cover) {
        (Layout::Plain, _) => Vec::new(),
        (Layout::Defense, true) => vec![
            mark([0., 0., W, H], Rectangle, Gradient("F9FBFF", "EAF0FB")),
            rule([0., 0., 10., H], "365BB0"),
            rule([84., 78., 88., 4.], "365BB0"),
            rule([84., 456., 792., 1.], "B9C9E5"),
            rule([824., 90., 1., 366.], "C5D3EA"),
            mark([842., 106., 34., 34.], Ellipse, Outline("9AB1D9")),
            mark([856., 120., 6., 6.], Ellipse, Solid("365BB0")),
        ],
        (Layout::Defense, false) => vec![
            rule([LEFT, 26., 78., 4.], "365BB0"),
            rule([LEFT, 512., WIDTH, 1.], "CBD8EC"),
            mark([W - 64., 24., 10., 10.], Ellipse, Outline("9AB1D9")),
        ],
        (Layout::Lab, true) => vec![
            mark([104., 54., 752., 432.], Rounded, Gradient("F0F8F4", "DCEFE7")),
            mark([104., 54., 752., 432.], Rounded, Outline("A5CEC2")),
            rule([432., 76., 96., 4.], "107E71"),
            rule([143., 440., 674., 1.], "A5CEC2"),
            mark([771., 88., 12., 12.], Ellipse, Solid("107E71")),
            mark([789., 88., 12., 12.], Ellipse, Outline("107E71")),
            mark([807., 88., 12., 12.], Ellipse, Outline("107E71")),
        ],
        (Layout::Lab, false) => vec![
            rule([35., 29., 4., 70.], "107E71"),
            rule([LEFT, 512., WIDTH, 2.], "B7DED1"),
            mark([W - 64., 24., 10., 10.], Ellipse, Outline("107E71")),
        ],
        (Layout::Launch, true) => vec![
            mark([0., 0., W, H], Rectangle, Gradient("11182E", "293B72")),
            mark([594., 33., 336., 336.], Ellipse, Outline("6576AE")),
            mark([625., 64., 274., 274.], Ellipse, Outline("4F629C")),
            mark([656., 95., 212., 212.], Ellipse, Outline("596EAC")),
            mark([855., 163., 10., 10.], Ellipse, Solid("B8CEFF")),
            rule([84., 76., 104., 4.], "A7BAFF"),
            rule([84., 476., 792., 1.], "596EAC"),
            rule([84., 476., 112., 2.], "A7BAFF"),
        ],
        (Layout::Launch, false) => vec![
            mark([0., 0., W, H], Rectangle, Gradient("131B31", "212D50")),
            rule([0., 0., W, 4.], "A7BAFF"),
            rule([LEFT, 513., WIDTH, 1.], "596EAC"),
        ],
    }
}

// Slide-entry fade is a native shape effect. Time IDs are local to the slide;
// the target is the final DrawingML shape ID, after decorations are inserted.
pub(super) fn title_fade(shape: usize) -> String {
    format!(r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst>
<p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>
<p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/><p:cond evt="onBegin" delay="0"><p:tn val="2"/></p:cond></p:stCondLst><p:childTnLst>
<p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>
<p:par><p:cTn id="5" presetID="10" presetClass="entr" presetSubtype="0" fill="hold" nodeType="withEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>
<p:set><p:cBhvr><p:cTn id="6" dur="1" fill="hold"/><p:tgtEl><p:spTgt spid="{shape}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set>
<p:animEffect transition="in" filter="fade"><p:cBhvr><p:cTn id="7" dur="420"/><p:tgtEl><p:spTgt spid="{shape}"/></p:tgtEl></p:cBhvr></p:animEffect>
</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>
</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq>
</p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#)
}
