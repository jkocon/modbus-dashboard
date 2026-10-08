//! Transport: a serial port (RS485, via the serialport crate - Windows/Linux/macOS) or a TCP
//! socket (RTU over TCP, or native Modbus TCP with an MBAP header).
//!
//! A response is read until the frame is complete (its length follows from the function code
//! or the MBAP header) or the line stays silent for longer than the Modbus inter-frame gap.
//! The previous reader stopped after 5 ms without a new byte, which cut every frame at
//! 1200 baud (one byte takes ~9 ms there) and could cut frames from USB adapters that deliver
//! data in 16 ms chunks.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};

use crate::core::{expected_len, int_or};

pub trait Connection {
    /// Send a request and return the response (possibly empty when nothing answered).
    /// `timeout_ms` is how long to wait for the first byte of the answer.
    fn send_request(&mut self, request: &[u8], timeout_ms: u64, mbap: bool) -> Result<Vec<u8>, String>;
}

/// Longest response we ever wait for (Modbus RTU ADU limit).
const MAX_FRAME: usize = 256;

fn complete(buf: &[u8], mbap: bool) -> bool {
    expected_len(buf, mbap).is_some_and(|len| buf.len() >= len)
}

pub struct Serial {
    port: Box<dyn SerialPort>,
    /// Time of one character on the line (start + 8 data + parity/stop ≈ 11 bits).
    char_time: Duration,
}

impl Serial {
    pub fn open(
        path: &str,
        baud: u32,
        data_bits: i64,
        parity: &str,
        stop_bits: &str,
        flow: &str,
    ) -> Result<Serial, String> {
        let data_bits = match data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            8 => DataBits::Eight,
            other => return Err(format!("Unsupported data bits: {other}. Allowed: [5, 6, 7, 8].")),
        };
        let parity = match parity {
            "None" => Parity::None,
            "Even" => Parity::Even,
            "Odd" => Parity::Odd,
            "Mark" | "Space" => return Err(format!("{parity} parity is not supported. Use None, Even or Odd.")),
            other => return Err(format!("Unknown parity: {other}. Allowed: [Even, None, Odd].")),
        };
        let stop_bits = match stop_bits {
            "One" => StopBits::One,
            "Two" => StopBits::Two,
            "OnePointFive" => return Err("1.5 stop bits are not supported. Use 1 or 2.".into()),
            other => return Err(format!("Unknown stop bits: {other}. Allowed: [One, Two].")),
        };
        let flow = match flow {
            "None" => FlowControl::None,
            "RtsCts" => FlowControl::Hardware,
            "XonXoff" => FlowControl::Software,
            "DsrDtr" => return Err("DSR/DTR flow control is not supported. Use None, RTS/CTS or XON/XOFF.".into()),
            other => return Err(format!("Unknown flow control: {other}. Allowed: [None, RtsCts, XonXoff].")),
        };
        let label = format!("{baud} baud, {data_bits}{}{stop_bits}, flow={flow:?}", format!("{parity:?}").chars().next().unwrap_or('N'));
        let port = serialport::new(path, baud)
            .data_bits(data_bits)
            .parity(parity)
            .stop_bits(stop_bits)
            .flow_control(flow)
            .timeout(Duration::from_millis(10))
            .open()
            .map_err(|e| {
                let msg = e.to_string();
                if msg.to_lowercase().contains("parameter is incorrect") {
                    format!(
                        "The {path} driver rejected these settings ({label}) - Win32 code ERROR_INVALID_PARAMETER. \
                         This combination of data bits/parity/stop bits/flow control is most likely not supported \
                         by this USB-RS485 adapter (typically limited to 7-8 data bits and 1/2 stop bits). \
                         Try the default 8N1 with no flow control."
                    )
                } else if msg.to_lowercase().contains("permission denied") {
                    format!("Cannot open {path}: permission denied. Add your user to the group that owns the port (uucp on Arch, dialout on Debian/Ubuntu) and log in again.")
                } else if msg.to_lowercase().contains("busy") {
                    format!("Cannot open {path}: the port is busy (used by another program).")
                } else {
                    format!("Cannot open {path} ({label}): {msg}")
                }
            })?;
        Ok(Serial { port, char_time: Duration::from_secs_f64(11.0 / baud.max(1) as f64) })
    }
}

