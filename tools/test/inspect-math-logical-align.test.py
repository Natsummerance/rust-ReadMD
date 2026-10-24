"""Pure controls for the independent oracle. No product, engine or GUI starts."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("logical_align_qa", Path(__file__).with_name("inspect-math-logical-align.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)
BASE = Path(__file__).resolve().parents[2] / ".cache/capability-upgrade/batch3-m07-logical-20261024/qa"


def inspect(parts):
    return qa.inspect_package(qa.Package(parts=parts))


def pr(text):
    return qa.declaration(qa.parse_xml((f'<w:pPr xmlns:w="{qa.W}">' + text + '</w:pPr>').encode()))


def context(parts):
    package = qa.Package(parts=parts)
    part = package.related("styles")
    return qa.context_from_styles(package.xml(part), part)


def mathml(align="left"):
    return '<math xmlns="http://www.w3.org/1998/Math/MathML" display="block" style="text-align:' + align + ';margin-left:36pt;margin-right:18pt"><mrow><mfrac><mi>a</mi><mi>b</mi></mfrac></mrow></math>'


def epub_parts(body=None):
    body = mathml() * 3 if body is None else body
    return {"mimetype": b"application/epub+zip",
            "META-INF/container.xml": b'<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
            "OEBPS/content.opf": b'<package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="first" href="first.xhtml" media-type="application/xhtml+xml"/><item id="second" href="second.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="first"/><itemref idref="second"/></spine></package>',
            "OEBPS/first.xhtml": b'<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>First</title></head><body><p>KEEP FIRST</p></body></html>',
            "OEBPS/second.xhtml": ('<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Second</title></head><body>' + body + '</body></html>').encode()}


class LogicalOracleControls(unittest.TestCase):
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

    def test_two_native_only_fixtures_have_identical_math_and_distinct_sparse_origins(self):
        for kind, physical in qa.LOGICAL.items():
            with self.subTest(kind=kind):
                value = inspect(qa.fixture_parts(kind))
                self.assertEqual(value["auxiliaryParts"], {})
                self.assertFalse(value["nativeStyleProvenanceRoundtripCertified"])
                self.assertFalse(value["sourcePayloadUsedAsNativeEvidence"])
                rows = value["paragraphs"]
                self.assertEqual([row["direct"]["jcPresent"] for row in rows], [True, False, False])
                self.assertEqual([row["direct"]["indPresent"] for row in rows], [True, False, False])
                self.assertEqual([row["effective"]["provenance"] for row in rows], [
                    {"left": "direct", "right": "direct", "jc": "direct"},
                    {"left": "style:Base", "right": "style:Equation", "jc": "style:Equation"},
                    {"left": "docDefaults", "right": "docDefaults", "jc": "docDefaults"}])
                for row in rows:
                    self.assertEqual(row["effective"]["axes"], {"left": "720", "right": "360", "jc": kind})
                    self.assertEqual(row["effective"]["physicalAxes"], {"left": "720", "right": "360", "jc": physical})
                    self.assertEqual(row["effective"]["unsupported"], [])
                    self.assertEqual(row["mathTexts"], ["ab"])
                    self.assertEqual(row["mathObjects"], rows[0]["mathObjects"])
                    self.assertEqual([run["sizeHalfPoints"] for run in row["runDeclarations"]], ["24", "24"])

    def test_fixture_is_deterministic_nonoverwriting_and_native_only(self):
        for kind in qa.LOGICAL:
            left, right = self.folder / (kind + "-1.docx"), self.folder / (kind + "-2.docx")
            qa.opc.write_package(left, qa.fixture_parts(kind))
            qa.opc.write_package(right, qa.fixture_parts(kind))
            self.assertEqual(left.read_bytes(), right.read_bytes())
            self.assertEqual(qa.inspect_package(qa.Package(left))["paragraphs"], inspect(qa.fixture_parts(kind))["paragraphs"])
            with self.assertRaises(AssertionError):
                qa.opc.write_package(left, qa.fixture_parts(kind))

    def test_raw_logical_value_and_context_are_never_rewritten_by_projection(self):
        ctx = context(qa.fixture_parts("start"))
        direct = pr('<w:pStyle w:val="Equation"/>')
        before = copy.deepcopy((ctx, direct))
        value = qa.effective(ctx, direct)
        self.assertEqual(value["axes"]["jc"], "start")
        self.assertEqual(value["physicalAxes"]["jc"], "left")
        self.assertEqual((ctx, direct), before)

    def test_direct_opposite_and_its_removal_restore_selected_style_origin(self):
        ctx = context(qa.fixture_parts("start"))
        changed = qa.effective(ctx, pr('<w:pStyle w:val="Equation"/><w:jc w:val="end"/>'))
        restored = qa.effective(ctx, pr('<w:pStyle w:val="Equation"/>'))
        self.assertEqual((changed["axes"]["jc"], changed["provenance"]["jc"]), ("end", "direct"))
        self.assertEqual((restored["axes"]["jc"], restored["provenance"]["jc"]), ("start", "style:Equation"))
        self.assertEqual(changed["axes"]["right"], "360")
        self.assertEqual(changed["provenance"]["left"], "style:Base")

    def test_selected_chain_and_defaults_do_not_use_unrelated_opposite_style(self):
        ctx = context(qa.fixture_parts("end"))
        self.assertEqual(qa.effective(ctx, pr('<w:pStyle w:val="Equation"/>'))["axes"]["jc"], "end")
        defaults = qa.effective(ctx, pr('<w:pStyle w:val="DefaultsOnly"/>'))
        self.assertEqual(defaults["provenance"]["jc"], "docDefaults")
        self.assertEqual(defaults["styleChain"], ["DefaultsOnly"])
        ctx["styles"]["Equation"]["declaration"]["axes"].pop("jc")
        value = qa.effective(ctx, pr('<w:pStyle w:val="Equation"/>'))
        self.assertEqual((value["axes"]["jc"], value["provenance"]["jc"]), ("start", "style:Base"))

    def test_inherited_bidi_true_blocks_and_direct_false_overrides(self):
        ctx = context(qa.fixture_parts("start"))
        ctx["styles"]["Base"]["declaration"]["guards"]["bidi"] = True
        blocked = qa.effective(ctx, pr('<w:pStyle w:val="Equation"/>'))
        self.assertIn("bidi", blocked["unsupported"])
        self.assertEqual(blocked["physicalAxes"]["jc"], "start")
        value = qa.effective(ctx, pr('<w:pStyle w:val="Equation"/><w:bidi w:val="0"/>'))
        self.assertEqual(value["unsupported"], [])
        self.assertEqual(value["physicalAxes"]["jc"], "left")

    def test_direction_guard_is_stricter_only_for_logical_jc(self):
        ctx = context(qa.fixture_parts("start"))
        for direction in qa.DIRECTIONS:
            value = qa.effective(ctx, pr(f'<w:jc w:val="start"/><w:textDirection w:val="{direction}"/>'))
            self.assertEqual(not value["unsupported"], direction == "lrTb", direction)
        self.assertEqual(qa.effective(ctx, pr('<w:jc w:val="left"/><w:textDirection w:val="tb"/>'))["unsupported"], [])

    def test_mirror_logical_indent_character_units_and_other_enums_do_not_enter_finite(self):
        ctx = context(qa.fixture_parts("start"))
        for text, key in [('<w:mirrorIndents/><w:jc w:val="start"/>', "mirrorIndents"),
                          ('<w:ind w:start="0"/><w:jc w:val="end"/>', "ind.start"),
                          ('<w:ind w:leftChars="0"/><w:jc w:val="start"/>', "ind.leftChars"),
                          ('<w:ind w:firstLine="0"/><w:jc w:val="start"/>', "ind.firstLine"),
                          ('<w:jc w:val="both"/>', "jc"),
                          ('<w:spacing w:after="0"/><w:jc w:val="start"/>', "spacing")]:
            with self.subTest(text=text):
                self.assertIn(key, qa.effective(ctx, pr(text))["unsupported"])

    def test_invalid_namespace_required_value_and_full_particle_order_reject(self):
        bad = ['<w:jc/>', '<w:jc w:val="bogus"/>', '<w:jc w:val="start"/><w:ind w:left="720"/>',
               '<w:jc w:val="start"/><w:jc w:val="end"/>', '<w:jc w:val="start"/><w:bidi/>',
               '<x:jc xmlns:x="urn:fake" x:val="start"/>', '<w:jc xmlns:x="urn:fake" x:val="start"/>',
               '<w:textDirection w:val="unknown"/>', '<w:bidi w:val="maybe"/>']
        for text in bad:
            with self.subTest(text=text), self.assertRaises(AssertionError):
                pr(text)

    def test_missing_empty_explicit_false_keep_sparse_states(self):
        self.assertFalse(qa.declaration(None)["pPrPresent"])
        self.assertTrue(pr("")["pPrPresent"])
        self.assertFalse(pr("")["indPresent"])
        self.assertTrue(pr('<w:ind/>')["indPresent"])
        self.assertIs(pr('<w:bidi w:val="0"/>')["guards"]["bidi"], False)

    def test_cycle_wrong_style_type_and_missing_selected_style_reject(self):
        for mode in ("cycle", "type", "missing"):
            ctx = context(qa.fixture_parts("start"))
            if mode == "cycle": ctx["styles"]["Base"]["basedOn"] = "Equation"
            if mode == "type": ctx["styles"]["Equation"]["type"] = "character"
            if mode == "missing": del ctx["styles"]["Equation"]
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                qa.effective(ctx, pr('<w:pStyle w:val="Equation"/>'))

    def test_native_duplicate_ppr_and_not_first_reject(self):
        for mode in ("duplicate", "position"):
            parts = qa.fixture_parts("start")
            root = qa.parse_xml(parts["word/document.xml"])
            paragraph = root.find("w:body/w:p[w:pPr]", qa.NS)
            properties = paragraph.find("w:pPr", qa.NS)
            if mode == "duplicate": paragraph.append(copy.deepcopy(properties))
            else: paragraph.remove(properties); paragraph.append(properties)
            parts["word/document.xml"] = qa.opc.xml_bytes(root)
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                inspect(parts)

    def test_current_fragment_keeps_whole_native_math_and_sparse_provenance(self):
        for kind in qa.LOGICAL:
            package = qa.Package(parts=qa.fixture_parts(kind))
            native = qa.inspect_package(package)
            for row in native["paragraphs"]:
                paragraph = list(package.xml(package.document).find("w:body", qa.NS))[row["bodyIndex"]]
                nodes = [package.xml(package.related("settings")).find("m:mathPr", qa.NS), package.xml(package.related("styles")), paragraph.find("w:pPr", qa.NS), paragraph.find("m:oMathPara", qa.NS)]
                fragment = qa.inspect_fragment("".join(ET.tostring(node, encoding="unicode") for node in nodes))
                self.assertEqual(fragment["direct"], row["direct"])
                self.assertEqual(fragment["effective"], row["effective"])
                self.assertEqual(fragment["mathObjects"], row["mathObjects"])

    def test_xml_dtd_and_utf16_are_not_authorized_by_epub_html5_exception(self):
        for raw in (b'<!DOCTYPE p><p/>', b'<!DOCTYPE p [<!ENTITY x "EXPAND">]><p>&x;</p>', '<!DOCTYPE p><p/>'.encode("utf-16")):
            with self.subTest(raw=raw[:30]), self.assertRaises(AssertionError):
                qa.parse_xml(raw)
        parts = qa.fixture_parts("start")
        parts["word/document.xml"] = parts["word/document.xml"].decode().encode("utf-16")
        with self.assertRaisesRegex(AssertionError, "UTF-8"):
            inspect(parts)

    def test_epub_all_selected_spine_keeps_three_actual_formulas_and_raw_hashes(self):
        parts = epub_parts()
        file = self.archive("good.epub", parts)
        digest = qa.opc.digest(file.read_bytes())
        value = qa.inspect_epub(file)
        self.assertEqual(value["spineParts"], ["OEBPS/first.xhtml", "OEBPS/second.xhtml"])
        self.assertEqual([row["tokens"] for row in value["math"]], [["a", "b"]] * 3)
        self.assertEqual(value["memberSha256"], {name: qa.opc.digest(raw) for name, raw in parts.items()})
        self.assertEqual(qa.opc.digest(file.read_bytes()), digest)

    def test_epub_cannot_credit_unselected_source_math_or_lost_wrong_operands(self):
        for name, body in (("missing", mathml() * 2), ("changed", mathml() * 2 + mathml().replace('<mi>a</mi>', '<mi>c</mi>')), ("raster", '<img src="old.svg"/>' + mathml() * 3), ("hidden", '<div style="display:none">' + mathml() * 3 + '</div>')):
            parts = epub_parts(body)
            parts["OEBPS/unselected.xhtml"] = mathml().encode()
            with self.subTest(name=name), self.assertRaises(AssertionError):
                qa.inspect_epub(self.archive(name + ".epub", parts))

    def test_epub_namespace_external_dtd_duplicate_spine_and_unsafe_zip_reject(self):
        for mode in ("container-ns", "opf-ns", "external-href", "duplicate-spine", "zip-escape", "external-dtd"):
            parts = epub_parts()
            if mode == "container-ns": parts["META-INF/container.xml"] = parts["META-INF/container.xml"].replace(b"urn:oasis:names:tc:opendocument:xmlns:container", b"urn:fake")
            if mode == "opf-ns": parts["OEBPS/content.opf"] = parts["OEBPS/content.opf"].replace(b"http://www.idpf.org/2007/opf", b"urn:fake")
            if mode == "external-href": parts["OEBPS/content.opf"] = parts["OEBPS/content.opf"].replace(b"first.xhtml", b"https://example.invalid/first.xhtml")
            if mode == "duplicate-spine": parts["OEBPS/content.opf"] = parts["OEBPS/content.opf"].replace(b'</spine>', b'<itemref idref="second"/></spine>')
            if mode == "zip-escape": parts["../escape"] = b"bad"
            if mode == "external-dtd": parts["OEBPS/first.xhtml"] = parts["OEBPS/first.xhtml"].replace(b'<!DOCTYPE html>', b'<!DOCTYPE html SYSTEM "https://example.invalid/remote.dtd">')
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                qa.inspect_epub(self.archive(mode + ".epub", parts))

    def test_html_current_math_only_and_hidden_script_source_do_not_count(self):
        flow = '<div class="readmd-export-math-block" style="text-align:left">' + mathml() + '</div>'
        self.assertEqual(len(qa.inspect_flow(flow * 3)["objects"]), 3)
        for wrong in (flow * 2, '<script type="application/json">' + flow * 3 + '</script>', '<div hidden>' + flow * 3 + '</div>', '<div style="visibility:hidden">' + flow * 3 + '</div>', (flow * 3).replace('<mi>a</mi>', '<mi>c</mi>')):
            with self.subTest(wrong=wrong[:80]), self.assertRaises(AssertionError):
                qa.inspect_flow(wrong)

    def test_existing_point_ink_helpers_still_use_actual_curve_extrema(self):
        curve = qa.base.drawing_item(("c", [0, 0], [0, 20], [20, 20], [20, 0]))
        self.assertEqual(qa.base.drawing_ink_bounds([curve]), [0, 0, 20, 15])
        with self.assertRaises(AssertionError):
            qa.base.drawing_item(("l", [float("nan"), 0], [1, 2]))

    def test_hidden_math_descendant_phantom_choice_and_explicit_zero_opacity_reject(self):
        positive = mathml()
        self.assertEqual(qa.inspect_mathml(positive)["tokens"], ["a", "b"])
        mutations = {
            "phantom": positive.replace('<mfrac>', '<mphantom><mfrac>').replace('</mfrac>', '</mfrac></mphantom>'),
            "choice": positive.replace('<mfrac>', '<maction actiontype="toggle"><mfrac>').replace('</mfrac>', '</mfrac></maction>'),
            "hidden-frac": positive.replace('<mfrac>', '<mfrac style="display:none">'),
            "hidden-operand": positive.replace('<mi>a</mi>', '<mi style="visibility:hidden!important">a</mi>'),
            "zero-important": positive.replace('<mrow>', '<mrow style="opacity:0!important">'),
            "zero-decimal": positive.replace('<mrow>', '<mrow style="opacity:0.00">'),
            "zero-signed": positive.replace('<mrow>', '<mrow style="opacity:+.0 !important">'),
        }
        for name, raw in mutations.items():
            with self.subTest(name=name, consumer="mathml"), self.assertRaises(AssertionError):
                qa.inspect_mathml(raw)
            flow = '<div class="readmd-export-math-block">' + raw + '</div>'
            with self.subTest(name=name, consumer="flow"), self.assertRaises(AssertionError):
                qa.inspect_flow(flow * 3)
            with self.subTest(name=name, consumer="epub"), self.assertRaises(AssertionError):
                qa.inspect_epub(self.archive("descendant-" + name + ".epub", epub_parts(raw * 3)))
        for nonzero in ("0.3", "1", ".01!important"):
            raw = positive.replace('<mrow>', '<mrow style="opacity:' + nonzero + '">')
            self.assertEqual(qa.inspect_mathml(raw)["tokens"], ["a", "b"])

    def test_actual_visible_svg_shape_may_be_aria_hidden_inside_accessible_math_wrapper(self):
        # Product shape: publication_svg() marks the painted SVG aria-hidden;
        # html_math provides the accessible role=math wrapper and name.
        svg = '<svg xmlns="http://www.w3.org/2000/svg" aria-hidden="true" width="1.02em" height="1em" viewBox="-10 -500 1020 1000"><g data-math-ink="true" transform="scale(1 -1)"><path d="M0 0L1 2" transform="translate(0 0) scale(1)"/><path d="M3 4L5 6" transform="translate(0 0) scale(1)"/></g></svg>'
        flow = '<div class="readmd-export-math-block" role="math" aria-label="a over b" style="text-align:left">' + svg + '</div>'
        value = qa.inspect_flow(flow * 3)
        self.assertEqual([row["kind"] for row in value["objects"]], ["svg"] * 3)
        self.assertTrue(all(row["ink"]["actualBezierExtremaInspected"] for row in value["objects"]))
        for wrong in ((flow * 3).replace('<g data-math-ink=', '<g style="display:none" data-math-ink='), '<div hidden>' + flow * 3 + '</div>'):
            with self.subTest(wrong=wrong[:80]), self.assertRaises(AssertionError):
                qa.inspect_flow(wrong)
        for container in ("defs", "symbol", "clipPath", "mask", "pattern", "marker"):
            wrong = flow.replace('<g data-math-ink=', '<' + container + '><g data-math-ink=').replace('</g></svg>', '</g></' + container + '></svg>')
            with self.subTest(container=container), self.assertRaises(AssertionError):
                qa.inspect_flow(wrong * 3)


if __name__ == "__main__":
    unittest.main(verbosity=2)
