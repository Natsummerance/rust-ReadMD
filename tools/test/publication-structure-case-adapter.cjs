'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const original=require('./publication-structure-original-layout.cjs'),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const SUMMARY='.cache/capability-upgrade/publication-context-inputs/ownedinputs/structure-qa-v2-20261015-first/independent-note-oracle-summary-remaining.json';
function privateFile(root,relative){const base=path.resolve(root,'.cache/private'),file=path.resolve(root,relative),rel=path.relative(base,file);assert(rel&&!rel.startsWith('..')&&!path.isAbsolute(rel),'Independent note oracle escaped private QA');for(let at=file;;at=path.dirname(at)){assert(!fs.lstatSync(at).isSymbolicLink(),'Reparsed private note oracle');if(at===base)break;}return file;}
function prepareCase(fixture,{root}){
  assert(path.isAbsolute(root));assert.equal(typeof fixture.source,'string');const slot='<!--readmd:bibliography-->';assert.equal(fixture.source.split(slot).length,2,'Exactly one original owned bibliography slot required');
  const originalSourceSha256=sha(fixture.source),boundaryMarker='READMDQABIBEND'+sha(fixture.id+'\0'+originalSourceSha256).slice(0,12).toUpperCase(),eol=fixture.source.includes('\r\n')?'\r\n':'\n';
  assert(!fixture.source.includes(boundaryMarker),'Owned END marker collides with original source');
  const prepared={...fixture,originalSourceSha256,boundaryMarker,source:fixture.source.replace(slot,slot+eol+eol+boundaryMarker),sourceAdapter:'v2 owned actual END boundary after unchanged bibliography slot; original INPUT/CSL/RESULT and original source occurrences unchanged'};
  if(!fixture.privateDerived){prepared.expectedNotes=[];prepared.expectedLayout=original.entries(fixture);return prepared;}
  const summaryFile=path.join(root,SUMMARY),summaryBytes=fs.readFileSync(summaryFile);assert.equal(sha(summaryBytes),'1ecebb879e2fda76a43e2a0fe9dedc9a1bb5d75818567e90ed2330028ad46424');
  const summary=JSON.parse(summaryBytes);assert.equal(summary.passed,true);assert.equal(summary.productInvocations,0);assert.equal(summary.engine.sha256,'2c659319b3b133c0204d60eba3e5daa566e1457e36b19cb6bfe4c8b1b4662650');
  const matches=summary.fixtures.filter(x=>x.id===fixture.id);assert.equal(matches.length,1);const record=matches[0],file=privateFile(root,record.path),bytes=fs.readFileSync(file);assert.equal(sha(bytes),record.sha256);assert.equal(bytes.length,record.bytes);
  const independent=JSON.parse(bytes);assert.equal(independent.originalBibliographyReproduced,true);assert.equal(independent.fixtureSha256,fixture.provenance.sha256);assert.equal(independent.styleSha256,sha(fixture.csl));assert.equal(independent.inputSha256,sha(fixture.blocks.INPUT));assert.equal(independent.originalResultSha256,sha(fixture.blocks.RESULT));assert.equal(independent.engine.sha256,summary.engine.sha256);
  assert.equal(independent.notes.length,fixture.adapted.request.clusters.length);for(const[i,note]of independent.notes.entries()){assert.equal(note.number,i+1);assert.equal(note.sourceClusterIndex,i);assert(Array.isArray(note.segments)&&note.segments.every(s=>typeof s==='string'));}
  const styleClass=fixture.csl.match(/<style\b[^>]*\bclass="([^"]+)"/su)?.[1];assert(['note','in-text'].includes(styleClass),'Original CSL class needs an explicit independent adapter');
  prepared.expectedNotes=styleClass==='note'?structuredClone(independent.notes):[];prepared.independentNoteOracle={path:record.path,sha256:record.sha256,engineSha256:summary.engine.sha256,styleClass,expectedNoteCount:prepared.expectedNotes.length,source:'Fixed independent citeproc-js citation baseline, original CSL/INPUT; only original note-class citations become notes; never ReadMD output'};return prepared;
}
module.exports={prepareCase};
