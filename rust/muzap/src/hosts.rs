use encoding_rs::{UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1251};
use std::{fs, path::Path};

use crate::{fsutil, AppError, AppResult};

#[derive(Debug, Clone, Copy)]
enum TextEncoding {
    Utf8Bom,
    Utf8NoBom,
    Utf16Le,
    Utf16Be,
    Windows1251,
}

#[derive(Debug)]
struct TextFile {
    text: String,
    encoding: TextEncoding,
    eol: String,
}

pub fn update_hosts_managed_block(
    hosts_path: &Path,
    source_text: &str,
    marker_name: &str,
) -> AppResult<()> {
    if !hosts_path.exists() {
        return Err(AppError::Msg(format!(
            "hosts файл не найден: {}",
            hosts_path.display()
        )));
    }

    let hosts = read_text_file(hosts_path)?;

    let updated = update_hosts_block(&hosts.text, source_text, marker_name, &hosts.eol);
    let updated_norm = normalize_eol(&updated, &hosts.eol);
    let bytes = encode_text(&updated_norm, hosts.encoding);

    fsutil::atomic_write_replace_bytes(hosts_path, &bytes)?;
    Ok(())
}

pub fn remove_hosts_managed_block(hosts_path: &Path, marker_name: &str) -> AppResult<bool> {
    if !hosts_path.exists() {
        return Err(AppError::Msg(format!(
            "hosts файл не найден: {}",
            hosts_path.display()
        )));
    }

    let hosts = read_text_file(hosts_path)?;
    let (cleaned, removed) = remove_hosts_block(&hosts.text, marker_name, &hosts.eol);

    let cleaned_norm = normalize_eol(&cleaned, &hosts.eol);
    let bytes = encode_text(&cleaned_norm, hosts.encoding);

    fsutil::atomic_write_replace_bytes(hosts_path, &bytes)?;
    Ok(removed)
}

/* ========================= text i/o ========================= */

fn detect_eol(s: &str) -> String {
    if s.contains("\r\n") {
        "\r\n".to_string()
    } else {
        "\n".to_string()
    }
}

fn read_text_file(path: &Path) -> AppResult<TextFile> {
    let bytes = fs::read(path)?;
    let (encoding, text) = decode_bytes(&bytes);
    let eol = detect_eol(&text);
    Ok(TextFile {
        text,
        encoding,
        eol,
    })
}

fn normalize_eol(s: &str, eol: &str) -> String {
    let tmp = s.replace("\r\n", "\n").replace('\r', "\n");
    if eol == "\n" {
        tmp
    } else {
        tmp.replace('\n', eol)
    }
}

fn decode_bytes(bytes: &[u8]) -> (TextEncoding, String) {
    // UTF-8 BOM
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        let (cow, _, _) = UTF_8.decode(&bytes[3..]);
        return (TextEncoding::Utf8Bom, cow.into_owned());
    }

    // UTF-16 LE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let (cow, _, _) = UTF_16LE.decode(&bytes[2..]);
        return (TextEncoding::Utf16Le, cow.into_owned());
    }

    // UTF-16 BE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let (cow, _, _) = UTF_16BE.decode(&bytes[2..]);
        return (TextEncoding::Utf16Be, cow.into_owned());
    }

    // Без BOM: пробуем UTF-8, иначе Windows-1251 (эвристика для RU Windows)
    match std::str::from_utf8(bytes) {
        Ok(s) => (TextEncoding::Utf8NoBom, s.to_string()),
        Err(_) => {
            let (cow, _, _) = WINDOWS_1251.decode(bytes);
            (TextEncoding::Windows1251, cow.into_owned())
        }
    }
}

fn encode_text(text: &str, enc: TextEncoding) -> Vec<u8> {
    match enc {
        TextEncoding::Utf8Bom => {
            let mut out = vec![0xEF, 0xBB, 0xBF];
            out.extend_from_slice(text.as_bytes());
            out
        }
        TextEncoding::Utf8NoBom => text.as_bytes().to_vec(),
        TextEncoding::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            let (cow, _, _) = UTF_16LE.encode(text);
            out.extend_from_slice(&cow);
            out
        }
        TextEncoding::Utf16Be => {
            let mut out = vec![0xFE, 0xFF];
            let (cow, _, _) = UTF_16BE.encode(text);
            out.extend_from_slice(&cow);
            out
        }
        TextEncoding::Windows1251 => {
            let (cow, _, _) = WINDOWS_1251.encode(text);
            cow.into_owned()
        }
    }
}

/* ========================= block editing ========================= */

fn begin_marker(marker: &str) -> String {
    format!("# --- {marker} BEGIN ---")
}

fn end_marker(marker: &str) -> String {
    format!("# --- {marker} END ---")
}

fn remove_hosts_block(hosts_text: &str, marker: &str, eol: &str) -> (String, bool) {
    let begin = begin_marker(marker);
    let end = end_marker(marker);

    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut removed = false;

    for line in hosts_text.lines() {
        let t = line.trim();
        if t == begin {
            in_block = true;
            removed = true;
            continue;
        }
        if in_block {
            if t == end {
                in_block = false;
            }
            continue;
        }
        out.push(line.to_string());
    }

    let mut result = out.join(eol);
    if !result.is_empty() && !result.ends_with(eol) {
        result.push_str(eol);
    }

    (result, removed)
}

fn normalize_source_lines(source_text: &str) -> Vec<String> {
    let mut out = Vec::new();

    for raw in source_text.lines() {
        let line = raw.trim_end();

        if line.is_empty() {
            out.push(String::new());
            continue;
        }

        // Комментарии из источника пропускаем
        let ts = line.trim_start();
        if ts.starts_with('#') || ts.starts_with(';') {
            continue;
        }

        out.push(line.to_string());
    }

    out
}

fn update_hosts_block(hosts_text: &str, source_text: &str, marker: &str, eol: &str) -> String {
    // Сначала удаляем существующий блок (если есть)
    let (cleaned, _removed) = remove_hosts_block(hosts_text, marker, eol);

    let mut lines: Vec<String> = cleaned.lines().map(|l| l.to_string()).collect();

    // Если файл не пуст и последняя строка не пустая — добавим пустую строку
    if !lines.is_empty() {
        if let Some(last) = lines.last() {
            if !last.trim().is_empty() {
                lines.push(String::new());
            }
        }
    }

    lines.push(begin_marker(marker));
    lines.extend(normalize_source_lines(source_text));
    lines.push(end_marker(marker));

    let mut result = lines.join(eol);
    if !result.ends_with(eol) {
        result.push_str(eol);
    }
    result
}
