use crate::config::network_retry_config;
use crate::error::{ManagerError, Result};
use crate::ui::Ui;

use reqwest::blocking::{Client, Response};
use reqwest::header::RETRY_AFTER;
use std::time::Duration;

pub fn with_retry<F, T>(ui: &dyn Ui, op_desc: &str, mut f: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let cfg = network_retry_config();

    for attempt in 0..cfg.attempts {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if attempt < cfg.attempts => {
                let raw = (cfg.base_delay_secs as f64) * cfg.multiplier.powi(attempt as i32);
                let delay_secs = raw.min(cfg.max_delay_secs as f64).ceil() as u64;

                ui.warn(&format!(
                    "{}失败，{} 秒后重试...（重试 {}/{}）",
                    op_desc,
                    delay_secs,
                    attempt + 1,
                    cfg.attempts
                ))?;
                ui.warn(&format!("错误：{}", e))?;

                let sleep_dur = if cfg!(test) {
                    Duration::from_millis(1)
                } else {
                    Duration::from_secs(delay_secs)
                };

                std::thread::sleep(sleep_dur);
            }
            Err(e) => return Err(e),
        }
    }

    Err(ManagerError::NetworkError(format!(
        "{}达到最大重试次数",
        op_desc
    )))
}

fn parse_retry_after_seconds(hv: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    hv.and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// 使用重试机制获取并解析 JSON 数据
pub fn get_json_with_retry<T: serde::de::DeserializeOwned>(
    client: &Client,
    ui: &dyn Ui,
    url: &str,
    accept_header: Option<&str>,
    op_desc: &str,
) -> Result<T> {
    with_retry(ui, op_desc, || {
        let mut req = client.get(url);
        if let Some(h) = accept_header {
            req = req.header("Accept", h);
        }

        let resp = req
            .send()
            .map_err(|e| ManagerError::NetworkError(format!("请求失败：{}", e)))?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 429 {
                if let Some(s) = parse_retry_after_seconds(resp.headers().get(RETRY_AFTER)) {
                    ui.warn(&format!("检测到限流，Retry-After={} 秒，等待后重试...", s))?;
                    std::thread::sleep(Duration::from_secs(s));
                } else {
                    ui.warn("检测到限流，稍后重试...")?;
                    std::thread::sleep(Duration::from_secs(5));
                }
                return Err(ManagerError::RateLimited(op_desc.to_string()));
            }
            return Err(ManagerError::NetworkError(format!(
                "{}返回错误：HTTP {}",
                op_desc,
                resp.status()
            )));
        }

        let text = resp
            .text()
            .map_err(|e| ManagerError::NetworkError(format!("读取响应失败：{}", e)))?;

        serde_json::from_str(&text)
            .map_err(|e| ManagerError::NetworkError(format!("解析 JSON 失败：{}", e)))
    })
}

/// 使用重试机制获取响应
pub fn get_response_with_retry(
    client: &Client,
    ui: &dyn Ui,
    url: &str,
    op_desc: &str,
) -> Result<Response> {
    with_retry(ui, op_desc, || {
        let resp = client
            .get(url)
            .send()
            .map_err(|e| ManagerError::NetworkError(format!("请求失败：{}", e)))?;

        if !resp.status().is_success() {
            if resp.status().as_u16() == 429 {
                if let Some(s) = parse_retry_after_seconds(resp.headers().get(RETRY_AFTER)) {
                    ui.warn(&format!("检测到限流，Retry-After={} 秒，等待后重试...", s))?;
                    std::thread::sleep(Duration::from_secs(s));
                } else {
                    std::thread::sleep(Duration::from_secs(5));
                }
                return Err(ManagerError::RateLimited(op_desc.to_string()));
            }
            return Err(ManagerError::NetworkError(format!(
                "{}返回错误：HTTP {}",
                op_desc,
                resp.status()
            )));
        }

        Ok(resp)
    })
}
