use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute, queue,
    style::{Attribute, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{
    diagnostics, fixes, ipset, service, settings, strategies, tools, updates, AppError, AppResult,
};

pub fn run() -> AppResult<()> {
    let _guard = TerminalGuard::enter()?;

    loop {
        let root: PathBuf = strategies::detect_root_dir()?;
        let rootp: &Path = root.as_path();

        ipset::bootstrap_user_lists(rootp)?;

        let app_cfg = settings::load_app_config_from_root(rootp);

        let selected = service::read_selected_strategy(rootp)?;
        let state = service::query_service_state_text().unwrap_or_else(|_| "UNKNOWN".into());
        let ipset_mode = ipset::detect_mode(rootp);

        let header = build_header(
            rootp,
            &app_cfg.version,
            selected.as_deref(),
            &state,
            &app_cfg,
            ipset_mode,
        );

        let items = vec![
            "Служба".to_string(),
            "Настройки".to_string(),
            "Обновления".to_string(),
            "Инструменты".to_string(),
            "Стратегии (просмотр)".to_string(),
            "Выход".to_string(),
        ];

        let choice = arrow_menu(&header, &items, 0)?;
        let Some(choice) = choice else {
            return Ok(());
        };

        match choice {
            0 => service_menu(rootp)?,
            1 => settings_menu(rootp)?,
            2 => updates_menu(rootp)?,
            3 => tools_menu(rootp)?,
            4 => strategies_menu(rootp)?,
            5 => return Ok(()),
            _ => {}
        }
    }
}

fn build_header(
    root: &Path,
    version: &str,
    strategy: Option<&str>,
    state: &str,
    cfg: &muzap_core::app::AppConfig,
    ipset_mode: ipset::IpSetMode,
) -> String {
    let mut s = String::new();
    s.push_str("MUZAP (Rust)\n");
    s.push_str(&format!("Версия: {version}\n"));
    s.push_str(&format!("Каталог: {}\n", root.display()));
    s.push_str(&format!("Служба: {state}\n"));
    s.push_str(&format!(
        "Стратегия: {}\n",
        strategy.unwrap_or("не выбрана")
    ));
    s.push_str(&format!(
        "Game Filter: {}\n",
        settings::game_filter_mode_ru(&cfg.game_filter_mode)
    ));
    s.push_str(&format!(
        "Телеметрия: {}\n",
        if cfg.telemetry_enabled {
            "вкл"
        } else {
            "выкл"
        }
    ));
    s.push_str(&format!(
        "IPSet: {} ({})\n",
        ipset_mode.as_str(),
        ipset_mode.ru()
    ));
    s.push_str("\nУправление: ↑↓ — выбрать, Enter — открыть, Esc — назад/выход\n");
    s
}

/* ========================= Служба ========================= */

fn service_menu(root: &Path) -> AppResult<()> {
    loop {
        let selected = service::read_selected_strategy(root)?;
        let state = service::query_service_state_text().unwrap_or_else(|_| "UNKNOWN".into());
        let installed = service::is_service_installed().unwrap_or(false);

        let title = format!(
            "Служба MuZap\n\nСостояние: {state}\nСтратегия: {}\n\nВыберите действие:",
            selected.as_deref().unwrap_or("не выбрана")
        );

        let items = vec![
            if installed {
                "Запустить/применить стратегию (перезапуск)".to_string()
            } else {
                "Установить и запустить службу".to_string()
            },
            "Сменить стратегию".to_string(),
            "Статус".to_string(),
            "Перезапустить службу".to_string(),
            "Удалить службу".to_string(),
            "Назад".to_string(),
        ];

        let choice = arrow_menu(&title, &items, 0)?;
        let Some(choice) = choice else {
            return Ok(());
        };

        match choice {
            0 => {
                let all = strategies::load_strategies_from_root(root)?;
                let picked = select_strategy_menu(&all, 0)?;
                let Some(picked) = picked else {
                    continue;
                };

                service::write_selected_strategy(root, &picked)?;

                if service::is_service_installed()? {
                    match service::restart_service() {
                        Ok(_) => show_message(
                            "OK",
                            &format!("Стратегия применена, служба перезапущена: {picked}"),
                        )?,
                        Err(e) => {
                            show_message("ОШИБКА", &format!("Не удалось перезапустить: {e}"))?
                        }
                    }
                } else {
                    match service::install_muzap_service(root, false, true) {
                        Ok(_) => show_message(
                            "OK",
                            &format!("Служба установлена и запущена. Стратегия: {picked}"),
                        )?,
                        Err(e) => show_message("ОШИБКА", &format!("Не удалось установить: {e}"))?,
                    }
                }
            }

            1 => {
                let all = strategies::load_strategies_from_root(root)?;
                let picked = select_strategy_menu(&all, 0)?;
                let Some(picked) = picked else {
                    continue;
                };

                service::write_selected_strategy(root, &picked)?;

                if service::is_service_installed()? {
                    match service::restart_service() {
                        Ok(_) => show_message(
                            "OK",
                            &format!("Стратегия обновлена и применена (перезапуск): {picked}"),
                        )?,
                        Err(e) => {
                            show_message("ОШИБКА", &format!("Не удалось перезапустить: {e}"))?
                        }
                    }
                } else {
                    show_message(
                        "ИНФО",
                        "Служба не установлена. Выберите: Установить и запустить службу.",
                    )?;
                }
            }

            2 => {
                clear_screen()?;
                service::print_status(root)?;
                wait_any_key("Нажмите любую клавишу, чтобы вернуться...")?;
            }

            3 => {
                if !service::is_service_installed()? {
                    show_message("ИНФО", "Служба не установлена.")?;
                    continue;
                }

                match service::restart_service() {
                    Ok(_) => show_message("OK", "Команда перезапуска отправлена.")?,
                    Err(e) => show_message("ОШИБКА", &format!("Не удалось перезапустить: {e}"))?,
                }
            }

            4 => {
                if !service::is_service_installed()? {
                    show_message("ИНФО", "Служба не установлена (удалять нечего).")?;
                    continue;
                }

                let ok = confirm_menu("Удалить службу MuZap?")?;
                if !ok {
                    continue;
                }

                match service::remove_service_best_effort(true) {
                    Ok(_) => show_message("OK", "Удаление службы выполнено (best-effort).")?,
                    Err(e) => show_message("ОШИБКА", &format!("Не удалось удалить: {e}"))?,
                }
            }

            5 => return Ok(()),

            _ => {}
        }
    }
}

/* ========================= Настройки ========================= */

fn settings_menu(root: &Path) -> AppResult<()> {
    loop {
        let cfg = settings::load_app_config_from_root(root);
        let ipm = ipset::detect_mode(root);

        let title =
            format!(
            "Настройки\n\nGame Filter: {}\nТелеметрия: {}\nIPSet: {} ({})\n\nВыберите действие:",
            settings::game_filter_mode_ru(&cfg.game_filter_mode),
            if cfg.telemetry_enabled { "вкл" } else { "выкл" },
            ipm.as_str(),
            ipm.ru(),
        );

        let items = vec![
            "Game Filter (режим портов игр)".to_string(),
            "IPSet Filter (none/any/loaded)".to_string(),
            "Телеметрия (анонимные результаты тестов)".to_string(),
            "Назад".to_string(),
        ];

        let choice = arrow_menu(&title, &items, 0)?;
        let Some(choice) = choice else {
            return Ok(());
        };

        match choice {
            0 => {
                let modes = [
                    ("off", "выкл"),
                    ("all", "TCP+UDP (все порты игр)"),
                    ("tcp", "только TCP (порты игр)"),
                    ("udp", "только UDP (порты игр)"),
                ];

                let mode_items: Vec<String> =
                    modes.iter().map(|(v, ru)| format!("{ru}  ({v})")).collect();

                let idx = arrow_menu(
                    "Game Filter Mode\n\nEnter — выбрать, Esc — отмена",
                    &mode_items,
                    0,
                )?;
                let Some(idx) = idx else {
                    continue;
                };
                let val = modes[idx].0;

                settings::set_game_filter_mode(root, val)?;
                let mut msg = format!("GameFilterMode установлен: {val}");

                if service::is_service_installed().unwrap_or(false) {
                    let ok = confirm_menu("Перезапустить службу сейчас, чтобы применить?")?;
                    if ok {
                        match service::restart_service() {
                            Ok(_) => msg.push_str("\nСлужба перезапущена."),
                            Err(e) => msg.push_str(&format!("\nНе удалось перезапустить: {e}")),
                        }
                    } else {
                        msg.push_str("\nПерезапуск пропущен. Применится после restart.");
                    }
                }

                show_message("OK", &msg)?;
            }

            1 => {
                let items = vec![
                    "none — заглушка (фильтр выключен)".to_string(),
                    "any — пустой файл (фильтр для всех IP)".to_string(),
                    "loaded — восстановить из backup".to_string(),
                    "Отмена".to_string(),
                ];

                let def = match ipm {
                    ipset::IpSetMode::None => 0,
                    ipset::IpSetMode::Any => 1,
                    ipset::IpSetMode::Loaded => 2,
                };

                let idx = arrow_menu("IPSet Filter\n\nEnter — выбрать, Esc — отмена", &items, def)?;
                let Some(idx) = idx else {
                    continue;
                };
                if idx == 3 {
                    continue;
                }

                let target = match idx {
                    0 => ipset::IpSetMode::None,
                    1 => ipset::IpSetMode::Any,
                    2 => ipset::IpSetMode::Loaded,
                    _ => ipset::IpSetMode::None,
                };

                let msg = match ipset::set_mode(root, target) {
                    Ok(m) => m,
                    Err(e) => {
                        show_message("ОШИБКА", &format!("{e}"))?;
                        continue;
                    }
                };

                let mut msg2 = msg;
                if service::is_service_installed().unwrap_or(false) {
                    let ok = confirm_menu("Перезапустить службу сейчас, чтобы применить?")?;
                    if ok {
                        match service::restart_service() {
                            Ok(_) => msg2.push_str("\nСлужба перезапущена."),
                            Err(e) => msg2.push_str(&format!("\nНе удалось перезапустить: {e}")),
                        }
                    } else {
                        msg2.push_str("\nПерезапуск пропущен. Применится после restart.");
                    }
                }

                show_message("OK", &msg2)?;
            }

            2 => {
                let items = vec![
                    "Включить".to_string(),
                    "Выключить".to_string(),
                    "Отмена".to_string(),
                ];

                let idx = arrow_menu(
                    "Телеметрия\n\nОтправляет анонимные результаты стандартных тестов.\n\nEnter — выбрать, Esc — отмена",
                    &items,
                    if cfg.telemetry_enabled { 0 } else { 1 },
                )?;
                let Some(idx) = idx else {
                    continue;
                };
                if idx == 2 {
                    continue;
                }

                let new_val = idx == 0;
                settings::set_telemetry_enabled(root, new_val)?;

                let mut msg = format!(
                    "TelemetryEnabled установлен: {}",
                    if new_val { "true" } else { "false" }
                );

                if service::is_service_installed().unwrap_or(false) {
                    let ok = confirm_menu("Перезапустить службу сейчас, чтобы применить?")?;
                    if ok {
                        match service::restart_service() {
                            Ok(_) => msg.push_str("\nСлужба перезапущена."),
                            Err(e) => msg.push_str(&format!("\nНе удалось перезапустить: {e}")),
                        }
                    } else {
                        msg.push_str("\nПерезапуск пропущен. Применится после restart.");
                    }
                }

                show_message("OK", &msg)?;
            }

            3 => return Ok(()),

            _ => {}
        }
    }
}

/* ========================= Обновления ========================= */

fn updates_menu(root: &Path) -> AppResult<()> {
    loop {
        let title = "Обновления\n\nВыберите действие:";

        let items = vec![
            "Обновить IPSet List (lists\\ipset-all.txt)".to_string(),
            "Обновить Hosts File (блок MuZap)".to_string(),
            "Удалить Hosts Entries (блок MuZap)".to_string(),
            "Проверить обновления сборки".to_string(),
            "Установить обновление сборки".to_string(),
            "Назад".to_string(),
        ];

        let choice = arrow_menu(title, &items, 0)?;
        let Some(choice) = choice else {
            return Ok(());
        };

        match choice {
            0 => match updates::update_ipset_list(root) {
                Ok(_) => show_message("OK", "IPSet успешно обновлён.")?,
                Err(e) => show_message("ОШИБКА", &format!("IPSet update не удался: {e}"))?,
            },

            1 => {
                let ok = confirm_menu("Обновить hosts (блок MuZap) прямо сейчас?")?;
                if !ok {
                    continue;
                }

                match updates::update_hosts_block(root, false) {
                    Ok(_) => show_message("OK", "Hosts блок обновлён.")?,
                    Err(e) => show_message("ОШИБКА", &format!("Hosts update не удался: {e}"))?,
                }
            }

            2 => {
                let ok = confirm_menu("Удалить MuZap-блок из hosts?")?;
                if !ok {
                    continue;
                }

                match updates::remove_hosts_block(root, false) {
                    Ok(_) => show_message("OK", "Hosts блок удалён (если был).")?,
                    Err(e) => show_message("ОШИБКА", &format!("Hosts remove не удался: {e}"))?,
                }
            }

            3 => {
                let status =
                    run_external_suspended(|| updates::run_release_updater_check(root, false));
                match status {
                    Ok(_) => show_message("OK", "Проверка обновлений завершена.")?,
                    Err(e) => {
                        show_message("ОШИБКА", &format!("Проверка обновлений не удалась: {e}"))?
                    }
                }
            }

            4 => {
                let ok = confirm_menu("Установить обновление сборки?")?;
                if !ok {
                    continue;
                }

                let status =
                    run_external_suspended(|| updates::run_release_updater_install(root, false));
                match status {
                    Ok(_) => show_message("OK", "Установка обновления завершена.")?,
                    Err(e) => {
                        show_message("ОШИБКА", &format!("Установка обновления не удалась: {e}"))?
                    }
                }
            }

            5 => return Ok(()),
            _ => {}
        }
    }
}

/* ========================= Инструменты ========================= */

fn tools_menu(root: &Path) -> AppResult<()> {
    loop {
        let title = "Инструменты\n\nВыберите действие:";

        let items = vec![
            "Диагностика".to_string(),
            "Исправления (best-effort)".to_string(),
            "Запустить тестер стратегий".to_string(),
            "Просмотреть лог службы".to_string(),
            "Открыть папку MuZap".to_string(),
            "Назад".to_string(),
        ];

        let choice = arrow_menu(title, &items, 0)?;
        let Some(choice) = choice else {
            return Ok(());
        };

        match choice {
            0 => {
                let its = diagnostics::run_all(root)?;
                let lines = diagnostics::render_lines(&its);
                scroll_view("Диагностика\n\nEsc/Enter — назад", &lines)?;
            }

            1 => fixes_menu(root)?,

            2 => {
                let status = run_external_suspended(|| tools::run_tester(root));
                match status {
                    Ok(_) => show_message("OK", "Тестер завершён.")?,
                    Err(e) => show_message("ОШИБКА", &format!("Тестер завершился с ошибкой: {e}"))?,
                }
            }

            3 => {
                let lines = tools::read_service_log_lines(root)?;
                scroll_view("Лог службы\n\nEsc/Enter — назад", &lines)?;
            }

            4 => match tools::open_root_folder(root) {
                Ok(_) => show_message("OK", "Открыто.")?,
                Err(e) => show_message("ОШИБКА", &format!("Не удалось открыть: {e}"))?,
            },

            5 => return Ok(()),
            _ => {}
        }
    }
}

fn fixes_menu(_root: &Path) -> AppResult<()> {
    loop {
        let title = "Исправления (best-effort)\n\nВнимание: некоторые действия удаляют службы/кэш.\n\nВыберите действие:";

        let items = vec![
            "Выполнить всё".to_string(),
            "Удалить конфликтующие службы + прибить winws".to_string(),
            "Удалить службы WinDivert/WinDivert14".to_string(),
            "Включить TCP timestamps".to_string(),
            "Очистить кэш Discord".to_string(),
            "Назад".to_string(),
        ];

        let choice = arrow_menu(title, &items, 0)?;
        let Some(choice) = choice else {
            return Ok(());
        };

        match choice {
            0 => {
                let ok = confirm_menu("Выполнить ВСЕ исправления?")?;
                if !ok {
                    continue;
                }
                let rep = fixes::fix_all()?;
                scroll_view("Исправления: отчёт\n\nEsc/Enter — назад", &rep)?;
            }

            1 => {
                let ok = confirm_menu("Удалить конфликтующие службы и завершить winws?")?;
                if !ok {
                    continue;
                }
                let rep = fixes::fix_remove_conflicting_services()?;
                scroll_view("Исправления: отчёт\n\nEsc/Enter — назад", &rep)?;
            }

            2 => {
                let ok = confirm_menu("Удалить службы WinDivert/WinDivert14?")?;
                if !ok {
                    continue;
                }
                let rep = fixes::fix_remove_windivert_services()?;
                scroll_view("Исправления: отчёт\n\nEsc/Enter — назад", &rep)?;
            }

            3 => {
                let ok = confirm_menu("Включить TCP timestamps?")?;
                if !ok {
                    continue;
                }
                let rep = fixes::fix_enable_tcp_timestamps()?;
                scroll_view("Исправления: отчёт\n\nEsc/Enter — назад", &rep)?;
            }

            4 => {
                let ok = confirm_menu("Закрыть Discord и очистить кэш?")?;
                if !ok {
                    continue;
                }
                let rep = fixes::fix_clear_discord_cache()?;
                scroll_view("Исправления: отчёт\n\nEsc/Enter — назад", &rep)?;
            }

            5 => return Ok(()),

            _ => {}
        }
    }
}

/* ========================= Стратегии ========================= */

fn strategies_menu(root: &Path) -> AppResult<()> {
    let list = strategies::load_strategies_from_root(root)?;
    if list.is_empty() {
        show_message("ИНФО", "В strategies.ini не найдено стратегий.")?;
        return Ok(());
    }

    let lines: Vec<String> = list
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = i + 1;
            let desc = if s.description.trim().is_empty() {
                "Без описания"
            } else {
                s.description.trim()
            };
            format!("{n:>3}. {:<22} — {desc}", s.name)
        })
        .collect();

    scroll_view("Стратегии (просмотр)\n\nEsc/Enter — назад", &lines)?;
    Ok(())
}

