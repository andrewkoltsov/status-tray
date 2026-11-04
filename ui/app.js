// Settings window. Plain JS, no bundler. Changes apply immediately (no Save button).
const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);
let catalog = [];
let enabled = new Set();
let info = null;

function showError(e) {
  const box = $("error");
  box.textContent = String(e);
  box.hidden = false;
  setTimeout(() => { box.hidden = true; }, 6000);
}

// Run an IPC call; on failure show the error and resync from the backend.
async function call(cmd, args) {
  try {
    await invoke(cmd, args);
  } catch (e) {
    showError(e);
    await loadSettings();
    render();
  }
}

function domain(url) {
  try { return new URL(url).hostname; } catch { return url; }
}

function render() {
  const q = $("search").value.trim().toLowerCase();
  const list = $("services");
  list.replaceChildren();

  const shown = catalog.filter((s) =>
    !q || s.name.toLowerCase().includes(q) || s.id.includes(q) || domain(s.url).includes(q));

  for (const s of shown) {
    const li = document.createElement("li");
    const label = document.createElement("label");

    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = enabled.has(s.id);
    box.addEventListener("change", () => {
      if (box.checked) enabled.add(s.id); else enabled.delete(s.id);
      call("set_enabled", { serviceId: s.id, enabled: box.checked });
    });

    const name = document.createElement("span");
    name.className = "name";
    name.textContent = s.name;

    const host = document.createElement("span");
    host.className = "domain";
    host.textContent = domain(s.url);

    label.append(box, name, host);
    li.append(label);
    list.append(li);
  }
  $("empty").hidden = shown.length > 0;
}

async function loadSettings() {
  const s = await invoke("get_settings");
  enabled = new Set(s.enabled);
  $("interval").value = String(s.interval_min);
  $("autostart").checked = s.autostart;
}

function renderInfo() {
  $("version").textContent = `Status Tray v${info.version}`;
  const box = $("update");
  if (info.update) {
    box.textContent = "";
    const a = document.createElement("a");
    a.href = "#";
    a.textContent = `Update available: v${info.update.version}`;
    a.addEventListener("click", (ev) => { ev.preventDefault(); call("open_url", { url: info.update.url }); });
    box.append(a);
    box.hidden = false;
  }
}

function link(id, getUrl) {
  $(id).addEventListener("click", (ev) => {
    ev.preventDefault();
    if (info) call("open_url", { url: getUrl() });
  });
}

async function init() {
  $("search").addEventListener("input", render);
  $("interval").addEventListener("change", (ev) =>
    call("set_interval", { minutes: Number(ev.target.value) }));
  $("autostart").addEventListener("change", (ev) =>
    call("set_autostart", { enabled: ev.target.checked }));
  link("request", () => info.issue_url);
  link("releases", () => info.releases_url);

  try {
    [catalog, info] = await Promise.all([invoke("get_catalog"), invoke("get_app_info")]);
    await loadSettings();
    render();
    renderInfo();
  } catch (e) {
    showError(e);
  }
}

init();
