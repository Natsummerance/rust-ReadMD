import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {selectUnifiedClosure} from '../plan-tex-material-closure.mjs';

const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const entry = (name, revision, dependencies = []) => ['name ' + name, 'revision ' + revision,
  ...dependencies.map(d => 'depend ' + d), 'containersize 12', 'containerchecksum ' + String(revision % 10).repeat(128)].join('\n');
const metadata = Buffer.from([entry('engine', 2, ['helper.ARCH']), entry('helper.windows', 9), entry('ctex', 3, ['engine'])].join('\n\n') + '\n');
const prior = {roots:['ctex'],components:[{name:'engine',revision:'1',base:true,containers:{}}]};
const installed = new Map([['engine',{revision:'1',containers:{run:{sha512:'0'.repeat(128)}}}]]);

test('a new closure replaces every base revision and resolves Windows dependencies solely from its one database', () => {
  const selected = selectUnifiedClosure(metadata, hash(metadata), prior, installed);
  assert.deepEqual(selected.components.map(c => c.name), ['ctex','engine','helper.windows']);
  assert.equal(selected.components.find(c => c.name === 'engine').revision, '2');
  assert.equal(selected.changes.find(c => c.name === 'engine').containers[0].oldSha512, '0'.repeat(128));
  assert.equal(selected.changes.find(c => c.name === 'engine').containers[0].newSha512, '2'.repeat(128));
  assert.equal(installed.get('engine').revision, '1');
});

test('unlocked metadata is refused even when its records are otherwise valid', () => {
  assert.throws(() => selectUnifiedClosure(metadata, '0'.repeat(64), prior, installed), /Unreviewed package database/);
});

test('an incomplete new snapshot cannot be patched with old installed packages', () => {
  const missing = Buffer.from(entry('ctex',3,['engine']) + '\n');
  assert.throws(() => selectUnifiedClosure(missing, hash(missing), prior, installed), /Missing TeX dependency: engine/);
});
