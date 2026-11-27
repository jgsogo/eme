const $ = id => document.getElementById(id);
const state = { providers: [], view: "cards", sortKey: null, sortDirection: "asc" };

const accessTiers = [
  ["free", "Free"],
  ["free_tier", "Free tier"],
  ["trial", "Trial"],
  ["paid", "Paid"],
  ["unknown", "Unknown"],
];

const testIcon = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 3h6M10 3v6l-5 10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2L14 9V3M8 15h8"/></svg>';
const escapeHtml = value => String(value ?? "").replace(/[&<>"']/g, char => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
}[char]));
const formatCapability = value => String(value ?? "").replace(/_/g, " ").replace(/\b\w/g, char => char.toUpperCase());
const formatAccessTier = value => accessTiers.find(([key]) => key === value)?.[1] ?? "Unknown";

function renderResult(container, result) {
  const success = Boolean(result.success);
  const latency = Number.isFinite(Number(result.latency_ms)) ? " · " + Number(result.latency_ms) + " ms" : "";
  const detail = success ? result.response : result.error;
  const diagnostics = result.diagnostics
    ? '<details class="provider-test-diagnostics"><summary>Response diagnostics</summary><pre>' + escapeHtml(JSON.stringify(result.diagnostics, null, 2)) + "</pre></details>"
    : "";
  container.className = "provider-model-result " + (success ? "success" : "error");
  container.innerHTML = "<strong>" + (success ? "Connection successful" : "Test failed") + "</strong><span>" +
    escapeHtml(latency) + "</span>" + (detail ? "<p>" + escapeHtml(detail) + "</p>" : "") + diagnostics;
}

async function submitTest(provider, model) {
  const response = await fetch("/providers", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ provider, ...(model ? { model } : {}) }),
  });
  const payload = await response.json().catch(() => ({ error: response.statusText }));
  if (!response.ok) throw new Error(payload.error || response.statusText);
  return payload.results || [];
}

