//! Snow golem entity.
//!
//! Vanilla parity: `SnowGolem`. It paves a trail of snow behind it, pelts
//! hostiles with snowballs that do nothing but push them around, melts where
//! the biome is hot or the rain reaches it, and gives up its pumpkin to shears.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::SnowGolemEntityData;
use steel_registry::{
    sound_events, vanilla_blocks, vanilla_damage_types, vanilla_entities, vanilla_game_events,
    vanilla_game_rules, vanilla_items, vanilla_loot_tables,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey};

use crate::behavior::{BLOCK_BEHAVIORS, InteractionResult};
use crate::entity::ai::goal::{
    LookAtPlayerGoal, NearestAttackableTargetGoal, RandomLookAroundGoal, RangedAttackGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::SnowballEntity;
use crate::entity::living_entity::shearing_loot_items_with_rng;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, MoveResult, PathfinderMob, Projectile as _, SharedEntity, next_entity_id,
};

use super::AMBIENT_SOUND_INTERVAL;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Bit of the synced pumpkin byte that says the head is still on.
///
/// Vanilla parity: `SnowGolem.PUMPKIN_FLAG`.
const PUMPKIN_FLAG: i8 = 16;

/// How fast the golem closes on whatever it is shooting at.
///
/// Vanilla parity: the `1.25` of `SnowGolem.registerGoals`.
const ATTACK_APPROACH_SPEED: f64 = 1.25;

/// Ticks between snowballs.
///
/// Vanilla parity: the `20` of `SnowGolem.registerGoals`.
const ATTACK_INTERVAL_TICKS: i32 = 20;

/// How far away the golem will still shoot from.
///
/// Vanilla parity: the `10.0F` of `SnowGolem.registerGoals`.
const ATTACK_RADIUS: f32 = 10.0;

/// Speed multiplier while wandering.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// How willing the golem is to walk into water while wandering.
///
/// Vanilla parity: the `1.0000001E-5F` probability of `SnowGolem.registerGoals`,
/// which is small enough that it effectively never happens -- and it must not,
/// because water melts the golem.
const STROLL_WATER_PROBABILITY: f32 = 1.000_000_1E-5;

/// Distance at which the golem watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 6.0;

/// How often the target search runs.
///
/// Vanilla parity: the `10` random interval of `SnowGolem.registerGoals`.
const TARGET_SEARCH_INTERVAL: i32 = 10;

/// Damage the golem takes each tick where it melts.
///
/// Vanilla parity: the `1.0F` of `SnowGolem.aiStep`.
const MELT_DAMAGE: f32 = 1.0;

/// Speed the snowball leaves at.
///
/// Vanilla parity: the `1.6F` of `SnowGolem.performRangedAttack`.
const SNOWBALL_POWER: f32 = 1.6;

/// Spread of the golem's aim.
///
/// Vanilla parity: the `12.0F` of `SnowGolem.performRangedAttack`.
const SNOWBALL_UNCERTAINTY: f32 = 12.0;

