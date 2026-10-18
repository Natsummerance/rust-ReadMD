import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const context={window:{},document:{readyState:'loading',addEventListener(){}}};vm.createContext(context);
vm.runInContext(fs.readFileSync('assets/js/editor/formula-studio.js','utf8'),context);
const {ranges,tokens,annotate}=context.window.ReadMDFormula;
test('formula offsets distinguish occurrences and ignore fenced, escaped and inline code',()=>{
  const src='😀 $x$ and \\(y\\)\n\n```md\n$fake$\n```\n\n`$fake$` \\$fake\\$\n\n$$\\frac{a}{b}$$';
  const found=ranges(src);assert.deepEqual(Array.from(found,r=>src.slice(r.from,r.to)),['$x$','\\(y\\)','$$\\frac{a}{b}$$']);
  for(const r of found)assert.equal(src.slice(r.bodyFrom,r.bodyTo),r.tex);
  assert.equal(ranges('price $5 and $6, not math').length,0);
});
test('hidden metadata, HTML attributes and raw code cannot steal the identity of a visible formula',()=>{
  const src='---\ntitle: "$x$"\n---\n<!-- $x$ -->\n<code>$x$</code> <a title="$x$">Link</a>\n\nVisible $x$.';
  const found=ranges(src);assert.equal(found.length,1);assert.equal(found[0].from,src.lastIndexOf('$x$'));
  assert.equal(ranges('<!-- unclosed $x$').length,0);
});
test('symbol source mapping preserves structural commands, text and supplementary scalars',()=>{
  const src='\\frac{α_i}{\\theta^2}+𝛂+\\text{速度}';const found=tokens(src);
  assert.deepEqual(Array.from(found,t=>t.tex),['α','i','\\theta','2','+','𝛂','+','\\text{速度}']);
  for(const t of found)assert.equal(src.slice(t.from,t.to),t.tex);
  const mapped=annotate(src,found,'safe-');assert.ok(mapped.startsWith('\\frac{'));assert.ok(mapped.includes('\\cssId{safe-2}{\\theta}'));
  assert.equal(tokens('\\left\\{x\\right\\}')[0].tex,'x');
});
test('hidden math properties are excluded from prose counts without losing source or code examples',()=>{
  const {metadataRanges,withoutMetadata}=context.window.ReadMDFormula;
  const note='<!--readmd:math:v1:YWJjZA==-->',src='😀 $x$'+note+'\n\n$y$ \n'+note+'\n\n```md\n$z$'+note+'\n```\n\n'+note;
  const found=metadataRanges(src);
  assert.deepEqual(Array.from(found,r=>src.slice(r.from,r.to)),[note,note]);
  assert.equal(withoutMetadata(src),'😀 $x$\n\n$y$ \n\n\n```md\n$z$'+note+'\n```\n\n'+note);
  assert.equal(src.slice(found[0].from,found[0].to),note);
});

test('nested DSL source preserves original indentation when unchanged and writes the actual formula body',()=>{
  const {write,asDsl}=context.window.ReadMDFormula;
  for(const source of ['> ```readmd-math v1\n> frac(a, b)\n> ```','- ```readmd-math v1\n  frac(a, b)\n  ```','  ```readmd-math v1\nfrac(a, b)\n  ```']){
    const range=ranges(source)[0];assert.equal(source.slice(0,range.from)+write(range,range.tex),source);
    const changed=source.slice(0,range.from)+write(range,range.tex.replace('a, b','x, b'));assert.equal(changed,source.replace('a, b','x, b'));
  }
  const source='> $$a/b$$\n',range=ranges(source)[0];assert.equal(source.slice(0,range.from)+asDsl(range,'frac(a, b)',source)+source.slice(range.to),'> ```readmd-math v1\n> frac(a, b)\n> ```\n');
  const example='> ```markdown\n> :math[frac(]\n> ```\n';assert.equal(ranges(example).length,0);
});
