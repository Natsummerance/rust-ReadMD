"""Pure independent controls. No ReadMD process, font engine or GUI starts."""
import copy
from decimal import Decimal
from fractions import Fraction
import importlib.util
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("unit_measures_qa", Path(__file__).with_name("inspect-math-unit-measures.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)
BASE = Path(__file__).resolve().parents[2] / ".cache/capability-upgrade/batch3-m07-unit-measures-20261024/qa"


def inspect(parts):
    return qa.inspect_package(qa.Package(parts=parts))


def pr(text):
    return qa.declaration(qa.parse_xml((f'<w:pPr xmlns:w="{qa.W}">' + text + '</w:pPr>').encode()))


def mathml(left="2.8346456692913384pt", right="28.346456692913385pt", align="left"):
    return '<math xmlns="http://www.w3.org/1998/Math/MathML" display="block" style="text-align:' + align + ';margin-left:' + left + ';margin-right:' + right + '"><mrow><mfrac><mi>a</mi><mi>b</mi></mfrac></mrow></math>'


def svg():
    return '<svg xmlns="http://www.w3.org/2000/svg" width="1.02em" height="1.02em" viewBox="-10 -10 1020 1020" style="vertical-align:0em" aria-hidden="true"><g data-math-ink="true" fill="currentColor" transform="scale(1 -1)"><path d="M0 0L10 0L10 10Z" transform="translate(100 200) scale(1)"/><path d="M0 0L10 0L10 10Z" transform="translate(100 100) scale(1)"/><rect x="95" y="150" width="20" height="2"/></g></svg>'


def epub_parts(body):
    return {"mimetype": b"application/epub+zip",
            "META-INF/container.xml": b'<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
            "OEBPS/content.opf": b'<package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/></spine></package>',
            "OEBPS/one.xhtml": b'<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>ONE</title></head><body>KEEP</body></html>',
            "OEBPS/two.xhtml": ('<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>TWO</title></head><body>' + body + '</body></html>').encode()}


class UnitMeasureOracleControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        BASE.mkdir(parents=True, exist_ok=True)
        cls.folder = Path(tempfile.mkdtemp(prefix="pure-oracle-", dir=BASE))

    def archive(self, name, parts):
        file = self.folder / name
        self.assertFalse(file.exists())
        with zipfile.ZipFile(file, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            for part, raw in parts.items():
                archive.writestr(part, raw)
        return file

    def test_exact_six_unit_factors_and_nonintegral_twips(self):
        expected = {"1mm": (Fraction(360, 127), Fraction(7200, 127)),
                    "1cm": (Fraction(3600, 127), Fraction(72000, 127)),
                    "0.5in": (Fraction(36), Fraction(720)),
                    "18.125pt": (Fraction(145, 8), Fraction(725, 2)),
                    "2pc": (Fraction(24), Fraction(480)),
                    "1.5pi": (Fraction(18), Fraction(360))}
        for raw, (points, twips) in expected.items():
            with self.subTest(raw=raw):
                value = qa.measure(raw)
                self.assertEqual(value["raw"], raw)
                self.assertEqual(value["points"], qa.ratio(points))
                self.assertEqual(value["twips"], qa.ratio(twips))
                self.assertEqual(value["pointsF64"], float(points))
                self.assertEqual(value["nonIntegralTwips"], twips.denominator != 1)
                self.assertTrue(value["inPhysicalDomain"])

    def test_same_value_different_unit_keeps_original_lexeme(self):
        left, right = qa.measure("1mm"), qa.measure("0.1cm")
        self.assertEqual(left["points"], right["points"])
        self.assertNotEqual(left["raw"], right["raw"])
        self.assertEqual(qa.measure("1pc")["points"], qa.measure("1pi")["points"])
        self.assertEqual(qa.measure("0.0005pt")["twips"], qa.ratio(Fraction(1, 100)))

    def test_integer_twips_and_signed_zero_are_not_unitless_decimal(self):
        self.assertEqual(qa.measure("  +720\t")["points"], qa.ratio(Fraction(36)))
        for raw in ("-0", "-000.000mm", "0000.00in", "-0pt"):
            with self.subTest(raw=raw):
                self.assertEqual(qa.measure(raw)["points"], qa.ratio(Fraction(0)))
                self.assertEqual(qa.measure(raw)["raw"], raw)
                self.assertTrue(qa.measure(raw)["inPhysicalDomain"])
        with self.assertRaises(AssertionError):
            qa.measure("1.5")

    def test_long_zero_padding_is_safely_normalized_only_for_independent_arithmetic(self):
        for raw, wanted in (("0" * 6000 + "1.000" + "0" * 6000 + "mm", Fraction(360, 127)),
                            ("-" + "0" * 6000 + "." + "0" * 6000 + "pt", Fraction(0))):
            self.assertEqual(qa.measure(raw)["points"], qa.ratio(wanted))
            self.assertEqual(qa.measure(raw)["raw"], raw)

    def test_invalid_schema_spelling_never_becomes_a_numeric_success(self):
        for raw in ("+1mm", ".5in", "1e2pt", "1MM", "1pt ", " 1pt", "1.", "1.mm", "NaNpt", "infpt", "1.2.3cm", "١mm", "", "1\x00mm"):
            with self.subTest(raw=raw), self.assertRaises(AssertionError):
                qa.measure(raw)

    def test_exact_admission_domain_and_u128_capacity_are_distinct(self):
        self.assertTrue(qa.measure("22in")["inPhysicalDomain"])
        for raw in ("-1mm", "1584.000000001pt", "23in"):
            self.assertFalse(qa.measure(raw)["inPhysicalDomain"])
        too_precise = qa.measure("0." + "1" * 80 + "pt")
        self.assertTrue(too_precise["inPhysicalDomain"])
        self.assertFalse(too_precise["exactConsumerRepresentable"])
        self.assertEqual(too_precise["raw"], "0." + "1" * 80 + "pt")

    def test_native_two_fixtures_cover_every_unit_and_sparse_origin(self):
        for kind in qa.KINDS:
            value = inspect(qa.fixture_parts(kind))
            self.assertEqual(value["auxiliaryParts"], {})
            self.assertFalse(value["sourcePayloadUsedAsNativeEvidence"])
            self.assertFalse(value["nativeStyleProvenanceRoundtripCertified"])
            rows = value["paragraphs"]
            self.assertEqual(len(rows), 3)
            self.assertEqual([row["direct"]["indPresent"] for row in rows], [True, False, False])
            self.assertEqual([row["direct"]["jcPresent"] for row in rows], [True, False, False])
            self.assertEqual([row["effective"]["provenance"] for row in rows], [
                {"left": "direct", "right": "direct", "jc": "direct"},
                {"left": "style:Base", "right": "style:Equation", "jc": "style:Equation"},
                {"left": "docDefaults", "right": "docDefaults", "jc": "docDefaults"}])
            for i, row in enumerate(rows):
                self.assertEqual(row["effective"]["axes"], dict(zip(("left", "right", "jc"), (*qa.RAW_ROWS[i], kind))))
                self.assertEqual(row["effective"]["unsupported"], [])
                self.assertEqual(row["mathTexts"], ["ab"])
                self.assertEqual(row["mathObjects"], rows[0]["mathObjects"])
                self.assertEqual([run["sizeHalfPoints"] for run in row["runDeclarations"]], ["24", "24"])
                for axis, raw in zip(("left", "right"), qa.RAW_ROWS[i]):
                    self.assertEqual(row["effective"]["measures"][axis], qa.measure(raw))

    def test_native_fixtures_are_deterministic_no_payload_and_never_overwrite(self):
        for kind in qa.KINDS:
            one, two = self.folder / (kind + "-one.docx"), self.folder / (kind + "-two.docx")
            qa.opc.write_package(one, qa.fixture_parts(kind))
            qa.opc.write_package(two, qa.fixture_parts(kind))
            self.assertEqual(one.read_bytes(), two.read_bytes())
            before = qa.opc.digest(one.read_bytes())
            with self.assertRaisesRegex(AssertionError, "Output must be new"):
                qa.opc.write_package(one, qa.fixture_parts(kind))
            self.assertEqual(qa.opc.digest(one.read_bytes()), before)

    def test_independent_cascade_does_not_use_a_direct_container_as_all_axes(self):
        parts = qa.fixture_parts("left")
        package = qa.Package(parts=parts)
        context = qa.context_from_styles(package.xml(package.related("styles")), package.related("styles"))
        value = qa.effective(context, pr('<w:pStyle w:val="Equation"/><w:ind w:left="0.1cm"/>'))
        self.assertEqual(value["axes"], {"left": "0.1cm", "right": "18.125pt", "jc": "left"})
        self.assertEqual(value["provenance"], {"left": "direct", "right": "style:Equation", "jc": "style:Equation"})
        restored = qa.effective(context, pr('<w:pStyle w:val="Equation"/>'))
        self.assertEqual(restored["axes"]["left"], "0.5in")
        self.assertEqual(restored["provenance"]["left"], "style:Base")

    def test_original_guards_are_not_erased_by_legal_unit_support(self):
        empty = {"default": None, "styles": {}, "defaults": pr("")}
        for xml in ('<w:ind w:left="-1mm"/>', '<w:ind w:left="1584.0001pt"/>',
                    '<w:ind w:left="1mm" w:start="0"/>', '<w:ind w:left="1mm" w:leftChars="100"/>',
                    '<w:ind w:left="1mm" w:firstLine="20"/>', '<w:bidi/><w:ind w:left="1mm"/>',
                    '<w:ind w:left="1mm"/><w:mirrorIndents/>', '<w:ind w:left="1mm"/><w:textDirection w:val="tbRl"/>'):
            with self.subTest(xml=xml):
                self.assertTrue(qa.effective(empty, pr(xml))["unsupported"])

    def test_unit_projection_and_existing_start_end_keep_raw_and_physical_separate(self):
        empty = {"default": None, "styles": {}, "defaults": pr("")}
        for raw, physical in (("start", "left"), ("end", "right")):
            value = qa.effective(empty, pr('<w:ind w:left="1mm"/><w:jc w:val="' + raw + '"/>'))
            self.assertEqual(value["axes"]["jc"], raw)
            self.assertEqual(value["physicalAxes"]["jc"], physical)
            self.assertEqual(value["physicalPoints"]["jc"], physical)
            self.assertEqual(value["unsupported"], [])

    def test_native_ppr_duplicate_order_and_foreign_uri_are_rejected(self):
        for xml in ('<w:ind w:left="1mm"/><w:ind w:right="1cm"/>',
                    '<w:jc w:val="left"/><w:ind w:left="1mm"/>',
                    '<w:ind xmlns:f="urn:foreign" f:left="1mm"/>', '<w:jc w:val="bogus"/>'):
            with self.subTest(xml=xml), self.assertRaises(AssertionError):
                pr(xml)

    def test_fractional_css_compares_against_rationals_without_rounding(self):
        value = qa.measure("1mm")
        qa.assert_css_points("2.8346456692913384pt", value["points"])
        for wrong in ("2.85pt", "2pt", "1mm", "0pt"):
            with self.assertRaises(AssertionError):
                qa.assert_css_points(wrong, value["points"])
        self.assertGreater(abs(Fraction(57, 20) - Fraction(360, 127)), Fraction(2, 1000))

    def test_selected_mathml_positive_has_real_fraction_and_tokens(self):
        value = qa.logical.inspect_mathml(mathml())
        self.assertEqual(value["tokens"], ["a", "b"])
        self.assertEqual(value["fractions"], 1)
        self.assertEqual(qa.logical.inspect_flow(mathml() * 3)["objects"][0]["kind"], "mathml")

    def test_mathml_hidden_subtrees_and_choice_content_cannot_certify_visible_math(self):
        variants = [mathml().replace('<mfrac>', '<mfrac style="display:none">'),
                    mathml().replace('<mfrac>', '<mphantom><mfrac>').replace('</mfrac>', '</mfrac></mphantom>'),
                    mathml().replace('<mfrac>', '<maction><mfrac>').replace('</mfrac>', '</mfrac></maction>'),
                    mathml().replace('<mfrac>', '<mfrac style="opacity:0!important">'),
                    mathml().replace('<mi>a</mi>', '<mi hidden="">a</mi>')]
        for raw in variants:
            with self.subTest(raw=raw), self.assertRaises(AssertionError):
                qa.logical.inspect_flow(raw * 3)

    def test_real_shaped_accessibility_hidden_svg_is_visually_valid_and_all_ink_is_inspected(self):
        value = qa.svg_ink(svg().encode())
        self.assertEqual(value["fixedUnitsPerEm"], 1000)
        self.assertEqual([row["kind"] for row in value["normalizedPrimitives"]], ["path", "path", "rect"])
        wrapped = '<div class="readmd-export-math-block" style="text-align:left"><span role="math" aria-label="a over b">' + svg() + '</span></div>'
        self.assertEqual(len(qa.logical.inspect_flow(wrapped * 3)["objects"]), 3)

    def test_inert_svg_container_or_wrong_normalization_cannot_prove_paint(self):
        hidden = svg().replace('<g data-math-ink', '<defs><g data-math-ink').replace('</g></svg>', '</g></defs></svg>')
        with self.assertRaises(AssertionError):
            qa.logical.inspect_flow(hidden * 3)
        with self.assertRaises(AssertionError):
            qa.svg_ink(svg().replace('1.02em', '1.04em').encode())

    def test_actual_epub_spine_all_members_and_visible_current_math_are_selected(self):
        body = ''.join(mathml(str(float(qa.measure(left)["pointsF64"])) + 'pt', str(float(qa.measure(right)["pointsF64"])) + 'pt') for left, right in qa.RAW_ROWS)
        value = qa.logical.inspect_epub(self.archive("actual-spine.epub", epub_parts(body)))
        self.assertEqual(len(value["math"]), 3)
        self.assertEqual(len(value["spineParts"]), 2)
        for i, row in enumerate(value["math"]):
            for axis, raw in zip(("left", "right"), qa.RAW_ROWS[i]):
                qa.assert_css_points(row["css"]["margin-" + axis], qa.measure(raw)["points"])

    def test_epub_missing_changed_hidden_math_and_surrogate_are_rejected(self):
        variants = [mathml() * 2, (mathml() * 3).replace('<mi>b</mi>', '<mi>c</mi>', 1),
                    (mathml() * 3).replace('<mfrac>', '<mfrac style="display:none">', 1), svg() + mathml() * 3]
        for i, body in enumerate(variants):
            with self.subTest(i=i), self.assertRaises(AssertionError):
                qa.logical.inspect_epub(self.archive("bad-visible-" + str(i) + ".epub", epub_parts(body)))

    def test_docx_xml_dtd_and_entities_stay_forbidden(self):
        for raw in (b'<!DOCTYPE x><x/>', b'<!DOCTYPE x [<!ENTITY e "x">]><x>&e;</x>',
                    b'<!DOCTYPE x SYSTEM "file:///never"><x/>', '<x/>'.encode('utf-16')):
            with self.assertRaises(AssertionError):
                qa.parse_xml(raw)

    def test_epub_only_exact_single_inert_html5_doctype_is_allowed(self):
        good = b'<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml"><body/></html>'
        qa.base.parse_epub_xhtml(good)
        for raw in (good.replace(b'<!DOCTYPE html>', b'<!DOCTYPE html SYSTEM "file:///never">'),
                    good.replace(b'<!DOCTYPE html>', b'<!DOCTYPE html [<!ENTITY e "x">]>'),
                    b'<!DOCTYPE html>\n' + good, good.decode().encode('utf-16')):
            with self.assertRaises(AssertionError):
                qa.base.parse_epub_xhtml(raw)

    def test_epub_external_relationship_and_zip_traversal_stay_forbidden(self):
        parts = epub_parts(mathml() * 3)
        parts["OEBPS/content.opf"] = parts["OEBPS/content.opf"].replace(b'href="two.xhtml"', b'href="https://invalid.example/two.xhtml"')
        with self.assertRaises(AssertionError):
            qa.logical.inspect_epub(self.archive("external.epub", parts))
        parts = epub_parts(mathml() * 3)
        parts["../escape"] = b"NO"
        with self.assertRaises(AssertionError):
            qa.logical.inspect_epub(self.archive("traversal.epub", parts))


if __name__ == "__main__":
    unittest.main(verbosity=2)
