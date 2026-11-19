"""Check exports of the actual browser-edited table, independently of the Rust layout."""
import json
import runpy
import sys
from pathlib import Path

import pdfplumber

suite = runpy.run_path(str(Path(__file__).with_name("v007-pdf-geometry.py")))
directory = Path(sys.argv[1])
width, height = (int(n) * 0.75 for n in sys.argv[2:])
pages, sample_ids, positions = [], [], {}
with pdfplumber.open(directory / "resized.pdf") as pdf:
    assert len(pdf.pages) >= 2, "expected a physical page break"
    for page in pdf.pages:
        rules = suite["read_rules"](page)
        words = page.extract_words(x_tolerance=2, y_tolerance=2)
        headers = [w for w in words if w["top"] < rules[1][2]]
        assert [w["text"] for w in headers] == ["Group", "Batch", "Sample", "RFU"]
        actual = headers[1]["x0"] - headers[0]["x0"]
        suite["assert_close"](actual, width, "dragged width in exported PDF")
        for w in words:
            if w in headers:
                continue
            assert rules[1][2] < w["top"] < w["bottom"] < rules[2][2], "body intersects a rule"
            label = w["text"]
            if len(label) == 3 and label[0] == "S" and label[1:].isdigit():
                sample_ids.append(label)
                positions[label] = (page.page_number,w["top"])
        pages.append({"number":page.page_number,"sizePt":[page.width,page.height],"rules":rules,"words":words})
    assert sample_ids == [f"S{i:02d}" for i in range(1,27)], "sample order, count or identity changed"
    words = [w["text"] for p in pages for w in p["words"]]
    for value in ["West","Dry","Red","Vial","North","Cold","Dark","Tube"]:
        assert words.count(value) == 1, f"missing or duplicated pasted value {value}"
    assert positions["S05"][0] == positions["S06"][0], "fixture measurement needs consecutive rows on one page"
    row_gap = positions["S06"][1] - positions["S05"][1]
    suite["assert_close"](row_gap, height, "dragged row minimum in exported PDF")
segments = suite["verify_pngs"](directory, "resized", pages)
print(json.dumps({"pages":len(pages),"rows":len(sample_ids),"columnWidthPt":actual,
                  "rowHeightPt":row_gap,"pngSegments":segments}))
