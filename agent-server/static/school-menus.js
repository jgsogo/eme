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
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, "0"),
    String(date.getDate()).padStart(2, "0"),
  ].join("-");
}

function monday(value) {
  const date = new Date(value + "T12:00:00");
  date.setDate(date.getDate() - (date.getDay() + 6) % 7);
  return date;
}

function statusLabel(status) {
  return ({
    known: "Menu published",
    not_published: "Not published",
    closed: "School closed",
    unknown: "Unknown / not imported",
  })[status] || status;
}

function formatDay(date, options) {
  return new Date(date + "T12:00:00").toLocaleDateString("en-GB", options);
}

function renderMenuPlaceholder(entry) {
  const status = entry ? statusLabel(entry.status) : "No menu added yet";
  return '<div class="school-menu-placeholder"><span>' + esc(status) + '</span></div>' +
    '<dl class="school-menu-empty-structure">' +
    '<div><dt>First course</dt><dd></dd></div>' +
    '<div><dt>Second course</dt><dd></dd></div>' +
    '<div><dt>Dessert</dt><dd></dd></div></dl>';
}

function renderMenuItems(items) {
  if (!items.length) {
    return '<p class="school-menu-placeholder">Menu published; no dishes recorded.</p>';
  }
  return '<ul class="school-menu-items">' + items.map(item =>
    '<li><strong>' + esc(item.name) + '</strong>' +
    (item.notes ? '<p>' + esc(item.notes) + '</p>' : '') +
    (item.allergens?.length ? '<small>Allergens: ' + esc(item.allergens.join(", ")) + '</small>' : '') +
    (item.dietary_alternatives?.length ? '<small>Alternatives: ' + esc(item.dietary_alternatives.join(", ")) + '</small>' : '') +
    '</li>'
  ).join("") + '</ul>';
}

function renderWeekView() {
  const start = monday($("week-date").value);
  const days = Array.from({ length: 7 }, (_, index) => {
    const date = new Date(start);
    date.setDate(date.getDate() + index);
    const key = dateString(date);
    return { date: key, entry: state.entries.find(item => item.date === key) };
  });

  $("menu-week-view").innerHTML = '<div class="school-menu-week">' + days.map(({ date, entry }) => {
    const hasMenu = Boolean(entry);
    const content = entry?.status === "known"
      ? renderMenuItems(entry.items || []) + (entry.notes ? '<p>' + esc(entry.notes) + '</p>' : '')
      : renderMenuPlaceholder(entry);
    const action = hasMenu ? "Edit menu" : "Add menu";

    return '<article class="school-menu-day">' +
      '<header><strong>' + esc(formatDay(date, { weekday: "long" })) + '</strong>' +
      '<span>' + esc(formatDay(date, { day: "2-digit", month: "2-digit" })) + '</span></header>' +
      content +
      '<button type="button" class="secondary" data-edit-date="' + date + '">' + action + '</button>' +
      '</article>';
  }).join("") + '</div>';

  if (state.selected) {
    const entry = state.entries.find(item => item.date === state.selected);
    edit(entry, state.selected);
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
    '<option value="not_published">Not published</option>' +
    '<option value="closed">School closed</option>' +
    '<option value="unknown">Unknown / not imported</option>' +
    '</select>' +
    '<label for="menu-items">Dishes</label>' +
    '<textarea id="menu-items" rows="7" placeholder="One dish per line: dish | notes | allergens | dietary alternatives&#10;Vegetable soup | contains celery | celery | vegan option"></textarea>' +
    '<p class="hint school-menu-field-help">Enter one dish per line. The optional fields after the dish name are notes, allergens and dietary alternatives, separated by |.</p>' +
    '<label for="menu-notes">Menu notes</label>' +
    '<textarea id="menu-notes" rows="3" placeholder="Additional information about this day's menu"></textarea>' +
    '<p id="menu-provenance" class="hint"></p>' +
    '<div class="recipe-actions">' +
    '<button type="submit">' + (isNew ? "Add menu" : "Save changes") + '</button>' +
    '<button type="button" class="secondary" id="cancel-menu">Cancel</button>' +
    (isNew ? "" : '<button type="button" class="danger" id="delete-menu">Delete menu</button>') +
    '</div>' +
    '<p id="menu-status-message" class="hint" aria-live="polite"></p>' +
    '</form></section>';

  $("menu-status").value = entry?.status || "unknown";
  $("menu-items").value = (entry?.items || []).map(item => [
    item.name,
    item.notes || "",
    item.allergens?.join(", ") || "",
    item.dietary_alternatives?.join(", ") || "",
  ].join(" | ")).join("\n");
  $("menu-notes").value = entry?.notes || "";
  $("menu-provenance").textContent = entry
    ? (entry.origin === "import"
      ? "Imported from " + (entry.source || "unspecified source") +
        (entry.imported_at ? " · " + new Date(entry.imported_at * 1000).toLocaleString() : "")
      : "Manually entered or corrected")
    : "No menu record exists for this date. Add the dishes and save to create it.";

  $("menu-form").addEventListener("submit", save);
  $("cancel-menu").addEventListener("click", () => {
    state.selected = null;
    renderWeekView();
  });
  $("delete-menu")?.addEventListener("click", remove);
}

function parseItems() {
  return $("menu-items").value.split("\n").map(line => line.trim()).filter(Boolean).map(line => {
    const parts = line.split("|").map(part => part.trim());
    return {
      name: parts[0],
      notes: parts[1] || null,
      allergens: parts[2] ? parts[2].split(",").map(value => value.trim()).filter(Boolean) : null,
      dietary_alternatives: parts[3] ? parts[3].split(",").map(value => value.trim()).filter(Boolean) : [],
    };
  });
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
      body: JSON.stringify({ date, status, items, notes: $("menu-notes").value || null }),
    });
    state.selected = null;
    await loadWeek();
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
    await loadWeek();
    $("menu-content").insertAdjacentHTML("afterbegin",
      '<p class="hint" role="status">Menu deleted for ' + esc(formatDay(date, { weekday: "long", day: "numeric", month: "long" })) + '.</p>');
  } catch (error) {
    const message = $("menu-status-message");
    if (message) message.textContent = error.message;
    button.disabled = false;
  }
}

