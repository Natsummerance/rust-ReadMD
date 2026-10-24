'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ROOT, inspectPe } = require('./interop-diagnostics.cjs');
const { createEpub, inspectArchive, compareFeatures, redactLog, runOwnedConsole } = require('./interop-calibre-roundtrip.cjs');
const zipParts = require('../../ui-tests/zip-parts.cjs');
const cache = path.join(ROOT, '.cache/capability-upgrade/longtail-20261019/interop');

test('the deterministic synthetic EPUB has a first uncompressed mimetype and real local notes/math', () => {
  const bytes = createEpub(), parts = zipParts(bytes);
  assert.deepEqual(createEpub(), bytes);
  assert.equal(bytes.readUInt16LE(8), 0);
  assert.equal(bytes.subarray(30, 30 + bytes.readUInt16LE(26)).toString(), 'mimetype');
  assert.equal(parts.get('mimetype'), 'application/epub+zip');
  assert.equal(parts.size, 7);
  assert.ok(parts.get('EPUB/one.xhtml').includes('display="block"'));
  assert.ok(parts.get('EPUB/notes.xhtml').includes('role="doc-footnote"'));
});

test('independent standard-library inspection resolves the fixed book links and MathML', () => {
  fs.mkdirSync(cache, { recursive: true });
  const directory = fs.mkdtempSync(path.join(cache, 'unit-epub-'));
  try {
    fs.writeFileSync(path.join(directory, 'source.epub'), createEpub());
    const source = inspectArchive(directory, 'source.epub');
    assert.equal(source.mathCount, 2);
    assert.equal(source.displayMathCount, 1);
    assert.equal(source.fractionCount, 1);
    assert.equal(source.noterefCount, 1);
    assert.equal(source.footnoteCount, 1);
    assert.equal(compareFeatures(source, source).passed, true);
    const missingMath = { ...source, mathCount: 0, displayMathCount: 0, fractionCount: 0, mathTokens: [] };
    assert.equal(compareFeatures(source, missingMath).checks.find(check => check.id === 'mathml-objects').passed, false);
    const badLinks = { ...source, links: source.links.map(link => ({ ...link, expectedContextFound: false })) };
    assert.equal(compareFeatures(source, badLinks).checks.find(check => check.id === 'crosslinks-and-note-return').passed, false);
    assert.equal(compareFeatures(source, { ...source, footnoteCount: 0 }).checks.find(check => check.id === 'epub-note-roles').passed, false);
  } finally { fs.unlinkSync(path.join(directory, 'source.epub')); fs.rmdirSync(directory); }
});

test('PE subsystem identity distinguishes a console from a GUI executable', () => {
  const bytes = Buffer.alloc(512); bytes.writeUInt16LE(0x5a4d); bytes.writeUInt32LE(128, 0x3c); bytes.writeUInt32LE(0x4550, 128); bytes.writeUInt16LE(0x8664, 132); bytes.writeUInt16LE(3, 128 + 24 + 68);
  assert.equal(inspectPe(bytes).console, true);
  bytes.writeUInt16LE(2, 128 + 24 + 68);
  assert.equal(inspectPe(bytes).console, false);
  bytes.writeUInt32LE(0xffffff00, 0x3c);
  assert.equal(inspectPe(bytes).verified, false);
});

test('CLI logs discard absolute user paths, full executable paths and credential-shaped values', () => {
  const log = 'X:\\Private\\Book.epub\nZ:\\Workspace\\run\\source.epub\nZ:\\Apps\\ebook-convert.exe\napi_key=do-not-publish token:abc';
  const redacted = redactLog(log, 'Z:\\Workspace\\run', 'Z:\\Apps\\ebook-convert.exe');
  assert.equal(redacted.includes('Book.epub'), false);
  assert.equal(redacted.includes('do-not-publish'), false);
  assert.equal(redacted.includes('token:abc'), false);
  assert.ok(redacted.includes('<owned-run>\\source.epub'));
});

test('owned Windows Job wrapper launches a console and completes without GUI access', { skip: process.platform !== 'win32' }, () => {
  fs.mkdirSync(cache, { recursive: true });
  const directory = fs.mkdtempSync(path.join(cache, 'unit-job-complete-'));
  const result = runOwnedConsole(process.execPath, ['--version'], directory, 'node-version', 15000);
  assert.equal(result.state, 'completed');
  assert.equal(result.jobOwned, true);
  assert.equal(result.exitCode, 0);
  assert.equal(result.outerTimedOut, false);
  assert.match(fs.readFileSync(path.join(directory, 'node-version.stdout.redacted.log'), 'utf8'), /^v\d+\./);
  fs.writeFileSync(path.join(directory, 'result.json'), JSON.stringify(result, null, 2) + '\n');
});

test('owned Windows Job deadline stops only the synthetic console process tree', { skip: process.platform !== 'win32' }, async () => {
  fs.mkdirSync(cache, { recursive: true });
  const directory = fs.mkdtempSync(path.join(cache, 'unit-job-timeout-'));
  fs.writeFileSync(path.join(directory, 'hang.cjs'), "const fs=require('node:fs'),{spawn}=require('node:child_process');if(process.argv[2]!=='child'){const child=spawn(process.execPath,[__filename,'child'],{windowsHide:true,stdio:'ignore'});fs.writeFileSync('owned-pids.json',JSON.stringify({parent:process.pid,child:child.pid}));}setInterval(()=>{},1000);\n");
  const result = runOwnedConsole(process.execPath, ['hang.cjs'], directory, 'node-timeout', 6000);
  assert.ok(['deadline_exceeded', 'wrapper_result_incomplete'].includes(result.state));
  assert.ok(result.outerElapsedMs < 8000);
  const pids = JSON.parse(fs.readFileSync(path.join(directory, 'owned-pids.json'), 'utf8'));
  for (const pid of Object.values(pids)) {
    let exists = true;
    const deadline = Date.now() + 1000;
    while (exists && Date.now() < deadline) {
      try { process.kill(pid, 0); } catch (error) { if (error.code === 'ESRCH') exists = false; else throw error; }
      if (exists) await new Promise(resolve => setTimeout(resolve, 50));
    }
    assert.equal(exists, false, 'only-owned synthetic child and descendant exited when helper job closed');
  }
  fs.writeFileSync(path.join(directory, 'result.json'), JSON.stringify({ ...result, ownedProcessesExited: true }, null, 2) + '\n');
});
