const $ = id => document.getElementById(id);
const state = { entries: [], selected: null, loadRequest: 0, familyId: "" };

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

function statusLabel(status) {
  return ({
    known: "Menu published",
    not_published: "Menu not published",
    closed: "School closed",
    unknown: "Unknown",
  })[status] || status;
}

function formatDay(date, options) {
  return new Date(date + "T12:00:00").toLocaleDateString("en-GB", options);
}


function appendMenuLog(message) {
  const log = $("menu-status-log");
  if (!log) return;
  const item = document.createElement("p");
  item.className = "hint";
  item.setAttribute("role", "status");
  item.textContent = message;
  log.append(item);
}

function renderMenuPlaceholder(entry) {
  const status = entry ? statusLabel(entry.status) : "Unknown";
  return '<p class="school-menu-placeholder">' + esc(status) + '</p>';
}

function renderMenuItems(items) {
  const names = items.map(item => item.name).filter(Boolean);
  if (!names.length) return '<p class="school-menu-placeholder">No dishes recorded</p>';
  return '<ul class="school-menu-items">' + names.map(name => '<li>' + esc(name) + '</li>').join("") + '</ul>';
}

function renderDayView() {
  const date = $("menu-date").value;
  const entry = state.entries.find(item => item.date === date);
  const heading = formatDay(date, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
  const status = entry ? statusLabel(entry.status) : "No menu recorded";
  const content = entry?.status === "known" ? renderMenuItems(entry.items || []) : renderMenuPlaceholder(entry);
  $("menu-content").innerHTML = '<article class="school-menu-day-detail"><header class="school-menu-day-detail-heading"><div><h3>' + esc(heading) + '</h3><p class="hint">' + esc(status) + '</p></div><button type="button" class="text-button" id="edit-day-menu">' + (entry ? "Edit menu" : "＋ Add menu") + '</button></header><section class="meal-calendar-slot school-menu-lunch-slot"><div class="meal-calendar-slot-heading"><h3>Lunch</h3></div><div class="meal-slot-details school-menu-lunch-details">' + content + '</div></section></article><div id="menu-status-log" class="school-menu-status-log" aria-live="polite"></div>';
  $("edit-day-menu").addEventListener("click", () => edit(entry, date));
}

function edit(entry, date) {
  state.selected = date;
  const isNew = !entry;
  const editor = $("menu-editor");
  if (!editor) return;

  editor.innerHTML =
    '<section class="school-menu-editor">' +
    '<div class="school-menu-editor-heading"><h2 id="menu-editor-title">' +
    (isNew ? "Add menu for " : "Edit menu for ") +
    esc(formatDay(date, { weekday: "long", day: "numeric", month: "long", year: "numeric" })) +
    '</h2></div>' +
    '<form id="menu-form" class="recipe-form">' +
    '<label for="menu-status">Menu status</label>' +
    '<select id="menu-status">' +
    '<option value="known">Menu published</option>' +
    '<option value="closed">School closed</option>' +
    '<option value="unknown">Unknown</option>' +
    '</select>' +
    '<label for="menu-items">Dishes</label>' +
    '<textarea id="menu-items" rows="5" placeholder="One dish per line&#10;Vegetable soup&#10;Baked fish with potatoes"></textarea>' +
    '<p class="hint school-menu-field-help">Enter one dish name per line.</p>' +
    '<div class="recipe-actions">' +
    '<button type="submit">' + (isNew ? "Add menu" : "Save changes") + '</button>' +
    '<button type="button" class="secondary" id="cancel-menu">Cancel</button>' +
    (isNew ? "" : '<button type="button" class="danger" id="delete-menu">Delete menu</button>') +
    '</div>' +
    '<p id="menu-status-message" class="hint" aria-live="polite"></p>' +
    '</form></section>';

  $("menu-status").value = entry?.status || "known";
  $("menu-items").value = (entry?.items || []).map(item => item.name).join("\n");
  $("menu-dialog").showModal();

  $("menu-form").addEventListener("submit", save);
  $("cancel-menu").addEventListener("click", () => {
    $("menu-dialog").close();
  });
  $("delete-menu")?.addEventListener("click", remove);
}

$("menu-dialog").addEventListener("close", () => { state.selected = null; });

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
    await api("/meal-planner/school-menus/" + date + "?family_id=" + encodeURIComponent(state.familyId), {
      method: "PUT",
      body: JSON.stringify({ date, status, items }),
    });
    state.selected = null;
    $("menu-dialog").close();
    await loadDay();
    appendMenuLog("Menu saved for " + formatDay(date, { weekday: "long", day: "numeric", month: "long" }) + ".");
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
    await api("/meal-planner/school-menus/" + date + "?family_id=" + encodeURIComponent(state.familyId), { method: "DELETE" });
    state.selected = null;
    $("menu-dialog").close();
    await loadDay();
    appendMenuLog("Menu deleted for " + formatDay(date, { weekday: "long", day: "numeric", month: "long" }) + ".");
  } catch (error) {
    const message = $("menu-status-message");
    if (message) message.textContent = error.message;
    button.disabled = false;
  }
}

async function loadDay() {
  const date = $("menu-date").value;
  if (!date) return;
  const request = ++state.loadRequest;
  state.selected = null;
  $("menu-content").innerHTML = '<p class="empty">Loading school menu…</p>';
  try {
    if (!state.familyId) {
      state.entries = [];
      $("menu-content").innerHTML = '<p class="empty">Create or join a family to manage school menus. <a href="/console/families">Manage families</a>.</p>';
      $("connection").textContent = "No family selected";
      return;
    }
    const entries = await api("/meal-planner/school-menus/?family_id=" + encodeURIComponent(state.familyId) + "&from=" + date + "&to=" + date);
    if (request !== state.loadRequest) return;
    state.entries = entries;
    renderDayView();
  } catch (error) {
    if (request === state.loadRequest) $("menu-content").innerHTML = '<p class="empty" role="alert">Could not load the school menu for this day: ' + esc(error.message) + '</p>';
    throw error;
  }
}
function shiftDay(offset) {
  const date = new Date($("menu-date").value + "T12:00:00");
  date.setDate(date.getDate() + offset);
  $("menu-date").value = dateString(date);
  loadDay().catch(() => {});
}

$("previous-day").addEventListener("click", () => shiftDay(-1));
$("next-day").addEventListener("click", () => shiftDay(1));
$("menu-date").addEventListener("change", () => loadDay().catch(() => {}));

window.addEventListener("eme:family-changed", () => { state.familyId = window.emeMealsFamily.currentFamilyId(); state.selected = null; loadDay(); });

(async () => {
  try {
    await window.emeConsoleReady;
    await window.emeMealsFamily.init();
    state.familyId = window.emeMealsFamily.currentFamilyId();
    $("menu-date").value = dateString(new Date());
    await loadDay();
  } catch (error) {
    $("connection").textContent = "Connection error";
    console.error("Failed to load school menus", error);
  }
})();
