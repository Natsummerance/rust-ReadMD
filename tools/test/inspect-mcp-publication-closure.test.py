"""Independent controls for named PDF GoTo proof; no product or MCP process.

Run with the existing reviewed QA Python/PyMuPDF. PDFs stay in memory.
"""
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

import pymupdf as fitz

spec = importlib.util.spec_from_file_location(
    "publication_closure_inspector", Path(__file__).with_name("inspect-mcp-publication-closure.py"))
inspector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inspector)


def fixture(*, name="target", source_name="target", point="60 752 null",
            numeric_page=False, bad_page=False, dictionary=False, action="GoTo"):
    document = fitz.open()
    document.new_page(width=595, height=842)
    document.new_page(width=595, height=842)
    target = document.page_xref(1)
    if bad_page:
        target = document.get_new_xref()
        document.update_object(target, "<<>>")
    reference = "1" if numeric_page else str(target) + " 0 R"
    array = "[" + reference + " /XYZ " + point + "]"
    if dictionary:
        value = document.get_new_xref()
        document.update_object(value, "<< /D " + array + " >>")
        array = str(value) + " 0 R"
    names = document.get_new_xref()
    document.update_object(names, "<< /Names [(" + name + ") " + array + "] >>")
    document.xref_set_key(document.pdf_catalog(), "Names", "<< /Dests " + str(names) + " 0 R >>")
    annotation = document.get_new_xref()
    document.update_object(annotation, "<< /Type /Annot /Subtype /Link /Rect [10 20 50 40]"
                           " /A << /S /" + action + " /D (" + source_name + ") >> >>")
    document.xref_set_key(document.page_xref(0), "Annots", "[" + str(annotation) + " 0 R]")
    link = {"kind": fitz.LINK_NAMED, "xref": annotation, "nameddest": source_name,
            "page": 1, "from": fitz.Rect(10, 802, 50, 822), "to": fitz.Point(60, 752)}
    return document, link


class PdfNamedGoToControls(unittest.TestCase):
    def record(self, document, link):
        return inspector.pdf_link_record(document, link, fitz)

    def test_real_names_array_preserves_evidence_and_transforms_pdf_point(self):
        document, _ = fixture()
        with document:
            link = document[0].get_links()[0]
            self.assertEqual(link["kind"], fitz.LINK_NAMED)
            record = self.record(document, link)
            self.assertEqual(record["kind"], fitz.LINK_GOTO)
            self.assertEqual(record["rawKind"], fitz.LINK_NAMED)
            self.assertEqual(record["rawTo"], list(link["to"]))
            self.assertEqual(record["nameddest"], "target")
            self.assertEqual(record["page"], 1)
            self.assertEqual(record["to"], [60, 90])
            basis = record["namedResolution"]
            self.assertEqual(basis["sourceXref"], link["xref"])
            self.assertEqual(basis["sourceKey"], "A/D")
            self.assertEqual(basis["pageXref"], document.page_xref(1))
            self.assertEqual(basis["pdfTo"], [60, 752])
            self.assertEqual(basis["pdfZoom"], None)
            self.assertEqual(basis["coordinateSpace"], "unrotated-page")
            json.dumps(record, allow_nan=False)

    def test_destination_dictionary_is_resolved_without_inventing_a_target(self):
        document, link = fixture(dictionary=True)
        with document:
            self.assertEqual(self.record(document, link)["to"], [60, 90])

    def test_direct_named_destination_is_verified_against_the_annotation(self):
        document, link = fixture()
        document.xref_set_key(link["xref"], "A", "null")
        document.xref_set_key(link["xref"], "Dest", "(target)")
        with document:
            record = self.record(document, link)
            self.assertEqual(record["kind"], fitz.LINK_GOTO)
            self.assertEqual(record["to"], [60, 90])
            self.assertEqual(record["namedResolution"]["sourceKey"], "Dest")

    def test_unknown_name_is_rejected(self):
        document, link = fixture(source_name="missing")
        with document, self.assertRaisesRegex(AssertionError, "Unresolved PDF named GoTo"):
            self.record(document, link)

    def test_extracted_name_must_match_the_actual_annotation(self):
        document, link = fixture()
        link["nameddest"] = "different"
        with document, self.assertRaisesRegex(AssertionError, "source name mismatch"):
            self.record(document, link)

    def test_nonpage_target_is_rejected(self):
        document, link = fixture(bad_page=True)
        with document, self.assertRaisesRegex(AssertionError, "Invalid PDF named GoTo page"):
            self.record(document, link)

    def test_local_destination_requires_a_real_page_reference(self):
        document, link = fixture(numeric_page=True)
        with document, self.assertRaisesRegex(AssertionError, "internal page reference"):
            self.record(document, link)

    def test_resolved_page_must_agree_with_the_actual_destination_array(self):
        document, link = fixture()
        document.resolve_names()["target"]["page"] = 0
        with document, self.assertRaisesRegex(AssertionError, "page reference mismatch"):
            self.record(document, link)

    def test_arbitrary_kind_four_and_named_viewer_actions_are_not_accepted(self):
        document, link = fixture(action="Named")
        with document:
            record = self.record(document, link)
            self.assertEqual(record["kind"], fitz.LINK_NAMED)
            self.assertNotIn("namedResolution", record)
            self.assertNotIn("rawKind", record)
        document, link = fixture()
        del link["xref"]
        with document:
            self.assertEqual(self.record(document, link)["kind"], fitz.LINK_NAMED)

    def test_named_xyz_null_is_not_changed_to_the_resolvers_zero(self):
        document, _ = fixture(point="null 752 null")
        with document:
            link = document[0].get_links()[0]
            self.assertEqual(document.resolve_names()["target"]["to"][0], 0)
            record = self.record(document, link)
            self.assertEqual(record["rawTo"], [0, 752])
            self.assertEqual(record["to"], [None, 90])
            self.assertEqual(record["toDefaultCoordinates"], [0])
            self.assertEqual(record["namedResolution"]["pdfTo"], [None, 752])
            self.assertEqual(json.loads(json.dumps(record, allow_nan=False))["to"], [None, 90])

    def test_page_transformation_includes_the_target_crop_offset(self):
        document, link = fixture()
        document[1].set_cropbox(fitz.Rect(20, 30, 575, 812))
        with document:
            record = self.record(document, link)
            expected = fitz.Point(60, 752) * document[1].transformation_matrix
            self.assertEqual(record["to"], list(expected))
            self.assertEqual(record["to"], [40, 60])

    def test_direct_xyz_null_stays_json_null(self):
        document, link = fixture()
        link.update(kind=fitz.LINK_GOTO, to=fitz.Point(math.nan, 90))
        with document:
            record = self.record(document, link)
            self.assertEqual(record["to"], [None, 90])
            self.assertEqual(record["toDefaultCoordinates"], [0])
            self.assertNotIn("rawKind", record)
            self.assertNotIn("NaN", json.dumps(record, allow_nan=False))

    def test_infinite_destination_is_rejected(self):
        document, link = fixture()
        link.update(kind=fitz.LINK_GOTO, to=fitz.Point(math.inf, 90))
        with document, self.assertRaisesRegex(AssertionError, "Infinite PDF destination"):
            self.record(document, link)

    def test_nonfinite_hit_rectangle_is_rejected(self):
        for value in (math.nan, math.inf):
            with self.subTest(value=value):
                document, link = fixture()
                link["from"] = fitz.Rect(value, 802, 50, 822)
                with document, self.assertRaisesRegex(AssertionError, "Nonfinite link hit rectangle"):
                    self.record(document, link)


