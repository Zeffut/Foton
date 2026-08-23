//! Non-living entity implementations grouped by behavior.

mod area_effect_cloud;
pub mod display_ui;
pub mod explosives;
pub mod items;
mod lightning_bolt;
mod marker;
pub mod projectiles;
pub mod vehicles;

pub use area_effect_cloud::AreaEffectCloudEntity;
pub use lightning_bolt::LightningBoltEntity;
pub(crate) use lightning_bolt::entity_thunder_hit;
pub use marker::MarkerEntity;
