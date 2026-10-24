"""Independent, bounded OPC settings oracle and owned native fixture/mutations.

Python standard library only.  It neither imports ReadMD code nor starts an
Office application.  Source/provenance parts are hashed, never used to infer
native settings.  Headless OPC edits do not certify Word interoperability.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import sys
from urllib.parse import unquote
import xml.etree.ElementTree as ET
import zipfile

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
P = "http://schemas.openxmlformats.org/package/2006/relationships"
CT = "http://schemas.openxmlformats.org/package/2006/content-types"
STRICT_W = "http://purl.oclc.org/ooxml/wordprocessingml/main"
STRICT_R = "http://purl.oclc.org/ooxml/officeDocument/relationships"
NS = {"w": W, "r": R, "p": P}
MAX_BYTES = 64 * 1024 * 1024
MAX_XML = 16 * 1024 * 1024
FIXTURE_KINDS = ("native-mixed", "native-note-mixed", "native-numbering", "native-zero-start", "native-multi-section", "native-multi-section-safe-ref", "native-heading-ref-safe",
                 "native-caption-mixed", "native-caption-note-mixed", "native-caption-numbering", "native-caption-zero-start",
                 "p06c-native-mixed-three", "p06c-native-format-matrix")
SCHEMA_PATH = ".cache/capability-upgrade/publication-structure-inputs-20261015/p04-tracking-docx-draft-20261016/primary-schema/wordprocessingml-fixed.json"
SCHEMA_SHA256 = "05a5be64625cd79e756b27606587c3a2868f2e42ade196bfca2d6f647e74fdb5"
for prefix, uri in (("w", W), ("r", R)):
    ET.register_namespace(prefix, uri)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def local(tag):
    return tag.rsplit("}", 1)[-1]


def val(node, name="val"):
    return None if node is None else node.get(f"{{{W}}}{name}")


def q(name):
    return f"{{{W}}}{name}"


def parse_xml(raw):
    assert 0 < len(raw) <= MAX_XML, "XML byte budget"
    assert b"<!DOCTYPE" not in raw.upper() and b"<!ENTITY" not in raw.upper(), "DTD/entity forbidden"
    root = ET.fromstring(raw)
    stack, count = [(root, 1)], 0
    while stack:
        node, depth = stack.pop()
        count += 1
        assert depth <= 128 and count <= 400000, "XML node/depth budget"
        stack.extend((child, depth + 1) for child in node)
    # Prefix choice and transitional/strict namespace do not alter semantics.
    for node in root.iter():
        if node.tag.startswith("{" + STRICT_W + "}"):
            node.tag = "{" + W + "}" + local(node.tag)
        node.attrib = {(key.replace("{" + STRICT_W + "}", "{" + W + "}")
                        .replace("{" + STRICT_R + "}", "{" + R + "}")): value
                       for key, value in node.attrib.items()}
    return root


def member(part, target):
    raw = unquote(target)
    assert raw and not any(char in raw for char in "\\:\x00?#"), "Unsafe internal relationship target"
    joined = posixpath.normpath(raw.lstrip("/") if raw.startswith("/") else posixpath.join(posixpath.dirname(part), raw))
    assert joined not in ("", ".", "..") and not joined.startswith("../"), "Relationship escapes OPC root"
    return joined


def rel_part(part):
    if not part:
        return "_rels/.rels"
    path = PurePosixPath(part)
    return str(path.parent / "_rels" / (path.name + ".rels"))


class Package:
    def __init__(self, path=None, parts=None):
        self.raw = path.read_bytes() if path is not None else None
        if parts is None:
            assert 0 < len(self.raw) <= MAX_BYTES, "ZIP byte budget"
            with zipfile.ZipFile(path) as archive:
                info = archive.infolist()
                names = [item.filename for item in info]
                assert len(info) <= 4096 and len(set(names)) == len(names), "Duplicate/excess OPC members"
                assert sum(item.file_size for item in info) <= MAX_BYTES, "Expanded ZIP byte budget"
                assert archive.testzip() is None, "Invalid CRC"
                self.parts = {item.filename: archive.read(item) for item in info}
        else:
            self.parts = dict(parts)
        for name in self.parts:
            assert not name.startswith("/") and "\\" not in name and ".." not in PurePosixPath(name).parts, "Unsafe OPC member"
        self.cache = {}
        roots = [rel for rel in self.relationships("") if rel["kind"] == "officeDocument"]
        assert len(roots) == 1 and roots[0]["mode"] != "External", "One internal officeDocument required"
        self.document = member("", roots[0]["target"])
        assert self.document in self.parts

    def xml(self, part):
        if part not in self.cache:
            assert part in self.parts, "Missing OPC part: " + part
            self.cache[part] = parse_xml(self.parts[part])
        return self.cache[part]

    def relationships(self, part):
        name = rel_part(part)
        if name not in self.parts:
            return []
        root = self.xml(name)
        assert root.tag == f"{{{P}}}Relationships", "Foreign relationships container"
        result, seen = [], set()
        for node in root:
            assert node.tag == f"{{{P}}}Relationship", "Foreign relationship"
            identity = node.get("Id")
            assert identity and identity not in seen, "Duplicate/missing relationship ID"
            seen.add(identity)
            kind, target, mode = node.get("Type", ""), node.get("Target", ""), node.get("TargetMode", "Internal")
            assert kind and target and mode in ("Internal", "External"), "Invalid relationship"
            result.append({"id": identity, "type": kind, "kind": kind.rsplit("/", 1)[-1], "target": target, "mode": mode})
        return result

    def related(self, kind, optional=True):
        matches = [rel for rel in self.relationships(self.document) if rel["kind"] == kind]
        assert len(matches) <= 1, "Duplicate related " + kind
        if not matches:
            assert optional, "Missing related " + kind
            return None
        assert matches[0]["mode"] != "External", "External " + kind
        part = member(self.document, matches[0]["target"])
        assert part in self.parts, "Missing related payload: " + part
        return part

    def serialized(self):
        parts = dict(self.parts)
        for name, root in self.cache.items():
            # Only callers explicitly replacing a part choose these bytes.
            assert name in parts
        return parts


def visible(node):
    output, stack = [], []
    for child in node.iter():
        if child.tag == q("fldChar"):
            kind = val(child, "fldCharType")
            if kind == "begin":
                stack.append(False)
            elif kind == "separate" and stack:
                stack[-1] = True
            elif kind == "end" and stack:
                stack.pop()
        elif child.tag == q("t") and all(stack):
            output.append(child.text or "")
    assert not stack, "Unclosed native field"
    return "".join(output)


def fields(node):
    result = [{"instruction": val(field, "instr"), "cached": visible(field), "kind": "simple", "depth": 0}
              for field in node.iter(q("fldSimple"))]
    stack = []
    for child in node.iter():
        if child.tag == q("fldChar"):
            kind = val(child, "fldCharType")
            if kind == "begin":
                stack.append({"instruction": "", "cached": "", "kind": "complex", "depth": len(stack), "separated": False})
            elif kind == "separate" and stack:
                stack[-1]["separated"] = True
            elif kind == "end" and stack:
                field = stack.pop()
                field.pop("separated")
                result.append(field)
        elif child.tag == q("instrText") and stack:
            stack[-1]["instruction"] += child.text or ""
        elif child.tag == q("t"):
            for field in stack:
                if field["separated"]:
                    field["cached"] += child.text or ""
    assert not stack, "Unclosed native field"
    return result


def properties(node):
    if node is None:
        return {}
    result = {}
    for child in node:
        if not child.tag.startswith("{" + W + "}"):
            continue
        name = local(child.tag)
        if name == "numPr":
            result[name] = {local(item.tag): val(item) for item in child if item.tag.startswith("{" + W + "}")}
        elif name in ("spacing", "ind", "rFonts"):
            result[name] = {local(key): value for key, value in child.attrib.items() if key.startswith("{" + W + "}")}
        elif name in ("keepLines", "keepNext", "widowControl", "b", "i"):
            result[name] = val(child) not in ("0", "false", "off")
        else:
            result[name] = val(child)
    return result


def merge_props(base, over):
    result = copy.deepcopy(base)
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(result.get(key), dict):
            result[key] = {**result[key], **value}
        else:
            result[key] = value
    return result


class Styles:
    def __init__(self, package):
        self.rows, self.cache = {}, {}
        self.defaults = {"p": {}, "r": {}}
        part = package.related("styles")
        if part is None:
            return
        root = package.xml(part)
        assert root.tag == q("styles"), "Foreign styles root"
        for kind in ("p", "r"):
            self.defaults[kind] = properties(root.find(f"w:docDefaults/w:{kind}PrDefault/w:{kind}Pr", NS))
        for style in root.findall("w:style", NS):
            identity = val(style, "styleId")
            assert identity and identity not in self.rows, "Duplicate style ID"
            self.rows[identity] = {"basedOn": val(style.find("w:basedOn", NS)), "name": val(style.find("w:name", NS)),
                                   "p": properties(style.find("w:pPr", NS)), "r": properties(style.find("w:rPr", NS))}

    def effective(self, identity, kind, trail=()):
        assert len(trail) <= 64 and identity not in trail, "Style inheritance cycle"
        key = (identity, kind)
        if key in self.cache:
            return self.cache[key]
        row = self.rows.get(identity, {})
        base = self.effective(row["basedOn"], kind, trail + (identity,)) if row.get("basedOn") else self.defaults[kind]
        result = merge_props(base, row.get(kind, {}))
        self.cache[key] = result
        return result


def paragraph(node, styles):
    ppr = node.find("w:pPr", NS)
    style = val(ppr.find("w:pStyle", NS)) if ppr is not None else None
    effective = merge_props(styles.effective(style, "p"), properties(ppr))
    runs = []
    for run in node.iter(q("r")):
        texts = [text.text or "" for text in run.findall("w:t", NS)]
        if not texts:
            continue
        rpr = run.find("w:rPr", NS)
        rstyle = val(rpr.find("w:rStyle", NS)) if rpr is not None else None
        props = styles.effective(style, "r")
        if rstyle:
            props = merge_props(props, styles.effective(rstyle, "r"))
        runs.append({"text": "".join(texts), "properties": merge_props(props, properties(rpr))})
    number = effective.get("numPr")
    return {"text": visible(node), "style": style, "properties": effective, "number": number,
            "runs": runs, "fields": fields(node),
            "bookmarks": [{"id": val(mark, "id"), "name": val(mark, "name")} for mark in node.findall("w:bookmarkStart", NS)]}


def note_properties(node):
    if node is None:
        return None
    values, references, unknown, children = {}, [], [], []
    for child in node:
        name = local(child.tag)
        children.append({"name": child.tag, "val": val(child), "format": val(child, "format"),
                         "formatPresent": q("format") in child.attrib})
        if child.tag in (q("footnote"), q("endnote")):
            references.append(val(child, "id"))
        elif child.tag in (q("numFmt"), q("numStart"), q("numRestart"), q("pos")):
            assert name not in values, "Duplicate native note property: " + name
            values[name] = val(child)
        else:
            unknown.append({"name": child.tag, "xml": ET.tostring(child, encoding="unicode")})
    return {"values": values, "specialIds": references, "unknown": unknown, "children": children}


def effective_note_properties(global_props, section_props, kind):
    result = {"numFmt": "lowerRoman" if kind == "endnote" else "decimal", "numStart": "1",
              "numRestart": "continuous", "pos": "docEnd" if kind == "endnote" else "pageBottom", "numberPattern": None}
    for props in (global_props, section_props):
        if props is not None:
            result.update(props["values"])
            fmt = next((child for child in props["children"] if child["name"] == q("numFmt")), None)
            if fmt is not None:
                result["numberPattern"] = fmt["format"]
    return result


def section_scopes(body, settings):
    """Read true top-level native terminators, never source annotations."""
    result, begin = [], 0
    for index, node in enumerate(body):
        if node.tag == q("sectPr"):
            section, terminator, end = node, [index], index
        elif node.tag == q("p"):
            props = node.find("w:pPr", NS)
            section = props.find("w:sectPr", NS) if props is not None else None
            if section is None:
                continue
            terminator, end = [index, list(node).index(props), list(props).index(section)], index + 1
        else:
            continue
        content = list(body)[begin:end]
        notes = {}
        for kind in ("footnote", "endnote"):
            sparse = note_properties(section.find("w:" + kind + "Pr", NS))
            notes[kind] = {"sparse": sparse, "effective": effective_note_properties(note_properties(settings.find("w:" + kind + "Pr", NS)), sparse, kind),
                           "references": [val(ref, "id") for item in content for ref in item.iter(q(kind + "Reference"))]}
        result.append({"ordinal": len(result), "bodyRange": [begin, end], "terminatorPath": terminator,
                       "break": val(section.find("w:type", NS)) or "nextPage", "text": "\n".join(visible(item) for item in content),
                       "bookmarks": [{"name": val(mark, "name"), "id": val(mark, "id")} for item in content for mark in item.iter(q("bookmarkStart"))],
                       "hyperlinks": [val(link, "anchor") for item in content for link in item.iter(q("hyperlink")) if val(link, "anchor")],
                       "notes": notes})
        begin = end
    return result


def inspect_package(package):
    styles = Styles(package)
    document = package.xml(package.document)
    assert document.tag == q("document")
    body = document.find("w:body", NS)
    assert body is not None
    paragraphs = [paragraph(node, styles) for node in body.iter(q("p"))]
    settings_part = package.related("settings")
    settings = package.xml(settings_part) if settings_part else ET.Element(q("settings"))
    assert settings.tag == q("settings")
    sections = list(body.iter(q("sectPr")))
    notes = {}
    for kind in ("footnote", "endnote"):
        references = [val(node, "id") for node in body.iter(q(kind + "Reference"))]
        part = package.related(kind + "s")
        definitions = []
        if part:
            story = package.xml(part)
            assert story.tag == q(kind + "s"), "Wrong note story namespace"
            ids = set()
            for node in story:
                assert node.tag == q(kind), "Foreign note definition"
                identity = val(node, "id")
                assert identity and identity not in ids, "Duplicate native note ID"
                ids.add(identity)
                definitions.append({"id": identity, "type": val(node, "type") or "normal", "text": visible(node),
                                    "paragraphs": [paragraph(p, styles) for p in node.iter(q("p"))],
                                    "separatorElements": [local(n.tag) for n in node.iter() if n.tag in (q("separator"), q("continuationSeparator"))]})
            normal = {row["id"] for row in definitions if row["type"] == "normal"}
            assert all(identity in normal for identity in references), "Unresolved native note reference"
        assert part or not references, "Note references without a related story"
        global_props = note_properties(settings.find("w:" + kind + "Pr", NS))
        section_props = [note_properties(section.find("w:" + kind + "Pr", NS)) for section in sections]
        selected_ids = (global_props or {}).get("specialIds", [])
        selected = [row for row in definitions if row["id"] in selected_ids]
        notes[kind] = {"part": part, "references": references, "settings": global_props, "sections": section_props,
                       "definitions": definitions, "selectedSpecial": selected}
    abstract, numbers = {}, {}
    numbering_part = package.related("numbering")
    if numbering_part:
        numbering = package.xml(numbering_part)
        assert numbering.tag == q("numbering")
        for item in numbering.findall("w:abstractNum", NS):
            identity = val(item, "abstractNumId")
            assert identity not in abstract, "Duplicate abstractNumId"
            abstract[identity] = [{"level": val(level, "ilvl"), "start": val(level.find("w:start", NS)),
                                   "format": val(level.find("w:numFmt", NS)), "text": val(level.find("w:lvlText", NS)),
                                   "suffix": val(level.find("w:suff", NS)), "restart": val(level.find("w:lvlRestart", NS)),
                                   "style": val(level.find("w:pStyle", NS))} for level in item.findall("w:lvl", NS)]
        for item in numbering.findall("w:num", NS):
            identity = val(item, "numId")
            assert identity not in numbers, "Duplicate numId"
            numbers[identity] = {"abstract": val(item.find("w:abstractNumId", NS)),
                                 "overrides": [{"level": val(level, "ilvl"), "start": val(level.find("w:startOverride", NS))}
                                               for level in item.findall("w:lvlOverride", NS)]}
    for row in paragraphs:
        num = row["number"]
        if num and num.get("numId") not in (None, "0"):
            instance = numbers.get(num["numId"])
            assert instance is not None, "Unresolved paragraph numId"
            level = num.get("ilvl", "0")
            rules = [rule for rule in abstract.get(instance["abstract"], []) if rule["level"] == level]
            assert len(rules) == 1, "Unresolved paragraph numbering level"
            row["rule"] = dict(rules[0])
            override = next((value for value in instance["overrides"] if value["level"] == level), None)
            if override and override["start"] is not None:
                row["rule"]["start"] = override["start"]
    aux = {name: digest(raw) for name, raw in package.parts.items() if name.startswith("customXml/")}
    return {"kind": "docx-settings-roundtrip", "reader": "Python stdlib ZIP/XML",
            "bytes": len(package.raw) if package.raw is not None else None,
            "sha256": digest(package.raw) if package.raw is not None else None,
            "documentPart": package.document, "parts": {name: digest(raw) for name, raw in sorted(package.parts.items())},
            "auxiliaryParts": aux, "paragraphs": paragraphs, "abstract": abstract, "numbers": numbers,
            "notes": notes, "styleDefinitions": styles.rows, "nativeSections": len(sections),
            "sectionScopes": section_scopes(body, settings),
            "wordApplicationRoundtripCertified": False, "sourcePayloadUsedAsNativeEvidence": False}


def inspect(path):
    return inspect_package(Package(path=Path(path)))


def inspect_epub_notes(path):
    """Check the real EPUB XHTML callouts, targets and reverse links."""
    raw = Path(path).read_bytes()
    assert 0 < len(raw) <= MAX_BYTES
    with zipfile.ZipFile(path) as archive:
        info = archive.infolist()
        assert len(info) <= 4096 and len({row.filename for row in info}) == len(info)
        assert sum(row.file_size for row in info) <= MAX_BYTES and archive.testzip() is None
        files = {row.filename: parse_xml(re.sub(br"<!DOCTYPE\s+html\s*>", b"", archive.read(row), flags=re.I))
                 for row in info if row.filename.endswith((".xhtml", ".html"))}
    assert files, "EPUB contains no XHTML"
    ids, references, backlinks, notes = {}, [], [], []
    for name, root in files.items():
        for node in root.iter():
            identity = node.get("id")
            if identity:
                assert (name, identity) not in ids, "Duplicate EPUB identity"
                ids[name, identity] = node
            role = node.get("role")
            if role in ("doc-noteref", "doc-backlink"):
                row = {"file": name, "id": identity, "text": "".join(node.itertext()), "href": node.get("href")}
                (references if role == "doc-noteref" else backlinks).append(row)
            if identity and (role == "doc-footnote" or node.get("{http://www.idpf.org/2007/ops}type") in ("footnote", "endnote")):
                notes.append({"file": name, "id": identity, "text": "".join(node.itertext())})
    assert references and notes, "EPUB dropped actual notes or callouts"
    for row in references + backlinks:
        href = row["href"]
        assert href and "#" in href, "Note relationship has no fragment target"
        file, fragment = href.split("#", 1)
        destination = member(row["file"], file) if file else row["file"]
        assert (destination, unquote(fragment)) in ids, "Unresolved EPUB note/backlink target"
        row.update({"targetFile": destination, "targetId": unquote(fragment)})
    for row in references:
        assert any(note["file"] == row["targetFile"] and note["id"] == row["targetId"] for note in notes), "Callout does not target a note definition"
        assert any(back["targetFile"] == row["file"] and back["targetId"] == row["id"] for back in backlinks), "Callout has no reverse link"
    return {"kind": "epub-note-numbering", "sha256": digest(raw), "bytes": len(raw), "references": references,
            "backlinks": backlinks, "notes": notes, "standardLibraryOnly": True, "externalReaderCertified": False}


def xml_bytes(root):
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)


def relationship_xml(rows):
    root = ET.Element(f"{{{P}}}Relationships")
    for identity, kind, target in rows:
        ET.SubElement(root, f"{{{P}}}Relationship", {"Id": identity, "Type": R + "/" + kind, "Target": target})
    return xml_bytes(root)


def fixture_parts(kind="native-mixed"):
    assert kind in FIXTURE_KINDS
    if kind.startswith("p06c-"):
        return p06c_fixture_parts(kind)
    caption = kind.startswith("native-caption-")
    safe_heading_ref = kind == "native-heading-ref-safe"
    safe_multi_section_ref = kind == "native-multi-section-safe-ref"
    base_kind = "native-multi-section" if safe_multi_section_ref else ("native-numbering" if safe_heading_ref else (kind.replace("native-caption-", "native-", 1) if caption else kind))
    with_notes, with_numbering = base_kind != "native-numbering", base_kind not in ("native-note-mixed", "native-multi-section")
    zero = base_kind == "native-zero-start"
    styles = f'''<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:styleId="NativeHeadingBase"><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="42"/></w:numPr><w:outlineLvl w:val="0"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="NativeHeadingBase"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:outlineLvl w:val="1"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="42"/></w:numPr></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="NoteBase"><w:pPr><w:spacing w:line="210" w:lineRule="exact"/><w:keepNext/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="FootnoteText"><w:name w:val="footnote text"/><w:basedOn w:val="NoteBase"/><w:pPr><w:keepLines w:val="0"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="EndnoteText"><w:name w:val="endnote text"/><w:basedOn w:val="NoteBase"/><w:pPr><w:spacing w:line="260" w:lineRule="atLeast"/><w:keepLines/></w:pPr><w:rPr><w:sz w:val="22"/></w:rPr></w:style></w:styles>'''
    if not with_numbering:
        root = parse_xml(styles.encode())
        for style in root.findall("w:style", NS):
            props = style.find("w:pPr", NS)
            if props is not None:
                num = props.find("w:numPr", NS)
                if num is not None:
                    props.remove(num)
        styles = ET.tostring(root, encoding="unicode")
    first_start = "0" if zero else "3"
    numbering = f'''<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="5"><w:multiLevelType w:val="multilevel"/>
<w:lvl w:ilvl="0"><w:start w:val="{first_start}"/><w:numFmt w:val="upperRoman"/><w:lvlRestart w:val="0"/><w:pStyle w:val="Heading1"/><w:suff w:val="tab"/><w:lvlText w:val="Article %1:"/></w:lvl>
<w:lvl w:ilvl="1"><w:start w:val="2"/><w:numFmt w:val="lowerLetter"/><w:lvlRestart w:val="1"/><w:pStyle w:val="Heading2"/><w:suff w:val="space"/><w:lvlText w:val="%1(%2)"/></w:lvl></w:abstractNum>
<w:num w:numId="42"><w:abstractNumId w:val="5"/></w:num>
<w:abstractNum w:abstractNumId="7"><w:lvl w:ilvl="0"><w:start w:val="5"/><w:numFmt w:val="decimal"/><w:suff w:val="space"/><w:lvlText w:val="%1)"/></w:lvl></w:abstractNum><w:num w:numId="77"><w:abstractNumId w:val="7"/></w:num></w:numbering>'''
    refs = ('<w:r><w:t>NATIVE_FIRST </w:t><w:footnoteReference w:id="7"/></w:r>'
            '<w:r><w:t> NATIVE_SECOND </w:t><w:endnoteReference w:id="7"/></w:r>'
            '<w:r><w:t> NATIVE_REPEAT </w:t><w:footnoteReference w:id="7"/></w:r>') if with_notes else '<w:r><w:t>NATIVE_BODY</w:t></w:r>'
    ordinary = '<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="77"/></w:numPr></w:pPr><w:r><w:t>ORDINARY_LIST_ONLY</w:t></w:r></w:p>' if with_numbering else ''
    # Footnote placement is explicitly section-owned.  A document-level pos
    # with a different value prevents an oracle from promoting the wrong one.
    sect_note = '<w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr>' if with_notes else ''
    # Keep the original hyphen-target material.  The separate open-boundary
    # fixture reaches the supported REF grammar without changing that control.
    heading_bookmark = "native_alpha" if safe_heading_ref or safe_multi_section_ref else "native-alpha"
    document = f'''<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="11" w:name="{heading_bookmark}"/><w:r><w:t>NATIVE_ALPHA</w:t></w:r><w:bookmarkEnd w:id="11"/></w:p>
<w:p>{refs}</w:p><w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>NATIVE_METHOD</w:t></w:r></w:p>{ordinary}
<w:p><w:r><w:t>NATIVE_FIELD </w:t></w:r><w:fldSimple w:instr=" REF {heading_bookmark} \\h "><w:r><w:t>NATIVE_ALPHA</w:t></w:r></w:fldSimple></w:p>
<w:sectPr>{sect_note}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1020" w:bottom="1134" w:left="1020"/></w:sectPr></w:body></w:document>'''
    note_settings = ('<w:footnotePr><w:pos w:val="pageBottom"/><w:numFmt w:val="upperRoman"/><w:numStart w:val="7"/><w:numRestart w:val="eachSect"/><w:footnote w:id="41"/><w:footnote w:id="42"/><w:footnote w:id="43"/></w:footnotePr>'
                     '<w:endnotePr><w:pos w:val="docEnd"/><w:numFmt w:val="lowerLetter"/><w:numStart w:val="3"/><w:numRestart w:val="continuous"/><w:endnote w:id="51"/><w:endnote w:id="52"/></w:endnotePr>') if with_notes else ''
    settings = f'<w:settings xmlns:w="{W}"><w:updateFields w:val="true"/>{note_settings}</w:settings>'
    parts = {"word/document.xml": document.encode(), "word/styles.xml": styles.encode(), "word/settings.xml": settings.encode(),
             "_rels/.rels": relationship_xml([("office", "officeDocument", "word/document.xml")])}
    relationships = [("styles", "styles", "styles.xml"), ("settings", "settings", "settings.xml")]
    if with_numbering:
        parts["word/numbering.xml"] = numbering.encode()
        relationships.append(("numbering", "numbering", "numbering.xml"))
    if with_notes:
        for note_kind, first, separator, continuation, notice, body_text in (
                ("footnote", "7", "41", "42", "43", "NATIVE_FOOT_BODY"),
                ("endnote", "7", "51", "52", None, "NATIVE_END_BODY")):
            style = "FootnoteText" if note_kind == "footnote" else "EndnoteText"
            story = ET.Element(q(note_kind + "s"))
            for identity, subtype, element in ((separator, "separator", "separator"), (continuation, "continuationSeparator", "continuationSeparator")):
                definition = ET.SubElement(story, q(note_kind), {q("id"): identity, q("type"): subtype})
                run = ET.SubElement(ET.SubElement(definition, q("p")), q("r"))
                if note_kind == "footnote" and subtype == "separator":
                    ET.SubElement(run, q("t")).text = ""  # explicit no-line separator
                else:
                    ET.SubElement(run, q(element))
            if notice:
                definition = ET.SubElement(story, q(note_kind), {q("id"): notice, q("type"): "continuationNotice"})
                ET.SubElement(ET.SubElement(ET.SubElement(definition, q("p")), q("r")), q("t")).text = "NATIVE_CONTINUED <next> & evidence"
            definition = ET.SubElement(story, q(note_kind), {q("id"): first})
            para = ET.SubElement(definition, q("p"))
            ET.SubElement(ET.SubElement(para, q("pPr")), q("pStyle"), {q("val"): style})
            ET.SubElement(ET.SubElement(para, q("r")), q("t")).text = body_text + " Evidence 😀."
            name = "word/notes/" + note_kind + "-custom.xml"
            parts[name] = xml_bytes(story)
            relationships.append((note_kind, note_kind + "s", "notes/" + note_kind + "-custom.xml"))
    if base_kind == "native-multi-section":
        document = parse_xml(parts["word/document.xml"])
        body = document.find("w:body", NS)
        final = body.find("w:sectPr", NS)
        final.find("w:footnotePr/w:pos", NS).set(q("val"), "pageBottom")
        boundary = ET.Element(q("p"))
        section = ET.SubElement(ET.SubElement(boundary, q("pPr")), q("sectPr"))
        ET.SubElement(section, q("type"), {q("val"): "continuous"})
        ET.SubElement(ET.SubElement(section, q("footnotePr")), q("pos"), {q("val"): "beneathText"})
        body.insert(list(body).index(final), boundary)
        after = ET.Element(q("p"))
        run = ET.SubElement(after, q("r"))
        ET.SubElement(run, q("t")).text = "NATIVE_SECOND_SECTION "
        ET.SubElement(run, q("footnoteReference"), {q("id"): "8"})
        body.insert(list(body).index(final), after)
        parts["word/document.xml"] = xml_bytes(document)
        name = "word/notes/footnote-custom.xml"
        story = parse_xml(parts[name])
        definition = ET.SubElement(story, q("footnote"), {q("id"): "8"})
        para = ET.SubElement(definition, q("p"))
        ET.SubElement(ET.SubElement(para, q("pPr")), q("pStyle"), {q("val"): "FootnoteText"})
        ET.SubElement(ET.SubElement(para, q("r")), q("t")).text = "NATIVE_SECOND_SECTION_NOTE Distinct placement."
        parts[name] = xml_bytes(story)
    if caption:
        # New positive fixtures coexist with the original heading-REF fixture.
        # A standard caption is representable in the existing source language;
        # arbitrary heading-REF semantics are retained as a separate open case.
        document = parse_xml(parts["word/document.xml"])
        body = document.find("w:body", NS)
        reference = next(node for node in body.findall("w:p", NS) if node.find("w:fldSimple", NS) is not None)
        field = reference.find("w:fldSimple", NS)
        field.set(q("instr"), " REF cap_table \\h ")
        field.find("w:r/w:t", NS).text = "Table 4"
        table = ET.Element(q("tbl"))
        ET.SubElement(ET.SubElement(table, q("tblPr")), q("tblW"), {q("w"): "2400", q("type"): "dxa"})
        grid = ET.SubElement(table, q("tblGrid"))
        for _ in range(2):
            ET.SubElement(grid, q("gridCol"), {q("w"): "1200"})
        row = ET.SubElement(table, q("tr"))
        for text in ("NATIVE_CELL", "42"):
            cell = ET.SubElement(row, q("tc"))
            ET.SubElement(ET.SubElement(ET.SubElement(cell, q("p")), q("r")), q("t")).text = text
        cap = ET.Element(q("p"))
        ET.SubElement(ET.SubElement(cap, q("pPr")), q("pStyle"), {q("val"): "Caption"})
        ET.SubElement(cap, q("bookmarkStart"), {q("id"): "21", q("name"): "cap_table"})
        prefix = ET.SubElement(ET.SubElement(cap, q("r")), q("t"))
        prefix.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
        prefix.text = "Table "
        seq = ET.SubElement(cap, q("fldSimple"), {q("instr"): " SEQ Table \\r 4 \\* ARABIC "})
        ET.SubElement(ET.SubElement(seq, q("r")), q("t")).text = "4"
        ET.SubElement(cap, q("bookmarkEnd"), {q("id"): "21"})
        suffix = ET.SubElement(ET.SubElement(cap, q("r")), q("t"))
        suffix.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
        suffix.text = " — NATIVE_CAPTION"
        at = list(body).index(reference)
        body.insert(at, table)
        body.insert(at + 1, cap)
        parts["word/document.xml"] = xml_bytes(document)
    parts["word/_rels/document.xml.rels"] = relationship_xml(relationships)
    types = ET.Element(f"{{{CT}}}Types")
    ET.SubElement(types, f"{{{CT}}}Default", {"Extension": "rels", "ContentType": "application/vnd.openxmlformats-package.relationships+xml"})
    ET.SubElement(types, f"{{{CT}}}Default", {"Extension": "xml", "ContentType": "application/xml"})
    for name in parts:
        if name == "word/document.xml":
            suffix = "document.main"
        elif name.startswith("word/notes/"):
            suffix = "footnotes" if "footnote" in name else "endnotes"
        elif name in ("word/styles.xml", "word/settings.xml", "word/numbering.xml"):
            suffix = PurePosixPath(name).stem
        else:
            continue
        ET.SubElement(types, f"{{{CT}}}Override", {"PartName": "/" + name, "ContentType": "application/vnd.openxmlformats-officedocument.wordprocessingml." + suffix + "+xml"})
    parts["[Content_Types].xml"] = xml_bytes(types)
    return parts


def p06c_formats():
    schema = Path(__file__).resolve().parents[2] / SCHEMA_PATH
    raw = schema.read_bytes()
    assert digest(raw) == SCHEMA_SHA256, "Fixed schema identity changed"
    rows = [row for row in json.loads(raw)["Enums"] if row.get("Type") == "w:ST_NumberFormat"]
    assert len(rows) == 1
    formats = [row["Value"] for row in rows[0]["Facets"]]
    assert len(formats) == len(set(formats)) == 63
    return formats


def p06c_bookmark(identity):
    stem = "section3a" + identity.replace("-", "_")
    return ("_s" + stem)[:23] + "_" + digest(("section:" + identity).encode())[:16]


def p06c_properties(parent, kind, values):
    if not values:
        return
    props = ET.SubElement(parent, q(kind + "Pr"))
    for name in ("pos", "numFmt", "numStart", "numRestart"):
        if name in values:
            node = ET.SubElement(props, q(name), {q("val"): str(values[name])})
            if name == "numFmt" and "numberPattern" in values:
                node.set(q("format"), values["numberPattern"])


def p06c_fixture_parts(kind):
    parts = fixture_parts("native-note-mixed")
    document = ET.Element(q("document"))
    body = ET.SubElement(document, q("body"))
    settings = ET.Element(q("settings"))
    global_values = {"footnote": {"pos": "pageBottom", "numFmt": "lowerLetter", "numStart": 3, "numRestart": "continuous"},
                     "endnote": {"pos": "docEnd", "numFmt": "lowerRoman", "numStart": 1, "numRestart": "continuous"}}
    original_settings = parse_xml(parts["word/settings.xml"])
    for note_kind, values in global_values.items():
        p06c_properties(settings, note_kind, values)
        props = settings.find("w:" + note_kind + "Pr", NS)
        for ref in original_settings.find("w:" + note_kind + "Pr", NS):
            if ref.tag == q(note_kind):
                props.append(copy.deepcopy(ref))
    if kind == "p06c-native-format-matrix":
        locals_ = [{note_kind: {"numFmt": fmt, "numStart": [0, 1, 27, 12000, 65535][index % 5],
                              "numRestart": ["continuous", "eachSect", "eachPage"][index % 3],
                              **({"numberPattern": "A"} if fmt == "custom" else {})}
                    for note_kind in ("footnote", "endnote")} for index, fmt in enumerate(p06c_formats())]
    else:
        locals_ = [{"footnote": {"pos": "beneathText", "numFmt": "upperRoman", "numStart": 7, "numRestart": "eachSect"},
                    "endnote": {"numFmt": "upperLetter", "numStart": 27, "numRestart": "eachSect"}},
                   {note_kind: {"numFmt": "decimal", "numStart": 0, "numRestart": "continuous"} for note_kind in ("footnote", "endnote")}, {}]
    for index, local_values in enumerate(locals_):
        identity = f"p06c{index}"
        para = ET.SubElement(body, q("p"))
        ET.SubElement(ET.SubElement(para, q("pPr")), q("pStyle"), {q("val"): "Heading1"})
        ET.SubElement(para, q("bookmarkStart"), {q("id"): str(100 + index), q("name"): p06c_bookmark(identity)})
        ET.SubElement(ET.SubElement(para, q("r")), q("t")).text = "P06C_DUPLICATE_HEADING"
        ET.SubElement(para, q("bookmarkEnd"), {q("id"): str(100 + index)})
        content = ET.SubElement(body, q("p"))
        ET.SubElement(ET.SubElement(content, q("r")), q("t")).text = f"P06C_SCOPE_{index:02d} 😀 "
        for note_kind in ("footnote", "endnote"):
            run = ET.SubElement(content, q("r"))
            ET.SubElement(run, q(note_kind + "Reference"), {q("id"): "7"})
        ET.SubElement(ET.SubElement(content, q("r")), q("footnoteReference"), {q("id"): "7"})
        if index == 0 and len(locals_) == 3:
            link = ET.SubElement(content, q("hyperlink"), {q("anchor"): p06c_bookmark("p06c2")})
            ET.SubElement(ET.SubElement(link, q("r")), q("t")).text = "P06C_SECTION_LINK"
        if index == 1 and len(locals_) == 3:
            for note_kind in ("footnote", "endnote"):
                ET.SubElement(ET.SubElement(content, q("r")), q(note_kind + "Reference"), {q("id"): "8"})
        if index + 1 == len(locals_):
            section = ET.SubElement(body, q("sectPr"))
        else:
            terminator = ET.SubElement(body, q("p"))
            section = ET.SubElement(ET.SubElement(terminator, q("pPr")), q("sectPr"))
        for note_kind in ("footnote", "endnote"):
            p06c_properties(section, note_kind, local_values.get(note_kind))
        ET.SubElement(section, q("type"), {q("val"): "continuous" if index else "nextPage"})
    if len(locals_) == 3:
        for note_kind in ("footnote", "endnote"):
            name = "word/notes/" + note_kind + "-custom.xml"
            story = parse_xml(parts[name])
            definition = ET.SubElement(story, q(note_kind), {q("id"): "8"})
            para = ET.SubElement(definition, q("p"))
            ET.SubElement(ET.SubElement(para, q("pPr")), q("pStyle"), {q("val"): "FootnoteText" if note_kind == "footnote" else "EndnoteText"})
            ET.SubElement(ET.SubElement(para, q("r")), q("t")).text = "P06C_NEW_" + note_kind.upper()
            parts[name] = xml_bytes(story)
    parts["word/document.xml"] = xml_bytes(document)
    parts["word/settings.xml"] = xml_bytes(settings)
    return parts


def mutate_p06c(parts, mode):
    before = dict(parts)
    parts = dict(parts)
    document = parse_xml(parts["word/document.xml"])
    body = document.find("w:body", NS)
    sections = list(body.iter(q("sectPr")))
    assert len(sections) >= 3
    target = sections[1]
    if mode in ("p06c-section-clear", "p06c-all-clear"):
        for section in sections:
            for note_kind in ("footnote", "endnote"):
                props = section.find("w:" + note_kind + "Pr", NS)
                if props is not None:
                    for node in list(props):
                        if node.tag in (q("numFmt"), q("numStart"), q("numRestart")):
                            props.remove(node)
        if mode == "p06c-all-clear":
            settings = parse_xml(parts["word/settings.xml"])
            for note_kind in ("footnote", "endnote"):
                props = settings.find("w:" + note_kind + "Pr", NS)
                if props is not None:
                    for node in list(props):
                        if node.tag in (q("numFmt"), q("numStart"), q("numRestart")):
                            props.remove(node)
            parts["word/settings.xml"] = xml_bytes(settings)
    else:
        foot = target.find("w:footnotePr", NS)
        if foot is None:
            foot = ET.Element(q("footnotePr"))
            target.insert(0, foot)
        if mode == "p06c-section-change":
            for node in list(foot):
                if node.tag in (q("numFmt"), q("numStart"), q("numRestart")):
                    foot.remove(node)
            for name, value in (("numFmt", "chicago"), ("numStart", "65535"), ("numRestart", "eachPage")):
                ET.SubElement(foot, q(name), {q("val"): value})
        elif mode == "p06c-invalid-start":
            set_value(foot, "numStart", "65536")
        elif mode == "p06c-missing-val":
            foot.find("w:numFmt", NS).attrib.pop(q("val"))
        elif mode == "p06c-foreign-format":
            foot.find("w:numFmt", NS).tag = "{urn:readmd:qa:foreign}numFmt"
        elif mode == "p06c-wrong-order":
            node = foot.find("w:numStart", NS)
            foot.remove(node)
            foot.insert(0, node)
        else:
            raise AssertionError("Unknown P06-C mutation")
    parts["word/document.xml"] = xml_bytes(document)
    changed = sorted(name for name in parts if parts[name] != before[name])
    assert changed == (["word/document.xml", "word/settings.xml"] if mode == "p06c-all-clear" else ["word/document.xml"])
    assert all(parts[name] == raw for name, raw in before.items() if name not in changed)
    return parts, {"mode": mode, "changedParts": changed, "allOtherPartsByteIdentical": True, "applicationEditingCertified": False}


def write_package(output, parts):
    output = Path(output)
    assert output.parent.is_dir() and not output.exists(), "Output must be new inside an existing owned directory"
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, raw in sorted(parts.items()):
            info = zipfile.ZipInfo(name, (2026, 10, 21, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, raw)


def set_value(parent, name, value):
    node = parent.find("w:" + name, NS)
    if node is None:
        node = ET.SubElement(parent, q(name))
    node.set(q("val"), str(value))


def refresh_owned_chapter_caches(document):
    """Explicit fixture edit, not a claim that Word or an engine refreshed it."""
    chapter_fields = [field for field in document.iter(q("fldSimple"))
                      if re.fullmatch(r"\s*STYLEREF\s+1\s+\\s\s*", val(field, "instr") or "", re.I)]
    assert len(chapter_fields) == 1, "Owned source requires one chapter field"
    chapter = chapter_fields[0]
    assert visible(chapter) == "III", "Unexpected original chapter cache"

    def cache(field, value):
        texts = list(field.iter(q("t")))
        assert texts, "Field has no native text cache"
        texts[0].text = value
        for text in texts[1:]:
            text.text = ""

    cache(chapter, "Part v.")
    captions = [p for p in document.iter(q("p")) if chapter in list(p)]
    assert len(captions) == 1
    children = list(captions[0])
    chapter_index = children.index(chapter)
    bindings = []
    for start, node in enumerate(children):
        if node.tag != q("bookmarkStart") or start >= chapter_index:
            continue
        end = next((index for index, mark in enumerate(children) if index > chapter_index
                    and mark.tag == q("bookmarkEnd") and val(mark, "id") == val(node, "id")), None)
        if end is not None:
            fragment = ET.Element(q("p"))
            fragment.extend(copy.deepcopy(children[start + 1:end]))
            bindings.append((val(node, "name"), visible(fragment)))
    assert len(bindings) == 1, "Chapter field must belong to one current caption bookmark"
    bookmark, label = bindings[0]
    assert label == "Table Part v..4", "Unexpected current caption range/cache"
    references = [field for field in document.iter(q("fldSimple"))
                  if (val(field, "instr") or "").split()[:2] == ["REF", bookmark]]
    assert len(references) == 1, "Owned source requires one REF to the caption"
    assert visible(references[0]) == "Table III.4", "Unexpected original REF cache"
    cache(references[0], label)
    return {"performedBy": "owned-headless-OPC-edit", "engineRefreshCertified": False,
            "styleRefCache": "Part v.", "refCache": label, "captionBookmark": bookmark}


def flatten_current_numbering_fields(paragraph):
    """Freeze the owned numbering fields at their outer visible result only."""
    native = fields(paragraph)
    assert native and all(re.match(r"\s*(?:STYLEREF\b|SEQ\b|REF\b|=)", field["instruction"], re.I)
                          for field in native), "Unexpected field in numbering-clear fixture"
    before, stack = visible(paragraph), []

    def keep(node):
        if node.tag == q("fldChar"):
            kind = val(node, "fldCharType")
            if kind == "begin":
                stack.append(False)
            elif kind == "separate":
                assert stack, "Unbound field separator"
                stack[-1] = True
            else:
                assert kind == "end" and stack, "Unbound field end"
                stack.pop()
            return []
        if node.tag == q("instrText"):
            return []
        if node.tag in (q("pPr"), q("rPr")):
            return [copy.deepcopy(node)]
        if node.tag == q("fldSimple"):
            return [kept for child in node for kept in keep(child)]
        if not len(node):
            return [copy.deepcopy(node)] if all(stack) else []
        result = copy.deepcopy(node)
        result[:] = [kept for child in node for kept in keep(child)]
        if node.tag == q("r") and not any(child.tag != q("rPr") for child in result):
            return []
        return [result]

    result = keep(paragraph)
    assert not stack and len(result) == 1
    paragraph[:] = list(result[0])
    assert visible(paragraph) == before and not fields(paragraph), "Clearing fields changed visible content"
    return len(native)


def mutate_parts(parts, mode):
    if mode.startswith("p06c-"):
        return mutate_p06c(parts, mode)
    assert mode in ("source-native", "source-native-refreshed", "notes-native", "numbering-native", "extended-format", "extended-format-uniform", "unknown-format", "foreign-format", "numbering-clear", "notes-clear")
    requested_mode = mode
    uniform = mode == "extended-format-uniform"
    if uniform:
        mode = "extended-format"
    refresh = mode == "source-native-refreshed"
    if refresh:
        mode = "source-native"
    package = Package(parts=parts)
    changed = set()

    def save(part, root):
        package.parts[part] = xml_bytes(root)
        changed.add(part)

    native_clear = None
    if mode == "numbering-clear":
        styles = Styles(package)
        document = copy.deepcopy(package.xml(package.document))

        def off(properties):
            for old in list(properties.findall("w:numPr", NS)):
                properties.remove(old)
            set_value(ET.SubElement(properties, q("numPr")), "numId", 0)

        headings, count = 0, 0
        for p in document.iter(q("p")):
            info = paragraph(p, styles)
            heading = re.fullmatch(r"Heading[1-6]", info["style"] or "") or info["properties"].get("outlineLvl") in tuple(map(str, range(6)))
            if heading and info["number"] and info["number"].get("numId") not in (None, "0"):
                properties = p.find("w:pPr", NS)
                if properties is None:
                    properties = ET.Element(q("pPr"))
                    p.insert(0, properties)
                off(properties)
                headings += 1
            if fields(p):
                count += flatten_current_numbering_fields(p)
        assert headings >= 2 and count, "Numbering clear requires current native headings and fields"
        save(package.document, document)
        style_part = package.related("styles", optional=False)
        root = copy.deepcopy(package.xml(style_part))
        style_count = 0
        for style in root.findall("w:style", NS):
            identity = val(style, "styleId")
            effective = styles.effective(identity, "p")
            if effective.get("outlineLvl") in tuple(map(str, range(6))) or re.fullmatch(r"Heading[1-6]", identity or ""):
                if effective.get("numPr", {}).get("numId") not in (None, "0"):
                    properties = style.find("w:pPr", NS)
                    if properties is None:
                        properties = ET.SubElement(style, q("pPr"))
                    off(properties)
                    style_count += 1
        if style_count:
            save(style_part, root)
        native_clear = {"headingsDisabled": headings, "headingStylesDisabled": style_count, "fieldsFlattened": count,
                        "visibleFieldResultsPreserved": True, "applicationEditingCertified": False}
    elif mode == "notes-clear":
        part = package.related("settings", optional=False)
        root = copy.deepcopy(package.xml(part))
        removed = 0
        for properties in list(root):
            if properties.tag in (q("footnotePr"), q("endnotePr")):
                root.remove(properties)
                removed += 1
        assert removed == 2, "Note clear requires both current native stories' settings"
        save(part, root)
        document = copy.deepcopy(package.xml(package.document))
        section_removed = 0
        for section in document.iter(q("sectPr")):
            for properties in list(section):
                if properties.tag in (q("footnotePr"), q("endnotePr")):
                    section.remove(properties)
                    section_removed += 1
        if section_removed:
            save(package.document, document)
        native_clear = {"notePropertiesRemoved": removed + section_removed, "storyBytesPreserved": True,
                        "nativeDefaults": {"footnote": "decimal", "endnote": "lowerRoman", "start": 1,
                                           "restart": "continuous"}, "applicationEditingCertified": False}
    if mode in ("source-native", "notes-native", "extended-format", "unknown-format", "foreign-format"):
        part = package.related("settings", optional=False)
        root = copy.deepcopy(package.xml(part))
        foot = root.find("w:footnotePr", NS)
        assert foot is not None, "Mutation requires actual footnote properties"
        if mode in ("extended-format", "unknown-format"):
            set_value(foot, "numFmt", "chicago" if mode == "extended-format" else "nativeMadeUp")
            if mode == "extended-format":
                set_value(foot, "numStart", 12000)
        elif mode == "foreign-format":
            node = foot.find("w:numFmt", NS)
            assert node is not None
            node.tag = "{urn:readmd:qa:foreign}numFmt"
        else:
            set_value(foot, "numFmt", "lowerLetter")
            set_value(foot, "numStart", 13)
            end = root.find("w:endnotePr", NS)
            if end is not None:
                set_value(end, "numFmt", "upperRoman")
                set_value(end, "numStart", 17)
        save(part, root)
        # Current section properties can override document-wide values.
        # Update them too, without touching any source/provenance member.
        document = copy.deepcopy(package.xml(package.document))
        touched = False
        for foot in document.findall(".//w:sectPr/w:footnotePr", NS):
            if mode in ("source-native", "notes-native"):
                set_value(foot, "numFmt", "lowerLetter")
                set_value(foot, "numStart", 13)
                touched = True
            elif mode in ("extended-format", "unknown-format"):
                set_value(foot, "numFmt", "chicago" if mode == "extended-format" else "nativeMadeUp")
                if mode == "extended-format":
                    set_value(foot, "numStart", 12000)
                touched = True
            elif mode == "foreign-format":
                node = foot.find("w:numFmt", NS)
                if node is not None:
                    node.tag = "{urn:readmd:qa:foreign}numFmt"
                    touched = True
        if touched:
            save(package.document, document)
        if mode == "extended-format":
            style_part = package.related("styles", optional=False)
            styles = copy.deepcopy(package.xml(style_part))
            rows = [style for style in styles.findall("w:style", NS) if val(style, "styleId") == "FootnoteText"]
            assert len(rows) == 1, "Mutation requires actual FootnoteText style"
            props = rows[0].find("w:rPr", NS)
            if props is None:
                props = ET.SubElement(rows[0], q("rPr"))
            set_value(props, "sz", 60)
            set_value(props, "szCs", 60)
            save(style_part, styles)
            if uniform:
                part = package.related("footnotes", optional=False)
                story = copy.deepcopy(package.xml(part))
                count, notes = 0, 0
                for note in story.findall("w:footnote", NS):
                    if val(note, "type") not in (None, "normal"):
                        continue
                    notes += 1
                    for run in note.iter(q("r")):
                        if not any(text.text for text in run.findall("w:t", NS)):
                            continue
                        props = run.find("w:rPr", NS)
                        if props is None:
                            props = ET.Element(q("rPr"))
                            run.insert(0, props)
                        set_value(props, "sz", 60)
                        set_value(props, "szCs", 60)
                        count += 1
                assert notes >= 2 and count >= 5, "Uniform fixture requires authored and SDT normal-note text"
                save(part, story)
                uniform_text = {"normalNotesUpdated": notes, "runsUpdated": count, "halfPoints": 60,
                                "specialNotesUnchanged": True, "endnotesUnchanged": True,
                                "metadataResigned": False, "applicationEditingCertified": False}
    if mode in ("source-native", "numbering-native"):
        part = package.related("numbering", optional=False)
        root = copy.deepcopy(package.xml(part))
        changed_levels = 0
        for abstract in root.findall("w:abstractNum", NS):
            for level in abstract.findall("w:lvl", NS):
                if val(level.find("w:pStyle", NS)) == "Heading1":
                    set_value(level, "numFmt", "lowerRoman")
                    set_value(level, "start", 5)
                    set_value(level, "lvlText", "Part %1.")
                    set_value(level, "suff", "tab")
                    changed_levels += 1
        assert changed_levels, "Mutation requires a native Heading1 level"
        save(part, root)
    field_refresh = None
    if refresh:
        document = parse_xml(package.parts[package.document])
        field_refresh = refresh_owned_chapter_caches(document)
        save(package.document, document)
    assert changed
    before, after = parts, package.parts
    assert before.keys() == after.keys()
    actual = {name for name in before if before[name] != after[name]}
    assert actual == changed
    assert all(before[name] == after[name] for name in before if name not in changed)
    auxiliary = {name: digest(raw) for name, raw in before.items() if name.startswith("customXml/")}
    assert all(digest(after[name]) == value for name, value in auxiliary.items()), "Mutation altered source/provenance"
    report = {"mode": requested_mode, "changedParts": sorted(changed), "unchangedParts": {name: digest(raw) for name, raw in sorted(before.items()) if name not in changed},
              "auxiliaryPartsUnchanged": auxiliary, "metadataResigned": False, "applicationEditingCertified": False}
    if field_refresh:
        report["fieldCacheRefresh"] = field_refresh
    if native_clear:
        report["nativeClear"] = native_clear
    if uniform:
        report["uniformFootnoteText"] = uniform_text
    return after, report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    read = commands.add_parser("inspect")
    read.add_argument("input", type=Path)
    epub = commands.add_parser("inspect-epub-notes")
    epub.add_argument("input", type=Path)
    fixture = commands.add_parser("fixture")
    fixture.add_argument("--kind", required=True, choices=FIXTURE_KINDS)
    fixture.add_argument("--output", required=True, type=Path)
    mutation = commands.add_parser("mutate")
    mutation.add_argument("--mode", required=True, choices=("source-native", "source-native-refreshed", "notes-native", "numbering-native", "extended-format", "extended-format-uniform", "unknown-format", "foreign-format", "numbering-clear", "notes-clear",
                         "p06c-section-change", "p06c-section-clear", "p06c-all-clear", "p06c-invalid-start", "p06c-missing-val", "p06c-foreign-format", "p06c-wrong-order"))
    mutation.add_argument("--input", required=True, type=Path)
    mutation.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    if args.action == "inspect":
        result = inspect(args.input)
    elif args.action == "inspect-epub-notes":
        result = inspect_epub_notes(args.input)
    elif args.action == "fixture":
        parts = fixture_parts(args.kind)
        write_package(args.output, parts)
        result = {"fixture": args.kind, "outputSha256": digest(args.output.read_bytes()), "nativeOnly": True,
                  "sourcePayloadPresent": False, "applicationEditingCertified": False, "inspection": inspect(args.output)}
        if args.kind == "p06c-native-format-matrix":
            result["fixedSchema"] = {"path": SCHEMA_PATH, "sha256": SCHEMA_SHA256, "formats": p06c_formats()}
    else:
        package = Package(path=args.input)
        parts, result = mutate_parts(package.parts, args.mode)
        write_package(args.output, parts)
        result.update({"inputSha256": digest(args.input.read_bytes()), "outputSha256": digest(args.output.read_bytes()), "inspection": inspect(args.output)})
    return result


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    print(json.dumps(main(), ensure_ascii=False, allow_nan=False))
