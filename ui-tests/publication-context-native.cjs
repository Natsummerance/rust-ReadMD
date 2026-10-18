'use strict';
// Final candidate only. Real WebView2/CSP, shared processor, actual files.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {createHarness, fileHash, sha256} = require('../tools/test/publication-batch3-harness.cjs');
const {references, styles, richStyle, noteStyle, contextBody} = require('../tools/test/publication-batch3-fixtures.cjs');
const {checkSpans, inspectArtifact, checkRichArtifact} = require('../tools/test/publication-batch3-checks.cjs');
const {runCitationOracle} = require('../tools/test/publication-citation-oracle.cjs');
const h = createHarness('publication-context-native', 29125, __filename);
const source = '\uFEFF' + ('---\ntitle: Synthetic citation context\nbibliography: references.json\ncsl: apa\nlanguage: en-US\nunknown-extension: "Keep, exact: trivia" # preserved\n---\n\n' + contextBody).replaceAll('\n', '\r\n');
const richSource = '---\ntitle: Rich typed citation\nbibliography: references.json\ncsl: rich.csl\nlanguage: en-US\n---\n# Evidence\n\nRead [@study].\n';
const literalSource = '---\ntitle: Literal affix\nbibliography: references.json\ncsl: rich.csl\nlanguage: en-US\n---\n\n[&lt;script&gt;window.__qaInjected=1&lt;/script&gt; @study]\n';
const noteSource = '---\ntitle: Author note context\nbibliography: references.json\ncsl: notes.csl\nlanguage: en-US\n---\n# Evidence\n\nFirst [@study].\n\nA manual note[^owned].\n\nNext [@study, p. 9].\n\n:cite[author-only @study3]\n\nNarrative @study [p. 10] supports the conclusion.\n\n[^owned]: Citation inside the author note [@study, p. 7].\n';
const manyNotes = '---\nbibliography: references.json\ncsl: notes.csl\nlanguage: en-US\n---\nFirst [@study].\n\n' +
  Array.from({length: 100}, (_, index) => `Manual ${index + 1}[^manual-${index + 1}].\n\n`).join('') +
  'Repeat the first manual note[^manual-1].\n\nThen :cite[author-only @study3].\n\nDistant [@study].\n\n' +
  Array.from({length: 100}, (_, index) => `[^manual-${100 - index}]: Original manual note ${100 - index}.\n\n`).join('');
