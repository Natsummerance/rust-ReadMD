"""Retained native and synthetic controls for the finite baseline PDF oracle.

No ReadMD, Typst, GUI or font installation is invoked. --artifacts NEW_DIR
preserves every generated PDF and the first failed control for independent QA.
"""
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zlib

spec = importlib.util.spec_from_file_location("typst_baseline_oracle", Path(__file__).with_name("inspect-typst-baseline.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)
ROOT = Path(__file__).resolve().parents[2]
FONT = ROOT / ".cache/capability-upgrade/batch3-native-defaults-baseline-20261024/calibration/raw-paint-01/fonts/DejaVuSans.ttf"
ARTIFACTS = None


def contract(rise=3, link=False):
    first = {"text": "SHIFTgjpy", "role": "ordinary", "rise": rise, "block": "SHIFT"}
    if link:
        first["link"] = "first"
    return {"schema": 1, "segments": [first, {"text": "CODEkeep", "role": "protected", "rise": 0, "block": "CODE"}],
            "links": {"first": {"uri": "https://example.invalid/repeated"}} if link else {}}


def rgb_png(width, height, samples):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    rows = b"".join(b"\x00" + samples[y * width * 3:(y + 1) * width * 3] for y in range(height))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


class BaselinePdfOracleTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(FONT.is_file(), "Existing readonly calibration font is required")
        if ARTIFACTS is None:
            temporary = tempfile.TemporaryDirectory(prefix="readmd-p04-pdf-controls-")
            self.addCleanup(temporary.cleanup)
            self.directory = Path(temporary.name)
        else:
            self.directory = ARTIFACTS / self._testMethodName
            self.directory.mkdir(exist_ok=False)

    def pdf(self, name, rise=0, link=False, bad=None, spaces=False, split=False):
        import pymupdf
        file = self.directory / name
        document = pymupdf.open()
        page = document.new_page(width=300, height=300)
        page.insert_font(fontname="QADejaVu", fontfile=str(FONT))
        text = "SHIFT gjpy" if spaces else "SHIFTgjpy"
        if bad == "missing":
            text = text[:-1]
        if bad == "visible-body":
            text = "CHANGEDgj"
        size, color = (13 if bad == "size" else 12), ((1, 0, 0) if bad == "color" else (0, 0, 0))
        if split:
            font = pymupdf.Font(fontfile=str(FONT))
            x = 40
            for char in text:
                page.insert_text((x, 108 - rise), char, fontname="QADejaVu", fontsize=size, color=color)
                # MuPDF's generated PDF /W entries quantize this font's
                # advance to integer thousand-em units. Match that exact PDF
                # metric when splitting BT runs, rather than TTF 2048/em.
                x += int(font.text_length(char, fontsize=1000)) * size / 1000
        else:
            page.insert_text((40, 108 - rise), text, fontname="QADejaVu", fontsize=size, color=color)
        code_rise = rise if bad == "protected-code" else 0
        page.insert_text((40, 170 - code_rise), "CODEkeep", fontname="QADejaVu", fontsize=12)
        if bad != "dropped-vector":
            y = 215 if bad == "changed-vector" else 210
            page.draw_line((40, y), (110, y), color=(0, 0, 0), width=1)
        if link:
            rect_rise = 0 if bad == "old-link-rect" else rise
            page.insert_link({"kind": pymupdf.LINK_URI, "from": pymupdf.Rect(39, 92 - rect_rise, 141, 113 - rect_rise),
                              "uri": "readmd-internal:publication/0123456789abcdef/probe" if bad == "private-link" else "https://example.invalid/repeated"})
        if bad == "extra-hidden":
            page.insert_text((40, 250), "HIDDEN_BODY", fontname="QADejaVu", fontsize=12, render_mode=3)
        if bad == "clip":
            member = page.get_contents()[0]
            document.update_stream(member, b"q\n0 0 1 1 re W n\n" + document.xref_stream(member) + b"\nQ\n")
        document.save(file, deflate=True)
        document.close()
        return file

    def pair(self, rise=3, **kwargs):
        return self.pdf("control.pdf", **kwargs), self.pdf("actual.pdf", rise, **kwargs)

    def test_01_real_positive_negative_fractional_baselines_keep_every_glyph_and_path(self):
        for index, rise in enumerate((0, 3, -2.5, 3.25)):
            before = self.pdf(f"control-{index}.pdf")
            after = self.pdf(f"actual-{index}.pdf", rise)
            result = qa.compare(before, after, contract(rise))
            self.assertTrue(result["passed"] and result["raster"]["exactAllPixels"])
            self.assertEqual(result["allGlyphs"], 17)
            self.assertEqual(result["protectedVisibleGlyphs"], 8)
            self.assertEqual(result["allDrawingPaths"], 1)
            self.assertFalse(result["applicationOrFullP04Certified"])

    def test_02_trace_grouping_does_not_determine_glyph_identity(self):
        before = self.pdf("control.pdf")
        after = self.pdf("split-traces.pdf", 3, split=True)
        self.assertTrue(qa.compare(before, after, contract())["passed"])

    def test_03_actual_same_context_comparison_is_full_paint_identity(self):
        before = self.pdf("already-shifted.pdf", 3)
        after = self.pdf("same.pdf", 3)
        result = qa.compare(before, after, contract(), same_paint=True, png_directory=self.directory / "png")
        self.assertEqual(result["movedVisibleGlyphs"], 0)
        self.assertEqual(len(result["raster"]["pngs"]), 2)
        self.assertTrue(all(Path(row["path"]).read_bytes().startswith(b"\x89PNG") for row in result["raster"]["pngs"]))

    def test_04_empty_outline_whitespace_is_counted_but_not_ink(self):
        before, after = self.pair(spaces=True)
        expected = contract()
        expected["segments"][0]["text"] = "SHIFT gjpy"
        result = qa.compare(before, after, expected)
        self.assertEqual(result["allGlyphs"], 18)
        self.assertEqual(result["emptyOutlineWhitespace"], 1)
        self.assertTrue(result["emptyWhitespaceRiseNotCertified"])

    def test_05_actual_link_rect_follows_its_entire_shifted_visible_text(self):
        before, after = self.pair(link=True)
        result = qa.compare(before, after, contract(link=True))
        self.assertEqual(result["realLinkFragments"], 1)

    def test_06_word_moved_but_original_link_rect_is_rejected(self):
        before = self.pdf("control.pdf", link=True)
        after = self.pdf("old-rect.pdf", 3, link=True, bad="old-link-rect")
        with self.assertRaisesRegex(AssertionError, "Rect-only shift"):
            qa.compare(before, after, contract(link=True))

    def test_07_wrong_rise_is_not_accepted_as_matching_text(self):
        before, after = self.pair(rise=2.9)
        with self.assertRaisesRegex(AssertionError, "baseline"):
            qa.compare(before, after, contract())

    def test_08_shifted_code_is_a_failure_even_when_ordinary_glyphs_are_correct(self):
        before = self.pdf("control.pdf")
        after = self.pdf("bad-code.pdf", 3, bad="protected-code")
        with self.assertRaisesRegex(AssertionError, "baseline"):
            qa.compare(before, after, contract())

    def test_09_visible_size_color_and_content_changes_cannot_hide_in_trace_groups(self):
        before = self.pdf("control.pdf")
        for name in ("size", "color", "missing", "visible-body"):
            with self.subTest(case=name), self.assertRaises(AssertionError):
                qa.compare(before, self.pdf(name + ".pdf", 3, bad=name), contract())

    def test_10_full_vector_inventory_rejects_changed_or_dropped_paths(self):
        before = self.pdf("control.pdf")
        for name in ("changed-vector", "dropped-vector"):
            with self.subTest(case=name), self.assertRaisesRegex(AssertionError, "vector drawing"):
                qa.compare(before, self.pdf(name + ".pdf", 3, bad=name), contract())

    def test_11_same_text_and_origins_do_not_qualify_clipped_glyph_paint(self):
        before = self.pdf("control.pdf")
        after = self.pdf("clipped.pdf", 3, bad="clip")
        with self.assertRaises(AssertionError):
            qa.compare(before, after, contract())

    def test_12_unknown_invisible_body_text_never_gets_a_math_copy_exception(self):
        before = self.pdf("control.pdf")
        after = self.pdf("hidden.pdf", 3, bad="extra-hidden")
        with self.assertRaisesRegex(AssertionError, "invisible"):
            qa.compare(before, after, contract())

    def test_13_paper_edges_and_missing_pages_are_not_logical_bbox_claims(self):
        before = self.pdf("control.pdf")
        after = self.pdf("outside-paper.pdf", 105)
        with self.assertRaises(AssertionError):
            qa.compare(before, after, contract())
        import pymupdf
        two = self.directory / "extra-page.pdf"
        with pymupdf.open(before) as document:
            page = document.new_page(width=300, height=300)
            page.insert_font(fontname="QADejaVu", fontfile=str(FONT))
            page.insert_text((40, 100), "EXTRA_PAGE", fontname="QADejaVu", fontsize=12)
            document.save(two, deflate=True)
        with self.assertRaises(AssertionError):
            qa.compare(before, two, contract())

    def test_14_private_uri_is_rejected_before_any_glyph_success(self):
        file = self.pdf("private.pdf", link=True, bad="private-link")
        with self.assertRaisesRegex(AssertionError, "Private publication mapping survived"):
            qa.clean_pdf(file)

    def test_15_orphan_packed_utf16_action_is_scanned(self):
        import pymupdf
        from pypdf import PdfReader
        source = self.pdf("source.pdf")
        file = self.directory / "orphan-packed.pdf"
        with pymupdf.open(source) as document:
            xref = document.get_new_xref()
            value = "readmd-internal:publication/0123456789abcdef/orphan".encode("utf-16-be").hex()
            document.update_object(xref, "<< /Type /Action /S /URI /URI <feff" + value + "> >>")
            document.save(file, garbage=0, deflate=True, use_objstms=1)
        self.assertIn(xref, PdfReader(file).xref_objStm, "The negative control must really be an orphan compressed object")
        with self.assertRaisesRegex(AssertionError, "Private publication mapping survived"):
            qa.clean_pdf(file)

    def test_16_an_em_label_does_not_certify_a_real_italic_font(self):
        before, after = self.pair()
        declared = contract()
        declared["requirePhysicalItalic"] = True
        declared["segments"][0]["role"] = "italic"
        with self.assertRaisesRegex(AssertionError, "actual italic"):
            qa.compare(before, after, declared)

    def test_17_matching_control_and_actual_cannot_both_ignore_the_requested_author_size(self):
        before, after = self.pair()
        declared = contract()
        declared["segments"][0]["expectedSize"] = 17
        with self.assertRaisesRegex(AssertionError, "Requested author font size"):
            qa.compare(before, after, declared)

    def change_links(self, source, name, variant):
        import pymupdf
        output = self.directory / name
        with pymupdf.open(source) as document:
            page = document[0]
            original = page.get_links()[0]
            rect = list(original["from"])
            link = {"kind": pymupdf.LINK_URI, "uri": original["uri"], "from": pymupdf.Rect(rect)}
            if variant in ("duplicate", "overlap"):
                if variant == "overlap":
                    rect[0] += .25
                    rect[2] += .25
                link["from"] = pymupdf.Rect(rect)
                page.insert_link(link)
            else:
                page.delete_link(original)
                if variant in ("nonlink", "partial-nonlink"):
                    rect[3] = 176 if variant == "nonlink" else 163
                    link["from"] = pymupdf.Rect(rect)
                    page.insert_link(link)
                elif variant == "missing":
                    rect[2] = 85
                    link["from"] = pymupdf.Rect(rect)
                    page.insert_link(link)
                else:
                    assert variant in ("split", "split-reversed")
                    glyphs = [row for row in qa.inventory(document)["glyphs"] if row["origin"][1] < 140]
                    split = (glyphs[3]["ink"][2] + glyphs[4]["ink"][0]) / 2
                    first, second = rect[:], rect[:]
                    first[2], second[0] = split, split
                    for part in ((second, first) if variant == "split-reversed" else (first, second)):
                        page.insert_link({**link, "from": pymupdf.Rect(part)})
            document.save(output, deflate=True)
        return output

    def test_18_duplicate_and_overlapping_fragments_are_rejected_even_in_both_pdfs(self):
        before, after = self.pair(link=True)
        for variant in ("duplicate", "overlap"):
            with self.subTest(variant=variant):
                old = self.change_links(before, variant + "-before.pdf", variant)
                new = self.change_links(after, variant + "-after.pdf", variant)
                with self.assertRaisesRegex(AssertionError, "Duplicate/overlapping"):
                    qa.compare(old, new, contract(link=True))

    def test_19_link_rect_must_not_cover_even_partial_nonlink_neighbor_ink(self):
        before, after = self.pair(link=True)
        for variant in ("nonlink", "partial-nonlink"):
            with self.subTest(variant=variant):
                old = self.change_links(before, variant + "-before.pdf", variant)
                new = self.change_links(after, variant + "-after.pdf", variant)
                with self.assertRaisesRegex(AssertionError, "non-link visible ink"):
                    qa.compare(old, new, contract(link=True))

    def test_20_legal_multifragment_ownership_is_independent_of_annotation_order(self):
        before, after = self.pair(link=True)
        old = self.change_links(before, "split-before.pdf", "split")
        new = self.change_links(after, "split-after.pdf", "split-reversed")
        proof = qa.compare(old, new, contract(link=True))
        self.assertEqual(proof["realLinkFragments"], 2)
        self.assertTrue(proof["everyVisibleLinkedGlyphExactlyOneFragment"])

    def test_21_unannotated_link_glyph_cannot_be_hidden_by_partial_union_coverage(self):
        before, after = self.pair(link=True)
        old = self.change_links(before, "missing-before.pdf", "missing")
        new = self.change_links(after, "missing-after.pdf", "missing")
        with self.assertRaises(AssertionError):
            qa.compare(old, new, contract(link=True))

    def preview(self, pdf, name, page=1):
        import pymupdf
        file = self.directory / name
        with pymupdf.open(pdf) as document:
            document[page - 1].get_pixmap(matrix=pymupdf.Matrix(2, 2), alpha=False).save(file)
        return file

    def test_22_visible_preview_is_bound_to_the_actual_pdf_and_page(self):
        import pymupdf
        source = self.pdf("current.pdf", 3)
        second = self.directory / "two-pages.pdf"
        with pymupdf.open(source) as document:
            page = document.new_page(width=300, height=300)
            page.insert_font(fontname="QADejaVu", fontfile=str(FONT))
            page.insert_text((75, 180), "DIFFERENT_PAGE", fontname="QADejaVu", fontsize=17)
            document.save(second, deflate=True)
        first_png, second_png = self.preview(second, "first.png"), self.preview(second, "second.png", 2)
        for number, png in ((1, first_png), (2, second_png)):
            proof = qa.compare_preview(second, png, number)
            self.assertTrue(proof["passed"] and proof["everyPixelCheckedBothDirections"])
            self.assertEqual(proof["pdfSha256"], hashlib.sha256(second.read_bytes()).hexdigest())
            self.assertEqual(proof["pngSha256"], hashlib.sha256(png.read_bytes()).hexdigest())
        with self.assertRaisesRegex(AssertionError, "Visible preview ink"):
            qa.compare_preview(second, first_png, 2)
        with self.assertRaises(AssertionError):
            qa.compare_preview(second, second_png, 3)

    def test_23_blank_stale_missing_glyph_and_wrong_color_pngs_are_real_failures(self):
        import pymupdf
        before, after = self.pair()
        stale, current = self.preview(before, "stale.png"), self.preview(after, "current.png")
        with self.assertRaisesRegex(AssertionError, "Visible preview ink"):
            qa.compare_preview(after, stale, 1)
        size, original = qa.png_rgb(current.read_bytes())
        width, height = size
        blank = self.directory / "blank.png"
        blank.write_bytes(rgb_png(width, height, b"\xff" * (width * height * 3)))
        with self.assertRaisesRegex(AssertionError, "Blank preview"):
            qa.compare_preview(after, blank, 1)
        with pymupdf.open(after) as document:
            first = qa.inventory(document)["glyphs"][0]["ink"]
        missing = bytearray(original)
        for y in range(math.floor(first[1] * 2) - 1, math.ceil(first[3] * 2) + 1):
            a, b = (y * width + math.floor(first[0] * 2) - 1) * 3, (y * width + math.ceil(first[2] * 2) + 1) * 3
            missing[a:b] = b"\xff" * (b - a)
        absent = self.directory / "missing-one-glyph.png"
        absent.write_bytes(rgb_png(width, height, missing))
        with self.assertRaisesRegex(AssertionError, "Visible preview ink"):
            qa.compare_preview(after, absent, 1)
        colored = bytearray(original)
        for at in range(0, len(colored), 3):
            if min(colored[at:at + 3]) < 224:
                colored[at + 2] = 255
        wrong = self.directory / "wrong-color.png"
        wrong.write_bytes(rgb_png(width, height, colored))
        with self.assertRaisesRegex(AssertionError, "color/coverage"):
            qa.compare_preview(after, wrong, 1)

    def test_24_png_integrity_size_and_fixed_local_aa_envelope_are_checked(self):
        source = self.pdf("source.pdf")
        png = self.preview(source, "valid.png")
        raw = png.read_bytes()
        for changed in (raw[:-3], raw + b"trailing", raw[:30] + bytes([raw[30] ^ 1]) + raw[31:]):
            with self.assertRaises(AssertionError):
                qa.png_rgb(changed)
        width = height = 20
        old = bytearray(b"\xff" * (width * height * 3))
        for y in range(7, 13):
            for x in range(7, 13):
                old[(y * width + x) * 3:(y * width + x + 1) * 3] = b"\x00\x00\x00"
        aa = bytearray(old)
        aa[(8 * width + 6) * 3:(8 * width + 7) * 3] = b"\xee\xee\xee"
        self.assertTrue(qa.preview_rgb_compare(bytes(old), bytes(aa), width, height)["everyPixelCheckedBothDirections"])
        aa[(2 * width + 2) * 3:(2 * width + 3) * 3] = b"\x00\x00\x00"
        with self.assertRaisesRegex(AssertionError, "Visible preview ink"):
            qa.preview_rgb_compare(bytes(old), bytes(aa), width, height)

    def metadata_pdf(self, source, name, creator="", producer=""):
        import pymupdf
        output = self.directory / name
        with pymupdf.open(source) as document:
            document.set_metadata({**document.metadata, "creator": creator, "producer": producer})
            document.save(output, deflate=True)
        return output

    def test_25_current_typst_creator_and_historical_producer_are_real_pdf_metadata(self):
        before, after = self.pair()
        declared = {**contract(), "requireTypstProducer": True}
        for index, (creator, producer, field) in enumerate((("Typst 0.15.1", "", "creator"),
                                                           ("", "Typst 0.13.1", "producer"),
                                                           ("", "Typst", "producer"))):
            with self.subTest(creator=creator, producer=producer):
                old = self.metadata_pdf(before, f"metadata-control-{index}.pdf", creator, producer)
                new = self.metadata_pdf(after, f"metadata-actual-{index}.pdf", creator, producer)
                proof = qa.compare(old, new, declared)
                self.assertTrue(proof["passed"])
                self.assertEqual(proof["typstEngineMetadata"]["control"],
                                 {"creator": creator, "producer": producer, "matchedField": field})
                self.assertEqual(proof["typstEngineMetadata"]["actual"], proof["typstEngineMetadata"]["control"])

    def test_26_missing_wrong_engine_wrong_creator_version_and_substring_spoofs_are_rejected(self):
        before, after = self.pair()
        declared = {**contract(), "requireTypstProducer": True}
        cases = (("", ""), ("pdfTeX", "pdfTeX"), ("Typst 0.15.0", ""),
                 ("ReadMD Typst 0.15.1", ""), ("", "NotTypst"),
                 ("", "XeLaTeX (Typst compatible)"), ("", "Typst helper"))
        for index, (creator, producer) in enumerate(cases):
            with self.subTest(creator=creator, producer=producer):
                old = self.metadata_pdf(before, f"bad-metadata-control-{index}.pdf", creator, producer)
                new = self.metadata_pdf(after, f"bad-metadata-actual-{index}.pdf", creator, producer)
                with self.assertRaisesRegex(AssertionError, "Missing/wrong Typst engine metadata"):
                    qa.compare(old, new, declared)

    def winrt_fixture(self):
        pdf = ROOT / "tests/fixtures/typst-baseline-winrt-native.pdf"
        png = ROOT / "tests/fixtures/typst-baseline-winrt-native.png"
        self.assertEqual(hashlib.sha256(pdf.read_bytes()).hexdigest(), "71bd9b671c322f4d245061b238bedc2db507e55647a3ccaabf75247fdfe02028")
        self.assertEqual(hashlib.sha256(png.read_bytes()).hexdigest(), "c780cb71f0f51b7dbf4380bd958514c346c462a65fdef483af2f60e49ede83f7")
        return pdf, png

    def test_27_original_native_winrt_hinting_is_bounded_in_both_directions(self):
        pdf, png = self.winrt_fixture()
        proof = qa.compare_preview(pdf, png, 1)
        self.assertEqual(proof["hintingStrongPixels"], [1, 2])
        self.assertTrue(proof["hintingBudgetCountsGeometryAndRgb"])
        self.assertEqual(proof["unmatchedStrongPixels"], [0, 0])
        self.assertEqual(proof["unmatchedFringePixels"], [0, 0])
        self.assertEqual(proof["aa"]["coreRgbDelta"], 64)
        self.assertEqual(proof["aa"]["edgeRgbDelta"], 128)

    def test_28_hinting_quota_is_not_a_global_shift_or_missing_ink_allowance(self):
        width, height = 320, 160
        base = bytearray(b"\xff" * (width * height * 3))
        for y in range(20, 100):
            for x in range(20, 120):
                base[(y * width + x) * 3:(y * width + x + 1) * 3] = b"\x26" * 3
        def dots(count, offset):
            result = bytearray(base)
            for index in range(count):
                x, y = 160 + index * 8 + offset, 140
                result[(y * width + x) * 3:(y * width + x + 1) * 3] = b"\x33" * 3
            return bytes(result)
        proof = qa.preview_rgb_compare(dots(1, 0), dots(1, 2), width, height)
        self.assertEqual(proof["hintingStrongPixels"], [1, 1])
        with self.assertRaisesRegex(AssertionError, "hinting budget"):
            qa.preview_rgb_compare(dots(10, 0), dots(10, 2), width, height)
        with self.assertRaisesRegex(AssertionError, "Visible preview ink"):
            qa.preview_rgb_compare(dots(1, 0), bytes(base), width, height)
        line = bytearray(base)
        for x in range(160, 260):
            line[(120 * width + x) * 3:(120 * width + x + 1) * 3] = b"\x26" * 3
        with self.assertRaisesRegex(AssertionError, "Visible preview ink"):
            qa.preview_rgb_compare(bytes(line), bytes(base), width, height)

    def test_29_real_png_shift_missing_glyph_descender_dot_brightness_and_color_fail(self):
        import pymupdf
        pdf, png = self.winrt_fixture()
        (width, height), original = qa.png_rgb(png.read_bytes())
        def rejected(label, raw):
            changed = self.directory / (label + ".png")
            changed.write_bytes(rgb_png(width, height, raw))
            with self.subTest(label=label), self.assertRaises(AssertionError):
                qa.compare_preview(pdf, changed, 1)
        for dx, dy in ((-2, 0), (2, 0), (0, -2), (0, 2)):
            moved = bytearray(b"\xff" * len(original))
            for y in range(height):
                yy = y + dy
                if 0 <= yy < height:
                    start, stop = max(0, -dx), min(width, width - dx)
                    moved[(yy * width + start + dx) * 3:(yy * width + stop + dx) * 3] = original[(y * width + start) * 3:(y * width + stop) * 3]
            rejected(f"shift-{dx}-{dy}", moved)
        with pymupdf.open(pdf) as document:
            frame = document[0].rect
            glyphs = qa.inventory(document)["glyphs"]
        sx, sy = width / frame.width, height / frame.height
        first = glyphs[0]
        code_g = next(row for row in glyphs if row["unicode"] == ord("g") and "mono" in qa.plain_font(row["font"]))
        dot = next(row for row in reversed(glyphs) if row["unicode"] == ord("."))
        for label, glyph, descender in (("missing-heading-glyph", first, False), ("missing-code-descender", code_g, True), ("missing-dot", dot, False)):
            missing = bytearray(original)
            x0, y0, x1, y1 = glyph["ink"]
            if descender:
                self.assertGreater(y1, glyph["origin"][1])
                y0 = glyph["origin"][1] + 0.3
            for y in range(max(0, math.floor(y0 * sy)), min(height, math.ceil(y1 * sy) + 1)):
                a, b = max(0, math.floor(x0 * sx)), min(width, math.ceil(x1 * sx) + 1)
                missing[(y * width + a) * 3:(y * width + b) * 3] = b"\xff" * ((b - a) * 3)
            rejected(label, missing)
        bright = bytes(min(255, channel + 100) for channel in original)
        rejected("brightened-all", bright)
        colored = bytearray(original)
        for at in range(0, len(colored), 3):
            if min(colored[at:at + 3]) < 224:
                colored[at + 2] = 255
        rejected("wrong-color-all", colored)


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    if "--artifacts" in sys.argv:
        index = sys.argv.index("--artifacts")
        ARTIFACTS = Path(sys.argv[index + 1]).resolve()
        del sys.argv[index:index + 2]
        ARTIFACTS.mkdir(exist_ok=False)
    program = unittest.main(verbosity=2, failfast=True, exit=False)
    result = program.result
    if ARTIFACTS is not None:
        manifest = [{"path": str(file.relative_to(ARTIFACTS)).replace("\\", "/"), "bytes": file.stat().st_size,
                     "sha256": hashlib.sha256(file.read_bytes()).hexdigest()} for file in sorted(ARTIFACTS.rglob("*")) if file.is_file()]
        report = {"passed": result.wasSuccessful(), "testsRun": result.testsRun, "failures": len(result.failures),
                  "errors": len(result.errors), "syntheticPdfControlsOnly": False, "pureOracleControlsOnly": True,
                  "retainedNativeWinrtFixture": True, "productOrEngineInvoked": False, "artifacts": manifest}
        with (ARTIFACTS / "report.json").open("x", encoding="utf-8") as stream:
            json.dump(report, stream, ensure_ascii=True, indent=2)
    sys.exit(0 if result.wasSuccessful() else 1)
