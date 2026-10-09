#!/usr/bin/env node
// Native installers from verified release binaries. No working-tree or user-state copies.
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const build=JSON.parse(await fs.readFile(path.join(root,'dist/build.json'),'utf8'));
if(build.platform!==process.platform)throw Error('Build native installers on their target OS.');
if(!['x64','arm64'].includes(build.arch))throw Error('Unsupported architecture');
const version=(await fs.readFile(path.join(root,'Cargo.toml'),'utf8')).match(/^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"/m)?.[1];
if(!version)throw Error('A numeric release version is required');
const scratch=path.join(root,'.cache/desktop-packages');await fs.mkdir(scratch,{recursive:true});
const stage=await fs.mkdtemp(path.join(scratch,'stage-'));
const output=path.join(root,'dist/artifacts');await fs.mkdir(output,{recursive:true});
const stem=`hycli-${version}-${build.platform}-${build.arch}`;
function run(command,args){const result=spawnSync(command,args,{cwd:root,stdio:'inherit'});if(result.error)throw result.error;if(result.status!==0)throw Error(`${command} failed (${result.status})`);}
async function put(relative,destination,mode=0o644){await fs.mkdir(path.dirname(destination),{recursive:true});await fs.copyFile(path.join(root,relative),destination);await fs.chmod(destination,mode);}
async function write(destination,text){await fs.mkdir(path.dirname(destination),{recursive:true});await fs.writeFile(destination,text);}
async function binaries(destination){
 for(const base of ['hycli','hycli-desktop']){
  const name=base+(build.platform==='win32'?'.exe':'');
  const hash=createHash('sha256').update(await fs.readFile(path.join(root,'dist',name))).digest('hex');
  if(hash!==build.binaries?.[name])throw Error(`Rebuild: binary checksum mismatch (${name})`);
  await put('dist/'+name,path.join(destination,name),0o755);
 }
}
async function receipt(file){
 const digest=createHash('sha256').update(await fs.readFile(file)).digest('hex');
 await fs.writeFile(file+'.sha256',`${digest}  ${path.basename(file)}\n`);
 console.log(JSON.stringify({artifact:file,sha256:digest}));
}
try{
 if(build.platform==='linux'){
  const pkg=path.join(stage,'deb'),lib=path.join(pkg,'usr/lib/hycli');await binaries(lib);
  await fs.mkdir(path.join(pkg,'usr/bin'),{recursive:true});await fs.symlink('../lib/hycli/hycli',path.join(pkg,'usr/bin/hycli'));
  await put('packaging/icons/hycli.png',path.join(pkg,'usr/share/icons/hicolor/256x256/apps/hycli.png'));
  await put('LICENSE',path.join(pkg,'usr/share/doc/hycli/copyright'));
  await put('dist/README.md',path.join(pkg,'usr/share/doc/hycli/README.md'));
  await write(path.join(pkg,'usr/share/applications/io.github.hybirdss.Hycli.desktop'),`[Desktop Entry]\nType=Application\nName=Hycli\nComment=Websites, ready for AI\nExec=/usr/lib/hycli/hycli-desktop\nIcon=hycli\nTerminal=false\nCategories=Development;Utility;\nActions=Quit;\n\n[Desktop Action Quit]\nName=Quit Hycli\nExec=/usr/lib/hycli/hycli-desktop stop\n`);
  // Derive libc's actual ABI floor from the built ELF, including builds newer than CI Ubuntu.
  const symbols=spawnSync('objdump',['-T',path.join(lib,'hycli')],{encoding:'utf8'});
  if(symbols.error||symbols.status!==0)throw Error('objdump is required to inspect Linux runtime compatibility');
  const versions=[...symbols.stdout.matchAll(/GLIBC_(\d+)\.(\d+)/g)].map(m=>[+m[1],+m[2]]).sort((a,b)=>a[0]-b[0]||a[1]-b[1]);
  if(!versions.length)throw Error('Cannot determine glibc requirement');
  const glibc=versions.at(-1).join('.');
  await write(path.join(pkg,'DEBIAN/control'),`Package: hycli\nVersion: ${version}\nArchitecture: ${build.arch==='x64'?'amd64':'arm64'}\nMaintainer: Hybirdss <noreply@github.com>\nSection: utils\nPriority: optional\nDepends: libc6 (>= ${glibc}), libgcc-s1, xdg-utils\nRecommends: ca-certificates, zenity | kdialog, libnotify-bin\nHomepage: https://github.com/Hybirdss/hycli\nDescription: Websites, ready for AI\n Local website tools, dashboard, CLI and MCP with an on-demand background engine.\n`);
  const deb=path.join(output,stem+'.deb');run('dpkg-deb',['--root-owner-group','--build',pkg,deb]);await receipt(deb);
 }else if(build.platform==='darwin'){
  const dmgRoot=path.join(stage,'image'),app=path.join(dmgRoot,'Hycli.app'),contents=path.join(app,'Contents');
  await binaries(path.join(contents,'MacOS'));
  const iconset=path.join(stage,'hycli.iconset');await fs.mkdir(iconset,{recursive:true});
  for(const size of [16,32,128,256,512]){
   for(const scale of [1,2])run('sips',['-z',String(size*scale),String(size*scale),path.join(root,'dashboard/public/brand/shima.png'),'--out',path.join(iconset,`icon_${size}x${size}${scale===2?'@2x':''}.png`)]);
  }
  await fs.mkdir(path.join(contents,'Resources'),{recursive:true});
  run('iconutil',['-c','icns',iconset,'-o',path.join(contents,'Resources/hycli.icns')]);
  await put('LICENSE',path.join(contents,'Resources/LICENSE'));
  await write(path.join(contents,'Info.plist'),`<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict>\n<key>CFBundleIdentifier</key><string>io.github.hybirdss.Hycli</string>\n<key>CFBundleName</key><string>Hycli</string>\n<key>CFBundleDisplayName</key><string>Hycli</string>\n<key>CFBundleExecutable</key><string>hycli-desktop</string>\n<key>CFBundleIconFile</key><string>hycli.icns</string>\n<key>CFBundlePackageType</key><string>APPL</string>\n<key>CFBundleShortVersionString</key><string>${version}</string>\n<key>CFBundleVersion</key><string>${version}</string>\n<key>LSMinimumSystemVersion</key><string>${build.minimum_macos||'13.0'}</string>\n<key>LSUIElement</key><true/>\n<key>NSHighResolutionCapable</key><true/>\n</dict></plist>\n`);
  const identity=process.env.MACOS_SIGN_IDENTITY;
  if(identity){
   for(const file of ['hycli','hycli-desktop'])run('codesign',['--force','--options','runtime','--timestamp','--sign',identity,path.join(contents,'MacOS',file)]);
   run('codesign',['--force','--options','runtime','--timestamp','--sign',identity,app]);
  }else{for(const file of ['hycli','hycli-desktop'])run('codesign',['--force','--sign','-',path.join(contents,'MacOS',file)]);run('codesign',['--force','--sign','-',app]);console.log('macOS: ad-hoc signature only; public distribution requires Developer ID and notarization.');}
  run('codesign',['--verify','--deep','--strict',app]);
  await fs.symlink('/Applications',path.join(dmgRoot,'Applications'));
  await put('dist/README.md',path.join(dmgRoot,'README.md'));
  const dmg=path.join(output,stem+'.dmg');await fs.rm(dmg,{force:true});
  run('hdiutil',['create','-volname','Hycli','-srcfolder',dmgRoot,'-ov','-format','UDZO',dmg]);
  if(identity)run('codesign',['--timestamp','--sign',identity,dmg]);
  if(process.env.MACOS_NOTARY_PROFILE){
   if(!identity)throw Error('Notarization requires MACOS_SIGN_IDENTITY');
   run('xcrun',['notarytool','submit',dmg,'--keychain-profile',process.env.MACOS_NOTARY_PROFILE,'--wait']);
   run('xcrun',['stapler','staple',dmg]);run('xcrun',['stapler','validate',dmg]);
  }
  await receipt(dmg);
 }else if(build.platform==='win32'){
  const payload=path.join(stage,'payload');await binaries(payload);
  for(const file of ['LICENSE','README.md'])await put('dist/'+file,path.join(payload,file));
  await put('packaging/icons/hycli.ico',path.join(payload,'hycli.ico'));
  const installer=path.join(output,stem+'-setup.exe');
  // Optional preinstalled signing command is invoked with an argument array, never a shell.
  const sign=file=>{if(process.env.WINDOWS_SIGNTOOL)run(process.env.WINDOWS_SIGNTOOL,['sign','/a','/fd','SHA256','/tr','http://timestamp.digicert.com','/td','SHA256',file]);};
  for(const base of ['hycli','hycli-desktop'])sign(path.join(payload,base+'.exe'));
  run(process.env.MAKENSIS||'makensis',['/V2',`/DSTAGE=${payload}`,`/DOUTPUT=${installer}`,`/DVERSION=${version}`,`/DARCH=${build.arch}`,`/DICON=${path.join(root,'packaging/icons/hycli.ico')}`,path.join(root,'packaging/windows.nsi')]);
  sign(installer);await receipt(installer);
 }else throw Error('Unsupported native platform');
}finally{await fs.rm(stage,{recursive:true,force:true});}
