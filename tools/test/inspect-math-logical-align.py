"""Independent finite M07 logical-alignment OPC/physical oracle.

No ReadMD imports, source-payload authority, engine, network or GUI. Reuse the
unchanged bounded OPC/XML, actual SVG extrema and PDF curve inspectors. Raw
Word values and their cascade origins stay separate from physical projection.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import sys
import urllib.parse
import xml.etree.ElementTree as ET
import zipfile
from html.parser import HTMLParser

_spec = importlib.util.spec_from_file_location("logical_word_oracle_base", Path(__file__).with_name("inspect-math-word-paragraph.py"))
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
opc, W, M, NS = base.opc, base.W, base.M, base.NS
w, m = base.w, base.m
LOGICAL = {"start": "left", "end": "right"}
PPR_ORDER = ("pStyle", "keepNext", "keepLines", "pageBreakBefore", "framePr", "widowControl", "numPr",
             "suppressLineNumbers", "pBdr", "shd", "tabs", "suppressAutoHyphens", "kinsoku", "wordWrap",
             "overflowPunct", "topLinePunct", "autoSpaceDE", "autoSpaceDN", "bidi", "adjustRightInd",
             "snapToGrid", "spacing", "ind", "contextualSpacing", "mirrorIndents", "suppressOverlap",
             "jc", "textDirection", "textAlignment", "textboxTightWrap", "outlineLvl", "divId", "cnfStyle",
             "rPr", "sectPr", "pPrChange")
BOOL_GUARDS = {"bidi", "mirrorIndents", "adjustRightInd", "snapToGrid"}
DIRECTIONS = {"lrTb", "tb", "tbRl", "rl", "btLr", "lr", "lrTbV", "tbV", "tbRlV", "rlV", "tbLrV", "lrV"}


def parse_xml(raw):
    assert b"\x00" not in raw, "Owned logical XML requires UTF-8"
    try:
        raw.decode("utf-8-sig")
    except UnicodeDecodeError as error:
        raise AssertionError("Owned logical XML requires UTF-8") from error
    return opc.parse_xml(raw)


class Package(opc.Package):
    def xml(self, part):
        if part not in self.cache:
            assert part in self.parts, "Missing OPC part: " + part
            self.cache[part] = parse_xml(self.parts[part])
        return self.cache[part]


def declaration(pr):
    result = base.declaration(pr)
    if pr is None:
        return result
    assert not pr.attrib and not (pr.text or "").strip(), "Invalid pPr container"
    names = [opc.local(node.tag) for node in pr]
    assert all(name in PPR_ORDER for name in names), "Unknown paragraph particle"
    order = [PPR_ORDER.index(name) for name in names]
    assert order == sorted(set(order)), "Native pPr sequence/duplicate particle"
    for node in pr:
        name = opc.local(node.tag)
        if name in BOOL_GUARDS:
            assert set(node.attrib) <= {w("val")} and not len(node)
            raw = opc.val(node)
            assert raw in (None, "0", "1", "false", "true", "off", "on"), "Invalid paragraph boolean"
            result["guards"][name] = raw not in ("0", "false", "off")
        elif name == "textDirection":
            assert set(node.attrib) == {w("val")} and not len(node)
            assert opc.val(node) in DIRECTIONS, "Invalid text direction"
            result["guards"][name] = base.tree(node)
        elif name not in {"pStyle", "ind", "jc", "rPr"}:
            result["guards"][name] = base.tree(node)
    return result


def context_from_styles(root, part):
    context = base.context_from_styles(root, part)
    if root is not None:
        defaults = root.find("w:docDefaults/w:pPrDefault/w:pPr", NS)
        context["defaults"] = declaration(defaults)
        for node in root.findall("w:style", NS):
            context["styles"][node.get(w("styleId"))]["declaration"] = declaration(node.find("w:pPr", NS))
    return context


def effective(context, direct):
    selected = direct.get("pStyle") if direct["pStylePresent"] else context["default"]
    chain, seen, identity = [], set(), selected
    while identity is not None:
        assert identity not in seen and len(chain) < 128, "Native style cycle/budget"
        seen.add(identity)
        style = context["styles"].get(identity)
        assert style is not None and style["type"] == "paragraph", "Missing/wrong selected style"
        chain.append(style)
        identity = style["basedOn"]
    layers = [("docDefaults", context["defaults"])] + [("style:" + row["id"], row["declaration"]) for row in reversed(chain)] + [("direct", direct)]
    axes, provenance, guards = {}, {}, {}
    for origin, layer in layers:
        for key, value in layer["axes"].items():
            axes[key], provenance[key] = value, origin
        guards.update(layer["guards"])
    unsupported = []
    logical = axes.get("jc") in LOGICAL
    if axes.get("jc") is not None and axes["jc"] not in {"left", "center", "right", *LOGICAL}:
        unsupported.append("jc")
    for key in base.IND_AXES:
        if key in axes and (not re.fullmatch(r"[+-]?[0-9]+", axes[key], re.ASCII) or not 0 <= int(axes[key]) <= 31680):
            unsupported.append("ind." + key)
    for key, value in guards.items():
        if key == "textDirection":
            raw = value["attributes"].get(w("val"))
            if raw not in ({"lrTb"} if logical else {"lrTb", "tb"}):
                unsupported.append(key)
        elif value not in (False, None):
            unsupported.append(key)
    physical = dict(axes)
    if not unsupported and logical:
        physical["jc"] = LOGICAL[axes["jc"]]
    return {"axes": axes, "physicalAxes": physical, "provenance": provenance,
            "selectedStyle": selected, "styleChain": [row["id"] for row in reversed(chain)],
            "guards": guards, "unsupported": unsupported,
            "logicalMappingApplied": logical and not unsupported}


def inspect_package(package):
    # Base reads actual native math/run data and rejects duplicate pPr/context.
    result = base.inspect_package(package)
    root = package.xml(package.document)
    body = root.find("w:body", NS)
    part = package.related("styles")
    context = context_from_styles(None if part is None else package.xml(part), part)
    for row in result["paragraphs"]:
        paragraph = list(body)[row["bodyIndex"]]
        props = paragraph.findall("w:pPr", NS)
        assert len(props) == 1 and list(paragraph)[0] is props[0], "Exactly one first native math pPr required"
        row["direct"] = declaration(props[0])
        row["effective"] = effective(context, row["direct"])
    result["oracleScope"] = "raw native Word axes/origins plus separate finite physical projection"
    result["nativeStyleProvenanceRoundtripCertified"] = False
    return result


def inspect_fragment(raw):
    text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    root = parse_xml((f'<qa xmlns:w="{W}" xmlns:m="{M}">' + text + '</qa>').encode())
    styles, props = root.findall("w:styles", NS), root.findall("w:pPr", NS)
    assert len(styles) <= 1 and len(props) == 1, "Unique current Word paragraph context required"
    context = context_from_styles(styles[0] if styles else None, "source-context" if styles else None)
    result = base.inspect_fragment(raw)
    result["direct"] = declaration(props[0])
    result["effective"] = effective(context, result["direct"])
    return result


def fixture_parts(kind):
    assert kind in LOGICAL
    opposite = "end" if kind == "start" else "start"
    parts = base.fixture_parts("direct-no-styles")
    package = Package(parts=parts)
    document = package.xml(package.document)
    body = document.find("w:body", NS)
    paragraphs = [node for node in body if node.tag == w("p") and node.find("m:oMathPara", NS) is not None]
    assert len(paragraphs) == 3
    declarations = [f'<w:pPr xmlns:w="{W}"><w:pStyle w:val="Opposite"/><w:ind w:left="720" w:right="360"/><w:jc w:val="{kind}"/></w:pPr>',
                    f'<w:pPr xmlns:w="{W}"><w:pStyle w:val="Equation"/></w:pPr>',
                    f'<w:pPr xmlns:w="{W}"><w:pStyle w:val="DefaultsOnly"/></w:pPr>']
    for paragraph, properties in zip(paragraphs, declarations):
        paragraph.remove(paragraph.find("w:pPr", NS))
        paragraph.insert(0, parse_xml(properties.encode()))
    styles = f'''<w:styles xmlns:w="{W}"><w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="720" w:right="360"/><w:jc w:val="{kind}"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:styleId="Opposite"><w:pPr><w:ind w:left="240" w:right="180"/><w:jc w:val="{opposite}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:ind w:left="720" w:right="180"/><w:jc w:val="{opposite}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="Base"/><w:pPr><w:ind w:right="360"/><w:jc w:val="{kind}"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="DefaultsOnly"><w:pPr/></w:style></w:styles>'''
    part = "word/style-parts/logical-current.xml"
    parts[package.document] = opc.xml_bytes(document)
    parts[part] = styles.encode()
    rels = package.xml(opc.rel_part(package.document))
    ET.SubElement(rels, "{" + opc.P + "}Relationship", {"Id": "logicalStyles", "Type": opc.R + "/styles", "Target": "style-parts/logical-current.xml"})
    parts[opc.rel_part(package.document)] = opc.xml_bytes(rels)
    types = package.xml("[Content_Types].xml")
    ET.SubElement(types, "{" + opc.CT + "}Override", {"PartName": "/" + part, "ContentType": "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"})
    parts["[Content_Types].xml"] = opc.xml_bytes(types)
    assert not any(name.startswith("customXml/") for name in parts)
    return parts


def read_archive(file):
    assert 0 < file.stat().st_size <= opc.MAX_BYTES, "ZIP byte budget"
    with zipfile.ZipFile(file) as archive:
        info = archive.infolist()
        names = [item.filename for item in info]
        assert len(info) <= 4096 and len(names) == len(set(names)), "Duplicate/excess EPUB members"
        assert sum(item.file_size for item in info) <= opc.MAX_BYTES, "Expanded ZIP byte budget"
        for item in info:
            name = item.filename
            assert name and not name.startswith("/") and not re.match(r"[A-Za-z]:", name) and "\\" not in name and ".." not in PurePosixPath(name).parts and "\x00" not in name, "Unsafe EPUB member"
            assert not item.flag_bits & 1, "Encrypted EPUB member"
        assert archive.testzip() is None, "Invalid EPUB CRC"
        return {item.filename: archive.read(item) for item in info}


def inspect_epub(file):
    parts = read_archive(file)
    assert parts.get("mimetype") == b"application/epub+zip"
    container_ns, opf_ns = "urn:oasis:names:tc:opendocument:xmlns:container", "http://www.idpf.org/2007/opf"
    container = parse_xml(parts["META-INF/container.xml"])
    assert container.tag == "{" + container_ns + "}container"
    roots = container.findall("{" + container_ns + "}rootfiles/{" + container_ns + "}rootfile")
    assert len(roots) == 1 and roots[0].get("media-type") == "application/oebps-package+xml"
    opf_part = opc.member("", roots[0].get("full-path", ""))
    assert opf_part in parts
    opf = parse_xml(parts[opf_part])
    assert opf.tag == "{" + opf_ns + "}package"
    manifest, spine = opf.find("{" + opf_ns + "}manifest"), opf.find("{" + opf_ns + "}spine")
    assert manifest is not None and spine is not None
    items = {}
    for item in manifest:
        assert item.tag == "{" + opf_ns + "}item"
        identity = item.get("id")
        assert identity and identity not in items
        href = item.get("href", "")
        url = urllib.parse.urlsplit(href)
        assert not url.scheme and not url.netloc and "#" not in href and "?" not in href
        part = opc.member(opf_part, href)
        assert part in parts
        items[identity] = (item, part)
    rows, spine_parts = [], []
    for itemref in spine:
        assert itemref.tag == "{" + opf_ns + "}itemref" and itemref.get("idref") in items
        item, part = items[itemref.get("idref")]
        assert item.get("media-type") == "application/xhtml+xml" and part not in spine_parts
        spine_parts.append(part)
        root = base.parse_epub_xhtml(parts[part])
        assert_visible_math_roots(ET.tostring(root, encoding="unicode"), xml_root=root)
        assert not any(opc.local(node.tag) in {"svg", "img", "object"} for node in root.iter()), "Unreviewed EPUB visual surrogate"
        for node in root.iter("{http://www.w3.org/1998/Math/MathML}math"):
            rows.append({"part": part, **inspect_mathml(ET.tostring(node, encoding="utf-8"))})
    assert len(rows) == 3, "All selected spine content must contain exactly three real formulas"
    assert not any(name.lower().endswith((".png", ".jpg", ".jpeg")) for name in parts), "Raster formula fallback"
    return {"kind": "math-logical-epub", "math": rows, "spineParts": spine_parts,
            "memberSha256": {name: opc.digest(raw) for name, raw in parts.items()},
            "independentXmlReparsed": True, "sourcePayloadUsedAsEvidence": False, "readerGeometryCertified": False}


def hidden(tag, attributes):
    tag = tag.lower()
    css = re.sub(r"\s+", "", attributes.get("style", "").lower())
    # aria-hidden is accessibility semantics. Real product SVGs use it inside
    # a visible role=math / aria-label wrapper; it does not suppress painting.
    return (tag in {"script", "style", "head", "template", "defs", "symbol", "clippath", "mask", "pattern", "marker"} or "hidden" in attributes
            or re.search(r"(?:^|;)(?:display:none|visibility:(?:hidden|collapse)|opacity:[+-]?(?:0+(?:\.0*)?|\.0+))(?:!important)?(?:;|$)", css) is not None)


def assert_visible_math_roots(text, xml_root=None):
    if xml_root is not None:
        def visit(node, invisible=False, in_math=False):
            tag = opc.local(node.tag)
            invisible = invisible or hidden(tag, node.attrib)
            in_math = in_math or tag in {"math", "svg"}
            if in_math:
                assert not invisible, "Hidden math cannot certify selected visible output"
                assert tag not in {"mphantom", "maction"}, "Unpainted/choice MathML cannot certify the owned fraction"
            for child in node:
                visit(child, invisible, in_math)
        visit(xml_root)
        return
    class Visibility(HTMLParser):
        def __init__(self):
            super().__init__(convert_charrefs=False)
            self.stack = []
        def handle_starttag(self, tag, attrs):
            invisible = hidden(tag, dict(attrs)) or any(item[1] for item in self.stack)
            in_math = tag in {"math", "svg"} or any(item[0] in {"math", "svg"} for item in self.stack)
            if in_math:
                assert not invisible, "Hidden math cannot certify selected visible output"
                assert tag not in {"mphantom", "maction"}, "Unpainted/choice MathML cannot certify the owned fraction"
            if tag not in {"br", "hr", "img", "input", "meta", "link", "source", "wbr"}:
                self.stack.append((tag, invisible))
        def handle_startendtag(self, tag, attrs):
            self.handle_starttag(tag, attrs)
            self.handle_endtag(tag)
        def handle_endtag(self, tag):
            for index in range(len(self.stack) - 1, -1, -1):
                if self.stack[index][0] == tag:
                    del self.stack[index:]
                    return
    parser = Visibility()
    parser.feed(text)
    parser.close()


def inspect_flow(raw):
    text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    assert_visible_math_roots(text)
    return base.inspect_flow(raw)


def inspect_mathml(raw):
    raw = raw.encode("utf-8") if isinstance(raw, str) else raw
    root = parse_xml(raw)
    assert_visible_math_roots("", xml_root=root)
    return base.inspect_mathml(raw)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    fixture = sub.add_parser("fixture")
    fixture.add_argument("--kind", choices=LOGICAL, required=True)
    fixture.add_argument("--output", type=Path, required=True)
    for action in ("inspect", "fragment", "mathml", "flow", "epub", "pdf", "svg-ink"):
        sub.add_parser(action).add_argument("input", type=Path)
    args = parser.parse_args()
    if args.action == "fixture":
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
        function = {"mathml": inspect_mathml, "flow": inspect_flow, "epub": inspect_epub, "pdf": base.inspect_pdf, "svg-ink": base.svg_ink}[args.action]
        result = function(args.input if args.action in {"epub", "pdf"} else args.input.read_bytes())
    result.setdefault("sha256", opc.digest((args.output if args.action == "fixture" else args.input).read_bytes()))
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
