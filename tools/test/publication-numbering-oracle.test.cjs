'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const {alpha,BOUNDARIES}=require('./publication-numbering-oracle.cjs');
test('Office letter numbering crosses Z to AA, BB and ZZ to AAA',()=>{
  for(const [number,text]of BOUNDARIES)assert.equal(alpha(number),text);
  assert.equal(alpha(99),'UUUU');assert.equal(alpha(999),'K'.repeat(39));
  assert.notEqual(alpha(28),'AB');assert.notEqual(alpha(53),'BA');
});
test('the independent artifact oracle rejects nonordinal values',()=>{
  for(const number of [0,-1,1.5,NaN,Infinity,'28',null,Number.MAX_SAFE_INTEGER+1])assert.throws(()=>alpha(number));
});
