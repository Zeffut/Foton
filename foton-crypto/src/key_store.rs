//! This module contains the `KeyStore` struct, which is used to store the server's encryption keys.
use rsa::{RsaPrivateKey, RsaPublicKey};

use crate::CryptError;

/// A struct that stores the server's encryption keys.
pub struct KeyStore {
    /// The server's private key.
    pub private_key: RsaPrivateKey,
    /// The server's public key in DER format.
    pub public_key_der: Vec<u8>,
}

impl KeyStore {
    /// Creates a new `KeyStore`.
    ///
    /// Both steps used to panic. Neither is provably infallible -- RSA key
    /// generation can fail, and so can DER encoding -- so they are reported
    /// instead: this runs once while the server is starting, and a startup
    /// that cannot make its keys should say so and stop, not abort.
    ///
    /// # Errors
    /// When RSA key generation fails, or the public key cannot be encoded.
    pub fn create() -> Result<Self, CryptError> {
        log::debug!("Creating encryption keys...");
        let private_key = Self::generate_private_key()?;

        let public_key = RsaPublicKey::from(&private_key);
        let public_key_der = crate::public_key_to_bytes(&public_key)?;

        Ok(Self {
            private_key,
            public_key_der,
        })
    }

    fn generate_private_key() -> Result<RsaPrivateKey, CryptError> {
        // Found out that OsRng is faster than rand::thread_rng here
        let mut rng = rand::rng();

        Ok(RsaPrivateKey::new(&mut rng, 1024)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_key_der_round_trips() {
        let ks = KeyStore::create().unwrap();
        let decoded = crate::public_key_from_bytes(&ks.public_key_der).unwrap();
        assert_eq!(decoded, RsaPublicKey::from(&ks.private_key));
    }
}
