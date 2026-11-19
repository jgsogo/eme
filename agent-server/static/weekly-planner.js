const $=id=>document.getElementById(id);
const state={weekStart:null,plan:null,members:[],recipes:[],menus:[],editingId:null,shoppingList:null,shoppingListNeedsRegeneration:false,familyId:""};
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
  state.plan=await api("/meal-planner/plans/"+state.weekStart+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({assignments:state.plan.assignments})});
  state.shoppingListNeedsRegeneration=Boolean(state.shoppingList);
  renderGrid();renderShoppingList();$("assignment-editor").hidden=true;
 }catch(e){$("assignment-error").textContent=e.message}
}

async function loadShoppingList(){
 if(!state.plan){$("shopping-list-panel").hidden=true;return}
 $("shopping-list-panel").hidden=false;
 $("shopping-list-message").textContent="";
 $("shopping-list-items").innerHTML='<p class="hint">Loading shopping list…</p>';
 try{
  state.shoppingList=await api("/meal-planner/plans/"+state.plan.id+"/shopping-list?family_id="+encodeURIComponent(state.familyId));
  state.shoppingListNeedsRegeneration=false;
 }catch(e){
  if(e.status===404)state.shoppingList=null;
  else{$("shopping-list-message").textContent=e.message;state.shoppingList=null}
 }
 renderShoppingList();
}
function renderShoppingList(){
 const list=state.shoppingList;
 $("shopping-list-panel").hidden=!state.plan;
 $("shopping-item-form").hidden=!list;
 $("generate-shopping-list").textContent=list?"Regenerate from this plan":"Generate list";
 $("shopping-list-status").textContent=list
  ?"Snapshot for plan revision "+list.plan_revision+" · edits are independent of plan confirmation"
  : "Only home-prepared assignments with recipes will contribute ingredients.";
 if(list&&state.shoppingListNeedsRegeneration){
  $("shopping-list-status").textContent="The plan changed. Regenerate this list to update its ingredients.";
 }
 const visible=list?.items?.filter(item=>!item.manually_removed)||[];
 $("shopping-list-items").innerHTML=!list
  ?'<p class="hint">No list yet. Generate one to collect ingredients from this weekly plan.</p>'
  :visible.length?visible.map(item=>'<div class="shopping-item" data-shopping-item="'+item.id+'">'+
   '<input class="shopping-item-name" data-shopping-name aria-label="Item name" value="'+esc(item.name)+'">'+
   '<input type="number" min="0.000001" step="any" data-shopping-quantity aria-label="Quantity" placeholder="Unknown" value="'+(item.quantity??"")+'">'+
   '<input data-shopping-unit aria-label="Unit" placeholder="Unit" value="'+esc(item.unit??"")+'">'+
   '<label class="shopping-purchased"><input type="checkbox" data-shopping-purchased '+(item.purchased?"checked":"")+'> Purchased</label>'+
   (item.stale?'<span class="shopping-stale">Review</span>':'')+
   '<div class="shopping-item-actions"><button type="button" class="secondary" data-save-shopping>Save</button><button type="button" class="danger" data-remove-shopping>Remove</button></div>'+
   '</div>').join("")
  :'<p class="hint">No items in this shopping list.</p>';
}
async function generateShoppingList(){
 if(!state.plan)return;
 if(state.shoppingList&&!confirm("Regenerate this list from the selected plan revision? Manual additions, removals, purchases and quantity overrides will be reconciled."))return;
 const button=$("generate-shopping-list");button.disabled=true;
 try{
  state.shoppingList=await api("/meal-planner/plans/"+state.plan.id+"/shopping-list/generate?family_id="+encodeURIComponent(state.familyId),{method:"POST"});
  state.shoppingListNeedsRegeneration=false;
  $("shopping-list-message").textContent="Shopping list generated.";
  renderShoppingList();
 }catch(e){$("shopping-list-message").textContent=e.message}
 finally{button.disabled=false}
}

async function loadWeek(){
 $("week-date").value=state.weekStart;
 $("planner-grid").innerHTML='<p class="hint">Loading weekly plan…</p>';
 $("assignment-editor").hidden=true;
 try{
  state.familyId=window.emeMealsFamily.currentFamilyId();
  if(!state.familyId){
   state.plan=null;state.shoppingList=null;
   $("planner-grid").innerHTML='<p class="empty">Create or join a family to start planning meals. <a href="/console/families">Manage families</a>.</p>';
   $("shopping-list-panel").hidden=true;
   $("connection").textContent="No family selected";
   return;
  }
  const [members,recipes]=await Promise.all([api("/meal-planner/members?family_id="+encodeURIComponent(state.familyId)),api("/meal-planner/recipes?family_id="+encodeURIComponent(state.familyId))]);
  state.members=members;state.recipes=recipes;
  const dates=weekDates();
  state.menus=await api("/meal-planner/school-menus?family_id="+encodeURIComponent(state.familyId)+"&from="+dates[0]+"&to="+dates[6]);
  try{state.plan=await api("/meal-planner/plans/"+state.weekStart+"?family_id="+encodeURIComponent(state.familyId))}
  catch(e){if(e.status!==404)throw e;state.plan=await api("/meal-planner/plans/"+state.weekStart+"?family_id="+encodeURIComponent(state.familyId),{method:"POST"})}
  renderGrid();await loadShoppingList();$("connection").textContent="Connected";
 }catch(e){$("connection").textContent="Connection error";$("planner-grid").innerHTML='<p class="empty">'+esc(e.message)+'</p>'}
}

