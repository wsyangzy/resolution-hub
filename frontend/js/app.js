/* global window, document, confirm */

const API = window.ResolutionAPI;

const state = {
    displays: [],
    modes: [],
    defaultModeOptions: [],
    selectedDefaultModeIndex: -1,
    settings: {
        defaultDisplayModes: [],
    },
    profiles: [],
    runtime: {
        phase: "idle",
        profileId: null,
        profileName: null,
        message: "就绪",
        canRestore: false,
        canCancel: false,
    },
    launchType: "executable",
    launcherReadyForDetection: false,
    source: "steam",
    selectedSource: null,
    steamGames: [],
    recentApps: [],
};

const byId = (id) => document.getElementById(id);

const icons = {
    play: '<svg viewBox="0 0 24 24"><path d="m8 5 11 7-11 7z"></path></svg>',
    edit: '<svg viewBox="0 0 24 24"><path d="M12 20h9"></path><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4z"></path></svg>',
    trash: '<svg viewBox="0 0 24 24"><path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v5M14 11v5"></path></svg>',
    monitor: '<svg viewBox="0 0 24 24"><rect x="2" y="3" width="20" height="14" rx="2"></rect><path d="M8 21h8M12 17v4"></path></svg>',
};

function escapeHtml(value) {
    return String(value ?? "")
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#039;");
}

function showError(error) {
    byId("error-message").textContent = String(error?.message ?? error);
    byId("error-banner").classList.remove("hidden");
}

function clearError() {
    byId("error-banner").classList.add("hidden");
}

function profileInitial(name) {
    const trimmed = name.trim();
    return trimmed ? trimmed.slice(0, 1).toUpperCase() : "G";
}

function renderSelectedSource() {
    const summary = byId("selected-source");
    if (!state.selectedSource) {
        summary.classList.add("hidden");
        return;
    }

    const selected = state.selectedSource;
    byId("selected-source-icon").textContent = profileInitial(selected.name);
    byId("selected-source-name").textContent = selected.name;
    byId("selected-source-detail").textContent = selected.detail;
    byId("selected-source-type").textContent = selected.type;
    summary.classList.remove("hidden");
}

function setSourceTab(source) {
    state.source = source;
    byId("source-tabs").querySelectorAll("button[data-source]").forEach((button) => {
        button.classList.toggle("active", button.dataset.source === source);
    });
    ["steam", "recent", "browse", "shortcut"].forEach((name) => {
        byId(`source-${name}`).classList.toggle("hidden", name !== source);
    });
}

function renderSteamGames() {
    const list = byId("steam-game-list");
    const query = byId("steam-search").value.trim().toLowerCase();
    const games = state.steamGames.filter((game) =>
        !query
        || game.name.toLowerCase().includes(query)
        || game.appId.includes(query)
    );

    if (!games.length) {
        list.innerHTML = `<div class="source-empty">${query ? "没有匹配的 Steam 游戏" : "没有发现已安装的 Steam 游戏"}</div>`;
        return;
    }

    list.innerHTML = games.map((game) => {
        const selected = state.selectedSource?.key === `steam:${game.appId}`;
        return `
            <button type="button" class="source-item ${selected ? "selected" : ""}"
                data-steam-id="${escapeHtml(game.appId)}">
                <span class="source-item-icon">${escapeHtml(profileInitial(game.name))}</span>
                <span class="source-item-copy">
                    <strong>${escapeHtml(game.name)}</strong>
                    <span>${escapeHtml(game.installDir)}</span>
                </span>
                <span class="source-item-meta">${escapeHtml(game.appId)}</span>
            </button>
        `;
    }).join("");
}

async function loadSteamGames() {
    const list = byId("steam-game-list");
    list.innerHTML = '<div class="source-loading">正在读取 Steam 游戏库...</div>';
    try {
        state.steamGames = await API.listSteamGames();
        renderSteamGames();
    } catch (error) {
        list.innerHTML = `<div class="source-empty">${escapeHtml(error)}</div>`;
    }
}

function renderRecentApps() {
    const list = byId("recent-app-list");
    if (!state.recentApps.length) {
        list.innerHTML = '<div class="source-empty">暂时没有最近活动的应用</div>';
        return;
    }

    list.innerHTML = state.recentApps.map((application, index) => {
        const selected = state.selectedSource?.key === `recent:${application.executable.toLowerCase()}`;
        return `
            <button type="button" class="source-item ${selected ? "selected" : ""}"
                data-recent-index="${index}">
                <span class="source-item-icon">${escapeHtml(profileInitial(application.name))}</span>
                <span class="source-item-copy">
                    <strong>${escapeHtml(application.name)}</strong>
                    <span>${escapeHtml(application.executable)}</span>
                </span>
                <span class="source-item-meta">${escapeHtml(application.processName)}</span>
            </button>
        `;
    }).join("");
}