async function saveOverride(payload) {
  $("providers-status").textContent = "Saving catalogue override…";
  $("providers-status").className = "usage-status";
  try {
    const response = await fetch("/providers/overrides", {
      method: "PUT",
      headers: { Accept: "application/json", "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    const result = await response.json().catch(() => ({ error: response.statusText }));
    if (!response.ok) throw new Error(result.error || response.statusText);
    const openDialog = document.querySelector(".provider-model-dialog[open]");
    const selection = openDialog ? {
      provider: openDialog.dataset.provider,
      model: openDialog.querySelector("[data-model-enabled]")?.dataset.modelEnabled,
    } : null;
    state.providers = result;
    render();
    if (selection && state.view === "table") {
      const provider = state.providers.find(item => item.name === selection.provider);
      const model = provider?.models?.find(item => item.name === selection.model);
      const host = $("provider-model-dialog-host");
      if (provider && model && host) {
        host.outerHTML = renderModelDialog(provider, model);
        $("provider-model-dialog-host").showModal();
      }
    }
    $("providers-status").textContent = "Catalogue override saved.";
  } catch (error) {
    $("providers-status").textContent = "Could not save catalogue override: " + error.message;
    $("providers-status").className = "usage-status error";
    await load();
  }
}

function renderAccessTier(model) {
  return '<span class="provider-setting provider-access-tier"><span>Access tier</span><strong>' +
    escapeHtml(formatAccessTier(model.access_tier)) + "</strong></span>";
}

const capabilityGroups = [
  {
    label: "Conversation & reasoning",
    keys: ["chat", "coding", "reasoning"],
  },
  {
    label: "Agent integration",
    keys: ["tool_calling", "structured_output"],
  },
  {
    label: "Multimodal",
    keys: ["vision", "video_understanding", "image_generation", "transcription", "audio_understanding", "text_to_speech"],
  },
  {
    label: "Embeddings",
    keys: ["embeddings"],
  },
];

const capabilityDescriptions = {
  chat: "Conversational text generation and understanding",
  coding: "Code generation, explanation and debugging",
  reasoning: "Catalogue classification for reasoning capabilities",
  tool_calling: "Model-generated calls to external tools",
  structured_output: "Outputs constrained to a structured format",
  vision: "Understanding visual inputs such as images",
  video_understanding: "Understanding video and temporal content",
  image_generation: "Generating images from supported inputs",
  transcription: "Converting speech or audio into text",
  audio_understanding: "Understanding audio beyond transcription",
  text_to_speech: "Generating spoken audio from text",
  embeddings: "Generating vector embeddings",
};

function renderDetailValue(value) {
  if (value === null || value === undefined || value === "") return "Not specified";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (Array.isArray(value)) return value.length ? value.map(item => String(item)).join(", ") : "None";
  if (typeof value === "object") return JSON.stringify(value, null, 2);
  return String(value);
}

function renderAdditionalModelMetadata(model) {
  const known = new Set(["name", "enabled", "access_tier", "disabled_reason", "capabilities"]);
  const entries = Object.entries(model)
    .filter(([key]) => !known.has(key))
    .sort(([a], [b]) => a.localeCompare(b));
  if (!entries.length) return "";
  const fields = entries.map(([key, value]) => {
    const complex = value !== null && typeof value === "object";
    return '<div class="provider-dialog-definition' + (complex ? " wide" : "") + '">' +
      '<dt>' + escapeHtml(formatCapability(key)) + '</dt><dd>' +
      (complex ? '<pre>' + escapeHtml(renderDetailValue(value)) + '</pre>' : escapeHtml(renderDetailValue(value))) +
      '</dd></div>';
  }).join("");
  return '<section class="provider-dialog-section provider-dialog-metadata">' +
    '<div class="provider-dialog-section-heading"><div><h4>Additional model metadata</h4>' +
    '<p>Other fields supplied by the catalogue.</p></div></div>' +
    '<dl class="provider-dialog-definition-list">' + fields + '</dl></section>';
}

function renderCapabilityGroups(capabilities) {
  const normalized = capabilities.map(value => String(value).toLowerCase());
  const remaining = new Set(normalized);
  const groups = capabilityGroups.map(group => {
    const values = group.keys.filter(key => remaining.has(key));
    values.forEach(key => remaining.delete(key));
    return { label: group.label, values };
  }).filter(group => group.values.length);
  if (remaining.size) {
    groups.push({ label: "Other capabilities", values: [...remaining].sort() });
  }
  if (!groups.length) {
    return '<p class="provider-no-capabilities">No capabilities are declared for this model.</p>';
  }
  return '<div class="provider-capability-groups">' + groups.map(group =>
    '<div class="provider-capability-group"><h5>' + escapeHtml(group.label) + '</h5>' +
    '<div class="provider-capability-list">' + group.values.map(value =>
      '<span class="provider-capability" title="' + escapeHtml(capabilityDescriptions[value] || "Declared model capability") + '">' +
      escapeHtml(formatCapability(value)) + '</span>'
    ).join("") + '</div></div>'
  ).join("") + '</div>';
}

function renderReason(reason, label) {
  if (!reason) return "";
  return '<div class="provider-dialog-reason"><strong>' + escapeHtml(label) + '</strong><p>' +
    escapeHtml(reason) + '</p></div>';
}

function renderModelDialog(provider, model) {
  const capabilities = Array.isArray(model.capabilities) ? model.capabilities : [];
  const modelStatus = model.enabled ? "Enabled" : "Disabled";
  const providerStatus = provider.enabled ? "Enabled" : "Disabled";
  const providerReason = !provider.enabled ? renderReason(provider.disabled_reason, "Provider disabled") : "";
  const modelReason = !model.enabled ? renderReason(model.disabled_reason, "Model disabled") : "";
  const modelMetadata = renderAdditionalModelMetadata(model);

  return '<dialog id="provider-model-dialog-host" class="modal-dialog provider-model-dialog" ' +
    'data-provider="' + escapeHtml(provider.name) + '" aria-labelledby="provider-model-dialog-title">' +
    '<div class="provider-dialog-shell">' +
      '<header class="provider-dialog-header">' +
        '<div class="provider-dialog-title"><p class="eyebrow">MODEL CATALOGUE</p>' +
          '<h3 id="provider-model-dialog-title">' + escapeHtml(model.name) + '</h3>' +
          '<p>' + escapeHtml(provider.name) + '<span aria-hidden="true"> / </span>' +
            escapeHtml(provider.client) + '</p></div>' +
        '<button type="button" class="secondary icon-button provider-model-dialog-close" ' +
          'data-close-model-dialog aria-label="Close model details" title="Close">' +
          '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">' +
            '<path d="m6 6 12 12M18 6 6 18"/></svg></button>' +
      '</header>' +
      '<div class="provider-dialog-content">' +
        '<div class="provider-dialog-layout">' +
          '<main class="provider-dialog-main">' +
            '<section class="provider-dialog-section provider-dialog-profile">' +
              '<div class="provider-dialog-section-heading"><div><h4>Model profile</h4>' +
                '<p>Catalogue availability and access classification.</p></div></div>' +
              '<div class="provider-dialog-profile-grid">' +
                '<div class="provider-dialog-profile-item"><span>Availability</span>' +
                  '<strong class="provider-dialog-state' + (model.enabled ? " is-enabled" : " is-disabled") + '">' +
                    modelStatus + '</strong></div>' +
                '<div class="provider-dialog-profile-item"><span>Access tier</span>' +
                  '<strong>' + escapeHtml(formatAccessTier(model.access_tier)) + '</strong></div>' +
              '</div>' + modelReason +
            '</section>' +
            '<section class="provider-dialog-section provider-dialog-capabilities">' +
              '<div class="provider-dialog-section-heading"><div><h4>Capabilities</h4>' +
                '<p>Declared compatibility features, grouped by the kind of work they support.</p></div>' +
                '<span class="provider-dialog-count">' + capabilities.length + '</span></div>' +
              renderCapabilityGroups(capabilities) +
              '<p class="provider-dialog-footnote">These are catalogue classifications, not standardized guarantees or quality measurements.</p>' +
            '</section>' +
            modelMetadata +
          '</main>' +
          '<aside class="provider-dialog-aside">' +
            '<section class="provider-dialog-section provider-dialog-provider">' +
              '<div class="provider-dialog-section-heading"><div><h4>Provider</h4>' +
                '<p>Connection context for this model.</p></div></div>' +
              '<dl class="provider-dialog-provider-list">' +
                '<div><dt>Provider</dt><dd>' + escapeHtml(provider.name) + '</dd></div>' +
                '<div><dt>Client adapter</dt><dd>' + escapeHtml(provider.client) + '</dd></div>' +
                '<div><dt>Provider availability</dt><dd><span class="provider-dialog-state' +
                  (provider.enabled ? " is-enabled" : " is-disabled") + '">' + providerStatus + '</span></dd></div>' +
              '</dl>' + providerReason +
            '</section>' +
          '</aside>' +
        '</div>' +
      '</div>' +
      '<footer class="provider-dialog-actions">' +
        '<label class="provider-dialog-toggle"><input type="checkbox" data-model-enabled="' +
          escapeHtml(model.name) + '"' + (model.enabled ? " checked" : "") + '>' +
          '<span><strong>Model enabled</strong><small>Allow the runtime to select this model.</small></span></label>' +
        '<div class="provider-dialog-action-buttons">' +
          '<button type="button" class="secondary provider-reset-button" data-reset-model="' +
            escapeHtml(model.name) + '">Reset to TOML default</button>' +
          '<button type="button" class="secondary provider-test-button" data-test-model="' +
            escapeHtml(model.name) + '">' + testIcon + '<span>Test model</span></button>' +
        '</div>' +
        '<div class="provider-model-result" aria-live="polite"></div>' +
      '</footer>' +
    '</div></dialog>';
}
function sortValue(provider, model, key) {
  switch (key) {
    case "model": return model.name || "";
    case "provider": return provider.name || "";
    case "access_tier": return formatAccessTier(model.access_tier);
    case "status": return model.enabled ? "Enabled" : "Disabled";
    case "capabilities": return (model.capabilities || []).map(formatCapability).join(", ");
    default: return "";
  }
}

function renderTable(providers) {
  const models = providers.flatMap(provider => provider.models.map(model => ({ provider, model })));
  if (state.sortKey) {
    const direction = state.sortDirection === "asc" ? 1 : -1;
    models.sort((left, right) => {
      const a = sortValue(left.provider, left.model, state.sortKey);
      const b = sortValue(right.provider, right.model, state.sortKey);
      const comparison = String(a).localeCompare(String(b), undefined, {
        numeric: true, sensitivity: "base",
      });
      return comparison * direction;
    });
  }
  const headers = [
    ["model", "Model"],
    ["provider", "Provider"],
    ["access_tier", "Access tier"],
    ["status", "Status"],
    ["capabilities", "Capabilities"],
  ];
  const heading = headers.map(([key, label]) => {
    const active = state.sortKey === key;
    const direction = active ? state.sortDirection : "none";
    const indicator = active ? (state.sortDirection === "asc" ? " ↑" : " ↓") : "";
    return '<th scope="col" aria-sort="' + (active ? (state.sortDirection === "asc" ? "ascending" : "descending") : "none") + '">' +
      '<button type="button" class="provider-sort-button" data-sort-key="' + key + '" aria-label="Sort by ' +
      label + (active ? ", " + state.sortDirection : "") + '">' + label + '<span aria-hidden="true">' +
      indicator + '</span></button></th>';
  }).join("");
  const rows = models.map(({ provider, model }) => {
    const capabilities = (model.capabilities || []).map(formatCapability).join(", ") || "—";
    return '<tr data-model-row data-provider="' + escapeHtml(provider.name) + '" data-model="' +
      escapeHtml(model.name) + '" tabindex="0" role="button" aria-label="Show details for ' +
      escapeHtml(model.name) + '">' +
      '<td class="provider-table-model">' + escapeHtml(model.name) + '</td>' +
      '<td>' + escapeHtml(provider.name) + '</td>' +
      '<td>' + escapeHtml(formatAccessTier(model.access_tier)) + '</td>' +
      '<td><span class="provider-table-status' + (model.enabled ? " enabled" : "") + '">' +
      (model.enabled ? "Enabled" : "Disabled") + '</span></td>' +
      '<td class="provider-table-capabilities" title="' + escapeHtml(capabilities) + '">' +
      escapeHtml(capabilities) + '</td></tr>';
  }).join("");
  const dialog = '<dialog id="provider-model-dialog-host" class="modal-dialog provider-model-dialog" aria-label="Model details"></dialog>';
  if (!rows) return '<div class="usage-empty">' +
    (providers.length ? "No models match this filter." : "No providers are configured.") + "</div>" + dialog;
  return '<div class="usage-table-wrap provider-list-table-wrap"><table class="usage-table provider-list-table">' +
    '<thead><tr>' + heading + '</tr></thead><tbody>' + rows + '</tbody></table></div>' + dialog;
}
function render() {
  const query = $("providers-search").value.trim().toLocaleLowerCase();
  const providers = state.providers.map(provider => {
    const allModels = provider.models || [];
    const models = allModels.filter(model =>
      [provider.name, provider.client, model.name, model.access_tier, ...(model.capabilities || [])]
        .some(value => String(value || "").toLocaleLowerCase().includes(query)));
    return { ...provider, allModels, models };
  }).filter(provider =>
    !query || provider.name.toLocaleLowerCase().includes(query) ||
    provider.client.toLocaleLowerCase().includes(query) || provider.models.length);

  const visibleModels = providers.reduce((total, provider) => total + provider.models.length, 0);
  if (state.view === "table") {
    $("providers-list").innerHTML = renderTable(providers);
  } else {
    $("providers-list").innerHTML = providers.length ? providers.map(provider => {
      const models = provider.models.map(model => {
        const capabilities = (model.capabilities || [])
          .map(capability => '<span class="provider-capability">' + escapeHtml(formatCapability(capability)) + "</span>")
          .join("");
        const capabilityCount = (model.capabilities || []).length;
        const disabledReason = model.disabled_reason
          ? '<p class="provider-disabled-reason"><strong>Disabled reason</strong> ' + escapeHtml(model.disabled_reason) + "</p>"
          : "";
        return '<section class="provider-model" data-model="' + escapeHtml(model.name) + '">' +
          '<div class="provider-model-top">' +
            '<div class="provider-model-identity"><h4>' + escapeHtml(model.name) + "</h4>" +
              renderAccessTier(model) + '</div>' +
            '<div class="provider-model-settings">' +
              '<label class="provider-setting provider-enabled"><input type="checkbox" data-model-enabled="' +
                escapeHtml(model.name) + '"' + (model.enabled ? " checked" : "") +
                '><span>Enabled</span></label>' +
              '<button class="secondary provider-reset-button" type="button" data-reset-model="' +
                escapeHtml(model.name) + '" title="Restore TOML default">Reset</button>' +
            "</div></div>" + disabledReason +
          '<div class="provider-model-details">' +
            '<div class="provider-capabilities"><span class="provider-field-label">Capabilities <span class="provider-capability-count">' +
              capabilityCount + "</span></span>" +
              '<div class="provider-capability-list">' +
                (capabilities || '<span class="provider-no-capabilities">No capabilities declared</span>') +
              "</div></div>" +
            '<div class="provider-model-test-group"><button class="secondary provider-test-button" type="button" data-test-model="' +
              escapeHtml(model.name) + '">' + testIcon + "<span>Test model</span></button>" +
              '<div class="provider-model-result" aria-live="polite"></div></div>' +
          "</div></section>";
      }).join("");

      const disabledReason = provider.disabled_reason
        ? '<p class="provider-disabled-reason"><strong>Disabled reason</strong> ' + escapeHtml(provider.disabled_reason) + "</p>"
        : "";
      const modelCount = provider.allModels.length;
      const visibleCount = provider.models.length;
      return '<article class="panel provider-card" data-provider="' + escapeHtml(provider.name) + '">' +
        '<header class="provider-card-header">' +
          '<div class="provider-card-identity">' +
            '<span class="provider-card-kicker">PROVIDER</span>' +
            '<div class="provider-title-line"><h3>' + escapeHtml(provider.name) + "</h3>" +
              '<span class="provider-models-count">' + modelCount + " " +
                (modelCount === 1 ? "model" : "models") + "</span></div>" +
            '<div class="provider-client-line"><span class="provider-field-label">Client adapter</span>' +
              '<strong class="provider-client-name">' + escapeHtml(provider.client) + "</strong></div>" +
            disabledReason +
          "</div>" +
          '<div class="provider-card-actions">' +
            '<label class="provider-setting provider-enabled"><input type="checkbox" data-provider-enabled' +
              (provider.enabled ? " checked" : "") + '><span>Provider enabled</span></label>' +
            '<button class="secondary provider-reset-button" type="button" data-reset-provider title="Restore TOML defaults">Reset</button>' +
            '<button class="secondary provider-test-button provider-test-all" type="button" data-test-provider>' +
              testIcon + "<span>Test all models</span></button>" +
          "</div></header>" +
        '<div class="provider-model-section-heading"><h4>Models</h4>' +
          (query ? '<span>' + visibleCount + " of " + modelCount + " shown</span>" : '<span>' + modelCount + (modelCount === 1 ? " configured model" : " configured models") + "</span>") +
        "</div>" +
        '<div class="provider-model-list">' + (models || '<p class="provider-no-models">No models match this filter.</p>') +
        "</div></article>";
    }).join("") : '<div class="usage-empty">' +
      (query ? 'No providers or models match this filter. <button type="button" class="secondary" data-clear-search>Clear filter</button>' : "No providers are configured.") +
      "</div>";
  }

  document.querySelectorAll("[data-provider-view]").forEach(button => {
    button.setAttribute("aria-pressed", String(button.dataset.providerView === state.view));
  });
  $("providers-status").textContent = query
    ? providers.length + " of " + state.providers.length + " providers · " + visibleModels + " matching models"
    : providers.length + " providers · " + visibleModels + " configured models";
}

async function testModel(button) {
  const card = button.closest("[data-provider]");
  const row = button.closest(".provider-model") || button.closest(".provider-model-dialog");
  const result = row.querySelector(".provider-model-result");
  button.disabled = true;
  button.innerHTML = "<span>Testing…</span>";
  result.className = "provider-model-result pending";
  result.textContent = "Sending a test completion request…";
  try {
    const results = await submitTest(card.dataset.provider, button.dataset.testModel);
    if (!results.length) throw new Error("The server returned no test result");
    renderResult(result, results[0]);
  } catch (error) {
    result.className = "provider-model-result error";
    result.textContent = "Test failed: " + error.message;
  } finally {
    button.disabled = false;
    button.innerHTML = testIcon + "<span>Test model</span>";
  }
}

async function testProvider(button) {
  const card = button.closest("[data-provider]");
  const rows = [...card.querySelectorAll(".provider-model")];
  const modelButtons = rows.map(row => row.querySelector("[data-test-model]"));
  button.disabled = true;
  button.innerHTML = "<span>Testing…</span>";
  rows.forEach(row => {
    const result = row.querySelector(".provider-model-result");
    result.className = "provider-model-result pending";
    result.textContent = "Waiting for test…";
  });
  modelButtons.forEach(modelButton => { modelButton.disabled = true; });
  try {
    const results = await submitTest(card.dataset.provider, null);
    results.forEach(result => {
      const row = rows.find(candidate => candidate.querySelector("[data-test-model]")?.dataset.testModel === result.model);
      if (row) renderResult(row.querySelector(".provider-model-result"), result);
    });
  } catch (error) {
    rows.forEach(row => {
      const result = row.querySelector(".provider-model-result");
      if (result.classList.contains("pending")) {
        result.className = "provider-model-result error";
        result.textContent = "Test failed: " + error.message;
      }
    });
  } finally {
    button.disabled = false;
    button.innerHTML = testIcon + "<span>Test all models</span>";
    modelButtons.forEach(modelButton => { modelButton.disabled = false; });
  }
}

async function load() {
  $("providers-status").textContent = "Loading configured providers…";
  $("providers-list").innerHTML = '<p class="usage-empty" role="status">Loading providers…</p>';
  try {
    const response = await fetch("/providers", { headers: { Accept: "application/json" } });
    if (!response.ok) throw new Error(await response.text() || response.statusText);
    state.providers = await response.json();
    render();
  } catch (error) {
    $("providers-status").textContent = "Could not load providers.";
    $("providers-status").className = "usage-status error";
    $("providers-list").innerHTML = '<div class="usage-empty" role="alert">Could not load providers: ' +
      escapeHtml(error.message) + '<p><button type="button" class="secondary" data-retry>Try again</button></p></div>';
  }
}

function providerFromControl(control) {
  return control.closest("[data-provider]")?.dataset.provider;
}

$("providers-search").addEventListener("input", render);
document.querySelectorAll("[data-provider-view]").forEach(button => {
  button.addEventListener("click", () => {
    state.view = button.dataset.providerView;
    render();
  });
});
$("providers-list").addEventListener("keydown", event => {
  const row = event.target.closest("[data-model-row]");
  if (row && (event.key === "Enter" || event.key === " ")) {
    event.preventDefault();
    row.click();
  }
});
$("providers-list").addEventListener("click", event => {
  const sortButton = event.target.closest("[data-sort-key]");
  if (sortButton) {
    const key = sortButton.dataset.sortKey;
    if (state.sortKey === key) {
      state.sortDirection = state.sortDirection === "asc" ? "desc" : "asc";
    } else {
      state.sortKey = key;
      state.sortDirection = "asc";
    }
    render();
    return;
  }
  const closeDialog = event.target.closest("[data-close-model-dialog]");
  if (closeDialog) {
    closeDialog.closest("dialog").close();
    return;
  }
  const modelRow = event.target.closest("[data-model-row]");
  if (modelRow && !event.target.closest("button")) {
    const provider = state.providers.find(item => item.name === modelRow.dataset.provider);
    const model = provider?.models?.find(item => item.name === modelRow.dataset.model);
    const dialogHost = $("provider-model-dialog-host");
    if (provider && model && dialogHost) {
      dialogHost.outerHTML = renderModelDialog(provider, model);
      const dialog = $("provider-model-dialog-host") || $("providers-list").querySelector("dialog.provider-model-dialog");
      dialog.showModal();
    }
    return;
  }
  if (event.target.closest("[data-clear-search]")) {
    $("providers-search").value = "";
    render();
    return;
  }
  if (event.target.closest("[data-retry]")) {
    load();
    return;
  }
  const resetModel = event.target.closest("[data-reset-model]");
  if (resetModel) {
    saveOverride({ provider: providerFromControl(resetModel), model: resetModel.dataset.resetModel, reset: true });
    return;
  }
  const resetProvider = event.target.closest("[data-reset-provider]");
  if (resetProvider) {
    saveOverride({ provider: providerFromControl(resetProvider), reset: true });
    return;
  }
  const modelButton = event.target.closest("[data-test-model]");
  if (modelButton) {
    testModel(modelButton);
    return;
  }
  const providerButton = event.target.closest("[data-test-provider]");
  if (providerButton) testProvider(providerButton);
});

$("providers-list").addEventListener("change", event => {
  const control = event.target;
  const provider = providerFromControl(control);
  if (!provider) return;

  if (control.matches("[data-provider-enabled]")) {
    let reason = null;
    if (!control.checked) {
      reason = window.prompt("Why are you disabling this provider? (Optional)", "") ?? null;
      if (reason === null) {
        control.checked = true;
        return;
      }
    }
    saveOverride({ provider, enabled: control.checked, disabled_reason: reason });
    return;
  }

  if (control.matches("[data-model-enabled]")) {
    const model = control.dataset.modelEnabled;
    let reason = null;
    if (!control.checked) {
      reason = window.prompt("Why are you disabling this model? (Optional)", "") ?? null;
      if (reason === null) {
        control.checked = true;
        return;
      }
    }
    saveOverride({ provider, model, enabled: control.checked, disabled_reason: reason });
    return;
  }

});

load();
