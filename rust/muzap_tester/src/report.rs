use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use chrono::Local;

use crate::{
    analytics::{Analytics, StrategyAnalytics},
    checker::TargetResult,
    dpi::DpiTargetResult,
    RunResult,
};

// ─── Сохранение отчёта ────────────────────────────────────────────────────────

pub fn save_report(
    dir: &Path,
    results: &[RunResult],
    analytics: &Analytics,
) -> io::Result<PathBuf> {
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("test_results_{timestamp}.txt");
    let path = dir.join(&filename);

    let mut buf = Vec::<u8>::new();
    write_report(&mut buf, results, analytics)?;

    fs::write(&path, &buf)?;
    Ok(path)
}

// ─── Формирование текста отчёта ───────────────────────────────────────────────

fn write_report(
    w: &mut impl Write,
    results: &[RunResult],
    analytics: &Analytics,
) -> io::Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(w, "MuZap Test Report")?;
    writeln!(w, "Дата: {timestamp}")?;
    writeln!(w, "{}", "=".repeat(64))?;
    writeln!(w)?;

    // ── Результаты по каждой стратегии ────────────────────────────────────
    for run in results {
        writeln!(
            w,
            "Стратегия: {} (тип: {:?})",
            run.strategy_name, run.test_type
        )?;
        writeln!(w, "{}", "-".repeat(64))?;

        if let Some(std_res) = &run.standard {
            write_standard_results(w, std_res)?;
        }

        if let Some(dpi_res) = &run.dpi {
            write_dpi_results(w, dpi_res)?;
        }

        writeln!(w)?;
    }

    // ── Аналитика ─────────────────────────────────────────────────────────
    writeln!(w, "{}", "=".repeat(64))?;
    writeln!(w, "АНАЛИТИКА")?;
    writeln!(w, "{}", "=".repeat(64))?;

    let mut entries: Vec<(&String, &StrategyAnalytics)> = analytics.iter().collect();
    entries.sort_by_key(|(name, _)| name.as_str());

    for (name, a) in &entries {
        match a {
            StrategyAnalytics::Standard(s) => {
                writeln!(
                    w,
                    "{}: HTTP OK={} ERR={} UNSUP={} PingOK={} PingFail={}",
                    name, s.ok, s.error, s.unsup, s.ping_ok, s.ping_fail
                )?;
            }
            StrategyAnalytics::Dpi(d) => {
                writeln!(
                    w,
                    "{}: OK={} FAIL={} UNSUP={} BLOCKED={}",
                    name, d.ok, d.fail, d.unsupported, d.likely_blocked
                )?;
            }
        }
    }

    writeln!(w)?;

    // ── Сводная таблица ───────────────────────────────────────────────────
    writeln!(w, "{}", "=".repeat(64))?;
    writeln!(w, "СВОДНАЯ ТАБЛИЦА")?;
    writeln!(w, "{}", "=".repeat(64))?;

    let is_standard = analytics
        .values()
        .next()
        .map(|a| matches!(a, StrategyAnalytics::Standard(_)))
        .unwrap_or(true);

    if is_standard {
        writeln!(
            w,
            "{:<32} {:>5} {:>5} {:>7} {:>8} {:>9}",
            "Стратегия", "OK", "ERR", "UNSUP", "PingOK", "PingFail"
        )?;
    } else {
        writeln!(
            w,
            "{:<32} {:>5} {:>6} {:>7} {:>9}",
            "Стратегия", "OK", "FAIL", "UNSUP", "BLOCKED"
        )?;
    }
    writeln!(w, "{}", "-".repeat(64))?;

    for (name, a) in &entries {
        match a {
            StrategyAnalytics::Standard(s) => {
                writeln!(
                    w,
                    "{:<32} {:>5} {:>5} {:>7} {:>8} {:>9}",
                    truncate(name, 32),
                    s.ok,
                    s.error,
                    s.unsup,
                    s.ping_ok,
                    s.ping_fail,
                )?;
            }
            StrategyAnalytics::Dpi(d) => {
                writeln!(
                    w,
                    "{:<32} {:>5} {:>6} {:>7} {:>9}",
                    truncate(name, 32),
                    d.ok,
                    d.fail,
                    d.unsupported,
                    d.likely_blocked,
                )?;
            }
        }
    }

    writeln!(w, "{}", "-".repeat(64))?;
    writeln!(w)?;

    // ── Лучшая стратегия ──────────────────────────────────────────────────
    if let Some(best) = crate::analytics::find_best(analytics) {
        writeln!(w, "Лучшая стратегия: {best}")?;
    }

    Ok(())
}

// ─── Запись стандартных результатов ──────────────────────────────────────────

fn write_standard_results(w: &mut impl Write, results: &[TargetResult]) -> io::Result<()> {
    for tr in results {
        write!(w, "  {:<36}", tr.name)?;

        if tr.is_url {
            for tok in &tr.http_tokens {
                write!(w, " {}:{}", tok.label, tok.status)?;
            }
            write!(w, " |")?;
        }

        let ping_str = tr
            .ping_ms
            .map(|ms| format!("{ms} мс"))
            .unwrap_or_else(|| "Таймаут".to_string());

        writeln!(w, " Ping: {ping_str}")?;
    }
    Ok(())
}

// ─── Запись DPI результатов ───────────────────────────────────────────────────

fn write_dpi_results(w: &mut impl Write, results: &[DpiTargetResult]) -> io::Result<()> {
    for dr in results {
        writeln!(w, "  [{}][{}] {}", dr.country, dr.provider, dr.target_id)?;

        for line in &dr.lines {
            writeln!(
                w,
                "    {}: code={} up={:.1}KB down={:.1}KB time={:.2}s status={}",
                line.test_label, line.code, line.up_kb, line.down_kb, line.time_secs, line.status,
            )?;
        }
    }
    Ok(())
}

// ─── Вспомогательные функции ──────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
