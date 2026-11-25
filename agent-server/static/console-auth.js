// Protect the console shell and send expired sessions to the shared login page.
const authFetch = window.fetch.bind(window);
const authEndpoints = new Set([
  "/auth/session",
  "/auth/telegram/start",
  "/auth/telegram/status",
  "/auth/logout",
]);

function loginUrl(returnTo = window.location.pathname + window.location.search + window.location.hash) {
  return "/login?return_to=" + encodeURIComponent(returnTo);
}

window.fetch = async (...args) => {
  const response = await authFetch(...args);
  let pathname = "";
  try {
    const requestUrl = args[0] instanceof Request ? args[0].url : args[0];
    pathname = new URL(requestUrl, window.location.href).pathname;
  } catch {
    // Leave unrecognized request inputs to the original fetch implementation.
  }
  if (response.status === 401 && !authEndpoints.has(pathname)) {
    window.location.replace(loginUrl());
  }
  return response;
};

async function authRequest(path, options = {}) {
  const response = await window.fetch(path, {
    ...options,
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
  });
  if (!response.ok) {
    const error = new Error(response.statusText || "Authentication request failed");
    error.status = response.status;
    throw error;
  }
  return response.status === 204 ? null : response.json();
}

let resolveReady;
let rejectReady;
window.emeConsoleReady = new Promise((resolve, reject) => {
  resolveReady = resolve;
  rejectReady = reject;
});

// Keep even the shell header out of view until the server confirms the session.
document.body.style.visibility = "hidden";

(async () => {
  try {
    await authRequest("/auth/session");
    document.body.style.visibility = "visible";
    resolveReady();
  } catch (error) {
    if (error.status === 401) {
      window.location.replace(loginUrl());
      return;
    }
    document.body.style.visibility = "visible";
    rejectReady(error);
  }
})();

document.getElementById("logout")?.addEventListener("click", async event => {
  const button = event.currentTarget;
  button.disabled = true;
  try {
    await authRequest("/auth/logout", { method: "POST" });
    window.location.replace("/login");
  } catch (error) {
    const connection = document.getElementById("connection");
    if (connection) {
      connection.textContent = "Sign out failed: " + error.message;
      connection.className = "hint error";
    }
    button.disabled = false;
  }
});
