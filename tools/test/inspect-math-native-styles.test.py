"""Pure oracle controls; synthetic models are never actual product evidence."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("native_style_qa", Path(__file__).with_name("inspect-math-native-styles.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)
BASE = Path(__file__).resolve().parents[2] / ".cache/capability-upgrade/batch3-m07-native-styles-20261024/qa"


def native_model_parts(kind, aliases=None):
    """Small independently assembled native shape for testing the checker."""
    parts = qa.fixture_parts(kind)
    package = qa.Package(parts=parts)
    before = qa.inspect_package(package)
    aliases = aliases or ({"Normal": "Model_B_Normal", "ReadMDMath": "Model_B_ReadMDMath"} if kind == "b" else {})
    style_part = package.related("styles")
    styles, document = package.xml(style_part), package.xml(package.document)
    for node in styles.findall("w:style", qa.NS):
        old = node.get(qa.w("styleId"))
        node.set(qa.w("styleId"), aliases.get(old, old))
        node.attrib.pop(qa.w("default"), None)
        based = node.find("w:basedOn", qa.NS)
        if based is not None:
            based.set(qa.w("val"), aliases.get(based.get(qa.w("val")), based.get(qa.w("val"))))
    for node in list(styles.findall("w:docDefaults", qa.NS)):
        styles.remove(node)
    defaults = ET.Element(qa.w("docDefaults"))
    ET.SubElement(ET.SubElement(defaults, qa.w("pPrDefault")), qa.w("pPr"))
    styles.insert(0, defaults)
    ET.SubElement(styles, qa.w("style"), {qa.w("type"): "paragraph", qa.w("styleId"): "Normal", qa.w("default"): "1"})
    ET.SubElement(styles, qa.w("style"), {qa.w("type"): "paragraph", qa.w("styleId"): "ReadMDMath"})
    body = document.find("w:body", qa.NS)
    for row in before["paragraphs"]:
        pr = list(body)[row["bodyIndex"]].find("w:pPr", qa.NS)
        selected = pr.find("w:pStyle", qa.NS)
        if selected is None:
            selected = ET.Element(qa.w("pStyle"))
            pr.insert(0, selected)
        original = row["effective"]["selectedStyle"]
        selected.set(qa.w("val"), aliases.get(original, original))
    parts[package.document], parts[style_part] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
    return parts


def combined_model_parts(reverse=False):
    a = native_model_parts("a", {"Equation": "Model_A_Equation", "DefaultMath": "Model_A_Default"})
    b = native_model_parts("b", {"Normal": "Model_B_Normal", "ReadMDMath": "Model_B_ReadMDMath", "Equation": "Model_B_Equation", "DefaultMath": "Model_B_Default"})
    ap, bp = qa.Package(parts=a), qa.Package(parts=b)
    parts = copy.deepcopy(a)
    styles = ap.xml(ap.related("styles"))
    by_id = {node.get(qa.w("styleId")): node for node in styles.findall("w:style", qa.NS)}
    for node in bp.xml(bp.related("styles")).findall("w:style", qa.NS):
        identity = node.get(qa.w("styleId"))
        if identity in by_id:
            assert qa.canonical(node) == qa.canonical(by_id[identity])
        else:
            styles.append(copy.deepcopy(node))
    document = ap.xml(ap.document)
    body = document.find("w:body", qa.NS)
    a_nodes = [copy.deepcopy(node) for node in body if node.tag != qa.w("sectPr")]
    b_nodes = [copy.deepcopy(node) for node in bp.xml(bp.document).find("w:body", qa.NS) if node.tag != qa.w("sectPr")]
    section = copy.deepcopy(body.find("w:sectPr", qa.NS))
    body[:] = (b_nodes + a_nodes if reverse else a_nodes + b_nodes) + [section]
    parts[ap.document], parts[ap.related("styles")] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
    return parts


def isolation_models():
    mixed = native_model_parts("a")
    mp = qa.Package(parts=mixed)
    document, styles = mp.xml(mp.document), mp.xml(mp.related("styles"))
    original_body = document.find("w:body", qa.NS)
    imported = [copy.deepcopy(node) for node in original_body if node.tag != qa.w("sectPr")]
    control = copy.deepcopy(next(node for node in original_body if node.find("m:oMathPara", qa.NS) is not None))
    pr = control.find("w:pPr", qa.NS)
    pr[:] = [ET.Element(qa.w("pStyle"), {qa.w("val"): "ReadMDMath"}), ET.Element(qa.w("jc"), {qa.w("val"): "center"})]
    ordinary = ET.Element(qa.w("p"))
    ET.SubElement(ET.SubElement(ordinary, qa.w("r")), qa.w("t")).text = "ORDINARY_UNCHANGED"
    section = copy.deepcopy(original_body.find("w:sectPr", qa.NS))
    original_body[:] = [ordinary, control, *imported, section]
    mixed[mp.document], mixed[mp.related("styles")] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
    baseline = copy.deepcopy(mixed)
    bp = qa.Package(parts=baseline)
    bd, bs = bp.xml(bp.document), bp.xml(bp.related("styles"))
    body = bd.find("w:body", qa.NS)
    body[:] = [copy.deepcopy(ordinary), copy.deepcopy(control), copy.deepcopy(section)]
    for node in list(bs.findall("w:style", qa.NS)):
        if node.get(qa.w("styleId")) in {"Base", "Equation", "DefaultMath"}:
            bs.remove(node)
    baseline[bp.document], baseline[bp.related("styles")] = qa.opc.xml_bytes(bd), qa.opc.xml_bytes(bs)
    return baseline, mixed


class NativeStyleOracleControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        BASE.mkdir(parents=True, exist_ok=True)
        cls.folder = Path(tempfile.mkdtemp(prefix="pure-oracle-", dir=BASE))

    def package(self, name, parts):
        file = self.folder / name
        qa.opc.write_package(file, parts)
        return file

    def test_inputs_are_native_only_three_repeated_explicit_stix_formulas(self):
        for kind in qa.KINDS:
            value = qa.inspect_package(qa.Package(parts=qa.fixture_parts(kind)))
            self.assertEqual(value["auxiliaryParts"], {})
            self.assertFalse(value["sourcePayloadUsedAsNativeEvidence"])
            self.assertEqual(len(value["paragraphs"]), 3)
            self.assertEqual([row["mathTexts"] for row in value["paragraphs"]], [["ab"]] * 3)
            for row in value["paragraphs"]:
                self.assertEqual(row["mathObjects"], value["paragraphs"][0]["mathObjects"])
                self.assertEqual([run["sizeHalfPoints"] for run in row["runDeclarations"]], ["24", "24"])

    def test_missing_empty_defaults_and_nonempty_fallback_are_separate(self):
        values = {kind: qa.inspect_package(qa.Package(parts=qa.fixture_parts(kind))) for kind in qa.KINDS}
        self.assertFalse(values["a"]["styleContext"]["paragraphDefaultsPresent"])
        self.assertTrue(values["b"]["styleContext"]["paragraphDefaultsPresent"])
        self.assertEqual(values["b"]["styleContext"]["defaults"]["axes"], {})
        self.assertEqual(values["fallback"]["styleContext"]["defaults"]["axes"], {"left": "2pc", "right": "1.5pi", "jc": "left"})
        self.assertEqual(values["fallback"]["paragraphs"][2]["effective"]["provenance"], {axis: "docDefaults" for axis in ("left", "right", "jc")})

    def test_six_units_empty_ind_explicit_zero_and_default_selected_style_are_native(self):
        a, b = (qa.inspect_package(qa.Package(parts=qa.fixture_parts(kind))) for kind in ("a", "b"))
        self.assertEqual(a["paragraphs"][1]["direct"]["axes"], {"left": "0"})
        self.assertEqual(b["paragraphs"][1]["direct"]["axes"], {})
        self.assertTrue(b["paragraphs"][1]["direct"]["indPresent"])
        for value in (a, b):
            row = value["paragraphs"][2]
            self.assertTrue(row["direct"]["pPrPresent"])
            self.assertFalse(row["direct"]["pStylePresent"])
            self.assertFalse(row["direct"]["indPresent"])
            self.assertEqual(row["effective"]["selectedStyle"], "DefaultMath")
        self.assertEqual([row["effective"]["axes"]["left"] for row in b["paragraphs"]], ["1mm", "0.5in", "2pc"])
        self.assertEqual([row["effective"]["axes"]["right"] for row in b["paragraphs"]], ["1cm", "18.125pt", "1.5pi"])

    def test_positive_synthetic_native_shape_keeps_uncontested_ids_and_binds_default(self):
        for kind in ("a", "b"):
            source = self.package(kind + "-source.docx", qa.fixture_parts(kind))
            native = self.package(kind + "-native-model.docx", native_model_parts(kind))
            value = qa.compare_native(source, native, kind)
            self.assertTrue(value["passed"])
            self.assertEqual([row["bindingSynthesized"] for row in value["bindings"]], [False, False, True])
            self.assertEqual(value["aliasMap"][kind + ":Equation"], "Equation")
            if kind == "b":
                self.assertNotEqual(value["aliasMap"]["b:Normal"], "Normal")
                self.assertNotEqual(value["aliasMap"]["b:ReadMDMath"], "ReadMDMath")

    def test_wrong_native_materialization_cannot_claim_style_source_preservation(self):
        source = self.package("materialized-source.docx", qa.fixture_parts("a"))
        parts = native_model_parts("a")
        package = qa.Package(parts=parts)
        root = package.xml(package.document)
        row = qa.inspect_package(package)["paragraphs"][1]
        pr = list(root.find("w:body", qa.NS))[row["bodyIndex"]].find("w:pPr", qa.NS)
        pr.find("w:pStyle", qa.NS).set(qa.w("val"), "ReadMDMath")
        parts[package.document] = qa.opc.xml_bytes(root)
        with self.assertRaises(AssertionError):
            qa.compare_native(source, self.package("materialized-bad.docx", parts), "a")

    def test_wrong_ancestor_empty_presence_and_raw_unit_are_not_equal_graphs(self):
        source = self.package("graph-source.docx", qa.fixture_parts("b"))
        for index, mode in enumerate(("ancestor", "empty", "raw", "default")):
            parts = native_model_parts("b")
            package = qa.Package(parts=parts)
            root = package.xml(package.related("styles"))
            by_id = {node.get(qa.w("styleId")): node for node in root.findall("w:style", qa.NS)}
            if mode == "ancestor":
                by_id["Equation"].find("w:basedOn", qa.NS).set(qa.w("val"), "Model_B_Normal")
            elif mode == "empty":
                pr = by_id["Model_B_ReadMDMath"].find("w:pPr", qa.NS)
                pr.remove(pr.find("w:ind", qa.NS))
            elif mode == "raw":
                by_id["Model_B_Normal"].find("w:pPr/w:ind", qa.NS).set(qa.w("left"), "36pt")
            else:
                by_id["DefaultMath"].set(qa.w("default"), "1")
            parts[package.related("styles")] = qa.opc.xml_bytes(root)
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                qa.compare_native(source, self.package("wrong-graph-" + str(index) + ".docx", parts), "b")

    def test_combined_forward_reverse_same_ids_have_distinct_stable_aliases(self):
        a, b = self.package("combined-source-a.docx", qa.fixture_parts("a")), self.package("combined-source-b.docx", qa.fixture_parts("b"))
        forward = qa.compare_combined(a, b, self.package("combined-forward-model.docx", combined_model_parts()), False)
        reverse = qa.compare_combined(a, b, self.package("combined-reverse-model.docx", combined_model_parts(True)), True)
        self.assertEqual(forward["aliasMap"], reverse["aliasMap"])
        self.assertNotEqual(forward["aliasMap"]["a:Equation"], forward["aliasMap"]["b:Equation"])

    def test_combined_first_definition_wins_is_rejected(self):
        a, b = self.package("first-wins-a.docx", qa.fixture_parts("a")), self.package("first-wins-b.docx", qa.fixture_parts("b"))
        parts = combined_model_parts()
        package = qa.Package(parts=parts)
        document = package.xml(package.document)
        rows = qa.inspect_package(package)["paragraphs"]
        p = list(document.find("w:body", qa.NS))[rows[4]["bodyIndex"]]
        p.find("w:pPr/w:pStyle", qa.NS).set(qa.w("val"), "Model_A_Equation")
        parts[package.document] = qa.opc.xml_bytes(document)
        with self.assertRaises(AssertionError):
            qa.compare_combined(a, b, self.package("first-wins-bad.docx", parts))

    def test_current_delete_changes_only_member_and_restores_actual_inherited_axis(self):
        before = self.package("current-a-input.docx", native_model_parts("a"))
        digest = qa.opc.digest(before.read_bytes())
        value = qa.mutate_current(before, self.folder / "current-a-output.docx", "delete-direct-left")
        self.assertEqual(value["changedMembers"], ["word/document.xml"])
        self.assertTrue(value["allOtherMembersByteIdentical"])
        self.assertEqual(value["addedMembers"], [])
        row = value["inspection"]["paragraphs"][1]
        self.assertEqual(row["effective"]["axes"]["left"], "0.5in")
        self.assertEqual(row["effective"]["provenance"]["left"], "style:Base")
        self.assertEqual(qa.opc.digest(before.read_bytes()), digest)

    def test_current_actual_aliased_style_changes_only_referencing_unshadowed_formula(self):
        source = self.package("current-b-input.docx", native_model_parts("b"))
        value = qa.mutate_current(source, self.folder / "current-b-output.docx", "style-right")
        self.assertEqual(value["changedMembers"], [qa.Package(source).related("styles")])
        self.assertEqual(value["inspection"]["paragraphs"][1]["effective"]["axes"]["right"], "1mm")
        for i in (0, 2):
            self.assertEqual(value["inspection"]["paragraphs"][i], value["before"]["paragraphs"][i])
        self.assertFalse(value["oldAuxiliaryConflictCertified"])

    def test_current_mutation_cannot_overwrite_or_accept_wrong_starting_axis(self):
        source = self.package("mutation-guard-source.docx", native_model_parts("a"))
        with self.assertRaises(AssertionError):
            qa.mutate_current(source, source, "delete-direct-left")
        with self.assertRaises(AssertionError):
            qa.mutate_current(source, self.folder / "wrong-start.docx", "style-right-unknown")

    def test_strict_ordinary_and_unassociated_control_remain_unchanged(self):
        before, mixed = isolation_models()
        baseline, combined = self.package("isolation-baseline.docx", before), self.package("isolation-mixed.docx", mixed)
        source = self.package("isolation-source-a.docx", qa.fixture_parts("a"))
        value = qa.compare_isolation(baseline, combined, source)
        self.assertTrue(value["missingAndExplicitZeroNotNormalized"])
        self.assertTrue(value["ordinaryBodyAndBareFormulaEqual"])
        self.assertTrue(value["completeDocDefaultsEqual"])
        self.assertTrue(value["finalSectPrEqual"])

    def test_isolation_checks_complete_doc_defaults_including_run_properties_and_presence(self):
        before, original = isolation_models()
        source = self.package("isolation-run-default-source.docx", qa.fixture_parts("a"))
        for parts in (before, original):
            package = qa.Package(parts=parts)
            styles = package.xml(package.related("styles"))
            defaults = styles.find("w:docDefaults", qa.NS)
            rpr = ET.Element(qa.w("rPrDefault"))
            ET.SubElement(ET.SubElement(rpr, qa.w("rPr")), qa.w("color"), {qa.w("val"): "123456"})
            defaults.insert(0, rpr)
            parts[package.related("styles")] = qa.opc.xml_bytes(styles)
        baseline = self.package("isolation-run-default-baseline.docx", before)
        good = self.package("isolation-run-default-good.docx", original)
        self.assertTrue(qa.compare_isolation(baseline, good, source)["completeDocDefaultsEqual"])
        for index, mode in enumerate(("color", "missing-run-default", "empty-run-default", "missing-doc-defaults")):
            parts = copy.deepcopy(original)
            package = qa.Package(parts=parts)
            styles = package.xml(package.related("styles"))
            defaults = styles.find("w:docDefaults", qa.NS)
            rpr = defaults.find("w:rPrDefault", qa.NS)
            if mode == "color":
                rpr.find("w:rPr/w:color", qa.NS).set(qa.w("val"), "654321")
            elif mode == "missing-run-default":
                defaults.remove(rpr)
            elif mode == "empty-run-default":
                rpr.clear()
            else:
                styles.remove(defaults)
            parts[package.related("styles")] = qa.opc.xml_bytes(styles)
            with self.subTest(mode=mode), self.assertRaisesRegex(AssertionError, "Ordinary docDefaults tree changed"):
                qa.compare_isolation(baseline, self.package("isolation-run-default-bad-" + str(index) + ".docx", parts), source)

    def test_isolation_checks_complete_final_section_page_declarations(self):
        before, original = isolation_models()
        baseline = self.package("isolation-section-baseline.docx", before)
        source = self.package("isolation-section-source.docx", qa.fixture_parts("a"))
        for index, mode in enumerate(("page-size", "page-margin", "missing-section")):
            parts = copy.deepcopy(original)
            package = qa.Package(parts=parts)
            document = package.xml(package.document)
            body = document.find("w:body", qa.NS)
            section = body[-1]
            self.assertEqual(section.tag, qa.w("sectPr"))
            if mode == "page-size":
                section.find("w:pgSz", qa.NS).set(qa.w("w"), "12001")
            elif mode == "page-margin":
                section.find("w:pgMar", qa.NS).set(qa.w("left"), "721")
            else:
                body.remove(section)
            parts[package.document] = qa.opc.xml_bytes(document)
            with self.subTest(mode=mode), self.assertRaisesRegex(AssertionError, "Ordinary final sectPr changed"):
                qa.compare_isolation(baseline, self.package("isolation-section-bad-" + str(index) + ".docx", parts), source)

    def test_ordinary_missing_is_not_normalized_to_zero_and_bare_axis_cannot_leak(self):
        before, original = isolation_models()
        baseline = self.package("isolation-negative-baseline.docx", before)
        source = self.package("isolation-negative-source.docx", qa.fixture_parts("a"))
        for index, mode in enumerate(("normal-zero", "body", "bare")):
            parts = copy.deepcopy(original)
            package = qa.Package(parts=parts)
            if mode == "normal-zero":
                styles = package.xml(package.related("styles"))
                normal = next(node for node in styles.findall("w:style", qa.NS) if node.get(qa.w("styleId")) == "Normal")
                ET.SubElement(ET.SubElement(normal, qa.w("pPr")), qa.w("ind"), {qa.w("left"): "0"})
                parts[package.related("styles")] = qa.opc.xml_bytes(styles)
            else:
                document = package.xml(package.document)
                body = document.find("w:body", qa.NS)
                if mode == "body":
                    body[0].find("w:r/w:t", qa.NS).text = "BROKEN_ORDINARY"
                else:
                    ET.SubElement(body[1].find("w:pPr", qa.NS), qa.w("ind"), {qa.w("left"): "0"})
                parts[package.document] = qa.opc.xml_bytes(document)
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                qa.compare_isolation(baseline, self.package("isolation-bad-" + str(index) + ".docx", parts), source)

    def test_unassociated_matching_defaults_fragment_does_not_require_word_context(self):
        source = qa.inspect_package(qa.Package(parts=qa.fixture_parts("a")))
        package = qa.Package(parts=qa.fixture_parts("a"))
        settings = package.xml(package.related("settings")).find("m:mathPr", qa.NS)
        document = package.xml(package.document)
        group = document.find("w:body/w:p/m:oMathPara", qa.NS)
        raw = ET.tostring(settings, encoding="unicode") + ET.tostring(group, encoding="unicode")
        value = qa.inspect_fragment(raw)
        self.assertFalse(value["direct"]["pPrPresent"])
        self.assertEqual(value["effective"]["axes"], {})
        self.assertEqual(value["mathPr"], source["mathPr"])

    def test_native_zip_and_xml_safety_guards_are_unchanged(self):
        for raw in (b'<!DOCTYPE x><x/>', b'<!DOCTYPE x SYSTEM "file:///never"><x/>', '<x/>'.encode('utf-16')):
            with self.assertRaises(AssertionError):
                qa.parse_xml(raw)
        parts = qa.fixture_parts("a")
        parts["../escape"] = b"NO"
        file = self.folder / "unsafe-member.docx"
        with zipfile.ZipFile(file, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, raw in parts.items():
                archive.writestr(name, raw)
        with self.assertRaises(AssertionError):
            qa.Package(file)

    def test_visible_math_oracle_rejects_hidden_or_phantom_selected_fraction(self):
        good = '<math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>'
        qa.logical.inspect_mathml(good)
        for bad in (good.replace('<mfrac>', '<mfrac style="display:none">'), good.replace('<mfrac>', '<mphantom><mfrac>').replace('</mfrac>', '</mfrac></mphantom>')):
            with self.assertRaises(AssertionError):
                qa.logical.inspect_mathml(bad)


if __name__ == "__main__":
    unittest.main(verbosity=2)
