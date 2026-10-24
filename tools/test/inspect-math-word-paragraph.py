"""Independent bounded Word paragraph/OfficeMath OPC fixtures and inspection.

Reuses the existing standard-library OPC implementation. No product import,
GUI, engine, network, source-payload authority or layout simulation is used.
"""
import argparse
import copy
import importlib.util
import json
import math
from html.parser import HTMLParser
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET

_spec = importlib.util.spec_from_file_location(
    "settings_opc", Path(__file__).with_name("inspect-docx-settings-roundtrip.py"))
opc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(opc)
W, M = opc.W, "http://schemas.openxmlformats.org/officeDocument/2006/math"
NS = {"w": W, "m": M}
KINDS = ("direct-no-styles", "styles-chain", "empty-zero")
SUPPORTED_JC = {"left", "center", "right"}
WORD_JC = SUPPORTED_JC | {"start", "end", "both", "mediumKashida", "distribute", "numTab", "highKashida", "lowKashida", "thaiDistribute"}
IND_AXES = ("left", "right")
WORD_IND = set(IND_AXES) | {"start", "end", "leftChars", "rightChars", "startChars", "endChars", "firstLine", "hanging", "firstLineChars", "hangingChars"}
MEASURE = r"(?:[+-]?[0-9]+|[+-]?(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:mm|cm|in|pt|pc|pi))"


def w(name):
    return "{" + W + "}" + name


def m(name):
    return "{" + M + "}" + name


def tree(node):
    return {"name": node.tag, "attributes": dict(sorted(node.attrib.items())),
            "text": node.text or "", "children": [tree(child) for child in node]}


def declaration(pr):
    result = {"pPrPresent": pr is not None, "indPresent": False,
              "jcPresent": False, "pStylePresent": False, "axes": {}, "guards": {}}
    if pr is None:
        return result
    assert pr.tag == w("pPr")
    selected = {}
    for child in pr:
        assert child.tag.startswith("{" + W + "}"), "Foreign paragraph property namespace"
        name = opc.local(child.tag)
        assert name not in selected, "Duplicate native paragraph property " + name
        selected[name] = child
    names = [opc.local(child.tag) for child in pr]
    indices = [names.index(name) for name in ("pStyle", "ind", "jc") if name in names]
    assert indices == sorted(indices), "Selected paragraph particle order"
    if "pStyle" in selected:
        result["pStylePresent"] = True
        assert set(selected["pStyle"].attrib) == {w("val")} and not len(selected["pStyle"])
        result["pStyle"] = opc.val(selected["pStyle"])
        assert result["pStyle"], "Required pStyle value"
    if "jc" in selected:
        result["jcPresent"] = True
        node = selected["jc"]
        assert set(node.attrib) == {w("val")} and not len(node), "Required native jc value/namespace"
        value = opc.val(node)
        assert value in WORD_JC, "Invalid Word jc enum"
        result["axes"]["jc"] = value
    if "ind" in selected:
        result["indPresent"] = True
        node = selected["ind"]
        assert not len(node), "Native ind leaf"
        for key, value in node.attrib.items():
            assert key.startswith("{" + W + "}"), "Foreign native ind attribute"
            name = opc.local(key)
            assert name in WORD_IND, "Unknown native ind attribute"
            if name in IND_AXES:
                assert re.fullmatch(MEASURE, value, re.ASCII), "Invalid physical measure spelling"
                result["axes"][name] = value
            else:
                result["guards"]["ind." + name] = value
    for name, node in selected.items():
        if name in {"bidi", "mirrorIndents"}:
            value = opc.val(node)
            assert value in (None, "0", "1", "false", "true", "off", "on"), "Bad native paragraph guard"
            result["guards"][name] = value not in ("0", "false", "off")
        elif name in {"textDirection", "framePr", "numPr", "spacing", "tabs", "contextualSpacing", "snapToGrid"}:
            result["guards"][name] = tree(node)
    return result


def style_context(package):
    part = package.related("styles")
    return context_from_styles(None if part is None else package.xml(part), part)


