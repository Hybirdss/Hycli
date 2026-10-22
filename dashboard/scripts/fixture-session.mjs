// Synthetic fixture setup only. The fixture redeems a test-issued ticket through
// the real bootstrap endpoint. Installing that cookie is NOT private-file or
// native browser handoff coverage; those transports need separate verification.
import assert from 'node:assert/strict';
export function fixtureSession(ports) {
  const address = new URL(ports.dashboard);
  assert.equal(address.protocol, 'http:');
  assert.equal(address.hostname, '127.0.0.1');
  assert.match(ports.session?.cookie ?? '', /^hycli_session=[^;\s]+$/);
  assert.equal(typeof ports.session?.csrf, 'string');
  assert.ok(ports.session.csrf.length > 0);
  return ports.session;
}
export async function installFixtureSession(context, ports) {
  const {cookie} = fixtureSession(ports);
  await context.addCookies([{
    name: 'hycli_session', value: cookie.slice('hycli_session='.length),
    url: ports.dashboard, httpOnly: true, sameSite: 'Strict',
  }]);
}
