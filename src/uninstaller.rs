use crate::config::uninstall_retry_config;
use crate::error::{ManagerError, Result};
use crate::file_ops::{
    DeletionStatus, count_results, execute_deletion, extract_failed_files, scan_existing_files,
};
use crate::permission::{elevate_and_restart, is_elevated};
use crate::ui::Ui;

use std::path::PathBuf;

/// 卸载管理器
pub struct Uninstaller<'a> {
    game_root: PathBuf,
    ui: &'a dyn Ui,
}

impl<'a> Uninstaller<'a> {
    pub fn new(game_root: PathBuf, ui: &'a dyn Ui) -> Result<Self> {
        Ok(Self { game_root, ui })
    }

    /// 执行卸载流程
    pub fn uninstall(&self) -> Result<()> {
        // 1. 选择卸载模式
        let mode = self.ui.uninstall_select_mode()?;

        // 2. 扫描实际存在的文件（相对于游戏目录）
        let existing_files = scan_existing_files(&self.game_root, mode);

        if existing_files.is_empty() {
            self.ui.uninstall_no_files_found()?;
            return Ok(());
        }

        // 3. 显示将要删除的文件列表
        self.ui.uninstall_display_target_files(&existing_files)?;

        // 4. 确认删除
        if !self.ui.uninstall_confirm_deletion()? {
            return Err(ManagerError::UserCancelled);
        }

        // 5. 检查当前权限状态
        let is_elevated = is_elevated()?;

        // 6. 执行删除操作
        let mut all_results = execute_deletion(&existing_files, self.ui);

        // 7. 处理失败项
        loop {
            let failed_files = extract_failed_files(&all_results);
            if failed_files.is_empty() {
                break;
            }

            let mut in_use_failures: Vec<std::path::PathBuf> = Vec::new();
            let mut perm_failures: Vec<std::path::PathBuf> = Vec::new();
            let mut other_failures: Vec<std::path::PathBuf> = Vec::new();

            for p in &failed_files {
                if let Some(r) = all_results.iter().find(|r| &r.path == p) {
                    match &r.status {
                        DeletionStatus::Failed(ManagerError::FileInUse(_)) => {
                            in_use_failures.push(p.clone())
                        }
                        DeletionStatus::Failed(ManagerError::PermissionDenied(_)) => {
                            perm_failures.push(p.clone())
                        }
                        _ => other_failures.push(p.clone()),
                    }
                } else {
                    other_failures.push(p.clone());
                }
            }

            if !in_use_failures.is_empty() {
                self.ui.uninstall_files_in_use_warning()?;

                let cfg = uninstall_retry_config();
                let mut still_in_use = in_use_failures.clone();

                for attempt in 0..cfg.attempts {
                    if still_in_use.is_empty() {
                        break;
                    }

                    let raw = (cfg.base_delay_secs as f64) * cfg.multiplier.powi(attempt as i32);
                    let delay_secs = raw.min(cfg.max_delay_secs as f64).ceil() as u64;

                    self.ui
                        .uninstall_wait_before_retry(delay_secs, attempt + 1, cfg.attempts)?;

                    std::thread::sleep(std::time::Duration::from_secs(delay_secs));

                    let retry_results = execute_deletion(&still_in_use, self.ui);

                    all_results.retain(|r| !still_in_use.contains(&r.path));
                    all_results.extend(retry_results.clone());

                    still_in_use = extract_failed_files(&all_results)
                        .into_iter()
                        .filter(|p| {
                            if let Some(r) = all_results.iter().find(|r| &r.path == p) {
                                matches!(
                                    r.status,
                                    DeletionStatus::Failed(ManagerError::FileInUse(_))
                                )
                            } else {
                                false
                            }
                        })
                        .collect();
                }

                let failed_files_after_in_use = extract_failed_files(&all_results);
                if failed_files_after_in_use.is_empty() {
                    break;
                }
            }

            let has_permission_issue = all_results.iter().any(|r| {
                matches!(
                    &r.status,
                    DeletionStatus::Failed(ManagerError::PermissionDenied(_))
                )
            });

            if has_permission_issue && !is_elevated && self.ui.uninstall_ask_elevate_permission()? {
                elevate_and_restart()?;
                self.ui.uninstall_restarting_elevated()?;
                std::process::exit(0);
            }

            if !self.ui.uninstall_ask_retry_failures()? {
                break;
            }

            self.ui.uninstall_retrying_failed_items()?;

            use std::collections::HashSet;
            let mut seen = HashSet::new();
            let mut retry_list: Vec<std::path::PathBuf> = Vec::new();

            let order: Vec<&Vec<std::path::PathBuf>> = if is_elevated {
                vec![&perm_failures, &other_failures]
            } else {
                vec![&other_failures, &perm_failures]
            };

            for group in order {
                for p in group {
                    if seen.insert(p.clone()) {
                        retry_list.push(p.clone());
                    }
                }
            }

            if !retry_list.is_empty() {
                let retry_results = execute_deletion(&retry_list, self.ui);
                all_results.retain(|r| !retry_list.contains(&r.path));
                all_results.extend(retry_results.clone());
            }
        }

        // 8. 显示操作摘要
        let (success, failed, skipped) = count_results(&all_results);
        self.ui.deletion_display_summary(success, failed, skipped);

        Ok(())
    }
}
