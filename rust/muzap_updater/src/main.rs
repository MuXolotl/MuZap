use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use indicatif::{ProgressBar, ProgressStyle};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    fs::File,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use sysinfo::System;
use thiserror::Error;
use walkdir::WalkDir;
use zip::ZipArchive;

#[derive(Debug, Error)]
enum AppError {
    #[error("Ошибка ввода/вывода: {0}")]
    Io(#[from] io::Error),

    #[error("Ошибка сети: {0}")]
    Net(#[from] reqwest::Error),

    #[error("Ошибка ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Некорректная версия (SemVer): {0}")]
    SemVer(#[from] semver::Error),

    #[error("{0}")]
    Msg(String),

    #[error("Не найден ZIP-ассет MuZap_*.zip в релизе")]
    NoZipAsset,

    #[error("Проверка SHA-256 не пройдена (ожидалось {expected}, получено {actual})")]
    DigestMismatch { expected: String, actual: String },
}

type AppResult<T> = Result<T, AppError>;

#[derive(Clone, Debug)]
struct Args {
    root: PathBuf,
    repo: String,
    yes: bool,
    force: bool,
    check_only: bool,
    timeout_sec: u64,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize, Clone)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
    digest: Option<String>, // sha256:...
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let no_pause = argv.iter().any(|a| a == "--no-pause");

    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        if !no_pause {
            let _ = pause_enter();
        }
        return;
    }

    let result = real_main();

    if let Err(e) = &result {
        eprintln!("\n[ОШИБКА] {e}");
    }

    if !no_pause {
        let _ = pause_enter();
    }

    if result.is_err() {
        std::process::exit(1);
    }
}

fn real_main() -> AppResult<()> {
    let args = parse_args()?;

    print_banner();

    println_kv("Репозиторий", &args.repo);
    println_kv("Каталог MuZap", &args.root.display().to_string());
    println_kv(
        "Режим",
        if args.check_only {
            "Проверка (без установки)"
        } else {
            "Установка (с обновлением)"
        },
    );
    println!();

    let ini_path = args.root.join("muzap.ini");
    let local_version = read_local_version(&ini_path).unwrap_or_else(|| "unknown".to_string());

    if ini_path.exists() {
        println_kv("Текущая версия", &local_version);
    } else {
        print_warn(&format!("Файл muzap.ini не найден: {}", ini_path.display()));
        if !args.check_only {
            return Err(AppError::Msg(
                "Не могу продолжить установку обновления: не найден muzap.ini.\nПодсказка: запусти программу из папки MuZap или укажи --root \"ПУТЬ_К_MUZAP\".\n(Режим --check-only можно запускать без muzap.ini.)"
                    .to_string(),
            ));
        }
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("MuZap-Updater/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(args.timeout_sec))
        .build()?;

    print_info("Получаю информацию о последнем релизе...");
    let release = fetch_latest_release(&client, &args.repo)?;

    let remote_version_str = release.tag_name.trim().trim_start_matches('v').to_string();
    println_kv("Последняя версия", &remote_version_str);

    let update_needed = is_update_needed(&local_version, &remote_version_str, args.force)?;
    if !update_needed {
        print_ok("У вас уже установлена последняя версия. Обновление не требуется.");
        return Ok(());
    }

    let asset = pick_zip_asset(&release.assets)?;
    println_kv("Архив", &asset.name);
    println_kv(
        "Размер",
        &asset
            .size
            .map(format_bytes)
            .unwrap_or_else(|| "неизвестно".to_string()),
    );
    println_kv(
        "SHA-256 (GitHub)",
        asset
            .digest
            .as_deref()
            .unwrap_or("нет (пропускаю проверку SHA-256)"),
    );
    println!();

    if args.check_only {
        print_ok("Обновление доступно, но режим --check-only не устанавливает его.");
        return Ok(());
    }

    if !args.yes {
        let question = format!(
            "Доступно обновление: {}  (у вас: {})",
            remote_version_str, local_version
        );
        let hint = "Управление: ↑↓, Enter — выбрать, Esc — отмена";
        let confirmed = ask_yes_no_tui(&question, hint, true)?;
        if !confirmed {
            print_warn("Отменено пользователем.");
            return Ok(());
        }
    } else {
        print_info("Флаг --yes указан: подтверждение не требуется.");
    }

    // best-effort остановки
    stop_service_best_effort("MuZap");
    kill_process_best_effort("winws.exe");
    kill_process_best_effort("winws");

    // Скачать ZIP + SHA-256
    print_info("Скачиваю архив обновления...");
    let tmp_zip = std::env::temp_dir().join(format!("muzap_update_{remote_version_str}.zip"));
    let (actual_sha256_hex, downloaded_bytes) = download_with_sha256(&client, &asset, &tmp_zip)?;
    println!();
    println_kv("Скачано", &format_bytes(downloaded_bytes));
    println_kv("SHA-256 (факт)", &actual_sha256_hex);

    // Проверить digest
    if let Some(digest) = &asset.digest {
        if let Some(expected) = digest.strip_prefix("sha256:") {
            let expected_norm = expected.trim().to_ascii_lowercase();
            let actual_norm = actual_sha256_hex.trim().to_ascii_lowercase();
            if expected_norm != actual_norm {
                return Err(AppError::DigestMismatch {
                    expected: expected_norm,
                    actual: actual_norm,
                });
            }
            print_ok("SHA-256 совпал (архив целый).");
        } else {
            print_warn("Digest имеет неожиданный формат — сравнение пропущено.");
        }
    }

    ensure_zip_magic(&tmp_zip)?;

    // Распаковка
    let tmp_extract =
        std::env::temp_dir().join(format!("muzap_update_extract_{remote_version_str}"));
    if tmp_extract.exists() {
        let _ = fs::remove_dir_all(&tmp_extract);
    }
    fs::create_dir_all(&tmp_extract)?;

    print_info("Распаковываю архив...");
    extract_zip_safely(&tmp_zip, &tmp_extract)?;
    let extracted_root = detect_extracted_root(&tmp_extract)?;

    // Применить обновление
    print_info("Применяю обновление (копирование файлов)...");
    apply_update(&args.root, &extracted_root)?;

    // Записать версию в muzap.ini
    write_ini_value(&ini_path, "App", "Version", &remote_version_str)?;
    print_ok(&format!(
        "Версия в muzap.ini обновлена на {remote_version_str}."
    ));

    // Старт службы (best-effort)
    start_service_best_effort("MuZap");

    // cleanup
    let _ = fs::remove_file(&tmp_zip);
    let _ = fs::remove_dir_all(&tmp_extract);

    println!();
    print_ok("Обновление завершено успешно.");
    print_info("Если MuZap.bat был обновлён — он применится при следующем запуске (через .service\\MuZap.bat.pending).");

    Ok(())
}

fn parse_args() -> AppResult<Args> {
    let mut root: Option<PathBuf> = None;
    let mut repo = "MuXolotl/MuZap".to_string();
    let mut yes = false;
    let mut force = false;
    let mut check_only = false;
    let mut timeout_sec: u64 = 30;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                let v = it
                    .next()
                    .ok_or_else(|| AppError::Msg("Ожидалось значение после --root".into()))?;
                root = Some(PathBuf::from(v));
            }
            "--repo" => {
                let v = it
                    .next()
                    .ok_or_else(|| AppError::Msg("Ожидалось значение после --repo".into()))?;
                repo = v;
            }
            "--yes" | "-y" => yes = true,
            "--force" => force = true,
            "--check-only" => check_only = true,
            "--timeout" => {
                let v = it
                    .next()
                    .ok_or_else(|| AppError::Msg("Ожидалось значение после --timeout".into()))?;
                timeout_sec = v.parse::<u64>().unwrap_or(30);
            }
            "--no-pause" => {
                // обрабатывается в main()
            }
            "--help" | "-h" => {
                // обрабатывается в main()
            }
            _ => {
                return Err(AppError::Msg(format!(
                    "Неизвестный аргумент: {a}\nИспользуйте --help"
                )));
            }
        }
    }

