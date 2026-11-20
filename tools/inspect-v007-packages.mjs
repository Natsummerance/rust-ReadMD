// Check bytes shipped by the bound build, without executing foreign binaries.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {inspectPlatformPackages} from './inspect-p0-platform-packages.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const run = (program, args, options = {}) => execFileSync(program, args, {maxBuffer: 256 * 1024 * 1024, ...options});
export function embeddedTemplateDigests(binary, templates) {
  const result = {};
  for (const [name, bytes] of templates) {
    if (!bytes.length || binary.indexOf(bytes) < 0) throw Error('Executable does not embed the exact source template: ' + name);
    result[name] = sha(bytes);
  }
  return {embeddedTemplates: result};
}
export function inspectV007Packages({directory, sourceCommit, runId, scratch, macReports}) {
  const original = file => run('git', ['show', sourceCommit + ':' + file], {cwd: root});
  if (original('VERSION').toString('utf8').trim() !== '0.0.7') throw Error('Expected the V0.0.7 product source');
  const templateNames = ['confluence', 'lightfield', 'pearl', 'chrome', 'field'];
  const additionalAssets = ['assets/css/tokens.css', 'assets/css/export-html-layouts.css', 'assets/data/export-templates.json',
    'assets/js/reader/export-html-layout.js', 'assets/js/features/export-html-preview.js',
    ...templateNames.map(name => 'assets/data/slide-covers/' + name + '.jpg')];
  const templates = new Map(templateNames.map(name => [name, original('rust/readmd-kernel/templates/pptx/' + name + '.pptx')]));
  const proof = inspectPlatformPackages({directory, sourceCommit, buildRunId: runId, scratch, additionalAssets,
    verifyBinary: binary => embeddedTemplateDigests(binary, templates)});
  const assetNames = ['assets/index.html', 'assets/readmd.boot.js', 'assets/js/core/lifecycle.js',
    'assets/css/shell.css', 'assets/css/dialogs.css', 'assets/css/reader.css',
    'assets/providers/provider-catalog.json', 'assets/i18n/zh-CN.json', ...additionalAssets];
  for (const arch of ['arm64', 'x64']) {
    const name = 'ReadMD-macos-' + arch + '.zip', file = path.join(directory, name);
    const read = member => run('unzip', ['-p', file, 'ReadMD.app/Contents/' + member]);
    const assetSha256 = {};
    for (const asset of assetNames) {
      const bytes = read('Resources/' + asset);
      if (!bytes.equals(original(asset))) throw Error('Mac package source asset mismatch: ' + name + ' / ' + asset);
      assetSha256[asset] = sha(bytes);
    }
    const binary = read('MacOS/ReadMD');
    if (binary.length < 32 || binary.readUInt32LE(0) !== 0xfeedfacf || binary.readUInt32LE(4) !== (arch === 'arm64' ? 0x100000c : 0x1000007)) throw Error('Mac executable architecture differs: ' + arch);
    const binarySha256 = sha(binary), embedded = embeddedTemplateDigests(binary, templates);
    const plist = read('Info.plist').toString('utf8');
    if (!/<key>CFBundleShortVersionString<\/key>\s*<string>0\.0\.7<\/string>/.test(plist)) throw Error('Mac bundle version differs');
    const report = macReports?.[arch];
    if (!report || report.version !== '0.0.7' || report.arch !== arch || report.bootSha256 !== assetSha256['assets/readmd.boot.js']
        || report.binarySha256 !== binarySha256 || !/^[a-f\d]{64}$/.test(report.sourceBinarySha256)
        || report.signing?.mode !== 'development' || report.signing.notarized !== false) throw Error('Mac CI packaging report is not bound to the inspected source and signed executable');
    for (const extension of ['zip', 'dmg']) {
      const packageName = 'ReadMD-macos-' + arch + '.' + extension, packageFile = path.join(directory, packageName);
      const size = fs.statSync(packageFile).size, sha256 = sha(fs.readFileSync(packageFile));
      const reported = report.files?.filter(file => file.name === packageName);
      if (reported?.length !== 1 || reported[0].size !== size || reported[0].sha256 !== sha256) throw Error('Mac ZIP/DMG differs from the same-stage CI packaging report');
      proof.packages.push({name: packageName, size, sha256, binarySha256, assetSha256, ...embedded,
        archiveInspected: extension === 'zip', sameAppStageVerified: true, signatureCheckedByNativeCI: true,
        nativeHostCertified: false, buildRunId: runId});
    }
    proof.checks.push({name, sourceAssetsMatch: true, executableArchitecture: arch === 'arm64' ? 'Mach-O arm64' : 'Mach-O x86_64',
      exactEmbeddedTemplates: templateNames, sameStageDmgSha256: report.files.find(file => file.name.endsWith('.dmg')).sha256,
      nativeHostCertified: false});
  }
  proof.allTenPackagesFromRun = runId;
  return proof;
}