async function loadRecentApps() {
    const list = byId("recent-app-list");
    list.innerHTML = '<div class="source-loading">正在读取最近应用...</div>';
    try {
        state.recentApps = await API.listRecentApps();
        renderRecentApps();
    } catch (error) {
        list.innerHTML = `<div class="source-empty">${escapeHtml(error)}</div>`;
    }
}

function selectSteamGame(game) {
    setLaunchType("steam");
    byId("game-name").value = game.name;
    byId("steam-app-id").value = game.appId;
    byId("game-executable").value = "";
    byId("game-arguments").value = "";
    byId("working-directory").value = "";
    byId("shortcut-path").value = "";
    byId("process-name").value = "";
    state.selectedSource = {
        key: `steam:${game.appId}`,
        name: game.name,
        detail: `${game.installDir} · App ID ${game.appId}`,
        type: "STEAM",
    };
    renderSelectedSource();
    renderSteamGames();
    updateProcessHint();
    byId("process-name").focus();
}

function selectRecentApplication(application) {
    setLaunchType("executable");
    byId("game-name").value = application.name;
    byId("game-executable").value = application.executable;
    byId("game-arguments").value = "";
    byId("working-directory").value = "";
    byId("steam-app-id").value = "";
    byId("shortcut-path").value = "";
    byId("process-name").value = application.processName;
    state.selectedSource = {
        key: `recent:${application.executable.toLowerCase()}`,
        name: application.name,
        detail: application.executable,
        type: "最近应用",
    };
    renderSelectedSource();
    renderRecentApps();
    updateProcessHint();
}

function launchLabel(profile) {
    if (profile.launchType === "steam") {
        return "Steam";
    }
    if (profile.launchType === "shortcut") {
        return profile.launcherProvider || "启动器";
    }
    return "直接启动";
}

function launchDetail(profile) {
    if (profile.launchType === "steam") {
        return `App ID ${profile.steamAppId}`;
    }
    if (profile.launchType === "shortcut") {
        return profile.shortcutPath || "未设置快捷方式";
    }
    return profile.executable || "未设置路径";
}

function renderProfiles() {
    const list = byId("profile-list");
    byId("profile-count").textContent = String(state.profiles.length);

    if (!state.profiles.length) {
        list.innerHTML = `
            <div class="empty-state">
                <div class="empty-icon">${icons.monitor}</div>
                <h3>还没有游戏配置</h3>
                <p>添加一个游戏，设置启动时使用的显示模式</p>
                <button class="btn btn-primary" data-action="add">${icons.play}添加第一个游戏</button>
            </div>
        `;
        return;
    }

    const activeId = state.runtime.profileId;
    const isBusy = !["idle", "error"].includes(state.runtime.phase);
    list.innerHTML = state.profiles.map((profile) => {
        const active = activeId === profile.id && isBusy;
        return `
            <article class="profile-item" data-id="${escapeHtml(profile.id)}">
                <div class="game-identity">
                    <div class="game-icon">${escapeHtml(profileInitial(profile.name))}</div>
                    <div class="game-copy">
                        <div class="game-name">${escapeHtml(profile.name)}</div>
                        <div class="game-process">${escapeHtml(profile.processName)}</div>
                    </div>
                </div>
                <div class="mode-copy">
                    <div class="mode-primary">${profile.width} × ${profile.height} · ${profile.refreshRate} Hz</div>
                    <div class="mode-secondary">${escapeHtml(profile.displayDevice)}</div>
                </div>
                <div class="launch-copy">
                    <div class="launch-primary">${escapeHtml(launchLabel(profile))}</div>
                    <div class="launch-secondary" title="${escapeHtml(launchDetail(profile))}">${escapeHtml(launchDetail(profile))}</div>
                </div>
                <div class="profile-actions">
                    <button class="btn ${active ? "launch-btn running" : "btn-primary launch-btn"}"
                        data-action="launch" title="启动游戏" ${isBusy ? "disabled" : ""}>
                        ${active ? "运行中" : `${icons.play}启动`}
                    </button>
                    <button class="btn btn-icon" data-action="edit" title="编辑" ${isBusy ? "disabled" : ""}>${icons.edit}</button>
                    <button class="btn btn-icon" data-action="delete" title="删除" ${isBusy ? "disabled" : ""}>${icons.trash}</button>
                </div>
            </article>
        `;
    }).join("");
}

function renderDisplays() {
    const list = byId("display-list");
    if (!state.displays.length) {
        list.innerHTML = '<div class="empty-state"><p>没有读取到活动显示器</p></div>';
        return;
    }

    list.innerHTML = state.displays.map((display) => `
        <div class="display-item">
            <div class="display-title">
                ${icons.monitor}
                <strong>${escapeHtml(display.friendlyName || display.deviceName)}</strong>
                ${display.isPrimary ? '<span class="primary-tag">主显示器</span>' : ""}
            </div>
            <div class="display-mode">${display.currentMode.width} × ${display.currentMode.height} · ${display.currentMode.refreshRate} Hz</div>
            <div class="display-device">${escapeHtml(display.deviceName)}</div>
        </div>
    `).join("");

    const primary = state.displays.find((item) => item.isPrimary) || state.displays[0];
    const configured = state.settings.defaultDisplayModes.find(
        (mode) => mode.deviceName === primary.deviceName,
    );
    const defaultMode = configured || primary.currentMode;
    byId("current-display-mode").textContent =
        `${defaultMode.width} × ${defaultMode.height} · ${defaultMode.refreshRate} Hz`;
}

