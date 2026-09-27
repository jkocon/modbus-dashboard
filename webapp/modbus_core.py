"""
Shared Modbus RTU logic: frame building (CRC16, RTU and MBAP/Modbus TCP)
and the transport layer - a serial port (RS485 via pyserial, works on
Windows/Linux/macOS) or a TCP socket (RTU over TCP, or native Modbus TCP
with an MBAP header).

Used by modbus_dashboard.py (HTTP server + API).
"""

from __future__ import annotations

import socket
import struct
import time
from typing import List, Optional


# ===================== CRC16 / RTU frames =====================

def modbus_crc(data: bytes) -> int:
    crc = 0xFFFF
    for b in data:
        crc ^= b
        for _ in range(8):
            if crc & 0x0001:
                crc = (crc >> 1) ^ 0xA001
            else:
                crc >>= 1
    return crc & 0xFFFF


def build_rtu_request(slave_id: int, function_code: int, register: int, quantity: int) -> bytes:
    frame = bytes([
        slave_id & 0xFF, function_code & 0xFF,
        (register >> 8) & 0xFF, register & 0xFF,
        (quantity >> 8) & 0xFF, quantity & 0xFF,
    ])
    crc = modbus_crc(frame)
    return frame + bytes([crc & 0xFF, (crc >> 8) & 0xFF])


def build_rtu_write_single(slave_id: int, register: int, value: int) -> bytes:
    frame = bytes([
        slave_id & 0xFF, 0x06,
        (register >> 8) & 0xFF, register & 0xFF,
        (value >> 8) & 0xFF, value & 0xFF,
    ])
    crc = modbus_crc(frame)
    return frame + bytes([crc & 0xFF, (crc >> 8) & 0xFF])


def test_rtu_crc(resp: bytes) -> bool:
    if not resp or len(resp) < 4:
        return False
    data = resp[:-2]
    crc = modbus_crc(data)
    lo, hi = crc & 0xFF, (crc >> 8) & 0xFF
    return resp[-2] == lo and resp[-1] == hi


# ===================== Native Modbus TCP (MBAP) =====================
#
# 7-byte header: Transaction Id(2) + Protocol Id(2, =0) + Length(2) +
# Unit Id(1), followed by the PDU (function + data) with NO CRC. Standard port 502.

def build_mbap_frame(unit_id: int, pdu: bytes, transaction_id: int = 1) -> bytes:
    length = len(pdu) + 1
    header = struct.pack(">HHHB", transaction_id & 0xFFFF, 0, length & 0xFFFF, unit_id & 0xFF)
    return header + pdu


def build_mbap_read(unit_id: int, function_code: int, register: int, quantity: int,
                     transaction_id: int = 1) -> bytes:
    pdu = bytes([
        function_code & 0xFF,
        (register >> 8) & 0xFF, register & 0xFF,
        (quantity >> 8) & 0xFF, quantity & 0xFF,
    ])
    return build_mbap_frame(unit_id, pdu, transaction_id)


def build_mbap_write_single(unit_id: int, register: int, value: int, transaction_id: int = 1) -> bytes:
    pdu = bytes([
        0x06,
        (register >> 8) & 0xFF, register & 0xFF,
        (value >> 8) & 0xFF, value & 0xFF,
    ])
    return build_mbap_frame(unit_id, pdu, transaction_id)


def test_mbap_response(resp: bytes, unit_id: int) -> bool:
    if not resp or len(resp) < 8:
        return False
    if resp[6] != (unit_id & 0xFF):
        return False
    length = (resp[4] << 8) | resp[5]
    if len(resp) < 6 + length:
        return False
    return True


def mbap_pdu(resp: bytes) -> bytes:
    return resp[7:]


def format_hex(data: Optional[bytes]) -> str:
    if not data:
        return ""
    return " ".join(f"{b:02X}" for b in data)


# ===================== Transport: serial (COM/RS485) and TCP =====================

class ModbusConnection:
    def send_request(self, request: bytes, timeout_ms: int) -> bytes:
        raise NotImplementedError

    def close(self) -> None:
        raise NotImplementedError


DATA_BITS_MAP = {5: "FIVEBITS", 6: "SIXBITS", 7: "SEVENBITS", 8: "EIGHTBITS"}
PARITY_MAP = {"None": "PARITY_NONE", "Even": "PARITY_EVEN", "Odd": "PARITY_ODD",
              "Mark": "PARITY_MARK", "Space": "PARITY_SPACE"}
