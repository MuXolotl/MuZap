use std::{fs, path::Path};

use crate::{CoreError, CoreResult};

pub fn detect_eol(s: &str) -> &'static str {
    if s.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Пишем (section,key) = value в ini-файл:
/// - если секции нет — добавляем в конец
/// - если ключа нет внутри секции — добавляем перед следующей секцией/концом
/// - сохраняем EOL как в исходнике (CRLF/LF), иначе используем CRLF по умолчанию
pub fn write_ini_value(path: &Path, section: &str, key: &str, value: &str) -> CoreResult<()> {
    if section.trim().is_empty() {
        return Err(CoreError::arg("Секция INI пустая"));
    }
    if key.trim().is_empty() {
        return Err(CoreError::arg("Ключ INI пустой"));
    }

    let (raw, eol) = if path.exists() {
        let raw = fs::read_to_string(path)?;
        let eol = detect_eol(&raw);
        (raw, eol)
    } else {
        (String::new(), "\r\n")
    };

    let updated = write_ini_value_to_string(&raw, section, key, value, eol);
    fs::write(path, updated)?;
    Ok(())
}

pub fn read_ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut cur_section = String::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            cur_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }

        if cur_section.eq_ignore_ascii_case(section.trim()) {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key.trim()) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }

    None
}

pub fn write_ini_value_to_string(
    text: &str,
    section: &str,
    key: &str,
    value: &str,
    eol: &str,
) -> String {
    let mut out: Vec<String> = Vec::new();

    let mut in_section = false;
    let mut section_found = false;
    let mut key_set = false;

    for line in text.lines() {
        let t = line.trim();

        if t.starts_with('[') && t.ends_with(']') {
            // Закрываем секцию: если это была целевая секция и ключ не найден — добавляем ключ.
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

    // Если секция была последней в файле и ключ не встретился
    if section_found && !key_set {
        out.push(format!("{key}={value}"));
    }

    // Если секция не найдена — добавляем в конец
    if !section_found {
        if !out.is_empty() && !out.last().unwrap().trim().is_empty() {
            out.push(String::new());
        }
        out.push(format!("[{section}]"));
        out.push(format!("{key}={value}"));
    }

    let mut final_text = out.join(eol);
    if !final_text.ends_with(eol) {
        final_text.push_str(eol);
    }
    final_text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_read_value_basic() {
        let s = r#"
[App]
Version=1.2.3

[Features]
TelemetryEnabled=true
"#;
        assert_eq!(
            read_ini_value(s, "App", "Version").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            read_ini_value(s, "Features", "TelemetryEnabled").as_deref(),
            Some("true")
        );
        assert_eq!(read_ini_value(s, "Nope", "X"), None);
    }

    #[test]
    fn ini_write_add_section_and_key() {
        let src = "";
        let out = write_ini_value_to_string(src, "App", "Version", "9.9.9", "\r\n");
        assert!(out.contains("[App]\r\nVersion=9.9.9\r\n"));
    }

    #[test]
    fn ini_write_replace_existing_key() {
        let src = "[App]\nVersion=1.0.0\n[Features]\nTelemetryEnabled=false\n";
        let out = write_ini_value_to_string(src, "App", "Version", "2.0.0", "\n");
        assert!(out.contains("[App]\nVersion=2.0.0\n"));
        assert!(out.contains("[Features]\nTelemetryEnabled=false\n"));
    }

    #[test]
    fn ini_write_append_key_if_missing_in_section() {
        let src = "[App]\n; comment\n\n[Features]\nTelemetryEnabled=false\n";
        let out = write_ini_value_to_string(src, "App", "Version", "3.3.3", "\n");
        // ключ должен появиться внутри App секции (до следующей секции)
        assert!(out.contains("[App]\n; comment\n\nVersion=3.3.3\n[Features]\n"));
    }
}
