<#
.SYNOPSIS
    Shared Modbus RTU functions: frame building (CRC16) and a transport layer
    that works over a COM port (RS485/SerialPort) as well as over the network
    (RTU over TCP - a transparent RS485<->Ethernet/WiFi converter such as
    Elfin EW11, USR-TCP232, Waveshare RS485 to ETH/WIFI).

    Modbus RTU frames (address + function + data + CRC16) are identical in both
    cases - only the way bytes are sent/received differs.

.NOTES
    Dot-source this file at the top of every script:
        . "$PSScriptRoot\ModbusCommon.ps1"
#>

function Get-ModbusCRC {
    param([byte[]]$Bytes)
    $crc = 0xFFFF
    foreach ($b in $Bytes) {
        $crc = $crc -bxor $b
        for ($i = 0; $i -lt 8; $i++) {
            if ($crc -band 0x0001) { $crc = ($crc -shr 1) -bxor 0xA001 }
            else { $crc = $crc -shr 1 }
        }
    }
    return $crc
}

function Format-Hex {
    param([byte[]]$Bytes)
    if (-not $Bytes) { return "" }
    return ($Bytes | ForEach-Object { $_.ToString("X2") }) -join ' '
}

function Build-ModbusRequest {
    param([byte]$SlaveId, [byte]$FunctionCode, [int]$Register, [int]$Quantity)
    $frame = [byte[]]@(
        $SlaveId, $FunctionCode,
        [byte](($Register -shr 8) -band 0xFF), [byte]($Register -band 0xFF),
        [byte](($Quantity -shr 8) -band 0xFF), [byte]($Quantity -band 0xFF)
    )
    $crc = Get-ModbusCRC -Bytes $frame
    return $frame + @([byte]($crc -band 0xFF), [byte](($crc -shr 8) -band 0xFF))
}

function Build-WriteSingleRegister {
    param([byte]$SlaveId, [int]$Register, [int]$Value)
    $frame = [byte[]]@(
        $SlaveId, 0x06,
        [byte](($Register -shr 8) -band 0xFF), [byte]($Register -band 0xFF),
        [byte](($Value -shr 8) -band 0xFF), [byte]($Value -band 0xFF)
    )
    $crc = Get-ModbusCRC -Bytes $frame
    return $frame + @([byte]($crc -band 0xFF), [byte](($crc -shr 8) -band 0xFF))
}

function Test-ModbusCRC {
    param([byte[]]$Response)
    if (-not $Response -or $Response.Length -lt 4) { return $false }
    $dataForCrc = $Response[0..($Response.Length - 3)]
    $calc = Get-ModbusCRC -Bytes $dataForCrc
    $lo = [byte]($calc -band 0xFF)
    $hi = [byte](($calc -shr 8) -band 0xFF)
    return ($Response[$Response.Length - 2] -eq $lo -and $Response[$Response.Length - 1] -eq $hi)
}

# ===================== Native Modbus TCP (MBAP) =====================
#
# Some gateways/converters do not tunnel raw RTU frames (RTU over TCP) but
# speak native Modbus TCP: a 7-byte MBAP header (Transaction Id, Protocol
# Id=0, Length, Unit Id) + PDU (function + data), with NO trailing CRC.
# Port 502 is the standard port for this mode - if a scan over -IpAddress on
# port 502 finds nothing, you probably need these functions instead of
# Build-ModbusRequest/Build-WriteSingleRegister.

function Build-MbapFrame {
    param([UInt16]$TransactionId = 1, [byte]$UnitId, [byte[]]$Pdu)
    $length = $Pdu.Length + 1
    $header = [byte[]]@(
        [byte](($TransactionId -shr 8) -band 0xFF), [byte]($TransactionId -band 0xFF),
        0x00, 0x00,
        [byte](($length -shr 8) -band 0xFF), [byte]($length -band 0xFF),
        $UnitId
    )
    return $header + $Pdu
}

