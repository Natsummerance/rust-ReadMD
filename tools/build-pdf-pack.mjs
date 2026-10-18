// Offline preparation; no downloads, package scripts or executable installers.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';
const [engine,worker,output]=process.argv.slice(2);if(!engine||!worker||!output)throw Error('Usage: node tools/build-pdf-pack.mjs ENGINE_DIR WORKER_EXE OUTPUT.readmd-pack');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const payload={},files=[];
function add(name,source){const bytes=fs.readFileSync(source);payload[name]=bytes.toString('base64');files.push({path:name,size:bytes.length,sha256:hash(bytes)});}
add('bin/pdfium.dll',path.join(engine,'bin/pdfium.dll'));add('bin/readmd-pdf-worker.exe',worker);
if(files[0].sha256!=='79d4676b656cfb1abcea88f9ade3b4b0826c5200382db5f4ec72a636c598c118')throw Error('Unexpected PDFium binary');
add('LICENSE',path.join(engine,'LICENSE'));add('VERSION',path.join(engine,'VERSION'));add('args.gn',path.join(engine,'args.gn'));
for(const name of fs.readdirSync(path.join(engine,'licenses')).sort())add('licenses/'+name,path.join(engine,'licenses',name));
const crate=process.env.READMD_PDFIUM_CRATE;if(!crate)throw Error('READMD_PDFIUM_CRATE must point to the reviewed pdfium-render 0.9.4 source');
add('licenses/pdfium-render-NOTICE.md',path.join(crate,'LICENSE.md'));add('licenses/pdfium-render-APACHE-2.0.txt','assets/licenses/pdfium-render-APACHE-2.0.txt');
const registry=process.env.READMD_REGISTRY_SRC;if(!registry)throw Error('READMD_REGISTRY_SRC must point to the prepared Cargo registry sources');
for(const name of ['bytemuck-1.25.2','byteorder-1.5.0','itertools-0.15.0','libloading-0.9.0','maybe-owned-0.3.4','piston-float-1.0.1','utf16string-0.2.0','vecmath-1.0.0']){
 const directory=path.join(registry,name);for(const file of fs.readdirSync(directory).filter(n=>/^(LICEN[SC]E|COPYING)/i.test(n)&&fs.statSync(path.join(directory,n)).isFile()))add('licenses/'+name+'-'+file,path.join(directory,file));
}
const manifest={schema:1,id:'pdfium',version:'7881',platform:'windows-x64',files};fs.mkdirSync('assets/capabilities',{recursive:true});fs.writeFileSync('assets/capabilities/pdfium-7881-windows-x64.lock.json',JSON.stringify(manifest,null,2)+'\n');fs.mkdirSync(path.dirname(output),{recursive:true});fs.writeFileSync(output,JSON.stringify({manifest,payload}));console.log(JSON.stringify({pack:output,size:fs.statSync(output).size,sha256:hash(fs.readFileSync(output)),files:files.length}));
