"use strict";
let lastPointer=0;
function controlledPeer(){return state?.teacher?.peers.find(p=>p.id===selected);}
function controlSend(events){const p=controlledPeer();if(p?.control?.active&&p.control.request)send('control_input',{id:p.id,request:p.control.request.id,events});}
function pointerPosition(e){const image=$('#detailScreen'),r=image.getBoundingClientRect(),scale=Math.min(r.width/image.naturalWidth,r.height/image.naturalHeight);const w=image.naturalWidth*scale,h=image.naturalHeight*scale;if(!w||!h)return null;const x=(e.clientX-r.left-(r.width-w)/2)/w,y=(e.clientY-r.top-(r.height-h)/2)/h;if(x<0||x>1||y<0||y>1)return null;return {kind:'move',x:Math.round(x*65535),y:Math.round(y*65535)};}
function renderControl(){const p=controlledPeer(),s=state?.student,req=s?.control_request;
 $('#requestControl').disabled=!p?.online||!p?.sharing;$('#requestControl').textContent=p?.control?.active?'Остановить управление':p?.control?.request?'Отменить запрос':'Запросить управление';
 $('#requestControl').classList.toggle('primary',!p?.control?.active);
 $('#controlStatus').textContent=p?.control?.active?'Нажми на экран для ввода. Закрытие карточки завершит помощь.':p?.control?.request?'Ученик увидит запрос и сможет разрешить мышь и клавиатуру.':'Сначала ученик должен включить показ экрана и разрешить управление.';
 $('#controlStateLabel').textContent=p?.control?.active?'Согласие получено':p?.control?.request?'Ждём согласия':'По запросу';
 $('.remote-controls').dataset.status=p?.control?.active?'active':p?.control?.request?'pending':'idle';
 $('#detailScreen').classList.toggle('remote-enabled',Boolean(p?.control?.active));$('#controlTextForm').hidden=!p?.control?.active;
 $('#controlOffer').hidden=!s?.connected||!req;$('#allowControl').disabled=!s?.sharing||Boolean(s?.control_pending);$('#denyControl').disabled=!s?.control_pending;
 $('#allowControl').hidden=Boolean(s?.control_pending);$('#denyControl').hidden=!s?.control_pending;
 $('#controlOfferTitle').textContent=s?.control_active?'Помощь подключена':s?.control_pending?'Подключаем помощь…':'Разрешить управление?';
 $('#controlOfferDescription').textContent=s?.control_active?'Мышь и клавиатура доступны преподавателю. Ты можешь остановить помощь в любой момент.':'Преподаватель сможет нажимать кнопки и вводить текст на этом компьютере.';
 $('#controlOffer').dataset.status=s?.control_active?'active':'pending';
 $('#controlOfferStatus').textContent=s?.control_error||(s?.control_active?'Для нового сеанса потребуется новое согласие.':s?.control_pending?'Esc отменит подключение.':!s?.sharing?'Сначала включи показ экрана.':'Только на время этого сеанса.');
 $('#remoteBanner').hidden=!s?.control_active;document.body.dataset.remote=s?.control_active?'active':'off';
}
const renderBeforeControl=render;render=function(){renderBeforeControl();renderControl();};
const detailsBeforeControl=updateDetails;updateDetails=function(){detailsBeforeControl();renderControl();};
$('#requestControl').addEventListener('click',()=>{const p=controlledPeer();if(p)send('control_request',{id:p.id,on:!p.control?.request});});
$('#allowControl').addEventListener('click',()=>{const request=state?.student?.control_request?.id;if(request)send('control_consent',{on:true,request});});
for(const id of ['denyControl','stopRemote'])$('#'+id).addEventListener('click',()=>send('control_consent',{on:false}));
$('#studentPanel').addEventListener('close',()=>{const p=controlledPeer();if(p?.control?.request)send('control_request',{id:p.id,on:false});});
const remoteImage=$('#detailScreen');
remoteImage.addEventListener('pointerdown',e=>{if(!controlledPeer()?.control?.active)return;const move=pointerPosition(e);if(!move)return;e.preventDefault();remoteImage.focus();if(e.button===0){remoteImage.setPointerCapture(e.pointerId);controlSend([move,{kind:'left',down:true}]);}});
remoteImage.addEventListener('pointermove',e=>{if(!controlledPeer()?.control?.active||performance.now()-lastPointer<80)return;const move=pointerPosition(e);if(move){lastPointer=performance.now();controlSend([move]);}});
remoteImage.addEventListener('pointerup',e=>{if(e.button!==0)return;const move=pointerPosition(e);controlSend([...(move?[move]:[]),{kind:'left',down:false}]);if(remoteImage.hasPointerCapture(e.pointerId))remoteImage.releasePointerCapture(e.pointerId);});
remoteImage.addEventListener('pointercancel',()=>controlSend([{kind:'left',down:false}]));
remoteImage.addEventListener('blur',()=>controlSend([{kind:'left',down:false}]));
remoteImage.addEventListener('contextmenu',e=>{if(!controlledPeer()?.control?.active)return;e.preventDefault();const move=pointerPosition(e);if(move)controlSend([move,{kind:'right_click'}]);});
remoteImage.addEventListener('wheel',e=>{if(!controlledPeer()?.control?.active)return;e.preventDefault();controlSend([{kind:'wheel',steps:e.deltaY>0?-1:1}]);},{passive:false});
remoteImage.addEventListener('keydown',e=>{
 if(!controlledPeer()?.control?.active||e.metaKey)return;
 const special={Backspace:8,Tab:9,Enter:13,Escape:27,' ':32,PageUp:33,PageDown:34,End:35,Home:36,ArrowLeft:37,ArrowUp:38,ArrowRight:39,ArrowDown:40,Insert:45,Delete:46};
 let event=null;if(e.key.length===1&&!e.ctrlKey&&!e.altKey)event={kind:'text',text:e.key};else{let vk=special[e.key]||(/^F([1-9]|1[0-2])$/.test(e.key)?111+Number(e.key.slice(1)):0);if(!vk&&e.key.length===1&&/[a-z0-9]/i.test(e.key))vk=e.key.toUpperCase().charCodeAt(0);if(vk)event={kind:'key',vk,ctrl:e.ctrlKey,alt:e.altKey,shift:e.shiftKey};}
 if(event){e.preventDefault();controlSend([event]);}
});
$('#controlTextForm').addEventListener('submit',e=>{e.preventDefault();const text=$('#controlText').value;if(text){controlSend([{kind:'text',text}]);$('#controlText').value='';}});
