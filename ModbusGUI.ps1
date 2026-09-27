<#
.SYNOPSIS
    Simple WinForms GUI for Modbus RTU devices over RS485: scanning, temperature
    read-out (R4DCB08), address change, baud rate change. Connects over a COM port
    (RS485) or over the network through an RS485<->Ethernet/WiFi converter
    (RTU over TCP, transparent mode). Windows only; the UI itself is in Polish -
    the cross-platform, English-first UI is the dashboard in webapp/.

.EXAMPLE
    .\ModbusGUI.ps1
#>

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

. "$PSScriptRoot\ModbusCommon.ps1"

function Get-R4DCB08Temperatures {
    param($Connection, [byte]$SlaveId, [int]$TimeoutMs, [bool]$Mbap = $false)

    if ($Mbap) {
        $request = Build-MbapReadRequest -UnitId $SlaveId -FunctionCode 0x03 -Register 0 -Quantity 8
        $resp = Send-ModbusRequest -Connection $Connection -Request $request -TimeoutMs $TimeoutMs
        if (-not (Test-MbapResponse -Response $resp -UnitId $SlaveId)) { throw "Brak lub nieprawidlowa odpowiedz (MBAP)." }
        $pdu = Get-MbapPdu -Response $resp
        if ($pdu[0] -ne 0x03) { throw "Nieoczekiwana odpowiedz (funkcja sie nie zgadza)." }
        $byteCount = $pdu[1]
        $data = $pdu[2..(2 + $byteCount - 1)]
    } else {
        $request = Build-ModbusRequest -SlaveId $SlaveId -FunctionCode 0x03 -Register 0 -Quantity 8
        $resp = Send-ModbusRequest -Connection $Connection -Request $request -TimeoutMs $TimeoutMs

        if (-not $resp -or $resp.Length -lt 21) { throw "Brak lub za krotka odpowiedz." }
        if (-not (Test-ModbusCRC $resp)) { throw "Blad CRC w odpowiedzi." }
        if ($resp[0] -ne $SlaveId -or $resp[1] -ne 0x03) { throw "Nieoczekiwana odpowiedz (adres/funkcja)." }

        $byteCount = $resp[2]
        $data = $resp[3..(3 + $byteCount - 1)]
    }

    $results = @()
    for ($ch = 0; $ch -lt 8; $ch++) {
        $raw = ($data[$ch * 2] -shl 8) -bor $data[$ch * 2 + 1]
        if ($raw -eq 0x8000) {
            $results += [PSCustomObject]@{ Kanal = $ch + 1; TemperaturaC = $null; Status = "Brak czujnika" }
        } else {
            if ($raw -gt 32767) { $raw = $raw - 65536 }
            $results += [PSCustomObject]@{ Kanal = $ch + 1; TemperaturaC = [math]::Round($raw / 10.0, 1); Status = "OK" }
        }
    }
    return $results
}

$baudCodeMap = @{ 1200 = 0; 2400 = 1; 4800 = 2; 9600 = 3; 19200 = 4 }

# ===================== GUI construction =====================

$form = New-Object System.Windows.Forms.Form
$form.Text = "Modbus RTU Toolkit"
$form.Size = New-Object System.Drawing.Size(760, 650)
$form.StartPosition = "CenterScreen"
$form.FormBorderStyle = "FixedSingle"
$form.MaximizeBox = $false
$form.Font = New-Object System.Drawing.Font("Segoe UI", 9)

# --- Connection panel (shared by all tabs) ---

$rbCom = New-Object System.Windows.Forms.RadioButton
$rbCom.Text = "Port COM"
$rbCom.Location = New-Object System.Drawing.Point(12, 10)
$rbCom.Size = New-Object System.Drawing.Size(90, 20)
$rbCom.Checked = $true

$rbIp = New-Object System.Windows.Forms.RadioButton
$rbIp.Text = "Siec (IP) - konwerter RS485<->Ethernet/WiFi"
$rbIp.Location = New-Object System.Drawing.Point(110, 10)
$rbIp.Size = New-Object System.Drawing.Size(320, 20)

# -- COM row --
$lblPort = New-Object System.Windows.Forms.Label
$lblPort.Text = "Port COM:"
$lblPort.Location = New-Object System.Drawing.Point(12, 38)
$lblPort.AutoSize = $true

$cmbPort = New-Object System.Windows.Forms.ComboBox
$cmbPort.Location = New-Object System.Drawing.Point(90, 35)
$cmbPort.Size = New-Object System.Drawing.Size(120, 24)
$cmbPort.DropDownStyle = "DropDownList"

$btnRefreshPorts = New-Object System.Windows.Forms.Button
$btnRefreshPorts.Text = "Odswiez porty"
$btnRefreshPorts.Location = New-Object System.Drawing.Point(220, 34)
$btnRefreshPorts.Size = New-Object System.Drawing.Size(110, 26)

# -- serial settings row (COM only): data bits, parity, stop bits, flow control --
$lblDataBits = New-Object System.Windows.Forms.Label
$lblDataBits.Text = "Bity:"
$lblDataBits.Location = New-Object System.Drawing.Point(12, 65)
$lblDataBits.AutoSize = $true
$cmbDataBits = New-Object System.Windows.Forms.ComboBox
$cmbDataBits.Location = New-Object System.Drawing.Point(50, 62)
$cmbDataBits.Size = New-Object System.Drawing.Size(50, 24)
$cmbDataBits.DropDownStyle = "DropDownList"
$cmbDataBits.Items.AddRange(@("5", "6", "7", "8"))
$cmbDataBits.SelectedItem = "8"

