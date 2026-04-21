use std::{io, io::Write, time::Duration};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};

use crate::{config::Strategy, AppError};

// ─── Типы выбора ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestType {
    Standard,
    Dpi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestMode {
    All,
    Select,
}

// ─── Публичные функции ────────────────────────────────────────────────────────

pub fn ask_test_type() -> Result<TestType, AppError> {
    let items = [
        ("Стандартный (HTTP / TLS / ping)", TestType::Standard),
        ("DPI-чекеры (TCP 16-20 заморозка)", TestType::Dpi),
    ];

    println!();
    let idx = run_arrow_menu(
        "Выберите тип теста:",
        &items.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
        0,
    )?;
    Ok(items[idx].1)
}

pub fn ask_mode() -> Result<TestMode, AppError> {
    let items = [
        ("Все стратегии из strategies.ini", TestMode::All),
        ("Выбрать стратегии вручную", TestMode::Select),
    ];

    println!();
    let idx = run_arrow_menu(
        "Режим запуска:",
        &items.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
        0,
    )?;
    Ok(items[idx].1)
}

pub fn ask_config_selection(strategies: &[Strategy]) -> Result<Vec<Strategy>, AppError> {
    let labels: Vec<String> = strategies
        .iter()
        .map(|s| format!("{} — {}", s.name, s.description))
        .collect();

    let selected_indices = run_checkbox_menu("Выберите стратегии:", &labels)?;

    let result: Vec<Strategy> = selected_indices
        .into_iter()
        .map(|i| strategies[i].clone())
        .collect();

    if result.is_empty() {
        return Err(AppError::msg("Не выбрано ни одной стратегии."));
    }

    Ok(result)
}

// ─── Стрелочное меню ──────────────────────────────────────────────────────────

fn run_arrow_menu(title: &str, items: &[&str], default: usize) -> Result<usize, AppError> {
    let mut selection = default.min(items.len().saturating_sub(1));

    let mut stdout = io::stdout();

    println!("{title}");
    println!();

    // Резервируем строки под пункты + одна пустая строка после
    let rows = items.len();
    for _ in 0..rows {
        println!();
    }
    println!(); // пустая строка после меню

    // Скрываем курсор и включаем raw-режим
    execute!(stdout, cursor::Hide)?;
    let _raw = RawModeGuard::enable()?;
    // Сбрасываем накопившиеся события
    drain_events();

    loop {
        queue!(
            stdout,
            // Поднимаемся на rows + 1 (пустая строка)
            cursor::MoveUp((rows + 1) as u16),
            cursor::MoveToColumn(0),
            Clear(ClearType::FromCursorDown)
        )?;

        for (i, item) in items.iter().enumerate() {
            if i == selection {
                queue!(
                    stdout,
                    SetForegroundColor(Color::Cyan),
                    Print(format!("  ► {item}\n")),
                    ResetColor,
                )?;
            } else {
                queue!(stdout, Print(format!("    {item}\n")))?;
            }
        }

        // Пустая строка после меню
        queue!(stdout, Print("\n"))?;
        stdout.flush()?;

        // Читаем только Press/Repeat, Release игнорируем
        match next_key_press()? {
            KeyCode::Up => {
                selection = selection.saturating_sub(1);
            }
            KeyCode::Down => {
                if selection + 1 < items.len() {
                    selection += 1;
                }
            }
            KeyCode::Enter => break,
            KeyCode::Esc => {
                // Показываем курсор перед выходом
                let _ = execute!(io::stdout(), cursor::Show);
                return Err(AppError::msg("Отменено пользователем."));
            }
            _ => {}
        }
    }

    // Восстанавливаем курсор (RawModeGuard сам отключит raw)
    execute!(stdout, cursor::Show)?;
    println!();
    Ok(selection)
}

// ─── Чекбокс-меню ─────────────────────────────────────────────────────────────

