//! Non-living entity implementations grouped by behavior.

mod area_effect_cloud;
pub mod display_ui;
pub mod explosives;
pub mod items;
mod lightning_bolt;
mod marker;
mod ominous_item_spawner;
pub mod projectiles;
pub mod vehicles;

pub use area_effect_cloud::{AreaEffectCloudEntity, CREEPER_CLOUD_DURATION_SCALE};
pub use lightning_bolt::{LightningBoltEntity, default_thunder_hit};
pub use marker::MarkerEntity;
pub use ominous_item_spawner::OminousItemSpawnerEntity;
