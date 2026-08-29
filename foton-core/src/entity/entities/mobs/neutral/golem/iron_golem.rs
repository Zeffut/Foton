//! Iron golem entity.
//!
//! Vanilla parity: `IronGolem`. It hits hard enough to launch what it hits,
//! cracks as it loses health, is welded back together with iron ingots, and
//! knows the difference between a golem a player built and one a village
//! summoned: the built one will never turn on a player.

use std::ptr;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::IronGolemEntityData;
use foton_registry::{sound_events, vanilla_attributes, vanilla_entities, vanilla_items};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, Downcast, DowncastType, DowncastTypeKey, UuidExt as _};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::enchantment_helper::{self, EnchantmentPostAttackContext};
use crate::entity::EntityEventSource;
use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, MoveTowardsTargetGoal,
    NearestAttackableTargetGoal, OfferFlowerGoal, RandomLookAroundGoal,
    ResetUniversalAngerTargetGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::neutral_mob::{
    PersistentAnger, TAG_ANGER_END_TIME, TAG_ANGRY_AT, read_persistent_anger, resolve_anger_target,
    write_persistent_anger,
};
use crate::entity::{
    Crackiness, CrackinessLevel, Entity, EntityBase, EntityBaseLoad, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, MoveResult, NeutralMob, PathfinderMob,
    SharedEntity,
};
use crate::player::Player;
use crate::world::World;

use super::AMBIENT_SOUND_INTERVAL;

/// Bit of the synced flag byte that marks a player-built golem.
///
/// Vanilla parity: the `1` mask of `IronGolem.isPlayerCreated`.
const PLAYER_CREATED_FLAG: i8 = 1;

/// How much an iron ingot mends.
///
/// Vanilla parity: `IronGolem.IRON_INGOT_HEAL_AMOUNT`.
const IRON_INGOT_HEAL_AMOUNT: f32 = 25.0;

/// How long the swing animation runs for.
///
/// Vanilla parity: the `10` of `IronGolem.doHurtTarget`.
const ATTACK_ANIMATION_TICKS: i32 = 10;

/// How long the golem holds a poppy out for.
///
/// Vanilla parity: the `400` of `IronGolem.offerFlower`.
const OFFER_FLOWER_TICKS: i32 = 400;

/// Shortest a grudge lasts.
///
/// Vanilla parity: the `TimeUtil.rangeOfSeconds(20, 39)` of
/// `IronGolem.PERSISTENT_ANGER_TIME`.
const ANGER_MIN_TICKS: i64 = 20 * 20;

/// Longest a grudge lasts.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// Speed the golem closes on its target at.
const MELEE_APPROACH_SPEED: f64 = 1.0;

/// Speed the golem drifts towards a distant target at.
///
/// Vanilla parity: the `0.9` of `MoveTowardsTargetGoal` in `registerGoals`.
const MOVE_TOWARDS_TARGET_SPEED: f64 = 0.9;

/// How far a target can be and still pull the golem towards it.
const MOVE_TOWARDS_TARGET_RANGE: f32 = 32.0;

/// Distance at which the golem watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 6.0;

/// How often the angry-player search runs.
const ANGRY_PLAYER_SEARCH_INTERVAL: i32 = 10;

/// How often the hostile-mob search runs.
const HOSTILE_SEARCH_INTERVAL: i32 = 5;

/// Extra upward shove a landed hit gives.
///
/// Vanilla parity: the `0.4F` of `IronGolem.doHurtTarget`.
const ATTACK_LAUNCH_VELOCITY: f64 = 0.4;

/// Mutable state vanilla keeps as plain fields on the golem.
#[derive(Debug, Default)]
struct GolemState {
    attack_animation_tick: i32,
    offer_flower_tick: i32,
}

/// An iron golem.
#[entity_behavior(class = "IronGolem")]
pub struct IronGolemEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<IronGolemEntityData>,
    state: SyncMutex<GolemState>,
    anger: PersistentAnger,
}

// SAFETY: This key is owned by Foton and uniquely identifies `IronGolemEntity`.
unsafe impl DowncastType for IronGolemEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/iron_golem");
}

