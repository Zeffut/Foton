//! Cat entity.
//!
//! Vanilla parity: `Cat`. A tameable animal that is tamed with fish rather than
//! bones, and whose whole character is where it chooses to sit: on a chest, on
//! a lit furnace, at the foot of a bed, or on the owner who is asleep in it --
//! and, when that owner wakes, the present it leaves behind.

mod relax_on_owner;
mod tempt;

use std::str::FromStr;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::cat_sound_variant::{CatAge, CatSoundVariantRef};
use foton_registry::cat_variant::CatVariantRef;
use foton_registry::data_components::vanilla_components::DYE;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::CatEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{DyeColor, REGISTRY, RegistryExt, RegistryReference, TaggedRegistryExt};
use foton_utils::Identifier;
use foton_utils::locks::SyncMutex;
use foton_utils::random::legacy_random::LegacyRandom;
use foton_utils::types::InteractionHand;
use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::control::MoveControlOperation;
use crate::entity::ai::goal::{
    AvoidEntityGoal, BreedGoal, CatLieOnBedGoal, CatSitOnBlockGoal, FloatGoal, FollowOwnerGoal,
    Goal, GoalControls, LeapAtTargetGoal, LookAtPlayerGoal, OcelotAttackGoal, SitWhenOrderedToGoal,
    TamableAnimalPanicGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::SheepEntity;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, PathfinderMob, SpawnGroupData, TamableAnimal, TamableAnimalBase,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

use relax_on_owner::CatRelaxOnOwnerGoal;
use tempt::CatTemptGoal;

/// Speed a cat creeps at.
///
/// Vanilla parity: `Cat.TEMPT_SPEED_MOD`. `Cat.customServerAiStep` compares the
/// move control's speed against this exact value to pick the crouching pose, so
/// the three constants are load-bearing rather than decorative.
const TEMPT_SPEED_MOD: f64 = 0.6;

/// Speed a cat walks at.
///
/// Vanilla parity: `Cat.WALK_SPEED_MOD`.
const WALK_SPEED_MOD: f64 = 0.8;

/// Speed a cat sprints at.
///
/// Vanilla parity: `Cat.SPRINT_SPEED_MOD`.
const SPRINT_SPEED_MOD: f64 = 1.33;

/// The cat's baby hitbox.
///
/// Vanilla parity: `Cat.BABY_DIMENSIONS`.
const CAT_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.3125, 0.0)];
const CAT_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.3,
    0.35,
    0.34375,
    EntityAttachments::new(&CAT_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// Default collar color.
///
/// Vanilla parity: `Cat.DEFAULT_COLLAR_COLOR`.
const DEFAULT_COLLAR_COLOR: DyeColor = DyeColor::Red;

/// One chance in this many that a fish tames the cat.
///
/// Vanilla parity: the `random.nextInt(3) == 0` of `Cat.tryToTame`.
const TAME_CHANCE: i32 = 3;

/// One chance in this many that a tamed idle cat purreows rather than meows.
///
/// Vanilla parity: the `random.nextInt(4) == 0` of `Cat.getAmbientSound`.
const PURREOW_CHANCE: i32 = 4;

/// Ticks between two purrs while lying down.
///
/// Vanilla parity: the `tickCount % 5 == 0` of `Cat.handleLieDown`.
const PURR_INTERVAL_TICKS: i32 = 5;

/// Ticks between two begging meows while being tempted.
///
/// Vanilla parity: the `tickCount % 100 == 0` of `Cat.tick`.
const BEG_FOR_FOOD_INTERVAL_TICKS: i32 = 100;

/// How long an untamed cat lives before it despawns.
///
/// Vanilla parity: the `tickCount > 2400` of `Cat.removeWhenFarAway`.
const STRAY_DESPAWN_AGE_TICKS: i32 = 2400;

/// How far an untamed cat runs from a player.
///
/// Vanilla parity: the `16.0F` of `Cat.CatAvoidEntityGoal`.
const AVOID_PLAYER_DISTANCE: f32 = 16.0;

/// Vanilla cat entity.
#[entity_behavior(class = "Cat")]
pub struct CatEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    tamable_base: TamableAnimalBase,
    entity_data: SyncMutex<CatEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `CatEntity`.
unsafe impl DowncastType for CatEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/cat");
}

