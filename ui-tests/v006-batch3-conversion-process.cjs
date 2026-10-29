'use strict';
// Owned synthetic documents, actual Rust service and production-CSP frontend.
// The installed browser is headless: no desktop automation or personal files.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const {chromium} = require('playwright');
const {zip} = require('./zip-fixtures.cjs');
const root = path.resolve(__dirname, '..');
const base = path.resolve(process.env.READMD_BATCH3_OUTPUT || path.join(root, '.cache/capability-upgrade/v006-batch3-20261027'));
assert.ok(base.startsWith(path.join(root, '.cache') + path.sep));
assert.ok(process.env.READMD_BIN, 'READMD_BIN must identify a built candidate');
fs.mkdirSync(base, {recursive: true});
const run = fs.mkdtempSync(path.join(base, 'conversion-'));
const docs = path.join(run, 'documents'), profile = path.join(run, 'profile');
fs.mkdirSync(docs); fs.mkdirSync(profile);
const original = path.join(docs, 'Report.docx'), existing = path.join(docs, 'Report.md');
const image = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/a1sAAAAASUVORK5CYII=', 'base64');
const document = `<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body><w:p><w:r><w:t>Research report with an image.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Research diagram" descr="Diagram"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="diagram.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="image1"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p><w:sectPr/></w:body></w:document>`;
const rels = `<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="image1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>`;
const types = `<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`;
const packageRels = `<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`;
fs.writeFileSync(original, zip([['[Content_Types].xml', types], ['_rels/.rels', packageRels], ['word/document.xml', document], ['word/_rels/document.xml.rels', rels], ['word/media/image1.png', image]]));
fs.writeFileSync(existing, '# Existing independent document\n\nKeep all original bytes.\n');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const originalHash = hash(fs.readFileSync(original)), existingHash = hash(fs.readFileSync(existing));
const report = {passed:false, synthetic:true, platform:process.platform, surface:'headless Edge and actual local kernel',
  binarySha256:hash(fs.readFileSync(path.resolve(process.env.READMD_BIN))),
  bootSha256:hash(fs.readFileSync(path.join(root, 'assets/readmd.boot.js'))), checks:[]};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function port() { const server = net.createServer(); await new Promise(resolve => server.listen(0, '127.0.0.1', resolve)); const result = server.address().port; await new Promise(resolve => server.close(resolve)); return result; }
