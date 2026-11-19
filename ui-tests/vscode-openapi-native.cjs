'use strict';
// Loaded by the installed VS Code Extension Host; decisions are actual UI clicks.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vscode=require('vscode');
exports.run=async()=>{
 const out=process.env.READMD_VSCODE_OPENAPI_AUDIT;
 const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
 const binding=JSON.parse(fs.readFileSync(path.join(out,'launch-binding.json'),'utf8'));
 const report={platform:process.platform,arch:process.arch,vscodeVersion:vscode.version,nativeExtensionHost:true,synthetic:true,passed:false,checks:[],projectionChecks:[],binarySha256:hash(fs.readFileSync(process.env.READMD_BIN)),buildBinding:binding.build,fixtureManifestSha256:binding.fixturesSha256,singleFileAuthorizationCertified:false};
 assert.equal(report.binarySha256,binding.kernelSha256);
 const delay=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async file=>{const deadline=Date.now()+30000;while(!fs.existsSync(file)){if(Date.now()>deadline)throw Error('Native review UI timed out');await delay(50);}};
 const state=(phase,expectedChanges)=>fs.writeFileSync(path.join(out,'state.json'),JSON.stringify({phase,expectedChanges}));
 const spec=path.join(out,'materials/openapi/bundle/api.yaml'),file=path.join(out,'materials','API-guide.md'),scalar=path.join(out,'materials/openapi/bundle/defs/scalar.yaml');
 const original='# API guide 😀\n\nAuthor notes and repeated heading stay intact.\n\n## 重复标题\n\nManual prose.\n';
 const scalarOriginal=fs.readFileSync(scalar,'utf8');
 const operations=[
  {method:'get',path:'/items/{id}',id:'getItem',pointer:'/paths/~1items~1{id}/get',definition:{source:'paths/items.yaml',pointer:'/get'}},
  {method:'post',path:'/items/{id}',id:'createItem',pointer:'/paths/~1items~1{id}/post',definition:{source:'paths/items.yaml',pointer:'/post'}},
  {method:'post',path:'/other',id:'otherItem',pointer:'/paths/~1other/post'},
  {method:'get',path:'/search',id:'searchItems',pointer:'/paths/~1search/get'}
 ];
 let document,editor,bridge;
 // Independent expected display: only the real transaction's exact boundary
 // can carry metadata. Never import the projection helper under test here.
 function expectedViews(before,data){
  const tx=data.transaction;assert.equal(tx.origin,'openapi');assert.equal(tx.base_revision,hash(before));assert.equal(tx.edits.length,1);
  const e=tx.edits[0];assert(Number.isSafeInteger(e.from)&&Number.isSafeInteger(e.to)&&e.from>=0&&e.to>=e.from&&e.to<=before.length);
  assert.equal(before.slice(0,e.from)+e.insert+before.slice(e.to),data.result.content);
  const append=e.from===before.length&&e.to===e.from,prefix=append&&before.length?'\n\n':'';assert(e.insert.startsWith(prefix));
  function carrier(segment){const end=segment.indexOf('\n');assert(end>0);const line=segment.slice(0,end);assert(line.startsWith('<!-- readmd:openapi:start ')&&line.endsWith(' -->'));
   const meta=JSON.parse(Buffer.from(line.slice('<!-- readmd:openapi:start '.length,-4),'base64url').toString('utf8'));assert.equal(meta.schema,1);assert.equal(Object.keys(meta.operations).length,4);assert.equal(meta.references.length,7);
   const tail='<!-- readmd:openapi:end '+meta.id+' -->';assert(segment.endsWith(tail));const body=segment.slice(end+1,-tail.length);assert.equal(hash(body),meta.body_sha256);return{line,body,meta};}
  const next=carrier(e.insert.slice(prefix.length)),old=append?undefined:carrier(before.slice(e.from,e.to));
  const changed=old&&old.line!==next.line;
  const summary=state=>'<!-- '+(state==='previous'?'Previous':state==='updated'?'Updated':'Generated')+' API record · Operations: 4 · Referenced files: 7. View raw text to inspect. -->';
  // Splitting into lines is deliberately independent of helper substring code.
  function replaceLine(raw,from,text){let offset=0,hits=0;const value=raw.split('\n').map(line=>{const at=offset;offset+=line.length+1;if(at!==from)return line;hits++;return text;}).join('\n');assert.equal(hits,1);return value;}
  return{before,after:data.result.content,left:old?replaceLine(before,e.from,summary(changed?'previous':undefined)):before,right:replaceLine(data.result.content,e.from+prefix.length,summary(changed?'updated':undefined)),next,old};
 }
 function ownedPair(){
  const tab=vscode.window.tabGroups.activeTabGroup.activeTab;assert(tab&&tab.input instanceof vscode.TabInputTextDiff,'Current active native diff required');
  const left=tab.input.original,right=tab.input.modified;assert.equal(left.scheme,'readmd-openapi-review');assert.equal(right.scheme,'readmd-openapi-review');
  assert.equal(left.path,'/API-documentation-before.md');assert.equal(right.path,'/API-documentation.md');
  assert.match(left.query,/^[a-f0-9-]{36}&before$/);assert.equal(right.query,left.query.slice(0,-6)+'after');
  assert.notEqual(vscode.workspace.fs.isWritableFileSystem(left.scheme),true);
  return{left,right,id:left.query.split('&')[0]};
 }
 function targetStamp(){return{uri:document.uri.toString(),version:document.version,textSha256:hash(document.getText()),dirty:document.isDirty,diskSha256:hash(fs.readFileSync(file))};}
 async function readViewPair(pair,expected,raw,phase,stamp){
  const left=await vscode.workspace.openTextDocument(pair.left),right=await vscode.workspace.openTextDocument(pair.right),until=Date.now()+5000;
  const a=raw?expected.before:expected.left,b=raw?expected.after:expected.right;
  while((left.getText()!==a||right.getText()!==b)&&Date.now()<until)await delay(25);
  assert.equal(left.getText(),a);assert.equal(right.getText(),b);assert.equal(left.isDirty,false);assert.equal(right.isDirty,false);
  const current=ownedPair();assert.equal(current.id,pair.id);assert.deepEqual(targetStamp(),stamp);
  return{phase,reviewId:pair.id,raw,leftSha256:hash(left.getText()),rightSha256:hash(right.getText()),targetUnchanged:true,readonlyProviderPair:true};
 }
 async function matchingReady(name,pair,phase){await wait(path.join(out,name));const value=JSON.parse(fs.readFileSync(path.join(out,name),'utf8'));assert.equal(value.phase,phase);assert.equal(value.reviewId,pair.id);}
 try{
  fs.writeFileSync(file,original,{flag:'wx'});
  const extension=vscode.extensions.all.find(e=>e.packageJSON.name==='readmd-vscode');assert.ok(extension);await extension.activate();
  await vscode.workspace.getConfiguration('readmd').update('executablePath',process.env.READMD_BIN,vscode.ConfigurationTarget.Global);
  await vscode.workspace.getConfiguration('readmd').update('aiAutocomplete',false,vscode.ConfigurationTarget.Global);
  const {ReadMDBridge}=require(path.join(extension.extensionPath,'out/bridge'));
  bridge=new ReadMDBridge({extensionPath:extension.extensionPath,subscriptions:[]});
  const args=content=>({file_path:spec,content,base_revision:hash(content),locale:'en'});
  const projected=await bridge.callMcpTool('readmd_openapi_document',args(original));
  assert.equal(projected.warnings.length,0);assert.equal(projected.source_map.length,4);
  const prefix='😀Z\n';assert.equal(prefix.length,4);
  const shifted=await bridge.callMcpTool('readmd_openapi_document',args(prefix+original));
  const text=projected.result.content;
  for(let i=0;i<operations.length;i++){
   const expected=operations[i],map=projected.source_map[i],next=operations[i+1];
   assert.equal(map.pointer,expected.pointer);assert.equal(map.method,expected.method);assert.equal(map.path,expected.path);assert.equal(map.operation_id,expected.id);
   if(expected.definition)assert.deepEqual(map.definition,expected.definition);
   const heading='## '+expected.method.toUpperCase()+' ` '+expected.path+' ` — 重复标题';
   assert.equal(text.indexOf(heading),text.lastIndexOf(heading));assert.equal(text.indexOf(heading),map.from);
   const boundary=next?'## '+next.method.toUpperCase()+' ` '+next.path+' ` — 重复标题':'#### Data models';
   assert.equal(text.indexOf(boundary,map.from+heading.length),map.to);
   assert.equal(shifted.source_map[i].from,map.from+4);assert.equal(shifted.source_map[i].to,map.to+4);
  }
  for(const field of['oneOf','anyOf','allOf','prefixItems','unevaluatedProperties','bearerFormat','authorizationCode'])assert(text.includes(field));
  assert(text.slice(projected.source_map[3].from,projected.source_map[3].to).includes('```json\n[]\n```'));
  report.sourceMaps={operations:operations.map(({method,path,id,pointer,definition})=>({method,path,id,pointer,definition})),utf16Prefix:'😀Z\\n',independentUtf16Delta:4};
  report.checks.push('shared real MCP resolves the fixed complex bundle; four fixed operations, external definitions, unique headings/component boundaries and independent UTF-16 prefix delta 4 agree');
  document=await vscode.workspace.openTextDocument(file);editor=await vscode.window.showTextDocument(document);
  await vscode.commands.executeCommand('workbench.action.closeSidebar');await vscode.commands.executeCommand('workbench.action.closePanel');
  async function review(phase,expectedChanges,mutate,projection){
   editor=await vscode.window.showTextDocument(document);const stamp=targetStamp();state(phase,expectedChanges);
   const command=vscode.commands.executeCommand('readmd.openApiDocument',vscode.Uri.file(spec));
   await wait(path.join(out,phase+'.ready'));let proof;
   if(projection){const pair=ownedPair(),views=[];views.push(await readViewPair(pair,projection,false,phase,stamp));fs.writeFileSync(path.join(out,phase+'.summary.checked'),JSON.stringify(views.at(-1)));
    await matchingReady(phase+'.raw.ready',pair,phase);views.push(await readViewPair(pair,projection,true,phase,stamp));fs.writeFileSync(path.join(out,phase+'.raw.checked'),JSON.stringify(views.at(-1)));
    if(phase!=='accept'){await matchingReady(phase+'.summary-again.ready',pair,phase);views.push(await readViewPair(pair,projection,false,phase,stamp));fs.writeFileSync(path.join(out,phase+'.summary-again.checked'),JSON.stringify(views.at(-1)));}
    proof={passed:true,phase,reviewId:pair.id,views,rawTransactionApplyExact:true,targetUnchangedDuringToggle:true,readonlyWriteDenialExplicitlyTested:false};
   }
   if(mutate)await mutate();fs.writeFileSync(path.join(out,phase+'.go'),'');
   await command;await wait(path.join(out,phase+'.clicked'));
   return proof;
  }
  const initialViews=expectedViews(original,projected);
  const cancelledView=await review('cancel',4,undefined,initialViews);assert.equal(document.getText(),original);assert.equal(document.isDirty,false);assert.equal(fs.readFileSync(file,'utf8'),original);
  report.checks.push('native diff Cancel leaves the document and disk unchanged');
  report.projectionChecks.push({id:'readable-raw-roundtrip',...cancelledView});
  const acceptedView=await review('accept',4,undefined,initialViews);const generated=document.getText();assert.equal(generated,text);assert.ok(generated.startsWith(original));assert.ok(document.isDirty);assert.equal(fs.readFileSync(file,'utf8'),original);
  editor=await vscode.window.showTextDocument(document);await vscode.commands.executeCommand('undo');assert.equal(document.getText(),original);await vscode.commands.executeCommand('redo');assert.equal(document.getText(),generated);assert.ok(await document.save());assert.equal(fs.readFileSync(file,'utf8'),generated);
  report.checks.push('native Accept is one undoable edit; disk changes only on Save');
  assert(generated.includes(initialViews.next.line));assert(!generated.includes('<!-- Generated API record · Operations: 4 · Referenced files: 7. View raw text to inspect. -->'));
  report.projectionChecks.push({id:'raw-accept-exact-transaction',...acceptedView,savedRawSha256:hash(fs.readFileSync(file)),generatedRawSha256:hash(text)});
  await vscode.commands.executeCommand('workbench.action.closeAllEditors');document=await vscode.workspace.openTextDocument(file);editor=await vscode.window.showTextDocument(document);
  assert.equal(document.getText(),generated);await vscode.commands.executeCommand('undo');assert.equal(document.getText(),original);assert.equal(fs.readFileSync(file,'utf8'),generated);await vscode.commands.executeCommand('redo');assert.equal(document.getText(),generated);assert.ok(await document.save());
  report.checks.push('actual VS Code save, editor close/reopen and one native undo/redo preserve the explicit undoStop group; this is separate from Rust durable history');
  const beforeVersion=document.version;state('idempotent');await vscode.commands.executeCommand('readmd.openApiDocument',vscode.Uri.file(spec));assert.equal(document.version,beforeVersion);assert.equal(document.isDirty,false);
  report.checks.push('identical specification is idempotent and does not dirty the editor');
  const rootOriginal=fs.readFileSync(spec);fs.appendFileSync(spec,'\n# OWNED NATIVE METADATA REVISION ONLY\n');
  try{const metadata=await bridge.callMcpTool('readmd_openapi_document',args(generated));assert.equal(metadata.result.changed,true);assert.equal(metadata.changes.length,0);assert.notEqual(metadata.source_revision,projected.source_revision);
   const views=expectedViews(generated,metadata);assert.equal(views.old.body,views.next.body);assert(views.left.includes('<!-- Previous API record ·'));assert(views.right.includes('<!-- Updated API record ·'));assert.notEqual(views.left,views.right);
   const proof=await review('metadata-only',0,undefined,views);assert.equal(document.getText(),generated);assert.equal(document.isDirty,false);assert.equal(fs.readFileSync(file,'utf8'),generated);
   report.projectionChecks.push({id:'metadata-only-visible',...proof,bodySha256:hash(views.next.body),operationChanges:0,operations:4,references:7,sourceRevisionChanged:true});
  }finally{fs.writeFileSync(spec,rootOriginal);}
  fs.writeFileSync(scalar,scalarOriginal.replace('minLength: 2','minLength: 5'));
  await review('source-conflict',3,()=>fs.writeFileSync(scalar,scalarOriginal.replace('minLength: 2','minLength: 8')));assert.equal(document.getText(),generated);assert.equal(document.isDirty,false);
  report.checks.push('dependency-only source change during the actual diff cannot be applied');
  fs.writeFileSync(scalar,scalarOriginal.replace('minLength: 2','minLength: 5'));
  await review('document-conflict',3,async()=>{const edit=new vscode.WorkspaceEdit();edit.insert(document.uri,new vscode.Position(0,0),'Concurrent author note\n');assert.ok(await vscode.workspace.applyEdit(edit));});
  assert.equal(document.getText(),'Concurrent author note\n'+generated);assert.equal(fs.readFileSync(file,'utf8'),generated);
  report.checks.push('concurrent author edits survive a stale Accept');
  editor=await vscode.window.showTextDocument(document);await vscode.commands.executeCommand('undo');assert.equal(document.getText(),generated);
  const marker='OPENAPI_V007_MULTIFILE_RESPONSE',start=document.getText().indexOf(marker);assert.ok(start>=0);await editor.edit(b=>b.replace(new vscode.Range(document.positionAt(start),document.positionAt(start+marker.length)),'Manual generated edit'));
  const manual=document.getText();state('manual-conflict');await vscode.commands.executeCommand('readmd.openApiDocument',vscode.Uri.file(spec));assert.equal(document.getText(),manual);assert.equal(fs.readFileSync(file,'utf8'),generated);
  report.checks.push('manual edits inside the generated region are preserved and updates refused');
  report.passed=true;state('passed');
 }catch(error){report.message=String(error.message).replaceAll(out,'<profile>').slice(0,900);state('failed');throw error;}
 finally{bridge?.dispose();report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));}
};
