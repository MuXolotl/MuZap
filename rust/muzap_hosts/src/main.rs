use clap::{CommandFactory, Parser, Subcommand};
use encoding_rs::{UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1251};
use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use sysinfo::{get_current_pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use thiserror::Error;

#[derive(Debug, Error)]
enum AppError {
    #[error("Ошибка ввода/вывода: {0}")]
    Io(#[from] io::Error),

    #[error("Некорректный аргумент: {0}")]
    Arg(String),

    #[error("Не найден файл-источник: {0}")]
    SourceMissing(String),

    #[error("hosts файл не найден: {0}")]
    HostsMissing(String),
}

type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Parser)]
#[command(
    name = "muzap_hosts",
    about = "MuZap: управление блоком в hosts (update/remove)",
    disable_help_subcommand = true
)]
struct Cli {
    /// Не ждать Enter перед закрытием (для батников/автоматизации)
    #[arg(long)]
    no_pause: bool,

    /// Путь к hosts (по умолчанию: %SystemRoot%\\System32\\drivers\\etc\\hosts)
    #[arg(long)]
    hosts_file: Option<PathBuf>,

    /// Имя маркера (MuZap => \"# --- MuZap BEGIN ---\")
    #[arg(long, default_value = "MuZap")]
    marker_name: String,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Обновить/создать управляемый блок в hosts из файла-источника
    Update {
        /// Файл с записями hosts (обычно скачанный из репозитория)
        #[arg(long)]
        source_file: PathBuf,
    },

    /// Удалить управляемый блок из hosts
    Remove,
}

#[derive(Debug, Clone, Copy)]
enum TextEncoding {
    Utf8Bom,
    Utf8NoBom,
    Utf16Le,
    Utf16Be,
    Windows1251,
}

#[derive(Debug)]
struct TextFile {
    text: String,
    encoding: TextEncoding,
    eol: String,
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();

    // Если запустили вообще без аргументов (двойной клик) — покажем help и примеры и НЕ закроем окно.
    if argv.len() == 1 {
        let mut cmd = Cli::command();
        let _ = cmd.print_help();
        println!();
        println!();
        println!("Примеры запуска:");
        println!(r#"  muzap_hosts.exe update --source-file "D:\MuZap\.service\hosts""#);
        println!(r#"  muzap_hosts.exe remove"#);
        println!();
        println!("Совет: запускать из cmd/Terminal (лучше от администратора).");
        let _ = pause_enter();
        return;
    }

    // Чтобы не было мгновенного закрытия на ошибках парсинга — используем try_parse()
    let cli = match Cli::try_parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            let _ = pause_enter();
            std::process::exit(2);
        }
    };

    let want_pause = should_pause_on_exit(cli.no_pause);
    let result = real_main(&cli);

    if let Err(e) = &result {
        eprintln!("\n[ОШИБКА] {e}");
    }

    if want_pause {
        let _ = pause_enter();
    }

    if result.is_err() {
        std::process::exit(1);
    }
}

fn real_main(cli: &Cli) -> AppResult<()> {
    let hosts_path = cli.hosts_file.clone().unwrap_or_else(default_hosts_path);

    match &cli.cmd {
        Command::Update { source_file } => {
            if !hosts_path.exists() {
                return Err(AppError::HostsMissing(hosts_path.display().to_string()));
            }
            if !source_file.exists() {
                return Err(AppError::SourceMissing(source_file.display().to_string()));
            }

            let marker = &cli.marker_name;

            println!("[ИНФО] hosts: {}", hosts_path.display());
            println!("[ИНФО] Источник: {}", source_file.display());
            println!("[ИНФО] Маркер: {marker}");

            let hosts = read_text_file(&hosts_path)?;
            let src = read_text_file(source_file)?;

            let updated_text = update_hosts_block(&hosts.text, &src.text, marker, &hosts.eol);
            let bytes = encode_text(&normalize_eol(&updated_text, &hosts.eol), hosts.encoding);

            atomic_write_replace(&hosts_path, &bytes)?;
            println!("[OK] Блок '{marker}' обновлён в hosts.");
        }

        Command::Remove => {
            if !hosts_path.exists() {
                return Err(AppError::HostsMissing(hosts_path.display().to_string()));
            }

            let marker = &cli.marker_name;

            println!("[ИНФО] hosts: {}", hosts_path.display());
            println!("[ИНФО] Маркер: {marker}");

            let hosts = read_text_file(&hosts_path)?;
            let (cleaned_text, removed) = remove_hosts_block(&hosts.text, marker, &hosts.eol);

            let bytes = encode_text(&normalize_eol(&cleaned_text, &hosts.eol), hosts.encoding);

            atomic_write_replace(&hosts_path, &bytes)?;

            if removed {
                println!("[OK] Блок '{marker}' удалён из hosts.");
            } else {
                println!("[OK] Блок '{marker}' не найден (удалять нечего).");
            }
        }
    }

    Ok(())
}

