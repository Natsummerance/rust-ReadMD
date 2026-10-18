"""Independent QA inspection only; this is never a product dependency."""
import hashlib
import json
import sys
import zipfile
import xml.etree.ElementTree as E
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")
p = Path(sys.argv[1])
if p.suffix.lower() == ".docx":
    w = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
    m = "{http://schemas.openxmlformats.org/officeDocument/2006/math}"
    with zipfile.ZipFile(p) as z:
        assert z.testzip() is None
        names = z.namelist()
        assert len(names) == len(set(names))
        assert sum(i.file_size for i in z.infolist()) <= 32 * 1024 * 1024
        parts = {name: z.read(name) for name in names if name.endswith((".xml", ".rels"))}
    parsed = {name: E.fromstring(data) for name, data in parts.items()}
    doc = parsed["word/document.xml"]
    paragraphs = []
    for para in doc.iter(w + "p"):
        runs = []
        for run in para.iter(w + "r"):
            pr = run.find(w + "rPr")
            positions = [] if pr is None else pr.findall(w + "position")
            assert len(positions) <= 1
            properties = {} if pr is None else {q.tag[len(w):]: {k[len(w):]: v for k, v in q.attrib.items()} for q in pr}
            runs.append({"text": "".join(t.text or "" for t in run.iter(w + "t")),
                         "positionHalfPt": None if not positions else int(positions[0].get(w + "val")), "properties": properties})
        paragraphs.append({"text": "".join(t.text or "" for t in para.iter(w + "t")), "runs": runs})
    result = {"format": "docx", "crcAndXml": True, "wordRoundTrip": False, "paragraphs": paragraphs,
              "mathHashes": [hashlib.sha256(E.tostring(n, encoding="utf-8")).hexdigest() for n in doc.iter(m + "oMath")],
              "mathPositions": sum(len(list(n.iter(w + "position"))) for n in doc.iter(m + "oMath")),
              "stylesSha256": hashlib.sha256(parts["word/styles.xml"]).hexdigest(),
              "sourceLeak": any(b"readmd:layout" in data or b"baselineShiftPt" in data for data in parts.values())}
else:
    import pymupdf as fitz
    doc = fitz.open(p)
    glyphs, drawings, links, fonts = [], [], [], {}
    for number, page in enumerate(doc, 1):
        for block in page.get_text("rawdict")["blocks"]:
            for line in block.get("lines", []):
                for span in line["spans"]:
                    for char in span["chars"]:
                        glyphs.append({"page": number, "c": char["c"], "origin": list(char["origin"]),
                                       "bbox": list(char["bbox"]), "font": span["font"], "size": span["size"]})
        drawings.append({"page": number, "paths": repr(page.get_drawings())})
        links.extend({"page": number, "uri": link.get("uri"), "rect": list(link["from"])} for link in page.get_links())
        for font in page.get_fonts(full=True):
            xref, base = font[0], font[3]
            program = doc.extract_font(xref)[3]
            kind, target = doc.xref_get_key(xref, "ToUnicode")
            mapping = doc.xref_stream(int(target.split()[0])) if kind == "xref" else b""
            fonts[base] = {"font": base, "programSha256": hashlib.sha256(program).hexdigest() if program else None,
                           "toUnicodeSha256": hashlib.sha256(mapping).hexdigest() if mapping else None}
        pix = page.get_pixmap(matrix=fitz.Matrix(1.25, 1.25), alpha=False)
        pix.save(str(p) + ".page" + str(number) + ".png")
    text = "".join(g["c"] for g in glyphs)
    markers = {}
    for marker in ["BASELINEABCDE", "BOLD", "LINK", "CODE"]:
        at = text.find(marker)
        assert at >= 0 and text.find(marker, at + len(marker)) < 0, (marker, text)
        markers[marker] = glyphs[at:at + len(marker)]
    result = {"format": "pdf", "pages": len(doc), "markers": markers, "text": text, "links": links,
              "fonts": sorted(fonts.values(), key=lambda f: f["font"]),
              "vectorPathsSha256": hashlib.sha256(json.dumps(drawings).encode("utf-8")).hexdigest(),
              "renderedPages": [str(p) + ".page" + str(number) + ".png" for number in range(1, len(doc) + 1)]}
print(json.dumps(result, ensure_ascii=False))
