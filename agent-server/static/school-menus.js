const $ = id => document.getElementById(id);
const state = { entries: [], selected: null, loadRequest: 0, familyId: "" };

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
  });
  const raw = await response.text();
  let body = null;
  try { body = raw ? JSON.parse(raw) : null; } catch { body = raw; }
  if (!response.ok) throw new Error(typeof body === "string" && body ? body : response.statusText);
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

function parseDate(value) {
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day, 12);
}

function addDays(date, count) {
  const result = new Date(date);
  result.setDate(result.getDate() + count);
  return result;
}

function mondayOf(date) {
  return addDays(date, -((date.getDay() + 6) % 7));
}

function weekDates(selectedDate) {
  const monday = mondayOf(parseDate(selectedDate));
  return Array.from({ length: 7 }, (_, index) => dateString(addDays(monday, index)));
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
  return parseDate(date).toLocaleDateString("en-GB", options);
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
  return '<p class="school-menu-placeholder">' + esc(entry ? statusLabel(entry.status) : "No menu recorded") + '</p>';
}

function renderMenuItems(items) {
  const names = items.map(item => item.name).filter(Boolean);
  if (!names.length) return '<p class="school-menu-placeholder">No dishes recorded</p>';
  return '<ul class="school-menu-items">' + names.map(name => '<li>' + esc(name) + '</li>').join("") + '</ul>';
}

function renderWeekView() {
  const dates = weekDates($("menu-date").value);
  const first = dates[0], last = dates[6];
  const range = formatDay(first, { day: "numeric", month: "long" }) + " – " +
    formatDay(last, { day: "numeric", month: "long", year: "numeric" });
  const days = dates.map(date => {
    const entry = state.entries.find(item => item.date === date);
    const heading = formatDay(date, { weekday: "long", day: "numeric", month: "long" });
    const status = entry ? statusLabel(entry.status) : "No menu recorded";
    const content = entry?.status === "known" ? renderMenuItems(entry.items || []) : renderMenuPlaceholder(entry);
    return '<article class="school-menu-day-detail" data-menu-date="' + date + '">' +
      '<header class="school-menu-day-detail-heading"><div><h3>' + esc(heading) +
      '</h3><p class="hint">' + esc(status) + '</p></div><button type="button" class="text-button" data-edit-day="' + date + '">' +
      (entry ? "Edit menu" : "＋ Add menu") + '</button></header>' +
      '<section class="meal-calendar-slot school-menu-lunch-slot"><div class="meal-calendar-slot-heading"><h3>Lunch</h3></div>' +
      '<div class="meal-slot-details school-menu-lunch-details">' + content + '</div></section></article>';
  }).join("");
  $("menu-content").innerHTML = '<div class="school-menu-week-heading"><h3>' + esc(range) +
    '</h3><p class="hint">Monday to Sunday</p></div><div class="school-menu-week-days">' + days +
    '</div><div id="menu-status-log" class="school-menu-status-log" aria-live="polite"></div>';
  $("menu-content").querySelectorAll("[data-edit-day]").forEach(button => {
    const date = button.dataset.editDay;
    const entry = state.entries.find(item => item.date === date);
    button.addEventListener("click", () => edit(entry, date));
  });
}

function edit(entry, date) {
  state.selected = date;
  const isNew = !entry;
  const editor = $("menu-editor");
  if (!editor) return;
  editor.innerHTML =
    '<section class="school-menu-editor"><div class="school-menu-editor-heading"><h2 id="menu-editor-title">' +
    (isNew ? "Add menu for " : "Edit menu for ") +
    esc(formatDay(date, { weekday: "long", day: "numeric", month: "long", year: "numeric" })) +
    '</h2></div><form id="menu-form" class="recipe-form">' +
    '<label for="menu-status">Menu status</label><select id="menu-status">' +
    '<option value="known">Menu published</option><option value="closed">School closed</option><option value="unknown">Unknown</option>' +
    '</select><label for="menu-items">Dishes</label>' +
    '<textarea id="menu-items" rows="5" placeholder="One dish per line&#10;Vegetable soup&#10;Baked fish with potatoes"></textarea>' +
    '<p class="hint school-menu-field-help">Enter one dish name per line.</p><div class="recipe-actions">' +
    '<button type="submit">' + (isNew ? "Add menu" : "Save changes") + '</button>' +
    '<button type="button" class="secondary" id="cancel-menu">Cancel</button>' +
    (isNew ? "" : '<button type="button" class="danger" id="delete-menu">Delete menu</button>') +
    '</div><p id="menu-status-message" class="hint" aria-live="polite"></p></form></section>';
  $("menu-status").value = entry?.status || "known";
  $("menu-items").value = (entry?.items || []).map(item => item.name).join("\n");
  $("menu-dialog").showModal();
  $("menu-form").addEventListener("submit", save);
  $("cancel-menu").addEventListener("click", () => $("menu-dialog").close());
  $("delete-menu")?.addEventListener("click", remove);
}

