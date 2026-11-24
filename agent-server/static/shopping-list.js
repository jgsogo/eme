const $=id=>document.getElementById(id);
const state={familyId:"",list:null,loading:false};
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const raw=await response.text();let value=null;try{value=raw?JSON.parse(raw):null}catch{value=raw}if(!response.ok)throw new Error(typeof value==="string"&&value?value:response.statusText);return value}
function esc(v){return String(v??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function parseDate(v){const[y,m,d]=v.split("-").map(Number);return new Date(y,m-1,d,12)}
function dateString(d){return[d.getFullYear(),String(d.getMonth()+1).padStart(2,"0"),String(d.getDate()).padStart(2,"0")].join("-")}
function monday(d){const x=new Date(d);x.setDate(x.getDate()-(x.getDay()+6)%7);return x}
function addDays(d,n){const x=new Date(d);x.setDate(x.getDate()+n);return x}
function formatDate(d){return d.toLocaleDateString(undefined,{day:"numeric",month:"short",year:"numeric"})}
function quantity(item){if(item.quantity===null||item.quantity===undefined)return"";const n=Number.isInteger(item.quantity)?item.quantity:Number(item.quantity.toFixed(2));return n+(item.unit?" "+item.unit:"")}
function range(){const start=monday(parseDate($("shopping-start").value));const end=addDays(start,Number($("shopping-weeks").value)*7-1);return{start:dateString(start),end:dateString(end),startDate:start,endDate:end}}
async function loadList(){
 if(state.loading)return;state.loading=true;$("shopping-refresh").disabled=true;$("shopping-content").innerHTML='<p class="hint">Collecting ingredients from planned meals…</p>';
 try{
  state.familyId=window.emeMealsFamily.currentFamilyId();
  if(!state.familyId){state.list=null;$("shopping-content").innerHTML='<p class="empty">Create or join a family to build a shopping list. <a href="/console/families">Manage families</a>.</p>';return}
  const r=range();$("shopping-range").textContent=formatDate(r.startDate)+" – "+formatDate(r.endDate);
  state.list=await api("/meal-planner/shopping-list/generate?family_id="+encodeURIComponent(state.familyId)+"&start_date="+r.start+"&end_date="+r.end,{method:"POST",body:"{}"});
  renderList();
 }catch(e){$("shopping-content").innerHTML='<p class="empty" role="alert">'+esc(e.message)+'</p>'}
 finally{state.loading=false;$("shopping-refresh").disabled=false}
}
function renderList(){
 const list=state.list,items=list?.items||[],active=items.filter(i=>!i.manually_removed),needed=active.filter(i=>!i.purchased).length;
 const query=$("shopping-search")?.value.trim().toLocaleLowerCase()||"";
 const hidePurchased=$("shopping-hide-purchased")?.checked||false;
 const visibleItems=items.map((item,index)=>({item,index})).filter(({item})=>(!hidePurchased||!item.purchased)&&item.name.toLocaleLowerCase().includes(query));
 $("shopping-restore-all").disabled=!items.some(i=>!i.manually_added&&(i.manually_removed||i.stale||i.quantity_overridden));
 if(!items.length){$("shopping-content").innerHTML='<p class="empty">No ingredients found in the planned meals for this date range.</p>';return}
 $("shopping-content").innerHTML='<div class="shopping-list-summary"><strong>'+needed+' items to buy</strong><span class="hint">'+active.length+' ingredients in this date range</span></div><ul class="shopping-list-items">'+visibleItems.map(({item:i,index:n})=>'<li class="'+(i.purchased?'is-available ':'')+(i.manually_removed?'is-disabled':'')+'"><div class="shopping-list-item"><input type="checkbox" data-index="'+n+'" aria-label="Mark '+esc(i.name)+' as available" '+(i.purchased?'checked':'')+(i.manually_removed?' disabled':'')+'><span class="shopping-list-item-name">'+esc(i.name)+'</span><span class="shopping-list-quantity" data-quantity="'+n+'">'+(i.manually_removed?'Not needed':esc(quantity(i)))+'</span><div class="shopping-list-item-actions"><button type="button" class="shopping-icon-button" data-action="edit" data-index="'+n+'" title="Change quantity" aria-label="Change quantity">✎</button><button type="button" class="shopping-icon-button" data-action="'+(i.manually_removed?'restore':'disable')+'" data-index="'+n+'" title="'+(i.manually_removed?'Restore item':'Disable item')+'" aria-label="'+(i.manually_removed?'Restore item':'Disable item')+'">'+(i.manually_removed?'↶':'⊘')+'</button></div></div></li>').join("")+'</ul><div class="shopping-list-footer"><button id="shopping-copy" type="button">Copy shopping list</button></div>';
 $("shopping-content").querySelectorAll('input[type="checkbox"][data-index]').forEach(el=>el.addEventListener("change",()=>toggleItem(Number(el.dataset.index),el.checked)));
 $("shopping-content").querySelectorAll(".shopping-icon-button").forEach(el=>el.addEventListener("click",()=>handleAction(Number(el.dataset.index),el.dataset.action)));
 $("shopping-copy").addEventListener("click",copyList);
}
async function update(item,fields){return api("/meal-planner/shopping-list/"+state.list.id+"/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({...item,...fields})})}
async function toggleItem(index,purchased){try{const item=state.list.items[index];state.list=await update(item,{purchased});renderList()}catch(e){alert("Could not update ingredient: "+e.message)}}
async function handleAction(index,action){
 const item=state.list.items[index];
 if(action==="edit"){
  const node=$("shopping-content").querySelector('[data-quantity="'+index+'"]'),input=document.createElement("input");input.type="number";input.min="0.001";input.step="any";input.value=item.quantity??"";input.className="shopping-quantity-editor";input.setAttribute("aria-label","New quantity for "+item.name);node.replaceChildren(input);input.focus();input.select();
  let done=false;const commit=async save=>{if(done)return;done=true;if(!save){renderList();return}const value=Number(input.value);if(!input.value.trim()||!Number.isFinite(value)||value<=0){done=false;input.focus();return}try{state.list=await update(item,{quantity:value});renderList()}catch(e){alert("Could not change quantity: "+e.message);renderList()}};
  input.addEventListener("keydown",e=>{if(e.key==="Enter")commit(true);if(e.key==="Escape")commit(false)});input.addEventListener("blur",()=>commit(true));return;
 }
 try{
  if(action==="disable")state.list=await api("/meal-planner/shopping-list/"+state.list.id+"/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"DELETE"});
  else state.list=await api("/meal-planner/shopping-list/"+state.list.id+"/items/"+item.id+"/restore?family_id="+encodeURIComponent(state.familyId),{method:"POST",body:"{}"});
  renderList();
 }catch(e){alert("Could not update ingredient: "+e.message)}
}
$("shopping-search")?.addEventListener("input",renderList);$("shopping-hide-purchased")?.addEventListener("change",renderList);
$("shopping-refresh").addEventListener("click",loadList);
$("shopping-restore-all").addEventListener("click",async()=>{const items=state.list?.items||[];for(const item of items.filter(i=>!i.manually_added&&(i.manually_removed||i.stale||i.quantity_overridden))){try{state.list=await api("/meal-planner/shopping-list/"+state.list.id+"/items/"+item.id+"/restore?family_id="+encodeURIComponent(state.familyId),{method:"POST",body:"{}"})}catch{}}renderList()});
$("shopping-start").addEventListener("change",loadList);$("shopping-weeks").addEventListener("change",loadList);window.addEventListener("eme:family-changed",loadList);
async function copyList(){const lines=(state.list?.items||[]).filter(i=>!i.manually_removed).map(i=>(i.purchased?"[x] ":"[ ] ")+i.name+(quantity(i)?" — "+quantity(i):""));if(!lines.length)return;try{await navigator.clipboard.writeText(lines.join("\n"));$("shopping-copy").textContent="Copied!"}catch(e){alert("Could not copy shopping list: "+e.message)}}
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();$("shopping-start").value=dateString(monday(new Date()));await loadList()}catch(e){$("connection").textContent="Error loading shopping list: "+e.message}})();
