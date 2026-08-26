//! What a swallowed block turns a sulfur cube into.
//!
//! Vanilla parity: `net.minecraft.world.entity.SulfurCubeArchetype`, a data pack
//! registry keyed by an item tag. A sulfur cube that swallows an item takes on
//! every archetype whose `items` tag holds it, so the block in its body decides
//! its bounce, its friction, whether it floats, whether it explodes, whether
//! touching it hurts, and which sounds it makes.

use rustc_hash::FxHashMap;
use steel_utils::Identifier;
use steel_utils::value_providers::FloatProvider;

use crate::attribute::{AttributeModifierOperation, AttributeRef};
use crate::damage_type::DamageTypeRef;
use crate::items::Item;
use crate::registry::RegistryHolderSet;
use crate::sound_event::SoundEventRef;

/// One attribute modifier an archetype applies while its block is swallowed.
///
/// Vanilla parity: `SulfurCubeArchetype.AttributeEntry`, which pairs an
/// attribute with the `AttributeModifier` to add transiently.
#[derive(Debug)]
pub struct SulfurCubeAttributeEntry {
    /// The attribute the modifier applies to.
    pub attribute: AttributeRef,
    /// The modifier's identity, which is also how it is taken off again.
    pub id: Identifier,
    /// The modifier amount.
    pub amount: f64,
    /// How the amount combines with the base value.
    pub operation: AttributeModifierOperation,
}

/// What an archetype blows up like.
///
/// Vanilla parity: `SulfurCubeArchetype.ExplosionData`.
#[derive(Debug, Clone, Copy)]
pub struct SulfurCubeExplosion {
    /// Blast radius, in the same units as TNT's four.
    pub power: i32,
    /// Whether the blast leaves fire behind.
    pub causes_fire: bool,
    /// Ticks between priming and the blast.
    pub fuse: i32,
}

/// What touching an archetype costs.
///
/// Vanilla parity: `SulfurCubeArchetype.ContactDamage`.
#[derive(Debug, Clone, Copy)]
pub struct SulfurCubeContactDamage {
    /// The damage type dealt on contact.
    pub damage_type: DamageTypeRef,
    /// How much, rolled per hit.
    pub amount: FloatProvider,
    /// Whether the cube is named as the attacker.
    pub attribute_to_source: bool,
}

/// How hard an archetype is knocked around when it is hit.
///
/// Vanilla parity: `SulfurCubeArchetype.KnockbackModifiers`.
#[derive(Debug, Clone, Copy)]
pub struct SulfurCubeKnockbackModifiers {
    /// Sideways power scale.
    pub horizontal_power: f32,
    /// Upward power scale.
    pub vertical_power: f32,
}

/// The two sounds an archetype makes, and how often the push one may repeat.
///
/// Vanilla parity: `SulfurCubeArchetype.SoundSettings`.
#[derive(Debug, Clone, Copy)]
pub struct SulfurCubeSoundSettings {
    /// Played when the cube is knocked by an attack.
    pub hit_sound: SoundEventRef,
    /// Played when a player shoves the cube hard enough.
    pub push_sound: SoundEventRef,
    /// Impulse below which a shove stays silent.
    pub push_sound_impulse_threshold: f32,
    /// Seconds between push sounds.
    pub push_sound_cooldown: f32,
}

/// Vanilla parity: `SulfurCubeArchetype.DEFAULT_KNOCKBACK_MODIFIERS`, what a
/// cube with an empty body is knocked around by.
pub const DEFAULT_KNOCKBACK_MODIFIERS: SulfurCubeKnockbackModifiers =
    SulfurCubeKnockbackModifiers {
        horizontal_power: 0.33,
        vertical_power: 0.06,
    };

/// Vanilla parity: `SulfurCubeArchetype.DEFAULT_SOUND_SETTINGS`, which is the
/// regular archetype's pair of sounds. An empty cube still makes a noise when
/// it is hit, and this is the one it makes.
pub const DEFAULT_SOUND_SETTINGS: SulfurCubeSoundSettings = SulfurCubeSoundSettings {
    hit_sound: &crate::sound_events::ENTITY_SULFUR_CUBE_REGULAR_HIT,
    push_sound: &crate::sound_events::ENTITY_SULFUR_CUBE_REGULAR_PUSH,
    push_sound_impulse_threshold: 0.2,
    push_sound_cooldown: 0.5,
};

/// A full sulfur cube archetype definition from a data pack JSON file.
#[derive(Debug)]
pub struct SulfurCubeArchetype {
    /// Registry key.
    pub key: Identifier,
    /// The items whose block this archetype describes.
    pub items: RegistryHolderSet<Item>,
    /// Attributes the archetype adds while its block is swallowed.
    pub attribute_modifiers: &'static [SulfurCubeAttributeEntry],
    /// Whether the cube rides up through water and lava rather than sinking.
    pub buoyant: bool,
    /// The blast, when this archetype has one.
    pub explosion: Option<SulfurCubeExplosion>,
    /// The contact damage, when this archetype has one.
    pub contact_damage: Option<SulfurCubeContactDamage>,
    /// How hard this archetype is knocked around.
    pub knockback_modifiers: SulfurCubeKnockbackModifiers,
    /// Which sounds this archetype makes.
    pub sound_settings: SulfurCubeSoundSettings,
}

/// A borrowed archetype from the registry.
pub type SulfurCubeArchetypeRef = &'static SulfurCubeArchetype;

/// The sulfur cube archetype registry.
pub struct SulfurCubeArchetypeRegistry {
    sulfur_cube_archetypes_by_id: Vec<SulfurCubeArchetypeRef>,
    sulfur_cube_archetypes_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl SulfurCubeArchetypeRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sulfur_cube_archetypes_by_id: Vec::new(),
            sulfur_cube_archetypes_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    SulfurCubeArchetypeRegistry,
    SulfurCubeArchetypeRef,
    sulfur_cube_archetypes_by_id,
    sulfur_cube_archetypes_by_key,
    allows_registering
);

crate::impl_registry!(
    SulfurCubeArchetypeRegistry,
    SulfurCubeArchetype,
    sulfur_cube_archetypes_by_id,
    sulfur_cube_archetypes_by_key,
    sulfur_cube_archetypes
);
