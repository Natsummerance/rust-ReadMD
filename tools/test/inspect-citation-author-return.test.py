"""Bounded synthetic OPC and actual PyMuPDF oracle controls, not product runs."""
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


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


qa = load('citation_author_oracle_control', 'inspect-citation-author-return.py')
base = qa.base
ARTIFACTS = None
W, C = qa.W, qa.C
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
P = 'http://schemas.openxmlformats.org/package/2006/relationships'


def sdt(identity, body, bibliography=False):
    prefix = 'readmd:bibliography:v1:' if bibliography else 'readmd:citation:v1:'
    return '<w:sdt><w:sdtPr><w:tag w:val="' + prefix + identity + '"/></w:sdtPr><w:sdtContent>' + body + '</w:sdtContent></w:sdt>'


def run(value, size=None):
    props = '' if size is None else '<w:rPr><w:sz w:val="' + str(size) + '"/><w:szCs w:val="20"/></w:rPr>'
    return '<w:r>' + props + '<w:t xml:space="preserve">' + value + '</w:t></w:r>'


def model_parts(story='footnote', native=False, edited=False):
    part = 'word/' + story + 's.xml'
    reference = lambda identity: '<w:r><w:' + story + 'Reference w:id="' + str(identity) + '"/></w:r>'
    body = '<w:p>' + run('P07_AUTHOR_BEGIN First ') + (reference(1) if native else sdt('a1', reference(1))) + run('. Same ') + (reference(2) if native else sdt('a2', reference(2))) + run('. Author ') + sdt('author', run('Beta')) + run('. Manual') + reference(3) + run(', repeated') + reference(3) + run('. P07_AUTHOR_END.') + '</w:p>'
    body += sdt('bib', '<w:p>' + run('References FIRST_TITLE / PUB_A SECOND_TITLE / PUB_B') + '</w:p>', True)
    body += '<w:p>' + run('P07_TAIL_KEEP.') + '</w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>'
    doc = '<w:document xmlns:w="' + W + '"><w:body>' + body + '</w:body></w:document>'
    notes = []
    for identity in (1, 2, 3):
        content = run('MANUAL_CURRENT strong and evidence.', 20) if identity == 3 else sdt('n' + str(identity), run('(1:Alpha:' + ('111' if edited and identity == 2 else '12') + ')', 23 if edited and identity == 2 else 20))
        notes.append('<w:' + story + ' w:id="' + str(identity) + '"><w:p><w:pPr><w:pStyle w:val="' + ('FootnoteText' if story == 'footnote' else 'EndnoteText') + '"/></w:pPr><w:r><w:' + story + 'Ref/></w:r>' + run(' ') + content + '</w:p></w:' + story + '>')
    note_xml = '<w:' + story + 's xmlns:w="' + W + '">' + ''.join(notes) + '</w:' + story + 's>'
    style = '<w:styles xmlns:w="' + W + '"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="' + ('FootnoteText' if story == 'footnote' else 'EndnoteText') + '"><w:rPr><w:sz w:val="' + ('40' if native else '20') + '"/><w:szCs w:val="' + ('40' if native else '20') + '"/></w:rPr></w:style></w:styles>'
    root = ET.Element('{' + C + '}citations', {'version': '1'})
    ET.SubElement(root, '{' + C + '}resources').text = json.dumps({'version': 1, 'manual_bibliography': False})
    def record(identity, kind, member, source, note_id=None, generated=False):
        node = ET.SubElement(root, '{' + C + '}record', {'id': identity, 'kind': kind, 'part': member, 'generated': str(generated).lower(), 'render_context_version': '2', 'render_sha256': '1' * 64, 'binding': '2' * 64})
        ET.SubElement(node, '{' + C + '}source').text = source
        ET.SubElement(node, '{' + C + '}original_source').text = source
        if note_id is not None:
            ET.SubElement(node, '{' + C + '}note', {'end': str(story == 'endnote').lower(), 'id': str(note_id), 'story_sha256': '3' * 64})
    for identity in (1, 2):
        source = '[@42, p. 111]' if edited and identity == 2 else '[@42, p. 12]'
        if not native:
            record('a' + str(identity), 'citation', 'word/document.xml', source, identity, True)
        record('n' + str(identity), 'citation', part, source, identity, not native)
    record('author', 'citation', 'word/document.xml', ':cite[author-only @same]')
    record('bib', 'bibliography', 'word/document.xml', '<!--readmd:bibliography-->')
    rels = '<Relationships xmlns="' + P + '"><Relationship Id="s" Type="' + R + '/styles" Target="styles.xml"/><Relationship Id="n" Type="' + R + '/' + story + 's" Target="' + story + 's.xml"/></Relationships>'
    return {'[Content_Types].xml': ('<Types xmlns="' + base.CT + '"/>').encode(), '_rels/.rels': ('<Relationships xmlns="' + P + '"><Relationship Id="r" Type="' + R + '/officeDocument" Target="word/document.xml"/></Relationships>').encode(), 'word/_rels/document.xml.rels': rels.encode(), 'word/document.xml': doc.encode(), 'word/styles.xml': style.encode(), part: note_xml.encode(), qa.PART: ET.tostring(root, encoding='utf-8'), 'owned-untouched.bin': b'KEEP_MEMBER\x00\xff'}


