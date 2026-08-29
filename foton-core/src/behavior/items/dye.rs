//! Vanilla `DyeItem` behavior: dyes an alive, unsheared sheep, and recolors the
//! text on a sign.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::data_components::vanilla_components::DYE;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_events::ITEM_DYE_USE;
use foton_utils::Downcast as _;
use foton_utils::types::InteractionHand;

use crate::behavior::{InteractionResult, ItemBehavior, SignApplicator};
use crate::block_entity::BlockEntity as _;
use crate::block_entity::entities::SignBlockEntity;
use crate::entity::entities::SheepEntity;
use crate::entity::{Entity, LivingEntity};
use crate::player::Player;
use crate::world::World;

/// Behavior for the sixteen dye items (`DyeItem`).
///
/// Ports vanilla `DyeItem.interactLivingEntity`: dying a sheep plays the `DYE_USE`
/// sound, sets the sheep's wool color, and consumes one dye from the stack.
/// Vanilla `DyeItem` is also a `SignApplicator`, which is what recolors sign text.
#[item_behavior(class = "DyeItem")]
pub struct DyeItem;

impl ItemBehavior for DyeItem {
    fn interact_living_entity(
        &self,
        stack: &mut ItemStack,
        _player: &Player,
        target: &dyn LivingEntity,
        _hand: InteractionHand,
    ) -> InteractionResult {
        let Some(dye_color) = stack.get(DYE).copied() else {
            return InteractionResult::Pass;
        };
        let Some(sheep) = target.downcast_ref::<SheepEntity>() else {
            return InteractionResult::Pass;
        };

        if !Entity::is_alive(sheep) || sheep.is_sheared() || sheep.color() == dye_color {
            return InteractionResult::Pass;
        }

        sheep.play_sound(&ITEM_DYE_USE, 1.0, 1.0);
        sheep.set_color(dye_color);
        stack.shrink(1);

        InteractionResult::Success
    }

    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
}

impl SignApplicator for DyeItem {
    /// Vanilla parity: `DyeItem.tryApplyToSign`. The color comes from the stack's
    /// `minecraft:dye` component, which is what tells the sixteen dyes apart.
    fn try_apply_to_sign(
        &self,
        world: &Arc<World>,
        sign: &SignBlockEntity,
        is_front_text: bool,
        stack: &ItemStack,
        _player: &Player,
    ) -> bool {
        let Some(dye) = stack.get(DYE).copied() else {
            return false;
        };
        if !sign.update_text(|text| text.set_color(dye), is_front_text) {
            return false;
        }
        world.play_sound(
            &ITEM_DYE_USE,
            SoundSource::Blocks,
            sign.get_block_pos(),
            1.0,
            1.0,
            None,
        );
        true
    }
}
