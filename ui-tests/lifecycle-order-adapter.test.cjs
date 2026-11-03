'use strict';
// Ordering/adapter checks; persistence and native consumers have separate tests.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.join(__dirname,'../assets/js/core/lifecycle.js'),'utf8');
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};}
function fixture(){
 const tab={id:'owned-a',path:'/synthetic/A.md',content:'Draft',revision:'r',editGeneration:1};
 const head={document_id:'owned-review',generation:2,review_revision:1,revision:'r'};
 const state={tabs:[tab],activeTabId:tab.id,editing:true};
 const window={ReadMDSession:{restoring:()=>false,retryRecovery:async()=>true},ReadMDReview:{snapshot:()=>head,lf:s=>s,onEditorUpdate(){}},ReadMDRecovery:{prepareClose:async()=>true,prepareInstall:async()=>true}};
 const context=vm.createContext({window,state,normalizePath:p=>p,getActiveTab:()=>state.tabs.find(t=>t.id===state.activeTabId),getEditContent:()=>tab.content,
  confirmExitEdit:async()=>true,closeTab:async()=>true,closeOtherTabs:async()=>true,closeAllTabs:async()=>true,switchTab:async()=>true,goHome:async()=>true,newDocument:async()=>{},
  saveEdit:async()=>true,commitDocumentSave(){window.ReadMDReview.onEditorUpdate({docChanged:true});return Promise.resolve(true);},
  exitEdit(){state.editing=false;},renderActiveTab:async()=>{},toggleEdit:async()=>{state.editing=true;},renderVirtual:async()=>{}});
 vm.runInContext(source,context);return{context,window,state,tab,head,api:window.ReadMDLifecycle};
}
test('an overlapping close settles as cancellation and preserves the first owner',async()=>{
 const f=fixture(),held=deferred(),first=f.api.guard('close',()=>held.promise),second=f.api.guard('quit',()=>assert.fail('overlap must not run'));
 const pending=first();assert.equal(await second(),false);assert.equal(f.api.busy(),true);held.resolve(true);assert.equal(await pending,true);assert.equal(f.api.busy(),false);
});
test('an exception releases deferred navigation without leaking a lifecycle owner',async()=>{
 const f=fixture(),held=deferred();const pending=f.api.guard('close',async()=>{await held.promise;throw Error('owned failure');})();
 let released=false;const waiter=f.api.whenIdle().then(()=>released=true);await Promise.resolve();assert.equal(released,false);
 held.resolve();await assert.rejects(pending,/owned failure/);await waiter;assert.equal(released,true);assert.equal(f.api.busy(),false);
});
test('a nested edit decision needs the exact local owner and settles with its parent',async()=>{
 const f=fixture(),child=f.api.guard('exit-edit',async()=>true);
 const parent=f.api.guard('home',async()=>{const owner=f.api.owner();await Promise.resolve();assert.equal(await child(),false);assert.equal(await f.api.within({},()=>child()),false);return f.api.within(owner,()=>child());});
 assert.equal(await parent(),true);assert.equal(f.api.busy(),false);
});
test('identical text cannot hide a new editor or Rust review generation',()=>{
 const f=fixture(),stamp=f.api.mark(f.tab);assert.equal(f.api.matches(stamp),true);
 f.tab.editGeneration++;assert.equal(f.api.matches(stamp),false);f.tab.editGeneration--;
 f.head.generation++;assert.equal(f.api.matches(stamp),false);f.head.generation--;
 f.tab.path='/synthetic/B.md';assert.equal(f.api.matches(stamp),false);
});
test('approved synchronous save projection is distinct from later input',async()=>{
 const f=fixture();f.api.install();await f.context.commitDocumentSave({tab:f.tab});assert.equal(f.tab.inputGeneration,undefined);
 f.window.ReadMDReview.onEditorUpdate({docChanged:true});assert.equal(f.tab.inputGeneration,1);
 assert.equal(f.api.applyingSave(f.tab),false);
});
test('ordinary saving keeps navigation available while its fixed snapshot is pending',async()=>{
 const f=fixture(),held=deferred();f.context.saveEdit=()=>held.promise;f.api.install();
 const pending=f.context.saveEdit();assert.equal(f.api.busy(),false);held.resolve(true);assert.equal(await pending,true);
});
