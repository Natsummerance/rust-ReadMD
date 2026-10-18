// Runs inside the real VS Code Extension Host with a private profile.
const fs=require('fs'),path=require('path'),assert=require('assert/strict'),vscode=require('vscode');
exports.run=async()=>{
 const out=process.env.READMD_VSCODE_AI_AUDIT,report={passed:false,checks:[],realProvider:true,syntheticOnly:true};
 const write=phase=>fs.writeFileSync(path.join(out,'state.json'),JSON.stringify({phase}));
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 try{
  const file=path.join(out,'materials','研究札记.md'),source='# 研究札记\n\n好的研究从观察开始，为了检验我们的判断，我们应该';
  fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,source);
  const extension=vscode.extensions.all.find(e=>e.packageJSON.name==='readmd-vscode');assert.ok(extension);await extension.activate();
  await vscode.workspace.getConfiguration('readmd').update('aiAutocomplete',false,vscode.ConfigurationTarget.Global);
  // VS Code normalizes the drive/path casing; use the loaded module to avoid
  // a second Windows require-cache entry for the same physical file.
  const loaded=Object.keys(require.cache).find(p=>p.toLowerCase().replaceAll('\\','/').endsWith('/readmd/packages/vscode-extension/out/bridge.js'));
  const {ReadMDBridge}=loaded?require.cache[loaded].exports:require(path.join(extension.extensionPath,'out/bridge'));
  const original=ReadMDBridge.prototype.callMcpTool;report.requests=[];
  ReadMDBridge.prototype.callMcpTool=async function(name,...args){
   if(name!=='readmd_ai_complete')return original.call(this,name,...args);
   const began=Date.now();try{const result=await original.call(this,name,...args);report.requests.push({elapsedMs:Date.now()-began,characters:result.completion?.length||0});return result;}
   catch(e){report.requests.push({elapsedMs:Date.now()-began,error:e.message.slice(0,160)});throw e;}
   finally{fs.writeFileSync(path.join(out,'requests.json'),JSON.stringify(report.requests));}
  };
  const doc=await vscode.workspace.openTextDocument(file),editor=await vscode.window.showTextDocument(doc);
  const pos=doc.positionAt(source.length);editor.selection=new vscode.Selection(pos,pos);
  report.environment={trusted:vscode.workspace.isTrusted,enabled:vscode.workspace.getConfiguration('readmd').get('aiAutocomplete'),language:doc.languageId};
  await vscode.commands.executeCommand('workbench.action.closeSidebar');await vscode.commands.executeCommand('workbench.action.closePanel');
  await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
  await vscode.workspace.getConfiguration('readmd').update('aiAutocomplete',true,vscode.ConfigurationTarget.Global);
  report.environment.enabled=vscode.workspace.getConfiguration('readmd').get('aiAutocomplete');
  write('ready');while(!fs.existsSync(path.join(out,'trigger.json')))await sleep(100);
  await vscode.commands.executeCommand('editor.action.inlineSuggest.trigger');write('waiting-for-real-ghost');
  const deadline=Date.now()+60000;while(doc.getText()===source&&Date.now()<deadline)await sleep(100);
  assert.ok(doc.getText().length>source.length,'Native Tab must accept the actual provider suggestion');
  assert.ok(doc.getText().startsWith(source));assert.ok(doc.getText().slice(source.length).trim().length>3,'A Tab indent is not a completion');
  assert.ok(report.requests.some(r=>r.characters>3),'This run must call the real shared Rust provider');
  report.checks.push('real inline provider, MCP and Rust response accepted by native Tab');
  await vscode.commands.executeCommand('undo');assert.equal(doc.getText(),source);assert.equal(fs.readFileSync(file,'utf8'),source);
  report.checks.push('one native undo restores draft; disk original untouched');report.passed=true;write('passed');
 }catch(e){report.failure=e.message;write('failed');throw e;}
 finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));fs.rmSync(path.join(out,'data','ai.json'),{force:true});}
};
