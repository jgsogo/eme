const params = new URLSearchParams(window.location.search);
const requestedDestination = params.get("return_to") || "/";
const returnTo = requestedDestination.startsWith("/")
  && !requestedDestination.startsWith("//")
  && !requestedDestination.includes("\\")
  ? requestedDestination
  : "/";

const button = document.getElementById("login-start");
const status = document.getElementById("login-status");
const link = document.getElementById("login-link");

async function authRequest(path, options = {}) {
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
  if (!response.ok && response.status !== 202) {
    throw new Error(typeof body === "string" ? body : body?.error || response.statusText);
  }
  return body;
}

button.addEventListener("click", async () => {
  const popup = window.open("about:blank", "_blank");
  button.disabled = true;
  status.textContent = "Creating a one-time login link…";
  link.hidden = true;

  try {
    const login = await authRequest("/auth/telegram/start", { method: "POST" });
    if (!login.bot_url) {
      throw new Error("Set TELEGRAM_BOT_USERNAME on agent-server to enable Telegram login links.");
    }

    link.href = login.bot_url;
    link.textContent = "Open EME bot in Telegram";
    link.hidden = false;
    if (popup) popup.location.href = login.bot_url;

    status.textContent = "Open the Telegram bot and press Start. Waiting for confirmation…";
    const deadline = Date.now() + login.expires_in_seconds * 1000;
    while (Date.now() < deadline) {
      const result = await authRequest("/auth/telegram/status", {
        method: "POST",
        body: JSON.stringify({ token: login.token }),
      });
      if (result.authenticated) {
        window.location.replace(returnTo);
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