impl CatEntity {
    /// Creates a new cat entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a cat entity from saved base data.
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
        let mut entity_data = CatEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let cat = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            tamable_base: TamableAnimalBase::new(),
            entity_data: SyncMutex::new(entity_data),
        };
        cat.register_goals();
        cat
    }

    /// Vanilla parity: `Cat.registerGoals`.
    ///
    /// Vanilla's `reassessTameGoals` adds and removes the avoid-players goal as
    /// the cat is tamed. Foton registers it once with the same tame checks the
    /// vanilla subclass carries in `canUse`/`canContinueToUse`, which produces
    /// the same behavior: a goal that cannot start and cannot continue while
    /// the cat is tamed is indistinguishable from an absent one.
    fn register_goals(&self) {
        let mut goals = self.mob_base.goal_selector().lock();
        goals.add_goal(1, FloatGoal::new(&self.mob_base));
        goals.add_goal(1, TamableAnimalPanicGoal::new(1.5));
        goals.add_goal(2, SitWhenOrderedToGoal::new());
        goals.add_goal(3, CatRelaxOnOwnerGoal::new());
        goals.add_goal(4, CatTemptGoal::new(TEMPT_SPEED_MOD));
        goals.add_goal(4, CatAvoidPlayersGoal::new());
        goals.add_goal(5, CatLieOnBedGoal::new(1.1));
        goals.add_goal(6, FollowOwnerGoal::new(1.0, 10.0, 5.0));
        goals.add_goal(7, CatSitOnBlockGoal::new(WALK_SPEED_MOD));
        goals.add_goal(8, LeapAtTargetGoal::new(0.3));
        goals.add_goal(9, OcelotAttackGoal::new());
        goals.add_goal(10, BreedGoal::new(WALK_SPEED_MOD));
        goals.add_goal(11, WaterAvoidingRandomStrollGoal::new(WALK_SPEED_MOD));
        goals.add_goal(12, LookAtPlayerGoal::new(10.0));
        // Vanilla parity gap: the two `NonTameRandomTargetGoal` entries hunt
        // rabbits and land-bound baby turtles. Neither mob exists in Foton yet,
        // so neither goal is registered.
    }

    /// Returns the current cat variant.
    #[must_use]
    pub fn variant(&self) -> CatVariantRef {
        self.entity_data.lock().variant.get().value()
    }

    /// Sets the current cat variant by registry entry.
    pub fn set_variant(&self, variant: CatVariantRef) {
        self.entity_data
            .lock()
            .variant
            .set(RegistryReference::new(variant));
    }

    /// Returns the current cat sound variant.
    #[must_use]
    pub fn sound_variant(&self) -> CatSoundVariantRef {
        self.entity_data.lock().sound_variant.get().value()
    }

    /// Sets the current cat sound variant by registry entry.
    pub fn set_sound_variant(&self, sound_variant: CatSoundVariantRef) {
        self.entity_data
            .lock()
            .sound_variant
            .set(RegistryReference::new(sound_variant));
    }

    fn set_variant_by_key(&self, key: &Identifier) -> bool {
        let Some(variant) = REGISTRY.cat_variants.by_key(key) else {
            return false;
        };
        self.set_variant(variant);
        true
    }

    fn set_sound_variant_by_key(&self, key: &Identifier) {
        if let Some(sound_variant) = REGISTRY.cat_sound_variants.by_key(key) {
            self.set_sound_variant(sound_variant);
        }
    }

    /// Vanilla parity: `Cat.getSoundSet`.
    fn sound_set(&self) -> &'static CatAge {
        let sound_variant = self.sound_variant();
        if AgeableMob::is_baby(self) {
            &sound_variant.baby_sounds
        } else {
            &sound_variant.adult_sounds
        }
    }

    /// Returns vanilla `Cat.isLying`.
    #[must_use]
    pub fn is_lying(&self) -> bool {
        *self.entity_data.lock().is_lying.get()
    }

    /// Sets vanilla `Cat.setLying`.
    pub fn set_lying(&self, value: bool) {
        self.entity_data.lock().is_lying.set(value);
    }

    /// Returns vanilla `Cat.isRelaxStateOne`.
    #[must_use]
    pub fn is_relax_state_one(&self) -> bool {
        *self.entity_data.lock().relax_state_one.get()
    }

    /// Sets vanilla `Cat.setRelaxStateOne`.
    pub fn set_relax_state_one(&self, value: bool) {
        self.entity_data.lock().relax_state_one.set(value);
    }

    /// Returns the collar color.
    ///
    /// Vanilla parity: `Cat.getCollarColor`.
    #[must_use]
    pub fn collar_color(&self) -> DyeColor {
        DyeColor::by_id(*self.entity_data.lock().collar_color.get())
    }

    /// Sets the collar color, synchronized to the client.
    pub fn set_collar_color(&self, color: DyeColor) {
        self.entity_data.lock().collar_color.set(color.id());
    }

    /// Plays vanilla `Cat.hiss`.
    pub fn hiss(&self) {
        self.make_sound(Some(self.sound_set().hiss_sound));
    }

    /// Returns whether the stack is vanilla cat food.
    #[must_use]
    pub fn is_cat_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CAT_FOOD)
    }

    /// Vanilla parity: `Cat.tryToTame`.
    fn try_to_tame(&self, player: &Player) {
        if rand::random_range(0..TAME_CHANCE) != 0 {
            self.spawn_taming_particles(false);
            return;
        }

        self.tame(player);
        self.set_ordered_to_sit(true);
        self.spawn_taming_particles(true);
    }

    /// Vanilla parity: `Cat.handleLieDown`, minus the two client-side
    /// interpolations. The purr is the only part the server owns.
    fn handle_lie_down(&self) {
        if (self.is_lying() || self.is_relax_state_one())
            && self.tick_count() % PURR_INTERVAL_TICKS == 0
        {
            let volume = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.4, 0.6);
            self.play_sound(self.sound_set().purr_sound, volume, 1.0);
        }
        // VANILLA CLIENT-LOCAL: `updateLieDownAmount`, `updateRelaxStateOneAmount`
        // and `isLyingOnTopOfSleepingPlayer` only feed the renderer.
    }

    /// Vanilla parity: the beg-for-food meow of `Cat.tick`.
    fn tick_beg_for_food_sound(&self) {
        if self.is_tame()
            || self.tick_count() % BEG_FOR_FOOD_INTERVAL_TICKS != 0
            || !self.is_being_tempted()
        {
            return;
        }

        self.play_sound(self.sound_set().beg_for_food_sound, 1.0, 1.0);
    }

    /// Vanilla parity: the owner half of `Cat.mobInteract`.
    fn owner_interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        item_stack: &ItemStack,
    ) -> InteractionResult {
        let is_collar_dye = REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CAT_COLLAR_DYES);
        if is_collar_dye {
            if let Some(color) = item_stack.get(DYE).copied()
                && color != self.collar_color()
            {
                self.set_collar_color(color);
                Mob::use_player_item(self, player, hand);
                self.set_persistence_required();
                return InteractionResult::Success;
            }
        } else if Self::is_cat_food(item_stack) && self.get_health() < self.get_max_health() {
            self.feed(player, hand, item_stack, 1.0, 1.0);
            return InteractionResult::Success;
        }

        let parent_interaction = Animal::mob_interact_animal(self, player, hand);
        if parent_interaction.consumes_action() {
            return parent_interaction;
        }

        self.set_ordered_to_sit(!self.is_ordered_to_sit());
        InteractionResult::Success
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let display = self.living_base.mob_effect_display_state();

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }
}

