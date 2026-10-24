"""Independent finite note-numbering oracle. No product or renderer process.

Expected numbers and lexical anchors are fixed below before actual execution.
PDF checks read real glyph programs, visible paint and GoTo destinations. Native
checks use actual Word stories/settings, never a ReadMD provenance payload.
"""
import argparse
import hashlib
import html
import importlib.util
import io
import json
import math
from pathlib import Path
import re
import sys


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


opc = load('note_number_native', 'inspect-author-inline.py')
links_base = load('note_number_links', 'inspect-mcp-publication-closure.py')
MAX_BYTES = 16 * 1024 * 1024
MAX_PAGES = 16
EPS = .1
SIZES = {'Z': 10, 'A': 11.5, 'B': 10}
REFS = [('RZFIRST', 'RZONE', 'Z'), ('RAFIRST', 'RAONE', 'A'),
        ('RZAGAIN', 'RZTWO', 'Z'), ('RBFIRST', 'RBONE', 'B')]
CASES = {
    'footnote-decimal': ('footnote', 'decimal', ['7', '8', '9']),
    'endnote-upperRoman': ('endnote', 'upperRoman', ['VII', 'VIII', 'IX']),
    'footnote-lowerRoman': ('footnote', 'lowerRoman', ['vii', 'viii', 'ix']),
    'footnote-lowerLetter': ('footnote', 'lowerLetter', ['g', 'h', 'i']),
    'footnote-upperLetter': ('footnote', 'upperLetter', ['G', 'H', 'I']),
}


def contract(case):
    assert case in CASES, 'Unknown finite numbering fixture'
    story, fmt, numbers = CASES[case]
    return {'case': case, 'story': story, 'format': fmt, 'start': 7,
            'numbers': dict(zip(('Z', 'A', 'B'), numbers)),
            'references': ['Z', 'A', 'Z', 'B'], 'literal': '[word-footnote-1]'}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def bounded_file(file):
    file = Path(file)
    assert file.is_file() and not file.is_symlink()
    assert 0 < file.stat().st_size <= MAX_BYTES, 'Input byte budget exceeded'
    return file


def source(file, case):
    file = bounded_file(file)
    text = file.read_text(encoding='utf-8-sig')
    expected = contract(case)
    visible = re.sub(r'\\([\\`*{}\[\]()#+.!_>~-])', r'\1', text)
    # CommonMark character references require a semicolon. Decode exactly one
    # layer for visible literals only; definitions/references below keep raw
    # Markdown so an entity-escaped [^label] cannot acquire note identity.
    visible = re.sub(r'&(?:\#[xX][0-9A-Fa-f]{1,6}|\#[0-9]{1,7}|[A-Za-z][A-Za-z0-9]{1,31});',
                     lambda match: html.unescape(match[0]), visible)
    assert visible.count(expected['literal']) == 1, 'Literal same-name body text changed'
    assert visible.count('[^missing]') == 1, 'Unparsed undefined literal changed'
    definitions = list(re.finditer(r'^\[\^([^\]\r\n]+)\]:([^\r\n]*(?:\r?\n(?:[ \t]+[^\r\n]*|(?=\r?$)))*)', text, re.M))
    assert len(definitions) == 3, 'Expected three actual note definitions'
    labels, clean = {}, text
    for row in reversed(definitions):
        clean = clean[:row.start()] + (' ' * (row.end() - row.start())) + clean[row.end():]
    for row in definitions:
        roles = [role for role in SIZES if 'NOTE_' + role + '_BODY' in row[2]]
        assert len(roles) == 1 and row[1] not in labels, 'Duplicate/missing definition identity'
        labels[row[1]] = roles[0]
    refs = [label for label in re.findall(r'(?<!\\)\[\^([^\]\r\n]+)\](?!:)', clean) if label != 'missing']
    assert all(label in labels for label in refs), 'Unresolved fixture reference'
    assert [labels[label] for label in refs] == expected['references'], 'Reference identity/order changed'
    for left, right, _ in REFS:
        assert text.count(left) == text.count(right) == 1
    for ordinal in range(1, 73):
        assert text.count('FLOW_' + str(ordinal).zfill(3) + '_KEEP') == 1, 'Body paragraph source changed'
    assert text.count('NOTE_' + 'Z_BODY') == 1
    return {'passed': True, 'kind': 'note-source', 'sha256': digest(file.read_bytes()),
            'labels': labels, 'references': expected['references'], 'literalPreserved': True}


