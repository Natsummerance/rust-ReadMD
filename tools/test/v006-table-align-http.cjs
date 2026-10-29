'use strict';
// Production alignment/save controllers in a Node VM, real CM GFM/history,
// real HTTP/disk and a different kernel PID for reopen. This is not a browser
// toolbar test; v006-table-align-browser.cjs covers that independent path.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { ROOT, host, tableCells } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const SOURCE = '# Synthetic alignment save\r\n\r\n| Kind | Note | Tail |\r\n| :---- | --- | ----: |\r\n| **Alpha** | left\\|right \\\\ end <br> next | 尾 |\r\n| Beta | [link](https://example.invalid/a) | 0002 |\r\n\r\nOutside evidence.\r\n';
async function run() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'An explicit prebuilt --bin is required');
  const documents = path.join(args.evidence, 'documents'); fs.mkdirSync(documents);
  const report = { schemaVersion: 1, syntheticOnly: true, guiStarted: false, compiled: false, userConfigurationRead: false, tokensLogged: false,
    transport: 'Production cmAlignTableColumn/saveEdit called in Node VM -> actual HTTP -> disk -> different PID readback',
    binarySha256: sha(fs.readFileSync(args.bin)), sourceSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/editor/editor.js'))), kernelPids: [], cases: [] };
  let kernel;
  try {
    kernel = await startKernel(args.bin, path.join(args.evidence, 'profile'), documents); report.kernelPids.push(kernel.pid);
    for (const alignment of ['left', 'center', 'right']) {
      const file = path.join(documents, alignment + '.md'); fs.writeFileSync(file, SOURCE, { flag: 'wx' });
      const before = await (await kernel.api('/api/file?p=' + encodeURIComponent(file))).json(); assert.equal(before.original ?? before.content, SOURCE);
      const f = host({ source: SOURCE, file, apiFetch: kernel.api }), CM = f.CM;
      f.view.state = CM.EditorState.create({ doc: SOURCE, selection: { anchor: SOURCE.replace(/\r\n/g, '\n').indexOf('left\\|right') + 1 }, extensions: [CM.history(), CM.markdown({ base: CM.markdownLanguage })] });
      Object.assign(f.tab, { revision: before.revision, mtime: before.mtime });
      const cells = tableCells(f.html()), delimiter = (alignment !== 'right' ? ':' : '') + '---' + (alignment !== 'left' ? ':' : '');
      const expected = SOURCE.replace('| :---- | --- | ----: |', '| :---- | ' + delimiter + ' | ----: |');
      assert.equal(f.context.cmAlignTableColumn(f.view, alignment), true); assert.equal(f.context.getEditContent(), expected); assert.deepEqual(tableCells(f.html()), cells);
      assert.equal(f.view.transactions.filter(transaction => transaction.docChanged).length, 1); assert.equal(fs.readFileSync(file, 'utf8'), SOURCE);
      f.undo(); assert.equal(f.context.getEditContent(), SOURCE); f.redo(); assert.equal(f.context.getEditContent(), expected);
      assert.equal(await f.save(), true); assert.equal(f.calls.length, 1); assert.equal(f.calls[0].route, '/api/save'); assert.equal(fs.readFileSync(file, 'utf8'), expected);
      f.undo(); assert.equal(f.context.getEditContent(), SOURCE); f.redo(); assert.equal(f.context.getEditContent(), expected);
      report.cases.push({ alignment, file, savedSha256: sha(expected), savedBytes: Buffer.byteLength(expected), explicitSaveOnly: true, oneUndoBeforeAndAfterSave: true, crlf: true });
    }
    await kernel.stop(); kernel = null; kernel = await startKernel(args.bin, path.join(args.evidence, 'profile-fresh'), documents); report.kernelPids.push(kernel.pid);
    assert.notEqual(report.kernelPids[0], kernel.pid);
    for (const row of report.cases) {
      const response = await kernel.api('/api/file?p=' + encodeURIComponent(row.file)); assert.equal(response.status, 200);
      const data = await response.json(), source = data.original ?? data.content; assert.equal(sha(source), row.savedSha256); assert.equal(source, fs.readFileSync(row.file, 'utf8'));
      const f = host({ source }), html = f.html(); assert.deepEqual(tableCells(html), tableCells(f.context.marked.parse(SOURCE, { gfm: true })));
      assert.equal([...html.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)].every(match => [...match[1].matchAll(/<t[hd]([^>]*)>/g)][1][1].includes('align="' + row.alignment + '"')), true);
      row.freshKernelReadback = true;
    }
    report.passed = true;
  } catch (error) { report.passed = false; report.failure = { name: error.name, message: error.message.slice(0, 1200) }; process.exitCode = 1; }
  finally { if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, report: path.join(args.evidence, 'report.json'), failure: report.failure }));
}
if (require.main === module) run().catch(error => { console.error(error.message); process.exitCode = 1; });
