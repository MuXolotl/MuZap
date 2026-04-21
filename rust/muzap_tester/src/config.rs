use std::path::{Path, PathBuf};

use crate::AppError;

// ─── Типы ─────────────────────────────────────────────────────────────────────

pub use muzap_core::app::{AppConfig, GamePorts, IpsetStatus};
pub use muzap_core::config_files::{Strategy, Target};

// ─── Определение корня MuZap ──────────────────────────────────────────────────

pub fn detect_root() -> Result<PathBuf, AppError> {
    Ok(muzap_core::paths::detect_root(10)?)
}

// ─── muzap.ini ────────────────────────────────────────────────────────────────

pub fn load_app_config(path: &Path) -> AppConfig {
    muzap_core::app::load_app_config(path)
}

// ─── strategies.ini ───────────────────────────────────────────────────────────

pub fn load_strategies(path: &Path) -> Result<Vec<Strategy>, AppError> {
    Ok(muzap_core::config_files::load_strategies(path)?)
}

// ─── targets.txt ──────────────────────────────────────────────────────────────

pub fn load_targets(path: &Path) -> Vec<Target> {
    muzap_core::config_files::load_targets(path)
}

// ─── Игровые порты ────────────────────────────────────────────────────────────

pub fn get_game_filter_ports(cfg: &AppConfig) -> GamePorts {
    muzap_core::app::get_game_filter_ports(cfg)
}

// ─── Управление ipset ─────────────────────────────────────────────────────────

pub fn get_ipset_status(root: &Path) -> IpsetStatus {
    muzap_core::app::get_ipset_status(root)
}

pub fn switch_ipset_to_any(root: &Path) -> Result<(), AppError> {
    muzap_core::app::switch_ipset_to_any(root)?;
    Ok(())
}

pub fn restore_ipset(root: &Path) -> Result<(), AppError> {
    muzap_core::app::restore_ipset(root)?;
    Ok(())
}
