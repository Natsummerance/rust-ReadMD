'use strict';
// Loaded by the installed VS Code Extension Host; decisions are actual UI clicks.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vscode=require('vscode');
exports.run=async()=>{
 const out=process.env.READMD_VSCODE_OPENAPI_AUDIT;
 const report={platform:'windows-x64',nativeExtensionHost:true,synthetic:true,passed:false,checks:[],binarySha256:crypto.createHash('sha256').update(fs.readFileSync(process.env.READMD_BIN)).digest('hex')};
 const delay=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async file=>{const deadline=Date.now()+30000;while(!fs.existsSync(file)){if(Date.now()>deadline)throw Error('Native review UI timed out');await delay(50);}};
 const state=phase=>fs.writeFileSync(path.join(out,'state.json'),JSON.stringify({phase}));
 const spec=path.join(out,'materials','research-api.yaml'),file=path.join(out,'materials','API-guide.md');
 const original='# API guide\n\nAuthor notes remain outside generated documentation. 😀\n';
 const yaml='openapi: 3.1.0\ninfo:\n  title: Research API\n  version: 1.0.0\npaths:\n  /notes:\n    get:\n      summary: List research notes\n      responses:\n        "200":\n          description: Note collection\n';
 let document,editor;
 try{
  fs.writeFileSync(file,original);fs.writeFileSync(spec,yaml);
  const extension=vscode.extensions.all.find(e=>e.packageJSON.name==='readmd-vscode');assert.ok(extension);await extension.activate();
  await vscode.workspace.getConfiguration('readmd').update('executablePath',process.env.READMD_BIN,vscode.ConfigurationTarget.Global);
  await vscode.workspace.getConfiguration('readmd').update('aiAutocomplete',false,vscode.ConfigurationTarget.Global);
  document=await vscode.workspace.openTextDocument(file);editor=await vscode.window.showTextDocument(document);
  await vscode.commands.executeCommand('workbench.action.closeSidebar');await vscode.commands.executeCommand('workbench.action.closePanel');
  async function review(phase,mutate){
   editor=await vscode.window.showTextDocument(document);state(phase);
   const command=vscode.commands.executeCommand('readmd.openApiDocument',vscode.Uri.file(spec));
   await wait(path.join(out,phase+'.ready'));if(mutate)await mutate();fs.writeFileSync(path.join(out,phase+'.go'),'');
   await command;await wait(path.join(out,phase+'.clicked'));
  }
  await review('cancel');assert.equal(document.getText(),original);assert.equal(document.isDirty,false);assert.equal(fs.readFileSync(file,'utf8'),original);
  report.checks.push('native diff Cancel leaves the document and disk unchanged');
  await review('accept');const generated=document.getText();assert.match(generated,/GET `\s*\/notes\s*`/);assert.ok(generated.startsWith(original));assert.ok(document.isDirty);assert.equal(fs.readFileSync(file,'utf8'),original);
  editor=await vscode.window.showTextDocument(document);await vscode.commands.executeCommand('undo');assert.equal(document.getText(),original);await vscode.commands.executeCommand('redo');assert.equal(document.getText(),generated);assert.ok(await document.save());assert.equal(fs.readFileSync(file,'utf8'),generated);
  report.checks.push('native Accept is one undoable edit; disk changes only on Save');
  const beforeVersion=document.version;state('idempotent');await vscode.commands.executeCommand('readmd.openApiDocument',vscode.Uri.file(spec));assert.equal(document.version,beforeVersion);assert.equal(document.isDirty,false);
  report.checks.push('identical specification is idempotent and does not dirty the editor');
  fs.writeFileSync(spec,yaml.replace('List research notes','List revised research notes'));
  await review('source-conflict',()=>fs.writeFileSync(spec,yaml.replace('List research notes','A newer source revision')));assert.equal(document.getText(),generated);assert.equal(document.isDirty,false);
  report.checks.push('source changed during the actual diff cannot be applied');
  fs.writeFileSync(spec,yaml.replace('List research notes','List revised research notes'));
  await review('document-conflict',async()=>{const edit=new vscode.WorkspaceEdit();edit.insert(document.uri,new vscode.Position(0,0),'Concurrent author note\n');assert.ok(await vscode.workspace.applyEdit(edit));});
  assert.equal(document.getText(),'Concurrent author note\n'+generated);assert.equal(fs.readFileSync(file,'utf8'),generated);
  report.checks.push('concurrent author edits survive a stale Accept');
  editor=await vscode.window.showTextDocument(document);await vscode.commands.executeCommand('undo');assert.equal(document.getText(),generated);
  const start=document.getText().indexOf('List research notes');assert.ok(start>=0);await editor.edit(b=>b.replace(new vscode.Range(document.positionAt(start),document.positionAt(start+'List research notes'.length)),'Manual generated edit'));
  const manual=document.getText();state('manual-conflict');await vscode.commands.executeCommand('readmd.openApiDocument',vscode.Uri.file(spec));assert.equal(document.getText(),manual);assert.equal(fs.readFileSync(file,'utf8'),generated);
  report.checks.push('manual edits inside the generated region are preserved and updates refused');
  report.passed=true;state('passed');
 }catch(error){report.message=String(error.message).replaceAll(out,'<profile>').slice(0,900);state('failed');throw error;}
 finally{report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));}
};
