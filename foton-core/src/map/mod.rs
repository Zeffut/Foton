//! Filled maps: their saved data, their markers, and where a domain keeps them.
//!
//! Vanilla parity: `net.minecraft.world.level.saveddata.maps`.

pub mod markers;
pub mod saved_data;
pub mod storage;

#[cfg(test)]
mod tests;

pub use markers::{MapBanner, MapDecoration, MapFrame};
pub use saved_data::{
    MAP_COLOR_COUNT, MAP_SIZE, MAX_SCALE, MapItemSavedData, MapPlayerSource, MapPlayerState,
    TRACKED_DECORATION_LIMIT,
};
pub use storage::{DomainMapData, MapStorage, SharedMapData};
