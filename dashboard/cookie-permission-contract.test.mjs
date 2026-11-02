import test from 'node:test';
import assert from 'node:assert/strict';
import {expectedFixtures,verifyVisibility} from './cookie-permission-contract.mjs';
const cookie = name => ({name,hostOnly:name.endsWith('_host'),...(name==='partitioned_parent'?{partitionKey:{topLevelSite:'https://top-proof.test'}}:{})});
function fixture(label,parentVisible=false) {
  const all = label==='none'?[]:(label==='both'?['www_host','www_domain','parent_domain','apex_host']:['www_host','www_domain',...(parentVisible?['parent_domain']:[])]).map(cookie);
  return {seeded:expectedFixtures.map(cookie),all,store:structuredClone(all),www:all.filter(c=>c.name!=='apex_host'),partitioned:label==='both'?[cookie('partitioned_parent')]:[]};
}
test('accepts no-grant control and complete both-grant result',()=>{
  assert.equal(verifyVisibility('none',fixture('none')),'no-host-permission');
  assert.equal(verifyVisibility('both',fixture('both')),'parent-visible-with-current-grants');
});
test('records either measured www-only parent outcome rather than prescribing browser behavior',()=>{
  assert.equal(verifyVisibility('www',fixture('www')),'parent-filtered-with-current-grants');
  assert.equal(verifyVisibility('www',fixture('www',true)),'parent-visible-with-current-grants');
});
test('missing seeded fixtures cannot yield a false pass',()=>{
  const value=fixture('both');value.seeded.pop();assert.throws(()=>verifyVisibility('both',value));
});
test('negative-control leaks, URL mismatch and default partition leakage fail',()=>{
  for(const [field,name] of [['all','unrelated'],['www','apex_host'],['all','partitioned_parent']]){
    const value=fixture('both');value[field].push(cookie(name));assert.throws(()=>verifyVisibility('both',value));
  }
});
test('API errors, missing permitted cookies and wrong store fail',()=>{
  for(const mutate of [v=>v.www={error:'native failure'},v=>{v.all=[];v.store=[];},v=>v.store=[]]){
    const value=fixture('both');mutate(value);assert.throws(()=>verifyVisibility('both',value));
  }
});
test('www URL query must retain its domain cookie and exclude partitioned cookies',()=>{
  const missing = fixture('www'); missing.www = missing.www.filter(c=>c.name!=='www_domain');
  assert.throws(()=>verifyVisibility('www',missing));
  for (const field of ['all','www','store']) {
    const leaked = fixture('www'); leaked[field].push(cookie('partitioned_parent'));
    assert.throws(()=>verifyVisibility('www',leaked));
  }
  const substituted = fixture('www'); substituted.www = [cookie('www_host'),cookie('partitioned_parent')];
  assert.throws(()=>verifyVisibility('www',substituted));
});
