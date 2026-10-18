'use strict';
// Ink geometry is checked in the actual renderer, not inferred from MathML
// attributes or a screenshot count. Keep this gate failing until rendering is
// corrected; the functional native test covers edits and file publication.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-delimiters-visual-'));
const file=path.join(data,'Formula-geometry.md'),hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const source='# 数学结构：检查实际绘制范围\n\n## 一、上标矩阵与贴合括号\n\n```readmd-math v1\ndelimiter(superscript(x, matrix([1], [2], [3])), grow: true, shp: match)\n```\n\n## 二、算子跟随被积式高度\n\n```readmd-math v1\nnary(matrix([a], [b], [c], [d], [e], [f]), character: "∫", from: 0, to: 1, grow: true)\n```\n';
let native,page;const report={platform:'windows-x64',nativeCsp:true,synthetic:true,passed:false,failures:[],pageErrors:[],geometry:[]};
function measure(element,index){
 const box=e=>{const b=e.getBoundingClientRect();return{x:b.x,y:b.y,width:b.width,height:b.height,bottom:b.bottom,right:b.right}};
 const union=nodes=>{if(!nodes.length)throw Error('No actual painted geometry');const boxes=nodes.map(box),x=Math.min(...boxes.map(b=>b.x)),y=Math.min(...boxes.map(b=>b.y)),right=Math.max(...boxes.map(b=>b.right)),bottom=Math.max(...boxes.map(b=>b.bottom));return{x,y,right,bottom,width:right-x,height:bottom-y};};
 const svg=element.querySelector('svg'),ink=svg.querySelector('[data-math-ink]');if(!ink)throw Error('Shared vector renderer missing');
 const owner=svg.querySelector('[data-math-node="'+(index===0?'sSup':'m')+'"]').id;
 const operators=(index===0?['begin-fence','end-fence']:['operator']).map(part=>union([...ink.querySelectorAll('[data-math-parts~="'+part+'"]')]));
 return{kind:index===0?'delimiter-match':'nary-grow',svg:box(svg),ink:box(ink),operand:union([...ink.querySelectorAll('[data-math-owners~="'+owner+'"]')]),operators,overflow:getComputedStyle(svg).overflow};
}
function verify(geometry){
 report.geometry.push(geometry);
 if(geometry.ink.y<geometry.svg.y-1||geometry.ink.bottom>geometry.svg.bottom+1||geometry.ink.x<geometry.svg.x-1||geometry.ink.right>geometry.svg.right+1)report.failures.push(geometry.kind+': ink extends beyond SVG viewport');
 for(const op of geometry.operators)if(op.y>geometry.operand.y+2||op.bottom<geometry.operand.bottom-2)report.failures.push(geometry.kind+': growing symbol does not cover its own operand');
}
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
(async()=>{try{
 assert(process.env.READMD_BIN);fs.writeFileSync(file,source);process.env.READMD_WORKSPACE=data;report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 native=await startNative(root,data,chromium,await port());page=native.page;page.on('pageerror',e=>report.pageErrors.push(e.message));await page.evaluate(()=>__readmdSessionReady);await page.setViewportSize({width:1160,height:820});await page.evaluate(async f=>{await i18n.setLanguage('zh-CN',false);await loadFile(f);},file);
 const end=Date.now()+45000;while(await page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count()!==2){assert(Date.now()<end,'Real typesetting timed out');await new Promise(r=>setTimeout(r,80));}
 for(let i=0;i<2;i++){
  const e=page.locator('#content .readmd-dsl-math').nth(i);await e.scrollIntoViewIfNeeded();
  assert.equal(await e.locator('[data-readmd-math-renderer="rust"]').count(),1);
  const geometry=await e.evaluate(measure,i);verify(geometry);await page.screenshot({path:path.join(data,geometry.kind+'.png')});
 }
 await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
 for(let i=0;i<2;i++){
  const e=page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(i);await e.waitFor();await e.click();
  const deadline=Date.now()+15000;while(!await page.locator('.formula-inline-controls button').first().isEnabled()){assert(Date.now()<deadline,'Inline editor did not become ready');await new Promise(r=>setTimeout(r,80));}
  assert.equal(await page.locator('.formula-inline-canvas [data-readmd-math-renderer="rust"]').count(),1);
  const geometry=await page.locator('.formula-inline-canvas').evaluate(measure,i);geometry.kind+='-inline';verify(geometry);await page.screenshot({path:path.join(data,geometry.kind+'.png')});await page.keyboard.press('Escape');
 }
 assert.equal(await page.evaluate(()=>getEditContent()),source);assert.equal(fs.readFileSync(file,'utf8'),source);
 assert.deepEqual(report.pageErrors,[]);assert.deepEqual(report.failures,[],'Actual visual geometry failed');report.passed=true;
}catch(e){report.error=String(e.message).replaceAll(root,'<workspace>').slice(0,1600);process.exitCode=1;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();report.scope='Actual Windows preview ink bounds and operand-relative grow. No mock and no visual certification from attribute presence.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,failures:report.failures,report:path.relative(root,path.join(data,'report.json'))}));}
})();
