use crate::config::UninstallMode;
use crate::error::Result;
use crate::metrics::{get_user_id, report_event};
use crate::model::VersionInfo;

use console::{Term, style};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 操作模式枚举
pub enum OperationMode {
    Install,
    Upgrade,
    Uninstall,
}

/// UI 抽象接口
pub trait Ui: Send + Sync {
    fn display_welcome(&self) -> Result<()>;
    fn display_version(&self, manager_version: Option<&str>) -> Result<()>;
    fn display_game_running_warning(&self) -> Result<()>;
    fn display_available_updates(
        &self,
        dll_available: bool,
        resourceex_available: bool,
    ) -> Result<()>;
    fn select_operation_mode(&self) -> Result<OperationMode>;

    fn blank_line(&self) -> Result<()>;
    fn wait_for_key(&self) -> Result<()>;

    // 通用输出
    fn message(&self, text: &str) -> Result<()>;
    #[allow(dead_code)]
    fn warn(&self, text: &str) -> Result<()>;
    #[allow(dead_code)]
    fn error(&self, text: &str) -> Result<()>;

    // 目录相关
    fn path_display_steam_found(&self, app_id: u32, name: Option<&str>, path: &Path) -> Result<()>;
    fn path_confirm_use_steam_found(&self) -> Result<bool>;

    // 安装相关
    fn install_display_step(&self, step: usize, description: &str);
    fn install_display_version_info(&self, version_info: &VersionInfo);
    fn install_warn_existing(
        &self,
        bepinex_installed: bool,
        metamystia_installed: bool,
        resourceex_installed: bool,
    ) -> Result<()>;
    fn install_confirm_overwrite(&self) -> Result<bool>;
    fn install_ask_install_resourceex(&self) -> Result<bool>;
    fn install_ask_show_bepinex_console(&self) -> Result<bool>;
    fn install_downloads_completed(&self) -> Result<()>;
    fn install_start_cleanup(&self) -> Result<()>;
    fn install_cleanup_result(&self, success_count: usize, failed_count: usize) -> Result<()>;
    fn install_finished(&self, show_bepinex_console: bool) -> Result<()>;

    // 升级相关
    fn upgrade_warn_unparse_version(&self, filename: &str) -> Result<()>;
    fn upgrade_backup_failed(&self, err: &str) -> Result<()>;
    fn upgrade_deleted(&self, path: &Path) -> Result<()>;
    fn upgrade_delete_failed(&self, path: &Path, err: &str) -> Result<()>;
    fn upgrade_checking_installed_version(&self) -> Result<()>;
    fn upgrade_detected_resourceex(&self) -> Result<()>;
    fn upgrade_display_current_and_latest_dll(&self, current: &str, latest: &str) -> Result<()>;
    fn upgrade_display_current_and_latest_resourceex(
        &self,
        current: &str,
        latest: &str,
    ) -> Result<()>;
    fn upgrade_no_update_needed(&self) -> Result<()>;
    fn upgrade_detected_new_dll(&self, current: &str, new: &str) -> Result<()>;
    fn upgrade_dll_already_latest(&self) -> Result<()>;
    fn upgrade_resourceex_needs_upgrade(&self) -> Result<()>;
    fn upgrade_downloading_dll(&self) -> Result<()>;
    fn upgrade_downloading_resourceex(&self) -> Result<()>;
    fn upgrade_installing_dll(&self) -> Result<()>;
    fn upgrade_installing_resourceex(&self) -> Result<()>;
    fn upgrade_install_success(&self, path: &Path) -> Result<()>;
    fn upgrade_cleanup_start(&self) -> Result<()>;
    fn upgrade_done(&self) -> Result<()>;

    // 卸载相关
    fn uninstall_select_mode(&self) -> Result<UninstallMode>;
    fn uninstall_no_files_found(&self) -> Result<()>;
    fn uninstall_display_target_files(&self, files: &[PathBuf]) -> Result<()>;
    fn uninstall_confirm_deletion(&self) -> Result<bool>;
    fn uninstall_files_in_use_warning(&self) -> Result<()>;
    fn uninstall_wait_before_retry(
        &self,
        delay_secs: u64,
        attempt: usize,
        attempts: usize,
    ) -> Result<()>;
    fn uninstall_ask_elevate_permission(&self) -> Result<bool>;
    fn uninstall_restarting_elevated(&self) -> Result<()>;
    fn uninstall_ask_retry_failures(&self) -> Result<bool>;
    fn uninstall_retrying_failed_items(&self) -> Result<()>;

    // 删除相关
    fn deletion_display_progress(&self, current: usize, total: usize, path: &str);
    fn deletion_display_success(&self, path: &str);
    fn deletion_display_failure(&self, path: &str, error: &str);
    fn deletion_display_skipped(&self, path: &str);
    fn deletion_display_summary(
        &self,
        success_count: usize,
        failed_count: usize,
        skipped_count: usize,
    );

