'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const source=fs.readFileSync(path.resolve(__dirname,'../../assets/js/features/convert.js'),'utf8');
test('native plugin entry waits for the existing locale/preferences barrier',async()=>{
 let ready;const calls=[],modal={classList:{remove:()=>calls.push('open')}};
 const context=vm.createContext({window:{__readmdPreferencesReady:new Promise(r=>{ready=r;})},$:()=>modal,_t:k=>k,setPluginListStatus:()=>calls.push('status'),refreshPluginList:async()=>calls.push('translated-cards')});
 const begin=source.indexOf('async function openPluginModal()'),end=source.indexOf('function closePluginModal()',begin);vm.runInContext(source.slice(begin,end),context);const opening=context.openPluginModal();await Promise.resolve();assert.deepEqual(calls,[]);ready();await opening;assert.deepEqual(calls,['open','status','translated-cards']);
});
test('visible dynamic cards refresh on locale changes; hidden panels stay idle',()=>{
 const begin=source.indexOf("window.addEventListener('readmd:language-changed'"),end=source.indexOf('function initPluginTabsOnce()',begin);let listener,hidden=false,refresh=0;
 const context=vm.createContext({window:{addEventListener:(_name,fn)=>{listener=fn;}},$:()=>({classList:{contains:()=>hidden}}),refreshPluginList:()=>refresh++});vm.runInContext(source.slice(begin,end),context);listener();assert.equal(refresh,1);hidden=true;listener();assert.equal(refresh,1);
});