STOP_BITS_MAP = {"One": "STOPBITS_ONE", "OnePointFive": "STOPBITS_ONE_POINT_FIVE", "Two": "STOPBITS_TWO"}
# pyserial has no single "Handshake" enum like .NET SerialPort - these are three independent flags.
FLOW_CONTROL_MAP = {
    "None": {"rtscts": False, "dsrdtr": False, "xonxoff": False},
    "RtsCts": {"rtscts": True, "dsrdtr": False, "xonxoff": False},
    "DsrDtr": {"rtscts": False, "dsrdtr": True, "xonxoff": False},
    "XonXoff": {"rtscts": False, "dsrdtr": False, "xonxoff": True},
}


class SerialConnection(ModbusConnection):
    def __init__(self, port: str, baud: int, timeout_ms: int,
                 data_bits: int = 8, parity: str = "None", stop_bits: str = "One",
                 flow_control: str = "None"):
        try:
            import serial  # pyserial
        except ImportError as e:
            raise RuntimeError(
                "Missing pyserial package. Install: pip install pyserial"
            ) from e

        if data_bits not in DATA_BITS_MAP:
            raise ValueError(f"Unsupported data bits: {data_bits}. Allowed: {sorted(DATA_BITS_MAP)}.")
        if parity not in PARITY_MAP:
            raise ValueError(f"Unknown parity: {parity}. Allowed: {sorted(PARITY_MAP)}.")
        if stop_bits not in STOP_BITS_MAP:
            raise ValueError(f"Unknown stop bits: {stop_bits}. Allowed: {sorted(STOP_BITS_MAP)}.")
        if flow_control not in FLOW_CONTROL_MAP:
            raise ValueError(f"Unknown flow control: {flow_control}. Allowed: {sorted(FLOW_CONTROL_MAP)}.")

        stop_bits_label = {"One": "1", "OnePointFive": "1.5", "Two": "2"}[stop_bits]
        settings_label = f"{baud} baud, {data_bits}{parity[0]}{stop_bits_label}, flow={flow_control}"

        try:
            self.ser = serial.Serial(
                port=port,
                baudrate=baud,
                bytesize=getattr(serial, DATA_BITS_MAP[data_bits]),
                parity=getattr(serial, PARITY_MAP[parity]),
                stopbits=getattr(serial, STOP_BITS_MAP[stop_bits]),
                timeout=timeout_ms / 1000.0,
                write_timeout=timeout_ms / 1000.0,
                **FLOW_CONTROL_MAP[flow_control],
            )
        except serial.SerialException as e:
            if "parameter is incorrect" in str(e).lower():
                raise RuntimeError(
                    f"The {port} driver rejected these settings ({settings_label}) - Win32 code "
                    "ERROR_INVALID_PARAMETER. This combination of data bits/parity/stop bits/"
                    "flow control is most likely not supported by this USB-RS485 adapter "
                    "(typically limited to 7-8 data bits and 1/2 stop bits). "
                    "Try the default 8N1 with no flow control."
                ) from e
            raise RuntimeError(f"Cannot open {port} ({settings_label}): {e}") from e

    def send_request(self, request: bytes, timeout_ms: int) -> bytes:
        self.ser.reset_input_buffer()
        self.ser.write(request)

        buf = bytearray()
        deadline = time.monotonic() + timeout_ms / 1000.0
        while time.monotonic() < deadline:
            n = self.ser.in_waiting
            if n > 0:
                buf += self.ser.read(n)
                time.sleep(0.005)
            elif buf:
                break
            else:
                time.sleep(0.005)
        return bytes(buf)

    def close(self) -> None:
        try:
            self.ser.close()
        except Exception:
            pass


class TcpConnection(ModbusConnection):
    def __init__(self, ip: str, port: int, timeout_ms: int):
        self.sock = socket.create_connection((ip, port), timeout=max(timeout_ms, 1000) / 1000.0)

    def _drain(self) -> None:
        self.sock.settimeout(0.0)
        try:
            while True:
                chunk = self.sock.recv(512)
                if not chunk:
                    break
        except BlockingIOError:
            pass
        except OSError:
            pass

    def send_request(self, request: bytes, timeout_ms: int) -> bytes:
        self._drain()
        self.sock.settimeout(timeout_ms / 1000.0)
        self.sock.sendall(request)

        buf = bytearray()
        deadline = time.monotonic() + timeout_ms / 1000.0
        self.sock.settimeout(0.02)
        while time.monotonic() < deadline:
            try:
                chunk = self.sock.recv(512)
            except socket.timeout:
                if buf:
                    break
                continue
            except OSError:
                break
            if chunk:
                buf += chunk
            else:
                break
        return bytes(buf)

    def close(self) -> None:
        try:
            self.sock.close()
        except Exception:
            pass


