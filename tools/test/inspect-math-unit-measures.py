"""Independent M07 native unit-measure oracle; no product or engine imports.

Raw author lexemes and cascade provenance are separate from exact Fraction
points. Reuses unchanged bounded ZIP/XML and selected visible math inspectors.
"""
import argparse
import copy
from decimal import Decimal
from fractions import Fraction
import importlib.util
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET

_spec = importlib.util.spec_from_file_location("unit_logical_oracle", Path(__file__).with_name("inspect-math-logical-align.py"))
logical = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(logical)
base, opc = logical.base, logical.opc
W, M, NS, w, m = logical.W, logical.M, logical.NS, logical.w, logical.m
Package, parse_xml = logical.Package, logical.parse_xml
declaration, context_from_styles = logical.declaration, logical.context_from_styles
KINDS = ("left", "right")
RAW_ROWS = (("1mm", "1cm"), ("0.5in", "18.125pt"), ("2pc", "1.5pi"))
FACTORS = {"mm": Fraction(360, 127), "cm": Fraction(3600, 127), "in": Fraction(72),
           "pt": Fraction(1), "pc": Fraction(12), "pi": Fraction(12)}
INTEGER = re.compile(r"[+-]?[0-9]+", re.ASCII)
UNIT = re.compile(r"(-?[0-9]+(?:\.[0-9]+)?)(mm|cm|in|pt|pc|pi)", re.ASCII)


def ratio(value):
    return {"numerator": str(value.numerator), "denominator": str(value.denominator)}


def measure(raw):
    """Schema spelling first; no float, twip rounding or ReadMD numeric input."""
    assert isinstance(raw, str) and 0 < len(raw) <= 65536, "Measure lexical budget"
    if INTEGER.fullmatch(raw.strip(" \t\r\n")):
        normalized = raw.strip(" \t\r\n")
        # Keep adversarial controls bounded even on Python's integer digit cap.
        negative = normalized.startswith("-")
        digits = normalized.lstrip("+-").lstrip("0") or "0"
        assert len(digits) <= 256, "Independent numeric-control budget"
        points = Fraction((-1 if negative else 1) * int(digits), 20)
        unit = "twip"
    else:
        match = UNIT.fullmatch(raw)
        assert match is not None, "Invalid unit measure spelling"
        decimal, unit = match.groups()
        sign = "-" if decimal.startswith("-") else ""
        pieces = decimal.lstrip("-").split(".")
        whole = pieces[0].lstrip("0") or "0"
        fraction = pieces[1].rstrip("0") if len(pieces) == 2 else ""
        assert len(whole) + len(fraction) <= 256, "Independent numeric-control budget"
        normalized = sign + whole + ("." + fraction if fraction else "")
        points = Fraction(Decimal(normalized)) * FACTORS[unit]
    twips = points * 20
    return {"raw": raw, "unit": unit, "points": ratio(points), "twips": ratio(twips),
            "pointsF64": float(points), "twipsF64": float(twips),
            "inPhysicalDomain": Fraction(0) <= points <= Fraction(1584),
            "exactConsumerRepresentable": abs(points.numerator) <= 2**128 - 1 and points.denominator <= 2**128 - 1,
            "nonIntegralTwips": twips.denominator != 1}


def effective(context, direct):
    result = logical.effective(context, direct)
    result["measures"], result["physicalPoints"] = {}, {"jc": result["physicalAxes"].get("jc", "left")}
    unsupported = [key for key in result["unsupported"] if key not in {"ind.left", "ind.right"}]
    for axis in ("left", "right"):
        if axis in result["axes"]:
            current = measure(result["axes"][axis])
            result["measures"][axis] = current
            result["physicalPoints"][axis] = current["pointsF64"]
            if not current["inPhysicalDomain"] or not current["exactConsumerRepresentable"]:
                unsupported.append("ind." + axis)
        else:
            result["physicalPoints"][axis] = 0.0
    # The source-preserving physicalAxes field keeps raw strings, never a
    # rounded integer. Only physicalPoints/measures contain numeric projection.
    result["unsupported"] = unsupported
    if not unsupported and result["axes"].get("jc") in logical.LOGICAL:
        result["physicalAxes"]["jc"] = logical.LOGICAL[result["axes"]["jc"]]
        result["physicalPoints"]["jc"] = result["physicalAxes"]["jc"]
        result["logicalMappingApplied"] = True
    result["unitProjectionIndependent"] = True
    return result