function updateRuntime(runtime) {
    const previousPhase = state.runtime.phase;
    state.runtime = runtime;

    byId("runtime-message").textContent = runtime.message || "就绪";
    byId("restore-btn").disabled = !runtime.canRestore;
    byId("cancel-wait-btn").classList.toggle("hidden", !runtime.canCancel);
    byId("status-display-btn").disabled =
        !["idle", "error"].includes(runtime.phase);
    const indicator = byId("runtime-indicator");
    indicator.className = "runtime-indicator";
    if (runtime.phase === "error") {
        indicator.classList.add("error");
        showError(runtime.message);
    } else if (runtime.phase !== "idle") {
        indicator.classList.add("busy");
    }

    renderProfiles();
    if (previousPhase !== "idle" && runtime.phase === "idle") {
        refreshDisplays().catch(showError);
    }
}

async function refreshDisplays() {
    state.displays = await API.enumerateDisplays();
    renderDisplays();
    populateDisplaySelect();
}

async function refreshProfiles() {
    state.profiles = await API.listProfiles();
    renderProfiles();
}

function configuredDefaultMode(deviceName) {
    return state.settings.defaultDisplayModes.find(
        (mode) => mode.deviceName === deviceName,
    );
}

function populateDefaultDisplaySelect(selectedDevice) {
    const select = byId("default-display-select");
    select.innerHTML = state.displays.map((display) => `
        <option value="${escapeHtml(display.deviceName)}">
            ${escapeHtml(display.friendlyName || display.deviceName)}${display.isPrimary ? "（主显示器）" : ""}
        </option>
    `).join("");
    if (selectedDevice && state.displays.some((item) => item.deviceName === selectedDevice)) {
        select.value = selectedDevice;
    }
}

function sameMode(left, right) {
    return left
        && right
        && left.width === right.width
        && left.height === right.height
        && left.refreshRate === right.refreshRate;
}

function sameResolution(left, right) {
    return left
        && right
        && left.width === right.width
        && left.height === right.height;
}

function groupedDefaultModes() {
    const groups = new Map();
    state.defaultModeOptions.forEach((mode, index) => {
        const key = `${mode.width}x${mode.height}`;
        if (!groups.has(key)) {
            groups.set(key, {
                key,
                width: mode.width,
                height: mode.height,
                modes: [],
            });
        }
        groups.get(key).modes.push({ mode, index });
    });

    return [...groups.values()]
        .sort((left, right) =>
            (right.width * right.height) - (left.width * left.height)
            || right.width - left.width
            || right.height - left.height
        )
        .map((group) => ({
            ...group,
            modes: group.modes.sort(
                (left, right) => right.mode.refreshRate - left.mode.refreshRate,
            ),
        }));
}

function renderDefaultModeList(deviceName) {
    const list = byId("default-mode-list");
    const display = state.displays.find((item) => item.deviceName === deviceName);
    const configured = configuredDefaultMode(deviceName);
    const selectedMode = state.defaultModeOptions[state.selectedDefaultModeIndex];
    const groups = groupedDefaultModes();
    byId("default-mode-count").textContent = `${groups.length} 种分辨率`;
    list.innerHTML = groups.map((group) => {
        const isExpanded = sameResolution(group, selectedMode);
        const isConfiguredResolution = sameResolution(group, configured);
        const isCurrentResolution = sameResolution(group, display?.currentMode);
        const refreshRates = group.modes.map(({ mode, index }) => {
            const isSelected = index === state.selectedDefaultModeIndex;
            const isConfigured = sameMode(mode, configured);
            const isCurrent = sameMode(mode, display?.currentMode);
            return `
                <label class="refresh-rate-option${isSelected ? " selected" : ""}">
                    <input type="radio" name="default-mode" value="${index}" ${isSelected ? "checked" : ""}>
                    <span class="resolution-radio" aria-hidden="true"></span>
                    <strong>${mode.refreshRate} Hz</strong>
                    <span class="resolution-mode-tags">
                        ${isCurrent ? '<span class="mode-tag current">当前</span>' : ""}
                        ${isConfigured ? '<span class="mode-tag">默认</span>' : ""}
                    </span>
                </label>
            `;
        }).join("");

        return `
            <div class="resolution-group${isExpanded ? " expanded" : ""}">
                <button type="button" class="resolution-group-button" data-resolution-key="${group.key}">
                    <span class="resolution-group-copy">
                        <strong>${group.width} × ${group.height}</strong>
                        <small>${group.modes.length} 个刷新率</small>
                    </span>
                    <span class="resolution-mode-tags">
                        ${isCurrentResolution ? '<span class="mode-tag current">当前</span>' : ""}
                        ${isConfiguredResolution ? '<span class="mode-tag">默认</span>' : ""}
                    </span>
                    <svg class="resolution-chevron" viewBox="0 0 24 24"><path d="m9 18 6-6-6-6"></path></svg>
                </button>
                <div class="refresh-rate-panel">
                    <div class="refresh-rate-label">刷新率</div>
                    <div class="refresh-rate-grid">${refreshRates}</div>
                </div>
            </div>
        `;
    }).join("");
}

