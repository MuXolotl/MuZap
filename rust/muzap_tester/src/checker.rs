use std::time::Duration;

use reqwest::{redirect::Policy, Client};

use crate::config::Target;

// ─── Типы ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HttpToken {
    pub label: String,
    pub status: String,
}

impl HttpToken {
    pub fn display(&self) -> String {
        format!("{}:{}", self.label, self.status)
    }
}

#[derive(Debug, Clone)]
pub struct TargetResult {
    pub name: String,
    pub http_tokens: Vec<HttpToken>,
    pub ping_ms: Option<u64>,
    pub is_url: bool,
}

// ─── Главная функция ──────────────────────────────────────────────────────────

pub async fn run_standard_tests(targets: &[Target]) -> Vec<TargetResult> {
    let mut handles = Vec::with_capacity(targets.len());

    for target in targets {
        // Сохраняем имя ДО перемещения target в замыкание
        let name = target.name.clone();
        let target = target.clone();
        let handle = tokio::spawn(async move { test_one_target(&target).await });
        handles.push((name, handle));
    }

    let mut results: Vec<TargetResult> = Vec::with_capacity(handles.len());

    for (name, handle) in handles {
        match handle.await {
            Ok(r) => results.push(r),
            Err(_) => {
                results.push(TargetResult {
                    name,
                    http_tokens: vec![
                        HttpToken {
                            label: "HTTP".into(),
                            status: "ERROR".into(),
                        },
                        HttpToken {
                            label: "TLS1.2".into(),
                            status: "ERROR".into(),
                        },
                        HttpToken {
                            label: "TLS1.3".into(),
                            status: "ERROR".into(),
                        },
                    ],
                    ping_ms: None,
                    is_url: true,
                });
            }
        }
    }

    results
}

// ─── Тест одной цели ─────────────────────────────────────────────────────────

async fn test_one_target(target: &Target) -> TargetResult {
    let http_tokens = if let Some(url) = &target.url {
        test_http_all(url).await
    } else {
        vec![]
    };

    let ping_ms = ping_host(&target.ping_target).await;

    TargetResult {
        name: target.name.clone(),
        http_tokens,
        ping_ms,
        is_url: target.url.is_some(),
    }
}

// ─── HTTP / TLS тесты ────────────────────────────────────────────────────────

async fn test_http_all(url: &str) -> Vec<HttpToken> {
    vec![
        test_single_http(url, "HTTP", TlsVariant::Any).await,
        test_single_http(url, "TLS1.2", TlsVariant::V12Only).await,
        test_single_http(url, "TLS1.3", TlsVariant::V13Only).await,
    ]
}

#[derive(Clone, Copy)]
enum TlsVariant {
    Any,
    V12Only,
    V13Only,
}

async fn test_single_http(url: &str, label: &str, variant: TlsVariant) -> HttpToken {
    let timeout = Duration::from_secs(5);

    let client_result = match variant {
        TlsVariant::Any => Client::builder()
            .timeout(timeout)
            .redirect(Policy::limited(4))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build(),

        TlsVariant::V12Only => Client::builder()
            .timeout(timeout)
            .redirect(Policy::limited(4))
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .max_tls_version(reqwest::tls::Version::TLS_1_2)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build(),

        TlsVariant::V13Only => Client::builder()
            .timeout(timeout)
            .redirect(Policy::limited(4))
            .min_tls_version(reqwest::tls::Version::TLS_1_3)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build(),
    };

    let client = match client_result {
        Ok(c) => c,
        Err(_) => {
            return HttpToken {
                label: label.to_string(),
                status: "ERROR".into(),
            };
        }
    };

    match client.head(url).send().await {
        Ok(_) => HttpToken {
            label: label.to_string(),
            status: "OK".into(),
        },
        Err(e) => HttpToken {
            label: label.to_string(),
            status: classify_reqwest_error(&e),
        },
    }
}

fn classify_reqwest_error(e: &reqwest::Error) -> String {
    let msg = e.to_string().to_ascii_lowercase();

    if msg.contains("certificate")
        || msg.contains("ssl")
        || msg.contains("self-signed")
        || msg.contains("unknown cert")
    {
        return "SSL".into();
    }

    if msg.contains("no supported versions")
        || msg.contains("protocol version")
        || msg.contains("alert handshake failure")
        || msg.contains("handshake")
        || msg.contains("unsupported protocol")
    {
        return "UNSUP".into();
    }

    "ERROR".into()
}

// ─── Ping ─────────────────────────────────────────────────────────────────────

async fn ping_host(host: &str) -> Option<u64> {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return tcp_ping(host, 443).await;
    }

    let url = format!("https://{host}");
    let client = build_client(5);

    let mut times: Vec<u64> = Vec::new();
    for _ in 0..3 {
        let start = std::time::Instant::now();
        if client.head(&url).send().await.is_ok() {
            times.push(start.elapsed().as_millis() as u64);
        }
    }

    times.into_iter().min()
}

async fn tcp_ping(ip: &str, port: u16) -> Option<u64> {
    let addr = format!("{ip}:{port}");
    let start = std::time::Instant::now();

    match tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => Some(start.elapsed().as_millis() as u64),
        _ => None,
    }
}

// ─── Вспомогательные функции ──────────────────────────────────────────────────

fn build_client(timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(Policy::limited(4))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .expect("Не удалось создать HTTP-клиент")
}
