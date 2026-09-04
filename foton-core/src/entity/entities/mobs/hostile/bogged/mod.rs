//! Bogged entity.
//!
//! Vanilla parity: `Bogged`. A swamp skeleton whose arrows carry poison and
//! whose mushrooms come off with shears -- once. It keeps the whole
//! `AbstractSkeleton` goal set unchanged and only slows its bow down.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::BoggedEntityData;
use foton_registry::{
    sound_events, vanilla_game_events, vanilla_items, vanilla_loot_tables, vanilla_mob_effects,
};
use foton_utils::BlockPos;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::LivingEntitySyncedData;
use crate::entity::SpawnGroupData;
use crate::entity::ai::goal::{
    FleeSunGoal, HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RangedBowAttackGoal, RestrictSunGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ArrowEntity;
use crate::entity::living_entity::shearing_loot_items_with_rng;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::weapon_holding_hand;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob,
};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Ticks between shots on Hard.
///
/// Vanilla parity: `Bogged.getHardAttackInterval`.
const HARD_ATTACK_INTERVAL_TICKS: i32 = 50;

/// Ticks between shots on every other difficulty.
///
/// Vanilla parity: `Bogged.getAttackInterval`.
const ATTACK_INTERVAL_TICKS: i32 = 70;

/// Range within which a bogged will loose an arrow.
///
/// Vanilla parity: the `attackRadius` of `AbstractSkeleton`'s bow goal.
const ATTACK_RADIUS: f64 = 15.0;

/// Speed of the arrows a bogged fires.
///
/// Vanilla parity: the `1.6F` velocity of `performRangedAttack`.
const ARROW_POWER: f32 = 1.6;

/// Spread of the arrows a bogged fires on normal difficulty.
///
/// Vanilla parity: `14 - difficulty * 4`, with difficulty 2.
const ARROW_UNCERTAINTY: f32 = 6.0;

/// Ticks of poison a bogged's arrow carries.
///
/// Vanilla parity: the `MobEffectInstance(MobEffects.POISON, 100)` of
/// `Bogged.getArrow`.
const POISON_TICKS: i32 = 100;

/// Speed multiplier while repositioning.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Speed the archer closes the distance at.
///
/// Vanilla parity: the `1.0` speed modifier `AbstractSkeleton` builds its bow
/// goal with.
const BOW_APPROACH_SPEED: f64 = 1.0;

/// Distance at which a bogged watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// NBT key for the shear flag.
///
/// Vanilla parity: `Bogged.SHEARED_TAG_NAME`.
const SHEARED_NBT_KEY: &str = "sheared";

/// A bogged.
#[entity_behavior(class = "Bogged")]
pub struct BoggedEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<BoggedEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `BoggedEntity`.
unsafe impl DowncastType for BoggedEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/bogged");
}

