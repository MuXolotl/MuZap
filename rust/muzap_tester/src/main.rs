mod analytics;
mod checker;
mod config;
mod dpi;
mod report;
mod runner;
mod telemetry;
mod ui;

use std::{fs, io};

use crossterm::execute;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};

use analytics::{build_analytics, find_best, print_analytics, print_summary_table};
use checker::run_standard_tests;
use config::{load_app_config, load_strategies, load_targets, IpsetStatus};
use dpi::run_dpi_tests;
use report::save_report;
use runner::{restore_winws_snapshot, stop_zapret, take_winws_snapshot, wait_for_winws};
use telemetry::send_telemetry;
use ui::{ask_config_selection, ask_mode, ask_test_type, TestMode, TestType};

// ─── Структуры результатов ────────────────────────────────────────────────────

pub struct RunResult {
    pub strategy_name: String,
    pub test_type: TestType,
    pub standard: Option<Vec<checker::TargetResult>>,
    pub dpi: Option<Vec<dpi::DpiTargetResult>>,
}

// ─── Точка входа ─────────────────────────────────────────────────────────────

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let no_pause = argv.iter().any(|a| a == "--no-pause");

    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        if !no_pause {
            pause();
        }
        return;
    }

    let result = run();

    if let Err(e) = &result {
        print_colored_tag("[ОШИБКА]", Color::Red, &e.to_string());
    }

    if !no_pause {
        pause();
    }

    if result.is_err() {
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError> {
    let root = config::detect_root()?;
    let app_cfg = load_app_config(&root.join("muzap.ini"));

    check_admin()?;

    // Проверяем флаг прерванного ipset-переключения
    let ipset_flag = root.join("ipset_switched.flag");
    if ipset_flag.exists() {
        print_colored_tag(
            "[ИНФО]",
            Color::Yellow,
            "Обнаружен флаг незавершённого переключения ipset. Восстанавливаю...",
        );
        config::restore_ipset(&root)?;
        let _ = fs::remove_file(&ipset_flag);
    }

    let original_ipset = config::get_ipset_status(&root);

    let strategies_path = root.join("strategies.ini");
    if !strategies_path.exists() {
        return Err(AppError::msg("strategies.ini не найден в корне MuZap."));
    }
    let all_strategies = load_strategies(&strategies_path)?;

    if runner::is_muzap_service_running() {
        return Err(AppError::msg(
            "Служба MuZap запущена. Удалите её через MuZap.bat → Сервис → Удалить, затем запустите тест снова.",
        ));
    }

    let winws_snapshot = take_winws_snapshot();

    let test_type = ask_test_type()?;
    let mode = ask_mode()?;

    let strategies_to_test = match mode {
        TestMode::All => all_strategies.clone(),
        TestMode::Select => ask_config_selection(&all_strategies)?,
    };

    if strategies_to_test.is_empty() {
        return Err(AppError::msg("Не выбрано ни одной стратегии."));
    }

    let targets = if test_type == TestType::Standard {
        Some(load_targets(&root.join("utils").join("targets.txt")))
    } else {
        None
    };

    let bin_path = root.join("bin").to_string_lossy().to_string() + "\\";
    let lists_path = root.join("lists").to_string_lossy().to_string() + "\\";
    let winws_exe = root.join("bin").join("winws.exe");

    if !winws_exe.exists() {
        return Err(AppError::msg(format!(
            "winws.exe не найден: {}",
            winws_exe.display()
        )));
    }

    let game_ports = config::get_game_filter_ports(&app_cfg);

    let dpi_targets = if test_type == TestType::Dpi {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match rt.block_on(dpi::fetch_dpi_suite()) {
            Ok(t) => t,
            Err(e) => {
                print_colored_tag(
                    "[ПРЕДУПРЕЖДЕНИЕ]",
                    Color::Yellow,
                    &format!("Не удалось загрузить DPI-сюиту: {e}."),
                );
                vec![]
            }
        }
    } else {
        vec![]
    };

    // Если DPI + ipset не "any" → переключаем
    let ipset_switched = if test_type == TestType::Dpi && original_ipset != IpsetStatus::Any {
        print_colored_tag(
            "[ПРЕДУПРЕЖДЕНИЕ]",
            Color::Yellow,
            &format!(
                "Ipset в режиме '{}'. Переключаю в 'any' для корректных DPI-тестов...",
                original_ipset.as_str()
            ),
        );
        config::switch_ipset_to_any(&root)?;
        fs::write(&ipset_flag, "")?;
        true
    } else {
        false
    };

    print_banner(strategies_to_test.len(), &test_type);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let mut all_results: Vec<RunResult> = Vec::new();
    let total = strategies_to_test.len();

    for (idx, strategy) in strategies_to_test.iter().enumerate() {
        let num = idx + 1;
        println!();
        print_separator();
        print_colored_tag(
            &format!("[{num}/{total}]"),
            Color::Yellow,
            &format!("{} — {}", strategy.name, strategy.description),
        );
        print_separator();

        let params = substitute_params(
            &strategy.params,
            &bin_path,
            &lists_path,
            &game_ports.tcp,
            &game_ports.udp,
        );

        stop_zapret();

        print_colored_tag("[ІНФО]", Color::Cyan, "Запускаю winws...");
        let proc = runner::start_winws(&winws_exe, &params)?;

        if !wait_for_winws(5) {
            print_colored_tag(
                "[ПРЕДУПРЕЖДЕНИЕ]",
                Color::Yellow,
                "winws не появился за 5 сек., продолжаю...",
            );
        }

        let result = match test_type {
            TestType::Standard => {
                let targets_ref = targets.as_ref().unwrap();
                print_colored_tag("[ІНФО]", Color::Cyan, "Запускаю HTTP/ping тесты...");
                let results = rt.block_on(run_standard_tests(targets_ref));
                RunResult {
                    strategy_name: strategy.name.clone(),
                    test_type: TestType::Standard,
                    standard: Some(results),
                    dpi: None,
                }
            }
            TestType::Dpi => {
                print_colored_tag("[ІНФО]", Color::Cyan, "Запускаю DPI-тесты...");
                let results = rt.block_on(run_dpi_tests(&dpi_targets));
                RunResult {
                    strategy_name: strategy.name.clone(),
                    test_type: TestType::Dpi,
                    standard: None,
                    dpi: Some(results),
                }
            }
        };

        print_run_result(&result);
        all_results.push(result);

        stop_zapret();
        if let Some(mut p) = proc {
            let _ = p.kill();
        }
    }

    let analytics = build_analytics(&all_results);
    println!();
    print_analytics(&analytics);
    print_summary_table(&analytics);

    if let Some(best) = find_best(&analytics) {
        println!();
        print_colored_tag(
            "[ЛУЧШАЯ]",
            Color::Green,
            &format!("Лучшая стратегия: {best}"),
        );
    }

    let report_dir = root.join("utils").join("test results");
    fs::create_dir_all(&report_dir)?;
    let report_path = save_report(&report_dir, &all_results, &analytics)?;
    println!();
    print_colored_tag(
        "[ІНФО]",
        Color::Cyan,
        &format!("Результаты сохранены: {}", report_path.display()),
    );

    if test_type == TestType::Standard && app_cfg.telemetry_enabled {
        rt.block_on(send_telemetry(&analytics, &app_cfg.version));
    } else if test_type == TestType::Standard {
        println!();
        print_colored_tag(
            "[Телеметрия]",
            Color::DarkGrey,
            "Отключена. Включите в настройках MuZap для отправки результатов.",
        );
    }

    if ipset_switched {
        print_colored_tag("[ІНФО]", Color::DarkGrey, "Восстанавливаю ipset...");
        let _ = config::restore_ipset(&root);
        let _ = fs::remove_file(&ipset_flag);
    }

    restore_winws_snapshot(&winws_snapshot);

    println!();
    print_colored_tag("[ГОТОВО]", Color::Green, "Все тесты завершены.");

    Ok(())
}

// ─── Вспомогательные функции ──────────────────────────────────────────────────

fn substitute_params(params: &str, bin: &str, lists: &str, tcp: &str, udp: &str) -> String {
    params
        .replace("%BIN%", bin)
        .replace("%LISTS%", lists)
        .replace("%GameFilterTCP%", tcp)
        .replace("%GameFilterUDP%", udp)
        .replace("EXCL_MARK", "!")
}

fn print_run_result(r: &RunResult) {
    if let Some(std_res) = &r.standard {
        for tr in std_res {
            print!("  {:<36}", tr.name);
            if tr.is_url {
                for tok in &tr.http_tokens {
                    let color = if tok.status == "OK" {
                        Color::Green
                    } else if tok.status == "UNSUP" {
                        Color::Yellow
                    } else {
                        Color::Red
                    };
                    print_colored_inline(&tok.display(), color);
                    print!(" ");
                }
            }
            print!("| Ping: ");
            let ping_color = if tr.ping_ms.is_some() {
                Color::Cyan
            } else {
                Color::Yellow
            };
            let ping_str = tr
                .ping_ms
                .map(|ms| format!("{ms} мс"))
                .unwrap_or_else(|| "Таймаут".to_string());
            print_colored_inline(&ping_str, ping_color);
            println!();
        }
    }

    if let Some(dpi_res) = &r.dpi {
        for dr in dpi_res {
            println!(
                "  === [{}][{}] {} ===",
                dr.country, dr.provider, dr.target_id
            );
            for line in &dr.lines {
                let color = match line.status.as_str() {
                    "OK" => Color::Green,
                    "LIKELY_BLOCKED" | "UNSUPPORTED" => Color::Yellow,
                    _ => Color::Red,
                };
                print_colored_tag(
                    &format!("[{}]", line.test_label),
                    color,
                    &format!(
                        "code={} up={:.1}KB down={:.1}KB time={:.2}s status={}",
                        line.code, line.up_kb, line.down_kb, line.time_secs, line.status
                    ),
                );
            }
        }
    }
}

fn print_banner(count: usize, test_type: &TestType) {
    let sep = "=".repeat(60);
    println!("{sep}");
    println!("{:^60}", "MUZAP CONFIG TESTER");
    println!(
        "{:^60}",
        format!(
            "Режим: {}  |  Стратегий: {count}",
            match test_type {
                TestType::Standard => "СТАНДАРТНЫЙ",
                TestType::Dpi => "DPI",
            }
        )
    );
    println!("{sep}");
}

fn print_separator() {
    println!("{}", "-".repeat(60));
}

// ─── Цветной вывод ────────────────────────────────────────────────────────────

pub fn print_colored_tag(tag: &str, color: Color, msg: &str) {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(color),
        Print(format!("{tag} ")),
        ResetColor,
        Print(msg),
        Print("\n")
    );
}

