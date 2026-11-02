import assert from 'node:assert/strict';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {createServer} from 'vite';

const server = await createServer({root:fileURLToPath(new URL('.',import.meta.url)),server:{middlewareMode:true},appType:'custom'});
try {
 const {WorkNotes} = await server.ssrLoadModule('/src/work.tsx');
 const {I18n} = await server.ssrLoadModule('/src/i18n.tsx');
 for(const locale of ['en','ko'])test(`worker failures use localized causes in ${locale}`,()=>{
  const codes=['ai_response_incomplete','ai_response_invalid','ai_connection_failed','rate_limited'];
  const job={notes:codes.map((code,index)=>({seq:index+1,at:'2026-01-01T00:00:00Z',code:`agent_failed_${code}`,agent:'explore',count:0,message:'synthetic-private-provider-body'}))};
  const html=renderToStaticMarkup(createElement(I18n,{locale},createElement(WorkNotes,{job})));
  assert.ok(!html.includes('synthetic-private-provider-body'));
  for(const code of codes)assert.ok(!html.includes(code));
  assert.ok(html.includes(locale==='en'?'The AI response was incomplete.':'AI 응답이 끝까지 생성되지 않았습니다.'));
  assert.ok(html.includes(locale==='en'?'The AI connection failed.':'AI 연결에 실패했습니다.'));
 });
} finally {await server.close();}
