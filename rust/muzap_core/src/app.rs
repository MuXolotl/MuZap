use std::{fs, path::Path};

use crate::ini;

#[derive(Debug, Default, Clone)]
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

const IPSET_NONE_PLACEHOLDER: &str = "203.0.113.113/32";

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

    // App.Version
    if let Some(v) = ini::read_ini_value(&text, "App", "Version") {
        if !v.trim().is_empty() {
            cfg.version = v;
        }
    }

    // Features.GameFilterMode
    if let Some(v) = ini::read_ini_value(&text, "Features", "GameFilterMode") {
        cfg.game_filter_mode = match v.to_ascii_lowercase().as_str() {
            "all" => GameFilterMode::All,
            "tcp" => GameFilterMode::Tcp,
            "udp" => GameFilterMode::Udp,
            _ => GameFilterMode::Off,
        };
    }

    // Features.TelemetryEnabled
    if let Some(v) = ini::read_ini_value(&text, "Features", "TelemetryEnabled") {
        cfg.telemetry_enabled = v.eq_ignore_ascii_case("true");
    }

    cfg
}

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

    if non_empty_lines.len() == 1 && non_empty_lines[0] == IPSET_NONE_PLACEHOLDER {
        return IpsetStatus::None;
    }

    IpsetStatus::Loaded
}

/// Для DPI-тестов: делаем ipset “any” (пустой файл), сохранив бэкап.
/// Бэкап: lists/ipset-all.test-backup.txt
pub fn switch_ipset_to_any(root: &Path) -> Result<(), crate::CoreError> {
    let list_file = root.join("lists").join("ipset-all.txt");
    let backup_file = root.join("lists").join("ipset-all.test-backup.txt");

    if list_file.exists() {
        fs::copy(&list_file, &backup_file)?;
    }

    fs::write(&list_file, "")?;
    Ok(())
}

pub fn restore_ipset(root: &Path) -> Result<(), crate::CoreError> {
    let list_file = root.join("lists").join("ipset-all.txt");
    let backup_file = root.join("lists").join("ipset-all.test-backup.txt");

    if backup_file.exists() {
        // rename может падать если файл существует — безопаснее удалить
        if list_file.exists() {
            let _ = fs::remove_file(&list_file);
        }
        fs::rename(&backup_file, &list_file)?;
    }

    Ok(())
}
