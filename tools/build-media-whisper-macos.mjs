// Explicit offline, CPU-only source build. All source/model/tool inputs are
// local arguments. Does not download, install, load a model or publish.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {prepareMedia,inspectMediaExecutable,mediaSpecs} from './lib/media-runtime.mjs';

export const source=Object.freeze({version:'1.9.5',commit:'d1be6fde11ac6e0407606b4e42fe72d34add8037',
  url:'https://codeload.github.com/ggml-org/whisper.cpp/tar.gz/d1be6fde11ac6e0407606b4e42fe72d34add8037',
  sha256:'645445da884df6dbd52b30f82d7be7c01d1767c938714e477e786b88d529e9f4'});
export const modelLicense=Object.freeze({url:'https://raw.githubusercontent.com/openai/whisper/86098128c0b4f24f0e2aa2994de830614b474227/LICENSE',sha256:'b5d65a59060e68c4ff940e1eddfa6f94b2d68fdf58ed7f4dd57721c997e35e9d'});
export const configureFlags=Object.freeze([
  '-G','Unix Makefiles','-DCMAKE_BUILD_TYPE=Release','-DCMAKE_MAKE_PROGRAM=/usr/bin/make',
  '-DCMAKE_C_COMPILER=/usr/bin/clang','-DCMAKE_CXX_COMPILER=/usr/bin/clang++',
  '-DCMAKE_OSX_ARCHITECTURES=arm64','-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0',
  '-DCMAKE_OSX_SYSROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk',
  '-DCMAKE_FIND_USE_PACKAGE_REGISTRY=OFF','-DCMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=OFF',
  '-DFETCHCONTENT_FULLY_DISCONNECTED=ON','-DBUILD_SHARED_LIBS=OFF',
  '-DWHISPER_BUILD_IS_DEV=OFF','-DWHISPER_BUILD_TESTS=OFF','-DWHISPER_BUILD_SERVER=OFF',
  '-DWHISPER_BUILD_EXAMPLES=ON','-DWHISPER_BUILD_NUMBER=0','-DWHISPER_BUILD_COMMIT=d1be6fde',
  '-DWHISPER_CURL=OFF','-DWHISPER_SDL2=OFF','-DWHISPER_COREML=OFF',
  '-DWHISPER_OPENVINO=OFF','-DWHISPER_USE_SYSTEM_GGML=OFF','-DWHISPER_USE_SYSTEM_LLAMA=OFF',
  '-DGGML_NATIVE=OFF','-DGGML_CPU_ARM_ARCH=armv8-a','-DGGML_BACKEND_DL=OFF',
  '-DGGML_CCACHE=OFF','-DGGML_OPENMP=OFF','-DGGML_ACCELERATE=OFF','-DGGML_BLAS=OFF',
  '-DGGML_METAL=OFF','-DGGML_CUDA=OFF','-DGGML_VULKAN=OFF','-DGGML_SYCL=OFF',
  '-DGGML_RPC=OFF','-DGGML_LLAMAFILE=OFF','-DGGML_CPU_KLEIDIAI=OFF',
]);
const hash=async file=>{const digest=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file))digest.update(chunk);return digest.digest('hex');};
function realFile(file) {file=path.resolve(file);const metadata=fs.lstatSync(file);if(!metadata.isFile()||metadata.isSymbolicLink()||fs.realpathSync(file)!==file)throw Error('Explicit regular local inputs are required');return file;}
function execute(program,args,{cwd,env,log}) {return new Promise((resolve,reject)=>{const descriptor=fs.openSync(log,'a'),child=spawn(program,args,{cwd,env,stdio:['ignore',descriptor,descriptor]});child.once('error',error=>{fs.closeSync(descriptor);reject(error);});child.once('exit',(code,signal)=>{fs.closeSync(descriptor);code===0?resolve():reject(Error(path.basename(program)+' failed ('+(signal||code)+'); inspect '+path.basename(log)));});});}
export async function buildWhisper({archive,model,license,cmake,output,jobs=1}) {
  if(process.platform!=='darwin'||process.arch!=='arm64')throw Error('This source builder requires native macOS arm64');
  if(![1,2].includes(jobs))throw Error('Use one or two build jobs');
  if(source.version!==mediaSpecs.whisper.version)throw Error('Source version differs from application runtime');
  archive=realFile(archive);model=realFile(model);license=realFile(license);cmake=realFile(cmake);
  if(await hash(archive)!==source.sha256)throw Error('Whisper source archive differs from the pinned upstream commit');
  if(fs.statSync(model).size!==mediaSpecs.whisper.model.size||await hash(model)!==mediaSpecs.whisper.model.sha256)throw Error('Whisper model differs from the application-pinned object');
  if(await hash(license)!==modelLicense.sha256)throw Error('Model license differs from the pinned upstream notice');
  output=path.resolve(output);const parent=path.dirname(output);
  if(fs.realpathSync(parent)!==parent||!fs.statSync(parent).isDirectory())throw Error('A real output parent is required');
  if(fs.existsSync(output))throw Error('Build output already exists; preserve the prior build');
  fs.mkdirSync(output,{mode:0o700});for(const name of ['source','build','inputs','tmp','cache'])fs.mkdirSync(path.join(output,name));
  const env={PATH:'/usr/bin:/bin:/usr/sbin:/sbin',LANG:'C',LC_ALL:'C',
    TMPDIR:path.join(output,'tmp'),TMP:path.join(output,'tmp'),TEMP:path.join(output,'tmp'),
    XDG_CACHE_HOME:path.join(output,'cache'),CLANG_MODULE_CACHE_PATH:path.join(output,'cache','clang'),
    SDKROOT:'/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk',MACOSX_DEPLOYMENT_TARGET:'11.0'};
  const log=path.join(output,'build.log'),upstream=path.join(output,'source');
  await execute('/usr/bin/tar',['-xf',archive,'--strip-components=1','-C',upstream],{cwd:output,env,log});
  const flags=['-S','../source','-B','.',...configureFlags,
    '-DCMAKE_C_FLAGS_RELEASE=-O2 -DNDEBUG -ffile-prefix-map='+output+'=/readmd/whisper-build',
    '-DCMAKE_CXX_FLAGS_RELEASE=-O2 -DNDEBUG -ffile-prefix-map='+output+'=/readmd/whisper-build'];
  await execute(cmake,flags,{cwd:path.join(output,'build'),env,log});
  await execute('/usr/bin/nice',['-n','10',cmake,'--build','.','--target','whisper-cli','--parallel',String(jobs)],{cwd:path.join(output,'build'),env,log});
  const binary=path.join(output,'build','bin','whisper-cli');inspectMediaExecutable(binary,'macos-arm64');
  if(fs.readFileSync(binary).includes(Buffer.from('/Users/')))throw Error('Private source paths remain in the Whisper executable');
  const inputs=path.join(output,'inputs');fs.copyFileSync(binary,path.join(inputs,'whisper-cli'),fs.constants.COPYFILE_EXCL);
  fs.copyFileSync(model,path.join(inputs,'ggml-base-q5_1.bin'),fs.constants.COPYFILE_EXCL);
  fs.copyFileSync(path.join(upstream,'LICENSE'),path.join(inputs,'LICENSE'),fs.constants.COPYFILE_EXCL);
  // This upstream archive places bundled GGML under its repository-root MIT
  // notice, rather than shipping a separate ggml/LICENSE file.
  fs.copyFileSync(path.join(upstream,'LICENSE'),path.join(inputs,'GGML.txt'),fs.constants.COPYFILE_EXCL);
  // Preserve the complete bundled reader notice, including both offered
  // alternatives. The full unchanged sources accompany the prepared pack.
  const notices=[];
  for(const [relative,name] of [['examples/miniaudio.h','MINIAUDIO.txt'],['examples/stb_vorbis.c','STB-VORBIS.txt']]){
    const text=fs.readFileSync(path.join(upstream,relative),'utf8'),start=text.lastIndexOf('This software is available');
    if(start<0)throw Error('Bundled audio-reader notice changed');
    fs.writeFileSync(path.join(inputs,name),'Upstream '+relative+' at '+source.commit+'\n\n'+text.slice(start),{flag:'wx'});notices.push(name);
  }
  fs.writeFileSync(path.join(inputs,'WHISPER-MODEL.txt'),[
    'OpenAI Whisper base model converted/quantized for whisper.cpp by Georgi Gerganov.',
    'Exact upstream object: '+mediaSpecs.whisper.model.source,
    'models/ggml-base-q5_1.bin SHA-256 '+mediaSpecs.whisper.model.sha256,
    'Model card identifies MIT: https://huggingface.co/ggerganov/whisper.cpp/raw/5359861c739e955e79d9a303bcbc70fb988958b1/README.md',
    'Original model license: '+modelLicense.url,'',fs.readFileSync(license,'utf8'),
  ].join('\n'),{flag:'wx'});
  const sourceName='whisper-'+source.commit+'.tar.gz';fs.copyFileSync(archive,path.join(output,sourceName),fs.constants.COPYFILE_EXCL);
  const compilerLog=path.join(output,'compiler.txt'),cmakeLog=path.join(output,'cmake.txt');
  await execute('/usr/bin/clang',['--version'],{cwd:output,env,log:compilerLog});await execute(cmake,['--version'],{cwd:output,env,log:cmakeLog});
  const compiler=fs.readFileSync(compilerLog,'utf8').split('\n')[0],cmakeVersion=fs.readFileSync(cmakeLog,'utf8').split('\n')[0];
  const publicFlags=flags.map(flag=>flag.replaceAll(output,'/readmd/whisper-build'));
  fs.writeFileSync(path.join(output,'build-recipe.json'),JSON.stringify({schema:1,source,compiler,cmakeVersion,configureFlags:publicFlags,jobs,sourceChanges:[],networkDuringBuild:false,modelLoadedDuringBuild:false,published:false},null,2)+'\n',{flag:'wx'});
  fs.writeFileSync(path.join(inputs,'SOURCE'),[
    'whisper.cpp '+source.version+' unmodified source commit '+source.commit,source.url,'Source SHA-256 '+source.sha256,
    'The exact source archive '+sourceName+' and build-recipe.json accompany this prepared directory.',
    'The fixed ggml-base-q5_1.bin model and its MIT notice are included in the pack.',
    'CPU-only, static GGML/Whisper libraries, no external runtimes, no GPU, no OpenMP, no download support.',
    'No source modifications were made. This local QA build is not approved for redistribution or published.\n',
  ].join('\n'),{flag:'wx'});
  fs.writeFileSync(path.join(inputs,'NOTICE'),'whisper.cpp, bundled GGML, stb_vorbis and OpenAI Whisper model: MIT. miniaudio: MIT No Attribution. Complete bundled audio-reader alternatives are in licenses/.\nCPU-only arm64 build; networking, GPU, dynamic backends, system GGML and OpenMP are disabled.\n',{flag:'wx'});
  const recipe={schema:1,id:'whisper',version:source.version,platform:'macos-arm64',files:[],review:{
    license:'MIT (whisper.cpp, GGML, OpenAI Whisper model, stb_vorbis); MIT-0 (miniaudio)',minimumPlatform:'macOS 11.0 or later; Apple arm64',portable:true,redistributionApproved:false,sourceDeliveryReviewed:false,
    sources:[{component:'whisper.cpp and bundled GGML source',url:source.url,sha256:source.sha256},{component:'Whisper ggml-base-q5_1 model',url:'https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base-q5_1.bin',sha256:mediaSpecs.whisper.model.sha256},{component:'OpenAI Whisper model license',...modelLicense}],
    buildConfiguration:[compiler,cmakeVersion,...publicFlags,'cmake --build . --target whisper-cli --parallel '+jobs],
  }};
  for(const name of ['whisper-cli','ggml-base-q5_1.bin','LICENSE','SOURCE','NOTICE','WHISPER-MODEL.txt','GGML.txt',...notices]){
    const file=path.join(inputs,name),target=name==='whisper-cli'?'bin/'+name:name==='ggml-base-q5_1.bin'?'models/'+name:['LICENSE','SOURCE','NOTICE'].includes(name)?name:'licenses/'+name;
    recipe.files.push({path:target,input:name,size:fs.statSync(file).size,sha256:await hash(file),executable:name==='whisper-cli'});
  }
  const lock=path.join(output,'inputs.lock.json');fs.writeFileSync(lock,JSON.stringify(recipe,null,2)+'\n',{flag:'wx'});
  const prepared=await prepareMedia({lock,inputs,output:path.join(output,'prepared')});
  return {built:true,...prepared,sourceSha256:source.sha256,jobs,modelLoaded:false,registered:false,published:false};
}
export async function main(args=process.argv.slice(2)) {
  const values={},allowed=['--archive','--model','--model-license','--cmake','--output','--jobs'];
  for(let at=0;at<args.length;at+=2){if(!allowed.includes(args[at])||!args[at+1]||args[at+1].startsWith('--')||values[args[at]])throw Error('Expected explicit --archive, --model, --model-license, --cmake, --output and optional --jobs 1|2');values[args[at]]=args[at+1];}
  if(allowed.slice(0,5).some(name=>!values[name]))throw Error('All source, model, license, CMake and output inputs are required');
  return buildWhisper({archive:values['--archive'],model:values['--model'],license:values['--model-license'],cmake:values['--cmake'],output:values['--output'],jobs:values['--jobs']===undefined?1:Number(values['--jobs'])});
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main().then(result=>console.log(JSON.stringify(result)),error=>{console.error('Offline Whisper build failed: '+error.message);process.exitCode=1;});
