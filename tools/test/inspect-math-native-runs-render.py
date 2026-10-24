"""QA-only finite native run-style paint oracle; never a product dependency.

The three-fraction corpus belongs to this unit. An invisible copy font is
separated only after the complete copy text and its nonpainting program pass.
All visible font programs, GIDs, paint primitives and coordinates still match.
"""
import argparse
import hashlib
from html.parser import HTMLParser
import importlib.util
import json
import math
from pathlib import Path
import re
import stat
import sys
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit
import zipfile

_spec = importlib.util.spec_from_file_location("native_runs_render_base", Path(__file__).with_name("inspect-math-document-defaults.py"))
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
M = "{http://www.w3.org/1998/Math/MathML}"
logical = base.native.logical
opc = logical.opc
X = "{http://www.w3.org/1999/xhtml}"


def verify_copy(traces, engine):
    hidden = ["".join(chr(c[0]) for c in trace["chars"]) for page in traces for trace in page if trace["type"] == 3]
    if engine == "rust":
        assert hidden == [r"\frac{a}{b}"] * 3, ("Unexpected finite native-runs copy corpus", hidden)
    else:
        assert engine == "typst" and not hidden, "Unexpected hidden text in the Typst output"
    return hidden


def compare_pdf(actual_file, control_file, engine):
    import pymupdf as fitz
    tolerance = .002
    def close(a, b):
        assert math.isfinite(a) and math.isfinite(b) and abs(a-b) <= tolerance, (a, b)
    with fitz.open(actual_file) as actual, fitz.open(control_file) as control:
        assert len(actual) == len(control) and 0 < len(actual) <= 8
        at, ct = ([page.get_texttrace() for page in document] for document in (actual, control))
        ac, cc = verify_copy(at, engine), verify_copy(ct, engine)
        ax, ap = base.rust_copy_fonts(actual, at) if engine == "rust" else (set(), [])
        cx, cp = base.rust_copy_fonts(control, ct) if engine == "rust" else (set(), [])
        assert (engine != "rust") or ax and cx, "Expected the proven unique nonpainting copy resource"
        glyphs = paths = 0
        for a, b, ta, tb in zip(actual, control, at, ct):
            assert list(a.mediabox) == list(b.mediabox) and list(a.cropbox) == list(b.cropbox) and a.rotation == b.rotation
            assert not a.get_images() and not b.get_images(), "A raster page cannot prove mathematical paint"
            assert base.pdf_font_shapes(actual, a, ax) == base.pdf_font_shapes(control, b, cx), "Visible font program changed"
            assert base.visible_trace_text(ta) == base.visible_trace_text(tb), "Visible prose changed"
            ta, tb = ([t for t in values if t["type"] != 3] for values in (ta, tb))
            assert len(ta) == len(tb)
            for p, q in zip(ta, tb):
                for key in ("font", "flags", "type", "wmode", "bidi_lvl", "bidi_dir", "color", "colorspace", "opacity", "dir", "ascender", "descender", "linewidth"):
                    assert p.get(key) == q.get(key), (key, p.get(key), q.get(key))
                assert p["type"] in (0, 1, 2) and 0 < p.get("opacity", 1) <= 1
                close(p["size"], q["size"])
                assert len(p["bbox"]) == len(q["bbox"]) == 4
                for x, y in zip(p["bbox"], q["bbox"]): close(x, y)
                assert len(p["chars"]) == len(q["chars"])
                for x, y in zip(p["chars"], q["chars"]):
                    assert len(x) == len(y) == 4 and x[:2] == y[:2], "Visible Unicode or glyph ID changed"
                    for xs, ys, length in ((x[2], y[2], 2), (x[3], y[3], 4)):
                        assert len(xs) == len(ys) == length
                        for px, py in zip(xs, ys): close(px, py)
                    glyphs += 1
            ad, bd = a.get_drawings(), b.get_drawings()
            assert len(ad) == len(bd)
            for p, q in zip(ad, bd):
                for key in ("fill", "color", "width", "closePath", "type", "lineCap", "lineJoin", "fill_opacity", "stroke_opacity"):
                    assert p.get(key) == q.get(key), (key, p.get(key), q.get(key))
                assert len(p["rect"]) == len(q["rect"]) == 4
                for x, y in zip(p["rect"], q["rect"]): close(x, y)
                assert len(p["items"]) == len(q["items"])
                for x, y in zip(p["items"], q["items"]):
                    x, y = base.native.base.drawing_item(x), base.native.base.drawing_item(y)
                    assert x["operation"] == y["operation"] and x["scalar"] == y["scalar"]
                    assert len(x["coordinates"]) == len(y["coordinates"])
                    for xs, ys in zip(x["coordinates"], y["coordinates"]):
                        assert len(xs) == len(ys)
                        for px, py in zip(xs, ys): close(px, py)
                paths += 1
        assert glyphs > 0 and paths >= 3
        return {"passed": True, "engine": engine, "pages": len(actual), "allDrawingPaths": paths,
                "visibleGlyphs": glyphs, "coordinateTolerancePt": tolerance,
                "allVisibleFontProgramsGidsPaintAndPositionsCompared": True,
                "copyCorpus": {"actual": ac, "control": cc, "actualFonts": ap, "controlFonts": cp,
                               "profile": "three-native-fractions", "otherHiddenTextAllowed": False}}


