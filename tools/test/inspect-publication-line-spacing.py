"""Independent actual PDF baseline checks for the P04 native increment."""
import json
import re
import sys
from pathlib import Path

import pymupdf as fitz
from pypdf import PdfReader


def inspect(filename, expected_gap, long_document=False, tall=False):
    document = fitz.open(filename)
    expected = ([f"P04ROW{i:04d}" for i in range(300)] if long_document
                else ["FIRST", "SECOND", "THIRD"])
    found = []
    paths = 0
    for page_index, page in enumerate(document):
        paths += len(page.get_drawings())
        for block in page.get_text("dict")["blocks"]:
            for line in block.get("lines", []):
                for span in line["spans"]:
                    for marker in expected:
                        if span["text"].strip() == marker:
                            x, y = span["origin"]
                            assert 0 <= x < page.rect.width and 0 < y < page.rect.height
                            found.append({"text": marker, "page": page_index + 1,
                                          "baselinePt": y, "fontSizePt": span["size"]})
    assert [item["text"] for item in found] == expected, "PDF lost, duplicated, or reordered source lines"
    gaps = []
    for previous, current in zip(found, found[1:]):
        if previous["page"] == current["page"]:
            gap = current["baselinePt"] - previous["baselinePt"]
            gaps.append(gap)
            if tall:
                assert gap >= expected_gap - 0.02, (gap, expected_gap)
            else:
                assert abs(gap - expected_gap) <= 0.02, (gap, expected_gap)
    if tall:
        assert paths > 0, "Formula must remain a real vector"
        assert max(gaps) > expected_gap + 1, "AtLeast must expand around the tall formula"
    if long_document:
        assert len(document) > 1 and len(gaps) >= 280
    other = "\n".join(page.extract_text() or "" for page in PdfReader(filename).pages)
    for marker in expected:
        assert len(re.findall(r"(?<![A-Z0-9])" + marker + r"(?![A-Z0-9])", other)) == 1
    assert "readmd:layout" not in other
    png = Path(filename).with_suffix(".page1.png")
    assert not png.exists()
    document[0].get_pixmap(matrix=fitz.Matrix(1.5, 1.5), alpha=False).save(png)
    return {"pages": len(document), "markers": len(expected), "baselines": found,
            "gapsPt": gaps, "vectorPaths": paths, "independentTextParsers": 2,
            "renderedPage": png.name, "mupdfVersion": fitz.VersionBind}


if __name__ == "__main__":
    print(json.dumps(inspect(sys.argv[1], float(sys.argv[2]),
                             "--long" in sys.argv[3:], "--tall" in sys.argv[3:]),
                     ensure_ascii=True))
