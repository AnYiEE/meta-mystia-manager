use crate::config::UninstallMode;
use crate::error::ManagerError;
use crate::ui::Ui;

use glob::glob;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

#[allow(clippy::permissions_set_readonly_false)]
fn ensure_owner_writable(metadata: &std::fs::Metadata) -> std::fs::Permissions {
    let mut perms = metadata.permissions();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = perms.mode() | 0o200;
        perms.set_mode(mode);
    }

    #[cfg(not(unix))]
    {
        perms.set_readonly(false);
    }

    perms
}

const ERROR_SHARING_VIOLATION: i32 = 32;

/// 将 io::Error 映射为更具体的 UninstallError
pub fn map_io_error_to_uninstall_error(err: &io::Error, path: &Path) -> ManagerError {
    if let Some(code) = err.raw_os_error()
        && cfg!(target_os = "windows")
        && code == ERROR_SHARING_VIOLATION
    {
        return ManagerError::FileInUse(path.display().to_string());
    }
    ManagerError::Io(format!("{}", err))
}

/// 原子重命名或回退到 copy + remove
pub fn atomic_rename_or_copy(src: &Path, dst: &Path) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match std::fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(rename_err) => match std::fs::copy(src, dst) {
            Ok(_) => {
                std::fs::remove_file(src)?;
                Ok(())
            }
            Err(copy_err) => Err(io::Error::other(format!(
                "重命名失败：{}；复制失败：{}",
                rename_err, copy_err
            ))),
        },
    }
}

fn backup_with_index(path: &Path, ext_suffix: &str) -> io::Result<PathBuf> {
    if !path.exists() {
        return Err(std::io::Error::new(
            ErrorKind::NotFound,
            format!("源路径不存在：{}", path.display()),
        ));
    }

    let mut idx = 0;
    loop {
        let backup = if idx == 0 {
            path.with_extension(ext_suffix)
        } else {
            path.with_extension(format!("{}.{}", ext_suffix, idx))
        };

        if backup.exists() {
            idx += 1;
            continue;
        }

        match atomic_rename_or_copy(path, &backup) {
            Ok(_) => return Ok(backup),
            Err(e) => {
                if backup.exists() {
                    idx += 1;
                    continue;
                } else {
                    return Err(e);
                }
            }
        }
    }
}

fn normalize_path_for_glob(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub struct RemoveGlobResult {
    pub removed: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, io::Error)>,
}

/// 删除匹配 glob 模式的文件/目录
pub fn remove_glob_files(pattern: &Path) -> RemoveGlobResult {
    let mut removed = Vec::new();
    let mut failed = Vec::new();

    let pattern_str = normalize_path_for_glob(pattern);
    if let Ok(entries) = glob(&pattern_str) {
        for entry in entries.flatten() {
            if entry.exists() {
                let res = if entry.is_dir() {
                    std::fs::remove_dir_all(&entry)
                } else {
                    std::fs::remove_file(&entry)
                };

                match res {
                    Ok(_) => removed.push(entry),
                    Err(e) => failed.push((entry, e)),
                }
            }
        }
    }

    RemoveGlobResult { removed, failed }
}

/// 备份一组路径（使用 backup_with_index）
pub fn backup_paths_with_index(
    paths: &[PathBuf],
    ext_suffix: &str,
) -> Vec<Result<PathBuf, io::Error>> {
    paths
        .iter()
        .map(|p| match backup_with_index(p, ext_suffix) {
            Ok(b) => Ok(b),
            Err(e) => Err(e),
        })
        .collect()
}

/// 根据 glob 模式获取匹配的路径列表
pub fn glob_matches(pattern: &Path) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    let s = normalize_path_for_glob(pattern);

    if let Ok(entries) = glob(&s) {
        for entry in entries.flatten() {
            if entry.exists() {
                matches.push(entry);
            }
        }
    }

    matches
}

#[derive(Clone)]
pub enum DeletionStatus {
    Success,
    Failed(ManagerError),
    Skipped,
}

#[derive(Clone)]
pub struct DeletionResult {
    pub path: PathBuf,
    pub status: DeletionStatus,
}

/// 扫描实际存在的文件
pub fn scan_existing_files(base: &Path, mode: UninstallMode) -> Vec<PathBuf> {
    let targets = mode.get_targets();
    let mut existing_files = Vec::new();

    for &(pattern, is_dir) in targets {
        scan_target(base, pattern, is_dir, &mut existing_files);
    }

    existing_files
}

/// 扫描单个删除目标
fn scan_target(base: &Path, pattern: &str, is_directory: bool, existing_files: &mut Vec<PathBuf>) {
    let target_path = base.join(pattern);
    let path_str = normalize_path_for_glob(&target_path);

    if path_str.contains('*') {
        if let Ok(entries) = glob(&path_str) {
            for entry in entries.flatten() {
                if entry.exists()
                    && ((is_directory && entry.is_dir()) || (!is_directory && entry.is_file()))
                {
                    existing_files.push(entry);
                }
            }
        }
    } else if target_path.exists() {
        let is_dir = target_path.is_dir();
        if is_dir == is_directory {
            existing_files.push(target_path);
        }
    }
}

