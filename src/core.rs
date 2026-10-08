//! Modbus logic shared by the API: CRC16, RTU and MBAP (Modbus TCP) frames, frame-length
//! detection, input validation and the high-level operations (R4DCB08 temperatures, coils,
//! write single register).

use serde_json::{json, Value};

use crate::transport::Connection;

// ===================== CRC16 / RTU frames =====================

pub fn crc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for b in data {
        crc ^= *b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

pub fn with_crc(mut frame: Vec<u8>) -> Vec<u8> {
    let c = crc(&frame);
    frame.extend_from_slice(&c.to_le_bytes());
    frame
}

pub fn check_crc(resp: &[u8]) -> bool {
    resp.len() >= 4 && crc(&resp[..resp.len() - 2]).to_le_bytes() == resp[resp.len() - 2..]
}

fn read_pdu(function_code: u8, register: u16, quantity: u16) -> Vec<u8> {
    let [rh, rl] = register.to_be_bytes();
    let [qh, ql] = quantity.to_be_bytes();
    vec![function_code, rh, rl, qh, ql]
}

fn write_single_pdu(register: u16, value: u16) -> Vec<u8> {
    read_pdu(0x06, register, value)
}

fn rtu(slave: u8, pdu: Vec<u8>) -> Vec<u8> {
    let mut frame = vec![slave];
    frame.extend(pdu);
    with_crc(frame)
}

pub fn rtu_request(slave: u8, function_code: u8, register: u16, quantity: u16) -> Vec<u8> {
    rtu(slave, read_pdu(function_code, register, quantity))
}

pub fn rtu_write_single(slave: u8, register: u16, value: u16) -> Vec<u8> {
    rtu(slave, write_single_pdu(register, value))
}

// ===================== Native Modbus TCP (MBAP) =====================
//
// 7-byte header: Transaction Id(2) + Protocol Id(2, =0) + Length(2) + Unit Id(1), followed by
// the PDU (function + data) with NO CRC. Standard port 502.

pub fn mbap_frame(unit: u8, pdu: &[u8], transaction_id: u16) -> Vec<u8> {
    let mut frame = Vec::with_capacity(7 + pdu.len());
    frame.extend_from_slice(&transaction_id.to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&((pdu.len() + 1) as u16).to_be_bytes());
    frame.push(unit);
    frame.extend_from_slice(pdu);
    frame
}

pub fn mbap_read(unit: u8, function_code: u8, register: u16, quantity: u16) -> Vec<u8> {
    mbap_frame(unit, &read_pdu(function_code, register, quantity), 1)
}

pub fn mbap_write_single(unit: u8, register: u16, value: u16) -> Vec<u8> {
    mbap_frame(unit, &write_single_pdu(register, value), 1)
}

pub fn mbap_ok(resp: &[u8], unit: u8) -> bool {
    resp.len() >= 8 && resp[6] == unit && resp.len() >= 6 + u16::from_be_bytes([resp[4], resp[5]]) as usize
}

pub fn mbap_pdu(resp: &[u8]) -> &[u8] {
    resp.get(7..).unwrap_or(&[])
}

pub fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Total length of the response frame that `buf` starts, once enough of it has arrived to tell;
/// None when it cannot be known yet (or for unknown function codes - then the reader falls back
/// to waiting for silence on the line).
pub fn expected_len(buf: &[u8], mbap: bool) -> Option<usize> {
    if mbap {
        return (buf.len() >= 6).then(|| 6 + u16::from_be_bytes([buf[4], buf[5]]) as usize);
    }
    let function_code = *buf.get(1)?;
    if function_code & 0x80 != 0 {
        return Some(5); // exception: address, function|0x80, code, CRC
    }
    match function_code {
        0x01..=0x04 => buf.get(2).map(|n| 3 + *n as usize + 2),
        0x05 | 0x06 | 0x0F | 0x10 => Some(8),
        _ => None,
    }
}

// ===================== Input validation =====================

/// Integer field of the request body (JSON number or numeric string, like Python's int()).
pub fn int(params: &Value, key: &str) -> Result<i64, String> {
    match params.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).ok_or(format!("{key} must be an integer.")),
        Some(Value::String(s)) => s.trim().parse().map_err(|_| format!("{key} must be an integer.")),
        None | Some(Value::Null) => Err(format!("{key} is missing or not a number.")),
        _ => Err(format!("{key} must be an integer.")),
    }
}

pub fn int_or(params: &Value, key: &str, default: i64) -> Result<i64, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        _ => int(params, key),
    }
}

