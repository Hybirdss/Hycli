import test from 'node:test';
import assert from 'node:assert/strict';
import {fixtureSession, installFixtureSession} from './fixture-session.mjs';
const ports = {dashboard:'http://127.0.0.1:4318', session:{cookie:'hycli_session=synthetic-session',csrf:'synthetic-csrf'}};
test('synthetic session setup retains private cookie attributes and clean URL', async () => {
  let cookies;
  await installFixtureSession({addCookies: async value => { cookies = value; }}, ports);
  assert.deepEqual(cookies, [{name:'hycli_session',value:'synthetic-session',url:'http://127.0.0.1:4318',httpOnly:true,sameSite:'Strict'}]);
  assert.deepEqual(fixtureSession(ports), ports.session);
});
test('synthetic setup refuses absent sessions and non-loopback destinations', () => {
  assert.throws(() => fixtureSession({dashboard:ports.dashboard}));
  assert.throws(() => fixtureSession({...ports,dashboard:'https://example.test'}));
  assert.throws(() => fixtureSession({...ports,session:{...ports.session,cookie:'hycli_session=synthetic; other=value'}}));
});
