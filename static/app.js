(function () {
    "use strict";

    // ── Line badge colors ──────────────────────────────────────────────────
    var LINE_COLORS = {
        "A": "#c8102e",
        "B": "#7b2d8b",
        "C": "#e07b10",
        "D": "#2e8b3c",
        "E": "#6f2c91",
        "F": "#c5145b",
        "G": "#5c6bc0"
    };
    var DEFAULT_COLOR = "#546e7a";

    // ── State ──────────────────────────────────────────────────────────────
    var departureData = null;
    var currentMonitoringRef = null;   // set from WS messages
    var ws = null;
    var reconnectDelay = 1000;

    // Config panel
    var allStops = null;               // cached stop list from /api/stops

    // ── WebSocket ──────────────────────────────────────────────────────────

    function connect() {
        var proto = location.protocol === "https:" ? "wss:" : "ws:";
        ws = new WebSocket(proto + "//" + location.host + "/ws");

        ws.onopen = function () {
            reconnectDelay = 1000;
            setDotColor("green");
        };

        ws.onmessage = function (event) {
            try {
                departureData = JSON.parse(event.data);
                // Keep track of which stop is currently shown
                if (departureData.monitoring_ref) {
                    currentMonitoringRef = departureData.monitoring_ref;
                }
                renderBoard();
                updateDotFromData();
            } catch (e) {
                console.error("Failed to parse departure data:", e);
            }
        };

        ws.onclose = function () {
            setDotColor("red");
            setTimeout(connect, reconnectDelay);
            reconnectDelay = Math.min(reconnectDelay * 2, 30000);
        };

        ws.onerror = function () {
            ws.close();
        };
    }

    // ── Board rendering ────────────────────────────────────────────────────

    function renderBoard() {
        if (!departureData) return;

        var container = document.getElementById("departures");
        container.innerHTML = "";

        // ── Offline / no-service message ───────────────────────────────────
        if (departureData.offline_message) {
            var msg = document.createElement("div");
            msg.className = "offline-message";
            msg.textContent = departureData.offline_message;
            container.appendChild(msg);
            return;
        }

        departureData.lines.forEach(function (lineDep) {
            var row = document.createElement("div");
            row.className = "departure-row";

            var badge = document.createElement("div");
            badge.className = "line-badge";
            badge.textContent = lineDep.line;
            badge.style.backgroundColor = LINE_COLORS[lineDep.line] || DEFAULT_COLOR;

            var dest = document.createElement("div");
            dest.className = "destination";
            dest.textContent = lineDep.destination;

            var next = document.createElement("div");
            next.className = "time-cell";
            if (lineDep.departures.length > 0) {
                next.dataset.expected = lineDep.departures[0].expected;
                next.dataset.realtime = String(lineDep.departures[0].is_real_time);
            }

            var following = document.createElement("div");
            following.className = "time-cell";
            if (lineDep.departures.length > 1) {
                following.dataset.expected = lineDep.departures[1].expected;
                following.dataset.realtime = String(lineDep.departures[1].is_real_time);
            }

            row.appendChild(badge);
            row.appendChild(dest);
            row.appendChild(next);
            row.appendChild(following);
            container.appendChild(row);
        });

        updateTimers();
    }

    // ── Countdown timers ───────────────────────────────────────────────────

    function updateTimers() {
        var now = Date.now();
        document.querySelectorAll(".time-cell").forEach(function (cell) {
            var expected = cell.dataset.expected;
            if (!expected) {
                cell.innerHTML = '<span class="unit">—</span>';
                return;
            }

            var diffMs = new Date(expected).getTime() - now;
            var cls = cell.dataset.realtime === "false" ? " theoretical" : "";

            if (diffMs < 0) {
                cell.innerHTML = '<span class="arriving' + cls + '">Arr.</span>';
            } else {
                var diffMin = Math.floor(diffMs / 60000);
                if (diffMin === 0) {
                    cell.innerHTML =
                        '<span class="arriving' + cls + '">&lt;&nbsp;1</span>' +
                        '<span class="unit">min</span>';
                } else {
                    cell.innerHTML =
                        '<span class="' + cls.trim() + '">' + diffMin + '</span>' +
                        '<span class="unit">min</span>';
                }
            }
        });
    }

    // ── Clock ──────────────────────────────────────────────────────────────

    function updateClock() {
        var now = new Date();
        document.getElementById("clock").textContent = now.toLocaleTimeString("fr-FR", {
            hour: "2-digit", minute: "2-digit"
        });
        document.getElementById("date").textContent = now.toLocaleDateString("fr-FR", {
            day: "numeric", month: "long", year: "numeric"
        });
    }

    // ── Status dot ─────────────────────────────────────────────────────────

    function setDotColor(color) {
        document.getElementById("status-dot").className = color;
    }

    function updateDotFromData() {
        if (!departureData) return;
        setDotColor(departureData.offline_message ? "yellow" : "green");
    }

    // ── Status overlay ─────────────────────────────────────────────────────

    function openStatus() {
        document.getElementById("status-overlay").classList.remove("hidden");
        fetchAndRenderStatus();
    }

    function closeStatus() {
        document.getElementById("status-overlay").classList.add("hidden");
    }

    function fetchAndRenderStatus() {
        var content = document.getElementById("status-content");
        content.innerHTML = '<div class="config-loading-msg">Chargement…</div>';

        fetch("/api/status")
            .then(function (r) {
                if (!r.ok) throw new Error("HTTP " + r.status);
                return r.json();
            })
            .then(renderStatus)
            .catch(function (err) {
                content.innerHTML = '<div class="config-loading-msg">Erreur\u00a0: ' + err.message + '</div>';
            });
    }

    function renderStatus(data) {
        var content = document.getElementById("status-content");
        content.innerHTML = "";

        function row(label, value, cls) {
            var el = document.createElement("div");
            el.className = "status-row";
            el.innerHTML =
                '<div class="status-label">' + label + "</div>" +
                '<div class="status-value' + (cls ? " " + cls : "") + '">' + value + "</div>";
            content.appendChild(el);
        }

        row("Arrêt surveillé", escHtml(data.monitoring_ref || "—"), "highlight");

        row("Mode",
            data.simulation ? "Simulation (pas d\u2019appel API)" : "Temps réel",
            data.simulation ? "highlight" : "");

        row("Intervalle de scrutation", data.polling_interval_minutes + "\u00a0min");

        if (data.next_poll_at && data.next_poll_at > 0) {
            var diffSec = Math.round((data.next_poll_at * 1000 - Date.now()) / 1000);
            var nextStr = diffSec <= 0 ? "Immédiatement"
                : diffSec < 120 ? "dans\u00a0" + diffSec + "\u00a0s"
                : "dans\u00a0" + Math.round(diffSec / 60) + "\u00a0min";
            row("Prochain appel API", nextStr);
        } else {
            row("Prochain appel API", "—", "dim");
        }

        row("Interrogation permanente", data.always_query ? "Oui" : "Non");

        if (!data.always_query) {
            row("Fenêtre active", data.in_window ? "Oui" : "Non",
                data.in_window ? "" : "dim");
            row("Plages horaires",
                escHtml(data.query_intervals_raw || "—"),
                data.query_intervals_raw ? "" : "dim");
        }

        var srvTime = "—";
        if (data.server_local_time) {
            try {
                srvTime = new Date(data.server_local_time).toLocaleTimeString("fr-FR", {
                    hour: "2-digit", minute: "2-digit", second: "2-digit", timeZoneName: "short"
                });
            } catch (_) { srvTime = data.server_local_time; }
        }
        row("Heure serveur", srvTime, "dim");
    }

    function escHtml(s) {
        return String(s)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;");
    }

    // ── Configuration panel ────────────────────────────────────────────────

    // Currently selected stop info used while navigating level 2
    var pendingStop = null;   // { code, name } for the logical stop being drilled into

    function openConfig() {
        document.getElementById("config-overlay").classList.remove("hidden");
        showLevel1();
        if (allStops === null) {
            loadStops();
        } else {
            renderStops(allStops);
            document.getElementById("config-filter").focus();
        }
    }

    function closeConfig() {
        document.getElementById("config-overlay").classList.add("hidden");
    }

    // ── Level 1 helpers ────────────────────────────────────────────────────

    function showLevel1() {
        document.getElementById("config-level1").style.display = "";
        document.getElementById("config-level2").classList.add("hidden");
        document.getElementById("config-filter").value = "";
        pendingStop = null;
    }

    function showLevel2() {
        document.getElementById("config-level1").style.display = "none";
        document.getElementById("config-level2").classList.remove("hidden");
    }

    function loadStops() {
        var loadingEl = document.getElementById("config-loading");
        var gridEl    = document.getElementById("stops-grid");

        loadingEl.textContent = "Chargement des arrêts…";
        loadingEl.classList.remove("hidden");
        gridEl.innerHTML = "";

        fetch("/api/stops")
            .then(function (r) {
                if (!r.ok) throw new Error("HTTP " + r.status);
                return r.json();
            })
            .then(function (data) {
                allStops = data.stops || [];
                loadingEl.classList.add("hidden");
                renderStops(allStops);
                document.getElementById("config-filter").focus();
            })
            .catch(function (err) {
                loadingEl.textContent = "Erreur de chargement : " + err.message;
            });
    }

    function renderStops(stops) {
        var grid = document.getElementById("stops-grid");
        grid.innerHTML = "";

        if (stops.length === 0) {
            var empty = document.createElement("div");
            empty.className = "grid-empty";
            empty.textContent = "Aucun arrêt trouvé";
            grid.appendChild(empty);
            return;
        }

        stops.forEach(function (stop) {
            var card = document.createElement("div");
            card.className = "stop-card" +
                (stop.code === currentMonitoringRef ? " selected" : "");

            var nameEl = document.createElement("div");
            nameEl.className = "stop-name";
            nameEl.textContent = stop.name;

            var codeEl = document.createElement("div");
            codeEl.className = "stop-code";
            codeEl.textContent = stop.code;

            card.appendChild(nameEl);
            card.appendChild(codeEl);
            card.addEventListener("click", function () { openStopDetails(stop); });
            grid.appendChild(card);
        });
    }

    // ── Level 2 : direction picker ─────────────────────────────────────────

    function openStopDetails(stop) {
        pendingStop = stop;
        document.getElementById("config-stop-title").textContent = stop.name;
        document.getElementById("config-loading2").classList.remove("hidden");
        document.getElementById("directions-grid").innerHTML = "";
        showLevel2();

        fetch("/api/stops/" + encodeURIComponent(stop.code) + "/details")
            .then(function (r) {
                if (!r.ok) throw new Error("HTTP " + r.status);
                return r.json();
            })
            .then(function (details) {
                document.getElementById("config-loading2").classList.add("hidden");
                renderDirections(stop.code, details);
            })
            .catch(function (err) {
                var el = document.getElementById("config-loading2");
                el.textContent = "Erreur : " + err.message;
            });
    }

    function renderDirections(logicalCode, details) {
        var grid = document.getElementById("directions-grid");
        grid.innerHTML = "";

        // ── "All directions" card (uses the logical stop code) ────────────
        var allCard = document.createElement("div");
        allCard.className = "direction-card" +
            (logicalCode === currentMonitoringRef ? " selected" : "");

        var allTag = document.createElement("span");
        allTag.className = "mode-tag all";
        allTag.textContent = "TOUS";
        allCard.appendChild(allTag);

        var allDesc = document.createElement("div");
        allDesc.className = "dir-destination";
        allDesc.style.fontWeight = "700";
        allDesc.style.fontSize = "0.95rem";
        allDesc.textContent = "Tous les passages — toutes directions";
        allCard.appendChild(allDesc);

        var allCode = document.createElement("div");
        allCode.className = "dir-code";
        allCode.textContent = logicalCode;
        allCard.appendChild(allCode);

        allCard.addEventListener("click", function () {
            selectStop({ code: logicalCode });
        });
        grid.appendChild(allCard);

        // ── One card per physical stop ─────────────────────────────────────
        if (details.length === 0) {
            var empty = document.createElement("div");
            empty.className = "grid-empty";
            empty.textContent = "Aucune direction disponible";
            grid.appendChild(empty);
            return;
        }

        details.forEach(function (phys) {
            var card = document.createElement("div");
            card.className = "direction-card" +
                (phys.stop_code === currentMonitoringRef ? " selected" : "");

            // Mode tag
            var mode = (phys.vehicle_mode || "").toLowerCase();
            var tagClass = mode === "tram" ? "tram"
                         : mode === "bus"  ? "bus"
                         : "other";
            var tagLabel = mode === "tram" ? "TRAM"
                         : mode === "bus"  ? "BUS"
                         : phys.vehicle_mode.toUpperCase() || "?";

            var modeTag = document.createElement("span");
            modeTag.className = "mode-tag " + tagClass;
            modeTag.textContent = tagLabel;
            card.appendChild(modeTag);

            // Lines list
            if (phys.lines && phys.lines.length > 0) {
                var linesEl = document.createElement("div");
                linesEl.className = "dir-lines";

                phys.lines.forEach(function (ld) {
                    var row = document.createElement("div");
                    row.className = "dir-line-row";

                    var badge = document.createElement("span");
                    badge.className = "line-badge-sm";
                    badge.textContent = ld.line;
                    badge.style.backgroundColor = LINE_COLORS[ld.line] || DEFAULT_COLOR;
                    row.appendChild(badge);

                    var dest = document.createElement("span");
                    dest.className = "dir-destination";
                    dest.textContent = ld.destination;
                    row.appendChild(dest);

                    linesEl.appendChild(row);
                });
                card.appendChild(linesEl);
            }

            // Physical stop code
            var codeEl = document.createElement("div");
            codeEl.className = "dir-code";
            codeEl.textContent = phys.stop_code;
            card.appendChild(codeEl);

            card.addEventListener("click", function () {
                selectStop({ code: phys.stop_code });
            });
            grid.appendChild(card);
        });
    }

    function selectStop(stop) {
        fetch("/api/config", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ monitoring_ref: stop.code })
        })
        .then(function (r) {
            if (!r.ok) throw new Error("HTTP " + r.status);
            currentMonitoringRef = stop.code;
            allStops = null;   // invalidate cache
            closeConfig();
        })
        .catch(function (err) {
            alert("Erreur lors de la mise à jour : " + err.message);
        });
    }

    // Live filter — client-side, no server round-trip
    document.getElementById("config-filter").addEventListener("input", function () {
        if (!allStops) return;
        var q = this.value.toLowerCase().trim();
        renderStops(q
            ? allStops.filter(function (s) { return s.name.toLowerCase().indexOf(q) !== -1; })
            : allStops
        );
    });

    document.getElementById("config-btn").addEventListener("click", openConfig);
    document.getElementById("config-back").addEventListener("click", closeConfig);
    document.getElementById("config-back2").addEventListener("click", showLevel1);
    document.getElementById("status-btn").addEventListener("click", openStatus);
    document.getElementById("status-back").addEventListener("click", closeStatus);

    // Close overlay on Escape key
    document.addEventListener("keydown", function (e) {
        if (e.key === "Escape") {
            var statusOverlay = document.getElementById("status-overlay");
            if (!statusOverlay.classList.contains("hidden")) {
                closeStatus();
                return;
            }
            var level2 = document.getElementById("config-level2");
            if (!level2.classList.contains("hidden")) {
                showLevel1();
            } else {
                closeConfig();
            }
        }
    });

    // ── Init ───────────────────────────────────────────────────────────────

    setInterval(function () {
        updateClock();
        updateTimers();
    }, 1000);

    updateClock();
    connect();
}());
