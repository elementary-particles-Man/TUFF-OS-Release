use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use kairo_lib::packet::AiTcpPacket;
use once_cell::sync::Lazy;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

const DEFAULT_LOG_DIR: &str = "logs_secure";
const SEED_FILE: &str = "audit_signing_seed.hex";
const STATE_FILE: &str = "audit_state.json";
const CHUNK_PREFIX: &str = "audit_";
const CHUNK_EXT: &str = "chunk";
const CHUNK_MAGIC: &[u8; 8] = b"KAIROLG1";
const CHUNK_VERSION: u16 = 1;
const CHUNK_SIZE: usize = 4096;
const HEADER_SIZE: usize = 256;
const SIGNATURE_LEN: usize = 64; // Ed25519 signature length

static LOG_STATE: Lazy<Mutex<SecureChunkLogger>> =
    Lazy::new(|| Mutex::new(SecureChunkLogger::load_or_init().expect("secure logger init failed")));

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    InToOut,
    OutToIn,
    Internal,
}

impl Direction {
    fn as_u8(self) -> u8 {
        match self {
            Self::InToOut => 0,
            Self::OutToIn => 1,
            Self::Internal => 2,
        }
    }

    fn from_u8(raw: u8) -> Result<Self, String> {
        match raw {
            0 => Ok(Self::InToOut),
            1 => Ok(Self::OutToIn),
            2 => Ok(Self::Internal),
            _ => Err("invalid direction".to_string()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PacketEvent<'a> {
    pub direction: Direction,
    pub source: &'a str,
    pub destination: &'a str,
    pub payload_len: usize,
    pub verdict: &'a str,
    pub note: &'a str,
}

#[derive(Serialize, Deserialize)]
struct ChunkState {
    next_sequence: u64,
    previous_entry_hash_hex: String,
}

#[derive(Debug, Clone)]
pub struct SignedAuditChunk {
    pub sequence: u64,
    pub timestamp_unix_ms: u64,
    pub direction: Direction,
    pub result_code: u16,
    pub subject_hash: [u8; 32],
    pub tool_hash: [u8; 32],
    pub args_hash: [u8; 32],
    pub output_hash: [u8; 32],
    pub previous_entry_hash: [u8; 32],
    pub entry_hash: [u8; 32],
    pub signer_public_key_hash: [u8; 32],
    pub signature: [u8; SIGNATURE_LEN],
}

use crate::vulkan_gpu::{
    global_backend, VulkanBackendState, VulkanBatchSubmission, VulkanExecutionPath,
    VulkanQueueClass, VulkanWorkloadClass,
};

pub struct DropRecord {
    pub direction: Direction,
    pub result_code: u16,
    pub subject_hash: [u8; 32],
    pub tool_hash: [u8; 32],
    pub args_hash: [u8; 32],
    pub outcome: String,
}

/// 一括署名（Bulk Signing）のスタブ化
pub async fn bulk_sign_and_log(drops: &[DropRecord]) -> Result<(), String> {
    run_gpu_audit_prefilter(drops).await;

    // TODO: Implement actual bulk signing logic using Ed25519 if required
    log::info!("KAIRO-Daemon: Bulk Signing (Standalone mode)");

    Ok(())
}

async fn run_gpu_audit_prefilter(drops: &[DropRecord]) {
    let backend = global_backend();
    if matches!(backend.state(), VulkanBackendState::Uninitialized) {
        let _ = backend.initialize();
    }

    let bytes = drops
        .len()
        .saturating_mul(core::mem::size_of::<DropRecord>());
    if bytes == 0 {
        return;
    }

    let workload = if drops.len() >= 64 {
        VulkanWorkloadClass::MaintenanceHashing
    } else {
        VulkanWorkloadClass::AuditScan
    };
    let handle = backend.submit_batch(VulkanBatchSubmission {
        workload,
        queue: VulkanQueueClass::ComputeOnly,
        payload_len: bytes,
        surface_words: Some(audit_prefilter_surface_words(drops)),
        timeout: std::time::Duration::from_millis(250),
        requires_zeroize: false,
        allows_gpu: true,
        is_boot_or_recovery_path: false,
        is_truth_boundary: false,
        is_single_block_sync: false,
    });
    let result = backend.wait_for_completion(handle).await;
    match result.path {
        VulkanExecutionPath::Vulkan => log::debug!(
            "KAIRO-Daemon: audit prefilter handled by Vulkan backend submission_id={} observation={:?}",
            handle.id(),
            result.audit_observation
        ),
        VulkanExecutionPath::CpuFallback => log::debug!(
            "KAIRO-Daemon: audit prefilter fell back to CPU submission_id={} reason={:?}",
            handle.id(),
            result.fallback_reason
        ),
    }
}

fn audit_prefilter_surface_words(drops: &[DropRecord]) -> Vec<u32> {
    let mut packer = SurfaceWordPacker::with_capacity(drops.len().saturating_mul(48));
    for drop in drops {
        packer.push(drop.direction.as_u8());
        packer.push(0);
        packer.extend(&drop.result_code.to_le_bytes());
        packer.extend(&drop.subject_hash[..8]);
        packer.extend(&drop.tool_hash[..8]);
        packer.extend(&drop.args_hash[..8]);
        packer.extend(&(drop.outcome.len() as u32).to_le_bytes());
        packer.extend(
            drop.outcome
                .as_bytes()
                .get(..16)
                .unwrap_or(drop.outcome.as_bytes()),
        );
    }
    packer.finish()
}

struct SurfaceWordPacker {
    words: Vec<u32>,
    partial: [u8; 4],
    partial_len: usize,
}

impl SurfaceWordPacker {
    fn with_capacity(byte_len: usize) -> Self {
        Self {
            words: Vec::with_capacity(byte_len.div_ceil(4).max(1)),
            partial: [0; 4],
            partial_len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        self.partial[self.partial_len] = byte;
        self.partial_len += 1;
        if self.partial_len == 4 {
            self.words.push(u32::from_le_bytes(self.partial));
            self.partial = [0; 4];
            self.partial_len = 0;
        }
    }

    fn extend(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push(byte);
        }
    }

    fn finish(mut self) -> Vec<u32> {
        if self.partial_len > 0 {
            self.words.push(u32::from_le_bytes(self.partial));
        }
        if self.words.is_empty() {
            self.words.push(0);
        }
        self.words
    }
}

pub fn append_packet_event(event: PacketEvent<'_>) -> Result<(), String> {
    let subject_hash = hash_fields(&[event.source, event.destination]);
    let tool_hash = hash_fields(&[event.verdict]);
    let args_hash = hash_fields(&[event.note]);
    let output_hash = hash_u64(event.payload_len as u64);

    let mut guard = LOG_STATE
        .lock()
        .map_err(|_| "secure logger lock poisoned".to_string())?;
    guard.append_hash_only_event(
        event.direction,
        0x0100,
        subject_hash,
        tool_hash,
        args_hash,
        output_hash,
    )
}

pub fn append_packet_event_owned(
    direction: Direction,
    source: &str,
    destination: &str,
    payload_len: usize,
    verdict: String,
    note: String,
) -> Result<(), String> {
    append_packet_event(PacketEvent {
        direction,
        source,
        destination,
        payload_len,
        verdict: verdict.as_str(),
        note: note.as_str(),
    })
}

pub fn append_tool_audit(
    packet: &AiTcpPacket,
    result_code: u16,
    outcome: &str,
) -> Result<(), String> {
    let mut guard = LOG_STATE
        .lock()
        .map_err(|_| "secure logger lock poisoned".to_string())?;
    guard.append_hash_only_event(
        Direction::Internal,
        result_code,
        packet.subject_hash(),
        hash_fields(&[packet.tool_name()]),
        hash_fields(&[packet.tool_args()]),
        hash_fields(&[outcome]),
    )
}

pub fn load_signed_chunk(sequence: u64) -> Result<SignedAuditChunk, String> {
    let path = chunk_path(sequence);
    let raw = fs::read(&path).map_err(|e| format!("read {:?}: {}", path, e))?;
    decode_chunk(&raw)
}

pub fn signing_public_key_hex() -> Result<String, String> {
    let guard = LOG_STATE
        .lock()
        .map_err(|_| "secure logger lock poisoned".to_string())?;
    Ok(hex::encode(guard.public_key()))
}

impl SignedAuditChunk {
    pub fn verify_with_public_key(&self, public_key: &[u8]) -> bool {
        let pk = VerifyingKey::from_bytes(public_key.try_into().unwrap()).unwrap();
        let signature = Signature::from_bytes(&self.signature);
        pk.verify(&self.entry_hash, &signature).is_ok()
    }
}

struct SecureChunkLogger {
    seed: [u8; 32],
    state: ChunkState,
}

impl SecureChunkLogger {
    fn load_or_init() -> Result<Self, String> {
        ensure_dir()?;
        let seed = load_or_init_seed()?;
        let state = load_or_init_state()?;
        Ok(Self { seed, state })
    }

    fn append_hash_only_event(
        &mut self,
        direction: Direction,
        result_code: u16,
        subject_hash: [u8; 32],
        tool_hash: [u8; 32],
        args_hash: [u8; 32],
        output_hash: [u8; 32],
    ) -> Result<(), String> {
        let sequence = self.state.next_sequence;
        let timestamp_unix_ms = unix_ms_now();
        let previous_entry_hash = decode_hash_hex(self.state.previous_entry_hash_hex.as_str())?;
        let signer_public_key_hash = Sha256::digest(self.public_key()).into();
        let entry_hash = compute_entry_hash(
            sequence,
            timestamp_unix_ms,
            direction,
            result_code,
            &subject_hash,
            &tool_hash,
            &args_hash,
            &output_hash,
            &previous_entry_hash,
            &signer_public_key_hash,
        );
        let signature = sign_entry_hash(&self.seed, &entry_hash)?;
        let chunk = SignedAuditChunk {
            sequence,
            timestamp_unix_ms,
            direction,
            result_code,
            subject_hash,
            tool_hash,
            args_hash,
            output_hash,
            previous_entry_hash,
            entry_hash,
            signer_public_key_hash,
            signature,
        };
        let encoded = encode_chunk(&chunk);
        fs::write(chunk_path(sequence), encoded).map_err(|e| format!("write chunk: {}", e))?;

        self.state.next_sequence = self.state.next_sequence.saturating_add(1);
        self.state.previous_entry_hash_hex = hex::encode(entry_hash);
        save_state(&self.state)
    }

    fn public_key(&self) -> [u8; 32] {
        let signing_key = SigningKey::from_bytes(&self.seed);
        signing_key.verifying_key().to_bytes()
    }
}

fn ensure_dir() -> Result<(), String> {
    fs::create_dir_all(log_dir()).map_err(|e| format!("create {:?}: {}", log_dir(), e))
}

fn load_or_init_seed() -> Result<[u8; 32], String> {
    let path = seed_path();
    if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| format!("read {:?}: {}", path, e))?;
        let bytes = hex::decode(raw.trim()).map_err(|e| e.to_string())?;
        if bytes.len() != 32 {
            return Err("invalid audit seed length".to_string());
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        return Ok(seed);
    }

    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    fs::write(&path, format!("{}\n", hex::encode(seed)))
        .map_err(|e| format!("write {:?}: {}", path, e))?;
    Ok(seed)
}

fn load_or_init_state() -> Result<ChunkState, String> {
    let path = state_path();
    if path.exists() {
        let raw = fs::read(&path).map_err(|e| format!("read {:?}: {}", path, e))?;
        return serde_json::from_slice(&raw).map_err(|e| e.to_string());
    }

    let state = ChunkState {
        next_sequence: 0,
        previous_entry_hash_hex: hex::encode([0u8; 32]),
    };
    save_state(&state)?;
    Ok(state)
}

fn save_state(state: &ChunkState) -> Result<(), String> {
    let raw = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    fs::write(state_path(), raw).map_err(|e| format!("write state: {}", e))
}

fn compute_entry_hash(
    sequence: u64,
    timestamp_unix_ms: u64,
    direction: Direction,
    result_code: u16,
    subject_hash: &[u8; 32],
    tool_hash: &[u8; 32],
    args_hash: &[u8; 32],
    output_hash: &[u8; 32],
    previous_entry_hash: &[u8; 32],
    signer_public_key_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CHUNK_MAGIC);
    hasher.update(CHUNK_VERSION.to_le_bytes());
    hasher.update([direction.as_u8()]);
    hasher.update(result_code.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    hasher.update(timestamp_unix_ms.to_le_bytes());
    hasher.update(subject_hash);
    hasher.update(tool_hash);
    hasher.update(args_hash);
    hasher.update(output_hash);
    hasher.update(previous_entry_hash);
    hasher.update(signer_public_key_hash);
    hasher.finalize().into()
}

fn sign_entry_hash(seed: &[u8; 32], entry_hash: &[u8; 32]) -> Result<[u8; SIGNATURE_LEN], String> {
    let signing_key = SigningKey::from_bytes(seed);
    let signature = signing_key.sign(entry_hash);
    Ok(signature.to_bytes())
}

fn encode_chunk(chunk: &SignedAuditChunk) -> [u8; CHUNK_SIZE] {
    let mut out = [0u8; CHUNK_SIZE];
    out[0..8].copy_from_slice(CHUNK_MAGIC);
    out[8..10].copy_from_slice(&CHUNK_VERSION.to_le_bytes());
    out[10] = chunk.direction.as_u8();
    out[12..14].copy_from_slice(&chunk.result_code.to_le_bytes());
    out[14..16].copy_from_slice(&(SIGNATURE_LEN as u16).to_le_bytes());
    out[16..24].copy_from_slice(&chunk.sequence.to_le_bytes());
    out[24..32].copy_from_slice(&chunk.timestamp_unix_ms.to_le_bytes());
    out[32..64].copy_from_slice(&chunk.subject_hash);
    out[64..96].copy_from_slice(&chunk.tool_hash);
    out[96..128].copy_from_slice(&chunk.args_hash);
    out[128..160].copy_from_slice(&chunk.output_hash);
    out[160..192].copy_from_slice(&chunk.previous_entry_hash);
    out[192..224].copy_from_slice(&chunk.entry_hash);
    out[224..256].copy_from_slice(&chunk.signer_public_key_hash);
    out[HEADER_SIZE..HEADER_SIZE + SIGNATURE_LEN].copy_from_slice(&chunk.signature);
    out
}

fn decode_chunk(raw: &[u8]) -> Result<SignedAuditChunk, String> {
    if raw.len() != CHUNK_SIZE {
        return Err("invalid audit chunk size".to_string());
    }
    if &raw[0..8] != CHUNK_MAGIC {
        return Err("invalid audit chunk magic".to_string());
    }
    if u16::from_le_bytes([raw[8], raw[9]]) != CHUNK_VERSION {
        return Err("invalid audit chunk version".to_string());
    }
    if u16::from_le_bytes([raw[14], raw[15]]) as usize != SIGNATURE_LEN {
        return Err("invalid audit signature length".to_string());
    }

    let mut subject_hash = [0u8; 32];
    subject_hash.copy_from_slice(&raw[32..64]);
    let mut tool_hash = [0u8; 32];
    tool_hash.copy_from_slice(&raw[64..96]);
    let mut args_hash = [0u8; 32];
    args_hash.copy_from_slice(&raw[96..128]);
    let mut output_hash = [0u8; 32];
    output_hash.copy_from_slice(&raw[128..160]);
    let mut previous_entry_hash = [0u8; 32];
    previous_entry_hash.copy_from_slice(&raw[160..192]);
    let mut entry_hash = [0u8; 32];
    entry_hash.copy_from_slice(&raw[192..224]);
    let mut signer_public_key_hash = [0u8; 32];
    signer_public_key_hash.copy_from_slice(&raw[224..256]);
    let mut signature = [0u8; SIGNATURE_LEN];
    signature.copy_from_slice(&raw[HEADER_SIZE..HEADER_SIZE + SIGNATURE_LEN]);

    Ok(SignedAuditChunk {
        sequence: u64::from_le_bytes(raw[16..24].try_into().unwrap()),
        timestamp_unix_ms: u64::from_le_bytes(raw[24..32].try_into().unwrap()),
        direction: Direction::from_u8(raw[10])?,
        result_code: u16::from_le_bytes(raw[12..14].try_into().unwrap()),
        subject_hash,
        tool_hash,
        args_hash,
        output_hash,
        previous_entry_hash,
        entry_hash,
        signer_public_key_hash,
        signature,
    })
}

fn hash_fields(fields: &[&str]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u32).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.finalize().into()
}

fn hash_u64(value: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(value.to_le_bytes());
    hasher.finalize().into()
}

fn decode_hash_hex(raw: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(raw).map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("invalid previous hash length".to_string());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn log_dir() -> PathBuf {
    std::env::var_os("KAIRO_SECURE_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_LOG_DIR))
}

fn seed_path() -> PathBuf {
    log_dir().join(SEED_FILE)
}

fn state_path() -> PathBuf {
    log_dir().join(STATE_FILE)
}

fn chunk_path(sequence: u64) -> PathBuf {
    log_dir().join(format!("{CHUNK_PREFIX}{sequence:020}.{CHUNK_EXT}"))
}
