//! Mobile-only navigation: a small global destination bar and the conversation drawer.
const $mobile = id => document.getElementById(id);

function installBottomNavigation() {
  if (document.body.dataset.layout !== "mobile") return;

  const path = window.location.pathname;
  const destinations = [
    { href: "/mobile/conversations", label: "Chats", icon: "◉", active: path.startsWith("/mobile/conversations") },
    { href: "/mobile/notes", label: "Notes", icon: "▤", active: path.startsWith("/mobile/notes") },
    { href: "/mobile/tasks", label: "Tasks", icon: "✓", active: path.startsWith("/mobile/tasks") },
    { href: "/mobile/meals", label: "Meals", icon: "◫", active: ["/mobile/meals", "/mobile/meal-planner", "/mobile/shopping-list", "/mobile/school-menus"].includes(path) },
    { href: "/mobile/profile", label: "Profile", icon: "○", active: ["/mobile/profile", "/mobile/families", "/mobile/integrations"].includes(path) }
  ];

  const nav = document.createElement("nav");
  nav.className = "mobile-bottom-nav";
  nav.setAttribute("aria-label", "Main navigation");
  nav.innerHTML = destinations.map(({ href, label, icon, active }) =>
    `<a class="mobile-bottom-nav-link${active ? " active" : ""}" href="${href}"${active ? ' aria-current="page"' : ""}>
      <span class="mobile-bottom-nav-icon" aria-hidden="true">${icon}</span>
      <span class="mobile-bottom-nav-label">${label}</span>
    </a>`
  ).join("");

  document.querySelector("header")?.insertAdjacentElement("afterend", nav);
}

function setDrawerOpen(open) {
  const sidebar = $mobile("conversation-sidebar");
  const backdrop = $mobile("drawer-backdrop");
  const toggle = $mobile("open-conversations");
  if (!sidebar || !backdrop || !toggle) return;

  sidebar.classList.toggle("open", open);
  backdrop.classList.toggle("open", open);
  toggle.setAttribute("aria-expanded", String(open));
  backdrop.setAttribute("aria-hidden", String(!open));
  document.body.style.overflow = open ? "hidden" : "";
}

function closeDrawer() {
  setDrawerOpen(false);
}

installBottomNavigation();
$mobile("open-conversations")?.addEventListener("click", () => setDrawerOpen(true));
$mobile("close-conversations")?.addEventListener("click", closeDrawer);
$mobile("drawer-backdrop")?.addEventListener("click", closeDrawer);
$mobile("conversations")?.addEventListener("click", event => {
  if (event.target.closest("li[data-id]")) closeDrawer();
});
document.addEventListener("keydown", event => {
  if (event.key === "Escape") closeDrawer();
});
