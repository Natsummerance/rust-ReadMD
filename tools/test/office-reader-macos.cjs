'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

// This function runs inside the dedicated native QA window.
async function checkReader(folder, names) {
  const checks = [];
  const check = (condition, label) => {
    if (!condition) throw Error(label);
    checks.push(label);
  };
  const wait = async (read, label) => {
    const until = Date.now() + 75000;
    while (!read()) {
      if (Date.now() >= until) throw Error(label);
      await new Promise(resolve => setTimeout(resolve, 60));
    }
  };
  try {
    await window.__readmdSessionReady;
    check(await listFolder(folder), 'opened_folder');
    for (const [i, name] of names.entries()) {
      const row = Array.from(document.querySelectorAll('#file-list [role="treeitem"]')).find(el => el.textContent.includes(name));
      check(row, 'source_row_' + i);
      row.click();
      const loaded = () => getActiveTab()?.name === name && document.querySelector('.office-preview')?.getAttribute('aria-busy') === 'false' && document.querySelector('.office-preview-page')?.naturalWidth > 0;
      await wait(loaded, 'first_page_' + i);
      check(getActiveTab().mode === 'office-preview' && !state.editing, 'reader_tab_' + i);
      check($('pg-page-select').value === '0' && !$('pg-next-btn').disabled && $('pg-prev-btn').disabled, 'pager_start_' + i);
      const image = document.querySelector('.office-preview-page');
      const url = image.src;
      $('pg-next-btn').click();
      await wait(() => loaded() && image.src !== url && image.complete && image.naturalWidth > 0, 'second_page_' + i);
      check($('pg-page-select').value === '1' && $('pg-next-btn').disabled && !$('pg-prev-btn').disabled, 'pager_end_' + i);
      const page = image.getBoundingClientRect(), pane = $('content').getBoundingClientRect();
      check(page.width > 200 && page.left >= pane.left - 1 && page.right <= pane.right + 1, 'page_fits_' + i);
      py.window_control('qa_capture_frame', {id:i+1});
      await new Promise(resolve => setTimeout(resolve, 700));
      const tab = getActiveTab();
      await closeTab(tab.id);
      await wait(() => !state.tabs.includes(tab), 'close_' + i);
      checks.push('closed_' + i);
    }
    window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'office_reader',checks}}));
    await py.request_quit();
  } catch (error) {
    const label = /^[a-z0-9_]+$/.test(error.message) ? error.message : 'reader_driver_error';
    window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase:'office_reader',checks:[...checks,label]}}));
    window.ipc.postMessage('readmd:quit');
  }
}

async function main() {
  assert.equal(process.platform, 'darwin');
  const [binary, assets, word, deck, output] = process.argv.slice(2).map(p=>path.resolve(p));
  fs.mkdirSync(output, {mode:0o700});
  const scratch=fs.mkdtempSync(path.join(output,'native-'));
  const [profile, docs, home, tmp] = ['profile','docs','home','tmp'].map(dir=>path.join(scratch,dir));
  for(const dir of [profile,docs,home,tmp]) fs.mkdirSync(dir);
  const names=['实验%20报告.docx','答辩%20演示.pptx'];
  const inputs=[word,deck];
  names.forEach((name,i)=>fs.copyFileSync(inputs[i],path.join(docs,name)));
  fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');
  fs.writeFileSync(path.join(profile,'native-qa.js'),'('+checkReader.toString()+')('+JSON.stringify(docs)+','+JSON.stringify(names)+')');
  fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',theme:'sepia',autoCheckUpdate:false,closeToTray:false,petEnabled:false}));
  const env = {PATH:process.env.PATH,LANG:'zh_CN.UTF-8',HOME:home,CFFIXED_USER_HOME:home,TMPDIR:tmp,SQLITE_TMPDIR:tmp,READMD_NATIVE_QA:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs};
  const child=spawn(binary,['--port','0','--require-token','--workspace',docs,'--data-dir',profile,'--assets',assets],{cwd:scratch,env,stdio:'ignore'});
  let timer;
  try {
    const code=await new Promise((resolve,reject)=>{
      child.once('error',reject);child.once('exit',resolve);
      timer=setTimeout(()=>{child.kill('SIGTERM');},180000);
    });
    assert.equal(code,0,'Owned native host must quit normally');
    const result=JSON.parse(fs.readFileSync(path.join(profile,'native-qa-result.json')));
    assert.equal(result.ok,true,JSON.stringify(result.checks));
    const frames=names.map((name,i)=>{
      assert.equal(digest(path.join(docs,name)),digest(inputs[i]));
      const frame=path.join(output,(i===0?'word':'slides')+'.png');
      fs.renameSync(path.join(profile,'native-frames',String(i+1).padStart(5,'0')+'.png'),frame);
      return digest(frame);
    });
    assert.deepEqual(fs.readdirSync(docs).sort(),names.slice().sort());
    const report={host:'Cocoa / WKWebView',kernel:digest(binary),boot:digest(path.join(assets,'readmd.boot.js')),checks:result.checks,frames};
    fs.writeFileSync(path.join(output,'result.json'),JSON.stringify(report,null,2)+'\n');console.log('Native window passed '+result.checks.length+' checks');
  } finally {
    clearTimeout(timer);
    if(child.exitCode!==null || child.signalCode || !child.pid) fs.rmSync(scratch,{recursive:true});
  }
}
main().catch(error=>{console.error(error.message);process.exitCode=1;});
