const $ = id => document.getElementById(id);
const state = { familyId:"", members:[], recipes:[], menus:[], participation:[], failedWeeks:new Set(), plans:new Map(), month:"", selectedDate:null, editingWeek:null, editingId:null, selectedRecipeIds:new Set(), recipeSearch:"", request:0 };
const meals = [["lunch","Lunch"],["dinner","Dinner"]];
async function api(path, options={}) {
  const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});
  const raw=await response.text(); let body=null; try{body=raw?JSON.parse(raw):null}catch{body=raw}
  if(!response.ok){const error=new Error(typeof body==="string"&&body?body:response.statusText);error.status=response.status;throw error}
  return body;
}
function setConnectionStatus(value){const element=$("connection");if(element)element.textContent=value}
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
function participationDetails(date,meal){
 const configured=state.participation.filter(p=>p.date===date&&p.meal_type===meal&&p.arrangement);
 if(!configured.length)return null;
 const labels={home:"At home",school:"At school",away:"Away"};
 const groups=new Map();
 configured.forEach(p=>{
  if(p.arrangement==="not_participating")return;
  const names=groups.get(p.arrangement)||[];
  names.push(p.member_name);
  groups.set(p.arrangement,names);
 });
 if(!groups.size)return "Nobody is participating";
 return [...groups].map(([arrangement,names])=>(labels[arrangement]||arrangement)+": "+names.join(", ")).join(" · ");
}
function renderSchoolMenu(date){
 if(state.failedWeeks.has(weekStartFor(date)))return '<p class="meal-school-menu-placeholder">School menu data unavailable</p>';
 const entry=state.menus.find(menu=>menu.date===date);
 if(!entry)return '<p class="meal-school-menu-placeholder">No school menu recorded</p>';
 if(entry.status!=="known"){
  const label=({not_published:"Menu not published",closed:"School closed",unknown:"Unknown"})[entry.status]||entry.status;
  return '<p class="meal-school-menu-placeholder">'+esc(label)+'</p>';
 }
 const items=(entry.items||[]).map(item=>item.name).filter(Boolean);
 return items.length?'<ul class="meal-school-menu-items">'+items.map(name=>'<li>'+esc(name)+'</li>').join("")+'</ul>':'<p class="meal-school-menu-placeholder">No dishes recorded</p>';
}
function renderWeek(){
 const monday=mondayOf(parseDate($("week-date").value));
 const dates=Array.from({length:7},(_,i)=>dateString(addDays(monday,i)));
 const range=labelDate(dates[0],{day:"numeric",month:"long"})+" – "+labelDate(dates[6],{day:"numeric",month:"long",year:"numeric"});
 const days=dates.map(date=>{
  const slots=meals.map(([meal,title])=>{
   const assignment=assignmentFor(date,meal),configuredPeople=participationDetails(date,meal);
   const people=assignment?.participants?.map(p=>p.name).join(", ")||configuredPeople||"No participation configured";
   const recipes=recipeNames(assignment||{}).join(" · "),failed=state.failedWeeks.has(weekStartFor(date));
   const detail=failed?'<p class="school-menu-placeholder">Planning data unavailable</p>':assignment||configuredPeople?'<div class="meal-slot-details"><strong>'+esc(people)+'</strong>'+(recipes?'<span>'+esc(recipes)+'</span>':'')+(!assignment?'<span>Meal not planned</span>':'')+'</div>':'<p class="school-menu-placeholder">Not planned · No participation configured</p>';
   const action=failed?'<button type="button" class="text-button" data-retry>Retry</button>':assignment?'<button type="button" class="text-button" data-edit="'+date+'" data-meal="'+meal+'">Edit</button>':'<button type="button" class="text-button" data-add="'+date+'" data-meal="'+meal+'">＋ Add</button>';
   return '<section class="meal-calendar-slot"><div class="meal-calendar-slot-heading"><h3>'+title+'</h3>'+action+'</div>'+detail+'</section>';
  }).join("");
  return '<article class="school-menu-day-detail meal-planner-day"><header class="school-menu-day-detail-heading"><div><h3>'+esc(labelDate(date,{weekday:"long",day:"numeric",month:"long"}))+'</h3></div></header><div class="meal-day-slots">'+slots+'</div><section class="meal-school-menu"><h3>School menu</h3>'+renderSchoolMenu(date)+'</section></article>';
 }).join("");
 $("planner-week-view").innerHTML='<div class="school-menu-week-heading"><h3>'+esc(range)+'</h3><p class="hint">Monday to Sunday</p></div><div class="school-menu-week-days">'+days+'</div>';
 $("planner-week-view").querySelectorAll("[data-add],[data-edit],[data-retry]").forEach(button=>button.addEventListener("click",()=>{
  if(button.hasAttribute("data-retry")){loadWeek();return}
  const date=button.dataset.add||button.dataset.edit;
  openEditor(date,button.dataset.meal,assignmentFor(date,button.dataset.meal));
 }));
 const count=[...state.plans.values()].reduce((n,p)=>n+p.assignments.length,0);
 $("plan-status").textContent=count+" planned meal"+(count===1?"":"s");
}
function renderRecipePicker(){
 const query=state.recipeSearch.trim().toLocaleLowerCase();
 const matches=state.recipes.filter(recipe=>recipe.title.toLocaleLowerCase().includes(query));
 $("recipe-selection-count").textContent=state.selectedRecipeIds.size+" selected";
 $("recipe-search-results").textContent=query?matches.length+" matching recipe"+(matches.length===1?"":"s"):"";
 $("recipe-selection-empty").hidden=matches.length>0;
 $("assignment-recipes-list").innerHTML=matches.map(recipe=>{
  const id=String(recipe.id),checked=state.selectedRecipeIds.has(id);
  return '<label class="recipe-option'+(checked?' selected':'')+'"><input type="checkbox" value="'+esc(id)+'"'+(checked?' checked':'')+'><span>'+esc(recipe.title)+'</span></label>';
 }).join("");
}
function openEditor(date,meal,assignment){
 const week=weekStartFor(date),plan=state.plans.get(week);
 state.selectedDate=date;state.editingWeek=week;state.editingId=assignment?.id??null;
 $("editor-title").textContent=(assignment?"Edit ":"Plan ")+(meal==="lunch"?"lunch":"dinner");
 $("assignment-date").value=date;$("assignment-meal").value=meal;
 $("assignment-participants").innerHTML=state.members.filter(m=>m.active).map(m=>'<option value="'+m.id+'">'+esc(m.name)+'</option>').join("");
 const selected=new Set((assignment?.participants||[]).map(p=>String(p.member_id)));
 if(!assignment)state.participation.filter(p=>p.date===date&&p.meal_type===meal&&p.arrangement==="home").forEach(p=>selected.add(String(p.member_id)));
 [...$("assignment-participants").options].forEach(o=>o.selected=selected.has(o.value));
 state.selectedRecipeIds=new Set((assignment?.recipe_ids||[]).map(String));
 state.recipeSearch="";
 $("recipe-search").value="";
 renderRecipePicker();
 $("assignment-error").textContent="";
 $("remove-assignment").hidden=!assignment;
 $("assignment-editor").showModal();
}
async function persistAssignment(remove=false){
 const week=state.editingWeek;
 try{
  const plan=state.plans.get(week)||{assignments:[]};
  let assignments=plan.assignments.filter(a=>a.id!==state.editingId);
  if(!remove){
   const date=$("assignment-date").value,meal=$("assignment-meal").value;
   if(weekStartFor(date)!==week){$("assignment-error").textContent="The date must remain in the selected week.";return}
   const participants=[...$("assignment-participants").selectedOptions].map(o=>{const m=state.members.find(x=>x.id===Number(o.value));return{member_id:m.id,name:m.name}});
   if(!participants.length){$("assignment-error").textContent="Select at least one family member.";return}
   const recipeIds=[...state.selectedRecipeIds].map(Number);
   const existing=assignments.find(a=>a.date===date&&a.meal_type===meal);
   if(existing){$("assignment-error").textContent="This meal already has an entry. Edit that entry instead.";return}
   assignments.push({id:Math.max(0,...plan.assignments.map(a=>a.id))+1,date,meal_type:meal,participants,arrangement:"home",recipe_ids:recipeIds,school_menu_date:null,notes:null});
  }
  const updated=await api("/meal-planner/plans/"+week+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({assignments})});
  state.plans.set(week,updated);$("assignment-editor").close();renderWeek();
 }catch(e){$("assignment-error").textContent=e.message}
}
async function loadWeek(){
 const request=++state.request;
 const monday=dateString(mondayOf(parseDate($("week-date").value)));
 $("week-date").value=monday;
 $("planner-week-view").innerHTML='<p class="hint">Loading meal planner…</p>';
 if($("assignment-editor").open)$("assignment-editor").close();
 try{
  state.familyId=window.emeMealsFamily.currentFamilyId();
  if(!state.familyId){state.members=[];state.recipes=[];state.menus=[];state.participation=[];state.failedWeeks.clear();state.plans.clear();$("planner-week-view").innerHTML='<p class="empty">Create or join a family to plan meals. <a href="/console/families">Manage families</a>.</p>';return}
  const context=await api("/meal-planner/context/"+monday+"?family_id="+encodeURIComponent(state.familyId));
  if(request!==state.request)return;
  state.failedWeeks.clear();state.plans.clear();
  state.members=(context.members||[]).filter(member=>member.active);
  state.recipes=context.recipes||[];
  state.menus=context.school_menus||[];
  state.participation=context.participation||[];
  if(context.current_plan)state.plans.set(monday,context.current_plan);
  renderWeek();
  setConnectionStatus("Connected");
 }catch(e){if(request===state.request){state.failedWeeks=new Set([monday]);$("planner-week-view").innerHTML='<p class="empty" role="alert">'+esc(e.message)+' <button type="button" class="text-button" id="retry-week">Retry</button></p>';$("retry-week")?.addEventListener("click",loadWeek);setConnectionStatus("Connection error")}}
}
$("recipe-search").addEventListener("input",e=>{state.recipeSearch=e.target.value;renderRecipePicker()});
$("assignment-recipes-list").addEventListener("change",e=>{
 const input=e.target;
 if(input.matches("input[type=checkbox]")){
  if(input.checked)state.selectedRecipeIds.add(input.value);
  else state.selectedRecipeIds.delete(input.value);
  input.closest(".recipe-option").classList.toggle("selected",input.checked);
  $("recipe-selection-count").textContent=state.selectedRecipeIds.size+" selected";
 }
});
$("assignment-form").addEventListener("submit",e=>{e.preventDefault();persistAssignment(false)});
$("remove-assignment").addEventListener("click",()=>persistAssignment(true));
$("close-editor").addEventListener("click",()=>$("assignment-editor").close());
$("previous-week").addEventListener("click",()=>{$("week-date").value=dateString(addDays(parseDate($("week-date").value),-7));loadWeek()});
$("next-week").addEventListener("click",()=>{$("week-date").value=dateString(addDays(parseDate($("week-date").value),7));loadWeek()});
$("week-date").addEventListener("change",loadWeek);
window.addEventListener("eme:family-changed",()=>loadWeek());
window.addEventListener("focus",()=>loadWeek());
document.addEventListener("visibilitychange",()=>{if(document.visibilityState==="visible")loadWeek()});
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();$("week-date").value=dateString(mondayOf(new Date()));await loadWeek()}catch(e){setConnectionStatus("Error loading planner: "+e.message);const view=$("planner-week-view");if(view)view.innerHTML='<p class="empty" role="alert">'+esc(e.message)+'</p>'}finally{$("planner-week-view")?.closest("main")?.removeAttribute("hidden")}})();
