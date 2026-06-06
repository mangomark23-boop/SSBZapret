//! Перехват пакетов через WinDivert (только Windows).
//!
//! Здесь происходит реальный обход DPI: мы ловим исходящий TLS
//! ClientHello, смотрим SNI и, если домен в активном списке, применяем
//! desync. Остальной трафик проходит насквозь без изменений.
//!
//! WinDivert.dll и WinDivert64.sys подгружаются ДИНАМИЧЕСКИ во время
//! работы, поэтому для сборки SDK WinDivert НЕ нужен — достаточно
//! положить эти два файла в папку resources рядом с приложением.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use libloading::Library;

use crate::engine::{desync, tls, Engine, Stats};
use crate::store::Preset;
use desync::Method;

// ---- FFI подписи WinDivert 2.x ----

type Handle = *mut c_void;
type Bool = i32;

type OpenFn = unsafe extern "system" fn(*const c_char, c_int, i16, u64) -> Handle;
type RecvFn =
    unsafe extern "system" fn(Handle, *mut c_void, u32, *mut u32, *mut WdAddress) -> Bool;
type SendFn =
    unsafe extern "system" fn(Handle, *const c_void, u32, *mut u32, *const WdAddress) -> Bool;
type CloseFn = unsafe extern "system" fn(Handle) -> Bool;

const LAYER_NETWORK: c_int = 0;

/// WINDIVERT_ADDRESS — 80 байт в WinDivert 2.x. Нам не нужно разбирать
/// поля — мы просто получаем её от recv и отдаём обратно в send.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct WdAddress {
    raw: [u8; 80],
}

struct Wd {
    _lib: Library,
    open: OpenFn,
    recv: RecvFn,
    send: SendFn,
    close: CloseFn,
}

impl Wd {
    fn load(path: &PathBuf) -> Result<Wd, String> {
        unsafe {
            let lib = Library::new(path).map_err(|e| format!("не удалось загрузить DLL: {e}"))?;
            let open = *lib
                .get::<OpenFn>(b"WinDivertOpen\0")
                .map_err(|e| format!("WinDivertOpen: {e}"))?;
            let recv = *lib
                .get::<RecvFn>(b"WinDivertRecv\0")
                .map_err(|e| format!("WinDivertRecv: {e}"))?;
            let send = *lib
                .get::<SendFn>(b"WinDivertSend\0")
                .map_err(|e| format!("WinDivertSend: {e}"))?;
            let close = *lib
                .get::<CloseFn>(b"WinDivertClose\0")
                .map_err(|e| format!("WinDivertClose: {e}"))?;
            Ok(Wd { _lib: lib, open, recv, send, close })
        }
    }
}