def context_from_styles(root, part):
    if root is None:
        return {"part": None, "defaults": declaration(None), "styles": {}, "default": None}
    assert root.tag == w("styles"), "Current related styles root namespace"
    defaults_nodes = root.findall("w:docDefaults/w:pPrDefault/w:pPr", NS)
    assert len(defaults_nodes) <= 1, "Duplicate paragraph docDefaults"
    if defaults_nodes:
        assert defaults_nodes[0].find("w:pStyle", NS) is None, "pStyle is not legal in docDefaults pPr"
    styles, default = {}, None
    for node in root.findall("w:style", NS):
        identity = node.get(w("styleId"))
        assert identity and identity not in styles, "Duplicate/empty native style ID"
        pr = node.findall("w:pPr", NS)
        assert len(pr) <= 1
        if pr:
            assert pr[0].find("w:pStyle", NS) is None, "pStyle is not legal in style pPr"
        based = node.findall("w:basedOn", NS)
        assert len(based) <= 1
        parent = opc.val(based[0]) if based else None
        if based:
            assert parent, "Required basedOn value"
        styles[identity] = {"id": identity, "type": node.get(w("type")),
                            "basedOn": parent, "declaration": declaration(pr[0] if pr else None)}
        if node.get(w("default")) in ("1", "true", "on") and node.get(w("type")) == "paragraph":
            assert default is None, "Duplicate default paragraph style"
            default = identity
    return {"part": part, "defaults": declaration(defaults_nodes[0] if defaults_nodes else None),
            "styles": styles, "default": default}


def effective(context, direct):
    selected = direct.get("pStyle") if direct["pStylePresent"] else context["default"]
    chain, seen, identity = [], set(), selected
    while identity is not None:
        assert identity not in seen and len(chain) < 128, "Native style cycle/budget"
        seen.add(identity)
        style = context["styles"].get(identity)
        assert style is not None and style["type"] == "paragraph", "Missing or wrong-type selected paragraph style"
        chain.append(style)
        identity = style["basedOn"]
    layers = [("docDefaults", context["defaults"])] + [("style:" + row["id"], row["declaration"]) for row in reversed(chain)] + [("direct", direct)]
    axes, provenance, guards = {}, {}, {}
    for origin, layer in layers:
        for name, value in layer["axes"].items():
            axes[name], provenance[name] = value, origin
        guards.update(layer["guards"])
    unsupported = []
    if "jc" in axes and axes["jc"] not in SUPPORTED_JC:
        unsupported.append("jc")
    for name in IND_AXES:
        if name in axes and (not re.fullmatch(r"[+-]?[0-9]+", axes[name], re.ASCII) or not 0 <= int(axes[name]) <= 31680):
            unsupported.append("ind." + name)
    unsupported.extend(name for name, value in guards.items() if value not in (False, None))
    return {"axes": axes, "provenance": provenance, "selectedStyle": selected,
            "styleChain": [row["id"] for row in reversed(chain)], "guards": guards,
            "unsupported": unsupported}


def inspect_package(package):
    root = package.xml(package.document)
    body = root.find("w:body", NS)
    assert body is not None
    context = style_context(package)
    rows = []
    for body_index, paragraph in enumerate(list(body)):
        groups = paragraph.findall("m:oMathPara", NS) if paragraph.tag == w("p") else []
        if not groups:
            continue
        assert len(groups) == 1, "One display carrier per owned host paragraph"
        props = paragraph.findall("w:pPr", NS)
        assert len(props) <= 1
        if props:
            assert list(paragraph)[0] is props[0], "pPr is not first in its real Word paragraph"
        direct = declaration(props[0] if props else None)
        group = groups[0]
        assert not any(node.tag in {w("pPr"), w("styles")} for node in group.iter()), "Internal context leaked into native OMML"
        math = group.findall("m:oMath", NS)
        assert math, "Real native display math missing"
        rows.append({"ordinal": len(rows), "bodyIndex": body_index,
                     "direct": direct, "effective": effective(context, direct),
                     "mathObjects": [tree(node) for node in math],
                     "runDeclarations": [{"fonts": dict(run.find("w:rPr/w:rFonts", NS).attrib) if run.find("w:rPr/w:rFonts", NS) is not None else {},
                                           "sizeHalfPoints": opc.val(run.find("w:rPr/w:sz", NS)),
                                           "complexSizeHalfPoints": opc.val(run.find("w:rPr/w:szCs", NS))}
                                          for obj in math for run in obj.iter(m("r"))],
                     "mathTexts": ["".join(leaf.text or "" for leaf in node.iter(m("t"))) for node in math]})
    settings = package.related("settings")
    math_defaults = [] if settings is None else package.xml(settings).findall("m:mathPr", NS)
    assert len(math_defaults) <= 1, "Duplicate global mathPr"
    auxiliary = {name: opc.digest(raw) for name, raw in package.parts.items()
                 if name.startswith("customXml/") or "readmd" in name.lower()}
    return {"kind": "math-word-paragraph", "documentPart": package.document,
            "stylesPart": context["part"], "settingsPart": settings,
            "globalMathPrCount": len(math_defaults),
            "mathPr": tree(math_defaults[0]) if math_defaults else None,
            "paragraphs": rows, "auxiliaryParts": auxiliary,
            "parts": {name: opc.digest(raw) for name, raw in package.parts.items()},
            "sourcePayloadUsedAsNativeEvidence": False,
            "fullWordSchemaCertified": False, "wordApplicationCertified": False}


