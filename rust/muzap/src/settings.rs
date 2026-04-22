use std::path::Path;

use muzap_core::app::{AppConfig, GameFilterMode};

use crate::{AppError, AppResult};

pub fn load_app_config_from_root(root: &Path) -> AppConfig {
    muzap_core::app::load_app_config(&root.join("muzap.ini"))
}

pub fn set_game_filter_mode(root: &Path, value: &str) -> AppResult<()> {
    let v = value.trim().to_ascii_lowercase();
    let ok = matches!(v.as_str(), "off" | "all" | "tcp" | "udp");
    if !ok {
        return Err(AppError::Msg(
            "Некорректное значение GameFilterMode. Допустимо: off | all | tcp | udp".into(),
        ));
    }

    let ini_path = root.join("muzap.ini");
    muzap_core::ini::write_ini_value(&ini_path, "Features", "GameFilterMode", &v)?;
    Ok(())
}

pub fn set_telemetry_enabled(root: &Path, enabled: bool) -> AppResult<()> {
    let ini_path = root.join("muzap.ini");
    let v = if enabled { "true" } else { "false" };
    muzap_core::ini::write_ini_value(&ini_path, "Features", "TelemetryEnabled", v)?;
    Ok(())
}

pub fn game_filter_mode_ru(m: &GameFilterMode) -> &'static str {
    match m {
        GameFilterMode::Off => "выкл",
        GameFilterMode::All => "TCP+UDP (все порты игр)",
        GameFilterMode::Tcp => "только TCP (порты игр)",
        GameFilterMode::Udp => "только UDP (порты игр)",
    }
}

/// Разрешаем ввод:
/// - true/false
/// - 1/0
/// - on/off
/// - вкл/выкл
pub fn parse_bool_ru(s: &str) -> AppResult<bool> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "true" | "1" | "on" | "вкл" | "yes" | "y" => Ok(true),
        "false" | "0" | "off" | "выкл" | "no" | "n" => Ok(false),
        _ => Err(AppError::Msg(
            "Некорректное значение. Допустимо: true/false, 1/0, on/off, вкл/выкл".into(),
        )),
    }
}
