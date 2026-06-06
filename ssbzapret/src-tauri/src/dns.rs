//! Системный переключатель DNS.
//!
//! Многие ИИ-сервисы (ChatGPT, Claude, Gemini, Copilot) блокируются не на
//! уровне DPI, а по IP/геолокации. Обойти это desync’ом нельзя — нужен
//! «умный» DNS (Comss.one, Xbox DNS и т.п.), который резолвит такие домены
//! на свои прокси. Здесь мы переключаем системный DNS на выбранный провайдер
//! и при необходимости регистрируем DoH-шаблон (Windows 11), чтобы запросы
//! шли по HTTPS и не подменялись провайдером.
//!
//! Требуются права администратора (приложение уже само повышает права).
//! На не-Windows платформах функции — заглушки (для разработки UI).

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn run_ps(script: &str) -> Result<(), String> {
        let out = Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .map_err(|e| format!("не удалось запустить powershell: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("powershell завершился с ошибкой: {}", err.trim()))
        }
    }

    /// Применить DNS-серверы провайдера ко всем активным физическим адаптерам.
    pub fn apply(ips: &[String], doh: &str) -> Result<(), String> {
        if ips.is_empty() {
            return Err("для провайдера не заданы адреса DNS".into());
        }
        let ip_list = ips
            .iter()
            .map(|i| format!("'{}'", i))
            .collect::<Vec<_>>()
            .join(",");
        // DoH-шаблоны (Windows 11): включаем шифрование запросов.
        let mut doh_block = String::new();
        if !doh.is_empty() {
            for ip in ips {
                doh_block.push_str(&format!(
                    "netsh dns delete encryption server={ip} >$null 2>&1; netsh dns add encryption server={ip} dohtemplate={doh} autoupgrade=yes udpfallback=no >$null 2>&1;\n"
                ));
            }
        }
        let mut script = String::from("$ErrorActionPreference='SilentlyContinue';\n");
        script.push_str(&format!("$ips=@({});\n", ip_list));
        script.push_str(
            "Get-NetAdapter -Physical | Where-Object { $_.Status -eq 'Up' } | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ServerAddresses $ips };\n",
        );
        script.push_str(&doh_block);
        script.push_str("Clear-DnsClientCache;\n");
        run_ps(&script)
    }

    /// Сбросить DNS на автоматический (DHCP / от провайдера).
    pub fn clear() -> Result<(), String> {
        let script = "$ErrorActionPreference='SilentlyContinue';\n\
            Get-NetAdapter -Physical | Where-Object { $_.Status -eq 'Up' } | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses };\n\
            Clear-DnsClientCache;\n";
        run_ps(script)
    }
}

#[cfg(windows)]
pub fn apply(ips: &[String], doh: &str) -> Result<(), String> {
    imp::apply(ips, doh)
}

#[cfg(windows)]
pub fn clear() -> Result<(), String> {
    imp::clear()
}

#[cfg(not(windows))]
pub fn apply(_ips: &[String], _doh: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn clear() -> Result<(), String> {
    Ok(())
}
