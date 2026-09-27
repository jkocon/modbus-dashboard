"use strict";

const ALL_BAUDS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 128000, 256000];

let scanPollTimer = null;
let currentScanJobId = null;
let tempLoopTimer = null;

// ===================== Helpers =====================

function $(id) { return document.getElementById(id); }

function nowTime() {
  return new Date().toLocaleTimeString();
}

function appendLog(el, text) {
  el.textContent += text + "\n";
  el.scrollTop = el.scrollHeight;
}

function setStatus(el, text, kind) {
  el.textContent = text;
  el.removeAttribute("data-i18n-dynamic");
  el.classList.remove("ok", "error");
  if (kind) el.classList.add(kind);
}

function currentTransport() {
  return document.querySelector('input[name="transport"]:checked').value;
}

function getConnection(extra) {
  extra = extra || {};
  const transport = currentTransport();
  const mbap = transport === "tcp" && $("mbap").checked;
  const conn = { transport, mbap, timeoutMs: extra.timeoutMs != null ? extra.timeoutMs : 300 };

  if (transport === "serial") {
    const port = $("comPort").value;
    if (!port) throw new Error(t("selectComPort"));
    conn.port = port;
    conn.baud = extra.baud != null ? extra.baud : 9600;
    conn.dataBits = parseInt($("dataBits").value, 10);
    conn.parity = $("parity").value;
    conn.stopBits = $("stopBits").value;
    conn.flowControl = $("flowControl").value;
  } else {
    const ip = $("ipAddress").value.trim();
    if (!ip) throw new Error(t("enterIp"));
    conn.ip = ip;
    conn.tcpPort = parseInt($("tcpPort").value, 10) || 502;
  }
  return conn;
}

async function apiPost(path, body) {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `Request failed (${res.status}).`);
  return data;
}

// ===================== Theme (light/dark) =====================

function applyTheme(theme) {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") {
    root.setAttribute("data-theme", theme);
  } else {
    root.removeAttribute("data-theme");
  }
}

function initTheme() {
  const saved = storageGet("theme");
  applyTheme(saved);
}

function toggleTheme() {
  const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const current = document.documentElement.getAttribute("data-theme") || (isDark ? "dark" : "light");
  const next = current === "dark" ? "light" : "dark";
  storageSet("theme", next);
  applyTheme(next);
}

// ===================== Connection panel =====================

function onTransportChange() {
  const isSerial = currentTransport() === "serial";
  $("serialRow").classList.toggle("hidden", !isSerial);
  $("serialParamsRow").classList.toggle("hidden", !isSerial);
  $("tcpRow").classList.toggle("hidden", isSerial);
  $("mbapRow").classList.toggle("hidden", isSerial);
  $("scanBaudBox").classList.toggle("hidden", !isSerial);
  $("tempBaudRow").classList.toggle("hidden", !isSerial);
  $("addrBaudRow").classList.toggle("hidden", !isSerial);
  $("baudCurBaudRow").classList.toggle("hidden", !isSerial);
  $("coilsBaudRow").classList.toggle("hidden", !isSerial);

  if (!isSerial) {
    const timeoutInput = $("timeoutMs");
    if (parseInt(timeoutInput.value, 10) < 500) timeoutInput.value = 500;
  }
}

async function refreshPorts() {
  const select = $("comPort");
  const current = select.value;
  try {
    const res = await fetch("/api/ports");
    const data = await res.json();
    select.innerHTML = "";
    (data.ports || []).forEach((p) => {
      const opt = document.createElement("option");
      opt.value = p;
      opt.textContent = p;
      select.appendChild(opt);
    });
    if (data.ports && data.ports.includes(current)) select.value = current;
  } catch (e) {
    console.error("Failed to fetch COM port list:", e);
  }
}

// ===================== Tabs =====================

