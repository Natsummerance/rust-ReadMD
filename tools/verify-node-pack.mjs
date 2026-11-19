// Verify prepared bytes against application-owned metadata. Registration is an
// explicit maintainer action, separate from archive creation and publication.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import{isDeepStrictEqual}from'node:util';import{fileURLToPath}from'node:url';
import{inputsLock}from'./prepare-node-runtime.mjs';import{validateManifest}from'./build-stream-capability-pack.mjs';import{validateCatalog}from'./lib/capability-publication.mjs';
const repo=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),assets=path.join(repo,'assets/capabilities');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
async function hash(file){const h=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file,{highWaterMark:65536}))h.update(chunk);return h.digest('hex');}
async function inspect(directory){
  const real=fs.realpathSync(directory);if(real!==path.resolve(directory)||fs.lstatSync(directory).isSymbolicLink())throw Error('Real prepared directory required');
  const receipt=JSON.parse(fs.readFileSync(path.join(real,'prepared.json'))),input=inputsLock.items.find(item=>item.platform===receipt.platform);
  if(!input||receipt.inputSha256!==input.sha256||receipt.id!=='node'||receipt.format!=='stream-v2'||!isDeepStrictEqual(receipt.compression,inputsLock.preparation))throw Error('Preparation differs from pinned source/toolchain');
  if(receipt.manifestFile!==`node-${inputsLock.version}-${input.platform}.lock.json`||receipt.packFile!==`readmd-node-${inputsLock.version}-${input.platform}.readmd-pack`)throw Error('Unexpected prepared filename');
  const manifest=JSON.parse(fs.readFileSync(path.join(real,receipt.manifestFile)));validateManifest(manifest);
  if(manifest.id!=='node'||manifest.version!==inputsLock.version||manifest.platform!==input.platform||manifest.files.length!==3||manifest.review.inputSha256!==input.sha256||!isDeepStrictEqual(manifest.review.compression,inputsLock.preparation))throw Error('Manifest differs from reviewed input');
  const file=path.join(real,receipt.packFile),metadata=fs.lstatSync(file);if(!metadata.isFile()||metadata.isSymbolicLink()||metadata.size!==receipt.bytes||await hash(file)!==receipt.sha256)throw Error('Prepared Node package changed');
  const fd=fs.openSync(file,'r');try{const first=Buffer.alloc(12);if(fs.readSync(fd,first,0,12,0)!==12||first.subarray(0,8).toString()!=='RMDPKG2\n')throw Error('Invalid streamed Node package');const length=first.readUInt32LE(8);if(!length||length>8*1048576)throw Error('Node header budget');const bytes=Buffer.alloc(length);if(fs.readSync(fd,bytes,0,length,12)!==length||!isDeepStrictEqual(JSON.parse(bytes),manifest))throw Error('Package/header manifest mismatch');}finally{fs.closeSync(fd);}
  // managed-v2 uses serde_json preserve_order for parity with its existing
  // Python/JS writers. Sorting keys here would create a different identity.
  const entry={id:'node',version:inputsLock.version,platform:input.platform,name:'Node.js',descriptionKey:'menu.runCode',features:['code_javascript'],delivery:'offline-only',format:'stream-v2',downloadBytes:receipt.bytes,installedBytes:manifest.files.reduce((n,f)=>n+f.size,0),sha256:receipt.sha256,manifestSha256:sha(Buffer.from(JSON.stringify(manifest))),license:'MIT; complete bundled third-party notices',minimumPlatform:input.minimum};
  if(entry.installedBytes!==receipt.installedBytes)throw Error('Prepared Node installed size mismatch');
  return{directory:real,file,manifest,entry};
}
export async function verifyPreparedNode(root,{register=false}={}){
  root=path.resolve(root);const directories=fs.existsSync(path.join(root,'prepared.json'))?[root]:inputsLock.items.map(item=>path.join(root,item.platform));
  const rows=[];for(const directory of directories)rows.push(await inspect(directory));
  const catalogFile=path.join(assets,'catalog.json'),before=fs.readFileSync(catalogFile),catalog=JSON.parse(before);
  if(register){
    if(rows.length!==6)throw Error('Registration requires all six reviewed platform packages');
    const old=catalog.offlinePacks||[];for(const row of rows){const existing=old.find(e=>e.id==='node'&&e.platform===row.entry.platform);if(existing&&!isDeepStrictEqual(existing,row.entry))throw Error('Existing Node metadata is immutable; review a new runtime version');}
    catalog.offlinePacks=[...old.filter(e=>e.id!=='node'),...rows.map(row=>row.entry)];validateCatalog(catalog);
    // Validate every existing target before any source write. No old lock is
    // replaced to turn a failed verification into a passing one.
    for(const row of rows){const file=path.join(assets,`node-${row.manifest.version}-${row.manifest.platform}.lock.json`);if(fs.existsSync(file)&&!isDeepStrictEqual(JSON.parse(fs.readFileSync(file)),row.manifest))throw Error('Existing Node lock differs');}
    for(const row of rows){const file=path.join(assets,`node-${row.manifest.version}-${row.manifest.platform}.lock.json`);if(!fs.existsSync(file))fs.writeFileSync(file,JSON.stringify(row.manifest,null,2)+'\n',{flag:'wx'});}
    if(!fs.readFileSync(catalogFile).equals(before))throw Error('Catalog changed during explicit registration');fs.writeFileSync(catalogFile,JSON.stringify(catalog,null,2)+'\n');
  }else{
    validateCatalog(catalog);for(const row of rows){const locked=[...catalog.packs,...(catalog.offlinePacks||[])].find(e=>e.id==='node'&&e.platform===row.entry.platform);if(!isDeepStrictEqual(locked,row.entry)||!isDeepStrictEqual(JSON.parse(fs.readFileSync(path.join(assets,`node-${row.manifest.version}-${row.manifest.platform}.lock.json`))),row.manifest))throw Error('Prepared Node bytes differ from application lock/catalog');}
  }
  return rows;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{const args=process.argv.slice(2),register=args.includes('--register'),github=args.includes('--github-env'),paths=args.filter(x=>!x.startsWith('--'));if(paths.length!==1||args.some(x=>x.startsWith('--')&&!['--register','--github-env'].includes(x)))throw Error('Provide one prepared directory and optional --register or --github-env');const rows=await verifyPreparedNode(paths[0],{register});
    if(github){if(rows.length!==1||!process.env.GITHUB_ENV)throw Error('One native platform and an explicit GitHub environment required');fs.appendFileSync(process.env.GITHUB_ENV,'READMD_TEST_NODE_PACK='+rows[0].file+'\n');}
    console.log(JSON.stringify({verified:rows.length,registered:register,packages:rows.map(row=>({platform:row.entry.platform,sha256:row.entry.sha256,manifestSha256:row.entry.manifestSha256})),nativeOperationVerified:false}));
  }catch(error){console.error(error.message);process.exitCode=1;}
}