class Controls(unittest.TestCase):
    def setUp(self):
        self.temp = None
        if ARTIFACTS:
            self.root = ARTIFACTS / self._testMethodName
            self.root.mkdir()
        else:
            self.temp = tempfile.TemporaryDirectory()
            self.root = Path(self.temp.name)
    def tearDown(self):
        if self.temp:
            self.temp.cleanup()
    def package(self, name, parts):
        file = self.root / (name + '.docx')
        base.save_package(parts, file)
        return file
    def test01_both_native_stories_and_exact_style_only_mutation_keep_original_members(self):
        for story in ('footnote', 'endnote'):
            source = self.package(story, model_parts(story))
            before = source.read_bytes()
            output = self.root / (story + '-current.docx')
            receipt = qa.mutate(source, output, story, 'template')
            self.assertEqual([row['path'] for row in receipt['changedParts']], ['word/styles.xml'])
            self.assertTrue(receipt['manifestUnchanged'])
            self.assertEqual(receipt['templateAxisHalfPoints'], {'before': 20, 'after': 40})
            self.assertEqual(source.read_bytes(), before)
            self.assertEqual(receipt['inspection']['references'], [0, 1, 2, 2])
            with zipfile.ZipFile(output) as archive:
                self.assertEqual(archive.read('owned-untouched.bin'), b'KEEP_MEMBER\x00\xff')
                styles = base.xml(archive.read('word/styles.xml'))
                self.assertEqual([base.val(node) for node in styles.findall('w:style/w:rPr/w:sz', qa.NS)], ['40'])
                self.assertEqual([base.val(node) for node in styles.findall('w:style/w:rPr/w:szCs', qa.NS)], ['40'])
                self.assertEqual([base.val(node) for node in styles.findall('w:docDefaults/w:rPrDefault/w:rPr/w:sz', qa.NS)], ['22'])
    def test02_two_generation_native_provenance_and_full_axes_are_independent_of_old_payload(self):
        for story in ('footnote', 'endnote'):
            source = self.package(story, model_parts(story))
            current = self.root / (story + '-current.docx')
            qa.mutate(source, current, story, 'template')
            first = self.package(story + '-first', model_parts(story, native=True, edited=True))
            second = self.package(story + '-second', model_parts(story, native=True, edited=True))
            self.assertTrue(qa.compare_native(current, first, story, True)['passed'])
            self.assertTrue(qa.compare_native(current, second, story, True)['passed'])
            self.assertFalse(qa.compare_native(current, first, story, True)['oldGeneratedProvenancePreserved'])
    def test03_changed_native_author_axis_or_non_target_body_or_reference_is_rejected(self):
        source = self.package('source', model_parts())
        current = self.root / 'current.docx'
        qa.mutate(source, current, 'footnote', 'template')
        for name, part, before, after in [
            ('wrong-size', 'word/footnotes.xml', b'w:val="23"', b'w:val="24"'),
            ('wrong-cs', 'word/footnotes.xml', b'<w:szCs w:val="20"/>', b'<w:szCs w:val="21"/>'),
            ('body', 'word/document.xml', b'P07_TAIL_KEEP', b'P07_TAIL_KEEP_BAD'),
            ('manual-target', 'word/document.xml', b'<w:footnoteReference w:id="3"/>', b'<w:footnoteReference w:id="2"/>'),
        ]:
            parts = model_parts(native=True, edited=True)
            self.assertIn(before, parts[part])
            parts[part] = parts[part].replace(before, after, 1)
            output = self.package(name, parts)
            with self.assertRaises(AssertionError):
                qa.compare_native(current, output, 'footnote', True)
    def test04_generated_flag_source_and_extra_note_cannot_fake_a_native_return(self):
        source = self.package('source', model_parts())
        current = self.root / 'current.docx'
        qa.mutate(source, current, 'footnote', 'template')
        for mode in ('generated', 'source', 'extra-note'):
            parts = model_parts(native=True, edited=True)
            if mode == 'extra-note':
                parts['word/footnotes.xml'] = parts['word/footnotes.xml'].replace(b'</w:footnotes>', b'<w:footnote w:id="4"><w:p/></w:footnote></w:footnotes>')
            else:
                root = base.xml(parts[qa.PART])
                record = root.find('c:record', qa.NS)
                if mode == 'generated':
                    record.set('generated', 'true')
                else:
                    record.find('c:source', qa.NS).text = '[@wrong]'
                parts[qa.PART] = ET.tostring(root, encoding='utf-8')
            output = self.package('negative-' + mode, parts)
            with self.assertRaises(AssertionError):
                qa.compare_native(current, output, 'footnote', True)
    def test05_bad_zip_paths_duplicates_xml_entities_and_duplicate_controls_fail(self):
        for mode in ('path', 'duplicate', 'entity', 'control'):
            parts = model_parts()
            file = self.root / (mode + '.docx')
            if mode == 'path':
                parts['../outside.xml'] = b'no'
            elif mode == 'entity':
                parts['word/document.xml'] = b'<!DOCTYPE doc [<!ENTITY x "evil">]><doc/>'
            elif mode == 'control':
                parts['word/document.xml'] = parts['word/document.xml'].replace(b'readmd:citation:v1:a2', b'readmd:citation:v1:a1')
            if mode == 'duplicate':
                with zipfile.ZipFile(file, 'x') as archive:
                    for name, raw in parts.items():
                        archive.writestr(name, raw)
                    archive.writestr('word/styles.xml', parts['word/styles.xml'])
            else:
                base.save_package(parts, file)
            with self.assertRaises(AssertionError):
                package = base.Package(file)
                if mode == 'control':
                    qa.control(package, qa.manifest(package)['records'][0])
                else:
                    qa.inspect_docx(file, 'footnote')
    def test06_negative_mutations_are_owned_and_preserve_every_unrelated_actual_member(self):
        source = self.package('source', model_parts())
        before = source.read_bytes()
        for mode in ('bad-signature', 'duplicate', 'extra-prose', 'prose'):
            output = self.root / (mode + '.docx')
            receipt = qa.mutate(source, output, 'footnote', mode)
            self.assertEqual(source.read_bytes(), before)
            self.assertEqual(len(receipt['changedParts']), 2)
            with zipfile.ZipFile(output) as archive:
                self.assertEqual(archive.read('owned-untouched.bin'), b'KEEP_MEMBER\x00\xff')
        with self.assertRaises((AssertionError, FileExistsError)):
            qa.mutate(source, self.root / 'prose.docx', 'footnote', 'prose')
    def pdf(self, name, mode='normal'):
        import pymupdf as pdf
        file = self.root / (name + '.pdf')
        document = pdf.open()
        page = document.new_page(width=300, height=400)
        content = 'P07 body evidence.' if mode != 'missing' else 'P07 body evidenc.'
        page.insert_text((31 if mode == 'moved' else 30, 50), content, fontsize=12 if mode == 'size' else 11,
                         fontname='cour' if mode == 'font' else 'helv', color=(1, 0, 0) if mode == 'color' else (0, 0, 0),
                         render_mode=3 if mode == 'hidden' else 0)
        page.insert_text((30, 100), '(1:Alpha:12)', fontsize=10)
        page.draw_line((30, 80), (250, 80), color=(0, 0, 1) if mode == 'path' else (0, 0, 0))
        page.insert_link({'kind': pdf.LINK_URI, 'from': pdf.Rect(30, 40, 120, 53), 'uri': 'https://wrong.test/' if mode == 'link' else 'https://example.test/current'})
        document.save(file)
        document.close()
        return file
    def test07_full_pdf_comparator_accepts_same_actual_glyphs_fonts_paths_and_links(self):
        first, second = self.pdf('first'), self.pdf('second')
        proof = qa.compare_pdf(first, second)
        self.assertTrue(proof['passed'])
        self.assertGreater(proof['glyphs'], 20)
        self.assertEqual(proof['allPaths'], 1)
        self.assertEqual(proof['links'], 1)
    def test08_pdf_dropped_moved_resized_recolored_hidden_or_different_font_glyph_and_path_link_fail(self):
        control = self.pdf('control')
        for mode in ('missing', 'moved', 'size', 'color', 'hidden', 'font', 'path', 'link'):
            actual = self.pdf(mode, mode)
            with self.assertRaises(AssertionError, msg=mode):
                qa.compare_pdf(actual, control)
    def test09_selected_html_checks_current_emitted_author_ink_visibility_not_aria_semantics(self):
        good = self.root / 'good.html'
        good.write_text('<aside id="note"><span aria-hidden="true" style="font-size:10pt">(1:Alpha:12)</span></aside>', encoding='utf-8')
        self.assertEqual(qa.inspect_selected_html(good)['texts'][0]['points'], 10)
        for index, attributes in enumerate(('hidden', 'style="display:none"', 'style="opacity:0!important"', 'style="visibility:hidden"')):
            bad = self.root / ('hidden' + str(index) + '.html')
            bad.write_text('<aside ' + attributes + '><span>(1:Alpha:12)</span></aside>', encoding='utf-8')
            with self.assertRaises(AssertionError):
                qa.inspect_selected_html(bad)
    def test10_cli_stdout_is_finite_utf8_json_and_inspection_does_not_rewrite_original(self):
        file = self.package('源', model_parts())
        before = file.read_bytes()
        result = subprocess.run([sys.executable, '-I', '-B', str(Path(qa.__file__)), 'inspect', '--input', str(file), '--story', 'footnote'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        (self.root / 'stdout.json').write_bytes(result.stdout)
        (self.root / 'stderr.log').write_bytes(result.stderr)
        self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8'))
        data = json.loads(result.stdout)
        self.assertEqual(data['kind'], 'citation-author-native')
        self.assertEqual(file.read_bytes(), before)
    def test11_declared_epub_landmarks_may_be_hidden_but_body_notes_toc_and_ancestors_must_stay_visible(self):
        wrap = lambda body: '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Control</title></head><body>' + body + '</body></html>'
        body = wrap('<p id="p">BODY</p><aside id="n" role="doc-footnote">NOTE</aside>')
        nav = wrap('<nav epub:type="toc"><ol><li><a href="body.xhtml#p">Body</a></li></ol></nav><nav epub:type="landmarks" hidden="hidden"><ol><li><a epub:type="bodymatter" href="body.xhtml#p">Body</a></li></ol></nav>')
        parts = {'mimetype': b'application/epub+zip', 'META-INF/container.xml': b'<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>', 'OEBPS/book.opf': b'<package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="body" href="body.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="body"/></spine></package>', 'OEBPS/body.xhtml': body.encode(), 'OEBPS/nav.xhtml': nav.encode()}
        good = self.root / 'landmarks.epub'
        base.save_package(parts, good)
        self.assertEqual(qa.inspect_selected_epub(good)['bodyParts'], ['OEBPS/body.xhtml'])
        for index, (part, old, new) in enumerate([
                ('OEBPS/body.xhtml', '<body>', '<body hidden="hidden">'),
                ('OEBPS/body.xhtml', '<p id="p">', '<p id="p" hidden="hidden">'),
                ('OEBPS/body.xhtml', '<aside id="n"', '<aside hidden="hidden" id="n"'),
                ('OEBPS/body.xhtml', '</body>', '<nav epub:type="landmarks" hidden="hidden">HIDDEN_BODY</nav></body>'),
                ('OEBPS/nav.xhtml', '<nav epub:type="toc">', '<nav epub:type="toc" hidden="hidden">'),
                ('OEBPS/nav.xhtml', '<body>', '<body hidden="hidden">'),
                ('OEBPS/nav.xhtml', '<head>', '<head hidden="hidden">'),
                ('OEBPS/nav.xhtml', 'epub:type="landmarks" hidden="hidden"', 'epub:type="landmarks" hidden="hidden" style="display:none"'),
                ('OEBPS/book.opf', ' properties="nav"', ''),
        ]):
            changed = dict(parts)
            self.assertIn(old.encode(), changed[part])
            changed[part] = changed[part].replace(old.encode(), new.encode(), 1)
            bad = self.root / ('hidden-' + str(index) + '.epub')
            base.save_package(changed, bad)
            if old == '<head>':
                # Metadata is not selected reading content; it is outside this finite proof.
                qa.inspect_selected_epub(bad)
            else:
                with self.assertRaises(AssertionError, msg=str(index)):
                    qa.inspect_selected_epub(bad)


if __name__ == '__main__':
    if '--artifacts' in sys.argv:
        at = sys.argv.index('--artifacts')
        ARTIFACTS = Path(sys.argv[at + 1]).resolve()
        ARTIFACTS.mkdir()
        del sys.argv[at:at + 2]
        (ARTIFACTS / 'scope.json').write_text(json.dumps({'syntheticOpcControlsOnly': True, 'actualProductOrEngineInvoked': False, 'externalApplicationInvoked': False}, indent=2) + '\n', encoding='utf-8')
    unittest.main(verbosity=2)
