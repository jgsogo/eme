// Shared authentication and application shell for the Meals views.
const apiAuth=async(path,options={})=>{
 const response=await fetch(path,{...options,headers:{"Content-Type":"application/json",...(options.headers||{})}});
 const raw=await response.text();let body=null;
 try{body=raw?JSON.parse(raw):null}catch{body=raw}
 if(!response.ok)throw new Error(typeof body==="string"?body:body?.error||response.statusText);
 return body;
};
const connection=document.getElementById("connection");
const main=document.querySelector("main");
const header=document.querySelector("header");
const login=document.createElement("section");
login.id="login-screen";login.hidden=true;
login.innerHTML='<div class="login-card"><h2>Sign in with Telegram</h2><p>Use your Telegram account to sign in to the EME web console.</p><button id="login-start" type="button">Continue with Telegram</button><a id="login-link" hidden target="_blank" rel="noopener noreferrer">Open EME bot in Telegram</a><p id="login-status">Your login link will expire after five minutes.</p></div>';
header.insertAdjacentElement("afterend",login);
let resolveReady;
window.emeConsoleReady=new Promise(resolve=>{resolveReady=resolve});
function showLogin(){
 document.body.classList.add("login-active");main.hidden=true;login.hidden=false;
 document.getElementById("logout").hidden=true;connection.textContent="Sign in";
}
function showConsole(){
 document.body.classList.remove("login-active");login.hidden=true;main.hidden=false;
 document.getElementById("logout").hidden=false;connection.textContent="Connected";
 resolveReady();
}
document.getElementById("login-start").addEventListener("click",async()=>{
 const popup=window.open("about:blank","_blank");
 const button=document.getElementById("login-start"),status=document.getElementById("login-status"),link=document.getElementById("login-link");
 button.disabled=true;status.textContent="Creating a one-time login link…";link.hidden=true;
 try{
  const loginResult=await apiAuth("/auth/telegram/start",{method:"POST"});
  if(!loginResult.bot_url)throw new Error("Set TELEGRAM_BOT_USERNAME on agent-server to enable Telegram login links.");
  link.href=loginResult.bot_url;link.textContent=loginResult.bot_url;link.hidden=false;
  if(popup)popup.location.href=loginResult.bot_url;
  status.textContent="Open the Telegram URL and press Start in your private chat with the bot. Waiting for confirmation…";
  const deadline=Date.now()+loginResult.expires_in_seconds*1000;
  while(Date.now()<deadline){
   const result=await apiAuth("/auth/telegram/status",{method:"POST",body:JSON.stringify({token:loginResult.token})});
   if(result.authenticated){showConsole();return}
   await new Promise(resolve=>setTimeout(resolve,1000));
  }
  status.textContent="This login link expired. Please try again.";
 }catch(error){if(popup&&!popup.closed)popup.close();status.textContent=error.message}
 finally{button.disabled=false}
});
document.getElementById("logout").addEventListener("click",async()=>{
 const button=document.getElementById("logout");button.disabled=true;
 try{await apiAuth("/auth/logout",{method:"POST"});window.location.reload()}
 catch(error){connection.textContent="Sign out failed: "+error.message;button.disabled=false}
});
(async()=>{
 try{await apiAuth("/auth/session");showConsole()}
 catch{showLogin()}
})();
