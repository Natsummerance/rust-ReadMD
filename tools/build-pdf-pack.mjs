// Assemble the reviewed library and current application worker offline.
import fs from 'node:fs';
import path from 'node:path';
import {verifyLocalPack, sha256} from './lib/capability-publication.mjs';
import {registerRecoveryLock} from './lib/capability-recovery.mjs';

const [engine, worker, output, platform = 'windows-x64', bundleFlag, bundleAssets] = process.argv.slice(2);
if (!engine || !worker || !output || (bundleFlag !== undefined && (bundleFlag !== '--bundle-assets' || !bundleAssets)) || process.argv.length > 8) {
  throw Error('Usage: node tools/build-pdf-pack.mjs ENGINE_DIR WORKER OUTPUT.readmd-pack [PLATFORM [--bundle-assets ASSETS_DIR]]');
}
const libraries = {
  'windows-x64': ['pdfium.dll', 'bin', 'readmd-pdf-worker.exe'],
  'linux-x64': ['libpdfium.so', 'lib', 'readmd-pdf-worker'],
  'linux-arm64': ['libpdfium.so', 'lib', 'readmd-pdf-worker'],
  'macos-x64': ['libpdfium.dylib', 'lib', 'readmd-pdf-worker'],
  'macos-arm64': ['libpdfium.dylib', 'lib', 'readmd-pdf-worker'],
};
if (!Object.hasOwn(libraries, platform)) throw Error('Unsupported PDFium pack platform');

const files = [], payload = {};
function add(name, source) {
  const bytes = fs.readFileSync(source);
  payload[name] = bytes.toString('base64');
  files.push({path: name, size: bytes.length, sha256: sha256(bytes)});
}
const [library, libraryDir, workerName] = libraries[platform];
add('bin/' + library, path.join(engine, libraryDir, library));
add('bin/' + workerName, worker);
if (platform === 'windows-x64' && files[0].sha256 !== '79d4676b656cfb1abcea88f9ade3b4b0826c5200382db5f4ec72a636c598c118') {
  throw Error('Unexpected PDFium binary');
}
for (const name of ['LICENSE','VERSION','args.gn']) add(name, path.join(engine, name));
for (const name of fs.readdirSync(path.join(engine,'licenses')).sort()) {
  add('licenses/' + name, path.join(engine,'licenses',name));
}
const crate = process.env.READMD_PDFIUM_CRATE, registry = process.env.READMD_REGISTRY_SRC;
if (!crate || !registry) throw Error('Point READMD_PDFIUM_CRATE and READMD_REGISTRY_SRC to the locked PDFium Cargo sources');
add('licenses/pdfium-render-NOTICE.md', path.join(crate,'LICENSE.md'));
add('licenses/pdfium-render-APACHE-2.0.txt','assets/licenses/pdfium-render-APACHE-2.0.txt');
for (const name of ['bytemuck-1.25.2','byteorder-1.5.0','itertools-0.15.0','libloading-0.9.0','maybe-owned-0.3.4','piston-float-1.0.1','utf16string-0.2.0','vecmath-1.0.0']) {
  const directory = path.join(registry, name);
  for (const file of fs.readdirSync(directory).sort().filter(n => /^(LICEN[SC]E|COPYING)/i.test(n) && fs.statSync(path.join(directory,n)).isFile())) {
    add('licenses/' + name + '-' + file, path.join(directory,file));
  }
}
const manifest = {schema: 1, id: 'pdfium', version: '7881', platform, files};
const bytes = Buffer.from(JSON.stringify({manifest, payload}));
const archiveSha = sha256(bytes);
let catalog, entry;
const assets = bundleAssets || 'assets', caps = path.join(assets, 'capabilities');
const lockFile = path.join(caps, 'pdfium-7881-' + platform + '.lock.json');
if (bundleAssets) {
  // Only a packaging copy gets the new lock and catalog. Existing online
  // downloads retain the repository's reviewed bytes and original URL.
  if (fs.realpathSync(assets) === fs.realpathSync('assets')) throw Error('Supply a staging copy of assets');
  const catalogFile = path.join(caps, 'catalog.json');
  catalog = JSON.parse(fs.readFileSync(catalogFile, 'utf8'));
  if (catalog.schema !== 1 || catalog.repository !== 'Natsummerance/tool-ReadMD' || !Array.isArray(catalog.packs)) throw Error('Invalid staged capability catalog');
  entry = {
    id: manifest.id, version: manifest.version, platform, delivery: 'bundled',
    labelKey: 'capability.pdfium.name', descriptionKey: 'capability.pdfium.purpose', features: ['pdf_editor'],
    downloadBytes: bytes.length, installedBytes: files.reduce((size, file) => size + file.size, 0),
    sha256: archiveSha, license: 'BSD-3-Clause; third-party notices included',
  };
  verifyLocalPack(bytes, manifest, entry);
  if (fs.existsSync(lockFile)) registerRecoveryLock(assets, lockFile);
  catalog.packs = catalog.packs.filter(pack => pack.id !== manifest.id || pack.platform !== platform);
  catalog.packs.unshift(entry);
}
fs.mkdirSync(caps, {recursive: true});
fs.mkdirSync(path.dirname(output), {recursive: true});
fs.writeFileSync(output, bytes);
if (bundleAssets) {
  const name = 'ReadMD-PDFium-' + platform + '.readmd-pack';
  const legacyLocation = platform.startsWith('linux-') && path.resolve(output) === path.resolve(assets, '..', name);
  if (!legacyLocation) {
    const folder = path.join(caps, 'bundled');
    fs.mkdirSync(folder, {recursive: true});
    const destination = path.join(folder, name);
    if (path.resolve(output) !== path.resolve(destination)) fs.copyFileSync(output, destination);
  }
  fs.writeFileSync(path.join(caps, 'catalog.json'), JSON.stringify(catalog, null, 2) + '\n');
}
fs.writeFileSync(lockFile, JSON.stringify(manifest, null, 2) + '\n');
console.log(JSON.stringify({pack: output, size: bytes.length, sha256: archiveSha, files: files.length, bundled: !!bundleAssets}));
