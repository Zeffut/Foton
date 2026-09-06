//! # Foton Protocol
//!
//! The core library for the Foton Minecraft server. Handles everything related to the PLAY state.
// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. This
// crate's production code carries none, and the lint keeps it that way. It is
// scoped to `not(test)` on purpose -- a panicking test is how a test reports a
// failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![expect(
    clippy::absolute_paths,
    clippy::allow_attributes_without_reason,
    clippy::match_same_arms,
    clippy::ref_option,
    clippy::unreadable_literal,
    reason = "packet definitions keep explicit protocol-oriented code and existing test/build invariants"
)]
#![cfg_attr(
    test,
    expect(
        clippy::float_cmp,
        clippy::unwrap_used,
        reason = "packet round-trip tests compare exact serialized float values and unwrap known-good buffers"
    )
)]

pub mod packet_reader;
pub mod packet_traits;
pub mod packet_writer;
pub mod packets;
pub mod utils;
