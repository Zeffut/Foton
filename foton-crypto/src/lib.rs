//! Cryptographic utilities for `Foton`, focused on RSA signing and verification
//! for secure chat message validation.
//!
//! This module implements the cryptographic primitives needed for Minecraft's
//! signed chat system, including RSA key pair generation, `SHA256withRSA` signing,
//! and signature verification.
// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. This
// crate's production code carries none, and the lint keeps it that way. It is
// scoped to `not(test)` on purpose -- a panicking test is how a test reports a
// failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![expect(
    missing_docs,
    reason = "crypto has a small public surface pending API documentation"
)]
#![expect(
    clippy::absolute_paths,
    clippy::manual_let_else,
    reason = "crypto code keeps direct error-path tests and small explicit RSA type paths"
)]
#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "crypto unit tests use unwraps for direct failure diagnostics"
    )
)]

pub mod key_store;
pub mod rsa_utils;
pub mod signature;

pub use rsa_utils::{CryptError, generate_key_pair, public_key_from_bytes, public_key_to_bytes};
pub use signature::{SignatureUpdater, SignatureValidator, Signer};

/// Signing algorithm used for chat messages (`SHA256withRSA`)
pub const SIGNING_ALGORITHM: &str = "SHA256withRSA";

/// Size of RSA signatures in bytes (for 1024-bit RSA keys, signatures are 128 bytes)
/// Note: Minecraft protocol specifies 256 bytes, but 1024-bit RSA produces 128-byte signatures
pub const SIGNATURE_BYTES: usize = 128;

/// Size of RSA keys in bits
pub const RSA_KEY_BITS: usize = 1024;