    // 下载相关
    /// 开始一个下载任务，返回一个用于后续更新的 id
    fn download_start(&self, filename: &str, total: Option<u64>) -> usize;
    /// 更新下载进度（传入 download_start 返回的 id）
    fn download_update(&self, id: usize, downloaded: u64);
    /// 完成下载任务（并显示完成信息）
    fn download_finish(&self, id: usize, message: &str);
    fn download_version_info_start(&self) -> Result<()>;
    fn download_version_info_failed(&self, err: &str) -> Result<()>;
    fn download_version_info_success(&self) -> Result<()>;
    fn download_version_info_parse_failed(&self, err: &str, snippet: &str) -> Result<()>;
    fn download_share_code_start(&self) -> Result<()>;
    fn download_share_code_failed(&self, err: &str) -> Result<()>;
    fn download_share_code_success(&self) -> Result<()>;
    fn download_attempt_github_dll(&self) -> Result<()>;
    fn download_found_github_asset(&self, name: &str) -> Result<()>;
    fn download_github_dll_not_found(&self) -> Result<()>;
    fn download_switch_to_fallback(&self, reason: &str) -> Result<()>;
    fn download_try_fallback_metamystia(&self) -> Result<()>;
    fn download_resourceex_start(&self) -> Result<()>;
    fn download_bepinex_attempt_primary(&self) -> Result<()>;
    fn download_bepinex_primary_failed(&self, err: &str) -> Result<()>;

    // 网络相关
    fn network_retrying(
        &self,
        op_desc: &str,
        delay_secs: u64,
        attempt: usize,
        attempts: usize,
        err: &str,
    ) -> Result<()>;
    fn network_rate_limited(&self, secs: u64) -> Result<()>;

    // 自升级相关
    fn manager_ask_self_update(&self, current_version: &str, latest_version: &str) -> Result<bool>;
    fn manager_update_starting(&self) -> Result<()>;
    fn manager_update_failed(&self, err: &str) -> Result<()>;
    fn manager_prompt_manual_update(&self) -> Result<()>;
}

pub struct ConsoleUI {
    bars: Mutex<HashMap<usize, ProgressBar>>,
    next_id: AtomicUsize,
}

impl ConsoleUI {
    pub fn new() -> Self {
        Self {
            bars: Mutex::new(HashMap::new()),
            next_id: AtomicUsize::new(1),
        }
    }
}

impl Ui for ConsoleUI {
    fn display_welcome(&self) -> Result<()> {
        display_welcome()
    }

    fn display_version(&self, manager_version: Option<&str>) -> Result<()> {
        display_version(manager_version)
    }

    fn display_game_running_warning(&self) -> Result<()> {
        display_game_running_warning()
    }

    fn display_available_updates(
        &self,
        dll_available: bool,
        resourceex_available: bool,
    ) -> Result<()> {
        display_available_updates(dll_available, resourceex_available)
    }

    fn select_operation_mode(&self) -> Result<OperationMode> {
        select_operation_mode()
    }

    fn blank_line(&self) -> Result<()> {
        blank_line()
    }

    fn wait_for_key(&self) -> Result<()> {
        wait_for_key()
    }

    fn message(&self, text: &str) -> Result<()> {
        println!("{}", text);
        Ok(())
    }

    fn warn(&self, text: &str) -> Result<()> {
        println!("{}", style(text).yellow());
        Ok(())
    }

    fn error(&self, text: &str) -> Result<()> {
        println!();
        println!("{}", style(text).red());
        Ok(())
    }

    fn path_display_steam_found(&self, app_id: u32, name: Option<&str>, path: &Path) -> Result<()> {
        path_display_steam_found(app_id, name, path)
    }

    fn path_confirm_use_steam_found(&self) -> Result<bool> {
        path_confirm_use_steam_found()
    }

    fn install_display_step(&self, step: usize, description: &str) {
        install_display_step(step, description)
    }

    fn install_display_version_info(&self, version_info: &VersionInfo) {
        install_display_version_info(version_info)
    }

    fn install_warn_existing(
        &self,
        bepinex_installed: bool,
        metamystia_installed: bool,
        resourceex_installed: bool,
    ) -> Result<()> {
        install_warn_existing(
            bepinex_installed,
            metamystia_installed,
            resourceex_installed,
        )
    }

    fn install_confirm_overwrite(&self) -> Result<bool> {
        install_confirm_overwrite()
    }

    fn install_ask_install_resourceex(&self) -> Result<bool> {
        install_ask_install_resourceex()
    }