def internal_part(part, href):
    url = urlsplit(href)
    assert href and not url.scheme and not url.netloc and not url.query and not url.fragment
    assert not href.startswith("/")
    return opc.member(part, href)


def finite_css(text):
    assert "@import" not in text.lower(), "Unreviewed imported stylesheet"
    for declarations in re.findall(r"\{([^{}]*)\}", text):
        assert not logical.hidden("div", {"style": declarations}), "Hidden rule in finite selected-output stylesheet"


def epub_inventory(file):
    # Reuse the established ZIP/DTD/UTF-8 admission and actual spine traversal.
    assert file.is_file() and not file.is_symlink()
    parts = logical.read_archive(file)
    with zipfile.ZipFile(file) as archive:
        assert not any(stat.S_ISLNK(item.external_attr >> 16) or item.is_dir() for item in archive.infolist()), "Nonordinary EPUB member"
    assert parts.get("mimetype") == b"application/epub+zip"
    container_ns, opf_ns = "{urn:oasis:names:tc:opendocument:xmlns:container}", "{http://www.idpf.org/2007/opf}"
    container = logical.parse_xml(parts["META-INF/container.xml"])
    assert container.tag == container_ns + "container"
    rootfiles = container.findall(container_ns + "rootfiles/" + container_ns + "rootfile")
    assert len(rootfiles) == 1 and rootfiles[0].get("media-type") == "application/oebps-package+xml"
    opf_part = internal_part("", rootfiles[0].get("full-path", ""))
    opf = logical.parse_xml(parts[opf_part])
    assert opf.tag == opf_ns + "package"
    manifests, spines = opf.findall(opf_ns + "manifest"), opf.findall(opf_ns + "spine")
    assert len(manifests) == len(spines) == 1
    manifest, paths = {}, set()
    for node in manifests[0]:
        assert node.tag == opf_ns + "item"
        identity = node.get("id")
        assert identity and identity not in manifest
        name = internal_part(opf_part, node.get("href", ""))
        assert name in parts and name not in paths, "Duplicate/dangling manifest path"
        paths.add(name)
        manifest[identity] = (node, name)
    maths, selected, contexts, sheets = [], [], [], {}
    assert len(spines[0]) > 0
    for reference in spines[0]:
        assert reference.tag == opf_ns + "itemref" and reference.get("idref") in manifest
        node, name = manifest[reference.get("idref")]
        assert node.get("media-type") == "application/xhtml+xml" and name not in selected
        assert reference.get("linear", "yes") == "yes", "Nonlinear content cannot supply the finite reading corpus"
        selected.append(name)
        root = logical.base.parse_epub_xhtml(parts[name])
        assert len(root.findall(X + "body")) == 1 and len(root.findall(X + "head")) == 1
        logical.assert_visible_math_roots("", xml_root=root)
        assert not any(opc.local(child.tag) in {"svg", "img", "object", "iframe", "script"} for child in root.iter()), "Unreviewed EPUB visual surrogate"
        for link in root.iter(X + "link"):
            assert link.get("rel") == "stylesheet" and link.get("type", "text/css") == "text/css"
            sheet = internal_part(name, link.get("href", ""))
            assert sheet in parts and any(item_name == sheet and item.get("media-type") == "text/css" for item, item_name in manifest.values())
            css = parts[sheet].decode("utf-8")
            finite_css(css)
            sheets[sheet] = hashlib.sha256(parts[sheet]).hexdigest()
        for style in root.iter(X + "style"):
            finite_css("".join(style.itertext()))
            sheets[name + "#inline-" + str(len(sheets))] = hashlib.sha256(ET.tostring(style)).hexdigest()
        def visit(current, ancestors):
            if current.tag == M + "math":
                assert any(parent.tag == X + "body" for parent in ancestors), "MathML must be selected from actual XHTML body"
                assert all(parent.tag in {X+tag for tag in ("html","body","main","article","section","div","p","span")} for parent in ancestors), "Unproved MathML ancestor"
                assert all(child.tag.startswith(M) for child in current.iter()), "Foreign MathML subtree"
                assert not any(opc.local(child.tag) in {"annotation", "annotation-xml", "mglyph", "merror"} for child in current.iter())
                fractions = list(current.iter(M + "mfrac"))
                assert len(fractions) == 1 and len(fractions[0]) == 2
                tokens = lambda element: ["".join(child.itertext()) for child in element.iter() if opc.local(child.tag) in {"mi", "mn", "mo", "mtext"}]
                assert tokens(fractions[0][0]) == ["a"] and tokens(fractions[0][1]) == ["b"], "Owned fraction operands missing/reordered"
                assert tokens(current) == ["a", "b"]
                maths.append(current)
                contexts.append([{"tag": parent.tag, "attributes": dict(sorted(parent.attrib.items()))} for parent in ancestors])
            for child in current:
                visit(child, ancestors + [current])
        visit(root, [])
        if any(current.tag == M + "math" for current in root.iter()):
            assert "mathml" in node.get("properties", "").split(), "Spine math was not declared in the real manifest"
    assert len(maths) == 3
    return {"maths": maths, "spine": selected, "contexts": contexts, "stylesheets": sheets}


