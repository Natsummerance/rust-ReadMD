"""Pure independent controls; generated PDFs/OPC are never product evidence."""
import copy
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
import zipfile
import re

spec = importlib.util.spec_from_file_location('note_number_oracle', Path(__file__).with_name('inspect-rust-note-numbering.py'))
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)
ROOT = Path(__file__).resolve().parents[2]
FONT = ROOT / 'assets/vendor/reveal/theme/fonts/source-sans-pro/source-sans-pro-regular.ttf'


def source_text():
    text = '[^beta-k]: NOTE_B_BODY evidence NOTE_B_END\n\nNOTE_DOCUMENT_BEGIN [word-footnote-1] and unparsed [^missing].\n\nRZFIRST[^zeta-k]RZONE RAFIRST[^alpha-k]RAONE\n\n'
    for number in range(1, 73):
        text += 'FLOW_' + str(number).zfill(3) + '_KEEP paragraph\n\n'
        if number == 36:
            text += 'RZAGAIN[^zeta-k]RZTWO\n\n'
    return text + 'RBFIRST[^beta-k]RBONE NOTE_DOCUMENT_END\n\n[^zeta-k]: NOTE_Z_BODY evidence NOTE_Z_END\n\n[^alpha-k]: NOTE_A_BODY evidence NOTE_A_END\n'


def native_parts(story='footnote', fmt='decimal'):
    w, r, p, ct = qa.opc.W, qa.opc.R, qa.opc.P, qa.opc.CT
    run = lambda text, size=20: '<w:r><w:rPr><w:sz w:val="' + str(size) + '"/></w:rPr><w:t>' + text + '</w:t></w:r>'
    reference = lambda identity: '<w:r><w:' + story + 'Reference w:id="' + identity + '"/></w:r>'
    body = '<w:p>' + run('[word-footnote-1]') + ''.join(reference(identity) for identity in ('41', '7', '41', '99')) + '</w:p><w:sectPr/>'
    doc = '<w:document xmlns:w="' + w + '"><w:body>' + body + '</w:body></w:document>'
    notes = ''
    for identity, role in [('99', 'B'), ('41', 'Z'), ('7', 'A')]:
        notes += '<w:' + story + ' w:id="' + identity + '"><w:p><w:r><w:' + story + 'Ref/></w:r>' + run('NOTE_' + role + '_BODY evidence', int(qa.SIZES[role] * 2)) + '</w:p></w:' + story + '>'
    rels = '<Relationships xmlns="' + p + '">' + ''.join('<Relationship Id="' + kind + '" Type="' + r + '/' + kind + '" Target="' + name + '"/>' for kind, name in [(story + 's', story + 's.xml'), ('settings', 'settings.xml'), ('styles', 'styles.xml')]) + '</Relationships>'
    settings = '<w:settings xmlns:w="' + w + '"><w:' + story + 'Pr><w:numFmt w:val="' + fmt + '"/><w:numStart w:val="7"/><w:numRestart w:val="continuous"/></w:' + story + 'Pr></w:settings>'
    return {'[Content_Types].xml': '<Types xmlns="' + ct + '"/>', '_rels/.rels': '<Relationships xmlns="' + p + '"><Relationship Id="root" Type="' + r + '/officeDocument" Target="word/document.xml"/></Relationships>', 'word/_rels/document.xml.rels': rels, 'word/document.xml': doc, 'word/' + story + 's.xml': '<w:' + story + 's xmlns:w="' + w + '">' + notes + '</w:' + story + 's>', 'word/settings.xml': settings, 'word/styles.xml': '<w:styles xmlns:w="' + w + '"/>'}


