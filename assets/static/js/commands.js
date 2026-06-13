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
 * ANSI SGR code to Tailwind class mapping.
 * Only foreground colors and text styles are mapped; background colors are ignored.
 */
const ANSI_STYLES = {
  0: null,
  1: "font-bold",
  2: "opacity-70",
  3: "italic",
  4: "underline",
  30: "text-gray-700",
  31: "text-red-500",
  32: "text-green-500",
  33: "text-yellow-500",
  34: "text-blue-500",
  35: "text-purple-500",
  36: "text-cyan-500",
  37: "text-gray-300",
  90: "text-gray-500",
  91: "text-red-400",
  92: "text-green-400",
  93: "text-yellow-400",
  94: "text-blue-400",
  95: "text-purple-400",
  96: "text-cyan-400",
  97: "text-white",
};

/**
 * Convert ANSI escape sequences to styled HTML spans using Tailwind classes.
 *
 * @param {string} text - Raw text containing ANSI escape sequences.
 * @returns {string} HTML string with styled spans.
 */
function ansiToHtml(text) {
  let html = "";
  let i = 0;
  let currentClasses = [];
  let buffer = "";

  const flushBuffer = () => {
    if (buffer.length === 0) return;
    if (currentClasses.length > 0) {
      html += `<span class="${currentClasses.join(" ")}">${buffer}</span>`;
    } else {
      html += buffer;
    }
    buffer = "";
  };

  while (i < text.length) {
    if (text[i] === "\x1b" && text[i + 1] === "[") {
      let j = i + 2;
      while (j < text.length && text[j] >= "0" && text[j] <= "9") {
        j++;
      }
      if (j < text.length && text[j] === "m") {
        flushBuffer();
        const codes = text.slice(i + 2, j).split(";");
        currentClasses = [];
        for (const code of codes) {
          const num = parseInt(code, 10);
          if (num === 0) {
            currentClasses = [];
            break;
          }
          const cls = ANSI_STYLES[num];
          if (cls) {
            currentClasses.push(cls);
          }
        }
        i = j + 1;
      } else {
        buffer += text[i];
        i++;
      }
    } else {
      buffer += text[i];
      i++;
    }
  }

  flushBuffer();
  return html;
}

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
      // Detect Steam Guard prompts on raw data before conversion
      checkForSteamGuard(data);
      // Convert ANSI escape codes and append as HTML
      logEl.innerHTML += ansiToHtml(data);
      // Auto-scroll to bottom
      logEl.scrollTop = logEl.scrollHeight;
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
    // Check for Steam Guard prompts before converting (raw text includes ANSI)
    checkForSteamGuard(logEl.textContent);
    // Convert ANSI escape codes to styled HTML
    logEl.innerHTML = ansiToHtml(logEl.textContent);
    // Remove initial opacity to avoid flash of unstyled content
    logEl.classList.remove("opacity-0");
  }
});

// Export for use in inline scripts
if (typeof window !== "undefined") {
  window.connectToCommand = connectToCommand;
  window.disconnectSocket = disconnect;
  window.ansiToHtml = ansiToHtml;
}
