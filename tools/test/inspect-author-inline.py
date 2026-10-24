"""Independent author-text OPC/XHTML oracle and bounded native-only fixtures.

Standard library only. Never starts Office, reads a relationship outside the
package, or derives native typography from a ReadMD source/provenance payload.
"""
import argparse
import hashlib
from html.parser import HTMLParser
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
NS = {"w": W, "r": R}
MAX_BYTES = 64 * 1024 * 1024
for prefix, uri in (("w", W), ("r", R)):
    ET.register_namespace(prefix, uri)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def q(name):
    return "{" + W + "}" + name


def xml(raw):
    assert 0 < len(raw) <= 16 * 1024 * 1024
    assert b"<!DOCTYPE" not in raw.upper() and b"<!ENTITY" not in raw.upper()
    root = ET.fromstring(raw)
    pending, count = [(root, 1)], 0
    while pending:
        node, depth = pending.pop()
        count += 1
        assert count <= 400000 and depth <= 128
        pending.extend((child, depth + 1) for child in node)
    return root


def xhtml(raw):
    # ReadMD emits the inert HTML5 doctype in EPUB content. Admit only that
    # exact prolog; DOCX/XML, external identifiers, internal subsets and entity
    # declarations retain the strict bounded parser below.
    assert 0 < len(raw) <= 16 * 1024 * 1024
    text = raw.decode('utf-8-sig')
    assert '\x00' not in text
    declaration = '<!DOCTYPE html>'
    if declaration in text:
        prefix = re.match(r'\A\s*(?:<\?xml\b[^?]{0,512}\?>\s*)?<!DOCTYPE html>(?=\s*<html(?:\s|>))', text)
        assert prefix is not None, 'Only the canonical XHTML prolog is permitted'
        start = text.index(declaration, 0, prefix.end())
        text = text[:start] + text[start + len(declaration):]
    return xml(text.encode('utf-8'))


def target(part, name):
    name = unquote(name)
    assert name and not any(char in name for char in "\\:\x00?#")
    path = posixpath.normpath(name.lstrip("/") if name.startswith("/") else posixpath.join(posixpath.dirname(part), name))
    assert path not in ("", ".", "..") and not path.startswith("../")
    return path


def relations_part(part):
    path = PurePosixPath(part)
    return str(path.parent / "_rels" / (path.name + ".rels")) if part else "_rels/.rels"


class Package:
    def __init__(self, file):
        self.raw = Path(file).read_bytes()
        assert 0 < len(self.raw) <= MAX_BYTES
        with zipfile.ZipFile(file) as archive:
            rows = archive.infolist()
            assert len(rows) <= 4096 and len({row.filename for row in rows}) == len(rows)
            assert sum(row.file_size for row in rows) <= MAX_BYTES
            assert archive.testzip() is None
            self.parts = {row.filename: archive.read(row) for row in rows}
        for name in self.parts:
            assert not name.startswith("/") and "\\" not in name and ".." not in PurePosixPath(name).parts
        self.cache = {}
        roots = [row for row in self.relationships("") if row["Type"].endswith("/officeDocument")]
        assert len(roots) == 1 and roots[0].get("TargetMode", "Internal") == "Internal"
        self.document = target("", roots[0]["Target"])
        assert self.document in self.parts

    def node(self, part):
        if part not in self.cache:
            self.cache[part] = xml(self.parts[part])
        return self.cache[part]

    def relationships(self, part):
        name = relations_part(part)
        if name not in self.parts:
            return []
        root = self.node(name)
        assert root.tag == "{" + P + "}Relationships"
        rows, identities = [], set()
        for node in root:
            assert node.tag == "{" + P + "}Relationship"
            row = dict(node.attrib)
            assert set(row) <= {"Id", "Type", "Target", "TargetMode"}
            assert all(row.get(key) for key in ("Id", "Type", "Target")) and row["Id"] not in identities
            assert row.get("TargetMode", "Internal") in ("Internal", "External")
            identities.add(row["Id"])
            rows.append(row)
        return rows

    def related(self, kind):
        rows = [row for row in self.relationships(self.document) if row["Type"].endswith("/" + kind)]
        assert len(rows) <= 1
        if not rows:
            return None
        assert rows[0].get("TargetMode", "Internal") == "Internal"
        part = target(self.document, rows[0]["Target"])
        assert part in self.parts
        return part


