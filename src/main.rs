mod config;
mod downloader;
mod env_check;
mod error;
mod extractor;
mod file_ops;
mod installer;
mod model;
mod net;
mod permission;
mod temp_dir;
mod ui;
mod uninstaller;
mod upgrader;

use crate::config::GAME_EXECUTABLE;
use crate::downloader::Downloader;
use crate::env_check::{check_game_directory, check_game_running};
use crate::error::{ManagerError, Result};
use crate::installer::Installer;
use crate::ui::OperationMode::*;
use crate::ui::{ConsoleUI, Ui};
use crate::uninstaller::Uninstaller;
use crate::upgrader::Upgrader;

use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let console_ui = ConsoleUI::new();

    if !cfg!(windows) {
        let _ = console_ui.error("错误：仅支持 Windows 平台");
        console_ui.wait_for_key().ok();
        return std::process::ExitCode::from(1);
    }

    match run(&console_ui) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            let _ = console_ui.error(&format!("错误：{}", e));
            console_ui.wait_for_key().ok();
            std::process::ExitCode::from(1)
        }
    }
}

fn run(ui: &dyn Ui) -> Result<()> {
    // 1. 显示欢迎信息
    ui.display_welcome()?;
    let mut manager_version: Option<String> = None;
    if let Ok(downloader) = Downloader::new(ui) {
        match downloader.get_version_info() {
            Ok(vi) => {
                manager_version = Some(vi.manager);
            }
            Err(e) => {
                let _ = ui.message(&format!("无法获取版本信息：{}", e));
            }
        }
    }
    ui.display_version(manager_version.as_deref())?;

    // 2. 目录环境检查
    let game_root = match check_game_directory() {
        Ok(path) => path,
        Err(e) => {
            ui.message(&format!("当前目录：{}", std::env::current_dir()?.display()))?;
            ui.message(&format!(
                "请在游戏根目录（包含 {} 的文件夹）下运行本程序。",
                GAME_EXECUTABLE
            ))?;
            return Err(e);
        }
    };

    // 3. 游戏进程检查
    if check_game_running()? {
        ui.display_game_running_warning()?;
        return Err(ManagerError::GameRunning);
    }

    // 4. 选择操作模式
    let operation = ui.select_operation_mode()?;

    match operation {
        Install => run_install(game_root.clone(), ui),
        Upgrade => run_upgrade(game_root.clone(), ui),
        Uninstall => run_uninstall(game_root.clone(), ui),
    }
}

fn run_install(game_root: PathBuf, ui: &dyn Ui) -> Result<()> {
    // 创建安装器
    let installer = Installer::new(game_root.clone(), ui)?;

    // 检查是否已安装组件
    let bepinex_installed = installer.check_bepinex_installed();
    let metamystia_installed = installer.check_metamystia_installed();
    let resourceex_installed = installer.check_resourceex_installed();
    let has_installed = bepinex_installed || metamystia_installed || resourceex_installed;

    if has_installed {
        ui.message("")?;
        ui.warn("警告：检测到已安装的组件")?;
        ui.message("")?;

        if bepinex_installed {
            ui.message("  • BepInEx 框架")?;
        }
        if metamystia_installed {
            ui.message("  • MetaMystia DLL")?;
        }
        if resourceex_installed {
            ui.message("  • ResourceExample ZIP")?;
        }

        ui.message("")?;
        ui.message("继续安装将会执行以下操作：")?;
        ui.message("  • 覆盖 BepInEx 框架相关文件（不包含 plugins 文件夹）")?;
        ui.message("  • 覆盖 MetaMystia 相关文件")?;
        ui.message("  • 安装最新版本的 BepInEx 和 MetaMystia 相关文件")?;
        ui.message("")?;

        let confirmed = ui.install_confirm_overwrite()?;
        if !confirmed {
            return Err(ManagerError::UserCancelled);
        }
    }

    // 执行安装
    installer.install(has_installed)?;

    ui.wait_for_key()?;
    Ok(())
}

fn run_upgrade(game_root: PathBuf, ui: &dyn Ui) -> Result<()> {
    // 创建升级器
    let upgrader = Upgrader::new(game_root, ui)?;

    // 执行升级
    upgrader.upgrade()?;

    ui.wait_for_key()?;
    Ok(())
}

fn run_uninstall(game_root: PathBuf, ui: &dyn Ui) -> Result<()> {
    // 创建卸载器
    let uninstaller = Uninstaller::new(game_root, ui)?;

    // 执行卸载
    uninstaller.uninstall()?;

    ui.wait_for_key()?;
    Ok(())
}