    let root = root.unwrap_or_else(detect_root_dir);

    Ok(Args {
        root,
        repo,
        yes,
        force,
        check_only,
        timeout_sec,
    })
}

fn print_help() {
    println!(
        r#"Обновить MuZap (Rust)

Аргументы:
  --root <путь>        Папка MuZap (где лежит muzap.ini). Обычно не нужно.
  --repo <owner/repo>  Репозиторий GitHub. По умолчанию: MuXolotl/MuZap
  --yes, -y            Не спрашивать подтверждение
  --force              Принудительно обновлять даже если версия одинаковая
  --check-only         Только проверить наличие обновления (без установки)
  --timeout <сек>      Таймаут сети (по умолчанию 30)
  --no-pause           Не ждать Enter перед закрытием (для автоматизации)
  --help, -h           Помощь
"#
    );
}

fn detect_root_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Some(found) = find_root_upwards(&cwd, 10) {
        return found;
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    if let Some(found) = find_root_upwards(&exe_dir, 10) {
        return found;
    }

    exe_dir
}

fn find_root_upwards(start: &Path, max_levels: usize) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    for _ in 0..=max_levels {
        let p = cur?;
        if p.join("muzap.ini").exists() {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

fn read_local_version(ini_path: &Path) -> Option<String> {
    let text = fs::read_to_string(ini_path).ok()?;
    let mut in_app = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_app = line[1..line.len() - 1].eq_ignore_ascii_case("App");
            continue;
        }
        if in_app {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case("Version") {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

fn is_update_needed(local: &str, remote: &str, force: bool) -> AppResult<bool> {
    if force {
        return Ok(true);
    }
    let local_v = Version::parse(local).ok();
    let remote_v = Version::parse(remote)?;
    match local_v {
        Some(lv) => Ok(remote_v > lv),
        None => Ok(true),
    }
}

fn fetch_latest_release(client: &reqwest::blocking::Client, repo: &str) -> AppResult<GhRelease> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        return Err(AppError::Msg(format!(
            "GitHub API вернул HTTP {}",
            resp.status()
        )));
    }
    Ok(resp.json::<GhRelease>()?)
}

fn pick_zip_asset(assets: &[GhAsset]) -> AppResult<GhAsset> {
    if let Some(a) = assets
        .iter()
        .find(|a| a.name.starts_with("MuZap_") && a.name.ends_with(".zip"))
    {
        return Ok(a.clone());
    }
    if let Some(a) = assets.iter().find(|a| a.name.ends_with(".zip")) {
        return Ok(a.clone());
    }
    Err(AppError::NoZipAsset)
}

fn download_with_sha256(
    client: &reqwest::blocking::Client,
    asset: &GhAsset,
    dest_zip: &Path,
) -> AppResult<(String, u64)> {
    let mut resp = client.get(&asset.browser_download_url).send()?;
    if !resp.status().is_success() {
        return Err(AppError::Msg(format!(
            "Скачивание не удалось (HTTP {})",
            resp.status()
        )));
    }

    let total = asset.size.or_else(|| resp.content_length());
    let pb = match total {
        Some(t) if t > 0 => {
            let pb = ProgressBar::new(t);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            Some(pb)
        }
        _ => None,
    };

    let mut out = File::create(dest_zip)?;
    let mut hasher = Sha256::new();

    let mut buf = [0u8; 64 * 1024];
    let mut total_written: u64 = 0;

    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        total_written += n as u64;
        if let Some(pb) = &pb {
            pb.set_position(total_written);
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message("Загрузка завершена");
    }

    Ok((hex::encode(hasher.finalize()), total_written))
}

fn ensure_zip_magic(path: &Path) -> AppResult<()> {
    let mut f = File::open(path)?;
    let mut sig = [0u8; 4];
    f.read_exact(&mut sig)?;
    if sig != [0x50, 0x4B, 0x03, 0x04] {
        return Err(AppError::Msg(
            "Файл не похож на ZIP (не совпадает сигнатура PK\\x03\\x04)".into(),
        ));
    }
    Ok(())
}

fn extract_zip_safely(zip_path: &Path, dest_dir: &Path) -> AppResult<()> {
    let f = File::open(zip_path)?;
    let reader = BufReader::new(f);
    let mut archive = ZipArchive::new(reader)?;
    archive.extract(dest_dir)?;
    Ok(())
}

fn detect_extracted_root(tmp_extract: &Path) -> AppResult<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(tmp_extract)?.filter_map(|e| e.ok()).collect();

    if entries.len() == 1 {
        let p = entries.remove(0).path();
        if p.is_dir() {
            return Ok(p);
        }
    }
    Ok(tmp_extract.to_path_buf())
}

fn apply_update(root: &Path, extracted_root: &Path) -> AppResult<()> {
    let service_dir = root.join(".service");
    fs::create_dir_all(&service_dir)?;

    let self_exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|x| x.to_string_lossy().to_string()));

    let protected = [
        "muzap.ini",
        "muzap.bat",
        "lists/ipset-exclude-user.txt",
        "lists/list-general-user.txt",
        "lists/list-exclude-user.txt",
    ];

    for entry in WalkDir::new(extracted_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let src = entry.path().to_path_buf();
        let rel = match src.strip_prefix(extracted_root) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let rel_norm = normalize_rel(rel);

        if rel_norm.starts_with(".git/") || rel_norm.starts_with(".github/") {
            continue;
        }

        if let Some(self_name) = &self_exe_name {
            if rel_norm.eq_ignore_ascii_case(self_name) {
                print_info(&format!("Пропуск (защищено): {rel_norm}"));
                continue;
            }
        }

        let is_protected = protected.iter().any(|p| rel_norm.eq_ignore_ascii_case(p));
        if is_protected {
            if rel_norm.eq_ignore_ascii_case("muzap.bat") {
                let pending = service_dir.join("MuZap.bat.pending");
                fs::copy(&src, &pending)?;
                print_info(&format!(
                    "Обновление MuZap.bat отложено: {}",
                    pending.display()
                ));
            } else {
                print_info(&format!("Пропуск (защищено): {rel_norm}"));
            }
            continue;
        }

        let dst = root.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(&src, &dst)?;
    }

    Ok(())
}

