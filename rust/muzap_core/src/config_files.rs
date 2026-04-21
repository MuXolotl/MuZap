use std::{fs, path::Path};

use crate::{CoreError, CoreResult};

/// Одна стратегия из strategies.ini
#[derive(Debug, Clone)]
pub struct Strategy {
    pub name: String,
    pub description: String,
    pub params: String,
}

/// Одна цель из targets.txt (URL либо PING-only)
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    /// None для PING-only целей
    pub url: Option<String>,
    /// Хост или IP для “пинга”
    pub ping_target: String,
}

/// Парсим strategies.ini (формат секций как у тебя).
///
/// Правила:
/// - секция = `[NAME]`
/// - интересуют только `Description=...` и `Params=...`
/// - комментарии `;` и `#` игнорируем
/// - стратегия считается валидной только если `Params` не пустой
pub fn load_strategies(path: &Path) -> CoreResult<Vec<Strategy>> {
    let text = fs::read_to_string(path)
        .map_err(|e| CoreError::msg(format!("Не удалось прочитать strategies.ini: {e}")))?;

    let mut strategies: Vec<Strategy> = Vec::new();
    let mut current: Option<Strategy> = None;

    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            // Сохраняем предыдущую секцию
            if let Some(s) = current.take() {
                if !s.params.is_empty() {
                    strategies.push(s);
                }
            }

            let name = line[1..line.len() - 1].to_string();
            current = Some(Strategy {
                name,
                description: String::new(),
                params: String::new(),
            });
            continue;
        }

        if let Some(ref mut s) = current {
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim().to_ascii_lowercase();
                let v = v.trim();

                match k.as_str() {
                    "description" => s.description = v.to_string(),
                    "params" => s.params = v.to_string(),
                    _ => {}
                }
            }
        }
    }

    // Последняя секция
    if let Some(s) = current {
        if !s.params.is_empty() {
            strategies.push(s);
        }
    }

    if strategies.is_empty() {
        return Err(CoreError::msg(
            "В strategies.ini не найдено ни одной стратегии с полем Params.",
        ));
    }

    Ok(strategies)
}

/// Парсим targets.txt.
///
/// Формат:
///   Key = "https://host/..."   -> HTTP + ping
///   Key = "PING:1.2.3.4"      -> ping-only
///
/// Если файл не найден/пуст — возвращаем встроенный набор целей.
pub fn load_targets(path: &Path) -> Vec<Target> {
    let mut targets: Vec<Target> = Vec::new();

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return default_targets(),
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Формат: KeyName = "https://..." или KeyName = "PING:1.2.3.4"
        if let Some((name, val)) = line.split_once('=') {
            let name = name.trim().to_string();
            let val = val.trim().trim_matches('"').to_string();

            let target = if let Some(addr) = val.strip_prefix("PING:") {
                Target {
                    name,
                    url: None,
                    ping_target: addr.trim().to_string(),
                }
            } else {
                let host = val
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                Target {
                    name,
                    url: Some(val),
                    ping_target: host,
                }
            };

            targets.push(target);
        }
    }

    if targets.is_empty() {
        default_targets()
    } else {
        targets
    }
}

fn default_targets() -> Vec<Target> {
    fn url(name: &str, url: &str) -> Target {
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        Target {
            name: name.to_string(),
            url: Some(url.to_string()),
            ping_target: host,
        }
    }

    fn ping(name: &str, ip: &str) -> Target {
        Target {
            name: name.to_string(),
            url: None,
            ping_target: ip.to_string(),
        }
    }

    vec![
        url("Discord Main", "https://discord.com"),
        url("Discord Gateway", "https://gateway.discord.gg"),
        url("Discord CDN", "https://cdn.discordapp.com"),
        url("Discord Updates", "https://updates.discord.com"),
        url("YouTube Web", "https://www.youtube.com"),
        url("YouTube Short", "https://youtu.be"),
        url("YouTube Image", "https://i.ytimg.com"),
        url(
            "YouTube Video Redirect",
            "https://redirector.googlevideo.com",
        ),
        url("Google Main", "https://www.google.com"),
        url("Google Gstatic", "https://www.gstatic.com"),
        url("Cloudflare Web", "https://www.cloudflare.com"),
        url("Cloudflare CDN", "https://cdnjs.cloudflare.com"),
        url("Telegram Main", "https://telegram.org"),
        url("Telegram Short", "https://t.me"),
        url("Telegram Web", "https://web.telegram.org"),
        ping("Cloudflare DNS 1.1.1.1", "1.1.1.1"),
        ping("Cloudflare DNS 1.0.0.1", "1.0.0.1"),
        ping("Google DNS 8.8.8.8", "8.8.8.8"),
        ping("Google DNS 8.8.4.4", "8.8.4.4"),
        ping("Quad9 DNS 9.9.9.9", "9.9.9.9"),
    ]
}
