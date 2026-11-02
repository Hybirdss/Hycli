import test from 'node:test';
import assert from 'node:assert/strict';
import { providerStatus } from './src/provider-status.ts';
const provider={id:'codex',connected:true,available:true,model:'chosen',default_model:'default',models:['chosen']};
test('saved configuration never pretends missing executable or unknown account is verified',()=>{
 assert.equal(providerStatus({...provider,available:false,verification:'verified'}),'actions.unverified');
 assert.equal(providerStatus(provider),'actions.unverified');
 assert.equal(providerStatus({...provider,verification:'checking'}),'connections.testing');
});
test('transient failure and recovery produce different live labels',()=>{
 const p={...provider,verification:'check_failed'};assert.equal(providerStatus(p),'actions.unverified');p.verification='verified';assert.equal(providerStatus(p),'connections.connected');
});
test('signed-out evidence is distinct from failed verification',()=>{
 assert.equal(providerStatus({...provider,verification:'signed_out'}),'connections.notConnected');
 assert.equal(providerStatus({...provider,verification:'check_failed'}),'actions.unverified');
});
test('passively discovered account does not label an explicitly disconnected provider connected',()=>{
 assert.equal(providerStatus({...provider,connected:false,verification:'verified'}),'connections.notConnected');
});
test('API key provider presentation remains unchanged',()=>{
 assert.equal(providerStatus({...provider,id:'openai',available:false}),'connections.connected');
 assert.equal(providerStatus({...provider,id:'anthropic',connected:false}),'connections.notConnected');
});
