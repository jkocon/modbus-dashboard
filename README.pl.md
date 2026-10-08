# Modbus RTU Dashboard

Wieloplatformowe narzędzie desktopowe do wyszukiwania, odczytu i konfiguracji urządzeń
**Modbus RTU** po **RS485 (port szeregowy)** albo **przez sieć** (konwertery
RS485↔Ethernet/WiFi lub prawdziwy Modbus TCP). Działa jako natywne okno (jak ESPHome Device
Builder) na Windows, Linux i macOS jako jeden plik wykonywalny napisany w Rust (wersja 2.0;
wcześniejsze wersje były w Pythonie).

Zestaw samodzielnych skryptów **PowerShell** (Windows) daje te same operacje z linii poleceń.

> **English documentation:** [README.md](README.md)

---

## Funkcje

| Zakładka | Co robi |
|---|---|
| **Wykryj w sieci** | Znajduje konwertery RS485↔Ethernet w LAN — jak *Search* w VirCOM. Zweryfikowany broadcast UDP HF/Elfin + skan portów TCP po podsieci. Jedno kliknięcie wpisuje IP urządzenia do panelu połączenia. |
| **Skanowanie** | Przechodzi baudrate × adresy i wypisuje każde urządzenie, które odpowiada poprawnym CRC. Dowolny kod funkcji / rejestr / ilość. Tryb *DiscoverAddress* (broadcast Waveshare). |
| **Temperatura (R4DCB08)** | Odczyt 8 kanałów DS18B20 modułu R4DCB08, jednorazowo albo ciągle. |
| **Cewki (przekaźniki)** | Siatka cewek dla płytek przekaźnikowych: kliknięcie przełącza jedną (0x05), *Wszystkie ON* / *Wszystkie OFF* dla całego zakresu (0x0F), stany odczytywane z powrotem z urządzenia (0x01). Cewka startowa i liczba cewek do wyboru. |
| **Zmiana adresu** | Przepisuje adres Modbus urządzenia (rejestr 254, funkcja 0x06) i weryfikuje echo. |
| **Zmiana baudrate** | Przepisuje baudrate R4DCB08 (rejestr 255) i weryfikuje echo. |

Wspólne dla wszystkich zakładek:

- **Port szeregowy** (COM/RS485): port, baudrate, bity danych 5–8, parzystość None/Even/Odd, bity stopu 1/2, kontrola przepływu None/RTS-CTS/XON-XOFF.
- **Sieć**: przezroczyste konwertery RTU-over-TCP (Elfin EW11, USR-TCP232, Waveshare RS485↔ETH…)
  **albo** natywny Modbus TCP (nagłówek MBAP, bez CRC) — jeden checkbox przełącza ramkowanie.
- Domyślnie angielski, dostępny polski; jasny/ciemny motyw z ręcznym przełącznikiem. Wybór
  języka i motywu jest zapamiętywany w `~/.config/modbus-dashboard/prefs.json`.
- Czytelne, konkretne błędy — np. gdy sterownik adaptera USB-RS485 odrzuca niewspieraną
  kombinację bitów danych/stopu, dostajesz dokładnie tę informację, a nie `The parameter is incorrect`.

---

## Szybki start

### Opcja A — gotowy plik wykonywalny (Linux x86_64)

