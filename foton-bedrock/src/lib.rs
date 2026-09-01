//! Bedrock Edition players, joining a Java server.
//!
//! Two halves that share only a key. [`floodgate`] decodes the identity Geyser
//! puts in the handshake and is pure. `geyser` supervises the process that put
//! it there. `foton-login` depends on the first and knows nothing of the second.
//! [`key`] is that shared secret: where it lives on disk, and how it reaches
//! both sides once loaded.

pub mod floodgate;
pub mod key;
