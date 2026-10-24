"""Independent native mixed-math checks; no ReadMD, GUI or network calls."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("defaults_native_oracle", Path(__file__).with_name("inspect-math-native-styles.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def descend(node):
    yield node
    for child in node.get("children", []):
        yield from descend(child)


def word_text(node):
    return "".join(part.get("text", "") for part in descend(node) if part["name"] == native.w("t"))


def inline_paragraph(inspection):
    matches = [node for node in inspection["bodyChildren"]
               if node["name"] == native.w("p") and word_text(node).startswith("Inline ")]
    assert len(matches) == 1, "Exactly one ordinary Inline paragraph is required"
    node = matches[0]
    maths = [part for part in descend(node) if part["name"] == native.m("oMath")]
    assert len(maths) == 2, "Ordinary Inline paragraph must retain both inline OfficeMath objects"
    assert not any(part["name"] == native.m("oMathPara") for part in descend(node))
    for math_node in maths:
        assert "".join(part.get("text", "") for part in descend(math_node)
                       if part["name"] == native.m("t")) == "x2"
        assert sum(part["name"] == native.m("sSup") for part in descend(math_node)) == 1
    return node


def docx(source_file, result_file, baseline_file):
    source = native.inspect_package(native.Package(source_file))
    result = native.inspect_package(native.Package(result_file))
    baseline = native.inspect_package(native.Package(baseline_file))
    imported = [row for row in result["paragraphs"] if row["mathTexts"] == ["ab"]]
    ordinary = [row for row in result["paragraphs"] if row["mathTexts"] == ["xy"]]
    assert len(baseline["paragraphs"]) == 2, "Ordinary baseline must contain both display formulas"
    assert all(row["mathTexts"] == ["xy"] for row in baseline["paragraphs"])
    assert len(imported) == 3 and len(ordinary) == 2 and len(result["paragraphs"]) == 5
    assert inline_paragraph(result) == inline_paragraph(baseline), "Complete ordinary Inline paragraph changed"
    graph = native.compare_rows(source, result, imported, "a", uncontested=True)
    for old, new in zip(baseline["paragraphs"], ordinary):
        assert old["direct"] == new["direct"], "Bare formula acquired imported paragraph formatting"
        assert old["effective"] == new["effective"], "Bare formula acquired imported paragraph style"
        assert old["mathTexts"] == new["mathTexts"] == ["xy"]
    for identity, definition in baseline["styleContext"]["definitions"].items():
        assert result["styleContext"]["definitions"][identity] == definition, "Template style changed"
    assert result["styleContext"]["docDefaultsTree"] == baseline["styleContext"]["docDefaultsTree"]
    package = native.Package(result_file)
    settings = package.xml(package.related("settings"))
    props = settings.findall("m:mathPr", native.NS)
    assert len(props) == 1
    actual = {node.tag.rsplit("}", 1)[-1]: node.get(native.m("val")) for node in props[0]}
    for key, value in {"dispDef": "0", "preSp": "120", "postSp": "240", "interSp": "40", "intraSp": "60"}.items():
        assert actual.get(key) == value, (key, actual)
    def plain_nodes(inspection):
        nodes = [node for node in inspection["bodyChildren"] if "PLAIN_" in word_text(node)]
        assert len(nodes) == 2, "Both ordinary heading and body markers are required"
        return nodes
    assert plain_nodes(result) == plain_nodes(baseline), "Ordinary paragraphs changed"
    assert not result["auxiliaryParts"], "Internal carrier leaked into OPC parts"
    return {"passed": True, "displayMath": 5, "imported": 3, "ordinary": 2,
            "settings": actual, "nativeBindings": graph, "plainParagraphsUnchanged": True,
            "ordinaryMathStyleIsolated": True, "inlineMath": 2, "inlineParagraphUnchanged": True,
            "applicationEditingCertified": False}


def epub(file, order="ordinary-first"):
    assert order in ("ordinary-first", "native-first")
    parts = native.logical.read_archive(file)
    assert parts.get("mimetype") == b"application/epub+zip"
    container_ns, opf_ns = "urn:oasis:names:tc:opendocument:xmlns:container", "http://www.idpf.org/2007/opf"
    container = native.parse_xml(parts["META-INF/container.xml"])
    assert container.tag == "{" + container_ns + "}container"
    roots = container.findall("{" + container_ns + "}rootfiles/{" + container_ns + "}rootfile")
    assert len(roots) == 1 and roots[0].get("media-type") == "application/oebps-package+xml"
    opf_path = roots[0].get("full-path", "")
    assert opf_path and not unquote(opf_path).startswith(("/", "\\"))
    opf_part = native.opc.member("", opf_path)
    assert opf_part in parts
    opf = native.parse_xml(parts[opf_part])
    assert opf.tag == "{" + opf_ns + "}package"
    manifest, spine = opf.find("{" + opf_ns + "}manifest"), opf.find("{" + opf_ns + "}spine")
    assert manifest is not None and spine is not None
    items = {}
    for item in manifest:
        assert item.tag == "{" + opf_ns + "}item"
        identity, href = item.get("id"), item.get("href", "")
        assert identity and identity not in items
        url = urlsplit(href)
        assert href and not unquote(href).startswith(("/", "\\"))
        assert not url.scheme and not url.netloc and "#" not in href and "?" not in href
        part = native.opc.member(opf_part, href)
        assert part in parts
        items[identity] = (item, part)
    maths, spine_parts = [], []
    ns = "{http://www.w3.org/1998/Math/MathML}"
    for itemref in spine:
        assert itemref.tag == "{" + opf_ns + "}itemref" and itemref.get("idref") in items
        item, part = items[itemref.get("idref")]
        assert item.get("media-type") == "application/xhtml+xml" and part not in spine_parts
        spine_parts.append(part)
        root = native.base.parse_epub_xhtml(parts[part])
        native.logical.assert_visible_math_roots(ET.tostring(root, encoding="unicode"), xml_root=root)
        assert not any(native.opc.local(node.tag) in {"svg", "img", "object"} for node in root.iter()), "Unreviewed EPUB visual surrogate"
        maths.extend(root.iter(ns + "math"))
    assert spine_parts and len(maths) == 7, "Selected EPUB spine lost/duplicated formulas"
    ordinary = [("x2", "inline", "msup"), ("x2", "inline", "msup"),
                ("xy", "block", "mfrac"), ("xy", "block", "mfrac")]
    imported = [("ab", "block", "mfrac")] * 3
    expected = ordinary + imported if order == "ordinary-first" else imported + ordinary
    padded = 0
    for node, (tokens, display, structure) in zip(maths, expected):
        actual_tokens = "".join("".join(part.itertext()) for part in node.iter()
                                if part.tag in {ns + name for name in ("mi", "mn", "mo", "mtext", "ms")})
        assert actual_tokens == tokens and node.get("display") == display, (actual_tokens, display, tokens)
        assert len(list(node.iter(ns + structure))) == 1
        assert not list(node.iter(ns + ("mfrac" if structure == "msup" else "msup")))
        spacing = [part for part in node.iter(ns + "mpadded") if part.get("height") is not None or part.get("depth") is not None]
        if display == "block":
            assert len(spacing) == 1 and spacing[0].get("height") == "+6pt" and spacing[0].get("depth") == "+12pt"
            padded += 1
        else:
            assert not spacing, "Inline math acquired display spacing"
    assert padded == 5
    return {"passed": True, "mathml": len(maths), "displaySpacing": padded, "sourceOrder": order,
            "tokensAndStructures": expected, "spineParts": spine_parts, "readerApplicationCertified": False}


def pdf(file):
    import pymupdf as fitz
    with fitz.open(file) as document:
        assert 0 < len(document) <= 8
        text = "".join(page.get_text() for page in document)
        assert "PLAIN_CONTROL" in text
        visible = "".join(visible_trace_text(page.get_texttrace()) for page in document)
        images = sum(len(page.get_images()) for page in document)
        drawings = sum(len(page.get_drawings()) for page in document)
        assert images == 0 and drawings >= 5, "Mathematics must remain vector graphics"
        return {"passed": True, "pages": len(document), "images": images, "vectorDrawings": drawings, "visibleText": visible}


def visible_trace_text(traces):
    return "".join(chr(char[0]) for trace in traces
                   if trace["type"] in (0, 1, 2) and trace.get("opacity", 1) > 0
                   for char in trace["chars"])


def expected_math_copy(order, control=False):
    assert order in ("ordinary-first", "native-first")
    ordinary = (["{x}^{2}"] if control else ["x^2"]) * 2 + [r"\frac{x}{y}"] * 2
    imported = [r"\frac{a}{b}"] * 3
    return ordinary + imported if order == "ordinary-first" else imported + ordinary


def compare_pdf(actual_file, control_file, math_order=None, engine=None):
    import pymupdf as fitz
    assert (math_order is None) == (engine is None), "An explicit math-copy profile requires both order and engine"
    assert engine in (None, "rust", "typst")
    assert math_order in (None, "ordinary-first", "native-first")
    tolerance = .002
    def close(a, b):
        assert abs(a - b) <= tolerance, (a, b)
    with fitz.open(actual_file) as actual, fitz.open(control_file) as control:
        assert len(actual) == len(control) > 0
        actual_traces = [page.get_texttrace() for page in actual]
        control_traces = [page.get_texttrace() for page in control]
        hidden_actual = ["".join(chr(char[0]) for char in trace["chars"])
                         for page in actual_traces for trace in page if trace["type"] == 3]
        hidden_control = ["".join(chr(char[0]) for char in trace["chars"])
                          for page in control_traces for trace in page if trace["type"] == 3]
        copy_proof = {"profile": engine or "strict", "isolatedInvisibleMathSpans": 0}
        actual_copy_fonts, control_copy_fonts = set(), set()
        if engine == "rust":
            # These seven known copy spans are not visible ink. Validate every
            # hidden span, in full document/page order, before separating them.
            # The two inline DSL controls have canonical braces; no visible
            # text is rewritten and no coordinate is used to guess identity.
            assert hidden_actual == expected_math_copy(math_order), ("Unexpected/missing actual hidden math copy", hidden_actual)
            assert hidden_control == expected_math_copy(math_order, control=True), ("Unexpected/missing control hidden math copy", hidden_control)
            copy_proof.update({"order": math_order, "isolatedInvisibleMathSpans": 7,
                               "actual": hidden_actual, "control": hidden_control,
                               "copyGeometryCompared": False, "visibleInkComparedSeparately": True})
            # Only after the complete hidden corpus passed may the dedicated,
            # non-painting copy resource be separated from visible font shapes.
            actual_copy_fonts, actual_font_proof = rust_copy_fonts(actual, actual_traces)
            control_copy_fonts, control_font_proof = rust_copy_fonts(control, control_traces)
            copy_proof["copyOnlyFontResources"] = {"actual": actual_font_proof, "control": control_font_proof}
        elif engine == "typst":
            assert not hidden_actual and not hidden_control, "Typst profile must have no hidden text spans"
        paths = glyphs = 0
        for a, b, left_text, right_text in zip(actual, control, actual_traces, control_traces):
            if engine == "rust":
                assert visible_trace_text(left_text) == visible_trace_text(right_text), "Control changed visible prose"
                left_text = [trace for trace in left_text if trace["type"] != 3]
                right_text = [trace for trace in right_text if trace["type"] != 3]
            else:
                assert a.get_text() == b.get_text(), "Control changed visible prose"
            assert list(a.mediabox) == list(b.mediabox) and list(a.cropbox) == list(b.cropbox)
            assert a.rotation == b.rotation and not a.get_images() and not b.get_images()
            assert pdf_font_shapes(actual, a, actual_copy_fonts) == pdf_font_shapes(control, b, control_copy_fonts), "Actual PDF font programs/shapes changed"
            assert len(left_text) == len(right_text), "Actual PDF text trace count changed"
            for p, q in zip(left_text, right_text):
                for key in ("font", "flags", "type", "wmode", "bidi_lvl", "bidi_dir", "color", "colorspace", "opacity", "dir", "ascender", "descender", "linewidth"):
                    assert p.get(key) == q.get(key), (key, p.get(key), q.get(key))
                assert p["type"] in (0, 1, 2) and 0 < p.get("opacity", 1) <= 1, "Invisible prose cannot certify actual paint"
                close(p["size"], q["size"])
                assert len(p["bbox"]) == len(q["bbox"]) == 4
                for x, y in zip(p["bbox"], q["bbox"]):
                    close(x, y)
                assert len(p["chars"]) == len(q["chars"])
                for x, y in zip(p["chars"], q["chars"]):
                    assert len(x) == len(y) == 4 and x[:2] == y[:2], "Unicode/GID shape identity changed"
                    for xs, ys, length in ((x[2], y[2], 2), (x[3], y[3], 4)):
                        assert len(xs) == len(ys) == length
                        for px, py in zip(xs, ys):
                            close(px, py)
                    glyphs += 1
            left, right = a.get_drawings(), b.get_drawings()
            assert len(left) == len(right)
            for p, q in zip(left, right):
                for key in ("fill", "color", "width", "closePath", "type", "lineCap", "lineJoin", "fill_opacity", "stroke_opacity"):
                    assert p.get(key) == q.get(key), (key, p.get(key), q.get(key))
                for x, y in zip(p["rect"], q["rect"]):
                    close(x, y)
                assert len(p["items"]) == len(q["items"])
                for x, y in zip(p["items"], q["items"]):
                    x, y = native.base.drawing_item(x), native.base.drawing_item(y)
                    assert x["operation"] == y["operation"] and x["scalar"] == y["scalar"]
                    assert len(x["coordinates"]) == len(y["coordinates"])
                    for xs, ys in zip(x["coordinates"], y["coordinates"]):
                        assert len(xs) == len(ys)
                        for px, py in zip(xs, ys):
                            close(px, py)
                paths += 1
        assert paths >= 5 and glyphs > 0
        return {"passed": True, "allDrawingPaths": paths, "coordinateTolerancePt": tolerance,
                "allGlyphsBarsStylesAndProseCompared": True, "textTraceGlyphs": glyphs,
                "fontProgramsOrStandardBase14Compared": True, "mathCopy": copy_proof, "pages": len(actual)}


def rust_copy_fonts(document, traces):
    # texttrace exposes a font name, not a resource xref. This finite corpus
    # uses one Type3 copy font; reject same-name xref ambiguity rather than
    # guessing which resource a trace used. A visible reference anywhere in
    # the document prevents this exception, even on a different page.
    identity = "ReadMDMathText"
    candidates = {}
    for page in document:
        for record in page.get_fonts(full=True):
            xref, extension, kind, base, *_ = record
            name, extracted_extension, subtype, program = document.extract_font(xref)
            if identity not in (base, name):
                continue
            assert base == name == identity and kind == subtype == "Type3" and not program, "Ambiguous mathematics copy font"
            assert document.xref_get_key(xref, "Name") == ("name", "/" + identity)
            assert document.xref_get_key(xref, "BaseFont") == ("name", "/" + identity)
            candidates[xref] = record
    if not candidates:
        return set(), []
    assert len(candidates) == 1, "Ambiguous mathematics copy font resources"
    used = [trace for page in traces for trace in page
            if re.sub(r"^[A-Z]{6}\+", "", trace["font"]) == identity]
    assert used and all(trace["font"] == identity and trace["type"] == 3 for trace in used), "Mathematics copy font used by visible text"
    xref = next(iter(candidates))
    kind, procedures = document.xref_get_key(xref, "CharProcs")
    assert kind == "dict" and re.fullmatch(r"<<\s*(?:/g[1-9][0-9]*\s+[1-9][0-9]*\s+0\s+R\s*)+>>", procedures), "Unreviewed mathematics copy glyph procedures"
    refs = set(int(value) for value in re.findall(r"/g[1-9][0-9]*\s+([1-9][0-9]*)\s+0\s+R", procedures))
    assert refs and all(document.xref_stream(ref) == b"1000 0 0 0 1000 1000 d1\n" for ref in refs), "Mathematics copy font must not paint"
    kind, reference = document.xref_get_key(xref, "ToUnicode")
    assert kind == "xref" and re.fullmatch(r"[1-9][0-9]* 0 R", reference)
    cmap = document.xref_stream(int(reference.split()[0]))
    assert cmap and b"beginbfchar" in cmap and b"endcmap" in cmap
    return {xref}, [{"xref": xref, "font": identity, "type": "Type3", "hiddenSpans": len(used),
                     "nonPaintingGlyphPrograms": len(refs), "toUnicodeSha256": hashlib.sha256(cmap).hexdigest(),
                     "visibleUse": False, "excludedFromVisibleFontComparison": True}]


def pdf_font_shapes(document, page, copy_only_xrefs=frozenset()):
    # Font names alone do not identify outlines. Embedded program bytes bind
    # the compared GIDs; unembedded fonts are limited to standard Base14 Type1.
    base14 = {"Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique", "Helvetica", "Helvetica-Bold",
              "Helvetica-Oblique", "Helvetica-BoldOblique", "Times-Roman", "Times-Bold", "Times-Italic",
              "Times-BoldItalic", "Symbol", "ZapfDingbats"}
    records = []
    for xref, extension, kind, base, resource, encoding, *rest in page.get_fonts(full=True):
        if xref in copy_only_xrefs:
            continue
        name, extracted_extension, subtype, program = document.extract_font(xref)
        plain = re.sub(r"^[A-Z]{6}\+", "", base)
        assert program or kind == "Type1" and plain in base14, "Unreviewed unembedded PDF font shape"
        records.append((plain, extension, kind, encoding, extracted_extension, subtype,
                        hashlib.sha256(program).hexdigest() if program else "standard-base14:" + plain))
    return sorted(records)


def compare_svg(actual_file, control_file):
    # Unlike the earlier two-glyph fraction oracle, inline superscripts need
    # not contain a fraction bar. Inspect the entire visible ink tree instead.
    actual, control = (ET.fromstring(file.read_bytes()) for file in (actual_file, control_file))
    assert actual.tag == control.tag == "{http://www.w3.org/2000/svg}svg"
    assert actual.attrib == control.attrib, "SVG root paint/layout attributes changed"
    a, b = ([node for node in root.iter() if node.get("data-math-ink") == "true"] for root in (actual, control))
    assert len(a) == len(b) == 1
    assert a[0] in list(actual) and b[0] in list(control), "Visible ink must be a direct SVG child"
    left, right = list(a[0].iter()), list(b[0].iter())
    assert len(left) == len(right) > 1
    primitives = 0
    number = r"[-+]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?"
    for p, q in zip(left, right):
        paint_p = {key: value for key, value in p.attrib.items() if key != "data-math-owners"}
        paint_q = {key: value for key, value in q.attrib.items() if key != "data-math-owners"}
        assert p.tag == q.tag and set(paint_p) == set(paint_q)
        tag = p.tag.rsplit("}", 1)[-1]
        assert tag in {"g", "path", "rect"}, "Unreviewed visible ink primitive"
        primitives += tag != "g"
        for key, value in paint_p.items():
            other = paint_q[key]
            if key in {"transform", "x", "y", "width", "height"}:
                assert re.sub(number, "#", value) == re.sub(number, "#", other)
                xs, ys = re.findall(number, value), re.findall(number, other)
                assert len(xs) == len(ys)
                for x, y in zip(xs, ys):
                    assert math.isfinite(float(x)) and math.isfinite(float(y))
                    assert abs(float(x) - float(y)) * .012 <= .00003, (key, value, other)
            else:
                assert value == other, (key, value, other)
    assert primitives > 0
    return {"passed": True, "allPrimitives": primitives, "coordinateTolerancePt": .00003,
            "allGlyphsBarsAndPaintCompared": True, "ignoredSourceIdentityAttributes": ["data-math-owners"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    d = sub.add_parser("docx")
    for name in ("source", "result", "baseline"):
        d.add_argument("--" + name, type=Path, required=True)
    book = sub.add_parser("epub")
    book.add_argument("file", type=Path)
    book.add_argument("--order", choices=("ordinary-first", "native-first"), default="ordinary-first")
    sub.add_parser("pdf").add_argument("file", type=Path)
    for name in ("compare-pdf", "compare-svg"):
        command = sub.add_parser(name)
        command.add_argument("--actual", type=Path, required=True)
        command.add_argument("--control", type=Path, required=True)
        if name == "compare-pdf":
            command.add_argument("--math-order", choices=("ordinary-first", "native-first"))
            command.add_argument("--engine", choices=("rust", "typst"))
    args = parser.parse_args()
    if args.operation == "docx":
        result = docx(args.source, args.result, args.baseline)
    elif args.operation == "epub":
        result = epub(args.file, args.order)
    elif args.operation == "compare-pdf":
        result = compare_pdf(args.actual, args.control, math_order=args.math_order, engine=args.engine)
    elif args.operation.startswith("compare-"):
        result = globals()[args.operation.replace("-", "_")](args.actual, args.control)
    else:
        result = globals()[args.operation](args.file)
    print(json.dumps(result, ensure_ascii=True))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