def epub_math(file):
    inventory = epub_inventory(file)
    return inventory["maths"], inventory["spine"]


def canonical(node):
    return {"tag":node.tag,"attributes":dict(sorted(node.attrib.items())),"text":node.text or "",
            "children":[canonical(child) for child in node]}


def compare_epub(actual,control):
    a, b = epub_inventory(actual), epub_inventory(control)
    assert [canonical(node) for node in a["maths"]]==[canonical(node) for node in b["maths"]], "MathML structure, styles or values changed"
    assert a["contexts"] == b["contexts"], "Visible MathML ancestor context changed"
    assert a["stylesheets"] == b["stylesheets"], "Selected EPUB stylesheet bytes changed"
    return {"passed":True,"mathml":3,"allStructureAttributesAndTextCompared":True,
            "visibleBodyAndAncestorsChecked":True,"selectedStylesheetsCompared":True,
            "actualSpine":a["spine"],"controlSpine":b["spine"],"readerApplicationCertified":False}


def html_vectors(input_file, output_dir):
    assert input_file.is_file() and not input_file.is_symlink() and 0 < input_file.stat().st_size <= 16*1024*1024
    original = input_file.read_bytes()
    text = original.decode("utf-8")
    logical.assert_visible_math_roots(text)
    # HTMLParser positions count LF only. Bare CR/CRCRLF remain source bytes,
    # rather than being mistaken for additional line boundaries.
    offsets = [0] + [match.end() for match in re.finditer("\n", text)]
    class Selected(HTMLParser):
        def __init__(self):
            super().__init__(convert_charrefs=False)
            self.stack, self.wrappers, self.vectors = [], [], []
        def position(self):
            line, column = self.getpos()
            return offsets[line - 1] + column
        def handle_starttag(self, tag, attributes):
            attrs = dict(attributes)
            assert len(attrs) == len(attributes), "Duplicate selected HTML attribute"
            assert tag not in {"style", "script"} and not (tag == "link" and attrs.get("rel") == "stylesheet"), "Selected HTML cannot introduce CSS/script policy"
            assert tag not in {"math","img","object","iframe","canvas","embed"}, "Unreviewed selected HTML visual surrogate"
            invisible = logical.hidden(tag, attrs) or any(frame["hidden"] for frame in self.stack)
            classes = attrs.get("class", "").split()
            wrapper = None
            if "readmd-export-math" in classes:
                assert not any(frame["wrapper"] is not None for frame in self.stack), "Nested math wrapper"
                assert not invisible and all(frame["tag"] in {"html", "body", "main", "article", "section", "div"} for frame in self.stack), "Unproved math wrapper ancestor"
                for frame in self.stack:
                    assert "style" not in frame["attrs"], "Unproved styled ancestor of selected math"
                assert tag == "div" and attrs == {"class":"readmd-export-math readmd-export-math-block",
                                                "style":"text-align:left","role":"math","aria-label":r"\frac{a}{b}"}, "Unexpected selected math wrapper"
                wrapper = len(self.wrappers)
                self.wrappers.append({"ordinal":wrapper,"svgCount":0})
            current = {"tag":tag,"attrs":attrs,"hidden":invisible,"wrapper":wrapper,"start":self.position(),"svgStart":None}
            if tag == "svg":
                containers = [frame for frame in self.stack if frame["wrapper"] is not None]
                assert len(containers) == 1 and not any(frame["tag"] == "svg" for frame in self.stack), "SVG not in one actual math wrapper"
                assert not invisible
                current["svgStart"] = self.position()
                current["owner"] = containers[0]["wrapper"]
                self.wrappers[current["owner"]]["svgCount"] += 1
            if tag not in {"br","hr","img","input","meta","link","source","wbr","area","base","col","embed","param","track"}:
                self.stack.append(current)
        def handle_startendtag(self, tag, attrs):
            self.handle_starttag(tag, attrs)
            if self.stack and self.stack[-1]["tag"] == tag:
                self.handle_endtag(tag)
        def handle_endtag(self, tag):
            assert self.stack and self.stack[-1]["tag"] == tag, "Malformed selected HTML nesting"
            frame = self.stack.pop()
            if frame["svgStart"] is not None:
                end = text.index(">", self.position()) + 1
                raw = text[frame["svgStart"]:end].encode("utf-8")
                svg = logical.parse_xml(raw)
                assert svg.tag == "{http://www.w3.org/2000/svg}svg"
                logical.assert_visible_math_roots("", xml_root=svg)
                ink = [node for node in svg.iter() if node.get("data-math-ink") == "true"]
                assert len(ink) == 1 and ink[0] in list(svg)
                assert any(node.tag == "{http://www.w3.org/2000/svg}path" and node.get("d") for node in ink[0].iter())
                self.vectors.append({"ordinal":frame["owner"],"raw":raw,
                                     "from":len(text[:frame["svgStart"]].encode("utf-8")),"to":len(text[:end].encode("utf-8"))})
    parser = Selected()
    parser.feed(text)
    parser.close()
    assert not parser.stack and len(parser.wrappers) == len(parser.vectors) == 3
    assert all(row["svgCount"] == 1 for row in parser.wrappers)
    assert [row["ordinal"] for row in parser.vectors] == [0,1,2]
    assert output_dir.parent.is_dir() and not output_dir.is_symlink()
    if output_dir.exists():
        assert output_dir.is_dir() and not list(output_dir.iterdir()), "HTML vector output must be a new empty owned directory"
    else:
        output_dir.mkdir()
    vectors = []
    for row in parser.vectors:
        file = output_dir / ("math-" + str(row["ordinal"]) + ".svg")
        with file.open("xb") as stream:
            stream.write(row["raw"])
        assert file.read_bytes() == original[row["from"]:row["to"]]
        vectors.append({"ordinal":row["ordinal"],"path":str(file),"sha256":hashlib.sha256(row["raw"]).hexdigest(),
                        "bytes":len(row["raw"]),"sourceByteRange":{"from":row["from"],"to":row["to"]}})
    return {"passed":True,"vectors":vectors,"inputSha256":hashlib.sha256(original).hexdigest(),
            "originalSvgBytesPreserved":True,"visibleWrappersAndAncestorsChecked":True,
            "alignment":"left","readerApplicationCertified":False}


def main():
    parser=argparse.ArgumentParser(description=__doc__); sub=parser.add_subparsers(dest="operation",required=True)
    for name in ("compare-pdf","compare-svg","compare-epub"):
        p=sub.add_parser(name);p.add_argument("--actual",type=Path,required=True);p.add_argument("--control",type=Path,required=True)
        if name=="compare-pdf": p.add_argument("--engine",choices=("rust","typst"),required=True)
    html = sub.add_parser("html")
    html.add_argument("--input",type=Path,required=True)
    html.add_argument("--output-dir",type=Path,required=True)
    args=parser.parse_args()
    if args.operation=="compare-pdf": result=compare_pdf(args.actual,args.control,args.engine)
    elif args.operation=="compare-svg": result=base.compare_svg(args.actual,args.control)
    elif args.operation=="compare-epub": result=compare_epub(args.actual,args.control)
    else: result=html_vectors(args.input,args.output_dir)
    print(json.dumps(result,ensure_ascii=True))


if __name__=="__main__":
    sys.stdout.reconfigure(encoding="utf-8"); main()
