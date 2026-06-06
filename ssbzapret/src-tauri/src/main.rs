#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! SSBZapret — Tauri-бэкенд.
//!
//! Слои:
//!   * `engine`   — собственный движок обхода DPI (WinDivert на Windows,
//!                 симуляция на остальных ОС для разработки UI).
//!   * `store`    — хранилище пресетов и списков доменов (JSON в %APPDATA%).
//!   * `commands` — Tauri-команды, вызываемые из фронтенда.

mod commands;
mod dns;
mod engine;
mod store;

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

use engine::Engine;
use store::Store;

/// Общее состояние, доступное всем командам.
pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub engine: Arc<Engine>,
}

/// На Windows: если процесс запущен без прав администратора —
/// перезапускаем себя с запросом UAC и выходим. Без elevation драйвер
/// WinDivert (внутри winws) не загрузится.
#[cfg(windows)]
#[allow(non_snake_case)]
mod elevation {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    struct TokenElevationData {
        token_is_elevated: u32,
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION_CLASS: i32 = 20;
    const SW_SHOWNORMAL: i32 = 1;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(handle: isize) -> i32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn GetTokenInformation(
            token: isize,
            class: i32,
            info: *mut c_void,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            op: *const u16,
            file: *const u16,
            params: *const u16,
            dir: *const u16,
            show: i32,
        ) -> isize;
    }

    fn is_elevated() -> bool {
        unsafe {
            let mut token: isize = 0;
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return true; // не смогли проверить — продолжаем как есть
            }
            let mut data = TokenElevationData { token_is_elevated: 0 };
            let mut ret_len: u32 = 0;
            let ok = GetTokenInformation(
                token,
                TOKEN_ELEVATION_CLASS,
                &mut data as *mut _ as *mut c_void,
                std::mem::size_of::<TokenElevationData>() as u32,
                &mut ret_len,
            );
            CloseHandle(token);
            ok != 0 && data.token_is_elevated != 0
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Возвращает true, если можно продолжать (уже админ). Если прав нет —
    /// запускает себя заново с UAC и возвращает false (текущий процесс должен выйти).
    pub fn ensure() -> bool {
        if is_elevated() {
            return true;
        }
        if let Ok(exe) = std::env::current_exe() {
            let exe_w: Vec<u16> = exe
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let verb = wide("runas");
            unsafe {
                ShellExecuteW(
                    0,
                    verb.as_ptr(),
                    exe_w.as_ptr(),
                    ptr::null(),
                    ptr::null(),
                    SW_SHOWNORMAL,
                );
            }
        }
        false
    }
}

fn main() {
    #[cfg(windows)]
    {
        if !elevation::ensure() {
            return;
        }
    }

    let store = Arc::new(Mutex::new(Store::load()));
    let engine = Arc::new(Engine::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Автозапуск вместе с системой (включается из настроек).
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        // Автообновление через GitHub Releases (как в магазине приложений).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup({
            let engine = engine.clone();
            move |app| {
                // Движок получает AppHandle для live-событий.
                engine.attach(app.handle().clone());

                // Системный трей с меню «Показать / Выход».
                let show = MenuItem::with_id(app, "show", "Показать", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &quit])?;
                TrayIconBuilder::with_id("main")
                    .icon(app.default_window_icon().unwrap().clone())
                    .tooltip("SSBZapret")
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        _ => {}
                    })
                    .build(app)?;

                Ok(())
            }
        })
        .manage(AppState { store, engine })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_engine,
            commands::stop_engine,
            commands::list_presets,
            commands::get_active_preset,
            commands::set_active_preset,
            commands::save_preset,
            commands::delete_preset,
            commands::add_domain,
            commands::remove_domain,
            commands::set_strategy,
            commands::probe_domains,
            commands::autotune,
            commands::engine_ready,
            commands::list_dns_providers,
            commands::get_dns,
            commands::set_dns,
            commands::check_update,
            commands::install_update
        ])
        .run(tauri::generate_context!())
        .expect("ошибка при запуске SSBZapret");
}
