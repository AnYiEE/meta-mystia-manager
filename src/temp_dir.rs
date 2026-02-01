use crate::metrics::report_event;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, OnceLock};

/// 在 Guard 被 drop 时删除目录
pub struct DirGuard {
    path: PathBuf,
}

static GLOBAL_TEMP_DIRS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
static SET_CTRL_HANDLER: Once = Once::new();

impl DirGuard {
    pub fn new(path: PathBuf) -> Self {
        let _ = register_temp_dir_for_cleanup(path.clone());
        Self { path }
    }

    pub fn from_existing(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
        unregister_temp_dir(&self.path);
    }
}

fn cleanup_temp_dir(temp_dir: &Path) -> std::io::Result<()> {
    if temp_dir.exists() {
        std::fs::remove_dir_all(temp_dir)?;
    }
    Ok(())
}

fn register_temp_dir_for_cleanup(path: PathBuf) -> std::io::Result<()> {
    let m = GLOBAL_TEMP_DIRS.get_or_init(|| Mutex::new(Vec::new()));
    {
        let mut v = match m.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !v.contains(&path) {
            v.push(path.clone());
        }
    }

    SET_CTRL_HANDLER.call_once(|| {
        let m_ref = GLOBAL_TEMP_DIRS.get_or_init(|| Mutex::new(Vec::new()));
        let _ = ctrlc::set_handler(move || {
            let guard = match m_ref.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            for p in guard.iter() {
                let _ = std::fs::remove_dir_all(p);
            }
            std::process::exit(0);
        });

        const CTRL_C_EVENT: u32 = 0;
        const CTRL_BREAK_EVENT: u32 = 1;
        const CTRL_CLOSE_EVENT: u32 = 2;
        const CTRL_LOGOFF_EVENT: u32 = 5;
        const CTRL_SHUTDOWN_EVENT: u32 = 6;

        unsafe extern "system" fn console_handler(ctrl_type: u32) -> i32 {
            match ctrl_type {
                CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT
                | CTRL_SHUTDOWN_EVENT => {
                    if let Some(m) = GLOBAL_TEMP_DIRS.get() {
                        let guard = match m.lock() {
                            Ok(g) => g,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        for p in guard.iter() {
                            let _ = std::fs::remove_dir_all(p);
                        }
                    }
                    1
                }
                _ => 0,
            }
        }

        unsafe extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }

        unsafe {
            let _ = SetConsoleCtrlHandler(Some(console_handler), 1);
        }
    });

    Ok(())
}

fn unregister_temp_dir(path: &Path) {
    if let Some(m) = GLOBAL_TEMP_DIRS.get() {
        let mut v = match m.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        v.retain(|p| p != path);
    }
}

pub fn create_temp_dir_with_guard(base: &Path) -> std::io::Result<(PathBuf, DirGuard)> {
    let temp_dir = base.join(".meta-mystia-tmp");

    if let Some(m) = GLOBAL_TEMP_DIRS.get() {
        let guard = match m.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.contains(&temp_dir) {
            return Ok((temp_dir.clone(), DirGuard::from_existing(temp_dir)));
        }
    }

    if temp_dir.exists()
        && let Err(e) = cleanup_temp_dir(&temp_dir)
    {
        report_event(
            "TempDir.CleanupFailed",
            Some(&format!("{};err={}", temp_dir.to_string_lossy(), e)),
        );
    }

    if let Err(e) = std::fs::create_dir_all(&temp_dir) {
        report_event(
            "TempDir.CreateFailed",
            Some(&format!("{};err={}", temp_dir.to_string_lossy(), e)),
        );
        return Err(e);
    }

    report_event("TempDir.Created", Some(&temp_dir.to_string_lossy()));

    Ok((temp_dir.clone(), DirGuard::new(temp_dir)))
}
