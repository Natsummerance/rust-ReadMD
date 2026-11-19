"""Check physical HTTP export files with independent PDF and PNG readers."""

import io
import json
import math
import statistics
import sys
import zipfile
from collections import Counter, defaultdict
from pathlib import Path

import pdfplumber
from PIL import Image, ImageChops, ImageDraw


PT_PER_MM = 72 / 25.4
PAGE_SIZE = (148 * PT_PER_MM, 210 * PT_PER_MM)
MARGIN = 16 * PT_PER_MM
TABLE_WIDTH = (PAGE_SIZE[0] - 2 * MARGIN - 12) * 0.9
RULE_RGB = (20, 79, 173)
LINE_HEIGHT = 11 * 1.35
REGULAR_ROW_HEIGHT = LINE_HEIGHT + 12
LONG_PNG_PADDING = 16


def numbered(text, start, stop, digits=3):
    return [f"{text}{index:0{digits}d}" for index in range(start, stop + 1)]


ORDINARY_TRACE = numbered("OTRACE", 1, 8, 2)
MERGED_TRACE = numbered("MTRACE", 1, 6, 2)
MERGED_CONTENT = [f"MGROUP{index:03d}{part}" for index in range(3, 35) for part in "AB"]
EXPECTED = {
    "ordinary": {
        "head": ["ORDSAMPLE", "ORDMETHOD", "ORDRESULT"],
        "body": [
            numbered("OITEM", 1, 50),
            numbered("OCTRL", 1, 5) + ORDINARY_TRACE + numbered("OCTRL", 6, 50),
            numbered("ODATA", 1, 50),
        ],
    },
    "merged": {
        "head": ["MERGROUP", "MERINDEX", "MERRESULT"],
        "body": [
            numbered("MSINGLE", 0, 2) + MERGED_CONTENT + numbered("MSINGLE", 35, 39),
            ["MINDEX000"] + MERGED_TRACE + numbered("MINDEX", 1, 39),
            numbered("MRESULT", 0, 39),
        ],
    },
}


def assert_close(actual, expected, field, tolerance=0.35):
    assert abs(actual - expected) <= tolerance, f"{field} measured {actual:.3f}, expected {expected:.3f}"


def assert_rule_color(ink):
    assert isinstance(ink, (list, tuple)) and len(ink) == 3, f"invalid rule color {ink}"
    for channel, expected in zip(ink, RULE_RGB):
        assert_close(channel * 255, expected, "rule color", tolerance=1)


def read_rules(page):
    assert not page.curves, f"page {page.page_number} contains an unexpected vector path"
    rules = []
    for line in page.lines:
        assert abs(line["top"] - line["bottom"]) < 0.05, f"page {page.page_number} has a non-horizontal line"
        rules.append((line["x0"], line["x1"], line["top"], line["linewidth"], line["stroking_color"]))
    for rect in page.rects:
        assert rect.get("fill") and not rect.get("stroke"), f"page {page.page_number} has a rectangle with a stroked edge"
        rules.append((rect["x0"], rect["x1"], (rect["top"] + rect["bottom"]) / 2, rect["height"], rect["non_stroking_color"]))
    assert len(rules) == 3, f"page {page.page_number} has {len(rules)} table rules, expected top, header and bottom"
    rules.sort(key=lambda rule: rule[2])
    for (x0, x1, _y, weight, ink), expected_weight in zip(rules, [2, 1, 2]):
        assert_close(x1 - x0, TABLE_WIDTH, "rule width")
        assert_close((x0 + x1) / 2, page.width / 2, "rule center")
        assert_close(weight, expected_weight, "rule weight", tolerance=0.08)
        assert_rule_color(ink)
    top, separator, bottom = (rule[2] for rule in rules)
    assert MARGIN <= top < separator < bottom <= page.height - MARGIN, "rules outside the page margins"
    assert_close(separator - top, REGULAR_ROW_HEIGHT, "header height")
    return rules


