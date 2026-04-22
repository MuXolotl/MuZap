use std::{
    ffi::OsStr,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{AppError, AppResult};

pub fn atomic_write_replace_bytes(dest: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| AppError::Msg("Не удалось определить папку назначения".into()))?;

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let tmp_name = format!(".muzap_tmp.{pid}.{nanos}");
    let tmp_path = dir.join(tmp_name);

    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
    }

    let res = atomic_replace_impl(dest, &tmp_path);

    if res.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }

    res
}

fn atomic_replace_impl(dest: &Path, tmp: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING,
        };

        fn to_wide_null(p: &Path) -> Vec<u16> {
            let mut v: Vec<u16> = OsStr::new(p).encode_wide().collect();
            v.push(0);
            v
        }

        let dest_w = to_wide_null(dest);
        let tmp_w = to_wide_null(tmp);

        // Если dest существует — ReplaceFileW.
        // Если dest не существует — ReplaceFileW скорее всего упадёт, и пойдём в MoveFileExW.
        let ok = unsafe {
            ReplaceFileW(
                dest_w.as_ptr(),
                tmp_w.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if ok != 0 {
            return Ok(());
        }

        let ok2 =
            unsafe { MoveFileExW(tmp_w.as_ptr(), dest_w.as_ptr(), MOVEFILE_REPLACE_EXISTING) };
        if ok2 != 0 {
            return Ok(());
        }

        let code = unsafe { GetLastError() };
        Err(AppError::Msg(format!(
            "Не удалось заменить файл атомарно (ReplaceFileW/MoveFileExW). Код WinAPI: {code}"
        )))
    }

    #[cfg(not(windows))]
    {
        if dest.exists() {
            let _ = std::fs::remove_file(dest);
        }
        std::fs::rename(tmp, dest)?;
        Ok(())
    }
}

pub fn default_hosts_path() -> PathBuf {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    PathBuf::from(sysroot).join(r"System32\drivers\etc\hosts")
}
