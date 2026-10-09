#!/usr/bin/env node
// Inspect the PE import directory directly; this runs without a developer SDK on PATH.
import fs from 'node:fs';
import path from 'node:path';
if(process.argv.length<3)throw Error('Usage: node scripts/verify-windows-runtime.mjs <exe>...');
for(const file of process.argv.slice(2)){
 const bytes=fs.readFileSync(file),pe=bytes.readUInt32LE(0x3c);
 if(bytes.toString('ascii',pe,pe+4)!=='PE\0\0')throw Error('Not a PE executable: '+file);
 const sections=bytes.readUInt16LE(pe+6),optional=pe+24,optionalSize=bytes.readUInt16LE(pe+20);
 const magic=bytes.readUInt16LE(optional);
 if(![0x20b,0x10b].includes(magic))throw Error('Unsupported PE optional header');
 const table=optional+optionalSize;
 function offset(rva){
  for(let i=0;i<sections;i++){
   const section=table+i*40,start=bytes.readUInt32LE(section+12),size=bytes.readUInt32LE(section+16);
   if(rva>=start&&rva<start+size)return bytes.readUInt32LE(section+20)+rva-start;
  }
  throw Error('Invalid PE import RVA');
 }
 const directory=optional+(magic===0x20b?112:96);
 const dlls=[];
 // Regular and delay-loaded imports (delay descriptor attributes select RVA addresses).
 for(const [index,width,nameField,delayed] of [[1,20,12,false],[13,32,4,true]]){
  const rva=bytes.readUInt32LE(directory+index*8);if(!rva)continue;
  let descriptor=offset(rva);
  while(bytes.subarray(descriptor,descriptor+width).some(value=>value!==0)){
   if(delayed&&!(bytes.readUInt32LE(descriptor)&1))throw Error('Unsupported VA-based delayed import');
   const start=offset(bytes.readUInt32LE(descriptor+nameField)),end=bytes.indexOf(0,start);
   if(end<0||end-start>260)throw Error('Invalid imported DLL name');
   dlls.push(bytes.toString('ascii',start,end));descriptor+=width;
   if(descriptor+width>bytes.length)throw Error('Invalid import descriptor');
  }
 }
 const external=dlls.filter(name=>/^(?:vcruntime|msvcp|msvcr|concrt|vcomp)\d.*\.dll$/i.test(name));
 if(external.length)throw Error(`${file} requires a Visual C++ redistributable: ${external.join(', ')}`);
 console.log(JSON.stringify({binary:path.basename(file),static_crt:'verified',imports:dlls}));
}
