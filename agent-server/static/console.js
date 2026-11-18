const TERMINAL=new Set(["completed","failed","cancelled","timed_out","max_iterations_reached"]);
const page=document.body.dataset.page;
const consoleBase=document.body.dataset.layout==="mobile"?"/mobile":"/console";
const pendingInviteMatch=window.location.pathname.match(/^\/invite\/([a-fA-F0-9]{64})$/);
let pendingInviteToken=pendingInviteMatch?pendingInviteMatch[1]:null;
const state={googleConnections:[],googleCalendars:{},agents:[],families:[],familyDetails:null,familyInvitations:[],conversations:[],notes:[],selectedNote:null,tasks:[],selectedTask:null,selected:null,selectedFamilyId:null,managedFamilyId:null,currentUserId:null,messageCount:0,runId:null,pollTimer:null};
const $=id=>document.getElementById(id);
const on=(id,event,handler)=>$(id)?.addEventListener(event,handler);
async function api(path,options={}){const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});const raw=await response.text();let body=null;try{body=raw?JSON.parse(raw):null}catch{body=raw}if(!response.ok)throw new Error(typeof body==="string"?body:body?.error||response.statusText);return body}
function escapeHtml(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function setStatus(message,kind=""){const node=$("run-status")||$("connection");if(node){node.textContent=message;node.className=(node.id==="run-status"?"run-status ":"hint ")+kind}}
function renderAgents(){const previous=$("agent").value;$("agent").innerHTML=state.agents.map(a=>'<option value="'+escapeHtml(a.name)+'">'+escapeHtml(a.name)+'</option>').join("");if(state.agents.some(a=>a.name===previous))$("agent").value=previous;updateAgentDescription()}
function updateAgentDescription(){const a=state.agents.find(x=>x.name===$("agent").value);$("agent-desc").textContent=a?[(a.capabilities||[]).join(", "),(a.tools||[]).length+" tools",a.description].filter(Boolean).join(" · "):""}
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
async function loadConversations(){state.conversations=await api("/conversations");renderConversations()}
async function loadFamilies(){const [families,current]=await Promise.all([api("/families"),api("/families/current-user")]);state.families=families;state.currentUserId=current.user_id;renderFamilySelector();renderFamiliesList();renderNoteFamilies()}
function renderNotes(){
 const target=$("notes-list");if(!target)return;
 target.innerHTML=state.notes.length?state.notes.map(note=>'<button type="button" class="family-list-item'+(note.id===state.selectedNote?" active":"")+'" data-note-id="'+escapeHtml(note.id)+'"><strong>'+escapeHtml(note.title)+'</strong><span>'+(note.family_id?"Shared with "+escapeHtml(state.families.find(f=>f.id===note.family_id)?.name||"a family"):"Private")+' · '+escapeHtml(new Date(note.updated_at).toLocaleDateString())+'</span></button>').join(""):'<div class="empty">No notes yet. Create one to get started.</div>';
}
function resetNoteEditor(){
 state.selectedNote=null;renderNotes();
 if($("note-heading"))$("note-heading").textContent="Select a note";
 $("note-title").value="";$("note-content").value="";$("note-family").value="";
 for(const id of ["note-title","note-content","note-family","save-note"])$(id).disabled=true;
 $("delete-note").disabled=true;$("notes-status").textContent="Choose a note or create a new one. Notes are private by default.";
}
function startNewNote(){
 state.selectedNote="new";renderNotes();
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
  if($("note-heading"))$("note-heading").textContent=full.title;
  $("note-title").value=full.title;$("note-content").value=full.content;$("note-family").value=full.family_id||"";
  const editable=full.owner_id===state.currentUserId;
  for(const field of ["note-title","note-content","note-family","save-note"])$(field).disabled=!editable;
  $("delete-note").disabled=!editable;
  $("notes-status").textContent=!editable?"Shared note · read-only":(full.family_id?"Shared with "+(state.families.find(f=>f.id===full.family_id)?.name||"a family"):"Private note");
 }catch(e){$("notes-status").textContent=e.message}
}
function renderNoteFamilies(){
 const select=$("note-family");if(!select)return;const selected=select.value;
 select.innerHTML='<option value="">Private — only me</option>'+state.families.map(f=>'<option value="'+escapeHtml(f.id)+'">'+escapeHtml(f.name)+'</option>').join("");
 if([...select.options].some(option=>option.value===selected))select.value=selected;
}
async function loadNotes(){
 try{state.notes=await api("/notes");renderNotes();if(state.selectedNote&&!state.notes.some(n=>n.id===state.selectedNote))resetNoteEditor()}
 catch(e){$("notes-list").innerHTML='<div class="hint">Unable to load notes: '+escapeHtml(e.message)+'</div>'}
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
 if(!state.families.length){target.innerHTML='<div class="empty">You do not belong to any families yet.</div>';return}
 target.innerHTML=state.families.map(f=>{
  const active=f.id===state.managedFamilyId;
  const count=Number.isInteger(f.member_count)?f.member_count:(state.familyDetails?.family?.id===f.id?state.familyDetails.members?.length:null);
  const role=state.familyDetails?.family?.id===f.id?currentFamilyMember()?.role:null;
  return '<button type="button" class="family-list-item'+(active?' active':'')+'" data-family-id="'+escapeHtml(f.id)+'" aria-pressed="'+String(active)+'"><strong>'+escapeHtml(f.name)+'</strong><span>'+(count===null?'Open to manage':count+' members')+(role?' · '+escapeHtml(role):'')+'</span></button>';
 }).join("");
}
function memberAge(birthday){if(!birthday)return null;const [year,month,day]=birthday.split("-").map(Number),today=new Date();let age=today.getUTCFullYear()-year;if(today.getUTCMonth()+1<month||(today.getUTCMonth()+1===month&&today.getUTCDate()<day))age--;return age>=0?age:null}
function renderFamilyMembers(){
 const target=$("family-members"),details=state.familyDetails;
 const canManage=!!details&&["owner","admin"].includes(currentFamilyMember()?.role.toLowerCase());
 $("rename-family").disabled=!canManage;$("add-family-member").disabled=!canManage;
 if(!details){target.textContent="Select a family to see its members.";renderFamilyLifecycle();return}
 const members=details.members||[];
 target.innerHTML='<div style="font-weight:600;margin:8px 0">'+escapeHtml(details.family.name)+' · '+members.length+' members</div>'+ (members.length?members.map(m=>{
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
 }).join(""):'<div class="hint">No members.</div>');
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
 const target=$("family-invitations"),family=state.familyDetails?.family,me=currentFamilyMember();
 const canManage=me&&["owner","admin"].includes(me.role.toLowerCase());
 $("create-invitation").disabled=!family||!canManage;
 if(!family){target.textContent="Select a family to manage invitations.";return}
 if(!canManage){target.textContent="Only family owners and admins can manage invitations.";return}
 const items=state.familyInvitations;
 target.innerHTML=items.length?items.map(inv=>{
  const status=invitationState(inv);
  return '<div class="family-invitation"><div><strong>'+escapeHtml(status)+'</strong><small>Expires '+escapeHtml(new Date(inv.expires_at).toLocaleString())+(inv.target_identity?' · '+escapeHtml(inv.target_identity.provider)+': '+escapeHtml(inv.target_identity.subject):'')+'</small></div><div class="family-invitation-actions">'+(status==="Pending"?'<button type="button" class="secondary" data-revoke-invitation="'+escapeHtml(inv.id)+'">Revoke</button>':'')+'</div></div>';
 }).join(""):'<div class="hint" style="padding:8px 0">No invitations yet.</div>';
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
 const days=window.prompt("Invitation validity in days","7");if(days===null)return;
 const validity=Number(days);if(!Number.isInteger(validity)||validity<1||validity>30){setStatus("Choose a validity between 1 and 30 days","error");return}
 const target=window.prompt("Optional Telegram user ID to restrict this invitation (leave empty for anyone)","");
 if(target===null)return;
 const expiresAt=new Date(Date.now()+validity*86400000).toISOString();
 const request={expires_at:expiresAt};
 if(target.trim())request.target_identity={provider:"telegram",subject:target.trim()};
 try{
  const created=await api("/families/"+encodeURIComponent(family.id)+"/invitations",{method:"POST",body:JSON.stringify(request)});
  if(created.lan_url){
   const copy=window.confirm("Invitation created. Copy this LAN invitation link now?\n\n"+created.lan_url);
   if(copy){try{await navigator.clipboard.writeText(created.lan_url);setStatus("LAN invitation link copied","ok")}catch{window.prompt("Copy the LAN invitation link",created.lan_url)}}
  }else{
   window.prompt("Invitation created. Save this one-time token; it will not be shown again.",created.token);
   setStatus("Invitation created; LAN_BASE_URL is not configured, so no LAN link is available","ok");
  }
  await loadFamilyInvitations(family.id);
 }catch(e){setStatus(e.message,"error")}
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
 const name=window.prompt("Family name");if(name===null)return;
 if(!name.trim()){setStatus("Family name must not be empty","error");return}
 const relationship=window.prompt("Your relationship in this family (father, mother, son, daughter, grandfather, grandmother, partner, other)","other");
 if(relationship===null)return;
 try{
  const created=await api("/families",{method:"POST",body:JSON.stringify({name:name.trim(),relationship:relationship.trim().toLowerCase()})});
  await loadFamilies();await loadFamilyDetails(created.id);
  setStatus("Family created","ok");
 }catch(e){setStatus(e.message,"error")}
}
async function renameFamily(){
 if(!state.familyDetails)return;
 const current=state.familyDetails.family;
 const name=window.prompt("Family name",current.name);if(name===null)return;
 if(!name.trim()){setStatus("Family name must not be empty","error");return}
 try{await api("/families/"+encodeURIComponent(current.id),{method:"PUT",body:JSON.stringify({name:name.trim()})});await loadFamilies();await loadFamilyDetails(current.id);setStatus("Family renamed","ok")}
 catch(e){setStatus(e.message,"error")}
}

