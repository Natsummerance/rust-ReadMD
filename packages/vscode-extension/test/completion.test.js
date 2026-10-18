const test=require('node:test'),assert=require('node:assert/strict'),Module=require('node:module'),path=require('node:path');
function fixture(){
 let provider,calls=0,enabled=true,finish,model='',argumentsSent;
 const p={line:1,character:5,isEqual:other=>other===p};
 const doc={version:1,getText:()=> '问题需要接受检验。',lineAt:()=>({text:'问题需要接受检验。'}),offsetAt:()=>8};
 const editor={document:doc,selections:[{}],selection:{isEmpty:true,active:p}};
 const stub={workspace:{isTrusted:true,getConfiguration:()=>({get:key=>key==='aiAutocompleteModel'?model:enabled}),onDidChangeConfiguration:()=>({dispose(){}})},
 window:{activeTextEditor:editor},languages:{registerInlineCompletionItemProvider:(_,v)=>{provider=v;return{dispose(){}};}},
 Range:class{constructor(start,end){this.start=start;this.end=end;}},InlineCompletionItem:class{constructor(insertText,range){this.insertText=insertText;this.range=range;}}};
 const load=Module._load,file=path.resolve(__dirname,'../out/completion.js');
 Module._load=function(name,...args){return name==='vscode'?stub:load.call(this,name,...args);};
 try{delete require.cache[file];require(file).registerCompletion({subscriptions:[]},{callMcpTool:async(_tool,args)=>{calls++;argumentsSent=args;return new Promise(r=>{finish=r;});}});}finally{Module._load=load;}
 const token={isCancellationRequested:false,onCancellationRequested:()=>({dispose(){}})};
 return {doc,stub,token,invoke:()=>provider.provideInlineCompletionItems(doc,p,{},token),calls:()=>calls,arguments:()=>argumentsSent,model:value=>{model=value;},finish:()=>finish({completion:'然后提出问题。'}),disable:()=>{enabled=false;}};
}
test('VS Code completion respects opt-in and workspace trust before AI egress',async()=>{
 const f=fixture();f.disable();assert.deepEqual(await f.invoke(),[]);assert.equal(f.calls(),0);
 const g=fixture();g.stub.workspace.isTrusted=false;assert.deepEqual(await g.invoke(),[]);assert.equal(g.calls(),0);
});
test('obsolete VS Code draft cannot receive a late completion',async()=>{
 const f=fixture(),result=f.invoke();await new Promise(r=>setTimeout(r,680));assert.equal(f.calls(),1);
 f.doc.version++;f.finish();assert.deepEqual(await result,[]);
});
test('VS Code returns native ghost text and reuses the identical in-memory context',async()=>{
 const f=fixture(),result=f.invoke();await new Promise(r=>setTimeout(r,680));f.finish();
 assert.equal((await result)[0].insertText,'然后提出问题。');
 const began=Date.now();assert.equal((await f.invoke())[0].insertText,'然后提出问题。');assert.equal(f.calls(),1);
 assert.ok(Date.now()-began<150,'A cached suggestion should skip network debounce');
});
test('VS Code can fix an independent completion model without changing chat settings',async()=>{
 const f=fixture();f.model('fast-writing-model');const result=f.invoke();
 await new Promise(r=>setTimeout(r,350));assert.equal(f.arguments().model,'fast-writing-model');
 f.finish();assert.equal((await result)[0].insertText,'然后提出问题。');
 f.model('another-model');const changed=f.invoke();await new Promise(r=>setTimeout(r,350));
 assert.equal(f.calls(),2);assert.equal(f.arguments().model,'another-model');f.finish();await changed;
});
