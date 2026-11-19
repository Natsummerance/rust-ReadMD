'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {randomUUID} = require('node:crypto');
const {startKernel} = require('../tools/test/v006-refactor-host.cjs');

module.exports = async function recover({file,profile,documents,id,pid}, {binary,output,states,physical}) {
  const tmp=fs.mkdtempSync(path.join(output,'reopen-'));
  Object.assign(process.env,{TMPDIR:tmp,TMP:tmp,TEMP:tmp});
  try {
    const server=await startKernel(binary,profile,documents);
    try {
      assert.notEqual(server.pid,pid);
      const call=async (route,body)=>{
        const response=await server.api(route,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{});
        assert.equal(response.status,200);return response.json();
      };
      await call('/api/file?p='+encodeURIComponent(file));
      const resume=await call('/api/review',{action:'resume',path:file});
      let head=resume.snapshot;
      assert.equal(head.document_id,id); assert.equal(head.content,states.at(-1));
      for(const [action,sequence] of [['undo',states.slice(0,-1).reverse()],['redo',states.slice(1)]]) {
        for (const expected of sequence) {
          head=await call('/api/review',{action,document_id:id,generation:head.generation,review_revision:head.review_revision,call_id:randomUUID()});
          assert.equal(head.content,expected,action);assert.deepEqual(fs.readFileSync(file),physical(states.at(-1)));
        }
      }
      return {undo:states.length-1,redo:states.length-1};
    } finally { await server.stop(); }
  } finally { fs.rmdirSync(tmp); }
};
