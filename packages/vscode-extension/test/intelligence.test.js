const test=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const Module=require('node:module');
const entry=path.resolve(__dirname,'../out/intelligence.js');
function fixture(){
 const root=path.resolve('synthetic-workspace'),events={},subscriptions=[],values=new Map();
 const uri=file=>({scheme:'file',fsPath:file,toString:()=>file,with(options){return {...this,...options};}});
 const doc={uri:uri(path.join(root,'Reading.md')),languageId:'markdown',version:1,isClosed:false,
  lineCount:1,getText:()=> '[[Target]]',lineAt:()=>({text:'[[Target]]'})};
 const disposable=()=>({dispose(){}}),listener=name=>handler=>{events[name]=handler;return disposable();};
 let provider;
 const stub={env:{language:'en'},Uri:{file:uri},DiagnosticSeverity:{Warning:1},
  Range:class{constructor(...args){this.args=args;}},Diagnostic:class{constructor(range,message,severity){Object.assign(this,{range,message,severity});}},
  DocumentLink:class{constructor(range,target){Object.assign(this,{range,target});}},
  languages:{createDiagnosticCollection:()=>({set:(key,value)=>values.set(key.toString(),value),delete:key=>values.delete(key.toString()),clear:()=>values.clear(),dispose(){}}),
   registerDocumentLinkProvider:(_,value)=>{provider=value;return disposable();}},
  commands:{registerCommand:()=>disposable()},window:{activeTextEditor:{document:doc}},
  workspace:{isTrusted:true,textDocuments:[doc],getWorkspaceFolder:key=>key.fsPath.startsWith(root+path.sep)?{uri:uri(root)}:undefined,
   createFileSystemWatcher:()=>({onDidCreate:listener('create'),onDidChange:listener('change'),onDidDelete:listener('delete'),dispose(){}}),
   onDidOpenTextDocument:listener('open'),onDidSaveTextDocument:listener('save'),onDidChangeTextDocument:listener('edit'),
   onDidCloseTextDocument:listener('close'),onDidChangeWorkspaceFolders:listener('folders'),onDidCreateFiles:listener('createFiles'),onDidDeleteFiles:listener('deleteFiles'),onDidRenameFiles:listener('renameFiles')}
 };
 const original=Module._load;
 Module._load=function(name,...args){return name==='vscode'?stub:original.call(this,name,...args);};
 let register;try{delete require.cache[entry];register=require(entry).registerIntelligence;}finally{Module._load=original;}
 let exists=false,calls=0;
 const result=()=>({truncated:false,links:exists?[{kind:'wiki',target:'Target',resolved_path:path.join(root,'Target.md'),position:{line:1,column:1}}]:[],
  diagnostics:exists?[]:[{code:'missing_file',target:'Target',position:{line:1,column:1}}]});
 const bridge={callMcpTool:async()=>{calls++;return result();}};
 register({subscriptions},bridge);
 return {events,doc,bridge,values,result,target:uri(path.join(root,'Target.md')),generated:uri(path.join(root,'node_modules','package.json')),
  links:()=>provider.provideDocumentLinks(doc,{isCancellationRequested:false}),setExists:value=>{exists=value;},calls:()=>calls,
  dispose:()=>subscriptions.forEach(s=>s.dispose())};
}
test('external create/change/delete refresh native diagnostics without editing the source document',async()=>{
 const f=fixture();try{
  await f.links();assert.equal(f.values.get(f.doc.uri.toString())[0].code,'missing_file');
  await f.links();assert.equal(f.calls(),1,'Unchanged source reuses the current analysis');
  f.events.create(f.generated);await f.links();assert.equal(f.calls(),1,'Generated tree churn does not invalidate the cache');
  f.setExists(true);f.events.create(f.target);assert.equal((await f.links()).length,1);assert.deepEqual(f.values.get(f.doc.uri.toString()),[]);
  f.events.change(f.target);await f.links();assert.equal(f.calls(),3);
  f.setExists(false);f.events.delete(f.target);await f.links();assert.equal(f.values.get(f.doc.uri.toString())[0].code,'missing_file');
  assert.equal(f.doc.version,1,'External target updates do not mutate the source draft');
 }finally{f.dispose();}
});
test('a filesystem invalidation discards a late analysis result',async()=>{
 const f=fixture();try{
  let resolve;f.bridge.callMcpTool=()=>new Promise(done=>{resolve=done;});
  const pending=f.links();f.events.change(f.target);resolve(f.result());
  assert.deepEqual(await pending,[]);assert.equal(f.values.size,0,'Late diagnostics cannot reappear');
  f.setExists(true);f.bridge.callMcpTool=async()=>f.result();assert.equal((await f.links()).length,1);
 }finally{f.dispose();}
});
