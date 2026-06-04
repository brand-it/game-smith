"use strict";

/**
 * Shutdown status poller.
 *
 * Polls /shutdown/status every 500ms and renders a live-updating
 * progress view with per-server status. Declares shutdown complete
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
    var knownServers = {};
    var consecutiveFailures = 0;
    var MAX_FAILURES = 10;

    function escapeHtml(str) {
        var div = document.createElement("div");
        div.textContent = str;
        return div.innerHTML;
    }

    function renderServer(s) {
        var label = statusLabel(s.status);
        var errorHtml = "";
        if (s.status === "failed" && s.error) {
            errorHtml = '<div class="server-error">' + escapeHtml(s.error) + "</div>";
        }
        return (
            '<div class="server-card ' +
            s.status +
            '" data-id="' +
            s.id +
            '">' +
            '<div class="server-dot ' +
            s.status +
            '"></div>' +
            '<div class="server-info">' +
            '<div class="server-name">' +
            escapeHtml(s.name) +
            "</div>" +
            errorHtml +
            "</div>" +
            '<span class="status-badge ' +
            s.status +
            '">' +
            label +
            "</span>" +
            "</div>"
        );
    }

    function statusLabel(status) {
        switch (status) {
            case "pending":
                return "Pending";
            case "stopping":
                return "Stopping";
            case "stopped":
                return "Stopped";
            case "failed":
                return "Failed";
            default:
                return escapeHtml(status);
        }
    }

    function renderServers(servers) {
        if (!servers || servers.length === 0) return;

        serverList.innerHTML = servers.map(renderServer).join("");
    }

    function updateProgress(servers) {
        if (!servers || servers.length === 0) return;
        var done = servers.filter(function (s) {
            return s.status === "stopped" || s.status === "failed";
        }).length;
        var pct = Math.round((done / servers.length) * 100);
        if (progressFill) {
            progressFill.style.width = pct + "%";
        }
        if (serverTally) {
            serverTally.textContent = done + " / " + servers.length + " stopped";
        }
    }

    function updateOverall(shuttingDown) {
        if (shuttingDown && overallText) {
            overallText.textContent = "Stopping game servers\u2026";
        }
    }

    function markComplete() {
        if (overallText) {
            overallText.textContent = "Shutdown complete.";
        }
        if (overallBar) {
            overallBar.classList.add("done");
        }
        if (progressFill) {
            progressFill.classList.add("complete");
            progressFill.style.width = "100%";
        }
        if (shutdownIcon) {
            shutdownIcon.classList.add("stopped");
        }
    }

    function showPolling() {
        if (overallText) {
            overallText.textContent = "Polling for shutdown status\u2026";
        }
    }

    function schedulePoll() {
        if (pollTimer) clearTimeout(pollTimer);
        pollTimer = setTimeout(poll, 500);
    }

    async function poll() {
        try {
            var res = await fetch("/shutdown/status");
            if (!res.ok) throw new Error("HTTP " + res.status);
            var data = await res.json();
            consecutiveFailures = 0;
            renderServers(data.servers || []);
            updateProgress(data.servers || []);
            if (data.shutting_down) {
                updateOverall(true);
                schedulePoll();
            } else {
                markComplete();
            }
        } catch {
            consecutiveFailures++;
            if (consecutiveFailures >= MAX_FAILURES) {
                markComplete();
            } else {
                showPolling();
                schedulePoll();
            }
        }
    }

    poll();
})();