pub fn in_range(value: i64, key: &str, min: i64, max: i64) -> Result<i64, String> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{key} must be between {min} and {max} (got {value})."))
    }
}

/// Modbus device address. Values are never masked into a byte: 256 used to become 0, the
/// broadcast address, which every device on the bus obeys.
pub fn address(params: &Value, key: &str, min: i64) -> Result<u8, String> {
    Ok(in_range(int(params, key)?, key, min, 247)? as u8)
}

pub fn u16_field(params: &Value, key: &str, default: i64) -> Result<u16, String> {
    Ok(in_range(int_or(params, key, default)?, key, 0, 0xFFFF)? as u16)
}

pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

// ===================== High-level operations =====================

pub const BAUD_CODES: [(u32, u16); 5] = [(1200, 0), (2400, 1), (4800, 2), (9600, 3), (19200, 4)];

pub fn baud_code(baud: i64) -> Result<u16, String> {
    BAUD_CODES.iter().find(|(b, _)| *b as i64 == baud).map(|(_, c)| *c).ok_or_else(|| {
        let allowed: Vec<String> = BAUD_CODES.iter().map(|(b, _)| b.to_string()).collect();
        format!("Unsupported baud rate: {baud}. Allowed: [{}].", allowed.join(", "))
    })
}

/// (request, response, success) of a write.
pub type WriteResult = (Vec<u8>, Vec<u8>, bool);

/// The 8 temperature channels of an R4DCB08 module (Read Holding Registers, register 0).
pub fn read_r4dcb08(conn: &mut dyn Connection, slave: u8, mbap: bool, timeout_ms: u64) -> Result<Vec<Value>, String> {
    let data: Vec<u8> = if mbap {
        let resp = conn.send_request(&mbap_read(slave, 0x03, 0, 8), timeout_ms, true).unwrap_or_default();
        if !mbap_ok(&resp, slave) {
            return Err("No or invalid response (MBAP).".into());
        }
        let pdu = mbap_pdu(&resp);
        if pdu.len() < 2 || pdu[0] != 0x03 {
            return Err("Unexpected response (function code mismatch).".into());
        }
        pdu[2..].iter().take(pdu[1] as usize).copied().collect()
    } else {
        let resp = conn.send_request(&rtu_request(slave, 0x03, 0, 8), timeout_ms, false).unwrap_or_default();
        if resp.len() < 21 {
            return Err(format!(
                "Response too short ({} bytes) - no response or a corrupted one from the device.",
                resp.len()
            ));
        }
        if !check_crc(&resp) {
            return Err("CRC error in response.".into());
        }
        if resp[0] != slave || resp[1] != 0x03 {
            return Err("Unexpected response (address/function mismatch).".into());
        }
        resp[3..].iter().take(resp[2] as usize).copied().collect()
    };
    if data.len() < 16 {
        return Err("Not enough data in response.".into());
    }
    Ok((0..8)
        .map(|ch| {
            let raw = u16::from_be_bytes([data[ch * 2], data[ch * 2 + 1]]);
            if raw == 0x8000 {
                json!({"channel": ch + 1, "temperatureC": null, "status": "No sensor"})
            } else {
                let celsius = raw as i16 as f64 / 10.0;
                json!({"channel": ch + 1, "temperatureC": celsius, "status": "OK"})
            }
        })
        .collect())
}

/// Send Write Single Register (0x06); success = the device echoes the request.
pub fn write_single_register(
    conn: &mut dyn Connection,
    unit: u8,
    register: u16,
    value: u16,
    mbap: bool,
    timeout_ms: u64,
) -> WriteResult {
    let request = if mbap { mbap_write_single(unit, register, value) } else { rtu_write_single(unit, register, value) };
    let resp = conn.send_request(&request, timeout_ms, mbap).unwrap_or_default();
    let success = !resp.is_empty() && resp == request;
    (request, resp, success)
}

// ===================== Coils (relays): 0x01 / 0x05 / 0x0F =====================

pub const MAX_COILS: usize = 256; // per request; the Modbus limit is higher, but this keeps the UI grid sane

