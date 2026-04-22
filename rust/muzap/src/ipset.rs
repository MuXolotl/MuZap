use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{AppError, AppResult};

const IPSET_NONE_PLACEHOLDER: &str = "203.0.113.113/32";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpSetMode {
    None,
    Any,
    Loaded,
}

impl IpSetMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            IpSetMode::None => "none",
            IpSetMode::Any => "any",
            IpSetMode::Loaded => "loaded",
        }
    }

    pub fn ru(&self) -> &'static str {
        match self {
            IpSetMode::None => "none (заглушка, фильтр выключен)",
            IpSetMode::Any => "any (пустой файл, фильтр для всех IP)",
            IpSetMode::Loaded => "loaded (ipset-all.txt со списком)",
        }
    }
}

pub fn ipset_file(root: &Path) -> PathBuf {
    root.join("lists").join("ipset-all.txt")
}

pub fn backup_file(root: &Path) -> PathBuf {
    root.join("lists").join("ipset-all.txt.backup")
}

pub fn detect_mode(root: &Path) -> IpSetMode {
    let p = ipset_file(root);

    let text = match fs::read_to_string(&p) {
        Ok(t) => t,
        Err(_) => return IpSetMode::None,
    };

    let non_empty: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    if non_empty.is_empty() {
        return IpSetMode::Any;
    }

    if non_empty.len() == 1 && non_empty[0] == IPSET_NONE_PLACEHOLDER {
        return IpSetMode::None;
    }

    IpSetMode::Loaded
}

pub fn bootstrap_user_lists(root: &Path) -> AppResult<()> {
    let lists_dir = root.join("lists");
    fs::create_dir_all(&lists_dir)?;

    let ipset_excl_user = lists_dir.join("ipset-exclude-user.txt");
    let list_general_user = lists_dir.join("list-general-user.txt");
    let list_exclude_user = lists_dir.join("list-exclude-user.txt");

    if !ipset_excl_user.exists() {
        fs::write(&ipset_excl_user, format!("{IPSET_NONE_PLACEHOLDER}\n"))?;
    }
    if !list_general_user.exists() {
        fs::write(&list_general_user, "domain.example.abc\n")?;
    }
    if !list_exclude_user.exists() {
        fs::write(&list_exclude_user, "domain.example.abc\n")?;
    }

    Ok(())
}

/// Переключить ipset режим как в bat:
/// - none: файл содержит только 203.0.113.113/32
/// - any: файл пустой
/// - loaded: пытаемся восстановить из ipset-all.txt.backup
pub fn set_mode(root: &Path, mode: IpSetMode) -> AppResult<String> {
    let lists_dir = root.join("lists");
    fs::create_dir_all(&lists_dir)?;

    let p = ipset_file(root);
    let bak = backup_file(root);

    let current = detect_mode(root);

    // helper: если текущий loaded, сохраним его в backup перед сменой
    let mut backup_saved = false;
    if current == IpSetMode::Loaded && mode != IpSetMode::Loaded {
        // best-effort: копия текущего файла в backup
        if p.exists() {
            let _ = fs::copy(&p, &bak);
            backup_saved = true;
        }
    }

    match mode {
        IpSetMode::None => {
            fs::write(&p, format!("{IPSET_NONE_PLACEHOLDER}\n"))?;
            Ok(format!(
                "IPSet режим установлен: none.{}",
                if backup_saved {
                    " (Список сохранён в ipset-all.txt.backup)"
                } else {
                    ""
                }
            ))
        }

        IpSetMode::Any => {
            fs::write(&p, "")?;
            Ok(format!(
                "IPSet режим установлен: any.{}",
                if backup_saved {
                    " (Список сохранён в ipset-all.txt.backup)"
                } else {
                    ""
                }
            ))
        }

        IpSetMode::Loaded => {
            if !bak.exists() {
                return Err(AppError::Msg(
                    "Нет backup-файла ipset-all.txt.backup.\nПодсказка: сначала выполните 'Обновить IPSet List', либо переключитесь из loaded в any/none (тогда backup создастся)."
                        .into(),
                ));
            }

            // Восстановим из backup (копия, чтобы backup остался)
            fs::copy(&bak, &p)?;
            Ok("IPSet режим установлен: loaded (восстановлено из ipset-all.txt.backup).".into())
        }
    }
}

pub fn parse_mode(s: &str) -> AppResult<IpSetMode> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "none" => Ok(IpSetMode::None),
        "any" => Ok(IpSetMode::Any),
        "loaded" => Ok(IpSetMode::Loaded),
        _ => Err(AppError::Msg(
            "Некорректный режим IPSet. Допустимо: none | any | loaded".into(),
        )),
    }
}