impl IronGolemEntity {
    /// Creates an iron golem at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an iron golem from saved base data.
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
        let mut entity_data = IronGolemEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla `IronGolem.registerGoals` priorities, in order. The three
            // village goals -- `MoveBackToVillageGoal` at 2,
            // `GolemRandomStrollInVillageGoal` at 4 and
            // `DefendVillageTargetGoal` at target priority 1 -- need a village
            // distance tracker and villagers, neither of which Foton has.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, MeleeAttackGoal::new(MELEE_APPROACH_SPEED, true));
            goals.add_goal(
                2,
                MoveTowardsTargetGoal::new(MOVE_TOWARDS_TARGET_SPEED, MOVE_TOWARDS_TARGET_RANGE),
            );
            goals.add_goal(5, OfferFlowerGoal::new(set_offering_flower));
            goals.add_goal(7, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(2, HurtByTargetGoal::new());
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new_for_players_with_interval(
                    ANGRY_PLAYER_SEARCH_INTERVAL,
                    true,
                    false,
                    is_angry_at_target,
                ),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new_with_interval(
                    HOSTILE_SEARCH_INTERVAL,
                    false,
                    false,
                    |_, target, _| is_hostile_worth_attacking(target),
                ),
            );
            targets.add_goal(4, ResetUniversalAngerTargetGoal::new(false));
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(GolemState::default()),
            anger: PersistentAnger::new(),
        }
    }

    /// Returns whether a player built this golem.
    ///
    /// Vanilla parity: `IronGolem.isPlayerCreated`.
    #[must_use]
    pub fn is_player_created(&self) -> bool {
        *self.entity_data.lock().iron_golem().flags.get() & PLAYER_CREATED_FLAG != 0
    }

    /// Records whether a player built this golem.
    ///
    /// Vanilla parity: `IronGolem.setPlayerCreated`.
    pub fn set_player_created(&self, player_created: bool) {
        let mut data = self.entity_data.lock();
        let current = *data.iron_golem().flags.get();
        let updated = if player_created {
            current | PLAYER_CREATED_FLAG
        } else {
            current & !PLAYER_CREATED_FLAG
        };
        data.iron_golem_mut().flags.set(updated);
    }

    /// Returns how cracked the golem currently looks.
    ///
    /// Vanilla parity: `IronGolem.getCrackiness`.
    #[must_use]
    pub fn crackiness(&self) -> CrackinessLevel {
        Crackiness::GOLEM.by_fraction(self.get_health() / self.get_max_health())
    }

    /// Returns how many ticks are left of the swing animation.
    ///
    /// Vanilla parity: `IronGolem.getAttackAnimationTick`.
    #[must_use]
    pub fn attack_animation_tick(&self) -> i32 {
        self.state.lock().attack_animation_tick
    }

    /// Returns how many ticks are left of the flower offer.
    ///
    /// Vanilla parity: `IronGolem.getOfferFlowerTick`.
    #[must_use]
    pub fn offer_flower_tick(&self) -> i32 {
        self.state.lock().offer_flower_tick
    }

    /// Holds a poppy out, or puts it away.
    ///
    /// Vanilla parity: `IronGolem.offerFlower`.
    pub fn offer_flower(&self, offer: bool) {
        let (ticks, event) = if offer {
            (OFFER_FLOWER_TICKS, EntityStatus::OfferFlower)
        } else {
            (0, EntityStatus::StopOfferFlower)
        };
        self.state.lock().offer_flower_tick = ticks;
        self.broadcast_entity_event(event);
    }
}

/// Told by [`OfferFlowerGoal`] when the golem picks the poppy up and puts it down.
fn set_offering_flower(mob: &dyn PathfinderMob, offering: bool) {
    if let Some(golem) = mob.downcast_ref::<IronGolemEntity>() {
        golem.offer_flower(offering);
    }
}

/// Returns whether this golem is angry at `target`.
///
/// Vanilla parity: the `this::isAngryAt` method reference of
/// `IronGolem.registerGoals`.
fn is_angry_at_target(
    golem: Option<&dyn LivingEntity>,
    target: &dyn LivingEntity,
    world: &World,
) -> bool {
    let Some(golem) = golem.and_then(Downcast::downcast_ref::<IronGolemEntity>) else {
        return false;
    };
    // `is_angry_at` needs the shared world handle rather than the borrow the
    // selector is given.
    let Some(owned) = golem
        .level()
        .filter(|owned| ptr::eq(Arc::as_ptr(owned), ptr::from_ref(world)))
    else {
        return false;
    };
    golem.is_angry_at(target, &owned)
}