def fixture_parts(kind):
    assert kind in KINDS
    wp, mp = ("v", "q") if kind == "styles-chain" else ("w", "m")
    def pr(inside):
        return f"<{wp}:pPr>{inside}</{wp}:pPr>"
    if kind == "direct-no-styles":
        props = [pr(f'<{wp}:ind {wp}:left="720" {wp}:right="180"/><{wp}:jc {wp}:val="right"/>')] * 3
        styles = None
    elif kind == "styles-chain":
        props = [pr(f'<{wp}:pStyle {wp}:val="Equation"/><{wp}:ind {wp}:left="0"/>')] * 3
        styles = f'''<{wp}:styles xmlns:{wp}="{W}"><{wp}:docDefaults><{wp}:pPrDefault><{wp}:pPr><{wp}:ind {wp}:left="240" {wp}:right="120"/><{wp}:jc {wp}:val="left"/></{wp}:pPr></{wp}:pPrDefault></{wp}:docDefaults>
<{wp}:style {wp}:type="paragraph" {wp}:default="1" {wp}:styleId="Unrelated"><{wp}:pPr><{wp}:ind {wp}:left="960" {wp}:right="960"/><{wp}:jc {wp}:val="center"/></{wp}:pPr></{wp}:style>
<{wp}:style {wp}:type="paragraph" {wp}:styleId="Base"><{wp}:pPr><{wp}:ind {wp}:left="480"/></{wp}:pPr></{wp}:style>
<{wp}:style {wp}:type="paragraph" {wp}:styleId="Equation"><{wp}:basedOn {wp}:val="Base"/><{wp}:pPr><{wp}:ind {wp}:right="360"/><{wp}:jc {wp}:val="center"/></{wp}:pPr></{wp}:style></{wp}:styles>'''
    else:
        props = [f"<{wp}:pPr/>", pr(f'<{wp}:ind {wp}:left="0" {wp}:right="0"/><{wp}:jc {wp}:val="left"/>'), pr(f"<{wp}:ind/>")]
        styles = f'<{wp}:styles xmlns:{wp}="{W}"/>'
    run_props = f'<{wp}:rPr><{wp}:rFonts {wp}:ascii="STIX Two Math" {wp}:hAnsi="STIX Two Math" {wp}:eastAsia="STIX Two Math" {wp}:cs="STIX Two Math"/><{wp}:sz {wp}:val="24"/><{wp}:szCs {wp}:val="24"/></{wp}:rPr>'
    formula = f'<{mp}:oMathPara><{mp}:oMath><{mp}:f><{mp}:num><{mp}:r>{run_props}<{mp}:t>a</{mp}:t></{mp}:r></{mp}:num><{mp}:den><{mp}:r>{run_props}<{mp}:t>b</{mp}:t></{mp}:r></{mp}:den></{mp}:f></{mp}:oMath></{mp}:oMathPara>'
    body = [f'<{wp}:p><{wp}:r><{wp}:t>M07_PREFIX_𝛂 KEEP_NEIGHBOR</{wp}:t></{wp}:r></{wp}:p>']
    for index, properties in enumerate(props):
        body.extend([f'<{wp}:p><{wp}:r><{wp}:t>M07_BEFORE_{index + 1}</{wp}:t></{wp}:r></{wp}:p>', f'<{wp}:p>{properties}{formula}</{wp}:p>'])
    body.append(f'<{wp}:p><{wp}:r><{wp}:t>M07_SUFFIX_KEEP</{wp}:t></{wp}:r></{wp}:p>')
    document = f'<{wp}:document xmlns:{wp}="{W}" xmlns:{mp}="{M}"><{wp}:body>{"".join(body)}<{wp}:sectPr><{wp}:pgSz {wp}:w="12240" {wp}:h="15840"/><{wp}:pgMar {wp}:top="1440" {wp}:right="1440" {wp}:bottom="1440" {wp}:left="1440"/></{wp}:sectPr></{wp}:body></{wp}:document>'
    settings = f'<{wp}:settings xmlns:{wp}="{W}" xmlns:{mp}="{M}"><{mp}:mathPr><{mp}:dispDef {mp}:val="0"/><{mp}:preSp {mp}:val="120"/><{mp}:postSp {mp}:val="240"/><{mp}:interSp {mp}:val="40"/><{mp}:intraSp {mp}:val="60"/></{mp}:mathPr></{wp}:settings>'
    types = f'''<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>'''
    rels = [("settings", "settings", "settings.xml")]
    parts = {"word/document.xml": document.encode(), "word/settings.xml": settings.encode(),
             "_rels/.rels": opc.relationship_xml([("root", "officeDocument", "word/document.xml")])}
    if styles is not None:
        parts["word/style-parts/current.xml"] = styles.encode()
        rels.append(("actualStyles", "styles", "style-parts/current.xml"))
        types += '<Override PartName="/word/style-parts/current.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
    parts["[Content_Types].xml"] = (types + "</Types>").encode()
    parts["word/_rels/document.xml.rels"] = opc.relationship_xml(rels)
    return parts


