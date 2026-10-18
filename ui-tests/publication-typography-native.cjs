'use strict';
// Actual native service/OPC/preset persistence checks. Word's native page
// layout, cross-format consumers and authoring controls remain separate gates.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {createHarness,fileHash,sha256}=require('../tools/test/publication-batch3-harness.cjs');
const root=path.resolve(__dirname,'..');
const INVALID='publication_local_style_invalid',UNSUPPORTED='publication_line_spacing_unsupported';
const options={cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'},typography:{lineSpacing:{mode:'exact',pt:18}}};
const declaration=(id,mode)=>'<!-- readmd:layout v1 '+JSON.stringify({id,paragraph:{lineSpacing:mode}})+' -->\r\n\r\n';
const source='\uFEFF# Line spacing workshop\r\n\r\nUnmodified body.\r\n\r\n'+
  declaration('exact',{mode:'exact',pt:14.33})+'EXACT α 中文\r\n\r\n'+
  declaration('least',{mode:'atLeast',pt:20})+'MINIMUM β 中文\r\n\r\n'+
  declaration('multiple',{mode:'multiple',multiple:3})+'MULTIPLE γ 中文\r\n';
const inspector=String.raw`
import json,sys,zipfile,xml.etree.ElementTree as E
from pathlib import Path
p=Path(sys.argv[1]);w='{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
with zipfile.ZipFile(p) as z:
 assert z.testzip() is None
 names=z.namelist(); assert len(names)==len(set(names))
 xml={n:E.fromstring(z.read(n)) for n in names if n.endswith(('.xml','.rels'))}
styles=xml['word/styles.xml'];doc=xml['word/document.xml']
def props(n):
 p=n.find(w+'pPr'); assert p is not None
 s=p.findall(w+'spacing');assert len(s)==1
 assert p.find(w+'snapToGrid').get(w+'val')=='0'
 return {'line':s[0].get(w+'line'),'rule':s[0].get(w+'lineRule')}
body=[s for s in styles.findall(w+'style') if s.get(w+'styleId')=='BodyText'];assert len(body)==1
assert props(body[0])=={'line':'360','rule':'exact'}
for s in styles.findall(w+'style'):
 if s.get(w+'styleId') in ['Normal','Heading1','Code','Quote','ListParagraph','TableText','FootnoteText','TOC1']:
  assert not any(n.get(w+'lineRule')=='exact' for n in s.iter(w+'spacing'))
paras=[(''.join(p.itertext()),p) for p in doc.iter(w+'p')]
actual={}
for value,line,rule in [('EXACT α 中文','287','exact'),('MINIMUM β 中文','400','atLeast'),('MULTIPLE γ 中文','720','auto')]:
 found=[p for text,p in paras if text==value];assert len(found)==1
 actual[value]=props(found[0]);assert actual[value]=={'line':line,'rule':rule}
text='\n'.join(t for t,p in paras)
assert 'readmd:layout' not in text and 'Unmodified body.' in text
print(json.dumps({'packageCrcAndXml':True,'bodyStyle':props(body[0]),'localParagraphs':actual,'wordRoundTrip':False}))
`;
async function freePort(){const s=net.createServer();await new Promise((r,j)=>{s.once('error',j);s.listen(0,'127.0.0.1',r);});const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function main(){
  assert.equal(process.platform,'win32');
  const h=createHarness('publication-typography-native',await freePort(),__filename);
  const sourceFile=path.join(h.docs,'Line-spacing-workshop.md');fs.writeFileSync(sourceFile,source,{flag:'wx'});
  const inspectorFile=path.join(h.data,'inspect-spacing.py');fs.writeFileSync(inspectorFile,inspector,{flag:'wx'});
  const inputs=h.snapshotInputs(),responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  let page,requests=0;
  const expectedBoot=process.env.READMD_CANDIDATE_BOOT_SHA256;assert.match(expectedBoot||'',/^[a-f0-9]{64}$/);
  const sources=['rust/readmd-kernel/src/publication_typography.rs','rust/readmd-kernel/src/publication_blocks.rs','rust/readmd-kernel/src/docx_local_style.rs','rust/readmd-kernel/src/docx_writer.rs','rust/readmd-kernel/src/mdexport.rs','rust/readmd-kernel/src/export_styles.rs'];
  const before=Object.fromEntries(sources.map(p=>[p,fileHash(path.join(root,p))]));
  h.report.scope='P04 first DOCX-only line-spacing increment: real native API, OPC parameters, guarded outputs and preset process restart; no Word layout or full P04 acceptance';
  h.report.outputs=[];
  async function boot(){page=await h.start();await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));assert.equal(h.report.servedBootSha256,expectedBoot);
    const assets=process.env.READMD_ASSETS_DIR||path.join(root,'assets');const expected=fs.readFileSync(path.join(assets,'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)[1];
    h.report.csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');assert.equal(h.report.csp,expected);}
  async function api(route,body,status=200,code){const value=await h.api(route,body);fs.writeFileSync(path.join(responses,String(++requests).padStart(3,'0')+'.json'),JSON.stringify({route,request:body,response:value},null,2),{flag:'wx'});assert.equal(value.status,status,route);if(code)assert.equal(value.data.error_code,code);else assert.equal(value.data.ok,true,JSON.stringify(value.data));return value.data;}
  async function exported(name){const out=path.join(h.outputs,name);await api('/api/export',{format:'docx',content:source,baseDir:h.docs,out_path:out,options,suggestedName:'Line-spacing-workshop'});
    const inspected=JSON.parse(h.command('python',[inspectorFile,out]));h.report.outputs.push({name,sha256:fileHash(out),inspected});return out;}
  const getPresets=()=>page.evaluate(async()=>{const r=await apiFetch('/api/export/presets');return{status:r.status,data:await r.json()};});
  await h.finish(async()=>{
    await h.step('native packaged CSP and candidate boot',boot);
    await h.step('DOCX exact/minimum/multiple native XML with source BOM/CRLF intact',async()=>{await exported('Line-spacing-first.docx');h.unchanged(inputs);});
    await h.step('unimplemented formats keep existing targets and return typed capability errors',async()=>{
      for(const [format,engine] of [['html','rust'],['epub','rust'],['tex','rust'],['pdf','rust'],['pdf','typst'],['pdf','xelatex']]){
        const out=path.join(h.outputs,'protected-'+format+'-'+engine+'.'+format);fs.writeFileSync(out,'ORIGINAL TARGET',{flag:'wx'});
        await api('/api/export',{format,content:source,baseDir:h.docs,out_path:out,options:{...options,pdf:{engine}}},200,UNSUPPORTED);assert.equal(fs.readFileSync(out,'utf8'),'ORIGINAL TARGET');
      }h.unchanged(inputs);
    });
    await h.step('invalid new parameters refuse before resources or replacing an existing DOCX',async()=>{
      const out=path.join(h.outputs,'protected-invalid.docx');fs.writeFileSync(out,'ORIGINAL DOCX',{flag:'wx'});
      for(const typography of [{lineSpacing:{mode:'exact',pt:0}},{lineSpacing:{mode:'exact',pt:18,multiple:1.6}},{lineHeight:1.6,lineSpacing:{mode:'exact',pt:18}}]){
        await api('/api/export',{format:'docx',content:'![not loaded](missing.png)',baseDir:h.docs,out_path:out,options:{typography}},200,INVALID);
        assert.equal(fs.readFileSync(out,'utf8'),'ORIGINAL DOCX');
      }
    });
    await h.step('unsupported previews never claim a ready projection',async()=>{
      for(const format of ['docx','pdf','epub']){const value=await api('/api/export/preview',{format,content:source,baseDir:h.docs,options,suggestedName:'Line spacing'},200,UNSUPPORTED);assert.equal(value.key,undefined);}
    });
    await h.step('preset persists typed union and invalid update leaves bytes intact',async()=>{
      await api('/api/export/presets',{custom:{'Line spacing workshop':options}});const preset=await getPresets();assert.equal(preset.status,200);assert.deepEqual(preset.data.custom['Line spacing workshop'].typography.lineSpacing,options.typography.lineSpacing);
      const persisted=path.join(h.nativeData,'export_presets.json'),before=fileHash(persisted);
      await api('/api/export/presets',{custom:{'Line spacing workshop':{typography:{lineHeight:1.6,lineSpacing:{mode:'exact',pt:18}}}}},422,INVALID);assert.equal(fileHash(persisted),before);h.report.presetFileSha256=before;
    });
    await h.step('real process restart reads the saved preset and writes a second valid DOCX',async()=>{
      await h.stop();await boot();const preset=await getPresets();assert.equal(preset.status,200);assert.deepEqual(preset.data.custom['Line spacing workshop'].typography.lineSpacing,options.typography.lineSpacing);await exported('Line-spacing-after-restart.docx');h.unchanged(inputs);
    });
    await h.step('original input and production source hashes stay unchanged',async()=>{for(const [p,hash]of Object.entries(before))assert.equal(fileHash(path.join(root,p)),hash);h.unchanged(inputs);h.report.sourceHashes=before;h.report.inputHashes=inputs;h.report.actualRequests=requests;h.report.helperSha256=sha256(inspector);});
  });
}
main().catch(error=>{console.error(String(error.stack||error));process.exitCode=1;});
