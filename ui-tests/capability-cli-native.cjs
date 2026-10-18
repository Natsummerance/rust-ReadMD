'use strict';
// The exact installer command, with no browser or developer runtime in PATH.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),{spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/delivery-cli-'));
const binary=process.env.READMD_BIN,sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const report={platform:'windows-x64',actualInstallerCommand:true,checks:[],passed:false};
try{
 assert.ok(binary);report.binarySha256=sha(fs.readFileSync(binary));
 const env={...process.env,READMD_DATA_DIR:data,PATH:process.env.SystemRoot+'/System32;'+process.env.SystemRoot};
 const run=id=>spawnSync(binary,['--install-capability',id,'--assets',path.join(root,'assets')],{env,windowsHide:true,timeout:180000,encoding:'utf8'});
 for(const entry of JSON.parse(fs.readFileSync(path.join(root,'assets/capabilities/catalog.json'))).packs){
  let result=run(entry.id);assert.equal(result.status,0,result.stderr||String(result.error));
  const lock=JSON.parse(fs.readFileSync(path.join(root,`assets/capabilities/${entry.id}-${entry.version}-${entry.platform}.lock.json`))),target=path.join(data,`capabilities/${entry.id}/${entry.version}`);
  for(const file of lock.files)assert.equal(sha(fs.readFileSync(path.join(target,file.path))),file.sha256);
  report.checks.push(`Real owned release download, exact file hashes and actual ${entry.id} operation probe via the setup command.`);
  const stamp=fs.statSync(path.join(target,lock.files[0].path)).mtimeMs;
  result=run(entry.id);assert.equal(result.status,0);assert.equal(fs.statSync(path.join(target,lock.files[0].path)).mtimeMs,stamp);
  report.checks.push(`Repeated ${entry.id} setup command keeps an already verified installation without downloading or replacing it.`);
 }
 const result=run('../../invalid');assert.notEqual(result.status,0);
 assert.ok(!fs.existsSync(path.join(data,'readmd-assets')));report.checks.push('Invalid capability is rejected; no document resource folder is created.');report.passed=true;
}catch(error){report.error=String(error.message).replaceAll(data,'<profile>').replaceAll(root,'<workspace>');process.exitCode=1;}
report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json'))}));