fn run_checkbox_menu(title: &str, items: &[String]) -> Result<Vec<usize>, AppError> {
    let mut cursor_pos: usize = 0;
    let mut checked: Vec<bool> = vec![false; items.len()];

    let visible = 20_usize.min(items.len());
    let mut scroll: usize = 0;

    let mut stdout = io::stdout();

    println!("{title}");
    println!("  ↑↓ — навигация   Пробел — отметить   A — все/сбросить   Enter — начать тест   Esc — отмена");
    println!();

    // Резервируем строки: visible строк + 1 строка индикатора прокрутки
    for _ in 0..(visible + 1) {
        println!();
    }

    execute!(stdout, cursor::Hide)?;
    let _raw = RawModeGuard::enable()?;
    // Сбрасываем накопившиеся события
    drain_events();

    // Первичная отрисовка
    redraw_checkbox(&mut stdout, items, &checked, cursor_pos, scroll, visible)?;

    loop {
        match next_key_press()? {
            KeyCode::Up => {
                cursor_pos = cursor_pos.saturating_sub(1);
                if cursor_pos < scroll {
                    scroll = cursor_pos;
                }
                redraw_checkbox(&mut stdout, items, &checked, cursor_pos, scroll, visible)?;
            }
            KeyCode::Down => {
                if cursor_pos + 1 < items.len() {
                    cursor_pos += 1;
                    if cursor_pos >= scroll + visible {
                        scroll = cursor_pos + 1 - visible;
                    }
                }
                redraw_checkbox(&mut stdout, items, &checked, cursor_pos, scroll, visible)?;
            }
            KeyCode::Char(' ') => {
                checked[cursor_pos] = !checked[cursor_pos];
                redraw_checkbox(&mut stdout, items, &checked, cursor_pos, scroll, visible)?;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                let all_on = checked.iter().all(|&v| v);
                checked.iter_mut().for_each(|v| *v = !all_on);
                redraw_checkbox(&mut stdout, items, &checked, cursor_pos, scroll, visible)?;
            }
            KeyCode::Enter => break,
            KeyCode::Esc => {
                let _ = execute!(io::stdout(), cursor::Show);
                return Err(AppError::msg("Отменено пользователем."));
            }
            _ => {}
        }
    }

    execute!(stdout, cursor::Show)?;
    println!();

    let selected: Vec<usize> = checked
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| if v { Some(i) } else { None })
        .collect();

    Ok(selected)
}

fn redraw_checkbox(
    stdout: &mut impl Write,
    items: &[String],
    checked: &[bool],
    cursor_pos: usize,
    scroll: usize,
    visible: usize,
) -> Result<(), AppError> {
    // visible строк + 1 строка индикатора
    let total_lines = visible + 1;

    queue!(
        stdout,
        cursor::MoveUp(total_lines as u16),
        cursor::MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )?;

    let end = (scroll + visible).min(items.len());

    for i in scroll..end {
        let marker = if checked[i] { "[*]" } else { "[ ]" };
        let arrow = if i == cursor_pos { "►" } else { " " };

        if i == cursor_pos {
            queue!(
                stdout,
                SetForegroundColor(Color::Cyan),
                Print(format!("  {arrow} {marker} {}\n", items[i])),
                ResetColor,
            )?;
        } else {
            queue!(stdout, Print(format!("    {marker} {}\n", items[i])))?;
        }
    }

    // Строка-индикатор прокрутки
    let total = items.len();
    if total > visible {
        queue!(
            stdout,
            SetForegroundColor(Color::DarkGrey),
            Print(format!("  Показано {}-{} из {}\n", scroll + 1, end, total)),
            ResetColor,
        )?;
    } else {
        queue!(stdout, Print("\n"))?;
    }

    stdout.flush()?;
    Ok(())
}

// ─── Вспомогательные функции терминала ───────────────────────────────────────

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self, AppError> {
        crossterm::terminal::enable_raw_mode()
            .map_err(|e| AppError::msg(format!("Не удалось включить raw-режим: {e}")))?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Сбрасываем все накопившиеся события в очереди.
/// Вызывается после каждого меню чтобы Release от Enter не попал в следующее меню.
fn drain_events() {
    while event::poll(Duration::from_millis(0)).unwrap_or(false) {
        let _ = event::read();
    }
}

/// Читаем следующее нажатие клавиши, игнорируя Release и Repeat.
/// Блокирует до появления события или таймаута 250 мс (тогда возвращает Null).
fn next_key_press() -> Result<KeyCode, AppError> {
    loop {
        // Ждём события с таймаутом
        let ready = event::poll(Duration::from_millis(250))
            .map_err(|e| AppError::msg(format!("Ошибка чтения терминала: {e}")))?;

        if !ready {
            continue;
        }

        let ev =
            event::read().map_err(|e| AppError::msg(format!("Ошибка чтения терминала: {e}")))?;

        if let Event::Key(KeyEvent { code, kind, .. }) = ev {
            // Принимаем только Press; Release и Repeat игнорируем
            if kind == KeyEventKind::Press {
                return Ok(code);
            }
        }
    }
}
