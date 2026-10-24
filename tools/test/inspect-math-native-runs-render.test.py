"""Adversarial QA controls for the finite run-style vector oracle."""
import importlib.util
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile

spec=importlib.util.spec_from_file_location("native_runs_render",Path(__file__).with_name("inspect-math-native-runs-render.py"))
oracle=importlib.util.module_from_spec(spec);spec.loader.exec_module(oracle)


SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100" aria-hidden="true"><g data-math-ink="true"><path d="M 1 2 L 3 4" fill="#336699"/><rect x="1" y="5" width="20" height="1"/></g></svg>'
OPEN = '<div class="readmd-export-math readmd-export-math-block" style="text-align:left" role="math" aria-label="\\frac{a}{b}">'


def html_source():
    return '<h1>Native vectors 😀</h1>\r\n<p>Visible ordinary prose</p>\r\n' + "\r\n".join(OPEN + SVG + "</div>" for _ in range(3))


def epub_parts():
    m = '<math xmlns="http://www.w3.org/1998/Math/MathML" display="block"><mfrac><mi mathvariant="bold">a</mi><mi>b</mi></mfrac></math>'
    chapter = '<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Owned</title><link rel="stylesheet" type="text/css" href="style.css"/></head><body><p>Visible prose</p>' + "".join('<div class="math-display" style="text-align:left">' + m + '</div>' for _ in range(3)) + '</body></html>'
    return {"mimetype":b"application/epub+zip",
            "META-INF/container.xml":b'<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
            "OEBPS/content.opf":b'<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Owned</dc:title><dc:identifier id="uid">qa-native-runs</dc:identifier><dc:language>en</dc:language><meta property="dcterms:modified">2026-10-24T00:00:00Z</meta></metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml" properties="mathml"/><item id="css" href="style.css" media-type="text/css"/></manifest><spine><itemref idref="chapter"/></spine></package>',
            "OEBPS/chapter.xhtml":chapter.encode(),"OEBPS/style.css":b'body { color:#262626; font-family:serif; } .math-display { text-align:center; }'}


