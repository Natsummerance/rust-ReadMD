"""Finite, independent whole-PDF baseline oracle. No product or engine calls.

The contract is a fixed complete visible corpus, not product source identities.
Embedded TrueType outlines bind GIDs and real ink. Empty-outline whitespace is
compared completely but is not claimed to paint at a shifted baseline.
"""
import argparse
import hashlib
import importlib.util
import io
import json
import math
from pathlib import Path
import re
import struct
import sys
import zlib

TOLERANCE = .002
SCALE = 4
MAX_BYTES = 16 * 1024 * 1024
MAX_EXPANDED = 64 * 1024 * 1024
MARKERS = ("readmd-internal:publication/", "readmd:page-source", "readmd:baseline", "readmd-baseline-")
# C54 and C72 retain the original WinRT response and exact PDF. Every ink
# pixel still needs a local match. Sparse font hinting may use the second
# pixel only within a separately counted, bidirectional 0.1% strong-pixel
# budget; this is not permission to omit ink or register/translate an image.
PREVIEW_AA = {"weakContrast": 8, "strongContrast": 32, "coreContrast": 192,
              "strongRadiusPx": 1, "fringeRadiusPx": 2,
              "hintingRadiusPx": 2, "maxHintingFraction": 0.001,
              "coreRgbDelta": 64, "edgeRgbDelta": 128}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def close(a, b, description="coordinate"):
    assert isinstance(a, (int, float)) and isinstance(b, (int, float))
    assert math.isfinite(a) and math.isfinite(b) and abs(a - b) <= TOLERANCE, (description, a, b)


def clean_pdf(file):
    """Include orphaned and compressed objects; a catalog-only scan is unsafe."""
    from pypdf import PdfReader
    from pypdf.generic import IndirectObject
    raw = file.read_bytes()
    assert 0 < len(raw) <= MAX_BYTES
    needles = [marker.encode(encoding) for marker in MARKERS for encoding in ("utf-8", "utf-16-be", "utf-16-le")]
    def clean(data):
        assert not any(needle in data for needle in needles), "Private publication mapping survived"
    clean(raw)
    reader, seen, expanded = PdfReader(file), set(), 0
    def scan(value, depth=0):
        nonlocal expanded
        assert depth < 64, "PDF object nesting budget"
        if hasattr(value, "idnum"):
            key = value.idnum, value.generation
            if key in seen:
                return
            seen.add(key)
            value = value.get_object()
        if isinstance(value, dict):
            for key, item in value.items():
                scan(str(key), depth + 1)
                scan(item, depth + 1)
            if hasattr(value, "get_data"):
                data = value.get_data()
                expanded += len(data)
                assert expanded <= MAX_EXPANDED, "PDF decoded stream budget"
                clean(data)
        elif isinstance(value, (list, tuple)):
            for item in value:
                scan(item, depth + 1)
        elif isinstance(value, bytes):
            clean(value)
        elif isinstance(value, str):
            assert not any(marker in value for marker in MARKERS), "Private publication mapping survived"
    scan(reader.trailer)
    for generation, rows in reader.xref.items():
        for object_id in rows:
            if object_id:
                scan(IndirectObject(object_id, generation, reader))
    for object_id in reader.xref_objStm:
        scan(IndirectObject(object_id, 0, reader))
    return {"allObjectsScanned": len(seen), "compressedObjectsScanned": len(reader.xref_objStm),
            "decodedBytes": expanded, "privateMappingAbsent": True}


def plain_font(name):
    return re.sub(r"^[A-Z]{6}\+", "", name).replace(" ", "").replace("-", "").lower()


def typst_metadata(document):
    """The pinned 0.15.1 engine uses Creator, not Producer.

    Preserve the older explicit Producer spelling, without accepting arbitrary
    strings merely containing 'typst'. Actual engine provenance also remains
    bound by the candidate, request and complete independent paint inventory.
    """
    creator = document.metadata.get("creator", "") or ""
    producer = document.metadata.get("producer", "") or ""
    current_creator = creator == "Typst 0.15.1"
    legacy_producer = isinstance(producer, str) and re.fullmatch(r"Typst(?: [0-9]+\.[0-9]+\.[0-9]+)?", producer, re.IGNORECASE) is not None
    assert current_creator or legacy_producer, ("Missing/wrong Typst engine metadata", creator, producer)
    return {"creator": creator, "producer": producer,
            "matchedField": "creator" if current_creator else "producer"}


