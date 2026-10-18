'use strict';
// Assemble the same real art/model/runtime layout used by xtask pet-package.
// build-renderer compiles scripts; its successful exit alone does not mean
// the complete optional desktop extension is usable.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const root=path.resolve(__dirname,'..');
const argument=process.argv.indexOf('--output');
assert.ok(argument>=0&&process.argv[argument+1],'--output <owned fixture directory> required');
const output=path.resolve(process.argv[argument+1]);
assert.ok(output.startsWith(path.join(root,'.cache')+path.sep),'Fixture must be inside this workspace cache');
assert.ok(!fs.existsSync(output),'Use a new fixture directory; existing fixtures are preserved');
const core=process.env.READMD_PET_CUBISM_CORE||path.join(root,'packages/readmd-hermes-pet-adapter/dist/vendor/live2dcubismcore.min.js');
const coreBytes=fs.readFileSync(core);
assert.equal(crypto.createHash('sha256').update(coreBytes).digest('hex'),'25ae938cb4fe282ce189b357bcc97e603d1e1f7ec78bf04150d401c23cdc792f','Pinned Cubism Core must be prepared offline');
execFileSync(process.execPath,[path.join(root,'packages/readmd-pet-rust/scripts/build-renderer.mjs'),'--renderer-cache',path.join(root,'packages/readmd-pet-rust/runtime-assets/renderer'),'--output',path.join(output,'renderer')],{cwd:root,windowsHide:true,stdio:'pipe'});
for(const name of ['mochi','hermes','amber','moss'])fs.copyFileSync(path.join(root,'assets/pet',name+'-sprite.png'),path.join(output,'renderer/assets',name+'-sprite.png'));
for(const name of ['cache-capy','niu-lai'])fs.copyFileSync(path.join(root,'assets/pet',name,'spritesheet.webp'),path.join(output,'renderer/assets',name+'-sprite.webp'));
fs.cpSync(path.join(root,'assets/pet/model'),path.join(output,'models/arch-chan'),{recursive:true});
fs.mkdirSync(path.join(output,'vendor'));fs.writeFileSync(path.join(output,'vendor/live2dcubismcore.min.js'),coreBytes);
const files=[];
function visit(directory){for(const entry of fs.readdirSync(directory,{withFileTypes:true})){const file=path.join(directory,entry.name);if(entry.isDirectory())visit(file);else files.push({path:path.relative(output,file).replaceAll('\\','/'),size:fs.statSync(file).size,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')});}}
visit(output);fs.writeFileSync(path.join(output,'fixture.json'),JSON.stringify({schema:1,offline:true,syntheticArt:false,corePinned:true,files},null,2));
console.log(JSON.stringify({renderer:path.relative(root,path.join(output,'renderer')),files:files.length,offline:true}));
