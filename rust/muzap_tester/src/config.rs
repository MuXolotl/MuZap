use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::AppError;

// ─── Типы ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Strategy {
    pub name: String,
    pub description: String,
    pub params: String,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    /// None для PING-only целей
    pub url: Option<String>,
    /// Хост или IP для пинга
    pub ping_target: String,
}

#[derive(Debug, Default)]
pub struct AppConfig {
    pub version: String,
    pub game_filter_mode: GameFilterMode,
    pub telemetry_enabled: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum GameFilterMode {
    #[default]
    Off,
    All,
    Tcp,
    Udp,
}

pub struct GamePorts {
    pub tcp: String,
    pub udp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpsetStatus {
    /// Файл содержит только заглушку 203.0.113.113/32
    None,
    /// Файл пустой — все IP проходят
    Any,
    /// Файл содержит реальные записи
    Loaded,
}

impl IpsetStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            IpsetStatus::None => "none",
            IpsetStatus::Any => "any",
            IpsetStatus::Loaded => "loaded",
        }
    }
}

// ─── Определение корня MuZap ──────────────────────────────────────────────────

pub fn detect_root() -> Result<PathBuf, AppError> {
    // Сначала пробуем cwd
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(p) = find_root_upward(&cwd, 10) {
            return Ok(p);
        }
    }

    // Потом папку exe
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(p) = find_root_upward(dir, 10) {
                return Ok(p);
            }
        }
    }

    Err(AppError::msg(
        "Не удалось найти корень MuZap (файл muzap.ini не найден). \
         Запустите из папки MuZap или укажите его через --root.",
    ))
}

fn find_root_upward(start: &Path, levels: usize) -> Option<PathBuf> {
    let mut cur = Some(start);
    for _ in 0..=levels {
        let p = cur?;
        if p.join("muzap.ini").exists() {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

// ─── Парсинг muzap.ini ────────────────────────────────────────────────────────

pub fn load_app_config(path: &Path) -> AppConfig {
    let mut cfg = AppConfig {
        version: "unknown".into(),
        game_filter_mode: GameFilterMode::Off,
        telemetry_enabled: false,
    };

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return cfg,
    };

    let mut section = String::new();

    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_ascii_lowercase();
            continue;
        }

        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();

            match section.as_str() {
                "app" => {
                    if k == "version" {
                        cfg.version = v.to_string();
                    }
                }
                "features" => match k.as_str() {
                    "gamefiltermode" => {
                        cfg.game_filter_mode = match v.to_ascii_lowercase().as_str() {
                            "all" => GameFilterMode::All,
                            "tcp" => GameFilterMode::Tcp,
                            "udp" => GameFilterMode::Udp,
                            _ => GameFilterMode::Off,
                        };
                    }
                    "telemetryenabled" => {
                        cfg.telemetry_enabled = v.eq_ignore_ascii_case("true");
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    cfg
}

// ─── Парсинг strategies.ini ───────────────────────────────────────────────────

pub fn load_strategies(path: &Path) -> Result<Vec<Strategy>, AppError> {
    let text = fs::read_to_string(path)
        .map_err(|e| AppError::msg(format!("Не удалось прочитать strategies.ini: {e}")))?;

    let mut strategies: Vec<Strategy> = Vec::new();
    let mut current: Option<Strategy> = None;

    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            // Сохраняем предыдущую секцию
            if let Some(s) = current.take() {
                if !s.params.is_empty() {
                    strategies.push(s);
                }
            }
            let name = line[1..line.len() - 1].to_string();
            current = Some(Strategy {
                name,
                description: String::new(),
                params: String::new(),
            });
            continue;
        }

        if let Some(ref mut s) = current {
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim().to_ascii_lowercase();
                let v = v.trim();
                match k.as_str() {
                    "description" => s.description = v.to_string(),
                    "params" => s.params = v.to_string(),
                    _ => {}
                }
            }
        }
    }

    // Последняя секция
    if let Some(s) = current {
        if !s.params.is_empty() {
            strategies.push(s);
        }
    }

    if strategies.is_empty() {
        return Err(AppError::msg(
            "В strategies.ini не найдено ни одной стратегии с полем Params.",
        ));
    }

    Ok(strategies)
}

// ─── Парсинг targets.txt ──────────────────────────────────────────────────────

pub fn load_targets(path: &Path) -> Vec<Target> {
    let mut targets: Vec<Target> = Vec::new();

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return default_targets(),
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Формат: KeyName = "https://..." или KeyName = "PING:1.2.3.4"
        if let Some((name, val)) = line.split_once('=') {
            let name = name.trim().to_string();
            let val = val.trim().trim_matches('"').to_string();

            let target = if let Some(addr) = val.strip_prefix("PING:") {
                Target {
                    name,
                    url: None,
                    ping_target: addr.trim().to_string(),
                }
            } else {
                let host = val
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                Target {
                    name,
                    url: Some(val),
                    ping_target: host,
                }
            };

            targets.push(target);
        }
    }

    if targets.is_empty() {
        default_targets()
    } else {
        targets
    }
}