def val(node, key="val"):
    return node.get(q(key)) if node is not None else None


def properties(node):
    result = {}
    if node is None:
        return result
    for child in node:
        if child.tag in (q("sz"), q("szCs"), q("cs"), q("rtl")):
            name = child.tag.rsplit("}", 1)[1]
            assert name not in result, "Duplicate current native property"
            assert set(child.attrib) <= {q("val")}, "Non-native property attribute"
            raw = val(child)
            if name in ("sz", "szCs"):
                assert raw and raw.isascii() and raw.isdigit() and 0 < int(raw) <= 2147483647
                result[name] = int(raw)
            else:
                assert raw in (None, "0", "1", "true", "false", "on", "off")
                result[name] = raw not in ("0", "false", "off")
    return result


class Styles:
    def __init__(self, package):
        self.rows, self.cache, self.defaults = {}, {}, {}
        self.normal = None
        part = package.related("styles")
        if part is None:
            return
        root = package.node(part)
        assert root.tag == q("styles")
        self.defaults = properties(root.find("w:docDefaults/w:rPrDefault/w:rPr", NS))
        for row in root.findall("w:style", NS):
            identity = val(row, "styleId")
            assert identity and identity not in self.rows
            self.rows[identity] = (val(row.find("w:basedOn", NS)), properties(row.find("w:rPr", NS)))
            if val(row, "type") == "paragraph" and val(row, "default") in ("1", "true", "on"):
                assert self.normal is None
                self.normal = identity

    def patch(self, identity, trail=()):
        # A character-style patch must not reapply docDefaults over the
        # paragraph's current sz/szCs. Only its actual ancestry overlays it.
        if not identity:
            return {}
        assert identity not in trail and len(trail) < 64
        if identity not in self.cache:
            base, own = self.rows.get(identity, (None, {}))
            self.cache[identity] = {**self.patch(base, trail + (identity,)), **own}
        return self.cache[identity]

    def run(self, paragraph, run):
        ppr, rpr = paragraph.find("w:pPr", NS), run.find("w:rPr", NS)
        pstyle = val(ppr.find("w:pStyle", NS)) if ppr is not None else None
        rstyle = val(rpr.find("w:rStyle", NS)) if rpr is not None else None
        return {**self.defaults, **self.patch(pstyle or self.normal), **self.patch(rstyle), **properties(rpr)}


def paragraph_rows(root, styles, story, part):
    rows = []
    for ordinal, paragraph in enumerate(root.iter(q("p"))):
        runs, at = [], 0
        parents = {child: parent for parent in paragraph.iter() for child in parent}
        for run in paragraph.iter(q("r")):
            visible = "".join(text.text or "" for text in run.findall("w:t", NS))
            if not visible:
                continue
            props = styles.run(paragraph, run)
            ancestor, hyperlink = parents.get(run), None
            while ancestor is not None and ancestor is not paragraph:
                if ancestor.tag == q('hyperlink'):
                    hyperlink = {'part': part, 'relationship': ancestor.get('{' + R + '}id'), 'anchor': val(ancestor, 'anchor')}
                    break
                ancestor = parents.get(ancestor)
            runs.append({"text": visible, "from": at, "to": at + len(visible), "halfPoints": props.get("sz"),
                         "halfPointsCs": props.get("szCs"), "cs": props.get("cs"), "rtl": props.get("rtl"),
                         "declared": properties(run.find("w:rPr", NS)), 'hyperlink': hyperlink})
            at += len(visible)
        rows.append({"story": story, "ordinal": ordinal, "text": "".join(row["text"] for row in runs), "runs": runs})
    return rows


