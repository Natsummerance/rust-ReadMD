import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { parseTlpdb, dependencyClosure, verifyContainer } from '../lib/tex-package-inputs.mjs';

test('TeX dependency closure resolves platform companions and cycles without replacing installed packages', () => {
  const records = parseTlpdb(Buffer.from('name ctex\nrevision 1\ndepend xecjk\ndepend native.ARCH\n\nname xecjk\nrevision 2\ndepend ctex\ndepend fontspec\n\nname native.windows\nrevision 3\n\nname fontspec\nrevision 4\n'));
  const installed = new Map([['fontspec', { revision: 'old-pinned-revision' }]]);
  assert.deepEqual(dependencyClosure(records, installed, ['ctex']).map(p => p.name), ['ctex', 'native.windows', 'xecjk']);
  assert.throws(() => dependencyClosure(records, installed, ['absent']), /Missing TeX dependency/);
  assert.throws(() => dependencyClosure(records, installed, ['ctex'], 'linux'), /native.linux/);
});

test('TeX source, docs, runtime and Windows binary sections retain distinct declared file sets', () => {
  const hash = 'a'.repeat(128);
  const records = parseTlpdb(Buffer.from(`name fixture\nrevision 7\nrelocated 1\ncontainersize 10\ncontainerchecksum ${hash}\nsrccontainersize 11\nsrccontainerchecksum ${hash}\ndoccontainersize 12\ndoccontainerchecksum ${hash}\nrunfiles size=1\n RELOC/tex/fixture.sty\nbinfiles arch=windows size=2\n bin/windows/fixture.exe\nsrcfiles size=3\n RELOC/source/fixture.dtx\ndocfiles size=4\n RELOC/doc/README.md details="Readme"\ncatalogue-license lppl1.3c\n`));
  const p = records.get('fixture');
  assert.deepEqual(p.containers.run.files, ['RELOC/tex/fixture.sty', 'bin/windows/fixture.exe']);
  assert.deepEqual(p.containers.source.files, ['RELOC/source/fixture.dtx']);
  assert.deepEqual(p.containers.doc.files, ['RELOC/doc/README.md']);
  assert.equal(p.containers.source.name, 'fixture.source.tar.xz');
});

test('TeX metadata rejects malformed names, duplicate identities, unsafe paths and archive budgets', () => {
  for (const input of ['name p\n\nname p\n', 'name ../p\n', 'name p\nrunfiles size=1\n RELOC/../../outside\n', 'name p\ncontainersize 999999999999\ncontainerchecksum ' + 'a'.repeat(128) + '\n', 'name p\ncontainersize 10\ncontainerchecksum not-a-digest\n']) assert.throws(() => parseTlpdb(Buffer.from(input)));
});

test('TeX container verification rejects correct-size substitutions and truncated payloads', () => {
  const bytes = Buffer.from('Synthetic package');
  const c = { name: 'fixture.tar.xz', size: bytes.length, sha512: crypto.createHash('sha512').update(bytes).digest('hex') };
  verifyContainer(bytes, c);
  assert.throws(() => verifyContainer(Buffer.alloc(bytes.length), c), /integrity/);
  assert.throws(() => verifyContainer(bytes.subarray(1), c), /integrity/);
});
