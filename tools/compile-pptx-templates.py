#!/usr/bin/env python3
"""Compile user-owned native PPTX designs into ReadMD's embedded art seeds.

Requires Pillow and defusedxml (also supplied by the PPTX skill environment).
Usage: python compile-pptx-templates.py --source <native style library> [--render]

The source library is read only. No source names, notes, sample data, document
properties, external links, fonts, 3D model files or source photographs enter
the resulting packages. Native artwork, geometry, art animations and slide
transitions survive; media is resampled to a presentation-sized pixel budget.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import posixpath
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path
from xml.dom import Node

from defusedxml import minidom
from PIL import Image
from pptx_template_art import replace_media

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust" / "readmd-kernel" / "templates" / "pptx"
EMU = 12700
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
STYLES = {
    "confluence": {
        "source": "01_合流", "name": "合流", "foreground": "171A16", "accent": "CC3B28",
        "font": "Source Han Serif CN", "bodyFont": "Source Han Sans SC",
        "titleIds": {1:216, 2:5, 3:4, 4:12, 5:4, 6:4, 7:4, 8:4, 9:4, 10:161},
        "coverTitle": [44,105,432,163], "coverBody": [52,295,438,153],
        "coverSize": 62, "coverColor": "FFFBED",
        "imageBody": [48,223,398,248], "imageRect": [462,86,442,393],
        "closingBody": [48,260,480,218],
    },
    "lightfield": {
        "source": "02_万象光场", "name": "万象光场", "foreground": "F3F8FF", "accent": "42FDE8",
        "font": "Source Han Sans SC Heavy", "bodyFont": "Source Han Sans SC",
        "titleIds": {1:2742, 2:570, 3:595, 4:1069, 5:511, 6:1473, 7:1809, 8:829, 9:1049, 10:884},
        "coverTitle": [39,156,380,204], "coverBody": [42,389,470,94],
        "coverSize": 60, "coverColor": "F3F8FF",
        "imageBody": [43,214,410,259], "imageRect": [490,84,393,273],
        "closingBody": [44,270,464,206],
    },
    "pearl": {
        "source": "03_浮光", "name": "浮光", "foreground": "11181D", "accent": "8FA5C2",
        "font": "Source Han Serif CN Medium", "bodyFont": "Source Han Sans SC",
        "titleIds": {1:163, 2:6, 3:5, 4:14, 5:5, 6:5, 7:5, 8:5, 9:5, 10:6},
        "coverTitle": [44,91,458,140], "coverBody": [48,252,830,164],
        "coverSize": 68, "coverColor": "11181D",
        "imageBody": [448,176,433,293], "imageRect": [47,84,379,338],
        "closingBody": [49,263,577,210],
    },
    "chrome": {
        "source": "04_液相", "name": "液相", "foreground": "F8F5F0", "accent": "8789FE",
        "font": "Smiley Sans Oblique", "bodyFont": "Source Han Sans SC",
        "titleIds": {1:188, 2:81, 3:80, 4:86, 5:80, 6:80, 7:80, 8:80, 9:80, 10:32},
        "coverTitle": [48,118,434,182], "coverBody": [48,407,435,84],
        "coverSize": 50, "coverColor": "F8F5F0",
        "imageBody": [48,227,433,247], "imageRect": [453,210,376,207],
        "closingBody": [49,263,575,211],
    },
    "field": {
        "source": "05_在场", "name": "在场", "foreground": "151611", "accent": "DBF04A",
        "font": "Smiley Sans Oblique", "bodyFont": "Source Han Sans SC",
        "titleIds": {1:322, 2:9, 3:7, 4:17, 5:8, 6:7, 7:7, 8:7, 9:7, 10:87},
        "coverTitle": [41,330,500,155], "coverBody": [560,352,355,126],
        "coverSize": 50, "coverColor": "151611",
        "imageBody": [520,209,390,258], "imageRect": [46,74,447,340],
        "closingBody": [49,323,842,153],
    },
}


def elements(node):
    return [c for c in node.childNodes if c.nodeType == Node.ELEMENT_NODE]


def first(node, tag):
    return next(iter(node.getElementsByTagName(tag)), None)


def remove(node):
    if node.parentNode is not None:
        node.parentNode.removeChild(node)


def xml_bytes(doc):
    return doc.toxml(encoding="UTF-8", standalone=True)


def shape_id(shape):
    prop = first(shape, "p:cNvPr")
    return int(prop.getAttribute("id")) if prop else 0


def shape_name(shape):
    prop = first(shape, "p:cNvPr")
    return prop.getAttribute("name") if prop else ""


def rect(shape):
    xf = first(shape, "a:xfrm") or first(shape, "p:xfrm")
    if xf is None:
        return [42, 64, 874, 70]
    off, ext = first(xf, "a:off"), first(xf, "a:ext")
    if off is None or ext is None:
        return [42, 64, 874, 70]
    return [round(int(off.getAttribute(k))/EMU, 2) for k in ("x", "y")] + [round(int(ext.getAttribute(k))/EMU, 2) for k in ("cx", "cy")]


def title_style(shape, cfg):
    rp = first(shape, "a:rPr")
    color = first(rp, "a:srgbClr") if rp else None
    font = first(rp, "a:latin") if rp else None
    align = first(shape, "a:pPr")
    return {
        "shapeId": shape_id(shape), "rect": rect(shape),
        "fontSize": float(rp.getAttribute("sz") or 3800)/100 if rp else 38,
        "color": color.getAttribute("val") if color else cfg["foreground"],
        "fontFace": font.getAttribute("typeface") if font else cfg["font"],
        "align": "center" if align and align.getAttribute("algn") == "ctr" else "left",
    }


def background(doc):
    bg = first(doc, "p:bg")
    color = first(bg, "a:srgbClr") if bg else None
    return color.getAttribute("val") if color else "FFFFFF"


def safe_art(shape):
    name = shape_name(shape)
    # These vectors are an institution's source logo, not template artwork.
    if "学院标识" in name:
        return False
    if shape.getElementsByTagName("a:t") or shape.tagName in ("p:graphicFrame", "mc:AlternateContent"):
        return False
    if shape.tagName == "p:pic":
        return any(v in name for v in ("珍珠", "透光薄缘", "黑场", "dtid", "现场摄影"))
    if shape.tagName == "p:cxnSp":
        return "刻线" in name
    # Source-specific statistic plates are replaced by authored content.
    if any(v in name for v in ("绩点", "专业排名", "累计人次标签", "图注底片", "图例色标", "成绩底板", "去向人数牌")):
        return False
    return True


def clean_labels(doc):
    for node in list(doc.getElementsByTagName("p:cNvPr")):
        ident = node.getAttribute("id")
        for attr in ("name", "title", "descr"):
            if node.hasAttribute(attr):
                node.removeAttribute(attr)
        node.setAttribute("name", "art-" + ident)
        for child in list(elements(node)):
            if child.tagName.endswith("extLst") or "hlink" in child.tagName:
                remove(child)
    for tag in ("a:hlinkClick", "a:hlinkMouseOver", "p:custDataLst", "p:tags", "p:controls"):
        for node in list(doc.getElementsByTagName(tag)):
            remove(node)
    for node in doc.getElementsByTagName("p:cSld"):
        if node.hasAttribute("name"):
            node.removeAttribute("name")


def clean_timing(doc):
    ids = {n.getAttribute("id") for n in doc.getElementsByTagName("p:cNvPr")}
    for timing in list(doc.getElementsByTagName("p:timing")):
        # Each native effect is a p:par with a presetClass cTn. Remove the
        # entire effect (rather than only its target) when content is removed.
        for par in reversed(list(timing.getElementsByTagName("p:par"))):
            ctn = next((x for x in elements(par) if x.tagName == "p:cTn"), None)
            targets = par.getElementsByTagName("p:spTgt")
            if ctn and ctn.hasAttribute("presetClass") and targets and all(x.getAttribute("spid") not in ids for x in targets):
                remove(par)
        for target in list(timing.getElementsByTagName("p:spTgt")):
            if target.getAttribute("spid") not in ids:
                # Conservatively discard the leaf behavior if a mixed native
                # effect references a removed subshape.
                effect = target
                while effect.parentNode and effect.parentNode.tagName not in ("p:childTnLst", "p:tnLst"):
                    effect = effect.parentNode
                remove(effect)
        # Empty wrapper pars have no behavioral targets and can safely go.
        for par in reversed(list(timing.getElementsByTagName("p:par"))):
            if not par.getElementsByTagName("p:spTgt"):
                remove(par)
        for tag in ("p:bldP", "p:bldDgm", "p:bldOleChart", "p:bldGraphic"):
            for node in list(timing.getElementsByTagName(tag)):
                if node.getAttribute("spid") not in ids:
                    remove(node)
        for node in list(timing.getElementsByTagName("p:childTnLst")) + list(timing.getElementsByTagName("p:bldLst")):
            if not elements(node):
                remove(node)
        if not timing.getElementsByTagName("p:spTgt"):
            remove(timing)
    # Some imported connector endpoints reference removed author content.
    for tag in ("a:stCxn", "a:endCxn"):
        for node in list(doc.getElementsByTagName(tag)):
            if node.getAttribute("id") not in ids:
                remove(node)


def safe_rels(data, source, documents):
    doc = minidom.parseString(data)
    used = set()
    if source in documents:
        source_doc = minidom.parseString(documents[source])
        for el in source_doc.getElementsByTagName("*"):
            for i in range(el.attributes.length):
                attr = el.attributes.item(i)
                if attr.namespaceURI == REL[:-1]:
                    used.add(attr.value)
    retained = []
    for rel in list(doc.getElementsByTagName("Relationship")):
        kind = rel.getAttribute("Type").rsplit("/", 1)[-1]
        keep = rel.getAttribute("TargetMode") != "External" and kind in {"slide", "slideMaster", "slideLayout", "theme", "image", "tableStyles"}
        if kind == "image" and rel.getAttribute("Id") not in used:
            keep = False
        if not keep:
            remove(rel)
        else:
            retained.append(posixpath.normpath(posixpath.join(posixpath.dirname(source), rel.getAttribute("Target"))))
    return xml_bytes(doc), retained


def optimize_image(raw, style):
    # Re-encoding removes EXIF and image text metadata as well as needless
    # source resolution. Fluid frames keep RGB channels and full 8-bit alpha.
    im = Image.open(io.BytesIO(raw))
    ext = (im.format or "PNG").lower()
    im.info.clear()
    maximum=1800 if style == "field" else 800
    im.thumbnail((maximum, maximum), Image.Resampling.LANCZOS)
    output = io.BytesIO()
    if ext in ("jpeg", "jpg"):
        im.convert("RGB").save(output, "JPEG", quality=90, optimize=True)
    else:
        # Pearlescent gradients are particularly sensitive to palette color
        # reduction. Keep their full color and alpha at the bounded resolution.
        im = im.convert("RGBA")
        if style == "chrome":
            # Near-lossless RGB precision is enough for studio metal, while a
            # palette produces visible highlight banding. Keep dimensions and
            # every alpha value; only round RGB by at most 2/255 per channel.
            channels = list(im.split())
            for i in range(3):
                channels[i] = channels[i].point(lambda value: round(value / 4) * 4)
            im = Image.merge("RGBA", channels)
        im.save(output, "PNG", optimize=True)
    return output.getvalue()


def make_frame(style, cfg, slide, title, bg, foreground, role="content"):
    title = dict(title)
    title["fontSize"] = max(32, min(title["fontSize"], 44))
    # Preserve the source title's x/y/font but free the former stat slots for
    # authored Markdown. Content remains in the original typographic grid.
    title["rect"] = [title["rect"][0], title["rect"][1], min(874, 916-title["rect"][0]), max(70, title["rect"][3])]
    body = {"rect": [48, 151, 860, 330], "fontSize": 22, "color": cfg["foreground"], "fontFace": cfg["bodyFont"], "align": "left"}
    if role in ("cover", "section"):
        title.update(rect=cfg["coverTitle"], fontSize=cfg["coverSize"], color=cfg["coverColor"])
        body.update(rect=cfg["coverBody"], color=cfg["coverColor"])
    elif role == "image":
        title["rect"] = [title["rect"][0], title["rect"][1], title["rect"][2] if style in ("pearl", "field") else 412, min(140,title["rect"][3])]
        body["rect"] = cfg["imageBody"]
    elif role == "table":
        title["rect"][2] = 858
        body["rect"] = [48, 150, 864, 329]
        body["fontSize"] = 19
    elif role == "closing":
        title["rect"][2] = min(title["rect"][2], 590) if style != "field" else 866
        title["rect"][3] = min(155, max(130, title["rect"][3])) if style != "field" else 70
        body["rect"] = cfg["closingBody"]
    if style == "lightfield" and role == "content":
        # Native cut-glass panels and spectral sculptures occupy the left.
        body["rect"] = [462, 170, 424, 303]
        if slide in (4, 8, 9):
            body["rect"] = [48, 171, 425, 302]
    if style == "chrome" and role == "content":
        # Original artwork alternates sides. Keep an independent safe column.
        if slide in (3,6,7):
            title["rect"]=[462,62,440,88]
            body["rect"]=[462,168,440,307]
        else:
            title["rect"]=[43,62,440,88]
            body["rect"]=[49,168,425,307]
    if style == "pearl" and role == "content":
        body["rect"] = [49, 158, 838, 319]
    # Confluence and Field deliberately alternate dark/light/color surfaces.
    # Body color must follow the selected native slide, not the cover palette.
    luminance = sum(int(bg[i:i+2],16)*w for i,w in ((0,.299),(2,.587),(4,.114)))/255
    if luminance < .46:
        body["color"] = "F9F9FF"
    if style == "field":
        body["color"] = "F9F9FF" if luminance < .46 else "151611"
        title["color"] = body["color"]
        if role == "content" and slide in (4,9):
            title["rect"]=[43,62,440,88]
            body["rect"]=[49,168,425,307]
        if role == "content" and slide in (6,7):
            title["rect"]=[462,62,440,88]
            body["rect"]=[462,168,440,250]
    if style == "lightfield" and role == "closing":
        title["rect"]=[38,102,420,140]
        body["rect"]=[44,270,420,206]
    frame = {"role":role, "slide":slide, "title":title, "body":body, "background":bg,
        "preview":f"previews/{style}-{slide}.png", "foregroundShapeIds":foreground}
    if role == "image":
        frame["imageRect"] = cfg["imageRect"]
    return frame


def compile_style(source_root, style, cfg, fluid_frames=None, field_photos=None):
    path = source_root / "风格" / cfg["source"] / "原版.pptx"
    with zipfile.ZipFile(path) as archive:
        documents = {n: archive.read(n) for n in archive.namelist() if not n.endswith("/")}
    source_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    replacements=replace_media(documents,style,fluid_frames,field_photos)
    pdoc = minidom.parseString(documents["ppt/presentation.xml"])
    size = first(pdoc, "p:sldSz")
    width, height = int(size.getAttribute("cx")), int(size.getAttribute("cy"))
    size.setAttribute("type", "screen16x9")
    for tag in ("p:notesMasterIdLst", "p:embeddedFontLst", "p:custDataLst", "p:extLst"):
        for node in list(pdoc.getElementsByTagName(tag)):
            remove(node)
    documents["ppt/presentation.xml"] = xml_bytes(pdoc)
    slides = {}
    art_shapes = 0
    for slide in range(1, 11):
        name = f"ppt/slides/slide{slide}.xml"
        doc = minidom.parseString(documents[name])
        tree = first(doc, "p:spTree")
        source_shapes = {shape_id(o):o for o in elements(tree)}
        selected = source_shapes.get(cfg["titleIds"][slide])
        if selected is None or not selected.getElementsByTagName("a:t"):
            candidates = [o for o in elements(tree) if o.getElementsByTagName("a:t")]
            # Fallback is deliberately geometric, never source-text matching.
            candidates.sort(key=lambda o: (abs(rect(o)[1]-70), -rect(o)[2]))
            selected = candidates[0]
        title = title_style(selected, cfg)
        bg = background(doc)
        foreground = []
        for obj in list(elements(tree)):
            if obj.tagName in ("p:nvGrpSpPr", "p:grpSpPr"):
                continue
            if not safe_art(obj):
                remove(obj)
            elif any(v in shape_name(obj) for v in ("透光薄缘", "建筑或树影前景", "新海岸前景")):
                foreground.append(shape_id(obj))
        clean_labels(doc)
        clean_timing(doc)
        art_shapes += len(doc.getElementsByTagName("p:cNvPr")) - 1
        documents[name] = xml_bytes(doc)
        slides[slide] = (title, bg, foreground)
    # Layouts/masters are reachable and editable, but must not carry prompt
    # labels or sample metadata in hidden placeholders.
    for name in list(documents):
        if re.fullmatch(r"ppt/(slideMasters|slideLayouts)/[^/]+\.xml", name):
            doc = minidom.parseString(documents[name])
            for t in list(doc.getElementsByTagName("a:t")):
                while t.firstChild:
                    t.removeChild(t.firstChild)
            clean_labels(doc)
            documents[name] = xml_bytes(doc)
    relationships = {}
    for name in list(documents):
        if not name.endswith(".rels") or name == "_rels/.rels":
            continue
        parent = posixpath.dirname(posixpath.dirname(name))
        source = posixpath.join(parent, posixpath.basename(name)[:-5])
        relationships[source] = (name, *safe_rels(documents[name], source, documents))
    # Traverse only the sanitized graph; this excludes every orphan image,
    # chart cache, workbook, notes page, 3D model and embedded font.
    keep, queue = set(), ["ppt/presentation.xml"]
    while queue:
        name = queue.pop()
        if name in keep:
            continue
        if name not in documents:
            raise ValueError(f"Missing package target: {name}")
        keep.add(name)
        if name in relationships:
            rel_name, rel_data, targets = relationships[name]
            documents[rel_name] = rel_data
            keep.add(rel_name)
            queue.extend(targets)
    documents["_rels/.rels"] = b'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>'
    keep.add("_rels/.rels")
    types = minidom.parseString(documents["[Content_Types].xml"])
    for node in list(types.getElementsByTagName("Override")):
        if node.getAttribute("PartName").lstrip("/") not in keep:
            remove(node)
    extensions = {Path(n).suffix.lstrip(".") for n in keep}
    for node in list(types.getElementsByTagName("Default")):
        if node.getAttribute("Extension") not in extensions:
            remove(node)
    documents["[Content_Types].xml"] = xml_bytes(types)
    keep.add("[Content_Types].xml")
    media_before, media_after = 0, 0
    for name in sorted(keep):
        if name.startswith("ppt/media/"):
            raw = documents[name]
            media_before += len(raw)
            documents[name] = optimize_image(raw, style)
            media_after += len(documents[name])
    destination = OUT / f"{style}.pptx"
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name in sorted(keep):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, documents[name])
    roles = []
    for role, slide in (("cover",1),("section",1),("content",3),("image",2),("table",5),("closing",10)):
        title, bg, fg = slides[slide]
        roles.append(make_frame(style,cfg,slide,title,bg,fg,role))
    variants = [make_frame(style,cfg,n,*slides[n]) for n in (4,6,7,8,9)]
    metadata = {"schema":1,"id":f"pptx-{style}","name":cfg["name"],"width":960,"height":540,
        "widthEmu":width,"heightEmu":height,"foreground":cfg["foreground"],"accent":cfg["accent"],
        "roles":roles,"contentVariants":variants,"sourceSha256":source_hash,
        "sourceSlideCount":10,"artShapeCount":art_shapes,
        "mediaCount":sum(n.startswith("ppt/media/") for n in keep),
        "mediaSourceBytes":media_before,"mediaCompiledBytes":media_after}
    if replacements is not None:
        metadata["replacementArt"]=replacements
    (OUT/f"{style}.json").write_text(json.dumps(metadata,ensure_ascii=False,separators=(",",":"))+"\n")
    return metadata


def verify(style):
    if (ROOT / "assets" / "pptx-templates").exists():
        raise ValueError("Compile-time templates must not also be bundled under assets/pptx-templates")
    path = OUT/f"{style}.pptx"
    with zipfile.ZipFile(path) as archive:
        names = set(archive.namelist())
        for name in names:
            if name.endswith(".xml") or name.endswith(".rels"):
                raw = archive.read(name)
                doc = minidom.parseString(raw)
                if any(t.firstChild and t.firstChild.data.strip() for t in doc.getElementsByTagName("a:t")):
                    raise ValueError(f"Sample text survived: {name}")
                if b"/Users/" in raw or b"file:" in raw or b"TargetMode=\"External\"" in raw:
                    raise ValueError(f"Private path/external relationship: {name}")
                if name.endswith(".rels"):
                    parent = posixpath.dirname(posixpath.dirname(name)) if name != "_rels/.rels" else ""
                    for rel in doc.getElementsByTagName("Relationship"):
                        target = posixpath.normpath(posixpath.join(parent,rel.getAttribute("Target")))
                        if target not in names:
                            raise ValueError(f"Dangling relationship: {name} -> {target}")
                if name.startswith("ppt/slides/") and not name.endswith(".rels"):
                    ids = {n.getAttribute("id") for n in doc.getElementsByTagName("p:cNvPr")}
                    if len(ids) != len(doc.getElementsByTagName("p:cNvPr")):
                        raise ValueError(f"Duplicate shape IDs: {name}")
                    for target in doc.getElementsByTagName("p:spTgt"):
                        if target.getAttribute("spid") not in ids:
                            raise ValueError(f"Dangling animation target: {name}")
                    timing_ids = {n.getAttribute("id") for n in doc.getElementsByTagName("p:cTn")}
                    for target in doc.getElementsByTagName("p:tn"):
                        if target.getAttribute("val") not in timing_ids:
                            raise ValueError(f"Dangling timing reference: {name}")
        banned = ("notes", "docProps", "charts/", "embeddings/", "fonts/", "model3d/")
        if any(any(s in n for s in banned) for n in names):
            raise ValueError("Non-art source package data survived")
        prefix={"chrome":"ppt/media/fluid-","field":"ppt/media/field-photo-"}.get(style)
        if prefix and any(n.startswith("ppt/media/") and not n.startswith(prefix) for n in names):
            raise ValueError(f"Old source media survived the replacement in {style}")


def render(metadata, ibis):
    """Render a static art-only seed for preview; production keeps animations."""
    style = metadata["id"].removeprefix("pptx-")
    with tempfile.TemporaryDirectory(prefix="readmd-native-art-") as temporary:
        temp = Path(temporary)
        # Editor stills must show a single metal frame, not all animation
        # poses composited. Keep only the first frame in each native group.
        with zipfile.ZipFile(OUT/f"{style}.pptx") as src, zipfile.ZipFile(temp/"preview.pptx","w",zipfile.ZIP_DEFLATED) as dst:
            for name in src.namelist():
                data = src.read(name)
                if re.fullmatch(r"ppt/slides/slide\d+.xml",name):
                    doc = minidom.parseString(data)
                    for timing in list(doc.getElementsByTagName("p:timing")):
                        remove(timing)
                    if style == "chrome":
                        for group in doc.getElementsByTagName("p:grpSp"):
                            pics = [n for n in elements(group) if n.tagName == "p:pic"]
                            if len(pics) >= 48:
                                for pic in pics[1:]:
                                    remove(pic)
                                prop=first(pics[0],"p:cNvPr")
                                if prop and prop.hasAttribute("hidden"):
                                    prop.removeAttribute("hidden")
                    data=xml_bytes(doc)
                dst.writestr(name,data)
        subprocess.run([ibis,"--export","png","--out-dir",str(temp/"render"),str(temp/"preview.pptx")],check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=180)
        files = sorted((temp/"render").glob("*.png"),key=lambda p:int(re.findall(r"\d+",p.stem)[-1]))
        if len(files) != 10:
            raise ValueError(f"Expected 10 rendered slides, found {len(files)}")
        for slide,file in enumerate(files,1):
            im=Image.open(file).convert("RGB")
            im=im.resize((960,540),Image.Resampling.LANCZOS)
            if style not in ("pearl","chrome","field"):
                im=im.quantize(colors=256)
            im.save(OUT/"previews"/f"{style}-{slide}.png",optimize=True)
    write_slide_cover(style)


def write_slide_cover(style):
    """Keep the template picker cover derived from the current seed preview."""
    covers = ROOT / "assets" / "data" / "slide-covers"
    covers.mkdir(parents=True, exist_ok=True)
    im = Image.open(OUT/"previews"/f"{style}-1.png").convert("RGB")
    im.resize((440, 248), Image.Resampling.LANCZOS).save(
        covers/f"{style}.jpg", "JPEG", quality=88, optimize=True)


def registry(metadata):
    lines = ["// Generated by tools/compile-pptx-templates.py; art-only previews.",
        "pub(super) fn preview(style: &str, slide: usize) -> Option<&'static [u8]> {", "    match (style, slide) {"]
    for meta in metadata:
        style=meta["id"].removeprefix("pptx-")
        for n in range(1,11):
            if (OUT/"previews"/f"{style}-{n}.png").exists():
                lines.append(f'        ("{meta["id"]}", {n}) => Some(include_bytes!("previews/{style}-{n}.png")),')
    lines += ["        _ => None,", "    }", "}", ""]
    (OUT/"previews.rs").write_text("\n".join(lines))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path)
    parser.add_argument("--render",action="store_true")
    parser.add_argument("--fluid-frames",type=Path)
    parser.add_argument("--field-photos",type=Path)
    parser.add_argument("--verify-only",action="store_true")
    parser.add_argument("--ibis",default="/Applications/ibisOffice.app/Contents/MacOS/ibisOffice")
    parser.add_argument("--style",choices=STYLES,action="append")
    args=parser.parse_args()
    if args.verify_only:
        for style in args.style or STYLES:
            verify(style)
            print(f"{style}: verified",flush=True)
        return
    if args.source is None:
        parser.error("--source is required unless --verify-only is used")
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/"previews").mkdir(exist_ok=True)
    metadata=[]
    for style in args.style or STYLES:
        meta=compile_style(args.source,style,STYLES[style],args.fluid_frames,args.field_photos)
        verify(style)
        if args.render:
            render(meta,args.ibis)
        metadata.append(meta)
        print(f"{style}: {(OUT/f'{style}.pptx').stat().st_size:,} bytes; {meta['artShapeCount']} native art shapes; {meta['mediaCount']} art images",flush=True)
    all_meta = [json.loads(p.read_text()) for p in sorted(OUT.glob("*.json")) if p.stem in STYLES]
    registry(all_meta)


if __name__ == "__main__":
    main()
