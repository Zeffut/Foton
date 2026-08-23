//! Holding a poppy out to a villager, and handing it over.

use glam::DVec3;
use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_type_tags::EntityTypeTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_items};

use steel_utils::WorldAabb;

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::{PathfinderMob, SharedEntity};

/// How long the golem holds the flower out for.
///
/// Vanilla parity: `OfferFlowerGoal.OFFER_TICKS`.
const OFFER_TICKS: i32 = 400;

/// How far the golem looks for someone to offer to.
///
/// Vanilla parity: the `range(6.0)` of `OfferFlowerGoal.OFFER_TARGET_CONTEXT`.
const OFFER_RANGE: f64 = 6.0;

/// One-in-this-many chance per tick of starting an offer.
///
/// Vanilla parity: the `nextInt(8000)` of `OfferFlowerGoal.canUse`.
const OFFER_CHANCE_DENOMINATOR: i32 = 8000;

/// How far the golem's own bounding box is grown when looking for a recipient.
///
/// Vanilla parity: the `inflate(6.0, 2.0, 6.0)` of
/// `OfferFlowerGoal.getGolemBoundingBox`.
const OFFER_BOX_HORIZONTAL_INFLATE: f64 = 6.0;
const OFFER_BOX_VERTICAL_INFLATE: f64 = 2.0;

/// How sharply the golem turns its head towards whoever it is offering to.
///
/// Vanilla parity: the `30.0F, 30.0F` of `OfferFlowerGoal.tick`.
const OFFER_LOOK_SPEED: f32 = 30.0;

/// Told when the golem starts and stops holding its flower out.
///
/// Vanilla parity: `IronGolem.offerFlower`, which lives on the golem because it
/// is what broadcasts the entity event the client animates from.
pub(crate) type SetOfferingFlower = fn(&dyn PathfinderMob, bool);

/// Makes a golem hold a poppy out, and hand it over if nothing interrupts.
///
/// Vanilla parity: `OfferFlowerGoal`.
pub(crate) struct OfferFlowerGoal {
    set_offering: SetOfferingFlower,
    targeting: TargetingConditions,
    target: Option<SharedEntity>,
    tick: i32,
}

impl OfferFlowerGoal {
    #[must_use]
    pub(crate) fn new(set_offering: SetOfferingFlower) -> Self {
        Self {
            set_offering,
            targeting: TargetingConditions::for_non_combat().range(OFFER_RANGE),
            target: None,
            tick: 0,
        }
    }

    /// Vanilla parity: `OfferFlowerGoal.getGolemBoundingBox`.
    fn offer_box(mob: &dyn PathfinderMob) -> WorldAabb {
        mob.bounding_box().inflate_xyz(
            OFFER_BOX_HORIZONTAL_INFLATE,
            OFFER_BOX_VERTICAL_INFLATE,
            OFFER_BOX_HORIZONTAL_INFLATE,
        )
    }

    /// Hands the poppy over once the offer has run its course.
    ///
    /// Vanilla parity: the `stop` body of `OfferFlowerGoal`. In 26.2 the gift
    /// goes into the recipient's saddle slot, which is the slot the copper
    /// golem wears its antenna in.
    fn give_flower(mob: &dyn PathfinderMob, recipient: &SharedEntity) {
        let Some(other) = recipient.as_mob() else {
            return;
        };
        if !REGISTRY.entity_types.is_in_tag(
            recipient.entity_type(),
            &EntityTypeTag::ACCEPTS_IRON_GOLEM_GIFT,
        ) {
            return;
        }
        let Some(living) = recipient.as_living_entity() else {
            return;
        };
        if living.has_item_in_slot(EquipmentSlot::Saddle) {
            return;
        }
        if !Self::offer_box(mob).intersects(recipient.bounding_box()) {
            return;
        }

        living.with_equipment_slot_mut(EquipmentSlot::Saddle, &mut |slot| {
            *slot = ItemStack::new(&vanilla_items::POPPY);
        });
        other.set_guaranteed_drop(EquipmentSlot::Saddle);
    }
}

impl Goal for OfferFlowerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        if !world.is_bright_outside() {
            return false;
        }
        if rand::random_range(0..OFFER_CHANCE_DENOMINATOR) != 0 {
            return false;
        }

        let origin = mob.position();
        self.target =
            world.nearest_entity_in_aabb_matching(&Self::offer_box(mob), origin, |candidate| {
                if !REGISTRY.entity_types.is_in_tag(
                    candidate.entity_type(),
                    &EntityTypeTag::CANDIDATE_FOR_IRON_GOLEM_GIFT,
                ) {
                    return false;
                }
                candidate.as_living_entity().is_some_and(|living| {
                    self.targeting.test(&world, mob.as_living_entity(), living)
                })
            });

        self.target.is_some()
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        self.tick > 0
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.tick = reduced_tick_delay(OFFER_TICKS);
        (self.set_offering)(mob, true);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        (self.set_offering)(mob, false);
        if self.tick == 0
            && let Some(recipient) = self.target.take()
        {
            Self::give_flower(mob, &recipient);
        }
        self.target = None;
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if let Some(recipient) = &self.target {
            let position = recipient.position();
            mob.mob_base().controls().lock().look_control.set_look_at(
                DVec3::new(position.x, recipient.get_eye_y(), position.z),
                OFFER_LOOK_SPEED,
                OFFER_LOOK_SPEED,
            );
        }
        self.tick -= 1;
    }
}
