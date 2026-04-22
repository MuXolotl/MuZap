use std::{fs, path::Path};

use crate::{fsutil, AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Ok,
    Warn,
    Bad,
    Info,
}

#[derive(Debug, Clone)]
pub struct DiagItem {
    pub level: DiagLevel,
    pub title: String,
    pub message: String,
}

impl DiagItem {
    fn ok(title: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            level: DiagLevel::Ok,
            title: title.into(),
            message: msg.into(),
        }
    }
    fn warn(title: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            level: DiagLevel::Warn,
            title: title.into(),
            message: msg.into(),
        }
    }
    fn bad(title: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            level: DiagLevel::Bad,
            title: title.into(),
            message: msg.into(),
        }
    }
    fn info(title: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            level: DiagLevel::Info,
            title: title.into(),
            message: msg.into(),
        }
    }
}

pub fn run_all(root: &Path) -> AppResult<Vec<DiagItem>> {
    #[cfg(not(windows))]
    {
        let _ = root;
        return Ok(vec![DiagItem::warn(
            "Диагностика",
            "Поддержана только в Windows.",
        )]);
    }

    #[cfg(windows)]
    {
        let mut out: Vec<DiagItem> = Vec::new();
        out.extend(check_basic_files(root)?);
        out.extend(check_bfe()?);
        out.extend(check_proxy()?);
        out.extend(check_tcp_timestamps_registry()?);
        out.extend(check_doh_flags()?);
        out.extend(check_process_conflicts()?);
        out.extend(check_service_conflicts()?);
        out.extend(check_hosts_file()?);
        Ok(out)
    }
}

pub fn render_lines(items: &[DiagItem]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("Диагностика MuZap".to_string());
    lines.push("".to_string());

    for it in items {
        let tag = match it.level {
            DiagLevel::Ok => "[OK]",
            DiagLevel::Warn => "[?]",
            DiagLevel::Bad => "[X]",
            DiagLevel::Info => "[ИНФО]",
        };

        lines.push(format!("{tag} {}: {}", it.title, it.message));
    }

    lines
}

/* ========================= Windows implementation ========================= */

#[cfg(windows)]
fn check_basic_files(root: &Path) -> AppResult<Vec<DiagItem>> {
    let mut out = Vec::new();

    let winws = root.join("bin").join("winws.exe");
    if winws.exists() {
        out.push(DiagItem::ok("Файлы", "winws.exe найден"));
    } else {
        out.push(DiagItem::bad(
            "Файлы",
            format!("winws.exe не найден: {}", winws.display()),
        ));
    }

    let strategies = root.join("strategies.ini");
    if strategies.exists() {
        out.push(DiagItem::ok("Файлы", "strategies.ini найден"));
    } else {
        out.push(DiagItem::bad(
            "Файлы",
            format!("strategies.ini не найден: {}", strategies.display()),
        ));
    }

    let ini = root.join("muzap.ini");
    if ini.exists() {
        out.push(DiagItem::ok("Файлы", "muzap.ini найден"));
    } else {
        out.push(DiagItem::warn(
            "Файлы",
            format!("muzap.ini не найден: {}", ini.display()),
        ));
    }

    Ok(out)
}

