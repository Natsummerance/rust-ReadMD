"""Independent, bounded inspection of files emitted by the real MCP process.

The product does not import this script.  DOCX inspection uses Python's standard
ZIP/XML parsers; the optional PDF inspection uses the installed PyMuPDF reader.
"""
import hashlib
import json
import math
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
import zipfile

sys.stdout.reconfigure(encoding="utf-8")
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
M = "http://schemas.openxmlformats.org/officeDocument/2006/math"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
NS = {"w": W, "m": M}


def attrs(node):
    return {} if node is None else {key.rsplit("}", 1)[-1]: value for key, value in node.attrib.items()}


def text(node):
    return "".join(n.text or "" for n in node.iter() if n.tag in (f"{{{W}}}t", f"{{{M}}}t"))


def inspect_docx(file):
    with zipfile.ZipFile(file) as package:
        entries = package.infolist()
        assert len(entries) <= 4096 and sum(n.file_size for n in entries) <= 64 * 1024 * 1024
        assert len({n.filename for n in entries}) == len(entries), "Duplicate OPC part"
        assert package.testzip() is None, "Damaged actual ZIP member"
        parts = {}
        for entry in entries:
            if entry.filename.endswith((".xml", ".rels")):
                raw = package.read(entry)
                assert b"<!DOCTYPE" not in raw and b"<!ENTITY" not in raw
                parts[entry.filename] = ET.fromstring(raw)
        document = parts["word/document.xml"]
        body = document.find("w:body", NS)
        rels = {n.get("Id"): dict(n.attrib) for n in parts["word/_rels/document.xml.rels"]}

        def running(reference):
            rel = rels[reference.get(f"{{{R}}}id")]
            assert rel.get("TargetMode") != "External"
            target = "word/" + rel["Target"]
            assert target in parts, "Missing actual running part"
            return {**attrs(reference), "part": target, "text": text(parts[target])}

        sections, owned = [], ""
        for child in body:
            owned += text(child)
            section = child if child.tag == f"{{{W}}}sectPr" else child.find(".//w:sectPr", NS)
            if section is not None:
                sections.append({
                    "text": owned,
                    "parent": "body" if child is section else "pPr",
                    "type": attrs(section.find("w:type", NS)),
                    "page": attrs(section.find("w:pgSz", NS)),
                    "columns": attrs(section.find("w:cols", NS)),
                    "numbering": attrs(section.find("w:pgNumType", NS)),
                    "headers": [running(n) for n in section.findall("w:headerReference", NS)],
                    "footers": [running(n) for n in section.findall("w:footerReference", NS)],
                })
                owned = ""
        paragraphs = []
        for paragraph in body.findall(".//w:p", NS):
            props = paragraph.find("w:pPr", NS)
            paragraphs.append({
                "text": text(paragraph),
                "spacing": attrs(props.find("w:spacing", NS)) if props is not None else {},
                "indent": attrs(props.find("w:ind", NS)) if props is not None else {},
                "keepNext": props is not None and props.find("w:keepNext", NS) is not None,
                "keepLines": props is not None and props.find("w:keepLines", NS) is not None,
                "runs": [{"text": text(run), "size": attrs(run.find("w:rPr/w:sz", NS)),
                          "color": attrs(run.find("w:rPr/w:color", NS))}
                         for run in paragraph.findall("w:r", NS)],
            })
        math = document.findall(".//m:oMath", NS)
        return {"kind": "docx", "reader": {"name": "Python stdlib ZIP/XML", "version": sys.version},
                "parts": sorted(parts), "sections": sections,
                "paragraphs": paragraphs, "math": [text(n) for n in math],
                "fractions": len(document.findall(".//m:f", NS)),
                "radicals": len(document.findall(".//m:rad", NS)),
                "tables": [text(n) for n in document.findall(".//w:tbl", NS)],
                "footnotes": text(parts["word/footnotes.xml"]) if "word/footnotes.xml" in parts else "",
                "settings": ET.tostring(parts["word/settings.xml"], encoding="unicode")}