$("generate-shopping-list").addEventListener("click",generateShoppingList);
$("shopping-item-form").addEventListener("submit",async event=>{
 event.preventDefault();
 if(!state.plan||!state.shoppingList)return;
 const name=$("shopping-item-name").value.trim();
 const raw=$("shopping-item-quantity").value.trim();
 const quantity=raw===""?null:Number(raw);
 if(raw!==""&&(!Number.isFinite(quantity)||quantity<=0)){$("shopping-list-message").textContent="Enter a positive quantity or leave it blank.";return}
 const button=event.submitter;button.disabled=true;
 try{
  state.shoppingList=await api("/meal-planner/plans/"+state.plan.id+"/shopping-list/items?family_id="+encodeURIComponent(state.familyId),{method:"POST",body:JSON.stringify({name,quantity,unit:$("shopping-item-unit").value.trim()||null})});
  $("shopping-item-name").value="";$("shopping-item-quantity").value="";$("shopping-item-unit").value="";
  $("shopping-list-message").textContent="Item added.";renderShoppingList();
 }catch(e){$("shopping-list-message").textContent=e.message}
 finally{button.disabled=false}
});
$("shopping-list-items").addEventListener("click",async event=>{
 const row=event.target.closest("[data-shopping-item]");if(!row||!state.plan)return;
 const itemId=Number(row.dataset.shoppingItem);
 if(event.target.closest("[data-remove-shopping]")){
  try{state.shoppingList=await api("/meal-planner/plans/"+state.plan.id+"/shopping-list/items/"+itemId+"?family_id="+encodeURIComponent(state.familyId),{method:"DELETE"});$("shopping-list-message").textContent="Item removed.";renderShoppingList()}catch(e){$("shopping-list-message").textContent=e.message}
  return;
 }
 if(event.target.closest("[data-save-shopping]")){
  const name=row.querySelector("[data-shopping-name]").value.trim();
  const raw=row.querySelector("[data-shopping-quantity]").value.trim();
  const quantity=raw===""?null:Number(raw);
  if(!name){$("shopping-list-message").textContent="Item name cannot be empty.";return}
  if(raw!==""&&(!Number.isFinite(quantity)||quantity<=0)){$("shopping-list-message").textContent="Enter a positive quantity or leave it blank.";return}
  try{
   state.shoppingList=await api("/meal-planner/plans/"+state.plan.id+"/shopping-list/items/"+itemId+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({name,quantity,unit:row.querySelector("[data-shopping-unit]").value.trim()||null,purchased:row.querySelector("[data-shopping-purchased]").checked})});
   $("shopping-list-message").textContent="Item updated.";renderShoppingList();
  }catch(e){$("shopping-list-message").textContent=e.message}
 }
});

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
$("confirm-plan").addEventListener("click",async()=>{if(!confirm("Confirm this week's plan? Confirmed plans are immutable; later changes require a new revision."))return;try{state.plan=await api("/meal-planner/plans/"+state.weekStart+"/confirm?family_id="+encodeURIComponent(state.familyId),{method:"POST"});renderGrid()}catch(e){alert(e.message)}});
$("revise-plan").addEventListener("click",async()=>{try{state.plan=await api("/meal-planner/plans/"+state.weekStart+"/revise?family_id="+encodeURIComponent(state.familyId),{method:"POST"});renderGrid();await loadShoppingList()}catch(e){alert(e.message)}});
$("previous-week").addEventListener("click",()=>{state.weekStart=localDate(addDays(parseDate(state.weekStart),-7));loadWeek()});
$("next-week").addEventListener("click",()=>{state.weekStart=localDate(addDays(parseDate(state.weekStart),7));loadWeek()});
$("week-date").addEventListener("change",()=>{if($("week-date").value){state.weekStart=localDate(mondayOf(parseDate($("week-date").value)));loadWeek()}});
window.addEventListener("eme:family-changed",()=>{state.plan=null;state.shoppingList=null;loadWeek()});
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();state.weekStart=localDate(mondayOf(new Date()));await loadWeek()}catch(e){$("connection").textContent="Error loading planner: "+e.message;$("planner-grid").innerHTML='<p class="empty">'+esc(e.message)+'</p>'}})();