def inspect_docx(file):
    package = Package(file)
    styles = Styles(package)
    body = package.node(package.document)
    assert body.tag == q("document")
    rows = paragraph_rows(body, styles, "body", package.document)
    notes = {}
    hyperlinks = [{"part": package.document, **row} for row in package.relationships(package.document) if row["Type"].endswith("/hyperlink")]
    for kind in ("footnote", "endnote"):
        part = package.related(kind + "s")
        if part is None:
            continue
        root = package.node(part)
        assert root.tag == q(kind + "s")
        definitions, seen = [], set()
        for note in root.findall("w:" + kind, NS):
            identity, mode = val(note, "id"), val(note, "type")
            assert identity is not None and identity not in seen
            seen.add(identity)
            if mode not in (None, "normal") or int(identity) <= 0:
                continue
            current = paragraph_rows(note, styles, kind + ":" + identity, part)
            rows.extend(current)
            definitions.append({"id": identity, "text": "\n".join(row["text"] for row in current)})
        notes[kind] = {"part": part, "definitions": definitions,
                       "references": [val(node, "id") for node in body.iter(q(kind + "Reference"))]}
        hyperlinks.extend({'part': part, **row} for row in package.relationships(part) if row['Type'].endswith('/hyperlink'))
    auxiliary = {name: digest(raw) for name, raw in package.parts.items() if name.startswith("customXml/") or name.startswith("readmd/")}
    fields, bookmarks = [], []
    for ordinal, paragraph in enumerate(body.iter(q('p'))):
        visible = ''.join(node.text or '' for node in paragraph.iter(q('t')))
        for node in paragraph.iter():
            if node.tag == q('instrText'):
                fields.append({'instruction': node.text or '', 'paragraphOrdinal': ordinal, 'paragraphText': visible})
            elif node.tag == q('fldSimple'):
                fields.append({'instruction': val(node, 'instr') or '', 'paragraphOrdinal': ordinal, 'paragraphText': visible})
            elif node.tag == q('bookmarkStart'):
                bookmarks.append({'name': val(node, 'name'), 'id': val(node, 'id'), 'paragraphOrdinal': ordinal, 'paragraphText': visible})
    return {"kind": "author-inline-docx", "sha256": digest(package.raw), "bytes": len(package.raw), "documentPart": package.document,
            "paragraphs": rows, "notes": notes, "hyperlinks": hyperlinks, "auxiliaryParts": auxiliary,
            "fields": fields, "bookmarks": bookmarks,
            "parts": {name: digest(raw) for name, raw in package.parts.items()}, "sourcePayloadUsedAsNativeEvidence": False,
            "applicationRoundtripCertified": False}


def r(text, raw=""):
    node = ET.Element(q("r"))
    if raw:
        node.append(ET.fromstring('<w:rPr xmlns:w="' + W + '">' + raw + '</w:rPr>'))
    child = ET.SubElement(node, q("t"), {"{http://www.w3.org/XML/1998/namespace}space": "preserve"})
    child.text = text
    return node