fn default_hosts_path() -> PathBuf {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    PathBuf::from(sysroot).join(r"System32\drivers\etc\hosts")
}

fn detect_eol(s: &str) -> String {
    if s.contains("\r\n") {
        "\r\n".to_string()
    } else {
        "\n".to_string()
    }
}

fn read_text_file(path: &Path) -> AppResult<TextFile> {
    let bytes = fs::read(path)?;
    let (encoding, text) = decode_bytes(&bytes);
    let eol = detect_eol(&text);
    Ok(TextFile {
        text,
        encoding,
        eol,
    })
}

fn normalize_eol(s: &str, eol: &str) -> String {
    let tmp = s.replace("\r\n", "\n").replace('\r', "\n");
    if eol == "\n" {
        tmp
    } else {
        tmp.replace('\n', eol)
    }
}

fn decode_bytes(bytes: &[u8]) -> (TextEncoding, String) {
    // UTF-8 BOM
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        let (cow, _, _) = UTF_8.decode(&bytes[3..]);
        return (TextEncoding::Utf8Bom, cow.into_owned());
    }

    // UTF-16 LE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let (cow, _, _) = UTF_16LE.decode(&bytes[2..]);
        return (TextEncoding::Utf16Le, cow.into_owned());
    }

    // UTF-16 BE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let (cow, _, _) = UTF_16BE.decode(&bytes[2..]);
        return (TextEncoding::Utf16Be, cow.into_owned());
    }

    // Без BOM: пробуем UTF-8, иначе Windows-1251 (эвристика для RU Windows)
    match std::str::from_utf8(bytes) {
        Ok(s) => (TextEncoding::Utf8NoBom, s.to_string()),
        Err(_) => {
            let (cow, _, _) = WINDOWS_1251.decode(bytes);
            (TextEncoding::Windows1251, cow.into_owned())
        }
    }
}

fn encode_text(text: &str, enc: TextEncoding) -> Vec<u8> {
    match enc {
        TextEncoding::Utf8Bom => {
            let mut out = vec![0xEF, 0xBB, 0xBF];
            out.extend_from_slice(text.as_bytes());
            out
        }
        TextEncoding::Utf8NoBom => text.as_bytes().to_vec(),
        TextEncoding::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            let (cow, _, _) = UTF_16LE.encode(text);
            out.extend_from_slice(&cow);
            out
        }
        TextEncoding::Utf16Be => {
            let mut out = vec![0xFE, 0xFF];
            let (cow, _, _) = UTF_16BE.encode(text);
            out.extend_from_slice(&cow);
            out
        }
        TextEncoding::Windows1251 => {
            let (cow, _, _) = WINDOWS_1251.encode(text);
            cow.into_owned()
        }
    }
}

fn begin_marker(marker: &str) -> String {
    format!("# --- {marker} BEGIN ---")
}

fn end_marker(marker: &str) -> String {
    format!("# --- {marker} END ---")
}

fn remove_hosts_block(hosts_text: &str, marker: &str, eol: &str) -> (String, bool) {
    let begin = begin_marker(marker);
    let end = end_marker(marker);

    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut removed = false;

    for line in hosts_text.lines() {
        let t = line.trim();
        if t == begin {
            in_block = true;
            removed = true;
            continue;
        }
        if in_block {
            if t == end {
                in_block = false;
            }
            continue;
        }
        out.push(line.to_string());
    }

    let mut result = out.join(eol);
    if !result.is_empty() && !result.ends_with(eol) {
        result.push_str(eol);
    }

    (result, removed)
}

