"""Pure controls for the M07/P06 native oracle; no application or host starts."""
import copy
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location("math_section_oracle", Path(__file__).with_name("inspect-math-section-roundtrip.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


class MathSectionOracleTests(unittest.TestCase):
    def inspect(self, parts):
        return qa.inspect_package(qa.opc.Package(parts=parts))

    def test_native_sections_have_actual_boundaries_not_heading_guesses(self):
        value = self.inspect(qa.section_fixture())
        self.assertEqual([s["effectivePlacement"] for s in value["sections"]], ["beneathText", "pageBottom"])
        self.assertEqual(value["auxiliaryParts"], {})
        self.assertFalse(value["sourcePayloadUsedAsNativeEvidence"])
        self.assertFalse(value["applicationRoundtripCertified"])
        self.assertTrue(value["sections"][0]["terminatorPath"] != value["sections"][1]["terminatorPath"])
        self.assertEqual([s["paragraphs"][0]["text"] for s in value["sections"]], ["SECTION_SAME_HEADING"] * 2)
        self.assertIn("SECTION_A_CODE_END", [p["text"] for p in value["sections"][0]["paragraphs"]])

    def test_shared_numeric_ids_do_not_merge_footnotes_and_endnotes(self):
        value = self.inspect(qa.section_fixture())
        refs = [r for s in value["sections"] for p in s["paragraphs"] for r in p["references"]]
        self.assertEqual([(r["kind"], r["id"]) for r in refs], [("footnote", "7"), ("footnote", "7"), ("endnote", "7"), ("footnote", "8"), ("footnote", "8"), ("endnote", "7")])
        self.assertIn("NATIVE_FOOT_BODY", refs[0]["text"])
        self.assertIn("NATIVE_END_BODY", refs[2]["text"])

    def test_links_resolve_the_actual_story_relationship(self):
        value = self.inspect(qa.section_fixture())
        links = [r for s in value["sections"] for p in s["paragraphs"] for r in p["links"]]
        self.assertEqual(links, [{"text": "SECTION_EVIDENCE", "target": "https://example.test/section-evidence"}] * 2)

    def test_only_one_current_section_changes_and_aux_is_byte_identical(self):
        parts = qa.section_fixture()
        parts["customXml/owned-source.xml"] = b"<source>stale beneathText never authoritative</source>"
        output, report = qa.mutate_parts(parts, "section-toggle", 0)
        self.assertEqual(report["changedParts"], ["word/document.xml"])
        self.assertEqual(output["customXml/owned-source.xml"], parts["customXml/owned-source.xml"])
        before, after = self.inspect(parts), report["inspection"]
        self.assertEqual(after["sections"][1], before["sections"][1])
        self.assertEqual(after["sections"][0]["effectivePlacement"], "pageBottom")
        self.assertEqual(after["notes"], before["notes"])

    def test_missing_section_override_does_not_inherit_previous_section(self):
        parts = qa.section_fixture()
        root = qa.opc.parse_xml(parts["word/document.xml"])
        second = list(root.iter(qa.q("sectPr")))[1]
        second.remove(second.find("w:footnotePr", qa.NS))
        parts["word/document.xml"] = qa.xml_bytes(root)
        self.assertEqual([s["effectivePlacement"] for s in self.inspect(parts)["sections"]], ["beneathText", "pageBottom"])

    def test_empty_section_is_observed(self):
        value = self.inspect(qa.section_fixture(True))
        self.assertEqual(len(value["sections"]), 3)
        self.assertEqual(value["sections"][1]["effectivePlacement"], "pageBottom")
        self.assertEqual([p["text"] for p in value["sections"][1]["paragraphs"]], [""])

    def test_unresolved_hyperlink_cannot_supply_evidence(self):
        parts = qa.section_fixture()
        root = qa.opc.parse_xml(parts["word/_rels/document.xml.rels"])
        root[:] = [node for node in root if node.get("Id") != "qaLink"]
        parts["word/_rels/document.xml.rels"] = qa.xml_bytes(root)
        with self.assertRaisesRegex(AssertionError, "hyperlink"):
            self.inspect(parts)

    def test_unresolved_story_id_is_rejected(self):
        parts = qa.section_fixture()
        root = qa.opc.parse_xml(parts["word/document.xml"])
        next(root.iter(qa.q("endnoteReference"))).set(qa.q("id"), "8")
        parts["word/document.xml"] = qa.xml_bytes(root)
        with self.assertRaisesRegex(AssertionError, "story-specific"):
            self.inspect(parts)

    def test_foreign_pos_cannot_override_current_native_placement(self):
        parts = qa.section_fixture()
        root = qa.opc.parse_xml(parts["word/document.xml"])
        first = next(root.iter(qa.q("sectPr")))
        pos = first.find("w:footnotePr/w:pos", qa.NS)
        pos.tag = "{urn:foreign}pos"
        parts["word/document.xml"] = qa.xml_bytes(root)
        self.assertEqual(self.inspect(parts)["sections"][0]["effectivePlacement"], "pageBottom")

    def test_three_math_states_are_separate_with_correct_effective_values(self):
        missing = self.inspect(qa.math_fixture("missing"))
        empty = self.inspect(qa.math_fixture("empty"))
        off = self.inspect(qa.math_fixture("off"))
        self.assertNotIn("smallFrac", missing["mathProperties"])
        self.assertEqual(empty["mathProperties"]["smallFrac"], {"state": "empty", "value": None})
        self.assertEqual(off["mathProperties"]["smallFrac"], {"state": "explicit", "value": "0"})
        self.assertEqual([row["mathEffective"]["smallFrac"] for row in (missing, empty, off)], [False, True, False])
        self.assertEqual([row["mathEffective"]["lMargin"] for row in (missing, empty, off)], [0, 1440, 0])
        self.assertEqual([row["mathEffective"]["wrapIndent"] for row in (missing, empty, off)], [1440, 1440, 0])

    def test_explicit_defaults_and_absence_are_semantically_equal(self):
        self.assertEqual(self.inspect(qa.math_fixture("missing"))["mathEffective"], self.inspect(qa.math_fixture("explicit-defaults"))["mathEffective"])

    def test_empty_and_explicit_margin_are_semantically_equal(self):
        self.assertEqual(self.inspect(qa.math_fixture("empty-margin"))["mathEffective"], self.inspect(qa.math_fixture("explicit-margin"))["mathEffective"])

    def test_dispdef_off_masks_five_paragraph_defaults_but_never_spacing_or_smallfrac(self):
        pr = {key: {"state": "explicit", "value": value} for key, value in {"dispDef": "0", "smallFrac": "1", "brkBin": "after", "lMargin": "1440", "wrapRight": "1", "preSp": "120"}.items()}
        effective = qa.effective_math(pr, "left")
        self.assertTrue(effective["smallFrac"])
        self.assertEqual(effective["brkBin"], "after")
        self.assertEqual(effective["preSp"], 120)
        self.assertIsNone(qa.effective_math(pr)["defJc"])
        self.assertEqual((effective["lMargin"], effective["wrapIndent"], effective["wrapRight"], effective["defJc"]), (0, 0, False, "left"))

    def test_native_local_alignment_wins_over_fragment_default(self):
        value = self.inspect(qa.math_fixture("left", local_alignment="right"))
        self.assertEqual(value["mathEffective"]["defJc"], "left")
        self.assertEqual(value["formulas"][0]["effective"]["defJc"], "right")

    def test_formula_native_structure_and_limit_defaults_are_observed(self):
        value = self.inspect(qa.math_fixture("left"))
        self.assertEqual(len(value["formulas"]), 1)
        row = value["formulas"][0]
        self.assertEqual(row["fractions"], 1)
        self.assertTrue(row["display"])
        self.assertEqual([n["effectiveLimitLocation"] for n in row["naries"]], ["undOvr", "subSup"])

    def test_nested_real_mathpr_cannot_be_treated_as_fragment_defaults(self):
        for raw in (
            '<m:oMath><m:mathPr><m:smallFrac/></m:mathPr><m:r><m:t>x</m:t></m:r></m:oMath>',
            '<m:oMathPara><m:mathPr><m:smallFrac/></m:mathPr><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>',
            '<m:oMath xmlns:a="' + qa.M + '"><a:mathPr><a:smallFrac/></a:mathPr><m:r><m:t>x</m:t></m:r></m:oMath>',
        ):
            with self.assertRaisesRegex(AssertionError, "Nested"):
                qa.inspect_fragment(raw)

    def test_actual_native_body_rejects_document_defaults_nested_in_a_formula(self):
        parts = qa.math_fixture()
        document = qa.opc.parse_xml(parts["word/document.xml"])
        formula = next(document.iter(qa.mq("oMath")))
        defaults = ET.Element(qa.mq("mathPr"))
        ET.SubElement(defaults, qa.mq("smallFrac"))
        formula.insert(0, defaults)
        parts["word/document.xml"] = qa.xml_bytes(document)
        with self.assertRaisesRegex(AssertionError, "Nested"):
            self.inspect(parts)

    def test_native_formula_mode_and_current_visible_text_are_independent_of_capsules(self):
        raw = '<m:oMathPara><m:oMath><m:r><m:t>y</m:t></m:r></m:oMath></m:oMathPara><m:oMath><m:r><m:t>y</m:t></m:r></m:oMath>'
        rows = qa.inspect_fragment(raw)["formulas"]
        self.assertEqual([(row["text"], row["display"]) for row in rows], [("y", True), ("y", False)])
        self.assertTrue(all(row["effective"] == qa.MATH_DEFAULTS for row in rows))

    def test_complete_fragment_tree_preserves_defaults_word_context_and_all_formula_nodes(self):
        raw = '<m:mathPr><m:brkBinSub/><m:smallFrac/><m:defJc/></m:mathPr><w:styles><w:docDefaults><w:rPrDefault><w:rPr><w:b/><w:color w:val="336699"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles><m:oMathPara><m:oMath><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath></m:oMathPara>'
        value = qa.inspect_fragment(raw)
        self.assertIn("semanticTree", value)
        self.assertEqual(len(value["mathObjects"]), 1)
        self.assertEqual(value["mathObjects"][0]["name"], qa.mq("oMathPara"))
        for altered in (raw.replace('m:t>2<', 'm:t>3<'), raw.replace('336699', '336698'), raw.replace('<m:smallFrac/>', '<m:smallFrac m:val="0"/>')):
            changed = qa.inspect_fragment(altered)
            self.assertNotEqual(value["semanticTree"], changed["semanticTree"])
            self.assertNotEqual(value["semanticTreeSha256"], changed["semanticTreeSha256"])

    def test_complete_tree_normalizes_namespace_aliases_and_only_ignorable_whitespace(self):
        first = '<m:oMathPara><m:oMath><m:r><m:t> x </m:t></m:r></m:oMath></m:oMathPara>'
        alias = '<a:oMathPara xmlns:a="' + qa.M + '">\n<a:oMath> <a:r><a:t> x </a:t></a:r></a:oMath>\n</a:oMathPara>'
        left, right = qa.inspect_fragment(first), qa.inspect_fragment(alias)
        self.assertEqual(left["semanticTree"], right["semanticTree"])
        self.assertEqual(left["mathObjects"], right["mathObjects"])
        self.assertNotEqual(left["semanticTree"], qa.inspect_fragment(first.replace('> x <', '>x<'))["semanticTree"])

    def test_native_math_objects_match_the_actual_fragment_tree_without_capsule_evidence(self):
        parts = qa.math_fixture("missing")
        native = self.inspect(parts)
        document = qa.opc.parse_xml(parts["word/document.xml"])
        actual = next(document.iter(qa.mq("oMathPara")))
        fragment = qa.inspect_fragment(ET.tostring(actual, encoding="unicode"))
        self.assertEqual(native["mathObjects"], fragment["mathObjects"])
        self.assertEqual(native["mathProperties"], {})

    def test_fixture_mathpr_order_is_schema_order(self):
        for variant in qa.MATH_VARIANTS:
            parts = qa.math_fixture(variant)
            root = qa.opc.parse_xml(parts["word/settings.xml"])
            pr = root.find("m:mathPr", {"m": qa.M})
            if pr is not None:
                order = [qa.MATH_ORDER.index(qa.opc.local(node.tag)) for node in pr]
                self.assertEqual(order, sorted(order), variant)

    def test_every_positive_fixture_has_only_one_wrap_choice(self):
        for variant in qa.MATH_VARIANTS:
            root = qa.opc.parse_xml(qa.math_fixture(variant)["word/settings.xml"])
            pr = root.find("m:mathPr", {"m": qa.M})
            if pr is not None:
                names = {qa.opc.local(node.tag) for node in pr}
                self.assertFalse({"wrapIndent", "wrapRight"} <= names, variant)

    def test_independent_oracle_rejects_the_illegal_native_wrap_choice(self):
        for attributes in ('', ' m:val="0"'):
            raw = '<m:mathPr><m:wrapIndent m:val="1440"/><m:wrapRight' + attributes + '/></m:mathPr><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>'
            with self.assertRaisesRegex(AssertionError, "Choice"):
                qa.inspect_fragment(raw)

    def test_other_required_properties_do_not_inherit_legacy_empty_exceptions(self):
        for name in ("mathFont", "preSp", "postSp", "interSp", "intraSp"):
            raw = '<m:mathPr><m:' + name + '/></m:mathPr><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>'
            with self.assertRaisesRegex(AssertionError, "required"):
                qa.inspect_fragment(raw)

    def test_wrap_branches_have_separate_absent_empty_off_on_and_zero_material(self):
        rows = {name: self.inspect(qa.math_fixture(name)) for name in ("missing", "wrap-empty", "wrap-off", "wrap-on", "indent-zero")}
        self.assertNotIn("wrapRight", rows["missing"]["mathProperties"])
        self.assertEqual(rows["wrap-empty"]["mathProperties"]["wrapRight"], {"state": "empty", "value": None})
        self.assertEqual(rows["wrap-off"]["mathProperties"]["wrapRight"], {"state": "explicit", "value": "0"})
        self.assertEqual([rows[name]["mathEffective"]["wrapRight"] for name in ("missing", "wrap-empty", "wrap-off", "wrap-on")], [False, True, False, True])
        self.assertEqual(rows["indent-zero"]["mathEffective"]["wrapIndent"], 0)
        for name in ("wrap-empty", "wrap-off", "wrap-on"):
            self.assertNotIn("wrapIndent", rows[name]["mathProperties"])
        self.assertNotIn("wrapRight", rows["indent-zero"]["mathProperties"])

    def test_documented_enum_empty_defaults_are_source_only_and_native_requires_values(self):
        parts = qa.math_fixture("empty")
        before = self.inspect(parts)
        for name, effective in (("brkBinSub", "--"), ("defJc", "centerGroup"), ("intLim", "subSup"), ("naryLim", "undOvr")):
            self.assertEqual(before["mathProperties"][name], {"state": "empty", "value": None})
            self.assertEqual(before["mathEffective"][name], effective)
        with self.assertRaisesRegex(AssertionError, "required"):
            qa.inspect_package(qa.opc.Package(parts=parts), require_explicit_required=True)
        root = qa.opc.parse_xml(parts["word/settings.xml"])
        for node in root.find("m:mathPr", {"m": qa.M}):
            name = qa.opc.local(node.tag)
            if name in qa.LEGACY_IMPLICIT_VALUES and qa.math_val(node) is None:
                node.set(qa.mq("val"), str(qa.EMPTY_DEFAULTS[name]))
        parts["word/settings.xml"] = qa.xml_bytes(root)
        output = qa.inspect_package(qa.opc.Package(parts=parts), require_explicit_required=True)
        self.assertEqual(output["mathEffective"], before["mathEffective"])
        self.assertTrue(output["mathSchema"]["requiredAttributesExplicit"])
        self.assertEqual(output["mathSchema"]["documentedImplicitProperties"], [])

    def test_independent_mathpr_sequence_and_required_set_match_fixed_schema_snapshot(self):
        schema_path = Path(__file__).parents[2] / "docs/design/office-math-coverage.json"
        schema = qa.json.loads(schema_path.read_text(encoding="utf-8"))
        properties = next(row for row in schema["elements"] if row["element"] == "mathPr")
        self.assertEqual(list(qa.MATH_ORDER), properties["children"])
        choices = [row for row in properties["particle"]["Items"] if row.get("Kind") == "Choice"]
        self.assertEqual(len(choices), 1)
        self.assertEqual([row["Name"].rsplit("/m:", 1)[-1] for row in choices[0]["Items"]], ["wrapIndent", "wrapRight"])
        required = {row["element"] for row in schema["elements"] if row["element"] in qa.MATH_ORDER
                    and any(attribute["name"] == "m:val" and attribute["required"] for attribute in row["attributes"])}
        self.assertEqual(required, qa.REQUIRED_MATH_VALUES)

    def test_invalid_choice_opc_is_owned_separate_material_never_a_positive(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "invalid-choice.docx"
            result = qa.main(["fixture", "--kind", "invalid-math-choice", "--output", str(output)])
            self.assertTrue(output.is_file())
            self.assertFalse(result["schemaValid"])
            self.assertTrue(result["inspectionRejected"])
            self.assertNotIn("inspection", result)
            with self.assertRaisesRegex(AssertionError, "Choice"):
                qa.inspect_package(qa.opc.Package(path=output))

    def test_mathpr_sequence_and_attribute_namespace_cannot_be_guessed(self):
        for raw, reason in (("<m:naryLim m:val=\"undOvr\"/><m:intLim m:val=\"subSup\"/>", "sequence"),
                            ("<m:smallFrac val=\"0\"/>", "attribute namespace"),
                            ("<m:smallFrac m:bogus=\"0\"/>", "attribute namespace")):
            with self.assertRaisesRegex(AssertionError, reason):
                qa.inspect_fragment('<m:mathPr>' + raw + '</m:mathPr><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>')

    def test_native_math_edit_does_not_change_formula_or_aux(self):
        parts = qa.math_fixture("missing")
        parts["customXml/source.xml"] = b"<source>old off</source>"
        output, report = qa.mutate_parts(parts, "math-native-small")
        self.assertEqual(output["word/document.xml"], parts["word/document.xml"])
        self.assertTrue(report["inspection"]["mathEffective"]["smallFrac"])
        self.assertTrue(report["auxiliaryUnchanged"])

    def test_native_mixed_size_negative_has_two_actual_glyph_sizes(self):
        output, report = qa.mutate_parts(qa.section_fixture(), "unsupported-mixed-size")
        package = qa.opc.Package(parts=output)
        story = package.xml(package.related("footnotes"))
        sizes = [(qa.opc.visible(run), qa.val(run.find("w:rPr/w:sz", qa.NS))) for run in story.iter(qa.q("r"))]
        self.assertIn((" MIXED_TEN", "20"), sizes)
        self.assertIn((" MIXED_THIRTY", "60"), sizes)
        self.assertEqual(report["changedParts"], ["word/notes/footnote-custom.xml"])

    def test_unknown_and_foreign_native_format_are_distinct_controls(self):
        for mode, needle in (("invalid-note-format", b"nativeMadeUp"), ("foreign-note-format", b"urn:readmd:qa:foreign")):
            output, report = qa.mutate_parts(qa.section_fixture(), mode)
            self.assertIn(needle, output["word/settings.xml"])
            self.assertEqual(report["changedParts"], ["word/settings.xml"])

    def test_fragment_checks_native_xml_not_readable_metadata(self):
        raw = '<m:mathPr><m:smallFrac/></m:mathPr><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>'
        value = qa.inspect_fragment(raw)
        self.assertTrue(value["mathEffective"]["smallFrac"])
        self.assertEqual(value["formulas"][0]["text"], "x")

    def test_foreign_math_property_does_not_impersonate_math_namespace(self):
        value = qa.inspect_fragment('<m:mathPr><x:smallFrac xmlns:x="urn:foreign"/></m:mathPr><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>')
        self.assertNotIn("smallFrac", value["mathProperties"])
        self.assertFalse(value["mathEffective"]["smallFrac"])

    def test_svg_oracle_measures_actual_text_and_wrap_regions(self):
        value = qa.inspect_svg(b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40"><text x="1" y="2" font-size="8">a</text><g data-kind="auto-wrap-line"/></svg>')
        self.assertEqual(value["texts"], [{"text": "a", "x": "1", "y": "2", "size": "8", "transform": None}])
        self.assertEqual(len(value["wrapRegions"]), 1)
        with self.assertRaisesRegex(AssertionError, "glyph"):
            qa.inspect_svg(b'<svg xmlns="http://www.w3.org/2000/svg"><!-- <text>a</text> --></svg>')

    def test_actual_outline_svg_is_not_mistaken_for_text_svg(self):
        value = qa.inspect_svg(b'<svg xmlns="http://www.w3.org/2000/svg" data-math-justification="left"><g data-math-parts="auto-wrap-line" data-math-line-baseline="0"><path d="M0 0L1 1Z" transform="translate(10 20) scale(0.5)"/></g><g data-math-parts="auto-wrap-line" data-math-line-baseline="0"><path d="M0 0L1 1Z" transform="translate(20 20) scale(0.5)"/></g><g data-math-parts="auto-wrap-line" data-math-line-baseline="-1000"><path d="M0 0L1 1Z" transform="translate(0 -1000) scale(0.25)"/></g></svg>')
        self.assertEqual(len(value["glyphPaths"]), 3)
        self.assertEqual(value["glyphPaths"][0]["scaleY"], 0.5)
        self.assertEqual(value["glyphPaths"][2]["scaleX"], 0.25)
        self.assertEqual(value["wrapBaselines"], [-1000.0, 0.0])
        self.assertEqual(value["alignment"], "left")

    def test_zip_write_is_new_only_and_retains_native_fixture(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "owned.docx"
            qa.opc.write_package(target, qa.section_fixture())
            self.assertEqual(len(qa.inspect_package(qa.opc.Package(path=target))["sections"]), 2)
            with self.assertRaisesRegex(AssertionError, "new"):
                qa.opc.write_package(target, qa.math_fixture())

    def test_dtd_is_rejected_without_resolving_entities(self):
        with self.assertRaisesRegex(AssertionError, "DTD"):
            qa.inspect_fragment('<!DOCTYPE a [<!ENTITY x SYSTEM "file:///private">]><m:oMath>&x;</m:oMath>')

    def broker_control(self, folder, text="shutdown\n", forced=False):
        folder = Path(folder)
        for name in ("profile", "documents", "assets", "wire"):
            (folder / name).mkdir()
        binary = folder / "owned-not-executed.exe"
        binary.write_bytes(b"PURE UNIT CONTROL; NOT EXECUTABLE")
        args = SimpleNamespace(binary=binary, profile=folder / "profile", documents=folder / "documents", assets=folder / "assets", out=folder / "wire", port=49152)
        class OwnedChild:
            pid, returncode, killed, signals, waits = 424242, None, False, [], 0
            def poll(self):
                return self.returncode
            def send_signal(self, event):
                self.signals.append(event)
            def wait(self, timeout):
                self.waits += 1
                if forced and self.waits == 1:
                    raise qa.subprocess.TimeoutExpired("owned-control", timeout)
                self.returncode = -1 if self.killed else 0
                return self.returncode
            def kill(self):
                self.killed = True
        child = OwnedChild()
        with mock.patch.object(qa.subprocess, "Popen", return_value=child) as popen, mock.patch.object(qa.sys, "stdin", io.StringIO(text)), mock.patch.object(qa.sys, "stdout", io.StringIO()):
            result = qa.http_broker(args)
        return result, child, popen.call_args

    def test_broker_control_targets_only_its_created_group_and_requires_clean_tail(self):
        with tempfile.TemporaryDirectory() as folder:
            result, child, call = self.broker_control(folder)
            self.assertTrue(result["passed"])
            self.assertFalse(result["forcedStop"])
            self.assertEqual(child.signals, [qa.signal.CTRL_BREAK_EVENT])
            self.assertFalse(child.killed)
            self.assertIn("--no-window", call.args[0])
            self.assertEqual(call.kwargs["creationflags"], qa.subprocess.CREATE_NEW_PROCESS_GROUP)
            self.assertNotIn("--print-token", call.args[0])

    def test_broker_forced_cleanup_can_never_pass_as_normal_exit(self):
        with tempfile.TemporaryDirectory() as folder:
            result, child, _ = self.broker_control(folder, forced=True)
            self.assertFalse(result["passed"])
            self.assertTrue(result["forcedStop"])
            self.assertTrue(child.killed)
            self.assertEqual(result["hostExitCode"], -1)

    def test_broker_unknown_stdin_still_cleans_owned_host_but_is_a_failed_control(self):
        with tempfile.TemporaryDirectory() as folder:
            result, child, _ = self.broker_control(folder, text="not-a-command\n")
            self.assertFalse(result["passed"])
            self.assertFalse(result["forcedStop"])
            self.assertEqual(result["error"], "Broker accepts only shutdown")
            self.assertEqual(child.signals, [qa.signal.CTRL_BREAK_EVENT])


if __name__ == "__main__":
    unittest.main()
