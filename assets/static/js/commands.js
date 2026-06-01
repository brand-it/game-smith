"use strict";

/** @type {import('socket.io-client').Socket|null} */
let socket = null;

/**
 * Tailwind utility classes for each status, matching _macros/status_badge.html.
 */
const BADGE_CLASSES = {
  running: "inline-flex items-center rounded-full bg-blue-100 px-2.5 py-0.5 text-xs font-semibold text-blue-800",
  finished: "inline-flex items-center rounded-full bg-green-100 px-2.5 py-0.5 text-xs font-semibold text-green-800",
  completed: "inline-flex items-center rounded-full bg-green-100 px-2.5 py-0.5 text-xs font-semibold text-green-800",
  failed: "inline-flex items-center rounded-full bg-red-100 px-2.5 py-0.5 text-xs font-semibold text-red-800",
  stopped: "inline-flex items-center rounded-full bg-yellow-100 px-2.5 py-0.5 text-xs font-semibold text-yellow-800",
  installing: "inline-flex items-center rounded-full bg-purple-100 px-2.5 py-0.5 text-xs font-semibold text-purple-800",
  installed: "inline-flex items-center rounded-full bg-green-100 px-2.5 py-0.5 text-xs font-semibold text-green-800",
  pending: "inline-flex items-center rounded-full bg-gray-100 px-2.5 py-0.5 text-xs font-semibold text-gray-800",
  error: "inline-flex items-center rounded-full bg-red-100 px-2.5 py-0.5 text-xs font-semibold text-red-800",
};

/**
 * Steam Guard / 2FA phrases that indicate user action is needed.
 * Matched case-insensitively against incoming log chunks.
 */
const STEAM_GUARD_PATTERNS = [
  "Please confirm the login in the Steam Mobile app",
  "Steam Guard code:",
  "Two-factor code:",
  "Enter the current code from your Steam Guard Mobile Authenticator",
];

/**
 * Show the Steam Guard action-required banner.
 * Safe to call multiple times — idempotent.
 *
 * @param {string} [message] - Optional message override for the detail line.
 */
function showSteamGuardBanner(message) {
  const banner = document.getElementById("steam-guard-banner");
  if (!banner) return;
  if (message) {
    const msgEl = document.getElementById("steam-guard-message");
    if (msgEl) msgEl.textContent = message;
  }
  banner.classList.remove("hidden");
}

/**
 * Check a log chunk for Steam Guard prompts and surface the banner if found.
 *
 * @param {string} text - Chunk of log output to inspect.
 */
function checkForSteamGuard(text) {
  for (const pattern of STEAM_GUARD_PATTERNS) {
    if (text.toLowerCase().includes(pattern.toLowerCase())) {
      const isEmailCode = pattern.includes("Steam Guard code") || pattern.includes("Two-factor");
      showSteamGuardBanner(
        isEmailCode
          ? "Enter your Steam Guard code in the Steam app or check your email."
          : "Open the Steam Mobile app on your phone and approve the login request."
      );
      return;
    }
  }
}

/**
 * Connect to the Socket.IO /commands namespace and subscribe to a run's log.
 *
 * @param {number} runId - The command run ID to tail.
 * @param {string} initialStatus - The current status from the server ("running" | "finished" | "failed" | "stopped").
 */
function connectToCommand(runId, initialStatus) {
  // If the run is already finished, skip connecting
  if (initialStatus !== "running") {
    return;
  }

  const logEl = document.getElementById("log-output");
  if (!logEl) {
    return;
  }

  socket = io("/commands", {
    transports: ["websocket", "polling"],
  });

  socket.on("connect", () => {
    socket.emit("subscribe", { run_id: runId });
  });

  socket.on("log", (event) => {
    const data = event.data;
    if (data) {
      logEl.textContent += data;
      // Auto-scroll to bottom
      logEl.scrollTop = logEl.scrollHeight;
      // Detect Steam Guard prompts and surface the action banner
      checkForSteamGuard(data);
    }
  });

  socket.on("status", (event) => {
    const status = event.status;
    const exitCode = event.exit_code;

    // Update status badge using Tailwind classes
    const badgeEl = document.getElementById("status-badge");
    if (badgeEl) {
      const classes = BADGE_CLASSES[status] || BADGE_CLASSES.pending;
      badgeEl.innerHTML =
        `<span class="${classes}">${status}</span>`;
    }

    // Update exit code display
    const exitCodeEl = document.getElementById("exit-code");
    if (exitCodeEl && exitCode !== null && exitCode !== undefined) {
      if (exitCode === 0) {
        exitCodeEl.innerHTML = '<span class="text-sm text-green-600">Success</span>';
      } else {
        exitCodeEl.innerHTML = `<span class="text-sm text-red-600">Exit code: ${exitCode}</span>`;
      }
    }

    // Hide live indicator
    const liveEl = document.getElementById("live-indicator");
    if (liveEl) {
      liveEl.classList.add("hidden");
    }

    // Show final status
    const finalEl = document.getElementById("final-status");
    if (finalEl) {
      finalEl.classList.remove("hidden");
      finalEl.textContent = `Run completed with status: ${status}`;
    }

    // Reload to refresh the actions section (Start/Stop buttons depend on server.is_running)
    const reloadTimer = setTimeout(() => {
      window.location.reload();
    }, 2000);
    window.addEventListener("beforeunload", () => clearTimeout(reloadTimer), { once: true });

    socket.disconnect();
    socket = null;
  });

  socket.on("error", (event) => {
    console.error("Socket.IO error:", event.message);
  });
}

/**
 * Disconnect the current Socket.IO connection.
 */
function disconnect() {
  if (socket) {
    socket.disconnect();
    socket = null;
  }
}

// Clean up on page unload
window.addEventListener("beforeunload", disconnect);

// Check initial log content for Steam Guard prompts (page loaded mid-run or after)
document.addEventListener("DOMContentLoaded", () => {
  const logEl = document.getElementById("log-output");
  if (logEl && logEl.textContent) {
    checkForSteamGuard(logEl.textContent);
  }
});

// Export for use in inline scripts
if (typeof window !== "undefined") {
  window.connectToCommand = connectToCommand;
  window.disconnectSocket = disconnect;
}
