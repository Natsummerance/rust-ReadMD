'use strict';
// Prepare-only commands never create a profile or execute a product/engine.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {createHarness,fileHash,root,stopAfterInventory}=require('../tools/test/publication-batch3-harness.cjs');
const {inspectArtifact}=require('../tools/test/publication-batch3-checks.cjs');
const checks=require('../tools/test/publication-tex-typography-checks.cjs');
const {waitCapabilityReady}=require('../tools/test/publication-capability-readiness.cjs');
const shared=require('./publication-typst-typography-native.cjs');
const base={cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},page:{size:'A4',marginTop:20,marginBottom:20,marginLeft:18,marginRight:18},
  typography:{font:'Arial',size:11,spacing:0,align:'left',firstLineIndent:0,keepLines:false},footer:{pageNumbers:false,text:''},header:{text:''}};
const engines=['xelatex','pdflatex'],options=(engine,mode)=>({...base,pdf:{engine},typography:{...base.typography,...(mode===undefined?{}:{lineSpacing:mode})}});
const paragraph=lines=>'\uFEFF'+lines.join('  \r\n')+'\r\n';
const local=(id,size,mode)=>'<!-- readmd:layout v1 '+JSON.stringify({id,text:{size},paragraph:{lineSpacing:mode}})+' -->\r\n\r\n'+shared.documents.short.slice(1);
const documents={short:shared.documents.short,long:shared.documents.long,localExact:shared.documents.localExact,localLeast:shared.documents.localLeast,localMultiple:shared.documents.localMultiple,
  tallNative:paragraph(['FIRST','SECOND $\\dfrac{X}{\\dfrac{Y}{Z}}$','THIRD']),
  localImpossible:local('local-impossible',72,{mode:'exact',pt:6}),
  sectionBad:'FIRST_SECTION ordinary legacy body.\r\n\r\n<!-- readmd-section:v1 {"id":"small","break":"nextPage","page":{"size":"A5","orientation":"landscape","marginTop":12,"marginBottom":16}} -->\r\n\r\n'+local('small-exact',72,{mode:'exact',pt:6}),
  heading:shared.documents.unsupported,office:shared.documents.tall48,missingGlyph:shared.documents.missingGlyph,
  footnote:'FIRST[^n]  \r\nSECOND  \r\nTHIRD\r\n\r\n[^n]: Stateful note must not be measured twice.\r\n',
  image:'FIRST ![Unread image](must-not-load.png)  \r\nSECOND  \r\nTHIRD\r\n',
  stateful:'FIRST $\\write18{must-not-execute}$  \r\nSECOND  \r\nTHIRD\r\n',unicode:paragraph(['FIRST','SECOND 中文😀','THIRD'])};
const rowMarkers=Array.from({length:100},(_,i)=>'P04ROW'+String(i).padStart(4,'0'));
const planned={scope:'P04 M4 ordinary native TeX body line spacing only',plannedStages:11,plannedSuccessfulPdfFiles:24,plannedNativeRestarts:1,
  engines,capabilities:['latex','fonts','pdfium'],automaticRetries:0,coldPerformanceAccepted:false,productInvocations:0,
  mathOutlineProof:'pending separate; native glyph existence/size and baseline expansion are checked',previewSourceMapAvailable:false};
