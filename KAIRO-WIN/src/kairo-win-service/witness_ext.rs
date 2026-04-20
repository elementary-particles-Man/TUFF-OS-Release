use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::secure_log::Direction;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[repr(u16)]
pub enum CauseCode {
    SilentDrop = 0x0001,
    IsolationTriggered = 0x0002,
    IngressKillMatch = 0x0003,
    InternalAnomaly = 0x0004,
    UnknownDest = 0x0005,
    KillMatch = 0x0006,
    IngressSizeExceeded = 0x0007,
    UnauthorizedListen = 0x0008,
    HookRejected = 0x0009,
    PolicyUpdated = 0x000A,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExtendedWitnessRecord {
    pub timestamp_ms: u64,
    pub direction: Direction,
    pub source: String,
    pub destination: String,
    pub payload_preview: String,
    pub cause: CauseCode,
    pub verdict: String,
    pub note: String,
}

static WITNESS_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn log_extended(
    direction: Direction,
    source: &str,
    destination: &str,
    payload: &str,
    cause: CauseCode,
) {
    inner_log(
        direction,
        source,
        destination,
        payload.len(),
        "extended_drop",
        "cause_matched",
        cause,
    );
}

pub fn log_standard(
    direction: Direction,
    source: &str,
    destination: &str,
    payload_len: usize,
    verdict: &str,
    note: &str,
) {
    inner_log(
        direction,
        source,
        destination,
        payload_len,
        verdict,
        note,
        CauseCode::SilentDrop,
    );
}

fn inner_log(
    direction: Direction,
    source: &str,
    destination: &str,
    payload_len: usize,
    verdict: &str,
    note: &str,
    cause: CauseCode,
) {
    let _guard = WITNESS_MUTEX.lock().unwrap();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let record = ExtendedWitnessRecord {
        timestamp_ms: now,
        direction,
        source: source.to_string(),
        destination: destination.to_string(),
        payload_preview: format!("len:{}", payload_len),
        cause,
        verdict: verdict.to_string(),
        note: note.to_string(),
    };

    if let Ok(json) = serde_json::to_string(&record) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("witness.log")
            .expect("failed to open witness.log");

        writeln!(file, "{}", json).expect("failed to write to witness.log");
    }

    // Zeroize sensitive temporary buffers
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use core::arch::x86_64::{__m256i, _mm256_setzero_si256, _mm256_storeu_si256};
        if is_x86_feature_detected!("avx2") {
            let zero = _mm256_setzero_si256();
            let mut stack_buf = [0u8; 32];
            _mm256_storeu_si256(stack_buf.as_mut_ptr() as *mut __m256i, zero);
        }
    }
}
