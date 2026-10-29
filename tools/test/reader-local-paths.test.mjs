import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../../assets/js/reader/render.js',import.meta.url),'utf8');
const start=source.indexOf('function resolvePath(baseDir, rel) {'),end=source.indexOf('\n\n/* 演示模式专用',start);
const resolve=vm.runInNewContext('('+source.slice(start,end)+')',{URL});
test('POSIX image and document links retain their root and decode Unicode',()=>{
 assert.equal(resolve('/work/书 稿','readmd-assets/观测%20图.svg'),'/work/书 稿/readmd-assets/观测 图.svg');
 assert.equal(resolve('/work/书 稿','../相关.md'),'/work/相关.md');
 assert.equal(resolve('/work/','readmd-assets/图.svg'),'/work/readmd-assets/图.svg');
});
test('Windows drive paths still omit only their URL drive prefix',()=>{
 assert.equal(resolve('C:\\Work\\书稿','readmd-assets/图.svg'),'C:/Work/书稿/readmd-assets/图.svg');
 assert.equal(resolve('D:/Work/','../note.md'),'D:/note.md');
});
test('UNC host and share are preserved as a local filesystem path',()=>{
 assert.equal(resolve('\\\\server\\share\\书稿','图.svg'),'//server/share/书稿/图.svg');
});
test('literal hash and question mark in a document directory are not URL delimiters',()=>{
 assert.equal(resolve('/work/chapter#1?draft','图.svg'),'/work/chapter#1?draft/图.svg');
});
test('a document without a known base does not guess a parent or filesystem root',()=>{
 assert.equal(resolve('','readmd-assets/图.svg'),'readmd-assets/图.svg');
});
