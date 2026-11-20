// Operator-only CI entry point. No compilation and no tag mutation.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {pipeline} from 'node:stream/promises';
import {PACKAGE_NAMES, writeChecksums, releaseNotes} from './publish-release.mjs';
import {FIXED_TAG, FIXED_TAG_COMMIT, FIXED_RELEASE_ID, assertCandidateRun, refreshV007} from './refresh-v007-transaction.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const runId = Number(process.env.CANDIDATE_RUN), sourceCommit = process.env.CANDIDATE_COMMIT, token = process.env.GH_TOKEN;
if (!process.env.GITHUB_ACTIONS || !token || !Number.isSafeInteger(runId) || runId <= 0 || !/^[a-f\d]{40}$/.test(sourceCommit)
    || fs.readFileSync(path.join(root, 'VERSION'), 'utf8').trim() !== '0.0.7') throw Error('Explicit version-bound CI run/source and repository credential required');
const base = 'repos/Natsummerance/rust-ReadMD', own = path.join(root, '.cache/v007-refresh'), directory = path.join(own, 'packages');
fs.mkdirSync(directory, {recursive: true, mode: 0o700});
const save = (name, value) => fs.writeFileSync(path.join(own, name), typeof value === 'string' ? value : JSON.stringify(value, null, 2) + '\n', {mode: 0o600});
const headers = {Authorization: 'Bearer ' + token, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28'};
async function request(method, route, data, {file} = {}) {
  const url = route.startsWith('https://uploads.github.com/') ? route : 'https://api.github.com/' + route;
  if (!/^https:\/\/(api|uploads)\.github\.com\/repos\/Natsummerance\/rust-ReadMD\//.test(url)) throw Error('Foreign repository endpoint refused');
  const response = await fetch(url, {method, headers: {...headers, 'Content-Type': file ? 'application/octet-stream' : 'application/json'},
    body: file ? await fs.openAsBlob(file) : data === undefined ? undefined : JSON.stringify(data), signal: AbortSignal.timeout(file ? 600000 : 60000)});
  if (!response.ok) throw Error('Repository ' + method + ' HTTP ' + response.status);
  return response.status === 204 ? null : response.json();
}
async function fileHash(file) {
  const digest = crypto.createHash('sha256');
  for await (const chunk of fs.createReadStream(file)) digest.update(chunk);
  return digest.digest('hex');
}
const run = await request('GET', base + '/actions/runs/' + runId);
const jobs = await request('GET', base + '/actions/runs/' + runId + '/jobs?filter=all&per_page=100');
if (jobs.total_count > 100) throw Error('Job history requires pagination review');
assertCandidateRun(run, jobs.jobs, sourceCommit);
save('build-run.json', run); save('build-jobs.json', jobs);
save('release-before.json', await request('GET', base + '/releases/' + FIXED_RELEASE_ID));
const artifacts = (await request('GET', base + '/actions/runs/' + runId + '/artifacts?per_page=100')).artifacts;
const required = {
  'ReadMD-windows-x64': ['ReadMD-windows-x64.zip', 'ReadMDSetup-windows-x64.exe'],
  'ReadMD-linux-x86_64': ['ReadMD-linux-x86_64.deb', 'ReadMD-linux-x86_64.tar.gz'],
  'ReadMD-macos-arm64': ['ReadMD-macos-arm64.dmg', 'ReadMD-macos-arm64.zip'],
  'ReadMD-macos-x64': ['ReadMD-macos-x64.dmg', 'ReadMD-macos-x64.zip'],
  'ReadMD-integrations': ['readmd-vscode-0.0.7.vsix', 'readmd-mcp-server-0.0.7.zip'],
};
const verification = {sourceCommit, tagCommit: FIXED_TAG_COMMIT, runId, packages: [], artifacts: []};
for (const [name, members] of Object.entries(required)) {
  const matches = artifacts.filter(artifact => artifact.name === name && !artifact.expired);
  if (matches.length !== 1 || !/^sha256:[a-f\d]{64}$/.test(matches[0].digest)) throw Error('Missing, ambiguous or unauthenticated source artifact: ' + name);
  const artifact = matches[0], file = path.join(own, name + '.zip');
  if (artifact.workflow_run?.head_sha && artifact.workflow_run.head_sha !== sourceCommit) throw Error('Artifact belongs to another source');
  const redirect = await fetch('https://api.github.com/' + base + '/actions/artifacts/' + artifact.id + '/zip', {headers, redirect: 'manual', signal: AbortSignal.timeout(60000)});
  if (redirect.status !== 302) throw Error('Artifact redirect HTTP ' + redirect.status);
  const location = new URL(redirect.headers.get('location'));
  if (location.protocol !== 'https:' || !/(\.blob\.core\.windows\.net|\.githubusercontent\.com|\.github\.com)$/.test(location.hostname)) throw Error('Unexpected artifact delivery endpoint');
  const response = await fetch(location, {signal: AbortSignal.timeout(600000)});
  if (!response.ok) throw Error('Artifact delivery HTTP ' + response.status);
  await pipeline(response.body, fs.createWriteStream(file, {flags: 'wx', mode: 0o600}));
  if ('sha256:' + await fileHash(file) !== artifact.digest) throw Error('CI artifact digest mismatch: ' + name);
  const entries = execFileSync('unzip', ['-Z1', file], {encoding: 'utf8'}).trim().split(/\r?\n/).sort();
  if (JSON.stringify(entries) !== JSON.stringify([...members].sort())) throw Error('Unexpected CI artifact members: ' + name);
  for (const member of members) {
    const output = path.join(directory, member), fd = fs.openSync(output, 'wx', 0o600);
    try { execFileSync('unzip', ['-p', file, member], {stdio: ['ignore', fd, 'pipe']}); } finally { fs.closeSync(fd); }
    verification.packages.push({name: member, size: fs.statSync(output).size, sha256: await fileHash(output), artifactId: artifact.id, artifactDigest: artifact.digest, artifactDigestVerified: true});
  }
  verification.artifacts.push({name, id: artifact.id, size: artifact.size_in_bytes, digest: artifact.digest, members});
  fs.unlinkSync(file);
  console.log('Authenticated CI artifact: ' + name);
}
if (verification.packages.length !== PACKAGE_NAMES.length) throw Error('Incomplete candidate packages');
writeChecksums(directory);
save('verification.json', verification); save('SHA256SUMS.txt', fs.readFileSync(path.join(directory, 'SHA256SUMS.txt'), 'utf8'));
const identity = `同版资产更新：实际构建源码 [\`${sourceCommit}\`](https://github.com/Natsummerance/rust-ReadMD/commit/${sourceCommit})；[发行 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/${runId})。版本保留 0.0.7，原标签 V0.0.7 固定在 \`${FIXED_TAG_COMMIT}\`，不移动标签。\n\n`;
const notes = releaseNotes(fs.readFileSync(path.join(root, 'RELEASE_NOTES.md'), 'utf8'), {tag: FIXED_TAG, commit: sourceCommit, repo: 'Natsummerance/rust-ReadMD'})
  .replace(/^# ReadMD V0\.0\.7\n\n/, '# ReadMD V0.0.7\n\n' + identity)
  .replaceAll('/blob/V0.0.7/', '/blob/' + sourceCommit + '/');
save('release-notes.md', notes);
async function verifyDownload(asset, file) {
  const url = new URL(asset.browser_download_url);
  if (url.origin !== 'https://github.com' || url.pathname !== '/Natsummerance/rust-ReadMD/releases/download/V0.0.7/' + file.name) throw Error('Unexpected public download address');
  url.searchParams.set('readmd-refresh', sourceCommit);
  const response = await fetch(url, {signal: AbortSignal.timeout(600000)});
  if (!response.ok) throw Error('Public download HTTP ' + response.status);
  const finalUrl = new URL(response.url);
  if (finalUrl.protocol !== 'https:' || !(finalUrl.hostname === 'github.com' || finalUrl.hostname.endsWith('.githubusercontent.com'))) throw Error('Unexpected public asset delivery endpoint');
  const digest = crypto.createHash('sha256'); let size = 0;
  for await (const bytes of response.body) {digest.update(bytes); size += bytes.length;}
  const result = {size, sha256: digest.digest('hex')};
  if (result.size !== file.size || result.sha256 !== file.sha256) throw Error('Public download bytes differ: ' + file.name);
  console.log('Public bytes verified: ' + file.name);
  return result;
}
const checkpoint = journal => {
  const target = path.join(own, 'transaction.json'), temporary = target + '.tmp';
  const fd = fs.openSync(temporary, 'wx', 0o600);
  try {fs.writeFileSync(fd, JSON.stringify(journal, null, 2) + '\n'); fs.fsyncSync(fd);} finally {fs.closeSync(fd);}
  fs.renameSync(temporary, target);
};
const result = await refreshV007({directory, sourceCommit, runId, notes, verification, request, verifyDownload, checkpoint});
if (result.status !== 'complete') throw Error('Verified replacement retains backups; rerun for safe cleanup');
save('release-after.json', await request('GET', base + '/releases/' + FIXED_RELEASE_ID));
console.log('Formal V0.0.7 refresh complete: eleven public downloads at ' + sourceCommit + '; original tag preserved.');
