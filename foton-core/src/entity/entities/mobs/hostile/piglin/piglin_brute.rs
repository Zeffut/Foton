//! Piglin brute entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.piglin.PiglinBrute`. The
//! bastion's guard: it does not barter, does not hunt, does not care what armor
//! you wear, and does not run. Gold armor buys nothing here.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::PiglinBruteEntityData;
use foton_registry::{sound_events, vanilla_entities, vanilla_items, vanilla_mob_effects};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, GlobalPos};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::Enemy;
use crate::entity::ai::brain::Brain;
use crate::entity::conversion::{ConversionParams, convert_to};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ZombifiedPiglinEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SpawnGroupData,
};
use crate::world::World;

use super::abstract_piglin::{self, ConvertiblePiglin, PiglinArmPose};
use super::piglin_brute_ai;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::conversion::ConversionReason::PiglinZombification;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 20` of the `PiglinBrute` constructor --
/// four times a plain monster's, and the only reason to fight one.
const XP_REWARD: i32 = 20;

/// How long the nausea lasts after zombification.
///
/// Vanilla parity: the `new MobEffectInstance(MobEffects.NAUSEA, 200, 0)` of
/// `AbstractPiglin.finishConversion`.
const CONVERSION_NAUSEA_TICKS: i32 = 200;

/// Fields a brute keeps that are neither synced nor on a base.
struct PiglinBruteState {
    /// Vanilla parity: `AbstractPiglin.timeInOverworld`.
    time_in_overworld: i32,
}

/// A piglin brute.
#[entity_behavior(class = "PiglinBrute")]
pub struct PiglinBruteEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<PiglinBruteEntityData>,
    state: SyncMutex<PiglinBruteState>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `PiglinBruteEntity`.
unsafe impl DowncastType for PiglinBruteEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/piglin_brute");
}

impl PiglinBruteEntity {
    /// Creates a piglin brute at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a piglin brute from saved base data.
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
        let mut entity_data = PiglinBruteEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let brute = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(PiglinBruteState {
                time_in_overworld: 0,
            }),
            brain: piglin_brute_ai::make_brain(),
        };
        abstract_piglin::apply_constructor(&brute);
        brute
    }

    /// The brain, without going through [`Mob::brain`].
    #[must_use]
    pub const fn brain_ref(&self) -> &Brain {
        &self.brain
    }

    /// Vanilla parity: `AbstractPiglin.setImmuneToZombification`.
    pub fn set_immune_to_zombification(&self, immune: bool) {
        self.entity_data
            .lock()
            .abstract_piglin_mut()
            .immune_to_zombification
            .set(immune);
    }

    /// Vanilla parity: `AbstractPiglin.isImmuneToZombification`.
    #[must_use]
    pub fn is_immune_to_zombification(&self) -> bool {
        *self
            .entity_data
            .lock()
            .abstract_piglin()
            .immune_to_zombification
            .get()
    }

    /// Sets how long this brute has stood in the overworld.
    ///
    /// Vanilla parity: the `@VisibleForTesting AbstractPiglin.setTimeInOverworld`.
    pub fn set_time_in_overworld(&self, time_in_overworld: i32) {
        self.state.lock().time_in_overworld = time_in_overworld;
    }

    /// Returns how long this brute has stood in the overworld.
    #[must_use]
    pub fn time_in_overworld(&self) -> i32 {
        self.state.lock().time_in_overworld
    }

    /// Vanilla parity: `PiglinBrute.getArmPose`.
    #[must_use]
    pub fn arm_pose(&self) -> PiglinArmPose {
        if Mob::is_aggressive(self) && abstract_piglin::is_holding_melee_weapon(self) {
            PiglinArmPose::AttackingWithMeleeWeapon
        } else {
            PiglinArmPose::Default
        }
    }

    /// Vanilla parity: `PiglinBrute.playAngrySound`.
    pub fn play_angry_sound(&self) {
        self.make_sound(Some(&sound_events::ENTITY_PIGLIN_BRUTE_ANGRY));
    }
}

impl ConvertiblePiglin for PiglinBruteEntity {
    /// Vanilla parity: `AbstractPiglin.isConverting`.
    fn is_converting(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        !self.is_immune_to_zombification()
            && !self.is_no_ai()
            && world.dimension_type.piglins_zombify
    }

    fn bump_time_in_overworld(&self, converting: bool) -> i32 {
        let mut state = self.state.lock();
        if converting {
            state.time_in_overworld += 1;
        } else {
            state.time_in_overworld = 0;
        }
        state.time_in_overworld
    }

    /// Vanilla parity: `PiglinBrute.playConvertedSound`.
    fn play_converted_sound(&self) {
        self.make_sound(Some(
            &sound_events::ENTITY_PIGLIN_BRUTE_CONVERTED_TO_ZOMBIFIED,
        ));
    }

    /// Vanilla parity: `AbstractPiglin.finishConversion`, which a brute does
    /// not override -- so unlike a piglin it spills nothing on the way.
    fn convert_to_zombified(&self) {
        let equipment: Vec<(EquipmentSlot, ItemStack)> = EquipmentSlot::ALL
            .into_iter()
            .map(|slot| (slot, self.get_item_by_slot(slot)))
            .filter(|(_, item)| !item.is_empty())
            .collect();

        convert_to(
            self,
            ConversionParams::single(true, true).with_reason(PiglinZombification),
            |id, position, world| {
                ZombifiedPiglinEntity::new(&vanilla_entities::ZOMBIFIED_PIGLIN, id, position, world)
            },
            |zombified| {
                for (slot, item) in equipment {
                    zombified.set_item_slot(slot, item);
                }
                zombified
                    .living_base()
                    .add_mob_effect(MobEffectInstance::with_duration(
                        vanilla_mob_effects::NAUSEA,
                        CONVERSION_NAUSEA_TICKS,
                        0,
                    ));
            },
        );
    }
}

impl Entity for PiglinBruteEntity {
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

    /// Vanilla parity: `PiglinBrute.playStepSound`.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_PIGLIN_BRUTE_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("IsImmuneToZombification", self.is_immune_to_zombification());
        nbt.insert("TimeInOverworld", self.state.lock().time_in_overworld);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        Mob::set_can_pick_up_loot(self, nbt.byte("CanPickUpLoot").is_none_or(|flag| flag != 0));
        self.set_immune_to_zombification(nbt.byte("IsImmuneToZombification").unwrap_or(0) != 0);
        self.state.lock().time_in_overworld = nbt.int("TimeInOverworld").unwrap_or(0);
        self.brain.load(nbt);
    }
}

impl LivingEntity for PiglinBruteEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: the `Mob.serverAiStep` a brute inherits, which is the
    /// only path to [`Mob::custom_server_ai_step`] and so to the brain.
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

    /// Vanilla parity: `PiglinBrute.hurtServer`.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        if !was_hurt {
            return false;
        }
        let Some(world) = self.level() else {
            return true;
        };
        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            return true;
        };
        if let Some(living) = attacker.as_living_entity() {
            piglin_brute_ai::was_hurt_by(&world, &self.brain, self, &attacker, living);
        }
        true
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PIGLIN_BRUTE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PIGLIN_BRUTE_DEATH)
    }
}

impl Mob for PiglinBruteEntity {
    /// Vanilla parity: `PiglinBrute` derives from `AbstractPiglin`, a `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `PiglinBrute.canHunt`, which is always false.
    fn can_hunt(&self) -> bool {
        false
    }

    /// Vanilla parity: `PiglinBrute.wantsToPickUp`, which is only ever true for
    /// the golden axe it fights with.
    fn wants_to_pick_up(&self, world: &World, item_stack: &ItemStack) -> bool {
        item_stack.is(&vanilla_items::GOLDEN_AXE) && self.mob_wants_to_pick_up(world, item_stack)
    }

    /// Vanilla parity: `PiglinBrute.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        let activity_changed = piglin_brute_ai::update_activity(&self.brain);
        Mob::set_aggressive(
            self,
            self.brain
                .has_memory_value(memory_module_types::ATTACK_TARGET.id()),
        );
        // Vanilla parity: `playActivitySound` on a change, then the random
        // `maybePlayActivitySound`; both only speak while fighting.
        if activity_changed && self.brain.active_non_core_activity() == Some(Activity::Fight) {
            self.play_angry_sound();
        }
        if piglin_brute_ai::should_play_activity_sound(&self.brain) {
            self.play_angry_sound();
        }
        abstract_piglin::tick_conversion(self);
    }

    /// Vanilla parity: `PiglinBrute.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PIGLIN_BRUTE_AMBIENT)
    }

    /// Vanilla parity: `PiglinBrute.finalizeSpawn`, which pins the brute's home
    /// to where it spawned and hands it its axe.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        piglin_brute_ai::init_memories(
            &self.brain,
            GlobalPos::new(world.key.clone(), self.block_position()),
        );
        self.set_item_slot(
            EquipmentSlot::MainHand,
            ItemStack::new(&vanilla_items::GOLDEN_AXE),
        );
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for PiglinBruteEntity {}

impl Enemy for PiglinBruteEntity {}
