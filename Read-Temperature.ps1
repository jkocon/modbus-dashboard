<#
.SYNOPSIS
    Reads temperatures from an R4DCB08 module (8-channel DS18B20, RS485 Modbus RTU).
    Works over a COM port (RS485) or over the network through an
    RS485<->Ethernet/WiFi converter (RTU over TCP, transparent mode).

.PARAMETER ComPort
    COM port name, e.g. COM3. Specify this OR IpAddress.

.PARAMETER IpAddress
    IP address of the RS485<->Ethernet/WiFi converter. Specify this OR ComPort.

.PARAMETER TcpPort
    TCP port of the converter in transparent mode (default 502 - check your
    converter's configuration; 8899/4196/23 are also common).

.PARAMETER BaudRate
    Baud rate (default 9600). COM only - over TCP the baud rate is configured
    in the converter (RS485 side).

.PARAMETER DataBits
    Data bits: 5-8 (default 8). COM only.

.PARAMETER Parity
    Parity: None/Even/Odd/Mark/Space (default None). COM only.
    The official Modbus RTU specification defaults to Even (8E1) - check the
    device's documentation if the default None does not work.

.PARAMETER StopBits
    Stop bits: One/OnePointFive/Two (default One). COM only.

.PARAMETER Handshake
    Flow control: None/XOnXOff/RequestToSend/RequestToSendXOnXOff (default None). COM only.

.PARAMETER SlaveId
    Modbus address of the device (default 1)

.PARAMETER Loop
    If set, the read repeats every IntervalSeconds until Ctrl+C

.PARAMETER IntervalSeconds
    Interval between reads in -Loop mode (default 2 s)

.EXAMPLE
    .\Read-Temperature.ps1 -ComPort COM3

.EXAMPLE
    .\Read-Temperature.ps1 -ComPort COM3 -SlaveId 1 -Loop -IntervalSeconds 5

.EXAMPLE
    .\Read-Temperature.ps1 -IpAddress 192.168.1.50 -TcpPort 8899 -SlaveId 1 -Loop
#>

param(
    [string]$ComPort,
    [string]$IpAddress,
    [int]$TcpPort = 502,

    [int]$BaudRate = 9600,
    [ValidateRange(5,8)]
    [int]$DataBits = 8,
    [System.IO.Ports.Parity]$Parity = [System.IO.Ports.Parity]::None,
    [System.IO.Ports.StopBits]$StopBits = [System.IO.Ports.StopBits]::One,
    [System.IO.Ports.Handshake]$Handshake = [System.IO.Ports.Handshake]::None,
    [byte]$SlaveId = 1,

    [switch]$Loop,
    [int]$IntervalSeconds = 2,

    [int]$TimeoutMs = 300
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

function Read-Temperatures {
    param($Connection, [byte]$SlaveId, [int]$TimeoutMs)

    $request = Build-ModbusRequest -SlaveId $SlaveId -FunctionCode 0x03 -Register 0 -Quantity 8
    $resp = Send-ModbusRequest -Connection $Connection -Request $request -TimeoutMs $TimeoutMs

    # Expected length: address(1) + function(1) + byte count(1) + 16 data bytes + CRC(2) = 21
    if (-not $resp -or $resp.Length -lt 21) {
        throw "Response too short ($($resp.Length) bytes) - no response or a corrupted one from the device."
    }
    if (-not (Test-ModbusCRC $resp)) {
        throw "CRC error in response."
    }
    if ($resp[0] -ne $SlaveId -or $resp[1] -ne 0x03) {
        throw "Unexpected response (address/function mismatch)."
    }

    $byteCount = $resp[2]
    $dataBytes = $resp[3..(3 + $byteCount - 1)]

    $results = @()
    for ($ch = 0; $ch -lt 8; $ch++) {
        $hi = $dataBytes[$ch * 2]
        $lo = $dataBytes[$ch * 2 + 1]
        $raw = ($hi -shl 8) -bor $lo

        if ($raw -eq 0x8000) {
            $results += [PSCustomObject]@{ Channel = $ch + 1; TemperatureC = $null; Status = "No sensor" }
        } else {
            if ($raw -gt 32767) { $raw = $raw - 65536 }
            $tempC = [math]::Round($raw / 10.0, 1)
            $results += [PSCustomObject]@{ Channel = $ch + 1; TemperatureC = $tempC; Status = "OK" }
        }
    }

    return $results
}

try {
    $connection = Open-ModbusConnection -ComPort $ComPort -IpAddress $IpAddress -TcpPort $TcpPort -BaudRate $BaudRate -DataBits $DataBits -Parity $Parity -StopBits $StopBits -Handshake $Handshake -TimeoutMs $TimeoutMs
} catch {
    Write-Error "Cannot establish connection: $_"
    exit 1
}

try {
    do {
        try {
            $temps = Read-Temperatures -Connection $connection -SlaveId $SlaveId -TimeoutMs $TimeoutMs
            Write-Host "`n[$(Get-Date -Format 'HH:mm:ss')] Device $SlaveId @ $($connection.Label):" -ForegroundColor Cyan
            $temps | Format-Table -AutoSize
        } catch {
            Write-Warning "Read error: $_"
        }

        if ($Loop) {
            Start-Sleep -Seconds $IntervalSeconds
        }
    } while ($Loop)
} finally {
    Close-ModbusConnection -Connection $connection
}
