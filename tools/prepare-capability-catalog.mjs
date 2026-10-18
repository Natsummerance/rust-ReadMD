// Assemble the application-owned delivery catalog from reviewed local packs.
// No network, package installation or script execution.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';
import {repository,validateCatalog,verifyLocalPack} from './lib/capability-publication.mjs';
const args=process.argv.slice(2),inputs=[];let tag='packs-2026-10-10';
for(let i=0;i<args.length;i++){if(args[i]==='--release'&&args[i+1])tag=args[++i];else if(args[i].startsWith('--'))throw Error('Unknown option');else inputs.push(args[i]);}
if(inputs.length!==4||!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(tag))throw Error('Usage: node tools/prepare-capability-catalog.mjs PDF_PACK TYPST_PACK OCR_PACK FONTS_PACK [--release TAG]');
const packs=inputs.map(file=>{const bytes=fs.readFileSync(file),manifest=JSON.parse(bytes).manifest;
  const {id,version,platform}=manifest;if(!['pdfium','typst','ocr','fonts'].includes(id)||![version,platform].every(v=>typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(v)))throw Error('Unreviewed platform or engine');
  const lock=JSON.parse(fs.readFileSync(`assets/capabilities/${id}-${version}-${platform}.lock.json`));if(JSON.stringify(lock)!==JSON.stringify(manifest))throw Error('Pack differs from application lock');
  const name=`readmd-${id}-${version}-${platform}.readmd-pack`;
  const entry={id,version,platform,labelKey:`capability.${id}.name`,descriptionKey:`capability.${id}.purpose`,features:id==='pdfium'?['pdf_editor']:id==='ocr'?['ocr']:id==='fonts'?['publication_fonts']:['academic_pdf'],downloadBytes:bytes.length,installedBytes:manifest.files.reduce((n,f)=>n+f.size,0),sha256:crypto.createHash('sha256').update(bytes).digest('hex'),url:`https://github.com/${repository}/releases/download/${tag}/${name}`,license:id==='pdfium'?'BSD-3-Clause; third-party notices included':id==='ocr'?'Apache-2.0 and BSD-2-Clause; licenses included':id==='fonts'?'OFL-1.1; original notices and derivative details included':'Apache-2.0; dependency and font notices included'};verifyLocalPack(bytes,lock,entry);return entry;
});
const catalog={schema:1,repository,release:tag,core:{labelKey:'capability.core.name',descriptionKey:'capability.core.purpose',requiresOffice:false,requiresPython:false,requiresNode:false},packs,pending:[{id:'speech',features:['transcribe'],reason:'portable-runtime-and-models-not-packaged'},{id:'latex',features:['xelatex','pdflatex'],reason:'portable-tex-distribution-not-packaged'}]};
validateCatalog(catalog);if(new Set(packs.map(p=>p.id)).size!==4)throw Error('One package per supported engine is required');
fs.writeFileSync('assets/capabilities/catalog.json',JSON.stringify(catalog,null,2)+'\n');
console.log(JSON.stringify(packs.map(({id,downloadBytes,installedBytes,sha256})=>({id,downloadBytes,installedBytes,sha256}))));
