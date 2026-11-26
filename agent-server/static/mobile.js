//! Mobile-only navigation: the icon destination bar and shared section drawers.
const $mobile = id => document.getElementById(id);

function installBottomNavigation() {
  if (document.body.dataset.layout !== "mobile") return;

  const path = window.location.pathname;
  const destinations = [
    { href: "/mobile/conversations", label: "Chats", icon: '<svg viewBox="0 0 24 24"><path d="M20 11.5a7.5 7.5 0 0 1-7.5 7.5H6l-3 2v-6.5A7.5 7.5 0 1 1 20 11.5Z"/><path d="M8 11h8M8 14h5"/></svg>', active: path.startsWith("/mobile/conversations") },
    { href: "/mobile/notes", label: "Notes", icon: '<svg viewBox="0 0 24 24"><path d="M6 3h9l4 4v14H6z"/><path d="M14 3v5h5M9 12h7M9 16h7"/></svg>', active: path.startsWith("/mobile/notes") },
    { href: "/mobile/tasks", label: "Tasks", icon: '<svg viewBox="0 0 24 24"><path d="m4 7 2 2 4-4M12 7h8M4 16l2 2 4-4M12 16h8"/></svg>', active: path.startsWith("/mobile/tasks") },
    { href: "/mobile/meals", label: "Meals", icon: '<svg viewBox="0 0 24 24"><path d="M7 3v7M4 3v4a3 3 0 0 0 6 0V3M7 10v11M17 3v18M17 3c3 2 3 7 0 9"/></svg>', active: ["/mobile/meals", "/mobile/meal-planner", "/mobile/shopping-list", "/mobile/school-menus"].includes(path) },
    { href: "/mobile/profile", label: "Profile", icon: '<svg viewBox="0 0 24 24"><circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/></svg>', active: ["/mobile/profile", "/mobile/families", "/mobile/integrations", "/mobile/admin/usage", "/mobile/admin/tools", "/mobile/admin/agents", "/mobile/admin/providers"].includes(path) }
  ];

  const nav = document.createElement("nav");
  nav.className = "mobile-bottom-nav";
  nav.setAttribute("aria-label", "Main navigation");
  nav.innerHTML = destinations.map(({ href, label, icon, active }) =>
    `<a class="mobile-bottom-nav-link${active ? " active" : ""}" href="${href}" aria-label="${label}" title="${label}"${active ? ' aria-current="page"' : ""}>
      <span class="mobile-bottom-nav-icon" aria-hidden="true">${icon}</span>
    </a>`
  ).join("");

  document.querySelector("main")?.insertAdjacentElement("beforebegin", nav);
}

function setDrawerOpen(open) {
  const sidebar = $mobile("profile-sidebar") || $mobile("meals-sidebar");
  const backdrop = $mobile("profile-drawer-backdrop") || $mobile("meals-drawer-backdrop");
  const toggle = $mobile("open-profile") || $mobile("open-meals");
  if (!sidebar || !backdrop || !toggle) return;
  sidebar.classList.toggle("open", open);
  backdrop.classList.toggle("open", open);
  toggle.setAttribute("aria-expanded", String(open));
  backdrop.setAttribute("aria-hidden", String(!open));
  document.body.style.overflow = open ? "hidden" : "";
}

function closeDrawer() { setDrawerOpen(false); }
installBottomNavigation();
$mobile("open-profile")?.addEventListener("click", () => setDrawerOpen(true));
$mobile("close-profile")?.addEventListener("click", closeDrawer);
$mobile("profile-drawer-backdrop")?.addEventListener("click", closeDrawer);
$mobile("profile-sidebar")?.addEventListener("click", event => {
  if (event.target.closest("a[data-profile-destination]")) closeDrawer();
});
const profilePath = window.location.pathname;
const profileDestination = profilePath.startsWith("/mobile/integrations") ? "integrations" : profilePath.startsWith("/mobile/families") ? "families" : profilePath.startsWith("/mobile/admin/") ? "admin" : "profile";
$mobile("profile-sidebar")?.querySelectorAll("a[data-profile-destination]").forEach(link => {
  if (link.dataset.profileDestination === profileDestination) link.setAttribute("aria-current", "page");
});
document.addEventListener("keydown", event => { if (event.key === "Escape") closeDrawer(); });

$mobile("open-meals")?.addEventListener("click", () => setDrawerOpen(true));
$mobile("close-meals")?.addEventListener("click", closeDrawer);
$mobile("meals-drawer-backdrop")?.addEventListener("click", closeDrawer);
$mobile("meals-sidebar")?.addEventListener("click", event => {
  if (event.target.closest("a[data-meals-destination]")) closeDrawer();
});
