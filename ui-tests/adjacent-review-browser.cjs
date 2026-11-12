'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { openOwned, until } = require('../tools/test/owned-browser-host.cjs');

const [binary, output] = process.argv.slice(2);
assert(path.isAbsolute(binary) && path.isAbsolute(output));
const source = '# 审阅\n\n😀|ab|中间 😀 内容|cd|尾部。\n';
const pending = '# 审阅\n\n😀|β😀|中间 😀 内容||尾部。\n';
const a = source.indexOf('ab'), c = source.indexOf('cd');
const propose = [
  { from: a, to: a + 1, insert: '' }, { from: a + 1, to: a + 2, insert: 'β😀' },
  { from: c, to: c + 1, insert: '' }, { from: c + 1, to: c + 2, insert: '' },
];
const restore = [{ from: a, to: a + 3, insert: 'ab' }, { from: c + 1, to: c + 1, insert: 'cd' }];
const physical = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const report = { passed: false, nativeUiCertified: false, checks: [], inferenceRequests: 0 };
const check = name => report.checks.push(name);

async function run() {
  let fixture;
  const host = await openOwned(output, (_, dirs) => {
    fixture = path.join(dirs.documents, '相邻审阅 😀.md');
    fs.writeFileSync(fixture, physical(source), { flag: 'wx' });
    fs.writeFileSync(path.join(dirs.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', autoCheckUpdate: false }), { flag: 'wx' });
  }, { binary });
  const { page } = host;
  const visible = text => page.evaluate(expected => state.editing && cmView?.state.doc.toString() === expected, text);
  const review = () => page.evaluate(() => ReadMDReview.snapshot(getActiveTab()));
  try {
    page.on('request', request => {
      if (request.method() === 'POST' && /^\/api\/(?:ai\/(?:chat|completion)|review\/ai)(?:\/|$)/.test(new URL(request.url()).pathname)) report.inferenceRequests++;
    });
    await page.evaluate(file => loadFile(file), fixture);
    await page.locator('#btn-edit').click();
    await until(() => visible(source), 'CodeMirror opened source');
    const proposed = await page.evaluate(async edits => {
      const tab = getActiveTab();
      return ReadMDReview.propose(edits, await ReadMDReview.capture(tab), tab);
    }, propose);
    assert.equal(proposed.content, pending);
    assert.equal(proposed.changes.length, 4);
    assert(await visible(pending));
    await page.keyboard.press('Control+s');
    await until(() => fs.readFileSync(fixture).equals(physical(pending)), 'explicit pending save');
    await until(async () => (await review())?.saved_revision === proposed.revision, 'pending save receipt');
    assert(await visible(pending));
    assert((await review()).changes.every(change => change.decision === 'pending'));
    check('Save retains all four pending items and BOM/CRLF');

    await page.evaluate(() => {
      window.__withdrawalEdits = [];
      addEventListener('readmd:document-transaction', event => window.__withdrawalEdits.push(event.detail.edits));
    });
    await page.locator('.review-more > summary').first().click();
    await page.locator('.review-more[open] [data-review-action="withdraw-all"]').click();
    await until(() => visible(source), 'withdraw all updates editor');
    const withdrawn = await review();
    assert(withdrawn.changes.every(change => change.decision === 'withdrawn' && change.current === change.original));
    assert.deepEqual(await page.evaluate(() => window.__withdrawalEdits), [restore]);
    assert.deepEqual(fs.readFileSync(fixture), physical(pending));
    check('One click withdraws both groups and projects exact disjoint coordinates');

    assert(await page.evaluate(() => ReadMDReview.history(false)));
    await until(() => visible(pending), 'one undo restores proposal');
    const undone = await review();
    const anchors = snapshot => Object.fromEntries(snapshot.changes.map(change => [change.id, [change.from, change.to, change.decision, change.application]]));
    assert.deepEqual(anchors(undone), anchors(proposed));
    assert(await page.evaluate(() => ReadMDReview.history(true)));
    await until(() => visible(source), 'one redo restores withdrawal');
    assert.deepEqual(anchors(await review()), anchors(withdrawn));
    await page.locator('#edit-save').click();
    await until(() => fs.readFileSync(fixture).equals(physical(source)), 'write original bytes');
    await until(() => page.evaluate(() => !state.editing), 'save button returns to reader');
    check('Undo, redo and Save preserve identities, bytes and editor exit');
    assert.equal(report.inferenceRequests, 0);
    assert.deepEqual(host.errors, []);
  } finally { await host.close(); }
  report.passed = true;
}

run().catch(error => {
  report.failure = error.message.replaceAll(output, '<owned>');
  process.exitCode = 1;
}).finally(() => {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify(report));
});
