//! Shared vanilla layers of the `animal.equine` package.
//!
//! Vanilla parity: `AbstractHorse`, `AbstractChestedHorse` and `Llama`. Every
//! horse-shaped mob sits on [`AbstractHorse`]: it is what carries the saddle,
//! the temper that decides when a wild horse gives up bucking, the rearing, the
//! steering seam a rider uses, and the inventory that survives a save. The
//! chest and the caravan are the two layers built on top of it, and both are
//! shared by more than one mob, which is why they live here rather than in a
//! single entity's file.

mod abstract_horse;
mod chested_horse;
pub mod llama_layer;

pub use abstract_horse::AbstractHorse;
pub(crate) use abstract_horse::{
    AbstractHorseBase, BABY_SCALE, generate_jump_strength, generate_max_health, generate_speed,
};
pub(crate) use chested_horse::AbstractChestedHorse;
pub use llama_layer::{Llama, LlamaVariant};
pub(crate) use llama_layer::{LlamaBase, is_llama, should_follow_mommy};
