const $=id=>document.getElementById(id);
const state={agents:[]};
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
function render(){
 const query=$("agents-search").value.trim().toLocaleLowerCase();
 const agents=state.agents.filter(agent=>[agent.name,agent.description,...(agent.tools||[]),...(agent.capabilities||[])].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 $("agents-list").innerHTML=agents.length?agents.map(agent=>{
  const capabilities=(agent.capabilities||[]).map(value=>'<span class="tool-card-tag">'+escapeHtml(value)+'</span>').join("");
  const assignedTools=(agent.tools||[]).map(value=>'<span class="tool-card-tag">'+escapeHtml(value)+'</span>').join("");
  return '<article class="panel tool-card"><div class="tool-card-heading"><div><h3>'+escapeHtml(agent.name)+'</h3><p>'+escapeHtml(agent.description||"No description provided.")+'</p></div><span class="tool-card-tag">Configured</span></div><div class="agent-metadata"><section><h4>Capabilities</h4><div class="agent-tags">'+(capabilities||'<span class="usage-empty">None declared</span>')+'</div></section><section><h4>Assigned tools</h4><div class="agent-tags">'+(assignedTools||'<span class="usage-empty">No tools assigned</span>')+'</div></section></div></article>';
 }).join(""):'<div class="usage-empty">'+(query?'No agents match this filter. <button type="button" class="secondary" data-clear-search>Clear filter</button>':'No agents are configured yet.')+'</div>';
 $("agents-status").textContent=agents.length+" of "+state.agents.length+" configured agents";
}
async function load(){
 const button=$("agents-refresh");
 button.disabled=true;button.textContent="Refreshing…";
 $("agents-status").textContent="Loading configured agents…";
 $("agents-list").innerHTML='<p class="usage-empty" role="status">Loading agents…</p>';
 try{
  const response=await fetch("/agents",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.agents=await response.json();
  render();
 }catch(error){
  $("agents-status").textContent="Could not load agents.";
  $("agents-status").className="usage-status error";
  $("agents-list").innerHTML='<div class="usage-empty" role="alert">Could not load agents: '+escapeHtml(error.message)+'<p><button type="button" class="secondary" data-retry>Try again</button></p></div>';
 }finally{button.disabled=false;button.textContent="Refresh"}
}
$("agents-search").addEventListener("input",render);
$("agents-refresh").addEventListener("click",load);
$("agents-list").addEventListener("click",event=>{
 if(event.target.closest("[data-clear-search]")){$("agents-search").value="";render()}
 if(event.target.closest("[data-retry]"))load();
});
load();