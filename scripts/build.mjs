#!/usr/bin/env node
// Reproducible source build and optional local install. No downloads execute as shell scripts.
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const {values}=parseArgs({options:{help:{type:'boolean'},check:{type:'boolean'},'install-dir':{type:'string'},'skip-dependencies':{type:'boolean'}}});
if(values.help){console.log('Usage: node scripts/build.mjs [--check] [--install-dir <directory>] [--skip-dependencies]\nBuilds dashboard and embedded release executable into dist/. --check only checks build prerequisites.\n--skip-dependencies uses an existing dashboard/node_modules; normal builds run npm ci.');process.exit(0);}
const [major,minor]=process.versions.node.split('.').map(Number);
if(!((major===20&&minor>=19)||(major===22&&minor>=12)||major>22))throw Error('Node.js 20.19+ or 22.12+ is required.');
function run(command,args,opts={}){
 const result=spawnSync(command,args,{cwd:root,stdio:'inherit',shell:process.platform==='win32'&&command==='npm.cmd',...opts});
 if(result.error)throw Error(`Cannot run ${command}: ${result.error.message}`);
 if(result.status!==0)throw Error(`${command} exited with ${result.status ?? result.signal}`);
}
run('rustc',['--version']);run('cargo',['--version']);run(process.platform==='win32'?'npm.cmd':'npm',['--version']);
if(values.check){console.log('JavaScript and Rust tools are available. Native C/C++ prerequisites are listed in docs/BUILD.md.');process.exit(0);}
const npm=process.platform==='win32'?'npm.cmd':'npm';
if(!values['skip-dependencies'])run(npm,['--prefix','dashboard','ci']);
run(npm,['--prefix','dashboard','run','check']);run(npm,['--prefix','dashboard','run','build']);
const remap=[`--remap-path-prefix=${os.homedir()}=build-home`,`--remap-path-prefix=${root}=hycli-source`];
const cargoHome=process.env.CARGO_HOME;if(cargoHome)remap.push(`--remap-path-prefix=${path.resolve(cargoHome)}=cargo-home`);
let rustflags=[process.env.CARGO_ENCODED_RUSTFLAGS||process.env.RUSTFLAGS?.split(/\s+/).filter(Boolean).join('\x1f')||'',...remap].filter(Boolean).join('\x1f');
const toolchain=spawnSync('rustc',['-vV'],{cwd:root,encoding:'utf8'});
if(toolchain.error||toolchain.status!==0)throw Error('Cannot identify the Rust build target');
const hostTriple=toolchain.stdout.match(/^host: (.+)$/m)?.[1];
const targetTriple=process.env.CARGO_BUILD_TARGET||(hostTriple?.includes('windows')?hostTriple:undefined);
const rustTarget=targetTriple||hostTriple;
const platform=rustTarget?.includes('windows')?'win32':rustTarget?.includes('apple-darwin')?'darwin':rustTarget?.includes('linux')?'linux':null;
const arch=rustTarget?.startsWith('x86_64-')?'x64':rustTarget?.startsWith('aarch64-')?'arm64':null;
if(!platform||!arch)throw Error(`Unsupported release target: ${rustTarget}. Supported: Linux, macOS and Windows on x64 or ARM64.`);
if(platform==='win32')rustflags+='\x1f-C\x1ftarget-feature=+crt-static';
const minimumMacOS=process.env.MACOSX_DEPLOYMENT_TARGET||'13.0';
run('cargo',['build','--release','--locked','--bins',...(targetTriple?['--target',targetTriple]:[])],{env:{...process.env,CARGO_ENCODED_RUSTFLAGS:rustflags,...(platform==='darwin'?{MACOSX_DEPLOYMENT_TARGET:minimumMacOS}:{})}});
const name=platform==='win32'?'hycli.exe':'hycli';
const target=path.resolve(root,process.env.CARGO_TARGET_DIR||'target');
const binary=path.join(target,...(targetTriple?[targetTriple]:[]),'release',name);
const output=path.join(root,'dist');fs.mkdirSync(output,{recursive:true});
const artifact=path.join(output,name);
const binaries={};
for(const base of ['hycli','hycli-desktop']){
 const filename=base+(platform==='win32'?'.exe':'');
 const built=path.join(path.dirname(binary),filename),destination=path.join(output,filename),temporary=path.join(output,`.${filename}.package-${process.pid}`);
 try{fs.copyFileSync(built,temporary);if(process.platform!=='win32')fs.chmodSync(temporary,0o755);fs.renameSync(temporary,destination);}
 finally{fs.rmSync(temporary,{force:true});}
 binaries[filename]=createHash('sha256').update(fs.readFileSync(destination)).digest('hex');
}
if(platform==='win32')run(process.execPath,['scripts/verify-windows-runtime.mjs',...Object.keys(binaries).map(file=>path.join(output,file))]);
const digest=binaries[name];
fs.writeFileSync(path.join(output,'SHA256SUMS'),Object.entries(binaries).map(([file,hash])=>`${hash}  ${file}\n`).join(''));
fs.writeFileSync(path.join(output,'build.json'),JSON.stringify({rust_target:rustTarget,platform,arch,sha256:digest,binaries,...(platform==='darwin'?{minimum_macos:minimumMacOS}:{}),...(platform==='win32'?{static_crt:true}:{})},null,2)+'\n');
fs.copyFileSync(path.join(root,'LICENSE'),path.join(output,'LICENSE'));
fs.writeFileSync(path.join(output,'README.md'),`# Hycli\n\nFrom the unpacked package directory, open Hycli with \`./${name} open\` or double-click \`hycli-desktop${platform==='win32'?'.exe':''}\`. The engine starts in the background and opens your browser. You can close the terminal. Open Hycli again to reuse the running instance; quit from Settings or with \`./${name} stop\`. Open the printed local address if your browser does not open automatically.\n\nConnect your AI, add a website and optional task, and review its available actions. Changes require your approval in the dashboard.\n\nThe executable includes the website engine and dashboard. Rust and Node.js are not needed to run it. ChatGPT login additionally uses an installed Codex CLI; API-key connections do not.\n\nCLI/MCP: \`./${name} --help\`. After adding the executable to PATH, the general agent connection is \`hycli mcp\`. Coding agents in the dashboard provides the configuration with your current executable path. Portable instructions and skill are included in native archives.\n\nThis package contains the native build for ${platform}/${arch} (${rustTarget}). Check SHA256SUMS before installing.\n\nTo the extent permitted by law, the authors and contributors are not liable for account bans, suspensions, or restrictions resulting from the use of Hycli. Do not use Hycli for hacking, unauthorized access, or attacks.\n`);
if(rustTarget===hostTriple)run(artifact,['--version']);
else console.log(`Cross-compiled ${rustTarget}; run the package smoke check on that target platform.`);
if(values['install-dir']){
 const install=path.resolve(values['install-dir']);fs.mkdirSync(install,{recursive:true});
 const destination=path.join(install,name),temporary=path.join(install,`.${name}.install-${process.pid}`);
 try {fs.copyFileSync(artifact,temporary);if(process.platform!=='win32')fs.chmodSync(temporary,0o755);fs.renameSync(temporary,destination);}
 finally{fs.rmSync(temporary,{force:true});}
 console.log(`Installed: ${destination}\nRun it with: ${JSON.stringify(destination)} open`);
}
console.log(`Ready: ${artifact}\nStart: ${JSON.stringify(artifact)} open\nChecksum: ${path.join(output,'SHA256SUMS')}`);
