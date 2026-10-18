"""Independent QA of real output, not an application dependency."""
import json
import re
import sys
from pathlib import Path
import pymupdf as fitz
from fontTools.ttLib import TTFont
from io import BytesIO

root = Path(sys.argv[1])
results = {}
for name in ["Native", "Typst"]:
    doc = fitz.open(root / (name + ".pdf"))
    text = "".join(page.get_text() for page in doc)
    for term in ["研究记录", "观察与解释", "保留原始证据"]:
        assert term in text, (name, term)
    fonts = {font[0]: font for page in doc for font in page.get_fonts(full=True)}
    embedded = []
    exchange = []
    for xref, info in fonts.items():
        if info[3] == "ReadMDMathText":
            # Type3 embeds PDF glyph programs, not an extractable TrueType
            # font file. This is the searchable exchange layer; the visible
            # formula remains vector outlines and is checked separately.
            assert info[2] == "Type3", (name, info)
            kind, procedures = doc.xref_get_key(xref, "CharProcs")
            assert kind == "dict", (name, info, kind)
            programs = set(int(n) for n in re.findall(r"/g\d+\s+(\d+)\s+0\s+R", procedures))
            assert programs and all(doc.xref_stream(n) for n in programs)
            kind, mapping = doc.xref_get_key(xref, "ToUnicode")
            assert kind == "xref", (name, info, kind)
            cmap = doc.xref_stream(int(mapping.split()[0]))
            assert cmap and b"beginbfchar" in cmap and b"endcmap" in cmap
            exchange.append({"name": info[3], "programs": len(programs), "unicodeMapBytes": len(cmap)})
            continue
        if "ReadMD" not in info[3]:
            continue
        fontname, ext, kind, data = doc.extract_font(xref)
        assert data and ext == "ttf", (name, info)
        face = TTFont(BytesIO(data))
        assert "glyf" in face and "fvar" not in face and "gvar" not in face
        assert face["maxp"].numGlyphs > 3
        embedded.append({"name": fontname, "bytes": len(data), "glyphs": face["maxp"].numGlyphs})
    assert any("Regular" in font["name"] for font in embedded), (name, embedded)
    assert any("Bold" in font["name"] for font in embedded), (name, embedded)
    assert not any("STSong" in info[3] for info in fonts.values()), (name, fonts)
    page = doc[0]
    page.get_pixmap(matrix=fitz.Matrix(2, 2), alpha=False).save(root / (name + ".png"))
    results[name] = {"pages": len(doc), "embedded": embedded, "mathExchange": exchange, "textChecked": True, "vectors": len(page.get_drawings())}
(root / "font-inspection.json").write_text(json.dumps(results, indent=2) + "\n")
print(json.dumps(results))