fn normalize_rel(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    s.trim_start_matches("./").to_string()
}

fn write_ini_value(path: &Path, section: &str, key: &str, value: &str) -> AppResult<()> {
    let mut text = String::new();
    let mut eol = "\r\n".to_string();

    if path.exists() {
        let raw = fs::read_to_string(path)?;
        eol = if raw.contains("\r\n") { "\r\n" } else { "\n" }.to_string();
        text = raw;
    }

    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut section_found = false;
    let mut key_set = false;

    for line in text.lines() {
        let t = line.trim();

        if t.starts_with('[') && t.ends_with(']') {
            if in_section && !key_set {
                out.push(format!("{key}={value}"));
                key_set = true;
            }

            let cur = &t[1..t.len() - 1];
            if cur.eq_ignore_ascii_case(section) {
                in_section = true;
                section_found = true;
            } else {
                in_section = false;
            }

            out.push(line.to_string());
            continue;
        }

        if in_section {
            if let Some((k, _v)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    out.push(format!("{key}={value}"));
                    key_set = true;
                    continue;
                }
            }
        }

        out.push(line.to_string());
    }

    if section_found && !key_set {
        out.push(format!("{key}={value}"));
    }

    if !section_found {
        if !out.is_empty() && !out.last().unwrap().trim().is_empty() {
            out.push("".to_string());
        }
        out.push(format!("[{section}]"));
        out.push(format!("{key}={value}"));
    }

    let final_text = out.join(&eol) + &eol;
    fs::write(path, final_text)?;
    Ok(())
}

