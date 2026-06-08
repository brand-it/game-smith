"use strict";

/**
 * Shutdown status poller.
 *
 * Polls /shutdown/status every 500ms and updates the live progress view
 * with per-server status. Updates individual DOM elements by ID rather
 * than rebuilding the entire server list. Declares shutdown complete
 * only after the API explicitly reports so, or after consecutive
 * fetch failures (server process has died).
 */

(function () {
    var serverList = document.getElementById("server-list");
    var overallText = document.getElementById("overall-text");
    var overallBar = document.getElementById("overall-bar");
    var progressFill = document.getElementById("progress-fill");
    var serverTally = document.getElementById("server-tally");
    var shutdownIcon = document.getElementById("shutdown-icon");

    var pollTimer = null;
    var consecutiveFailures = 0;
    var MAX_FAILURES = 12;

    function schedulePoll() {
        if (pollTimer) clearTimeout(pollTimer);
        pollTimer = setTimeout(poll, 500);
    }

    async function poll() {
        try {
            var res = await fetch("/shutdown/status");
            if (!res.ok) throw new Error("HTTP " + res.status);
            var data = await res.json();
            if (!data || typeof data.shutting_down !== "boolean") {
                throw new Error("Invalid response");
            }
            consecutiveFailures = 0;
            updateServers(data.servers || []);
            updateProgress(data.servers || []);
            if (data.shutting_down) {
                updateOverall(true);
                schedulePoll();
            } else {
                verifyServerDead();
            }
        } catch (e) {
            consecutiveFailures++;
            if (consecutiveFailures >= MAX_FAILURES) {
                markComplete();
            } else {
                showPolling();
                schedulePoll();
            }
        }
    }

    function statusLabel(status) {
        switch (status) {
            case "stopping":
                return "Stopping";
            case "stopped":
                return "Stopped";
            case "failed":
                return "Failed";
            default:
                return String(status);
        }
    }

    function updateServers(servers) {
        if (!serverList || !servers || servers.length === 0) return;

        servers.forEach(function (s) {
            var card = document.getElementById("server-" + s.id);
            if (!card) return;

            // Update server card status class
            card.className = "server-card " + s.status;

            // Update server dot
            var dot = card.querySelector(".server-dot");
            if (dot) dot.className = "server-dot " + s.status;

            // Update server name
            var nameEl = card.querySelector(".server-name");
            if (nameEl) nameEl.textContent = s.name;

            // Update error text
            var errorEl = card.querySelector(".server-error");
            if (errorEl) {
                if (s.status === "failed" && s.error) {
                    errorEl.textContent = s.error;
                    errorEl.classList.add("error");
                } else {
                    errorEl.textContent = "";
                    errorEl.classList.remove("error");
                }
            }

            // Update status badge
            var badge = card.querySelector(".status-badge");
            if (badge) {
                badge.className = "status-badge " + s.status;
                badge.textContent = statusLabel(s.status);
            }
        });
    }

    function updateProgress(servers) {
        if (!servers || servers.length === 0) {
            if (progressFill) progressFill.style.width = "100%";
            if (serverTally) serverTally.textContent = "0 / 0 stopped";
            return;
        }
        var done = servers.filter(function (s) {
            return s.status === "stopped" || s.status === "failed";
        }).length;
        var pct = Math.round((done / servers.length) * 100);
        if (progressFill) progressFill.style.width = pct + "%";
        if (serverTally) serverTally.textContent = done + " / " + servers.length + " stopped";
    }

    function updateOverall(shuttingDown) {
        if (shuttingDown && overallText) {
            overallText.textContent = "Stopping game servers\u2026";
        }
    }

    function verifyServerDead() {
        // API reports shutdown is done \u2014 confirm the server process
        // has actually died by checking that /ping stops responding.
        fetch("/ping")
            .then(function () {
                // Server still responding \u2014 wait and try again.
                setTimeout(verifyServerDead, 500);
            })
            .catch(function () {
                // Server no longer responds \u2014 safe to mark complete.
                markComplete();
            });
    }

    function markComplete() {
        if (overallText) overallText.textContent = "Shutdown complete.";
        if (overallBar) overallBar.classList.add("done");
        if (progressFill) {
            progressFill.classList.add("complete");
            progressFill.style.width = "100%";
        }
        if (shutdownIcon) shutdownIcon.classList.add("stopped");
    }

    function showPolling() {
        if (overallText) overallText.textContent = "Polling for shutdown status\u2026";
    }

    poll();
})();
