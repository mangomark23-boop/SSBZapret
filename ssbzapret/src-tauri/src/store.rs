//! Персистентное хранилище пресетов и доменов.
//!
//! Всё лежит в `%APPDATA%/SSBZapret/config.json`. Встроенные пресеты
//! (AI Bypass / Social / Games) воссоздаются при каждом запуске,
//! если их нет — их можно дополнять доменами, но нельзя удалить.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_fooling() -> String {
    "md5sig".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Strategy {
    /// "split" | "fake" | "disorder" | "fakeddisorder"
    pub method: String,
    /// TTL для fake-пакетов (чтобы они умирали после DPI, но до сервера).
    pub ttl: u8,
    /// Где резать ClientHello: "sni" | "mid" | числовой оффсет строкой.
    pub split_pos: String,
    /// Блокировать QUIC (UDP 443) — заставляет YouTube/Google работать через TCP.
    #[serde(default)]
    pub block_quic: bool,
    /// Метод обмана DPI для fake-пакетов: "md5sig" | "badseq" | "datanoack".
    #[serde(default = "default_fooling")]
    pub fooling: String,
}

impl Default for Strategy {
    fn default() -> Self {
        Strategy {
            method: "fakeddisorder".into(),
            ttl: 8,
            split_pos: "sni".into(),
            block_quic: false,
            fooling: "md5sig".into(),
        }
    }
}