function Build-MbapReadRequest {
    param([byte]$UnitId, [byte]$FunctionCode, [int]$Register, [int]$Quantity, [UInt16]$TransactionId = 1)
    $pdu = [byte[]]@(
        $FunctionCode,
        [byte](($Register -shr 8) -band 0xFF), [byte]($Register -band 0xFF),
        [byte](($Quantity -shr 8) -band 0xFF), [byte]($Quantity -band 0xFF)
    )
    return Build-MbapFrame -TransactionId $TransactionId -UnitId $UnitId -Pdu $pdu
}

function Build-MbapWriteSingleRegister {
    param([byte]$UnitId, [int]$Register, [int]$Value, [UInt16]$TransactionId = 1)
    $pdu = [byte[]]@(
        0x06,
        [byte](($Register -shr 8) -band 0xFF), [byte]($Register -band 0xFF),
        [byte](($Value -shr 8) -band 0xFF), [byte]($Value -band 0xFF)
    )
    return Build-MbapFrame -TransactionId $TransactionId -UnitId $UnitId -Pdu $pdu
}

function Test-MbapResponse {
    param([byte[]]$Response, [byte]$UnitId)
    if (-not $Response -or $Response.Length -lt 8) { return $false }
    if ($Response[6] -ne $UnitId) { return $false }
    $len = ($Response[4] -shl 8) -bor $Response[5]
    if ($Response.Length -lt (6 + $len)) { return $false }
    return $true
}

function Get-MbapPdu {
    param([byte[]]$Response)
    return $Response[7..($Response.Length - 1)]
}

<#
.SYNOPSIS
    Opens a Modbus connection - either a COM port (RS485) or a TCP socket to an
    RS485<->Ethernet/WiFi converter (transparent mode / RTU over TCP).

.PARAMETER ComPort
    COM port name, e.g. COM3. Mutually exclusive with IpAddress.

.PARAMETER IpAddress
    IP address of the RS485<->Ethernet/WiFi converter. Mutually exclusive with ComPort.

.PARAMETER TcpPort
    TCP port of the converter in transparent mode (device-dependent - often 502,
    8899, 4196 or 23 - check the converter's configuration). Default 502.

.PARAMETER BaudRate
    Baud rate - COM only. Over TCP the baud rate is configured in the converter
    itself (RS485 side) and cannot be changed from the TCP client.

.PARAMETER DataBits
    Data bits - COM only. Default 8 (standard for almost every Modbus RTU device).

.PARAMETER Parity
    Parity - COM only. None/Even/Odd/Mark/Space. Default None (matches most
    inexpensive RS485 modules such as R4DCB08/Waveshare), but the official Modbus
    RTU specification defaults to Even (8E1) - check your device's documentation
    if None does not work.

.PARAMETER StopBits
    Stop bits - COM only. One/OnePointFive/Two. Default One.

.PARAMETER Handshake
    Flow control - COM only. None/XOnXOff/RequestToSend/RequestToSendXOnXOff.
    Default None - Modbus RTU (master/slave polling, half-duplex) practically
    never uses flow control.
#>
function Open-ModbusConnection {
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
        [int]$TimeoutMs = 300
    )

    if ($IpAddress) {
        $client = New-Object System.Net.Sockets.TcpClient
        $client.ReceiveTimeout = $TimeoutMs
        $client.SendTimeout = $TimeoutMs
        $connectTask = $client.ConnectAsync($IpAddress, $TcpPort)
        if (-not $connectTask.Wait([Math]::Max($TimeoutMs, 1000))) {
            $client.Close()
            throw "Timeout while connecting to $IpAddress`:$TcpPort"
        }
        if ($connectTask.IsFaulted) {
            $client.Close()
            throw $connectTask.Exception.InnerException.Message
        }
        $stream = $client.GetStream()
        $stream.ReadTimeout = $TimeoutMs
        $stream.WriteTimeout = $TimeoutMs
        return [PSCustomObject]@{
            Type   = 'Tcp'
            Client = $client
            Stream = $stream
            Label  = "$IpAddress`:$TcpPort"
        }
    }
    elseif ($ComPort) {
        $stopBitsLabel = if ($StopBits -eq [System.IO.Ports.StopBits]::OnePointFive) { '1.5' } else { [int]$StopBits }
        $settingsLabel = "$BaudRate baud, $DataBits$($Parity.ToString().Substring(0,1))$stopBitsLabel, flow=$Handshake"

        $port = New-Object System.IO.Ports.SerialPort $ComPort, $BaudRate, $Parity, $DataBits, $StopBits
        $port.Handshake = $Handshake
        $port.ReadTimeout = $TimeoutMs
        $port.WriteTimeout = $TimeoutMs

        try {
            $port.Open()
        } catch {
            $port.Dispose()
            $innerMessage = $_.Exception.Message
            if ($innerMessage -match 'parameter is incorrect') {
                throw "The $ComPort driver rejected these settings ($settingsLabel) - Win32 code ERROR_INVALID_PARAMETER. This combination of data bits/parity/stop bits/flow control is most likely not supported by this USB-RS485 adapter (typically limited to 7-8 data bits and 1/2 stop bits). Try the default 8N1 with no flow control."
            }
            throw "Cannot open $ComPort ($settingsLabel): $innerMessage"
        }

        return [PSCustomObject]@{
            Type  = 'Serial'
            Port  = $port
            Label = "$ComPort @ $settingsLabel"
        }
    }
    else {
        throw "Specify -ComPort or -IpAddress."
    }
}