Pobierz `modbus-dashboard-<wersja>-linux-x86_64.tar.gz` ze strony
[Releases](https://github.com/jkocon/modbus-dashboard/releases), rozpakuj i uruchom
`./modbus-dashboard`. Otwiera własne okno; nic się nie instaluje, a serwer w środku nasłuchuje
tylko na losowym porcie `127.0.0.1`. Pliki interfejsu są wbudowane w binarkę. Wymaga glibc 2.39+
(Arch/CachyOS, Ubuntu 24.04+, Debian 13, Fedora 40+) oraz WebKitGTK 4.1 i GTK 3.

Gotowych binarek na Windows ani macOS nie ma – zbuduj ją sam, to kilka minut (patrz
*Budowanie na Windows* niżej).

### Opcja B — ze źródeł

Wymaga **Rust 1.85+** (`cargo`).

```bash
cargo run --release                 # natywne okno (zalecane)
cargo run --release -- --serve      # serwer HTTP + przeglądarka, http://127.0.0.1:6070/
```

`--serve` przyjmuje też `--address 0.0.0.0` (LAN, patrz *Model bezpieczeństwa*), `--port N`
i `--no-browser`.

Uwagi do natywnego okna (`wry`):

- **Windows** — wbudowany WebView2 (jest razem z Edge na Windows 10/11).
- **macOS** — WKWebView.
- **Linux** — do budowania i działania potrzebne WebKitGTK i GTK 3: `webkit2gtk-4.1 gtk3`
  (Arch), `libwebkit2gtk-4.1-dev libgtk-3-dev` (Debian/Ubuntu). Dodaj się też do grupy portów
  szeregowych (`uucp` na Arch, `dialout` na Debian/Ubuntu) i zaloguj ponownie.

### Budowanie samodzielnego pliku wykonywalnego

```bash
cargo build --release
```

Wynik: `target/release/modbus-dashboard` (`.exe` na Windows). Budujesz **na każdym docelowym
systemie osobno**. Na CachyOS `install.sh` buduje binarkę i instaluje ją w `/opt/modbus-dashboard`
ze skrótem w menu.

### Budowanie na Windows

1. Zainstaluj Visual Studio **Build Tools** z pakietem *Desktop development with C++* (linker MSVC
   i Windows SDK).
2. Zainstaluj Rust przez [rustup](https://rustup.rs) (domyślny toolchain `x86_64-pc-windows-msvc`).
3. W katalogu repozytorium uruchom:

   ```powershell
   cargo build --release
   ```

Wynikiem jest jeden plik `target\release\modbus-dashboard.exe` – skopiuj go gdziekolwiek
i uruchom. Korzysta z WebView2, który jest w Windows 10/11, nie potrzebuje innych plików,
a wersja release nie otwiera okna konsoli (`--serve` dalej uruchamia tryb przeglądarki, tylko nic nie wypisuje). Porty
szeregowe widać jako `COM3`, `COM4`, … Wersja na Windows nie jest testowana przez autora przy
każdym wydaniu.

---

## Obsługa

### 1. Panel połączenia (góra okna)

Wybierz **Port szeregowy** albo **Sieć (IP)**.

**Port szeregowy.** Domyślnie **8N1 bez kontroli przepływu** — pasuje do większości tanich
modułów RS485 (przekaźniki Waveshare, R4DCB08). Specyfikacja Modbus RTU domyślnie zakłada
**8E1** (parzystość Even) — jeśli urządzenie milczy na 8N1, spróbuj `Even`. Kontrola
przepływu w Modbus RTU praktycznie nie występuje; zostaw `None`, chyba że instrukcja
urządzenia mówi inaczej.

Nie każdy adapter USB-RS485 wspiera każdą kombinację. Wiele adapterów klasy CH340 obsługuje
tylko 7–8 bitów danych i 1/2 bity stopu; 5/6 bitów albo 1.5 bitu stopu sterownik odrzuca na
poziomie systemu. Aplikacja rozpoznaje ten konkretny błąd (`ERROR_INVALID_PARAMETER`) i
podpowiada powrót do 8N1 zamiast pokazywać surowy wyjątek. Parzystości Mark/Space, 1,5 bitu
stopu i kontroli DSR/DTR nie ma (biblioteka portów ich nie obsługuje, Modbus RTU ich nie używa).

Odpowiedź jest czytana do końca ramki (długość wynika z kodu funkcji albo nagłówka MBAP) albo
do ciszy na linii przez 3,5 znaku (co najmniej 20 ms, z zapasem na opóźnienie adapterów USB),
więc działają też wolne prędkości, np. 1200.

**Sieć.** Podaj IP konwertera i port TCP. Port zależy od konfiguracji konwertera — 502, 8899,
4196 i 23 są typowe. Są dwa ramkowania:

| Ramkowanie | Kiedy | Checkbox |
|---|---|---|
| **RTU over TCP** | Konwerter w trybie *przezroczystym*: surowe bajty RTU (z CRC) są tunelowane. Baudrate/parzystość ustawiasz **w konwerterze**, nie tutaj. | wyłączony (domyślnie) |
| **Modbus TCP (MBAP)** | Konwerter/bramka/PLC mówi natywnym Modbus TCP: 7-bajtowy nagłówek MBAP, bez CRC. Port 502 zwykle oznacza właśnie to. | **włączony** |

Jeśli skan po IP na porcie 502 nic nie znajduje, zaznacz MBAP i spróbuj ponownie.

### 2. Wykryj w sieci

Podsieć (CIDR) uzupełnia się z lokalnego IP; w razie potrzeby popraw. Dwie metody na raz:

1. **Broadcast HF/Elfin** — wysyła `HF-A11ASSISTHREAD` na UDP 48899. Moduły na bazie
   Hi-Flying HF-LPB100 (Elfin EW10/EW11/EW12 i wiele klonów) odpowiadają `IP, MAC, model`.
   Protokół publiczny, zweryfikowany w kilku niezależnych implementacjach.
2. **Skan portów TCP** — próbuje 502/8899/4196/23 (edytowalne) na każdym hoście podsieci,
   z jednym automatycznym ponowieniem na port, żeby nie przegapić urządzenia przy pierwszym,
   „zimnym" połączeniu. Nie rozpozna marki, ale znajdzie **każdy** konwerter z otwartym
   jednym z tych portów — także te z własnościowym protokołem wykrywania (USR-IOT/VirCOM).

Limit: 1024 adresy na skan (`/22`); węższa podsieć = szybszy skan.

### 3. Skanowanie

Wybierz baudrate'y do sprawdzenia (tylko COM — po IP baudrate ustala konwerter, więc
przebieg jest jeden), zakres adresów, kod funkcji, rejestr startowy i ilość. Dla R4DCB08:
`0x03` / rejestr 0 / ilość 8; dla przekaźników Waveshare: `0x01` (Read Coils).
*DiscoverAddress* wysyła jeden broadcast do rejestru `0x4000`, na który część płytek
Waveshare odpowiada swoim prawdziwym adresem.

### 4. Cewki (przekaźniki)

Ustaw adres urządzenia, pierwszą cewkę i liczbę cewek płytki (8/16/32 dla przekaźników
Waveshare), potem **Odczytaj stany**. Każda cewka to przycisk: kliknięcie przełącza dany
przekaźnik przez *Write Single Coil* (0x05). **Wszystkie ON** / **Wszystkie OFF** zapisują
cały zakres jednym żądaniem *Write Multiple Coils* (0x0F). Po każdym zapisie stany są
odczytywane z urządzenia, więc siatka zawsze pokazuje to, co raportuje sprzęt, a nie to, o
co poproszono. Wyjątek Modbus (np. niedozwolony adres, gdy liczba cewek przekracza liczbę
na płytce) jest pokazywany z kodem.

### 5–6. Zmiana adresu / baudrate

Obie operacje wysyłają *Write Single Register* i uznają dokładne echo za sukces. **Kolizja
adresów:** jeśli dwa urządzenia mają ten sam adres, zapis zmieni oba. Podłączaj urządzenia
pojedynczo, żeby nadać im unikalne adresy. Po zmianie baudrate część modułów wymaga
wyłączenia i włączenia zasilania; po IP zaktualizuj też baudrate w konfiguracji konwertera.

---

## Skrypty PowerShell (Windows)

Ten sam silnik, bez GUI. Wszystkie skrypty dot-source'ują `ModbusCommon.ps1`, który musi
leżeć obok nich. Każdy skrypt przyjmuje **albo** `-ComPort COM3` **albo**
`-IpAddress 192.168.1.50 [-TcpPort 502]`, plus ustawienia portu `-BaudRate`, `-DataBits 5..8`,
`-Parity None|Even|Odd|Mark|Space`, `-StopBits One|OnePointFive|Two`,
`-Handshake None|XOnXOff|RequestToSend|RequestToSendXOnXOff` i `-TimeoutMs`.

```powershell
# Znajdź urządzenia (profil R4DCB08, port szeregowy)
.\Scan-Modbus.ps1 -ComPort COM3 -BaudRates 1200,2400,4800,9600,19200 -FunctionCode 3 -Register 0 -Quantity 8

# To samo przez bramkę Modbus TCP
.\Scan-Modbus.ps1 -IpAddress 10.10.0.201 -TcpPort 502 -Mbap -FunctionCode 3 -Quantity 8 -StartAddress 1 -EndAddress 20

# Odczyt temperatur co 5 s
.\Read-Temperature.ps1 -ComPort COM3 -SlaveId 3 -Loop -IntervalSeconds 5

# Zmiana adresu i baudrate
.\Set-ModbusAddress.ps1  -ComPort COM3 -OldAddress 3 -NewAddress 2
.\Set-ModbusBaudRate.ps1 -ComPort COM3 -CurrentBaudRate 4800 -SlaveId 2 -NewBaudRate 9600
```

`ModbusGUI.ps1` to nakładka WinForms na te skrypty (tylko Windows, interfejs po polsku).
Powstała przed dashboardem i zostaje dla osób chcących GUI bez żadnych zależności;
rozwijanym interfejsem jest dashboard.

---

## Architektura

```
src/
  main.rs        punkt wejścia: natywne okno (wry/tao) z serwerem na losowym porcie loopback
                 albo --serve do trybu headless/LAN z przeglądarką
  server.rs      serwer HTTP + API JSON (tiny_http), kontrola Host/Origin/Content-Type;
                 interfejs jest wkompilowany w binarkę
  core.rs        protokół: CRC16, ramki RTU i MBAP, wykrywanie długości ramki, walidacja
                 danych, dekodowanie R4DCB08, cewki, Write Single Register z weryfikacją echa
  transport.rs   transport szeregowy (crate serialport) i TCP
  discovery.rs   broadcast UDP HF/Elfin + równoległy skan portów TCP
  jobs.rs        zadania w tle (skan, wykrywanie), które UI odpytuje
static/          interfejs jednostronicowy (czysty HTML/CSS/JS, bez bundlera)
  i18n.js        słowniki EN/PL; domyślnie angielski
ModbusCommon.ps1 …  port tej samej warstwy protokołu na PowerShell + skrypty CLI
```

Bez frameworka webowego i bez bundlera JS: zależności to `serialport` (porty szeregowe),
`tiny_http` (serwer), `serde_json`, `clap` i `wry`/`tao` (natywne okno). Frontend rozmawia z backendem przez małe
API JSON; długie operacje (skan, wykrywanie) działają jako zadania w tle, które UI odpytuje.
Pełna referencja API: sekcja *JSON API* w [README.md](README.md#json-api).

---

## Model bezpieczeństwa (skrót)

To narzędzie **jednego użytkownika, lokalne**. Serwer domyślnie nasłuchuje na `127.0.0.1`,
aplikacja desktopowa używa losowego portu. API sprawdza nagłówki `Host` i `Origin` (blokuje
DNS rebinding i żądania cross-site), wymaga `application/json` na `POST`, ogranicza rozmiar
żądania i liczbę równoległych zadań, a serwuje tylko pliki interfejsu wbudowane w binarkę.

**Nie ma uwierzytelniania.** Po uruchomieniu `modbus-dashboard --serve --address 0.0.0.0` każdy,
kto dosięgnie tego portu, może przekonfigurować Twoje urządzenia Modbus i uruchamiać skany
portów z Twojej maszyny. Rób to tylko w zaufanej sieci. Szczegóły i zgłaszanie:
[SECURITY.md](SECURITY.md).

---

## Testy

```bash
cargo test
```

Testy protokołu używają gotowych ramek (bez sprzętu); para pseudoterminali symuluje urządzenie
na 1200 baud. Testy API startują prawdziwy serwer na porcie efemerycznym i sprawdzają
zabezpieczenia oraz walidację danych (adresy 1–247, `oldAddress` także 0 = broadcast, nigdy
obcinane do bajtu).

---

## Sprzęt, na którym to powstało

- **R4DCB08** — 8-kanałowy moduł DS18B20 RS485 (temperatura, rejestry adresu/baudrate 254/255)
- Płytki **Waveshare Modbus RTU Relay** (Read Coils, `0x4000` DiscoverAddress)
- Adapter USB-RS485 na CH340 (tylko 7–8 bitów danych, 1/2 bity stopu)
- Bramka Modbus TCP na porcie 502 (ramkowanie MBAP)
- Konwertery RS485↔Ethernet typu Elfin EW11 / USR-TCP232 (tryb przezroczysty)

Własnościowy protokół wykrywania USR-IOT/VirCOM **nie** jest zaimplementowany (jest
nieudokumentowany). Jeśli skan portów nie znajduje Twojego urządzenia USR-IOT, przechwyć
Wiresharkiem ruch z *Search* w VirCOM i otwórz issue z bajtami żądania/odpowiedzi.

---

## Licencja

MIT — patrz [LICENSE](LICENSE).
