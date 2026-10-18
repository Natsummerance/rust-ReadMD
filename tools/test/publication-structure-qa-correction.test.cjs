'use strict';
// Pure QA controls. Self-authored material only; no host or product invocation.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),Module=require('node:module');
const repo=process.cwd(),sourceRoot=process.env.READMD_QA_SOURCE_ROOT||repo;
const checks=require(path.join(sourceRoot,'tools/test/publication-structure-checks.cjs'));
const spatial=require(path.join(sourceRoot,'tools/test/publication-structure-spatial.cjs'));
const runnerPath=path.join(repo,'ui-tests/publication-structure-native.cjs');
const runnerModule=new Module(runnerPath,module);runnerModule.filename=runnerPath;runnerModule.paths=Module._nodeModulePaths(path.dirname(runnerPath));
runnerModule._compile(fs.readFileSync(path.join(sourceRoot,'ui-tests/publication-structure-native.cjs'),'utf8'),runnerPath);
const runner=runnerModule.exports,results=[];
const yes=(name,work)=>{work();results.push({name,accepted:true,expected:true});};
const no=(name,work)=>{assert.throws(work,assert.AssertionError);results.push({name,accepted:false,expected:false});};
const inline=text=>({type:'inline',runs:[{text,marks:{},href:null}]});let identity=0;
const chars=(text,x,y,size=11)=>[...text].map((c,i)=>({id:identity++,page:1,c,size,font:'Original QA font',origin:[x+3.4*i,y],bbox:[x+3.4*i,y-8,x+3.4*i+2.5,y+2]}));
const boundary='READMDQABIBEND012345ABCDEF',left=18*72/25.4;
const fixture={id:'self-authored mixed-style note',boundaryMarker:boundary,expectedNotes:[{number:1,segments:['AB, CD']}],expectedLayout:[{type:'block',indent:false,children:[inline('BODY')]}]};
function noteActual(){
  const parts=[['References',20,11,left],['BODY',40,11,left],[boundary,60,11,left],['1',710,5.4,left],['AB',714,9,left+10],[', CD',714,9,left+16.8]];
  const characters=parts.flatMap(([text,y,size,x])=>chars(text,x,y,size));
  const spans=parts.map(([text,y,size,x])=>({text,size,origin:[x,y],bbox:[x,y-8,x+text.length*3.4,y+2]}));
  // A real MuPDF copy span may inherit the adjacent superscript's origin;
  // the actual body characters/glyphs retain their common paragraph baseline.
  spans[4].origin[1]=710;
  return{kind:'pdf',text:'References\nBODY\n'+boundary+'\n1\nAB, CD',pypdfText:'References\nBODY\n'+boundary+'\n1\nAB, CD',pages:[{width:595,height:842,spans,characters,drawings:[{width:.5,bbox:[left,700,left+100,700]}],glyphTraces:[{size:11,chars:characters.map(c=>({...c,unicode:c.c.codePointAt(0),glyphId:c.id+1}))}]}]};
}
yes('superscript-inherited span origin does not insert a false note-body newline',()=>checks.displayPdfRegions(noteActual(),fixture,{engine:'typst'}));
no('true body baseline change before punctuation remains rejected',()=>{const value=noteActual();for(const c of value.pages[0].characters)if(c.origin[0]>=left+16.8&&c.origin[1]===714){c.origin[1]+=9;c.bbox[1]+=9;c.bbox[3]+=9;}checks.displayPdfRegions(value,fixture,{engine:'typst'});});
no('actual word space cannot disappear from the note',()=>{const value=noteActual();value.pages[0].characters=value.pages[0].characters.filter(c=>c.c!==' ');checks.displayPdfRegions(value,fixture,{engine:'typst'});});
no('changing the actual note label remains rejected',()=>{const value=noteActual();value.pages[0].characters.find(c=>c.origin[1]===710).c='2';checks.displayPdfRegions(value,fixture,{engine:'typst'});});
function tailActual(){const input=chars('AB ',92.7,20),space=input.at(-1);space.bbox[2]=space.origin[0]+2.75;space.emptySpaceOutline={verified:true,fontXref:1,fontSha256:'a'.repeat(64),outlineSha256:'b'.repeat(64),advanceEm:.25,glyphId:3,traceSequence:1,origin:[...space.origin],size:11,helperSha256:'667fa0a83b29ce38b0ec2d432441ccd8a149b60e24d1d6d4045772b43b815ff9'};return input;}
const prove=input=>spatial.proveFlow([inline('AB ')],input,{left:0,right:100},{assertExact:checks.assertExactSegments,fontSize:11,requiredWhitespaceIds:[input.at(-1).id]});
yes('verified empty-outline line-tail space keeps its original text and identity',()=>{const input=tailActual(),result=prove(input);assert.deepEqual(result.proof.whitespaceIds,[input.at(-1).id]);});
no('an unverified tail-space outline cannot bypass the cell boundary',()=>{const input=tailActual();delete input.at(-1).emptySpaceOutline;prove(input);});
no('an actual nonempty space outline cannot bypass the cell boundary',()=>{const input=tailActual();input.at(-1).emptySpaceOutline.verified=false;prove(input);});
no('advance inconsistent with the actual embedded space remains rejected',()=>{const input=tailActual();input.at(-1).emptySpaceOutline.advanceEm=.5;prove(input);});
no('moving a visible glyph outside the original .75pt limit remains rejected',()=>{const input=tailActual();input[0].bbox[2]=101;prove(input);});
no('moving even an empty-space origin outside its cell remains rejected',()=>{const input=tailActual(),c=input.at(-1);c.origin[0]=101;c.bbox[0]=101;c.bbox[2]=103.75;c.emptySpaceOutline.origin[0]=101;prove(input);});
no('an actual trace tail-space ID cannot be dropped from the partition',()=>{const input=tailActual(),id=input.at(-1).id;spatial.proveFlow([inline('AB ')],input.slice(0,-1),{left:0,right:100},{assertExact:checks.assertExactSegments,fontSize:11,requiredWhitespaceIds:[id]});});
no('ordinary interior whitespace has no tail-space exception',()=>{const input=tailActual();input.push(...chars('C',99.7,20));prove(input);});
const original=require(path.join(repo,'tools/test/publication-structure-fixtures.cjs')).originalCases.find(c=>c.id==='Multipair');
const prepared=require(path.join(repo,'tools/test/publication-structure-case-adapter.cjs')).prepareCase(original,{root:repo});
yes('prepared source keeps the owned boundary, BOM and CRLF exactly',()=>assert.equal(runner.assertPreparedReaderInput(prepared,Buffer.from(prepared.source)),prepared.source.replace(/^\uFEFF/u,'')));
no('unprepared source cannot be mistaken for the actual opened prepared file',()=>runner.assertPreparedReaderInput(prepared,Buffer.from(original.source)));
no('dropping the original on-disk BOM remains rejected',()=>runner.assertPreparedReaderInput(prepared,Buffer.from(prepared.source.replace(/^\uFEFF/u,''))));
no('normalizing original on-disk CRLF remains rejected',()=>runner.assertPreparedReaderInput(prepared,Buffer.from(prepared.source.replaceAll('\r\n','\n'))));
no('an actual opened source content edit remains rejected',()=>runner.assertPreparedReaderInput(prepared,Buffer.from(prepared.source+'X')));
yes('final counts derive from all actual output records despite an earlier failure',()=>{const output=Array.from({length:44},(_,i)=>({validated:i!==0&&i!==1}));assert.deepEqual(runner.matrixOutputCounts(output),{attemptedMatrixOutputs:44,validatedMatrixOutputs:42});});
yes('truthy unvalidated status cannot be counted as a validated output',()=>assert.deepEqual(runner.matrixOutputCounts([{validated:true},{validated:'true'},{}]),{attemptedMatrixOutputs:3,validatedMatrixOutputs:1}));
console.log(JSON.stringify({passed:true,productInvocations:0,nativeAcceptance:false,controls:results.length,results}));
