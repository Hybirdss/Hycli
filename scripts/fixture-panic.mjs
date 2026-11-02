// Only the owned Rust fixture's panic banner is fatal here. Expected HTTP errors
// and failed application jobs are asserted by individual suites, not this gate.
export function fixturePanicMonitor() {
 let tail='';
 let failed=false;
 return {
  write(chunk) {
   const text=tail+chunk.toString();
   failed ||= /(?:^|\n)thread '[^\r\n]*' (?:\([^\r\n]*\) )?panicked at(?: |:)/.test(text);
   tail=text.slice(-8192);
  },
  check() { if(failed) throw Error('Owned fixture panicked; see fixture stderr above'); }
 };
}
