"""Pure synthetic controls for the native run-source oracle, never product QA."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

_spec = importlib.util.spec_from_file_location(
    "native_runs_oracle", Path(__file__).with_name("inspect-math-native-runs.py"))
qa = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(qa)


def native_model_parts(kind="a", prefix=None):
    """An independent alias-bearing OPC model, not a ReadMD export."""
    parts = qa.fixture_parts(kind)
    package = qa.Package(parts=parts)
    styles, document = package.xml(package.related("styles")), package.xml(package.document)
    prefix = prefix or ("Model_" + kind.upper() + "_")
    mapping = {name: prefix + name for name in ("Base", "Equation", "CharBase", "Accent")}
    for node in styles.findall(qa.w("style")):
        identity = node.get(qa.w("styleId"))
        node.set(qa.w("styleId"), mapping.get(identity, identity))
        parent = qa.one(node, qa.w("basedOn"))
        if parent is not None:
            parent.set(qa.w("val"), mapping[parent.get(qa.w("val"))])
    for node in document.iter():
        if node.tag in (qa.w("pStyle"), qa.w("rStyle")):
            value = node.get(qa.w("val"))
            node.set(qa.w("val"), mapping.get(value, value))
    defaults = qa.one(styles, qa.w("docDefaults"))
    defaults.insert(0, qa.parse_xml(f'<w:rPrDefault xmlns:w="{qa.W}"><w:rPr><w:color w:val="262626"/><w:sz w:val="24"/></w:rPr></w:rPrDefault>'.encode()))
    ET.SubElement(styles, qa.w("style"), {qa.w("type"): "paragraph", qa.w("styleId"): "Normal", qa.w("default"): "1"})
    parts[package.document], parts[package.related("styles")] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
    return parts


def combined_model_parts(reverse=False):
    a, b = native_model_parts("a"), native_model_parts("b")
    ap, bp = qa.Package(parts=a), qa.Package(parts=b)
    parts = dict(a)
    styles = ap.xml(ap.related("styles"))
    definitions = {node.get(qa.w("styleId")): node for node in styles.findall(qa.w("style"))}
    for node in bp.xml(bp.related("styles")).findall(qa.w("style")):
        identity = node.get(qa.w("styleId"))
        if identity in definitions:
            assert qa.canonical(node) == qa.canonical(definitions[identity])
        else:
            styles.append(copy.deepcopy(node))
    document = ap.xml(ap.document)
    body, other = qa.one(document, qa.w("body")), qa.one(bp.xml(bp.document), qa.w("body"))
    left = [copy.deepcopy(node) for node in body if node.tag != qa.w("sectPr")]
    right = [copy.deepcopy(node) for node in other if node.tag != qa.w("sectPr")]
    section = copy.deepcopy(qa.one(body, qa.w("sectPr")))
    body[:] = (right + left if reverse else left + right) + [section]
    parts[ap.document], parts[ap.related("styles")] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
    return parts


def isolation_models():
    mixed = native_model_parts("a")
    package = qa.Package(parts=mixed)
    document, styles = package.xml(package.document), package.xml(package.related("styles"))
    body = qa.one(document, qa.w("body"))
    section = copy.deepcopy(qa.one(body, qa.w("sectPr")))
    imported = [copy.deepcopy(node) for node in body if node.tag != qa.w("sectPr")]
    bare = copy.deepcopy([node for node in body if node.find("m:oMathPara", qa.NS) is not None][2])
    qa.one(qa.one(bare, qa.w("pPr")), qa.w("pStyle")).set(qa.w("val"), "Normal")
    for pr in bare.iter(qa.w("rPr")):
        for node in list(pr.findall(qa.w("rStyle"))):
            pr.remove(node)
    plain = qa.parse_xml(f'<w:p xmlns:w="{qa.W}"><w:pPr><w:pStyle w:val="Normal"/></w:pPr><w:r><w:t>ORDINARY_NATIVE_RUN_CONTROL</w:t></w:r></w:p>'.encode())
    body[:] = [plain, bare, *imported, section]
    mixed[package.document], mixed[package.related("styles")] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
    baseline = dict(mixed)
    baseline_document, baseline_styles = copy.deepcopy(document), copy.deepcopy(styles)
    qa.one(baseline_document, qa.w("body"))[:] = [copy.deepcopy(plain), copy.deepcopy(bare), copy.deepcopy(section)]
    for node in list(baseline_styles.findall(qa.w("style"))):
        if node.get(qa.w("styleId")).startswith("Model_A_"):
            baseline_styles.remove(node)
    baseline[package.document], baseline[package.related("styles")] = qa.opc.xml_bytes(baseline_document), qa.opc.xml_bytes(baseline_styles)
    return baseline, mixed


def fragment_bytes(kind="a", ordinal=0):
    package = qa.Package(parts=qa.fixture_parts(kind))
    document, styles = package.xml(package.document), package.xml(package.related("styles"))
    paragraphs = [node for node in qa.one(document, qa.w("body")) if node.find("m:oMathPara", qa.NS) is not None]
    paragraph = paragraphs[ordinal]
    math_pr = qa.one(package.xml(package.related("settings")), qa.m("mathPr"))
    return b"".join(qa.opc.xml_bytes(node).split(b"?>", 1)[-1].lstrip(b"\n")
                    for node in (styles, qa.one(paragraph, qa.w("pPr")), math_pr, qa.one(paragraph, qa.m("oMathPara"))))


class NativeRunOracleTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="readmd-native-runs-controls-")
        self.base = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)

    def write(self, name, parts):
        file = self.base / name
        qa.opc.write_package(file, parts)
        return file

    def test_native_only_a_b_fixed_cascade_and_sparse_shadows(self):
        for kind, italic, size, color in (("a", True, "30", "336699"), ("b", False, "26", "993366")):
            with self.subTest(kind=kind):
                inspection = qa.inspect_package(qa.Package(parts=qa.fixture_parts(kind)))
                self.assertFalse(inspection["auxiliaryParts"])
                self.assertEqual(len(inspection["paragraphs"]), 3)
                first, second, third = inspection["paragraphs"]
                for site in first["sites"]:
                    self.assertEqual(site["effective"]["axes"], {"b": True, "i": italic, "sz": size, "color": color})
                    self.assertTrue(site["nativeEligible"])
                    self.assertTrue(all(cell["state"] == "missing" for cell in site["direct"]["axes"].values()))
                    self.assertEqual(site["characterChain"], ["CharBase", "Accent"])
                    self.assertEqual(site["direct"]["otherProperties"][0]["attributes"][qa.w("ascii")], "STIX Two Math")
                self.assertFalse(second["sites"][0]["effective"]["axes"]["i"])
                self.assertFalse(second["sites"][1]["effective"]["axes"]["b"])
                self.assertEqual(second["sites"][1]["direct"]["axes"]["b"]["raw"], "off")
                self.assertEqual(second["sites"][1]["effective"]["axes"]["sz"], "24")
                self.assertEqual(second["sites"][1]["effective"]["axes"]["color"], "AA5500")
                for site in third["sites"]:
                    self.assertEqual(site["effective"]["axes"], {"b": False, "i": False, "sz": "24", "color": "262626"})
                self.assertEqual(inspection["mathPr"]["children"][0]["attributes"][qa.m("val")], "0")
                self.assertTrue(all(child["attributes"][qa.m("val")] == "0" for child in inspection["mathPr"]["children"][1:]))

    def test_style_true_true_cancel_false_noop_and_direct_off_absolute(self):
        row = qa.inspect_package(qa.Package(parts=qa.fixture_parts("a")))["paragraphs"][1]
        events = row["sites"][1]["effective"]["trace"]
        paragraph_b = [event for event in events if event["axis"] == "b" and event["origin"].startswith("paragraph:")]
        self.assertEqual([event["after"] for event in paragraph_b], [True, False])
        false_style = [event for event in events if event["axis"] == "b" and event["origin"] == "character:Accent"][0]
        self.assertFalse(false_style["applied"])
        direct = [event for event in events if event["axis"] == "b" and event["origin"] == "direct"][0]
        self.assertTrue(direct["applied"])
        self.assertFalse(direct["after"])

    def test_missing_empty_and_explicit_false_are_distinct(self):
        missing = qa.declaration(None)
        empty = qa.declaration(qa.parse_xml(f'<w:rPr xmlns:w="{qa.W}"/>'.encode()))
        switch = qa.declaration(qa.parse_xml(f'<w:rPr xmlns:w="{qa.W}"><w:b/><w:i w:val="false"/></w:rPr>'.encode()))
        self.assertFalse(missing["rPrPresent"])
        self.assertTrue(empty["rPrPresent"])
        self.assertEqual(switch["axes"]["b"]["state"], "empty")
        self.assertEqual(switch["axes"]["i"]["state"], "explicit")
        self.assertEqual(switch["axes"]["i"]["raw"], "false")
        self.assertNotEqual(missing["tree"], empty["tree"])

    def test_aliases_are_derived_from_actual_paragraph_and_character_graphs(self):
        for kind in ("a", "b"):
            source = self.write(kind + ".docx", qa.fixture_parts(kind))
            native = self.write(kind + "-native.docx", native_model_parts(kind))
            proof = qa.compare_native(source, native, kind)
            self.assertTrue(proof["passed"])
            self.assertEqual(len(proof["aliasMap"]), 4)
            self.assertEqual(proof["aliasMap"][kind + ":Accent"], "Model_" + kind.upper() + "_Accent")
            self.assertTrue(all(site["sparseDirectPreserved"] for row in proof["bindings"] for site in row["sites"]))

    def test_combined_order_stable_aliases_and_both_types_share_one_domain(self):
        a, b = self.write("a.docx", qa.fixture_parts("a")), self.write("b.docx", qa.fixture_parts("b"))
        forward, reverse = self.write("forward.docx", combined_model_parts()), self.write("reverse.docx", combined_model_parts(True))
        proof = qa.compare_combined(a, b, forward, opposite=reverse)
        self.assertTrue(proof["orderStabilityChecked"])
        self.assertEqual(len(set(proof["aliasMap"].values())), 8)

    def test_same_id_different_definition_cannot_be_merged(self):
        parts = combined_model_parts()
        package = qa.Package(parts=parts)
        styles, document = package.xml(package.related("styles")), package.xml(package.document)
        styles.remove(next(node for node in styles.findall(qa.w("style")) if node.get(qa.w("styleId")) == "Model_B_Accent"))
        for node in document.iter(qa.w("rStyle")):
            if node.get(qa.w("val")) == "Model_B_Accent":
                node.set(qa.w("val"), "Model_A_Accent")
        parts[package.document], parts[package.related("styles")] = qa.opc.xml_bytes(document), qa.opc.xml_bytes(styles)
        a, b = self.write("a.docx", qa.fixture_parts("a")), self.write("b.docx", qa.fixture_parts("b"))
        with self.assertRaises(AssertionError):
            qa.compare_combined(a, b, self.write("bad.docx", parts))

    def test_all_effective_direct_materialization_is_not_native_source_proof(self):
        parts = native_model_parts()
        package = qa.Package(parts=parts)
        inspection, document = qa.inspect_package(package), package.xml(package.document)
        sites = [node for paragraph in qa.one(document, qa.w("body"))
                 for node in paragraph.iter() if node.tag in (qa.m("r"), qa.m("ctrlPr"))]
        values = [site["effective"]["axes"] for row in inspection["paragraphs"] for site in row["sites"]]
        for node, axes in zip(sites, values):
            pr = qa.one(node, qa.w("rPr"))
            pr[:] = [child for child in pr if child.tag not in {qa.w(axis) for axis in qa.AXES}]
            for axis, value in axes.items():
                ET.SubElement(pr, qa.w(axis), {qa.w("val"): ("1" if value else "0") if axis in ("b", "i") else value})
        parts[package.document] = qa.opc.xml_bytes(document)
        source, native = self.write("source.docx", qa.fixture_parts("a")), self.write("materialized.docx", parts)
        with self.assertRaisesRegex(AssertionError, "materialized|declarations"):
            qa.compare_native(source, native, "a")

    def test_missing_rstyle_is_not_replaced_by_unused_style_definition(self):
        parts = native_model_parts()
        package = qa.Package(parts=parts)
        document = package.xml(package.document)
        pr = next(document.iter(qa.w("rPr")))
        pr.remove(qa.one(pr, qa.w("rStyle")))
        parts[package.document] = qa.opc.xml_bytes(document)
        with self.assertRaises(AssertionError):
            qa.compare_native(self.write("source.docx", qa.fixture_parts("a")), self.write("unused.docx", parts), "a")

    def test_legitimate_outside_scope_defaults_theme_auto_and_size_report_loss(self):
        for variant, reason in (("defaults", "source-root-run-defaults"), ("theme", "run-color-not-literal-rgb"),
                                ("auto", "run-color-not-literal-rgb"), ("size-range", "run-size-outside-domain")):
            with self.subTest(variant=variant):
                parts = qa.fixture_parts("a", variant)
                inspection = qa.inspect_package(qa.Package(parts=parts))
                losses = {loss for row in inspection["paragraphs"] for site in row["sites"] for loss in site["lossReasons"]}
                self.assertIn(reason, losses)
                with self.assertRaises(AssertionError):
                    qa.eligible_source(inspection)

    def test_selected_wrong_type_missing_parent_cycle_duplicate_are_rejected(self):
        for variant in ("wrong-type", "missing-parent", "cycle", "duplicate-id"):
            with self.subTest(variant=variant), self.assertRaises(AssertionError):
                qa.inspect_package(qa.Package(parts=qa.fixture_parts("a", variant)))

    def test_chain_depth_and_cross_type_collision_reject(self):
        parts = qa.fixture_parts("a")
        package = qa.Package(parts=parts)
        styles = package.xml(package.related("styles"))
        for index in range(129):
            node = ET.SubElement(styles, qa.w("style"), {qa.w("type"): "character", qa.w("styleId"): "Deep" + str(index)})
            if index:
                ET.SubElement(node, qa.w("basedOn"), {qa.w("val"): "Deep" + str(index - 1)})
        accent = next(node for node in styles.findall(qa.w("style")) if node.get(qa.w("styleId")) == "Accent")
        qa.one(accent, qa.w("basedOn")).set(qa.w("val"), "Deep128")
        parts[package.related("styles")] = qa.opc.xml_bytes(styles)
        with self.assertRaisesRegex(AssertionError, "budget"):
            qa.inspect_package(qa.Package(parts=parts))
        parts = qa.fixture_parts("a")
        package = qa.Package(parts=parts)
        styles = package.xml(package.related("styles"))
        node = ET.SubElement(styles, qa.w("style"), {qa.w("type"): "paragraph", qa.w("styleId"): "Accent"})
        parts[package.related("styles")] = qa.opc.xml_bytes(styles)
        with self.assertRaisesRegex(AssertionError, "Duplicate"):
            qa.inspect_package(qa.Package(parts=parts))

    def test_size_bounds_zero_long_padding_and_no_float_admission(self):
        for raw, value in (("1", 1), ("3276", 3276), (" +00030\t\r\n", 30), ("0" * 5000 + "30", 30)):
            self.assertEqual(qa.size_number(raw), (value, None))
        for raw in ("0", "3277", "999999999999999999999", "-1", "1.5", "1pt", ""):
            self.assertIsNone(qa.size_number(raw)[0])
        with self.assertRaisesRegex(AssertionError, "budget"):
            qa.size_number("0" * 65537)

    def test_attribute_whitespace_and_rgb_case_survive_native_graph(self):
        parts = qa.fixture_parts("a")
        package = qa.Package(parts=parts)
        styles = package.xml(package.related("styles"))
        accent = next(node for node in styles.findall(qa.w("style")) if node.get(qa.w("styleId")) == "Accent")
        qa.one(qa.one(accent, qa.w("rPr")), qa.w("sz")).set(qa.w("val"), " +00030\t\r\n")
        qa.one(qa.one(accent, qa.w("rPr")), qa.w("color")).set(qa.w("val"), "aB55cC")
        parts[package.related("styles")] = qa.opc.xml_bytes(styles)
        row = qa.inspect_package(qa.Package(parts=parts))["paragraphs"][0]["sites"][0]
        self.assertEqual(row["effective"]["axes"]["sz"], " +00030\t\r\n")
        self.assertEqual(row["effective"]["axes"]["color"], "aB55cC")
        self.assertEqual(row["effective"]["sizePoints"], 15)
        self.assertIn(b"&#09;", parts[package.related("styles")])
        self.assertIn(b"&#10;", parts[package.related("styles")])
        self.assertIn(b"&#13;", parts[package.related("styles")])

    def test_wrong_namespace_and_prefix_spelling_never_grant_a_style(self):
        raw = fragment_bytes().replace(b"<w:styles", b"<evil:styles", 1).replace(b"</w:styles>", b"</evil:styles>", 1)
        raw = raw.replace(b"<evil:styles", b'<evil:styles xmlns:evil="urn:foreign"', 1)
        with self.assertRaises(AssertionError):
            qa.inspect_fragment(raw)
        parts = native_model_parts()
        package = qa.Package(parts=parts)
        styles = package.xml(package.related("styles"))
        style = next(node for node in styles.findall(qa.w("style")) if node.get(qa.w("styleId")) == "Model_A_Accent")
        style.set(qa.w("styleId"), "ReadMD_TrustedLooking_Accent")
        parts[package.related("styles")] = qa.opc.xml_bytes(styles)
        with self.assertRaises(AssertionError):
            qa.inspect_package(qa.Package(parts=parts))

    def test_isolation_uses_output_baseline_complete_body_defaults_and_section(self):
        baseline, mixed = isolation_models()
        source = self.write("source.docx", qa.fixture_parts("a"))
        proof = qa.compare_isolation(self.write("baseline.docx", baseline), self.write("mixed.docx", mixed), source)
        self.assertTrue(proof["ordinaryBareMathEqual"])
        self.assertTrue(proof["completeDocDefaultsEqual"])
        self.assertTrue(proof["finalSectPrEqual"])

    def test_isolation_rejects_template_defaults_theme_settings_and_section_changes(self):
        baseline, mixed = isolation_models()
        source = self.write("source.docx", qa.fixture_parts("a"))
        base_file = self.write("baseline.docx", baseline)
        for variant in ("template", "defaults", "settings", "section", "ordinary", "extra-style"):
            with self.subTest(variant=variant):
                parts = dict(mixed)
                package = qa.Package(parts=parts)
                if variant in ("template", "defaults", "extra-style"):
                    part = package.related("styles")
                    root = package.xml(part)
                    if variant == "template":
                        node = next(node for node in root.findall(qa.w("style")) if node.get(qa.w("styleId")) == "Normal")
                        ET.SubElement(ET.SubElement(node, qa.w("rPr")), qa.w("b"))
                    elif variant == "defaults":
                        pr = qa.one(qa.one(qa.one(root, qa.w("docDefaults")), qa.w("rPrDefault")), qa.w("rPr"))
                        qa.one(pr, qa.w("color")).set(qa.w("val"), "FFFFFF")
                    else:
                        ET.SubElement(root, qa.w("style"), {qa.w("type"): "character", qa.w("styleId"): "UnprovedUnused"})
                elif variant == "settings":
                    part = package.related("settings")
                    root = package.xml(part)
                    qa.one(qa.one(root, qa.m("mathPr")), qa.m("dispDef")).set(qa.m("val"), "1")
                else:
                    part = package.document
                    root = package.xml(part)
                    body = qa.one(root, qa.w("body"))
                    if variant == "section":
                        qa.one(qa.one(body, qa.w("sectPr")), qa.w("pgMar")).set(qa.w("left"), "1")
                    else:
                        next(body.iter(qa.w("t"))).text = "CHANGED_ORDINARY"
                parts[part] = qa.opc.xml_bytes(root)
                with self.assertRaises(AssertionError):
                    qa.compare_isolation(base_file, self.write(variant + ".docx", parts), source)

    def test_current_style_color_and_size_follow_real_leaf_with_bare_prefix(self):
        _, mixed = isolation_models()
        original = self.write("mixed.docx", mixed)
        before = original.read_bytes()
        for operation in ("style-color", "style-size"):
            with self.subTest(operation=operation):
                target = self.base / (operation + ".docx")
                proof = qa.mutate_current(original, target, operation)
                self.assertEqual(proof["selectedActualCharacterStyle"], "Model_A_Accent")
                self.assertEqual(proof["changedMembers"], [qa.Package(original).related("styles")])
                self.assertTrue(proof["allOtherMembersByteIdentical"])
                self.assertGreater(len(proof["changedSites"]), 0)
                self.assertGreater(len(proof["shadowedSites"]), 0)
                self.assertEqual(original.read_bytes(), before)
                a, b = qa.Package(original).parts, qa.Package(target).parts
                for name in a:
                    if name not in proof["changedMembers"]:
                        self.assertEqual(a[name], b[name])
                # Bare first formula and the fully shadowed third native one
                # retain every final effective axis.
                self.assertEqual(proof["before"]["paragraphs"][0], proof["inspection"]["paragraphs"][0])
                old = proof["before"]["paragraphs"][-1]
                new = proof["inspection"]["paragraphs"][-1]
                self.assertEqual([site["effective"]["axes"] for site in old["sites"]],
                                 [site["effective"]["axes"] for site in new["sites"]])

    def test_current_direct_b_removal_restores_toggle_only_in_second_native_numerator(self):
        for kind in ("a", "b"):
            with self.subTest(kind=kind):
                source = self.write(kind + ".docx", native_model_parts(kind))
                proof = qa.mutate_current(source, self.base / (kind + "-removed.docx"), "delete-direct-b")
                self.assertEqual(proof["changedMembers"], [qa.Package(source).document])
                self.assertEqual(proof["changedSites"], [{"paragraph": 1, "site": 1, "axis": "b", "before": False, "after": True}])
                self.assertTrue(proof["ordinaryEffectiveUnchanged"])
                self.assertEqual(proof["before"]["paragraphs"][0], proof["inspection"]["paragraphs"][0])
                self.assertEqual(proof["before"]["paragraphs"][2], proof["inspection"]["paragraphs"][2])

    def test_mutator_requires_explicit_group_in_two_source_combination(self):
        original = self.write("combined.docx", combined_model_parts())
        with self.assertRaisesRegex(AssertionError, "Ambiguous"):
            qa.mutate_current(original, self.base / "ambiguous.docx", "style-color")
        self.assertFalse((self.base / "ambiguous.docx").exists())
        proof = qa.mutate_current(original, self.base / "selected.docx", "style-color", style_id="Model_B_Accent")
        self.assertEqual(proof["selectedActualCharacterStyle"], "Model_B_Accent")
        self.assertTrue(all(item["paragraph"] >= 3 for item in proof["changedSites"]))

    def test_mutator_never_overwrites_existing_output_or_original(self):
        original = self.write("original.docx", native_model_parts())
        before = original.read_bytes()
        target = self.base / "exists.docx"
        target.write_bytes(b"KEEP_OLD_TARGET")
        with self.assertRaises(AssertionError):
            qa.mutate_current(original, target, "style-color")
        self.assertEqual(target.read_bytes(), b"KEEP_OLD_TARGET")
        self.assertEqual(original.read_bytes(), before)
        with self.assertRaises(AssertionError):
            qa.mutate_current(original, original, "style-size")
        self.assertEqual(original.read_bytes(), before)

    def test_independent_materialization_retains_math_and_all_four_effective_axes(self):
        for kind in ("a", "b"):
            for ordinal in range(3):
                with self.subTest(kind=kind, ordinal=ordinal):
                    source, target = self.base / f"{kind}-{ordinal}.xml", self.base / f"{kind}-{ordinal}-control.xml"
                    raw = fragment_bytes(kind, ordinal)
                    source.write_bytes(raw)
                    proof = qa.materialize_control(source, target)
                    self.assertEqual(source.read_bytes(), raw)
                    self.assertTrue(proof["independentFourAxisCascade"])
                    self.assertEqual(proof["allExistingRunAndControlSites"], 3)
                    before, after = qa.inspect_fragment(raw), qa.inspect_fragment(target.read_bytes())
                    self.assertEqual(before["paragraphs"][0]["mathShape"], after["paragraphs"][0]["mathShape"])
                    self.assertEqual([site["effective"]["axes"] for site in before["paragraphs"][0]["sites"]],
                                     [site["effective"]["axes"] for site in after["paragraphs"][0]["sites"]])
                    self.assertNotIn(b"rStyle", target.read_bytes())
                    self.assertNotIn(b"styles", target.read_bytes())
                    self.assertEqual(before["mathPr"], after["mathPr"])

    def test_materializer_keeps_foreign_semantics_and_rejects_uncovered_other_properties(self):
        raw = fragment_bytes()
        root = qa.parse_xml(f'<qa xmlns:w="{qa.W}" xmlns:m="{qa.M}">'.encode() + raw + b"</qa>")
        fraction = next(root.iter(qa.m("f")))
        fraction.set("{urn:foreign}retained", "yes")
        run = next(root.iter(qa.m("r")))
        math_props = ET.Element(qa.m("rPr"))
        ET.SubElement(math_props, qa.m("sty"), {qa.m("val"): "p"})
        run.insert(0, math_props)
        source = self.base / "foreign.xml"
        source.write_bytes(b"".join(ET.tostring(child, encoding="utf-8") for child in root))
        target = self.base / "foreign-control.xml"
        qa.materialize_control(source, target)
        self.assertIn(b"retained", target.read_bytes())
        self.assertEqual(qa.inspect_fragment(source.read_bytes())["paragraphs"][0]["mathShape"],
                         qa.inspect_fragment(target.read_bytes())["paragraphs"][0]["mathShape"])
        styles = qa.one(root, qa.w("styles"))
        accent = next(node for node in styles.findall(qa.w("style")) if node.get(qa.w("styleId")) == "Accent")
        ET.SubElement(qa.one(accent, qa.w("rPr")), qa.w("spacing"), {qa.w("val"): "20"})
        bad = self.base / "uncovered.xml"
        bad.write_bytes(b"".join(ET.tostring(child, encoding="utf-8") for child in root))
        with self.assertRaisesRegex(AssertionError, "Uncovered"):
            qa.materialize_control(bad, self.base / "must-not-exist.xml")
        self.assertFalse((self.base / "must-not-exist.xml").exists())

    def test_materializer_unsupported_axes_and_existing_output_do_not_destroy_bytes(self):
        source, output = self.base / "source.xml", self.base / "control.xml"
        raw = fragment_bytes()
        source.write_bytes(raw)
        output.write_bytes(b"KEEP")
        with self.assertRaises(AssertionError):
            qa.materialize_control(source, output)
        self.assertEqual(source.read_bytes(), raw)
        self.assertEqual(output.read_bytes(), b"KEEP")
        bad = raw.replace(b'w:val="336699"', b'w:val="auto"')
        source.write_bytes(bad)
        with self.assertRaises(AssertionError):
            qa.materialize_control(source, self.base / "auto-control.xml")
        self.assertEqual(source.read_bytes(), bad)

    def test_zip_duplicate_link_path_and_xml_dtd_are_rejected(self):
        parts = qa.fixture_parts("a")
        for name in ("../escape.xml", "/absolute.xml", "word/../escape.xml", "word\\escape.xml", "word/a:bad.xml"):
            with self.subTest(name=name), self.assertRaises(AssertionError):
                qa.Package(parts={**parts, name: b"bad"})
        linked = self.base / "linked.docx"
        with zipfile.ZipFile(linked, "x") as archive:
            for name, raw in parts.items():
                info = zipfile.ZipInfo(name)
                info.external_attr = (0o120777 << 16)
                archive.writestr(info, raw)
        with self.assertRaises(AssertionError):
            qa.Package(linked)
        bad = dict(parts)
        package = qa.Package(parts=parts)
        bad[package.document] = b'<!DOCTYPE x [<!ENTITY e "bad">]>' + parts[package.document]
        with self.assertRaises(AssertionError):
            qa.inspect_package(qa.Package(parts=bad))

    def test_duplicate_axis_invalid_switch_and_member_case_alias_reject(self):
        for inside in ('<w:b/><w:b w:val="0"/>', '<w:i w:val="FALSE"/>', '<w:sz w:val="24" w:other="x"/>'):
            with self.subTest(inside=inside), self.assertRaises(AssertionError):
                qa.declaration(qa.parse_xml(f'<w:rPr xmlns:w="{qa.W}">{inside}</w:rPr>'.encode()))
        parts = qa.fixture_parts("a")
        with self.assertRaisesRegex(AssertionError, "Ambiguous"):
            qa.Package(parts={**parts, "WORD/DOCUMENT.XML": parts["word/document.xml"]})

    def test_cli_fixture_inspect_fragment_and_control_contracts(self):
        script = Path(__file__).with_name("inspect-math-native-runs.py")
        def run(*arguments):
            result = subprocess.run([sys.executable, "-I", "-B", str(script), *map(str, arguments)],
                                    check=False, capture_output=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
            return json.loads(result.stdout)
        for kind in ("a", "b"):
            file = self.base / (kind + "-cli.docx")
            fixture = run("fixture", "--kind", kind, "--output", file)
            inspection = run("inspect", file)
            self.assertEqual(fixture["inspection"], inspection)
            self.assertEqual(fixture["sha256"], qa.opc.digest(file.read_bytes()))
        fragment = self.base / "cli-source.xml"
        fragment.write_bytes(fragment_bytes())
        inspection = run("fragment", fragment)
        self.assertTrue(inspection["independentXmlReparsed"])
        control = self.base / "cli-control.xml"
        proof = run("materialize-control", "--input", fragment, "--output", control)
        self.assertTrue(proof["passed"])
        self.assertEqual(proof["inputSha256"], qa.opc.digest(fragment.read_bytes()))
        self.assertEqual(proof["outputSha256"], qa.opc.digest(control.read_bytes()))

    def test_readback_control_merges_finite_fonts_languages_and_complex_size(self):
        raw = fragment_bytes()
        root = qa.parse_xml(f'<qa xmlns:w="{qa.W}" xmlns:m="{qa.M}">'.encode() + raw + b"</qa>")
        styles = qa.one(root, qa.w("styles"))
        defaults = qa.one(styles, qa.w("docDefaults"))
        defaults.insert(0, qa.parse_xml(f'<w:rPrDefault xmlns:w="{qa.W}"><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsi="Arial" w:eastAsia="SimSun" w:cs="Arial" w:hint="default"/><w:color w:val="262626"/><w:sz w:val="24"/><w:szCs w:val="24"/><w:lang w:val="en-US" w:eastAsia="zh-CN" w:bidi="ar-SA"/></w:rPr></w:rPrDefault>'.encode()))
        definitions = {node.get(qa.w("styleId")): node for node in styles.findall(qa.w("style"))}
        ET.SubElement(qa.one(definitions["Base"], qa.w("rPr")), qa.w("lang"), {qa.w("val"): "fr-FR"})
        ET.SubElement(qa.one(definitions["Base"], qa.w("rPr")), qa.w("szCs"), {qa.w("val"): "28"})
        ET.SubElement(qa.one(definitions["Accent"], qa.w("rPr")), qa.w("lang"), {qa.w("eastAsia"): "ja-JP"})
        ET.SubElement(qa.one(definitions["Accent"], qa.w("rPr")), qa.w("szCs"), {qa.w("val"): "32"})
        control = next(root.iter(qa.m("ctrlPr")))
        ET.SubElement(qa.one(control, qa.w("rPr")), qa.w("lang"), {qa.w("bidi"): "he-IL"})
        ET.SubElement(qa.one(control, qa.w("rPr")), qa.w("szCs"), {qa.w("val"): " +0034\t"})
        source, output = self.base / "readback.xml", self.base / "readback-control.xml"
        source.write_bytes(b"".join(ET.tostring(child, encoding="utf-8") for child in root))
        original = source.read_bytes()
        proof = qa.materialize_control(source, output)
        self.assertTrue(proof["otherRunPropertiesIndependentlyMaterialized"])
        self.assertEqual(source.read_bytes(), original)
        for index, site in enumerate(proof["inspection"]["paragraphs"][0]["sites"]):
            extras = {entry["name"]: entry for entry in site["direct"]["otherProperties"]}
            fonts = extras[qa.w("rFonts")]["attributes"]
            self.assertEqual(fonts, {qa.w(axis): "STIX Two Math" for axis in ("ascii", "hAnsi", "eastAsia", "cs")} | {qa.w("hint"): "default"})
            self.assertEqual(extras[qa.w("lang")]["attributes"],
                             {qa.w("val"): "fr-FR", qa.w("eastAsia"): "ja-JP", qa.w("bidi"): "he-IL" if index == 0 else "ar-SA"})
            self.assertEqual(extras[qa.w("szCs")]["attributes"][qa.w("val")], " +0034\t" if index == 0 else "32")
            self.assertEqual(site["effective"]["axes"], {"b": True, "i": True, "sz": "30", "color": "336699"})

    def test_control_rejects_residual_theme_font_bad_language_and_complex_size(self):
        for case in ("theme-font", "language-foreign", "complex-size"):
            with self.subTest(case=case):
                raw = fragment_bytes()
                root = qa.parse_xml(f'<qa xmlns:w="{qa.W}" xmlns:m="{qa.M}">'.encode() + raw + b"</qa>")
                if case == "theme-font":
                    for fonts in root.iter(qa.w("rFonts")):
                        fonts.attrib.pop(qa.w("ascii"), None)
                        fonts.set(qa.w("asciiTheme"), "majorHAnsi")
                elif case == "language-foreign":
                    ET.SubElement(qa.one(next(root.iter(qa.m("r"))), qa.w("rPr")), qa.w("lang"), {"{urn:foreign}val": "en-US"})
                else:
                    ET.SubElement(qa.one(next(root.iter(qa.m("r"))), qa.w("rPr")), qa.w("szCs"), {qa.w("val"): "3277"})
                source, output = self.base / (case + ".xml"), self.base / (case + "-control.xml")
                source.write_bytes(b"".join(ET.tostring(child, encoding="utf-8") for child in root))
                with self.assertRaises(AssertionError):
                    qa.materialize_control(source, output)
                self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