def synthetic_pdf(file, case='footnote-decimal', mutation=None):
    import pymupdf as pdf
    expected = qa.contract(case)
    font = pdf.Font(fontfile=str(FONT))
    document = pdf.open()
    for _ in range(3):
        page = document.new_page(width=595, height=842)
        page.insert_font(fontname='Owned', fontfile=str(FONT))
    def text(page, x, y, value, size):
        document[page].insert_text((x, y), value, fontname='Owned', fontsize=size, color=(1, 1, 1) if mutation == 'white-marker' and value == '7' else (0, 0, 0), render_mode=3 if mutation == 'hidden' and value == 'NOTE_DOCUMENT_END' else 0)
        return font.text_length(value, fontsize=size)
    text(0, 40, 45, 'NOTE_DOCUMENT_BEGIN [word-footnote-1] [^missing]', 11)
    text(2, 40, 760, 'NOTE_DOCUMENT_END', 11)
    for number in range(1, 73):
        if mutation == 'missing-body' and number == 33:
            continue
        page, row = divmod(number - 1, 24)
        text(page, 40, 250 + row * 9, 'FLOW_' + str(number).zfill(3) + '_KEEP', 6)
    refs, defs = [], {}
    for index, (left, right, role) in enumerate(qa.REFS):
        page, y = [(0, 100), (0, 145), (1, 100), (2, 100)][index]
        x = 40 + text(page, 40, y, left, 11)
        size = 11 if mutation == 'callout-size' and index == 0 else 7.7
        baseline = y if mutation == 'callout-baseline' and index == 0 else y - 3.85
        value = '8' if mutation == 'wrong-number' and index == 0 else expected['numbers'][role]
        width = text(page, x, baseline, value, size)
        text(page, x + width, y, right, 11)
        refs.append({'role': role, 'page': page, 'point': (x, baseline), 'rect': pdf.Rect(x, baseline - size, x + width, baseline + size * .25)})
    for role, y in [('Z', 550), ('A', 610), ('B', 670)]:
        value, size = expected['numbers'][role], qa.SIZES[role]
        width = text(2, 40, y, value, 10)
        body = 'NOTE_' + role + '_BODY note evidence NOTE_' + role + '_END'
        body_width = text(2, 40 + width + 4, y, body, size)
        defs[role] = {'page': 2, 'point': (40, y), 'rect': pdf.Rect(40, y - 10, 40 + width, y + 2.5)}
        if role == 'Z':
            x, baseline = 40 + width + 4 + body_width + 4, y - 3.5
            back_width = text(2, x, baseline, '[2]', 7)
            back = pdf.Rect(x, baseline - 7, x + back_width, baseline + 1.75)
    if mutation == 'technical-label':
        text(1, 40, 200, 'word-footnote-42', 11)
    for index, ref in enumerate(refs):
        dest = defs['A'] if mutation == 'wrong-target' and index == 0 else defs[ref['role']]
        rect = pdf.Rect(ref['rect'])
        if mutation == 'neighbor-hit' and index == 0:
            rect.x0 = 40
        link = {'kind': pdf.LINK_GOTO, 'from': rect, 'page': dest['page'], 'to': pdf.Point(dest['point'])}
        document[ref['page']].insert_link(link)
        if mutation == 'duplicate-link' and index == 0:
            document[ref['page']].insert_link(link)
    for role, definition in defs.items():
        ref = next(row for row in refs if row['role'] == role)
        document[2].insert_link({'kind': pdf.LINK_GOTO, 'from': definition['rect'], 'page': ref['page'], 'to': pdf.Point(ref['point'])})
    if mutation != 'missing-backlink':
        ref = refs[0] if mutation == 'wrong-backlink' else refs[2]
        document[2].insert_link({'kind': pdf.LINK_GOTO, 'from': back, 'page': ref['page'], 'to': pdf.Point(ref['point'])})
    if mutation == 'occluded-marker':
        document[0].draw_rect(refs[0]['rect'], color=None, fill=(1, 1, 1), overlay=True)
    document.save(file)
    document.close()


