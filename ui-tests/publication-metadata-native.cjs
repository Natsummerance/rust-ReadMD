'use strict';
// Root-selected Windows candidate, production CSP, real source files/exports.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {createHarness, fileHash, sha256} = require('../tools/test/publication-batch3-harness.cjs');
const {paperMetadata, bookMetadata, sourceFor, paperBody, bookBody, references} = require('../tools/test/publication-batch3-fixtures.cjs');
const {assertMetadata, inspectArtifact, checkMetadataArtifact} = require('../tools/test/publication-batch3-checks.cjs');
const {metadataFieldCases, exportMetadataArtifact, runMetadataFieldMatrix, contentLabelsPath} = require('../tools/test/publication-metadata-field-matrix.cjs');
const h = createHarness('publication-metadata-native', 29123, __filename);
const paper = sourceFor(paperMetadata, paperBody, {bom: true, crlf: true});
const book = sourceFor(bookMetadata, bookBody);
const nativeText = content => content.replace(/^\uFEFF/, '');
const paperFile = path.join(h.docs, 'Paper.md'), bookFile = path.join(h.docs, 'Book.md');
fs.writeFileSync(paperFile, paper); fs.writeFileSync(bookFile, book);
fs.writeFileSync(path.join(h.docs, 'references.json'), JSON.stringify(references));
let expectedInputs = h.snapshotInputs(), patched, page;
h.report.inputs = {...expectedInputs};
const renamed = structuredClone(paperMetadata); renamed.authors[0].name = 'Nguyen, Linh revised';
const metadata = body => h.api('/api/publication/metadata', body);
const baseOptions = {pdf: {engine: 'rust'}, typography: {font: 'Arial', size: 11}, cover: {enabled: false}, toc: {enabled: false}, footer: {pageNumbers: false}};
fs.copyFileSync(__filename, path.join(h.data, 'qa-script.cjs'));
process.env.READMD_NATIVE_LOG_FILE ||= path.join(h.data, 'native-host.log');
h.report.archivedQaScript = 'qa-script.cjs';
h.report.scope = 'full-publication-metadata-native';
const matrixHelper = path.join(h.root, 'tools/test/publication-metadata-field-matrix.cjs');
h.report.qaDependencySha256['tools/test/publication-metadata-field-matrix.cjs'] = fileHash(matrixHelper);
fs.copyFileSync(matrixHelper, path.join(h.data, 'qa-metadata-field-matrix.cjs'));
h.report.contentLabelsSha256 = fileHash(contentLabelsPath);

