//! Техники desync, применяемые к первому исходящему TLS ClientHello.
//!
//! Каждая функция получает исходный IPv4+TCP пакет (как его отдаёт
//! WinDivert) и возвращает упорядоченный список пакетов для повторной
//! инжекции. Работаем на уровне байтов (без внешних крейтов), чтобы
//! модуль компилировался на любой ОС.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// Разрезать ClientHello на два TCP-сегмента (по SNI).
    Split,
    /// Отправить fake-сегмент с низким TTL, затем реальные данные.
    Fake,
    /// Отправить сегменты в обратном порядке (DPI теряет контекст).
    Disorder,
    /// fake + disorder — самый устойчивый вариант.
    FakedDisorder,
}

impl Method {
    pub fn parse(s: &str) -> Method {
        match s {
            "split" => Method::Split,
            "fake" => Method::Fake,
            "disorder" => Method::Disorder,
            _ => Method::FakedDisorder,
        }
    }
}

/// Пакет к отправке + опциональный TTL (для fake-пакетов).
pub struct OutPacket {
    pub bytes: Vec<u8>,
    pub ttl: Option<u8>,
}

fn read_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Пересчёт контрольной суммы IPv4-заголовка.
pub fn ipv4_checksum(pkt: &mut [u8]) {
    let ihl = ((pkt[0] & 0x0f) as usize) * 4;
    pkt[10] = 0;
    pkt[11] = 0;
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < ihl {
        sum += u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
        i += 2;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let c = !(sum as u16);
    pkt[10..12].copy_from_slice(&c.to_be_bytes());
}

/// Пересчёт контрольной суммы TCP (с учётом псевдо-заголовка).
pub fn tcp_checksum(pkt: &mut [u8]) {
    let ihl = ((pkt[0] & 0x0f) as usize) * 4;
    let total = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
    if total <= ihl || total > pkt.len() {
        return;
    }
    let tcp_len = total - ihl;
    pkt[ihl + 16] = 0;
    pkt[ihl + 17] = 0;
    let mut sum = 0u32;
    // псевдо-заголовок: src(4) + dst(4)
    let mut i = 12;
    while i < 20 {
        sum += u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
        i += 2;
    }
    sum += 6u32; // protocol = TCP
    sum += tcp_len as u32;
    let mut j = ihl;
    while j + 1 < ihl + tcp_len {
        sum += u16::from_be_bytes([pkt[j], pkt[j + 1]]) as u32;
        j += 2;
    }
    if tcp_len & 1 == 1 {
        sum += (pkt[ihl + tcp_len - 1] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let c = !(sum as u16);
    pkt[ihl + 16..ihl + 18].copy_from_slice(&c.to_be_bytes());
}

/// Собирает TCP-сегмент с payload[start..end], корректируя seq, длину и чексуммы.
fn build_segment(orig: &[u8], start: usize, end: usize) -> Vec<u8> {
    let ihl = ((orig[0] & 0x0f) as usize) * 4;
    let data_off = (((orig[ihl + 12] >> 4) & 0x0f) as usize) * 4;
    let hdr_len = ihl + data_off;
    let payload = &orig[hdr_len..];
    let seg = &payload[start..end.min(payload.len())];

    let mut pkt = Vec::with_capacity(hdr_len + seg.len());
    pkt.extend_from_slice(&orig[..hdr_len]);
    pkt.extend_from_slice(seg);

    let total = (hdr_len + seg.len()) as u16;
    pkt[2..4].copy_from_slice(&total.to_be_bytes());

    let seq = read_u32(&orig[ihl + 4..ihl + 8]).wrapping_add(start as u32);
    pkt[ihl + 4..ihl + 8].copy_from_slice(&seq.to_be_bytes());

    ipv4_checksum(&mut pkt);
    tcp_checksum(&mut pkt);
    pkt
}

/// Строит план отправки для выбранного метода desync.
pub fn build_plan(orig: &[u8], split_at: usize, method: Method, fake_ttl: u8) -> Vec<OutPacket> {
    let ihl = ((orig[0] & 0x0f) as usize) * 4;
    let data_off = (((orig[ihl + 12] >> 4) & 0x0f) as usize) * 4;
    let hdr_len = ihl + data_off;
    let payload_len = orig.len().saturating_sub(hdr_len);
    if payload_len < 2 {
        return vec![OutPacket { bytes: orig.to_vec(), ttl: None }];
    }
    let cut = split_at.clamp(1, payload_len - 1);

    let seg1 = build_segment(orig, 0, cut);
    let seg2 = build_segment(orig, cut, payload_len);

    let make_fake = || {
        let mut f = build_segment(orig, 0, cut);
        // Портим payload: DPI увидит мусор, но из-за низкого TTL пакет
        // не дойдёт до реального сервера.
        for b in f[hdr_len..].iter_mut() {
            *b = 0x00;
        }
        ipv4_checksum(&mut f);
        tcp_checksum(&mut f);
        OutPacket { bytes: f, ttl: Some(fake_ttl) }
    };

    match method {
        Method::Split => vec![
            OutPacket { bytes: seg1, ttl: None },
            OutPacket { bytes: seg2, ttl: None },
        ],
        Method::Fake => vec![
            make_fake(),
            OutPacket { bytes: seg1, ttl: None },
            OutPacket { bytes: seg2, ttl: None },
        ],
        Method::Disorder => vec![
            OutPacket { bytes: seg2, ttl: None },
            OutPacket { bytes: seg1, ttl: None },
        ],
        Method::FakedDisorder => vec![
            make_fake(),
            OutPacket { bytes: seg2, ttl: None },
            OutPacket { bytes: seg1, ttl: None },
        ],
    }
}
