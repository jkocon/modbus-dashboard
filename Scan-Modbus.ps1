<#
.SYNOPSIS
    Scans for Modbus RTU devices across baud rates and RS485 addresses.
    Works over a COM port (RS485) or over the network through an
    RS485<->Ethernet/WiFi converter (RTU over TCP, transparent mode).

.PARAMETER ComPort
    COM port name, e.g. COM3. Specify this OR IpAddress.

.PARAMETER IpAddress
    IP address of the RS485<->Ethernet/WiFi converter. Specify this OR ComPort.
    In this mode -BaudRates is ignored: the baud rate is configured in the
    converter (RS485 side) and cannot be swept from the TCP client, so the scan
    walks the addresses once over a single open connection.

.PARAMETER TcpPort
    TCP port of the converter in transparent mode (default 502).

.PARAMETER Mbap
    Use native Modbus TCP (MBAP header, no CRC) instead of RTU tunnelled over
    TCP. -IpAddress only. Port 502 is the standard port for this mode - if a
    scan without -Mbap finds nothing on port 502, try it with -Mbap.

.PARAMETER BaudRates
    Baud rates to try (-ComPort mode only)

.PARAMETER DataBits
    Data bits: 5-8 (default 8). COM only.

.PARAMETER Parity
    Parity: None/Even/Odd/Mark/Space (default None). COM only.

.PARAMETER StopBits
    Stop bits: One/OnePointFive/Two (default One). COM only.

.PARAMETER Handshake
    Flow control: None/XOnXOff/RequestToSend/RequestToSendXOnXOff (default None). COM only.

.PARAMETER StartAddress / EndAddress
    RS485 address range to scan (Modbus allows 1-247)

.PARAMETER FunctionCode
    Modbus function code (default 0x01 - Read Coils, right for Waveshare Modbus RTU Relay boards)

.PARAMETER Register
    Start register/coil to read (default 0)

.PARAMETER Quantity
    Number of registers/coils to read (default 32, for the 32-channel relay variant)

.PARAMETER TimeoutMs
    Response timeout in ms (default 150)

.EXAMPLE
    .\Scan-Modbus.ps1 -ComPort COM3

.EXAMPLE
    .\Scan-Modbus.ps1 -ComPort COM3 -StartAddress 1 -EndAddress 255 -FunctionCode 4

.EXAMPLE
    .\Scan-Modbus.ps1 -IpAddress 192.168.1.50 -TcpPort 8899 -StartAddress 1 -EndAddress 32

.EXAMPLE
    # Gateway speaking native Modbus TCP (typically port 502)
    .\Scan-Modbus.ps1 -IpAddress 10.10.0.201 -TcpPort 502 -Mbap -FunctionCode 3 -Register 0 -Quantity 8 -StartAddress 1 -EndAddress 20
#>

param(
    [string]$ComPort,
    [string]$IpAddress,
    [int]$TcpPort = 502,
    [switch]$Mbap,

    [int[]]$BaudRates = @(4800,9600,19200,38400,57600,115200,128000,256000),
    [ValidateRange(5,8)]
    [int]$DataBits = 8,
    [System.IO.Ports.Parity]$Parity = [System.IO.Ports.Parity]::None,
    [System.IO.Ports.StopBits]$StopBits = [System.IO.Ports.StopBits]::One,
    [System.IO.Ports.Handshake]$Handshake = [System.IO.Ports.Handshake]::None,

    [int]$StartAddress = 1,
    [int]$EndAddress = 10,

    [byte]$FunctionCode = 0x01,
    [int]$Register = 0,
    [int]$Quantity = 32,

    [int]$TimeoutMs = 150,

    # Waveshare Modbus RTU Relay/IO specific mode: sends a single broadcast
    # query (address 0, function 0x03, register 0x4000) to which the device
    # answers with its real address.
    [switch]$DiscoverAddress
)

. "$PSScriptRoot\ModbusCommon.ps1"

if (-not $ComPort -and -not $IpAddress) {
    Write-Error "Specify -ComPort (e.g. COM3) or -IpAddress (RS485<->Ethernet/WiFi converter)."
    exit 1
}
if ($ComPort -and $IpAddress) {
    Write-Error "Specify only one of -ComPort or -IpAddress, not both."
    exit 1
}
if ($Mbap -and $ComPort) {
    Write-Error "-Mbap only makes sense with -IpAddress (native Modbus TCP), not with -ComPort."
    exit 1
}
if ($IpAddress -and -not $PSBoundParameters.ContainsKey('TimeoutMs')) {
    # The 150 ms default is fine for RS485, but over the network (especially WiFi)
    # the round trip is longer - raise the default unless one was given explicitly.
    $TimeoutMs = 500
}