def inspect_package(package):
    result = logical.inspect_package(package)
    root = package.xml(package.document)
    body = root.find("w:body", NS)
    part = package.related("styles")
    context = context_from_styles(None if part is None else package.xml(part), part)
    for row in result["paragraphs"]:
        paragraph = list(body)[row["bodyIndex"]]
        props = paragraph.findall("w:pPr", NS)
        assert len(props) == 1 and list(paragraph)[0] is props[0]
        row["direct"] = declaration(props[0])
        row["effective"] = effective(context, row["direct"])
    result["oracleScope"] = "native raw units/origins plus independent exact rational points"
    result["nativeStyleProvenanceRoundtripCertified"] = False
    return result


def inspect_fragment(raw):
    result = logical.inspect_fragment(raw)
    text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    root = parse_xml((f'<qa xmlns:w="{W}" xmlns:m="{M}">' + text + '</qa>').encode())
    styles, props = root.findall("w:styles", NS), root.findall("w:pPr", NS)
    assert len(styles) <= 1 and len(props) == 1
    context = context_from_styles(styles[0] if styles else None, "source-context" if styles else None)
    result["direct"] = declaration(props[0])
    result["effective"] = effective(context, result["direct"])
    return result


def fixture_parts(kind):
    assert kind in KINDS
    parts = base.fixture_parts("direct-no-styles")
    package = Package(parts=parts)
    document = package.xml(package.document)
    body = document.find("w:body", NS)
    paragraphs = [node for node in body if node.tag == w("p") and node.find("m:oMathPara", NS) is not None]
    assert len(paragraphs) == 3
    declarations = [f'<w:pPr xmlns:w="{W}"><w:pStyle w:val="Opposite"/><w:ind w:left="{RAW_ROWS[0][0]}" w:right="{RAW_ROWS[0][1]}"/><w:jc w:val="{kind}"/></w:pPr>',
                    f'<w:pPr xmlns:w="{W}"><w:pStyle w:val="Equation"/></w:pPr>',
                    f'<w:pPr xmlns:w="{W}"><w:pStyle w:val="DefaultsOnly"/></w:pPr>']
    for paragraph, properties in zip(paragraphs, declarations):
        paragraph.remove(paragraph.find("w:pPr", NS))
        paragraph.insert(0, parse_xml(properties.encode()))
    opposite = "right" if kind == "left" else "left"
    styles = f'''<w:styles xmlns:w="{W}"><w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="{RAW_ROWS[2][0]}" w:right="{RAW_ROWS[2][1]}"/><w:jc w:val="{kind}"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:styleId="Opposite"><w:pPr><w:ind w:left="3pi" w:right="4pc"/><w:jc w:val="{opposite}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:ind w:left="{RAW_ROWS[1][0]}" w:right="6pt"/><w:jc w:val="{opposite}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="Base"/><w:pPr><w:ind w:right="{RAW_ROWS[1][1]}"/><w:jc w:val="{kind}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="DefaultsOnly"><w:pPr/></w:style></w:styles>'''
    part = "word/style-parts/unit-current.xml"
    parts[package.document], parts[part] = opc.xml_bytes(document), styles.encode()
    rels = package.xml(opc.rel_part(package.document))
    ET.SubElement(rels, "{" + opc.P + "}Relationship", {"Id": "unitStyles", "Type": opc.R + "/styles", "Target": "style-parts/unit-current.xml"})
    parts[opc.rel_part(package.document)] = opc.xml_bytes(rels)
    types = package.xml("[Content_Types].xml")
    ET.SubElement(types, "{" + opc.CT + "}Override", {"PartName": "/" + part, "ContentType": "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"})
    parts["[Content_Types].xml"] = opc.xml_bytes(types)
    assert not any(name.startswith("customXml/") for name in parts)
    return parts