/// Returns whether a hostile is one the golem will pick a fight with.
///
/// Vanilla parity: the `target instanceof Enemy && !(target instanceof Creeper)`
/// of `IronGolem.registerGoals`. The golem leaves creepers alone so it does not
/// blow the village up trying to defend it.
fn is_hostile_worth_attacking(target: &dyn LivingEntity) -> bool {
    target.is_enemy() && target.entity_type() != &vanilla_entities::CREEPER
}

impl Entity for IronGolemEntity {
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

    /// Vanilla parity: `IronGolem.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_IRON_GOLEM_STEP, 1.0, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("PlayerCreated", i8::from(self.is_player_created()));
        write_persistent_anger(self, nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_player_created(nbt.byte("PlayerCreated").is_some_and(|value| value != 0));
        let angry_at = nbt
            .int_array(TAG_ANGRY_AT)
            .and_then(|array| Uuid::from_int_array(&array));
        read_persistent_anger(
            self,
            nbt.long(TAG_ANGER_END_TIME),
            nbt.int("AngerTime"),
            angry_at,
        );
        if let Some(world) = self.level() {
            let target = resolve_anger_target(&world, self.persistent_anger_target());
            let _ = Mob::set_target(self, target.as_ref());
        }
    }
}

impl LivingEntity for IronGolemEntity {
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
        Some(&sound_events::ENTITY_IRON_GOLEM_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_IRON_GOLEM_DEATH)
    }

    /// Vanilla parity: `IronGolem.hurtServer`, which is only there to creak
    /// when the golem crosses into a new crack stage.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let previous = self.crackiness();
        let was_hurt = self.living_hurt_server(world, source, amount);
        if was_hurt && self.crackiness() != previous {
            self.play_sound(&sound_events::ENTITY_IRON_GOLEM_DAMAGE, 1.0, 1.0);
        }
        was_hurt
    }

    /// Vanilla parity: `IronGolem.decreaseAirSupply`, which never spends any.
    fn decrease_air_supply(&self, current_supply: i32) -> i32 {
        current_supply
    }

    /// Vanilla parity: `IronGolem.doPush`.
    fn do_push(&self, entity: &SharedEntity) {
        if let Some(living) = entity.as_living_entity()
            && is_hostile_worth_attacking(living)
            && rand::random_range(0..20) == 0
        {
            let _ = Mob::set_target(self, Some(entity));
        }
        self.living_do_push(entity);
    }

    /// Vanilla parity: `IronGolem.aiStep`.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();

        {
            let mut state = self.state.lock();
            state.attack_animation_tick = (state.attack_animation_tick - 1).max(0);
            state.offer_flower_tick = (state.offer_flower_tick - 1).max(0);
        }

        if let Some(world) = self.level() {
            self.update_persistent_anger(&world, true);
        }

        result
    }
}

