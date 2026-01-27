use crate::config::UninstallMode;
use crate::error::Result;
use crate::model::VersionInfo;

use console::{Term, style};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 显示欢迎信息
pub fn display_welcome() -> Result<()> {
    let term = Term::stdout();
    term.clear_screen()?;

    println!("{}", style("═".repeat(60)).cyan());
    println!(
        "{}{}（v{}）",
        " ".repeat(7),
        style("MetaMystia Mod 一键安装/升级/卸载工具").cyan().bold(),
        env!("CARGO_PKG_VERSION")
    );
    println!("{}", style("═".repeat(60)).cyan());
    println!();

    Ok(())
}

/// 显示游戏正在运行的警告
pub fn display_game_running_warning() -> Result<()> {
    println!("请先关闭游戏，然后重新运行本程序。");
    Ok(())
}

/// 选择卸载模式
pub fn select_uninstall_mode() -> Result<UninstallMode> {
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

/// 显示将要删除的文件列表
pub fn display_target_files(files: &[PathBuf]) -> Result<()> {
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

/// 确认删除操作
pub fn confirm_deletion() -> Result<bool> {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否继续？")
        .default(false)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => Ok(true),
        _ => Ok(false),
    }
}

/// 显示删除进度
pub fn display_deletion_progress(current: usize, total: usize, path: &str) {
    println!(
        "{} [{}/{}] {}",
        style("正在删除").cyan(),
        current,
        total,
        path
    );
}

/// 显示删除成功
pub fn display_success(path: &str) {
    println!("  {} {}", style("✔ ").green(), style(path).dim());
}

/// 显示删除失败
pub fn display_failure(path: &str, error: &str) {
    println!(
        "  {} {} - {}",
        style("✗ ").red(),
        style(path).dim(),
        style(error).red()
    );
}

/// 显示删除跳过（文件不存在）
pub fn display_skipped(path: &str) {
    println!("  {} {}", style("○ ").dim(), style(path).dim());
}

/// 询问是否重试失败的项目
pub fn ask_retry_failures() -> Result<bool> {
    println!();
    let retry = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否重试失败的项目？")
        .default(true)
        .interact_on_opt(&Term::stdout())?;

    Ok(retry.unwrap_or(false))
}

/// 询问是否以管理员权限重试
pub fn ask_elevate_permission() -> Result<bool> {
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

    Ok(elevate.unwrap_or(false))
}

/// 显示操作摘要
pub fn display_summary(success_count: usize, failed_count: usize, skipped_count: usize) {
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

/// 等待用户按键
pub fn wait_for_key() -> Result<()> {
    println!("{}", style("按回车退出...").dim());

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    Ok(())
}

/// UI 抽象接口
pub trait Ui: Send + Sync {
    fn display_welcome(&self) -> Result<()>;
    fn display_game_running_warning(&self) -> Result<()>;
    fn select_uninstall_mode(&self) -> Result<UninstallMode>;
    fn display_target_files(&self, files: &[PathBuf]) -> Result<()>;
    fn confirm_deletion(&self) -> Result<bool>;
    fn display_deletion_progress(&self, current: usize, total: usize, path: &str);
    fn display_success(&self, path: &str);
    fn display_failure(&self, path: &str, error: &str);
    fn display_skipped(&self, path: &str);
    fn ask_retry_failures(&self) -> Result<bool>;
    fn ask_elevate_permission(&self) -> Result<bool>;
    fn display_summary(&self, success_count: usize, failed_count: usize, skipped_count: usize);
    fn wait_for_key(&self) -> Result<()>;

    // 通用输出
    fn message(&self, text: &str) -> Result<()>;
    fn warn(&self, text: &str) -> Result<()>;
    fn error(&self, text: &str) -> Result<()>;

    // 安装相关
    fn select_operation_mode(&self) -> Result<OperationMode>;
    fn display_step(&self, step: usize, description: &str);
    fn display_version_info(&self, version_info: &VersionInfo);
    fn confirm_overwrite(&self) -> Result<bool>;
    fn ask_install_resourceex(&self) -> Result<bool>;

    // 下载进度相关
    /// 开始一个下载任务，返回一个用于后续更新的 id
    fn download_start(&self, filename: &str, total: Option<u64>) -> usize;
    /// 更新下载进度（传入 download_start 返回的 id）
    fn download_update(&self, id: usize, downloaded: u64);
    /// 完成下载任务（并显示完成信息）
    fn download_finish(&self, id: usize, message: &str);
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

    fn display_game_running_warning(&self) -> Result<()> {
        display_game_running_warning()
    }

    fn select_uninstall_mode(&self) -> Result<UninstallMode> {
        select_uninstall_mode()
    }

    fn display_target_files(&self, files: &[PathBuf]) -> Result<()> {
        display_target_files(files)
    }

    fn confirm_deletion(&self) -> Result<bool> {
        confirm_deletion()
    }

    fn ask_retry_failures(&self) -> Result<bool> {
        ask_retry_failures()
    }

    fn ask_elevate_permission(&self) -> Result<bool> {
        ask_elevate_permission()
    }

    fn display_deletion_progress(&self, current: usize, total: usize, path: &str) {
        display_deletion_progress(current, total, path)
    }

    fn display_success(&self, path: &str) {
        display_success(path)
    }

    fn display_failure(&self, path: &str, error: &str) {
        display_failure(path, error)
    }

    fn display_skipped(&self, path: &str) {
        display_skipped(path)
    }

    fn display_summary(&self, success_count: usize, failed_count: usize, skipped_count: usize) {
        display_summary(success_count, failed_count, skipped_count)
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

    fn select_operation_mode(&self) -> Result<OperationMode> {
        select_operation_mode()
    }

    fn display_step(&self, step: usize, description: &str) {
        display_step(step, description)
    }

    fn display_version_info(&self, version_info: &VersionInfo) {
        display_version_info(version_info)
    }

    fn confirm_overwrite(&self) -> Result<bool> {
        confirm_overwrite()
    }

    fn ask_install_resourceex(&self) -> Result<bool> {
        ask_install_resourceex()
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

        let mut bars = self.bars.lock().unwrap();
        bars.insert(id, pb);

        id
    }

    fn download_update(&self, id: usize, downloaded: u64) {
        let bars = self.bars.lock().unwrap();
        if let Some(pb) = bars.get(&id) {
            pb.set_position(downloaded);
        }
    }

    fn download_finish(&self, id: usize, message: &str) {
        let mut bars = self.bars.lock().unwrap();
        if let Some(pb) = bars.remove(&id) {
            pb.finish_with_message(message.to_string());
        }
    }
}

// ==================== 安装相关 UI ====================

/// 选择操作模式（安装或卸载）
pub fn select_operation_mode() -> Result<OperationMode> {
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

/// 显示安装步骤
pub fn display_step(step: usize, description: &str) {
    println!();
    println!(
        "{} {}",
        style(format!("[{}/4]", step)).cyan().bold(),
        style(description).cyan()
    );
    println!();
}

/// 显示版本信息
pub fn display_version_info(version_info: &VersionInfo) {
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

/// 确认覆盖安装
pub fn confirm_overwrite() -> Result<bool> {
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否继续安装？")
        .default(false)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => Ok(true),
        _ => Ok(false),
    }
}

/// 询问是否安装 ResourceExample ZIP
pub fn ask_install_resourceex() -> Result<bool> {
    println!();
    println!(
        "{}",
        style("ResourceExample ZIP 是 MetaMystia 的可选组件").cyan()
    );
    println!("可以在游戏中加入由 MetaMystia 所提供的额外内容（如：新的稀客）");
    println!();

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(" 是否安装 ResourceExample ZIP？")
        .default(true)
        .interact_on_opt(&Term::stdout())?;

    match confirmed {
        Some(true) => Ok(true),
        _ => Ok(false),
    }
}

/// 操作模式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationMode {
    Install,
    Upgrade,
    Uninstall,
}
