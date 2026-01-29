use crate::error::{ManagerError, Result};
use crate::file_ops::atomic_rename_or_copy;
use crate::model::VersionInfo;
use crate::net::{get_json_with_retry, get_response_with_retry, with_retry};
use crate::ui::Ui;

use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use reqwest::blocking::{Client, ClientBuilder};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

const FILE_API: &str = "https://file.izakaya.cc/api/public/dl";
const REDIRECT_URL: &str = "https://url.izakaya.cc/getMetaMystia";
const VERSION_API: &str = "https://api.izakaya.cc/version/meta-mystia";

const BEPINEX_PRIMARY: &str = "https://builds.bepinex.dev/projects/bepinex_be";
const GITHUB_API_URL: &str = "https://api.github.com/repos/MetaMikuAI/MetaMystia/releases/latest";

const RATE_LIMIT: usize = 192 * 1024; // 192KB/s
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5); // 连接超时

/// 下载器
pub struct Downloader<'a> {
    client: Client,
    ui: &'a dyn Ui,
    cached_version: Mutex<Option<VersionInfo>>,
}

impl<'a> Downloader<'a> {
    pub fn new(ui: &'a dyn Ui) -> Result<Self> {
        let client = Self::build_client(CONNECT_TIMEOUT)?;
        Ok(Self {
            client,
            ui,
            cached_version: Mutex::new(None),
        })
    }

    fn build_client(connect_timeout: Duration) -> Result<Client> {
        ClientBuilder::new()
            .connect_timeout(connect_timeout)
            .user_agent(format!(
                "meta-mystia-manager/{} (+https://github.com/AnYiEE/meta-mystia-manager)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| ManagerError::NetworkError(format!("创建 HTTP 客户端失败：{}", e)))
    }

    fn retry<F, T>(&self, op_desc: &str, f: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        with_retry(self.ui, op_desc, f)
    }

    fn file_api_url(share_code: &str, filename: &str) -> String {
        format!("{}/{}/{}", FILE_API, share_code, filename)
    }

    fn parse_share_code_from_url(url: &str) -> Option<String> {
        url.trim_end_matches('/')
            .split('/')
            .next_back()
            .and_then(|s| s.split(&['?', '#'][..]).next())
            .map(|s| s.to_string())
    }

    /// 获取版本信息
    pub fn get_version_info(&self) -> Result<VersionInfo> {
        if let Some(cached) = self.cached_version.lock().unwrap().clone() {
            return Ok(cached);
        }

        let vi = self.retry("获取版本信息", || self.try_get_version_info())?;
        let mut lock = self.cached_version.lock().unwrap();
        *lock = Some(vi.clone());

        Ok(vi)
    }

    fn try_get_version_info(&self) -> Result<VersionInfo> {
        self.ui.download_version_info_start()?;

        let response = self.client.get(VERSION_API).send().map_err(|e| {
            let _ = self.ui.download_version_info_failed(&format!("{}", e));
            let base_msg = if e.is_timeout() {
                "请求超时".to_string()
            } else if e.is_connect() {
                "连接失败".to_string()
            } else if e.is_status() {
                format!(
                    "服务器返回错误：HTTP {}",
                    e.status()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "未知".to_string())
                )
            } else {
                format!("请求失败：{}", e)
            };
            ManagerError::NetworkError(base_msg)
        })?;

        if !response.status().is_success() {
            return Err(ManagerError::NetworkError(format!(
                "获取版本信息失败：HTTP {}",
                response.status()
            )));
        }

        self.ui.download_version_info_success()?;

        let text = response
            .text()
            .map_err(|e| ManagerError::NetworkError(format!("读取响应失败：{}", e)))?;

        serde_json::from_str(&text).map_err(|e| {
            let snippet: String = text.chars().take(200).collect();
            let _ = self
                .ui
                .download_version_info_parse_failed(&format!("{}", e), &snippet);
            ManagerError::Other(format!("解析版本信息失败：{}", e))
        })
    }

    /// 获取分享码
    pub fn get_share_code(&self) -> Result<String> {
        self.retry("获取下载链接", || self.try_get_share_code())
    }

    fn try_get_share_code(&self) -> Result<String> {
        self.ui.download_share_code_start()?;

        let response = self.client.get(REDIRECT_URL).send().map_err(|e| {
            let _ = self.ui.download_share_code_failed(&format!("{}", e));
            let base_msg = if e.is_timeout() {
                "请求超时".to_string()
            } else if e.is_connect() {
                "连接失败".to_string()
            } else if e.is_status() {
                format!(
                    "服务器返回错误：HTTP {}",
                    e.status()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "未知".to_string())
                )
            } else {
                format!("请求失败：{}", e)
            };
            ManagerError::NetworkError(base_msg)
        })?;

        if !response.status().is_success() {
            return Err(ManagerError::NetworkError(format!(
                "获取下载链接失败：HTTP {}",
                response.status()
            )));
        }

        self.ui.download_share_code_success()?;

        let final_url = response.url().as_str();
        Self::parse_share_code_from_url(final_url)
            .ok_or_else(|| ManagerError::NetworkError("无法从下载链接中解析分享码".to_string()))
    }

