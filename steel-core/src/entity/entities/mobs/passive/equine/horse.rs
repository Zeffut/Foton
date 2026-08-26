//! The vanilla horse.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.equine.Horse`. Everything
//! shared with the donkey and the llama lives on [`AbstractHorse`]; what is left
//! here is the coat, the armor slot, and the mule a horse and a donkey make.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_types::SoundType;
use steel_registry::vanilla_entity_data::HorseEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_entities, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::passive::equine::variant::{
    HorseMarkings, HorseVariant, pack_type_variant, with_variant,
};
use crate::entity::entities::mobs::passive::equine::{
    add_abstract_horse_goals, add_default_horse_behaviour_goals, sync_mob_effect_entity_data,
};
use crate::entity::{
    AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, BABY_SCALE,
    ENTITIES, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData,
    HorseGroupData, LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase,
    MoveResult, PathfinderMob, SharedEntity, SpawnGroupData, generate_jump_strength,
    generate_max_health, generate_speed, next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;

/// Where a foal carries its rider.
///
/// Vanilla parity: `Horse.BABY_DIMENSIONS`, which reattaches the passenger at
/// `HORSE.getHeight() - 0.125F` and then scales the whole thing by `0.7F`.
const HORSE_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] = [EntityAttachmentPoint::new(
    0.0,
    (1.6 - 0.125) * BABY_SCALE as f64,
    0.0,
)];

/// A foal's hitbox.
const HORSE_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    1.396_484_4 * BABY_SCALE,
    1.6 * BABY_SCALE,
    1.52 * BABY_SCALE,
    EntityAttachments::new(&HORSE_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Odds of a galloping horse also snorting.
///
/// Vanilla parity: the `random.nextInt(10) == 0` of `Horse.playGallopSound`.
const BREATHE_CHANCE: i32 = 10;

/// How the coat of a bred foal is chosen.
///
/// Vanilla parity: the `random.nextInt(9)` of `Horse.getBreedOffspring`: four
/// ninths from each parent and one ninth wholly new.
const COAT_INHERITANCE_SIDES: i32 = 9;
const COAT_FROM_SELF_BELOW: i32 = 4;
const COAT_FROM_PARTNER_BELOW: i32 = 8;

/// How the markings of a bred foal are chosen.
///
/// Vanilla parity: the `random.nextInt(5)` of the same method.
const MARKINGS_INHERITANCE_SIDES: i32 = 5;
const MARKINGS_FROM_SELF_BELOW: i32 = 2;
const MARKINGS_FROM_PARTNER_BELOW: i32 = 4;

/// A vanilla horse.
#[entity_behavior(class = "Horse")]
pub struct HorseEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    horse_base: AbstractHorseBase,
    entity_data: SyncMutex<HorseEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `HorseEntity`.
unsafe impl DowncastType for HorseEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/horse");
}

impl HorseEntity {
    /// Creates a new horse.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a horse from saved base data.
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
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = HorseEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goals = mob_base.goal_selector().lock();
            add_abstract_horse_goals(&mut goals, true);
            add_default_horse_behaviour_goals(&mut goals, &mob_base);
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            horse_base: AbstractHorseBase::new(0),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    fn type_variant(&self) -> i32 {
        *self.entity_data.lock().id_type_variant.get()
    }

    fn set_type_variant(&self, type_variant: i32) {
        self.entity_data.lock().id_type_variant.set(type_variant);
    }

    /// Returns vanilla `Horse.getVariant`.
    #[must_use]
    pub fn variant(&self) -> HorseVariant {
        HorseVariant::by_id(self.type_variant() & 0xFF)
    }

    /// Applies vanilla `Horse.setVariant`.
    pub fn set_variant(&self, variant: HorseVariant) {
        self.set_type_variant(with_variant(self.type_variant(), variant));
    }

    /// Returns vanilla `Horse.getMarkings`.
    #[must_use]
    pub fn markings(&self) -> HorseMarkings {
        HorseMarkings::by_id((self.type_variant() & 0xFF00) >> 8)
    }

    /// Applies vanilla `Horse.setVariantAndMarkings`.
    pub fn set_variant_and_markings(&self, variant: HorseVariant, markings: HorseMarkings) {
        self.set_type_variant(pack_type_variant(variant, markings));
    }
}

