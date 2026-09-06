//! World generation noise, density functions, and surface rule runtime support.

// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. The lint
// is scoped to `not(test)` on purpose -- a panicking test is how a test reports
// a failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![feature(portable_simd)]

extern crate self as foton_worldgen;

pub use foton_utils::{BlockStateId, random};

/// Biome sources and climate samplers.
pub mod biomes;
/// Density function system for world generation.
pub mod density;
/// Noise generation utilities for world generation.
pub mod noise;
/// `state_resolver`
pub mod state_resolver;
/// structure
pub mod structure;
/// Surface rule context types for generated code.
pub mod surface;
/// utils
pub mod utils;

#[expect(warnings)]
#[rustfmt::skip]
#[path = "generated/vanilla_multi_noise.rs"]
pub mod multi_noise;

#[expect(warnings)]
#[rustfmt::skip]
#[path = "generated/vanilla_noise_parameters.rs"]
pub mod noise_parameters;

#[expect(warnings)]
#[rustfmt::skip]
#[path = "generated/vanilla_density_functions/mod.rs"]
pub mod density_functions;