def nameless_subset_pdf(input_file, output_file, ambiguous=False):
    import pymupdf
    from fontTools import subset
    from fontTools.ttLib import TTFont
    with pymupdf.open(input_file) as document:
        xref = document[0].get_fonts(full=True)[0][0]
        program = document.extract_font(xref)[3]
        font = TTFont(io.BytesIO(program))
        postscript = font['name'].getDebugName(6)
        assert postscript and re.fullmatch(r'[A-Za-z0-9_-]+', postscript)
        options = subset.Options()
        options.retain_gids = True
        worker = subset.Subsetter(options=options)
        worker.populate(gids={char[1] for page in document for trace in page.get_texttrace() for char in trace['chars']})
        worker.subset(font)
        if 'name' in font:
            del font['name']
        encoded = io.BytesIO()
        font.save(encoded)
        kind, descendants = document.xref_get_key(xref, 'DescendantFonts')
        assert kind == 'array'
        children = re.findall(r'(\d+)\s+0\s+R', descendants)
        assert len(children) == 1
        kind, descriptor = document.xref_get_key(int(children[0]), 'FontDescriptor')
        assert kind == 'xref'
        # Construct a canonical nameless subset like the actual ReadMD font:
        # exact PDF BaseFont / descriptor FontName, not the original fixture's
        # full-name versus PostScript-name alias requiring its removed table.
        document.xref_set_key(xref, 'BaseFont', '/' + postscript)
        document.xref_set_key(int(children[0]), 'BaseFont', '/' + postscript)
        document.xref_set_key(int(descriptor.split()[0]), 'FontName', '/' + postscript)
        kind, stream = document.xref_get_key(int(descriptor.split()[0]), 'FontFile2')
        assert kind == 'xref'
        document.update_stream(int(stream.split()[0]), encoded.getvalue())
        if ambiguous:
            duplicate = document.get_new_xref()
            document.update_object(duplicate, document.xref_object(xref, compressed=False))
            kind, resources = document.xref_get_key(document[0].xref, 'Resources')
            assert kind == 'xref'
            document.xref_set_key(int(resources.split()[0]), 'Font/OwnedAmbiguous', str(duplicate) + ' 0 R')
        document.save(output_file)


def synthetic_cff_program(matrix=None, multiple=False, blank=False):
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.t2CharStringPen import T2CharStringPen
    builder = FontBuilder(1000, isTTF=False)
    builder.setupGlyphOrder(['.notdef', 'A'])
    builder.setupCharacterMap({65: 'A'})
    builder.setupHorizontalMetrics({name: (600, 0) for name in ['.notdef', 'A']})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({'familyName': 'Owned CFF', 'styleName': 'Regular',
                            'psName': 'OwnedCFF-Regular', 'fullName': 'Owned CFF Regular'})
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    chars = {}
    for name in ['.notdef', 'A']:
        pen = T2CharStringPen(600, None)
        if name == 'A' and not blank:
            pen.moveTo((60, 0))
            pen.lineTo((300, 700))
            pen.lineTo((540, 0))
            pen.closePath()
        chars[name] = pen.getCharString()
    builder.setupCFF('OwnedCFF-Regular', {'FullName': 'Owned CFF Regular',
                     'FamilyName': 'Owned CFF', 'Weight': 'Regular'}, chars, {})
    cff = builder.font['CFF '].cff
    if matrix is not None:
        cff.topDictIndex[0].FontMatrix = matrix
    if multiple:
        cff.fontNames.append('OwnedCFF-Other')
        cff.topDictIndex.append(copy.deepcopy(cff.topDictIndex[0]))
    encoded = io.BytesIO()
    cff.compile(encoded, builder.font)
    return encoded.getvalue()


def synthetic_cff_pdf(file, ambiguous=False, blank=False):
    import pymupdf
    program = synthetic_cff_program(blank=blank)
    with pymupdf.open() as document:
        page = document.new_page(width=595, height=842)
        xref = page.insert_font(fontname='OwnedCFF', fontbuffer=program)
        page.insert_text((40, 145), 'A', fontname='OwnedCFF', fontsize=100)
        if ambiguous:
            duplicate = document.get_new_xref()
            document.update_object(duplicate, document.xref_object(xref, compressed=False))
            kind, resources = document.xref_get_key(page.xref, 'Resources')
            assert kind == 'xref'
            document.xref_set_key(int(resources.split()[0]), 'Font/OwnedAmbiguous', str(duplicate) + ' 0 R')
        document.save(file)