fn default_targets() -> Vec<Target> {
    fn url(name: &str, url: &str) -> Target {
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        Target {
            name: name.to_string(),
            url: Some(url.to_string()),
            ping_target: host,
        }
    }
    fn ping(name: &str, ip: &str) -> Target {
        Target {
            name: name.to_string(),
            url: None,
            ping_target: ip.to_string(),
        }
    }

    vec![
        url("Discord Main", "https://discord.com"),
        url("Discord Gateway", "https://gateway.discord.gg"),
        url("Discord CDN", "https://cdn.discordapp.com"),
        url("Discord Updates", "https://updates.discord.com"),
        url("YouTube Web", "https://www.youtube.com"),
        url("YouTube Short", "https://youtu.be"),
        url("YouTube Image", "https://i.ytimg.com"),
        url(
            "YouTube Video Redirect",
            "https://redirector.googlevideo.com",
        ),
        url("Google Main", "https://www.google.com"),
        url("Google Gstatic", "https://www.gstatic.com"),
        url("Cloudflare Web", "https://www.cloudflare.com"),
        url("Cloudflare CDN", "https://cdnjs.cloudflare.com"),
        url("Telegram Main", "https://telegram.org"),
        url("Telegram Short", "https://t.me"),
        url("Telegram Web", "https://web.telegram.org"),
        ping("Cloudflare DNS 1.1.1.1", "1.1.1.1"),
        ping("Cloudflare DNS 1.0.0.1", "1.0.0.1"),
        ping("Google DNS 8.8.8.8", "8.8.8.8"),
        ping("Google DNS 8.8.4.4", "8.8.4.4"),
        ping("Quad9 DNS 9.9.9.9", "9.9.9.9"),
    ]
}

// ─── Игровые порты ────────────────────────────────────────────────────────────

pub fn get_game_filter_ports(cfg: &AppConfig) -> GamePorts {
    match cfg.game_filter_mode {
        GameFilterMode::All => GamePorts {
            tcp: "1024-65535".into(),
            udp: "1024-65535".into(),
        },
        GameFilterMode::Tcp => GamePorts {
            tcp: "1024-65535".into(),
            udp: "12".into(),
        },
        GameFilterMode::Udp => GamePorts {
            tcp: "12".into(),
            udp: "1024-65535".into(),
        },
        GameFilterMode::Off => GamePorts {
            tcp: "12".into(),
            udp: "12".into(),
        },
    }
}

// ─── Управление ipset ─────────────────────────────────────────────────────────

pub fn get_ipset_status(root: &Path) -> IpsetStatus {
    let list_file = root.join("lists").join("ipset-all.txt");

    let text = match fs::read_to_string(&list_file) {
        Ok(t) => t,
        Err(_) => return IpsetStatus::None,
    };

    let non_empty_lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    if non_empty_lines.is_empty() {
        return IpsetStatus::Any;
    }

    if non_empty_lines.len() == 1 && non_empty_lines[0] == "203.0.113.113/32" {
        return IpsetStatus::None;
    }

    IpsetStatus::Loaded
}

pub fn switch_ipset_to_any(root: &Path) -> Result<(), AppError> {
    let list_file = root.join("lists").join("ipset-all.txt");
    let backup_file = root.join("lists").join("ipset-all.test-backup.txt");

    if list_file.exists() {
        fs::copy(&list_file, &backup_file)?;
    }
    // Пустой файл = режим "any"
    fs::write(&list_file, "")?;
    Ok(())
}

pub fn restore_ipset(root: &Path) -> Result<(), AppError> {
    let list_file = root.join("lists").join("ipset-all.txt");
    let backup_file = root.join("lists").join("ipset-all.test-backup.txt");

    if backup_file.exists() {
        fs::rename(&backup_file, &list_file)?;
    }
    Ok(())
}