#[cfg(windows)]
fn check_bfe() -> AppResult<Vec<DiagItem>> {
    let mut out = Vec::new();
    match win_services::query_service_state("BFE") {
        Ok(Some(st)) => {
            if st.running {
                out.push(DiagItem::ok(
                    "BFE (Base Filtering Engine)",
                    "служба запущена",
                ));
            } else {
                out.push(DiagItem::bad(
                    "BFE (Base Filtering Engine)",
                    format!("служба не запущена (state={})", st.state_str),
                ));
            }
        }
        Ok(None) => {
            out.push(DiagItem::warn(
                "BFE (Base Filtering Engine)",
                "служба не найдена (неожиданно)",
            ));
        }
        Err(e) => {
            out.push(DiagItem::warn(
                "BFE (Base Filtering Engine)",
                format!("не удалось проверить: {e}"),
            ));
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn check_proxy() -> AppResult<Vec<DiagItem>> {
    use win_registry::{hkey_current_user, read_dword, read_string};

    let mut out = Vec::new();

    let key = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let proxy_enable = read_dword(hkey_current_user(), key, "ProxyEnable").unwrap_or(0);
    let proxy_server = read_string(hkey_current_user(), key, "ProxyServer").unwrap_or_default();

    if proxy_enable == 1 {
        if proxy_server.trim().is_empty() {
            out.push(DiagItem::warn(
                "Proxy",
                "включён, но ProxyServer пустой (проверьте настройки)",
            ));
        } else {
            out.push(DiagItem::warn("Proxy", format!("включён: {proxy_server}")));
        }
    } else {
        out.push(DiagItem::ok("Proxy", "выключен"));
    }

    Ok(out)
}

#[cfg(windows)]
fn check_tcp_timestamps_registry() -> AppResult<Vec<DiagItem>> {
    use win_registry::{hkey_local_machine, read_dword};

    let mut out = Vec::new();

    let key = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters";
    let v = read_dword(hkey_local_machine(), key, "Tcp1323Opts");

    match v {
        Some(raw) => {
            let timestamps = (raw & 0b10) != 0;
            if timestamps {
                out.push(DiagItem::ok(
                    "TCP timestamps",
                    format!("включены (Tcp1323Opts={raw})"),
                ));
            } else {
                out.push(DiagItem::warn(
                    "TCP timestamps",
                    format!("выключены (Tcp1323Opts={raw}). Иногда это влияет на обход DPI."),
                ));
            }
        }
        None => {
            out.push(DiagItem::info(
                "TCP timestamps",
                "ключ Tcp1323Opts не найден (Windows использует значения по умолчанию)",
            ));
        }
    }

    Ok(out)
}

#[cfg(windows)]
fn check_doh_flags() -> AppResult<Vec<DiagItem>> {
    use win_registry::{enum_subkeys, hkey_local_machine, read_dword};

    let mut out = Vec::new();

    let base = r"SYSTEM\CurrentControlSet\Services\Dnscache\InterfaceSpecificParameters";
    let subkeys = enum_subkeys(hkey_local_machine(), base).unwrap_or_default();

    let mut any = 0usize;
    for sk in subkeys {
        let p = format!(r"{base}\{sk}");
        if let Some(v) = read_dword(hkey_local_machine(), &p, "DohFlags") {
            if v > 0 {
                any += 1;
            }
        }
    }

    if any > 0 {
        out.push(DiagItem::info(
            "DoH (Windows)",
            format!("обнаружены интерфейсы с DohFlags>0: {any}"),
        ));
    } else {
        out.push(DiagItem::ok(
            "DoH (Windows)",
            "DohFlags не обнаружены (или DoH выключен на уровне Windows)",
        ));
    }

    Ok(out)
}

#[cfg(windows)]
fn check_process_conflicts() -> AppResult<Vec<DiagItem>> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

    let mut out = Vec::new();

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut found = Vec::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        if name == "adguardsvc.exe" {
            found.push("AdguardSvc.exe".to_string());
        }
    }

    if found.is_empty() {
        out.push(DiagItem::ok(
            "Процессы",
            "конфликтующие процессы не найдены",
        ));
    } else {
        out.push(DiagItem::warn(
            "Процессы",
            format!("обнаружены потенциальные конфликты: {}", found.join(", ")),
        ));
    }

    Ok(out)
}

#[cfg(windows)]
fn check_service_conflicts() -> AppResult<Vec<DiagItem>> {
    let mut out = Vec::new();

    let known = [
        "GoodbyeDPI",
        "discordfix_zapret",
        "winws1",
        "winws2",
        "WinDivert",
        "WinDivert14",
    ];

    for s in known {
        if let Ok(Some(st)) = win_services::query_service_state(s) {
            if st.running {
                out.push(DiagItem::bad(
                    "Конфликтующая служба",
                    format!("{s} запущена (RUNNING)"),
                ));
            } else {
                out.push(DiagItem::warn(
                    "Конфликтующая служба",
                    format!("{s} установлена (state={})", st.state_str),
                ));
            }
        }
    }

    let all = win_services::enum_services_best_effort().unwrap_or_default();

    let killer: Vec<String> = all
        .iter()
        .filter(|e| e.name_lc.contains("killer") || e.display_lc.contains("killer"))
        .map(|e| format!("{} ({})", e.name, e.state_str))
        .collect();
    if !killer.is_empty() {
        out.push(DiagItem::warn(
            "Killer",
            format!("обнаружены службы: {}", killer.join(", ")),
        ));
    } else {
        out.push(DiagItem::ok("Killer", "службы Killer не обнаружены"));
    }

    let smartbyte: Vec<String> = all
        .iter()
        .filter(|e| e.name_lc.contains("smartbyte") || e.display_lc.contains("smartbyte"))
        .map(|e| format!("{} ({})", e.name, e.state_str))
        .collect();
    if !smartbyte.is_empty() {
        out.push(DiagItem::warn(
            "SmartByte",
            format!("обнаружены службы: {}", smartbyte.join(", ")),
        ));
    } else {
        out.push(DiagItem::ok("SmartByte", "службы SmartByte не обнаружены"));
    }

    let checkpoint: Vec<String> = all
        .iter()
        .filter(|e| {
            e.name_lc.contains("tracsrvwrapper")
                || e.display_lc.contains("tracsrvwrapper")
                || e.name_lc.contains("epwd")
                || e.display_lc.contains("epwd")
        })
        .map(|e| format!("{} ({})", e.name, e.state_str))
        .collect();
    if !checkpoint.is_empty() {
        out.push(DiagItem::warn(
            "Check Point",
            format!("обнаружены службы: {}", checkpoint.join(", ")),
        ));
    } else {
        out.push(DiagItem::ok(
            "Check Point",
            "службы Check Point не обнаружены",
        ));
    }

    let intel: Vec<String> = all
        .iter()
        .filter(|e| {
            let t = format!("{} {}", e.name_lc, e.display_lc);
            t.contains("intel") && t.contains("connect") && t.contains("network")
        })
        .map(|e| format!("{} ({})", e.name, e.state_str))
        .collect();
    if !intel.is_empty() {
        out.push(DiagItem::warn(
            "Intel Connectivity",
            format!("обнаружены службы: {}", intel.join(", ")),
        ));
    } else {
        out.push(DiagItem::ok(
            "Intel Connectivity",
            "службы Intel Connectivity не обнаружены",
        ));
    }

    let vpn: Vec<String> = all
        .iter()
        .filter(|e| e.name_lc.contains("vpn") || e.display_lc.contains("vpn"))
        .map(|e| format!("{} ({})", e.name, e.state_str))
        .collect();
    if !vpn.is_empty() {
        out.push(DiagItem::info(
            "VPN",
            format!("обнаружены службы: {}", vpn.join(", ")),
        ));
    } else {
        out.push(DiagItem::ok("VPN", "VPN-службы не обнаружены"));
    }

    Ok(out)
}

#[cfg(windows)]
fn check_hosts_file() -> AppResult<Vec<DiagItem>> {
    let mut out = Vec::new();

    let hosts_path = fsutil::default_hosts_path();
    let text = match fs::read_to_string(&hosts_path) {
        Ok(t) => t,
        Err(e) => {
            out.push(DiagItem::warn(
                "hosts",
                format!("не удалось прочитать: {} ({e})", hosts_path.display()),
            ));
            return Ok(out);
        }
    };

    let yt = [
        "youtube.com",
        "youtu.be",
        "googlevideo.com",
        "ytimg.com",
        "ggpht.com",
        "googleusercontent.com",
    ];
    if contains_any_domain(&text, &yt) {
        out.push(DiagItem::warn(
            "hosts (YouTube/Google)",
            "обнаружены записи, это может ломать YouTube/GoogleVideo",
        ));
    } else {
        out.push(DiagItem::ok(
            "hosts (YouTube/Google)",
            "подозрительных записей не найдено",
        ));
    }

    let dc = [
        "discord.com",
        "discordapp.com",
        "discord.gg",
        "discord.media",
        "gateway.discord.gg",
    ];
    if contains_any_domain(&text, &dc) {
        out.push(DiagItem::warn(
            "hosts (Discord)",
            "обнаружены записи, это может ломать Discord",
        ));
    } else {
        out.push(DiagItem::ok(
            "hosts (Discord)",
            "подозрительных записей не найдено",
        ));
    }

    let tg = [
        "telegram.org",
        "t.me",
        "web.telegram.org",
        "api.telegram.org",
    ];
    if telegram_outside_muzap_block(&text, &tg, "MuZap") {
        out.push(DiagItem::warn(
            "hosts (Telegram)",
            "обнаружены записи Telegram ВНЕ блока MuZap (возможен конфликт)",
        ));
    } else if contains_any_domain(&text, &tg) {
        out.push(DiagItem::ok(
            "hosts (Telegram)",
            "записи Telegram есть, но выглядят как управляемые MuZap (или отсутствуют вне блока)",
        ));
    } else {
        out.push(DiagItem::ok(
            "hosts (Telegram)",
            "записей Telegram не найдено",
        ));
    }

    Ok(out)
}

#[cfg(windows)]
fn contains_any_domain(text: &str, domains: &[&str]) -> bool {
    let t = text.to_ascii_lowercase();
    domains.iter().any(|d| t.contains(&d.to_ascii_lowercase()))
}

#[cfg(windows)]
fn telegram_outside_muzap_block(text: &str, domains: &[&str], marker: &str) -> bool {
    let begin = format!("# --- {marker} begin ---").to_ascii_lowercase();
    let end = format!("# --- {marker} end ---").to_ascii_lowercase();

    let mut in_block = false;

    for raw in text.lines() {
        let line = raw.trim().to_ascii_lowercase();

        if line == begin {
            in_block = true;
            continue;
        }
        if line == end {
            in_block = false;
            continue;
        }

        if !in_block {
            for d in domains {
                if line.contains(&d.to_ascii_lowercase()) {
                    return true;
                }
            }
        }
    }

    false
}

/* ========================= Windows: Registry helpers ========================= */

#[cfg(windows)]
mod win_registry {
    use super::AppError;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_SZ,
    };

    pub fn hkey_current_user() -> HKEY {
        HKEY_CURRENT_USER
    }

    pub fn hkey_local_machine() -> HKEY {
        HKEY_LOCAL_MACHINE
    }

    fn to_wide_null(s: &str) -> Vec<u16> {
        let mut v: Vec<u16> = OsStr::new(s).encode_wide().collect();
        v.push(0);
        v
    }

    pub fn read_dword(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
        unsafe {
            let mut h: HKEY = std::ptr::null_mut();
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

    pub fn read_string(root: HKEY, subkey: &str, value: &str) -> Option<String> {
        unsafe {
            let mut h: HKEY = std::ptr::null_mut();
            let sk = to_wide_null(subkey);
            if RegOpenKeyExW(root, sk.as_ptr(), 0, KEY_READ, &mut h) != 0 {
                return None;
            }

            let vn = to_wide_null(value);

            let mut typ: u32 = 0;
            let mut cb: u32 = 0;
            let rc1 = RegQueryValueExW(
                h,
                vn.as_ptr(),
                std::ptr::null_mut(),
                &mut typ,
                std::ptr::null_mut(),
                &mut cb,
            );
            if rc1 != 0 || typ != REG_SZ || cb < 2 {
                RegCloseKey(h);
                return None;
            }

            // clippy manual_div_ceil: (cb as usize).div_ceil(2)
            let mut buf: Vec<u16> = vec![0u16; (cb as usize).div_ceil(2)];
            let rc2 = RegQueryValueExW(
                h,
                vn.as_ptr(),
                std::ptr::null_mut(),
                &mut typ,
                buf.as_mut_ptr() as *mut u8,
                &mut cb,
            );

            RegCloseKey(h);

            if rc2 != 0 || typ != REG_SZ {
                return None;
            }

            if let Some(pos) = buf.iter().position(|&x| x == 0) {
                buf.truncate(pos);
            }

            Some(String::from_utf16_lossy(&buf))
        }
    }

    pub fn enum_subkeys(root: HKEY, subkey: &str) -> Result<Vec<String>, AppError> {
        unsafe {
            let mut h: HKEY = std::ptr::null_mut();
            let sk = to_wide_null(subkey);
            if RegOpenKeyExW(root, sk.as_ptr(), 0, KEY_READ, &mut h) != 0 {
                return Ok(vec![]);
            }

            let mut out: Vec<String> = Vec::new();

            for i in 0u32..10000 {
                let mut name_buf: [u16; 256] = [0; 256];
                let mut name_len: u32 = (name_buf.len() - 1) as u32;

                let rc = RegEnumKeyExW(
                    h,
                    i,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );

                if rc != 0 {
                    break;
                }

                let s = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }

            RegCloseKey(h);
            Ok(out)
        }
    }
}

/* ========================= Windows: Services helpers ========================= */

#[cfg(windows)]
mod win_services {
    use super::{AppError, AppResult};

    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW,
        SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_STATE_ALL,
        SERVICE_STATUS_PROCESS, SERVICE_WIN32,
    };

    #[derive(Debug, Clone)]
    pub struct ServiceEntry {
        pub name: String,
        pub name_lc: String,
        pub display_lc: String,
        pub state_str: String,
        pub running: bool,
    }

    #[derive(Debug, Clone)]
    pub struct ServiceStateInfo {
        pub state_str: String,
        pub running: bool,
    }

    fn wide_ptr_to_string(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        unsafe {
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
                if len > 32768 {
                    break;
                }
            }
            let slice = std::slice::from_raw_parts(p, len);
            String::from_utf16_lossy(slice)
        }
    }

    pub fn query_service_state(name: &str) -> AppResult<Option<ServiceStateInfo>> {
        let all = enum_services_best_effort()?;
        let wanted = name.to_ascii_lowercase();
        for e in all {
            if e.name.to_ascii_lowercase() == wanted {
                return Ok(Some(ServiceStateInfo {
                    state_str: e.state_str,
                    running: e.running,
                }));
            }
        }
        Ok(None)
    }

    pub fn enum_services_best_effort() -> Result<Vec<ServiceEntry>, AppError> {
        unsafe {
            let scm: SC_HANDLE = OpenSCManagerW(
                std::ptr::null(),
                std::ptr::null(),
                SC_MANAGER_ENUMERATE_SERVICE,
            );

            if scm.is_null() {
                return Ok(vec![]);
            }

            let mut bytes_needed: u32 = 0;
            let mut services_returned: u32 = 0;
            let mut resume: u32 = 0;

            let _ = EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                std::ptr::null_mut(),
                0,
                &mut bytes_needed,
                &mut services_returned,
                &mut resume,
                std::ptr::null(),
            );

            if bytes_needed == 0 {
                let _ = CloseServiceHandle(scm);
                return Ok(vec![]);
            }

            let mut buf: Vec<u8> = vec![0u8; bytes_needed as usize];

            resume = 0;
            services_returned = 0;
            let ok1 = EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut bytes_needed,
                &mut services_returned,
                &mut resume,
                std::ptr::null(),
            );

            if ok1 == 0 {
                let _ = CloseServiceHandle(scm);
                return Ok(vec![]);
            }

            let count = services_returned as usize;
            let mut out: Vec<ServiceEntry> = Vec::with_capacity(count);

            let base = buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW;
            for i in 0..count {
                let item = &*base.add(i);

                let name = wide_ptr_to_string(item.lpServiceName);
                let display = wide_ptr_to_string(item.lpDisplayName);

                let st: SERVICE_STATUS_PROCESS = item.ServiceStatusProcess;
                let (state_str, running) = state_to_str(st.dwCurrentState);

                out.push(ServiceEntry {
                    name_lc: name.to_ascii_lowercase(),
                    display_lc: display.to_ascii_lowercase(),
                    name,
                    state_str,
                    running,
                });
            }

            let _ = CloseServiceHandle(scm);
            Ok(out)
        }
    }

    fn state_to_str(v: u32) -> (String, bool) {
        let (s, running) = match v {
            1 => ("STOPPED", false),
            2 => ("START_PENDING", false),
            3 => ("STOP_PENDING", false),
            4 => ("RUNNING", true),
            5 => ("CONTINUE_PENDING", false),
            6 => ("PAUSE_PENDING", false),
            7 => ("PAUSED", false),
            _ => ("UNKNOWN", false),
        };
        (s.to_string(), running)
    }
}
