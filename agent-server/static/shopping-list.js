const $=id=>document.getElementById(id);
const state={familyId:"",list:null,loading:false};
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const raw=await response.text();let value=null;try{value=raw?JSON.parse(raw):null}catch{value=raw}if(!response.ok)throw new Error(typeof value==="string"&&value?value:response.statusText);return value}
function esc(v){return String(v??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function parseDate(v){if(/^\d{2}\/\d{2}\/\d{4}$/.test(v)){const[d,m,y]=v.split("/").map(Number);return new Date(y,m-1,d,12)}const[y,m,d]=v.split("-").map(Number);return new Date(y,m-1,d,12)}
function dateString(d){return[d.getFullYear(),String(d.getMonth()+1).padStart(2,"0"),String(d.getDate()).padStart(2,"0")].join("-")}
function validDateInput(v){if(desktopDatePicker){if(!/^\d{2}\/\d{2}\/\d{4}$/.test(v))return false;const d=parseDate(v);return d.getFullYear()>=1900&&formatDate(d)===v}if(!/^\d{4}-\d{2}-\d{2}$/.test(v))return false;const d=parseDate(v);return dateString(d)===v}
function monday(d){const x=new Date(d);x.setDate(x.getDate()-(x.getDay()+6)%7);return x}
function addDays(d,n){const x=new Date(d);x.setDate(x.getDate()+n);return x}
function formatDate(d){return[String(d.getDate()).padStart(2,"0"),String(d.getMonth()+1).padStart(2,"0"),d.getFullYear()].join("/")}
function fieldDate(d){return desktopDatePicker?formatDate(d):dateString(d)}
const desktopDatePicker=window.matchMedia("(min-width: 761px)").matches;
function quantity(item){if(item.quantity===null||item.quantity===undefined)return"";const n=Number.isInteger(item.quantity)?item.quantity:Number(item.quantity.toFixed(2));return n+(item.unit?" "+item.unit:"")}

function installDesktopDatePicker(){
 const field=$("shopping-start");field.type="text";field.placeholder="DD/MM/YYYY";field.inputMode="numeric";field.autocomplete="off";field.setAttribute("aria-label","Starting week (DD/MM/YYYY)");
 const wrap=document.createElement("div");wrap.className="shopping-date-picker";field.parentNode.insertBefore(wrap,field);wrap.append(field);
 const button=document.createElement("button");button.type="button";button.className="shopping-date-picker-toggle";button.textContent="▦";button.setAttribute("aria-label","Open calendar");wrap.append(button);
 const popup=document.createElement("div");popup.className="shopping-date-picker-popup";popup.hidden=true;popup.setAttribute("role","dialog");popup.setAttribute("aria-label","Choose starting week");wrap.append(popup);
 let visibleMonth=monday(new Date());visibleMonth.setDate(1);
 const monthLabel=d=>new Intl.DateTimeFormat("en-GB",{month:"long",year:"numeric"}).format(d);
 const render=()=>{
  const first=new Date(visibleMonth.getFullYear(),visibleMonth.getMonth(),1,12),offset=(first.getDay()+6)%7;
  const start=new Date(first);start.setDate(first.getDate()-offset);
  const selected=/^\\d{2}\\/\\d{2}\\/\\d{4}$/.test(field.value)?parseDate(field.value):null;
  const weekdays=["Mon","Tue","Wed","Thu","Fri","Sat","Sun"];
  let html='<div class="shopping-date-picker-head"><button type="button" data-month="-1" aria-label="Previous month">‹</button><strong>'+monthLabel(visibleMonth)+'</strong><button type="button" data-month="1" aria-label="Next month">›</button></div><div class="shopping-date-picker-grid">'+weekdays.map(x=>'<span class="weekday">'+x+'</span>').join("");
  for(let i=0;i<42;i++){const day=new Date(start);day.setDate(start.getDate()+i);const iso=dateString(day),chosen=selected&&dateString(selected)===iso;html+='<button type="button" data-date="'+iso+'" class="'+(day.getMonth()!==visibleMonth.getMonth()?'outside ':'')+(chosen?'selected':'')+'">'+day.getDate()+'</button>'}
  popup.innerHTML=html+'</div>';
  popup.querySelectorAll("[data-month]").forEach(b=>b.addEventListener("click",()=>{visibleMonth.setMonth(visibleMonth.getMonth()+Number(b.dataset.month));render()}));
  popup.querySelectorAll("[data-date]").forEach(b=>b.addEventListener("click",()=>{field.value=fieldDate(monday(parseDate(b.dataset.date)));field.dispatchEvent(new Event("change",{bubbles:true}));popup.hidden=true;button.setAttribute("aria-expanded","false")}));
 };
 const open=()=>{const current=/^\\d{2}\\/\\d{2}\\/\\d{4}$/.test(field.value)?parseDate(field.value):new Date();visibleMonth=new Date(current.getFullYear(),current.getMonth(),1,12);render();popup.hidden=false;button.setAttribute("aria-expanded","true")};
 button.setAttribute("aria-expanded","false");button.addEventListener("click",()=>popup.hidden?open():(popup.hidden=true,button.setAttribute("aria-expanded","false")));
 field.addEventListener("focus",()=>{if(popup.hidden)open()});