function initTabs() {
  document.querySelectorAll(".tab-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".tab-btn").forEach((b) => b.classList.remove("active"));
      document.querySelectorAll(".tab-page").forEach((p) => p.classList.remove("active"));
      btn.classList.add("active");
      $("tab-" + btn.dataset.tab).classList.add("active");
    });
  });
}

// ===================== Scan =====================

function initBaudList() {
  const container = $("baudList");
  container.innerHTML = "";
  ALL_BAUDS.forEach((b, i) => {
    const label = document.createElement("label");
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.value = String(b);
    cb.checked = i < 5; // default 1200-19200
    label.appendChild(cb);
    label.appendChild(document.createTextNode(" " + b));
    container.appendChild(label);
  });
}

function getCheckedBauds() {
  return Array.from($("baudList").querySelectorAll("input:checked")).map((cb) => parseInt(cb.value, 10));
}

function setScanButtons(scanning) {
  $("btnScan").disabled = scanning;
  $("btnCancelScan").disabled = !scanning;
}

function renderScanResults(found) {
  const tbody = $("scanResults");
  tbody.innerHTML = "";
  for (const f of found) {
    const tr = document.createElement("tr");
    const baudCell = document.createElement("td");
    const addrCell = document.createElement("td");
    const respCell = document.createElement("td");
    baudCell.textContent = f.baud != null ? f.baud : t("networkWord");
    addrCell.textContent = f.slaveId;
    respCell.textContent = f.response;
    tr.appendChild(baudCell);
    tr.appendChild(addrCell);
    tr.appendChild(respCell);
    tbody.appendChild(tr);
  }
}

async function startScan() {
  $("scanResults").innerHTML = "";
  $("scanProgressBar").style.width = "0%";
  setStatus($("scanStatus"), t("statusStarting"));

  try {
    const timeoutMs = parseInt($("timeoutMs").value, 10);
    const connection = getConnection({ timeoutMs });
    const transport = currentTransport();

    let bauds = null;
    if (transport === "serial") {
      bauds = getCheckedBauds();
      if (bauds.length === 0) {
        alert(t("selectBaud"));
        return;
      }
    }

    const body = {
      connection,
      bauds,
      startAddress: parseInt($("startAddr").value, 10),
      endAddress: parseInt($("endAddr").value, 10),
      functionCode: parseInt($("funcCode").value, 10),
      register: parseInt($("register").value, 10),
      quantity: parseInt($("quantity").value, 10),
      discoverAddress: $("discoverAddress").checked,
    };

    setScanButtons(true);
    const data = await apiPost("/api/scan", body);
    currentScanJobId = data.jobId;
    pollScan();
  } catch (e) {
    setStatus($("scanStatus"), t("errorPrefix") + e.message, "error");
    setScanButtons(false);
  }
}

function pollScan() {
  if (scanPollTimer) clearInterval(scanPollTimer);
  scanPollTimer = setInterval(async () => {
    if (!currentScanJobId) return;
    try {
      const res = await fetch(`/api/scan/${currentScanJobId}`);
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || t("scanJobGone"));

      const p = data.progress || { done: 0, total: 1, status: "" };
      const pct = p.total ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
      $("scanProgressBar").style.width = pct + "%";
      renderScanResults(data.found || []);

      if (data.done) {
        const found = (data.found || []).length;
        if (data.error) {
          setStatus($("scanStatus"), t("errorPrefix") + data.error, "error");
        } else {
          setStatus($("scanStatus"), t("scanDoneFound", { n: found }), found > 0 ? "ok" : null);
        }
        clearInterval(scanPollTimer);
        scanPollTimer = null;
        currentScanJobId = null;
        setScanButtons(false);
      } else {
        setStatus($("scanStatus"), p.status || t("statusScanning"));
      }
    } catch (e) {
      setStatus($("scanStatus"), t("errorPrefix") + e.message, "error");
      clearInterval(scanPollTimer);
      scanPollTimer = null;
      currentScanJobId = null;
      setScanButtons(false);
    }
  }, 300);
}

