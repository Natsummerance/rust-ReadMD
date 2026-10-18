// Offline catalogue, derived from the already bundled MathJax character maps
// (Apache-2.0, see vendor notices) and the build runtime's Unicode Math property.
// No downloads, packages, font installations or document rewrites.
import fs from 'node:fs';
import crypto from 'node:crypto';
if (process.versions.unicode !== '16.0') throw Error('Use a Node runtime with Unicode 16.0 to reproduce this catalogue.');
const source = fs.readFileSync('assets/vendor/mathjax/tex-svg.js', 'utf8');
const aliases = new Map();
for (const match of source.matchAll(/CharacterMap\("([^"\n]*mathchar[^"\n]*)",[^,]+,\{/g)) {
  const start = match.index + match[0].length - 1;
  let depth = 0, quoted = false, escaped = false, end = start;
  for (; end < source.length; end++) {
    const ch = source[end];
    if (quoted) { if (escaped) escaped = false; else if (ch === '\\') escaped = true; else if (ch === '"') quoted = false; }
    else if (ch === '"') quoted = true;
    else if (ch === '{') depth++;
    else if (ch === '}' && --depth === 0) break;
  }
  for (const item of source.slice(start, end).matchAll(/(?:^|[,\{])([A-Za-z]+):\[?("(?:\\.|[^"\\])*")/g)) {
    const glyph = JSON.parse(item[2].replace(/\\x([\da-f]{2})/gi, '\\u00$1'));
    if ([...glyph].length !== 1) continue;
    const names = aliases.get(glyph) || [];
    if (!names.includes(item[1])) names.push(item[1]);
    aliases.set(glyph, names);
  }
}
const symbols = [];
for (let cp = 0; cp <= 0x10ffff; cp++) {
  const symbol = String.fromCodePoint(cp);
  if (!/\p{Math}/u.test(symbol) && !(cp >= 0x391 && cp <= 0x3f5 && /\p{Letter}/u.test(symbol)) && !aliases.has(symbol)) continue;
  if (/\p{Default_Ignorable_Code_Point}/u.test(symbol)) continue;
  const names = aliases.get(symbol) || [];
  const category = cp >= 0x370 && cp <= 0x3ff ? 'greek'
    : cp >= 0x1d400 && cp <= 0x1d7ff || cp >= 0x2100 && cp <= 0x214f ? 'letters'
    : cp >= 0x2190 && cp <= 0x21ff || cp >= 0x27f0 && cp <= 0x27ff || cp >= 0x2900 && cp <= 0x297f ? 'arrows'
    : /equal|sim|approx|cong|subset|supset|less|greater|leq|geq|prec|succ|in$|ni$/.test(names.join(' ')) ? 'relations' : 'operators';
  const code = 'U+' + cp.toString(16).toUpperCase().padStart(4, '0');
  // The glyph is a portable source spelling for symbols without a TeX alias.
  // TeX syntax characters need their usual literal escapes.
  const tex = names.length ? '\\' + names[0] : /[#$%&_{}]/.test(symbol) ? '\\' + symbol : symbol;
  symbols.push({ id: code, symbol, tex, aliases: names, category });
}
const templates = [
  ['fraction', 'f', '\\frac{a}{b}'], ['root', 'rad', '\\sqrt[n]{x}'], ['squareRoot', 'rad', '\\sqrt{x}'],
  ['scripts', 'sSubSup', 'x_{i}^{n}'], ['superscript', 'sSup', 'x^{n}'], ['subscript', 'sSub', 'x_{i}'],
  ['delimiters', 'd', '\\left(\\frac{a}{b}\\right)'], ['matrix', 'm', '\\begin{bmatrix}a&b\\\\c&d\\end{bmatrix}'],
  ['cases', 'eqArr', 'f(x)=\\begin{cases}x,&x\\ge0\\\\-x,&x<0\\end{cases}'],
  ['aligned', 'eqArr', '\\begin{aligned}a&=b+c\\\\d&=e+f\\end{aligned}'],
  ['sum', 'nary', '\\sum_{i=1}^{n}x_i'], ['integral', 'nary', '\\int_{a}^{b}f(x)\\,dx'],
  ['limit', 'limLow', '\\lim_{x\\to0}f(x)'], ['accent', 'acc', '\\hat{x}'],
  ['bar', 'bar', '\\overline{x}'], ['boxed', 'borderBox', '\\boxed{x}'],
  ['quadratic', 'f', 'x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}'],
  ['binomial', 'f', '\\binom{n}{k}'], ['vector', 'acc', '\\vec{x}'],
].map(([id, omml, tex]) => ({ id, omml, tex, category: 'structures', label: 'formula.template.' + id }));
for (const name of 'sin cos tan cot sec csc arcsin arccos arctan sinh cosh tanh ln log lg exp det dim ker deg gcd hom inf sup lim max min'.split(' ')) {
  templates.push({ id: 'function.' + name, omml: 'func', tex: '\\' + name + '{x}', category: 'functions', label: 'formula.template.function', labelArgs: { name } });
}
const result = { schema: 1, unicodeVersion: process.versions.unicode,
  derivation: 'Unicode Math property, Greek letters and bundled MathJax mathchar maps; not a claim of complete OMML layout compatibility.',
  mathjaxSha256: crypto.createHash('sha256').update(source).digest('hex'), templates, symbols };
const output = JSON.stringify(result) + '\n';
if (process.argv.includes('--check')) {
  if (fs.readFileSync('assets/math-catalog.json', 'utf8') !== output) throw Error('Math catalogue is stale. Run node tools/build-math-catalog.mjs offline.');
} else fs.writeFileSync('assets/math-catalog.json', output);
console.log(JSON.stringify({ symbols: symbols.length, aliases: symbols.reduce((n, s) => n + s.aliases.length, 0), templates: templates.length, unicodeVersion: result.unicodeVersion }));