def native(file, case):
    expected, package = contract(case), opc.Package(bounded_file(file))
    story = expected['story']
    document = package.node(package.document)
    body = document.find('w:body', opc.NS)
    assert body is not None
    text = ''.join(body.itertext())
    assert text.count(expected['literal']) == 1, 'Native literal text changed'
    part = package.related(story + 's')
    assert part, 'Actual native note story missing'
    notes = package.node(part)
    ids, roles = {}, {}
    for node in notes.findall('w:' + story, opc.NS):
        if opc.val(node, 'type') in ('separator', 'continuationSeparator', 'continuationNotice'):
            continue
        identity = opc.val(node, 'id')
        assert identity is not None and identity not in ids, 'Duplicate native definition'
        current = ''.join(node.itertext())
        found = [role for role in SIZES if 'NOTE_' + role + '_BODY' in current]
        assert len(found) == 1 and found[0] not in roles, 'Native definition identity missing'
        assert len(node.findall('.//w:' + story + 'Ref', opc.NS)) == 1, 'Native display marker missing'
        ids[identity], roles[found[0]] = found[0], identity
    assert len(ids) == 3
    references = [opc.val(node, 'id') for node in body.findall('.//w:' + story + 'Reference', opc.NS)]
    assert all(identity in ids for identity in references)
    assert [ids[identity] for identity in references] == expected['references']
    other = 'endnote' if story == 'footnote' else 'footnote'
    assert not body.findall('.//w:' + other + 'Reference', opc.NS), 'Wrong native story'
    settings_part = package.related('settings')
    assert settings_part
    settings = package.node(settings_part)
    policy = {}
    containers = settings.findall('w:' + story + 'Pr', opc.NS)
    sections = body.findall('.//w:sectPr', opc.NS)
    assert len(sections) == 1, 'No section/restart scope is certified'
    containers += sections[0].findall('w:' + story + 'Pr', opc.NS)
    for node in containers:
        for name in ('numFmt', 'numStart', 'numRestart'):
            children = node.findall('w:' + name, opc.NS)
            assert len(children) <= 1, 'Duplicate numbering policy'
            if children:
                policy[name] = opc.val(children[0])
    assert policy.get('numFmt', 'decimal' if story == 'footnote' else 'lowerRoman') == expected['format']
    assert policy.get('numStart', '1') == '7', 'Native start7 was lost'
    assert policy.get('numRestart', 'continuous') == 'continuous'
    styles = opc.Styles(package)
    axes = {}
    for role, identity in roles.items():
        node = next(n for n in notes.findall('w:' + story, opc.NS) if opc.val(n, 'id') == identity)
        axes[role] = []
        for paragraph in node.findall('.//w:p', opc.NS):
            for run in paragraph.findall('.//w:r', opc.NS):
                current = ''.join(t.text or '' for t in run.findall('w:t', opc.NS))
                if 'NOTE_' in current:
                    effective = styles.run(paragraph, run)
                    assert effective.get('sz') == SIZES[role] * 2, 'Effective native author size changed'
                    axes[role].append({'text': current, 'effectiveHalfPoints': effective['sz'],
                                       'declared': opc.properties(run.find('w:rPr', opc.NS))})
        assert axes[role], 'Visible native author run missing'
    return {'passed': True, 'kind': 'native-numbering', 'sha256': digest(package.raw),
            'story': story, 'policy': policy, 'references': expected['references'],
            'nativeIds': roles, 'authorDeclarations': axes,
            'sourcePayloadUsedAsNativeEvidence': False, 'applicationCertified': False}


