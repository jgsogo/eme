const $=id=>document.getElementById(id);
const number=value=>Number(value||0).toLocaleString();
const escapeHtml=value=>String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const dimension=(record,name)=>record.usage?.dimensions?.[name]||{source:"unavailable"};
const quantity=(record,name)=>{const item=dimension(record,name);return typeof item.value==="number"?item.value:null};
const money=(micros,currency)=>{try{return new Intl.NumberFormat(undefined,{style:"currency",currency:currency,maximumFractionDigits:4}).format(micros/1000000)}catch{return (micros/1000000).toFixed(4)+" "+currency}};
const costLabel=cost=>cost?money(cost.amount_micros,cost.currency):"Unpriced";
const state={records:[]};

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
 let reported=0,calculated=0,estimated=0,incomplete=0,unpriced=0;
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
 const max=Math.max(1,...models.map(m=>m.input+m.output));
 $("usage-models").innerHTML=models.length?models.map(model=>{
  const total=model.input+model.output;
  const inputWidth=total?model.input/max*100:0;
  const outputWidth=total?model.output/max*100:0;
  const costs=Array.from(model.costs).map(([currency,amount])=>money(amount,currency)).join(" · ")||"Unpriced";
  return '<div class="usage-model-row"><div class="usage-model-name"><strong>'+escapeHtml(model.provider)+'</strong><small>'+escapeHtml(model.model)+' · '+number(model.calls)+' calls</small></div><div class="usage-bar-track" title="Input '+number(model.input)+', output '+number(model.output)+'"><span class="usage-bar-input" style="width:'+inputWidth+'%"></span><span class="usage-bar-output" style="width:'+outputWidth+'%"></span></div><div class="usage-model-total">'+number(total)+' tokens<br><small>'+escapeHtml(costs)+'</small></div></div>';
 }).join("")+'<div class="usage-legend"><span>Input tokens</span><span>Output tokens</span></div>':'<p class="usage-empty">No usage records for this period.</p>';

 const recent=records.slice().sort((a,b)=>new Date(b.occurred_at)-new Date(a.occurred_at));
 $("usage-record-count").textContent=number(recent.length)+" records";
 $("usage-records").innerHTML=recent.length?recent.map(record=>{
  const input=quantity(record,"input_tokens"),output=quantity(record,"output_tokens");
  const source=record.cost?record.cost.origin.replaceAll("_"," ")+" · "+record.cost.status:"No matching price";
  return '<tr><td>'+escapeHtml(new Date(record.occurred_at).toLocaleString())+'</td><td>'+escapeHtml(record.agent)+'</td><td>'+escapeHtml(record.provider)+'<span class="usage-model-id">'+escapeHtml(record.model)+'</span></td><td><span class="usage-outcome '+escapeHtml(record.outcome)+'">'+escapeHtml(record.outcome.replaceAll("_"," "))+'</span></td><td>'+(input===null?"—":number(input))+'</td><td>'+(output===null?"—":number(output))+'</td><td>'+escapeHtml(costLabel(record.cost))+'<span class="usage-cost-detail">'+escapeHtml(source)+'</span></td></tr>';
 }).join(""):'<tr><td colspan="7" class="usage-empty">No usage records for this period.</td></tr>';
}
async function load(){
 $("usage-status").textContent="Loading usage records…";
 $("usage-status").className="usage-status";
 try{
  const response=await fetch("/runs/usage",{headers:{Accept:"application/json"}});
  if(!response.ok)throw new Error(await response.text()||response.statusText);
  state.records=await response.json();
  render();
  $("usage-status").textContent="Updated "+new Date().toLocaleTimeString()+" · "+number(state.records.length)+" stored invocations";
 }catch(error){
  $("usage-status").textContent="Could not load usage: "+error.message;
  $("usage-status").className="usage-status error";
 }
}
$("usage-refresh").addEventListener("click",load);
$("usage-period").addEventListener("change",render);
load();
