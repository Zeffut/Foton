//! all the math of foton

// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. This
// crate's production code carries none, and the lint keeps it that way. It is
// scoped to `not(test)` on purpose -- a panicking test is how a test reports a
// failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![feature(portable_simd)]
/// Math utilities used by vanilla world generation noise.
mod noise_math;
/// SIMD-based utility functions for matrix transpositions and vector manipulations.
#[cfg(not(target_feature = "avx512f"))]
mod simd_utils;
pub mod trig;

pub use crate::noise_math::*;
