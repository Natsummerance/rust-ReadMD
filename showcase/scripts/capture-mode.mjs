/** Temporary presentation changes belong to the recorder, never user settings. */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
export const CAPTURE_PROFILE = 'immersive-v2';
export function uiSourceHash(root,{canonicalText=false}={}) {
  const hash = crypto.createHash('sha256');
  function walk(directory) {
    for (const entry of fs.readdirSync(directory, {withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name,'en'))) {
      if (['upstream','node_modules','vendor'].includes(entry.name)) continue;
      const file=path.join(directory,entry.name);
      if(entry.isDirectory())walk(file);
      else if(entry.isFile() && (/\.(html|js|css)$/.test(entry.name)||entry.name==='icon-256.png')) {
        const bytes=fs.readFileSync(file);
        hash.update(path.relative(root,file).replaceAll('\\','/'));hash.update('\0');hash.update(canonicalText&&/\.(html|js|css)$/.test(entry.name)?bytes.toString('utf8').replace(/\r\n?/g,'\n'):bytes);hash.update('\0');
      }
    }
  }
  walk(path.join(root,'assets'));
  return hash.digest('hex');
}
/** Preserve the actual recording hash when a checked release changes only version metadata. */
export function recordingUiHash(root) {
  const current=uiSourceHash(root);
  const file=path.join(root,'showcase/checks/release-ui-compatibility.json');
  if(!fs.existsSync(file))return current;
  const proof=JSON.parse(fs.readFileSync(file,'utf8'));
  const canonical=uiSourceHash(root,{canonicalText:true});
  if(proof.release_ui_sha256!==current&&proof.release_ui_canonical_sha256!==canonical)return current;
  const allowed=new Set(['assets/index.html','assets/js/features/updater.js','assets/readmd.boot.js']);
  for(const change of proof.changes){
    const bytes=fs.readFileSync(path.join(root,change.file));
    const raw=crypto.createHash('sha256').update(bytes).digest('hex');
    const normalized=crypto.createHash('sha256').update(bytes.toString('utf8').replace(/\r\n?/g,'\n')).digest('hex');
    if(!allowed.has(change.file)||(raw!==change.after_sha256&&normalized!==change.after_canonical_sha256))throw Error('Invalid release UI compatibility proof');
  }
  return proof.recorded_ui_sha256;
}
export async function installImmersiveCapture(page) {
  await page.evaluate(() => {
    if(document.getElementById('showcase-capture-style'))return;
    const style=document.createElement('style');style.id='showcase-capture-style';
    style.textContent='#toast,#busy,#status-mods{visibility:hidden!important}#showcase-caption,#showcase-cursor{display:none!important}';
    document.head.append(style);
    document.querySelector('#showcase-caption')?.remove();document.querySelector('#showcase-cursor')?.remove();
  });
}
export async function restoreCapturePresentation(page) {
  if(!page || page.isClosed())return;
  await page.evaluate(() => {document.getElementById('showcase-capture-style')?.remove();document.getElementById('showcase-privacy-layer')?.remove();});
}
