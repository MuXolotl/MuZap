use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use crate::{AppError, AppResult};

const SERVICE_NAME: &str = "MuZap";
const SERVICE_DISPLAY_NAME: &str = "MuZap";
const SERVICE_DESCRIPTION: &str =
    "MuZap service (Rust): запускает winws.exe по выбранной стратегии";

pub fn write_selected_strategy(root: &Path, name: &str) -> AppResult<()> {
    let service_dir = root.join(".service");
    fs::create_dir_all(&service_dir)?;

    let p = service_dir.join("selected_strategy.txt");
    fs::write(p, format!("{}\n", name.trim()))?;
    Ok(())
}

pub fn read_selected_strategy(root: &Path) -> AppResult<Option<String>> {
    let p = root.join(".service").join("selected_strategy.txt");
    if !p.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(p)?;
    let name = text.lines().next().unwrap_or("").trim().to_string();
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

pub fn is_service_installed() -> AppResult<bool> {
    #[cfg(not(windows))]
    {
        Ok(false)
    }

    #[cfg(windows)]
    {
        use windows_service::{
            service::ServiceAccess,
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

pub fn query_service_state_text() -> AppResult<String> {
    #[cfg(not(windows))]
    {
        Ok("НЕ ДОСТУПНО (не Windows)".into())
    }

    #[cfg(windows)]
    {
        use windows_service::{
            service::ServiceAccess,
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

        let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
            Ok(s) => s,
            Err(_) => return Ok("НЕ УСТАНОВЛЕНА".into()),
        };

        let st = service.query_status()?;
        Ok(format!("{:?}", st.current_state).to_ascii_uppercase())
    }
}

pub fn print_status(root: &Path) -> AppResult<()> {
    #[cfg(not(windows))]
    {
        let _ = root;
        return Err(AppError::Msg(
            "Команды службы доступны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use windows_service::{
            service::ServiceAccess,
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        let selected = read_selected_strategy(root)?;
        println!(
            "Выбранная стратегия: {}",
            selected.as_deref().unwrap_or("не выбрана")
        );

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

        let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
            Ok(s) => s,
            Err(_) => {
                println!("Служба: НЕ установлена");
                return Ok(());
            }
        };

        let st = service.query_status()?;
        println!("Служба: установлена");
        println!("Имя: {SERVICE_NAME}");
        println!(
            "Состояние: {}",
            format!("{:?}", st.current_state).to_ascii_uppercase()
        );

        Ok(())
    }
}

pub fn install_muzap_service(root: &Path, force: bool, start_now: bool) -> AppResult<()> {
    #[cfg(not(windows))]
    {
        let _ = (root, force, start_now);
        return Err(AppError::Msg(
            "Установка службы доступна только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use std::time::Duration;
        use windows_service::{
            service::{
                ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
                ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
                ServiceType,
            },
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        let service_exe = find_service_exe(root)?;

        let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
        let manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        if force {
            remove_service_best_effort(true)?;
        } else if is_service_installed()? {
            return Err(AppError::Msg(
                "Служба MuZap уже установлена. Используйте: MuZap.exe service install --force"
                    .into(),
            ));
        }

        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: service_exe.to_path_buf(),
            launch_arguments: vec![OsString::from("--service")],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };

        // Добавляем CHANGE_CONFIG, чтобы можно было настроить recovery options
        let access = ServiceAccess::QUERY_STATUS
            | ServiceAccess::START
            | ServiceAccess::STOP
            | ServiceAccess::CHANGE_CONFIG;

        let service = manager.create_service(&service_info, access)?;
        let _ = service.set_description(SERVICE_DESCRIPTION);

        // Делает автозапуск менее “агрессивным” на старте Windows (полезно для сетевых сервисов)
        let _ = service.set_delayed_auto_start(true);

        // Failure actions: перезапускать службу при падении.
        // Это важно, если по какой-то причине muzap_service.exe упал.
        let actions = vec![
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(2),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(2),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(2),
            },
        ];

        let failure_actions = ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86400)),
            reboot_msg: None,
            command: None,
            actions: Some(actions),
        };

        let _ = service.update_failure_actions(failure_actions);
        let _ = service.set_failure_actions_on_non_crash_failures(true);

        if start_now {
            // Пустой slice требует явного типа (иначе E0283)
            let empty: [OsString; 0] = [];
            let _ = service.start(&empty);
        }

        Ok(())
    }
}

pub fn restart_service() -> AppResult<()> {
    #[cfg(not(windows))]
    {
        return Err(AppError::Msg(
            "Команды службы доступны только в Windows.".into(),
        ));
    }

    #[cfg(windows)]
    {
        use windows_service::{
            service::{ServiceAccess, ServiceState},
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::START;
        let service = manager.open_service(SERVICE_NAME, access)?;

        let st = service.query_status()?;
        if st.current_state != ServiceState::Stopped {
            let _ = service.stop();
        }

        let empty: [OsString; 0] = [];
        let _ = service.start(&empty);
        Ok(())
    }
}

pub fn remove_service_best_effort(try_stop: bool) -> AppResult<()> {
    #[cfg(not(windows))]
    {
        let _ = try_stop;
        return Ok(());
    }

    #[cfg(windows)]
    {
        use windows_service::{
            service::{ServiceAccess, ServiceState},
            service_manager::{ServiceManager, ServiceManagerAccess},
        };

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;

        let service = match manager.open_service(SERVICE_NAME, access) {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };

        if try_stop {
            if let Ok(st) = service.query_status() {
                if st.current_state != ServiceState::Stopped {
                    let _ = service.stop();
                }
            }
        }

        let _ = service.delete();
        drop(service);

        Ok(())
    }
}

fn find_service_exe(root: &Path) -> AppResult<PathBuf> {
    let p = root.join("muzap_service.exe");
    if p.exists() {
        return Ok(p);
    }

    Err(AppError::Msg(format!(
        "Не найден muzap_service.exe: {}\nСоберите rust/muzap_service (release) и положите muzap_service.exe в корень MuZap.",
        p.display()
    )))
}