def resolved_named_goto(document, link, record, fitz):
    """Normalize only an actual PDF GoTo backed by an existing destination.

    PyMuPDF may expose a resolved named GoTo as LINK_NAMED with its point still
    in PDF coordinates. Its resolve_names() also replaces XYZ null with zero,
    so inspect the actual destination array before transforming the point.
    """
    name, xref = link.get("nameddest"), link.get("xref")
    if link.get("kind") != fitz.LINK_NAMED or not isinstance(name, str) or not name:
        return
    if type(xref) is not int or xref <= 0:
        return
    action = document.xref_get_key(xref, "A/S")
    source_key = "A/D" if action == ("name", "/GoTo") else "Dest"
    if action != ("name", "/GoTo") and action != ("null", "null"):
        return  # A named viewer action is not an internal GoTo destination.
    kind, source_name = document.xref_get_key(xref, source_key)
    if kind not in ("string", "name"):
        return
    if kind == "name":
        source_name = source_name.removeprefix("/")
    assert source_name == name, "PDF named GoTo source name mismatch"
    destinations = document.resolve_names()
    assert name in destinations, "Unresolved PDF named GoTo destination"
    destination = destinations[name]
    assert isinstance(destination, dict), "Invalid PDF named GoTo destination"
    page_number = destination.get("page")
    assert type(page_number) is int and 0 <= page_number < len(document), "Invalid PDF named GoTo page"

    pdf = fitz.mupdf.pdf_document_from_fz_document(document)
    array = fitz.mupdf.pdf_lookup_dest(pdf, fitz.mupdf.pdf_new_text_string(name))
    if array.pdf_is_dict():
        array = fitz.mupdf.pdf_dict_gets(array, "D")
    assert array.pdf_is_array(), "Invalid PDF named GoTo array"
    count = fitz.mupdf.pdf_array_len(array)
    assert count >= 2, "Incomplete PDF named GoTo array"
    page_reference, mode = (fitz.mupdf.pdf_array_get(array, i) for i in (0, 1))
    assert page_reference.pdf_is_indirect(), "PDF named GoTo lacks an internal page reference"
    page_xref = page_reference.pdf_to_num()
    assert page_xref == document.page_xref(page_number), "PDF named GoTo page reference mismatch"
    assert mode.pdf_is_name(), "Invalid PDF named GoTo mode"
    if mode.pdf_to_name() != "XYZ":
        return  # Other fit modes do not prove a physical heading point.
    assert count == 5, "Invalid PDF named XYZ destination"
    operands = []
    for i in (2, 3, 4):
        value = fitz.mupdf.pdf_array_get(array, i)
        if value.pdf_is_null():
            operands.append(None)
        else:
            assert value.pdf_is_number(), "Invalid PDF named XYZ operand"
            number = value.pdf_to_real()
            assert math.isfinite(number), "Nonfinite PDF named XYZ operand"
            operands.append(number)
    matrix = list(document[page_number].transformation_matrix)
    assert len(matrix) == 6 and all(math.isfinite(n) for n in matrix), "Invalid PDF page transformation"
    left, top, zoom = operands

    def coordinate(a, b, offset):
        terms = [(left, a), (top, b)]
        if any(value is None and coefficient != 0 for value, coefficient in terms):
            return None
        result = offset + sum(value * coefficient for value, coefficient in terms if coefficient != 0)
        assert math.isfinite(result), "Nonfinite transformed PDF destination"
        return result

    point = [coordinate(matrix[0], matrix[2], matrix[4]), coordinate(matrix[1], matrix[3], matrix[5])]
    record.update({"rawKind": record["kind"], "rawTo": record.get("to"),
                   "kind": fitz.LINK_GOTO, "page": page_number, "to": point,
                   "namedResolution": {"method": "PDF GoTo + Names/Dests + page transformation",
                                       "sourceXref": xref, "sourceKey": source_key, "name": name,
                                       "pageXref": page_xref, "destinationType": "XYZ",
                                       "pdfTo": [left, top], "pdfZoom": zoom,
                                       "pageTransformation": matrix,
                                       "coordinateSpace": "unrotated-page"}})
    record.pop("toDefaultCoordinates", None)
    defaulted = [i for i, value in enumerate(point) if value is None]
    if defaulted:
        record["toDefaultCoordinates"] = defaulted


