#!/usr/bin/env node
// Zero-dependency privacy gate: fail when retired private AI seeds, likely
// plaintext API keys, hard-coded developer paths or real personal documents
// appear in the repository (tracked + non-ignored files), or in the release
// artifacts passed as arguments.
// Usage: node tools/privacy-scan.mjs [artifact-file-or-dir ...]
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// Split so this file does not match itself.
export const RETIRED = ['cc' + '-switch', 'xem' + '8k5', 'hot' + 'api', 'penguins' + 'aichat'];
export const KEY_PATTERNS = [/\bsk-[A-Za-z0-9_-]{20,}\b/, /\bAIza[0-9A-Za-z_-]{30,}\b/];
export const LOCAL_PATH_PATTERNS = [
  /[a-zA-Z]:[/\\](?:users|programming|project|workspace|home|desktop|downloads|natsumer|[a-zA-Z0-9_.-]+[/\\]\.(?:codex|gemini|antigravity))/i,
  /[a-zA-Z]:[/\\][a-zA-Z0-9_.-]+[/\\](?:skills|plugins|creator)[/\\]/i,
  /\/(?:Users|home)\/[a-zA-Z0-9_.-]+\/(?:\.codex|\.gemini|Programming|Projects)/i,
];
// Real personal documents must never enter the repo; a filename hit fails
// before any other early return.
export const SENSITIVE_NAME_PATTERNS = [/北京交通大学软件学院毕业实习文档/];

// Test fixtures that spell out obviously fake key-shaped strings.
const FAKE_KEY_FILES = new Set(['rust/readmd-kernel/src/ai.rs', 'rust/readmd-kernel/src/crypto.rs']);
// Test fixtures that use a synthetic install root (`C:/app/plugins/...`).
const FAKE_PATH_FILES = new Set(['rust/readmd-kernel/src/pet_host.rs']);
// The scanner's own pattern source and its unit-test fixtures.
const SELF = new Set(['tools/privacy-scan.mjs', 'tools/test/privacy-scan.test.mjs']);
// The public provider-catalog attribution is intentional and audited.
const CC_ATTRIBUTION = [
  'assets/providers/provider-catalog.json', 'provider-catalog.json', 'tools/check-assets.mjs',
  'tools/privacy-scan.mjs', 'source-snapshot-manifest.json', 'THIRD_PARTY_LICENSES.md',
  'candidate.json', 'SHA256SUMS.txt',
];
const SKIP_EXT = [
  '.png', '.ico', '.icns', '.lock', '.svg', '.woff', '.woff2', '.ttf', '.eot',
  '.exe', '.dll', '.pyd', '.pyc', '.dylib', '.so', '.zip', '.gz', '.bin', '.dat', '.obj', '.o', '.a',
  '.node', '.vsix', '.hap', '.deb', '.appimage', '.tar', '.xz', '.bz2', '.7z', '.pak', '.dmg',
  '.strings', '.nib', '.storyboardc', '.db', '.sqlite', '.sqlite3', '.jpg', '.jpeg', '.webp', '.gif',
  '.mp4', '.webm', '.pdf', '.docx', '.xlsx', '.pptx', '.epub',
];

const under = (label, dir) => label.startsWith(dir + '/') || label.includes('/' + dir + '/');

const NATIVE_EXT = /\.(?:exe|dll|pyd|dylib|so(?:\.\d+)*|node|a|lib|rlib|o|obj)$/i;
const NATIVE_MAGIC = new Set([
  0x7f454c46, // ELF, including extensionless Linux executables.
  0xfeedface, 0xcefaedfe, 0xfeedfacf, 0xcffaedfe, // Mach-O.
  0xcafebabe, 0xbebafeca, 0xcafebabf, 0xbfbafeca, // Universal Mach-O.
]);
function nativeBinary(label, data) {
  return NATIVE_EXT.test(label) || (data.length >= 2 && data[0] === 0x4d && data[1] === 0x5a) ||
    (data.length >= 4 && NATIVE_MAGIC.has(data.readUInt32BE(0))) ||
    (data.length >= 8 && ['!<arch>\n', '!<thin>\n'].includes(data.toString('ascii', 0, 8)));
}