fn stop_service_best_effort(name: &str) {
    let _ = Command::new("sc.exe").args(["stop", name]).output();
    std::thread::sleep(Duration::from_millis(800));
}

fn start_service_best_effort(name: &str) {
    let _ = Command::new("sc.exe").args(["start", name]).output();
}

fn kill_process_best_effort(process_name: &str) {
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut killed_any = false;
    for p in sys.processes().values() {
        let pname = p.name().to_string_lossy().to_string();
        if pname.eq_ignore_ascii_case(process_name) {
            let _ = p.kill();
            killed_any = true;
        }
    }

    if killed_any {
        std::thread::sleep(Duration::from_millis(300));
    }
}

/* ========================= UI / вывод ========================= */

fn print_banner() {
    let title = "Обновление MuZap";
    let line = "=".repeat(58);
    println!("{line}");
    println!("{:^58}", title);
    println!("{line}");
    println!("Версия программы: {}", env!("CARGO_PKG_VERSION"));
    println!();
}

fn println_kv(k: &str, v: &str) {
    println!("{:<18}: {}", k, v);
}

fn print_ok(msg: &str) {
    println_colored("OK", Color::Green, msg);
}

fn print_warn(msg: &str) {
    println_colored("ВНИМАНИЕ", Color::Yellow, msg);
}