async function loadWeek() {
  const value = $("week-date").value;
  if (!value) return;
  const request = ++state.loadRequest;
  const start = monday(value);
  $("week-date").value = dateString(start);
  const end = new Date(start);
  end.setDate(end.getDate() + 6);
  state.selected = null;
  $("menu-content").innerHTML = '<p class="empty">Loading school menus…</p>';

  try {
    const entries = await api("/meal-planner/school-menus?from=" +
      dateString(start) + "&to=" + dateString(end));
    if (request !== state.loadRequest) return;
    state.entries = entries;
    const count = new Set(entries.map(entry => entry.date)).size;
    $("menu-content").innerHTML =
      '<p class="hint" role="status">' +
      (count
        ? "Week loaded. Menu data is available for " + count + " of 7 days."
        : "Week loaded successfully, but no menu data is available for this week.") +
      '</p><div id="menu-week-view"></div><div id="menu-editor"></div>';
    renderWeekView();
  } catch (error) {
    if (request === state.loadRequest) {
      $("menu-content").innerHTML =
        '<p class="empty" role="alert">Could not load school menus for this week: ' +
        esc(error.message) + '</p>';
    }
    throw error;
  }
}

$("menu-content").addEventListener("click", event => {
  const button = event.target.closest("[data-edit-date]");
  if (!button) return;
  edit(state.entries.find(item => item.date === button.dataset.editDate), button.dataset.editDate);
});

$("previous-week").addEventListener("click", () => {
  const start = monday($("week-date").value);
  start.setDate(start.getDate() - 7);
  $("week-date").value = dateString(start);
  loadWeek().catch(() => {});
});

$("next-week").addEventListener("click", () => {
  const start = monday($("week-date").value);
  start.setDate(start.getDate() + 7);
  $("week-date").value = dateString(start);
  loadWeek().catch(() => {});
});

$("week-date").addEventListener("change", () => loadWeek().catch(() => {}));

(async () => {
  try {
    await window.emeConsoleReady;
    $("week-date").value = dateString(monday(dateString(new Date())));
    await loadWeek();
  } catch (error) {
    $("connection").textContent = "Connection error";
    console.error("Failed to load school menus", error);
  }
})();
