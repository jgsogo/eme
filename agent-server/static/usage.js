const $=id=>document.getElementById(id);
const number=value=>Number(value||0).toLocaleString();
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const dimension=(record,name)=>record.usage?.dimensions?.[name]||{source:"unavailable"};
const quantity=(record,name)=>{const item=dimension(record,name);return typeof item.value==="number"?item.value:null};
const money=(micros,currency)=>{try{return new Intl.NumberFormat(undefined,{style:"currency",currency:currency,maximumFractionDigits:4}).format(micros/1000000)}catch{return (micros/1000000).toFixed(4)+" "+currency}};
const costLabel=cost=>cost?money(cost.amount_micros,cost.currency):"Unpriced";
const state={records:[],pricing:[]};

function selectedRecords(){
 const days=$("usage-period").value;
 if(days==="all")return state.records;
 const since=Date.now()-Number(days)*24*60*60*1000;
 return state.records.filter(record=>new Date(record.occurred_at).getTime()>=since);
}
function render(){
 const records=selectedRecords();
 const sum=name=>records.reduce((total,record)=>total+(quantity(record,name)??0),0);
 const succeeded=records.filter(r=>r.outcome==="succeeded").length;
 const failed=records.filter(r=>r.outcome==="failed").length;
 const timedOut=records.filter(r=>r.outcome==="timed_out").length;
 const cancelled=records.filter(r=>r.outcome==="cancelled").length;
 $("metric-invocations").textContent=number(records.length);
 $("metric-outcomes").textContent=number(succeeded)+" successful · "+number(failed)+" failed · "+number(timedOut)+" timed out · "+number(cancelled)+" cancelled";
 $("metric-input").textContent=number(sum("input_tokens"));
 $("metric-cached").textContent="Cached input: "+number(sum("cached_input_tokens"));
 $("metric-output").textContent=number(sum("output_tokens"));
 $("metric-reasoning").textContent="Reasoning: "+number(sum("reasoning_tokens"));

 const costs=new Map();
 let reported=0,calculated=0,estimated=0,incomplete=0,unpriced=0,estimatedDimensions=0,unavailableDimensions=0;
 for(const record of records){
  for(const item of Object.values(record.usage?.dimensions||{})){
   if(item.source==="estimated")estimatedDimensions++;
   if(item.source==="unavailable")unavailableDimensions++;
  }
 }
 for(const record of records){
  const cost=record.cost;
  if(!cost){unpriced++;continue}
  if(cost.status!=="complete")incomplete++;
  if(cost.origin==="provider_reported")reported++;
  else if(cost.origin==="estimated")estimated++;
  else calculated++;
  costs.set(cost.currency,(costs.get(cost.currency)||0)+cost.amount_micros);
 }
 const costText=Array.from(costs).map(([currency,amount])=>money(amount,currency)).join(" · ");
 $("metric-cost").textContent=costText||"—";
 $("metric-cost-detail").textContent=number(reported)+" provider-reported · "+number(calculated)+" calculated · "+number(estimated)+" estimated";
 $("quality-reported").textContent=number(reported);
 $("quality-calculated").textContent=number(calculated);
 $("quality-estimated").textContent=number(estimated);
 $("quality-incomplete").textContent=number(incomplete)+" partial · "+number(unpriced)+" unpriced";
 $("quality-estimated-tokens").textContent=number(estimatedDimensions);
 $("quality-unavailable-tokens").textContent=number(unavailableDimensions);
 $("quality-failed").textContent=number(failed+timedOut);

 const groups=new Map();
 for(const record of records){
  const key=record.provider+"/"+record.model;
  if(!groups.has(key))groups.set(key,{provider:record.provider,model:record.model,calls:0,input:0,output:0,costs:new Map()});
  const group=groups.get(key);group.calls++;
  group.input+=quantity(record,"input_tokens")??0;
  group.output+=quantity(record,"output_tokens")??0;
  if(record.cost)group.costs.set(record.cost.currency,(group.costs.get(record.cost.currency)||0)+record.cost.amount_micros);
 }
 const models=Array.from(groups.values()).sort((a,b)=>(b.input+b.output)-(a.input+a.output));
 const modelQuery=$("usage-model-search")?.value.trim().toLocaleLowerCase()||"";
 const visibleModels=models.filter(model=>[model.provider,model.model].some(value=>String(value||"").toLocaleLowerCase().includes(modelQuery)));
 const max=Math.max(1,...visibleModels.map(m=>m.input+m.output));
 if($("usage-model-count"))$("usage-model-count").textContent=modelQuery?visibleModels.length+" of "+models.length+" models":models.length+" models";
 $("usage-models").innerHTML=visibleModels.length?visibleModels.map(model=>{
  const total=model.input+model.output;
  const inputWidth=total?model.input/max*100:0;
  const outputWidth=total?model.output/max*100:0;
  const costs=Array.from(model.costs).map(([currency,amount])=>money(amount,currency)).join(" · ")||"Unpriced";
  return '<div class="usage-model-row"><div class="usage-model-name"><strong>'+escapeHtml(model.provider)+'</strong><small>'+escapeHtml(model.model)+' · '+number(model.calls)+' calls</small></div><div class="usage-bar-track" title="Input '+number(model.input)+', output '+number(model.output)+'"><span class="usage-bar-input" style="width:'+inputWidth+'%"></span><span class="usage-bar-output" style="width:'+outputWidth+'%"></span></div><div class="usage-model-total">'+number(total)+' tokens<br><small>'+escapeHtml(costs)+'</small></div></div>';
 }).join("")+'<div class="usage-legend"><span>Input tokens</span><span>Output tokens</span></div>':models.length?'<p class="usage-empty">No models match this filter. <button type="button" class="secondary" data-clear-model-filter>Clear filter</button></p>':'<p class="usage-empty">No usage records for this period.</p>';

 const recent=records.slice().sort((a,b)=>new Date(b.occurred_at)-new Date(a.occurred_at));
 const query=$("usage-record-search")?.value.trim().toLocaleLowerCase()||"";
 const visible=recent.filter(record=>[record.run_id,record.agent,record.provider,record.model,record.outcome,record.cost?.origin,record.cost?.pricing_version].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 $("usage-record-count").textContent=query?number(visible.length)+" of "+number(recent.length)+" records":number(recent.length)+" records";
 $("usage-records").innerHTML=visible.length?visible.map(record=>{
  const input=quantity(record,"input_tokens"),output=quantity(record,"output_tokens");
  const source=record.cost?record.cost.origin.replaceAll("_"," ")+" · "+record.cost.status+" · "+record.cost.pricing_version:"No matching price";
  return '<tr><td>'+escapeHtml(new Date(record.occurred_at).toLocaleString())+'</td><td title="'+escapeHtml(record.run_id)+'"><a href="'+(document.body.dataset.layout==="mobile"?"/mobile/admin/usage":"/console/admin/usage")+'?run_id='+encodeURIComponent(record.run_id)+'">'+escapeHtml(String(record.run_id).slice(0,8))+'</a></td><td>'+escapeHtml(record.agent)+'</td><td>'+escapeHtml(record.provider)+'<span class="usage-model-id">'+escapeHtml(record.model)+'</span></td><td><span class="usage-outcome '+escapeHtml(record.outcome)+'">'+escapeHtml(record.outcome.replaceAll("_"," "))+'</span></td><td>'+(input===null?"—":number(input))+'</td><td>'+(output===null?"—":number(output))+'</td><td>'+escapeHtml(costLabel(record.cost))+'<span class="usage-cost-detail">'+escapeHtml(source)+'</span></td></tr>';
 }).join(""):'<tr><td colspan="8" class="usage-empty">'+(query?'No invocations match this filter.':'No usage records for this period.')+'</td></tr>';
}
async function load(){
 const runId=new URLSearchParams(window.location.search).get("run_id");
 const refresh=$("usage-refresh");if(refresh)refresh.disabled=true;
 $("usage-status").textContent=runId?"Loading run usage…":"Loading usage records…";
 $("usage-status").className="usage-status";
 $("usage-models").innerHTML='<p class="usage-empty" role="status">Loading usage…</p>';
 $("usage-records").innerHTML='<tr><td colspan="8" class="usage-empty" role="status">Loading invocations…</td></tr>';
 try{
  const endpoint=runId?"/runs/"+encodeURIComponent(runId)+"/usage":"/runs/usage";
  const response=await fetch(endpoint,{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  const payload=await response.json();
  state.records=runId?(payload.invocations||[]):payload;
  if(runId){
   document.querySelector(".usage-heading h2").textContent="Run usage & costs";
   document.querySelector(".usage-intro").textContent="Accounting for run "+runId;
  }
  render();
  const count=runId?(payload.summary?.invocation_count??state.records.length):state.records.length;
  $("usage-status").textContent=(runId?"Run ":"Updated ")+new Date().toLocaleTimeString()+" · "+number(count)+" stored invocations";
 }catch(error){
  $("usage-status").textContent="Could not load usage: "+error.message;
  $("usage-status").className="usage-status error";
  $("usage-models").innerHTML='<p class="usage-empty" role="alert">Usage data could not be loaded.</p>';
  $("usage-records").innerHTML=refresh?'<tr><td colspan="8" class="usage-empty" role="alert">Usage data could not be loaded. Use Refresh to try again.</td></tr>':'<tr><td colspan="8" class="usage-empty" role="alert">Usage data could not be loaded. <button type="button" class="secondary" data-retry-usage>Try again</button></td></tr>';
 }finally{if(refresh)refresh.disabled=false}
}
function renderPricing(){
 const rate=value=>value==null?"—":(value/1000000).toFixed(6);
 const date=value=>value?new Date(value).toLocaleString():"No end date";
 const query=$("pricing-search")?.value.trim().toLocaleLowerCase()||"";
 const prices=state.pricing.filter(price=>[price.provider,price.model,price.version,price.currency,price.effective_from,price.effective_until].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 if($("pricing-count"))$("pricing-count").textContent=query?prices.length+" of "+state.pricing.length+" versions":state.pricing.length+" versions";
 $("pricing-versions").innerHTML=prices.length?'<table class="pricing-table"><thead><tr><th scope="col">Provider / model</th><th scope="col">Version</th><th scope="col">Currency</th><th scope="col">Effective</th><th scope="col">Input / 1M</th><th scope="col">Output / 1M</th><th scope="col">Cached / 1M</th><th scope="col">Reasoning / 1M</th></tr></thead><tbody>'+prices.map(price=>{
  const rates=price.rates_per_million||{};
  return '<tr><td>'+escapeHtml(price.provider)+'<span class="usage-model-id">'+escapeHtml(price.model)+'</span></td><td>'+escapeHtml(price.version)+'</td><td>'+escapeHtml(price.currency)+'</td><td>'+escapeHtml(date(price.effective_from))+'<span class="usage-model-id">'+escapeHtml(price.effective_until?date(price.effective_until):"Open-ended")+'</span></td><td>'+rate(rates.input_tokens)+'</td><td>'+rate(rates.output_tokens)+'</td><td>'+rate(rates.cached_input_tokens)+'</td><td>'+rate(rates.reasoning_tokens)+'</td></tr>';
 }).join("")+'</tbody></table>':query?'<p class="usage-empty">No pricing versions match this filter. <button type="button" class="secondary" data-clear-pricing-filter>Clear filter</button></p>':'<p class="usage-empty">No pricing versions configured yet.</p>';
}
async function loadPricing(){
 $("pricing-versions").innerHTML='<p class="usage-empty" role="status">Loading pricing catalogue…</p>';
 try{
  const response=await fetch("/runs/pricing",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.pricing=await response.json();renderPricing();
 }catch(error){
  $("pricing-versions").innerHTML='<p class="usage-empty" role="alert">Could not load pricing catalogue: '+escapeHtml(error.message)+' <button type="button" class="secondary" data-retry-pricing>Try again</button></p>';
 }
}
async function savePricing(event){
 event.preventDefault();
 const submitButton=event.submitter||$("pricing-form").querySelector('[type="submit"]');
 if(submitButton)submitButton.disabled=true;
 const status=$("pricing-status");
 const readRate=id=>{
  const raw=$(id).value.trim();
  if(!raw)return null;
  const value=Number(raw);
  if(!Number.isFinite(value)||value<0)throw new Error("Rates must be non-negative numbers.");
  const micros=Math.round(value*1000000);
  if(!Number.isSafeInteger(micros))throw new Error("Rate is too large.");
  return micros;
 };
 try{
  const effectiveFrom=new Date($("price-from").value);
  if(Number.isNaN(effectiveFrom.getTime()))throw new Error("Choose a valid effective-from date.");
  const untilValue=$("price-until").value;
  const effectiveUntil=untilValue?new Date(untilValue):null;
  if(effectiveUntil&&Number.isNaN(effectiveUntil.getTime()))throw new Error("Choose a valid effective-until date.");
  const rates={
   input_tokens:readRate("price-input"),
   output_tokens:readRate("price-output"),
   cached_input_tokens:readRate("price-cached"),
   reasoning_tokens:readRate("price-reasoning")
  };
  if(Object.values(rates).every(value=>value===null))throw new Error("Enter at least one token rate.");
  const payload={
   provider:$("price-provider").value.trim(),
   model:$("price-model").value.trim(),
   version:$("price-version").value.trim(),
   currency:$("price-currency").value.trim().toUpperCase(),
   effective_from:effectiveFrom.toISOString(),
   effective_until:effectiveUntil?effectiveUntil.toISOString():null,
   rates_per_million:rates
  };
  status.textContent="Saving pricing version…";
  const response=await fetch("/runs/pricing",{
   method:"POST",
   headers:{"Content-Type":"application/json",Accept:"application/json"},
   body:JSON.stringify(payload)
  });
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  status.textContent="Pricing version saved.";
  $("pricing-form").reset();
  $("price-currency").value="USD";
  await Promise.all([loadPricing(),load()]);
 }catch(error){
  status.textContent="Could not save pricing: "+error.message;
 }finally{if(submitButton)submitButton.disabled=false}
}
$("usage-refresh")?.addEventListener("click",()=>{load();loadPricing()});
$("usage-period").addEventListener("change",render);
$("usage-record-search")?.addEventListener("input",render);
$("usage-model-search")?.addEventListener("input",render);
$("usage-models")?.addEventListener("click",event=>{if(event.target.closest("[data-clear-model-filter]")){$("usage-model-search").value="";render()}});
$("pricing-search")?.addEventListener("input",renderPricing);
$("pricing-versions")?.addEventListener("click",event=>{
 if(event.target.closest("[data-clear-pricing-filter]")){$("pricing-search").value="";renderPricing()}
 if(event.target.closest("[data-retry-pricing]"))loadPricing();
});
$("usage-records")?.addEventListener("click",event=>{if(event.target.closest("[data-retry-usage]")){load();loadPricing()}});
$("pricing-form").addEventListener("submit",savePricing);
load();
loadPricing();


// Execution history is a separate sub-view of Admin. The run API includes
// durable records, including runs whose temporary one-shot conversation was
// soft-deleted after completion.
const EXECUTION_PAGE_SIZE=25;
const executionState={runs:[],selected:null,details:{},offset:0,hasMore:false};
const executionDate=value=>value?new Date(value).toLocaleString():"—";
const executionDuration=run=>{
 if(typeof run.duration_ms==="number"){
  const seconds=Math.max(0,Math.round(run.duration_ms/1000));
  return seconds<60?seconds+"s":Math.floor(seconds/60)+"m "+(seconds%60)+"s";
 }
 const start=run.started_at?new Date(run.started_at).getTime():new Date(run.created_at).getTime();
 const end=run.finished_at?new Date(run.finished_at).getTime():Date.now();
 if(!Number.isFinite(start)||!Number.isFinite(end))return "—";
 const seconds=Math.max(0,Math.round((end-start)/1000));
 return seconds<60?seconds+"s":Math.floor(seconds/60)+"m "+(seconds%60)+"s";
};
function showAdminView(view){
 const history=view==="execution-history";
 $("observability-view").hidden=history;
 $("execution-history-view").hidden=!history;
 document.querySelectorAll("[data-admin-view]").forEach(link=>{
  const active=link.dataset.adminView===(history?"execution-history":"observability");
  link.classList.toggle("active",active);
  if(active)link.setAttribute("aria-current","page");else link.removeAttribute("aria-current");
 });
 if(history)loadExecutionHistory(0);
}
function renderExecutionHistory(){
 const query=$("execution-search")?.value.trim().toLocaleLowerCase()||"";
 const runs=executionState.runs.filter(run=>[
  run.agent,run.input,run.status,run.run_id,run.type,run.task_name
 ].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 const start=executionState.offset+1;
 const end=executionState.offset+runs.length;
 $("execution-count").textContent=runs.length?("Showing "+start+"–"+end+(executionState.hasMore?" · more available":"")):"No executions on this page";
 $("execution-records").innerHTML=runs.length?runs.map(run=>{
  const type=run.type==="one-shot"?"One-shot":run.type==="task"?"Task":"Conversation";
  return '<tr class="execution-row" data-run-id="'+escapeHtml(run.run_id)+'" tabindex="0" role="button" aria-label="View '+escapeHtml(type.toLowerCase())+' execution for '+escapeHtml(run.agent)+'"><td>'+escapeHtml(executionDate(run.started_at||run.created_at))+'</td><td>'+escapeHtml(type)+'</td><td>'+escapeHtml(run.agent)+(run.task_name?'<span class="usage-model-id">'+escapeHtml(run.task_name)+'</span>':'')+'</td><td class="execution-prompt">'+escapeHtml(run.input)+'</td><td><span class="usage-outcome '+escapeHtml(run.status)+'">'+escapeHtml(String(run.status).replaceAll("_"," "))+'</span></td><td>'+escapeHtml(executionDuration(run))+'</td></tr>';
 }).join(""):'<tr><td colspan="6" class="usage-empty">No executions recorded on this page.</td></tr>';
 $("execution-previous").disabled=executionState.offset===0;
 $("execution-next").disabled=!executionState.hasMore;
}
async function loadExecutionHistory(offset=executionState.offset){
 executionState.offset=offset;
 $("execution-status").textContent="Loading executions…";
 $("execution-status").className="usage-status";
 $("execution-previous").disabled=true;
 $("execution-next").disabled=true;
 try{
  const requested=offset+EXECUTION_PAGE_SIZE;
  const [runsResponse,tasksResponse]=await Promise.all([
   fetch("/runs?offset=0&limit="+requested,{headers:{Accept:"application/json"}}),
   fetch("/tasks",{headers:{Accept:"application/json"}})
  ]);
  if(!runsResponse.ok)throw new Error(await runsResponse.text()||runsResponse.statusText);
  if(!tasksResponse.ok)throw new Error(await tasksResponse.text()||tasksResponse.statusText);
  const [runsPage,tasks]=await Promise.all([runsResponse.json(),tasksResponse.json()]);
  const taskPages=await Promise.all(tasks.map(async task=>{
   const response=await fetch("/tasks/"+encodeURIComponent(task.id)+"/executions?limit="+requested,{headers:{Accept:"application/json"}});
   if(!response.ok)throw new Error("Could not load executions for task "+(task.name||task.id)+": "+(await response.text()||response.statusText));
   const page=await response.json();
   const items=page.items.map(execution=>{
    const iso=value=>value==null?null:new Date(Number(value)).toISOString();
    const run={...execution,type:"task",task_id:task.id,task_name:task.name,
     created_at:iso(execution.created_at_unix_ms),started_at:iso(execution.started_at_unix_ms),
     finished_at:iso(execution.finished_at_unix_ms),duration_ms:execution.duration_ms};
    executionState.details[run.run_id]={run,events:execution.events||[],usage:{invocations:execution.usage||[]}};
    return run;
   });
   return {items,total:page.total};
  }));
  const conversationRuns=runsPage.items.map(run=>({...run,type:run.one_shot?"one-shot":"conversation"}));
  const allRuns=[...conversationRuns,...taskPages.flatMap(page=>page.items)].sort((a,b)=>new Date(b.started_at||b.created_at||0)-new Date(a.started_at||a.created_at||0));
  executionState.runs=allRuns.slice(offset,requested);
  executionState.hasMore=allRuns.length>requested||runsPage.has_more||taskPages.some(page=>page.total>requested);
  renderExecutionHistory();
  $("execution-status").textContent="Updated "+new Date().toLocaleTimeString()+" · page "+(Math.floor(offset/EXECUTION_PAGE_SIZE)+1);
 }catch(error){
  $("execution-status").textContent="Could not load executions: "+error.message;
  $("execution-status").className="usage-status error";
 }
}

function selectExecutionTab(tab){
 document.querySelectorAll("[data-execution-tab]").forEach(button=>button.classList.toggle("active",button.dataset.executionTab===tab));
 const detail=executionState.details[executionState.selected];
 const content=$("execution-detail-content");
 if(!detail)return;
 if(tab==="result"){
  content.innerHTML=detail.run.response?'<div class="markdown-content">'+(typeof marked!=="undefined"&&typeof DOMPurify!=="undefined"?DOMPurify.sanitize(marked.parse(detail.run.response,{gfm:true}),{USE_PROFILES:{html:true},FORBID_TAGS:["img","style","iframe","svg","math"]}):'<pre>'+escapeHtml(detail.run.response)+'</pre>')+'</div>':detail.run.error?'<p class="error">'+escapeHtml(detail.run.error)+'</p>':'<p class="usage-empty">No result is available yet.</p>';
 }else if(tab==="input"){
  content.innerHTML='<h4>Prompt</h4><pre class="execution-pre">'+escapeHtml(detail.run.input)+'</pre>';
 }else if(tab==="timeline"){
  content.innerHTML=detail.events.length?'<ol class="execution-timeline">'+detail.events.map(event=>'<li><strong>'+escapeHtml(String(event.kind).replaceAll("_"," "))+'</strong><time>'+escapeHtml(event.occurred_at_unix_ms?new Date(Number(event.occurred_at_unix_ms)).toLocaleString():"")+'</time>'+(event.tool_name?'<span>Tool: '+escapeHtml(event.tool_name)+'</span>':'')+(event.model_id?'<span>Model: '+escapeHtml(event.model_id)+'</span>':'')+(event.error_category?'<span class="error">Error category: '+escapeHtml(event.error_category)+'</span>':'')+'</li>').join("")+'</ol>':'<p class="usage-empty">No lifecycle events are available.</p>';
 }else{
  const invocations=detail.usage?.invocations||[];
  const totals={input:0,output:0,cached:0,reasoning:0};
  const get=(record,key)=>record.usage?.dimensions?.[key]?.value;
  invocations.forEach(record=>{totals.input+=get(record,"input_tokens")||0;totals.output+=get(record,"output_tokens")||0;totals.cached+=get(record,"cached_input_tokens")||0;totals.reasoning+=get(record,"reasoning_tokens")||0});
  const costs=new Map();
  invocations.forEach(record=>{if(record.cost)costs.set(record.cost.currency,(costs.get(record.cost.currency)||0)+record.cost.amount_micros)});
  const costText=Array.from(costs).map(([currency,value])=>money(value,currency)).join(" · ")||"Unpriced";
  content.innerHTML='<div class="execution-usage-grid"><div><span>Input tokens</span><strong>'+number(totals.input)+'</strong></div><div><span>Output tokens</span><strong>'+number(totals.output)+'</strong></div><div><span>Cached input</span><strong>'+number(totals.cached)+'</strong></div><div><span>Reasoning tokens</span><strong>'+number(totals.reasoning)+'</strong></div><div><span>Recorded cost</span><strong>'+escapeHtml(costText)+'</strong></div><div><span>Model invocations</span><strong>'+number(invocations.length)+'</strong></div></div>'+(invocations.length?'<div class="usage-table-wrap"><table class="usage-table"><thead><tr><th>Provider / model</th><th>Input</th><th>Output</th><th>Outcome</th><th>Cost</th></tr></thead><tbody>'+invocations.map(record=>'<tr><td>'+escapeHtml(record.provider)+'<span class="usage-model-id">'+escapeHtml(record.model)+'</span></td><td>'+escapeHtml(get(record,"input_tokens")??"—")+'</td><td>'+escapeHtml(get(record,"output_tokens")??"—")+'</td><td>'+escapeHtml(record.outcome||"—")+'</td><td>'+escapeHtml(costLabel(record.cost))+'</td></tr>').join("")+'</tbody></table></div>':'<p class="usage-empty">No model usage was recorded for this execution.</p>');
 }
}
async function openExecution(runId){
 const run=executionState.runs.find(item=>item.run_id===runId);
 if(!run)return;
 executionState.selected=runId;
 $("execution-detail-title").textContent=run.agent+" · "+String(run.run_id).slice(0,8);
 $("execution-detail-meta").textContent=(run.type==="task"?"Task · "+(run.task_name||"Scheduled task"):run.type==="one-shot"?"One-shot":"Conversation")+" · "+executionDate(run.created_at)+" · "+run.status+" · "+executionDuration(run);
 $("execution-detail-content").innerHTML='<p class="usage-empty">Loading execution details…</p>';
 const dialog=$("execution-detail");
 if(!dialog.open)dialog.showModal();
 try{
  let detail=executionState.details[runId];
  if(!detail){
   const [eventsResponse,usageResponse]=await Promise.all([
    fetch("/runs/"+encodeURIComponent(runId)+"/events",{headers:{Accept:"application/json"}}),
    fetch("/runs/"+encodeURIComponent(runId)+"/usage",{headers:{Accept:"application/json"}})
   ]);
   if(!eventsResponse.ok)throw new Error("Could not load execution timeline: "+eventsResponse.statusText);
   if(!usageResponse.ok)throw new Error("Could not load execution usage: "+usageResponse.statusText);
   const [events,usage]=await Promise.all([eventsResponse.json(),usageResponse.json()]);
   detail={run,events,usage};
   executionState.details[runId]=detail;
  }
  selectExecutionTab("result");
 }catch(error){$("execution-detail-content").innerHTML='<p class="usage-status error">Could not load execution details: '+escapeHtml(error.message)+'</p>'}
}
document.querySelectorAll("[data-admin-view]").forEach(link=>link.addEventListener("click",event=>{event.preventDefault();showAdminView(link.dataset.adminView);history.replaceState(null,"",link.href)}));
$("execution-search")?.addEventListener("input",renderExecutionHistory);
$("execution-previous")?.addEventListener("click",()=>loadExecutionHistory(Math.max(0,executionState.offset-EXECUTION_PAGE_SIZE)));
$("execution-next")?.addEventListener("click",()=>loadExecutionHistory(executionState.offset+EXECUTION_PAGE_SIZE));

$("execution-records")?.addEventListener("click",event=>{const row=event.target.closest("[data-run-id]");if(row)openExecution(row.dataset.runId)});
$("execution-records")?.addEventListener("keydown",event=>{if((event.key==="Enter"||event.key===" ")&&event.target.matches("[data-run-id]")){event.preventDefault();openExecution(event.target.dataset.runId)}});
$("execution-detail-close")?.addEventListener("click",()=>{$("execution-detail").close();executionState.selected=null});
$("execution-detail")?.addEventListener("close",()=>{executionState.selected=null});
$("execution-detail")?.addEventListener("click",event=>{if(event.target===$("execution-detail"))$("execution-detail").close()});
document.querySelectorAll("[data-execution-tab]").forEach(button=>button.addEventListener("click",()=>selectExecutionTab(button.dataset.executionTab)));
if(window.location.hash==="#execution-history")showAdminView("execution-history");
