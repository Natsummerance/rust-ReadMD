"""Physical PDF small-caps evidence from real glyphs and embedded font programs.

QA only. This does not infer rendering from CSL/Typst/TeX source instructions.
Original offline fonts are retained privately; no font is downloaded or changed.
"""
import argparse
import base64
import gzip
import hashlib
from io import BytesIO
import json
import math
from pathlib import Path
import re
import struct
import sys
import traceback

import pymupdf as fitz
from fontTools import cffLib
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.recordingPen import DecomposingRecordingPen, RecordingPen, replayRecording
from fontTools.ttLib import TTFont

sys.stdout.reconfigure(encoding="utf-8")
ROOT = Path(__file__).resolve().parents[2]
FACES = {"R": (False, False), "B": (True, False), "I": (False, True), "BI": (True, True)}
WORD = "Abcxyz"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def font_key(name):
    return re.sub(r"[^a-z0-9]", "", re.sub(r"^[A-Z]{6}\+", "", name).replace("-Identity-H", "").lower())


def names(font):
    # An embedded PDF subset may omit the optional naming metadata. Keep the
    # absence explicit: CID/resource binding and real outlines still provide
    # physical evidence; never invent a requested family name here.
    if "name" not in font:
        return []
    return sorted({record.toUnicode() for record in font["name"].names if record.nameID in (1, 2, 4, 6, 16, 17)})


class OutlineFont:
    def __init__(self, data):
        self.sha256, self.length = sha(data), len(data)
        self.cache = {}
        self.cid_map = None
        if data[:4] in (b"OTTO", b"\x00\x01\x00\x00", b"true"):
            self.font = TTFont(BytesIO(data), lazy=False)
            self.order = self.font.getGlyphOrder()
            self.glyphs = self.font.getGlyphSet()
            self.units = self.font["head"].unitsPerEm
            self.names = names(self.font)
            self.name_sources = [{"source": "sfnt-name-table", "value": name} for name in self.names]
            self.format = "sfnt"
        else:
            self.font = None
            cff = cffLib.CFFFontSet()
            cff.decompile(BytesIO(data), TTFont(), isCFF2=False)
            self.cff = cff
            top = cff.topDictIndex[0]
            self.order, self.glyphs = top.charset, top.CharStrings
            matrix = top.FontMatrix
            assert matrix[0] > 0 and matrix[3] == matrix[0] and matrix[1] == matrix[2] == 0
            self.units = round(1 / matrix[0])
            self.name_sources = [{"source": "cff-name-index", "index": index, "value": str(name)}
                                 for index, name in enumerate(cff.fontNames)]
            # Subset CFF programs need not carry these optional Top DICT names.
            # Keep names from the actual font program; never supply the requested
            # family as a substitute for missing physical font evidence.
            for field in ("FullName", "FamilyName"):
                value = getattr(top, field, None)
                if value is not None:
                    self.name_sources.append({"source": "cff-top-dict." + field, "value": str(value)})
            self.names = list(dict.fromkeys(item["value"] for item in self.name_sources))
            self.format = "cff"
        assert 16 <= self.units <= 65536 and len(self.order) < 100000

    def glyph_name(self, glyph):
        if type(glyph) is int:
            if self.format == "cff" and hasattr(self.cff.topDictIndex[0], "ROS"):
                assert self.cid_map is not None, "Actual CID-keyed font lacks a verified PDF resource binding"
                assert glyph in self.cid_map, "Actual PDF CID is absent from its embedded CFF charset"
                return self.cid_map[glyph]
            assert 0 <= glyph < len(self.order), "Actual PDF glyph ID lies outside its embedded font"
            return self.order[glyph]
        assert isinstance(glyph, str) and glyph in self.glyphs, "Actual glyph name is absent from its embedded font"
        return glyph

    def outline(self, glyph):
        glyph = self.glyph_name(glyph)
        if glyph in self.cache:
            return self.cache[glyph]
        pen = DecomposingRecordingPen(self.glyphs) if self.font is not None else RecordingPen()
        self.glyphs[glyph].draw(pen)
        bounds = BoundsPen(None)
        replayRecording(pen.value, bounds)
        normalized = [(operator, [None if point is None else [round(coordinate / self.units, 5) for coordinate in point]
                                 for point in operands]) for operator, operands in pen.value]
        result = {"outlineSha256": sha(json.dumps(normalized, separators=(",", ":")).encode()),
                  "bounds": None if bounds.bounds is None else [round(value / self.units, 5) for value in bounds.bounds]}
        self.cache[glyph] = result
        return result

    def smallcaps(self):
        assert self.font is not None and "GSUB" in self.font, "Original font lacks a readable GSUB table"
        gsub = self.font["GSUB"].table
        result = {}
        def subtable(table, kind):
            if kind == 1:
                result.update(table.mapping)
            elif kind == 2:
                result.update({glyph: sequence[0] if len(sequence) == 1 else tuple(sequence)
                               for glyph, sequence in table.mapping.items()})
            elif kind == 7:
                subtable(table.ExtSubTable, table.ExtensionLookupType)
            else:
                raise AssertionError("Unreviewed smcp substitution lookup type: " + str(kind))
        for feature in gsub.FeatureList.FeatureRecord:
            if feature.FeatureTag == "smcp":
                for index in feature.Feature.LookupListIndex:
                    lookup = gsub.LookupList.Lookup[index]
                    for table in lookup.SubTable:
                        subtable(table, lookup.LookupType)
        assert result, "Original font has no actual smcp substitution"
        return result


