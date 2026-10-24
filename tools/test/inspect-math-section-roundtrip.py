"""Independent M07/P06 OPC oracle, owned fixtures and headless HTTP broker.

Standard library only. No ReadMD imports, host paths in relationships, network
fetches, Office application, or provenance-derived native assertions.
"""
import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time
import xml.etree.ElementTree as ET

_spec = importlib.util.spec_from_file_location(
    "settings_opc_oracle", Path(__file__).with_name("inspect-docx-settings-roundtrip.py"))
opc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(opc)
W, R, P, NS = opc.W, opc.R, opc.P, opc.NS
M = "http://schemas.openxmlformats.org/officeDocument/2006/math"
STRICT_M = "http://purl.oclc.org/ooxml/officeDocument/math"
ET.register_namespace("m", M)
q, val, xml_bytes = opc.q, opc.val, opc.xml_bytes
MATH_DEFAULTS = {
    "smallFrac": False, "wrapRight": False, "dispDef": True,
    "lMargin": 0, "rMargin": 0, "wrapIndent": 1440,
    "preSp": 0, "postSp": 0, "interSp": 0, "intraSp": 0,
    "defJc": "centerGroup", "brkBin": "before", "brkBinSub": "--",
    "intLim": "subSup", "naryLim": "undOvr",
}
EMPTY_DEFAULTS = {**MATH_DEFAULTS, "smallFrac": True, "wrapRight": True,
                  "lMargin": 1440, "rMargin": 1440}
MATH_ORDER = ("mathFont", "brkBin", "brkBinSub", "smallFrac", "dispDef", "lMargin", "rMargin", "defJc",
              "preSp", "postSp", "interSp", "intraSp", "wrapIndent", "wrapRight", "intLim", "naryLim")
REQUIRED_MATH_VALUES = {"mathFont", "brkBinSub", "lMargin", "rMargin", "defJc", "preSp", "postSp",
                        "interSp", "intraSp", "wrapIndent", "intLim", "naryLim"}
# Explicit compatibility exceptions with documented Office empty defaults.
# They never relax mathFont or the four spacing fields; actual output must
# supply the pinned schema's required native attributes.
LEGACY_IMPLICIT_VALUES = {"brkBinSub", "lMargin", "rMargin", "defJc", "wrapIndent", "intLim", "naryLim"}
MATH_VARIANTS = {
    "missing": {},
    "empty": {name: None for name in ("brkBinSub", "smallFrac", "dispDef", "lMargin", "rMargin", "wrapIndent", "defJc", "intLim", "naryLim")},
    "off": {name: "0" for name in ("smallFrac", "dispDef", "lMargin", "rMargin", "wrapIndent")},
    "on": {"smallFrac": "1", "dispDef": "1", "lMargin": "1440", "rMargin": "1440", "wrapIndent": "1440"},
    "explicit-defaults": {key: ("1" if value is True else "0" if value is False else str(value)) for key, value in MATH_DEFAULTS.items() if key != "wrapRight"},
    "left": {"defJc": "left", "intLim": "undOvr", "naryLim": "subSup"},
    "right": {"defJc": "right", "intLim": "subSup", "naryLim": "undOvr"},
    "disabled-paragraph": {"dispDef": "0", "defJc": "right", "lMargin": "1440", "wrapRight": "1", "preSp": "120", "postSp": "240"},
    "empty-margin": {"lMargin": None, "rMargin": None, "wrapIndent": None},
    "explicit-margin": {"lMargin": "1440", "rMargin": "1440", "wrapIndent": "1440"},
    "small-on": {"smallFrac": "1"},
    "wrap-empty": {"wrapRight": None},
    "wrap-off": {"wrapRight": "0"},
    "wrap-on": {"wrapRight": "1"},
    "indent-zero": {"wrapIndent": "0"},
}


def mq(name):
    return "{" + M + "}" + name


def math_val(node):
    return None if node is None else node.get(mq("val"))


