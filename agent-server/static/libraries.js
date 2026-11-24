(() => {
  const $ = id => document.getElementById(id);
  const state = { available: [], names: new Map(), selected: new Set(), query: "", loaded: false };
  async function api(path, options = {}) {
    const response = await fetch(path, { credentials: "same-origin", ...options, headers: { "Content-Type": "application/json", ...(options.headers || {}) } });
    if (!response.ok) throw new Error(await response.text() || `HTTP ${response.status}`);
    return response.status === 204 ? null : response.json();
  }
  function status(message, error = false) {
    $("libraries-status").textContent = message;
    $("libraries-status").classList.toggle("error", error);
  }
  function render() {
    const container = $("library-options");
    container.replaceChildren();
    const availableIds = new Set(state.available.map(library => library.id));
    const query = state.query.trim().toLocaleLowerCase();
    const visible = state.available.filter(library =>
      !query || library.name.toLocaleLowerCase().includes(query) || library.id.toLocaleLowerCase().includes(query)
    );
    const stale = [...state.selected]
      .filter(id => !availableIds.has(id))
      .map(id => ({ id, name: state.names.get(id) || id }));
    for (const library of [...visible, ...stale.filter(item =>
      !query || item.name.toLocaleLowerCase().includes(query) || item.id.toLocaleLowerCase().includes(query)
    )]) {
      const label = document.createElement("label");
      label.className = "library-choice";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = state.selected.has(library.id);
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) state.selected.add(library.id);
        else state.selected.delete(library.id);
        updateCount();
      });
      const name = document.createElement("span");
      name.textContent = library.name + (availableIds.has(library.id) ? "" : " (not currently listed by RABEL)");
      label.append(checkbox, name);
      container.append(label);
    }
    if (!container.childElementCount) {
      container.textContent = query ? "No libraries match your search." : "RABEL did not return any libraries.";
    }
    updateCount();
  }
  function updateCount() {
    $("libraries-count").textContent = `${state.available.length} libraries available · ${state.selected.size} selected`;
  }
  async function refresh() {
    const button = $("libraries-refresh");
    button.disabled = true;
    status("Refreshing the library list from RABEL…");
    try {
      const data = await api("/libraries");
      const previous = new Set(state.selected);
      state.available = data.available || [];
      for (const library of state.available) state.names.set(library.id, library.name);
      for (const [id, name] of data.selected || []) state.names.set(id, name);
      if (!state.loaded) state.selected = new Set((data.selected || []).map(([id]) => id));
      else state.selected = previous;
      state.loaded = true;
      render();
      $("libraries-refreshed").textContent = `Last refreshed: ${new Date().toLocaleString()}`;
      status("Library list refreshed. Your current selections were preserved.");
    } catch (error) {
      status(`Could not load libraries: ${error.message}`, true);
    } finally {
      button.disabled = false;
    }
  }
  async function save() {
    const button = $("libraries-save");
    button.disabled = true;
    try {
      const data = await api("/libraries", { method: "PUT", body: JSON.stringify({ library_ids: [...state.selected] }) });
      state.selected = new Set((data.selected || []).map(([id]) => id));
      for (const [id, name] of data.selected || []) state.names.set(id, name);
      render();
      status("Preferred libraries saved.");
    } catch (error) {
      status(`Could not save preferences: ${error.message}`, true);
    } finally {
      button.disabled = false;
    }
  }
  $("libraries-search").addEventListener("input", event => { state.query = event.target.value; render(); });
  $("libraries-refresh").addEventListener("click", refresh);
  $("libraries-save").addEventListener("click", save);
  refresh();
})();
