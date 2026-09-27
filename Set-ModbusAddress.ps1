<#
.SYNOPSIS
    Changes the RS485/Modbus address of an R4DCB08 (and similar devices that use
    register 254 for address configuration, function 0x06 - Write Single Register).
    Works over a COM port (RS485) or over the network through an
    RS485<->Ethernet/WiFi converter (RTU over TCP, transparent mode).

.PARAMETER ComPort
    COM port name, e.g. COM3. Specify this OR IpAddress.

.PARAMETER IpAddress
    IP address of the RS485<->Ethernet/WiFi converter. Specify this OR ComPort.

.PARAMETER TcpPort
    TCP port of the converter in transparent mode (default 502).

.PARAMETER BaudRate
    The device's current baud rate (default 9600). COM only - over TCP the baud
    rate is configured in the converter.

.PARAMETER DataBits
    Data bits: 5-8 (default 8). COM only.

.PARAMETER Parity
    Parity: None/Even/Odd/Mark/Space (default None). COM only.

.PARAMETER StopBits
    Stop bits: One/OnePointFive/Two (default One). COM only.

.PARAMETER Handshake
    Flow control: None/XOnXOff/RequestToSend/RequestToSendXOnXOff (default None). COM only.

.PARAMETER OldAddress
    The device's current Modbus address (the one it answers on now)

.PARAMETER NewAddress
    New Modbus address to set (1-247)

.EXAMPLE
    .\Set-ModbusAddress.ps1 -ComPort COM3 -OldAddress 3 -NewAddress 4

.EXAMPLE
    .\Set-ModbusAddress.ps1 -IpAddress 192.168.1.50 -TcpPort 8899 -OldAddress 3 -NewAddress 4

.NOTES
    IMPORTANT: if TWO devices with the same address (OldAddress) sit on the same
    bus, this command changes the address of BOTH at once (both answer to the same
    address) and the collision is not resolved. To split colliding addresses
    safely, connect the devices to the bus ONE AT A TIME, give each a unique
    address, and only then connect them together.
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

    [Parameter(Mandatory=$true)]
    [ValidateRange(0,247)]
    [byte]$OldAddress,

    [Parameter(Mandatory=$true)]
    [ValidateRange(1,247)]
    [byte]$NewAddress,

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

if ($OldAddress -eq 0) {
    Write-Warning "OldAddress = 0 (broadcast) will change the address of EVERY device on the bus to $NewAddress. Continue? (Ctrl+C to abort, Enter to continue)"
    Read-Host | Out-Null
}

$request = Build-WriteSingleRegister -SlaveId $OldAddress -Register 254 -Value $NewAddress
Write-Host "Sending: $(Format-Hex $request)" -ForegroundColor Cyan

try {
    $connection = Open-ModbusConnection -ComPort $ComPort -IpAddress $IpAddress -TcpPort $TcpPort -BaudRate $BaudRate -DataBits $DataBits -Parity $Parity -StopBits $StopBits -Handshake $Handshake -TimeoutMs $TimeoutMs
} catch {
    Write-Error "Cannot establish connection: $_"
    exit 1
}

try {
    $resp = Send-ModbusRequest -Connection $connection -Request $request -TimeoutMs $TimeoutMs

    if (-not $resp) {
        Write-Warning "No response. Check OldAddress/BaudRate or the connection."
        exit 1
    }

    Write-Host "Received: $(Format-Hex $resp)" -ForegroundColor Cyan

    # On a successful Write Single Register the device echoes the exact same frame
    if ($resp.Length -eq $request.Length -and (Compare-Object $resp $request -SyncWindow 0).Count -eq 0) {
        Write-Host "Success: address changed from $OldAddress to $NewAddress." -ForegroundColor Green
    } else {
        Write-Warning "Response is not the expected echo - verify that the change took effect (e.g. by scanning again)."
    }
} finally {
    Close-ModbusConnection -Connection $connection
}