def math_properties(pr, require_explicit_required=False):
    result, seen, order = {}, set(), []
    if pr is not None:
        assert not pr.attrib and not (pr.text or "").strip(), "Invalid native mathPr container"
        for node in pr:
            if node.tag.startswith("{" + M + "}"):
                name = opc.local(node.tag)
                assert name in MATH_ORDER, "Unknown native math property"
                assert name not in seen, "Duplicate native math property"
                seen.add(name)
                order.append(MATH_ORDER.index(name))
                assert not len(node) and not (node.text or "").strip(), "Native math property must be a leaf"
                assert all(key == mq("val") for key in node.attrib), "Unexpected native math attribute namespace/name"
                raw = math_val(node)
                if name in REQUIRED_MATH_VALUES and raw is None:
                    assert not require_explicit_required and name in LEGACY_IMPLICIT_VALUES, name + " required native value missing"
                if name in MATH_DEFAULTS:
                    result[name] = {"state": "empty" if raw is None else "explicit", "value": raw}
            elif require_explicit_required:
                raise AssertionError("Foreign property in native mathPr")
        assert not {"wrapIndent", "wrapRight"} <= seen, "Illegal native mathPr wrap Choice: wrapIndent and wrapRight"
        assert order == sorted(order), "Native mathPr sequence order differs from fixed schema"
    return result


def effective_math(properties, local_alignment=None, display=True):
    values = dict(MATH_DEFAULTS)
    for key, entry in properties.items():
        assert key in MATH_DEFAULTS and entry["state"] in ("empty", "explicit")
        raw = entry["value"]
        if entry["state"] == "empty":
            assert raw is None
            values[key] = EMPTY_DEFAULTS[key]
        elif isinstance(values[key], bool):
            assert raw in ("0", "1", "false", "true", "off", "on"), "Bad math on/off"
            values[key] = raw not in ("0", "false", "off")
        elif isinstance(values[key], int):
            assert raw.isascii() and raw.isdigit() and 0 <= int(raw) <= 31680, "Bad math twips"
            values[key] = int(raw)
        else:
            values[key] = raw
    # MS-OE376 mathPr/dispDef ignores these five paragraph-default items,
    # never the four spacing properties. False retains paragraph inheritance.
    if not values["dispDef"]:
        for key in ("lMargin", "rMargin", "wrapIndent"):
            values[key] = 0
        values["wrapRight"], values["defJc"] = False, None
    if not display:
        values["smallFrac"] = False
        for key in ("preSp", "postSp", "interSp", "intraSp"):
            values[key] = 0
    if local_alignment is not None:
        values["defJc"] = local_alignment
    return values


def formula_rows(root, defaults):
    result = []
    def visit(node, in_paragraph=False):
        if node.tag == mq("oMathPara"):
            jc = node.find("m:oMathParaPr/m:jc", {"m": M})
            alignment = math_val(jc) if jc is not None else None
            for formula in node.findall("m:oMath", {"m": M}):
                append(formula, True, alignment)
            return
        if node.tag == mq("oMath") and not in_paragraph:
            append(node, False, None)
            return
        for child in node:
            visit(child, in_paragraph)
    def append(node, display, alignment):
        text = "".join(n.text or "" for n in node.iter(mq("t")))
        naries = []
        for nary in node.iter(mq("nary")):
            pr = nary.find("m:naryPr", {"m": M})
            location = None if pr is None else math_val(pr.find("m:limLoc", {"m": M}))
            char_node = None if pr is None else pr.find("m:chr", {"m": M})
            char = math_val(char_node) if char_node is not None else "∫"
            naries.append({"character": char, "localLimitLocation": location,
                           "effectiveLimitLocation": location or effective_math(defaults)["intLim" if char == "∫" else "naryLim"]})
        effective = effective_math(defaults, alignment, display)
        for key, integral in (("intLim", True), ("naryLim", False)):
            actual = {row["effectiveLimitLocation"] for row in naries if (row["character"] == "∫") == integral}
            if len(actual) == 1:
                effective[key] = next(iter(actual))
        result.append({"text": text, "display": display, "localAlignment": alignment,
                       "effective": effective,
                       "naries": naries, "fractions": len(list(node.iter(mq("f")))),
                       "sha256": opc.digest(xml_bytes(node))})
    visit(root)
    return result


def semantic_tree(node):
    """Expanded names and ordered content; ignore only formatting whitespace."""
    content = []
    preserve = opc.local(node.tag) in ("t", "delText")
    if node.text is not None and (preserve or node.text.strip()):
        content.append({"text": node.text})
    for child in node:
        content.append({"element": semantic_tree(child)})
        if child.tail is not None and (preserve or child.tail.strip()):
            content.append({"text": child.tail})
    return {"name": node.tag, "attributes": dict(sorted(node.attrib.items())), "content": content}


