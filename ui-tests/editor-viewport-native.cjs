'use strict';
// Real WebView2/CSP regression. Only explicit binaries, owned profiles and
// synthetic Markdown are used; no browser/runtime download or Cargo invocation.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), net = require('node:net');
const { chromium } = require('playwright'), { startNative } = require('./native-session.cjs');
const root = path.resolve(__dirname, '..');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const sizes = [[3840,2160,1],[2560,1440,1.5],[1920,1080,2],[1160,820,1],[1024,680,1]];
const intro = '# Viewport evidence\n\nA short synthetic introduction. 窗口布局检查。\n\n';
const table = '| Observation | Context | Sample | Duration | Method | Confidence | Limitation | Next step |\n|---|---|---|---|---|---|---|---|\n' +
  Array.from({length:8}, (_, n) => `| Evidence ${n+1} | Workshop | Group ${n+1} | 12 weeks | Recorded observation | Moderate | Synthetic fixture only | Compare another sample |`).join('\n');
const code = '```javascript\nconst observation = { label: "synthetic viewport evidence", context: "desktop reading and editing", confidence: "bounded", decision: "compare independent observations" };\n```';
const imagery='\n\n![Synthetic wide image](Viewport-wide.svg)\n\nInline ![Inline symbol](Viewport-icon.svg) stays within its sentence.\n';
const body = intro + table + '\n\n' + code + '\n\n' + Array.from({length:90}, (_, n) =>
  `## Section ${n+1}\n\nEvidence paragraph ${n+1}. Compare the observation with its context before drawing a conclusion. 保留观察、语境与边界。\n\n- Recorded observation\n- Alternative explanation\n`).join('\n')+imagery;

async function frames(page, count = 2) {
  await page.evaluate(async count => { while (count-- > 0) await new Promise(requestAnimationFrame); }, count);
}
async function until(fn, timeout = 30000) {
  const end = Date.now() + timeout;
  while (!await fn()) { if (Date.now() > end) throw Error('Native condition timed out'); await new Promise(r => setTimeout(r, 50)); }
}
async function metrics(page) {
  return page.evaluate(() => {
    const rect = el => { const r=el?.getBoundingClientRect(); return r ? {x:r.x,y:r.y,width:r.width,height:r.height} : null; };
    const view = window.cmView, scroller = view?.scrollDOM, sr = scroller?.getBoundingClientRect();
    const visible = scroller ? [...view.contentDOM.querySelectorAll('.cm-line')].filter(el => {
      const r=el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.bottom > sr.top && r.top < sr.bottom && el.textContent.trim();
    }) : [];
    const article = document.querySelector('#content .rd-article') || document.querySelector('#content .markdown-body');
    const para = article?.querySelector('p'), preview = document.getElementById('preview-pane');
    const textWidth = el => { if (!el) return null; const s=getComputedStyle(el); return el.getBoundingClientRect().width-parseFloat(s.paddingLeft)-parseFloat(s.paddingRight); };
    // Observe the painted DOM without forcing a CodeMirror measurement here.
    // A geometry flush during sampling can replace the nodes being inspected.
    const top = view?.viewportLineBlocks.find(block=>block.bottom>scroller.scrollTop+8)?.from;
    const first = visible[0], firstPosition = first ? view.posAtDOM(first,0) : null;
    return { viewport:{width:innerWidth,height:innerHeight,dpr:devicePixelRatio}, editing:state.editing, layout:state.pvLayout,
      outerOverflow:document.documentElement.scrollWidth > innerWidth+1, source:rect(scroller), sourceHost:rect(document.getElementById('edit-cm')),
      sourceOuterTop:document.getElementById('edit-cm').scrollTop, visibleSourceLines:visible.length, inView:view?.inView,
      sourceTop:scroller?.scrollTop, sourceTopLine:view&&top!==undefined?view.state.doc.lineAt(top).number:null,
      sourceFirstVisible:first?{line:view.state.doc.lineAt(firstPosition).number,offset:first.getBoundingClientRect().top-sr.top}:null,
      selection:view?.state.selection.toJSON(), documentLength:view?.state.doc.length,
      cmGeometry:view?{scaleX:view.scaleX,scaleY:view.scaleY,lineHeight:view.defaultLineHeight,charWidth:view.defaultCharacterWidth,viewport:view.viewport,offsetWidth:scroller.offsetWidth,offsetHeight:scroller.offsetHeight}:null,
      sourceMotion:view?{reduced:matchMedia('(prefers-reduced-motion: reduce)').matches,
        spacers:[...view.contentDOM.querySelectorAll('.cm-gap')].map(el=>({height:parseFloat(el.style.height),actual:el.getBoundingClientRect().height/view.scaleY,
          transition:getComputedStyle(el).transitionProperty,animated:el.getAnimations().some(animation=>animation.effect.getKeyframes().some(frame=>'height'in frame))}))}:null,
      preview:rect(preview), previewWrap:rect(document.getElementById('preview-wrap')), previewTextWidth:textWidth(preview),
      previewParagraph:rect(preview?.querySelector('p')), previewTable:rect(preview?.querySelector('table')), previewCode:rect(preview?.querySelector('pre')),
      article:rect(article), articleTextWidth:textWidth(article), readerParagraph:rect(para), readerTable:rect(article?.querySelector('table')),readerTableViewport:rect(article?.querySelector('.rd-table-wrap')||article?.querySelector('table')),
      readerImage:rect(article?.querySelector('img[alt="Synthetic wide image"]')),readerInlineImage:rect(article?.querySelector('img[alt="Inline symbol"]')),
      previewImage:rect(preview?.querySelector('img[alt="Synthetic wide image"]')),previewInlineImage:rect(preview?.querySelector('img[alt="Inline symbol"]')),
      previewInlineParagraph:rect(preview?.querySelector('img[alt="Inline symbol"]')?.closest('p')),
      readingWidth:state.readingWidth,lineWidth:state.lineWidth,
    };
  });
}

