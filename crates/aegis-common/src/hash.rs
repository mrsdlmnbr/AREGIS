use sha2::{Digest, Sha256};

pub type Hash256 = [u8; 32];

pub fn sha256(bytes: &[u8]) -> Hash256 {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes))
}
