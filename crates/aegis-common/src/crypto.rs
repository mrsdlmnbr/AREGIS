//! Ed25519 signing — the mechanism behind the two-key model (spec §12.3).
//!
//! In production the Governor's key lives in the appliance HSM and never
//! leaves it (spec §19). Seed-based construction exists for the simulator and
//! tests; a seed appearing outside sim fixtures/tests is an archlint failure.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid hex seed: {0}")]
    BadSeed(String),
    #[error("invalid public key bytes")]
    BadPublicKey,
    #[error("invalid signature bytes")]
    BadSignature,
    #[error("signature verification failed")]
    Verify,
}

#[derive(Clone)]
pub struct KeyPair {
    signing: SigningKey,
}

impl KeyPair {
    pub fn from_seed_hex(seed_hex: &str) -> Result<Self, CryptoError> {
        let bytes = hex::decode(seed_hex).map_err(|e| CryptoError::BadSeed(e.to_string()))?;
        let seed: [u8; 32] = bytes
            .try_into()
            .map_err(|_| CryptoError::BadSeed("seed must be 32 bytes".into()))?;
        Ok(Self { signing: SigningKey::from_bytes(&seed) })
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.signing.sign(msg).to_bytes().to_vec()
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }
}

pub fn verify(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> Result<(), CryptoError> {
    let pk_bytes: [u8; 32] = pubkey.try_into().map_err(|_| CryptoError::BadPublicKey)?;
    let vk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| CryptoError::BadPublicKey)?;
    let sig_bytes: [u8; 64] = sig.try_into().map_err(|_| CryptoError::BadSignature)?;
    let signature = Signature::from_bytes(&sig_bytes);
    vk.verify(msg, &signature).map_err(|_| CryptoError::Verify)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED_A: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const SEED_B: &str = "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb";

    #[test]
    fn sign_verify_roundtrip() {
        let kp = KeyPair::from_seed_hex(SEED_A).unwrap();
        let sig = kp.sign(b"hold-to-authorize");
        assert!(verify(&kp.public_key_bytes(), b"hold-to-authorize", &sig).is_ok());
    }

    #[test]
    fn wrong_key_fails() {
        let a = KeyPair::from_seed_hex(SEED_A).unwrap();
        let b = KeyPair::from_seed_hex(SEED_B).unwrap();
        let sig = b.sign(b"forged approval");
        assert!(verify(&a.public_key_bytes(), b"forged approval", &sig).is_err());
    }

    #[test]
    fn tampered_message_fails() {
        let kp = KeyPair::from_seed_hex(SEED_A).unwrap();
        let sig = kp.sign(b"ANNOUNCE mission-1");
        assert!(verify(&kp.public_key_bytes(), b"ANNOUNCE mission-2", &sig).is_err());
    }
}
