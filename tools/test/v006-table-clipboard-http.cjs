'use strict';
// node tools/test/v006-table-clipboard-http.cjs --bin ./ReadMD.exe --evidence .cache/v006-table-clipboard/http-1
// Runs production source functions in a Node VM with real CM state/history,
// then calls the real HTTP writer. This is deliberately not a GUI paste test.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { ROOT, host, tableCells } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');

async function run() {
  const args = options(process.argv.slice(2));
  if (!args.bin || args.base) throw Error('HTTP restart acceptance needs an explicit --bin');
  const documents = path.join(args.evidence, 'documents');
  fs.mkdirSync(documents);
  const original = '# Owned table acceptance\r\n\r\n';
  const cases = [
    { name: 'tsv', text: '\tName\tNote\t\r\n001\t"two\tparts"\t"line 1\r\nline 2"\t\r\n002\t"He said ""yes"""\t"\\| <b> &amp; **x**"\t\r\n\t\t\t\r\n',
      expected: [['', 'Name', 'Note', ''], ['001', 'two\tparts', 'line 1\nline 2', ''], ['002', 'He said "yes"', '\\| <b> &amp; **x**', ''], ['', '', '', '']] },
    { name: 'csv', text: 'fallback', types: { 'text/csv': '\uFEFFName,Note,Extra\r\n"One, two","He said ""yes""\r\nnext",\r\n' },
      expected: [['Name', 'Note', 'Extra'], ['One, two', 'He said "yes"\nnext', '']] }
  ];
  const report = { schemaVersion: 1, syntheticOnly: true, guiStarted: false, compiled: false,
    pasteTransport: 'production cmHandlePaste called in Node VM; real CodeMirror documents/history',
    storageTransport: 'production saveEdit -> authenticated HTTP /api/save -> disk -> fresh kernel /api/file',
    userConfigurationRead: false, tokensLogged: false, binarySha256: sha(fs.readFileSync(args.bin)),
    sourceSha256: Object.fromEntries(['assets/js/editor/editor.js', 'assets/js/editor/preview.js', 'assets/vendor/codemirror.bundle.js', 'assets/vendor/marked.min.js']
      .map(file => [file, sha(fs.readFileSync(path.join(ROOT, file)))])), cases: [], kernelPids: [] };
  let kernel;
  try {
    kernel = await startKernel(args.bin, path.join(args.evidence, 'profile-first'), documents);
    report.kernelPids.push(kernel.pid);
    for (const fixture of cases) {
      const file = path.join(documents, fixture.name + '.md');
      fs.writeFileSync(file, original, { flag: 'wx' });
      const before = await (await kernel.api('/api/file?p=' + encodeURIComponent(file))).json();
      // Match loadFile: /api/file may offer a fixed reader projection in
      // content, while the editable/persisted source is original.
      const authorSource = before.original ?? before.content;
      assert.equal(authorSource, original);
      const f = host({ source: authorSource, file, apiFetch: kernel.api });
      Object.assign(f.tab, { revision: before.revision, mtime: before.mtime });
      assert.equal(f.paste(fixture.text, fixture.types).handled, true);
      assert.deepEqual(tableCells(f.html()), fixture.expected);
      const saved = f.context.getEditContent();
      assert.equal(/(^|[^\r])\n/.test(saved), false);
      f.undo(); assert.equal(f.context.getEditContent(), original);
      f.redo(); assert.equal(f.context.getEditContent(), saved);
      assert.equal(await f.save(), true);
      assert.equal(f.calls.length, 1);
      assert.equal(f.calls[0].route, '/api/save');
      assert.equal(fs.readFileSync(file, 'utf8'), saved);
      assert.equal(f.tab.original, saved);
      assert.equal(f.tab.revision, sha(saved));
      f.undo(); assert.equal(f.context.getEditContent(), original);
      f.redo(); assert.equal(f.context.getEditContent(), saved);
      const conflict = await kernel.api('/api/save', { method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: file, content: original, expected_revision: before.revision, encoding: 'utf-8' }) });
      assert.equal(conflict.status, 409);
      assert.equal((await conflict.json()).conflict, true);
      assert.equal(fs.readFileSync(file, 'utf8'), saved);
      fixture.saved = saved;
      report.cases.push({ name: fixture.name, savedSha256: sha(saved), savedBytes: Buffer.byteLength(saved),
        singlePasteUndoBeforeAndAfterSave: true, crlf: true, staleSaveRejected: true, savedViaHttp: true });
    }
    await kernel.stop(); kernel = null;
    kernel = await startKernel(args.bin, path.join(args.evidence, 'profile-fresh'), documents);
    report.kernelPids.push(kernel.pid);
    assert.notEqual(report.kernelPids[0], report.kernelPids[1]);
    for (const [index, fixture] of cases.entries()) {
      const file = path.join(documents, fixture.name + '.md');
      const response = await kernel.api('/api/file?p=' + encodeURIComponent(file));
      assert.equal(response.status, 200);
      const reopened = await response.json();
      const authorSource = reopened.original ?? reopened.content;
      assert.equal(authorSource, fixture.saved);
      assert.equal(reopened.revision, sha(fixture.saved));
      const f = host({ source: authorSource, file });
      assert.equal(f.context.getEditContent(), fixture.saved);
      assert.deepEqual(tableCells(f.html()), fixture.expected);
      report.cases[index].freshKernelReadback = true;
    }
    report.passed = true;
  } catch (error) {
    report.passed = false;
    report.failure = { name: error.name, message: error.message.slice(0, 1200) };
    process.exitCode = 1;
  } finally {
    if (kernel) await kernel.stop();
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2));
  }
  console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, freshKernelReadback: report.cases.filter(row => row.freshKernelReadback).length,
    report: path.join(args.evidence, 'report.json'), ...(report.failure ? { failure: report.failure } : {}) }));
}
if (require.main === module) run().catch(error => { console.error(error.message); process.exitCode = 1; });