/// Правило: группа доменов со своей стратегией обхода.
/// Несколько правил в одном пресете объединяются в одну сессию winws
/// через `--new`, что позволяет применять разные desync к разным сервисам.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub strategy: Strategy,
    #[serde(default)]
    pub domains: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub builtin: bool,
    /// Понятные названия сервисов для карточки (например, "ChatGPT").
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub strategy: Strategy,
    /// Пользовательские/доп. домены (добавляются поверх встроенных правил).
    #[serde(default)]
    pub domains: Vec<String>,
    /// Правила chaining: каждая группа доменов со своей стратегией.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Preset {
    /// Эффективный набор правил для движка: встроенные правила пресета
    /// плюс пользовательские домены (`domains`) отдельным правилом.
    /// Если правил нет вовсе — синтезируем одно из strategy+domains.
    pub fn effective_rules(&self) -> Vec<Rule> {
        let mut out: Vec<Rule> = self.rules.clone();
        if !self.domains.is_empty() {
            out.push(Rule {
                id: "custom".into(),
                name: "Мои домены".into(),
                strategy: self.strategy.clone(),
                domains: self.domains.clone(),
            });
        }
        if out.is_empty() {
            out.push(Rule {
                id: "general".into(),
                name: "Основной".into(),
                strategy: self.strategy.clone(),
                domains: self.domains.clone(),
            });
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Store {
    pub presets: Vec<Preset>,
    pub active_id: String,
    /// Выбранный системный DNS-провайдер ("off" = системный по умолчанию).
    #[serde(default = "default_dns")]
    pub dns_provider: String,
}

fn default_dns() -> String {
    "off".into()
}

impl Store {
    pub fn config_path() -> PathBuf {
        let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        dir.push("SSBZapret");
        let _ = fs::create_dir_all(&dir);
        dir.push("config.json");
        dir
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(txt) = fs::read_to_string(&path) {
            if let Ok(mut s) = serde_json::from_str::<Store>(&txt) {
                s.ensure_builtins();
                return s;
            }
        }
        let mut s = Store { presets: vec![], active_id: String::new(), dns_provider: "off".into() };
        s.ensure_builtins();
        if s.active_id.is_empty() {
            s.active_id = "ai_bypass".into();
        }
        s.save();
        s
    }

    pub fn save(&self) {
        if let Ok(txt) = serde_json::to_string_pretty(self) {
            let _ = fs::write(Self::config_path(), txt);
        }
    }

    /// Гарантируем наличие встроенных пресетов и обновляем их
    /// встроенные правила/списки из кода, сохраняя пользовательские домены.
    fn ensure_builtins(&mut self) {
        for b in builtin_presets() {
            if let Some(existing) = self.presets.iter_mut().find(|p| p.id == b.id) {
                if existing.builtin {
                    existing.name = b.name.clone();
                    existing.icon = b.icon.clone();
                    existing.services = b.services.clone();
                    existing.rules = b.rules.clone();
                }
            } else {
                self.presets.push(b);
            }
        }
    }

    pub fn find(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }
    pub fn find_mut(&mut self, id: &str) -> Option<&mut Preset> {
        self.presets.iter_mut().find(|p| p.id == id)
    }
}

/// Builder для правила.
fn rule(id: &str, name: &str, method: &str, ttl: u8, block_quic: bool, domains: &[&str]) -> Rule {
    Rule {
        id: id.into(),
        name: name.into(),
        strategy: Strategy {
            method: method.into(),
            ttl,
            split_pos: "sni".into(),
            block_quic,
            fooling: "md5sig".into(),
        },
        domains: domains.iter().map(|s| s.to_string()).collect(),
    }
}

fn ai_rules() -> Vec<Rule> {
    vec![rule(
        "ai",
        "ИИ-сервисы",
        "fakeddisorder",
        8,
        false,
        &[
            "openai.com", "chatgpt.com", "oaistatic.com", "oaiusercontent.com", "auth0.openai.com",
            "claude.ai", "anthropic.com", "claudeusercontent.com",
            "gemini.google.com", "bard.google.com", "aistudio.google.com",
            "generativelanguage.googleapis.com", "makersuite.google.com",
            "x.ai", "grok.com",
            "copilot.microsoft.com", "githubcopilot.com",
            "perplexity.ai", "poe.com", "huggingface.co", "midjourney.com",
            "deepseek.com", "mistral.ai", "character.ai", "suno.com", "civitai.com",
        ],
    )]
}

fn social_rules() -> Vec<Rule> {
    vec![
        rule(
            "discord",
            "Discord",
            "fake",
            6,
            false,
            &[
                "discord.com", "discordapp.com", "discord.gg", "discord.media",
                "discordapp.net", "dis.gd", "discordcdn.com", "discordstatus.com",
            ],
        ),
        rule(
            "youtube",
            "YouTube",
            "fakeddisorder",
            8,
            true,
            &[
                "youtube.com", "youtu.be", "youtubei.googleapis.com", "googlevideo.com",
                "ytimg.com", "ggpht.com", "yt3.ggpht.com", "yt4.ggpht.com",
                "jnn-pa.googleapis.com", "youtube-nocookie.com",
            ],
        ),
        rule(
            "meta",
            "Instagram · Facebook",
            "fake",
            6,
            false,
            &[
                "instagram.com", "cdninstagram.com", "facebook.com", "fbcdn.net",
                "fb.com", "fbsbx.com", "whatsapp.com", "whatsapp.net", "wa.me",
            ],
        ),
        rule(
            "telegram",
            "Telegram",
            "split",
            8,
            false,
            &["telegram.org", "t.me", "telegram-cdn.org", "telesco.pe", "cdn-telegram.org", "tdesktop.com"],
        ),
        rule(
            "twitter",
            "X (Twitter)",
            "fake",
            6,
            false,
            &["twitter.com", "x.com", "twimg.com", "t.co", "twitterstat.us"],
        ),
    ]
}

fn game_rules() -> Vec<Rule> {
    vec![
        rule("roblox", "Roblox", "split", 8, false, &["roblox.com", "rbxcdn.com"]),
        rule(
            "stores",
            "Epic · Steam",
            "fake",
            6,
            false,
            &["epicgames.com", "fortnite.com", "steamcommunity.com", "steampowered.com", "steamstatic.com"],
        ),
    ]
}

/// Стартовый набор встроенных пресетов.
pub fn builtin_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "ai_bypass".into(),
            name: "AI Bypass".into(),
            icon: "ai".into(),
            builtin: true,
            services: vec!["ChatGPT".into(), "Claude".into(), "Gemini".into(), "Grok".into()],
            strategy: Strategy::default(),
            domains: vec![],
            rules: ai_rules(),
        },
        Preset {
            id: "social".into(),
            name: "Social".into(),
            icon: "social".into(),
            builtin: true,
            services: vec!["Discord".into(), "YouTube".into(), "Instagram".into(), "Telegram".into()],
            strategy: Strategy::default(),
            domains: vec![],
            rules: social_rules(),
        },
        Preset {
            id: "games".into(),
            name: "Games".into(),
            icon: "games".into(),
            builtin: true,
            services: vec!["Roblox".into(), "Epic".into(), "Steam".into()],
            strategy: Strategy::default(),
            domains: vec![],
            rules: game_rules(),
        },
        Preset {
            id: "full".into(),
            name: "Всё сразу".into(),
            icon: "social".into(),
            builtin: true,
            services: vec!["ИИ".into(), "Discord".into(), "YouTube".into(), "Игры".into()],
            strategy: Strategy::default(),
            domains: vec![],
            rules: {
                let mut all = ai_rules();
                all.extend(social_rules());
                all.extend(game_rules());
                all
            },
        },
    ]
}

