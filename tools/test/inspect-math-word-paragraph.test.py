"""Pure OPC/oracle controls only; no product or engine acceptance claims."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("word_paragraph_qa", Path(__file__).with_name("inspect-math-word-paragraph.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)
ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / ".cache/capability-upgrade/batch3-m07-paragraph-20261023/qa"


def inspected(kind):
    return qa.inspect_package(qa.opc.Package(parts=qa.fixture_parts(kind)))


def document_change(kind, edit):
    parts = qa.fixture_parts(kind)
    package = qa.opc.Package(parts=parts)
    root = package.xml(package.document)
    edit(root)
    parts[package.document] = qa.opc.xml_bytes(root)
    return qa.opc.Package(parts=parts)


class WordParagraphControls(unittest.TestCase):
    def test_three_native_only_fixtures_keep_three_identical_real_formulas(self):
        for kind in qa.KINDS:
            with self.subTest(kind=kind):
                value = inspected(kind)
                self.assertEqual(len(value["paragraphs"]), 3)
                self.assertEqual(value["globalMathPrCount"], 1)
                self.assertEqual(value["auxiliaryParts"], {})
                self.assertFalse(value["sourcePayloadUsedAsNativeEvidence"])
                self.assertTrue(all(row["mathTexts"] == ["ab"] for row in value["paragraphs"]))
                self.assertEqual(value["paragraphs"][0]["mathObjects"], value["paragraphs"][2]["mathObjects"])
                for row in value["paragraphs"]:
                    self.assertEqual(len(row["runDeclarations"]), 2)
                    for run in row["runDeclarations"]:
                        self.assertEqual(run["sizeHalfPoints"], "24")
                        self.assertEqual(run["complexSizeHalfPoints"], "24")
                        self.assertEqual(set(run["fonts"].values()), {"STIX Two Math"})

    def test_direct_without_styles_does_not_drop_context(self):
        value = inspected("direct-no-styles")
        self.assertIsNone(value["stylesPart"])
        for row in value["paragraphs"]:
            self.assertEqual(row["effective"]["axes"], {"left": "720", "right": "180", "jc": "right"})
            self.assertEqual(set(row["effective"]["provenance"].values()), {"direct"})

    def test_based_on_independent_axes_uses_selected_chain_not_unrelated_default(self):
        value = inspected("styles-chain")
        self.assertEqual(value["stylesPart"], "word/style-parts/current.xml")
        for row in value["paragraphs"]:
            self.assertEqual(row["effective"]["axes"], {"left": "0", "right": "360", "jc": "center"})
            self.assertEqual(row["effective"]["styleChain"], ["Base", "Equation"])
            self.assertEqual(row["effective"]["provenance"], {"left": "direct", "right": "style:Equation", "jc": "style:Equation"})
        self.assertIn(b"<v:ind", qa.fixture_parts("styles-chain")["word/document.xml"])

    def test_empty_missing_and_explicit_zero_are_distinct(self):
        rows = inspected("empty-zero")["paragraphs"]
        self.assertTrue(rows[0]["direct"]["pPrPresent"])
        self.assertFalse(rows[0]["direct"]["indPresent"])
        self.assertEqual(rows[0]["effective"]["axes"], {})
        self.assertEqual(rows[1]["effective"]["axes"], {"left": "0", "right": "0", "jc": "left"})
        self.assertTrue(rows[2]["direct"]["indPresent"])
        self.assertEqual(rows[2]["effective"]["axes"], {})

    def test_current_second_axis_change_and_clear_leave_every_other_part_and_formula(self):
        parts = qa.fixture_parts("styles-chain")
        parts["customXml/readmd-unmodified.xml"] = b"<old>left=999; not native authority</old>"
        changed, detail = qa.mutate(parts, "left", "720")
        self.assertTrue(detail["onlySecondParagraphChanged"])
        rows = qa.inspect_package(qa.opc.Package(parts=changed))["paragraphs"]
        self.assertEqual([row["effective"]["axes"]["left"] for row in rows], ["0", "720", "0"])
        cleared, detail = qa.mutate(changed, "left", None)
        rows = qa.inspect_package(qa.opc.Package(parts=cleared))["paragraphs"]
        self.assertEqual([row["effective"]["axes"]["left"] for row in rows], ["0", "480", "0"])
        self.assertEqual(rows[1]["effective"]["axes"]["right"], "360")
        self.assertTrue(rows[1]["direct"]["indPresent"])
        self.assertEqual(cleared["customXml/readmd-unmodified.xml"], parts["customXml/readmd-unmodified.xml"])

    def test_native_jc_clear_recovers_style_and_does_not_set_a_default(self):
        changed, _ = qa.mutate(qa.fixture_parts("styles-chain"), "jc", "center")
        cleared, _ = qa.mutate(changed, "jc", None)
        row = qa.inspect_package(qa.opc.Package(parts=cleared))["paragraphs"][1]
        self.assertFalse(row["direct"]["jcPresent"])
        self.assertEqual(row["effective"]["axes"]["jc"], "center")

    def test_namespace_and_required_values_are_strict(self):
        for wrong in (f'<w:jc xmlns:w="{qa.W}"/>', f'<w:jc xmlns:w="{qa.W}" w:val=""/>', '<x:ind xmlns:x="urn:fake" x:left="0"/>', f'<w:ind xmlns:w="{qa.W}" xmlns:x="urn:fake" x:left="0"/>', f'<w:ind xmlns:w="{qa.W}" w:left="1.5"/>', f'<w:ind xmlns:w="{qa.W}" w:bogus="0"/>'):
            with self.subTest(wrong=wrong):
                with self.assertRaises(AssertionError):
                    qa.declaration(ET.fromstring(f'<w:pPr xmlns:w="{qa.W}">{wrong}</w:pPr>'))

    def test_duplicate_jc_ind_and_wrong_particle_position_reject(self):
        for leaf in ("jc", "ind"):
            def edit(root):
                pr = root.find("w:body/w:p[w:pPr]/w:pPr", qa.NS)
                pr.append(copy.deepcopy(pr.find("w:" + leaf, qa.NS)))
            with self.subTest(leaf=leaf):
                with self.assertRaises(AssertionError):
                    qa.inspect_package(document_change("direct-no-styles", edit))
        def move(root):
            paragraph = root.find("w:body/w:p[w:pPr]", qa.NS)
            pr = paragraph.find("w:pPr", qa.NS); paragraph.remove(pr); paragraph.append(pr)
        with self.assertRaises(AssertionError):
            qa.inspect_package(document_change("direct-no-styles", move))

    def test_legal_unsupported_values_remain_native_values_not_parse_errors(self):
        for axis, value in (("left", "-1"), ("left", "31681"), ("right", "1.5cm"), ("jc", "start")):
            with self.subTest(axis=axis, value=value):
                parts, _ = qa.mutate(qa.fixture_parts("direct-no-styles"), axis, value)
                row = qa.inspect_package(qa.opc.Package(parts=parts))["paragraphs"][1]
                self.assertEqual(row["effective"]["axes"][axis], value)
                self.assertTrue(row["effective"]["unsupported"])

    def test_missing_referenced_style_cycle_and_wrong_type_reject(self):
        for mode in ("missing", "cycle", "type"):
            parts = qa.fixture_parts("styles-chain"); package = qa.opc.Package(parts=parts)
            root = package.xml(package.related("styles"))
            base = root.find("w:style[@w:styleId='Base']", qa.NS)
            equation = root.find("w:style[@w:styleId='Equation']", qa.NS)
            if mode == "missing":
                root.remove(base)
            elif mode == "cycle":
                based = ET.Element(qa.w("basedOn"), {qa.w("val"): "Equation"}); base.insert(0, based)
            else:
                equation.set(qa.w("type"), "character")
            parts[package.related("styles")] = qa.opc.xml_bytes(root)
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                qa.inspect_package(qa.opc.Package(parts=parts))

    def test_fixtures_are_deterministic_opc_and_existing_target_is_protected(self):
        BASE.mkdir(parents=True, exist_ok=True)
        folder = Path(tempfile.mkdtemp(prefix="pure-opc-", dir=BASE))
        for kind in qa.KINDS:
            left, right = folder / (kind + "-one.docx"), folder / (kind + "-two.docx")
            qa.opc.write_package(left, qa.fixture_parts(kind)); qa.opc.write_package(right, qa.fixture_parts(kind))
            self.assertEqual(left.read_bytes(), right.read_bytes())
            self.assertEqual(qa.inspect_package(qa.opc.Package(left))["paragraphs"], inspected(kind)["paragraphs"])
            with self.assertRaises(AssertionError):
                qa.opc.write_package(left, qa.fixture_parts(kind))

    def test_current_fragment_reparses_sparse_direct_and_actual_selected_style_context(self):
        for kind in qa.KINDS:
            package = qa.opc.Package(parts=qa.fixture_parts(kind))
            native = qa.inspect_package(package)
            # Select by the current native formula ordinal, not its repeated text.
            paragraph = list(package.xml(package.document).find("w:body", qa.NS))[native["paragraphs"][1]["bodyIndex"]]
            fragments = [package.xml(package.related("settings")).find("m:mathPr", qa.NS)]
            if package.related("styles"):
                fragments.append(package.xml(package.related("styles")))
            fragments.extend([paragraph.find("w:pPr", qa.NS), paragraph.find("m:oMathPara", qa.NS)])
            raw = "".join(ET.tostring(node, encoding="unicode") for node in fragments)
            current = qa.inspect_fragment(raw)
            self.assertEqual(current["direct"], native["paragraphs"][1]["direct"])
            self.assertEqual(current["effective"]["axes"], native["paragraphs"][1]["effective"]["axes"])
            self.assertEqual(current["mathObjects"], native["paragraphs"][1]["mathObjects"])

    def test_mathml_inspection_requires_current_real_fraction_and_namespace(self):
        ns = "http://www.w3.org/1998/Math/MathML"
        raw = f'<math xmlns="{ns}" display="block" style="text-align:center;margin-left:24pt;margin-right:18pt"><mrow><mfrac><mi>a</mi><mi>b</mi></mfrac></mrow></math>'
        value = qa.inspect_mathml(raw)
        self.assertEqual(value["tokens"], ["a", "b"])
        self.assertEqual(value["css"]["margin-left"], "24pt")
        for wrong in (raw.replace(ns, "urn:fake"), raw.replace("<mi>a</mi>", "<mi>b</mi>"), raw.replace("<mfrac>", "<mrow>").replace("</mfrac>", "</mrow>"), raw.replace("<mrow>", "<mrow><merror>bad</merror>", 1), raw.replace("text-align:center", "text-align:center;text-align:right")):
            with self.subTest(wrong=wrong), self.assertRaises(AssertionError):
                qa.inspect_mathml(wrong)

    def test_epub_xhtml_allows_only_the_inert_html5_prolog_without_changing_bytes(self):
        html = b'<html xmlns="http://www.w3.org/1999/xhtml"><body><p>KEEP</p></body></html>'
        for prefix in (b"", b"<!DOCTYPE html>\n", b'<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html>\n', b'\xef\xbb\xbf<?xml version="1.0"?>\r\n<!DOCTYPE html>\r\n'):
            raw = prefix + html
            before = qa.opc.digest(raw)
            root = qa.parse_epub_xhtml(raw)
            self.assertEqual(root.tag, "{http://www.w3.org/1999/xhtml}html")
            self.assertEqual("".join(root.itertext()), "KEEP")
            self.assertEqual(qa.opc.digest(raw), before)

    def test_epub_xhtml_rejects_external_internal_duplicate_and_misplaced_declarations(self):
        html = b'<html xmlns="http://www.w3.org/1999/xhtml"><body/></html>'
        bad = (
            b'<!DOCTYPE html SYSTEM "https://example.invalid/remote.dtd">' + html,
            b'<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "https://example.invalid/remote.dtd">' + html,
            b'<!DOCTYPE html [<!ENTITY x "EXPAND">]>' + html,
            b'<!DOCTYPE html [ ]>' + html,
            b'<!DOCTYPE html><!DOCTYPE html>' + html,
            b'<!DOCTYPE html >' + html,
            html.replace(b'<body/>', b'<body><!DOCTYPE html></body>'),
            b'<wrapper><!DOCTYPE html>' + html + b'</wrapper>',
            b'<!ENTITY x "EXPAND">' + html,
            (b'<!DOCTYPE html>' + html).decode().encode("utf-16"),
            (b'<!DOCTYPE html>' + html).decode().encode("utf-32-le"),
            b'<!DOCTYPE html>' + html.replace(b'http://www.w3.org/1999/xhtml', b'urn:fake'),
        )
        for raw in bad:
            with self.subTest(raw=raw[:100]), self.assertRaises(AssertionError):
                qa.parse_epub_xhtml(raw)

    def test_docx_parser_still_rejects_an_html5_doctype(self):
        with self.assertRaisesRegex(AssertionError, "DTD/entity forbidden"):
            qa.opc.parse_xml(b'<!DOCTYPE html><html/>')

    def test_epub_html5_spine_keeps_current_math_and_still_rejects_lost_or_wrong_formula(self):
        # An independent synthetic package tests the reader, not the product.
        BASE.mkdir(parents=True, exist_ok=True)
        folder = Path(tempfile.mkdtemp(prefix="pure-epub-reader-", dir=BASE))
        prefix = '<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html>\n'
        html = '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Control</title></head><body>{}</body></html>'
        formula = '<math xmlns="http://www.w3.org/1998/Math/MathML" display="block" style="text-align:right;margin-left:36pt;margin-right:9pt"><mrow><mfrac><mi>a</mi><mi>b</mi></mfrac></mrow></math>'
        container = '<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>'
        opf = '<package><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml"/><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="nav"/><itemref idref="chapter"/></spine></package>'
        def package(name, chapter):
            target = folder / name
            with zipfile.ZipFile(target, "w") as archive:
                archive.writestr("META-INF/container.xml", container)
                archive.writestr("OEBPS/content.opf", opf)
                archive.writestr("OEBPS/nav.xhtml", prefix + html.format("<nav>Contents</nav>"))
                archive.writestr("OEBPS/chapter.xhtml", prefix + html.format(chapter))
            return target
        good = package("valid-html5.epub", formula * 3)
        before = qa.opc.digest(good.read_bytes())
        current = qa.inspect_epub(good)
        self.assertEqual(len(current["math"]), 3)
        self.assertTrue(current["independentXmlReparsed"])
        self.assertFalse(current["readerGeometryCertified"])
        self.assertEqual(qa.opc.digest(good.read_bytes()), before)
        self.assertTrue(all(row["tokens"] == ["a", "b"] for row in current["math"]))
        for name, content in (("missing.epub", formula * 2), ("changed.epub", formula * 2 + formula.replace("<mi>a</mi>", "<mi>c</mi>"))):
            with self.subTest(name=name), self.assertRaises(AssertionError):
                qa.inspect_epub(package(name, content))

    def test_flow_inspection_does_not_accept_raster_or_embedded_original_text(self):
        svg = '<svg xmlns="http://www.w3.org/2000/svg" width="1.02em" height="1em" viewBox="-10 -500 1020 1000"><g data-math-ink="true" transform="scale(1 -1)"><path d="M0 0L1 2" transform="translate(0 0) scale(1)"/><path d="M3 4L5 6" transform="translate(0 0) scale(1)"/></g></svg>'
        good = '<div>' + svg * 3 + '</div>'
        self.assertEqual(len(qa.inspect_flow(good)["objects"]), 3)
        for wrong in (svg, good.replace('<path d="M3 4L5 6" transform="translate(0 0) scale(1)"/>', '<image href="old.png"/>'), '<pre>readmd-math</pre>'):
            with self.subTest(wrong=wrong), self.assertRaises(AssertionError):
                qa.inspect_flow(wrong)

    def test_drawing_coordinates_preserve_raw_curve_rectangle_and_scalar_values(self):
        curve = qa.drawing_item(("c", [1.0, 2.0], [3.0, 4.0], [5.0, 6.0], [7.0, 8.0]))
        self.assertEqual(curve["coordinates"], [[1, 2], [3, 4], [5, 6], [7, 8]])
        rectangle = qa.drawing_item(("re", [1.0, 2.0, 3.0, 4.0], -1))
        self.assertEqual(rectangle["scalar"], [-1])
        self.assertEqual(rectangle["coordinates"], [[1, 2, 3, 4]])
        for wrong in (("new-op", [1, 2]), ("l", [float("nan"), 2]), ("l", [1, 2, 3])):
            with self.subTest(wrong=wrong), self.assertRaises(AssertionError):
                qa.drawing_item(wrong)

    def test_svg_ink_uses_real_bezier_extrema_and_current_em_scale(self):
        raw = b'<svg xmlns="http://www.w3.org/2000/svg" width="25.02em" height="1.5em" viewBox="-10 -1000 25020 1500"><g data-math-ink="true" transform="scale(1 -1)"><path d="M0 0Q1000 2000 2000 0Z" transform="translate(100 0) scale(1)"/><path d="M0 0L1000 1000L2000 0Z" transform="translate(150 400) scale(1)"/><rect x="50" y="100" width="2200" height="40"/></g><g data-math-selection-regions="true"><rect x="-9999" y="-9999" width="99999" height="99999" fill="transparent"/></g></svg>'
        current = qa.svg_ink(raw)
        self.assertEqual(current["unitsPerEm"], 1000)
        self.assertAlmostEqual(current["layoutAdvancePt"], 300)
        self.assertAlmostEqual(current["glyphs"][0]["inkBoundsPt"][3], 12)
        self.assertAlmostEqual(current["inkBoundsPt"][0], .6)
        self.assertAlmostEqual(current["inkBoundsPt"][2], 27)
        self.assertTrue(current["actualBezierExtremaInspected"])
        self.assertEqual(current["glyphs"][0]["sideBearingXPt"], 0)
        for wrong in (raw.replace(b'width="25.02em"', b'width="300em"'), raw.replace(b'transform="scale(1 -1)"', b'transform="translate(30 0)"'), raw.replace(b'viewBox="-10', b'viewBox="-100')):
            with self.subTest(wrong=wrong), self.assertRaises(AssertionError):
                qa.svg_ink(wrong)

    def test_pdf_ink_uses_curve_extrema_rather_than_control_point_envelope(self):
        # The cubic has controls at y=20 but its actual maximum is y=15.
        item = qa.drawing_item(("c", [0, 0], [0, 20], [20, 20], [20, 0]))
        actual = qa.drawing_ink_bounds([item])
        self.assertEqual(actual, [0, 0, 20, 15])

    def test_single_stroke_has_exact_physical_half_width_and_cap_not_centerline_only(self):
        item = qa.drawing_item(("l", [0, 0], [20, 0]))
        drawing = {"items": [item], "fill": None, "color": [0, 0, 0], "width": 2, "lineCap": [0, 0, 0]}
        self.assertEqual(qa.painted_drawing_bounds(drawing), [0, -1, 20, 1])
        drawing["lineCap"] = [1, 1, 1]
        self.assertEqual(qa.painted_drawing_bounds(drawing), [-1, -1, 21, 1])
        drawing["strokeOpacity"] = 0
        with self.assertRaises(AssertionError):
            qa.painted_drawing_bounds(drawing)
        drawing["strokeOpacity"] = 1
        drawing["items"] = [qa.drawing_item(("c", [0, 0], [0, 20], [20, 20], [20, 0]))]
        with self.assertRaises(AssertionError):
            qa.painted_drawing_bounds(drawing)

    def test_pdf_oracle_reads_actual_vector_paths_and_page_frames_in_owned_control_pdf(self):
        # This independent synthetic PDF tests the reader, not ReadMD output.
        import pymupdf as fitz
        BASE.mkdir(parents=True, exist_ok=True)
        folder = Path(tempfile.mkdtemp(prefix="pure-reader-", dir=BASE))
        document = fitz.open(); page = document.new_page(width=210 * 72 / 25.4, height=297 * 72 / 25.4)
        for ordinal in range(3):
            baseline = 100 + ordinal * 100
            page.insert_text((18 * 72 / 25.4, baseline), "M07_BEFORE_" + str(ordinal + 1), fontsize=12)
            page.draw_circle((100, baseline + 30), 4, fill=(0, 0, 0), width=0)
            page.draw_rect(fitz.Rect(97, baseline + 42, 103, baseline + 49), fill=(0, 0, 0), width=0)
            page.draw_line((94, baseline + 38), (106, baseline + 38), width=0.5)
        page.insert_text((18 * 72 / 25.4, 400), "M07_SUFFIX_KEEP", fontsize=12)
        file = folder / "synthetic-reader-control.pdf"; document.save(file); document.close()
        current = qa.inspect_pdf(file)
        self.assertEqual(len(current["formulas"]), 3)
        self.assertTrue(current["actualVectorPathsInspected"])
        self.assertTrue(all(len(row["drawings"]) == 3 for row in current["formulas"]))
        self.assertTrue(all(row["page"] == 0 for row in current["formulas"]))
        self.assertIn("c", {item["operation"] for row in current["formulas"] for drawing in row["drawings"] for item in drawing["items"]})
        self.assertFalse(current["wordApplicationCertified"])
        broken = fitz.open(); page = broken.new_page()
        for ordinal in range(3):
            page.insert_text((51, 100 + ordinal * 100), "M07_BEFORE_" + str(ordinal + 1))
        broken.new_page().insert_text((51, 100), "M07_SUFFIX_KEEP")
        bad_file = folder / "synthetic-wrong-page.pdf"; broken.save(bad_file); broken.close()
        with self.assertRaises(AssertionError):
            qa.inspect_pdf(bad_file)


if __name__ == "__main__":
    unittest.main()