/// The goal that keeps a stray cat away from players.
///
/// Vanilla parity: `Cat.CatAvoidEntityGoal`, whose only additions to
/// `AvoidEntityGoal` are the two tame checks.
struct CatAvoidPlayersGoal {
    avoid: AvoidEntityGoal,
}

impl CatAvoidPlayersGoal {
    fn new() -> Self {
        Self {
            avoid: AvoidEntityGoal::with_selector(
                AVOID_PLAYER_DISTANCE,
                WALK_SPEED_MOD,
                SPRINT_SPEED_MOD,
                |_, target, _| {
                    target.as_player().is_some_and(|player| {
                        !target.is_spectator() && !player.has_infinite_materials()
                    })
                },
            ),
        }
    }

    fn is_tame(mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<CatEntity>()
            .is_some_and(TamableAnimal::is_tame)
    }
}

impl Goal for CatAvoidPlayersGoal {
    fn controls(&self) -> GoalControls {
        self.avoid.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !Self::is_tame(mob) && self.avoid.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !Self::is_tame(mob) && self.avoid.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.avoid.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.avoid.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.avoid.tick(mob);
    }
}

impl Entity for CatEntity {
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
        self.tick_beg_for_food_sound();
        self.handle_lie_down();
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            CAT_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    fn is_allied_to(&self, other: &dyn Entity) -> bool {
        self.considers_entity_as_ally_tamable(other)
    }

