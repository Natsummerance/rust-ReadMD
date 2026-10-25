// Give native debug/panic metadata stable paths, including vendored workers.
// Encoded flags preserve spaces without invoking a shell or exposing paths.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export function nativeBuildFlags({workspace,home,cargoHome,rustupHome,platform=process.platform,existing=[]}) {
  const api=platform==='win32'?path.win32:path.posix;
  if(!Array.isArray(existing)||existing.some(v=>typeof v!=='string'||/[\r\n\0\x1f]/.test(v)))throw Error('Invalid native build flags');
  const mappings=[[home,'/readmd/home'],[workspace,'/readmd/source'],[cargoHome,'/readmd/cargo'],[rustupHome,'/readmd/rustup']];
  const flags=[...existing];
  for(const [source,target] of mappings) {
    if(typeof source!=='string'||!api.isAbsolute(source)||/[\r\n\0\x1f]/.test(source))throw Error('Native build roots must be absolute paths');
    const normalized=api.normalize(source);
    const spellings=platform==='win32'?[normalized,normalized.replaceAll('\\','/')]:[normalized];
    for(const spelling of new Set(spellings))flags.push('--remap-path-prefix='+spelling+'='+target);
  }
  return flags;
}

export function nativeBuildEnvironment(env=process.env,workspace=process.cwd()) {
  if(env.RUSTFLAGS?.trim()&&!env.CARGO_ENCODED_RUSTFLAGS)throw Error('Use encoded native flags to preserve argument boundaries');
  const home=os.homedir();
  const flags=nativeBuildFlags({workspace:path.resolve(workspace),home,cargoHome:env.CARGO_HOME||path.join(home,'.cargo'),rustupHome:env.RUSTUP_HOME||path.join(home,'.rustup'),existing:env.CARGO_ENCODED_RUSTFLAGS?env.CARGO_ENCODED_RUSTFLAGS.split('\x1f'):[]});
  const result={...env,CARGO_ENCODED_RUSTFLAGS:flags.join('\x1f')};delete result.RUSTFLAGS;return result;
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  if(process.argv.length!==3||process.argv[2]!=='--github-env'||!process.env.GITHUB_ENV)throw Error('Expected --github-env in a GitHub Actions step');
  fs.appendFileSync(process.env.GITHUB_ENV,'CARGO_ENCODED_RUSTFLAGS='+nativeBuildEnvironment().CARGO_ENCODED_RUSTFLAGS+'\n');
  console.log('Native source and toolchain path remapping enabled');
}
