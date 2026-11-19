'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');
const {execFileSync} = require('node:child_process');

module.exports = async function exportEditedTable({host,docs,options,python,browserOutput}) {
  const browser = JSON.parse(fs.readFileSync(path.join(browserOutput,'report.json')));
  assert.equal(browser.passed,true);
  const source = fs.readFileSync(path.join(browserOutput,'documents','tracks.md'));
  const hash = data=>createHash('sha256').update(data).digest('hex');
  assert.equal(hash(source),browser.savedSha256);
  const content = source.toString('utf8').replace(/^\ufeff/,'');
  const outputs = {};
  for(const [format,ext] of [['pdf','pdf'],['png','png'],['png-pages','zip']]) {
    const file = path.join(docs,'resized.'+ext);
    const resp = await host.api('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},
      body:JSON.stringify({format,content,options,baseDir:docs,out_path:file,suggestedName:'resized.md'})});
    const result=await resp.json();
    assert(result.ok,format+' '+result.error_code);
    outputs[format]=hash(fs.readFileSync(file));
  }
  // A valid editor height can exceed the printable area of a small page.
  // Repeat against the existing successful files so a late failure cannot truncate them.
  const impossible=content.replace(/Group \{width=(\d+)\}/,'Group {width=$1 height=600}');
  assert.notEqual(impossible,content);
  const rejected={};
  const shorterPage={...options,page:{...options.page,marginBottom:50}};
  for(const [format,ext] of [['pdf','pdf'],['png','png'],['png-pages','zip']]) {
    const file=path.join(docs,'resized.'+ext);
    const result=await (await host.api('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},
      body:JSON.stringify({format,content:impossible,options:shorterPage,baseDir:docs,out_path:file,suggestedName:'resized.md'})})).json();
    assert.equal(result.error_code,'publication_layout_geometry');
    assert.equal(hash(fs.readFileSync(file)),outputs[format]);
    rejected[format]=result.error_code;
  }
  const measurements=JSON.parse(execFileSync(python,['-B',path.join(__dirname,'table-track-export.py'),
    docs,String(browser.widthPx),String(browser.heightPx)],{encoding:'utf8',timeout:90000}));
  return {savedSourceSha256:browser.savedSha256,outputs,rejected,measurements};
};