$lblParity = New-Object System.Windows.Forms.Label
$lblParity.Text = "Parzyst.:"
$lblParity.Location = New-Object System.Drawing.Point(112, 65)
$lblParity.AutoSize = $true
$cmbParity = New-Object System.Windows.Forms.ComboBox
$cmbParity.Location = New-Object System.Drawing.Point(172, 62)
$cmbParity.Size = New-Object System.Drawing.Size(80, 24)
$cmbParity.DropDownStyle = "DropDownList"
$cmbParity.Items.AddRange(@("None", "Even", "Odd", "Mark", "Space"))
$cmbParity.SelectedItem = "None"

$lblStopBits = New-Object System.Windows.Forms.Label
$lblStopBits.Text = "Stop:"
$lblStopBits.Location = New-Object System.Drawing.Point(262, 65)
$lblStopBits.AutoSize = $true
$cmbStopBits = New-Object System.Windows.Forms.ComboBox
$cmbStopBits.Location = New-Object System.Drawing.Point(300, 62)
$cmbStopBits.Size = New-Object System.Drawing.Size(90, 24)
$cmbStopBits.DropDownStyle = "DropDownList"
$cmbStopBits.Items.AddRange(@("1", "1.5", "2"))
$cmbStopBits.SelectedItem = "1"
$stopBitsDisplayMap = [ordered]@{
    '1'   = [System.IO.Ports.StopBits]::One
    '1.5' = [System.IO.Ports.StopBits]::OnePointFive
    '2'   = [System.IO.Ports.StopBits]::Two
}

$lblHandshake = New-Object System.Windows.Forms.Label
$lblHandshake.Text = "Kontrola przeplywu:"
$lblHandshake.Location = New-Object System.Drawing.Point(400, 65)
$lblHandshake.AutoSize = $true
$cmbHandshake = New-Object System.Windows.Forms.ComboBox
$cmbHandshake.Location = New-Object System.Drawing.Point(520, 62)
$cmbHandshake.Size = New-Object System.Drawing.Size(170, 24)
$cmbHandshake.DropDownStyle = "DropDownList"
$cmbHandshake.Items.AddRange(@("None", "XOnXOff", "RequestToSend", "RequestToSendXOnXOff"))
$cmbHandshake.SelectedItem = "None"

function Update-PortList {
    $current = $cmbPort.SelectedItem
    $cmbPort.Items.Clear()
    $ports = [System.IO.Ports.SerialPort]::GetPortNames() | Sort-Object
    foreach ($p in $ports) { $cmbPort.Items.Add($p) | Out-Null }
    if ($current -and $cmbPort.Items.Contains($current)) { $cmbPort.SelectedItem = $current }
    elseif ($cmbPort.Items.Count -gt 0) { $cmbPort.SelectedIndex = 0 }
}
$btnRefreshPorts.Add_Click({ Update-PortList })

# -- IP row (same position as the COM row, toggled via visibility) --
$lblIp = New-Object System.Windows.Forms.Label
$lblIp.Text = "Adres IP:"
$lblIp.Location = New-Object System.Drawing.Point(12, 38)
$lblIp.AutoSize = $true
$lblIp.Visible = $false

$txtIp = New-Object System.Windows.Forms.TextBox
$txtIp.Location = New-Object System.Drawing.Point(90, 35)
$txtIp.Size = New-Object System.Drawing.Size(120, 24)
$txtIp.Visible = $false

$lblTcpPort = New-Object System.Windows.Forms.Label
$lblTcpPort.Text = "Port TCP:"
$lblTcpPort.Location = New-Object System.Drawing.Point(220, 38)
$lblTcpPort.AutoSize = $true
$lblTcpPort.Visible = $false

$nudTcpPort = New-Object System.Windows.Forms.NumericUpDown
$nudTcpPort.Location = New-Object System.Drawing.Point(285, 35)
$nudTcpPort.Size = New-Object System.Drawing.Size(70, 24)
$nudTcpPort.Minimum = 1; $nudTcpPort.Maximum = 65535; $nudTcpPort.Value = 502
$nudTcpPort.Visible = $false

$lblIpHint = New-Object System.Windows.Forms.Label
$lblIpHint.Text = "np. Elfin EW11 / USR-TCP232 w trybie przezroczystym (port bywa 502/8899/4196/23)"
$lblIpHint.Location = New-Object System.Drawing.Point(365, 38)
$lblIpHint.AutoSize = $true
$lblIpHint.ForeColor = [System.Drawing.Color]::Gray
$lblIpHint.Visible = $false

$chkMbap = New-Object System.Windows.Forms.CheckBox
$chkMbap.Text = "Prawdziwy Modbus TCP (MBAP, bez CRC) zamiast RTU tunelowanego przez TCP - sprobuj, jesli skan nic nie znajduje (typowo port 502)"
$chkMbap.Location = New-Object System.Drawing.Point(90, 62)
$chkMbap.Size = New-Object System.Drawing.Size(620, 20)
$chkMbap.Visible = $false