def assert_aligned(word, left, right, alignment):
    if alignment == "left":
        assert_close(word["x0"], left + 6, word["text"] + " left alignment", tolerance=1.75)
    elif alignment == "right":
        assert_close(word["x1"], right - 6, word["text"] + " right alignment", tolerance=1.75)
    else:
        assert_close((word["x0"] + word["x1"]) / 2, (left + right) / 2, word["text"] + " center alignment", tolerance=1.75)


def read_page(page, expected, body_columns, words_by_name, ordinary):
    for actual, target in zip((page.width, page.height), PAGE_SIZE):
        assert_close(actual, target, "A5 page dimension")
    rules = read_rules(page)
    left, right = rules[0][:2]
    column_width = (right - left) / 3
    top, separator, bottom = (rule[2] for rule in rules)
    words = page.extract_words(x_tolerance=2.0, y_tolerance=2.0)
    prefix = f"page {page.page_number}"
    headers = sorted([word for word in words if word["text"] in expected["head"]], key=lambda word: word["x0"])
    assert [word["text"] for word in headers] == expected["head"], f"{prefix} missing or repeated header"
    body = [word for word in words if word["text"] not in expected["head"]]
    assert body, f"{prefix} has an orphan header"
    last_ink_top = max(word["top"] for word in body)
    if ordinary:
        top_inset = headers[0]["top"] - top
        assert_close(bottom - last_ink_top, REGULAR_ROW_HEIGHT - top_inset, f"{prefix} bottom rule placement")
    allowed_columns = {text: index for index, column in enumerate(expected["body"]) for text in column}
    for index, word in enumerate(headers):
        assert top + 1 < word["top"] < word["bottom"] < separator - 0.5, f"{prefix} header intersects a rule"
        assert_aligned(word, left + index * column_width, left + (index + 1) * column_width, ["left", "center", "right"][index])
        assert_close(word["top"], headers[0]["top"], "header baseline")
    for word in body:
        text = word["text"]
        assert text in allowed_columns, f"{prefix} contains unexpected marker {text!r}"
        column = allowed_columns[text]
        cell_left = left + column * column_width
        cell_right = cell_left + column_width
        assert separator + 0.5 < word["top"] < word["bottom"] < bottom - 1, (
            f"{prefix} {text} at {word['top']:.3f}–{word['bottom']:.3f} pt intersects a table rule "
            f"({separator:.3f} and {bottom:.3f} pt)"
        )
        assert cell_left < word["x0"] < word["x1"] < cell_right, f"{prefix} {text} overflows its column"
        alignment = "right" if text.startswith("MGROUP") else ["left", "center", "right"][column]
        assert_aligned(word, cell_left, cell_right, alignment)
        assert text not in words_by_name, f"{prefix} repeats {text}"
        words_by_name[text] = {**word, "page": page.page_number}
        body_columns[column].append(text)

    for index in range(3):
        column_words = [word for word in body if allowed_columns[word["text"]] == index]
        for first, second in zip(column_words, column_words[1:]):
            overlaps_x = min(first["x1"], second["x1"]) - max(first["x0"], second["x0"]) > 1
            overlaps_y = min(first["bottom"], second["bottom"]) - max(first["top"], second["top"]) > 1
            assert not (overlaps_x and overlaps_y), f"{prefix} overlaps {first['text']} and {second['text']}"
    return {"number": page.page_number, "sizePt": [page.width, page.height], "rules": rules, "words": words}