impl BoggedEntity {
    /// Creates a bogged at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a bogged from saved base data.
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
        let mut entity_data = BoggedEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // A bogged keeps the AbstractSkeleton goal set unchanged.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(2, RestrictSunGoal::new());
            goals.add_goal(3, FleeSunGoal::new(1.0));
            goals.add_goal(
                4,
                RangedBowAttackGoal::by_difficulty(
                    HARD_ATTACK_INTERVAL_TICKS,
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

    /// Returns whether the mushrooms have already been taken.
    ///
    /// Vanilla parity: `Bogged.isSheared`.
    #[must_use]
    pub fn is_sheared(&self) -> bool {
        *self.entity_data.lock().bogged().sheared.get()
    }

    /// Sets the shear flag.
    ///
    /// Vanilla parity: `Bogged.setSheared`.
    pub fn set_sheared(&self, sheared: bool) {
        self.entity_data.lock().bogged_mut().sheared.set(sheared);
    }

    /// Returns whether shears would take anything off this bogged.
    ///
    /// Vanilla parity: `Bogged.readyForShearing`. A bogged only grows its
    /// mushrooms once, so a sheared one is worth nothing to a farm.
    #[must_use]
    pub fn ready_for_shearing(&self) -> bool {
        !self.is_sheared()
    }

    /// Takes the mushrooms off and drops them.
    ///
    /// Vanilla parity: `Bogged.shear` and `Bogged.spawnShearedMushrooms`.
    pub fn shear(&self, world: &Arc<World>, sound_source: SoundSource, tool: &ItemStack) {
        world.play_sound_at(
            &sound_events::ENTITY_BOGGED_SHEAR,
            sound_source,
            self.position(),
            1.0,
            1.0,
            None,
        );

        let mut rng = rand::rng();
        // Vanilla drops from the mob's full height rather than its eye height.
        let drop_height = f64::from(self.base.dimensions().height);
        for dropped in shearing_loot_items_with_rng(
            self,
            &vanilla_loot_tables::SHEARING_BOGGED,
            tool,
            &mut rng,
        ) {
            let _ = self.spawn_at_location(dropped, drop_height);
        }

        self.set_sheared(true);
    }
}

/// Looses a poisoned arrow at `target`.
///
/// Vanilla parity: `AbstractSkeleton.performRangedAttack` with the
/// `Bogged.getArrow` override.
fn fire_arrow(mob: &dyn PathfinderMob, target: DVec3, power: f32) {
    let Some(archer) = mob.downcast_ref::<BoggedEntity>() else {
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
    // Vanilla parity: the `ProjectileUtil.getMobArrow(.., power, ..)`
    // of `performRangedAttack` -- a shallower draw hits softer.
    arrow.set_base_damage_from_mob(power);
    if bow.is(&vanilla_items::BOW) {
        arrow.set_fired_from_weapon(Some(bow));
    }
    // Vanilla parity: `Bogged.getArrow` poisons every arrow it fires.
    arrow.add_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::POISON,
        POISON_TICKS,
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

impl Entity for BoggedEntity {
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

    /// Vanilla parity: `Bogged.addAdditionalSaveData`, whose own contribution
    /// is the shear flag on top of the shared mob half.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert(SHEARED_NBT_KEY, self.is_sheared());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        // Vanilla parity: `getBooleanOr("sheared", false)`.
        self.set_sheared(nbt.byte(SHEARED_NBT_KEY).is_some_and(|value| value != 0));
    }
}

impl LivingEntity for BoggedEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BOGGED_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BOGGED_DEATH)
    }
}

impl Mob for BoggedEntity {
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
    /// Vanilla parity: `Bogged` derives from `AbstractSkeleton`, and so from
    /// `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` a bogged is
    /// registered with in `SpawnPlacements`.
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

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BOGGED_AMBIENT)
    }

    /// Shears the mushrooms off.
    ///
    /// Vanilla parity: `Bogged.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        if !item_stack.is(&vanilla_items::SHEARS) || !self.ready_for_shearing() {
            return InteractionResult::Pass;
        }

        let Some(world) = self.level() else {
            return InteractionResult::Pass;
        };
        // Vanilla passes `SoundSource.PLAYERS` here rather than the mob's own
        // source, because the shears are the player's.
        self.shear(&world, SoundSource::Players, &item_stack);
        world.game_event_at(
            &vanilla_game_events::SHEAR,
            self.position(),
            &GameEventContext::new(Some(player as &dyn Entity), None),
        );
        player.hurt_item_in_hand(hand, 1);

        InteractionResult::Success
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for BoggedEntity {}

impl Enemy for BoggedEntity {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::*;
    use crate::entity::next_entity_id;

    fn bogged() -> BoggedEntity {
        init_vanilla_registry();
        BoggedEntity::new(
            &vanilla_entities::BOGGED,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    fn reload(nbt: &NbtCompound) -> BoggedEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let mob = bogged();
        mob.load_additional((&borrowed).into());
        mob
    }

    /// The shear flag has to survive a save, or a farm could shear the same
    /// bogged again after every chunk reload.
    #[test]
    fn a_sheared_bogged_stays_sheared_across_a_save() {
        let mob = bogged();
        assert!(mob.ready_for_shearing());

        mob.set_sheared(true);
        assert!(!mob.ready_for_shearing());

        let mut nbt = NbtCompound::new();
        mob.save_additional(&mut nbt);
        assert_eq!(nbt.byte("sheared"), Some(1));
        assert!(reload(&nbt).is_sheared());
    }

    /// Vanilla's `getBooleanOr("sheared", false)` means an entity written
    /// before the flag existed comes back unsheared, not sheared.
    #[test]
    fn a_bogged_saved_without_the_flag_comes_back_unsheared() {
        assert!(!reload(&NbtCompound::new()).is_sheared());
    }
}
