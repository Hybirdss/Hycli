import {createServer} from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export async function startDiscoveryFixture({web,port=0}) {
  const stamp='2026-10-20T22:45:00Z';
  const requests=[],controls=[],connections=[],streams=new Set(),held=[];
  let accountVisible=false,nativeMode='deny',revision=0,nativeEnabled=true;
  const account={id:'verified-account',site_id:'site',site_url:'https://example.test',label:'Fixture account',browser:'Companion',profile:'Synthetic',status:'connected',verified_identity:true,checked_at:stamp,cookie_count:1,created_at:stamp};
  const state=()=>({settings:{locale:'en',theme:'light',notifications:false,native_browser_discovery:nativeEnabled},providers:[],sites:[{id:'site',title:'Example',url:'https://example.test',host:'example.test',status:'ready',actions:[],updated_at:stamp}],accounts:accountVisible?[account]:[],jobs:[],approvals:[],engine:{available:true,compatible:true,version:'1'},version:'synthetic-discovery'});
  const changed=()=>{revision++;for(const stream of streams)stream.write('data: changed\n\n');};
  const control=mode=>{controls.push({mode,at:Date.now()});if(mode==='Show verified account')accountVisible=true;else if(mode==='Remove account')accountVisible=false;else if(mode==='Hold native request')nativeMode='hold';else if(mode==='Deny native request'){nativeMode='deny';for(const send of held.splice(0))send(503,{error:{code:'browser_unavailable'}});}else if(mode==='Empty native result'){nativeMode='empty';for(const send of held.splice(0))send(200,{profiles_checked:1,profiles_unavailable:0,accounts:[],selected_account:'',needs_choice:false,login_url:'https://example.test'});}changed();};
  const evidence=()=>({revision,nativeMode,nativeEnabled,accountVisible,requests,controls,connections});
  const page=()=>`<!doctype html><html><head><meta charset="utf-8"><title>Synthetic discovery controls</title></head><body style="font:20px sans-serif;margin:30px"><h1>Synthetic discovery controls</h1><p>Native ${nativeMode}. Verified account ${accountVisible}. Native requests ${requests.filter(r=>r.path==='/api/browser/find').length}.</p><form method="POST" action="/fixture/control">${['Show verified account','Remove account','Hold native request','Deny native request','Empty native result'].map(mode=>`<button name="mode" value="${mode}" style="font:20px sans-serif;padding:12px;margin:6px">${mode}</button>`).join('')}</form><p><a href="/fixture/evidence">Evidence</a></p><p><a href="/#accounts">Dashboard</a></p></body></html>`;
  const server=createServer(async(req,res)=>{
    const url=new URL(req.url,'http://localhost');res.setHeader('Cache-Control','no-store');
    const send=(status,value)=>{if(res.destroyed)return;res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(value));};
    if(url.pathname==='/fixture/control'){if(req.method==='POST'){let raw='';for await(const chunk of req)raw+=chunk;control(new URLSearchParams(raw).get('mode'));res.writeHead(303,{Location:'/fixture/control'});res.end();}else{res.writeHead(200,{'Content-Type':'text/html'});res.end(page());}return;}
    if(url.pathname==='/fixture/evidence'){send(200,evidence());return;}
    if(url.pathname==='/api/events'){res.writeHead(200,{'Content-Type':'text/event-stream'});res.write(': ready\n\n');streams.add(res);req.on('close',()=>streams.delete(res));return;}
    if(url.pathname.startsWith('/api/')){
      let raw='';for await(const chunk of req)raw+=chunk;let body;try{body=raw?JSON.parse(raw):undefined;}catch{}
      requests.push({method:req.method,path:url.pathname,body,at:Date.now()});
      if(url.pathname==='/api/session'){send(200,{csrf:'synthetic'});return;}
      if(url.pathname==='/api/state'){send(200,state());return;}
      if(url.pathname==='/api/settings'){if(req.method!=='PATCH'){send(405,{});return;}nativeEnabled=body.native_browser_discovery;changed();send(200,{saved:true});return;}
      if(url.pathname==='/api/browser/discovery'){send(200,{profiles:[{id:'synthetic-profile',browser:'Chromium',profile:'Synthetic'}]});return;}
      if(url.pathname==='/api/browser/find'){res.on('close',()=>connections.push({path:url.pathname,closedAt:Date.now(),completed:res.writableEnded}));if(nativeMode==='hold')held.push(send);else if(nativeMode==='deny')send(503,{error:{code:'browser_unavailable'}});else send(200,{profiles_checked:1,profiles_unavailable:0,accounts:[],selected_account:'',needs_choice:false,login_url:'https://example.test'});return;}
      if(url.pathname==='/api/browser/login'){send(200,{opened:true});return;}
      if(url.pathname==='/api/accounts/pair'){send(200,{code:'SYNTHETIC',expires_at:'2099-01-01T00:00:00Z'});return;}
      if(url.pathname==='/api/browser/select'){send(200,{account});return;}
      if(url.pathname==='/api/sites/prepare'){send(200,{job:{id:'synthetic-preparation',kind:'prepare',status:'queued',site_id:'site',site_title:'Example',started_at:stamp,updated_at:stamp,notes:[],found:[]}});return;}
      send(404,{error:{code:'not_found'}});return;
    }
    const relative=url.pathname==='/'?'index.html':decodeURIComponent(url.pathname.slice(1));if(relative.split('/').includes('..')){res.writeHead(404);res.end();return;}
    try{const bytes=await fs.readFile(path.join(web,relative));res.writeHead(200,{'Content-Type':relative.endsWith('.js')?'text/javascript':relative.endsWith('.css')?'text/css':relative.endsWith('.svg')?'image/svg+xml':'text/html'});res.end(bytes);}catch{res.writeHead(404);res.end();}
  });
  await new Promise(resolve=>server.listen(port,'127.0.0.1',resolve));
  return {origin:`http://127.0.0.1:${server.address().port}`,control,evidence,close:async()=>{for(const stream of streams)stream.end();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const root=fileURLToPath(new URL('..',import.meta.url));
  const fixture=await startDiscoveryFixture({web:process.env.DISCOVERY_WEB||path.join(root,'web'),port:Number(process.env.PORT||4176)});
  console.log(`Synthetic discovery fixture ${fixture.origin}/#accounts`);
}
