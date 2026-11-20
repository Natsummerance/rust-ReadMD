import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {PACKAGE_NAMES, writeChecksums} from '../publish-release.mjs';
import {FIXED_TAG, FIXED_TAG_COMMIT, FIXED_RELEASE_ID, DOWNLOAD_NAMES, refreshV007} from '../refresh-v007-transaction.mjs';
import {embeddedTemplateDigests} from '../inspect-v007-packages.mjs';

const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function fixture(t, {badUpload = false, failRenameAt = 0, badDownload = false} = {}) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-v007-refresh-'));
  t.after(() => fs.rmSync(directory, {recursive: true, force: true}));
  const sourceCommit = 'a'.repeat(40), runId = 23, events = [], checkpoints = [];
  const packages = PACKAGE_NAMES.map(name => {
    const bytes = Buffer.from('new verified ' + name); fs.writeFileSync(path.join(directory, name), bytes);
    return {name, size: bytes.length, sha256: sha(bytes), artifactDigestVerified: true, sourceInspectionVerified: true};
  });
  writeChecksums(directory);
  const original = DOWNLOAD_NAMES.map((name, i) => ({id: i + 1, name, size: 3, state: 'uploaded', digest: 'sha256:' + sha('old')}));
  const all = structuredClone(original), release = {id: FIXED_RELEASE_ID, tag_name: FIXED_TAG, draft: false, immutable: false, body: 'Original notes', upload_url: 'https://uploads.github.com/repos/Natsummerance/rust-ReadMD/releases/451629011/assets{?name}'};
  const run = {head_sha: sourceCommit, head_branch: 'codex/v007-export-layout-publish-20261120', repository: {full_name: 'Natsummerance/rust-ReadMD'}, status: 'completed', conclusion: 'success', event: 'workflow_dispatch', path: '.github/workflows/release.yml'};
  const names = ['Build · Windows x64', 'Build · Linux x64', 'Build · macOS ARM64', 'Build · macOS Intel', 'Kernel tests · windows-latest', 'Kernel tests · ubuntu-latest', 'Kernel tests · macos-latest', 'Frontend gates', 'UI quality (Playwright)', 'VS Code and MCP quality', 'Owned small volume · ubuntu-latest', 'Owned small volume · windows-latest'];
  const jobs = names.map((name, i) => ({id: 100 + i, name, conclusion: 'success', steps: ['P0 lifecycle against the actual Windows or Linux release binary', 'Verify platform image exports and installed PDFium'].map(name => ({name, conclusion: 'success'}))}));
  let next = 200, renames = 0;
  const state = {tagCommit: FIXED_TAG_COMMIT, mainCommit: 'd'.repeat(40), mainStatus: 'ahead'};
  const request = async (method, route, data, {file} = {}) => {
    events.push({method, route, data});
    if (route.includes('/git/ref/heads/main')) return {object: {type: 'commit', sha: state.mainCommit}};
    if (route.includes('/git/ref/tags/')) return {object: {type: 'commit', sha: state.tagCommit}};
    if (route.includes('/commits/')) return {sha: sourceCommit};
    if (route.includes('/compare/')) return {status: state.mainStatus};
    if (route.endsWith('/actions/runs/23')) return structuredClone(run);
    if (route.includes('/jobs?')) return {total_count: jobs.length, jobs: structuredClone(jobs)};
    if (method === 'GET' && route.includes('/releases/tags/')) return structuredClone(release);
    if (method === 'GET' && route.includes('/assets?')) return structuredClone(all);
    if (file) {
      const asset = {id: next++, name: new URL(route).searchParams.get('name'), state: 'uploaded', size: fs.statSync(file).size, digest: 'sha256:' + (badUpload ? 'f'.repeat(64) : sha(fs.readFileSync(file)))};
      all.push(asset); return structuredClone(asset);
    }
    if (route.includes('/releases/assets/')) {
      const at = all.findIndex(asset => asset.id === Number(route.split('/').at(-1)));
      if (method === 'DELETE') {all.splice(at, 1); return null;}
      if (++renames === failRenameAt) throw Error('controlled rename interruption');
      Object.assign(all[at], data); return structuredClone(all[at]);
    }
    if (method === 'PATCH' && route.endsWith('/releases/' + FIXED_RELEASE_ID)) {Object.assign(release, data); return structuredClone(release);}
    throw Error('Unexpected request: ' + method + ' ' + route);
  };
  const verifyDownload = async (_asset, file) => ({size: file.size, sha256: badDownload ? 'f'.repeat(64) : file.sha256});
  return {directory, sourceCommit, runId, notes: 'Updated package source ' + sourceCommit + '; fixed tag ' + FIXED_TAG_COMMIT,
    verification: {sourceCommit, publicationCommit: state.mainCommit, runId, packages}, request, verifyDownload, checkpoint: j => checkpoints.push(structuredClone(j)),
    all, original, events, checkpoints, run, jobs, release, state};
}

