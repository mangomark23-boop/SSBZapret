//! Минимальный парсер TLS ClientHello / SNI.
//!
//! Нам нужно ровно столько, чтобы прочитать SNI-хост и решить,
//! относится ли соединение к домену из активного списка.

/// Извлекает SNI из TCP-payload (если это TLS ClientHello).
pub fn extract_sni(tcp_payload: &[u8]) -> Option<String> {
    // TLS record: type(1)=22 handshake, version(2), length(2)
    if tcp_payload.len() < 5 || tcp_payload[0] != 0x16 {
        return None;
    }
    let buf = &tcp_payload[5..];
    // Handshake: type(1)=1 ClientHello, length(3)
    if buf.len() < 4 || buf[0] != 0x01 {
        return None;
    }
    let mut p = 4usize;
    // client_version(2) + random(32)
    p += 2 + 32;
    if p >= buf.len() {
        return None;
    }
    // session_id
    let sid_len = buf[p] as usize;
    p += 1 + sid_len;
    if p + 2 > buf.len() {
        return None;
    }
    // cipher_suites
    let cs_len = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
    p += 2 + cs_len;
    if p + 1 > buf.len() {
        return None;
    }
    // compression_methods
    let cm_len = buf[p] as usize;
    p += 1 + cm_len;
    if p + 2 > buf.len() {
        return None;
    }
    // extensions
    let ext_total = u16::from_be_bytes([buf[p], buf[p + 1]]) as usize;
    p += 2;
    let end = (p + ext_total).min(buf.len());
    while p + 4 <= end {
        let etype = u16::from_be_bytes([buf[p], buf[p + 1]]);
        let elen = u16::from_be_bytes([buf[p + 2], buf[p + 3]]) as usize;
        p += 4;
        if etype == 0x0000 {
            // server_name extension: list_len(2), name_type(1), name_len(2), name
            if p + 5 > buf.len() {
                return None;
            }
            let mut q = p + 2; // пропускаем server_name_list length
            let _name_type = buf[q];
            q += 1;
            let name_len = u16::from_be_bytes([buf[q], buf[q + 1]]) as usize;
            q += 2;
            if q + name_len > buf.len() {
                return None;
            }
            return std::str::from_utf8(&buf[q..q + name_len]).ok().map(|s| s.to_string());
        }
        p += elen;
    }
    None
}

/// Оффсет SNI-хоста внутри TCP-payload (используется для split).
pub fn sni_offset(tcp_payload: &[u8]) -> Option<usize> {
    let host = extract_sni(tcp_payload)?;
    tcp_payload
        .windows(host.len())
        .position(|w| w == host.as_bytes())
}

/// `example.com` совпадает с `example.com` и любым поддоменом `*.example.com`.
pub fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.trim_end_matches('.').to_lowercase();
    let pattern = pattern.trim_end_matches('.').to_lowercase();
    host == pattern || host.ends_with(&format!(".{pattern}"))
}