def font_inventory(document, page):
    from fontTools.ttLib import TTFont
    result = {}
    for xref, extension, kind, name, resource, encoding, *rest in page.get_fonts(full=True):
        assert kind == "Type0" and encoding == "Identity-H", "Unproved visible font kind/encoding"
        extracted_name, extracted_extension, subtype, program = document.extract_font(xref)
        assert program and len(program) <= MAX_EXPANDED, "Visible font is not embedded"
        font = TTFont(io.BytesIO(program), lazy=False)
        assert "glyf" in font and "head" in font and "hmtx" in font, "Finite oracle needs TrueType outlines"
        record = {"xref": xref, "resource": resource, "programSha256": digest(program),
                  "name": re.sub(r"^[A-Z]{6}\+", "", name), "kind": kind, "encoding": encoding,
                  "unitsPerEm": font["head"].unitsPerEm, "font": font, "glyphs": {},
                  "italic": bool(font["post"].italicAngle or (font["OS/2"].fsSelection & 1) if "OS/2" in font else font["post"].italicAngle)}
        aliases = {plain_font(name), plain_font(extracted_name)}
        # MuPDF may expose the program's PostScript/full name rather than the
        # PDF BaseFont (e.g. DejaVuSans versus DejaVu Sans Book). Bind only real
        # embedded full/PS names, never a guessed family or source label.
        if "name" in font:
            for entry in font["name"].names:
                if entry.nameID in (4, 6):
                    try:
                        aliases.add(plain_font(entry.toUnicode()))
                    except UnicodeDecodeError:
                        pass
        for alias in aliases:
            assert alias not in result or result[alias]["programSha256"] == record["programSha256"], "Ambiguous visible font identity"
            result[alias] = record
    return result


def glyph_shape(font, gid):
    from fontTools.pens.boundsPen import BoundsPen
    from fontTools.pens.recordingPen import RecordingPen
    if gid not in font["glyphs"]:
        face = font["font"]
        order = face.getGlyphOrder()
        assert isinstance(gid, int) and 0 < gid < len(order), "Missing/invalid real glyph ID"
        name, glyphs = order[gid], face.getGlyphSet()
        outline, bounds = RecordingPen(), BoundsPen(glyphs)
        glyphs[name].draw(outline)
        glyphs[name].draw(bounds)
        font["glyphs"][gid] = {"shapeSha256": digest(json.dumps(outline.value, separators=(",", ":")).encode()),
                               "bounds": bounds.bounds, "advanceUnits": face["hmtx"].metrics[name][0]}
    return font["glyphs"][gid]


def finite(value):
    if isinstance(value, (int, float)):
        assert math.isfinite(value)
        return value
    if isinstance(value, (list, tuple)):
        return [finite(item) for item in value]
    if isinstance(value, dict):
        return {key: finite(item) for key, item in value.items()}
    if isinstance(value, (str, bool)) or value is None:
        return value
    return [finite(item) for item in value]