impl Mob for IronGolemEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Golems are silent apart from their step, hurt and death sounds.
    ///
    /// Vanilla parity: `AbstractGolem.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractGolem.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    /// Vanilla parity: `IronGolem.canAttack`. A player-built golem never turns
    /// on a player, and no golem ever goes for a creeper.
    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        if self.is_player_created() && target.entity_type() == &vanilla_entities::PLAYER {
            return false;
        }
        if target.entity_type() == &vanilla_entities::CREEPER {
            return false;
        }
        target.entity_type() != &vanilla_entities::GHAST && LivingEntity::can_attack(self, target)
    }

    /// Vanilla parity: `IronGolem.doHurtTarget`. The damage is rolled rather
    /// than fixed, and a landed hit throws the target up as well as back.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        self.state.lock().attack_animation_tick = ATTACK_ANIMATION_TICKS;
        self.broadcast_entity_event(EntityStatus::StartAttacking);

        let attack_damage = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;
        let whole_damage = attack_damage as i32;
        let damage = if whole_damage > 0 {
            attack_damage / 2.0 + rand::random_range(0..whole_damage) as f32
        } else {
            attack_damage
        };

        let weapon_item = {
            let mut main_hand = ItemStack::empty();
            self.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
                main_hand = item_stack.copy_with_count(item_stack.count());
            });
            main_hand
        };
        let damage_source = self.mob_attack_damage_source(&weapon_item, self);
        let hurt = target.hurt(world, &damage_source, damage);
        if hurt {
            let knockback_resistance = target
                .as_living_entity()
                .map_or(0.0, LivingEntity::knockback_resistance);
            let scale = (1.0 - knockback_resistance).max(0.0);
            target.set_velocity(
                target.velocity() + DVec3::new(0.0, ATTACK_LAUNCH_VELOCITY * scale, 0.0),
            );
            let context = EnchantmentPostAttackContext::new(
                target.as_ref(),
                Some(self.as_entity_event_source()),
                Some(self.as_entity_event_source()),
                &damage_source,
            );
            enchantment_helper::do_post_attack_effects_with_item_source(
                world,
                target.as_ref(),
                &weapon_item,
                &context,
            );
        }

        self.play_sound(&sound_events::ENTITY_IRON_GOLEM_ATTACK, 1.0, 1.0);
        hurt
    }

    /// Vanilla parity: `IronGolem.mobInteract`, mending the golem with an ingot.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let is_ingot = {
            let inventory = player.inventory.lock();
            inventory
                .get_item_in_hand(hand)
                .is(&vanilla_items::IRON_INGOT)
        };
        if !is_ingot {
            return InteractionResult::Pass;
        }

        let health_before = self.get_health();
        self.heal(IRON_INGOT_HEAL_AMOUNT);
        if (self.get_health() - health_before).abs() < f32::EPSILON {
            return InteractionResult::Pass;
        }

        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
        self.play_sound(&sound_events::ENTITY_IRON_GOLEM_REPAIR, 1.0, pitch);
        self.use_player_item(player, hand);

        InteractionResult::Success
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for IronGolemEntity {}

impl NeutralMob for IronGolemEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `IronGolem.PERSISTENT_ANGER_TIME`.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::init_vanilla_registry;
    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::*;

    fn iron_golem() -> IronGolemEntity {
        init_vanilla_registry();
        IronGolemEntity::new(
            &vanilla_entities::IRON_GOLEM,
            1,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    fn load_from(nbt: &NbtCompound) -> IronGolemEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let golem = iron_golem();
        golem.load_additional((&borrowed).into());
        golem
    }

    #[test]
    fn cracks_appear_at_the_vanilla_health_fractions() {
        let golem = iron_golem();
        // The crack fractions are read against the extracted max health, so the
        // table below is only meaningful while that stays 100.
        assert!((golem.get_max_health() - 100.0).abs() < f32::EPSILON);

        for (health, expected) in [
            (100.0, CrackinessLevel::None),
            (75.0, CrackinessLevel::None),
            (74.0, CrackinessLevel::Low),
            (50.0, CrackinessLevel::Low),
            (49.0, CrackinessLevel::Medium),
            (25.0, CrackinessLevel::Medium),
            (24.0, CrackinessLevel::High),
            (1.0, CrackinessLevel::High),
        ] {
            golem.set_health(health);
            assert_eq!(golem.crackiness(), expected, "at {health} health");
        }
    }

    #[test]
    fn a_player_built_golem_remembers_it_was_player_built() {
        let golem = iron_golem();
        assert!(!golem.is_player_created());

        golem.set_player_created(true);
        let mut nbt = NbtCompound::new();
        golem.save_additional(&mut nbt);
        assert_eq!(nbt.byte("PlayerCreated"), Some(1));
        assert!(load_from(&nbt).is_player_created());
    }

    #[test]
    fn a_grudge_survives_a_save_and_load() {
        let golem = iron_golem();
        let victim = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        golem.set_persistent_anger_end_time(4321);
        golem.set_persistent_anger_target(Some(victim));

        let mut nbt = NbtCompound::new();
        golem.save_additional(&mut nbt);
        assert_eq!(nbt.long(TAG_ANGER_END_TIME), Some(4321));

        let loaded = load_from(&nbt);
        assert_eq!(loaded.persistent_anger_end_time(), 4321);
        assert_eq!(loaded.persistent_anger_target(), Some(victim));
    }

    #[test]
    fn a_calm_golem_loads_calm_from_an_empty_compound() {
        init_vanilla_registry();
        let loaded = load_from(&NbtCompound::new());
        assert_eq!(loaded.persistent_anger_end_time(), -1);
        assert_eq!(loaded.persistent_anger_target(), None);
    }
}