    fn download_file_with_progress(
        &self,
        url: &str,
        dest: &Path,
        file_size: Option<u64>,
        rate_limit: bool,
    ) -> Result<()> {
        match self.retry("下载文件", || {
            self.try_download(url, dest, file_size, rate_limit)
        }) {
            Ok(()) => Ok(()),
            Err(_) => Err(ManagerError::DownloadFailed("多次重试后仍失败".to_string())),
        }
    }

    fn try_download(
        &self,
        url: &str,
        dest: &Path,
        file_size: Option<u64>,
        rate_limit: bool,
    ) -> Result<()> {
        let mut response = self
            .client
            .get(url)
            .send()
            .map_err(|e| ManagerError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ManagerError::NetworkError(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let total_size = file_size.or_else(|| response.content_length());

        let filename = dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dest.display().to_string());

        let id = self.ui.download_start(&filename, total_size);

        self.write_response_to_file(&mut response, dest, id, rate_limit)
    }

    fn write_response_to_file<R: Read>(
        &self,
        resp: &mut R,
        dest: &Path,
        id: usize,
        rate_limit: bool,
    ) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ManagerError::Io(format!("创建目录失败：{}", e)))?;
        }

        let mut tmp_path = dest.with_extension("dl.tmp");
        let mut tmp_idx = 0;
        while tmp_path.exists() {
            tmp_idx += 1;
            tmp_path = dest.with_extension(format!("dl.tmp{}", tmp_idx));
        }

        let mut tmp_file = std::fs::File::create(&tmp_path)
            .map_err(|e| ManagerError::Io(format!("创建临时文件失败：{}", e)))?;

        let buf_len = std::cmp::min(RATE_LIMIT, 8192) as usize;
        let mut buffer = vec![0; buf_len];
        let mut downloaded = 0u64;

        let mut tokens = RATE_LIMIT as f64;
        let mut last_check = std::time::Instant::now();

        loop {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_check).as_secs_f64();
            last_check = now;
            tokens += elapsed * RATE_LIMIT as f64;
            if tokens > RATE_LIMIT as f64 {
                tokens = RATE_LIMIT as f64;
            }

            let mut to_read = buffer.len();
            if rate_limit {
                let available = tokens.floor() as usize;
                if available == 0 {
                    let wait_secs = 1.0 / RATE_LIMIT as f64;
                    let sleep_dur = if cfg!(test) {
                        Duration::from_millis(1)
                    } else {
                        Duration::from_secs_f64(wait_secs)
                    };
                    std::thread::sleep(sleep_dur);
                    continue;
                }
                to_read = std::cmp::min(to_read, available);
            }

            let n = resp
                .read(&mut buffer[..to_read])
                .map_err(|e| ManagerError::NetworkError(e.to_string()))?;
            if n == 0 {
                break;
            }

            tmp_file
                .write_all(&buffer[..n])
                .map_err(|e| ManagerError::Io(format!("写入临时文件失败：{}", e)))?;

            if rate_limit {
                tokens -= n as f64;
                if tokens < 0.0 {
                    tokens = 0.0;
                }
            }

            downloaded += n as u64;
            self.ui.download_update(id, downloaded);
        }

        tmp_file
            .flush()
            .map_err(|e| ManagerError::Io(format!("同步临时文件失败：{}", e)))?;

        match atomic_rename_or_copy(&tmp_path, dest) {
            Ok(_) => {
                let _ = std::fs::remove_file(&tmp_path);
                self.ui.download_finish(
                    id,
                    &format!(
                        "下载完成：{}",
                        dest.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| dest.display().to_string())
                    ),
                );
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_path);
                Err(ManagerError::Io(format!("重命名或复制临时文件失败：{}", e)))
            }
        }
    }

    fn get_dll_download_url_from_github(&self) -> Result<String> {
        self.ui.download_attempt_github_dll()?;

        let json: serde_json::Value = get_json_with_retry(
            &self.client,
            self.ui,
            GITHUB_API_URL,
            Some("application/vnd.github+json"),
            "请求 GitHub API ",
        )?;

        if let Some(assets) = json["assets"].as_array() {
            for asset in assets {
                match (
                    asset["name"].as_str(),
                    asset["browser_download_url"].as_str(),
                ) {
                    (Some(name), Some(url))
                        if name.starts_with("MetaMystia-v") && name.ends_with(".dll") =>
                    {
                        self.ui.download_found_github_asset(name)?;
                        return Ok(url.to_string());
                    }
                    _ => {}
                }
            }
        }

        self.ui.download_github_dll_not_found()?;
        Err(ManagerError::NetworkError(
            "未找到 MetaMystia DLL 文件".to_string(),
        ))
    }

    /// 下载 MetaMystia DLL
    pub fn download_metamystia(
        &self,
        share_code: &str,
        version_info: &VersionInfo,
        dest: &Path,
    ) -> Result<()> {
        match self.get_dll_download_url_from_github() {
            Ok(url) => {
                if let Err(e) = self.download_file_with_progress(&url, dest, None, false) {
                    self.ui.download_switch_to_fallback(&format!(
                        "从 GitHub 下载 MetaMystia DLL 失败：{}，切换到备用源...",
                        e
                    ))?;
                    self.ui.download_try_fallback_metamystia()?;
                    let filename = version_info.metamystia_filename();
                    let fallback_url = Self::file_api_url(share_code, &filename);
                    self.download_file_with_progress(&fallback_url, dest, None, true)
                } else {
                    Ok(())
                }
            }
            Err(_) => {
                self.ui.download_switch_to_fallback(
                    "从 GitHub 获取 MetaMystia DLL 下载链接失败，切换到备用源...",
                )?;
                self.ui.download_try_fallback_metamystia()?;
                let filename = version_info.metamystia_filename();
                let url = Self::file_api_url(share_code, &filename);
                self.download_file_with_progress(&url, dest, None, true)
            }
        }
    }

    /// 下载 ResourceExample ZIP
    pub fn download_resourceex(
        &self,
        share_code: &str,
        version_info: &VersionInfo,
        dest: &Path,
    ) -> Result<()> {
        self.ui.download_resourceex_start()?;
        let filename = version_info.resourceex_filename();
        let url = Self::file_api_url(share_code, &filename);
        self.download_file_with_progress(&url, dest, None, true)
    }

    /// 下载 BepInEx
    pub fn download_bepinex(&self, version_info: &VersionInfo, dest: &Path) -> Result<()> {
        let filename = version_info.bepinex_filename()?;
        let version = version_info.bepinex_version()?;
        let filename_with_version = percent_encode(
            format!("{}#{}", version, filename).as_bytes(),
            NON_ALPHANUMERIC,
        )
        .to_string();

        let primary_url = format!("{}/{}/{}", BEPINEX_PRIMARY, version, filename);

        self.ui.download_bepinex_attempt_primary()?;

        let primary_result =
            get_response_with_retry(&self.client, self.ui, &primary_url, "请求 BepInEx 主源");

        match primary_result {
            Ok(mut resp) => {
                let total_size = resp.content_length();
                let id = self.ui.download_start("BepInEx（bepinex.dev）", total_size);

                if let Err(e) = self.write_response_to_file(&mut resp, dest, id, false) {
                    self.ui.download_finish(id, "从 bepinex.dev 下载失败");
                    self.ui.download_bepinex_primary_failed(&format!(
                        "从 bepinex.dev 下载失败 ({}), 切换到备用源...",
                        e
                    ))?;
                    let share_code = self.get_share_code()?;
                    let fallback_url = Self::file_api_url(&share_code, &filename_with_version);
                    return self.download_file_with_progress(&fallback_url, dest, None, true);
                }

                Ok(())
            }
            Err(_) => {
                self.ui.download_bepinex_primary_failed(
                    "从 bepinex.dev 下载失败或超时，切换到备用源...",
                )?;
                let share_code = self.get_share_code()?;
                let fallback_url = Self::file_api_url(&share_code, &filename_with_version);
                self.download_file_with_progress(&fallback_url, dest, None, true)
            }
        }
    }

    /// 下载管理工具可执行文件
    pub fn download_manager(&self, version_info: &VersionInfo, dest: &Path) -> Result<()> {
        let filename = version_info.manager_filename();
        let share_code = self.get_share_code()?;
        let url = Self::file_api_url(&share_code, &filename);

        self.download_file_with_progress(&url, dest, None, true)
    }
}
