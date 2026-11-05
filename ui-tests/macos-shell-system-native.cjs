'use strict';
// Import/--plan is inert. --run is for the separately approved native window:
// one dedicated QA bundle, three fresh OS processes and synthetic workspace files.
// DOM activation/focus/scroll in WKWebView is explicit; no trusted-key or wheel claim.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), vm = require('node:vm');
const repo = path.resolve(__dirname, '..'), workspace = path.dirname(repo);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const phases = ['shell_edit_save', 'reopen_close', 'closed_stays_closed'];
const cases = [
  { id: 101, width: 1024, height: 680, theme: 'light', lang: 'zh-CN' },
  { id: 102, width: 1160, height: 820, theme: 'dark', lang: 'en' },
  { id: 103, width: 1024, height: 680, theme: 'dark', lang: 'ar' },
  { id: 104, width: 1160, height: 820, theme: 'light', lang: 'de' },
];
const A = '# OWNED_NATIVE_SHELL_A\n\n固定正文 A 中文😀。\n';
const B = '# OWNED_NATIVE_SHELL_B\n\n固定正文 B 中文😀。\n';
const ADDITION = '\nOWNED_NATIVE_SHELL_SAVED 中文🧭。\n';
const FIX = '# OWNED_NATIVE_SHELL_FIX\n\n' + Array.from({ length: 48 }, (_, index) => {
  const number = String(index + 1).padStart(3, '0');
  return '[OWNED_MISSING_' + number + '](#missing-' + number + ')\n\n';
}).join('');
const encode = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const frameIds = [101, 102, 103, 104, 201];

function plan() {
  return {
    status: 'prepared_not_executed', nativeAcceptanceExecuted: false,
    phases, cases, headerHeight: 52, minimumDomTarget: 44,
    independentInputs: { a: A, b: B, addition: ADDITION, fix: FIX, missingAnchors: 48 },
    expectations: [
      'Actual AppKit standard buttons fit the 52px web row with measured leading clearance; actions stay physically left of the right-anchored brand.',
      'At each fixed size/theme/locale the original sidebar returns active-tab focus, uses 44px controls and releases its measured width when closed.',
      'More opens its existing groups; the real bounded menu scrolls to Fix; the inspection lists exactly 48 fixed missing anchors and the real list scrolls.',
      'The modal receives actual WKWebView document.activeElement focus, contains 44px controls, and Close returns focus to More without changing source.',
      'AppKit green-button target/action and the existing native menu toggle actual NSWindow FullScreen state; stable viewport and native state are required.',
      'A synthetic editor change is saved through the existing visible Save control and genuine services, preserving BOM/CRLF bytes.',
      'Three normal OS launches/true quits restore the saved duplicate-title document, close only the background A tab, then keep it closed in a new process.',
      'One genuine guarded quit must return strict true; an explicitly opted-in debug QA host holds exactly one real quit until the matching valid terminal is freshly written and synced, then runs the original quit path.',
    ],
    boundaries: {
      f11AndEscape: 'pending: the existing host only exposes fixed Cmd+S/Cmd+Q NSEvent menu equivalents; no F11/Escape or generic key injection is exposed and this harness dispatches no synthetic KeyboardEvents',
      pointerAndScroll: 'WKWebView DOM activation/focus/programmatic scrolling; physical mouse, native wheel and keyboard traversal remain unverified',
      nativeFullscreenEvidence: '__readmdNativeWindowState.chrome.nativeFullscreen from NSWindow.styleMask FullScreen; DOM fullscreen flags are insufficient',
      frameEvidence: 'existing QA WKSnapshotConfiguration plus AppKit standard-button drawing, not a WindowServer/physical-display screenshot',
      osWrites: 'explicit --run uses the existing dedicated QA LaunchServices host; ordinary OS registration/cache effects require the separately coordinated native window',
      rustLayout: 'current macos_window_chrome HEADER is 48; assert actual buttons inside the 52px web row, never relabel that Rust constant as 52',
    },
  };
}