def finite(values):
    return all(isinstance(value, (int, float)) and math.isfinite(value) for value in values)


def union(rows):
    return [min(row[0] for row in rows), min(row[1] for row in rows),
            max(row[2] for row in rows), max(row[3] for row in rows)]


def contains(outer, inner, eps=EPS):
    return outer[0] - eps <= inner[0] <= inner[2] <= outer[2] + eps and outer[1] - eps <= inner[1] <= inner[3] <= outer[3] + eps


def normalized_font(value):
    return re.sub(r'^[A-Z]{6}\+', '', value).replace(' ', '').lower()


def program_font_aliases(base_font, extracted_name, font):
    aliases = {base_font, extracted_name}
    if 'name' in font:
        aliases.update(item.toUnicode() for item in font['name'].names if item.nameID in (4, 6))
    else:
        assert normalized_font(base_font) == normalized_font(extracted_name), 'Nameless subset resource/extracted font binding mismatch'
    return aliases


def read_font_program(program):
    """Bind real GIDs to outlines in an embedded sfnt or single-font CFF1."""
    from fontTools.ttLib import TTFont
    if program[:4] in (b'\x00\x01\x00\x00', b'OTTO', b'true'):
        font = TTFont(io.BytesIO(program), lazy=False)
        upem = font['head'].unitsPerEm
        assert type(upem) is int and 16 <= upem <= 16384, 'Unproved sfnt unitsPerEm'
        return {'font': font, 'order': font.getGlyphOrder(), 'glyphs': font.getGlyphSet(),
                'upem': upem, 'bounds': {}, 'format': 'sfnt', 'hasNameTable': 'name' in font}
    from fontTools.cffLib import CFFFontSet
    assert len(program) >= 4 and program[0] == 1 and program[1] == 0 and 4 <= program[2] <= len(program) and 1 <= program[3] <= 4, 'Unproved embedded font format'
    cff = CFFFontSet()
    cff.decompile(io.BytesIO(program), None)
    assert len(cff.fontNames) == len(cff.topDictIndex) == 1, 'CFF must contain exactly one font'
    name = cff.fontNames[0]
    assert isinstance(name, str) and name and name.isascii() and not any(c.isspace() for c in name), 'Invalid CFF Name INDEX identity'
    top = cff.topDictIndex[0]
    matrix = list(top.FontMatrix)
    assert len(matrix) == 6 and finite(matrix) and matrix[0] == matrix[3] and matrix[0] > 0 and matrix[1:3] == [0, 0] and matrix[4:] == [0, 0], 'Unproved CFF FontMatrix'
    upem = 1 / matrix[0]
    assert math.isfinite(upem) and upem.is_integer() and 16 <= upem <= 16384, 'Unproved CFF unitsPerEm'
    order, glyphs = list(top.charset), top.CharStrings
    assert 1 < len(order) <= 65536 and order[0] == '.notdef' and len(set(order)) == len(order), 'Invalid CFF charset/GID mapping'
    assert set(glyphs.keys()) == set(order) and glyphs.charStringsAreIndexed and len(glyphs.charStringsIndex) == len(order) and [glyphs.charStrings[value] for value in order] == list(range(len(order))), 'Invalid CFF CharStrings/GID mapping'
    if hasattr(top, 'FDArray'):
        assert hasattr(top, 'FDSelect') and 0 < len(top.FDArray) <= 256 and len(top.FDSelect) == len(order), 'Invalid CFF FDSelect mapping'
        for fd in top.FDArray:
            # A nonidentity local matrix needs separate per-glyph transform
            # proof; the admitted CID subset uses the top-level matrix only.
            assert list(getattr(fd, 'FontMatrix', [1, 0, 0, 1, 0, 0])) == [1, 0, 0, 1, 0, 0], 'Unproved CFF FD FontMatrix'
        assert all(type(selector) is int and 0 <= selector < len(top.FDArray) for selector in top.FDSelect.gidArray), 'Invalid CFF FDSelect index'
        assert all(glyphs.getItemAndSelector(value)[1] == top.FDSelect[gid] for gid, value in enumerate(order)), 'CFF CharStrings/FDSelect mismatch'
    else:
        assert not hasattr(top, 'FDSelect'), 'CFF FDSelect lacks FDArray'
    return {'font': cff, 'order': order, 'glyphs': glyphs, 'upem': upem,
            'bounds': {}, 'format': 'cff1', 'hasNameTable': False,
            'cffName': name, 'fontMatrix': matrix}