def list_serial_ports() -> List[str]:
    try:
        from serial.tools import list_ports
    except ImportError as e:
        raise RuntimeError(
            "Missing pyserial package. Install: pip install pyserial"
        ) from e
    return sorted(p.device for p in list_ports.comports())


def open_connection(conn: dict) -> ModbusConnection:
    """conn: {"transport": "serial"|"tcp", "port"/"baud"/"dataBits"/"parity"/"stopBits"/"flowControl"
    or "ip"/"tcpPort", "timeoutMs"}"""
    timeout_ms = int(conn.get("timeoutMs", 300))
    transport = conn.get("transport")

    if transport == "serial":
        port = conn.get("port")
        if not port:
            raise ValueError("Select a COM port.")
        baud = int(conn.get("baud", 9600))
        data_bits = int(conn.get("dataBits", 8))
        parity = conn.get("parity", "None")
        stop_bits = conn.get("stopBits", "One")
        flow_control = conn.get("flowControl", "None")
        return SerialConnection(port, baud, timeout_ms, data_bits, parity, stop_bits, flow_control)

    if transport == "tcp":
        ip = conn.get("ip")
        if not ip:
            raise ValueError("Enter the converter's IP address.")
        tcp_port = int(conn.get("tcpPort", 502))
        return TcpConnection(ip, tcp_port, timeout_ms)

    raise ValueError("Unknown connection type (transport must be 'serial' or 'tcp').")


# ===================== High-level operations =====================

BAUD_CODE_MAP = {1200: 0, 2400: 1, 4800: 2, 9600: 3, 19200: 4}


def read_r4dcb08(conn: ModbusConnection, slave_id: int, mbap: bool, timeout_ms: int) -> list:
    """Read the 8 temperature channels of an R4DCB08 module (Read Holding Registers, register 0)."""
    if mbap:
        request = build_mbap_read(slave_id, 0x03, 0, 8)
        resp = conn.send_request(request, timeout_ms)
        if not test_mbap_response(resp, slave_id):
            raise ValueError("No or invalid response (MBAP).")
        pdu = mbap_pdu(resp)
        if not pdu or pdu[0] != 0x03:
            raise ValueError("Unexpected response (function code mismatch).")
        byte_count = pdu[1]
        data = pdu[2:2 + byte_count]
    else:
        request = build_rtu_request(slave_id, 0x03, 0, 8)
        resp = conn.send_request(request, timeout_ms)
        if not resp or len(resp) < 21:
            raise ValueError(f"Response too short ({len(resp)} bytes) - no response or a corrupted one from the device.")
        if not test_rtu_crc(resp):
            raise ValueError("CRC error in response.")
        if resp[0] != slave_id or resp[1] != 0x03:
            raise ValueError("Unexpected response (address/function mismatch).")
        byte_count = resp[2]
        data = resp[3:3 + byte_count]

    if len(data) < 16:
        raise ValueError("Not enough data in response.")

    results = []
    for ch in range(8):
        raw = (data[ch * 2] << 8) | data[ch * 2 + 1]
        if raw == 0x8000:
            results.append({"channel": ch + 1, "temperatureC": None, "status": "No sensor"})
        else:
            if raw > 32767:
                raw -= 65536
            results.append({"channel": ch + 1, "temperatureC": round(raw / 10.0, 1), "status": "OK"})
    return results


# ===================== Coils (relays): 0x01 / 0x05 / 0x0F =====================

MAX_COILS = 256  # per request; the Modbus limit is higher, but this keeps the UI grid sane


