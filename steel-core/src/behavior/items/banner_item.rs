//! Banner items.

use steel_macros::item_behavior;
use steel_registry::DyeColor;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::properties::Direction;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};

use super::StandingAndWallBlockItem;

/// The sixteen colored banners.
///
/// Vanilla parity: `BannerItem`, which in 26.2 is exactly
/// `StandingAndWallBlockItem(block, wallBlock, Direction.DOWN)` plus a
/// `getColor()` reading the block's dye color.
///
/// The pattern layers a banner carries are the `minecraft:banner_patterns`
/// component: the loom writes it, `BannerBlockEntity` reads it back when the
/// banner is placed, and the client draws it. `BannerItem` itself no longer
/// touches them -- the tooltip moved onto `BannerPatternLayers`, which is a
/// `TooltipProvider` and therefore client-rendered.
///
/// Steel gap: Steel has no server-side hover-text hook at all, so no item can
/// contribute tooltip lines. That is not a banner-specific hole: vanilla's
/// `ItemStack.getTooltipLines` is only ever called by the client, so a server
/// has nothing to send beyond the component it already sends.
#[item_behavior]
pub struct BannerItem {
    #[json_arg(vanilla_blocks, json = "block")]
    block: BlockRef,
    #[json_arg(vanilla_blocks, json = "wall_block")]
    _wall_block: BlockRef,
    #[json_arg(
        r#enum = "Direction",
        module = "steel_registry::blocks::properties",
        json = "attachment_direction"
    )]
    _attachment_direction: Direction,
    base: StandingAndWallBlockItem,
}

impl BannerItem {
    /// Creates a banner item behavior.
    #[must_use]
    pub const fn new(
        block: BlockRef,
        wall_block: BlockRef,
        attachment_direction: Direction,
    ) -> Self {
        Self {
            block,
            _wall_block: wall_block,
            _attachment_direction: attachment_direction,
            base: StandingAndWallBlockItem::new(block, wall_block, attachment_direction),
        }
    }

    /// Returns the banner's base color.
    ///
    /// Vanilla parity: `BannerItem.getColor`, which asks the standing block.
    #[must_use]
    pub fn color(&self) -> Option<DyeColor> {
        banner_color(self.block)
    }
}

impl ItemBehavior for BannerItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        self.base.use_on(context)
    }
}

/// Reads the dye color out of a banner block's registry key.
///
/// Vanilla parity: `AbstractBannerBlock.getColor`, a field set from the block's
/// constructor. Steel's extracted block data carries no color field, so the
/// name is the only source -- the sixteen banners are named `<color>_banner`.
fn banner_color(block: BlockRef) -> Option<DyeColor> {
    let color = block.key.path.strip_suffix("_banner")?;
    DyeColor::from_serialized_name(color)
}
