use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::AppError;

// ─── Типы ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct DpiSuiteEntry {
    pub id: String,
    pub provider: String,
    pub country: String,
    pub host: String,
}

#[derive(Debug, Clone)]
pub struct DpiTestLine {
    pub test_label: String,
    pub code: String,
    pub up_kb: f64,
    pub down_kb: f64,
    pub time_secs: f64,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct DpiTargetResult {
    pub target_id: String,
    pub provider: String,
    pub country: String,
    pub lines: Vec<DpiTestLine>,
    pub has_warning: bool,
}

// ─── Константы ────────────────────────────────────────────────────────────────

const DPI_SUITE_URL: &str = "https://hyperion-cs.github.io/dpi-checkers/ru/tcp-16-20/suite.v2.json";

const DPI_RANGE_BYTES: usize = 65_536;
const DPI_TIMEOUT_SECS: u64 = 10;
const DPI_MAX_PARALLEL: usize = 8;

// ─── Загрузка сюиты ───────────────────────────────────────────────────────────

pub async fn fetch_dpi_suite() -> Result<Vec<DpiSuiteEntry>, AppError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("MuZap-Tester")
        .build()
        .map_err(|e| AppError::msg(format!("Не удалось создать HTTP-клиент: {e}")))?;

    let entries: Vec<DpiSuiteEntry> = client
        .get(DPI_SUITE_URL)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("Ошибка загрузки DPI-сюиты: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::msg(format!("Ошибка парсинга DPI-сюиты: {e}")))?;

    Ok(entries)
}

// ─── Главная функция ──────────────────────────────────────────────────────────

pub async fn run_dpi_tests(targets: &[DpiSuiteEntry]) -> Vec<DpiTargetResult> {
    if targets.is_empty() {
        return vec![];
    }

    crate::print_colored_tag(
        "[ІНФО]",
        crossterm::style::Color::Cyan,
        &format!(
            "DPI: {} хостов, диапазон {} байт, таймаут {}с, параллельно {}",
            targets.len(),
            DPI_RANGE_BYTES,
            DPI_TIMEOUT_SECS,
            DPI_MAX_PARALLEL,
        ),
    );

    let mut all_results: Vec<DpiTargetResult> = Vec::with_capacity(targets.len());

    for chunk in targets.chunks(DPI_MAX_PARALLEL) {
        let mut handles = Vec::with_capacity(chunk.len());

        for entry in chunk {
            let entry = entry.clone();
            let handle = tokio::spawn(async move { test_dpi_target(entry).await });
            handles.push(handle);
        }

        for handle in handles {
            match handle.await {
                Ok(r) => all_results.push(r),
                Err(e) => {
                    crate::print_colored_tag(
                        "[ПРЕДУПРЕЖДЕНИЕ]",
                        crossterm::style::Color::Yellow,
                        &format!("DPI-задача завершилась с паникой: {e}"),
                    );
                }
            }
        }
    }

    all_results
}

// ─── Тест одного DPI-хоста ───────────────────────────────────────────────────

async fn test_dpi_target(entry: DpiSuiteEntry) -> DpiTargetResult {
    let payload = random_bytes(DPI_RANGE_BYTES);

    let tests: &[(
        &str,
        Option<reqwest::tls::Version>,
        Option<reqwest::tls::Version>,
    )] = &[
        ("HTTP", None, None),
        (
            "TLS1.2",
            Some(reqwest::tls::Version::TLS_1_2),
            Some(reqwest::tls::Version::TLS_1_2),
        ),
        ("TLS1.3", Some(reqwest::tls::Version::TLS_1_3), None),
    ];

    let mut lines: Vec<DpiTestLine> = Vec::with_capacity(3);
    let mut has_warning = false;

    for (label, min_tls, max_tls) in tests {
        let line = run_dpi_probe(&entry.host, label, *min_tls, *max_tls, &payload).await;
        if line.status == "LIKELY_BLOCKED" {
            has_warning = true;
        }
        lines.push(line);
    }

    DpiTargetResult {
        target_id: entry.id,
        provider: entry.provider,
        country: entry.country,
        lines,
        has_warning,
    }
}

// ─── Один DPI-зонд ───────────────────────────────────────────────────────────

async fn run_dpi_probe(
    host: &str,
    label: &str,
    min_tls: Option<reqwest::tls::Version>,
    max_tls: Option<reqwest::tls::Version>,
    payload: &[u8],
) -> DpiTestLine {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(DPI_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("MuZap-Tester/DPI");

    if let Some(min) = min_tls {
        builder = builder.min_tls_version(min);
    }
    if let Some(max) = max_tls {
        builder = builder.max_tls_version(max);
    }

    let client = match builder.build() {
        Ok(c) => c,
        Err(_) => {
            return DpiTestLine {
                test_label: label.to_string(),
                code: "ERR".into(),
                up_kb: 0.0,
                down_kb: 0.0,
                time_secs: -1.0,
                status: "FAIL".into(),
            };
        }
    };

    let url = format!("https://{host}");
    let start = std::time::Instant::now();
    let up_bytes = payload.len() as f64;

    let result = client
        .post(&url)
        .header("Range", format!("bytes=0-{}", DPI_RANGE_BYTES - 1))
        .body(payload.to_vec())
        .send()
        .await;

    let elapsed = start.elapsed().as_secs_f64();

    match result {
        Ok(resp) => {
            let code = resp.status().as_u16().to_string();
            let body_len = match resp.bytes().await {
                Ok(b) => b.len() as f64,
                Err(_) => 0.0,
            };

            let up_kb = up_bytes / 1024.0;
            let down_kb = body_len / 1024.0;

            // Паттерн «16-20 КБ заморозки»
            let status = if up_bytes > 0.0 && body_len == 0.0 && elapsed >= DPI_TIMEOUT_SECS as f64
            {
                "LIKELY_BLOCKED".into()
            } else {
                "OK".into()
            };

            DpiTestLine {
                test_label: label.to_string(),
                code,
                up_kb,
                down_kb,
                time_secs: elapsed,
                status,
            }
        }
        Err(e) => {
            let msg = e.to_string().to_ascii_lowercase();

            let status = if msg.contains("no supported versions")
                || msg.contains("protocol version")
                || msg.contains("handshake")
                || msg.contains("unsupported protocol")
            {
                "UNSUPPORTED"
            } else if msg.contains("timed out") || msg.contains("timeout") {
                "LIKELY_BLOCKED"
            } else {
                "FAIL"
            };

            let reported_up_kb = if status == "LIKELY_BLOCKED" {
                up_bytes / 1024.0
            } else {
                0.0
            };

            DpiTestLine {
                test_label: label.to_string(),
                code: "ERR".into(),
                up_kb: reported_up_kb,
                down_kb: 0.0,
                time_secs: elapsed,
                status: status.to_string(),
            }
        }
    }
}

// ─── Вспомогательные функции ──────────────────────────────────────────────────

fn random_bytes(n: usize) -> Vec<u8> {
    let mut state: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}