def math_objects(root):
    objects = []
    def visit(node):
        if node.tag in (mq("oMathPara"), mq("oMath")):
            objects.append(semantic_tree(node))
            return
        for child in node:
            visit(child)
    visit(root)
    return objects


def inspect_package(package, require_explicit_required=False):
    document = package.xml(package.document)
    assert document.tag == q("document")
    body = document.find("w:body", NS)
    assert body is not None
    assert not list(body.iter(mq("mathPr"))), "Nested native mathPr in the document body"
    settings_part = package.related("settings")
    settings = package.xml(settings_part) if settings_part else ET.Element(q("settings"))
    assert settings.tag == q("settings")
    defaults = math_properties(settings.find("m:mathPr", {"m": M}), require_explicit_required)
    relationships = {row["id"]: row for row in package.relationships(package.document)}
    notes, story_maps = {}, {}
    for kind in ("footnote", "endnote"):
        part = package.related(kind + "s")
        definitions = []
        if part:
            story = package.xml(part)
            assert story.tag == q(kind + "s")
            ids = set()
            for definition in story:
                assert definition.tag == q(kind), "Foreign note definition"
                identity = val(definition, "id")
                assert identity and identity not in ids
                ids.add(identity)
                definitions.append({"id": identity, "type": val(definition, "type") or "normal", "text": opc.visible(definition)})
        story_maps[kind] = {row["id"]: row for row in definitions if row["type"] == "normal"}
        notes[kind] = {"part": part, "definitions": definitions}
    global_pos = val(settings.find("w:footnotePr/w:pos", NS)) or "pageBottom"
    sections, current = [], []
    def paragraph(node):
        references = []
        for child in node.iter():
            for kind in ("footnote", "endnote"):
                if child.tag == q(kind + "Reference"):
                    identity = val(child, "id")
                    assert identity in story_maps[kind], "Unresolved story-specific reference"
                    references.append({"kind": kind, "id": identity, "text": story_maps[kind][identity]["text"]})
        links = []
        for link in node.iter(q("hyperlink")):
            identity, anchor = link.get("{" + R + "}id"), val(link, "anchor")
            if identity:
                rel = relationships.get(identity)
                assert rel and rel["kind"] == "hyperlink", "Unresolved actual hyperlink"
                target = rel["target"]
            else:
                assert anchor
                target = "#" + anchor
            links.append({"text": opc.visible(link), "target": target})
        return {"text": opc.visible(node), "references": references, "links": links,
                "bookmarks": [val(mark, "name") for mark in node.iter(q("bookmarkStart"))]}
    def finish(section, path, index):
        pos = val(section.find("w:footnotePr/w:pos", NS))
        sections.append({"ordinal": len(sections), "terminatorPath": path, "bodyEnd": index,
                         "nativePlacement": pos, "effectivePlacement": pos or global_pos,
                         "paragraphs": list(current), "break": val(section.find("w:type", NS)) or "nextPage"})
        current.clear()
    for index, child in enumerate(body):
        if child.tag == q("sectPr"):
            finish(child, [index], index)
        else:
            if child.tag == q("p"):
                current.append(paragraph(child))
            elif child.tag == q("sdt"):
                for p in child.iter(q("p")):
                    current.append(paragraph(p))
            boundary = child.find("w:pPr/w:sectPr", NS) if child.tag == q("p") else None
            if boundary is not None:
                finish(boundary, [index, list(child).index(child.find("w:pPr", NS)), list(child.find("w:pPr", NS)).index(boundary)], index)
    assert not current, "Unterminated main story"
    return {"kind": "math-section-opc", "documentPart": package.document,
            "parts": {name: opc.digest(raw) for name, raw in sorted(package.parts.items())},
            "auxiliaryParts": {name: opc.digest(raw) for name, raw in sorted(package.parts.items()) if name.startswith("customXml/")},
            "mathProperties": defaults, "mathEffective": effective_math(defaults),
            "mathSchema": {"wrapChoiceValidated": True, "sequenceValidated": True,
                           "requiredAttributesExplicit": require_explicit_required,
                           "documentedImplicitProperties": [name for name in MATH_ORDER if name in defaults and defaults[name]["state"] == "empty" and name in REQUIRED_MATH_VALUES]},
            "formulas": formula_rows(body, defaults), "mathObjects": math_objects(body), "sections": sections, "notes": notes,
            "sourcePayloadUsedAsNativeEvidence": False, "applicationRoundtripCertified": False}


