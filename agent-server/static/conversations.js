const TERMINAL=new Set(["completed","failed","cancelled","timed_out","max_iterations_reached"]);
const mobileDrawer=()=>window.matchMedia("(max-width:760px)").matches;
function setDrawerOpen(open){const sidebar=$("conversation-sidebar"),backdrop=$("drawer-backdrop"),toggle=$("open-conversations");if(!sidebar||!backdrop||!toggle)return;sidebar.classList.toggle("open",open);backdrop.classList.toggle("open",open);toggle.setAttribute("aria-expanded",String(open));backdrop.setAttribute("aria-hidden",String(!open));document.body.style.overflow=open?"hidden":"";}

function closeDrawer(){setDrawerOpen(false)}

function renderAgents(){const previous=$("agent").value;$("agent").innerHTML=state.agents.map(a=>'<option value="'+escapeHtml(a.name)+'">'+escapeHtml(a.name)+'</option>').join("");if(state.agents.some(a=>a.name===previous))$("agent").value=previous;updateAgentDescription()}

function updateAgentDescription(){const a=state.agents.find(x=>x.name===$("agent").value);$("agent-desc").textContent=a?[a.model,(a.tools||[]).length+" tools",a.description].filter(Boolean).join(" · "):""}

function renderConversations(){$("conversations").innerHTML=state.conversations.length?state.conversations.map(c=>'<li data-id="'+escapeHtml(c.id)+'" class="'+(c.id===state.selected?"active":"")+'"><div class="conversation-row"><div class="conversation-main"><div class="conv-title">'+escapeHtml(c.title||"New conversation")+'</div><div class="conv-agent">'+escapeHtml(c.agent)+'</div><div class="conv-meta">'+new Date(c.updated_at).toLocaleString()+' · '+c.message_count+' messages</div></div><button type="button" class="danger conversation-delete" data-delete-id="'+escapeHtml(c.id)+'" aria-label="Delete '+escapeHtml(c.title||"conversation")+'" title="Delete conversation">Delete</button></div></li>').join():'<li style="cursor:default;color:var(--muted)">No conversations yet.</li>'}

function messageParts(message){if(!message||typeof message!=="object")return null;const [kind,value]=Object.entries(message)[0]||[];if(!kind)return null;return {kind:kind.toLowerCase(),value:value||{}}}

function renderMarkdown(source){
 if(typeof marked==="undefined"||typeof DOMPurify==="undefined")return escapeHtml(source).replace(/\\n/g,"<br>");
 const html=marked.parse(String(source??""),{gfm:true,breaks:false});
 return DOMPurify.sanitize(html,{USE_PROFILES:{html:true},FORBID_TAGS:["img","style","iframe","svg","math"]});
}

function renderMessages(messages=[]){
 const rendered=[],toolEntriesById=new Map(),unmatchedToolResults=[];
 const prettyArguments=value=>{
  if(value===undefined||value===null)return null;
  if(typeof value==="string"){try{return JSON.stringify(JSON.parse(value),null,2)}catch{return value}}
  try{return JSON.stringify(value,null,2)}catch{return String(value)}
 };
 for(const message of messages){
  const part=messageParts(message);if(!part)continue;
  const {kind,value}=part;
  if(kind==="user"){
   rendered.push('<article class="message user"><div class="role">You</div>'+escapeHtml(value.content)+'</article>');
  }else if(kind==="assistant"){
   if(value.content)rendered.push('<article class="message assistant"><div class="role">Assistant</div><div class="markdown-content">'+renderMarkdown(value.content)+'</div></article>');
   for(const call of value.tool_calls||[]){
    const fn=call.function||{};
    const entry={id:call.id||null,name:call.name||fn.name||"unknown",arguments:call.arguments??fn.arguments??{},result:null,isError:false};
    rendered.push(entry);
    if(entry.id)toolEntriesById.set(entry.id,entry);
   }
  }else if(kind==="tool"){
   const entry=value.tool_call_id?toolEntriesById.get(value.tool_call_id):null;
   if(entry){entry.result=value.content??"";entry.isError=!!value.is_error}
   else unmatchedToolResults.push({id:value.tool_call_id||null,name:value.name||"unknown",arguments:null,result:value.content??"",isError:!!value.is_error});
  }
 }
 rendered.push(...unmatchedToolResults);
 const html=rendered.map(item=>{
  if(typeof item==="string")return item;
  const args=prettyArguments(item.arguments);
  const status=item.isError?'<span class="tool-error">Error</span>':(item.result===null?'<span class="tool-status">Running</span>':'<span class="tool-status">Completed</span>');
  return '<details class="message tool tool-details"><summary aria-label="Show details for tool '+escapeHtml(item.name)+'"><span class="tool-name">Tool · '+escapeHtml(item.name)+'</span>'+status+'</summary><div class="tool-detail-body">'+(args!==null?'<section class="tool-section"><div class="tool-section-title">Arguments</div><pre>'+escapeHtml(args)+'</pre></section>':'')+(item.result!==null?'<section class="tool-section"><div class="tool-section-title">Result</div><pre>'+escapeHtml(item.result)+'</pre></section>':'<section class="tool-section"><div class="tool-section-title">Result</div><pre>Pending</pre></section>')+'</div></details>';
 }).join("");
 $("messages").innerHTML=html||'<div class="empty">Send the first message to begin this conversation.</div>';
 $("messages").scrollTop=$("messages").scrollHeight
}

