use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

#[cfg(windows)]
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
    System::Threading::GetCurrentProcess,
};

const SERVICE_NAME: &str = "MuZap";

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Msg(String),

    #[error("Ошибка ввода/вывода: {0}")]
    Io(#[from] std::io::Error),

    #[error("Ошибка muzap_core: {0}")]
    Core(#[from] muzap_core::CoreError),

    #[cfg(windows)]
    #[error("Ошибка windows_service: {0}")]
    WinService(#[from] windows_service::Error),
}

type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
struct JobHandle(HANDLE);

impl Drop for JobHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.0.is_null() {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[derive(Debug)]
struct SpawnedWinws {
    job: Option<JobHandle>,
    child: Child,
    attach_error: Option<u32>,
    child_in_job_before_attach: Option<bool>,
}

fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("muzap_service работает только в Windows.");
        std::process::exit(1);
    }

    #[cfg(windows)]
    {
        let args: Vec<OsString> = std::env::args_os().collect();
        let is_service_mode = args.iter().any(|a| a == "--service");

        if !is_service_mode {
            println!(
                "Это бинарник Windows-службы MuZap.\n\n\
                 Обычно его не запускают вручную.\n\
                 Установите/управляйте службой через:\n\
                 - MuZap.exe (меню)\n\
                 - MuZap.exe service install\n\
                 - MuZap.exe service status\n"
            );
            return;
        }

        if let Err(e) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
            eprintln!("[ОШИБКА] Не удалось запустить диспетчер службы: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
define_windows_service!(ffi_service_main, my_service_main);

#[cfg(windows)]
fn my_service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        let _ = log_line(&format!("[ОШИБКА] {e}"));
    }
}

#[cfg(windows)]
fn run_service() -> AppResult<()> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag2 = stop_flag.clone();

    let status_handle = register_handler(stop_flag2)?;
    set_status(
        &status_handle,
        ServiceState::StartPending,
        ServiceExitCode::Win32(0),
    )?;

    log_line("[ИНФО] Служба запускается...")?;
    log_job_context()?;

    // 1) Загружаем конфиг/стратегию
    let root = detect_root_fallback();
    let selected = read_selected_strategy(&root)?.ok_or_else(|| {
        AppError::Msg("Не выбрана стратегия: отсутствует .service\\selected_strategy.txt".into())
    })?;

    let winws_exe = root.join("bin").join("winws.exe");
    if !winws_exe.exists() {
        return Err(AppError::Msg(format!(
            "Не найден winws.exe: {}",
            winws_exe.display()
        )));
    }

    // ВАЖНО: если служба упала и SCM её рестартнул, старый winws мог остаться.
    // Мы убиваем только winws, который запущен именно из нашего bin\winws.exe.
    let killed = kill_existing_winws_by_path(&winws_exe);
    if killed > 0 {
        log_line(&format!(
            "[ИНФО] Перед стартом: завершено старых winws процессов (по пути): {killed}"
        ))?;
    }

    let strategies_ini = root.join("strategies.ini");
    let strategies = muzap_core::config_files::load_strategies(&strategies_ini)?;

    let strategy = strategies
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&selected))
        .cloned()
        .ok_or_else(|| {
            AppError::Msg(format!(
                "Выбранная стратегия не найдена в strategies.ini: '{selected}'"
            ))
        })?;

    let app_cfg = muzap_core::app::load_app_config(&root.join("muzap.ini"));
    let game_ports = muzap_core::app::get_game_filter_ports(&app_cfg);

    let bin_path = root.join("bin").to_string_lossy().to_string() + "\\";
    let lists_path = root.join("lists").to_string_lossy().to_string() + "\\";

    let params = muzap_core::params::substitute_params(
        &strategy.params,
        &bin_path,
        &lists_path,
        &game_ports.tcp,
        &game_ports.udp,
    );

    // 2) Запускаем winws + пробуем привязать к Job Object (kill-on-close)
    let spawned = spawn_winws_try_attach_job(&winws_exe, &params)?;

    if let Some(in_job) = spawned.child_in_job_before_attach {
        log_line(&format!(
            "[ИНФО] winws.exe до привязки: IsProcessInJob(any) = {in_job}"
        ))?;
    } else {
        log_line("[ИНФО] winws.exe до привязки: IsProcessInJob(any) = (не удалось определить)")?;
    }

    if let Some(code) = spawned.attach_error {
        log_line(&format!(
            "[ПРЕДУПРЕЖДЕНИЕ] Не удалось привязать winws.exe к Job Object. WinAPI error={code} ({})",
            winerr_hint(code)
        ))?;
        log_line(
            "[ПРЕДУПРЕЖДЕНИЕ] Продолжаю работу без Job Object. При аварийном завершении службы winws.exe может остаться запущенным.",
        )?;
        log_line(
            "[ПОДСКАЗКА] Это часто бывает, если процесс уже находится в Job Object (sandbox/дебаггер/агенты).",
        )?;
    } else {
        log_line("[ИНФО] winws.exe привязан к Job Object (KILL_ON_JOB_CLOSE).")?;
    }

    let mut child = spawned.child;
    let job_opt = spawned.job;

    log_line(&format!(
        "[ИНФО] Запущен winws.exe. Стратегия: {}",
        strategy.name
    ))?;

    set_status(
        &status_handle,
        ServiceState::Running,
        ServiceExitCode::Win32(0),
    )?;
    log_line("[ИНФО] Служба запущена (RUNNING).")?;

    // 3) Основной цикл: ждём stop или завершение child
    loop {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                log_line(&format!("[ПРЕДУПРЕЖДЕНИЕ] winws завершился: {status}"))?;
                break;
            }
            Ok(None) => {}
            Err(e) => {
                log_line(&format!(
                    "[ПРЕДУПРЕЖДЕНИЕ] Ошибка try_wait() для winws: {e}"
                ))?;
            }
        }

        thread::sleep(Duration::from_millis(300));
    }

    // 4) Остановка
    set_status(
        &status_handle,
        ServiceState::StopPending,
        ServiceExitCode::Win32(0),
    )?;
    log_line("[ИНФО] Остановка службы...")?;

    let _ = child.kill();
    let _ = child.wait();

    drop(job_opt);

    set_status(
        &status_handle,
        ServiceState::Stopped,
        ServiceExitCode::Win32(0),
    )?;
    log_line("[ИНФО] Служба остановлена.")?;

    Ok(())
}

