'use strict';
// String envelopes avoid ArkWeb complex-type marshaling. The Rust server owns
// parsing, history, conflict checks and AI, just as on desktop.
window.installReadMDHarmony=function(){
 if(!window.ReadMDHarmony||window.__readmdHarmonyInstalled)return;
 window.__readmdHarmonyInstalled=true;
 const pending=new Map();let serial=0;
 window.__readmdHarmonyReply=(id,value,error)=>{
  const item=pending.get(id);if(!item)return;pending.delete(id);clearTimeout(item.timer);
  if(error)item.reject(new Error(error));else item.resolve(value);
 };
 const native=(op,args={})=>new Promise((resolve,reject)=>{
  const id=++serial,timer=setTimeout(()=>{pending.delete(id);reject(new Error('native_operation_timeout'));},120000);
  pending.set(id,{resolve,reject,timer});window.ReadMDHarmony.call(JSON.stringify({id,op,...args}));
 });
 const api=async(route,body)=>{
  const r=await apiFetch(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  const data=await r.json();if(!r.ok||data.ok===false)throw new Error(data.error_code||'native_operation_failed');return data;
 };
 window.pywebview={api:{
  choose_file:()=>native('choose_file'),choose_any_file:()=>native('choose_file'),
  choose_many_files:async()=>{const p=await native('choose_file');return p?[p]:[];},
  save_file:(path,content,encoding,expected_mtime)=>api('/api/save',{path,content,encoding,expected_mtime}),
  save_as:async(content,suggestedName)=>{
   const value=await native('save_as',{text:content,name:suggestedName});
   if(!value)return {canceled:true};
   // External document URIs are picker capabilities, not Rust filesystem
   // paths. Retarget to the private working copy, never to an unreadable URI.
   const result=JSON.parse(value);
   result.warns=[navigator.language.startsWith('zh')?'已导出到所选位置。后续修改保存在应用工作副本中；更新外部文件请再次另存为。':'Exported to the selected location. Further edits use the private working copy; use Save As again to update an external file.'];
   return result;
  },
  read_clipboard:async()=>({text:await native('clipboard'),html:'',source_type:'text'}),
  get_app_info:async()=>({version:'0.0.5',platform:'harmonyos',native:true}),
  get_system_language:async()=>navigator.language,
  show_window:async()=>true,report_ready:async()=>true,start_modules:()=>api('/api/modules'),
 }};
 window.dispatchEvent(new Event('pywebviewready'));
};
