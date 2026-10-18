'use strict';
// Prepare/run one bounded same-candidate M3 scope. --self-test/--list/--help
// never create a profile, execute ReadMD or launch an offline engine.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {createHarness,fileHash,sha256,root,stopAfterInventory}=require('../tools/test/publication-batch3-harness.cjs');
const {inspectArtifact}=require('../tools/test/publication-batch3-checks.cjs');
const checks=require('../tools/test/publication-typst-typography-checks.cjs');
const base={cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},pdf:{engine:'typst'},
  page:{size:'A4',marginTop:20,marginBottom:20,marginLeft:18,marginRight:18},
  typography:{font:'Arial',size:11,spacing:0,align:'left',firstLineIndent:0,keepLines:false},
  footer:{pageNumbers:false,text:''},header:{text:''}};
const options=mode=>({...base,typography:{...base.typography,...(mode===undefined?{}:{lineSpacing:mode})}});
const paragraph=words=>'\uFEFF'+words.join('  \r\n')+'\r\n';
const short=paragraph(['FIRST','SECOND','THIRD']),duplicate='SAME_TARGET source-bound duplicate.';
const math=size=>'office.r(word.rPr(word.sz(val: "'+size*2+'")), office.t("𝑥"))';
const withMath=size=>paragraph(['FIRST','SECOND :math['+math(size)+']','THIRD']);
const local=(id,mode)=>'<!-- readmd:layout v1 '+JSON.stringify({id,text:{size:24},paragraph:{lineSpacing:mode}})+' -->\r\n\r\n'+short.slice(1);
const sectionBad='FIRST_SECTION ordinary legacy body.\r\n\r\n<!-- readmd-section:v1 '+JSON.stringify({id:'small',break:'nextPage',page:{size:'A5',orientation:'landscape',marginTop:12,marginBottom:16}})+' -->\r\n\r\n'+local('small-exact',{mode:'exact',pt:18});
const rowMarkers=Array.from({length:100},(_,i)=>'P04ROW'+String(i).padStart(4,'0'));
const documents={short,tall48:withMath(48),medium24:withMath(24),long:paragraph(rowMarkers),duplicate:'\uFEFF'+duplicate+'\r\n\r\n'+duplicate+'\r\n',
  localExact:local('local-exact',{mode:'exact',pt:30}),localLeast:local('local-least',{mode:'atLeast',pt:30}),localMultiple:local('local-multiple',{mode:'multiple',multiple:1.5}),
  localImpossible:local('local-impossible',{mode:'exact',pt:18}),sectionBad,
  orphanQuote:'> <!-- readmd:layout v1 {"id":"quote-exact","paragraph":{"lineSpacing":{"mode":"exact","pt":18}}} -->\r\n>\r\n> Quote typed mode is unsupported.\r\n',
  unsupported:'<!-- readmd:layout v1 {"id":"p04-heading","paragraph":{"lineSpacing":{"mode":"exact","pt":18}}} -->\r\n\r\n# Synthetic heading\r\n',
  missingGlyph:paragraph(['FIRST','SECOND '+String.fromCodePoint(0x10ffff),'THIRD'])};
const planned={scope:'P04 M3 ordinary Typst body line spacing only',plannedStages:11,plannedSuccessfulPdfFiles:18,plannedNativeRestarts:1,productInvocations:0,
  capabilities:['typst','fonts','pdfium'],noTeXCompile:true,coldPerformanceAccepted:false,automaticRetries:0};
