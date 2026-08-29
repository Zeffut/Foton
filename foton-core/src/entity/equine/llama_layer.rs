//! Shared vanilla `Llama` state and hooks.
//!
//! Vanilla parity: `Llama`. Both the wild llama and the trader's llama sit on
//! this: the strength that decides how much the chest holds, the caravan that
//! strings leashed llamas into a line, and the spit they answer a wolf with.

use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_entities, vanilla_items,
};
use foton_utils::locks::SyncMutex;
use glam::DVec3;

use crate::entity::damage::DamageSource;
use crate::entity::entities::LlamaSpitEntity;
use crate::entity::equine::AbstractChestedHorse;
use crate::entity::{AgeableMob, Entity, Projectile, SharedEntity, next_entity_id};
use crate::player::Player;
use crate::world::World;

/// The strongest a llama can be, in inventory columns.
///
/// Vanilla parity: `Llama.MAX_STRENGTH`.
pub const MAX_STRENGTH: i32 = 5;

/// Odds of a spawning llama rolling from the wider strength range.
///
/// Vanilla parity: the `random.nextFloat() < 0.04F` of `Llama.setRandomStrength`.
const RARE_STRENGTH_CHANCE: f32 = 0.04;

/// Strength range a common llama rolls within.
///
/// Vanilla parity: the `3` of `Llama.setRandomStrength`.
const COMMON_STRENGTH_RANGE: i32 = 3;

/// Odds of a bred llama being stronger than either parent.
///
/// Vanilla parity: the `random.nextFloat() < 0.03F` of `Llama.getBreedOffspring`.
const STRENGTH_BONUS_CHANCE: f32 = 0.03;

/// How hard a llama spits.
///
/// Vanilla parity: the `1.5F` velocity of `Llama.spit`.
const SPIT_VELOCITY: f32 = 1.5;

/// How wide a llama's aim wanders.
///
/// Vanilla parity: the `10.0F` inaccuracy of `Llama.spit`.
const SPIT_INACCURACY: f32 = 10.0;

/// The four coats a llama comes in.
///
/// Vanilla parity: `Llama.Variant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LlamaVariant {
    /// Vanilla `Llama.Variant.CREAMY`.
    #[default]
    Creamy,
    /// Vanilla `Llama.Variant.WHITE`.
    White,
    /// Vanilla `Llama.Variant.BROWN`.
    Brown,
    /// Vanilla `Llama.Variant.GRAY`.
    Gray,
}

impl LlamaVariant {
    /// Every variant in vanilla id order.
    pub const ALL: [Self; 4] = [Self::Creamy, Self::White, Self::Brown, Self::Gray];

    /// Returns the vanilla synchronized id.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::Creamy => 0,
            Self::White => 1,
            Self::Brown => 2,
            Self::Gray => 3,
        }
    }

    /// Returns the variant for a synchronized id.
    ///
    /// Vanilla parity: `Llama.Variant.BY_ID`, which clamps rather than wraps.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        match id {
            ..=0 => Self::Creamy,
            1 => Self::White,
            2 => Self::Brown,
            _ => Self::Gray,
        }
    }

    /// Picks a variant at random, as vanilla's `Util.getRandom` does.
    #[must_use]
    pub fn random() -> Self {
        Self::ALL[rand::random_range(0..Self::ALL.len())]
    }
}

/// Caravan links and the spit flag vanilla keeps on `Llama` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct LlamaState {
    /// Vanilla parity: `Llama.caravanHead`, held as an entity id rather than a
    /// strong reference so a caravan cannot keep its own members alive.
    caravan_head: Option<i32>,
    /// Vanilla parity: `Llama.caravanTail`, held the same way.
    caravan_tail: Option<i32>,
    did_spit: bool,
}

/// Runtime fields shared by vanilla llamas.
#[derive(Debug, Default)]
pub struct LlamaBase {
    state: SyncMutex<LlamaState>,
}

impl LlamaBase {
    /// Creates llama runtime state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SyncMutex::new(LlamaState {
                caravan_head: None,
                caravan_tail: None,
                did_spit: false,
            }),
        }
    }
}

/// Vanilla-shaped behavior shared by entities that extend `Llama`.
pub trait Llama: AbstractChestedHorse {
    /// Returns shared llama runtime state.
    fn llama_base(&self) -> &LlamaBase;

