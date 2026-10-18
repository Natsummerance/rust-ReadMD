'use strict';
// Actual setup executable shares quota with real API owner processes.
// Only declared input is reserved; this does not upload two GiB of data.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),{spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/transfer-cli-native-'));
const binary=process.env.READMD_BIN,probe=process.env.READMD_TRANSFER_PROBE,owners=[];
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={passed:false,platform:process.platform,actualInstallerCommand:true,checks:[],declaredInputBytes:2*1024**3,actualUploadPayloadBytes:0};
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function exited(child,ms){
 if(child.exitCode!==null||child.signalCode!==null)return {code:child.exitCode,signal:child.signalCode};
 return new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>{child.removeListener('exit',onExit);reject(Error('Owned process exit deadline'));},ms);
  function onExit(code,signal){clearTimeout(timer);resolve({code,signal});}
  child.once('exit',onExit);
 });
}
(async()=>{try{
 assert.ok(binary&&probe,'Explicit offline executable paths required');
 const assets=path.join(root,'assets'),env={...process.env,PATH:path.join(process.env.SystemRoot,'System32'),READMD_DATA_DIR:path.join(data,'profile')};
 fs.mkdirSync(path.join(data,'documents'));
 report.binarySha256=hash(binary);report.driverSha256=hash(probe);report.scriptSha256=hash(__filename);
 for(const name of ['bytes-one','bytes-two']){
  const fd=fs.openSync(path.join(data,name+'.log'),'wx');
  let child;try{child=spawn(probe,['--holder',data,assets,name,String(1024**3)],{env,windowsHide:true,stdio:['ignore',fd,fd]});}finally{fs.closeSync(fd);}
  owners.push({name,child});const end=Date.now()+30000;
  while(!fs.existsSync(path.join(data,name+'.ready'))){assert.equal(child.exitCode,null,'API owner exited before ready');assert.ok(Date.now()<end,'API readiness deadline');await delay(10);}
 }
 report.checks.push('Two independent processes started actual uploads in the same application profile and reserve the complete declared input budget.');
 const started=Date.now(),cli=spawn(binary,['--install-capability','typst','--assets',assets],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
 let stdout='',stderr='';cli.stdout.on('data',b=>{stdout=(stdout+b).slice(0,16000);});cli.stderr.on('data',b=>{stderr=(stderr+b).slice(0,16000);});
 let result;try{result=await exited(cli,5000);}catch(error){cli.kill();await exited(cli,2000).catch(()=>{});throw error;}
 report.rejectionMs=Date.now()-started;assert.notEqual(result.code,0);assert.match(stdout+stderr,/capability_install_busy/);
 fs.writeFileSync(path.join(data,'installer-diagnostic.txt'),(stdout+stderr).replaceAll(data,'<owned-qa>').replaceAll(root,'<workspace>'));
 assert.ok(!fs.existsSync(path.join(data,'profile/capabilities/downloads')));assert.ok(!fs.existsSync(path.join(data,'profile/capabilities/managed-v2/typst/0.15.1')));
 report.checks.push('The actual installer CLI rejects the saturated shared quota before starting a download or installing an engine.');
 const transfers=path.join(data,'profile/capabilities/managed-v2/transfers');assert.equal(fs.readdirSync(transfers).length,2);
 report.cancellationMs=[];
 for(const {name,child}of owners){const start=Date.now();fs.writeFileSync(path.join(data,name+'.stop'),'cancel');assert.equal((await exited(child,2000)).code,0);report.cancellationMs.push(Date.now()-start);}
 assert.deepEqual(fs.readdirSync(transfers),[]);report.checks.push('Normal owner cancellation releases both real upload payloads and reservations without a CLI-created temporary download.');
 report.passed=true;
}catch(error){report.error=String(error.message).replaceAll(data,'<owned-qa>').replaceAll(root,'<workspace>').slice(0,1200);process.exitCode=1;}
finally{
 for(const {child}of owners)if(child.exitCode===null&&child.signalCode===null){child.kill();await exited(child,2000).catch(()=>{});}
 report.recordedAt=new Date().toISOString();report.scope='Actual Windows setup command and independent Rust API upload owners, declared budget only. Not a network-download success, GUI installer, desktop/MCP transport, 2 GiB payload, old-client or full release certification.';
 fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json'))}));
}})();