#[cfg(windows)]
fn register_handler(stop_flag: Arc<AtomicBool>) -> AppResult<ServiceStatusHandle> {
    let handler = move |event| -> ServiceControlHandlerResult {
        match event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                stop_flag.store(true, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    Ok(service_control_handler::register(SERVICE_NAME, handler)?)
}

#[cfg(windows)]
fn set_status(
    handle: &ServiceStatusHandle,
    state: ServiceState,
    exit: ServiceExitCode,
) -> AppResult<()> {
    let status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: exit,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };

    handle.set_service_status(status)?;
    Ok(())
}

fn detect_root_fallback() -> PathBuf {
    if let Ok(found) = muzap_core::paths::detect_root(12) {
        return found;
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.parent().unwrap_or(Path::new(".")).to_path_buf()
}

fn read_selected_strategy(root: &Path) -> AppResult<Option<String>> {
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

fn log_path() -> PathBuf {
    let root = detect_root_fallback();
    root.join(".service").join("muzap_service.log")
}

fn log_line(line: &str) -> AppResult<()> {
    let path = log_path();

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "[{ts}] {line}")?;
    Ok(())
}

/* ========================= Job Object / debug helpers ========================= */

#[cfg(windows)]
fn win_last_error() -> u32 {
    unsafe { GetLastError() }
}

#[cfg(windows)]
fn winerr_hint(code: u32) -> &'static str {
    match code {
        5 => "ERROR_ACCESS_DENIED",
        6 => "ERROR_INVALID_HANDLE",
        87 => "ERROR_INVALID_PARAMETER",
        _ => "UNKNOWN",
    }
}

#[cfg(windows)]
fn is_process_in_any_job(process: HANDLE) -> Option<bool> {
    let mut r: i32 = 0;
    let ok = unsafe { IsProcessInJob(process, std::ptr::null_mut(), &mut r) };
    if ok == 0 {
        None
    } else {
        Some(r != 0)
    }
}

#[cfg(windows)]
fn log_job_context() -> AppResult<()> {
    let self_h = unsafe { GetCurrentProcess() };
    match is_process_in_any_job(self_h) {
        Some(v) => log_line(&format!("[ИНФО] Служба: IsProcessInJob(any) = {v}"))?,
        None => log_line("[ИНФО] Служба: IsProcessInJob(any) = (не удалось определить)")?,
    }
    Ok(())
}

#[cfg(windows)]
fn spawn_winws_try_attach_job(exe: &Path, params: &str) -> AppResult<SpawnedWinws> {
    let args = muzap_core::process::split_args(params);

    let child = Command::new(exe)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map_err(|e| AppError::Msg(format!("Не удалось запустить winws.exe: {e}")))?;

    let hproc = child.as_raw_handle() as HANDLE;
    let child_in_job = is_process_in_any_job(hproc);

    let job = create_kill_on_close_job()?;

    let ok = unsafe { AssignProcessToJobObject(job.0, hproc) };
    if ok == 0 {
        let code = win_last_error();
        drop(job);
        return Ok(SpawnedWinws {
            job: None,
            child,
            attach_error: Some(code),
            child_in_job_before_attach: child_in_job,
        });
    }

    Ok(SpawnedWinws {
        job: Some(job),
        child,
        attach_error: None,
        child_in_job_before_attach: child_in_job,
    })
}

#[cfg(windows)]
fn create_kill_on_close_job() -> AppResult<JobHandle> {
    let h = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if h.is_null() {
        return Err(AppError::Msg("CreateJobObjectW вернул NULL.".into()));
    }

    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    let ok = unsafe {
        SetInformationJobObject(
            h,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };

    if ok == 0 {
        unsafe { CloseHandle(h) };
        return Err(AppError::Msg(
            "SetInformationJobObject (KILL_ON_JOB_CLOSE) не удалось.".into(),
        ));
    }

    Ok(JobHandle(h))
}

/* ========================= Start cleanup ========================= */

fn kill_existing_winws_by_path(wanted_exe: &Path) -> usize {
    let wanted_norm = normalize_path(wanted_exe);

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut killed = 0usize;

    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        if name != "winws.exe" && name != "winws" {
            continue;
        }

        let Some(exe) = p.exe() else { continue };
        let exe_norm = normalize_path(exe);

        if exe_norm == wanted_norm {
            let _ = p.kill();
            killed += 1;
        }
    }

    killed
}

fn normalize_path(p: &Path) -> String {
    // best-effort canonicalize; fallback to raw
    let s = fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    s
}