$rbCom.Add_CheckedChanged({
    $comVisible = $rbCom.Checked
    $lblPort.Visible = $comVisible
    $cmbPort.Visible = $comVisible
    $btnRefreshPorts.Visible = $comVisible
    $lblIp.Visible = -not $comVisible
    $txtIp.Visible = -not $comVisible
    $lblTcpPort.Visible = -not $comVisible
    $nudTcpPort.Visible = -not $comVisible
    $lblIpHint.Visible = -not $comVisible
    $chkMbap.Visible = -not $comVisible
    $lblDataBits.Visible = $comVisible
    $cmbDataBits.Visible = $comVisible
    $lblParity.Visible = $comVisible
    $cmbParity.Visible = $comVisible
    $lblStopBits.Visible = $comVisible
    $cmbStopBits.Visible = $comVisible
    $lblHandshake.Visible = $comVisible
    $cmbHandshake.Visible = $comVisible
    if ($lblBaud) {
        # Baud rate only applies to COM connections - nothing to show in network mode
        $lblBaud.Visible = $comVisible
        $clbBaud.Visible = $comVisible
        $btnPresetR4.Visible = $comVisible
        $btnPresetAll.Visible = $comVisible
    }
    if (-not $comVisible -and $nudTimeout -and $nudTimeout.Value -lt 500) {
        # Local RS485 answers within ~150 ms, but over the network (especially WiFi) the round trip is longer
        $nudTimeout.Value = 500
    }
})