def _run(parent, text):
    run = ET.SubElement(parent, q("r"))
    ET.SubElement(run, q("t"), {"{http://www.w3.org/XML/1998/namespace}space": "preserve"}).text = text
    return run


def section_fixture(empty=False):
    parts = opc.fixture_parts("native-multi-section")
    document = opc.parse_xml(parts["word/document.xml"])
    body = document.find("w:body", NS)
    body.clear()
    for index, placement in enumerate(("beneathText", "pageBottom")):
        heading = ET.SubElement(body, q("p"))
        ET.SubElement(ET.SubElement(heading, q("pPr")), q("pStyle"), {q("val"): "Heading1"})
        ET.SubElement(heading, q("bookmarkStart"), {q("id"): str(60 + index), q("name"): "section_" + str(index + 1)})
        _run(heading, "SECTION_SAME_HEADING")
        ET.SubElement(heading, q("bookmarkEnd"), {q("id"): str(60 + index)})
        p = ET.SubElement(body, q("p"))
        run = _run(p, "SECTION_" + ("A" if index == 0 else "B") + "_BODY ")
        ET.SubElement(run, q("footnoteReference"), {q("id"): "7" if index == 0 else "8"})
        ET.SubElement(_run(p, " repeated "), q("footnoteReference"), {q("id"): "7" if index == 0 else "8"})
        ET.SubElement(_run(p, " end "), q("endnoteReference"), {q("id"): "7"})
        link = ET.SubElement(p, q("hyperlink"), {"{" + R + "}id": "qaLink"})
        _run(link, "SECTION_EVIDENCE")
        if index == 0:
            terminal = ET.SubElement(body, q("p"))
            props = ET.SubElement(terminal, q("pPr"))
            ET.SubElement(props, q("pStyle"), {q("val"): "CodeBlock"})
            section = ET.SubElement(props, q("sectPr"))
            ET.SubElement(section, q("type"), {q("val"): "continuous"})
            _run(terminal, "SECTION_A_CODE_END")
        else:
            section = ET.SubElement(body, q("sectPr"))
        ET.SubElement(ET.SubElement(section, q("footnotePr")), q("pos"), {q("val"): placement})
        ET.SubElement(section, q("pgSz"), {q("w"): "11906", q("h"): "16838"})
        ET.SubElement(section, q("pgMar"), {q(name): "1134" for name in ("top", "bottom", "left", "right")})
        if empty and index == 0:
            p = ET.SubElement(body, q("p"))
            boundary = ET.SubElement(ET.SubElement(p, q("pPr")), q("sectPr"))
            ET.SubElement(boundary, q("type"), {q("val"): "continuous"})
            ET.SubElement(ET.SubElement(boundary, q("footnotePr")), q("pos"), {q("val"): "pageBottom"})
    parts["word/document.xml"] = xml_bytes(document)
    rels = opc.parse_xml(parts["word/_rels/document.xml.rels"])
    ET.SubElement(rels, "{" + P + "}Relationship", {"Id": "qaLink", "Type": R + "/hyperlink", "Target": "https://example.test/section-evidence", "TargetMode": "External"})
    parts["word/_rels/document.xml.rels"] = xml_bytes(rels)
    # A genuine code paragraph style, independent of its authored text/heading.
    styles = opc.parse_xml(parts["word/styles.xml"])
    style = ET.SubElement(styles, q("style"), {q("type"): "paragraph", q("styleId"): "CodeBlock"})
    ET.SubElement(style, q("name"), {q("val"): "Code Block"})
    ET.SubElement(ET.SubElement(style, q("rPr")), q("rFonts"), {q("ascii"): "Consolas", q("hAnsi"): "Consolas"})
    parts["word/styles.xml"] = xml_bytes(styles)
    return parts


