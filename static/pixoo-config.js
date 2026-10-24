/* SPDX-License-Identifier: MIT */
(function () {
    "use strict";
    var base = "", data = null, draft = null, dirty = false, active = false, timer = null, saving = false;
    var root, rows, message, summary, saveButton, preview, previewCaption;
    var loadVersion = 0;
    var names = {station: "Station", miniature: "Strasbourg miniature", birthdays: "Anniversaires", events: "Événements Jour J"};
    function copy(x) { return JSON.parse(JSON.stringify(x)); }
    function el(tag, cls, text) {
        var e = document.createElement(tag); if (cls) e.className = cls; if (text !== undefined) e.textContent = text; return e;
    }
    function button(text, action, cls) {
        var b = el("button", cls || "px-button", text); b.type = "button"; b.addEventListener("click", action); return b;
    }
    function option(value, text) { var o = el("option", "", text); o.value = value; return o; }
    function field(text, input) {
        var label = el("label", "px-field"); label.append(el("span", "", text), input); return label;
    }
    function report(text, error) { message.textContent = text; message.classList.toggle("px-error", !!error); }
    function changed() {
        dirty = true; report("Modifications non enregistrées."); refreshSummary();
    }
    function platformLabel(p) {
        return (p.name || "Quai") + " · " + (p.directions.length ? p.directions.join(" / ") : "Réf. " + p.ref) + (p.directions.length ? " (" + p.ref + ")" : "");
    }
    function isDeparture(step) { return step.screen === "station" || step.screen === "miniature"; }
    function refreshSummary() {
        var valid = draft.steps.length > 0 && draft.steps.length <= 64 && draft.steps.some(isDeparture);
        var departureTime = 0;
        draft.steps.forEach(function (s) {
            valid = valid && Number.isInteger(s.seconds) && s.seconds >= 1 && s.seconds <= 300;
            if (isDeparture(s)) {
                if (s.platform && !data.platforms.some(function (p) { return p.ref === s.platform; })) valid = false;
                departureTime += (s.seconds || 0) * (s.platform ? 1 : data.platforms.length);
            }
        });
        summary.textContent = draft.steps.length + " étapes · " + departureTime + " s de départs, plus les moments disponibles · répétition en boucle";
        saveButton.disabled = saving || !dirty || !valid;
        if (!valid) report("Ajoutez un écran de départs et vérifiez les quais et les durées (1 à 300 s).", true);
    }
    function focusStep(i) { var e = document.getElementById("px-type-" + i); if (e) e.focus(); }
    function renderSteps() {
        rows.replaceChildren();
        draft.steps.forEach(function (step, i) {
            var row = el("li", "px-step px-step-" + step.screen);
            row.append(el("span", "px-number", String(i + 1)));
            var type = el("select"); type.id = "px-type-" + i;
            Object.keys(names).forEach(function (key) { type.append(option(key, names[key])); }); type.value = step.screen;
            type.addEventListener("change", function () {
                step.screen = type.value;
                if (!isDeparture(step)) delete step.platform;
                changed(); renderSteps(); focusStep(i);
            });
            var platform = el("select"); platform.id = "px-platform-" + i;
            if (isDeparture(step)) {
                platform.append(option("", "Tous les quais configurés"));
                data.platforms.forEach(function (p) { platform.append(option(p.ref, platformLabel(p))); });
                if (step.platform && !data.platforms.some(function (p) {return p.ref === step.platform;})) platform.append(option(step.platform, "Quai retiré : " + step.platform));
                platform.value = step.platform || "";
                platform.addEventListener("change", function () {
                    if (platform.value) step.platform = platform.value; else delete step.platform;
                    changed();
                });
            } else { platform.append(option("", "Tous les éléments disponibles")); platform.disabled = true; }
            var seconds = el("input"); seconds.type = "number"; seconds.min = "1"; seconds.max = "300"; seconds.step = "1"; seconds.value = step.seconds; seconds.id = "px-seconds-" + i;
            seconds.addEventListener("input", function () { step.seconds = seconds.valueAsNumber; changed(); });
            row.append(field("Écran", type), field(isDeparture(step) ? "Quai et directions" : "Contenu", platform), field(isDeparture(step) ? "Durée (s)" : "Par élément (s)", seconds));
            var actions = el("div", "px-step-actions");
            [["↑", "Monter", -1], ["↓", "Descendre", 1]].forEach(function (a) {
                var b = button(a[0], function () { var target = i + a[2]; draft.steps.splice(target, 0, draft.steps.splice(i, 1)[0]); changed(); renderSteps(); focusStep(target); });
                b.disabled = i + a[2] < 0 || i + a[2] >= draft.steps.length; b.setAttribute("aria-label", a[1] + " l’étape " + (i + 1)); b.title = a[1]; actions.append(b);
            });
            var duplicate = button("Dupliquer", function () { draft.steps.splice(i + 1, 0, copy(step)); changed(); renderSteps(); focusStep(i + 1); });
            duplicate.disabled = draft.steps.length >= 64; duplicate.setAttribute("aria-label", "Dupliquer l’étape " + (i + 1)); actions.append(duplicate);
            var remove = button("Supprimer", function () { draft.steps.splice(i, 1); changed(); renderSteps(); focusStep(Math.max(0, i - 1)); });
            remove.setAttribute("aria-label", "Supprimer l’étape " + (i + 1)); actions.append(remove); row.append(actions); rows.append(row);
        });
        refreshSummary();
    }
    function previewLoop() {
        clearTimeout(timer);
        if (!active || !data || !data.enabled || !preview) return;
        if (!document.hidden) preview.src = base + "api/pixoo64/preview?t=" + Date.now();
        timer = setTimeout(previewLoop, 2000);
    }
    function render() {
        root.replaceChildren();
        var intro = el("section", "px-intro");
        var text = el("div"); text.append(el("h2", "", "Votre cycle sur le Pixoo64"), el("p", "px-description", "Choisissez les écrans, leurs quais et leur temps d’affichage. Enregistrez pour appliquer le cycle sans redémarrer l’application."));
        text.append(el("p", "px-notice", data.platforms.length + (data.platforms.length === 1 ? " quai configuré" : " quais configurés") + " dans l’onglet Arrêt. Les modèles et ce cycle suivent cette sélection."));
        if (!data.birthdays_enabled || !data.events_enabled) text.append(el("p", "px-description", "Les anniversaires et événements s’affichent seulement si leur source est activée et contient des éléments."));
        var figure = el("figure", "px-live"); preview = el("img"); preview.alt = "Écran actuel du Pixoo64"; preview.width = 128; preview.height = 128;
        previewCaption = el("figcaption", "", data.enabled ? "Aperçu en direct · cycle enregistré" : "Appareil désactivé");
        preview.addEventListener("load", function () {preview.hidden = false; previewCaption.textContent = "Aperçu en direct · cycle enregistré";});
        preview.addEventListener("error", function () {preview.hidden = true; previewCaption.textContent = "Aperçu momentanément indisponible";});
        preview.hidden = !data.enabled; figure.append(preview, previewCaption); intro.append(text, figure); root.append(intro);
        var presets = el("section", "px-presets"); presets.append(el("h3", "", "Partir d’un modèle"));
        var options = el("div", "px-preset-options");
        [["station", "Station seulement", "Le tableau complet, quai après quai."], ["miniature", "Strasbourg miniature", "Le décor et les lignes C / D, par quai."], ["alternating", "Alterner par tournée", "Station, moments, miniature, moments."], ["interleaved", "Alterner par quai", "Station puis miniature pour chaque quai."]].forEach(function (p) {
            var b = button("", function () {draft = copy(data.presets[p[0]]); changed(); renderSteps(); document.getElementById("px-lines").value = draft.lines_per_screen;});
            b.className = "px-preset"; b.append(el("strong", "", p[1]), el("span", "", p[2])); options.append(b);
        }); presets.append(options, el("p", "px-hint", "Les modèles incluent les moments. Vous pouvez ensuite retirer ou déplacer chaque étape.")); root.append(presets);
        var heading = el("div", "px-cycle-heading"); heading.append(el("h3", "", "Ordre de passage"));
        var lines = el("select"); lines.id = "px-lines"; [1,2,3,4].forEach(function (n) {lines.append(option(String(n), n + (n === 1 ? " ligne" : " lignes")));}); lines.value = draft.lines_per_screen;
        lines.addEventListener("change", function () {draft.lines_per_screen = Number(lines.value); changed();}); heading.append(field("Lignes du tableau Station", lines)); root.append(heading);
        summary = el("p", "px-summary"); root.append(summary);
        root.append(el("p", "px-hint", "Une étape « Tous les quais » affiche chaque quai pendant la durée choisie. Les moments sans contenu sont ignorés. Le cycle reprend ensuite à la première étape."));
        rows = el("ol", "px-steps"); root.append(rows);
        var add = button("+ Ajouter une étape", function () {if (draft.steps.length >= 64) return; draft.steps.push({screen:"station", seconds:6}); changed(); renderSteps(); focusStep(draft.steps.length - 1);}); root.append(add);
        var footer = el("div", "px-savebar"); message = el("p", "px-message"); message.setAttribute("role", "status"); message.setAttribute("aria-live", "polite");
        var buttons = el("div", "px-save-actions");
        var cancel = button("Annuler les modifications", function () {draft = copy(data.settings); dirty = false; render();});
        saveButton = button("Enregistrer et appliquer", save, "px-button px-primary"); buttons.append(cancel, saveButton); footer.append(message, buttons); root.append(footer);
        renderSteps();
        if (saving) root.querySelectorAll("button,input,select").forEach(function (e) {e.disabled = true;});
        if (dirty) report("Modifications non enregistrées."); else report("Cycle enregistré. Les réglages seront conservés après un redémarrage."); previewLoop();
    }
    async function save() {
        if (saving || saveButton.disabled) return;
        saving = true; refreshSummary(); report("Enregistrement…");
        // Freeze the draft while awaiting confirmation; never discard later edits.
        root.querySelectorAll("button,input,select").forEach(function (e) {e.disabled = true;});
        try {
            var r = await fetch(base + "api/pixoo64/config", {method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify(draft)});
            if (!r.ok) throw new Error(await r.text());
            var result = await r.json(); data.settings = copy(result.settings); draft = copy(result.settings); dirty = false; saving = false; render();
            report(result.applied ? "Enregistré et appliqué. Le cycle reprend à la première étape." : "Cycle sauvegardé pour la prochaine activation du Pixoo64.");
        } catch (err) {saving = false; render(); report(err.message || "Enregistrement impossible. Vos modifications sont conservées.", true);}
    }
    async function open(url) {
        var requestVersion = ++loadVersion;
        base = url; active = true; root = document.getElementById("pixoo-config-content");
        root.replaceChildren(el("p", "px-description", "Chargement du cycle…"));
        try {
            var r = await fetch(base + "api/pixoo64/config", {cache:"no-store"});
            if (!r.ok) throw new Error("Impossible de charger la configuration (" + r.status + ").");
            var loaded = await r.json();
            if (requestVersion !== loadVersion) return;
            var previousRefs = data ? data.platforms.map(function (p) {return p.ref;}).join("|") : "";
            var nextRefs = loaded.platforms.map(function (p) {return p.ref;});
            var selectionChanged = previousRefs !== nextRefs.join("|");
            data = loaded;
            if (draft && dirty) {
                // Preserve unsaved durations/order, while dropping removed platforms.
                var hasDepartures = draft.steps.some(function (s) {return isDeparture(s) && (!s.platform || nextRefs.includes(s.platform));});
                var seen = [];
                draft.steps = draft.steps.filter(function (s) {
                    if (!isDeparture(s) || !s.platform || nextRefs.includes(s.platform)) return true;
                    if (!hasDepartures && !seen.includes(s.screen)) {seen.push(s.screen); delete s.platform; return true;}
                    return false;
                });
            } else {draft = copy(data.settings); dirty = false;}
            render();
            if (selectionChanged && previousRefs) report("Cycle adapté aux quais choisis dans Arrêt." + (dirty ? " Vos autres modifications sont conservées ; pensez à enregistrer." : ""));
        } catch (err) { if (requestVersion !== loadVersion) return; root.replaceChildren(el("p", "px-error", err.message), button("Réessayer", function () {open(base);})); }
    }
    window.addEventListener("beforeunload", function (e) {if (dirty) {e.preventDefault(); e.returnValue = "";}});
    async function checkEnabled(url) {
        var enabled = false;
        try {
            var response = await fetch(url + "api/pixoo64/config", {cache:"no-store"});
            enabled = response.ok && (await response.json()).enabled === true;
        } catch (_) { /* Keep device-only controls hidden when unavailable. */ }
        document.querySelector('[data-tab="pixoo64"]').hidden = !enabled;
        if (!enabled) {draft = null; data = null; dirty = false;}
        return enabled;
    }
    window.PixooConfig = {open:open, checkEnabled:checkEnabled, close:function () {++loadVersion; active = false; clearTimeout(timer);}};
}());