function selectDefaultResolution(deviceName, resolutionKey) {
    const group = groupedDefaultModes().find((item) => item.key === resolutionKey);
    if (!group) {
        return;
    }

    const display = state.displays.find((item) => item.deviceName === deviceName);
    const preferredModes = [
        configuredDefaultMode(deviceName),
        display?.currentMode,
    ];
    const preferred = preferredModes
        .map((mode) => group.modes.find((item) => sameMode(item.mode, mode)))
        .find(Boolean);
    state.selectedDefaultModeIndex = preferred?.index ?? group.modes[0].index;
    renderDefaultModeList(deviceName);

    window.requestAnimationFrame(() => {
        const list = byId("default-mode-list");
        const expanded = list.querySelector(".resolution-group.expanded");
        if (!expanded) {
            return;
        }
        const listBounds = list.getBoundingClientRect();
        const expandedBounds = expanded.getBoundingClientRect();
        if (expandedBounds.bottom > listBounds.bottom) {
            list.scrollBy({
                top: expandedBounds.bottom - listBounds.bottom + 8,
                behavior: "smooth",
            });
        } else if (expandedBounds.top < listBounds.top) {
            list.scrollBy({
                top: expandedBounds.top - listBounds.top - 8,
                behavior: "smooth",
            });
        }
    });
}

async function populateDefaultModeList(deviceName) {
    const list = byId("default-mode-list");
    state.selectedDefaultModeIndex = -1;
    byId("default-mode-count").textContent = "";
    list.innerHTML = '<div class="resolution-mode-message">正在读取显示模式...</div>';
    try {
        state.defaultModeOptions = await API.enumerateDisplayModes(deviceName);

        const display = state.displays.find((item) => item.deviceName === deviceName);
        const selected = configuredDefaultMode(deviceName) || display?.currentMode;
        state.selectedDefaultModeIndex = state.defaultModeOptions.findIndex(
            (mode) => sameMode(mode, selected),
        );
        if (state.selectedDefaultModeIndex < 0 && state.defaultModeOptions.length) {
            state.selectedDefaultModeIndex = 0;
        }
        renderDefaultModeList(deviceName);
    } catch (error) {
        state.defaultModeOptions = [];
        list.innerHTML = '<div class="resolution-mode-message error">无法读取该显示器的模式</div>';
        throw error;
    }
}

async function openDefaultResolutionDrawer() {
    clearError();
    if (!state.displays.length) {
        showError("没有可用的显示器");
        return;
    }
    const primary = state.displays.find((item) => item.isPrimary) || state.displays[0];
    const selectedDevice = configuredDefaultMode(primary.deviceName)?.deviceName
        || primary.deviceName;
    populateDefaultDisplaySelect(selectedDevice);
    byId("default-resolution-drawer").classList.remove("hidden");
    byId("default-resolution-overlay").classList.remove("hidden");
    await populateDefaultModeList(selectedDevice);
}

function closeDefaultResolutionDrawer() {
    byId("default-resolution-drawer").classList.add("hidden");
    byId("default-resolution-overlay").classList.add("hidden");
}

async function saveDefaultResolution(event) {
    event.preventDefault();
    clearError();
    const selected = state.defaultModeOptions[state.selectedDefaultModeIndex];
    if (!selected) {
        showError("请选择有效的默认显示模式");
        return;
    }

    try {
        state.settings = await API.setDefaultDisplayMode(selected);
        closeDefaultResolutionDrawer();
        await refreshDisplays();
    } catch (error) {
        showError(error);
    }
}

function populateDisplaySelect(selectedDevice) {
    const select = byId("display-select");
    if (!state.displays.length) {
        select.innerHTML = '<option value="">没有活动显示器</option>';
        return;
    }

    select.innerHTML = state.displays.map((display) => `
        <option value="${escapeHtml(display.deviceName)}">
            ${escapeHtml(display.friendlyName || display.deviceName)}${display.isPrimary ? "（主显示器）" : ""}
        </option>
    `).join("");

    if (selectedDevice && state.displays.some((item) => item.deviceName === selectedDevice)) {
        select.value = selectedDevice;
    }
}

