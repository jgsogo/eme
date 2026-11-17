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

function renderFamilyMembers(){
 const target=$("family-members"),details=state.familyDetails;
 $("rename-family").disabled=!details||!["owner","admin"].includes(currentFamilyMember()?.role.toLowerCase());
 if(!details){target.textContent="Select a family to see its members.";renderFamilyLifecycle();return}
 const members=details.members||[];
 target.innerHTML='<div style="font-weight:600;margin:8px 0">'+escapeHtml(details.family.name)+' · '+members.length+' members</div>'+ (members.length?members.map(m=>{
  const role=String(m.role).toLowerCase(),actorRole=currentFamilyMember()?.role.toLowerCase();
  const isSelf=m.user_id===state.currentUserId,isOwner=actorRole==="owner",isAdmin=actorRole==="admin";
  const roleControl=!isSelf&&isOwner?'<label>Role <select data-member-role="'+escapeHtml(m.user_id)+'" style="width:auto;min-width:90px;margin:4px 4px 4px 0;padding:5px"><option value="member" '+(role==="member"?"selected":"")+'>Member</option><option value="admin" '+(role==="admin"?"selected":"")+'>Admin</option></select></label><button type="button" class="secondary" data-save-role="'+escapeHtml(m.user_id)+'">Save role</button>':(!isSelf&&isAdmin&&role==="member"?'<button type="button" class="secondary" data-promote-member="'+escapeHtml(m.user_id)+'">Promote to admin</button>':'');
  const transfer=isOwner&&!isSelf?'<button type="button" class="secondary" data-transfer-owner="'+escapeHtml(m.user_id)+'">Make owner</button>':'';
  const remove=!isSelf&&(isOwner||(isAdmin&&role==="member"))?'<button type="button" class="danger" data-remove-member="'+escapeHtml(m.user_id)+'">Remove</button>':'';
  const actions=roleControl||transfer||remove?'<div class="member-actions">'+roleControl+transfer+remove+'</div>':'';
  return '<div class="family-member"><strong>'+escapeHtml(m.user_id)+'</strong><span>'+escapeHtml(m.relationship)+' · '+escapeHtml(m.role)+'</span>'+actions+'</div>';
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

function loadFamilyInvitations(familyId){
 const member=currentFamilyMember();
 if(!familyId||!member||!["owner","admin"].includes(member.role.toLowerCase())){state.familyInvitations=[];renderFamilyInvitations();return}
 try{state.familyInvitations=await api("/families/"+encodeURIComponent(familyId)+"/invitations");renderFamilyInvitations()}
 catch(e){state.familyInvitations=[];renderFamilyInvitations();setStatus(e.message,"error")}
}

function createInvitation(){
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

function revokeInvitation(invitationId){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Revoke this invitation? The link will stop working."))return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/invitations/"+encodeURIComponent(invitationId),{method:"DELETE"});await loadFamilyInvitations(family.id);setStatus("Invitation revoked","ok")}
 catch(e){setStatus(e.message,"error")}
}

function changeMemberRole(userId,roleOverride=null){
 const family=state.familyDetails?.family;if(!family)return;
 const select=document.querySelector('[data-member-role="'+CSS.escape(userId)+'"]');if(!select&&!roleOverride)return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/members/"+encodeURIComponent(userId)+"/role",{method:"PUT",body:JSON.stringify({role:roleOverride||select.value})});await loadFamilyDetails(family.id);setStatus("Member role updated","ok")}
 catch(e){setStatus(e.message,"error")}
}

function removeFamilyMember(userId){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Remove this member from "+family.name+"? Their EME user account will not be deleted."))return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/members/"+encodeURIComponent(userId),{method:"DELETE"});await loadFamilyDetails(family.id);setStatus("Member removed","ok")}
 catch(e){setStatus(e.message,"error")}
}

function transferFamilyOwnership(userId){
 const family=state.familyDetails?.family;if(!family)return;
 if(!window.confirm("Transfer ownership of "+family.name+" to "+userId+"? You will become an admin."))return;
 try{await api("/families/"+encodeURIComponent(family.id)+"/transfer-ownership",{method:"POST",body:JSON.stringify({new_owner_user_id:userId})});await loadFamilyDetails(family.id);setStatus("Family ownership transferred","ok")}
 catch(e){setStatus(e.message,"error")}
}

function leaveFamily(){
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

function deleteFamily(){
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

function loadFamilyDetails(familyId){
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

function createFamily(){
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

function renameFamily(){
 if(!state.familyDetails)return;
 const current=state.familyDetails.family;
 const name=window.prompt("Family name",current.name);if(name===null)return;
 if(!name.trim()){setStatus("Family name must not be empty","error");return}
 try{await api("/families/"+encodeURIComponent(current.id),{method:"PUT",body:JSON.stringify({name:name.trim()})});await loadFamilies();await loadFamilyDetails(current.id);setStatus("Family renamed","ok")}
 catch(e){setStatus(e.message,"error")}
}
async function initializeFamilies(){
 await loadFamilies();
 await loadFamilyDetails(state.families[0]?.id||null);
}
on("families-list","click",e=>{const item=e.target.closest("button[data-family-id]");if(item)loadFamilyDetails(item.dataset.familyId)});
on("create-family","click",createFamily);
on("leave-family","click",leaveFamily);
on("delete-family","click",deleteFamily);
on("family-members","click",e=>{const role=e.target.closest("button[data-save-role]"),promote=e.target.closest("button[data-promote-member]"),remove=e.target.closest("button[data-remove-member]"),transfer=e.target.closest("button[data-transfer-owner]");if(role)changeMemberRole(role.dataset.saveRole);else if(promote)changeMemberRole(promote.dataset.promoteMember,"admin");else if(remove)removeFamilyMember(remove.dataset.removeMember);else if(transfer)transferFamilyOwnership(transfer.dataset.transferOwner)});
on("rename-family","click",renameFamily);
on("create-invitation","click",createInvitation);
on("family-invitations","click",e=>{const button=e.target.closest("button[data-revoke-invitation]");if(button)revokeInvitation(button.dataset.revokeInvitation)});