async function main() {
  let child, failedChild, browser, page, phase='startup';
  function unchanged() {
    assert.equal(hash(fs.readFileSync(original)), originalHash);
    assert.equal(hash(fs.readFileSync(existing)), existingHash);
    assert.deepEqual(fs.readdirSync(docs).sort(), ['Report.docx', 'Report.md']);
  }
  try {
    const p = await port(), origin = 'http://127.0.0.1:' + p;
    child = spawn(path.resolve(process.env.READMD_BIN), ['--no-window','--port',String(p),'--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')], {cwd:root,windowsHide:true,stdio:'ignore',env:{...process.env,READMD_DATA_DIR:profile}});
    const deadline = Date.now() + 30000;
    for (;;) { if (child.exitCode !== null) throw Error('kernel exited during startup'); try { if ((await fetch(origin)).ok) break; } catch (_) {} if (Date.now() > deadline) throw Error('startup deadline'); await sleep(50); }
    browser = await chromium.launch({channel:'msedge', headless:true});
    const context = await browser.newContext({viewport:{width:1160,height:820},bypassCSP:false});
    page = await context.newPage();
    await page.goto(origin);
    while (!await page.evaluate(() => Boolean(window.__readmdAppReady))) { if (Date.now() > deadline) throw Error('frontend readiness deadline'); await sleep(50); }
    const get = route => page.evaluate(async route => { const r = await apiFetch(route); return {status:r.status,data:await r.json()}; }, route);
    const post = (route, body) => page.evaluate(async ({route,body}) => { const r = await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}); return {status:r.status,data:await r.json()}; }, {route,body});
    if (process.env.READMD_IMPORT_FIRST === '1') {
      phase='legacy-path-import-first';
      await post('/api/import/process',{paths:[original]});
      unchanged();
    }
    phase='direct-file-preview';
    const opened = await get('/api/file?p=' + encodeURIComponent(original));
    assert.equal(opened.status,200); assert.equal(opened.data.converted,true);
    assert.match(opened.data.content,/Research report/); assert.match(opened.data.content,/!\[/);
    unchanged();
    assert.equal(opened.data.assets.length,1); assert.deepEqual(fs.readFileSync(opened.data.assets[0].path),image);
    report.checks.push('Direct file conversion stages the image without changing or creating source-directory files');
    phase='legacy-path-import';
    const imported = await post('/api/import/process',{paths:[original]});
    assert.equal(imported.status,200); assert.equal(imported.data.ok,true); assert.equal(imported.data.pending,1);
    assert.equal(imported.data.imported[0].saved,false); assert.equal(imported.data.imported[0].path,null);
    assert.match(imported.data.imported[0].content,/Research report/); assert.equal(imported.data.imported[0].assets.length,1);
    unchanged();
    report.checks.push('Path-based import returns an unsaved preview and preserves an existing Markdown file');
    phase='repeat-and-text';
    const repeated = await get('/api/file?p=' + encodeURIComponent(original)); assert.equal(repeated.data.assets[0].path,opened.data.assets[0].path);
    const plain = await post('/api/import/process',{paths:[existing]}); assert.equal(plain.data.imported[0].path,existing); assert.equal(plain.data.pending,0); unchanged();
    report.checks.push('Repeated preview reuses staged bytes; existing text-path import remains readable');
    phase='frontend-open';
    await page.evaluate(file => loadFile(file), original);
    const tab = await page.evaluate(() => { const t=getActiveTab(); return {path:t?.path,source:t?.source,isVirtual:t?.isVirtual,assets:t?.webAssets,content:t?.content}; });
    assert.equal(tab.path,null); assert.equal(tab.source,'convert'); assert.equal(tab.isVirtual,true); assert.equal(tab.assets.length,1);
    assert.match(tab.content,/Research report/);
    const rendered = page.locator('#content img').first(); await rendered.waitFor({state:'visible'});
    const imageDeadline = Date.now() + 30000;
    while (!await rendered.evaluate(image => image.complete && image.naturalWidth > 0)) {
      if (Date.now() > imageDeadline) throw Error('staged image did not decode');
      await sleep(50);
    }
    await page.locator('#btn-edit').click();
    const editDeadline = Date.now() + 30000;
    while (!await page.evaluate(()=>state.editing && Boolean(window.cmView))) {
      if (Date.now() > editDeadline) throw Error('editor readiness deadline');
      await sleep(50);
    }
    assert.equal(await page.evaluate(()=>state.editing),true);
    assert.equal(await page.evaluate(()=>getActiveTab().path),null);
    unchanged();
    report.checks.push('The frontend opens the conversion as an unsaved Markdown document with its staged image');
    phase='explicit-save-and-reopen';
    const destination = path.join(docs,'Saved.md'); fs.writeFileSync(destination,'');
    const blank = await get('/api/file?p='+encodeURIComponent(destination));
    const saved = await post('/api/save',{path:destination,content:tab.content,assets:tab.assets,encoding:'utf-8',expected_revision:blank.data.revision});
    assert.equal(saved.status,200); assert.equal(saved.data.ok,true);
    const contents = fs.readFileSync(destination,'utf8'); assert.match(contents,/Research report/); assert.match(contents,/readmd-assets\//); assert.ok(!contents.includes('/raw?p='));
    const asset = path.join(docs,...saved.data.saved_assets[0].reference.split('/').map(decodeURIComponent)); assert.deepEqual(fs.readFileSync(asset),image);
    const reopened = await get('/api/file?p='+encodeURIComponent(destination)); assert.equal(reopened.data.content,contents);
    assert.equal(hash(fs.readFileSync(original)),originalHash); assert.equal(hash(fs.readFileSync(existing)),existingHash);
    report.checks.push('Explicit save materializes the referenced image and the saved Markdown reopens intact');
    phase='staging-failure';
    const failedProfile=path.join(run,'failed-profile'); fs.mkdirSync(failedProfile);
    fs.writeFileSync(path.join(failedProfile,'resource-staging'),'occupied synthetic file');
    const failedPort=await port(), failedOrigin='http://127.0.0.1:'+failedPort;
    failedChild=spawn(path.resolve(process.env.READMD_BIN),['--no-window','--port',String(failedPort),'--data-dir',failedProfile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:root,windowsHide:true,stdio:'ignore',env:{...process.env,READMD_DATA_DIR:failedProfile}});
    const failedDeadline=Date.now()+30000;
    for(;;){if(failedChild.exitCode!==null)throw Error('failure fixture kernel exited');try{if((await fetch(failedOrigin)).ok)break;}catch(_){}if(Date.now()>failedDeadline)throw Error('failure fixture startup deadline');await sleep(50);}
    const failedContext=await browser.newContext({viewport:{width:1160,height:820},bypassCSP:false});
    const failedPage=await failedContext.newPage();await failedPage.goto(failedOrigin);
    while(!await failedPage.evaluate(()=>Boolean(window.__readmdAppReady))){if(Date.now()>failedDeadline)throw Error('failure fixture frontend deadline');await sleep(50);}
    const failed = await failedPage.evaluate(async file=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(file));return r.json();},original);
    assert.equal(failed.conversion_error,'asset_staging_failed'); assert.notEqual(failed.converted,true);
    await failedPage.evaluate(file=>loadFile(file),original);
    assert.equal(await failedPage.evaluate(()=>state.tabs.length),0);
    assert.equal(await failedPage.evaluate(()=>document.body.innerText.includes(i18n.t('toast.convertFailed'))),true);
    assert.equal(hash(fs.readFileSync(original)),originalHash); assert.equal(hash(fs.readFileSync(existing)),existingHash);
    report.checks.push('A blocked staging directory reports failure without opening a partial document or changing sources');
    await failedContext.close();
    report.passed=true;
  } catch (error) { report.phase=phase; report.error=String(error.message).replaceAll(root,'<workspace>').replaceAll(run,'<fixture>').slice(0,500); process.exitCode=1; }
  finally { if(browser)await browser.close().catch(()=>{}); for(const owned of [failedChild,child])if(owned&&owned.exitCode===null){const ended=new Promise(resolve=>owned.once('exit',resolve));owned.kill();await Promise.race([ended,sleep(3000)]);} fs.writeFileSync(path.join(run,'report.json'),JSON.stringify(report,null,2)); console.log(JSON.stringify({passed:report.passed,phase:report.phase,checks:report.checks.length,report:path.relative(root,path.join(run,'report.json'))})); }
}
main().catch(()=>{console.error('Conversion process validation failed');process.exitCode=1;});
