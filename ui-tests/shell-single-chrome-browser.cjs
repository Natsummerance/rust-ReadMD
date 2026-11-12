'use strict';
// Real bundled UI and authenticated Rust document services. No screenshot artifacts.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {ROOT, openOwned, until, sha}=require('../tools/test/owned-browser-host.cjs');
const arg=name=>{const i=process.argv.indexOf(name);return i>=0?process.argv[i+1]:null;};
const out=arg('--evidence'), binary=arg('--bin'), scenario=arg('--case')||'shell';
assert(out && binary && path.isAbsolute(out) && path.isAbsolute(binary));
const bound=['assets/index.html','assets/readmd.boot.js','assets/css/shell.css','assets/css/editor.css'];
const stamp=()=>Object.fromEntries(bound.map(file=>[file,sha(fs.readFileSync(path.join(ROOT,file)))]));
const before=stamp();
const report={passed:false,scenario,checks:[],nativeUiCertified:false,inferenceRequests:[],pageErrors:[]};
let h,phase='start',files;
const check=(name)=>report.checks.push({name,passed:true});
(async()=>{
 try{
  h=await openOwned(out,(_,ctx)=>{
    files=[path.join(ctx.documents,'工作说明.md'),path.join(ctx.documents,'窄窗标签和持续编辑保存测试说明.md')];
    fs.writeFileSync(files[0],'# 工作说明\r\n\r\n选择文本 ABCDEF GHIJKLM\r\n第二行供拖选定位。\r\n',{flag:'wx'});
    fs.writeFileSync(files[1],'# 窄窗标签\r\n\r\n正文内容\r\n',{flag:'wx'});
    for(let i=2;i<11;i++){const f=path.join(ctx.documents,'回归文档'+i+' 标签空间测试.md');files.push(f);fs.writeFileSync(f,'# 回归文档 '+i+'\n',{flag:'wx'});}
    fs.writeFileSync(path.join(ctx.profile,'settings.json'),JSON.stringify({lang:'zh-CN',autoCheckUpdate:false}),{flag:'wx'});
  },{binary,playwright:arg('--playwright')||undefined});const p=h.page;
  p.on('request',request=>{
    if(request.method()==='POST'&&/^\/api\/ai\/(?:chat|completion)(?:\/|$)/.test(new URL(request.url()).pathname)) report.inferenceRequests.push(new URL(request.url()).pathname);
  });
  const denied=await p.evaluate(async()=> (await fetch('/api/session')).status);
  assert([401,403].includes(denied));check('actual Rust auth and production CSP');
  const geometry=async()=>{
    const m=await p.evaluate(()=>{
      const rect=id=>$(id).getBoundingClientRect();
      const header=rect('app-titlebar'), bar=rect('doc-tabs-bar'), ai=rect('btn-ai'), side=rect('side-rail');
      return {overflow:document.documentElement.scrollWidth-innerWidth,bar:bar.right,ai:ai.left,headBottom:header.bottom,sideTop:side.top,
        railMatches:getComputedStyle($('side-rail')).backgroundColor===getComputedStyle($('app-titlebar')).backgroundColor};
    });
    assert(m.overflow<=1 && m.bar<=m.ai+.5 && m.railMatches);return m;
  };
  const box=async sel=>p.locator(sel).boundingBox();
  const load=async i=>p.evaluate(file=>loadFile(file),files[i]);
  await load(0);await load(1);
  const ids=await p.evaluate(()=>state.tabs.map(t=>t.id));
  if(scenario==='rtl'){
    phase='RTL and native caption projections';
    await p.setViewportSize({width:1024,height:680});
    await p.evaluate(async()=>{await i18n.setLanguage('ar',false);document.body.classList.add('native-macos');});
    assert.equal(await p.locator('html').getAttribute('dir'),'rtl');
    await geometry();check('Arabic sidebar and Mac inset geometry');
    for(const [id,key] of [['btn-toc','shell.sidebar'],['tab-toc','tabs.toc'],['tab-files','sidebar.files']]){
      assert.equal(await p.locator('#'+id).getAttribute('aria-label'),await p.evaluate(k=>i18n.t(k),key));
    }
    check('translated labels stay named');
    await p.locator('#tab-files').click();assert.ok(await p.locator('#status-summary').isVisible());
    await p.locator('#doc-tabs-bar .tab-item.active').focus();await p.keyboard.press('F2');
    const rename=p.locator('#doc-tabs-bar .tab-title-input');await rename.fill('Uncommitted filename');
    const handle=await rename.elementHandle();
    await p.evaluate(()=>i18n.setLanguage('en',false));
    assert.equal(await handle.evaluate(el=>el.isConnected&&el.value==='Uncommitted filename'),true);
    await p.keyboard.press('Escape');await rename.waitFor({state:'hidden'});
    assert.ok(await p.locator('#side').isVisible());check('language switch preserves rename and Escape only cancels the rename');
    await p.evaluate(()=>{document.body.classList.remove('native-macos');document.body.classList.add('custom-titlebar');$('window-controls').classList.remove('hidden');});
    await geometry();assert.ok((await p.locator('#window-controls').boundingBox()).x>(await p.locator('#btn-ai').boundingBox()).x);
    check('Windows/Linux caption placement projection');
    assert.match(fs.readFileSync(files[1],'utf8'),/^# 窄窗标签\r\n/);
    check('cancelled rename preserves file bytes');
    report.passed=true;return;
  }
  if(scenario==='compact'){
    phase='compact responsive controls';
    for(const width of [360,390,412,600,640,641,1024,1160]){
      await p.setViewportSize({width,height:740});
      await geometry();
      await p.locator('#btn-toc').click();await p.locator('#side-files').waitFor();
      assert.ok(await p.locator('#btn-open').isVisible());
      assert.ok(await p.locator('#btn-ai').isVisible());
      for(const id of ['btn-print','btn-palette','btn-theme','btn-home']){
        const b=await p.locator('#'+id).boundingBox();assert(b.width>=44&&b.height>=44);
      }
      await p.locator('#btn-toc').click();assert.equal(await p.locator('#side').isVisible(),false);
      check('fixed rail and reachable commands at '+width);
    }
    await p.locator('#btn-palette').click();await p.locator('#command-palette-modal').waitFor();
    await until(()=>p.locator('#command-palette-modal').evaluate(el=>el.contains(document.activeElement)),'palette focus acquired');
    for(const width of [390,641,1160]){
      await p.setViewportSize({width,height:740});await geometry();
      assert.ok(await p.locator('#command-palette-modal').evaluate(el=>el.contains(document.activeElement)));
    }
    await p.keyboard.press('Escape');await p.locator('#command-palette-modal').waitFor({state:'hidden'});
    assert.equal(await p.locator('#btn-palette').evaluate(el=>el===document.activeElement),true);
    check('command focus stays owned through resize and restores to the rail');
    report.passed=true;return;
  }
  phase='titlebar and rail';assert.deepEqual(await p.locator('#app-titlebar button:visible').evaluateAll(els=>els.filter(e=>e.id).map(e=>e.id)),['btn-toc','btn-edit','btn-zen','doc-tabs-new','btn-ai']);
  assert.equal(await p.locator('#app-titlebar img').count(),0);
  assert.equal(await p.locator('#window-title').isVisible(), false);
  assert.equal(await p.locator('#doc-tabs-bar').getAttribute('aria-orientation'),'horizontal');
  assert.equal(await p.locator('#side-shortcuts > :last-child').getAttribute('id'),'btn-home');
  assert.ok(await p.evaluate(()=>getComputedStyle($('app-titlebar')).backgroundColor===getComputedStyle($('side-rail')).backgroundColor));
  assert.equal(await p.locator('#statusbar').isVisible(),false);
  await p.locator('#btn-toc').click();await p.locator('#side-files').waitFor();
  await until(()=>p.locator('#status-words').textContent().then(Boolean),'document statistics');
  assert.equal(await p.locator('#status-format').textContent(),'MD');
  assert.match(await p.locator('#status-encoding').textContent(),/UTF-8/i);
  assert.doesNotMatch(await p.locator('#status-summary').textContent(),/\b(?:LF|CRLF)\b/);
  await p.locator('#tab-toc').click();await p.locator('#side-toc').waitFor();
  await p.locator('#btn-recent').click();await until(()=>p.locator('#side-recent-list button').count().then(n=>n>=2),'real recent files');
  await p.locator('#tab-conversion').click();await p.locator('#side-conversion').waitFor();
  assert.ok(await p.locator('#btn-saveas').isVisible());assert.ok(await p.locator('#btn-print').isVisible());
  await p.locator('#btn-toc').click();assert.equal(await p.locator('#side').isVisible(),false);
  await p.locator('#btn-toc').click();assert.ok(await p.locator('#status-summary').isVisible());
  await p.locator('#side-conversion').waitFor();
  assert.equal(await p.locator('#side').getAttribute('data-view'),'conversion');
  check('top tabs, sidebar view restoration and automatic details');
  phase='font and hover';
  await p.locator('#rd-prefs-btn').click();await p.locator('#rd-prefs').waitFor();
  const pref=await box('#rd-prefs');assert(pref.y>=0&&pref.y+pref.height<=820);
  await p.keyboard.press('Escape');
  await p.locator('#btn-print').hover();await p.locator('#rm-tooltip.is-visible').waitFor();
  assert.match(await p.locator('#rm-tooltip').textContent(),/导出/);
  await p.locator('#btn-print').click();await p.locator('#export-modal').waitFor();await p.locator('#export-close').click();
  check('font popover fits, print glyph tooltip opens actual export');
  phase='search and mode';
  await load(0);await p.keyboard.press('Control+f');
  assert.equal(await p.locator('#side').getAttribute('data-view'),'search');assert.ok(await p.locator('#side-search #search-input').isVisible());
  await p.locator('#search-input').fill('ABCDEF');await p.locator('#search-input').press('Enter');
  await until(()=>p.locator('#search-count').textContent().then(s=>s.includes('1/1')),'document search matched');
  await p.keyboard.press('Escape');assert.equal(await p.locator('#side').isVisible(),false);
  await p.keyboard.press('Control+Shift+F');assert.equal(await p.locator('#side').getAttribute('data-view'),'toc');
  check('sidebar find and separate outline shortcut');
  phase='edit and preview';
  const sideBefore = await box('#side'), railBefore = await box('#side-rail');
  await p.locator('#btn-edit').click();await until(()=>p.evaluate(()=>state.editing&&!!cmView),'editor ready');
  const sideAfter = await box('#side'), railAfter = await box('#side-rail'), editorBar = await box('#edit-bar');
  assert.deepEqual(sideAfter, sideBefore);
  assert.equal(railAfter.y, railBefore.y);
  assert.equal(railAfter.height, railBefore.height);
  assert.ok(editorBar.x >= sideAfter.x + sideAfter.width - 1);
  assert.equal(await p.locator('#status-summary').evaluate(el => getComputedStyle(el).whiteSpace), 'nowrap');
  assert.equal(await p.locator('#status-mods').count(), 0);
  assert.equal(await p.locator('#edit-bar > :last-child').getAttribute('id'),'edit-save');
  assert.equal((await p.locator('#edit-save').textContent()).trim(),'保存');
  await p.locator('#edit-view-trigger').click();assert.equal(await p.locator('#edit-view-menu .pv-btn').count(),5);
  await p.locator('#edit-view-menu [data-pv="right"]').click();await p.locator('#preview-wrap').waitFor();
  assert.equal(await p.locator('[data-pv="right"]').getAttribute('aria-pressed'),'true');
  await p.locator('#edit-view-trigger').click();await p.locator('[data-pv="none"]').click();
  assert.equal(await p.locator('#preview-wrap').isVisible(),false);
  const row=await box('#edit-bar'),save=await box('#edit-save');assert(save.y>=row.y&&save.y+save.height<=row.y+row.height+1);
  check('one edit row and existing preview directions');
  phase='editor selection';
  const range=await p.evaluate(()=>{
    const p=cmView.state.doc.toString().indexOf('ABCDEF');
    return [cmView.coordsAtPos(p),cmView.coordsAtPos(p+12),cmView.coordsAtPos(p+1),cmView.coordsAtPos(p+2)];
  });
  await p.mouse.move(range[0].left,range[0].top+6);await p.mouse.down();await p.mouse.move(range[1].left,range[1].top+6,{steps:8});
  await new Promise(r=>setTimeout(r,155));assert.equal(await p.locator('#cm-selection-toolbar').isVisible(),false);
  await p.mouse.up();assert.equal(await p.locator('#cm-selection-toolbar').isVisible(),false);
  await p.locator('#cm-selection-toolbar').waitFor();
  const selected=await p.evaluate(()=>cmView.state.sliceDoc(cmView.state.selection.main.from,cmView.state.selection.main.to));
  assert.ok(selected.includes('ABCDEF'),JSON.stringify(selected));
  const mid=(range[2].left+range[3].left)/2,y=range[2].top+6;
  await p.mouse.move(mid-1,y);await p.mouse.down();await p.mouse.move(mid+1,y,{steps:2});await p.mouse.up();
  await new Promise(r=>setTimeout(r,230));
  assert.equal(await p.locator('#cm-selection-toolbar').isVisible(),false);
  await p.keyboard.press('Shift+ArrowRight');await p.locator('#cm-selection-toolbar').waitFor();
  check('drag does not raise toolbar early, tiny movement suppressed, keyboard selection works');
  phase='save';
  await p.evaluate(()=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n快捷保存\n'}}));
  await p.keyboard.press('Control+s');
  await until(()=>p.evaluate(()=>state.editing&&!getActiveTab().isDirty),'keyboard save stays');
  assert.match(fs.readFileSync(files[0],'utf8'),/快捷保存\r\n/);
  await p.evaluate(()=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'保存后退出\n'}}));
  await p.locator('#edit-save').click();
  await until(()=>p.evaluate(()=>!state.editing&&!getActiveTab().isDirty),'button save exits');
  assert.match(fs.readFileSync(files[0],'utf8'),/保存后退出\r\n/);
  check('real CRLF save; shortcut stays; button returns to reader');
  await p.emulateMedia({media:'print'});
  assert.ok(await p.locator('#content').isVisible());
  assert.equal(await p.locator('#app-titlebar').isVisible(), false);
  assert.equal(await p.locator('#side-rail').isVisible(), false);
  await p.emulateMedia({media:null});
  check('reading document still prints through the document column');
  phase='overflow and update';
  await p.setViewportSize({width:1024,height:680});
  for(let i=2;i<files.length;i++)await load(i);
  await p.locator('#doc-tabs-overflow-btn').waitFor();await p.locator('#doc-tabs-overflow-btn').click();
  await until(()=>p.locator('#doc-tabs-dropdown [role="menuitem"]').count().then(n=>n===11),'all eleven tabs in overflow');
  await p.locator('#doc-tabs-dropdown [role="menuitem"]').filter({hasText:'工作说明.md'}).click();
  await until(()=>p.evaluate(id=>state.activeTabId===id,ids[0]),'overflow opens real document');
  assert.equal(await p.locator('#doc-tabs-overflow-btn').getAttribute('aria-expanded'),'false');
  await p.route('**/api/update/check',rt=>rt.fulfill({json:{ok:true,has_update:true,current_version:'0.0.6',latest_version:'0.0.7',html_url:'https://example.invalid/'}}));
  await p.evaluate(()=>checkUpdate(true));await p.locator('#status-update-badge').waitFor();
  assert.equal(await p.locator('#side-shortcuts > :first-child').getAttribute('id'),'status-update-badge');
  assert.equal(await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true);
  check('narrow tab overflow, update above shortcuts');
  phase='native-independent state';
  await p.locator('#btn-zen').click();
  assert.ok(await p.evaluate(()=>document.body.classList.contains('zen-mode')));
  assert.equal(await p.locator('#side-rail').isVisible(),false);
  await p.keyboard.press('Escape');
  assert.ok(await p.locator('#side-rail').isVisible());
  check('Zen releases rail');
  phase='dirty close, durable reopen and tab order';
  await p.locator('#btn-edit').click();
  await p.evaluate(()=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n关闭保护\n'}}));
  const active=p.locator('#doc-tabs-bar .tab-item.active');
  await active.locator('.tab-close').click();await p.locator('#close-confirm-modal').waitFor();
  await p.locator('#close-confirm-cancel').click();
  assert.ok(await p.evaluate(()=>state.editing&&getEditContent().includes('关闭保护')));
  await active.locator('.tab-close').click();await p.locator('#close-confirm-save').click();
  await until(()=>p.evaluate(id=>!state.tabs.some(t=>t.id===id),ids[0]),'save and close');
  await load(0);assert.ok(await p.evaluate(()=>state.original.includes('关闭保护')));
  assert.match(fs.readFileSync(files[0],'utf8'),/关闭保护\r\n/);
  const identity=await p.evaluate(()=>getActiveTab().id);
  await active.focus();await p.keyboard.press('Alt+ArrowLeft');
  assert.ok(await p.evaluate(id=>state.tabs.findIndex(t=>t.id===id)<state.tabs.length-1,identity));
  check('dirty tab cancel, save-close, file reopen and horizontal reorder');
  report.passed=true;
 }catch(e){report.failure={phase,message:e.message};console.error('Phase:',phase,e.message);throw e;}
 finally{
   if(h){report.pageErrors=h.errors;await h.close();}
   report.runtimeSourceUnchanged=JSON.stringify(stamp())===JSON.stringify(before);
   if(!report.runtimeSourceUnchanged||report.pageErrors.length||report.inferenceRequests.length)report.passed=false;
   if(fs.existsSync(out))fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');
   if(report.passed)console.log(scenario+': '+report.checks.length+' checks');
   else process.exitCode=1;
 }
})().catch(e=>{console.error(e);process.exitCode=1});