    fn install_ask_show_bepinex_console(&self) -> Result<bool> {
        install_ask_show_bepinex_console()
    }

    fn install_downloads_completed(&self) -> Result<()> {
        install_downloads_completed()
    }

    fn install_start_cleanup(&self) -> Result<()> {
        install_start_cleanup()
    }

    fn install_cleanup_result(&self, success_count: usize, failed_count: usize) -> Result<()> {
        install_cleanup_result(success_count, failed_count)
    }

    fn install_finished(&self, show_bepinex_console: bool) -> Result<()> {
        install_finished(show_bepinex_console)
    }

    fn upgrade_warn_unparse_version(&self, filename: &str) -> Result<()> {
        upgrade_warn_unparse_version(filename)
    }

    fn upgrade_backup_failed(&self, err: &str) -> Result<()> {
        upgrade_backup_failed(err)
    }

    fn upgrade_deleted(&self, path: &Path) -> Result<()> {
        upgrade_deleted(path)
    }

    fn upgrade_delete_failed(&self, path: &Path, err: &str) -> Result<()> {
        upgrade_delete_failed(path, err)
    }

    fn upgrade_checking_installed_version(&self) -> Result<()> {
        upgrade_checking_installed_version()
    }

    fn upgrade_detected_resourceex(&self) -> Result<()> {
        upgrade_detected_resourceex()
    }

    fn upgrade_display_current_and_latest_dll(&self, current: &str, latest: &str) -> Result<()> {
        upgrade_display_current_and_latest_dll(current, latest)
    }

    fn upgrade_display_current_and_latest_resourceex(
        &self,
        current: &str,
        latest: &str,
    ) -> Result<()> {
        upgrade_display_current_and_latest_resourceex(current, latest)
    }

    fn upgrade_no_update_needed(&self) -> Result<()> {
        upgrade_no_update_needed()
    }

    fn upgrade_detected_new_dll(&self, current: &str, new: &str) -> Result<()> {
        upgrade_detected_new_dll(current, new)
    }

    fn upgrade_dll_already_latest(&self) -> Result<()> {
        upgrade_dll_already_latest()
    }

    fn upgrade_resourceex_needs_upgrade(&self) -> Result<()> {
        upgrade_resourceex_needs_upgrade()
    }

    fn upgrade_downloading_dll(&self) -> Result<()> {
        upgrade_downloading_dll()
    }

    fn upgrade_downloading_resourceex(&self) -> Result<()> {
        upgrade_downloading_resourceex()
    }

    fn upgrade_installing_dll(&self) -> Result<()> {
        upgrade_installing_dll()
    }

    fn upgrade_installing_resourceex(&self) -> Result<()> {
        upgrade_installing_resourceex()
    }

    fn upgrade_install_success(&self, path: &Path) -> Result<()> {
        upgrade_install_success(path)
    }

    fn upgrade_cleanup_start(&self) -> Result<()> {
        upgrade_cleanup_start()
    }

    fn upgrade_done(&self) -> Result<()> {
        upgrade_done()
    }

    fn uninstall_select_mode(&self) -> Result<UninstallMode> {
        uninstall_select_uninstall_mode()
    }

    fn uninstall_no_files_found(&self) -> Result<()> {
        uninstall_no_files_found()
    }

    fn uninstall_display_target_files(&self, files: &[PathBuf]) -> Result<()> {
        uninstall_display_target_files(files)
    }

    fn uninstall_confirm_deletion(&self) -> Result<bool> {
        uninstall_confirm_deletion()
    }

    fn uninstall_files_in_use_warning(&self) -> Result<()> {
        uninstall_files_in_use_warning()
    }

    fn uninstall_wait_before_retry(
        &self,
        delay_secs: u64,
        attempt: usize,
        attempts: usize,
    ) -> Result<()> {
        uninstall_wait_before_retry(delay_secs, attempt, attempts)
    }

    fn uninstall_ask_elevate_permission(&self) -> Result<bool> {
        uninstall_ask_elevate_permission()
    }

    fn uninstall_restarting_elevated(&self) -> Result<()> {
        uninstall_restarting_elevated()
    }

    fn uninstall_ask_retry_failures(&self) -> Result<bool> {
        uninstall_ask_retry_failures()
    }

    fn uninstall_retrying_failed_items(&self) -> Result<()> {
        uninstall_retrying_failed_items()
    }

    fn deletion_display_progress(&self, current: usize, total: usize, path: &str) {
        deletion_display_progress(current, total, path)
    }

    fn deletion_display_success(&self, path: &str) {
        deletion_display_success(path)
    }

    fn deletion_display_failure(&self, path: &str, error: &str) {
        deletion_display_failure(path, error)
    }

    fn deletion_display_skipped(&self, path: &str) {
        deletion_display_skipped(path)
    }