fn select_strategy_menu(
    list: &[muzap_core::config_files::Strategy],
    default: usize,
) -> AppResult<Option<String>> {
    let items: Vec<String> = list
        .iter()
        .map(|s| {
            let desc = if s.description.trim().is_empty() {
                "Без описания"
            } else {
                s.description.trim()
            };
            format!("{} — {}", s.name, desc)
        })
        .collect();

    let idx = arrow_menu(
        "Выберите стратегию:\n\nEnter — выбрать, Esc — отмена",
        &items,
        default,
    )?;
    Ok(idx.map(|i| list[i].name.clone()))
}

/* ========================= Terminal helpers ========================= */

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> AppResult<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)?;
        clear_screen()?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), cursor::Show, LeaveAlternateScreen, ResetColor);
    }
}

struct TerminalSuspend;

impl TerminalSuspend {
    fn enter() -> AppResult<Self> {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), cursor::Show, LeaveAlternateScreen, ResetColor);
        Ok(Self)
    }
}

impl Drop for TerminalSuspend {
    fn drop(&mut self) {
        let _ = terminal::enable_raw_mode();
        let _ = execute!(io::stdout(), EnterAlternateScreen, cursor::Hide, ResetColor);
        let _ = execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        );
    }
}

fn run_external_suspended<T>(f: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    let _s = TerminalSuspend::enter()?;
    println!();
    let r = f();
    println!();
    r
}

