//! Which hostile mob a parrot mimics, and what it sounds like doing it.
//!
//! Vanilla parity: `Parrot.MOB_SOUND_MAP`, the whole reason a parrot is worth
//! carrying: it tells you what is nearby before you see it.

use std::ptr;

use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::{sound_events, vanilla_entities};

/// The imitation table, in vanilla's order.
///
/// Vanilla parity: `Parrot.MOB_SOUND_MAP`. The happy ghast maps to
/// `SoundEvents.EMPTY` there, which Foton expresses by leaving it out -- an
/// absent entry falls back to the parrot's own chirp either way.
pub(super) static MOB_SOUND_MAP: &[(EntityTypeRef, SoundEventRef)] = &[
    (
        &vanilla_entities::BLAZE,
        &sound_events::ENTITY_PARROT_IMITATE_BLAZE,
    ),
    (
        &vanilla_entities::BOGGED,
        &sound_events::ENTITY_PARROT_IMITATE_BOGGED,
    ),
    (
        &vanilla_entities::BREEZE,
        &sound_events::ENTITY_PARROT_IMITATE_BREEZE,
    ),
    (
        &vanilla_entities::CAMEL_HUSK,
        &sound_events::ENTITY_PARROT_IMITATE_CAMEL_HUSK,
    ),
    (
        &vanilla_entities::CAVE_SPIDER,
        &sound_events::ENTITY_PARROT_IMITATE_SPIDER,
    ),
    (
        &vanilla_entities::CREAKING,
        &sound_events::ENTITY_PARROT_IMITATE_CREAKING,
    ),
    (
        &vanilla_entities::CREEPER,
        &sound_events::ENTITY_PARROT_IMITATE_CREEPER,
    ),
    (
        &vanilla_entities::DROWNED,
        &sound_events::ENTITY_PARROT_IMITATE_DROWNED,
    ),
    (
        &vanilla_entities::ELDER_GUARDIAN,
        &sound_events::ENTITY_PARROT_IMITATE_ELDER_GUARDIAN,
    ),
    (
        &vanilla_entities::ENDER_DRAGON,
        &sound_events::ENTITY_PARROT_IMITATE_ENDER_DRAGON,
    ),
    (
        &vanilla_entities::ENDERMITE,
        &sound_events::ENTITY_PARROT_IMITATE_ENDERMITE,
    ),
    (
        &vanilla_entities::EVOKER,
        &sound_events::ENTITY_PARROT_IMITATE_EVOKER,
    ),
    (
        &vanilla_entities::GHAST,
        &sound_events::ENTITY_PARROT_IMITATE_GHAST,
    ),
    (
        &vanilla_entities::GUARDIAN,
        &sound_events::ENTITY_PARROT_IMITATE_GUARDIAN,
    ),
    (
        &vanilla_entities::HOGLIN,
        &sound_events::ENTITY_PARROT_IMITATE_HOGLIN,
    ),
    (
        &vanilla_entities::HUSK,
        &sound_events::ENTITY_PARROT_IMITATE_HUSK,
    ),
    (
        &vanilla_entities::ILLUSIONER,
        &sound_events::ENTITY_PARROT_IMITATE_ILLUSIONER,
    ),
    (
        &vanilla_entities::MAGMA_CUBE,
        &sound_events::ENTITY_PARROT_IMITATE_MAGMA_CUBE,
    ),
    (
        &vanilla_entities::PARCHED,
        &sound_events::ENTITY_PARROT_IMITATE_PARCHED,
    ),
    (
        &vanilla_entities::PHANTOM,
        &sound_events::ENTITY_PARROT_IMITATE_PHANTOM,
    ),
    (
        &vanilla_entities::PIGLIN,
        &sound_events::ENTITY_PARROT_IMITATE_PIGLIN,
    ),
    (
        &vanilla_entities::PIGLIN_BRUTE,
        &sound_events::ENTITY_PARROT_IMITATE_PIGLIN_BRUTE,
    ),
    (
        &vanilla_entities::PILLAGER,
        &sound_events::ENTITY_PARROT_IMITATE_PILLAGER,
    ),
    (
        &vanilla_entities::RAVAGER,
        &sound_events::ENTITY_PARROT_IMITATE_RAVAGER,
    ),
    (
        &vanilla_entities::SHULKER,
        &sound_events::ENTITY_PARROT_IMITATE_SHULKER,
    ),
    (
        &vanilla_entities::SILVERFISH,
        &sound_events::ENTITY_PARROT_IMITATE_SILVERFISH,
    ),
    (
        &vanilla_entities::SKELETON,
        &sound_events::ENTITY_PARROT_IMITATE_SKELETON,
    ),
    (
        &vanilla_entities::SLIME,
        &sound_events::ENTITY_PARROT_IMITATE_SLIME,
    ),
    (
        &vanilla_entities::SPIDER,
        &sound_events::ENTITY_PARROT_IMITATE_SPIDER,
    ),
    (
        &vanilla_entities::STRAY,
        &sound_events::ENTITY_PARROT_IMITATE_STRAY,
    ),
    (
        &vanilla_entities::VEX,
        &sound_events::ENTITY_PARROT_IMITATE_VEX,
    ),
    (
        &vanilla_entities::VINDICATOR,
        &sound_events::ENTITY_PARROT_IMITATE_VINDICATOR,
    ),
    (
        &vanilla_entities::WARDEN,
        &sound_events::ENTITY_PARROT_IMITATE_WARDEN,
    ),
    (
        &vanilla_entities::WITCH,
        &sound_events::ENTITY_PARROT_IMITATE_WITCH,
    ),
    (
        &vanilla_entities::WITHER,
        &sound_events::ENTITY_PARROT_IMITATE_WITHER,
    ),
    (
        &vanilla_entities::WITHER_SKELETON,
        &sound_events::ENTITY_PARROT_IMITATE_WITHER_SKELETON,
    ),
    (
        &vanilla_entities::ZOGLIN,
        &sound_events::ENTITY_PARROT_IMITATE_ZOGLIN,
    ),
    (
        &vanilla_entities::ZOMBIE,
        &sound_events::ENTITY_PARROT_IMITATE_ZOMBIE,
    ),
    (
        &vanilla_entities::ZOMBIE_HORSE,
        &sound_events::ENTITY_PARROT_IMITATE_ZOMBIE_HORSE,
    ),
    (
        &vanilla_entities::ZOMBIE_NAUTILUS,
        &sound_events::ENTITY_PARROT_IMITATE_ZOMBIE_NAUTILUS,
    ),
    (
        &vanilla_entities::ZOMBIE_VILLAGER,
        &sound_events::ENTITY_PARROT_IMITATE_ZOMBIE_VILLAGER,
    ),
];

/// Returns whether a parrot has anything to say about this mob.
///
/// Vanilla parity: the `NOT_PARROT_PREDICATE` of `Parrot`, which despite its
/// name only asks whether the map has an entry.
#[must_use]
pub(super) fn is_imitable(entity_type: EntityTypeRef) -> bool {
    imitated_sound(entity_type).is_some()
}

/// Returns the sound a parrot makes at this mob.
///
/// Vanilla parity: `Parrot.getImitatedSound`, which falls back to the parrot's
/// own ambient sound for anything absent.
#[must_use]
pub(super) fn imitated_sound(entity_type: EntityTypeRef) -> Option<SoundEventRef> {
    MOB_SOUND_MAP
        .iter()
        .find(|(mob, _)| ptr::eq(*mob, entity_type))
        .map(|(_, sound)| *sound)
}