impl Connection for Serial {
    fn send_request(&mut self, request: &[u8], timeout_ms: u64, mbap: bool) -> Result<Vec<u8>, String> {
        let _ = self.port.clear(ClearBuffer::Input);
        self.port.write_all(request).map_err(|e| format!("Write failed: {e}"))?;
        let _ = self.port.flush();
        let start = Instant::now();
        // The request itself takes time on the line before the device can even start to answer.
        let tx = self.char_time * request.len() as u32;
        let first_byte_deadline = start + tx + Duration::from_millis(timeout_ms);
        let hard_deadline = first_byte_deadline + self.char_time * MAX_FRAME as u32;
        // Modbus RTU: a frame ends after 3.5 characters of silence; USB adapters add their own
        // latency (FTDI: 16 ms), so never less than 20 ms.
        let gap = (self.char_time * 7 / 2).max(Duration::from_millis(20));

        let mut buf = Vec::new();
        let mut last_rx = start;
        let mut chunk = [0u8; MAX_FRAME];
        loop {
            match self.port.read(&mut chunk) {
                Ok(n) if n > 0 => {
                    buf.extend_from_slice(&chunk[..n]);
                    last_rx = Instant::now();
                    if complete(&buf, mbap) || buf.len() >= MAX_FRAME {
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::TimedOut => {}
                Err(e) => return Err(format!("Read failed: {e}")),
            }
            let now = Instant::now();
            if buf.is_empty() && now >= first_byte_deadline {
                break;
            }
            if !buf.is_empty() && (now - last_rx >= gap || now >= hard_deadline) {
                break;
            }
        }
        Ok(buf)
    }
}

pub struct Tcp {
    sock: TcpStream,
}

impl Tcp {
    pub fn open(ip: &str, port: u16, timeout_ms: u64) -> Result<Tcp, String> {
        let addr = (ip, port)
            .to_socket_addrs()
            .map_err(|e| format!("Cannot resolve {ip}: {e}"))?
            .next()
            .ok_or_else(|| format!("Cannot resolve {ip}"))?;
        let sock = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms.max(1000)))
            .map_err(|e| format!("Cannot connect to {ip}:{port}: {e}"))?;
        let _ = sock.set_nodelay(true);
        Ok(Tcp { sock })
    }

    /// Throw away whatever arrived late for a previous request.
    fn drain(&mut self) {
        if self.sock.set_nonblocking(true).is_ok() {
            let mut chunk = [0u8; 512];
            while matches!(self.sock.read(&mut chunk), Ok(n) if n > 0) {}
            let _ = self.sock.set_nonblocking(false);
        }
    }
}

