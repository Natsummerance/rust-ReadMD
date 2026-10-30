'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
test('a probed native service is usable without claiming a managed installation',async()=>{
  const calls=[],window={i18n:{t:key=>key}};
  const context=vm.createContext({window,apiFetch:async(route,options)=>{
    calls.push([route,JSON.parse(options.body)]);
    return {ok:true,json:async()=>({ok:true,id:'ocr',native:true,installed:false,ready:true})};
  }});
  vm.runInContext(fs.readFileSync(path.resolve(__dirname,'../../assets/js/core/capabilities.js'),'utf8'),context);
  assert.equal(await window.ReadMDCapabilities.ensure('ocr'),true);
  assert.deepEqual(calls,[['/api/capabilities/packs',{action:'status',id:'ocr'}]]);
});
