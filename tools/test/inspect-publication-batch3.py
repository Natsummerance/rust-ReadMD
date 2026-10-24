"""Independent, read-only inspection of actual P01/P07 DOCX/PDF/EPUB files.

Python and PyMuPDF/pypdf are QA tools only. The product never imports this file.
No document is repaired, rewritten, flattened, or accepted from its API JSON.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET
import zipfile

sys.stdout.reconfigure(encoding="utf-8")


def sha(value):
    return hashlib.sha256(value).hexdigest()


def local(name):
    return name.rsplit("}", 1)[-1]


def leaked_citation_slot(text):
    # Layout CSS and DOCX citation-entry identities are legitimate output.
    # The reserved projection element / data-slot payload is not. Checking a
    # bare class prefix would reject every correctly styled complex EPUB.
    return bool(re.search(r"(?:<|&lt;)\s*readmd-csl(?:\s|/|>|&gt;)|\breadmd-csl\s+data-slot\s*=|\breadmd:csl\b", text))


def xml_values(element):
    return [{"tag": local(node.tag), "attributes": dict(node.attrib),
             "text": "".join(node.itertext())}
            for node in element]


def package_parts(file):
    with zipfile.ZipFile(file) as package:
        assert sum(item.file_size for item in package.infolist()) <= 64 * 1024 * 1024
        names = package.namelist()
        assert len(names) == len(set(names)), "Duplicate ZIP members"
        assert all(not name.startswith("/") and ".." not in Path(name).parts for name in names)
        return {name: package.read(name) for name in names if not name.endswith("/")}


def inspect_docx(file):
    parts = package_parts(file)
    xml = {name: ET.fromstring(value) for name, value in parts.items()
           if name.endswith((".xml", ".rels"))}
    ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
    w = "{" + ns["w"] + "}"
    document = xml["word/document.xml"]
    paragraphs = ["".join(node.itertext()) for node in document.findall(".//w:p", ns)]
    core = {local(node.tag): "".join(node.itertext()) for node in xml["docProps/core.xml"]}
    custom = {}
    for node in xml.get("docProps/custom.xml", []):
        value = "".join(node.itertext())
        try:
            value = json.loads(value)
        except (ValueError, TypeError):
            pass
        custom[node.get("name")] = value
    note_parts = {name: value for name, value in xml.items()
                  if name in ("word/footnotes.xml", "word/endnotes.xml")}
    notes = []
    nested = []
    for name, value in note_parts.items():
        for node in value:
            kind = local(node.tag)
            if kind not in ("footnote", "endnote"):
                continue
            notes.append({"part": name, "id": node.get(w + "id"), "text": "".join(node.itertext())})
            for child in node.iter():
                if local(child.tag) in ("footnoteReference", "endnoteReference"):
                    nested.append({"part": name, "id": node.get(w + "id")})
    formatting = []
    all_xml = []
    links = []
    for name, value in xml.items():
        all_xml.append(parts[name].decode("utf-8"))
        if name.startswith("word/"):
            for node in value.findall(".//w:r", ns):
                properties = node.find("w:rPr", ns)
                formatting.append({"part": name, "text": "".join(node.itertext()),
                                   "properties": {} if properties is None else
                                   {local(p.tag): {local(k): v for k, v in p.attrib.items()} for p in properties}})
        if name.endswith(".rels"):
            links += [{"part": name, **node.attrib} for node in value
                      if node.get("TargetMode") == "External"]
    combined = "\n".join(all_xml)
    custom_relationship = [node.attrib for node in xml["_rels/.rels"]
                           if node.get("Type", "").endswith("/custom-properties")]
    custom_content_type = [node.attrib for node in xml["[Content_Types].xml"]
                           if node.get("PartName") == "/docProps/custom.xml"]
    return {"kind": "docx", "core": core, "custom": custom,
            "customRelationship": custom_relationship, "customContentType": custom_content_type,
            "paragraphs": paragraphs, "text": "\n".join(paragraphs), "notes": notes,
            "nestedNoteReferences": nested, "runs": formatting, "links": links,
            "leakedCitationSlot": leaked_citation_slot(combined),
            "parts": sorted(parts)}


def inspect_epub(file):
    parts = package_parts(file)
    container = ET.fromstring(parts["META-INF/container.xml"])
    rootfile = next(node.get("full-path") for node in container.iter()
                    if local(node.tag) == "rootfile")
    opf = ET.fromstring(parts[rootfile])
    metadata = next(node for node in opf if local(node.tag) == "metadata")
    content = []
    for name, value in parts.items():
        if name.endswith((".xhtml", ".html")):
            tree = ET.fromstring(value)
            content.append({"name": name, "text": "".join(tree.itertext()),
                            "xml": value.decode("utf-8")})
    css = "\n".join(value.decode("utf-8") for name, value in parts.items() if name.endswith(".css"))
    values = xml_values(metadata)
    return {"kind": "epub", "opf": rootfile, "packageAttributes": dict(opf.attrib), "metadata": values,
            "creators": [v for v in values if v["tag"] == "creator"],
            "contributors": [v for v in values if v["tag"] == "contributor"],
            "identifiers": [v for v in values if v["tag"] == "identifier"],
            "content": content, "css": css,
            "text": "\n".join(item["text"] for item in content),
            "leakedCitationSlot": any(leaked_citation_slot(item["xml"]) for item in content),
            "parts": sorted(parts)}


def pdf_xref_structure(data, reader):
    """Prove only the actual final startxref/Prev chain, not objects by Type."""
    from pypdf.generic import ArrayObject, DictionaryObject, NameObject, NumberObject, StreamObject, read_object
    final = re.search(rb"(?:\r\n|\r|\n)startxref\s+(\d+)\s+%%EOF[\t\r\n ]*\Z", data)
    assert final is not None, "Final PDF startxref/EOF is missing"
    offset = int(final.group(1))
    footers = [(int(match.group(1)), match.end()) for match in
               re.finditer(rb"startxref\s+(\d+)\s+%%EOF(?:\r\n|\r|\n)?", data)]
    stream = io.BytesIO(data)
    chain, visited = [], set()
    for _ in range(64):
        assert 0 < offset < len(data) and offset not in visited, "Invalid/cyclic xref chain"
        visited.add(offset)
        assert sum(value == offset for value, _ in footers) == 1, "Ambiguous xref revision footer"
        proof = {"offset": offset, "objectId": None}
        if data[offset:offset + 4] == b"xref":
            position, entries = offset + 4, 0
            while True:
                position += re.match(rb"[\x00\t\n\f\r ]*", data[position:]).end()
                if data[position:position + 7] == b"trailer":
                    position += 7
                    position += re.match(rb"[\x00\t\n\f\r ]*", data[position:]).end()
                    stream.seek(position)
                    value = read_object(stream, reader)
                    assert isinstance(value, DictionaryObject) and not isinstance(value, StreamObject)
                    assert "/XRefStm" not in value, "Hybrid xref is not supported by this QA classification"
                    break
                header = re.match(rb"(\d+)[\t ]+(\d+)[\t ]*(?:\r\n|\r|\n)", data[position:position + 128])
                assert header is not None, "Malformed classic xref subsection"
                count = int(header.group(2))
                entries += count
                assert entries <= 100000, "Classic xref entry budget exceeded"
                position += header.end()
                for _ in range(count):
                    entry = re.match(rb"\d{10}[\t ]\d{5}[\t ][fn][\t ]*(?:\r\n|\r|\n)", data[position:position + 64])
                    assert entry is not None, "Malformed classic xref entry"
                    position += entry.end()
            proof["kind"] = "classic-xref"
        else:
            header = re.match(rb"(\d+)\s+(\d+)\s+obj\b", data[offset:offset + 128])
            assert header is not None, "startxref does not point to an indirect object"
            stream.seek(offset)
            number, generation = reader.read_object_header(stream)
            assert (number, generation) == (int(header.group(1)), int(header.group(2)))
            value = read_object(stream, reader)
            assert isinstance(value, StreamObject) and isinstance(value.get("/Type"), NameObject) and value["/Type"] == "/XRef"
            size, widths = value.get("/Size"), value.get("/W")
            assert isinstance(size, NumberObject) and 0 < size <= 100000
            assert isinstance(widths, ArrayObject) and len(widths) == 3
            assert all(isinstance(width, NumberObject) and width >= 0 for width in widths)
            index = value.get("/Index", ArrayObject([NumberObject(0), size]))
            assert isinstance(index, ArrayObject) and len(index) % 2 == 0
            assert all(isinstance(item, NumberObject) and item >= 0 for item in index)
            entries, end = 0, 0
            for start, count in zip(index[::2], index[1::2]):
                assert start >= end and start + count <= size, "Invalid xref stream Index"
                entries += count
                end = start + count
            expected_bytes = entries * sum(widths)
            assert entries <= 100000 and 0 < expected_bytes <= 4 * 1024 * 1024
            decoded = value.get_data()
            assert len(decoded) == expected_bytes, "Xref stream W/Index/payload mismatch"
            proof.update(kind="xref-stream", objectId=f"{generation}:{number}",
                         encodedSha256=sha(value._data), decodedSha256=sha(decoded))
        previous = value.get("/Prev")
        assert previous is None or isinstance(previous, NumberObject), "Xref Prev is not a direct integer"
        assert previous is None or 0 < previous < offset, "Xref Prev must point strictly backwards"
        proof["previousStartXref"] = None if previous is None else int(previous)
        chain.append(proof)
        if previous is None:
            return {"finalStartXref": chain[0]["offset"], "chain": chain}
        offset = int(previous)
    raise AssertionError("Xref chain depth budget exceeded")


def attach_outline_ink(document, page, evidence, cache, proof, include_visible=True):
    """Bind each actual trace glyph to its actual embedded font outline."""
    import importlib.util
    helper_file = Path(__file__).with_name("publication-smallcaps-proof.py")
    expected = "e4dc26067ecbae5341731bea2a6d3d5591c544281480aba41a4972526f64ef80"
    assert sha(helper_file.read_bytes()) == expected, "Reviewed outline/CID helper changed"
    if "module" not in cache:
        spec = importlib.util.spec_from_file_location("readmd_qa_outline_binding", helper_file)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        cache["module"] = module
    module = cache["module"]
    raw = page.get_texttrace()
    space_fonts = {module.font_key(trace["font"]) for trace in raw
                   if any(char[0] == 32 for char in trace["chars"])}
    resources = page.get_fonts(full=True)
    base_keys = set()
    for resource_record in resources:
        base = resource_record[3]
        plain = re.sub(r"^[A-Z]{6}\+", "", base)
        base_keys.update(module.font_key(name) for name in (base, base[:31], plain, plain[:31]))
    # A trace may use a real program name rather than its PDF BaseFont. Such
    # unmatched spaces still require the original strict embedded-name binding.
    unmatched_spaces = space_fonts - base_keys
    aliases = {}
    for xref, extension, kind, basefont, resource, encoding, *rest in resources:
        plain_base = re.sub(r"^[A-Z]{6}\+", "", basefont)
        base_aliases = (basefont, basefont[:31], plain_base, plain_base[:31])
        needed = include_visible or bool(unmatched_spaces) or any(
            module.font_key(name) in space_fonts for name in base_aliases)
        if needed and xref not in cache:
            name, extracted_extension, subtype, data = document.extract_font(xref)
            if not data:
                assert not include_visible, "Actual ink has no embedded font program"
                # Legal unembedded fonts supply no empty-space proof. Their
                # PDF aliases still take part in resource ambiguity checks.
                cache[xref] = None
                proof.setdefault("unavailableFonts", []).append({
                    "xref": xref, "baseFont": basefont, "reason": "unembedded-font-program"})
            else:
                font = module.OutlineFont(data)
                binding = module.bind_pdf_cids(document, xref, font, data)
                cache[xref] = font
                proof["fonts"].append({"xref": xref, "sha256": font.sha256, "bytes": len(data),
                                       "format": font.format, "cidBinding": binding})
        # Space-only inspection does not require a visible glyph's font program.
        # A later page requesting this resource's space will bind it normally.
        font = cache.get(xref) if needed else None
        assert font is not None or not include_visible, "Actual ink has no embedded font program"
        for name in [basefont, *(font.names if font is not None else ())]:
            plain = re.sub(r"^[A-Z]{6}\+", "", name)
            for alias in (name, name[:31], plain, plain[:31]):
                aliases.setdefault(module.font_key(alias), set()).add(xref)
    evidence["spaceOutlineCharacters"] = []
    assert len(raw) == len(evidence["glyphTraces"])
    for trace, target in zip(raw, evidence["glyphTraces"]):
        assert trace["font"] == target["font"] and trace["seqno"] == target["sequence"]
        assert tuple(trace["dir"]) == (1.0, 0.0), "Unreviewed rotated/sheared actual ink"
        if not include_visible and not any(char[0] == 32 for char in trace["chars"]):
            # This trace requests no outline proof. Keep its exact identity;
            # do not upgrade copy/trace collection into visible-ink validation.
            assert len(trace["chars"]) == len(target["chars"])
            assert all(char["unicode"] == source[0] and char["glyphId"] == source[1]
                       for source, char in zip(trace["chars"], target["chars"]))
            continue
        candidates = aliases.get(module.font_key(trace["font"]), set())
        assert len(candidates) == 1, "Actual trace font resource is missing or ambiguous"
        xref = next(iter(candidates))
        font = cache.get(xref)
        assert len(trace["chars"]) == len(target["chars"])
        for source, char in zip(trace["chars"], target["chars"]):
            unicode_value, gid, origin, bbox = source
            assert char["unicode"] == unicode_value and char["glyphId"] == gid
            if font is None:
                # No outline was requested, or none is available. Preserve the
                # character without a fabricated ink box or space exemption.
                continue
            if unicode_value == 32 and type(gid) is int and gid > 0:
                outline = font.outline(gid)
                if outline["bounds"] is None:
                    advance = font.glyphs[font.glyph_name(gid)].width / font.units
                    assert 0 < advance <= 4, "Unreviewed actual space advance"
                    matches = [copy for copy in evidence["characters"]
                               if copy["c"] == " " and copy["font"] == trace["font"]
                               and all(abs(a - b) <= .0001 for a, b in zip(copy["origin"], origin))]
                    assert len(matches) <= 1, "Ambiguous actual space character identity"
                    if matches:
                        copy = matches[0]
                        copy["emptySpaceOutline"] = {"verified": True, "fontXref": xref,
                            "fontSha256": font.sha256, "outlineSha256": outline["outlineSha256"],
                            "advanceEm": advance, "glyphId": gid, "traceSequence": trace["seqno"],
                            "origin": list(origin), "size": trace["size"], "helperSha256": expected}
                        evidence["spaceOutlineCharacters"].append({"id": copy["id"], "page": copy["page"],
                                                                  "origin": copy["origin"], "bbox": copy["bbox"]})
            if chr(unicode_value).isspace() or unicode_value < 32 or unicode_value == 127:
                continue
            if not include_visible:
                continue
            assert type(gid) is int and gid > 0, "Actual ink has no drawable GID/CID"
            bounds = font.outline(gid)["bounds"]
            assert bounds is not None, "Actual visible glyph has an empty embedded outline"
            x_min, y_min, x_max, y_max = bounds
            size = trace["size"]
            assert size > 0
            char["inkBBox"] = [origin[0] + x_min * size, origin[1] - y_max * size,
                               origin[0] + x_max * size, origin[1] - y_min * size]
            char["inkFontXref"] = xref
            proof["glyphs"] += 1
    proof["helperSha256"] = expected
    proof["verified"] = True


def inspect_pdf(file, render=None, glyphs=False, outline_ink=False):
    import pypdf
    from pypdf import PdfReader
    from pypdf.generic import IndirectObject, StreamObject, DictionaryObject, ArrayObject, ByteStringObject, read_object
    import pymupdf as fitz

    data = file.read_bytes()
    assert len(data) <= 32 * 1024 * 1024
    reader = PdfReader(io.BytesIO(data), strict=True)
    assert not reader.is_encrypted, "Generated publication PDF is unexpectedly encrypted"
    xref_structure = pdf_xref_structure(data, reader)
    catalog = reader.trailer["/Root"]
    metadata_stream = catalog.get("/Metadata")
    xmp = b"" if metadata_stream is None else metadata_stream.get_object().get_data()
    assert len(xmp) <= 1024 * 1024
    tree = None if not xmp else ET.fromstring(xmp)
    dc = "http://purl.org/dc/elements/1.1/"
    rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    def dc_list(name):
        return [] if tree is None else [node.text or "" for node in tree.findall(".//{" + dc + "}" + name + "/.//{" + rdf + "}li")]
    def dc_alt(name):
        return {} if tree is None else {node.get("{http://www.w3.org/XML/1998/namespace}lang", ""): node.text or ""
                                      for node in tree.findall(".//{" + dc + "}" + name + "/.//{" + rdf + "}li")}
    def container(name):
        node = None if tree is None else tree.find(".//{" + dc + "}" + name)
        return None if node is None or not list(node) else local(node[0].tag)
    def canonical(value, depth=0, seen=None):
        assert depth <= 32
        seen = set() if seen is None else seen
        if isinstance(value, IndirectObject):
            key = (value.idnum, value.generation)
            if key in seen:
                return {"cycle": True}
            return canonical(value.get_object(), depth + 1, seen | {key})
        if isinstance(value, StreamObject):
            return {"dictionary": {str(k): canonical(v, depth + 1, seen) for k, v in value.items()
                                    if str(k) not in ("/Length", "/Filter", "/DecodeParms")},
                    "decodedSha256": sha(value.get_data())}
        if isinstance(value, DictionaryObject):
            return {str(k): canonical(v, depth + 1, seen) for k, v in sorted(value.items())}
        if isinstance(value, ArrayObject):
            return [canonical(v, depth + 1, seen) for v in value]
        if isinstance(value, bytes):
            return {"bytesSha256": sha(value)}
        if value is None or isinstance(value, (str, bool, int, float)):
            return value
        return str(value)
    def shallow(value):
        if isinstance(value, IndirectObject):
            return {"indirect": [value.generation, value.idnum]}
        if isinstance(value, StreamObject):
            return {"dictionary": {str(k): shallow(v) for k, v in sorted(value.items())},
                    "encodedSha256": sha(value._data), "decodedSha256": sha(value.get_data())}
        if isinstance(value, DictionaryObject):
            return {str(k): shallow(v) for k, v in sorted(value.items())}
        if isinstance(value, ArrayObject):
            return [shallow(v) for v in value]
        if isinstance(value, bytes):
            return {"bytesSha256": sha(value)}
        if value is None or isinstance(value, (str, bool, int, float)):
            return value
        return str(value)
    metadata_ids = set()
    info_reference = reader.trailer.raw_get("/Info") if "/Info" in reader.trailer else None
    xmp_reference = catalog.raw_get("/Metadata") if "/Metadata" in catalog else None
    for reference in (info_reference, xmp_reference):
        if isinstance(reference, IndirectObject):
            metadata_ids.add((reference.generation, reference.idnum))
    catalog_reference = reader.trailer.raw_get("/Root")
    signatures = {}
    all_signatures = {}
    active_xref_offsets = {}
    reference_targets = set()
    reference_visits = 0
    def collect_references(value, depth=0):
        nonlocal reference_visits
        reference_visits += 1
        assert depth <= 32 and reference_visits <= 100000, "PDF reference scan budget exceeded"
        if isinstance(value, IndirectObject):
            reference_targets.add(f"{value.generation}:{value.idnum}")
        elif isinstance(value, DictionaryObject):
            for child in value.values():
                collect_references(child, depth + 1)
        elif isinstance(value, ArrayObject):
            for child in value:
                collect_references(child, depth + 1)
    object_keys = {(generation, number) for generation, objects in reader.xref.items()
                   for number in objects}
    object_keys.update((0, number) for number in getattr(reader, "xref_objStm", {}))
    assert len(object_keys) <= 100000
    chain_stream_ids = {node["objectId"] for node in xref_structure["chain"] if node["objectId"] is not None}
    for identity in chain_stream_ids:
        generation, number = map(int, identity.split(":"))
        assert (generation, number) in object_keys, "Actual chain xref object has no active mapping"
        assert not any(entries.get(number, False) for entries in reader.xref_free_entry.values()), "Actual chain xref object was freed"
    for generation, number in sorted(object_keys):
        if generation == 65535 or number == 0:
            continue
        identity = f"{generation}:{number}"
        if identity in chain_stream_ids:
            # pypdf may cache an old /Prev stream after reading a newer entry
            # for the same identity. Hash the active mapping's actual bytes.
            offset = reader.xref.get(generation, {}).get(number)
            assert isinstance(offset, int) and 0 < offset < len(data), "Xref stream active mapping is missing"
            active_xref_offsets[identity] = offset
            header = re.match(rb"(\d+)\s+(\d+)\s+obj\b", data[offset:offset + 128])
            assert header is not None and (int(header.group(1)), int(header.group(2))) == (number, generation)
            raw = io.BytesIO(data)
            raw.seek(offset)
            assert reader.read_object_header(raw) == (number, generation)
            value = read_object(raw, reader)
        else:
            value = reader.get_object(IndirectObject(number, generation, reader))
        representation = shallow(value)
        if number == catalog_reference.idnum and generation == catalog_reference.generation:
            representation = {k: v for k, v in representation.items() if k not in ("/Metadata", "/Lang")}
        all_signatures[identity] = sha(json.dumps(representation, ensure_ascii=False, sort_keys=True).encode())
        if (generation, number) not in metadata_ids:
            signatures[identity] = all_signatures[identity]
            collect_references(value)
    page_objects = []
    for page in reader.pages:
        page_objects.append({"mediaBox": list(page.mediabox),
                             "contentSha256": sha(page.get_contents().get_data()),
                             "resourcesSha256": sha(json.dumps(canonical(page.get("/Resources")), ensure_ascii=False, sort_keys=True).encode()),
                             "annotations": canonical(page.get("/Annots", []))})
    xmp_identifier_nodes = [] if tree is None else tree.findall(".//{" + dc + "}identifier")
    xmp_identifiers = [] if tree is None else [node.text or "" for node in tree.findall(".//{http://ns.adobe.com/xap/1.0/}Identifier/.//{" + rdf + "}li")]
    extension_text = "" if tree is None else "\n".join(text for node in tree.iter()
                                                       if not node.tag.startswith(("{" + dc + "}", "{" + rdf + "}"))
                                                       for text in ([node.text] if node.text else []))
    pages = []
    document = fitz.open(file)
    if render:
        render.mkdir(parents=True, exist_ok=True)
    outline_cache = {}
    outline_proof = {"verified": False, "fonts": [], "glyphs": 0}
    assert not outline_ink or glyphs, "Actual outline ink requires physical glyph traces"
    character_id = 0
    for index, page in enumerate(document):
        image = page.get_pixmap(matrix=fitz.Matrix(1, 1), alpha=False)
        if render:
            image.save(render / ("page-" + str(index + 1) + ".png"))
        spans = [span for block in page.get_text("dict")["blocks"] if "lines" in block
                 for line in block["lines"] for span in line["spans"]]
        page_evidence = {"index": index, "width": page.rect.width, "height": page.rect.height,
                      "pixelsSha256": sha(image.samples), "text": page.get_text(),
                      "spans": [{key: span[key] for key in ("text", "font", "size", "flags", "bbox", "origin")} for span in spans],
                      "drawings": [{"bbox": list(drawing["rect"]), "width": drawing.get("width", 0)} for drawing in page.get_drawings()]}
        if glyphs:
            # Copy text and actual character geometry remain separate from
            # physical glyph IDs/outlines. The display QA proves recursive
            # cell order from these actual positions without linearizing two
            # interleaved columns or searching by expected string lengths.
            page_evidence["characters"] = []
            for block in page.get_text("rawdict")["blocks"]:
                for line in block.get("lines", []):
                    for span in line["spans"]:
                        for char in span["chars"]:
                            page_evidence["characters"].append({
                                "id": character_id, "page": index + 1,
                                "c": char["c"], "origin": list(char["origin"]),
                                "bbox": list(char["bbox"]), "font": span["font"],
                                "size": span["size"]})
                            character_id += 1
            page_evidence["glyphTraces"] = [{"font": trace["font"], "size": trace["size"], "sequence": trace["seqno"],
                                             "chars": [{"unicode": value, "glyphId": gid, "origin": list(origin), "bbox": list(bbox)}
                                                       for value, gid, origin, bbox in trace["chars"]]}
                                            for trace in page.get_texttrace()]
            page_evidence["embeddedFonts"] = []
            page_evidence["physicalAstralGlyphs"] = []
            for xref, extension, font_type, base_font, resource, encoding, *rest in page.get_fonts(full=True):
                name, extracted_extension, subtype, data = document.extract_font(xref)
                page_evidence["embeddedFonts"].append({"xref": xref, "name": name, "baseFont": base_font,
                                                       "extension": extracted_extension, "type": subtype,
                                                       "resource": resource, "bytes": len(data), "sha256": sha(data)})
                if data and "DejaVuSans" in base_font.replace(" ", ""):
                    from io import BytesIO
                    from fontTools.ttLib import TTFont
                    from fontTools.pens.boundsPen import BoundsPen
                    subset = TTFont(BytesIO(data))
                    upem = subset["head"].unitsPerEm
                    glyph_order = subset.getGlyphOrder()
                    glyph_set = subset.getGlyphSet()
                    for trace in page_evidence["glyphTraces"]:
                        if "DejaVuSans" not in trace["font"].replace(" ", ""):
                            continue
                        for character in trace["chars"]:
                            if character["unicode"] != 0x1F600:
                                continue
                            gid = character["glyphId"]
                            assert 0 < gid < len(glyph_order), "Astral PDF glyph ID has no embedded outline"
                            name = glyph_order[gid]
                            pen = BoundsPen(glyph_set)
                            glyph_set[name].draw(pen)
                            assert pen.bounds, "Astral PDF glyph outline is empty"
                            x_min, y_min, x_max, y_max = pen.bounds
                            origin_x, origin_y = character["origin"]
                            scale = trace["size"] / upem
                            ink_box = [origin_x + x_min * scale, origin_y - y_max * scale,
                                       origin_x + x_max * scale, origin_y - y_min * scale]
                            ink = page.get_pixmap(matrix=fitz.Matrix(4, 4), clip=fitz.Rect(ink_box), alpha=False)
                            pixels = ink.samples
                            dark = sum(max(pixels[i:i + 3]) < 200 for i in range(0, len(pixels), ink.n))
                            page_evidence["physicalAstralGlyphs"].append({
                                "unicode": character["unicode"], "glyphId": gid, "font": trace["font"],
                                "fontXref": xref, "fontBytes": len(data), "fontSha256": sha(data),
                                "unitsPerEm": upem, "advanceUnits": subset["hmtx"][name][0],
                                "inkBounds": list(pen.bounds), "origin": character["origin"],
                                "pdfInkBox": ink_box, "pixelScale": 4, "darkPixels": dark,
                                "pixelWidth": ink.width, "pixelHeight": ink.height, "pixelsSha256": sha(pixels)})
                    subset.close()
        if glyphs:
            # Copy-space advance can protrude at a native line break without
            # painting ink. Bind that exception to the actual embedded glyph;
            # never infer an empty outline from Unicode alone.
            attach_outline_ink(document, page, page_evidence, outline_cache, outline_proof, outline_ink)
        pages.append(page_evidence)
    document.close()
    return {"kind": "pdf", "info": {str(k): str(v) for k, v in (reader.metadata or {}).items()},
            "language": str(catalog.get("/Lang", "")), "xmp": xmp.decode("utf-8"),
            "dcCreators": dc_list("creator"), "dcSubjects": dc_list("subject"),
            "dcCreatorContainer": container("creator"), "dcSubjectContainer": container("subject"),
            "dcTitle": dc_alt("title"), "dcDescription": dc_alt("description"), "dcRights": dc_alt("rights"),
            "dcIdentifierIsText": all(not list(node) for node in xmp_identifier_nodes),
            "dcIdentifierText": [node.text or "" for node in xmp_identifier_nodes], "xmpIdentifiers": xmp_identifiers,
            "extensionText": extension_text, "pageObjects": page_objects, "nonMetadataObjects": signatures,
            "allObjectSignatures": all_signatures, "metadataObjectIds": [f"{generation}:{number}" for generation, number in sorted(metadata_ids)], "pages": pages,
            "xrefStructure": xref_structure, "xrefActiveObjectOffsets": active_xref_offsets,
            "nonMetadataReferenceTargets": sorted(reference_targets),
            "independentParsers": {"python": sys.version.split()[0], "pypdf": pypdf.__version__, "pymupdf": fitz.VersionBind},
            "text": "\n".join(page["text"] for page in pages),
            "pypdfText": "\n".join(page.extract_text() or "" for page in reader.pages),
            "leakedCitationSlot": any(leaked_citation_slot(page["text"]) for page in pages),
            **({"outlineInk": outline_proof} if outline_ink else {})}


def inspect(file, render=None, glyphs=False, outline_ink=False):
    kind = file.suffix.lower()
    result = inspect_docx(file) if kind == ".docx" else inspect_epub(file) if kind == ".epub" else inspect_pdf(file, render, glyphs, outline_ink)
    result.update(bytes=file.stat().st_size, sha256=sha(file.read_bytes()))
    return result


def compare_pdf_objects(result, baseline):
    metadata_ids = set(result["metadataObjectIds"]) | set(baseline["metadataObjectIds"])
    current_structure, old_structure = result.get("xrefStructure"), baseline.get("xrefStructure")
    structural_ids = set()
    linked = bool(current_structure and old_structure and any(
        node["offset"] == old_structure["finalStartXref"] for node in current_structure["chain"]))
    if linked:
        for node in current_structure["chain"]:
            identity = node["objectId"]
            if (identity is not None and node["kind"] == "xref-stream"
                    and node["offset"] >= baseline["bytes"]
                    and identity not in baseline["allObjectSignatures"]
                    and result["xrefActiveObjectOffsets"].get(identity) == node["offset"]
                    and identity not in result["nonMetadataReferenceTargets"]):
                structural_ids.add(identity)
    excluded = metadata_ids | structural_ids
    ordinary = lambda value: {key: digest for key, digest in value["allObjectSignatures"].items() if key not in excluded}
    superseded = set(baseline["metadataObjectIds"]) - set(result["metadataObjectIds"])
    preserved_metadata = all(result["allObjectSignatures"].get(key) == baseline["allObjectSignatures"][key] for key in superseded)
    return {"sameNonMetadataObjects": ordinary(result) == ordinary(baseline) and preserved_metadata,
            "metadataObjectIdsExcluded": sorted(metadata_ids), "supersededMetadataObjectsUnchanged": preserved_metadata,
            "xrefStructureObjectIdsExcluded": sorted(structural_ids), "xrefChainBaselineLinkVerified": linked}


def selfcheck_comparison():
    import copy
    baseline = {"metadataObjectIds": ["0:7", "0:8"], "allObjectSignatures": {"0:1": "page", "0:2": "font", "0:7": "old-info", "0:8": "old-xmp"}}
    valid = {"metadataObjectIds": ["0:9", "0:10"], "allObjectSignatures": {**baseline["allObjectSignatures"], "0:9": "new-info", "0:10": "new-xmp"}}
    cases = []
    for name in ("unchanged-incremental-metadata", "changed-page", "unexpected-ordinary-object", "deleted-font", "changed-retired-info", "deleted-retired-xmp"):
        current = copy.deepcopy(valid)
        if name == "changed-page": current["allObjectSignatures"]["0:1"] = "changed"
        if name == "unexpected-ordinary-object": current["allObjectSignatures"]["0:11"] = "unexpected"
        if name == "deleted-font": del current["allObjectSignatures"]["0:2"]
        if name == "changed-retired-info": current["allObjectSignatures"]["0:7"] = "changed"
        if name == "deleted-retired-xmp": del current["allObjectSignatures"]["0:8"]
        actual = compare_pdf_objects(current, baseline)["sameNonMetadataObjects"]
        expected = name == "unchanged-incremental-metadata"
        assert actual == expected, name
        cases.append({"case": name, "expected": expected, "actual": actual})
    print(json.dumps({"passed": True, "cases": cases, "scope": "QA object-classification calibration only; not product or native acceptance"}))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("file", type=Path, nargs="?")
    parser.add_argument("--selfcheck-comparison", action="store_true")
    parser.add_argument("--render", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--glyphs", action="store_true")
    parser.add_argument("--outline-ink", action="store_true")
    args = parser.parse_args()
    if args.selfcheck_comparison:
        selfcheck_comparison()
        return
    if args.file is None:
        parser.error("file is required")
    result = inspect(args.file, args.render, args.glyphs, args.outline_ink)
    if args.baseline:
        baseline = inspect(args.baseline)
        assert result["kind"] == baseline["kind"] == "pdf"
        # Incremental metadata legitimately changes the active references and
        # retains the old Info/XMP objects in the original prefix. Classify
        # only the two documents' actual refs, and independently prove every
        # superseded metadata object remains byte-semantically unchanged.
        result["comparison"] = {
            "baselineSha256": baseline["sha256"],
            "originalPrefixUnchanged": args.file.read_bytes().startswith(args.baseline.read_bytes()),
            "samePageObjects": result["pageObjects"] == baseline["pageObjects"],
            **compare_pdf_objects(result, baseline),
            "samePagePixels": [p["pixelsSha256"] for p in result["pages"]] == [p["pixelsSha256"] for p in baseline["pages"]],
        }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
