'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const { openOwned, until } = require('../tools/test/owned-browser-host.cjs');
const { startKernel } = require('../tools/test/v006-refactor-host.cjs');

const [binary, output] = process.argv.slice(2);
assert(path.isAbsolute(binary) && path.isAbsolute(output));
const header = '| 项目 | 备注 |\n| :--- | ---: |';
const upper = '| Z😀 | **保留** |', lower = '| A \\| B | [站点](https://example.invalid) |';
const table = `${header}\n${upper}\n${lower}`;
const source = `# 双表\n\n${table}\n\n${table}\n\n外部原文\n`;
const tail = '待审正文';
const pending = source.replace('外部原文', tail);
const moved = `# 双表\n\n${table}\n\n${header}\n${lower}\n${upper}\n\n${tail}\n`;
const physical = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const report = { passed: false, nativeUiCertified: false, checks: [] };

async function exerciseEditor() {
  let fixture;
  const owned = await openOwned(output, (_, dirs) => {
    fixture = path.join(dirs.documents, '双表 😀.md');
    fs.writeFileSync(fixture, physical(source), { flag: 'wx' });
    fs.writeFileSync(path.join(dirs.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', autoCheckUpdate: false }), { flag: 'wx' });
  }, { binary });
  const { page } = owned;
  const review = () => page.evaluate(() => ReadMDReview.snapshot(getActiveTab()));
  const proposal = (from, to, insert) => page.evaluate(async edit => {
    const tab = getActiveTab();
    return ReadMDReview.propose([edit], await ReadMDReview.capture(tab), tab);
  }, { from, to, insert });
  const visible = text => page.evaluate(expected => cmView?.state.doc.toString() === expected, text);
  async function openTable(position) {
    await page.evaluate(anchor => cmView.dispatch({ selection: { anchor } }), position);
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('#btn-insert-table').click();
    await page.locator('#table-cell-controls').waitFor();
  }
  try {
    await page.evaluate(file => loadFile(file), fixture);
    await page.locator('#btn-edit').click();
    await until(() => visible(source), 'editor opened original');
    const top = source.lastIndexOf(upper), bottom = source.lastIndexOf(lower);
    await openTable(source.lastIndexOf(header) + 2);
    assert(await page.locator('[data-table-move="up"]').isDisabled());
    assert(await page.locator('[data-table-move="down"]').isDisabled());
    await page.locator('#table-cell-cancel').click();

    const tailAt = source.indexOf('外部原文');
    const outside = (await proposal(tailAt, tailAt + '外部原文'.length, tail)).changes[0];
    const candidate = await proposal(top + 2, top + 3, 'Y');
    const inside = candidate.changes.find(change => change.id !== outside.id);
    await openTable(bottom + 2);
    const moveUp = page.locator('[data-table-move="up"]');
    assert.equal(await moveUp.getAttribute('aria-label'), '上移');
    await moveUp.click();
    const overlapMessage = await page.evaluate(() => i18n.t('review.conflict'));
    await until(() => page.locator('#table-cell-controls .hint').textContent().then(text => text === overlapMessage), 'overlap refusal is visible');
    assert(await visible(candidate.content));
    assert.equal((await review()).review_revision, candidate.review_revision);
    assert.deepEqual(fs.readFileSync(fixture), physical(source));
    report.checks.push('Header stays fixed and row move refuses a pending review in the affected range');
    await page.locator('#table-cell-cancel').click();
    await page.evaluate(id => ReadMDReview.resolve([id], 'withdraw'), inside.id);
    assert(await visible(pending));

    await openTable(bottom + 2);
    await moveUp.click();
    await until(() => visible(moved), 'one row move changes only the second table');
    await page.locator('#table-modal').waitFor({ state: 'hidden' });
    const staged = await review();
    const retained = staged.changes.find(change => change.id === outside.id);
    for (const key of ['from', 'to', 'original', 'replacement', 'decision', 'application']) assert.equal(retained[key], outside[key]);
    assert.deepEqual(fs.readFileSync(fixture), physical(source));
    await page.locator('#edit-save').click();
    await until(() => fs.readFileSync(fixture).equals(physical(moved)), 'save row move with BOM and CRLF');
    await until(() => page.evaluate(() => !state.editing), 'save exits editor');
    report.checks.push('Move preserves exact markup, the duplicate table and the outside pending review');
    assert.deepEqual(owned.errors, []);
    return { profile: owned.profile, documents: owned.documents, fixture, retained, oldPid: owned.host.pid };
  } finally { await owned.close(); }
}

async function reopen(result) {
  const temp = fs.mkdtempSync(path.join(output, 'reopen-tmp-'));
  Object.assign(process.env, { TMPDIR: temp, TMP: temp, TEMP: temp });
  try {
    const server = await startKernel(binary, result.profile, result.documents);
    try {
      assert.notEqual(server.pid, result.oldPid);
      const request = async (route, data) => {
        const response = await server.api(route, data ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) } : {});
        assert.equal(response.status, 200);
        return response.json();
      };
      await request('/api/file?p=' + encodeURIComponent(result.fixture));
      const reopened = await request('/api/review', { action: 'resume', path: result.fixture });
      assert.equal(reopened.found, true);
      assert.equal(reopened.snapshot.content, moved);
      const history = (action, head) => request('/api/review', { action, document_id: head.document_id,
        generation: head.generation, review_revision: head.review_revision, call_id: randomUUID() });
      const undone = await history('undo', reopened.snapshot);
      assert.equal(undone.content, pending);
      const redone = await history('redo', undone);
      assert.equal(redone.content, moved);
      assert.deepEqual(redone.changes.find(change => change.id === result.retained.id), result.retained);
      assert.deepEqual(fs.readFileSync(result.fixture), physical(moved));
      report.checks.push('Fresh process restores the table and one undo/redo leaves the saved bytes and review intact');
    } finally { await server.stop(); }
  } finally { fs.rmdirSync(temp); }
}

exerciseEditor().then(reopen).then(() => { report.passed = true; }).catch(error => {
  report.failure = error.message.replaceAll(output, '<owned>');
  process.exitCode = 1;
}).finally(() => {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify(report));
});
