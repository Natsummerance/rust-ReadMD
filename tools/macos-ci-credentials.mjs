// A dedicated, explicitly addressed keychain; never changes the user's keychain
// search list. All generated files stay in the selected checkout directory.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),directory=path.join(root,'.cache/macos-signing');
const keychain=path.join(directory,'signing.keychain-db');
function run(args) {
  const result=spawnSync('/usr/bin/security',args,{stdio:'pipe'});
  if(result.error||result.status!==0)throw Error('Temporary macOS keychain operation failed');
}
function cleanup() {
  if(!fs.existsSync(directory))return;
  if(fs.lstatSync(directory).isSymbolicLink()||fs.readFileSync(path.join(directory,'owned'),'utf8')!=='readmd-signing-v1')throw Error('Refusing unowned signing directory cleanup');
  if(fs.existsSync(keychain)) {
    if(fs.lstatSync(keychain).isSymbolicLink())throw Error('Refusing keychain alias cleanup');
    run(['delete-keychain',keychain]);
  }
  for(const name of ['certificate.p12','notary.p8','owned']) {
    const file=path.join(directory,name);
    if(fs.existsSync(file))fs.unlinkSync(file);
  }
  fs.rmdirSync(directory);
}
try {
  if(process.platform!=='darwin')throw Error('Native Mac required');
  if(process.argv[2]==='cleanup')cleanup();
  else if(process.argv[2]==='prepare') {
    const names=['MACOS_CERTIFICATE_P12_BASE64','MACOS_CERTIFICATE_PASSWORD','MACOS_NOTARY_KEY_P8','READMD_MACOS_SIGNING_IDENTITY','READMD_MACOS_NOTARY_KEY_ID','READMD_MACOS_NOTARY_ISSUER'];
    if(names.some(name=>!process.env[name]))throw Error('Formal macOS distribution requires certificate and notarization secrets');
    fs.mkdirSync(path.dirname(directory),{recursive:true});
    fs.mkdirSync(directory,{mode:0o700});
    fs.writeFileSync(path.join(directory,'owned'),'readmd-signing-v1',{flag:'wx',mode:0o600});
    try {
      const certificate=path.join(directory,'certificate.p12');
      fs.writeFileSync(certificate,Buffer.from(process.env.MACOS_CERTIFICATE_P12_BASE64,'base64'),{flag:'wx',mode:0o600});
      fs.writeFileSync(path.join(directory,'notary.p8'),process.env.MACOS_NOTARY_KEY_P8,{flag:'wx',mode:0o600});
      const password=crypto.randomBytes(32).toString('hex');
      run(['create-keychain','-p',password,keychain]);
      run(['set-keychain-settings','-lut','21600',keychain]);
      run(['unlock-keychain','-p',password,keychain]);
      run(['import',certificate,'-k',keychain,'-P',process.env.MACOS_CERTIFICATE_PASSWORD,'-T','/usr/bin/codesign']);
      run(['set-key-partition-list','-S','apple-tool:,apple:,codesign:','-s','-k',password,keychain]);
      fs.unlinkSync(certificate);
    }catch(error){cleanup();throw error;}
  } else throw Error('Expected prepare or cleanup');
  console.log('macOS signing credentials operation complete; no credential values printed.');
} catch(error){console.error(error.message);process.exitCode=1;}
