// Offline VSIX packager: Node built-ins only, no npm downloads or Python.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { zip } from '../../../tools/lib/zip.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const escape = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&apos;'}[c]));
const english = JSON.parse(fs.readFileSync(path.join(root, 'package.nls.json'), 'utf8'));
const nls = value => String(value).replace(/^%(.+)%$/, (_, key) => english[key] || key);
const entries = [];
const add = (name, data, mode = 0o644) => entries.push({ name, data: Buffer.isBuffer(data) ? data : Buffer.from(data), mode });
const walk = relative => {
  const base = path.join(root, relative);
  for (const item of fs.readdirSync(base, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name, 'en'))) {
    const child = path.posix.join(relative, item.name);
    if (item.isSymbolicLink()) throw Error('Package cannot contain symlinks: ' + child);
    if (item.isDirectory()) walk(child);
    else if (item.isFile() && !child.endsWith('.map')) add('extension/' + child, fs.readFileSync(path.join(root, child)), child === 'core/bin/readmd' ? 0o755 : 0o644);
  }
};
for (const required of ['out/extension.js','out/webview.js','media/preview.js','media/diagram-frame.js','media/vendor/marked.min.js','media/vendor/mermaid.min.js','media/vendor/reveal/theme/fonts/source-sans-pro/source-sans-pro.css']) {
  if (!fs.statSync(path.join(root, required), { throwIfNoEntry: false })?.isFile()) throw Error('Compile and stage assets before packaging: ' + required);
}
for (const file of ['package.json','package.nls.json','package.nls.zh-cn.json','README.md','LICENSE','icon.png']) add('extension/' + file, fs.readFileSync(path.join(root, file)));
for (const directory of ['out','l10n','media','snippets']) walk(directory);
if (fs.existsSync(path.join(root, 'core/bin'))) walk('core');
const manifest = `<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
<Metadata><Identity Language="en-US" Id="${escape(pkg.name)}" Version="${escape(pkg.version)}" Publisher="${escape(pkg.publisher)}"/><DisplayName>${escape(nls(pkg.displayName))}</DisplayName><Description xml:space="preserve">${escape(nls(pkg.description))}</Description><Tags>${escape(pkg.keywords.join(','))}</Tags><Categories>${escape(pkg.categories.join(','))}</Categories><Properties><Property Id="Microsoft.VisualStudio.Code.Engine" Value="${escape(pkg.engines.vscode)}"/><Property Id="Microsoft.VisualStudio.Code.ExtensionDependencies" Value=""/><Property Id="Microsoft.VisualStudio.Code.ExtensionPack" Value=""/><Property Id="Microsoft.VisualStudio.Code.ExtensionKind" Value="workspace"/><Property Id="Microsoft.VisualStudio.Services.Links.Source" Value="${escape(pkg.repository.url)}"/></Properties><License>extension/LICENSE</License><Icon>extension/icon.png</Icon></Metadata>
<Installation><InstallationTarget Id="Microsoft.VisualStudio.Code"/></Installation><Dependencies/><Assets><Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/><Asset Type="Microsoft.VisualStudio.Services.Content.Details" Path="extension/README.md" Addressable="true"/><Asset Type="Microsoft.VisualStudio.Services.Content.License" Path="extension/LICENSE" Addressable="true"/><Asset Type="Microsoft.VisualStudio.Services.Icons.Default" Path="extension/icon.png" Addressable="true"/></Assets></PackageManifest>`;
add('extension.vsixmanifest', manifest);
const types = { json:'application/json',js:'application/javascript',css:'text/css',html:'text/html',md:'text/markdown',txt:'text/plain',png:'image/png',svg:'image/svg+xml',woff:'font/woff',woff2:'font/woff2',ttf:'font/ttf',eot:'application/vnd.ms-fontobject',wasm:'application/wasm',gz:'application/gzip',exe:'application/octet-stream',vsixmanifest:'text/xml' };
const extensions = new Set(entries.map(entry => path.posix.extname(entry.name).slice(1)).filter(Boolean));
add('[Content_Types].xml', '<?xml version="1.0" encoding="utf-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' + [...extensions].sort().map(extension => `<Default Extension="${escape(extension)}" ContentType="${types[extension] || 'application/octet-stream'}"/>`).join('') + entries.filter(entry => !path.posix.extname(entry.name)).map(entry => `<Override PartName="/${escape(entry.name)}" ContentType="text/plain"/>`).join('') + '</Types>');
const output = path.resolve(process.argv[2] || path.join(root, `readmd-vscode-${pkg.version}.vsix`));
fs.mkdirSync(path.dirname(output), {recursive:true});
const temporary = output + '.' + process.pid + '.tmp'; fs.writeFileSync(temporary, zip(entries), {flag:'wx'}); fs.renameSync(temporary,output);
console.log(`VSIX: ${output} (${entries.length} files; no source, tests or dependencies)`);