def write_epub(file, parts):
    with zipfile.ZipFile(file, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in parts.items():
            archive.writestr(name, data)


class RenderControls(unittest.TestCase):
    def test_html_extracts_original_svg_bytes_with_unicode_crlf_and_viewbox_case(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder)/"source.html"
            for index, eol in enumerate(("\r\n", "\n", "\r", "\r\r\n")):
                with self.subTest(eol=eol):
                    output = Path(folder)/("vectors-" + str(index))
                    original = html_source().replace("\r\n", eol).encode()
                    source.write_bytes(original)
                    proof = oracle.html_vectors(source, output)
                    self.assertTrue(proof["passed"])
                    self.assertEqual(proof["inputSha256"], hashlib.sha256(original).hexdigest())
                    self.assertEqual(len(proof["vectors"]), 3)
                    for row in proof["vectors"]:
                        raw = Path(row["path"]).read_bytes()
                        self.assertEqual(raw, SVG.encode())
                        self.assertIn(b"viewBox", raw)
                        self.assertNotIn(b"viewbox", raw)
                        self.assertEqual(raw, original[row["sourceByteRange"]["from"]:row["sourceByteRange"]["to"]])
                        self.assertEqual(row["sha256"], hashlib.sha256(raw).hexdigest())

    def test_html_actual_only_hidden_ancestor_and_wrong_wrapper_reject(self):
        good = html_source()
        changes = [good.replace(OPEN, OPEN.replace("text-align:left", "display:none"), 1),
                   good.replace(OPEN, OPEN.replace('role="math"', 'role="presentation"'), 1),
                   good.replace(OPEN, OPEN.replace(r"\frac{a}{b}", "wrong"), 1),
                   good.replace(OPEN, '<div hidden>' + OPEN, 1).replace('</div>', '</div></div>', 1),
                   good.replace(OPEN, '<section style="opacity:0!important">' + OPEN, 1).replace('</div>', '</div></section>', 1),
                   '<template>' + good + '</template>',
                   '<head>' + good + '</head>',
                   '<details>' + good + '</details>',
                   '<style>.readmd-export-math{display:none}</style>' + good,
                   good.replace('<path ', '<path style="visibility:hidden" ', 1),
                   good.replace('<div class=', '<div role="math" class=', 1)]
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder)/"source.html"
            for index, changed in enumerate(changes):
                with self.subTest(index=index):
                    source.write_text(changed, encoding="utf-8")
                    output = Path(folder)/("bad-" + str(index))
                    with self.assertRaises(AssertionError):
                        oracle.html_vectors(source, output)
                    self.assertFalse(output.exists())

    def test_html_requires_exact_three_wrappers_no_nested_or_extra_svg_and_new_output(self):
        good = html_source()
        with tempfile.TemporaryDirectory() as folder:
            source, output = Path(folder)/"source.html", Path(folder)/"vectors"
            for changed in [good.replace(OPEN + SVG + '</div>', '', 1), good + OPEN + SVG + '</div>',
                            good.replace(SVG, SVG + SVG, 1), good.replace(SVG, OPEN + SVG + '</div>', 1)]:
                source.write_text(changed, encoding="utf-8")
                with self.assertRaises(AssertionError):
                    oracle.html_vectors(source, output)
            source.write_text(good, encoding="utf-8")
            output.mkdir()
            (output/"keep.txt").write_bytes(b"KEEP")
            with self.assertRaises(AssertionError):
                oracle.html_vectors(source, output)
            self.assertEqual((output/"keep.txt").read_bytes(), b"KEEP")

    def test_epub_real_spine_visible_three_operand_fractions(self):
        with tempfile.TemporaryDirectory() as folder:
            a,b=Path(folder)/"a.epub",Path(folder)/"b.epub"
            write_epub(a,epub_parts());write_epub(b,epub_parts())
            proof=oracle.compare_epub(a,b)
            self.assertTrue(proof["visibleBodyAndAncestorsChecked"])
            self.assertTrue(proof["selectedStylesheetsCompared"])
            self.assertEqual(proof["actualSpine"],["OEBPS/chapter.xhtml"])

    def test_epub_actual_only_hidden_ancestor_head_and_phantom_reject(self):
        good=epub_parts()
        chapter=good["OEBPS/chapter.xhtml"]
        hidden=[chapter.replace(b'style="text-align:left"',b'style="display:none"',1),
                chapter.replace(b'style="text-align:left"',b'style="opacity:0.0!important"',1),
                chapter.replace(b'<div class="math-display"',b'<div hidden="hidden" class="math-display"',1),
                chapter.replace(b'<body>',b'<body><template>').replace(b'</body>',b'</template></body>'),
                chapter.replace(b'<body>',b'<body><details>').replace(b'</body>',b'</details></body>'),
                chapter.replace(b'<body>',b'<head>').replace(b'</body>',b'</head>'),
                chapter.replace(b'<mfrac>',b'<mphantom><mfrac>',1).replace(b'</mfrac>',b'</mfrac></mphantom>',1),
                chapter.replace(b'<mi mathvariant="bold">',b'<mi style="display:none" mathvariant="bold">',1)]
        with tempfile.TemporaryDirectory() as folder:
            control=Path(folder)/"control.epub";write_epub(control,good)
            for index,changed in enumerate(hidden):
                with self.subTest(index=index):
                    actual=Path(folder)/(str(index)+".epub");parts=dict(good);parts["OEBPS/chapter.xhtml"]=changed;write_epub(actual,parts)
                    with self.assertRaises(AssertionError):
                        oracle.compare_epub(actual,control)

    def test_epub_malformed_opc_dtd_utf16_and_missing_operands_reject(self):
        good=epub_parts()
        variants=[]
        for old,new in [(b'properties="mathml"',b''),(b'media-type="application/xhtml+xml"',b'media-type="text/plain"'),
                        (b'href="chapter.xhtml"',b'href="../missing.xhtml"'),
                        (b'<itemref idref="chapter"/>',b'<itemref idref="chapter"/><itemref idref="chapter"/>')]:
            parts=dict(good);parts["OEBPS/content.opf"]=parts["OEBPS/content.opf"].replace(old,new);variants.append(parts)
        parts=dict(good);parts["mimetype"]=b"text/plain";variants.append(parts)
        parts=dict(good);parts["OEBPS/chapter.xhtml"]=parts["OEBPS/chapter.xhtml"].replace(b'<!DOCTYPE html>',b'<!DOCTYPE html [<!ENTITY q "bad">]>');variants.append(parts)
        parts=dict(good);parts["OEBPS/chapter.xhtml"]=parts["OEBPS/chapter.xhtml"].decode().encode("utf-16");variants.append(parts)
        parts=dict(good);parts["OEBPS/chapter.xhtml"]=parts["OEBPS/chapter.xhtml"].replace(b'<mi>b</mi>',b'');variants.append(parts)
        with tempfile.TemporaryDirectory() as folder:
            for index,parts in enumerate(variants):
                with self.subTest(index=index):
                    actual=Path(folder)/(str(index)+".epub");write_epub(actual,parts)
                    with self.assertRaises((AssertionError,KeyError)):
                        oracle.epub_math(actual)

    def test_epub_changed_ancestor_style_or_css_does_not_disappear_from_comparison(self):
        good=epub_parts()
        with tempfile.TemporaryDirectory() as folder:
            control=Path(folder)/"control.epub";write_epub(control,good)
            for index,change in enumerate(("ancestor","css","hidden-css")):
                parts=dict(good)
                if change=="ancestor":
                    parts["OEBPS/chapter.xhtml"]=parts["OEBPS/chapter.xhtml"].replace(b'text-align:left',b'text-align:right',1)
                elif change=="css":
                    parts["OEBPS/style.css"]=parts["OEBPS/style.css"].replace(b"#262626",b"#993366")
                else:
                    parts["OEBPS/style.css"]+=b" math { display:none!important; }"
                actual=Path(folder)/(str(index)+".epub");write_epub(actual,parts)
                with self.assertRaises(AssertionError):
                    oracle.compare_epub(actual,control)
    def test_complete_copy_corpus_not_old_seven_formula_profile(self):
        trace=lambda text:{"type":3,"chars":[(ord(c),0,(0,0),(0,0,1,1)) for c in text]}
        good=[[trace(r"\frac{a}{b}") for _ in range(3)]]
        self.assertEqual(len(oracle.verify_copy(good,"rust")),3)
        for bad in [good+[[trace("private")]],[[trace(r"\frac{a}{b}")]*7],[[trace("a/b")]*3],[]]:
            with self.assertRaises(AssertionError):oracle.verify_copy(bad,"rust")
        with self.assertRaises(AssertionError):oracle.verify_copy(good,"typst")
        self.assertEqual(oracle.verify_copy([],"typst"),[])

    def test_svg_full_paths_styles_and_root_are_checked(self):
        svg='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"><g data-math-ink="true"><path d="M 1 2 L 3 4" fill="#336699"/><rect x="1" y="5" width="20" height="1"/></g></svg>'
        with tempfile.TemporaryDirectory() as folder:
            a,b=Path(folder)/"a.svg",Path(folder)/"b.svg";a.write_text(svg);b.write_text(svg)
            self.assertTrue(oracle.base.compare_svg(a,b)["passed"])
            for changed in [svg.replace("#336699","#993366"),svg.replace("L 3 4","L 3 5"),svg.replace('width="20"','width="21"'),svg.replace("200 100","201 100")]:
                b.write_text(changed)
                with self.assertRaises(AssertionError):oracle.base.compare_svg(a,b)

    def test_pdf_visible_font_glyph_paint_position_and_invisibility_are_checked(self):
        import pymupdf as fitz
        def make(file,change=""):
            doc=fitz.open();page=doc.new_page(width=400,height=500)
            page.insert_text((30,40 if change!="position" else 41),"Native run control",fontname="helv" if change!="font" else "cour",fontsize=12,render_mode=3 if change=="hidden" else 0)
            for y in (100,200,300):page.draw_line((30,y),(100,y),color=(1,0,0) if change=="paint" else (0,0,0))
            doc.save(file);doc.close()
        with tempfile.TemporaryDirectory() as folder:
            a,b=Path(folder)/"a.pdf",Path(folder)/"b.pdf";make(a);make(b)
            self.assertTrue(oracle.compare_pdf(a,b,"typst")["passed"])
            for change in ("position","font","hidden","paint"):
                b.unlink();make(b,change)
                with self.assertRaises(AssertionError):oracle.compare_pdf(a,b,"typst")


if __name__=="__main__":unittest.main()