async function openProperties() {
  await page.evaluate(() => openFrontmatterModal());
  await page.locator('#frontmatter-modal').waitFor({state: 'visible'});
  await page.locator('#publication-metadata-editor [data-pm-field="authors"] [data-pm-row]').first().waitFor({state: 'attached'});
}
async function metadataExport() {
  await page.evaluate(() => openExportModal());
  await page.locator('#export-tab-docx').click();
  const details = page.locator('#export-box .export-customize');
  if (!await details.evaluate(element => element.open)) await details.locator(':scope > summary').click();
  const host = page.locator('#publication-metadata-export');
  await host.waitFor({state: 'attached'});
  const section = page.locator('#export-opts .exp-sec').filter({has: host});
  if (await section.count() && !await section.evaluate(element => element.classList.contains('open'))) await section.locator('.exp-sec-head').click();
  // The host is mounted before its inherited values arrive from the service.
  // Changing authors to value before the real rows load would change the test.
  await host.locator('[data-pm-field="authors"] [data-pm-row]').first().waitFor({state: 'attached'});
  return host;
}
function applyEdits(content, edits) {
  for (const edit of [...edits].sort((a, b) => b.from - a.from)) content = content.slice(0, edit.from) + edit.insert + content.slice(edit.to);
  return content;
}
async function exportFile(...args) { return exportMetadataArtifact(h, baseOptions, ...args); }
h.finish(async () => {
  assert.match(process.env.READMD_CANDIDATE_BOOT_SHA256 || '', /^[a-f0-9]{64}$/i, 'Root must supply the unique final boot hash');
  const expectedBoot = process.env.READMD_CANDIDATE_BOOT_SHA256.toLowerCase();
  assert.equal(fileHash(path.join(process.env.READMD_ASSETS_DIR || path.join(h.root, 'assets'), 'readmd.boot.js')), expectedBoot, 'Boot differs from root’s supplied final candidate');
  h.report.binarySha256 = h.report.candidateSha256; h.report.expectedBootSha256 = expectedBoot;
  page = await h.start();
  assert.equal(h.report.bootSha256, expectedBoot);
  await h.step('production CSP and offline managed pack installation', async () => {
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
    assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
    await h.installPacks();
  });
  await h.step('independent paper/book input expectations and structured metadata', async () => {
    for (const [content, expected, source_name] of [[paper, paperMetadata, 'Paper.md'], [book, bookMetadata, 'Book.md']]) {
      const result = await metadata({action: 'get', content, source_name});
      assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true);
      assertMetadata(result.data.metadata, expected); assert.equal(result.data.revision, sha256(content));
      assert(result.data.ranges && typeof result.data.ranges === 'object');
    }
    const dangling = await metadata({action: 'get', content: paper, options: {meta: {authors: [], subtitle: null, clear: ['publisher']}}});
    assert.equal(dangling.status, 422, JSON.stringify(dangling.data));
    assert.equal(dangling.data.error_code, 'publication_metadata_reference'); h.unchanged(expectedInputs);
    const cleared = await metadata({action: 'get', content: paper, options: {meta: {authors: [], funding: [], subtitle: null, clear: ['publisher']}}});
    assert.equal(cleared.status, 200, JSON.stringify(cleared.data)); assert.deepEqual(cleared.data.metadata.authors, []);
    assert.deepEqual(cleared.data.metadata.funding, []);
    assert.equal(cleared.data.metadata.subtitle, paperMetadata.subtitle, 'null must inherit');
    assert.equal(cleared.data.metadata.publisher ?? null, null);
    h.unchanged(expectedInputs);
  });
  await h.step('single-name exact UTF-16 transaction, inverse, no-op and stale rejection', async () => {
    const result = await metadata({action: 'patch', content: paper, source_name: 'Paper.md', base_revision: sha256(paper), patch: {authors: renamed.authors}});
    assert.equal(result.status, 200, JSON.stringify(result.data)); patched = result.data.content;
    const expected = paper.replace('name: "Nguyen, Linh"', 'name: "Nguyen, Linh revised"');
    assert.equal(patched, expected, 'One author edit reserialized unrelated front matter, trivia, BOM or EOL');
    assert.equal(result.data.transaction.edits.length, 1); assert.equal(result.data.changed, true);
    assert.equal(applyEdits(paper, result.data.transaction.edits), patched);
    assert.equal(applyEdits(patched, result.data.inverse), paper);
    const chinese = structuredClone(paperMetadata.authors); chinese[1].name = '林墨校订';
    const funding = structuredClone(paperMetadata.funding); funding[0].award = 'QA-000043';
    for (const [patch, before, after] of [[{authors: chinese}, 'name: "林墨"', 'name: "林墨校订"'],
      [{funding}, 'award: "QA-000042"', 'award: "QA-000043"']]) {
      const one = await metadata({action: 'patch', content: paper, base_revision: sha256(paper), patch});
      assert.equal(one.status, 200, JSON.stringify(one.data)); assert.equal(one.data.content, paper.replace(before, after));
      assert.equal(one.data.transaction.edits.length, 1, 'A field after Chinese/emoji was not one precise UTF-16 edit');
      assert.equal(applyEdits(one.data.content, one.data.inverse), paper);
    }
    const noop = await metadata({action: 'patch', content: paper, base_revision: sha256(paper), patch: {authors: paperMetadata.authors}});
    assert.equal(noop.data.changed, false); assert.deepEqual(noop.data.transaction.edits, []);
    const stale = await metadata({action: 'patch', content: patched, base_revision: sha256(paper), patch: {subtitle: 'stale'}});
    assert.equal(stale.status, 409); assert.equal(stale.data.error_code, 'transaction_conflict');
    h.unchanged(expectedInputs);
  });
  await h.step('native structured properties, cancel and one-step keyboard undo/redo', async () => {
    await page.evaluate(async file => {await i18n.setLanguage('en', false); await loadFile(file); await toggleEdit(); setEditorPref('aiCompletion', false); setPvLayout('right');}, paperFile);
    assert.equal(await page.evaluate(() => getEditContent()), nativeText(paper));
    await openProperties();
    const field = page.locator('#publication-metadata-editor [data-pm-field="authors"] [data-pm-row]').first().locator('[data-pm-key="name"]');
    await field.fill(renamed.authors[0].name); await page.locator('#fm-modal-cancel').click();
    assert.equal(await page.evaluate(() => getEditContent()), nativeText(paper));
    await openProperties(); await field.fill(renamed.authors[0].name); await page.locator('#fm-modal-insert').click();
    await h.until(() => page.locator('#frontmatter-modal').isHidden());
    assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched));
    assert.equal(fs.readFileSync(paperFile, 'utf8'), paper, 'Dialog committed to disk before Save');
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z');
    assert.equal(await page.evaluate(() => getEditContent()), nativeText(paper));
    await page.keyboard.press('Control+y'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched));
    await openProperties(); await page.locator('#fm-modal-insert').click(); await h.until(() => page.locator('#frontmatter-modal').isHidden());
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(paper), 'No-op submit inserted an undo step');
    await page.keyboard.press('Control+y'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched));
  });
  await h.step('native add/remove/reorder controls retain stable person/affiliation identities', async () => {
    await openProperties();
    const authors = page.locator('#publication-metadata-editor [data-pm-field="authors"]');
    await authors.locator('[data-pm-row]').first().locator('[data-pm-action="down"]').click();
    await page.locator('#fm-modal-insert').click(); await h.until(() => page.locator('#frontmatter-modal').isHidden());
    const ordered = await page.evaluate(() => getEditContent());
    fs.writeFileSync(path.join(h.data, 'ordered-draft.md'), ordered);
    const expected = structuredClone(renamed); expected.authors.reverse();
    const parsed = await metadata({action: 'get', content: ordered}); assertMetadata(parsed.data.metadata, expected);
    assert(ordered.includes('# This comment travels with the second author.\r\n  - id: "mo"'), 'Reorder detached an author’s original comment');
    assert(ordered.endsWith(patched.slice(patched.indexOf('\r\n---\r\n') + 7)), 'Author reorder changed the document body');
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched));
    await openProperties(); await authors.locator('[data-pm-action="add"]').click();
    const third = authors.locator('[data-pm-row]').last(); await third.locator('[data-pm-key="name"]').fill('Synthetic Third Author');
    await page.locator('#fm-modal-insert').click(); await h.until(() => page.locator('#frontmatter-modal').isHidden());
    const added = await metadata({action: 'get', content: await page.evaluate(() => getEditContent())});
    assert.equal(added.data.metadata.authors.length, 3); assert.equal(added.data.metadata.authors[2].name, 'Synthetic Third Author');
    assert.deepEqual(added.data.metadata.authors.slice(0, 2).map(person => person.id), ['linh', 'mo']);
    await openProperties(); await authors.locator('[data-pm-row]').last().locator('[data-pm-action="remove"]').click();
    await page.locator('#fm-modal-insert').click(); await h.until(() => page.locator('#frontmatter-modal').isHidden());
    const removed = await metadata({action: 'get', content: await page.evaluate(() => getEditContent())}); assertMetadata(removed.data.metadata, renamed);
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z');
    assert.equal((await metadata({action: 'get', content: await page.evaluate(() => getEditContent())})).data.metadata.authors.length, 3);
    await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched));
    assert.equal(fs.readFileSync(paperFile, 'utf8'), paper, 'Structured row controls wrote before Save');
  });
  await h.step('native stale property snapshot keeps intervening typed text', async () => {
    await openProperties();
    await page.locator('#publication-metadata-editor [data-pm-field="authors"] [data-pm-row]').first().locator('[data-pm-key="name"]').fill('Stale Name Must Not Apply');
    await page.evaluate(() => cmView.dispatch({changes: {from: cmView.state.doc.length, insert: '\nStale snapshot sentinel.\n'}, userEvent: 'input.type'}));
    const typed = await page.evaluate(() => getEditContent());
    await page.locator('#fm-modal-insert').click();
    assert.equal(await page.evaluate(() => getEditContent()), typed); assert.equal(await page.locator('#frontmatter-modal').isVisible(), true);
    assert(!typed.includes('Stale Name Must Not Apply')); await page.locator('#fm-modal-cancel').click();
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched));
  });
  await h.step('real Save, reopen and complete source-preserving metadata', async () => {
    await page.locator('#edit-save').click(); await h.until(() => Promise.resolve(fs.readFileSync(paperFile, 'utf8') === patched));
    expectedInputs['Paper.md'] = fileHash(paperFile);
    await page.evaluate(async file => {await exitEdit(); await loadFile(file);}, paperFile);
    assert.equal(await page.evaluate(() => state.original), nativeText(patched));
    const parsed = await metadata({action: 'get', content: fs.readFileSync(paperFile, 'utf8')}); assertMetadata(parsed.data.metadata, renamed);
    h.unchanged(expectedInputs);
  });
  await h.step('native typed export overrides, sparse clear preset and format switching', async () => {
    const host = await metadataExport();
    await host.locator('[data-pm-field="authors"] [data-pm-state]').selectOption('value');
    await host.locator('[data-pm-field="authors"] [data-pm-row]').first().locator('[data-pm-key="name"]').fill('Export Only, Linh');
    await host.locator('[data-pm-field="subtitle"] [data-pm-state]').selectOption('clear');
    const options = await page.evaluate(() => collectExportOptions());
    assert(Array.isArray(options.meta.authors)); assert.equal(options.meta.authors.length, 2);
    assert.equal(options.meta.authors[0].name, 'Export Only, Linh'); assert.deepEqual(options.meta.authors[0].affiliationIds, ['north', 'east']);
    assert(options.meta.clear.includes('subtitle')); assert.equal(options.meta.abstracts, undefined, 'Inherited abstracts were materialized into a preset');
    await page.locator('#exp-save-preset').click(); await page.locator('#exp-save-input').fill('QA-publication-complex'); await page.locator('#exp-save-ok').click();
    await h.until(() => page.locator('#exp-save-name').isHidden());
    for (const format of ['epub', 'pdf', 'docx']) {
      await page.locator('#export-tab-' + format).click();
      const selected = await page.evaluate(() => collectExportOptions());
      assert.equal(selected.meta.authors[0].name, 'Export Only, Linh'); assert(selected.meta.clear.includes('subtitle'));
      assert(Array.isArray(selected.meta.authors) && Array.isArray(selected.meta.authors[0].affiliationIds));
    }
    h.unchanged(expectedInputs); await page.evaluate(() => closeExportModal());
  });
  await h.step('six actual DOCX/Rust PDF/EPUB paper and translated-book outputs', async () => {
    for (const [name, content, expected, mode] of [['Paper', patched, renamed, 'paper'], ['Book', book, bookMetadata, 'book']]) {
      for (const format of ['docx', 'pdf', 'epub']) {
        const result = await exportFile(name, format, content, expected, mode);
        if (format === 'docx') assert.equal(result.inspected.text.split(expected.title).length - 1, 1, 'Cover and front matter duplicated the title page');
        if (format === 'pdf') {
          const opened = await h.api('/api/pdf/workspace', {action: 'open', path: result.file, width: 1000});
          assert.equal(opened.status, 200); assert.equal(opened.data.ok, true); assert(opened.data.image);
          fs.writeFileSync(path.join(h.data, name + '-pdfium.bmp'), Buffer.from(opened.data.image, 'base64'));
          await h.api('/api/pdf/workspace', {action: 'close', session: opened.data.session});
        }
      }
    }
    h.unchanged(expectedInputs);
  });
  await h.step('actual format-specific EPUB overrides do not leak into DOCX metadata', async () => {
    const shared = {meta: {title: 'Generic override', authors: []}, epub: {title: 'EPUB only title', author: 'EPUB Only, Contributor', language: 'fr-CA'}};
    const wordExpected = structuredClone(bookMetadata); wordExpected.title = shared.meta.title; wordExpected.authors = [];
    await exportFile('Format-DOCX', 'docx', book, wordExpected, 'none', shared);
    const epubExpected = structuredClone(bookMetadata); epubExpected.title = shared.epub.title;
    epubExpected.authors = [{name: shared.epub.author, affiliationIds: []}]; epubExpected.language = shared.epub.language;
    await exportFile('Format-EPUB', 'epub', book, epubExpected, 'none', shared); h.unchanged(expectedInputs);
  });
  await h.step('35 independent per-field A/B cases produce actual DOCX, Rust PDF and EPUB files', () => runMetadataFieldMatrix(h, baseOptions, expectedInputs));
  await h.step('all actual EPUB outputs including the original three pass the locked offline independent validator', async () => {
    const epubs = fs.readdirSync(h.outputs).filter(name => name.endsWith('.epub')).map(name => path.join(h.outputs, name));
    assert(epubs.length >= 3, 'The original three EPUB gates are required');
    for (const name of ['Paper.epub', 'Book.epub', 'Format-EPUB.epub']) assert(epubs.some(file => path.basename(file) === name), 'Missing original EPUB gate: ' + name);
    assert.equal(epubs.length, 3 + metadataFieldCases().length + 2, 'A per-field EPUB or A baseline was omitted');
    const jar = process.env.READMD_EPUBCHECK_JAR || path.join(h.root, '.cache/community-upgrade/qa-tools/epubcheck-v5.4.0/epubcheck-5.4.0/epubcheck.jar');
    assert(fs.existsSync(jar), 'The existing locked offline EPUBCheck jar is required');
    const {checkEpub} = await import('../tools/check-epub.mjs');
    const validation = await checkEpub(epubs, {jar, output: path.join(h.data, 'epubcheck.json')}); assert.equal(validation.passed, true);
    h.report.epubcheck = validation.files.map(item => ({name: item.name, counts: item.counts, sha256: item.sha256}));
  });
  await h.step('hidden PDF metadata A/B preserves actual page objects and pixels', async () => {
    const a = await exportFile('Hidden-A', 'pdf', patched, renamed, 'none');
    const bMetadata = structuredClone(renamed); bMetadata.subtitle = 'Hidden override B';
    const b = await exportFile('Hidden-B', 'pdf', patched, bMetadata, 'none', {meta: {subtitle: bMetadata.subtitle}});
    const compared = inspectArtifact(h, b.file, {baseline: a.file});
    assert.equal(compared.comparison.samePageObjects, true); assert.equal(compared.comparison.sameNonMetadataObjects, true); assert.equal(compared.comparison.samePagePixels, true);
    h.report.hiddenPdfComparison = compared.comparison;
  });
  await h.step('invalid relationships/identifiers and missing glyph protect completed files', async () => {
    const old = path.join(h.outputs, 'Paper.docx'), digest = fileHash(old);
    const invalid = [
      {authors: [{...paperMetadata.authors[0], affiliationIds: ['missing-institution']}]},
      {authors: [{...paperMetadata.authors[0], orcid: '0000-0000-0000-0000'}]},
      {identifiers: [{scheme: 'isbn', value: '978-0-00-000000-9', primary: true}]},
    ];
    for (const meta of invalid) {
      const result = await h.api('/api/export', {format: 'docx', content: patched, baseDir: h.docs, out_path: old, options: {...baseOptions, meta}});
      assert.equal(result.data.ok, false); assert.equal(fileHash(old), digest);
      assert.match(result.data.error_code || '', /^publication_metadata_/);
      assert(typeof result.data.path === 'string' || typeof result.data.field === 'string', 'Metadata error lacks its field path');
    }
    const oldPdf = path.join(h.outputs, 'Paper.pdf'), pdfDigest = fileHash(oldPdf);
    const absentPdf = path.join(h.outputs, 'Missing-glyph-must-not-exist.pdf');
    const expected = {codepoint: 'U+10FFFF', page: 1, item: 1, span: 2, coordinateSpace: 'pdfSpanUtf16', from: 0, to: 2};
    h.report.missingGlyphRejections = [];
    for (const destination of [oldPdf, absentPdf]) {
      const request = {format: 'pdf', content: 'Unicode \u{10ffff}\n', baseDir: h.docs,
        out_path: destination, options: {...baseOptions, frontMatter: {mode: 'none'}}};
      const response = await h.api('/api/export', request);
      const evidence = {request, expected, response, originalPdfSha256: pdfDigest,
        actualPdfSha256: fileHash(oldPdf), absentPdfCreated: fs.existsSync(absentPdf)};
      h.report.missingGlyphRejections.push(evidence);
      fs.writeFileSync(path.join(h.data, 'missing-glyph-rejections.json'), JSON.stringify(h.report.missingGlyphRejections, null, 2) + '\n');
      assert.equal(response.status, 200); assert.equal(response.data.ok, false);
      assert.equal(response.data.error_code, 'pdf_missing_glyph');
      for (const [key, value] of Object.entries(expected)) assert.deepEqual(response.data.details?.[key], value, 'Missing glyph location: ' + key);
      assert(Number.isFinite(response.data.details.x) && Number.isFinite(response.data.details.y));
      assert.equal(evidence.actualPdfSha256, pdfDigest); assert.equal(evidence.absentPdfCreated, false);
    }
    h.unchanged(expectedInputs);
  });
  await h.step('real host restart preserves structured source and sparse preset types', async () => {
    await page.evaluate(() => ReadMDSession.flush()); await h.stop(); page = await h.start();
    await page.evaluate(file => loadFile(file), paperFile); assert.equal(await page.evaluate(() => state.original), nativeText(patched));
    await metadataExport();
    const presets = page.locator('#export-box .export-preset-list');
    if (!await presets.evaluate(element => element.open)) await presets.locator(':scope > summary').click();
    await page.locator('#exp-preset').selectOption('QA-publication-complex');
    const options = await page.evaluate(() => collectExportOptions()); assert.equal(options.meta.authors[0].name, 'Export Only, Linh');
    assert(Array.isArray(options.meta.authors[0].affiliationIds)); assert(options.meta.clear.includes('subtitle'));
    assert.equal(options.meta.abstracts, undefined); h.unchanged(expectedInputs); await page.evaluate(() => closeExportModal());
  });
  await h.step('light/dark reference sizes and Arabic structured-property controls', async () => {
    await page.evaluate(() => toggleEdit());
    for (const [width, height, language, theme] of [[1024, 680, 'en', 'light'], [1160, 820, 'de', 'dark'], [1024, 680, 'ar', 'dark']]) {
      await page.setViewportSize({width, height});
      for (let count = 0; count < 4 && await page.locator('body').getAttribute('data-theme') !== theme; count++) {
        const before = await page.evaluate(() => state.theme);
        await page.locator('#btn-theme').click();
        await h.until(() => page.evaluate(choice => state.theme !== choice, before));
        await h.until(() => page.evaluate(() => !document.documentElement.classList.contains('rm-theme-vt')));
      }
      assert.equal(await page.locator('body').getAttribute('data-theme'), theme);
      await page.evaluate(language => i18n.setLanguage(language, false), language); await openProperties();
      assert.equal(await page.locator('#frontmatter-modal .modal-dialog').evaluate(element => element.scrollWidth > element.clientWidth + 2), false);
      assert.equal(await page.locator('#fm-modal-insert').evaluate(element => {const box = element.getBoundingClientRect(); return box.top >= 0 && box.bottom <= innerHeight;}), true);
      await page.screenshot({path: path.join(h.data, `metadata-${width}-${language}-${theme}.png`)});
      await page.locator('#fm-modal-cancel').click();
    }
    assert.equal(await page.evaluate(() => getEditContent()), nativeText(patched)); h.unchanged(expectedInputs);
  });
});
