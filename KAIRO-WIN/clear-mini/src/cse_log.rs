use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::witness::WitnessRecord;

type HmacSha256 = Hmac<Sha256>;

const LOG_DIR: &str = "logs_secure";
const KEY_FILE: &str = "logs_secure/clearmini_cse_keys.json";
const STATE_FILE: &str = "logs_secure/clearmini_chunk_state.json";
const MAX_ENTRIES_PER_CHUNK: usize = 512;

#[derive(Clone, Copy)]
pub struct CseKeys {
    pub enc_key: [u8; 32],
    pub mac_key: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    enc_key_hex: String,
    mac_key_hex: String,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct ChunkState {
    current_chunk: u64,
    entries_in_chunk: usize,
    sequence: u64,
}

#[derive(Serialize, Deserialize)]
struct CseRecord {
    version: String,
    nonce_hex: String,
    ciphertext_hex: String,
    mac_hex: String,
}

#[derive(Serialize)]
struct LoggedWitness {
    sequence: u64,
    record: WitnessRecord,
}

struct CseEnvelope {
    nonce: [u8; 16],
    ciphertext: Vec<u8>,
    mac: [u8; 32],
}

struct Inner {
    keys: CseKeys,
    state: ChunkState,
}

pub struct CseChunkLogger {
    inner: Mutex<Inner>,
}

impl CseChunkLogger {
    pub fn new() -> Result<Self, String> {
        ensure_dir()?;
        let keys = load_or_init_keys()?;
        let state = load_or_init_state()?;
        Ok(Self {
            inner: Mutex::new(Inner { keys, state }),
        })
    }

    pub fn append_witness(&self, record: &WitnessRecord) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "cse chunk logger lock poisoned".to_string())?;

        guard.state.sequence = guard.state.sequence.saturating_add(1);
        if guard.state.entries_in_chunk >= MAX_ENTRIES_PER_CHUNK {
            guard.state.current_chunk = guard.state.current_chunk.saturating_add(1);
            guard.state.entries_in_chunk = 0;
        }

        let existing = read_latest_plaintext(guard.state.current_chunk, &guard.keys)?;
        let line = serde_json::to_string(&LoggedWitness {
            sequence: guard.state.sequence,
            record: record.clone(),
        })
        .map_err(|e| e.to_string())?;

        let next = if existing.trim().is_empty() {
            line
        } else {
            format!("{existing}\n{line}")
        };

        write_rotated_chunk(guard.state.current_chunk, next.as_bytes(), &guard.keys)?;
        guard.state.entries_in_chunk = guard.state.entries_in_chunk.saturating_add(1);
        save_state(guard.state)
    }
}

fn ensure_dir() -> Result<(), String> {
    fs::create_dir_all(LOG_DIR).map_err(|e| e.to_string())
}

