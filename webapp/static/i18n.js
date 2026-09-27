"use strict";

// Default language: English. The user can switch to Polish; the choice is
// remembered in localStorage. Translations cover the whole static UI and the
// messages generated in JS (statuses, logs). Errors returned by the backend
// (Python) are always English regardless of the selected language - a deliberate
// trade-off to avoid duplicating every error message in two languages server-side.

const TRANSLATIONS = {
  en: {
    appTitle: "Modbus RTU Dashboard",
    subtitle: "Scanning, reading, and configuring Modbus RTU devices — COM/RS485 and network",

    transportSerial: "Serial port",
    transportNetwork: "Network (IP)",
    comPortLabel: "COM port:",
    btnRefreshPorts: "Refresh ports",
    dataBitsLabel: "Data bits:",
    parityLabel: "Parity:",
    stopBitsLabel: "Stop bits:",
    flowControlLabel: "Flow control:",
    ipAddressLabel: "IP address:",
    ipAddressPlaceholder: "e.g. 192.168.1.50",
    tcpPortLabel: "TCP port:",
    tcpHint: "e.g. Elfin EW11 / USR-TCP232 in transparent mode (port is often 502/8899/4196/23)",
    mbapLabel: "Real Modbus TCP (MBAP, no CRC) instead of RTU tunneled over TCP — try this if scanning finds nothing (typically port 502)",

    tabDiscover: "Discover on network",
    tabScan: "Scan",
    tabTemp: "Temperature (R4DCB08)",
    tabAddr: "Change address",
    tabBaud: "Change baud rate",

    discoverSubnetLabel: "Subnet (CIDR):",
    discoverSubnetPlaceholder: "e.g. 192.168.1.0/24",
    discoverPortsLabel: "Ports to scan:",
    discoverTimeoutLabel: "Timeout (ms):",
    discoverHfLabel: "HF/Elfin broadcast (port 48899)",
    discoverHint: "Two independent methods at once: a verified UDP broadcast for Hi-Flying/Elfin modules (identifies IP, MAC, and model) and a TCP port scan across the whole subnet (finds any converter with one of the given ports open, regardless of brand — the only way to detect devices with a closed protocol, e.g. USR-IOT/VirCOM).",
    btnDiscover: "Search",
    btnCancelDiscover: "Cancel",
    discoverTableIp: "IP address",
    discoverTableMethod: "Method",
    discoverTableDetails: "Details",
    btnUse: "Use",
    btnUseTitle: "Fill in the IP address in the connection panel and switch to Network (IP) mode",
    methodHfBroadcast: "HF/Elfin (broadcast)",
    methodPortScan: "Open port",
    portWord: "port",

    scanBaudLabel: "Baud rate (COM only):",
    btnPresetR4: "R4DCB08 only",
    btnPresetAll: "Select all",
    startAddrLabel: "Start address:",
    endAddrLabel: "End address:",
    funcCodeLabel: "Modbus function:",
    registerLabel: "Start register:",
    quantityLabel: "Quantity:",
    timeoutMsLabel: "Timeout (ms):",
    discoverAddressLabel: "DiscoverAddress mode (broadcast, register 0x4000 — some Waveshare devices)",
    btnScan: "Scan",
    btnCancelScan: "Cancel",
    scanTableBaud: "Baud rate",
    scanTableAddr: "Address",
    scanTableResp: "Response (hex)",
    networkWord: "network",

    tempBaudLabel: "Baud rate:",
    tempAddrLabel: "Address:",
    btnReadOnce: "Read once",
    tempLoopLabel: "Continuous mode, every (s):",
    tempTableCh: "Channel",
    tempTableTemp: "Temperature °C",
    tempTableStatus: "Status",
    statusOk: "OK",
    statusNoSensor: "No sensor",

    addrBaudLabel: "Current baud rate:",
    oldAddrLabel: "Current address:",
    newAddrLabel: "New address:",
    btnSetAddr: "Change address",
    addrWarn: "Warning: with an address collision (two devices sharing the same address), the change will affect BOTH at once.",

    baudCurLabel: "Current baud rate:",
    baudAddrLabel: "Device address:",
    newBaudLabel: "New baud rate:",
    btnSetBaud: "Change baud rate",
    baudWarn: "After the change, the device may need a power cycle to respond on the new baud rate.",

    tabCoils: "Coils (relays)",
    coilsBaudLabel: "Baud rate:",
    coilsStartLabel: "Start coil:",
    coilsCountLabel: "Number of coils:",
    btnReadCoils: "Read states",
    btnAllOn: "All ON",
    btnAllOff: "All OFF",
    coilsHint: "Click a coil to toggle it (Write Single Coil, 0x05). \"All ON\" / \"All OFF\" write every coil in the range at once (Write Multiple Coils, 0x0F). States are re-read from the device after each write. Waveshare relay boards use coils 0..7 / 0..15 / 0..31.",
    coilOn: "ON",
    coilOff: "OFF",
    coilUnknown: "?",
    coilsReadOk: "States read - {time}",
    logCoilWrite: "[{time}] Coil {coil} -> {state}",
    logCoilsWriteAll: "[{time}] Coils {first}..{last} -> {state}",
    logAck: "  OK: acknowledged by the device.",
    logAckMismatch: "  Response is not the expected acknowledgement - re-read the states to verify.",

    statusReady: "Ready.",
    statusStarting: "Starting...",
    statusScanning: "Scanning...",
    statusSearching: "Searching...",
    errorPrefix: "Error: ",
    scanDoneFound: "Scan complete. Found: {n}",
    discoverDoneFound: "Search complete. Found: {n}",
    readOk: "Read OK - {time}",
    selectComPort: "Select a COM port.",
    enterIp: "Enter the converter's IP address.",
    selectBaud: "Select at least one baud rate.",
    scanJobGone: "The scan job disappeared.",
    discoverJobGone: "The search job disappeared.",
    dash: "-",

    logSendingAddr: "[{time}] Sending address change {old} -> {new}...",
    logSent: "  Sent: {hex}",
    logNoResponse: "  No response. Check address/baud rate/connection.",
    logReceived: "  Received: {hex}",
    logAddrSuccess: "  SUCCESS: address changed from {old} to {new}.",
    logEchoMismatch: "  Response is not the expected echo — verify by scanning again.",
    logErrorPrefix: "  Error: {msg}",
    logSendingBaud: "[{time}] Sending baud rate change for device {slaveId} -> {newBaud}...",
    logBaudSuccess: "  SUCCESS: baud rate changed to {newBaud}.",
    logPowerCycleHint: "  If the device doesn't respond on the new baud rate, power-cycle it.",
    logEchoMismatchBaud: "  Response is not the expected echo — verify by scanning again on the new baud rate.",
  },

  pl: {
    appTitle: "Modbus RTU Dashboard",
    subtitle: "Skanowanie, odczyt i konfiguracja urządzeń Modbus RTU — COM/RS485 i sieć",

    transportSerial: "Port COM",
    transportNetwork: "Sieć (IP)",
    comPortLabel: "Port COM:",
    btnRefreshPorts: "Odśwież porty",
    dataBitsLabel: "Bity:",
    parityLabel: "Parzystość:",
    stopBitsLabel: "Bity stopu:",
    flowControlLabel: "Kontrola przepływu:",
    ipAddressLabel: "Adres IP:",
    ipAddressPlaceholder: "np. 192.168.1.50",
    tcpPortLabel: "Port TCP:",
    tcpHint: "np. Elfin EW11 / USR-TCP232 w trybie przezroczystym (port bywa 502/8899/4196/23)",
    mbapLabel: "Prawdziwy Modbus TCP (MBAP, bez CRC) zamiast RTU tunelowanego przez TCP — spróbuj, jeśli skan nic nie znajduje (typowo port 502)",

    tabDiscover: "Wykryj w sieci",
    tabScan: "Skanowanie",
    tabTemp: "Temperatura (R4DCB08)",
    tabAddr: "Zmiana adresu",
    tabBaud: "Zmiana baudrate",

    discoverSubnetLabel: "Podsieć (CIDR):",
    discoverSubnetPlaceholder: "np. 192.168.1.0/24",
    discoverPortsLabel: "Porty do skanu:",
    discoverTimeoutLabel: "Timeout (ms):",
    discoverHfLabel: "Broadcast HF/Elfin (port 48899)",
    discoverHint: "Dwie niezależne metody na raz: zweryfikowany broadcast UDP dla modułów Hi-Flying/Elfin (identyfikuje IP, MAC i model) oraz skan portów TCP po całej podsieci (znajdzie każdy konwerter z otwartym jednym z podanych portów, niezależnie od marki — to jedyny sposób na wykrycie urządzeń z zamkniętym protokołem, np. USR-IOT/VirCOM).",
    btnDiscover: "Szukaj",
    btnCancelDiscover: "Anuluj",
    discoverTableIp: "Adres IP",
    discoverTableMethod: "Metoda",
    discoverTableDetails: "Szczegóły",
    btnUse: "Użyj",
    btnUseTitle: "Wypełnij adres IP w panelu połączenia i przełącz na tryb Sieć (IP)",
    methodHfBroadcast: "HF/Elfin (broadcast)",
    methodPortScan: "Otwarty port",
    portWord: "port",

    scanBaudLabel: "Baudrate (tylko COM):",
    btnPresetR4: "Tylko R4DCB08",
    btnPresetAll: "Zaznacz wszystkie",
    startAddrLabel: "Adres od:",
    endAddrLabel: "Adres do:",
    funcCodeLabel: "Funkcja Modbus:",
    registerLabel: "Rejestr startowy:",
    quantityLabel: "Ilość:",
    timeoutMsLabel: "Timeout (ms):",
    discoverAddressLabel: "Tryb DiscoverAddress (broadcast, rejestr 0x4000 — część urządzeń Waveshare)",
    btnScan: "Skanuj",
    btnCancelScan: "Anuluj",
    scanTableBaud: "Baudrate",
    scanTableAddr: "Adres",
    scanTableResp: "Odpowiedź (hex)",
    networkWord: "sieć",

    tempBaudLabel: "Baudrate:",
    tempAddrLabel: "Adres:",
    btnReadOnce: "Odczytaj raz",
    tempLoopLabel: "Tryb ciągły, co (s):",
    tempTableCh: "Kanał",
    tempTableTemp: "Temperatura °C",
    tempTableStatus: "Status",
    statusOk: "OK",
    statusNoSensor: "Brak czujnika",

    addrBaudLabel: "Aktualny baudrate:",
    oldAddrLabel: "Aktualny adres:",
    newAddrLabel: "Nowy adres:",
    btnSetAddr: "Zmień adres",
    addrWarn: "Uwaga: przy kolizji adresów (dwa urządzenia na tym samym adresie) zmiana obejmie OBA naraz.",

    baudCurLabel: "Aktualny baudrate:",
    baudAddrLabel: "Adres urządzenia:",
    newBaudLabel: "Nowy baudrate:",
    btnSetBaud: "Zmień baudrate",
    baudWarn: "Po zmianie urządzenie może wymagać ponownego włączenia zasilania (power cycle).",

    tabCoils: "Cewki (przekaźniki)",
    coilsBaudLabel: "Baudrate:",
    coilsStartLabel: "Cewka startowa:",
    coilsCountLabel: "Liczba cewek:",
    btnReadCoils: "Odczytaj stany",
    btnAllOn: "Wszystkie ON",
    btnAllOff: "Wszystkie OFF",
    coilsHint: "Kliknij cewkę, żeby ją przełączyć (Write Single Coil, 0x05). „Wszystkie ON” / „Wszystkie OFF” zapisują cały zakres naraz (Write Multiple Coils, 0x0F). Po każdym zapisie stany są odczytywane ponownie z urządzenia. Płytki przekaźnikowe Waveshare używają cewek 0..7 / 0..15 / 0..31.",
    coilOn: "ON",
    coilOff: "OFF",
    coilUnknown: "?",
    coilsReadOk: "Stany odczytane - {time}",
    logCoilWrite: "[{time}] Cewka {coil} -> {state}",
    logCoilsWriteAll: "[{time}] Cewki {first}..{last} -> {state}",
    logAck: "  OK: potwierdzone przez urządzenie.",
    logAckMismatch: "  Odpowiedź nie jest oczekiwanym potwierdzeniem — odczytaj stany ponownie, żeby zweryfikować.",

    statusReady: "Gotowy.",
    statusStarting: "Uruchamianie...",
    statusScanning: "Skanowanie...",
    statusSearching: "Szukam...",
    errorPrefix: "Błąd: ",
    scanDoneFound: "Skanowanie zakończone. Znaleziono: {n}",
    discoverDoneFound: "Wyszukiwanie zakończone. Znaleziono: {n}",
    readOk: "Odczyt OK - {time}",
    selectComPort: "Wybierz port COM.",
    enterIp: "Podaj adres IP konwertera.",
    selectBaud: "Zaznacz co najmniej jeden baudrate.",
    scanJobGone: "Zadanie skanowania zniknęło.",
    discoverJobGone: "Zadanie wyszukiwania zniknęło.",
    dash: "-",

    logSendingAddr: "[{time}] Wysyłam zmianę adresu {old} -> {new}...",
    logSent: "  Wysłano: {hex}",
    logNoResponse: "  Brak odpowiedzi. Sprawdź adres/baudrate/połączenie.",
    logReceived: "  Odebrano: {hex}",
    logAddrSuccess: "  SUKCES: adres zmieniony z {old} na {new}.",
    logEchoMismatch: "  Odpowiedź nie jest oczekiwanym echem — zweryfikuj ponownym skanowaniem.",
    logErrorPrefix: "  Błąd: {msg}",
    logSendingBaud: "[{time}] Wysyłam zmianę baudrate urządzenia {slaveId} -> {newBaud}...",
    logBaudSuccess: "  SUKCES: baudrate zmieniony na {newBaud}.",
    logPowerCycleHint: "  Jeśli urządzenie nie odpowiada na nowym baudrate, wyłącz i włącz jego zasilanie.",
    logEchoMismatchBaud: "  Odpowiedź nie jest oczekiwanym echem — zweryfikuj ponownym skanowaniem na nowym baudrate.",
  },
};

let currentLang = localStorage.getItem("lang") || "en";
if (!TRANSLATIONS[currentLang]) currentLang = "en";

function t(key, vars) {
  let text = (TRANSLATIONS[currentLang] && TRANSLATIONS[currentLang][key]) || TRANSLATIONS.en[key] || key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      text = text.replace(new RegExp("\\{" + k + "\\}", "g"), v);
    }
  }
  return text;
}

function setLang(lang) {
  if (!TRANSLATIONS[lang]) return;
  currentLang = lang;
  localStorage.setItem("lang", lang);
  document.documentElement.lang = lang;
  applyTranslations();
}

function applyTranslations() {
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n);
  });
  document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder);
  });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => {
    el.title = t(el.dataset.i18nTitle);
  });
  document.title = t("appTitle");

  // Status fields that haven't been overwritten by a dynamic message yet
  document.querySelectorAll(".status[data-i18n-dynamic='ready']").forEach((el) => {
    el.textContent = t("statusReady");
  });
}
