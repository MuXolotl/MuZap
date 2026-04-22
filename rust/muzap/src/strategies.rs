use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use muzap_core::config_files::Strategy;

use crate::{AppError, AppResult};

pub fn detect_root_dir() -> AppResult<PathBuf> {
    Ok(muzap_core::paths::detect_root(10)?)
}

pub fn load_strategies_from_root(root: &Path) -> AppResult<Vec<Strategy>> {
    let p = root.join("strategies.ini");
    Ok(muzap_core::config_files::load_strategies(&p)?)
}

pub fn print_strategies(list: &[Strategy]) {
    println!("Стратегии (strategies.ini):");
    println!("{}", "-".repeat(60));
    for (i, s) in list.iter().enumerate() {
        let idx = i + 1;
        let desc = if s.description.trim().is_empty() {
            "Без описания"
        } else {
            s.description.trim()
        };
        println!("{idx:>3}. {:<24} — {desc}", s.name);
    }
    println!("{}", "-".repeat(60));
}

pub fn validate_strategy_name(list: &[Strategy], name: &str) -> AppResult<Strategy> {
    let wanted = name.trim();

    if wanted.is_empty() {
        return Err(AppError::Msg("Имя стратегии пустое.".into()));
    }

    list.iter()
        .find(|s| s.name.eq_ignore_ascii_case(wanted))
        .cloned()
        .ok_or_else(|| {
            AppError::Msg(format!(
                "Стратегия не найдена: '{wanted}'. Посмотрите список: muzap strategies list"
            ))
        })
}

/// Простой интерактив (без raw-mode и стрелок).
/// Позже заменим на красивое меню, но это уже работает и не тянет UI-зависимости.
pub fn choose_strategy_interactive(list: &[Strategy]) -> AppResult<Strategy> {
    if list.is_empty() {
        return Err(AppError::Msg(
            "В strategies.ini не найдено ни одной стратегии.".into(),
        ));
    }

    println!();
    println!("Выберите стратегию для службы MuZap.");
    print_strategies(list);
    println!("Введите номер (1..{}), либо 0 для отмены.", list.len());

    loop {
        print!("Номер: ");
        io::stdout().flush()?;

        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let s = buf.trim();

        if s == "0" {
            return Err(AppError::Msg("Отменено пользователем.".into()));
        }

        let idx: usize = match s.parse() {
            Ok(v) => v,
            Err(_) => {
                println!("Некорректный ввод. Нужно число.");
                continue;
            }
        };

        if idx == 0 || idx > list.len() {
            println!("Некорректный номер. Допустимо 1..{}.", list.len());
            continue;
        }

        return Ok(list[idx - 1].clone());
    }
}