async function run() {
  assert.ok(process.env.READMD_BIN, 'READMD_BIN is required; this runner never builds a binary');
  const base = path.join(root,'.cache/capability-upgrade/longtail-step12-20261018/editor-ux');
  fs.mkdirSync(base,{recursive:true});
  const label = process.env.READMD_EDITOR_UX_LABEL || 'native';
  assert.match(label,/^[a-z0-9-]+$/i);
  const directory = fs.mkdtempSync(path.join(base,label+'-'));
  const documents=path.join(directory,'documents'), profile=path.resolve(process.env.READMD_EDITOR_UX_PROFILE||path.join(directory,'profile')); fs.mkdirSync(documents);
  assert.ok(!path.relative(base,profile).startsWith('..')&&!path.isAbsolute(path.relative(base,profile)),'Reuse only an owned UX profile');
  const file=path.join(documents,'Viewport-evidence.md'), second=path.join(documents,'Second-document.md'),other=path.join(documents,'Independent-long-document.md'),picture=path.join(documents,'Wide-image.md');
  fs.writeFileSync(file,body); fs.writeFileSync(second,'# Second document\n\nIndependent short source.\n');fs.writeFileSync(other,'# Independent viewport\n\n'+body);
  fs.writeFileSync(picture,'# Image workspace\n\nA bounded prose line accompanies the image. Inline ![Inline symbol](Viewport-icon.svg) stays small.\n\n![Synthetic wide image](Viewport-wide.svg)\n');
  fs.writeFileSync(path.join(documents,'Viewport-wide.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="1800" height="600" viewBox="0 0 1800 600"><rect width="1800" height="600" fill="#e6eef7"/><path d="M100 450H1700M100 100V450" stroke="#496580" stroke-width="8"/><path d="M130 410 500 280 900 330 1300 170 1680 120" fill="none" stroke="#397f8d" stroke-width="20"/><text x="130" y="85" font-family="sans-serif" font-size="42" fill="#243c55">Synthetic observation map</text></svg>');
  fs.writeFileSync(path.join(documents,'Viewport-icon.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="6" fill="#397f8d"/></svg>');
  const assets=path.resolve(process.env.READMD_ASSETS_DIR || path.join(root,'assets'));
  const report={passed:false,formalCsp:true,nativeHost:'Windows WebView2',syntheticOnly:true,profileReused:Boolean(process.env.READMD_EDITOR_UX_PROFILE),checks:[],measurements:[],viewportTransitions:[],screenshots:[],pageErrors:[],
    binarySha256:hash(process.env.READMD_BIN),bootSha256:hash(path.join(assets,'readmd.boot.js')),panelsSha256:hash(path.join(assets,'css/panels.css')),scriptSha256:hash(__filename)};
  const portServer=net.createServer(); await new Promise(r=>portServer.listen(0,'127.0.0.1',r)); const port=portServer.address().port; await new Promise(r=>portServer.close(r));
  let session,cdp,configured;
  const screenshot=async(name,strict=true)=>{
    if(!session||!cdp)return;
    // Playwright's native screenshot path reapplies its context DPI and can
    // reset a direct CDP viewport override. Capture the real WebView surface.
    const before=(await metrics(session.page)).viewport,result=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false,fromSurface:true}),bytes=Buffer.from(result.data,'base64');
    const pixels={width:bytes.readUInt32BE(16),height:bytes.readUInt32BE(20)},after=(await metrics(session.page)).viewport;
    fs.writeFileSync(path.join(directory,name),bytes);report.screenshots.push({name,requested:configured,before,after,pixels,sha256:crypto.createHash('sha256').update(bytes).digest('hex')});
    if(strict){assert.ok(Math.abs(before.dpr-after.dpr)<0.001,'Capturing a screenshot must retain the actual DPI');assert.equal(pixels.width,Math.round(before.width*before.dpr));assert.equal(pixels.height,Math.round(before.height*before.dpr));}
  };
  const settleEditor=async()=>{
    let previous='',stable=0,last;
    await until(async()=>{last=await metrics(session.page);const key=JSON.stringify([last.viewport,last.source,last.cmGeometry,last.sourceFirstVisible]);
      const matches=!configured||Math.abs(last.viewport.width-configured.width)<0.5&&Math.abs(last.viewport.height-configured.height)<0.5&&Math.abs(last.viewport.dpr-configured.deviceScaleFactor)<0.001;
      stable=matches&&key===previous?stable+1:0;previous=key;return stable>=2;});
    return last;
  };
  const check=async(name,fn)=>{
    try { const detail=await fn(); report.checks.push({name,passed:true,detail}); }
    catch(error) {
      const failure={name,passed:false,error:String(error.message)}; report.checks.push(failure);
      if(!report.firstFailure)report.firstFailure=failure;
      if(session){failure.metrics=await metrics(session.page).catch(()=>null);await screenshot(name.replace(/[^a-z0-9-]/gi,'-')+'-failure.png',false).catch(()=>{});}
    }
    fs.writeFileSync(path.join(directory,'report.json'),JSON.stringify(report,null,2)+'\n');
  };
  try {
    process.env.READMD_WORKSPACE=documents; process.env.READMD_NATIVE_LOG_FILE=path.join(directory,'native.log');
    session=await startNative(root,profile,chromium,port); const {page}=session;
    page.on('pageerror',e=>report.pageErrors.push({name:e.name,message:e.message}));
    await page.evaluate(()=>Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]));
    const servedPanels=await page.evaluate(async()=>await(await fetch('/assets/css/panels.css')).text());
    report.servedPanelsSha256=crypto.createHash('sha256').update(servedPanels).digest('hex');
    assert.equal(report.servedPanelsSha256,report.panelsSha256,'The native host must serve the supplied CSS assets');
    report.csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    assert.ok(!report.csp.includes("'unsafe-eval'"));
    cdp=await page.context().newCDPSession(page);
    await page.emulateMedia({reducedMotion:'reduce'});
    await page.evaluate(async file=>{await i18n.setLanguage('en',false);if(state.editing)exitEdit();await loadFile(file);$('side').classList.add('hidden');state.pvSync=false;state.lineWidth=860;state.readingWidth='normal';applySettings();},file);
    const viewport=async([width,height,deviceScaleFactor])=>{
      configured={width,height,deviceScaleFactor};const transition={requested:configured,initial:(await metrics(page)).viewport,observations:[]};report.viewportTransitions.push(transition);
      await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor,mobile:false});
      await page.evaluate(()=>document.fonts?.ready);let stable=0,previous='';
      await until(async()=>{const m=await metrics(page),actual=m.viewport,geometry=m.cmGeometry;
        const matches=Math.abs(actual.width-width)<0.5&&Math.abs(actual.height-height)<0.5&&Math.abs(actual.dpr-deviceScaleFactor)<0.001;
        const key=JSON.stringify([actual,m.source,geometry]);if(transition.observations.length<12&&(transition.observations.at(-1)?.key!==key))transition.observations.push({key,actual,geometry});
        const measured=!m.editing||!geometry||Math.abs(geometry.scaleX-m.source.width/geometry.offsetWidth)<0.01&&Math.abs(geometry.scaleY-m.source.height/geometry.offsetHeight)<0.01;
        stable=matches&&measured&&key===previous?stable+1:0;previous=key;return stable>=2;
      });transition.settled=(await metrics(page)).viewport;
    };
    const resetEditor=async()=>{
      await page.evaluate(async file=>{if(state.editing)exitEdit();const tab=findTabByPath(file);if(tab&&tab.id!==state.activeTabId)await switchTab(tab.id);await loadFile(file,{force:true,discardConfirmed:true});
        state.pvPublication=false;state.pvSync=false;state.lineWidth=860;state.readingWidth='normal';applySettings();$('side').classList.add('hidden');state.pvLayout='right';await toggleEdit();},file);
      await settleEditor();
    };
    const selectFixture=async()=>{
      await page.evaluate(()=>{const CM=ReadMDCodeMirror,d=cmView.state.doc;cmView.dispatch({selection:CM.EditorSelection.create([CM.EditorSelection.range(d.line(85).from,d.line(81).from),CM.EditorSelection.range(d.line(89).from,d.line(90).to)],1),effects:CM.EditorView.scrollIntoView(d.line(81).from,{y:'start'})});cmView.focus();});
      const value=await settleEditor();assert.ok(Math.abs(value.sourceFirstVisible.line-81)<=2,'The requested source scroll target must be visible before testing restoration');return value;
    };

    await check('reader-width-preferences-and-4k',async()=>{
      const measurements=[];report.measurements.push({name:'reader',values:measurements});
      for(const size of sizes){
        await viewport(size);
        for(const width of ['narrow','normal','wide']){
          await page.evaluate(width=>{state.readingWidth=width;applySettings();},width);await frames(page);
          measurements.push(await metrics(page));
        }
        if(size[0]===3840){
          const [narrow,normal,wide]=measurements.slice(-3);
          assert.ok(wide.articleTextWidth>normal.articleTextWidth+80,`Wide reading preference must visibly widen the reader: ${JSON.stringify({viewport:wide.viewport,narrow:narrow.articleTextWidth,normal:normal.articleTextWidth,wide:wide.articleTextWidth})}`);
          assert.ok(normal.articleTextWidth>=1000&&wide.articleTextWidth>=1200,`4K structured reader should use a wider frame: ${JSON.stringify({viewport:wide.viewport,normal:normal.articleTextWidth,wide:wide.articleTextWidth})}`);
          assert.ok(Math.abs(wide.readerParagraph.x-wide.readerTable.x)<=2,'Wide reader prose and table must share a leading edge');
        }
      }
      assert.ok(measurements.every(m=>!m.outerOverflow),'Reader must not overflow compact windows');
      await screenshot('reader-compact.png');
      await viewport(sizes[0]);await page.evaluate(()=>{state.readingWidth='wide';applySettings();});await frames(page);
      await screenshot('reader-3840-wide.png');
      return measurements.length;
    });
    // Preserve before screenshots even when an earlier width assertion fails.
    await viewport(sizes[0]);await page.evaluate(()=>{state.readingWidth='wide';applySettings();});await frames(page);
    await screenshot('reader-3840-wide.png');

    await check('standalone-wide-image-and-inline-symbol-respect-measures',async()=>{
      await viewport(sizes[0]);await page.evaluate(async file=>{if(state.editing)exitEdit();await loadFile(file);state.pvSync=false;},picture);
      const values=[];
      for(const [lineWidth,readingWidth,minimum,maximum]of[[860,'normal',1000,1600],[860,'wide',1200,1600],[860,'narrow',0,650],[640,'normal',0,641],[640,'wide',0,641]]){
        await page.evaluate(({lineWidth,readingWidth})=>{if(state.editing)exitEdit();state.lineWidth=lineWidth;state.readingWidth=readingWidth;applySettings();},{lineWidth,readingWidth});
        await until(()=>page.locator('#content img[alt="Synthetic wide image"]').evaluate(img=>img.complete&&img.naturalWidth===1800));
        const reader=await metrics(page);assert.ok(reader.readerImage.width>=minimum&&reader.readerImage.width<=maximum,'Reader image retains its selected workspace measure');assert.ok(reader.readerInlineImage.width<=17,'An inline symbol must retain its natural size');
        assert.equal(await page.locator('#content img[alt="Synthetic wide image"]').evaluate(img=>img.parentElement.tagName),'FIGURE','Ordinary Markdown images use the real reader figure upgrade');
        if(lineWidth===860&&readingWidth==='wide')await screenshot('reader-wide-image-3840.png');
        await page.evaluate(async()=>{state.pvLayout='right';await toggleEdit();});
        await until(()=>page.locator('#preview-pane img[alt="Synthetic wide image"]').evaluate(img=>img.complete&&img.naturalWidth===1800));
        const preview=await settleEditor();assert.ok(preview.previewImage.width>=minimum&&preview.previewImage.width<=maximum,'Standalone preview images retain the workspace measure');assert.ok(preview.previewInlineImage.width<=17,'Preview inline symbols stay inside prose at their natural size');assert.ok(!preview.outerOverflow);
        if(minimum){assert.ok(reader.readerParagraph.width<reader.readerImage.width-100,'Inline image paragraphs retain the reader prose measure');assert.ok(preview.previewInlineParagraph.width<preview.previewImage.width-100,'Inline image paragraphs retain the preview prose measure');}
        values.push({lineWidth,readingWidth,reader,preview});if(lineWidth===860&&readingWidth==='wide')await screenshot('split-wide-image-3840.png');
      }
      report.measurements.push({name:'standalone-images',values});await resetEditor();return values.length;
    });

    await check('first-visible-source-without-scrolling',async()=>{
      const measurements=[];
      for(const size of sizes){
        await viewport(size);
        for(const layout of ['right','none','left','top','bottom']){
          await page.evaluate(async layout=>{if(state.editing)exitEdit();state.pvLayout=layout;await toggleEdit();},layout);
          await frames(page,2);const m=await metrics(page);measurements.push(m);
          assert.ok(m.visibleSourceLines>=3,`Source must paint without a scroll at ${size.join('/')} ${layout}`);
          assert.ok(!m.outerOverflow,`Split layout must fit ${size.join('/')} ${layout}`);
          assert.equal(m.sourceOuterTop,0,'CodeMirror must own its source scrolling');
        }
      }
      report.measurements.push({name:'source-first-paint',values:measurements});return measurements.length;
    });

    await check('split-preview-4k-structured-content',async()=>{
      await viewport(sizes[0]);await page.evaluate(()=>{setPvLayout('right');state.readingWidth='normal';applySettings();});
      await until(()=>page.locator('#preview-pane table').count().then(n=>n>0));await frames(page,3);
      const m=await metrics(page);report.measurements.push({name:'split-4k',values:[m]});
      await screenshot('split-3840.png');
      assert.ok(m.source.width>=600&&m.previewWrap.width>=600,'Both split panes need usable width');
      assert.ok(m.previewCode.width>=Math.min(1320,m.previewWrap.width-80),'4K structured preview must use the available pane width');
      assert.ok(Math.abs(m.previewParagraph.x-m.previewTable.x)<=2,'Preview prose and table must share a leading edge');
      return m;
    });

    await check('selection-and-scroll-survive-repeated-entry',async()=>{
      const values=[];
      for(const reducedMotion of ['reduce','no-preference']){
        await page.emulateMedia({reducedMotion});await viewport(sizes[3]);
        await resetEditor();const before=await selectFixture();
        await page.evaluate(async()=>{exitEdit();await toggleEdit();});
        const after=await settleEditor();values.push({reducedMotion,before,after});
        assert.equal(after.sourceMotion.reduced,reducedMotion==='reduce');
        for(const value of [before,after])for(const spacer of value.sourceMotion.spacers){
          assert.equal(spacer.transition,'none','Virtual source spacer geometry must never transition');
          assert.equal(spacer.animated,false,'Virtual source height must not animate under either motion preference');
          assert.ok(Math.abs(spacer.actual-spacer.height)<=1,'Virtual source spacer height must match its measured layout');
        }
        assert.deepEqual(after.selection,before.selection,'Re-entering must preserve the full oriented multi-selection');
        assert.equal(after.sourceFirstVisible.line,before.sourceFirstVisible.line,'Re-entering must retain the visible source anchor');
        assert.ok(Math.abs(after.sourceFirstVisible.offset-before.sourceFirstVisible.offset)<=2,'Re-entering must retain the visible source offset');
        assert.ok(after.visibleSourceLines>=3,'Restored viewport must paint without a scroll');
      }
      await page.emulateMedia({reducedMotion:'reduce'});report.measurements.push({name:'selection-entry',values});return values;
    });

    await check('persisted-selection-and-independent-document',async()=>{
      await viewport(sizes[3]);await resetEditor();const before=await selectFixture();
      await page.evaluate(async()=>{exitEdit();const tab=getActiveTab();delete tab.editorState;delete tab.editorScrollSnapshot;await toggleEdit();});
      const restored=await settleEditor();assert.deepEqual(restored.selection,before.selection,'Profile memory retains all selection ranges');
      assert.equal(restored.sourceFirstVisible.line,before.sourceFirstVisible.line,'Profile memory restores the visible source anchor');
      assert.ok(Math.abs(restored.sourceFirstVisible.offset-before.sourceFirstVisible.offset)<=2,'Profile memory restores the visible source offset');
      const originalId=await page.evaluate(()=>state.activeTabId);
      await page.evaluate(async second=>{exitEdit();await loadFile(second);await toggleEdit();},second);await settleEditor();
      assert.equal(await page.evaluate(()=>getEditContent()),fs.readFileSync(second,'utf8'));
      assert.equal(await page.evaluate(()=>cmView.state.selection.main.head),0,'A different document retains its own caret');
      await page.evaluate(async id=>{exitEdit();await switchTab(id);await toggleEdit();},originalId);await frames(page,3);
      assert.deepEqual((await metrics(page)).selection,before.selection,'Inactive tabs retain their selection');return {persistentSelection:true,independentTab:true};
    });

    await check('rapid-layout-exit-keeps-measured-anchor',async()=>{
      await viewport(sizes[3]);await resetEditor();const before=await selectFixture();
      await page.evaluate(async()=>{setPvLayout('none');exitEdit();await toggleEdit();});const after=await settleEditor();
      report.measurements.push({name:'rapid-layout-exit',before,after});
      assert.deepEqual(after.selection,before.selection);assert.equal(after.sourceFirstVisible.line,before.sourceFirstVisible.line);
      assert.ok(Math.abs(after.sourceFirstVisible.offset-before.sourceFirstVisible.offset)<=2,'A pending layout and immediate exit must retain the visible anchor offset');return {before,after};
    });

    await check('queued-preview-scroll-does-not-cross-tabs-or-layout',async()=>{
      await viewport(sizes[3]);await resetEditor();
      const ids=await page.evaluate(async other=>{const original=state.activeTabId;exitEdit();await loadFile(other);await toggleEdit();cmView.scrollDOM.scrollTop=0;exitEdit();const independent=state.activeTabId;await switchTab(original);await toggleEdit();return {original,independent};},other);
      await settleEditor();await until(()=>page.evaluate(()=>!pvScrollDriver));
      const pending=await page.evaluate(async ids=>{state.pvSync=true;setPvLayout('right');$('preview-wrap').scrollTop=450;pvSyncFromPreview();const queued=pvSyncFrame;exitEdit();const cleared=pvSyncFrame===0&&pvScrollDriver===null&&pvScrollDriverTimer===null;await switchTab(ids.independent);await toggleEdit();state.pvSync=false;return {queued:Boolean(queued),cleared};},ids);
      assert.deepEqual(pending,{queued:true,cleared:true});const independent=await settleEditor();assert.equal(independent.sourceTop,0,'An old preview frame must not scroll a newly mounted document');
      const layout=await page.evaluate(()=>{state.pvSync=true;pvScrollDriver=null;$('preview-wrap').scrollTop=450;pvSyncFromPreview();setPvLayout('none');return {sourceTop:cmView.scrollDOM.scrollTop};});await settleEditor();
      assert.equal(await page.evaluate(()=>cmView.scrollDOM.scrollTop),layout.sourceTop,'An old preview frame must not apply after changing layout');
      await resetEditor();return {pending,independent,layout};
    });

    await check('layout-sidebar-resize-preserve-edit-and-undo',async()=>{
      await resetEditor();
      await page.evaluate(()=>{cmView.dispatch({selection:{anchor:0}});cmView.focus();});await page.keyboard.type('Viewport edit ');
      const edited=await page.evaluate(()=>getEditContent()), selection=await page.evaluate(()=>cmView.state.selection.toJSON());
      await page.locator('#edit-cancel').click();await page.locator('#close-confirm-cancel').click();
      assert.equal(await page.evaluate(()=>state.editing),true);assert.deepEqual(await page.evaluate(()=>cmView.state.selection.toJSON()),selection);
      for(const size of sizes){await viewport(size);for(const layout of ['left','right','top','bottom','none']){
        await page.evaluate(layout=>{setPvLayout(layout);toggleSide('toc');},layout);await frames(page,3);
        const m=await metrics(page);assert.ok(m.visibleSourceLines>=3);assert.ok(!m.outerOverflow);assert.deepEqual(m.selection,selection);
        assert.equal(await page.evaluate(()=>getEditContent()),edited);
      }}
      await page.evaluate(()=>cmUndo());assert.equal(await page.evaluate(()=>getEditContent()),body);assert.equal(fs.readFileSync(file,'utf8'),body);
      return {viewports:sizes.length,layouts:5,undo:true,sourceUntouched:true};
    });

    await check('compact-sidebar-and-extreme-split-retain-usable-source',async()=>{
      await resetEditor();
      const values=[];
      for(const size of [sizes[3],sizes[4]]){
        await viewport(size);
        for(const pct of [25,70]){
          await page.evaluate(pct=>{state.pvSplitX=pct;showSide('toc');setPvLayout('right');},pct);await frames(page,3);
          const m=await metrics(page);values.push(m);assert.ok(!m.outerOverflow,'A remembered split cannot overflow a sidebar-constrained window');
          assert.ok(m.source.width>=319,'The visible source retains a usable width');assert.ok(m.visibleSourceLines>=3);
          if(m.previewWrap.width>0)assert.ok(m.previewWrap.width>=319,'A visible preview retains a usable width');
          assert.equal(await page.evaluate(()=>state.pvSplitX),pct,'A temporary constraint must retain the preferred split');
        }
      }
      await page.evaluate(()=>{state.pvSplitX=50;$('side').classList.add('hidden');setPvLayout('right');});report.measurements.push({name:'compact-split-extremes',values});return values.length;
    });

    await check('explicit-measures-and-print-layout-remain-independent',async()=>{
      await resetEditor();
      await viewport(sizes[0]);await page.evaluate(()=>{state.lineWidth=640;state.readingWidth='narrow';applySettings();setPvLayout('right');});await frames(page,3);
      const m=await metrics(page);report.measurements.push({name:'explicit-narrow',values:[m]});
      const width=await page.evaluate(()=>{const el=cmView.contentDOM,s=getComputedStyle(el);return el.getBoundingClientRect().width-parseFloat(s.paddingLeft)-parseFloat(s.paddingRight);});
      assert.ok(width<=641,'Explicit source measure must be retained');
      assert.ok(m.previewParagraph.width<=700,'Explicit/narrow prose preference must be retained');
      assert.ok(m.preview.width<=641&&m.previewCode.width<=641&&m.previewTable.width<=641,'An explicit measure also bounds structured preview blocks');
      await page.evaluate(()=>exitEdit());
      const explicit=[];
      for(const lineWidth of [640,1200])for(const readingWidth of ['normal','wide']){
        await page.evaluate(({lineWidth,readingWidth})=>{state.lineWidth=lineWidth;state.readingWidth=readingWidth;applySettings();},{lineWidth,readingWidth});await frames(page,2);
        const reader=await metrics(page);assert.ok(reader.article.width<=lineWidth+1,'Reader retains an explicit measure in '+readingWidth);assert.ok(reader.readerTableViewport.width<=lineWidth+1,'The table scroll viewport retains the explicit measure');assert.ok(!reader.outerOverflow);
        await page.evaluate(()=>toggleEdit());await frames(page,2);const preview=await metrics(page);
        assert.ok(preview.preview.width<=lineWidth+1&&preview.previewCode.width<=lineWidth+1&&preview.previewTable.width<=lineWidth+1,'Preview retains an explicit measure in '+readingWidth);
        explicit.push({lineWidth,readingWidth,reader,preview});await page.evaluate(()=>exitEdit());
      }
      const reload=await page.evaluate(async()=>{const saved=await saveSettings();state.lineWidth=undefined;applySettings();const fallback={custom:document.body.dataset.customLineWidth,width:document.body.style.getPropertyValue('--line-width')};await loadSettings();return {saved,fallback,restored:state.lineWidth,custom:document.body.dataset.customLineWidth};});
      assert.equal(reload.saved,true);assert.deepEqual(reload.fallback,{custom:'false',width:'860px'});assert.equal(reload.restored,1200);assert.equal(reload.custom,'true');
      await page.evaluate(()=>{state.readingWidth='narrow';applySettings();});await page.emulateMedia({media:'print'});await frames(page,2);
      const narrowPrint=await metrics(page);
      await page.evaluate(()=>{state.readingWidth='normal';applySettings();});await frames(page,2);const normalPrint=await metrics(page);
      const printStyle=await page.evaluate(()=>{const el=document.querySelector('#content .rd-article'),s=getComputedStyle(el);return {maxWidth:s.maxWidth,padding:s.paddingLeft};});
      assert.deepEqual(printStyle,{maxWidth:'none',padding:'0px'},'Print retains its existing full printable surface');
      assert.ok(Math.abs(narrowPrint.articleTextWidth-normalPrint.articleTextWidth)<=2,'Screen reading widths must not enter print layout');
      await page.emulateMedia({media:'screen',reducedMotion:'reduce'});await page.evaluate(()=>toggleEdit());
      return {sourceMeasure:width,paragraph:m.previewParagraph.width,explicit,reload,narrowPrint,normalPrint,printStyle};
    });

    await check('native-ime-layout-preserves-composition',async()=>{
      await viewport(sizes[3]);await resetEditor();await page.evaluate(()=>{state.lineWidth=860;applySettings();setPvLayout('none');cmView.dispatch({selection:{anchor:0}});cmView.focus();});
      await cdp.send('Input.imeSetComposition',{text:'编辑中',selectionStart:1,selectionEnd:2});
      assert.equal(await page.evaluate(()=>cmView.composing),true,'Native IME must be active');
      await page.evaluate(()=>{setPvLayout('right');toggleSide('toc');});await frames(page,2);
      assert.equal(await page.evaluate(()=>cmView.composing),true,'Changing the layout must retain native composition');
      await cdp.send('Input.insertText',{text:'编辑完成'});await frames(page,2);
      assert.ok((await page.evaluate(()=>getEditContent())).startsWith('编辑完成'));
      await page.evaluate(()=>cmUndo());assert.equal(await page.evaluate(()=>getEditContent()),body);return {compositionRetained:true,undo:true};
    });

    const packPaths=[['typst',process.env.READMD_EDITOR_UX_TYPST_PACK],['fonts',process.env.READMD_EDITOR_UX_FONTS_PACK],['pdfium',process.env.READMD_EDITOR_UX_PDFIUM_PACK]];
    if(packPaths.every(([,file])=>file))await check('actual-publication-pages-retain-physical-layout',async()=>{
      report.offlinePacks=packPaths.map(([capability,file])=>({capability,sha256:hash(file),bytes:fs.statSync(file).size}));
      report.packReadiness=[];
      for(const [id,pack]of packPaths){
        const status=await page.evaluate(async id=>{const response=await apiFetch('/api/capabilities/packs',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'status',id})});return await response.json();},id);
        report.packReadiness.push({id,installed:status.installed,ready:status.ready,reused:status.installed&&status.ready});if(status.installed&&status.ready)continue;
        const result=await page.evaluate(async pack=>{const response=await apiFetch('/api/capabilities/packs',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'install',path:pack})});return {status:response.status,data:await response.json()};},path.resolve(pack));
        assert.equal(result.status,200);assert.equal(result.data.installed,true);
      }
      await viewport(sizes[0]);
      await page.evaluate(async file=>{exitEdit();await loadFile(file);await toggleEdit();await openExportModal();state.export.options=collectExportOptions();closeExportModal();
        state.pvPublication=true;setPvLayout('right');if(pvTimer){clearTimeout(pvTimer);pvTimer=null;}void renderPreview();},second);
      await until(()=>page.locator('.publication-page-sheet > img').evaluateAll(images=>images.length>0&&images.every(img=>img.complete&&img.naturalWidth>0)),120000);
      const pageMetrics=()=>page.evaluate(()=>{const pane=$('preview-pane'),sheet=pane.querySelector('.publication-page-sheet'),image=sheet.querySelector('img'),r=sheet.getBoundingClientRect(),s=getComputedStyle(pane);return {
        viewport:{width:innerWidth,height:innerHeight,dpr:devicePixelRatio},paged:pane.classList.contains('publication-paged'),maxWidth:s.maxWidth,padding:s.paddingLeft,
        sheet:{width:r.width,height:r.height,aspect:getComputedStyle(sheet).aspectRatio,revision:sheet.dataset.publicationRevision},image:{width:image.naturalWidth,height:image.naturalHeight},source:getEditContent()};});
      const before=await pageMetrics();assert.equal(before.paged,true);assert.ok(parseFloat(before.maxWidth)<=861,'Screen workspace width must not enter physical pages');
      assert.ok(before.sheet.revision,'An actual engine source-map revision is required');assert.ok(before.image.width>500&&before.image.height>500);
      assert.ok(Math.abs(before.sheet.width/before.sheet.height-before.image.width/before.image.height)<0.002,'Actual page image retains its physical aspect');
      await screenshot('actual-publication-3840.png');
      await viewport(sizes[2]);await frames(page,3);const after=await pageMetrics();
      assert.equal(after.sheet.revision,before.sheet.revision);assert.deepEqual(after.image,before.image);assert.equal(after.padding,before.padding);assert.equal(after.maxWidth,before.maxWidth);
      assert.ok(Math.abs(after.sheet.width/after.sheet.height-before.sheet.width/before.sheet.height)<0.002);assert.equal(after.source,fs.readFileSync(second,'utf8'));
      await screenshot('actual-publication-1920-dpr2.png');
      await page.evaluate(()=>{state.pvPublication=false;ReadMDPublication.stopPaged();});return {before,after,offlineOnly:true};
    });
    else report.publicationPages={executed:false,reason:'Supply the three explicit offline pack paths to enable actual engine pages'};
    await check('no-native-errors-or-file-mutation',async()=>{assert.deepEqual(report.pageErrors,[]);assert.equal(fs.readFileSync(file,'utf8'),body);return true;});
    report.passed=report.checks.every(c=>c.passed);
  } catch(error) {
    report.fatal=String(error.stack||error);process.exitCode=1;
    if(session)await screenshot('fatal.png',false).catch(()=>{});
  } finally {
    if(cdp)await cdp.detach().catch(()=>{});
    if(session){await session.browser.close().catch(()=>{});if(session.server.child.exitCode===null)session.server.child.kill();}
    report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(directory,'report.json'),JSON.stringify(report,null,2)+'\n');
    console.log(JSON.stringify({passed:report.passed,checks:report.checks.map(({name,passed})=>({name,passed})),report:path.join(directory,'report.json')}));
    if(!report.passed)process.exitCode=1;
  }
}
if(process.argv.includes('--list')||process.argv.includes('--help'))console.log(JSON.stringify({native:false,viewports:sizes,requiredEnvironment:['READMD_BIN'],optionalEnvironment:['READMD_ASSETS_DIR','READMD_EDITOR_UX_LABEL','READMD_EDITOR_UX_PROFILE','READMD_EDITOR_UX_TYPST_PACK','READMD_EDITOR_UX_FONTS_PACK','READMD_EDITOR_UX_PDFIUM_PACK'],downloads:false,builds:false}));
else run().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
