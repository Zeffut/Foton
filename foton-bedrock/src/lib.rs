//! Bedrock Edition players, joining a Java server.
//!
//! Two halves that share only a key. [`floodgate`] decodes the identity Geyser
//! puts in the handshake and is pure. [`geyser`] supervises the process that put
//! it there. `foton-login` depends on the first and knows nothing of the second.
//! [`key`] is that shared secret: where it lives on disk, and how it reaches
//! both sides once loaded.

// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. This
// crate's production code carries none, and the lint keeps it that way. It is
// scoped to `not(test)` on purpose -- a panicking test is how a test reports a
// failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic, clippy::unreachable, clippy::todo))]

pub mod config;
pub mod floodgate;
pub mod geyser;
pub mod key;
