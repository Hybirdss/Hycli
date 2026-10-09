import fs from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const base=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../src/locales');
const en=JSON.parse(await fs.readFile(path.join(base,'en.json'),'utf8'));
const keys=Object.keys(en).sort();let count=0;
const placeholders=value=>[...value.matchAll(/\{([^{}]+)\}/g)].map(x=>x[1]).sort().join('|');
for(const name of await fs.readdir(base)){if(!name.endsWith('.json'))continue;const v=JSON.parse(await fs.readFile(path.join(base,name),'utf8'));if(JSON.stringify(Object.keys(v).sort())!==JSON.stringify(keys))throw Error(`${name}: key mismatch`);for(const key of keys){if(!v[key]||placeholders(v[key])!==placeholders(en[key]))throw Error(`${name}: ${key} mismatch`);}count++;}
console.log(`${count} locales: ${keys.length} keys with matching placeholders.`);
