//! Движок на базе winws (из проекта Zapret).
//!
//! Мы не перехватываем пакеты сами, а запускаем проверенный winws.exe
//! с аргументами, собранными из активного пресета. winws приносит
//! свой WinDivert, поэтому велосипед не изобретаем.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::{Engine, LogLine, Stats};
use crate::store::{Preset, Rule, Strategy};

/// Перенаправляет поток вывода winws (stdout/stderr) в журнал приложения.
/// Без этого причина «обход не работает» (ошибка открытия hostlist,
/// проблема с WinDivert и т.п.) оставалась невидимой.
fn pipe_logs<R: std::io::Read + Send + 'static>(app: AppHandle, mut reader: R, level: &'static str) {
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        let emit = |bytes: &[u8]| {
            // winws (cygwin) пишет в CP866/CP1251 — читаем байты и декодируем
            // «без потерь», иначе BufReader::lines() обрывался на первой
            // не-UTF8 строке и журнал замолкал.
            let s = String::from_utf8_lossy(bytes);
            let s = s.trim();
            if !s.is_empty() {
                let _ = app.emit(
                    "engine://log",
                    LogLine { level: level.into(), tag: "winws".into(), msg: s.to_string() },
                );
            }
        };
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    for &b in &chunk[..n] {
                        if b == b'\n' {
                            emit(&line);
                            line.clear();
                        } else if b != b'\r' {
                            line.push(b);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        emit(&line);
    });
}

/// Не показывать консольное окно winws.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// desync-аргументы winws для секции TCP 443 (TLS).
fn desync_args_443(s: &Strategy) -> Vec<String> {
    let pos = match s.split_pos.as_str() {
        "sni" | "mid" | "" => "midsld".to_string(),
        other => other.to_string(),
    };
    let fooling = if s.fooling.is_empty() { "md5sig" } else { s.fooling.as_str() };
    let ttl = s.ttl.max(1);
    match s.method.as_str() {
        "split" => vec![
            "--dpi-desync=split2".into(),
            format!("--dpi-desync-split-pos={pos}"),
        ],
        "disorder" => vec![
            "--dpi-desync=disorder2".into(),
            format!("--dpi-desync-split-pos={pos}"),
        ],
        "fake" => vec![
            "--dpi-desync=fake".into(),
            format!("--dpi-desync-ttl={ttl}"),
            format!("--dpi-desync-fooling={fooling}"),
        ],
        // fakeddisorder и всё прочее — самый устойчивый комбинированный режим.
        _ => vec![
            "--dpi-desync=fake,disorder2".into(),
            format!("--dpi-desync-split-pos={pos}"),
            format!("--dpi-desync-ttl={ttl}"),
            format!("--dpi-desync-fooling={fooling}"),
        ],
    }
}

/// Полный набор аргументов winws: по одной chain-секции на каждое правило.
/// Разные правила объединяются через `--new` в одном процессе winws.
pub fn build_args(rules: &[(Rule, PathBuf)]) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let any_quic = rules.iter().any(|(r, _)| r.strategy.block_quic);

    // Окно фильтрации WinDivert (общее для всех правил).
    args.push("--wf-tcp=80,443".into());
    if any_quic {
        args.push("--wf-udp=443".into());
    }

    let mut first = true;
    for (rule, path) in rules {
        if rule.domains.is_empty() {
            continue;
        }
        let hl = path.to_string_lossy().to_string();

        // Секция TCP 443 (TLS) — стратегия конкретного правила.
        if !first {
            args.push("--new".into());
        }
        first = false;
        args.push("--filter-tcp=443".into());
        args.push(format!("--hostlist={hl}"));
        args.extend(desync_args_443(&rule.strategy));

        // Секция TCP 80 (HTTP).
        args.push("--new".into());
        args.push("--filter-tcp=80".into());
        args.push(format!("--hostlist={hl}"));
        args.push("--dpi-desync=fake,split2".into());
        args.push("--dpi-desync-split-pos=host+1".into());
        args.push("--dpi-desync-fooling=md5sig".into());

        // Секция UDP 443 (QUIC) — глушим, если правило просит.
        if rule.strategy.block_quic {
            args.push("--new".into());
            args.push("--filter-udp=443".into());
            args.push(format!("--hostlist={hl}"));
            args.push("--dpi-desync=fake".into());
            args.push("--dpi-desync-repeats=6".into());
        }
    }

    args
}

