// Opt-in, offline source build. Requires the exact local upstream archive;
// never downloads, discovers Homebrew libraries, installs or publishes a pack.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {prepareMedia, inspectMediaExecutable, mediaSpecs} from './lib/media-runtime.mjs';

export const source = Object.freeze({
  version: '9.0.2',
  url: 'https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz',
  sha256: '8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e',
});
const decoders = [
  'aac','aac_fixed','aac_latm','ac3','ac3_fixed','alac','amrnb','amrwb',
  'ape','atrac1','atrac3','atrac3p','dca','eac3','flac','g723_1','g729',
  'mlp','mp1','mp1float','mp2','mp2float','mp3','mp3float','mp3adu',
  'mp3adufloat','mp3on4','mp3on4float','nellymoser','opus','qdm2','qdmc',
  'real_144','real_288','shorten','sipr','speex','truehd','tta','vorbis',
  'wavpack','wmalossless','wmapro','wmav1','wmav2','wmavoice',
  'adpcm_ima_wav','adpcm_ima_qt','adpcm_ms','adpcm_swf','adpcm_yamaha',
  'pcm_alaw','pcm_mulaw','pcm_s8','pcm_u8','pcm_s16be','pcm_s16le',
  'pcm_u16be','pcm_u16le','pcm_s24be','pcm_s24le','pcm_u24be','pcm_u24le',
  'pcm_s32be','pcm_s32le','pcm_u32be','pcm_u32le','pcm_s64be','pcm_s64le',
  'pcm_f16le','pcm_f24le','pcm_f32be','pcm_f32le','pcm_f64be','pcm_f64le',
  'pcm_s16be_planar','pcm_s16le_planar','pcm_s24le_planar','pcm_s32le_planar',
  'pcm_s8_planar','pcm_sga','pcm_vidc','pcm_zork',
];
export const configureFlags = Object.freeze([
  '--prefix=/readmd/media-ffmpeg', '--arch=aarch64', '--target-os=darwin',
  '--cc=/usr/bin/clang', '--disable-autodetect', '--disable-shared',
  '--enable-static', '--disable-network', '--disable-doc', '--disable-ffplay',
  '--disable-debug', '--disable-gpl', '--disable-nonfree', '--disable-everything',
  '--enable-pthreads', '--enable-ffmpeg', '--enable-ffprobe',
  '--enable-indev=lavfi', '--enable-protocol=file,pipe',
  '--enable-demuxer=aac,asf,avi,flac,matroska,mov,mp3,ogg,wav',
  '--enable-parser=aac,aac_latm,ac3,dca,flac,mpegaudio,opus,vorbis',
  '--enable-decoder=' + decoders.join(','),
  '--enable-encoder=pcm_s16le,aac,flac',
  '--enable-muxer=wav,pcm_s16le,adts,mp4,flac,ogg',
  '--enable-filter=aresample,atrim,asetpts,sine,anull,aformat,concat,amix,pan',
  '--extra-cflags=-arch arm64 -mmacosx-version-min=11.0 -O2',
  '--extra-ldflags=-arch arm64 -mmacosx-version-min=11.0',
]);
const hash = file => new Promise((resolve,reject) => {
  const digest=crypto.createHash('sha256'), stream=fs.createReadStream(file);
  stream.on('data',chunk=>digest.update(chunk));stream.on('error',reject);
  stream.on('end',()=>resolve(digest.digest('hex')));
});
function realFile(file) {
  const absolute=path.resolve(file), metadata=fs.lstatSync(absolute);
  if(!metadata.isFile()||metadata.isSymbolicLink()||fs.realpathSync(absolute)!==absolute)throw Error('An explicit ordinary local archive is required');
  return absolute;
}
function execute(program,args,{cwd,env,log}) {
  return new Promise((resolve,reject)=>{
    const descriptor=fs.openSync(log,'a');
    const child=spawn(program,args,{cwd,env,stdio:['ignore',descriptor,descriptor]});
    child.once('error',error=>{fs.closeSync(descriptor);reject(error);});
    child.once('exit',(code,signal)=>{fs.closeSync(descriptor);code===0?resolve():reject(Error(path.basename(program)+' failed ('+(signal||code)+'); inspect '+path.basename(log)));});
  });
}
export async function buildFFmpeg({archive,output,jobs=1}) {
  if(process.platform!=='darwin'||process.arch!=='arm64')throw Error('This source builder requires native macOS arm64');
  if(source.version!==mediaSpecs['media-ffmpeg'].version)throw Error('Source version differs from application runtime');
  if(![1,2].includes(jobs))throw Error('Use one or two build jobs');
  archive=realFile(archive);
  if(await hash(archive)!==source.sha256)throw Error('FFmpeg archive differs from the pinned upstream release');
  output=path.resolve(output);
  const parent=path.dirname(output);
  if(fs.realpathSync(parent)!==parent||!fs.statSync(parent).isDirectory())throw Error('A real output parent is required');
  if(fs.existsSync(output))throw Error('Build output already exists; preserve the prior build');
  fs.mkdirSync(output,{mode:0o700});
  for(const name of ['source','build','inputs','tmp','cache'])fs.mkdirSync(path.join(output,name));
  const env={PATH:'/usr/bin:/bin:/usr/sbin:/sbin',LANG:'C',LC_ALL:'C',
    TMPDIR:path.join(output,'tmp'),TMP:path.join(output,'tmp'),TEMP:path.join(output,'tmp'),
    XDG_CACHE_HOME:path.join(output,'cache'),CLANG_MODULE_CACHE_PATH:path.join(output,'cache','clang'),
    SDKROOT:'/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk',MACOSX_DEPLOYMENT_TARGET:'11.0'};
  const log=path.join(output,'build.log');
  await execute('/usr/bin/tar',['-xf',archive,'--strip-components=1','-C',path.join(output,'source')],{cwd:output,env,log});
  const upstream=path.join(output,'source');
  if(fs.readFileSync(path.join(upstream,'RELEASE'),'utf8').trim()!==source.version)throw Error('Unexpected upstream release identity');
  await execute('../source/configure',configureFlags,{cwd:path.join(output,'build'),env,log});
  const configuration=fs.readFileSync(path.join(output,'build','config.h'),'utf8');
  for(const setting of ['CONFIG_GPL','CONFIG_NONFREE','CONFIG_NETWORK','CONFIG_SHARED'])if(!configuration.includes('#define '+setting+' 0'))throw Error('Unexpected FFmpeg license, network or shared-library configuration');
  await execute('/usr/bin/nice',['-n','10','/usr/bin/make','-j'+jobs,'ffmpeg','ffprobe'],{cwd:path.join(output,'build'),env,log});
  const inputs=path.join(output,'inputs');
  for(const name of ['ffmpeg','ffprobe']) {
    const binary=path.join(output,'build',name);inspectMediaExecutable(binary,'macos-arm64');
    fs.copyFileSync(binary,path.join(inputs,name),fs.constants.COPYFILE_EXCL);
  }
  fs.copyFileSync(path.join(upstream,'COPYING.LGPLv2.1'),path.join(inputs,'LICENSE'),fs.constants.COPYFILE_EXCL);
  fs.copyFileSync(path.join(upstream,'LICENSE.md'),path.join(inputs,'FFMPEG-LICENSE.md'),fs.constants.COPYFILE_EXCL);
  const sourceName='ffmpeg-'+source.version+'.tar.xz';
  fs.copyFileSync(archive,path.join(output,sourceName),fs.constants.COPYFILE_EXCL);
  const compilerLog=path.join(output,'compiler.txt');
  await execute('/usr/bin/clang',['--version'],{cwd:output,env,log:compilerLog});
  const compiler=fs.readFileSync(compilerLog,'utf8').split('\n')[0];
  const recipe={schema:1,id:'media-ffmpeg',version:source.version,platform:'macos-arm64',files:[],review:{
    license:'LGPL-2.1-or-later (FFmpeg command-line programs; no external libraries, GPL or nonfree components)',
    minimumPlatform:'macOS 11.0 or later; Apple arm64',portable:true,
    redistributionApproved:false,sourceDeliveryReviewed:false,sources:[{component:'FFmpeg unmodified release source',url:source.url,sha256:source.sha256}],
    buildConfiguration:[compiler,'Darwin arm64; macOS SDK; deployment target 11.0',...configureFlags,'make -j'+jobs+' ffmpeg ffprobe'],
  }};
  fs.writeFileSync(path.join(output,'build-recipe.json'),JSON.stringify({schema:1,source,compiler,configureFlags,jobs,sourceChanges:[],networkDuringBuild:false,registered:false,published:false},null,2)+'\n',{flag:'wx'});
  fs.writeFileSync(path.join(inputs,'SOURCE'),[
    'FFmpeg '+source.version+' unmodified upstream source.',source.url,'SHA-256 '+source.sha256,
    'The exact source archive '+sourceName+' and build-recipe.json accompany this prepared directory.',
    'Rebuild offline with tools/build-media-ffmpeg-macos.mjs --archive '+sourceName+' --output NEW_DIRECTORY --jobs 1.',
    'The source archive contains the full FFmpeg command-line programs, static libraries and build scripts.',
    'No external libraries were enabled. No source modifications were made.',
    'This local QA build has not been approved for redistribution or published.\n',
  ].join('\n'),{flag:'wx'});
  fs.writeFileSync(path.join(inputs,'NOTICE'),[
    'This runtime uses FFmpeg '+source.version+' under LGPL-2.1-or-later.',
    'Audio extraction profile: AAC, ASF, AVI, FLAC, Matroska/WebM, MOV/MP4, MP3, Ogg and WAV containers;',
    'selected native audio decoders, PCM/AAC/FLAC encoders, and local file/pipe protocols.',
    'External codecs, networking, hardware acceleration, video decoding/encoding and ffplay are disabled.',
    'Full license terms are in LICENSE and licenses/FFMPEG-LICENSE.md.\n',
  ].join('\n'),{flag:'wx'});
  for(const name of ['ffmpeg','ffprobe','LICENSE','SOURCE','NOTICE','FFMPEG-LICENSE.md']) {
    const file=path.join(inputs,name);
    recipe.files.push({path:['ffmpeg','ffprobe'].includes(name)?'bin/'+name:name==='FFMPEG-LICENSE.md'?'licenses/'+name:name,
      input:name,size:fs.statSync(file).size,sha256:await hash(file),executable:['ffmpeg','ffprobe'].includes(name)});
  }
  const lock=path.join(output,'inputs.lock.json');fs.writeFileSync(lock,JSON.stringify(recipe,null,2)+'\n',{flag:'wx'});
  const prepared=await prepareMedia({lock,inputs,output:path.join(output,'prepared')});
  return {built:true,...prepared,sourceSha256:source.sha256,jobs,registered:false,published:false};
}
export async function main(args=process.argv.slice(2)) {
  const values={};
  for(let i=0;i<args.length;i+=2) {
    if(!['--archive','--output','--jobs'].includes(args[i])||!args[i+1]||args[i+1].startsWith('--')||values[args[i]])throw Error('Expected --archive LOCAL_TAR_XZ --output NEW_DIRECTORY [--jobs 1|2]');
    values[args[i]]=args[i+1];
  }
  if(!values['--archive']||!values['--output'])throw Error('An explicit source archive and new output directory are required');
  return buildFFmpeg({archive:values['--archive'],output:values['--output'],jobs:values['--jobs']===undefined?1:Number(values['--jobs'])});
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main().then(result=>console.log(JSON.stringify(result)),error=>{console.error('Offline FFmpeg build failed: '+error.message);process.exitCode=1;});
