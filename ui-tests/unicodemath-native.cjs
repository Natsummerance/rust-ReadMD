'use strict';
// Real Windows WebView2 + Rust APIs, production CSP, isolated synthetic files.
// Clipboard events supply only the test formula; never read the user's clipboard.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const {chromium} = require('playwright'), {startNative} = require('./native-session.cjs'), zipParts = require('./zip-parts.cjs');
const root = path.resolve(__dirname, '..');
const data = fs.mkdtempSync(path.join(root, '.cache/capability-upgrade/unicodemath-native-'));
const docs = path.join(data, 'documents'); fs.mkdirSync(docs);
const file = path.join(docs, 'Evidence-and-uncertainty.md');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report = {platform:'windows-x64', nativeCsp:true, synthetic:true, passed:false, checks:[], wordApplicationRoundTrip:'unverified'};
const source = '# 证据与不确定性\n\n## 研究札记 · 数学表达的来源与修订\n\n> [!NOTE]\n> 保留观察、计算与判断之间的区别。修订公式不能改变未选中的相同内容。\n\n| 样本 | 数量 | 平均得分 | 标准差 |\n|---|---:|---:|---:|\n| 实验组 | 48 | 0.84 | 0.11 |\n| 对照组 | 52 | 0.76 | 0.14 |\n\n第一次估计 $x^2/y$。第二次校订 $x^2/y$。\n\n## 矩阵与误差传播\n\n作者将 Office 线性公式粘贴为可读结构，确认后再保存；原始观察不能随预览改变。\n\n- [x] 明确计算来源\n- [ ] 检查导出后公式是否仍为可编辑结构\n';
let native, phase = 'startup';
async function until(fn) {
  const end = Date.now() + 45000;
  while (!await fn()) { if (Date.now() > end) throw Error('Timed out: ' + phase); await new Promise(r => setTimeout(r, 60)); }
}
(async () => {
  try {
    fs.writeFileSync(file, source); process.env.READMD_WORKSPACE = docs;
    report.binarySha256 = hash(process.env.READMD_BIN);
    report.bootSha256 = hash(path.join(root, 'assets/readmd.boot.js'));
    const originalPath = process.env.PATH;
    try {
      process.env.PATH = path.join(process.env.WINDIR, 'System32');
      native = await startNative(root, data, chromium, 28934);
    } finally { process.env.PATH = originalPath; }
    const page = native.page, errors = []; page.on('pageerror', e => errors.push(e.message));
    report.servedBootSha256 = crypto.createHash('sha256').update(await (await page.request.get(new URL('/assets/readmd.boot.js', page.url()).href)).body()).digest('hex');
    assert.equal(report.servedBootSha256, report.bootSha256);
    await page.evaluate(() => Promise.all([window.__readmdSessionReady, window.__readmdPreferencesReady]));
    const api = (route, body) => page.evaluate(async ({route, body}) => {
      const response = await apiFetch(route, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body)});
      return {status:response.status, data:await response.json()};
    }, {route, body});
    report.importProbes = [];
    for (const source of ['\\matrix(a&b@c)', '\\sqrt(5&a^2)', '(abc)\\vec']) {
      const result = await api('/api/math/serialize', {source, dialect:'unicodemath', target:'dsl', display:true});
      report.importProbes.push({source, status:result.status, output:result.data.output, error:result.data.error_code});
    }
    await page.evaluate(async file => { await i18n.setLanguage('zh-CN', false); await loadFile(file); await toggleEdit(); setEditorPref('aiCompletion', false); setPvLayout('right'); }, file);
    phase = 'original UTF-16 structure and revision';
    const unicode = '𝛼 + x^2/y + x^2/y';
    const parsed = await api('/api/math/parse', {source:unicode, dialect:'unicodemath'});
    assert.equal(parsed.status, 200); assert.deepEqual(parsed.data.document.diagnostics, []);
    const fractions = parsed.data.document.nodes.filter(n => n.kind === 'f'); assert.equal(fractions.length, 2); assert.equal(fractions[0].from, 5);
    const body = {source:unicode, dialect:'unicodemath', expected_revision:parsed.data.document.revision,
      patch:{operation:'replace_slot', node_id:fractions[1].id, role:'den', text:'(z+1)'}};
    const patched = await api('/api/math/patch', body); assert.equal(patched.status, 200);
    assert.equal(patched.data.document.source, '𝛼 + x^2/y + x^2/(z+1)');
    assert.equal((await api('/api/math/patch', {...body, source:patched.data.document.source})).status, 409);
    const advanced = await api('/api/math/patch', {...body, patch:{operation:'set_property', node_id:fractions[1].id, container:'fPr', property:'type', value:'skw'}});
    assert.equal(advanced.status, 200); assert.equal(advanced.data.document.dialect, 'dsl'); assert.ok(!advanced.data.document.source.includes('<m:'));
    report.checks.push('real Rust API preserves scripted numerator, supplementary UTF-16 positions, repeated identity, revision rejection and readable advanced edits');
    async function paste(text) {
      const receipt = await page.locator('#formula-source').evaluate((input, text) => {
        input.focus(); input.setSelectionRange(0, input.value.length);
        const clipboardData = new DataTransfer(); clipboardData.setData('text/plain', text);
        const accepted = !input.dispatchEvent(new ClipboardEvent('paste', {clipboardData, bubbles:true, cancelable:true}));
        return {text, accepted, sourceAfter:input.value, editing:state.editing};
      }, text);
      (report.pasteReceipts ||= []).push(receipt); assert.equal(receipt.accepted, true, 'complete external formula paste must be handled');
    }
    phase = 'import selected repeated Office formula';
    await page.evaluate(async () => { const range = ReadMDFormula.ranges(getEditContent())[1]; await ReadMDFormula.open('inline', range); });
    await paste('√(x^2+1)/y');
    await until(() => page.locator('#formula-apply').isEnabled());
    const imported = await page.locator('#formula-source').inputValue(); assert.ok(imported.includes('frac(')); assert.ok(imported.includes('root(') || imported.includes('sqrt('));
    assert.equal(await page.evaluate(() => getEditContent()), source); assert.equal(fs.readFileSync(file, 'utf8'), source);
    await page.screenshot({path:path.join(data, 'office-import-draft.png')});
    await page.locator('#formula-apply').click();
    let current = await page.evaluate(() => getEditContent());
    assert.ok(current.includes('第一次估计 $x^2/y$。第二次校订 :math[')); assert.equal((current.match(/\$x\^2\/y\$/g) || []).length, 1);
    assert.ok(!current.includes('readmd:math:v1')); assert.ok(!current.includes('<m:'));
    await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), source);
    await page.keyboard.press('Control+y'); assert.equal(await page.evaluate(() => getEditContent()), current);
    report.checks.push('native paste into second identical legacy formula converts only that occurrence, keeps readable DSL and one body undo/redo');
    phase = 'edit imported denominator in rendered preview';
    await until(() => page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n => n === 1));
    await page.locator('#preview-pane .readmd-dsl-math mjx-container').click();
    await until(() => page.locator('.formula-inline-controls button').first().isEnabled());
    await page.locator('.formula-inline').getByRole('button', {name:'分母', exact:true}).click();
    await page.locator('.formula-inline-input').fill('θ + 1'); await page.locator('.formula-inline-input').press('Enter');
    await until(() => page.locator('.formula-inline-controls button').first().isEnabled());
    await page.locator('.formula-inline-controls button').first().click();
    current = await page.evaluate(() => getEditContent()); assert.ok(current.includes('θ + 1')); assert.ok(current.includes('第一次估计 $x^2/y$'));
    report.checks.push('rendered imported formula reuses exact slot editing; preview draft does not mutate its identical sibling');
    async function openAtEnd(mode = 'block') {
      await page.evaluate(async mode => { const end = cmView.state.doc.length; cmView.dispatch({selection:{anchor:end}}); await ReadMDFormula.open(mode); }, mode);
    }
    phase = 'matrix import with ragged rows'; await openAtEnd(); await paste('\\matrix(a&b@c)');
    await until(() => page.locator('#formula-apply').isEnabled()); assert.ok((await page.locator('#formula-source').inputValue()).includes('matrix('));
    assert.equal(await page.locator('#formula-preview [data-mml-node="merror"]').count(), 0); await page.locator('#formula-apply').click();
    phase = 'MathML import'; await openAtEnd('inline');
    await paste('<math xmlns="http://www.w3.org/1998/Math/MathML"><mfrac><msup><mi>σ</mi><mn>2</mn></msup><mi>n</mi></mfrac></math>');
    await until(() => page.locator('#formula-apply').isEnabled()); assert.ok((await page.locator('#formula-source').inputValue()).includes('superscript(')); await page.locator('#formula-apply').click();
    current = await page.evaluate(() => getEditContent());
    report.checks.push('same native source field imports Office ragged matrices and MathML scripts into shared, editable DSL without adding permanent UI');
    phase = 'invalid and closed import isolation'; await openAtEnd();
    const invalid = '\\matrix(a&\\notReadMD@c&d)'; await paste(invalid);
    await until(() => page.locator('#formula-error').innerText().then(Boolean));
    assert.equal(await page.locator('#formula-source').inputValue(), invalid); assert.equal(await page.locator('#formula-apply').isDisabled(), true);
    await page.evaluate(() => ReadMDFormula.close()); assert.equal(await page.evaluate(() => getEditContent()), current);
    await openAtEnd();
    await page.evaluate(() => {
      const input = document.getElementById('formula-source'), clipboardData = new DataTransfer();
      clipboardData.setData('text/plain', '■(a&b@c&d)');
      input.dispatchEvent(new ClipboardEvent('paste', {clipboardData, bubbles:true, cancelable:true}));
      ReadMDFormula.close();
    });
    await openAtEnd(); assert.equal(await page.locator('#formula-source').inputValue(), '');
    await page.evaluate(() => ReadMDFormula.close()); assert.equal(await page.evaluate(() => getEditContent()), current);
    report.checks.push('unknown macros retain pasted source and disable Apply; delayed results cannot replace a closed or subsequent draft');
    phase = 'atomic save and reopen'; assert.equal(await page.evaluate(() => saveEdit()), true);
    assert.equal(fs.readFileSync(file, 'utf8'), current); assert.deepEqual(fs.readdirSync(docs), [path.basename(file)]);
    await page.evaluate(async file => { await exitEdit(); await loadFile(file, {force:true, discardConfirmed:true}); }, file);
    await until(() => page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n => n === 3));
    assert.equal(await page.evaluate(() => hasUnsavedEditorChanges()), false); await page.screenshot({path:path.join(data, 'saved-reading.png')});
    report.checks.push('native atomic save/reopen preserves all imported structures, no hidden XML or companion files, and no false unsaved state');
    phase = 'actual exports'; report.exports = {};
    assert.ok(process.env.READMD_FONTS_PACK, 'READMD_FONTS_PACK must select the audited offline Chinese font package');
    const fonts = await api('/api/capabilities/packs', {action:'install', id:'fonts', path:process.env.READMD_FONTS_PACK});
    assert.equal(fonts.status, 200); assert.equal(fonts.data.installed, true);
    report.fontPackSha256 = hash(process.env.READMD_FONTS_PACK);
    for (const format of ['docx', 'pdf', 'epub']) {
      const out = path.join(docs, 'Evidence-and-uncertainty.' + format);
      const result = await api('/api/export', {format, content:current, baseDir:docs, options:{pdf:{engine:'rust'}}, out_path:out});
      assert.equal(result.status, 200); assert.equal(result.data.ok, true); assert.ok(fs.statSync(out).size > 1000);
      report.exports[format] = {path:path.relative(data, out), bytes:fs.statSync(out).size, sha256:hash(out)};
    }
    const word = zipParts(fs.readFileSync(path.join(docs, 'Evidence-and-uncertainty.docx'))).get('word/document.xml');
    for (const tag of ['f', 'sSup', 'rad', 'm']) assert.ok(word.includes('<m:' + tag + '>'), tag);
    const epub = [...zipParts(fs.readFileSync(path.join(docs, 'Evidence-and-uncertainty.epub')))].filter(([name]) => name.endsWith('.xhtml')).map(([,text]) => text).join('');
    assert.ok(epub.includes('<math')); assert.ok(epub.includes('<mtable')); assert.ok(epub.includes('<mfrac'));
    assert.equal(fs.readFileSync(file, 'utf8'), current); assert.deepEqual(errors, []);
    report.checks.push('actual DOCX exports editable Office fraction/root/script/matrix objects; EPUB exports MathML; PDF is independently inspected');
    report.passed = true;
  } catch (error) {
    report.failedStage = phase; report.error = error.name; report.message = String(error.message).replaceAll(root, '<workspace>').replaceAll(data, '<test-profile>').slice(0, 700);
    if (native) {
      report.formulaDraft = await native.page.evaluate(() => ({source:document.getElementById('formula-source')?.value,
        error:document.getElementById('formula-error')?.textContent, preview:document.getElementById('formula-preview')?.textContent,
        disabled:document.getElementById('formula-apply')?.disabled})).catch(() => null);
      await native.page.screenshot({path:path.join(data, 'failure.png')}).catch(() => {});
    }
  } finally {
    if (native) { await native.browser.close().catch(() => {}); native.server.child.kill(); }
    report.recordedAt = new Date().toISOString(); fs.writeFileSync(path.join(data, 'report.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify({passed:report.passed, checks:report.checks.length, report:path.relative(root, path.join(data, 'report.json')), failedStage:report.failedStage}));
    if (!report.passed) process.exitCode = 1;
  }
})();