async function cancelScan() {
  if (!currentScanJobId) return;
  try {
    await fetch(`/api/scan/${currentScanJobId}/cancel`, { method: "POST" });
  } catch (e) {
    console.error("Failed to cancel scan:", e);
  }
}

// ===================== Temperature (R4DCB08) =====================

function translateDeviceStatus(status) {
  if (status === "OK") return t("statusOk");
  if (status === "No sensor") return t("statusNoSensor");
  return status;
}

function renderTempResults(results) {
  const tbody = $("tempResults");
  tbody.innerHTML = "";
  for (const r of results) {
    const tr = document.createElement("tr");
    const chCell = document.createElement("td");
    const tempCell = document.createElement("td");
    const statusCell = document.createElement("td");
    chCell.textContent = r.channel;
    tempCell.textContent = r.temperatureC === null ? t("dash") : r.temperatureC;
    statusCell.textContent = translateDeviceStatus(r.status);
    tr.appendChild(chCell);
    tr.appendChild(tempCell);
    tr.appendChild(statusCell);
    tbody.appendChild(tr);
  }
}

async function doTempRead() {
  try {
    const baud = parseInt($("tempBaud").value, 10);
    const connection = getConnection({ baud, timeoutMs: 300 });
    const slaveId = parseInt($("tempAddr").value, 10);
    const data = await apiPost("/api/read-temperature", { connection, slaveId });
    renderTempResults(data.results);
    setStatus($("tempStatus"), t("readOk", { time: nowTime() }), "ok");
  } catch (e) {
    setStatus($("tempStatus"), t("errorPrefix") + e.message, "error");
  }
}

function onTempLoopChange(e) {
  if (e.target.checked) {
    const interval = Math.max(1, parseInt($("tempInterval").value, 10)) * 1000;
    tempLoopTimer = setInterval(doTempRead, interval);
    $("btnReadOnce").disabled = true;
    doTempRead();
  } else {
    if (tempLoopTimer) clearInterval(tempLoopTimer);
    tempLoopTimer = null;
    $("btnReadOnce").disabled = false;
  }
}

// ===================== Change address =====================

async function doSetAddress() {
  const log = $("addrLog");
  try {
    const baud = parseInt($("addrBaud").value, 10);
    const connection = getConnection({ baud, timeoutMs: 300 });
    const oldAddress = parseInt($("oldAddr").value, 10);
    const newAddress = parseInt($("newAddr").value, 10);

    appendLog(log, t("logSendingAddr", { time: nowTime(), old: oldAddress, new: newAddress }));
    const data = await apiPost("/api/set-address", { connection, oldAddress, newAddress });

    appendLog(log, t("logSent", { hex: data.request }));
    if (!data.response) {
      appendLog(log, t("logNoResponse"));
      return;
    }
    appendLog(log, t("logReceived", { hex: data.response }));
    appendLog(
      log,
      data.success
        ? t("logAddrSuccess", { old: oldAddress, new: newAddress })
        : t("logEchoMismatch")
    );
  } catch (e) {
    appendLog(log, t("logErrorPrefix", { msg: e.message }));
  }
}

// ===================== Change baud rate =====================

async function doSetBaudRate() {
  const log = $("baudLog");
  try {
    const baud = parseInt($("baudCurBaud").value, 10);
    const connection = getConnection({ baud, timeoutMs: 300 });
    const slaveId = parseInt($("baudAddr").value, 10);
    const newBaudRate = parseInt($("newBaudRate").value, 10);

    appendLog(log, t("logSendingBaud", { time: nowTime(), slaveId, newBaud: newBaudRate }));
    const data = await apiPost("/api/set-baudrate", { connection, slaveId, newBaudRate });

    appendLog(log, t("logSent", { hex: data.request }));
    if (!data.response) {
      appendLog(log, t("logNoResponse"));
      return;
    }
    appendLog(log, t("logReceived", { hex: data.response }));
    if (data.success) {
      appendLog(log, t("logBaudSuccess", { newBaud: newBaudRate }));
      appendLog(log, t("logPowerCycleHint"));
    } else {
      appendLog(log, t("logEchoMismatchBaud"));
    }
  } catch (e) {
    appendLog(log, t("logErrorPrefix", { msg: e.message }));
  }
}