$found = @()
$totalAddrs = $EndAddress - $StartAddress + 1

# Over TCP the baud rate lives in the converter, so there is nothing to sweep -
# run a single "iteration" labelled with the connection instead of the baud list.
$iterations = if ($IpAddress) { @($null) } else { $BaudRates }
$totalCombos = $iterations.Count * $totalAddrs
$done = 0

if ($DiscoverAddress) {
    Write-Host "DiscoverAddress mode: broadcast (address 0, function 0x03, register 0x4000)" -ForegroundColor Cyan
}

foreach ($baud in $iterations) {
    if ($IpAddress) {
        Write-Host "=== Scanning via $IpAddress`:$TcpPort ===" -ForegroundColor Cyan
    } else {
        Write-Host "=== Trying baud rate: $baud ===" -ForegroundColor Cyan
    }

    try {
        $connection = Open-ModbusConnection -ComPort $ComPort -IpAddress $IpAddress -TcpPort $TcpPort -BaudRate $baud -DataBits $DataBits -Parity $Parity -StopBits $StopBits -Handshake $Handshake -TimeoutMs $TimeoutMs
    } catch {
        if ($IpAddress) {
            Write-Warning "Cannot connect to $IpAddress`:$TcpPort : $_"
        } else {
            Write-Warning "Cannot open $ComPort at $baud baud: $_"
        }
        $done += $totalAddrs
        continue
    }

    if ($DiscoverAddress) {
        $request = if ($Mbap) { Build-MbapReadRequest -UnitId 0 -FunctionCode 0x03 -Register 0x4000 -Quantity 1 } else { Build-ModbusRequest -SlaveId 0 -FunctionCode 0x03 -Register 0x4000 -Quantity 1 }
        try {
            $resp = Send-ModbusRequest -Connection $connection -Request $request -TimeoutMs $TimeoutMs
            $ok = if ($Mbap) { $resp -and $resp.Length -ge 9 } else { $resp -and $resp.Length -ge 5 -and (Test-ModbusCRC $resp) }
            if ($ok) {
                $discoveredAddr = if ($Mbap) { $resp[6] } else { $resp[0] }
                Write-Host "  FOUND -> $($connection.Label), device address: $discoveredAddr, response: $(Format-Hex $resp)" -ForegroundColor Green
                $found += [PSCustomObject]@{ BaudRate = $baud; SlaveId = $discoveredAddr; Response = (Format-Hex $resp) }
            }
        } catch { }

        Close-ModbusConnection -Connection $connection
        continue
    }

    for ($slaveId = $StartAddress; $slaveId -le $EndAddress; $slaveId++) {
        $done++
        $statusLabel = if ($IpAddress) { "address $slaveId" } else { "Baud $baud, address $slaveId" }
        Write-Progress -Activity "Modbus scan" -Status $statusLabel -PercentComplete (($done / $totalCombos) * 100)

        $request = if ($Mbap) { Build-MbapReadRequest -UnitId $slaveId -FunctionCode $FunctionCode -Register $Register -Quantity $Quantity } else { Build-ModbusRequest -SlaveId $slaveId -FunctionCode $FunctionCode -Register $Register -Quantity $Quantity }

        try {
            $resp = Send-ModbusRequest -Connection $connection -Request $request -TimeoutMs $TimeoutMs
            $ok = if ($Mbap) { Test-MbapResponse -Response $resp -UnitId $slaveId } else { $resp -and $resp.Length -ge 5 -and $resp[0] -eq $slaveId -and (Test-ModbusCRC $resp) }
            if ($ok) {
                Write-Host "  FOUND -> $($connection.Label), address: $slaveId, response: $(Format-Hex $resp)" -ForegroundColor Green
                $found += [PSCustomObject]@{ BaudRate = $baud; SlaveId = $slaveId; Response = (Format-Hex $resp) }
            }
        } catch { }
    }

    Close-ModbusConnection -Connection $connection
}

Write-Progress -Activity "Modbus scan" -Completed
Write-Host "`n=== Scan complete ===" -ForegroundColor Yellow
if ($found.Count -gt 0) {
    $found | Format-Table -AutoSize
} else {
    Write-Host "No Modbus devices found." -ForegroundColor Red
}