fn clear_screen() -> AppResult<()> {
    execute!(
        io::stdout(),
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    Ok(())
}

fn arrow_menu(title: &str, items: &[String], default: usize) -> AppResult<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }

    let mut stdout = io::stdout();
    let mut selection = default.min(items.len().saturating_sub(1));

    drain_events();

    loop {
        render_arrow_menu(&mut stdout, title, items, selection)?;
        match next_key_press()? {
            KeyCode::Up => selection = selection.saturating_sub(1),
            KeyCode::Down => {
                if selection + 1 < items.len() {
                    selection += 1;
                }
            }
            KeyCode::Enter => return Ok(Some(selection)),
            KeyCode::Esc => return Ok(None),
            _ => {}
        }
    }
}

fn render_arrow_menu(
    stdout: &mut impl Write,
    title: &str,
    items: &[String],
    selection: usize,
) -> AppResult<()> {
    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All),
        SetForegroundColor(crossterm::style::Color::Cyan),
        Print(title),
        ResetColor,
        Print("\n")
    )?;

    for (i, item) in items.iter().enumerate() {
        if i == selection {
            queue!(
                stdout,
                SetForegroundColor(crossterm::style::Color::Cyan),
                Print("  ► "),
                SetAttribute(Attribute::Bold),
                Print(item),
                SetAttribute(Attribute::Reset),
                ResetColor,
                Print("\n")
            )?;
        } else {
            queue!(stdout, Print("    "), Print(item), Print("\n"))?;
        }
    }

    stdout.flush()?;
    Ok(())
}

