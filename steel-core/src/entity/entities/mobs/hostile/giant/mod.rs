//! Giant entity.
//!
//! Vanilla parity: `Giant`. The oldest mob in the game and the emptiest: it
//! never overrides `registerGoals`, so a giant stands where it spawned and does
//! nothing at all. Everything that makes it dangerous is in its attributes --
//! a hundred health and fifty attack damage -- which come from the extracted
//! entity type rather than from any code here.
//!
//! The one behaviour it does own is [`PathfinderMob::get_walk_target_value`]:
//! `Monster` returns the *negative* light cost so a monster prefers the dark,
//! and `Giant` flips the sign, so a giant prefers the light. It is the only
//! override in the class.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::GiantEntityData;
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::world::{LevelReader as _, World};

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// this class does not override.
const XP_REWARD: i32 = 5;

/// A giant.
#[entity_behavior(class = "Giant")]
pub struct GiantEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<GiantEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `GiantEntity`.
unsafe impl DowncastType for GiantEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/giant");
}

impl GiantEntity {
    /// Creates a giant at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a giant from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        mob_base.set_xp_reward(XP_REWARD);
        let mut entity_data = GiantEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        // Vanilla parity: `Giant` registers no goals. The empty selectors are
        // deliberate, not an omission.

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for GiantEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }
}

impl LivingEntity for GiantEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`. A giant has no goals, but the step
    /// also runs the move control and the navigation, so it still has to run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }
}

impl Mob for GiantEntity {
    /// Vanilla parity: `Giant` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` a giant is
    /// registered with in `SpawnPlacements`. It has a spawn rule but no spawn
    /// weight in any biome, which is why one is only ever summoned.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for GiantEntity {
    /// Scores a candidate step by how *bright* it is.
    ///
    /// Vanilla parity: `Giant.getWalkTargetValue`, which returns the light cost
    /// unnegated where `Monster.getWalkTargetValue` returns `-cost`. A giant is
    /// the one monster that walks toward the light.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };
        world.pathfinding_cost_from_light_levels(pos)
    }
}

impl Enemy for GiantEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::ChunkPos;

    use crate::entity::next_entity_id;

    fn giant(world: Weak<World>) -> GiantEntity {
        GiantEntity::new(
            &vanilla_entities::GIANT,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            world,
        )
    }

    /// A giant is the one monster that walks *toward* the light: it returns the
    /// light cost where `Monster.getWalkTargetValue` returns its negation. If
    /// the sign were copied from `Monster` this comes back inverted.
    #[test]
    fn a_giant_scores_a_block_by_the_unnegated_light_cost() {
        init_vanilla_registry();
        let world = fresh_test_world("giant_walk_target_value");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let mob = giant(Arc::downgrade(&world));

        let pos = BlockPos::new(8, 65, 8);
        let cost = world.pathfinding_cost_from_light_levels(pos);
        assert!(
            cost.abs() > f32::EPSILON,
            "the test position must have a non-zero light cost or the sign is \
             untestable; got {cost}"
        );
        assert!(
            (mob.get_walk_target_value(pos) - cost).abs() < f32::EPSILON,
            "`Giant.getWalkTargetValue` returns the light cost unnegated, not \
             `Monster`'s negation"
        );
    }

    /// Without a world there is no light to read, and vanilla's caller would
    /// never reach this method off-level. Returning zero keeps it total.
    #[test]
    fn a_giant_with_no_world_scores_every_block_zero() {
        init_vanilla_registry();
        let mob = giant(Weak::new());
        assert!(mob.get_walk_target_value(BlockPos::new(0, 64, 0)).abs() < f32::EPSILON);
    }
}
