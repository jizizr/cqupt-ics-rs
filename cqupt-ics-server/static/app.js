const providerSelect = document.getElementById("provider");
const providerDescription = document.getElementById("provider-description");
const toastContainer = document.getElementById("toast-container");
const form = document.getElementById("ics-form");
const usernameInput = document.getElementById("username");
const passwordInput = document.getElementById("password");
const togglePassword = document.getElementById("toggle-password");
const startDateInput = document.getElementById("start-date");
const formatSelect = document.getElementById("format");
const rememberSettings = document.getElementById("remember-settings");
const rememberPassword = document.getElementById("remember-password");
const clearButton = document.getElementById("clear-form");
const resultCard = document.getElementById("result-card");
const linkBox = document.getElementById("ics-link");
const copyButton = document.getElementById("copy-link");
const importButton = document.getElementById("import-calendar");
const genericButton = document.getElementById("open-generic");
const formatHint = document.getElementById("format-hint");
const resultHints = document.getElementById("result-hints");
const currentYear = document.getElementById("current-year");

const SETTINGS_KEY = "cqupt-ics-settings";
const PASSWORD_KEY = "cqupt-ics-password";

let lastResult = null;

const EyeIcon = `
    <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 5c-5.2 0-9.54 3.11-11.25 7C2.46 15.89 6.8 19 12 19s9.54-3.11 11.25-7C21.54 8.11 17.2 5 12 5Zm0 12a5 5 0 1 1 0-10 5 5 0 0 1 0 10Zm0-2a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" fill="currentColor"/>
    </svg>
`;

const EyeOffIcon = `
    <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="m3.28 2.22-1.06 1.27 3.07 3.07C3.03 8.41 1.6 10.07 1 12c1.71 3.89 6.05 7 11 7 2.02 0 3.92-.47 5.6-1.3L20.5 21l1.28-1.28ZM15.28 17.54C14.42 17.83 13.47 18 12 18c-4.26 0-7.64-2.7-8.95-6 .52-1.21 1.33-2.3 2.39-3.2l2.16 2.16A5 5 0 0 0 15 15.95l.28.28Zm-4-4.01L9.59 11.3A3 3 0 0 1 11 9l.28.02L14 11.73A3 3 0 0 1 11.28 13.53ZM21.25 12c-.33.8-.8 1.55-1.37 2.24l-1.36-1.36c.29-.55.48-1.18.48-1.88a5 5 0 0 0-6-4.9l-1.68-1.68c.44-.05.89-.07 1.33-.07 4.26 0 7.64 2.7 8.95 6Z" fill="currentColor"/>
    </svg>
`;

const detectPlatform = () => {
    const uaDataPlatform = navigator.userAgentData?.platform || navigator.platform || "";
    const ua = (navigator.userAgent || navigator.vendor || window.opera || "").toLowerCase();

    const check = (pattern) => pattern.test(ua);
    const platform = uaDataPlatform.toLowerCase();

    if (check(/android/)) {
        return { platform: "android", label: "Android" };
    }

    if (check(/iphone|ipad|ipod/)) {
        return { platform: "apple", label: "iOS" };
    }

    if (check(/macintosh|mac os x/) || platform.includes("mac")) {
        return { platform: "apple", label: "macOS" };
    }

    if (check(/windows/)) {
        return { platform: "windows", label: "Windows" };
    }

    if (check(/cros/)) {
        return { platform: "chromeos", label: "ChromeOS" };
    }

    if (check(/linux/)) {
        return { platform: "linux", label: "Linux" };
    }

    return { platform: "unknown", label: uaDataPlatform || "当前系统" };
};

currentYear.textContent = new Date().getFullYear();

const showToast = (message, tone = "info", options = {}) => {
    if (!toastContainer) {
        return null;
    }
    const toast = document.createElement("div");
    toast.className = `toast${tone === "error" ? " toast--error" : tone === "success" ? " toast--success" : ""}`;

    const icon = document.createElement("span");
    icon.className = "toast__icon";
    icon.textContent = tone === "error" ? "!" : tone === "success" ? "✓" : "i";
    toast.appendChild(icon);

    const text = document.createElement("span");
    text.textContent = message;
    toast.appendChild(text);

    const close = document.createElement("button");
    close.className = "toast__close";
    close.type = "button";
    close.setAttribute("aria-label", "关闭通知");
    close.innerHTML = "&times;";
    close.addEventListener("click", () => dismissToast(toast));
    toast.appendChild(close);

    toastContainer.appendChild(toast);

    const ttl = options.ttl ?? (tone === "error" ? 6000 : 3500);
    if (ttl > 0) {
        const timeoutId = window.setTimeout(() => dismissToast(toast), ttl);
        toast.dataset.timeoutId = String(timeoutId);
    }

    return toast;
};