// ===================== Coils (relays) =====================

let coilStates = [];   // null = unknown, true = ON, false = OFF
let coilsBusy = false;

function coilsParams() {
  return {
    connection: getConnection({ baud: parseInt($("coilsBaud").value, 10), timeoutMs: 300 }),
    slaveId: parseInt($("coilsAddr").value, 10),
    start: parseInt($("coilsStart").value, 10) || 0,
    count: Math.min(64, Math.max(1, parseInt($("coilsCount").value, 10) || 1)),
  };
}

function setCoilsBusy(busy) {
  coilsBusy = busy;
  ["btnReadCoils", "btnAllOn", "btnAllOff"].forEach((id) => ($(id).disabled = busy));
  $("coilGrid").querySelectorAll("button").forEach((b) => (b.disabled = busy));
}

function renderCoilGrid() {
  const grid = $("coilGrid");
  const start = parseInt($("coilsStart").value, 10) || 0;
  const count = Math.min(64, Math.max(1, parseInt($("coilsCount").value, 10) || 1));
  if (coilStates.length !== count) coilStates = Array(count).fill(null);

  grid.innerHTML = "";
  coilStates.forEach((state, i) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "coil " + (state === null ? "unknown" : state ? "on" : "off");
    const idx = document.createElement("span");
    idx.className = "coil-idx";
    idx.textContent = start + i;
    const st = document.createElement("span");
    st.className = "coil-state";
    st.textContent = state === null ? t("coilUnknown") : state ? t("coilOn") : t("coilOff");
    btn.appendChild(idx);
    btn.appendChild(st);
    btn.disabled = coilsBusy;
    btn.addEventListener("click", () => toggleCoil(i));
    grid.appendChild(btn);
  });
}

async function readCoils(quiet) {
  const p = coilsParams();
  const data = await apiPost("/api/coils/read", {
    connection: p.connection, slaveId: p.slaveId, start: p.start, quantity: p.count,
  });
  coilStates = data.coils.map(Boolean);
  renderCoilGrid();
  if (!quiet) setStatus($("coilsStatus"), t("coilsReadOk", { time: nowTime() }), "ok");
}

async function doReadCoils() {
  if (coilsBusy) return;
  setCoilsBusy(true);
  try {
    await readCoils(false);
  } catch (e) {
    setStatus($("coilsStatus"), t("errorPrefix") + e.message, "error");
  } finally {
    setCoilsBusy(false);
  }
}

async function toggleCoil(i) {
  if (coilsBusy) return;
  const log = $("coilsLog");
  setCoilsBusy(true);
  try {
    const p = coilsParams();
    const coil = p.start + i;
    const on = !(coilStates[i] === true);
    appendLog(log, t("logCoilWrite", { time: nowTime(), coil, state: on ? t("coilOn") : t("coilOff") }));
    const data = await apiPost("/api/coils/write", { connection: p.connection, slaveId: p.slaveId, coil, on });
    appendLog(log, t("logSent", { hex: data.request }));
    if (!data.response) {
      appendLog(log, t("logNoResponse"));
      setStatus($("coilsStatus"), t("logNoResponse").trim(), "error");
      return;
    }
    appendLog(log, t("logReceived", { hex: data.response }));
    appendLog(log, data.success ? t("logAck") : t("logAckMismatch"));
    coilStates[i] = on;
    renderCoilGrid();
    await readCoils(true);
    setStatus($("coilsStatus"), t("coilsReadOk", { time: nowTime() }), data.success ? "ok" : "error");
  } catch (e) {
    appendLog(log, t("logErrorPrefix", { msg: e.message }));
    setStatus($("coilsStatus"), t("errorPrefix") + e.message, "error");
  } finally {
    setCoilsBusy(false);
  }
}

