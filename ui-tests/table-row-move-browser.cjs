'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const { openOwned, until } = require('../tools/test/owned-browser-host.cjs');
const { startKernel } = require('../tools/test/v006-refactor-host.cjs');

const [binary, output] = process.argv.slice(2);
assert(path.isAbsolute(binary) && path.isAbsolute(output));
const header = '  | 项目 | 数值 | 备注 |';
const delimiter = '  | :--- | ---: | :---: |';
const upper = '  | Z😀 | 10 | **保留** |';
const lower = '  |  A \\| B  |   2 | [站点](https://example.invalid) |';
const empty = '  | 空值 | | *保留* |';
const tied = '  | 同分 | 02   | `代码` |';
const discard = '  | 删除此行 | 7 | ~~删线~~ |';
const blank = '  | | | |';
const table = (...rows) => [header, delimiter, ...rows].join('\n');
const original = table(upper, lower, empty, tied, discard);
const tail = '待审正文';
const paired = (second, ending = tail) => `# 双表\n\n${original}\n\n${second}\n\n${ending}\n`;
const source = paired(original, '外部原文');
const pending = paired(original);
const moved = paired(table(lower, upper, empty, tied, discard));
const insertedRow = paired(table(blank, lower, upper, empty, tied, discard));
const deletedRow = paired(table(blank, lower, upper, empty, tied));
const sortedAscending = paired(table(lower, tied, upper, blank, empty));
const sortedDescending = paired(table(upper, lower, tied, blank, empty));
const insertedColumn = paired([
  '  | 项目 | 数值 | | 备注 |',
  '  | :--- | ---: | --- | :---: |',
  '  | Z😀 | 10 | | **保留** |',
  '  |  A \\| B  |   2 | | [站点](https://example.invalid) |',
  '  | 同分 | 02   | | `代码` |',
  '  | | | | |',
  '  | 空值 | | | *保留* |',
].join('\n'));
const deletedColumn = paired([
  '  | 项目 | | 备注 |',
  '  | :--- | --- | :---: |',
  '  | Z😀 | | **保留** |',
  '  |  A \\| B  | | [站点](https://example.invalid) |',
  '  | 同分 | | `代码` |',
  blank,
  empty,
].join('\n'));
const checkpoints = [pending, moved, insertedRow, deletedRow, sortedAscending, sortedDescending, insertedColumn, deletedColumn];
const columnActions = ['insert-column', 'delete-column', 'sort-asc', 'sort-desc'];
const physical = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const report = { passed: false, nativeUiCertified: false, checks: [] };

