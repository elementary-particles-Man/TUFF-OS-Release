use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub struct CseKeys {
    pub enc_key: [u8; 32],
    pub mac_key: [u8; 32],
}

pub struct CseEnvelope {
    pub nonce: [u8; 16],
    pub ciphertext: Vec<u8>,
    pub mac: [u8; 32],
}

pub fn generate_keys() -> CseKeys {
    let mut enc_key = [0u8; 32];
    let mut mac_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut enc_key);
    rand::thread_rng().fill_bytes(&mut mac_key);
    CseKeys { enc_key, mac_key }
}

pub fn encrypt(plaintext: &[u8], keys: &CseKeys) -> CseEnvelope {
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

pub fn decrypt(envelope: &CseEnvelope, keys: &CseKeys) -> Result<Vec<u8>, String> {
    let expected = compute_mac(&keys.mac_key, &envelope.nonce, &envelope.ciphertext);
    if expected != envelope.mac {
        return Err("CSE MAC verification failed".to_string());
    }
    Ok(xor_keystream(
        &envelope.ciphertext,
        &keys.enc_key,
        &envelope.nonce,
    ))
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
    let mut mac = HmacSha256::new_from_slice(mac_key).expect("valid HMAC key length");
    mac.update(nonce);
    mac.update(ciphertext);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ok() {
        let keys = generate_keys();
        let plain = b"kairo secure chunk";
        let env = encrypt(plain, &keys);
        let out = decrypt(&env, &keys).expect("decrypt should succeed");
        assert_eq!(out, plain);
    }

    #[test]
    fn tamper_detected() {
        let keys = generate_keys();
        let plain = b"tamper check";
        let mut env = encrypt(plain, &keys);
        env.ciphertext[0] ^= 0xFF;
        let out = decrypt(&env, &keys);
        assert!(out.is_err());
    }
}
