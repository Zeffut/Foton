//! The three things a map can draw on top of its colors: a decoration, the
//! banner that produced one, and the item frame that produced one.

use std::sync::Arc;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::dye_color::DyeColor;
use steel_registry::map_decoration_type::MapDecorationTypeRef;
use steel_registry::vanilla_map_decoration_types as decoration_types;
use steel_utils::{BlockPos, Downcast as _};
use text_components::TextComponent;

use crate::block_entity::entities::BannerBlockEntity;
use crate::world::{LevelReader as _, World};

/// One marker drawn on a map.
///
/// Vanilla parity: `net.minecraft.world.level.saveddata.maps.MapDecoration`.
#[derive(Debug, Clone, PartialEq)]
pub struct MapDecoration {
    /// Registry entry deciding the sprite, the tracking rules and the tint.
    pub decoration_type: MapDecorationTypeRef,
    /// Horizontal position on the map, two units per pixel.
    pub x: i8,
    /// Vertical position on the map, two units per pixel.
    pub y: i8,
    /// Facing, in sixteenths of a turn.
    pub rot: i8,
    /// Label the client draws beside the marker.
    pub name: Option<TextComponent>,
}

impl MapDecoration {
    /// Vanilla parity: the compact constructor, which masks the rotation into
    /// the sixteen directions the client can draw.
    #[must_use]
    pub const fn new(
        decoration_type: MapDecorationTypeRef,
        x: i8,
        y: i8,
        rot: i8,
        name: Option<TextComponent>,
    ) -> Self {
        Self {
            decoration_type,
            x,
            y,
            rot: rot & 15,
            name,
        }
    }
}

/// A banner a player marked on a map by right-clicking it.
///
/// Vanilla parity: `MapBanner`.
#[derive(Debug, Clone, PartialEq)]
pub struct MapBanner {
    /// Position of the banner block this marker follows.
    pub pos: BlockPos,
    /// Base color of that banner, which picks the marker sprite.
    pub color: DyeColor,
    /// The banner's custom name, drawn as the marker's label.
    pub name: Option<TextComponent>,
}

impl MapBanner {
    /// Builds a banner marker from its parts.
    #[must_use]
    pub const fn new(pos: BlockPos, color: DyeColor, name: Option<TextComponent>) -> Self {
        Self { pos, color, name }
    }

    /// Reads the banner standing at `pos`, or `None` if there is none.
    ///
    /// Vanilla parity: `MapBanner.fromWorld`. Vanilla asks the block entity for
    /// its base color; Steel's banner block entity does not carry one, so the
    /// color is read off the block itself -- which is where vanilla's
    /// `AbstractBannerBlock.color` comes from in the first place.
    #[must_use]
    pub fn from_world(world: &Arc<World>, pos: BlockPos) -> Option<Self> {
        let block_entity = world.get_block_entity(pos)?;
        let banner = block_entity.downcast_ref::<BannerBlockEntity>()?;
        let color = banner_base_color(world, pos)?;
        Some(Self::new(pos, color, banner.custom_name()))
    }

    /// Vanilla parity: `MapBanner.getDecoration`.
    #[must_use]
    pub fn decoration(&self) -> MapDecorationTypeRef {
        match self.color {
            DyeColor::White => &decoration_types::BANNER_WHITE,
            DyeColor::Orange => &decoration_types::BANNER_ORANGE,
            DyeColor::Magenta => &decoration_types::BANNER_MAGENTA,
            DyeColor::LightBlue => &decoration_types::BANNER_LIGHT_BLUE,
            DyeColor::Yellow => &decoration_types::BANNER_YELLOW,
            DyeColor::Lime => &decoration_types::BANNER_LIME,
            DyeColor::Pink => &decoration_types::BANNER_PINK,
            DyeColor::Gray => &decoration_types::BANNER_GRAY,
            DyeColor::LightGray => &decoration_types::BANNER_LIGHT_GRAY,
            DyeColor::Cyan => &decoration_types::BANNER_CYAN,
            DyeColor::Purple => &decoration_types::BANNER_PURPLE,
            DyeColor::Blue => &decoration_types::BANNER_BLUE,
            DyeColor::Brown => &decoration_types::BANNER_BROWN,
            DyeColor::Green => &decoration_types::BANNER_GREEN,
            DyeColor::Red => &decoration_types::BANNER_RED,
            DyeColor::Black => &decoration_types::BANNER_BLACK,
        }
    }

    /// Vanilla parity: `MapBanner.getId`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("banner-{},{},{}", self.pos.x(), self.pos.y(), self.pos.z())
    }
}

/// Reads a banner block's dye color out of its registry key.
///
/// Vanilla stores it on `AbstractBannerBlock`; every banner block is named
/// `<color>_banner` or `<color>_wall_banner`, which is the same information.
fn banner_base_color(world: &Arc<World>, pos: BlockPos) -> Option<DyeColor> {
    let path = world.get_block_state(pos).get_block().key.path.clone();
    let color = path
        .strip_suffix("_wall_banner")
        .or_else(|| path.strip_suffix("_banner"))?;
    DyeColor::from_serialized_name(color)
}

/// An item frame that a map has noticed itself hanging in.
///
/// Vanilla parity: `MapFrame`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapFrame {
    /// Position of the item frame block.
    pub pos: BlockPos,
    /// Frame facing in degrees: vanilla's 2D direction value times ninety.
    pub rotation: i32,
    /// Runtime entity id of the frame, which keys its decoration.
    pub entity_id: i32,
}

impl MapFrame {
    /// Builds a frame marker from its parts.
    #[must_use]
    pub const fn new(pos: BlockPos, rotation: i32, entity_id: i32) -> Self {
        Self {
            pos,
            rotation,
            entity_id,
        }
    }

    /// Vanilla parity: `MapFrame.getId`.
    #[must_use]
    pub fn id(&self) -> String {
        Self::frame_id(self.pos)
    }

    /// Vanilla parity: `MapFrame.frameId`.
    #[must_use]
    pub fn frame_id(pos: BlockPos) -> String {
        format!("frame-{},{},{}", pos.x(), pos.y(), pos.z())
    }
}
