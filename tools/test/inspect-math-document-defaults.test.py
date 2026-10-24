"""Positive and adversarial controls for the independent mixed-math inspector."""
import importlib.util
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("defaults_oracle", Path(__file__).with_name("inspect-math-document-defaults.py"))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

SVG = '<svg xmlns="http://www.w3.org/2000/svg" width=".1em" height=".1em" viewBox="0 0 100 100" style="vertical-align:0em"><g data-math-ink="true" fill="currentColor" transform="scale(1 -1)"><path transform="translate(1 2) scale(1)" d="M0 0L1 1Z"/></g></svg>'


def copy_material(order="ordinary-first", control=False):
    # Independent fixed QA material from the finite source contract, not text
    # obtained by asking the inspector which spans it would accept.
    ordinary = ["{x}^{2}", "{x}^{2}", r"\frac{x}{y}", r"\frac{x}{y}"] if control else ["x^2", "x^2", r"\frac{x}{y}", r"\frac{x}{y}"]
    native = [r"\frac{a}{b}", r"\frac{a}{b}", r"\frac{a}{b}"]
    return ordinary + native if order == "ordinary-first" else native + ordinary


class Controls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="readmd-defaults-oracle-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)

    def svg(self, name, content):
        file = self.directory / name
        file.write_text(content, encoding="utf-8")
        return file

    def test_identical_inline_without_fraction_bar_is_inspected(self):
        a, b = self.svg("a.svg", SVG), self.svg("b.svg", SVG)
        self.assertEqual(oracle.compare_svg(a, b)["allPrimitives"], 1)

    def test_svg_rejects_glyph_path_paint_geometry_and_missing_ink_changes(self):
        a = self.svg("a.svg", SVG)
        for index, content in enumerate((SVG.replace("L1 1Z", "L2 1Z"), SVG.replace("currentColor", "red"),
                                         SVG.replace("translate(1 2)", "translate(2 2)"), SVG.replace("height=\".1em\"", "height=\".2em\""),
                                         SVG.replace("<path transform=\"translate(1 2) scale(1)\" d=\"M0 0L1 1Z\"/>", ""))):
            with self.subTest(index=index), self.assertRaises(AssertionError):
                oracle.compare_svg(a, self.svg(str(index) + ".svg", content))

    def test_svg_rejects_hidden_root_and_ink_moved_to_inert_defs(self):
        a = self.svg("a.svg", SVG)
        for index, content in enumerate((SVG.replace('<svg xmlns=', '<svg opacity="0" xmlns='),
                                         SVG.replace('<g data-math-ink=', '<defs><g data-math-ink=').replace('</g></svg>', '</g></defs></svg>'))):
            with self.subTest(index=index), self.assertRaises(AssertionError):
                oracle.compare_svg(a, self.svg(str(index) + ".svg", content))

    def test_svg_source_owner_identity_does_not_mask_a_changed_ink_path(self):
        a = self.svg("a.svg", SVG.replace('<path ', '<path data-math-owners="source-a" '))
        b = self.svg("b.svg", SVG.replace('<path ', '<path data-math-owners="source-b" '))
        self.assertEqual(oracle.compare_svg(a, b)["ignoredSourceIdentityAttributes"], ["data-math-owners"])
        with self.assertRaises(AssertionError):
            oracle.compare_svg(a, self.svg("changed.svg", b.read_text().replace("L1 1Z", "L2 1Z")))

    def epub(self, name, count=7, padded=5, before="+6pt", mime=b"application/epub+zip", order="ordinary-first", all_x=False, outside_spine=False):
        file = self.directory / name
        ordinary = [("x2", "inline", "msup")] * 2 + [("xy", "block", "mfrac")] * 2
        imported = [("ab", "block", "mfrac")] * 3
        values = ordinary + imported if order == "ordinary-first" else imported + ordinary
        equations, padded_count = [], 0
        for tokens, display, structure in values[:count]:
            second = "mn" if tokens[1].isdigit() else "mi"
            content = f'<m:{structure}><m:mi>{tokens[0]}</m:mi><m:{second}>{tokens[1]}</m:{second}></m:{structure}>'
            if all_x:
                content = f'<m:{structure}><m:mi>x</m:mi><m:mi>x</m:mi></m:{structure}>'
            if display == "block" and padded_count < padded:
                content = f'<m:mpadded height="{before}" depth="+12pt">{content}</m:mpadded>'
                padded_count += 1
            equations.append(f'<m:math display="{display}">' + content + '</m:math>')
        def xhtml(body):
            return '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:m="http://www.w3.org/1998/Math/MathML"><body>' + body + '</body></html>'
        extra = '<item id="orphan" href="orphan.xhtml" media-type="application/xhtml+xml"/>' if outside_spine else ''
        with zipfile.ZipFile(file, "x") as archive:
            archive.writestr("mimetype", mime)
            archive.writestr("META-INF/container.xml", '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="EPUB/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
            archive.writestr("EPUB/content.opf", '<package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>' + extra + '</manifest><spine><itemref idref="chapter"/></spine></package>')
            archive.writestr("EPUB/chapter.xhtml", xhtml('' if outside_spine else "".join(equations)))
            if outside_spine:
                archive.writestr("EPUB/orphan.xhtml", xhtml("".join(equations)))
        return file

    def test_epub_positive_and_adversarial_counts_spacing_and_mimetype(self):
        self.assertTrue(oracle.epub(self.epub("ok.epub"))["passed"])
        for name, options in (("missing.epub", {"count": 6}), ("spacing.epub", {"before": "+7pt"}),
                              ("unpadded.epub", {"padded": 4}), ("mime.epub", {"mime": b"wrong"})):
            with self.subTest(name=name), self.assertRaises(AssertionError):
                oracle.epub(self.epub(name, **options))

    def test_epub_both_orders_require_selected_spine_tokens_structures_and_display(self):
        for order in ("ordinary-first", "native-first"):
            file = self.epub(order + ".epub", order=order)
            self.assertEqual(oracle.epub(file, order)["sourceOrder"], order)
            other = "native-first" if order == "ordinary-first" else "ordinary-first"
            with self.assertRaises(AssertionError):
                oracle.epub(file, other)
        for name, options in (("all-x.epub", {"all_x": True}), ("orphan.epub", {"outside_spine": True})):
            with self.subTest(name=name), self.assertRaises(AssertionError):
                oracle.epub(self.epub(name, **options))

    def pdf(self, name, color=(0, 0, 0), count=5, shift=0, text_shift=0, text_size=11, text_color=(0, 0, 0), text_mode=0, font="helv", body=True,
            hidden_copies=(), hidden_size=7, hidden_split=None, type3_copy=False,
            copy_font_name="ReadMDMathText", copy_font_visible=False, copy_font_alias=False):
        import pymupdf as fitz
        file = self.directory / name
        with fitz.open() as document:
            page = document.new_page(width=612, height=792)
            page.insert_text((20 + text_shift, 20), "PLAIN_CONTROL", fontsize=text_size, color=text_color, render_mode=text_mode, fontname=font)
            if body:
                page.insert_text((20, 35), "PLAIN_BODY must remain unchanged.")
            for index in range(count):
                page.draw_rect(fitz.Rect(20 + index * 20 + shift, 50, 25 + index * 20 + shift, 60),
                               color=None, fill=color if index == 4 else (0, 0, 0))
            if type3_copy:
                # A real owned PDF Type3 resource with no extractable font
                # file, explicit ToUnicode and non-painting glyph programs.
                # This is synthetic oracle material, never product evidence.
                characters = sorted(set("".join(hidden_copies) + "VISIBLE_POLLUTION"))
                codes = {char: index + 1 for index, char in enumerate(characters)}
                glyph = document.get_new_xref()
                document.update_object(glyph, "<<>>")
                document.update_stream(glyph, b"1000 0 0 0 1000 1000 d1\n")
                cmap = ("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n"
                        "/CIDSystemInfo << /Registry (Oracle) /Ordering (Unicode) /Supplement 0 >> def\n"
                        "/CMapName /OracleCopy def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n"
                        + str(len(characters)) + " beginbfchar\n"
                        + "".join(f"<{codes[char]:02X}> <{char.encode('utf-16-be').hex()}>\n" for char in characters)
                        + "endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n")
                unicode = document.get_new_xref()
                document.update_object(unicode, "<<>>")
                document.update_stream(unicode, cmap.encode("ascii"))
                copy_font = document.get_new_xref()
                procedures = " ".join(f"/g{code} {glyph} 0 R" for code in codes.values())
                differences = " ".join(f"/g{code}" for code in codes.values())
                widths = " ".join("1000" for _ in characters)
                document.update_object(copy_font, f"<< /Type /Font /Subtype /Type3 /Name /{copy_font_name} /BaseFont /{copy_font_name}"
                                       f" /FontBBox [0 0 1000 1000] /FontMatrix [.001 0 0 .001 0 0] /CharProcs << {procedures} >>"
                                       f" /Encoding << /Type /Encoding /Differences [1 {differences}] >> /FirstChar 1 /LastChar {len(characters)}"
                                       f" /Widths [{widths}] /ToUnicode {unicode} 0 R /Resources << >> >>")
                def add_copy(target, text, y, render_mode):
                    resource_kind, resource_value = document.xref_get_key(target.xref, "Resources")
                    assert resource_kind == "xref"
                    resources = int(resource_value.split()[0])
                    document.xref_set_key(resources, "Font/RMCOPY", f"{copy_font} 0 R")
                    if copy_font_alias:
                        alias = document.get_new_xref()
                        document.update_object(alias, document.xref_object(copy_font))
                        document.xref_set_key(resources, "Font/RMCOPYALIAS", f"{alias} 0 R")
                    content = document.get_new_xref()
                    document.update_object(content, "<<>>")
                    encoded = "".join(f"{codes[char]:02X}" for char in text)
                    document.update_stream(content, f"q BT /RMCOPY {hidden_size} Tf {render_mode} Tr 1 0 0 1 20 {792 - y} Tm <{encoded}> Tj ET Q\n".encode("ascii"))
                    members = target.get_contents() + [content]
                    document.xref_set_key(target.xref, "Contents", "[" + " ".join(f"{xref} 0 R" for xref in members) + "]")
            second = None
            for index, text in enumerate(hidden_copies):
                if hidden_split is not None and index >= hidden_split:
                    if second is None:
                        second = document.new_page(width=612, height=792)
                        second.insert_text((20, 20), "COPY_LAYER_SECOND_PAGE")
                    target = second
                else:
                    target = page
                if type3_copy:
                    add_copy(target, text, 80 + index * 18, 3)
                else:
                    target.insert_text((20, 80 + index * 18), text, fontsize=hidden_size, render_mode=3)
            if copy_font_visible:
                assert type3_copy
                add_copy(target, "VISIBLE_POLLUTION", 260, 0)
            document.save(file)
        return file

    def test_pdf_complete_path_comparison_and_adversarial_controls(self):
        a = self.pdf("a.pdf")
        self.assertTrue(oracle.pdf(a)["passed"])
        self.assertEqual(oracle.compare_pdf(a, self.pdf("b.pdf"))["allDrawingPaths"], 5)
        for name, options in (("paint.pdf", {"color": (0, 0, 1)}), ("missing.pdf", {"count": 4}),
                              ("geometry.pdf", {"shift": .1})):
            with self.subTest(name=name), self.assertRaises(AssertionError):
                oracle.compare_pdf(a, self.pdf(name, **options))

    def test_pdf_cli_writes_one_json_document_without_library_stdout_noise(self):
        file = self.pdf("cli.pdf")
        result = subprocess.run([sys.executable, "-I", "-B", str(Path(oracle.__file__)), "pdf", str(file)],
                                capture_output=True, check=True, timeout=30)
        value = json.loads(result.stdout)
        self.assertTrue(value["passed"])
        self.assertEqual(value["vectorDrawings"], 5)

    def test_pdf_same_copy_with_moved_resized_recolored_invisible_or_different_font_text_fails(self):
        import pymupdf as fitz
        a = self.pdf("a.pdf")
        for name, options in (("moved.pdf", {"text_shift": .1}), ("size.pdf", {"text_size": 12}),
                              ("color.pdf", {"text_color": (0, 0, 1)}), ("invisible.pdf", {"text_mode": 3}),
                              ("font.pdf", {"font": "cour"})):
            other = self.pdf(name, **options)
            with fitz.open(a) as left, fitz.open(other) as right:
                self.assertEqual(left[0].get_text(), right[0].get_text(), "Control must defeat the old copy-only assertion")
            with self.subTest(name=name), self.assertRaises(AssertionError):
                oracle.compare_pdf(a, other)

    def test_pdf_missing_body_line_fails_even_with_unchanged_title_and_math_paths(self):
        a, b = self.pdf("a.pdf"), self.pdf("b.pdf", body=False)
        self.assertTrue(oracle.pdf(b)["passed"])
        with self.assertRaises(AssertionError):
            oracle.compare_pdf(a, b)

    def test_rust_copy_profile_accepts_only_the_known_seven_cross_page_raw_and_canonical_spans(self):
        for order in ("ordinary-first", "native-first"):
            actual = self.pdf(order + "-actual.pdf", hidden_copies=copy_material(order), hidden_split=3)
            control = self.pdf(order + "-control.pdf", hidden_copies=copy_material(order, control=True), hidden_size=5, hidden_split=3)
            proof = oracle.compare_pdf(actual, control, math_order=order, engine="rust")
            self.assertEqual(proof["mathCopy"]["isolatedInvisibleMathSpans"], 7)
            self.assertEqual(proof["allDrawingPaths"], 5)
            self.assertIn("PLAIN_BODY", oracle.pdf(actual)["visibleText"])
            self.assertNotIn(r"\frac", oracle.pdf(actual)["visibleText"])
            with self.assertRaises(AssertionError):
                oracle.compare_pdf(actual, control)

    def test_rust_copy_profile_rejects_unknown_hidden_body_extra_eighth_and_missing_math_copy(self):
        expected = copy_material()
        control = self.pdf("control.pdf", hidden_copies=copy_material(control=True))
        for index, copies in enumerate((expected + ["UNEXPECTED_HIDDEN_BODY"], expected[:-1],
                                        ["PLAIN_BODY must remain unchanged."] + expected[1:])):
            actual = self.pdf(str(index) + ".pdf", hidden_copies=copies)
            with self.subTest(index=index), self.assertRaises(AssertionError):
                oracle.compare_pdf(actual, control, math_order="ordinary-first", engine="rust")

    def test_rust_copy_profile_keeps_visible_prose_glyph_paint_and_math_draws_strict(self):
        control = self.pdf("control.pdf", hidden_copies=copy_material(control=True))
        for index, options in enumerate(({"text_shift": .1}, {"text_color": (0, 0, 1)}, {"body": False}, {"shift": .1})):
            actual = self.pdf(str(index) + ".pdf", hidden_copies=copy_material(), **options)
            with self.subTest(index=index), self.assertRaises(AssertionError):
                oracle.compare_pdf(actual, control, math_order="ordinary-first", engine="rust")

    def test_typst_and_default_profiles_reject_hidden_text_and_require_explicit_paired_selectors(self):
        plain = self.pdf("plain.pdf")
        self.assertTrue(oracle.compare_pdf(plain, plain, math_order="ordinary-first", engine="typst")["passed"])
        hidden = self.pdf("hidden.pdf", hidden_copies=copy_material())
        for options in ({}, {"math_order": "ordinary-first", "engine": "typst"}, {"engine": "rust"}, {"math_order": "ordinary-first"}):
            with self.subTest(options=options), self.assertRaises(AssertionError):
                oracle.compare_pdf(hidden, hidden, **options)

    def test_rust_profile_separates_only_verified_cross_page_nonpainting_type3_copy_font(self):
        import pymupdf as fitz
        for order in ("ordinary-first", "native-first"):
            actual = self.pdf(order + "-type3-actual.pdf", hidden_copies=copy_material(order), hidden_split=3, type3_copy=True)
            control = self.pdf(order + "-type3-control.pdf", hidden_copies=copy_material(order, control=True), hidden_split=3, type3_copy=True)
            with fitz.open(actual) as document:
                fonts = {record[0]: record for page in document for record in page.get_fonts(full=True)}
                copy_fonts = [record for record in fonts.values() if record[3] == "ReadMDMathText"]
                self.assertEqual(len(copy_fonts), 1)
                self.assertEqual(copy_fonts[0][2], "Type3")
                self.assertEqual(document.extract_font(copy_fonts[0][0])[3], b"")
                self.assertEqual([trace["font"] for page in document for trace in page.get_texttrace() if trace["type"] == 3], ["ReadMDMathText"] * 7)
            proof = oracle.compare_pdf(actual, control, math_order=order, engine="rust")
            for side in ("actual", "control"):
                self.assertEqual(proof["mathCopy"]["copyOnlyFontResources"][side][0]["hiddenSpans"], 7)
            for options in ({}, {"math_order": order, "engine": "typst"}):
                with self.subTest(order=order, options=options), self.assertRaises(AssertionError):
                    oracle.compare_pdf(actual, actual, **options)

    def test_rust_profile_rejects_visible_copy_font_unknown_type3_and_same_name_resource_ambiguity(self):
        import pymupdf as fitz
        for suffix, options, reason in (("visible", {"copy_font_visible": True}, "copy font used by visible text"),
                                        ("unknown", {"copy_font_name": "UnreviewedCopy"}, "Unreviewed unembedded PDF font shape"),
                                        ("alias", {"copy_font_alias": True}, "Ambiguous mathematics copy font resources")):
            actual = self.pdf(suffix + "-actual.pdf", hidden_copies=copy_material(), type3_copy=True, **options)
            control = self.pdf(suffix + "-control.pdf", hidden_copies=copy_material(control=True), type3_copy=True, **options)
            if suffix == "visible":
                with fitz.open(actual) as document:
                    self.assertTrue(any(trace["font"] == "ReadMDMathText" and trace["type"] == 0
                                        for page in document for trace in page.get_texttrace()))
            with self.subTest(suffix=suffix), self.assertRaisesRegex(AssertionError, reason):
                oracle.compare_pdf(actual, control, math_order="ordinary-first", engine="rust")

    def docx_materials(self):
        # Synthetic oracle controls only. No package here is claimed to be a
        # product export; native input parts come from the existing OPC fixture.
        native = oracle.native
        source_parts = native.fixture_parts("a")
        source = self.directory / "synthetic-native.docx"
        native.opc.write_package(source, source_parts)
        package = native.Package(parts=source_parts)
        source_document = package.xml(package.document)
        source_body = source_document.find("w:body", native.NS)
        section = copy.deepcopy(source_body.find("w:sectPr", native.NS))
        source_styles = copy.deepcopy(package.xml(package.related("styles")))
        for style in source_styles:
            style.attrib.pop(native.w("default"), None)
        template = native.parse_xml((f'<w:styles xmlns:w="{native.W}"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"/><w:style w:type="paragraph" w:styleId="ReadMDMath"><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/></w:pPr></w:style></w:styles>').encode())
        mixed_styles = copy.deepcopy(template)
        for style in source_styles:
            mixed_styles.append(style)
        def paragraph(text):
            node = ET.Element(native.w("p"))
            ET.SubElement(ET.SubElement(node, native.w("r")), native.w("t")).text = text
            return node
        def math_object(sup=False):
            inside = '<m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup>' if sup else '<m:f><m:num><m:r><m:t>x</m:t></m:r></m:num><m:den><m:r><m:t>y</m:t></m:r></m:den></m:f>'
            return native.parse_xml((f'<m:oMath xmlns:m="{native.M}">' + inside + '</m:oMath>').encode())
        inline = paragraph("Inline ")
        inline.append(math_object(True))
        ET.SubElement(ET.SubElement(inline, native.w("r")), native.w("t")).text = " and duplicate "
        inline.append(math_object(True))
        ET.SubElement(ET.SubElement(inline, native.w("r")), native.w("t")).text = "."
        displays = []
        for _ in range(2):
            node = ET.Element(native.w("p"))
            props = ET.SubElement(node, native.w("pPr"))
            ET.SubElement(props, native.w("pStyle")), ET.SubElement(props, native.w("jc"))
            props[0].set(native.w("val"), "ReadMDMath")
            props[1].set(native.w("val"), "center")
            ET.SubElement(node, native.m("oMathPara")).append(math_object())
            displays.append(node)
        plain = [paragraph("PLAIN_CONTROL"), paragraph("PLAIN_BODY must remain unchanged."), inline] + displays
        imported = [copy.deepcopy(node) for node in source_body if node.tag != native.w("sectPr")]
        for node in imported:
            if node.find("m:oMathPara", native.NS) is not None:
                props = node.find("w:pPr", native.NS)
                if props.find("w:pStyle", native.NS) is None:
                    selected = ET.Element(native.w("pStyle"), {native.w("val"): "DefaultMath"})
                    props.insert(0, selected)
        def write(name, nodes, styles):
            parts = dict(source_parts)
            document = copy.deepcopy(source_document)
            body = document.find("w:body", native.NS)
            body.clear()
            for node in nodes:
                body.append(copy.deepcopy(node))
            body.append(copy.deepcopy(section))
            parts[package.document] = native.opc.xml_bytes(document)
            parts[package.related("styles")] = native.opc.xml_bytes(styles)
            file = self.directory / name
            native.opc.write_package(file, parts)
            return file
        baseline = write("synthetic-baseline.docx", plain, template)
        result = write("synthetic-result.docx", plain + imported, mixed_styles)
        return source, result, baseline

    def mutate_docx(self, file, name, mutation):
        native = oracle.native
        package = native.Package(file)
        root = package.xml(package.document)
        mutation(root.find("w:body", native.NS))
        parts = dict(package.parts)
        parts[package.document] = native.opc.xml_bytes(root)
        output = self.directory / name
        native.opc.write_package(output, parts)
        return output

    def test_docx_synthetic_positive_binds_complete_inline_and_two_display_baseline(self):
        source, result, baseline = self.docx_materials()
        proof = oracle.docx(source, result, baseline)
        self.assertEqual(proof["inlineMath"], 2)
        self.assertTrue(proof["inlineParagraphUnchanged"])

    def test_docx_baseline_missing_one_or_both_display_formulas_is_rejected(self):
        source, result, baseline = self.docx_materials()
        for count in (1, 2):
            def damage(body):
                nodes = [node for node in body if node.find("m:oMathPara", oracle.native.NS) is not None]
                for node in nodes[:count]:
                    body.remove(node)
            broken = self.mutate_docx(baseline, f"baseline-missing-{count}.docx", damage)
            with self.subTest(count=count), self.assertRaises(AssertionError):
                oracle.docx(source, result, broken)

    def test_docx_inline_missing_duplicate_or_changed_content_is_rejected(self):
        source, result, baseline = self.docx_materials()
        for where in ("baseline", "result"):
            for damage in ("paragraph", "math", "paint", "duplicate"):
                def mutation(body):
                    node = next(node for node in body if "".join(t.text or "" for t in node.iter(oracle.native.w("t"))).startswith("Inline "))
                    if damage == "paragraph":
                        body.remove(node)
                    elif damage == "math":
                        node.remove(node.find("m:oMath", oracle.native.NS))
                    elif damage == "paint":
                        props = ET.Element(oracle.native.w("rPr"))
                        node.find("w:r", oracle.native.NS).insert(0, props)
                        ET.SubElement(props, oracle.native.w("color"), {oracle.native.w("val"): "0000FF"})
                    else:
                        body.insert(list(body).index(node), copy.deepcopy(node))
                broken = self.mutate_docx(baseline if where == "baseline" else result, where + "-" + damage + ".docx", mutation)
                with self.subTest(where=where, damage=damage), self.assertRaises(AssertionError):
                    oracle.docx(source, broken if where == "result" else result, broken if where == "baseline" else baseline)


if __name__ == "__main__":
    unittest.main()
