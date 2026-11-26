// Shared desktop console header. Keep the mobile shell independent.
(() => {
  const header = document.querySelector("header");
  if (!header) return;

  const path = window.location.pathname;
  const section = path.startsWith("/console/admin") ? "admin"
    : path.startsWith("/console/notes") ? "notes"
    : path.startsWith("/console/books") ? "books"
    : path.startsWith("/console/tasks") ? "tasks"
    : ["/console/meals", "/console/recipes", "/console/meal-planner", "/console/shopping-list", "/console/school-menus"].includes(path) ? "meals"
    : path.startsWith("/console/profile") || path.startsWith("/console/integrations") || path.startsWith("/console/families") ? "profile"
    : "conversations";
  const profileActive = section === "profile";

  const navItems = [
    ["conversations", "Conversations", "/console/conversations"],
    ["notes", "Notes", "/console/notes"],
    ["books", "Books", "/console/books"],
    ["tasks", "Tasks", "/console/tasks"],
    ["meals", "Meals", "/console/meals"],
    ["admin", "Admin", "/console/admin/usage"],
  ];

  header.innerHTML = `
    <div class="header-title">
      <h1>EME Agent Console</h1>
      <nav class="main-nav" aria-label="Main navigation">
        ${navItems.map(([key, label, href]) =>
          `<a class="main-nav-link${section === key ? " active" : ""}" href="${href}"${section === key ? ' aria-current="page"' : ""}>${label}</a>`
        ).join("")}
      </nav>
    </div>
    <div class="header-actions">


      <a class="profile-link${profileActive ? " active" : ""}" href="/console/profile" aria-label="Profile" title="Profile"${profileActive ? ' aria-current="page"' : ""}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/></svg>
      </a>
      <button class="secondary" id="logout" type="button" hidden>Sign out</button>
      <span id="connection" class="hint">Connecting…</span>
    </div>`;
})();
