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
 }).join(""):'<p class="usage-empty">No agents match this search.</p>';
 $("agents-status").textContent=agents.length+" of "+state.agents.length+" configured agents";
}
async function load(){
 $("agents-status").textContent="Loading configured agents…";
 $("agents-status").className="usage-status";
 try{
  const response=await fetch("/agents",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.agents=await response.json();
  render();
 }catch(error){
  $("agents-status").textContent="Could not load agents: "+error.message;
  $("agents-status").className="usage-status error";
  $("agents-list").innerHTML="";
 }
}
$("agents-search").addEventListener("input",render);
$("agents-refresh").addEventListener("click",load);
load();
