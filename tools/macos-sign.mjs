// Explicit inside-out signing. The free development mode creates an ad-hoc
// signature; users must allow the trusted app in macOS Privacy & Security.
// It never claims Developer ID identity or notarization. Credentials stay private.
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import crypto from 'node:crypto';

const MACH_O = new Set([0xfeedface,0xcefaedfe,0xfeedfacf,0xcffaedfe,0xcafebabe,0xbebafeca,0xcafebabf,0xbfbafeca]);
const digest=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
function command(program,args) {
  const result=spawnSync(program,args,{encoding:'utf8',maxBuffer:4*1024*1024});
  if(result.error||result.status!==0)throw Error(path.basename(program)+' failed (diagnostics withheld; inspect the owned artifact locally)');
  return result.stdout;
}
export function signingOptions(env=process.env) {
  const mode=env.READMD_MACOS_SIGNING_MODE||'development';
  if(!['development','distribution'].includes(mode))throw Error('Unknown macOS signing mode');
  if(mode==='development')return {mode,identity:'-',keychain:null};
  const identity=env.READMD_MACOS_SIGNING_IDENTITY;
  if(!identity||identity==='-'||/[\r\n\0]/.test(identity))throw Error('Distribution requires a Developer ID Application identity');
  return {mode,identity,keychain:env.READMD_MACOS_SIGNING_KEYCHAIN||null};
}
function inventory(directory) {
  const binaries=[],bundles=[];
  function visit(dir) {
    for(const entry of fs.readdirSync(dir,{withFileTypes:true})) {
      const file=path.join(dir,entry.name);
      if(entry.isSymbolicLink()) {
        const real=fs.realpathSync(file);
        if(real!==directory&&!real.startsWith(directory+path.sep))throw Error('Code bundle contains an escaping symlink');
      } else if(entry.isDirectory()) {
        visit(file);
        if(/\.(app|framework|xpc|appex|bundle)$/.test(entry.name))bundles.push(file);
      } else if(entry.isFile()) {
        const fd=fs.openSync(file,'r');
        try {const magic=Buffer.alloc(4);if(fs.readSync(fd,magic,0,4,0)===4&&MACH_O.has(magic.readUInt32BE()))binaries.push(file);}
        finally{fs.closeSync(fd);}
      }
    }
  }
  visit(directory);
  return {binaries,bundles};
}
function signOne(file,options) {
  const args=['--force','--sign',options.identity];
  if(options.mode==='distribution') {
    if(!file.endsWith('.dmg'))args.push('--options','runtime');
    args.push('--timestamp');
  }
  else args.push('--timestamp=none');
  if(options.keychain)args.push('--keychain',options.keychain);
  command('/usr/bin/codesign',[...args,file]);
  command('/usr/bin/codesign',['--verify','--strict',file]);
}
export function signTarget(target,{env=process.env}={}) {
  if(process.platform!=='darwin')throw Error('macOS signing requires a native Mac');
  const options=signingOptions(env);
  target=path.resolve(target);
  const stat=fs.lstatSync(target);
  if(stat.isSymbolicLink())throw Error('Refusing to sign a target alias');
  if(stat.isFile()) {
    signOne(target,options);
    return {mode:options.mode,binaries:1,bundles:0,notarized:false};
  }
  const items=inventory(target);
  if(!items.binaries.length)throw Error('No Mach-O code found');
  // Signing changes executable bytes. Validate each managed worker's incoming
  // manifest before signing anything, then seal the new bytes in its manifest
  // before the outer app resource signature is generated.
  const workers=items.binaries.filter(file=>/\/engines\/resvg\/macos-(arm64|x64)\/resvg$/.test(file)).map(file=>{
    const manifest=path.join(path.dirname(file),'lock.json');
    const lock=JSON.parse(fs.readFileSync(manifest,'utf8'));
    if(lock.id!=='resvg'||lock.version!=='0.48.1'||lock.sha256!==digest(file)||lock.size!==fs.statSync(file).size)throw Error('Managed SVG worker failed integrity before signing');
    return {file,manifest,lock};
  });
  for(const binary of items.binaries)signOne(binary,options);
  for(const worker of workers){
    worker.lock.sha256=digest(worker.file);worker.lock.size=fs.statSync(worker.file).size;
    worker.lock.codeSignatureMode=options.mode;
    const temporary=worker.manifest+'.sign-'+crypto.randomUUID()+'.tmp';
    try{fs.writeFileSync(temporary,JSON.stringify(worker.lock,null,2)+'\n',{flag:'wx'});fs.renameSync(temporary,worker.manifest);}
    finally{if(fs.existsSync(temporary))fs.unlinkSync(temporary);}
  }
  for(const bundle of items.bundles)signOne(bundle,options);
  if(target.endsWith('.app')) {
    command('/usr/bin/plutil',['-lint',path.join(target,'Contents/Info.plist')]);
    signOne(target,options);
    command('/usr/bin/codesign',['--verify','--deep','--strict',target]);
  }
  return {mode:options.mode,binaries:items.binaries.length,bundles:items.bundles.length+(target.endsWith('.app')?1:0),notarized:false};
}
export function notarizeTarget(target,{env=process.env}={}) {
  if(process.platform!=='darwin')throw Error('Notarization requires a native Mac');
  if(signingOptions(env).mode!=='distribution')throw Error('Development signatures cannot be notarized');
  const key=env.READMD_MACOS_NOTARY_KEY_FILE,keyId=env.READMD_MACOS_NOTARY_KEY_ID,issuer=env.READMD_MACOS_NOTARY_ISSUER;
  if(!key||!keyId||!issuer||![keyId,issuer].every(v=>/^[a-zA-Z0-9-]+$/.test(v))||!fs.statSync(key).isFile())throw Error('Notarization credentials are required');
  target=path.resolve(target);
  if(!/\.(app|dmg)$/.test(target)||fs.lstatSync(target).isSymbolicLink())throw Error('Expected an owned app or disk image');
  let temporary;
  try {
    let upload=target;
    if(target.endsWith('.app')) {
      command('/usr/bin/codesign',['--verify','--deep','--strict',target]);
      temporary=fs.mkdtempSync(path.join(path.dirname(target),'.readmd-notary-'));
      upload=path.join(temporary,'ReadMD.zip');
      command('/usr/bin/ditto',['-c','-k','--sequesterRsrc','--keepParent',target,upload]);
    }
    const response=JSON.parse(command('/usr/bin/xcrun',['notarytool','submit',upload,'--key',key,'--key-id',keyId,'--issuer',issuer,'--wait','--output-format','json']));
    if(response.status!=='Accepted')throw Error('Apple notarization did not accept this artifact');
    command('/usr/bin/xcrun',['stapler','staple',target]);
    command('/usr/bin/xcrun',['stapler','validate',target]);
    if(target.endsWith('.app')) {
      command('/usr/bin/codesign',['--verify','--deep','--strict',target]);
      command('/usr/sbin/spctl',['--assess','--type','execute',target]);
    } else command('/usr/sbin/spctl',['--assess','--type','open','--context','context:primary-signature',target]);
    return {mode:'distribution',notarized:true,stapled:true,gatekeeperAccepted:true};
  } finally {if(temporary)fs.rmSync(temporary,{recursive:true});}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  try {
    const args=process.argv.slice(2);
    if(args[0]!=='--path'||!args[1]||args.length>3||(args[2]&&args[2]!=='--notarize'))throw Error('Usage: macos-sign.mjs --path <code, app or dmg> [--notarize]');
    const result=signTarget(args[1]);
    if(args[2])Object.assign(result,notarizeTarget(args[1]));
    console.log(JSON.stringify(result));
  } catch(error) {console.error(error.message);process.exitCode=1;}
}