def math_fixture(variant="missing", marker="FORMULA_A", local_alignment=None):
    assert variant in MATH_VARIANTS
    parts = opc.fixture_parts("native-numbering")
    document = opc.parse_xml(parts["word/document.xml"])
    body = document.find("w:body", NS)
    body.clear()
    _run(ET.SubElement(body, q("p")), "MATH_SOURCE_" + marker)
    para = ET.SubElement(ET.SubElement(body, q("p")), mq("oMathPara"))
    if local_alignment:
        ET.SubElement(ET.SubElement(para, mq("oMathParaPr")), mq("jc"), {mq("val"): local_alignment})
    formula = ET.SubElement(para, mq("oMath"))
    ET.SubElement(ET.SubElement(formula, mq("r")), mq("t")).text = marker
    fraction = ET.SubElement(formula, mq("f"))
    for role, text in (("num", "a"), ("den", "b")):
        ET.SubElement(ET.SubElement(ET.SubElement(fraction, mq(role)), mq("r")), mq("t")).text = text
    for char in ("∫", "∑"):
        nary = ET.SubElement(formula, mq("nary"))
        ET.SubElement(ET.SubElement(nary, mq("naryPr")), mq("chr"), {mq("val"): char})
        for role, text in (("sub", "0"), ("sup", "1"), ("e", "x")):
            ET.SubElement(ET.SubElement(ET.SubElement(nary, mq(role)), mq("r")), mq("t")).text = text
    ET.SubElement(body, q("sectPr"))
    settings = opc.parse_xml(parts["word/settings.xml"])
    if MATH_VARIANTS[variant]:
        defaults = ET.SubElement(settings, mq("mathPr"))
        for name in MATH_ORDER:
            if name not in MATH_VARIANTS[variant]:
                continue
            value = MATH_VARIANTS[variant][name]
            ET.SubElement(defaults, mq(name), {} if value is None else {mq("val"): value})
    parts["word/document.xml"], parts["word/settings.xml"] = xml_bytes(document), xml_bytes(settings)
    return parts


def invalid_wrap_choice_fixture():
    parts = math_fixture()
    settings = opc.parse_xml(parts["word/settings.xml"])
    pr = ET.SubElement(settings, mq("mathPr"))
    ET.SubElement(pr, mq("wrapIndent"), {mq("val"): "1440"})
    ET.SubElement(pr, mq("wrapRight"), {mq("val"): "0"})
    parts["word/settings.xml"] = xml_bytes(settings)
    return parts


def mutate_parts(parts, mode, ordinal=0):
    package = opc.Package(parts=parts)
    before = inspect_package(package)
    changed = set()
    if mode in ("section-toggle", "section-foreign-format"):
        document = copy.deepcopy(package.xml(package.document))
        sections = list(document.iter(q("sectPr")))
        assert 0 <= ordinal < len(sections)
        section = sections[ordinal]
        if mode == "section-toggle":
            pos = section.find("w:footnotePr/w:pos", NS)
            assert pos is not None
            pos.set(q("val"), "pageBottom" if val(pos) == "beneathText" else "beneathText")
        else:
            pr = section.find("w:footnotePr", NS)
            assert pr is not None
            ET.SubElement(pr, "{urn:readmd:qa:foreign}numFmt", {q("val"): "decimal"})
        package.parts[package.document] = xml_bytes(document)
        changed.add(package.document)
    elif mode == "math-native-small":
        part = package.related("settings", optional=False)
        settings = copy.deepcopy(package.xml(part))
        pr = settings.find("m:mathPr", {"m": M})
        if pr is None:
            pr = ET.SubElement(settings, mq("mathPr"))
        small = pr.find("m:smallFrac", {"m": M})
        if small is None:
            small = ET.SubElement(pr, mq("smallFrac"))
        small.set(mq("val"), "1")
        pr[:] = sorted(pr, key=lambda node: MATH_ORDER.index(opc.local(node.tag)))
        package.parts[part] = xml_bytes(settings)
        changed.add(part)
    elif mode in ("unsupported-mixed-size", "invalid-note-format", "foreign-note-format"):
        if mode == "unsupported-mixed-size":
            part = package.related("footnotes", optional=False)
            story = copy.deepcopy(package.xml(part))
            normal = next(node for node in story if val(node, "type") in (None, "normal"))
            para = normal.find("w:p", NS)
            for size, text in (("20", " MIXED_TEN"), ("60", " MIXED_THIRTY")):
                run = _run(para, text)
                properties = ET.Element(q("rPr"))
                ET.SubElement(properties, q("sz"), {q("val"): size})
                ET.SubElement(properties, q("szCs"), {q("val"): size})
                run.insert(0, properties)
            package.parts[part] = xml_bytes(story)
        else:
            part = package.related("settings", optional=False)
            settings = copy.deepcopy(package.xml(part))
            pr = settings.find("w:footnotePr", NS)
            node = pr.find("w:numFmt", NS)
            assert node is not None
            if mode == "foreign-note-format":
                node.tag = "{urn:readmd:qa:foreign}numFmt"
            else:
                node.set(q("val"), "nativeMadeUp")
            package.parts[part] = xml_bytes(settings)
        changed.add(part)
    else:
        raise AssertionError("Unknown mutation " + mode)
    assert all(package.parts[name] == raw for name, raw in parts.items() if name not in changed)
    after = inspect_package(opc.Package(parts=package.parts))
    assert before["auxiliaryParts"] == after["auxiliaryParts"], "Auxiliary bytes changed"
    return package.parts, {"mode": mode, "changedParts": sorted(changed), "ordinal": ordinal,
                           "auxiliaryUnchanged": True, "applicationEditingCertified": False, "inspection": after}