def verify_row_heights(name, words):
    ordinary = name == "ordinary"
    if ordinary:
        left_names = numbered("OITEM", 1, 50)
        indices = numbered("OCTRL", 1, 50)
        right_names = numbered("ODATA", 1, 50)
        tall_row = 4
        traces = ORDINARY_TRACE
    else:
        left_names = numbered("MSINGLE", 0, 2) + [None] * 32 + numbered("MSINGLE", 35, 39)
        indices = numbered("MINDEX", 0, 39)
        right_names = numbered("MRESULT", 0, 39)
        tall_row = 0
        traces = MERGED_TRACE

    rows = [words[text] for text in indices]
    ordinary_gaps = []
    for index, (row, left_name, right_name) in enumerate(zip(rows, left_names, right_names)):
        right = words[right_name]
        assert right["page"] == row["page"], f"{right_name} was detached from its row"
        if index != tall_row or ordinary:
            assert_close(right["top"], row["top"], right_name + " top alignment")
        if left_name is not None:
            left = words[left_name]
            assert left["page"] == row["page"], f"{left_name} was detached from its row"
            assert_close(left["top"], row["top"], left_name + " top alignment")
        if index + 1 < len(rows) and rows[index + 1]["page"] == row["page"]:
            if index != tall_row and (ordinary or index == 1 or index >= 35):
                ordinary_gaps.append(rows[index + 1]["top"] - row["top"])

    assert len(ordinary_gaps) >= 2, "fixture did not expose enough regular rows to measure"
    for gap in ordinary_gaps:
        assert_close(gap, REGULAR_ROW_HEIGHT, "single-line row height")
    tall = rows[tall_row]
    following = rows[tall_row + 1]
    assert tall["page"] == following["page"], "tall fixture row and its succession must fit on one page"
    trace_words = [words[text] for text in traces]
    assert all(word["page"] == tall["page"] for word in trace_words), "multiline row split unexpectedly"
    trace_bottom = max(word["bottom"] for word in trace_words)
    trace_top = max(word["top"] for word in trace_words)
    tall_height = following["top"] - tall["top"]
    assert tall_height > 2 * REGULAR_ROW_HEIGHT and following["top"] - trace_bottom > 8, "wrapped text failed to expand its row"
    assert_close(following["top"] - trace_top, REGULAR_ROW_HEIGHT, "multiline row bottom padding")
    if not ordinary:
        assert_close(words[right_names[tall_row]]["top"], trace_top, "short cell bottom alignment")
    summary = {"regularRowHeightPt": statistics.median(ordinary_gaps), "tallRowHeightPt": tall_height}
    if not ordinary:
        group = [words[text] for text in MERGED_CONTENT]
        group_pages = sorted({word["page"] for word in group})
        assert len(group_pages) >= 2 and rows[3]["page"] < rows[34]["page"], "rowspan fixture did not cross a physical page"
        assert len({(word["page"], round(word["top"], 2)) for word in group}) == len(group), "merged body did not retain its wrap and line order"
        assert LINE_HEIGHT * len(group) > PAGE_SIZE[1] - 2 * MARGIN, "merged content no longer exceeds one page"
        summary["mergedContentPages"] = group_pages
        summary["mergedContentLines"] = len(group)
    return summary


def pixel_box(x0, y0, x1, y1, scale_x, scale_y, tolerance):
    return (math.floor(x0 * scale_x) - tolerance, math.floor(y0 * scale_y) - tolerance,
            math.ceil(x1 * scale_x) + tolerance, math.ceil(y1 * scale_y) + tolerance)


def verify_raster(page, image):
    width, height = page["sizePt"]
    sx, sy = image.width / width, image.height / height
    assert sx >= 1 and abs(sx / sy - 1) < 0.01, "page PNG size or aspect ratio does not match the PDF"
    allowed = Image.new("L", image.size)
    draw = ImageDraw.Draw(allowed)
    red, green, blue = image.split()
    blue_ink = ImageChops.subtract(blue, red).point(lambda pixel: 255 if pixel > 48 else 0)
    for x0, x1, y, weight, _color in page["rules"]:
        bounds = pixel_box(x0, y - weight / 2, x1, y + weight / 2, sx, sy, 3)
        columns, _ = blue_ink.crop(bounds).getprojection()
        assert sum(columns) > 0.97 * (x1 - x0) * sx, f"table rule missing or shortened in the PNG at {y:.2f} pt"
        draw.rectangle(bounds, fill=255)
    dark = image.convert("L").point(lambda pixel: 255 if pixel < 160 else 0)
    for word in page["words"]:
        bounds = pixel_box(word["x0"], word["top"], word["x1"], word["bottom"], sx, sy, 3)
        assert dark.crop(bounds).histogram()[-1] > 8, f"PDF text position {word['text']} contains no text ink in the PNG"
        draw.rectangle(bounds, fill=255)
    darkest_channel = ImageChops.darker(ImageChops.darker(red, green), blue)
    ink = darkest_channel.point(lambda pixel: 255 if pixel < 212 else 0)
    unexplained = ImageChops.subtract(ink, allowed)
    assert unexplained.getbbox() is None, f"unexpected PNG ink outside PDF text and rules at {unexplained.getbbox()}"


