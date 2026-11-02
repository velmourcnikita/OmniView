<script setup lang="ts">
import { computed, ref } from 'vue'
const open=ref(false),closed=ref(false),selected=ref('Ученик 1')
const answers=ref<Record<string,number>>({})
const students=['Ученик 1','Ученик 2','Ученик 3']
const options=['4','8','16','32']
const rows=computed(()=>options.map((text,i)=>({text,count:Object.values(answers.value).filter(a=>a===i).length})))
function start(){answers.value={};open.value=true;closed.value=false}
function answer(i:number){if(open.value&&!closed.value&&answers.value[selected.value]===undefined)answers.value[selected.value]=i}
</script>
<template>
 <section class="lesson-example" aria-label="Пример проверки понимания">
  <p class="example-note">Пример в этом браузере. Сетевое подключение и чужие компьютеры не используются.</p>
  <div class="example-actions"><button @click="start">{{open?'Начать заново':'Опубликовать вопрос'}}</button><button :disabled="!open||closed" @click="closed=true">Завершить приём</button></div>
  <h3>Сколько бит в одном байте?</h3>
  <div v-if="open" class="example-grid">
   <div><label>Окно ученика <select v-model="selected"><option v-for="s in students" :key="s">{{s}}</option></select></label><div class="example-answers"><button v-for="(option,i) in options" :key="option" :disabled="closed||answers[selected]!==undefined" :class="{chosen:answers[selected]===i}" @click="answer(i)">{{option}}</button></div><p role="status">{{closed?'Приём завершён':answers[selected]!==undefined?'Ответ получен':'Выбери один ответ'}}</p></div>
   <div><strong>Ответили {{Object.keys(answers).length}} из 3</strong><div v-for="row in rows" :key="row.text" class="example-bar"><span>{{row.text}}</span><progress :value="row.count" max="3"/><span>{{row.count}}</span></div><p v-if="closed">Правильный ответ: 8. Верно: {{Object.values(answers).filter(a=>a===1).length}}.</p><p v-else>В программе правильный вариант раскрывается ученикам после завершения.</p></div>
  </div>
  <p v-else>Опубликуй вопрос, затем переключай окна учеников и отвечай от каждого.</p>
 </section>
</template>