def inventory(document):
    from fontTools.pens.boundsPen import BoundsPen
    pages, fonts, font_by_program = [], {}, {}
    for page_number, page in enumerate(document):
        assert page.rotation == 0 and list(page.cropbox) == list(page.mediabox), 'Unproved page transform'
        assert page.rect.width > 0 and page.rect.height > 0
        assert not page.get_images(full=True), 'Raster text substitute is not admitted'
        resources = {}
        for row in page.get_fonts(full=True):
            xref = row[0]
            name, _, kind, program = document.extract_font(xref)
            aliases = {row[3], name}
            if program and kind != 'Type3':
                program_sha = digest(program)
                if program_sha not in font_by_program:
                    font_by_program[program_sha] = read_font_program(program)
                parsed = font_by_program[program_sha]
                # PDF BaseFont may use a full name while the trace exposes the
                # PostScript name. Both must originate in this exact program.
                if parsed['format'] == 'cff1':
                    assert normalized_font(row[3]) == normalized_font(name), 'CFF resource/extracted font binding mismatch'
                    aliases.update([parsed['cffName']])
                else:
                    aliases = program_font_aliases(row[3], name, parsed['font'])
            for identity in aliases:
                key = normalized_font(identity)
                if key in resources:
                    assert resources[key][0] == xref, 'Ambiguous actual font resource'
                resources[key] = (xref, kind, program, row[3])
        glyphs = []
        for trace in page.get_texttrace():
            assert trace['type'] in (0, 1, 2) and trace.get('opacity', 1) == 1, 'Hidden/transparent text is not admitted'
            color = trace.get('color')
            assert trace.get('colorspace') in (1, 3) and isinstance(color, (list, tuple)) and finite(color) and any(c < .99 for c in color), 'White/unproved text paint is not visible on the fixed white page'
            assert tuple(trace['dir']) == (1, 0) and trace.get('wmode', 0) == 0
            size = trace['size']
            assert finite([size]) and size > 0
            key = normalized_font(trace['font'])
            assert key in resources, 'Text lacks a uniquely embedded font resource'
            xref, kind, program, base_font = resources[key]
            assert program and kind != 'Type3', 'Visible text requires a real embedded font program'
            program_sha = digest(program)
            parsed = font_by_program[program_sha]
            order, glyph_set, bounds_cache, upem = parsed['order'], parsed['glyphs'], parsed['bounds'], parsed['upem']
            for codepoint, gid, origin, bbox in trace['chars']:
                assert type(codepoint) is int and 0 <= codepoint <= 0x10ffff and codepoint != 0xfffd
                assert type(gid) is int and 0 < gid < len(order), 'Missing or invalid real glyph'
                assert finite([*origin, *bbox]) and contains(list(page.rect), bbox), 'Glyph outside physical page'
                if gid not in bounds_cache:
                    pen = BoundsPen(glyph_set)
                    glyph_set[order[gid]].draw(pen)
                    assert pen.bounds is None or finite(pen.bounds), 'Nonfinite embedded glyph outline'
                    bounds_cache[gid] = pen.bounds
                bounds = bounds_cache[gid]
                ink = None if bounds is None else [origin[0] + bounds[0] * size / upem,
                       origin[1] - bounds[3] * size / upem, origin[0] + bounds[2] * size / upem,
                       origin[1] - bounds[1] * size / upem]
                if not chr(codepoint).isspace():
                    assert ink and ink[0] < ink[2] and ink[1] < ink[3], 'Nonblank character has no visible outline'
                glyphs.append({'char': chr(codepoint), 'gid': gid, 'origin': list(origin), 'box': list(bbox),
                               'ink': ink, 'size': size, 'fontSha256': program_sha, 'page': page_number,
                               'color': trace.get('color'), 'type': trace['type']})
            fonts[str(xref)] = {'xref': xref, 'baseFont': base_font,
                               'sha256': program_sha, 'bytes': len(program), 'kind': kind,
                               'programHasNameTable': parsed['hasNameTable'], 'programFormat': parsed['format']}
            if parsed['format'] == 'cff1':
                fonts[str(xref)].update({'cffName': parsed['cffName'], 'fontMatrix': parsed['fontMatrix'],
                                        'charsetGlyphs': len(order)})
        assert glyphs, 'Blank PDF page'
        for drawing in page.get_drawings():
            assert finite(list(drawing['rect'])) and contains(list(page.rect), list(drawing['rect'])), 'Paint outside page'
        records = [links_base.pdf_link_record(document, link, __import__('pymupdf')) for link in page.get_links()]
        for link in records:
            assert finite(link.get('from', [])) and contains(list(page.rect), link['from'])
            assert link.get('kind') == 1 and type(link.get('page')) is int and 0 <= link['page'] < len(document)
            assert len(link.get('to', [])) == 2 and finite(link['to']), 'GoTo lacks an actual finite destination'
        pages.append({'glyphs': glyphs, 'links': records, 'width': page.rect.width, 'height': page.rect.height})
    return pages, list(fonts.values())