const dismissToast = (toast) => {
    if (!toast || toast.dataset.dismissed) {
        return;
    }
    toast.dataset.dismissed = "true";
    const timeoutId = toast.dataset.timeoutId;
    if (timeoutId) {
        window.clearTimeout(Number(timeoutId));
    }
    toast.style.animation = "toast-out 0.2s ease-in forwards";
    toast.addEventListener(
        "animationend",
        () => {
            toast.remove();
        },
        { once: true },
    );
};

const markDirty = () => {
    lastResult = null;
};

const setPasswordVisibility = (visible) => {
    passwordInput.type = visible ? "text" : "password";
    togglePassword.innerHTML = visible ? EyeOffIcon : EyeIcon;
    togglePassword.setAttribute("aria-label", visible ? "隐藏密码" : "显示密码");
    togglePassword.setAttribute("title", visible ? "隐藏密码" : "显示密码");
    togglePassword.setAttribute("data-visible", visible ? "true" : "false");
};

const applyProviderDescription = () => {
    const option = providerSelect.selectedOptions[0];
    providerDescription.textContent = option?.dataset.description ?? "";
};

const defaultSettings = () => ({
    rememberSettings: true,
    rememberPassword: false,
    lastProvider: "",
    startDate: "",
    format: "ics",
    providers: {},
});

const loadSettings = () => {
    const fallback = defaultSettings();
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) {
        return fallback;
    }
    try {
        const parsed = JSON.parse(raw);
        if (!parsed || typeof parsed !== "object") {
            return fallback;
        }
        const withDefaults = {
            ...fallback,
            rememberSettings: parsed.rememberSettings ?? fallback.rememberSettings,
            rememberPassword: parsed.rememberPassword ?? fallback.rememberPassword,
            lastProvider: parsed.lastProvider ?? parsed.provider ?? fallback.lastProvider,
            startDate: parsed.startDate ?? fallback.startDate,
            format: parsed.format ?? fallback.format,
            providers: {},
        };
        const source =
            parsed.providers && typeof parsed.providers === "object"
                ? parsed.providers
                : parsed.provider
                  ? { [parsed.provider]: { username: parsed.username ?? "" } }
                  : {};
        for (const [key, value] of Object.entries(source)) {
            if (value && typeof value === "object") {
                withDefaults.providers[key] = {
                    username: value.username ?? "",
                };
            }
        }
        return withDefaults;
    } catch {
        return fallback;
    }
};

const loadPasswordStore = (fallbackProvider) => {
    const raw = localStorage.getItem(PASSWORD_KEY);
    if (!raw) {
        return {};
    }
    try {
        const parsed = JSON.parse(raw);
        if (typeof parsed === "string") {
            return fallbackProvider ? { [fallbackProvider]: parsed } : {};
        }
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
            const store = {};
            for (const [key, value] of Object.entries(parsed)) {
                if (typeof value === "string") {
                    store[key] = value;
                }
            }
            return store;
        }
    } catch {
        if (fallbackProvider) {
            return { [fallbackProvider]: raw };
        }
    }
    return {};
};

let settingsState = loadSettings();
if (!settingsState.rememberSettings) {
    settingsState.rememberPassword = false;
}

let passwordStore = loadPasswordStore(settingsState.lastProvider);

const persistSettings = () => {
    if (settingsState.rememberSettings) {
        const payload = {
            rememberSettings: true,
            rememberPassword: settingsState.rememberPassword,
            lastProvider: settingsState.lastProvider,
            startDate: settingsState.startDate,
            format: settingsState.format,
            providers: settingsState.providers,
        };
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(payload));
    } else {
        localStorage.removeItem(SETTINGS_KEY);
    }
};

const persistPasswords = () => {
    if (settingsState.rememberSettings && settingsState.rememberPassword) {
        localStorage.setItem(PASSWORD_KEY, JSON.stringify(passwordStore));
    } else {
        localStorage.removeItem(PASSWORD_KEY);
    }
};