    fn deletion_display_summary(
        &self,
        success_count: usize,
        failed_count: usize,
        skipped_count: usize,
    ) {
        deletion_display_summary(success_count, failed_count, skipped_count)
    }

    fn download_start(&self, filename: &str, total: Option<u64>) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let pb = match total {
            Some(size) => {
                let pb = ProgressBar::new(size);
                let style = match ProgressStyle::default_bar()
                    .template("{msg}\n[{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                {
                    Ok(s) => s.progress_chars("#>-"),
                    Err(_) => ProgressStyle::default_bar(),
                };
                pb.set_style(style);
                pb.set_message(format!("下载：{}", filename));
                pb
            }
            None => {
                let pb = ProgressBar::new_spinner();
                pb.set_message(format!("下载：{}", filename));
                pb
            }
        };

        let mut bars = match self.bars.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        bars.insert(id, pb);

        id
    }

    fn download_update(&self, id: usize, downloaded: u64) {
        let bars = match self.bars.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(pb) = bars.get(&id) {
            pb.set_position(downloaded);
        }
    }

    fn download_finish(&self, id: usize, message: &str) {
        let mut bars = match self.bars.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(pb) = bars.remove(&id) {
            pb.finish_with_message(message.to_string());
        }
    }

    fn download_version_info_start(&self) -> Result<()> {
        download_version_info_start()
    }

    fn download_version_info_failed(&self, err: &str) -> Result<()> {
        download_version_info_failed(err)
    }

    fn download_version_info_success(&self) -> Result<()> {
        download_version_info_success()
    }

    fn download_version_info_parse_failed(&self, err: &str, snippet: &str) -> Result<()> {
        download_version_info_parse_failed(err, snippet)
    }

    fn download_share_code_start(&self) -> Result<()> {
        download_share_code_start()
    }

    fn download_share_code_failed(&self, err: &str) -> Result<()> {
        download_share_code_failed(err)
    }

    fn download_share_code_success(&self) -> Result<()> {
        download_share_code_success()
    }

    fn download_attempt_github_dll(&self) -> Result<()> {
        download_attempt_github_dll()
    }

    fn download_found_github_asset(&self, name: &str) -> Result<()> {
        download_found_github_asset(name)
    }

    fn download_github_dll_not_found(&self) -> Result<()> {
        download_github_dll_not_found()
    }

    fn download_switch_to_fallback(&self, reason: &str) -> Result<()> {
        download_switch_to_fallback(reason)
    }

    fn download_try_fallback_metamystia(&self) -> Result<()> {
        download_try_fallback_metamystia()
    }

    fn download_resourceex_start(&self) -> Result<()> {
        download_resourceex_start()
    }

    fn download_bepinex_attempt_primary(&self) -> Result<()> {
        download_bepinex_attempt_primary()
    }

    fn download_bepinex_primary_failed(&self, err: &str) -> Result<()> {
        download_bepinex_primary_failed(err)
    }

    fn network_retrying(
        &self,
        op_desc: &str,
        delay_secs: u64,
        attempt: usize,
        attempts: usize,
        err: &str,
    ) -> Result<()> {
        network_retrying(op_desc, delay_secs, attempt, attempts, err)
    }

    fn network_rate_limited(&self, secs: u64) -> Result<()> {
        network_rate_limited(secs)
    }

    fn manager_ask_self_update(&self, current_version: &str, latest_version: &str) -> Result<bool> {
        manager_ask_self_update(current_version, latest_version)
    }

    fn manager_update_starting(&self) -> Result<()> {
        manager_update_starting()
    }

    fn manager_update_failed(&self, err: &str) -> Result<()> {
        manager_update_failed(err)
    }

    fn manager_prompt_manual_update(&self) -> Result<()> {
        manager_prompt_manual_update()
    }
}

// ==================== 通用 UI ====================

fn display_welcome() -> Result<()> {
    let term = Term::stdout();
    term.clear_screen()?;

    println!("{}", style("═".repeat(60)).cyan());
    println!(
        "{}{}（v{}）",
        " ".repeat(7),
        style("MetaMystia Mod 一键安装/升级/卸载工具").cyan().bold(),
        env!("CARGO_PKG_VERSION")
    );

    let user_id = get_user_id();
    print!("{}", " ".repeat(14));
    println!("{}", style(user_id).dim());

    println!("{}", style("═".repeat(60)).cyan());
    println!();

    Ok(())
}