function driver(phase, files, config) {
  assert(phases.includes(phase));
  return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},config=${JSON.stringify(config)},checks=[];let sequence=0;
 const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
 const host=()=>window.__readmdNativeWindowState;
 const text=value=>String(value??'').replace(/^\uFEFF/,'').replace(/\r\n?/g,'\n');
 const visible=element=>!!element&&element.getClientRects().length>0&&getComputedStyle(element).display!=='none'&&!element.closest('.hidden');
 const progress=label=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:label||phase,checks}}));
 const check=(value,key)=>{if(!value)throw Error(key);checks.push(key);if(checks.length>62)throw Error('check_receipt_budget');progress();};
 const until=async(fn,key,ms=20000)=>{const end=Date.now()+ms;while(Date.now()<end){if(await fn())return;await wait(40);}throw Error(key);};
 const rect=element=>{const r=element.getBoundingClientRect();return{x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const settled=async()=>{await ReadMDSession.whenIdle();await until(()=>!window.ReadMDLifecycle?.busy?.(),'document_not_idle');await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));};
 const animationsSettled=async element=>{await until(()=>typeof element?.getAnimations==='function'&&element.getAnimations({subtree:true}).every(animation=>['finished','idle'].includes(animation.playState)),'actual_animation_did_not_finish');await settled();};
 const json=async(route,body)=>{const response=await apiFetch(route,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:undefined);const result=await response.json();if(!response.ok||result.ok===false)throw Error('genuine_api_rejected');return result;};
 const freshNative=async()=>{const before=host();py.window_control('status');await until(()=>host()&&host()!==before&&host().chrome?.buttons?.length===3,'fresh_native_snapshot_missing');return host();};
 const nativeAction=async(kind,action)=>{const id=++sequence;py.window_control(kind,{id,...action});const key=kind==='qa_native_button'?'__readmdNativeButtonResult':'__readmdNativeTrayResult';await until(()=>window[key]?.id===id,'native_action_ack_missing');if(!window[key].performed)throw Error('native_action_not_performed');};
 const stableNative=async wanted=>{let last='',since=0;await until(async()=>{const s=await freshNative(),c=s.chrome,key=[innerWidth,innerHeight,c.contentWidth,c.contentHeight,c.nativeFullscreen].join(',');if(key!==last){last=key;since=Date.now();}return c.nativeFullscreen===wanted&&s.fullscreen===wanted&&Date.now()-since>=1200&&(!wanted||innerWidth>=screen.availWidth-2&&innerHeight>=screen.availHeight-2)&&c.buttons.find(b=>b.kind==='fullscreen')?.enabled;},'native_space_did_not_settle',30000);};
 const capture=async id=>{py.window_control('qa_capture_frame',{id});progress('shell_capture_'+id);await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(files.ack));return r.ok&&String((await r.json()).content||'').split('\n').includes(String(id));},'owned_native_frame_missing',15000);};
 const tabFor=file=>state.tabs.find(tab=>tab.path===file);
 const rowFor=tab=>[...document.querySelectorAll('#side .tab-item')].find(row=>row.dataset.tabId===tab.id);
 async function sidebar(){
  $('titlebar-documents').click();await until(()=>visible($('side'))&&document.activeElement?.dataset.tabId===state.activeTabId,'sidebar_active_focus_missing');await settled();
  const side=rect($('side')),before=rect($('main-col'));
  if(side.width<200||$('doc-tabs-bar').getAttribute('aria-orientation')!=='vertical')throw Error('original_vertical_sidebar_missing');
  for(const button of [...$('side').querySelectorAll('.tab-close,#side-close-btn')].filter(visible)){const r=rect(button);if(r.width<44||r.height<44||!(button.getAttribute('aria-label')||button.title))throw Error('sidebar_target_too_small_or_unnamed');}
  $('side-close-btn').click();await until(()=>!visible($('side'))&&Math.abs(rect($('main-col')).width-before.width-side.width)<=1,'sidebar_width_not_released');
  await until(()=>document.activeElement===$('titlebar-documents'),'sidebar_opener_focus_not_restored');
 }
 async function layout(test){
  py.window_control('qa_window_size',{width:test.width,height:test.height});await until(()=>Math.abs(innerWidth-test.width)<2&&Math.abs(innerHeight-test.height)<2,'native_size_not_applied');
  state.theme=test.theme;applySettings();await i18n.setLanguage(test.lang,false);await settled();
  const s=await freshNative(),c=s.chrome,h=rect($('app-titlebar')),actions=rect($('titlebar-actions')),identity=rect(document.querySelector('.titlebar-identity')),logo=rect($('titlebar-home')),title=rect($('window-drag-region')),nativeRight=Math.max(...c.buttons.map(b=>b.x+b.width));
  check(s.focused===true&&s.visible===true&&c.platform==='macos'&&c.systemControls&&c.fullSizeContent&&c.titleHidden&&c.transparentTitlebar&&c.nativeFullscreen===false,'actual_cocoa_chrome_'+test.id);
  check(Math.abs(h.height-52)<=0.5&&Math.abs(h.y)<=0.5&&c.buttons.length===3&&c.buttons.every(b=>!b.hidden&&b.enabled&&b.width>8&&b.height>8&&b.x>=0&&b.y>=0&&b.y+b.height<=52)&&c.leadingInset>=nativeRight+10&&actions.x>=c.leadingInset-1,'native_buttons_clear_52px_row_'+test.id);
  check(actions.right<=identity.x+1&&title.right<=logo.x+1&&logo.x-title.right<=8&&logo.right>=innerWidth-20&&logo.right<=innerWidth+1&&getComputedStyle($('window-controls')).display==='none'&&!document.querySelector('.window-resize-edge'),'right_brand_left_actions_'+test.id);
  const controls=[...document.querySelectorAll('#app-titlebar button')].filter(visible),boxes=controls.map(rect);
  check(controls.every((button,index)=>{const r=boxes[index];return r.width>=44&&r.height>=44&&r.x>=nativeRight+10&&r.right<=innerWidth+1&&r.y>=0&&r.bottom<=52.5&&(button.getAttribute('aria-label')||button.title);})&&boxes.every((a,index)=>boxes.slice(index+1).every(b=>a.right<=b.x+0.5||b.right<=a.x+0.5||a.bottom<=b.y+0.5||b.bottom<=a.y+0.5))&&document.documentElement.scrollWidth<=innerWidth+1,'named_nonoverlapping_native_targets_'+test.id);
  await sidebar();check(true,'sidebar_width_and_focus_'+test.id);await capture(test.id);
 }
 async function fixMenu(){
  py.window_control('qa_window_size',{width:1024,height:680});await until(()=>Math.abs(innerWidth-1024)<2&&Math.abs(innerHeight-680)<2,'fix_native_size_not_applied');
  await i18n.setLanguage('zh-CN',false);$('btn-more').focus();$('btn-more').click();await until(()=>$('btn-more').getAttribute('aria-expanded')==='true','more_menu_did_not_open');
  for(const group of [...$('more-menu').querySelectorAll('.more-group')].filter(visible))if(!group.classList.contains('open'))group.querySelector('.more-group-header').click();
  const menu=$('more-menu');await settled();await animationsSettled(menu);const box=rect(menu);
  check(box.y>=0&&box.bottom<=innerHeight+1&&menu.scrollWidth<=menu.clientWidth+1&&menu.scrollHeight>menu.clientHeight&&['auto','scroll'].includes(getComputedStyle(menu).overflowY),'more_menu_bounded_and_scrollable');
  menu.scrollTop=menu.scrollHeight;await until(()=>menu.scrollTop>0,'more_menu_did_not_scroll');$('btn-fix').scrollIntoView({block:'nearest'});$('btn-fix').focus();await settled();
  await animationsSettled(menu);const item=rect($('btn-fix')),m=rect(menu);await freshNative();check(host().focused===true&&document.activeElement===$('btn-fix')&&!$('btn-fix').disabled&&item.height>=44&&item.width>=44&&item.y>=m.y-1&&item.bottom<=m.bottom+1,'more_fix_target_focused_and_reachable');
  $('btn-fix').click();await until(()=>visible($('fix-modal'))&&document.activeElement===$('fix-list'),'fix_modal_initial_focus_missing');
  await until(()=>document.querySelectorAll('#fix-list .document-inspection-issue').length===48,'fixed_anchor_inspection_missing');
  const issues=[...document.querySelectorAll('#fix-list .document-inspection-issue')];check(issues.every((row,index)=>row.textContent.includes('#missing-'+String(index+1).padStart(3,'0'))),'fixed_48_anchor_targets_preserved');
  await animationsSettled($('fix-box'));const modal=rect($('fix-box')),list=$('fix-list');check(modal.y>=0&&modal.bottom<=innerHeight+1&&$('btn-more').getAttribute('aria-expanded')==='false'&&[...$('fix-modal').querySelectorAll('button')].filter(visible).every(button=>rect(button).width>=44&&rect(button).height>=44)&&list.scrollHeight>list.clientHeight,'fix_dialog_44px_and_scroll_boundaries');
  list.scrollTop=list.scrollHeight;await settled();const last=rect(issues[47]),bounds=rect(list);check(list.scrollTop>0&&last.y>=bounds.y-1&&last.bottom<=bounds.bottom+1,'fix_last_issue_reachable_by_real_scroll');
  $('fix-close').click();await until(()=>!visible($('fix-modal'))&&document.activeElement===$('btn-more'),'fix_close_did_not_return_focus');check(true,'fix_close_returns_more_focus');
 }
 try{
  await __readmdSessionReady;
  check(hasPy&&py.native_titlebar==='macos'&&document.body.classList.contains('native-macos')&&document.body.classList.contains('shell-single-chrome'),'actual_cocoa_wkwebview');
  await until(()=>document.querySelector('.titlebar-mark')?.complete&&document.querySelector('.titlebar-mark').naturalWidth>0,'packaged_mac_brand_not_loaded');check(new URL(document.querySelector('.titlebar-mark').src).pathname==='/assets/ReadMD-macos.svg','packaged_mac_brand_asset');
  check(document.querySelector('meta[http-equiv="Content-Security-Policy"]').content===config.csp,'production_csp_unchanged');
  const denied=await fetch('/api/session');check([401,403].includes(denied.status),'unauthenticated_api_refused');
  check(editorPrefs.aiCompletion===false,'automatic_completion_disabled');
  if(phase==='shell_edit_save'){
   for(const file of [files.a,files.b,files.fix]){await loadFile(file);await settled();}
   check(state.tabs.length===3&&tabFor(files.a).id!==tabFor(files.b).id&&tabFor(files.a).name===tabFor(files.b).name,'duplicate_titles_keep_distinct_real_tabs');
   check(text(getActiveTab().content)===config.fix,'inspection_fixture_loaded_unchanged');
   for(const test of config.cases)await layout(test);
   await fixMenu();check(text((await json('/api/file?p='+encodeURIComponent(files.fix))).content)===config.fix,'inspection_keeps_source_unchanged');
   await i18n.setLanguage('en',false);py.window_control('qa_window_size',{width:1160,height:820});await until(()=>Math.abs(innerWidth-1160)<2&&Math.abs(innerHeight-820)<2,'normal_size_missing');await freshNative();
   await nativeAction('qa_native_button',{button:'fullscreen'});await stableNative(true);check(host().chrome.nativeFullscreen===true&&host().fullscreen===true&&document.body.classList.contains('zen-mode'),'appkit_green_enters_actual_fullscreen');await capture(201);
   await nativeAction('qa_tray',{action:'zen'});await until(()=>!document.body.classList.contains('zen-mode'),'native_zen_did_not_exit');await freshNative();check(host().chrome.nativeFullscreen===true&&host().fullscreen===true,'zen_exit_preserves_actual_fullscreen');
   await nativeAction('qa_native_button',{button:'fullscreen'});await stableNative(false);check(!document.body.classList.contains('zen-mode'),'appkit_green_exit_clears_zen');
   await nativeAction('qa_tray',{action:'fullscreen'});await stableNative(true);check(host().chrome.nativeFullscreen===true,'native_menu_enters_actual_fullscreen');
   await nativeAction('qa_tray',{action:'fullscreen'});await stableNative(false);check(!document.body.classList.contains('zen-mode'),'native_menu_exits_actual_fullscreen');
   await switchTab(tabFor(files.b).id);await settled();$('btn-edit').click();await until(()=>state.editing&&!!cmView,'visible_editor_missing');
   check(text(getEditContent())===config.b,'editor_opens_exact_saved_source');cmView.dispatch({changes:{from:cmView.state.doc.length,insert:config.addition}});await until(()=>getActiveTab().isDirty,'editor_did_not_become_dirty');await freshNative();check(host().chrome.documentEdited===true,'actual_native_dirty_indicator');
   $('edit-save').click();await until(()=>!getActiveTab().isDirty&&text(getEditContent())===config.b+config.addition,'visible_save_did_not_settle');
   check(text((await json('/api/file?p='+encodeURIComponent(files.b))).content)===config.b+config.addition,'genuine_save_committed_exact_body');await freshNative();check(host().chrome.documentEdited===false,'native_dirty_clears_after_save');
   if(state.editing){$('btn-edit').click();await until(()=>!state.editing,'saved_editor_did_not_close');}
  }else{
   await settled();check(getActiveTab()?.path===files.b&&text(getActiveTab().content)===config.b+config.addition&&!getActiveTab().isDirty,'new_process_restores_saved_body');
   if(phase==='reopen_close'){
    check(state.tabs.length===3&&!!tabFor(files.a)&&!!tabFor(files.fix),'new_process_restores_three_identities');
    $('titlebar-documents').click();await until(()=>visible(rowFor(tabFor(files.a))),'background_tab_not_visible');const selected=state.activeTabId;rowFor(tabFor(files.a)).querySelector('.tab-close').click();await until(()=>!tabFor(files.a),'background_tab_not_closed');
    check(state.tabs.length===2&&state.activeTabId===selected&&!visible($('close-confirm-modal')),'clean_background_close_preserves_active_document');
   }else check(state.tabs.length===2&&!tabFor(files.a)&&!!tabFor(files.fix),'closed_tab_stays_closed_after_new_process');
  }
  check(await ReadMDSession.flush()===true,'genuine_session_flush_acknowledged');
  const modules=await json('/api/modules');check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','model_modules_remain_idle');
  // Wry's actual IPC object is frozen. Leave it and the business guard intact;
  // only the explicitly opted-in debug QA host delays the received quit.
  const accepted=await py.request_quit();if(accepted!==true)throw Error('owned_quit_not_accepted');
  check(true,'guarded_quit_accepted');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
 }catch(error){checks.push('failed_'+String(error.message).replace(/[^a-z0-9_]/g,'_').slice(0,90));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks:checks.slice(-64)}}));}
})();`;
}

function owned(input, kind, fresh = false) {
  assert(typeof input === 'string' && path.isAbsolute(input) && path.resolve(input) === input, 'Explicit canonical absolute workspace path required');
  assert(input.startsWith(workspace + path.sep), 'Only workspace-owned QA inputs');
  let current = workspace;
  for (const part of path.relative(workspace, input).split(path.sep)) {
    current = path.join(current, part);
    if (!fs.existsSync(current)) { assert(fresh && current === input, 'Owned parent must exist'); continue; }
    assert(!fs.lstatSync(current).isSymbolicLink(), 'Owned input traverses a symlink');
  }
  if (fresh) assert(!fs.existsSync(input), 'Fresh evidence directory required');
  else { const stat = fs.lstatSync(input); assert(kind === 'file' ? stat.isFile() : stat.isDirectory()); assert.equal(fs.realpathSync(input), input); }
  return input;
}
async function run(app, out, assets) {
  assert.equal(process.platform, 'darwin', 'Native Mac QA only');
  for (const [input, kind] of [[app, 'directory'], [assets, 'directory']]) owned(input, kind);
  owned(out, 'directory', true);
  const { prepare } = require('./macos-native-system-host.cjs');
  const assetNames = ['index.html', 'readmd.boot.js', 'style.css', 'css/shell.css', 'css/panels.css', 'ReadMD-macos.svg'];
  const bindings = () => Object.fromEntries(assetNames.map(name => [name, sha(fs.readFileSync(path.join(assets, name)))]));
  const before = bindings(), csp = fs.readFileSync(path.join(assets, 'index.html'), 'utf8').match(/<meta\s+http-equiv="Content-Security-Policy"\s+content="([^"]+)"/i)?.[1];
  assert(csp?.includes("script-src 'self'") && !/script-src[^;]*'unsafe-(?:inline|eval)'/.test(csp), 'Formal CSP required');
  fs.mkdirSync(out, { mode: 0o700 });
  const dirs = Object.fromEntries(['documents', 'profile', 'tmp', 'native-home'].map(name => { const directory = path.join(out, name); fs.mkdirSync(directory, { mode: 0o700 }); return [name, directory]; }));
  const scratch = ['tmp', 'native-home'].map(name => ({ path: dirs[name], identity: fs.lstatSync(dirs[name]) }));
  for (const folder of ['a', 'b']) fs.mkdirSync(path.join(dirs.documents, folder));
  const files = { a: path.join(dirs.documents, 'a', '相同标题.md'), b: path.join(dirs.documents, 'b', '相同标题.md'), fix: path.join(dirs.documents, '检查与滚动.md'), ack: path.join(dirs.documents, 'native-frame-ack.txt') };
  for (const [key, text] of [['a', A], ['b', B], ['fix', FIX]]) fs.writeFileSync(files[key], encode(text), { flag: 'wx', mode: 0o600 });
  fs.writeFileSync(files.ack, '', { flag: 'wx', mode: 0o600 }); const ackIdentity = fs.lstatSync(files.ack);
  fs.writeFileSync(path.join(dirs.profile, '.readmd-native-qa-owned'), 'readmd-synthetic-native-qa-v1', { flag: 'wx', mode: 0o600 });
  fs.writeFileSync(path.join(dirs.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', theme: 'light', closeToTray: false, autoCheckUpdate: false, petEnabled: false }), { flag: 'wx', mode: 0o600 });
  const report = { passed: false, nativeAcceptanceExecuted: false, nativeHost: 'dedicated QA LaunchServices / Cocoa / WKWebView',
    nativeF11Certified: false, nativeEscapeCertified: false, trustedPointerCertified: false, nativeWheelCertified: false,
    modelInferenceRequestedByHarness: false, nodeInstallationRequested: false, chooserRequested: false, credentialsRequested: false,
    fakeBridge: false, cspModified: false, qaQuitHostBarrier: true, businessQuitGuardUnchanged: true, physicalMonitorScreenshot: false, frameRenderer: 'existing WKSnapshotConfiguration with AppKit standard controls',
    desktopBudgetMs: 180000, harnessSha256: sha(fs.readFileSync(__filename)), systemHostSha256: sha(fs.readFileSync(require.resolve('./macos-native-system-host.cjs'))),
    assetsBefore: before, plan: plan(), phases: [], frames: [], cleanup: { instances: [], allOwnedTerminated: false, scratchRemoved: false }, inputs: [['a', A], ['b', B], ['fix', FIX]].map(([key, text]) => ({ key, name: path.basename(files[key]), bytes: encode(text).length, sha256: sha(encode(text)) })) };
  const captured = new Set(); let desktopStarted, quitOptInIdentity;
  const failure = error => ({ name: error.name, message: String(error.message).replaceAll(out, '<owned>').replaceAll(workspace, '<workspace>').slice(0, 1000) });
  const verify = () => { assert.deepEqual(fs.readFileSync(files.a), encode(A)); assert.deepEqual(fs.readFileSync(files.b), encode(B + ADDITION)); assert.deepEqual(fs.readFileSync(files.fix), encode(FIX)); assert.deepEqual(bindings(), before); };
  function writeAcknowledgements() {
    const fd = fs.openSync(files.ack, fs.constants.O_RDWR | fs.constants.O_NOFOLLOW);
    try {
      const current = fs.fstatSync(fd); assert(current.isFile() && current.ino === ackIdentity.ino && current.dev === ackIdentity.dev, 'Original owned ACK inode required');
      const bytes = Buffer.from([...captured].join('\n') + '\n');
      fs.writeFileSync(fd, bytes); fs.ftruncateSync(fd, bytes.length); fs.fsyncSync(fd);
    } finally { fs.closeSync(fd); }
  }
  async function framesReady() {
    const progressFile = path.join(dirs.profile, 'native-qa-progress.json');
    if (!fs.existsSync(progressFile)) return;
    const p = JSON.parse(fs.readFileSync(progressFile)), id = Number(p.phase.match(/^shell_capture_(\d+)$/)?.[1]);
    if (!p.ok || !frameIds.includes(id) || captured.has(id)) return;
    const frame = path.join(dirs.profile, 'native-frames', String(id).padStart(5, '0') + '.png');
    if (!fs.existsSync(frame)) return; const stat = fs.lstatSync(frame);
    assert(stat.isFile() && !stat.isSymbolicLink(), 'Only the requested owned native frame');
    const bytes = fs.readFileSync(frame); if (bytes.length < 32 || bytes.subarray(-8, -4).toString() !== 'IEND') return;
    assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a'); assert.equal(bytes.readUInt32BE(16), 3840); assert(bytes.readUInt32BE(20) > 0);
    report.frames.push({ id, relativePath: path.relative(out, frame), bytes: bytes.length, width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20), sha256: sha(bytes) }); captured.add(id);
    writeAcknowledgements();
  }
  try {
    const host = prepare(app, out, { tmp: dirs.tmp, home: dirs['native-home'], assets });
    Object.assign(report, { binarySha256: host.binarySha256, helperSha256: host.helperSha256, bundleIdentifier: host.bundleIdentifier });
    desktopStarted = Date.now(); const deadline = desktopStarted + report.desktopBudgetMs;
    for (const phase of phases) {
      const script = driver(phase, files, { csp, cases, a: A, b: B, addition: ADDITION, fix: FIX });
      assert(Buffer.byteLength(script) <= 64 * 1024); new vm.Script(script);
      fs.writeFileSync(path.join(out, phase + '-driver.js'), script, { flag: 'wx', mode: 0o600 }); fs.writeFileSync(path.join(dirs.profile, 'native-qa.js'), script);
      const quitOptIn = Buffer.from(JSON.stringify({ schema: 'readmd-native-qa-guarded-quit-v1', phase }) + '\n');
      const quitFile = path.join(dirs.profile, 'native-qa-guarded-quit.json');
      const fd = fs.openSync(quitFile, fs.constants.O_WRONLY | fs.constants.O_NOFOLLOW | (quitOptInIdentity ? 0 : fs.constants.O_CREAT | fs.constants.O_EXCL), 0o600);
      try {
        const stat = fs.fstatSync(fd); assert(stat.isFile());
        if (quitOptInIdentity) assert(stat.dev === quitOptInIdentity.dev && stat.ino === quitOptInIdentity.ino, 'Original owned quit opt-in inode required');
        else quitOptInIdentity = stat;
        fs.writeFileSync(fd, quitOptIn); fs.ftruncateSync(fd, quitOptIn.length); fs.fsyncSync(fd);
      } finally { fs.closeSync(fd); }
      const ownership = { phase, launchAttempted: true, instanceReturned: false, cleanupAttempted: false, cleanupConfirmed: false }; report.cleanup.instances.push(ownership);
      const entry = { phase, driverSha256: sha(script), host: null, passed: false, quitBarrier: { ownedProfileOptIn: true, phase, optInSha256: sha(quitOptIn) } }; report.phases.push(entry);
      let instance, phaseError;
      try {
        // A rejected launch may have performed its own cleanup, but without a
        // returned owner this caller has no proof that all users of scratch stopped.
        instance = await host.launch({ profile: dirs.profile, docs: dirs.documents, log: path.join(out, phase + '.log') });
        ownership.instanceReturned = true; entry.host = instance.receipt; report.nativeAcceptanceExecuted = true;
        const resultFile = path.join(dirs.profile, 'native-qa-result.json');
        for (;;) {
          let terminal;
          try { terminal = JSON.parse(fs.readFileSync(resultFile, 'utf8')); }
          catch (error) { if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error; }
          if (terminal) {
            assert(typeof terminal.ok === 'boolean' && terminal.phase === phase && Array.isArray(terminal.checks) && terminal.checks.length <= 64 && terminal.checks.every(label => typeof label === 'string' && /^[a-z_0-9]{1,127}$/.test(label)), 'Invalid native terminal receipt');
            entry.result = terminal;
            assert.equal(terminal.ok, true, 'Native terminal rejected: ' + terminal.checks.join(','));
            assert(terminal.checks.includes('guarded_quit_accepted'), 'Strict genuine quit guard acceptance required');
          }
          if (instance.launcherStopped || Date.now() >= deadline) break;
          await framesReady(); await pause(40);
        }
        assert(Date.now() < deadline, 'Bounded native desktop deadline'); await instance.wait(Math.max(1, deadline - Date.now()));
        // The app may stop after accepting quit but before posting a terminal.
        // Missing/partial receipts still fail closed; process exit is not an ACK.
        entry.result = JSON.parse(fs.readFileSync(resultFile)); fs.renameSync(resultFile, path.join(out, phase + '.json'));
        assert.equal(entry.result.ok, true, entry.result.checks.join(',')); assert.equal(entry.result.phase, phase); assert(entry.result.checks.includes('guarded_quit_accepted'), 'Strict genuine quit guard acceptance required');
        assert.equal(instance.receipt.forcedCleanup, false); assert.equal(instance.receipt.nativeProcessGone, true); verify(); entry.passed = true;
      } catch (error) {
        phaseError = error; entry.failure = failure(error);
        if (!instance) ownership.launchError = failure(error);
        throw error;
      } finally {
        if (instance) {
          ownership.cleanupAttempted = true;
          try {
            await instance.cleanup(); ownership.cleanupConfirmed = true;
            ownership.nativeProcessGone = instance.receipt.nativeProcessGone === true;
            ownership.forcedCleanup = instance.receipt.forcedCleanup === true;
          } catch (error) {
            ownership.error = failure(error); entry.passed = false;
            if (!phaseError) throw error;
          }
        }
      }
    }
    assert.deepEqual([...captured].sort((a, b) => a - b), frameIds); assert.equal(new Set(report.phases.map(entry => entry.host.initialAppContext.pid)).size, phases.length);
    report.actualNsWindowFullscreenCertified = report.phases[0].result.checks.includes('appkit_green_enters_actual_fullscreen') && report.phases[0].result.checks.includes('native_menu_enters_actual_fullscreen');
    report.productionCspCertified = report.phases.every(entry => entry.result.checks.includes('production_csp_unchanged'));
    report.strictAuthCertified = report.phases.every(entry => entry.result.checks.includes('unauthenticated_api_refused'));
    report.desktopElapsedMs = Date.now() - desktopStarted; verify(); report.passed = true;
  } catch (error) {
    report.failure = failure(error); throw error;
  } finally {
    report.cleanup.allOwnedTerminated = report.cleanup.instances.every(instance => instance.cleanupConfirmed);
    report.cleanup.scratchPreserved = !report.cleanup.allOwnedTerminated;
    let finalizationError;
    try {
      if (report.cleanup.allOwnedTerminated) {
        for (const item of scratch) { const now = fs.lstatSync(item.path); assert(now.isDirectory() && !now.isSymbolicLink() && now.ino === item.identity.ino && now.dev === item.identity.dev); fs.rmSync(item.path, { recursive: true }); }
        report.cleanup.scratchRemoved = true;
      } else report.passed = false;
      report.assetsAfter = bindings(); report.assetsUnchanged = JSON.stringify(report.assetsAfter) === JSON.stringify(before); if (!report.assetsUnchanged) { report.passed = false; process.exitCode = 1; }
    } catch (error) {
      finalizationError = error; report.passed = false; report.failure ||= failure(error); report.cleanup.finalizationError = failure(error);
    }
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    if (finalizationError) throw finalizationError;
  }
  console.log(JSON.stringify({ passed: report.passed, phases: report.phases.length, frames: report.frames.length, nativeF11Certified: false, nativeEscapeCertified: false }));
}
if (require.main === module) {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--plan') console.log(JSON.stringify(plan(), null, 2));
  else if (args.length === 4 && args[0] === '--run') run(...args.slice(1)).catch(() => { console.error('Native shell acceptance failed; inspect the fresh owned report.'); process.exitCode = 1; });
  else { console.error('Usage: node ui-tests/macos-shell-system-native.cjs --plan | --run /absolute/NativeQA.app /absolute/new-output /absolute/assets'); process.exitCode = 1; }
}
module.exports = { plan, driver, run, phases, cases, encode, A, B, ADDITION, FIX };
