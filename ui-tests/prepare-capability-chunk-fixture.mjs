// Offline capacity fixture, never a distributable plugin or production catalog.
// Keeps the reviewed Typst worker unchanged; adds deterministic synthetic data
// so browser transport really crosses the old 128 MiB single-request boundary.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {build} from '../tools/build-stream-capability-pack.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=process.env.READMD_CHUNK_TYPST_RUNTIME;
if(!source)throw Error('READMD_CHUNK_TYPST_RUNTIME must name the reviewed extracted Typst runtime (bin, LICENSE, NOTICE). Preparation is offline.');
const base=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/chunk-fixture-')),runtime=path.join(base,'runtime'),assets=path.join(base,'assets'),manifestPath=path.join(base,'typst.lock.json'),output=path.join(base,'Capacity.readmd-pack');
fs.mkdirSync(runtime);fs.mkdirSync(assets);fs.mkdirSync(path.join(assets,'capabilities'));
function linkTree(from,to){const item=fs.lstatSync(from);if(item.isSymbolicLink())throw Error('Fixture source must not be a link');if(item.isDirectory()){fs.mkdirSync(to);for(const name of fs.readdirSync(from))linkTree(path.join(from,name),path.join(to,name));}else if(item.isFile())fs.linkSync(from,to);else throw Error('Unsupported asset entry');}
const manifest=JSON.parse(fs.readFileSync(path.join(root,'assets/capabilities/typst-0.15.1-windows-x64.lock.json')));manifest.schema=2;
for(const entry of manifest.files){const destination=path.join(runtime,...entry.path.split('/'));fs.mkdirSync(path.dirname(destination),{recursive:true});fs.linkSync(path.join(source,...entry.path.split('/')),destination);delete entry.compressedSize;}
fs.mkdirSync(path.join(runtime,'capacity'));
const padding=path.join(runtime,'capacity/synthetic.bin'),fd=fs.openSync(padding,'wx'),cipher=crypto.createCipheriv('aes-256-ctr',Buffer.alloc(32,71),Buffer.alloc(16,19)),hash=crypto.createHash('sha256'),size=112*1048576;
try{for(let written=0;written<size;){const bytes=cipher.update(Buffer.alloc(Math.min(1048576,size-written)));let offset=0;while(offset<bytes.length)offset+=fs.writeSync(fd,bytes,offset,bytes.length-offset);hash.update(bytes);written+=bytes.length;}cipher.final();fs.fsyncSync(fd);}finally{fs.closeSync(fd);}
manifest.files.push({path:'capacity/synthetic.bin',size,sha256:hash.digest('hex')});
manifest.review={...manifest.review,capacityTest:'TEST ONLY: deterministic 112 MiB synthetic bytes. Not published, not licensed as a new engine, not a TeX acceptance claim.'};
fs.writeFileSync(manifestPath,JSON.stringify(manifest,null,2)+'\n');
const built=await build({manifestPath,runtime,output,writeLock:true});
const prepared=JSON.parse(fs.readFileSync(manifestPath));
for(const entry of fs.readdirSync(path.join(root,'assets'),{withFileTypes:true})){
  if(entry.name==='capabilities')continue;
  const from=path.join(root,'assets',entry.name),to=path.join(assets,entry.name);
  // Native static-file confinement deliberately rejects directory junctions
  // pointing outside this root. Real directories plus immutable hard-linked
  // files preserve that boundary and avoid duplicating large assets on disk.
  linkTree(from,to);
}
for(const name of fs.readdirSync(path.join(root,'assets/capabilities')))fs.copyFileSync(path.join(root,'assets/capabilities',name),path.join(assets,'capabilities',name));
fs.copyFileSync(manifestPath,path.join(assets,'capabilities/typst-0.15.1-windows-x64.lock.json'));
const catalog=JSON.parse(fs.readFileSync(path.join(assets,'capabilities/catalog.json'))),entry=catalog.packs.find(p=>p.id==='typst');
Object.assign(entry,{format:'stream-v2',downloadBytes:built.bytes,installedBytes:built.installedBytes,sha256:built.sha256,manifestSha256:crypto.createHash('sha256').update(JSON.stringify(prepared)).digest('hex')});
fs.writeFileSync(path.join(assets,'capabilities/catalog.json'),JSON.stringify(catalog,null,2)+'\n');
const record={synthetic:true,productionCatalogChanged:false,published:false,bytes:built.bytes,sha256:built.sha256,expandedBytes:built.installedBytes,pack:output,assets,runtime,manifest:manifestPath};
fs.writeFileSync(path.join(base,'fixture.json'),JSON.stringify(record,null,2)+'\n');
console.log(JSON.stringify({fixture:path.relative(root,path.join(base,'fixture.json')),bytes:built.bytes,sha256:built.sha256,published:false}));