const relative=file=>path.relative(root,file).replaceAll(path.sep,'/'),write=(file,value)=>fs.writeFileSync(file,JSON.stringify(value,null,2)+'\n',{flag:'wx'});
async function port(){const server=net.createServer();await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});const number=server.address().port;await new Promise(resolve=>server.close(resolve));return number;}
function inventory(h,profile,known=[]){const q=value=>"'"+value.replaceAll("'","''")+"'",ps=path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe');
  const remembered=known.length?known.map(p=>`($_.ProcessId -eq ${p.pid} -and $_.CreationDate.ToUniversalTime().ToString('o') -eq ${q(p.created)})`).join(' -or '):'$false';
  return JSON.parse(h.command(ps,['-NoProfile','-NonInteractive','-Command',`$ErrorActionPreference='Stop'\n$OwnedScope=${q(profile)}\n$All=@(Get-CimInstance Win32_Process)\n$Ids=@($All|Where-Object {($_.CommandLine -and $_.CommandLine.IndexOf($OwnedScope,[StringComparison]::OrdinalIgnoreCase)-ge 0 -and $_.Name -in @('readmd.exe','msedgewebview2.exe','xelatex.exe','pdflatex.exe','xdvipdfmx.exe','dvipdfmx.exe')) -or (${remembered})}|ForEach-Object {$_.ProcessId})\nfor($i=0;$i -lt 16;$i++){$Next=@($All|Where-Object {$Ids -contains $_.ParentProcessId -and $Ids -notcontains $_.ProcessId}|ForEach-Object {$_.ProcessId});if($Next.Count -eq 0){break};$Ids+=$Next}\n[pscustomobject]@{recordedAt=[DateTime]::UtcNow.ToString('o');processes=@($All|Where-Object {$Ids -contains $_.ProcessId}|ForEach-Object {[pscustomobject]@{pid=$_.ProcessId;parentPid=$_.ParentProcessId;name=$_.Name;created=$_.CreationDate.ToUniversalTime().ToString('o')}})}|ConvertTo-Json -Depth 4 -Compress`]));}
