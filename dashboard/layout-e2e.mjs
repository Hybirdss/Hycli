import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
process.chdir(fileURLToPath(new URL('..',import.meta.url)));
const ports=JSON.parse(await fs.readFile(process.env.HYCLI_FIXTURE_OUTPUT || '.cache/e2e-ports.json','utf8'));
const locales=(await fs.readdir('dashboard/src/locales')).filter(f=>f.endsWith('.json')).map(f=>f.slice(0,-5));
const browser=process.env.TEST_CDP?await chromium.connectOverCDP(process.env.TEST_CDP):await chromium.launch({headless:true,executablePath:process.env.BROWSER_BIN,args:['--no-sandbox','--disable-gpu','--disable-dev-shm-usage'],timeout:120000});
const context=await browser.newContext({viewport:{width:1536,height:1024}});const page=await context.newPage();page.setDefaultTimeout(120000);const errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
 await page.goto(ports.dashboard+'/?launch='+ports.launch,{waitUntil:'domcontentloaded',timeout:120000});await page.locator('.site-card').first().waitFor();
 for(const locale of locales){const dictionary=JSON.parse(await fs.readFile(`dashboard/src/locales/${locale}.json`,'utf8'));if(await page.locator('.topbar select').inputValue()===locale){console.log('Layout:',locale);continue;}const saved=page.waitForResponse(r=>r.url().endsWith('/api/settings')&&r.request().method()==='PATCH');await page.locator('.topbar select').selectOption(locale);assert.equal((await saved).status(),200);await page.getByRole('heading',{name:dictionary['sites.title'],exact:true}).waitFor();assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true,`${locale}: desktop overflow`);assert.equal(await page.locator('html').getAttribute('dir'),locale==='ar'?'rtl':'ltr');console.log('Layout:',locale);}
 await page.locator('.topbar select').selectOption('en');const en=JSON.parse(await fs.readFile('dashboard/src/locales/en.json','utf8'));
 await page.getByRole('button',{name:en['nav.settings'],exact:true}).click();await page.getByRole('button',{name:en['settings.themeDark'],exact:true}).click();await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark');if(process.env.HYCLI_NO_SCREENSHOTS!=='1'){const storage=await page.locator('.settings-section').nth(4).boundingBox();await page.screenshot({animations:'disabled',path:'docs/design/settings-dark.png',clip:{x:0,y:0,width:1536,height:Math.min(1024,Math.floor(storage.y)-16)},timeout:120000});}
 await page.getByRole('button',{name:en['settings.themeLight'],exact:true}).click();await page.waitForFunction(()=>document.documentElement.dataset.theme==='light');await page.getByRole('button',{name:en['nav.websites'],exact:true}).click();
 await page.setViewportSize({width:390,height:844});await page.locator('.prepare-with select').waitFor({state:'visible'});if(process.env.HYCLI_NO_SCREENSHOTS!=='1')await page.screenshot({animations:'disabled',path:'docs/design/websites-mobile.png',fullPage:true,timeout:120000});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true,'mobile overflow');
 assert.deepEqual(errors,[]);console.log(JSON.stringify({locales:locales.length,dark:'passed',mobile:'passed',consoleErrors:0}));
}finally{await context.close();if(!process.env.TEST_CDP)await browser.close();}
if(process.env.TEST_CDP)process.exit(0);