impl Entity for HorseEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn tick(&self) {
        LivingEntity::tick_living_entity(self);
        self.tick_abstract_horse();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            HORSE_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if Mob::is_saddled(self)
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some()
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    fn passenger_attachment_point(&self, passenger: &dyn Entity) -> DVec3 {
        let scale = LivingEntity::get_scale(self);
        self.default_passenger_attachment_point(passenger)
            + self.abstract_horse_rearing_rider_offset(scale)
    }

    fn on_climbable(&self) -> bool {
        false
    }

    fn is_pushable(&self) -> bool {
        !self.is_vehicle()
    }

    fn can_jump_while_ridden(&self) -> bool {
        Mob::is_saddled(self)
    }

    fn handle_start_jump(&self, _jump_scale: i32) {
        self.handle_start_jump_abstract_horse();
    }

    fn open_custom_inventory_screen(&self, player: &Player) {
        if (self.is_vehicle() && !self.has_passenger(player)) || !self.is_tamed() {
            return;
        }
        self.open_horse_inventory_screen(player);
    }

    fn cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        self.abstract_horse_cause_fall_damage(fall_distance, damage_modifier, source)
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        sync_mob_effect_entity_data(self, &self.entity_data);
    }

    fn play_step_sound(&self, pos: BlockPos, block_state: BlockStateId) {
        self.abstract_horse_play_step_sound(pos, block_state);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_abstract_horse(nbt);
        nbt.insert("Variant", self.type_variant());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
        self.set_type_variant(nbt.int("Variant").unwrap_or(0));
    }
}

impl LivingEntity for HorseEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
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

    fn sound_volume(&self) -> f32 {
        0.8
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_HORSE_HURT
        } else {
            &sound_events::ENTITY_HORSE_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_HORSE_DEATH
        } else {
            &sound_events::ENTITY_HORSE_DEATH
        })
    }

    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let was_hurt = self.living_hurt_server(world, source, amount);
        self.abstract_horse_react_to_hurt(was_hurt)
    }

    fn hurt_armor(&self, source: &DamageSource, damage: f32) {
        self.do_hurt_equipment(source, damage, &[EquipmentSlot::Body]);
    }

    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        self.abstract_horse_can_dispenser_equip_into_slot(slot)
    }

    fn equip_sound(&self, slot: EquipmentSlot, stack: &ItemStack) -> Option<SoundEventRef> {
        if slot == EquipmentSlot::Saddle {
            return Some(&sound_events::ENTITY_HORSE_SADDLE);
        }
        LivingEntity::default_equip_sound(self, slot, stack)
    }

    fn is_immobile(&self) -> bool {
        self.abstract_horse_is_immobile()
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.tick_ridden_abstract_horse(controller, ridden_input);
    }

    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        self.abstract_horse_ridden_input(controller)
    }

    fn ridden_speed(&self, _controller: &Player) -> f32 {
        self.abstract_horse_ridden_speed()
    }

    fn drop_custom_death_loot(&self, _source: &DamageSource, _killed_by_player: bool) {
        self.drop_abstract_horse_inventory();
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.ai_step_abstract_horse();
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.server_ai_step_abstract_horse();
        result
    }
}

