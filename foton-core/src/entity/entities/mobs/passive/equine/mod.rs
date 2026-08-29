//! The horse family.
//!
//! Vanilla parity: the `net.minecraft.world.entity.animal.equine` package. All
//! eight mobs share the [`AbstractHorse`](crate::entity::AbstractHorse) layer, so
//! they are grouped the way vanilla groups them rather than split across Foton's
//! passive/hostile folders -- the zombie horse is a `MONSTER` by category and
//! still belongs here.

mod camel;
mod camel_ai;
mod camel_common;
mod camel_husk;
mod donkey;
mod horse;
mod llama;
mod mule;
mod skeleton_horse;
mod skeleton_trap_goal;
mod trader_llama;
mod variant;
mod zombie_horse;

pub use camel::CamelEntity;
pub use camel_husk::CamelHuskEntity;
pub use donkey::DonkeyEntity;
pub use horse::HorseEntity;
pub use llama::LlamaEntity;
pub use mule::MuleEntity;
pub use skeleton_horse::SkeletonHorseEntity;
#[cfg(test)]
pub(crate) use skeleton_trap_goal::SkeletonTrapGoal;
pub use trader_llama::TraderLlamaEntity;
pub use variant::{HorseMarkings, HorseVariant};
pub use zombie_horse::ZombieHorseEntity;

#[cfg(test)]
mod tests;

use foton_registry::vanilla_entity_data::{VanillaEntityData, VanillaLivingEntityData};
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _};
use foton_utils::locks::SyncMutex;

use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, GoalSelector, LookAtPlayerGoal, PanicGoal,
    RandomLookAroundGoal, RandomStandGoal, RunAroundLikeCrazyGoal, TemptGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::{EntitySyncedData, LivingEntity, MobBase};

/// Registers the goals every horse shares.
///
/// Vanilla parity: `AbstractHorse.registerGoals`, minus the `addBehaviourGoals`
/// call each subclass supplies itself.
pub(super) fn add_abstract_horse_goals(goals: &mut GoalSelector, can_perform_rearing: bool) {
    goals.add_goal(1, RunAroundLikeCrazyGoal::new(1.2));
    goals.add_goal(2, BreedGoal::new(1.0));
    goals.add_goal(4, FollowParentGoal::new(1.0));
    goals.add_goal(6, WaterAvoidingRandomStrollGoal::new(0.7));
    goals.add_goal(7, LookAtPlayerGoal::new(6.0));
    goals.add_goal(8, RandomLookAroundGoal::new());
    if can_perform_rearing {
        goals.add_goal(9, RandomStandGoal::new());
    }
}

/// Registers the goals a horse that has not replaced them keeps.
///
/// Vanilla parity: `AbstractHorse.addBehaviourGoals`. The panic goal is vanilla's
/// inner `MountPanicGoal`, which sits out while a mob is steering the horse.
pub(super) fn add_default_horse_behaviour_goals(goals: &mut GoalSelector, mob_base: &MobBase) {
    goals.add_goal(0, FloatGoal::new(mob_base));
    goals.add_goal(
        1,
        PanicGoal::new(1.2).with_panic_filter(|mob| {
            mob.as_abstract_horse()
                .is_none_or(|horse| !horse.is_mob_controlled())
        }),
    );
    goals.add_goal(
        3,
        TemptGoal::new(
            1.25,
            |item_stack| {
                REGISTRY
                    .items
                    .is_in_tag(item_stack.item(), &ItemTag::HORSE_TEMPT_ITEMS)
            },
            false,
        ),
    );
}

/// Pushes freshly changed mob-effect state into synchronized entity data.
///
/// Vanilla parity: the `LivingEntity.tick` effect sync. Every Foton mob repeats
/// this; the horse family shares one copy because eight of them would not.
pub(super) fn sync_mob_effect_entity_data<E, D>(entity: &E, data: &SyncMutex<D>)
where
    E: LivingEntity + ?Sized,
    D: VanillaLivingEntityData + VanillaEntityData + Send + Sync,
{
    if !entity.living_base().take_effects_dirty() {
        return;
    }

    let display = entity.living_base().mob_effect_display_state();
    {
        let mut entity_data = data.lock();
        let living = entity_data.living_entity_mut();
        living.effect_particles.set(display.particles);
        living.effect_ambience.set(display.ambient);
    }

    data.set_base_invisible_flag(display.invisible);
    data.set_base_glowing_flag(entity.has_glowing_tag() || display.glowing);
}