def link_parser():
    spec = importlib.util.spec_from_file_location("baseline_link_oracle", Path(__file__).with_name("inspect-mcp-publication-closure.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.pdf_link_record


def inventory(document):
    import pymupdf
    parse_link = link_parser()
    assert 0 < len(document) <= 8 and not document.embfile_count(), "Page/attachment boundary"
    pages, glyphs = [], []
    for index, page in enumerate(document):
        assert page.rotation == 0 and list(page.mediabox) == list(page.cropbox), "Unproved rotated/cropped page"
        user_unit = document.xref_get_key(page.xref, "UserUnit")
        assert user_unit[0] == "null" or user_unit[1] in ("1", "1.0"), "Unproved page UserUnit"
        assert not page.get_images(), "Finite baseline material must not become a raster image"
        fonts = font_inventory(document, page)
        page_glyphs = []
        for trace in page.get_texttrace():
            assert trace["type"] == 0 and 0 < trace.get("opacity", 1) <= 1, "Unexpected invisible/nonfill prose"
            assert list(trace["dir"]) == [1., 0.] and trace.get("wmode", 0) == 0, "Unproved rotated text"
            font = fonts.get(plain_font(trace["font"]))
            assert font is not None, "Trace does not name one actual embedded font"
            for codepoint, gid, origin, bbox in trace["chars"]:
                shape = glyph_shape(font, gid)
                size = trace["size"]
                assert math.isfinite(size) and size > 0
                ink = None
                if shape["bounds"] is not None:
                    x0, y0, x1, y1 = shape["bounds"]
                    scale = size / font["unitsPerEm"]
                    ink = [origin[0] + x0 * scale, origin[1] - y1 * scale,
                           origin[0] + x1 * scale, origin[1] - y0 * scale]
                    assert ink[2] > ink[0] and ink[3] > ink[1]
                    assert ink[0] >= -TOLERANCE and ink[1] >= -TOLERANCE and ink[2] <= page.rect.width + TOLERANCE and ink[3] <= page.rect.height + TOLERANCE, "Actual glyph ink is outside paper"
                else:
                    assert chr(codepoint).isspace(), "Non-whitespace text has no real glyph outline"
                row = {"page": index + 1, "unicode": codepoint, "gid": gid, "origin": list(origin), "bbox": list(bbox),
                       "ink": ink, "font": font["name"], "fontProgramSha256": font["programSha256"],
                       "fontItalicMetadata": font["italic"], "shapeSha256": shape["shapeSha256"],
                       "advance": shape["advanceUnits"] * size / font["unitsPerEm"], "size": size,
                       "paint": {key: finite(trace.get(key)) for key in ("flags", "type", "wmode", "bidi_lvl", "bidi_dir", "color", "colorspace", "opacity", "dir", "ascender", "descender", "linewidth")}}
                finite(row)
                page_glyphs.append(row)
                glyphs.append(row)
        links = [parse_link(document, link, pymupdf) for link in page.get_links()]
        semantic_links = [{key: finite(link[key]) for key in ("kind", "page", "to", "from", "uri", "name", "nameddest", "toDefaultCoordinates") if key in link} for link in links]
        pages.append({"page": index + 1, "pageObject": page.xref, "rect": list(page.rect), "glyphs": page_glyphs,
                      "links": semantic_links, "linkProofs": links, "drawings": finite(page.get_drawings()),
                      "pixelBytes": 0})
    assert glyphs, "The entire visible corpus disappeared"
    return {"pages": pages, "glyphs": glyphs, "visibleText": "".join(chr(row["unicode"]) for row in glyphs)}


def bind_corpus(glyphs, contract, same_paint=False):
    segments = contract["segments"]
    expected = [(ord(char), segment) for segment in segments for char in segment["text"] if not char.isspace()]
    actual = [row for row in glyphs if not chr(row["unicode"]).isspace()]
    assert [row["unicode"] for row in actual] == [code for code, _ in expected], "Full visible corpus differs; missing/extra/changed body text"
    bound = {}
    for row, (_, segment) in zip(actual, expected):
        bound[id(row)] = {**segment, "rise": 0 if same_paint else segment["rise"]}
    for index, row in enumerate(glyphs):
        if id(row) in bound:
            continue
        adjacent = []
        for direction in (-1, 1):
            cursor = index + direction
            while 0 <= cursor < len(glyphs) and chr(glyphs[cursor]["unicode"]).isspace():
                cursor += direction
            if 0 <= cursor < len(glyphs):
                adjacent.append(bound[id(glyphs[cursor])]["rise"])
        bound[id(row)] = {"role": "empty-outline-whitespace", "rise": 0, "allowedRises": sorted({0, *adjacent})}
    return bound


def compare_values(left, right, description):
    if isinstance(left, (int, float)) and not isinstance(left, bool) and isinstance(right, (int, float)) and not isinstance(right, bool):
        close(left, right, description)
    elif isinstance(left, list):
        assert isinstance(right, list) and len(left) == len(right), description
        for a, b in zip(left, right):
            compare_values(a, b, description)
    elif isinstance(left, dict):
        assert isinstance(right, dict) and set(left) == set(right), description
        for key in left:
            compare_values(left[key], right[key], description + ":" + key)
    else:
        assert left == right, (description, left, right)


def contains(rect, ink):
    return all((rect[0] <= ink[0] + TOLERANCE, rect[1] <= ink[1] + TOLERANCE,
                rect[2] >= ink[2] - TOLERANCE, rect[3] >= ink[3] - TOLERANCE))


def check_links(control, actual, bound, contract):
    # Corpus roles come from the complete independent glyph sequence. Neither
    # URI equality, annotation order nor PDF object numbers establish ownership.
    roles = [bound[id(row)] for row in control["glyphs"]]

    def overlap(a, b):
        return min(a[2], b[2]) - max(a[0], b[0]) > TOLERANCE and min(a[3], b[3]) - max(a[1], b[1]) > TOLERANCE

    def fragments(document):
        indices = {id(row): index for index, row in enumerate(document["glyphs"])}
        owned, counts = {}, [0] * len(roles)
        for page in document["pages"]:
            seen = []
            for link in page["links"]:
                rect = link["from"]
                assert len(rect) == 4 and all(math.isfinite(value) for value in rect) and rect[0] < rect[2] and rect[1] < rect[3], "Invalid real annotation Rect"
                assert not any(overlap(rect, previous) for previous in seen), "Duplicate/overlapping real link fragments"
                seen.append(rect)
                targets = []
                for row in page["glyphs"]:
                    if row["ink"] is None or not overlap(rect, row["ink"]):
                        continue
                    index = indices[id(row)]
                    assert roles[index].get("link"), "A real link fragment covers non-link visible ink"
                    assert contains(rect, row["ink"]), "A real link fragment cuts through visible glyph ink"
                    targets.append(index)
                identities = {roles[index]["link"] for index in targets}
                assert len(identities) == 1, "A real annotation has no unique independent corpus link owner"
                identity = next(iter(identities))
                key = page["page"], tuple(targets)
                assert key not in owned, "Duplicate real link glyph ownership"
                owned[key] = link, identity
                for index in targets:
                    counts[index] += 1
        for identity, declared in contract.get("links", {}).items():
            required = [index for index, row in enumerate(document["glyphs"]) if row["ink"] is not None and roles[index].get("link") == identity]
            assert required and all(counts[index] == 1 for index in required), "Every visible linked glyph must have exactly one real fragment"
            assert len({document["glyphs"][index]["page"] for index in required}) >= declared.get("minPages", 1), "Required cross-page link was not actually cross-page"
        return owned

    old_fragments, new_fragments = fragments(control), fragments(actual)
    assert old_fragments.keys() == new_fragments.keys(), "A real link fragment disappeared/appeared or changed its glyph ownership"
    for key, (old, identity) in old_fragments.items():
        new, new_identity = new_fragments[key]
        assert identity == new_identity
        declared = contract["links"][identity]
        if "uri" in declared:
            assert old.get("uri") == new.get("uri") == declared["uri"]
        else:
            assert old["kind"] == new["kind"] == 1
            anchors = [row for row in control["glyphs"] if bound[id(row)].get("anchor") == declared["goto"]]
            assert anchors and {row["page"] for row in anchors} == {old["page"] + 1}
            top, bottom = min(row["ink"][1] for row in anchors), max(row["ink"][3] for row in anchors)
            assert old.get("to") and old["to"][1] is not None and top - 24 <= old["to"][1] <= bottom + TOLERANCE, "Internal link does not target the actual heading"
        compare_values({name: value for name, value in old.items() if name != "from"}, {name: value for name, value in new.items() if name != "from"}, "Real link destination")
        rises = {roles[index]["rise"] for index in key[1]}
        assert len(rises) == 1
        rise = next(iter(rises))
        compare_values(new["from"], [old["from"][0], old["from"][1] - rise, old["from"][2], old["from"][3] - rise], "Real link Rect-only shift")
    return len(old_fragments)


def png_rgb(raw):
    """Bounded, CRC-checked nonanimated RGB/RGBA PNG, composited over white."""
    import pymupdf
    assert 0 < len(raw) <= 8 * 1024 * 1024 and raw[:8] == b"\x89PNG\r\n\x1a\n", "Invalid preview PNG"
    at, size, channels, compressed, ended, seen_header = 8, None, None, [], False, False
    while at < len(raw):
        assert at + 12 <= len(raw), "Truncated PNG chunk"
        length = int.from_bytes(raw[at:at + 4], "big")
        kind = raw[at + 4:at + 8]
        stop = at + 8 + length
        assert stop + 4 <= len(raw), "Truncated PNG data"
        data = raw[at + 8:stop]
        assert zlib.crc32(kind + data) & 0xffffffff == int.from_bytes(raw[stop:stop + 4], "big"), "PNG CRC mismatch"
        if kind == b"IHDR":
            assert not seen_header and at == 8 and length == 13, "Invalid PNG header"
            width, height, bits, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            assert 0 < width <= 3000 and 0 < height <= 3000 and width * height <= 8_000_000, "PNG pixel budget"
            assert bits == 8 and color in (2, 6) and compression == filtering == interlace == 0, "Unproved PNG encoding"
            size, channels, seen_header = (width, height), 3 if color == 2 else 4, True
        elif kind == b"IDAT":
            assert seen_header and not ended
            compressed.append(data)
        elif kind == b"IEND":
            assert seen_header and compressed and not length and stop + 4 == len(raw), "Invalid PNG end"
            ended = True
        else:
            assert kind not in (b"acTL", b"fcTL", b"fdAT") and kind[:1].islower(), "Unknown critical/animated PNG chunk"
        at = stop + 4
    assert ended and size is not None, "Incomplete preview PNG"
    expected = size[1] * (size[0] * channels + 1)
    decoder = zlib.decompressobj()
    decoded = decoder.decompress(b"".join(compressed), expected + 1)
    assert decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail and len(decoded) == expected, "PNG decoded-size boundary"
    pixmap = pymupdf.Pixmap(raw)
    assert (pixmap.width, pixmap.height) == size and pixmap.n == channels
    samples = pixmap.samples
    if channels == 4:
        rgb = bytearray(size[0] * size[1] * 3)
        for index in range(size[0] * size[1]):
            alpha = samples[index * 4 + 3]
            for channel in range(3):
                # MuPDF samples are premultiplied, including decoded PNG.
                rgb[index * 3 + channel] = min(255, samples[index * 4 + channel] + 255 - alpha)
        samples = bytes(rgb)
    return size, samples


def preview_masks(samples, width, height):
    weak, strong = [], []
    for y in range(height):
        a = b = 0
        for x in range(width):
            at = (y * width + x) * 3
            contrast = 255 - min(samples[at], samples[at + 1], samples[at + 2])
            if contrast >= PREVIEW_AA["weakContrast"]:
                a |= 1 << x
            if contrast >= PREVIEW_AA["strongContrast"]:
                b |= 1 << x
        weak.append(a)
        strong.append(b)
    return weak, strong


def dilate_mask(rows, width, radius):
    trim = (1 << width) - 1
    result = []
    for y in range(len(rows)):
        row = 0
        for yy in range(max(0, y - radius), min(len(rows), y + radius + 1)):
            row |= rows[yy]
        for _ in range(radius):
            row |= (row << 1) | (row >> 1)
        result.append(row & trim)
    return result


def preview_rgb_compare(reference, rendered, width, height):
    """Match all local ink pixels; the quota never excuses an unmatched pixel.

    This finite raster check is not a proof for arbitrary touching tiny
    symbols. Complete glyph/font/outline/path and page checks remain separate.
    """
    masks = [preview_masks(raw, width, height) for raw in (reference, rendered)]
    assert all(any(strong) for weak, strong in masks), "Blank preview page"
    maxima, hinting_counts = [0, 0], [0, 0]
    for side, (weak, strong) in enumerate(masks):
        other_weak = masks[1 - side][0]
        near = dilate_mask(other_weak, width, PREVIEW_AA["strongRadiusPx"])
        fringe = dilate_mask(other_weak, width, PREVIEW_AA["fringeRadiusPx"])
        hinting = dilate_mask(other_weak, width, PREVIEW_AA["hintingRadiusPx"])
        assert not any(row & ~allowed for row, allowed in zip(strong, hinting)), "Visible preview ink differs from the current PDF (missing/stale/wrong page)"
        assert not any(row & ~allowed for row, allowed in zip(weak, fringe)), "Preview fringe is outside the fixed AA envelope"
        source, target = (reference, rendered) if side == 0 else (rendered, reference)
        for y, bits in enumerate(strong):
            while bits:
                bit = bits & -bits
                x = bit.bit_length() - 1
                bits ^= bit
                at = (y * width + x) * 3
                rgb = source[at:at + 3]
                limit = PREVIEW_AA["coreRgbDelta"] if 255 - min(rgb) >= PREVIEW_AA["coreContrast"] else PREVIEW_AA["edgeRgbDelta"]
                def local_delta(radius):
                    best = 256
                    for yy in range(max(0, y - radius), min(height, y + radius + 1)):
                        for xx in range(max(0, x - radius), min(width, x + radius + 1)):
                            to = (yy * width + xx) * 3
                            best = min(best, max(abs(rgb[channel] - target[to + channel]) for channel in range(3)))
                    return best
                best = local_delta(PREVIEW_AA["strongRadiusPx"])
                # Count the UNION of geometric and RGB exceptions. Counting
                # only weak-mask distances could hide a whole-image shift.
                exceptional = not (near[y] & bit) or best > limit
                if exceptional:
                    hinting_counts[side] += 1
                    best = local_delta(PREVIEW_AA["hintingRadiusPx"])
                maxima[side] = max(maxima[side], best)
                assert best <= limit, "Visible preview color/coverage differs from the current PDF"
        strong_count = sum(row.bit_count() for row in strong)
        assert hinting_counts[side] <= math.floor(strong_count * PREVIEW_AA["maxHintingFraction"]), "Preview exceeds the sparse font-hinting budget (shifted/changed image)"
    return {"strongPixels": [sum(row.bit_count() for row in mask[1]) for mask in masks],
            "weakPixels": [sum(row.bit_count() for row in mask[0]) for mask in masks],
            "hintingStrongPixels": hinting_counts, "hintingBudgetCountsGeometryAndRgb": True,
            "maxLocalRgbDelta": maxima, "unmatchedStrongPixels": [0, 0], "unmatchedFringePixels": [0, 0],
            "everyPixelCheckedBothDirections": True, "aa": dict(PREVIEW_AA)}


def compare_preview(pdf_file, png_file, page_number):
    import pymupdf
    pdf, png = pdf_file.read_bytes(), png_file.read_bytes()
    assert 0 < len(pdf) <= MAX_BYTES and isinstance(page_number, int) and not isinstance(page_number, bool)
    (width, height), rendered = png_rgb(png)
    with pymupdf.open(stream=pdf, filetype="pdf") as document:
        assert 0 < len(document) <= 8 and 1 <= page_number <= len(document), "Preview page does not belong to its current PDF"
        page = document[page_number - 1]
        assert page.rotation == 0 and list(page.mediabox) == list(page.cropbox)
        # Compare on the actual PNG grid, with no image registration/translation.
        # C54 is an older retained renderer resolution; current WinRT is bounded
        # independently by the product. Neither a 1px thumbnail nor stretched
        # aspect ratio can pass this finite 12pt corpus.
        assert width >= page.rect.width and height >= page.rect.height, "Preview resolution is below one pixel per PDF point"
        assert abs(width / page.rect.width - height / page.rect.height) <= 1 / page.rect.width + 1 / page.rect.height, "Preview/PDF aspect ratio mismatch"
        pixmap = page.get_pixmap(matrix=pymupdf.Matrix(width / page.rect.width, height / page.rect.height), alpha=False)
        assert pixmap.n == 3 and width <= pixmap.width <= width + 1 and height <= pixmap.height <= height + 1
        samples = pixmap.samples
        reference = b"".join(samples[y * pixmap.width * 3:(y * pixmap.width + width) * 3] for y in range(height))
        proof = preview_rgb_compare(reference, rendered, width, height)
        return {"passed": True, "page": page_number, "pages": len(document), "width": width, "height": height,
                "renderer": "Windows.Data.Pdf (WinRT) response versus independent MuPDF",
                "pdfSha256": digest(pdf), "pngSha256": digest(png), "decodedRgbSha256": digest(rendered),
                "referenceRgbSha256": digest(reference), "pageFrame": list(page.rect), **proof,
                "externalApplicationCertified": False}


def pixel_compare(control_doc, actual_doc, control, actual, bound, png_directory=None):
    import pymupdf
    pngs, pixels = [], 0
    if png_directory is not None:
        assert not png_directory.exists()
        png_directory.mkdir()
    for index, (before, after) in enumerate(zip(control_doc, actual_doc)):
        left = before.get_pixmap(matrix=pymupdf.Matrix(SCALE, SCALE), alpha=False)
        right = after.get_pixmap(matrix=pymupdf.Matrix(SCALE, SCALE), alpha=False)
        assert (left.width, left.height, left.n) == (right.width, right.height, right.n) and left.n == 3
        assert left.width * left.height <= 12_000_000
        original = left.samples
        expected = bytearray(original)
        patches = []
        for row in control["pages"][index]["glyphs"]:
            rise = bound[id(row)]["rise"]
            if row["ink"] is None or rise == 0:
                continue
            offset = -rise * SCALE
            assert abs(offset - round(offset)) <= 1e-8, "Finite raster proof needs a quarter-point shift"
            offset = round(offset)
            x0, y0, x1, y1 = row["ink"]
            rect = (max(0, math.floor(x0 * SCALE) - 1), max(0, math.floor(y0 * SCALE) - 1),
                    min(left.width, math.ceil(x1 * SCALE) + 1), min(left.height, math.ceil(y1 * SCALE) + 1))
            assert rect[1] + offset >= 0 and rect[3] + offset <= left.height
            patches.append((rect, offset))
        for (x0, y0, x1, y1), offset in patches:
            for y in range(y0, y1):
                start = (y * left.width + x0) * 3
                expected[start:start + (x1 - x0) * 3] = b"\xff" * ((x1 - x0) * 3)
        for (x0, y0, x1, y1), offset in patches:
            for y in range(y0, y1):
                source = (y * left.width + x0) * 3
                target = ((y + offset) * left.width + x0) * 3
                expected[target:target + (x1 - x0) * 3] = original[source:source + (x1 - x0) * 3]
        assert bytes(expected) == right.samples, "Whole-page rendered paint differs from the exact glyph-only shift (clipping, missing paint or unintended changes)"
        pixels += left.width * left.height
        if png_directory is not None:
            for label, page in (("control", before), ("actual", after)):
                file = png_directory / f"{label}-{index + 1}.png"
                page.get_pixmap(matrix=pymupdf.Matrix(2, 2), alpha=False).save(file)
                pngs.append({"path": str(file), "bytes": file.stat().st_size, "sha256": digest(file.read_bytes())})
    return {"pixels": pixels, "scale": SCALE, "exactAllPixels": True, "pngs": pngs}


def compare(control_file, actual_file, contract, same_paint=False, png_directory=None):
    import pymupdf
    assert contract.get("schema") == 1 and contract["segments"]
    for segment in contract["segments"]:
        assert isinstance(segment["text"], str) and segment["text"]
        assert isinstance(segment["rise"], (int, float)) and math.isfinite(segment["rise"]) and -72 <= segment["rise"] <= 72
        assert segment["role"] in ("ordinary", "protected", "fixed", "italic")
        assert segment["role"] not in ("protected", "fixed") or segment["rise"] == 0
    clean_control, clean_actual = clean_pdf(control_file), clean_pdf(actual_file)
    with pymupdf.open(control_file) as before, pymupdf.open(actual_file) as after:
        control, actual = inventory(before), inventory(after)
        assert len(control["pages"]) == len(actual["pages"]) >= contract.get("minPages", 1)
        engine_metadata = None
        if contract.get("requireTypstProducer"):
            # Keep the existing contract flag, accepting the engine's actual
            # Creator field as well as historical explicit Producer metadata.
            engine_metadata = {"control": typst_metadata(before), "actual": typst_metadata(after)}
        left, right = control["glyphs"], actual["glyphs"]
        assert len(left) == len(right), "Full glyph inventory changed"
        bound = bind_corpus(left, contract, same_paint)
        bind_corpus(right, contract, same_paint)
        moved = protected = empty = 0
        for a, b in zip(left, right):
            for key in ("page", "unicode", "gid", "font", "fontProgramSha256", "fontItalicMetadata", "shapeSha256", "paint"):
                assert a[key] == b[key], ("Glyph identity/paint changed", key, a[key], b[key])
            close(a["size"], b["size"], "Font size")
            close(a["advance"], b["advance"], "Glyph advance")
            role = bound[id(a)]
            if "expectedSize" in role:
                close(a["size"], role["expectedSize"], "Requested author font size")
            allowed = role.get("allowedRises", [role["rise"]])
            assert any(abs(b["origin"][1] - a["origin"][1] + rise) <= TOLERANCE for rise in allowed), ("Glyph baseline is wrong", chr(a["unicode"]), a["origin"], b["origin"], allowed)
            close(a["origin"][0], b["origin"][0], "Glyph x")
            rise = a["origin"][1] - b["origin"][1]
            compare_values(b["bbox"], [a["bbox"][0], a["bbox"][1] - rise, a["bbox"][2], a["bbox"][3] - rise], "Logical glyph bbox")
            if a["ink"] is None:
                assert b["ink"] is None
                empty += 1
            else:
                compare_values(b["ink"], [a["ink"][0], a["ink"][1] - rise, a["ink"][2], a["ink"][3] - rise], "Real outline ink")
                moved += role["rise"] != 0
                protected += role["role"] == "protected"
        italic_fonts = set()
        if contract.get("requirePhysicalItalic"):
            italic_rows = [row for row in left if bound[id(row)]["role"] == "italic"]
            ordinary_fonts = {row["fontProgramSha256"] for row in left if bound[id(row)]["role"] == "ordinary"}
            assert italic_rows and all(row["fontItalicMetadata"] for row in italic_rows), "Em did not select an actual italic/oblique font"
            italic_fonts = {row["fontProgramSha256"] for row in italic_rows}
            assert not italic_fonts & ordinary_fonts, "An upright font program was mislabeled as italic"
        for a, b in zip(control["pages"], actual["pages"]):
            compare_values(a["rect"], b["rect"], "Paper frame")
            def paths(page):
                return [{key: value for key, value in row.items() if key not in ("seqno", "layer")} for row in page["drawings"]]
            compare_values(paths(a), paths(b), "All protected/other vector drawing paths")
        links = check_links(control, actual, bound, contract)
        raster = pixel_compare(before, after, control, actual, bound, png_directory)
        return {"passed": True, "pages": len(before), "allGlyphs": len(left), "movedVisibleGlyphs": moved,
                "pageFrames": [[page.rect.width, page.rect.height] for page in after],
                "corpusInk": [{"block": bound[id(a)]["block"], "page": b["page"], "ink": b["ink"]}
                              for a, b in zip(left, right) if b["ink"] is not None and bound[id(a)].get("block")],
                "protectedVisibleGlyphs": protected, "emptyOutlineWhitespace": empty,
                "emptyWhitespaceRiseNotCertified": True, "allVisibleFontProgramsGidsShapesAdvancesCompared": True,
                "coordinateTolerancePt": TOLERANCE, "realLinkFragments": links,
                "everyVisibleLinkedGlyphExactlyOneFragment": True,
                "overlappingFragmentsOrNonLinkInkCoverageRejected": True,
                "allDrawingPaths": sum(len(page["drawings"]) for page in control["pages"]),
                "physicalItalicCertified": bool(italic_fonts), "italicFontPrograms": sorted(italic_fonts),
                "typstEngineMetadata": engine_metadata,
                "cleanup": {"control": clean_control, "actual": clean_actual}, "raster": raster,
                "controlSha256": digest(control_file.read_bytes()), "actualSha256": digest(actual_file.read_bytes()),
                "applicationOrFullP04Certified": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control", type=Path)
    parser.add_argument("--actual", type=Path)
    parser.add_argument("--contract", type=Path)
    parser.add_argument("--same-paint", action="store_true")
    parser.add_argument("--png-dir", type=Path)
    parser.add_argument("--preview-pdf", type=Path)
    parser.add_argument("--preview-png", type=Path)
    parser.add_argument("--page", type=int)
    args = parser.parse_args()
    if args.preview_pdf is not None or args.preview_png is not None or args.page is not None:
        assert args.preview_pdf is not None and args.preview_png is not None and args.page is not None
        assert args.control is None and args.actual is None and args.contract is None and args.png_dir is None and not args.same_paint
        result = compare_preview(args.preview_pdf, args.preview_png, args.page)
    else:
        assert args.control is not None and args.actual is not None and args.contract is not None
        contract = json.loads(args.contract.read_text(encoding="utf-8"))
        result = compare(args.control, args.actual, contract, args.same_paint, args.png_dir)
    print(json.dumps(result, ensure_ascii=True))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
