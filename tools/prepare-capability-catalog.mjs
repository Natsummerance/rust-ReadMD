// Assemble the application-owned delivery catalog from reviewed local packs.
// No network, package installation or script execution. Preserve independently
// reviewed offline entries when regenerating the four online engine entries.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {repository,validateCatalog,verifyLocalPack} from './lib/capability-publication.mjs';

const assets=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../assets/capabilities');
const defaults={schema:1,repository,release:'packs-2026-10-10',core:{labelKey:'capability.core.name',descriptionKey:'capability.core.purpose',requiresOffice:false,requiresPython:false,requiresNode:false},packs:[],pending:[{id:'speech',features:['transcribe'],reason:'portable-runtime-and-models-not-packaged'},{id:'media-ffmpeg',features:['media_decode'],reason:'reviewed-platform-builds-not-packaged'},{id:'latex',features:['xelatex','pdflatex'],reason:'portable-tex-distribution-not-packaged'}]};
export function prepareCatalog({inputs,release,capabilities=assets}) {
  const catalogFile=path.join(capabilities,'catalog.json'),before=fs.existsSync(catalogFile)?fs.readFileSync(catalogFile):null;
  const existing=before?validateCatalog(JSON.parse(before)):structuredClone(defaults),tag=release??existing.release;
  if(inputs.length!==4||!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(tag))throw Error('Four reviewed engine packs and an immutable release tag required');
  const packs=inputs.map(file=>{
    const bytes=fs.readFileSync(file),manifest=JSON.parse(bytes).manifest;
    const {id,version,platform}=manifest;
    if(!['pdfium','typst','ocr','fonts'].includes(id)||![version,platform].every(v=>typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(v)))throw Error('Unreviewed platform or engine');
    const lock=JSON.parse(fs.readFileSync(path.join(capabilities,`${id}-${version}-${platform}.lock.json`)));
    const name=`readmd-${id}-${version}-${platform}.readmd-pack`;
    const entry={id,version,platform,labelKey:`capability.${id}.name`,descriptionKey:`capability.${id}.purpose`,features:id==='pdfium'?['pdf_editor']:id==='ocr'?['ocr']:id==='fonts'?['publication_fonts']:['academic_pdf'],downloadBytes:bytes.length,installedBytes:manifest.files.reduce((n,f)=>n+f.size,0),sha256:crypto.createHash('sha256').update(bytes).digest('hex'),url:`https://github.com/${repository}/releases/download/${tag}/${name}`,license:id==='pdfium'?'BSD-3-Clause; third-party notices included':id==='ocr'?'Apache-2.0 and BSD-2-Clause; licenses included':id==='fonts'?'OFL-1.1; original notices and derivative details included':'Apache-2.0; dependency and font notices included'};
    verifyLocalPack(bytes,lock,entry);return entry;
  });
  if(new Set(packs.map(p=>p.id)).size!==4)throw Error('One package per supported engine is required');
  const catalog={...existing,release:tag,packs};validateCatalog(catalog);
  if(before&&!fs.readFileSync(catalogFile).equals(before))throw Error('Catalog changed during preparation');
  const temporary=catalogFile+'.'+crypto.randomUUID()+'.partial';
  try{fs.writeFileSync(temporary,JSON.stringify(catalog,null,2)+'\n',{flag:'wx'});fs.renameSync(temporary,catalogFile);}
  finally{if(fs.existsSync(temporary))fs.unlinkSync(temporary);}
  return catalog;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  try {
    const args=process.argv.slice(2),inputs=[];let release;
    for(let i=0;i<args.length;i++){if(args[i]==='--release'&&args[i+1]&&!release)release=args[++i];else if(args[i].startsWith('--'))throw Error('Unknown option');else inputs.push(args[i]);}
    if(inputs.length!==4)throw Error('Usage: node tools/prepare-capability-catalog.mjs PDF_PACK TYPST_PACK OCR_PACK FONTS_PACK [--release TAG]');
    const catalog=prepareCatalog({inputs,release});
    console.log(JSON.stringify({packs:catalog.packs.map(({id,downloadBytes,installedBytes,sha256})=>({id,downloadBytes,installedBytes,sha256})),preservedOfflinePacks:catalog.offlinePacks?.length||0,published:false}));
  }catch(error){console.error('Catalog preparation failed: '+error.message);process.exitCode=1;}
}
