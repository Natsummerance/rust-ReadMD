'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../assets/js/core/tabs.js'),'utf8');
const begin=source.indexOf('function promptDirtyClose('),end=source.indexOf('\nfunction askChoice(',begin);
function fixture(){
 const nodes=new Map(),listeners=new Map(),timers=new Map();let serial=0,focuses=0;
 for(const id of ['close-confirm-modal','close-confirm-title','close-confirm-desc','close-confirm-save','close-confirm-discard','close-confirm-cancel'])nodes.set(id,{textContent:'',onclick:null,classList:{add(){},remove(){}},addEventListener(k,f){listeners.set(k,f);},removeEventListener(k){listeners.delete(k);},focus(){focuses++;}});
 const context=vm.createContext({window:{i18n:{t:(key,p)=>p?.name||key}},$:id=>nodes.get(id),document:{addEventListener(k,f){listeners.set(k,f);},removeEventListener(k){listeners.delete(k);}},setTimeout(fn){const id=++serial;timers.set(id,fn);return id;},clearTimeout:id=>timers.delete(id)});
 vm.runInContext(source.slice(begin,end),context);
 return{context,nodes,listeners,timers,focuses:()=>focuses,click:action=>nodes.get('close-confirm-'+action).onclick()};
}
test('a second close request cannot replace the first dialog handlers or leave its promise hanging',async()=>{
 const f=fixture();const first=f.context.promptDirtyClose('First.md');let secondResult;
 const second=f.context.promptDirtyClose('Other.md').then(value=>secondResult=value);
 await Promise.resolve();assert.equal(secondResult,'cancel');
 assert.equal(f.nodes.get('close-confirm-desc').textContent,'First.md');
 f.click('discard');assert.equal(await first,'discard');await second;
 assert.equal(f.listeners.size,0);assert.equal(f.timers.size,0);
});
test('cancel releases the dialog for a later independent save decision',async()=>{
 const f=fixture();const first=f.context.promptDirtyClose('A.md');f.click('cancel');assert.equal(await first,'cancel');
 const next=f.context.promptDirtyClose('B.md');f.click('save');assert.equal(await next,'save');
 assert.equal(f.listeners.size,0);assert.equal(f.timers.size,0);
});