async function setAllCoils(on) {
  if (coilsBusy) return;
  const log = $("coilsLog");
  setCoilsBusy(true);
  try {
    const p = coilsParams();
    const values = Array(p.count).fill(on);
    appendLog(log, t("logCoilsWriteAll", {
      time: nowTime(), first: p.start, last: p.start + p.count - 1, state: on ? t("coilOn") : t("coilOff"),
    }));
    const data = await apiPost("/api/coils/write-multiple", {
      connection: p.connection, slaveId: p.slaveId, start: p.start, values,
    });
    appendLog(log, t("logSent", { hex: data.request }));
    if (!data.response) {
      appendLog(log, t("logNoResponse"));
      setStatus($("coilsStatus"), t("logNoResponse").trim(), "error");
      return;
    }
    appendLog(log, t("logReceived", { hex: data.response }));
    appendLog(log, data.success ? t("logAck") : t("logAckMismatch"));
    coilStates = values.slice();
    renderCoilGrid();
    await readCoils(true);
    setStatus($("coilsStatus"), t("coilsReadOk", { time: nowTime() }), data.success ? "ok" : "error");
  } catch (e) {
    appendLog(log, t("logErrorPrefix", { msg: e.message }));
    setStatus($("coilsStatus"), t("errorPrefix") + e.message, "error");
  } finally {
    setCoilsBusy(false);
  }
}

// ===================== Discover on network =====================

let discoverPollTimer = null;
let currentDiscoverJobId = null;

async function initDiscoverSubnet() {
  try {
    const res = await fetch("/api/network-discovery/default-subnet");
    const data = await res.json();
    if (data.subnet) $("discoverSubnet").value = data.subnet;
  } catch (e) {
    console.error("Failed to determine the default subnet:", e);
  }
}

function setDiscoverButtons(running) {
  $("btnDiscover").disabled = running;
  $("btnCancelDiscover").disabled = !running;
}

function useDiscoveredDevice(f) {
  document.querySelector('input[name="transport"][value="tcp"]').checked = true;
  onTransportChange();
  $("ipAddress").value = f.ip;
  if (f.port) $("tcpPort").value = f.port;
}

function renderDiscoverResults(found) {
  const tbody = $("discoverResults");
  tbody.innerHTML = "";
  for (const f of found) {
    const tr = document.createElement("tr");
    const ipCell = document.createElement("td");
    const methodCell = document.createElement("td");
    const detailsCell = document.createElement("td");
    const actionCell = document.createElement("td");

    ipCell.textContent = f.ip;
    methodCell.textContent = f.method === "hf-broadcast" ? t("methodHfBroadcast") : t("methodPortScan");
    detailsCell.textContent = f.method === "hf-broadcast"
      ? [f.model, f.mac].filter(Boolean).join(" / ") || f.raw || t("dash")
      : `${t("portWord")} ${f.port}`;

    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = t("btnUse");
    btn.title = t("btnUseTitle");
    btn.addEventListener("click", () => useDiscoveredDevice(f));
    actionCell.appendChild(btn);

    tr.appendChild(ipCell);
    tr.appendChild(methodCell);
    tr.appendChild(detailsCell);
    tr.appendChild(actionCell);
    tbody.appendChild(tr);
  }
}

async function startDiscover() {
  $("discoverResults").innerHTML = "";
  $("discoverProgressBar").style.width = "0%";
  setStatus($("discoverStatus"), t("statusStarting"));

  try {
    const body = {
      subnet: $("discoverSubnet").value.trim(),
      ports: $("discoverPorts").value
        .split(",")
        .map((p) => parseInt(p.trim(), 10))
        .filter((p) => !isNaN(p)),
      timeoutMs: parseInt($("discoverTimeout").value, 10),
      hfBroadcast: $("discoverHf").checked,
    };

    setDiscoverButtons(true);
    const data = await apiPost("/api/network-discovery", body);
    currentDiscoverJobId = data.jobId;
    pollDiscover();
  } catch (e) {
    setStatus($("discoverStatus"), t("errorPrefix") + e.message, "error");
    setDiscoverButtons(false);
  }
}

