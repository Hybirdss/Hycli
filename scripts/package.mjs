#!/usr/bin/env node
// Package an explicit public file set; never tar a working tree or an entire dist/.
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {parseArgs} from 'node:util';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const {values}=parseArgs({options:{source:{type:'boolean'},help:{type:'boolean'}}});
if(values.help){console.log('Usage: node scripts/package.mjs [--source]\nPackage dist/hycli (native) or an allowlisted public source snapshot. Output: dist/artifacts/. No Git history, user data or developer state is included.');process.exit(0);}
const version=(await fs.readFile(path.join(root,'Cargo.toml'),'utf8')).match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if(!version)throw Error('Missing package version');
const tarVersion=spawnSync('tar',['--version'],{encoding:'utf8'});
if(tarVersion.error||tarVersion.status!==0)throw Error('Packaging requires GNU tar or bsdtar (libarchive).');
const tarDescription=tarVersion.stdout+tarVersion.stderr;
const metadataOptions=tarDescription.includes('GNU tar')?['--owner=0','--group=0','--numeric-owner','--no-acls','--no-xattrs']: /bsdtar|libarchive/i.test(tarDescription)?['--uid','0','--gid','0','--uname','root','--gname','root','--no-acls','--no-xattrs','--no-fflags',...(process.platform==='darwin'?['--no-mac-metadata']:[])]:null;
if(!metadataOptions)throw Error('Packaging requires GNU tar or bsdtar (libarchive) to normalize archive metadata.');
const tarEnv={...process.env,COPYFILE_DISABLE:'1'};delete tarEnv.TAR_OPTIONS;
const build=values.source?null:JSON.parse(await fs.readFile(path.join(root,'dist','build.json'),'utf8'));
if(build&&(!['linux','darwin','win32'].includes(build.platform)||!['x64','arm64'].includes(build.arch)))throw Error('Invalid native build metadata; rebuild with scripts/build.mjs');
const name=`hycli-${version}-${values.source?'source':build.platform+'-'+build.arch}`;
const scratch=path.join(root,'.cache','packages');await fs.mkdir(scratch,{recursive:true});
const stage=await fs.mkdtemp(path.join(scratch,name+'-'));
const content=path.join(stage,name);await fs.mkdir(content);
const excluded=new Set(['node_modules','target','dist','web','.cache','.localdata','.repowise','.git','test-results','playwright-report','__pycache__']);
const authoringOnly=new Set(['scripts/build-motion.py','dashboard/inspect-ui.mjs','dashboard/progress-visual.mjs','dashboard/visual-fixture.mjs']);
const manifest=[];
async function copy(relative,destination=relative){
 if(authoringOnly.has(relative.split(path.sep).join('/')))return;
 const source=path.join(root,relative),stat=await fs.lstat(source);
 if(stat.isSymbolicLink())throw Error(`Source package refuses symbolic link: ${relative}`);
 if(stat.isDirectory()){
  for(const entry of (await fs.readdir(source)).sort()){
   if(excluded.has(entry)||entry.startsWith('.')||/\.(?:log|sqlite3?|db|har|pyc)$/.test(entry))continue;
   await copy(path.join(relative,entry),path.join(destination,entry));
  }
 }else if(stat.isFile()){
  await fs.mkdir(path.dirname(path.join(content,destination)),{recursive:true});await fs.copyFile(source,path.join(content,destination));
  await fs.chmod(path.join(content,destination),stat.mode&0o111?0o755:0o644);
  manifest.push(destination.split(path.sep).join('/'));
 }
}
try{
 if(values.source){
  for(const file of ['Cargo.toml','Cargo.lock','LICENSE','README.md','CONTRIBUTING.md','CHANGELOG.md','AGENTS.md','CLAUDE.md','.gitignore'])await copy(file);
  for(const directory of ['src','tests','examples','agent','skills','browser-companion','scripts','packaging','dashboard','.github'])await copy(directory);
  for(const file of ['docs/BUILD.md','docs/ARCHITECTURE.md','docs/SITESPEC.md','docs/RELEASING.md','docs/brand/concepts/hycli-shima-banner-v4.png'])await copy(file);
  for(const directory of ['docs/i18n','docs/images','docs/brand/badges'])await copy(directory);
  for(const file of ['docs/design/brand-research/claude-official-32.provenance.txt','docs/design/motion/PROVENANCE.md'])await copy(file);
 }else{
  const executable=build.platform==='win32'?'hycli.exe':'hycli';
  if(createHash('sha256').update(await fs.readFile(path.join(root,'dist',executable))).digest('hex')!==build.sha256)throw Error('Native executable does not match build.json');
  await copy('dist/build.json','build.json');
  for(const base of ['hycli','hycli-desktop']){
   const filename=base+(build.platform==='win32'?'.exe':'');
   if(createHash('sha256').update(await fs.readFile(path.join(root,'dist',filename))).digest('hex')!==build.binaries?.[filename])throw Error('Binary checksum mismatch: '+filename);
   await copy('dist/'+filename,filename);await fs.chmod(path.join(content,filename),0o755);
  }
  if(build.platform==='linux'){await copy('packaging/install-linux.sh','install.sh');await fs.chmod(path.join(content,'install.sh'),0o755);await copy('packaging/icons/hycli.png','hycli.png');}
  for(const file of ['LICENSE','agent/AGENTS.md','agent/CLAUDE.md','docs/SITESPEC.md','docs/BUILD.md','docs/RELEASING.md'])await copy(file);
  await copy('skills');await copy('dist/README.md','README.md');
 }
 const checksums=[];
 for(const file of manifest.sort())checksums.push(`${createHash('sha256').update(await fs.readFile(path.join(content,file))).digest('hex')}  ${file}`);
 await fs.writeFile(path.join(content,'SHA256SUMS'),checksums.join('\n')+'\n');
 const output=path.join(root,'dist','artifacts');await fs.mkdir(output,{recursive:true});
 const archive=path.join(output,name+'.tar.gz');
 const packed=spawnSync('tar',[...metadataOptions,'-czf',archive,'-C',stage,name],{stdio:'inherit',env:tarEnv});
 if(packed.error)throw packed.error;if(packed.status!==0)throw Error('tar failed');
 const digest=createHash('sha256').update(await fs.readFile(archive)).digest('hex');
 await fs.writeFile(archive+'.sha256',`${digest}  ${path.basename(archive)}\n`);
 await fs.writeFile(path.join(output,name+'.manifest.json'),JSON.stringify({version,kind:values.source?'source':'native',platform:build?.platform??null,arch:build?.arch??null,files:[...manifest,'SHA256SUMS'],sha256:digest},null,2)+'\n');
 console.log(JSON.stringify({archive,sha256:digest,files:manifest.length+1}));
}finally{await fs.rm(stage,{recursive:true,force:true});}