function assertOutsideReview(snapshot, expected) {
  const change = snapshot.changes.find(change => change.id === expected.id);
  assert(change, 'outside proposal still exists');
  for (const key of ['original', 'replacement', 'decision', 'application', 'origin', 'taken_over']) {
    assert.equal(change[key], expected[key]);
  }
  assert(change.version >= expected.version);
  assert.equal(change.from, snapshot.content.lastIndexOf(tail));
  assert.equal(change.to, change.from + tail.length);
  assert.equal(snapshot.content.slice(change.from, change.to), tail);
}

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
  const button = action => page.locator(`[data-table-move="${action}"]`);
  const cancel = page.locator('#table-cell-cancel');
  const input = page.locator('#table-cell-input');
  const hint = page.locator('#table-cell-controls .hint');
  const cell = (row, column) => page.locator(`[data-table-row="${row}"][data-table-column="${column}"]`);
  async function openDocument(file, text) {
    await page.evaluate(file => loadFile(file), file);
    await page.locator('#btn-edit').click();
    await until(() => visible(text), 'editor opened fixture');
  }
  async function openTable(position) {
    await page.evaluate(anchor => cmView.dispatch({ selection: { anchor } }), position);
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('#btn-insert-table').click();
    await page.locator('#table-cell-controls').waitFor();
    await until(() => page.locator('#table-cell-apply').isEnabled(), 'literal cell capture finished');
  }
  const openSecond = text => openTable(text.lastIndexOf(header) + header.indexOf('项目') + 1);
  async function selectCell(row, column) {
    const target = cell(row, column);
    await target.click();
    assert.equal(await target.getAttribute('aria-pressed'), 'true');
  }
  async function apply(action, before, after, label) {
    const previous = await review();
    assert.equal(previous.content, before, label);
    await button(action).click();
    await until(() => visible(after), label);
    await page.locator('#table-modal').waitFor({ state: 'hidden' });
    const next = await review();
    assert.equal(next.content, after, label);
    assert.equal(next.generation, previous.generation + 1, label);
    assert.equal(next.review_revision, previous.review_revision + 1, label);
    return next;
  }
  async function assertRefused(previous, label) {
    const message = await page.evaluate(() => i18n.t('review.conflict'));
    await until(async () => await hint.textContent() === message, label);
    assert(await visible(previous.content), label);
    const next = await review();
    for (const key of ['content', 'generation', 'review_revision', 'changes']) {
      assert.deepEqual(next[key], previous[key], label);
    }
  }

  try {
    const single = path.join(owned.documents, '单行.md');
    const oneRow = table(upper);
    const headerOnly = table();
    const withBlank = table(blank);
    const twoColumns = '  | 项目 | 数值 |\n  | :--- | ---: |\n  | | |';
    const oneColumn = '  | 项目 |\n  | :--- |\n  | |';
    fs.writeFileSync(single, physical(oneRow), { flag: 'wx' });
    await openDocument(single, oneRow);
    await openTable(oneRow.indexOf('Z😀') + 1);
    for (const width of [1024, 390]) {
      await page.setViewportSize({ width, height: 680 });
      await until(() => page.locator('.table-cell-editor-actions').evaluate(bar => {
        const box = bar.getBoundingClientRect();
        const apply = bar.querySelector('#table-cell-apply').getBoundingClientRect();
        const cancel = bar.querySelector('#table-cell-cancel').getBoundingClientRect();
        const tools = bar.querySelector('.table-structure-tools').getBoundingClientRect();
        return Math.abs(apply.top - cancel.top) < 1 && tools.bottom <= apply.top &&
          [...bar.querySelectorAll('button')].every(button => {
            const rect = button.getBoundingClientRect();
            return rect.width >= 44 && rect.height >= 44 && rect.left >= box.left - 1 &&
              rect.right <= box.right + 1 && rect.left >= 0 && rect.right <= innerWidth;
          });
      }), 'table actions fit with 44px targets at ' + width);
    }
    await page.setViewportSize({ width: 1160, height: 820 });
    await apply('delete', oneRow, headerOnly, 'last body row removed with delimiter intact');
    await openSecond(headerOnly);
    await apply('insert', headerOnly, withBlank, 'header-only table inserts a row with the original indentation');
    await openSecond(withBlank);
    await selectCell(0, 2);
    await apply('delete-column', withBlank, twoColumns, 'delete the selected last column');
    await openTable(twoColumns.indexOf('数值') + 1);
    await apply('delete-column', twoColumns, oneColumn, 'two-column table narrows to valid one-column syntax');
    await openTable(oneColumn.indexOf('项目') + 1);
    assert(await button('delete-column').isDisabled());
    assert.equal(await cell(0, 0).textContent(), '项目');
    assert(await visible(oneColumn));
    await cancel.click();
    await page.locator('#edit-save').click();
    await until(() => fs.readFileSync(single).equals(physical(oneColumn)), 'single table saved');
    await until(() => page.evaluate(() => !state.editing), 'single table save completed');
    report.checks.push('Delete the last body row, insert below an indented header, and block deletion of the final column');

    await openDocument(fixture, source);
    await openSecond(source);
    assert(await button('up').isDisabled());
    assert(await button('down').isDisabled());
    await cancel.click();
    const tailAt = source.indexOf('外部原文');
    const outside = (await proposal(tailAt, tailAt + '外部原文'.length, tail)).changes[0];
    const labelAt = source.lastIndexOf('A \\| B');
    const candidate = await proposal(labelAt, labelAt + 1, 'Y');
    const inside = candidate.changes.find(change => change.id !== outside.id);
    const movedCell = labelAt + 1;
    await openTable(movedCell);
    assert.equal(await button('up').getAttribute('aria-label'), '上移');
    await button('up').click();
    await assertRefused(candidate, 'overlap refuses a row move');
    await cancel.click();
    await openTable(candidate.content.lastIndexOf('数值') + 1);
    for (const action of columnActions) {
      await button(action).click();
      await assertRefused(candidate, `overlap refuses ${action}`);
    }
    assert.deepEqual(fs.readFileSync(fixture), physical(source));
    report.checks.push('The fixed header and review-overlap checks protect row moves, column edits and sorting');
    await cancel.click();
    await page.evaluate(id => ReadMDReview.resolve([id], 'withdraw'), inside.id);
    assert(await visible(pending));

    await openTable(movedCell);
    const staged = await apply('up', pending, moved, 'one row move changes only the second table');
    assertOutsideReview(staged, outside);
    await openSecond(moved);
    assert(await button('delete').isDisabled());
    await input.fill('尚未应用');
    for (const action of ['insert', ...columnActions]) assert(await button(action).isDisabled());
    await input.fill('项目');
    await apply('insert', moved, insertedRow, 'insert an indented blank row under the second header');
    await openTable(insertedRow.lastIndexOf('删除此行') + 1);
    await apply('delete', insertedRow, deletedRow, 'delete exactly the selected rich body row');
    await openTable(deletedRow.lastIndexOf('数值') + 1);
    await apply('sort-asc', deletedRow, sortedAscending, 'ascending numeric sort keeps ties stable and blanks last');
    await openTable(sortedAscending.lastIndexOf('数值') + 1);
    await apply('sort-desc', sortedAscending, sortedDescending, 'descending numeric sort keeps ties stable and blanks last');
    await openTable(sortedDescending.lastIndexOf('数值') + 1);
    await apply('insert-column', sortedDescending, insertedColumn, 'insert a blank, unaligned column immediately to the right');
    await openTable(insertedColumn.lastIndexOf('数值') + 1);
    assert.equal(await cell(0, 2).textContent(), '');
    assert.equal(await cell(0, 3).textContent(), '备注');
    const final = await apply('delete-column', insertedColumn, deletedColumn, 'remove the populated selected column only');
    assertOutsideReview(final, outside);
    assert.deepEqual(fs.readFileSync(fixture), physical(source));
    await page.locator('#edit-save').click();
    await until(() => fs.readFileSync(fixture).equals(physical(deletedColumn)), 'save structural edits with BOM and CRLF');
    await until(() => page.evaluate(() => !state.editing), 'save exits editor');
    report.checks.push('Numeric sorts keep ties stable and blanks last, then column edits preserve exact rich Markdown, the duplicate table and the outside review');

    const bareFile = path.join(owned.documents, '无外侧管道.md');
    const bare = '  项目 | 数值\n  :--- | ---:\n  第一 | 10\n  第二 | 2\n  第三 | 5';
    const framed = '  |项目 | 数值| |\n  |:--- | ---:| --- |\n  |第一 | 10| |\n  |第二 | 2| |\n  |第三 | 5| |';
    fs.writeFileSync(bareFile, physical(bare), { flag: 'wx' });
    await openDocument(bareFile, bare);
    await openTable(bare.indexOf('数值') + 1);
    await apply('insert-column', bare, framed, 'explicit outer pipes retain a blank final cell');
    await openTable(framed.indexOf('数值') + 1);
    assert.equal(await cell(0, 2).textContent(), '');
    assert.equal(await cell(3, 2).textContent(), '');
    assert.deepEqual(fs.readFileSync(bareFile), physical(bare));
    const beforeExternal = await review();
    const externalBytes = physical(bare + '\n\n外部写入\n');
    fs.writeFileSync(bareFile, externalBytes);
    for (const action of columnActions) {
      const [resumed] = await Promise.all([
        page.waitForResponse(response => new URL(response.url()).pathname === '/api/review' &&
          response.request().method() === 'POST' && response.request().postDataJSON()?.action === 'resume'),
        button(action).click(),
      ]);
      assert.equal(resumed.status(), 200);
      const { snapshot } = await resumed.json();
      assert.equal(snapshot.recovery.reason, 'external_change');
      assert.equal(snapshot.content, framed);
      await assertRefused(beforeExternal, `an external write refuses ${action}`);
      assert.deepEqual(fs.readFileSync(bareFile), externalBytes);
    }
    await cancel.click();
    report.checks.push('A bare table receives the outer pipes needed for a blank last column, and a real external file write blocks every column action');
    assert.deepEqual(owned.errors, []);
    return { profile: owned.profile, documents: owned.documents, fixture, outside, documentId: final.document_id, oldPid: owned.host.pid };
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
      let head = reopened.snapshot;
      assert.equal(head.document_id, result.documentId);
      assert.equal(head.content, deletedColumn);
      assertOutsideReview(head, result.outside);
      assert.deepEqual(fs.readFileSync(result.fixture), physical(deletedColumn));
      const history = (action, snapshot) => request('/api/review', {
        action, document_id: snapshot.document_id, generation: snapshot.generation,
        review_revision: snapshot.review_revision, call_id: randomUUID(),
      });
      for (const [action, states] of [
        ['undo', checkpoints.slice(0, -1).reverse()],
        ['redo', checkpoints.slice(1)],
      ]) {
        for (const [index, content] of states.entries()) {
          head = await history(action, head);
          assert.equal(head.content, content, `${action} checkpoint ${index + 1}`);
          assertOutsideReview(head, result.outside);
          assert.deepEqual(fs.readFileSync(result.fixture), physical(deletedColumn));
        }
      }
      report.checks.push('A fresh Rust process restores the saved table and all seven undo and redo checkpoints preserve the outside review and BOM/CRLF bytes');
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