function renderRunEvents(events=[]){$("run-events").innerHTML=events.length?events.map(event=>"<li>"+escapeHtml(event.kind)+(event.tool_name?" · "+escapeHtml(event.tool_name):"")+"</li>").join(""):"<li>No events yet.</li>"}

function setComposerEnabled(enabled){$("prompt").disabled=!enabled||!!state.runId;$("send").disabled=!enabled||!!state.runId}

function loadConversations(){state.conversations=await api("/conversations");renderConversations()}

function renderFamilySelector(){const selector=$("family-selector");if(!selector)return;const selected=state.selectedFamilyId||"";selector.innerHTML='<option value="">No family</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");selector.value=state.families.some(f=>f.id===selected)?selected:"";selector.disabled=!state.selected||!!state.runId;state.selectedFamilyId=selector.value||null}

function selectConversation(id){if(mobileDrawer())closeDrawer();state.selected=id;renderConversations();$("messages").innerHTML='<div class="empty">Loading conversation…</div>';try{const c=await api("/conversations/"+encodeURIComponent(id));$("conversation-title").textContent=c.title||"New conversation";$("conversation-meta").textContent="Agent · "+c.agent+" · Conversation "+c.id;$("rename-conversation").disabled=false;$("agent").value=c.agent;updateAgentDescription();state.selectedFamilyId=c.selected_family_id||null;renderFamilySelector();state.messageCount=(c.messages||[]).length;renderMessages(c.messages);setComposerEnabled(true);setStatus("")}catch(e){setStatus(e.message,"error")}}

function createConversation(){const agent=$("agent").value;if(!agent)return;try{const c=await api("/conversations",{method:"POST",body:JSON.stringify({agent})});await loadConversations();await selectConversation(c.id);$("prompt").focus()}catch(e){setStatus(e.message,"error")}}

function renameConversation(){if(!state.selected)return;const current=state.conversations.find(c=>c.id===state.selected);if(!current)return;const title=window.prompt("Conversation name",current.title||"New conversation");if(title===null)return;if(!title.trim()){setStatus("Conversation name must not be empty","error");return}try{const updated=await api("/conversations/"+encodeURIComponent(state.selected),{method:"PUT",body:JSON.stringify({title})});state.conversations=state.conversations.map(c=>c.id===updated.id?{...c,...updated}:c);$("conversation-title").textContent=updated.title;$("conversation-meta").textContent="Agent · "+updated.agent+" · Conversation "+updated.id;renderConversations();setStatus("Conversation renamed","ok")}catch(e){setStatus(e.message,"error")}}

function clearSelectedConversation(){state.selected=null;state.selectedFamilyId=null;renderFamilySelector();state.messageCount=0;$("conversation-title").textContent="No conversation selected";$("conversation-meta").textContent="Create a conversation to get started";$("rename-conversation").disabled=true;$("messages").innerHTML='<div class="empty">Choose an existing conversation or create a new one.</div>';$("prompt").value="";setComposerEnabled(false);renderConversations()}

function deleteConversation(id){const conversation=state.conversations.find(c=>c.id===id);if(!conversation)return;const title=conversation.title||"New conversation";if(!window.confirm('Permanently delete "'+title+'"? This cannot be undone.'))return;try{await api("/conversations/"+encodeURIComponent(id),{method:"DELETE"});state.conversations=state.conversations.filter(c=>c.id!==id);if(state.selected===id)clearSelectedConversation();else renderConversations();setStatus("Conversation deleted","ok")}catch(e){setStatus(e.message,"error")}}

function sendMessage(){const input=$("prompt").value.trim();if(!input||!state.selected||state.runId)return;const id=state.selected,previousMessageCount=state.messageCount;$("prompt").value="";setComposerEnabled(false);setStatus("Queued…");try{const result=await api("/conversations/"+encodeURIComponent(id)+"/runs",{method:"POST",body:JSON.stringify({input})});state.runId=result.run_id;$("cancel-run").hidden=false;setStatus("Running · "+result.run_id.slice(0,8));await pollRun(id,result.run_id,previousMessageCount)}catch(e){setStatus(e.message,"error");setComposerEnabled(true);$("prompt").value=input}}

function pollRun(conversationId,runId,previousMessageCount){clearInterval(state.pollTimer);renderRunEvents([]);const poll=async()=>{try{const [run,events]=await Promise.all([api("/runs/"+encodeURIComponent(runId)),api("/runs/"+encodeURIComponent(runId)+"/events")]);renderRunEvents(events);if(!TERMINAL.has(run.status)){setStatus("Running · "+run.status);return}clearInterval(state.pollTimer);state.runId=null;$("cancel-run").hidden=true;let conversation=null;for(let attempt=0;attempt<40;attempt++){conversation=await api("/conversations/"+encodeURIComponent(conversationId));if((conversation.messages||[]).length>previousMessageCount)break;await new Promise(resolve=>setTimeout(resolve,100))}if(state.selected===conversationId){$("conversation-title").textContent=conversation.title||"New conversation";$("conversation-meta").textContent="Agent · "+conversation.agent+" · Conversation "+conversation.id;state.messageCount=(conversation.messages||[]).length;renderMessages(conversation.messages);setComposerEnabled(true)}await loadConversations();if(run.status==="completed")setStatus("Completed","ok");else setStatus(run.error||run.status,"error");setComposerEnabled(state.selected===conversationId);renderFamilySelector()}catch(e){clearInterval(state.pollTimer);state.runId=null;$("cancel-run").hidden=true;setStatus("Unable to retrieve run: "+e.message,"error");setComposerEnabled(true);renderFamilySelector()}};await poll();if(state.runId)state.pollTimer=setInterval(poll,1000)}
async function initializeConversations(){
 state.agents=await api("/agents");renderAgents();await Promise.all([loadFamilies(),loadConversations()]);
 if(state.conversations.length)await selectConversation(state.conversations[0].id);else renderFamilySelector();
}
on("open-conversations","click",()=>setDrawerOpen(true));
on("close-conversations","click",closeDrawer);
on("drawer-backdrop","click",closeDrawer);
on("rename-conversation","click",renameConversation);
on("cancel-run","click",async()=>{if(!state.runId)return;try{await api("/runs/"+encodeURIComponent(state.runId)+"/cancel",{method:"POST"});setStatus("Cancellation requested")}catch(e){setStatus(e.message,"error")}});
on("agent","change",updateAgentDescription);
on("new-conversation","click",createConversation);
on("conversations","click",e=>{const deleteButton=e.target.closest("button[data-delete-id]");if(deleteButton){e.stopPropagation();if(!state.runId)deleteConversation(deleteButton.dataset.deleteId);return}const item=e.target.closest("li[data-id]");if(item&&!state.runId)selectConversation(item.dataset.id)});
on("composer","submit",e=>{e.preventDefault();sendMessage()});
on("prompt","keydown",e=>{if(e.key==="Enter"&&!e.shiftKey){e.preventDefault();$("composer").requestSubmit()}});
on("refresh","click",async()=>{try{await Promise.all([loadConversations(),loadFamilies()]);if(state.managedFamilyId)await loadFamilyDetails(state.managedFamilyId)}catch(e){setStatus(e.message,"error")}});
document.addEventListener("keydown",e=>{if(e.key==="Escape")closeDrawer()});
window.addEventListener("resize",()=>{if(!mobileDrawer())closeDrawer()});
