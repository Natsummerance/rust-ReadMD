// Keep OCR error and batch labels identical using the existing i18n writer.
// --check is read-only. This does not build the frontend or create repo caches.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
if(process.argv.slice(2).some(arg=>arg!=='--check'))throw Error('Only --check is supported');
const read=name=>JSON.parse(fs.readFileSync(path.join(root,name),'utf8'));
const spec=read('tools/worker-budget-translations.json');
const locales=Object.keys(read('assets/i18n/meta.json'));
const dicts=Object.fromEntries(locales.map(locale=>[locale,read('assets/i18n/'+locale+'.json')]));
for(const [key,texts]of Object.entries(spec))for(const locale of locales){
  if(typeof texts[locale]!=='string'||!texts[locale].trim())throw Error('Missing translation '+key+' '+locale);
  if(locale!=='en'&&texts[locale]===texts.en)throw Error('English fallback '+key+' '+locale);
}
spec['batch.err.ocr_busy']=spec['error.ocr_busy'];
spec['batch.err.ocr_timeout']=spec['error.ocr_timeout'];
spec['batch.err.cancelled']=Object.fromEntries(locales.map(locale=>[locale,dicts[locale]['error.cancelled']]));
if(Object.values(spec['batch.err.cancelled']).some(text=>typeof text!=='string'||!text.trim()))throw Error('Missing cancellation translation');
const changes=locales.flatMap(locale=>Object.entries(spec).filter(([key,texts])=>dicts[locale][key]!==texts[locale]).map(([key])=>({locale,key})));
if(process.argv.includes('--check')){
  console.log(JSON.stringify({locales:locales.length,keys:Object.keys(spec).length,changes},null,2));
  if(changes.length)process.exitCode=1;
}else if(changes.length){
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-worker-i18n-'));
  const file=path.join(directory,'keys.json');
  try{
    fs.writeFileSync(file,JSON.stringify(spec));
    execFileSync(process.execPath,[path.join(root,'tools/i18n-add.mjs'),file],{cwd:root,stdio:'inherit'});
  }finally{
    if(fs.existsSync(file))fs.unlinkSync(file);
    fs.rmdirSync(directory);
  }
  console.log(JSON.stringify({updated:changes.length,locales:locales.length,keys:Object.keys(spec).length}));
}else console.log(JSON.stringify({updated:0,locales:locales.length,keys:Object.keys(spec).length}));
