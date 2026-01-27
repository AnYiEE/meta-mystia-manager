use crate::downloader::Downloader;
use crate::error::{ManagerError, Result};
use crate::file_ops::{
    atomic_rename_or_copy, backup_paths_with_index, glob_matches, remove_glob_files,
};
use crate::temp_dir::create_temp_dir_with_guard;
use crate::ui::Ui;

use semver::Version;
use std::fs;
use std::path::PathBuf;

/// 升级管理器
pub struct Upgrader<'a> {
    game_root: PathBuf,
    downloader: Downloader<'a>,
    ui: &'a dyn Ui,
}

impl<'a> Upgrader<'a> {
    pub fn new(game_root: PathBuf, ui: &'a dyn Ui) -> Result<Self> {
        let downloader = Downloader::new(ui)?;
        Ok(Self {
            game_root,
            downloader,
            ui,
        })
    }

    fn parse_version(name: &str, prefix: &str, suffix: &str) -> Option<Version> {
        if let Some(s) = name.strip_prefix(prefix)
            && let Some(ver_part) = s.strip_suffix(suffix)
            && let Ok(v) = Version::parse(ver_part)
        {
            return Some(v);
        }
        None
    }

    fn consolidate_installed_dlls(&self) -> Result<Option<(String, PathBuf)>> {
        let plugins_dir = self.game_root.join("BepInEx").join("plugins");

        if !plugins_dir.exists() {
            return Ok(None);
        }

        let mut parsed: Vec<(Version, PathBuf)> = Vec::new();
        let mut unparsed: Vec<PathBuf> = Vec::new();

        for path in glob_matches(&plugins_dir.join("MetaMystia-*.dll")).into_iter() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(v) = Self::parse_version(filename, "MetaMystia-v", ".dll") {
                    parsed.push((v, path.clone()));
                } else {
                    self.ui.warn(&format!("无法解析版本：{}", filename))?;
                    unparsed.push(path.clone());
                }
            }
        }

        if parsed.is_empty() && unparsed.is_empty() {
            return Ok(None);
        }

        let latest: PathBuf;
        let latest_version_str: String;

        if !parsed.is_empty() {
            parsed.sort_by(|a, b| a.0.cmp(&b.0));

            let (v, p) = parsed.last().unwrap();
            latest = p.clone();
            latest_version_str = v.to_string();

            let to_backup: Vec<PathBuf> =
                parsed.into_iter().rev().skip(1).map(|(_, p)| p).collect();

            let results = backup_paths_with_index(&to_backup, "dll.old");
            for res in results {
                match res {
                    Ok(_backup) => (),
                    Err(e) => self.ui.warn(&format!("备份失败：{}", e))?,
                }
            }
        } else {
            if unparsed.is_empty() {
                return Ok(None);
            }

            unparsed.sort();

            latest = unparsed.last().unwrap().clone();
            latest_version_str = latest
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let to_backup: Vec<PathBuf> = unparsed.into_iter().rev().skip(1).collect();

            let results = backup_paths_with_index(&to_backup, "dll.old");
            for res in results {
                match res {
                    Ok(_backup) => (),
                    Err(e) => self.ui.warn(&format!("备份失败：{}", e))?,
                }
            }
        }

        Ok(Some((latest_version_str, latest)))
    }

    fn cleanup_old_files(&self) -> Result<()> {
        let plugins_dir = self.game_root.join("BepInEx").join("plugins");
        if plugins_dir.exists() {
            let pattern = plugins_dir.join("MetaMystia-v*.dll.old*");
            let result = remove_glob_files(&pattern);
            for removed in result.removed.iter() {
                self.ui.message(&format!("已删除：{}", removed.display()))?;
            }
            for (path, err) in result.failed.into_iter() {
                self.ui
                    .warn(&format!("删除失败：{}（{}）", path.display(), err))?;
            }
        }

        let resourceex_dir = self.game_root.join("ResourceEx");
        if resourceex_dir.exists() {
            let pattern = resourceex_dir.join("ResourceExample-v*.zip.old*");
            let result = remove_glob_files(&pattern);
            for removed in result.removed.iter() {
                self.ui.message(&format!("已删除：{}", removed.display()))?;
            }
            for (path, err) in result.failed.into_iter() {
                self.ui
                    .warn(&format!("删除失败：{}（{}）", path.display(), err))?;
            }
        }

        Ok(())
    }

    /// 执行升级
    pub fn upgrade(&self) -> Result<()> {
        // 1. 查找当前安装的版本
        self.ui.message("")?;
        self.ui.message("正在检查当前安装的版本...")?;

        let (current_version, _current_dll_path) = match self.consolidate_installed_dlls()? {
            Some((version, path)) => (version, path),
            None => {
                return Err(ManagerError::Other(
                    "未找到已安装的 MetaMystia Mod，请先使用安装功能。".to_string(),
                ));
            }
        };

        // 检查是否已安装 ResourceEx
        let resourceex_dir = self.game_root.join("ResourceEx");
        let has_resourceex = resourceex_dir.exists()
            && resourceex_dir.is_dir()
            && !glob_matches(&resourceex_dir.join("ResourceExample-v*.zip")).is_empty();

        if has_resourceex {
            self.ui.message("检测到已安装 ResourceExample ZIP")?;
        }

        // 2. 获取最新版本信息
        self.ui.message("")?;
        let version_info = self.downloader.get_version_info()?;
        let new_version = &version_info.dll;

        self.ui.message("")?;
        self.ui
            .message(&format!("当前 MetaMystia DLL 版本：v{}", current_version))?;
        self.ui
            .message(&format!("最新 MetaMystia DLL 版本：v{}", new_version))?;

        // 检查 MetaMystia DLL 是否需要升级
        let dll_needs_upgrade = current_version != *new_version;

        // 检查 ResourceExample ZIP 是否需要升级
        let mut resourceex_needs_upgrade = false;
        if has_resourceex {
            // 查找当前安装的 ResourceExample ZIP 版本
            let current_resourceex_pattern = resourceex_dir.join("ResourceExample-v*.zip");
            let mut current_resourceex_version = None;

            for entry in glob_matches(&current_resourceex_pattern) {
                if let Some(filename) = entry.file_name().and_then(|n| n.to_str()) {
                    if filename.ends_with(".old") {
                        continue;
                    }
                    if let Some(version_part) =
                        Self::parse_version(filename, "ResourceExample-v", ".zip")
                    {
                        current_resourceex_version = Some(version_part.to_string());
                        break;
                    }
                }
            }

            if let Some(current_ver) = current_resourceex_version {
                self.ui
                    .message(&format!("当前 ResourceExample ZIP 版本：v{}", current_ver))?;
                self.ui.message(&format!(
                    "最新 ResourceExample ZIP 版本：v{}",
                    version_info.zip
                ))?;
                resourceex_needs_upgrade = current_ver != version_info.zip;
            }
        }

        if !dll_needs_upgrade && !resourceex_needs_upgrade {
            self.ui.message("")?;
            self.ui.message("✔  已是最新版本，无需升级！")?;
            return Ok(());
        }

        // 显示升级信息
        self.ui.message("")?;
        if dll_needs_upgrade {
            self.ui.message(&format!(
                "发现新版本 MetaMystia DLL：v{} -> v{}",
                current_version, new_version
            ))?;
        } else {
            self.ui.message("MetaMystia DLL 已是最新版本")?;
        }
        if resourceex_needs_upgrade {
            self.ui.message("ResourceExample ZIP 需要升级")?;
        }

        // 3. 获取分享码
        let share_code = self.downloader.get_share_code()?;

        // 4. 下载新版本
        self.ui.message("")?;
        self.ui.message("正在下载 MetaMystia DLL...")?;

        let (temp_dir, _temp_guard) = create_temp_dir_with_guard(&self.game_root)
            .map_err(|e| ManagerError::Io(format!("创建临时目录失败：{}", e)))?;

        // 下载 DLL（仅当需要升级时）
        let temp_dll_path = if dll_needs_upgrade {
            let new_dll_filename = format!("MetaMystia-v{}.dll", new_version);
            let path = temp_dir.join(&new_dll_filename);

            self.downloader
                .download_metamystia(&share_code, &version_info, &path)?;

            Some((path, new_dll_filename))
        } else {
            None
        };

        // 下载 ResourceExample ZIP（仅当已安装且需要升级时）
        let temp_resourceex_path = if has_resourceex && resourceex_needs_upgrade {
            let resourceex_filename = version_info.resourceex_filename();
            let path = temp_dir.join(&resourceex_filename);

            self.ui.message("")?;
            self.ui.message("正在下载 ResourceExample ZIP...")?;

            self.downloader
                .download_resourceex(&share_code, &version_info, &path)?;

            Some((path, resourceex_filename))
        } else {
            None
        };

        // 5. 安装新版本 MetaMystia DLL（仅当需要升级时）
        if let Some((temp_path, filename)) = temp_dll_path {
            let plugins_dir = self.game_root.join("BepInEx").join("plugins");
            let mut backup_paths: Vec<PathBuf> = Vec::new();

            let old_dll_pattern = plugins_dir.join("MetaMystia-v*.dll");
            let mut to_backup: Vec<PathBuf> = Vec::new();
            for old_entry in glob_matches(&old_dll_pattern) {
                if let Some(old_filename) = old_entry.file_name().and_then(|n| n.to_str()) {
                    if old_filename == filename || old_filename.ends_with(".old") {
                        continue;
                    }
                }
                to_backup.push(old_entry);
            }

            for res in backup_paths_with_index(&to_backup, "dll.old") {
                match res {
                    Ok(backup_path) => backup_paths.push(backup_path),
                    Err(e) => self.ui.warn(&format!("备份失败：{}", e))?,
                }
            }

            self.ui.message("")?;
            self.ui.message("正在安装 MetaMystia DLL...")?;

            let new_dll_path = plugins_dir.join(&filename);

            if !plugins_dir.exists() {
                fs::create_dir_all(&plugins_dir)
                    .map_err(|e| ManagerError::Io(format!("创建 plugins 目录失败：{}", e)))?;
            }

            let tmp_new = new_dll_path.with_extension("dll.tmp");
            fs::copy(&temp_path, &tmp_new)
                .map_err(|e| ManagerError::Io(format!("复制临时文件失败：{}", e)))?;
            atomic_rename_or_copy(&tmp_new, &new_dll_path)
                .map_err(|e| ManagerError::Io(format!("安装新版本失败：{}", e)))?;

            self.ui
                .message(&format!("安装成功：{}", new_dll_path.display()))?;

            if backup_paths.is_empty() {
                None
            } else {
                Some(backup_paths)
            }
        } else {
            None
        };

        // 6. 安装 ResourceExample ZIP（仅当需要升级时）
        if let Some((temp_path, filename)) = temp_resourceex_path {
            let old_resourceex_pattern = resourceex_dir.join("ResourceExample-v*.zip");
            let mut to_backup: Vec<PathBuf> = Vec::new();
            for old_entry in glob_matches(&old_resourceex_pattern) {
                if let Some(old_filename) = old_entry.file_name().and_then(|n| n.to_str()) {
                    if old_filename == filename || old_filename.ends_with(".old") {
                        continue;
                    }
                }
                to_backup.push(old_entry);
            }

            for res in backup_paths_with_index(&to_backup, "zip.old") {
                match res {
                    Ok(_) => (),
                    Err(e) => self.ui.warn(&format!("备份失败：{}", e))?,
                }
            }

            self.ui.message("")?;
            self.ui.message("正在安装 ResourceExample ZIP...")?;

            let new_zip_path = resourceex_dir.join(&filename);
            let tmp_new = new_zip_path.with_extension("zip.tmp");
            fs::copy(&temp_path, &tmp_new)
                .map_err(|e| ManagerError::Io(format!("复制临时文件失败：{}", e)))?;
            atomic_rename_or_copy(&tmp_new, &new_zip_path)
                .map_err(|e| ManagerError::Io(format!("安装新版本失败：{}", e)))?;

            self.ui
                .message(&format!("安装成功：{}", new_zip_path.display()))?;
        }

        // 7. 清理临时文件
        self.ui.message("")?;
        self.ui.message("正在清理临时文件...")?;
        self.cleanup_old_files()?;

        self.ui.message("")?;
        self.ui.message("✔  升级完成！")?;

        Ok(())
    }
}