def verify_pngs(directory, name, pages):
    file_names = [f"page-{page['number']:03d}.png" for page in pages]
    with zipfile.ZipFile(directory / (name + ".zip")) as archive, Image.open(directory / (name + ".png")) as long_file:
        assert archive.namelist() == file_names, "page ZIP contents do not match PDF page count and order"
        assert long_file.format == "PNG", "long image is not a PNG"
        long_image = long_file.convert("RGB")
        y = 0
        segments = []
        for page, file in zip(pages, file_names):
            with Image.open(io.BytesIO(archive.read(file))) as decoded:
                assert decoded.format == "PNG", f"{file} is not a PNG"
                image = decoded.convert("RGB")
            try:
                verify_raster(page, image)
            except AssertionError as error:
                raise AssertionError(f"{file}: {error}") from error
            background = Image.new("RGB", image.size, image.getpixel((0, 0)))
            bbox = ImageChops.difference(image, background).getbbox()
            assert bbox is not None, f"{file} is blank"
            top = max(bbox[1] - LONG_PNG_PADDING, 0)
            bottom = min(bbox[3] + LONG_PNG_PADDING, image.height)
            height = bottom - top
            assert long_image.width == image.width, "long PNG and its source pages have different widths"
            source = image.crop((0, top, image.width, bottom))
            segment = long_image.crop((0, y, long_image.width, y + height))
            difference = ImageChops.difference(source, segment).getbbox()
            assert difference is None, f"long PNG segment for {file} differs from the page PNG at {difference}"
            segments.append({"page": file, "pagePixels": list(image.size), "sourceRows": [top, bottom], "longPngRows": [y, y + height]})
            y += height
        assert y == long_image.height, "long PNG contains extra or missing rows"
    return segments


def verify_case(directory, name, expected):
    words = {}
    body_columns = [[], [], []]
    with pdfplumber.open(directory / (name + ".pdf")) as document:
        assert len(document.pages) >= 3, "long-table fixture must exercise several page boundaries"
        pages = [read_page(page, expected, body_columns, words, name == "ordinary") for page in document.pages]
    for index, (actual, target) in enumerate(zip(body_columns, expected["body"]), 1):
        missing = list((Counter(target) - Counter(actual)).elements())
        assert not missing, f"column {index} lost body markers: {missing[:10]}"
        assert actual == target, f"column {index} body markers changed order"
    layout = verify_row_heights(name, words)
    segments = verify_pngs(directory, name, pages)
    return {
        "pages": [{key: value for key, value in page.items() if key != "words"} for page in pages],
        "bodyWords": len(words), "repeatedHeaders": len(pages), "rowGeometry": layout,
        "pngSegments": segments,
    }


def grid_strokes(page):
    assert not page.curves, "grid has an unexpected vector path"
    horizontal = defaultdict(list)
    vertical = defaultdict(list)
    edges = page.lines + [edge for edge in page.edges if edge["object_type"] == "rect_edge" and edge.get("stroke")]
    for edge in edges:
        x0, x1, top, bottom = (edge[key] for key in ("x0", "x1", "top", "bottom"))
        assert_close(edge["linewidth"], 1, "grid rule weight", tolerance=0.08)
        assert_rule_color(edge["stroking_color"])
        if abs(top - bottom) < 0.05:
            horizontal[round(top, 3)].append((x0, x1))
        else:
            assert abs(x0 - x1) < 0.05, "grid contains a diagonal rule"
            vertical[round(x0, 3)].append((top, bottom))
    return horizontal, vertical