fn display_version(manager_version: Option<&str>) -> Result<()> {
    if let Some(v) = manager_version {
        println!();
        println!("管理工具最新版本：{}", style(v).green());
        if v != env!("CARGO_PKG_VERSION") {
            println!(
                "{}",
                style("升级提醒：您当前使用的不是最新版本，建议升级至最新版本。").yellow()
            );
            println!(
                "手动下载：https://doc.meta-mystia.izakaya.cc/user_guide/how_to_install.html#onclick_install"
            );
        }
        println!();
    }

    println!("{}", style("═".repeat(60)).cyan());
    println!();

    Ok(())
}

fn display_game_running_warning() -> Result<()> {
    println!("请先关闭游戏，然后重新运行本程序。");
    Ok(())
}

fn display_available_updates(dll_available: bool, resourceex_available: bool) -> Result<()> {
    if dll_available || resourceex_available {
        println!("检测到可升级项：");
        if dll_available {
            println!("  • MetaMystia DLL 可升级");
        }
        if resourceex_available {
            println!("  • ResourceExample ZIP 可升级");
        }
        println!();
    }

    Ok(())
}

fn select_operation_mode() -> Result<OperationMode> {
    println!("{}", style("请选择操作模式：").cyan().bold());
    println!();
    println!("  {} 安装 Mod", style("[1]").green());
    println!("  {} 升级 Mod", style("[2]").green());
    println!("  {} 卸载 Mod", style("[3]").green());
    println!("  {} 退出程序", style("[0]").dim());
    println!();

    loop {
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(" 请输入选项")
            .interact_text()?;

        match input.trim() {
            "1" => return Ok(OperationMode::Install),
            "2" => return Ok(OperationMode::Upgrade),
            "3" => return Ok(OperationMode::Uninstall),
            "0" => {
                std::process::exit(0);
            }
            _ => {
                println!();
                println!("{}", style("无效的选项，请输入 0、1、2 或 3").yellow());
                continue;
            }
        }
    }
}

fn blank_line() -> Result<()> {
    println!();
    Ok(())
}

fn wait_for_key() -> Result<()> {
    println!("{}", style("按回车（Enter）键退出...").dim());

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    Ok(())
}

// ==================== 目录相关 UI ====================

fn path_display_steam_found(app_id: u32, name: Option<&str>, path: &Path) -> Result<()> {
    println!(
        "{}",
        style(format!(
            "检测到 Steam 上已安装的游戏：{}（AppID {}）",
            name.unwrap_or("未知"),
            app_id
        ))
        .cyan()
    );
    println!("路径：{}", path.display());
    println!();

    Ok(())
}

fn path_confirm_use_steam_found() -> Result<bool> {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否将此路径作为运行目录并继续？")
        .default(true)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => {
            report_event("UI.SteamPathChoice", Some("yes"));
            Ok(true)
        }
        _ => {
            report_event("UI.SteamPathChoice", Some("no"));
            Ok(false)
        }
    }
}

// ==================== 安装相关 UI ====================

fn install_display_step(step: usize, description: &str) {
    println!();
    println!(
        "{} {}",
        style(format!("[{}/4]", step)).cyan().bold(),
        style(description).cyan()
    );
    println!();
}

fn install_display_version_info(version_info: &VersionInfo) {
    println!("检测到的最新版本：");
    println!("  • MetaMystia DLL：{}", style(&version_info.dll).green());
    println!(
        "  • ResourceExample ZIP：{}",
        style(&version_info.zip).green()
    );

    if let Ok(bep_ver) = version_info.bepinex_version() {
        println!("  • BepInEx：{}", style(bep_ver).green());
    }
}

fn install_warn_existing(
    bepinex_installed: bool,
    metamystia_installed: bool,
    resourceex_installed: bool,
) -> Result<()> {
    println!();
    println!("{}", style("警告：检测到已安装的组件").yellow());
    println!();

    if bepinex_installed {
        println!("  • BepInEx 框架");
    }
    if metamystia_installed {
        println!("  • MetaMystia DLL");
    }
    if resourceex_installed {
        println!("  • ResourceExample ZIP");
    }

    println!();
    println!("继续安装将会执行以下操作：");
    println!("  • 覆盖 BepInEx 框架相关文件（不包含 plugins 文件夹）");
    println!("  • 覆盖 MetaMystia 相关文件");
    println!("  • 安装最新版本的 BepInEx 和 MetaMystia 相关文件");
    println!();

    Ok(())
}

fn install_confirm_overwrite() -> Result<bool> {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否继续安装？")
        .default(false)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => {
            report_event("UI.Install.Confirm", Some("yes"));
            Ok(true)
        }
        _ => {
            report_event("UI.Install.Confirm", Some("no"));
            Ok(false)
        }
    }
}