function pollDiscover() {
  if (discoverPollTimer) clearInterval(discoverPollTimer);
  discoverPollTimer = setInterval(async () => {
    if (!currentDiscoverJobId) return;
    try {
      const res = await fetch(`/api/network-discovery/${currentDiscoverJobId}`);
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || t("discoverJobGone"));

      const p = data.progress || { done: 0, total: 1, status: "" };
      const pct = p.total ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
      $("discoverProgressBar").style.width = pct + "%";
      renderDiscoverResults(data.found || []);

      if (data.done) {
        const found = (data.found || []).length;
        if (data.error) {
          setStatus($("discoverStatus"), t("errorPrefix") + data.error, "error");
        } else {
          setStatus($("discoverStatus"), t("discoverDoneFound", { n: found }), found > 0 ? "ok" : null);
        }
        clearInterval(discoverPollTimer);
        discoverPollTimer = null;
        currentDiscoverJobId = null;
        setDiscoverButtons(false);
      } else {
        setStatus($("discoverStatus"), p.status || t("statusSearching"));
      }
    } catch (e) {
      setStatus($("discoverStatus"), t("errorPrefix") + e.message, "error");
      clearInterval(discoverPollTimer);
      discoverPollTimer = null;
      currentDiscoverJobId = null;
      setDiscoverButtons(false);
    }
  }, 300);
}

async function cancelDiscover() {
  if (!currentDiscoverJobId) return;
  try {
    await fetch(`/api/network-discovery/${currentDiscoverJobId}/cancel`, { method: "POST" });
  } catch (e) {
    console.error("Failed to cancel search:", e);
  }
}

// ===================== Start =====================

document.addEventListener("DOMContentLoaded", () => {
  initTheme();
  $("themeToggle").addEventListener("click", toggleTheme);

  $("langSelect").value = currentLang;
  applyTranslations();
  $("langSelect").addEventListener("change", (e) => setLang(e.target.value));

  initTabs();
  initBaudList();
  refreshPorts();
  onTransportChange();

  document.querySelectorAll('input[name="transport"]').forEach((r) => r.addEventListener("change", onTransportChange));
  $("btnRefreshPorts").addEventListener("click", refreshPorts);

  $("btnPresetR4").addEventListener("click", () => {
    Array.from($("baudList").querySelectorAll("input")).forEach((cb, i) => (cb.checked = i < 5));
  });
  $("btnPresetAll").addEventListener("click", () => {
    Array.from($("baudList").querySelectorAll("input")).forEach((cb) => (cb.checked = true));
  });

  $("btnScan").addEventListener("click", startScan);
  $("btnCancelScan").addEventListener("click", cancelScan);

  $("btnReadOnce").addEventListener("click", doTempRead);
  $("tempLoop").addEventListener("change", onTempLoopChange);

  $("btnSetAddr").addEventListener("click", doSetAddress);
  $("btnSetBaud").addEventListener("click", doSetBaudRate);

  renderCoilGrid();
  $("btnReadCoils").addEventListener("click", doReadCoils);
  $("btnAllOn").addEventListener("click", () => setAllCoils(true));
  $("btnAllOff").addEventListener("click", () => setAllCoils(false));
  ["coilsStart", "coilsCount"].forEach((id) => $(id).addEventListener("change", () => { coilStates = []; renderCoilGrid(); }));
  $("langSelect").addEventListener("change", () => renderCoilGrid());

  initDiscoverSubnet();
  $("btnDiscover").addEventListener("click", startDiscover);
  $("btnCancelDiscover").addEventListener("click", cancelDiscover);
});
