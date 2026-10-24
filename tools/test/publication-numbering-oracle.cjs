'use strict';
// Independent Office letter-numbering oracle shared by the artifact readers.
// Primary implementation: LibreOffice ConversionHelper.cxx maps upperLetter /
// lowerLetter to CHARS_*_LETTER_N; defaultnumberingprovider.cxx formats those
// with lcl_formatChars1. ww8par5.cxx uses the same type for ALPHABETIC fields.
const assert=require('node:assert/strict');
const BOUNDARIES=Object.freeze([[1,'A'],[26,'Z'],[27,'AA'],[28,'BB'],[49,'WW'],[52,'ZZ'],[53,'AAA'],[54,'BBB'],[78,'ZZZ'],[79,'AAAA']].map(Object.freeze));
function alpha(number){
  assert(Number.isSafeInteger(number)&&number>=1,'Expected a positive numbering ordinal');
  return String.fromCharCode(65+(number-1)%26).repeat(Math.ceil(number/26));
}
module.exports={alpha,BOUNDARIES};
