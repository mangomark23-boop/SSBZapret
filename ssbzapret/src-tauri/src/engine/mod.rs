//! Движок обхода DPI.
//!
//! На Windows реальный перехват идёт через WinDivert (`windivert.rs`).
//! На остальных ОС работает симулятор — чтобы UI можно было
//! разрабатывать без драйвера.

pub mod desync;
pub mod tls;
pub mod probe;
#[cfg(windows)]
#[allow(dead_code)]
mod windivert;
#[cfg(windows)]
pub mod winws;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::store::Preset;

pub use desync::Method;

#[derive(Clone, Serialize)]
pub struct Status {
    pub running: bool,
    pub uptime_secs: u64,
    pub preset_id: String,
}

#[derive(Clone, Serialize)]
pub struct Stats {
    pub pkt_s: u64,
    pub processed: u64,
    pub active_domains: usize,
    pub mbit: u64,
}

#[derive(Clone, Serialize)]
pub struct LogLine {
    pub level: String, // ok | warn | err | inf
    pub tag: String,
    pub msg: String,
}

pub struct Engine {
    running: AtomicBool,
    processed: AtomicU64,
    started_at: Mutex<Option<Instant>>,
    preset: Mutex<Option<Preset>>,
    app: Mutex<Option<AppHandle>>,
    stop_flag: Arc<AtomicBool>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            running: AtomicBool::new(false),
            processed: AtomicU64::new(0),
            started_at: Mutex::new(None),
            preset: Mutex::new(None),
            app: Mutex::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn attach(&self, app: AppHandle) {
        *self.app.lock() = Some(app);
    }

    /// Папка ресурсов приложения (где лежат WinDivert.dll / .sys).
    #[cfg(windows)]
    pub fn resource_dir(&self) -> Option<std::path::PathBuf> {
        use tauri::Manager;
        self.app
            .lock()
            .as_ref()
            .and_then(|a| a.path().resource_dir().ok())
    }

    pub fn log(&self, level: &str, tag: &str, msg: &str) {
        if let Some(app) = self.app.lock().as_ref() {
            let _ = app.emit(
                "engine://log",
                LogLine { level: level.into(), tag: tag.into(), msg: msg.into() },
            );
        }
    }

    fn emit_status(&self) {
        if let Some(app) = self.app.lock().as_ref() {
            let _ = app.emit("engine://status", self.status());
        }
    }

    pub fn emit_runtime_stats(&self, s: Stats) {
        if let Some(app) = self.app.lock().as_ref() {
            let _ = app.emit("engine://stats", s);
        }
    }

    pub fn note_processed(&self, n: u64) {
        self.processed.fetch_add(n, Ordering::Relaxed);
    }

    pub fn total_processed(&self) -> u64 {
        self.processed.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> Status {
        let uptime = self
            .started_at
            .lock()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        Status {
            running: self.running.load(Ordering::SeqCst),
            uptime_secs: uptime,
            preset_id: self
                .preset
                .lock()
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_default(),
        }
    }

    /// (Пере)запуск движка с указанным пресетом.
    pub fn start(self: &Arc<Self>, preset: Preset) {
        self.stop(); // гарантируем, что предыдущий воркер остановлен
        *self.preset.lock() = Some(preset.clone());
        *self.started_at.lock() = Some(Instant::now());
        self.running.store(true, Ordering::SeqCst);
        self.stop_flag.store(false, Ordering::SeqCst);
        self.emit_status();
        self.log(
            "ok",
            "engine",
            &format!(
                "движок запущен · пресет {} · метод {}",
                preset.name, preset.strategy.method
            ),
        );

        let this = self.clone();
        let stop_flag = self.stop_flag.clone();
        thread::spawn(move || {
            this.worker(preset, stop_flag);
        });
    }

    pub fn stop(&self) {
        if self.running.swap(false, Ordering::SeqCst) {
            self.stop_flag.store(true, Ordering::SeqCst);
            *self.started_at.lock() = None;
            self.emit_status();
            self.log("warn", "engine", "движок остановлен");
        }
    }

    /// Горячее обновление списка доменов/стратегии без полного рестарта.
    pub fn hot_update(self: &Arc<Self>, preset: Preset) {
        let running = self.running.load(Ordering::SeqCst);
        *self.preset.lock() = Some(preset.clone());
        if running {
            self.log(
                "inf",
                "engine",
                &format!("обновлён список доменов ({})", preset.domains.len()),
            );
        }
    }

    fn worker(self: Arc<Self>, preset: Preset, stop_flag: Arc<AtomicBool>) {
        #[cfg(windows)]
        {
            if let Err(e) = winws::run(self.as_ref(), &preset, &stop_flag) {
                self.log("err", "winws", &format!("{e}"));
                self.running.store(false, Ordering::SeqCst);
                self.emit_status();
            }
        }
        #[cfg(not(windows))]
        {
            self.simulate(&preset, &stop_flag);
        }
    }

    /// Dev-симуляция для не-Windows хостов (реальный перехват — только Windows).
    #[cfg(not(windows))]
    fn simulate(&self, preset: &Preset, stop_flag: &AtomicBool) {
        use std::time::Duration;
        let mut tick = 0u64;
        while !stop_flag.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(1000));
            tick += 1;
            let pps = 900 + (tick * 37) % 600;
            self.note_processed(pps);
            self.emit_runtime_stats(Stats {
                pkt_s: pps,
                processed: self.total_processed(),
                active_domains: preset.domains.len(),
                mbit: 70 + (tick % 25),
            });
            if tick % 3 == 0 && !preset.domains.is_empty() {
                let d = &preset.domains[(tick as usize) % preset.domains.len()];
                self.log("ok", "tls", &format!("{} ClientHello → {} · OK", preset.strategy.method, d));
            }
        }
    }
}