pub fn print_colored_inline(text: &str, color: Color) {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, SetForegroundColor(color), Print(text), ResetColor);
}

// ─── Проверка прав администратора ────────────────────────────────────────────

fn check_admin() -> Result<(), AppError> {
    if !windows_check::is_elevated() {
        return Err(AppError::msg(
            "Требуются права администратора. Запустите через MuZap.bat.",
        ));
    }
    Ok(())
}

#[cfg(windows)]
mod windows_check {
    use std::mem;

    pub fn is_elevated() -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut size: u32 = 0;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            );
            CloseHandle(token);
            ok != 0 && elevation.TokenIsElevated != 0
        }
    }
}

#[cfg(not(windows))]
mod windows_check {
    pub fn is_elevated() -> bool {
        true
    }
}

// ─── Пауза ────────────────────────────────────────────────────────────────────

fn pause() {
    // Явно выключаем raw-режим и показываем курсор перед ожиданием Enter
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), crossterm::cursor::Show);

    println!();
    println!("Нажмите Enter для выхода...");
    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
}

fn print_help() {
    println!(
        r#"muzap_tester — тестирование стратегий MuZap

Аргументы:
  --no-pause    Не ждать Enter перед закрытием (для автоматизации)
  --help, -h    Помощь
"#
    );
}

// ─── Тип ошибки ───────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),

    // tokio::io::Error это псевдоним std::io::Error, поэтому одного варианта достаточно
    #[error("Ошибка ввода/вывода: {0}")]
    Io(#[from] io::Error),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }
}
