'use strict';
// Actual original CSL -> shared service -> reader/packages/three PDF engines.
// A completed engine run owns the installed runtime; this suite owns fresh QA
// files only and never copies, reinstalls or relaxes that runtime's checks.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net');
const assert = require('node:assert/strict');
const {createHarness, fileHash, sha256} = require('./publication-batch3-harness.cjs');
const {inspectArtifact} = require('./publication-batch3-checks.cjs');
const ROOT = path.resolve(__dirname, '../..');
const TOLERANCE = Object.freeze({coordinatePt: 0.75, intervalPt: 1, baselineClusterPt: 0.2, cssPixelsPerPt: 96 / 72});
const TITLES = [
  'BODYA alpha beta gamma delta theta data words notes proof tests safe local plain text rows align under one shared known label column while each body wraps cleanly and keeps its last mark intact here END_A',
  'BODYB sigma beta gamma delta theta data words notes proof tests safe local plain text rows align under one shared known label column while each body wraps cleanly and keeps its last mark intact here END_B',
];
const MARKERS = [['BODYA', 'END_A'], ['BODYB', 'END_B']];
const LABELS = ['[9]', '[100]'];
const REFERENCES = TITLES.map((title, i) => ({id: ['a', 'b'][i], type: 'book', title,
  volume: ['9', '100'][i], author: [{literal: 'Synthetic QA Author ' + ['A', 'B'][i]}], issued: {'date-parts': [[2026]]}}));
const CASES = [1, 3].flatMap(line => [0, 2].map(entry => ({id: `L${line}-E${entry}`, line, entry})));
const OPTIONS = {typography: {font: 'Arial', size: 11, lineHeight: 1, spacing: 0, align: 'left', firstLineIndent: 0},
  cover: {enabled: false}, toc: {enabled: false}, footer: {pageNumbers: false}, frontMatter: {mode: 'none'}};