def assert_css_points(value, wanted):
    assert re.fullmatch(r"-?[0-9]+(?:\.[0-9]+)?pt", value, re.ASCII), "Expected physical point CSS"
    actual = Fraction(Decimal(value[:-2]))
    expected = Fraction(int(wanted["numerator"]), int(wanted["denominator"]))
    # Rust Display f64 shortest roundtrip spelling, compared to independent
    # rational expectation; far below a nonintegral twip rounding difference.
    assert abs(actual - expected) <= Fraction(1, 10**10), "Wrong physical unit CSS"
    return {"actual": ratio(actual), "expected": ratio(expected)}


def svg_ink(raw):
    result = base.svg_ink(raw)
    root = parse_xml(raw)
    box = [float(value) for value in root.get("viewBox").split()]
    # The bound current renderer explicitly emits 1000 drawing units per em.
    # Check both independently rounded em/viewBox dimensions against that fixed
    # contract. Do not infer a convenient scale from the observed glyph shift.
    for dimension, extent in zip(("width", "height"), box[2:]):
        match = re.fullmatch(r"([+0-9.e-]+)em", root.get(dimension, ""))
        assert match and abs(float(match[1]) * 1000 - extent) <= .501
    ink = [node for node in root.iter() if node.get("data-math-ink") == "true"]
    assert len(ink) == 1
    primitives = []
    for node in ink[0].iter():
        tag = opc.local(node.tag)
        if tag == "path":
            match = re.fullmatch(r"translate\(([-+0-9.e]+) ([-+0-9.e]+)\) scale\(([-+0-9.e]+)(?: ([-+0-9.e]+))?\)(?: matrix\(1 0 ([-+0-9.e]+) 1 0 0\))?", node.get("transform", ""))
            assert match
            x, y, sx, sy, shear = (float(value) if value is not None else None for value in match.groups())
            primitives.append({"kind": "path", "x": x, "y": y, "sx": sx, "sy": sx if sy is None else sy,
                               "shear": shear or 0, "attributes": {key: value for key, value in node.attrib.items() if key != "transform"}})
        elif tag == "rect":
            primitives.append({"kind": "rect", **{key: float(node.get(key, "0")) for key in ("x", "y", "width", "height")},
                               "attributes": {key: value for key, value in node.attrib.items() if key not in {"x", "y", "width", "height"}}})
        else:
            assert tag == "g", "Unreviewed visible ink primitive"
    assert len(primitives) >= 3
    result["normalizedPrimitives"] = primitives
    result["fixedUnitsPerEm"] = 1000
    result["fixedPointsPerUnit"] = 12 / 1000
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    fixture = sub.add_parser("fixture")
    fixture.add_argument("--kind", choices=KINDS, required=True)
    fixture.add_argument("--output", type=Path, required=True)
    units = sub.add_parser("measure")
    units.add_argument("raw")
    for action in ("inspect", "fragment", "mathml", "flow", "epub", "pdf", "svg-ink"):
        sub.add_parser(action).add_argument("input", type=Path)
    args = parser.parse_args()
    if args.action == "measure":
        result = measure(args.raw)
    elif args.action == "fixture":
        parts = fixture_parts(args.kind)
        opc.write_package(args.output, parts)
        result = {"kind": args.kind, "nativeOnly": True, "sha256": opc.digest(args.output.read_bytes()), "inspection": inspect_package(Package(args.output))}
    elif args.action == "inspect":
        result = inspect_package(Package(args.input))
    elif args.action == "fragment":
        result = inspect_fragment(args.input.read_bytes())
    else:
        if args.action in {"mathml", "svg-ink"}:
            parse_xml(args.input.read_bytes())
        function = {"mathml": logical.inspect_mathml, "flow": logical.inspect_flow,
                    "epub": logical.inspect_epub, "pdf": base.inspect_pdf, "svg-ink": svg_ink}[args.action]
        result = function(args.input if args.action in {"epub", "pdf"} else args.input.read_bytes())
    if args.action != "measure":
        result.setdefault("sha256", opc.digest((args.output if args.action == "fixture" else args.input).read_bytes()))
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