    fn is_tame_owned_by(&self, owner: &dyn LivingEntity) -> bool {
        self.is_tame() && self.is_owned_by(owner.as_entity_event_source())
    }

    /// Vanilla parity: `Cat.isSteppingCarefully`, which is how a crouching cat
    /// stops itself walking off the block it is stalking from.
    fn is_stepping_carefully(&self) -> bool {
        Entity::is_crouching(self) || self.is_suppressing_bounce()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_tamable_animal(nbt);
        nbt.insert("variant", self.variant().key.to_string());
        nbt.insert("sound_variant", self.sound_variant().key.to_string());
        nbt.insert(
            "CollarColor",
            i8::try_from(self.collar_color().id()).unwrap_or(0),
        );
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_tamable_animal(nbt);

        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
        {
            self.set_variant_by_key(&key);
        }
        if let Some(sound_variant) = nbt.string("sound_variant")
            && let Ok(key) = Identifier::from_str(sound_variant.to_str().as_ref())
        {
            self.set_sound_variant_by_key(&key);
        }
        self.set_collar_color(
            nbt.byte("CollarColor")
                .map_or(DEFAULT_COLLAR_COLOR, |id| DyeColor::by_id(i32::from(id))),
        );
    }
}

impl LivingEntity for CatEntity {
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(self.sound_set().hurt_sound)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(self.sound_set().death_sound)
    }

    fn die(&self, source: &DamageSource) {
        self.notify_owner_of_death(source);
        self.living_die(source);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for CatEntity {
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

impl Animal for CatEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_cat_food(item_stack)
    }

    fn play_eating_sound(&self) {
        self.play_sound(self.sound_set().eat_sound, 1.0, 1.0);
    }

    /// Vanilla parity: `Cat.canMate`. Both cats must be tamed.
    fn can_mate(&self, partner: &dyn Animal) -> bool {
        if !self.is_tame() {
            return false;
        }
        let Some(other) = partner.as_entity_event_source().downcast_ref::<Self>() else {
            return false;
        };

        other.is_tame()
            && self.uuid() != partner.uuid()
            && self.entity_type() == partner.entity_type()
            && self.is_in_love()
            && other.is_in_love()
    }

    fn breed_variant_key(&self) -> Option<&Identifier> {
        Some(&self.variant().key)
    }

    fn set_breed_variant_key(&self, key: &Identifier) -> bool {
        self.set_variant_by_key(key)
    }

    /// Vanilla parity: `Cat.getBreedOffspring`.
    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let partner_cat = partner.as_entity_event_source().downcast_ref::<Self>();
        let Some(kitten) = offspring.as_entity_event_source().downcast_ref::<Self>() else {
            return;
        };

        let inherit_from_self = rand::random::<bool>();
        let variant = match (inherit_from_self, partner_cat) {
            (false, Some(partner_cat)) => partner_cat.variant(),
            _ => self.variant(),
        };
        kitten.set_variant(variant);

        if !self.is_tame() {
            return;
        }

        kitten.set_owner_uuid(self.owner_uuid());
        kitten.set_tame(true, true);
        if let Some(partner_cat) = partner_cat {
            kitten.set_collar_color(SheepEntity::get_mixed_color(
                self.collar_color(),
                partner_cat.collar_color(),
            ));
        }
    }
}

