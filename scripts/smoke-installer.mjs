#!/usr/bin/env node
// Extract/mount or silently install the actual native artifact, then exercise its launcher.
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const artifact=path.resolve(process.argv[2]);
const scratch=path.join(root,'.cache/installer-smoke');await fs.mkdir(scratch,{recursive:true});
const directory=await fs.mkdtemp(path.join(scratch,'fresh-'));
function run(command,args,opts={}){const r=spawnSync(command,args,{cwd:root,stdio:'inherit',...opts});if(r.error)throw r.error;if(r.status!==0)throw Error(`${command}: ${r.status}`);return r;}
let binary,uninstall,mounted=false;
try{
 if(process.platform==='linux'){
  run('dpkg-deb',['--info',artifact]);run('dpkg-deb',['--extract',artifact,directory]);
  const desktop=await fs.readFile(path.join(directory,'usr/share/applications/io.github.hybirdss.Hycli.desktop'),'utf8');
  assert(desktop.includes('Exec=/usr/lib/hycli/hycli-desktop\n'));assert(desktop.includes('Terminal=false'));
  assert.equal(await fs.readlink(path.join(directory,'usr/bin/hycli')),'../lib/hycli/hycli');
  binary=path.join(directory,'usr/lib/hycli/hycli');
 }else if(process.platform==='darwin'){
  run('hdiutil',['attach',artifact,'-nobrowse','-readonly','-mountpoint',directory]);mounted=true;
  const app=path.join(directory,'Hycli.app');run('plutil',['-lint',path.join(app,'Contents/Info.plist')]);
  run('codesign',['--verify','--deep','--strict',app]);binary=path.join(app,'Contents/MacOS/hycli');
 }else if(process.platform==='win32'){
  const install=path.join(directory,'installed');
  // /D is deliberately last, as required by NSIS.
  run(artifact,['/S',`/D=${install}`]);binary=path.join(install,'hycli.exe');uninstall=path.join(install,'Uninstall.exe');
  await fs.access(uninstall);await fs.access(path.join(install,'hycli.ico'));
 }else throw Error('Unsupported installer platform');
 run(process.execPath,[path.join(root,'scripts/smoke-desktop.mjs'),binary]);
 console.log(JSON.stringify({artifact,installer:'passed',verification:process.platform==='linux'?'DEB extraction and installed layout':process.platform==='darwin'?'mounted DMG and app bundle':'silent per-user installation',directory}));
}finally{
 if(uninstall){
  // The uninstaller uses the normal data-dir resolver: isolate it from any existing user instance.
  run(uninstall,['/S'],{env:{...process.env,HYCLI_DATA_DIR:path.join(directory,'uninstall-data')}});
  for(let i=0;i<100;i++){try{await fs.access(path.dirname(uninstall));}catch{break;}await new Promise(r=>setTimeout(r,100));}
  await assert.rejects(fs.access(binary));
 }
 if(mounted)run('hdiutil',['detach',directory]);
}