def audit_typst(pack_file, lock_file, destination):
    destination = destination.resolve()
    assert destination.is_relative_to((ROOT / ".cache/private").resolve()), "Extracted font evidence must remain private"
    assert not destination.exists(), "Preserve the first extracted font evidence"
    pack_bytes = pack_file.read_bytes()
    assert len(pack_bytes) < 128 * 1024 * 1024
    pack, lock = json.loads(pack_bytes), json.loads(lock_file.read_text(encoding="utf-8"))
    assert pack["manifest"] == lock
    entry = next(item for item in lock["files"] if item["path"] == "bin/typst.exe")
    payload = pack["payload"][entry["path"]]
    assert payload["compression"] == "gzip"
    compressed = base64.b64decode(payload["data"], validate=True)
    with gzip.GzipFile(fileobj=BytesIO(compressed)) as stream:
        engine = stream.read(entry["size"] + 1)
    assert len(engine) == entry["size"] and sha(engine) == entry["sha256"]
    extracted, offset = [], 0
    while True:
        offset = engine.find(b"OTTO", offset)
        if offset < 0:
            break
        start = offset
        offset += 4
        if start + 12 > len(engine):
            continue
        count = struct.unpack_from(">H", engine, start + 4)[0]
        if not 8 <= count <= 80 or start + 12 + count * 16 > len(engine):
            continue
        tables, end, valid = set(), 0, True
        for index in range(count):
            tag, _, table_offset, length = struct.unpack_from(">4sIII", engine, start + 12 + index * 16)
            if not all(32 <= value <= 126 for value in tag) or table_offset < 12 + count * 16 or table_offset + length > 16 * 1024 * 1024 or start + table_offset + length > len(engine):
                valid = False
                break
            tables.add(tag)
            end = max(end, (table_offset + length + 3) & ~3)
        if not valid or not {b"head", b"name", b"cmap", b"CFF ", b"GSUB"}.issubset(tables):
            continue
        raw = engine[start:start + end]
        font = OutlineFont(raw)
        if not any("libertinusserif" in font_key(name) for name in font.names):
            continue
        substitutions = font.smallcaps()
        cmap = font.font.getBestCmap()
        assert all(isinstance(substitutions.get(cmap[ord(character)]), str) for character in WORD[1:])
        extracted.append((raw, {"offset": start, "bytes": len(raw), "sha256": font.sha256,
                                "names": font.names, "smcpMappings": len(substitutions)}))
    assert len(extracted) >= 4, "Locked Typst binary contains fewer than four auditable Libertinus Serif faces"
    destination.mkdir(parents=True)
    records = []
    for index, (raw, record) in enumerate(extracted):
        target = destination / ("libertinus-serif-" + str(index) + ".otf")
        target.write_bytes(raw)
        records.append({**record, "path": str(target.relative_to(ROOT))})
    result = {"qaOnly": True, "redistributionApproved": False, "source": "Read-only OTTO/table-directory extraction from the existing locked engine; the engine was not executed",
              "packSha256": sha(pack_bytes), "engineSha256": sha(engine), "lockSha256": sha(lock_file.read_bytes()), "fonts": records}
    (destination / "audit.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def source_fonts(audit_file, family):
    audit = json.loads(audit_file.read_text(encoding="utf-8-sig"))
    result = {}
    for entry in audit["fonts"]:
        if not any(font_key(family) in font_key(name) for name in entry["names"]):
            continue
        name = " ".join(entry["names"]).lower()
        bold, italic = "bold" in name, "italic" in name or "oblique" in name
        # Embedded Typst also has semibold variants; use the exact four requested faces.
        if "semibold" in name:
            continue
        face = next(key for key, flags in FACES.items() if flags == (bold, italic))
        data = (ROOT / entry["path"]).read_bytes()
        assert sha(data) == entry["sha256"], "The independently audited source font changed"
        font = OutlineFont(data)
        assert face not in result, "Ambiguous original font face in audit"
        result[face] = {"font": font, "smcp": font.smallcaps(), "cmap": font.font.getBestCmap(), "path": entry["path"]}
    assert set(result) == set(FACES), "All four actual original font faces are required"
    return result


def pdf_font_name_sources(document, xref, basefont, extracted_name):
    result = [{"source": "pdf.BaseFont", "xref": xref, "value": basefont},
              {"source": "mupdf.extract_font", "xref": xref, "value": extracted_name}]
    font_xrefs = [xref]
    kind, descendants = document.xref_get_key(xref, "DescendantFonts")
    if kind == "array":
        font_xrefs.extend(int(value) for value in re.findall(r"(\d+)\s+0\s+R", descendants))
    for font_xref in font_xrefs:
        kind, descriptor = document.xref_get_key(font_xref, "FontDescriptor")
        if kind != "xref":
            continue
        descriptor_xref = int(descriptor.split()[0])
        kind, value = document.xref_get_key(descriptor_xref, "FontName")
        if kind == "name":
            result.append({"source": "pdf.FontDescriptor.FontName", "xref": descriptor_xref,
                           "fontXref": font_xref, "value": value.removeprefix("/")})
    return result


def bind_pdf_cids(document, xref, embedded, extracted):
    """Interpret trace integers as CIDs only for a verified actual CID CFF resource."""
    if embedded.format != "cff" or not hasattr(embedded.cff.topDictIndex[0], "ROS"):
        return None
    assert document.xref_get_key(xref, "Type") == ("name", "/Font"), "CID resource is not a PDF font"
    assert document.xref_get_key(xref, "Subtype") == ("name", "/Type0"), "CID resource is not Type0"
    assert document.xref_get_key(xref, "Encoding") == ("name", "/Identity-H"), "Unreviewed CID trace encoding"
    kind, value = document.xref_get_key(xref, "DescendantFonts")
    child = re.fullmatch(r"\[\s*([1-9][0-9]*)\s+0\s+R\s*\]", value) if kind == "array" else None
    assert child, "CID resource must have one exact indirect descendant"
    descendant = int(child[1])
    assert document.xref_get_key(descendant, "Type") == ("name", "/Font"), "CID descendant is not a font"
    assert document.xref_get_key(descendant, "Subtype") == ("name", "/CIDFontType0"), "CID descendant has the wrong font type"

    def reference(owner, field):
        kind, value = document.xref_get_key(owner, field)
        match = re.fullmatch(r"([1-9][0-9]*)\s+0\s+R", value) if kind == "xref" else None
        assert match, "CID font lacks an exact indirect " + field
        return int(match[1])

    descriptor = reference(descendant, "FontDescriptor")
    font_file = reference(descriptor, "FontFile3")
    assert document.xref_get_key(font_file, "Subtype") == ("name", "/CIDFontType0C"), "CID FontFile3 has the wrong subtype"
    actual_bytes = document.xref_stream(font_file)
    assert actual_bytes == extracted and sha(actual_bytes) == embedded.sha256, "CID FontFile3 differs from the actual extracted font"
    registry, ordering, supplement = (document.xref_get_key(descendant, "CIDSystemInfo/" + key)
                                      for key in ("Registry", "Ordering", "Supplement"))
    assert registry[0] == ordering[0] == "string" and supplement[0] == "int", "CIDSystemInfo has unreviewed value types"
    assert re.fullmatch(r"[0-9]+", supplement[1]), "Invalid CIDSystemInfo supplement"
    pdf_ros = (registry[1], ordering[1], int(supplement[1]))
    assert len(embedded.cff.topDictIndex) == 1, "CID FontFile3 contains ambiguous CFF fonts"
    top = embedded.cff.topDictIndex[0]
    ros = tuple(top.ROS)
    assert ros == pdf_ros == ("Adobe", "Identity", 0), "Actual CFF ROS and PDF CIDSystemInfo do not match the reviewed Identity binding"
    assert top.charset and top.charset[0] == ".notdef", "CID CFF lacks its required initial .notdef"
    assert len(top.charset) == len(set(top.charset)) == len(top.CharStrings), "CID CFF charset is ambiguous"
    assert set(top.charset) == set(top.CharStrings.keys()), "CID CFF charset and CharStrings differ"
    assert hasattr(top, "FDArray") and hasattr(top, "FDSelect"), "CID CFF lacks FDArray or FDSelect"
    assert 0 < len(top.FDArray) <= 65536 and len(top.FDSelect) == len(top.charset), "CID CFF has an invalid FDSelect size"
    mapping = {}
    for index, name in enumerate(top.charset):
        selector = top.FDSelect[index]
        assert type(selector) is int and 0 <= selector < len(top.FDArray), "CID CFF selects a missing font dictionary"
        _, actual_selector = top.CharStrings.getItemAndSelector(name)
        assert actual_selector == selector, "CID CharString disagrees with its actual FDSelect"
        if index == 0:
            continue
        match = re.fullmatch(r"cid([0-9]+)", name)
        assert match, "CID charset contains a non-CID glyph name"
        cid = int(match[1])
        assert 0 < cid <= 65535 and cid not in mapping, "CID CFF contains a duplicate or invalid CID"
        mapping[cid] = name
    assert mapping, "CID CFF has no actual printable CID"
    embedded.cid_map = mapping
    return {"traceIntegerKind": "CID", "fontXref": xref, "descendantXref": descendant,
            "descriptorXref": descriptor, "fontFile3Xref": font_file, "encoding": "Identity-H",
            "fontFile3Sha256": sha(actual_bytes), "fontFile3Bytes": len(actual_bytes),
            "cffRos": list(ros), "pdfCidSystemInfo": list(pdf_ros), "charsetCount": len(top.charset),
            "fdArrayCount": len(top.FDArray), "fdSelectCount": len(top.FDSelect),
            "mapping": [{"cid": cid, "charString": name} for cid, name in sorted(mapping.items())]}


def locate_probe(glyphs, copied, marker, word):
    # Original Unicode copy text stays exact even when MuPDF omits space glyphs.
    assert re.search(r"(?<!\w)" + re.escape(marker + " " + word) + r"(?!\w)", copied), "Actual PDF copy text changed original Unicode: " + marker
    assert all(len(glyph["text"]) == 1 for glyph in glyphs), "Glyph stream has a multi-character entry"
    stream = "".join(glyph["text"] for glyph in glyphs)
    # Retain the leading boundary (I_NORMAL must not match BI_NORMAL). The
    # complete word and actual geometric gap replace only the trailing boundary.
    matches = list(re.finditer(r"(?<!\w)" + re.escape(marker), stream))
    assert len(matches) == 1, "Actual glyph stream lacks one unambiguous probe marker: " + marker
    match = matches[0]
    label = glyphs[match.start():match.end()]
    start, spaces = match.end(), []
    while start < len(glyphs) and glyphs[start]["text"].isspace():
        assert glyphs[start]["text"] not in "\r\n", "Probe word is on a different marker line"
        spaces.append(glyphs[start])
        start += 1
    selected = glyphs[start:start + len(word)]
    assert len(selected) == len(word), "Physical glyph stream has an incomplete probe word"
    assert "".join(glyph["text"] for glyph in selected).upper() == word.upper(), "Physical glyph stream changed the probe text"
    row = label + spaces + selected
    assert all(glyph["page"] == row[0]["page"] for glyph in row), "Probe marker and word are on different pages"
    assert all(math.isfinite(value) for glyph in row for value in [*glyph["origin"], *glyph["bbox"], glyph["size"]]), "Probe has non-finite physical geometry"
    assert all(glyph["size"] > 0 and glyph["bbox"][0] <= glyph["bbox"][2] for glyph in row), "Probe has invalid physical size or bounds"
    assert max(glyph["origin"][1] for glyph in row) - min(glyph["origin"][1] for glyph in row) < .1, "Probe marker and word have different baselines"
    gap = selected[0]["origin"][0] - label[-1]["bbox"][2]
    assert .1 < gap <= min(label[-1]["size"], selected[0]["size"]), "Probe word lacks a real bounded marker gap"
    assert all(a["origin"][0] < b["origin"][0] for a, b in zip(selected, selected[1:])), "Physical probe glyphs are out of order"
    end = start + len(word)
    if end < len(glyphs):
        following = glyphs[end]
        if (re.match(r"\w", following["text"]) and following["page"] == selected[-1]["page"]
                and abs(following["origin"][1] - selected[-1]["origin"][1]) < .1):
            assert following["origin"][0] - selected[-1]["bbox"][2] > .1, "Probe is a prefix of a longer tight word"
    return selected, {"actualGapPt": gap, "spaceGlyphCount": len(spaces),
                      "page": row[0]["page"], "baseline": row[0]["origin"][1]}


def prove_pdf(file, family, audit_file, report):
    report.update(pdfSha256=sha(file.read_bytes()), family=family, auditSha256=sha(audit_file.read_bytes()), rows=[], embeddedFonts=[])
    sources = source_fonts(audit_file, family)
    document = fitz.open(file)
    glyphs, fonts, font_records, page_font_names, page_font_aliases, text = [], {}, {}, {}, {}, []
    for page_index, page in enumerate(document):
        text.append(page.get_text())
        for xref, extension, font_type, basefont, resource, encoding, *rest in page.get_fonts(full=True):
            if xref not in fonts:
                extracted_name, extracted_extension, _, data = document.extract_font(xref)
                assert data, "The physical font program is not embedded"
                fonts[xref] = OutlineFont(data)
                record = {"xref": xref, "baseFont": basefont, "fontType": font_type,
                                                "extension": extracted_extension, "sha256": fonts[xref].sha256,
                                                "bytes": len(data), "names": fonts[xref].names,
                                                "nameSources": fonts[xref].name_sources,
                                                "pdfNameSources": pdf_font_name_sources(document, xref, basefont, extracted_name),
                                                "cidBinding": None}
                report["embeddedFonts"].append(record)
                font_records[xref] = record
            # PDF BaseFont names may be truncated while texttrace reports the
            # embedded PostScript name (MuPDF retains at most 31 characters).
            # Accept only a unique exact full/31-character alias;
            # never choose a resource because its glyph matches an expectation.
            for source in [{"source": "pdf.BaseFont", "value": basefont}, *fonts[xref].name_sources]:
                alias = source["value"]
                unprefixed = re.sub(r"^[A-Z]{6}\+", "", alias)
                for exact, truncated in ((alias, False), (alias[:31], len(alias) > 31),
                                         (unprefixed, False), (unprefixed[:31], len(unprefixed) > 31)):
                    key = (page_index, font_key(exact))
                    page_font_names.setdefault(key, set()).add(xref)
                    match = {**source, "xref": xref, "matchedAlias": exact, "truncated": truncated}
                    matches = page_font_aliases.setdefault(key, [])
                    if match not in matches:
                        matches.append(match)
        for trace in sorted(page.get_texttrace(), key=lambda span: span["seqno"]):
            key = (page_index, font_key(trace["font"]))
            candidates = page_font_names.get(key, set())
            xref = next(iter(candidates)) if len(candidates) == 1 else None
            for unicode_value, gid, origin, bbox in trace["chars"]:
                if glyphs and (glyphs[-1]["page"] != page_index or abs(glyphs[-1]["origin"][1] - origin[1]) > .1):
                    glyphs.append({"text": "\n", "page": page_index, "origin": list(origin)})
                glyphs.append({"text": chr(unicode_value), "gid": gid, "font": trace["font"], "xref": xref,
                               "fontCandidates": sorted(candidates), "fontAliasMatches": page_font_aliases.get(key, []),
                               "page": page_index, "size": trace["size"], "origin": list(origin), "bbox": list(bbox)})
    copied = re.sub(r"\s+", " ", "\n".join(text))
    rows = {}
    for face, (bold, italic) in FACES.items():
        for mode in ("NORMAL", "SMALL", "UPPER"):
            marker, word = face + "_" + mode, WORD.upper() if mode == "UPPER" else WORD
            selected, boundary = locate_probe(glyphs, copied, marker, word)
            physical = []
            for index, glyph in enumerate(selected):
                assert glyph["xref"] in fonts, "Actual glyph cannot be linked to one embedded font resource"
                embedded = fonts[glyph["xref"]]
                assert any(font_key(family) in font_key(name) for name in embedded.names), "Control and small-caps use different font families"
                # MuPDF's 31-character alias can truncate a style suffix. After
                # the unique actual resource mapping, classify its complete
                # embedded names, not a repaired alias or the requested face.
                observed = font_key(" ".join(embedded.names))
                assert ("bold" in observed) == bold and ("italic" in observed or "oblique" in observed) == italic, "Physical glyph uses the wrong font face"
                # Bind only actual probe resources. Unrelated page fonts (for
                # example a CJK date) are not part of the 72-glyph proof.
                if embedded.format == "cff" and hasattr(embedded.cff.topDictIndex[0], "ROS") and embedded.cid_map is None:
                    font_records[glyph["xref"]]["cidBinding"] = bind_pdf_cids(document, glyph["xref"], embedded,
                                                                            document.extract_font(glyph["xref"])[3])
                actual = embedded.outline(glyph["gid"])
                original = sources[face]
                source_name = original["cmap"][ord(word[index])]
                if mode == "SMALL" and word[index].islower():
                    source_name = original["smcp"][source_name]
                expected = original["font"].outline(source_name)
                assert actual["outlineSha256"] == expected["outlineSha256"], "Embedded physical glyph does not match the independently audited original " + mode + " outline"
                physical.append({**glyph, **actual, "embeddedSha256": embedded.sha256,
                                 "sourceFontSha256": original["font"].sha256, "sourceGlyph": source_name,
                                 "traceIntegerKind": "CID" if embedded.cid_map is not None else "GID",
                                 "embeddedGlyphName": embedded.glyph_name(glyph["gid"])})
            assert max(glyph["origin"][1] for glyph in physical) - min(glyph["origin"][1] for glyph in physical) < .1, "Small-caps moved individual glyph baselines"
            rows[(face, mode)] = physical
            report["rows"].append({"face": face, "mode": mode, "glyphs": physical, "markerBoundary": boundary})
        normal, small, upper = (rows[(face, mode)] for mode in ("NORMAL", "SMALL", "UPPER"))
        sizes = [glyph["size"] for row in (normal, small, upper) for glyph in row]
        assert max(sizes) - min(sizes) < .01, "Small-caps were synthesized by changing the physical point size"
        assert small[0]["outlineSha256"] == normal[0]["outlineSha256"] == upper[0]["outlineSha256"], "The original uppercase A changed in small-caps"
        for index in range(1, len(WORD)):
            assert small[index]["outlineSha256"] not in (normal[index]["outlineSha256"], upper[index]["outlineSha256"]), "Small-caps are ordinary lowercase or full uppercase glyphs"
            small_height = small[index]["bounds"][3] - small[index]["bounds"][1]
            upper_height = upper[index]["bounds"][3] - upper[index]["bounds"][1]
            assert .4 < small_height / upper_height < .95, "Physical small-caps glyph height does not distinguish it from a full capital"
    document.close()
    report.update(passed=True, faces=len(FACES), glyphsVerified=len(FACES) * 3 * len(WORD),
                  proof="Actual PDF glyph IDs, point size, baseline and embedded outlines match locked source normal/uppercase/GSUB smcp glyphs; original Unicode copy text is preserved.")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    audit = sub.add_parser("audit-typst")
    audit.add_argument("pack", type=Path)
    audit.add_argument("lock", type=Path)
    audit.add_argument("destination", type=Path)
    prove = sub.add_parser("prove")
    prove.add_argument("file", type=Path)
    prove.add_argument("--family", required=True)
    prove.add_argument("--font-audit", type=Path, required=True)
    prove.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if args.action == "audit-typst":
        print(json.dumps(audit_typst(args.pack, args.lock, args.destination), ensure_ascii=False))
        return
    assert not args.report.exists(), "Preserve the first physical glyph evidence"
    report = {"passed": False, "noAutomaticRetries": True, "qaOnly": True}
    try:
        prove_pdf(args.file, args.family, args.font_audit, report)
    except Exception:
        report["error"] = traceback.format_exc()
    args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "report": str(args.report), "faces": report.get("faces"), "glyphsVerified": report.get("glyphsVerified")}))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
