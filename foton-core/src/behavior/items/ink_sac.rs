//! Ink sac behaviors: the two items that turn a sign's glow on and off.
//!
//! Vanilla parity: `InkSacItem` and `GlowInkSacItem`. Both exist only as
//! `SignApplicator`s -- neither overrides `useOn` -- so everything they do is
//! reached through `SignBlock.useItemOn`, which owns the waxed check, the item
//! consumption and the game event.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_events::{ITEM_GLOW_INK_SAC_USE, ITEM_INK_SAC_USE};

use crate::behavior::{ItemBehavior, SignApplicator};
use crate::block_entity::BlockEntity as _;
use crate::block_entity::entities::SignBlockEntity;
use crate::player::Player;
use crate::world::World;

/// Behavior for the ink sac (`InkSacItem`): takes the glow off a sign.
#[item_behavior]
pub struct InkSacItem;

impl ItemBehavior for InkSacItem {
    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
}

impl SignApplicator for InkSacItem {
    /// Vanilla parity: `InkSacItem.tryApplyToSign`.
    fn try_apply_to_sign(
        &self,
        world: &Arc<World>,
        sign: &SignBlockEntity,
        is_front_text: bool,
        _stack: &ItemStack,
        _player: &Player,
    ) -> bool {
        if !sign.update_text(|text| text.set_has_glowing_text(false), is_front_text) {
            return false;
        }
        world.play_sound(
            &ITEM_INK_SAC_USE,
            SoundSource::Blocks,
            sign.get_block_pos(),
            1.0,
            1.0,
            None,
        );
        true
    }
}

/// Behavior for the glow ink sac (`GlowInkSacItem`): makes a sign's text glow.
#[item_behavior]
pub struct GlowInkSacItem;

impl ItemBehavior for GlowInkSacItem {
    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
}

impl SignApplicator for GlowInkSacItem {
    /// Vanilla parity: `GlowInkSacItem.tryApplyToSign`.
    fn try_apply_to_sign(
        &self,
        world: &Arc<World>,
        sign: &SignBlockEntity,
        is_front_text: bool,
        _stack: &ItemStack,
        _player: &Player,
    ) -> bool {
        if !sign.update_text(|text| text.set_has_glowing_text(true), is_front_text) {
            return false;
        }
        world.play_sound(
            &ITEM_GLOW_INK_SAC_USE,
            SoundSource::Blocks,
            sign.get_block_pos(),
            1.0,
            1.0,
            None,
        );
        true
    }
}