function Close-ModbusConnection {
    param($Connection)
    if (-not $Connection) { return }
    try {
        if ($Connection.Type -eq 'Tcp') {
            $Connection.Stream.Close()
            $Connection.Client.Close()
        } else {
            if ($Connection.Port.IsOpen) { $Connection.Port.Close() }
        }
    } catch { }
}

function Clear-ModbusBuffer {
    param($Connection)
    try {
        if ($Connection.Type -eq 'Tcp') {
            $tmp = New-Object byte[] 512
            while ($Connection.Client.Available -gt 0) {
                $Connection.Stream.Read($tmp, 0, [Math]::Min($tmp.Length, $Connection.Client.Available)) | Out-Null
            }
        } else {
            $Connection.Port.DiscardInBuffer()
        }
    } catch { }
}

<#
.SYNOPSIS
    Sends a Modbus frame and waits for the response - works identically for
    serial and TCP connections (the object returned by Open-ModbusConnection).
#>
function Send-ModbusRequest {
    param($Connection, [byte[]]$Request, [int]$TimeoutMs)

    Clear-ModbusBuffer -Connection $Connection

    if ($Connection.Type -eq 'Tcp') {
        $Connection.Stream.Write($Request, 0, $Request.Length)
    } else {
        $Connection.Port.Write($Request, 0, $Request.Length)
    }

    $buffer = New-Object byte[] 256
    $totalRead = 0
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)

    while ((Get-Date) -lt $deadline) {
        $bytesAvailable = if ($Connection.Type -eq 'Tcp') { $Connection.Client.Available } else { $Connection.Port.BytesToRead }

        if ($bytesAvailable -gt 0) {
            if ($Connection.Type -eq 'Tcp') {
                $n = $Connection.Stream.Read($buffer, $totalRead, $buffer.Length - $totalRead)
            } else {
                $n = $Connection.Port.Read($buffer, $totalRead, $buffer.Length - $totalRead)
            }
            $totalRead += $n
            Start-Sleep -Milliseconds 5
        } elseif ($totalRead -gt 0) {
            break
        } else {
            Start-Sleep -Milliseconds 5
        }
    }

    if ($totalRead -eq 0) { return $null }
    return $buffer[0..($totalRead - 1)]
}