def inspect_fragment(raw):
    root = opc.parse_xml(("<qa xmlns:m=\"" + M + "\" xmlns:w=\"" + W + "\">" + raw + "</qa>").encode())
    properties = [node for node in root.iter(mq("mathPr"))]
    assert len(properties) <= 1
    assert all(node in list(root) for node in properties), "Nested native mathPr is not a root document-default declaration"
    defaults = math_properties(properties[0] if properties else None)
    tree = semantic_tree(root)
    return {"mathProperties": defaults, "mathEffective": effective_math(defaults),
            "formulas": formula_rows(root, defaults), "mathObjects": math_objects(root),
            "semanticTree": tree, "semanticTreeSha256": opc.digest(json.dumps(tree, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8"))}


def inspect_svg(raw):
    root = opc.parse_xml(raw)
    assert root.tag == "{http://www.w3.org/2000/svg}svg"
    texts, regions, glyphs, baselines = [], [], [], set()
    for node in root.iter():
        if node.tag == "{http://www.w3.org/2000/svg}text":
            texts.append({"text": "".join(node.itertext()), "x": node.get("x"), "y": node.get("y"), "size": node.get("font-size"), "transform": node.get("transform")})
        if node.tag == "{http://www.w3.org/2000/svg}g" and any("auto-wrap-line" in str(value) for value in node.attrib.values()):
            regions.append(dict(node.attrib))
            if node.get("data-math-line-baseline") is not None:
                baselines.add(float(node.get("data-math-line-baseline")))
        if node.tag == "{http://www.w3.org/2000/svg}path" and node.get("transform"):
            match = re.fullmatch(r"translate\(([-+0-9.e]+) ([-+0-9.e]+)\) scale\(([-+0-9.e]+)(?: ([-+0-9.e]+))?\)(?: matrix\([^)]*\))?", node.get("transform"))
            assert match and node.get("d"), "Unsupported actual glyph transform"
            x, y, sx, sy = match.groups()
            glyphs.append({"outlineSha256": opc.digest(node.get("d").encode()), "x": float(x), "y": float(y),
                           "scaleX": float(sx), "scaleY": float(sy or sx)})
    assert texts or glyphs, "No actual SVG glyphs"
    return {"viewBox": root.get("viewBox"), "width": root.get("width"), "height": root.get("height"), "texts": texts,
            "glyphPaths": glyphs, "wrapRegions": regions, "wrapBaselines": sorted(baselines),
            "alignment": root.get("data-math-justification")}


def http_broker(args):
    """Run only the owned headless host. Parent supplies a hidden console.

    CREATE_NEW_CONSOLE belongs to this Python broker, not its child. The child
    gets its own process group in the inherited console. Windows ignores the
    NEW_PROCESS_GROUP flag when combined with NEW_CONSOLE on the same process.
    """
    assert os.name == "nt", "HTTP broker is Windows QA only"
    binary, profile, documents, assets = map(lambda value: Path(value).resolve(strict=True), (args.binary, args.profile, args.documents, args.assets))
    assert binary.is_file() and all(value.is_dir() for value in (profile, documents, assets))
    assert 0 < args.port < 65536
    out = Path(args.out).resolve(strict=True)
    assert out.is_dir() and profile.parent == out.parent and documents.parent == out.parent, "Broker paths are not one owned evidence root"
    assert not (profile / "instance.json").exists(), "Refuse an occupied profile"
    environment = dict(os.environ)
    for key in list(environment):
        if key.upper().startswith(("READMD_APP_TOKEN", "READMD_REQUIRE_TOKEN", "READMD_PRINT_TOKEN", "LAN_TOKEN", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY")):
            del environment[key]
    environment["READMD_DATA_DIR"], environment["READMD_WORKSPACE"] = str(profile), str(documents)
    environment["PATH"] = str(Path(environment.get("SystemRoot", "C:/Windows")) / "System32")
    command = [str(binary), "--no-window", "--host", "127.0.0.1", "--port", str(args.port), "--data-dir", str(profile), "--workspace", str(documents), "--assets", str(assets)]
    stop = threading.Event()
    input_errors = []
    def stdin_loop():
        try:
            for line in sys.stdin:
                assert line.strip() == "shutdown", "Broker accepts only shutdown"
                break
        except BaseException as error:
            input_errors.append(str(error))
        finally:
            stop.set()
    child, forced, normal, failure = None, False, False, None
    report = {"schemaVersion": 1, "kind": "owned-headless-http-broker", "passed": False, "hiddenConsole": True,
              "browserStarted": False, "webviewStarted": False, "targetedCtrlBreak": False, "forcedStop": False}
    try:
        with (out / "host.stdout.log").open("xb") as stdout, (out / "host.stderr.log").open("xb") as stderr:
            child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                     creationflags=subprocess.CREATE_NEW_PROCESS_GROUP, env=environment)
            assert child.pid > 0 and child.pid != os.getpid()
            report["pid"] = child.pid
            print(json.dumps({"event": "host-created", "pid": child.pid, "port": args.port}), flush=True)
            threading.Thread(target=stdin_loop, daemon=True).start()
            deadline = time.monotonic() + 600
            while not stop.wait(0.1):
                assert time.monotonic() < deadline, "Owned HTTP broker deadline"
                assert sum(path.stat().st_size for path in (out / "host.stdout.log", out / "host.stderr.log")) <= 16 * 1024 * 1024, "Owned HTTP host log budget"
                if child.poll() is not None:
                    raise AssertionError("Owned HTTP host exited before shutdown")
            assert child.poll() is None, "Owned HTTP host already exited"
            # Windows directs this event only to the group created above.
            child.send_signal(signal.CTRL_BREAK_EVENT)
            report["targetedCtrlBreak"] = True
            try:
                child.wait(timeout=15)
            except subprocess.TimeoutExpired:
                forced = True
                child.kill()
                child.wait(timeout=5)
            normal = not forced and not input_errors and child.returncode == 0 and not (profile / "instance.json").exists()
            if input_errors:
                failure = input_errors[0]
    except BaseException as error:
        failure = str(error)
    finally:
        if child is not None and child.poll() is None:
            try:
                child.send_signal(signal.CTRL_BREAK_EVENT)
                report["targetedCtrlBreak"] = True
                child.wait(timeout=5)
            except BaseException:
                forced = True
                child.kill()
                child.wait(timeout=5)
        report.update({"passed": normal and failure is None, "forcedStop": forced,
                       "hostExitCode": None if child is None else child.returncode,
                       "instanceRemoved": not (profile / "instance.json").exists(), "error": failure})
        with (out / "broker-report.json").open("x", encoding="utf-8") as stream:
            json.dump(report, stream, ensure_ascii=False, allow_nan=False, indent=2)
        print(json.dumps({"event": "host-stopped", **report}), flush=True)
    return report


def launch_http_broker(args):
    """Create the broker's hidden console, then inherit it in the host group."""
    assert os.name == "nt", "HTTP broker is Windows QA only"
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0  # SW_HIDE; never create a visible console window.
    command = [sys.executable, "-I", "-B", str(Path(__file__).resolve()), "http-broker", "--console-worker"]
    for name in ("binary", "profile", "documents", "assets", "out", "port"):
        command.extend(("--" + name, str(getattr(args, name))))
    # Do not combine CREATE_NEW_PROCESS_GROUP with CREATE_NEW_CONSOLE here.
    child = subprocess.Popen(command, stdin=sys.stdin, stdout=sys.stdout, stderr=sys.stderr,
                             startupinfo=startup, creationflags=subprocess.CREATE_NEW_CONSOLE)
    try:
        code = child.wait(timeout=630)
    except BaseException:
        # Closing inherited stdin normally asks the worker to clean its host.
        # No broad console event or user process lookup is ever issued.
        raise
    report = json.loads((Path(args.out) / "broker-report.json").read_text(encoding="utf-8"))
    assert code == (0 if report["passed"] else 1), "Broker exit/report disagree"
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    read = commands.add_parser("inspect")
    read.add_argument("input", type=Path)
    read.add_argument("--strict-native", action="store_true")
    fixture = commands.add_parser("fixture")
    fixture.add_argument("--kind", choices=("sections", "empty-section", "math", "invalid-math-choice"), required=True)
    fixture.add_argument("--variant", choices=tuple(MATH_VARIANTS), default="missing")
    fixture.add_argument("--marker", default="FORMULA_A")
    fixture.add_argument("--local-alignment", choices=("left", "right", "center", "centerGroup"))
    fixture.add_argument("--output", type=Path, required=True)
    mutation = commands.add_parser("mutate")
    mutation.add_argument("--input", type=Path, required=True)
    mutation.add_argument("--output", type=Path, required=True)
    mutation.add_argument("--mode", choices=("section-toggle", "section-foreign-format", "math-native-small", "unsupported-mixed-size", "invalid-note-format", "foreign-note-format"), required=True)
    mutation.add_argument("--ordinal", type=int, default=0)
    for command in ("fragment", "svg"):
        child = commands.add_parser(command)
        child.add_argument("input", type=Path)
    broker = commands.add_parser("http-broker")
    broker.add_argument("--console-worker", action="store_true", help=argparse.SUPPRESS)
    for name in ("binary", "profile", "documents", "assets", "out"):
        broker.add_argument("--" + name, required=True)
    broker.add_argument("--port", type=int, required=True)
    args = parser.parse_args(argv)
    if args.action == "http-broker":
        return http_broker(args) if args.console_worker else launch_http_broker(args)
    if args.action == "inspect":
        return inspect_package(opc.Package(path=args.input), args.strict_native)
    if args.action == "fragment":
        return inspect_fragment(args.input.read_text(encoding="utf-8"))
    if args.action == "svg":
        return inspect_svg(args.input.read_bytes())
    if args.action == "fixture":
        if args.kind == "invalid-math-choice":
            parts = invalid_wrap_choice_fixture()
            opc.write_package(args.output, parts)
            try:
                inspect_package(opc.Package(path=args.output))
            except AssertionError as error:
                assert "Choice" in str(error), "Illegal Choice control failed for another reason"
                return {"nativeOnly": True, "schemaValid": False, "inspectionRejected": True,
                        "violation": "mathPr_wrapIndent_wrapRight_Choice", "applicationEditingCertified": False,
                        "parts": {name: opc.digest(raw) for name, raw in sorted(parts.items())}}
            raise AssertionError("Illegal Choice fixture was accepted")
        parts = math_fixture(args.variant, args.marker, args.local_alignment) if args.kind == "math" else section_fixture(args.kind == "empty-section")
        opc.write_package(args.output, parts)
        return {"nativeOnly": True, "applicationEditingCertified": False, "inspection": inspect_package(opc.Package(path=args.output))}
    parts, report = mutate_parts(opc.Package(path=args.input).parts, args.mode, args.ordinal)
    opc.write_package(args.output, parts)
    report["outputSha256"] = opc.digest(args.output.read_bytes())
    return report


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    result = main()
    if not (isinstance(result, dict) and result.get("kind") == "owned-headless-http-broker"):
        print(json.dumps(result, ensure_ascii=False, allow_nan=False))
    if isinstance(result, dict) and result.get("passed") is False:
        sys.exit(1)