const plain = nodes => nodes.map(n => n.text ?? (n.children ? plain(n.children) : '')).join('');
const compact = text => text.replace(/\s+/gu, ' ').trim();
const median = values => {const v = [...values].sort((a, b) => a - b); return v.length ? v[Math.floor(v.length / 2)] : NaN;};
const write = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
function style(line, entry, kind = 'pair') {
  const body = kind === 'pair' || kind === 'complex'
    ? '<text variable="volume" display="left-margin" prefix="[" suffix="]"/><text variable="title" display="right-inline"/>' +
      (kind === 'complex' ? '<text value="Unrepresentable full-width tail" display="block"/>' : '')
    : `<text variable="title" display="${kind === 'left' ? 'left-margin' : 'right-inline'}"/>`;
  return '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Original layout QA</title>' +
    '<id>urn:readmd:qa:original-layout</id><updated>2026-10-14T00:00:00Z</updated></info>' +
    '<citation><layout prefix="[" suffix="]" delimiter=", "><text variable="citation-number"/></layout></citation>' +
    `<bibliography line-spacing="${line}" entry-spacing="${entry}"><layout>${body}</layout></bibliography></style>`;
}
const source = csl => `---\ntitle: Original publication layout QA\nbibliography: layout-references.json\ncsl: ${csl}\nlanguage: en-US\n---\n\nCited [@a; @b].\n\n<!--readmd:bibliography-->\n`;
function uniqueIndex(text, marker) {
  const at = text.indexOf(marker); assert(at >= 0, 'Missing real marker: ' + marker);
  assert.equal(text.indexOf(marker, at + marker.length), -1, 'Duplicated real marker: ' + marker); return at;
}
function checkText(result) {
  for (const parser of ['text', 'pypdfText']) for (const [i, [first, last]] of MARKERS.entries()) {
    const text = result[parser]; assert.equal(typeof text, 'string', 'Both independent PDF parsers are required');
    const from = uniqueIndex(text, first), to = uniqueIndex(text, last) + last.length;
    assert.equal(compact(text.slice(from, to)), TITLES[i], parser + ' dropped, reordered or joined original title words');
  }
}
function measurePdf(result) {
  assert.equal(result.pages.length, 1, 'Small layout fixture must stay on one actual PDF page'); checkText(result);
  const page = result.pages[0], characters = (page.glyphTraces || []).flatMap((trace, ti) => trace.chars.map((c, ci) =>
    ({...c, c: String.fromCodePoint(c.unicode), font: trace.font, size: trace.size, trace: ti, order: ci})))
    .filter(c => c.unicode >= 32 && c.unicode !== 0x7f).sort((a, b) => a.origin[1] - b.origin[1] || a.origin[0] - b.origin[0]);
  assert(characters.length, 'Actual glyph traces are required');
  const lines = [];
  for (const c of characters) {
    let line = lines.find(l => Math.abs(l.baseline - c.origin[1]) <= TOLERANCE.baselineClusterPt);
    if (!line) {line = {baseline: c.origin[1], chars: []}; lines.push(line);} line.chars.push(c);
  }
  lines.sort((a, b) => a.baseline - b.baseline);
  let offset = 0;
  const ordered = lines.flatMap((l, index) => l.chars.sort((a, b) => a.origin[0] - b.origin[0] || a.trace - b.trace || a.order - b.order)
    .map(c => {const value = {...c, line: index, offset}; offset += c.c.length; return value;}));
  const text = ordered.map(c => c.c).join('');
  const entries = MARKERS.map(([first, last], index) => {
    const from = uniqueIndex(text, first), end = uniqueIndex(text, last) + last.length;
    const glyphs = ordered.filter(c => c.offset >= from && c.offset < end);
    assert.equal(glyphs.map(c => c.c).join('').replace(/\s/gu, ''), TITLES[index].replace(/\s/gu, ''), 'Actual page glyphs lost or reordered title letters');
    const bodyLines = [...new Set(glyphs.map(c => c.line))].map(li => {
      const letters = glyphs.filter(c => c.line === li && /\S/u.test(c.c));
      return {baseline: median(letters.map(c => c.origin[1])), firstX: Math.min(...letters.map(c => c.origin[0])),
        inkLeft: Math.min(...letters.map(c => c.bbox[0])), inkRight: Math.max(...letters.map(c => c.bbox[2])),
        text: letters.map(c => c.c).join(''), glyphs: letters};
    });
    const labelAt = uniqueIndex(text, LABELS[index]);
    const label = ordered.filter(c => c.offset >= labelAt && c.offset < labelAt + LABELS[index].length);
    return {marker: first, endMarker: last, from, end, lines: bodyLines, label: {text: LABELS[index],
      baseline: median(label.map(c => c.origin[1])), inkRight: Math.max(...label.map(c => c.bbox[2])), glyphs: label}};
  });
  assert(entries[0].end < entries[1].from, 'Page reading order must finish entry A before entry B');
  return {pageIndex: 0, pageWidth: page.width, pageHeight: page.height, entries,
    intervals: entries.flatMap(e => e.lines.slice(1).map((l, i) => l.baseline - e.lines[i].baseline)),
    actualEntryInterval: entries[1].lines[0].baseline - entries[0].lines.at(-1).baseline};
}
function comparePdf(measured, item, baseline) {
  const unit = baseline?.unit ?? median(measured.intervals), comparisons = [];
  const near = (name, actual, expected, tolerance) => comparisons.push({name, actual, expected, tolerance,
    difference: actual - expected, passed: Number.isFinite(actual) && Number.isFinite(expected) && Math.abs(actual - expected) <= tolerance});
  const predicate = (name, actual) => comparisons.push({name, actual, expected: true, passed: actual === true});
  predicate('actual positive single-line unit', Number.isFinite(unit) && unit > 0);
  const commonX = measured.entries[0].lines[0].firstX;
  for (const [index, entry] of measured.entries.entries()) {
    predicate(entry.marker + ' really wraps', entry.lines.length >= 2);
    if (baseline) predicate(entry.marker + ' line count unchanged', entry.lines.length === baseline.lineCounts[index]);
    for (const [li, line] of entry.lines.entries()) {
      near(`${entry.marker} line ${li} shared body origin`, line.firstX, commonX, TOLERANCE.coordinatePt);
      predicate(`${entry.marker} line ${li} inside page`, line.inkLeft >= -TOLERANCE.coordinatePt && line.inkRight <= measured.pageWidth + TOLERANCE.coordinatePt);
      predicate(`${entry.marker} line ${li} no actual .notdef`, line.glyphs.every(c => c.glyphId !== 0));
    }
    predicate(entry.marker + ' label does not overlap body ink', entry.label.inkRight <= Math.min(...entry.lines.map(l => l.inkLeft)) + TOLERANCE.coordinatePt);
  }
  for (const [i, interval] of measured.intervals.entries()) near('within-entry baseline interval ' + i, interval, item.line * unit, TOLERANCE.intervalPt);
  near('next first baseline minus previous last baseline', measured.actualEntryInterval, item.line * (1 + item.entry) * unit, TOLERANCE.intervalPt);
  return {passed: comparisons.every(c => c.passed), unit, lineCounts: measured.entries.map(e => e.lines.length), comparisons,
    failures: comparisons.filter(c => !c.passed)};
}
const PACKAGE_STRUCTURE = String.raw`
import json,sys,zipfile,xml.etree.ElementTree as E
with zipfile.ZipFile(sys.argv[1]) as z:
 assert sum(x.file_size for x in z.infolist())<=64*1024*1024
 root=E.fromstring(z.read('word/document.xml'))
w='{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
def attrs(x):return {} if x is None else {k.split('}')[-1]:v for k,v in x.attrib.items()}
rows=[]
for p in root.iter(w+'p'):
 text=''.join(x.text or '' for x in p.iter(w+'t'))
 if 'BODYA' not in text and 'BODYB' not in text:continue
 pp=p.find(w+'pPr')
 rows.append({'text':text,'tabs':[attrs(x) for x in pp.findall(w+'tabs/'+w+'tab')],
 'indent':attrs(pp.find(w+'ind')),'spacing':attrs(pp.find(w+'spacing')),
 'actualRunTabs':len(p.findall('.//'+w+'r/'+w+'tab'))})
print(json.dumps({'rows':rows}))
`;
function checkDocxPackage(evidence, item) {
  assert.equal(evidence.rows.length, 2, 'Both actual bibliography paragraphs are required'); let stop;
  for (const [i, row] of evidence.rows.entries()) {
    assert.equal(row.text, LABELS[i] + TITLES[i]); assert.equal(row.actualRunTabs, 1);
    assert.equal(row.tabs.length, 1); assert.equal(row.tabs[0].val, 'left');
    const current = Number(row.tabs[0].pos); assert(current > 0);
    if (stop === undefined) stop = current; else assert.equal(current, stop, 'DOCX paragraphs do not share the column stop');
    assert.equal(Number(row.indent.left), current); assert.equal(Number(row.indent.hanging), current);
    assert.equal(row.spacing.lineRule, 'auto'); assert.equal(Number(row.spacing.line), 240 * item.line);
    assert.equal(Number(row.spacing.after), 0, 'Inherited point gap was not canceled');
    assert.equal(Number(row.spacing.afterLines), i ? 0 : 100 * item.line * item.entry, 'OOXML additional lines or final gap are wrong');
  }
}
async function measureReader(page, sectionSelector = '#content .readmd-bibliography') {
  return page.locator(sectionSelector).evaluate(section => {
    const rect = element => {const r = element.getBoundingClientRect(); return {x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom};};
    const rows = [...section.querySelectorAll('.readmd-bibliography-entry')].map(row => {
      const body = row.querySelector('.readmd-bibliography-content'), label = row.querySelector('.readmd-bibliography-first-field');
      const glyphs = [], walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT); let node;
      while ((node = walker.nextNode())) {let offset = 0; for (const c of node.textContent) {
        if (/\S/u.test(c)) {const range = document.createRange(); range.setStart(node, offset); range.setEnd(node, offset + c.length);
          for (const r of range.getClientRects()) glyphs.push({c, x: r.x, y: r.y, width: r.width, height: r.height, right: r.right});}
        offset += c.length;
      }}
      const lines = [];
      for (const g of glyphs) {let line = lines.find(l => Math.abs(l.y - g.y) <= 0.5);
        if (!line) {line = {y: g.y, glyphs: []}; lines.push(line);} line.glyphs.push(g);}
      const style = getComputedStyle(row), bodyStyle = getComputedStyle(body);
      return {text: body.textContent, labelText: label?.textContent, row: rect(row), body: rect(body), label: label ? rect(label) : null,
        fontSize: parseFloat(style.fontSize), lineHeight: parseFloat(bodyStyle.lineHeight), marginBottom: parseFloat(style.marginBottom),
        lines: lines.sort((a, b) => a.y - b.y).map(l => ({y: l.y, x: Math.min(...l.glyphs.map(g => g.x)), right: Math.max(...l.glyphs.map(g => g.right)), glyphs: l.glyphs}))};
    });
    return {rows, section: rect(section), sharedContainer: section.querySelectorAll('.readmd-bibliography-shared-columns').length,
      horizontalOverflow: document.querySelector('#content').scrollWidth > document.querySelector('#content').clientWidth + 1,
      source: state.original};
  });
}
function checkReader(evidence, item, content) {
  assert.equal(evidence.source, content); assert.equal(evidence.sharedContainer, 1); assert.equal(evidence.rows.length, 2);
  assert.equal(evidence.horizontalOverflow, false);
  const epsilon = TOLERANCE.coordinatePt * TOLERANCE.cssPixelsPerPt, commonX = evidence.rows[0].lines[0].x;
  for (const [i, row] of evidence.rows.entries()) {
    assert.equal(row.text, TITLES[i]); assert.equal(row.labelText, LABELS[i]); assert(row.lines.length >= 2, 'Reader fixture must really wrap');
    assert(Math.abs(row.marginBottom - (i ? 0 : item.line * item.entry * row.fontSize)) <= epsilon, 'Reader margin is not entry * line * fontSize, or last gap is nonzero');
    assert(Math.abs(row.lineHeight - item.line * row.fontSize) <= epsilon, 'Reader line-height differs from the requested CSL multiplier');
    assert(row.label && row.label.right <= commonX + epsilon, 'Reader label overlaps the body column');
    for (const line of row.lines) {
      assert(Math.abs(line.x - commonX) <= epsilon, 'Reader wrapped line returned to the label column or body columns differ');
      assert(line.right <= row.row.right + epsilon, 'Reader body glyphs overflow the actual entry');
    }
  }
}
async function inspectEpubStructure(page, inspected) {
  const chapters = inspected.content.filter(c => c.xml.includes('BODYA') && c.xml.includes('BODYB'));
  assert.equal(chapters.length, 1, 'Both original entries must share a real content document');
  return page.evaluate(({xml, css}) => {
    const doc = new DOMParser().parseFromString(xml, 'application/xhtml+xml');
    if (doc.querySelector('parsererror')) throw Error('Actual EPUB XHTML is invalid');
    const tables = [...doc.querySelectorAll('section[role="doc-bibliography"] table.bibliography-columns')];
    return {tableCount: tables.length, tableLineHeight: tables[0]?.style.lineHeight, role: tables[0]?.getAttribute('role'),
      rows: [...(tables[0]?.querySelectorAll('tr') || [])].map(row => [...row.children].map(cell => ({text: cell.textContent,
        columnSpan: cell.getAttribute('colspan'), paddingBottom: cell.style.paddingBottom, className: cell.className}))),
      stylesheet: {nowrapFirstField: /\.bibliography-columns\s+\.bibliography-first-field\s*\{[^}]*white-space:\s*nowrap/.test(css),
        sharedFullWidthTable: /\.bibliography-columns\s*\{[^}]*width:\s*100%/.test(css)}};
  }, {xml: chapters[0].xml, css: inspected.css});
}
function checkEpub(evidence, item) {
  assert.equal(evidence.tableCount, 1); assert.equal(evidence.role, 'presentation'); assert.equal(evidence.tableLineHeight, String(item.line));
  assert.deepEqual(evidence.stylesheet, {nowrapFirstField: true, sharedFullWidthTable: true}); assert.equal(evidence.rows.length, 2);
  for (const [i, cells] of evidence.rows.entries()) {
    assert.equal(cells.length, 2); assert.equal(cells[0].text, LABELS[i]); assert.equal(cells[1].text, TITLES[i]);
    assert.equal(cells[0].className, 'bibliography-first-field'); assert.equal(cells[1].className, 'bibliography-content');
    for (const cell of cells) {assert(!cell.columnSpan || cell.columnSpan === '1'); assert.equal(cell.paddingBottom, `${i ? 0 : item.line * item.entry}em`);}
  }
}
async function ownedPort() {const server = net.createServer(); await new Promise((resolve, reject) => {server.once('error', reject); server.listen(0, '127.0.0.1', resolve);});
  const port = server.address().port; await new Promise(resolve => server.close(resolve)); return port;}
