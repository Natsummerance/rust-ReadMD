// Compile the real installer with isolated absolute input/output paths. This
// does not execute an installer, touch the registry or install onto the PC.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {execFileSync} from 'node:child_process';
const [compiler,binary]=process.argv.slice(2);if(!compiler||!binary)throw Error('Provide local makensis.exe and ReadMD binary');
const root=process.cwd(),directory=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/installer-compile-'));
let script=fs.readFileSync('ReadMD.nsi','utf8');
const inputs={'ReadMD.exe':path.resolve(binary),'vcruntime140.dll':path.join(root,'assets/runtimes/windows-x64/vcruntime140.dll'),'vcruntime140_1.dll':path.join(root,'assets/runtimes/windows-x64/vcruntime140_1.dll'),'ReadMD-Pet-Rust.zip':path.join(root,'dist/ReadMD-Pet-Rust.zip'),'assets':path.join(root,'assets'),'LICENSE':path.join(root,'LICENSE'),'README.md':path.join(root,'README.md')};
for(const[name,file]of Object.entries(inputs)){if(!fs.existsSync(file))throw Error('Missing actual package input: '+name);script=script.replaceAll('dist\\ReadMD-windows-x64\\'+name,file);}
script=script.replace('OutFile "dist\\ReadMDSetup-windows-x64.exe"','OutFile "'+path.join(directory,'ReadMDSetup-test.exe')+'"');
script=script.replaceAll('"assets\\readmd.ico"','"'+path.join(root,'assets/readmd.ico')+'"').replace('"installer\\capability-components.nsh"','"'+path.join(root,'installer/capability-components.nsh')+'"');
const file=path.join(directory,'ReadMD-test.nsi');fs.writeFileSync(file,script);let passed=false;
try{const output=execFileSync(path.resolve(compiler),['/V3','/DAPP_VERSION='+fs.readFileSync('VERSION','utf8').trim(),file],{encoding:'utf8',windowsHide:true,maxBuffer:2*1024*1024});fs.writeFileSync(path.join(directory,'compiler.log'),output);passed=true;}catch(error){fs.writeFileSync(path.join(directory,'compiler.log'),String(error.stdout||'')+String(error.stderr||''));throw Error('Installer compilation failed; isolated compiler log retained');}
finally{const report={passed,compilerSha256:crypto.createHash('sha256').update(fs.readFileSync(compiler)).digest('hex'),scriptSha256:crypto.createHash('sha256').update(fs.readFileSync('ReadMD.nsi')).digest('hex'),actualInputs:true,installerExecuted:false,recordedAt:new Date().toISOString()};fs.writeFileSync(path.join(directory,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed,report:path.relative(root,path.join(directory,'report.json'))}));}
