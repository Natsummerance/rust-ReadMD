'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const {spawnSync}=require('node:child_process');
test('independent artifact inspector distinguishes actual opaque projection slots from public layout and citation identities',()=>{
  const cases=[
    ['<readmd-csl data-slot="0123456789abcdef0123456789abcdef c0"/>',true],
    ['&lt;readmd-csl data-slot=&quot;0123456789abcdef0123456789abcdef b&quot;/&gt;',true],
    ['readmd-csl data-slot="0123456789abcdef0123456789abcdef c1"',true],
    ['<!--readmd:csl nonce c1-->',true],
    ['<readmd-csl/>',true],
    ['<div class="readmd-csl-layout-columns"><span class="readmd-csl-layout-left">[1]</span></div>',false],
    ['<w:tag w:val="readmd-csl-entry:study"/>',false],
    ['.readmd-csl-layout-indent { padding-left:4em; }',false],
    ['readmd-csl-layout-block',false],
    ['<span class="readmd-csl-smallcaps">Original 文献</span>',false],
    ['Plain original source content',false],
  ];
  const program='import importlib.util,json,sys\nspec=importlib.util.spec_from_file_location("inspector",sys.argv[1])\nm=importlib.util.module_from_spec(spec)\nspec.loader.exec_module(m)\nprint(json.dumps([m.leaked_citation_slot(text) for text in json.load(sys.stdin)]))';
  const r=spawnSync('python',['-c',program,path.join(__dirname,'inspect-publication-batch3.py')],{input:JSON.stringify(cases.map(row=>row[0])),encoding:'utf8',windowsHide:true,timeout:10000});
  assert.equal(r.status,0,String(r.stderr||r.error));assert.deepEqual(JSON.parse(r.stdout),cases.map(row=>row[1]));
});
