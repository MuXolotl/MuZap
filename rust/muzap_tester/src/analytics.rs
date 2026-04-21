use std::collections::HashMap;

use crossterm::style::Color;

use crate::{checker::TargetResult, dpi::DpiTargetResult, RunResult};

// ─── Типы ─────────────────────────────────────────────────────────────────────

/// Аналитика по одной стратегии — стандартный тест
#[derive(Debug, Clone, Default)]
pub struct StandardAnalytics {
    pub ok: u32,
    pub error: u32,
    pub unsup: u32,
    pub ping_ok: u32,
    pub ping_fail: u32,
}

/// Аналитика по одной стратегии — DPI тест
#[derive(Debug, Clone, Default)]
pub struct DpiAnalytics {
    pub ok: u32,
    pub fail: u32,
    pub unsupported: u32,
    pub likely_blocked: u32,
}

#[derive(Debug, Clone)]
pub enum StrategyAnalytics {
    Standard(StandardAnalytics),
    Dpi(DpiAnalytics),
}

impl StrategyAnalytics {
    /// Основной счётчик «хорошего» результата — для поиска лучшей стратегии
    pub fn ok_score(&self) -> u32 {
        match self {
            StrategyAnalytics::Standard(a) => a.ok,
            StrategyAnalytics::Dpi(a) => a.ok,
        }
    }

    /// Вторичный счётчик (ping ok / не заблокировано) — тай-брейкер
    pub fn secondary_score(&self) -> u32 {
        match self {
            StrategyAnalytics::Standard(a) => a.ping_ok,
            StrategyAnalytics::Dpi(a) => {
                // Меньше блокировок — лучше
                a.ok.saturating_sub(a.likely_blocked)
            }
        }
    }
}

/// Карта: имя стратегии → аналитика
pub type Analytics = HashMap<String, StrategyAnalytics>;

// ─── Построение аналитики ─────────────────────────────────────────────────────

pub fn build_analytics(results: &[RunResult]) -> Analytics {
    let mut map: Analytics = HashMap::new();

    for run in results {
        let entry = match (&run.standard, &run.dpi) {
            (Some(std_res), _) => StrategyAnalytics::Standard(build_standard(std_res)),
            (_, Some(dpi_res)) => StrategyAnalytics::Dpi(build_dpi(dpi_res)),
            _ => continue,
        };

        map.insert(run.strategy_name.clone(), entry);
    }

    map
}

fn build_standard(results: &[TargetResult]) -> StandardAnalytics {
    let mut a = StandardAnalytics::default();

    for tr in results {
        if tr.is_url {
            for tok in &tr.http_tokens {
                match tok.status.as_str() {
                    "OK" => a.ok += 1,
                    "UNSUP" => a.unsup += 1,
                    _ => a.error += 1,
                }
            }
        }

        match tr.ping_ms {
            Some(_) => a.ping_ok += 1,
            None => a.ping_fail += 1,
        }
    }

    a
}

fn build_dpi(results: &[DpiTargetResult]) -> DpiAnalytics {
    let mut a = DpiAnalytics::default();

    for dr in results {
        for line in &dr.lines {
            match line.status.as_str() {
                "OK" => a.ok += 1,
                "UNSUPPORTED" => a.unsupported += 1,
                "LIKELY_BLOCKED" => a.likely_blocked += 1,
                _ => a.fail += 1,
            }
        }
    }

    a
}

// ─── Поиск лучшей стратегии ───────────────────────────────────────────────────

pub fn find_best(analytics: &Analytics) -> Option<String> {
    analytics
        .iter()
        .max_by(|(_, a), (_, b)| {
            a.ok_score()
                .cmp(&b.ok_score())
                .then(a.secondary_score().cmp(&b.secondary_score()))
        })
        .map(|(name, _)| name.clone())
}

// ─── Вывод аналитики ──────────────────────────────────────────────────────────

pub fn print_analytics(analytics: &Analytics) {
    crate::print_colored_tag("[АНАЛИТИКА]", Color::Cyan, "");

    // Сортируем по имени для стабильного вывода
    let mut entries: Vec<(&String, &StrategyAnalytics)> = analytics.iter().collect();
    entries.sort_by_key(|(name, _)| name.as_str());

    for (name, a) in &entries {
        match a {
            StrategyAnalytics::Standard(s) => {
                println!(
                    "  {:<32} HTTP OK: {:>3}  ERR: {:>3}  UNSUP: {:>3}  Ping OK: {:>3}  Ping Fail: {:>3}",
                    truncate(name, 32),
                    s.ok,
                    s.error,
                    s.unsup,
                    s.ping_ok,
                    s.ping_fail,
                );
            }
            StrategyAnalytics::Dpi(d) => {
                println!(
                    "  {:<32} OK: {:>3}  FAIL: {:>3}  UNSUP: {:>3}  BLOCKED: {:>3}",
                    truncate(name, 32),
                    d.ok,
                    d.fail,
                    d.unsupported,
                    d.likely_blocked,
                );
            }
        }
    }
}

pub fn print_summary_table(analytics: &Analytics) {
    if analytics.is_empty() {
        return;
    }

    // Определяем тип таблицы по первому элементу
    let is_standard = analytics
        .values()
        .next()
        .map(|a| matches!(a, StrategyAnalytics::Standard(_)))
        .unwrap_or(true);

    let best_score = analytics.values().map(|a| a.ok_score()).max().unwrap_or(0);

    println!();
    crate::print_colored_tag("[ТАБЛИЦА]", Color::Cyan, "");

    let sep = "-".repeat(64);
    println!("{sep}");

    if is_standard {
        println!(
            "{:<32} {:>5} {:>5} {:>7} {:>8} {:>9}",
            "Стратегия", "OK", "ERR", "UNSUP", "PingOK", "PingFail"
        );
    } else {
        println!(
            "{:<32} {:>5} {:>6} {:>7} {:>9}",
            "Стратегия", "OK", "FAIL", "UNSUP", "BLOCKED"
        );
    }
    println!("{sep}");

    let mut entries: Vec<(&String, &StrategyAnalytics)> = analytics.iter().collect();
    entries.sort_by_key(|(name, _)| name.as_str());

    for (name, a) in entries {
        let is_best = best_score > 0 && a.ok_score() == best_score;

        if is_best {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::style::SetForegroundColor(Color::Green)
            );
        }

        match a {
            StrategyAnalytics::Standard(s) => {
                println!(
                    "{:<32} {:>5} {:>5} {:>7} {:>8} {:>9}",
                    truncate(name, 32),
                    s.ok,
                    s.error,
                    s.unsup,
                    s.ping_ok,
                    s.ping_fail,
                );
            }
            StrategyAnalytics::Dpi(d) => {
                println!(
                    "{:<32} {:>5} {:>6} {:>7} {:>9}",
                    truncate(name, 32),
                    d.ok,
                    d.fail,
                    d.unsupported,
                    d.likely_blocked,
                );
            }
        }

        if is_best {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::style::ResetColor);
        }
    }

    println!("{sep}");
}

// ─── Вспомогательные функции ──────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
