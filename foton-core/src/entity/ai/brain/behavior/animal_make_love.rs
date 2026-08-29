//! Vanilla `AnimalMakeLove`.

use foton_registry::entity_type::EntityTypeRef;

use super::{BrainContext, MemoryStatus, TimedBehavior, utils};
use crate::entity::ai::brain::memory::{EntityMemory, MemoryModuleId, memory_module_types};
use crate::entity::{Animal, Mob, SharedEntity};

/// Vanilla parity: `AnimalMakeLove.BREED_RANGE`.
const BREED_RANGE: f64 = 3.0;
/// Vanilla parity: `AnimalMakeLove.MIN_DURATION`.
const MIN_DURATION: i32 = 60;
/// Vanilla parity: `AnimalMakeLove.MAX_DURATION`.
const MAX_DURATION: i32 = 110;
/// Vanilla parity: the `60 + random.nextInt(50)` the child timer is seeded with.
const CHILD_DELAY_MIN: i64 = 60;
const CHILD_DELAY_SPREAD: i64 = 50;

/// Walks two animals together and spawns their child.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.AnimalMakeLove`. It
/// is the brain's half of breeding, and the reason the hoglin needs no
/// `BreedGoal`: writing `BREED_TARGET` is also what stops its fight activity.
pub struct AnimalMakeLove {
    partner_type: EntityTypeRef,
    speed_modifier: f64,
    close_enough_distance: i32,
    entry_condition: [(MemoryModuleId, MemoryStatus); 5],
    spawn_child_at_time: i64,
}

impl AnimalMakeLove {
    /// Vanilla parity: `new AnimalMakeLove(EntityType, float, int)`.
    #[must_use]
    pub const fn new(
        partner_type: EntityTypeRef,
        speed_modifier: f64,
        close_enough_distance: i32,
    ) -> Self {
        Self {
            partner_type,
            speed_modifier,
            close_enough_distance,
            entry_condition: [
                (
                    memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::BREED_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::WALK_TARGET.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::LOOK_TARGET.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::IS_PANICKING.id(),
                    MemoryStatus::ValueAbsent,
                ),
            ],
            spawn_child_at_time: 0,
        }
    }

    fn body_as_animal<'a>(ctx: &'a BrainContext<'a>) -> Option<&'a dyn Animal> {
        ctx.mob().as_entity_event_source().as_animal()
    }

    /// Vanilla parity: the private `AnimalMakeLove.findValidBreedPartner`.
    fn find_valid_breed_partner(&self, ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        let body = Self::body_as_animal(ctx)?;
        let partner_type = self.partner_type;
        ctx.brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)?
            .find_closest(|candidate| {
                if !utils::is_of_type(candidate.as_entity_event_source(), partner_type) {
                    return false;
                }
                candidate
                    .as_entity_event_source()
                    .as_animal()
                    .is_some_and(|partner| body.can_mate(partner))
            })
    }

    fn breed_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        ctx.brain()
            .get_memory(memory_module_types::BREED_TARGET)
            .and_then(|memory| memory.get())
    }
}

impl TimedBehavior for AnimalMakeLove {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (MAX_DURATION, MAX_DURATION)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::body_as_animal(ctx).is_some_and(Animal::is_in_love)
            && self.find_valid_breed_partner(ctx).is_some()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(partner) = self.find_valid_breed_partner(ctx) else {
            return;
        };
        let Some(partner_brain) = partner.as_mob().and_then(Mob::brain) else {
            return;
        };
        let Some(body) = ctx.world().get_entity_by_id(ctx.mob().id()) else {
            return;
        };

        ctx.brain().set_memory(
            memory_module_types::BREED_TARGET,
            EntityMemory::new(&partner),
        );
        partner_brain.set_memory(memory_module_types::BREED_TARGET, EntityMemory::new(&body));
        utils::lock_gaze_and_walk_to_each_other(
            &body,
            &partner,
            self.speed_modifier,
            self.close_enough_distance,
        );

        let delay = CHILD_DELAY_MIN + rand::random_range(0..CHILD_DELAY_SPREAD);
        self.spawn_child_at_time = ctx.game_time() + delay;
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(partner) = Self::breed_target(ctx) else {
            return false;
        };
        if !utils::is_of_type(partner.as_ref(), self.partner_type) {
            return false;
        }
        let Some(body) = Self::body_as_animal(ctx) else {
            return false;
        };
        let Some(partner_animal) = partner.as_entity_event_source().as_animal() else {
            return false;
        };

        partner.is_alive()
            && body.can_mate(partner_animal)
            && utils::can_see(ctx.brain(), partner.as_ref())
            && ctx.game_time() <= self.spawn_child_at_time
            && !ctx.mob().is_panicking()
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(partner) = Self::breed_target(ctx) else {
            return;
        };
        let Some(body) = ctx.world().get_entity_by_id(ctx.mob().id()) else {
            return;
        };
        utils::lock_gaze_and_walk_to_each_other(
            &body,
            &partner,
            self.speed_modifier,
            self.close_enough_distance,
        );

        if partner.position().distance_squared(ctx.mob().position()) >= BREED_RANGE * BREED_RANGE
            || ctx.game_time() < self.spawn_child_at_time
        {
            return;
        }

        let Some(body_animal) = Self::body_as_animal(ctx) else {
            return;
        };
        let Some(partner_animal) = partner.as_entity_event_source().as_animal() else {
            return;
        };
        body_animal.spawn_child_from_breeding(ctx.world(), partner_animal);

        ctx.brain()
            .erase_memory(memory_module_types::BREED_TARGET.id());
        if let Some(partner_brain) = partner.as_mob().and_then(Mob::brain) {
            partner_brain.erase_memory(memory_module_types::BREED_TARGET.id());
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.erase_memory(memory_module_types::BREED_TARGET.id());
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
        self.spawn_child_at_time = 0;
    }

    fn debug_name(&self) -> &'static str {
        "AnimalMakeLove"
    }
}
