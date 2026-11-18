"""Verify the research fixture's concentrations, physical table fragments and PNG exports."""

import json
import runpy
import statistics
import sys
from collections import Counter
from decimal import Decimal
from pathlib import Path

import pdfplumber


qa = runpy.run_path(str(Path(__file__).with_name("v007-pdf-geometry.py")))
assert_close = qa["assert_close"]
HEADINGS = ("Run and preparation", "Dose", "Mean", "CI low", "CI high")
RUNS = ("Fresh dilution", "Held dilution")
FRAME_PADDING_PT = 6


def source_rows(file):
    lines = file.read_text(encoding="utf-8").splitlines()
    cells = [[cell.strip() for cell in line[1:-1].split("|")] for line in lines if line.startswith("|")]
    assert all(len(row) == 6 for row in cells), "research source must contain six columns"
    rows = cells[2:]
    assert len(rows) == 42, "research source must contain 21 dose levels per run"
    for start, name in zip((0, 21), RUNS):
        assert rows[start][0] == "**" + name + "** {colspan=2 rowspan=21 align=left valign=top}"
    expected = [tuple(row[2:]) for row in rows]
    doses = [row[0] for row in expected]
    assert doses[:21] == doses[21:], "runs must use the same nominal dose levels"
    assert list(map(Decimal, doses[:21])) == sorted(set(map(Decimal, doses[:21]))), "doses must increase"
    for _, mean, low, high in expected:
        assert Decimal(low) < Decimal(mean) < Decimal(high), "confidence interval must contain its mean"
    return expected


def blue_rules(page):
    def is_rule_color(color):
        return isinstance(color, (list, tuple)) and len(color) == 3 and all(
            abs(channel * 255 - expected) <= 1 for channel, expected in zip(color, qa["RULE_RGB"])
        )

    rules = []
    for line in page.lines:
        if is_rule_color(line["stroking_color"]):
            assert_close(line["top"], line["bottom"], "table rule orientation", tolerance=0.05)
            rules.append((line["x0"], line["x1"], line["top"], line["linewidth"], line["stroking_color"]))
    for rect in page.rects:
        if is_rule_color(rect["non_stroking_color"]):
            assert rect["fill"] and not rect["stroke"], "filled table rule has an unexpected border"
            rules.append((rect["x0"], rect["x1"], (rect["top"] + rect["bottom"]) / 2, rect["height"], rect["non_stroking_color"]))
    return sorted(rules, key=lambda rule: rule[2])


def table_fragment(words, rules, page_number):
    assert len(rules) == 3, f"page {page_number} must have three table rules"
    left, right = rules[0][:2]
    top, separator, bottom = (rule[2] for rule in rules)
    for rule, weight in zip(rules, (2, 1, 2)):
        assert_close(rule[0], left, "left table edge")
        assert_close(rule[1], right, "right table edge")
        assert_close(rule[3], weight, "table rule width", tolerance=0.08)
    column = (right - left) / 6
    data_start = left + 2 * column
    header_cells = [[] for _ in HEADINGS]
    for word in words:
        if top < word["top"] < separator:
            assert left < word["x0"] < word["x1"] < right and word["bottom"] < separator, "clipped table header"
            index = 0 if word["x0"] < data_start else 1 + int((word["x0"] - data_start) / column)
            assert word["x1"] <= data_start + index * column, "header label overflowed its cell"
            header_cells[index].append(word["text"])
    headers = tuple(" ".join(cell) for cell in header_cells)
    assert headers == HEADINGS, f"incorrect header on page {page_number}: {headers}"
    body = [word for word in words if separator < word["top"] < bottom]
    assert all(left < word["x0"] < word["x1"] < right and word["bottom"] < bottom for word in body), (
        f"clipped table body on page {page_number}"
    )
    group_words = [word for word in body if word["x0"] < data_start]
    assert all(word["x1"] <= data_start for word in group_words), "merged preparation text overflowed"

    numeric_columns = [[] for _ in range(4)]
    for word in body:
        if word["x0"] >= data_start:
            index = int((word["x0"] - data_start) / column)
            assert_close(word["x1"], data_start + (index + 1) * column - 6, "numeric right alignment", tolerance=1.75)
            numeric_columns[index].append(word)
    heights = {len(values) for values in numeric_columns}
    assert len(heights) == 1 and 0 not in heights, f"misaligned data or orphan header on page {page_number}"
    rows = []
    for values in zip(*numeric_columns):
        for word in values[1:]:
            assert_close(word["top"], values[0]["top"], "numeric row baseline")
        rows.append(tuple(word["text"] for word in values))
    return rows, [word["top"] for word in numeric_columns[0]], group_words, (left, separator, data_start, bottom)