/// 执行删除操作
pub fn execute_deletion(files: &[PathBuf], ui: &dyn Ui) -> Vec<DeletionResult> {
    let total = files.len();
    let mut results = Vec::new();

    let _ = ui.message("");

    for (index, path) in files.iter().enumerate() {
        ui.deletion_display_progress(index + 1, total, &path.to_string_lossy());

        let result = if path.is_dir() {
            delete_directory(path)
        } else {
            delete_file(path)
        };

        match &result.status {
            DeletionStatus::Success => ui.deletion_display_success(&path.to_string_lossy()),
            DeletionStatus::Failed(error) => {
                ui.deletion_display_failure(&path.to_string_lossy(), &error.to_string())
            }
            DeletionStatus::Skipped => ui.deletion_display_skipped(&path.to_string_lossy()),
        }

        results.push(result);
    }

    results
}

/// 删除单个文件
fn delete_file(path: &Path) -> DeletionResult {
    if !path.exists() {
        return DeletionResult {
            path: path.to_path_buf(),
            status: DeletionStatus::Skipped,
        };
    }

    match std::fs::remove_file(path) {
        Ok(_) => {
            if path.exists() {
                DeletionResult {
                    path: path.to_path_buf(),
                    status: DeletionStatus::Failed(ManagerError::Other(
                        "执行删除后文件仍存在".to_string(),
                    )),
                }
            } else {
                DeletionResult {
                    path: path.to_path_buf(),
                    status: DeletionStatus::Success,
                }
            }
        }
        Err(e) => {
            // 先检测是否为“文件被占用”类错误
            if let ManagerError::FileInUse(_) = map_io_error_to_uninstall_error(&e, path) {
                return DeletionResult {
                    path: path.to_path_buf(),
                    status: DeletionStatus::Failed(ManagerError::FileInUse(
                        path.display().to_string(),
                    )),
                };
            }

            // 若为权限错误，尝试清除只读属性并重试一次
            if e.kind() == ErrorKind::PermissionDenied
                && let Ok(metadata) = std::fs::metadata(path)
            {
                let perms = ensure_owner_writable(&metadata);
                let _ = std::fs::set_permissions(path, perms);
                if std::fs::remove_file(path).is_ok() {
                    return DeletionResult {
                        path: path.to_path_buf(),
                        status: DeletionStatus::Success,
                    };
                }
            }

            let error = match e.kind() {
                ErrorKind::PermissionDenied => {
                    ManagerError::PermissionDenied(path.display().to_string())
                }
                ErrorKind::NotFound => {
                    return DeletionResult {
                        path: path.to_path_buf(),
                        status: DeletionStatus::Skipped,
                    };
                }
                _ => map_io_error_to_uninstall_error(&e, path),
            };

            DeletionResult {
                path: path.to_path_buf(),
                status: DeletionStatus::Failed(error),
            }
        }
    }
}

/// 删除目录
fn delete_directory(path: &Path) -> DeletionResult {
    if !path.exists() {
        return DeletionResult {
            path: path.to_path_buf(),
            status: DeletionStatus::Skipped,
        };
    }

    match std::fs::remove_dir_all(path) {
        Ok(_) => {
            if path.exists() {
                DeletionResult {
                    path: path.to_path_buf(),
                    status: DeletionStatus::Failed(ManagerError::Other(
                        "执行删除后文件夹仍存在".to_string(),
                    )),
                }
            } else {
                DeletionResult {
                    path: path.to_path_buf(),
                    status: DeletionStatus::Success,
                }
            }
        }
        Err(e) => {
            // 先检测是否为“文件/目录被占用”类错误
            if let ManagerError::FileInUse(_) = map_io_error_to_uninstall_error(&e, path) {
                return DeletionResult {
                    path: path.to_path_buf(),
                    status: DeletionStatus::Failed(ManagerError::FileInUse(
                        path.display().to_string(),
                    )),
                };
            }

            // 权限错误时尝试清除只读并重试一次
            if e.kind() == ErrorKind::PermissionDenied
                && let Ok(metadata) = std::fs::metadata(path)
            {
                let perms = ensure_owner_writable(&metadata);
                let _ = std::fs::set_permissions(path, perms);
                if std::fs::remove_dir_all(path).is_ok() {
                    return DeletionResult {
                        path: path.to_path_buf(),
                        status: DeletionStatus::Success,
                    };
                }
            }

            let error = match e.kind() {
                ErrorKind::PermissionDenied => {
                    ManagerError::PermissionDenied(path.display().to_string())
                }
                ErrorKind::NotFound => {
                    return DeletionResult {
                        path: path.to_path_buf(),
                        status: DeletionStatus::Skipped,
                    };
                }
                _ => map_io_error_to_uninstall_error(&e, path),
            };

            DeletionResult {
                path: path.to_path_buf(),
                status: DeletionStatus::Failed(error),
            }
        }
    }
}

/// 从结果中提取失败的项目
pub fn extract_failed_files(results: &[DeletionResult]) -> Vec<PathBuf> {
    results
        .iter()
        .filter_map(|r| match r.status {
            DeletionStatus::Failed(_) => Some(r.path.clone()),
            _ => None,
        })
        .collect()
}

/// 统计删除结果
pub fn count_results(results: &[DeletionResult]) -> (usize, usize, usize) {
    let mut success = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for result in results {
        match result.status {
            DeletionStatus::Success => success += 1,
            DeletionStatus::Failed(_) => failed += 1,
            DeletionStatus::Skipped => skipped += 1,
        }
    }

    (success, failed, skipped)
}
