const $=id=>document.getElementById(id);
const state={familyId:"",groups:[],loading:false};
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const raw=await response.text();let value=null;try{value=raw?JSON.parse(raw):null}catch{value=raw}if(!response.ok)throw new Error(typeof value==="string"&&value?value:response.statusText);return value}
function esc(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function parseDate(value){const [y,m,d]=value.split("-").map(Number);return new Date(y,m-1,d,12)}
function dateString(date){return [date.getFullYear(),String(date.getMonth()+1).padStart(2,"0"),String(date.getDate()).padStart(2,"0")].join("-")}
function monday(date){const result=new Date(date);result.setDate(result.getDate()-(result.getDay()+6)%7);return result}
function addDays(date,days){const result=new Date(date);result.setDate(result.getDate()+days);return result}
function formatDate(date){return date.toLocaleDateString(undefined,{day:"numeric",month:"short",year:"numeric"})}
function keyFor(item){return [item.name.trim().toLocaleLowerCase(),(item.unit||"").trim().toLocaleLowerCase()].join("|")}
function displayQuantity(item){if(item.quantity===null||item.quantity===undefined)return "";const value=Number.isInteger(item.quantity)?item.quantity:Number(item.quantity.toFixed(2));return value+" "+(item.unit||"")}
async function loadList(){
 if(state.loading)return;state.loading=true;$("shopping-refresh").disabled=true;$("shopping-content").innerHTML='<p class="hint">Collecting ingredients from planned meals…</p>';
 try{
  state.familyId=window.emeMealsFamily.currentFamilyId();
  if(!state.familyId){$("shopping-content").innerHTML='<p class="empty">Create or join a family to build a shopping list. <a href="/console/families">Manage families</a>.</p>';return}
  const start=monday(parseDate($("shopping-start").value));const weeks=Number($("shopping-weeks").value);const end=addDays(start,weeks*7-1);
  $("shopping-range").textContent=formatDate(start)+" – "+formatDate(end);
  const grouped=new Map();let plannedWeeks=0;
  for(let offset=0;offset<weeks;offset++){
   const week=dateString(addDays(start,offset*7));
   const plan=await api("/meal-planner/plans/"+week+"?family_id="+encodeURIComponent(state.familyId));
   if(!plan)continue;
   plannedWeeks++;
   const list=await api("/meal-planner/plans/"+plan.id+"/shopping-list/generate?family_id="+encodeURIComponent(state.familyId),{method:"POST"});
   for(const item of list.items||[]){
    const key=keyFor(item);let group=grouped.get(key);
    if(!group){group={name:item.name,unit:item.unit,quantity:0,originalQuantity:0,purchased:item.purchased,entries:[]};grouped.set(key,group)}
    const original=Number(item.quantity);
    if(Number.isFinite(original)){group.originalQuantity+=original;if(!item.manually_removed)group.quantity+=original}else{group.quantity=null;group.originalQuantity=null}
    group.purchased=group.purchased&&item.purchased;
    group.entries.push({week,planId:plan.id,item,originalQuantity:item.quantity});
   }
  }
  state.groups=[...grouped.values()].map(group=>({...group,disabled:group.entries.every(entry=>entry.item.manually_removed),quantity:group.quantity===null?null:group.quantity})).sort((a,b)=>a.name.localeCompare(b.name));
  renderList(plannedWeeks,weeks);
 }catch(error){$("shopping-content").innerHTML='<p class="empty">'+esc(error.message)+'</p>'}
 finally{state.loading=false;$("shopping-refresh").disabled=false}
}
function renderList(plannedWeeks,weeks){
 const items=state.groups;const needed=items.filter(item=>!item.purchased&&!item.disabled).length;
 if(!items.length){$("shopping-content").innerHTML='<p class="empty">'+(plannedWeeks?'No ingredients found in the planned meals.':'No meal plans found for this date range.')+'</p>';return}
 $("shopping-content").innerHTML='<div class="shopping-list-summary"><strong>'+needed+' items to buy</strong><span class="hint">'+items.length+' ingredients across '+plannedWeeks+' planned '+(plannedWeeks===1?'week':'weeks')+'</span></div><ul class="shopping-list-items">'+items.map((item,index)=>'<li class="'+(item.purchased?'is-available ':'')+(item.disabled?'is-disabled':'')+'"><div class="shopping-list-item"><input type="checkbox" data-index="'+index+'" aria-label="Mark '+esc(item.name)+' as available" '+(item.purchased?'checked':'')+(item.disabled?' disabled':'')+'><span class="shopping-list-item-name">'+esc(item.name)+'</span><span class="shopping-list-quantity" data-quantity="'+index+'">'+(item.disabled?'Not needed':esc(displayQuantity(item)))+'</span><div class="shopping-list-item-actions"><button type="button" class="shopping-icon-button" data-action="edit" data-index="'+index+'" title="Change quantity" aria-label="Change quantity">✎</button><button type="button" class="shopping-icon-button" data-action="disable" data-index="'+index+'" title="Disable item" aria-label="Disable item">⊘</button><button type="button" class="shopping-icon-button" data-action="restore" data-index="'+index+'" title="Restore original quantity" aria-label="Restore original quantity">↶</button></div></div></li>').join("")+'</ul>';
 $("shopping-content").querySelectorAll('input[type="checkbox"][data-index]').forEach(input=>input.addEventListener("change",()=>toggleItem(Number(input.dataset.index),input.checked)));
 $("shopping-content").querySelectorAll(".shopping-icon-button").forEach(button=>button.addEventListener("click",()=>handleItemAction(Number(button.dataset.index),button.dataset.action)));
}
async function updateEntries(group, transform){
 const updates=group.entries.map(({planId,item})=>{const fields=transform(item);return api("/meal-planner/plans/"+planId+"/shopping-list/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({...item,...fields})})});
 await Promise.all(updates);
}
async function handleItemAction(index,action){
 const group=state.groups[index];
 if(action==="edit"){
  const quantityNode=$("shopping-content").querySelector('[data-quantity="'+index+'"]');
  const current=group.disabled?group.originalQuantity:group.quantity;
  const input=document.createElement("input");input.type="number";input.min="0";input.step="any";input.value=current??"";input.className="shopping-quantity-editor";input.setAttribute("aria-label","New quantity for "+group.name);
  quantityNode.replaceChildren(input);input.focus();input.select();
  const commit=async save=>{if(!input.isConnected)return;if(!save){renderList(new Set(state.groups.flatMap(g=>g.entries.map(e=>e.week))).size,Number($("shopping-weeks").value));return}const next=Number(input.value);if(!input.value.trim()||!Number.isFinite(next)||next<0){input.focus();return}
   const originalTotal=group.entries.reduce((sum,e)=>sum+(Number(e.originalQuantity)||0),0);
   const active=group.entries.filter(e=>!e.item.manually_removed);
   try{await Promise.all(group.entries.map(({planId,item,originalQuantity})=>{const base=Number(originalQuantity)||0;const scaled=originalTotal>0?next*base/originalTotal:next/group.entries.length;return api("/meal-planner/plans/"+planId+"/shopping-list/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({...item,quantity:scaled,manually_removed:false})})}));await loadList()}catch(error){alert("Could not change quantity: "+error.message);await loadList()}
  };
  input.addEventListener("keydown",event=>{if(event.key==="Enter")commit(true);if(event.key==="Escape")commit(false)});
  input.addEventListener("blur",()=>commit(true));return;
 }
 try{
  if(action==="disable")await updateEntries(group,()=>({manually_removed:true}));
  else if(action==="restore")await Promise.all(group.entries.map(({planId,item,originalQuantity})=>api("/meal-planner/plans/"+planId+"/shopping-list/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({...item,quantity:originalQuantity,manually_removed:false})})));
  await loadList();
 }catch(error){alert("Could not update ingredient: "+error.message);await loadList()}
}
async function toggleItem(index,available){
 const group=state.groups[index];$("shopping-content").querySelectorAll("button,input").forEach(input=>input.disabled=true);
 try{await updateEntries(group,()=>({purchased:available}));group.purchased=available;renderList(new Set(state.groups.flatMap(g=>g.entries.map(e=>e.week))).size,Number($("shopping-weeks").value))}
 catch(error){alert("Could not update ingredient: "+error.message);await loadList()}
}
$("shopping-refresh").addEventListener("click",loadList);
$("shopping-start").addEventListener("change",loadList);
$("shopping-weeks").addEventListener("change",loadList);
window.addEventListener("eme:family-changed",loadList);
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();const today=new Date();$("shopping-start").value=dateString(monday(today));await loadList()}catch(error){$("connection").textContent="Error loading shopping list: "+error.message;$("shopping-content").textContent=error.message}})();