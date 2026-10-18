'use strict';
// Real JSON-RPC stdio integration. Final root-selected binary; no API mocks.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const {createHarness, fileHash, sha256} = require('./publication-batch3-harness.cjs');
const {paperMetadata, bookMetadata, sourceFor, paperBody, bookBody, references, contextBody, richStyle, styles} = require('./publication-batch3-fixtures.cjs');
const {assertMetadata, checkSpans, inspectArtifact, checkMetadataArtifact, checkRichArtifact} = require('./publication-batch3-checks.cjs');
const h = createHarness('publication-context-mcp', 29129, __filename);
delete h.report.nativeCsp; h.report.transport = 'stdio';
const paper = sourceFor(paperMetadata, paperBody, {bom: true, crlf: true}), book = sourceFor(bookMetadata, bookBody);
const context = '---\ntitle: Synthetic context over MCP\nbibliography: references.json\ncsl: apa\nlanguage: en-US\n---\n\n' + contextBody;
const rich = '---\ntitle: Typed citation over MCP\nbibliography: references.json\ncsl: rich.csl\nlanguage: en-US\n---\n\nRead [@study].\n';
for (const [name, value] of Object.entries({'Paper.md': paper, 'Book.md': book, 'Context.md': context, 'Rich.md': rich, 'references.json': JSON.stringify(references), 'rich.csl': richStyle})) fs.writeFileSync(path.join(h.docs, name), value);
const initial = h.snapshotInputs(); let child, next = 1, buffer = '';
// Owned profile fixture for the actual preset -> request merge path. Native
// preset creation/persistence is independently exercised in the native suite.
const layeredPresetName = 'QA metadata clear';
fs.writeFileSync(path.join(h.data, 'export_presets.json'), JSON.stringify({custom: {[layeredPresetName]: {cover: {enabled: false}, toc: {enabled: false}, meta: {clear: ['subtitle'], authors: []}}}, last: null}));
const pending = new Map();
function rejectAll(message) { for (const item of pending.values()) { clearTimeout(item.timer); item.reject(Error(message)); } pending.clear(); }
function request(method, params) {
  return new Promise((resolve, reject) => {
    const id = next++, timer = setTimeout(() => {pending.delete(id); reject(Error('Real MCP request timed out: ' + method));}, 120000);
    pending.set(id, {resolve, reject, timer});
    child.stdin.write(JSON.stringify({jsonrpc: '2.0', id, method, params}) + '\n', error => {if (error) {clearTimeout(timer); pending.delete(id); reject(Error('MCP input closed'));}});
  });
}
async function call(name, args, {error = false} = {}) {
  const result = await request('tools/call', {name, arguments: args});
  const data = result.structuredContent || JSON.parse(result.content.find(item => item.type === 'text').text);
  if (error) { assert.equal(result.isError, true, 'Invalid operation was accepted by real MCP'); assert.equal(data.ok, false); }
  else { assert.notEqual(result.isError, true, JSON.stringify(data)); assert.equal(data.ok, true, JSON.stringify(data)); }
  return data;
}
async function launch() {
  const log = fs.openSync(path.join(h.data, 'stderr.log'), 'a');
  child = spawn(process.env.READMD_BIN, ['--mcp', '--data-dir', h.data, '--workspace', h.docs, '--assets', process.env.READMD_ASSETS_DIR || path.join(h.root, 'assets')],
    {cwd: h.root, windowsHide: true, stdio: ['pipe', 'pipe', log]}); fs.closeSync(log);
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', chunk => {
    buffer += chunk;
    if (Buffer.byteLength(buffer) > 16 * 1024 * 1024) {rejectAll('MCP output exceeded its QA budget'); child.kill(); return;}
    for (;;) {
      const end = buffer.indexOf('\n'); if (end < 0) break;
      const line = buffer.slice(0, end); buffer = buffer.slice(end + 1); if (!line.trim()) continue;
      let message; try {message = JSON.parse(line);} catch {rejectAll('MCP emitted invalid JSON'); child.kill(); return;}
      const item = pending.get(message.id); if (!item) continue;
      clearTimeout(item.timer); pending.delete(message.id);
      message.error ? item.reject(Error('MCP protocol error: ' + JSON.stringify(message.error))) : item.resolve(message.result);
    }
  });
  child.on('error', () => rejectAll('MCP process launch failed'));
  child.on('exit', () => rejectAll('MCP process stopped before replying'));
  child.stdin.on('error', () => rejectAll('MCP stdin failed'));
  await request('initialize', {protocolVersion: '2024-11-05', capabilities: {}, clientInfo: {name: 'readmd-publication-batch3-independent-qa', version: '1'}});
  child.stdin.write(JSON.stringify({jsonrpc: '2.0', method: 'notifications/initialized'}) + '\n');
}
async function stop() {
  if (!child) return;
  rejectAll('Owned MCP session closing'); child.stdin.end();
  if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve));
    await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 1500))]);
    if (child.exitCode === null && child.signalCode === null) {child.kill(); await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000))]);}
  }
  h.report.processStopped = child.exitCode !== null || child.signalCode !== null;
  assert.equal(h.report.processStopped, true, 'Owned MCP process did not stop');
}
h.finish(async () => {
  try {
    await h.step('real stdio initialization and shared metadata/citation/export schemas', async () => {
      await launch(); const catalog = await request('tools/list', {});
      for (const name of ['readmd_publication_metadata', 'readmd_academic_document', 'readmd_content_transaction']) {
        const tool = catalog.tools.find(item => item.name === name); assert(tool, 'Missing real MCP tool: ' + name);
        assert.equal(tool.annotations.readOnlyHint, true, 'In-memory operations must not claim disk writes');
      }
      const epub = catalog.tools.find(item => item.name === 'readmd_export_epub');
      assert.equal(epub.inputSchema.properties.options.type, 'object', 'EPUB lost the shared options schema');
      h.report.inputs = initial;
    });
    await h.step('same independent paper/book metadata and exact in-memory transaction/inverse', async () => {
      for (const [content, expected, source_name] of [[paper, paperMetadata, 'Paper.md'], [book, bookMetadata, 'Book.md']]) {
        const result = await call('readmd_publication_metadata', {action: 'get', content, source_name});
        assertMetadata(result.metadata, expected); assert.equal(result.revision, sha256(content));
      }
      const authors = structuredClone(paperMetadata.authors); authors[0].name = 'MCP Only, Linh';
      const patch = await call('readmd_publication_metadata', {action: 'patch', content: paper, base_revision: sha256(paper), patch: {authors}});
      assert.equal(patch.content, paper.replace('name: "Nguyen, Linh"', 'name: "MCP Only, Linh"'));
      assert.equal(patch.transaction.edits.length, 1);
      const inverse = await call('readmd_content_transaction', {content: patch.content, transaction: {base_revision: patch.revision, origin: 'editor', edits: patch.inverse}});
      assert.equal(inverse.result.content, paper);
      const stale = await call('readmd_publication_metadata', {action: 'patch', content: patch.content, base_revision: sha256(paper), patch: {subtitle: 'stale'}}, {error: true});
      assert.equal(stale.error_code, 'transaction_conflict'); h.unchanged(initial);
    });
    await h.step('five styles over real shared document tool retain rich/source contexts and original bytes', async () => {
      for (const style of styles) {
        const result = await call('readmd_academic_document', {content: context, baseDir: h.docs, options: {citations: {style}}});
        checkSpans(context, result); assert.equal(result.projection.revision, sha256(context));
        assert(result.projection.citations.every(item => Array.isArray(item.rich)));
        h.unchanged(initial);
      }
      const result = await call('readmd_academic_document', {content: rich, baseDir: h.docs});
      const nodes = JSON.stringify(result.projection.citations);
      for (const value of ['small-caps', 'italic', 'underline', 'sup', 'sub', 'https://example.invalid/study']) assert(nodes.includes(value), 'Real MCP projection lost: ' + value);
      h.unchanged(initial);
    });
    await h.step('actual MCP preset clear and later request values obey metadata layer order', async () => {
      const raw = await request('tools/call', {name: 'readmd_export_presets', arguments: {}}); assert.notEqual(raw.isError, true);
      const catalog = raw.structuredContent || JSON.parse(raw.content.find(item => item.type === 'text').text);
      assert.deepEqual(catalog.custom[layeredPresetName].meta.clear, ['subtitle']);
      const expected = structuredClone(bookMetadata); expected.subtitle = 'Later request value';
      const overridden = path.join(h.outputs, 'MCP-layered.docx');
      await call('readmd_export_document', {markdown_content: book, output_path: overridden, output_format: 'docx', base_dir: h.docs, style_preset: layeredPresetName, confirm: true,
        options: {meta: {authors: bookMetadata.authors, subtitle: expected.subtitle}}});
      checkMetadataArtifact(inspectArtifact(h, overridden), expected);
      const cleared = structuredClone(bookMetadata); cleared.authors = []; cleared.subtitle = null;
      const clearedFile = path.join(h.outputs, 'MCP-cleared.docx');
      await call('readmd_export_document', {markdown_content: book, output_path: clearedFile, output_format: 'docx', base_dir: h.docs, style_preset: layeredPresetName, confirm: true});
      checkMetadataArtifact(inspectArtifact(h, clearedFile), cleared); h.unchanged(initial);
    });
    await h.step('actual MCP Word export carries native typed citation runs and metadata', async () => {
      const output = path.join(h.outputs, 'MCP-rich.docx');
      await call('readmd_export_document', {markdown_content: rich, output_path: output, output_format: 'docx', base_dir: h.docs, confirm: true,
        options: {cover: {enabled: false}, toc: {enabled: false}, meta: {authors: bookMetadata.authors, subtitle: 'MCP typed override'}}});
      checkRichArtifact(inspectArtifact(h, output));
      h.report.docxSha256 = fileHash(output); h.unchanged(initial);
    });
    await h.step('actual MCP EPUB uses the same structured overrides and passes locked offline EPUBCheck', async () => {
      const expected = structuredClone(bookMetadata); expected.subtitle = 'MCP subtitle override';
      const output = path.join(h.outputs, 'MCP-book.epub');
      await call('readmd_export_epub', {markdown_content: book, output_path: output, base_dir: h.docs, confirm: true,
        options: {frontMatter: {mode: 'book'}, cover: {enabled: false}, meta: {subtitle: expected.subtitle}}});
      checkMetadataArtifact(inspectArtifact(h, output), expected, 'book');
      const jar = process.env.READMD_EPUBCHECK_JAR || path.join(h.root, '.cache/capability-upgrade/tools/epubcheck-5.4.0/epubcheck.jar');
      assert(fs.existsSync(jar), 'The existing locked offline EPUBCheck jar is required');
      const {checkEpub} = await import('../check-epub.mjs'); const validation = await checkEpub([output], {jar, output: path.join(h.data, 'epubcheck.json')});
      assert.equal(validation.passed, true); h.report.epubSha256 = fileHash(output);
      const digest = fileHash(output);
      const invalid = await call('readmd_export_epub', {markdown_content: book, output_path: output, base_dir: h.docs, confirm: true, overwrite: true,
        options: {meta: {identifiers: [{scheme: 'isbn', value: '978-0-00-000000-9'}]}}}, {error: true});
      assert.match(invalid.error_code, /^publication_metadata_/); assert.equal(fileHash(output), digest); h.unchanged(initial);
    });
    await h.step('real MCP core-font PDF preserves astral Unicode and rejects unavailable glyphs without replacing files', async () => {
      const output = path.join(h.outputs, 'MCP-core-unicode.pdf');
      const options = {cover: {enabled: false}, toc: {enabled: false}, frontMatter: {mode: 'none'}};
      await call('readmd_export_document', {markdown_content: 'Portable Unicode 😀 remains searchable.\n', output_path: output, output_format: 'pdf', base_dir: h.docs, confirm: true, options});
      const inspected = inspectArtifact(h, output);
      assert(inspected.text.includes('😀'), 'MuPDF lost the original astral scalar');
      assert(inspected.pypdfText.includes('😀'), 'Independent pypdf extraction lost the astral scalar');
      const digest = fileHash(output), absent = path.join(h.outputs, 'MCP-unavailable-glyph.pdf');
      for (const target of [output, absent]) {
        const rejected = await call('readmd_export_document', {markdown_content: 'Unavailable \u{10ffff}\n', output_path: target, output_format: 'pdf', base_dir: h.docs, confirm: true, overwrite: true, options}, {error: true});
        assert.equal(rejected.error_code, 'pdf_missing_glyph');
        assert.equal(rejected.details.codepoint, 'U+10FFFF'); assert.equal(rejected.details.coordinateSpace, 'pdfSpanUtf16');
        assert.equal(rejected.details.to - rejected.details.from, 2); assert.equal(rejected.details.page, 1);
      }
      assert.equal(fileHash(output), digest); assert.equal(fs.existsSync(absent), false); h.unchanged(initial);
      h.report.coreUnicodePdf = {file: path.basename(output), sha256: digest, bothIndependentExtractorsPreserveScalar: true, unavailableGlyphRejectedAtomically: true};
    });
  } finally { await stop(); }
});
