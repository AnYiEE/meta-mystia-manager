use crate::config::{GAME_EXECUTABLE, GAME_PROCESS_NAME};
use crate::error::{ManagerError, Result};

use std::path::PathBuf;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

struct SnapshotHandle(HANDLE);

impl SnapshotHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn as_raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for SnapshotHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// 检查游戏根目录
pub fn check_game_directory() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;

    let game_exe = current_dir.join(GAME_EXECUTABLE);
    if !game_exe.is_file() {
        return Err(ManagerError::GameNotFound);
    }

    Ok(current_dir)
}

/// 检查游戏进程是否正在运行
pub fn check_game_running() -> Result<bool> {
    unsafe {
        let snapshot_handle = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(handle) => SnapshotHandle::new(handle),
            Err(e) => {
                return Err(ManagerError::ProcessListError(format!(
                    "无法获取进程列表：{:?}",
                    e
                )));
            }
        };
        let snapshot = snapshot_handle.as_raw();

        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_err() {
            return Err(ManagerError::ProcessListError(
                "读取进程列表失败".to_string(),
            ));
        }

        let target = GAME_PROCESS_NAME.to_lowercase();

        loop {
            let process_name = String::from_utf16_lossy(
                &entry.szExeFile[..entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len())],
            );

            if process_name.to_lowercase() == target {
                return Ok(true);
            }

            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }

        Ok(false)
    }
}