const relative=file=>path.relative(root,file).replaceAll(path.sep,'/'),write=(file,value)=>fs.writeFileSync(file,JSON.stringify(value,null,2)+'\n',{flag:'wx'});
async function port(){const s=net.createServer();await new Promise((r,j)=>{s.once('error',j);s.listen(0,'127.0.0.1',r);});const n=s.address().port;await new Promise(r=>s.close(r));return n;}
function inventory(h,profile){const q=s=>"'"+s.replaceAll("'","''")+"'",ps=path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe');
  return JSON.parse(h.command(ps,['-NoProfile','-NonInteractive','-Command',`$ErrorActionPreference='Stop'\n$Scope=${q(profile)}\n$All=@(Get-CimInstance Win32_Process)\n$Ids=@($All|Where-Object {$_.CommandLine -and $_.CommandLine.IndexOf($Scope,[StringComparison]::OrdinalIgnoreCase)-ge 0 -and $_.Name -in @('readmd.exe','msedgewebview2.exe','typst.exe')}|ForEach-Object {$_.ProcessId})\nfor($i=0;$i -lt 16;$i++){$Next=@($All|Where-Object {$Ids -contains $_.ParentProcessId -and $Ids -notcontains $_.ProcessId}|ForEach-Object {$_.ProcessId});if($Next.Count -eq 0){break};$Ids+=$Next}\n[pscustomobject]@{recordedAt=[DateTime]::UtcNow.ToString('o');processes=@($All|Where-Object {$Ids -contains $_.ProcessId}|ForEach-Object {[pscustomobject]@{pid=$_.ProcessId;parentPid=$_.ParentProcessId;name=$_.Name;created=$_.CreationDate}})}|ConvertTo-Json -Depth 4 -Compress`]));}