async function populateModeSelect(deviceName, selectedProfile) {
    const select = byId("mode-select");
    select.disabled = true;
    select.innerHTML = '<option>正在读取模式...</option>';
    try {
        state.modes = await API.enumerateDisplayModes(deviceName);
        select.innerHTML = state.modes.map((mode, index) => `
            <option value="${index}">${mode.width} × ${mode.height} · ${mode.refreshRate} Hz</option>
        `).join("");

        if (selectedProfile) {
            const index = state.modes.findIndex((mode) =>
                mode.width === selectedProfile.width
                && mode.height === selectedProfile.height
                && mode.refreshRate === selectedProfile.refreshRate
            );
            if (index >= 0) {
                select.value = String(index);
            }
        } else {
            const display = state.displays.find((item) => item.deviceName === deviceName);
            const index = state.modes.findIndex((mode) =>
                display
                && mode.width === display.currentMode.width
                && mode.height === display.currentMode.height
                && mode.refreshRate === display.currentMode.refreshRate
            );
            if (index >= 0) {
                select.value = String(index);
            }
        }
    } finally {
        select.disabled = false;
    }
}

function setLaunchType(type, updateTiming = true) {
    if (state.launchType !== type) {
        state.launcherReadyForDetection = false;
    }
    state.launchType = type;
    byId("executable-fields").classList.toggle("hidden", type !== "executable");
    byId("steam-fields").classList.toggle("hidden", type !== "steam");
    byId("shortcut-fields").classList.toggle("hidden", type !== "shortcut");
    if (updateTiming) {
        byId("switch-timing").value =
            type === "executable" ? "beforeLaunch" : "onProcessDetected";
    }
    updateDetectAction();
}

function updateDetectAction() {
    const label = byId("detect-process-label");
    if (state.launchType === "shortcut") {
        label.textContent = state.launcherReadyForDetection ? "开始识别" : "打开启动器";
    } else {
        label.textContent = "启动并识别";
    }
}

function updateProcessHint() {
    const needsIdentification =
        Boolean(state.selectedSource) && !byId("process-name").value.trim();
    byId("process-hint-text").textContent = state.launchType === "shortcut"
        ? state.launcherReadyForDetection
            ? "登录和更新完成后，先点击“开始识别”，再从启动器进入游戏。"
            : "首次添加：先打开启动器并完成登录或更新，然后再识别游戏主进程。"
        : "首次添加：点击“启动并识别”，然后正常进入游戏。识别期间不会切换分辨率。";
    byId("process-hint").classList.toggle("hidden", !needsIdentification);
}

async function openProfileModal(profile = null) {
    clearError();
    byId("profile-form").reset();
    byId("process-candidates").classList.add("hidden");
    byId("process-candidates").innerHTML = "";
    byId("detect-status").classList.add("hidden");
    state.launcherReadyForDetection = false;
    const sourceKey = profile?.launchType === "steam"
        ? `steam:${profile.steamAppId}`
        : profile?.launchType === "shortcut"
            ? `shortcut:${profile.shortcutPath.toLowerCase()}`
            : `recent:${profile?.executable.toLowerCase()}`;
    state.selectedSource = profile ? {
        key: sourceKey,
        name: profile.name,
        detail: profile.launchType === "steam"
            ? `Steam App ID ${profile.steamAppId}`
            : profile.launchType === "shortcut"
                ? profile.shortcutPath
                : profile.executable,
        type: profile.launchType === "steam"
            ? "STEAM"
            : profile.launchType === "shortcut"
                ? (profile.launcherProvider === "Riot Client" ? "RIOT" : "启动器")
                : "EXE",
    } : null;
    byId("profile-id").value = profile?.id || "";
    byId("modal-title").textContent = profile ? "编辑游戏" : "添加游戏";
    byId("game-name").value = profile?.name || "";
    byId("game-executable").value = profile?.executable || "";
    byId("game-arguments").value = profile?.arguments || "";
    byId("working-directory").value = profile?.workingDirectory || "";
    byId("steam-app-id").value = profile?.steamAppId || "";
    byId("shortcut-path").value = profile?.shortcutPath || "";
    byId("process-name").value = profile?.processName || "";
    updateProcessHint();
    byId("restore-delay").value = String((profile?.restoreDelayMs ?? 2000) / 1000);
    byId("switch-timing").value = profile?.switchTiming
        || (profile?.launchType === "executable" ? "beforeLaunch" : "onProcessDetected");
    byId("launch-timeout").value = String((profile?.launchTimeoutSec ?? 600) / 60);
    const initialLaunchType = profile?.launchType || "steam";
    const initialSource = profile
        ? profile.launchType === "steam"
            ? "steam"
            : profile.launchType === "shortcut" ? "shortcut" : "browse"
        : "steam";
    setLaunchType(initialLaunchType, false);
    setSourceTab(initialSource);
    renderSelectedSource();

    const primary = state.displays.find((item) => item.isPrimary) || state.displays[0];
    const deviceName = profile?.displayDevice || primary?.deviceName || "";
    populateDisplaySelect(deviceName);
    byId("profile-modal").classList.remove("hidden");
    await Promise.all([
        deviceName ? populateModeSelect(deviceName, profile) : Promise.resolve(),
        loadSteamGames(),
        loadRecentApps(),
    ]);
    if (profile) {
        byId("game-name").focus();
    } else {
        byId("steam-search").focus();
    }
}