def pack_coils(values: list) -> bytes:
    """Pack booleans into Modbus coil bytes (coil 0 = LSB of byte 0)."""
    out = bytearray((len(values) + 7) // 8)
    for i, v in enumerate(values):
        if v:
            out[i // 8] |= 1 << (i % 8)
    return bytes(out)


def unpack_coils(data: bytes, quantity: int) -> list:
    return [bool((data[i // 8] >> (i % 8)) & 1) for i in range(quantity)]


def _write_single_coil_pdu(coil: int, on: bool) -> bytes:
    return bytes([0x05, (coil >> 8) & 0xFF, coil & 0xFF, 0xFF if on else 0x00, 0x00])


def _write_multiple_coils_pdu(start: int, values: list) -> bytes:
    packed = pack_coils(values)
    return bytes([
        0x0F,
        (start >> 8) & 0xFF, start & 0xFF,
        (len(values) >> 8) & 0xFF, len(values) & 0xFF,
        len(packed),
    ]) + packed


def _with_crc(frame: bytes) -> bytes:
    crc = modbus_crc(frame)
    return frame + bytes([crc & 0xFF, (crc >> 8) & 0xFF])


def build_rtu_write_single_coil(slave_id: int, coil: int, on: bool) -> bytes:
    return _with_crc(bytes([slave_id & 0xFF]) + _write_single_coil_pdu(coil, on))


def build_rtu_write_multiple_coils(slave_id: int, start: int, values: list) -> bytes:
    return _with_crc(bytes([slave_id & 0xFF]) + _write_multiple_coils_pdu(start, values))


def build_mbap_write_single_coil(unit_id: int, coil: int, on: bool, transaction_id: int = 1) -> bytes:
    return build_mbap_frame(unit_id, _write_single_coil_pdu(coil, on), transaction_id)


def build_mbap_write_multiple_coils(unit_id: int, start: int, values: list, transaction_id: int = 1) -> bytes:
    return build_mbap_frame(unit_id, _write_multiple_coils_pdu(start, values), transaction_id)


def _raise_if_exception(function_code: int, pdu: bytes) -> None:
    if pdu and pdu[0] == (function_code | 0x80):
        code = pdu[1] if len(pdu) > 1 else "?"
        raise ValueError(f"Device returned Modbus exception code {code} for function 0x{function_code:02X}.")


def read_coils(conn: ModbusConnection, slave_id: int, start: int, quantity: int,
               mbap: bool, timeout_ms: int) -> list:
    """Read Coils (0x01). Returns a list of booleans, one per coil."""
    if not 1 <= quantity <= MAX_COILS:
        raise ValueError(f"quantity must be between 1 and {MAX_COILS}.")
    expected_bytes = (quantity + 7) // 8

    if mbap:
        resp = conn.send_request(build_mbap_read(slave_id, 0x01, start, quantity), timeout_ms)
        if not test_mbap_response(resp, slave_id):
            raise ValueError("No or invalid response (MBAP).")
        pdu = mbap_pdu(resp)
    else:
        resp = conn.send_request(build_rtu_request(slave_id, 0x01, start, quantity), timeout_ms)
        if not resp or len(resp) < 5:
            raise ValueError(f"Response too short ({len(resp)} bytes) - no response or a corrupted one from the device.")
        if not test_rtu_crc(resp):
            raise ValueError("CRC error in response.")
        if resp[0] != slave_id:
            raise ValueError("Unexpected response (address mismatch).")
        pdu = resp[1:-2]

    _raise_if_exception(0x01, pdu)
    if pdu[0] != 0x01:
        raise ValueError("Unexpected response (function code mismatch).")
    byte_count = pdu[1]
    data = pdu[2:2 + byte_count]
    if len(data) < expected_bytes:
        raise ValueError("Not enough data in response.")
    return unpack_coils(data, quantity)


def write_single_coil(conn: ModbusConnection, unit_id: int, coil: int, on: bool,
                      mbap: bool, timeout_ms: int):
    """Write Single Coil (0x05). Success = the device echoes the request. Returns (request, response, success)."""
    request = (build_mbap_write_single_coil(unit_id, coil, on) if mbap
               else build_rtu_write_single_coil(unit_id, coil, on))
    resp = conn.send_request(request, timeout_ms)
    return request, resp, bool(resp) and resp == request


def write_multiple_coils(conn: ModbusConnection, unit_id: int, start: int, values: list,
                         mbap: bool, timeout_ms: int):
    """Write Multiple Coils (0x0F). Success = the device acknowledges start+quantity. Returns (request, response, success)."""
    if not 1 <= len(values) <= MAX_COILS:
        raise ValueError(f"values must contain between 1 and {MAX_COILS} coils.")
    request = (build_mbap_write_multiple_coils(unit_id, start, values) if mbap
               else build_rtu_write_multiple_coils(unit_id, start, values))
    resp = conn.send_request(request, timeout_ms)

    if mbap:
        success = test_mbap_response(resp, unit_id) and mbap_pdu(resp)[:5] == request[7:12]
    else:
        success = (bool(resp) and len(resp) >= 8 and test_rtu_crc(resp) and resp[:6] == request[:6])
    return request, resp, success


def write_single_register(conn: ModbusConnection, unit_id: int, register: int, value: int,
                           mbap: bool, timeout_ms: int):
    """Send Write Single Register (function 0x06) and check the echo. Returns (request, response, success)."""
    if mbap:
        request = build_mbap_write_single(unit_id, register, value)
    else:
        request = build_rtu_write_single(unit_id, register, value)

    resp = conn.send_request(request, timeout_ms)
    success = bool(resp) and resp == request
    return request, resp, success