impl TamableAnimal for CatEntity {
    fn tamable_base(&self) -> &TamableAnimalBase {
        &self.tamable_base
    }

    fn tamable_flags(&self) -> i8 {
        *self.entity_data.lock().tamable_animal().flags.get()
    }

    fn set_tamable_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .flags
            .set(flags);
    }

    fn owner_uuid(&self) -> Option<Uuid> {
        *self.entity_data.lock().tamable_animal().owneruuid.get()
    }

    fn set_owner_uuid(&self, owner: Option<Uuid>) {
        self.entity_data
            .lock()
            .tamable_animal_mut()
            .owneruuid
            .set(owner);
    }
}

impl Mob for CatEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        self.can_attack_tamable(target)
    }

    fn ambient_sound_interval(&self) -> i32 {
        120
    }

    /// Vanilla parity: `Cat.getAmbientSound`.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        let sound_set = self.sound_set();
        if !self.is_tame() {
            return Some(sound_set.stray_ambient_sound);
        }
        if self.is_in_love() {
            return Some(sound_set.purr_sound);
        }

        if rand::random_range(0..PURREOW_CHANCE) == 0 {
            Some(sound_set.purreow_sound)
        } else {
            Some(sound_set.ambient_sound)
        }
    }

    /// Vanilla parity: `Cat.customServerAiStep`, which reads the speed the move
    /// control was given back out to choose between crouching, walking and
    /// sprinting.
    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);

        let wanted_speed = {
            let controls = self.mob_base.controls().lock();
            (controls.move_control.operation() == MoveControlOperation::MoveTo)
                .then(|| controls.move_control.speed_modifier())
        };

        let (pose, sprinting) = match wanted_speed {
            Some(speed) if (speed - TEMPT_SPEED_MOD).abs() < f64::EPSILON => {
                (EntityPose::Sneaking, false)
            }
            Some(speed) if (speed - SPRINT_SPEED_MOD).abs() < f64::EPSILON => {
                (EntityPose::Standing, true)
            }
            _ => (EntityPose::Standing, false),
        };

        self.set_pose(pose);
        self.set_sprinting(sprinting);
    }

    /// Vanilla parity: `Cat.removeWhenFarAway`. A stray cat is temporary; a
    /// tamed one is not.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.is_tame() && self.tick_count() > STRAY_DESPAWN_AGE_TICKS
    }

    /// Vanilla parity: `Cat.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = self.finalize_spawn_ageable_mob(world, spawn_reason, group_data);

        let mut random = LegacyRandom::from_seed(rand::random());
        if let Some(variant) = world.biome_at(self.block_position()).and_then(|biome| {
            REGISTRY
                .cat_variants
                .select_spawn_variant(biome, &mut random)
        }) {
            self.set_variant(variant);
        }
        if let Some(sound_variant) = REGISTRY.cat_sound_variants.pick_random(&mut random) {
            self.set_sound_variant(sound_variant);
        }

        group_data
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        if self.is_tame() {
            if self.is_owned_by(player) {
                return self.owner_interact(player, hand, &item_stack);
            }
        } else if Self::is_cat_food(&item_stack) {
            Mob::use_player_item(self, player, hand);
            self.try_to_tame(player);
            self.set_persistence_required();
            self.play_eating_sound();
            return InteractionResult::Success;
        }

        let interact = Animal::mob_interact_animal(self, player, hand);
        if interact.consumes_action() {
            self.set_persistence_required();
        }
        interact
    }
}

impl PathfinderMob for CatEntity {}

#[cfg(test)]
mod tests;
