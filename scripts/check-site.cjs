const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const {createRequire}=require('node:module');
const {chromium}=createRequire(path.join(process.env.RUNTIME_NODE_MODULES||path.join(__dirname,'../site/node_modules'),'package.json'))('playwright');
const root=path.resolve(__dirname,'../site/.vitepress/dist');
const server=http.createServer((req,res)=>{
 let pathname=decodeURIComponent(new URL(req.url,'http://localhost').pathname);if(pathname==='/')pathname='/index.html';
 let file=path.resolve(root,'.'+pathname);if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}
 if(!path.extname(file))file+='.html';if(!fs.existsSync(file)){res.writeHead(404).end();return;}
 res.setHeader('Content-Type',({'.html':'text/html;charset=utf-8','.js':'application/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.json':'application/json','.ttf':'font/ttf'})[path.extname(file)]||'application/octet-stream');res.end(fs.readFileSync(file));
});
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const local='C:/Program Files/BraveSoftware/Brave-Browser/Application/brave.exe';const browser=await chromium.launch({headless:true,executablePath:process.env.BROWSER_BINARY||(fs.existsSync(local)?local:undefined)});
 try{const page=await browser.newPage({viewport:{width:1365,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/demo`,{waitUntil:'networkidle'});
  await page.getByRole('button',{name:'Опубликовать вопрос',exact:true}).click();
  await page.locator('.example-answers button').nth(1).click();assert(await page.locator('.example-answers button').first().isDisabled());
  await page.locator('.lesson-example select').selectOption('Ученик 2');await page.locator('.example-answers button').nth(2).click();
  await page.getByRole('button',{name:'Завершить приём',exact:true}).click();assert((await page.locator('.lesson-example').textContent()).includes('Верно: 1'));
  await page.goto(`http://127.0.0.1:${server.address().port}/`,{waitUntil:'networkidle'});
  assert(await page.locator('.vp-doc img[alt="Задания и результаты в OmniView"]').evaluate(e=>e.complete&&e.naturalWidth>0));
  assert.equal(await page.locator('a[href="https://github.com/velmourcnikita/OmniView/actions/workflows/verify.yml"]').count(),1);
  const text=await page.locator('body').innerText();for(const word of ['Rust','WebView2','Google Sans','LIVE DEMO'])assert(!text.includes(word),'Technology label on homepage: '+word);
  assert(text.includes('OmniCONTROL'));assert.deepEqual(errors,[]);
  for(const width of [1365,390]){await page.setViewportSize({width,height:1000});assert(!await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth));}
  fs.mkdirSync(path.resolve(__dirname,'../verification/site'),{recursive:true});await page.screenshot({path:path.resolve(__dirname,'../verification/site/mobile.png'),fullPage:true});
  console.log('Site checks passed: SSR, example, image, branding, homepage text, desktop/mobile width');
 }finally{await browser.close();server.close();}
})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
