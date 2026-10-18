'use strict';
// Synthetic fixtures in the real Windows host, formal CSP and real file I/O.
// This certifies the differential/phantom/box spacing increment, not Office
// interoperability or the rest of OMML's layout semantics.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-spacing-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);const file=path.join(docs,'Spacing-workshop.md');
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,passed:false,checks:[],pageErrors:[],wordRoundTrip:'unverified',scope:'Differential, transparent phantom spacing and explicit box line breaking.'};
let host,page,stage='startup';
// Preserve the first connection failure instead of letting the shared session
// helper hide a sandbox/network error as a generic readiness timeout.
async function startHost(){
 report.cdpConnections??=[];
 const diagnosticChromium={connectOverCDP:async(endpoint,options)=>{
  try{return await chromium.connectOverCDP(endpoint,options);}
  catch(error){
   const message=String(error.message),prior=report.cdpConnections.find(e=>e.endpoint===endpoint&&e.message===message);
   if(prior){prior.attempts++;prior.lastAt=new Date().toISOString();}
   else report.cdpConnections.push({endpoint,name:error.name,message,attempts:1,firstAt:new Date().toISOString(),lastAt:new Date().toISOString()});
   throw error;
  }
 }};
 return startNative(root,data,diagnosticChromium,await port());
}
const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const run=t=>'<m:r><m:t>'+t+'</m:t></m:r>',equation=body=>'<m:oMath>'+body+'</m:oMath>';
const box=(body,props)=>'<m:box><m:boxPr>'+props+'</m:boxPr><m:e>'+body+'</m:e></m:box>';
const phantom=(body,transp,show=true)=>'<m:phant><m:phantPr><m:show m:val="'+Number(show)+'"/><m:transp m:val="'+Number(transp)+'"/></m:phantPr><m:e>'+body+'</m:e></m:phant>';
const diff=enabled=>equation(run('x')+box(run('dx'),'<m:diff m:val="'+Number(enabled)+'"/>'));
const plus=transp=>equation(run('x')+phantom(run('+'),transp)+run('z'));
const multi=run('+')+run('y')+run('+');
const xmlSources=[diff(false),diff(true),plus(false),plus(true),equation(run('x')+multi+run('z')),equation(run('x')+phantom(phantom(multi,true),true)+run('z')),equation(run('x')+phantom(multi,true,false)+run('z')),diff(false),diff(false)];
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function stopNative(){if(!host)return;const owned=host;host=null;await owned.browser.close().catch(()=>{});const child=owned.server.child;if(child.exitCode===null&&child.signalCode===null){await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('Owned native process did not stop')),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill();});}}
async function until(check){const end=Date.now()+45000;while(!await check()){assert(Date.now()<end,'Timed out: '+stage);await new Promise(r=>setTimeout(r,80));}}
async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});}
async function serialize(source,dialect,target){const r=await api('/api/math/serialize',{source,dialect,target,display:true});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true);return r.data.output;}
async function parsed(source,extra={}){const r=await api('/api/math/parse',{source,dialect:'omml',display:true,font_size:12,...extra});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true);assert(r.data.vector_preview);return r.data;}
async function vectorGeometry(svg){return page.evaluate(svg=>{const d=new DOMParser().parseFromString(svg,'image/svg+xml'),el=d.documentElement;return{width:el.viewBox.baseVal.width/1000,height:el.viewBox.baseVal.height/1000,cssWidthEm:parseFloat(el.getAttribute('width')),paths:el.querySelectorAll('[data-math-ink] path:not([stroke])').length,lines:el.querySelectorAll('[data-math-line-baseline]').length,glyphs:[...el.querySelectorAll('[data-math-ink] path:not([stroke])')].map(p=>{const m=p.transform.baseVal.consolidate().matrix;return{x:m.e/1000,y:m.f/1000};})};},svg);}
async function mathmlGeometry(markup){return page.evaluate(markup=>{const host=document.createElement('div');host.style.cssText='position:fixed;left:0;top:0;background:white;color:black;font-size:16px';host.innerHTML=markup;document.body.append(host);try{const row=host.querySelector('math > mrow');return{width:row.getBoundingClientRect().width,text:host.textContent,spaces:[...host.querySelectorAll('mspace')].map(e=>e.getBoundingClientRect().width),tokens:[...host.querySelectorAll('mi,mo,mn')].map(e=>{const b=e.getBoundingClientRect(),s=getComputedStyle(e);return{tag:e.localName,text:e.textContent,x:b.x,y:b.y,width:b.width,fontSize:s.fontSize,mathDepth:s.mathDepth,mathStyle:s.mathStyle};})};}finally{host.remove();}},markup);}
async function scriptSpacingProbe(markup){
 const variants=await page.evaluate(markup=>{
  const stripped=new DOMParser().parseFromString(markup,'application/xml');
  for(const e of [...stripped.querySelectorAll('mrow[id]')])if(e.children.length===1&&['mi','mo','mn'].includes(e.firstElementChild.localName))e.replaceWith(e.firstElementChild);
  const zero=new DOMParser().parseFromString(markup,'application/xml');
  for(const e of zero.querySelectorAll('msup > :nth-child(2) mo')){e.setAttribute('lspace','0em');e.setAttribute('rspace','0em');}
  const xml=new XMLSerializer();return{withoutRunIdentityWrappers:xml.serializeToString(stripped),zeroNativeOperatorSpacing:xml.serializeToString(zero)};
 },markup);
 const result={};for(const [variant,xml]of Object.entries(variants)){result[variant]={markup:xml,geometry:await mathmlGeometry(xml)};}
 return result;
}
function near(a,b,tolerance=.00001){assert(Math.abs(a-b)<tolerance,`${a} != ${b}`);}
async function openFixture(){await page.evaluate(async f=>{await i18n.setLanguage('zh-CN',false);await loadFile(f);},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===xmlSources.length));}
(async()=>{try{
 assert(process.env.READMD_BIN,'Explicit current binary required');process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');
 report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 host=await startHost();page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});
 report.browserVersion=host.browser.version();report.csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
 const packagedCsp=fs.readFileSync(path.join(root,'assets/index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)[1];assert.equal(report.csp,packagedCsp);
 const dsl=[];for(const xml of xmlSources)dsl.push(await serialize(xml,'omml','dsl'));
 const block=s=>'```readmd-math v1\n'+s+'\n```';
 const source=['# Mathematical spacing workshop','','## Integrands and differentials','A differential has typographic spacing without changing the characters.','',...dsl.slice(0,2).flatMap(s=>[block(s),'']),'## Phantom spacing and visibility','Transparent spacing preserves the operator context. Hidden ink is an independent choice.','',...dsl.slice(2,7).flatMap(s=>[block(s),'']),'## Independent repeated expressions','',...dsl.slice(7).flatMap(s=>[block(s),''])].join('\n');fs.writeFileSync(file,source);fs.writeFileSync(path.join(data,'original.md'),source);await openFixture();
 stage='actual shared vector geometry';report.vectors=[];report.mathml=[];
 for(const [index,xml]of xmlSources.entries()){const r=await parsed(xml);fs.writeFileSync(path.join(data,`vector-${index}.svg`),r.vector_preview);fs.writeFileSync(path.join(data,`mathml-${index}.xml`),r.preview);report.vectors.push(await vectorGeometry(r.vector_preview));report.mathml.push(await mathmlGeometry(r.preview));}
 const v=report.vectors,m=report.mathml;near(v[1].width-v[0].width,1/6);near(v[1].glyphs[1].x-v[0].glyphs[1].x,1/6);assert.equal(v[1].paths,v[0].paths);
 near(v[3].width-v[2].width,8/18);near(v[4].width,v[5].width);near(v[4].width,v[6].width);assert.equal(v[6].paths,2);
 report.checks.push('The real math API paints differential thin spacing, preserves nested phantom operator context, and separates visibility from advance.');
 stage='actual generated MathML geometry';near(m[1].width-m[0].width,16/6,.2);near(m[3].width-m[2].width,16*8/18,.3);assert.equal(m[0].text,m[1].text);assert.equal(m[2].text,m[3].text);near(m[4].width,m[5].width,.3);near(m[4].width,m[6].width,.3);
 report.checks.push('Real service-generated MathML produces measured browser spacing without duplicate native operator margins or fake text.');
 stage='script-level spacing geometry';report.scriptGeometry=[];
 for(const operator of ['+','=']){
  const script=body=>equation('<m:sSup><m:e>'+run('x')+'</m:e><m:sup>'+body+'</m:sup></m:sSup>');
  const a=await parsed(script(run('a')+run(operator)+run('b'))),b=await parsed(script(run('a')+phantom(run(operator),true)+run('b')));
  const av=await vectorGeometry(a.vector_preview),bv=await vectorGeometry(b.vector_preview),am=await mathmlGeometry(a.preview),bm=await mathmlGeometry(b.preview);
  const name=operator==='+'?'binary':'relation';
  for(const [kind,result] of [['plain',a],['phantom',b]]){fs.writeFileSync(path.join(data,`script-${name}-${kind}.xml`),result.preview);fs.writeFileSync(path.join(data,`script-${name}-${kind}.svg`),result.vector_preview);}
  const diagnostic=await scriptSpacingProbe(a.preview);report.scriptGeometry.push({operator,vector:[av,bv],mathml:[am,bm],diagnostic});
  near(av.width,bv.width);near(am.width,bm.width,.4);assert.equal(bm.spaces.length,0);
 }
 report.checks.push('Actual superscript MathML and vector geometry agree: transparent binary and relation phantoms add no display-style spacing.');
 stage='explicit box wrapping';report.wrapping=[];
 for(const noBreak of [false,true]){const xml='<m:mathPr><m:wrapIndent m:val="0"/></m:mathPr>'+equation(box(run('a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p'),'<m:noBreak m:val="'+Number(noBreak)+'"/>'));const r=await parsed(xml,{layout_width:90});const g=await vectorGeometry(r.vector_preview);report.wrapping.push(g);const ys=new Set(g.glyphs.map(p=>p.y.toFixed(5)));assert(noBreak?ys.size===1:ys.size>1);assert.equal(g.paths,31);}
 report.checks.push('Explicit noBreak=false really wraps box contents at operators; true keeps every glyph on one baseline.');
 await page.locator('#content .readmd-dsl-math').nth(3).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'spacing-1160-light.png')});
 stage='precise formula property edit';await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===dsl.length));await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(8).click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="boxPr"] > summary').click();
 const input=page.locator('[data-math-focus="boxPr.diff"]');await input.selectOption('true');await until(()=>input.inputValue().then(v=>v==='true'));await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());assert.equal(await page.evaluate(()=>getEditContent()),source);await page.locator('.formula-inline-controls button').first().click();
 const changed=await page.evaluate(()=>getEditContent()),ranges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.notEqual(changed,source);assert.equal(ranges[7].tex,dsl[7]);assert((await serialize(ranges[8].tex,'dsl','omml')).includes('<m:diff m:val="true"/>'));await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
 report.checks.push('Editing the second duplicate through native formula properties changes only that node, and one body undo/redo restores exact source.');
 stage='save and reload';assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);assert.deepEqual(fs.readdirSync(docs),['Spacing-workshop.md']);await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===dsl.length));assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);
 report.checks.push('Actual atomic save and reopening retain the readable DSL without side files or dirty state.');
 await page.setViewportSize({width:1024,height:680});await page.evaluate(()=>{state.theme='dark';applySettings();});await page.locator('#content .readmd-dsl-math').nth(3).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'spacing-1024-dark.png')});
 stage='real owned process restart';await page.evaluate(()=>ReadMDSession.flush());await stopNative();host=await startHost();page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await openFixture();assert.equal(fs.readFileSync(file,'utf8'),changed);assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);assert.deepEqual(report.pageErrors,[]);report.checks.push('A real process restart preserves the saved formula changes under formal CSP.');report.passed=true;
}catch(error){report.failedStage=stage;report.error=error.stack;process.exitCode=1;if(page){report.failureState=await page.evaluate(()=>({formulaDraftActive:ReadMDInlineMath.active(),body:getEditContent(),focusedTag:document.activeElement?.localName,focusedField:document.activeElement?.dataset?.mathFocus})).catch(()=>null);await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});}}
finally{report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));try{await stopNative();}catch(error){report.passed=false;report.cleanupError=error.stack;process.exitCode=1;fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));}console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
