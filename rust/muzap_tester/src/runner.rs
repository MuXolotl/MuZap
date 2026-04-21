use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

// ─── Запуск winws ─────────────────────────────────────────────────────────────

pub fn start_winws(exe: &Path, params: &str) -> Result<Option<Child>, crate::AppError> {
    let args = split_args(params);

    let child = Command::new(exe)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map_err(|e| crate::AppError::msg(format!("Не удалось запустить winws.exe: {e}")))?;

    Ok(Some(child))
}

/// Ждём, пока winws.exe появится в списке процессов, но не дольше `max_secs`.
pub fn wait_for_winws(max_secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(max_secs);

    loop {
        if is_winws_running() {
            std::thread::sleep(Duration::from_millis(1500));
            return true;
        }
        if Instant::now() >= deadline {
            return is_winws_running();
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Убиваем все процессы winws.
pub fn stop_zapret() {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        if name == "winws.exe" || name == "winws" {
            let _ = process.kill();
        }
    }
}

fn is_winws_running() -> bool {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        name == "winws.exe" || name == "winws"
    })
}

// ─── Служба MuZap ────────────────────────────────────────────────────────────

pub fn is_muzap_service_running() -> bool {
    let Ok(out) = Command::new("sc.exe").args(["query", "MuZap"]).output() else {
        return false;
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_ascii_uppercase();
    stdout.contains("RUNNING")
}

// ─── Снимок winws-процессов ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WinwsSnapshot {
    pub exe_path: String,
    pub cmdline: String,
}

pub fn take_winws_snapshot() -> Vec<WinwsSnapshot> {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    sys.processes()
        .values()
        .filter(|p| {
            let name = p.name().to_string_lossy().to_ascii_lowercase();
            name == "winws.exe" || name == "winws"
        })
        .filter_map(|p| {
            let exe = p.exe()?.to_string_lossy().to_string();
            let cmd = p
                .cmd()
                .iter()
                .map(|a| {
                    let s = a.to_string_lossy();
                    if s.contains(' ') {
                        format!("\"{s}\"")
                    } else {
                        s.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Some(WinwsSnapshot {
                exe_path: exe,
                cmdline: cmd,
            })
        })
        .collect()
}

pub fn restore_winws_snapshot(snapshot: &[WinwsSnapshot]) {
    if snapshot.is_empty() {
        return;
    }

    crate::print_colored_tag(
        "[ІНФО]",
        crossterm::style::Color::DarkGrey,
        "Восстанавливаю запущенные до теста winws-процессы...",
    );

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let running_cmds: Vec<String> = sys
        .processes()
        .values()
        .filter(|p| {
            let name = p.name().to_string_lossy().to_ascii_lowercase();
            name == "winws.exe" || name == "winws"
        })
        .map(|p| {
            p.cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();

    for snap in snapshot {
        if running_cmds.iter().any(|c| c == &snap.cmdline) {
            continue;
        }

        let exe_path = Path::new(&snap.exe_path);
        let workdir = exe_path.parent().unwrap_or(Path::new(".")).to_path_buf();

        let args_str = snap
            .cmdline
            .strip_prefix(&format!("\"{}\"", snap.exe_path))
            .or_else(|| snap.cmdline.strip_prefix(&snap.exe_path))
            .unwrap_or("")
            .trim();

        let args = split_args(args_str);

        match Command::new(exe_path)
            .args(&args)
            .current_dir(&workdir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => crate::print_colored_tag(
                "[ІНФО]",
                crossterm::style::Color::DarkGrey,
                "winws-процесс восстановлен.",
            ),
            Err(e) => crate::print_colored_tag(
                "[ПРЕДУПРЕЖДЕНИЕ]",
                crossterm::style::Color::Yellow,
                &format!("Не удалось восстановить winws: {e}"),
            ),
        }
    }
}

// ─── Парсер аргументов ────────────────────────────────────────────────────────

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
