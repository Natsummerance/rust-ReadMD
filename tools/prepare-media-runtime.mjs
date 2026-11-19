// Offline maintainer media assembly; never installs, downloads or runs engines.
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {prepareMedia,inspectPreparedMedia,registerPreparedMedia} from './lib/media-runtime.mjs';

export async function main(args=process.argv.slice(2)) {
  const [mode,...options]=args,values={};
  const allowed=mode==='build'?['--lock','--inputs','--output']:mode==='verify'?['--prepared','--lock']:mode==='register'?['--prepared','--lock','--assets','--native-receipt']:[];
  for(let i=0;i<options.length;i+=2){if(!allowed.includes(options[i])||!options[i+1]||options[i+1].startsWith('--')||values[options[i]])throw Error('Invalid media preparation option');values[options[i]]=options[i+1];}
  if(mode==='build'&&allowed.every(k=>values[k]))return prepareMedia({lock:values['--lock'],inputs:values['--inputs'],output:values['--output']});
  if(mode==='verify'&&values['--prepared']&&values['--lock']){
    const row=await inspectPreparedMedia(values['--prepared'],{lock:values['--lock']});
    return {verified:true,offline:true,id:row.entry.id,platform:row.entry.platform,sha256:row.entry.sha256,manifestSha256:row.entry.manifestSha256,nativeOperationVerified:false,published:false};
  }
  if(mode==='register'&&allowed.every(k=>values[k]))return registerPreparedMedia({directory:values['--prepared'],lock:values['--lock'],assets:values['--assets'],nativeReceipt:values['--native-receipt']});
  throw Error('Usage: build --lock REVIEWED.json --inputs DIR --output NEW_DIR | verify --lock REVIEWED.json --prepared DIR | register --lock REVIEWED.json --prepared DIR --assets ASSETS --native-receipt REVIEWED_NATIVE.json');
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main().then(result=>console.log(JSON.stringify(result)),error=>{console.error('Media preparation failed: '+error.message);process.exitCode=1;});
