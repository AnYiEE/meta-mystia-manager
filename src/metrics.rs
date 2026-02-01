use crate::error::Result;

use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use reqwest::blocking::Client;
use std::collections::HashMap;
use std::process::Command;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const ID_SITE: &str = "13";
const TRACKING_ENDPOINT: &str = "https://track.izakaya.cc/api.php";
const DEFAULT_TIMEOUT_SECS: u64 = 10;

fn build_tracking_url(user_id: &str, params: &HashMap<&str, String>) -> String {
    let mut base = vec![
        ("idsite".to_string(), ID_SITE.to_string()),
        ("rec".to_string(), "1".to_string()),
        ("_id".to_string(), user_id.to_string()),
        ("uid".to_string(), user_id.to_string()),
    ];

    for (k, v) in params.iter() {
        base.push((k.to_string(), v.clone()));
    }

    let q: String = base
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, percent_encode(v.as_bytes(), NON_ALPHANUMERIC)))
        .collect::<Vec<_>>()
        .join("&");

    format!("{}?{}", TRACKING_ENDPOINT, q)
}

fn read_machine_guid() -> Option<String> {
    let out = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }

    let s = String::from_utf8_lossy(&out.stdout);
    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("MachineGuid") {
            let parts: Vec<&str> = t.split_whitespace().collect();
            if let Some(val) = parts.last() {
                return Some(val.to_string());
            }
        }
    }

    None
}

fn md5_hex(input: &str) -> String {
    format!("{:x}", md5::compute(input))
}

static CACHED_USER_ID: OnceLock<String> = OnceLock::new();

fn get_user_id() -> String {
    CACHED_USER_ID
        .get_or_init(|| {
            if let Some(guid) = read_machine_guid() {
                return md5_hex(&guid);
            }

            let hostname = std::env::var("COMPUTERNAME").unwrap_or_default();
            let username = std::env::var("USERNAME").unwrap_or_default();
            let combined = format!("{}|{}", hostname, username);

            md5_hex(&combined)
        })
        .clone()
}

fn build_client() -> Result<Client> {
    let client = Client::builder()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .user_agent(crate::config::USER_AGENT)
        .build()
        .map_err(|e| {
            crate::error::ManagerError::NetworkError(format!("创建 metrics HTTP 客户端失败：{}", e))
        })?;

    Ok(client)
}

fn send_tracking_request(url: String) {
    thread::spawn(move || {
        if let Ok(client) = build_client() {
            let _ = client.get(&url).send();
        }
    });
}

pub fn report_event(action: &str, name: Option<&str>) {
    let user_id = get_user_id();

    let mut params: HashMap<&str, String> = HashMap::new();
    params.insert("ca", "1".to_string());
    params.insert("e_c", "Manager".to_string());
    params.insert("e_a", action.to_string());
    if let Some(n) = name {
        params.insert("e_n", n.to_string());
    }

    let url = build_tracking_url(&user_id, &params);
    send_tracking_request(url);
}