impl Connection for Tcp {
    fn send_request(&mut self, request: &[u8], timeout_ms: u64, mbap: bool) -> Result<Vec<u8>, String> {
        self.drain();
        let _ = self.sock.set_write_timeout(Some(Duration::from_millis(timeout_ms.max(1))));
        self.sock.write_all(request).map_err(|e| format!("Send failed: {e}"))?;
        let start = Instant::now();
        let first_byte_deadline = start + Duration::from_millis(timeout_ms);
        // Converters pack serial data into TCP segments with their own gap; wait a bit longer
        // than on a direct serial line when the length is unknown.
        let gap = Duration::from_millis(50);
        let hard_deadline = first_byte_deadline + Duration::from_secs(2);
        let _ = self.sock.set_read_timeout(Some(Duration::from_millis(10)));
        let mut buf = Vec::new();
        let mut last_rx = start;
        let mut chunk = [0u8; 512];
        loop {
            match self.sock.read(&mut chunk) {
                Ok(0) => break, // closed by the peer
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    last_rx = Instant::now();
                    if complete(&buf, mbap) || buf.len() >= 4096 {
                        break;
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
            let now = Instant::now();
            if buf.is_empty() && now >= first_byte_deadline {
                break;
            }
            if !buf.is_empty() && (now - last_rx >= gap || now >= hard_deadline) {
                break;
            }
        }
        Ok(buf)
    }
}

pub fn list_serial_ports() -> Result<Vec<String>, String> {
    let mut ports: Vec<String> =
        serialport::available_ports().map_err(|e| e.to_string())?.into_iter().map(|p| p.port_name).collect();
    ports.sort();
    Ok(ports)
}

/// conn: {"transport": "serial"|"tcp", "port"/"baud"/"dataBits"/"parity"/"stopBits"/"flowControl"
/// or "ip"/"tcpPort", "timeoutMs"}
pub fn open_connection(conn: &Value) -> Result<Box<dyn Connection>, String> {
    let timeout_ms = int_or(conn, "timeoutMs", 300)?.clamp(1, 60_000) as u64;
    let text = |key: &str, default: &str| conn.get(key).and_then(Value::as_str).unwrap_or(default).to_string();
    match conn.get("transport").and_then(Value::as_str) {
        Some("serial") => {
            let port = text("port", "");
            if port.is_empty() {
                return Err("Select a COM port.".into());
            }
            let baud = int_or(conn, "baud", 9600)?;
            if !(50..=4_000_000).contains(&baud) {
                return Err(format!("Unsupported baud rate: {baud}."));
            }
            let serial = Serial::open(
                &port,
                baud as u32,
                int_or(conn, "dataBits", 8)?,
                &text("parity", "None"),
                &text("stopBits", "One"),
                &text("flowControl", "None"),
            )?;
            // Give the port a moment after opening: some USB-RS485 adapters drop the first bytes.
            thread::sleep(Duration::from_millis(2));
            Ok(Box::new(serial))
        }
        Some("tcp") => {
            let ip = text("ip", "");
            if ip.is_empty() {
                return Err("Enter the converter's IP address.".into());
            }
            let port = int_or(conn, "tcpPort", 502)?;
            if !(1..=65535).contains(&port) {
                return Err(format!("tcpPort must be between 1 and 65535 (got {port})."));
            }
            Ok(Box::new(Tcp::open(&ip, port as u16, timeout_ms)?))
        }
        _ => Err("Unknown connection type (transport must be 'serial' or 'tcp').".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// RS485 converter on TCP that sends the reply in pieces with pauses in between.
    #[test]
    fn tcp_reads_whole_frame_sent_in_pieces() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reply = crate::core::with_crc(vec![1, 3, 16].into_iter().chain([0u8; 16]).collect());
        let sent = reply.clone();
        let device = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut req = [0u8; 8];
            s.read_exact(&mut req).unwrap();
            for piece in sent.chunks(4) {
                s.write_all(piece).unwrap();
                thread::sleep(Duration::from_millis(30));
            }
        });
        let mut conn = Tcp::open("127.0.0.1", port, 500).unwrap();
        let got = conn.send_request(&crate::core::rtu_request(1, 3, 0, 8), 500, false).unwrap();
        assert_eq!(got, reply);
        device.join().unwrap();
    }

    /// A device on a serial line at 1200 baud: one byte every ~9 ms. The old reader stopped
    /// after 5 ms of silence and returned 2 of 21 bytes.
    #[cfg(unix)]
    #[test]
    fn serial_reads_whole_frame_at_1200_baud() {
        let (mut device, ours) = serialport::TTYPort::pair().unwrap();
        let reply = crate::core::with_crc(vec![1, 3, 16].into_iter().chain([0u8; 16]).collect());
        let sent = reply.clone();
        let char_time = Duration::from_secs_f64(11.0 / 1200.0);
        let dev = thread::spawn(move || {
            let mut req = [0u8; 8];
            device.set_timeout(Duration::from_secs(2)).unwrap();
            device.read_exact(&mut req).unwrap();
            for b in sent {
                device.write_all(&[b]).unwrap();
                thread::sleep(char_time);
            }
        });
        let mut conn = Serial { port: Box::new(ours), char_time };
        let got = conn.send_request(&crate::core::rtu_request(1, 3, 0, 8), 300, false).unwrap();
        assert_eq!(got, reply);
        dev.join().unwrap();
    }

    #[test]
    fn connection_errors_are_readable() {
        assert!(open_connection(&serde_json::json!({})).err().unwrap().contains("transport"));
        assert!(open_connection(&serde_json::json!({"transport": "serial"})).err().unwrap().contains("COM port"));
        let err = open_connection(&serde_json::json!({"transport": "serial", "port": "/dev/x", "parity": "Mark"}));
        assert!(err.err().unwrap().contains("Mark parity"));
    }
}
