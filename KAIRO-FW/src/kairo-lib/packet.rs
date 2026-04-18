use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PacketSubject {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReplayMetadata {
    #[serde(default)]
    pub timestamp_utc: u64,
    #[serde(default)]
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTcpPacket {
    pub source: String,
    pub destination: String,
    pub version: u8,
    pub source_p_address: String,
    pub destination_p_address: String,
    pub source_public_key: String,
    pub sequence: u64,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub timestamp_utc: u64,
    #[serde(default)]
    pub nonce: String,
    pub payload_type: String,
    pub payload: String,
    pub signature: String,
}

impl AiTcpPacket {
    pub fn subject(&self) -> PacketSubject {
        PacketSubject {
            agent_id: self.agent_id.clone(),
            session_id: self.session_id.clone(),
        }
    }

    pub fn replay_metadata(&self) -> ReplayMetadata {
        ReplayMetadata {
            timestamp_utc: self.timestamp_utc,
            nonce: self.nonce.clone(),
        }
    }

    pub fn subject_key(&self) -> String {
        format!("{}::{}", self.agent_id, self.session_id)
    }

    pub fn has_complete_identity(&self) -> bool {
        !self.agent_id.trim().is_empty() && !self.session_id.trim().is_empty()
    }

    pub fn has_complete_replay_metadata(&self) -> bool {
        self.timestamp_utc != 0 && !self.nonce.trim().is_empty()
    }

    pub fn tool_name(&self) -> &str {
        self.payload_type.as_str()
    }

    pub fn tool_args(&self) -> &str {
        self.payload.as_str()
    }

    pub fn canonical_hash(&self) -> [u8; 32] {
        #[cfg(target_arch = "x86_64")]
        {
            // --- VAES / AVX-512 Dynamic Upgrade Path ---
            // In real implementation, we use DISPATCH.has_vaes / tag == Zen5
            // to call aes_gcm_vaes_512bit_parallel(&packet) or similar.
            // This is a simulated high-speed hash path.
        }

        let mut hasher = Sha256::new();
        hasher.update([self.version]);
        hasher.update(self.sequence.to_le_bytes());
        hasher.update(self.timestamp_utc.to_le_bytes());
        update_field(&mut hasher, &self.source);
        update_field(&mut hasher, &self.destination);
        update_field(&mut hasher, &self.source_p_address);
        update_field(&mut hasher, &self.destination_p_address);
        update_field(&mut hasher, &self.source_public_key);
        update_field(&mut hasher, &self.agent_id);
        update_field(&mut hasher, &self.session_id);
        update_field(&mut hasher, &self.nonce);
        update_field(&mut hasher, &self.payload_type);
        update_field(&mut hasher, &self.payload);
        hasher.finalize().into()
    }

    pub fn subject_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        update_field(&mut hasher, &self.agent_id);
        update_field(&mut hasher, &self.session_id);
        update_field(&mut hasher, &self.source_public_key);
        hasher.finalize().into()
    }

    pub fn tool_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        update_field(&mut hasher, &self.payload_type);
        update_field(&mut hasher, &self.payload);
        update_field(&mut hasher, &self.destination_p_address);
        hasher.finalize().into()
    }
}

fn update_field(hasher: &mut Sha256, field: &str) {
    hasher.update((field.len() as u32).to_le_bytes());
    hasher.update(field.as_bytes());
}

pub type Packet = AiTcpPacket;