def assert_rule_coverage(intervals, start, end, label, visible=True):
    overlaps = [(max(first, start), min(last, end)) for first, last in intervals if min(last, end) - max(first, start) > 0.05]
    if not visible:
        assert not overlaps, f"{label} was drawn inside the merged cell"
        return
    cursor = start
    for first, last in sorted(overlaps):
        assert first - cursor < 0.35, f"{label} is broken at {cursor:.2f} pt"
        cursor = max(cursor, last)
    assert end - cursor < 0.35, f"{label} ends early at {cursor:.2f} pt"


def verify_grid(directory):
    with pdfplumber.open(directory / "grid.pdf") as document:
        assert len(document.pages) == 1, "short grid should fit on one page"
        page = document.pages[0]
        horizontal, vertical = grid_strokes(page)
        ys, xs = sorted(horizontal), sorted(vertical)
        assert len(ys) == 5 and len(xs) == 4, "grid needs four column boundaries and five row boundaries"
        assert_close(xs[-1] - xs[0], TABLE_WIDTH, "grid outer width")
        assert_close(ys[1] - ys[0], REGULAR_ROW_HEIGHT, "grid header height")
        for x0, x1 in zip(xs, xs[1:]):
            assert_close(x1 - x0, TABLE_WIDTH / 3, "grid column width")
        for y in [ys[0], ys[1], ys[3], ys[4]]:
            assert_rule_coverage(horizontal[y], xs[0], xs[3], "horizontal outer or ordinary boundary")
        assert_rule_coverage(horizontal[ys[2]], xs[0], xs[2], "interior horizontal rule", visible=False)
        assert_rule_coverage(horizontal[ys[2]], xs[2], xs[3], "horizontal rule between neighboring cells")
        for x in [xs[0], xs[2], xs[3]]:
            assert_rule_coverage(vertical[x], ys[0], ys[4], "vertical outer or neighboring boundary")
        assert_rule_coverage(vertical[xs[1]], ys[1], ys[3], "interior vertical rule", visible=False)
        assert_rule_coverage(vertical[xs[1]], ys[0], ys[1], "vertical rule between header cells")
        assert_rule_coverage(vertical[xs[1]], ys[3], ys[4], "vertical rule between following cells")

        words = page.extract_words(x_tolerance=2.0, y_tolerance=2.0)
        cells = [([name], col, col + 1, 0, 1) for col, name in enumerate(["GRIDLEFT", "GRIDMID", "GRIDRIGHT"])]
        covered = ["GORIGIN", "GCOVERTOP", "GCOVERLOW", "GCOVEREND"]
        cells.append((covered, 0, 2, 1, 3))
        cells.extend([(["GNEIGHBORA"], 2, 3, 1, 2), (["GNEIGHBORB"], 2, 3, 2, 3)])
        cells.extend(([name], col, col + 1, 3, 4) for col, name in enumerate(["GLEFTEND", "GMIDEND", "GRIGHTEND"]))
        checked = []
        for expected, x0, x1, y0, y1 in cells:
            actual = [word["text"] for word in words
                      if xs[x0] < word["x0"] < word["x1"] < xs[x1] and ys[y0] < word["top"] < word["bottom"] < ys[y1]]
            assert actual == expected, f"grid cell contents changed or escaped: expected {expected}, found {actual}"
            checked.extend(actual)
        assert Counter(checked) == Counter(word["text"] for word in words), "grid has extra or overlapping text"
    return {"verticalBoundariesPt": xs, "horizontalBoundariesPt": ys, "coveredContents": covered, "words": len(checked)}


def main():
    directory = Path(sys.argv[1])
    report = {"cases": {}, "failures": []}
    for name, expected in EXPECTED.items():
        try:
            report["cases"][name] = verify_case(directory, name, expected)
        except (AssertionError, FileNotFoundError, ValueError, zipfile.BadZipFile) as error:
            report["failures"].append({"case": name, "error": str(error)})
    try:
        report["cases"]["grid"] = verify_grid(directory)
    except (AssertionError, FileNotFoundError, ValueError) as error:
        report["failures"].append({"case": "grid", "error": str(error)})
    report["passed"] = not report["failures"]
    print(json.dumps(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
