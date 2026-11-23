const $=id=>document.getElementById(id);
const state={providers:[]};
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const formatCapability=value=>String(value??"").replace(/_/g," ").replace(/\b\w/g,c=>c.toUpperCase());
function render(){
 const query=$("providers-search").value.trim().toLocaleLowerCase();
 const providers=state.providers.map(provider=>{
  const models=(provider.models||[]).filter(model=>[provider.name,provider.client,model.name,...(model.capabilities||[])].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
  return {...provider,models};
 }).filter(provider=>!query||provider.name.toLocaleLowerCase().includes(query)||provider.client.toLocaleLowerCase().includes(query)||provider.models.length);
 $("providers-list").innerHTML=providers.length?providers.map(provider=>{
  const models=provider.models.map(model=>{
   const capabilities=(model.capabilities||[]).map(capability=>'<span class="tool-card-tag">'+escapeHtml(formatCapability(capability))+'</span>').join("");
   return '<div class="provider-model"><div class="provider-model-heading"><strong>'+escapeHtml(model.name)+'</strong><span class="tool-card-tag">'+(model.capabilities||[]).length+' capabilities</span></div><div class="agent-tags">'+(capabilities||'<span class="usage-empty">No capabilities declared</span>')+'</div></div>';
  }).join("");
  return '<article class="panel tool-card"><div class="tool-card-heading"><div><h3>'+escapeHtml(provider.name)+'</h3><p>Client adapter: '+escapeHtml(provider.client)+'</p></div><span class="tool-card-tag">'+(provider.models||[]).length+' models</span></div><div class="provider-models">'+models+'</div></article>';
 }).join(""):'<p class="usage-empty">No providers or models match this search.</p>';
 const count=state.providers.reduce((total,provider)=>total+(provider.models||[]).length,0);
 $("providers-status").textContent=providers.length+" of "+state.providers.length+" providers · "+count+" configured models";
}
async function load(){
 $("providers-status").textContent="Loading configured providers…";
 $("providers-status").className="usage-status";
 try{
  const response=await fetch("/providers",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.providers=await response.json();
  render();
 }catch(error){
  $("providers-status").textContent="Could not load providers: "+error.message;
  $("providers-status").className="usage-status error";
  $("providers-list").innerHTML="";
 }
}
$("providers-search").addEventListener("input",render);
$("providers-refresh").addEventListener("click",load);
load();
