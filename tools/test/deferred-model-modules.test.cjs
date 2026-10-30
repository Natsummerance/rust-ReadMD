'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
function fixture(replies){
 const calls=[],timers=[],state={modulesStarted:true,modules:{},ai:{config:{}}},status={dataset:{},setAttribute(){}};
 const window={i18n:{t:key=>key},addEventListener(){}};
 const context=vm.createContext({window,state,moduleLoadRequests:Object.create(null),$:id=>id==='status-mods'?status:null,
  setTimeout:(fn,ms)=>timers.push(ms),showToast:()=>assert.fail('unexpected error toast'),loadAiConfig:()=>assert.fail('unexpected connection load'),
  apiFetch:async(route,options)=>{calls.push([route,options?.method||'GET']);const value=replies.shift();assert(value,'unexpected request');return {ok:true,json:async()=>value};}});
 vm.runInContext(fs.readFileSync(path.resolve(__dirname,'../../assets/js/core/modules.js'),'utf8'),context);
 return {context,state,calls,timers,status};
}
test('deferred recognition stays pending without an endless status poll or a ready claim',async()=>{
 const f=fixture([{modules:{convert:'ready',ocr:'idle',transcribe:'idle'},deferred_modules:['ocr','transcribe']}]);
 await f.context.pollModules();assert.deepEqual(f.calls,[['/api/modules','GET']]);assert.deepEqual(f.timers,[]);
 assert.equal(f.state.modules.ocr,'idle');assert.equal(f.status.dataset.moduleState,'idle');
 assert.match(f.status.title,/convert.statusPending/);
});
test('an explicit recognition entry still uses the real load/readiness protocol',async()=>{
 const f=fixture([{status:'loading'},{modules:{ocr:'ready'}}]);
 assert.equal(await f.context.ensureModule('ocr'),true);
 assert.deepEqual(f.calls,[['/api/modules/load','POST'],['/api/modules','GET']]);assert.equal(f.state.modules.ocr,'ready');
});
test('an explicitly loading deferred engine remains observable until it settles',async()=>{
 const f=fixture([{modules:{ocr:'loading',transcribe:'idle'},deferred_modules:['ocr','transcribe']}]);
 await f.context.pollModules();assert.deepEqual(f.timers,[900]);assert.equal(f.status.dataset.moduleState,'loading');
});
