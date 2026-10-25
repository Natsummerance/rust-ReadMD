'use strict';
// Freeze product + QA together. Never builds or changes a historical candidate.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), {execFileSync} = require('node:child_process');
const root = path.resolve(__dirname, '../..');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const relative = file => path.relative(root, file).split(path.sep).join('/');
const parent = path.join(root, '.cache/capability-upgrade/longtail-step12-20261018');
const previousRoot = path.join(root, '.cache/capability-upgrade/batch3-closeout-20261018');
const sourcePattern = /^(assets\/(js\/|css\/|i18n\/|vendor\/|data\/|capabilities\/|fonts\/|licenses\/|math-catalog\.json$|index\.html$|style\.css$|app\.js$|readmd\.boot\.js$)|rust\/(readmd-kernel\/(src\/|Cargo\.toml$|build\.rs$)|Cargo\.(toml|lock)$|xtask\/|\.cargo\/)|tests\/(frontend\/|fixtures\/)|tools\/(test\/|lib\/|check-[^/]+\.mjs$|plan-tex-material-closure\.mjs$|no-python\.allow\.json$|i18n-add\.mjs$|i18n-batch3-(?:docx-settings|math-section|author-inline)\.json$|prepare-batch3-author-inline-i18n\.cjs$|.*lock\.json$)|docs\/design\/office-math-coverage\.json$|ui-tests\/[^/]+\.(cjs|mjs|js|json)$|packages\/(mcp-server|vscode-readmd|vscode-extension)\/(src\/|test\/|package(-lock)?\.json$))/;
// QA observers and packaging inputs are part of the release identity too.
const supplementalInputs = new Set(["ui-tests/helpers/windows-worker-observer.cs","ui-tests/helpers/windows-worker-observer.ps1","tools/i18n-v005-release-closeout.json","VERSION","ReadMD.nsi","release/platform-matrix.json",".github/workflows/release.yml",".github/workflows/repo-quality.yml","tools/publish-release.mjs","tools/stage-windows-runtime.mjs","tools/package-integrations.mjs","docs/design/v005-release-scope-2026-10-25.md","assets/readmd.ico","assets/runtimes/windows-x64/runtime.lock.json","assets/runtimes/windows-x64/vcruntime140.dll","assets/runtimes/windows-x64/vcruntime140_1.dll","docs/design/capability-upgrade-acceptance.json","README.md","README.en.md","README.ja.md","README.zh-TW.md","RELEASE_NOTES.md","release/release_notes.md","tools/privacy-scan.mjs"]);
for(const file of ['tools/native-build-env.mjs','tools/build-svg-renderer.mjs','assets/engines/resvg/windows-x64/resvg.exe','assets/engines/resvg/windows-x64/lock.json'])supplementalInputs.add(file);
function contained(base, file) {
  const name = path.relative(base, file);
  return name && name !== '..' && !name.startsWith('..' + path.sep) && !path.isAbsolute(name);
}
function repositoryFile(base, file) {
  assert(contained(base, file), 'Literal Rust include escapes repository');
  assert(fs.existsSync(file), 'Missing literal Rust include file: ' + path.relative(base, file));
  assert(fs.statSync(file).isFile(), 'Literal Rust include must name a file');
  assert(contained(fs.realpathSync(base), fs.realpathSync(file)), 'Literal Rust include resolves outside repository');
  return file;
}
function relativeIncludeName(value) {
  assert(typeof value === 'string' && value.length > 0 && !/[\x00-\x1f\x7f]/.test(value), 'Invalid literal Rust include path');
  assert(!path.posix.isAbsolute(value) && !path.win32.isAbsolute(value) && !/^[a-z]:/i.test(value), 'Absolute literal Rust include path');
  return value.replaceAll('\\', path.sep).replaceAll('/', path.sep);
}
function resolveLiteralInclude(source, reference, base = root) {
  base = path.resolve(base);
  const sourceFile = repositoryFile(base, path.resolve(base, relativeIncludeName(source)));
  const file = repositoryFile(base, path.resolve(path.dirname(sourceFile), relativeIncludeName(reference)));
  return path.relative(base, file).split(path.sep).join('/');
}
function literalRustInputs(files, base = root) {
  base = path.resolve(base); const inputs = new Set();
  for (const source of files.filter(file => file.startsWith('rust/') && file.endsWith('.rs'))) {
    const file = repositoryFile(base, path.resolve(base, relativeIncludeName(source)));
    // Only explicit file literals extend the curated set. Dynamic macros do
    // not grant permission to enumerate an engines, skills or fixture folder.
    for (const match of fs.readFileSync(file, 'utf8').matchAll(/\binclude_(?:str|bytes)!\s*\(\s*"([^"\r\n]+)"/g)) {
      inputs.add(resolveLiteralInclude(source, match[1], base));
    }
  }
  return [...inputs].sort();
}
function sources() {
  const curated = [...new Set(execFileSync('git', ['ls-files', '-co', '--exclude-standard', '-z'], {cwd: root}).toString('utf8').split('\0'))]
    .filter(file => file && (sourcePattern.test(file) || supplementalInputs.has(file)) && !file.split('/').includes('node_modules') && fs.existsSync(path.join(root, file)))
    .sort();
  return [...new Set([...curated, ...literalRustInputs(curated)])].sort();
}
function verify(candidateFile) {
  const candidate = JSON.parse(fs.readFileSync(candidateFile, 'utf8'));
  const manifestFile = path.resolve(root, candidate.sourceFreeze);
  assert.equal(hash(candidate.binary), candidate.binarySha256, 'Binary changed');
  assert.equal(hash(candidate.boot), candidate.bootSha256, 'Boot changed');
  assert.equal(hash(manifestFile), candidate.sourceFreezeSha256, 'Manifest changed');
  const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
  assert.deepEqual(sources(), manifest.sources.map(row => row.path), 'Product/QA file set changed');
  for (const row of manifest.sources) {
    assert.equal(hash(path.join(root, row.path)), row.sha256, 'Working source changed: ' + row.path);
    assert.equal(hash(path.join(root, row.copy)), row.sha256, 'Frozen source changed: ' + row.path);
  }
  return {candidate: relative(candidateFile), verified: manifest.sources.length, binarySha256: candidate.binarySha256, bootSha256: candidate.bootSha256};
}
function candidateNumber(label) {
  assert(typeof label === 'string' && /^candidate-(0[9]|[1-9]\d{1,5})$/.test(label), 'Use candidate-09 or a later unused label');
  return Number(label.slice('candidate-'.length));
}
function freeze(label) {
  candidateNumber(label);
  fs.mkdirSync(parent, {recursive: true});
  const base = path.join(parent, label); fs.mkdirSync(base, {recursive: false});
  const previous = new Map();
  for (const directory of [previousRoot, parent]) {
    for (const name of fs.readdirSync(directory).filter(name => /^candidate-\d{2,6}$/.test(name)).sort((a, b) => Number(a.slice(10)) - Number(b.slice(10)))) {
      const file = path.join(directory, name, 'source-freeze/manifest.json');
      if (fs.existsSync(file)) for (const row of JSON.parse(fs.readFileSync(file, 'utf8')).sources) previous.set(row.path, row);
    }
  }
  let reused = 0, newBytes = 0;
  const rows = sources().map(file => {
    const original = path.join(root, file), sha256 = hash(original), old = previous.get(file); let copy;
    if (old && old.sha256 === sha256 && fs.existsSync(path.join(root, old.copy)) && hash(path.join(root, old.copy)) === sha256) {
      copy = old.copy; reused++;
    } else {
      const target = path.join(base, 'source-freeze/source', file); fs.mkdirSync(path.dirname(target), {recursive: true});
      fs.copyFileSync(original, target, fs.constants.COPYFILE_EXCL); assert.equal(hash(target), sha256);
      copy = relative(target); newBytes += fs.statSync(target).size;
    }
    return {path: file, copy, sha256};
  });
  assert(rows.some(row => row.path === 'tests/frontend/toolbar.test.mjs'), 'Toolbar regression omitted');
  const manifestFile = path.join(base, 'source-freeze/manifest.json'); fs.mkdirSync(path.dirname(manifestFile), {recursive: true});
  fs.writeFileSync(manifestFile, JSON.stringify({recordedAt: new Date().toISOString(), sources: rows, reused, newBytes}, null, 2) + '\n', {flag: 'wx'});
  const binary = path.join(root, '.cache/community-upgrade/build/debug/readmd.exe'), boot = path.join(root, 'assets/readmd.boot.js');
  const candidate = {recordedAt: new Date().toISOString(), binary, binarySha256: hash(binary), binaryBytes: fs.statSync(binary).size,
    boot, bootSha256: hash(boot), sourceFreeze: relative(manifestFile), sourceFreezeSha256: hash(manifestFile), sourceCount: rows.length,
    gitHead: execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim(),
    offline: true, locked: true, fullBatch3Certification: false, releaseAccepted: false};
  const file = path.join(base, 'candidate.json'); fs.writeFileSync(file, JSON.stringify(candidate, null, 2) + '\n', {flag: 'wx'});
  return {...verify(file), reused, newBytes};
}
if (require.main === module) {
  if (process.argv[2] === '--verify') console.log(JSON.stringify(verify(path.resolve(process.argv[3]))));
  else if (process.argv[2] === '--list') console.log(JSON.stringify({files: sources().length, toolbarIncluded: sources().includes('tests/frontend/toolbar.test.mjs'), nativeExecuted: false}));
  else console.log(JSON.stringify(freeze(process.argv[2])));
}
module.exports = {sources, verify, resolveLiteralInclude, literalRustInputs, candidateNumber};
