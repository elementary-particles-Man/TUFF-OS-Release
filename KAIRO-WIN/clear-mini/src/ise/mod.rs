use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct Sig {
    pub nonce: [u8; 16],
    pub sig: [u8; 32],
}

/// Legacy entrypoint name.
/// Internally migrated from ISE-style hash to CSE-compatible HMAC tag.
pub fn ise_sign(payload: &[u8]) -> Sig {
    cse_sign(payload)
}

pub fn cse_sign(payload: &[u8]) -> Sig {
    let mut nonce = [0; 16];
    OsRng.fill_bytes(&mut nonce);
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);

    let mut mac = HmacSha256::new_from_slice(&key).expect("valid HMAC key");
    mac.update(&nonce);
    mac.update(payload);
    let mut sig = [0u8; 32];
    sig.copy_from_slice(&mac.finalize().into_bytes());

    Sig { nonce, sig }
}
