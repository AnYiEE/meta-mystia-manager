pub enum UninstallMode {
    Light,
    Full,
}

impl UninstallMode {
    const LIGHT_TARGETS: &'static [(&'static str, bool)] = &[
        ("BepInEx/plugins/MetaMystia-*.dll", false),
        ("ResourceEx/ResourceExample-*.zip", false),
    ];

    const FULL_TARGETS: &'static [(&'static str, bool)] = &[
        ("BepInEx", true),
        (".doorstop_version", false),
        ("changelog.txt", false),
        ("doorstop_config.ini", false),
        ("MinHook.x64.dll", false),
        ("winhttp.dll", false),
        ("ResourceEx", true),
    ];

    /// 要删除的目标（模式字符串，是否为目录）
    pub fn get_targets(&self) -> &'static [(&'static str, bool)] {
        match self {
            UninstallMode::Light => Self::LIGHT_TARGETS,
            UninstallMode::Full => Self::FULL_TARGETS,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            UninstallMode::Light => {
                "仅移除 MetaMystia 相关文件（保留 BepInEx 框架和其他 Mod 相关文件）"
            }
            UninstallMode::Full => "移除所有和 Mod 有关的文件（还原为原版游戏）",
        }
    }
}

pub const GAME_EXECUTABLE: &str = "Touhou Mystia Izakaya.exe";
pub const GAME_PROCESS_NAME: &str = "Touhou Mystia Izakaya.exe";
pub const GAME_STEAM_APP_ID: u32 = 1_584_090;

pub const USER_AGENT: &str = concat!(
    "meta-mystia-manager/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/AnYiEE/meta-mystia-manager)"
);

pub struct NetworkRetryConfig {
    /// 最大重试次数（至少 1）
    pub attempts: usize,
    /// 基础延迟（秒）
    pub base_delay_secs: u64,
    /// 指数倍数（例如 2.0 表示每次延迟翻倍）
    pub multiplier: f64,
    /// 最大延迟（秒）上限
    pub max_delay_secs: u64,
}

impl Default for NetworkRetryConfig {
    fn default() -> Self {
        Self {
            attempts: 3,
            base_delay_secs: 5,
            multiplier: 2.0,
            max_delay_secs: 15,
        }
    }
}

pub fn network_retry_config() -> NetworkRetryConfig {
    NetworkRetryConfig::default()
}

pub struct UninstallRetryConfig {
    pub attempts: usize,
    pub base_delay_secs: u64,
    pub multiplier: f64,
    pub max_delay_secs: u64,
}

impl Default for UninstallRetryConfig {
    fn default() -> Self {
        Self {
            attempts: 3,
            base_delay_secs: 10,
            multiplier: 2.0,
            max_delay_secs: 60,
        }
    }
}

pub fn uninstall_retry_config() -> UninstallRetryConfig {
    UninstallRetryConfig::default()
}
