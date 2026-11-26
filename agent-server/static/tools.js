const $=id=>document.getElementById(id);
const state={tools:[]};
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
function render(){
 const query=$("tools-search").value.trim().toLocaleLowerCase();
 const tools=state.tools.filter(tool=>[tool.name,tool.description].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 $("tools-list").innerHTML=tools.length?tools.map(tool=>{
  const schema=JSON.stringify(tool.parameters??{},null,2);
  return '<article class="panel tool-card"><div class="tool-card-heading"><div><h3>'+escapeHtml(tool.name)+'</h3><p>'+escapeHtml(tool.description||"No description provided.")+'</p></div><span class="tool-card-tag">Registered</span></div><details><summary>Input schema</summary><pre>'+escapeHtml(schema)+'</pre></details></article>';
 }).join(""):'<div class="usage-empty">'+(query?'No tools match this filter. <button type="button" class="secondary" data-clear-search>Clear filter</button>':'No tools are registered.')+'</div>';
 $("tools-status").textContent=tools.length+" of "+state.tools.length+" registered tools";
}
async function load(){
 const button=$("tools-refresh");
 if(button){button.disabled=true;button.textContent="Refreshing…";}
 $("tools-status").textContent="Loading registered tools…";
 $("tools-list").innerHTML='<p class="usage-empty" role="status">Loading tools…</p>';
 try{
  const response=await fetch("/tools",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.tools=await response.json();
  render();
 }catch(error){
  $("tools-status").textContent="Could not load tools.";
  $("tools-status").className="usage-status error";
  $("tools-list").innerHTML='<div class="usage-empty" role="alert">Could not load tools: '+escapeHtml(error.message)+'<p><button type="button" class="secondary" data-retry>Try again</button></p></div>';
 }finally{if(button){button.disabled=false;button.textContent="Refresh"}}
}
$("tools-search").addEventListener("input",render);
$("tools-refresh")?.addEventListener("click",load);
$("tools-list").addEventListener("click",event=>{
 if(event.target.closest("[data-clear-search]")){$("tools-search").value="";render()}
 if(event.target.closest("[data-retry]"))load();
});
load();