fn install_ask_install_resourceex() -> Result<bool> {
    println!();
    println!(
        "{}",
        style("ResourceExample ZIP 是 MetaMystia 的可选组件").cyan()
    );
    println!("可以在游戏中加入由 MetaMystia 所提供的额外内容（如：新的稀客、料理和食材等）");
    println!("更多介绍：https://doc.meta-mystia.izakaya.cc/resource_ex/use_resource-ex.html");
    println!();

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否安装 ResourceExample ZIP？")
        .default(true)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => {
            report_event("UI.Install.ResourceEx", Some("yes"));
            Ok(true)
        }
        _ => {
            report_event("UI.Install.ResourceEx", Some("no"));
            Ok(false)
        }
    }
}

fn install_ask_show_bepinex_console() -> Result<bool> {
    println!();

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否在游戏启动时弹出 BepInEx 的控制台窗口用于显示日志？")
        .default(false)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => {
            report_event("UI.Install.BepInExConsole", Some("yes"));
            Ok(true)
        }
        _ => {
            report_event("UI.Install.BepInExConsole", Some("no"));
            Ok(false)
        }
    }
}

fn install_downloads_completed() -> Result<()> {
    println!("所有文件下载完成");
    Ok(())
}

fn install_start_cleanup() -> Result<()> {
    println!();
    println!("正在清理旧版本...");
    Ok(())
}

fn install_cleanup_result(success: usize, failed: usize) -> Result<()> {
    if failed > 0 {
        println!("旧版本删除完成（成功：{}，失败：{}）", success, failed);
        println!("{}", style("  部分文件删除失败，将继续安装").yellow());
    } else {
        println!("旧版本删除完成（清理 {} 项）", success);
    }
    Ok(())
}

fn install_finished(show_bepinex_console: bool) -> Result<()> {
    println!("安装完成！");
    println!("现在可以启动游戏，Mod 将自动加载。");

    if show_bepinex_console {
        println!(
            "{}",
            style("注意：首次启动需要较长时间加载，请您耐心等待。").yellow()
        );
    } else {
        println!(
            "{}",
            style(
                "注意：首次启动需要较长时间加载（可能需要几分钟且没有任何窗口弹出），请您耐心等待。"
            )
            .yellow()
        );
    }

    println!("祝您游戏愉快！");

    Ok(())
}

// ==================== 升级相关 UI ====================

fn upgrade_warn_unparse_version(filename: &str) -> Result<()> {
    println!("{}", style(format!("无法解析版本：{}", filename)).yellow());
    Ok(())
}

fn upgrade_backup_failed(err: &str) -> Result<()> {
    println!("{}", style(format!("备份失败：{}", err)).yellow());
    Ok(())
}

fn upgrade_deleted(path: &Path) -> Result<()> {
    println!("已删除：{}", path.display());
    Ok(())
}

fn upgrade_delete_failed(path: &Path, err: &str) -> Result<()> {
    println!(
        "{}",
        style(format!("删除失败：{}（{}）", path.display(), err)).yellow()
    );
    Ok(())
}

fn upgrade_checking_installed_version() -> Result<()> {
    println!();
    println!("正在检查当前安装的版本...");
    Ok(())
}

fn upgrade_detected_resourceex() -> Result<()> {
    println!("检测到已安装 ResourceExample ZIP");
    Ok(())
}

fn upgrade_display_current_and_latest_dll(current: &str, latest: &str) -> Result<()> {
    println!();
    println!("当前 MetaMystia DLL 版本：{}", style(current).green());
    println!("最新 MetaMystia DLL 版本：{}", style(latest).green());
    Ok(())
}

fn upgrade_no_update_needed() -> Result<()> {
    println!();
    println!("✔  已是最新版本，无需升级！");
    Ok(())
}

fn upgrade_detected_new_dll(current: &str, new: &str) -> Result<()> {
    println!();
    println!("发现新版本 MetaMystia DLL：v{} -> v{}", current, new);
    Ok(())
}

fn upgrade_dll_already_latest() -> Result<()> {
    println!();
    println!("MetaMystia DLL 已是最新版本");
    Ok(())
}

fn upgrade_resourceex_needs_upgrade() -> Result<()> {
    println!("ResourceExample ZIP 需要升级");
    println!();
    Ok(())
}

fn upgrade_downloading_dll() -> Result<()> {
    println!();
    println!("正在下载 MetaMystia DLL...");
    Ok(())
}

fn upgrade_downloading_resourceex() -> Result<()> {
    println!();
    println!("正在下载 ResourceExample ZIP...");
    Ok(())
}

fn upgrade_installing_dll() -> Result<()> {
    println!();
    println!();
    println!("正在安装 MetaMystia DLL...");
    Ok(())
}

fn upgrade_installing_resourceex() -> Result<()> {
    println!("正在安装 ResourceExample ZIP...");
    Ok(())
}

fn upgrade_install_success(path: &Path) -> Result<()> {
    println!("安装成功：{}", path.display());
    Ok(())
}