def find_word(pages, word):
    matches = []
    for page in pages:
        text = ''.join(glyph['char'] for glyph in page['glyphs'])
        start = 0
        while True:
            found = text.find(word, start)
            if found < 0:
                break
            matches.append(page['glyphs'][found:found + len(word)])
            start = found + len(word)
    assert len(matches) == 1, ('Unique visible lexical anchor missing/duplicated', word)
    return matches[0]


def same_line(left, right):
    return left['page'] == right['page'] and abs(left['origin'][1] - right['origin'][1]) <= EPS


def marker_before(pages, word, token, engine='rust'):
    anchor = find_word(pages, word)
    first = anchor[0]
    before = [g for g in pages[first['page']]['glyphs'] if g['origin'][0] < first['origin'][0] and
              (same_line(g, first) if engine == 'rust' else min(g['box'][3], first['box'][3]) > max(g['box'][1], first['box'][1]))]
    text = ''.join(g['char'] for g in before)
    assert text.strip() == token, ('Definition visible number missing/wrong', word, token, text)
    return [glyph for glyph in before if not glyph['char'].isspace()]


def actual_link_for(pages, glyphs, used):
    assert glyphs and all(g['ink'] for g in glyphs)
    page = glyphs[0]['page']
    matches = []
    for index, link in enumerate(pages[page]['links']):
        if all(contains(link['from'], glyph['ink']) for glyph in glyphs):
            matches.append(index)
    assert len(matches) == 1, 'Visible marker must have exactly one real complete link fragment'
    key = (page, matches[0])
    assert key not in used, 'One link annotation reused for distinct source markers'
    used.add(key)
    link = pages[page]['links'][matches[0]]
    for glyph in pages[page]['glyphs']:
        if glyph in glyphs or not glyph['ink']:
            continue
        ink = glyph['ink']
        center = [(ink[0] + ink[2]) / 2, (ink[1] + ink[3]) / 2]
        assert not (link['from'][0] < center[0] < link['from'][2] and link['from'][1] < center[1] < link['from'][3]), 'Link covers unrelated visible glyph'
    return link