function Get-GuiConnection {
    param([int]$Baud = 9600, [int]$TimeoutMs = 300)
    if ($rbIp.Checked) {
        if ([string]::IsNullOrWhiteSpace($txtIp.Text)) { throw "Podaj adres IP konwertera." }
        return Open-ModbusConnection -IpAddress $txtIp.Text.Trim() -TcpPort ([int]$nudTcpPort.Value) -TimeoutMs $TimeoutMs
    } else {
        if (-not $cmbPort.SelectedItem) { throw "Wybierz port COM." }
        $dataBits = [int]$cmbDataBits.SelectedItem
        $parity = [System.IO.Ports.Parity]$cmbParity.SelectedItem
        $stopBits = $stopBitsDisplayMap[$cmbStopBits.SelectedItem.ToString()]
        $handshake = [System.IO.Ports.Handshake]$cmbHandshake.SelectedItem
        return Open-ModbusConnection -ComPort $cmbPort.SelectedItem.ToString() -BaudRate $Baud `
            -DataBits $dataBits -Parity $parity -StopBits $stopBits -Handshake $handshake -TimeoutMs $TimeoutMs
    }
}

function Get-GuiMbap {
    return ($rbIp.Checked -and $chkMbap.Checked)
}

$form.Controls.AddRange(@(
    $rbCom, $rbIp,
    $lblPort, $cmbPort, $btnRefreshPorts,
    $lblDataBits, $cmbDataBits, $lblParity, $cmbParity, $lblStopBits, $cmbStopBits, $lblHandshake, $cmbHandshake,
    $lblIp, $txtIp, $lblTcpPort, $nudTcpPort, $lblIpHint, $chkMbap
))

$tabs = New-Object System.Windows.Forms.TabControl
$tabs.Location = New-Object System.Drawing.Point(12, 90)
$tabs.Size = New-Object System.Drawing.Size(720, 508)
$form.Controls.Add($tabs)

# ===================== Tab 1: Scan =====================

$tabScan = New-Object System.Windows.Forms.TabPage
$tabScan.Text = "Skanowanie"
$tabs.TabPages.Add($tabScan)

$lblBaud = New-Object System.Windows.Forms.Label
$lblBaud.Text = "Baudrate (tylko COM):"
$lblBaud.Location = New-Object System.Drawing.Point(10, 12)
$lblBaud.AutoSize = $true

$clbBaud = New-Object System.Windows.Forms.CheckedListBox
$clbBaud.Location = New-Object System.Drawing.Point(10, 32)
$clbBaud.Size = New-Object System.Drawing.Size(110, 130)
$allBaudRates = 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 128000, 256000
foreach ($b in $allBaudRates) { $clbBaud.Items.Add($b) | Out-Null }
for ($i = 0; $i -lt 5; $i++) { $clbBaud.SetItemChecked($i, $true) }  # default 1200-19200

$btnPresetR4 = New-Object System.Windows.Forms.Button
$btnPresetR4.Text = "Tylko R4DCB08"
$btnPresetR4.Location = New-Object System.Drawing.Point(10, 166)
$btnPresetR4.Size = New-Object System.Drawing.Size(110, 24)
$btnPresetR4.Add_Click({
    for ($i = 0; $i -lt $clbBaud.Items.Count; $i++) { $clbBaud.SetItemChecked($i, $i -lt 5) }
})

$btnPresetAll = New-Object System.Windows.Forms.Button
$btnPresetAll.Text = "Zaznacz wszystkie"
$btnPresetAll.Location = New-Object System.Drawing.Point(10, 194)
$btnPresetAll.Size = New-Object System.Drawing.Size(110, 24)
$btnPresetAll.Add_Click({
    for ($i = 0; $i -lt $clbBaud.Items.Count; $i++) { $clbBaud.SetItemChecked($i, $true) }
})

$lblStartAddr = New-Object System.Windows.Forms.Label
$lblStartAddr.Text = "Adres od:"
$lblStartAddr.Location = New-Object System.Drawing.Point(140, 15)
$lblStartAddr.AutoSize = $true
$nudStartAddr = New-Object System.Windows.Forms.NumericUpDown
$nudStartAddr.Location = New-Object System.Drawing.Point(220, 12)
$nudStartAddr.Size = New-Object System.Drawing.Size(60, 24)
$nudStartAddr.Minimum = 0; $nudStartAddr.Maximum = 255; $nudStartAddr.Value = 1

$lblEndAddr = New-Object System.Windows.Forms.Label
$lblEndAddr.Text = "Adres do:"
$lblEndAddr.Location = New-Object System.Drawing.Point(140, 45)
$lblEndAddr.AutoSize = $true
$nudEndAddr = New-Object System.Windows.Forms.NumericUpDown
$nudEndAddr.Location = New-Object System.Drawing.Point(220, 42)
$nudEndAddr.Size = New-Object System.Drawing.Size(60, 24)
$nudEndAddr.Minimum = 0; $nudEndAddr.Maximum = 255; $nudEndAddr.Value = 20

$lblFunc = New-Object System.Windows.Forms.Label
$lblFunc.Text = "Funkcja Modbus:"
$lblFunc.Location = New-Object System.Drawing.Point(140, 75)
$lblFunc.AutoSize = $true
$cmbFunc = New-Object System.Windows.Forms.ComboBox
$cmbFunc.Location = New-Object System.Drawing.Point(140, 95)
$cmbFunc.Size = New-Object System.Drawing.Size(220, 24)
$cmbFunc.DropDownStyle = "DropDownList"
$funcCodeMap = [ordered]@{
    "0x01 - Read Coils"              = 1
    "0x03 - Read Holding Registers"  = 3
    "0x04 - Read Input Registers"    = 4
}
foreach ($key in $funcCodeMap.Keys) { $cmbFunc.Items.Add($key) | Out-Null }
$cmbFunc.SelectedIndex = 1

$lblRegister = New-Object System.Windows.Forms.Label
$lblRegister.Text = "Rejestr startowy:"
$lblRegister.Location = New-Object System.Drawing.Point(380, 15)
$lblRegister.AutoSize = $true
$nudRegister = New-Object System.Windows.Forms.NumericUpDown
$nudRegister.Location = New-Object System.Drawing.Point(500, 12)
$nudRegister.Size = New-Object System.Drawing.Size(60, 24)
$nudRegister.Minimum = 0; $nudRegister.Maximum = 65535; $nudRegister.Value = 0

$lblQuantity = New-Object System.Windows.Forms.Label
$lblQuantity.Text = "Ilosc:"
$lblQuantity.Location = New-Object System.Drawing.Point(380, 45)
$lblQuantity.AutoSize = $true
$nudQuantity = New-Object System.Windows.Forms.NumericUpDown
$nudQuantity.Location = New-Object System.Drawing.Point(500, 42)
$nudQuantity.Size = New-Object System.Drawing.Size(60, 24)
$nudQuantity.Minimum = 1; $nudQuantity.Maximum = 125; $nudQuantity.Value = 8

$lblTimeout = New-Object System.Windows.Forms.Label
$lblTimeout.Text = "Timeout (ms):"
$lblTimeout.Location = New-Object System.Drawing.Point(380, 75)
$lblTimeout.AutoSize = $true
$nudTimeout = New-Object System.Windows.Forms.NumericUpDown
$nudTimeout.Location = New-Object System.Drawing.Point(500, 72)
$nudTimeout.Size = New-Object System.Drawing.Size(60, 24)
$nudTimeout.Minimum = 50; $nudTimeout.Maximum = 5000; $nudTimeout.Value = 150

$btnScan = New-Object System.Windows.Forms.Button
$btnScan.Text = "Skanuj"
$btnScan.Location = New-Object System.Drawing.Point(600, 32)
$btnScan.Size = New-Object System.Drawing.Size(100, 30)

$btnCancelScan = New-Object System.Windows.Forms.Button
$btnCancelScan.Text = "Anuluj"
$btnCancelScan.Location = New-Object System.Drawing.Point(600, 68)
$btnCancelScan.Size = New-Object System.Drawing.Size(100, 30)
$btnCancelScan.Enabled = $false

$progressScan = New-Object System.Windows.Forms.ProgressBar
$progressScan.Location = New-Object System.Drawing.Point(10, 225)
$progressScan.Size = New-Object System.Drawing.Size(690, 20)

$lblScanStatus = New-Object System.Windows.Forms.Label
$lblScanStatus.Text = "Gotowy."
$lblScanStatus.Location = New-Object System.Drawing.Point(10, 250)
$lblScanStatus.AutoSize = $true

$lvScanResults = New-Object System.Windows.Forms.ListView
$lvScanResults.Location = New-Object System.Drawing.Point(10, 275)
$lvScanResults.Size = New-Object System.Drawing.Size(690, 210)
$lvScanResults.View = "Details"
$lvScanResults.FullRowSelect = $true
$lvScanResults.Columns.Add("Baudrate", 90) | Out-Null
$lvScanResults.Columns.Add("Adres", 60) | Out-Null
$lvScanResults.Columns.Add("Odpowiedz (hex)", 500) | Out-Null

$tabScan.Controls.AddRange(@(
    $lblBaud, $clbBaud, $btnPresetR4, $btnPresetAll,
    $lblStartAddr, $nudStartAddr, $lblEndAddr, $nudEndAddr,
    $lblFunc, $cmbFunc, $lblRegister, $nudRegister, $lblQuantity, $nudQuantity,
    $lblTimeout, $nudTimeout, $btnScan, $btnCancelScan,
    $progressScan, $lblScanStatus, $lvScanResults
))

$script:cancelScan = $false

$btnScan.Add_Click({
    # Over IP there is no point sweeping baud rates (the converter fixes the baud rate) -
    # run a single connection "iteration"; over COM, one per checked baud rate.
    $baudList = @()
    if ($rbIp.Checked) {
        if ([string]::IsNullOrWhiteSpace($txtIp.Text)) {
            [System.Windows.Forms.MessageBox]::Show("Podaj adres IP konwertera.", "Brak adresu IP") | Out-Null
            return
        }
        $baudList = @(0)  # placeholder - single iteration, the value is not used to open the connection
    } else {
        if (-not $cmbPort.SelectedItem) {
            [System.Windows.Forms.MessageBox]::Show("Wybierz port COM.", "Brak portu") | Out-Null
            return
        }
        foreach ($item in $clbBaud.CheckedItems) { $baudList += [int]$item }
        if ($baudList.Count -eq 0) {
            [System.Windows.Forms.MessageBox]::Show("Zaznacz co najmniej jeden baudrate.", "Brak baudrate") | Out-Null
            return
        }
    }

    $startAddr = [int]$nudStartAddr.Value
    $endAddr = [int]$nudEndAddr.Value
    $funcCode = [byte]$funcCodeMap[$cmbFunc.SelectedItem.ToString()]
    $register = [int]$nudRegister.Value
    $quantity = [int]$nudQuantity.Value
    $timeout = [int]$nudTimeout.Value

    $lvScanResults.Items.Clear()
    $script:cancelScan = $false
    $btnScan.Enabled = $false
    $btnCancelScan.Enabled = $true

    $total = $baudList.Count * ($endAddr - $startAddr + 1)
    $done = 0
    $progressScan.Minimum = 0
    $progressScan.Maximum = [Math]::Max($total, 1)
    $progressScan.Value = 0

    foreach ($baud in $baudList) {
        if ($script:cancelScan) { break }
        $baudLabel = if ($rbIp.Checked) { "$($txtIp.Text.Trim()):$([int]$nudTcpPort.Value)" } else { $baud }
        $lblScanStatus.Text = "Laczenie: $baudLabel..."
        [System.Windows.Forms.Application]::DoEvents()

        $connection = $null
        try {
            $connection = Get-GuiConnection -Baud $baud -TimeoutMs $timeout
        } catch {
            $done += ($endAddr - $startAddr + 1)
            $progressScan.Value = [Math]::Min($done, $progressScan.Maximum)
            continue
        }

        for ($addr = $startAddr; $addr -le $endAddr; $addr++) {
            if ($script:cancelScan) { break }
            $done++
            $progressScan.Value = [Math]::Min($done, $progressScan.Maximum)
            $lblScanStatus.Text = "$baudLabel, adres $addr..."

            $useMbap = Get-GuiMbap
            $req = if ($useMbap) { Build-MbapReadRequest -UnitId $addr -FunctionCode $funcCode -Register $register -Quantity $quantity } else { Build-ModbusRequest -SlaveId $addr -FunctionCode $funcCode -Register $register -Quantity $quantity }
            $resp = $null
            try { $resp = Send-ModbusRequest -Connection $connection -Request $req -TimeoutMs $timeout } catch { $resp = $null }

            [System.Windows.Forms.Application]::DoEvents()

            $found = if ($useMbap) { Test-MbapResponse -Response $resp -UnitId $addr } else { $resp -and $resp.Length -ge 5 -and $resp[0] -eq $addr -and (Test-ModbusCRC $resp) }
            if ($found) {
                $item = New-Object System.Windows.Forms.ListViewItem($baudLabel.ToString())
                $item.SubItems.Add($addr.ToString()) | Out-Null
                $item.SubItems.Add((Format-Hex $resp)) | Out-Null
                $lvScanResults.Items.Add($item) | Out-Null
            }
        }
        Close-ModbusConnection -Connection $connection
    }

    $lblScanStatus.Text = if ($script:cancelScan) { "Anulowano." } else { "Skanowanie zakonczone. Znaleziono: $($lvScanResults.Items.Count)" }
    $btnScan.Enabled = $true
    $btnCancelScan.Enabled = $false
})

$btnCancelScan.Add_Click({ $script:cancelScan = $true })

# ===================== Tab 2: Temperature (R4DCB08) =====================

$tabTemp = New-Object System.Windows.Forms.TabPage
$tabTemp.Text = "Temperatura (R4DCB08)"
$tabs.TabPages.Add($tabTemp)

$lblTempBaud = New-Object System.Windows.Forms.Label
$lblTempBaud.Text = "Baudrate (tylko COM):"
$lblTempBaud.Location = New-Object System.Drawing.Point(10, 15)
$lblTempBaud.AutoSize = $true
$cmbTempBaud = New-Object System.Windows.Forms.ComboBox
$cmbTempBaud.Location = New-Object System.Drawing.Point(140, 12)
$cmbTempBaud.Size = New-Object System.Drawing.Size(90, 24)
$cmbTempBaud.DropDownStyle = "DropDownList"
$cmbTempBaud.Items.AddRange(@("1200", "2400", "4800", "9600", "19200"))
$cmbTempBaud.SelectedItem = "9600"

$lblTempAddr = New-Object System.Windows.Forms.Label
$lblTempAddr.Text = "Adres:"
$lblTempAddr.Location = New-Object System.Drawing.Point(250, 15)
$lblTempAddr.AutoSize = $true
$nudTempAddr = New-Object System.Windows.Forms.NumericUpDown
$nudTempAddr.Location = New-Object System.Drawing.Point(300, 12)
$nudTempAddr.Size = New-Object System.Drawing.Size(60, 24)
$nudTempAddr.Minimum = 1; $nudTempAddr.Maximum = 247; $nudTempAddr.Value = 1

$btnReadOnce = New-Object System.Windows.Forms.Button
$btnReadOnce.Text = "Odczytaj raz"
$btnReadOnce.Location = New-Object System.Drawing.Point(380, 11)
$btnReadOnce.Size = New-Object System.Drawing.Size(100, 26)

$chkLoop = New-Object System.Windows.Forms.CheckBox
$chkLoop.Text = "Tryb ciagly, co (s):"
$chkLoop.Location = New-Object System.Drawing.Point(500, 15)
$chkLoop.AutoSize = $true
$nudInterval = New-Object System.Windows.Forms.NumericUpDown
$nudInterval.Location = New-Object System.Drawing.Point(630, 12)
$nudInterval.Size = New-Object System.Drawing.Size(50, 24)
$nudInterval.Minimum = 1; $nudInterval.Maximum = 3600; $nudInterval.Value = 2

$lvTemp = New-Object System.Windows.Forms.ListView
$lvTemp.Location = New-Object System.Drawing.Point(10, 55)
$lvTemp.Size = New-Object System.Drawing.Size(690, 200)
$lvTemp.View = "Details"
$lvTemp.FullRowSelect = $true
$lvTemp.Columns.Add("Kanal", 80) | Out-Null
$lvTemp.Columns.Add("Temperatura C", 150) | Out-Null
$lvTemp.Columns.Add("Status", 150) | Out-Null

$lblTempStatus = New-Object System.Windows.Forms.Label
$lblTempStatus.Text = "Gotowy."
$lblTempStatus.Location = New-Object System.Drawing.Point(10, 265)
$lblTempStatus.AutoSize = $true

$tabTemp.Controls.AddRange(@(
    $lblTempBaud, $cmbTempBaud, $lblTempAddr, $nudTempAddr, $btnReadOnce,
    $chkLoop, $nudInterval, $lvTemp, $lblTempStatus
))

function Update-TempListView {
    param($Results)
    $lvTemp.Items.Clear()
    foreach ($r in $Results) {
        $item = New-Object System.Windows.Forms.ListViewItem($r.Kanal.ToString())
        $tempText = if ($null -eq $r.TemperaturaC) { "-" } else { $r.TemperaturaC.ToString() }
        $item.SubItems.Add($tempText) | Out-Null
        $item.SubItems.Add($r.Status) | Out-Null
        $lvTemp.Items.Add($item) | Out-Null
    }
}

function Invoke-TempRead {
    $addr = [byte]$nudTempAddr.Value
    $baud = [int]$cmbTempBaud.SelectedItem
    $connection = $null
    try {
        $connection = Get-GuiConnection -Baud $baud -TimeoutMs 300
        $results = Get-R4DCB08Temperatures -Connection $connection -SlaveId $addr -TimeoutMs 300 -Mbap (Get-GuiMbap)
        Update-TempListView -Results $results
        $lblTempStatus.Text = "Odczyt OK - $(Get-Date -Format 'HH:mm:ss')"
    } catch {
        $lblTempStatus.Text = "Blad: $($_.Exception.Message)"
    } finally {
        Close-ModbusConnection -Connection $connection
    }
}

$btnReadOnce.Add_Click({ Invoke-TempRead })

$timerTemp = New-Object System.Windows.Forms.Timer
$timerTemp.Add_Tick({ Invoke-TempRead })

$chkLoop.Add_Click({
    if ($chkLoop.Checked) {
        $timerTemp.Interval = [int]$nudInterval.Value * 1000
        $timerTemp.Start()
        $btnReadOnce.Enabled = $false
    } else {
        $timerTemp.Stop()
        $btnReadOnce.Enabled = $true
    }
})

# ===================== Tab 3: Change address =====================

$tabAddr = New-Object System.Windows.Forms.TabPage
$tabAddr.Text = "Zmiana adresu"
$tabs.TabPages.Add($tabAddr)

$lblAddrBaud = New-Object System.Windows.Forms.Label
$lblAddrBaud.Text = "Aktualny baudrate (tylko COM):"
$lblAddrBaud.Location = New-Object System.Drawing.Point(10, 18)
$lblAddrBaud.AutoSize = $true
$cmbAddrBaud = New-Object System.Windows.Forms.ComboBox
$cmbAddrBaud.Location = New-Object System.Drawing.Point(210, 15)
$cmbAddrBaud.Size = New-Object System.Drawing.Size(90, 24)
$cmbAddrBaud.DropDownStyle = "DropDownList"
$cmbAddrBaud.Items.AddRange(@("1200", "2400", "4800", "9600", "19200"))
$cmbAddrBaud.SelectedItem = "9600"

$lblOldAddr = New-Object System.Windows.Forms.Label
$lblOldAddr.Text = "Aktualny adres:"
$lblOldAddr.Location = New-Object System.Drawing.Point(10, 55)
$lblOldAddr.AutoSize = $true
$nudOldAddr = New-Object System.Windows.Forms.NumericUpDown
$nudOldAddr.Location = New-Object System.Drawing.Point(150, 52)
$nudOldAddr.Size = New-Object System.Drawing.Size(60, 24)
$nudOldAddr.Minimum = 0; $nudOldAddr.Maximum = 247; $nudOldAddr.Value = 1

$lblNewAddr = New-Object System.Windows.Forms.Label
$lblNewAddr.Text = "Nowy adres:"
$lblNewAddr.Location = New-Object System.Drawing.Point(10, 92)
$lblNewAddr.AutoSize = $true
$nudNewAddr = New-Object System.Windows.Forms.NumericUpDown
$nudNewAddr.Location = New-Object System.Drawing.Point(150, 89)
$nudNewAddr.Size = New-Object System.Drawing.Size(60, 24)
$nudNewAddr.Minimum = 1; $nudNewAddr.Maximum = 247; $nudNewAddr.Value = 1

$btnSetAddr = New-Object System.Windows.Forms.Button
$btnSetAddr.Text = "Zmien adres"
$btnSetAddr.Location = New-Object System.Drawing.Point(150, 125)
$btnSetAddr.Size = New-Object System.Drawing.Size(120, 30)

$txtAddrLog = New-Object System.Windows.Forms.TextBox
$txtAddrLog.Location = New-Object System.Drawing.Point(10, 170)
$txtAddrLog.Size = New-Object System.Drawing.Size(690, 300)
$txtAddrLog.Multiline = $true
$txtAddrLog.ScrollBars = "Vertical"
$txtAddrLog.ReadOnly = $true
$txtAddrLog.Font = New-Object System.Drawing.Font("Consolas", 9)

$lblAddrWarn = New-Object System.Windows.Forms.Label
$lblAddrWarn.Text = "Uwaga: przy kolizji adresow (dwa urzadzenia na tym samym adresie) zmiana obejmie OBA naraz."
$lblAddrWarn.Location = New-Object System.Drawing.Point(280, 55)
$lblAddrWarn.Size = New-Object System.Drawing.Size(420, 60)
$lblAddrWarn.ForeColor = [System.Drawing.Color]::DarkOrange

$tabAddr.Controls.AddRange(@(
    $lblAddrBaud, $cmbAddrBaud, $lblOldAddr, $nudOldAddr, $lblNewAddr, $nudNewAddr,
    $btnSetAddr, $txtAddrLog, $lblAddrWarn
))

$btnSetAddr.Add_Click({
    $baud = [int]$cmbAddrBaud.SelectedItem
    $oldAddr = [byte]$nudOldAddr.Value
    $newAddr = [byte]$nudNewAddr.Value

    $request = if (Get-GuiMbap) { Build-MbapWriteSingleRegister -UnitId $oldAddr -Register 254 -Value $newAddr } else { Build-WriteSingleRegister -SlaveId $oldAddr -Register 254 -Value $newAddr }
    $txtAddrLog.AppendText("[$(Get-Date -Format 'HH:mm:ss')] Wysylam: $(Format-Hex $request)`r`n")

    $connection = $null
    try {
        $connection = Get-GuiConnection -Baud $baud -TimeoutMs 300
        $resp = Send-ModbusRequest -Connection $connection -Request $request -TimeoutMs 300

        if (-not $resp) {
            $txtAddrLog.AppendText("  Brak odpowiedzi. Sprawdz adres/baudrate/polaczenie.`r`n")
            return
        }
        $txtAddrLog.AppendText("  Odebrano: $(Format-Hex $resp)`r`n")

        if ($resp.Length -eq $request.Length -and (Compare-Object $resp $request -SyncWindow 0).Count -eq 0) {
            $txtAddrLog.AppendText("  SUKCES: adres zmieniony z $oldAddr na $newAddr.`r`n")
        } else {
            $txtAddrLog.AppendText("  Odpowiedz nie jest oczekiwanym echem - zweryfikuj ponownym skanowaniem.`r`n")
        }
    } catch {
        $txtAddrLog.AppendText("  Blad: $($_.Exception.Message)`r`n")
    } finally {
        Close-ModbusConnection -Connection $connection
    }
})

# ===================== Tab 4: Change baud rate =====================

$tabBaud = New-Object System.Windows.Forms.TabPage
$tabBaud.Text = "Zmiana baudrate"
$tabs.TabPages.Add($tabBaud)

$lblBaudCurBaud = New-Object System.Windows.Forms.Label
$lblBaudCurBaud.Text = "Aktualny baudrate (tylko COM):"
$lblBaudCurBaud.Location = New-Object System.Drawing.Point(10, 18)
$lblBaudCurBaud.AutoSize = $true
$cmbBaudCurBaud = New-Object System.Windows.Forms.ComboBox
$cmbBaudCurBaud.Location = New-Object System.Drawing.Point(210, 15)
$cmbBaudCurBaud.Size = New-Object System.Drawing.Size(90, 24)
$cmbBaudCurBaud.DropDownStyle = "DropDownList"
$cmbBaudCurBaud.Items.AddRange(@("1200", "2400", "4800", "9600", "19200"))
$cmbBaudCurBaud.SelectedItem = "9600"

$lblBaudAddr = New-Object System.Windows.Forms.Label
$lblBaudAddr.Text = "Adres urzadzenia:"
$lblBaudAddr.Location = New-Object System.Drawing.Point(10, 55)
$lblBaudAddr.AutoSize = $true
$nudBaudAddr = New-Object System.Windows.Forms.NumericUpDown
$nudBaudAddr.Location = New-Object System.Drawing.Point(150, 52)
$nudBaudAddr.Size = New-Object System.Drawing.Size(60, 24)
$nudBaudAddr.Minimum = 1; $nudBaudAddr.Maximum = 247; $nudBaudAddr.Value = 1

$lblNewBaud = New-Object System.Windows.Forms.Label
$lblNewBaud.Text = "Nowy baudrate:"
$lblNewBaud.Location = New-Object System.Drawing.Point(10, 92)
$lblNewBaud.AutoSize = $true
$cmbNewBaud = New-Object System.Windows.Forms.ComboBox
$cmbNewBaud.Location = New-Object System.Drawing.Point(150, 89)
$cmbNewBaud.Size = New-Object System.Drawing.Size(90, 24)
$cmbNewBaud.DropDownStyle = "DropDownList"
$cmbNewBaud.Items.AddRange(@("1200", "2400", "4800", "9600", "19200"))
$cmbNewBaud.SelectedItem = "9600"

$btnSetBaud = New-Object System.Windows.Forms.Button
$btnSetBaud.Text = "Zmien baudrate"
$btnSetBaud.Location = New-Object System.Drawing.Point(150, 125)
$btnSetBaud.Size = New-Object System.Drawing.Size(120, 30)

$txtBaudLog = New-Object System.Windows.Forms.TextBox
$txtBaudLog.Location = New-Object System.Drawing.Point(10, 170)
$txtBaudLog.Size = New-Object System.Drawing.Size(690, 300)
$txtBaudLog.Multiline = $true
$txtBaudLog.ScrollBars = "Vertical"
$txtBaudLog.ReadOnly = $true
$txtBaudLog.Font = New-Object System.Drawing.Font("Consolas", 9)

$lblBaudWarn = New-Object System.Windows.Forms.Label
$lblBaudWarn.Text = "Po zmianie urzadzenie moze wymagac ponownego wlaczenia zasilania (power cycle)."
$lblBaudWarn.Location = New-Object System.Drawing.Point(280, 55)
$lblBaudWarn.Size = New-Object System.Drawing.Size(420, 60)
$lblBaudWarn.ForeColor = [System.Drawing.Color]::DarkOrange

$tabBaud.Controls.AddRange(@(
    $lblBaudCurBaud, $cmbBaudCurBaud, $lblBaudAddr, $nudBaudAddr, $lblNewBaud, $cmbNewBaud,
    $btnSetBaud, $txtBaudLog, $lblBaudWarn
))

$btnSetBaud.Add_Click({
    $curBaud = [int]$cmbBaudCurBaud.SelectedItem
    $addr = [byte]$nudBaudAddr.Value
    $newBaud = [int]$cmbNewBaud.SelectedItem
    $code = $baudCodeMap[$newBaud]

    $request = if (Get-GuiMbap) { Build-MbapWriteSingleRegister -UnitId $addr -Register 255 -Value $code } else { Build-WriteSingleRegister -SlaveId $addr -Register 255 -Value $code }
    $txtBaudLog.AppendText("[$(Get-Date -Format 'HH:mm:ss')] Wysylam (kod $code -> $newBaud): $(Format-Hex $request)`r`n")

    $connection = $null
    try {
        $connection = Get-GuiConnection -Baud $curBaud -TimeoutMs 300
        $resp = Send-ModbusRequest -Connection $connection -Request $request -TimeoutMs 300

        if (-not $resp) {
            $txtBaudLog.AppendText("  Brak odpowiedzi. Sprawdz adres/baudrate/polaczenie.`r`n")
            return
        }
        $txtBaudLog.AppendText("  Odebrano: $(Format-Hex $resp)`r`n")

        if ($resp.Length -eq $request.Length -and (Compare-Object $resp $request -SyncWindow 0).Count -eq 0) {
            $txtBaudLog.AppendText("  SUKCES: baudrate zmieniony na $newBaud.`r`n")
        } else {
            $txtBaudLog.AppendText("  Odpowiedz nie jest oczekiwanym echem - zweryfikuj ponownym skanowaniem.`r`n")
        }
    } catch {
        $txtBaudLog.AppendText("  Blad: $($_.Exception.Message)`r`n")
    } finally {
        Close-ModbusConnection -Connection $connection
    }
})

# ===================== Start =====================

Update-PortList
[System.Windows.Forms.Application]::Run($form)
