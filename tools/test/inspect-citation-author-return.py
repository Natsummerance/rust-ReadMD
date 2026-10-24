"""Finite independent OPC/selected-XHTML/PDF oracle for authored citation notes.

Only local standard-library package mutations are performed. No application or
publication engine is started. Synthetic controls are never product evidence.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import re
import sys
import zipfile
from urllib.parse import unquote, urlsplit
import xml.etree.ElementTree as ET


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


base = load('citation_author_native', 'inspect-author-inline.py')
pdf_base = load('citation_author_links', 'inspect-mcp-publication-closure.py')
fonts = load('citation_author_fonts', 'inspect-math-document-defaults.py')
W, C = base.W, 'urn:readmd:citations:1'
NS = {'w': W, 'c': C}
PART = 'customXml/readmd-citations.xml'
MAX_BYTES = 16 * 1024 * 1024
MARKERS = ['P07_AUTHOR_BEGIN', 'P07_AUTHOR_END', 'P07_TAIL_KEEP', 'MANUAL_CURRENT', 'FIRST_TITLE', 'SECOND_TITLE']


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def one(node, tag):
    rows = node.findall(tag, NS)
    assert len(rows) == 1, ('Missing or duplicate element', tag)
    return rows[0]


def manifest(package):
    root = package.node(PART)
    assert root.tag == '{' + C + '}citations' and root.get('version') == '1'
    resources = json.loads(one(root, 'c:resources').text)
    assert resources['version'] == 1
    records, identities = [], set()
    for node in root.findall('c:record', NS):
        row = {**node.attrib, 'source': one(node, 'c:source').text,
               'original_source': one(node, 'c:original_source').text}
        assert row['id'] and row['id'] not in identities
        identities.add(row['id'])
        assert row['generated'] in ('true', 'false') and row['kind'] in ('citation', 'bibliography')
        assert row['render_context_version'] == '2'
        for field in ('render_sha256', 'binding'):
            assert re.fullmatch('[0-9a-f]{64}', row[field]), field
        note = node.find('c:note', NS)
        row['note'] = dict(note.attrib) if note is not None else None
        records.append(row)
    return {'records': records, 'resources': resources, 'sha256': digest(package.parts[PART])}


def control(package, record):
    prefix = 'readmd:bibliography:v1:' if record['kind'] == 'bibliography' else 'readmd:citation:v1:'
    rows = [node for node in package.node(record['part']).iter(base.q('sdt'))
            if (tag := node.find('w:sdtPr/w:tag', NS)) is not None and base.val(tag) == prefix + record['id']]
    assert len(rows) == 1, 'The manifest must select one actual native control'
    return rows[0]


def text(node):
    return ''.join(child.text or '' for child in node.iter(base.q('t')))


def characters(row):
    result = []
    for run in row['runs']:
        props = [run['halfPoints'], run['halfPointsCs'], run['cs'], run['rtl']]
        result.extend([[char, *props] for char in run['text']])
    assert ''.join(char[0] for char in result) == row['text']
    return result


def inspect_docx(file, story):
    assert story in ('footnote', 'endnote')
    package = base.Package(file)
    actual = base.inspect_docx(file)
    model = manifest(package)
    body = package.node(package.document)
    note_data = actual['notes'][story]
    refs = note_data['references']
    assert len(refs) == 4 and len(set(refs)) == 3 and refs[0] != refs[1] != refs[2] and refs[2] == refs[3], 'Actual repeated manual note topology changed'
    ordered = list(dict.fromkeys(refs))
    defs = {row['id']: row for row in note_data['definitions']}
    assert set(defs) == set(ordered), 'Missing or unreferenced normal native note'
    other = 'endnote' if story == 'footnote' else 'footnote'
    assert not actual['notes'].get(other, {}).get('references', [])
    body_rows = [row for row in actual['paragraphs'] if row['story'] == 'body']
    notes = []
    for identity in ordered:
        rows = [row for row in actual['paragraphs'] if row['story'] == story + ':' + identity]
        assert len(rows) == 1, 'The finite note is exactly one current paragraph'
        notes.append({'text': rows[0]['text'], 'characters': characters(rows[0])})
    visible = ''.join(row['text'] for row in body_rows) + ''.join(row['text'] for row in notes)
    for marker in MARKERS:
        assert visible.count(marker) == 1, ('Missing/duplicate native body/bibliography marker', marker)
    assert visible.count('Alpha') == 2 and 'Beta' in visible
    assert [base.val(node, 'id') for node in body.iter(base.q(story + 'Reference'))] == refs
    return {'kind': 'citation-author-native', 'file': str(file), 'sha256': digest(package.raw), 'bytes': len(package.raw),
            'story': story, 'body': [{'text': row['text'], 'characters': characters(row)} for row in body_rows],
            'notes': notes, 'references': [ordered.index(identity) for identity in refs], 'manifest': model,
            'parts': actual['parts'], 'auxiliaryParts': actual['auxiliaryParts'],
            'nativeSourcePayloadUsedAsPaintEvidence': False, 'applicationRoundtripCertified': False}


def mutate(file, output, story, mode):
    before = Path(file).read_bytes()
    package = base.Package(file)
    original = dict(package.parts)
    data = manifest(package)
    kind_part = package.related(story + 's')
    assert kind_part is not None
    generated = [row for row in data['records'] if row['kind'] == 'citation' and row['generated'] == 'true' and row['part'] == 'word/document.xml']
    assert len(generated) == 2 and all(row['source'] == '[@42, p. 12]' for row in generated)
    styles_part = package.related('styles')
    styles = package.node(styles_part)
    style_id = 'FootnoteText' if story == 'footnote' else 'EndnoteText'
    style_rows = [node for node in styles.findall('w:style', NS) if base.val(node, 'styleId') == style_id]
    assert len(style_rows) == 1
    rpr = one(style_rows[0], 'w:rPr')
    for axis in ('sz', 'szCs'):
        node = one(rpr, 'w:' + axis)
        assert base.val(node) == '20', 'Fixture must start from actual 10pt note template'
        node.set(base.q('val'), '40')
    package.parts[styles_part] = ET.tostring(styles, encoding='utf-8', xml_declaration=True)
    if mode in ('bad-signature', 'prose'):
        root = package.node(PART)
        if mode == 'bad-signature':
            record = next(node for node in root.findall('c:record', NS) if node.get('id') == generated[1]['id'])
            record.set('render_sha256', '0' * 64)
        else:
            pair = generated[0]['note']
            for record in root.findall('c:record', NS):
                n = record.find('c:note', NS)
                if n is not None and dict(n.attrib) == pair:
                    one(record, 'c:source').text = '@42 [p. 12]'
        package.parts[PART] = ET.tostring(root, encoding='utf-8', xml_declaration=True)
    elif mode == 'duplicate':
        row = generated[1]
        selected = control(package, row)
        root = package.node(row['part'])
        parents = {child: parent for parent in root.iter() for child in parent}
        parent = parents[selected]
        parent.insert(list(parent).index(selected) + 1, copy.deepcopy(selected))
        package.parts[row['part']] = ET.tostring(root, encoding='utf-8', xml_declaration=True)
    elif mode == 'extra-prose':
        root = package.node(kind_part)
        note = next(node for node in root if base.val(node, 'id') == generated[0]['note']['id'])
        paragraph = one(note, 'w:p')
        run = ET.SubElement(paragraph, base.q('r'))
        ET.SubElement(run, base.q('t')).text = ' P07_UNBOUND_CURRENT'
        package.parts[kind_part] = ET.tostring(root, encoding='utf-8', xml_declaration=True)
    else:
        assert mode == 'template'
    assert set(package.parts) == set(original)
    changed = [name for name in original if package.parts[name] != original[name]]
    assert styles_part in changed and set(changed) <= {styles_part, PART, package.document, kind_part}
    if mode == 'template':
        assert changed == [styles_part]
        assert all(package.parts[name] == raw for name, raw in original.items() if name != styles_part)
    base.save_package(package.parts, output)
    assert Path(file).read_bytes() == before
    receipt = {'mode': mode, 'story': story, 'inputSha256': digest(before), 'outputSha256': digest(Path(output).read_bytes()),
               'changedParts': [{'path': name, 'beforeSha256': digest(original[name]), 'afterSha256': digest(package.parts[name])} for name in changed],
               'unchangedParts': [{'path': name, 'sha256': digest(raw), 'bytes': len(raw)} for name, raw in original.items() if name not in changed],
               'manifestUnchanged': original[PART] == package.parts[PART], 'applicationRoundtripCertified': False}
    if mode == 'template':
        receipt['templateAxisHalfPoints'] = {'before': 20, 'after': 40}
        receipt['inspection'] = inspect_docx(output, story)
    return receipt


def compare_native(original, native, story, edited=False):
    left, right = inspect_docx(original, story), inspect_docx(native, story)
    expected = copy.deepcopy(left)
    if edited:
        note = expected['notes'][1]
        assert note['text'].endswith(':12)') and note['text'].count(':12)') == 1
        at = len(note['characters']) - 4
        assert ''.join(row[0] for row in note['characters'][at:]) == ':12)'
        props = note['characters'][at + 1][1:]
        note['characters'][at + 1:at + 3] = [[char, *props] for char in '111']
        note['text'] = note['text'][:-4] + ':111)'
        assert note['characters'][0][0] == ' '
        for char in note['characters'][1:]:
            char[1] = 23
    for key in ('body', 'notes', 'references'):
        assert expected[key] == right[key], ('Current complete native text/author axes/topology changed', key)
    records = right['manifest']['records']
    assert len(records) == 4 and all(row['generated'] == 'false' for row in records)
    citations = [row for row in records if row['kind'] == 'citation' and row['part'] == 'word/' + story + 's.xml']
    assert len(citations) == 2 and all(row['note']['end'] == str(story == 'endnote').lower() for row in citations)
    assert sorted(row['source'] for row in citations) == sorted(['[@42, p. 12]', '[@42, p. 111]' if edited else '[@42, p. 12]'])
    assert len([row for row in records if row['kind'] == 'bibliography']) == 1
    assert not right['manifest']['resources']['manual_bibliography']
    return {'passed': True, 'story': story, 'sourceSha256': left['sha256'], 'nativeSha256': right['sha256'],
            'allCurrentTextAxesAndReferenceTopologyCompared': True, 'generated': False, 'requiresSeparateActualApiContextProof': True,
            'nativeRecords': len(records), 'authorNoteCitationRecords': 2, 'editedSecondOnly': edited,
            'oldGeneratedProvenancePreserved': False, 'applicationRoundtripCertified': False}


def close(left, right):
    assert math.isfinite(left) and math.isfinite(right) and abs(left - right) <= .002, (left, right)


def compare_pdf(actual_file, control_file):
    import pymupdf as pdf
    glyphs = paths = links = 0
    with pdf.open(actual_file) as actual, pdf.open(control_file) as control_pdf:
        assert len(actual) == len(control_pdf) > 0
        for a, b in zip(actual, control_pdf):
            assert list(a.mediabox) == list(b.mediabox) and list(a.cropbox) == list(b.cropbox) and a.rotation == b.rotation
            assert not a.get_images() and not b.get_images(), 'No raster text substitute is admitted'
            assert fonts.pdf_font_shapes(actual, a) == fonts.pdf_font_shapes(control_pdf, b), 'Actual embedded font programs changed'
            def flat(page):
                out = []
                for trace in page.get_texttrace():
                    assert trace['type'] in (0, 1, 2) and 0 < trace.get('opacity', 1) <= 1, 'Hidden text is not admitted in this no-math corpus'
                    identity = {key: trace.get(key) for key in ('font', 'flags', 'type', 'wmode', 'bidi_lvl', 'bidi_dir', 'color', 'colorspace', 'opacity', 'dir', 'ascender', 'descender', 'linewidth')}
                    for char in trace['chars']:
                        assert len(char) == 4
                        out.append((identity, trace['size'], char))
                return out
            left, right = flat(a), flat(b)
            assert len(left) == len(right) > 0
            for (p, size, char), (q, other_size, other) in zip(left, right):
                assert p == q and char[:2] == other[:2], 'Unicode/GID/font/paint identity changed'
                close(size, other_size)
                for xs, ys, length in ((char[2], other[2], 2), (char[3], other[3], 4)):
                    assert len(xs) == len(ys) == length
                    for x, y in zip(xs, ys):
                        close(x, y)
                glyphs += 1
            left, right = a.get_drawings(), b.get_drawings()
            assert len(left) == len(right)
            for p, q in zip(left, right):
                for key in ('fill', 'color', 'width', 'closePath', 'type', 'lineCap', 'lineJoin', 'fill_opacity', 'stroke_opacity'):
                    assert p.get(key) == q.get(key), key
                for x, y in zip(p['rect'], q['rect']):
                    close(x, y)
                assert len(p['items']) == len(q['items'])
                for x, y in zip(p['items'], q['items']):
                    x, y = fonts.native.base.drawing_item(x), fonts.native.base.drawing_item(y)
                    assert x['operation'] == y['operation'] and x['scalar'] == y['scalar']
                    assert len(x['coordinates']) == len(y['coordinates'])
                    for xs, ys in zip(x['coordinates'], y['coordinates']):
                        assert len(xs) == len(ys)
                        for px, py in zip(xs, ys):
                            close(px, py)
                paths += 1
            def normalized_links(document, page):
                rows = []
                for link in page.get_links():
                    row = pdf_base.pdf_link_record(document, link, pdf)
                    if row['kind'] == 4:
                        assert 'page' in row and 'to' in row
                        row['kind'] = 1
                        row.pop('name', None)
                        row.pop('nameddest', None)
                    rows.append(row)
                return rows
            left, right = normalized_links(actual, a), normalized_links(control_pdf, b)
            assert len(left) == len(right)
            for p, q in zip(left, right):
                for key in ('kind', 'page', 'uri', 'toDefaultCoordinates'):
                    assert p.get(key) == q.get(key), key
                for key in ('from', 'to'):
                    assert (key in p) == (key in q)
                    if key in p:
                        assert len(p[key]) == len(q[key])
                        for x, y in zip(p[key], q[key]):
                            if x is None or y is None:
                                assert x == y
                            else:
                                close(x, y)
                links += 1
        assert glyphs > 0
        return {'passed': True, 'pages': len(actual), 'glyphs': glyphs, 'allPaths': paths, 'links': links,
                'allVisibleFontProgramsGlyphsPositionsPaintAndLinkDestinationsCompared': True, 'coordinateTolerancePt': .002,
                'actualSha256': digest(Path(actual_file).read_bytes()), 'controlSha256': digest(Path(control_file).read_bytes())}


def assert_not_hidden(attributes):
    assert 'hidden' not in attributes, 'Hidden selected note content'
    style = re.sub(r'\s+', '', attributes.get('style', '')).lower()
    assert not re.search(r'(?:^|;)(?:display:none|visibility:(?:hidden|collapse)|opacity:0(?:\.0+)?)(?:!important)?(?:;|$)', style), 'Hidden selected note content'


def inspect_selected_html(file):
    class Visible(base.RenderedHtml):
        def handle_starttag(self, tag, attrs):
            assert_not_hidden(dict(attrs))
            super().handle_starttag(tag, attrs)
    raw = Path(file).read_bytes()
    assert 0 < len(raw) <= MAX_BYTES
    parser = Visible()
    parser.feed(raw.decode('utf-8-sig'))
    return {'kind': 'author-inline-rendered-html', 'sha256': digest(raw), 'texts': parser.texts, 'keys': parser.keys,
            'links': parser.links, 'identities': parser.identities, 'browserComputedStyleCertified': False}


def inspect_selected_epub(file):
    value = base.inspect_epub(file)
    html_ns, epub_ns = 'http://www.w3.org/1999/xhtml', 'http://www.idpf.org/2007/ops'
    navigation_parts = set(value['navigationParts'])
    with zipfile.ZipFile(file) as archive:
        for part in sorted(set(value['bodyParts']) | navigation_parts):
            root = base.xhtml(archive.read(part))
            bodies = root.findall('{' + html_ns + '}body')
            assert len(bodies) == 1
            assert_not_hidden(root.attrib)
            for node in bodies[0].iter():
                attributes = dict(node.attrib)
                if (part in navigation_parts and node.tag == '{' + html_ns + '}nav'
                        and set(node.get('{' + epub_ns + '}type', '').split()) == {'landmarks'}
                        and attributes.get('hidden') in ('', 'hidden')):
                    del attributes['hidden']
                assert_not_hidden(attributes)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest='action', required=True)
    for action in ('inspect', 'mutate', 'compare-native'):
        command = actions.add_parser(action)
        command.add_argument('--input', required=True)
        command.add_argument('--story', choices=('footnote', 'endnote'), required=True)
        if action == 'mutate':
            command.add_argument('--output', required=True)
            command.add_argument('--mode', choices=('template', 'bad-signature', 'prose', 'duplicate', 'extra-prose'), required=True)
        elif action == 'compare-native':
            command.add_argument('--native', required=True)
            command.add_argument('--edited', action='store_true')
    command = actions.add_parser('compare-pdf')
    command.add_argument('--actual', required=True)
    command.add_argument('--control', required=True)
    for action in ('html', 'epub', 'rendered-html'):
        actions.add_parser(action).add_argument('file')
    args = parser.parse_args()
    if args.action == 'inspect':
        value = inspect_docx(args.input, args.story)
    elif args.action == 'mutate':
        value = mutate(args.input, args.output, args.story, args.mode)
    elif args.action == 'compare-native':
        value = compare_native(args.input, args.native, args.story, args.edited)
    elif args.action == 'compare-pdf':
        value = compare_pdf(args.actual, args.control)
    else:
        value = {'html': base.inspect_html, 'epub': inspect_selected_epub, 'rendered-html': inspect_selected_html}[args.action](args.file)
    print(json.dumps(value, ensure_ascii=True, allow_nan=False))


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    main()
