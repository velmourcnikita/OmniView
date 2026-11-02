// Product UI screenshots with explicit demonstration data, never a real classroom.
const {createRequire}=require('node:module');
const fs=require('node:fs');const path=require('node:path');const http=require('node:http');
const bundled=createRequire(path.join(process.env.RUNTIME_NODE_MODULES,'package.json'));
const {chromium}=bundled('playwright');
const root=path.resolve(__dirname,'..'),out=path.join(root,'presentation/screenshots');
const files={'/':'web/index.html','/app.js':'web/app.js','/styles.css':'web/styles.css','/AppRegular.ttf':'assets/fonts/AppRegular.ttf','/AppBold.ttf':'assets/fonts/AppBold.ttf'};
const types={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.ttf':'font/ttf'};
let frame=Buffer.alloc(0);
const server=http.createServer((req,res)=>{
 if(req.url.startsWith('/preview/')){res.setHeader('Content-Type','image/png');res.end(frame);return;}
 const file=files[req.url];if(!file){res.writeHead(404);res.end();return;}
 res.setHeader('Content-Type',types[path.extname(file)]);res.end(fs.readFileSync(path.join(root,file)));
});
const prefs={name:'Преподаватель',class_name:'Компьютерный класс',host:'192.168.1.10',low:true,theme:'dark',animations:false};
const names=['Анна','Дмитрий','Алексей','Мария','Иван','София'];
const peers=names.map((name,i)=>({id:'demo'+i,name,machine:'PC-'+String(i+1).padStart(2,'0'),online:i!==5,help:i===0||i===3,sharing:i<3,frame:i<3?{seq:1}:null}));
const at=Date.UTC(2026,10,2,7,30);
const teacher={active:true,invite:'OV2-[демонстрационный код]',address:'192.168.1.10:50000',peers,broadcasting:false,log:[{at,from:'Учитель',text:'Начинаем самостоятельную работу. Если возникнет вопрос, нажми «Нужна помощь».'},{at:at+60000,from:'Анна',text:'Не получается выполнить второе задание.'}]};
const state={version:'0.3.0',arch:'x86',role:'teacher',prefs,teacher,student:null,notice:{id:0,text:''},export_file:''};
(async()=>{
 fs.mkdirSync(out,{recursive:true});await new Promise(r=>server.listen(0,'127.0.0.1',r));
 const browser=await chromium.launch({headless:true,executablePath:'C:/Program Files/BraveSoftware/Brave-Browser/Application/brave.exe'});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000},deviceScaleFactor:1});
  await page.addInitScript(()=>{window.ipc={postMessage(){}};});
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.waitForFunction(()=>typeof window.receiveState==='function');
  await page.evaluate(s=>window.receiveState(s),{...state,teacher:null,prefs:{...prefs,theme:'light'}});await page.evaluate(()=>document.fonts.ready);
  frame=await page.screenshot({path:path.join(out,'screen-example.png')});
  await page.evaluate(s=>window.receiveState(s),state);
  await page.locator('#cards img').first().waitFor({state:'visible'});
  await page.screenshot({path:path.join(out,'demo-overview.png')});
  await page.locator('#cards .card').first().click();
  await page.locator('#detailMessage').fill('Сейчас подойду. Проверь условие второго задания.');
  await page.locator('#studentPanel').screenshot({path:path.join(out,'demo-help.png')});
  await page.locator('[data-close="studentPanel"]').click();
  await page.locator('#messageAll').click();
  await page.locator('#classMessage').fill('Сохраняем результаты работы. На выполнение осталось 5 минут.');
  await page.locator('#messageDialog').screenshot({path:path.join(out,'demo-message.png')});
  await page.locator('[data-close="messageDialog"]').click();
  await page.locator('[data-view="settings"]').click();
  await page.locator('#settingsForm').screenshot({path:path.join(out,'settings.png')});
  const first=await page.locator('.setting-row').nth(0).boundingBox(),second=await page.locator('.setting-row').nth(1).boundingBox();
  await page.screenshot({path:path.join(out,'settings-options.png'),clip:{x:first.x-8,y:first.y-8,width:first.width+16,height:second.y+second.height-first.y+16}});
  await page.locator('[data-view="class"]').click();
  await page.evaluate(s=>window.receiveState(s),{...state,role:'student',teacher:null,prefs:{...prefs,name:'Анна'}});
  await page.locator('#studentName').fill('Анна');
  await page.locator('#joinForm').screenshot({path:path.join(out,'student-join.png')});
  await page.evaluate(s=>window.receiveState(s),{...state,role:'student',teacher:null,prefs:{...prefs,name:'Анна'},student:{connected:true,connecting:false,status:'В классе. Соединение защищено TLS',sharing:false,help:false,messages:[{at,from:'Учитель',text:'Разбираем интерфейс OmniView. Слева показано окно преподавателя.'}],broadcasting:true,broadcast:{seq:1}}});
  await page.locator('#teacherScreen').waitFor({state:'visible'});
  await page.screenshot({path:path.join(out,'demo-student.png')});
  await page.locator('#studentConnected').screenshot({path:path.join(out,'demo-broadcast.png')});
  await page.evaluate(s=>window.receiveState(s),{...state,prefs:{...prefs,theme:'light'}});
  await page.screenshot({path:path.join(out,'demo-light.png')});
  fs.writeFileSync(path.join(out,'README.txt'),'Скриншоты интерфейса OmniView 0.3.0.\nУчастники, сообщения, код и изображения передаваемых экранов — демонстрационные данные.\nСкриншоты сделаны через браузер с тестовым IPC. Рабочая сборка EXE и сетевые функции проверялись отдельно.\n');
  console.log('Presentation screenshots: '+out);
 }finally{await browser.close();server.close();}
})().catch(e=>{console.error(e);server.close();process.exitCode=1;});
