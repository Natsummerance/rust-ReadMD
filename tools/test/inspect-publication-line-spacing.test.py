"""Positive/negative controls for the independent P04 PDF checker."""
import json
from pathlib import Path
import runpy
import tempfile

import fitz

inspect = runpy.run_path(str(Path(__file__).with_name("inspect-publication-line-spacing.py")))["inspect"]
root = Path(__file__).resolve().parents[2]
parent = root / ".cache/capability-upgrade"
parent.mkdir(exist_ok=True, parents=True)
folder = Path(tempfile.mkdtemp(prefix="line-spacing-checker-controls-", dir=parent))
results = []
cases = [
    ("correct", ["FIRST", "SECOND", "THIRD"], [60, 78, 96], True),
    ("wrong-gap", ["FIRST", "SECOND", "THIRD"], [60, 76, 92], False),
    ("missing", ["FIRST", "THIRD"], [60, 78], False),
    ("reordered", ["SECOND", "FIRST", "THIRD"], [60, 78, 96], False),
    ("duplicate", ["FIRST", "SECOND", "SECOND", "THIRD"], [60, 78, 96, 114], False),
    ("metadata-leak", ["FIRST", "SECOND", "THIRD", "readmd:layout"], [60, 78, 96, 120], False),
]
for name, texts, positions, should_pass in cases:
    filename = folder / (name + ".pdf")
    doc = fitz.open()
    page = doc.new_page()
    for text, y in zip(texts, positions):
        page.insert_text((60, y), text, fontsize=11)
    doc.save(filename)
    doc.close()
    try:
        inspect(str(filename), 18)
        actual = True
    except AssertionError:
        actual = False
    assert actual == should_pass, name
    results.append({"name": name, "expectedAcceptance": should_pass, "matched": True})
(folder / "report.json").write_text(json.dumps(results, indent=2), encoding="utf8")
print(json.dumps({"passed": len(results), "report": str((folder / "report.json").relative_to(root))}))