async function run() {
  const helpers = require('../../ui-tests/publication-engines-native.cjs');
  const reused = helpers.resolveEngineReuseProfile(process.env.READMD_ENGINE_REUSE_PROFILE); assert(reused, 'Explicit fixed owned n2e7Ug reuse is required');
  const h = createHarness('publication-layout-native', await ownedPort(), __filename, reused);
  process.env.READMD_NATIVE_LOG_FILE = path.join(h.data, 'native.log');
  fs.copyFileSync(__filename, path.join(h.data, 'qa-script.cjs'));
  for (const file of ['ui-tests/publication-engines-native.cjs', 'tools/test/publication-runtime-reuse.cjs']) h.report.qaDependencySha256[file] = fileHash(path.join(ROOT, file));
  h.report.scope = 'Four original CSL display/spacing cases: actual Windows reader, DOCX/EPUB package layout, Rust/Typst/managed XeLaTeX PDF glyph baselines. Explicit owned engine native-data reuse; fresh QA documents, outputs and report.';
  h.report.tolerance = TOLERANCE; h.report.binarySha256 = h.report.candidateSha256; h.report.outputs = []; h.report.cases = []; h.report.singleLineUnits = {};
  h.report.limitations.push('No Word application or independent EPUB reader geometry is certified. Package structure and actual PDF/browser measurements are distinct evidence.',
    'Existing QA-only TeX is not a redistributable runtime; no cold-performance acceptance, all-font or all-CSL-standard claim is made.');
  const index = CASES.map(c => ({...c, style: `layout-${c.id}.csl`, document: `Layout-${c.id}.md`, expectedLabels: LABELS, expectedTitles: TITLES}));
  fs.writeFileSync(path.join(h.docs, 'layout-references.json'), JSON.stringify(REFERENCES, null, 2));
  for (const c of index) {fs.writeFileSync(path.join(h.docs, c.style), style(c.line, c.entry)); fs.writeFileSync(path.join(h.docs, c.document), source(c.style));}
  for (const kind of ['left', 'right', 'complex']) {fs.writeFileSync(path.join(h.docs, kind + '.csl'), style(1, 0, kind)); fs.writeFileSync(path.join(h.docs, kind + '.md'), source(kind + '.csl'));}
  write(path.join(h.data, 'case-index.json'), {synthetic: true, originalExpectations: true, cases: index, titlesWordCounts: TITLES.map(t => t.split(' ').length), tolerance: TOLERANCE, plannedOutputs: 20, plannedPdfOutputs: 12});
  const initial = h.snapshotInputs(); h.report.inputs = initial; const baselines = {}; let page, reuseProof, engineReportFile, engineReportSha;
  const progress = () => write(path.join(h.data, 'layout-measurements.json'), {candidateSha256: h.report.candidateSha256, tolerance: TOLERANCE,
    singleLineUnits: h.report.singleLineUnits, cases: h.report.cases, outputs: h.report.outputs, inputHashes: h.snapshotInputs()});
  const apiDocument = content => h.api('/api/academic/document', {content, baseDir: h.docs, view: true});
  async function exportActual(item, format, engine) {
    const name = item.id + '-' + (engine || format) + '.' + format, file = path.join(h.outputs, name);
    const request = {format, content: fs.readFileSync(path.join(h.docs, item.document), 'utf8'), baseDir: h.docs, out_path: file,
      options: {...OPTIONS, ...(engine ? {pdf: {engine}} : {})}};
    const output = {file: name, format, ...(engine ? {engine} : {}), startedAt: new Date().toISOString(), request};
    const current = h.report.cases.find(c => c.id === item.id); current.outputs.push(output); progress();
    const response = await h.api('/api/export', request); output.response = response; write(path.join(h.data, name + '.response.json'), {request, response}); progress();
    assert.equal(response.status, 200, JSON.stringify(response.data)); assert.equal(response.data.ok, true, JSON.stringify(response.data));
    assert(fs.existsSync(file)); output.bytes = fs.statSync(file).size; output.sha256 = fileHash(file);
    h.report.outputs.push({file: name, bytes: output.bytes, sha256: output.sha256, validated: false}); progress();
    const inspected = inspectArtifact(h, file, format === 'pdf' ? {glyphs: true, render: path.join(h.data, name + '-pages')} : {});
    output.inspection = name + '.inspection.json'; output.inspectionSha256 = fileHash(path.join(h.data, output.inspection));
    progress();
    return {output, inspected, file};
  }
  function markValidated(output) {output.validated = true; h.report.outputs.find(o => o.file === output.file).validated = true; progress(); h.unchanged(initial);}
  await h.finish(async () => {try {
    await h.step('completed same-candidate engine report, exact assets overlay, stopped fixed owned runtime and original proof hashes', async () => {
      assert(process.env.READMD_LAYOUT_ENGINE_REPORT, 'A completed successful engine report is required');
      assert.match(process.env.READMD_LAYOUT_ENGINE_REPORT_SHA256 || '', /^[a-f0-9]{64}$/i, 'Root must bind the successful report SHA');
      engineReportFile = path.resolve(ROOT, process.env.READMD_LAYOUT_ENGINE_REPORT);
      assert(path.relative(path.join(ROOT, '.cache/capability-upgrade'), engineReportFile).startsWith('publication-engines-native-'));
      engineReportSha = fileHash(engineReportFile); assert.equal(engineReportSha, process.env.READMD_LAYOUT_ENGINE_REPORT_SHA256.toLowerCase());
      const report = JSON.parse(fs.readFileSync(engineReportFile, 'utf8')), boot = process.env.READMD_CANDIDATE_BOOT_SHA256;
      assert(report.passed && report.checks.length && report.checks.every(c => c.passed)); assert(!report.shutdownError);
      assert(!report.composite && !report.compositeMandatoryGroups, 'A complete successful engine execution is required');
      assert.equal(report.candidateSha256, h.report.candidateSha256); assert.match(boot || '', /^[a-f0-9]{64}$/i);
      assert.equal(report.bootSha256, boot.toLowerCase()); assert.equal(report.servedBootSha256, boot.toLowerCase());
      assert.equal(report.nativeCsp, true); assert.equal(report.noAutomaticRetries, true); assert.equal(report.nativeDataFresh, false);
      assert.equal(path.resolve(ROOT, report.nativeDataDirectory).toLowerCase(), reused.toLowerCase());
      assert(report.engineProfileReuse?.enabled && report.engineProfileReuse.actualProductReadinessReexecuted && report.engineProfileReuse.oldEvidenceBytesPreservedAtCompletion);
      const assets = path.resolve(process.env.READMD_ASSETS_DIR || ''); assert.equal(assets.toLowerCase(), path.resolve(ROOT, report.engineProfileReuse.assetsOverlay).toLowerCase());
      assert.equal(path.dirname(assets).toLowerCase(), path.dirname(engineReportFile).toLowerCase());
      assert(fs.lstatSync(assets).isDirectory() && !fs.lstatSync(assets).isSymbolicLink(), 'The successful owned assets overlay must be an ordinary directory');
      assert.equal(fileHash(path.join(assets, 'readmd.boot.js')), boot.toLowerCase());
      assert.equal(fileHash(path.join(assets, 'capabilities/catalog.json')), report.engineProfileReuse.catalogSha256);
      assert.equal(fileHash(path.join(assets, 'capabilities/latex-2026.10-r1-windows-x64.lock.json')), report.engineProfileReuse.texLockSha256);
      assert.equal(fileHash(path.join(ROOT, 'ui-tests/publication-engines-native.cjs')), report.scriptSha256, 'Successful engine QA source changed before layout reuse');
      reuseProof = helpers.verifyEngineProfileReuse(h, reused);
      Object.assign(h.report.engineProfileReuse, {assetsOverlay: report.engineProfileReuse.assetsOverlay,
        currentAssetsBootSha256: boot.toLowerCase(), catalogSha256: report.engineProfileReuse.catalogSha256, texLockSha256: report.engineProfileReuse.texLockSha256});
      h.report.engineEvidence = {path: path.relative(ROOT, engineReportFile).replaceAll(path.sep, '/'), sha256: engineReportSha,
        actualChecks: report.checks.length, candidateSha256: report.candidateSha256, bootSha256: report.bootSha256, assetsOverlay: report.engineProfileReuse.assetsOverlay};
      h.report.expectedBootSha256 = boot.toLowerCase();
    });
    await h.step('real native host preserves formal CSP, served boot and System32-only engine PATH', async () => {
      const oldPath = process.env.PATH; try {process.env.PATH = path.join(process.env.WINDIR, 'System32'); page = await h.start();} finally {process.env.PATH = oldPath;}
      h.report.system32OnlyChildPath = true; assert.equal(h.report.servedBootSha256, h.report.expectedBootSha256);
      const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
      const scripts = csp.split(';').find(s => s.trim().startsWith('script-src'));
      assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
      await page.setViewportSize({width: 1024, height: 820});
      await page.evaluate(() => i18n.setLanguage('en', false));
      await h.until(() => page.evaluate(() => !document.documentElement.classList.contains('rm-theme-vt')));
      h.report.readerLocale = 'en'; h.report.fixtureLanguage = 'en-US';
    });
    h.report.capabilityStatus = [];
    for (const id of ['typst', 'latex', 'fonts', 'pdfium']) await h.step('real reused-profile capability readiness ' + id, async () => {
      const entry = {id, startedAt: new Date().toISOString()}, start = Date.now(); h.report.capabilityStatus.push(entry);
      console.log(JSON.stringify({readinessStarted: id, at: entry.startedAt}));
      const response = await page.evaluate(async id => {const r = await apiFetch('/api/capabilities/packs', {method: 'POST', headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({action: 'status', id}), signal: AbortSignal.timeout(600000)}); return {status: r.status, data: await r.json()};}, id);
      Object.assign(entry, {elapsedMs: Date.now() - start, response, exceedsOriginal120s: Date.now() - start > 120000});
      write(path.join(h.data, 'capability-status.json'), h.report.capabilityStatus);
      console.log(JSON.stringify({readinessFinished: id, elapsedMs: entry.elapsedMs}));
      assert.equal(response.status, 200); assert.equal(response.data.installed, true); assert.equal(response.data.ready, true);
    });
    h.report.engineProfileReuse.actualProductReadinessReexecuted = true;
    for (const item of index) {
      const current = {...item, outputs: []}; h.report.cases.push(current); progress();
      const file = path.join(h.docs, item.document), content = fs.readFileSync(file, 'utf8');
      await h.step(item.id + ': original CSL resolves typed columns and actual reader geometry without source edits', async () => {
        const result = await apiDocument(content); current.documentResponse = result; write(path.join(h.data, item.id + '.document.json'), result); progress();
        assert.equal(result.status, 200); assert.equal(result.data.ok, true);
        const projection = result.data.projection, bibliography = projection?.bibliography;
        assert.equal(projection.revision, sha256(content)); assert.equal(bibliography.items.length, 2);
        assert.deepEqual(bibliography.items.map(i => plain(i.first_field)), LABELS); assert.deepEqual(bibliography.items.map(i => plain(i.rich)), TITLES);
        assert.deepEqual([bibliography.line_spacing, bibliography.entry_spacing], [item.line, item.entry]);
        assert(!fs.readFileSync(path.join(h.docs, item.style), 'utf8').includes('second-field-align'));
        await page.evaluate(async file => {if (state.editing) await exitEdit(); await loadFile(file);}, file);
        await h.until(() => page.locator('#content .readmd-bibliography-entry').count().then(n => n === 2));
        current.reader = await measureReader(page); progress(); checkReader(current.reader, item, content);
        await page.screenshot({path: path.join(h.data, 'reader-' + item.id + '.png')}); h.unchanged(initial);
      });
      await h.step(item.id + ': actual DOCX and EPUB share columns and encode only inter-entry spacing', async () => {
        const word = await exportActual(item, 'docx'); word.output.package = JSON.parse(h.command('python', ['-c', PACKAGE_STRUCTURE, word.file])); progress();
        checkDocxPackage(word.output.package, item); markValidated(word.output);
        const epub = await exportActual(item, 'epub'); epub.output.package = await inspectEpubStructure(page, epub.inspected); progress();
        checkEpub(epub.output.package, item); markValidated(epub.output);
      });
      for (const engine of ['rust', 'typst', 'xelatex']) await h.step(item.id + ': ' + engine + ' real PDF shared columns and glyph baseline geometry', async () => {
        const actual = await exportActual(item, 'pdf', engine); actual.output.geometry = measurePdf(actual.inspected);
        actual.output.comparison = comparePdf(actual.output.geometry, item, baselines[engine]); progress();
        assert(actual.output.comparison.passed, engine + ': ' + JSON.stringify(actual.output.comparison.failures));
        if (!baselines[engine]) {baselines[engine] = {unit: actual.output.comparison.unit, lineCounts: actual.output.comparison.lineCounts,
          sourceCase: item.id, sourcePdfSha256: actual.output.sha256}; h.report.singleLineUnits[engine] = baselines[engine];}
        markValidated(actual.output);
      });
    }
    await h.step('isolated left-margin and right-inline displays remain actual full-width reader blocks', async () => {
      h.report.orphans = [];
      for (const kind of ['left', 'right']) {
        const content = fs.readFileSync(path.join(h.docs, kind + '.md'), 'utf8'), response = await apiDocument(content);
        const record = {kind, response}; h.report.orphans.push(record); write(path.join(h.data, 'orphan-displays.json'), h.report.orphans);
        assert.equal(response.status, 200); assert.equal(response.data.ok, true);
        const items = response.data.projection.bibliography.items;
        assert(items.every(i => i.first_field.length === 0)); assert.deepEqual(items.map(i => plain(i.rich)), TITLES);
        await page.evaluate(async file => {if (state.editing) await exitEdit(); await loadFile(file);}, path.join(h.docs, kind + '.md'));
        await h.until(() => page.locator('#content .readmd-bibliography-entry').count().then(n => n === 2));
        record.geometry = await page.locator('#content .readmd-bibliography').evaluate(section => ({
          sharedColumns: section.querySelectorAll('.readmd-bibliography-shared-columns').length,
          blocks: [...section.querySelectorAll('.readmd-bibliography-entry')].map(row => {const block = row.querySelector('.readmd-csl-layout-block, .readmd-csl-display-block'), a = row.getBoundingClientRect(), b = block?.getBoundingClientRect();
            return {display: block ? getComputedStyle(block).display : null, rowX: a.x, rowWidth: a.width, blockX: b?.x, blockWidth: b?.width};})}));
        write(path.join(h.data, 'orphan-displays.json'), h.report.orphans);
        assert.equal(record.geometry.sharedColumns, 0); assert.equal(record.geometry.blocks.length, 2);
        for (const b of record.geometry.blocks) {assert.equal(b.display, 'block'); assert(Math.abs(b.rowX - b.blockX) <= 1); assert(Math.abs(b.rowWidth - b.blockWidth) <= 1);}
        assert.equal(await page.evaluate(() => state.original), content); h.unchanged(initial);
      }
    });
    await h.step('mixed display is represented; genuinely invalid section preserves all five completed targets', async () => {
      const mixed = fs.readFileSync(path.join(h.docs, 'complex.md'), 'utf8'), supported = await apiDocument(mixed);
      assert.equal(supported.status, 200); assert.equal(supported.data.ok, true);
      assert(supported.data.projection.bibliography.items.every(item => item.layout.some(node => node.type === 'columns') || JSON.stringify(item.layout).includes('"type":"columns"')));
      const content = mixed + '\n<!-- readmd-section:v1 {"id":"invalid","unknown":true} -->\n', response = await apiDocument(content);
      h.report.complexRejections = [{stage: 'academic.document', response}]; write(path.join(h.data, 'complex-rejections.json'), h.report.complexRejections);
      assert.equal(response.status, 422); assert.equal(response.data.error_code, 'publication_section_invalid');
      for (const [format, engine] of [['docx'], ['epub'], ['pdf', 'rust'], ['pdf', 'typst'], ['pdf', 'xelatex']]) {
        const file = path.join(h.outputs, 'L1-E0-' + (engine || format) + '.' + format), before = fileHash(file);
        const request = {format, content, baseDir: h.docs, out_path: file, options: {...OPTIONS, ...(engine ? {pdf: {engine}} : {})}};
        const result = await h.api('/api/export', request), record = {format, engine, request, response: result, beforeSha256: before, afterSha256: fileHash(file)};
        h.report.complexRejections.push(record); write(path.join(h.data, 'complex-rejections.json'), h.report.complexRejections);
        assert.equal(result.data.ok, false); assert.equal(result.data.error_code, 'publication_section_invalid'); assert.equal(record.afterSha256, before); h.unchanged(initial);
      }
    });
    assert.equal(h.report.outputs.length, 20); assert.equal(h.report.outputs.filter(o => o.file.endsWith('.pdf')).length, 12);
    assert(h.report.outputs.every(o => o.validated)); h.unchanged(initial);
  } finally {
    progress(); if (reuseProof) reuseProof.assertPreserved();
    if (engineReportFile) assert.equal(fileHash(engineReportFile), engineReportSha, 'The successful engine evidence changed');
  }});
}
function selfTest() {
  function fixture(line, entry, changes = {}) {
    const traces = []; const unit = 10, first = 40, aLast = first + line * unit;
    const starts = [first, aLast + line * (1 + entry) * unit + (changes.entryShift || 0)];
    for (const [i, text] of TITLES.entries()) {
      const words = text.split(' '), split = [words.slice(0, 18).join(' '), words.slice(18).join(' ')];
      const emit = (value, x, y) => traces.push({font: 'Synthetic', size: 11, chars: [...value].map((c, j) => ({unicode: c.codePointAt(0), glyphId: c.codePointAt(0) + 1,
        origin: [x + j * 4, y], bbox: [x + j * 4, y - 8, x + j * 4 + 3, y + 2]}))});
      emit(LABELS[i], 20, starts[i]); split.forEach((v, n) => emit(v, 100 + (i ? changes.bodyShift || 0 : 0) + (n ? changes.wrapShift || 0 : 0), starts[i] + n * (line * unit + (changes.lineShift || 0))));
    }
    const text = LABELS.map((l, i) => l + ' ' + TITLES[i]).join('\n');
    return {text, pypdfText: text, pages: [{width: 1200, height: 842, glyphTraces: traces}]};
  }
  let checks = 0;
  const baseline = comparePdf(measurePdf(fixture(1, 0)), CASES[0]); assert(baseline.passed); checks++;
  const unit = {unit: baseline.unit, lineCounts: baseline.lineCounts};
  for (const item of CASES) {assert(comparePdf(measurePdf(fixture(item.line, item.entry)), item, unit).passed); checks++;}
  for (const change of [{bodyShift: 0.76}, {wrapShift: 0.76}, {entryShift: 1.01}, {lineShift: 1.01}]) {
    assert.equal(comparePdf(measurePdf(fixture(3, 2, change)), CASES[3], unit).passed, false); checks++;
  }
  const missing = fixture(1, 0); missing.pypdfText = missing.pypdfText.replace('alpha ', ''); assert.throws(() => measurePdf(missing)); checks++;
  const lostGlyph = fixture(1, 0); lostGlyph.pages[0].glyphTraces[1].chars.splice(8, 1); assert.throws(() => measurePdf(lostGlyph)); checks++;
  const item = CASES[3], word = {rows: TITLES.map((text, i) => ({text: LABELS[i] + text, actualRunTabs: 1,
    tabs: [{val: 'left', pos: '600'}], indent: {left: '600', hanging: '600'},
    spacing: {lineRule: 'auto', line: '720', after: '0', afterLines: i ? '0' : '600'}}))};
  checkDocxPackage(word, item); checks++;
  for (const mutate of [x => x.rows[0].spacing.afterLines = '200', x => x.rows[1].spacing.afterLines = '600', x => x.rows[1].tabs[0].pos = '720']) {
    const invalid = structuredClone(word); mutate(invalid); assert.throws(() => checkDocxPackage(invalid, item)); checks++;
  }
  const epub = {tableCount: 1, role: 'presentation', tableLineHeight: '3', stylesheet: {nowrapFirstField: true, sharedFullWidthTable: true},
    rows: TITLES.map((text, i) => [{text: LABELS[i], columnSpan: null, className: 'bibliography-first-field', paddingBottom: i ? '0em' : '6em'},
      {text, columnSpan: null, className: 'bibliography-content', paddingBottom: i ? '0em' : '6em'}])};
  checkEpub(epub, item); checks++;
  for (const mutate of [x => x.rows[0][0].paddingBottom = '2em', x => x.rows[1][1].paddingBottom = '6em', x => x.tableCount = 2]) {
    const invalid = structuredClone(epub); mutate(invalid); assert.throws(() => checkEpub(invalid, item)); checks++;
  }
  const reader = {source: 'original', sharedContainer: 1, horizontalOverflow: false, rows: TITLES.map((text, i) => ({text, labelText: LABELS[i],
    fontSize: 16, lineHeight: 48, marginBottom: i ? 0 : 96, row: {right: 500}, label: {right: 95}, lines: [{x: 100, right: 450}, {x: 100, right: 400}]}))};
  checkReader(reader, item, 'original'); checks++;
  for (const mutate of [x => x.rows[0].marginBottom = 32, x => x.rows[1].lines[0].x = 101.01, x => x.rows[0].lines[1].x = 80]) {
    const invalid = structuredClone(reader); mutate(invalid); assert.throws(() => checkReader(invalid, item, 'original')); checks++;
  }
  console.log(JSON.stringify({passed: true, selfTestOnly: true, productInvocations: 0, checks, tolerance: TOLERANCE}));
}
module.exports = {measurePdf, comparePdf, checkDocxPackage, checkEpub, checkReader, TOLERANCE, TITLES, CASES};
if (require.main === module) {
  if (process.argv.includes('--self-test')) selfTest(); else run().catch(error => {console.error(error.stack || error.message); process.exitCode = 1;});
}
