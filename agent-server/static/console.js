const TERMINAL=new Set(["completed","failed","cancelled","timed_out","max_iterations_reached"]);
const page=document.body.dataset.page;
const consoleBase=document.body.dataset.layout==="mobile"?"/mobile":"/console";
const pendingInviteMatch=window.location.pathname.match(/^\/invite\/([a-fA-F0-9]{64})$/);
let pendingInviteToken=pendingInviteMatch?pendingInviteMatch[1]:null;
let familyActionHandler=null;
const state={googleConnections:[],googleCalendars:{},traktProfiles:[],traktSummaries:{},activeIntegration:"google",agents:[],families:[],familyDetails:null,familyInvitations:[],conversations:[],showArchived:false,conversationSearch:"",notes:[],selectedNote:null,tasks:[],taskExecutions:[],selectedTask:null,selected:null,selectedFamilyId:null,managedFamilyId:null,currentUserId:null,messageCount:0,messages:[],retry:null,runId:null,eventSource:null,runEvents:[],activeRuns:{}};
const $=id=>document.getElementById(id);
const on=(id,event,handler)=>$(id)?.addEventListener(event,handler);
function setProfileView(view){
 if(document.body.dataset.layout!=="mobile")return;
 document.body.dataset.profileView=view;
 const back=$("profile-content-back"),menu=$("open-profile");
 if(back)back.hidden=view!=="detail";
 if(menu)menu.hidden=view==="detail";
}
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const raw=await response.text();let body=null;try{body=raw?JSON.parse(raw):null}catch{body=raw}if(!response.ok)throw new Error(typeof body==="string"?body:body?.error||response.statusText);return body}
function escapeHtml(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function setStatus(message,kind=""){const node=$("run-status")||$("connection");if(node){node.textContent=message;node.className=(node.id==="run-status"?"run-status ":"hint ")+kind}}
function renderAgents(){const previous=$("agent")?.value||$("new-conversation-agent")?.value||"";const options=state.agents.map(a=>'<option value="'+escapeHtml(a.name)+'">'+escapeHtml(a.name)+'</option>').join("");if($("agent")){$("agent").innerHTML=options;if(state.agents.some(a=>a.name===previous))$("agent").value=previous;updateAgentDescription()}const dialogAgent=$("new-conversation-agent");if(dialogAgent){dialogAgent.innerHTML=options;if(state.agents.some(a=>a.name===previous))dialogAgent.value=previous}}
function updateAgentDescription(){const dialogSelector=$("new-conversation-agent"),selector=dialogSelector||$("agent"),description=dialogSelector?$("new-conversation-agent-desc"):$("agent-desc");if(!selector||!description)return;const a=state.agents.find(x=>x.name===selector.value);description.textContent=a?[a.description,(a.capabilities||[]).join(", "),(a.tools||[]).length+" tools"].filter(Boolean).join(" · "):""}
function renderConversations(){
 const query=state.conversationSearch.trim().toLocaleLowerCase();
 const conversations=state.conversations.filter(c=>!query||[c.title,c.agent,c.id].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 const search=$("conversation-search");if(search&&search.value!==state.conversationSearch)search.value=state.conversationSearch;
 const archived=$("show-archived");if(archived)archived.checked=state.showArchived;
 const legacyList=$("conversations");
 if(legacyList)legacyList.innerHTML=conversations.map(c=>'<li data-id="'+escapeHtml(c.id)+'">'+escapeHtml(c.title||"New conversation")+'</li>').join("");
 renderConversationsTable(conversations);
}
function renderConversationsTable(conversations){
 const body=$("conversations-table-body");if(!body)return;
 body.innerHTML=conversations.length?conversations.map(c=>{
  const family=state.families.find(f=>f.id===c.selected_family_id);
  const activeRun=state.activeRuns[c.id];
  const updated=c.updated_at?new Date(c.updated_at).toLocaleString():"—";
  const actionLabel=state.showArchived?"Restore":"Archive";
  const archiveIcon=state.showArchived?'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7h18v13H3zM2 3h20v4H2zM10 11l2 2 2-2M12 13V9"/></svg>':'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7h18v13H3zM2 3h20v4H2zM12 10v7m-3-3 3 3 3-3"/></svg>';
  const actions='<div class="conversation-card-actions"><button type="button" class="conversation-card-action" data-edit-id="'+escapeHtml(c.id)+'" aria-label="Edit conversation" title="Edit conversation" '+(activeRun?'disabled':'')+'><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20h9M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"/></svg></button><button type="button" class="conversation-card-action" data-archive-id="'+escapeHtml(c.id)+'" aria-label="'+actionLabel+' conversation" title="'+actionLabel+'">'+archiveIcon+'</button><button type="button" class="conversation-card-action danger" data-delete-id="'+escapeHtml(c.id)+'" aria-label="Remove conversation" title="Remove"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3m-9 0 1 14h10l1-14M10 11v6m4-6v6"/></svg></button></div>';
  return '<div class="item-card conversation-item-card" data-conversation-id="'+escapeHtml(c.id)+'" role="group" tabindex="0" aria-label="Conversation: '+escapeHtml(c.title||"New conversation")+'"><div class="conversation-card-heading"><strong>'+escapeHtml(c.title||"New conversation")+'</strong>'+actions+'</div><div class="conversation-card-agent">'+escapeHtml(c.agent||"No agent assigned")+'</div><div class="conversation-card-footer"><div class="conversation-card-stats"><span class="conversation-card-family">'+escapeHtml(family?.name||"Private")+'</span><span>'+escapeHtml(c.message_count||0)+' messages</span><span>~'+Number(c.history_tokens_estimate||0).toLocaleString()+' tokens</span>'+(activeRun?'<span class="conversation-card-running">● '+(activeRun.status==="queued"?"Queued":"Running")+'</span>':'')+'</div><span class="conversation-card-updated">Updated '+escapeHtml(updated)+'</span></div><span class="item-card-arrow" aria-hidden="true">›</span></div>';
 }).join(""):'<div class="tasks-table-empty"><strong>'+(state.conversations.length?"No conversations found":state.showArchived?"No archived conversations":"No conversations yet")+'</strong><span>'+(state.conversations.length?"Try changing your search.":"Create a conversation to get started.")+'</span></div>';
 const info=$("conversations-page-info");if(info)info.textContent=conversations.length?conversations.length+" conversations":"0 conversations";
}
function showConversationsOverview(){
 document.body.dataset.conversationView="list";
 $("conversations-overview").hidden=false;$("conversation-detail-view").hidden=true;
 if($("open-conversations"))$("open-conversations").hidden=false;$("back-to-conversations").hidden=true;
 $("conversations-main-title").textContent="All conversations";
 $("conversations-main-subtitle").textContent="Your conversations, ordered by most recently updated";
}
function showConversationDetail(){
 document.body.dataset.conversationView="detail";
 $("conversations-overview").hidden=true;$("conversation-detail-view").hidden=false;
 if($("open-conversations"))$("open-conversations").hidden=true;$("back-to-conversations").hidden=false;
 $("conversations-main-title").textContent="Conversation";
 $("conversations-main-subtitle").textContent="View and continue the conversation";
}

function messageParts(message){if(!message||typeof message!=="object")return null;const [kind,value]=Object.entries(message)[0]||[];if(!kind)return null;return {kind:kind.toLowerCase(),value:value||{}}}
function renderMarkdown(source){
 if(typeof marked==="undefined"||typeof DOMPurify==="undefined")return escapeHtml(source).replace(/\\n/g,"<br>");
 const html=marked.parse(String(source??""),{gfm:true,breaks:false});
 return DOMPurify.sanitize(html,{USE_PROFILES:{html:true},FORBID_TAGS:["img","style","iframe","svg","math"]});
}
function renderMessages(messages=[],pending=null,retry=null){
 const rendered=[],toolEntriesById=new Map(),unmatchedToolResults=[];
 const prettyArguments=value=>{
  if(value===undefined||value===null)return null;
  if(typeof value==="string"){try{return JSON.stringify(JSON.parse(value),null,2)}catch{return value}}
  try{return JSON.stringify(value,null,2)}catch{return String(value)}
 };
 for(let messageIndex=0;messageIndex<messages.length;messageIndex++){
  const message=messages[messageIndex];
  const part=messageParts(message);if(!part)continue;
  const {kind,value}=part;
  if(kind==="user"){
   const retryButton=retry&&retry.messageIndex===messageIndex?'<button type="button" class="secondary message-retry" data-retry-message>↻ Retry</button>':'';
   rendered.push('<article class="message user"><div class="role">You</div>'+escapeHtml(value.content)+retryButton+'</article>');
  }else if(kind==="assistant"){
   if(value.content||(value.tool_calls||[]).length||value.model_id||value.generated_at){
    const generatedAt=value.generated_at?new Date(value.generated_at):null;
    const timestamp=generatedAt&&!Number.isNaN(generatedAt.getTime())?generatedAt.toLocaleString(undefined,{dateStyle:"medium",timeStyle:"short"}):null;
    const metadata=[value.model_id,timestamp].filter(Boolean).map(escapeHtml).join(" · ");
    rendered.push('<article class="message assistant"><div class="message-heading"><div class="role">Assistant</div>'+(metadata?'<div class="message-metadata">'+metadata+'</div>':'')+'</div>'+(value.content?'<div class="markdown-content">'+renderMarkdown(value.content)+'</div>':'')+'</article>');
   }
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
 $("messages").innerHTML=(html||'<div class="empty">Send the first message to begin this conversation.</div>')+(pending?'<div class="pending-message" role="status" aria-live="polite"><span class="pending-spinner" aria-hidden="true"></span><span class="pending-label">'+escapeHtml(pending.label||"Thinking…")+'</span><button type="button" class="danger pending-cancel" id="cancel-run" '+(pending.cancelEnabled?"":"disabled")+' aria-label="Cancel this run">Cancel</button></div>':"");
 $("messages").scrollTop=$("messages").scrollHeight
}
function renderRunEvents(events=[]){
 const labels={queued:"Queued",started:"Run started",model_requested:"Model request",model_candidates_discovered:"Compatible models",model_candidate_attempt_started:"Trying model",model_candidate_attempt_failed:"Candidate failed",model_fallback:"Fallback",model_selected:"Model selected",model_responded:"Model response",tool_execution_started:"Tool started",tool_execution_finished:"Tool finished",model_failed:"Model request failed",runtime_cancelled:"Cancelled",runtime_timed_out:"Timed out",completed:"Completed",failed:"Run failed",cancelled:"Cancelled",timed_out:"Timed out",max_iterations_reached:"Maximum iterations reached"};
 $("run-events").innerHTML=events.length?events.map(event=>{
  const details=[];
  if(event.model_candidates?.length)details.push(event.model_candidates.join(" → "));
  if(event.model_id)details.push(event.model_id);
  if(event.next_model_id)details.push("→ "+event.next_model_id);
  if(event.tool_name)details.push(event.tool_name);
  if(event.error_category)details.push(event.error_category.replaceAll("_"," "));
  if(Number.isInteger(event.attempted_candidates))details.push(event.attempted_candidates+" attempted");
  const iteration=Number.isInteger(event.iteration)?"Iteration "+event.iteration+" · ":"";
  return '<li class="run-event run-event-'+escapeHtml(event.kind)+'"><strong>'+escapeHtml(labels[event.kind]||event.kind)+'</strong><span>'+escapeHtml(iteration+details.join(" · "))+'</span></li>';
 }).join(""):"<li>No events yet.</li>"
}
function setComposerEnabled(enabled){$("prompt").disabled=!enabled||!!state.runId;$("send").disabled=!enabled||!!state.runId;if($("edit-conversation-settings"))$("edit-conversation-settings").disabled=!state.selected||!!state.runId}
async function loadConversations(){const [conversations,runs]=await Promise.all([api(state.showArchived?"/conversations/archived":"/conversations"),api("/runs")]);state.conversations=conversations;state.activeRuns={};for(const run of runs){if(run.conversation_id&&["queued","running"].includes(run.status)&&!state.activeRuns[run.conversation_id])state.activeRuns[run.conversation_id]=run}renderConversations()}
async function loadFamilies(){const [families,current]=await Promise.all([api("/families"),api("/families/current-user")]);state.families=families;state.currentUserId=current.user_id;renderFamilySelector();renderFamiliesList();renderNoteFamilies();renderNoteFamilyFilter()}
function renderNoteFamilyFilter(){
 const select=$("note-family-filter");if(!select)return;
 const selected=select.value;
 select.innerHTML='<option value="">All notes</option><option value="private">Private notes</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");
 if([...select.options].some(option=>option.value===selected))select.value=selected;
}
let notesPage=1;
const NOTES_PAGE_SIZE=10;
function filteredNotes(){
 const query=$("note-search")?.value.trim().toLocaleLowerCase()||"";
 const familyFilter=$("note-family-filter")?.value||"";
 return state.notes.filter(note=>(!query||String(note.title||"").toLocaleLowerCase().includes(query))&&(!familyFilter||(familyFilter==="private"?!note.family_id:note.family_id===familyFilter))).sort((a,b)=>new Date(b.updated_at||b.created_at||0)-new Date(a.updated_at||a.created_at||0));
}
function noteDate(note){
 const value=note.updated_at||note.created_at;
 return value?new Date(value).toLocaleDateString(): "—";
}
function renderNotes(){
 const target=$("notes-list");if(!target)return;
 if(!$("notes-overview")){
  const query=$("note-search")?.value.trim().toLocaleLowerCase()||"";
  const familyFilter=$("note-family-filter")?.value||"";
  const notes=state.notes.filter(note=>(!query||String(note.title||"").toLocaleLowerCase().includes(query))&&(!familyFilter||(familyFilter==="private"?!note.family_id:note.family_id===familyFilter)));
  target.innerHTML=notes.length?notes.map(note=>'<button type="button" class="family-list-item'+(note.id===state.selectedNote?" active":"")+'" data-note-id="'+escapeHtml(note.id)+'"><strong>'+escapeHtml(note.title)+'</strong><span>'+(note.family_id?"Shared with "+escapeHtml(state.families.find(f=>f.id===note.family_id)?.name||"a family"):"Private")+' · '+escapeHtml(new Date(note.updated_at).toLocaleDateString())+'</span></button>').join(""):'<div class="empty">'+(state.notes.length?'No notes match these filters. <button type="button" class="secondary" data-clear-note-filters>Clear filters</button>':'No notes yet. Create one to get started.')+'</div>';
  return;
 }
 const notes=filteredNotes();
 target.innerHTML=notes.length?notes.map(note=>'<button type="button" class="family-list-item'+(note.id===state.selectedNote?" active":"")+'" data-note-id="'+escapeHtml(note.id)+'"><strong>'+escapeHtml(note.title)+'</strong><span>'+(note.family_id?"Shared with "+escapeHtml(state.families.find(f=>f.id===note.family_id)?.name||"a family"):"Private")+' · '+escapeHtml(noteDate(note))+'</span></button>').join(""):'<div class="empty">'+(state.notes.length?'No notes match these filters. <button type="button" class="secondary" data-clear-note-filters>Clear filters</button>':'No notes yet. Create one to get started.')+'</div>';
 renderNotesTable();
}
function renderNotesTable(){
 const body=$("notes-table-body");if(!body)return;
 const notes=filteredNotes();
 body.innerHTML=notes.length?notes.map(note=>{
  const family=note.family_id?state.families.find(f=>f.id===note.family_id)?.name||"Family":null;
  const sharing=note.family_id?"Shared note · "+family:"Private note";
  return '<button type="button" class="item-card note-item-card" data-note-id="'+escapeHtml(note.id)+'" aria-label="Open '+escapeHtml(note.title||"Untitled note")+'"><div class="item-card-heading"><strong>'+escapeHtml(note.title||"Untitled note")+'</strong></div><div class="item-card-meta"><span>'+escapeHtml(sharing)+'</span><span>Updated · '+escapeHtml(noteDate(note))+'</span></div><span class="item-card-arrow" aria-hidden="true">›</span></button>';
 }).join(""):'<div class="tasks-table-empty"><strong>'+(state.notes.length?"No notes found":"No notes yet")+'</strong><span>'+(state.notes.length?"Try changing your search or filters.":"Create a note to keep something worth remembering.")+'</span></div>';
 const info=$("notes-page-info");if(info)info.textContent=notes.length?notes.length+" notes":"0 notes";
}
function showNotesOverview(){
 document.body.dataset.noteView="list";
 $("notes-overview").hidden=false;$("notes-editor-panel").hidden=true;
 if($("open-notes"))$("open-notes").hidden=false;
 if($("back-to-notes"))$("back-to-notes").hidden=true;
 $("notes-main-title").textContent="All notes";
 $("notes-main-subtitle").textContent="Your notes, ordered by most recently updated";
}
function showNoteEditor(title){
 document.body.dataset.noteView="detail";
 $("notes-overview").hidden=true;$("notes-editor-panel").hidden=false;
 if($("open-notes"))$("open-notes").hidden=true;
 if($("back-to-notes"))$("back-to-notes").hidden=false;
 $("notes-main-title").textContent=title;
 $("notes-main-subtitle").textContent="View and edit note details";
}
function resetNoteEditor(){
 state.selectedNote=null;renderNotes();
 if($("notes-overview"))showNotesOverview();
 if($("note-heading"))$("note-heading").textContent="Select a note";
 $("note-title").value="";$("note-content").value="";$("note-family").value="";
 for(const id of ["note-title","note-content","note-family","save-note"])$(id).disabled=true;
 $("delete-note").disabled=true;$("notes-status").textContent="Choose a note or create a new one. Notes are private by default.";
}
function startNewNote(){
 state.selectedNote="new";renderNotes();
 if($("notes-overview"))showNoteEditor("New note");
 if($("note-heading"))$("note-heading").textContent="New note";
 $("note-title").value="";$("note-content").value="";$("note-family").value="";
 for(const id of ["note-title","note-content","note-family","save-note"])$(id).disabled=false;
 $("delete-note").disabled=true;$("notes-status").textContent="New private note. You can optionally share it with one family.";
 $("note-title").focus();
}
async function selectNote(id){
 const note=state.notes.find(item=>item.id===id);if(!note)return;
 try{
  const full=await api("/notes/"+encodeURIComponent(id));state.selectedNote=id;renderNotes();
  if($("notes-overview"))showNoteEditor(full.title);
  if($("note-heading"))$("note-heading").textContent=full.title;
  $("note-title").value=full.title;$("note-content").value=full.content;$("note-family").value=full.family_id||"";
  const editable=full.owner_id===state.currentUserId;
  for(const field of ["note-title","note-content","note-family","save-note"])$(field).disabled=!editable;
  $("delete-note").disabled=!editable;
  $("notes-status").textContent=!editable?"Shared note · read-only":(full.family_id?"Shared with "+(state.families.find(f=>f.id===full.family_id)?.name||"a family"):"Private note");
 }catch(e){$("notes-status").textContent=e.message;if($("notes-main-subtitle"))$("notes-main-subtitle").textContent="Unable to open note: "+e.message}
}
function renderNoteFamilies(){
 const select=$("note-family");if(!select)return;const selected=select.value;
 select.innerHTML='<option value="">Private — only me</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");
 if([...select.options].some(option=>option.value===selected))select.value=selected;
}
async function loadNotes(){
 const button=$("refresh-notes");if(button)button.disabled=true;
 $("notes-list").innerHTML='<div class="empty" role="status">Loading notes…</div>';
 try{state.notes=await api("/notes");notesPage=1;renderNotes();if(state.selectedNote&&!state.notes.some(n=>n.id===state.selectedNote))resetNoteEditor()}
 catch(e){const error='<div class="empty" role="alert">Unable to load notes: '+escapeHtml(e.message)+'<p><button type="button" class="secondary" data-retry-notes>Try again</button></p></div>';$("notes-list").innerHTML=error;if($("notes-table-body"))$("notes-table-body").innerHTML='<tr><td colspan="3">'+error+'</td></tr>'}
 finally{if(button)button.disabled=false}
}
async function saveNote(event){
 event.preventDefault();const title=$("note-title").value.trim(),content=$("note-content").value,family_id=$("note-family").value||null;
 if(!title){$("notes-status").textContent="A title is required.";return}
 const body=JSON.stringify({title,content,family_id});$("save-note").disabled=true;
 try{
  const saved=state.selectedNote==="new"?await api("/notes",{method:"POST",body}):await api("/notes/"+encodeURIComponent(state.selectedNote),{method:"PUT",body});
  await loadNotes();state.selectedNote=saved.id;renderNotes();await selectNote(saved.id);$("notes-status").textContent="Note saved.";
 }catch(e){$("notes-status").textContent=e.message}
 finally{$("save-note").disabled=false}
}
async function deleteNote(){
 if(!state.selectedNote||state.selectedNote==="new")return;
 if(!window.confirm("Delete this note permanently?"))return;
 try{await api("/notes/"+encodeURIComponent(state.selectedNote),{method:"DELETE"});await loadNotes();resetNoteEditor();$("notes-status").textContent="Note deleted."}
 catch(e){$("notes-status").textContent=e.message}
}

function renderFamilySelector(){const selector=$("family-selector");if(!selector)return;const selected=state.selectedFamilyId||"";selector.innerHTML='<option value="">No family</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");selector.value=state.families.some(f=>f.id===selected)?selected:"";selector.disabled=!state.selected||!!state.runId;state.selectedFamilyId=selector.value||null}
function renderFamiliesList(){
 const target=$("families-list");if(!target)return;
 const query=$("family-search")?.value.trim().toLocaleLowerCase()||"";
 const families=state.families.filter(family=>family.name.toLocaleLowerCase().includes(query));
 if(!families.length){target.innerHTML='<div class="empty">'+(query?'No families match this filter.':'You do not belong to any families yet.')+'</div>';return}
 target.innerHTML=families.map(f=>{
  const active=f.id===state.managedFamilyId;
  const count=Number.isInteger(f.member_count)?f.member_count:(state.familyDetails?.family?.id===f.id?state.familyDetails.members?.length:null);
  const role=state.familyDetails?.family?.id===f.id?currentFamilyMember()?.role:null;
  return '<button type="button" class="family-list-item'+(active?' active':'')+'" data-family-id="'+escapeHtml(f.id)+'" aria-pressed="'+String(active)+'"><strong>'+escapeHtml(f.name)+'</strong><span>'+(count===null?'Open to manage':count+' members')+(role?' · '+escapeHtml(role):'')+'</span></button>';
 }).join("");
}
function memberAge(birthday){if(!birthday)return null;const [year,month,day]=birthday.split("-").map(Number),today=new Date();let age=today.getUTCFullYear()-year;if(today.getUTCMonth()+1<month||(today.getUTCMonth()+1===month&&today.getUTCDate()<day))age--;return age>=0?age:null}
function renderFamilyMembers(){
 const target=$("family-members"),details=state.familyDetails,filter=$("family-members-search");
 const canManage=!!details&&["owner","admin"].includes(currentFamilyMember()?.role.toLowerCase());
 $("rename-family").disabled=!canManage;$("add-family-member").disabled=!canManage;
 if(filter)filter.disabled=!details;
 if(!details){target.textContent="Select a family to see its members.";renderFamilyLifecycle();return}
 const allMembers=details.members||[],query=filter?.value.trim().toLocaleLowerCase()||"";
 const members=allMembers.filter(member=>[member.display_name,member.user_id,member.relationship,member.role,member.birthday].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 target.innerHTML='<div style="font-weight:600;margin:8px 0">'+escapeHtml(details.family.name)+' · '+allMembers.length+' members</div>'+ (members.length?members.map(m=>{
  const role=String(m.role).toLowerCase(),actorRole=currentFamilyMember()?.role.toLowerCase();
  const isSelf=m.user_id===state.currentUserId,isOwner=actorRole==="owner",isAdmin=actorRole==="admin";
  const roleControl=!isSelf&&isOwner?'<label>Role <select data-member-role="'+escapeHtml(m.user_id)+'" style="width:auto;min-width:90px;margin:4px 4px 4px 0;padding:5px"><option value="member" '+(role==="member"?"selected":"")+'>Member</option><option value="admin" '+(role==="admin"?"selected":"")+'>Admin</option></select></label><button type="button" class="secondary" data-save-role="'+escapeHtml(m.user_id)+'">Save role</button>':(!isSelf&&isAdmin&&role==="member"?'<button type="button" class="secondary" data-promote-member="'+escapeHtml(m.user_id)+'">Promote to admin</button>':'');
  const transfer=isOwner&&!isSelf?'<button type="button" class="secondary" data-transfer-owner="'+escapeHtml(m.user_id)+'">Make owner</button>':'';
  const remove=!isSelf&&(isOwner||(isAdmin&&role==="member"))?'<button type="button" class="danger" data-remove-member="'+escapeHtml(m.user_id)+'">Remove</button>':'';
  const birthday=m.birthday?new Date(m.birthday+"T00:00:00Z").toLocaleDateString():"Birthday not set";
  const age=memberAge(m.birthday);
  const editProfile=actorRole==="owner"||actorRole==="admin"?'<button type="button" class="secondary" data-edit-member-profile="'+escapeHtml(m.user_id)+'">Edit details</button>':"";
  const actions=roleControl||transfer||remove||editProfile?'<div class="member-actions">'+roleControl+transfer+remove+editProfile+'</div>':'';
  return '<div class="family-member"><strong>'+escapeHtml(m.display_name||m.user_id)+'</strong><span>'+escapeHtml(m.relationship)+' · '+escapeHtml(m.role)+'</span><span>'+escapeHtml(birthday)+(age===null?'':' · '+age+' years old')+'</span>'+actions+'</div>';
 }).join(""):'<div class="hint">'+(allMembers.length?'No members match this filter. <button type="button" class="secondary" data-clear-family-member-filter>Clear filter</button>':'No members.')+'</div>');
}
function currentFamilyMember(){
 return (state.familyDetails?.members||[]).find(member=>member.user_id===state.currentUserId)||null;
}
function renderFamilyLifecycle(){
 const family=state.familyDetails?.family,me=currentFamilyMember();
 const isOwner=me?.role.toLowerCase()==="owner";
 $("leave-family").disabled=!family||!me||isOwner;
 $("delete-family").disabled=!family||!isOwner;
 $("family-lifecycle-hint").textContent=!family?"Select a family to manage its membership.":!me?"Your account is not an active member of this family.":isOwner?"Transfer ownership to another member before leaving. Only the owner can delete this family.":"You can leave this family. This removes only your membership.";
}

function invitationState(invitation){
 if(invitation.revoked_at)return "Revoked";
 if(invitation.accepted_at)return "Accepted";
 if(new Date(invitation.expires_at)<=new Date())return "Expired";
 return "Pending";
}
function renderFamilyInvitations(){
 const target=$("family-invitations"),family=state.familyDetails?.family,me=currentFamilyMember(),filter=$("family-invitations-search");
 const canManage=me&&["owner","admin"].includes(me.role.toLowerCase());
 $("create-invitation").disabled=!family||!canManage;
 if(filter)filter.disabled=!family||!canManage;
 if(!family){target.textContent="Select a family to manage invitations.";return}
 if(!canManage){target.textContent="Only family owners and admins can manage invitations.";return}
 const query=filter?.value.trim().toLocaleLowerCase()||"",allItems=state.familyInvitations;
 const items=allItems.filter(inv=>[invitationState(inv),inv.target_identity?.provider,inv.target_identity?.subject,inv.expires_at].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 target.innerHTML=items.length?items.map(inv=>{
  const status=invitationState(inv);
  return '<div class="family-invitation"><div><strong>'+escapeHtml(status)+'</strong><small>Expires '+escapeHtml(new Date(inv.expires_at).toLocaleString())+(inv.target_identity?' · '+escapeHtml(inv.target_identity.provider)+': '+escapeHtml(inv.target_identity.subject):'')+'</small></div><div class="family-invitation-actions">'+(status==="Pending"?'<button type="button" class="secondary" data-revoke-invitation="'+escapeHtml(inv.id)+'">Revoke</button>':'')+'</div></div>';
 }).join(""):'<div class="hint" style="padding:8px 0">'+(allItems.length?'No invitations match this filter. <button type="button" class="secondary" data-clear-family-invitation-filter>Clear filter</button>':'No invitations yet.')+'</div>';
}
function openFamilyActionDialog({title,description,fields,submitLabel="Save",handler}){
 const dialog=$("family-action-dialog");
 if(!dialog){
  const values={};
  for(const field of fields){
   const choices=field.type==="select"?" ("+field.options.map(option=>option.value).join(", ")+")":"";
   const value=window.prompt(field.label+choices,field.value??"");
   if(value===null)return;
   values[field.name]=value;
  }
  Promise.resolve(handler(values)).catch(error=>setStatus(error.message,"error"));
  return;
 }
 familyActionHandler=handler;
 $("family-action-title").textContent=title;
 $("family-action-description").textContent=description||"";
 $("family-action-fields").innerHTML=fields.map(field=>{
  const id="family-action-"+field.name,required=field.required?" required":"",placeholder=field.placeholder?' placeholder="'+escapeHtml(field.placeholder)+'"':"",limits=(field.maxLength?' maxlength="'+field.maxLength+'"':"")+(field.min!==undefined?' min="'+field.min+'"':"")+(field.max!==undefined?' max="'+field.max+'"':"");
  const control=field.type==="select"
   ?'<select id="'+id+'" name="'+field.name+'"'+required+'>'+field.options.map(option=>'<option value="'+escapeHtml(option.value)+'" '+(String(option.value)===String(field.value??"")?"selected":"")+'>'+escapeHtml(option.label)+'</option>').join("")+'</select>'
   :'<input id="'+id+'" name="'+field.name+'" type="'+(field.type||"text")+'" value="'+escapeHtml(field.value??"")+'"'+required+placeholder+limits+'>';
  return '<label class="family-action-field" for="'+id+'">'+escapeHtml(field.label)+control+'</label>';
 }).join("");
 $("family-action-status").textContent="";
 $("family-action-submit").textContent=submitLabel;
 $("family-action-dialog").showModal();
 $("family-action-fields").querySelector("input,select")?.focus();
}
async function addManagedFamilyMember(){
 const family=state.familyDetails?.family;if(!family)return;
 openFamilyActionDialog({title:"Add family member",description:"Add a person to this family. Birthday is optional.",submitLabel:"Add member",fields:[
  {name:"name",label:"Name",required:true,maxLength:80},
  {name:"relationship",label:"Relationship",type:"select",value:"other",options:["father","mother","son","daughter","grandfather","grandmother","partner","other"].map(value=>({value,label:value[0].toUpperCase()+value.slice(1)}))},
  {name:"birthday",label:"Birthday (optional)",type:"date"}
 ],handler:async values=>{
  await api("/families/"+encodeURIComponent(family.id)+"/members",{method:"POST",body:JSON.stringify({name:values.name.trim(),relationship:values.relationship,birthday:values.birthday||null})});
  await loadFamilyDetails(family.id);await loadFamilies();setStatus("Family member added","ok");
 }});
}
async function editManagedFamilyMember(userId){
 const family=state.familyDetails?.family,member=state.familyDetails?.members?.find(item=>item.user_id===userId);if(!family||!member)return;
 openFamilyActionDialog({title:"Edit family member",description:"Update the member's display name and birthday.",submitLabel:"Save changes",fields:[
  {name:"name",label:"Name",required:true,maxLength:80,value:member.display_name||""},
  {name:"birthday",label:"Birthday (optional)",type:"date",value:member.birthday||""}
 ],handler:async values=>{
  await api("/families/"+encodeURIComponent(family.id)+"/members/"+encodeURIComponent(userId)+"/profile",{method:"PUT",body:JSON.stringify({name:values.name.trim(),birthday:values.birthday||null})});
  await loadFamilyDetails(family.id);setStatus("Member details updated","ok");
 }});
}
async function addManagedFamilyMember(){
 const family=state.familyDetails?.family;if(!family)return;
 const name=window.prompt("Member's name");if(name===null)return;if(!name.trim()){setStatus("Member name is required","error");return}
 const relationship=window.prompt("Relationship (father, mother, son, daughter, grandfather, grandmother, partner, other)","other");if(relationship===null)return;
 const birthday=window.prompt("Birthday (YYYY-MM-DD, optional)","");if(birthday===null)return;
 if(birthday.trim()&&!/^\d{4}-\d{2}-\d{2}$/.test(birthday.trim())){setStatus("Birthday must use YYYY-MM-DD","error");return}
 try{await api("/families/"+encodeURIComponent(family.id)+"/members",{method:"POST",body:JSON.stringify({name:name.trim(),relationship:relationship.trim().toLowerCase(),birthday:birthday.trim()||null})});await loadFamilyDetails(family.id);await loadFamilies();setStatus("Family member added","ok")}
 catch(e){setStatus(e.message,"error")}
}
async function editManagedFamilyMember(userId){
 const family=state.familyDetails?.family,member=state.familyDetails?.members?.find(item=>item.user_id===userId);if(!family||!member)return;
 const name=window.prompt("Member's name",member.display_name||"");if(name===null)return;if(!name.trim()){setStatus("Member name is required","error");return}
 const birthday=window.prompt("Birthday (YYYY-MM-DD, leave empty to clear)",member.birthday||"");if(birthday===null)return;
 if(birthday.trim()&&!/^\d{4}-\d{2}-\d{2}$/.test(birthday.trim())){setStatus("Birthday must use YYYY-MM-DD","error");return}
 try{await api("/families/"+encodeURIComponent(family.id)+"/members/"+encodeURIComponent(userId)+"/profile",{method:"PUT",body:JSON.stringify({name:name.trim(),birthday:birthday.trim()||null})});await loadFamilyDetails(family.id);setStatus("Member details updated","ok")}
 catch(e){setStatus(e.message,"error")}
}

async function loadFamilyInvitations(familyId){
 const member=currentFamilyMember();
 if(!familyId||!member||!["owner","admin"].includes(member.role.toLowerCase())){state.familyInvitations=[];renderFamilyInvitations();return}
 try{state.familyInvitations=await api("/families/"+encodeURIComponent(familyId)+"/invitations");renderFamilyInvitations()}
 catch(e){state.familyInvitations=[];renderFamilyInvitations();setStatus(e.message,"error")}
}
async function createInvitation(){
 const family=state.familyDetails?.family;if(!family)return;
 openFamilyActionDialog({title:"Create invitation",description:"Choose how long this invitation remains valid. Optionally restrict it to a Telegram user ID.",submitLabel:"Create invitation",fields:[
  {name:"days",label:"Validity (days)",type:"number",value:7,min:1,max:30,required:true},
  {name:"target",label:"Telegram user ID (optional)",placeholder:"Leave empty to allow anyone"}
 ],handler:async values=>{
  const validity=Number(values.days);
  if(!Number.isInteger(validity)||validity<1||validity>30)throw new Error("Choose a validity between 1 and 30 days.");
  const request={expires_at:new Date(Date.now()+validity*86400000).toISOString()};
  if(values.target.trim())request.target_identity={provider:"telegram",subject:values.target.trim()};
  const created=await api("/families/"+encodeURIComponent(family.id)+"/invitations",{method:"POST",body:JSON.stringify(request)});
  if(created.lan_url){
   const copy=window.confirm("Invitation created. Copy this LAN invitation link now?\n\n"+created.lan_url);
   if(copy){try{await navigator.clipboard.writeText(created.lan_url);setStatus("LAN invitation link copied","ok")}catch{window.prompt("Copy the LAN invitation link",created.lan_url)}}
  }else{
   window.prompt("Invitation created. Save this one-time token; it will not be shown again.",created.token);
   setStatus("Invitation created; LAN_BASE_URL is not configured, so no LAN link is available","ok");
  }
  await loadFamilyInvitations(family.id);
 }});
}
async function revokeInvitation(invitationId){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Revoke this invitation? The link will stop working."))return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/invitations/"+encodeURIComponent(invitationId),{method:"DELETE"});await loadFamilyInvitations(family.id);setStatus("Invitation revoked","ok")}
 catch(e){setStatus(e.message,"error")}
}
async function changeMemberRole(userId,roleOverride=null){
 const family=state.familyDetails?.family;if(!family)return;
 const select=document.querySelector('[data-member-role="'+CSS.escape(userId)+'"]');if(!select&&!roleOverride)return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/members/"+encodeURIComponent(userId)+"/role",{method:"PUT",body:JSON.stringify({role:roleOverride||select.value})});await loadFamilyDetails(family.id);setStatus("Member role updated","ok")}
 catch(e){setStatus(e.message,"error")}
}
async function removeFamilyMember(userId){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Remove this member from "+family.name+"? Their EME user account will not be deleted."))return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/members/"+encodeURIComponent(userId),{method:"DELETE"});await loadFamilyDetails(family.id);setStatus("Member removed","ok")}
 catch(e){setStatus(e.message,"error")}
}
async function transferFamilyOwnership(userId){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Transfer ownership of "+family.name+" to "+userId+"? You will become an admin."))return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/transfer-ownership",{method:"POST",body:JSON.stringify({new_owner_user_id:userId})});await loadFamilyDetails(family.id);setStatus("Family ownership transferred","ok")}
 catch(e){setStatus(e.message,"error")}
}
async function leaveFamily(){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Leave "+family.name+"? You will lose access to this family's context and invitations."))return;
 try{
  if(state.selected&&state.selectedFamilyId===family.id){
   await api("/conversations/"+encodeURIComponent(state.selected)+"/family",{method:"PUT",body:JSON.stringify({family_id:null})});
   state.selectedFamilyId=null;renderFamilySelector();
  }
  await api("/families/"+encodeURIComponent(family.id)+"/leave",{method:"POST"});
  state.familyDetails=null;state.familyInvitations=[];state.managedFamilyId=null;await loadFamilies();await loadFamilyDetails(null);
  setStatus("You left the family","ok");
 }catch(e){setStatus(e.message,"error")}
}
async function deleteFamily(){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Permanently delete "+family.name+"? This removes all memberships and invitations and cannot be undone."))return;
 const confirmation=window.prompt("Type the family name to confirm deletion:",family.name);
 if(confirmation!==family.name)return;
 try{
  if(state.selected&&state.selectedFamilyId===family.id){
   await api("/conversations/"+encodeURIComponent(state.selected)+"/family",{method:"PUT",body:JSON.stringify({family_id:null})});
   state.selectedFamilyId=null;renderFamilySelector();
  }
  await api("/families/"+encodeURIComponent(family.id),{method:"DELETE"});
  state.familyDetails=null;state.familyInvitations=[];state.managedFamilyId=null;await loadFamilies();await loadFamilyDetails(null);
  setStatus("Family deleted","ok");
 }catch(e){setStatus(e.message,"error")}
}
async function loadFamilyDetails(familyId){
 state.managedFamilyId=familyId||null;
 setProfileView(familyId?"detail":"list");
 $("family-empty").hidden=!!familyId;$("family-management").hidden=!familyId;
 if(!familyId){state.familyDetails=null;state.familyInvitations=[];$("managed-family-name").textContent="Select a family";$("managed-family-meta").textContent="Choose a family to manage its members and invitations.";renderFamiliesList();renderFamilyMembers();renderFamilyInvitations();renderFamilyLifecycle();return}
 const family=state.families.find(item=>item.id===familyId);
 $("managed-family-name").textContent=family?.name||"Family";
 $("managed-family-meta").textContent="Loading family details…";
 renderFamiliesList();
 try{state.familyDetails=await api("/families/"+encodeURIComponent(familyId));$("managed-family-name").textContent=state.familyDetails.family.name;$("managed-family-meta").textContent="Family ID · "+familyId;renderFamilyMembers();renderFamilyLifecycle();await loadFamilyInvitations(familyId)}
 catch(e){state.familyDetails=null;$("managed-family-meta").textContent=e.message;renderFamilyMembers();renderFamilyLifecycle();setStatus(e.message,"error")}
}
async function createFamily(){
 openFamilyActionDialog({title:"Create family",description:"Create a family and choose your relationship to it.",submitLabel:"Create family",fields:[
  {name:"name",label:"Family name",required:true,maxLength:80},
  {name:"relationship",label:"Your relationship",type:"select",value:"other",options:["father","mother","son","daughter","grandfather","grandmother","partner","other"].map(value=>({value,label:value[0].toUpperCase()+value.slice(1)}))}
 ],handler:async values=>{
  const created=await api("/families",{method:"POST",body:JSON.stringify({name:values.name.trim(),relationship:values.relationship})});
  await loadFamilies();await loadFamilyDetails(created.id);setStatus("Family created","ok");
 }});
}
async function renameFamily(){
 if(!state.familyDetails)return;
 const current=state.familyDetails.family;
 const name=window.prompt("Family name",current.name);if(name===null)return;
 if(!name.trim()){setStatus("Family name must not be empty","error");return}
 try{await api("/families/"+encodeURIComponent(current.id),{method:"PUT",body:JSON.stringify({name:name.trim()})});await loadFamilies();await loadFamilyDetails(current.id);setStatus("Family renamed","ok")}
 catch(e){setStatus(e.message,"error")}
}

async function selectConversation(id){if($('conversation-detail-view'))showConversationDetail();closeRunStream();state.runId=null;state.runEvents=[];state.selected=id;state.retry=null;state.messages=[];renderRunEvents([]);try{localStorage.setItem("eme.selectedConversation",id)}catch{}renderConversations();$("messages").innerHTML='<div class="empty">Loading conversation…</div>';try{const [c,runs]=await Promise.all([api("/conversations/"+encodeURIComponent(id)),api("/conversations/"+encodeURIComponent(id)+"/runs")]);if(state.selected!==id)return;$("conversation-title").textContent=c.title||"New conversation";$("conversation-meta").textContent="Agent · "+c.agent+" · Conversation "+c.id+" · ~"+Number(c.history_tokens_estimate||0).toLocaleString()+" history tokens (estimated)";$("edit-conversation-settings").disabled=false;if($("agent"))$("agent").value=c.agent;updateAgentDescription();state.selectedFamilyId=c.selected_family_id||null;renderFamilySelector();state.messageCount=(c.messages||[]).length;state.messages=c.messages||[];const activeRun=runs.find(run=>["queued","running"].includes(run.status));if(activeRun){state.runId=activeRun.run_id;state.activeRuns[id]=activeRun;const alreadyShown=state.messages.some(message=>{const part=messageParts(message);return part?.kind==="user"&&part.value.content===activeRun.input});if(!alreadyShown)state.messages=[...state.messages,{user:{content:activeRun.input}}];renderMessages(state.messages,{label:activeRun.status==="queued"?"Queued…":"Working in the background…",cancelEnabled:true});setStatus("Running · "+activeRun.status)}else{delete state.activeRuns[id];renderMessages(state.messages);setStatus("")}renderConversations();setComposerEnabled(!activeRun);if(activeRun)connectRunStream(id,activeRun.run_id,state.messageCount,activeRun.input)}catch(e){if(state.selected===id){setStatus(e.message,"error");setComposerEnabled(true)}}}
function openNewConversationDialog(){const dialog=$("new-conversation-dialog");if(!dialog)return;const name=$("new-conversation-name"),family=$("new-conversation-family"),agent=$("new-conversation-agent");name.value="New conversation";family.innerHTML='<option value="">No family</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");family.value=state.families.some(f=>f.id===state.selectedFamilyId)?state.selectedFamilyId:"";const preferred=$("agent")?.value||agent.value;if(state.agents.some(a=>a.name===preferred))agent.value=preferred;updateAgentDescription();$("new-conversation-status").textContent="";$("new-conversation-submit").disabled=!state.agents.length;dialog.showModal();name.focus();name.select()}
async function createConversation(event){event.preventDefault();const name=$("new-conversation-name").value.trim(),agent=$("new-conversation-agent").value,familyId=$("new-conversation-family").value||null,status=$("new-conversation-status"),button=$("new-conversation-submit");if(!name){status.textContent="Conversation name must not be empty.";return}if(!agent){status.textContent="Choose an agent.";return}button.disabled=true;status.textContent="Creating conversation…";try{const c=await api("/conversations",{method:"POST",body:JSON.stringify({agent,family_id:familyId})});const updated=await api("/conversations/"+encodeURIComponent(c.id),{method:"PUT",body:JSON.stringify({title:name})});state.showArchived=false;state.selectedFamilyId=familyId;$("new-conversation-dialog").close();await loadConversations();await selectConversation(updated.id||c.id);$("prompt").focus()}catch(e){status.textContent=e.message;setStatus(e.message,"error")}finally{button.disabled=false}}
function openEditConversationDialog(id=state.selected){
 if(!id)return;
 const conversation=state.conversations.find(c=>c.id===id);
 if(!conversation||state.activeRuns[id]||(state.selected===id&&state.runId))return;
 state.editingConversationId=id;
 const dialog=$("edit-conversation-dialog"),name=$("edit-conversation-name"),family=$("edit-conversation-family");
 name.value=conversation.title||"New conversation";
 family.innerHTML='<option value="">No family</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");
 family.value=conversation.selected_family_id||"";
 $("edit-conversation-status").textContent="";
 $("edit-conversation-save").disabled=false;
 dialog.showModal();name.focus();name.select();
}
async function saveConversationSettings(event){
 event.preventDefault();
 const id=state.editingConversationId;
 if(!id||state.activeRuns[id]||(state.selected===id&&state.runId))return;
 const name=$("edit-conversation-name").value.trim(),familyId=$("edit-conversation-family").value||null;
 const status=$("edit-conversation-status"),button=$("edit-conversation-save");
 if(!name){status.textContent="Conversation name must not be empty.";return}
 const previous=state.conversations.find(c=>c.id===id);
 if(!previous)return;
 button.disabled=true;status.textContent="Saving changes…";
 try{
  let updated=previous;
  if(name!==(previous.title||"New conversation"))updated=await api("/conversations/"+encodeURIComponent(id),{method:"PUT",body:JSON.stringify({title:name})});
  if(familyId!==(previous.selected_family_id||null)){
   const familyUpdated=await api("/conversations/"+encodeURIComponent(id)+"/family",{method:"PUT",body:JSON.stringify({family_id:familyId})});
   updated={...updated,...familyUpdated};
  }
  const selectedFamilyId=updated.selected_family_id??familyId;
  state.conversations=state.conversations.map(c=>c.id===id?{...c,...updated,title:updated.title||name,selected_family_id:selectedFamilyId}:c);
  if(state.selected===id){
   state.selectedFamilyId=selectedFamilyId;
   $("conversation-title").textContent=updated.title||name;
   $("conversation-meta").textContent="Agent · "+(updated.agent||previous.agent)+" · Conversation "+id+" · ~"+Number(updated.history_tokens_estimate??previous.history_tokens_estimate??0).toLocaleString()+" history tokens (estimated)";
  }
  renderConversations();
  $("edit-conversation-dialog").close();
  setStatus("Conversation settings saved","ok");
 }catch(error){
  status.textContent=error.message;
  setStatus(error.message,"error");
  try{
   const latest=await api("/conversations/"+encodeURIComponent(id));
   state.conversations=state.conversations.map(c=>c.id===id?{...c,...latest}:c);
   if(state.selected===id){
    state.selectedFamilyId=latest.selected_family_id||null;
    $("conversation-title").textContent=latest.title||"New conversation";
    renderFamilySelector();
   }
   renderConversations();
  }catch{}
 }finally{button.disabled=false}
}
function clearSelectedConversation(){if($('conversations-overview'))showConversationsOverview();closeRunStream();state.runId=null;state.runEvents=[];state.selected=null;try{localStorage.removeItem("eme.selectedConversation")}catch{}state.selectedFamilyId=null;renderFamilySelector();state.messageCount=0;state.messages=[];$("conversation-title").textContent="No conversation selected";$("conversation-meta").textContent="Create a conversation to get started";$("edit-conversation-settings").disabled=true;$("messages").innerHTML='<div class="empty">Choose an existing conversation or create a new one.</div>';$("prompt").value="";setComposerEnabled(false);renderConversations()}
async function archiveConversation(id){
 const conversation=state.conversations.find(c=>c.id===id);if(!conversation)return;
 const restoring=state.showArchived;
 try{
  await api("/conversations/"+encodeURIComponent(id)+(restoring?"/unarchive":"/archive"),{method:"PUT"});
  if(state.selected===id&&!restoring)clearSelectedConversation();
  await loadConversations();
  setStatus(restoring?"Conversation restored":"Conversation archived","ok");
 }catch(e){setStatus(e.message,"error")}
}
async function deleteConversation(id){const conversation=state.conversations.find(c=>c.id===id);if(!conversation)return;const title=conversation.title||"New conversation";if(!window.confirm('Permanently delete "'+title+'"? This cannot be undone.'))return;try{await api("/conversations/"+encodeURIComponent(id),{method:"DELETE"});state.conversations=state.conversations.filter(c=>c.id!==id);if(state.selected===id)clearSelectedConversation();else renderConversations();setStatus("Conversation deleted","ok")}catch(e){setStatus(e.message,"error")}}
async function sendMessage(){const input=$("prompt").value.trim();if(!input||!state.selected||state.runId)return;const id=state.selected,previousMessageCount=state.messageCount,previousMessages=[...state.messages];state.retry=null;$("prompt").value="";state.messages=[...state.messages,{user:{content:input}}];renderMessages(state.messages,{label:"Sending…",cancelEnabled:false});setComposerEnabled(false);setStatus("Queued…");try{const result=await api("/conversations/"+encodeURIComponent(id)+"/runs",{method:"POST",body:JSON.stringify({input})});state.activeRuns[id]={run_id:result.run_id,conversation_id:id,input,status:result.status};await loadConversations();if(state.selected!==id)return;state.runId=result.run_id;renderMessages(state.messages,{label:"Thinking…",cancelEnabled:true});setStatus("Running · "+result.run_id.slice(0,8));connectRunStream(id,result.run_id,previousMessageCount,input)}catch(e){if(state.selected===id){state.messages=previousMessages;renderMessages(state.messages);setStatus(e.message,"error");setComposerEnabled(true);$("prompt").value=input}}}
async function retryMessage(){const retry=state.retry;if(!retry||!state.selected||state.runId)return;const id=state.selected,previousMessageCount=state.messageCount;state.retry=null;renderMessages(state.messages,{label:"Retrying…",cancelEnabled:false});setComposerEnabled(false);setStatus("Retrying…");try{const result=await api("/conversations/"+encodeURIComponent(id)+"/runs",{method:"POST",body:JSON.stringify({input:retry.input,retry:true})});state.activeRuns[id]={run_id:result.run_id,conversation_id:id,input:retry.input,status:result.status};await loadConversations();if(state.selected!==id)return;state.runId=result.run_id;renderMessages(state.messages,{label:"Thinking…",cancelEnabled:true});setStatus("Running · "+result.run_id.slice(0,8));connectRunStream(id,result.run_id,previousMessageCount,retry.input)}catch(error){if(state.selected===id){state.retry=retry;renderMessages(state.messages,null,state.retry);setStatus(error.message,"error");setComposerEnabled(true)}}}
function closeRunStream(){
 if(state.eventSource){state.eventSource.close();state.eventSource=null}
}
function connectRunStream(conversationId,runId,previousMessageCount,input){
 closeRunStream();
 if(state.selected!==conversationId||state.runId!==runId)return;
 state.runEvents=[];renderRunEvents([]);
 const source=new EventSource("/runs/"+encodeURIComponent(runId)+"/events/stream");
 state.eventSource=source;
 source.onmessage=async event=>{
  if(state.selected!==conversationId||state.runId!==runId){source.close();return}
  let update;
  try{update=JSON.parse(event.data)}catch(error){console.error("Invalid run event",error);return}
  if(!state.runEvents.some(item=>item.sequence===update.sequence)){
   state.runEvents.push(update);
   state.runEvents.sort((left,right)=>left.sequence-right.sequence);
   renderRunEvents(state.runEvents);
  }
  if(!TERMINAL.has(update.kind))return;
  source.close();
  if(state.eventSource===source)state.eventSource=null;
  try{
   const run=await api("/runs/"+encodeURIComponent(runId));
   state.runId=null;delete state.activeRuns[conversationId];
   let conversation=null;
   for(let attempt=0;attempt<40;attempt++){
    conversation=await api("/conversations/"+encodeURIComponent(conversationId));
    if((conversation.messages||[]).length>previousMessageCount)break;
    await new Promise(resolve=>setTimeout(resolve,100));
   }
   if(state.selected===conversationId){
    $("conversation-title").textContent=conversation.title||"New conversation";
    $("conversation-meta").textContent="Agent · "+conversation.agent+" · Conversation "+conversation.id+" · ~"+Number(conversation.history_tokens_estimate||0).toLocaleString()+" history tokens (estimated)";
    state.messageCount=(conversation.messages||[]).length;state.messages=conversation.messages||[];state.retry=null;
    if(run.status!=="completed"&&run.status!=="cancelled"){
     for(let i=state.messages.length-1;i>=0;i--){
      const part=messageParts(state.messages[i]);
      if(part?.kind==="user"&&part.value.content===input){state.retry={input,messageIndex:i};break}
     }
    }
    renderMessages(state.messages,null,state.retry);setComposerEnabled(true);
    setStatus(run.status==="completed"?"Completed":run.error||run.status,run.status==="completed"?"ok":"error");
   }
   await loadConversations();renderFamilySelector();
  }catch(error){
   if(state.selected===conversationId){state.runId=null;delete state.activeRuns[conversationId];setStatus("Unable to retrieve completed run: "+error.message,"error");setComposerEnabled(true);renderFamilySelector()}
  }
 };
 source.onerror=()=>{
  if(state.selected===conversationId&&state.runId===runId){
   // EventSource reconnects automatically with Last-Event-ID.
   setStatus("Connection interrupted · reconnecting…");
  }
 };
 setStatus("Running · "+runId.slice(0,8));
}
on("families-list","click",e=>{const item=e.target.closest("button[data-family-id]");if(item)loadFamilyDetails(item.dataset.familyId)});on("family-search","input",renderFamiliesList);on("family-members-search","input",renderFamilyMembers);on("family-invitations-search","input",renderFamilyInvitations);on("task-search","input",()=>{tasksPage=1;renderTasks()});on("tasks-enabled-only","change",()=>{tasksPage=1;renderTasks()});on("note-search","input",()=>{notesPage=1;renderNotes()});on("note-family-filter","change",()=>{notesPage=1;renderNotes()});
function updateListFabVisibility(overview,button){if(!overview||!button)return;button.hidden=overview.scrollTop>8;}
for(const [overviewId,buttonId] of [["conversations-overview","new-conversation"],["tasks-overview","new-task"],["notes-overview","new-note"]]){
 const overview=$(overviewId),button=$(buttonId);
 if(overview&&button){overview.addEventListener("scroll",()=>updateListFabVisibility(overview,button),{passive:true});updateListFabVisibility(overview,button);}
}
on("new-task","click",startNewTask);on("back-to-tasks","click",resetTaskEditor);on("tasks-prev-page","click",()=>{tasksPage--;renderTasksTable()});on("tasks-next-page","click",()=>{tasksPage++;renderTasksTable()});on("tasks-list","click",e=>{if(e.target.closest("[data-retry-tasks]")){loadTasks();return}if(e.target.closest("[data-clear-task-filters]")){$("task-search").value="";$("tasks-enabled-only").checked=false;tasksPage=1;renderTasks();return}const button=e.target.closest("button[data-task-id]");if(button)selectTask(button.dataset.taskId)});on("tasks-table-body","click",e=>{const row=e.target.closest("[data-task-id]");if(row)selectTask(row.dataset.taskId)});on("tasks-table-body","keydown",e=>{const row=e.target.closest("[data-task-id]");if(row&&(e.key==="Enter"||e.key===" ")&&e.target===row){e.preventDefault();selectTask(row.dataset.taskId)}});on("task-form","submit",saveTask);on("delete-task","click",deleteTask);on("notes-list","click",e=>{if(e.target.closest("[data-retry-notes]")){loadNotes();return}if(e.target.closest("[data-clear-note-filters]")){$("note-search").value="";$("note-family-filter").value="";notesPage=1;renderNotes();return}const button=e.target.closest("button[data-note-id]");if(button)selectNote(button.dataset.noteId)});on("notes-table-body","click",e=>{const row=e.target.closest("[data-note-id]");if(row)selectNote(row.dataset.noteId)});on("notes-table-body","keydown",e=>{const row=e.target.closest("[data-note-id]");if(row&&(e.key==="Enter"||e.key===" ")&&e.target===row){e.preventDefault();selectNote(row.dataset.noteId)}});on("notes-prev-page","click",()=>{notesPage--;renderNotesTable()});on("notes-next-page","click",()=>{notesPage++;renderNotesTable()});on("new-note","click",startNewNote);on("back-to-notes","click",resetNoteEditor);on("note-form","submit",saveNote);on("delete-note","click",deleteNote);
on("create-family","click",createFamily);on("add-family-member","click",addManagedFamilyMember);on("family-action-cancel","click",()=> $("family-action-dialog").close());on("family-action-form","submit",async event=>{event.preventDefault();if(!familyActionHandler)return;const button=$("family-action-submit");button.disabled=true;$("family-action-cancel").disabled=true;$("family-action-status").textContent="Saving…";try{const values=Object.fromEntries(new FormData(event.currentTarget).entries());await familyActionHandler(values);$("family-action-dialog").close()}catch(error){$("family-action-status").textContent=error.message}finally{button.disabled=false;$("family-action-cancel").disabled=false}});on("family-action-dialog","close",()=>{familyActionHandler=null});on("leave-family","click",leaveFamily);on("delete-family","click",deleteFamily);on("family-members","click",e=>{if(e.target.closest("[data-clear-family-member-filter]")){$("family-members-search").value="";renderFamilyMembers();return}const role=e.target.closest("button[data-save-role]"),promote=e.target.closest("button[data-promote-member]"),remove=e.target.closest("button[data-remove-member]"),transfer=e.target.closest("button[data-transfer-owner]"),edit=e.target.closest("button[data-edit-member-profile]");if(edit)editManagedFamilyMember(edit.dataset.editMemberProfile);else if(role)changeMemberRole(role.dataset.saveRole);else if(promote)changeMemberRole(promote.dataset.promoteMember,"admin");else if(remove)removeFamilyMember(remove.dataset.removeMember);else if(transfer)transferFamilyOwnership(transfer.dataset.transferOwner)});on("rename-family","click",renameFamily);on("create-invitation","click",createInvitation);on("family-invitations","click",e=>{if(e.target.closest("[data-clear-family-invitation-filter]")){$("family-invitations-search").value="";renderFamilyInvitations();return}const button=e.target.closest("button[data-revoke-invitation]");if(button)revokeInvitation(button.dataset.revokeInvitation)});on("edit-conversation-settings","click",openEditConversationDialog);on("edit-conversation-form","submit",saveConversationSettings);on("edit-conversation-cancel","click",()=> $("edit-conversation-dialog").close());on("edit-conversation-dialog","click",event=>{if(event.target===$("edit-conversation-dialog"))$("edit-conversation-dialog").close()});on("messages","click",async e=>{if(e.target.closest("[data-retry-message]")){e.preventDefault();await retryMessage();return}if(!e.target.closest("#cancel-run")||!state.runId)return;const button=e.target.closest("#cancel-run");button.disabled=true;button.textContent="Cancelling…";try{await api("/runs/"+encodeURIComponent(state.runId)+"/cancel",{method:"POST"});setStatus("Cancellation requested")}catch(error){button.disabled=false;button.textContent="Cancel";setStatus(error.message,"error")}});on("new-conversation","click",openNewConversationDialog);on("new-conversation-form","submit",createConversation);on("new-conversation-agent","change",updateAgentDescription);on("new-conversation-close","click",()=>$("new-conversation-dialog").close());on("new-conversation-dialog","click",event=>{if(event.target===$("new-conversation-dialog"))$("new-conversation-dialog").close()});on("conversations","click",e=>{const archiveButton=e.target.closest("button[data-archive-id]");if(archiveButton){e.stopPropagation();if(!state.runId)archiveConversation(archiveButton.dataset.archiveId);return}const deleteButton=e.target.closest("button[data-delete-id]");if(deleteButton){e.stopPropagation();if(!state.runId)deleteConversation(deleteButton.dataset.deleteId);return}const item=e.target.closest("li[data-id]");if(item&&item.dataset.id!==state.selected)selectConversation(item.dataset.id)});on("composer","submit",e=>{e.preventDefault();sendMessage()});on("prompt","keydown",e=>{if(e.key==="Enter"&&!e.shiftKey){e.preventDefault();$("composer").requestSubmit()}}); 
async function loadTasks(){
 const button=$("refresh-tasks");if(button)button.disabled=true;
 $("tasks-list").innerHTML='<div class="empty" role="status">Loading tasks…</div>';
 try{state.tasks=await api("/tasks");renderTasks()}
 catch(e){$("tasks-list").innerHTML='<div class="empty" role="alert">Unable to load tasks: '+escapeHtml(e.message)+'<p><button type="button" class="secondary" data-retry-tasks>Try again</button></p></div>'}
 finally{if(button)button.disabled=false}
}
let tasksPage=1;
const TASKS_PAGE_SIZE=10;
function filteredTasks(){
 const query=$("task-search")?.value.trim().toLocaleLowerCase()||"";
 const enabledOnly=$("tasks-enabled-only")?.checked||false;
 return state.tasks.filter(task=>(!enabledOnly||task.enabled)&&[task.name,task.description,task.agent,task.schedule||"One-time"].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
}
function taskScheduleLabel(task){
 if(task.schedule)return task.schedule;
 if(task.at)return "Once";
 return "One-time";
}
function renderTasks(){
 const tasks=filteredTasks();
 const target=$("tasks-list");
 if(target)target.innerHTML=tasks.length?tasks.map(task=>'<button type="button" class="family-list-item '+(task.id===state.selectedTask?"active":"")+'" data-task-id="'+escapeHtml(task.id)+'"><strong>'+escapeHtml(task.name)+'</strong><span>'+(task.enabled?"Enabled":"Disabled")+' · '+escapeHtml(task.schedule||"One-time")+(task.next_run?' · Next '+escapeHtml(new Date(task.next_run).toLocaleString()):'')+'</span></button>').join(""):'<div class="empty">'+(state.tasks.length?'No tasks match these filters. <button type="button" class="secondary" data-clear-task-filters>Clear filters</button>':'No tasks yet. Create one to get started.')+'</div>';
 renderTasksTable(tasks);
}
function renderTasksTable(tasks=filteredTasks()){
 const body=$("tasks-table-body");if(!body)return;
 body.innerHTML=tasks.length?tasks.map(task=>{
  const next=task.next_run||task.at;
  const nextLabel=next?new Date(next).toLocaleString():"";
  return '<button type="button" class="item-card task-item-card" data-task-id="'+escapeHtml(task.id)+'" aria-label="Open '+escapeHtml(task.name||"Untitled task")+'"><div class="item-card-heading"><strong>'+escapeHtml(task.name||"Untitled task")+'</strong><span class="tasks-status-label '+(task.enabled?"enabled":"disabled")+'">'+(task.enabled?"Enabled":"Disabled")+'</span></div><p class="item-card-description">'+escapeHtml(task.description||task.agent||"No description")+'</p><div class="item-card-meta"><span class="item-card-tag">'+escapeHtml(taskScheduleLabel(task))+'</span><span>Agent · '+escapeHtml(task.agent||"—")+'</span></div>'+(nextLabel?'<div class="item-card-footnote">Next run · '+escapeHtml(nextLabel)+'</div>':'')+'<span class="item-card-arrow" aria-hidden="true">›</span></button>';
 }).join(""):'<div class="tasks-table-empty"><strong>'+(state.tasks.length?"No tasks found":"No tasks yet")+'</strong><span>'+(state.tasks.length?"Try changing your search or filters.":"Create a task to get started.")+'</span></div>';
 const info=$("tasks-page-info");if(info)info.textContent=tasks.length?tasks.length+" tasks":"0 tasks";
}
function showTasksOverview(){
 document.body.dataset.taskView="list";
 const overview=$("tasks-overview"),editor=$("task-editor-view");
 if(!overview||!editor)return;
 overview.hidden=false;editor.hidden=true;
 if($("open-tasks"))$("open-tasks").hidden=false;$("back-to-tasks").hidden=true;
 $("tasks-main-title").textContent="All tasks";
 $("tasks-main-subtitle").textContent="Your tasks, including their schedules and status";
}
function showTaskEditor(title){
 document.body.dataset.taskView="detail";
 const overview=$("tasks-overview"),editor=$("task-editor-view");
 if(!overview||!editor)return;
 overview.hidden=true;editor.hidden=false;
 if($("open-tasks"))$("open-tasks").hidden=true;$("back-to-tasks").hidden=false;
 $("tasks-main-title").textContent=title;
 $("tasks-main-subtitle").textContent="View and edit task details";
}

function renderTaskAgents(){
 const select=$("task-agent"),selected=select.value;
 select.innerHTML=state.agents.map(agent=>'<option value="'+escapeHtml(agent.name)+'">'+escapeHtml(agent.name)+'</option>').join("");
 if(state.agents.some(agent=>agent.name===selected))select.value=selected;
}
function renderTaskFamilies(){
 const select=$("task-family");if(!select)return;
 const selected=select.value;
 select.innerHTML='<option value="">No family context</option>'+state.families.map(family=>'<option value="'+escapeHtml(family.id)+'">'+escapeHtml(family.name)+'</option>').join("");
 select.value=state.families.some(family=>family.id===selected)?selected:"";
}
function setTaskSection(active,focus=false){
 const tabs=[...document.querySelectorAll("[data-task-section-tab]")];
 const selectedTab=tabs.find(tab=>tab.dataset.taskSectionTab===active&&!tab.disabled);
 if(!selectedTab)return;
 for(const tab of tabs){
  const selected=tab===selectedTab;
  tab.classList.toggle("active",selected);
  tab.setAttribute("aria-selected",String(selected));
  tab.tabIndex=selected?0:-1;
 }
 for(const section of document.querySelectorAll("[data-task-section]"))section.hidden=section.dataset.taskSection!==active;
 if(focus)selectedTab.focus();
}
function updateTaskSectionTabs(active="details"){
 const historyTab=document.querySelector("[data-task-section-tab=\"history\"]");
 if(historyTab)historyTab.disabled=!state.selectedTask||state.selectedTask==="new";
 setTaskSection(historyTab?.disabled?"details":active);
}
on("task-section-nav","click",event=>{const tab=event.target.closest("[data-task-section-tab]");if(tab&&!tab.disabled)setTaskSection(tab.dataset.taskSectionTab)});
on("task-section-nav","keydown",event=>{
 const tabs=[...document.querySelectorAll("[data-task-section-tab]:not(:disabled)")];
 const current=tabs.indexOf(event.target.closest("[data-task-section-tab]"));
 if(current<0)return;
 let next=current;
 if(event.key==="ArrowRight")next=(current+1)%tabs.length;
 else if(event.key==="ArrowLeft")next=(current-1+tabs.length)%tabs.length;
 else if(event.key==="Home")next=0;
 else if(event.key==="End")next=tabs.length-1;
 else return;
 event.preventDefault();
 setTaskSection(tabs[next].dataset.taskSectionTab,true);
});
function resetTaskEditor(){
 state.selectedTask=null;renderTasks();
 showTasksOverview();
 updateTaskSectionTabs("details");
 const executionPanel=$("task-executions");if(executionPanel)executionPanel.hidden=true;
 for(const id of ["task-name","task-description","task-agent","task-prompt","task-family","task-schedule","task-at","task-timezone","task-enabled","save-task"])$(id).disabled=true;
 $("task-heading").textContent="Select a task";
 $("task-name").value="";$("task-description").value="";$("task-prompt").value="";$("task-family").value="";$("task-schedule").value="";$("task-at").value="";$("task-timezone").value="Europe/Madrid";$("task-enabled").checked=true;
 $("delete-task").disabled=true;$("tasks-status").textContent="Choose a task or create a new one.";
}
function startNewTask(){
 state.selectedTask="new";renderTasks();
 showTaskEditor("New task");
 updateTaskSectionTabs("details");
 for(const id of ["task-name","task-description","task-agent","task-prompt","task-family","task-schedule","task-at","task-timezone","task-enabled","save-task"])$(id).disabled=false;
 $("task-heading").textContent="New task";
 $("delete-task").disabled=true;$("task-name").value="";$("task-description").value="";$("task-prompt").value="";$("task-family").value="";$("task-schedule").value="";$("task-at").value="";$("task-timezone").value="Europe/Madrid";$("task-enabled").checked=true;
 renderTaskAgents();renderTaskFamilies();$("tasks-status").textContent="Set either a cron schedule or a one-time execution date, not both.";$("task-name").focus();
}
function toLocalDateTime(value){const date=new Date(value);return new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,16)}
function selectTask(id){
 const task=state.tasks.find(item=>item.id===id);if(!task)return;
 state.selectedTask=id;renderTasks();
 showTaskEditor(task.name);
 updateTaskSectionTabs("details");
 for(const field of ["task-name","task-description","task-agent","task-prompt","task-family","task-schedule","task-at","task-timezone","task-enabled","save-task"])$(field).disabled=false;
 $("delete-task").disabled=false;$("task-heading").textContent=task.name;
 renderTaskAgents();renderTaskFamilies();$("task-name").value=task.name;$("task-description").value=task.description||"";$("task-agent").value=task.agent;$("task-prompt").value=task.prompt;$("task-family").value=task.family_id||"";
 $("task-schedule").value=task.schedule||"";
 $("task-at").value=task.at?toLocalDateTime(task.at):"";
 $("task-timezone").value=task.timezone||"Europe/Madrid";$("task-enabled").checked=!!task.enabled;
 $("tasks-status").textContent=task.enabled?"Task is enabled.":"Task is disabled.";
 if(typeof loadTaskExecutions==="function")void loadTaskExecutions(id);
}
async function saveTask(event){
 event.preventDefault();
 const schedule=$("task-schedule").value.trim(),atValue=$("task-at").value;
 if(Boolean(schedule)===Boolean(atValue)){$("tasks-status").textContent="Specify either a cron schedule or a one-time date, but not both.";return}
 const body={name:$("task-name").value.trim(),description:$("task-description").value.trim(),agent:$("task-agent").value,prompt:$("task-prompt").value,schedule:schedule||null,at:atValue?new Date(atValue).toISOString():null,timezone:schedule?($("task-timezone").value.trim()||"UTC"):null,family_id:$("task-family").value||null,enabled:$("task-enabled").checked};
 $("save-task").disabled=true;
 try{
  const saved=state.selectedTask==="new"?await api("/tasks",{method:"POST",body:JSON.stringify(body)}):await api("/tasks/"+encodeURIComponent(state.selectedTask),{method:"PUT",body:JSON.stringify(body)});
  await loadTasks();selectTask(saved.id);$("tasks-status").textContent="Task saved.";
 }catch(e){$("tasks-status").textContent=e.message}
 finally{$("save-task").disabled=false}
}
async function deleteTask(){
 if(!state.selectedTask||state.selectedTask==="new")return;
 if(!window.confirm("Delete this task permanently?"))return;
 try{await api("/tasks/"+encodeURIComponent(state.selectedTask),{method:"DELETE"});await loadTasks();resetTaskEditor();$("tasks-status").textContent="Task deleted."}
 catch(e){$("tasks-status").textContent=e.message}
}
async function loadProfile(){
 const displayName=$("profile-display-name"),timezone=$("profile-timezone"),status=$("profile-status");
 if(!displayName||!status)return;
 try{const profile=await api("/profile");displayName.value=profile.display_name||"";if(timezone)timezone.value=profile.timezone||"";status.textContent=profile.display_name?"Your profile is up to date.":"Choose the name agents should use for you."}
 catch(e){status.textContent=e.message}
}
async function saveProfile(event){
 event.preventDefault();const name=$("profile-display-name").value.trim(),timezone=$("profile-timezone")?.value.trim()||"";if(!name){$("profile-status").textContent="Display name must not be empty.";return}
 $("save-profile").disabled=true;
 try{const profile=await api("/profile",{method:"PUT",body:JSON.stringify({display_name:name,timezone})});$("profile-display-name").value=profile.display_name;if($("profile-timezone"))$("profile-timezone").value=profile.timezone||"";$("profile-status").textContent="Profile saved."}
 catch(e){$("profile-status").textContent=e.message}
 finally{$("save-profile").disabled=false}
}
on("profile-form","submit",saveProfile);
function googleHasWriteScope(connection){return (connection.scopes||[]).some(scope=>scope==="https://www.googleapis.com/auth/calendar.events"||scope==="https://www.googleapis.com/auth/calendar")}
function googleHasReadScope(connection){return (connection.scopes||[]).some(scope=>scope==="https://www.googleapis.com/auth/calendar.events.readonly"||scope==="https://www.googleapis.com/auth/calendar.events"||scope==="https://www.googleapis.com/auth/calendar")}
function renderIntegrationNav(){
 const summary=$("google-integration-summary");
 if(summary){
  const active=state.googleConnections.filter(connection=>connection.status==="active").length;
  summary.textContent=state.googleConnections.length
   ? active+" active account"+(active===1?"":"s")+" · "+state.googleConnections.length+" saved"
   : "Connect an account to get started";
 }
 const traktSummary=$("trakt-integration-summary");
 if(traktSummary)traktSummary.textContent=state.traktProfiles.length
  ?state.traktProfiles.length+" personal media profile"+(state.traktProfiles.length===1?"":"s")
  :"Personal profile is provisioned automatically";
}
function renderIntegrationView(){
 document.querySelectorAll("[data-integration]").forEach(button=>{
  const selected=button.dataset.integration===state.activeIntegration;
  button.classList.toggle("active",selected);
  button.setAttribute("aria-pressed",String(selected));
  if(selected)button.setAttribute("aria-current","page");else button.removeAttribute("aria-current");
 });
 document.querySelectorAll("[data-integration-panel]").forEach(panel=>{
  panel.hidden=panel.dataset.integrationPanel!==state.activeIntegration;
 });
}
function selectIntegration(name){
 if(!["google","trakt"].includes(name))return;
 state.activeIntegration=name;
 setProfileView("detail");
 renderIntegrationView();
}
function renderGoogleConnections(){
 renderIntegrationNav();
 const target=$("google-connections");if(!target)return;
 const query=$("google-search")?.value.trim().toLocaleLowerCase()||"";
 if(!state.googleConnections.length){target.innerHTML='<div class="panel integrations-empty"><div class="empty">No Google accounts connected yet.</div></div>';return}
 const connections=state.googleConnections.map(connection=>{
  const calendars=state.googleCalendars[connection.id]||[];
  const accountMatches=[connection.email,connection.status].some(value=>String(value||"").toLocaleLowerCase().includes(query));
  return {connection,calendars:!query||accountMatches?calendars:calendars.filter(calendar=>[calendar.name,calendar.summary,calendar.id].some(value=>String(value||"").toLocaleLowerCase().includes(query))),accountMatches};
 }).filter(item=>!query||item.accountMatches||item.calendars.length);
 target.innerHTML=connections.length?connections.map(({connection,calendars})=>{
  const writeScope=googleHasWriteScope(connection),status=connection.status==="active"?"Active":"Disconnected";
  return '<section class="panel google-account" data-connection-id="'+escapeHtml(connection.id)+'"><div class="google-account-heading"><div><h3>'+escapeHtml(connection.email||"Google account")+'</h3><p class="hint">'+status+' · '+(googleHasReadScope(connection)?"Calendar read access":"No calendar read scope")+' · '+(writeScope?"Google write scope granted":"Read-only Google scope")+'</p></div><button type="button" class="danger" data-google-action="disconnect" data-connection-id="'+escapeHtml(connection.id)+'" '+(connection.status!=="active"?"disabled":"")+'>Disconnect</button></div><div class="calendar-toolbar"><div><h4>Calendars</h4><p class="hint">New calendars are not selected automatically.</p></div><button type="button" class="secondary" data-google-action="sync" data-connection-id="'+escapeHtml(connection.id)+'" '+(connection.status!=="active"?"disabled":"")+'>Sync calendars</button></div>'+(connection.status!=="active"?'<p class="hint">Reconnect this Google account to restore access.</p>':calendars.length?calendars.map(calendar=>renderGoogleCalendar(connection,calendar,writeScope)).join(""):'<p class="hint">'+(query?"No calendars match this filter.":"No calendars discovered yet. Select “Sync calendars” to load them from Google.")+'</p>')+'</section>';
 }).join(""):'<div class="panel integrations-empty"><div class="empty">No accounts or calendars match this filter. <button type="button" class="secondary" data-clear-google-filter>Clear filter</button></div></div>';
}
function renderGoogleCalendar(connection,calendar,writeScope){
 const familyOptions=state.families.map(f=>'<option value="'+escapeHtml(f.id)+'" '+(calendar.family_id===f.id?"selected":"")+'>'+escapeHtml(f.name)+'</option>').join(""),familyEnabled=calendar.selected&&state.families.length>0,familyAccess=calendar.family_access||"viewer";
 const accessOptions='<option value="read_only" '+(calendar.access_mode==="read_only"?"selected":"")+'>Read only</option><option value="read_write" '+(calendar.access_mode==="read_write"?"selected":"")+' '+(!writeScope?"disabled":"")+'>Read and write</option>';
 return '<article class="calendar-item" data-calendar-id="'+escapeHtml(calendar.id)+'" data-connection-id="'+escapeHtml(connection.id)+'"><div class="calendar-main"><label class="calendar-select"><input type="checkbox" data-calendar-field="selected" '+(calendar.selected?"checked":"")+'> <span><strong>'+escapeHtml(calendar.display_name||"Untitled calendar")+'</strong><small>'+escapeHtml(calendar.time_zone||"Timezone not provided")+(calendar.provider_access_role?" · Google role: "+escapeHtml(calendar.provider_access_role):"")+'</small></span></label><label class="calendar-access">EME access<select data-calendar-field="access_mode" '+(!calendar.selected?"disabled":"")+'>'+accessOptions+'</select></label></div><div class="calendar-sharing"><div class="calendar-sharing-current">'+(calendar.family_id?'<span class="hint">Shared with '+escapeHtml(state.families.find(f=>f.id===calendar.family_id)?.name||"a family")+' as '+escapeHtml(calendar.family_access||"viewer")+'</span>':'<span class="hint">Not shared with a family</span>')+'</div><div class="calendar-sharing-controls"><label>Family<select data-family-id '+(!familyEnabled?"disabled":"")+'><option value="">Choose a family</option>'+familyOptions+'</select></label><label>Permission<select data-family-access '+(!familyEnabled?"disabled":"")+'><option value="viewer" '+(familyAccess==="viewer"?"selected":"")+'>Viewer</option><option value="contributor" '+(familyAccess==="contributor"?"selected":"")+' '+(calendar.access_mode!=="read_write"?"disabled":"")+'>Contributor</option></select></label><button type="button" class="secondary" data-google-action="share" '+(!familyEnabled?"disabled":"")+'>'+(calendar.family_id?"Update sharing":"Share")+'</button>'+(calendar.family_id?'<button type="button" class="danger" data-google-action="unshare">Stop sharing</button>':"")+'</div>'+(calendar.access_mode!=="read_write"&&calendar.family_access==="contributor"?'<p class="hint">Contributor access requires owner read/write permission.</p>':"")+'</div></article>';
}
async function loadGoogleConnections(){
 state.googleConnections=await api("/integrations/google/connections");
 await Promise.all(state.googleConnections.filter(c=>c.status==="active").map(async c=>{try{state.googleCalendars[c.id]=await api("/integrations/google/connections/"+encodeURIComponent(c.id)+"/calendars")}catch(e){state.googleCalendars[c.id]=[]}}));
 renderGoogleConnections();
}
function renderTraktProfiles(){
 const target=$("trakt-profiles");if(!target)return;
 renderIntegrationNav();
 const connectable=state.traktProfiles.filter(profile=>!profile.trakt);
 const selector=$("trakt-connect-profile"),connectButton=$("trakt-connect");
 if(selector){
  selector.innerHTML=connectable.length?connectable.map(profile=>'<option value="'+escapeHtml(profile.profile_id)+'">'+escapeHtml(profile.profile_name)+' · '+escapeHtml(profile.family_name)+'</option>').join(""):'<option value="">No unconnected profile available</option>';
  selector.disabled=connectable.length===0;
 }
 if(connectButton)connectButton.disabled=connectable.length===0;
 const query=$("trakt-search")?.value.trim().toLocaleLowerCase()||"";
 if(!state.traktProfiles.length){target.innerHTML='<div class="panel integrations-empty"><div class="empty">No personal media profile is available yet.</div></div>';return}
 const profiles=state.traktProfiles.filter(profile=>[profile.profile_name,profile.family_name,profile.trakt?.username,profile.trakt?.display_name].some(value=>String(value||"").toLocaleLowerCase().includes(query)));
 target.innerHTML=profiles.length?profiles.map(profile=>{
  const trakt=profile.trakt,sync=profile.sync||{},lastSync=sync.last_synced_at?new Date(sync.last_synced_at).toLocaleString():"Never synchronized";
  const account=trakt?'<strong>'+escapeHtml(trakt.display_name||trakt.username)+'</strong><small>Trakt account · @'+escapeHtml(trakt.username)+'</small>':'<span class="hint">No Trakt account connected</span>';
  const summary=state.traktSummaries[profile.profile_id]||{};
  const actions=trakt?'<button type="button" data-trakt-action="sync" data-profile-id="'+escapeHtml(profile.profile_id)+'" data-family-id="'+escapeHtml(profile.family_id||"")+'">Sync now</button><button type="button" class="danger" data-trakt-action="disconnect" data-profile-id="'+escapeHtml(profile.profile_id)+'" data-family-id="'+escapeHtml(profile.family_id||"")+'">Disconnect</button>':'<span class="hint">Use the Connect Trakt action above to link this profile.</span>';
  const hasImportedData=(summary.watched_movies||0)+(summary.watched_shows||0)+(summary.ratings||0)+(summary.watchlist_items||0)>0;
  const stats=hasImportedData?'<div class="trakt-stats"><div><strong>'+Number(summary.watched_movies||0).toLocaleString()+'</strong><span>Movies watched</span></div><div><strong>'+Number(summary.watched_shows||0).toLocaleString()+'</strong><span>Shows watched</span></div><div><strong>'+Number(summary.ratings||0).toLocaleString()+'</strong><span>Ratings</span></div><div><strong>'+Number(summary.watchlist_items||0).toLocaleString()+'</strong><span>Watchlist</span></div></div>':'<p class="hint trakt-no-data">'+(trakt?"No imported Trakt activity yet. Select Sync now to import your history, ratings and watchlist.":"No imported Trakt data for this profile.")+'</p>';
  const recent=(summary.recently_watched||[]).length?'<div class="trakt-recent"><h4>Recently watched</h4><ul>'+summary.recently_watched.map(item=>'<li><span>'+escapeHtml(item.title)+'</span><small>'+((item.kind==="movie")?"Movie":"TV show")+' · '+escapeHtml(new Date(item.watched_at).toLocaleDateString())+'</small></li>').join("")+'</ul></div>':'';
  return '<section class="panel google-account"><div class="google-account-heading"><div><h3>'+escapeHtml(profile.profile_name)+'</h3><p class="hint">'+escapeHtml(profile.family_name)+' · '+(trakt?"Connected":"Not connected")+'</p></div></div><div class="calendar-toolbar"><div>'+account+'<p class="hint">'+(trakt?"Last synchronized: "+escapeHtml(lastSync):"Imported media data is retained when Trakt is disconnected.")+'</p></div><div class="integration-button-row">'+actions+'</div></div><div class="trakt-data">'+stats+recent+'</div></section>';
 }).join(""):'<div class="panel integrations-empty"><div class="empty">No media profiles match this filter. <button type="button" class="secondary" data-clear-trakt-filter>Clear filter</button></div></div>';
}
async function loadTraktProfiles(loadSummaries=true){
 state.traktProfiles=await api("/media/trakt/profiles");
 if(loadSummaries){
  const summaries=await Promise.all(state.traktProfiles.map(async profile=>{
   const family=profile.family_id?"?family_id="+encodeURIComponent(profile.family_id):"";
   try{return [profile.profile_id,await api("/media/trakt/profiles/"+encodeURIComponent(profile.profile_id)+"/summary"+family)]}
   catch(error){console.warn("Unable to load Trakt summary for profile",profile.profile_id,error);return [profile.profile_id,null]}
  }));
  state.traktSummaries=Object.fromEntries(summaries.filter(([,summary])=>summary));
 }
 renderTraktProfiles();
}
async function loadIntegrations(){
 const status=$("integrations-status");renderIntegrationView();await loadFamilies();const errors=[];
 try{await loadGoogleConnections()}catch(e){const message=e.message.includes("Google OAuth is not configured")?"Google Calendar integration is not configured on this server.":e.message;$("google-connections").innerHTML='<div class="panel integrations-empty"><div class="empty">'+escapeHtml(message)+'</div></div>';errors.push(message)}
 try{await loadTraktProfiles()}catch(e){const message=e.message.includes("Trakt OAuth is not configured")?"Trakt integration is not configured on this server.":e.message;$("trakt-profiles").innerHTML='<div class="panel integrations-empty"><div class="empty">'+escapeHtml(message)+'</div></div>';errors.push(message);renderIntegrationNav()}
 status.textContent=errors.join(" ");
}
async function connectTrakt(profile){
 const status=$("integrations-status"),popup=window.open("about:blank","eme-trakt-oauth","popup,width=620,height=760");
 if(!popup){status.textContent="Allow pop-ups for this site to connect a Trakt account.";return}
 const previous=profile.trakt?.linked_at||null;status.textContent="Preparing Trakt authorization…";
 try{
  const auth=await api("/media/trakt/link",{method:"POST",body:JSON.stringify({profile_id:profile.profile_id,...(profile.family_id?{family_id:profile.family_id}:{})})});
  popup.location.href=auth.authorization_url;status.textContent="Complete authorization in the Trakt window. This page will update automatically.";
  const deadline=Date.now()+10*60*1000;
  while(Date.now()<deadline){
   await new Promise(resolve=>setTimeout(resolve,1500));await loadTraktProfiles(false);
   const updated=state.traktProfiles.find(item=>item.profile_id===profile.profile_id);
   if(updated?.trakt&&updated.trakt.linked_at!==previous){await loadTraktProfiles();if(!popup.closed)popup.close();status.textContent="Trakt account connected.";return}
   if(popup.closed)break;
  }
  status.textContent="Authorization was not completed. You can try again.";
 }catch(e){status.textContent=e.message}finally{if(!popup.closed)popup.close()}
}
async function handleTraktIntegrationAction(event){
 const button=event.target.closest("[data-trakt-action]");if(!button)return;
 const action=button.dataset.traktAction,profileId=button.dataset.profileId,familyId=button.dataset.familyId;
 const profile=state.traktProfiles.find(item=>item.profile_id===profileId),status=$("integrations-status");if(!profile)return;
 button.disabled=true;
 try{
  if(action==="connect")await connectTrakt(profile);
  else if(action==="sync"){
   const result=await api("/media/trakt/profiles/"+encodeURIComponent(profileId)+"/sync"+(familyId?"?family_id="+encodeURIComponent(familyId):""),{method:"POST"});
   await loadTraktProfiles();status.textContent="Trakt synchronized: "+(result.watched_movies||0)+" movies, "+(result.watched_shows||0)+" shows, "+(result.ratings||0)+" ratings and "+(result.watchlist_items||0)+" watchlist items.";
  }else if(action==="disconnect"){
   if(!window.confirm("Disconnect this Trakt account? Imported media history will remain in the profile."))return;
   await api("/media/trakt/profiles/"+encodeURIComponent(profileId),{method:"DELETE",body:JSON.stringify(familyId?{family_id:familyId}:{})});
   await loadTraktProfiles();status.textContent="Trakt account disconnected.";
  }
 }catch(e){status.textContent=e.message}finally{button.disabled=false}
}
async function connectGoogle(){
 const button=$("google-connect"),status=$("integrations-status"),access=$("google-connect-access").value,popup=window.open("about:blank","eme-google-oauth","popup,width=620,height=760");
 if(!popup){status.textContent="Allow pop-ups for this site to connect a Google account.";return}
 button.disabled=true;status.textContent="Preparing Google authorization…";
 let callbackCompleted=false;
 const onOAuthMessage=async event=>{
  if(event.origin!==window.location.origin||event.source!==popup||event.data?.type!=="eme-google-oauth"||event.data?.status!=="success")return;
  callbackCompleted=true;
  try{await loadGoogleConnections();status.textContent="Google account connected."}
  catch(error){status.textContent="Google authorization succeeded, but refreshing account details failed: "+error.message}
  if(!popup.closed)popup.close();
 };
 window.addEventListener("message",onOAuthMessage);
 try{
  const before=state.googleConnections.map(c=>c.id+":"+c.status+":"+c.updated_at+":"+(c.scopes||[]).join(",")).join("|"),auth=await api("/integrations/google/oauth/start",{method:"POST",body:JSON.stringify({access})});
  popup.location.href=auth.authorization_url;status.textContent="Complete authorization in the Google window. This page will update automatically.";
  const deadline=Date.now()+auth.expires_in_seconds*1000;
  while(Date.now()<deadline){
   await new Promise(resolve=>setTimeout(resolve,1500));
   const connections=await api("/integrations/google/connections"),after=connections.map(c=>c.id+":"+c.status+":"+c.updated_at+":"+(c.scopes||[]).join(",")).join("|");
   if(after!==before){callbackCompleted=true;await loadGoogleConnections();status.textContent="Google account connected.";if(!popup.closed)popup.close();return}
   if(popup.closed)break;
  }
  if(!callbackCompleted)status.textContent="Authorization was not completed. You can try again.";
 }catch(e){status.textContent=e.message}
 finally{window.removeEventListener("message",onOAuthMessage);if(!popup.closed)popup.close();button.disabled=false}
}
async function handleGoogleIntegrationAction(event){
 const button=event.target.closest("[data-google-action]");if(!button)return;
 const action=button.dataset.googleAction,account=button.dataset.connectionId,item=button.closest("[data-calendar-id]"),calendar=item?.dataset.calendarId,status=$("integrations-status");button.disabled=true;
 try{
  if(action==="disconnect"){
   const connection=state.googleConnections.find(c=>c.id===account);
   if(!window.confirm("Disconnect "+(connection?.email||"this Google account")+"? Agents will immediately lose access to its calendars."))return;
   await api("/integrations/google/connections/"+encodeURIComponent(account),{method:"DELETE"});delete state.googleCalendars[account];await loadGoogleConnections();status.textContent="Google account disconnected.";
  }else if(action==="sync"){
   state.googleCalendars[account]=await api("/integrations/google/connections/"+encodeURIComponent(account)+"/calendars/sync",{method:"POST"});renderGoogleConnections();status.textContent="Calendars synchronized.";
  }else if(action==="share"){
   const familyId=item.querySelector("[data-family-id]").value,access=item.querySelector("[data-family-access]").value;if(!familyId)throw new Error("Choose a family before sharing this calendar.");
   await api("/integrations/google/calendars/"+encodeURIComponent(calendar)+"/family-sharing",{method:"PUT",body:JSON.stringify({family_id:familyId,access})});await loadGoogleConnections();status.textContent="Family sharing saved.";
  }else if(action==="unshare"){
   await api("/integrations/google/calendars/"+encodeURIComponent(calendar)+"/family-sharing",{method:"DELETE"});await loadGoogleConnections();status.textContent="Family sharing removed.";
  }
 }catch(e){status.textContent=e.message}
 finally{button.disabled=false}
}
async function handleGoogleCalendarChange(event){
 const field=event.target.dataset.calendarField;if(!field)return;const item=event.target.closest("[data-calendar-id]");if(!item)return;
 const status=$("integrations-status"),body=field==="selected"?{selected:event.target.checked}:{access_mode:event.target.value};event.target.disabled=true;
 try{await api("/integrations/google/calendars/"+encodeURIComponent(item.dataset.calendarId),{method:"PATCH",body:JSON.stringify(body)});await loadGoogleConnections();status.textContent="Calendar settings saved."}
 catch(e){status.textContent=e.message;await loadGoogleConnections()}
}
on("google-connect","click",connectGoogle);

on("integrations-sidebar","click",event=>{
 const button=event.target.closest("[data-integration]");
 if(button)selectIntegration(button.dataset.integration);
});
on("profile-content-back","click",()=>{
 if(page==="integrations")setProfileView("list");
 else if(page==="families")void loadFamilyDetails(null);
});
on("google-search","input",renderGoogleConnections);on("google-connections","click",e=>{if(e.target.closest("[data-clear-google-filter]")){$("google-search").value="";renderGoogleConnections();return}handleGoogleIntegrationAction(e)});
on("google-connections","change",handleGoogleCalendarChange);
on("trakt-search","input",renderTraktProfiles);on("trakt-profiles","click",e=>{if(e.target.closest("[data-clear-trakt-filter]")){$("trakt-search").value="";renderTraktProfiles();return}handleTraktIntegrationAction(e)});
on("trakt-connect","click",async()=>{
 const profileId=$("trakt-connect-profile")?.value;
 const profile=state.traktProfiles.find(item=>item.profile_id===profileId&&!item.trakt);
 if(!profile){$("integrations-status").textContent="Select a media profile before connecting Trakt.";return}
 const button=$("trakt-connect");button.disabled=true;
 try{await connectTrakt(profile)}finally{renderTraktProfiles()}
});

async function showConsole(){
 document.body.classList.remove("login-active");
 $("invite-screen").hidden=true;if($("logout"))$("logout").hidden=false;
 if(pendingInviteToken){document.querySelector("main").hidden=true;$("invite-screen").hidden=false;$("connection").textContent="Accept invitation";return}
 document.querySelector("main").hidden=false;
 if(page==="conversations"){
  state.agents=await api("/agents");renderAgents();await Promise.all([loadFamilies(),loadConversations()]);
  if($("conversations-overview")){clearSelectedConversation();conversationsPage=1;renderConversations()}else if(state.conversations.length){let preferred=null;try{preferred=localStorage.getItem("eme.selectedConversation")}catch{}const selected=state.conversations.find(c=>c.id===preferred)||state.conversations[0];await selectConversation(selected.id)}else renderFamilySelector();
 }else if(page==="families"){
  await loadFamilies();
  await loadFamilyDetails(state.families[0]?.id||null);
 }else if(page==="tasks"){
  state.agents=await api("/agents");renderTaskAgents();state.families=await api("/families");renderTaskFamilies();await loadTasks();resetTaskEditor();
 }else if(page==="profile"){
  await Promise.all([loadProfile(),loadFamilies()]);
 }else if(page==="integrations"){
  await loadIntegrations();
 }else{
  await loadFamilies();await loadNotes();resetNoteEditor();
 }
 $("connection").textContent="Connected";
}
on("invite-accept","click",async()=>{
 if(!pendingInviteToken)return;const button=$("invite-accept"),status=$("invite-status");button.disabled=true;status.textContent="Validating and accepting invitation…";
 try{await api("/invitations/accept",{method:"POST",body:JSON.stringify({token:pendingInviteToken,relationship:$("invite-relationship").value})});pendingInviteToken=null;window.history.replaceState({},"", consoleBase+"/families");$("invite-screen").hidden=true;await showConsole();setStatus("Family invitation accepted","ok")}catch(e){status.textContent=e.message;button.disabled=false}
});
on("show-archived","change",async e=>{state.showArchived=e.target.checked;try{await loadConversations()}catch(e){setStatus(e.message,"error")}});
on("conversation-search","input",e=>{state.conversationSearch=e.target.value;conversationsPage=1;renderConversations()});
on("conversations-table-body","click",e=>{
 const editButton=e.target.closest("button[data-edit-id]");
 if(editButton){e.stopPropagation();openEditConversationDialog(editButton.dataset.editId);return}
 const archiveButton=e.target.closest("button[data-archive-id]");
 if(archiveButton){e.stopPropagation();if(!state.runId)archiveConversation(archiveButton.dataset.archiveId);return}
 const deleteButton=e.target.closest("button[data-delete-id]");
 if(deleteButton){e.stopPropagation();if(!state.runId)deleteConversation(deleteButton.dataset.deleteId);return}
 const row=e.target.closest("[data-conversation-id]");if(row)selectConversation(row.dataset.conversationId);
});
on("conversations-table-body","keydown",e=>{const row=e.target.closest("[data-conversation-id]");if(row&&e.target===row&&(e.key==="Enter"||e.key===" ")){e.preventDefault();selectConversation(row.dataset.conversationId)}});
on("back-to-conversations","click",clearSelectedConversation);
on("conversations-prev-page","click",()=>{conversationsPage=Math.max(1,conversationsPage-1);renderConversations()});
on("conversations-next-page","click",()=>{conversationsPage++;renderConversations()});

window.emeConsoleReady.then(async () => {
  try {
    await showConsole();
  } catch (error) {
    // A protected API may have returned 401 while the view was initializing.
    // In that case the shared auth gate owns the screen; do not reveal the view.
    if (error?.status === 401) return;
    document.body.classList.remove("login-active");
    $("invite-screen").hidden = true;
    if ($("logout")) $("logout").hidden = false;
    document.querySelector("main").hidden = false;
    $("connection").textContent = "Error loading page: " + error.message;
    $("connection").className = "hint error";
    console.error("Failed to initialize console page", error);
  }
});


const taskExecutionLabels={
 queued:"Queued",started:"Execution started",model_requested:"Model request",
 model_candidates_discovered:"Compatible models",model_candidate_attempt_started:"Trying model",
 model_candidate_attempt_failed:"Model candidate failed",model_fallback:"Model fallback",
 model_selected:"Model selected",model_routing_failed:"Model routing failed",
 model_responded:"Model response",tool_execution_started:"Tool started",
 tool_execution_finished:"Tool finished",model_failed:"Model request failed",
 runtime_cancelled:"Runtime cancelled",runtime_timed_out:"Runtime timed out",
 completed:"Completed",failed:"Failed",cancelled:"Cancelled",timed_out:"Timed out",
 max_iterations_reached:"Maximum iterations reached"
};

function renderTaskExecutionEvent(event){
 const timestamp=Number(event.occurred_at_unix_ms);
 const time=Number.isFinite(timestamp)?new Date(timestamp).toLocaleString():"Unknown time";
 const details=[
  event.iteration!==null&&event.iteration!==undefined?"Iteration "+event.iteration:null,
  event.tool_name?"Tool: "+event.tool_name:null,
  event.model_id?"Model: "+event.model_id:null,
  event.next_model_id?"Next model: "+event.next_model_id:null,
  event.model_candidates?.length?"Candidates: "+event.model_candidates.join(", "):null,
  event.error_category?"Error: "+event.error_category.replaceAll("_"," "):null
 ].filter(Boolean).join(" · ");
 return '<tr><td class="task-execution-event-time">'+escapeHtml(time)+'</td><td><strong>'+escapeHtml(taskExecutionLabels[event.kind]||event.kind)+'</strong></td><td>'+(details?'<span>'+escapeHtml(details)+'</span>':'<span class="hint">—</span>')+'<details class="task-execution-event-payload"><summary>Payload</summary><pre>'+escapeHtml(JSON.stringify(event,null,2))+'</pre></details></td></tr>';
}

function renderTaskExecution(execution){
 const events=execution.events||[],usage=execution.usage||[];
 const output=execution.response
  ?'<div class="markdown-content">'+renderMarkdown(execution.response)+'</div>'
  :'<p class="hint">No final response was recorded.</p>';
 const failure=execution.error
  ?'<section class="tool-section"><h4>Error</h4><pre class="tool-error">'+escapeHtml(execution.error)+'</pre></section>':"";
 const input=execution.input
  ?'<pre>'+escapeHtml(execution.input)+'</pre>'
  :'<p class="hint">No submitted prompt was recorded.</p>';
 const timeline='<div class="task-execution-table-wrap"><table class="task-execution-events-table"><thead><tr><th scope="col">Time</th><th scope="col">Event</th><th scope="col">Details</th></tr></thead><tbody>'+(events.length?events.map(renderTaskExecutionEvent).join(""):'<tr><td colspan="3" class="hint">No events recorded.</td></tr>')+'</tbody></table></div>';
 const usageContent=usage.length
  ?'<pre>'+escapeHtml(JSON.stringify(usage,null,2))+'</pre>'
  :'<p class="hint">No model usage was recorded.</p>';
 const sections=[
  {id:"output",label:"Final output",count:null,content:output+failure},
  {id:"prompt",label:"Submitted prompt",count:null,content:input},
  {id:"timeline",label:"Execution timeline",count:events.length,content:timeline},
  {id:"usage",label:"Model usage",count:usage.length,content:usageContent}
 ];
 return '<div class="task-execution-detail"><div class="hint task-execution-run-meta">Agent: '+escapeHtml(execution.agent||"")+' · Run: '+escapeHtml(execution.run_id||"")+'</div><nav class="task-execution-section-nav" aria-label="Execution detail sections" role="tablist">'+sections.map((section,index)=>'<button type="button" class="task-execution-section-tab'+(index===0?' active':'')+'" id="task-execution-tab-'+section.id+'" role="tab" aria-selected="'+(index===0?'true':'false')+'" aria-controls="task-execution-panel-'+section.id+'" tabindex="'+(index===0?'0':'-1')+'" data-execution-section="'+section.id+'">'+section.label+(section.count===null?'':' <span>'+section.count+'</span>')+'</button>').join("")+'</nav>'+sections.map((section,index)=>'<section class="task-execution-section" id="task-execution-panel-'+section.id+'" role="tabpanel" aria-labelledby="task-execution-tab-'+section.id+'" '+(index===0?'':'hidden')+'><h4>'+section.label+'</h4>'+section.content+'</section>').join("")+'</div>';
}
function renderTaskExecutions(){
 const target=$("task-executions-list");if(!target)return;
 const query=$("task-executions-search")?.value.trim().toLocaleLowerCase()||"";
 const matches=state.taskExecutions.map((execution,index)=>({execution,index})).filter(({execution})=>{
  const created=Number(execution.created_at_unix_ms),date=Number.isFinite(created)?new Date(created).toLocaleString():"Unknown date";
  return [execution.status,date,execution.agent,execution.id].some(value=>String(value||"").toLocaleLowerCase().includes(query));
 });
 const visible=matches.slice(0,25);
 if(!visible.length){
  target.innerHTML='<div class="empty">'+(state.taskExecutions.length?'No executions match this filter. <button type="button" class="secondary" data-clear-execution-filter>Clear filter</button>':'No executions recorded for this task yet.')+'</div>';
  return;
 }
 target.innerHTML='<div class="task-executions-table-wrap"><table class="task-executions-table"><thead><tr><th scope="col">Started</th><th scope="col">Status</th><th scope="col">Duration</th></tr></thead><tbody>'+visible.map(({execution,index})=>{
  const created=Number(execution.created_at_unix_ms);
  const date=Number.isFinite(created)?new Date(created).toLocaleString():"Unknown date";
  const duration=Number.isFinite(execution.duration_ms)?(execution.duration_ms<1000?execution.duration_ms+" ms":(execution.duration_ms/1000).toFixed(1)+" s"):"Still running";
  const status=String(execution.status||"unknown").replaceAll("_"," ");
  return '<tr data-execution-index="'+index+'" tabindex="0" role="button" aria-label="Execution on '+escapeHtml(date)+', '+escapeHtml(status)+'. Open details"><td>'+escapeHtml(date)+'</td><td><span class="task-execution-status">'+escapeHtml(status)+'</span></td><td>'+escapeHtml(duration)+'</td></tr>';
 }).join("")+'</tbody></table></div>'+(matches.length>visible.length?'<p class="hint">Showing the latest 25 of '+matches.length+' matching executions.</p>':"");
}
async function loadTaskExecutions(taskId){
 const panel=$("task-executions"),target=$("task-executions-list");
 if(!panel||!target||!taskId||taskId==="new")return;
 target.innerHTML='<div class="empty" role="status">Loading execution history…</div>';
 const refresh=$("task-executions-refresh");if(refresh)refresh.disabled=true;
 try{
  const executions=await api("/tasks/"+encodeURIComponent(taskId)+"/executions");
  if(state.selectedTask!==taskId)return;
  state.taskExecutions=executions;renderTaskExecutions();
 }catch(error){
  if(state.selectedTask===taskId)target.innerHTML='<div class="empty" role="alert">Unable to load execution history: '+escapeHtml(error.message)+'<p><button type="button" class="secondary" data-retry-executions>Try again</button></p></div>';
 }finally{if(refresh)refresh.disabled=false}
}
$("task-executions-refresh")?.addEventListener("click",()=>{
 if(state.selectedTask&&state.selectedTask!=="new")void loadTaskExecutions(state.selectedTask);
});
function openTaskExecution(index){
 const execution=state.taskExecutions[index];
 if(!execution)return;
 const created=Number(execution.created_at_unix_ms);
 const date=Number.isFinite(created)?new Date(created).toLocaleString():"Unknown date";
 const status=String(execution.status||"unknown").replaceAll("_"," ");
 $("task-execution-dialog-meta").textContent=date+" · "+status+" · "+(execution.agent||"");
 $("task-execution-detail").innerHTML=renderTaskExecution(execution);
 $("task-execution-dialog").showModal();
}
$("task-executions-search")?.addEventListener("input",renderTaskExecutions);
$("task-executions-list")?.addEventListener("click",event=>{
 if(event.target.closest("[data-retry-executions]")){if(state.selectedTask)loadTaskExecutions(state.selectedTask);return}
 if(event.target.closest("[data-clear-execution-filter]")){$("task-executions-search").value="";renderTaskExecutions();return}
 const row=event.target.closest("tr[data-execution-index]");
 if(row)openTaskExecution(Number(row.dataset.executionIndex));
});
$("task-executions-list")?.addEventListener("keydown",event=>{
 const row=event.target.closest("tr[data-execution-index]");
 if(!row||!(event.key==="Enter"||event.key===" "))return;
 event.preventDefault();
 openTaskExecution(Number(row.dataset.executionIndex));
});
$("task-execution-detail")?.addEventListener("click",event=>{
 const tab=event.target.closest("[data-execution-section]");
 if(!tab)return;
 activateTaskExecutionSection(tab.dataset.executionSection);
});
function activateTaskExecutionSection(section){
 const root=$("task-execution-detail");
 const tabs=[...root.querySelectorAll("[data-execution-section]")];
 const selected=tabs.find(tab=>tab.dataset.executionSection===section);
 if(!selected)return;
 for(const tab of tabs){
  const active=tab===selected;
  tab.classList.toggle("active",active);
  tab.setAttribute("aria-selected",String(active));
  tab.tabIndex=active?0:-1;
  const panel=$(tab.getAttribute("aria-controls"));
  if(panel)panel.hidden=!active;
 }
 selected.focus();
}
$("task-execution-detail")?.addEventListener("keydown",event=>{
 const tab=event.target.closest("[data-execution-section]");
 if(!tab)return;
 const tabs=[...$("task-execution-detail").querySelectorAll("[data-execution-section]")];
 const index=tabs.indexOf(tab);
 let next=index;
 if(event.key==="ArrowRight")next=(index+1)%tabs.length;
 else if(event.key==="ArrowLeft")next=(index-1+tabs.length)%tabs.length;
 else if(event.key==="Home")next=0;
 else if(event.key==="End")next=tabs.length-1;
 else return;
 event.preventDefault();
 activateTaskExecutionSection(tabs[next].dataset.executionSection);
});
$("task-execution-dialog-close")?.addEventListener("click",()=>$("task-execution-dialog").close());
$("task-execution-dialog")?.addEventListener("click",event=>{
 if(event.target===$("task-execution-dialog"))$("task-execution-dialog").close();
});
