"""Independent QA of actual pagination; never used by application runtime."""
import hashlib
import json
import re
import sys
from pathlib import Path

import pymupdf


def inspect(mapped, control, source_map):
    left, right = pymupdf.open(mapped), pymupdf.open(control)
    assert len(left) == len(right) >= 2, "Actual page counts must match"
    original_links = []
    for index, (a, b) in enumerate(zip(left, right)):
        assert a.rect == b.rect, "Physical paper geometry differs"
        pa, pb = a.get_pixmap(matrix=pymupdf.Matrix(1.5, 1.5)), b.get_pixmap(matrix=pymupdf.Matrix(1.5, 1.5))
        assert pa.samples == pb.samples, f"Mapping changes real pixels on page {index + 1}"
        assert a.get_text() == b.get_text(), "Mapping changes extracted document content"
        la, lb = a.get_links(), b.get_links()
        assert len(la) == len(lb), "Mapping removes or adds actual user links"
        for x, y in zip(la, lb):
            assert x.get("uri") == y.get("uri") and x["from"] == y["from"], "User link target or geometry changed"
        original_links.extend(link.get("uri") for link in la if link.get("uri"))
    # Removed annotations leave legal free xref slots. Only referenced missing
    # objects are dangling; querying every free slot is itself an invalid QA.
    present = {}
    for index in range(1, left.xref_length()):
        try:
            value = left.xref_object(index)
            if value and value != "null":
                present[index] = value
        except RuntimeError:
            continue
    for value in present.values():
        for reference in re.findall(r"(\d+) 0 R", value):
            assert int(reference) in present, "Dangling PDF object reference"
    objects = "\n".join(present.values())
    assert "readmd-internal:publication/" not in objects, "Private mapping URI leaked"
    assert "StructTreeRoot" in objects and "/MCR" in objects, "Tagged semantic content lost"
    assert "https://example.test/research" in original_links, "Original HTTP link missing"
    text = "\n".join(page.get_text() for page in left)
    assert "Equation 1" in text and "Note origin" in text, "Caption/reference/footnote lost"
    mapping = json.loads(Path(source_map).read_text(encoding="utf-8"))
    assert len(mapping["pages"]) == len(left)
    assert len({hit["page"] for hit in mapping["hits"]}) >= 2
    for index, page in enumerate(left):
        for link in page.get_links():
            if link.get("uri") == "https://example.test/research":
                box = link["from"]
                x, y = (box.x0 + box.x1) / 2, (box.y0 + box.y1) / 2
                assert any(hit["page"] == index + 1 and hit["rect"][0] <= x <= hit["rect"][0] + hit["rect"][2] and hit["rect"][1] <= y <= hit["rect"][1] + hit["rect"][3] for hit in mapping["hits"]), "An actual link leaves a non-editable hole in its paragraph"
    note_hits = []
    for index, page in enumerate(left):
        for word in page.get_text("words"):
            if word[4] == "Note":
                x, y = (word[0] + word[2]) / 2, (word[1] + word[3]) / 2
                note_hits.extend(hit for hit in mapping["hits"] if hit["page"] == index + 1 and hit["rect"][0] <= x <= sum(hit["rect"][::2]) and hit["rect"][1] <= y <= hit["rect"][1] + hit["rect"][3])
    assert not note_hits, "Footnotes must not incorrectly target a body paragraph"
    return {"passed": True, "pages": len(left), "pixelsEqual": True, "textEqual": True,
            "originalHttpLinks": len(original_links), "mappedRectangles": len(mapping["hits"]),
            "privateActions": 0, "danglingObjectReferences": 0, "footnoteBodyTargets": 0,
            "mappedSha256": hashlib.sha256(Path(mapped).read_bytes()).hexdigest()}


if __name__ == "__main__":
    try:
        print(json.dumps(inspect(*sys.argv[1:]), ensure_ascii=False))
    except Exception as error:
        print(json.dumps({"passed": False, "error": str(error)}, ensure_ascii=False))
        sys.exit(1)
