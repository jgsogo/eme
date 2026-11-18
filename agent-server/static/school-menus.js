const $ = id => document.getElementById(id);
const state = { entries: [], selected: null, loadRequest: 0 };

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
  });
  const raw = await response.text();
  let body = null;
  try {
    body = raw ? JSON.parse(raw) : null;
  } catch {
    body = raw;
  }
  if (!response.ok) {
    throw new Error(typeof body === "string" && body ? body : response.statusText);
  }
  return body;
}

function esc(value) {
  return String(value ?? "").replace(/[&<>"']/g, char => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[char]);
}

function dateString(date) {
  return [date.getFullYear(), String(date.getMonth() + 1).padStart(2, "0"), String(date.getDate()).padStart(2, "0")].join("-");
}

function monthString(date) {
  return date.getFullYear() + "-" + String(date.getMonth() + 1).padStart(2, "0");
}

function monthStart(value) {
  return new Date(value + "-01T12:00:00");
}

function statusLabel(status) {
  return ({
    known: "Menu published",
    not_published: "Unknown / not imported",
    closed: "School closed",
    unknown: "Unknown / not imported",
  })[status] || status;
}

function formatDay(date, options) {
  return new Date(date + "T12:00:00").toLocaleDateString("en-GB", options);
}

function renderMenuPlaceholder(entry) {
  const status = entry ? statusLabel(entry.status) : "Unknown / not imported";
  return '<p class="school-menu-placeholder">' + esc(status) + '</p>';
}

function renderMenuItems(items) {
  const names = items.map(item => item.name).filter(Boolean);
  if (!names.length) return '<p class="school-menu-placeholder">No dishes recorded</p>';
  return '<ul class="school-menu-items">' + names.map(name => '<li>' + esc(name) + '</li>').join("") + '</ul>';
}

function renderMonthView() {
  const first = monthStart($("month-date").value);
  const year = first.getFullYear();
  const month = first.getMonth();
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const offset = (first.getDay() + 6) % 7;
  const cells = [];
  for (let i = 0; i < offset; i++) cells.push('<div class="school-menu-calendar-empty" aria-hidden="true"></div>');
  for (let day = 1; day <= daysInMonth; day++) {
    const date = dateString(new Date(year, month, day, 12));
    const entry = state.entries.find(item => item.date === date);
    const hasMenu = Boolean(entry);
    const content = entry?.status === "known"
      ? renderMenuItems(entry.items || [])
      : renderMenuPlaceholder(entry);
    const statusClass = entry?.status === "closed"
      ? " status-closed"
      : entry?.status !== "known"
        ? " status-unknown"
        : "";
    cells.push(
      '<article class="school-menu-day' + (hasMenu ? ' has-entry' : '') + statusClass + '">' +
      '<header><strong>' + day + '</strong><span>' + esc(formatDay(date, { weekday: "short" })) + '</span></header>' +
      '<div class="school-menu-day-content">' + content + '</div>' +
      '<button type="button" class="secondary" data-edit-date="' + date + '">' +
      (hasMenu ? "Edit" : "Add") + '</button></article>'
    );
  }
  $("menu-month-view").innerHTML =
    '<div class="school-menu-calendar-weekdays">' +
    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map(day => '<span>' + day + '</span>').join("") +
    '</div><div class="school-menu-month">' + cells.join("") + '</div>';

  $("menu-month-view").querySelectorAll("[data-edit-date]").forEach(button => {
    button.addEventListener("click", () => {
      const date = button.dataset.editDate;
      edit(state.entries.find(item => item.date === date), date);
    });
  });

  if (state.selected) {
    edit(state.entries.find(item => item.date === state.selected), state.selected);
  } else {
    $("menu-editor").innerHTML = "";
  }
}

function edit(entry, date) {
  state.selected = date;
  const isNew = !entry;
  const editor = $("menu-editor");
  if (!editor) return;

  editor.innerHTML =
    '<section class="school-menu-editor">' +
    '<div class="school-menu-editor-heading"><h2>' +
    (isNew ? "Add menu for " : "Edit menu for ") +
    esc(formatDay(date, { weekday: "long", day: "numeric", month: "long", year: "numeric" })) +
    '</h2></div>' +
    '<form id="menu-form" class="recipe-form">' +
    '<label for="menu-status">Menu status</label>' +
    '<select id="menu-status">' +
    '<option value="known">Menu published</option>' +
    '<option value="closed">School closed</option>' +
    '<option value="unknown">Unknown / not imported</option>' +
    '</select>' +
    '<label for="menu-items">Dishes</label>' +
    '<textarea id="menu-items" rows="5" placeholder="One dish per line&#10;Vegetable soup&#10;Baked fish with potatoes"></textarea>' +
    '<p class="hint school-menu-field-help">Enter one dish name per line.</p>' +
    '<p id="menu-provenance" class="hint"></p>' +
    '<div class="recipe-actions">' +
    '<button type="submit">' + (isNew ? "Add menu" : "Save changes") + '</button>' +
    '<button type="button" class="secondary" id="cancel-menu">Cancel</button>' +
    (isNew ? "" : '<button type="button" class="danger" id="delete-menu">Delete menu</button>') +
    '</div>' +
    '<p id="menu-status-message" class="hint" aria-live="polite"></p>' +
    '</form></section>';

  $("menu-status").value = entry?.status === "not_published" ? "unknown" : (entry?.status || "known");
  $("menu-items").value = (entry?.items || []).map(item => item.name).join("\n");
  $("menu-provenance").textContent = entry
    ? (entry.origin === "import"
      ? "Imported from " + (entry.source || "unspecified source") +
        (entry.imported_at ? " · " + new Date(entry.imported_at * 1000).toLocaleString() : "")
      : "Manually entered or corrected")
    : "No menu record exists for this date.";
  editor.scrollIntoView({ behavior: "smooth", block: "start" });

  $("menu-form").addEventListener("submit", save);
  $("cancel-menu").addEventListener("click", () => {
    state.selected = null;
    renderMonthView();
  });
  $("delete-menu")?.addEventListener("click", remove);
}

function parseItems() {
  return $("menu-items").value.split("\n").map(line => line.trim()).filter(Boolean).map(name => ({
    name,
    notes: null,
    allergens: null,
    dietary_alternatives: [],
  }));
}

async function save(event) {
  event.preventDefault();
  const button = event.submitter || $("menu-form").querySelector('[type="submit"]');
  const date = state.selected;
  const items = parseItems();
  const status = $("menu-status").value;
  if (status === "known" && items.length === 0) {
    $("menu-status-message").textContent = "Add at least one dish before publishing a menu.";
    return;
  }
  if (status !== "known" && items.length > 0) {
    $("menu-status-message").textContent = "Dishes can only be saved when the menu status is “Menu published”.";
    return;
  }

  button.disabled = true;
  try {
    await api("/meal-planner/school-menus/" + date, {
      method: "PUT",
      body: JSON.stringify({ date, status, items }),
    });
    state.selected = null;
    await loadMonth();
    $("menu-content").insertAdjacentHTML("afterbegin",
      '<p class="hint" role="status">Menu saved for ' + esc(formatDay(date, { weekday: "long", day: "numeric", month: "long" })) + '.</p>');
  } catch (error) {
    const message = $("menu-status-message");
    if (message) message.textContent = error.message;
  } finally {
    button.disabled = false;
  }
}

async function remove() {
  const date = state.selected;
  if (!date || !confirm("Delete this school menu record?")) return;
  const button = $("delete-menu");
  button.disabled = true;
  try {
    await api("/meal-planner/school-menus/" + date, { method: "DELETE" });
    state.selected = null;
    await loadMonth();
    $("menu-content").insertAdjacentHTML("afterbegin",
      '<p class="hint" role="status">Menu deleted for ' + esc(formatDay(date, { weekday: "long", day: "numeric", month: "long" })) + '.</p>');
  } catch (error) {
    const message = $("menu-status-message");
    if (message) message.textContent = error.message;
    button.disabled = false;
  }
}

async function loadMonth() {
  const value = $("month-date").value;
  if (!value) return;
  const request = ++state.loadRequest;
  const start = monthStart(value);
  const end = new Date(start.getFullYear(), start.getMonth() + 1, 0, 12);
  state.selected = null;
  $("menu-content").innerHTML = '<p class="empty">Loading school menus…</p>';

  try {
    const entries = await api("/meal-planner/school-menus/?from=" +
      dateString(start) + "&to=" + dateString(end));
    if (request !== state.loadRequest) return;
    state.entries = entries;
    const count = new Set(entries.filter(entry => entry.status === "known").map(entry => entry.date)).size;
    $("menu-content").innerHTML =
      '<p class="hint" role="status">' +
      (count
        ? "Month loaded. Published menus are available for " + count + " of " + end.getDate() + " days."
        : "Month loaded successfully, but no published menus are available for this month.") +
      '</p><div id="menu-month-view"></div><div id="menu-editor"></div>';
    renderMonthView();
  } catch (error) {
    if (request === state.loadRequest) {
      $("menu-content").innerHTML =
        '<p class="empty" role="alert">Could not load school menus for this month: ' +
        esc(error.message) + '</p>';
    }
    throw error;
  }
}

$("previous-month").addEventListener("click", () => {
  const date = monthStart($("month-date").value);
  date.setMonth(date.getMonth() - 1);
  $("month-date").value = monthString(date);
  loadMonth().catch(() => {});
});

$("next-month").addEventListener("click", () => {
  const date = monthStart($("month-date").value);
  date.setMonth(date.getMonth() + 1);
  $("month-date").value = monthString(date);
  loadMonth().catch(() => {});
});

$("month-date").addEventListener("change", () => loadMonth().catch(() => {}));

(async () => {
  try {
    await window.emeConsoleReady;
    $("month-date").value = monthString(new Date());
    await loadMonth();
  } catch (error) {
    $("connection").textContent = "Connection error";
    console.error("Failed to load school menus", error);
  }
})();
