use crate::{AppError, AppResult};

/// Отчёт выполнения “исправления”: готовые строки для вывода/scroll_view
pub type FixReport = Vec<String>;

pub fn fix_all() -> AppResult<FixReport> {
    #[cfg(not(windows))]
    {
        return Err(AppError::Msg(
            "Исправления поддержаны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        let mut out: FixReport = Vec::new();
        out.push("Исправления: выполнить всё".into());
        out.push("".into());

        out.extend(fix_remove_conflicting_services()?);
        out.push("".into());

        out.extend(fix_remove_windivert_services()?);
        out.push("".into());

        out.extend(fix_enable_tcp_timestamps()?);
        out.push("".into());

        out.extend(fix_clear_discord_cache()?);

        Ok(out)
    }
}

/// 1) Остановить и удалить известные конфликтующие службы + прибить процессы winws (best-effort)
pub fn fix_remove_conflicting_services() -> AppResult<FixReport> {
    #[cfg(not(windows))]
    {
        return Err(AppError::Msg(
            "Исправления поддержаны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

        let mut out: FixReport = Vec::new();
        out.push("Исправление: конфликтующие службы/процессы".into());

        {
            let mut sys = System::new_with_specifics(
                RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
            );
            sys.refresh_processes(ProcessesToUpdate::All, true);

            let killed = kill_processes_by_names(&mut sys, &["winws.exe", "winws"]);
            if killed > 0 {
                out.push(format!("[OK] Завершено процессов winws: {killed}"));
            } else {
                out.push("[OK] winws не найден (не запущен)".into());
            }
        }

        let services = [
            "GoodbyeDPI",
            "discordfix_zapret",
            "winws1",
            "winws2",
            "zapret",
        ];

        for name in services {
            match stop_delete_service_best_effort(name) {
                Ok(r) => out.extend(r),
                Err(e) => out.push(format!(
                    "[?] {name}: не удалось обработать (best-effort): {e}"
                )),
            }
        }

        Ok(out)
    }
}

/// 2) Удалить службы WinDivert / WinDivert14 (best-effort)
pub fn fix_remove_windivert_services() -> AppResult<FixReport> {
    #[cfg(not(windows))]
    {
        return Err(AppError::Msg(
            "Исправления поддержаны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        let mut out: FixReport = Vec::new();
        out.push("Исправление: удалить службы WinDivert".into());

        let services = ["WinDivert", "WinDivert14"];

        for name in services {
            match stop_delete_service_best_effort(name) {
                Ok(r) => out.extend(r),
                Err(e) => out.push(format!(
                    "[?] {name}: не удалось обработать (best-effort): {e}"
                )),
            }
        }

        Ok(out)
    }
}

/// 3) Включить TCP timestamps:
/// - ставим бит timestamps в Tcp1323Opts (HKLM)
/// - дополнительно пробуем netsh (best-effort)
pub fn fix_enable_tcp_timestamps() -> AppResult<FixReport> {
    #[cfg(not(windows))]
    {
        return Err(AppError::Msg(
            "Исправления поддержаны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use std::process::Command;
        let mut out: FixReport = Vec::new();
        out.push("Исправление: включить TCP timestamps".into());

        match win_registry::enable_tcp_timestamps_bit() {
            Ok((old, newv)) => {
                if old == newv {
                    out.push(format!(
                        "[OK] Реестр: Tcp1323Opts уже содержит timestamps (Tcp1323Opts={old})."
                    ));
                } else {
                    out.push(format!(
                        "[OK] Реестр: Tcp1323Opts обновлён: {old} -> {newv}."
                    ));
                }
            }
            Err(e) => out.push(format!(
                "[?] Реестр: не удалось обновить Tcp1323Opts (best-effort): {e}"
            )),
        }

        let status = Command::new("netsh")
            .args(["interface", "tcp", "set", "global", "timestamps=enabled"])
            .status();

        match status {
            Ok(st) if st.success() => {
                out.push("[OK] netsh: timestamps=enabled выполнено.".into());
            }
            Ok(st) => {
                out.push(format!(
                    "[?] netsh: команда завершилась с кодом: {:?}",
                    st.code()
                ));
            }
            Err(e) => {
                out.push(format!("[?] netsh: не удалось запустить netsh: {e}"));
            }
        }

        Ok(out)
    }
}

/// 4) Очистить кэш Discord: завершить Discord.exe и удалить Cache/Code Cache/GPUCache
pub fn fix_clear_discord_cache() -> AppResult<FixReport> {
    #[cfg(not(windows))]
    {
        return Err(AppError::Msg(
            "Исправления поддержаны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use std::path::PathBuf;
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

        let mut out: FixReport = Vec::new();
        out.push("Исправление: очистить кэш Discord".into());

        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
        );
        sys.refresh_processes(ProcessesToUpdate::All, true);

        let killed = kill_processes_by_names(&mut sys, &["discord.exe", "discord"]);
        if killed > 0 {
            out.push(format!(
                "[OK] Discord закрыт (процессов завершено: {killed})."
            ));
        } else {
            out.push("[OK] Discord не запущен.".into());
        }

        let appdata = std::env::var("APPDATA")
            .map(PathBuf::from)
            .map_err(|_| AppError::Msg("Не удалось прочитать переменную APPDATA.".into()))?;

        let variants = ["discord", "discordptb", "discordcanary"];
        let mut deleted_any = 0usize;

        for v in variants {
            let base = appdata.join(v);
            if !base.exists() {
                continue;
            }

            for d in ["Cache", "Code Cache", "GPUCache"] {
                let p = base.join(d);
                if p.exists() {
                    match std::fs::remove_dir_all(&p) {
                        Ok(_) => {
                            out.push(format!("[OK] Удалено: {}", p.display()));
                            deleted_any += 1;
                        }
                        Err(e) => {
                            out.push(format!("[?] Не удалось удалить {}: {e}", p.display()));
                        }
                    }
                }
            }
        }

        if deleted_any == 0 {
            out.push("[OK] Папки кэша не найдены (или уже удалены).".into());
        }

        Ok(out)
    }
}

/* ========================= helpers (windows) ========================= */

#[cfg(windows)]
fn stop_delete_service_best_effort(name: &str) -> AppResult<FixReport> {
    use windows_service::{
        service::{ServiceAccess, ServiceState},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    let mut out: FixReport = Vec::new();

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;

    let service = match manager.open_service(name, access) {
        Ok(s) => s,
        Err(_) => {
            out.push(format!("[OK] {name}: не установлена (пропуск)."));
            return Ok(out);
        }
    };

    if let Ok(st) = service.query_status() {
        if st.current_state != ServiceState::Stopped {
            let _ = service.stop();
            out.push(format!("[OK] {name}: stop отправлен."));
        }
    }

    let _ = service.delete();
    out.push(format!("[OK] {name}: delete отправлен."));
    drop(service);

    Ok(out)
}

#[cfg(windows)]
fn kill_processes_by_names(sys: &mut sysinfo::System, names: &[&str]) -> usize {
    let wanted: Vec<String> = names.iter().map(|s| s.to_ascii_lowercase()).collect();
    let mut killed = 0usize;

    for p in sys.processes().values() {
        let pname = p.name().to_string_lossy().to_ascii_lowercase();
        if wanted.iter().any(|w| w == &pname) {
            let _ = p.kill();
            killed += 1;
        }
    }

    if killed > 0 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    }

    killed
}

#[cfg(windows)]
mod win_registry {
    use super::{AppError, AppResult};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
        KEY_READ, KEY_SET_VALUE, REG_DWORD,
    };

    fn to_wide_null(s: &str) -> Vec<u16> {
        let mut v: Vec<u16> = OsStr::new(s).encode_wide().collect();
        v.push(0);
        v
    }

    fn read_dword(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
        unsafe {
            let mut h: HKEY = std::ptr::null_mut(); // FIX
            let sk = to_wide_null(subkey);
            if RegOpenKeyExW(root, sk.as_ptr(), 0, KEY_READ, &mut h) != 0 {
                return None;
            }

            let mut typ: u32 = 0;
            let mut data: u32 = 0;
            let mut cb: u32 = std::mem::size_of::<u32>() as u32;

            let vn = to_wide_null(value);
            let rc = RegQueryValueExW(
                h,
                vn.as_ptr(),
                std::ptr::null_mut(),
                &mut typ,
                &mut data as *mut _ as *mut u8,
                &mut cb,
            );

            RegCloseKey(h);

            if rc != 0 || typ != REG_DWORD {
                return None;
            }

            Some(data)
        }
    }

    fn write_dword(root: HKEY, subkey: &str, value: &str, data: u32) -> AppResult<()> {
        unsafe {
            let mut h: HKEY = std::ptr::null_mut(); // FIX
            let sk = to_wide_null(subkey);
            let rc = RegOpenKeyExW(root, sk.as_ptr(), 0, KEY_SET_VALUE, &mut h);
            if rc != 0 {
                return Err(AppError::Msg(format!(
                    "RegOpenKeyExW(KEY_SET_VALUE) не удалось (rc={rc})."
                )));
            }

            let vn = to_wide_null(value);
            let rc2 = RegSetValueExW(
                h,
                vn.as_ptr(),
                0,
                REG_DWORD,
                &data as *const _ as *const u8,
                std::mem::size_of::<u32>() as u32,
            );

            RegCloseKey(h);

            if rc2 != 0 {
                return Err(AppError::Msg(format!(
                    "RegSetValueExW(REG_DWORD) не удалось (rc={rc2})."
                )));
            }

            Ok(())
        }
    }

    pub fn enable_tcp_timestamps_bit() -> AppResult<(u32, u32)> {
        let key = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters";
        let old = read_dword(HKEY_LOCAL_MACHINE, key, "Tcp1323Opts").unwrap_or(0);
        let newv = old | 0b10;
        if newv != old {
            write_dword(HKEY_LOCAL_MACHINE, key, "Tcp1323Opts", newv)?;
        }
        Ok((old, newv))
    }
}
