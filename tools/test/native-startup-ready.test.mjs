import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../../rust/readmd-kernel/src/main.rs',import.meta.url),'utf8');
const script=source.match(/"(String\(document\.readyState === 'complete'[^\n]+)"/)?.[1];
test('native probe rejects a complete blank page and waits for production boot and bridge',()=>{
  assert.ok(script);
  const ready=(document,window)=>vm.runInNewContext(script,{document,window});
  assert.equal(ready({readyState:'complete'},{}),'false');
  assert.equal(ready({readyState:'loading'},{__readmdAppReady:true,pywebview:{api:{report_ready(){}}}}),'false');
  assert.equal(ready({readyState:'complete'},{__readmdAppReady:true}),'false');
  assert.equal(ready({readyState:'complete'},{__readmdAppReady:true,pywebview:{api:{report_ready(){}}}}),'true');
});
