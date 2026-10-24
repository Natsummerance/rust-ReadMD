"""QA-only, local OPC mutations. This is not an external-application round trip."""
import argparse
import binascii
import copy
import hashlib
import json
from pathlib import Path
import posixpath
import struct
import sys
import xml.etree.ElementTree as ET
import zipfile
import zlib

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
C = "urn:readmd:citations:1"
PART = "customXml/readmd-citations.xml"
NS = {"w": W, "c": C}
LITERAL = "USER_LITERAL <!--HIDDEN--> <b>NOT_HTML</b> [@same] [FAKE](https://fake.test) *literal* &lt; &amp; :ref[eq:fake] $$x$$"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
for prefix, uri in [("w", W), ("r", R), ("rc", C), ("m", "http://schemas.openxmlformats.org/officeDocument/2006/math")]:
    ET.register_namespace(prefix, uri)


def inspect(entries):
    root = ET.fromstring(entries[PART])
    assert root.tag == "{" + C + "}citations" and root.get("version") == "1"
    resources = json.loads(root.find("c:resources", NS).text)
    assert resources["version"] == 1
    records = []
    for record in root.findall("c:record", NS):
        note = record.find("c:note", NS)
        records.append({**record.attrib, "source": record.find("c:source", NS).text,
                        "original_source": record.find("c:original_source", NS).text,
                        "note": note.attrib if note is not None else None})
    return root, resources, records


def sha256(value):
    return hashlib.sha256(value).hexdigest()


def png_pixel(rgba):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", binascii.crc32(kind + data) & 0xffffffff)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"\0" + bytes(rgba))) + chunk(b"IEND", b"")


def checked_pixel(data):
    assert data.startswith(b"\x89PNG\r\n\x1a\n")
    at, compressed, kinds = 8, b"", []
    while at < len(data):
        size = struct.unpack(">I", data[at:at + 4])[0]
        kind, body = data[at + 4:at + 8], data[at + 8:at + 8 + size]
        assert len(body) == size
        assert struct.unpack(">I", data[at + 8 + size:at + 12 + size])[0] == binascii.crc32(kind + body) & 0xffffffff
        kinds.append(kind)
        if kind == b"IHDR":
            assert struct.unpack(">IIBBBBB", body) == (1, 1, 8, 6, 0, 0, 0)
        if kind == b"IDAT":
            compressed += body
        at += 12 + size
    assert at == len(data) and kinds[0] == b"IHDR" and kinds[-1] == b"IEND"
    row = zlib.decompress(compressed)
    assert len(row) == 5 and row[0] == 0
    return list(row[1:])


def expanded(name):
    return name[1:].split("}", 1) if name.startswith("{") else ("", name)


