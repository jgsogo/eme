// One authentication gate shared by every desktop console view.
const authFetch = window.fetch.bind(window);
const authEndpoints = new Set([
  "/auth/session",
  "/auth/telegram/start",
  "/auth/telegram/status",
  "/auth/logout",
]);
const loginScreen = document.createElement("section");
loginScreen.id = "login-screen";
loginScreen.hidden = true;
loginScreen.innerHTML = `
  <div class="login-card">
    <h2>Sign in with Telegram</h2>
    <p>Use your Telegram account to sign in to the EME web console.</p>
    <button id="login-start" type="button">Continue with Telegram</button>
    <a id="login-link" hidden target="_blank" rel="noopener noreferrer">Open EME bot in Telegram</a>
    <p id="login-status">Your login link will expire after five minutes.</p>
  </div>
`;
document.querySelector("header").insertAdjacentElement("afterend", loginScreen);

const main = document.querySelector("main");
const connection = document.getElementById("connection");
const logout = document.getElementById("logout");
let resolveReady;
let rejectReady;
let showingLogin = false;
window.emeConsoleReady = new Promise((resolve, reject) => {
  resolveReady = resolve;
  rejectReady = reject;
});

function showLogin() {
  if (showingLogin) return;
  showingLogin = true;
  document.body.classList.add("login-active");
  main.hidden = true;
  loginScreen.hidden = false;
  const inviteScreen = document.getElementById("invite-screen");
  if (inviteScreen) inviteScreen.hidden = true;
  logout.hidden = true;
  connection.textContent = "Sign in";
  connection.className = "hint";
}

function showAuthenticatedConsole() {
  showingLogin = false;
  document.body.classList.remove("login-active");
  loginScreen.hidden = true;
  main.hidden = false;
  logout.hidden = false;
  connection.textContent = "Connected";
  connection.className = "hint";
  resolveReady();
}

// Any protected API returning 401 returns the current view to the same login gate.
window.fetch = async (...args) => {
  const response = await authFetch(...args);
  let pathname = "";
  try {
    const requestUrl = args[0] instanceof Request ? args[0].url : args[0];
    pathname = new URL(requestUrl, window.location.href).pathname;
  } catch {
    // Leave unrecognized request inputs to the original fetch implementation.
  }
  if (response.status === 401 && !authEndpoints.has(pathname)) showLogin();
  return response;
};

async function apiAuth(path, options = {}) {
  const response = await window.fetch(path, {
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
    const error = new Error(typeof body === "string" ? body : body?.error || response.statusText);
    error.status = response.status;
    throw error;
  }
  return body;
}

document.getElementById("login-start").addEventListener("click", async () => {
  const popup = window.open("about:blank", "_blank");
  const button = document.getElementById("login-start");
  const status = document.getElementById("login-status");
  const link = document.getElementById("login-link");
  button.disabled = true;
  status.textContent = "Creating a one-time login link…";
  link.hidden = true;
  try {
    const login = await apiAuth("/auth/telegram/start", { method: "POST" });
    if (!login.bot_url) {
      throw new Error("Set TELEGRAM_BOT_USERNAME on agent-server to enable Telegram login links.");
    }
    link.href = login.bot_url;
    link.textContent = login.bot_url;
    link.hidden = false;
    if (popup) popup.location.href = login.bot_url;
    status.textContent = "Open the Telegram URL in the new tab and press Start in your private chat with the bot. Waiting for confirmation…";
    const deadline = Date.now() + login.expires_in_seconds * 1000;
    while (Date.now() < deadline) {
      const result = await apiAuth("/auth/telegram/status", {
        method: "POST",
        body: JSON.stringify({ token: login.token }),
      });
      if (result.authenticated) {
        window.location.reload();
        return;
      }
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    status.textContent = "This login link expired. Please try again.";
  } catch (error) {
    if (popup && !popup.closed) popup.close();
    status.textContent = error.message;
  } finally {
    button.disabled = false;
  }
});

logout.addEventListener("click", async () => {
  logout.disabled = true;
  try {
    await apiAuth("/auth/logout", { method: "POST" });
    window.location.reload();
  } catch (error) {
    connection.textContent = "Sign out failed: " + error.message;
    logout.disabled = false;
  }
});

// Hide protected content before making the first session request.
showLogin();
(async () => {
  try {
    await apiAuth("/auth/session");
    showAuthenticatedConsole();
  } catch (error) {
    if (error.status === 401) {
      showLogin();
      return;
    }
    connection.textContent = "Unable to verify session: " + error.message;
    connection.className = "hint error";
    rejectReady(error);
  }
})();
