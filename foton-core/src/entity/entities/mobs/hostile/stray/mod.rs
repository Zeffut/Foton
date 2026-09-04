//! Stray entity.
//!
//! Vanilla parity: `Stray`. A snowy skeleton that shoots and seeks shade just
//! like its cousin.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::StrayEntityData;
use foton_registry::{sound_events, vanilla_mob_effects};
use foton_utils::locks::SyncMutex;
use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::LivingEntitySyncedData;
use crate::entity::SpawnGroupData;
use crate::entity::ai::goal::{
    FleeSunGoal, HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RangedBowAttackGoal, RestrictSunGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ArrowEntity;
use crate::entity::spawn_rules::check_stray_spawn_rules;
use crate::entity::weapon_holding_hand;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, MobEffectInstance, PathfinderMob,
};
use crate::world::World;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_items;
use foton_utils::BlockPos;
use foton_utils::types::InteractionHand;
use std::sync::Arc;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Ticks between shots.
///
/// Vanilla parity: the `attackIntervalMin` a skeleton is built with on normal
/// difficulty.
const ATTACK_INTERVAL_TICKS: i32 = 20;

/// Range within which a skeleton will loose an arrow.
///
/// Vanilla parity: the `attackRadius` of `RangedBowAttackGoal`.
const ATTACK_RADIUS: f64 = 15.0;

/// Speed of the arrows a skeleton fires.
///
/// Vanilla parity: the `1.6F` velocity of `performRangedAttack`.
const ARROW_POWER: f32 = 1.6;

/// Spread of the arrows a skeleton fires on normal difficulty.
///
/// Vanilla parity: `14 - difficulty * 4`, with difficulty 2.
const ARROW_UNCERTAINTY: f32 = 6.0;

/// Ticks of slowness a stray's arrow carries.
///
/// Vanilla parity: the `MobEffectInstance(MobEffects.SLOWNESS, 600)` of
/// `Stray.getArrow`.
const SLOWNESS_TICKS: i32 = 600;

/// Speed multiplier while repositioning.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Speed the archer closes the distance at.
///
/// Vanilla parity: the `1.0` speed modifier `AbstractSkeleton` builds its bow
/// goal with.
const BOW_APPROACH_SPEED: f64 = 1.0;

/// Distance at which a skeleton watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// A stray.
#[entity_behavior(class = "Stray")]
pub struct StrayEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<StrayEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `StrayEntity`.
unsafe impl DowncastType for StrayEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/stray");
}

impl StrayEntity {
    /// Creates a stray at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a stray from saved base data.
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
        let mut entity_data = StrayEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // A stray keeps the AbstractSkeleton goal set unchanged.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(2, RestrictSunGoal::new());
            goals.add_goal(3, FleeSunGoal::new(1.0));
            goals.add_goal(
                4,
                RangedBowAttackGoal::new(
                    ATTACK_INTERVAL_TICKS,
                    ATTACK_RADIUS,
                    BOW_APPROACH_SPEED,
                    fire_arrow,
                ),
            );
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(6, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(6, RandomLookAroundGoal::new());
            // TODO: vanilla also flees wolves at priority 3.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

/// Looses an arrow at `target`.
///
///
/// Vanilla parity: `AbstractSkeleton.performRangedAttack`.
fn fire_arrow(mob: &dyn PathfinderMob, target: DVec3) {
    let Some(archer) = mob.downcast_ref::<StrayEntity>() else {
        return;
    };
    let Some(world) = archer.level() else {
        return;
    };
    // Vanilla parity: `AbstractSkeleton.performRangedAttack` reads the bow out
    // of whichever hand holds one and hands it to `ProjectileUtil.getMobArrow`,
    // so the arrow can read Power and Flame off it when it lands. Nothing
    // leaves a quiver: `Monster.getProjectile` conjures the arrow when the mob
    // carries none, which is why a skeleton never runs out.
    let bow = archer.get_item_in_hand(weapon_holding_hand(archer, &vanilla_items::BOW));
    let arrow = ArrowEntity::shoot_at(&world, archer, target, ARROW_POWER, ARROW_UNCERTAINTY);
    if bow.is(&vanilla_items::BOW) {
        arrow.set_fired_from_weapon(Some(bow));
    }
    // Vanilla parity: `Stray.getArrow` tips every arrow with slowness.
    arrow.add_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::SLOWNESS,
        SLOWNESS_TICKS,
        0,
    ));

    world.play_sound_at(
        &sound_events::ENTITY_SKELETON_SHOOT,
        SoundSource::Hostile,
        archer.position(),
        1.0,
        0.4f32.mul_add(rand::random::<f32>(), 0.8).recip(),
        None,
    );
}

impl Entity for StrayEntity {
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

    /// Vanilla parity: `Stray` adds nothing to `AbstractSkeleton`, which adds
    /// nothing to `Mob`, so the shared half is the whole of it.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for StrayEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    /// Without this the goal selector is never ticked and every goal this mob
    /// registers is dead code.
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_STRAY_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_STRAY_DEATH)
    }
}

impl Mob for StrayEntity {
    /// Arms the skeleton with the bow it shoots with.
    ///
    /// Vanilla parity: `AbstractSkeleton.finalizeSpawn`, which runs the shared
    /// `Mob.finalizeSpawn` and then `populateDefaultEquipmentSlots` -- and that
    /// is the only place a skeleton's bow comes from.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = self.finalize_spawn_mob_base(world, spawn_reason, group_data);
        // Vanilla parity: the `setCanPickUpLoot` of
        // `AbstractSkeleton.finalizeSpawn`, which is what lets a skeleton pick
        // your bow up off the ground.
        self.roll_spawn_can_pick_up_loot(world);
        self.set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::BOW),
        );
        group_data
    }
    /// Vanilla parity: `Stray` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `Stray::checkStraySpawnRules`, which looks for sky past
    /// any powder snow piled on top of the spot.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_stray_spawn_rules(world, spawn_reason, pos)
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

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_STRAY_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for StrayEntity {}

impl Enemy for StrayEntity {}
