(() => {
  const $ = id => document.getElementById(id);
  const state = {
    available: [],
    names: new Map(),
    selected: new Set(),
    collapsed: new Set(),
    query: "",
    loaded: false,
  };

  async function api(path, options = {}) {
    const response = await fetch(path, {
      credentials: "same-origin",
      ...options,
      headers: { "Content-Type": "application/json", ...(options.headers || {}) },
    });
    if (!response.ok) throw new Error(await response.text() || `HTTP ${response.status}`);
    return response.status === 204 ? null : response.json();
  }

  function status(message, error = false) {
    $("libraries-status").textContent = message;
    $("libraries-status").classList.toggle("error", error);
  }

  function buildTree(libraries) {
    const roots = [];
    const parents = [];
    for (const library of libraries) {
      const depth = Math.max(0, Number(library.depth) || 0);
      const node = { ...library, depth, children: [] };
      while (parents.length > depth) parents.pop();
      const parent = parents[parents.length - 1];
      if (parent) parent.children.push(node);
      else roots.push(node);
      parents[depth] = node;
      parents.length = depth + 1;
    }
    return roots;
  }

  function matchesQuery(node, query) {
    if (!query) return true;
    return node.name.toLocaleLowerCase().includes(query)
      || node.id.toLocaleLowerCase().includes(query)
      || node.children.some(child => matchesQuery(child, query));
  }

  function subtreeIds(node) {
    return [node.id, ...node.children.flatMap(subtreeIds)];
  }

  function selectionState(node) {
    const ids = subtreeIds(node);
    const selectedCount = ids.filter(id => state.selected.has(id)).length;
    return {
      checked: selectedCount === ids.length,
      indeterminate: selectedCount > 0 && selectedCount < ids.length,
    };
  }

  function setSubtreeSelected(node, selected) {
    for (const id of subtreeIds(node)) {
      if (selected) state.selected.add(id);
      else state.selected.delete(id);
    }
  }

  function renderNode(node, container, query, availableIds) {
    if (!matchesQuery(node, query)) return;

    const row = document.createElement("div");
    row.className = "library-choice";
    row.style.paddingLeft = (node.depth * 18 + 4) + "px";

    if (node.children.length > 0) {
      const toggle = document.createElement("button");
      const isCollapsed = state.collapsed.has(node.id) && !query;
      toggle.type = "button";
      toggle.className = "library-tree-toggle secondary";
      toggle.textContent = isCollapsed ? "▸" : "▾";
      toggle.setAttribute("aria-label", isCollapsed ? `Expand ${node.name}` : `Collapse ${node.name}`);
      toggle.setAttribute("aria-expanded", String(!isCollapsed));
      toggle.addEventListener("click", () => {
        if (state.collapsed.has(node.id)) state.collapsed.delete(node.id);
        else state.collapsed.add(node.id);
        render();
      });
      row.append(toggle);
    } else {
      const spacer = document.createElement("span");
      spacer.className = "library-tree-spacer";
      row.append(spacer);
    }

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    const selection = selectionState(node);
    checkbox.checked = selection.checked;
    checkbox.indeterminate = selection.indeterminate;
    checkbox.setAttribute("aria-label", node.name);
    checkbox.addEventListener("change", () => {
      setSubtreeSelected(node, checkbox.checked);
      render();
    });

    const name = document.createElement("span");
    name.textContent = node.name + (availableIds.has(node.id) ? "" : " (not currently listed by RABEL)");
    row.append(checkbox, name);
    container.append(row);

    if (node.children.length > 0 && (!state.collapsed.has(node.id) || query)) {
      for (const child of node.children) renderNode(child, container, query, availableIds);
    }
  }

  function render() {
    const container = $("library-options");
    container.replaceChildren();
    const availableIds = new Set(state.available.map(library => library.id));
    const query = state.query.trim().toLocaleLowerCase();
    const roots = buildTree(state.available);
    const stale = [...state.selected]
      .filter(id => !availableIds.has(id))
      .map(id => ({ id, name: state.names.get(id) || id, depth: 0, children: [] }));

    for (const root of roots) renderNode(root, container, query, availableIds);
    for (const node of stale) {
      if (!query || node.name.toLocaleLowerCase().includes(query) || node.id.toLocaleLowerCase().includes(query)) {
        renderNode(node, container, query, availableIds);
      }
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
      if($("libraries-refreshed"))$("libraries-refreshed").textContent = `Last refreshed: ${new Date().toLocaleString()}`;
      status("Library list loaded. Your current selections were preserved.");
    } catch (error) {
      status(`Could not load libraries: ${error.message}`, true);
    } finally {
      if (button) button.disabled = false;
    }
  }

  async function save() {
    const button = $("libraries-save");
    button.disabled = true;
    try {
      const data = await api("/libraries", {
        method: "PUT",
        body: JSON.stringify({ library_ids: [...state.selected] }),
      });
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

  $("libraries-search").addEventListener("input", event => {
    state.query = event.target.value;
    render();
  });
  $("libraries-refresh")?.addEventListener("click", refresh);
  $("libraries-save").addEventListener("click", save);
  refresh();
})();
