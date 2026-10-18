"""Independent native TeX output QA; never shipped or called by ReadMD."""
import json
import sys
from pathlib import Path
import pymupdf as fitz
from PIL import Image, ImageDraw

root = Path(sys.argv[1])
report = json.loads((root / 'report.json').read_text(encoding='utf8'))
assert report['passed'] and report['synthetic']
results, thumbnails = {}, []
for name in ['article', 'ctexart', 'ctexrep', 'ctexbook', 'latin-pdflatex']:
    doc = fitz.open(root / (name + '.pdf'))
    text = '\n'.join(page.get_text() for page in doc)
    expected = ['ReadMD portable output'] if name == 'latin-pdflatex' else ['证据与推理', '研究问题', '观察方法', '比较证据', '结论与局限', '0.954']
    for term in expected:
        assert term in text, (name, term)
    fonts = {row[0]: row for page in doc for row in page.get_fonts(full=True)}
    embedded = []
    for xref, info in fonts.items():
        fontname, ext, kind, data = doc.extract_font(xref)
        assert data, (name, fontname, 'Font not embedded')
        assert not any(host in fontname.lower() for host in ['simsun', 'microsoft', 'timesnewroman'])
        embedded.append({'name': fontname, 'format': ext, 'bytes': len(data)})
    if name != 'latin-pdflatex':
        assert any('FandolSong-Regular' in font['name'] for font in embedded)
        assert any('FandolSong-Bold' in font['name'] for font in embedded)
    vectors = sum(len(page.get_drawings()) for page in doc)
    if name != 'latin-pdflatex':
        assert vectors > 0
    original = fitz.open(root / name / 'document.pdf')
    assert len(original) == len(doc)
    for before, after in zip(original, doc):
        assert before.get_pixmap(alpha=False).samples == after.get_pixmap(alpha=False).samples, (name, 'Unicode map changed rendered pixels')
    for index, page in enumerate(doc):
        pixmap = page.get_pixmap(matrix=fitz.Matrix(0.65, 0.65), alpha=False)
        thumbnail = Image.frombytes('RGB', (pixmap.width, pixmap.height), pixmap.samples)
        thumbnails.append((name + ' / ' + str(index + 1), thumbnail))
    doc[0].get_pixmap(matrix=fitz.Matrix(1.5, 1.5), alpha=False).save(root / (name + '-first.png'))
    (root / (name + '-independent-text.txt')).write_text(text, encoding='utf8')
    results[name] = {'pages': len(doc), 'exactUnicodeText': True, 'fonts': embedded, 'vectors': vectors, 'renderedPixelsUnchanged': True}
columns = 4
width = max(image.width for _, image in thumbnails) + 20
height = max(image.height for _, image in thumbnails) + 40
sheet = Image.new('RGB', (width * columns, height * ((len(thumbnails) + columns - 1) // columns)), '#e8eaf0')
draw = ImageDraw.Draw(sheet)
for i, (label, image) in enumerate(thumbnails):
    x, y = (i % columns) * width, (i // columns) * height
    draw.text((x + 10, y + 10), label, fill='#202330')
    sheet.paste(image, (x + 10, y + 30))
sheet.save(root / 'contact-sheet.png')
result = {'passed': True, 'independentEngine': 'PyMuPDF', 'synthetic': True, 'scope': 'Actual produced PDF pages, exact Unicode text, embedded font bytes and vector objects. Contact sheet is QA, not 4K promotional material.', 'documents': results}
(root / 'independent-inspection.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(json.dumps({'passed': True, 'documents': len(results), 'pages': sum(r['pages'] for r in results.values()), 'exactUnicodeText': True, 'embeddedFontBytes': True}))
