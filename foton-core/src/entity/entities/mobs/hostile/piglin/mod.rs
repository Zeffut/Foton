//! The piglin, the piglin brute, and the brain they share half of.

mod abstract_piglin;
mod behaviors;
mod entity;
mod piglin_ai;
mod piglin_brute;
mod piglin_brute_ai;

#[cfg(test)]
mod tests;

pub use entity::PiglinEntity;
pub use piglin_ai::anger_nearby_piglins;
pub use piglin_brute::PiglinBruteEntity;
