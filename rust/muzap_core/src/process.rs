use std::{
    path::Path,
    process::{Child, Command, Stdio},
};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::{CoreError, CoreResult};

/// Простой парсер аргументов командной строки:
/// - пробелы разделяют аргументы
/// - поддерживаются двойные и одинарные кавычки
///
/// Важно: это не полный cmd.exe-парсер, но для наших Params подходит.
pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_double = false;
    let mut in_single = false;

    for c in s.chars() {
        match c {
            '"' if !in_single => {
                in_double = !in_double;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            ' ' | '\t' if !in_double && !in_single => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Запуск внешнего процесса (например winws.exe) с аргументами-строкой.
/// stdout/stderr по умолчанию глушим.
pub fn spawn_quiet(exe: &Path, params: &str) -> CoreResult<Child> {
    let args = split_args(params);

    Command::new(exe)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map_err(|e| CoreError::msg(format!("Не удалось запустить процесс: {e}")))
}

/// Убить все процессы по имени (без ошибок, best-effort).
pub fn kill_process_by_name_best_effort(name1: &str, name2: Option<&str>) {
    let n1 = name1.to_ascii_lowercase();
    let n2 = name2.map(|x| x.to_ascii_lowercase());

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    for p in sys.processes().values() {
        let n = p.name().to_string_lossy().to_ascii_lowercase();

        let ok1 = n == n1;
        let ok2 = n2.as_ref().is_some_and(|x| &n == x);

        if ok1 || ok2 {
            let _ = p.kill();
        }
    }
}

/// Проверка: запущен ли процесс по имени.
pub fn is_process_running(name1: &str, name2: Option<&str>) -> bool {
    let n1 = name1.to_ascii_lowercase();
    let n2 = name2.map(|x| x.to_ascii_lowercase());

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    sys.processes().values().any(|p| {
        let n = p.name().to_string_lossy().to_ascii_lowercase();

        if n == n1 {
            return true;
        }

        n2.as_ref().is_some_and(|x| &n == x)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_args_basic() {
        let s = r#"--a=1 --b=2"#;
        assert_eq!(split_args(s), vec!["--a=1", "--b=2"]);
    }

    #[test]
    fn split_args_quotes_keep_spaces() {
        let s = r#"--hostlist="C:\Program Files\MuZap\lists\list.txt" --x=1"#;
        assert_eq!(
            split_args(s),
            vec![
                r#"--hostlist=C:\Program Files\MuZap\lists\list.txt"#,
                "--x=1"
            ]
        );
    }

    #[test]
    fn split_args_single_quotes() {
        let s = "--name='hello world' --z=9";
        assert_eq!(split_args(s), vec!["--name=hello world", "--z=9"]);
    }
}