fn print_info(msg: &str) {
    println_colored("ИНФО", Color::Cyan, msg);
}

fn println_colored(tag: &str, color: Color, msg: &str) {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(color),
        Print(format!("[{tag}] ")),
        ResetColor,
        Print(msg),
        Print("\n")
    );
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} ГБ", b / GB)
    } else if b >= MB {
        format!("{:.2} МБ", b / MB)
    } else if b >= KB {
        format!("{:.2} КБ", b / KB)
    } else {
        format!("{bytes} байт")
    }
}

fn pause_enter() -> io::Result<()> {
    println!();
    println!("Нажмите Enter, чтобы закрыть окно...");
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s)?;
    Ok(())
}

/* ========================= Стрелочный выбор Да/Нет ========================= */

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

struct CursorGuard;

impl CursorGuard {
    fn hide() -> io::Result<Self> {
        execute!(io::stdout(), cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show);
    }
}

fn clear_pending_events() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

fn ask_yes_no_tui(question: &str, hint: &str, default_yes: bool) -> AppResult<bool> {
    let _raw = RawModeGuard::enable()?;
    let _cur = CursorGuard::hide()?;
    let mut stdout = io::stdout();

    clear_pending_events();

    // 0 = Да, 1 = Нет
    let mut selection: usize = if default_yes { 0 } else { 1 };
    let mut warning: Option<&'static str> = None;

    println!();
    println!("{}", "-".repeat(58));
    println!("{question}");
    println!("{hint}");
    println!("Выберите действие:");
    println!();

    render_menu(&mut stdout, selection, warning)?;

    loop {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(KeyEvent { code, kind, .. }) = event::read()? {
                // Игнорируем Release, иначе Up/Down переключит туда-сюда
                if kind == KeyEventKind::Release {
                    continue;
                }

                match code {
                    KeyCode::Up | KeyCode::Down => {
                        selection = 1 - selection; // 0<->1
                        warning = None;
                        rerender_menu(&mut stdout, selection, warning)?;
                    }
                    KeyCode::Enter => {
                        println!("{}", "-".repeat(58));
                        return Ok(selection == 0);
                    }
                    KeyCode::Esc => {
                        println!("{}", "-".repeat(58));
                        return Ok(false);
                    }
                    _ => {
                        warning = Some("Допустимо только: ↑↓, Enter, Esc");
                        rerender_menu(&mut stdout, selection, warning)?;
                    }
                }
            }
        }
    }
}

fn render_menu(
    stdout: &mut impl Write,
    selection: usize,
    warning: Option<&'static str>,
) -> io::Result<()> {
    draw_choice_line(stdout, "Да, обновить", selection == 0)?;
    draw_choice_line(stdout, "Нет, выйти", selection == 1)?;

    if let Some(w) = warning {
        execute!(
            stdout,
            SetForegroundColor(Color::Yellow),
            Print(w),
            ResetColor,
            Print("\n")
        )?;
    } else {
        execute!(stdout, Print("\n"))?;
    }

    stdout.flush()?;
    Ok(())
}

fn rerender_menu(
    stdout: &mut impl Write,
    selection: usize,
    warning: Option<&'static str>,
) -> io::Result<()> {
    // Всегда печатаем ровно 3 строки: Да/Нет/предупреждение.
    execute!(stdout, cursor::MoveUp(3), cursor::MoveToColumn(0))?;
    execute!(stdout, Clear(ClearType::FromCursorDown))?;
    render_menu(stdout, selection, warning)
}

fn draw_choice_line(stdout: &mut impl Write, text: &str, selected: bool) -> io::Result<()> {
    if selected {
        execute!(
            stdout,
            SetForegroundColor(Color::Cyan),
            Print("  ► "),
            Print(text),
            ResetColor,
            Print("\n")
        )?;
    } else {
        execute!(stdout, Print("    "), Print(text), Print("\n"))?;
    }
    Ok(())
}
