use crate::config::UninstallMode;
use crate::downloader::Downloader;
use crate::error::{ManagerError, Result};
use crate::extractor::Extractor;
use crate::file_ops::{count_results, execute_deletion, glob_matches};
use crate::temp_dir::create_temp_dir_with_guard;
use crate::ui::Ui;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 安装管理器
pub struct Installer<'a> {
    game_root: PathBuf,
    downloader: Downloader<'a>,
    ui: &'a dyn Ui,
}

impl<'a> Installer<'a> {
    pub fn new(game_root: PathBuf, ui: &'a dyn Ui) -> Result<Self> {
        let downloader = Downloader::new(ui)?;
        Ok(Self {
            game_root,
            downloader,
            ui,
        })
    }

    /// 检查是否已安装 MetaMystia DLL
    pub fn check_metamystia_installed(&self) -> bool {
        let metamystia_pattern = self
            .game_root
            .join("BepInEx")
            .join("plugins")
            .join("MetaMystia-*.dll");

        let matches = glob_matches(&metamystia_pattern);
        !matches.is_empty()
    }

    /// 检查是否已安装 ResourceExample ZIP
    pub fn check_resourceex_installed(&self) -> bool {
        let resourceex_dir = self.game_root.join("ResourceEx");
        resourceex_dir.exists() && resourceex_dir.is_dir() && {
            let resourceex_pattern = resourceex_dir.join("ResourceExample-*.zip");
            let matches = glob_matches(&resourceex_pattern);
            !matches.is_empty()
        }
    }

    /// 检查是否已安装 BepInEx
    pub fn check_bepinex_installed(&self) -> bool {
        let bepinex_dir = self.game_root.join("BepInEx");
        bepinex_dir.exists() && bepinex_dir.is_dir() && {
            let core_pattern = bepinex_dir.join("core").join("BepInEx.Core.dll");
            let matches = glob_matches(&core_pattern);
            !matches.is_empty()
        }
    }

    /// 执行安装前的清理：全量卸载但保留 BepInEx/plugins（除了 MetaMystia DLL）
    fn execute_install_cleanup(game_root: &Path, ui: &dyn Ui) -> Result<(usize, usize)> {
        let mut targets = Vec::new();
        let mut seen = HashSet::new();

        // 添加路径到删除列表
        let mut push = |p: PathBuf| {
            if seen.insert(p.clone()) {
                targets.push(p);
            }
        };

        // 1. 删除 BepInEx 目录下的所有项目（跳过 plugins）
        let bepinex_dir = game_root.join("BepInEx");
        if bepinex_dir.exists() {
            for entry in
                std::fs::read_dir(&bepinex_dir).map_err(|e| ManagerError::Io(e.to_string()))?
            {
                let entry = entry.map_err(|e| ManagerError::Io(e.to_string()))?;
                let path = entry.path();
                let name = entry.file_name();

                if name.to_string_lossy().eq_ignore_ascii_case("plugins") {
                    continue;
                }

                push(path);
            }
        }

        // 2. 删除 plugins 目录中的 MetaMystia DLL
        let plugins_dir = bepinex_dir.join("plugins");
        if plugins_dir.exists() {
            let metamystia_pattern = plugins_dir.join("MetaMystia-*.dll");
            for entry in glob_matches(&metamystia_pattern) {
                push(entry);
            }
        }

        // 3. 删除 ResourceEx 目录中的 ResourceExample ZIP
        let resourceex_dir = game_root.join("ResourceEx");
        if resourceex_dir.exists() {
            let resourceex_pattern = resourceex_dir.join("ResourceExample-*.zip");
            for entry in glob_matches(&resourceex_pattern) {
                push(entry);
            }
        }

        // 4. 删除完全卸载模式中的其他文件
        let full_targets = UninstallMode::Full.get_targets();
        for &(pattern, is_dir) in full_targets {
            if pattern == "BepInEx" || pattern == "ResourceEx" {
                continue;
            }

            let target_path = game_root.join(pattern);

            if is_dir {
                if target_path.exists() {
                    push(target_path);
                }
            } else if pattern.contains('*') {
                for entry in glob_matches(&target_path) {
                    push(entry);
                }
            } else if target_path.exists() {
                push(target_path);
            }
        }

        let results = execute_deletion(&targets, ui);
        let (success, failed, _skipped) = count_results(&results);

        Ok((success, failed))
    }

    /// 执行安装流程
    pub fn install(&self, cleanup_before_deploy: bool) -> Result<()> {
        // 1. 获取版本信息
        self.ui.install_display_step(1, "获取版本信息");
        let version_info = self.downloader.get_version_info()?;
        self.ui.install_display_version_info(&version_info);

        // 2. 获取分享码
        self.ui.install_display_step(2, "获取下载链接");
        let share_code = self.downloader.get_share_code()?;

        // 2.5. 询问是否安装 ResourceEx
        let install_resourceex = if cleanup_before_deploy {
            let resourceex_pattern = self
                .game_root
                .join("ResourceEx")
                .join("ResourceExample-*.zip");
            let resourceex_exists = !glob_matches(&resourceex_pattern).is_empty();
            if resourceex_exists {
                true
            } else {
                self.ui.install_ask_install_resourceex()?
            }
        } else {
            self.ui.install_ask_install_resourceex()?
        };

        // 3. 创建临时下载目录
        let (temp_dir, _temp_guard) = create_temp_dir_with_guard(&self.game_root)
            .map_err(|e| ManagerError::Io(format!("创建临时目录失败：{}", e)))?;

        // 4. 下载文件
        self.ui.install_display_step(3, "下载必要文件");

        // 下载 BepInEx
        let bepinex_path = temp_dir.join(version_info.bepinex_filename()?);
        self.downloader
            .download_bepinex(&version_info, &bepinex_path)?;

        // 下载 MetaMystia DLL
        let dll_path = temp_dir.join(version_info.metamystia_filename());
        self.downloader
            .download_metamystia(&share_code, &version_info, &dll_path)?;

        // 下载 ResourceExample ZIP
        let resourceex_path = if install_resourceex {
            let path = temp_dir.join(version_info.resourceex_filename());
            self.downloader
                .download_resourceex(&share_code, &version_info, &path)?;
            Some(path)
        } else {
            None
        };

        self.ui.install_downloads_completed()?;

        // 5. 在安装前清理旧版本
        if cleanup_before_deploy {
            self.ui.install_start_cleanup()?;
            let (success, failed) = Self::execute_install_cleanup(&self.game_root, self.ui)?;
            self.ui.install_cleanup_result(success, failed)?;
        }

        // 6. 安装文件
        self.ui.install_display_step(4, "安装文件");

        // 检查 BepInEx 是否存在（用于决定是否跳过 plugins）
        let bepinex_dir = self.game_root.join("BepInEx");
        let bepinex_exists = bepinex_dir.exists();

        // 安装 BepInEx（如果之前存在则保留 plugins 目录）
        Extractor::deploy_bepinex(&bepinex_path, &self.game_root, bepinex_exists)?;

        // 安装 MetaMystia DLL
        Extractor::deploy_metamystia(&dll_path, &self.game_root)?;

        // 安装 ResourceExample ZIP
        if let Some(ref path) = resourceex_path {
            Extractor::deploy_resourceex(path, &self.game_root)?;
        }

        self.ui.install_finished()?;

        Ok(())
    }
}