def verify_page_use(pages, positions, counts):
    assert counts[1] > 0, "the first page ended before any research data despite room under the caption"
    table_pages = [page for page in pages if page["rules"]]
    assert [page["number"] for page in table_pages] == list(range(1, table_pages[-1]["number"] + 1)), (
        "an empty page interrupted the research table"
    )
    # Compare the free space before each continuation with the measured pitch of
    # regular numeric rows. The extra half-pitch allows padding around carried ink.
    gaps = [top2 - top1 for (page1, top1), (page2, top2) in zip(positions, positions[1:]) if page1 == page2]
    assert gaps, "no adjacent research rows share a page"
    allowance = statistics.median(gaps) * 1.5
    remaining = {}
    for page in table_pages:
        number = page["number"]
        rule_bottom = page["rules"][-1][2]
        frame_bottom = page["sizePt"][1] - qa["MARGIN"] - FRAME_PADDING_PT
        space = frame_bottom - rule_bottom
        assert space >= -0.35, f"page {number} table extends past the content frame"
        if number != table_pages[-1]["number"]:
            assert space <= allowance, f"page {number} leaves {space:.1f} pt unused before the next table fragment"
        remaining[number] = round(space, 2)
    return remaining


def verify_research(directory):
    expected = source_rows(directory / "research.md")
    traces, rows, positions, group, links = [], [], [], [], []
    with pdfplumber.open(directory / "research.pdf") as document:
        note = None
        for page in document.pages:
            number = page.page_number
            for actual, target in zip((page.width, page.height), qa["PAGE_SIZE"]):
                assert_close(actual, target, "A5 page dimension")
            words = page.extract_words(x_tolerance=2, y_tolerance=2)
            rules = blue_rules(page)
            if rules:
                actual, baselines, labels, region = table_fragment(words, rules, number)
                rows.extend(actual)
                positions.extend((number, baseline) for baseline in baselines)
                group.extend((number, word) for word in labels)
                links.extend((annot, region) for annot in page.annots if "Dest" in annot["data"])
            text = " ".join(word["text"] for word in words)
            if "Each dose represents four technical replicate wells." in text:
                assert "95% Student's t interval for the mean." in text, "confidence interval footnote is incomplete"
                assert note is None, "footnote definition repeated"
                note = (page.page_obj.pageid, page.height, next(word["top"] for word in words if word["text"] == "Each"))
            traces.append({"number": number, "sizePt": [page.width, page.height], "rules": rules, "words": words})

        assert rows == expected, "PDF numeric data changed, reordered or dropped a dose"
        run_pages = [sorted({page for page, _ in positions[start:start + 21]}) for start in (0, 21)]
        assert all(len(pages) >= 2 for pages in run_pages), "each 2D merged group should cross a page boundary"
        group_text = " ".join(word["text"] for _, word in group)
        for phrase in (*RUNS, "Prepared immediately before", "The fresh and held", "Kept in the dark", "The plate reader used"):
            assert group_text.count(phrase) == 1, f"merged label or preparation text missing or repeated: {phrase!r}"
        for start, run in zip((0, 21), RUNS):
            page, label = next((page, word) for page, word in group if word["text"] == run.split()[0])
            expected_page, expected_top = positions[start]
            assert page == expected_page, f"{run} is missing next to its first measurement"
            assert_close(label["top"], expected_top, f"{run} alignment to first measurement", tolerance=1.5)
        lines = {(number, round(word["top"], 1)) for number, word in group}
        assert len(lines) >= 8, "the merged preparation text should wrap into several physical lines"
        assert note is not None, "footnote definition is absent"
        note_page, height, note_top = note
        for annot, (left, separator, data_start, bottom) in links:
            if left < annot["x0"] < data_start and separator < annot["top"] < bottom:
                target = annot["data"]["Dest"]
                if target[0].objid == note_page and abs(height - target[3] - note_top) < 14:
                    break
        else:
            raise AssertionError("merged-cell reference does not link to the confidence interval footnote")

    page_rows = Counter(page for page, _ in positions)
    remaining = verify_page_use(traces, positions, page_rows)
    return {
        "passed": True, "doseRows": len(rows), "pages": len(traces),
        "doseRowsByPage": [page_rows[page["number"]] for page in traces],
        "remainingBodyPtByPage": [remaining.get(page["number"]) for page in traces],
        "repeatedHeaders": sum(bool(page["rules"]) for page in traces),
        "mergedGroupPages": dict(zip(RUNS, run_pages)), "mergedTextLines": len(lines),
        "footnoteLinked": True,
        "pngSegments": qa["verify_pngs"](directory, "research", traces),
    }


if __name__ == "__main__":
    print(json.dumps(verify_research(Path(sys.argv[1]))))
