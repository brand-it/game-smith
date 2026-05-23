"use strict";

/** @type {import('socket.io-client').Socket|null} */
let socket = null;

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
    }
  });

  socket.on("status", (event) => {
    const status = event.status;
    const exitCode = event.exit_code;

    // Update status badge
    const badge = document.getElementById("status-badge");
    if (badge) {
      badge.textContent = status;
      badge.className = `badge badge-${status}`;
    }

    // Update exit code
    const exitCodeEl = document.getElementById("exit-code");
    if (exitCodeEl && exitCode !== null && exitCode !== undefined) {
      exitCodeEl.textContent = exitCode;
    }

    // Hide live indicator
    const liveEl = document.getElementById("live-indicator");
    if (liveEl) {
      liveEl.style.display = "none";
    }

    // Show final status
    const finalEl = document.getElementById("final-status");
    if (finalEl) {
      finalEl.style.display = "block";
      finalEl.textContent = `Run completed with status: ${status}`;
    }

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

// Export for use in inline scripts
if (typeof window !== "undefined") {
  window.connectToCommand = connectToCommand;
  window.disconnectSocket = disconnect;
}