def mutate(parts, axis, value, ordinal=1):
    assert axis in ("left", "right", "jc") and ordinal == 1
    before = copy.deepcopy(parts)
    package = opc.Package(parts=parts)
    inspection = inspect_package(package)
    root = package.xml(package.document)
    body = root.find("w:body", NS)
    paragraph = list(body)[inspection["paragraphs"][ordinal]["bodyIndex"]]
    pr = paragraph.find("w:pPr", NS)
    if pr is None:
        pr = ET.Element(w("pPr")); paragraph.insert(0, pr)
    name = "jc" if axis == "jc" else "ind"
    node = pr.find("w:" + name, NS)
    if value is None:
        assert node is not None
        if axis == "jc":
            pr.remove(node)
        else:
            assert w(axis) in node.attrib
            del node.attrib[w(axis)]
    else:
        if node is None:
            node = ET.Element(w(name))
            if name == "ind" and pr.find("w:jc", NS) is not None:
                pr.insert(list(pr).index(pr.find("w:jc", NS)), node)
            else:
                pr.append(node)
        node.set(w("val" if axis == "jc" else axis), value)
    parts = dict(parts)
    parts[package.document] = opc.xml_bytes(root)
    changed = [name for name in parts if parts[name] != before[name]]
    assert changed == [package.document]
    after = inspect_package(opc.Package(parts=parts))
    for index in (0, 2):
        assert after["paragraphs"][index] == inspection["paragraphs"][index]
    for index in range(3):
        assert after["paragraphs"][index]["mathObjects"] == inspection["paragraphs"][index]["mathObjects"]
    assert after["auxiliaryParts"] == inspection["auxiliaryParts"]
    return parts, {"changedParts": changed, "onlySecondParagraphChanged": True,
                   "formulaObjectsUnchanged": True, "auxiliaryUnchanged": True}


def inspect_mathml(raw):
    root = opc.parse_xml(raw.encode() if isinstance(raw, str) else raw)
    namespace = "http://www.w3.org/1998/Math/MathML"
    assert root.tag == "{" + namespace + "}math", "Actual MathML namespace required"
    assert all(node.tag.startswith("{" + namespace + "}") for node in root.iter()), "Foreign MathML node"
    assert not any(opc.local(node.tag) in {"annotation", "annotation-xml", "mglyph", "merror"} for node in root.iter()), "A source/image/error surrogate is not current math"
    fractions = list(root.iter("{" + namespace + "}mfrac"))
    assert len(fractions) == 1, "Owned simple fraction structure changed"
    assert len(fractions[0]) == 2, "Actual fraction must have numerator and denominator"
    tokens = ["".join(node.itertext()) for node in root.iter() if opc.local(node.tag) in {"mi", "mn", "mo", "mtext"}]
    assert tokens == ["a", "b"], "Current fraction operands changed/reordered"
    css = {}
    for item in root.get("style", "").split(";"):
        if item.strip():
            name, value = item.split(":", 1)
            assert name.strip() not in css, "Duplicate root MathML CSS axis"
            css[name.strip()] = value.strip()
    return {"kind": "mathml", "tree": tree(root), "tokens": tokens,
            "fractions": len(fractions), "display": root.get("display"), "css": css,
            "independentXmlReparsed": True, "browserGeometryCertified": False}


def inspect_fragment(raw):
    text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    root = opc.parse_xml((f'<qa xmlns:w="{W}" xmlns:m="{M}">' + text + '</qa>').encode())
    styles = root.findall("w:styles", NS); props = root.findall("w:pPr", NS)
    assert len(styles) <= 1 and len(props) <= 1, "Duplicate internal Word context"
    context = context_from_styles(styles[0] if styles else None, "source-context" if styles else None)
    direct = declaration(props[0] if props else None)
    groups = root.findall("m:oMathPara", NS)
    assert len(groups) == 1
    objects = groups[0].findall("m:oMath", NS)
    assert len(objects) == 1
    defaults = root.findall("m:mathPr", NS)
    assert len(defaults) == 1
    return {"kind": "math-word-fragment", "direct": direct, "effective": effective(context, direct),
            "mathPr": tree(defaults[0]), "mathObjects": [tree(node) for node in objects],
            "intrinsicOmml": ET.tostring(defaults[0], encoding="unicode") + ET.tostring(groups[0], encoding="unicode"),
            "intrinsicDerivation": "current XML mathPr/oMathPara copied structurally; only sibling Word paragraph/styles context excluded",
            "independentXmlReparsed": True, "sourcePayloadUsedAsNativeEvidence": False}


