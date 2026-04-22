use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{AppError, AppResult};

pub fn run_tester(root: &Path) -> AppResult<()> {
    let exe = root.join("utils").join("muzap_tester.exe");
    if !exe.exists() {
        return Err(AppError::Msg(format!(
            "Не найден utils\\muzap_tester.exe: {}",
            exe.display()
        )));
    }

    // Запускаем как отдельную программу с её интерфейсом.
    // Тестер сам просит админ-права и сам делает паузу при необходимости.
    let status = Command::new(&exe)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        return Err(AppError::Msg(format!(
            "muzap_tester.exe завершился с кодом: {:?}",
            status.code()
        )));
    }

    Ok(())
}

pub fn read_service_log_lines(root: &Path) -> AppResult<Vec<String>> {
    let p = service_log_path(root);
    if !p.exists() {
        return Ok(vec!["(лог службы не найден)".into()]);
    }

    let text = fs::read_to_string(p)?;
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    if lines.is_empty() {
        lines.push("(лог пуст)".into());
    }
    Ok(lines)
}

pub fn service_log_path(root: &Path) -> PathBuf {
    root.join(".service").join("muzap_service.log")
}

pub fn open_root_folder(root: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        let status = Command::new("explorer.exe")
            .arg(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if !status.success() {
            return Err(AppError::Msg("Не удалось открыть проводник.".into()));
        }

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = root;
        Err(AppError::Msg(
            "Открытие папки поддержано только в Windows.".into(),
        ))
    }
}
