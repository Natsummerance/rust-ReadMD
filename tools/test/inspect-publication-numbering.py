"""Read actual OPC numbering/fields with Python's independent ZIP/XML parsers.

No Office process is started. Cached field text and the native field definition
are reported separately; this is not a Word field-update or lossless-readback
certificate.
"""
import hashlib
import importlib.util
import struct
import zlib
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
import zipfile

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W}
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
WP = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
_spec = importlib.util.spec_from_file_location("numbering_opc", Path(__file__).with_name("inspect-docx-settings-roundtrip.py"))
opc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(opc)


def png_pixels(raw):
    # This bounded QA fixture is a real noninterlaced RGBA8 PNG. Verify the
    # original compressed bytes and decoded pixel; never ask the SUT's codec.
    assert raw.startswith(b"\x89PNG\r\n\x1a\n") and len(raw) <= 1024 * 1024, "PNG signature/budget"
    at, chunks, compressed, dimensions, pixel_format, finished = 8, [], bytearray(), None, None, False
    while at < len(raw):
        assert at + 12 <= len(raw) and not finished, "PNG chunk boundary"
        length = struct.unpack(">I", raw[at:at + 4])[0]
        kind = raw[at + 4:at + 8]
        assert at + 12 + length <= len(raw), "PNG chunk length"
        data = raw[at + 8:at + 8 + length]
        crc = struct.unpack(">I", raw[at + 8 + length:at + 12 + length])[0]
        assert zlib.crc32(kind + data) & 0xffffffff == crc, "PNG chunk CRC: " + kind.decode("ascii", "replace")
        chunks.append({"type": kind.decode("ascii"), "crc": f"{crc:08x}"})
        if kind == b"IHDR":
            assert dimensions is None and len(chunks) == 1 and length == 13, "PNG IHDR"
            width, height, depth, colour, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            dimensions = [width, height]
            pixel_format = (depth, colour, compression, filtering, interlace)
        elif kind == b"IDAT":
            assert dimensions is not None and not any(row["type"] not in ("IHDR", "IDAT") for row in chunks[:-1]), "PNG IDAT order"
            compressed.extend(data)
        elif kind == b"IEND":
            assert length == 0 and compressed, "PNG IEND"
            finished = True
        else:
            raise AssertionError("Unexpected QA PNG chunk")
        at += 12 + length
    assert finished and at == len(raw), "PNG completion"
    # Verify every chunk's integrity before interpreting this fixture format.
    assert dimensions == [1, 1] and pixel_format == (8, 6, 0, 0, 0), "QA PNG must be 1x1 RGBA8"
    decoder = zlib.decompressobj()
    pixels = decoder.decompress(bytes(compressed), 6)
    assert decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail, "PNG IDAT compression"
    assert len(pixels) == 5 and pixels[0] == 0, "PNG actual scanline/filter"
    return {"dimensions": dimensions, "rgba": list(pixels[1:]), "chunks": chunks, "allChunkCrcValid": True}


def visible_metrics(paragraph, styles):
    native = opc.paragraph(paragraph, styles)
    runs = [run for run in paragraph.iter(f"{{{W}}}r") if run.findall("w:t", NS)]
    assert len(runs) == len(native["runs"])
    props = {id(run): row["properties"] for run, row in zip(runs, native["runs"])}
    values, active, bookmarks, stack = {}, {}, [], []
    def walk(node, run=None):
        if node.tag in (f"{{{W}}}pPr", f"{{{W}}}rPr"):
            return
        if node.tag == f"{{{W}}}r":
            run = node
        if node.tag == f"{{{W}}}fldChar":
            kind = value(node, "fldCharType")
            if kind == "begin": stack.append(False)
            elif kind == "separate" and stack: stack[-1] = True
            elif kind == "end" and stack: stack.pop()
        elif node.tag == f"{{{W}}}bookmarkStart":
            identity = value(node, "id")
            assert identity not in active, "Duplicate active bookmark"
            active[identity] = {"id": identity, "name": value(node, "name"), "text": ""}
        elif node.tag == f"{{{W}}}bookmarkEnd":
            identity = value(node, "id")
            assert identity in active, "Unpaired bookmark end"
            bookmarks.append(active.pop(identity))
        elif node.tag == f"{{{W}}}t" and all(stack):
            fragment = node.text or ""
            if run is not None and fragment:
                row = values.setdefault(id(run), {"text": "", "properties": props[id(run)]})
                row["text"] += fragment
            for bookmark in active.values(): bookmark["text"] += fragment
        for child in node: walk(child, run)
    walk(paragraph)
    assert not stack and not active, "Unclosed field/bookmark in numbered fixture"
    return list(values.values()), bookmarks


def drawing_images(paragraph, package, body_index):
    result = []
    relationships = {row["id"]: row for row in package.relationships(package.document)}
    for drawing in paragraph.iter(f"{{{W}}}drawing"):
        descriptions = list(drawing.iter(f"{{{WP}}}docPr"))
        blips = list(drawing.iter(f"{{{A}}}blip"))
        assert len(descriptions) == len(blips) == 1, "One native drawing description and blip required"
        identity = blips[0].get(f"{{{R}}}embed")
        assert identity and identity in relationships, "Missing image relationship"
        rel = relationships[identity]
        assert rel["mode"] == "Internal" and rel["type"] in (R + "/image", "http://purl.oclc.org/ooxml/officeDocument/relationships/image"), "Non-image/external relationship"
        member = opc.member(package.document, rel["target"])
        assert member in package.parts, "Missing image payload"
        raw = package.parts[member]
        description = descriptions[0]
        result.append({"bodyIndex": body_index, "relationshipId": identity, "relationshipType": rel["type"],
                       "targetMode": rel["mode"], "member": member, "sha256": hashlib.sha256(raw).hexdigest(),
                       "bytes": len(raw), "docPrId": description.get("id"),
                       "descriptionPresent": "descr" in description.attrib, "description": description.get("descr"),
                       "png": png_pixels(raw)})
    return result