def svg_ink(raw):
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.svgLib.path import parse_path
    import fontTools
    root = opc.parse_xml(raw)
    assert root.tag == "{http://www.w3.org/2000/svg}svg"
    width = re.fullmatch(r"([+0-9.e-]+)em", root.get("width", ""))
    height = re.fullmatch(r"([+0-9.e-]+)em", root.get("height", ""))
    box = [float(value) for value in root.get("viewBox", "").split()]
    assert width and height and len(box) == 4 and all(math.isfinite(value) for value in box)
    width_em, height_em = float(width[1]), float(height[1])
    assert width_em > 0 and height_em > 0
    units_per_em = box[2] / width_em
    # Frozen serializer rounds each dimension to .001. This is conversion
    # uncertainty from those actual lexical values, not a wider ink tolerance.
    uncertainty = .0005 * units_per_em + .0005 + height_em * units_per_em * .0005 / width_em
    assert abs(box[3] - height_em * units_per_em) <= uncertainty + 1e-8, "Inconsistent SVG em axes"
    point_scale = 12 / units_per_em
    ink = [node for node in root.iter() if node.get("data-math-ink") == "true"]
    assert len(ink) == 1 and ink[0].get("transform") == "scale(1 -1)"
    glyphs, bounds = [], []
    for node in ink[0].iter():
        kind = opc.local(node.tag)
        if kind == "g":
            assert node is ink[0] or node.get("transform") is None, "Unexpected nested ink transform"
        elif kind == "path":
            match = re.fullmatch(r"translate\(([-+0-9.e]+) ([-+0-9.e]+)\) scale\(([-+0-9.e]+)(?: ([-+0-9.e]+))?\)(?: matrix\(1 0 ([-+0-9.e]+) 1 0 0\))?", node.get("transform", ""))
            assert match, "Expected current glyph outline transform"
            x, y, sx, sy, shear = (float(value) if value is not None else None for value in match.groups())
            sy = sx if sy is None else sy; shear = 0 if shear is None else shear
            pen = BoundsPen(None)
            parse_path(node.get("d"), TransformPen(pen, (sx, 0, sx * shear, sy, x, y)))
            assert pen.bounds is not None
            actual = [axis * point_scale for axis in pen.bounds]
            glyphs.append({"outlineSha256": opc.digest(node.get("d").encode()), "originPt": [x * point_scale, y * point_scale],
                           "inkBoundsPt": actual, "sideBearingXPt": actual[0] - x * point_scale})
            bounds.append(actual)
        elif kind == "rect":
            assert node.get("fill") != "transparent", "Selection rectangle leaked into real ink group"
            x, y, width, height = (float(node.get(name, "0")) * point_scale for name in ("x", "y", "width", "height"))
            bounds.append([x, y, x + width, y + height])
        else:
            raise AssertionError("Unexpected ink primitive " + kind)
    assert len(glyphs) == 2 and bounds
    assert abs(box[0] * point_scale + .12) <= .007, "Owned simple fraction has unexpected left overflow; do not infer its advance"
    return {"kind": "math-word-svg-ink", "unitsPerEm": units_per_em, "pointsPerSvgUnit": point_scale,
            "unitEmSizePt": 12, "unitSource": "this bounded lane's schema-bound font_size=12pt and explicit native run size=24 half-points",
            "layoutAdvancePt": (width_em - .02) * 12,
            "layoutAdvanceDerivation": "current viewport includes exactly .01em antialias padding on both sides; owned simple fraction has no overflow",
            "glyphs": glyphs, "inkBoundsPt": [min(row[0] for row in bounds), min(row[1] for row in bounds), max(row[2] for row in bounds), max(row[3] for row in bounds)],
            "reader": {"name": "fontTools BoundsPen/TransformPen/svgLib.parse_path", "version": fontTools.__version__},
            "actualBezierExtremaInspected": True, "sourcePayloadUsedAsEvidence": False}


def drawing_ink_bounds(items):
    from fontTools.pens.boundsPen import BoundsPen
    pen = BoundsPen(None)
    for item in items:
        points = item["coordinates"]
        if item["operation"] == "c":
            assert len(points) == 4
            pen.moveTo(tuple(points[0])); pen.curveTo(*(tuple(point) for point in points[1:])); pen.endPath()
        elif item["operation"] == "l":
            assert len(points) == 2
            pen.moveTo(tuple(points[0])); pen.lineTo(tuple(points[1])); pen.endPath()
        elif item["operation"] in {"re", "qu"}:
            assert len(points) == 1
            coords = points[0]
            if item["operation"] == "re":
                assert len(coords) == 4
                x0, y0, x1, y1 = coords; corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            else:
                assert len(coords) == 8
                corners = list(zip(coords[::2], coords[1::2])); corners = [corners[index] for index in [0, 1, 3, 2]]
            pen.moveTo(corners[0])
            for point in corners[1:]:
                pen.lineTo(point)
            pen.closePath()
        else:
            raise AssertionError("Unknown drawing primitive")
    assert pen.bounds is not None
    return list(pen.bounds)