const applySettingsToForm = () => {
    rememberSettings.checked = settingsState.rememberSettings;
    rememberPassword.checked =
        settingsState.rememberSettings && settingsState.rememberPassword;
    rememberPassword.disabled = !settingsState.rememberSettings;
    startDateInput.value = settingsState.startDate ?? "";
    formatSelect.value = settingsState.format ?? "ics";
};

let activeProvider = "";

const fillCredentialsForProvider = (provider) => {
    const target = provider?.trim();
    if (!target) {
        usernameInput.value = "";
        passwordInput.value = "";
        setPasswordVisibility(false);
        return;
    }
    if (!settingsState.rememberSettings) {
        usernameInput.value = "";
        passwordInput.value = "";
        setPasswordVisibility(false);
        return;
    }
    const entry = settingsState.providers[target];
    usernameInput.value = entry?.username ?? "";
    if (settingsState.rememberPassword) {
        passwordInput.value = passwordStore[target] ?? "";
    } else {
        passwordInput.value = "";
    }
    setPasswordVisibility(false);
};

const captureCurrentProviderCredentials = () => {
    if (!settingsState.rememberSettings) {
        return;
    }
    const provider = activeProvider?.trim();
    if (!provider) {
        return;
    }
    if (!settingsState.providers[provider]) {
        settingsState.providers[provider] = {};
    }
    settingsState.providers[provider].username = usernameInput.value.trim();
    if (settingsState.rememberPassword) {
        passwordStore[provider] = passwordInput.value;
    } else {
        delete passwordStore[provider];
    }
};

const savePreferences = (data) => {
    settingsState.lastProvider = data.provider;
    settingsState.startDate = data.startDate;
    settingsState.format = data.format;
    settingsState.rememberSettings = rememberSettings.checked;
    settingsState.rememberPassword =
        rememberSettings.checked && rememberPassword.checked;

    if (settingsState.rememberSettings && data.provider) {
        if (!settingsState.providers[data.provider]) {
            settingsState.providers[data.provider] = {};
        }
        settingsState.providers[data.provider].username = data.username;
        if (settingsState.rememberPassword) {
            passwordStore[data.provider] = data.password;
        } else {
            delete passwordStore[data.provider];
        }
    }

    if (!settingsState.rememberSettings) {
        settingsState.providers = {};
        passwordStore = {};
    }

    persistSettings();
    persistPasswords();
};

applySettingsToForm();

const generationInputs = [
    providerSelect,
    usernameInput,
    passwordInput,
    startDateInput,
    formatSelect,
];

generationInputs.forEach((element) => {
    element.addEventListener("input", markDirty);
    element.addEventListener("change", markDirty);
});

startDateInput.addEventListener("change", () => {
    settingsState.startDate = startDateInput.value;
    if (settingsState.rememberSettings) {
        persistSettings();
    }
});

formatSelect.addEventListener("change", () => {
    settingsState.format = formatSelect.value;
    if (settingsState.rememberSettings) {
        persistSettings();
    }
});

setPasswordVisibility(false);

const legacyCopy = (text) => {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    textarea.style.pointerEvents = "none";
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    textarea.setSelectionRange(0, textarea.value.length);
    let success = false;
    try {
        success = document.execCommand("copy");
    } catch {
        success = false;
    }
    document.body.removeChild(textarea);
    return success;
};

const copyToClipboard = async (text) => {
    if (navigator.clipboard?.writeText) {
        try {
            await navigator.clipboard.writeText(text);
            return true;
        } catch {
            // fallback to legacy copy
        }
    }
    return legacyCopy(text);
};

