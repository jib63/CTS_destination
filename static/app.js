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

    // ── Jour J icon SVGs (inline, keyed by icon name) ─────────────────────
    var JJ_ICONS = {
        star:
            '<svg class="jj-entry-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<path d="M12 2 L14 8 L20 8 L15 12 L17 18 L12 14 L7 18 L9 12 L4 8 L10 8 Z" fill="#f0c83c"/>' +
            '</svg>',
        party:
            '<svg class="jj-entry-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<path d="M4 20 L10 8 L16 14 Z" fill="#f4a030" opacity="0.9"/>' +
            '<circle cx="14" cy="5" r="1.2" fill="#e87070"/>' +
            '<circle cx="18" cy="8" r="1" fill="#6df083"/>' +
            '<circle cx="11" cy="4" r="0.9" fill="#5b8dee"/>' +
            '<circle cx="16" cy="11" r="1.1" fill="#f0c83c"/>' +
            '<circle cx="19" cy="5" r="0.8" fill="#e87070"/>' +
            '</svg>',
        heart:
            '<svg class="jj-entry-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<path d="M12 21 C12 21 3 14 3 8.5 C3 5.4 5.4 3 8.5 3 C10.2 3 11.7 3.8 12 4.5 C12.3 3.8 13.8 3 15.5 3 C18.6 3 21 5.4 21 8.5 C21 14 12 21 12 21 Z" fill="#e87070"/>' +
            '</svg>',
        present:
            '<svg class="jj-entry-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<rect x="3" y="10" width="18" height="12" rx="2" fill="#5b8dee" opacity="0.9"/>' +
            '<rect x="9" y="10" width="6" height="12" fill="#e6dca8" opacity="0.5"/>' +
            '<rect x="3" y="7" width="18" height="4" rx="1" fill="#7aaaf5"/>' +
            '<rect x="10.5" y="7" width="3" height="4" fill="#e6dca8" opacity="0.5"/>' +
            '<path d="M12 7 Q10 4 8 5 Q6 6 8 8 Q10 9 12 7Z" fill="#e87070"/>' +
            '<path d="M12 7 Q14 4 16 5 Q18 6 16 8 Q14 9 12 7Z" fill="#e87070"/>' +
            '</svg>',
        skull:
            '<svg class="jj-entry-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">' +
            '<ellipse cx="12" cy="10" rx="7" ry="7.5" fill="#b0b8cc"/>' +
            '<rect x="7" y="16" width="10" height="5" rx="1" fill="#b0b8cc"/>' +
            '<rect x="8.5" y="16.5" width="2.5" height="3" rx="0.5" fill="#1a2744"/>' +
            '<rect x="13" y="16.5" width="2.5" height="3" rx="0.5" fill="#1a2744"/>' +
            '<ellipse cx="9.5" cy="10" rx="2" ry="2.2" fill="#1a2744"/>' +
            '<ellipse cx="14.5" cy="10" rx="2" ry="2.2" fill="#1a2744"/>' +
            '<ellipse cx="12" cy="13.5" rx="1.2" ry="0.8" fill="#1a2744"/>' +
            '</svg>'
    };

    // ── State ──────────────────────────────────────────────────────────────
    var departureData = null;
    var currentMonitoringRefs = [];    // full list of monitored stop codes
    var currentBoardIndex = 0;         // which board is currently displayed
    var rotationTimer = null;          // setInterval handle for stop rotation
    var stopRotationSecs = null;       // rotation interval in seconds (from server)
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
                // Extract monitoring refs and rotation config from the new payload
                if (departureData.boards && Array.isArray(departureData.boards)) {
                    currentMonitoringRefs = departureData.boards.map(function (b) {
                        return b.monitoring_ref || "";
                    }).filter(Boolean);
                    stopRotationSecs = departureData.stop_rotation_secs || null;
                }
                currentBoardIndex = 0;
                resetRotation();
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
            '<div id="wx-bg" aria-hidden="true"></div>' +
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

        renderWeatherBg(w.pictocode);
    }

    // ── Weather background animation ───────────────────────────────────────

    function pictocodeToWxType(code) {
        var night = code > 100;
        var c = night ? code - 100 : code;
        if (night && c <= 3) return 'night-clear';
        if (night)           return 'night-cloudy';
        if (c === 1)         return 'sun';
        if (c === 2 || c === 3 || c === 30)                                  return 'partly-cloudy';
        if (c === 4 || c === 31 || c === 34)                                  return 'cloudy';
        if (c === 5 || c === 6)                                               return 'fog';
        if (c === 7)                                                           return 'drizzle';
        if (c >= 8 && c <= 11 || c === 19 || c === 33 || c === 35)           return 'rain';
        if (c === 20 || c === 21 || c === 22 || c === 23 ||
            c === 25 || c === 26 || c === 28)                                 return 'thunder';
        if (c === 12 || c === 13 || c === 24 || c === 27 ||
            c === 29 || c === 32)                                             return 'sleet';
        if (c >= 14 && c <= 18)                                              return 'snow';
        return 'rain';
    }

    function wxRnd(lo, hi) { return lo + Math.random() * (hi - lo); }

    function renderWeatherBg(pictocode) {
        var bg = document.getElementById("wx-bg");
        if (!bg) return;
        bg.innerHTML = "";
        var type = pictocodeToWxType(pictocode);

        if (type === 'sun' || type === 'partly-cloudy') {
            wxAddSun(bg, type === 'partly-cloudy');
        }
        if (type === 'partly-cloudy' || type === 'cloudy' || type === 'night-cloudy') {
            wxAddClouds(bg, type === 'cloudy' ? 4 : 2);
        }
        if (type === 'fog')     wxAddFog(bg);
        if (type === 'drizzle') wxAddRain(bg, 12, false);
        if (type === 'rain')    wxAddRain(bg, 26, false);
        if (type === 'thunder') { wxAddRain(bg, 22, false); wxAddLightning(bg); }
        if (type === 'sleet')   wxAddRain(bg, 18, true);
        if (type === 'snow')    wxAddSnow(bg);
        if (type === 'night-clear')  wxAddStars(bg, 24);
        if (type === 'night-cloudy') { wxAddStars(bg, 12); }
    }

    function wxAddSun(bg, small) {
        var s = small ? 52 : 72;
        var el = document.createElement("div");
        el.className = "wx-sun-el";
        el.style.cssText =
            "right:" + wxRnd(8,18) + "%;" +
            "top:50%;margin-top:-" + (s/2) + "px;" +
            "width:" + s + "px;height:" + s + "px;" +
            "animation-duration:22s;";
        el.innerHTML =
            '<svg viewBox="0 0 100 100" width="' + s + '" height="' + s + '" style="display:block">' +
            '<circle cx="50" cy="50" r="18" fill="rgba(255,210,0,0.2)"/>' +
            '<g stroke="rgba(255,210,0,0.16)" stroke-width="4.5" stroke-linecap="round">' +
            '<line x1="50" y1="4"  x2="50" y2="22"/>' +
            '<line x1="50" y1="78" x2="50" y2="96"/>' +
            '<line x1="4"  y1="50" x2="22" y2="50"/>' +
            '<line x1="78" y1="50" x2="96" y2="50"/>' +
            '<line x1="15" y1="15" x2="27" y2="27"/>' +
            '<line x1="73" y1="73" x2="85" y2="85"/>' +
            '<line x1="85" y1="15" x2="73" y2="27"/>' +
            '<line x1="27" y1="73" x2="15" y2="85"/>' +
            '</g>' +
            '<circle cx="50" cy="50" r="10" fill="rgba(255,200,0,0.1)"/>' +
            '</svg>';
        bg.appendChild(el);
    }

    function wxAddClouds(bg, count) {
        for (var i = 0; i < count; i++) {
            var w = wxRnd(80, 155);
            var h = w * 0.44;
            var el = document.createElement("div");
            el.className = "wx-cloud-el";
            el.style.cssText =
                "top:" + wxRnd(8, 72) + "%;" +
                "left:-220px;" +
                "opacity:" + wxRnd(0.18, 0.3) + ";" +
                "animation-duration:" + wxRnd(20, 42) + "s;" +
                "animation-delay:-" + wxRnd(0, 35) + "s;";
            el.innerHTML =
                '<svg viewBox="0 0 160 70" width="' + Math.round(w) + '" height="' + Math.round(h) + '">' +
                '<ellipse cx="100" cy="52" rx="60" ry="19" fill="rgba(155,175,215,1)"/>' +
                '<ellipse cx="68"  cy="44" rx="42" ry="28" fill="rgba(155,175,215,1)"/>' +
                '<ellipse cx="115" cy="41" rx="34" ry="24" fill="rgba(155,175,215,1)"/>' +
                '</svg>';
            bg.appendChild(el);
        }
    }

    function wxAddFog(bg) {
        for (var i = 0; i < 6; i++) {
            var el = document.createElement("div");
            el.className = "wx-fog-band";
            el.style.cssText =
                "top:" + wxRnd(10, 88) + "%;" +
                "width:" + wxRnd(55, 95) + "%;" +
                "animation-duration:" + wxRnd(9, 20) + "s;" +
                "animation-delay:-" + wxRnd(0, 15) + "s;";
            bg.appendChild(el);
        }
    }

    function wxAddRain(bg, count, isSleet) {
        for (var i = 0; i < count; i++) {
            var el = document.createElement("div");
            el.className = "wx-drop";
            el.style.cssText =
                "left:" + wxRnd(0, 100) + "%;" +
                "height:" + wxRnd(12, 26) + "px;" +
                "animation-duration:" + wxRnd(0.65, 1.3) + "s;" +
                "animation-delay:-" + wxRnd(0, 1.3) + "s;";
            if (isSleet) {
                el.style.background = "linear-gradient(to bottom, transparent, rgba(175,210,235,0.55))";
            }
            bg.appendChild(el);
        }
    }

    function wxAddLightning(bg) {
        var el = document.createElement("div");
        el.className = "wx-lightning-el";
        el.style.cssText =
            "left:" + wxRnd(20, 70) + "%;" +
            "animation-duration:" + wxRnd(4, 9) + "s;" +
            "animation-delay:-" + wxRnd(0, 7) + "s;";
        el.innerHTML =
            '<svg viewBox="0 0 20 70" width="12" height="48" style="display:block">' +
            '<polyline points="15,0 5,30 12,30 3,70"' +
            ' fill="rgba(255,235,80,0.45)" stroke="rgba(255,245,100,0.35)"' +
            ' stroke-width="1.5" stroke-linejoin="round"/>' +
            '</svg>';
        bg.appendChild(el);
    }

    function wxAddSnow(bg) {
        for (var i = 0; i < 18; i++) {
            var el = document.createElement("div");
            el.className = "wx-flake";
            var s = wxRnd(3, 7);
            el.style.cssText =
                "left:" + wxRnd(0, 100) + "%;" +
                "width:" + s + "px;height:" + s + "px;" +
                "animation-duration:" + wxRnd(2.5, 5) + "s;" +
                "animation-delay:-" + wxRnd(0, 5) + "s;";
            bg.appendChild(el);
        }
    }

    function wxAddStars(bg, count) {
        for (var i = 0; i < count; i++) {
            var el = document.createElement("div");
            el.className = "wx-star-el";
            var s = wxRnd(1.5, 3.5);
            el.style.cssText =
                "left:" + wxRnd(2, 98) + "%;" +
                "top:"  + wxRnd(5, 90) + "%;" +
                "width:" + s + "px;height:" + s + "px;" +
                "animation-duration:" + wxRnd(1.5, 4) + "s;" +
                "animation-delay:-"   + wxRnd(0, 4) + "s;";
            bg.appendChild(el);
        }
    }

    // ── Arabesque ornamental animations ───────────────────────────────────
    //
    // Five distinct calligraphic flourish designs, each drawn stroke-by-stroke
    // using stroke-dashoffset CSS transitions, then faded, then the next shown.
    // The #wx-arabesque container is preserved across weather data refreshes so
    // the animation cycle never restarts unexpectedly.
    //
    // viewBox "0 0 800 80" — wide & shallow to fill the lower footer zone.
    // preserveAspectRatio "none" stretches to fill the full width naturally.

    var ARA_DESIGNS = [

        // 1 ── Twin outward scrolls connected by a central vine ────────────
        '<path d="M 55,50 C 55,22 90,8 122,24 C 154,40 150,74 126,75 C 102,76 94,50 116,45 C 138,40 150,62 136,70"/>' +
        '<path d="M 745,50 C 745,22 710,8 678,24 C 646,40 650,74 674,75 C 698,76 706,50 684,45 C 662,40 650,62 664,70"/>' +
        '<path d="M 136,55 C 200,35 290,54 400,44 C 510,34 600,53 664,55"/>' +
        '<path d="M 270,49 L 266,30 C 263,19 276,19 274,30"/>' +
        '<path d="M 400,44 L 396,24 C 393,13 407,13 405,24"/>' +
        '<path d="M 530,49 L 534,30 C 537,19 524,19 526,30"/>',

        // 2 ── Flowing S-wave with opposing volute endpoints ───────────────
        '<path d="M 10,48 C 50,48 75,15 120,32 C 162,49 172,80 210,65 C 248,50 265,18 308,36 C 348,52 358,82 395,67 C 432,52 448,22 490,38 C 530,54 542,80 580,65 C 618,50 635,20 678,36 C 718,52 730,70 790,55"/>' +
        '<path d="M 120,32 C 112,12 92,8 86,22 C 80,36 96,50 110,42 C 124,34 118,18 112,20"/>' +
        '<path d="M 308,36 C 300,16 280,12 274,26 C 268,40 284,54 298,46 C 312,38 306,22 300,24"/>' +
        '<path d="M 490,38 C 498,18 518,14 524,28 C 530,42 514,56 500,48 C 486,40 492,24 498,26"/>' +
        '<path d="M 678,36 C 686,16 706,12 712,26 C 718,40 702,54 688,46 C 674,38 680,22 686,24"/>',

        // 3 ── Central medallion with branching anthemion fans ─────────────
        '<path d="M 10,52 C 80,52 140,36 195,46 C 240,55 268,44 295,48"/>' +
        '<path d="M 505,48 C 532,44 560,55 605,46 C 660,36 720,52 790,52"/>' +
        '<path d="M 295,48 C 332,54 362,42 400,48 C 438,54 468,42 505,48"/>' +
        '<path d="M 400,48 L 396,26 C 392,12 408,12 406,26"/>' +
        '<path d="M 396,26 C 388,14 374,10 370,22 C 366,34 378,44 390,38"/>' +
        '<path d="M 406,26 C 414,14 428,10 432,22 C 436,34 424,44 412,38"/>' +
        '<path d="M 155,44 L 153,26 C 151,14 163,14 161,26 C 156,16 146,14 144,24"/>' +
        '<path d="M 645,44 L 647,26 C 649,14 661,14 659,26 C 664,16 674,14 676,24"/>',

        // 4 ── Intertwined paired tendrils with knotted centre ─────────────
        '<path d="M 10,42 C 45,22 80,62 115,47 C 145,34 162,15 198,28 C 234,41 226,74 206,74 C 186,74 180,52 200,47 C 220,42 232,62 218,70"/>' +
        '<path d="M 790,42 C 755,22 720,62 685,47 C 655,34 638,15 602,28 C 566,41 574,74 594,74 C 614,74 620,52 600,47 C 580,42 568,62 582,70"/>' +
        '<path d="M 218,58 C 260,68 308,52 356,58 C 376,62 390,72 400,68"/>' +
        '<path d="M 582,58 C 540,68 492,52 444,58 C 424,62 410,72 400,68"/>' +
        '<path d="M 356,58 L 353,38 C 351,26 363,26 362,38"/>' +
        '<path d="M 444,58 L 447,38 C 449,26 461,26 460,38"/>' +
        '<path d="M 400,68 L 396,80 C 391,92 409,92 404,80"/>',

        // 5 ── Floral scrolls with symmetrical petal offshoots ────────────
        '<path d="M 55,44 C 75,22 108,18 135,34 C 152,44 148,66 128,66 C 108,66 102,44 122,39 C 142,34 152,56 138,64"/>' +
        '<path d="M 745,44 C 725,22 692,18 665,34 C 648,44 652,66 672,66 C 692,66 698,44 678,39 C 658,34 648,56 662,64"/>' +
        '<path d="M 138,50 C 192,32 255,50 310,42 C 354,36 378,48 400,44"/>' +
        '<path d="M 662,50 C 608,32 545,50 490,42 C 446,36 422,48 400,44"/>' +
        '<path d="M 226,46 L 224,28 C 222,16 234,16 232,28"/>' +
        '<path d="M 574,46 L 576,28 C 578,16 590,16 588,28"/>' +
        '<path d="M 400,44 L 396,22 C 392,8 408,8 406,22"/>' +
        '<path d="M 400,44 L 400,66 C 400,76 390,82 385,72 M 400,66 C 400,76 410,82 415,72"/>'

    ];

    var _araIdx    = 0;
    var _araTimer  = null;

    function araInit(container) {
        container.innerHTML = "";
        var ns = "http://www.w3.org/2000/svg";
        ARA_DESIGNS.forEach(function (design) {
            var svg = document.createElementNS(ns, "svg");
            svg.setAttribute("class", "ara-svg");
            svg.setAttribute("viewBox", "0 0 800 80");
            svg.setAttribute("preserveAspectRatio", "none");
            svg.innerHTML = design;
            container.appendChild(svg);
        });
        _araIdx = 0;
        if (_araTimer) { clearTimeout(_araTimer); _araTimer = null; }
        araShowNext();
    }

    function araStop() {
        if (_araTimer) { clearTimeout(_araTimer); _araTimer = null; }
    }

    function araShowNext() {
        var container = document.getElementById("wx-arabesque");
        if (!container) return;
        var svgs = container.querySelectorAll(".ara-svg");
        if (!svgs.length) return;

        var svg = svgs[_araIdx % svgs.length];

        // Reset all SVGs to invisible
        svgs.forEach(function (s) { s.style.opacity = "0"; });

        // Initialise all paths to un-drawn
        var paths = svg.querySelectorAll("path");
        paths.forEach(function (p) {
            var len = p.getTotalLength ? p.getTotalLength() : 1200;
            p.style.transition = "none";
            p.style.strokeDasharray  = String(len);
            p.style.strokeDashoffset = String(len);
        });

        // Force reflow, then reveal SVG and cascade draw each path
        void svg.getBoundingClientRect();
        svg.style.opacity = "0.9";

        paths.forEach(function (p, i) {
            var len = parseFloat(p.style.strokeDasharray) || 1200;
            // Stagger: each path starts drawing 1.4 s after the previous
            p.style.transition =
                "stroke-dashoffset 3.2s ease-in-out " + (i * 1.4) + "s";
            // Assign target in next frame so transitions fire
            requestAnimationFrame(function () {
                p.style.strokeDashoffset = "0";
            });
        });

        // Total draw window = 3.2 + (n-1)*1.4 s  →  hold 3 s  →  fade 2 s
        var drawWindow = 3.2 + Math.max(0, paths.length - 1) * 1.4;
        var holdMs  = 3000;
        var fadeMs  = 2000;

        _araTimer = setTimeout(function () {
            svg.style.opacity = "0";           // triggers CSS transition: 2s ease
            _araIdx++;
            _araTimer = setTimeout(araShowNext, fadeMs + 200);
        }, (drawWindow + holdMs / 1000) * 1000);
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
            var board0 = departureData && departureData.boards && departureData.boards[0];
            renderJourJ(board0 || null);
            scheduleMidnightRefresh();
        }, msUntil);
    }

    function renderJourJ(board) {
        var el = document.getElementById("jour-j-row");
        var events = (board && board.jour_j_events) || [];

        if (events.length === 0) {
            el.style.display = "none";
            updateExtrasRowVisibility();
            return;
        }

        el.style.display = "";

        // Build one HTML block per event; doubled for seamless loop
        var entryHtml = "";
        events.forEach(function (ev) {
            var iconSvg = JJ_ICONS[ev.icon] || JJ_ICONS.star;
            var isToday = ev.days === 0;
            var entryClass = 'jj-entry' + (isToday ? ' jj-entry--today' : '');
            var sparks = isToday
                ? '<span class="jj-spark jj-spark--1"></span>' +
                  '<span class="jj-spark jj-spark--2"></span>' +
                  '<span class="jj-spark jj-spark--3"></span>' +
                  '<span class="jj-spark jj-spark--4"></span>'
                : '';
            entryHtml +=
                '<span class="' + entryClass + '">' +
                sparks +
                iconSvg +
                '<span class="jj-entry-badge">J\u2011' + ev.days + '</span>' +
                '<span class="jj-entry-label">\u00a0' + escHtml(ev.label) + '\u00a0\u00a0\u00a0\u00a0</span>' +
                '</span>';
        });

        var wrap = document.createElement("div");
        wrap.className = "jour-j-marquee-wrap";

        var scroll = document.createElement("div");
        scroll.className = "jj-scroll";
        scroll.innerHTML = entryHtml + entryHtml; // doubled for seamless loop

        wrap.appendChild(scroll);
        el.innerHTML = "";
        el.appendChild(wrap);
        updateExtrasRowVisibility();
    }

    function renderDepartureRows(board) {
        var inner = document.getElementById("departures-inner");
        inner.innerHTML = "";

        if (!board) return;

        // ── Offline / no-service message ───────────────────────────────────
        if (board.offline_message) {
            var msg = document.createElement("div");
            msg.className = "offline-message";
            msg.textContent = board.offline_message;
            inner.appendChild(msg);
            return;
        }

        var lines = board.lines || [];
        lines.forEach(function (lineDep) {
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
            inner.appendChild(row);
        });

        updateTimers();
    }

    function renderBoard() {
        if (!departureData) return;

        var boards = departureData.boards;

        // ── Legacy / offline path (no boards array) ────────────────────────
        if (!boards || !Array.isArray(boards) || boards.length === 0) {
            var inner = document.getElementById("departures-inner");
            inner.innerHTML = "";
            if (departureData.offline_message) {
                var msg = document.createElement("div");
                msg.className = "offline-message";
                msg.textContent = departureData.offline_message;
                inner.appendChild(msg);
            }
            renderBirthday(null);
            renderJourJ(null);
            renderWeather(null);
            return;
        }

        var board0     = boards[0];
        var boardCurr  = boards[currentBoardIndex] || board0;

        // ── Departure rows: always the currently-rotated stop ──────────────
        renderDepartureRows(boardCurr);

        // ── Extras / weather: always from boards[0] ────────────────────────
        renderBirthday(board0);
        renderJourJ(board0);
        renderWeather(board0);
    }

    // ── Stop rotation ──────────────────────────────────────────────────────

    function resetRotation() {
        if (rotationTimer !== null) {
            clearInterval(rotationTimer);
            rotationTimer = null;
        }
        if (!departureData || !departureData.boards) return;
        var count = departureData.boards.length;
        if (count <= 1 || !stopRotationSecs || stopRotationSecs <= 0) return;

        rotationTimer = setInterval(function () {
            if (!departureData || !departureData.boards) return;
            var next = (currentBoardIndex + 1) % departureData.boards.length;
            rotateTo(next);
        }, stopRotationSecs * 1000);
    }

    function rotateTo(newIdx) {
        if (!departureData || !departureData.boards) return;
        if (newIdx === currentBoardIndex) return;

        var inner = document.getElementById("departures-inner");

        // Wipe out current rows to the left
        inner.classList.remove("departures-wipe-out", "departures-wipe-in");
        void inner.offsetWidth; // force reflow
        inner.classList.add("departures-wipe-out");

        setTimeout(function () {
            currentBoardIndex = newIdx;
            renderDepartureRows(departureData.boards[currentBoardIndex]);

            // Slide new rows in from the right
            inner.classList.remove("departures-wipe-out");
            void inner.offsetWidth;
            inner.classList.add("departures-wipe-in");

            inner.addEventListener("animationend", function onEnd() {
                inner.removeEventListener("animationend", onEnd);
                inner.classList.remove("departures-wipe-in");
            });
        }, 360);
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
        var refsDisplay = (cts.monitoring_refs && cts.monitoring_refs.length > 0)
            ? escHtml(cts.monitoring_refs.join(", "))
            : escHtml(cts.monitoring_ref || "—");
        row("Arrêts surveillés", refsDisplay, "highlight");

        if (cts.stop_rotation_secs) {
            row("Rotation des arrêts", cts.stop_rotation_secs + "\u00a0s");
        }

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

        var events            = jj.events || [];
        var birthdayDaysAhead = (jj.birthday_days_ahead !== undefined) ? jj.birthday_days_ahead : 7;

        // ── Status ─────────────────────────────────────────────────────────
        var statusEl = document.createElement("div");
        statusEl.className = "status-row";
        statusEl.innerHTML =
            '<div class="status-label">Compteur Jour J</div>' +
            '<div class="status-value' + (jj.enabled ? '' : ' dim') + '">' +
            (jj.enabled ? 'Activ\u00e9' : 'D\u00e9sactiv\u00e9') + '</div>';
        container.appendChild(statusEl);

        // ── Event list ─────────────────────────────────────────────────────
        var listWrap = document.createElement("div");
        listWrap.className = "jj-event-list";

        if (events.length === 0) {
            var emptyMsg = document.createElement("div");
            emptyMsg.className = "config-loading-msg";
            emptyMsg.style.padding = "0.6rem 0";
            emptyMsg.textContent = "Aucun \u00e9v\u00e9nement configur\u00e9";
            listWrap.appendChild(emptyMsg);
        } else {
            events.forEach(function (ev, idx) {
                var evRow = document.createElement("div");
                var isToday = ev.days === 0;
                evRow.className = "jj-event-row" + (isToday ? " jj-event-row--today" : "");

                var iconSvg  = JJ_ICONS[ev.icon] || JJ_ICONS.star;
                var daysText = isToday
                    ? "Aujourd\u2019hui\u00a0!"
                    : (ev.days !== undefined && ev.days !== null) ? 'J\u2011' + ev.days : ev.date;

                evRow.innerHTML =
                    '<span class="jj-event-icon-cell">' + iconSvg + '</span>' +
                    '<span class="jj-event-info">' +
                    '<span class="jj-event-date">' + escHtml(ev.date || '') + '</span>' +
                    '<span class="jj-event-badge' + (isToday ? ' jj-event-badge--today' : '') + '">' + escHtml(daysText) + '</span>' +
                    '<span class="jj-event-label">' + escHtml(ev.label) + '</span>' +
                    '</span>' +
                    '<button class="jj-trash-btn" data-idx="' + idx + '" title="Supprimer">' +
                    '<svg viewBox="0 0 24 24" fill="none">' +
                    '<path d="M6 7h12M9 7V5h6v2M10 11v6M14 11v6M5 7l1 13h12l1-13"' +
                    ' stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>' +
                    '</svg></button>';
                listWrap.appendChild(evRow);
            });
        }
        container.appendChild(listWrap);

        // Trash button handlers (after DOM is in place)
        listWrap.querySelectorAll(".jj-trash-btn").forEach(function (btn) {
            btn.addEventListener("click", function () {
                var idx       = parseInt(btn.dataset.idx, 10);
                var newEvents = events.filter(function (_, i) { return i !== idx; });
                postJourJEvents(newEvents, birthdayDaysAhead, openConfigJourJ);
            });
        });

        // ── Add event button + inline form ─────────────────────────────────
        var addBtn = document.createElement("button");
        addBtn.className = "jj-add-btn";
        addBtn.textContent = "+ Ajouter un \u00e9v\u00e9nement";
        container.appendChild(addBtn);

        var addForm = document.createElement("div");
        addForm.className = "jj-add-form hidden";
        addForm.innerHTML =
            '<input class="jj-input" type="text" id="jj-new-date" placeholder="JJ/MM/AAAA" maxlength="10">' +
            '<input class="jj-input" type="text" id="jj-new-label" placeholder="\u00c9v\u00e9nement (ex\u00a0: No\u00ebl)">' +
            '<select class="jj-select" id="jj-new-icon">' +
            '<option value="star">\u2605 \u00c9toile</option>' +
            '<option value="party">\ud83c\udf89 F\u00eate</option>' +
            '<option value="heart">\u2665 C\u0153ur</option>' +
            '<option value="present">\ud83c\udf81 Cadeau</option>' +
            '<option value="skull">\ud83d\udc80 Cr\u00e2ne</option>' +
            '</select>' +
            '<div class="jj-form-actions">' +
            '<button class="jj-save-new-btn">Enregistrer</button>' +
            '<button class="jj-cancel-btn">Annuler</button>' +
            '</div>';
        container.appendChild(addForm);

        addBtn.addEventListener("click", function () {
            addForm.classList.remove("hidden");
            addBtn.style.display = "none";
            document.getElementById("jj-new-date").focus();
        });

        addForm.querySelector(".jj-cancel-btn").addEventListener("click", function () {
            addForm.classList.add("hidden");
            addBtn.style.display = "";
        });

        addForm.querySelector(".jj-save-new-btn").addEventListener("click", function () {
            var date  = (document.getElementById("jj-new-date").value  || "").trim();
            var label = (document.getElementById("jj-new-label").value || "").trim();
            var icon  = document.getElementById("jj-new-icon").value;
            if (!date || !label) { alert("La date et le label sont requis."); return; }
            var parts = date.split("/");
            if (parts.length !== 3 || parts[0].length !== 2 || parts[1].length !== 2 || parts[2].length !== 4) {
                alert("La date doit \u00eatre au format JJ/MM/AAAA");
                return;
            }
            var newEvents = events.concat([{ date: date, label: label, icon: icon }]);
            postJourJEvents(newEvents, birthdayDaysAhead, openConfigJourJ);
        });

        // ── Birthday days ahead + upcoming list ────────────────────────────
        var daysSection = document.createElement("div");
        daysSection.className = "jj-days-ahead-section";

        var upcomingBdays = jj.birthday_upcoming || [];
        var bdayListHtml = "";
        if (upcomingBdays.length > 0) {
            bdayListHtml = '<div class="jj-bday-list">';
            upcomingBdays.forEach(function (ev) {
                var daysText = ev.days === 1 ? "demain" : "dans\u00a0" + ev.days + "\u00a0j";
                bdayListHtml +=
                    '<div class="jj-bday-row">' +
                    JJ_ICONS.present +
                    '<span class="jj-bday-label">' + escHtml(ev.label) + '</span>' +
                    '<span class="jj-bday-days">' + daysText + '</span>' +
                    '</div>';
            });
            bdayListHtml += '</div>';
        }

        daysSection.innerHTML =
            '<div class="status-label">Anniversaires \u00e0 venir (J\u20111 \u00e0 J\u2011<span id="jj-days-display">' + birthdayDaysAhead + '</span>)</div>' +
            bdayListHtml +
            '<div class="jj-days-row">' +
            '<input class="jj-input jj-days-input" type="number" id="jj-days-ahead" min="1" max="30" value="' + birthdayDaysAhead + '">' +
            '<button class="jj-save-days-btn">Appliquer</button>' +
            '</div>';
        container.appendChild(daysSection);

        daysSection.querySelector(".jj-save-days-btn").addEventListener("click", function () {
            var val = parseInt(document.getElementById("jj-days-ahead").value, 10);
            if (isNaN(val) || val < 1 || val > 30) { alert("Valeur entre 1 et 30"); return; }
            postJourJEvents(events, val, openConfigJourJ);
        });
    }

    function postJourJEvents(events, birthdayDaysAhead, onSuccess) {
        fetch("/api/jour-j", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ events: events, birthday_days_ahead: birthdayDaysAhead })
        })
        .then(function (r) {
            if (!r.ok) return r.text().then(function (t) { throw new Error(t); });
            if (onSuccess) onSuccess();
        })
        .catch(function (err) { alert("Erreur\u00a0: " + err.message); });
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

    function renderCurrentRefsBar() {
        var bar = document.getElementById("current-refs-bar");
        if (!bar) return;
        bar.innerHTML = "";
        if (currentMonitoringRefs.length === 0) return;

        currentMonitoringRefs.forEach(function (ref) {
            var chip = document.createElement("span");
            chip.className = "ref-chip";
            chip.textContent = ref;

            // Only show remove button if there's more than one stop
            if (currentMonitoringRefs.length > 1) {
                var btn = document.createElement("button");
                btn.className = "ref-chip-remove";
                btn.textContent = "\u00d7";
                btn.title = "Retirer " + ref;
                btn.addEventListener("click", function (e) {
                    e.stopPropagation();
                    var newRefs = currentMonitoringRefs.filter(function (r) { return r !== ref; });
                    postMonitoringRefs(newRefs, function () {
                        currentMonitoringRefs = newRefs;
                        renderCurrentRefsBar();
                        if (allStops) renderStops(allStops);
                    });
                });
                chip.appendChild(btn);
            }
            bar.appendChild(chip);
        });
    }

    function postMonitoringRefs(refs, onSuccess) {
        fetch("/api/config", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ monitoring_refs: refs })
        })
        .then(function (r) {
            if (!r.ok) return r.text().then(function (t) { throw new Error(t); });
            if (onSuccess) onSuccess();
        })
        .catch(function (err) {
            alert("Erreur lors de la mise à jour\u00a0: " + err.message);
        });
    }

    function showLevel1() {
        document.getElementById("config-level1").style.display = "";
        document.getElementById("config-level2").classList.add("hidden");
        document.getElementById("config-filter").value = "";
        pendingStop = null;
        renderCurrentRefsBar();
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
                (currentMonitoringRefs.indexOf(stop.code) !== -1 ? " selected" : "");

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
            (currentMonitoringRefs.indexOf(logicalCode) !== -1 ? " selected" : "");

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
                (currentMonitoringRefs.indexOf(phys.stop_code) !== -1 ? " selected" : "");

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
        var idx = currentMonitoringRefs.indexOf(stop.code);
        var newRefs;
        if (idx === -1) {
            // Not yet monitored: add it
            newRefs = currentMonitoringRefs.concat([stop.code]);
        } else if (currentMonitoringRefs.length === 1) {
            // Only stop — replace it rather than leaving an empty list
            newRefs = [stop.code];
        } else {
            // Already monitored: remove it
            newRefs = currentMonitoringRefs.filter(function (r) { return r !== stop.code; });
        }

        postMonitoringRefs(newRefs, function () {
            currentMonitoringRefs = newRefs;
            closeConfig();
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

    // ── Touch / swipe (iPad and other touch devices) ───────────────────────

    (function () {
        if (!("ontouchstart" in window) && navigator.maxTouchPoints === 0) return;

        var touchStartX = 0;
        var touchStartY = 0;
        var SWIPE_MIN_X  = 60;   // minimum horizontal distance (px)
        var SWIPE_MAX_Y  = 80;   // maximum vertical drift before we ignore the gesture

        var el = document.getElementById("departures");

        el.addEventListener("touchstart", function (e) {
            var t = e.changedTouches[0];
            touchStartX = t.clientX;
            touchStartY = t.clientY;
        }, { passive: true });

        el.addEventListener("touchend", function (e) {
            if (!departureData || !departureData.boards) return;
            var count = departureData.boards.length;
            if (count <= 1) return;

            var t = e.changedTouches[0];
            var dx = t.clientX - touchStartX;
            var dy = t.clientY - touchStartY;

            if (Math.abs(dx) < SWIPE_MIN_X || Math.abs(dy) > SWIPE_MAX_Y) return;

            // Swipe left → next stop;  swipe right → previous stop
            var next = dx < 0
                ? (currentBoardIndex + 1) % count
                : (currentBoardIndex - 1 + count) % count;

            // Reset the auto-rotation timer so it doesn't fire too soon after a manual swipe
            if (rotationTimer !== null) {
                clearInterval(rotationTimer);
                rotationTimer = null;
                resetRotation();
            }

            rotateTo(next);
        }, { passive: true });
    }());

    // ── Init ───────────────────────────────────────────────────────────────

    setInterval(function () {
        updateClock();
        updateTimers();
    }, 1000);

    updateClock();
    scheduleMidnightRefresh();
    connect();

    // Arabesque ornamental animation — lives in #board-canvas (below weather row)
    (function () {
        var canvas = document.getElementById("board-canvas");
        if (!canvas) return;
        var araEl = document.createElement("div");
        araEl.id = "wx-arabesque";
        araEl.setAttribute("aria-hidden", "true");
        canvas.appendChild(araEl);
        araInit(araEl);
    }());
}());
