import test from 'node:test';import assert from 'node:assert/strict';import {validateResvgInventory} from '../prepare-linux-svg-worker.mjs';
const ordinary=['resvg-0.48.1/Cargo.toml','resvg-0.48.1/README.md'],types=['-rw-r--r-- root/root resvg-0.48.1/Cargo.toml','-rw-r--r-- root/root resvg-0.48.1/README.md'];
test('pinned SVG source permits only ordinary files and its documented README link',()=>{
 assert.doesNotThrow(()=>validateResvgInventory(ordinary,types));
 assert.doesNotThrow(()=>validateResvgInventory([...ordinary,'resvg-0.48.1/crates/resvg/README.md'],[...types,'lrwxrwxrwx root/root resvg-0.48.1/crates/resvg/README.md -> ../../README.md']));
});
test('SVG source extraction refuses traversal, an external link and a hard link',()=>{
 for(const name of ['../escape','/escape','resvg-0.48.1/../escape','resvg-0.48.1/a\\escape'])assert.throws(()=>validateResvgInventory([name],[types[0]]));
 for(const link of ['lrwxrwxrwx root/root resvg-0.48.1/crates/resvg/README.md -> /etc/passwd','lrwxrwxrwx root/root resvg-0.48.1/LICENSE -> ../../escape','hrw-r--r-- root/root resvg-0.48.1/LICENSE link to /escape'])assert.throws(()=>validateResvgInventory([ordinary[0]],[link]));
 assert.throws(()=>validateResvgInventory(ordinary,[types[0]]));
});