def painted_drawing_bounds(drawing):
    parts = []
    if drawing.get("fill") is not None:
        assert drawing.get("fillOpacity", 1) == 1, "Transparent/composited ink is outside this bounded physical oracle"
        parts.append(drawing_ink_bounds(drawing["items"]))
    if drawing.get("color") is not None:
        assert drawing.get("strokeOpacity", 1) == 1
        items = drawing["items"]
        assert len(items) == 1 and items[0]["operation"] == "l", "This finite oracle supports filled curves and exact single-line strokes; complex stroked curves need a new exact offset oracle"
        a, b = items[0]["coordinates"]
        width = drawing.get("width")
        assert isinstance(width, (int, float)) and math.isfinite(width) and width > 0, "A zero hairline has no fixed physical ink width"
        half = width / 2; dx, dy = b[0] - a[0], b[1] - a[1]; length = math.hypot(dx, dy)
        assert length > 0
        caps = drawing.get("lineCap")
        assert caps and len(set(caps)) == 1 and caps[0] in {0, 1, 2}, "Unreviewed PDF line caps"
        if caps[0] == 0:
            xpad, ypad = half * abs(dy) / length, half * abs(dx) / length
        elif caps[0] == 1:
            xpad = ypad = half
        else:
            xpad, ypad = half * (abs(dy) + abs(dx)) / length, half * (abs(dx) + abs(dy)) / length
        parts.append([min(a[0], b[0]) - xpad, min(a[1], b[1]) - ypad,
                      max(a[0], b[0]) + xpad, max(a[1], b[1]) + ypad])
    assert parts, "Drawing has no actual fill or stroke"
    return [min(row[0] for row in parts), min(row[1] for row in parts), max(row[2] for row in parts), max(row[3] for row in parts)]


def inspect_flow(raw):
    """Inspect selected output markup, not embedded original author source."""
    text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    lines = [0]
    for line in text.splitlines(keepends=True):
        lines.append(lines[-1] + len(line))
    class Ancestors(HTMLParser):
        def __init__(self):
            super().__init__(convert_charrefs=False)
            self.stack, self.roots = [], {}
        def handle_starttag(self, tag, attrs):
            values = dict(attrs); assert len(values) == len(attrs), "Duplicate current HTML attribute"
            if tag in {"math", "svg"}:
                line, column = self.getpos()
                self.roots[lines[line - 1] + column] = copy.deepcopy(self.stack)
            if tag not in {"br", "hr", "img", "input", "meta", "link", "source", "wbr"}:
                self.stack.append({"tag": tag, "attributes": values})
        def handle_startendtag(self, tag, attrs):
            self.handle_starttag(tag, attrs); self.handle_endtag(tag)
        def handle_endtag(self, tag):
            for index in range(len(self.stack) - 1, -1, -1):
                if self.stack[index]["tag"] == tag:
                    del self.stack[index:]; return
    parser = Ancestors(); parser.feed(text); parser.close()
    objects = []
    for match in re.finditer(r"<(math|svg)\b[^>]*>.*?</\1\s*>", text, re.S):
        kind = match.group(1)
        ancestors = parser.roots.get(match.start())
        assert ancestors is not None, "Output math object is not an actual HTML element"
        wrappers = [node for node in ancestors if "readmd-export-math-block" in node["attributes"].get("class", "").split()]
        wrapper = wrappers[-1] if wrappers else None
        if kind == "math":
            objects.append({**inspect_mathml(match.group(0)), "wrapper": wrapper})
        else:
            root = opc.parse_xml(match.group(0).encode())
            assert root.tag == "{http://www.w3.org/2000/svg}svg"
            assert not any(opc.local(node.tag) in {"image", "foreignObject", "text"} for node in root.iter()), "Actual vector glyph paths required"
            paths = [node for node in root.iter() if opc.local(node.tag) == "path"]
            assert len(paths) >= 2 and all(node.get("d") for node in paths), "Missing real numerator/denominator outlines"
            objects.append({"kind": "svg", "tree": tree(root), "glyphPaths": len(paths),
                            "width": root.get("width"), "height": root.get("height"),
                            "viewBox": root.get("viewBox"), "ink": svg_ink(match.group(0).encode()), "wrapper": wrapper, "independentXmlReparsed": True,
                            "browserGeometryCertified": False})
    assert len(objects) == 3, "Exactly three current output math objects required"
    return {"kind": "math-word-flow", "objects": objects,
            "sourcePayloadUsedAsEvidence": False, "browserGeometryCertified": False}