def native_parts(mode="positive"):
    document = ET.Element(q("document"), {"xmlns:r": R})
    body = ET.SubElement(document, q("body"))
    paragraph = ET.SubElement(body, q("p"))
    ppr = ET.SubElement(paragraph, q("pPr"))
    ET.SubElement(ppr, q("pStyle"), {q("val"): "NativeBase"})
    paragraph.extend([r("NATIVE_BASE "), r("NATIVE_BIG ", '<w:sz w:val="60"/><w:szCs w:val="20"/>'),
                      r("NATIVE_CHAR ", '<w:rStyle w:val="NativeCharacter"/>'),
                      r("NATIVE_FORCED ", '<w:sz w:val="50"/><w:szCs w:val="16"/><w:cs/>'),
                      r("NATIVE_ROUTE_CLEAR", '<w:cs w:val="false"/><w:rtl w:val="0"/>')])
    if mode == "invalid-size":
        paragraph.append(r("NATIVE_INVALID", '<w:sz w:val="0"/>'))
    elif mode == "foreign-size":
        paragraph.append(r("NATIVE_INVALID", '<w:sz xmlns:x="urn:qa:foreign" x:val="60"/>'))
    elif mode == "field-size":
        field = ET.SubElement(paragraph, q("fldSimple"), {q("instr"): "PAGE"})
        field.append(r("NATIVE_FIELD", '<w:sz w:val="60"/>'))
    note_paragraph = ET.SubElement(body, q("p"))
    note_paragraph.append(r("NATIVE_NOTE_REFERENCE "))
    ET.SubElement(ET.SubElement(note_paragraph, q("r")), q("footnoteReference"), {q("id"): "7"})
    note_paragraph.append(r(" NATIVE_END_REFERENCE "))
    ET.SubElement(ET.SubElement(note_paragraph, q("r")), q("endnoteReference"), {q("id"): "7"})
    ET.SubElement(body, q("sectPr"))
    footnotes = ET.Element(q("footnotes"))
    note = ET.SubElement(footnotes, q("footnote"), {q("id"): "7"})
    note_p = ET.SubElement(note, q("p"))
    note_ppr = ET.SubElement(note_p, q("pPr"))
    ET.SubElement(note_ppr, q("pStyle"), {q("val"): "FootnoteText"})
    marker_run = ET.SubElement(note_p, q("r"))
    ET.SubElement(marker_run, q("footnoteRef"))
    note_p.extend([r("NATIVE_NOTE_BASE "), r("NATIVE_NOTE_BIG", '<w:sz w:val="60"/><w:szCs w:val="14"/>')])
    endnotes = ET.Element(q("endnotes"))
    endnote = ET.SubElement(endnotes, q("endnote"), {q("id"): "7"})
    end_p = ET.SubElement(endnote, q("p"))
    end_ppr = ET.SubElement(end_p, q("pPr"))
    ET.SubElement(end_ppr, q("pStyle"), {q("val"): "EndnoteText"})
    ET.SubElement(ET.SubElement(end_p, q("r")), q("endnoteRef"))
    end_p.extend([r("NATIVE_END_BASE "), r("NATIVE_END_BIG", '<w:sz w:val="52"/><w:szCs w:val="13"/>')])
    styles = ('<w:styles xmlns:w="' + W + '"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="23"/><w:szCs w:val="19"/></w:rPr></w:rPrDefault></w:docDefaults>'
              '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
              '<w:style w:type="paragraph" w:styleId="NativeParent"><w:rPr><w:sz w:val="24"/><w:szCs w:val="20"/></w:rPr></w:style>'
              '<w:style w:type="paragraph" w:styleId="NativeBase"><w:basedOn w:val="NativeParent"/></w:style>'
              '<w:style w:type="character" w:styleId="NativeCharacterParent"><w:rPr><w:szCs w:val="15"/></w:rPr></w:style>'
              '<w:style w:type="character" w:styleId="NativeCharacter"><w:basedOn w:val="NativeCharacterParent"/></w:style>'
              '<w:style w:type="paragraph" w:styleId="FootnoteText"><w:rPr><w:sz w:val="20"/><w:szCs w:val="18"/></w:rPr></w:style>'
              '<w:style w:type="paragraph" w:styleId="EndnoteText"><w:rPr><w:sz w:val="22"/><w:szCs w:val="16"/></w:rPr></w:style></w:styles>')
    relationships = '<Relationships xmlns="' + P + '">' + ''.join('<Relationship Id="' + key + '" Type="' + R + '/' + kind + '" Target="' + payload + '"/>' for key, kind, payload in [('styles', 'styles', 'styles.xml'), ('notes', 'footnotes', 'footnotes.xml'), ('endnotes', 'endnotes', 'endnotes.xml')]) + '</Relationships>'
    types = '<Types xmlns="' + CT + '"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>' + ''.join('<Override PartName="/word/' + name + '.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.' + kind + '+xml"/>' for name, kind in [('document', 'document.main'), ('styles', 'styles'), ('footnotes', 'footnotes'), ('endnotes', 'endnotes')]) + '</Types>'
    return {'[Content_Types].xml': types.encode(), '_rels/.rels': ('<Relationships xmlns="' + P + '"><Relationship Id="office" Type="' + R + '/officeDocument" Target="word/document.xml"/></Relationships>').encode(),
            'word/document.xml': ET.tostring(document, encoding='utf-8', xml_declaration=True), 'word/_rels/document.xml.rels': relationships.encode(),
            'word/styles.xml': styles.encode(), 'word/footnotes.xml': ET.tostring(footnotes, encoding='utf-8', xml_declaration=True),
            'word/endnotes.xml': ET.tostring(endnotes, encoding='utf-8', xml_declaration=True)}