fn upgrade_cleanup_start() -> Result<()> {
    println!();
    println!("正在清理临时文件...");
    Ok(())
}

fn upgrade_done() -> Result<()> {
    println!();
    println!("✔  升级完成！");
    Ok(())
}

fn upgrade_display_current_and_latest_resourceex(current: &str, latest: &str) -> Result<()> {
    println!("当前 ResourceExample ZIP 版本：{}", style(current).green());
    println!("最新 ResourceExample ZIP 版本：{}", style(latest).green());
    Ok(())
}

// ==================== 卸载相关 UI ====================

fn uninstall_select_uninstall_mode() -> Result<UninstallMode> {
    println!();
    println!("{}", style("请选择卸载模式：").cyan().bold());
    println!();
    println!(
        "  {} {}",
        style("[1]").green(),
        UninstallMode::Light.description()
    );
    println!(
        "  {} {}",
        style("[2]").green(),
        UninstallMode::Full.description()
    );
    println!("  {} 退出程序", style("[0]").dim());
    println!();

    loop {
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(" 请输入选项")
            .interact_text()?;

        match input.trim() {
            "1" => return Ok(UninstallMode::Light),
            "2" => return Ok(UninstallMode::Full),
            "0" => {
                std::process::exit(0);
            }
            _ => {
                println!();
                println!("{}", style("无效的选项，请输入 0、1 或 2").yellow());
                continue;
            }
        }
    }
}

fn uninstall_no_files_found() -> Result<()> {
    println!();
    println!("未找到需要删除的文件，可能已经卸载完成。");
    Ok(())
}

fn uninstall_display_target_files(files: &[PathBuf]) -> Result<()> {
    println!();
    println!("{}", style("即将删除以下文件/文件夹：").yellow().bold());
    println!();

    for file in files {
        let file_type = if file.is_dir() { "📁" } else { "📄" };
        println!("  {} {} {}", style("•").cyan(), file_type, file.display());
    }

    println!();

    Ok(())
}

fn uninstall_confirm_deletion() -> Result<bool> {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否继续？")
        .default(false)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => {
            report_event("UI.Uninstall.Confirm", Some("yes"));
            Ok(true)
        }
        _ => {
            report_event("UI.Uninstall.Confirm", Some("no"));
            Ok(false)
        }
    }
}

fn uninstall_files_in_use_warning() -> Result<()> {
    println!();
    println!(
        "{}",
        style("部分文件被占用，请关闭相关程序后重试。正在短暂等待并自动重试这些文件...").yellow()
    );
    Ok(())
}

fn uninstall_wait_before_retry(delay_secs: u64, attempt: usize, attempts: usize) -> Result<()> {
    println!();
    println!(
        "等待 {} 秒后重试被占用文件（重试 {}/{}）...",
        delay_secs, attempt, attempts
    );
    Ok(())
}

fn uninstall_ask_elevate_permission() -> Result<bool> {
    println!();
    println!(
        "{}",
        style("部分文件删除失败，可能需要管理员权限。").yellow()
    );
    println!();

    let elevate = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否以管理员权限重新运行？")
        .default(true)
        .interact_on_opt(&Term::stdout())?;

    let choice = elevate.unwrap_or(false);

    report_event(
        "UI.Uninstall.Elevate",
        Some(if choice { "yes" } else { "no" }),
    );

    Ok(choice)
}

fn uninstall_restarting_elevated() -> Result<()> {
    println!();
    println!("正在以管理员权限重新启动...");
    Ok(())
}

fn uninstall_ask_retry_failures() -> Result<bool> {
    println!();
    let retry = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否重试失败的项目？")
        .default(true)
        .interact_on_opt(&Term::stdout())?;

    let choice = retry.unwrap_or(false);

    report_event(
        "UI.Uninstall.Retry",
        Some(if choice { "yes" } else { "no" }),
    );

    Ok(choice)
}

fn uninstall_retrying_failed_items() -> Result<()> {
    println!();
    println!("正在重试失败的项目...");
    Ok(())
}

// ==================== 下载相关 UI ====================

fn download_version_info_start() -> Result<()> {
    println!("正在获取版本信息...");
    Ok(())
}

fn download_version_info_failed(err: &str) -> Result<()> {
    println!("{}", style(format!("获取版本信息失败：{}", err)).yellow());
    Ok(())
}

fn download_version_info_success() -> Result<()> {
    println!("获取版本信息成功");
    Ok(())
}

fn download_version_info_parse_failed(err: &str, snippet: &str) -> Result<()> {
    println!(
        "{}",
        style(format!(
            "版本信息解析失败：{}，response snippet：{}",
            err, snippet
        ))
        .yellow()
    );
    Ok(())
}