def parse_epub_xhtml(raw):
    """Accept only the inert HTML5 declaration emitted in EPUB XHTML.

    The shared OPC parser remains DTD-free. Do not authorize a general DTD,
    external identifiers, entity declarations or a declaration inside content.
    Only the inspection copy loses the exact prolog declaration; the EPUB
    member bytes and their recorded digest are never changed.
    """
    assert 0 < len(raw) <= opc.MAX_XML, "XML byte budget"
    assert b"\x00" not in raw, "Owned EPUB inspection requires UTF-8 XHTML"
    try:
        raw.decode("utf-8-sig")
    except UnicodeDecodeError as error:
        raise AssertionError("Owned EPUB inspection requires UTF-8 XHTML") from error
    upper = raw.upper()
    assert b"<!ENTITY" not in upper, "DTD/entity forbidden"
    if b"<!DOCTYPE" in upper:
        match = re.match(br'\A(?:\xef\xbb\xbf)?(?:<\?xml[^?]*\?>)?[ \t\r\n]*(<!DOCTYPE html>)(?=[ \t\r\n]*<html(?:[ \t\r\n>]))', raw)
        assert match is not None and upper.count(b"<!DOCTYPE") == 1, "Only a single prolog HTML5 doctype is permitted"
        start, end = match.span(1)
        raw = raw[:start] + raw[end:]
    root = opc.parse_xml(raw)
    assert root.tag == "{http://www.w3.org/1999/xhtml}html", "EPUB XHTML root namespace"
    return root


def inspect_epub(file):
    import zipfile
    import posixpath
    with zipfile.ZipFile(file) as package:
        names = package.namelist()
        assert len(set(names)) == len(names), "Duplicate EPUB members"
        container = opc.parse_xml(package.read("META-INF/container.xml"))
        roots = [node.get("full-path") for node in container.iter() if opc.local(node.tag) == "rootfile"]
        assert len(roots) == 1 and roots[0] in names
        opf = opc.parse_xml(package.read(roots[0])); base = posixpath.dirname(roots[0])
        manifest = {node.get("id"): node for node in opf.iter() if opc.local(node.tag) == "item"}
        rows = []
        for itemref in (node for node in opf.iter() if opc.local(node.tag) == "itemref"):
            item = manifest[itemref.get("idref")]
            assert item.get("media-type") == "application/xhtml+xml"
            part = posixpath.normpath(posixpath.join(base, item.get("href")))
            assert not part.startswith("../") and part in names
            root = parse_epub_xhtml(package.read(part))
            for node in root.iter("{http://www.w3.org/1998/Math/MathML}math"):
                rows.append({"part": part, **inspect_mathml(ET.tostring(node, encoding="utf-8"))})
        assert len(rows) == 3, "EPUB current spine lost/duplicated formulas"
        assert not any(name.lower().endswith((".png", ".jpg", ".jpeg")) for name in names), "Raster formula fallback"
        return {"kind": "math-word-epub", "math": rows, "memberSha256": {
            name: opc.digest(package.read(name)) for name in names},
            "independentXmlReparsed": True, "readerGeometryCertified": False,
            "sourcePayloadUsedAsEvidence": False}


def drawing_item(item):
    op, values = item[0], item[1:]
    assert op in {"l", "c", "re", "qu"}, "Unreviewed PDF drawing operation"
    coordinates = []
    scalar = []
    for value in values:
        if isinstance(value, (int, float)):
            scalar.append(value)
        else:
            coords = list(value)
            if coords and not isinstance(coords[0], (int, float)):
                coords = [axis for point in coords for axis in point]
            assert len(coords) in {2, 4, 8} and all(math.isfinite(axis) for axis in coords)
            coordinates.append(coords)
    return {"operation": op, "coordinates": coordinates, "scalar": scalar}


