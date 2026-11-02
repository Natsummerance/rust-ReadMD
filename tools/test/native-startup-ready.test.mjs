import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash} from 'node:crypto';
const source=fs.readFileSync(new URL('../../rust/readmd-kernel/src/main.rs',import.meta.url),'utf8');
const script=source.match(/"(String\(document\.readyState === 'complete'[^\n]+)"/)?.[1];
for (const asset of ['readmd.boot.js','css/reader.css']) test('same-version hotfixes bind the immutable cache URL to '+asset,()=>{
  const html=fs.readFileSync(new URL('../../assets/index.html',import.meta.url),'utf8');
  const src=[...html.matchAll(/(?:src|href)="(\/assets\/[^"?]+\?[^"]+)"/g)].map(match=>match[1]).find(value=>value.startsWith('/assets/'+asset+'?'));assert.ok(src);
  const url=new URL(src.replaceAll('&amp;','&'),'http://127.0.0.1');
  assert.equal(url.searchParams.get('v'),fs.readFileSync(new URL('../../VERSION',import.meta.url),'utf8').trim());
  const sha=createHash('sha256').update(fs.readFileSync(new URL('../../assets/'+asset,import.meta.url))).digest('hex');
  assert.equal(url.searchParams.get('rev'),sha.slice(0,12),'Refresh the content revision after rebuilding '+asset);
});
test('native probe rejects a complete blank page and waits for production boot and bridge',()=>{
  assert.ok(script);
  const ready=(document,window)=>vm.runInNewContext(script,{document,window});
  assert.equal(ready({readyState:'complete'},{}),'false');
  assert.equal(ready({readyState:'loading'},{__readmdAppReady:true,pywebview:{api:{report_ready(){}}}}),'false');
  assert.equal(ready({readyState:'complete'},{__readmdAppReady:true}),'false');
  assert.equal(ready({readyState:'complete'},{__readmdAppReady:true,pywebview:{api:{report_ready(){}}}}),'true');
});