async function selectConversation(id){state.selected=id;renderConversations();$("messages").innerHTML='<div class="empty">Loading conversation…</div>';try{const c=await api("/conversations/"+encodeURIComponent(id));$("conversation-title").textContent=c.title||"New conversation";$("conversation-meta").textContent="Agent · "+c.agent+" · Conversation "+c.id;$("rename-conversation").disabled=false;$("agent").value=c.agent;updateAgentDescription();state.selectedFamilyId=c.selected_family_id||null;renderFamilySelector();state.messageCount=(c.messages||[]).length;renderMessages(c.messages);setComposerEnabled(true);setStatus("")}catch(e){setStatus(e.message,"error")}}
async function createConversation(){const agent=$("agent").value;if(!agent)return;try{const c=await api("/conversations",{method:"POST",body:JSON.stringify({agent})});await loadConversations();await selectConversation(c.id);$("prompt").focus()}catch(e){setStatus(e.message,"error")}}
async function renameConversation(){if(!state.selected)return;const current=state.conversations.find(c=>c.id===state.selected);if(!current)return;const title=window.prompt("Conversation name",current.title||"New conversation");if(title===null)return;if(!title.trim()){setStatus("Conversation name must not be empty","error");return}try{const updated=await api("/conversations/"+encodeURIComponent(state.selected),{method:"PUT",body:JSON.stringify({title})});state.conversations=state.conversations.map(c=>c.id===updated.id?{...c,...updated}:c);$("conversation-title").textContent=updated.title;$("conversation-meta").textContent="Agent · "+updated.agent+" · Conversation "+updated.id;renderConversations();setStatus("Conversation renamed","ok")}catch(e){setStatus(e.message,"error")}}
function clearSelectedConversation(){state.selected=null;state.selectedFamilyId=null;renderFamilySelector();state.messageCount=0;$("conversation-title").textContent="No conversation selected";$("conversation-meta").textContent="Create a conversation to get started";$("rename-conversation").disabled=true;$("messages").innerHTML='<div class="empty">Choose an existing conversation or create a new one.</div>';$("prompt").value="";setComposerEnabled(false);renderConversations()}
async function deleteConversation(id){const conversation=state.conversations.find(c=>c.id===id);if(!conversation)return;const title=conversation.title||"New conversation";if(!window.confirm('Permanently delete "'+title+'"? This cannot be undone.'))return;try{await api("/conversations/"+encodeURIComponent(id),{method:"DELETE"});state.conversations=state.conversations.filter(c=>c.id!==id);if(state.selected===id)clearSelectedConversation();else renderConversations();setStatus("Conversation deleted","ok")}catch(e){setStatus(e.message,"error")}}
async function sendMessage(){const input=$("prompt").value.trim();if(!input||!state.selected||state.runId)return;const id=state.selected,previousMessageCount=state.messageCount;$("prompt").value="";setComposerEnabled(false);setStatus("Queued…");try{const result=await api("/conversations/"+encodeURIComponent(id)+"/runs",{method:"POST",body:JSON.stringify({input})});state.runId=result.run_id;$("cancel-run").hidden=false;setStatus("Running · "+result.run_id.slice(0,8));await pollRun(id,result.run_id,previousMessageCount)}catch(e){setStatus(e.message,"error");setComposerEnabled(true);$("prompt").value=input}}
async function pollRun(conversationId,runId,previousMessageCount){clearInterval(state.pollTimer);renderRunEvents([]);const poll=async()=>{try{const [run,events]=await Promise.all([api("/runs/"+encodeURIComponent(runId)),api("/runs/"+encodeURIComponent(runId)+"/events")]);renderRunEvents(events);if(!TERMINAL.has(run.status)){setStatus("Running · "+run.status);return}clearInterval(state.pollTimer);state.runId=null;$("cancel-run").hidden=true;let conversation=null;for(let attempt=0;attempt<40;attempt++){conversation=await api("/conversations/"+encodeURIComponent(conversationId));if((conversation.messages||[]).length>previousMessageCount)break;await new Promise(resolve=>setTimeout(resolve,100))}if(state.selected===conversationId){$("conversation-title").textContent=conversation.title||"New conversation";$("conversation-meta").textContent="Agent · "+conversation.agent+" · Conversation "+conversation.id;state.messageCount=(conversation.messages||[]).length;renderMessages(conversation.messages);setComposerEnabled(true)}await loadConversations();if(run.status==="completed")setStatus("Completed","ok");else setStatus(run.error||run.status,"error");setComposerEnabled(state.selected===conversationId);renderFamilySelector()}catch(e){clearInterval(state.pollTimer);state.runId=null;$("cancel-run").hidden=true;setStatus("Unable to retrieve run: "+e.message,"error");setComposerEnabled(true);renderFamilySelector()}};await poll();if(state.runId)state.pollTimer=setInterval(poll,1000)}
on("families-list","click",e=>{const item=e.target.closest("button[data-family-id]");if(item)loadFamilyDetails(item.dataset.familyId)});
on("new-task","click",startNewTask);on("refresh-tasks","click",loadTasks);on("tasks-list","click",e=>{const button=e.target.closest("button[data-task-id]");if(button)selectTask(button.dataset.taskId)});on("task-form","submit",saveTask);on("delete-task","click",deleteTask);on("notes-list","click",e=>{const button=e.target.closest("button[data-note-id]");if(button)selectNote(button.dataset.noteId)});on("new-note","click",startNewNote);on("refresh-notes","click",loadNotes);on("note-form","submit",saveNote);on("delete-note","click",deleteNote);
on("create-family","click",createFamily);on("add-family-member","click",addManagedFamilyMember);on("leave-family","click",leaveFamily);on("delete-family","click",deleteFamily);on("family-members","click",e=>{const role=e.target.closest("button[data-save-role]"),promote=e.target.closest("button[data-promote-member]"),remove=e.target.closest("button[data-remove-member]"),transfer=e.target.closest("button[data-transfer-owner]"),edit=e.target.closest("button[data-edit-member-profile]");if(edit)editManagedFamilyMember(edit.dataset.editMemberProfile);else if(role)changeMemberRole(role.dataset.saveRole);else if(promote)changeMemberRole(promote.dataset.promoteMember,"admin");else if(remove)removeFamilyMember(remove.dataset.removeMember);else if(transfer)transferFamilyOwnership(transfer.dataset.transferOwner)});on("rename-family","click",renameFamily);on("create-invitation","click",createInvitation);on("family-invitations","click",e=>{const button=e.target.closest("button[data-revoke-invitation]");if(button)revokeInvitation(button.dataset.revokeInvitation)});on("family-selector","change",async()=>{if(!state.selected)return;const selector=$("family-selector"),previous=state.selectedFamilyId;selector.disabled=true;try{const updated=await api("/conversations/"+encodeURIComponent(state.selected)+"/family",{method:"PUT",body:JSON.stringify({family_id:selector.value||null})});state.selectedFamilyId=updated.selected_family_id||null;selector.value=state.selectedFamilyId||"";setStatus(state.selectedFamilyId?"Family context updated":"Family context cleared","ok")}catch(e){selector.value=previous||"";setStatus(e.message,"error")}finally{selector.disabled=!state.selected||!!state.runId}});on("rename-conversation","click",renameConversation);on("cancel-run","click",async()=>{if(!state.runId)return;try{await api("/runs/"+encodeURIComponent(state.runId)+"/cancel",{method:"POST"});setStatus("Cancellation requested")}catch(e){setStatus(e.message,"error")}});on("agent","change",updateAgentDescription);on("new-conversation","click",createConversation);on("conversations","click",e=>{const deleteButton=e.target.closest("button[data-delete-id]");if(deleteButton){e.stopPropagation();if(!state.runId)deleteConversation(deleteButton.dataset.deleteId);return}const item=e.target.closest("li[data-id]");if(item&&!state.runId)selectConversation(item.dataset.id)});on("composer","submit",e=>{e.preventDefault();sendMessage()});on("prompt","keydown",e=>{if(e.key==="Enter"&&!e.shiftKey){e.preventDefault();$("composer").requestSubmit()}}); 
async function loadTasks(){
 try{state.tasks=await api("/tasks");renderTasks()}
 catch(e){$("tasks-list").innerHTML='<div class="empty">Unable to load tasks: '+escapeHtml(e.message)+'</div>'}
}
function renderTasks(){
 $("tasks-list").innerHTML=state.tasks.length?state.tasks.map(task=>'<button type="button" class="family-list-item '+(task.id===state.selectedTask?"active":"")+'" data-task-id="'+escapeHtml(task.id)+'"><strong>'+escapeHtml(task.name)+'</strong><span>'+(task.enabled?"Enabled":"Disabled")+' · '+escapeHtml(task.schedule||"One-time")+(task.next_run?' · Next '+escapeHtml(new Date(task.next_run).toLocaleString()):'')+'</span></button>').join(""):'<div class="empty">No tasks yet. Create one to get started.</div>';
}
function renderTaskAgents(){
 const select=$("task-agent"),selected=select.value;
 select.innerHTML=state.agents.map(agent=>'<option value="'+escapeHtml(agent.name)+'">'+escapeHtml(agent.name)+'</option>').join("");
 if(state.agents.some(agent=>agent.name===selected))select.value=selected;
}
function resetTaskEditor(){
 state.selectedTask=null;renderTasks();
 for(const id of ["task-name","task-description","task-agent","task-prompt","task-schedule","task-at","task-timezone","task-enabled","save-task"])$(id).disabled=true;
 $("task-heading").textContent="Select a task";
 $("task-name").value="";$("task-description").value="";$("task-prompt").value="";$("task-schedule").value="";$("task-at").value="";$("task-timezone").value="Europe/Madrid";$("task-enabled").checked=true;
 $("delete-task").disabled=true;$("tasks-status").textContent="Choose a task or create a new one.";
}
function startNewTask(){
 state.selectedTask="new";renderTasks();
 for(const id of ["task-name","task-description","task-agent","task-prompt","task-schedule","task-at","task-timezone","task-enabled","save-task"])$(id).disabled=false;
 $("task-heading").textContent="New task";
 $("delete-task").disabled=true;$("task-name").value="";$("task-description").value="";$("task-prompt").value="";$("task-schedule").value="";$("task-at").value="";$("task-timezone").value="Europe/Madrid";$("task-enabled").checked=true;
 renderTaskAgents();$("tasks-status").textContent="Set either a cron schedule or a one-time execution date, not both.";$("task-name").focus();
}
function toLocalDateTime(value){const date=new Date(value);return new Date(date.getTime()-date.getTimezoneOffset()*60000).toISOString().slice(0,16)}
function selectTask(id){
 const task=state.tasks.find(item=>item.id===id);if(!task)return;
 state.selectedTask=id;renderTasks();
 for(const field of ["task-name","task-description","task-agent","task-prompt","task-schedule","task-at","task-timezone","task-enabled","save-task"])$(field).disabled=false;
 $("delete-task").disabled=false;$("task-heading").textContent=task.name;
 renderTaskAgents();$("task-name").value=task.name;$("task-description").value=task.description||"";$("task-agent").value=task.agent;$("task-prompt").value=task.prompt;
 $("task-schedule").value=task.schedule||"";
 $("task-at").value=task.at?toLocalDateTime(task.at):"";
 $("task-timezone").value=task.timezone||"Europe/Madrid";$("task-enabled").checked=!!task.enabled;
 $("tasks-status").textContent=task.enabled?"Task is enabled.":"Task is disabled.";
}
async function saveTask(event){
 event.preventDefault();
 const schedule=$("task-schedule").value.trim(),atValue=$("task-at").value;
 if(Boolean(schedule)===Boolean(atValue)){$("tasks-status").textContent="Specify either a cron schedule or a one-time date, but not both.";return}
 const body={name:$("task-name").value.trim(),description:$("task-description").value.trim(),agent:$("task-agent").value,prompt:$("task-prompt").value,schedule:schedule||null,at:atValue?new Date(atValue).toISOString():null,timezone:schedule?($("task-timezone").value.trim()||"UTC"):null,enabled:$("task-enabled").checked};
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
 try{const profile=await api("/profile");$("profile-display-name").value=profile.display_name||"";$("profile-status").textContent=profile.display_name?"Your profile is up to date.":"Choose the name agents should use for you."}
 catch(e){$("profile-status").textContent=e.message}
}
async function saveProfile(event){
 event.preventDefault();const name=$("profile-display-name").value.trim();if(!name){$("profile-status").textContent="Display name must not be empty.";return}
 $("save-profile").disabled=true;
 try{const profile=await api("/profile",{method:"PUT",body:JSON.stringify({display_name:name})});$("profile-display-name").value=profile.display_name;$("profile-status").textContent="Profile saved."}
 catch(e){$("profile-status").textContent=e.message}
 finally{$("save-profile").disabled=false}
}
on("profile-form","submit",saveProfile);
function googleHasWriteScope(connection){return (connection.scopes||[]).some(scope=>scope==="https://www.googleapis.com/auth/calendar.events"||scope==="https://www.googleapis.com/auth/calendar")}
function googleHasReadScope(connection){return (connection.scopes||[]).some(scope=>scope==="https://www.googleapis.com/auth/calendar.events.readonly"||scope==="https://www.googleapis.com/auth/calendar.events"||scope==="https://www.googleapis.com/auth/calendar")}
function renderGoogleConnections(){
 const target=$("google-connections");if(!target)return;
 if(!state.googleConnections.length){target.innerHTML='<div class="panel integrations-empty"><div class="empty">No Google accounts connected yet.</div></div>';return}
 target.innerHTML=state.googleConnections.map(connection=>{
  const calendars=state.googleCalendars[connection.id]||[],writeScope=googleHasWriteScope(connection),status=connection.status==="active"?"Active":"Disconnected";
  return '<section class="panel google-account" data-connection-id="'+escapeHtml(connection.id)+'"><div class="google-account-heading"><div><h3>'+escapeHtml(connection.email||"Google account")+'</h3><p class="hint">'+status+' · '+(googleHasReadScope(connection)?"Calendar read access":"No calendar read scope")+' · '+(writeScope?"Google write scope granted":"Read-only Google scope")+'</p></div><button type="button" class="danger" data-google-action="disconnect" data-connection-id="'+escapeHtml(connection.id)+'" '+(connection.status!=="active"?"disabled":"")+'>Disconnect</button></div><div class="calendar-toolbar"><div><h4>Calendars</h4><p class="hint">New calendars are not selected automatically.</p></div><button type="button" class="secondary" data-google-action="sync" data-connection-id="'+escapeHtml(connection.id)+'" '+(connection.status!=="active"?"disabled":"")+'>Sync calendars</button></div>'+(connection.status!=="active"?'<p class="hint">Reconnect this Google account to restore access.</p>':calendars.length?calendars.map(calendar=>renderGoogleCalendar(connection,calendar,writeScope)).join(""):'<p class="hint">No calendars discovered yet. Select “Sync calendars” to load them from Google.</p>')+'</section>'
 }).join("");
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
async function loadIntegrations(){
 const status=$("integrations-status");
 try{await loadFamilies();await loadGoogleConnections();status.textContent="Google account settings loaded."}
 catch(e){status.textContent=e.message.includes("Google OAuth is not configured")?"Google Calendar integration is not configured on this server.":e.message;$("google-connections").innerHTML='<div class="panel integrations-empty"><div class="empty">'+escapeHtml(status.textContent)+'</div></div>'}
}
async function connectGoogle(){
 const button=$("google-connect"),status=$("integrations-status"),access=$("google-connect-access").value,popup=window.open("about:blank","eme-google-oauth","popup,width=620,height=760");
 if(!popup){status.textContent="Allow pop-ups for this site to connect a Google account.";return}
 button.disabled=true;status.textContent="Preparing Google authorization…";
 try{
  const before=state.googleConnections.map(c=>c.id+":"+c.status+":"+c.updated_at+":"+(c.scopes||[]).join(",")).join("|"),auth=await api("/integrations/google/oauth/start",{method:"POST",body:JSON.stringify({access})});
  popup.location.href=auth.authorization_url;status.textContent="Complete authorization in the Google window. This page will update automatically.";
  const deadline=Date.now()+auth.expires_in_seconds*1000;
  while(Date.now()<deadline){
   await new Promise(resolve=>setTimeout(resolve,1500));
   const connections=await api("/integrations/google/connections"),after=connections.map(c=>c.id+":"+c.status+":"+c.updated_at+":"+(c.scopes||[]).join(",")).join("|");
   if(after!==before){await loadGoogleConnections();status.textContent="Google account connected.";if(!popup.closed)popup.close();return}
   if(popup.closed)break;
  }
  status.textContent="Authorization was not completed. You can try again.";
 }catch(e){status.textContent=e.message}
 finally{if(!popup.closed)popup.close();button.disabled=false}
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
on("refresh-integrations","click",loadIntegrations);
on("google-connections","click",handleGoogleIntegrationAction);
on("google-connections","change",handleGoogleCalendarChange);

async function showConsole(){
 document.body.classList.remove("login-active");
 $("login-screen").hidden=true;$("invite-screen").hidden=true;$("logout").hidden=false;
 if(pendingInviteToken){document.querySelector("main").hidden=true;$("invite-screen").hidden=false;$("connection").textContent="Accept invitation";return}
 document.querySelector("main").hidden=false;
 if(page==="conversations"){
  state.agents=await api("/agents");renderAgents();await Promise.all([loadFamilies(),loadConversations()]);
  if(state.conversations.length)await selectConversation(state.conversations[0].id);else renderFamilySelector();
 }else if(page==="families"){
  await loadFamilies();
  await loadFamilyDetails(state.families[0]?.id||null);
 }else if(page==="tasks"){
  state.agents=await api("/agents");renderTaskAgents();await loadTasks();resetTaskEditor();
 }else if(page==="profile"){
  await loadProfile();
 }else if(page==="integrations"){
  await loadIntegrations();
 }else{
  await loadFamilies();await loadNotes();resetNoteEditor();
 }
 $("connection").textContent="Connected";
}
async function showLogin(){
 document.body.classList.add("login-active");
 if($("rename-conversation"))$("rename-conversation").disabled=true;$("invite-screen").hidden=true;
 document.querySelector("main").hidden=true;$("login-screen").hidden=false;$("logout").hidden=true;$("connection").textContent="Sign in";
 const title=$("login-screen").querySelector("h2"),description=$("login-screen").querySelector("p");
 title.textContent=pendingInviteToken?"Sign in to accept invitation":"Sign in with Telegram";
 description.textContent=pendingInviteToken?"Sign in with Telegram to accept your family invitation.":"Use your Telegram account to sign in to the EME web console."
}
on("login-start","click",async()=>{
 const popup=window.open("about:blank","_blank");
 const status=$("login-status"),link=$("login-link");$("login-start").disabled=true;status.textContent="Creating a one-time login link…";link.hidden=true;
 try{
  const login=await api("/auth/telegram/start",{method:"POST"});
  if(!login.bot_url)throw new Error("Set TELEGRAM_BOT_USERNAME on agent-server to enable Telegram login links.");
  link.href=login.bot_url;link.textContent=login.bot_url;link.hidden=false;
  if(popup)popup.location.href=login.bot_url;
  status.textContent="Open the Telegram URL in the new tab and press Start in your private chat with the bot. Waiting for confirmation…";
  const deadline=Date.now()+login.expires_in_seconds*1000;
  while(Date.now()<deadline){
   const result=await api("/auth/telegram/status",{method:"POST",body:JSON.stringify({token:login.token})});
   if(result.authenticated){await showConsole();return}
   await new Promise(resolve=>setTimeout(resolve,1000))
  }
  status.textContent="This login link expired. Please try again."
 }catch(e){if(popup&&!popup.closed)popup.close();status.textContent=e.message}finally{$("login-start").disabled=false}
});
on("invite-accept","click",async()=>{
 if(!pendingInviteToken)return;const button=$("invite-accept"),status=$("invite-status");button.disabled=true;status.textContent="Validating and accepting invitation…";
 try{await api("/invitations/accept",{method:"POST",body:JSON.stringify({token:pendingInviteToken,relationship:$("invite-relationship").value})});pendingInviteToken=null;window.history.replaceState({},"", consoleBase+"/families");$("invite-screen").hidden=true;await showConsole();setStatus("Family invitation accepted","ok")}catch(e){status.textContent=e.message;button.disabled=false}
});
on("refresh","click",async()=>{try{await Promise.all([loadConversations(),loadFamilies()]);if(state.managedFamilyId)await loadFamilyDetails(state.managedFamilyId)}catch(e){setStatus(e.message,"error")}});
on("logout","click",async()=>{
 try{await api("/auth/logout",{method:"POST"})}finally{state.selected=null;state.conversations=[];state.notes=[];state.selectedNote=null;state.tasks=[];state.selectedTask=null;if(page==="conversations")renderConversations();if(page==="notes")renderNotes();if(page==="tasks")renderTasks();await showLogin()}
});
(async()=>{try{await api("/auth/session");await showConsole()}catch(e){await showLogin()}})();
