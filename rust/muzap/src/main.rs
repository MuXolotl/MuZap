mod diagnostics;
mod fixes;
mod fsutil;
mod hosts;
mod ipset;
mod service;
mod settings;
mod strategies;
mod tools;
mod tui;
mod updates;

use clap::{Parser, Subcommand};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),

    #[error("Ошибка muzap_core: {0}")]
    Core(#[from] muzap_core::CoreError),

    #[error("Ошибка ввода/вывода: {0}")]
    Io(#[from] std::io::Error),

    #[error("Ошибка сети: {0}")]
    Net(#[from] reqwest::Error),

    #[cfg(windows)]
    #[error("Ошибка windows_service: {0}")]
    WinService(#[from] windows_service::Error),
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Parser)]
#[command(
    name = "MuZap",
    about = "MuZap (Rust): менеджер службы, стратегий, настроек, обновлений и диагностики",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Strategies {
        #[command(subcommand)]
        cmd: StrategiesCmd,
    },
    Service {
        #[command(subcommand)]
        cmd: ServiceCmd,
    },
    Settings {
        #[command(subcommand)]
        cmd: SettingsCmd,
    },
    Updates {
        #[command(subcommand)]
        cmd: UpdatesCmd,
    },
    Tools {
        #[command(subcommand)]
        cmd: ToolsCmd,
    },
}

#[derive(Debug, Subcommand)]
enum StrategiesCmd {
    List,
}

#[derive(Debug, Subcommand)]
enum ServiceCmd {
    Install {
        #[arg(long)]
        strategy: Option<String>,
        #[arg(long)]
        no_start: bool,
        #[arg(long)]
        force: bool,
    },
    SetStrategy {
        #[arg(long)]
        strategy: String,
        #[arg(long)]
        restart: bool,
    },
    Remove {
        #[arg(long)]
        no_stop: bool,
    },
    Restart,
    Status,
}

#[derive(Debug, Subcommand)]
enum SettingsCmd {
    Show,

    SetGameFilterMode {
        #[arg(long)]
        value: String,
        #[arg(long)]
        restart: bool,
    },

    SetTelemetry {
        #[arg(long)]
        value: String,
        #[arg(long)]
        restart: bool,
    },

    SetIpSetMode {
        /// none | any | loaded
        #[arg(long)]
        value: String,

        /// Перезапустить службу после изменения
        #[arg(long)]
        restart: bool,
    },
}

#[derive(Debug, Subcommand)]
enum UpdatesCmd {
    IpSetUpdate,
    HostsUpdate,
    HostsRemove,
    CheckRelease,
    InstallRelease,
}

#[derive(Debug, Subcommand)]
enum ToolsCmd {
    Diagnostics,
    FixAll,
    FixConflicts,
    FixWinDivert,
    FixTcpTimestamps,
    FixDiscordCache,
    RunTester,
    ViewServiceLog,
}

fn main() {
    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();

    if argv.len() == 1 {
        if let Err(e) = tui::run() {
            eprintln!("\n[ОШИБКА] {e}");
            std::process::exit(1);
        }
        return;
    }

    let cli = match Cli::try_parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    let result = real_main(cli);

    if let Err(e) = result {
        eprintln!("\n[ОШИБКА] {e}");
        std::process::exit(1);
    }
}

fn real_main(cli: Cli) -> AppResult<()> {
    match cli.cmd {
        Command::Strategies { cmd } => match cmd {
            StrategiesCmd::List => {
                let root = strategies::detect_root_dir()?;
                let list = strategies::load_strategies_from_root(&root)?;
                strategies::print_strategies(&list);
                Ok(())
            }
        },

        Command::Service { cmd } => {
            match cmd {
                ServiceCmd::Install {
                    strategy,
                    no_start,
                    force,
                } => {
                    let root = strategies::detect_root_dir()?;
                    ipset::bootstrap_user_lists(&root)?; // полезно иметь всегда

                    let all = strategies::load_strategies_from_root(&root)?;

                    let selected = match strategy {
                        Some(name) => strategies::validate_strategy_name(&all, &name)?,
                        None => strategies::choose_strategy_interactive(&all)?,
                    };

                    service::write_selected_strategy(&root, &selected.name)?;
                    service::install_muzap_service(&root, force, !no_start)?;
                    println!("[OK] Служба установлена. Стратегия: {}", selected.name);
                    Ok(())
                }

                ServiceCmd::SetStrategy { strategy, restart } => {
                    let root = strategies::detect_root_dir()?;
                    ipset::bootstrap_user_lists(&root)?;

                    let all = strategies::load_strategies_from_root(&root)?;
                    let s = strategies::validate_strategy_name(&all, &strategy)?;
                    service::write_selected_strategy(&root, &s.name)?;

                    if restart {
                        service::restart_service()?;
                        println!("[OK] Стратегия обновлена и служба перезапущена: {}", s.name);
                    } else {
                        println!("[OK] Стратегия обновлена: {}", s.name);
                        println!("[ИНФО] Чтобы применить, перезапустите службу: MuZap.exe service restart");
                    }

                    Ok(())
                }

                ServiceCmd::Remove { no_stop } => {
                    service::remove_service_best_effort(!no_stop)?;
                    println!("[OK] Удаление службы завершено (best-effort).");
                    Ok(())
                }

                ServiceCmd::Restart => {
                    service::restart_service()?;
                    println!("[OK] Команда перезапуска отправлена.");
                    Ok(())
                }

                ServiceCmd::Status => {
                    let root = strategies::detect_root_dir()?;
                    service::print_status(&root)?;
                    Ok(())
                }
            }
        }

        Command::Settings { cmd } => {
            let root = strategies::detect_root_dir()?;
            ipset::bootstrap_user_lists(&root)?;

            match cmd {
                SettingsCmd::Show => {
                    let cfg = settings::load_app_config_from_root(&root);

                    let ipset_mode = ipset::detect_mode(&root);

                    println!("Настройки (muzap.ini / lists):");
                    println!(
                        "  GameFilterMode   : {}",
                        settings::game_filter_mode_ru(&cfg.game_filter_mode)
                    );
                    println!(
                        "  TelemetryEnabled : {}",
                        if cfg.telemetry_enabled {
                            "вкл"
                        } else {
                            "выкл"
                        }
                    );
                    println!(
                        "  IPSet mode       : {} ({})",
                        ipset_mode.as_str(),
                        ipset_mode.ru()
                    );

                    Ok(())
                }

                SettingsCmd::SetGameFilterMode { value, restart } => {
                    settings::set_game_filter_mode(&root, &value)?;
                    println!("[OK] GameFilterMode установлен: {value}");

                    if restart && service::is_service_installed()? {
                        service::restart_service()?;
                        println!("[OK] Служба перезапущена.");
                    }
                    Ok(())
                }

                SettingsCmd::SetTelemetry { value, restart } => {
                    let v = settings::parse_bool_ru(&value)?;
                    settings::set_telemetry_enabled(&root, v)?;
                    println!(
                        "[OK] TelemetryEnabled установлен: {}",
                        if v { "true" } else { "false" }
                    );

                    if restart && service::is_service_installed()? {
                        service::restart_service()?;
                        println!("[OK] Служба перезапущена.");
                    }
                    Ok(())
                }

                SettingsCmd::SetIpSetMode { value, restart } => {
                    let m = ipset::parse_mode(&value)?;
                    let msg = ipset::set_mode(&root, m)?;
                    println!("[OK] {msg}");

                    if restart && service::is_service_installed()? {
                        service::restart_service()?;
                        println!("[OK] Служба перезапущена.");
                    }
                    Ok(())
                }
            }
        }

        Command::Updates { cmd } => {
            let root = strategies::detect_root_dir()?;
            ipset::bootstrap_user_lists(&root)?;

            match cmd {
                UpdatesCmd::IpSetUpdate => {
                    updates::update_ipset_list(&root)?;
                    println!("[OK] IPSet обновлён.");
                    Ok(())
                }
                UpdatesCmd::HostsUpdate => {
                    updates::update_hosts_block(&root, true)?;
                    println!("[OK] Hosts блок обновлён.");
                    Ok(())
                }
                UpdatesCmd::HostsRemove => {
                    updates::remove_hosts_block(&root, true)?;
                    println!("[OK] Hosts блок удалён (если был).");
                    Ok(())
                }
                UpdatesCmd::CheckRelease => {
                    updates::run_release_updater_check(&root, true)?;
                    Ok(())
                }
                UpdatesCmd::InstallRelease => {
                    updates::run_release_updater_install(&root, true)?;
                    Ok(())
                }
            }
        }

        Command::Tools { cmd } => {
            let root = strategies::detect_root_dir()?;
            ipset::bootstrap_user_lists(&root)?;

            match cmd {
                ToolsCmd::Diagnostics => {
                    let items = diagnostics::run_all(&root)?;
                    for l in diagnostics::render_lines(&items) {
                        println!("{l}");
                    }
                    Ok(())
                }
                ToolsCmd::FixAll => {
                    for l in fixes::fix_all()? {
                        println!("{l}");
                    }
                    Ok(())
                }
                ToolsCmd::FixConflicts => {
                    for l in fixes::fix_remove_conflicting_services()? {
                        println!("{l}");
                    }
                    Ok(())
                }
                ToolsCmd::FixWinDivert => {
                    for l in fixes::fix_remove_windivert_services()? {
                        println!("{l}");
                    }
                    Ok(())
                }
                ToolsCmd::FixTcpTimestamps => {
                    for l in fixes::fix_enable_tcp_timestamps()? {
                        println!("{l}");
                    }
                    Ok(())
                }
                ToolsCmd::FixDiscordCache => {
                    for l in fixes::fix_clear_discord_cache()? {
                        println!("{l}");
                    }
                    Ok(())
                }
                ToolsCmd::RunTester => {
                    tools::run_tester(&root)?;
                    Ok(())
                }
                ToolsCmd::ViewServiceLog => {
                    let lines = tools::read_service_log_lines(&root)?;
                    for l in lines {
                        println!("{l}");
                    }
                    Ok(())
                }
            }
        }
    }
}
