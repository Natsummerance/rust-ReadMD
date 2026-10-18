'use strict';
// Real WebView2/CSP, Rust PDF export and the distributed, managed PDFium pack.
// All documents are synthetic. No clipboard reads or user installation changes.
const fs = require('node:fs'), path = require('node:path');
const assert = require('node:assert/strict'), crypto = require('node:crypto');
const {chromium} = require('playwright'), {startNative} = require('./native-session.cjs');
const root = path.resolve(__dirname, '..');
const data = fs.mkdtempSync(path.join(root, '.cache/capability-upgrade/pdf-math-text-native-'));
const docs = path.join(data, 'documents'); fs.mkdirSync(docs);
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report = {platform:'windows-x64', nativeCsp:true, synthetic:true, passed:false, checks:[], exports:{}, timings:[]};
const fallback = [
  '# 证据与不确定性', '', '## 研究札记 · 计算与校订', '',
  '> [!NOTE]', '> 公式需要能够复制、搜索，并与正文保持同一阅读顺序。', '',
  '| 样本 | 数量 | 平均得分 | 标准差 |', '|---|---:|---:|---:|',
  '| 实验组 | 48 | 0.84 | 0.11 |', '| 对照组 | 52 | 0.76 | 0.14 |', '',
  '第一次估计 $x^2/y$。第二次校订 :math[frac(root(empty(), x^2 + 1), θ + 1)]。', '',
  '## 矩阵与误差传播', '', '作者需要确认导出后的公式仍然清晰且能检索。', '',
  '§§§readmd-math v1', 'matrix([a, b], [c, empty()])', '§§§', '',
  ':math[frac(σ^2, n)]', '',
].join('\n').replaceAll('§', String.fromCharCode(96));
const source = process.env.READMD_PDF_BASELINE_MD ? fs.readFileSync(process.env.READMD_PDF_BASELINE_MD, 'utf8') : fallback;
const file = path.join(docs, 'Evidence-and-uncertainty.md');
let native, phase = 'startup';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn) {
  const end = Date.now() + 45000;
  while (!await fn()) { if (Date.now() > end) throw Error('Timed out: ' + phase); await delay(60); }
}
(async () => {
  try {
    fs.writeFileSync(file, source); process.env.READMD_WORKSPACE = docs;
    // Exercise the installed capability path, not developer executable overrides.
    delete process.env.READMD_PDF_WORKER; delete process.env.READMD_PDFIUM_LIB;
    report.binarySha256 = hash(process.env.READMD_BIN);
    report.bootSha256 = hash(path.join(root, 'assets/readmd.boot.js'));
    report.sourceSha256 = hash(file);
    if (process.env.READMD_PDF_BASELINE) report.baselinePdfSha256 = hash(process.env.READMD_PDF_BASELINE);
    const originalPath = process.env.PATH;
    try {
      process.env.PATH = path.join(process.env.WINDIR, 'System32');
      native = await startNative(root, data, chromium, 28935);
    } finally { process.env.PATH = originalPath; }
    const page = native.page, errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.evaluate(() => Promise.all([window.__readmdSessionReady, window.__readmdPreferencesReady]));
    await page.evaluate(() => i18n.setLanguage('zh-CN', false));
    report.servedBootSha256 = crypto.createHash('sha256').update(await (await page.request.get(new URL('/assets/readmd.boot.js', page.url()).href)).body()).digest('hex');
    assert.equal(report.servedBootSha256, report.bootSha256);
    const api = (route, body) => page.evaluate(async ({route, body}) => {
      const response = await apiFetch(route, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body)});
      return {status:response.status, data:await response.json()};
    }, {route, body});
    const pdf = body => api('/api/pdf/workspace', body);
    phase = 'managed offline capability installation';
    assert.equal((await pdf({action:'probe'})).status, 503);
    for (const [id, env] of [['fonts','READMD_FONTS_PACK'], ['pdfium','READMD_PDFIUM_PACK']]) {
      assert.ok(process.env[env], env + ' is required');
      const installed = await api('/api/capabilities/packs', {action:'install', id, path:process.env[env]});
      assert.equal(installed.status, 200); assert.equal(installed.data.installed, true);
      report[id + 'PackSha256'] = hash(process.env[env]);
    }
    assert.equal((await pdf({action:'probe'})).status, 200);
    report.checks.push('missing engine refuses readiness; audited offline font/PDFium packs install and perform a real operation with System32-only child PATH');
    phase = 'real formula projections';
    await page.evaluate(file => loadFile(file), file);
    const ranges = await page.evaluate(source => ReadMDFormula.ranges(source), source);
    assert.equal(ranges.length, 4);
    report.expectedFormulas = [];
    for (const range of ranges) {
      if (!range.dialect || range.dialect === 'latex') report.expectedFormulas.push(range.tex.trim());
      else {
        const serialized = await api('/api/math/serialize', {source:range.tex, dialect:range.dialect, target:'latex', display:range.mode==='block'});
        assert.equal(serialized.status, 200); report.expectedFormulas.push(serialized.data.output);
      }
    }
    assert.ok(report.expectedFormulas.some(source => source.includes('θ')));
    async function exportPdf(name, content) {
      const output = path.join(docs, name + '.pdf'), start = performance.now();
      const exported = await api('/api/export', {format:'pdf', content, baseDir:docs, options:{pdf:{engine:'rust'}}, out_path:output});
      report.timings.push({operation:'export ' + name, milliseconds:performance.now()-start});
      assert.equal(exported.status, 200); assert.equal(exported.data.ok, true);
      report.exports[name] = {path:path.relative(data, output), bytes:fs.statSync(output).size, sha256:hash(output)};
      return output;
    }
    phase = 'vector PDF export and reopen';
    const output = await exportPdf('Evidence-and-uncertainty', source);
    assert.equal(hash(file), report.sourceSha256);
    let opened = await pdf({action:'open', path:output, width:1000});
    assert.equal(opened.status, 200); let session = opened.data;
    const math = session.objects.filter(item => item.font === 'ReadMDMathText');
    assert.equal(math.length, 4);
    for (const expected of report.expectedFormulas) assert.ok(math.some(item => item.text === expected), 'managed engine must recover exact formula projection');
    for (const item of math) {
      assert.equal(item.readonly, true);
      assert.ok(item.bounds.width > 2 && item.bounds.height > 2);
    }
    report.formulaObjects = math.map(({id,text,bounds,readonly}) => ({id,text,bounds,readonly}));
    report.checks.push('distributed PDFium worker reopens actual export and recovers all four formula sources with non-zero selectable geometry');
    phase = 'backend semantic layer protection';
    const target = math.find(item => item.text.includes('θ'));
    for (const kind of ['text','remove']) {
      const rejected = await pdf({action:'edit', session:session.session, revision:session.revision,
        operation:{kind, page:0, object:target.id, text:'replacement'}});
      assert.equal(rejected.status, 422); assert.equal(rejected.data.error_code, 'pdf_unsupported_object');
    }
    const stable = await pdf({action:'render', session:session.session, revision:session.revision, page:0, width:1000});
    assert.equal(stable.status, 200); assert.equal(stable.data.dirty, false);
    const noted = await pdf({action:'edit', session:session.session, revision:session.revision,
      operation:{kind:'note', page:0, x:30, y:30, text:'Temporary native inspection'}});
    assert.equal(noted.status, 200); session = noted.data;
    const protectedAfterEdit = session.objects.find(item => item.text === target.text);
    assert.equal(protectedAfterEdit.readonly, true);
    assert.equal((await pdf({action:'edit', session:session.session, revision:session.revision,
      operation:{kind:'remove', page:0, object:protectedAfterEdit.id}})).status, 422);
    const undone = await pdf({action:'undo', session:session.session, revision:session.revision, page:0, width:1000});
    assert.equal(undone.status, 200); session = undone.data;
    assert.equal(session.dirty, false); assert.equal(session.image, stable.data.image);
    assert.equal(hash(output), report.exports['Evidence-and-uncertainty'].sha256);
    await pdf({action:'close', session:session.session});
    report.checks.push('Rust rejects semantic layer editing/removal, also after a real annotation revision and undo; clean state and rendered pixels recover exactly');
    phase = 'real in-app PDF extraction';
    const namesBefore = fs.readdirSync(docs).sort();
    const converted = await page.evaluate(async output => {
      const response = await apiFetch('/api/convert?p=' + encodeURIComponent(output) + '&preview=1');
      return {status:response.status, data:await response.json()};
    }, output);
    assert.equal(converted.status, 200);
    assert.ok(converted.data.content.includes('θ') && converted.data.content.includes('σ'));
    report.conversionEngine = converted.data.engine;
    assert.deepEqual(fs.readdirSync(docs).sort(), namesBefore);
    report.checks.push('production PDF extraction recovers mathematical Unicode; preview does not create a Markdown copy or resources in the source directory');
    phase = 'native formula selection';
    assert.equal(await page.evaluate(output => ReadMDPdf.open(output), output), true);
    await page.getByRole('button', {name:target.text, exact:true}).click();
    const input = page.locator('#pdf-text');
    assert.equal(await input.inputValue(), target.text);
    assert.equal(await input.evaluate(el => el.readOnly), true);
    assert.equal(await page.locator('#pdf-apply').isDisabled(), true);
    assert.equal(await page.locator('#pdf-remove').isDisabled(), true);
    assert.equal(await page.locator('#pdf-font').isHidden(), true);
    assert.equal(await page.locator('#pdf-replace-image').isHidden(), true);
    assert.equal(await page.locator('#pdf-overflow').isHidden(), true);
    await input.focus(); await page.keyboard.press('Control+a');
    const selected = await input.evaluate(el => el.value.slice(el.selectionStart, el.selectionEnd));
    assert.equal(selected, target.text);
    await page.keyboard.type('must not edit'); assert.equal(await input.inputValue(), target.text);
    assert.equal(await page.evaluate(() => ReadMDPdf.dirty), false);
    const cdp = await page.context().newCDPSession(page);
    for (const [width,height] of [[1024,680],[1160,820]]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', {width,height,deviceScaleFactor:1,mobile:false});
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
      assert.ok(await page.locator('#pdf-viewport').evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'fitted PDF must not overflow horizontally');
      await page.getByRole('button', {name:target.text, exact:true}).scrollIntoViewIfNeeded();
      await page.screenshot({path:path.join(data,'formula-copy-' + width + '.png')});
    }
    await cdp.detach();
    await page.locator('#pdf-close').click();
    report.checks.push('native PDF hit target selects copyable source in existing context; editing/removal disabled and keyboard cannot mutate it, at both compact window sizes');
    phase = 'copy projection back into structural editor';
    await page.evaluate(async () => {
      await toggleEdit(); setEditorPref('aiCompletion', false);
      cmView.dispatch({selection:{anchor:cmView.state.doc.length}});
      await ReadMDFormula.open('inline');
    });
    async function paste(text) { return page.locator('#formula-source').evaluate((input, text) => {
      const clipboardData = new DataTransfer(); clipboardData.setData('text/plain', text);
      return !input.dispatchEvent(new ClipboardEvent('paste', {clipboardData,bubbles:true,cancelable:true}));
    }, text); }
    assert.equal(await paste(JSON.stringify(target.text)), false, 'a quoted DSL string is not a TeX formula');
    assert.equal(await page.locator('#formula-source').inputValue(), '');
    const invalid = '\\frac{\\notReadMD}{n}';
    assert.equal(await paste(invalid), true);
    await until(() => page.locator('#formula-error').innerText().then(Boolean));
    assert.equal(await page.locator('#formula-apply').isDisabled(), true);
    assert.equal(await page.locator('#formula-source').inputValue(), invalid);
    await page.evaluate(async () => { ReadMDFormula.close(); await ReadMDFormula.open('inline'); });
    const accepted = await paste(target.text);
    assert.equal(accepted, true);
    await until(() => page.locator('#formula-apply').isEnabled());
    const dsl = await page.locator('#formula-source').inputValue();
    assert.ok(dsl.includes('frac(') && dsl.includes('θ') && !dsl.includes('<m:'));
    await page.locator('#formula-source').evaluate(input => input.setSelectionRange(1,1));
    assert.equal(await paste(target.text), false, 'partial DSL insertion must retain the current language');
    assert.equal(await page.locator('#formula-source').inputValue(), dsl);
    assert.equal(await page.evaluate(() => getEditContent()), source);
    await page.locator('#formula-apply').click();
    assert.ok((await page.evaluate(() => getEditContent())).includes(':math['+dsl+']'));
    await page.keyboard.press('Control+z');
    assert.equal(await page.evaluate(() => getEditContent()), source);
    assert.equal(await page.evaluate(() => hasUnsavedEditorChanges()), false);
    await page.evaluate(() => exitEdit());
    assert.equal(hash(file), report.sourceSha256);
    report.checks.push('copied PDF LaTeX projection pastes into the native DSL editor, previews structurally, applies as one transaction and undoes without changing disk');
    phase = 'formula-only export';
    report.formulaOnlySource = '$$\\frac{σ^2}{n} + θ$$\n\n$$\\sqrt{x^2+1}$$\n\n$$𝛼 + β$$\n';
    await exportPdf('Formula-only', report.formulaOnlySource);
    report.checks.push('formula-only document exports without relying on prose font resources; independent inspection checks Unicode and visual equivalence');
    phase = 'thousand-formula export';
    report.stressFormulaCount = 1000;
    const stress = '# Formula production stress\n\n' + Array.from({length:1000}, (_,i) => 'Equation ' + String(i).padStart(4,'0') + ': $\\frac{x_{' + i + '}+σ}{n+1}$.\n').join('\n');
    await exportPdf('Thousand-formulas', stress);
    report.checks.push('actual export accepts 1000 uniquely indexed formulas; independent inspection checks every formula and all pages');
    assert.equal(hash(file), report.sourceSha256); assert.deepEqual(errors, []);
    report.passed = true;
  } catch (error) {
    report.failedStage = phase; report.error = error.name;
    report.message = String(error.message).replaceAll(root, '<workspace>').replaceAll(data, '<profile>').slice(0,800);
    if (native) await native.page.screenshot({path:path.join(data,'failure.png')}).catch(() => {});
    throw error;
  } finally {
    if (native) {
      await native.browser.close().catch(() => {}); native.server.child.kill();
      if (native.server.child.exitCode === null && native.server.child.signalCode === null)
        await new Promise(resolve => native.server.child.once('exit',resolve));
    }
    report.recordedAt = new Date().toISOString();
    fs.writeFileSync(path.join(data,'report.json'), JSON.stringify(report,null,2));
    console.log(JSON.stringify({passed:report.passed, checks:report.checks.length, report:path.relative(root,path.join(data,'report.json')), failedStage:report.failedStage}));
  }
})().catch(() => { process.exitCode = 1; });