function normalizeBuildRoot(value) {
  if (typeof value !== 'string' || value.length < 4 || value !== value.trim() || /[\x00-\x1f\x7f]/.test(value)) return null;
  let normalized = value.replaceAll('\\', '/').replace(/^\/\/\?\/UNC\//i, '//').replace(/^\/\/\?\//, '');
  if (/^[a-z]:\//i.test(normalized)) normalized = path.win32.normalize(normalized).replaceAll('\\', '/');
  else if (normalized.startsWith('//')) {
    if (!/^\/\/[^/]+\/[^/]+(?:\/|$)/.test(normalized)) return null;
    normalized = path.win32.normalize(normalized).replaceAll('\\', '/');
  } else if (normalized.startsWith('/')) normalized = path.posix.normalize(normalized);
  else return null;
  normalized = normalized.replace(/\/+$/, '');
  return normalized.length >= 4 ? normalized : null;
}

function defaultBuildRoots() {
  const values = [root, os.homedir(), process.env.CARGO_HOME, process.env.RUSTUP_HOME, process.env.GITHUB_WORKSPACE];
  return values.flatMap(value => {
    if (!normalizeBuildRoot(value)) return [];
    const variants = [value];
    if (path.isAbsolute(value)) {
      variants.push(path.resolve(value));
      try { variants.push(fs.realpathSync.native(value)); } catch { /* An absent configured root still has its lexical form. */ }
    }
    return variants;
  });
}

function buildRootPatterns(values) {
  const roots = new Set((Array.isArray(values) ? values : []).map(normalizeBuildRoot).filter(Boolean));
  return [...roots].map(value => {
    let body = value.split('/').map(part => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('[\\\\/]+');
    if (/^[a-z]:\//i.test(value)) body = String.raw`(?:[\\/]{2}\?[\\/])?` + body;
    // Match a directory boundary, not a similarly named directory or an
    // identifier containing the root. Neither patterns nor matches are logged.
    return new RegExp('(?:^|[^a-z0-9_./\\\\:-])' + body + '(?=$|[\\\\/\\s\\x00\"\'<>():;,\\[\\]{}=])', 'i');
  });
}

/** Inspect only native executables/libraries; never echo matched binary bytes. */
export function scanNativePaths(label, data, { buildRoots } = {}) {
  if (!nativeBinary(label, data)) return [];
  const failure = [`embedded developer absolute path in native binary ${label}`];
  const roots = buildRootPatterns(buildRoots === undefined ? defaultBuildRoots() : buildRoots);
  const matches = text => LOCAL_PATH_PATTERNS.some(pattern => pattern.test(text)) || roots.some(pattern => pattern.test(text));
  if (matches(data.toString('latin1'))) return failure;
  // Embedded wide strings can start at either byte alignment. Check both byte
  // orders without stripping NULs, which would manufacture paths in image data.
  const bigEndian = new TextDecoder('utf-16be');
  for (const offset of [0, 1]) {
    if (matches(data.toString('utf16le', offset)) || matches(bigEndian.decode(data.subarray(offset)))) return failure;
  }
  return [];
}

/** Scan one file's bytes. Returns a list of failure strings. */
export function scanBuffer(label, data, { sourceTree = true, buildRoots } = {}) {
  label = label.replace(/\\/g, '/');
  const failures = [];
  if (SENSITIVE_NAME_PATTERNS.some(re => re.test(label))) return [`sensitive real-document name in ${label}`];
  // Native artifacts must be checked even in dist, vendored directories and
  // macOS Contents/Frameworks, or when an external artifact is being scanned.
  failures.push(...scanNativePaths(label, data, { buildRoots }));
  if (['assets/vendor', 'assets/upstream', 'tests', 'ui-tests', 'showcase'].some(d => under(label, d))) return failures;
  if (label.includes('verify-macos') || under(label, 'Contents') || label.startsWith('Contents/') || under(label, 'Frameworks')) return failures;
  const lower = label.toLowerCase();
  if (SKIP_EXT.some(ext => lower.endsWith(ext)) || label.endsWith('ReadMD')) return failures;
  const text = data.toString('latin1'); // byte-exact for ASCII patterns
  const low = text.toLowerCase();
  const attributed = CC_ATTRIBUTION.some(n => label === n || label.endsWith('/' + n));
  for (const token of RETIRED) {
    if (token === 'cc' + '-switch' && attributed) continue;
    if (low.includes(token)) failures.push(`retired provider marker in ${label}`);
  }
  if (!FAKE_KEY_FILES.has(label) && KEY_PATTERNS.some(re => re.test(text))) failures.push(`possible plaintext API key in ${label}`);
  if (sourceTree && !FAKE_PATH_FILES.has(label) && !SELF.has(label) &&!under(label, 'dist') && !label.startsWith('dist/')) {
    if (LOCAL_PATH_PATTERNS.some(re => re.test(text))) failures.push(`hardcoded local absolute path in ${label}`);
  }
  return failures;
}

function candidateFiles() {
  const raw = execFileSync('git', ['ls-files', '-co', '--exclude-standard', '-z'], { cwd: root });
  return raw.toString('utf8').split('\0').filter(Boolean);
}

function* walkExternal(target) {
  const abs = path.resolve(target);
  if (!fs.existsSync(abs)) return;
  if (fs.statSync(abs).isFile()) { yield [abs, path.basename(abs)]; return; }
  const stack = [abs];
  while (stack.length) {
    const dir = stack.pop();
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      if (['node_modules', '.git', '__pycache__', '_internal'].includes(e.name)) continue;
      const full = path.join(dir, e.name);
      if (e.isDirectory()) stack.push(full);
      else if (e.isFile()) yield [full, path.relative(abs, full)];
    }
  }
}

function main(args) {
  const failures = new Set();
  let count = 0;
  const entries = args.length
    ? args.flatMap(a => [...walkExternal(a)]).map(([p, l]) => [p, l, false])
    : candidateFiles().map(r => [path.join(root, r), r, true]);
  for (const [file, label, sourceTree] of entries) {
    let data;
    try { data = fs.readFileSync(file); } catch { continue; }
    count++;
    for (const f of scanBuffer(label, data, { sourceTree })) failures.add(f);
  }
  if (failures.size) {
    console.error('PRIVACY SCAN FAILED');
    for (const f of [...failures].sort()) console.error(f);
    return 1;
  }
  console.log(`privacy scan PASSED (${count} files)`);
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