def pdf_link_record(document, link, fitz):
    record = {key: link[key] for key in ("kind", "page", "name", "nameddest", "uri")
              if key in link and isinstance(link[key], (str, int, float, bool))}
    for key in ("from", "to"):
        if key in link and isinstance(link[key], (fitz.Rect, fitz.Point)):
            coordinates = list(link[key])
            if key == "from":
                assert all(math.isfinite(n) for n in coordinates), "Nonfinite link hit rectangle"
            else:
                # PDF XYZ null leaves that coordinate unchanged. PyMuPDF
                # exposes a null coordinate as NaN; preserve its default
                # meaning as JSON null, never a nonstandard NaN token.
                assert not any(math.isinf(n) for n in coordinates), "Infinite PDF destination"
                defaulted = [i for i, n in enumerate(coordinates) if math.isnan(n)]
                if defaulted:
                    record["toDefaultCoordinates"] = defaulted
                coordinates = [None if math.isnan(n) else n for n in coordinates]
            record[key] = coordinates
    resolved_named_goto(document, link, record, fitz)
    return record


def pdf_character_bounds(page):
    """Return actual reader glyph boxes, without splitting word rectangles."""
    characters = []
    for block_index, block in enumerate(page.get_text("rawdict")["blocks"]):
        for line_index, line in enumerate(block.get("lines", [])):
            for span_index, span in enumerate(line["spans"]):
                size, font = span["size"], span["font"]
                assert isinstance(size, (int, float)) and math.isfinite(size) and size > 0
                assert isinstance(font, str) and font, "Missing PDF character font"
                for character_index, character in enumerate(span["chars"]):
                    bounds, origin, value = list(character["bbox"]), list(character["origin"]), character["c"]
                    assert len(bounds) == 4 and all(math.isfinite(n) for n in bounds), "Nonfinite PDF character bounds"
                    assert bounds[0] <= bounds[2] and bounds[1] <= bounds[3], "Reversed PDF character bounds"
                    assert len(origin) == 2 and all(math.isfinite(n) for n in origin), "Nonfinite PDF character origin"
                    assert isinstance(value, str) and value, "Missing PDF character text"
                    characters.append({"text": value, "bounds": bounds, "origin": origin,
                                       "block": block_index, "line": line_index,
                                       "span": span_index, "index": character_index,
                                       "font": font, "size": size})
    return characters


def inspect_pdf(file, character_bounds=False):
    import pymupdf as fitz
    with fitz.open(file) as document:
        assert not document.is_encrypted and 0 < len(document) <= 256
        def inspect_page(page):
            record = {"width": page.rect.width, "height": page.rect.height,
                      "text": page.get_text(), "words": page.get_text("words"),
                      "links": [pdf_link_record(document, link, fitz) for link in page.get_links()],
                      "drawings": len(page.get_drawings()), "images": len(page.get_images())}
            if character_bounds:
                record["characters"] = pdf_character_bounds(page)
            return record
        return {"kind": "pdf", "reader": {"name": "PyMuPDF", "version": fitz.VersionBind},
                "metadata": document.metadata,
                "outline": document.get_toc(),
                "pages": [inspect_page(page) for page in document]}


def main():
    assert len(sys.argv) in (2, 3) and (len(sys.argv) == 2 or sys.argv[2] == "--character-bounds"), "Expected FILE [--character-bounds]"
    character_bounds = len(sys.argv) == 3
    file = Path(sys.argv[1])
    data = file.read_bytes()
    assert 0 < len(data) <= 64 * 1024 * 1024
    if file.suffix.lower() == ".docx":
        assert not character_bounds, "Character bounds are only available for PDF"
        result = inspect_docx(file)
    elif file.suffix.lower() == ".pdf":
        result = inspect_pdf(file, character_bounds=character_bounds)
    else:
        raise ValueError("Expected an actual DOCX or PDF output")
    result.update({"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)})
    print(json.dumps(result, ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