/// Booleans into Modbus coil bytes (coil 0 = LSB of byte 0).
pub fn pack_coils(values: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; values.len().div_ceil(8)];
    for (i, v) in values.iter().enumerate() {
        if *v {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

pub fn unpack_coils(data: &[u8], quantity: usize) -> Vec<bool> {
    (0..quantity).map(|i| (data[i / 8] >> (i % 8)) & 1 == 1).collect()
}

fn write_single_coil_pdu(coil: u16, on: bool) -> Vec<u8> {
    let [h, l] = coil.to_be_bytes();
    vec![0x05, h, l, if on { 0xFF } else { 0x00 }, 0x00]
}

fn write_multiple_coils_pdu(start: u16, values: &[bool]) -> Vec<u8> {
    let packed = pack_coils(values);
    let [sh, sl] = start.to_be_bytes();
    let [qh, ql] = (values.len() as u16).to_be_bytes();
    let mut pdu = vec![0x0F, sh, sl, qh, ql, packed.len() as u8];
    pdu.extend(packed);
    pdu
}

pub fn rtu_write_single_coil(slave: u8, coil: u16, on: bool) -> Vec<u8> {
    rtu(slave, write_single_coil_pdu(coil, on))
}

pub fn rtu_write_multiple_coils(slave: u8, start: u16, values: &[bool]) -> Vec<u8> {
    rtu(slave, write_multiple_coils_pdu(start, values))
}

pub fn mbap_write_single_coil(unit: u8, coil: u16, on: bool) -> Vec<u8> {
    mbap_frame(unit, &write_single_coil_pdu(coil, on), 1)
}

pub fn mbap_write_multiple_coils(unit: u8, start: u16, values: &[bool]) -> Vec<u8> {
    mbap_frame(unit, &write_multiple_coils_pdu(start, values), 1)
}

fn exception_check(function_code: u8, pdu: &[u8]) -> Result<(), String> {
    if pdu.first() == Some(&(function_code | 0x80)) {
        let code = pdu.get(1).map(|c| c.to_string()).unwrap_or_else(|| "?".into());
        return Err(format!("Device returned Modbus exception code {code} for function 0x{function_code:02X}."));
    }
    Ok(())
}

fn check_coil_range(start: u16, count: usize) -> Result<(), String> {
    if start as usize + count > 0x1_0000 {
        return Err(format!("Coils {start}..{} go past the last coil address 65535.", start as usize + count - 1));
    }
    Ok(())
}

/// Read Coils (0x01): one boolean per coil.
pub fn read_coils(
    conn: &mut dyn Connection,
    slave: u8,
    start: u16,
    quantity: usize,
    mbap: bool,
    timeout_ms: u64,
) -> Result<Vec<bool>, String> {
    if !(1..=MAX_COILS).contains(&quantity) {
        return Err(format!("quantity must be between 1 and {MAX_COILS}."));
    }
    check_coil_range(start, quantity)?;
    let expected_bytes = quantity.div_ceil(8);
    let pdu: Vec<u8> = if mbap {
        let resp = conn.send_request(&mbap_read(slave, 0x01, start, quantity as u16), timeout_ms, true).unwrap_or_default();
        if !mbap_ok(&resp, slave) {
            return Err("No or invalid response (MBAP).".into());
        }
        mbap_pdu(&resp).to_vec()
    } else {
        let resp = conn.send_request(&rtu_request(slave, 0x01, start, quantity as u16), timeout_ms, false).unwrap_or_default();
        if resp.len() < 5 {
            return Err(format!(
                "Response too short ({} bytes) - no response or a corrupted one from the device.",
                resp.len()
            ));
        }
        if !check_crc(&resp) {
            return Err("CRC error in response.".into());
        }
        if resp[0] != slave {
            return Err("Unexpected response (address mismatch).".into());
        }
        resp[1..resp.len() - 2].to_vec()
    };
    exception_check(0x01, &pdu)?;
    if pdu.len() < 2 || pdu[0] != 0x01 {
        return Err("Unexpected response (function code mismatch).".into());
    }
    let data: Vec<u8> = pdu[2..].iter().take(pdu[1] as usize).copied().collect();
    if data.len() < expected_bytes {
        return Err("Not enough data in response.".into());
    }
    Ok(unpack_coils(&data, quantity))
}

/// Write Single Coil (0x05); success = the device echoes the request.
pub fn write_single_coil(conn: &mut dyn Connection, unit: u8, coil: u16, on: bool, mbap: bool, timeout_ms: u64) -> WriteResult {
    let request = if mbap { mbap_write_single_coil(unit, coil, on) } else { rtu_write_single_coil(unit, coil, on) };
    let resp = conn.send_request(&request, timeout_ms, mbap).unwrap_or_default();
    let success = !resp.is_empty() && resp == request;
    (request, resp, success)
}

/// Write Multiple Coils (0x0F); success = the device acknowledges start + quantity.
pub fn write_multiple_coils(
    conn: &mut dyn Connection,
    unit: u8,
    start: u16,
    values: &[bool],
    mbap: bool,
    timeout_ms: u64,
) -> Result<WriteResult, String> {
    if !(1..=MAX_COILS).contains(&values.len()) {
        return Err(format!("values must contain between 1 and {MAX_COILS} coils."));
    }
    check_coil_range(start, values.len())?;
    let request = if mbap { mbap_write_multiple_coils(unit, start, values) } else { rtu_write_multiple_coils(unit, start, values) };
    let resp = conn.send_request(&request, timeout_ms, mbap).unwrap_or_default();
    let success = if mbap {
        mbap_ok(&resp, unit) && mbap_pdu(&resp).get(..5) == request.get(7..12)
    } else {
        resp.len() >= 8 && check_crc(&resp) && resp[..6] == request[..6]
    };
    Ok((request, resp, success))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Answers every request with a fixed response and remembers what was sent.
    pub struct Fake {
        pub response: Vec<u8>,
        pub sent: Vec<u8>,
    }

    impl Fake {
        pub fn new(response: &[u8]) -> Fake {
            Fake { response: response.to_vec(), sent: Vec::new() }
        }
    }

    impl Connection for Fake {
        fn send_request(&mut self, request: &[u8], _timeout_ms: u64, _mbap: bool) -> Result<Vec<u8>, String> {
            self.sent = request.to_vec();
            Ok(self.response.clone())
        }
    }

    fn unhex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn crc_known_vector() {
        assert_eq!(crc(&unhex("010300000008")), 0x0C44);
        let frame = rtu_request(1, 3, 0, 8);
        assert_eq!(frame, unhex("010300000008440C"));
        assert!(check_crc(&frame));
        let mut bad = frame.clone();
        bad[3] ^= 1;
        assert!(!check_crc(&bad));
        assert!(!check_crc(&[]));
        assert!(!check_crc(&[1, 3]));
        let w = rtu_write_single(3, 0xFE, 2);
        assert_eq!(w[..6], [3, 0x06, 0x00, 0xFE, 0x00, 0x02]);
        assert!(check_crc(&w));
    }

    #[test]
    fn mbap_frames() {
        assert_eq!(mbap_frame(7, &read_pdu(3, 0x10, 2), 0x1234), unhex("1234 0000 0006 07 03 0010 0002"));
        let good = unhex("0001 0000 0005 07 03 02 002A");
        assert!(mbap_ok(&good, 7));
        assert!(!mbap_ok(&good, 8));
        assert!(!mbap_ok(&good[..good.len() - 1], 7));
        assert!(!mbap_ok(&[], 7));
        assert_eq!(mbap_pdu(&good), unhex("0302002A"));
    }

    #[test]
    fn frame_lengths() {
        assert_eq!(expected_len(&[1], false), None);
        assert_eq!(expected_len(&[1, 3], false), None);
        assert_eq!(expected_len(&[1, 3, 16], false), Some(21));
        assert_eq!(expected_len(&[1, 0x83], false), Some(5));
        assert_eq!(expected_len(&[1, 6], false), Some(8));
        assert_eq!(expected_len(&[1, 0x2B], false), None);
        assert_eq!(expected_len(&unhex("0001 0000 0005"), true), Some(11));
    }

    fn r4dcb08_response() -> Vec<u8> {
        let mut data = vec![1u8, 3, 16];
        for raw in [234u16, (-50i16) as u16, 0x8000, 0, 0, 0, 0, 0] {
            data.extend_from_slice(&raw.to_be_bytes());
        }
        with_crc(data)
    }

    #[test]
    fn temperatures_and_missing_sensor() {
        let mut conn = Fake::new(&r4dcb08_response());
        let results = read_r4dcb08(&mut conn, 1, false, 100).unwrap();
        assert_eq!(conn.sent, rtu_request(1, 3, 0, 8));
        assert_eq!(results[0], json!({"channel": 1, "temperatureC": 23.4, "status": "OK"}));
        assert_eq!(results[1]["temperatureC"], json!(-5.0));
        assert_eq!(results[2], json!({"channel": 3, "temperatureC": null, "status": "No sensor"}));
        assert_eq!(results.len(), 8);
        let mut bad = r4dcb08_response();
        bad[5] ^= 0xFF;
        assert!(read_r4dcb08(&mut Fake::new(&bad), 1, false, 100).is_err());
        assert!(read_r4dcb08(&mut Fake::new(&bad[..10]), 1, false, 100).is_err());
        assert!(read_r4dcb08(&mut Fake::new(&r4dcb08_response()), 2, false, 100).is_err());
        // MBAP: same data without address and CRC
        let resp = r4dcb08_response();
        let mbap = mbap_frame(1, &resp[1..resp.len() - 2], 1);
        assert_eq!(read_r4dcb08(&mut Fake::new(&mbap), 1, true, 100).unwrap()[0]["temperatureC"], json!(23.4));
    }

    #[test]
    fn write_echo() {
        let request = rtu_write_single(1, 254, 5);
        let (req, _, ok) = write_single_register(&mut Fake::new(&request), 1, 254, 5, false, 100);
        assert_eq!(req, request);
        assert!(ok);
        assert!(!write_single_register(&mut Fake::new(&[]), 1, 254, 5, false, 100).2);
        assert!(!write_single_register(&mut Fake::new(&rtu_write_single(1, 254, 6)), 1, 254, 5, false, 100).2);
    }

    #[test]
    fn coils() {
        let values = [true, false, true, true, false, false, true, true, true];
        let packed = pack_coils(&values);
        assert_eq!(packed, [0xCD, 0x01]);
        assert_eq!(unpack_coils(&packed, 9), values);
        // Modbus spec example: write 10 coils from 20 (0x13) on slave 17
        let spec = [true, false, true, true, false, false, true, true, true, false];
        assert_eq!(rtu_write_multiple_coils(0x11, 0x13, &spec), unhex("110F0013000A02CD01BF0B"));
        assert_eq!(rtu_write_single_coil(1, 3, true)[..6], unhex("01050003FF00"));
        assert_eq!(rtu_write_single_coil(1, 3, false)[..6], unhex("010500030000"));
        // Read Coils spec response: 37 coils
        let resp = with_crc(unhex("110105CD6BB20E1B"));
        let coils = read_coils(&mut Fake::new(&resp), 0x11, 0x13, 37, false, 100).unwrap();
        assert_eq!(coils.len(), 37);
        assert_eq!(coils[..8], [true, false, true, true, false, false, true, true]);
        assert!(coils[36]);
        let mbap = mbap_frame(7, &[0x01, 0x01, 0x05], 1);
        assert_eq!(read_coils(&mut Fake::new(&mbap), 7, 0, 3, true, 100).unwrap(), [true, false, true]);
        let exc = with_crc(vec![7, 0x81, 0x02]);
        let err = read_coils(&mut Fake::new(&exc), 7, 0, 3, false, 100).unwrap_err();
        assert!(err.contains("exception code 2"), "{err}");
        // A bare MBAP header used to crash with "index out of range".
        let short = mbap_frame(7, &[0x01], 1);
        assert!(read_coils(&mut Fake::new(&short), 7, 0, 3, true, 100).unwrap_err().contains("mismatch"));
        assert!(read_coils(&mut Fake::new(&[]), 7, 65530, 10, false, 100).unwrap_err().contains("65535"));
    }

    #[test]
    fn coil_writes() {
        let req = rtu_write_single_coil(1, 3, true);
        assert!(write_single_coil(&mut Fake::new(&req), 1, 3, true, false, 100).2);
        assert!(!write_single_coil(&mut Fake::new(&[]), 1, 3, true, false, 100).2);
        let ack = with_crc(unhex("110F0013000A"));
        assert!(write_multiple_coils(&mut Fake::new(&ack), 0x11, 0x13, &[true; 10], false, 100).unwrap().2);
        assert!(write_multiple_coils(&mut Fake::new(&ack), 1, 0, &[], false, 100).is_err());
    }

    #[test]
    fn validation() {
        let p = json!({"a": 256, "b": "12", "c": null, "d": 1.0, "e": "x"});
        assert!(address(&p, "a", 1).unwrap_err().contains("between 1 and 247"));
        assert_eq!(address(&p, "b", 1).unwrap(), 12);
        assert!(address(&p, "c", 1).is_err());
        assert_eq!(address(&p, "d", 1).unwrap(), 1);
        assert!(address(&p, "e", 1).is_err());
        assert!(address(&json!({"z": 0}), "z", 1).is_err());
        assert_eq!(address(&json!({"z": 0}), "z", 0).unwrap(), 0);
        assert_eq!(baud_code(9600).unwrap(), 3);
        assert!(baud_code(115200).is_err());
    }
}