impl AgeableMob for HorseEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl Animal for HorseEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        self.is_horse_food(item_stack)
    }

    /// Vanilla parity: `Horse.canMate`, which lets a horse pair with a donkey.
    fn can_mate(&self, partner: &dyn Animal) -> bool {
        if partner.uuid() == self.uuid() {
            return false;
        }
        let partner_type = partner.entity_type();
        if partner_type != &vanilla_entities::DONKEY && partner_type != &vanilla_entities::HORSE {
            return false;
        }
        let Some(partner_horse) = partner.as_abstract_horse() else {
            return false;
        };
        self.can_parent() && partner_horse.can_parent()
    }

    /// Vanilla parity: `Horse.getBreedOffspring`, the one place mules come from.
    fn get_breed_offspring(
        &self,
        world: &Arc<World>,
        partner: &dyn Animal,
    ) -> Option<SharedEntity> {
        let partner_is_donkey = partner.entity_type() == &vanilla_entities::DONKEY;
        let baby_type = if partner_is_donkey {
            &vanilla_entities::MULE
        } else {
            &vanilla_entities::HORSE
        };
        let offspring = ENTITIES.create(
            baby_type,
            next_entity_id(),
            self.position(),
            Arc::downgrade(world),
        )?;
        let baby = offspring.as_abstract_horse()?;

        if !partner_is_donkey
            && let Some(partner_horse) = partner.downcast_ref::<HorseEntity>()
            && let Some(baby_horse) = offspring.downcast_ref::<HorseEntity>()
        {
            let coat_roll = rand::random_range(0..COAT_INHERITANCE_SIDES);
            let variant = if coat_roll < COAT_FROM_SELF_BELOW {
                self.variant()
            } else if coat_roll < COAT_FROM_PARTNER_BELOW {
                partner_horse.variant()
            } else {
                HorseVariant::random()
            };

            let markings_roll = rand::random_range(0..MARKINGS_INHERITANCE_SIDES);
            let markings = if markings_roll < MARKINGS_FROM_SELF_BELOW {
                self.markings()
            } else if markings_roll < MARKINGS_FROM_PARTNER_BELOW {
                partner_horse.markings()
            } else {
                HorseMarkings::random()
            };

            baby_horse.set_variant_and_markings(variant, markings);
        }

        self.set_offspring_attributes(partner, baby);
        Some(offspring)
    }
}

impl AbstractHorse for HorseEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.horse_base
    }

    fn horse_flags(&self) -> i8 {
        *self.entity_data.lock().abstract_horse().id_flags.get()
    }

    fn set_horse_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .abstract_horse_mut()
            .id_flags
            .set(flags);
    }

    fn eating_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_HORSE_EAT
        } else {
            &sound_events::ENTITY_HORSE_EAT
        })
    }

    fn angry_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_HORSE_ANGRY
        } else {
            &sound_events::ENTITY_HORSE_ANGRY
        })
    }

    /// Vanilla parity: `Horse.playGallopSound`, which adds the occasional snort.
    fn play_gallop_sound(&self, sound_type: SoundType) {
        self.play_sound(
            &sound_events::ENTITY_HORSE_GALLOP,
            sound_type.volume * 0.15,
            sound_type.pitch,
        );
        if rand::random_range(0..BREATHE_CHANCE) == 0 {
            let breathe = if AgeableMob::is_baby(self) {
                &sound_events::ENTITY_BABY_HORSE_BREATHE
            } else {
                &sound_events::ENTITY_HORSE_BREATHE
            };
            self.play_sound(breathe, sound_type.volume * 0.6, sound_type.pitch);
        }
    }

    /// Vanilla parity: `Horse.randomizeAttributes`, which rolls all three.
    fn randomize_attributes(&self) {
        let mut attributes = self.attributes().lock();
        attributes.set_base_value(
            vanilla_attributes::MAX_HEALTH,
            f64::from(generate_max_health(&mut |bound| {
                rand::random_range(0..bound)
            })),
        );
        attributes.set_base_value(
            vanilla_attributes::MOVEMENT_SPEED,
            generate_speed(&mut || rand::random::<f64>()),
        );
        attributes.set_base_value(
            vanilla_attributes::JUMP_STRENGTH,
            generate_jump_strength(&mut || rand::random::<f64>()),
        );
    }
}

impl Mob for HorseEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    /// Vanilla parity: `AbstractHorse.supportQuadLeash`.
    fn support_quad_leash(&self) -> bool {
        true
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos)
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    fn ambient_sound_interval(&self) -> i32 {
        400
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_BABY_HORSE_AMBIENT
        } else {
            &sound_events::ENTITY_HORSE_AMBIENT
        })
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let (variant, group_data) = if let Some(SpawnGroupData::Horse(horse_group)) = group_data {
            (horse_group.variant(), SpawnGroupData::Horse(horse_group))
        } else {
            let variant = HorseVariant::random();
            (variant, SpawnGroupData::Horse(HorseGroupData::new(variant)))
        };

        self.set_variant_and_markings(variant, HorseMarkings::random());
        self.finalize_spawn_abstract_horse(world, spawn_reason, Some(group_data))
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if self.skips_feeding_interact(player, Some(&vanilla_items::GOLDEN_DANDELION)) {
            return self.abstract_horse_mob_interact(player, hand);
        }
        if let Some(result) = self.try_feed_or_anger(player, hand) {
            return result;
        }
        self.abstract_horse_mob_interact(player, hand)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for HorseEntity {}
