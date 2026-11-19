'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {startKernel} = require('../tools/test/v006-refactor-host.cjs');

const png = Buffer.from([137,80,78,71,13,10,26,10]);
async function main() {
  const binary = process.env.READMD_BIN;
  const assets = process.env.READMD_IMAGE_TEST_ASSETS;
  assert(binary && path.isAbsolute(binary), 'Supply the platform release binary');
  const root = fs.mkdtempSync(path.join(os.tmpdir(),'readmd-image-export-'));
  let host;
  try {
    const docs = path.join(root,'docs');
    const profile = path.join(root,'profile');
    fs.mkdirSync(docs);
    fs.mkdirSync(profile);
    const post = async (route, body) => {
      const response = await host.api(route, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body)});
      const result = await response.json();
      assert(response.ok && result.ok, JSON.stringify({status:response.status, code:result.error_code || result.code || result.error}));
      return result;
    };
    host = await startKernel(binary, profile, docs, {assets});
    if (assets) {
      const route = '/api/capabilities/packs', id = 'pdfium';
      const status = () => post(route,{action:'status',id});
      const entry = (await post(route,{action:'catalog'})).packs.find(pack=>pack.id===id);
      assert(entry?.available && entry.delivery==='bundled' && !entry.url, 'expected a locked offline pack');
      assert.equal((await status()).installed,false);
      let job = await post(route,{action:'download',id});
      const deadline = Date.now()+45000;
      while (job.phase !== 'ready') {
        assert(job.job && Date.now() < deadline && job.phase !== 'error' && job.phase !== 'cancelled',job.error_code||'offline installation failed');
        await new Promise(resolve=>setTimeout(resolve,100));
        job = await post(route,{action:'download_status',job:job.job});
      }
      await host.stop();
      host = await startKernel(binary, profile, docs, {assets});
      const reopened = await status();
      assert(reopened.installed && reopened.ready, 'validated PDFium must reopen as ready');
    } else if (process.platform === 'linux') {
      const pack = path.join(docs,'PDFium.readmd-pack');
      fs.copyFileSync(path.resolve(__dirname,'../dist/ReadMD-PDFium-linux-x64.readmd-pack'),pack);
      await post('/api/capabilities/packs',{action:'install',id:'pdfium',path:pack});
      await host.stop();
      host = await startKernel(binary, profile, docs);
      assert((await post('/api/capabilities/packs',{action:'status',id:'pdfium'})).installed);
    }
    const content = '# Rendered report\n\n' + Array.from({length:35},(_,i)=>'## Result '+i+'\n\nA rasterized page contains searchable source text.\n\n').join('');
    const exportFile = async (format, file) => {
      const out = path.join(docs,file);
      await post('/api/export',{format,content,baseDir:docs,out_path:out,options:{footer:{pageNumbers:false}}});
      return fs.readFileSync(out);
    };
    const long = await exportFile('png','report.png');
    assert(long.subarray(0,8).equals(png));
    const pages = await exportFile('png-pages','report.zip');
    assert.equal(pages.readUInt32LE(0),0x04034b50);
    assert(pages.includes(Buffer.from('page-002.png')) && pages.includes(png));
    assert(long.readUInt32BE(20) > long.readUInt32BE(16));
    console.log(JSON.stringify({platform:process.platform, png:long.length, pagesZip:pages.length}));
  } finally {
    await host?.stop();
    fs.rmSync(root,{recursive:true});
  }
}
main().catch(error=>{console.error(error.message);process.exitCode=1;});
