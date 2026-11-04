import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {inputsLock,select,prepare,downloadInputs} from '../prepare-node-runtime.mjs';

test('Node preparation pins six distinct official targets and rejects platform aliases',()=>{
  assert.equal(inputsLock.version,'24.22.2');assert.equal(inputsLock.items.length,6);
  assert.equal(new Set(inputsLock.items.map(x=>x.platform)).size,6);
  assert.equal(select('windows-x64').binary,'node.exe');
  for(const value of ['universal','darwin-arm64','../node','linux-riscv64'])assert.throws(()=>select(value));
});
test('corrupt local Node input is preserved and never expanded or silently downloaded',async t=>{
  const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'node-input-'))),item=select('macos-arm64');t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const file=path.join(root,item.file),output=path.join(root,'output');fs.writeFileSync(file,'preserve these foreign bytes');
  await assert.rejects(prepare({inputs:root,output,platform:item.platform}),/digest mismatch/);
  await assert.rejects(downloadInputs({inputs:root,platforms:[item.platform]}),/differs from lock/);
  assert.equal(fs.existsSync(output),false);assert.equal(fs.readFileSync(file,'utf8'),'preserve these foreign bytes');
  assert.deepEqual(fs.readdirSync(root),[item.file]);
});
