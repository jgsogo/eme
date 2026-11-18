const $=id=>document.getElementById(id);
const state={weekStart:null,plan:null,members:[],recipes:[],menus:[],editingId:null};
const meals=[["breakfast","Breakfast"],["lunch","Lunch"],["dinner","Dinner"]];
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const text=await response.text();let value=null;try{value=text?JSON.parse(text):null}catch{value=text}if(!response.ok){const error=new Error(typeof value==="string"?value:value?.error||response.statusText);error.status=response.status;throw error}return value}
function esc(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function localDate(date){return [date.getFullYear(),String(date.getMonth()+1).padStart(2,"0"),String(date.getDate()).padStart(2,"0")].join("-")}
function parseDate(value){const [y,m,d]=value.split("-").map(Number);return new Date(y,m-1,d,12)}
function mondayOf(date){const d=new Date(date.getFullYear(),date.getMonth(),date.getDate(),12);d.setDate(d.getDate()-(d.getDay()+6)%7);return d}
function addDays(date,n){const d=new Date(date);d.setDate(d.getDate()+n);return d}
function dateLabel(value,options={weekday:"short",day:"numeric",month:"short"}){return parseDate(value).toLocaleDateString(undefined,options)}
function weekDates(){return Array.from({length:7},(_,i)=>localDate(addDays(parseDate(state.weekStart),i)))}
function recipeTitle(id){return state.recipes.find(r=>r.id===id)?.title||"Recipe unavailable"}
function menuFor(date){return state.menus.find(menu=>menu.draft.date===date)}
function renderMembers(){$("member-list").innerHTML=state.members.filter(m=>m.active).map(m=>'<span class="member-chip">'+esc(m.name)+'</span>').join("")||'<span class="hint">Add household members to start assigning meals.</span>'}
function renderGrid(){
 const plan=state.plan;
 if(!plan){$("planner-grid").innerHTML='<p class="empty">No weekly plan exists.</p>';return}
 const editable=plan.status==="draft";
 $("plan-status").textContent="Revision "+plan.revision+" · "+(editable?"Draft":"Confirmed");
 $("confirm-plan").hidden=!editable;
 $("revise-plan").hidden=editable;
 $("planner-grid").innerHTML=weekDates().map(date=>{
  const dayAssignments=plan.assignments.filter(a=>a.date===date);
  return '<article class="planner-day"><header class="planner-day-heading"><strong>'+esc(dateLabel(date,{weekday:"long"}))+'</strong><span>'+esc(dateLabel(date,{day:"numeric",month:"short"}))+'</span></header>'+
   meals.map(([meal,label])=>{
    const items=dayAssignments.filter(a=>a.meal_type===meal);
    return '<section class="planner-meal"><div class="planner-meal-heading"><h3>'+label+'</h3>'+(editable?'<button type="button" class="text-button" data-add="'+date+'" data-meal="'+meal+'">＋ Add</button>':'')+'</div>'+
     (items.length?items.map(a=>{
      const people=a.participants.map(p=>p.name).join(", ");
      let extra="";
      if(a.arrangement==="home"&&a.recipe_id)extra='<div class="planner-extra">'+esc(recipeTitle(a.recipe_id))+'</div>';
      if(a.arrangement==="school"){
       const menu=menuFor(date);
       const status=menu?.draft?.status;
       const dishes=menu?.draft?.items?.map(i=>i.name).join(" · ");
       extra='<div class="planner-extra">'+(menu?(status==="known"?esc(dishes||"Menu recorded"):("School menu: "+esc(status))):"No school menu recorded")+'</div>';
      }
      return '<div class="planner-assignment"><div><strong>'+esc(people)+'</strong><span class="arrangement-label">'+esc(a.arrangement.replaceAll("_"," "))+'</span>'+extra+(a.notes?'<p class="hint">'+esc(a.notes)+'</p>':'')+'</div>'+(editable?'<div class="assignment-actions"><button type="button" class="text-button" data-edit="'+a.id+'">Edit</button><button type="button" class="text-button danger-text" data-delete="'+a.id+'">Remove</button></div>':'')+'</div>'
     }).join(""):'<p class="planner-empty">No assignment</p>')+'</section>'
   }).join("")+'</article>'
 }).join("");
}
function populateEditor(assignment=null){
 if(state.plan.status!=="draft")return;
 state.editingId=assignment?.id??null;
 $("editor-title").textContent=assignment?"Edit assignment":"Add meal assignment";
 $("assignment-date").value=assignment?.date||state.weekStart;
 $("assignment-meal").value=assignment?.meal_type||"lunch";
 $("assignment-arrangement").value=assignment?.arrangement||"home";
 $("assignment-notes").value=assignment?.notes||"";
 $("assignment-participants").innerHTML=state.members.filter(m=>m.active).map(m=>'<option value="'+m.id+'">'+esc(m.name)+'</option>').join("");
 const selected=new Set((assignment?.participants||[]).map(p=>String(p.member_id)));
 [...$("assignment-participants").options].forEach(option=>option.selected=selected.has(option.value));
 $("assignment-recipe").innerHTML='<option value="">No recipe selected</option>'+state.recipes.map(r=>'<option value="'+r.id+'">'+esc(r.title)+'</option>').join("");
 $("assignment-recipe").value=assignment?.recipe_id?String(assignment.recipe_id):"";
 $("remove-assignment").hidden=!assignment;
 $("assignment-error").textContent=state.members.some(m=>m.active)?"":"Add a household member before creating assignments.";
 $("assignment-editor").hidden=false;
 updateRecipeField();
 $("assignment-editor").scrollIntoView({behavior:"smooth",block:"start"});
}
function updateRecipeField(){
 const arrangement=$("assignment-arrangement").value;
 $("recipe-field").hidden=arrangement!=="home";
 $("assignment-date").disabled=false;
}
async function savePlan(){
 try{
  state.plan=await api("/meal-planner/plans/"+state.weekStart,{method:"PUT",body:JSON.stringify({assignments:state.plan.assignments})});
  renderGrid();$("assignment-editor").hidden=true;
 }catch(e){$("assignment-error").textContent=e.message}
}
async function loadWeek(){
 $("week-date").value=state.weekStart;
 $("planner-grid").innerHTML='<p class="hint">Loading weekly plan…</p>';
 $("assignment-editor").hidden=true;
 try{
  const [members,recipes]=await Promise.all([api("/meal-planner/members"),api("/meal-planner/recipes")]);
  state.members=members;state.recipes=recipes;
  const dates=weekDates();
  state.menus=await api("/meal-planner/school-menus?from="+dates[0]+"&to="+dates[6]);
  try{state.plan=await api("/meal-planner/plans/"+state.weekStart)}
  catch(e){if(e.status!==404)throw e;state.plan=await api("/meal-planner/plans/"+state.weekStart,{method:"POST"})}
  renderMembers();renderGrid();$("connection").textContent="Connected";
 }catch(e){$("connection").textContent="Connection error";$("planner-grid").innerHTML='<p class="empty">'+esc(e.message)+'</p>'}
}
$("member-form").addEventListener("submit",async event=>{event.preventDefault();const input=$("member-name");const button=event.submitter;button.disabled=true;try{await api("/meal-planner/members",{method:"POST",body:JSON.stringify({name:input.value})});input.value="";state.members=await api("/meal-planner/members");renderMembers()}catch(e){alert(e.message)}finally{button.disabled=false}});
$("planner-grid").addEventListener("click",event=>{
 const add=event.target.closest("[data-add]");if(add){populateEditor({date:add.dataset.add,meal_type:add.dataset.meal,participants:[]});return}
 const edit=event.target.closest("[data-edit]");if(edit){const item=state.plan.assignments.find(a=>a.id===Number(edit.dataset.edit));if(item)populateEditor(item);return}
 const remove=event.target.closest("[data-delete]");if(remove){state.plan.assignments=state.plan.assignments.filter(a=>a.id!==Number(remove.dataset.delete));savePlan()}
});
$("assignment-form").addEventListener("submit",async event=>{
 event.preventDefault();
 const participants=[...$("assignment-participants").selectedOptions].map(option=>{const member=state.members.find(m=>m.id===Number(option.value));return{member_id:member.id,name:member.name}});
 if(!participants.length){$("assignment-error").textContent="Select at least one participant.";return}
 const date=$("assignment-date").value;
 if(!weekDates().includes(date)){$("assignment-error").textContent="Choose a date inside this week.";return}
 const arrangement=$("assignment-arrangement").value;
 const assignment={id:state.editingId??(Math.max(0,...state.plan.assignments.map(a=>a.id))+1),date,meal_type:$("assignment-meal").value,participants,arrangement,recipe_id:arrangement==="home"&&$("assignment-recipe").value?Number($("assignment-recipe").value):null,school_menu_date:arrangement==="school"?date:null,notes:$("assignment-notes").value.trim()||null};
 const button=event.submitter;button.disabled=true;
 try{
  if(state.editingId)state.plan.assignments=state.plan.assignments.map(a=>a.id===state.editingId?assignment:a);
  else state.plan.assignments.push(assignment);
  await savePlan();
 }finally{button.disabled=false}
});
$("assignment-arrangement").addEventListener("change",updateRecipeField);
$("close-editor").addEventListener("click",()=>$("assignment-editor").hidden=true);
$("remove-assignment").addEventListener("click",()=>{if(state.editingId!==null){state.plan.assignments=state.plan.assignments.filter(a=>a.id!==state.editingId);savePlan()}});
$("confirm-plan").addEventListener("click",async()=>{if(!confirm("Confirm this week's plan? Confirmed plans are immutable; later changes require a new revision."))return;try{state.plan=await api("/meal-planner/plans/"+state.weekStart+"/confirm",{method:"POST"});renderGrid()}catch(e){alert(e.message)}});
$("revise-plan").addEventListener("click",async()=>{try{state.plan=await api("/meal-planner/plans/"+state.weekStart+"/revise",{method:"POST"});renderGrid()}catch(e){alert(e.message)}});
$("previous-week").addEventListener("click",()=>{state.weekStart=localDate(addDays(parseDate(state.weekStart),-7));loadWeek()});
$("next-week").addEventListener("click",()=>{state.weekStart=localDate(addDays(parseDate(state.weekStart),7));loadWeek()});
$("week-date").addEventListener("change",()=>{if($("week-date").value){state.weekStart=localDate(mondayOf(parseDate($("week-date").value)));loadWeek()}});
(async()=>{try{await api("/auth/session");state.weekStart=localDate(mondayOf(new Date()));await loadWeek()}catch(e){$("connection").textContent="Sign in required";$("planner-grid").innerHTML='<p class="empty">Sign in to use the weekly planner.</p>'}})();