def target_is(link, glyphs):
    assert link['page'] == glyphs[0]['page'], 'GoTo points to wrong page'
    x, y = link['to']
    origin = glyphs[0]['origin']
    assert abs(x - origin[0]) <= EPS and abs(y - origin[1]) <= EPS, 'GoTo must target actual inline marker origin/baseline'


def check_pages(pages, case, engine='rust'):
    expected, refs, definitions = contract(case), [], {}
    full_text = ''.join(g['char'] for page in pages for g in page['glyphs'])
    assert full_text.count(expected['literal']) == 1, 'Ordinary same-name text changed'
    assert full_text.count('[^missing]') == 1, 'Ordinary unparsed undefined literal changed'
    for word in ['NOTE_DOCUMENT_BEGIN', 'NOTE_DOCUMENT_END'] + ['FLOW_' + str(n).zfill(3) + '_KEEP' for n in range(1, 73)]:
        assert full_text.count(word) == 1, ('Visible body text omitted/duplicated', word)
    stripped = full_text.replace(expected['literal'], '')
    assert not re.search(r'word-(?:footnote|endnote)-[0-9]+|(?:zeta|alpha|beta)-k', stripped), 'Technical source identity leaked into visible numbering'
    for left, right, role in REFS:
        a, b = find_word(pages, left), find_word(pages, right)
        assert a[-1]['page'] == b[0]['page'] and same_line(a[-1], b[0])
        row = pages[a[-1]['page']]['glyphs']
        start, end = row.index(a[-1]) + 1, row.index(b[0])
        between = row[start:end]
        while between and between[0]['char'].isspace():
            between = between[1:]
        while between and between[-1]['char'].isspace():
            between = between[:-1]
        assert ''.join(g['char'] for g in between) == expected['numbers'][role], 'Wrong visible reference number'
        if engine == 'rust':
            assert all(abs(g['size'] - 7.7) <= .02 for g in between), 'Body callout must be 0.7 of11pt'
            assert all(abs(g['origin'][1] - (a[-1]['origin'][1] - 3.85)) <= .02 for g in between), 'Body callout lacks actual0.35em superscript rise'
        else:
            assert all(0 < g['size'] < 11 and g['origin'][1] < a[-1]['origin'][1] for g in between), 'Typst semantic marker must remain real visible superscript text'
        refs.append((role, between))
    assert refs[0][1][0]['page'] != refs[2][1][0]['page'], 'Fixed source must exercise a repeated reference across actual pages'
    for role, size in SIZES.items():
        body = find_word(pages, 'NOTE_' + role + '_BODY')
        assert all(abs(g['size'] - size) <= .02 for g in body), 'Author note size lost'
        definitions[role] = marker_before(pages, 'NOTE_' + role + '_BODY', expected['numbers'][role], engine)
        if engine == 'rust':
            assert all(abs(g['size'] - 10) <= .02 for g in definitions[role]), 'Definition display marker base size changed'
    used, proof = set(), []
    marker_glyphs = [glyph for _, values in refs for glyph in values] + [glyph for values in definitions.values() for glyph in values]
    if engine == 'rust':
        for role, glyphs in refs:
            link = actual_link_for(pages, glyphs, used)
            target_is(link, definitions[role])
            proof.append({'role': role, 'direction': 'reference-to-definition', 'fromPage': glyphs[0]['page'] + 1, 'toPage': link['page'] + 1, 'from': link['from'], 'to': link['to']})
        for role, glyphs in definitions.items():
            link = actual_link_for(pages, glyphs, used)
            target = next(values for ref_role, values in refs if ref_role == role)
            target_is(link, target)
            proof.append({'role': role, 'direction': 'definition-to-first-reference', 'toPage': link['page'] + 1})
        end = find_word(pages, 'NOTE_Z_END')[-1]
        after = [g for g in pages[end['page']]['glyphs'] if abs(g['origin'][1] - end['origin'][1]) <= 4 and g['origin'][0] > end['origin'][0]]
        text = ''.join(g['char'] for g in after).strip()
        assert text == '[2]', 'Repeated definition must expose exactly the later-reference backlink'
        back = [g for g in after if not g['char'].isspace()]
        marker_glyphs.extend(back)
        assert all(abs(g['size'] - 7) <= .02 and abs(g['origin'][1] - (end['origin'][1] - 3.5)) <= .02 for g in back), 'Repeated backlink superscript changed'
        link = actual_link_for(pages, back, used)
        target_is(link, refs[2][1])
        proof.append({'role': 'Z', 'direction': 'definition-to-second-reference', 'toPage': link['page'] + 1})
        assert sum(len(page['links']) for page in pages) == len(used) == 8, 'Missing/extra/duplicate note links'
    return {'visibleReferences': [expected['numbers'][role] for role, _ in refs],
            'visibleDefinitions': expected['numbers'], 'authorSizesPt': SIZES,
            'links': proof, 'everyReferenceBacklinkCertified': engine == 'rust',
            'literalPreserved': True, 'crossPageRepeat': True, '_markerGlyphs': marker_glyphs}


