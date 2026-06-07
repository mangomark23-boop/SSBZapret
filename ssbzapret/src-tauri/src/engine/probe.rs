//! Лёгкая проверка доступности домена (probe) без драйвера.
//!
//! Делает TCP-подключение к :443, отправляет TLS ClientHello с нужным SNI
//! и смотрит ответ:
//!   * пришёл ServerHello (0x16)  → домен доступен;
//!   * RST / таймаут / TLS-alert  → вероятно режется DPI.
//!
//! Дополнительно умеет проверять метод `split` прямо из пользовательского
//! пространства (фрагментация ClientHello на два TCP-сегмента по SNI) —
//! это не требует прав администратора и WinDivert.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct ProbeReport {
    pub domain: String,
    pub direct_ok: bool,
    pub split_ok: bool,
    pub ms: u64,
    pub detail: String,
}

/// Строит TLS ClientHello и возвращает (байты записи, позиция начала SNI-имени).
fn build_client_hello(sni: &str) -> (Vec<u8>, usize) {
    let host = sni.as_bytes();

    // Тело расширения server_name (ServerNameList).
    let mut sni_body = Vec::new();
    sni_body.extend_from_slice(&(((host.len() + 3) as u16)).to_be_bytes()); // длина списка
    sni_body.push(0x00); // тип host_name
    sni_body.extend_from_slice(&(host.len() as u16).to_be_bytes());
    let sni_name_in_body = sni_body.len();
    sni_body.extend_from_slice(host);

    // Расширение server_name.
    let mut ext = Vec::new();
    ext.extend_from_slice(&0u16.to_be_bytes()); // тип расширения = server_name
    ext.extend_from_slice(&(sni_body.len() as u16).to_be_bytes());
    let ext_body_start = ext.len();
    ext.extend_from_slice(&sni_body);

    // Расширение ALPN (h2, http/1.1). Без него многие серверы и CDN
    // отвечают TLS-alert даже на доступный домен — probe ложно считал его
    // заблокированным. ALPN добавляется ПОСЛЕ server_name, поэтому позиция
    // SNI (sni_pos ниже) не меняется.
    {
        let protos: [&[u8]; 2] = [b"h2", b"http/1.1"];
        let mut list = Vec::new();
        for p in protos {
            list.push(p.len() as u8);
            list.extend_from_slice(p);
        }
        let mut alpn_body = Vec::new();
        alpn_body.extend_from_slice(&(list.len() as u16).to_be_bytes());
        alpn_body.extend_from_slice(&list);
        ext.extend_from_slice(&16u16.to_be_bytes()); // тип расширения = ALPN (0x0010)
        ext.extend_from_slice(&(alpn_body.len() as u16).to_be_bytes());
        ext.extend_from_slice(&alpn_body);
    }

    // Тело ClientHello.
    let cipher_bytes: Vec<u8> = vec![0xc0, 0x2f, 0xc0, 0x30, 0x00, 0x9c, 0x00, 0x2f];
    let mut hello = Vec::new();
    hello.extend_from_slice(&[0x03, 0x03]); // версия TLS 1.2
    hello.extend_from_slice(&[0x53u8; 32]); // random (фиксированный — для probe ок)
    hello.push(0x00); // session id len
    hello.extend_from_slice(&(cipher_bytes.len() as u16).to_be_bytes());
    hello.extend_from_slice(&cipher_bytes);
    hello.push(0x01); // compression methods len
    hello.push(0x00); // null
    hello.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    let hello_ext_start = hello.len();
    hello.extend_from_slice(&ext);

    // Заголовок рукопожатия.
    let mut hs = Vec::new();
    hs.push(0x01); // ClientHello
    let l = hello.len();
    hs.push((l >> 16) as u8);
    hs.push((l >> 8) as u8);
    hs.push(l as u8);
    let hs_body_start = hs.len();
    hs.extend_from_slice(&hello);

    // Заголовок записи TLS.
    let mut rec = Vec::new();
    rec.push(0x16); // handshake
    rec.extend_from_slice(&[0x03, 0x01]); // версия записи
    rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    let rec_hs_start = rec.len();
    rec.extend_from_slice(&hs);

    let sni_pos = rec_hs_start + hs_body_start + hello_ext_start + ext_body_start + sni_name_in_body;
    (rec, sni_pos)
}

fn resolve(domain: &str) -> Option<SocketAddr> {
    (domain, 443u16).to_socket_addrs().ok()?.next()
}

fn read_first_byte(stream: &mut TcpStream) -> (bool, u8) {
    let mut buf = [0u8; 8];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => (buf[0] == 0x16, buf[0]),
        _ => (false, 0),
    }
}

fn connect(addr: SocketAddr, timeout: Duration) -> Result<TcpStream, String> {
    let s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("TCP: {e}"))?;
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(timeout));
    Ok(s)
}

fn classify(ok: bool, b: u8) -> (bool, String) {
    if ok {
        (true, "ServerHello".into())
    } else if b == 0 {
        (false, "нет ответа / RST".into())
    } else if b == 0x15 {
        (false, "TLS alert".into())
    } else {
        (false, format!("байт 0x{:02x}", b))
    }
}

fn probe_direct(addr: SocketAddr, domain: &str, timeout: Duration) -> (bool, String) {
    let mut s = match connect(addr, timeout) {
        Ok(s) => s,
        Err(e) => return (false, e),
    };
    let (hello, _) = build_client_hello(domain);
    if s.write_all(&hello).is_err() {
        return (false, "ошибка отправки".into());
    }
    let (ok, b) = read_first_byte(&mut s);
    classify(ok, b)
}

fn probe_split(addr: SocketAddr, domain: &str, timeout: Duration) -> (bool, String) {
    let mut s = match connect(addr, timeout) {
        Ok(s) => s,
        Err(e) => return (false, e),
    };
    let _ = s.set_nodelay(true);
    let (hello, sni_pos) = build_client_hello(domain);
    let cut = sni_pos.min(hello.len().saturating_sub(1)).max(1);
    if s.write_all(&hello[..cut]).is_err() {
        return (false, "ошибка отправки (1)".into());
    }
    let _ = s.flush();
    std::thread::sleep(Duration::from_millis(20));
    if s.write_all(&hello[cut..]).is_err() {
        return (false, "ошибка отправки (2)".into());
    }
    let (ok, b) = read_first_byte(&mut s);
    classify(ok, b)
}

/// Полная проверка одного домена: сначала напрямую, при блокировке — split.
pub fn probe_domain(domain: &str) -> ProbeReport {
    let timeout = Duration::from_millis(3000);
    let start = Instant::now();

    let addr = match resolve(domain) {
        Some(a) => a,
        None => {
            return ProbeReport {
                domain: domain.into(),
                direct_ok: false,
                split_ok: false,
                ms: start.elapsed().as_millis() as u64,
                detail: "DNS не разрешается".into(),
            }
        }
    };

    let (direct_ok, d1) = probe_direct(addr, domain, timeout);
    if direct_ok {
        return ProbeReport {
            domain: domain.into(),
            direct_ok: true,
            split_ok: true,
            ms: start.elapsed().as_millis() as u64,
            detail: "доступен напрямую".into(),
        };
    }

    let (split_ok, d2) = probe_split(addr, domain, timeout);
    let detail = if split_ok {
        "обходится через split".into()
    } else {
        format!("заблокирован ({d1} / split: {d2})")
    };
    ProbeReport {
        domain: domain.into(),
        direct_ok,
        split_ok,
        ms: start.elapsed().as_millis() as u64,
        detail,
    }
}
