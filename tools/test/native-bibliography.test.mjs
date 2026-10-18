import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../../rust/readmd-kernel/src/main.rs',import.meta.url),'utf8');
const script=source.match(/get_bibtex: (async function\(path\) \{[\s\S]*?\n {16}\}),/)[1];
async function invoke(data){
  const requests=[];
  const context=vm.createContext({encodeURIComponent,bridgeHeaders:()=>({Authorization:'fixture'}),fetch:async(url,options)=>{requests.push({url,options});return{json:async()=>data};}});
  const result=await vm.runInContext('('+script+')',context)('C:/Research/note.md');
  assert.equal(requests[0].url,'/api/bibtex?p=C%3A%2FResearch%2Fnote.md');
  assert.equal(requests[0].options.headers.Authorization,'fixture');
  return JSON.parse(JSON.stringify(result));
}
test('native bibliography preserves current citations when legacy entries is empty',async()=>{
  const citations={work:{cite_key:'work',title:'Evidence',short_cite:'Group, 2026'}};
  assert.deepEqual(await invoke({ok:true,count:1,entries:[],citations}),citations);
});
test('native bibliography continues to accept legacy arrays and bare maps',async()=>{
  assert.deepEqual(await invoke({entries:[{key:'work',type:'book',fields:{title:'Evidence'}}]}),{work:{title:'Evidence',entry_type:'book',cite_key:'work'}});
  assert.deepEqual(await invoke({work:{title:'Evidence'}}),{work:{title:'Evidence'}});
  assert.deepEqual(await invoke({ok:false,error:'missing'}),{});
});