function closeProfileModal() {
    byId("profile-modal").classList.add("hidden");
}

async function saveProfile(event) {
    event.preventDefault();
    clearError();
    const selectedMode = state.modes[Number(byId("mode-select").value)];
    if (!selectedMode) {
        showError("请选择有效的显示模式");
        return;
    }
    if (!byId("process-name").value.trim()) {
        showError("请填写游戏主进程，或使用“启动并识别”");
        byId("process-name").focus();
        return;
    }

    const profile = {
        id: byId("profile-id").value,
        name: byId("game-name").value.trim(),
        launchType: state.launchType,
        executable: byId("game-executable").value.trim(),
        arguments: byId("game-arguments").value.trim(),
        workingDirectory: byId("working-directory").value.trim(),
        steamAppId: byId("steam-app-id").value.trim(),
        shortcutPath: byId("shortcut-path").value.trim(),
        launcherProvider: state.launchType === "shortcut"
            ? (state.selectedSource?.type === "RIOT" ? "Riot Client" : "启动器")
            : "",
        processName: byId("process-name").value.trim(),
        switchTiming: byId("switch-timing").value,
        launchTimeoutSec: Math.round(Number(byId("launch-timeout").value || 10) * 60),
        displayDevice: byId("display-select").value,
        width: selectedMode.width,
        height: selectedMode.height,
        refreshRate: selectedMode.refreshRate,
        restoreDelayMs: Math.round(Number(byId("restore-delay").value || 0) * 1000),
    };

    try {
        await API.saveProfile(profile);
        closeProfileModal();
        await refreshProfiles();
    } catch (error) {
        showError(error);
    }
}

async function browseExecutable(replaceName = false) {
    const path = await API.selectExecutable();
    if (!path) {
        return;
    }
    byId("game-executable").value = path;
    const filename = path.split(/[\\/]/).pop() || "";
    const gameName = filename.replace(/\.exe$/i, "");
    byId("process-name").value = filename;
    if (replaceName || !byId("game-name").value) {
        byId("game-name").value = gameName;
    }
    byId("steam-app-id").value = "";
    byId("shortcut-path").value = "";
    setLaunchType("executable");
    state.selectedSource = {
        key: `recent:${path.toLowerCase()}`,
        name: byId("game-name").value || gameName,
        detail: path,
        type: "EXE",
    };
    renderSelectedSource();
    updateProcessHint();
}

async function browseShortcut(valorantPreset = false) {
    const path = await API.selectShortcut();
    if (!path) {
        return;
    }

    const filename = path.split(/[\\/]/).pop() || "";
    const shortcutName = filename.replace(/\.(lnk|url)$/i, "");
    byId("shortcut-path").value = path;
    byId("game-executable").value = "";
    byId("game-arguments").value = "";
    byId("working-directory").value = "";
    byId("steam-app-id").value = "";
    state.launcherReadyForDetection = false;
    byId("game-name").value = valorantPreset ? "无畏契约" : shortcutName;
    if (valorantPreset) {
        byId("process-name").value = "VALORANT-Win64-Shipping.exe";
        byId("launch-timeout").value = "10";
    } else if (!byId("process-name").value) {
        byId("process-name").value = "";
    }
    setLaunchType("shortcut");
    state.selectedSource = {
        key: `shortcut:${path.toLowerCase()}`,
        name: byId("game-name").value,
        detail: path,
        type: valorantPreset ? "RIOT" : "启动器",
    };
    renderSelectedSource();
    updateProcessHint();
    byId("process-name").focus();
}

