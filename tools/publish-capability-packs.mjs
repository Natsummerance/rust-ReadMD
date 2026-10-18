// Offline preflight by default. --publish explicitly writes only tool-ReadMD.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { repository, validateCatalog, verifyLocalPack, verifyPublishedAsset } from './lib/capability-publication.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
async function main() {
  const args = process.argv.slice(2);
  let folder, publish = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--packs' && args[i + 1]) folder = path.resolve(args[++i]);
    else if (args[i] === '--publish') publish = true;
    else throw Error('Usage: node tools/publish-capability-packs.mjs --packs DIRECTORY [--publish]');
  }
  if (!folder) throw Error('A directory containing catalog-named .readmd-pack files is required');
  const catalogBytes = fs.readFileSync(path.join(root, 'assets/capabilities/catalog.json'));
  const catalog = validateCatalog(JSON.parse(catalogBytes));
  const artifacts = [], files = { 'catalog.json': catalogBytes, LICENSE: fs.readFileSync(path.join(root, 'LICENSE')) };
  for (const entry of catalog.packs) {
    const name = entry.url.split('/').pop();
    let bytes;
    try { bytes = fs.readFileSync(path.join(folder, name)); } catch { throw Error('Missing local artifact: ' + name); }
    const lockName = `${entry.id}-${entry.version}-${entry.platform}.lock.json`;
    const lockBytes = fs.readFileSync(path.join(root, 'assets/capabilities', lockName));
    verifyLocalPack(bytes, JSON.parse(lockBytes), entry);
    artifacts.push({ entry, name, bytes });
    files['locks/' + lockName] = lockBytes;
  }
  for (const name of ['build-pdf-pack.mjs', 'build-typst-pack.mjs', 'build-ocr-runtime.mjs', 'build-ocr-pack.mjs', 'ocr-runtime-inputs.lock.json', 'build-publication-fonts.mjs', 'publication-font-inputs.lock.json', 'publication-static-fonts.lock.json', 'fonts/prepare_static.py', 'prepare-capability-catalog.mjs', 'publish-capability-packs.mjs', 'lib/capability-publication.mjs']) files['builders/' + name] = fs.readFileSync(path.join(root, 'tools', name));
  const available = catalog.packs.map(p => `| ${p.id} ${p.version} | ${p.platform} | ${(p.downloadBytes / 1048576).toFixed(1)} MiB | ${(p.installedBytes / 1048576).toFixed(1)} MiB |`).join('\n');
  const readme = `# ReadMD capability packs

Optional offline engines for ReadMD. Markdown, Rust-native DOCX/EPUB conversion and basic PDF output remain in the core. Generating Word files does not require Office, WPS or LibreOffice.

| Engine | Platform | Download | Installed |
| --- | --- | ---: | ---: |
${available}

PDFium provides PDF page-object editing/rasterization. Typst provides academic PDF typesetting. OCR includes a static Tesseract/Leptonica engine and English, simplified Chinese and traditional Chinese models; Rust decodes images without Office, Python, Node or OS OCR language packs. Handwriting and universal recognition accuracy are not certified.

Use the feature-entry installer, choose components in Windows setup, or import a .readmd-pack offline. Application-pinned locks, each file digest and an actual engine operation are checked before installation. Packs contain no installer scripts. A remotely edited catalog cannot authorize new code for an existing application.

The universal fonts pack contains private regular/bold sans/serif Chinese publication fonts, derived at fixed weights from pinned Noto sources. Copyright, OFL licenses and modification notices are included; no OS font registration. Small STIX Two Math ships in the main ReadMD core. The font preparation tool is development-only; neither Python nor fontTools is required by ReadMD.

Only listed engine platform packs are prepared. Native evidence currently covers Windows x64. Transcription, traditional TeX, further script coverage, GUI host delivery and other engine platforms remain open. Original upstream licenses and notices are retained inside every pack. This release is independent of the main ReadMD application version.

## Rebuilding and publishing

Builders are archived for inspection; run them from the matching rust-ReadMD checkout, which supplies assets/, tools/, the worker source and Cargo.lock. Copy builders/ into that checkout's tools/ if necessary. Prepare exact locked local inputs separately, then build offline. OCR source/model/tool locks are in builders/ocr-runtime-inputs.lock.json. Compiler output is locked per reviewed build; bit-identical output across arbitrary compilers is not promised.

Put catalog-named packages in one staging directory. From the main checkout run:

\`\`\`sh
node tools/publish-capability-packs.mjs --packs STAGING_DIRECTORY
\`\`\`

The default is an offline preflight: no network or credential read. After review, append --publish to update only this owned repository using the configured GitHub credential. All local payloads are checked first. Existing published assets with a different digest are refused rather than overwritten. New versions need a new release tag, updated application locks, licenses and native evidence. No documents, videos, social assets or user profile files are included.
`;
  files['README.md'] = Buffer.from(readme);
  if (!publish) {
    console.log(JSON.stringify({ offline: true, published: false, release: catalog.release, verifiedPacks: artifacts.length, repository }));
    return;
  }
  const credential = spawnSync('git', ['credential', 'fill'], { input: 'protocol=https\nhost=github.com\n\n', encoding: 'utf8', env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GCM_INTERACTIVE: 'Never' }, windowsHide: true });
  const token = (credential.stdout || '').match(/^password=(.+)$/m)?.[1];
  if (!token) throw Error('Configured GitHub credential unavailable');
  const headers = { Authorization: 'Bearer ' + token, 'User-Agent': 'ReadMD-capability-delivery', Accept: 'application/vnd.github+json' };
  async function api(route, method = 'GET', body) {
    const response = await fetch('https://api.github.com' + route, { method, headers: { ...headers, 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(120000) });
    const data = await response.json().catch(() => ({}));
    if (!response.ok && !(method === 'GET' && response.status === 404)) throw Error('GitHub API status ' + response.status);
    return { status: response.status, data };
  }
  if ((await api('/user')).data.login !== repository.split('/')[0]) throw Error('Unexpected authenticated owner');
  let repo = await api('/repos/' + repository);
  if (repo.status === 404) repo = await api('/user/repos', 'POST', { name: 'tool-ReadMD', description: 'Reviewed, pinned offline capability packs for ReadMD. No document or user data.', private: false, auto_init: true });
  if (repo.data.full_name !== repository || repo.data.private) throw Error('Unexpected repository');
  let release = await api('/repos/' + repository + '/releases/tags/' + catalog.release);
  if (release.status !== 404) for (const { entry, name } of artifacts) {
    const existing = (release.data.assets || []).find(a => a.name === name);
    if (existing) verifyPublishedAsset(existing, entry);
  }
  // No repository mutation before local and existing-release integrity checks.
  for (const [name, bytes] of Object.entries(files)) {
    const old = await api('/repos/' + repository + '/contents/' + name);
    if (old.status === 200 && Buffer.from(old.data.content, 'base64').equals(bytes)) continue;
    await api('/repos/' + repository + '/contents/' + name, 'PUT', { message: 'Maintain reviewed offline capability delivery', content: bytes.toString('base64'), ...(old.status === 200 ? { sha: old.data.sha } : {}) });
  }
  const body = 'Pinned capability engines and models listed in catalog.json; exact package and per-file hashes. Native evidence currently covers Windows x64. Upstream licenses are included. This does not release a new main application version.';
  if (release.status === 404) release = await api('/repos/' + repository + '/releases', 'POST', { tag_name: catalog.release, name: 'ReadMD capability packs', draft: true, prerelease: false, body });
  for (const { entry, name, bytes } of artifacts) {
    if ((release.data.assets || []).some(a => a.name === name)) continue;
    const uploadUrl = release.data.upload_url?.split('{')[0];
    if (!uploadUrl?.startsWith(`https://uploads.github.com/repos/${repository}/releases/`)) throw Error('Unexpected upload endpoint');
    const response = await fetch(uploadUrl + '?name=' + encodeURIComponent(name), { method: 'POST', headers: { ...headers, 'Content-Type': 'application/octet-stream' }, body: bytes, signal: AbortSignal.timeout(600000) });
    const asset = await response.json().catch(() => ({}));
    if (!response.ok) throw Error('Upload status ' + response.status);
    verifyPublishedAsset(asset, entry);
  }
  await api('/repos/' + repository + '/releases/' + release.data.id, 'PATCH', { draft: false, body });
  console.log(JSON.stringify({ repository: 'https://github.com/' + repository, release: catalog.release, published: true, verifiedPacks: artifacts.length }));
}
main().catch(error => { console.error('Capability publication failed: ' + String(error.message).replaceAll(root, '<workspace>')); process.exitCode = 1; });