fn load_or_init_keys() -> Result<CseKeys, String> {
    let path = Path::new(KEY_FILE);
    if path.exists() {
        let raw = fs::read(path).map_err(|e| e.to_string())?;
        let stored: StoredKeys = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let enc_vec = hex::decode(stored.enc_key_hex).map_err(|e| e.to_string())?;
        let mac_vec = hex::decode(stored.mac_key_hex).map_err(|e| e.to_string())?;
        if enc_vec.len() != 32 || mac_vec.len() != 32 {
            return Err("invalid key length".to_string());
        }
        let mut enc_key = [0u8; 32];
        let mut mac_key = [0u8; 32];
        enc_key.copy_from_slice(&enc_vec);
        mac_key.copy_from_slice(&mac_vec);
        return Ok(CseKeys { enc_key, mac_key });
    }

    let keys = generate_keys();
    let stored = StoredKeys {
        enc_key_hex: hex::encode(keys.enc_key),
        mac_key_hex: hex::encode(keys.mac_key),
    };
    fs::write(
        path,
        serde_json::to_vec_pretty(&stored).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(keys)
}

fn load_or_init_state() -> Result<ChunkState, String> {
    let path = Path::new(STATE_FILE);
    if path.exists() {
        let raw = fs::read(path).map_err(|e| e.to_string())?;
        return serde_json::from_slice(&raw).map_err(|e| e.to_string());
    }
    let state = ChunkState {
        current_chunk: 0,
        entries_in_chunk: 0,
        sequence: 0,
    };
    save_state(state)?;
    Ok(state)
}

fn save_state(state: ChunkState) -> Result<(), String> {
    fs::write(
        STATE_FILE,
        serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn chunk_path(chunk_id: u64, generation: u8) -> PathBuf {
    PathBuf::from(format!(
        "{LOG_DIR}/clearmini_chunk_{chunk_id:08}.j{generation}.cse"
    ))
}

fn read_latest_plaintext(chunk_id: u64, keys: &CseKeys) -> Result<String, String> {
    let path = chunk_path(chunk_id, 0);
    if !path.exists() {
        return Ok(String::new());
    }
    let raw = fs::read(path).map_err(|e| e.to_string())?;
    let record: CseRecord = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let env = parse_record(record)?;
    let plain = decrypt(&env, keys)?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}

fn write_rotated_chunk(chunk_id: u64, plaintext: &[u8], keys: &CseKeys) -> Result<(), String> {
    let j0 = chunk_path(chunk_id, 0);
    let j1 = chunk_path(chunk_id, 1);
    let j2 = chunk_path(chunk_id, 2);

    if j2.exists() {
        fs::remove_file(&j2).map_err(|e| e.to_string())?;
    }
    if j1.exists() {
        fs::rename(&j1, &j2).map_err(|e| e.to_string())?;
    }
    if j0.exists() {
        fs::rename(&j0, &j1).map_err(|e| e.to_string())?;
    }

    let env = encrypt(plaintext, keys);
    let out = CseRecord {
        version: "CSE1-ETM".to_string(),
        nonce_hex: hex::encode(env.nonce),
        ciphertext_hex: hex::encode(env.ciphertext),
        mac_hex: hex::encode(env.mac),
    };
    fs::write(j0, serde_json::to_vec(&out).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn parse_record(record: CseRecord) -> Result<CseEnvelope, String> {
    if record.version != "CSE1-ETM" {
        return Err("unsupported CSE version".to_string());
    }
    let nonce_vec = hex::decode(record.nonce_hex).map_err(|e| e.to_string())?;
    let ciphertext = hex::decode(record.ciphertext_hex).map_err(|e| e.to_string())?;
    let mac_vec = hex::decode(record.mac_hex).map_err(|e| e.to_string())?;
    if nonce_vec.len() != 16 || mac_vec.len() != 32 {
        return Err("invalid CSE envelope lengths".to_string());
    }

    let mut nonce = [0u8; 16];
    let mut mac = [0u8; 32];
    nonce.copy_from_slice(&nonce_vec);
    mac.copy_from_slice(&mac_vec);
    Ok(CseEnvelope {
        nonce,
        ciphertext,
        mac,
    })
}

fn generate_keys() -> CseKeys {
    let mut enc_key = [0u8; 32];
    let mut mac_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut enc_key);
    rand::thread_rng().fill_bytes(&mut mac_key);
    CseKeys { enc_key, mac_key }
}

fn encrypt(plaintext: &[u8], keys: &CseKeys) -> CseEnvelope {
    let mut nonce = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = xor_keystream(plaintext, &keys.enc_key, &nonce);
    let mac = compute_mac(&keys.mac_key, &nonce, &ciphertext);
    CseEnvelope {
        nonce,
        ciphertext,
        mac,
    }
}

fn decrypt(env: &CseEnvelope, keys: &CseKeys) -> Result<Vec<u8>, String> {
    let expected = compute_mac(&keys.mac_key, &env.nonce, &env.ciphertext);
    if expected != env.mac {
        return Err("CSE MAC verification failed".to_string());
    }
    Ok(xor_keystream(&env.ciphertext, &keys.enc_key, &env.nonce))
}

fn xor_keystream(data: &[u8], enc_key: &[u8; 32], nonce: &[u8; 16]) -> Vec<u8> {
    let mut out = vec![0u8; data.len()];
    let mut counter: u64 = 0;
    let mut offset = 0usize;

    while offset < data.len() {
        let mut hasher = Sha256::new();
        hasher.update(enc_key);
        hasher.update(nonce);
        hasher.update(counter.to_le_bytes());
        let block = hasher.finalize();

        let block_len = usize::min(32, data.len() - offset);
        for i in 0..block_len {
            out[offset + i] = data[offset + i] ^ block[i];
        }
        offset += block_len;
        counter = counter.saturating_add(1);
    }

    out
}

fn compute_mac(mac_key: &[u8; 32], nonce: &[u8; 16], ciphertext: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(mac_key).expect("valid HMAC key");
    mac.update(nonce);
    mac.update(ciphertext);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}
