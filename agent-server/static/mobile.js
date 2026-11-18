//! Mobile-only navigation behavior for the conversation drawer.
const $mobile=id=>document.getElementById(id);
function setDrawerOpen(open){const sidebar=$mobile("conversation-sidebar"),backdrop=$mobile("drawer-backdrop"),toggle=$mobile("open-conversations");if(!sidebar||!backdrop||!toggle)return;sidebar.classList.toggle("open",open);backdrop.classList.toggle("open",open);toggle.setAttribute("aria-expanded",String(open));backdrop.setAttribute("aria-hidden",String(!open));document.body.style.overflow=open?"hidden":"";}
function closeDrawer(){setDrawerOpen(false)}
$mobile("open-conversations")?.addEventListener("click",()=>setDrawerOpen(true));
$mobile("close-conversations")?.addEventListener("click",closeDrawer);
$mobile("drawer-backdrop")?.addEventListener("click",closeDrawer);
$mobile("conversations")?.addEventListener("click",event=>{if(event.target.closest("li[data-id]"))closeDrawer()});
document.addEventListener("keydown",event=>{if(event.key==="Escape")closeDrawer()});
