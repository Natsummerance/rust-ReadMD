'use strict';
// Prepared bounded M11 QA. Real native execution requires root's unique candidate.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),fileHash=p=>sha(fs.readFileSync(p));
const NS='http://www.w3.org/1998/Math/MathML';
const near=(a,b,label,t=.00004)=>assert(Number.isFinite(a)&&Math.abs(a-b)<=t,`${label}: ${a} != ${b}`);
// CSS px -> pt remains .75. DOM layout has 1/64-CSS-px quantization;
// reserve .02pt only for screen bounding boxes, never API/SVG or PDF values.
const DOM_TOLERANCE_PT=.02;
const domNear=(a,b,label)=>near(a,b,label,DOM_TOLERANCE_PT);
const flatten=n=>[n,...n.content.filter(p=>p.type==='element').flatMap(p=>flatten(p.value))];
const local=n=>n.name.split(':').at(-1);
function semantics(n){return{namespace:n.namespace,local:local(n),attributes:Object.entries(n.attributes).filter(([k])=>!k.startsWith('xmlns')&&k!=='readmdId').map(([k,v])=>[n.attribute_namespaces?.[k]||'',k.split(':').at(-1),v]).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))),content:n.content.map(p=>p.type==='element'?{element:semantics(p.value)}:{[p.type]:p.value})};}
const fingerprint=n=>sha(JSON.stringify(semantics(n)));
function fontXHeight(bytes){
  const tables=bytes.readUInt16BE(4),table=tag=>{for(let i=0;i<tables;i++){const at=12+i*16;if(bytes.toString('ascii',at,at+4)===tag)return bytes.readUInt32BE(at+8);}throw Error('Independent font table missing: '+tag);};
  const head=table('head'),os2=table('OS/2'),unitsPerEm=bytes.readUInt16BE(head+18);assert(bytes.readUInt16BE(os2)>=2);const sxHeight=bytes.readInt16BE(os2+86);assert(sxHeight>0&&unitsPerEm>0);return{unitsPerEm,sxHeight,ratio:sxHeight/unitsPerEm};
}
function vectorLogical(metrics,size){
  assert.equal(metrics.paths,0,'An authored mspace cannot paint glyphs/rules');assert.equal(metrics.texts,0);assert.equal(metrics.images,0);
  const depth=-(metrics.verticalEm+.01)*size;
  return{width:(metrics.viewBox[2]/1000-.02)*size,height:(metrics.viewBox[3]/1000-.02)*size-depth,depth};
}
function assertPdfGap(base,gap,points){
  assert.equal(base.pages.length,1);assert.equal(gap.pages.length,1);
  const a=base.pages[0].drawings,b=gap.pages[0].drawings;assert.equal(a.length,2);assert.equal(b.length,2,'Spacing cannot add painted ink');
  for(let i=0;i<2;i++){
    near(b[i].bbox[2]-b[i].bbox[0],a[i].bbox[2]-a[i].bbox[0],'Unchanged glyph ink width',.004);
    near(b[i].bbox[3]-b[i].bbox[1],a[i].bbox[3]-a[i].bbox[1],'Unchanged glyph ink height',.004);
  }
  near(b[0].bbox[0],a[0].bbox[0],'First ink remains fixed',.004);
  near(b[1].bbox[0]-a[1].bbox[0],points,'Actual second PDF outline translation',.004);
}
const groups=[
  'unique candidate, served boot and formal packaged CSP',
  'zero-ink dimensions in physical/font-relative units, explicit zero and omitted attributes',
  'MathML to DSL to unified tree and MathML preserves source units and semantic identity',
  'exact UTF16 dimension slots, source-owned patch, revision conflict and unchanged siblings',
  'invalid versus valid-but-unsupported MathML, resolved size budget and missing vector',
  'real basic PDF outlined glyph positions and zero additional spacing ink',
  'real inherited PDF size budget and physical frame rejection preserve existing targets',
  'native Office and TeX exports reject mixed spacing before replacing owned targets',
  'real repeated-formula slot draft, invalid recovery, Esc and exactly one body undo/redo',
  'real save, file reopen, process restart, unchanged dependencies and owned host stop',
];
if(process.argv.includes('--plan')){console.log(JSON.stringify({scope:'prepared-bounded-M11-native',nativeExecuted:false,productInvocations:0,groups,requires:['READMD_BIN','READMD_CANDIDATE_SHA256','READMD_CANDIDATE_BOOT_SHA256','READMD_QA_PYTHON','READMD_QA_PYTHON_SHA256'],installAt:'ui-tests/math-space-native.cjs',limits:['No full MathML Core or independent Word/TeX renderer certification','Spacing dimensions edited as DSL in an existing structural slot; no claim of a dedicated mspace property UI']},null,2));process.exit(0);}
if(process.argv.includes('--self-test')){
  assert.deepEqual(vectorLogical({paths:0,texts:0,images:0,viewBox:[0,-1510,1020,1770],verticalEm:-.26},12),{width:12,height:18,depth:3});
  assert.throws(()=>vectorLogical({paths:1,texts:0,images:0,viewBox:[0,0,20,20],verticalEm:-.01},12));
  const base={pages:[{drawings:[{bbox:[10,10,15,16]},{bbox:[17,10,22,16]}]}]},gap={pages:[{drawings:[{bbox:[10,10,15,16]},{bbox:[29,10,34,16]}]}]};
  assertPdfGap(base,gap,12);
  for(const mutate of [v=>v.pages[0].drawings.push({bbox:[0,0,1,1]}),v=>v.pages[0].drawings[1].bbox[0]++,v=>v.pages[0].drawings[0].bbox[2]++]){const wrong=structuredClone(gap);mutate(wrong);assert.throws(()=>assertPdfGap(base,wrong,12));}
  const node={name:'m:mspace',namespace:NS,attributes:{width:'0px'},content:[]};
  assert.notEqual(fingerprint(node),fingerprint({...node,attributes:{}}),'Explicit zero and omission differ');
  assert.notEqual(fingerprint(node),fingerprint({...node,attributes:{width:'0pt'}}),'Original units are identity');
  assert.equal(fingerprint(node),fingerprint({...node,id:'new',attributes:{...node.attributes,'xmlns:m':NS,readmdId:'internal'}}));
  assert.notEqual(fingerprint({...node,attribute_namespaces:{width:'urn:first'}}),fingerprint({...node,attribute_namespaces:{width:'urn:second'}}));
  assert.notEqual(fingerprint({...node,content:[{type:'text',value:'x'}]}),fingerprint({...node,content:[{type:'comment',value:'x'}]}));
  domNear(12+.75/64,12,'One DOM layout quantum');
  assert.throws(()=>domNear(12+.02001,12,'Real screen width error'));
  assert.throws(()=>near(12+.001,12,'SVG/API keeps strict precision'));
  console.log(JSON.stringify({scope:'pure-QA-controls',productInvocations:0,passed:14}));process.exit(0);
}
const {createHarness,captureFailureScreenshot}=require(path.join(root,'tools/test/publication-batch3-harness.cjs'));
async function freePort(){const s=net.createServer();await new Promise((resolve,reject)=>{s.once('error',reject);s.listen(0,'127.0.0.1',resolve);});const port=s.address().port;await new Promise(resolve=>s.close(resolve));return port;}
function ownedInventory(directory){
  const escaped=directory.replaceAll("'","''");
  const script="$owned='"+escaped+"'; $rows=@(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.CommandLine -and $_.CommandLine.IndexOf($owned,[StringComparison]::OrdinalIgnoreCase) -ge 0 -and $_.Name -match '^(readmd|msedgewebview2|xelatex|xdvipdfmx)\\.exe$' } | ForEach-Object { [pscustomobject]@{pid=$_.ProcessId;parentPid=$_.ParentProcessId;name=$_.Name} }); ConvertTo-Json -InputObject $rows -Compress";
  const r=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{encoding:'utf8',windowsHide:true,timeout:15000});
  assert.equal(r.status,0,'Owned CIM inventory failed');const activeOwned=JSON.parse(r.stdout.trim()||'[]');assert(Array.isArray(activeOwned));return{observedAt:new Date().toISOString(),activeOwned};
}
async function main(){
  assert.equal(process.platform,'win32');
  const python=process.env.READMD_QA_PYTHON;
  assert(python&&path.isAbsolute(python),'Explicit independent QA Python runtime required');
  assert.match(process.env.READMD_QA_PYTHON_SHA256||'',/^[a-f0-9]{64}$/);
  assert.equal(fileHash(python),process.env.READMD_QA_PYTHON_SHA256,'Independent QA runtime changed');
  const h=createHarness('math-space-native',await freePort(),__filename),responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  const inspector=path.join(root,'tools/test/inspect-publication-batch3.py');
  h.report.independentRuntime={sha256:fileHash(python),productDependency:false};
  h.report.qaDependencySha256['tools/test/inspect-publication-batch3.py']=fileHash(inspector);
  const assets=process.env.READMD_ASSETS_DIR||path.join(root,'assets'),bootSha=process.env.READMD_CANDIDATE_BOOT_SHA256||process.env.READMD_BOOT_SHA256;
  const mathFont=path.join(assets,'fonts/STIXTwoMath-Regular.ttf'),fontMetrics=fontXHeight(fs.readFileSync(mathFont));
  h.report.independentFont={sha256:fileHash(mathFont),unitsPerEm:fontMetrics.unitsPerEm,sxHeight:fontMetrics.sxHeight,source:'raw OpenType head/OS2 tables, not product metrics'};
  const sourceFiles=['rust/readmd-kernel/src/math_spacing.rs','rust/readmd-kernel/src/math_layout.rs','rust/readmd-kernel/src/math_document.rs','rust/readmd-kernel/src/math_formats.rs','rust/readmd-kernel/src/math_dsl.rs','rust/readmd-kernel/src/omml.rs','rust/readmd-kernel/src/upgrade_api.rs','rust/readmd-kernel/src/pdf_render.rs','rust/readmd-kernel/src/publication_pdf_paragraph.rs','rust/readmd-kernel/src/mdexport.rs','rust/readmd-kernel/src/docx_writer.rs','rust/readmd-kernel/src/latex_writer.rs','assets/js/editor/formula-inline.js','assets/js/editor/formula-studio.js','assets/js/editor/formula-properties.js','assets/js/reader/formula.js'];
  let page,stage='startup',count=0,original,changed,expectedRanges;const pageErrors=[],file=path.join(h.docs,'Synthetic-space-workshop.md');
  Object.assign(h.report,{scope:'bounded-M11-native',fullMathmlCertified:false,groups,apiRequests:[],outputs:[],geometry:[],screenshots:[],unverified:['Independent Word and TeX applications','Signed/percentage/linebreak/advanced MathML spacing','Dedicated mspace property controls','Narrator full accessibility'],sourceSha256:Object.fromEntries(sourceFiles.map(p=>[p,fileHash(path.join(root,p))]))});
  const step=(label,fn)=>{stage=label;return h.step(label,fn);};
  async function api(route,body,status=200,ok=true){
    const start=Date.now(),result=await h.api(route,body),name=String(++count).padStart(3,'0')+'.json',target=path.join(responses,name);
    fs.writeFileSync(target,JSON.stringify({route,body,response:result,elapsedMs:Date.now()-start},null,2)+'\n',{flag:'wx'});
    h.report.apiRequests.push({route,status:result.status,path:'responses/'+name,sha256:fileHash(target)});
    assert.equal(result.status,status,route+' '+JSON.stringify(result.data).slice(0,1200));assert.equal(result.data.ok,ok);return result.data;
  }
  const parse=async(source,dialect='mathml',extra={})=>{const value=await api('/api/math/parse',{source,dialect,display:false,font_size:12,...extra});assert.equal(value.document.source,source);assert.equal(value.document.revision,sha(source),'Revision binds exact UTF8 source bytes');return value;};
  const serialize=(source,dialect,target)=>api('/api/math/serialize',{source,dialect,target,display:false});
  async function screenshot(name,scene){const bytes=await page.screenshot();fs.writeFileSync(path.join(h.data,name),bytes,{flag:'wx'});h.report.screenshots.push({path:name,sha256:sha(bytes),bytes:bytes.length,scene,visualReview:'pending root',viewport:page.viewportSize()});}
  async function boot(){page=await h.start();page.on('pageerror',e=>pageErrors.push(e.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});
    const csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),expected=fs.readFileSync(path.join(assets,'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)[1];assert.equal(csp,expected);assert.equal(h.report.servedBootSha256,bootSha.toLowerCase());h.report.csp=csp;
  }
  async function svgMetrics(value){return page.evaluate(svg=>{const doc=new DOMParser().parseFromString(svg,'image/svg+xml');assertNoParserError();function assertNoParserError(){if(doc.querySelector('parsererror'))throw Error('Actual vector SVG is malformed');}const el=doc.documentElement;return{viewBox:el.getAttribute('viewBox').split(/\s+/).map(Number),verticalEm:parseFloat(el.style.verticalAlign),paths:el.querySelectorAll('[data-math-ink] path').length,texts:el.querySelectorAll('[data-math-ink] text').length,images:el.querySelectorAll('[data-math-ink] image').length};},value.vector_preview);}
  const options=size=>({pdf:{engine:'rust'},typography:{font:'Arial',size},cover:{enabled:false},toc:{enabled:false},footer:{pageNumbers:false},header:{text:''}});
  async function exportPdf(name,content,opt=options(12)){
    const output=path.join(h.outputs,name+'.pdf');await api('/api/export',{format:'pdf',content,baseDir:h.docs,out_path:output,options:opt});assert(fs.statSync(output).size>0);
    assert.equal(fileHash(python),h.report.independentRuntime.sha256);assert.equal(fileHash(inspector),h.report.qaDependencySha256['tools/test/inspect-publication-batch3.py']);
    const args=[inspector,output,'--render',path.join(h.data,name+'-pages')],result=spawnSync(python,args,{windowsHide:true,timeout:90000,maxBuffer:32*1024*1024});
    const stdout=path.join(h.data,name+'.inspector.stdout.log'),stderr=path.join(h.data,name+'.inspector.stderr.log');fs.writeFileSync(stdout,result.stdout||'',{flag:'wx'});fs.writeFileSync(stderr,result.stderr||'',{flag:'wx'});
    (h.report.independentCommands??=[]).push({runtimeSha256:fileHash(python),inspectorSha256:fileHash(inspector),status:result.status,error:result.error?.code||null,stdout:path.basename(stdout),stdoutSha256:fileHash(stdout),stderr:path.basename(stderr),stderrSha256:fileHash(stderr)});
    assert.equal(result.status,0,'Independent PDF inspection failed: '+String(result.error?.code||result.stderr).slice(0,1000));
    const evidence=JSON.parse(result.stdout.toString('utf8')),inspection=path.basename(output)+'.inspection.json';fs.writeFileSync(path.join(h.data,inspection),JSON.stringify(evidence,null,2)+'\n',{flag:'wx'});
    assert.equal(evidence.kind,'pdf');assert.equal(evidence.sha256,fileHash(output));assert.equal(Boolean(evidence.leakedCitationSlot),false);
    h.report.outputs.push({path:path.relative(h.data,output),sha256:fileHash(output),bytes:fs.statSync(output).size,inspection,inspectionSha256:fileHash(path.join(h.data,inspection))});return evidence;
  }
  async function open(){await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file,{force:true,discardConfirmed:true});},file);await h.until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===2));}
  async function assertRenderedSpaces(selector){const widths=await page.locator(selector+' .readmd-dsl-math[data-readmd-ready="true"]').evaluateAll(elements=>elements.map(element=>{const regions=element.querySelectorAll('[data-math-selection-regions] [data-math-node="mspace"] > rect');if(regions.length!==1)throw Error('Exactly one actual source-bound spacing region required');return regions[0].getBoundingClientRect().width*.75;}));assert.equal(widths.length,2);domNear(widths[0],12,'First rendered duplicate stays12pt');domNear(widths[1],24,'Second rendered duplicate becomes24pt');return widths;}
  async function editNumerator(value){const target=page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(1);const located=await target.evaluate(element=>{const range=ReadMDInlineMath.locate(element);return range?{from:range.from,to:range.to,tex:range.tex}:null;});assert.deepEqual(located,{from:expectedRanges[1].from,to:expectedRanges[1].to,tex:expectedRanges[1].tex});(h.report.duplicateLocators??=[]).push(located);await target.click();await h.until(()=>page.locator('.formula-inline-controls button').first().isEnabled());const slot=page.locator('.formula-inline-canvas [data-math-node="num"][data-math-span]').first();await slot.click();await page.locator('.formula-inline-input').fill(value);await page.locator('.formula-inline-input').press('Enter');}
  try{
    await step(groups[0],async()=>{assert.match(bootSha||'',/^[a-f0-9]{64}$/i);assert.equal(fileHash(path.join(assets,'readmd.boot.js')),bootSha.toLowerCase());await boot();});
    await step(groups[1],async()=>{
      for(const size of [12,24])for(const [width,expected] of [['16px',12],['1em',size],['2ex',2*size*fontMetrics.ratio],['1pc',12],['1in',72],['2.54cm',72],['25.4mm',72],['12pt',12],['0px',0],[null,0]]){
        const source=`<math xmlns="${NS}"><mspace${width===null?'':` width="${width}"`} height="18pt" depth="4px"/></math>`,value=await parse(source,'mathml',{font_size:size});
        assert.equal(value.document.source,source);assert.deepEqual(value.document.diagnostics,[]);const geometry=vectorLogical(await svgMetrics(value),size);near(geometry.width,expected,'Physical width');near(geometry.height,18,'Physical height');near(geometry.depth,3,'Physical depth');
        const node=flatten(value.document.root).find(n=>n.namespace===NS&&local(n)==='mspace');assert(node);if(width===null)assert(!Object.hasOwn(node.attributes,'width'));else assert.equal(node.attributes.width,width);h.report.geometry.push({source,size,expected:{width:expected,height:18,depth:3},actual:geometry});
      }
      for(const size of [12,24]){const value=await parse('<math><mspace width="1em" height="1em" depth="4px" mathsize="18pt"/></math>','mathml',{font_size:size}),actual=vectorLogical(await svgMetrics(value),size);near(actual.width,18,'Explicit fixed mathsize width');near(actual.height,18,'Explicit fixed mathsize height');near(actual.depth,3,'Physical depth is not rescaled');}
      for(const source of [`<math><mspace/></math>`,`<math><mspace width="0" height="0pt" depth="0px"/></math>`]){const value=await parse(source),g=vectorLogical(await svgMetrics(value),12);near(g.width,0,'Zero logical width');near(g.height,0,'Zero logical height');near(g.depth,0,'Zero logical depth');}
    });
    await step(groups[2],async()=>{
      const source='<math><mi>a</mi><mspace width=" 1.25EM " height="2pt" depth="0px"/><mi>b</mi><mspace/></math>',a=await parse(source),dsl=await serialize(source,'mathml','dsl'),b=await parse(dsl.output,'dsl');
      assert.equal(fingerprint(a.document.root),fingerprint(b.document.root));const mathml=await serialize(dsl.output,'dsl','mathml'),c=await parse(mathml.output);
      const attrs=value=>flatten(value.document.root).filter(n=>n.namespace===NS&&local(n)==='mspace').map(n=>Object.fromEntries(Object.entries(n.attributes).filter(([k])=>!k.startsWith('xmlns')&&k!=='readmdId')));
      assert.deepEqual(attrs(c),attrs(a));assert.equal(a.document.source,source);assert(!dsl.output.includes('readmdId'));h.report.roundtrip={before:fingerprint(a.document.root),after:fingerprint(b.document.root),dslSha256:sha(dsl.output)};
    });
    await step(groups[3],async()=>{
      const prefix='<math><!-- 😀 --><mi>𝑎</mi>',raw='<mspace width=\'1em\' depth="0px"/>',suffix='<!-- kept --><mi>b</mi></math>',source=prefix+raw+suffix,a=await parse(source);
      const node=a.document.nodes.find(n=>n.kind==='mspace');assert(node);assert.equal(node.from,prefix.length);assert.equal(source.slice(node.from,node.to),raw);assert.equal(source.slice(node.slots.find(s=>s.role==='width').from,node.slots.find(s=>s.role==='width').to),'1em');
      const request={source,dialect:'mathml',display:false,expected_revision:a.document.revision,patch:{operation:'set_attribute',node_id:node.id,name:'width',value:'16px'}};
      const b=await api('/api/math/patch',request);assert.equal(b.document.dialect,'mathml');assert(b.document.source.startsWith(prefix));assert(b.document.source.endsWith(suffix));assert.notEqual(b.document.revision,a.document.revision);assert.equal(b.document.revision,sha(b.document.source));
      const stale=await api('/api/math/patch',{...request,source:b.document.source},409,false);assert.equal(stale.error_code,'transaction_conflict');
      const current=b.document.nodes.find(n=>n.kind==='mspace');const removed=await api('/api/math/patch',{source:b.document.source,dialect:'mathml',expected_revision:b.document.revision,patch:{operation:'set_attribute',node_id:current.id,name:'depth',value:null}});
      const removedSpace=flatten(removed.document.root).find(n=>n.namespace===NS&&local(n)==='mspace');assert(!Object.hasOwn(removedSpace.attributes,'depth'));assert.equal(removedSpace.attributes.width,'16px');assert(removed.document.source.startsWith(prefix));assert(removed.document.source.endsWith(suffix));assert.equal(a.document.source,source);h.report.utf16PatchProof={sourceSha256:sha(source),sourceRevision:a.document.revision,nodeFrom:node.from,nodeTo:node.to,widthSlot:node.slots.find(s=>s.role==='width'),patchedRevision:b.document.revision,removedRevision:removed.document.revision,unchangedSiblings:true};
    });
    await step(groups[4],async()=>{
      for(const [space,code] of [["width='-1em'",'math_unsupported_extension'],["width='50%'",'math_unsupported_extension'],["width='nonsense'",'math_invalid_property'],["linebreak='newline'",'math_unsupported_extension'],["width='300in'",'math_budget']]){const result=await api('/api/math/parse',{source:`<math><mspace ${space}/></math>`,dialect:'mathml'},422,false);assert.equal(result.error_code,code);}
      const source='<math><mspace width="1000em"/></math>';await parse(source);const actual=await api('/api/math/parse',{source,dialect:'mathml',font_size:24},422,false);assert.equal(actual.error_code,'math_budget');
      const missing=await api('/api/math/parse',{source:'<math><mspace width="1em"/><mi>\u{10ffff}</mi></math>',dialect:'mathml'},422,false);assert.equal(missing.error_code,'math_unsupported_extension');
      const foreign=await api('/api/math/parse',{source:'<math xmlns="urn:impostor"><mspace width="1em"/></math>',dialect:'mathml'},422,false);assert.equal(foreign.error_code,'math_invalid_xml');
    });
    await step(groups[5],async()=>{
      const base=await exportPdf('Space-baseline',':math[row(a, b)]\n');
      const gap=await exportPdf('Space-physical',':math[row(a, space(width: "12pt"), b)]\n');assertPdfGap(base,gap,12);
      const relative=await exportPdf('Space-relative',':math[row(a, space(width: "1em"), b)]\n');assertPdfGap(base,relative,12);
      const largeBase=await exportPdf('Space-baseline20',':math[row(a, b)]\n',options(20)),largeGap=await exportPdf('Space-relative20',':math[row(a, space(width: "1em"), b)]\n',options(20));assertPdfGap(largeBase,largeGap,20);
      const only=await exportPdf('Space-no-ink',':math[space(width: "12pt", height: "18pt", depth: "4px")]\n');assert.equal(only.pages.length,1);assert.deepEqual(only.pages[0].drawings,[]);
      h.report.pdfGapProof={points12:true,points20:true,additionalSpacingInk:0};
    });
    await step(groups[6],async()=>{
      const target=path.join(h.outputs,'protected-size.pdf');fs.writeFileSync(target,'owned existing PDF',{flag:'wx'});const originalHash=fileHash(target);
      const cases=[{content:'# :math[space(width: "1100em")]\n',options:{...options(12),headings:{h1:{size:20}}},code:'math_budget'},
        {content:'| cell |\n|---|\n| :math[space(width: "1500em")] |\n',options:{...options(12),table:{cellSize:16}},code:'math_budget'},
        {content:'###### :math[space(width: "2000em")]\n',options:{...options(12),headings:{h6:{size:8}}},code:'publication_layout_geometry'},
        {content:':math[space(width: "1000em")]\n',options:options(12),code:'publication_layout_geometry'}];
      for(const {content,options,code} of cases){const r=await api('/api/export',{format:'pdf',baseDir:h.docs,out_path:target,content,options},200,false);assert.equal(r.error_code,code);assert.equal(fileHash(target),originalHash);}
      h.report.protectedPdfSha256=originalHash;
    });
    await step(groups[7],async()=>{
      const source='row(a, space(width: "1em", height: "2pt"), b)';for(const target of ['omml','latex']){const r=await api('/api/math/serialize',{source,dialect:'dsl',target},422,false);assert.equal(r.error_code,'math_unsupported_extension');}
      for(const format of ['docx','tex']){const target=path.join(h.outputs,'protected.'+format);fs.writeFileSync(target,'owned original '+format,{flag:'wx'});const original=fileHash(target),r=await api('/api/export',{format,content:':math['+source+']\n',baseDir:h.docs,out_path:target,options:options(12)},200,false);assert.equal(r.error_code,'math_unsupported_extension');assert.equal(fileHash(target),original);(h.report.protectedTargets??=[]).push({path:path.relative(h.data,target),sha256:original,format,scope:'Existing owned sentinel bytes; not a newly generated artifact'});}
      await api('/api/math/serialize',{source:'x^2',dialect:'latex',target:'omml'});
    });
    await step(groups[8],async()=>{
      const dsl='frac(row(a, space(width: "12pt", height: "2pt", depth: "0px"), b), c)';
      original=['# Synthetic space workshop 😀','','Only the second repeated formula changes.','',...Array.from({length:2},()=>['```readmd-math v1',dsl,'```','']).flat()].join('\n');fs.writeFileSync(file,original,{flag:'wx'});await open();await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
      await h.until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===2));
      const ranges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));expectedRanges=ranges;assert.equal(ranges.length,2);assert.equal(ranges[0].tex,ranges[1].tex);for(const range of ranges)assert.equal(original.slice(range.bodyFrom,range.bodyTo),range.tex);const firstFence=original.indexOf('```readmd-math v1'),secondFence=original.indexOf('```readmd-math v1',firstFence+1);assert.equal(ranges[0].from,firstFence);assert.equal(ranges[1].from,secondFence);
      await editNumerator('row(a, space(width: "-1em"), b)');await h.until(()=>page.locator('.formula-inline-message').textContent().then(t=>t.trim().length>0));const invalidMessage=await page.locator('.formula-inline-message').textContent();assert.equal(invalidMessage,await page.evaluate(()=>i18n.t('formula.invalid')));assert.notEqual(invalidMessage,'formula.invalid');assert(!invalidMessage.includes('math_unsupported_extension'));assert.equal(await page.evaluate(()=>getEditContent()),original);
      await page.locator('.formula-inline-input').fill('row(a, space(width: "24pt", height: "2pt", depth: "0px"), b)');await page.locator('.formula-inline-input').press('Enter');await h.until(()=>page.locator('.formula-inline-controls button').first().isEnabled());assert.equal(await page.locator('.formula-inline-message').textContent(),'');
      await screenshot('space-slot-editor-1160.png','Actual structural DSL slot with corrected spacing; body unchanged');await page.locator('.formula-inline-input').press('Escape');await h.until(()=>page.evaluate(()=>!ReadMDInlineMath.active()));assert.equal(await page.evaluate(()=>getEditContent()),original);
      await editNumerator('row(a, space(width: "24pt", height: "2pt", depth: "0px"), b)');await h.until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();await h.until(()=>page.evaluate(()=>!ReadMDInlineMath.active()));changed=await page.evaluate(()=>getEditContent());assert.notEqual(changed,original);
      const after=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.equal(after[0].tex,ranges[0].tex);assert.equal(changed.slice(0,after[1].from),original.slice(0,ranges[1].from));assert.equal(changed.slice(after[1].to),original.slice(ranges[1].to));
      const actual=await parse(after[1].tex,'dsl');assert.equal(flatten(actual.document.root).find(n=>n.namespace===NS&&local(n)==='mspace').attributes.width,'24pt');
      await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),original);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);await h.until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===2));h.report.editorProof={escapeExact:true,oneUndoExact:true,duplicateSourceRangeIsolated:true,renderedWidthsPt:await assertRenderedSpaces('#preview-pane'),originalRanges:ranges.map(({from,to,bodyFrom,bodyTo})=>({from,to,bodyFrom,bodyTo})),changedRanges:after.map(({from,to,bodyFrom,bodyTo})=>({from,to,bodyFrom,bodyTo})),originalSha256:sha(original),changedSha256:sha(changed)};
    });
    await step(groups[9],async()=>{
      assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);assert.deepEqual(fs.readdirSync(h.docs),['Synthetic-space-workshop.md']);
      await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);await h.until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===2));assert.equal(await page.evaluate(()=>state.original),changed);h.report.reopenedWidthsPt=await assertRenderedSpaces('#content');await screenshot('space-saved-reopened-1160.png','Actual saved and reopened body');
      await page.evaluate(()=>ReadMDSession.flush());await h.stop();h.report.beforeRestartOwned=ownedInventory(h.nativeData);assert.deepEqual(h.report.beforeRestartOwned.activeOwned,[]);await boot();await open();assert.equal(await page.evaluate(()=>state.original),changed);assert.equal(fs.readFileSync(file,'utf8'),changed);assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);h.report.restartedWidthsPt=await assertRenderedSpaces('#content');
      assert.deepEqual(Object.fromEntries(sourceFiles.map(p=>[p,fileHash(path.join(root,p))])),h.report.sourceSha256);assert.deepEqual(Object.fromEntries(Object.keys(h.report.qaDependencySha256).map(p=>[p,fileHash(path.join(root,p))])),h.report.qaDependencySha256);assert.equal(fileHash(process.env.READMD_BIN),h.report.candidateSha256);assert.equal(fileHash(path.join(assets,'readmd.boot.js')),bootSha.toLowerCase());assert.equal(fileHash(mathFont),h.report.independentFont.sha256);assert.equal(fileHash(python),h.report.independentRuntime.sha256);assert.deepEqual(pageErrors,[]);h.report.savedSourceSha256=fileHash(file);
    });h.report.passed=true;
  }catch(error){h.report.passed=false;h.report.failedStage=stage;h.report.error=String(error.stack||error).replaceAll(root,'<workspace>');process.exitCode=1;if(page)try{h.report.failureScreenshot=await captureFailureScreenshot(page,root,h.data);}catch(e){h.report.failureScreenshotError=e.name;}}
  finally{try{await h.stop();}catch(e){h.report.passed=false;h.report.shutdownError=e.message;process.exitCode=1;}try{h.report.ownedStop=ownedInventory(h.nativeData);assert.deepEqual(h.report.ownedStop.activeOwned,[]);}catch(e){h.report.passed=false;h.report.ownedStopError=e.message;process.exitCode=1;}
    h.report.pageErrors=pageErrors;h.report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(h.data,'report.json'),JSON.stringify(h.report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({passed:h.report.passed,checks:h.report.checks.length,report:path.relative(root,path.join(h.data,'report.json')),sha256:fileHash(path.join(h.data,'report.json')),failedStage:h.report.failedStage,hostStopped:h.report.ownedStop?.activeOwned.length===0}));}
}
main().catch(error=>{console.error(String(error.stack||error));process.exitCode=1;});
