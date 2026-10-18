#!/usr/bin/env node
// Preparation is separate: this builder only reads locked, local archives.
// PNM input is decoded by the existing Rust image library. No curl, archive,
// TIFF, GUI, system language pack or C++ redistributable is needed at runtime.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {spawnSync} from 'node:child_process';
const args=process.argv.slice(2),value=key=>args[args.indexOf(key)+1];
if(!args.includes('--inputs')||!args.includes('--cmake')||!args.includes('--output'))throw Error('Usage: --inputs DIR --cmake EXE --output DIR');
if(process.platform!=='win32'||process.arch!=='x64')throw Error('This recipe is only verified on Windows x64.');
const inputs=path.resolve(value('--inputs')),output=path.resolve(value('--output')),cmake=path.resolve(value('--cmake'));
const lock=JSON.parse(fs.readFileSync('tools/ocr-runtime-inputs.lock.json'));
for(const item of lock.items){const bytes=fs.readFileSync(path.join(inputs,item.file));if(bytes.length!==item.bytes||crypto.createHash('sha256').update(bytes).digest('hex')!==item.sha256)throw Error('Input digest mismatch: '+item.id);}
fs.mkdirSync(output,{recursive:true});
function run(exe,argv){const r=spawnSync(exe,argv,{stdio:'inherit',windowsHide:true});if(r.status!==0)throw Error('Offline compiler failed: '+path.basename(exe));}
const sources=path.join(output,'sources');fs.mkdirSync(sources,{recursive:true});
for(const name of ['leptonica.tar.gz','tesseract.zip'])run('tar',['-xf',path.join(inputs,name),'-C',sources]);
const prefix=path.join(output,'prefix'),lept=path.join(output,'leptonica-build'),tess=path.join(output,'tesseract-build');
const common=['-G','Visual Studio 17 2022','-A','x64','-DBUILD_SHARED_LIBS=OFF','-DCMAKE_POLICY_DEFAULT_CMP0091=NEW','-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded','-DCMAKE_INSTALL_PREFIX='+prefix];
run(cmake,['-S',path.join(sources,'leptonica-1.87.0'),'-B',lept,...common,'-DSW_BUILD=OFF','-DBUILD_PROG=OFF',...['ZLIB','PNG','GIF','JPEG','TIFF','WEBP','OPENJPEG'].map(k=>'-DENABLE_'+k+'=OFF')]);
run(cmake,['--build',lept,'--config','Release','--parallel','4']);run(cmake,['--install',lept,'--config','Release']);
const commit=lock.items.find(i=>i.id==='tesseract').commit;
run(cmake,['-S',path.join(sources,'tesseract-'+commit),'-B',tess,...common,'-DCMAKE_PREFIX_PATH='+prefix,'-DBUILD_TRAINING_TOOLS=OFF','-DBUILD_TESTS=OFF','-DOPENMP_BUILD=OFF','-DENABLE_NATIVE=OFF','-DUSE_SYSTEM_ICU=OFF','-DDISABLE_ARCHIVE=ON','-DDISABLE_CURL=ON','-DDISABLE_TIFF=ON','-DGRAPHICS_DISABLED=ON','-DDISABLED_LEGACY_ENGINE=ON','-DENABLE_CCACHE=OFF']);
run(cmake,['--build',tess,'--config','Release','--target','tesseract','--parallel','4']);
const runtime=path.join(output,'runtime');fs.mkdirSync(runtime,{recursive:true});fs.copyFileSync(path.join(tess,'bin/Release/tesseract.exe'),path.join(runtime,'tesseract.exe'));
for(const name of ['eng','chi_sim','chi_tra'])fs.copyFileSync(path.join(inputs,name+'.traineddata'),path.join(runtime,name+'.traineddata'));
fs.copyFileSync(path.join(sources,'tesseract-'+commit,'LICENSE'),path.join(runtime,'TESSERACT-LICENSE'));fs.copyFileSync(path.join(sources,'leptonica-1.87.0','leptonica-license.txt'),path.join(runtime,'LEPTONICA-LICENSE'));fs.copyFileSync(path.join(inputs,'models.LICENSE'),path.join(runtime,'MODELS-LICENSE'));
fs.writeFileSync(path.join(runtime,'NOTICE'),'ReadMD portable OCR\nTesseract 5.5.3: Apache-2.0\nLeptonica 1.87.0: BSD-2-Clause\nOfficial tessdata_fast models: Apache-2.0\nBuilt with static MSVC runtime, no third-party codec libraries, networking, training tools or legacy OCR engine. Rust decodes supported images to PNM. CPU-specific host optimization disabled.\n');
run(path.join(runtime,'tesseract.exe'),['--version']);
console.log(JSON.stringify({runtime,offline:true}));
