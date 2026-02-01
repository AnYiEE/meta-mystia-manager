use crate::error::{ManagerError, Result};
use crate::metrics::report_event;

use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct VersionInfo {
    #[serde(rename = "bepInEx")]
    pub bep_in_ex: String,
    pub manager: String,
    pub dll: String,
    pub zip: String,
}

impl VersionInfo {
    /// 解析 BepInEx 的文件名
    pub fn bepinex_filename(&self) -> Result<&str> {
        self.bep_in_ex
            .split('#')
            .nth(1)
            .map(|s| s.trim())
            .ok_or_else(|| {
                report_event("Model.VersionInfo.Invalid", Some("bepinex_filename"));
                ManagerError::InvalidVersionInfo
            })
    }

    /// 解析 BepInEx 的版本号
    pub fn bepinex_version(&self) -> Result<&str> {
        self.bep_in_ex
            .split('#')
            .nth(0)
            .map(|s| s.trim())
            .ok_or_else(|| {
                report_event("Model.VersionInfo.Invalid", Some("bepinex_version"));
                ManagerError::InvalidVersionInfo
            })
    }

    /// MetaMystia DLL 文件名
    pub fn metamystia_filename(&self) -> String {
        format!("MetaMystia-v{}.dll", self.dll.trim())
    }

    /// ResourceExample ZIP 文件名
    pub fn resourceex_filename(&self) -> String {
        format!("ResourceExample-v{}.zip", self.zip.trim())
    }

    //// MetaMystia Manager 可执行文件名
    pub fn manager_filename(&self) -> String {
        format!("meta-mystia-manager-v{}.exe", self.manager.trim())
    }
}