/// A snow golem.
#[entity_behavior(class = "SnowGolem")]
pub struct SnowGolemEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SnowGolemEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SnowGolemEntity`.
unsafe impl DowncastType for SnowGolemEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/snow_golem");
}

impl SnowGolemEntity {
    /// Creates a snow golem at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a snow golem from saved base data.
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
        let mut entity_data = SnowGolemEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla `SnowGolem.registerGoals` priorities, in order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(
                1,
                RangedAttackGoal::new(
                    ATTACK_APPROACH_SPEED,
                    ATTACK_INTERVAL_TICKS,
                    ATTACK_RADIUS,
                    throw_snowball,
                ),
            );
            goals.add_goal(
                2,
                WaterAvoidingRandomStrollGoal::with_probability(
                    STROLL_SPEED_MODIFIER,
                    STROLL_WATER_PROBABILITY,
                ),
            );
            goals.add_goal(3, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(4, RandomLookAroundGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(
                1,
                NearestAttackableTargetGoal::new_with_interval(
                    TARGET_SEARCH_INTERVAL,
                    true,
                    false,
                    |_, target, _| target.is_enemy(),
                ),
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

    /// Returns whether the golem still wears its carved pumpkin.
    ///
    /// Vanilla parity: `SnowGolem.hasPumpkin`.
    #[must_use]
    pub fn has_pumpkin(&self) -> bool {
        *self.entity_data.lock().snow_golem().pumpkin.get() & PUMPKIN_FLAG != 0
    }

    /// Puts the pumpkin on or takes it off.
    ///
    /// Vanilla parity: `SnowGolem.setPumpkin`.
    pub fn set_pumpkin(&self, pumpkin: bool) {
        let mut data = self.entity_data.lock();
        let current = *data.snow_golem().pumpkin.get();
        let updated = if pumpkin {
            current | PUMPKIN_FLAG
        } else {
            current & !PUMPKIN_FLAG
        };
        data.snow_golem_mut().pumpkin.set(updated);
    }

    /// Returns whether shears would take anything off this golem.
    ///
    /// Vanilla parity: `SnowGolem.readyForShearing`.
    #[must_use]
    pub fn ready_for_shearing(&self) -> bool {
        self.has_pumpkin()
    }

    /// Takes the pumpkin off and drops it.
    ///
    /// Vanilla parity: `SnowGolem.shear`.
    pub fn shear(&self, world: &Arc<World>, tool: &ItemStack) {
        world.play_sound_at(
            &sound_events::ENTITY_SNOW_GOLEM_SHEAR,
            SoundSource::Players,
            self.position(),
            1.0,
            1.0,
            None,
        );
        self.set_pumpkin(false);

        let mut rng = rand::rng();
        let eye_height = self.get_eye_height();
        for dropped in shearing_loot_items_with_rng(
            self,
            &vanilla_loot_tables::SHEARING_SNOW_GOLEM,
            tool,
            &mut rng,
        ) {
            let _ = self.spawn_at_location(dropped, eye_height);
        }
    }

    /// Melts the golem where the environment is hostile to snow.
    ///
    /// Vanilla parity: the `SNOW_GOLEM_MELTS` branch of `SnowGolem.aiStep`.
    fn melt_if_too_warm(&self, world: &Arc<World>) {
        if !world.snow_golem_melts_at(self.block_position()) {
            return;
        }
        self.hurt(
            world,
            &DamageSource::environment(&vanilla_damage_types::ON_FIRE),
            MELT_DAMAGE,
        );
    }

    /// Lays the trail of snow the golem is known for.
    ///
    /// Vanilla parity: the `MOB_GRIEFING` branch of `SnowGolem.aiStep`.
    fn lay_snow_trail(&self, world: &Arc<World>) {
        if !world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING) {
            return;
        }

        let snow = vanilla_blocks::SNOW.default_state();
        let position = self.position();
        for corner in 0..4 {
            let x = (position.x + f64::from(corner % 2 * 2 - 1) * 0.25).floor() as i32;
            let y = position.y.floor() as i32;
            let z = (position.z + f64::from(corner / 2 % 2 * 2 - 1) * 0.25).floor() as i32;
            let snow_pos = BlockPos::new(x, y, z);

            if !world.get_block_state(snow_pos).is_air() {
                continue;
            }
            if !BLOCK_BEHAVIORS.get_behavior(snow.get_block()).can_survive(
                snow,
                world.as_ref(),
                snow_pos,
            ) {
                continue;
            }

            world.set_block(snow_pos, snow, UpdateFlags::UPDATE_ALL);
            world.game_event(
                &vanilla_game_events::BLOCK_PLACE,
                snow_pos,
                &GameEventContext::new(Some(self as &dyn Entity), Some(snow)),
            );
        }
    }
}

/// Throws a snowball at `target`.
///
/// Vanilla parity: `SnowGolem.performRangedAttack`.
fn throw_snowball(mob: &dyn PathfinderMob, target: &SharedEntity, _power: f32) {
    let Some(golem) = mob.downcast_ref::<SnowGolemEntity>() else {
        return;
    };
    let Some(world) = golem.level() else {
        return;
    };

    let position = golem.position();
    let target_position = target.position();
    // Vanilla spawns the snowball at the thrower's eye level minus 0.1 and then
    // aims at a point 1.1 below the target's eyes, lifted by a fifth of the
    // horizontal distance so the throw arcs.
    let spawn = DVec3::new(position.x, golem.get_eye_y() - 0.1, position.z);
    let xd = target_position.x - position.x;
    let zd = target_position.z - position.z;
    let horizontal = xd.hypot(zd);
    let yd = horizontal.mul_add(0.2, target.get_eye_y() - 1.1) - spawn.y;

    let snowball = Arc::new(SnowballEntity::new(
        &vanilla_entities::SNOWBALL,
        next_entity_id(),
        spawn,
        Arc::downgrade(&world),
    ));
    snowball.set_owner_uuid(Some(golem.uuid()));
    snowball.shoot(DVec3::new(xd, yd, zd), SNOWBALL_POWER, SNOWBALL_UNCERTAINTY);

    let entity: SharedEntity = snowball;
    if let Err(error) = world.try_add_entity(entity) {
        log::debug!("snow golem failed to throw a snowball: {error}");
        return;
    }

    golem.play_sound(
        &sound_events::ENTITY_SNOW_GOLEM_SHOOT,
        1.0,
        0.4f32.mul_add(rand::random::<f32>(), 0.8).recip(),
    );
}

impl Entity for SnowGolemEntity {
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

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("Pumpkin", i8::from(self.has_pumpkin()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_pumpkin(nbt.byte("Pumpkin").is_none_or(|value| value != 0));
    }
}

impl LivingEntity for SnowGolemEntity {
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

    /// Vanilla parity: `SnowGolem.isSensitiveToWater`.
    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SNOW_GOLEM_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SNOW_GOLEM_DEATH)
    }

    /// Vanilla parity: `SnowGolem.aiStep`.
    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        if let Some(world) = self.level() {
            self.melt_if_too_warm(&world);
            self.lay_snow_trail(&world);
        }
        result
    }
}

impl Mob for SnowGolemEntity {
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
        Some(&sound_events::ENTITY_SNOW_GOLEM_AMBIENT)
    }

    /// Vanilla parity: `AbstractGolem.getAmbientSoundInterval`.
    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractGolem.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        false
    }

    /// Vanilla parity: `SnowGolem.mobInteract`.
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
        self.shear(&world, &item_stack);
        world.game_event_at(
            &vanilla_game_events::SHEAR,
            self.position(),
            &GameEventContext::new(Some(player as &dyn Entity), None),
        );
        player
            .inventory
            .lock()
            .hurt_item_in_hand(hand, 1, player.has_infinite_materials());

        InteractionResult::Success
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SnowGolemEntity {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::init_vanilla_registry;

    use super::*;

    fn snow_golem() -> SnowGolemEntity {
        init_vanilla_registry();
        SnowGolemEntity::new(
            &vanilla_entities::SNOW_GOLEM,
            1,
            DVec3::new(8.5, 64.0, 8.5),
            Weak::new(),
        )
    }

    fn load_from(nbt: &NbtCompound) -> SnowGolemEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let golem = snow_golem();
        golem.load_additional((&borrowed).into());
        golem
    }

    #[test]
    fn shearing_a_snow_golem_takes_the_pumpkin_off_for_good() {
        let golem = snow_golem();
        assert!(golem.has_pumpkin());
        assert!(golem.ready_for_shearing());

        golem.set_pumpkin(false);
        assert!(!golem.has_pumpkin());
        assert!(!golem.ready_for_shearing());

        let mut nbt = NbtCompound::new();
        golem.save_additional(&mut nbt);
        assert_eq!(nbt.byte("Pumpkin"), Some(0));
        assert!(!load_from(&nbt).has_pumpkin());
    }

    #[test]
    fn a_golem_saved_without_a_pumpkin_flag_still_wears_one() {
        // Vanilla `SnowGolem.readAdditionalSaveData` defaults `Pumpkin` to true.
        init_vanilla_registry();
        assert!(load_from(&NbtCompound::new()).has_pumpkin());
    }
}
