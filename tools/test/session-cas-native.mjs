// Run a genuine two-process CAS repeatedly. Stop at the first failure; no
// retries turn a failed invocation into a passing receipt. No models or UI.
import fs from 'node:fs';import path from 'node:path';import {spawnSync}from'node:child_process';
if(!process.env.CI||!process.env.GITHUB_WORKSPACE)throw Error('Dedicated CI fixture required');
const own=path.resolve('.cache/p1-session-cas');if(fs.existsSync(own))throw Error('CAS evidence already exists');fs.mkdirSync(own,{recursive:true});
const repetitions=process.platform==='win32'?128:16;
const report={passed:false,platform:process.platform,sourceCommit:process.env.GITHUB_SHA,modelsRequested:false,expectedRepeatedPasses:repetitions,passedInvocations:0};
try {
 for(let i=0;i<repetitions;i++){
  const result=spawnSync('cargo',['test','--offline','--locked','--manifest-path','rust/Cargo.toml','-p','readmd-kernel','--lib','settings_persistence::tests::session_two_processes_with_one_expected_version_have_exactly_one_winner','--','--exact','--test-threads=1','--nocapture'],{encoding:'utf8',maxBuffer:8*1024*1024,windowsHide:true});
  if(result.status!==0||result.error||result.signal||!/^test result: ok\. 1 passed; 0 failed;/m.test(result.stdout||'')){
   report.failedInvocation=i+1;const log=(result.stdout||'')+(result.stderr||'');fs.writeFileSync(path.join(own,'first-failure.log'),log,{flag:'wx'});process.stdout.write(log);throw Error('Actual session CAS invocation failed');
  }report.passedInvocations++;console.log('Actual two-process session CAS passed: '+(i+1)+'/'+repetitions);
 }
 report.passed=true;
}catch{process.exitCode=1;}
finally{fs.writeFileSync(path.join(own,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report));}
