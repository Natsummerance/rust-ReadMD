'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const {artifactAssertion, transferAssertion, independentStep, requireNoFailures} = require('./publication-independent-validation.cjs');
const {root,sha256,failureScreenshotPath,captureFailureScreenshot,stopAfterInventory}=require('./publication-batch3-harness.cjs');
async function privacyControls() {
  let controls=0;
  const sentinel='PRIVATE_ORIGINAL_RESULT_SENTINEL',expected='PRIVATE_EXPECTED_RESULT_SENTINEL';
  let original;
  try { artifactAssertion(()=>assert.deepEqual({text:sentinel},{text:expected})); } catch(error) { original=error; }
  assert(String(original.stack).includes(sentinel)&&String(original.stack).includes(expected),'Negative control must actually expose the original stack');
  const publicError=transferAssertion(original,Error('Original fixed display failed; private details retained by SHA256'));
  const retained=[];
  await independentStep(async(label,work)=>work(),'private stack',()=>{throw publicError;},async(label,error)=>retained.push({label,error:String(error.stack||error)}));
  assert(!JSON.stringify(retained).includes(sentinel)&&!JSON.stringify(retained).includes(expected));
  assert(String(original.stack).includes(sentinel),'Original private failure must remain intact');
  assert.throws(()=>requireNoFailures(retained),/validation failures remain/);controls++;
  const transport=Error('private API failure '+sentinel),safeTransport=transferAssertion(transport,Error('Private API request failed; original retained privately'));
  await assert.rejects(independentStep(async(label,work)=>work(),'fatal API',()=>{throw safeTransport;},()=>assert.fail('API failure was treated as an independent artifact failure')),error=>error===safeTransport);controls++;

  const evidence=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/publication-privacy-controls-'));
  // A small owned fixture tree, not an actual native profile or private oracle.
  const privateDirectory=path.join(evidence,'.cache/private/screenshots'),data=path.join(evidence,'.cache/capability-upgrade/public-output');
  fs.mkdirSync(privateDirectory,{recursive:true});fs.mkdirSync(data,{recursive:true});
  const bytes=Buffer.from('SYNTHETIC SCREENSHOT '+sentinel);let captures=0;
  const page={async screenshot(options){assert.equal(options,undefined,'Use returned bytes so the writer can enforce wx');captures++;return bytes;}};
  const screenshot=await captureFailureScreenshot(page,evidence,data,privateDirectory);
  assert.equal(screenshot.privateDerived,true);assert.equal(screenshot.sha256,sha256(bytes));
  assert.equal(fs.readFileSync(path.join(evidence,screenshot.path)).toString(),bytes.toString());
  assert(!fs.existsSync(path.join(data,'failure.png')));controls++;
  const first=await captureFailureScreenshot(page,evidence,data,privateDirectory,'first-failure.png');
  assert.equal(first.privateDerived,true);assert(!fs.existsSync(path.join(data,'first-failure.png')));controls++;
  for(const directory of [data,path.join(privateDirectory,'..','..','..','outside'),'relative/private']) {
    const before=captures;
    await assert.rejects(captureFailureScreenshot(page,evidence,data,directory),/private|absolute/i);
    assert.equal(captures,before,'Invalid private policy captured before validation');
    assert(!fs.existsSync(path.join(data,'failure.png')));controls++;
  }
  await assert.rejects(captureFailureScreenshot(page,evidence,data,privateDirectory,'../failure.png'),/filename/);controls++;
  await assert.rejects(captureFailureScreenshot(page,evidence,data,privateDirectory),error=>error.code==='EEXIST');
  assert.equal(sha256(fs.readFileSync(path.join(privateDirectory,'failure.png'))),screenshot.sha256,'Original screenshot was overwritten');
  assert(!fs.existsSync(path.join(data,'failure.png')));controls++;
  assert.equal(failureScreenshotPath(evidence,data),path.join(data,'failure.png'),'Other suites retain their explicit default public destination');controls++;

  const cim=Error('CIM inventory failed'),stopError=Error('Owned host failed to stop'),order=[];
  const stopped=await stopAfterInventory(()=>{order.push('inventory');throw cim;},()=>{order.push('stop');});
  assert.deepEqual(order,['inventory','stop']);assert.equal(stopped.inventoryError,cim);assert.equal(stopped.stopError,undefined);controls++;
  const both=await stopAfterInventory(()=>{throw cim;},()=>{throw stopError;});
  assert.equal(both.inventoryError,cim);assert.equal(both.stopError,stopError);controls++;
  const failedStop=await stopAfterInventory(()=>{},()=>{throw stopError;});
  assert.equal(failedStop.inventoryError,undefined);assert.equal(failedStop.stopError,stopError);controls++;
  // The native finally keeps the product failure primary and records cleanup
  // separately. Even a failed inventory cannot turn the run into success.
  let primary=publicError,postVerificationError;
  try { if(both.inventoryError||both.stopError)throw both.inventoryError||both.stopError; }
  catch(error){postVerificationError=error;if(!primary)throw error;}
  assert.equal(primary,publicError);assert.equal(postVerificationError,cim);
  assert(!String(primary.stack).includes(sentinel));assert(String(original.stack).includes(sentinel));controls++;
  return {controls,evidence:path.relative(root,evidence).replaceAll(path.sep,'/')};
}
async function run() {
  const checks = [], failures = [];
  const step = async (label, work) => { try { const value = await work(); checks.push({label, passed:true}); return value; } catch(error) { checks.push({label, passed:false}); throw error; } };
  const retain = async (label,error) => failures.push({label,error});
  await independentStep(step, 'bad geometry', () => artifactAssertion(() => assert.equal(2,1)), retain);
  assert.equal(await independentStep(step, 'next format', () => 7, retain), 7);
  assert.deepEqual(checks.map(c=>c.passed),[false,true]);
  assert.equal(failures.length,1);
  assert.throws(()=>requireNoFailures(failures),/validation failures remain/);
  for(const error of [Error('API rejected'),Error('timeout'),new assert.AssertionError({message:'source changed'})]) {
    await assert.rejects(independentStep(step,'fatal',()=>{throw error;},retain),e=>e===error);
  }
  await assert.rejects(independentStep(step,'retention failed',()=>artifactAssertion(()=>assert.fail('layout')),async()=>{throw Error('source identity changed');}),/source identity changed/);
  let secret;
  try { artifactAssertion(()=>assert.fail('private expected content')); } catch(error) { secret=error; }
  const publicError=transferAssertion(secret,Error('private result retained by hash'));
  await independentStep(step,'private case',()=>{throw publicError;},retain);
  assert.equal(failures[1].error.message,'private result retained by hash');
  assert(!JSON.stringify(failures.map(f=>({label:f.label,error:String(f.error.stack||f.error)}))).includes('private expected content'));
  requireNoFailures([]);
  const privacy=await privacyControls();
  console.log(JSON.stringify({passed:true,controls:8+privacy.controls,independentControls:8,privacyControls:privacy.controls,productInvocations:0,evidence:privacy.evidence}));
}
if(require.main===module)run().catch(e=>{console.error(e);process.exitCode=1;});
module.exports={run};