def save_package(parts, file):
    assert not Path(file).exists(), "Never overwrite first evidence"
    with zipfile.ZipFile(file, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, raw in parts.items():
            archive.writestr(name, raw)


def mutate(file, output, mode):
    package = Package(file)
    original = inspect_docx(file)
    root = package.node(package.document)
    candidates = [run for run in root.iter(q("r")) if any("BODY_WEST" in (text.text or "") for text in run.findall("w:t", NS))]
    assert len(candidates) == 1
    run = candidates[0]
    props = run.find("w:rPr", NS)
    if props is None:
        props = ET.Element(q("rPr"))
        run.insert(0, props)
    for child in list(props):
        if child.tag in (q("sz"), q("szCs"), q("cs"), q("rtl")):
            props.remove(child)
    if mode == "native-change":
        ET.SubElement(props, q("sz"), {q("val"): "52"})
        ET.SubElement(props, q("szCs"), {q("val"): "28"})
    else:
        assert mode == "native-clear"
    package.parts[package.document] = ET.tostring(root, encoding="utf-8", xml_declaration=True)
    save_package(package.parts, output)
    after = inspect_docx(output)
    changed = [part for part in original["parts"] if original["parts"][part] != after["parts"][part]]
    assert changed == [package.document]
    assert original["auxiliaryParts"] == after["auxiliaryParts"]
    return {"nativeMutation": mode, "changedParts": changed, "auxiliaryUnchanged": True, "applicationRoundtripCertified": False,
            "inputSha256": original["sha256"], "inspection": after}


def font_points(style):
    found = re.findall(r"(?:^|;)\s*font-size\s*:\s*([0-9]+(?:\.[0-9]+)?)pt\s*(?:;|$)", style)
    assert len(found) <= 1
    return float(found[0]) if found else None


def inspect_epub(file):
    package = zipfile.ZipFile(file)
    with package:
        rows = package.infolist()
        assert len(rows) <= 4096 and sum(row.file_size for row in rows) <= MAX_BYTES
        assert len({row.filename for row in rows}) == len(rows) and package.testzip() is None
        names = {row.filename for row in rows}
        for name in names:
            assert not name.startswith('/') and '\\' not in name and '..' not in PurePosixPath(name).parts
        assert package.read('mimetype') == b'application/epub+zip'
        container_ns = 'urn:oasis:names:tc:opendocument:xmlns:container'
        opf_ns, html_ns, epub_ns = 'http://www.idpf.org/2007/opf', 'http://www.w3.org/1999/xhtml', 'http://www.idpf.org/2007/ops'
        container = xml(package.read('META-INF/container.xml'))
        assert container.tag == '{' + container_ns + '}container'
        rootfiles = container.findall('./{' + container_ns + '}rootfiles/{' + container_ns + '}rootfile')
        assert len(rootfiles) == 1 and rootfiles[0].get('media-type') == 'application/oebps-package+xml'
        root_path = rootfiles[0].get('full-path', '')
        assert root_path and not root_path.startswith('/')
        opf_part = target('', root_path)
        assert opf_part in names
        opf = xml(package.read(opf_part))
        assert opf.tag == '{' + opf_ns + '}package'
        manifests, spines = opf.findall('{' + opf_ns + '}manifest'), opf.findall('{' + opf_ns + '}spine')
        assert len(manifests) == len(spines) == 1
        manifest, by_id, by_part = [], {}, {}
        for item in manifests[0]:
            assert item.tag == '{' + opf_ns + '}item'
            identity, href, media = item.get('id'), item.get('href', ''), item.get('media-type')
            assert identity and identity not in by_id and href and not href.startswith('/')
            part = target(opf_part, href)
            assert part in names and part not in by_part and media
            row = {'id': identity, 'href': href, 'part': part, 'mediaType': media, 'properties': sorted(set(item.get('properties', '').split()))}
            manifest.append(row)
            by_id[identity], by_part[part] = row, row
        nav_parts = {row['part'] for row in manifest if 'nav' in row['properties']}
        assert len(nav_parts) == 1, 'EPUB must declare one actual navigation document'
        assert all(by_part[part]['mediaType'] == 'application/xhtml+xml' for part in nav_parts)
        spine, spine_parts = [], set()
        for item in spines[0]:
            assert item.tag == '{' + opf_ns + '}itemref'
            identity, linear = item.get('idref'), item.get('linear', 'yes')
            assert identity in by_id and linear in ('yes', 'no')
            row = by_id[identity]
            assert row['mediaType'] == 'application/xhtml+xml' and row['part'] not in spine_parts
            spine.append({'idref': identity, 'part': row['part'], 'linear': linear == 'yes'})
            spine_parts.add(row['part'])
        assert spine
        texts, links, identities, toc_counts = [], [], [], {part: 0 for part in nav_parts}
        html_parts = {name for name in names if name.endswith(('.xhtml', '.html'))} | {row['part'] for row in manifest if row['mediaType'] == 'application/xhtml+xml'}
        for part in sorted(html_parts):
            root = xhtml(package.read(part))
            assert root.tag == '{' + html_ns + '}html'
            def visit(node, points=None, in_body=False, navigation=None, ids=()):
                points = font_points(node.get('style', '')) or points
                ids = ids + ((node.get('id'),) if node.get('id') else ())
                in_body = in_body or node.tag == '{' + html_ns + '}body'
                if part in nav_parts and node.tag == '{' + html_ns + '}nav':
                    kinds = set(node.get('{' + epub_ns + '}type', '').split()) & {'toc', 'landmarks', 'page-list'}
                    if kinds:
                        assert len(kinds) == 1 and in_body
                        navigation = next(iter(kinds))
                        if navigation == 'toc':
                            toc_counts[part] += 1
                area = 'navigation' if in_body and navigation else 'body' if in_body and part in spine_parts else 'auxiliary' if in_body else 'metadata'
                if node.get('id'):
                    identities.append({'part': part, 'id': node.get('id'), 'text': ''.join(node.itertext()), 'contentRole': area})
                if node.tag == '{' + html_ns + '}a':
                    if navigation:
                        assert node.get('href'), 'Navigation anchor has no target'
                    if node.get('href'):
                        links.append({'part': part, 'href': node.get('href'), 'id': node.get('id'), 'role': node.get('role'),
                                      'text': ''.join(node.itertext()), 'navigationType': navigation if in_body else None, 'contentRole': area})
                if node.text:
                    texts.append({'part': part, 'text': node.text, 'points': points, 'tag': node.tag.rsplit('}', 1)[-1], 'contentRole': area, 'ids': list(ids)})
                for child in node:
                    visit(child, points, in_body, navigation, ids)
                    if child.tail:
                        texts.append({'part': part, 'text': child.tail, 'points': points, 'tag': node.tag.rsplit('}', 1)[-1], 'contentRole': area, 'ids': list(ids)})
            visit(root)
        assert all(count == 1 for count in toc_counts.values()), 'Declared nav is not an actual EPUB toc'
        # A combined nav/reading document may have ordinary prose outside its
        # real nav elements. It is still body evidence when present in spine.
        body_parts = (spine_parts - nav_parts) | {row['part'] for row in texts if row['contentRole'] == 'body' and row['text'].strip()}
        assert body_parts, 'No actual spine body documents'
        assert len({(row['part'], row['id']) for row in identities}) == len(identities), 'Duplicate current XHTML identity'
        navigation = []
        for link in links:
            if not link['navigationType']:
                continue
            href, separator, fragment = link['href'].partition('#')
            assert not href.startswith('/')
            destination = target(link['part'], href) if href else link['part']
            assert destination in body_parts | nav_parts, 'Navigation target is not a body/nav document'
            fragment = unquote(fragment, errors='strict') if separator and fragment else None
            assert fragment is None or not any(ord(char) < 32 or ord(char) == 127 for char in fragment)
            if fragment is not None:
                matches = [row for row in identities if row['part'] == destination and row['id'] == fragment]
                assert len(matches) == 1, 'Navigation target identity is missing/duplicate'
                destination_role, destination_text = matches[0]['contentRole'], matches[0]['text']
            else:
                destination_role = 'body' if destination in body_parts else 'navigation'
                destination_text = ''.join(row['text'] for row in texts if row['part'] == destination and row['contentRole'] == destination_role)
            if link['navigationType'] == 'toc':
                assert destination in body_parts and destination_role == 'body', 'TOC must target actual reading content'
            navigation.append({**link, 'targetPart': destination, 'fragment': fragment, 'targetContentRole': destination_role, 'targetText': destination_text})
        assert any(row['navigationType'] == 'toc' for row in navigation), 'TOC has no actual targets'
    assert len({(row['part'], row['id']) for row in identities}) == len(identities), 'Duplicate current XHTML identity'
    return {'kind': 'author-inline-epub', 'sha256': digest(Path(file).read_bytes()), 'texts': texts, 'links': links,
            'identities': identities, 'packageDocument': opf_part, 'manifest': manifest, 'spine': spine,
            'bodyParts': sorted(body_parts), 'navigationParts': sorted(nav_parts), 'navigation': navigation,
            'applicationRoundtripCertified': False}


class ExportHtml(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.script = None
        self.scripts = []
    def handle_starttag(self, tag, attrs):
        if tag == 'script':
            assert len(dict(attrs)) == len(attrs), 'Duplicate script attribute'
            self.script = [dict(attrs), '']
    def handle_data(self, value):
        if self.script is not None:
            self.script[1] += value
    def handle_endtag(self, tag):
        if tag == 'script' and self.script is not None:
            self.scripts.append(self.script)
            self.script = None


def inspect_html(file):
    raw = Path(file).read_bytes()
    assert 0 < len(raw) <= MAX_BYTES
    parser = ExportHtml()
    parser.feed(raw.decode('utf-8-sig'))
    assert parser.script is None, 'Unterminated export script'
    source = [(attributes, body) for attributes, body in parser.scripts if attributes.get('id') == 'md-source']
    assert len(source) == 1
    assert source[0][0].get('type') == 'application/json'
    footnotes = [(attributes, body) for attributes, body in parser.scripts if attributes.get('id') == 'readmd-footnote-html']
    assert len(footnotes) <= 1
    markdown = json.loads(source[0][1])
    assert isinstance(markdown, str)
    if footnotes:
        assert footnotes[0][0].get('type') == 'application/json'
    rendered = json.loads(footnotes[0][1]) if footnotes else None
    assert not footnotes or isinstance(rendered, str) and rendered
    models = []
    for attributes, script in parser.scripts:
        if attributes.get('type') == 'application/json':
            continue
        for prefix in ("window.ReadMDAuthorText.apply(root,", "window.ReadMDAuthorText.apply(document.getElementById('content'),"):
            offset = 0
            while (at := script.find(prefix, offset)) >= 0:
                model, end = json.JSONDecoder().raw_decode(script[at + len(prefix):])
                assert script[at + len(prefix) + end:].lstrip().startswith(');')
                if prefix.endswith('(root,'):
                    before = script[:at]
                    assert "document.getElementById('content').innerHTML = html;" in before
                    assert "var root = document.getElementById('content');" in before
                    chosen = "html = JSON.parse(document.getElementById('readmd-footnote-html').textContent);" if rendered is not None else "html = marked.parse(md, { gfm: true, breaks: true });"
                    assert chosen in before, 'HTML payload is not the actual selected render path'
                models.append(model)
                offset = at + len(prefix) + end
    assert len(models) == 1
    return {'kind': 'author-inline-html', 'sha256': digest(raw), 'markdown': markdown, 'model': models[0],
            'renderedHtml': rendered, 'renderedMode': 'typed-footnotes' if rendered is not None else 'locked-marked',
            'actualExportRead': True, 'browserComputedStyleCertified': False}


class RenderedHtml(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack, self.texts, self.keys = [], [], []
        self.links, self.identities, self.by_id = [], [], {}
    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        assert len(attributes) == len(attrs), 'Duplicate HTML attribute'
        if 'data-readmd-author-run' in attributes:
            self.keys.append(attributes['data-readmd-author-run'])
        if attributes.get('id'):
            identity = {'part': 'content.html', 'id': attributes['id'], 'text': '', 'textIndex': len(self.texts)}
            assert identity['id'] not in self.by_id, 'Duplicate current HTML identity'
            self.identities.append(identity)
            self.by_id[identity['id']] = identity
        if tag == 'a' and attributes.get('href'):
            attributes['_oracleLinkIndex'] = len(self.links)
            self.links.append({'part': 'content.html', 'href': attributes['href'], 'id': attributes.get('id'), 'role': attributes.get('role'), 'text': ''})
        if tag not in ('br', 'hr', 'img', 'input', 'meta', 'link', 'source', 'wbr'):
            self.stack.append((tag, attributes))
    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        self.handle_endtag(tag)
    def handle_endtag(self, tag):
        for index in range(len(self.stack) - 1, -1, -1):
            if self.stack[index][0] == tag:
                del self.stack[index:]
                return
    def handle_data(self, value):
        points, key, href = None, None, None
        for _, attributes in self.stack:
            points = font_points(attributes.get('style', '')) or points
            key = attributes.get('data-readmd-author-run', key)
            href = attributes.get('href', href)
            if attributes.get('id'):
                self.by_id[attributes['id']]['text'] += value
            if '_oracleLinkIndex' in attributes:
                self.links[attributes['_oracleLinkIndex']]['text'] += value
        self.texts.append({'text': value, 'tags': [tag for tag, _ in self.stack], 'points': points, 'key': key, 'href': href,
                           'ids': [attributes['id'] for _, attributes in self.stack if attributes.get('id')]})


def inspect_rendered_html(file):
    raw = Path(file).read_bytes()
    assert 0 < len(raw) <= MAX_BYTES
    parser = RenderedHtml()
    parser.feed(raw.decode('utf-8-sig'))
    return {'kind': 'author-inline-rendered-html', 'sha256': digest(raw), 'texts': parser.texts, 'keys': parser.keys,
            'links': parser.links, 'identities': parser.identities,
            'browserComputedStyleCertified': False}


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest='action', required=True)
    inspect = commands.add_parser('inspect')
    inspect.add_argument('file')
    rendered = commands.add_parser('rendered-html')
    rendered.add_argument('file')
    fixture = commands.add_parser('fixture')
    fixture.add_argument('--mode', choices=('positive', 'invalid-size', 'foreign-size', 'field-size'), default='positive')
    fixture.add_argument('--output', required=True)
    change = commands.add_parser('mutate')
    change.add_argument('--mode', choices=('native-change', 'native-clear'), required=True)
    change.add_argument('--input', required=True)
    change.add_argument('--output', required=True)
    args = parser.parse_args()
    if args.action == 'fixture':
        save_package(native_parts(args.mode), args.output)
        # Negative material is deliberately not passed through the strict
        # positive oracle: the real import must diagnose its current XML.
        result = {'nativeOnly': True, 'sourcePayloadPresent': False, 'mode': args.mode,
                  'sha256': digest(Path(args.output).read_bytes()), 'applicationRoundtripCertified': False}
        if args.mode in ('positive', 'field-size'):
            result['inspection'] = inspect_docx(args.output)
    elif args.action == 'rendered-html':
        result = inspect_rendered_html(args.file)
    elif args.action == 'mutate':
        result = mutate(args.input, args.output, args.mode)
    else:
        suffix = Path(args.file).suffix.lower()
        result = {'.docx': inspect_docx, '.epub': inspect_epub, '.html': inspect_html}[suffix](args.file)
    print(json.dumps(result, ensure_ascii=False, allow_nan=False))


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    main()