class Controls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
    def tearDown(self):
        self.temp.cleanup()
    def pdf(self, mutation=None, case='footnote-decimal'):
        file = self.root / ((mutation or case) + '.pdf')
        synthetic_pdf(file, case, mutation)
        return file
    def test01_five_formats_real_embedded_glyphs_all_eight_links_and_cross_page_repeat(self):
        for case in qa.CASES:
            result = qa.pdf(self.pdf(case=case), case)
            self.assertEqual(result['pages'], 3)
            self.assertEqual(len(result['links']), 8)
            self.assertEqual(result['visibleReferences'], [qa.contract(case)['numbers'][role] for _, _, role in qa.REFS])
            self.assertTrue(all(row['bytes'] > 0 for row in result['fontPrograms']))
    def test02_wrong_visible_number_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'Wrong visible reference number'):
            qa.pdf(self.pdf('wrong-number'), 'footnote-decimal')
    def test03_wrong_definition_destination_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'actual inline marker origin'):
            qa.pdf(self.pdf('wrong-target'), 'footnote-decimal')
    def test04_missing_second_backlink_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'exactly one real complete link'):
            qa.pdf(self.pdf('missing-backlink'), 'footnote-decimal')
    def test05_wrong_second_backlink_destination_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'wrong page|actual inline marker origin'):
            qa.pdf(self.pdf('wrong-backlink'), 'footnote-decimal')
    def test06_duplicate_fragment_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'exactly one real complete link'):
            qa.pdf(self.pdf('duplicate-link'), 'footnote-decimal')
    def test07_unrelated_body_ink_inside_link_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'unrelated visible glyph'):
            qa.pdf(self.pdf('neighbor-hit'), 'footnote-decimal')
    def test08_technical_identity_leak_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'Technical source identity'):
            qa.pdf(self.pdf('technical-label'), 'footnote-decimal')
    def test09_missing_real_body_paragraph_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'Visible body text omitted'):
            qa.pdf(self.pdf('missing-body'), 'footnote-decimal')
    def test10_superscript_size_and_baseline_are_real(self):
        for mutation, reason in [('callout-size', '0.7 of11pt'), ('callout-baseline', 'superscript rise')]:
            with self.assertRaisesRegex(AssertionError, reason):
                qa.pdf(self.pdf(mutation), 'footnote-decimal')
    def test11_hidden_visible_corpus_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, 'Hidden/transparent text'):
            qa.pdf(self.pdf('hidden'), 'footnote-decimal')
    def test12_actual_native_settings_ids_references_and_author_sizes_both_stories(self):
        for story, fmt, case in [('footnote', 'decimal', 'footnote-decimal'), ('endnote', 'upperRoman', 'endnote-upperRoman')]:
            file = self.root / (story + '.docx')
            with zipfile.ZipFile(file, 'x') as archive:
                for name, value in native_parts(story, fmt).items():
                    archive.writestr(name, value)
            value = qa.native(file, case)
            self.assertEqual(value['references'], ['Z', 'A', 'Z', 'B'])
            self.assertFalse(value['sourcePayloadUsedAsNativeEvidence'])
    def test13_native_wrong_start_and_same_id_wrong_definition_rejected(self):
        for mutation in ('start', 'identity'):
            parts = native_parts()
            if mutation == 'start':
                parts['word/settings.xml'] = parts['word/settings.xml'].replace('w:val="7"', 'w:val="1"')
            else:
                parts['word/document.xml'] = parts['word/document.xml'].replace('w:id="41"', 'w:id="7"')
            file = self.root / (mutation + '.docx')
            with zipfile.ZipFile(file, 'x') as archive:
                for name, value in parts.items():
                    archive.writestr(name, value)
            with self.assertRaises(AssertionError):
                qa.native(file, 'footnote-decimal')
    def test14_source_first_use_early_out_of_order_definition_and_unparsed_literal(self):
        file = self.root / 'source.md'
        file.write_text(source_text(), encoding='utf-8')
        self.assertEqual(qa.source(file, 'footnote-decimal')['references'], ['Z', 'A', 'Z', 'B'])
    def test15_source_duplicate_and_missing_definition_fail(self):
        for label, text in [('duplicate', source_text() + '\n[^zeta-k]: EXTRA\n'), ('missing', source_text().replace('[^alpha-k]:', 'NOT_A_DEFINITION:'))]:
            file = self.root / (label + '.md')
            file.write_text(text, encoding='utf-8')
            with self.assertRaises(AssertionError):
                qa.source(file, 'footnote-decimal')
    def test16_unknown_case_and_nonfinite_destination_fail_closed(self):
        with self.assertRaises(AssertionError):
            qa.contract('invented')
        with self.assertRaises(AssertionError):
            qa.target_is({'page': 0, 'to': [float('nan'), 0]}, [{'page': 0, 'origin': [0, 0]}])
    def test17_white_marker_is_not_visible_numbering(self):
        with self.assertRaisesRegex(AssertionError, 'White/unproved text paint'):
            qa.pdf(self.pdf('white-marker'), 'footnote-decimal')
    def test18_later_white_paint_cannot_hide_a_real_number_glyph(self):
        with self.assertRaisesRegex(AssertionError, 'occluded or absent'):
            qa.pdf(self.pdf('occluded-marker'), 'footnote-decimal')
    def test19_real_nameless_subset_retains_actual_gid_outline_and_link_proof(self):
        file = self.root / 'nameless.pdf'
        nameless_subset_pdf(self.pdf(), file)
        result = qa.pdf(file, 'footnote-decimal')
        self.assertEqual(result['visibleReferences'], ['7', '8', '7', '9'])
        self.assertEqual(len(result['links']), 8)
        self.assertTrue(all(not row['programHasNameTable'] for row in result['fontPrograms']))
        self.assertTrue(all(row['xref'] > 0 and row['baseFont'] and row['sha256'] for row in result['fontPrograms']))
        self.assertGreater(result['markerPixelsChecked'], 0)
    def test20_nameless_program_cannot_bind_unrelated_extracted_font_name(self):
        from fontTools.ttLib import TTFont
        font = TTFont(str(FONT))
        del font['name']
        with self.assertRaisesRegex(AssertionError, 'resource/extracted font binding mismatch'):
            qa.program_font_aliases('AAAAAA+SourceSansPro-Regular', 'BBBBBB+UnrelatedFont', font)
    def test21_duplicate_real_font_xref_alias_is_rejected_even_with_same_program(self):
        import pymupdf
        file = self.root / 'ambiguous-font.pdf'
        nameless_subset_pdf(self.pdf(), file, ambiguous=True)
        with pymupdf.open(file) as document:
            self.assertEqual(len(document[0].get_fonts(full=True)), 2)
        with self.assertRaisesRegex(AssertionError, 'Ambiguous actual font resource'):
            qa.pdf(file, 'footnote-decimal')
    def test22_valid_markdown_entities_preserve_visible_literals_without_creating_refs(self):
        variants = [('[word-footnote-1]', '[^missing]'),
                    ('&#91;word&#45;footnote&#45;1&#93;', '&#91;&#94;missing&#93;'),
                    ('&#x5b;word&#x2d;footnote&#x2d;1&#x5d;', '&#x5b;&#x5e;missing&#x5d;'),
                    ('&lbrack;word-footnote-1&rbrack;', '&lbrack;&Hat;missing&rbrack;')]
        for index, (literal, missing) in enumerate(variants):
            text = source_text().replace('[word-footnote-1]', literal).replace('[^missing]', missing)
            text += '\nENTITY_LITERAL &#91;&#94;zeta-k&#93;\n\n&#91;&#94;alpha-k&#93;: ordinary entity text\n'
            file = self.root / ('entity-' + str(index) + '.md')
            file.write_text(text, encoding='utf-8')
            self.assertEqual(qa.source(file, 'footnote-decimal')['references'], ['Z', 'A', 'Z', 'B'])
    def test23_entity_encoding_does_not_excuse_actual_lost_literal_characters(self):
        for index, text in enumerate([source_text().replace('[word-footnote-1]', '&#91;word&#45;footnote&#45;2&#93;'),
                                      source_text().replace('[^missing]', '&#91;missing&#93;'),
                                      source_text().replace('[word-footnote-1]', '&amp;#91;word-footnote-1&amp;#93;')]):
            file = self.root / ('lost-entity-' + str(index) + '.md')
            file.write_text(text, encoding='utf-8')
            with self.assertRaisesRegex(AssertionError, 'Literal same-name|undefined literal'):
                qa.source(file, 'footnote-decimal')
    def test24_entity_text_cannot_replace_a_real_reference_in_the_grammar(self):
        file = self.root / 'encoded-real-reference.md'
        file.write_text(source_text().replace('RZAGAIN[^zeta-k]', 'RZAGAIN&#91;&#94;zeta-k&#93;'), encoding='utf-8')
        with self.assertRaisesRegex(AssertionError, 'Reference identity/order changed'):
            qa.source(file, 'footnote-decimal')
    def test25_real_cff_pdf_uses_actual_gid_charstring_bounds_and_visible_pixels(self):
        import pymupdf
        file = self.root / 'real-cff.pdf'
        synthetic_cff_pdf(file)
        with pymupdf.open(file) as document:
            pages, fonts = qa.inventory(document)
            self.assertEqual(len(fonts), 1)
            self.assertEqual(fonts[0]['programFormat'], 'cff1')
            self.assertEqual(fonts[0]['cffName'], 'OwnedCFF-Regular')
            self.assertEqual(fonts[0]['fontMatrix'], [.001, 0, 0, .001, 0, 0])
            self.assertEqual(fonts[0]['charsetGlyphs'], 2)
            glyph = pages[0]['glyphs'][0]
            self.assertEqual((glyph['char'], glyph['gid']), ('A', 1))
            self.assertEqual(glyph['ink'], [46, 75, 94, 145])
            image = document[0].get_pixmap(alpha=False)
            # This point is inside the independently specified triangle, so
            # the parsed bounds are tied to actual embedded-font paint.
            at = (120 * image.width + 70) * image.n
            self.assertTrue(any(c < 240 for c in image.samples[at:at + 3]))
    def test26_cff_nonuniform_shear_translation_and_negative_matrices_fail_closed(self):
        for matrix in [[.001, 0, 0, .002, 0, 0], [.001, .01, 0, .001, 0, 0],
                       [.001, 0, 0, .001, 1, 0], [-.001, 0, 0, -.001, 0, 0]]:
            with self.assertRaisesRegex(AssertionError, 'Unproved CFF FontMatrix'):
                qa.read_font_program(synthetic_cff_program(matrix=matrix))
    def test27_cff_multiple_font_name_index_is_not_an_alias_license(self):
        with self.assertRaisesRegex(AssertionError, 'exactly one font'):
            qa.read_font_program(synthetic_cff_program(multiple=True))
    def test28_cff_duplicate_actual_resource_is_rejected(self):
        import pymupdf
        file = self.root / 'ambiguous-cff.pdf'
        synthetic_cff_pdf(file, ambiguous=True)
        with pymupdf.open(file) as document:
            with self.assertRaisesRegex(AssertionError, 'Ambiguous actual font resource'):
                qa.inventory(document)
    def test29_cff_nonblank_character_requires_real_charstring_outline(self):
        import pymupdf
        file = self.root / 'blank-cff.pdf'
        synthetic_cff_pdf(file, blank=True)
        with pymupdf.open(file) as document:
            with self.assertRaisesRegex(AssertionError, 'Nonblank character has no visible outline'):
                qa.inventory(document)
    def test30_unproved_font_format_and_cff_unbounded_scale_are_rejected(self):
        for raw in [b'not a font', b'\x02\x00\x05\x04', b'\x01\x00\xff\x04']:
            with self.assertRaisesRegex(AssertionError, 'Unproved embedded font format'):
                qa.read_font_program(raw)
        with self.assertRaisesRegex(AssertionError, 'Unproved CFF unitsPerEm'):
            qa.read_font_program(synthetic_cff_program(matrix=[.00001, 0, 0, .00001, 0, 0]))


if __name__ == '__main__':
    unittest.main()