fn normalize_source_lines(source_text: &str) -> Vec<String> {
    let mut out = Vec::new();

    for raw in source_text.lines() {
        let line = raw.trim_end();

        if line.is_empty() {
            out.push(String::new());
            continue;
        }

        // Комментарии из источника пропускаем
        if line.trim_start().starts_with('#') {
            continue;
        }

        out.push(line.to_string());
    }

    out
}

fn update_hosts_block(hosts_text: &str, source_text: &str, marker: &str, eol: &str) -> String {
    // Сначала удаляем существующий блок (если есть)
    let (cleaned, _removed) = remove_hosts_block(hosts_text, marker, eol);

    let mut lines: Vec<String> = cleaned.lines().map(|l| l.to_string()).collect();

    // Если файл не пуст и последняя строка не пустая — добавим пустую строку
    if !lines.is_empty() {
        if let Some(last) = lines.last() {
            if !last.trim().is_empty() {
                lines.push(String::new());
            }
        }
    }

    lines.push(begin_marker(marker));
    lines.extend(normalize_source_lines(source_text));
    lines.push(end_marker(marker));

    let mut result = lines.join(eol);
    if !result.ends_with(eol) {
        result.push_str(eol);
    }
    result
}

fn atomic_write_replace(dest: &Path, new_bytes: &[u8]) -> AppResult<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| AppError::Arg("Не удалось определить папку назначения".into()))?;

    // temp должен быть на том же томе, поэтому создаём рядом с hosts
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let tmp_name = format!(".muzap_hosts.tmp.{pid}.{nanos}");
    let tmp_path = dir.join(tmp_name);

    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        f.write_all(new_bytes)?;
        f.flush()?;
        f.sync_all()?;
    }

    let res = atomic_replace_impl(dest, &tmp_path);

    if res.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }

    res
}

fn atomic_replace_impl(dest: &Path, tmp: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING,
        };

        let dest_w = to_wide_null(dest);
        let tmp_w = to_wide_null(tmp);

        let ok = unsafe {
            ReplaceFileW(
                dest_w.as_ptr(),
                tmp_w.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if ok != 0 {
            return Ok(());
        }

        let ok2 =
            unsafe { MoveFileExW(tmp_w.as_ptr(), dest_w.as_ptr(), MOVEFILE_REPLACE_EXISTING) };
        if ok2 != 0 {
            return Ok(());
        }

        let code = unsafe { GetLastError() };
        Err(io::Error::other(format!(
            "Не удалось заменить файл атомарно (ReplaceFileW/MoveFileExW). Код WinAPI: {code}"
        ))
        .into())
    }

    #[cfg(not(windows))]
    {
        fs::rename(tmp, dest)?;
        Ok(())
    }
}

#[cfg(windows)]
fn to_wide_null(p: &Path) -> Vec<u16> {
    let mut v: Vec<u16> = OsStr::new(p).encode_wide().collect();
    v.push(0);
    v
}

fn pause_enter() -> io::Result<()> {
    println!();
    println!("Нажмите Enter, чтобы закрыть окно...");
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s)?;
    Ok(())
}

fn should_pause_on_exit(no_pause: bool) -> bool {
    if no_pause {
        return false;
    }

    // Пауза только если запуск похож на “двойной клик из проводника”.
    // Если не смогли определить — лучше подстрахуемся и покажем вывод (пауза).
    is_parent_explorer_or_unknown()
}

fn is_parent_explorer_or_unknown() -> bool {
    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let pid = match get_current_pid() {
        Ok(p) => p,
        Err(_) => return true,
    };

    let me = match sys.process(pid) {
        Some(p) => p,
        None => return true,
    };

    let ppid = match me.parent() {
        Some(p) => p,
        None => return true,
    };

    let parent = match sys.process(ppid) {
        Some(p) => p,
        None => return true,
    };

    let name = parent.name().to_string_lossy().to_ascii_lowercase();

    if name == "explorer.exe" {
        return true;
    }

    // Если это cmd/powershell/terminal — пауза не нужна (окно не закроется само)
    !matches!(
        name.as_str(),
        "cmd.exe" | "powershell.exe" | "pwsh.exe" | "wt.exe" | "windowsterminal.exe"
    )
}