def pdf(file, case, engine='rust'):
    import pymupdf
    file = bounded_file(file)
    with pymupdf.open(file) as document:
        assert 1 < len(document) <= MAX_PAGES and not document.is_encrypted
        pages, fonts = inventory(document)
        proof = check_pages(pages, case, engine)
        marker_glyphs = proof.pop('_markerGlyphs')
        images = []
        for page_number, page in enumerate(document):
            image = page.get_pixmap(matrix=pymupdf.Matrix(2, 2), alpha=False)
            assert image.n == 3 and sum(byte < 240 for byte in image.samples) > 100, 'Blank rendered page'
            pixels = image.samples
            for glyph in (g for g in marker_glyphs if g['page'] == page_number):
                x0, y0, x1, y1 = glyph['ink']
                dark = 0
                for y in range(max(0, math.floor(y0 * 2)), min(image.height, math.ceil(y1 * 2))):
                    for x in range(max(0, math.floor(x0 * 2)), min(image.width, math.ceil(x1 * 2))):
                        if x0 < (x + .5) / 2 < x1 and y0 < (y + .5) / 2 < y1:
                            at = (y * image.width + x) * 3
                            dark += any(channel < 240 for channel in pixels[at:at + 3])
                assert dark > 0, 'Marker glyph is occluded or absent from actual page pixels'
            images.append({'width': image.width, 'height': image.height, 'rgbSha256': digest(image.samples)})
    return {'passed': True, 'kind': 'visible-note-numbering-pdf', 'sha256': digest(file.read_bytes()),
            'case': case, 'engine': engine, 'pages': len(pages), 'glyphs': sum(len(p['glyphs']) for p in pages),
            'fontPrograms': fonts, 'renderedPages': images, 'markerPixelsChecked': len(marker_glyphs), **proof,
            'applicationCertified': False, 'pageBottomLayoutCertified': False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['source', 'native', 'pdf'])
    parser.add_argument('--input', required=True)
    parser.add_argument('--case', required=True, choices=sorted(CASES))
    parser.add_argument('--engine', choices=['rust', 'typst'], default='rust')
    args = parser.parse_args(argv)
    assert args.operation == 'pdf' or args.engine == 'rust', 'Engine applies only to PDF inspection'
    value = pdf(args.input, args.case, args.engine) if args.operation == 'pdf' else globals()[args.operation](args.input, args.case)
    print(json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True))


if __name__ == '__main__':
    main()
