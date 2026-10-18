import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import zlib from 'node:zlib';

const context={window:{},document:{readyState:'loading',addEventListener(){}}};
vm.createContext(context);
vm.runInContext(fs.readFileSync('assets/js/editor/formula-studio.js','utf8'),context);
const formula=context.window.ReadMDFormula;
const note=id=>'<!--readmd:math:v2:'+id+':'+zlib.gzipSync(JSON.stringify({id,unknown:'$fake$ :math[not source]'})).toString('base64')+'-->';

test('v2 carriers are hidden by their exact occurrence range and never parsed as formula source',()=>{
  const expression='frac(root(3, "𝑎"), "β")',notes=['eq-one','eq-two','eq-three'].map(note);
  const source='\uFEFF# Author 😀\r\n\r\n'+notes.map((n,i)=>'Occurrence '+i+': :math['+expression+']'+n+'.').join('\r\n')+'\r\n';
  const found=formula.ranges(source),hidden=formula.metadataRanges(source);
  assert.equal(found.length,3);assert.equal(hidden.length,3);
  for(let i=0;i<3;i++){assert.equal(found[i].tex,expression);assert.equal(source.slice(hidden[i].from,hidden[i].to),notes[i]);assert.equal(hidden[i].from,found[i].to);}
  assert.equal(formula.withoutMetadata(source),source.replaceAll(notes[0],'').replaceAll(notes[1],'').replaceAll(notes[2],''));
  const selected=found[1],changed=source.slice(0,selected.from)+formula.write(selected,'frac(root(4, "𝑎"), "β")')+source.slice(selected.to);
  assert.equal(formula.ranges(changed)[0].tex,expression);assert.equal(formula.ranges(changed)[2].tex,expression);
  assert.equal(changed.slice(0,selected.from),source.slice(0,selected.from));assert(changed.includes(notes[1]));
});

test('code, raw HTML and unassociated v2 data remain ordinary source instead of stealing visible identities',()=>{
  const n=note('code'),visible=':math[frac(a, b)]'+note('actual');
  const source='```markdown\r\n:math[frac(a, b)]'+n+'\r\n```\r\n`'+visible+'`\r\n<pre>'+visible+'</pre>\r\n'+n+'\r\n'+visible;
  const hidden=formula.metadataRanges(source);assert.equal(hidden.length,1);assert.equal(source.slice(hidden[0].from,hidden[0].to),note('actual'));
  assert.equal(formula.ranges(source).length,1);assert(formula.withoutMetadata(source).includes(n));
});

test('v2 carrier offsets retain nested fence indentation and CRLF author trivia',()=>{
  const n=note('nested'),source='> ```readmd-math v1\r\n> // 😀 author\r\n> frac( x , root(3, β) )\r\n> ```\r\n'+n;
  const ranges=formula.ranges(source);assert.equal(ranges.length,1);const range=ranges[0];
  assert.equal(source.slice(0,range.from)+formula.write(range,range.tex)+source.slice(range.to),source);
  const hidden=formula.metadataRanges(source);assert.equal(hidden.length,1);assert.equal(source.slice(hidden[0].from,hidden[0].to),n);
});
