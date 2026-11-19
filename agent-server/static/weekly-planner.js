const $ = id => document.getElementById(id);
const state = { familyId:"", members:[], recipes:[], menus:[], plans:new Map(), month:"", selectedDate:null, editingWeek:null, editingId:null, request:0, showWeekends:true };
const meals = [["lunch","Lunch"],["dinner","Dinner"]];
async function api(path, options={}) {
  const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});
  const raw=await response.text(); let body=null; try{body=raw?JSON.parse(raw):null}catch{body=raw}
  if(!response.ok){const error=new Error(typeof body==="string"&&body?body:response.statusText);error.status=response.status;throw error}
  return body;
}
function esc(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function dateString(d){return [d.getFullYear(),String(d.getMonth()+1).padStart(2,"0"),String(d.getDate()).padStart(2,"0")].join("-")}
function parseDate(s){const [y,m,d]=s.split("-").map(Number);return new Date(y,m-1,d,12)}
function monthStart(s){return new Date(Number(s.slice(0,4)),Number(s.slice(5,7))-1,1,12)}
function monthString(d){return d.getFullYear()+"-"+String(d.getMonth()+1).padStart(2,"0")}
function mondayOf(d){const x=new Date(d);x.setDate(x.getDate()-(x.getDay()+6)%7);return x}
function addDays(d,n){const x=new Date(d);x.setDate(x.getDate()+n);return x}
function labelDate(s,opts={weekday:"short",day:"numeric",month:"short"}){return parseDate(s).toLocaleDateString(undefined,opts)}
function weekStartFor(s){return dateString(mondayOf(parseDate(s)))}
function planFor(date){return state.plans.get(weekStartFor(date))||null}
function assignmentFor(date,meal){return planFor(date)?.assignments.find(a=>a.date===date&&a.meal_type===meal)||null}
function recipeNames(a){return (a.recipe_ids||[]).map(id=>state.recipes.find(r=>r.id===id)?.title||"Recipe unavailable")}
function renderSchoolMenu(date){
 const entry=state.menus.find(menu=>menu.date===date);
 if(!entry)return '<p class="meal-school-menu-placeholder">No school menu recorded</p>';
 if(entry.status!=="known"){
  const label=({not_published:"Menu not published",closed:"School closed",unknown:"Unknown"})[entry.status]||entry.status;
  return '<p class="meal-school-menu-placeholder">'+esc(label)+'</p>';
 }
 const items=(entry.items||[]).map(item=>item.name).filter(Boolean);
 return items.length?'<ul class="meal-school-menu-items">'+items.map(name=>'<li>'+esc(name)+'</li>').join("")+'</ul>':'<p class="meal-school-menu-placeholder">No dishes recorded</p>';
}
function renderMonth(){
 const first=monthStart(state.month),year=first.getFullYear(),month=first.getMonth(),days=new Date(year,month+1,0).getDate();
 const offset=(first.getDay()+6)%7,visibleOffset=state.showWeekends?offset:Math.min(offset,5),cells=[];
 for(let i=0;i<visibleOffset;i++)cells.push('<div class="school-menu-calendar-empty" aria-hidden="true"></div>');
 for(let day=1;day<=days;day++){
  const date=dateString(new Date(year,month,day,12)),weekday=parseDate(date).getDay();
  if(!state.showWeekends&&(weekday===0||weekday===6))continue;
  const slots=meals.map(([meal,title])=>{
   const assignment=assignmentFor(date,meal);
   const people=assignment?.participants?.map(p=>p.name).join(", ")||"No participants";
   const recipes=recipeNames(assignment||{}).join(" · ");
   const detail=assignment?'<div class="meal-slot-details"><strong>'+esc(people)+'</strong>'+(recipes?'<span>'+esc(recipes)+'</span>':'')+'</div>':'<p class="school-menu-placeholder">Not planned</p>';
   const action=assignment?'<button type="button" class="text-button" data-edit="'+date+'" data-meal="'+meal+'">Edit</button>':'<button type="button" class="text-button" data-add="'+date+'" data-meal="'+meal+'">＋ Add</button>';
   return '<section class="meal-calendar-slot"><div class="meal-calendar-slot-heading"><h3>'+title+'</h3>'+action+'</div>'+detail+'</section>';
  }).join("");
  const plan=planFor(date);
  cells.push('<article class="school-menu-day meal-planner-day"><header class="school-menu-day-heading"><strong>'+parseDate(date).getDate()+'</strong><span>'+esc(labelDate(date,{weekday:"short"}))+'</span></header><div class="meal-day-slots">'+slots+'</div><section class="meal-school-menu"><h3>School menu</h3>'+renderSchoolMenu(date)+'</section>'+'</article>');
 }
 const calendar=$("planner-month-view");calendar.classList.toggle("weekdays-only",!state.showWeekends);
 calendar.innerHTML='<div class="school-menu-calendar-weekdays">'+["Mon","Tue","Wed","Thu","Fri","Sat","Sun"].slice(0,state.showWeekends?7:5).map(d=>'<span>'+d+'</span>').join("")+'</div><div class="school-menu-month">'+cells.join("")+'</div>';
 calendar.querySelectorAll("[data-add],[data-edit]").forEach(button=>button.addEventListener("click",()=>{
  const date=button.dataset.add||button.dataset.edit,meal=button.dataset.meal;
  openEditor(date,meal,assignmentFor(date,meal));
 }));
 const count=[...state.plans.values()].reduce((n,p)=>n+p.assignments.length,0);
 $("plan-status").textContent=count+" planned meal"+(count===1?"":"s")+" · "+state.plans.size+" weekly plan(s) loaded";
}
function openEditor(date,meal,assignment){
 const week=weekStartFor(date),plan=state.plans.get(week);
 state.selectedDate=date;state.editingWeek=week;state.editingId=assignment?.id??null;
 $("editor-title").textContent=(assignment?"Edit ":"Plan ")+(meal==="lunch"?"lunch":"dinner");
 $("assignment-date").value=date;$("assignment-meal").value=meal;
 $("assignment-participants").innerHTML=state.members.filter(m=>m.active).map(m=>'<option value="'+m.id+'">'+esc(m.name)+'</option>').join("");
 const selected=new Set((assignment?.participants||[]).map(p=>String(p.member_id)));
 [...$("assignment-participants").options].forEach(o=>o.selected=selected.has(o.value));
 $("assignment-recipes").innerHTML=state.recipes.map(r=>'<option value="'+r.id+'">'+esc(r.title)+'</option>').join("");
 const recipeIds=new Set((assignment?.recipe_ids||[]).map(String));
 [...$("assignment-recipes").options].forEach(o=>o.selected=recipeIds.has(o.value));
 $("assignment-error").textContent="";
 $("remove-assignment").hidden=!assignment;
 $("assignment-editor").showModal();
}
async function ensureDraft(week){
 let plan=state.plans.get(week);
 if(!plan){
  plan=await api("/meal-planner/plans/"+week+"?family_id="+encodeURIComponent(state.familyId),{method:"POST",body:JSON.stringify({assignments:[]})});
  state.plans.set(week,plan);
 }
 return plan;
}
async function persistAssignment(remove=false){
 const week=state.editingWeek;
 try{
  const plan=await ensureDraft(week);
  let assignments=plan.assignments.filter(a=>a.id!==state.editingId);
  if(!remove){
   const date=$("assignment-date").value,meal=$("assignment-meal").value;
   if(weekStartFor(date)!==week){$("assignment-error").textContent="The date must remain in the selected week.";return}
   const participants=[...$("assignment-participants").selectedOptions].map(o=>{const m=state.members.find(x=>x.id===Number(o.value));return{member_id:m.id,name:m.name}});
   if(!participants.length){$("assignment-error").textContent="Select at least one family member.";return}
   const recipeIds=[...$("assignment-recipes").selectedOptions].map(o=>Number(o.value));
   const existing=assignments.find(a=>a.date===date&&a.meal_type===meal);
   if(existing){$("assignment-error").textContent="This meal already has an entry. Edit that entry instead.";return}
   assignments.push({id:Math.max(0,...plan.assignments.map(a=>a.id))+1,date,meal_type:meal,participants,arrangement:"home",recipe_ids:recipeIds,school_menu_date:null,notes:null});
  }
  const updated=await api("/meal-planner/plans/"+week+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({assignments})});
  state.plans.set(week,updated);$("assignment-editor").close();renderMonth();
 }catch(e){$("assignment-error").textContent=e.message}
}
async function loadMonth(){
 const request=++state.request;state.month=$("month-date").value;
 if(!state.month)return;
 $("planner-month-view").innerHTML='<p class="hint">Loading meal planner…</p>';
 if($("assignment-editor").open)$("assignment-editor").close();
 try{
  state.familyId=window.emeMealsFamily.currentFamilyId();
  if(!state.familyId){state.plans.clear();$("planner-month-view").innerHTML='<p class="empty">Create or join a family to plan meals. <a href="/console/families">Manage families</a>.</p>';return}
  const [members,recipes]=await Promise.all([
   api("/meal-planner/members?family_id="+encodeURIComponent(state.familyId)),
   api("/meal-planner/recipes?family_id="+encodeURIComponent(state.familyId))
  ]);
  if(request!==state.request)return;state.members=members;state.recipes=recipes;state.plans.clear();
  const first=monthStart(state.month),last=new Date(first.getFullYear(),first.getMonth()+1,0,12);
  const from=mondayOf(first),to=mondayOf(last);
  const weekStarts=[];for(let d=from;d<=to;d=addDays(d,7))weekStarts.push(dateString(d));
  const results=await Promise.all(weekStarts.map(async week=>{
   try{return await api("/meal-planner/plans/"+week+"?family_id="+encodeURIComponent(state.familyId))}
   catch(e){if(e.status===404)return null;throw e}
  }));
  if(request!==state.request)return;
  results.forEach((plan,i)=>{if(plan)state.plans.set(weekStarts[i],plan)});
  const menus=await api("/meal-planner/school-menus?family_id="+encodeURIComponent(state.familyId)+"&from="+dateString(first)+"&to="+dateString(last));
  if(request!==state.request)return;state.menus=menus;
  renderMonth();$("connection").textContent="Connected";
 }catch(e){if(request===state.request){$("planner-month-view").innerHTML='<p class="empty" role="alert">'+esc(e.message)+'</p>';$("connection").textContent="Connection error"}}
}
$("assignment-form").addEventListener("submit",e=>{e.preventDefault();persistAssignment(false)});
$("remove-assignment").addEventListener("click",()=>persistAssignment(true));
$("close-editor").addEventListener("click",()=>$("assignment-editor").close());
$("previous-month").addEventListener("click",()=>{const d=monthStart($("month-date").value);d.setMonth(d.getMonth()-1);$("month-date").value=monthString(d);loadMonth()});
$("next-month").addEventListener("click",()=>{const d=monthStart($("month-date").value);d.setMonth(d.getMonth()+1);$("month-date").value=monthString(d);loadMonth()});
$("month-date").addEventListener("change",loadMonth);
$("show-weekends").addEventListener("change",e=>{state.showWeekends=e.target.checked;try{localStorage.setItem("eme-meal-planner-show-weekends",String(state.showWeekends))}catch{}renderMonth()});
try{state.showWeekends=localStorage.getItem("eme-meal-planner-show-weekends")!=="false"}catch{}
$("show-weekends").checked=state.showWeekends;
window.addEventListener("eme:family-changed",()=>loadMonth());
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();$("month-date").value=monthString(new Date());await loadMonth()}catch(e){$("connection").textContent="Error loading planner: "+e.message}})();