def value(node, name="val"):
    return None if node is None else node.get(f"{{{W}}}{name}")


def text(node):
    # An inner SEQ result can occur in the outer formula's instruction area.
    # That ordinal is part of the field code, not an additional visible glyph.
    stack, output = [], []
    for child in node.iter():
        if child.tag == f"{{{W}}}fldChar":
            kind = value(child, "fldCharType")
            if kind == "begin":
                stack.append(False)
            elif kind == "separate" and stack:
                stack[-1] = True
            elif kind == "end" and stack:
                stack.pop()
        elif child.tag == f"{{{W}}}t" and all(stack):
            output.append(child.text or "")
    assert not stack, "Unclosed complex field in visible text"
    return "".join(output)


def fields(paragraph):
    result = [{"instruction": value(n, "instr"), "cached": text(n), "kind": "simple", "depth": 0}
              for n in paragraph.iter(f"{{{W}}}fldSimple")]
    stack = []
    for node in paragraph.iter():
        if node.tag == f"{{{W}}}fldChar":
            kind = value(node, "fldCharType")
            if kind == "begin":
                stack.append({"instruction": "", "cached": "", "kind": "complex", "depth": len(stack), "separated": False})
            elif kind == "separate" and stack:
                stack[-1]["separated"] = True
            elif kind == "end" and stack:
                field = stack.pop()
                field.pop("separated")
                result.append(field)
        elif node.tag == f"{{{W}}}instrText" and stack:
            stack[-1]["instruction"] += node.text or ""
        elif node.tag == f"{{{W}}}t":
            for field in stack:
                if field["separated"]:
                    field["cached"] += node.text or ""
    assert not stack, "Unclosed native field"
    return result


def inspect(file):
    data = file.read_bytes()
    assert 0 < len(data) <= 64 * 1024 * 1024
    with zipfile.ZipFile(file) as package:
        names = package.namelist()
        assert len(names) <= 4096 and len(names) == len(set(names))
        assert sum(n.file_size for n in package.infolist()) <= 64 * 1024 * 1024
        assert package.testzip() is None
    package = opc.Package(file)
    styles = opc.Styles(package)
    parts = {"word/document.xml": package.xml(package.document),
             "word/numbering.xml": package.xml(package.related("numbering", optional=False)),
             "word/settings.xml": package.xml(package.related("settings", optional=False))}
    abstract = {}
    for definition in parts["word/numbering.xml"].findall("w:abstractNum", NS):
        levels = []
        for level in definition.findall("w:lvl", NS):
            levels.append({"level": int(value(level, "ilvl")),
                           "format": value(level.find("w:numFmt", NS)),
                           "start": value(level.find("w:start", NS)),
                           "restart": value(level.find("w:lvlRestart", NS)),
                           "text": value(level.find("w:lvlText", NS)),
                           "suffix": value(level.find("w:suff", NS)),
                           "style": value(level.find("w:pStyle", NS))})
        abstract[value(definition, "abstractNumId")] = levels
    numbers = {}
    for number in parts["word/numbering.xml"].findall("w:num", NS):
        numbers[value(number, "numId")] = {
            "abstract": value(number.find("w:abstractNumId", NS)),
            "overrides": [{"level": value(n, "ilvl"), "start": value(n.find("w:startOverride", NS))}
                          for n in number.findall("w:lvlOverride", NS)]}
    paragraphs, images = [], []
    body = parts["word/document.xml"].find("w:body", NS)
    assert body is not None
    for body_index, paragraph in enumerate(body):
        if paragraph.tag != f"{{{W}}}p": continue
        actual_runs, bookmark_ranges = visible_metrics(paragraph, styles)
        drawings = drawing_images(paragraph, package, body_index)
        images.extend(drawings)
        props = paragraph.find("w:pPr", NS)
        style = value(props.find("w:pStyle", NS)) if props is not None else None
        num = props.find("w:numPr", NS) if props is not None else None
        paragraphs.append({"style": style, "text": text(paragraph),
                           "number": None if num is None else {
                               "id": value(num.find("w:numId", NS)),
                               "level": value(num.find("w:ilvl", NS))},
                           "fields": fields(paragraph),
                           "bookmarks": [value(n, "name") for n in paragraph.findall("w:bookmarkStart", NS)],
                           "bodyIndex": body_index, "visibleRuns": actual_runs,
                           "bookmarkRanges": bookmark_ranges, "images": drawings})
    return {"kind": "docx-numbering", "reader": "Python stdlib ZIP/XML",
            "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
            "abstract": abstract, "numbers": numbers, "paragraphs": paragraphs, "images": images,
            "bodyVisibleText": text(body), "tableCount": len(list(body.iter(f"{{{W}}}tbl"))),
            "mathTexts": [node.text or "" for node in body.iter("{http://schemas.openxmlformats.org/officeDocument/2006/math}t")],
            "updateFields": value(parts["word/settings.xml"].find("w:updateFields", NS)),
            "wordApplicationRoundtripCertified": False}


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    print(json.dumps(inspect(Path(sys.argv[1])), ensure_ascii=False))