async function run(){
  assert.equal(process.platform,'win32');const candidatePath=process.env.READMD_M4_CANDIDATE_JSON;
  assert(candidatePath&&path.isAbsolute(candidatePath),'READMD_M4_CANDIDATE_JSON must identify the frozen absolute candidate JSON');
  const candidate=JSON.parse(fs.readFileSync(candidatePath,'utf8'));assert.equal(candidate.binarySha256,process.env.READMD_CANDIDATE_SHA256);assert.equal(candidate.bootSha256,process.env.READMD_CANDIDATE_BOOT_SHA256);
  const {freezeSources}=require('./publication-structure-native.cjs'),freeze=freezeSources(candidate),helpers=require('./publication-engines-native.cjs');
  const profile=helpers.resolveEngineReuseProfile(process.env.READMD_ENGINE_REUSE_PROFILE);assert(profile,'Explicit fixed n2e7Ug owned native data is required');
  const python=process.env.READMD_QA_PYTHON;assert(python&&path.isAbsolute(python));assert.match(process.env.READMD_QA_PYTHON_SHA256||'',/^[a-f0-9]{64}$/u);assert.equal(fileHash(python),process.env.READMD_QA_PYTHON_SHA256);
  const h=createHarness('publication-tex-typography-native',await port(),__filename,profile),currentAssets=path.resolve(process.env.READMD_ASSETS_DIR||path.join(root,'assets'));
  for(const file of ['ui-tests/publication-tex-typography-native.cjs','tools/test/publication-tex-typography-checks.cjs','tools/test/publication-tex-typography-checks.test.cjs','tools/test/publication-capability-readiness.cjs','tools/test/publication-capability-readiness.test.cjs','tools/test/publication-typst-typography-checks.cjs','ui-tests/publication-typst-typography-native.cjs','ui-tests/publication-engines-native.cjs','ui-tests/publication-structure-native.cjs'])h.report.qaDependencySha256[file]=fileHash(path.join(root,file));
  for(const file of Object.keys(h.report.qaDependencySha256))fs.copyFileSync(path.join(root,file),path.join(h.data,'qa-'+path.basename(file)));
  for(const [name,source]of Object.entries(documents))fs.writeFileSync(path.join(h.docs,name+'.md'),source,{flag:'wx'});
  const inputs=h.snapshotInputs(),responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  Object.assign(h.report,{scope:planned.scope,plan:planned,warmReuse:true,coldPerformanceAccepted:false,outputs:[],requests:[],capabilityStatus:[],
    candidateFile:{path:relative(candidatePath),sha256:fileHash(candidatePath)},sourceFreeze:{path:freeze.file,sha256:freeze.sha256,sources:freeze.sources},originalInputHashes:inputs,
    independentRuntime:{executable:'<qa-python>',sha256:fileHash(python),productDependency:false},mathFontProof:{status:'pending-separate-outline-proof',nativeBaselineAndGlyphSizeChecked:true,actualOutlineInkVerified:false}});
  h.report.limitations.push('Only ordinary native TeX paragraphs are certified by this scope. Office physical runs, stateful inline math, native external bibliography and unsupported stories are explicit negative cases. CSL spacing remains separate; this is not complete P04 certification.',
    'Exact uses a conservative paragraph envelope. line=1 names that envelope; blockIndex is a typed paragraph ordinal, not an exact UTF16/physical-line location. availableHeightPt is current section full body height, not remaining page space.',
    'TeX PDF previews expose no validated source map. Native math existence, physical glyph size and line expansion do not certify a complete measured math ink envelope.',
    'Managed native data/runtime is warm reused with original immutable proofs; there is no runtime clone, installation, download or cold-performance claim.');
  let page,reuse,primary,requestId=0,known=[];
  async function rawApi(route,body,method='POST',budget=120000){const file=path.join(responses,String(++requestId).padStart(3,'0')+'.json');let response;
    try{response=method==='POST'&&budget===120000?await h.api(route,body):await page.evaluate(async({route,body,method,budget})=>{const r=await apiFetch(route,{method,...(method==='POST'?{headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{}),signal:AbortSignal.timeout(budget)});return {status:r.status,data:await r.json()};},{route,body,method,budget});}
    catch(error){write(file,{route,method,budgetMs:budget,request:body,error:String(error.stack||error)});h.report.requests.push({route,method,budgetMs:budget,file:relative(file),sha256:fileHash(file),status:null});throw error;}
    write(file,{route,method,budgetMs:budget,request:body,response});h.report.requests.push({route,method,budgetMs:budget,file:relative(file),sha256:fileHash(file),status:response.status});return response;}
  async function api(route,body,code){const r=await rawApi(route,body);assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,!code,JSON.stringify(r.data));if(code)assert.equal(r.data.error_code,code,JSON.stringify(r.data));return r.data;}
  async function boot(){h.report.nativeStartAttempts=(h.report.nativeStartAttempts||0)+1;page=await h.start();h.report.nativeStarts=(h.report.nativeStarts||0)+1;
    await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));assert.equal(h.report.servedBootSha256,candidate.bootSha256);
    const csp=fs.readFileSync(path.join(process.env.READMD_ASSETS_DIR,'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/u)?.[1];assert(csp);
    assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),csp);h.report.productionCspUnchanged=true;}
  function inspect(file,expected){const record={path:relative(file),bytes:fs.statSync(file).size,sha256:fileHash(file),validated:false};h.report.outputs.push(record);
    // Reuse the established QA inspector, binding only this synchronous call to
    // root's pinned QA interpreter. No interpreter is a product dependency.
    const original=h.command;let value;h.command=(_executable,args,...rest)=>original(python,args,...rest);
    try{value=inspectArtifact(h,file,{glyphs:true,render:path.join(h.data,path.basename(file)+'-pages')});}finally{h.command=original;}
    record.inspection=relative(path.join(h.data,path.basename(file)+'.inspection.json'));record.inspectionSha256=fileHash(path.join(root,record.inspection));
    if(expected)record.proof=checks.checkPdf(value,expected);record.validated=true;return value;}
  async function exported(name,source,settings,expected,suggestedName=name){const file=path.join(h.outputs,name+'.pdf');await api('/api/export',{format:'pdf',content:source,baseDir:h.docs,out_path:file,suggestedName,options:settings});return {file,inspection:inspect(file,expected)};}
  async function preview(name,source,engine,settings,expected){const value=await api('/api/export/preview',{format:'pdf',content:source,baseDir:h.docs,suggestedName:name,options:settings});
    const proof=checks.checkPreview(value,engine),file=path.join(h.outputs,name+'-preview.pdf');fs.writeFileSync(file,Buffer.from(value.pdf,'base64'),{flag:'wx'});const inspection=inspect(file,expected);h.report.outputs.at(-1).previewProof=proof;return {file,inspection};}
  async function protectedFailure(label,source,settings,code,geometry){const file=path.join(h.outputs,'protected-'+label+'.pdf'),before=Buffer.from('Original owned output: '+label);fs.writeFileSync(file,before,{flag:'wx'});
    const request={format:'pdf',content:source,baseDir:h.docs,out_path:file,suggestedName:label,options:settings},error=await api('/api/export',request,code);assert.deepEqual(fs.readFileSync(file),before,'Failed export replaced the old target');
    const {out_path,...previewRequest}=request,previewError=await api('/api/export/preview',previewRequest,code);assert.equal(previewError.key,undefined);assert.equal(previewError.pdf,undefined);
    if(geometry){checks.checkGeometry(error,geometry);checks.checkGeometry(previewError,geometry);}
    h.report.protectedTargets??=[];h.report.protectedTargets.push({label,path:relative(file),sha256:fileHash(file),unchanged:true,exportError:error,previewError});h.unchanged(inputs);}
  await h.finish(async()=>{try{
    await h.step('fixed owned profile, old immutable proofs and fresh frozen-assets overlay',()=>{reuse=helpers.prepareEngineProfileReuse(h,profile,currentAssets);h.report.preStartProcesses=inventory(h,profile);assert.deepEqual(h.report.preStartProcesses.processes,[]);});
    await h.step('real native candidate, served boot, production CSP and current locked TeX/fonts/PDFium readiness',async()=>{await boot();for(const id of planned.capabilities){
      if(id==='latex'){const start=Date.now(),r=await rawApi('/api/capabilities/packs',{action:'status',id});assert.equal(r.status,200);assert.equal(r.data.installed,true,JSON.stringify(r.data));assert.equal(r.data.ready,true,JSON.stringify(r.data));h.report.capabilityStatus.push({id,elapsedMs:Date.now()-start,response:r.data});}
      else await waitCapabilityReady({id,budget:120000,request:remaining=>rawApi('/api/capabilities/packs',{action:'status',id},'POST',remaining),observe:observation=>h.report.capabilityStatus.push({...observation,response:observation.response.data,httpStatus:observation.response.status})});}
      h.report.engineProfileReuse.currentRunReadinessCapabilities=[...planned.capabilities];h.report.engineProfileReuse.actualProductReadinessReexecuted=true;});
    const modes=[['exact-18',{mode:'exact',pt:18},18],['multiple-2',{mode:'multiple',multiple:2},22],['least-30',{mode:'atLeast',pt:30},30]];
    await h.step('both native engines export all three physical body baseline modes',async()=>{for(const engine of engines)for(const [name,mode,gap]of modes)await exported(engine+'-'+name,documents.short,options(engine,mode),{gap});h.unchanged(inputs);});
    await h.step('both actual PDF previews retain physical baselines and honestly omit unavailable source maps',async()=>{for(const engine of engines)await preview(engine+'-exact-18',documents.short,engine,options(engine,{mode:'exact',pt:18}),{gap:18});});
    await h.step('local 24pt text retains absolute exact/minimum and local multiplier intervals in both engines',async()=>{for(const engine of engines)for(const [key,name,gap]of [['localExact','local-exact-30',30],['localLeast','local-least-30',30],['localMultiple','local-multiple-36',36]])await exported(engine+'-'+name,documents[key],options(engine,{mode:'exact',pt:20}),{gap,fontSize:24});});
    await h.step('native nested display fractions retain all three 11pt math glyphs and expand minimum lines',async()=>{for(const engine of engines){const result=await exported(engine+'-native-math-minimum',documents.tallNative,options(engine,{mode:'atLeast',pt:18}),{gap:18,minimum:true,nativeMathFixture:true,engine});
      const record=h.report.outputs.at(-1);record.validated=false;record.mathBaselineProof=checks.checkMathMinimum(result.inspection,engine);record.validated=true;}});
    await h.step('a single original 100-line paragraph really paginates without loss or exact-baseline drift',async()=>{for(const engine of engines)await exported(engine+'-long-exact-18',documents.long,options(engine,{mode:'exact',pt:18}),{gap:18,markers:rowMarkers,long:true});});
    await h.step('trusted geometry, changed-section dimensions and explicit unsupported/invalid/missing glyph paths protect old targets',async()=>{for(const engine of engines){
      await protectedFailure(engine+'-tall-exact',documents.tallNative,options(engine,{mode:'exact',pt:18}),'publication_layout_geometry',{requested:18});
      await protectedFailure(engine+'-local-exact',documents.localImpossible,options(engine),'publication_layout_geometry',{requested:6,id:'local-impossible'});
      await protectedFailure(engine+'-section-exact',documents.sectionBad,options(engine),'publication_layout_geometry',{requested:6,id:'small-exact',width:174*72/25.4,height:120*72/25.4});
      for(const key of ['heading','footnote','image','office','stateful'])await protectedFailure(engine+'-'+key,documents[key],options(engine,{mode:'exact',pt:18}),'publication_line_spacing_unsupported');
      await protectedFailure(engine+'-native-bib',documents.short,{...options(engine,{mode:'exact',pt:18}),tex:{bibEngine:'biblatex'}},'publication_line_spacing_unsupported');
      await protectedFailure(engine+'-invalid-union',documents.short,{...options(engine),typography:{lineHeight:1.6,lineSpacing:{mode:'exact',pt:18}}},'publication_local_style_invalid');
    }
    await protectedFailure('xelatex-missing-glyph',documents.missingGlyph,options('xelatex',{mode:'exact',pt:18}),'latex_compilation_failed');
    const browserOnly=await api('/api/export/preview',{format:'tex',content:documents.short,baseDir:h.docs,options:options('xelatex',{mode:'exact',pt:18})},'publication_line_spacing_unsupported');assert.equal(browserOnly.pdf,undefined);assert.equal(browserOnly.key,undefined);});
    await h.step('absent/null modes retain same-candidate legacy actual text, objects, pixels and baselines',async()=>{h.report.legacy={};for(const engine of engines){const settings=options(engine);settings.typography.lineHeight=1.9;
      const a=await exported(engine+'-legacy-absent',documents.short,settings,undefined,'Legacy reference'),b=await exported(engine+'-legacy-null',documents.short,{...settings,typography:{...settings.typography,lineSpacing:null}},undefined,'Legacy reference');h.report.legacy[engine]=checks.checkLegacyPair(a.inspection,b.inspection);}});
    await h.step('both engine presets persist across actual host restart; XeLaTeX retains original CJK and actual emoji glyph',async()=>{const existing=await rawApi('/api/export/presets',undefined,'GET');assert.equal(existing.status,200);const custom={...(existing.data.custom||{})},keys=engines.map(engine=>'P04 M4 '+engine+' exact 18');
      for(const [i,engine]of engines.entries())custom[keys[i]]=options(engine,{mode:'exact',pt:18});await api('/api/export/presets',{custom});const file=path.join(h.nativeData,'export_presets.json'),before=fileHash(file);h.report.preset={path:relative(file),sha256:before,names:keys};
      const pre=inventory(h,profile);known.push(...pre.processes);await h.stop();h.report.restartStopProcesses=inventory(h,profile,known);assert.deepEqual(h.report.restartStopProcesses.processes,[]);await boot();
      const after=await rawApi('/api/export/presets',undefined,'GET');assert.equal(after.status,200);for(const key of keys)assert.deepEqual(after.data.custom[key].typography.lineSpacing,custom[key].typography.lineSpacing);assert.equal(fileHash(file),before);
      await exported('xelatex-restarted-exact-18',documents.short,after.data.custom[keys[0]],{gap:18});h.report.preset.processRestarted=true;
      const unicode=await exported('xelatex-original-unicode',documents.unicode,options('xelatex',{mode:'atLeast',pt:30}));const record=h.report.outputs.at(-1);record.validated=false;record.unicodeProof=checks.checkUnicode(unicode.inspection);record.validated=true;});
    await h.step('all 24 real artifacts, source bytes and candidate/QA identities remain bound',()=>{assert.equal(h.report.checks.length,10);assert.equal(h.report.outputs.length,planned.plannedSuccessfulPdfFiles);assert(h.report.outputs.every(o=>o.validated&&fileHash(path.join(root,o.path))===o.sha256));h.unchanged(inputs);freeze.verify();h.report.actualSuccessfulPdfFiles=h.report.outputs.length;h.report.actualRequests=requestId;});
  }catch(error){primary=error;throw error;}finally{try{
    const stopped=await stopAfterInventory(()=>{h.report.preStopProcesses=inventory(h,profile,known);known.push(...h.report.preStopProcesses.processes);},()=>h.stop());if(stopped.inventoryError||stopped.stopError)throw stopped.inventoryError||stopped.stopError;
    h.report.postStopProcesses=inventory(h,profile,known);assert.deepEqual(h.report.postStopProcesses.processes,[],'Owned host/engine processes remain');reuse?.assertPreserved();freeze.verify();h.unchanged(inputs);
    for(const [file,hash]of Object.entries(h.report.qaDependencySha256))assert.equal(fileHash(path.join(root,file)),hash,'Frozen QA dependency changed');assert.equal(fileHash(python),h.report.independentRuntime.sha256);assert.equal(fileHash(candidatePath),h.report.candidateFile.sha256);
    h.report.hostStopped=true;h.report.sourceAndDependenciesUnchanged=true;h.report.finalInputHashes=h.snapshotInputs();
  }catch(error){h.report.postVerificationError=String(error.stack||error);if(!primary)throw error;}}});
}
function selfTest(){const result=require('../tools/test/publication-tex-typography-checks.test.cjs').run();assert.equal(Object.keys(documents).length,15);assert.equal(planned.plannedSuccessfulPdfFiles,24);assert.equal(rowMarkers.length,100);assert(documents.tallNative.includes('\\dfrac{X}{\\dfrac{Y}{Z}}'));return result;}
function help(){console.log('M4 prepare-only until root authorizes the current frozen candidate. Windows, one run, no automatic retry.\nRequired: READMD_M4_CANDIDATE_JSON absolute with sourceFreeze/hash; READMD_BIN/CANDIDATE_SHA256/CANDIDATE_BOOT_SHA256; READMD_ASSETS_DIR frozen assets; READMD_ENGINE_REUSE_PROFILE fixed absolute n2e7Ug; locked fonts/PDFium/Typst pack env for immutable reuse proof; READMD_QA_PYTHON/SHA256.\n24 real PDFs / 11 stages, actual XeLaTeX + pdfLaTeX through managed API; warm runtime reuse, no downloads or runtime copy. TeX source maps and full math outline proof remain unavailable/separate.\n--list / --self-test / --help start neither ReadMD nor an engine.');}
module.exports={run,planned,documents,options,selfTest};
if(require.main===module){if(process.argv.includes('--help'))help();else if(process.argv.includes('--list'))console.log(JSON.stringify(planned,null,2));else if(process.argv.includes('--self-test'))selfTest();else run().catch(error=>{console.error(error.stack||error.message);process.exitCode=1;});}
