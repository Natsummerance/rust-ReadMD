"""Independent oracle controls; actual synthetic ZIP/XML, no product host."""
import importlib.util
from io import BytesIO
import json
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

from fontTools.ttLib import TTFont

FILE = Path(__file__).with_name('inspect-author-inline.py')
SPEC = importlib.util.spec_from_file_location('author_inline_oracle', FILE)
oracle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(oracle)
BASE = FILE.parents[2] / '.cache/capability-upgrade/batch3-author-inline-20261021/qa'


class IndependentAuthorOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        BASE.mkdir(parents=True, exist_ok=True)
        cls.output = Path(tempfile.mkdtemp(prefix='oracle-selftest-', dir=BASE))
        print('retained_oracle_fixtures=' + str(cls.output), flush=True)

    def fixture(self, name, mode='positive'):
        file = self.output / (name + '.docx')
        oracle.save_package(oracle.native_parts(mode), file)
        return file

    def epub_parts(self):
        namespace = 'http://www.w3.org/1999/xhtml'
        first = '<!DOCTYPE html>\n<html xmlns="' + namespace + '"><head><title>First</title></head><body><p id="heading"><span style="font-size:30pt">WEST<span style="font-size:10pt">CS</span></span><a id="ref1" href="b.xhtml#note1" role="doc-noteref">1</a><a id="ref2" href="b.xhtml#note1" role="doc-noteref">1</a></p></body></html>'
        second = '<html xmlns="' + namespace + '"><head><title>Notes</title></head><body><aside id="note1"><span style="font-size:9pt">NOTE_WEST</span><a href="a.xhtml#ref1" role="doc-backlink">back</a><a href="a.xhtml#ref2" role="doc-backlink">back</a></aside></body></html>'
        nav = '<html xmlns="' + namespace + '" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Navigation</title></head><body><nav epub:type="toc" id="toc"><ol><li><a href="a.xhtml#heading">WEST</a></li></ol></nav></body></html>'
        container = '<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>'
        opf = '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="book"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="book">urn:uuid:owned-control</dc:identifier><dc:title>Owned control</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-10-21T00:00:00Z</meta></metadata><manifest><item id="navigation" href="reading.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="first" href="a.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="b.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="navigation"/><itemref idref="first"/><itemref idref="notes"/></spine></package>'
        return {name: value.encode('utf-8') for name, value in {'mimetype': 'application/epub+zip', 'META-INF/container.xml': container, 'OEBPS/book.opf': opf, 'OEBPS/a.xhtml': first, 'OEBPS/b.xhtml': second, 'OEBPS/reading.xhtml': nav}.items()}

    def epub_fixture(self, name, parts=None):
        file = self.output / (name + '.epub')
        with zipfile.ZipFile(file, 'x') as archive:
            for part, data in (self.epub_parts() if parts is None else parts).items():
                archive.writestr(part, data, compress_type=zipfile.ZIP_STORED)
        return file

    def marker(self, value, marker):
        rows = [run for paragraph in value['paragraphs'] for run in paragraph['runs'] if marker in run['text']]
        self.assertEqual(len(rows), 1)
        return rows[0]

    def test_current_caption_fields_and_bookmarks_are_read_from_native_xml(self):
        parts = oracle.native_parts()
        root = oracle.xml(parts['word/document.xml'])
        body = root.find(oracle.q('body'))
        paragraph = ET.SubElement(body, oracle.q('p'))
        ET.SubElement(paragraph, oracle.q('bookmarkStart'), {oracle.q('name'): 'current_caption', oracle.q('id'): '7'})
        field = ET.SubElement(paragraph, oracle.q('fldSimple'), {oracle.q('instr'): ' SEQ ReadMDTable \\* ARABIC '})
        ET.SubElement(ET.SubElement(field, oracle.q('r')), oracle.q('t')).text = '1'
        ET.SubElement(ET.SubElement(paragraph, oracle.q('r')), oracle.q('t')).text = ' NATIVE_CAPTION'
        reference = ET.SubElement(body, oracle.q('p'))
        ET.SubElement(ET.SubElement(reference, oracle.q('r')), oracle.q('instrText')).text = ' REF current_caption \\h '
        parts['word/document.xml'] = ET.tostring(root, encoding='utf-8')
        file = self.output / 'current-caption-fields.docx'
        oracle.save_package(parts, file)
        value = oracle.inspect_docx(file)
        sequence = [row for row in value['fields'] if 'NATIVE_CAPTION' in row['paragraphText']]
        self.assertEqual(len(sequence), 1)
        self.assertEqual(sequence[0]['instruction'], ' SEQ ReadMDTable \\* ARABIC ')
        bookmark = [row for row in value['bookmarks'] if row['name'] == 'current_caption']
        self.assertEqual(bookmark[0]['paragraphOrdinal'], sequence[0]['paragraphOrdinal'])
        self.assertIn('NATIVE_CAPTION', bookmark[0]['paragraphText'])
        self.assertTrue(any(row['instruction'] == ' REF current_caption \\h ' for row in value['fields']))

    def test_rendered_html_records_current_anchor_position_and_real_link_text(self):
        file = self.output / 'caption-anchor-control.html'
        file.write_text('<span id="tbl:first"></span><p><span style="font-size:30pt">CAPTION</span></p><p><a href="#tbl:first">Table 1</a></p>', encoding='utf-8')
        value = oracle.inspect_rendered_html(file)
        self.assertEqual(value['identities'][0]['textIndex'], 0)
        self.assertEqual(value['links'][0]['text'], 'Table 1')
        self.assertEqual(value['links'][0]['href'], '#tbl:first')
        self.assertEqual(value['texts'][0]['points'], 30)
        self.assertEqual(value['texts'][0]['ids'], [], 'A preceding empty anchor must not claim ownership of later text')

    def test_epub_text_rows_keep_their_actual_current_ancestor_identity(self):
        value = oracle.inspect_epub(self.epub_fixture('caption-ancestor-control'))
        west = [row for row in value['texts'] if row['text'] == 'WEST' and row['contentRole'] == 'body']
        self.assertEqual(len(west), 1)
        self.assertEqual(west[0]['ids'], ['heading'])
        self.assertEqual(west[0]['points'], 30)

    def test_real_sfnt_without_optional_names_still_requires_exact_outlines(self):
        file = FILE.with_name('publication-smallcaps-proof.py')
        spec = importlib.util.spec_from_file_location('author_font_outline_control', file)
        checker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(checker)
        source = FILE.parents[2] / 'assets/fonts/STIXTwoMath-Regular.ttf'
        original = checker.OutlineFont(source.read_bytes())
        self.assertTrue(original.names)
        font = TTFont(source)
        glyph = font.getBestCmap()[ord('A')]
        del font['name']
        buffer = BytesIO()
        font.save(buffer)
        data = buffer.getvalue()
        (self.output / 'optional-name-absent.ttf').write_bytes(data)
        unnamed = checker.OutlineFont(data)
        self.assertEqual(unnamed.names, [])
        self.assertEqual(unnamed.name_sources, [])
        self.assertEqual(unnamed.units, original.units)
        self.assertEqual(unnamed.outline(glyph), original.outline(glyph))
        self.assertEqual(unnamed.format, 'sfnt')

    def test_optional_name_support_does_not_accept_a_corrupt_font_program(self):
        file = FILE.with_name('publication-smallcaps-proof.py')
        spec = importlib.util.spec_from_file_location('author_bad_font_outline_control', file)
        checker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(checker)
        with self.assertRaises(Exception):
            checker.OutlineFont(b'not an embedded font')

    def test_only_epub_accepts_the_inert_canonical_html5_doctype(self):
        body = '<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Visible text</p></body></html>'
        for prefix in ('<!DOCTYPE html>\n', '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE html>\n'):
            raw = (prefix + body).encode('utf-8')
            root = oracle.xhtml(raw)
            self.assertEqual(''.join(root.itertext()), 'Visible text')
            with self.assertRaises(AssertionError):
                oracle.xml(raw)

    def test_epub_doctype_never_enables_dtd_entities_or_external_identifiers(self):
        body = '<html xmlns="http://www.w3.org/1999/xhtml"><body>Visible text</body></html>'
        for prefix in (
            '<!DOCTYPE html [<!ENTITY x "expanded">]>',
            '<!DOCTYPE html SYSTEM "https://example.invalid/not-fetched">',
            '<!DOCTYPE html PUBLIC "identifier" "https://example.invalid/not-fetched">',
            '<!DOCTYPE html><!DOCTYPE html>',
            '<!-- not a prolog --><!DOCTYPE html>',
            '<!ENTITY x "expanded">',
        ):
            with self.assertRaises(AssertionError):
                oracle.xhtml((prefix + body).encode('utf-8'))
        with self.assertRaises((AssertionError, UnicodeError)):
            oracle.xhtml(('<!DOCTYPE html>' + body).encode('utf-16'))

    def test_actual_native_only_zip_and_exact_independent_cascade(self):
        value = oracle.inspect_docx(self.fixture('positive'))
        self.assertEqual(value['auxiliaryParts'], {})
        self.assertFalse(value['sourcePayloadUsedAsNativeEvidence'])
        expected = {'NATIVE_BASE': (24, 20), 'NATIVE_BIG': (60, 20), 'NATIVE_CHAR': (24, 15),
                    'NATIVE_FORCED': (50, 16), 'NATIVE_ROUTE_CLEAR': (24, 20),
                    'NATIVE_NOTE_BASE': (20, 18), 'NATIVE_NOTE_BIG': (60, 14),
                    'NATIVE_END_BASE': (22, 16), 'NATIVE_END_BIG': (52, 13)}
        for marker, pair in expected.items():
            run = self.marker(value, marker)
            self.assertEqual((run['halfPoints'], run['halfPointsCs']), pair, marker)
        self.assertTrue(self.marker(value, 'NATIVE_FORCED')['cs'])
        self.assertFalse(self.marker(value, 'NATIVE_ROUTE_CLEAR')['cs'])
        self.assertFalse(self.marker(value, 'NATIVE_ROUTE_CLEAR')['rtl'])
        self.assertEqual(value['notes']['footnote']['references'], ['7'])
        self.assertEqual(value['notes']['endnote']['references'], ['7'])
        self.assertEqual(value['notes']['footnote']['definitions'][0]['id'], value['notes']['endnote']['definitions'][0]['id'])

    def test_character_style_does_not_reset_paragraph_axis_to_doc_defaults(self):
        value = oracle.inspect_docx(self.fixture('character-defaults'))
        self.assertEqual(self.marker(value, 'NATIVE_CHAR')['halfPoints'], 24)
        self.assertNotEqual(self.marker(value, 'NATIVE_CHAR')['halfPoints'], 23)

    def test_hyperlink_run_binds_a_real_qname_relationship(self):
        parts = oracle.native_parts()
        root = oracle.xml(parts['word/document.xml'])
        body = root.find('w:body', oracle.NS)
        paragraph = ET.Element(oracle.q('p'))
        body.insert(len(body) - 1, paragraph)
        link = ET.SubElement(paragraph, oracle.q('hyperlink'), {'{' + oracle.R + '}id': 'external'})
        link.append(oracle.r('BODY_LINK', '<w:sz w:val="60"/><w:szCs w:val="20"/>'))
        rels = oracle.xml(parts['word/_rels/document.xml.rels'])
        ET.SubElement(rels, '{' + oracle.P + '}Relationship', {'Id': 'external', 'Type': oracle.R + '/hyperlink', 'Target': 'https://example.test/inline', 'TargetMode': 'External'})
        parts['word/document.xml'] = ET.tostring(root)
        parts['word/_rels/document.xml.rels'] = ET.tostring(rels)
        file = self.output / 'native-hyperlink.docx'
        oracle.save_package(parts, file)
        value = oracle.inspect_docx(file)
        self.assertEqual(self.marker(value, 'BODY_LINK')['hyperlink'], {'part': 'word/document.xml', 'relationship': 'external', 'anchor': None})
        self.assertEqual(value['hyperlinks'][0]['Target'], 'https://example.test/inline')

    def test_invalid_and_foreign_size_have_real_current_xml(self):
        for mode in ('invalid-size', 'foreign-size'):
            file = self.fixture(mode, mode)
            with self.assertRaises(AssertionError):
                oracle.inspect_docx(file)
            self.assertIn(b'NATIVE_INVALID', oracle.Package(file).parts['word/document.xml'])

    def test_formatted_live_field_fixture_is_preserved_as_native_material(self):
        file = self.fixture('field', 'field-size')
        value = oracle.inspect_docx(file)
        self.assertEqual(self.marker(value, 'NATIVE_FIELD')['halfPoints'], 60)
        self.assertEqual(len(list(oracle.Package(file).node('word/document.xml').iter(oracle.q('fldSimple')))), 1)

    def test_mutation_changes_only_actual_run_and_preserves_auxiliary_bytes(self):
        parts = oracle.native_parts()
        root = oracle.xml(parts['word/document.xml'])
        body = root.find('w:body', oracle.NS)
        paragraph = ET.Element(oracle.q('p'))
        body.insert(len(body) - 1, paragraph)
        paragraph.append(oracle.r('BODY_WEST', '<w:sz w:val="60"/><w:szCs w:val="20"/>'))
        parts['word/document.xml'] = ET.tostring(root)
        parts['customXml/readmd-source.xml'] = b'<source>old text that is never typography evidence</source>'
        file = self.output / 'mutation-input.docx'
        oracle.save_package(parts, file)
        for mode in ('native-change', 'native-clear'):
            output = self.output / (mode + '.docx')
            result = oracle.mutate(file, output, mode)
            self.assertEqual(result['changedParts'], ['word/document.xml'])
            self.assertTrue(result['auxiliaryUnchanged'])
            run = self.marker(result['inspection'], 'BODY_WEST')
            self.assertEqual((run['halfPoints'], run['halfPointsCs']), (52, 28) if mode == 'native-change' else (23, 19))

    def test_relationship_target_cannot_escape_or_read_a_host_uri(self):
        for name in ('../../host.xml', 'file:///C:/host.xml', 'C:/host.xml', 'https://example.test/x', '../x?y', '../x#y', '..\\host.xml'):
            with self.assertRaises(AssertionError, msg=name):
                oracle.target('word/document.xml', name)
        self.assertEqual(oracle.target('word/document.xml', 'notes/current.xml'), 'word/notes/current.xml')

    def test_dtd_and_oversized_xml_are_not_accepted(self):
        for raw in (b'<!DOCTYPE x><x/>', b'<!ENTITY x "bad"><x/>', b''):
            with self.assertRaises(AssertionError):
                oracle.xml(raw)
        with self.assertRaises(AssertionError):
            oracle.properties(ET.fromstring('<w:rPr xmlns:w="' + oracle.W + '"><w:sz w:val="10"/><w:sz w:val="20"/></w:rPr>'))

    def test_html_tokens_decode_entities_once_without_activating_markup(self):
        file = self.output / 'marked-output.html'
        file.write_text('<p><span data-readmd-author-run="key" style="font-size:30pt"><strong>BODY&#95;BOLD</strong> &amp; &lt;literal&gt;</span></p>', encoding='utf-8')
        result = oracle.inspect_rendered_html(file)
        self.assertEqual(result['keys'], ['key'])
        self.assertEqual(''.join(row['text'] for row in result['texts']), 'BODY_BOLD & <literal>')
        marker = next(row for row in result['texts'] if 'BODY_BOLD' in row['text'])
        self.assertIn('strong', marker['tags'])
        self.assertEqual(marker['points'], 30)
        self.assertFalse(result['browserComputedStyleCertified'])

    def test_html_export_inspection_reads_real_encoded_source_and_model(self):
        file = self.output / 'actual-export-shape.html'
        model = {'nonce': 'a' * 32, 'runs': [], 'content': 'literal <script> text'}
        file.write_text('<html><script type="application/json" id="md-source">' + json.dumps('source & <literal>').replace('<', '\\u003c') + '</script><script>window.ReadMDAuthorText.apply(document.getElementById(\'content\'),' + json.dumps(model).replace('<', '\\u003c') + ');</script></html>', encoding='utf-8')
        value = oracle.inspect_html(file)
        self.assertEqual(value['markdown'], 'source & <literal>')
        self.assertEqual(value['model'], model)
        self.assertTrue(value['actualExportRead'])

    def typed_html(self):
        note = '<p>Body<a id="ref1" href="#note1" role="doc-noteref">1</a> and<a id="ref2" href="#note1" role="doc-noteref">1</a></p><ol><li id="note1"><span data-readmd-author-run="' + 'a' * 32 + '-0" style="font-size:30pt">NOTE_WEST</span><a href="#ref1" role="doc-backlink">back</a><a href="#ref2" role="doc-backlink">back</a></li></ol>'
        model = {'nonce': 'a' * 32, 'runs': [{'key': 'a' * 32 + '-0', 'size': 30, 'base': None, 'complex': False, 'blockBase': False}], 'content': 'source [^f]'}
        encode = lambda value: json.dumps(value).replace('<', '\\u003c')
        source = "source [^f] window.ReadMDAuthorText.apply(root,THIS_IS_NOT_A_MODEL)"
        prefix = '<html><script type="application/json" id="md-source">' + encode(source) + '</script><script type="application/json" id="readmd-footnote-html">' + encode(note) + '</script>'
        script = "<script>(function () { var md=JSON.parse(document.getElementById('md-source').textContent); var html; try { html = JSON.parse(document.getElementById('readmd-footnote-html').textContent); } catch(e) { html=''; } document.getElementById('content').innerHTML = html; var root = document.getElementById('content'); window.ReadMDAuthorText.apply(root," + encode(model) + "); root.querySelectorAll('code'); })();</script></html>"
        return prefix + script, source, note, model

    def test_actual_typed_note_path_and_iife_model_keep_all_note_identities(self):
        raw, source, note, model = self.typed_html()
        file = self.output / 'typed-note-export.html'
        file.write_text(raw, encoding='utf-8')
        value = oracle.inspect_html(file)
        self.assertEqual(value['markdown'], source)
        self.assertEqual(value['model'], model)
        self.assertEqual(value['renderedMode'], 'typed-footnotes')
        self.assertEqual(value['renderedHtml'], note)
        rendered_file = self.output / 'typed-note-selected.html'
        rendered_file.write_text(value['renderedHtml'], encoding='utf-8')
        rendered = oracle.inspect_rendered_html(rendered_file)
        self.assertEqual(len(rendered['links']), 4)
        self.assertEqual([row['id'] for row in rendered['identities']], ['ref1', 'ref2', 'note1'])
        self.assertIn('NOTE_WEST', rendered['identities'][2]['text'])
        self.assertEqual(rendered['keys'], [model['runs'][0]['key']])
        self.assertEqual(sum(row['text'].count('NOTE_WEST') for row in rendered['texts']), 1)

    def test_html_payload_must_be_selected_before_the_real_model_is_applied(self):
        raw, _, _, _ = self.typed_html()
        for index, modified in enumerate((
            raw.replace("html = JSON.parse(document.getElementById('readmd-footnote-html').textContent);", "html = marked.parse(md, { gfm: true, breaks: true });"),
            raw.replace('id="readmd-footnote-html">', 'id="readmd-footnote-html">null</script><script type="application/json" id="readmd-footnote-html">'),
            raw.replace("document.getElementById('content').innerHTML = html;", ''),
            raw.replace("var root = document.getElementById('content');", ''),
        )):
            file = self.output / ('bad-note-path-' + str(index) + '.html')
            file.write_text(modified, encoding='utf-8')
            with self.assertRaises(AssertionError):
                oracle.inspect_html(file)
        duplicate = self.output / 'duplicate-note-id.html'
        duplicate.write_text('<a id="same">one</a><a id="same">two</a>', encoding='utf-8')
        with self.assertRaises(AssertionError):
            oracle.inspect_rendered_html(duplicate)

    def test_actual_xhtml_zip_keeps_nearest_points_and_note_link_identities(self):
        value = oracle.inspect_epub(self.epub_fixture('actual-xhtml'))
        for marker, points in (('WEST', 30), ('CS', 10), ('NOTE_WEST', 9)):
            rows = [row for row in value['texts'] if row['text'] == marker and row['contentRole'] == 'body']
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]['points'], points)
        self.assertEqual(len(value['links']), 5)
        self.assertEqual(len(value['identities']), 5)
        self.assertEqual(value['bodyParts'], ['OEBPS/a.xhtml', 'OEBPS/b.xhtml'])
        self.assertEqual(value['navigationParts'], ['OEBPS/reading.xhtml'])
        self.assertEqual(len([row for row in value['texts'] if row['text'] == 'WEST']), 2)
        self.assertEqual(value['navigation'][0]['targetPart'], 'OEBPS/a.xhtml')
        self.assertEqual(value['navigation'][0]['fragment'], 'heading')
        self.assertEqual(value['navigation'][0]['targetContentRole'], 'body')
        self.assertEqual(len([row for row in value['links'] if row['role'] == 'doc-noteref']), 2)
        self.assertEqual(len([row for row in value['links'] if row['role'] == 'doc-backlink']), 2)

    def test_epub_roles_follow_manifest_and_spine_instead_of_part_names(self):
        parts = self.epub_parts()
        parts['OEBPS/nav.xhtml'] = parts.pop('OEBPS/a.xhtml')
        parts['OEBPS/chapter_1.xhtml'] = parts.pop('OEBPS/reading.xhtml').replace(b'a.xhtml#heading', b'nav.xhtml#heading')
        parts['OEBPS/b.xhtml'] = parts['OEBPS/b.xhtml'].replace(b'a.xhtml#ref', b'nav.xhtml#ref')
        parts['OEBPS/book.opf'] = parts['OEBPS/book.opf'].replace(b'href="a.xhtml"', b'href="nav.xhtml"').replace(b'href="reading.xhtml"', b'href="chapter_1.xhtml"')
        value = oracle.inspect_epub(self.epub_fixture('misleading-part-names', parts))
        self.assertEqual(value['navigationParts'], ['OEBPS/chapter_1.xhtml'])
        self.assertIn('OEBPS/nav.xhtml', value['bodyParts'])
        rows = [row for row in value['texts'] if row['text'] == 'WEST']
        self.assertEqual({(row['part'], row['contentRole']) for row in rows}, {('OEBPS/nav.xhtml', 'body'), ('OEBPS/chapter_1.xhtml', 'navigation')})

    def test_declared_nav_requires_actual_namespace_correct_toc(self):
        for name, change in [('fake-property', lambda parts: parts.__setitem__('OEBPS/book.opf', parts['OEBPS/book.opf'].replace(b' properties="nav"', b'').replace(b'id="first"', b'id="first" properties="nav"'))), ('fake-namespace', lambda parts: parts.__setitem__('OEBPS/reading.xhtml', parts['OEBPS/reading.xhtml'].replace(b'http://www.idpf.org/2007/ops', b'urn:fake-epub'))), ('no-nav-property', lambda parts: parts.__setitem__('OEBPS/book.opf', parts['OEBPS/book.opf'].replace(b' properties="nav"', b'')))]:
            with self.subTest(name=name):
                parts = self.epub_parts()
                change(parts)
                with self.assertRaises(AssertionError):
                    oracle.inspect_epub(self.epub_fixture(name, parts))

    def test_nav_cannot_hide_ordinary_reading_prose_outside_real_navigation_elements(self):
        parts = self.epub_parts()
        parts['OEBPS/reading.xhtml'] = parts['OEBPS/reading.xhtml'].replace(b'</body>', b'<p><span style="font-size:30pt">WEST</span></p></body>')
        value = oracle.inspect_epub(self.epub_fixture('combined-nav-reading', parts))
        self.assertIn('OEBPS/reading.xhtml', value['bodyParts'])
        rows = [row for row in value['texts'] if row['text'] == 'WEST' and row['contentRole'] == 'body']
        self.assertEqual(len(rows), 2, 'A body duplicate was hidden by a nav manifest property')

    def test_epub_navigation_rejects_missing_external_and_non_body_toc_targets(self):
        for index, href in enumerate(('a.xhtml#missing', 'missing.xhtml#heading', 'reading.xhtml#toc', 'https://example.test/outside', '../../host.xhtml', 'a.xhtml?query#heading')):
            with self.subTest(href=href):
                parts = self.epub_parts()
                parts['OEBPS/reading.xhtml'] = parts['OEBPS/reading.xhtml'].replace(b'a.xhtml#heading', href.encode())
                with self.assertRaises(AssertionError):
                    oracle.inspect_epub(self.epub_fixture('bad-nav-target-' + str(index), parts))

    def test_epub_container_manifest_and_spine_must_resolve_real_unique_parts(self):
        for name, part, old, new in [('bad-container', 'META-INF/container.xml', b'OEBPS/book.opf', b'../../outside.opf'), ('missing-manifest', 'OEBPS/book.opf', b'href="a.xhtml"', b'href="missing.xhtml"'), ('bad-spine', 'OEBPS/book.opf', b'idref="first"', b'idref="missing"'), ('duplicate-nav', 'OEBPS/book.opf', b'id="first"', b'id="first" properties="nav"')]:
            with self.subTest(name=name):
                parts = self.epub_parts()
                parts[part] = parts[part].replace(old, new)
                with self.assertRaises((AssertionError, KeyError)):
                    oracle.inspect_epub(self.epub_fixture(name, parts))


if __name__ == '__main__':
    unittest.main()
