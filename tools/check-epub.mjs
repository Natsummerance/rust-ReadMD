// Run the pinned independent validator offline; never installs or downloads.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath} from 'node:url';
const run=promisify(execFile);
export async function checkEpub(files,{jar,output,java='java'}={}) {
  if(!jar||!files.length)throw Error('Supply the installed EPUBCheck 5.4.0 jar and EPUB files.');
  jar=path.resolve(jar);
  const lock=JSON.parse(fs.readFileSync(new URL('./epubcheck.lock.json',import.meta.url)));
  for(const [file,expected]of Object.entries(lock.runtimeFiles)){
    const actual=crypto.createHash('sha256').update(fs.readFileSync(path.join(path.dirname(jar),file))).digest('hex');
    if(actual!==expected)throw Error('EPUBCheck runtime checksum differs from the reviewed distribution: '+file);
  }
  const version=await run(java,['-jar',jar,'--version'],{windowsHide:true,timeout:20000,maxBuffer:1024*1024});
  if(!version.stdout.includes('EPUBCheck v5.4.0'))throw Error('EPUBCheck version differs from the reviewed 5.4.0 distribution.');
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-epubcheck-')),results=[];
  try {
    for(const [index,file]of files.entries()){
      const target=path.resolve(file),reportFile=path.join(temporary,index+'.json');
      const bytes=fs.readFileSync(target);let exitCode=0;
      try{await run(java,['-jar',jar,target,'--json',reportFile],{windowsHide:true,timeout:90000,maxBuffer:2*1024*1024});}
      catch(error){if(typeof error.code!=='number')throw Error('EPUBCheck failed to execute or exceeded its time limit.');exitCode=error.code;}
      if(!fs.existsSync(reportFile))throw Error('EPUBCheck produced no report.');
      const raw=JSON.parse(fs.readFileSync(reportFile,'utf8')),counts=raw.checker;
      const messages=raw.messages.map(message=>({id:message.ID||message.id,severity:message.severity,message:message.message,
        locations:message.locations?.map(l=>({path:l.path?.replaceAll(target,path.basename(target)),line:l.line,column:l.column}))}));
      const passed=exitCode===0&&counts.nFatal===0&&counts.nError===0&&counts.nWarning===0;
      results.push({name:path.basename(target),sha256:crypto.createHash('sha256').update(bytes).digest('hex'),passed,exitCode,
        counts:{fatal:counts.nFatal,error:counts.nError,warning:counts.nWarning},messages,elapsedMs:counts.elapsedTime});
    }
  }finally{
    if(path.dirname(path.resolve(temporary))!==path.resolve(os.tmpdir())||!path.basename(temporary).startsWith('readmd-epubcheck-'))throw Error('Unexpected validator temporary directory.');
    fs.rmSync(temporary,{recursive:true,force:true});
  }
  const report={validator:'EPUBCheck',version:'5.4.0',jarSha256:crypto.createHash('sha256').update(fs.readFileSync(jar)).digest('hex'),
    offline:true,passed:results.every(r=>r.passed),files:results,recordedAt:new Date().toISOString()};
  if(output){fs.mkdirSync(path.dirname(path.resolve(output)),{recursive:true});fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');}
  return report;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const args=process.argv.slice(2),options={jar:process.env.READMD_EPUBCHECK_JAR},files=[];
  while(args.length){const arg=args.shift();if(arg==='--jar')options.jar=args.shift();else if(arg==='--out')options.output=args.shift();else files.push(arg);}
  try{const result=await checkEpub(files,options);console.log(JSON.stringify({passed:result.passed,files:result.files.map(f=>({name:f.name,passed:f.passed,counts:f.counts}))}));if(!result.passed)process.exitCode=1;}
  catch(error){console.error(error.message);process.exitCode=1;}
}
