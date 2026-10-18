'use strict';
const {expect}=require('@playwright/test');

// Poll from the test process: the page's CSP must remain enabled. Playwright's
// in-page waitForFunction poller evaluates a string on subsequent animation frames.
// All callers consume only the truth value, not a JSHandle.
async function waitForPage(page,predicate,arg,options={}){
  await expect.poll(async()=>{
    const handle=await page.evaluateHandle(predicate,arg);
    try{return await handle.evaluate(value=>Boolean(value));}
    finally{await handle.dispose();}
  },{
    timeout:options.timeout??30000,
    intervals:[16,50,100,250],
    message:'Page condition did not become true under the production CSP',
  }).toBe(true);
}
module.exports={waitForPage};
