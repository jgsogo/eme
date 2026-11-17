const page=document.body.dataset.page;
const pendingInviteMatch=window.location.pathname.match(/^\/invite\/([a-fA-F0-9]{64})$/);
let pendingInviteToken=pendingInviteMatch?pendingInviteMatch[1]:null;
const state={agents:[],families:[],familyDetails:null,familyInvitations:[],conversations:[],selected:null,selectedFamilyId:null,managedFamilyId:null,currentUserId:null,messageCount:0,runId:null,pollTimer:null};
const $=id=>document.getElementById(id);
const on=(id,event,handler)=>$(id)?.addEventListener(event,handler);
function api(path,options={}

function setStatus(message,kind=""){const node=$("run-status")||$("connection");if(node){node.textContent=message;node.className=(node.id==="run-status"?"run-status ":"hint ")+kind}}

function loadFamilies(){const [families,current]=await Promise.all([api("/families"),api("/families/current-user")]);state.families=families;state.currentUserId=current.user_id;renderFamilySelector();renderFamiliesList()}

function showConsole(){
 document.body.classList.remove("login-active");
 $("login-screen").hidden=true;$("invite-screen").hidden=true;$("logout").hidden=false;
 if(pendingInviteToken){document.querySelector("main").hidden=true;$("invite-screen").hidden=false;$("connection").textContent="Accept invitation";return}
 document.querySelector("main").hidden=false;
 if(page==="conversations"){
  state.agents=await api("/agents");renderAgents();await Promise.all([loadFamilies(),loadConversations()]);
  if(state.conversations.length)await selectConversation(state.conversations[0].id);else renderFamilySelector();
 }else{
  await loadFamilies();
  await loadFamilyDetails(state.families[0]?.id||null);
 }
 $("connection").textContent="Connected";
}

function showLogin(){
 document.body.classList.add("login-active");
 if($("rename-conversation"))$("rename-conversation").disabled=true;$("invite-screen").hidden=true;
 document.querySelector("main").hidden=true;$("login-screen").hidden=false;$("logout").hidden=true;$("connection").textContent="Sign in";
 const title=$("login-screen").querySelector("h2"),description=$("login-screen").querySelector("p");
 title.textContent=pendingInviteToken?"Sign in to accept invitation":"Sign in with Telegram";
 description.textContent=pendingInviteToken?"Sign in with Telegram to accept your family invitation.":"Use your Telegram account to sign in to the EME web console."
}
function escapeHtml(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]))}
function loadFamilies(){const [families,current]=await Promise.all([api("/families"),api("/families/current-user")]);state.families=families;state.currentUserId=current.user_id;renderFamilySelector();renderFamiliesList()}
function showConsole(){
 document.body.classList.remove("login-active");
 $("login-screen").hidden=true;$("invite-screen").hidden=true;$("logout").hidden=false;
 if(pendingInviteToken){document.querySelector("main").hidden=true;$("invite-screen").hidden=false;$("connection").textContent="Accept invitation";return}
 document.querySelector("main").hidden=false;
 if(page==="conversations"){
  state.agents=await api("/agents");renderAgents();await Promise.all([loadFamilies(),loadConversations()]);
  if(state.conversations.length)await selectConversation(state.conversations[0].id);else renderFamilySelector();
 }else{
  await loadFamilies();
  await loadFamilyDetails(state.families[0]?.id||null);
 }
 $("connection").textContent="Connected";
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
 try{await api("/invitations/accept",{method:"POST",body:JSON.stringify({token:pendingInviteToken,relationship:$("invite-relationship").value})});pendingInviteToken=null;window.history.replaceState({},"", "/console/families");$("invite-screen").hidden=true;await showConsole();setStatus("Family invitation accepted","ok")}catch(e){status.textContent=e.message;button.disabled=false}
});
on("logout","click",async()=>{
 try{await api("/auth/logout",{method:"POST"})}finally{state.selected=null;state.conversations=[];if(page==="conversations")renderConversations();await showLogin()}
});
(async()=>{try{await api("/auth/session");await showConsole()}catch(e){await showLogin()}})();