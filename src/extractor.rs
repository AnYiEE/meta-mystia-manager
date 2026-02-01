use crate::error::{ManagerError, Result};
use crate::file_ops::atomic_rename_or_copy;
use crate::metrics::report_event;

use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

/// 文件解压器
pub struct Extractor;

impl Extractor {
    /// 检查 ZIP 路径是否安全
    fn is_safe_path(path: &Path) -> bool {
        if path.is_absolute() {
            return false;
        }
        !path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    }

    /// 解压文件到指定目录
    pub fn extract_zip_safe(zip_path: &Path, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        Self::extract_zip_safe_with_exclusions(zip_path, dest_dir, &[])
    }

    /// 解压文件到指定目录（支持排除路径）
    pub fn extract_zip_safe_with_exclusions(
        zip_path: &Path,
        dest_dir: &Path,
        exclude_patterns: &[&str],
    ) -> Result<Vec<PathBuf>> {
        report_event("Extract.Start", Some(&zip_path.to_string_lossy()));

        let file = match std::fs::File::open(zip_path) {
            Ok(f) => f,
            Err(e) => {
                report_event(
                    "Extract.Failed",
                    Some(&format!(
                        "open:{zip}:{err}",
                        zip = zip_path.to_string_lossy(),
                        err = e
                    )),
                );
                return Err(ManagerError::from(std::io::Error::new(
                    e.kind(),
                    format!("打开 ZIP 失败：{}", e),
                )));
            }
        };

        let mut archive = match ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                report_event(
                    "Extract.Failed",
                    Some(&format!(
                        "read_zip_failed:{zip}:{err}",
                        zip = zip_path.to_string_lossy(),
                        err = e
                    )),
                );
                return Err(ManagerError::ExtractFailed(format!("读取 ZIP 失败：{}", e)));
            }
        };

        let mut extracted_files = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| {
                ManagerError::ExtractFailed(format!("读取条目失败（index {}）：{}", i, e))
            })?;

            let file_path = match file.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => {
                    report_event(
                        "Extract.Entry.Failed",
                        Some(&format!("index:{};reason=unsafe_enclosed_name", i)),
                    );
                    return Err(ManagerError::ExtractFailed(format!(
                        "条目 {} 包含不安全的文件路径",
                        i
                    )));
                }
            };

            if !Self::is_safe_path(&file_path) {
                report_event(
                    "Extract.Entry.Failed",
                    Some(&format!("index:{};path={}", i, file_path.display())),
                );
                return Err(ManagerError::ExtractFailed(format!(
                    "不安全的文件路径：{}",
                    file_path.display()
                )));
            }

            // 禁止符号链接与特殊文件类型
            if file
                .unix_mode()
                .map(|m| (m & 0o170000) == 0o120000)
                .unwrap_or(false)
            {
                report_event(
                    "Extract.Entry.Failed",
                    Some(&format!(
                        "index:{};reason=symlink;path={}",
                        i,
                        file_path.display()
                    )),
                );
                return Err(ManagerError::ExtractFailed(format!(
                    "条目 {} 为符号链接，禁止解压：{}",
                    i,
                    file_path.display()
                )));
            }

            let should_exclude = exclude_patterns.iter().any(|pattern| {
                let pat = Path::new(pattern);
                file_path.starts_with(pat)
            });

            if should_exclude {
                continue;
            }

            let outpath = dest_dir.join(&file_path);

            if file.name().ends_with('/') {
                std::fs::create_dir_all(&outpath).map_err(|e| {
                    ManagerError::from(std::io::Error::new(
                        e.kind(),
                        format!("创建目录失败：{}", e),
                    ))
                })?;
            } else {
                if let Some(p) = outpath.parent() {
                    std::fs::create_dir_all(p).map_err(|e| {
                        ManagerError::from(std::io::Error::new(
                            e.kind(),
                            format!("创建父目录失败：{}", e),
                        ))
                    })?;
                }

                let mut tmp_path = outpath.with_extension("tmp");

                let mut tmp_idx = 0;
                while tmp_path.exists() {
                    tmp_idx += 1;
                    tmp_path = outpath.with_extension(format!("tmp{}", tmp_idx));
                }

                let mut tmp_file = match std::fs::File::create(&tmp_path) {
                    Ok(f) => f,
                    Err(e) => {
                        report_event(
                            "Extract.Entry.Failed",
                            Some(&format!("index:{};reason=tmp_create;err={}", i, e)),
                        );
                        return Err(ManagerError::from(std::io::Error::new(
                            e.kind(),
                            format!("创建临时文件失败：{}", e),
                        )));
                    }
                };

                if let Err(e) = std::io::copy(&mut file, &mut tmp_file) {
                    report_event(
                        "Extract.Entry.Failed",
                        Some(&format!("index:{};reason=write;err={}", i, e)),
                    );
                    return Err(ManagerError::from(std::io::Error::new(
                        e.kind(),
                        format!("写入临时文件失败：{}", e),
                    )));
                }

                match atomic_rename_or_copy(&tmp_path, &outpath) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&tmp_path);
                        extracted_files.push(outpath);
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp_path);
                        report_event(
                            "Extract.Entry.Failed",
                            Some(&format!("index:{};reason=rename;err={}", i, e)),
                        );
                        return Err(ManagerError::from(std::io::Error::other(format!(
                            "重命名或复制临时文件失败：{}",
                            e
                        ))));
                    }
                }
            }
        }

        report_event(
            "Extract.Success",
            Some(&format!("count:{}", extracted_files.len())),
        );
        Ok(extracted_files)
    }

    /// 安装 BepInEx 到游戏根目录
    pub fn deploy_bepinex(zip_path: &Path, game_root: &Path, skip_plugins: bool) -> Result<()> {
        report_event("Deploy.BepInEx.Start", Some(&zip_path.to_string_lossy()));
        let res = if skip_plugins {
            Self::extract_zip_safe_with_exclusions(zip_path, game_root, &["BepInEx/plugins"])
        } else {
            Self::extract_zip_safe(zip_path, game_root)
        };

        match res {
            Ok(_) => {
                report_event("Deploy.BepInEx.Success", Some(&zip_path.to_string_lossy()));
                Ok(())
            }
            Err(e) => {
                report_event("Deploy.BepInEx.Failed", Some(&format!("{}", e)));
                Err(e)
            }
        }
    }

    /// 安装 MetaMystia DLL 到 BepInEx/plugins/ 目录
    pub fn deploy_metamystia(dll_path: &Path, game_root: &Path) -> Result<()> {
        let plugins_dir = game_root.join("BepInEx/plugins");

        if !plugins_dir.exists() {
            return Err(ManagerError::Other(
                "BepInEx/plugins 目录不存在，请先执行安装操作".to_string(),
            ));
        }

        let dest = plugins_dir.join(
            dll_path
                .file_name()
                .ok_or_else(|| ManagerError::Other("无效的文件名".to_string()))?,
        );

        report_event("Deploy.MetaMystia.Start", Some(&dll_path.to_string_lossy()));
        let tmp_dest = dest.with_extension("dll.tmp");
        std::fs::copy(dll_path, &tmp_dest).map_err(|e| {
            report_event(
                "Deploy.MetaMystia.Failed",
                Some(&format!("copy failed:{}", e)),
            );
            ManagerError::from(std::io::Error::new(
                e.kind(),
                format!("复制文件失败：{}", e),
            ))
        })?;
        match atomic_rename_or_copy(&tmp_dest, &dest) {
            Ok(_) => {
                report_event("Deploy.MetaMystia.Success", Some(&dest.to_string_lossy()));
                Ok(())
            }
            Err(e) => {
                report_event(
                    "Deploy.MetaMystia.Failed",
                    Some(&format!("rename failed:{}", e)),
                );
                Err(ManagerError::from(std::io::Error::other(format!(
                    "安装失败：{}",
                    e
                ))))
            }
        }
    }

    /// 安装 ResourceExample ZIP 到 ResourceEx/ 目录
    pub fn deploy_resourceex(zip_path: &Path, game_root: &Path) -> Result<()> {
        let resourceex_dir = game_root.join("ResourceEx");

        if !resourceex_dir.exists() {
            std::fs::create_dir_all(&resourceex_dir).map_err(|e| {
                ManagerError::from(std::io::Error::new(
                    e.kind(),
                    format!("创建 ResourceEx 目录失败：{}", e),
                ))
            })?;
        }

        let filename = zip_path
            .file_name()
            .ok_or_else(|| ManagerError::Other("无效的文件名".to_string()))?;
        let dest = resourceex_dir.join(filename);

        report_event("Deploy.ResourceEx.Start", Some(&zip_path.to_string_lossy()));
        let tmp_dest = dest.with_extension("zip.tmp");
        std::fs::copy(zip_path, &tmp_dest).map_err(|e| {
            report_event(
                "Deploy.ResourceEx.Failed",
                Some(&format!("copy failed:{}", e)),
            );
            ManagerError::from(std::io::Error::new(
                e.kind(),
                format!("复制文件失败：{}", e),
            ))
        })?;
        match atomic_rename_or_copy(&tmp_dest, &dest) {
            Ok(_) => {
                report_event("Deploy.ResourceEx.Success", Some(&dest.to_string_lossy()));
                Ok(())
            }
            Err(e) => {
                report_event(
                    "Deploy.ResourceEx.Failed",
                    Some(&format!("rename failed:{}", e)),
                );
                Err(ManagerError::from(std::io::Error::other(format!(
                    "安装失败：{}",
                    e
                ))))
            }
        }
    }
}