    /// Creates the kind of llama this one breeds.
    ///
    /// Vanilla parity: `Llama.makeNewLlama`, which the trader llama overrides so
    /// its offspring stays a trader llama and never despawns.
    fn make_new_llama(&self, world: &Arc<World>) -> Option<SharedEntity>;

    /// Returns the raw synchronized `Llama.DATA_STRENGTH_ID`.
    fn synced_strength(&self) -> i32;

    /// Writes the raw synchronized `Llama.DATA_STRENGTH_ID`.
    fn set_synced_strength(&self, strength: i32);

    /// Returns the raw synchronized `Llama.DATA_VARIANT_ID`.
    fn synced_variant_id(&self) -> i32;

    /// Writes the raw synchronized `Llama.DATA_VARIANT_ID`.
    fn set_synced_variant_id(&self, variant_id: i32);

    /// Returns vanilla `Llama.getStrength`.
    fn strength(&self) -> i32 {
        self.synced_strength()
    }

    /// Applies vanilla `Llama.setStrength`, which clamps into `1..=5`.
    fn set_strength(&self, strength: i32) {
        self.set_synced_strength(strength.clamp(1, MAX_STRENGTH));
    }

    /// Returns vanilla `Llama.getVariant`.
    fn llama_variant(&self) -> LlamaVariant {
        LlamaVariant::by_id(self.synced_variant_id())
    }

    /// Applies vanilla `Llama.setVariant`.
    fn set_llama_variant(&self, variant: LlamaVariant) {
        self.set_synced_variant_id(variant.id());
    }

    /// Returns vanilla `Llama.isTraderLlama`.
    fn is_trader_llama(&self) -> bool {
        false
    }

    /// Applies vanilla `Llama.setRandomStrength`.
    fn set_random_strength(&self) {
        let max_strength = if rand::random::<f32>() < RARE_STRENGTH_CHANCE {
            MAX_STRENGTH
        } else {
            COMMON_STRENGTH_RANGE
        };
        self.set_strength(1 + rand::random_range(0..max_strength));
    }

    /// Returns vanilla `Llama.getInventoryColumns`.
    fn llama_inventory_columns(&self) -> usize {
        if self.has_chest() {
            self.strength().max(0) as usize
        } else {
            0
        }
    }

