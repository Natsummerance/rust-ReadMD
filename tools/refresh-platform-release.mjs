// Explicit operator workflow for the remaining six V0.0.6 artifacts. The
// immutable release tag and the independently accepted Mac packages stay put.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {pipeline} from 'node:stream/promises';
import {inspectPlatformPackages} from './inspect-p0-platform-packages.mjs';
import {MAC_NAMES,refreshPlatforms,mergedPlatformChecksums} from './refresh-macos-release.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const [runText,sourceCommit]=process.argv.slice(2),runId=Number(runText),token=process.env.GH_TOKEN;
if(!process.env.GITHUB_ACTIONS||!token||!Number.isSafeInteger(runId)||runId<=0||!/^[a-f\d]{40}$/.test(sourceCommit))throw Error('Explicit CI run/source and repository credential required');
const base='repos/Natsummerance/rust-ReadMD',own=path.join(root,'.cache/platform-refresh'),directory=path.join(own,'packages');
fs.mkdirSync(directory,{recursive:true});
const save=(name,value)=>fs.writeFileSync(path.join(own,name),typeof value==='string'?value:JSON.stringify(value,null,2)+'\n',{flag:'wx',mode:0o600});
const digest=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const headers={Authorization:'Bearer '+token,Accept:'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28'};
async function request(method,route,data,{file}={}) {
 const url=route.startsWith('https://uploads.github.com/')?route:'https://api.github.com/'+route;
 if(!/^https:\/\/(api|uploads)\.github\.com\/repos\/Natsummerance\/rust-ReadMD\//.test(url))throw Error('Foreign repository endpoint refused');
 const response=await fetch(url,{method,headers:{...headers,'Content-Type':file?'application/octet-stream':'application/json'},body:file?await fs.openAsBlob(file):data===undefined?undefined:JSON.stringify(data),signal:AbortSignal.timeout(file?600000:60000)});
 if(!response.ok)throw Error('Repository '+method+' HTTP '+response.status);
 return response.status===204?null:response.json();
}
const run=await request('GET',base+'/actions/runs/'+runId);
if(run.head_sha!==sourceCommit||run.status!=='completed'||run.conclusion!=='success'||run.event!=='workflow_dispatch'||run.path!=='.github/workflows/release.yml')throw Error('The selected release build has not passed');
const macProof=JSON.parse(fs.readFileSync(path.join(root,'docs/reviews/readmd-v006-p0-macos-closeout-2026-11-03.json')));
execFileSync('git',['diff','--exit-code','--quiet',macProof.sourceCommit,sourceCommit,'--','assets','rust','VERSION'],{cwd:root});
const before=await request('GET',base+'/releases/tags/V0.0.6');
if(before.assets.some(a=>a.name.startsWith('platform-refresh-')))throw Error('Previous platform refresh requires inspection before retry');
for(const name of MAC_NAMES)if(before.assets.find(a=>a.name===name)?.digest!=='sha256:'+macProof.packages.find(a=>a.name===name).sha256)throw Error('Accepted Mac package changed');
save('release-before.json',before);save('build-run.json',run);
const artifacts=(await request('GET',base+'/actions/runs/'+runId+'/artifacts?per_page=100')).artifacts;
const required={
 'ReadMD-windows-x64':['ReadMD-windows-x64.zip','ReadMDSetup-windows-x64.exe'],
 'ReadMD-linux-x86_64':['ReadMD-linux-x86_64.deb','ReadMD-linux-x86_64.tar.gz'],
 'ReadMD-integrations':['readmd-vscode-0.0.6.vsix','readmd-mcp-server-0.0.6.zip'],
};
const downloads=[];
for(const [name,members] of Object.entries(required)){
 const matches=artifacts.filter(a=>a.name===name&&!a.expired);if(matches.length!==1||!/^sha256:[a-f\d]{64}$/.test(matches[0].digest))throw Error('Missing or ambiguous source artifact');
 const artifact=matches[0],file=path.join(own,name+'.zip');
 const redirect=await fetch('https://api.github.com/'+base+'/actions/artifacts/'+artifact.id+'/zip',{headers,redirect:'manual',signal:AbortSignal.timeout(60000)});
 if(redirect.status!==302)throw Error('Artifact redirect HTTP '+redirect.status);
 const location=new URL(redirect.headers.get('location'));
 if(location.protocol!=='https:'||!(/\.blob\.core\.windows\.net$|\.githubusercontent\.com$|\.github\.com$/.test(location.hostname)))throw Error('Unexpected delivery endpoint');
 const reply=await fetch(location,{signal:AbortSignal.timeout(600000)});if(!reply.ok)throw Error('Artifact delivery HTTP '+reply.status);
 await pipeline(reply.body,fs.createWriteStream(file,{flags:'wx',mode:0o600}));
 if('sha256:'+digest(file)!==artifact.digest)throw Error('Artifact digest mismatch');
 const entries=execFileSync('unzip',['-Z1',file],{encoding:'utf8'}).trim().split(/\r?\n/).sort();
 if(JSON.stringify(entries)!==JSON.stringify([...members].sort()))throw Error('Unexpected artifact members');
 for(const member of members){const fd=fs.openSync(path.join(directory,member),'wx',0o600);try{execFileSync('unzip',['-p',file,member],{stdio:['ignore',fd,'pipe']});}finally{fs.closeSync(fd);}}
 downloads.push({name,id:artifact.id,digest:artifact.digest,size:artifact.size_in_bytes});
 fs.unlinkSync(file);
}
save('source-artifacts.json',downloads);
const verification=inspectPlatformPackages({directory,sourceCommit,buildRunId:runId,scratch:own});
verification.tagCommit=macProof.tagCommit;
if(verification.bootSha256!==macProof.bootSha256)throw Error('Runtime differs from native Mac acceptance');
save('verification.json',verification);
fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),mergedPlatformChecksums(before,verification.packages),{flag:'wx'});
save('SHA256SUMS.txt',fs.readFileSync(path.join(directory,'SHA256SUMS.txt'),'utf8'));
let previous=before.body||'',separator='\n---\n\n';
if(previous.startsWith('## macOS P0 修复更新')&&previous.includes(separator))previous=previous.slice(previous.indexOf(separator)+separator.length);
const notes=`## V0.0.6 P0 修复更新 · 2026-11-03

Windows x64 ZIP/安装程序、Linux x86_64 tar.gz/DEB，以及 VS Code/MCP 集成包已同步更新；四个 Mac ZIP/DMG 保留已完成原生验收的本轮 P0 候选。版本仍为 0.0.6，原标签不移动。请重新下载，正常退出后更新安装，并保留应用数据备份。

- 标签关闭、退出编辑、取消/不保存、回主页、新建及真正退出共用完整操作边界；确认绑定文档、修订及编辑 generation，旧请求不会丢掉新输入或保存到其他页面。
- 缺少恢复正文时保留失败页及权威描述，提供重试/历史入口，不做空正文假保存。前端及 Rust 会话 CAS 的修复由各桌面平台共用。
- 支持范围内的待审图片跨目录另存保留必要资源和恢复投影，重开可撤回，保存不等于同意 AI。无法精确映射的手工图片历史仍安全拒绝搬迁，完整映射列入 P1。
- 审阅库格式提升至 6；实际旧 V0.0.6 会拒绝读写新格式。Mac 新增真实卷容量/分配单位准入及跨 profile 安装协调，受控 ENOSPC/取消/新进程恢复保护原文和旧包。

验证：Windows、Linux 各在自身 CI 系统运行正式发行二进制，30 条真实 HTTP/浏览器生命周期回归分别通过；跨平台 Rust、前端、VS Code/MCP 和 UI 质量门槛均通过。安装程序与便携包中的可执行文件及修复资源逐字节核对，DEB 与 tar 包同样核对。此处浏览器证据不冒充 Windows/Linux 原生窗口、系统选择器或真实 IME 实测。集成包中的 MCP 仍是文档/配置工具包，服务来自更新后的 Rust 程序。

Mac 最终宿主 274 项检查、四包 8 次冷启动/重开、真实 PowerBox 另存、六类编码及公开 ARM 下载核对已通过，本机应用已更新。Apple Silicon 为原生；Intel 为 Rosetta，实体 Intel 待验。Mac 仍为免费 ad-hoc 签名，没有 Developer ID/公证。非 Windows 严格组重命名仍明确拒绝，未删除安全保护。

Mac 修复源码：[${macProof.sourceCommit.slice(0,8)}](https://github.com/Natsummerance/rust-ReadMD/commit/${macProof.sourceCommit})；其他平台构建：[${sourceCommit.slice(0,8)}](https://github.com/Natsummerance/rust-ReadMD/commit/${sourceCommit})（Rust/前端 Git 源码相同，仅增加验收流水线与测试；Windows 检出为 CRLF，记录其独立实际 boot 摘要并精确核对换行差异）。[完整构建与测试](${run.html_url})。所有十个包的 SHA-256 以更新后的 SHA256SUMS.txt 为准。虚拟机仅调研，未安装；75 条完整合同不因本次限定修复自动验收。P1、P2 全部完成后才发布 V0.0.7。

---

${previous}`;
save('release-notes.md',notes);
const journalFile=path.join(own,'publication-journal.json');
const checkpoint=journal=>{const temp=journalFile+'.tmp',fd=fs.openSync(temp,'wx',0o600);try{fs.writeFileSync(fd,JSON.stringify(journal,null,2)+'\n');fs.fsyncSync(fd);}finally{fs.closeSync(fd);}fs.renameSync(temp,journalFile);};
const result=await refreshPlatforms({before,directory,sourceCommit,tagCommit:verification.tagCommit,notes,verification,request,checkpoint});
const after=await request('GET',base+'/releases/tags/V0.0.6');save('release-after.json',after);
const publicDownloads=[];
for(const name of [...verification.packages.map(p=>p.name),'SHA256SUMS.txt']){
 const asset=after.assets.find(a=>a.name===name);if(!asset||asset.state!=='uploaded')throw Error('Canonical asset missing');
 const reply=await fetch(asset.browser_download_url,{headers:{'Cache-Control':'no-cache'},signal:AbortSignal.timeout(300000)});if(!reply.ok)throw Error('Public download HTTP '+reply.status);
 const hash=crypto.createHash('sha256');let size=0;for await(const chunk of reply.body){size+=chunk.length;hash.update(chunk);}
 const sha256=hash.digest('hex');if(size!==asset.size||asset.digest!=='sha256:'+sha256)throw Error('Anonymous public download mismatch: '+name);
 publicDownloads.push({name,size,sha256,passed:true});
}
save('public-downloads.json',publicDownloads);
save('result.json',{status:result.status,sourceCommit,ciRunId:runId,tagPreserved:true,macPackagesPreserved:true,packagesReplaced:6,publicDownloadsVerified:publicDownloads.length,retainedBackups:result.retainedBackups.length,release:after.html_url});
// Keep small receipts, original/final inventories and the durable journal.
for(const file of fs.readdirSync(directory))fs.unlinkSync(path.join(directory,file));fs.rmdirSync(directory);
console.log(JSON.stringify({status:result.status,packagesReplaced:6,publicDownloadsVerified:publicDownloads.length,tagPreserved:true,macPackagesPreserved:true}));
