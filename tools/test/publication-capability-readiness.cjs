'use strict';
const assert=require('node:assert/strict');
// Readiness is a state observation, not an operation retry. Only the explicit
// installed+busy state may be observed again; errors/missing/corruption fail.
async function waitCapabilityReady({id,request,observe,now=Date.now,sleep=ms=>new Promise(r=>setTimeout(r,ms)),budget=120000}){
  const started=now(),busyCode={fonts:'capability_install_busy',pdfium:'pdf_busy'}[id];assert(busyCode);
  for(;;){const remaining=budget-(now()-started);assert(remaining>0,'Capability readiness deadline exhausted');
    const response=await request(remaining),elapsedMs=now()-started;observe({id,elapsedMs,response});
    assert(elapsedMs<=budget,'Capability readiness exceeded its total deadline');assert.equal(response.status,200);assert.equal(response.data.ok,true);assert.equal(response.data.installed,true);
    if(response.data.ready){assert.equal(response.data.busy,false);return;}
    assert.equal(response.data.busy,true,'Installed capability is not usable');assert.equal(response.data.busy_code,busyCode);
    await sleep(Math.min(100,Math.max(0,budget-(now()-started))));
  }
}
module.exports={waitCapabilityReady};