class PdfCharacterBoundsControls(unittest.TestCase):
    def test_opt_in_returns_actual_character_boxes_and_default_stays_compact(self):
        with tempfile.TemporaryDirectory(prefix="readmd-character-control-") as directory:
            file = Path(directory) / "actual-character-control.pdf"
            with fitz.open() as document:
                page = document.new_page(width=595, height=842)
                page.insert_text((45, 90), "IV.AA TEXHEAD02", fontsize=12)
                document.save(file)
            plain = inspector.inspect_pdf(file)
            self.assertNotIn("characters", plain["pages"][0])
            result = inspector.inspect_pdf(file, character_bounds=True)
            characters = result["pages"][0]["characters"]
            self.assertEqual("".join(character["text"] for character in characters), "IV.AA TEXHEAD02")
            with fitz.open(file) as document:
                actual = document[0].get_text("rawdict")["blocks"][0]["lines"][0]["spans"][0]["chars"]
            self.assertEqual([character["bounds"] for character in characters], [list(character["bbox"]) for character in actual])
            self.assertEqual([character["origin"] for character in characters], [list(character["origin"]) for character in actual])
            self.assertTrue(all(character["font"] == "Helvetica" and character["size"] == 12 for character in characters))
            json.dumps(result, allow_nan=False)

    def test_nonfinite_or_reversed_character_geometry_is_rejected(self):
        class RawPage:
            def get_text(self, mode):
                self.mode = mode
                return {"blocks": [{"lines": [{"spans": [{"font": "QA", "size": 10, "chars": [self.character]}]}]}]}
        for key, value, message in (("bbox", [0, 1, math.nan, 3], "Nonfinite PDF character bounds"),
                                    ("bbox", [4, 1, 0, 3], "Reversed PDF character bounds"),
                                    ("origin", [0, math.inf], "Nonfinite PDF character origin")):
            with self.subTest(key=key, value=value):
                page = RawPage()
                page.character = {"c": "X", "bbox": [0, 1, 2, 3], "origin": [0, 2]}
                page.character[key] = value
                with self.assertRaisesRegex(AssertionError, message):
                    inspector.pdf_character_bounds(page)


if __name__ == "__main__":
    unittest.main()
