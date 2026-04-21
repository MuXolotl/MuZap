use std::path::{Path, PathBuf};

use crate::{CoreError, CoreResult};

#[derive(Debug, Clone)]
pub struct MuZapPaths {
    pub root: PathBuf,
}

impl MuZapPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn muzap_ini(&self) -> PathBuf {
        self.root.join("muzap.ini")
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn lists_dir(&self) -> PathBuf {
        self.root.join("lists")
    }

    pub fn service_dir(&self) -> PathBuf {
        self.root.join(".service")
    }

    pub fn utils_dir(&self) -> PathBuf {
        self.root.join("utils")
    }
}

/// Ищем корень MuZap по наличию `muzap.ini`.
/// Алгоритм:
/// 1) текущая папка (cwd) + подъём вверх до `max_levels`
/// 2) папка exe + подъём вверх до `max_levels`
pub fn detect_root(max_levels: usize) -> CoreResult<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(found) = find_root_upward(&cwd, max_levels) {
            return Ok(found);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(found) = find_root_upward(dir, max_levels) {
                return Ok(found);
            }
        }
    }

    Err(CoreError::msg(
        "Не удалось найти корень MuZap (файл muzap.ini не найден). Запустите из папки MuZap.",
    ))
}

pub fn find_root_upward(start: &Path, max_levels: usize) -> Option<PathBuf> {
    let mut cur = Some(start);
    for _ in 0..=max_levels {
        let p = cur?;
        if p.join("muzap.ini").exists() {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}
