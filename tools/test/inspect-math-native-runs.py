"""Independent finite native Word math run-source oracle.

Only standard-library ZIP/XML and existing pure geometry helpers are used.
Native evidence is the actual OPC graph and run declarations, never a ReadMD
carrier, product alias table, materialized model, or application screenshot.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import xml.etree.ElementTree as ET
import zipfile

_spec = importlib.util.spec_from_file_location(
    "native_runs_opc_helpers", Path(__file__).with_name("inspect-math-native-styles.py"))
helpers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(helpers)
opc, W, M, NS, w, m = helpers.opc, helpers.W, helpers.M, helpers.NS, helpers.w, helpers.m
parse_xml = helpers.parse_xml
AXES = ("b", "i", "sz", "color")
TRUE, FALSE = {None, "1", "true", "on"}, {"0", "false", "off"}
KINDS = ("a", "b")
VARIANTS = ("normal", "defaults", "theme", "auto", "size-range",
            "wrong-type", "missing-parent", "cycle", "duplicate-id")
MAX_STYLES, MAX_CHAIN, MAX_SITES = 4096, 128, 16384
ET.register_namespace("w", W)
ET.register_namespace("m", M)


def canonical(node):
    if node is None:
        return None
    text = node.text or ""
    if node.tag not in (w("t"), m("t")) and not text.strip():
        text = ""
    return {"name": node.tag, "attributes": dict(sorted(node.attrib.items())),
            "text": text, "children": [canonical(child) for child in node]}


def one(parent, tag):
    found = [] if parent is None else parent.findall(tag)
    assert len(found) <= 1, "Duplicate XML particle: " + tag
    return found[0] if found else None


def boolean(raw):
    assert raw in TRUE | FALSE, "Invalid Word on/off spelling"
    return raw in TRUE


def size_number(raw):
    if raw is None:
        return None, "run-size-not-integer"
    assert len(raw) <= 65536, "Run size lexical budget"
    normalized = raw.strip(" \t\r\n")
    if not re.fullmatch(r"[+]?[0-9]+", normalized, re.ASCII):
        return None, "run-size-not-integer"
    digits = normalized.lstrip("+").lstrip("0") or "0"
    if len(digits) > 4 or not 1 <= int(digits) <= 3276:
        return None, "run-size-outside-domain"
    return int(digits), None


class Package(helpers.Package):
    """Keep the existing bounds and reject nonordinary/ambiguous ZIP members."""
    def __init__(self, file=None, parts=None):
        if file is not None:
            file = Path(file)
            assert file.is_file() and not file.is_symlink(), "Ordinary input file required"
            assert 0 < file.stat().st_size <= opc.MAX_BYTES, "ZIP byte budget"
            with zipfile.ZipFile(file) as archive:
                entries = archive.infolist()
                assert len(entries) <= 4096
                assert sum(row.file_size for row in entries) <= opc.MAX_BYTES
                for row in entries:
                    mode = row.external_attr >> 16
                    assert not row.flag_bits & 1 and not stat.S_ISLNK(mode), "Encrypted/linked ZIP member"
                    assert not row.is_dir(), "Owned OPC uses ordinary member files"
        super().__init__(path=file, parts=parts)
        assert len(self.parts) <= 4096 and sum(map(len, self.parts.values())) <= opc.MAX_BYTES
        folded = set()
        for name, raw in self.parts.items():
            assert isinstance(raw, bytes)
            assert name and "\0" not in name and ":" not in name
            assert all(part not in ("", ".", "..") for part in name.split("/"))
            assert not PurePosixPath(name).is_absolute()
            assert name.casefold() not in folded, "Ambiguous OPC member spelling"
            folded.add(name.casefold())


def declaration(pr):
    result = {"rPrPresent": pr is not None, "rStylePresent": False, "rStyle": None,
              "axes": {}, "otherProperties": [], "tree": canonical(pr), "lossReasons": []}
    if pr is not None:
        assert pr.tag == w("rPr") and not pr.attrib and not (pr.text or "").strip(), "Invalid rPr"
    for axis in AXES:
        node = one(pr, w(axis))
        if node is None:
            state, raw, tree = "missing", None, None
        else:
            assert not len(node) and not (node.text or "").strip(), "Run axis must be an attribute leaf"
            raw = node.get(w("val"))
            state, tree = ("empty" if raw is None else "explicit"), canonical(node)
            if axis in ("b", "i"):
                assert set(node.attrib) <= {w("val")}
                boolean(raw)
            elif axis == "sz":
                assert set(node.attrib) <= {w("val")}
                _, reason = size_number(raw)
                if reason:
                    result["lossReasons"].append(reason)
            else:
                assert set(node.attrib) <= {w("val"), w("themeColor"), w("themeTint"), w("themeShade")}
                if set(node.attrib) != {w("val")} or not re.fullmatch(r"[0-9a-fA-F]{6}", raw or ""):
                    result["lossReasons"].append("run-color-not-literal-rgb")
        result["axes"][axis] = {"state": state, "raw": raw, "tree": tree}
    if pr is not None:
        style = one(pr, w("rStyle"))
        if style is not None:
            assert set(style.attrib) == {w("val")} and not len(style) and style.get(w("val")), "Invalid rStyle"
            result["rStylePresent"], result["rStyle"] = True, style.get(w("val"))
        seen = set()
        for node in pr:
            assert node.tag not in seen, "Duplicate direct run property"
            seen.add(node.tag)
            if node.tag not in {w(axis) for axis in AXES} | {w("rStyle")}:
                result["otherProperties"].append(canonical(node))
    return result


class Context:
    def __init__(self, root=None, part=None):
        self.root, self.part, self.nodes, self.definitions = root, part, {}, {}
        if root is not None:
            assert root.tag == w("styles")
        defaults = one(root, w("docDefaults"))
        run_defaults = one(one(defaults, w("rPrDefault")), w("rPr"))
        self.run_defaults, self.doc_defaults = declaration(run_defaults), canonical(defaults)
        self.paragraph_default, self.character_default = None, None
        nodes = [] if root is None else root.findall(w("style"))
        assert len(nodes) <= MAX_STYLES, "Style definition budget"
        for node in nodes:
            identity, kind = node.get(w("styleId")), node.get(w("type"))
            assert identity and len(identity) <= 4096 and identity not in self.nodes, "Duplicate/missing style ID"
            assert kind in ("paragraph", "character", "table", "numbering"), "Unknown style type"
            parent = one(node, w("basedOn"))
            if parent is not None:
                assert set(parent.attrib) == {w("val")} and parent.get(w("val")) and not len(parent), "Invalid basedOn"
            rpr = declaration(one(node, w("rPr")))
            assert not rpr["rStylePresent"], "Style rPr cannot introduce a second style-reference graph"
            default = boolean(node.get(w("default"))) if w("default") in node.attrib else False
            if default and kind in ("paragraph", "character"):
                key = "paragraph_default" if kind == "paragraph" else "character_default"
                assert getattr(self, key) is None, "Multiple default styles of one type"
                setattr(self, key, identity)
            self.nodes[identity] = node
            self.definitions[identity] = {
                "id": identity, "type": kind, "default": default,
                "basedOn": None if parent is None else parent.get(w("val")),
                "pPr": canonical(one(node, w("pPr"))), "rPr": rpr, "tree": canonical(node)}

    def chain(self, identity, kind):
        output, seen = [], set()
        while identity is not None:
            assert identity not in seen and len(output) < MAX_CHAIN, "Style cycle/depth budget"
            seen.add(identity)
            node = self.definitions.get(identity)
            assert node is not None and node["type"] == kind, "Missing/wrong-type style parent"
            output.append(node)
            identity = node["basedOn"]
        return list(reversed(output))

    def description(self):
        return {"part": self.part, "definitions": self.definitions, "docDefaultsTree": self.doc_defaults,
                "runDefaults": self.run_defaults, "paragraphDefault": self.paragraph_default,
                "characterDefault": self.character_default}


def value_of(axis, cell):
    if cell["state"] == "missing":
        return None
    if axis in ("b", "i"):
        return boolean(cell["raw"])
    raw = cell["raw"]
    if axis == "sz":
        return raw if size_number(raw)[0] is not None else None
    return raw if cell["tree"]["attributes"] == {w("val"): raw} and re.fullmatch(r"[0-9a-fA-F]{6}", raw or "") else None


def cascade(context, paragraph_chain, direct):
    """Word style toggles differ from absolute root/default/direct switches."""
    character_id = direct["rStyle"] if direct["rStylePresent"] else context.character_default
    character_chain = context.chain(character_id, "character") if character_id is not None else []
    values, origins, trace = {"b": False, "i": False, "sz": None, "color": None}, {}, []
    # False here is the implicit Word switch basis, not an authored Missing
    # declaration. Root compatibility is proved separately when comparing OPC.
    origins.update({axis: "implicit-word-basis" for axis in AXES})
    layers = [("docDefaults", context.run_defaults, False)]
    layers += [("paragraph:" + row["id"], row["rPr"], True) for row in paragraph_chain]
    layers += [("character:" + row["id"], row["rPr"], True) for row in character_chain]
    layers.append(("direct", direct, False))
    losses = []
    for origin, layer, toggle in layers:
        losses.extend(layer["lossReasons"])
        for axis in AXES:
            cell = layer["axes"][axis]
            if cell["state"] == "missing":
                continue
            value, before, applied = value_of(axis, cell), values[axis], True
            if axis in ("b", "i") and toggle:
                if not value:
                    applied = False
                else:
                    values[axis] = not values[axis]
            else:
                values[axis] = value
            if applied:
                origins[axis] = origin
            trace.append({"axis": axis, "origin": origin, "mode": "toggle" if toggle and axis in ("b", "i") else "absolute",
                          "declaration": cell, "before": before, "after": values[axis], "applied": applied})
    effective = {"axes": values, "origins": origins, "trace": trace,
                 "sizePoints": None if values["sz"] is None else size_number(values["sz"])[0] / 2}
    return character_chain, effective, sorted(set(losses))


def math_shape(node):
    if node.tag == w("rPr"):
        return None
    return {"name": node.tag, "attributes": dict(sorted(node.attrib.items())),
            "text": node.text or "" if node.tag == m("t") else "",
            "children": [shape for child in node if (shape := math_shape(child)) is not None]}


def paragraph_selection(context, pr):
    selected = one(pr, w("pStyle"))
    if selected is not None:
        assert set(selected.attrib) == {w("val")} and selected.get(w("val")) and not len(selected)
    identity = selected.get(w("val")) if selected is not None else context.paragraph_default
    return identity, context.chain(identity, "paragraph") if identity is not None else []


def inspect_math(context, objects, pr, ordinal, body_index):
    identity, chain = paragraph_selection(context, pr)
    sites = []
    def visit(node, path):
        if node.tag in (m("r"), m("ctrlPr")):
            direct = declaration(one(node, w("rPr")))
            characters, effective, losses = cascade(context, chain, direct)
            # Nonempty source defaults and implicit character selection are
            # reported explicitly; pairwise output proof handles covered axes.
            if context.run_defaults["rPrPresent"] and any(context.run_defaults["axes"][axis]["state"] != "missing" for axis in AXES):
                losses.append("source-root-run-defaults")
            if not direct["rStylePresent"]:
                losses.append("no-explicit-character-reference")
            sites.append({"ordinal": len(sites), "kind": "run" if node.tag == m("r") else "control",
                          "path": path, "text": "".join(leaf.text or "" for leaf in node.iter(m("t"))),
                          "direct": direct, "characterChain": [row["id"] for row in characters],
                          "effective": effective, "nativeEligible": not losses, "lossReasons": sorted(set(losses))})
        for index, child in enumerate(node):
            if child.tag != w("rPr"):
                visit(child, path + "/" + opc.local(child.tag) + "[" + str(index) + "]")
    for index, obj in enumerate(objects):
        visit(obj, "math[" + str(index) + "]")
    assert len(sites) <= MAX_SITES
    return {"ordinal": ordinal, "bodyIndex": body_index, "pPr": canonical(pr),
            "paragraphStyle": identity, "paragraphChain": [row["id"] for row in chain],
            "mathShape": [math_shape(obj) for obj in objects], "sites": sites}


def inspect_package(package):
    document = package.xml(package.document)
    assert document.tag == w("document")
    body = one(document, w("body"))
    assert body is not None
    part = package.related("styles")
    context = Context(None if part is None else package.xml(part), part)
    paragraphs, ordinary = [], []
    for body_index, paragraph in enumerate(body):
        if paragraph.tag != w("p"):
            continue
        pr = one(paragraph, w("pPr"))
        groups = paragraph.findall(m("oMathPara"))
        objects = [obj for group in groups for obj in group.findall(m("oMath"))]
        objects += paragraph.findall(m("oMath"))
        if objects:
            assert len(groups) <= 1 and all(obj.tag == m("oMath") for obj in objects)
            assert not any(node.tag in (w("styles"), w("pPr")) for obj in objects for node in obj.iter()), "Internal context in native OMML"
            paragraphs.append(inspect_math(context, objects, pr, len(paragraphs), body_index))
        _, chain = paragraph_selection(context, pr)
        for run_index, run in enumerate(paragraph.findall(w("r"))):
            direct = declaration(one(run, w("rPr")))
            _, effective, losses = cascade(context, chain, direct)
            ordinary.append({"bodyIndex": body_index, "runIndex": run_index, "tree": canonical(run),
                             "effective": effective, "lossReasons": losses})
    assert sum(len(row["sites"]) for row in paragraphs) <= MAX_SITES
    sections = body.findall(w("sectPr"))
    assert len(sections) <= 1 and (not sections or list(body)[-1] is sections[0])
    settings = package.related("settings")
    settings_root = package.xml(settings) if settings else None
    themes = {name: opc.digest(raw) for name, raw in package.parts.items() if "/theme/" in name}
    auxiliary = {name: opc.digest(raw) for name, raw in package.parts.items() if name.startswith("customXml/") or "readmd" in name.lower()}
    return {"kind": "math-native-runs", "documentPart": package.document, "stylesPart": part,
            "paragraphs": paragraphs, "styles": context.description(), "ordinaryRuns": ordinary,
            "bodyChildren": [canonical(node) for node in body], "sectPr": canonical(sections[0]) if sections else None,
            "settingsTree": canonical(settings_root), "mathPr": canonical(one(settings_root, m("mathPr"))),
            "themeParts": themes, "auxiliaryParts": auxiliary,
            "parts": {name: opc.digest(raw) for name, raw in package.parts.items()},
            "sourcePayloadUsedAsNativeEvidence": False, "wordApplicationCertified": False,
            "fullWordStylePackageCertified": False}


def inspect_fragment(raw):
    raw = raw.encode() if isinstance(raw, str) else raw
    root = parse_xml((f'<qa xmlns:w="{W}" xmlns:m="{M}">'.encode() + raw + b"</qa>"))
    styles, pr = one(root, w("styles")), one(root, w("pPr"))
    context = Context(styles, "source-fragment" if styles is not None else None)
    objects = root.findall(m("oMath")) + [obj for group in root.findall(m("oMathPara")) for obj in group.findall(m("oMath"))]
    assert objects, "Fragment needs actual OfficeMath"
    return {"kind": "math-native-runs-fragment", "independentXmlReparsed": True,
            "styles": context.description(), "paragraphs": [inspect_math(context, objects, pr, 0, 0)],
            "mathPr": canonical(one(root, m("mathPr"))),
            "sourcePayloadUsedAsNativeEvidence": False}


def fixture_parts(kind, variant="normal"):
    assert kind in KINDS and variant in VARIANTS
    # All source formatting is ordinary native Word/OMML, with no carrier.
    parts = helpers.unit.base.fixture_parts("empty-zero")
    package = Package(parts=parts)
    font = '<w:rFonts w:ascii="STIX Two Math" w:hAnsi="STIX Two Math" w:eastAsia="STIX Two Math" w:cs="STIX Two Math"/>'
    def run_pr(extra=""):
        return '<w:rPr><w:rStyle w:val="Accent"/>' + font + extra + '</w:rPr>'
    full = '<w:b w:val="0"/><w:i w:val="0"/><w:color w:val="262626"/><w:sz w:val="24"/>'
    mixed = '<w:b w:val="off"/><w:color w:val="AA5500"/><w:sz w:val="24"/>'
    formulas = []
    for index in range(3):
        control = full if index == 2 else ('<w:i w:val="0"/>' if index == 1 else "")
        numerator = full if index == 2 else (mixed if index == 1 else "")
        denominator = full if index == 2 else ""
        formulas.append('<m:oMathPara><m:oMath><m:f><m:fPr><m:ctrlPr>' + run_pr(control)
                        + '</m:ctrlPr></m:fPr><m:num><m:r>' + run_pr(numerator) + '<m:t>a</m:t></m:r></m:num>'
                        + '<m:den><m:r>' + run_pr(denominator) + '<m:t>b</m:t></m:r></m:den></m:f></m:oMath></m:oMathPara>')
    body = []
    for index, formula in enumerate(formulas):
        body.append(f'<w:p><w:pPr><w:pStyle w:val="NativeRunBody"/></w:pPr><w:r><w:t>NATIVE_RUN_{kind.upper()}_BEFORE_{index} KEEP_ORDINARY</w:t></w:r></w:p>')
        body.append('<w:p><w:pPr><w:pStyle w:val="Equation"/></w:pPr>' + formula + '</w:p>')
    body.append(f'<w:p><w:pPr><w:pStyle w:val="NativeRunBody"/></w:pPr><w:r><w:t>NATIVE_RUN_{kind.upper()}_SUFFIX KEEP_ORDINARY</w:t></w:r></w:p>')
    document = f'<w:document xmlns:w="{W}" xmlns:m="{M}"><w:body>{"".join(body)}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>'
    # Both inputs include two true style toggles, a false style no-op, and
    # different raw b/i definitions in the same IDs. No type/name grants trust.
    base_bi = '<w:b/><w:i/>' if kind == "a" else '<w:b/><w:i w:val="0"/>'
    equation_bi = '<w:b w:val="true"/><w:i w:val="false"/>' if kind == "a" else '<w:b w:val="false"/><w:i/>'
    character_bi = '<w:b/><w:i/>' if kind == "a" else '<w:b/><w:i w:val="false"/>'
    accent_bi = '<w:b w:val="0"/><w:i/>' if kind == "a" else '<w:b/><w:i/>'
    color, size = ("336699", "30") if kind == "a" else ("993366", "26")
    styles = f'''<w:styles xmlns:w="{W}"><w:docDefaults><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:jc w:val="left"/></w:pPr><w:rPr>{base_bi}<w:color w:val="112233"/><w:sz w:val="24"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="Base"/><w:pPr><w:ind w:left="0" w:right="0"/></w:pPr><w:rPr>{equation_bi}</w:rPr></w:style>
<w:style w:type="character" w:styleId="CharBase"><w:rPr>{character_bi}<w:color w:val="224466"/><w:sz w:val="24"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="Accent"><w:basedOn w:val="CharBase"/><w:rPr>{accent_bi}<w:color w:val="{color}"/><w:sz w:val="{size}"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="NativeRunBody"><w:pPr><w:ind w:left="0" w:right="0"/><w:jc w:val="left"/></w:pPr><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:color w:val="262626"/><w:sz w:val="24"/></w:rPr></w:style></w:styles>'''
    parts[package.document], parts[package.related("styles")] = document.encode(), styles.encode()
    settings = f'<w:settings xmlns:w="{W}" xmlns:m="{M}"><m:mathPr><m:dispDef m:val="0"/><m:preSp m:val="0"/><m:postSp m:val="0"/><m:interSp m:val="0"/><m:intraSp m:val="0"/></m:mathPr></w:settings>'
    parts[package.related("settings")] = settings.encode()
    if variant != "normal":
        style_root = parse_xml(parts[package.related("styles")])
        by_id = {node.get(w("styleId")): node for node in style_root.findall(w("style"))}
        accent = by_id["Accent"]
        if variant == "defaults":
            defaults = one(style_root, w("docDefaults"))
            defaults.insert(0, parse_xml(f'<w:rPrDefault xmlns:w="{W}"><w:rPr><w:b/><w:i/><w:color w:val="556677"/><w:sz w:val="28"/></w:rPr></w:rPrDefault>'.encode()))
        elif variant == "theme":
            node = one(one(accent, w("rPr")), w("color"))
            node.set(w("themeColor"), "accent1")
            # The theme-dependent source really supplies the referenced slot.
            drawing = "http://schemas.openxmlformats.org/drawingml/2006/main"
            solid = '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
            formats = '<a:fmtScheme name="Owned"><a:fillStyleLst>' + solid * 3 + '</a:fillStyleLst><a:lnStyleLst>' + ('<a:ln w="9525">' + solid + '<a:prstDash val="solid"/></a:ln>') * 3 + '</a:lnStyleLst><a:effectStyleLst>' + '<a:effectStyle><a:effectLst/></a:effectStyle>' * 3 + '</a:effectStyleLst><a:bgFillStyleLst>' + solid * 3 + '</a:bgFillStyleLst></a:fmtScheme>'
            theme = f'<a:theme xmlns:a="{drawing}" name="Owned native run theme"><a:themeElements><a:clrScheme name="Owned">' + "".join(f'<a:{name}><a:srgbClr val="{value}"/></a:{name}>' for name, value in [("dk1","000000"),("lt1","FFFFFF"),("dk2","222222"),("lt2","EEEEEE"),("accent1","336699"),("accent2","993366"),("accent3","117733"),("accent4","775511"),("accent5","551177"),("accent6","337755"),("hlink","0000FF"),("folHlink","800080")]) + '</a:clrScheme><a:fontScheme name="Owned"><a:majorFont><a:latin typeface="STIX Two Math"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Arial"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>' + formats + '</a:themeElements></a:theme>'
            parts["word/theme/native-runs.xml"] = theme.encode()
            rels_part = opc.rel_part(package.document)
            rels = package.xml(rels_part)
            ET.SubElement(rels, "{" + opc.P + "}Relationship",
                          {"Id": "nativeRunTheme", "Type": opc.R + "/theme", "Target": "theme/native-runs.xml"})
            parts[rels_part] = opc.xml_bytes(rels)
            types = package.xml("[Content_Types].xml")
            ET.SubElement(types, "{http://schemas.openxmlformats.org/package/2006/content-types}Override",
                          {"PartName": "/word/theme/native-runs.xml", "ContentType": "application/vnd.openxmlformats-officedocument.theme+xml"})
            parts["[Content_Types].xml"] = opc.xml_bytes(types)
        elif variant == "auto":
            one(one(accent, w("rPr")), w("color")).set(w("val"), "auto")
        elif variant == "size-range":
            one(one(accent, w("rPr")), w("sz")).set(w("val"), "3277")
        elif variant == "wrong-type":
            by_id["CharBase"].set(w("type"), "paragraph")
        elif variant == "missing-parent":
            one(accent, w("basedOn")).set(w("val"), "Missing")
        elif variant == "cycle":
            by_id["CharBase"].insert(0, ET.Element(w("basedOn"), {w("val"): "Accent"}))
        elif variant == "duplicate-id":
            style_root.append(copy.deepcopy(accent))
        parts[package.related("styles")] = opc.xml_bytes(style_root)
    assert not any("readmd" in name.lower() or name.startswith("customXml/") for name in parts)
    assert not any(b"readmd:" in raw.lower() for raw in parts.values())
    return parts


def eligible_source(inspection):
    assert inspection["paragraphs"], "Native source needs math paragraphs"
    assert not inspection["auxiliaryParts"], "Source metadata cannot prove native runs"
    for row in inspection["paragraphs"]:
        assert row["paragraphChain"], "Explicit native paragraph chain required"
        assert row["sites"] and all(site["nativeEligible"] for site in row["sites"]), "Source is outside native run-source admission"


def mapped_tree(tree, mapping):
    result = copy.deepcopy(tree)
    if result is None:
        return None
    if result["name"] in (w("pStyle"), w("rStyle"), w("basedOn")):
        identity = result["attributes"].get(w("val"))
        assert identity in mapping, "Unproved style reference"
        result["attributes"][w("val")] = mapping[identity]
    result["children"] = [mapped_tree(child, mapping) for child in result["children"]]
    return result


def bind_chain(source, result, old_ids, new_ids, kind, label, aliases, owners):
    assert len(old_ids) == len(new_ids) > 0, "Native style chain length changed"
    local_map, signatures = {}, []
    for old_id, new_id in zip(old_ids, new_ids):
        old, new = source["styles"]["definitions"][old_id], result["styles"]["definitions"][new_id]
        assert old["type"] == new["type"] == kind, "Native style type changed"
        assert old["pPr"] == new["pPr"], "Native style sparse pPr changed"
        assert old["rPr"]["tree"] == new["rPr"]["tree"], "Native style sparse four-axis rPr changed"
        assert not new["default"], "Imported style became a package global default"
        key = label + ":" + old_id
        assert key not in aliases or aliases[key] == new_id, "One source style acquired inconsistent aliases"
        aliases[key], local_map[old_id] = new_id, new_id
        signatures.append({"type": kind, "pPr": old["pPr"], "rPr": old["rPr"]["tree"]})
        closed_signature = json.dumps(signatures, sort_keys=True, separators=(",", ":"))
        assert new_id not in owners or owners[new_id] == closed_signature, "Different source definitions merged into one alias"
        owners[new_id] = closed_signature
    for old_id, new_id in zip(old_ids, new_ids):
        old, new = source["styles"]["definitions"][old_id], result["styles"]["definitions"][new_id]
        assert new["basedOn"] == (local_map[old["basedOn"]] if old["basedOn"] else None), "Native basedOn edge changed"
    return local_map


def root_compatibility(source, result, before, after):
    proof = {}
    for axis in AXES:
        direct = before["direct"]["axes"][axis]
        if direct["state"] != "missing":
            mode = "absolute-direct"
        elif axis in ("sz", "color") and any(event["axis"] == axis and event["origin"] != "docDefaults" and event["origin"] != "direct" for event in before["effective"]["trace"]):
            mode = "absolute-style"
        else:
            old = value_of(axis, source["styles"]["runDefaults"]["axes"][axis])
            new = value_of(axis, result["styles"]["runDefaults"]["axes"][axis])
            if axis in ("b", "i"):
                old, new = (False if old is None else old), (False if new is None else new)
            assert old == new, "Uncovered root run defaults differ"
            assert old is not None, "Missing size/color cannot prove a consumer basis"
            mode = "equal-root-basis"
        proof[axis] = {"mode": mode, "sourceDeclaration": source["styles"]["runDefaults"]["axes"][axis],
                       "nativeDeclaration": result["styles"]["runDefaults"]["axes"][axis]}
    return proof


def compare_rows(source, result, rows, label, aliases=None, owners=None):
    eligible_source(source)
    aliases, owners = ({} if aliases is None else aliases), ({} if owners is None else owners)
    assert len(source["paragraphs"]) == len(rows)
    bindings = []
    for before, after in zip(source["paragraphs"], rows):
        mapping = bind_chain(source, result, before["paragraphChain"], after["paragraphChain"],
                             "paragraph", label, aliases, owners)
        assert len(before["sites"]) == len(after["sites"]) and before["sites"], "Native run/control inventory changed"
        for left, right in zip(before["sites"], after["sites"]):
            assert (left["kind"], left["path"], left["text"]) == (right["kind"], right["path"], right["text"]), "Native run/control identity changed"
            mapping.update(bind_chain(source, result, left["characterChain"], right["characterChain"],
                                      "character", label, aliases, owners))
        assert after["pPr"] == mapped_tree(before["pPr"], mapping), "Native sparse direct pPr changed"
        assert after["mathShape"] == before["mathShape"], "Native math structure/semantics changed"
        site_bindings = []
        for left, right in zip(before["sites"], after["sites"]):
            assert right["direct"]["tree"] == mapped_tree(left["direct"]["tree"], mapping), "Inherited runs were direct-materialized or raw declarations changed"
            assert right["direct"]["otherProperties"] == left["direct"]["otherProperties"], "Non-axis direct run properties changed"
            assert right["effective"]["axes"] == left["effective"]["axes"], "Native four-axis effective cascade changed"
            for axis, origin in left["effective"]["origins"].items():
                if origin.startswith(("paragraph:", "character:")):
                    category, identity = origin.split(":", 1)
                    origin = category + ":" + mapping[identity]
                assert right["effective"]["origins"][axis] == origin, "Native effective source origin changed"
            site_bindings.append({"ordinal": left["ordinal"], "kind": left["kind"], "path": left["path"],
                                  "sourceCharacterStyle": left["direct"]["rStyle"],
                                  "nativeCharacterStyle": right["direct"]["rStyle"],
                                  "axes": left["effective"]["axes"], "sourceTrace": left["effective"]["trace"],
                                  "nativeTrace": right["effective"]["trace"],
                                  "rootCompatibility": root_compatibility(source, result, left, right),
                                  "sparseDirectPreserved": True})
        bindings.append({"ordinal": before["ordinal"], "sourceParagraphStyle": before["paragraphStyle"],
                         "nativeParagraphStyle": after["paragraphStyle"], "sites": site_bindings})
    return {"passed": True, "aliasMap": aliases, "bindings": bindings,
            "nativeRunSourceCertified": True, "fullWordStylePackageCertified": False,
            "wordApplicationCertified": False}


def compare_native(source_file, native_file, kind):
    source, result = inspect_package(Package(source_file)), inspect_package(Package(native_file))
    proof = compare_rows(source, result, result["paragraphs"], kind)
    assert not result["auxiliaryParts"], "Internal metadata part leaked into current native output"
    proof.update({"sourceSha256": opc.digest(source_file.read_bytes()), "nativeSha256": opc.digest(native_file.read_bytes())})
    return proof


def compare_combined(a_file, b_file, native_file, reverse=False, opposite=None):
    inputs = [("a", inspect_package(Package(a_file))), ("b", inspect_package(Package(b_file)))]
    if reverse:
        inputs.reverse()
    result, aliases, owners, bindings, offset = inspect_package(Package(native_file)), {}, {}, [], 0
    for label, source in inputs:
        count = len(source["paragraphs"])
        proof = compare_rows(source, result, result["paragraphs"][offset:offset + count], label, aliases, owners)
        bindings.extend(proof["bindings"])
        offset += count
    assert offset == len(result["paragraphs"]) and not result["auxiliaryParts"]
    for identity in ("Base", "Equation", "CharBase", "Accent"):
        assert aliases["a:" + identity] != aliases["b:" + identity], "Distinct A/B definitions shared an alias"
    if opposite is not None:
        other = compare_combined(a_file, b_file, opposite, not reverse)
        assert other["aliasMap"] == aliases, "Source order changed native aliases"
    return {"passed": True, "aliasMap": aliases, "bindings": bindings, "reverse": reverse,
            "sourceOrder": [label for label, _ in inputs], "orderStabilityChecked": opposite is not None,
            "nativeSha256": opc.digest(native_file.read_bytes()), "nativeRunSourceCertified": True,
            "fullWordStylePackageCertified": False, "wordApplicationCertified": False}


def compare_isolation(baseline_file, mixed_file, source_file, b_file=None, reverse=False):
    baseline, mixed = inspect_package(Package(baseline_file)), inspect_package(Package(mixed_file))
    inputs = [("a", inspect_package(Package(source_file)))]
    if b_file is not None:
        inputs.append(("b", inspect_package(Package(b_file))))
    if reverse:
        inputs.reverse()
    assert baseline["sectPr"] is not None and mixed["sectPr"] == baseline["sectPr"], "Ordinary final section changed"
    assert mixed["bodyChildren"][:len(baseline["bodyChildren"]) - 1] == baseline["bodyChildren"][:-1], "Ordinary complete body/bare formula changed"
    assert mixed["styles"]["docDefaultsTree"] == baseline["styles"]["docDefaultsTree"], "Ordinary docDefaults tree changed"
    assert mixed["settingsTree"] == baseline["settingsTree"], "Ordinary settings changed"
    assert mixed["themeParts"] == baseline["themeParts"], "Ordinary theme bytes changed"
    assert mixed["styles"]["paragraphDefault"] == baseline["styles"]["paragraphDefault"]
    assert mixed["styles"]["characterDefault"] == baseline["styles"]["characterDefault"]
    for identity, definition in baseline["styles"]["definitions"].items():
        assert mixed["styles"]["definitions"].get(identity) == definition, "Template style definition changed"
    assert mixed["paragraphs"][:len(baseline["paragraphs"])] == baseline["paragraphs"], "Bare formula direct/effective run source changed"
    assert mixed["ordinaryRuns"][:len(baseline["ordinaryRuns"])] == baseline["ordinaryRuns"], "Ordinary effective runs changed"
    offset, aliases, owners, bindings = len(baseline["paragraphs"]), {}, {}, []
    for label, source in inputs:
        count = len(source["paragraphs"])
        proof = compare_rows(source, mixed, mixed["paragraphs"][offset:offset + count], label, aliases, owners)
        bindings.extend(proof["bindings"])
        offset += count
    assert offset == len(mixed["paragraphs"])
    old_ids, new_ids = set(baseline["styles"]["definitions"]), set(mixed["styles"]["definitions"])
    assert new_ids - old_ids == set(aliases.values()) - old_ids, "Unused or unproved imported style definitions"
    assert not mixed["auxiliaryParts"], "Source metadata leaked into native OPC"
    return {"passed": True, "aliasMap": aliases, "bindings": bindings,
            "ordinaryCompleteBodyAndEffectiveRunsEqual": True, "ordinaryBareMathEqual": True,
            "templateDefinitionsEqual": True, "completeDocDefaultsEqual": True,
            "settingsEqual": True, "themeBytesEqual": True, "finalSectPrEqual": True,
            "wordApplicationCertified": False}


def tree_child(tree, tag):
    found = [child for child in tree["children"] if child["name"] == tag]
    assert len(found) == 1, "Expected one native tree particle " + tag
    return found[0]


def native_group(inspection, style_id=None):
    groups = {}
    for row in inspection["paragraphs"]:
        sites = row["sites"]
        if [site["kind"] for site in sites] != ["control", "run", "run"] or [site["text"] for site in sites] != ["", "a", "b"]:
            continue
        assert len(row["mathShape"]) == 1
        shape = row["mathShape"][0]
        assert shape["name"] == m("oMath") and len(shape["children"]) == 1 and shape["children"][0]["name"] == m("f")
        refs = {site["direct"]["rStyle"] for site in sites}
        if refs == {None}:
            continue  # A real unassociated bare fraction is not this source.
        assert len(refs) == 1 and None not in refs, "Native group needs real character references"
        reference = next(iter(refs))
        groups.setdefault(reference, []).append(row)
    if style_id is None:
        assert len(groups) == 1, "Ambiguous actual native character group; specify --style-id"
        style_id = next(iter(groups))
    assert style_id in groups and len(groups[style_id]) == 3, "Expected three native fractions using the actual character style"
    assert inspection["styles"]["definitions"][style_id]["type"] == "character"
    return style_id, groups[style_id]


def mutate_current(input_file, output_file, operation, value=None, style_id=None):
    assert operation in ("style-color", "style-size", "delete-direct-b")
    input_sha = opc.digest(input_file.read_bytes())
    package = Package(input_file)
    before_parts, parts = package.parts, dict(package.parts)
    before = inspect_package(package)
    selected, group = native_group(before, style_id)
    if operation == "delete-direct-b":
        assert value is None
        member = package.document
        document = package.xml(member)
        paragraph = list(one(document, w("body")))[group[1]["bodyIndex"]]
        obj = one(one(paragraph, m("oMathPara")), m("oMath"))
        run = one(one(one(obj, m("f")), m("num")), m("r"))
        pr = one(run, w("rPr"))
        switch = one(pr, w("b"))
        assert switch is not None and not boolean(switch.get(w("val"))), "Second native numerator must have direct bold off"
        pr.remove(switch)
        parts[member] = opc.xml_bytes(document)
    else:
        member = package.related("styles", optional=False)
        styles = package.xml(member)
        matches = [node for node in styles.findall(w("style")) if node.get(w("styleId")) == selected]
        assert len(matches) == 1
        axis = "color" if operation == "style-color" else "sz"
        target = one(one(matches[0], w("rPr")), w(axis))
        assert target is not None and set(target.attrib) == {w("val")}, "Current leaf must directly declare the mutated axis"
        value = value or ("117733" if axis == "color" else "36")
        if axis == "color":
            assert re.fullmatch(r"[0-9a-fA-F]{6}", value)
        else:
            assert size_number(value)[0] is not None
        assert target.get(w("val")) != value, "Mutation must actually change a value"
        target.set(w("val"), value)
        parts[member] = opc.xml_bytes(styles)
    after = inspect_package(Package(parts=parts))
    assert set(parts) == set(before_parts)
    changed = [name for name in parts if parts[name] != before_parts[name]]
    assert changed == [member], "Current mutation changed another OPC member"
    assert after["ordinaryRuns"] == before["ordinaryRuns"], "Current style mutation affected ordinary effective content"
    assert after["sectPr"] == before["sectPr"]
    changed_sites, shadowed_sites = [], []
    if operation == "delete-direct-b":
        assert after["styles"] == before["styles"]
        expected = copy.deepcopy(before["bodyChildren"])
        paragraph = expected[group[1]["bodyIndex"]]
        target = tree_child(tree_child(tree_child(tree_child(tree_child(tree_child(paragraph, m("oMathPara")), m("oMath")), m("f")), m("num")), m("r")), w("rPr"))
        old = tree_child(target, w("b"))
        target["children"].remove(old)
        assert after["bodyChildren"] == expected, "Mutation changed more than the precise native direct bold node"
        for row in before["paragraphs"]:
            current = after["paragraphs"][row["ordinal"]]
            if row["ordinal"] != group[1]["ordinal"]:
                assert current == row
        old, new = group[1]["sites"][1], after["paragraphs"][group[1]["ordinal"]]["sites"][1]
        assert old["effective"]["axes"]["b"] is False and new["effective"]["axes"]["b"] is True, "Deleting direct off did not restore the actual style toggle"
        for axis in ("i", "sz", "color"):
            assert new["effective"]["axes"][axis] == old["effective"]["axes"][axis]
        changed_sites.append({"paragraph": group[1]["ordinal"], "site": 1, "axis": "b", "before": False, "after": True})
    else:
        assert after["bodyChildren"] == before["bodyChildren"], "Style-only mutation changed document XML semantics"
        expected_defs = copy.deepcopy(before["styles"]["definitions"])
        axis = "color" if operation == "style-color" else "sz"
        expected_tree = expected_defs[selected]["tree"]
        tree_child(tree_child(expected_tree, w("rPr")), w(axis))["attributes"][w("val")] = value
        # The declaration mirror is derived from the same modified native XML,
        # so compare actual definition trees and every unrelated definition.
        for identity, old in before["styles"]["definitions"].items():
            new = after["styles"]["definitions"][identity]
            if identity == selected:
                assert new["tree"] == expected_tree
            else:
                assert new == old, "Another native definition changed"
        assert after["styles"]["docDefaultsTree"] == before["styles"]["docDefaultsTree"]
        group_ordinals = {row["ordinal"] for row in group}
        for old_row, new_row in zip(before["paragraphs"], after["paragraphs"]):
            assert old_row["mathShape"] == new_row["mathShape"]
            for old, new in zip(old_row["sites"], new_row["sites"]):
                assert old["direct"] == new["direct"]
                before_axes, after_axes = old["effective"]["axes"], new["effective"]["axes"]
                if old_row["ordinal"] in group_ordinals and old["direct"]["axes"][axis]["state"] == "missing":
                    assert before_axes[axis] != after_axes[axis] and after_axes[axis] == value, "Real inheritor did not follow current native style edit"
                    changed_sites.append({"paragraph": old_row["ordinal"], "site": old["ordinal"], "axis": axis,
                                          "before": before_axes[axis], "after": after_axes[axis]})
                else:
                    assert before_axes[axis] == after_axes[axis], "A direct-shadowed or non-target site changed"
                    shadowed_sites.append({"paragraph": old_row["ordinal"], "site": old["ordinal"], "axis": axis})
                for other in AXES:
                    if other != axis:
                        assert before_axes[other] == after_axes[other], "Style mutation changed a different effective axis"
        assert changed_sites and shadowed_sites
    opc.write_package(output_file, parts)
    assert Package(output_file).parts == parts, "Mutated OPC readback differs"
    assert opc.digest(input_file.read_bytes()) == input_sha, "Mutator rewrote its original input"
    return {"passed": True, "operation": operation, "selectedActualCharacterStyle": selected,
            "inputSha256": input_sha, "outputSha256": opc.digest(output_file.read_bytes()),
            "changedMembers": changed, "allOtherMembersByteIdentical": True, "addedMembers": [],
            "changedSites": changed_sites, "shadowedSites": shadowed_sites, "ordinaryEffectiveUnchanged": True,
            "before": before, "inspection": after, "sourcePayloadUsedAsNativeEvidence": False,
            "wordApplicationCertified": False}


def paragraph_control(context, direct):
    _, chain = paragraph_selection(context, direct)
    defaults = one(one(one(context.root, w("docDefaults")), w("pPrDefault")), w("pPr"))
    layers = [defaults] + [one(context.nodes[row["id"]], w("pPr")) for row in chain] + [direct]
    values = {}
    for pr in layers:
        if pr is None:
            continue
        assert not pr.attrib
        for node in pr:
            assert node.tag in (w("pStyle"), w("ind"), w("jc")), "Unsupported paragraph property in independent paint control"
            if node.tag == w("ind"):
                assert set(node.attrib) <= {w("left"), w("right")} and not len(node)
                for axis in ("left", "right"):
                    if w(axis) in node.attrib:
                        raw = node.get(w(axis))
                        measure = helpers.unit.measure(raw)
                        assert measure["inPhysicalDomain"] and measure["exactConsumerRepresentable"]
                        values[axis] = raw
            elif node.tag == w("jc"):
                assert set(node.attrib) == {w("val")} and not len(node)
                assert node.get(w("val")) in ("left", "center", "right"), "Finite control requires physical LTR alignment"
                values["jc"] = node.get(w("val"))
    if not values:
        return None
    result = ET.Element(w("pPr"))
    if "left" in values or "right" in values:
        ET.SubElement(result, w("ind"), {w(axis): values[axis] for axis in ("left", "right") if axis in values})
    if "jc" in values:
        ET.SubElement(result, w("jc"), {w("val"): values["jc"]})
    return result


def finite_other_effective(context, paragraph_chain, direct):
    """Finite independent non-axis support for same-context paint controls.

    Fonts/languages merge attributes; font direct/theme pairs replace lower
    pairs together. szCs is absolute. Other inherited properties still need
    an identical covering direct declaration; no unknown source is discarded.
    """
    characters = context.chain(direct["rStyle"], "character") if direct["rStylePresent"] else context.chain(context.character_default, "character") if context.character_default else []
    layers = [context.run_defaults] + [row["rPr"] for row in paragraph_chain + characters] + [direct]
    pairs = (("ascii", "asciiTheme"), ("hAnsi", "hAnsiTheme"),
             ("eastAsia", "eastAsiaTheme"), ("cs", "cstheme"))
    font_fields = {name for pair in pairs for name in pair} | {"hint"}
    direct_others = {entry["name"]: entry for entry in direct["otherProperties"]}
    result = {}
    for layer_index, layer in enumerate(layers):
        for entry in layer["otherProperties"]:
            tag = entry["name"]
            if tag in (w("rFonts"), w("lang")):
                assert not entry["children"] and not entry["text"], "Non-leaf finite font/language control"
                allowed = font_fields if tag == w("rFonts") else {"val", "eastAsia", "bidi"}
                assert all(key.startswith("{" + W + "}") and opc.local(key) in allowed
                           and isinstance(value, str) and 0 < len(value) <= 1024
                           for key, value in entry["attributes"].items()), "Unproved font/language attributes"
                merged = copy.deepcopy(result.get(tag, entry))
                if tag == w("rFonts"):
                    for literal, theme in pairs:
                        if w(literal) in entry["attributes"] or w(theme) in entry["attributes"]:
                            merged["attributes"].pop(w(literal), None)
                            merged["attributes"].pop(w(theme), None)
                    if w("hint") in entry["attributes"]:
                        assert entry["attributes"][w("hint")] in ("default", "eastAsia", "cs")
                merged["attributes"].update(entry["attributes"])
                result[tag] = merged
            elif tag == w("szCs"):
                assert not entry["children"] and not entry["text"] and set(entry["attributes"]) == {w("val")}
                assert size_number(entry["attributes"][w("val")])[0] is not None, "Unproved complex-script size in control"
                result[tag] = copy.deepcopy(entry)
            else:
                assert layer_index == len(layers) - 1 or direct_others.get(tag) == entry, "Uncovered non-axis inherited property in paint control"
                result[tag] = copy.deepcopy(entry)
    fonts = result.get(w("rFonts"))
    if fonts is not None:
        assert not any(w(theme) in fonts["attributes"] for _, theme in pairs), "Unresolved theme font remains in independent control"
    return result


def from_canonical(tree):
    node = ET.Element(tree["name"], tree["attributes"])
    node.text = tree["text"]
    node[:] = [from_canonical(child) for child in tree["children"]]
    return node


def materialize_control(input_file, output_file):
    original = input_file.read_bytes()
    before = inspect_fragment(original)
    root = parse_xml(f'<qa xmlns:w="{W}" xmlns:m="{M}">'.encode() + original + b"</qa>")
    context = Context(one(root, w("styles")), "source-control")
    direct_ppr = one(root, w("pPr"))
    _, paragraph_chain = paragraph_selection(context, direct_ppr)
    objects = root.findall(m("oMath")) + [obj for group in root.findall(m("oMathPara")) for obj in group.findall(m("oMath"))]
    sites = [node for obj in objects for node in obj.iter() if node.tag in (m("r"), m("ctrlPr"))]
    before_sites = before["paragraphs"][0]["sites"]
    assert len(sites) == len(before_sites)
    order = ("rFonts", "b", "bCs", "i", "iCs", "caps", "smallCaps", "strike", "dstrike",
             "outline", "shadow", "emboss", "imprint", "noProof", "snapToGrid", "vanish",
             "webHidden", "color", "spacing", "w", "kern", "position", "sz", "szCs",
             "highlight", "u", "effect", "bdr", "shd", "fitText", "vertAlign", "rtl",
             "cs", "em", "lang", "eastAsianLayout", "specVanish", "oMath", "rPrChange")
    expected_others = []
    for node, site in zip(sites, before_sites):
        assert all(site["effective"]["axes"][axis] is not None for axis in AXES), "Independent control needs finite effective values"
        assert not any(reason != "source-root-run-defaults" and reason != "no-explicit-character-reference" for reason in site["lossReasons"]), "Unsupported independent control source"
        pr = one(node, w("rPr"))
        direct = declaration(pr)
        extras = finite_other_effective(context, paragraph_chain, direct)
        expected_others.append(extras)
        if pr is None:
            pr = ET.Element(w("rPr"))
            node.insert(0, pr)
        pr[:] = [child for child in pr if child.tag not in {w(axis) for axis in AXES} | {w("rStyle"), w("rFonts"), w("lang"), w("szCs")}]
        for tag in (w("rFonts"), w("szCs"), w("lang")):
            if tag in extras:
                entry = from_canonical(extras[tag])
                rank = order.index(opc.local(tag))
                position = next((index for index, other in enumerate(pr)
                                 if other.tag.startswith("{" + W + "}") and opc.local(other.tag) in order and order.index(opc.local(other.tag)) > rank), len(pr))
                pr.insert(position, entry)
        for axis in AXES:
            value = site["effective"]["axes"][axis]
            value = ("1" if value else "0") if axis in ("b", "i") else value
            child = ET.Element(w(axis), {w("val"): value})
            rank = order.index(axis)
            position = next((index for index, other in enumerate(pr)
                             if other.tag.startswith("{" + W + "}") and opc.local(other.tag) in order and order.index(opc.local(other.tag)) > rank), len(pr))
            pr.insert(position, child)
    drawing = "http://schemas.openxmlformats.org/drawingml/2006/main"
    context_tags = {w("styles"), w("pPr"), w("clrSchemeMapping"), w("themeFontLang"),
                    "{" + drawing + "}clrScheme", "{" + drawing + "}fontScheme"}
    root[:] = [node for node in root if node.tag not in context_tags]
    ppr = paragraph_control(context, direct_ppr)
    if ppr is not None:
        root.insert(0, ppr)
    assert not any(node.tag == w("rStyle") for node in root.iter()), "Control retained a character style reference"
    output = b"".join(ET.tostring(node, encoding="utf-8") for node in root)
    after = inspect_fragment(output)
    assert before["mathPr"] == after["mathPr"], "Independent control changed math defaults"
    assert before["paragraphs"][0]["mathShape"] == after["paragraphs"][0]["mathShape"], "Independent control changed math semantics"
    assert len(after["paragraphs"][0]["sites"]) == len(before_sites)
    after_context = Context()
    for old, new, extras in zip(before_sites, after["paragraphs"][0]["sites"], expected_others):
        assert old["effective"]["axes"] == new["effective"]["axes"], "Independent control changed effective run values"
        assert finite_other_effective(after_context, [], new["direct"]) == extras, "Independent control changed effective non-axis run values"
    assert output_file.parent.is_dir() and not output_file.exists() and not output_file.is_symlink()
    with output_file.open("xb") as stream:
        stream.write(output)
    assert output_file.read_bytes() == output and input_file.read_bytes() == original
    return {"passed": True, "inputSha256": opc.digest(original), "outputSha256": opc.digest(output),
            "independentFourAxisCascade": True, "allExistingRunAndControlSites": len(sites),
            "allSourceStyleReferencesRemoved": True, "otherRunPropertiesIndependentlyMaterialized": True,
            "finiteNonAxisProperties": ["rFonts", "lang", "szCs"],
            "mathSemanticsUnchanged": True, "mathDefaultsUnchanged": True, "paragraphControl": canonical(ppr),
            "inspection": after, "sourceFileUnchanged": True, "wordApplicationCertified": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    fixture = sub.add_parser("fixture")
    fixture.add_argument("--kind", choices=KINDS, required=True)
    fixture.add_argument("--variant", choices=VARIANTS, default="normal")
    fixture.add_argument("--output", type=Path, required=True)
    for action in ("inspect", "fragment"):
        sub.add_parser(action).add_argument("input", type=Path)
    compare = sub.add_parser("compare")
    compare.add_argument("--source", type=Path, required=True)
    compare.add_argument("--native", type=Path, required=True)
    compare.add_argument("--kind", choices=KINDS, required=True)
    combined = sub.add_parser("combined")
    for name in ("a", "b", "native"):
        combined.add_argument("--" + name, type=Path, required=True)
    combined.add_argument("--reverse", action="store_true")
    combined.add_argument("--opposite-order", type=Path)
    isolation = sub.add_parser("isolation")
    for name in ("baseline", "mixed", "source"):
        isolation.add_argument("--" + name, type=Path, required=True)
    isolation.add_argument("--b", type=Path)
    isolation.add_argument("--reverse", action="store_true")
    mutate = sub.add_parser("mutate-current")
    mutate.add_argument("--input", type=Path, required=True)
    mutate.add_argument("--output", type=Path, required=True)
    mutate.add_argument("--operation", choices=("style-color", "style-size", "delete-direct-b"), required=True)
    mutate.add_argument("--value")
    mutate.add_argument("--style-id")
    materialize = sub.add_parser("materialize-control")
    materialize.add_argument("--input", type=Path, required=True)
    materialize.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.action == "fixture":
        parts = fixture_parts(args.kind, args.variant)
        opc.write_package(args.output, parts)
        # Malformed graph fixtures deliberately fail inspection; still record
        # their bytes, without labeling them native source acceptance.
        malformed = args.variant in ("wrong-type", "missing-parent", "cycle", "duplicate-id")
        result = {"kind": args.kind, "variant": args.variant, "nativeOnly": True,
                  "sha256": opc.digest(args.output.read_bytes()),
                  "inspection": None if malformed else inspect_package(Package(args.output)),
                  "intentionallyInvalidGraph": malformed}
    elif args.action == "inspect":
        result = inspect_package(Package(args.input))
    elif args.action == "fragment":
        result = inspect_fragment(args.input.read_bytes())
    elif args.action == "compare":
        result = compare_native(args.source, args.native, args.kind)
    elif args.action == "combined":
        result = compare_combined(args.a, args.b, args.native, args.reverse, args.opposite_order)
    elif args.action == "isolation":
        result = compare_isolation(args.baseline, args.mixed, args.source, args.b, args.reverse)
    elif args.action == "mutate-current":
        result = mutate_current(args.input, args.output, args.operation, args.value, args.style_id)
    else:
        result = materialize_control(args.input, args.output)
    print(json.dumps(result, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
