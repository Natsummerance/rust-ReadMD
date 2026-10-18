// Explicit offline maintainer lock generation, not part of a build. Changing
// the reviewed snapshot requires updating this input digest deliberately.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { parseTlpdb, dependencyClosure } from './lib/tex-package-inputs.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const folder = path.join(root, '.cache/capability-upgrade/tex-inputs');
const input = fs.readFileSync(path.join(folder, 'metadata/yihui-texlive.tlpdb'));
const digest = 'aa5fdf664eed073cbe9fe5a06da33d61d1006946914ded0a26c49a9b23f7b7cc';
if (crypto.createHash('sha256').update(input).digest('hex') !== digest) throw Error('Unreviewed TeX package database');
const pin = JSON.parse(fs.readFileSync(path.join(root, 'tools/tex-runtime-inputs.lock.json')));
const inventory = JSON.parse(fs.readFileSync(path.join(folder, 'inventory.json')));
if (JSON.stringify(inventory.archive) !== JSON.stringify(pin.archive)) throw Error('Base input identity differs');
const database = fs.readFileSync(path.join(folder, 'TinyTeX/tlpkg/texlive.tlpdb'));
const expected = inventory.files.find(f => f.path === 'tlpkg/texlive.tlpdb');
if (!expected || crypto.createHash('sha256').update(database).digest('hex') !== expected.sha256) throw Error('Base package database changed');
const available = parseTlpdb(input), installed = parseTlpdb(database);
const additions = dependencyClosure(available, installed, ['ctex']);
const components = [...installed.values()].filter(p => !p.name.startsWith('00texlive.')).map(p => {
  const current = available.get(p.name);
  const containers = {}, unresolvedMaterials = [];
  for (const kind of ['source', 'doc']) {
    const a = current?.containers[kind], b = p.containers[kind];
    if (!b) continue;
    if (a?.sha512 !== b.sha512 || a?.size !== b.size) { unresolvedMaterials.push({ kind, ...b }); continue; }
    containers[kind] = a;
  }
  // Installed databases omit source/doc file sections. Matching revision and
  // container digest bind the full upstream lists to these installed bytes.
  return { ...p, base: true, containers, ...(unresolvedMaterials.length ? { unresolvedMaterials } : {}) };
}).concat(additions.map(p => ({ ...p, base: false })));
const lock = { schema: 1, id: 'latex-publication-inputs', platform: pin.platform, baseArchiveSha256: pin.archive.sha256,
  metadata: { url: 'https://tlnet.yihui.org/tlpkg/texlive.tlpdb', size: input.length, sha256: digest },
  archiveOrigin: 'https://ctan.math.illinois.edu/systems/texlive/tlnet/archive/',
  scope: 'Maintainer-only pinned Chinese dependency closure and corresponding package sources/notices. Binary corresponding sources and redistribution review remain separate; not an application catalog.',
  redistributionApproved: false, roots: ['ctex'], additionalPackages: additions.length, components };
fs.writeFileSync(path.join(root, 'tools/tex-publication-inputs.lock.json'), JSON.stringify(lock, null, 2) + '\n');
const sizes = { run: 0, source: 0, doc: 0 };
for (const p of components) for (const [kind, c] of Object.entries(p.containers)) sizes[kind] += c.size;
console.log(JSON.stringify({ offline: true, additionalPackages: additions.length, components: components.length, compressedBytes: sizes, unresolvedMaterials: components.filter(p => p.unresolvedMaterials).map(p => ({ name: p.name, kinds: p.unresolvedMaterials.map(m => m.kind) })), redistributionApproved: false }));
