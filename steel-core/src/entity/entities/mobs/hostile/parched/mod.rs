//! Parched entity.
//!
//! Vanilla parity: `Parched`. A desert skeleton whose arrows carry weakness and
//! which cannot be weakened itself. It keeps the whole `AbstractSkeleton` goal
//! set unchanged and only slows its bow down.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::ParchedEntityData;
use steel_registry::{sound_events, vanilla_mob_effects};
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::SpawnGroupData;
use crate::entity::ai::goal::{
    FleeSunGoal, HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RangedBowAttackGoal, RestrictSunGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ArrowEntity;
use crate::entity::spawn_rules::check_surface_monster_spawn_rules;
use crate::entity::weapon_holding_hand;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob,
};
use crate::world::World;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_items;
use steel_utils::types::InteractionHand;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Ticks between shots.
///
/// Vanilla parity: `Parched.getAttackInterval`, the value `reassessWeaponGoal`
/// installs on every difficulty below hard.
const ATTACK_INTERVAL_TICKS: i32 = 70;

/// Range within which a parched will loose an arrow.
///
/// Vanilla parity: the `attackRadius` of `AbstractSkeleton`'s bow goal.
const ATTACK_RADIUS: f64 = 15.0;

/// Speed of the arrows a parched fires.
///
/// Vanilla parity: the `1.6F` velocity of `performRangedAttack`.
const ARROW_POWER: f32 = 1.6;

/// Spread of the arrows a parched fires on normal difficulty.
///
/// Vanilla parity: `14 - difficulty * 4`, with difficulty 2.
const ARROW_UNCERTAINTY: f32 = 6.0;

/// Ticks of weakness a parched's arrow carries.
///
/// Vanilla parity: the `MobEffectInstance(MobEffects.WEAKNESS, 600)` of
/// `Parched.getArrow`.
const WEAKNESS_TICKS: i32 = 600;

/// Speed multiplier while repositioning.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Speed the archer closes the distance at.
///
/// Vanilla parity: the `1.0` speed modifier `AbstractSkeleton` builds its bow
/// goal with.
const BOW_APPROACH_SPEED: f64 = 1.0;

/// Distance at which a parched watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// A parched.
#[entity_behavior(class = "Parched")]
pub struct ParchedEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<ParchedEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ParchedEntity`.
unsafe impl DowncastType for ParchedEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/parched");
}

impl ParchedEntity {
    /// Creates a parched at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a parched from saved base data.
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
        let mut entity_data = ParchedEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // A parched keeps the AbstractSkeleton goal set unchanged.
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

/// Looses a weakening arrow at `target`.
///
/// Vanilla parity: `AbstractSkeleton.performRangedAttack` with the
/// `Parched.getArrow` override.
fn fire_arrow(mob: &dyn PathfinderMob, target: DVec3) {
    let Some(archer) = mob.downcast_ref::<ParchedEntity>() else {
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
    // Vanilla parity: `Parched.getArrow` weakens every arrow it fires.
    arrow.add_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::WEAKNESS,
        WEAKNESS_TICKS,
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

impl Entity for ParchedEntity {
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

impl LivingEntity for ParchedEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
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

    /// Refuses weakness, and nothing else.
    ///
    /// Vanilla parity: `Parched.canBeAffected`. A parched hands out the effect
    /// it is immune to, which is what stops two of them disarming each other.
    fn can_be_affected(&self, effect: &MobEffectInstance) -> bool {
        if effect.effect() == vanilla_mob_effects::WEAKNESS {
            return false;
        }
        self.default_can_be_affected(effect)
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PARCHED_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PARCHED_DEATH)
    }
}

impl Mob for ParchedEntity {
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
        self.set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::BOW),
        );
        group_data
    }
    /// Vanilla parity: `Parched` derives from `AbstractSkeleton`, and so from
    /// `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkSurfaceMonstersSpawnRules` a parched
    /// is registered with in `SpawnPlacements`, which is why one only appears
    /// under open sky.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_surface_monster_spawn_rules(world, spawn_reason, pos)
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
        Some(&sound_events::ENTITY_PARCHED_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for ParchedEntity {}

impl Enemy for ParchedEntity {}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;
    use crate::entity::next_entity_id;

    fn parched() -> ParchedEntity {
        init_vanilla_registry();
        ParchedEntity::new(
            &vanilla_entities::PARCHED,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    /// A parched shoots weakness and is immune to it. If the override were
    /// dropped, two parched shooting at each other would disarm one another.
    ///
    /// Slowness is the control: `Parched.canBeAffected` refuses weakness and
    /// then defers, so everything the undead tags allow still gets through.
    /// Poison would be a useless control -- a parched is undead, so
    /// `ignores_poison_and_regen` already refuses that one.
    #[test]
    fn a_parched_refuses_weakness_but_still_takes_slowness() {
        let mob = parched();

        assert!(!mob.can_be_affected(&MobEffectInstance::with_duration(
            vanilla_mob_effects::WEAKNESS,
            600,
            0
        )));
        assert!(mob.can_be_affected(&MobEffectInstance::with_duration(
            vanilla_mob_effects::SLOWNESS,
            600,
            0
        )));
    }

    /// The undead immunities `Parched` inherits rather than declares. If the
    /// override forgot to defer to `default_can_be_affected` a poison would get
    /// through.
    #[test]
    fn a_parched_still_inherits_the_undead_poison_immunity() {
        let mob = parched();
        assert!(!mob.can_be_affected(&MobEffectInstance::with_duration(
            vanilla_mob_effects::POISON,
            100,
            0
        )));
    }
}
