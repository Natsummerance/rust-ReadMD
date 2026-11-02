'use strict';
// Evaluate the real lifecycle functions; these ordering tests do not certify native persistence.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const tabs=fs.readFileSync(path.join(__dirname,'../assets/js/core/tabs.js'),'utf8');
const history=fs.readFileSync(path.join(__dirname,'../assets/js/features/document-history.js'),'utf8');
function fixture(){
 const a={id:'a',path:'/synthetic/A.md',content:'A original',original:'A original',isDirty:false};
 const b={id:'b',path:'/synthetic/B.md',content:'B draft',original:'B original',isDirty:true};
 const state={tabs:[a,b],activeTabId:a.id,editing:false};let saves=0,closes=0;
 const window={ReadMDReview:{flush:async tab=>tab!==a,flushAll:async()=>true},ReadMDSession:{busy:()=>false,async closeTabs(items,remove){closes++;remove();return true;}}};
 const context=vm.createContext({window,state,document:{activeElement:null},Element:class{},
  getActiveTab:()=>state.tabs.find(tab=>tab.id===state.activeTabId),promptDirtyClose:async()=> 'save',
  saveEdit:async()=>{saves++;const tab=context.getActiveTab();tab.isDirty=false;state.editing=false;return true;},
  exitEdit:()=>{state.editing=false;},toggleEdit:async()=>{state.editing=true;},renderActiveTab:async()=>{},
  syncStateFromActiveTab(){},syncActiveTabDirty(){},renderTabsBar(){},resetHome(){},showToast(){},
  getEditContent:()=>context.getActiveTab()?.content||'',focusVisibleTab(){},$:()=>({value:''})});
 vm.runInContext(tabs.slice(tabs.indexOf('async function activateTabForSave('),tabs.indexOf('\nasync function switchTab(')),context);
 vm.runInContext(tabs.slice(tabs.indexOf('async function closeTab('),tabs.indexOf('\n/* Persist the open page set,')),context);
 vm.runInContext(history.slice(history.indexOf('  async function confirmExitChanges('),history.indexOf('\n  async function sessionDrafts(')),context);
 return{a,b,state,window,context,saves:()=>saves,closes:()=>closes};
}
for(const operation of ['single','others','all','quit'])test(operation+' never saves the visible page when activating the dirty target fails',async()=>{
 const f=fixture();
 if(operation==='single')await f.context.closeTab(f.b.id);
 else if(operation==='others')await f.context.closeOtherTabs(f.a.id);
 else if(operation==='all')await f.context.closeAllTabs();
 else assert.equal(await f.context.confirmExitChanges(),false);
 assert.equal(f.saves(),0,'No save is allowed after a failed target activation');
 assert.equal(f.closes(),0);assert.deepEqual(f.state.tabs,[f.a,f.b]);
 assert.equal(f.state.activeTabId,f.a.id);assert.equal(f.b.content,'B draft');assert.equal(f.b.isDirty,true);
});
test('activation refuses a closed target instead of creating an empty editor',async()=>{
 const f=fixture();f.state.tabs=[f.a];f.window.ReadMDReview.flush=async()=>true;
 assert.equal(await f.context.activateTabForSave(f.b.id),false);
 assert.equal(f.state.activeTabId,f.a.id);assert.equal(f.state.editing,false);
});
test('activation rechecks the visible owner after its review flush yields',async()=>{
 const f=fixture();let release;f.window.ReadMDReview.flush=()=>new Promise(resolve=>release=resolve);
 const activation=f.context.activateTabForSave(f.b.id);f.state.activeTabId=f.b.id;release(true);
 assert.equal(await activation,false);assert.equal(f.state.editing,false);
});
test('successful activation confirms the exact editor owner before saving and closing it',async()=>{
 const f=fixture();f.window.ReadMDReview.flush=async()=>true;
 await f.context.closeTab(f.b.id);
 assert.equal(f.saves(),1);assert.equal(f.closes(),1);assert.deepEqual(f.state.tabs,[f.a]);
 assert.equal(f.a.content,'A original');
});