const buildCalendarLinks = (baseUrl, format) => {
    const url = new URL(baseUrl);
    const isIcs = format === "ics";

    const plain = url.toString();
    const apple = isIcs ? plain.replace(/^https?:\/\//i, "webcal://") : "";
    const android = isIcs
        ? (() => {
            const scheme = url.protocol.replace(":", "");
            const hostAndPath = `${url.host}${url.pathname}${url.search}`;
            return `intent://${hostAndPath}#Intent;scheme=${scheme};action=android.intent.action.VIEW;type=text/calendar;end`;
        })()
        : "";

    return { plain, apple, android, isIcs };
};

const populateProviders = async () => {
    const loadingToast = showToast("正在加载 provider 列表…", "info", { ttl: 0 });
    try {
        const response = await fetch("/providers", { cache: "no-store" });
        if (!response.ok) {
            throw new Error("无法加载 provider 列表");
        }
        const payload = await response.json();
        const providers = Array.isArray(payload.providers) ? payload.providers : [];

        providerSelect.innerHTML = '<option value="">请选择 provider</option>';
        providers.forEach((p) => {
            if (p?.name) {
                const option = document.createElement("option");
                option.value = p.name;
                option.textContent = p.description
                    ? `${p.name} · ${p.description}`
                    : p.name;
                option.dataset.description = p.description ?? "";
                providerSelect.appendChild(option);
            }
        });

        const storedProvider = settingsState.lastProvider;
        if (storedProvider && providers.some((p) => p?.name === storedProvider)) {
            providerSelect.value = storedProvider;
        } else {
            providerSelect.value = "";
        }

        activeProvider = providerSelect.value.trim();
        settingsState.lastProvider = activeProvider;
        if (settingsState.rememberSettings) {
            persistSettings();
        }
        applyProviderDescription();
        fillCredentialsForProvider(activeProvider);
        if (loadingToast) {
            dismissToast(loadingToast);
        }
        showToast("provider 列表加载完成，可以开始配置啦。", "success");
    } catch (error) {
        console.error(error);
        providerSelect.innerHTML = '<option value="">加载失败</option>';
        if (loadingToast) {
            dismissToast(loadingToast);
        }
        showToast("获取 provider 列表失败，请刷新后重试。", "error", { ttl: 7000 });
    }
};

providerSelect.addEventListener("change", () => {
    if (settingsState.rememberSettings) {
        captureCurrentProviderCredentials();
        persistSettings();
        persistPasswords();
    }
    activeProvider = providerSelect.value.trim();
    settingsState.lastProvider = activeProvider;
    applyProviderDescription();
    fillCredentialsForProvider(activeProvider);
    if (settingsState.rememberSettings) {
        persistSettings();
    }
});

togglePassword.addEventListener("click", () => {
    const nextVisible = passwordInput.type === "password";
    setPasswordVisibility(nextVisible);
});

rememberPassword.addEventListener("change", () => {
    if (!rememberSettings.checked && rememberPassword.checked) {
        rememberPassword.checked = false;
        return;
    }
    settingsState.rememberPassword =
        rememberSettings.checked && rememberPassword.checked;
    if (!settingsState.rememberPassword) {
        passwordStore = {};
    } else if (activeProvider) {
        passwordStore[activeProvider] = passwordInput.value;
    }
    persistSettings();
    persistPasswords();
});

rememberSettings.addEventListener("change", () => {
    settingsState.rememberSettings = rememberSettings.checked;
    rememberPassword.disabled = !settingsState.rememberSettings;
    if (!settingsState.rememberSettings) {
        settingsState.rememberPassword = false;
        rememberPassword.checked = false;
        settingsState.providers = {};
        passwordStore = {};
        persistSettings();
        persistPasswords();
    } else {
        settingsState.rememberPassword = rememberPassword.checked;
        if (activeProvider) {
            captureCurrentProviderCredentials();
        }
        settingsState.startDate = startDateInput.value;
        settingsState.format = formatSelect.value;
        settingsState.lastProvider = providerSelect.value.trim();
        persistSettings();
        persistPasswords();
        fillCredentialsForProvider(activeProvider);
    }
});

clearButton.addEventListener("click", () => {
    providerDescription.textContent = "";
    showToast("表单已清空，可以重新填写。", "info");
    resultCard.hidden = true;
    linkBox.value = "";
    usernameInput.value = "";
    passwordInput.value = "";
    startDateInput.value = "";
    formatSelect.value = "ics";
    setPasswordVisibility(false);
    formatHint.classList.remove("error");
    formatHint.textContent = "iCalendar 订阅可直接导入";
    resultHints.innerHTML = `
        <li>iOS 会自动尝试通过 webcal:// 打开日历，若未弹窗，可在「设置 › 邮件 › 帐号 › 添加已订阅的日历」中粘贴链接。</li>
        <li>Android 将尝试 intent:// 协议，若设备不支持，可手动复制链接到系统日历或 Google 日历中。</li>
    `;
    importButton.disabled = false;
    lastResult = null;
    if (settingsState.rememberSettings && activeProvider) {
        if (!settingsState.providers[activeProvider]) {
            settingsState.providers[activeProvider] = {};
        }
        settingsState.providers[activeProvider].username = "";
        if (settingsState.rememberPassword) {
            delete passwordStore[activeProvider];
        }
    }
    settingsState.startDate = "";
    settingsState.format = "ics";
    persistSettings();
    persistPasswords();
});

const generateLink = () => {
    const provider = providerSelect.value.trim();
    const username = usernameInput.value.trim();
    const password = passwordInput.value;
    const startDate = startDateInput.value;
    const format = formatSelect.value;

    if (!provider) {
        showToast("请选择一个 provider。", "error", { ttl: 5000 });
        providerSelect.focus();
        return null;
    }

    if (!username) {
        showToast("请输入账号。", "error", { ttl: 5000 });
        usernameInput.focus();
        return null;
    }

    if (!password) {
        showToast("请输入密码。", "error", { ttl: 5000 });
        passwordInput.focus();
        return null;
    }

    const url = new URL("/courses", window.location.origin);
    url.searchParams.set("provider", provider);
    url.searchParams.set("username", username);
    url.searchParams.set("password", password);

    if (startDate) {
        url.searchParams.set("start_date", startDate);
    }

    if (format && format !== "ics") {
        url.searchParams.set("format", format);
    } else {
        url.searchParams.delete("format");
    }

    const wasHidden = resultCard.hidden;
    const result = buildCalendarLinks(url.toString(), format);

    linkBox.value = result.plain;
    resultCard.hidden = false;
    formatHint.classList.remove("error");
    importButton.disabled = !result.isIcs;
    formatHint.textContent = result.isIcs
        ? "iCalendar 订阅可直接导入"
        : "JSON 格式仅供调试，不支持日历订阅";

    if (result.isIcs) {
        resultHints.innerHTML = `
            <li>若 iOS 未弹窗，可在「设置 › 邮件 › 帐号 › 添加已订阅的日历」中粘贴链接。</li>
            <li>Android 将尝试 intent:// 协议，若不支持，可复制链接到系统日历或 Google 日历。</li>
        `;
    } else {
        resultHints.innerHTML = `
            <li>当前为 JSON 格式，仅适合开发调试，请切换回 iCalendar 以同步课程表。</li>
        `;
    }

    savePreferences({ provider, username, password, startDate, format });
    showToast("链接生成成功，请选择导入方式。", "success");

    if (wasHidden) {
        requestAnimationFrame(() => {
            resultCard.scrollIntoView({ behavior: "smooth", block: "start" });
        });
    }

    lastResult = result;
    return result;
};

const ensureGenerated = () => lastResult ?? generateLink();

form.addEventListener("submit", (event) => {
    event.preventDefault();
    generateLink();
});

copyButton.addEventListener("click", async () => {
    const result = ensureGenerated();
    if (!result) {
        return;
    }

    if (await copyToClipboard(result.plain)) {
        formatHint.textContent = "链接已复制，快去订阅吧！";
        formatHint.classList.remove("error");
    } else {
        formatHint.textContent = "复制失败，请手动选择链接复制。";
        formatHint.classList.add("error");
        linkBox.focus();
        linkBox.select();
    }
});

importButton.addEventListener("click", () => {
    const result = ensureGenerated();
    if (!result) {
        return;
    }

    if (!result.isIcs) {
        formatHint.textContent = "请切换到 iCalendar 格式以导入日历。";
        formatHint.classList.add("error");
        return;
    }

    const { platform, label } = detectPlatform();

    const openIntent = () => {
        if (result.android) {
            window.location.href = result.android;
            return true;
        }
        return false;
    };

    const openWebcal = () => {
        if (result.apple) {
            window.location.href = result.apple;
            return true;
        }
        return false;
    };

    const promptManualImport = (systemLabel) => {
        showToast(`暂不支持 ${systemLabel} 的一键导入，请复制订阅链接后在日历应用中手动添加。`, "info", {
            ttl: 7000,
        });
        linkBox.focus();
        linkBox.select();
    };

    if (platform === "android") {
        if (!openIntent()) {
            promptManualImport(label || "Android");
        } else {
            setTimeout(() => {
                if (document.visibilityState === "visible") {
                    promptManualImport(label || "Android");
                }
            }, 1200);
        }
        return;
    }

    if (platform === "apple") {
        if (!openWebcal()) {
            promptManualImport(label || "Apple");
        } else {
            setTimeout(() => {
                if (document.visibilityState === "visible") {
                    promptManualImport(label || "Apple");
                }
            }, 1200);
        }
        return;
    }

    promptManualImport(label || "当前系统");
});

genericButton.addEventListener("click", () => {
    const result = ensureGenerated();
    if (!result) {
        return;
    }

    window.open(result.plain, "_blank", "noopener");
});

populateProviders();
