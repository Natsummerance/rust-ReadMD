import fs from 'node:fs';
const catalog=JSON.parse(fs.readFileSync('assets/capabilities/catalog.json'));
const lines=['; Generated from assets/capabilities/catalog.json. Do not guess pack sizes.'];
for(const id of ['pdfium','typst','ocr','fonts']){const entry=catalog.packs.find(p=>p.id===id&&(p.platform==='windows-x64'||p.platform==='universal'));if(!entry)throw Error('Missing reviewed installer component');const prefix=id==='pdfium'?'PDF_PACK':id==='ocr'?'OCR_PACK':id==='fonts'?'FONTS_PACK':'TYPST_PACK';for(const[key,value]of Object.entries({DOWNLOAD_MIB:(entry.downloadBytes/1048576).toFixed(1),INSTALLED_MIB:(entry.installedBytes/1048576).toFixed(1),INSTALLED_KIB:Math.ceil(entry.installedBytes/1024)}))lines.push(`!define ${prefix}_${key} "${value}"`);}
const file='installer/capability-components.nsh',output=lines.join('\n')+'\n';
if(process.argv.includes('--check')){if(fs.readFileSync(file,'utf8').replaceAll('\r\n','\n')!==output)throw Error('Installer sizes differ from reviewed catalog');console.log('Installer capability sizes match reviewed packages.');}else{fs.writeFileSync(file,output);console.log('Installer capability sizes updated.');}
