use std::collections::HashSet;
use std::env;
use std::fs;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use ipnet::IpNet;
use kairo_lib::packet::AiTcpPacket;
use moka::future::Cache;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use reqwest::{Client, Url};

use crate::vulkan_gpu::{
    global_backend, VulkanBackendState, VulkanBatchSubmission, VulkanExecutionPath,
    VulkanFallbackReason, VulkanPacketObservation, VulkanQueueClass, VulkanWorkloadClass,
};

static GLOBAL_REMOTE_GUARD: Lazy<Mutex<Arc<RemoteGuard>>> =
    Lazy::new(|| Mutex::new(Arc::new(RemoteGuard::default())));

#[derive(Debug, Clone)]
pub struct RemoteGuardConfig {
    pub black_hosts: HashSet<String>,
    pub black_nets: Vec<IpNet>,
    pub ai_hosts: HashSet<String>,
    pub ai_probe_url: Option<Url>,
    pub trace_timeout_ms: u64,
}

impl Default for RemoteGuardConfig {
    fn default() -> Self {
        Self {
            black_hosts: HashSet::new(),
            black_nets: Vec::new(),
            ai_hosts: HashSet::new(),
            ai_probe_url: None,
            trace_timeout_ms: 2000,
        }
    }
}

impl RemoteGuardConfig {
    pub fn from_files_and_env() -> Result<Self, String> {
        let mut config = Self::default();

        if let Ok(content) = fs::read_to_string("/etc/kairo-fw/ai_hosts.txt") {
            config.ai_hosts = content.lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .map(|s| s.to_lowercase())
                .collect();
        }
        if let Ok(content) = fs::read_to_string("/etc/kairo-fw/blacklist.txt") {
            config.black_hosts = content.lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .map(|s| s.to_lowercase())
                .collect();
        }

        Ok(config)
    }
}

pub struct RemoteGuard {
    config: RemoteGuardConfig,
    client: Client,
    trace_cache: Cache<String, bool>,
}

impl Default for RemoteGuard {
    fn default() -> Self {
        Self::new(RemoteGuardConfig::default()).unwrap()
    }
}

impl RemoteGuard {
    pub fn new(config: RemoteGuardConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.trace_timeout_ms))
            .build()
            .map_err(|e| e.to_string())?;

        let trace_cache = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .max_capacity(1000)
            .build();

        Ok(Self { config, client, trace_cache })
    }

    pub async fn inspect(
        &self,
        packet: &AiTcpPacket,
        _remote_is_ai: bool,
        _pid: Option<u32>,
    ) -> RemoteDecision {
        // Vulkan Pre-filter (Core Feature)
        let _signal = self.run_gpu_prefilter(packet).await;

        let host = packet.destination.clone().to_lowercase();
        
        if self.config.black_hosts.contains(&host) {
            return RemoteDecision::SilentDrop(RemoteDropReason::BlacklistedHost);
        }

        RemoteDecision::Allow
    }

    async fn run_gpu_prefilter(&self, _packet: &AiTcpPacket) -> Option<VulkanPacketObservation> {
        // KAIRO-FW: Keep the integration point for Vulkan
        let _backend = global_backend();
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDecision {
    Allow,
    SilentDrop(RemoteDropReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDropReason {
    BlacklistedHost,
    BlacklistedIp,
    RiskyAiResponse,
    TraceFailure,
}

impl RemoteDropReason {
    pub fn code(&self) -> u16 {
        match self {
            Self::BlacklistedHost => 0x2001,
            Self::BlacklistedIp => 0x2002,
            Self::RiskyAiResponse => 0x2003,
            Self::TraceFailure => 0x2004,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::BlacklistedHost => "blacklisted_host",
            Self::BlacklistedIp => "blacklisted_ip",
            Self::RiskyAiResponse => "risky_ai_response",
            Self::TraceFailure => "trace_failure",
        }
    }
}

pub fn global_remote_guard() -> Arc<RemoteGuard> {
    GLOBAL_REMOTE_GUARD.lock().clone()
}

pub fn set_global_remote_guard(guard: Arc<RemoteGuard>) {
    *GLOBAL_REMOTE_GUARD.lock() = guard;
}

pub fn configure_global_remote_guard() -> Result<Arc<RemoteGuard>, String> {
    let guard = Arc::new(RemoteGuard::new(RemoteGuardConfig::from_files_and_env()?)?);
    set_global_remote_guard(guard.clone());
    Ok(guard)
}
