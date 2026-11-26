const $=id=>document.getElementById(id);
const state={familyId:"",list:null,loading:false};
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const raw=await response.text();let value=null;try{value=raw?JSON.parse(raw):null}catch{value=raw}if(!response.ok)throw new Error(typeof value==="string"&&value?value:response.statusText);return value}
function esc(v){return String(v??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
const desktopDatePicker=window.matchMedia("(min-width: 761px)").matches;
function parseDate(v){if(desktopDatePicker){const[d,m,y]=v.split("/").map(Number);return new Date(y,m-1,d,12)}const[y,m,d]=v.split("-").map(Number);return new Date(y,m-1,d,12)}
function fieldDate(d){return desktopDatePicker?formatDate(d):dateString(d)}
function dateString(d){return[d.getFullYear(),String(d.getMonth()+1).padStart(2,"0"),String(d.getDate()).padStart(2,"0")].join("-")}
function monday(d){const x=new Date(d);x.setDate(x.getDate()-(x.getDay()+6)%7);return x}
function addDays(d,n){const x=new Date(d);x.setDate(x.getDate()+n);return x}
function formatDate(d){return[String(d.getDate()).padStart(2,"0"),String(d.getMonth()+1).padStart(2,"0"),d.getFullYear()].join("/")}
function quantity(item){if(item.quantity===null||item.quantity===undefined)return"";const n=Number.isInteger(item.quantity)?item.quantity:Number(item.quantity.toFixed(2));return n+(item.unit?" "+item.unit:"")}

function installDesktopDatePicker(){
 const field=$("shopping-start");
 field.type="text";field.placeholder="DD/MM/YYYY";field.inputMode="numeric";field.autocomplete="off";
 field.setAttribute("aria-label","Starting week (DD/MM/YYYY)");
 const wrap=document.createElement("div");wrap.className="shopping-date-picker";
 field.parentNode.insertBefore(wrap,field);wrap.append(field);
 const toggle=document.createElement("button");toggle.type="button";toggle.className="shopping-date-picker-toggle";
 toggle.textContent="▦";toggle.setAttribute("aria-label","Open calendar");toggle.setAttribute("aria-expanded","false");wrap.append(toggle);
 const popup=document.createElement("div");popup.className="shopping-date-picker-popup";popup.hidden=true;
 popup.setAttribute("role","dialog");popup.setAttribute("aria-label","Choose starting week");wrap.append(popup);
 let month=new Date();month.setDate(1);month.setHours(12,0,0,0);
 const render=()=>{
  const first=new Date(month.getFullYear(),month.getMonth(),1,12),offset=(first.getDay()+6)%7;
  const start=new Date(first);start.setDate(1-offset);
  const headings=["Mon","Tue","Wed","Thu","Fri","Sat","Sun"];
  let html='<div class="shopping-date-picker-head"><button type="button" data-month="-1" aria-label="Previous month">‹</button><strong>'+new Intl.DateTimeFormat("en-GB",{month:"long",year:"numeric"}).format(month)+'</strong><button type="button" data-month="1" aria-label="Next month">›</button></div><div class="shopping-date-picker-grid">'+headings.map(day=>'<span class="weekday">'+day+'</span>').join("");
  for(let i=0;i<42;i++){const day=new Date(start);day.setDate(start.getDate()+i);const chosen=field.value===formatDate(day);html+='<button type="button" data-date="'+formatDate(day)+'" class="'+(day.getMonth()!==month.getMonth()?'outside ':'')+(chosen?'selected':'')+'">'+day.getDate()+'</button>'}
  popup.innerHTML=html+'</div>';
  popup.querySelectorAll("[data-month]").forEach(button=>button.addEventListener("click",()=>{month.setMonth(month.getMonth()+Number(button.dataset.month));render()}));
  popup.querySelectorAll("[data-date]").forEach(button=>button.addEventListener("click",()=>{field.value=button.dataset.date;field.dispatchEvent(new Event("change",{bubbles:true}));popup.hidden=true;toggle.setAttribute("aria-expanded","false")}));
 };
 const open=()=>{const date=/^\d{2}\/\d{2}\/\d{4}$/.test(field.value)?parseDate(field.value):new Date();month=new Date(date.getFullYear(),date.getMonth(),1,12);render();popup.hidden=false;toggle.setAttribute("aria-expanded","true")};
 toggle.addEventListener("click",()=>{if(popup.hidden)open();else{popup.hidden=true;toggle.setAttribute("aria-expanded","false")}});
 field.addEventListener("focus",()=>{if(popup.hidden)open()});
 field.addEventListener("keydown",event=>{if(event.key==="Escape"){popup.hidden=true;toggle.setAttribute("aria-expanded","false")}});
 document.addEventListener("pointerdown",event=>{if(!wrap.contains(event.target)){popup.hidden=true;toggle.setAttribute("aria-expanded","false")}});
}

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
 $("shopping-content").innerHTML='<div class="shopping-list-summary"><strong>'+needed+' items to buy</strong><span class="hint">'+active.length+' ingredients in this date range</span></div><ul class="shopping-list-items">'+(visibleItems.length?visibleItems.map(({item:i,index:n})=>'<li class="'+(i.purchased?'is-available ':'')+(i.manually_removed?'is-disabled':'')+'"><div class="shopping-list-item"><input type="checkbox" data-index="'+n+'" aria-label="Mark '+esc(i.name)+' as available" '+(i.purchased?'checked':'')+(i.manually_removed?' disabled':'')+'><span class="shopping-list-item-name">'+esc(i.name)+'</span><span class="shopping-list-quantity" data-quantity="'+n+'">'+(i.manually_removed?'Not needed':esc(quantity(i)))+'</span><div class="shopping-list-item-actions"><button type="button" class="shopping-icon-button" data-action="edit" data-index="'+n+'" title="Change quantity" aria-label="Change quantity">✎</button><button type="button" class="shopping-icon-button" data-action="'+(i.manually_removed?'restore':'disable')+'" data-index="'+n+'" title="'+(i.manually_removed?'Restore item':'Disable item')+'" aria-label="'+(i.manually_removed?'Restore item':'Disable item')+'">'+(i.manually_removed?'↶':'⊘')+'</button></div></div></li>').join(""):'<li class="empty">No ingredients match these filters.</li>')+'</ul><div class="shopping-list-footer"><button id="shopping-copy" type="button">Copy shopping list</button></div>';
 $("shopping-content").querySelectorAll('input[type="checkbox"][data-index]').forEach(el=>el.addEventListener("change",()=>toggleItem(Number(el.dataset.index),el.checked)));
 $("shopping-content").querySelectorAll(".shopping-icon-button").forEach(el=>el.addEventListener("click",()=>handleAction(Number(el.dataset.index),el.dataset.action)));
 $("shopping-copy").addEventListener("click",copyList);
}
async function update(item,fields){return api("/meal-planner/shopping-list/"+state.list.id+"/items/"+item.id+"?family_id="+encodeURIComponent(state.familyId),{method:"PUT",body:JSON.stringify({...item,...fields})})}
async function toggleItem(index,purchased){try{const item=state.list.items[index];state.list=await update(item,{purchased});renderList()}catch(e){alert("Could not update ingredient: "+e.message)}}
async function handleAction(index,action){
 const item=state.list.items[index];
 if(action==="edit"){
  const node=$("shopping-content").querySelector('[data-quantity="'+index+'"]'),wrap=document.createElement("span"),input=document.createElement("input");wrap.className="shopping-quantity-editor-wrap";input.type="number";input.min="0.001";input.step="any";input.value=item.quantity??"";input.className="shopping-quantity-editor";input.setAttribute("aria-label","New quantity for "+item.name);wrap.append(input);if(item.unit){const unit=document.createElement("span");unit.className="shopping-quantity-unit";unit.textContent=item.unit;wrap.append(unit)}node.replaceChildren(wrap);input.focus();input.select();
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
$("shopping-start").addEventListener("change",()=>{const field=$("shopping-start");if(!field.value)return;const date=parseDate(field.value);const invalid=Number.isNaN(date.getTime())||date.getFullYear()<1900||(desktopDatePicker?(date.getMonth()!==Number(field.value.slice(3,5))-1||date.getDate()!==Number(field.value.slice(0,2))):dateString(date)!==field.value);if(invalid){field.setAttribute("aria-invalid","true");return}field.value=fieldDate(monday(date));field.removeAttribute("aria-invalid");loadList()});$("shopping-weeks").addEventListener("change",loadList);window.addEventListener("eme:family-changed",loadList);
async function copyList(){const lines=(state.list?.items||[]).filter(i=>!i.manually_removed).map(i=>(i.purchased?"[x] ":"[ ] ")+i.name+(quantity(i)?" — "+quantity(i):""));if(!lines.length)return;try{await navigator.clipboard.writeText(lines.join("\n"));$("shopping-copy").textContent="Copied!"}catch(e){alert("Could not copy shopping list: "+e.message)}}
(async()=>{try{await window.emeConsoleReady;await window.emeMealsFamily.init();if(desktopDatePicker)installDesktopDatePicker();$("shopping-start").value=fieldDate(monday(new Date()));await loadList()}catch(e){$("connection").textContent="Error loading shopping list: "+e.message;const content=$("shopping-content");if(content)content.innerHTML='<p class="empty" role="alert">'+esc(e.message)+'</p>'}finally{$("shopping-content")?.closest("main")?.removeAttribute("hidden")}})();