fn confirm_menu(question: &str) -> AppResult<bool> {
    let items = vec!["Да".to_string(), "Нет".to_string()];
    let title = format!("{question}\n\n↑↓ — выбрать, Enter — подтвердить, Esc — отмена");
    let idx = arrow_menu(&title, &items, 1)?;
    Ok(matches!(idx, Some(0)))
}

fn show_message(tag: &str, msg: &str) -> AppResult<()> {
    clear_screen()?;
    println!("[{tag}] {msg}");
    println!();
    wait_any_key("Нажмите любую клавишу...")?;
    Ok(())
}

fn wait_any_key(hint: &str) -> AppResult<()> {
    println!("{hint}");
    loop {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(KeyEvent { kind, .. }) = event::read()? {
                if kind == KeyEventKind::Press {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn scroll_view(title: &str, lines: &[String]) -> AppResult<()> {
    let mut stdout = io::stdout();
    let mut offset: usize = 0;

    drain_events();

    loop {
        render_scroll_view(&mut stdout, title, lines, offset)?;
        match next_key_press()? {
            KeyCode::Up => offset = offset.saturating_sub(1),
            KeyCode::Down => {
                if offset + 1 < lines.len() {
                    offset += 1;
                }
            }
            KeyCode::PageUp => offset = offset.saturating_sub(10),
            KeyCode::PageDown => offset = (offset + 10).min(lines.len().saturating_sub(1)),
            KeyCode::Enter | KeyCode::Esc => return Ok(()),
            _ => {}
        }
    }
}

fn render_scroll_view(
    stdout: &mut impl Write,
    title: &str,
    lines: &[String],
    offset: usize,
) -> AppResult<()> {
    let (w, h) = terminal::size().unwrap_or((80, 24));
    let width = w as usize;
    let height = h as usize;

    let header_lines = count_lines(title) + 2;
    let usable = height.saturating_sub(header_lines).max(5);
    let end = (offset + usable).min(lines.len());

    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All),
        SetForegroundColor(crossterm::style::Color::Cyan),
        Print(title),
        ResetColor,
        Print("\n\n")
    )?;

    if lines.is_empty() {
        queue!(stdout, Print("(пусто)\n"))?;
        stdout.flush()?;
        return Ok(());
    }

    if offset > 0 {
        queue!(
            stdout,
            SetForegroundColor(crossterm::style::Color::DarkGrey),
            Print("↑ ещё выше\n"),
            ResetColor
        )?;
    }

    for line in lines.iter().skip(offset).take(end.saturating_sub(offset)) {
        let mut s = line.clone();
        if s.len() > width.saturating_sub(2) {
            s.truncate(width.saturating_sub(5));
            s.push_str("...");
        }
        queue!(stdout, Print(s), Print("\n"))?;
    }

    if end < lines.len() {
        queue!(
            stdout,
            SetForegroundColor(crossterm::style::Color::DarkGrey),
            Print("↓ ещё ниже\n"),
            ResetColor
        )?;
    }

    stdout.flush()?;
    Ok(())
}

fn count_lines(s: &str) -> usize {
    let n = s.lines().count();
    if n == 0 {
        1
    } else {
        n
    }
}

fn drain_events() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

fn next_key_press() -> AppResult<KeyCode> {
    loop {
        let ready = event::poll(Duration::from_millis(250))
            .map_err(|e| AppError::Msg(format!("Ошибка терминала: {e}")))?;

        if !ready {
            continue;
        }

        let ev = event::read().map_err(|e| AppError::Msg(format!("Ошибка терминала: {e}")))?;

        if let Event::Key(KeyEvent { code, kind, .. }) = ev {
            if kind == KeyEventKind::Press {
                return Ok(code);
            }
        }
    }
}
