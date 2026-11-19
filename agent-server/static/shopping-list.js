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
    if(item.manually_removed)continue;
    const key=keyFor(item);let group=grouped.get(key);
    if(!group){group={name:item.name,unit:item.unit,quantity:item.quantity,purchased:item.purchased,entries:[]};grouped.set(key,group)}
    else if(group.quantity!==null&&item.quantity!==null)group.quantity+=item.quantity;else group.quantity=null;
    group.purchased=group.purchased&&item.purchased;
    group.entries.push({week,planId:plan.id,item});
   }
  }
  state.groups=[...grouped.values()].sort((a,b)=>a.name.localeCompare(b.name));
  renderList(plannedWeeks,weeks);
 }catch(error){$("shopping-content").innerHTML='<p class="empty">'+esc(error.message)+'</p>'}
 finally{state.loading=false;$("shopping-refresh").disabled=false}
}
function renderList(plannedWeeks,weeks){
 const items=state.groups;const needed=items.filter(item=>!item.purchased).length;
 if(!items.length){$("shopping-content").innerHTML='<p class="empty">'+(plannedWeeks?'No ingredients found in the planned meals.':'No meal plans found for this date range.')+'</p>';return}
 $("shopping-content").innerHTML='<div class="shopping-list-summary"><strong>'+needed+' items to buy</strong><span class="hint">'+items.length+' ingredients across '+plannedWeeks+' planned '+(plannedWeeks===1?'week':'weeks')+'</span></div><ul class="shopping-list-items">'+items.map((item,index)=>'<li class="'+(item.purchased?'is-available':'')+'"><label class="shopping-list-item"><input type="checkbox" data-index="'+index+'" '+(item.purchased?'checked':'')+'><span class="shopping-list-item-name">'+esc(item.name)+'</span><span class="shopping-list-quantity">'+esc(displayQuantity(item))+'</span></label></li>').join("")+'</ul>';
 $("shopping-content").querySelectorAll('input[type="checkbox"]').forEach(input=>input.addEventListener("change",()=>toggleItem(Number(input.dataset.index),input.checked)));
}
async function toggleItem(index,available){
 const group=state.groups[index];const content=$("shopping-content");content.querySelectorAll('input[type="checkbox"]').forEach(input=>input.disabled=true);
 try{
  await Promise.all(group.entries.map(({week,planId,item})=>api("/meal-planner/plans/"+planId+"/shopping-list/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({name:item.name,quantity:item.quantity,unit:item.unit,purchased:available})})));
  group.purchased=available;renderList(new Set(state.groups.flatMap(g=>g.entries.map(e=>e.week))).size,Number($("shopping-weeks").value));
 }catch(error){alert("Could not update ingredient: "+error.message);await loadList()}
}
$("shopping-refresh").addEventListener("click",loadList);
$("shopping-start").addEventListener("change",loadList);
$("shopping-weeks").addEventListener("change",loadList);
window.addEventListener("eme:family-changed",loadList);
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();const today=new Date();$("shopping-start").value=dateString(monday(today));await loadList()}catch(error){$("connection").textContent="Error loading shopping list: "+error.message;$("shopping-content").textContent=error.message}})();