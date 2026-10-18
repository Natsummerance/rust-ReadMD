'use strict';
// Real packaged desktop API -> EPUB -> exact exported XHTML/CSS/font in Edge.
// Browser MathML inspection is distinct from independent EPUB-reader acceptance.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),http=require('node:http'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/mathml-epub-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,passed:false,checks:[],geometry:[],pageErrors:[],epubReader:'unverified',wordRoundTrip:'unverified'};
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');let native,browser,page,server,stage='startup';
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});}
function near(value,expected){assert(Math.abs(value-expected)<0.025,`${value} != ${expected}`);}
(async()=>{try{
 assert(process.env.READMD_BIN,'Explicit tested binary required');process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');
 report.binarySha256=sha(fs.readFileSync(process.env.READMD_BIN));report.bootSha256=sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js')));report.scriptSha256=sha(fs.readFileSync(__filename));
 native=await startNative(root,data,chromium,await port());page=native.page;page.on('pageerror',e=>report.pageErrors.push(e.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));
 const run=(text,size=32,color='336699')=>`<m:r>${size!==null||color!==null?'<w:rPr>'+ (size===null?'':`<w:sz w:val="${size}"/>`)+(color===null?'':`<w:color w:val="${color}"/>`)+'</w:rPr>':''}<m:t>${text}</m:t></m:r>`;
 const control=kind=>`<m:${kind}Pr><m:ctrlPr><w:rPr><w:sz w:val="48"/><w:color w:val="CC3366"/></w:rPr></m:ctrlPr></m:${kind}Pr>`;
 const frac=`<m:f>${control('f')}<m:num>${run('a')}</m:num><m:den>${run('b',32,'auto')}</m:den></m:f>`;
 const xml=body=>`<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">${body}</m:oMath>`;
 const cases=[
  {name:'Display fraction',display:true,body:frac},
  {name:'Inline fraction',display:false,body:frac},
  {name:'Root and script',display:true,body:`<m:rad>${control('rad')}<m:deg>${run('c',32,'auto')}</m:deg><m:e>${run('d')}</m:e></m:rad><m:sSup><m:e>${run('x')}</m:e><m:sup>${run('y',32,'auto')}</m:sup></m:sSup>`},
  {name:'Nested inline fraction',display:false,body:`<m:f><m:num>${frac}</m:num><m:den>${run('n')}</m:den></m:f>`},
  {name:'Document defaults',display:true,body:run('u',null,'CC3366')+run('v',null,null)},
 ];
 stage='native MathML compile and readable round-trip';let content='# Office Math\n\n';const previews=new Map();
 for(const c of cases){
  const source=xml(c.body),parsed=await api('/api/math/parse',{source,dialect:'omml',display:c.display,font_size:18});assert.equal(parsed.data.ok,true,JSON.stringify(parsed.data));
  const dsl=await api('/api/math/serialize',{source,dialect:'omml',display:c.display,target:'dsl'});assert.equal(dsl.data.ok,true);c.dsl=dsl.data.output;
  const again=await api('/api/math/parse',{source:c.dsl,dialect:'dsl',display:c.display,font_size:18});assert.equal(again.data.ok,true);assert.deepEqual(Object.values(again.data.projections),Object.values(parsed.data.projections));
  // Source-node paths legitimately change when the DSL adds its document root;
  // compare semantic MathML elements and all rendering attributes, not paths.
  const semantic=await page.evaluate(previews=>previews.map(s=>[...new DOMParser().parseFromString(s,'application/xml').querySelectorAll('*')].filter(n=>n.localName!=='mrow').map(n=>({tag:n.localName,attributes:[...n.attributes].filter(a=>!['id','alttext'].includes(a.name)).map(a=>[a.name,a.value]),text:n.children.length?'':n.textContent}))),[parsed.data.preview,again.data.preview]);assert.deepEqual(semantic[1],semantic[0]);
  previews.set(c.name,parsed.data.preview);assert(parsed.data.preview.includes('mathsize='));
  content+='## '+c.name+'\n\n'+(c.display?'```readmd-math v1\n'+c.dsl+'\n```':':math['+c.dsl+']')+'\n\n';
 }
 fs.writeFileSync(path.join(docs,'Office-math.md'),content);for(const [name,preview]of previews)fs.writeFileSync(path.join(data,name.replaceAll(' ','-')+'.mathml'),preview);
 report.checks.push('The packaged desktop parses actual Office structures and serializes/reopens readable DSL without changing their projections; API MathML uses the requested 18pt ambient context.');
 stage='actual EPUB output';const books=new Map();report.outputs={};
 for(const [name,source]of [['office',content],['plain','# A plain book\n\nText without formulas.'],['latex','# Standard math\n\n$$\nx^2 + \\alpha\n$$']]){
  const out_path=path.join(docs,name+'.epub'),r=await api('/api/export',{format:'epub',content:source,baseDir:docs,out_path,options:{typography:{size:18,color:'#123456'},epub:{fontSize:18},cover:{enabled:false},toc:{enabled:false}}});assert.equal(r.data.ok,true,JSON.stringify(r.data));
  const bytes=fs.readFileSync(out_path),parts=zipParts(bytes,null);books.set(name,parts);report.outputs[name]={path:path.relative(data,out_path),sha256:sha(bytes),bytes:bytes.length};
  const math=name!=='plain';assert.equal(parts.has('OEBPS/fonts/STIXTwoMath-Regular.ttf'),math);assert.equal(parts.has('OEBPS/fonts/STIX-OFL-1.1.txt'),math);
  if(math){assert(parts.get('OEBPS/fonts/STIXTwoMath-Regular.ttf').equals(fs.readFileSync(path.join(root,'assets/fonts/STIXTwoMath-Regular.ttf'))));assert(parts.get('OEBPS/fonts/STIX-OFL-1.1.txt').equals(fs.readFileSync(path.join(root,'assets/licenses/STIX-OFL-1.1.txt'))));}
 }
 assert.equal(fs.readFileSync(path.join(docs,'Office-math.md'),'utf8'),content);assert(!fs.existsSync(path.join(docs,'readmd-assets')));
 report.checks.push('Real Office, ordinary LaTeX and plain EPUBs export without modifying their source or creating assets. Only math books include the exact bundled STIX font and its full existing license.');
 // Serve only parts already produced by the real exporter. No HTML rewriting,
 // injected font or app mocks; the font must come from the actual EPUB archive.
 const parts=books.get('office'),chapter=[...parts.keys()].find(p=>/^OEBPS\/chapter_\d+\.xhtml$/.test(p)&&parts.get(p).toString().includes('Display fraction'));assert(chapter);
 const requests=[],mime={'.xhtml':'application/xhtml+xml','.css':'text/css','.ttf':'font/ttf','.txt':'text/plain'};
 server=http.createServer((req,res)=>{let key;try{key=decodeURIComponent(new URL(req.url,'http://127.0.0.1').pathname).slice(1);}catch{res.writeHead(400);return res.end();}const bytes=parts.get(key);if(!bytes){res.writeHead(404);return res.end();}requests.push(key);res.writeHead(200,{'Content-Type':mime[path.extname(key)]||'application/octet-stream','Content-Security-Policy':"default-src 'none'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; base-uri 'none'"});res.end(bytes);});
 await new Promise(r=>server.listen(0,'127.0.0.1',r));browser=await chromium.launch({channel:'msedge',headless:true});const artifact=await browser.newPage({viewport:{width:1160,height:820},bypassCSP:false});artifact.on('pageerror',e=>report.pageErrors.push(e.message));
 stage='actual exported MathML in Edge';const response=await artifact.goto('http://127.0.0.1:'+server.address().port+'/'+chapter);assert.equal(response.status(),200);await artifact.evaluate(()=>document.fonts.ready);assert.equal(await artifact.locator('math').count(),5);assert(requests.includes('OEBPS/fonts/STIXTwoMath-Regular.ttf'));
 const geometry=await artifact.locator('math').evaluateAll(elements=>elements.map(e=>({display:e.getAttribute('display'),controls:[...e.querySelectorAll('mfrac,mroot,msup')].map(n=>({tag:n.tagName,color:getComputedStyle(n).color,size:parseFloat(getComputedStyle(n).fontSize)})),tokens:[...e.querySelectorAll('mi,mn,mo,mtext')].map(n=>{const s=getComputedStyle(n),r=n.getBoundingClientRect();return{text:n.textContent,size:parseFloat(s.fontSize),color:s.color,font:s.fontFamily,width:r.width,height:r.height};})})));
 report.geometry=geometry;const find=(i,text)=>{const t=geometry[i].tokens.find(t=>t.text===text);assert(t,'Missing token '+text);assert(t.width>0&&t.height>0);assert(t.font.includes('ReadMD Math'));return t;};
 for(const [i,points]of [[0,16],[1,11.2],[3,8.8]]){near(find(i,'a').size,points*4/3);near(find(i,'b').size,points*4/3);assert.equal(find(i,'a').color,'rgb(51, 102, 153)');assert.equal(find(i,'b').color,'rgb(18, 52, 86)');const f=geometry[i].controls.find(n=>n.tag==='mfrac'&&n.color==='rgb(204, 51, 102)');assert(f);near(f.size,(i===3?16.8:24)*4/3);}
 near(find(2,'c').size,8.8*4/3);near(find(2,'d').size,16*4/3);near(find(2,'y').size,11.2*4/3);assert.equal(find(2,'c').color,'rgb(18, 52, 86)');assert.equal(geometry[2].controls.find(n=>n.tag==='mroot').color,'rgb(204, 51, 102)');
 near(find(4,'u').size,24);near(find(4,'v').size,24);assert.equal(find(4,'v').color,'rgb(18, 52, 86)');report.checks.push('Exact exported XHTML renders red 24pt controls independently of blue 16pt operands and profile-default auto color; inline/nested/script/root-degree sizes follow the bundled 70%/55% MATH constants, verified in actual computed geometry.');
 await artifact.locator('math').nth(1).scrollIntoViewIfNeeded();await artifact.screenshot({path:path.join(data,'epub-mathml-1160.png')});await artifact.setViewportSize({width:1024,height:680});await artifact.locator('math').nth(3).scrollIntoViewIfNeeded();await artifact.screenshot({path:path.join(data,'epub-mathml-1024.png')});
 report.fontRequests=requests.filter(p=>p.endsWith('.ttf'));assert.deepEqual(report.pageErrors,[]);report.checks.push('Actual EPUB output is inspected at 1160x820 and 1024x680 without CSP bypass, external font requests, page script errors or private document inputs. This is browser rendering, not independent EPUB-reader certification.');report.passed=true;
}catch(e){report.failedStage=stage;report.error=String(e.stack||e.message).replaceAll(root,'<workspace>').slice(0,2200);process.exitCode=1;}
finally{if(browser)await browser.close().catch(()=>{});if(server)await new Promise(r=>server.close(r));if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();report.scope='Actual Windows desktop API and self-contained EPUB MathML structure/run format, physical script sizes and profile defaults. Full Office fonts/themes/semantics, independent reader, actual Word and complete B3 remain separate unverified gates.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
