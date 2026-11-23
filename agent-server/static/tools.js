const $=id=>document.getElementById(id);
const state={tools:[]};
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
function render(){
 const query=$("tools-search").value.trim().toLocaleLowerCase();
 const tools=state.tools.filter(tool=>[tool.name,tool.description].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 $("tools-list").innerHTML=tools.length?tools.map(tool=>{
  const schema=JSON.stringify(tool.parameters??{},null,2);
  return '<article class="panel tool-card"><div class="tool-card-heading"><div><h3>'+escapeHtml(tool.name)+'</h3><p>'+escapeHtml(tool.description||"No description provided.")+'</p></div><span class="tool-card-tag">Registered</span></div><details><summary>Input schema</summary><pre>'+escapeHtml(schema)+'</pre></details></article>';
 }).join(""):'<p class="usage-empty">No tools match this search.</p>';
 $("tools-status").textContent=tools.length+" of "+state.tools.length+" registered tools";
}
async function load(){
 $("tools-status").textContent="Loading registered tools…";
 $("tools-status").className="usage-status";
 try{
  const response=await fetch("/tools",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.tools=await response.json();
  render();
 }catch(error){
  $("tools-status").textContent="Could not load tools: "+error.message;
  $("tools-status").className="usage-status error";
  $("tools-list").innerHTML="";
 }
}
$("tools-search").addEventListener("input",render);
$("tools-refresh").addEventListener("click",load);
load();