fn download_share_code_start() -> Result<()> {
    println!("正在获取下载链接...");
    Ok(())
}

fn download_share_code_failed(err: &str) -> Result<()> {
    println!("{}", style(format!("获取下载链接失败：{}", err)).yellow());
    Ok(())
}

fn download_share_code_success() -> Result<()> {
    println!("获取下载链接成功");
    Ok(())
}

fn download_attempt_github_dll() -> Result<()> {
    println!("尝试从 GitHub 下载 MetaMystia DLL...");
    Ok(())
}

fn download_found_github_asset(name: &str) -> Result<()> {
    println!("找到文件：{}", name);
    Ok(())
}

fn download_github_dll_not_found() -> Result<()> {
    println!("{}", style("未找到 MetaMystia DLL 文件").yellow());
    Ok(())
}

fn download_switch_to_fallback(reason: &str) -> Result<()> {
    println!();
    println!("{}", style(reason).yellow());
    Ok(())
}

fn download_try_fallback_metamystia() -> Result<()> {
    println!("尝试从备用源下载 MetaMystia DLL...");
    Ok(())
}

fn download_resourceex_start() -> Result<()> {
    println!("下载 ResourceExample ZIP...");
    Ok(())
}

fn download_bepinex_attempt_primary() -> Result<()> {
    println!("尝试从 bepinex.dev 下载 BepInEx...");
    Ok(())
}

fn download_bepinex_primary_failed(err: &str) -> Result<()> {
    println!("{}", style(err).yellow());
    Ok(())
}

// ==================== 删除相关 UI ====================

fn deletion_display_progress(current: usize, total: usize, path: &str) {
    println!(
        "{} [{}/{}] {}",
        style("正在删除").cyan(),
        current,
        total,
        path
    );
}

fn deletion_display_success(path: &str) {
    println!("  {} {}", style("✔ ").green(), style(path).dim());
}

fn deletion_display_failure(path: &str, error: &str) {
    println!(
        "  {} {} - {}",
        style("✗ ").red(),
        style(path).dim(),
        style(error).red()
    );
}

fn deletion_display_skipped(path: &str) {
    println!("  {} {}", style("○ ").dim(), style(path).dim());
}

fn deletion_display_summary(success_count: usize, failed_count: usize, skipped_count: usize) {
    println!();
    println!("删除成功：{} 项", style(success_count).green());

    if skipped_count > 0 {
        println!(
            "  {} 跳过：{} 项（文件不存在）",
            style("○").dim(),
            style(skipped_count).dim()
        );
    }

    if failed_count > 0 {
        println!("  删除失败：{} 项", style(failed_count).red());
    } else {
        println!();
        println!("✔  卸载完成！");
    }
}

// ==================== 网络相关 UI ====================

fn network_retrying(
    op_desc: &str,
    delay_secs: u64,
    attempt: usize,
    attempts: usize,
    err: &str,
) -> Result<()> {
    println!(
        "{}",
        style(format!(
            "{}失败，{} 秒后重试...（重试 {}/{}）",
            op_desc, delay_secs, attempt, attempts
        ))
        .yellow()
    );
    println!("{}", style(format!("错误：{}", err)).yellow());
    println!(
        "{}",
        style("提醒：若重试次数耗尽后仍失败，将自动切换至备用源下载，请耐心等待。").dim()
    );
    Ok(())
}

fn network_rate_limited(secs: u64) -> Result<()> {
    println!(
        "{}",
        style(format!(
            "检测到限流，服务器指定 Retry-After={} 秒，将等待后重试...",
            secs
        ))
        .yellow()
    );
    Ok(())
}

// ==================== 自升级相关 UI ====================

fn manager_ask_self_update(current_version: &str, latest_version: &str) -> Result<bool> {
    println!(
        "管理工具可以升级：{} -> {}",
        style(current_version).green(),
        style(latest_version).green()
    );
    println!();

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否立即升级？")
        .default(false)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => {
            report_event("UI.SelfUpdate.Choice", Some("yes"));
            println!();
            Ok(true)
        }
        _ => {
            report_event("UI.SelfUpdate.Choice", Some("no"));
            println!();
            Ok(false)
        }
    }
}

fn manager_update_starting() -> Result<()> {
    println!();
    println!("正在启动升级脚本，请稍候...");
    println!();
    Ok(())
}

fn manager_update_failed(err: &str) -> Result<()> {
    println!();
    println!("{}", style(format!("升级失败：{}", err)).red());
    println!("请手动下载并升级管理工具。");
    println!();
    Ok(())
}

fn manager_prompt_manual_update() -> Result<()> {
    println!();
    println!("无法向当前运行目录写入文件，请手动下载并升级管理工具。");
    println!();
    Ok(())
}
