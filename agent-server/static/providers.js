const $ = id => document.getElementById(id);
const state = { providers: [] };

const accessTiers = [
  ["free", "Free"],
  ["free_tier", "Free tier"],
  ["trial", "Trial"],
  ["paid", "Paid"],
  ["unknown", "Unknown"],
];

const testIcon = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 3h6M10 3v6l-5 10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2L14 9V3M8 15h8"/></svg>';
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
    state.providers = result;
    render();
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

function render() {
  const query = $("providers-search").value.trim().toLocaleLowerCase();
  const providers = state.providers.map(provider => {
    const models = (provider.models || []).filter(model =>
      [provider.name, provider.client, model.name, model.access_tier, ...(model.capabilities || [])]
        .some(value => String(value || "").toLocaleLowerCase().includes(query)));
    return { ...provider, models };
  }).filter(provider =>
    !query || provider.name.toLocaleLowerCase().includes(query) ||
    provider.client.toLocaleLowerCase().includes(query) || provider.models.length);

  const visibleModels = providers.reduce((total, provider) => total + provider.models.length, 0);
  $("providers-list").innerHTML = providers.length ? providers.map(provider => {
    const models = provider.models.map(model => {
      const capabilities = (model.capabilities || [])
        .map(capability => '<span class="provider-capability">' + escapeHtml(formatCapability(capability)) + "</span>")
        .join("");
      const capabilityCount = (model.capabilities || []).length;
      const disabledReason = model.disabled_reason
        ? '<p class="provider-disabled-reason">Disabled: ' + escapeHtml(model.disabled_reason) + "</p>"
        : "";
      return '<section class="provider-model" data-model="' + escapeHtml(model.name) + '">' +
        '<div class="provider-model-header">' +
          '<div class="provider-model-identity"><h4>' + escapeHtml(model.name) + "</h4>" +
            '<span class="provider-model-count">' + capabilityCount + " " +
            (capabilityCount === 1 ? "capability" : "capabilities") + "</span></div>" +
          '<div class="provider-model-settings">' +
            '<label class="provider-setting provider-enabled"><input type="checkbox" data-model-enabled="' +
              escapeHtml(model.name) + '"' + (model.enabled ? " checked" : "") +
              '><span>Enabled</span></label>' +
            renderAccessTier(model) +
            '<button class="secondary provider-reset-button" type="button" data-reset-model="' +
              escapeHtml(model.name) + '">Reset</button>' +
          "</div></div>" + disabledReason +
        '<div class="provider-model-test-group"><button class="secondary provider-test-button" type="button" data-test-model="' +
          escapeHtml(model.name) + '">' + testIcon + "<span>Test model</span></button>" +
          '<div class="provider-model-result" aria-live="polite"></div></div>' +
        '<div class="provider-capabilities"><span class="provider-field-label">Capabilities</span>' +
          '<div class="provider-capability-list">' +
            (capabilities || '<span class="provider-no-capabilities">None declared</span>') +
          "</div></div></section>";
    }).join("");

    const disabledReason = provider.disabled_reason
      ? '<p class="provider-disabled-reason">Disabled: ' + escapeHtml(provider.disabled_reason) + "</p>"
      : "";
    return '<article class="panel provider-card" data-provider="' + escapeHtml(provider.name) + '">' +
      '<header class="provider-card-header"><div class="provider-card-identity">' +
        '<span class="provider-card-kicker">PROVIDER</span><h3>' + escapeHtml(provider.name) + "</h3>" +
        '<p><span class="provider-field-label">Client adapter</span><span class="provider-client-name">' +
          escapeHtml(provider.client) + "</span></p>" + disabledReason +
      '</div><div class="provider-card-actions">' +
        '<span class="provider-models-count">' + (provider.models || []).length + " " +
          ((provider.models || []).length === 1 ? "model" : "models") + "</span>" +
        '<label class="provider-setting provider-enabled"><input type="checkbox" data-provider-enabled' +
          (provider.enabled ? " checked" : "") + '><span>Enabled</span></label>' +
        '<button class="secondary provider-reset-button" type="button" data-reset-provider>Reset</button>' +
        '<button class="secondary provider-test-button provider-test-all" type="button" data-test-provider>' +
          testIcon + "<span>Test all models</span></button></div></header>" +
      '<div class="provider-model-list">' + (models || '<p class="provider-no-models">No models match this filter.</p>') +
      "</div></article>";
  }).join("") : '<div class="usage-empty">' +
    (query ? 'No providers or models match this filter. <button type="button" class="secondary" data-clear-search>Clear filter</button>' : "No providers are configured.") +
    "</div>";

  $("providers-status").textContent = query
    ? providers.length + " of " + state.providers.length + " providers · " + visibleModels + " matching models"
    : providers.length + " providers · " + visibleModels + " configured models";
}

async function testModel(button) {
  const card = button.closest("[data-provider]");
  const row = button.closest(".provider-model");
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
$("providers-list").addEventListener("click", event => {
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