def inspect_pdf(file):
    import pymupdf as fitz
    with fitz.open(file) as document:
        assert not document.is_encrypted and 0 < len(document) <= 16
        markers = {}
        for name in ["M07_BEFORE_1", "M07_BEFORE_2", "M07_BEFORE_3", "M07_SUFFIX_KEEP"]:
            matches = [(index, list(rect)) for index, page in enumerate(document) for rect in page.search_for(name)]
            assert len(matches) == 1, "Unique physical formula marker required: " + name
            markers[name] = {"page": matches[0][0], "bounds": matches[0][1]}
        rows = []
        for ordinal in range(3):
            before = markers["M07_BEFORE_" + str(ordinal + 1)]
            after = markers["M07_BEFORE_" + str(ordinal + 2)] if ordinal < 2 else markers["M07_SUFFIX_KEEP"]
            assert before["page"] == after["page"], "Owned wide simple formula moved to another physical page"
            page = document[before["page"]]
            low, high = before["bounds"][3], after["bounds"][1]
            assert low < high
            drawings = []
            for drawing in page.get_drawings():
                rect = list(drawing["rect"])
                if rect[1] >= low and rect[3] <= high:
                    drawings.append({"bounds": rect, "items": [drawing_item(item) for item in drawing["items"]],
                                     "fill": drawing.get("fill"), "color": drawing.get("color"),
                                     "width": drawing.get("width"), "closePath": drawing.get("closePath"),
                                     "type": drawing.get("type"), "lineCap": drawing.get("lineCap"), "lineJoin": drawing.get("lineJoin"),
                                     "fillOpacity": drawing.get("fill_opacity"), "strokeOpacity": drawing.get("stroke_opacity")})
            assert len(drawings) >= 3, "Actual vector numerator, denominator and fraction bar required"
            assert not page.get_images(), "Owned math PDF must not use a raster surrogate"
            bounds = [min(row["bounds"][0] for row in drawings), min(row["bounds"][1] for row in drawings),
                      max(row["bounds"][2] for row in drawings), max(row["bounds"][3] for row in drawings)]
            ink_bounds = [painted_drawing_bounds(drawing) for drawing in drawings]
            rows.append({"ordinal": ordinal, "page": before["page"], "window": [low, high],
                         "bounds": bounds, "drawings": drawings,
                         "inkBounds": [min(row[0] for row in ink_bounds), min(row[1] for row in ink_bounds), max(row[2] for row in ink_bounds), max(row[3] for row in ink_bounds)]})
        pages = [{"mediaBox": list(page.mediabox), "cropBox": list(page.cropbox),
                  "rect": list(page.rect), "rotation": page.rotation,
                  "imageCount": len(page.get_images())} for page in document]
        return {"kind": "math-word-pdf", "reader": {"name": "PyMuPDF", "version": fitz.VersionBind},
                "coordinateSpace": "unrotated-page-physical-points-top-left", "pages": pages,
                "metadata": document.metadata,
                "inkBoundsDefinition": {"filledPaths": "exact quadratic/cubic Bézier extrema", "stroke": "exact single straight line with physical width/cap; complex strokes rejected", "rawDrawingRectangleIsNotInkAuthority": True},
                "markers": markers, "formulas": rows, "sourcePayloadUsedAsEvidence": False,
                "actualVectorPathsInspected": True, "wordApplicationCertified": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    fixture = sub.add_parser("fixture"); fixture.add_argument("--kind", choices=KINDS, required=True); fixture.add_argument("--output", type=Path, required=True)
    inspect = sub.add_parser("inspect"); inspect.add_argument("input", type=Path)
    for action in ("fragment", "mathml", "flow", "epub", "pdf", "svg-ink"):
        command = sub.add_parser(action); command.add_argument("input", type=Path)
    mutation = sub.add_parser("mutate"); mutation.add_argument("--input", type=Path, required=True); mutation.add_argument("--output", type=Path, required=True); mutation.add_argument("--axis", choices=("left", "right", "jc"), required=True); mutation.add_argument("--value"); mutation.add_argument("--clear", action="store_true")
    args = parser.parse_args()
    if args.action == "fixture":
        parts = fixture_parts(args.kind); opc.write_package(args.output, parts)
        result = {"kind": args.kind, "nativeOnly": True, "sha256": opc.digest(args.output.read_bytes()), "inspection": inspect_package(opc.Package(args.output))}
    elif args.action == "mutate":
        assert args.clear != (args.value is not None), "One exact value or clear is required"
        package = opc.Package(args.input); parts, detail = mutate(package.parts, args.axis, None if args.clear else args.value)
        opc.write_package(args.output, parts)
        result = {**detail, "sha256": opc.digest(args.output.read_bytes()), "inspection": inspect_package(opc.Package(args.output))}
    elif args.action == "inspect":
        result = inspect_package(opc.Package(args.input)); result["sha256"] = opc.digest(args.input.read_bytes())
    elif args.action == "mathml":
        result = inspect_mathml(args.input.read_bytes())
    elif args.action == "fragment":
        result = inspect_fragment(args.input.read_bytes())
    elif args.action == "flow":
        result = inspect_flow(args.input.read_bytes())
    elif args.action == "epub":
        result = inspect_epub(args.input)
    elif args.action == "svg-ink":
        result = svg_ink(args.input.read_bytes())
    else:
        result = inspect_pdf(args.input)
    result.setdefault("sha256", opc.digest(args.output.read_bytes() if args.action in {"fixture", "mutate"} else args.input.read_bytes()))
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
