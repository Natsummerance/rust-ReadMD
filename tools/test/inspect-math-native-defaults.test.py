"""Independent synthetic OPC controls, never actual product/export evidence.

Run with --artifacts NEW_DIRECTORY to retain every input, output and child CLI
response. These tests exercise the finite oracle; they do not invoke ReadMD,
Office, a renderer, a font loader or any other publication engine.
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


qa = load("native_defaults_oracle", "inspect-math-native-defaults.py")
models = load("native_defaults_models", "inspect-math-native-runs.test.py")
base, w, m = qa.base, qa.w, qa.m
ARTIFACTS = None
LANGUAGE = {w("val"): "en-US", w("eastAsia"): "zh-CN", w("bidi"): "ar-SA"}
FONTS = {w(axis): "STIX Two Math" for axis in ("ascii", "hAnsi", "eastAsia", "cs")}


def math_paragraphs(document):
    return [node for node in qa.one(document, w("body"))
            if node.tag == w("p") and (node.find(m("oMathPara")) is not None or node.find(m("oMath")) is not None)]


def sites(paragraph):
    return [node for node in paragraph.iter() if node.tag in (m("r"), m("ctrlPr"))]


def complete_parts(parts, inherited=True, no_character=False):
    """Add our own known globals/direct nodes to an old synthetic OPC model."""
    parts = dict(parts)
    package = base.Package(parts=parts)
    styles, document = package.xml(package.related("styles")), package.xml(package.document)
    default = qa.one(qa.one(qa.one(styles, w("docDefaults")), w("rPrDefault")), w("rPr"))
    assert default is not None
    default[:] = [ET.Element(w("rFonts"), FONTS), ET.Element(w("b"), {w("val"): "0"}),
                  ET.Element(w("i"), {w("val"): "0"}), ET.Element(w("color"), {w("val"): "262626"}),
                  ET.Element(w("sz"), {w("val"): "24"}), ET.Element(w("szCs"), {w("val"): "24"}),
                  ET.Element(w("lang"), LANGUAGE)]
    for ordinal, paragraph in enumerate(math_paragraphs(document)):
        for site in sites(paragraph):
            pr = qa.one(site, w("rPr"))
            assert pr is not None
            for name, attributes in (("rFonts", FONTS), ("szCs", {w("val"): "24"}), ("lang", LANGUAGE)):
                old = qa.one(pr, w(name))
                if old is not None:
                    pr.remove(old)
                if not inherited or ordinal != 0:
                    pr.append(ET.Element(w(name), attributes))
            if no_character and ordinal == 0:
                reference = qa.one(pr, w("rStyle"))
                assert reference is not None
                pr.remove(reference)
            order = [w(name) for name in ("rStyle", "rFonts", "b", "i", "color", "sz", "szCs", "lang")]
            pr[:] = sorted(pr, key=lambda node: order.index(node.tag))
    parts[package.document] = qa.opc.xml_bytes(document)
    parts[package.related("styles")] = qa.opc.xml_bytes(styles)
    return parts


def model_parts(prefix="Source_", inherited=True, no_character=False):
    return complete_parts(models.native_model_parts("a", prefix=prefix), inherited, no_character)


def edited(parts, member, mutate):
    package = base.Package(parts=parts)
    actual = package.related("styles") if member == "styles" else package.document
    root = package.xml(actual)
    mutate(root)
    result = dict(parts)
    result[actual] = qa.opc.xml_bytes(root)
    return result


def defaults_node(styles):
    return qa.one(qa.one(qa.one(styles, w("docDefaults")), w("rPrDefault")), w("rPr"))


def with_global_font(parts, family):
    """Author only the synthetic package global, never its direct font shadows."""
    def change(styles):
        fonts = qa.one(defaults_node(styles), w("rFonts"))
        for axis in ("ascii", "hAnsi", "eastAsia", "cs"):
            fonts.set(w(axis), family)
    return edited(parts, "styles", change)


def materialized_parts():
    """Independently spell nine known effective sites as absolute direct values."""
    parts = model_parts()
    known = [
        [(True, True, "30", "336699")] * 3,
        [(True, False, "30", "336699"), (False, True, "24", "AA5500"), (True, True, "30", "336699")],
        [(False, False, "24", "262626")] * 3,
    ]
    def write_document(document):
        for row, expected in zip(math_paragraphs(document), known):
            assert len(sites(row)) == len(expected) == 3
            for site, (bold, italic, size, color) in zip(sites(row), expected):
                old = qa.one(site, w("rPr"))
                index = list(site).index(old)
                pr = ET.Element(w("rPr"))
                for name, attrs in (("rFonts", FONTS), ("b", {w("val"): "1" if bold else "0"}),
                                    ("i", {w("val"): "1" if italic else "0"}), ("color", {w("val"): color}),
                                    ("sz", {w("val"): size}), ("szCs", {w("val"): "24"}), ("lang", LANGUAGE)):
                    ET.SubElement(pr, w(name), attrs)
                site.remove(old)
                site.insert(index, pr)
    parts = edited(parts, "document", write_document)
    def different_globals(styles):
        pr = defaults_node(styles)
        qa.one(pr, w("lang")).set(w("val"), "fr-FR")
        qa.one(pr, w("szCs")).set(w("val"), "28")
        for axis in ("ascii", "hAnsi", "eastAsia", "cs"):
            qa.one(pr, w("rFonts")).set(w(axis), "DejaVu Sans")
    return edited(parts, "styles", different_globals)


class NativeDefaultsOracleTests(unittest.TestCase):
    def setUp(self):
        if ARTIFACTS is None:
            temporary = tempfile.TemporaryDirectory(prefix="readmd-native-defaults-controls-")
            self.addCleanup(temporary.cleanup)
            self.directory = Path(temporary.name)
        else:
            self.directory = ARTIFACTS / self._testMethodName
            self.directory.mkdir(exist_ok=False)

    def write(self, name, parts):
        file = self.directory / name
        qa.opc.write_package(file, parts)
        return file

    def command(self, name, *arguments):
        script = Path(__file__).with_name("inspect-math-native-defaults.py")
        request = [sys.executable, "-I", "-B", str(script), *map(str, arguments)]
        result = subprocess.run(request, capture_output=True, check=False, timeout=20)
        for suffix, data in (("request.json", json.dumps(request, ensure_ascii=True).encode()),
                             ("stdout", result.stdout), ("stderr", result.stderr)):
            with (self.directory / (name + "." + suffix)).open("xb") as stream:
                stream.write(data)
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
        return json.loads(result.stdout.decode("utf-8", errors="strict"))

    def test_01_known_complete_defaults_and_true_three_site_inheritance(self):
        result = qa.inspect(base.Package(parts=model_parts()))
        self.assertFalse(result["auxiliaryParts"] or result["themeParts"])
        self.assertEqual(len(result["paragraphs"]), 3)
        self.assertEqual(result["styles"]["characterDefault"], None)
        self.assertEqual([site["kind"] for site in result["paragraphs"][0]["sites"]], ["control", "run", "run"])
        for paragraph in result["paragraphs"]:
            self.assertEqual(len(paragraph["sites"]), 3)
            for site in paragraph["sites"]:
                self.assertEqual(site["fullEffective"]["other"], {
                    w("rFonts"): qa.canonical(ET.Element(w("rFonts"), FONTS)),
                    w("szCs"): qa.canonical(ET.Element(w("szCs"), {w("val"): "24"})),
                    w("lang"): qa.canonical(ET.Element(w("lang"), LANGUAGE)),
                })
                self.assertEqual({entry["name"] for entry in site["direct"]["otherProperties"]},
                                 set() if paragraph["ordinal"] == 0 else {w("rFonts"), w("lang"), w("szCs")})
        for site in result["paragraphs"][0]["sites"]:
            self.assertEqual(site["fullEffective"]["four"], {"b": True, "i": True, "sz": "30", "color": "336699"})

    def test_02_two_synthetic_native_generations_preserve_aliases_globals_and_sparse_direct(self):
        previous = self.write("source.docx", model_parts())
        for generation in ("First_", "Second_"):
            target = self.write(generation + ".docx", model_parts(prefix=generation))
            proof = qa.compare(previous, target)
            self.assertTrue(proof["nativeInheritanceCertified"] and proof["completeRprDefaultEqual"])
            self.assertFalse(proof["materializedConsumptionCertified"] or proof["wordApplicationCertified"])
            self.assertEqual(len(proof["bindings"]), 9)
            self.assertEqual(len(proof["aliasMap"]), 4)
            self.assertTrue(all(binding["sourceGlobalState"] == "explicit" for binding in proof["bindings"]))
            previous = target

    def test_03_equal_four_axes_do_not_cover_changed_non_four_globals(self):
        source_parts = model_parts()
        original = self.write("source.docx", source_parts)
        for operation in ("lang", "szCs", "rFonts"):
            def mutation(styles, operation=operation):
                node = qa.one(defaults_node(styles), w(operation))
                node.set(w("ascii") if operation == "rFonts" else w("val"),
                         "Another Family" if operation == "rFonts" else "28" if operation == "szCs" else "fr-FR")
            changed_parts = edited(source_parts, "styles", mutation)
            before, after = qa.inspect(base.Package(parts=source_parts)), qa.inspect(base.Package(parts=changed_parts))
            self.assertEqual([[site["fullEffective"]["four"] for site in row["sites"]] for row in before["paragraphs"]],
                             [[site["fullEffective"]["four"] for site in row["sites"]] for row in after["paragraphs"]])
            with self.subTest(operation=operation), self.assertRaisesRegex(AssertionError, "rPrDefault"):
                qa.compare(original, self.write(operation + ".docx", changed_parts))

    def test_04_identical_direct_values_must_not_be_guessed_to_be_inherited(self):
        parts = model_parts(inherited=False)
        source = self.write("all-direct.docx", parts)
        for name in ("lang", "szCs", "rFonts"):
            def mutation(document, name=name):
                pr = qa.one(sites(math_paragraphs(document)[0])[1], w("rPr"))
                pr.remove(qa.one(pr, w(name)))
            changed = edited(parts, "document", mutation)
            a, b = qa.inspect(base.Package(parts=parts)), qa.inspect(base.Package(parts=changed))
            self.assertEqual(a["paragraphs"][0]["sites"][1]["fullEffective"], b["paragraphs"][0]["sites"][1]["fullEffective"])
            with self.subTest(property=name), self.assertRaisesRegex(AssertionError, "sparse direct"):
                qa.compare(source, self.write("deleted-" + name + ".docx", changed))

    def test_05_inherited_run_and_control_cannot_acquire_equal_direct_shadows(self):
        parts = model_parts()
        source = self.write("inherited.docx", parts)
        for ordinal in range(3):
            for name, attributes in (("lang", LANGUAGE), ("szCs", {w("val"): "24"}), ("rFonts", FONTS)):
                def mutation(document, ordinal=ordinal, name=name, attributes=attributes):
                    ET.SubElement(qa.one(sites(math_paragraphs(document)[0])[ordinal], w("rPr")), w(name), attributes)
                changed = edited(parts, "document", mutation)
                a, b = qa.inspect(base.Package(parts=parts)), qa.inspect(base.Package(parts=changed))
                self.assertEqual(a["paragraphs"][0]["sites"][ordinal]["fullEffective"], b["paragraphs"][0]["sites"][ordinal]["fullEffective"])
                with self.subTest(site=ordinal, property=name), self.assertRaisesRegex(AssertionError, "sparse direct"):
                    qa.compare(source, self.write(f"shadow-{ordinal}-{name}.docx", changed))

    def test_06_actual_global_mutations_follow_inheritors_preserve_direct_and_other_members(self):
        original = self.write("input.docx", with_global_font(model_parts(), "Arial"))
        raw = original.read_bytes()
        before = base.Package(original)
        for operation in ("lang", "szCs", "rFonts"):
            target = self.directory / (operation + ".docx")
            proof = qa.mutate(original, target, operation)
            self.assertEqual(proof["changedMembers"], [before.related("styles")])
            self.assertEqual(len(proof["changedSites"]), 3)
            self.assertEqual(len(proof["shadowedSites"]), 6)
            self.assertEqual({site["paragraph"] for site in proof["changedSites"]}, {0})
            self.assertEqual({site["role"] for site in proof["changedSites"]}, {"run", "control"})
            self.assertTrue(proof["allOtherMembersByteIdentical"] and proof["sourceFileUnchanged"])
            self.assertEqual(original.read_bytes(), raw)
            after = base.Package(target)
            if operation == "rFonts":
                inspection = qa.inspect(after)
                for paragraph in inspection["paragraphs"]:
                    for site in paragraph["sites"]:
                        expected = "DejaVu Sans" if paragraph["ordinal"] == 0 else "STIX Two Math"
                        self.assertEqual(site["fullEffective"]["other"][w("rFonts")]["attributes"],
                                         {w(axis): expected for axis in ("ascii", "hAnsi", "eastAsia", "cs")})
            for member in before.parts:
                if member != before.related("styles"):
                    self.assertEqual(before.parts[member], after.parts[member])

    def test_07_no_explicit_or_automatic_character_chain_is_not_manufactured(self):
        a = self.write("source.docx", model_parts(no_character=True))
        b = self.write("native.docx", model_parts("Native_", no_character=True))
        proof = qa.compare(a, b)
        self.assertTrue(proof["passed"])
        inspection = qa.inspect(base.Package(b))
        for site in inspection["paragraphs"][0]["sites"]:
            self.assertEqual(site["characterChain"], [])
            self.assertIsNone(site["automaticSelectedId"])
            self.assertFalse(site["direct"]["rStylePresent"])
        def introduce_default(styles):
            next(node for node in styles.findall(w("style")) if node.get(w("styleId")) == "Native_Accent").set(w("default"), "1")
        contaminated = edited(model_parts("Native_", no_character=True), "styles", introduce_default)
        with self.assertRaisesRegex(AssertionError, "Automatic character"):
            qa.compare(a, self.write("manufactured-default.docx", contaminated))

    def test_08_fixture_derivations_preserve_inventory_all_other_bytes_and_original(self):
        original = self.write("source-native.docx", with_global_font(model_parts(inherited=False), "Arial"))
        raw, before = original.read_bytes(), base.Package(original)
        for kind in qa.KINDS:
            target = self.directory / (kind + ".docx")
            proof = qa.fixture(original, target, kind)
            after = base.Package(target)
            self.assertEqual(set(before.parts), set(after.parts))
            self.assertEqual(original.read_bytes(), raw)
            self.assertEqual(proof["inputSha256"], qa.opc.digest(raw))
            self.assertEqual(proof["outputSha256"], qa.opc.digest(target.read_bytes()))
            self.assertTrue(set(proof["changedMembers"]) <= {before.document, before.related("styles")})
            for member in before.parts:
                if member not in proof["changedMembers"]:
                    self.assertEqual(before.parts[member], after.parts[member])
            if kind == "original":
                self.assertEqual(before.parts, after.parts)
                self.assertEqual(proof["globalAuthorChoices"], [])
            else:
                choice, = proof["globalAuthorChoices"]
                self.assertEqual(choice["action"], "retain-actual-global")
                self.assertEqual(choice["choice"], "Arial")
                self.assertEqual(choice["before"], choice["after"])
                for site in proof["inspection"]["paragraphs"][0]["sites"]:
                    self.assertEqual(site["direct"]["otherProperties"], [])

    def test_09_materialized_consumption_is_distinct_from_native_source_certification(self):
        source = self.write("source.docx", model_parts())
        target = self.write("materialized.docx", materialized_parts())
        proof = qa.compare(source, target, materialized=True)
        self.assertTrue(proof["passed"] and proof["materializedConsumptionCertified"])
        self.assertFalse(proof["nativeInheritanceCertified"] or proof["completeRprDefaultEqual"])
        self.assertEqual(len(proof["bindings"]), 9)
        with self.assertRaisesRegex(AssertionError, "rPrDefault"):
            qa.compare(source, target)
        def corrupt(document):
            qa.one(qa.one(sites(math_paragraphs(document)[2])[1], w("rPr")), w("lang")).set(w("val"), "bad-language")
        with self.assertRaisesRegex(AssertionError, "effective"):
            qa.compare(source, self.write("wrong-materialized.docx", edited(materialized_parts(), "document", corrupt)), materialized=True)

    def test_10_isolation_keeps_complete_globals_template_body_settings_and_section(self):
        baseline, mixed = [complete_parts(parts, inherited=False) for parts in models.isolation_models()]
        baseline_file = self.write("baseline.docx", baseline)
        self.assertTrue(qa.isolation(baseline_file, self.write("mixed.docx", mixed))["ordinaryBodyAndBareMathUnchanged"])
        for variant in ("global", "template", "body", "section", "settings"):
            parts = dict(mixed)
            package = base.Package(parts=parts)
            if variant in ("global", "template"):
                def mutate_style(styles, variant=variant):
                    if variant == "global":
                        qa.one(defaults_node(styles), w("lang")).set(w("val"), "fr-FR")
                    else:
                        normal = next(node for node in styles.findall(w("style")) if node.get(w("styleId")) == "Normal")
                        ET.SubElement(ET.SubElement(normal, w("rPr")), w("b"))
                parts = edited(parts, "styles", mutate_style)
            elif variant in ("body", "section"):
                def mutate_body(document, variant=variant):
                    if variant == "body":
                        next(document.iter(w("t"))).text = "POLLUTED_ORDINARY"
                    else:
                        qa.one(qa.one(qa.one(document, w("body")), w("sectPr")), w("pgMar")).set(w("left"), "1")
                parts = edited(parts, "document", mutate_body)
            else:
                member = package.related("settings")
                settings = package.xml(member)
                qa.one(qa.one(settings, m("mathPr")), m("dispDef")).set(m("val"), "1")
                parts[member] = qa.opc.xml_bytes(settings)
            with self.subTest(variant=variant), self.assertRaises(AssertionError):
                qa.isolation(baseline_file, self.write(variant + ".docx", parts))

    def test_11_namespace_aliases_do_not_change_the_real_default_cascade(self):
        source = self.write("source.docx", model_parts())
        parts = model_parts("Alias_")
        package = base.Package(parts=parts)
        for member in (package.document, package.related("styles")):
            parts[member] = parts[member].replace(b"w:", b"q:").replace(b"xmlns:w=", b"xmlns:q=")
        proof = qa.compare(source, self.write("aliases.docx", parts))
        self.assertTrue(proof["completeRprDefaultEqual"])

    def test_12_no_overwrite_even_for_existing_input_or_output(self):
        source = self.write("source.docx", model_parts(inherited=False))
        raw = source.read_bytes()
        inherited = self.write("inherited.docx", model_parts())
        inherited_raw = inherited.read_bytes()
        target = self.directory / "existing.docx"
        target.write_bytes(b"KEEP_EXISTING_TARGET")
        for invoke in (lambda: qa.fixture(source, target, "inherited"), lambda: qa.mutate(inherited, target, "lang"),
                       lambda: qa.fixture(source, source, "original"), lambda: qa.mutate(inherited, inherited, "szCs")):
            with self.assertRaises(AssertionError):
                invoke()
            self.assertEqual(source.read_bytes(), raw)
            self.assertEqual(inherited.read_bytes(), inherited_raw)
            self.assertEqual(target.read_bytes(), b"KEEP_EXISTING_TARGET")

    def test_13_bad_inputs_do_not_become_native_defaults_evidence(self):
        for case in ("complex-size", "foreign-language", "theme-font", "duplicate-property", "cycle", "dtd", "private", "theme"):
            parts = model_parts()
            package = base.Package(parts=parts)
            if case in ("complex-size", "foreign-language", "theme-font", "duplicate-property", "cycle"):
                def corrupt(styles, case=case):
                    default = defaults_node(styles)
                    if case == "complex-size":
                        qa.one(default, w("szCs")).set(w("val"), "3277")
                    elif case == "foreign-language":
                        qa.one(default, w("lang")).set("{urn:foreign}val", "en-US")
                    elif case == "theme-font":
                        qa.one(default, w("rFonts")).set(w("asciiTheme"), "minorHAnsi")
                    elif case == "duplicate-property":
                        default.append(copy.deepcopy(qa.one(default, w("lang"))))
                    else:
                        identity = next(node for node in styles.findall(w("style")) if node.get(w("styleId")) == "Source_CharBase")
                        ET.SubElement(identity, w("basedOn"), {w("val"): "Source_Accent"})
                parts = edited(parts, "styles", corrupt)
            elif case == "dtd":
                parts[package.document] = b'<!DOCTYPE x [<!ENTITY e "bad">]>' + parts[package.document]
            else:
                parts["customXml/forbidden.xml" if case == "private" else "word/theme/forbidden.xml"] = b"<forbidden/>"
            with self.subTest(case=case), self.assertRaises(AssertionError):
                qa.inspect(base.Package(parts=parts))

    def test_14_actual_member_alias_and_duplicate_zip_entries_are_rejected(self):
        parts = model_parts()
        with self.assertRaises(AssertionError):
            qa.inspect(base.Package(parts={**parts, "WORD/DOCUMENT.XML": parts["word/document.xml"]}))
        file = self.directory / "duplicate.docx"
        with zipfile.ZipFile(file, "x") as archive:
            for name, raw in parts.items():
                archive.writestr(name, raw)
            with self.assertWarns(UserWarning):
                archive.writestr("word/document.xml", parts["word/document.xml"])
        with self.assertRaises(AssertionError):
            qa.inspect(base.Package(file))

    def test_15_cli_outputs_are_real_utf8_json_with_astral_body_and_strict_binding(self):
        parts = model_parts()
        marker = "PURE_CONTROL_中文_\U0001d6c2"
        parts = edited(parts, "document", lambda document: setattr(next(document.iter(w("t"))), "text", marker))
        source = self.write("unicode.docx", parts)
        inspection = self.command("inspect", "inspect", source)
        self.assertIn(marker, json.dumps(inspection, ensure_ascii=False))
        self.assertEqual(inspection, qa.inspect(base.Package(source)))
        receipt = self.command("compare", "compare", "--source", source, "--native", source)
        self.assertEqual(receipt["sourceSha256"], qa.opc.digest(source.read_bytes()))
        derived = self.directory / "derived.docx"
        made = self.command("fixture", "fixture", "--input", source, "--output", derived, "--kind", "original")
        self.assertEqual(made["inspection"], qa.inspect(base.Package(derived)))
        mutated = self.directory / "current.docx"
        changed = self.command("mutate", "mutate", "--input", source, "--output", mutated, "--operation", "lang")
        self.assertEqual(len(changed["changedSites"]), 3)
        target = self.write("materialized.docx", edited(materialized_parts(), "document", lambda document: setattr(next(document.iter(w("t"))), "text", marker)))
        proof = self.command("materialized", "materialized", "--source", source, "--native", target)
        self.assertTrue(proof["materializedConsumptionCertified"])

    def test_16_complete_default_wrapper_attributes_are_not_hidden_by_equal_rpr_children(self):
        parts = model_parts()
        source = self.write("source.docx", parts)
        def wrapper_pollution(styles):
            qa.one(qa.one(styles, w("docDefaults")), w("rPrDefault")).set("{urn:foreign}changed", "yes")
        changed = self.write("wrapper-pollution.docx", edited(parts, "styles", wrapper_pollution))
        with self.assertRaisesRegex(AssertionError, "rPrDefault"):
            qa.compare(source, changed)

    def test_17_actual_c68_shape_records_already_missing_fields_and_new_author_shadows(self):
        # The C68 shape has direct STIX fonts and no direct lang/szCs. This is
        # our independent synthetic construction of that shape, not a product.
        def omit_language_and_complex_size(document):
            for paragraph in math_paragraphs(document):
                for site in sites(paragraph):
                    pr = qa.one(site, w("rPr"))
                    for name in ("lang", "szCs"):
                        pr.remove(qa.one(pr, w(name)))
        parts = edited(with_global_font(model_parts(inherited=False), "Arial"),
                       "document", omit_language_and_complex_size)
        original = self.write("source-shape.docx", parts)
        before = qa.inspect(base.Package(original))
        for kind in ("inherited", "no-character", "lang-mismatch", "szcs-mismatch"):
            proof = qa.fixture(original, self.directory / (kind + ".docx"), kind)
            receipt = proof["explicitFixtureOperations"]
            choice, = proof["globalAuthorChoices"]
            self.assertEqual(choice["before"]["attributes"], {w(axis): "Arial" for axis in ("ascii", "hAnsi", "eastAsia", "cs")})
            self.assertEqual(choice["before"], choice["after"])
            for ordinal in range(3):
                first = [entry for entry in receipt if entry["paragraph"] == 0 and entry["site"] == ordinal]
                font, = [entry for entry in first if entry["property"] == "rFonts"]
                self.assertEqual(font["action"], "remove-actual-direct")
                self.assertEqual(font["before"]["attributes"], FONTS)
                for name in ("lang", "szCs"):
                    missing, = [entry for entry in first if entry["property"] == name]
                    self.assertEqual(missing["action"], "retain-already-inherited")
                    self.assertNotIn("before", missing)
                self.assertEqual(proof["inspection"]["paragraphs"][0]["sites"][ordinal]["fullEffective"]["other"][w("rFonts")]["attributes"],
                                 {w(axis): "Arial" for axis in ("ascii", "hAnsi", "eastAsia", "cs")})
            shadows = [entry for entry in receipt if entry["action"] == "author-new-direct-shadow"]
            self.assertEqual(len(shadows), 12)
            self.assertEqual({entry["paragraph"] for entry in shadows}, {1, 2})
            for old, new in zip(before["paragraphs"][1:], proof["inspection"]["paragraphs"][1:]):
                for left, right in zip(old["sites"], new["sites"]):
                    self.assertEqual(next(entry for entry in left["direct"]["otherProperties"] if entry["name"] == w("rFonts")),
                                     next(entry for entry in right["direct"]["otherProperties"] if entry["name"] == w("rFonts")))

    def test_18_complete_direct_shadows_do_not_make_incompatible_global_fonts_native(self):
        source_parts = model_parts(inherited=False)
        target_parts = with_global_font(source_parts, "Microsoft YaHei")
        before, after = qa.inspect(base.Package(parts=source_parts)), qa.inspect(base.Package(parts=target_parts))
        self.assertNotEqual(before["rPrDefaultTree"], after["rPrDefaultTree"])
        self.assertEqual([[site["fullEffective"] for site in row["sites"]] for row in before["paragraphs"]],
                         [[site["fullEffective"] for site in row["sites"]] for row in after["paragraphs"]])
        source = self.write("stix-source.docx", source_parts)
        target = self.write("yahei-target.docx", target_parts)
        with self.assertRaisesRegex(AssertionError, "Full real rPrDefault differs"):
            qa.compare(source, target)
        consumption = qa.compare(source, target, materialized=True)
        self.assertTrue(consumption["materializedConsumptionCertified"])
        self.assertFalse(consumption["nativeInheritanceCertified"])

    def test_19_incompatible_stix_globals_cannot_be_rewritten_into_a_positive_fixture(self):
        source = self.write("stix-source.docx", model_parts(inherited=False))
        original = source.read_bytes()
        for kind in ("inherited", "no-character", "lang-mismatch", "szcs-mismatch"):
            target = self.directory / (kind + ".docx")
            with self.subTest(kind=kind), self.assertRaisesRegex(AssertionError, "actual Arial package defaults"):
                qa.fixture(source, target, kind)
            self.assertFalse(target.exists())
            self.assertEqual(source.read_bytes(), original)
        target = self.directory / "wrong-current-font.docx"
        with self.assertRaisesRegex(AssertionError, "actual Arial defaults"):
            qa.mutate(source, target, "rFonts")
        self.assertFalse(target.exists())
        self.assertEqual(source.read_bytes(), original)


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    if "--artifacts" in sys.argv:
        index = sys.argv.index("--artifacts")
        ARTIFACTS = Path(sys.argv[index + 1]).resolve()
        del sys.argv[index:index + 2]
        ARTIFACTS.mkdir(parents=False, exist_ok=False)
    program = unittest.main(verbosity=2, failfast=True, exit=False)
    result = program.result
    if ARTIFACTS is not None:
        manifest = [{"path": str(file.relative_to(ARTIFACTS)).replace("\\", "/"),
                     "bytes": file.stat().st_size, "sha256": hashlib.sha256(file.read_bytes()).hexdigest()}
                    for file in sorted(ARTIFACTS.rglob("*")) if file.is_file()]
        report = {"passed": result.wasSuccessful(), "testsRun": result.testsRun,
                  "failures": len(result.failures), "errors": len(result.errors), "skipped": len(result.skipped),
                  "syntheticOpcControlsOnly": True, "actualProductOrEngineInvoked": False, "artifacts": manifest}
        with (ARTIFACTS / "report.json").open("x", encoding="utf-8") as stream:
            json.dump(report, stream, ensure_ascii=True, indent=2)
            stream.write("\n")
    sys.exit(0 if result.wasSuccessful() else 1)
