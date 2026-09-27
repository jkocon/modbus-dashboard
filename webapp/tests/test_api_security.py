"""Integration tests for the HTTP API's security controls.

Starts the real server on an ephemeral loopback port and checks the DNS-rebinding,
CSRF, request-size and input-validation protections. No hardware or network
scanning is exercised.

Run from the webapp/ directory:
    python -m unittest discover -s tests -v
"""

import http.client
import json
import sys
import threading
import unittest
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import modbus_dashboard as md  # noqa: E402


class ApiSecurityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), md.Handler)
        cls.port = cls.server.server_address[1]
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def _request(self, method, path, body=None, headers=None):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
        hdrs = {"Host": f"127.0.0.1:{self.port}"}
        if headers:
            hdrs.update(headers)
        data = None
        if body is not None:
            data = body if isinstance(body, (bytes, str)) else json.dumps(body)
            hdrs.setdefault("Content-Type", "application/json")
        conn.request(method, path, body=data, headers=hdrs)
        resp = conn.getresponse()
        payload = resp.read()
        conn.close()
        return resp.status, payload

    # ---- baseline: legitimate same-origin traffic works ----

    def test_index_and_api_work_for_loopback_host(self):
        status, body = self._request("GET", "/")
        self.assertEqual(status, 200)
        self.assertIn(b"<html", body)
        status, body = self._request("GET", "/api/ports")
        self.assertEqual(status, 200)
        self.assertIn("ports", json.loads(body))

    def test_same_origin_post_with_origin_header_is_allowed(self):
        status, body = self._request(
            "POST", "/api/read-temperature",
            body={"connection": {}, "slaveId": 1},
            headers={"Origin": f"http://127.0.0.1:{self.port}"},
        )
        # Reaches the handler (400 from open_connection's validation, not 403)
        self.assertEqual(status, 400)
        self.assertIn("transport", json.loads(body)["error"])

    # ---- DNS rebinding ----

    def test_foreign_host_header_is_rejected(self):
        status, _ = self._request("GET", "/", headers={"Host": "evil.example.com"})
        self.assertEqual(status, 403)
        status, _ = self._request("GET", "/api/ports", headers={"Host": "evil.example.com:80"})
        self.assertEqual(status, 403)

    # ---- CSRF ----

    def test_foreign_origin_is_rejected(self):
        status, _ = self._request(
            "POST", "/api/set-address",
            body={"connection": {}, "oldAddress": 1, "newAddress": 2},
            headers={"Origin": "http://attacker.example"},
        )
        self.assertEqual(status, 403)

    def test_null_origin_is_rejected(self):
        status, _ = self._request("POST", "/api/scan", body={}, headers={"Origin": "null"})
        self.assertEqual(status, 403)

    def test_post_without_json_content_type_is_rejected(self):
        # A cross-site "simple request" (text/plain) must never reach a handler.
        status, body = self._request(
            "POST", "/api/set-baudrate",
            body='{"connection":{},"slaveId":1,"newBaudRate":9600}',
            headers={"Content-Type": "text/plain"},
        )
        self.assertEqual(status, 415)
        self.assertIn("application/json", json.loads(body)["error"])

    # ---- input handling ----

    def test_oversized_body_is_rejected(self):
        status, _ = self._request(
            "POST", "/api/scan", body=b"{}",
            headers={"Content-Length": str(md.MAX_BODY_BYTES + 1)},
        )
        self.assertEqual(status, 413)

    def test_malformed_json_is_400_not_500(self):
        status, body = self._request("POST", "/api/scan", body="{not json")
        self.assertEqual(status, 400)
        self.assertIn("Invalid JSON", json.loads(body)["error"])
        status, body = self._request("POST", "/api/scan", body="[1,2,3]")
        self.assertEqual(status, 400)

    def test_static_path_traversal_is_blocked(self):
        for path in ("/../modbus_core.py", "/static/../../modbus_core.py", "/..%2f..%2fmodbus_core.py"):
            status, body = self._request("GET", path)
            self.assertIn(status, (403, 404), path)
            self.assertNotIn(b"def modbus_crc", body, path)

    def test_unknown_api_path_and_job_ids(self):
        status, _ = self._request("POST", "/api/does-not-exist", body={})
        self.assertEqual(status, 404)
        status, _ = self._request("GET", "/api/scan/" + "0" * 32)
        self.assertEqual(status, 404)
        status, _ = self._request("GET", "/api/scan/not-a-job-id")
        self.assertEqual(status, 404)


if __name__ == "__main__":
    unittest.main()
