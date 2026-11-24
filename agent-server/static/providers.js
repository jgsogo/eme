const $=id=>document.getElementById(id);
const state={providers:[]};
const testIcon='<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 3h6M10 3v6l-5 10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2L14 9V3M8 15h8"/></svg>';
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const formatCapability=value=>String(value??"").replace(/_/g," ").replace(/\b\w/g,c=>c.toUpperCase());
function renderResult(container,result){
 const success=Boolean(result.success);
 const latency=Number.isFinite(Number(result.latency_ms))?" · "+Number(result.latency_ms)+" ms":"";
 const detail=success?result.response:result.error;
 const diagnostics=result.diagnostics?'<details class="provider-test-diagnostics"><summary>Response diagnostics</summary><pre>'+escapeHtml(JSON.stringify(result.diagnostics,null,2))+'</pre></details>':"";
 container.className="provider-model-result "+(success?"success":"error");
 container.innerHTML='<strong>'+(success?"Connection successful":"Test failed")+'</strong><span>'+escapeHtml(latency)+'</span>'+(detail?'<p>'+escapeHtml(detail)+'</p>':"")+diagnostics;
}
async function submitTest(provider,model){
 const response=await fetch("/providers",{method:"POST",headers:{"Accept":"application/json","Content-Type":"application/json"},body:JSON.stringify({provider,...(model?{model}:{})})});
 const payload=await response.json().catch(()=>({error:response.statusText}));
 if(!response.ok)throw new Error(payload.error||response.statusText);
 return payload.results||[];
}
function render(){
 const query=$("providers-search").value.trim().toLocaleLowerCase();
 const providers=state.providers.map(provider=>{
  const models=(provider.models||[]).filter(model=>[provider.name,provider.client,model.name,...(model.capabilities||[])].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
  return {...provider,models};
 }).filter(provider=>!query||provider.name.toLocaleLowerCase().includes(query)||provider.client.toLocaleLowerCase().includes(query)||provider.models.length);
 $("providers-list").innerHTML=providers.length?providers.map(provider=>{
  const models=provider.models.map((model,index)=>{
   const capabilities=(model.capabilities||[]).map(capability=>'<span class="tool-card-tag">'+escapeHtml(formatCapability(capability))+'</span>').join("");
   return '<div class="provider-model" data-model-index="'+index+'"><div class="provider-model-heading"><div><strong>'+escapeHtml(model.name)+'</strong><span class="tool-card-tag">'+(model.capabilities||[]).length+' capabilities</span></div><button class="secondary provider-test-button" type="button" data-test-model="'+escapeHtml(model.name)+'"> + testIcon + <span>Test</span></button></div><div class="agent-tags">'+(capabilities||'<span class="usage-empty">No capabilities declared</span>')+'</div><div class="provider-model-result" aria-live="polite"></div></div>';
  }).join("");
  return '<article class="panel tool-card" data-provider="'+escapeHtml(provider.name)+'"><div class="tool-card-heading"><div><h3>'+escapeHtml(provider.name)+'</h3><p>Client adapter: '+escapeHtml(provider.client)+'</p></div><div class="provider-actions"><span class="tool-card-tag">'+(provider.models||[]).length+' models</span><button class="secondary provider-test-button" type="button" data-test-provider> + testIcon + <span>Test all</span></button></div></div><div class="provider-models">'+models+'</div></article>';
 }).join(""):'<p class="usage-empty">No providers or models match this search.</p>';
 const count=state.providers.reduce((total,provider)=>total+(provider.models||[]).length,0);
 $("providers-status").textContent=providers.length+" of "+state.providers.length+" providers · "+count+" configured models";
}
async function testModel(button){
 const card=button.closest("[data-provider]");
 const row=button.closest(".provider-model");
 const result=row.querySelector(".provider-model-result");
 button.disabled=true;
 button.innerHTML="<span>Testing…</span>";
 result.className="provider-model-result pending";
 result.textContent="Sending a test completion request…";
 try{
  const results=await submitTest(card.dataset.provider,button.dataset.testModel);
  if(!results.length)throw new Error("The server returned no test result");
  renderResult(result,results[0]);
 }catch(error){
  result.className="provider-model-result error";
  result.textContent="Test failed: "+error.message;
 }finally{
  button.disabled=false;
  button.innerHTML=testIcon+"<span>Test</span>";
 }
}
async function testProvider(button){
 const card=button.closest("[data-provider]");
 const rows=[...card.querySelectorAll(".provider-model")];
 const modelButtons=rows.map(row=>row.querySelector("[data-test-model]"));
 button.disabled=true;
 button.innerHTML="<span>Testing…</span>";
 for(const row of rows){
  const result=row.querySelector(".provider-model-result");
  result.className="provider-model-result pending";
  result.textContent="Waiting for test…";
 }
 modelButtons.forEach(modelButton=>modelButton.disabled=true);
 try{
  const results=await submitTest(card.dataset.provider,null);
  results.forEach(result=>{
   const row=rows.find(candidate=>candidate.querySelector("[data-test-model]")?.dataset.testModel===result.model);
   if(row)renderResult(row.querySelector(".provider-model-result"),result);
  });
 }catch(error){
  rows.forEach(row=>{
   const result=row.querySelector(".provider-model-result");
   if(result.classList.contains("pending")){
    result.className="provider-model-result error";
    result.textContent="Test failed: "+error.message;
   }
  });
 }finally{
  button.disabled=false;
  button.innerHTML=testIcon+"<span>Test all</span>";
  modelButtons.forEach(modelButton=>modelButton.disabled=false);
 }
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
$("providers-list").addEventListener("click",event=>{
 const modelButton=event.target.closest("[data-test-model]");
 if(modelButton){testModel(modelButton);return;}
 const providerButton=event.target.closest("[data-test-provider]");
 if(providerButton)testProvider(providerButton);
});
load();
