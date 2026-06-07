//! Tauri-команды, вызываемые из фронтенда через `invoke`.

use tauri::State;

use crate::engine::Status;
use crate::store::{Preset, Strategy};
use crate::AppState;
use crate::engine::probe::{probe_domain, ProbeReport};
use std::thread;
use serde::Serialize;

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Status {
    state.engine.status()
}

#[tauri::command(rename_all = "snake_case")]
pub fn start_engine(preset_id: String, state: State<AppState>) -> Result<(), String> {
    let preset = {
        let store = state.store.lock();
        store
            .find(&preset_id)
            .cloned()
            .ok_or_else(|| format!("пресет {preset_id} не найден"))?
    };
    {
        let mut store = state.store.lock();
        store.active_id = preset_id.clone();
        store.save();
    }
    state.engine.start(preset);
    Ok(())
}

#[tauri::command]
pub fn stop_engine(state: State<AppState>) {
    state.engine.stop();
}

#[tauri::command]
pub fn list_presets(state: State<AppState>) -> Vec<Preset> {
    state.store.lock().presets.clone()
}

#[tauri::command]
pub fn get_active_preset(state: State<AppState>) -> String {
    state.store.lock().active_id.clone()
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_active_preset(preset_id: String, state: State<AppState>) -> Result<(), String> {
    let preset = {
        let mut store = state.store.lock();
        let p = store
            .find(&preset_id)
            .cloned()
            .ok_or_else(|| format!("пресет {preset_id} не найден"))?;
        store.active_id = preset_id.clone();
        store.save();
        p
    };
    // hot_update перезапустит winws, если движок сейчас работает.
    state.engine.hot_update(preset);
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_preset(preset: Preset, state: State<AppState>) -> Result<(), String> {
    let mut store = state.store.lock();
    if let Some(existing) = store.find_mut(&preset.id) {
        if existing.builtin {
            // У встроенных можно менять домены/стратегию, но не identity.
            existing.domains = preset.domains;
            existing.strategy = preset.strategy;
            existing.services = preset.services;
        } else {
            *existing = preset;
        }
    } else {
        store.presets.push(preset);
    }
    store.save();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_preset(preset_id: String, state: State<AppState>) -> Result<(), String> {
    let mut store = state.store.lock();
    if let Some(p) = store.find(&preset_id) {
        if p.builtin {
            return Err("встроенный пресет нельзя удалить".into());
        }
    }
    store.presets.retain(|p| p.id != preset_id);
    if store.active_id == preset_id {
        store.active_id = "ai_bypass".into();
    }
    store.save();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn add_domain(
    preset_id: String,
    domain: String,
    state: State<AppState>,
) -> Result<Vec<String>, String> {
    let (domains, updated) = {
        let mut store = state.store.lock();
        let p = store.find_mut(&preset_id).ok_or("пресет не найден")?;
        let d = domain.trim().to_lowercase();
        if !d.is_empty() && !p.domains.contains(&d) {
            p.domains.push(d);
        }
        let domains = p.domains.clone();
        let updated = p.clone();
        store.save();
        (domains, updated)
    };
    state.engine.hot_update(updated);
    Ok(domains)
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_domain(
    preset_id: String,
    domain: String,
    state: State<AppState>,
) -> Result<Vec<String>, String> {
    let (domains, updated) = {
        let mut store = state.store.lock();
        let p = store.find_mut(&preset_id).ok_or("пресет не найден")?;
        p.domains.retain(|x| x != &domain);
        let domains = p.domains.clone();
        let updated = p.clone();
        store.save();
        (domains, updated)
    };
    state.engine.hot_update(updated);
    Ok(domains)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_strategy(
    preset_id: String,
    strategy: Strategy,
    state: State<AppState>,
) -> Result<(), String> {
    let updated = {
        let mut store = state.store.lock();
        let p = store.find_mut(&preset_id).ok_or("пресет не найден")?;
        p.strategy = strategy;
        let updated = p.clone();
        store.save();
        updated
    };
    state.engine.hot_update(updated);
    Ok(())
}

/// Авто-тест: проверяет доступность доменов (напрямую и через split).
/// Пробы идут параллельно, без драйвера и без прав администратора.
#[tauri::command(rename_all = "snake_case")]
pub fn probe_domains(domains: Vec<String>) -> Vec<ProbeReport> {
    let handles: Vec<_> = domains
        .into_iter()
        .take(8)
        .map(|d| thread::spawn(move || probe_domain(&d)))
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}


#[derive(Clone, Serialize)]
pub struct AutotuneRow {
    pub id: String,
    pub name: String,
    pub ok: usize,
    pub total: usize,
    pub applied: bool,
}

#[cfg(windows)]
fn winws_present(state: &State<AppState>) -> bool {
    let dir = state
        .engine
        .resource_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    crate::engine::winws::locate_winws(&dir).is_some()
}

/// Готов ли движок к работе (на Windows — есть ли winws.exe в ресурсах).
#[tauri::command]
pub fn engine_ready(state: State<AppState>) -> bool {
    #[cfg(windows)]
    {
        winws_present(&state)
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        false
    }
}

/// Автоподбор: по очереди запускает winws с разными профилями обхода,
/// проверяет домены пресета и оставляет лучший работающий профиль.
#[tauri::command(rename_all = "snake_case")]
pub fn autotune(preset_id: String, state: State<AppState>) -> Result<Vec<AutotuneRow>, String> {
    #[cfg(windows)]
    {
        if !winws_present(&state) {
            return Err(
                "winws.exe не найден в resources. Скопируйте winws и файлы WinDivert из дистрибутива Zapret."
                    .into(),
            );
        }
    }

    // Запоминаем состояние до автоподбора, чтобы корректно восстановить его
    // в конце (раньше движок оставался выключенным, а активный пресет —
    // подменённым временной выборкой).
    let was_running = state.engine.status().running;
    let prev_active = state.store.lock().active_id.clone();

    let base = {
        let store = state.store.lock();
        store
            .find(&preset_id)
            .cloned()
            .ok_or_else(|| format!("пресет {preset_id} не найден"))?
    };
    // Берём до 2 доменов с каждого правила (а не первые 4 из одного),
    // чтобы покрыть все сервисы пресета «Всё сразу» (YouTube, Discord, игры).
    let sample: Vec<String> = {
        let mut s: Vec<String> = Vec::new();
        for r in base.effective_rules() {
            for d in r.domains.iter().take(2) {
                if !s.contains(d) {
                    s.push(d.clone());
                }
                if s.len() >= 10 {
                    break;
                }
            }
            if s.len() >= 10 {
                break;
            }
        }
        s
    };
    if sample.is_empty() {
        return Err("в пресете нет доменов для проверки".into());
    }

    let candidates = crate::store::candidate_strategies();
    let mut rows: Vec<AutotuneRow> = Vec::new();
    let mut best_idx: Option<usize> = None;
    let mut best_ok: usize = 0;

    for (i, (id, name, strat)) in candidates.iter().enumerate() {
        let mut p = base.clone();
        p.rules = vec![];
        p.domains = sample.clone();
        p.strategy = strat.clone();
        state.engine.log("inf", "autotune", &format!("проверка профиля: {name}"));
        state.engine.start(p);
        // Даём winws и драйверу WinDivert время полностью подняться,
        // иначе первые пробы ложно «не проходят».
        thread::sleep(std::time::Duration::from_millis(3500));
        let ok = sample
            .iter()
            .filter(|d| {
                if probe_domain(d).direct_ok {
                    return true;
                }
                // одна повторная попытка — драйвер мог ещё прогреваться
                std::thread::sleep(std::time::Duration::from_millis(250));
                probe_domain(d).direct_ok
            })
            .count();
        let level = if ok == sample.len() {
            "ok"
        } else if ok > 0 {
            "warn"
        } else {
            "err"
        };
        state.engine.log(
            level,
            "autotune",
            &format!("{name}: {ok}/{} доменов прошли", sample.len()),
        );
        rows.push(AutotuneRow {
            id: id.clone(),
            name: name.clone(),
            ok,
            total: sample.len(),
            applied: false,
        });
        if ok > best_ok {
            best_ok = ok;
            best_idx = Some(i);
        }
    }

    state.engine.stop();

    if let Some(i) = best_idx {
        if best_ok > 0 {
            let (id, name, strat) = &candidates[i];
            {
                let mut store = state.store.lock();
                if let Some(p) = store.find_mut(&preset_id) {
                    if p.rules.is_empty() {
                        p.strategy = strat.clone();
                    } else {
                        for r in p.rules.iter_mut() {
                            r.strategy = strat.clone();
                        }
                    }
                }
                store.save();
            }
            state.engine.log(
                "ok",
                "autotune",
                &format!("выбран профиль: {name} ({best_ok}/{})", sample.len()),
            );
            if let Some(row) = rows.iter_mut().find(|r| &r.id == id) {
                row.applied = true;
            }
        }
    }

    // Восстанавливаем движок: если он работал до автоподбора — запускаем
    // заново ранее активный пресет (уже с подобранной стратегией, если это
    // он и был); иначе просто синхронизируем активный пресет на дашборде.
    let restore = {
        let store = state.store.lock();
        store.find(&prev_active).cloned()
    };
    if let Some(p) = restore {
        if was_running {
            state.engine.start(p);
        } else {
            state.engine.hot_update(p);
        }
    }

    Ok(rows)
}

/// Список доступных DNS-провайдеров для разблокировки ИИ.
#[tauri::command]
pub fn list_dns_providers() -> Vec<crate::store::DnsProvider> {
    crate::store::dns_providers()
}

/// Текущий выбранный DNS-провайдер ("off" = системный).
#[tauri::command]
pub fn get_dns(state: State<AppState>) -> String {
    state.store.lock().dns_provider.clone()
}

/// Переключить системный DNS на выбранного провайдера (или сбросить при "off").
#[tauri::command(rename_all = "snake_case")]
pub fn set_dns(provider_id: String, state: State<AppState>) -> Result<(), String> {
    // Все известные IP провайдеров — чтобы при сбросе/переключении снять
    // ранее прописанные DoH-шаблоны (иначе зашифрованный DNS ломает VPN).
    let all_ips: Vec<String> = crate::store::dns_providers()
        .into_iter()
        .flat_map(|p| p.ips)
        .collect();
    if provider_id == "off" {
        crate::dns::clear(&all_ips)?;
        {
            let mut store = state.store.lock();
            store.dns_provider = "off".into();
            store.save();
        }
        state
            .engine
            .log("inf", "dns", "системный DNS сброшен на автоматический (DHCP)");
        return Ok(());
    }
    let prov = crate::store::dns_providers()
        .into_iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| format!("DNS-провайдер {provider_id} не найден"))?;
    // Сначала снимаем прежние настройки (адреса + DoH), затем применяем новые —
    // не накапливаем устаревшие DoH-шаблоны от других провайдеров.
    crate::dns::clear(&all_ips)?;
    crate::dns::apply(&prov.ips, &prov.doh)?;
    {
        let mut store = state.store.lock();
        store.dns_provider = provider_id.clone();
        store.save();
    }
    state.engine.log(
        "ok",
        "dns",
        &format!("DNS переключён на {} ({})", prov.name, prov.ips.join(", ")),
    );
    Ok(())
}

/// Информация о доступном обновлении (для фронтенда).
#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
}

/// Проверяет наличие обновления на GitHub Releases.
/// Возвращает None, если обновлений нет (или обновлятель не настроен).
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Скачивает и устанавливает обновление, затем перезапускает приложение.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "обновлений нет".to_string())?;
    update
        .download_and_install(|_downloaded, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}
