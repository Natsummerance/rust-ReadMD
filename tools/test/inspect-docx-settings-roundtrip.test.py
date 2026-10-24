"""Offline controls for the independent settings oracle; no ReadMD process."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import warnings
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location(
    "docx_settings_oracle", Path(__file__).with_name("inspect-docx-settings-roundtrip.py"))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


class NativeSettingsOracleTests(unittest.TestCase):
    def package(self, kind="native-mixed", parts=None):
        return qa.Package(parts=parts if parts is not None else qa.fixture_parts(kind))

    def test_p06c_three_native_scopes_keep_sparse_inheritance_same_id_stories_and_true_terminators(self):
        value = qa.inspect_package(self.package("p06c-native-mixed-three"))
        self.assertEqual(value["auxiliaryParts"], {})
        scopes = value["sectionScopes"]
        self.assertEqual(value["nativeSections"], 3)
        self.assertEqual([len(row["terminatorPath"]) for row in scopes], [3, 3, 1])
        self.assertEqual([row["notes"]["footnote"]["effective"]["numStart"] for row in scopes], ["7", "0", "3"])
        self.assertEqual([row["notes"]["endnote"]["effective"]["numFmt"] for row in scopes], ["upperLetter", "decimal", "lowerRoman"])
        self.assertIsNone(scopes[2]["notes"]["footnote"]["sparse"])
        self.assertIsNone(scopes[2]["notes"]["endnote"]["sparse"])
        for index, scope in enumerate(scopes):
            self.assertIn(f"P06C_SCOPE_{index:02d}", scope["text"])
            self.assertIn(qa.p06c_bookmark(f"p06c{index}"), [row["name"] for row in scope["bookmarks"]])
        for kind in ("footnote", "endnote"):
            self.assertIn("7", value["notes"][kind]["references"])
            self.assertIn("8", value["notes"][kind]["references"])
        self.assertEqual(scopes[0]["hyperlinks"], [qa.p06c_bookmark("p06c2")])

    def test_p06c_63_native_formats_come_from_the_fixed_schema_and_rotate_all_u16_restart_representatives(self):
        value = qa.inspect_package(self.package("p06c-native-format-matrix"))
        scopes, formats = value["sectionScopes"], qa.p06c_formats()
        self.assertEqual(len(scopes), 63)
        for kind in ("footnote", "endnote"):
            self.assertEqual([scope["notes"][kind]["effective"]["numFmt"] for scope in scopes], formats)
            self.assertEqual({scope["notes"][kind]["effective"]["numStart"] for scope in scopes}, {"0", "1", "27", "12000", "65535"})
            self.assertEqual({scope["notes"][kind]["effective"]["numRestart"] for scope in scopes}, {"continuous", "eachSect", "eachPage"})
        custom = scopes[formats.index("custom")]["notes"]["footnote"]
        self.assertEqual(custom["effective"]["numberPattern"], "A")
        self.assertTrue(custom["sparse"]["children"][0]["formatPresent"])

    def test_p06c_mutations_change_only_current_target_xml_and_never_auxiliary_bytes(self):
        original = qa.fixture_parts("p06c-native-mixed-three")
        original["customXml/owned-source.xml"] = b"<source>sentinel that is never native evidence</source>"
        baseline = qa.inspect_package(self.package(parts=original))["sectionScopes"]
        for mode in ("p06c-section-change", "p06c-section-clear", "p06c-all-clear"):
            changed, report = qa.mutate_parts(original, mode)
            self.assertTrue(report["allOtherPartsByteIdentical"])
            for name, raw in original.items():
                if name not in report["changedParts"]:
                    self.assertEqual(changed[name], raw)
            scopes = qa.inspect_package(self.package(parts=changed))["sectionScopes"]
            if mode == "p06c-section-change":
                self.assertEqual(scopes[0], baseline[0])
                self.assertEqual(scopes[2], baseline[2])
                self.assertEqual(scopes[1]["notes"]["footnote"]["effective"]["numStart"], "65535")
            else:
                for scope in scopes:
                    for kind in ("footnote", "endnote"):
                        props = scope["notes"][kind]["sparse"]
                        self.assertFalse(props and any(name in props["values"] for name in ("numFmt", "numStart", "numRestart")))
                self.assertEqual(scopes[2]["notes"]["endnote"]["effective"]["numFmt"], "lowerRoman")
                self.assertEqual(scopes[2]["notes"]["footnote"]["effective"]["numStart"], "1" if mode == "p06c-all-clear" else "3")

    def test_p06c_numfmt_without_pattern_clears_global_pattern_and_missing_val_is_never_guessed(self):
        parts = qa.fixture_parts("p06c-native-mixed-three")
        settings = qa.parse_xml(parts["word/settings.xml"])
        settings.find("w:footnotePr/w:numFmt", qa.NS).set(qa.q("format"), "GLOBAL_A")
        parts["word/settings.xml"] = qa.xml_bytes(settings)
        scopes = qa.inspect_package(self.package(parts=parts))["sectionScopes"]
        self.assertIsNone(scopes[0]["notes"]["footnote"]["effective"]["numberPattern"])
        self.assertEqual(scopes[2]["notes"]["footnote"]["effective"]["numberPattern"], "GLOBAL_A")
        changed, _ = qa.mutate_parts(parts, "p06c-missing-val")
        scopes = qa.inspect_package(self.package(parts=changed))["sectionScopes"]
        self.assertIsNone(scopes[1]["notes"]["footnote"]["sparse"]["values"]["numFmt"])
        self.assertIsNone(scopes[1]["notes"]["footnote"]["effective"]["numFmt"])

    def test_epub_note_oracle_reads_real_xhtml_and_refuses_lost_targets_and_reverse_links(self):
        source = b'''<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><p><a role="doc-noteref" id="ref1" href="#note1"><sup>AA</sup></a></p><aside role="doc-footnote" id="note1"><a role="doc-backlink" href="#ref1">AA.</a> UNIFORM_BODY_ONE</aside></body></html>'''
        with tempfile.TemporaryDirectory() as directory:
            for label, text, valid in (("valid", source, True), ("missing-target", source.replace(b'href="#note1"', b'href="#missing"'), False), ("missing-back", source.replace(b'role="doc-backlink"', b'role="none"'), False)):
                file = Path(directory) / (label + ".epub")
                qa.write_package(file, {"OEBPS/chapter.xhtml": text})
                if valid:
                    value = qa.inspect_epub_notes(file)
                    self.assertEqual([row["text"] for row in value["references"]], ["AA"])
                    self.assertEqual(len(value["notes"]), 1)
                else:
                    with self.assertRaises(AssertionError):
                        qa.inspect_epub_notes(file)

    def note_parts_with_direct_author_sizes(self):
        parts = qa.fixture_parts("native-caption-mixed")
        part = self.package(parts=parts).related("footnotes", optional=False)
        story = qa.parse_xml(parts[part])
        normal = next(node for node in story if qa.val(node, "type") in (None, "normal"))
        p = normal.find("w:p", qa.NS)
        author = p.find("w:r", qa.NS)
        props = ET.Element(qa.q("rPr"))
        for name in ("sz", "szCs"):
            qa.set_value(props, name, 20)
        author.insert(0, props)
        gap = ET.Element(qa.q("r"))
        ET.SubElement(gap, qa.q("t")).text = " "
        p.insert(1, gap)
        for container, text in ((p, " strong"), (ET.SubElement(p, qa.q("hyperlink"), {qa.q("anchor"): "native-alpha"}), " evidence.")):
            run = ET.SubElement(container, qa.q("r"))
            run.append(copy.deepcopy(props))
            ET.SubElement(run, qa.q("t")).text = text
        # A normal note with native SDT content, not a forged citation binding.
        extra = ET.SubElement(story, qa.q("footnote"), {qa.q("id"): "8"})
        para = ET.SubElement(extra, qa.q("p"))
        ET.SubElement(ET.SubElement(para, qa.q("pPr")), qa.q("pStyle"), {qa.q("val"): "FootnoteText"})
        content = ET.SubElement(ET.SubElement(para, qa.q("sdt")), qa.q("sdtContent"))
        run = ET.SubElement(content, qa.q("r"))
        run.append(copy.deepcopy(props))
        ET.SubElement(run, qa.q("t")).text = "NATIVE_SDT_NOTE_TEXT"
        parts[part] = qa.xml_bytes(story)
        parts["customXml/oracle-only.xml"] = b"<sentinel>unmodified QA metadata</sentinel>"
        return parts, part

    def test_native_only_fixture_has_distinct_stories_same_id_and_repeated_reference(self):
        value = qa.inspect_package(self.package())
        self.assertEqual(value["auxiliaryParts"], {})
        self.assertFalse(value["sourcePayloadUsedAsNativeEvidence"])
        self.assertFalse(value["wordApplicationRoundtripCertified"])
        foot, end = value["notes"]["footnote"], value["notes"]["endnote"]
        self.assertEqual(foot["references"], ["7", "7"])
        self.assertEqual(end["references"], ["7"])
        self.assertEqual(foot["part"], "word/notes/footnote-custom.xml")
        self.assertEqual(end["part"], "word/notes/endnote-custom.xml")
        for story in (foot, end):
            normal = [row for row in story["definitions"] if row["type"] == "normal"]
            self.assertEqual([row["id"] for row in normal], ["7"])
            self.assertIn("Evidence 😀.", normal[0]["text"])

    def test_effective_style_inheritance_and_spacing_units_are_independent(self):
        value = qa.inspect_package(self.package())
        notes = value["notes"]
        foot = next(row for row in notes["footnote"]["definitions"] if row["type"] == "normal")["paragraphs"][0]
        end = next(row for row in notes["endnote"]["definitions"] if row["type"] == "normal")["paragraphs"][0]
        self.assertEqual(foot["properties"]["spacing"], {"line": "210", "lineRule": "exact"})
        self.assertEqual(end["properties"]["spacing"], {"line": "260", "lineRule": "atLeast"})
        self.assertEqual(foot["runs"][0]["properties"]["sz"], "20")
        self.assertEqual(end["runs"][0]["properties"]["sz"], "22")
        self.assertFalse(foot["properties"]["keepLines"])
        self.assertTrue(foot["properties"]["keepNext"])
        self.assertTrue(end["properties"]["keepLines"])

    def test_section_footnote_position_and_document_endnote_position_are_separate(self):
        value = qa.inspect_package(self.package())
        foot, end = value["notes"]["footnote"], value["notes"]["endnote"]
        self.assertEqual(foot["settings"]["values"]["pos"], "pageBottom")
        self.assertEqual(foot["sections"][0]["values"]["pos"], "beneathText")
        self.assertEqual(end["settings"]["values"]["pos"], "docEnd")
        self.assertEqual(end["sections"], [None])

    def test_special_stories_use_referenced_nonstandard_ids_and_actual_content(self):
        notes = qa.inspect_package(self.package())["notes"]
        foot = notes["footnote"]
        self.assertEqual(foot["settings"]["specialIds"], ["41", "42", "43"])
        selected = {row["type"]: row for row in foot["selectedSpecial"]}
        self.assertEqual(selected["separator"]["separatorElements"], [])
        self.assertEqual(selected["continuationSeparator"]["separatorElements"], ["continuationSeparator"])
        self.assertEqual(selected["continuationNotice"]["text"], "NATIVE_CONTINUED <next> & evidence")
        self.assertEqual([row["id"] for row in notes["endnote"]["selectedSpecial"]], ["51", "52"])

    def test_heading_style_numbering_keeps_per_level_text_suffix_and_ordinary_list_separate(self):
        value = qa.inspect_package(self.package())
        alpha, method = [row for row in value["paragraphs"] if row["text"] in ("NATIVE_ALPHA", "NATIVE_METHOD")]
        self.assertEqual(alpha["rule"], {"level": "0", "start": "3", "format": "upperRoman", "text": "Article %1:", "suffix": "tab", "restart": "0", "style": "Heading1"})
        self.assertEqual(method["rule"]["text"], "%1(%2)")
        self.assertEqual(method["rule"]["suffix"], "space")
        self.assertEqual(method["rule"]["start"], "2")
        ordinary = next(row for row in value["paragraphs"] if row["text"] == "ORDINARY_LIST_ONLY")
        self.assertEqual(ordinary["number"]["numId"], "77")
        self.assertEqual(ordinary["rule"]["text"], "%1)")
        self.assertIsNone(ordinary["style"])
        self.assertTrue(any(field["instruction"] == " REF native-alpha \\h " for row in value["paragraphs"] for field in row["fields"]))

    def test_instance_start_override_is_reported_over_abstract_start(self):
        parts = qa.fixture_parts("native-numbering")
        root = qa.parse_xml(parts["word/numbering.xml"])
        instance = next(row for row in root.findall("w:num", qa.NS) if qa.val(row, "numId") == "42")
        level = ET.SubElement(instance, qa.q("lvlOverride"), {qa.q("ilvl"): "0"})
        ET.SubElement(level, qa.q("startOverride"), {qa.q("val"): "8"})
        parts["word/numbering.xml"] = qa.xml_bytes(root)
        value = qa.inspect_package(self.package(parts=parts))
        alpha = next(row for row in value["paragraphs"] if row["text"] == "NATIVE_ALPHA")
        self.assertEqual(alpha["rule"]["start"], "8")

    def test_zero_start_is_not_coerced_to_one(self):
        value = qa.inspect_package(self.package("native-zero-start"))
        alpha = next(row for row in value["paragraphs"] if row["text"] == "NATIVE_ALPHA")
        self.assertEqual(alpha["rule"]["start"], "0")

    def test_native_mutation_changes_settings_and_numbering_without_resigning_source(self):
        parts = qa.fixture_parts()
        # This is an oracle-only unchanged-member control, never a product
        # binding or an accepted fixture for native/provenance restoration.
        parts["customXml/oracle-only.xml"] = b"<sentinel>unchanged source bytes</sentinel>"
        after, report = qa.mutate_parts(parts, "source-native")
        self.assertFalse(report["metadataResigned"])
        self.assertEqual(after["customXml/oracle-only.xml"], parts["customXml/oracle-only.xml"])
        self.assertIn("word/settings.xml", report["changedParts"])
        self.assertIn("word/numbering.xml", report["changedParts"])
        value = qa.inspect_package(self.package(parts=after))
        self.assertEqual(value["notes"]["footnote"]["settings"]["values"]["numStart"], "13")
        self.assertEqual(value["notes"]["footnote"]["sections"][0]["values"]["numStart"], "13")
        self.assertEqual(value["notes"]["endnote"]["settings"]["values"]["numFmt"], "upperRoman")
        alpha = next(row for row in value["paragraphs"] if row["text"] == "NATIVE_ALPHA")
        self.assertEqual((alpha["rule"]["format"], alpha["rule"]["start"], alpha["rule"]["text"]), ("lowerRoman", "5", "Part %1."))

    def test_numbering_clear_changes_real_consumers_and_fields_but_preserves_text_and_auxiliary_bytes(self):
        parts = qa.fixture_parts("native-caption-mixed")
        # An oracle-only sentinel, never a forged product source binding.
        parts["customXml/oracle-only.xml"] = b"<sentinel>unchanged</sentinel>"
        before = qa.inspect_package(self.package(parts=parts))
        after, report = qa.mutate_parts(parts, "numbering-clear")
        value = qa.inspect_package(self.package(parts=after))
        self.assertEqual([p["text"] for p in value["paragraphs"]], [p["text"] for p in before["paragraphs"]])
        headings = [p for p in value["paragraphs"] if p["style"] in ("Heading1", "Heading2")]
        self.assertEqual(len(headings), 2)
        self.assertTrue(all(p["number"]["numId"] == "0" and "rule" not in p for p in headings))
        self.assertTrue(all(not p["fields"] for p in value["paragraphs"]))
        ordinary = next(p for p in value["paragraphs"] if p["text"] == "ORDINARY_LIST_ONLY")
        self.assertEqual(ordinary["rule"]["text"], "%1)")
        self.assertEqual(after["word/numbering.xml"], parts["word/numbering.xml"])
        self.assertEqual(after["customXml/oracle-only.xml"], parts["customXml/oracle-only.xml"])
        self.assertFalse(report["metadataResigned"])
        self.assertGreater(report["nativeClear"]["headingsDisabled"], 0)
        self.assertGreater(report["nativeClear"]["fieldsFlattened"], 0)

    def test_notes_clear_removes_actual_global_and_section_properties_without_changing_story_or_styles(self):
        parts = qa.fixture_parts("native-caption-mixed")
        parts["customXml/oracle-only.xml"] = b"<sentinel>unchanged</sentinel>"
        before = qa.inspect_package(self.package(parts=parts))
        after, report = qa.mutate_parts(parts, "notes-clear")
        value = qa.inspect_package(self.package(parts=after))
        for kind in ("footnote", "endnote"):
            self.assertIsNone(value["notes"][kind]["settings"])
            self.assertTrue(all(section is None for section in value["notes"][kind]["sections"]))
            self.assertEqual(value["notes"][kind]["references"], before["notes"][kind]["references"])
            self.assertEqual(value["notes"][kind]["definitions"], before["notes"][kind]["definitions"])
            self.assertEqual(value["notes"][kind]["selectedSpecial"], [])
            part = value["notes"][kind]["part"]
            self.assertEqual(after[part], parts[part])
        self.assertEqual(after["word/styles.xml"], parts["word/styles.xml"])
        self.assertEqual(after["word/numbering.xml"], parts["word/numbering.xml"])
        self.assertEqual(after["customXml/oracle-only.xml"], parts["customXml/oracle-only.xml"])
        self.assertGreater(report["nativeClear"]["notePropertiesRemoved"], 0)
        self.assertFalse(report["metadataResigned"])

    def test_chapter_parameter_only_and_explicit_cache_refresh_are_distinct_native_mutations(self):
        parts = qa.fixture_parts("native-caption-mixed")
        document = qa.parse_xml(parts["word/document.xml"])
        caption = next(p for p in document.findall("w:body/w:p", qa.NS) if p.find("w:bookmarkStart[@w:name='cap_table']", qa.NS) is not None)
        seq = caption.find("w:fldSimple", qa.NS)
        at = list(caption).index(seq)
        chapter = ET.Element(qa.q("fldSimple"), {qa.q("instr"): " STYLEREF 1 \\s "})
        ET.SubElement(ET.SubElement(chapter, qa.q("r")), qa.q("t")).text = "III"
        separator = ET.Element(qa.q("r"))
        ET.SubElement(separator, qa.q("t")).text = "."
        caption.insert(at, chapter)
        caption.insert(at + 1, separator)
        reference = next(field for field in document.iter(qa.q("fldSimple")) if " REF cap_table " in qa.val(field, "instr"))
        reference.find("w:r/w:t", qa.NS).text = "Table III.4"
        parts["word/document.xml"] = qa.xml_bytes(document)
        parts["customXml/oracle-only.xml"] = b"<sentinel>Unchanged QA metadata bytes</sentinel>"
        raw, _ = qa.mutate_parts(parts, "source-native")
        stale = qa.inspect_package(self.package(parts=raw))
        self.assertTrue(any(field["instruction"] == " STYLEREF 1 \\s " and field["cached"] == "III" for p in stale["paragraphs"] for field in p["fields"]))
        refreshed, report = qa.mutate_parts(parts, "source-native-refreshed")
        value = qa.inspect_package(self.package(parts=refreshed))
        self.assertTrue(any(p["text"] == "Table Part v..4 — NATIVE_CAPTION" for p in value["paragraphs"]))
        self.assertTrue(any(field["instruction"] == " REF cap_table \\h " and field["cached"] == "Table Part v..4" for p in value["paragraphs"] for field in p["fields"]))
        self.assertEqual(refreshed["customXml/oracle-only.xml"], parts["customXml/oracle-only.xml"])
        self.assertEqual(report["fieldCacheRefresh"]["performedBy"], "owned-headless-OPC-edit")
        self.assertFalse(report["fieldCacheRefresh"]["engineRefreshCertified"])

    def test_owned_cache_refresh_keeps_nested_sequence_code_and_result_stages_separate(self):
        root = qa.parse_xml(f'''<w:document xmlns:w="{qa.W}"><w:body><w:p>
<w:bookmarkStart w:id="21" w:name="cap_current"/><w:r><w:t>Table </w:t></w:r>
<w:fldSimple w:instr=" STYLEREF 1 \\s "><w:r><w:t>III</w:t></w:r></w:fldSimple><w:r><w:t>.</w:t></w:r>
<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> = </w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> SEQ ReadMDTable \\s 1 </w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>
<w:r><w:instrText> + 3 \\* ARABIC </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>
<w:r><w:t>4</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:bookmarkEnd w:id="21"/><w:r><w:t> — TITLE</w:t></w:r>
</w:p><w:p><w:fldSimple w:instr=" REF cap_current \\h "><w:r><w:t>Table III.4</w:t></w:r></w:fldSimple></w:p></w:body></w:document>'''.encode())
        caption = root.find("w:body/w:p", qa.NS)
        self.assertEqual(qa.visible(caption), "Table III.4 — TITLE")
        qa.refresh_owned_chapter_caches(root)
        self.assertEqual(qa.visible(caption), "Table Part v..4 — TITLE")
        fields = qa.fields(caption)
        sequence = next(field for field in fields if "SEQ ReadMDTable" in field["instruction"])
        formula = next(field for field in fields if field["instruction"].lstrip().startswith("="))
        self.assertEqual((sequence["cached"], sequence["depth"]), ("1", 1))
        self.assertEqual((formula["cached"], formula["depth"]), ("4", 0))
        self.assertEqual(qa.visible(root.find("w:body/w:p[2]", qa.NS)), "Table Part v..4")
        self.assertEqual(qa.flatten_current_numbering_fields(caption), 3)
        self.assertEqual(qa.visible(caption), "Table Part v..4 — TITLE")
        self.assertEqual(qa.fields(caption), [])
        self.assertIsNotNone(caption.find("w:bookmarkStart[@w:name='cap_current']", qa.NS))

    def test_foreign_namespace_same_local_name_cannot_be_native_numfmt(self):
        parts, report = qa.mutate_parts(qa.fixture_parts(), "foreign-format")
        value = qa.inspect_package(self.package(parts=parts))
        props = value["notes"]["footnote"]["settings"]
        self.assertNotIn("numFmt", props["values"])
        self.assertTrue(any(row["name"] == "{urn:readmd:qa:foreign}numFmt" for row in props["unknown"]))
        self.assertFalse(report["metadataResigned"])
        self.assertIn("NATIVE_ALPHA", [row["text"] for row in value["paragraphs"]])

    def test_unknown_native_value_is_reported_lexically_without_sanitizing(self):
        parts, _ = qa.mutate_parts(qa.fixture_parts(), "unknown-format")
        value = qa.inspect_package(self.package(parts=parts))
        self.assertEqual(value["notes"]["footnote"]["settings"]["values"]["numFmt"], "nativeMadeUp")

    def test_extended_legal_format_remains_distinct_from_a_truly_unknown_value(self):
        original, _ = self.note_parts_with_direct_author_sizes()
        parts, _ = qa.mutate_parts(original, "extended-format-uniform")
        value = qa.inspect_package(self.package(parts=parts))
        self.assertEqual(value["notes"]["footnote"]["settings"]["values"]["numFmt"], "chicago")
        self.assertEqual(value["notes"]["footnote"]["settings"]["values"]["numStart"], "12000")
        note = next(row for row in value["notes"]["footnote"]["definitions"] if row["type"] == "normal")
        self.assertEqual(note["paragraphs"][0]["runs"][0]["properties"]["sz"], "60")
        self.assertEqual(note["paragraphs"][0]["runs"][0]["properties"]["szCs"], "60")
        # The old shallow first-run check above misses authored direct values.
        runs = [run for item in value["notes"]["footnote"]["definitions"] if item["type"] == "normal"
                for p in item["paragraphs"] for run in p["runs"] if run["text"]]
        self.assertTrue(runs)
        self.assertTrue(all(run["properties"].get("sz") == "60" and run["properties"].get("szCs") == "60" for run in runs), runs)

    def test_style_only_extended_format_retains_real_mixed_runs_as_a_negative(self):
        original, part = self.note_parts_with_direct_author_sizes()
        changed, report = qa.mutate_parts(original, "extended-format")
        value = qa.inspect_package(self.package(parts=changed))
        pairs = {(run["properties"].get("sz"), run["properties"].get("szCs")) for note in value["notes"]["footnote"]["definitions"]
                 if note["type"] == "normal" for p in note["paragraphs"] for run in p["runs"] if run["text"]}
        self.assertEqual(pairs, {("20", "20"), ("60", "60")})
        self.assertEqual(changed[part], original[part])
        self.assertFalse(report["metadataResigned"])

    def test_uniform_extended_format_changes_only_normal_footnote_runs_and_keeps_special_endnote_and_auxiliary_bytes(self):
        original, part = self.note_parts_with_direct_author_sizes()
        changed, report = qa.mutate_parts(original, "extended-format-uniform")
        before, after = qa.parse_xml(original[part]), qa.parse_xml(changed[part])
        for old, new in zip(before, after):
            if qa.val(old, "type") not in (None, "normal"):
                self.assertEqual(ET.tostring(new), ET.tostring(old))
        end = self.package(parts=original).related("endnotes", optional=False)
        self.assertEqual(changed[end], original[end])
        self.assertEqual(changed["customXml/oracle-only.xml"], original["customXml/oracle-only.xml"])
        self.assertEqual(qa.visible(after), qa.visible(before))
        self.assertFalse(report["metadataResigned"])
        self.assertGreaterEqual(report["uniformFootnoteText"]["runsUpdated"], 5)

    def test_native_multisection_fixture_keeps_two_real_boundaries_and_distinct_positions(self):
        value = qa.inspect_package(self.package("native-multi-section"))
        self.assertEqual(value["nativeSections"], 2)
        self.assertEqual([row["values"]["pos"] for row in value["notes"]["footnote"]["sections"]], ["beneathText", "pageBottom"])
        self.assertEqual(value["notes"]["footnote"]["references"], ["7", "7", "8"])
        self.assertTrue(any(row["id"] == "8" and "NATIVE_SECOND_SECTION_NOTE" in row["text"] for row in value["notes"]["footnote"]["definitions"]))

    def test_standard_seq_caption_fixture_has_live_ref_matching_current_caption_cache(self):
        value = qa.inspect_package(self.package("native-caption-mixed"))
        caption = next(row for row in value["paragraphs"] if row["text"] == "Table 4 — NATIVE_CAPTION")
        self.assertEqual(caption["fields"], [{"instruction": " SEQ Table \\r 4 \\* ARABIC ", "cached": "4", "kind": "simple", "depth": 0}])
        self.assertEqual(caption["bookmarks"], [{"id": "21", "name": "cap_table"}])
        reference = next(row for row in value["paragraphs"] if row["text"].startswith("NATIVE_FIELD"))
        self.assertEqual(reference["fields"][0]["instruction"], " REF cap_table \\h ")
        self.assertEqual(reference["fields"][0]["cached"], "Table 4")
        document = self.package("native-caption-mixed").xml("word/document.xml")
        body = document.find("w:body", qa.NS)
        index = next(i for i, node in enumerate(body) if node.find("w:bookmarkStart[@w:name='cap_table']", qa.NS) is not None)
        self.assertEqual(body[index - 1].tag, qa.q("tbl"))
        # The stronger original fixture is still generated without rewriting
        # its heading-REF into the accepted caption case.
        original = qa.inspect_package(self.package("native-mixed"))
        self.assertTrue(any(field["instruction"] == " REF native-alpha \\h " for row in original["paragraphs"] for field in row["fields"]))

    def test_heading_reference_open_fixture_uses_a_safe_matching_bookmark_and_keeps_legacy_material(self):
        value = qa.inspect_package(self.package("native-heading-ref-safe"))
        heading = next(row for row in value["paragraphs"] if row["text"] == "NATIVE_ALPHA")
        self.assertIn("native_alpha", [row["name"] for row in heading["bookmarks"]])
        reference = next(row for row in value["paragraphs"] if row["text"].startswith("NATIVE_FIELD"))
        self.assertEqual(reference["fields"][0]["instruction"], " REF native_alpha \\h ")
        self.assertEqual(reference["fields"][0]["cached"], "NATIVE_ALPHA")
        original = qa.inspect_package(self.package("native-numbering"))
        self.assertTrue(any(field["instruction"] == " REF native-alpha \\h " for row in original["paragraphs"] for field in row["fields"]))

    def test_invalid_duplicates_dtd_cycles_and_missing_reference_are_rejected(self):
        with self.assertRaisesRegex(AssertionError, "DTD/entity"):
            qa.parse_xml(b'<!DOCTYPE x [<!ENTITY y "z">]><x>&y;</x>')
        with self.assertRaisesRegex(AssertionError, "escapes"):
            qa.member("word/document.xml", "../../outside.xml")
        for control in ("duplicate-note", "missing-note", "duplicate-property", "style-cycle"):
            parts = qa.fixture_parts()
            if control in ("duplicate-note", "missing-note"):
                root = qa.parse_xml(parts["word/notes/footnote-custom.xml"])
                normal = next(node for node in root if qa.val(node, "id") == "7")
                if control == "duplicate-note":
                    root.append(copy.deepcopy(normal))
                else:
                    root.remove(normal)
                parts["word/notes/footnote-custom.xml"] = qa.xml_bytes(root)
            elif control == "duplicate-property":
                root = qa.parse_xml(parts["word/settings.xml"])
                root.find("w:footnotePr", qa.NS).append(ET.Element(qa.q("numFmt"), {qa.q("val"): "decimal"}))
                parts["word/settings.xml"] = qa.xml_bytes(root)
            else:
                root = qa.parse_xml(parts["word/styles.xml"])
                row = next(row for row in root.findall("w:style", qa.NS) if qa.val(row, "styleId") == "NoteBase")
                ET.SubElement(row, qa.q("basedOn"), {qa.q("val"): "FootnoteText"})
                parts["word/styles.xml"] = qa.xml_bytes(root)
            with self.subTest(control=control), self.assertRaises(AssertionError):
                qa.inspect_package(self.package(parts=parts))

    def test_real_zip_crc_and_duplicate_member_checks_and_exclusive_output(self):
        with tempfile.TemporaryDirectory(prefix="readmd-settings-oracle-") as directory:
            output = Path(directory) / "native.docx"
            qa.write_package(output, qa.fixture_parts())
            self.assertEqual(qa.inspect(output)["notes"]["footnote"]["references"], ["7", "7"])
            with self.assertRaisesRegex(AssertionError, "new"):
                qa.write_package(output, qa.fixture_parts())
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(output, "a") as archive:
                    archive.writestr("word/document.xml", b"<duplicate/>")
            with self.assertRaisesRegex(AssertionError, "Duplicate"):
                qa.inspect(output)

    def test_isolated_numbering_and_note_fixtures_have_no_accidental_other_feature(self):
        numbers = qa.inspect_package(self.package("native-numbering"))
        self.assertEqual(numbers["notes"]["footnote"]["references"], [])
        self.assertIsNone(numbers["notes"]["footnote"]["part"])
        notes = qa.inspect_package(self.package("native-note-mixed"))
        self.assertEqual(notes["numbers"], {})
        self.assertTrue(all(row["number"] is None for row in notes["paragraphs"]))

    def test_safe_multisection_reference_changes_only_two_names_and_retains_legacy(self):
        legacy = qa.fixture_parts("native-multi-section")
        current = qa.fixture_parts("native-multi-section-safe-ref")
        self.assertEqual(set(legacy), set(current))
        self.assertEqual([name for name in legacy if legacy[name] != current[name]], ["word/document.xml"])
        before, after = qa.parse_xml(legacy["word/document.xml"]), qa.parse_xml(current["word/document.xml"])
        differences = []
        for old, new in zip(before.iter(), after.iter(), strict=True):
            self.assertEqual((old.tag, old.text, old.tail), (new.tag, new.text, new.tail))
            if old.attrib != new.attrib:
                differences.append((qa.local(old.tag), dict(old.attrib), dict(new.attrib)))
        self.assertEqual(differences, [
            ("bookmarkStart", {qa.q("id"): "11", qa.q("name"): "native-alpha"}, {qa.q("id"): "11", qa.q("name"): "native_alpha"}),
            ("fldSimple", {qa.q("instr"): " REF native-alpha \\h "}, {qa.q("instr"): " REF native_alpha \\h "})])
        native = qa.inspect_package(self.package(parts=current))
        self.assertEqual(native["nativeSections"], 2)
        self.assertEqual([row["values"]["pos"] for row in native["notes"]["footnote"]["sections"]], ["beneathText", "pageBottom"])
        self.assertEqual(native["notes"]["footnote"]["references"], ["7", "7", "8"])
        self.assertEqual(native["notes"]["endnote"]["references"], ["7"])
        field = next(row for row in native["paragraphs"] if row["fields"])
        self.assertEqual(field["fields"], [{"instruction": " REF native_alpha \\h ", "cached": "NATIVE_ALPHA", "kind": "simple", "depth": 0}])
        self.assertEqual([run["properties"] for run in field["runs"]], [{"sz": "22"}, {"sz": "22"}])
        self.assertNotIn("word/numbering.xml", current)

    def test_safe_multisection_reference_is_an_actual_cli_fixture_choice(self):
        import json
        import subprocess
        import sys
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "native-multi-section-safe-ref.docx"
            child_options = {"creationflags": subprocess.CREATE_NO_WINDOW} if sys.platform == "win32" else {}
            child = subprocess.run([sys.executable, str(Path(qa.__file__).resolve()), "fixture", "--kind", "native-multi-section-safe-ref", "--output", str(path)], capture_output=True, encoding="utf-8", check=True, timeout=30, **child_options)
            self.assertEqual(child.stderr, "")
            report = json.loads(child.stdout)
            self.assertEqual(report["fixture"], "native-multi-section-safe-ref")
            self.assertTrue(report["nativeOnly"])
            self.assertFalse(report["sourcePayloadPresent"])
            self.assertEqual(report["inspection"]["nativeSections"], 2)
            self.assertEqual(report["outputSha256"], qa.digest(path.read_bytes()))


if __name__ == "__main__":
    unittest.main()
