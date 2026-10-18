'use strict';
// Prepared M04 QA. This uses the actual native service and packaged CSP.
// Synthetic inputs and independent FontTools metrics are public QA material;
// Word compatibility and standalone MathML distribution remain separate gates.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const fileHash=file=>hash(fs.readFileSync(file));
const oraclePath=path.join(root,'rust/readmd-kernel/tests/fixtures/math-font-geometry-stix.json');
const oracle=JSON.parse(fs.readFileSync(oraclePath,'utf8'));
const escape=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
const run=(text,properties='')=>'<m:r><m:rPr><m:sty m:val="p"/>'+properties+'</m:rPr><m:t>'+escape(text)+'</m:t></m:r>';
const array=(maximum=false,flush=false,rows=[run('a&b&x&y'),run('x&y&b&a')])=>'<m:eqArr><m:eqArrPr><m:maxDist m:val="'+Number(maximum)+'"/><m:objDist m:val="'+Number(flush)+'"/></m:eqArrPr>'+rows.map(row=>'<m:e>'+row+'</m:e>').join('')+'</m:eqArr>';
const equation=body=>'<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'+body+'</m:oMath>';
const local=node=>node.name.split(':').at(-1);
const flatten=node=>[node,...node.content.filter(p=>p.type==='element').flatMap(p=>flatten(p.value))];
const text=node=>node.content.map(p=>p.type==='text'?p.value:p.type==='element'?text(p.value):'').join('');
const near=(actual,expected,label,tolerance=.00003)=>assert(Number.isFinite(actual)&&Math.abs(actual-expected)<=tolerance,`${label}: actual=${actual}, expected=${expected}, tolerance=${tolerance}`);
const glyph=c=>oracle.glyphs[oracle.characters[c]];
const advance=c=>glyph(c).advance/oracle.unitsPerEm;
function coordinates(maximum,flush,size,width){
  const c=[Math.max(advance('a'),advance('x'))+Math.max(advance('b'),advance('y')),
    Math.max(advance('x'),advance('b'))+Math.max(advance('y'),advance('a'))].map(v=>v*size);
  const natural=c[0]+c[1],actual=maximum?Math.max(width,natural):natural,extra=actual-natural;
  const outer=flush?0:extra/3,gap=flush?extra:extra/3;
  return {width:actual,natural,anchors:[outer+Math.max(advance('a'),advance('x'))*size,
    outer+c[0]+gap+Math.max(advance('x'),advance('b'))*size]};
}
function assertCrossRunPresentation(value,source){
  assert.equal(value.source,source,'Actual parse.document.source must preserve original OMML');
  assert.equal(value.presentationText,'abxyabxy','Only presentation text determines visible marker leakage');
  assert.equal(value.rows.length,2);
  for(const row of value.rows){
    assert.deepEqual(row.map(cell=>cell.span),[1,1,1,1,1,1,2]);
    assert.equal(row.reduce((sum,cell)=>sum+cell.span,0),8);
    assert.deepEqual(row.map(cell=>cell.text),['a','b','','','x','y','']);
    assert.deepEqual(row.flatMap(cell=>cell.tokens),['a','b','x','y']);
  }
}
function assertAdjacentCapacity(diagnostics,width){
  // Two independent arrays cannot share the same full container capacity.
  // The natural width comes from the separately generated STIX font oracle.
  const natural=coordinates(false,false,12,width).natural;
  if(width>=2*natural){assert.deepEqual(diagnostics,[]);return;}
  assert.equal(diagnostics.length,2);
  assert.equal(new Set(diagnostics.map(issue=>issue.node_id)).size,2);
  for(const issue of diagnostics){
    assert.equal(issue.code,'math_equation_array_overflow');
    near(issue.required_points,natural,'Adjacent array intrinsic requirement');
    near(issue.available_points,width-natural,'Adjacent array remaining capacity');
  }
}
const workflows=[
  'candidate, served boot, packaged CSP and independent font identity',
  'all maxDist/objDist combinations at 12/24pt with independent painted coordinates',
  'cross-run and empty-column markers, UTF-16 source owners and explicit paragraph alignment',
  'narrow/nested local capacities retain ink and report physical overflow',
  'real patch, stale revision and invalid width reject without changing source',
  'native MathML intrinsic columns plus explicit unimplemented-distribution warning',
  'actual CSS content-box units, localization and warning removal using real service results',
  'second repeated formula property change, Esc, one body undo/redo and source-range isolation',
  'real save, file reopen, process restart, unchanged dependencies and owned-host stop',
];
if(process.argv.includes('--plan')){
  console.log(JSON.stringify({scope:'prepared-M04-native-QA',nativeExecuted:false,productInvocations:0,workflows,
    required:['READMD_BIN','READMD_CANDIDATE_SHA256','READMD_CANDIDATE_BOOT_SHA256'],
    wordApplication:'unverified',standaloneMathmlDistribution:'explicit-warning-not-certified'},null,2));process.exit(0);
}
if(process.argv.includes('--self-test')){
  near(coordinates(false,false,12,96).natural,24.084,'Independent 12pt natural width');
  near(coordinates(false,true,12,96).anchors[1],18.288,'Independent intrinsic second anchor');
  near(coordinates(true,true,12,96).anchors[1],90.204,'Independent flush second anchor');
  near(coordinates(true,false,12,96).anchors[0],29.732,'Independent outer-gap anchor');
  assert.throws(()=>near(90.5,90.204,'Wrong physical anchor'),/Wrong physical anchor/);
  assert.throws(()=>near(NaN,90.204,'Invalid metric'),/Invalid metric/);
  assert.ok(equation(array(true,false)).includes('a&amp;b&amp;x&amp;y'));
  const presentation={source:'own original OMML',presentationText:'abxyabxy',rows:Array.from({length:2},()=>['a','b','','','x','y',''].map((text,index)=>({text,tokens:text?[text]:[],span:index===6?2:1})))};
  assertCrossRunPresentation(presentation,'own original OMML');
  for(const change of [value=>{value.rows[0][0].tokens=['a','&'];},value=>{value.rows[0][1].text='';},value=>{value.rows[1][6].span=1;},value=>{value.source='different source';}]){
    const negative=structuredClone(presentation);change(negative);assert.throws(()=>assertCrossRunPresentation(negative,'own original OMML'));
  }
  const overflow=[0,1].map(index=>({node_id:'array-'+index,code:'math_equation_array_overflow',required_points:24.084,available_points:23.916}));
  assertAdjacentCapacity(overflow,48);assertAdjacentCapacity([],96);
  assert.throws(()=>assertAdjacentCapacity([],48));
  assert.throws(()=>assertAdjacentCapacity(overflow,96));
  assert.throws(()=>assertAdjacentCapacity(overflow.map(issue=>({...issue,available_points:48})),48));
  console.log(JSON.stringify({scope:'pure-QA-expectation-controls',nativeExecuted:false,productInvocations:0,controls:17,passed:17}));process.exit(0);
}
const {createHarness,captureFailureScreenshot}=require('../tools/test/publication-batch3-harness.cjs');
async function freePort(){const server=net.createServer();await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});const port=server.address().port;await new Promise(resolve=>server.close(resolve));return port;}
function ownedInventory(directory){
  const quoted=directory.replaceAll("'","''");
  const script="$owned='"+quoted+"'; $rows=@(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.CommandLine -and $_.CommandLine.IndexOf($owned,[StringComparison]::OrdinalIgnoreCase) -ge 0 -and $_.Name -match '^(readmd|msedgewebview2|xelatex|xdvipdfmx)\\.exe$' } | ForEach-Object { [pscustomobject]@{pid=$_.ProcessId;parentPid=$_.ParentProcessId;name=$_.Name} }); ConvertTo-Json -InputObject $rows -Compress";
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{encoding:'utf8',windowsHide:true,timeout:15000});
  assert.equal(result.status,0,'Owned-process inventory failed: '+String(result.error?.code||result.stderr).slice(0,200));
  const value=JSON.parse(result.stdout.trim()||'[]');assert(Array.isArray(value));return {observedAt:new Date().toISOString(),activeOwned:value};
}
async function main(){
  assert.equal(process.platform,'win32','Real Windows native host required');
  const h=createHarness('math-equation-array-native',await freePort(),__filename);
  let page,stage='startup',apiCount=0,changed,source,originalDsl,sourceRanges;
  const errors=[],responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  const assets=process.env.READMD_ASSETS_DIR||path.join(root,'assets');
  const bootExpected=process.env.READMD_CANDIDATE_BOOT_SHA256||process.env.READMD_BOOT_SHA256;
  const sources=['rust/readmd-kernel/src/math_equation_array.rs','rust/readmd-kernel/src/math_layout.rs',
    'rust/readmd-kernel/src/omml.rs','rust/readmd-kernel/src/math_document.rs','rust/readmd-kernel/src/upgrade_api.rs',
    'rust/readmd-kernel/src/api_codes.rs','assets/js/reader/formula.js','assets/js/editor/formula-inline.js'];
  h.report.independentOracle={path:path.relative(root,oraclePath),sha256:fileHash(oraclePath),fontSha256:oracle.fontSha256};
  h.report.wordApplication='unverified';h.report.mathmlContainerDistribution='explicit-warning-not-certified';
  h.report.checks=[];h.report.geometry=[];h.report.apiRequests=[];h.report.automaticRetries=0;
  h.report.observedMathResponses=[];h.report.apiRequestCountScope='Explicit QA wrapper calls; observedMathResponses additionally includes genuine UI and browser-probe requests.';
  const file=path.join(h.docs,'Equation-array-workshop.md');
  async function screenshot(name,scene){
    const bytes=await page.screenshot();fs.writeFileSync(path.join(h.data,name),bytes,{flag:'wx'});
    (h.report.screenshots??=[]).push({path:name,sha256:hash(bytes),bytes:bytes.length,viewport:page.viewportSize(),scene,visualReview:'pending root'});
  }
  const step=async(name,fn)=>{stage=name;return h.step(name,fn);};
  async function api(route,body,expected=200){
    const start=Date.now(),result=await h.api(route,body);apiCount++;
    const record={route,body,response:result,elapsedMs:Date.now()-start};
    const name=String(apiCount).padStart(3,'0')+'.json';fs.writeFileSync(path.join(responses,name),JSON.stringify(record,null,2)+'\n',{flag:'wx'});
    h.report.apiRequests.push({index:apiCount,route,status:result.status,elapsedMs:record.elapsedMs,artifact:'responses/'+name,sha256:fileHash(path.join(responses,name))});
    assert.equal(result.status,expected,route+' '+JSON.stringify(result.data).slice(0,1000));
    if(expected===200)assert.equal(result.data.ok,true);return result.data;
  }
  const parse=(xml,extra={})=>api('/api/math/parse',{source:xml,dialect:'omml',display:true,font_size:12,...extra});
  const serialize=async(value,dialect,target)=>(await api('/api/math/serialize',{source:value,dialect,target,display:true}));
  async function geometry(value,size=12){
    return page.evaluate(({svg,size})=>{
      const parsed=new DOMParser().parseFromString(svg,'image/svg+xml');if(parsed.querySelector('parsererror'))throw Error('Invalid actual SVG');
      const element=parsed.documentElement;
      const glyphs=[...element.querySelectorAll('[data-math-ink] path:not([stroke])')].map(path=>{
        const matrix=path.transform.baseVal.consolidate().matrix;return{x:matrix.e/1000*size,y:matrix.f/1000*size,
          owners:path.parentElement.getAttribute('data-math-owners'),outline:path.getAttribute('d')};});
      const regions=[...element.querySelectorAll('[data-math-selection-regions] [data-math-node]')].map(group=>({id:group.id,kind:group.dataset.mathNode,
        from:group.hasAttribute('data-math-text-from')?Number(group.dataset.mathTextFrom):null,
        to:group.hasAttribute('data-math-text-to')?Number(group.dataset.mathTextTo):null,
        widths:[...group.querySelectorAll('rect')].map(rect=>Number(rect.getAttribute('width'))/1000*size)}));
      return{glyphs,regions};
    },{svg:value.vector_preview,size});
  }
  async function inkContained(value,fontSize=16){
    return page.evaluate(async({value,fontSize})=>{
      const probe=document.createElement('div');probe.dataset.eqarrQa='ink';probe.style.cssText='position:fixed;left:0;top:0;background:white;color:black;font-size:'+fontSize+'px';document.body.append(probe);
      try{const rendered=await renderMathDocument(value,true);probe.append(rendered);const svg=rendered.querySelector('svg'),ink=svg.querySelector('[data-math-ink]');
        const a=svg.getBoundingClientRect(),b=ink.getBoundingClientRect();return{svg:{x:a.x,y:a.y,right:a.right,bottom:a.bottom,width:a.width},ink:{x:b.x,y:b.y,right:b.right,bottom:b.bottom,width:b.width}};
      }finally{probe.remove();}
    },{value,fontSize});
  }
  function contained(value){for(const key of ['x','y'])assert(value.ink[key]>=value.svg[key]-.8,'Ink escaped SVG '+key);for(const key of ['right','bottom'])assert(value.ink[key]<=value.svg[key]+.8,'Ink clipped '+key);}
  async function boot(){page=await h.start();page.on('pageerror',error=>errors.push(error.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));
    page.on('response',response=>{const route=new URL(response.url()).pathname;if(route.startsWith('/api/math/'))h.report.observedMathResponses.push({route,status:response.status(),method:response.request().method()});});
    const csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    const expected=fs.readFileSync(path.join(assets,'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)[1];assert.equal(csp,expected);
    assert.equal(h.report.servedBootSha256,bootExpected.toLowerCase());h.report.csp=csp;await page.setViewportSize({width:1160,height:820});}
  async function open(){await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file,{force:true,discardConfirmed:true});},file);
    await h.until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(count=>count===6));}
  async function propertyEditor(){await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(5).click();
    await h.until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-properties > summary').click();
    await h.until(()=>page.locator('[data-math-container="eqArrPr"] > summary').count());await page.locator('[data-math-container="eqArrPr"] > summary').click();
    await page.locator('[data-math-focus="eqArrPr.objDist"]').selectOption('true');
    await h.until(()=>page.locator('.formula-inline-controls button').first().isEnabled());}
  try{
    await step(workflows[0],async()=>{
      assert.match(bootExpected||'',/^[a-f0-9]{64}$/i,'Root supplies the final boot hash');
      assert.equal(fileHash(path.join(assets,'readmd.boot.js')),bootExpected.toLowerCase());
      h.report.sourceSha256=Object.fromEntries(sources.map(file=>[file,fileHash(path.join(root,file))]));
      assert.equal(fileHash(path.join(assets,'fonts/STIXTwoMath-Regular.ttf')),oracle.fontSha256);await boot();return{servedBootSha256:h.report.servedBootSha256};
    });
    await step(workflows[1],async()=>{
      for(const size of [12,24])for(const maximum of [false,true])for(const flush of [false,true]){
        const xml=equation(array(maximum,flush)),value=await parse(xml,{font_size:size,layout_width:96});
        assert.equal(value.document.source,xml);assert.deepEqual(value.layout_diagnostics,[]);
        const actual=await geometry(value,size),expected=coordinates(maximum,flush,size,96);assert.equal(actual.glyphs.length,8);
        near(actual.regions.find(region=>region.kind==='eqArr').widths[0],expected.width,'Array logical width');
        for(const index of [1,5])near(actual.glyphs[index].x,expected.anchors[0],'First painted anchor');
        for(const index of [3,7])near(actual.glyphs[index].x,expected.anchors[1],'Second painted anchor');
        contained(await inkContained(value,size/0.75));h.report.geometry.push({phase:'four-flags',size,maximum,flush,expected,actual});
      }
    });
    await step(workflows[2],async()=>{
      const split=equation(array(false,false,[run('a&')+run('b&&')+run('&x&y&'),run('a&b&&&x&y&')]));
      const value=await parse(split,{layout_width:96});assert.deepEqual(value.layout_diagnostics,[]);assert.equal(value.document.source,split);
      const mathml=await serialize(split,'omml','mathml');assert.equal((mathml.output.match(/<mtd /g)||[]).length,14);
      const presentation=await page.evaluate(markup=>{
        const document=new DOMParser().parseFromString(markup,'application/xml');if(document.querySelector('parsererror'))throw Error('Invalid actual MathML');
        const altText=document.documentElement.getAttribute('alttext'),visible=document.documentElement.cloneNode(true);
        // Metadata/alternative text preserves author ampersands. It does not
        // participate in painted MathML token/cell content.
        for(const annotation of visible.querySelectorAll('annotation,annotation-xml'))annotation.remove();
        return {altText,presentationText:visible.textContent,rows:[...visible.querySelectorAll('mtr')].map(row=>[...row.children].map(cell=>({
          span:Number(cell.getAttribute('columnspan')||1),text:cell.textContent,tokens:[...cell.querySelectorAll('mi,mn,mo,mtext')].map(token=>token.textContent),
        })))};
      },mathml.output);
      presentation.source=value.document.source;
      h.report.altTextDiagnostic={present:presentation.altText!==null,matchesOriginalOmml:presentation.altText===split,
        scope:'Existing alternative-text identity observation only; raw XML is not certified as accessible mathematical description. M14 follow-up required.'};
      delete presentation.altText;assertCrossRunPresentation(presentation,split);h.report.crossRunPresentation=presentation;
      const utf16=equation(array(false,false,[run('𝑎&b&x&y'),run('x&y&b&a')]));
      const utf=await parse(utf16,{layout_width:96}),g=await geometry(utf);assert.equal(utf.document.source,utf16);
      const firstRun=flatten(utf.document.root).find(node=>local(node)==='r'),owner='rd-formula-token-omml-'+firstRun.id.replaceAll('.','-');
      for(const [from,to]of [[0,2],[3,4],[5,6],[7,8]])assert(g.regions.some(region=>region.id===owner+'-text-'+from&&region.from===from&&region.to===to),'Original UTF-16 owner missing '+from);
      assert.equal(g.glyphs.length,8);assert(!g.regions.some(region=>region.id.includes('-text-0-text-')));
      const para='<m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">'+
        '<m:oMath>'+run('a')+run('-x','<m:aln/>')+'</m:oMath><m:oMath>'+run('123')+run('-y','<m:aln/>')+'</m:oMath></m:oMathPara>';
      const aligned=await parse(para,{layout_width:96}),ag=await geometry(aligned);assert.deepEqual(aligned.layout_diagnostics,[]);
      near(ag.glyphs[1].x,ag.glyphs[6].x,'Explicit run first minus anchor');
      const box=word=>'<m:box><m:boxPr><m:opEmu/><m:aln/></m:boxPr><m:e>'+run(word)+'</m:e></m:box>';
      const bp='<m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:oMath>'+run('a')+box('-x')+'</m:oMath><m:oMath>'+run('123')+box('-y')+'</m:oMath></m:oMathPara>';
      const bg=await geometry(await parse(bp,{layout_width:96}));near(bg.glyphs[1].x,bg.glyphs[6].x,'Explicit operator-box anchor');
      h.report.geometry.push({phase:'cross-run-empty-utf16',split:await geometry(value),utf16:g,paragraph:ag,operatorBox:bg});
    });
    await step(workflows[3],async()=>{
      const basic=array(true,true),nested=[
        '<m:f><m:num>'+basic+'</m:num><m:den>'+run('1')+'</m:den></m:f>',
        '<m:rad><m:radPr><m:degHide/></m:radPr><m:e>'+basic+'</m:e></m:rad>',
        '<m:d><m:e>'+basic+'</m:e></m:d>',
        '<m:borderBox><m:e>'+basic+'</m:e></m:borderBox>',
        '<m:func><m:fName>'+run('sin')+'</m:fName><m:e>'+basic+'</m:e></m:func>',
        '<m:m><m:mr><m:e>'+basic+'</m:e><m:e>'+run('1')+'</m:e></m:mr></m:m>',basic+basic,
      ];
      for(const width of [48,96])for(const [index,body]of nested.entries()){
        const value=await parse(equation(body),{layout_width:width});
        if(index===6)assertAdjacentCapacity(value.layout_diagnostics,width);else assert.deepEqual(value.layout_diagnostics,[]);
        const actual=await geometry(value),arrays=actual.regions.filter(region=>region.kind==='eqArr');assert(arrays.length>=1);
        for(const region of arrays){assert(region.widths[0]<=width+.00003);assert(region.widths[0]>0);}
        contained(await inkContained(value));h.report.geometry.push({phase:'nested',width,index,arrays});
      }
      const inner=array(true,true),outer=array(true,true,[inner+run('&a&x&y'),run('123123&b&x&y')]);
      const localValue=await parse(equation(outer),{layout_width:120}),localGeometry=await geometry(localValue);
      assert.deepEqual(localValue.layout_diagnostics,[]);
      const innerNode=flatten(localValue.document.root).filter(node=>local(node)==='eqArr')[1];
      const innerId='rd-formula-token-omml-'+innerNode.id.replaceAll('.','-');
      const innerRegion=localGeometry.regions.find(region=>region.id===innerId);
      near(innerRegion.widths[0],6*advance('1')*12,'Inner array uses its left column capacity, not outer spacers');
      assert(innerRegion.widths[0]<60);h.report.geometry.push({phase:'inner-column',innerRegion});
      const xml=equation(array(true,true)),narrow=await parse(xml,{layout_width:10});assert.equal(narrow.document.source,xml);
      const issue=narrow.layout_diagnostics.find(issue=>issue.code==='math_equation_array_overflow');assert(issue);
      near(issue.required_points,24.084,'Physical required points');near(issue.available_points,10,'Physical available points');
      assert.match(issue.node_id,/^rd-formula-token-omml-/);contained(await inkContained(narrow));
      for(const [body,code]of [
        [array(true,false,[run('A&B','<m:nor/>'),run('C&D','<m:nor/>')]),'math_equation_array_literal_alignment'],
        [array(true,false,[run('a&b','<m:aln/>'),run('x&y')]),'math_equation_array_mixed_alignment'],
      ]){const source=equation(body),value=await parse(source,{layout_width:96});assert.equal(value.document.source,source);assert(value.layout_diagnostics.some(issue=>issue.code===code));assert.equal((await geometry(value)).glyphs.length,6,'Ambiguous rows keep literal ampersand ink');contained(await inkContained(value));}
    });
    await step(workflows[4],async()=>{
      const xml=equation(array(true,false)),original=await parse(xml,{layout_width:96}),node=flatten(original.document.root).find(node=>local(node)==='eqArr');
      const body={source:xml,dialect:'omml',display:true,font_size:12,layout_width:96,expected_revision:original.document.revision,
        patch:{operation:'set_property',node_id:node.id,container:'eqArrPr',property:'objDist',value:'true'}};
      const patched=await api('/api/math/patch',body);assert.notEqual(patched.document.source,xml);assert.equal(original.document.source,xml);
      const expected=coordinates(true,true,12,96),actual=await geometry(patched);near(actual.glyphs[3].x,expected.anchors[1],'Patched flush anchor');
      const stale=await api('/api/math/patch',{...body,source:patched.document.source},409);assert.equal(stale.error_code,'transaction_conflict');
      for(const width of [0,-1,'96',null,20001]){const value=await api('/api/math/parse',{source:xml,dialect:'omml',display:true,layout_width:width},400);assert.equal(value.error_code,'invalid_json');}
    });
    await step(workflows[5],async()=>{
      const intrinsic=await parse(equation(array(false,false)),{layout_width:96});
      const measured=await page.evaluate(preview=>{
        const box=document.createElement('div');box.style.cssText='position:fixed;left:0;top:0;font-size:16px;background:white;color:black';
        const parsed=new DOMParser().parseFromString(preview,'application/xml');box.append(document.importNode(parsed.documentElement,true));document.body.append(box);
        try{return {nativeMathML:box.querySelector('math').namespaceURI,cells:[...box.querySelectorAll('mtd')].map(cell=>{const token=cell.querySelector('mi,mn,mo');return {cell:cell.getBoundingClientRect().x,token:token?.getBoundingClientRect().x,text:cell.textContent};})};}
        finally{box.remove();}
      },intrinsic.preview);
      assert.equal(measured.nativeMathML,'http://www.w3.org/1998/Math/MathML');assert.equal(measured.cells.length,8);
      near(measured.cells[1].token,measured.cells[5].token,'Browser first native MathML anchor',.3);
      near(measured.cells[3].token,measured.cells[7].token,'Browser second native MathML anchor',.3);
      const distributed=await serialize(equation(array(true,true)),'omml','mathml');
      assert(distributed.warns.includes('math_equation_array_mathml_distribution'));
      assert(distributed.warn_items.some(warning=>warning.code==='math_equation_array_mathml_distribution'));
      h.report.nativeMathml=measured;
    });
    await step(workflows[6],async()=>{
      const sample=equation(array(true,true)),narrow=await parse(sample,{layout_width:10}),wide=await parse(sample,{layout_width:96});
      h.report.warningState=await page.evaluate(async({sample,narrow,wide})=>{
        const holder=document.createElement('div');holder.style.cssText='position:fixed;left:0;top:0;width:344px;padding:12px;border:3px solid black;box-sizing:border-box;font-size:16px';document.body.append(holder);
        try{
          const metrics=mathLayoutMetrics(holder,true),rendered=await renderMathDocument(narrow,true);holder.append(rendered);
          const first=rendered.title;await i18n.setLanguage('ar',false);await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
          const translated=rendered.title;applyMathDocumentWarnings(rendered,wide);
          const cleared={title:rendered.title,marker:rendered.hasAttribute('data-readmd-math-warnings'),ownedTitles:rendered.querySelectorAll('[data-readmd-layout-warning-title]').length};
          const contentPixels=holder.clientWidth-parseFloat(getComputedStyle(holder).paddingLeft)-parseFloat(getComputedStyle(holder).paddingRight);
          const actual=await apiFetch('/api/math/parse',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:sample,dialect:'omml',display:true,...metrics})});
          const parsed=await actual.json();if(!actual.ok||!parsed.ok)throw Error('Actual content-capacity parse failed');
          const sized=await renderMathDocument(parsed,true);holder.replaceChildren(sized);
          const region=sized.querySelector('[data-math-node="eqArr"] rect'),physicalWidth=Number(region.getAttribute('width'))/1000*metrics.font_size;
          const cssWidth=region.getBoundingClientRect().width;
          return {metrics,contentPixels,physicalWidth,cssWidth,first,translated,cleared};
        }finally{holder.remove();await i18n.setLanguage('en',false);}
      },{sample,narrow,wide});
      const state=h.report.warningState;near(state.metrics.font_size,12,'CSS font pt');
      near(state.metrics.layout_width,Math.round(state.contentPixels*.75*2)/2,'CSS content width pt');
      near(state.physicalWidth,state.metrics.layout_width,'Logical maxDist allocation');near(state.cssWidth,state.physicalWidth/.75,'CSS display allocation',.1);
      assert(state.cssWidth<=state.contentPixels+.7,'maxDist must fit the actual padded content box');
      assert.notEqual(state.first,state.translated);assert(!state.translated.includes('math_equation_array_'));
      assert.deepEqual(state.cleared,{title:'',marker:false,ownedTitles:0});
    });
    await step(workflows[7],async()=>{
      const xml=[...([false,true].flatMap(maximum=>[false,true].map(flush=>equation(array(maximum,flush))))),equation(array(true,false)),equation(array(true,false))];
      originalDsl=[];for(const item of xml)originalDsl.push((await serialize(item,'omml','dsl')).output);
      source=['# Equation-array workshop 😀','','Only the second repeated formula changes.','',...originalDsl.flatMap((value,index)=>['## Case '+index,'','```readmd-math v1\n'+value+'\n```',''])].join('\n');
      fs.writeFileSync(file,source,{flag:'wx'});fs.writeFileSync(path.join(h.data,'original.md'),source,{flag:'wx'});await open();
      await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
      await h.until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(count=>count===6));
      sourceRanges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.equal(sourceRanges.length,6);assert.equal(sourceRanges[4].tex,sourceRanges[5].tex);
      const second=page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(5);
      h.report.duplicateLocator=await second.evaluate(element=>{const range=ReadMDInlineMath.locate(element);return {from:range.from,to:range.to,tex:range.tex};});
      assert.equal(h.report.duplicateLocator.from,sourceRanges[5].from);assert.equal(h.report.duplicateLocator.to,sourceRanges[5].to);
      await propertyEditor();assert.equal(await page.evaluate(()=>getEditContent()),source);
      await page.locator('.formula-properties').scrollIntoViewIfNeeded();await screenshot('eqarr-property-editing-1160.png','Real property/slot editor before Esc; body still unchanged');
      await page.locator('[data-math-focus="eqArrPr.objDist"]').press('Escape');await h.until(()=>page.evaluate(()=>!ReadMDInlineMath.active()));
      assert.equal(await page.evaluate(()=>getEditContent()),source);assert.equal(fs.readFileSync(file,'utf8'),source);
      await propertyEditor();assert.equal(await page.evaluate(()=>getEditContent()),source);await page.locator('.formula-inline-controls button').first().click();
      await h.until(()=>page.evaluate(()=>!ReadMDInlineMath.active()));changed=await page.evaluate(()=>getEditContent());assert.notEqual(changed,source);
      const ranges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.equal(ranges.length,6);
      for(let index=0;index<5;index++)assert.equal(ranges[index].tex,sourceRanges[index].tex);
      assert.equal(changed.slice(0,ranges[5].from),source.slice(0,sourceRanges[5].from));
      assert.equal(changed.slice(ranges[5].to),source.slice(sourceRanges[5].to));
      const result=await api('/api/math/parse',{source:ranges[5].tex,dialect:'dsl',display:true,font_size:12,layout_width:96});
      const flush=flatten(result.document.root).find(node=>local(node)==='objDist');assert(flush);
      assert(['true','1','on'].includes(Object.entries(flush.attributes).find(([key])=>key.split(':').at(-1)==='val')[1]));
      await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);
      await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
      await h.until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(count=>count===6));
      const painted=[];
      for(const index of [4,5])painted.push(await page.locator('#preview-pane .readmd-dsl-math').nth(index).evaluate(element=>{
        const paths=element.querySelectorAll('[data-math-ink] path:not([stroke])'),size=parseFloat(getComputedStyle(element).fontSize)*.75;
        return {size,first:paths[1].transform.baseVal.consolidate().matrix.e/1000*size,second:paths[3].transform.baseVal.consolidate().matrix.e/1000*size};
      }));
      assert(painted[0].first>painted[1].first+.5,'Changing flush removes the real first outer spacer');
      assert(painted[1].second>painted[0].second+.5,'Changing flush moves the real final group right');h.report.duplicatePaintedAnchors=painted;
      await second.scrollIntoViewIfNeeded();await screenshot('eqarr-edited-light.png','Committed second duplicate after actual undo/redo');
      h.report.sourceRangeIsolation={original:sourceRanges.map(range=>({from:range.from,to:range.to})),after:ranges.map(range=>({from:range.from,to:range.to})),oneUndoExact:true,escapeExact:true};
    });
    await step(workflows[8],async()=>{
      assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);
      assert.deepEqual(fs.readdirSync(h.docs),['Equation-array-workshop.md']);
      await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);
      await h.until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(count=>count===6));
      assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);assert.equal(await page.evaluate(()=>state.original),changed);
      await page.locator('#content .readmd-dsl-math').nth(5).scrollIntoViewIfNeeded();await screenshot('eqarr-saved-reopened-1160.png','Actual saved and reopened document body at 1160 by 820');
      await page.setViewportSize({width:1024,height:680});await page.evaluate(()=>{state.theme='dark';applySettings();});
      await page.locator('#content .readmd-dsl-math').nth(5).scrollIntoViewIfNeeded();await screenshot('eqarr-reopened-dark.png','Saved document in narrow dark reading');
      await page.evaluate(()=>ReadMDSession.flush());await h.stop();const stopped=ownedInventory(h.nativeData);assert.deepEqual(stopped.activeOwned,[]);h.report.beforeRestartOwnedStop=stopped;
      await boot();await open();assert.equal(await page.evaluate(()=>state.original),changed);assert.equal(fs.readFileSync(file,'utf8'),changed);
      assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);
      assert.deepEqual(Object.fromEntries(sources.map(file=>[file,fileHash(path.join(root,file))])),h.report.sourceSha256);
      assert.deepEqual(Object.fromEntries(Object.keys(h.report.qaDependencySha256).map(file=>[file,fileHash(path.join(root,file))])),h.report.qaDependencySha256);
      assert.equal(fileHash(process.env.READMD_BIN),h.report.candidateSha256);assert.equal(fileHash(path.join(assets,'readmd.boot.js')),bootExpected.toLowerCase());
      assert.equal(fileHash(oraclePath),h.report.independentOracle.sha256);assert.deepEqual(errors,[]);
      h.report.savedSourceSha256=fileHash(file);h.report.originalSourceSha256=hash(source);
    });
    h.report.passed=true;
  }catch(error){h.report.passed=false;h.report.failedStage=stage;h.report.error=String(error.stack||error).replaceAll(root,'<workspace>');process.exitCode=1;
    if(page)try{h.report.failureScreenshot=await captureFailureScreenshot(page,root,h.data);}catch(capture){h.report.failureScreenshotError=capture.name;}
  }finally{
    // Shutdown runs even if the independent CIM inventory fails. Preserve the
    // original failed stage and add cleanup failures separately.
    try{await h.stop();}catch(error){h.report.passed=false;h.report.shutdownError=error.message;process.exitCode=1;}
    try{h.report.ownedStop=ownedInventory(h.nativeData);assert.deepEqual(h.report.ownedStop.activeOwned,[]);}
    catch(error){h.report.passed=false;h.report.ownedStopError=error.message;process.exitCode=1;}
    h.report.pageErrors=errors;h.report.recordedAt=new Date().toISOString();
    fs.writeFileSync(path.join(h.data,'report.json'),JSON.stringify(h.report,null,2)+'\n',{flag:'wx'});
    console.log(JSON.stringify({passed:h.report.passed,checks:h.report.checks.length,requests:apiCount,
      report:path.relative(root,path.join(h.data,'report.json')),reportSha256:fileHash(path.join(h.data,'report.json')),failedStage:h.report.failedStage,hostStopped:h.report.ownedStop?.activeOwned.length===0}));
  }
}
main().catch(error=>{console.error(String(error.stack||error));process.exitCode=1;});
