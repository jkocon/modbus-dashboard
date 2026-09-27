<#
.SYNOPSIS
    Changes the baud rate of an R4DCB08 device (register 255, function 0x06 - Write Single Register).
    Works over a COM port (RS485) or over the network through an
    RS485<->Ethernet/WiFi converter (RTU over TCP, transparent mode).

.PARAMETER ComPort
    COM port name, e.g. COM3. Specify this OR IpAddress.

.PARAMETER IpAddress
    IP address of the RS485<->Ethernet/WiFi converter. Specify this OR ComPort.

.PARAMETER TcpPort
    TCP port of the converter in transparent mode (default 502).

.PARAMETER CurrentBaudRate
    The baud rate the device answers on NOW (used to open the COM port). COM only -
    over TCP the baud rate is configured in the converter (RS485 side) and must
    already match the device there.

.PARAMETER DataBits
    Data bits: 5-8 (default 8). COM only.

.PARAMETER Parity
    Parity: None/Even/Odd/Mark/Space (default None). COM only.

.PARAMETER StopBits
    Stop bits: One/OnePointFive/Two (default One). COM only.

.PARAMETER Handshake
    Flow control: None/XOnXOff/RequestToSend/RequestToSendXOnXOff (default None). COM only.

.PARAMETER SlaveId
    Modbus address of the device

.PARAMETER NewBaudRate
    Target baud rate: 1200, 2400, 4800, 9600 or 19200

.EXAMPLE
    .\Set-ModbusBaudRate.ps1 -ComPort COM3 -CurrentBaudRate 4800 -SlaveId 2 -NewBaudRate 9600

.EXAMPLE
    .\Set-ModbusBaudRate.ps1 -IpAddress 192.168.1.50 -TcpPort 8899 -SlaveId 2 -NewBaudRate 9600

.NOTES
    After the change the device may need a power cycle before it answers at the
    new speed. Afterwards reconnect with Scan-Modbus.ps1 or Read-Temperature.ps1
    using -BaudRate/-BaudRates set to the NEW value (over TCP, also update the
    baud rate in the converter's configuration).
#>

param(
    [string]$ComPort,
    [string]$IpAddress,
    [int]$TcpPort = 502,

    [int]$CurrentBaudRate = 9600,
    [ValidateRange(5,8)]
    [int]$DataBits = 8,
    [System.IO.Ports.Parity]$Parity = [System.IO.Ports.Parity]::None,
    [System.IO.Ports.StopBits]$StopBits = [System.IO.Ports.StopBits]::One,
    [System.IO.Ports.Handshake]$Handshake = [System.IO.Ports.Handshake]::None,

    [Parameter(Mandatory=$true)]
    [byte]$SlaveId,

    [Parameter(Mandatory=$true)]
    [ValidateSet(1200,2400,4800,9600,19200)]
    [int]$NewBaudRate,

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

$baudCodeMap = @{
    1200  = 0
    2400  = 1
    4800  = 2
    9600  = 3
    19200 = 4
}

$code = $baudCodeMap[$NewBaudRate]
$request = Build-WriteSingleRegister -SlaveId $SlaveId -Register 255 -Value $code
Write-Host "Sending (register 255 = $code -> $NewBaudRate baud): $(Format-Hex $request)" -ForegroundColor Cyan

try {
    $connection = Open-ModbusConnection -ComPort $ComPort -IpAddress $IpAddress -TcpPort $TcpPort -BaudRate $CurrentBaudRate -DataBits $DataBits -Parity $Parity -StopBits $StopBits -Handshake $Handshake -TimeoutMs $TimeoutMs
} catch {
    Write-Error "Cannot establish connection: $_"
    exit 1
}

try {
    $resp = Send-ModbusRequest -Connection $connection -Request $request -TimeoutMs $TimeoutMs

    if (-not $resp) {
        Write-Warning "No response. Check SlaveId/CurrentBaudRate or the connection."
        exit 1
    }

    Write-Host "Received: $(Format-Hex $resp)" -ForegroundColor Cyan

    if ($resp.Length -eq $request.Length -and (Compare-Object $resp $request -SyncWindow 0).Count -eq 0) {
        Write-Host "Success: baud rate changed to $NewBaudRate." -ForegroundColor Green
        Write-Host "If the device does not answer at the new baud rate, power-cycle it." -ForegroundColor Yellow
    } else {
        Write-Warning "Response is not the expected echo - verify that the change took effect (e.g. by scanning again at the new baud rate)."
    }
} finally {
    Close-ModbusConnection -Connection $connection
}
