use std::collections::{HashMap, HashSet};
use std::env;
use std::net::IpAddr;
use std::os::fd::RawFd;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use kairo_lib::packet::AiTcpPacket;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use crate::tracker::AgentPidTracker;

const DEFAULT_REPLAY_WINDOW_MS: u64 = 1000;
const DEFAULT_IPI_PATTERNS: [&str; 6] = [
    "ignore previous instructions",
    "ignore all previous instructions",
    "system prompt",
    "send all data",
    "upload secrets",
    "exfiltrate",
];

#[derive(Clone, Debug)]
pub struct PacketMeta<'a> {
    pub fd: Option<RawFd>,
    pub source_port: u16,
    pub dest_ip: Option<IpAddr>,
    pub dest_host: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterDecision {
    SkipNonAgent,
    FastBypass,
    DeepInspect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SilentDropReason {
    MissingIdentity,
    MissingReplayMetadata,
    MissingSubjectPolicy,
    ReplayOutsideWindow,
    ReplayNonceReuse,
    InvalidSignatureEncoding,
    InvalidSignatureDomain,
    SignatureVerificationFailed,
    UnauthorizedTool,
    UnauthorizedDestination,
    IndirectPromptInjection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryDecision {
    Allow,
    SilentDrop(SilentDropReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedDestination {
    pub host: Option<String>,
    pub ip: Option<IpAddr>,
    pub port: Option<u16>,
}

impl AllowedDestination {
    pub fn new(host: Option<String>, ip: Option<IpAddr>, port: Option<u16>) -> Self {
        Self { host, ip, port }
    }

    fn matches(&self, host: Option<&str>, ip: Option<IpAddr>, port: Option<u16>) -> bool {
        if let Some(allowed_ip) = self.ip {
            if Some(allowed_ip) != ip {
                return false;
            }
        }
        if let Some(allowed_port) = self.port {
            if Some(allowed_port) != port {
                return false;
            }
        }
        if let Some(expected_host) = self.host.as_deref() {
            let Some(actual_host) = host else {
                return false;
            };
            if !expected_host.eq_ignore_ascii_case(actual_host) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
pub struct SubjectCapability {
    pub agent_id: String,
    pub session_id: String,
    pub allowed_tools: HashSet<String>,
    pub allowed_destinations: Vec<AllowedDestination>,
    pub blocked_arg_patterns: Vec<String>,
}

impl SubjectCapability {
    pub fn new(agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: session_id.into(),
            allowed_tools: HashSet::new(),
            allowed_destinations: Vec::new(),
            blocked_arg_patterns: Vec::new(),
        }
    }

    pub fn subject_key(&self) -> String {
        format!("{}::{}", self.agent_id, self.session_id)
    }
}

pub struct DefenseFilter {
    replay_window_ms: u64,
    subjects: RwLock<HashMap<String, SubjectCapability>>,
    used_nonces: Mutex<HashMap<String, HashMap<String, u64>>>,
}

impl Default for DefenseFilter {
    fn default() -> Self {
        Self::new(DEFAULT_REPLAY_WINDOW_MS)
    }
}

impl DefenseFilter {
    pub fn new(replay_window_ms: u64) -> Self {
        Self {
            replay_window_ms,
            subjects: RwLock::new(HashMap::new()),
            used_nonces: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_subject(&self, capability: SubjectCapability) {
        self.subjects
            .write()
            .unwrap()
            .insert(capability.subject_key(), capability);
    }

    pub fn evaluate_boundary(
        &self,
        packet: &AiTcpPacket,
        meta: &PacketMeta<'_>,
    ) -> BoundaryDecision {
        if !packet.has_complete_identity() {
            return BoundaryDecision::SilentDrop(SilentDropReason::MissingIdentity);
        }
        if !packet.has_complete_replay_metadata() {
            return BoundaryDecision::SilentDrop(SilentDropReason::MissingReplayMetadata);
        }

        let Some(subject) = self
            .subjects
            .read()
            .unwrap()
            .get(&packet.subject_key())
            .cloned()
        else {
            return BoundaryDecision::SilentDrop(SilentDropReason::MissingSubjectPolicy);
        };

        if let Err(reason) = self.check_replay(packet) {
            return BoundaryDecision::SilentDrop(reason);
        }
        if let Err(reason) = verify_packet_signature(packet) {
            return BoundaryDecision::SilentDrop(reason);
        }
        if !subject.allowed_tools.contains(packet.tool_name()) {
            return BoundaryDecision::SilentDrop(SilentDropReason::UnauthorizedTool);
        }
        if matches_ipi(packet, &subject) {
            return BoundaryDecision::SilentDrop(SilentDropReason::IndirectPromptInjection);
        }

        let packet_endpoint = parse_endpoint(packet.destination_p_address.as_str());
        let meta_host = meta.dest_host.map(normalize_host_owned);
        let host = meta_host.as_deref().or(packet_endpoint.host.as_deref());
        let ip = meta.dest_ip.or(packet_endpoint.ip);
        let port = packet_endpoint.port;

        if !subject
            .allowed_destinations
            .iter()
            .any(|dest| dest.matches(host, ip, port))
        {
            return BoundaryDecision::SilentDrop(SilentDropReason::UnauthorizedDestination);
        }

        BoundaryDecision::Allow
    }

    fn check_replay(&self, packet: &AiTcpPacket) -> Result<(), SilentDropReason> {
        let now_ms = current_unix_ms();
        let ts = packet.timestamp_utc;
        let delta = now_ms.abs_diff(ts);
        if delta > self.replay_window_ms {
            return Err(SilentDropReason::ReplayOutsideWindow);
        }

        let mut guard = self.used_nonces.lock().unwrap();
        let entries = guard.entry(packet.subject_key()).or_default();

        entries.retain(|_, seen_at| now_ms.saturating_sub(*seen_at) <= self.replay_window_ms);
        if entries.contains_key(packet.nonce.as_str()) {
            return Err(SilentDropReason::ReplayNonceReuse);
        }
        entries.insert(packet.nonce.clone(), ts);
        Ok(())
    }
}

static GLOBAL_DEFENSE_FILTER: OnceLock<Arc<DefenseFilter>> = OnceLock::new();

pub fn set_global_defense_filter(filter: Arc<DefenseFilter>) {
    let _ = GLOBAL_DEFENSE_FILTER.set(filter);
}

pub fn global_defense_filter() -> Arc<DefenseFilter> {
    GLOBAL_DEFENSE_FILTER
        .get_or_init(|| Arc::new(DefenseFilter::default()))
        .clone()
}

pub fn configure_global_defense_filter_from_env() -> Result<Option<Arc<DefenseFilter>>, String> {
    let Some(agent_id) = env_nonempty("KAIRO_DEFENSE_AGENT_ID") else {
        return Ok(None);
    };
    let session_id = env_nonempty("KAIRO_DEFENSE_SESSION_ID")
        .ok_or_else(|| "KAIRO_DEFENSE_SESSION_ID is required when agent id is set".to_string())?;
    let replay_window_ms = env_nonempty("KAIRO_DEFENSE_REPLAY_WINDOW_MS")
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|e| format!("invalid KAIRO_DEFENSE_REPLAY_WINDOW_MS: {}", e))
        })
        .transpose()?
        .unwrap_or(DEFAULT_REPLAY_WINDOW_MS);

    let mut capability = SubjectCapability::new(agent_id, session_id);
    capability.allowed_tools = env_csv("KAIRO_DEFENSE_ALLOWED_TOOLS").into_iter().collect();
    if capability.allowed_tools.is_empty() {
        return Err("KAIRO_DEFENSE_ALLOWED_TOOLS must contain at least one tool".to_string());
    }

    capability.allowed_destinations = env_csv("KAIRO_DEFENSE_ALLOWED_DESTINATIONS")
        .into_iter()
        .map(|raw| parse_allowed_destination(raw.as_str()))
        .collect::<Result<Vec<_>, _>>()?;
    if capability.allowed_destinations.is_empty() {
        return Err(
            "KAIRO_DEFENSE_ALLOWED_DESTINATIONS must contain at least one destination".to_string(),
        );
    }

    capability.blocked_arg_patterns = env_csv("KAIRO_DEFENSE_BLOCKED_PATTERNS");

    let filter = Arc::new(DefenseFilter::new(replay_window_ms));
    filter.register_subject(capability);
    set_global_defense_filter(filter.clone());
    Ok(Some(filter))
}

pub fn evaluate_packet(tracker: &AgentPidTracker, packet: &PacketMeta<'_>) -> FilterDecision {
    if let Some(fd) = packet.fd {
        if tracker.is_bypass_fd(fd) {
            return FilterDecision::FastBypass;
        }
        let Some(pid) = tracker.flow_pid(fd) else {
            return FilterDecision::SkipNonAgent;
        };
        if !tracker.is_agent_pid(pid) {
            return FilterDecision::SkipNonAgent;
        }
        return FilterDecision::DeepInspect;
    }

    if tracker.is_bypass_port(packet.source_port) {
        return FilterDecision::FastBypass;
    }
    if !tracker.is_agent_port(packet.source_port) {
        return FilterDecision::SkipNonAgent;
    }
    FilterDecision::DeepInspect
}

pub fn verify_packet_signature(packet: &AiTcpPacket) -> Result<(), SilentDropReason> {
    let sig_bytes = hex::decode(packet.signature.as_str())
        .map_err(|_| SilentDropReason::InvalidSignatureEncoding)?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| SilentDropReason::InvalidSignatureEncoding)?;

    let pk_bytes = hex::decode(packet.source_public_key.as_str())
        .map_err(|_| SilentDropReason::InvalidSignatureEncoding)?;
    let public_key = VerifyingKey::from_bytes(pk_bytes.as_slice().try_into().map_err(|_| SilentDropReason::InvalidSignatureEncoding)?)
        .map_err(|_| SilentDropReason::InvalidSignatureEncoding)?;

    let hash = packet.canonical_hash();
    public_key.verify(&hash, &signature)
        .map_err(|_| SilentDropReason::SignatureVerificationFailed)
}

pub fn encode_signature_blob_hex(signature: &[u8]) -> String {
    hex::encode(signature)
}

pub fn decode_signature_blob_hex(raw: &str) -> Result<Vec<u8>, &'static str> {
    hex::decode(raw).map_err(|_| "invalid_hex")
}

fn matches_ipi(packet: &AiTcpPacket, subject: &SubjectCapability) -> bool {
    let payload = packet.payload.to_ascii_lowercase();
    if DEFAULT_IPI_PATTERNS
        .iter()
        .any(|pattern| payload.contains(pattern))
    {
        return true;
    }

    subject
        .blocked_arg_patterns
        .iter()
        .map(|pattern| pattern.to_ascii_lowercase())
        .any(|pattern| payload.contains(pattern.as_str()))
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Default)]
struct ParsedEndpoint {
    host: Option<String>,
    ip: Option<IpAddr>,
    port: Option<u16>,
}

fn parse_endpoint(endpoint: &str) -> ParsedEndpoint {
    let mut parsed = ParsedEndpoint::default();
    let host = normalize_host_owned(endpoint);
    parsed.ip = host.parse::<IpAddr>().ok();
    parsed.host = if host.is_empty() { None } else { Some(host) };

    let body = endpoint.rsplit("://").next().unwrap_or(endpoint);
    let body = body.split('/').next().unwrap_or(body);
    parsed.port = body
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok());
    parsed
}

fn normalize_host_owned(endpoint: &str) -> String {
    endpoint
        .rsplit("://")
        .next()
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or(endpoint)
        .split(':')
        .next()
        .unwrap_or(endpoint)
        .to_string()
}

fn parse_allowed_destination(raw: &str) -> Result<AllowedDestination, String> {
    let parsed = parse_endpoint(raw);
    if parsed.host.is_none() && parsed.ip.is_none() {
        return Err(format!("invalid allowed destination: {}", raw));
    }
    Ok(AllowedDestination::new(parsed.host, parsed.ip, parsed.port))
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn env_csv(name: &str) -> Vec<String> {
    env_nonempty(name)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
