use std::collections::HashMap;

use reqwest::Client;
use serde::Serialize;

use crate::analytics::{Analytics, StrategyAnalytics};

// ─── Константы ────────────────────────────────────────────────────────────────

const TELEMETRY_URL: &str =
    "https://script.google.com/macros/s/AKfycbzfFdg38vAx6T3kR1_ynZ4io7NpDC2t-hXo0cVR_LCYY9jkOC9sQGw4l2XHJDHioQm0/exec";

const TELEMETRY_TOKEN: &str = "mzTelemetry_k9x2p7";

// ─── Структуры данных ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct TelemetryPayload {
    token: String,
    version: String,
    country: String,
    region: String,
    isp: String,
    results: HashMap<String, StrategyTelemetry>,
}

#[derive(Debug, Serialize)]
struct StrategyTelemetry {
    ok: u32,
    err: u32,
    unsup: u32,
    #[serde(rename = "pingOk")]
    ping_ok: u32,
    #[serde(rename = "pingFail")]
    ping_fail: u32,
}

#[derive(Debug, serde::Deserialize)]
struct GeoResponse {
    status: Option<String>,
    isp: Option<String>,
    #[serde(rename = "regionName")]
    region_name: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
}

// ─── Отправка телеметрии ──────────────────────────────────────────────────────

pub async fn send_telemetry(analytics: &Analytics, version: &str) {
    println!();
    crate::print_colored_tag(
        "[Телеметрия]",
        crossterm::style::Color::DarkGrey,
        "Собираю гео-данные...",
    );

    // Собираем только стандартные результаты (не DPI)
    let mut results: HashMap<String, StrategyTelemetry> = HashMap::new();

    for (name, a) in analytics {
        if let StrategyAnalytics::Standard(s) = a {
            results.insert(
                name.clone(),
                StrategyTelemetry {
                    ok: s.ok,
                    err: s.error,
                    unsup: s.unsup,
                    ping_ok: s.ping_ok,
                    ping_fail: s.ping_fail,
                },
            );
        }
    }

    if results.is_empty() {
        crate::print_colored_tag(
            "[Телеметрия]",
            crossterm::style::Color::DarkGrey,
            "Нет стандартных результатов для отправки. Пропускаю.",
        );
        return;
    }

    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("MuZap-Tester/Telemetry")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            crate::print_colored_tag(
                "[Телеметрия]",
                crossterm::style::Color::Yellow,
                &format!("Не удалось создать HTTP-клиент: {e}"),
            );
            return;
        }
    };

    // Гео-данные (без хранения IP)
    let (isp, region, country) = fetch_geo(&client).await;

    crate::print_colored_tag(
        "[Телеметрия]",
        crossterm::style::Color::DarkGrey,
        &format!(
            "Отправляю результаты ({} стратегий, ISP: {isp}, {region}, {country})...",
            results.len()
        ),
    );

    let payload = TelemetryPayload {
        token: TELEMETRY_TOKEN.to_string(),
        version: version.to_string(),
        country,
        region,
        isp,
        results,
    };

    match client.post(TELEMETRY_URL).json(&payload).send().await {
        Ok(resp) => {
            // Сервер возвращает JSON { status: 200, message: "..." }
            match resp.json::<serde_json::Value>().await {
                Ok(json) => {
                    let status = json.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
                    if status == 200 {
                        crate::print_colored_tag(
                            "[Телеметрия]",
                            crossterm::style::Color::Green,
                            "Отправлено успешно. Спасибо!",
                        );
                    } else {
                        let msg = json
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("нет сообщения");
                        crate::print_colored_tag(
                            "[Телеметрия]",
                            crossterm::style::Color::Yellow,
                            &format!("Сервер ответил: {status} — {msg}"),
                        );
                    }
                }
                Err(_) => {
                    crate::print_colored_tag(
                        "[Телеметрия]",
                        crossterm::style::Color::Yellow,
                        "Сервер вернул непустой, но нечитаемый ответ.",
                    );
                }
            }
        }
        Err(e) => {
            crate::print_colored_tag(
                "[Телеметрия]",
                crossterm::style::Color::Yellow,
                &format!("Не удалось отправить: {e}"),
            );
        }
    }
}

// ─── Гео-данные ───────────────────────────────────────────────────────────────

/// Запрашиваем ISP / регион / страну без сохранения IP-адреса.
async fn fetch_geo(client: &Client) -> (String, String, String) {
    let unknown = || "Unknown".to_string();

    let url = "http://ip-api.com/json/?fields=status,isp,regionName,countryCode";

    match client.get(url).send().await {
        Ok(resp) => match resp.json::<GeoResponse>().await {
            Ok(geo) if geo.status.as_deref() == Some("success") => {
                let isp = geo.isp.unwrap_or_else(unknown);
                let region = geo.region_name.unwrap_or_else(unknown);
                let country = geo.country_code.unwrap_or_else(unknown);
                (isp, region, country)
            }
            _ => {
                crate::print_colored_tag(
                    "[Телеметрия]",
                    crossterm::style::Color::DarkGrey,
                    "Гео-запрос не удался, отправляю без местоположения.",
                );
                (unknown(), unknown(), unknown())
            }
        },
        Err(e) => {
            crate::print_colored_tag(
                "[Телеметрия]",
                crossterm::style::Color::DarkGrey,
                &format!("Гео-запрос не удался: {e}"),
            );
            (unknown(), unknown(), unknown())
        }
    }
}
