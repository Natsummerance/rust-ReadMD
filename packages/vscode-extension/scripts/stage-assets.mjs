import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const vendor = path.resolve(root, '../../assets/vendor');
const target = path.join(root, 'media/vendor');
if (!target.startsWith(root + path.sep)) throw Error('Invalid staging directory');
fs.rmSync(target, {recursive:true,force:true});
const files = [
  ['marked.min.js','marked.min.js'],
  ['katex/dist/katex.min.js','katex/katex.min.js'],
  ['katex/dist/katex.min.css','katex/katex.min.css'],
  ['katex/dist/contrib/auto-render.min.js','katex/auto-render.min.js'],
  ['katex/LICENSE','katex/LICENSE'],
  ['diagrams/mermaid/mermaid.min.js','mermaid.min.js'],
  ['reveal/reveal.min.js','reveal/reveal.min.js'],
  ['reveal/reveal.css','reveal/reveal.css'],
  ['reveal/LICENSE','reveal/LICENSE'],
];
for (const [source, output] of files) {
  const from = path.join(vendor, source);
  if (!fs.statSync(from, { throwIfNoEntry: false })?.isFile()) throw new Error(`Missing vendored preview asset: ${source}`);
  const to = path.join(target, output);
  fs.mkdirSync(path.dirname(to), { recursive: true }); fs.copyFileSync(from, to);
}
fs.cpSync(path.join(vendor, 'katex/dist/fonts'), path.join(target, 'katex/fonts'), { recursive: true });
fs.cpSync(path.join(vendor, 'reveal/theme'), path.join(target, 'reveal/theme'), { recursive: true });
for (const name of fs.readdirSync(path.join(target, 'reveal/theme')).filter(name => name.endsWith('.css'))) {
  const file = path.join(target, 'reveal/theme', name);
  // Vendored themes retain system font fallbacks without online Google Fonts.
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace(/@import\s+url\(([^)]+)\);/g, (match, url) => {
    const local = url.replace(/^["']|["']$/g, '');
    return /^https?:/i.test(local) || !fs.existsSync(path.resolve(path.dirname(file), local)) ? '' : match;
  }));
}
for (const engine of ['wavedrom', 'bitfield', 'viz', 'vega', 'vega-lite', 'chart', 'tikzjax']) {
  fs.cpSync(path.join(vendor, 'diagrams', engine), path.join(target, 'diagrams', engine), { recursive: true });
}
fs.copyFileSync(path.join(vendor, 'diagrams/VERSIONS.md'), path.join(target, 'diagrams/VERSIONS.md'));
console.log('Staged local preview assets; no downloads.');
