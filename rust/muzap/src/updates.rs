use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{fsutil, hosts, AppError, AppResult};

const IPSET_URL: &str =
    "https://raw.githubusercontent.com/MuXolotl/MuZap/main/.service/ipset-service.txt";
const HOSTS_URL: &str = "https://raw.githubusercontent.com/MuXolotl/MuZap/main/.service/hosts";

pub fn update_ipset_list(root: &Path) -> AppResult<()> {
    let lists_dir = root.join("lists");
    fs::create_dir_all(&lists_dir)?;

    let dest = lists_dir.join("ipset-all.txt");

    let bytes = download_bytes(IPSET_URL, 15)?;
    let text = String::from_utf8_lossy(&bytes);

    let count = count_valid_cidr_lines(&text);
    if count == 0 {
        return Err(AppError::Msg(
            "Скачанный ipset-файл не содержит ни одной CIDR-записи (возможна ошибка сервера)."
                .into(),
        ));
    }

    // Бэкап предыдущего (best-effort)
    backup_prev(&dest, &lists_dir.join("ipset-all.prev.txt"));

    fsutil::atomic_write_replace_bytes(&dest, &bytes)?;
    Ok(())
}

pub fn update_hosts_block(_root: &Path, verbose: bool) -> AppResult<()> {
    let hosts_path = fsutil::default_hosts_path();

    if verbose {
        println!("[ИНФО] hosts: {}", hosts_path.display());
        println!("[ИНФО] Скачиваю источник hosts из репозитория...");
    }

    let bytes = download_bytes(HOSTS_URL, 15)?;
    let text = String::from_utf8_lossy(&bytes);

    if count_valid_hosts_lines(&text) == 0 {
        return Err(AppError::Msg(
            "Скачанный hosts-источник не похож на hosts-записи (возможна ошибка сервера).".into(),
        ));
    }

    if verbose {
        println!("[ИНФО] Применяю блок MuZap (update)...");
    }

    hosts::update_hosts_managed_block(&hosts_path, &text, "MuZap")?;

    if verbose {
        println!("[OK] Блок MuZap обновлён.");
    }

    Ok(())
}

pub fn remove_hosts_block(_root: &Path, verbose: bool) -> AppResult<()> {
    let hosts_path = fsutil::default_hosts_path();

    if verbose {
        println!("[ИНФО] hosts: {}", hosts_path.display());
        println!("[ИНФО] Удаляю блок MuZap (remove)...");
    }

    let removed = hosts::remove_hosts_managed_block(&hosts_path, "MuZap")?;

    if verbose {
        if removed {
            println!("[OK] Блок MuZap удалён.");
        } else {
            println!("[OK] Блок MuZap не найден (удалять нечего).");
        }
    }

    Ok(())
}

pub fn run_release_updater_check(root: &Path, no_pause: bool) -> AppResult<()> {
    let exe = find_release_updater_exe(root)?;
    let mut args: Vec<String> = vec![
        "--root".into(),
        root.to_string_lossy().to_string(),
        "--check-only".into(),
    ];
    if no_pause {
        args.push("--no-pause".into());
    }

    let status = run_external_strings(&exe, &args, true)?;
    if !status.success() {
        return Err(AppError::Msg(format!(
            "Updater завершился с кодом: {:?}",
            status.code()
        )));
    }
    Ok(())
}

pub fn run_release_updater_install(root: &Path, no_pause: bool) -> AppResult<()> {
    let exe = find_release_updater_exe(root)?;
    let mut args: Vec<String> = vec!["--root".into(), root.to_string_lossy().to_string()];
    if no_pause {
        args.push("--no-pause".into());
    }

    let status = run_external_strings(&exe, &args, true)?;
    if !status.success() {
        return Err(AppError::Msg(format!(
            "Updater завершился с кодом: {:?}",
            status.code()
        )));
    }
    Ok(())
}

/* ========================= helpers ========================= */

fn find_release_updater_exe(root: &Path) -> AppResult<PathBuf> {
    let variants = ["Update MuZap.exe", "muzap_update.exe", "MuZap Update.exe"];

    for v in variants {
        let p = root.join(v);
        if p.exists() {
            return Ok(p);
        }
    }

    Err(AppError::Msg(
        "Не найден updater в корне MuZap (ожидается: 'Update MuZap.exe').".into(),
    ))
}

fn download_bytes(url: &str, timeout_secs: u64) -> AppResult<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent("MuZap/Updates")
        .build()?;

    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        return Err(AppError::Msg(format!(
            "Скачивание не удалось (HTTP {}): {url}",
            resp.status()
        )));
    }

    let b = resp.bytes()?;
    Ok(b.to_vec())
}

fn count_valid_cidr_lines(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with('#') && !l.starts_with(';'))
        .filter(|l| is_valid_cidr(l))
        .count()
}

fn is_valid_cidr(line: &str) -> bool {
    let Some((ip_s, pref_s)) = line.split_once('/') else {
        return false;
    };

    let ip: IpAddr = match ip_s.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };

    let pref: u8 = match pref_s.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };

    match ip {
        IpAddr::V4(_) => pref <= 32,
        IpAddr::V6(_) => pref <= 128,
    }
}

fn count_valid_hosts_lines(text: &str) -> usize {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with('#') && !l.starts_with(';'))
        .filter(|l| is_valid_hosts_line(l))
        .count()
}

fn is_valid_hosts_line(line: &str) -> bool {
    let mut it = line.split_whitespace();
    let Some(ip) = it.next() else {
        return false;
    };
    let Some(host) = it.next() else {
        return false;
    };

    if ip.parse::<IpAddr>().is_err() {
        return false;
    }

    !host.is_empty() && host.len() >= 2
}

fn backup_prev(src: &Path, dst: &Path) {
    if src.exists() {
        let _ = fs::copy(src, dst);
    }
}

fn run_external_strings(
    exe: &Path,
    args: &[String],
    inherit_stdio: bool,
) -> AppResult<std::process::ExitStatus> {
    let mut cmd = Command::new(exe);
    cmd.args(args);

    if inherit_stdio {
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    } else {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }

    Ok(cmd.status()?)
}

#[allow(dead_code)]
fn nanos_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}
