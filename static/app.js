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

    // ── Weather display ────────────────────────────────────────────────────

    function pictocodeToIcon(code) {
        var night = code > 100;
        var c = night ? code - 100 : code;
        if (night && c <= 3) return "#w-night-clear";
        if (night)           return "#w-night-cloudy";
        if (c === 1)         return "#w-sunny";
        if (c === 2 || c === 30) return "#w-mostly-sunny";
        if (c === 3)         return "#w-partly-cloudy";
        if (c === 4 || c === 31 || c === 34) return "#w-cloudy";
        if (c === 5 || c === 6) return "#w-fog";
        if (c === 7)         return "#w-drizzle";
        if (c === 8 || c === 9 || c === 10 || c === 11 || c === 19 || c === 33 || c === 35) return "#w-rain";
        if (c === 20 || c === 21 || c === 22 || c === 23 || c === 25 || c === 26 || c === 28) return "#w-thunder";
        if (c === 12 || c === 13 || c === 24 || c === 27 || c === 29 || c === 32) return "#w-sleet";
        if (c >= 14 && c <= 18) return "#w-snow";
        return "#w-rain"; // fallback
    }

    function renderWeather(board) {
        var footer = document.getElementById("board-footer");
        if (!board || !board.weather) {
            footer.innerHTML = "";
            return;
        }
        var w = board.weather;
        var iconId = pictocodeToIcon(w.pictocode);

        footer.innerHTML =
            '<div id="weather-row">' +
            '<svg id="weather-icon" viewBox="0 0 64 64" aria-hidden="true"><use href="' + iconId + '"/></svg>' +

            '<div class="wx-group">' +
            '<span class="wx-label">Temp\u00e9rature</span>' +
            '<div class="wx-temps">' +
            '<span class="wx-temp-min">' + Math.round(w.temp_min) + '\u00a0\u00b0C</span>' +
            '<span class="wx-temp-sep">/</span>' +
            '<span class="wx-temp-max">' + Math.round(w.temp_max) + '\u00a0\u00b0C</span>' +
            '</div></div>' +

            '<div class="wx-sep-bar"></div>' +

            '<div class="wx-group">' +
            '<span class="wx-label">Maintenant</span>' +
            '<div class="wx-item">' +
            '<span class="wx-value">' + Math.round(w.temp_now) + '<span class="wx-unit">\u00a0\u00b0C</span></span>' +
            '</div></div>' +

            '<div class="wx-sep-bar"></div>' +

            '<div class="wx-group">' +
            '<span class="wx-label">Pr\u00e9cipitations</span>' +
            '<div class="wx-item">' +
            '<svg viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<path d="M12 3C12 3 5 10 5 15C5 18.87 8.13 22 12 22C15.87 22 19 18.87 19 15C19 10 12 3 12 3Z" fill="#5ba3d8"/>' +
            '</svg>' +
            '<span class="wx-value">' + w.precipitation.toFixed(1) + '<span class="wx-unit">\u00a0mm</span></span>' +
            '</div></div>' +

            '<div class="wx-sep-bar"></div>' +

            '<div class="wx-group">' +
            '<span class="wx-label">Ensoleillement</span>' +
            '<div class="wx-item">' +
            '<svg viewBox="0 0 24 24" aria-hidden="true">' +
            '<circle cx="12" cy="12" r="5" fill="#FFD600"/>' +
            '<g stroke="#FFD600" stroke-width="2" stroke-linecap="round">' +
            '<line x1="12" y1="2" x2="12" y2="5"/><line x1="12" y1="19" x2="12" y2="22"/>' +
            '<line x1="2" y1="12" x2="5" y2="12"/><line x1="19" y1="12" x2="22" y2="12"/>' +
            '<line x1="4.9" y1="4.9" x2="7.1" y2="7.1"/><line x1="16.9" y1="16.9" x2="19.1" y2="19.1"/>' +
            '<line x1="19.1" y1="4.9" x2="16.9" y2="7.1"/><line x1="7.1" y1="16.9" x2="4.9" y2="19.1"/>' +
            '</g></svg>' +
            '<span class="wx-value">' + w.sunshine_hours.toFixed(0) + '<span class="wx-unit">\u00a0h</span></span>' +
            '</div></div>' +

            '<div class="wx-location">' + escHtml(w.location_name) + '</div>' +
            '</div>';
    }

    // ── Extras row visibility ──────────────────────────────────────────────

    function updateExtrasRowVisibility() {
        var bday = document.getElementById("birthday-row");
        var jj   = document.getElementById("jour-j-row");
        var row  = document.getElementById("extras-row");
        var anyVisible = bday.style.display !== "none" || jj.style.display !== "none";
        row.style.display = anyVisible ? "" : "none";
    }

    // ── Birthday row ───────────────────────────────────────────────────────

    function renderBirthday(board) {
        var el = document.getElementById("birthday-row");
        var names = (board && board.birthdays_today) || [];

        if (names.length === 0) {
            el.style.display = "none";
            updateExtrasRowVisibility();
            return;
        }
        el.style.display = "";

        var text = "\u00a0\u00a0\ud83c\udf81\u00a0Bon anniversaire\u00a0\u2022\u00a0" + names.join("\u00a0\u2022\u00a0") + "\u00a0\u00a0";

        // Present icon (SVG)
        var iconSvg =
            '<svg class="birthday-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<rect x="3" y="10" width="18" height="12" rx="2" fill="#5b8dee" opacity="0.9"/>' +
            '<rect x="9" y="10" width="6" height="12" fill="#e6dca8" opacity="0.5"/>' +
            '<rect x="3" y="7" width="18" height="4" rx="1" fill="#7aaaf5"/>' +
            '<rect x="10.5" y="7" width="3" height="4" fill="#e6dca8" opacity="0.5"/>' +
            '<path d="M12 7 Q10 4 8 5 Q6 6 8 8 Q10 9 12 7Z" fill="#e87070"/>' +
            '<path d="M12 7 Q14 4 16 5 Q18 6 16 8 Q14 9 12 7Z" fill="#e87070"/>' +
            '</svg>';

        var wrap = document.createElement("div");
        wrap.className = "birthday-text-wrap";

        var span = document.createElement("span");
        span.textContent = text + text; // duplicate for seamless loop
        wrap.appendChild(span);

        el.innerHTML = iconSvg;
        el.appendChild(wrap);
        wrap.classList.add("scrolling");
        updateExtrasRowVisibility();
    }

    // ── Jour J row ─────────────────────────────────────────────────────────

    function isJourJPast(jourJDate) {
        if (!jourJDate) return true;
        var parts = jourJDate.split("/");
        if (parts.length !== 3) return true;
        var target = new Date(parseInt(parts[2]), parseInt(parts[1]) - 1, parseInt(parts[0]));
        var today = new Date(); today.setHours(0,0,0,0);
        return target < today;
    }

    var _jourJMidnightTimer = null;

    function scheduleMidnightRefresh() {
        if (_jourJMidnightTimer) clearTimeout(_jourJMidnightTimer);
        var now = new Date();
        var midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 5);
        var msUntil = midnight.getTime() - now.getTime();
        _jourJMidnightTimer = setTimeout(function () {
            renderJourJ(departureData);
            scheduleMidnightRefresh();
        }, msUntil);
    }

    function renderJourJ(board) {
        var el = document.getElementById("jour-j-row");
        if (!board || !board.jour_j) {
            el.style.display = "none";
            updateExtrasRowVisibility();
            return;
        }
        var days  = board.jour_j[0];
        var label = board.jour_j[1];

        if (days === null || days === undefined || days < 0) {
            el.style.display = "none";
            updateExtrasRowVisibility();
            return;
        }

        el.style.display = "";

        var badgeText = "J\u2011" + days;

        var iconSvg =
            '<svg class="jour-j-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<path d="M12 2 L14 8 L20 8 L15 12 L17 18 L12 14 L7 18 L9 12 L4 8 L10 8 Z" fill="#f0c83c"/>' +
            '</svg>';

        var badge = document.createElement("span");
        badge.className = "jour-j-badge";
        badge.textContent = badgeText;

        var wrap = document.createElement("div");
        wrap.className = "jour-j-text-wrap";

        var text = "\u00a0" + label + "\u00a0\u00a0\u00a0";
        var span = document.createElement("span");
        span.textContent = text + text; // duplicate for seamless loop
        wrap.appendChild(span);

        el.innerHTML = iconSvg;
        el.appendChild(badge);
        el.appendChild(wrap);
        wrap.classList.add("scrolling");
        updateExtrasRowVisibility();
    }

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
            renderBirthday(null);
            renderJourJ(null);
            renderWeather(null);
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
        renderBirthday(departureData);
        renderJourJ(departureData);
        renderWeather(departureData);
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

    var activeStatusTab = "cts";
    var lastStatusData = null;

    function openStatus() {
        document.getElementById("status-overlay").classList.remove("hidden");
        fetchAndRenderStatus();
    }

    function closeStatus() {
        document.getElementById("status-overlay").classList.add("hidden");
    }

    function fetchAndRenderStatus() {
        var content = document.getElementById("status-content");
        content.innerHTML = '<div class="config-loading-msg">Chargement\u2026</div>';

        fetch("/api/status")
            .then(function (r) {
                if (!r.ok) throw new Error("HTTP " + r.status);
                return r.json();
            })
            .then(function (data) {
                lastStatusData = data;
                renderStatusTab(activeStatusTab, data);
            })
            .catch(function (err) {
                content.innerHTML = '<div class="config-loading-msg">Erreur\u00a0: ' + err.message + '</div>';
            });
    }

    function renderStatusTab(tab, data) {
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

        if (tab === "cts") {
            renderStatusCts(data.cts || {}, data.server_local_time, row);
        } else if (tab === "meteoblue") {
            renderStatusMeteoblue(data.meteoblue || {}, row);
        }
    }

    function renderStatusCts(cts, serverLocalTime, row) {
        row("Arrêt surveillé", escHtml(cts.monitoring_ref || "—"), "highlight");

        row("Mode",
            cts.simulation ? "Simulation (pas d\u2019appel API)" : "Temps r\u00e9el",
            cts.simulation ? "highlight" : "");

        row("Intervalle de scrutation", (cts.polling_interval_minutes || "—") + "\u00a0min");

        if (cts.next_poll_at && cts.next_poll_at > 0) {
            var diffSec = Math.round((cts.next_poll_at * 1000 - Date.now()) / 1000);
            var nextStr = diffSec <= 0 ? "Imm\u00e9diatement"
                : diffSec < 120 ? "dans\u00a0" + diffSec + "\u00a0s"
                : "dans\u00a0" + Math.round(diffSec / 60) + "\u00a0min";
            row("Prochain appel API", nextStr);
        } else {
            row("Prochain appel API", "—", "dim");
        }

        row("Interrogation permanente", cts.always_query ? "Oui" : "Non");

        if (!cts.always_query) {
            row("Fen\u00eatre active", cts.in_window ? "Oui" : "Non",
                cts.in_window ? "" : "dim");
            row("Expressions crontab",
                escHtml(cts.query_intervals_raw || "—"),
                cts.query_intervals_raw ? "" : "dim");
        }

        var srvTime = "—";
        if (serverLocalTime) {
            try {
                srvTime = new Date(serverLocalTime).toLocaleTimeString("fr-FR", {
                    hour: "2-digit", minute: "2-digit", second: "2-digit", timeZoneName: "short"
                });
            } catch (_) { srvTime = serverLocalTime; }
        }
        row("Heure serveur", srvTime, "dim");
    }

    function renderStatusMeteoblue(mb, row) {
        row("Widget météo",
            mb.enabled ? "Activ\u00e9" : "D\u00e9sactiv\u00e9",
            mb.enabled ? "" : "dim");

        row("Mode",
            mb.simulation ? "Simulation (pas d\u2019appel API)" : "Temps r\u00e9el",
            mb.simulation ? "highlight" : "");

        row("Localisation configur\u00e9e",
            escHtml(mb.location_config || "—"),
            mb.location_config ? "" : "dim");

        row("Localisation r\u00e9solue",
            escHtml(mb.location_resolved || "—"),
            mb.location_resolved ? "" : "dim");

        if (mb.lat != null) {
            row("Coordonn\u00e9es",
                mb.lat.toFixed(4) + "\u00b0N, " + mb.lon.toFixed(4) + "\u00b0E — " + mb.asl + "\u00a0m", "dim");
        }

        row("Intervalle de relev\u00e9", (mb.polling_interval_minutes || "—") + "\u00a0min");

        row("Interrogation permanente", mb.always_query ? "Oui" : "Non");

        if (!mb.always_query) {
            row("Fen\u00eatre active", mb.in_window ? "Oui" : "Non",
                mb.in_window ? "" : "dim");
            row("Expressions crontab",
                escHtml(mb.query_intervals_raw || "—"),
                mb.query_intervals_raw ? "" : "dim");
        }

        if (mb.last_fetch) {
            var fetchTime = "—";
            try {
                fetchTime = new Date(mb.last_fetch).toLocaleTimeString("fr-FR", {
                    hour: "2-digit", minute: "2-digit", second: "2-digit"
                });
            } catch (_) { fetchTime = mb.last_fetch; }
            row("Dernier relev\u00e9", fetchTime);
        } else {
            row("Dernier relev\u00e9", "Aucun", "dim");
        }

        if (mb.temp_now != null) {
            row("Temp\u00e9rature actuelle",
                Math.round(mb.temp_now) + "\u00a0\u00b0C");
            row("Min\u00a0/ Max du jour",
                Math.round(mb.temp_min) + "\u00a0\u00b0C\u00a0/\u00a0" + Math.round(mb.temp_max) + "\u00a0\u00b0C");
            row("Pr\u00e9cipitations",
                mb.precipitation.toFixed(1) + "\u00a0mm");
            row("Ensoleillement",
                mb.sunshine_hours.toFixed(0) + "\u00a0h");
            row("Pictocode", String(mb.pictocode), "dim");
        } else {
            row("Donn\u00e9es m\u00e9t\u00e9o", "Pas encore disponibles", "dim");
        }
    }

    // Tab switching
    document.getElementById("status-tabs").addEventListener("click", function (e) {
        var btn = e.target.closest(".status-tab");
        if (!btn) return;
        var tab = btn.dataset.tab;
        if (tab === activeStatusTab) return;
        activeStatusTab = tab;
        document.querySelectorAll(".status-tab").forEach(function (b) {
            b.classList.toggle("active", b.dataset.tab === tab);
        });
        if (lastStatusData) {
            renderStatusTab(tab, lastStatusData);
        }
    });

    function escHtml(s) {
        return String(s)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;");
    }

    // ── Configuration panel ────────────────────────────────────────────────

    // Currently selected stop info used while navigating level 2
    var pendingStop = null;   // { code, name } for the logical stop being drilled into
    var activeConfigTab = "arret";

    function activateConfigTab(tab) {
        activeConfigTab = tab;
        document.querySelectorAll("#config-tabs .config-tab").forEach(function (b) {
            b.classList.toggle("active", b.dataset.tab === tab);
        });
        var arretEl = document.getElementById("config-tab-arret");
        var jourjEl = document.getElementById("config-tab-jour-j");
        if (tab === "arret") {
            arretEl.classList.remove("hidden");
            jourjEl.classList.add("hidden");
            showLevel1();
            if (allStops === null) {
                loadStops();
            } else {
                renderStops(allStops);
                document.getElementById("config-filter").focus();
            }
        } else if (tab === "jour-j") {
            arretEl.classList.add("hidden");
            jourjEl.classList.remove("hidden");
            openConfigJourJ();
        }
    }

    function openConfig() {
        document.getElementById("config-overlay").classList.remove("hidden");
        activateConfigTab(activeConfigTab);
    }

    function closeConfig() {
        document.getElementById("config-overlay").classList.add("hidden");
    }

    // ── Jour J config (in config overlay) ─────────────────────────────────

    function openConfigJourJ() {
        var content = document.getElementById("config-jour-j-content");
        content.innerHTML = '<div class="config-loading-msg">Chargement\u2026</div>';
        fetch("/api/status")
            .then(function (r) {
                if (!r.ok) throw new Error("HTTP " + r.status);
                return r.json();
            })
            .then(function (data) {
                renderConfigJourJ(data.jour_j || {}, content);
            })
            .catch(function (err) {
                content.innerHTML = '<div class="config-loading-msg">Erreur\u00a0: ' + err.message + '</div>';
            });
    }

    function renderConfigJourJ(jj, container) {
        container.innerHTML = "";

        function row(label, value, cls) {
            var el = document.createElement("div");
            el.className = "status-row";
            el.innerHTML =
                '<div class="status-label">' + label + "</div>" +
                '<div class="status-value' + (cls ? " " + cls : "") + '">' + value + "</div>";
            container.appendChild(el);
        }

        row("Compteur Jour J",
            jj.enabled ? "Activ\u00e9" : "D\u00e9sactiv\u00e9",
            jj.enabled ? "" : "dim");

        if (jj.days_remaining !== null && jj.days_remaining !== undefined) {
            row("Jours restants", "J\u2011" + jj.days_remaining, "highlight");
        } else if (jj.date) {
            row("Jours restants", "Date pass\u00e9e", "dim");
        }

        // Form
        var form = document.createElement("div");
        form.className = "status-row";
        form.style.flexDirection = "column";
        form.style.gap = "0.6rem";
        form.style.paddingTop = "0.5rem";
        form.innerHTML =
            '<div class="status-label">Configurer l\u2019\u00e9v\u00e9nement</div>' +
            '<input id="jj-date-input" type="text" placeholder="JJ/MM/AAAA" maxlength="10"' +
            ' value="' + escHtml(jj.date || "") + '"' +
            ' style="background:#1a2744;color:#fff;border:1px solid rgba(255,255,255,0.2);' +
            'border-radius:6px;padding:0.4em 0.7em;font-size:0.9rem;width:100%;">' +
            '<input id="jj-label-input" type="text" placeholder="\u00c9v\u00e9nement (ex\u00a0: No\u00ebl)"' +
            ' value="' + escHtml(jj.label || "") + '"' +
            ' style="background:#1a2744;color:#fff;border:1px solid rgba(255,255,255,0.2);' +
            'border-radius:6px;padding:0.4em 0.7em;font-size:0.9rem;width:100%;">' +
            '<button id="jj-save-btn"' +
            ' style="background:#2e8b3c;color:#fff;border:none;border-radius:6px;' +
            'padding:0.45em 1.2em;font-size:0.9rem;cursor:pointer;align-self:flex-start;">' +
            'Enregistrer</button>' +
            '<div id="jj-toast" style="display:none;color:#6df083;font-size:0.85rem;">Enregistr\u00e9 !</div>';
        container.appendChild(form);

        document.getElementById("jj-save-btn").addEventListener("click", function () {
            var date  = (document.getElementById("jj-date-input").value  || "").trim();
            var label = (document.getElementById("jj-label-input").value || "").trim();
            if (!date || !label) { alert("La date et le label sont requis."); return; }
            fetch("/api/jour-j", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ date: date, label: label })
            })
            .then(function (r) {
                if (!r.ok) return r.text().then(function (t) { throw new Error(t); });
                var toast = document.getElementById("jj-toast");
                if (toast) { toast.style.display = ""; setTimeout(function () { toast.style.display = "none"; }, 2000); }
                openConfigJourJ();
            })
            .catch(function (err) { alert("Erreur\u00a0: " + err.message); });
        });
    }

    // Config tab switching
    document.getElementById("config-tabs").addEventListener("click", function (e) {
        var btn = e.target.closest(".config-tab");
        if (!btn) return;
        var tab = btn.dataset.tab;
        if (tab === activeConfigTab) return;
        activateConfigTab(tab);
    });

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
    scheduleMidnightRefresh();
    connect();
}());