test('all eleven candidates authenticate before replacement; public bytes precede old-asset deletion', async t => {
  const f = fixture(t), result = await refreshV007(f);
  assert.equal(result.status, 'complete');
  const firstRename = f.events.findIndex(event => event.method === 'PATCH');
  assert.equal(f.events.slice(0, firstRename).filter(event => event.method === 'POST').length, 11);
  assert.equal(f.all.length, 11);
  assert.deepEqual(f.all.map(asset => asset.name).sort(), [...DOWNLOAD_NAMES].sort());
  assert(f.all.every(asset => asset.id >= 200));
  assert(f.events.every(event => !event.route.includes('/git/') || event.method === 'GET'));
  assert.equal(f.release.body, f.notes);
  assert(f.checkpoints.find(j => j.status === 'verified'));
});

test('a rename failure restores every original asset identity and notes without deleting old bytes', async t => {
  const f = fixture(t, {failRenameAt: 9});
  await assert.rejects(refreshV007(f), /rolled_back.*interruption/);
  for (const old of f.original) assert.deepEqual(f.all.find(asset => asset.id === old.id), old);
  assert.equal(f.release.body, 'Original notes');
  assert(f.events.every(event => event.method !== 'DELETE'));
  assert.equal((await refreshV007(f)).status, 'complete');
});

test('a mismatched public download rolls back all eleven files before any old asset deletion', async t => {
  const f = fixture(t, {badDownload: true});
  await assert.rejects(refreshV007(f), /rolled_back.*Public download bytes differ/);
  for (const old of f.original) assert.deepEqual(f.all.find(asset => asset.id === old.id), old);
  assert(f.events.every(event => event.method !== 'DELETE'));
});

test('bad uploads and changed fixed tags fail before public asset changes', async t => {
  const f = fixture(t, {badUpload: true});
  await assert.rejects(refreshV007(f), /SHA-256 authentication/);
  assert(f.events.every(event => !['PATCH', 'DELETE'].includes(event.method)));
  const g = fixture(t); g.state.tagCommit = 'b'.repeat(40);
  await assert.rejects(refreshV007(g), /tag changed/);
  assert(g.events.every(event => event.method === 'GET'));
});

test('latest failed quality jobs and source not integrated into main are refused before mutation', async t => {
  const f = fixture(t); f.jobs.push({id: 500, name: 'Release quality / Frontend gates', conclusion: 'failure'});
  await assert.rejects(refreshV007(f), /latest job has not passed/);
  assert(f.events.every(event => event.method === 'GET'));
  const g = fixture(t); g.state.mainStatus = 'diverged';
  await assert.rejects(refreshV007(g), /integrated into public main/);
  assert(g.events.every(event => event.method === 'GET'));
});

test('a killed same-source switch is recovered and retried using the retained original identity', async t => {
  const f = fixture(t), prefix = 'v007-refresh-' + f.sourceCommit.slice(0, 12) + '-';
  const name = PACKAGE_NAMES[0], proof = f.verification.packages.find(file => file.name === name), old = f.all.find(asset => asset.name === name);
  old.name = prefix + 'previous-' + old.id + '-' + name;
  f.all.push({id: 190, name, size: proof.size, state: 'uploaded', digest: 'sha256:' + proof.sha256});
  assert.equal((await refreshV007(f)).status, 'complete');
  assert.equal(f.all.length, 11);
});

test('a completed same-source transaction is safely idempotent and does not reupload packages', async t => {
  const f = fixture(t); await refreshV007(f); f.events.length = 0;
  assert.equal((await refreshV007(f)).status, 'complete');
  assert(!f.events.some(event => event.method === 'POST'));
  assert.equal(f.all.length, 11);
});

test('another refresh namespace cannot be silently consumed or removed', async t => {
  const f = fixture(t); f.all.push({id: 800, name: 'v007-refresh-foreign-old.zip'});
  await assert.rejects(refreshV007(f), /another refresh asset/);
  assert(f.events.every(event => event.method === 'GET'));
});

test('container provenance cannot replace inspection of the shipped source bytes', async t => {
  const f = fixture(t); f.verification.packages[0].sourceInspectionVerified = false;
  await assert.rejects(refreshV007(f), /source-inspected CI artifact/);
  assert.deepEqual(f.events, []);
});

test('all exact native template bytes must survive compilation into the executable', () => {
  const templates = new Map([['one', Buffer.from('native design one')], ['two', Buffer.from('native design two')]]);
  const binary = Buffer.concat([Buffer.from('binary prefix'), ...templates.values(), Buffer.from('binary suffix')]);
  assert.deepEqual(embeddedTemplateDigests(binary, templates).embeddedTemplates, {one: sha(templates.get('one')), two: sha(templates.get('two'))});
  const changed = new Map(templates); changed.set('two', Buffer.from('native design XXo'));
  assert.throws(() => embeddedTemplateDigests(binary, changed), /exact source template: two/);
});

test('a newer public main cannot be silently certified by an earlier source inspection', async t => {
  const f = fixture(t); f.state.mainCommit = 'e'.repeat(40);
  await assert.rejects(refreshV007(f), /main changed since product\/source inspection/);
  assert(f.events.every(event => event.method === 'GET'));
});
