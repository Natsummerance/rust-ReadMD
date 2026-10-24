import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {verifyGitSource, compareBytes, sha256} from './tex-material-closure-evidence.mjs';

const bytes = Buffer.from('pinned source\n');
const blob = crypto.createHash('sha1').update(Buffer.from('blob ' + bytes.length + '\0')).update(bytes).digest('hex');
const record = {upstreamPath:'bundle/source.dtx', gitBlobSha1:blob, sha256:sha256(bytes)};
const tree = {tree:[{type:'blob', path:record.upstreamPath, size:bytes.length, sha:blob}]};

test('source identity binds a path, length, Git blob and SHA-256 together', () => {
  verifyGitSource(bytes, record, tree);
  const changed = Buffer.from(bytes); changed[0] ^= 1;
  assert.throws(() => verifyGitSource(changed, record, tree));
  assert.throws(() => verifyGitSource(bytes.subarray(1), record, tree));
  assert.throws(() => verifyGitSource(bytes, {...record, upstreamPath:'other.dtx'}, tree));
  assert.throws(() => verifyGitSource(bytes, {...record, sha256:'0'.repeat(64)}, tree));
});

test('a forged tree entry cannot replace the fixed Git blob with a raw SHA-1', () => {
  const raw = crypto.createHash('sha1').update(bytes).digest('hex');
  assert.throws(() => verifyGitSource(bytes, {...record, gitBlobSha1:raw}, {tree:[{...tree.tree[0], sha:raw}]}));
});

test('newline-only correspondence is never counted as exact bytes', () => {
  assert.deepEqual(compareBytes(bytes, bytes), {exact:true, newlineNormalized:true});
  assert.deepEqual(compareBytes(Buffer.from('pinned source\r\n'), bytes), {exact:false, newlineNormalized:true});
});

test('a semantic source change remains a mismatch after newline normalization', () => {
  assert.deepEqual(compareBytes(Buffer.from('changed source\r\n'), bytes), {exact:false, newlineNormalized:false});
});