/// Ищем WinDivert.dll рядом с exe / в resources / в ресурсах Tauri.
fn find_dll(extra: &[PathBuf]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
            dirs.push(d.join("resources"));
        }
    }
    for e in extra {
        dirs.push(e.clone());
        dirs.push(e.join("resources"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("resources"));
        dirs.push(cwd.join("src-tauri").join("resources"));
        dirs.push(cwd);
    }
    for d in dirs {
        let p = d.join("WinDivert.dll");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Вырезает TCP-payload из IPv4-пакета.
fn tcp_payload(pkt: &[u8]) -> Option<&[u8]> {
    if pkt.len() < 20 || (pkt[0] >> 4) != 4 {
        return None;
    }
    let ihl = ((pkt[0] & 0x0f) as usize) * 4;
    if pkt.get(9) != Some(&6) {
        return None; // не TCP
    }
    if pkt.len() < ihl + 20 {
        return None;
    }
    let data_off = (((pkt[ihl + 12] >> 4) & 0x0f) as usize) * 4;
    let start = ihl + data_off;
    if pkt.len() <= start {
        return None;
    }
    Some(&pkt[start..])
}

fn set_ttl(pkt: &mut [u8], ttl: u8) {
    if pkt.len() > 8 {
        pkt[8] = ttl;
        desync::ipv4_checksum(pkt);
    }
}

pub fn run(engine: &Engine, preset: &Preset, stop_flag: &AtomicBool) -> Result<(), String> {
    let extra: Vec<PathBuf> = engine.resource_dir().into_iter().collect();
    let dll = find_dll(&extra).ok_or_else(|| {
        "не найден WinDivert.dll — положите WinDivert.dll и WinDivert64.sys в папку resources рядом с приложением"
            .to_string()
    })?;
    engine.log("inf", "windivert", &format!("загружаю {}", dll.display()));
    let wd = Wd::load(&dll)?;

    // Ловим исходящий TCP к 80/443. Если включён block_quic — добавляем UDP 443 (QUIC).
    let block_quic = preset.strategy.block_quic;
    let filter_str = if block_quic {
        "outbound and ip and ((tcp and (tcp.DstPort == 443 or tcp.DstPort == 80)) or (udp and udp.DstPort == 443))"
    } else {
        "outbound and ip and tcp and (tcp.DstPort == 443 or tcp.DstPort == 80)"
    };
    let filter = CString::new(filter_str).unwrap();
    let handle = unsafe { (wd.open)(filter.as_ptr(), LAYER_NETWORK, 0, 0) };
    let invalid = usize::MAX as Handle;
    if handle.is_null() || handle == invalid {
        return Err(
            "WinDivertOpen не удался — запустите от имени администратора и проверьте, что WinDivert64.sys лежит рядом с WinDivert.dll"
                .into(),
        );
    }
    engine.log(
        "ok",
        "windivert",
        if block_quic {
            "драйвер загружен · перехват 80/443 + QUIC(UDP 443)"
        } else {
            "драйвер загружен · перехват 80/443"
        },
    );

    let method = Method::parse(&preset.strategy.method);
    let fake_ttl = preset.strategy.ttl;
    let mut quic_dropped: u64 = 0;
    let mut quic_logged = false;

    let mut buf = vec![0u8; 65535];
    let mut addr = WdAddress { raw: [0u8; 80] };
    let mut window_pkts: u64 = 0;
    let mut last_tick = Instant::now();

    while !stop_flag.load(Ordering::SeqCst) {
        let mut recv_len: u32 = 0;
        let ok = unsafe {
            (wd.recv)(
                handle,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
                &mut recv_len,
                &mut addr,
            )
        };
        if ok == 0 || recv_len == 0 {
            continue;
        }
        let len = recv_len as usize;
        let mut handled = false;

        // QUIC: дропаем исходящий UDP 443, чтобы браузер откатился на TCP/TLS.
        if block_quic && buf.get(9) == Some(&17) {
            quic_dropped += 1;
            if !quic_logged {
                engine.log("ok", "quic", "блокирую QUIC (UDP 443) → откат на TCP");
                quic_logged = true;
            }
            engine.note_processed(1);
            handled = true; // не реинжектим — пакет уничтожается
        }

        if let Some(host) = tcp_payload(&buf[..len]).and_then(tls::extract_sni) {
            if preset.domains.iter().any(|d| tls::host_matches(&host, d)) {
                let split_at = tcp_payload(&buf[..len])
                    .and_then(tls::sni_offset)
                    .or_else(|| tcp_payload(&buf[..len]).map(|p| p.len() / 2))
                    .unwrap_or(1);
                let plan = desync::build_plan(&buf[..len], split_at, method, fake_ttl);
                for op in plan {
                    let mut bytes = op.bytes;
                    if let Some(ttl) = op.ttl {
                        set_ttl(&mut bytes, ttl);
                    }
                    let mut send_len: u32 = 0;
                    unsafe {
                        (wd.send)(
                            handle,
                            bytes.as_ptr() as *const c_void,
                            bytes.len() as u32,
                            &mut send_len,
                            &addr,
                        );
                    }
                }
                engine.log("ok", "tls", &format!("{:?} → {host}", method));
                engine.note_processed(1);
                handled = true;
            }
        }

        if !handled {
            // Не наш домен — пропускаем без изменений.
            let mut send_len: u32 = 0;
            unsafe {
                (wd.send)(
                    handle,
                    buf.as_ptr() as *const c_void,
                    len as u32,
                    &mut send_len,
                    &addr,
                );
            }
        }

        window_pkts += 1;
        if last_tick.elapsed() >= Duration::from_secs(1) {
            engine.emit_runtime_stats(Stats {
                pkt_s: window_pkts,
                processed: engine.total_processed(),
                active_domains: preset.domains.len(),
                mbit: 0,
            });
            window_pkts = 0;
            last_tick = Instant::now();
        }
    }

    if block_quic && quic_dropped > 0 {
        engine.log("inf", "quic", &format!("всего заблокировано QUIC-пакетов: {quic_dropped}"));
    }
    unsafe {
        (wd.close)(handle);
    }
    Ok(())
}
