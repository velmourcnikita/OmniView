"use strict";
// DOM is assembled with textContent, including question and response text received over the network.
let lessonSignature='', studentQuestionSignature='', lessonBank=[];
try { lessonBank=JSON.parse(localStorage.getItem('omniview.questions')||'[]').slice(0,30); } catch {}
function element(tag,text,cls){const el=document.createElement(tag);if(text!==undefined)el.textContent=text;if(cls)el.className=cls;return el;}
function paintBank(){const select=$('#questionBank');select.replaceChildren(element('option','Выбрать сохранённый вопрос…'));lessonBank.forEach((q,i)=>{const o=element('option',q.prompt);o.value=String(i);select.append(o);});}
function questionDraft(){return {prompt:$('#questionPrompt').value,options:$('#questionOptions').value.split('\n').map(s=>s.trim()).filter(Boolean),correct:Number($('#questionCorrect').value),seconds:Number($('#questionSeconds').value)};}
function renderLesson(){
 const teacher=state.role==='teacher',t=state.teacher,lesson=t?.lesson,questions=lesson?.questions||[],q=questions.at(-1);
 $('#lessonNav').hidden=!teacher;$('#lessonView').hidden=view!=='lesson'||!teacher;
 $('#publishQuestion').disabled=!t?.active||Boolean(q&&!q.closed);$('#closeQuestion').disabled=!q||q.closed;
 $('#lessonReport').disabled=!questions.length&&!t?.assessment?.events?.length;
 $('#questionComposer').hidden=Boolean(q&&!q.closed);$('#questionEmpty').hidden=Boolean(q);$('#questionResults').hidden=!q;
 const signature=JSON.stringify([questions,t?.assessment,t?.peers?.map(p=>[p.id,p.monitoring,p.monitor_error])]);
 if(signature!==lessonSignature){lessonSignature=signature;
  if(q){$('#resultPrompt').textContent=q.prompt;const answers=Object.values(q.answers),right=answers.filter(a=>a.choice===q.correct).length;
   $('#resultSummary').textContent=`${q.closed?'Завершено':'Приём ответов'} · ${answers.length} из ${Object.keys(q.invited).length} ответили · ${right} верных`;
   const bars=$('#answerBars');bars.replaceChildren();q.options.forEach((option,i)=>{const count=answers.filter(a=>a.choice===i).length,row=element('div',undefined,'answer-row');const label=element('div',undefined,'answer-label');label.append(element('span',`${String.fromCharCode(65+i)}. ${option}`),element('strong',`${count}`));const meter=element('progress');meter.max=Math.max(1,Object.keys(q.invited).length);meter.value=count;if(q.closed&&i===q.correct)row.classList.add('correct');row.append(label,meter);bars.append(row);});
   const rows=$('#responseRows');rows.replaceChildren();for(const [id,name] of Object.entries(q.invited)){const a=q.answers[id],tr=element('tr');[name,a?q.options[a.choice]:'Ожидание',a?(a.choice===q.correct?'Верно':'Нужно разобрать'):'—',a?`${(a.elapsed_ms/1000).toFixed(1)} с`:'—'].forEach(text=>tr.append(element('td',text)));rows.append(tr);}
  }
  const history=$('#questionHistory');history.replaceChildren();questions.forEach((item,i)=>{const answers=Object.values(item.answers),right=answers.filter(a=>a.choice===item.correct).length;history.append(element('p',`${i+1}. ${item.prompt} · ${answers.length}/${Object.keys(item.invited).length} ответов · верно ${right}`));});
  const a=t?.assessment,events=$('#assessmentEvents');events.replaceChildren();(a?.events||[]).slice(-30).reverse().forEach(e=>events.append(element('p',`${new Date(e.at).toLocaleTimeString('ru')} · ${e.name} · ${e.executable}`)));
  $('#assessmentState').textContent=a?.policy.active?'Проверка предложена ученикам':'Проверка выключена';
  $('#assessmentStop').disabled=!a?.policy.active;$('#assessmentStart').disabled=!t?.active;
  const consent=$('#monitorParticipants');consent.replaceChildren();(t?.peers||[]).filter(p=>p.online).forEach(p=>consent.append(element('p',`${p.name}: ${p.monitor_error?'ошибка проверки':p.monitoring?'проверка включена':'не подтвердил / отключил'}`)));
 }
 const s=state.student,sq=s?.question;$('#studentQuiz').hidden=!s?.connected||!sq;
 const sqSignature=JSON.stringify([sq,s?.answer_pending]);if(sqSignature!==studentQuestionSignature){studentQuestionSignature=sqSignature;
  if(sq){$('#studentPrompt').textContent=sq.prompt;const choices=$('#studentChoices');choices.replaceChildren();sq.options.forEach((option,i)=>{const b=element('button',`${String.fromCharCode(65+i)}. ${option}`);b.type='button';b.disabled=sq.closed||sq.own_choice!==null||s.answer_pending;b.classList.toggle('chosen',sq.own_choice===i);if(sq.closed&&sq.correct===i)b.classList.add('correct');b.addEventListener('click',()=>send('question_answer',{id:sq.id,choice:i}));choices.append(b);});
   $('#studentAnswerStatus').textContent=sq.closed?(sq.own_choice===null?'Вопрос завершён. Ответ не получен.':sq.correct===sq.own_choice?'Верно. Вопрос завершён.':'Ответ принят. Обсуди решение с учителем.'):sq.own_choice!==null?'Ответ получен учителем':s.answer_pending?'Отправляем ответ…':'Выбери один ответ. Изменить его после отправки нельзя.';
  }
 }
 $('#monitorPanel').hidden=!s?.connected||!s?.policy?.active;
 $('#monitorToggle').checked=Boolean(s?.monitoring);$('#monitorList').textContent=(s?.policy?.blocked||[]).join(', ');
 $('#monitorStatus').textContent=s?.monitor_error|| (s?.monitoring?`Проверка включена. Найдено: ${(s?.detections||[]).map(d=>d.executable).join(', ')||'совпадений нет'}`:'Проверка выключена. Другие процессы не передаются.');
 updateQuestionTimers();
}
function updateQuestionTimers(){for(const [el,q] of [[$('#questionTimer'),state?.teacher?.lesson?.questions?.at(-1)],[$('#studentTimer'),state?.student?.question]]){el.textContent=q&&!q.closed?`Осталось ${Math.max(0,Math.ceil((q.closes_at-Date.now())/1000))} с`:'';}}
setInterval(updateQuestionTimers,1000);
$('#questionForm').addEventListener('submit',e=>{e.preventDefault();send('question_start',questionDraft());});
$('#closeQuestion').addEventListener('click',()=>send('question_close'));
$('#lessonReport').addEventListener('click',()=>send('lesson_report'));
$('#projectorButton').addEventListener('click',()=>{const on=document.body.classList.toggle('projector');$('#projectorButton').textContent=on?'Вернуть имена и управление':'Режим проектора';});
$('#saveQuestion').addEventListener('click',()=>{const draft=questionDraft();if(!draft.prompt.trim()||draft.options.length<2||draft.correct>=draft.options.length){toast('Заполни вопрос, варианты и правильный ответ');return;}lessonBank=lessonBank.filter(q=>q.prompt!==draft.prompt);lessonBank.unshift(draft);lessonBank=lessonBank.slice(0,30);try{localStorage.setItem('omniview.questions',JSON.stringify(lessonBank));paintBank();toast('Вопрос сохранён на этом компьютере');}catch{toast('Не удалось сохранить вопрос');}});
$('#questionBank').addEventListener('change',e=>{const q=lessonBank[Number(e.target.value)];if(!q)return;$('#questionPrompt').value=q.prompt;$('#questionOptions').value=q.options.join('\n');$('#questionCorrect').value=q.correct;$('#questionSeconds').value=q.seconds;});
$('#assessmentForm').addEventListener('submit',e=>{e.preventDefault();send('assessment_configure',{blocked:$('#blockedPrograms').value,active:true});});
$('#assessmentStop').addEventListener('click',()=>send('assessment_configure',{blocked:$('#blockedPrograms').value,active:false}));
$('#monitorToggle').addEventListener('change',e=>send('monitor_consent',{on:e.target.checked}));
paintBank();
