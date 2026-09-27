"""Unit tests for the Modbus framing/parsing logic (no hardware required).

Run from the webapp/ directory:
    python -m unittest discover -s tests -v
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import modbus_core as mc  # noqa: E402


class FakeConnection(mc.ModbusConnection):
    """Returns a canned response and records what was sent."""

    def __init__(self, response: bytes):
        self.response = response
        self.sent = None

    def send_request(self, request: bytes, timeout_ms: int) -> bytes:
        self.sent = request
        return self.response

    def close(self) -> None:
        pass


class CrcTests(unittest.TestCase):
    def test_known_vector(self):
        # Classic Modbus reference: 01 03 00 00 00 08 -> CRC 44 0C (low byte first on the wire)
        self.assertEqual(mc.modbus_crc(bytes.fromhex("010300000008")), 0x0C44)

    def test_rtu_request_layout_and_crc(self):
        frame = mc.build_rtu_request(1, 0x03, 0, 8)
        self.assertEqual(frame, bytes.fromhex("010300000008440C"))
        self.assertTrue(mc.test_rtu_crc(frame))

    def test_crc_rejects_corruption(self):
        frame = bytearray(mc.build_rtu_request(1, 0x03, 0, 8))
        frame[3] ^= 0xFF
        self.assertFalse(mc.test_rtu_crc(bytes(frame)))
        self.assertFalse(mc.test_rtu_crc(b""))
        self.assertFalse(mc.test_rtu_crc(b"\x01\x03"))

    def test_write_single_register_frame(self):
        frame = mc.build_rtu_write_single(3, 254, 2)
        self.assertEqual(frame[:6], bytes([3, 0x06, 0x00, 0xFE, 0x00, 0x02]))
        self.assertTrue(mc.test_rtu_crc(frame))


class MbapTests(unittest.TestCase):
    def test_read_frame_layout(self):
        frame = mc.build_mbap_read(unit_id=7, function_code=0x03, register=0x0010, quantity=2, transaction_id=0x1234)
        # txn(2) proto(2) length(2)=6 unit(1) fc reg(2) qty(2) -> 12 bytes, no CRC
        self.assertEqual(frame, bytes.fromhex("1234 0000 0006 07 03 0010 0002".replace(" ", "")))

    def test_response_validation(self):
        good = bytes.fromhex("0001 0000 0005 07 03 02 00 2A".replace(" ", ""))
        self.assertTrue(mc.test_mbap_response(good, 7))
        self.assertFalse(mc.test_mbap_response(good, 8))          # unit id mismatch
        self.assertFalse(mc.test_mbap_response(good[:-1], 7))     # shorter than declared length
        self.assertFalse(mc.test_mbap_response(b"", 7))
        self.assertEqual(mc.mbap_pdu(good), bytes.fromhex("0302002A"))


class R4DCB08Tests(unittest.TestCase):
    def _rtu_response(self, values: list[int]) -> bytes:
        body = bytes([1, 0x03, 16]) + b"".join(v.to_bytes(2, "big", signed=False) for v in values)
        crc = mc.modbus_crc(body)
        return body + bytes([crc & 0xFF, (crc >> 8) & 0xFF])

    def test_parses_temperatures_and_missing_sensor(self):
        # 23.4 C, -5.0 C (signed), no sensor (0x8000), then five zeros
        values = [234, (-50) & 0xFFFF, 0x8000, 0, 0, 0, 0, 0]
        conn = FakeConnection(self._rtu_response(values))
        results = mc.read_r4dcb08(conn, slave_id=1, mbap=False, timeout_ms=100)

        self.assertEqual(conn.sent, mc.build_rtu_request(1, 0x03, 0, 8))
        self.assertEqual(results[0], {"channel": 1, "temperatureC": 23.4, "status": "OK"})
        self.assertEqual(results[1]["temperatureC"], -5.0)
        self.assertEqual(results[2], {"channel": 3, "temperatureC": None, "status": "No sensor"})
        self.assertEqual(len(results), 8)

    def test_rejects_bad_crc_and_short_response(self):
        good = self._rtu_response([0] * 8)
        with self.assertRaises(ValueError):
            mc.read_r4dcb08(FakeConnection(good[:-1] + b"\x00"), 1, False, 100)  # bad CRC
        with self.assertRaises(ValueError):
            mc.read_r4dcb08(FakeConnection(b"\x01\x03"), 1, False, 100)  # too short
        with self.assertRaises(ValueError):
            mc.read_r4dcb08(FakeConnection(b""), 1, False, 100)  # no response

    def test_mbap_path(self):
        pdu = bytes([0x03, 16]) + (234).to_bytes(2, "big") + bytes(14)
        resp = mc.build_mbap_frame(unit_id=5, pdu=pdu)
        results = mc.read_r4dcb08(FakeConnection(resp), slave_id=5, mbap=True, timeout_ms=100)
        self.assertEqual(results[0]["temperatureC"], 23.4)


class WriteRegisterTests(unittest.TestCase):
    def test_echo_means_success(self):
        request = mc.build_rtu_write_single(1, 254, 9)
        req, resp, ok = mc.write_single_register(FakeConnection(request), 1, 254, 9, mbap=False, timeout_ms=100)
        self.assertEqual(req, request)
        self.assertTrue(ok)

    def test_no_or_different_response_is_failure(self):
        _, _, ok = mc.write_single_register(FakeConnection(b""), 1, 254, 9, mbap=False, timeout_ms=100)
        self.assertFalse(ok)
        _, _, ok = mc.write_single_register(FakeConnection(b"\x01\x86\x02\xc2\xc1"), 1, 254, 9, mbap=False, timeout_ms=100)
        self.assertFalse(ok)


class CoilTests(unittest.TestCase):
    """Vectors from the Modbus Application Protocol specification examples."""

    def test_pack_unpack_round_trip_lsb_first(self):
        values = [True, False, True, True, False, False, True, True, True]  # 9 coils -> 2 bytes
        packed = mc.pack_coils(values)
        self.assertEqual(packed, bytes([0xCD, 0x01]))
        self.assertEqual(mc.unpack_coils(packed, 9), values)

    def test_write_multiple_coils_spec_frame(self):
        # Spec: slave 0x11, start 19, 10 coils, data CD 01 -> CRC BF 0B
        values = mc.unpack_coils(bytes([0xCD, 0x01]), 10)
        frame = mc.build_rtu_write_multiple_coils(0x11, 19, values)
        self.assertEqual(frame, bytes.fromhex("110F0013000A02CD01BF0B"))

    def test_write_single_coil_frame(self):
        self.assertEqual(mc.build_rtu_write_single_coil(1, 3, True)[:6], bytes.fromhex("01050003FF00"))
        self.assertEqual(mc.build_rtu_write_single_coil(1, 3, False)[:6], bytes.fromhex("010500030000"))
        self.assertTrue(mc.test_rtu_crc(mc.build_rtu_write_single_coil(1, 3, True)))

    def test_read_coils_spec_response(self):
        # Spec: slave 0x11 Read Coils 20..56 -> 11 01 05 CD 6B B2 0E 1B + CRC
        resp = bytes.fromhex("110105CD6BB20E1B")
        resp = mc._with_crc(resp)
        coils = mc.read_coils(FakeConnection(resp), 0x11, 19, 37, mbap=False, timeout_ms=100)
        self.assertEqual(len(coils), 37)
        self.assertEqual(coils[:8], [True, False, True, True, False, False, True, True])  # 0xCD
        self.assertTrue(coils[36])  # last coil (bit 4 of 0x1B)

    def test_read_coils_mbap_and_exception(self):
        resp = mc.build_mbap_frame(7, bytes([0x01, 0x01, 0b00000101]))
        self.assertEqual(mc.read_coils(FakeConnection(resp), 7, 0, 3, mbap=True, timeout_ms=100), [True, False, True])
        exc = mc.build_mbap_frame(7, bytes([0x81, 0x02]))  # illegal data address
        with self.assertRaises(ValueError) as cm:
            mc.read_coils(FakeConnection(exc), 7, 0, 3, mbap=True, timeout_ms=100)
        self.assertIn("exception code 2", str(cm.exception))

    def test_write_single_coil_echo(self):
        req = mc.build_rtu_write_single_coil(1, 0, True)
        _, _, ok = mc.write_single_coil(FakeConnection(req), 1, 0, True, mbap=False, timeout_ms=100)
        self.assertTrue(ok)
        _, _, ok = mc.write_single_coil(FakeConnection(b""), 1, 0, True, mbap=False, timeout_ms=100)
        self.assertFalse(ok)

    def test_write_multiple_coils_ack(self):
        values = [True] * 8
        req = mc.build_rtu_write_multiple_coils(1, 0, values)
        ack = mc._with_crc(bytes.fromhex("010F00000008"))
        _, _, ok = mc.write_multiple_coils(FakeConnection(ack), 1, 0, values, mbap=False, timeout_ms=100)
        self.assertTrue(ok)
        wrong_qty = mc._with_crc(bytes.fromhex("010F00000004"))
        _, _, ok = mc.write_multiple_coils(FakeConnection(wrong_qty), 1, 0, values, mbap=False, timeout_ms=100)
        self.assertFalse(ok)
        # MBAP: response PDU is FC + start + quantity
        mreq = mc.build_mbap_write_multiple_coils(5, 0, values)
        mack = mc.build_mbap_frame(5, bytes.fromhex("0F00000008"))
        _, _, ok = mc.write_multiple_coils(FakeConnection(mack), 5, 0, values, mbap=True, timeout_ms=100)
        self.assertTrue(ok)
        self.assertEqual(mreq[7:12], bytes.fromhex("0F00000008"))

    def test_rejects_out_of_range_quantity(self):
        with self.assertRaises(ValueError):
            mc.read_coils(FakeConnection(b""), 1, 0, 0, False, 100)
        with self.assertRaises(ValueError):
            mc.write_multiple_coils(FakeConnection(b""), 1, 0, [], False, 100)


class ConnectionParamTests(unittest.TestCase):
    def test_rejects_missing_or_unknown_transport(self):
        with self.assertRaises(ValueError):
            mc.open_connection({})
        with self.assertRaises(ValueError):
            mc.open_connection({"transport": "serial"})   # no port
        with self.assertRaises(ValueError):
            mc.open_connection({"transport": "tcp"})      # no ip

    def test_rejects_invalid_serial_settings_before_touching_hardware(self):
        base = {"transport": "serial", "port": "COM_DOES_NOT_MATTER"}
        for bad in ({"dataBits": 9}, {"parity": "Bogus"}, {"stopBits": "Three"}, {"flowControl": "Nope"}):
            with self.assertRaises(ValueError):
                mc.open_connection({**base, **bad})


if __name__ == "__main__":
    unittest.main()