/// Папка для hostlist-файлов.
///
/// ВАЖНО: используем ASCII-путь %ProgramData%\SSBZapret, не зависящий от
/// имени пользователя. Если имя профиля содержит кириллицу (например
/// «Администратор»), winws (cygwin) не может открыть путь к hostlist —
/// домены не подхватываются и обход молча не срабатывает.
fn hostlist_dir() -> PathBuf {
    if let Ok(pd) = std::env::var("ProgramData") {
        if !pd.trim().is_empty() {
            let mut p = PathBuf::from(pd);
            p.push("SSBZapret");
            p.push("hostlists");
            return p;
        }
    }
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("SSBZapret");
    dir.push("hostlists");
    dir
}

/// Записывает hostlist правила в <hostlist_dir>/<preset>__<rule>.txt.
fn write_rule_hostlist(preset_id: &str, rule: &Rule) -> Result<PathBuf, String> {
    let mut dir = hostlist_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("не удалось создать папку hostlist: {e}"))?;
    dir.push(format!("{}__{}.txt", preset_id, rule.id));
    let body = rule.domains.join("\n");
    fs::write(&dir, body).map_err(|e| format!("не удалось записать hostlist: {e}"))?;
    Ok(dir)
}

/// Ищет winws.exe в нескольких возможных местах.
///
/// Tauri сохраняет префикс `resources/` из tauri.conf.json, поэтому
/// файл может лежать как в самой папке ресурсов, так и в её
/// подпапке resources/. Дополнительно ищем рядом с exe
/// (запуск из target/release без установки).
pub fn locate_winws(resource_dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![
        resource_dir.join("winws.exe"),
        resource_dir.join("resources").join("winws.exe"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            candidates.push(p.join("winws.exe"));
            candidates.push(p.join("resources").join("winws.exe"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

fn spawn(exe: &Path, args: &[String]) -> Result<Child, String> {
    let dir = exe.parent().unwrap_or(Path::new("."));
    Command::new(exe)
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("не удалось запустить winws.exe: {e}"))
}

/// Синхронно убивает дерево процессов winws по PID (вызывается при выходе/остановке).
pub fn kill_pid(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Основной запуск: держит winws живым, пока не выставлен stop_flag.
pub fn run(engine: &Engine, preset: &Preset, stop_flag: &AtomicBool) -> Result<(), String> {
    let dir = engine
        .resource_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    let exe = locate_winws(&dir).ok_or_else(|| {
        "winws.exe не найде��. Положите winws.exe и файлы WinDivert в src-tauri/resources/ и пересоберите, либо скопируйте их рядом с ssbzapret.exe."
            .to_string()
    })?;
    engine.log("inf", "winws", &format!("winws.exe: {}", exe.display()));
    let rules = preset.effective_rules();
    let mut pairs: Vec<(Rule, PathBuf)> = Vec::new();
    for r in &rules {
        let path = write_rule_hostlist(&preset.id, r)?;
        pairs.push((r.clone(), path));
    }
    let total_domains: usize = rules.iter().map(|r| r.domains.len()).sum();
    let args = build_args(&pairs);
    engine.log("inf", "winws", &format!("правил: {} · доменов: {}", rules.len(), total_domains));
    engine.log("inf", "winws", &format!("аргументы: winws {}", args.join(" ")));

    let mut child = spawn(&exe, &args)?;
    let pid = child.id();
    engine.set_child_pid(Some(pid));
    engine.log("ok", "winws", &format!("winws запущен (PID {})", pid));

    // Транслируем вывод winws в журнал приложения (диагностика обхода).
    if let Some(app) = engine.app_handle() {
        if let Some(out) = child.stdout.take() {
            pipe_logs(app.clone(), out, "inf");
        }
        if let Some(err) = child.stderr.take() {
            pipe_logs(app, err, "warn");
        }
    }

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            engine.clear_child_pid_if(pid);
            engine.log("warn", "winws", "winws остановлен");
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                engine.clear_child_pid_if(pid);
                engine.log("err", "winws", &format!("winws завершился неожиданно: {status}"));
                return Err(format!("winws завершился ({status})"));
            }
            Ok(None) => {}
            Err(e) => {
                engine.clear_child_pid_if(pid);
                return Err(format!("ошибка ожидания winws: {e}"));
            }
        }
        // Реальный per-packet учёт идёт внутри winws и наружу не выдаётся,
        // поэтому фейковые pps/Mbit не эмитим — отдаём только число активных
        // доменов; дашборд показывает «активно» вместо нулей.
        engine.emit_runtime_stats(Stats {
            pkt_s: 0,
            processed: 0,
            active_domains: total_domains,
            mbit: 0,
        });
        thread::sleep(Duration::from_millis(1000));
    }
    Ok(())
}
