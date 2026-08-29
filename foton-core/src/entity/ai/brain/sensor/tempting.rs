//! Vanilla `TemptingSensor`.

use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_attributes;

use super::Sensor;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::{Entity as _, LivingEntity as _, PathfinderMob, SharedEntity};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;

/// What counts as a temptation for this mob.
type Temptations = Box<dyn Fn(&dyn PathfinderMob, &ItemStack) -> bool + Send>;

/// Remembers the nearest player holding something this mob wants.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.TemptingSensor`.
pub struct TemptingSensor {
    temptations: Temptations,
}

impl TemptingSensor {
    /// Tempts a mob with whatever its own `isFood` accepts.
    ///
    /// Vanilla parity: `TemptingSensor.forAnimal`.
    #[must_use]
    pub fn for_animal() -> Self {
        Self::new(|mob, item_stack| {
            mob.as_animal()
                .is_some_and(|animal| animal.is_food(item_stack))
        })
    }

    /// Tempts a mob with a fixed set of items.
    ///
    /// Vanilla parity: `new TemptingSensor(Predicate<ItemStack>)`.
    #[must_use]
    pub fn new(
        temptations: impl Fn(&dyn PathfinderMob, &ItemStack) -> bool + Send + 'static,
    ) -> Self {
        Self {
            temptations: Box::new(temptations),
        }
    }

    fn is_holding_temptation(&self, mob: &dyn PathfinderMob, player: &Player) -> bool {
        [EquipmentSlot::MainHand, EquipmentSlot::OffHand]
            .into_iter()
            .any(|slot| {
                let mut tempting = false;
                player.with_equipment_slot(slot, &mut |item_stack| {
                    tempting = (self.temptations)(mob, item_stack);
                });
                tempting
            })
    }
}

impl Sensor for TemptingSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::TEMPTING_PLAYER.id()]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let range = body
            .attributes()
            .lock()
            .required_value(vanilla_attributes::TEMPT_RANGE);
        // Vanilla parity: `TEMPT_TARGETING`, a non-combat condition that
        // ignores line of sight so a mob follows food held behind a fence.
        let conditions = TargetingConditions::for_non_combat()
            .ignore_line_of_sight()
            .range(range);

        let tempter = ctx
            .world()
            .nearest_player(body.position(), range, |player| {
                !player.is_spectator()
                    && conditions.test(ctx.world(), Some(body), player)
                    && self.is_holding_temptation(body, player)
            });

        ctx.brain().set_memory_or_erase(
            memory_module_types::TEMPTING_PLAYER,
            tempter.map(|player| EntityMemory::new(&(player as SharedEntity))),
        );
    }
}