def legacy_signature(node):
    """Independent implementation of the documented v1 XML-only digest."""
    def semantic(element):
        namespace, local = expanded(element.tag)
        attrs = {uri + ":" + name: value for key, value in element.attrib.items()
                 for uri, name in [expanded(key)] if not name.startswith("rsid") and not (uri == "http://www.w3.org/XML/1998/namespace" and name == "space")}
        children = []
        def text(value):
            if value and (local == "t" or value.strip()):
                children.append({"text": value})
        text(element.text)
        for child in element:
            uri, name = expanded(child.tag)
            if not (uri == W and name in ("proofErr", "bookmarkStart", "bookmarkEnd", "lastRenderedPageBreak")):
                children.append(semantic(child))
            text(child.tail)
        normalized = []
        for child in children:
            if isinstance(child, list) and child[:2] == [W, "r"]:
                properties = [item for item in child[3] if isinstance(item, list) and item[1] == "rPr"]
                content = [item for item in child[3] if not (isinstance(item, list) and item[1] == "rPr")]
                if all(isinstance(item, list) and item[1] == "t" for item in content):
                    value = "".join(atom["text"] for item in content for atom in item[3] if isinstance(atom, dict) and "text" in atom)
                    if normalized and isinstance(normalized[-1], dict) and normalized[-1].get("run_properties") == properties and "run_text" in normalized[-1]:
                        normalized[-1]["run_text"] += value
                    else:
                        normalized.append({"run_properties": properties, "run_text": value})
                    continue
            normalized.append(child)
        return [namespace, local, attrs, normalized]
    return sha256(json.dumps(semantic(node), ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8"))


def downgrade_legacy(entries, manifest):
    trees = {name: ET.fromstring(entries[name]) for name in ("word/document.xml", "word/footnotes.xml", "word/endnotes.xml") if name in entries}
    for record in manifest.findall("c:record", NS):
        prefix = "readmd:bibliography:v1:" if record.get("kind") == "bibliography" else "readmd:citation:v1:"
        controls = [control for control in trees[record.get("part")].findall(".//w:sdt", NS)
                    if control.find("w:sdtPr/w:tag", NS) is not None and control.find("w:sdtPr/w:tag", NS).get("{" + W + "}val") == prefix + record.get("id")]
        assert len(controls) == 1
        record.set("render_sha256", legacy_signature(controls[0].find("w:sdtContent", NS)))
        record.attrib.pop("render_context_version", None)
        note = record.find("c:note", NS)
        note_debug = "None"
        if note is not None:
            end = note.get("end") == "true"
            part, kind = ("word/endnotes.xml", "endnote") if end else ("word/footnotes.xml", "footnote")
            story = next(row for row in trees[part].findall("w:" + kind, NS) if row.get("{" + W + "}id") == note.get("id"))
            wrapper = ET.Element("{" + W + "}sdtContent")
            wrapper.append(copy.deepcopy(story))
            note.set("story_sha256", legacy_signature(wrapper))
            note_debug = 'Some((' + note.get("end") + ', ' + note.get("id") + ', "' + note.get("story_sha256") + '"))'
        fields = [record.get("id"), record.get("kind"), record.get("part"), record.find("c:original_source", NS).text, record.get("render_sha256"), record.get("generated"), note_debug]
        record.set("binding", sha256(("ReadMD citation binding 1\0" + "\0".join(fields)).encode("utf-8")))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True)
    parser.add_argument("--output")
    parser.add_argument("--mode", choices=["inspect", "source", "render", "mixed", "bibliography", "note-extra", "duplicate", "bad-resource", "move-story", "literal", "mixed-literal", "bibliography-literal", "relationship-target", "relationship-unrelated", "relationship-legacy", "media-bytes", "media-target", "foreign-text"], default="inspect")
    parser.add_argument("--index", type=int, default=1)
    parser.add_argument("--source", default="[compare @42, p. 99 **changed**]")
    args = parser.parse_args()
    assert Path(args.input).stat().st_size <= 64 * 1024 * 1024
    with zipfile.ZipFile(args.input) as package:
        names = package.namelist()
        assert len(names) <= 20000 and len(set(names)) == len(names)
        assert sum(row.file_size for row in package.infolist()) <= 64 * 1024 * 1024
        entries = {name: package.read(name) for name in names}
    root, resources, records = inspect(entries)
    summary = {"mode": args.mode, "applicationRoundtripCertified": False,
               "headlessPackageMutation": True, "records": records,
               "resourceNames": [row["name"] for row in resources["files"]]}
    if args.mode == "inspect":
        print(json.dumps(summary, ensure_ascii=False))
        return
    assert args.output and not Path(args.output).exists()
    candidates = [row for row in records if row["kind"] == "citation" and row["part"] == "word/document.xml"]
    assert candidates and 0 <= args.index < len(candidates)
    selected = candidates[args.index]
    record = next(row for row in root.findall("c:record", NS) if row.get("id") == selected["id"])
    if args.mode == "relationship-legacy":
        downgrade_legacy(entries, root)
        summary["legacyXmlOnlySignatures"] = True
    if args.mode in ("source", "mixed", "mixed-literal"):
        record.find("c:source", NS).text = args.source
    if args.mode == "bad-resource":
        resources["files"][0]["sha256"] = "0" * 64
        root.find("c:resources", NS).text = json.dumps(resources, ensure_ascii=False)
    document = ET.fromstring(entries["word/document.xml"])
    body = document.find("w:body", NS)
    assert body is not None
    def body_paragraph():
        paragraph = ET.Element("{" + W + "}p")
        final_section = next((i for i, node in enumerate(body) if node.tag == "{" + W + "}sectPr"), len(body))
        body.insert(final_section, paragraph)
        return paragraph
    def find_control(tree, row):
        prefix = "readmd:bibliography:v1:" if row["kind"] == "bibliography" else "readmd:citation:v1:"
        found = [node for node in tree.findall(".//w:sdt", NS)
                 if node.find("w:sdtPr/w:tag", NS) is not None
                 and node.find("w:sdtPr/w:tag", NS).get("{" + W + "}val") == prefix + row["id"]]
        assert len(found) == 1
        return found[0]
    if args.mode == "foreign-text":
        row = selected
        if row["generated"] == "true" and row["note"]:
            row = next(candidate for candidate in records if candidate["part"] != "word/document.xml" and candidate["generated"] == "true" and candidate["note"] == row["note"])
        part = row["part"]
        tree = document if part == "word/document.xml" else ET.fromstring(entries[part])
        control = find_control(tree, row)
        candidates = [node for node in control.findall("w:sdtContent//w:t", NS) if node.text and "Alpha" in node.text]
        assert len(candidates) == 1
        target = candidates[0]
        original_text = target.text
        target.tag = "{urn:readmd:qa:foreign-citation-text}t"
        remaining = "".join(node.text or "" for node in control.findall("w:sdtContent//w:t", NS))
        assert remaining.strip() and any(word in remaining for word in ("evidence", "see"))
        if part != "word/document.xml":
            entries[part] = ET.tostring(tree, encoding="utf-8", xml_declaration=True)
        summary.update({"foreignTextStory": part, "foreignTextBinding": "readmd:citation:v1:" + row["id"],
                        "originalSource": row["original_source"], "foreignNodeText": original_text,
                        "remainingWordText": remaining, "foreignNamespace": "urn:readmd:qa:foreign-citation-text",
                        "manifestSha256Before": sha256(entries[PART]), "manifestUnchanged": True,
                        "signatureMetadataUnchanged": True, "renamedTextOnlyInsideControl": True})
    if args.mode == "relationship-target":
        row = selected
        if row["generated"] == "true" and row["note"]:
            row = next(candidate for candidate in records if candidate["part"] != "word/document.xml" and candidate["generated"] == "true" and candidate["note"] == row["note"])
        part = row["part"]
        control = find_control(ET.fromstring(entries[part]), row)
        links = control.findall(".//w:hyperlink", NS)
        assert len(links) == 1
        rid = links[0].get("{" + R + "}id")
        rel_part = posixpath.dirname(part) + "/_rels/" + posixpath.basename(part) + ".rels"
        rels = ET.fromstring(entries[rel_part])
        relations = [node for node in rels if node.get("Id") == rid]
        assert len(relations) == 1 and relations[0].get("TargetMode") == "External"
        assert relations[0].get("Target") == "https://old.example/"
        relations[0].set("Target", "https://new.example/")
        entries[rel_part] = ET.tostring(rels, encoding="utf-8", xml_declaration=True)
        summary.update({"relationshipStory": part, "relationshipId": rid, "oldTarget": "https://old.example/", "newTarget": "https://new.example/", "selectedControlXmlUnchanged": True})
    if args.mode == "relationship-unrelated":
        rel_part = "word/_rels/document.xml.rels"
        rels = ET.fromstring(entries[rel_part])
        relations = [node for node in rels if node.get("Target") == "https://unrelated.example/"]
        assert len(relations) == 1
        relations[0].set("Target", "https://changed-unrelated.example/")
        entries[rel_part] = ET.tostring(rels, encoding="utf-8", xml_declaration=True)
        summary["changedUnrelatedRelationship"] = relations[0].get("Id")
    if args.mode in ("media-bytes", "media-target"):
        part = "word/footnotes.xml"
        notes = ET.fromstring(entries[part])
        blips = notes.findall(".//{http://schemas.openxmlformats.org/drawingml/2006/main}blip")
        assert len(blips) == 1
        rid = blips[0].get("{" + R + "}embed")
        rel_part = "word/_rels/footnotes.xml.rels"
        rels = ET.fromstring(entries[rel_part])
        relations = [node for node in rels if node.get("Id") == rid]
        assert len(relations) == 1 and relations[0].get("TargetMode", "Internal") == "Internal"
        relation = relations[0]
        old_target = relation.get("Target")
        assert not any(char in old_target for char in (":", "\\", "?", "#"))
        member = posixpath.normpath(posixpath.join("word", old_target))
        assert member.startswith("word/media/") and member in entries
        old_bytes = entries[member]
        assert checked_pixel(old_bytes) == [220, 30, 40, 255]
        new_bytes = png_pixel([20, 80, 230, 255])
        assert checked_pixel(new_bytes) == [20, 80, 230, 255] and new_bytes != old_bytes
        if args.mode == "media-bytes":
            entries[member] = new_bytes
        else:
            member = "word/media/current-citation-pixel.png"
            assert member not in entries
            entries[member] = new_bytes
            relation.set("Target", "media/current-citation-pixel.png")
            entries[rel_part] = ET.tostring(rels, encoding="utf-8", xml_declaration=True)
        summary.update({"mediaScope": "authored-manual-note-resource-preservation", "sourceOwnedCitationImageBindingCertified": False,
                        "storyXmlUnchanged": True, "relationshipId": rid, "oldTarget": old_target, "newTarget": relation.get("Target"),
                        "oldMediaSha256": sha256(old_bytes), "newMediaSha256": sha256(new_bytes), "newMediaMember": member,
                        "oldPixel": checked_pixel(old_bytes), "newPixel": checked_pixel(new_bytes), "pngCrcAndZlibVerified": True})
    if args.mode == "move-story":
        control = find_control(document, selected)
        parent = next(node for node in document.iter() if control in list(node))
        parent.remove(control)
        notes = ET.fromstring(entries["word/footnotes.xml"])
        story = next(node for node in notes.findall("w:footnote", NS) if not node.get("{" + W + "}type"))
        paragraph = story.find("w:p", NS)
        assert paragraph is not None
        paragraph.append(control)
        entries["word/footnotes.xml"] = ET.tostring(notes, encoding="utf-8", xml_declaration=True)
    if args.mode in ("literal", "mixed-literal", "bibliography-literal"):
        row = next(row for row in records if row["kind"] == "bibliography") if args.mode == "bibliography-literal" else selected
        control = find_control(document, row)
        content = control.find("w:sdtContent", NS)
        assert content is not None
        content.clear()
        container = ET.SubElement(content, "{" + W + "}p") if args.mode == "bibliography-literal" else content
        run = ET.SubElement(container, "{" + W + "}r")
        ET.SubElement(ET.SubElement(run, "{" + W + "}rPr"), "{" + W + "}b")
        ET.SubElement(run, "{" + W + "}t").text = LITERAL
        link = ET.SubElement(container, "{" + W + "}hyperlink", {"{" + R + "}id": "literalRealLink"})
        run = ET.SubElement(link, "{" + W + "}r")
        ET.SubElement(ET.SubElement(run, "{" + W + "}rPr"), "{" + W + "}i")
        ET.SubElement(run, "{" + W + "}t").text = " REAL_LINK [literal] "
        run = ET.SubElement(container, "{" + W + "}r")
        ET.SubElement(ET.SubElement(run, "{" + W + "}rPr"), "{" + W + "}rFonts", {"{" + W + "}ascii": "Consolas"})
        ET.SubElement(run, "{" + W + "}t").text = "`CODE` [@same]"
        rel_part = "word/_rels/document.xml.rels"
        rels = ET.fromstring(entries[rel_part])
        assert not any(node.get("Id") == "literalRealLink" for node in rels)
        ET.SubElement(rels, "{" + PKG + "}Relationship", {"Id": "literalRealLink", "Type": R + "/hyperlink", "Target": "https://example.test/real?a=1&b=2", "TargetMode": "External"})
        entries[rel_part] = ET.tostring(rels, encoding="utf-8", xml_declaration=True)
        summary["literalText"] = LITERAL
    if args.mode in ("render", "mixed", "bibliography"):
        row = next(row for row in records if row["kind"] == "bibliography") if args.mode == "bibliography" else selected
        control = find_control(document, row)
        text = control.find("w:sdtContent//w:t", NS)
        assert text is not None
        text.text = "USER_BIBLIOGRAPHY" if args.mode == "bibliography" else "USER_CITATION_EDIT_中文"
    if args.mode == "duplicate":
        control = find_control(document, selected)
        paragraph = body_paragraph()
        paragraph.append(copy.deepcopy(control))
    if args.mode == "note-extra":
        row = next(row for row in candidates if row["generated"] == "true" and row["note"])
        note = row["note"]
        end = note["end"] == "true"
        part = "word/endnotes.xml" if end else "word/footnotes.xml"
        notes = ET.fromstring(entries[part])
        kind = "endnote" if end else "footnote"
        story = next(node for node in notes.findall("w:" + kind, NS) if node.get("{" + W + "}id") == note["id"])
        paragraph = ET.SubElement(story, "{" + W + "}p")
        ET.SubElement(ET.SubElement(paragraph, "{" + W + "}r"), "{" + W + "}t").text = "USER_NOTE_OUTSIDE_CONTROL_中文"
        entries[part] = ET.tostring(notes, encoding="utf-8", xml_declaration=True)
    paragraph = body_paragraph()
    ET.SubElement(ET.SubElement(paragraph, "{" + W + "}r"), "{" + W + "}t").text = "DOCX_RETURN_EDITED_BODY_中文"
    entries["word/document.xml"] = ET.tostring(document, encoding="utf-8", xml_declaration=True)
    if args.mode != "foreign-text":
        entries[PART] = ET.tostring(root, encoding="utf-8", xml_declaration=True)
    else:
        summary["manifestSha256After"] = sha256(entries[PART])
        assert summary["manifestSha256Before"] == summary["manifestSha256After"]
    with zipfile.ZipFile(args.output, "x", compression=zipfile.ZIP_DEFLATED) as output:
        for name, value in entries.items():
            output.writestr(name, value)
    summary["selectedId"] = selected["id"]
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