async function detectProcess() {
    clearError();
    const request = {
        launchType: state.launchType,
        executable: byId("game-executable").value.trim(),
        arguments: byId("game-arguments").value.trim(),
        workingDirectory: byId("working-directory").value.trim(),
        steamAppId: byId("steam-app-id").value.trim(),
        shortcutPath: byId("shortcut-path").value.trim(),
        launchBeforeDetect: state.launchType !== "shortcut",
    };
    if (request.launchType === "steam" && !request.steamAppId) {
        showError("请先从 Steam 游戏库选择游戏");
        return;
    }
    if (request.launchType === "executable" && !request.executable) {
        showError("请先选择游戏 EXE");
        return;
    }
    if (request.launchType === "shortcut" && !request.shortcutPath) {
        showError("请先选择启动器或游戏快捷方式");
        return;
    }

    const button = byId("detect-process-btn");
    const status = byId("detect-status");
    const candidates = byId("process-candidates");
    button.disabled = true;
    status.textContent = "游戏已启动，正在等待新的可见进程...";
    status.classList.remove("hidden");
    candidates.classList.add("hidden");
    candidates.innerHTML = "";

    try {
        if (request.launchType === "shortcut" && !state.launcherReadyForDetection) {
            status.textContent = "正在打开启动器...";
            await API.openLaunchSource(request);
            state.launcherReadyForDetection = true;
            updateDetectAction();
            updateProcessHint();
            status.textContent =
                "启动器已打开。完成登录和更新后，点击“开始识别”，再从启动器进入游戏。";
            return;
        }

        status.textContent = request.launchType === "shortcut"
            ? "正在等待游戏进程，请现在从启动器进入游戏..."
            : "游戏已启动，正在等待新的可见进程...";
        const applications = await API.detectGameProcess(request);
        if (applications.length === 1) {
            byId("process-name").value = applications[0].processName;
            updateProcessHint();
            status.textContent = `已检测到 ${applications[0].processName}`;
            return;
        }

        status.textContent = "检测到多个候选进程，请选择游戏主进程";
        candidates.innerHTML = applications.map((application, index) => `
            <button type="button" class="candidate-button" data-candidate-index="${index}"
                title="${escapeHtml(application.executable)}">
                ${escapeHtml(application.processName)}
            </button>
        `).join("");
        candidates.dataset.applications = JSON.stringify(applications);
        candidates.classList.remove("hidden");
    } catch (error) {
        status.classList.add("hidden");
        showError(error);
    } finally {
        button.disabled = false;
    }
}

async function handleProfileAction(event) {
    const button = event.target.closest("[data-action]");
    if (!button) {
        return;
    }
    const action = button.dataset.action;
    if (action === "add") {
        await openProfileModal();
        return;
    }

    const item = button.closest(".profile-item");
    const profile = state.profiles.find((entry) => entry.id === item?.dataset.id);
    if (!profile) {
        return;
    }

    if (action === "edit") {
        await openProfileModal(profile);
    } else if (action === "delete") {
        if (confirm(`删除“${profile.name}”的配置？`)) {
            await API.deleteProfile(profile.id);
            await refreshProfiles();
        }
    } else if (action === "launch") {
        clearError();
        try {
            await API.launchProfile(profile.id);
        } catch (error) {
            showError(error);
        }
    }
}

function openDrawer() {
    byId("display-drawer").classList.remove("hidden");
    byId("drawer-overlay").classList.remove("hidden");
}

function closeDrawer() {
    byId("display-drawer").classList.add("hidden");
    byId("drawer-overlay").classList.add("hidden");
}

async function closeWindow() {
    if (state.runtime.canRestore) {
        const shouldClose = confirm("游戏显示模式仍在接管中。恢复桌面并关闭 Resolution Hub？");
        if (!shouldClose) {
            return;
        }
        try {
            await API.restoreDisplay();
        } catch (error) {
            showError(error);
            return;
        }
    }
    await API.close();
}

function toggleTheme() {
    const isLight = document.documentElement.classList.toggle("light");
    window.localStorage.setItem("resolution-hub-theme", isLight ? "light" : "dark");
}

async function openAbout() {
    byId("about-modal").classList.remove("hidden");
    const toggle = byId("auto-start-toggle");
    toggle.disabled = true;
    try {
        toggle.checked = await API.getAutoStartEnabled();
    } finally {
        toggle.disabled = false;
    }
}

function closeAbout() {
    byId("about-modal").classList.add("hidden");
}

async function updateAutoStart(event) {
    const toggle = event.currentTarget;
    const requested = toggle.checked;
    toggle.disabled = true;
    try {
        toggle.checked = await API.setAutoStartEnabled(requested);
    } catch (error) {
        toggle.checked = !requested;
        showError(error);
    } finally {
        toggle.disabled = false;
    }
}