async function run(){
  assert.equal(process.platform,'win32');const candidatePath=process.env.READMD_M3_CANDIDATE_JSON;
  assert(candidatePath&&path.isAbsolute(candidatePath),'READMD_M3_CANDIDATE_JSON must identify root’s frozen absolute candidate JSON');
  const candidate=JSON.parse(fs.readFileSync(candidatePath,'utf8'));assert.equal(candidate.binarySha256,process.env.READMD_CANDIDATE_SHA256);assert.equal(candidate.bootSha256,process.env.READMD_CANDIDATE_BOOT_SHA256);
  const {freezeSources}=require('./publication-structure-native.cjs'),freeze=freezeSources(candidate);
  const helpers=require('./publication-engines-native.cjs'),profile=helpers.resolveEngineReuseProfile(process.env.READMD_ENGINE_REUSE_PROFILE);assert(profile,'Explicit fixed n2e7Ug owned profile is required');
  const h=createHarness('publication-typst-typography-native',await port(),__filename,profile),currentAssets=path.resolve(process.env.READMD_ASSETS_DIR||path.join(root,'assets'));
  const python=process.env.READMD_QA_PYTHON;assert(python&&path.isAbsolute(python));assert.match(process.env.READMD_QA_PYTHON_SHA256||'',/^[a-f0-9]{64}$/u);assert.equal(fileHash(python),process.env.READMD_QA_PYTHON_SHA256);
  const originalCommand=h.command;h.command=(exe,args,...rest)=>originalCommand(exe==='python'?python:exe,args,...rest);
  for(const file of ['ui-tests/publication-typst-typography-native.cjs','tools/test/publication-typst-typography-checks.cjs','tools/test/publication-typst-typography-checks.test.cjs','ui-tests/publication-engines-native.cjs','ui-tests/publication-structure-native.cjs','rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json'])h.report.qaDependencySha256[file]=fileHash(path.join(root,file));
  for(const file of Object.keys(h.report.qaDependencySha256))if(!file.endsWith('.json'))fs.copyFileSync(path.join(root,file),path.join(h.data,'qa-'+path.basename(file)));
  for(const [name,source]of Object.entries(documents))fs.writeFileSync(path.join(h.docs,name+'.md'),source,{flag:'wx'});
  const inputs=h.snapshotInputs(),responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  const oracle=JSON.parse(fs.readFileSync(path.join(root,'rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json'),'utf8'));assert.equal(oracle.fontToolsVersion,'4.63.0');assert.equal(fileHash(path.join(root,oracle.fontPath)),oracle.fontSha256);
  const originalX=oracle.glyphs[oracle.characters['𝑥']];assert(originalX&&originalX.inkBounds.length===4);
  h.report.scope=planned.scope;h.report.plan=planned;h.report.warmReuse=true;h.report.coldPerformanceAccepted=false;h.report.outputs=[];h.report.requests=[];h.report.capabilityStatus=[];
  h.report.candidateFile={path:relative(candidatePath),sha256:fileHash(candidatePath)};h.report.sourceFreeze={path:freeze.file,sha256:freeze.sha256,sources:freeze.sources};h.report.originalInputHashes=inputs;
  h.report.independentRuntime={executable:'<qa-python>',sha256:fileHash(python),productDependency:false};
  h.report.formulaOracle={fontSha256:oracle.fontSha256,fixtureSha256:h.report.qaDependencySha256['rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json'],unitsPerEm:oracle.unitsPerEm,character:'𝑥',glyph:originalX};
  h.report.limitations.push('Ordinary body typed spacing only. CSL own spacing and other stories retain their separate contracts. No TeX, Word, EPUB-reader or complete P04 certification.',
    'Exact uses a conservative paragraph envelope. line=1 is that envelope and blockIndex is the typed paragraph ordinal, not an exact UTF16/physical-line location. availableHeightPt is the current section full body height, not current-page remaining space.',
    'The fixed native data/profile is warm reused. This run does not measure cold performance; no runtime is copied or downloaded.');
  let page,reuse,requestId=0,primary;
  async function rawApi(route,body,method='POST'){const result=method==='POST'?await h.api(route,body):await page.evaluate(async route=>{const response=await apiFetch(route,{signal:AbortSignal.timeout(120000)});return {status:response.status,data:await response.json()};},route),file=path.join(responses,String(++requestId).padStart(3,'0')+'.json');write(file,{route,method,request:body,response:result});h.report.requests.push({route,method,file:relative(file),sha256:fileHash(file),status:result.status});return result;}
  async function api(route,body,code,status=200){const r=await rawApi(route,body);assert.equal(r.status,status,JSON.stringify(r.data));assert.equal(r.data.ok,!code,JSON.stringify(r.data));if(code)assert.equal(r.data.error_code,code,JSON.stringify(r.data));return r.data;}
  async function boot(){h.report.nativeStartAttempts=(h.report.nativeStartAttempts||0)+1;page=await h.start();h.report.nativeStarts=(h.report.nativeStarts||0)+1;await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));assert.equal(h.report.servedBootSha256,candidate.bootSha256);
    const csp=fs.readFileSync(path.join(process.env.READMD_ASSETS_DIR,'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/u)?.[1];assert(csp);
    assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),csp);h.report.productionCspUnchanged=true;}
  async function inspect(file,expected){const inspected=inspectArtifact(h,file,{glyphs:true,render:path.join(h.data,path.basename(file)+'-pages')});
    const record={path:relative(file),bytes:fs.statSync(file).size,sha256:fileHash(file),inspection:relative(path.join(h.data,path.basename(file)+'.inspection.json')),validated:false};h.report.outputs.push(record);
    record.proof=expected?checks.checkPdf(inspected,expected):undefined;record.inspectionSha256=fileHash(path.join(root,record.inspection));record.validated=true;return inspected;}
  async function exported(name,source,settings,expected,sourceName=name){const file=path.join(h.outputs,name+'.pdf');await api('/api/export',{format:'pdf',content:source,baseDir:h.docs,out_path:file,suggestedName:sourceName,options:settings});return {file,inspection:await inspect(file,expected)};}
  async function preview(name,source,settings,expected,mapOptions){const result=await api('/api/export/preview',{format:'pdf',content:source,baseDir:h.docs,suggestedName:name,options:settings});
    const file=path.join(h.outputs,name+'-preview.pdf');fs.writeFileSync(file,Buffer.from(result.pdf,'base64'),{flag:'wx'});const inspected=await inspect(file,expected),record=h.report.outputs.at(-1);record.validated=false;const proof=checks.checkMap(source,result,inspected,mapOptions);record.sourceMapProof=proof;record.validated=true;
    write(path.join(h.data,name+'.source-map.json'),{proof,sourceMap:result.sourceMap});return {result,file,inspection:inspected};}
  async function protectedFailure(label,source,settings,code,geometry){const file=path.join(h.outputs,'protected-'+label+'.pdf'),before=Buffer.from('Original owned output: '+label);fs.writeFileSync(file,before,{flag:'wx'});
    const error=await api('/api/export',{format:'pdf',content:source,baseDir:h.docs,out_path:file,suggestedName:label,options:settings},code);assert.deepEqual(fs.readFileSync(file),before,'Failed export replaced old target');
    const previewError=await api('/api/export/preview',{format:'pdf',content:source,baseDir:h.docs,suggestedName:label,options:settings},code);assert.equal(previewError.key,undefined);assert.equal(previewError.pdf,undefined);
    if(geometry){checks.checkGeometry(error,geometry);checks.checkGeometry(previewError,geometry);}
    h.report.protectedTargets??=[];h.report.protectedTargets.push({label,path:relative(file),sha256:fileHash(file),unchanged:true,exportError:error,previewError});h.unchanged(inputs);
  }
  await h.finish(async()=>{try{
    await h.step('fixed owned profile and immutable old proofs, fresh current-assets overlay',()=>{reuse=helpers.prepareEngineProfileReuse(h,profile,currentAssets);h.report.preStartProcesses=inventory(h,profile);assert.deepEqual(h.report.preStartProcesses.processes,[]);});
    await h.step('real native candidate, served boot, unchanged production CSP and actual locked Typst/fonts/PDFium readiness',async()=>{await boot();for(const id of planned.capabilities){const started=Date.now(),status=await rawApi('/api/capabilities/packs',{action:'status',id});assert.equal(status.status,200);assert.equal(status.data.installed,true,JSON.stringify(status.data));assert.equal(status.data.ready,true,JSON.stringify(status.data));h.report.capabilityStatus.push({id,elapsedMs:Date.now()-started,response:status.data});}
      h.report.engineProfileReuse.currentRunReadinessCapabilities=[...planned.capabilities];h.report.engineProfileReuse.actualProductReadinessReexecuted=true;});
    const modes=[['exact-18',{mode:'exact',pt:18},18],['multiple-2',{mode:'multiple',multiple:2},22],['least-30',{mode:'atLeast',pt:30},30]];
    await h.step('three supported body modes really export physical baseline intervals',async()=>{for(const [name,mode,gap]of modes)await exported(name,short,options(mode),{gap});h.unchanged(inputs);});
    await h.step('actual previews preserve same-source targets and cached export bytes, including duplicate identities',async()=>{for(const [name,mode,gap]of modes){const p=await preview(name,short,options(mode),{gap});if(name==='exact-18'){
        const file=path.join(h.outputs,'exact-18-cached.pdf');await api('/api/export',{format:'pdf',content:short,baseDir:h.docs,out_path:file,suggestedName:name,options:options(mode)});assert.equal(fileHash(file),fileHash(p.file));await inspect(file,{gap});}}
      await preview('duplicate',documents.duplicate,options({mode:'exact',pt:18}),undefined,{duplicate});h.unchanged(inputs);});
    await h.step('24pt local text keeps fixed pt and resolves explicit local multiple',async()=>{for(const [key,name,gap]of [['localExact','local-exact-30',30],['localLeast','local-least-30',30],['localMultiple','local-multiple-36',36]])await exported(name,documents[key],options({mode:'exact',pt:20}),{gap,fontSize:24});});
    await h.step('real 24/48pt Office glyph vectors retain independent physical bounds while native minimum lines expand',async()=>{const shape=size=>({...originalX,unitsPerEm:oracle.unitsPerEm,size});
      await exported('exact-18-medium',documents.medium24,options({mode:'exact',pt:18}),{gap:18,formula:shape(24)});
      await exported('least-18-tall',documents.tall48,options({mode:'atLeast',pt:18}),{gap:18,minimum:true,formula:shape(48)});
      await exported('multiple-1p5-tall',documents.tall48,options({mode:'multiple',multiple:1.5}),{gap:16.5,minimum:true,formula:shape(48)});});
    await h.step('one 100-line paragraph actually paginates without loss, duplication or exact-baseline drift',async()=>{await exported('long-exact-18',documents.long,options({mode:'exact',pt:18}),{gap:18,markers:rowMarkers,long:true});});
    await h.step('trusted geometry details, changed-section dimensions and unsupported/invalid/missing-glyph errors preserve old outputs',async()=>{
      await protectedFailure('tall-exact',documents.tall48,options({mode:'exact',pt:18}),'publication_layout_geometry',{requested:18});
      await protectedFailure('local-exact',documents.localImpossible,options(),'publication_layout_geometry',{requested:18,id:'local-impossible'});
      await protectedFailure('section-exact',documents.sectionBad,options(),'publication_layout_geometry',{requested:18,id:'small-exact',width:174*72/25.4,height:120*72/25.4});
      await protectedFailure('orphan-quote',documents.orphanQuote,options(),'publication_local_style_orphan');
      await protectedFailure('unsupported-story',documents.unsupported,options(),'publication_line_spacing_unsupported');
      await protectedFailure('invalid-union',short,{...base,typography:{lineHeight:1.6,lineSpacing:{mode:'exact',pt:18}}},'publication_local_style_invalid');
      await protectedFailure('missing-glyph',documents.missingGlyph,options({mode:'exact',pt:18}),'typst_invalid_content');});
    await h.step('legacy absent/null typed mode retains actual objects, text, pixels and baselines',async()=>{const a=await exported('legacy-absent',short,{...base,typography:{...base.typography,lineHeight:1.9}},undefined,'Legacy reference'),b=await exported('legacy-null',short,{...base,typography:{...base.typography,lineHeight:1.9,lineSpacing:null}},undefined,'Legacy reference');h.report.legacy=checks.checkLegacyPair(a.inspection,b.inspection);});
    await h.step('typed preset persists across actual host stop/restart and remains operational',async()=>{
      const existing=await rawApi('/api/export/presets',undefined,'GET');assert.equal(existing.status,200);
      const key='P04 M3 Typst exact 18',settings=options({mode:'exact',pt:18});await api('/api/export/presets',{custom:{...(existing.data.custom||{}),[key]:settings}});const file=path.join(h.nativeData,'export_presets.json'),before=fileHash(file);
      h.report.preset={path:relative(file),sha256:before,name:key};await h.stop();const stopped=inventory(h,profile);assert.deepEqual(stopped.processes,[]);h.report.restartStopProcesses=stopped;await boot();
      const after=await rawApi('/api/export/presets',undefined,'GET');assert.equal(after.status,200);assert.deepEqual(after.data.custom[key].typography.lineSpacing,settings.typography.lineSpacing);assert.equal(fileHash(file),before);
      await exported('restarted-exact-18',short,after.data.custom[key],{gap:18});h.report.preset.processRestarted=true;});
    await h.step('all actual output indexes, original bytes and candidate/QA source identities remain bound',()=>{assert.equal(h.report.outputs.length,planned.plannedSuccessfulPdfFiles);assert(h.report.outputs.every(o=>o.validated&&fileHash(path.join(root,o.path))===o.sha256));h.unchanged(inputs);freeze.verify();h.report.actualSuccessfulPdfFiles=h.report.outputs.length;h.report.actualRequests=requestId;});
  }catch(error){primary=error;throw error;}finally{try{
    const stopped=await stopAfterInventory(()=>{h.report.preStopProcesses=inventory(h,profile);},()=>h.stop());if(stopped.inventoryError||stopped.stopError)throw stopped.inventoryError||stopped.stopError;
    h.report.postStopProcesses=inventory(h,profile);assert.deepEqual(h.report.postStopProcesses.processes,[],'Owned host/engine processes remain');reuse?.assertPreserved();freeze.verify();h.unchanged(inputs);
    for(const [file,hash]of Object.entries(h.report.qaDependencySha256))assert.equal(fileHash(path.join(root,file)),hash,'Frozen QA dependency changed');assert.equal(fileHash(python),h.report.independentRuntime.sha256);
    h.report.hostStopped=true;h.report.sourceAndDependenciesUnchanged=true;h.report.finalInputHashes=h.snapshotInputs();
  }catch(error){h.report.postVerificationError=String(error.stack||error);if(!primary)throw error;}}});
}
function help(){console.log('M3 prepare-only until root supplies and authorizes the frozen candidate. Run once on Windows; no automatic retry.\nRequired: READMD_M3_CANDIDATE_JSON absolute (sourceFreeze/hash), READMD_BIN/CANDIDATE_SHA256/CANDIDATE_BOOT_SHA256, READMD_ASSETS_DIR current frozen assets, READMD_ENGINE_REUSE_PROFILE fixed absolute n2e7Ug, locked Typst/fonts/PDFium pack env, READMD_QA_PYTHON/SHA256. Uses fresh QA evidence and warm owned native data; no TeX compile/download/runtime copy.\n--list prints planned scope; --self-test executes checker controls only; neither starts a host.');}
module.exports={run,planned,documents};
if(require.main===module){if(process.argv.includes('--help'))help();else if(process.argv.includes('--list'))console.log(JSON.stringify(planned,null,2));else if(process.argv.includes('--self-test'))require('../tools/test/publication-typst-typography-checks.test.cjs').run();else run().catch(error=>{console.error(error.stack||error.message);process.exitCode=1;});}