const nativeText = content => content.replace(/^\uFEFF/, '');
const file = path.join(h.docs, 'Context.md');
for (const [name, content] of Object.entries({'Context.md': source, 'Rich.md': richSource, 'Literal.md': literalSource, 'Author-notes.md': noteSource, 'Many-notes.md': manyNotes, 'references.json': JSON.stringify(references), 'rich.csl': richStyle, 'notes.csl': noteStyle})) fs.writeFileSync(path.join(h.docs, name), content);
const initial = h.snapshotInputs(); let page;
h.report.qaContractCorrection = {
  reason: 'The shell applies theme changes inside an asynchronous ViewTransition. Wait for the actual button choice change and transition completion after each real click.',
  originalFailure: '.cache/capability-upgrade/publication-context-native-7xhfNA/report.json',
  originalReportSha256: 'd5536ab6b6348405767817871e6c917d35ff4cd4f32870a3ab2c94e316eaaeb4',
  originalScriptSha256: '91fe889ca217a9759299f0aee44991f28cac3c60535190b16a7a809d18ee4e51',
  unchangedAssertions: 'Four-click bound, exact requested theme, source identity, citation selection and clipping checks are retained. Independent same-candidate oracle requests are not repeated.',
};
const doc = (content, options = {}) => h.api('/api/academic/document', {content, baseDir: h.docs, options, view: true});
const cites = (clusters, csl = richStyle, contexts) => h.api('/api/academic/citations', {format: 'csl-json', data: JSON.stringify(references), csl, style: 'apa', locale: 'en-US', clusters, ...(contexts ? {contexts} : {})});
const options = {pdf: {engine: 'rust'}, typography: {font: 'Arial', size: 11}, cover: {enabled: false}, toc: {enabled: false}, footer: {pageNumbers: false}};
function projection(data) { assert(data.projection && Array.isArray(data.projection.citations), 'Shared typed projection is missing'); return data.projection; }
function flattened(nodes) { return nodes.flatMap(node => node.children ? flattened(node.children) : [node]); }
h.finish(async () => {
  page = await h.start();
  await h.step('production CSP, independent source hashes and offline font/PDFium packs', async () => {
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
    assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
    h.report.inputs = initial; await h.installPacks();
  });
  await h.step('eleven exact source clusters, item/field byte and UTF-16 ranges, modes and revision', async () => {
    const result = await doc(source); assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true);
    checkSpans(source, result.data); assert.equal(result.data.source_citations.length, 11);
    assert.equal(result.data.revision || projection(result.data).revision, sha256(source));
    const all = projection(result.data).citations;
    const items = all.flatMap(citation => citation.source.items);
    assert(items.some(item => item.cite.mode === 'suppress-author' && item.cite.locator === '42'));
    assert(items.some(item => item.cite.mode === 'prose' && item.cite.locator === '43'));
    assert(items.some(item => item.cite.mode === 'author-only'));
    const first = all[0].source.items;
    assert.equal(first.length, 2); assert.equal(first[0].cite.key, 'study'); assert.equal(first[1].cite.key, 'study2');
    assert.equal(first[0].cite.locator, '33-35'); assert.equal(first[1].cite.kind, 'chapter');
    assert(flattened(first[0].cite.suffix_rich || []).some(node => node.text?.includes('passim') && node.format?.font_style === 'italic'), 'Formatted per-item suffix was lost');
    assert(items.some(item => item.cite.suffix?.includes('99 years later') && !item.cite.locator), 'Forced empty locator became a false page number');
    assert(all.some(citation => citation.source.context.in_note && citation.source.context.note_label === 'owned'), 'Author footnote identity was lost');
    h.unchanged(initial);
  });
  await h.step('five real CSL style overrides retain the original source/bibliography/style bytes', async () => {
    const signatures = [];
    for (const style of styles) {
      const result = await doc(source, {citations: {style}}); assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true);
      checkSpans(source, result.data); const typed = projection(result.data);
      signatures.push(typed.citations.map(item => flattened(item.rich).map(node => node.text || '').join('')).join('|'));
      assert.equal(typed.revision, sha256(source)); h.unchanged(initial);
    }
    assert(new Set(signatures).size >= 3, 'Style overrides did not select the requested processors’ style data');
    h.report.styleOverrides = styles;
  });
  await h.step('real BibLaTeX/CSL JSON books, Chinese chapters, organizations and valid unauthored legislation', async () => {
    const json = [references[0],
      {id: 'chapter-cn', type: 'chapter', title: '可复核研究章', 'container-title': 'Synthetic Collected Studies', author: [{family: '林', given: '墨'}], issued: {'date-parts': [[2026]]}, page: '12-24'},
      {id: 'web', type: 'webpage', title: 'Synthetic Organization Guidance', author: [{literal: 'Synthetic Institute'}], URL: 'https://example.invalid/guidance', issued: {'date-parts': [[2026]]}},
      {id: 'law', type: 'legislation', title: 'Synthetic Evidence Act', number: 'QA-42', issued: {'date-parts': [[2026]]}},
    ];
    const result = await h.api('/api/academic/citations', {format: 'csl-json', data: JSON.stringify(json), style: 'apa', locale: 'en-US', clusters: json.map(item => [{key: item.id}])});
    assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.citations.length, 4);
    assert(result.data.citations.every(item => item.text.length)); assert(result.data.citations[1].text.includes('林'));
    assert(result.data.citations[2].text.includes('Synthetic Institute'));
    assert(!result.data.diagnostics.some(item => item.key === 'law' && item.code === 'academic_missing_author'), 'Valid legislation was classified as corrupt for lacking an author');
    const bib = '@book{study,title={Synthetic Evidence},author={Example, Ada},date={2020},publisher={Example Press}}\n' +
      '@incollection{chapter-cn,title={可复核研究章},author={{林墨}},booktitle={Synthetic Collected Studies},date={2026},pages={12--24}}\n' +
      '@online{web,title={Synthetic Organization Guidance},author={{Synthetic Institute}},date={2026},url={https://example.invalid/guidance}}\n';
    const fromBib = await h.api('/api/academic/citations', {format: 'biblatex', data: bib, style: 'apa', locale: 'en-US', clusters: ['study', 'chapter-cn', 'web'].map(key => [{key}])});
    assert.equal(fromBib.status, 200, JSON.stringify(fromBib.data)); assert.equal(fromBib.data.citations.length, 3);
    assert(fromBib.data.citations[1].text.includes('林')); assert(fromBib.data.citations[2].text.includes('Synthetic Institute'));
    h.unchanged(initial);
  });
  await h.step('per-item affix collapse endpoints and middle-item preservation', async () => {
    const numeric = '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Original numeric QA</title><id>urn:readmd:qa:numeric</id><updated>2026-10-13T00:00:00Z</updated></info><citation collapse="citation-number"><layout prefix="[" suffix="]" delimiter=", "><text variable="citation-number"/></layout></citation></style>';
    const cluster = () => ['study', 'study2', 'study3'].map(key => ({key}));
    const middle = cluster(); middle[1].prefix = 'see ';
    const first = cluster(); first[0].prefix = 'see ';
    const last = cluster(); last[2].suffix = ' suffix';
    const result = await cites([cluster(), middle, first, last], numeric); assert.equal(result.status, 200, JSON.stringify(result.data));
    assert.deepEqual(result.data.citations.map(item => item.text), ['[1–3]', '[1, see 2, 3]', '[see 1–3]', '[1–3 suffix]']);
  });
  await h.step('real note-number gaps, author-only state isolation and missing-cluster isolation', async () => {
    const contexts = [{note_number: 1, in_note: false}, {note_number: 102, in_note: false}];
    const distant = await cites([[{key: 'study'}], [{key: 'study'}]], noteStyle, contexts); assert.equal(distant.status, 200, JSON.stringify(distant.data));
    assert.deepEqual(distant.data.citations.map(item => item.text), ['Example|FIRST:Synthetic Evidence', 'Example|DISTANT:Synthetic Evidence']);
    const author = await cites([[{key: 'study'}], [{key: 'study3', mode: 'author-only'}], [{key: 'study'}]], noteStyle,
      [{note_number: 1, in_note: false}, {note_number: null, in_note: false}, {note_number: 2, in_note: false}]);
    assert.equal(author.status, 200, JSON.stringify(author.data)); assert.equal(author.data.citations[2].text, 'Example|IBID:Synthetic Evidence');
    const missing = await cites([[{key: 'study'}], [{key: 'study'}, {key: 'missing'}], [{key: 'study'}]], noteStyle,
      [{note_number: 1, in_note: false}, {note_number: 2, in_note: false}, {note_number: 3, in_note: false}]);
    assert.equal(missing.status, 200, JSON.stringify(missing.data));
    assert(missing.data.diagnostics.some(item => item.key === 'missing'));
    assert.equal(missing.data.citations[2].text, 'Example|NEAR:Synthetic Evidence', 'The found subset of an incomplete cluster polluted later note state');
    const actualNotes = await doc(manyNotes); assert.equal(actualNotes.status, 200, JSON.stringify(actualNotes.data));
    const projected = projection(actualNotes.data).citations;
    const last = projected.at(-1); assert.equal(flattened(last.rich).map(node => node.text || '').join(''), 'Example|DISTANT:Synthetic Evidence');
    assert.equal(last.source.context.note_number, 102, 'Source note state used definition order, cluster count, or author-only as a note');
  });
  await h.step('conflicting explicit flags, aliases, unknown fields, locator and byte-budget errors', async () => {
    const cases = [
      [{mode: 'suppress-author', suppress_author: false}, 'suppress_author'],
      [{mode: 'author-only', 'author-only': false}, 'author_only'],
      [{suppress_author: true, 'suppress-author': false}, 'suppress-author'],
      [{author_only: true, suppress_author: true}, 'author_only'],
      [{kind: 'page', label: 'chapter'}, 'label'],
      [{prefixx: 'typo'}, 'prefixx'],
      [{kind: 'unknown-locator'}, 'kind'],
      [{prefix: '😀'.repeat(1025)}, 'prefix'],
    ];
    for (const [extra, field] of cases) {
      const result = await cites([[{key: 'study', ...extra}]]);
      assert.equal(result.status, 422, JSON.stringify(result.data)); assert.equal(result.data.error_code, 'academic_context');
      assert.equal(result.data.field, `clusters[0][0].${field}`);
    }
    const literal = await cites([[{key: 'study', prefix: '<script>window.__qaInjected=1</script> '}]]);
    assert.equal(literal.status, 200); assert(literal.data.citations[0].html.includes('&lt;script&gt;'));
    assert(!literal.data.citations[0].html.includes('<script>'));
    const unsafe = await cites([[{key: 'study', prefix_rich: [{type: 'link', href: 'javascript:window.__qaInjected=1', children: [{type: 'text', text: 'unsafe'}]}]}]]);
    assert.equal(unsafe.status, 422); assert.equal(await page.evaluate(() => window.__qaInjected), undefined);
    h.unchanged(initial);
  });
  await h.step('reader and keyboard preview citation activation selects the exact repeated source occurrence', async () => {
    const raw = nativeText(source), repeats = [...raw.matchAll(/\[@study\]/g)].map(match => match.index);
    assert.equal(repeats.length, 4, 'Original synthetic source changed its three prose occurrences and one table occurrence');
    const selector = from => `.readmd-citation[data-readmd-source-from="${from}"]`;
    const expected = from => ({from: raw.slice(0, from).replace(/\r\n?/g, '\n').length, to: raw.slice(0, from + 8).replace(/\r\n?/g, '\n').length});
    const selected = () => page.evaluate(() => ({from: cmView.state.selection.main.from, to: cmView.state.selection.main.to}));
    await page.evaluate(file => loadFile(file), file);
    await h.until(() => page.locator('#content .readmd-citation[role="button"][tabindex="0"]').count().then(count => count === 11));
    const target = page.locator('#content ' + selector(repeats[1]));
    assert.equal(await target.getAttribute('data-readmd-source-to'), String(repeats[1] + 8));
    assert.equal(await target.getAttribute('data-readmd-source-revision'), sha256(raw));
    await target.click();
    await h.until(() => page.evaluate(() => state.editing && Boolean(cmView)));
    await h.until(async () => JSON.stringify(await selected()) === JSON.stringify(expected(repeats[1])));
    assert.equal(await page.evaluate(() => getEditContent()), raw);
    await page.evaluate(() => {setEditorPref('aiCompletion', false); setPvLayout('right');});
    const preview = page.locator('#preview-pane ' + selector(repeats[2]));
    await h.until(() => preview.getAttribute('role').then(role => role === 'button'));
    await preview.focus(); await preview.press('Enter');
    assert.deepEqual(await selected(), expected(repeats[2]));
    assert.equal(await page.evaluate(() => cmView.hasFocus), true);
    const stale = await page.evaluate(async selector => {
      const node = document.querySelector('#preview-pane ' + selector);
      cmView.dispatch({changes: {from: 0, insert: 'x'}, userEvent: 'input'});
      const result = await ReadMDPublication.locateCitation(node); cmUndo(); return result;
    }, selector(repeats[2]));
    assert.equal(stale, false); assert.equal(await page.evaluate(() => getEditContent()), raw);
    await page.evaluate(() => exitEdit()); h.unchanged(initial);
  });
  await h.step('native reader/split preview typed styles and unchanged editor source', async () => {
    await page.evaluate(file => loadFile(file), path.join(h.docs, 'Literal.md'));
    await h.until(() => page.locator('#content').innerText().then(text => text.includes('<script>window.__qaInjected=1</script>')));
    assert.equal(await page.locator('#content script, #content [onerror], #content [onclick]').count(), 0);
    assert.equal(await page.evaluate(() => window.__qaInjected), undefined);
    assert.equal(await page.evaluate(() => state.original), literalSource);
    await page.evaluate(async file => {await i18n.setLanguage('en', false); await loadFile(file);}, path.join(h.docs, 'Rich.md'));
    await h.until(() => page.locator('#content').innerText().then(text => text.includes('UNDERLINE')));
    const computed = async selector => page.locator(selector).evaluate(element => {
      const nodes = [...element.querySelectorAll('*')];
      return {smallCaps: nodes.some(node => getComputedStyle(node).fontVariant.includes('small-caps')),
        underline: nodes.some(node => node.textContent === 'UNDERLINE' && getComputedStyle(node).textDecorationLine.includes('underline')),
        sup: nodes.some(node => node.textContent === 'SUP' && getComputedStyle(node).verticalAlign === 'super'),
        sub: nodes.some(node => node.textContent === 'SUB' && getComputedStyle(node).verticalAlign === 'sub'),
        link: nodes.some(node => node.tagName === 'A' && node.getAttribute('href') === 'https://example.invalid/study')};
    });
    assert.deepEqual(await computed('#content'), {smallCaps: true, underline: true, sup: true, sub: true, link: true});
    await page.evaluate(async () => {await toggleEdit(); setEditorPref('aiCompletion', false); setPvLayout('right');});
    await h.until(() => page.locator('#preview-pane').innerText().then(text => text.includes('UNDERLINE')));
    assert.deepEqual(await computed('#preview-pane'), {smallCaps: true, underline: true, sup: true, sub: true, link: true});
    assert.equal(await page.evaluate(() => getEditContent()), richSource); h.unchanged(initial);
  });
  await h.step('light/dark citation preview at both reference window sizes has no horizontal clipping', async () => {
    h.report.themeTransitions = [];
    for (const [width, height, theme] of [[1024, 680, 'light'], [1160, 820, 'dark']]) {
      await page.setViewportSize({width, height});
      await h.until(() => page.evaluate(() => !document.documentElement.classList.contains('rm-theme-vt')));
      for (let count = 0; count < 4 && await page.locator('body').getAttribute('data-theme') !== theme; count++) {
        const beforeChoice = await page.locator('#btn-theme').getAttribute('data-theme-icon');
        assert(['auto', 'light', 'dark', 'sepia'].includes(beforeChoice));
        await page.locator('#btn-theme').click();
        await h.until(() => page.evaluate(before => document.getElementById('btn-theme').dataset.themeIcon !== before &&
          !document.documentElement.classList.contains('rm-theme-vt'), beforeChoice));
        const afterChoice = await page.locator('#btn-theme').getAttribute('data-theme-icon');
        assert.equal(afterChoice, ['auto', 'light', 'dark', 'sepia'][(['auto', 'light', 'dark', 'sepia'].indexOf(beforeChoice) + 1) % 4]);
        h.report.themeTransitions.push({width, height, target: theme, beforeChoice, afterChoice, actualTheme: await page.locator('body').getAttribute('data-theme')});
      }
      assert.equal(await page.locator('body').getAttribute('data-theme'), theme);
      assert.equal(await page.locator('#preview-pane').evaluate(element => element.scrollWidth > element.clientWidth + 2), false);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1), false);
      await page.screenshot({path: path.join(h.data, `citation-${width}-${theme}.png`)});
    }
    assert.equal(await page.evaluate(() => getEditContent()), richSource); h.unchanged(initial);
  });
  await h.step('actual rich DOCX/PDF/EPUB attributes, independent parsing and EPUBCheck', async () => {
    const produced = [];
    for (const format of ['docx', 'pdf', 'epub']) {
      const output = path.join(h.outputs, 'Rich.' + format);
      const result = await h.api('/api/export', {format, content: richSource, baseDir: h.docs, out_path: output, options});
      assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true);
      checkRichArtifact(inspectArtifact(h, output, format === 'pdf' ? {glyphs: true, render: path.join(h.data, 'rich-pdf-pages')} : {}));
      produced.push(output);
    }
    const noteWord = path.join(h.outputs, 'Author-notes.docx');
    const noteResult = await h.api('/api/export', {format: 'docx', content: noteSource, baseDir: h.docs, out_path: noteWord, options});
    assert.equal(noteResult.status, 200, JSON.stringify(noteResult.data)); const notes = inspectArtifact(h, noteWord);
    assert.equal(notes.nestedNoteReferences.length, 0); assert(notes.notes.some(note => note.text.includes('Citation inside the author note')));
    assert(notes.notes.filter(note => Number(note.id) > 0).length >= 4, 'Real note references, including narrative prose, were dropped');
    const narrative = notes.paragraphs.find(text => text.startsWith('Narrative '));
    assert(narrative?.includes('Example'), 'Note-style narrative prose lost the inline author');
    assert(!narrative.includes('10'), 'Note-style narrative locator leaked into the prose instead of its note');
    assert(notes.notes.some(note => note.text.includes('10')), 'Note-style narrative locator did not reach a real note');
    const jar = process.env.READMD_EPUBCHECK_JAR || path.join(h.root, '.cache/capability-upgrade/tools/epubcheck-5.4.0/epubcheck.jar');
    assert(fs.existsSync(jar), 'The existing locked offline EPUBCheck jar is required');
    const {checkEpub} = await import('../tools/check-epub.mjs'); const validation = await checkEpub(produced.filter(file => file.endsWith('.epub')), {jar, output: path.join(h.data, 'epubcheck.json')});
    assert.equal(validation.passed, true); h.report.outputs = produced.map(file => ({file: path.basename(file), sha256: fileHash(file)}));
    h.unchanged(initial);
  });
  await h.step('forged/replayed/duplicated source nonce markers cannot replace completed output', async () => {
    const output = path.join(h.outputs, 'Rich.docx'), digest = fileHash(output);
    const prepared = await doc(richSource); const nonce = projection(prepared.data).nonce; assert.match(nonce, /^[a-f0-9]{32}$/);
    for (const marker of ['<readmd-csl data-slot="00000000000000000000000000000000 c0"/>',
      `<readmd-csl data-slot="${nonce} c0"/>`, `<readmd-csl data-slot="${nonce} c0"/><readmd-csl data-slot="${nonce} c0"/>`]) {
      const result = await h.api('/api/export', {format: 'docx', content: richSource + '\n' + marker + '\n', baseDir: h.docs, out_path: output, options});
      assert.equal(result.data.ok, false, 'A forged source marker was treated as an authorized typed slot');
      assert.match(result.data.error_code || '', /^academic_(?:slot|source|projection)/); assert.equal(fileHash(output), digest);
    }
    h.unchanged(initial);
  });
  await h.step('native original citation edit, keyboard undo/redo, Save/reopen and real host restart', async () => {
    await page.evaluate(async file => {await exitEdit(); await loadFile(file); await toggleEdit();}, file);
    assert.equal(await page.evaluate(() => getEditContent()), nativeText(source));
    const edited = source.replace('[-@study, p. 42]', '[-@study, p. 44]');
    await page.evaluate(() => {const at = cmView.state.doc.toString().indexOf('[-@study, p. 42]') + '[-@study, p. '.length; cmView.dispatch({selection: {anchor: at, head: at + 2}}); cmView.focus();});
    await page.keyboard.type('44'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(edited));
    await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(source));
    await page.keyboard.press('Control+y'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(edited));
    assert.equal(fs.readFileSync(file, 'utf8'), source);
    await page.locator('#edit-save').click(); await h.until(() => Promise.resolve(fs.readFileSync(file, 'utf8') === edited));
    initial['Context.md'] = fileHash(file);
    await page.evaluate(() => ReadMDSession.flush()); await h.stop(); page = await h.start();
    await page.evaluate(file => loadFile(file), file); assert.equal(await page.evaluate(() => state.original), nativeText(edited));
    const result = await doc(edited); checkSpans(edited, result.data);
    assert(projection(result.data).citations.some(citation => citation.source.items.some(item => item.cite.mode === 'suppress-author' && item.cite.locator === '44')));
    h.unchanged(initial);
  });
  await h.step('fixed independent upstream RESULT oracle over this native candidate, once per fixture', async () => {
    h.report.oracle = await runCitationOracle(h.api, {summaryDirectory: h.data});
    assert.equal(h.report.oracle.mandatoryFixtures, 20);
    assert.equal(h.report.oracle.optionalCompatibility.length, 1);
    assert.equal(h.report.oracle.mandatoryFailures, 0, 'Mandatory independent oracle mismatches retained; no fixture was retried');
    h.report.oracleEvidence = {kind: 'actual current-candidate requests', requestsRepeated: false, originalFixtureHashesRevalidated: true};
  });
});