async function init() {
    if (window.localStorage.getItem("resolution-hub-theme") === "light") {
        document.documentElement.classList.add("light");
    }

    byId("add-profile-btn").addEventListener("click", () => openProfileModal().catch(showError));
    byId("profile-list").addEventListener("click", (event) => handleProfileAction(event).catch(showError));
    byId("profile-form").addEventListener("submit", saveProfile);
    byId("modal-close-btn").addEventListener("click", closeProfileModal);
    byId("modal-cancel-btn").addEventListener("click", closeProfileModal);
    byId("browse-executable-btn").addEventListener("click", () => browseExecutable().catch(showError));
    byId("source-browse-btn").addEventListener("click", () => browseExecutable(true).catch(showError));
    byId("browse-shortcut-btn").addEventListener("click", () => browseShortcut().catch(showError));
    byId("source-shortcut-btn").addEventListener("click", () => browseShortcut().catch(showError));
    byId("valorant-preset-btn").addEventListener("click", () => browseShortcut(true).catch(showError));
    byId("source-tabs").addEventListener("click", (event) => {
        const button = event.target.closest("button[data-source]");
        if (button) {
            setSourceTab(button.dataset.source);
        }
    });
    byId("refresh-steam-btn").addEventListener("click", () => loadSteamGames().catch(showError));
    byId("refresh-recent-btn").addEventListener("click", () => loadRecentApps().catch(showError));
    byId("steam-search").addEventListener("input", renderSteamGames);
    byId("steam-game-list").addEventListener("click", (event) => {
        const button = event.target.closest("[data-steam-id]");
        if (!button) {
            return;
        }
        const game = state.steamGames.find((item) => item.appId === button.dataset.steamId);
        if (game) {
            selectSteamGame(game);
        }
    });
    byId("recent-app-list").addEventListener("click", (event) => {
        const button = event.target.closest("[data-recent-index]");
        if (!button) {
            return;
        }
        const application = state.recentApps[Number(button.dataset.recentIndex)];
        if (application) {
            selectRecentApplication(application);
        }
    });
    byId("detect-process-btn").addEventListener("click", () => detectProcess().catch(showError));
    byId("process-candidates").addEventListener("click", (event) => {
        const button = event.target.closest("[data-candidate-index]");
        if (!button) {
            return;
        }
        const applications = JSON.parse(event.currentTarget.dataset.applications || "[]");
        const application = applications[Number(button.dataset.candidateIndex)];
        if (application) {
            byId("process-name").value = application.processName;
            updateProcessHint();
            byId("detect-status").textContent = `已选择 ${application.processName}`;
            byId("process-candidates").classList.add("hidden");
        }
    });
    byId("process-name").addEventListener("input", updateProcessHint);
    byId("shortcut-path").addEventListener("input", () => {
        state.launcherReadyForDetection = false;
        updateDetectAction();
        updateProcessHint();
    });
    byId("display-select").addEventListener("change", (event) => {
        populateModeSelect(event.target.value).catch(showError);
    });
    byId("default-display-select").addEventListener("change", (event) => {
        populateDefaultModeList(event.target.value).catch(showError);
    });
    byId("default-mode-list").addEventListener("change", (event) => {
        if (event.target.name !== "default-mode") {
            return;
        }
        state.selectedDefaultModeIndex = Number(event.target.value);
        renderDefaultModeList(byId("default-display-select").value);
    });
    byId("default-mode-list").addEventListener("click", (event) => {
        const button = event.target.closest("[data-resolution-key]");
        if (!button) {
            return;
        }
        selectDefaultResolution(
            byId("default-display-select").value,
            button.dataset.resolutionKey,
        );
    });
    byId("default-resolution-form").addEventListener("submit", saveDefaultResolution);
    byId("default-resolution-close-btn").addEventListener("click", closeDefaultResolutionDrawer);
    byId("default-resolution-overlay").addEventListener("click", closeDefaultResolutionDrawer);

    byId("displays-btn").addEventListener("click", openDrawer);
    byId("status-display-btn").addEventListener("click", () => {
        openDefaultResolutionDrawer().catch(showError);
    });
    byId("drawer-close-btn").addEventListener("click", closeDrawer);
    byId("drawer-overlay").addEventListener("click", closeDrawer);
    byId("about-btn").addEventListener("click", () => openAbout().catch(showError));
    byId("auto-start-toggle").addEventListener("change", updateAutoStart);
    byId("about-close-btn").addEventListener("click", closeAbout);
    byId("about-modal").addEventListener("click", (event) => {
        if (event.target === event.currentTarget) {
            closeAbout();
        }
    });
    byId("theme-btn").addEventListener("click", toggleTheme);
    byId("error-close-btn").addEventListener("click", clearError);
    byId("restore-btn").addEventListener("click", async () => {
        try {
            await API.restoreDisplay();
        } catch (error) {
            showError(error);
        }
    });
    byId("cancel-wait-btn").addEventListener("click", async () => {
        try {
            await API.cancelLaunchWait();
        } catch (error) {
            showError(error);
        }
    });
    byId("minimize-btn").addEventListener("click", () => API.minimize());
    byId("maximize-btn").addEventListener("click", () => API.toggleMaximize());
    byId("close-btn").addEventListener("click", () => closeWindow().catch(showError));

    document.addEventListener("keydown", (event) => {
        if (event.key === "Escape") {
            closeProfileModal();
            closeDefaultResolutionDrawer();
            closeDrawer();
            closeAbout();
        }
    });

    await API.listenRuntime(updateRuntime);

    const pending = await API.getPendingRestore();
    if (pending && confirm(`检测到“${pending.profileName}”留下的桌面恢复点，立即恢复？`)) {
        await API.restoreDisplay();
    }

    const [runtime, settings] = await Promise.all([
        API.getRuntimeStatus(),
        API.getSettings(),
        refreshDisplays(),
        refreshProfiles(),
    ]);
    state.settings = settings;
    renderDisplays();
    updateRuntime(runtime);
}

init().catch(showError);
