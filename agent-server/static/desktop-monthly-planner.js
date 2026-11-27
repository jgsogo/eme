const $ = id => document.getElementById(id);
const state = { familyId:"", members:[], recipes:[], menus:[], participation:[], failedWeeks:new Set(), plans:new Map(), month:"", selectedDate:null, editingWeek:null, editingId:null, selectedRecipeIds:new Set(), recipeSearch:"", request:0, showWeekends:true };
const meals = [["lunch","Lunch"],["dinner","Dinner"]];
async function api(path, options={}) {
  const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});
  const raw=await response.text(); let body=null; try{body=raw?JSON.parse(raw):null}catch{body=raw}
  if(!response.ok){const error=new Error(typeof body==="string"&&body?body:response.statusText);error.status=response.status;throw error}
  return body;
}
function setConnectionStatus(value){const element=$("connection");if(element){element.textContent=value;element.className=value.toLowerCase().includes("error")?"hint error":"hint"}}
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
function renderMonth(){
 const first=monthStart(state.month),year=first.getFullYear(),month=first.getMonth(),days=new Date(year,month+1,0).getDate();
 const offset=(first.getDay()+6)%7,visibleOffset=state.showWeekends?offset:Math.min(offset,5),cells=[];
 for(let i=0;i<visibleOffset;i++)cells.push('<div class="school-menu-calendar-empty" aria-hidden="true"></div>');
 for(let day=1;day<=days;day++){
  const date=dateString(new Date(year,month,day,12)),weekday=parseDate(date).getDay();
  if(!state.showWeekends&&(weekday===0||weekday===6))continue;
  const slots=meals.map(([meal,title])=>{
   const assignment=assignmentFor(date,meal);
   const configuredPeople=participationDetails(date,meal);
   const people=assignment?.participants?.map(p=>p.name).join(", ")||configuredPeople||"No participation configured";
   const recipes=recipeNames(assignment||{}).join(" · ");
   const detail=state.failedWeeks.has(weekStartFor(date))?'<p class="school-menu-placeholder">Planning data unavailable</p>':assignment||configuredPeople?'<div class="meal-slot-details"><strong>'+esc(people)+'</strong>'+(recipes?'<span>'+esc(recipes)+'</span>':'')+(!assignment?'<span>Meal not planned</span>':'')+'</div>':'<p class="school-menu-placeholder">Not planned · No participation configured</p>';
   const failed=state.failedWeeks.has(weekStartFor(date));
   const action=failed?'<button type="button" class="text-button" data-retry>Retry</button>':assignment?'<button type="button" class="text-button" data-edit="'+date+'" data-meal="'+meal+'">Edit</button>':'<button type="button" class="text-button" data-add="'+date+'" data-meal="'+meal+'">＋ Add</button>';
   return '<section class="meal-calendar-slot"><div class="meal-calendar-slot-heading"><h3>'+title+'</h3>'+action+'</div>'+detail+'</section>';
  }).join("");
  const plan=planFor(date);
  cells.push('<article class="school-menu-day meal-planner-day"><header class="school-menu-day-heading"><strong>'+parseDate(date).getDate()+'</strong><span>'+esc(labelDate(date,{weekday:"short"}))+'</span></header><div class="meal-day-slots">'+slots+'</div><section class="meal-school-menu"><h3>School menu</h3>'+renderSchoolMenu(date)+'</section>'+'</article>');
 }
 const calendar=$("planner-month-view");calendar.classList.toggle("weekdays-only",!state.showWeekends);
 calendar.innerHTML='<div class="school-menu-calendar-weekdays">'+["Mon","Tue","Wed","Thu","Fri","Sat","Sun"].slice(0,state.showWeekends?7:5).map(d=>'<span>'+d+'</span>').join("")+'</div><div class="school-menu-month">'+cells.join("")+'</div>';
 calendar.querySelectorAll("[data-add],[data-edit],[data-retry]").forEach(button=>button.addEventListener("click",()=>{
  if(button.hasAttribute("data-retry")){loadMonth();return}
  const date=button.dataset.add||button.dataset.edit,meal=button.dataset.meal;
  openEditor(date,meal,assignmentFor(date,meal));
 }));
 const count=[...state.plans.values()].reduce((n,p)=>n+p.assignments.length,0);
 $("plan-status").textContent=count+" planned meal"+(count===1?"":"s")+" · "+state.plans.size+" weekly plan(s) loaded";
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
  if(!state.familyId){state.members=[];state.recipes=[];state.menus=[];state.participation=[];state.failedWeeks.clear();state.plans.clear();$("planner-month-view").innerHTML='<p class="empty">Create or join a family to plan meals. <a href="/console/families">Manage families</a>.</p>';return}
  const first=monthStart(state.month),last=new Date(first.getFullYear(),first.getMonth()+1,0,12);
  const from=mondayOf(first),to=mondayOf(last);
  const weekStarts=[];for(let d=from;d<=to;d=addDays(d,7))weekStarts.push(dateString(d));
  const results=await Promise.allSettled(weekStarts.map(week=>
   api("/meal-planner/context/"+week+"?family_id="+encodeURIComponent(state.familyId))
  ));
  if(request!==state.request)return;
  const contexts=results.map(result=>result.status==="fulfilled"?result.value:null);
  const failed=results.filter(result=>result.status==="rejected");
  state.failedWeeks=new Set(results.flatMap((result,i)=>result.status==="rejected"?[weekStarts[i]]:[]));
  if(failed.length===results.length)throw failed[0].reason;
  const firstContext=contexts.find(Boolean);
  state.members=(firstContext?.members||[]).filter(member=>member.active);
  state.recipes=firstContext?.recipes||[];
  state.plans.clear();
  state.participation=contexts.flatMap(context=>context?.participation||[]);
  state.menus=contexts.flatMap(context=>context?.school_menus||[]);
  contexts.forEach((context,i)=>{if(context?.current_plan)state.plans.set(weekStarts[i],context.current_plan)});
  renderMonth();
  setConnectionStatus(failed.length?("Partially connected · "+failed.length+" week(s) failed to load"):"Connected");
 }catch(e){if(request===state.request){$("planner-month-view").innerHTML='<p class="empty" role="alert">'+esc(e.message)+'</p>';setConnectionStatus("Connection error")}}
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
$("previous-month").addEventListener("click",()=>{const d=monthStart($("month-date").value);d.setMonth(d.getMonth()-1);$("month-date").value=monthString(d);loadMonth()});
$("next-month").addEventListener("click",()=>{const d=monthStart($("month-date").value);d.setMonth(d.getMonth()+1);$("month-date").value=monthString(d);loadMonth()});
$("month-date").addEventListener("change",loadMonth);
$("show-weekends").addEventListener("change",e=>{state.showWeekends=e.target.checked;try{localStorage.setItem("eme-meal-planner-show-weekends",String(state.showWeekends))}catch{}renderMonth()});
try{state.showWeekends=localStorage.getItem("eme-meal-planner-show-weekends")!=="false"}catch{}
$("show-weekends").checked=state.showWeekends;
window.addEventListener("eme:family-changed",()=>loadMonth());
window.addEventListener("focus",()=>loadMonth());
document.addEventListener("visibilitychange",()=>{if(document.visibilityState==="visible")loadMonth()});
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();$("month-date").value=monthString(new Date());await loadMonth()}catch(e){setConnectionStatus("Error loading planner: "+e.message);const view=$("planner-month-view");if(view)view.innerHTML='<p class="empty" role="alert">'+esc(e.message)+'</p>'}finally{$("planner-month-view")?.closest("main")?.removeAttribute("hidden")}})();
