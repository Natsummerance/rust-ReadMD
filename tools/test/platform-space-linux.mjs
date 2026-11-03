// Fill only this fresh, bounded CI tmpfs; never fill a host/user filesystem.
import fs from 'node:fs';import path from 'node:path';import {spawnSync} from 'node:child_process';import {fileURLToPath} from 'node:url';
if(process.platform!=='linux'||!process.env.CI||!process.env.GITHUB_WORKSPACE)throw Error('Dedicated Linux CI required');
const repo=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');if(fs.realpathSync(repo)!==fs.realpathSync(process.env.GITHUB_WORKSPACE))throw Error('Unexpected workspace');
const out=path.join(repo,'.cache/p1-platform-space'),prepared=JSON.parse(fs.readFileSync(path.join(out,'prepared.json'))),mount=path.join(out,'mount'),tmp=path.join(out,'tmp'),report=path.join(out,'native-space.json'),extra=path.join(out,'linux-extra.json');
fs.mkdirSync(mount);fs.mkdirSync(tmp);const env=Object.fromEntries(Object.entries(process.env).filter(([key])=>!/TOKEN|SECRET|CREDENTIAL|API_KEY/i.test(key)));
Object.assign(env,{TMPDIR:tmp,SQLITE_TMPDIR:tmp,READMD_SPACE_OWNED_VOLUME:mount,READMD_SPACE_INSTALL_ASSETS:path.join(repo,prepared.assets),READMD_SPACE_INSTALL_PACK:path.join(repo,prepared.pack),READMD_SPACE_REPORT_PATH:report,READMD_SPACE_EXTRA_REPORT_PATH:extra});
const run=(program,args,childEnv=env,log)=>{const r=spawnSync(program,args,{cwd:repo,env:childEnv,encoding:'utf8',timeout:60000,maxBuffer:1024*1024});if(log)fs.writeFileSync(path.join(out,log),(r.stdout||'')+(r.stderr||''),{flag:'wx'});if(r.error||r.status!==0){
 const first=path.join(out,'first-failure.json');if(!fs.existsSync(first)){fs.writeFileSync(first,JSON.stringify({passed:false,program:path.basename(program),phase:log||'volume_setup_or_cleanup',exit:r.status,sourceCommit:prepared.sourceCommit,modelsRequested:false},null,2)+'\n',{flag:'wx'});fs.writeFileSync(path.join(out,'first-failure.log'),(r.stdout||'')+(r.stderr||''),{flag:'wx'});}
 throw Error(path.basename(program)+' owned volume phase failed');}return r.stdout;};
let mounted=false;
try{
 const uid=Number(run('/usr/bin/id',['-u']).trim()),gid=Number(run('/usr/bin/id',['-g']).trim());if(!Number.isSafeInteger(uid)||uid===0||!Number.isSafeInteger(gid))throw Error('Unprivileged quota caller required');
 run('/usr/bin/sudo',['-n','--','/usr/bin/mount','-t','tmpfs','-o',`size=192m,nr_inodes=2048,mode=0700,uid=${uid},gid=${gid},nodev,nosuid,noexec`,'tmpfs',mount]);mounted=true;
 fs.writeFileSync(path.join(mount,'.readmd-owned-small-volume-v1'),'ReadMD P0 capacity evidence only\n',{flag:'wx'});
 const binary=path.join(repo,prepared.binary),args=['--exact','capability_packs::space_volume_tests::native_small_volume_preflight_enospc_cancel_and_reopen','--ignored','--nocapture','--test-threads=1'];
 run(binary,args,env,'first-process.log');run(binary,args,{...env,READMD_SPACE_REOPEN_ONLY:'1'},'reopen-process.log');
 run(binary,['--exact','capability_packs::space_volume_tests::native_small_volume_bind_mount_and_inode_exhaustion','--ignored','--nocapture','--test-threads=1'],env,'mount-inode-process.log');
 const value=JSON.parse(fs.readFileSync(report)),additional=JSON.parse(fs.readFileSync(extra));if(!value.passed||!value.native_enospc||!value.new_process_reopened||!value.pending_review_retained||!additional.passed)throw Error('Missing native Linux consumer proof');
 Object.assign(value,{sourceCommit:prepared.sourceCommit,kernel_test_sha256:prepared.kernelTestSha256,offline_pack_sha256:prepared.offlinePackSha256,linux_bind_mount_and_inodes:true,model_inference:false,hard_user_or_project_quota_certified:false});fs.writeFileSync(report,JSON.stringify(value,null,2)+'\n');
}finally{
 if(mounted){run('/usr/bin/sudo',['-n','--','/usr/bin/umount',mount]);mounted=false;}
 if(fs.readdirSync(mount).length)throw Error('Preserve unexpected mount namespace');fs.rmdirSync(mount);fs.rmSync(tmp,{recursive:true});
}
console.log(JSON.stringify({passed:true,nativeLinuxEnospc:true,newProcessReopened:true,bindMountAndInodes:true,ownedMountRemoved:true,modelsRequested:false}));