$("menu-dialog").addEventListener("close", () => { state.selected = null; });

function parseItems() {
  return $("menu-items").value.split("\n").map(line => line.trim()).filter(Boolean).map(name => ({
    name, notes: null, allergens: null, dietary_alternatives: [],
  }));
}

async function save(event) {
  event.preventDefault();
  const button = event.submitter || $("menu-form").querySelector('[type="submit"]');
  const date = state.selected, items = parseItems(), status = $("menu-status").value;
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
      method: "PUT", body: JSON.stringify({ date, status, items }),
    });
    state.selected = null;
    $("menu-dialog").close();
    await loadWeek();
    appendMenuLog("Menu saved for " + formatDay(date, { weekday: "long", day: "numeric", month: "long" }) + ".");
  } catch (error) {
    const message = $("menu-status-message");
    if (message) message.textContent = error.message;
  } finally { button.disabled = false; }
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
    await loadWeek();
    appendMenuLog("Menu deleted for " + formatDay(date, { weekday: "long", day: "numeric", month: "long" }) + ".");
  } catch (error) {
    const message = $("menu-status-message");
    if (message) message.textContent = error.message;
    button.disabled = false;
  }
}

async function loadWeek() {
  const selectedDate = $("menu-date").value;
  if (!selectedDate) return;
  selectWeekContaining(selectedDate);
  const dates = weekDates($("menu-date").value);
  const from = dates[0], to = dates[6];
  const request = ++state.loadRequest;
  state.selected = null;
  $("menu-content").innerHTML = '<p class="empty">Loading school menus…</p>';
  try {
    if (!state.familyId) {
      state.entries = [];
      $("menu-content").innerHTML = '<p class="empty">Create or join a family to manage school menus. <a href="/console/families">Manage families</a>.</p>';
      return;
    }
    const entries = await api("/meal-planner/school-menus/?family_id=" + encodeURIComponent(state.familyId) + "&from=" + from + "&to=" + to);
    if (request !== state.loadRequest) return;
    state.entries = entries;
    renderWeekView();
  } catch (error) {
    if (request === state.loadRequest) $("menu-content").innerHTML =
      '<p class="empty" role="alert">Could not load the school menus for this week: ' + esc(error.message) + '</p>';
  }
}

function selectWeekContaining(value) {
  if (!value) return;
  $("menu-date").value = dateString(mondayOf(parseDate(value)));
}

function shiftWeek(offset) {
  const monday = mondayOf(parseDate($("menu-date").value));
  $("menu-date").value = dateString(addDays(monday, offset * 7));
  loadWeek();
}

$("previous-day").addEventListener("click", () => shiftWeek(-1));
$("next-day").addEventListener("click", () => shiftWeek(1));
$("menu-date").addEventListener("change", () => {
  selectWeekContaining($("menu-date").value);
  loadWeek();
});

window.addEventListener("eme:family-changed", () => {
  state.familyId = window.emeMealsFamily.currentFamilyId();
  state.selected = null;
  loadWeek();
});

(async () => {
  try {
    await window.emeConsoleReady;
    await window.emeMealsFamily.init();
    state.familyId = window.emeMealsFamily.currentFamilyId();
    selectWeekContaining(dateString(new Date()));
    await loadWeek();
  } catch (error) {
    console.error("Failed to load school menus", error);
    const content = $("menu-content");
    if (content) content.innerHTML = '<p class="empty" role="alert">Could not initialize school menus: ' + esc(error.message) + '</p>';
  } finally {
    $("menu-content")?.closest("main")?.removeAttribute("hidden");
  }
})();