    /// Returns vanilla `Llama.isFood`.
    fn is_llama_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::LLAMA_FOOD)
    }

    /// Applies vanilla `Llama.handleEating`.
    fn llama_handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        let mut item_used = false;
        let (heal, age_up, temper) = if item_stack.is(&vanilla_items::WHEAT) {
            (2.0, 10, 3)
        } else if item_stack.is(&vanilla_items::HAY_BLOCK) {
            if self.is_tamed() && self.get_age() == 0 && self.can_fall_in_love() {
                item_used = true;
                self.set_in_love(Some(player));
            }
            (10.0, 90, 6)
        } else {
            (0.0, 0, 0)
        };

        let item_used = self.apply_eating_effects(item_used, heal, age_up, temper);
        if item_used && let Some(eating_sound) = self.eating_sound() {
            let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
            self.play_sound(eating_sound, 1.0, pitch);
        }
        item_used
    }

    /// Returns vanilla `Llama.isImmobile`.
    fn llama_is_immobile(&self) -> bool {
        self.is_dead_or_dying() || self.is_eating()
    }

    /// Returns vanilla `Llama.didSpit`.
    fn did_spit(&self) -> bool {
        self.llama_base().state.lock().did_spit
    }

    /// Applies vanilla `Llama.setDidSpit`.
    fn set_did_spit(&self, did_spit: bool) {
        self.llama_base().state.lock().did_spit = did_spit;
    }

    /// Returns vanilla `Llama.inCaravan`.
    fn in_caravan(&self) -> bool {
        self.llama_base().state.lock().caravan_head.is_some()
    }

    /// Returns vanilla `Llama.hasCaravanTail`.
    fn has_caravan_tail(&self) -> bool {
        self.llama_base().state.lock().caravan_tail.is_some()
    }

    /// Returns vanilla `Llama.getCaravanHead`.
    fn caravan_head(&self) -> Option<SharedEntity> {
        let head_id = self.llama_base().state.lock().caravan_head?;
        self.level()?.get_entity_by_id(head_id)
    }

    /// Applies vanilla `Llama.joinCaravan`.
    fn join_caravan(&self, head: &dyn Llama) {
        self.llama_base().state.lock().caravan_head = Some(head.id());
        head.llama_base().state.lock().caravan_tail = Some(self.id());
    }

    /// Applies vanilla `Llama.leaveCaravan`.
    fn leave_caravan(&self) {
        let head = self.caravan_head();
        if let Some(head) = head
            && let Some(head_llama) = head.as_llama()
        {
            head_llama.llama_base().state.lock().caravan_tail = None;
        }
        self.llama_base().state.lock().caravan_head = None;
    }

    /// Applies vanilla `Llama.spit`.
    fn spit(&self, target: &SharedEntity) {
        let Some(world) = self.level() else {
            return;
        };

        // Vanilla's `LlamaSpit(Level, Llama)` constructor places the spit beside
        // the llama's muzzle, offset by half its width along the body rotation.
        let position = self.position();
        let body_rot = f64::from(self.y_body_rot()).to_radians();
        let side = f64::from(self.dimensions_for_pose(self.pose()).width + 1.0) * 0.5;
        let spawn = DVec3::new(
            side.mul_add(-body_rot.sin(), position.x),
            self.get_eye_y() - f64::from(0.1_f32),
            side.mul_add(body_rot.cos(), position.z),
        );

        let target_position = target.position();
        let target_height = f64::from(target.dimensions_for_pose(target.pose()).height);
        let xd = target_position.x - position.x;
        let yd = target_height.mul_add(1.0 / 3.0, target_position.y) - spawn.y;
        let zd = target_position.z - position.z;
        let arc = xd.hypot(zd) * f64::from(0.2_f32);

        let spit = Arc::new(LlamaSpitEntity::new(
            &vanilla_entities::LLAMA_SPIT,
            next_entity_id(),
            spawn,
            Arc::downgrade(&world),
        ));
        spit.set_owner_uuid(Some(self.uuid()));
        spit.shoot(DVec3::new(xd, yd + arc, zd), SPIT_VELOCITY, SPIT_INACCURACY);

        let entity: SharedEntity = spit;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("llama failed to spit: {error}");
            return;
        }

        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
        self.play_sound(&sound_events::ENTITY_LLAMA_SPIT, 1.0, pitch);
        self.set_did_spit(true);
    }

    /// Applies vanilla `Llama.causeFallDamage`.
    ///
    /// A llama shrugs off anything under six blocks, which is what lets a
    /// caravan drop down a cliff face intact.
    fn llama_cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        let damage = self.calculate_fall_damage(fall_distance, damage_modifier);
        if damage <= 0 {
            return false;
        }

        if fall_distance >= 6.0 {
            if let Some(world) = self.level() {
                self.hurt(&world, source, damage as f32);
            }
            self.propagate_fall_to_passengers(fall_distance, damage_modifier, source);
        }

        self.play_block_fall_sound();
        true
    }

    /// Rolls the strength and variant a bred llama inherits.
    ///
    /// Vanilla parity: the tail of `Llama.getBreedOffspring`.
    fn initialize_bred_llama(&self, partner: &dyn Llama, baby: &dyn Llama) {
        self.set_offspring_attributes(partner, baby);

        let mut strength =
            rand::random_range(0..self.strength().max(partner.strength()).max(1)) + 1;
        if rand::random::<f32>() < STRENGTH_BONUS_CHANCE {
            strength += 1;
        }
        baby.set_strength(strength);
        baby.set_llama_variant(if rand::random::<bool>() {
            self.llama_variant()
        } else {
            partner.llama_variant()
        });
    }

    /// Applies vanilla `Llama.finalizeSpawn`'s strength roll.
    fn finalize_spawn_llama(&self, _world: &Arc<World>) {
        self.set_random_strength();
    }
}

/// Returns whether `entity` is a llama of either kind.
///
/// Vanilla parity: the `is(EntityTypes.LLAMA) || is(EntityTypes.TRADER_LLAMA)`
/// filter that `LlamaFollowCaravanGoal` searches with.
#[must_use]
pub fn is_llama(entity: &dyn Entity) -> bool {
    entity.entity_type() == &vanilla_entities::LLAMA
        || entity.entity_type() == &vanilla_entities::TRADER_LLAMA
}

/// Returns whether a baby llama is old enough to be led by a caravan.
///
/// Vanilla parity: the `!this.inCaravan() && this.isBaby()` guard of
/// `Llama.followMommy`, kept here because both llama kinds share it.
#[must_use]
pub fn should_follow_mommy(llama: &dyn Llama) -> bool {
    !llama.in_caravan() && AgeableMob::is_baby(llama)
}