/// Набор кандидатных стратегий для автоподбора (резервные системы обхода).
/// Перебираются по очереди, пока одна не сработает.
pub fn candidate_strategies() -> Vec<(String, String, Strategy)> {
    vec![
        (
            "fakeddisorder_md5".into(),
            "Fake+Disorder · md5sig · TTL2".into(),
            Strategy { method: "fakeddisorder".into(), ttl: 2, split_pos: "sni".into(), block_quic: false, fooling: "md5sig".into() },
        ),
        (
            "fakeddisorder_badseq".into(),
            "Fake+Disorder · badseq · TTL4".into(),
            Strategy { method: "fakeddisorder".into(), ttl: 4, split_pos: "sni".into(), block_quic: false, fooling: "badseq".into() },
        ),
        (
            "fake_ttl3".into(),
            "Fake · TTL3 · md5sig".into(),
            Strategy { method: "fake".into(), ttl: 3, split_pos: "sni".into(), block_quic: false, fooling: "md5sig".into() },
        ),
        (
            "split2_midsld".into(),
            "Split2 · midsld".into(),
            Strategy { method: "split".into(), ttl: 8, split_pos: "sni".into(), block_quic: false, fooling: "md5sig".into() },
        ),
        (
            "disorder2".into(),
            "Disorder2 · midsld".into(),
            Strategy { method: "disorder".into(), ttl: 8, split_pos: "sni".into(), block_quic: false, fooling: "md5sig".into() },
        ),
        (
            "fake_quic".into(),
            "Fake · TTL3 + QUIC off".into(),
            Strategy { method: "fake".into(), ttl: 3, split_pos: "sni".into(), block_quic: true, fooling: "md5sig".into() },
        ),
    ]
}

/// DNS-провайдер для разблокировки ИИ-сервисов.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DnsProvider {
    pub id: String,
    pub name: String,
    /// Короткое описание для карточки.
    pub note: String,
    /// IPv4-адреса резолвера.
    pub ips: Vec<String>,
    /// DoH-шаблон (Windows 11) — пустая строка, если нет.
    pub doh: String,
}

/// Список встроенных DNS-провайдеров.
pub fn dns_providers() -> Vec<DnsProvider> {
    vec![
        DnsProvider {
            id: "comss".into(),
            name: "Comss.one DNS".into(),
            note: "Разблокирует ИИ: ChatGPT, Gemini, Copilot, Claude. Рекомендуется.".into(),
            ips: vec!["83.220.169.155".into(), "212.109.195.93".into()],
            doh: "https://dns.comss.one/dns-query".into(),
        },
        DnsProvider {
            id: "xbox".into(),
            name: "Xbox DNS".into(),
            note: "ИИ + игры (Xbox Live, Supercell). Альтернатива Comss.".into(),
            ips: vec!["176.99.11.77".into()],
            doh: "https://xbox-dns.ru/dns-query".into(),
        },
        DnsProvider {
            id: "cloudflare".into(),
            name: "Cloudflare".into(),
            note: "Шифрованный DNS от подмены провайдером. ИИ сам по себе не открывает.".into(),
            ips: vec!["1.1.1.1".into(), "1.0.0.1".into()],
            doh: "https://cloudflare-dns.com/dns-query".into(),
        },
        DnsProvider {
            id: "google".into(),
            name: "Google Public DNS".into(),
            note: "Публичный DNS Google. Базовая защита от подмены DNS.".into(),
            ips: vec!["8.8.8.8".into(), "8.8.4.4".into()],
            doh: "https://dns.google/dns-query".into(),
        },
    ]
}
