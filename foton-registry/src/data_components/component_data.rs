//! Type-erased data component values.

use std::fmt::{self, Debug, Formatter};

use foton_utils::{Downcast as _, DowncastType, DowncastTypeKey, ErasedType};

use super::components::{
    ArmorTrim, AttackRange, BannerPatternLayers, Bees, BlockEntityData, BlockItemStateProperties,
    BlocksAttacks, BundleContents, ChargedProjectiles, Consumable, CustomData, CustomModelData,
    DamageResistant, DamageTypeComponent, DeathProtection, DebugStickState, DyedItemColor,
    Enchantable, EntityData, Equippable, FireworkExplosion, Fireworks, FoodProperties,
    InstrumentComponent, ItemAttributeModifiers, ItemContainerContents, ItemEnchantments, ItemLore,
    JukeboxPlayable, KineticWeapon, LodestoneTracker, MapDecorations, MapId, MapItemColor,
    MapPostProcessing, OminousBottleAmplifier, PaintingVariantComponent, PiercingWeapon,
    PotDecorations, PotionContents, ProvidesBannerPatterns, ProvidesTrimMaterial, Rarity, Recipes,
    Repairable, SeededContainerLoot, SulfurCubeContent, SuspiciousStewEffects, SwingAnimation,
    Tool, TooltipDisplay, UseCooldown, UseEffects, UseRemainder, Weapon, WritableBookContent,
    WrittenBookContent,
};
use crate::cat_sound_variant::CatSoundVariant;
use crate::cat_variant::CatVariant;
use crate::chicken_sound_variant::ChickenSoundVariant;
use crate::chicken_variant::ChickenVariant;
use crate::cow_sound_variant::CowSoundVariant;
use crate::cow_variant::CowVariant;
use crate::frog_variant::FrogVariant;
use crate::item_predicate::{AdventureModePredicate, LockCode};
use crate::pig_sound_variant::PigSoundVariant;
use crate::pig_variant::PigVariant;
use crate::resolvable_profile::ResolvableProfile;
use crate::villager_type::VillagerType;
use crate::wolf_sound_variant::WolfSoundVariant;
use crate::wolf_variant::WolfVariant;
use crate::zombie_nautilus_variant::ZombieNautilusVariant;
use crate::{
    AxolotlVariant, DyeColor, FoxVariant, HorseVariant, LlamaVariant, MooshroomVariant,
    ParrotVariant, RabbitVariant, RegistryReference, SalmonVariant, TropicalFishPattern,
};

/// Behavior required from a value stored in a [`ComponentData`].
///
/// Concrete type recovery is provided by Foton's deterministic keyed
/// downcasting foundation. A value is eligible for the blanket implementation
/// when it also supports cloning, comparison, debugging, and shared server
/// access. Persistent-codec hashing is registered separately so transient
/// values do not need a fake hash representation.
pub trait Component: ErasedType + Debug + Send + Sync + 'static {
    #[doc(hidden)]
    fn clone_component(&self) -> Box<dyn Component>;

    #[doc(hidden)]
    fn component_eq(&self, other: &dyn Component) -> bool;
}

impl<T> Component for T
where
    T: DowncastType + Clone + Debug + PartialEq + Send + Sync,
{
    fn clone_component(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }

    fn component_eq(&self, other: &dyn Component) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

/// A type-erased component value.
///
/// Component values retain their concrete Rust type and can be recovered with
/// [`Self::downcast_ref`].
pub struct ComponentData {
    value: Box<dyn Component>,
}

impl ComponentData {
    /// Erases a typed component value.
    #[must_use]
    pub fn new(value: impl Component) -> Self {
        Self {
            value: Box::new(value),
        }
    }

    /// Returns the concrete value when it has type `T`.
    #[must_use]
    pub fn downcast_ref<T: DowncastType>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }

    /// Returns the concrete type key.
    #[must_use]
    pub fn type_key(&self) -> DowncastTypeKey {
        self.value.downcast_type_key()
    }
}

impl Clone for ComponentData {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone_component(),
        }
    }
}

impl Debug for ComponentData {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ComponentData")
            .field(&self.value)
            .finish()
    }
}

impl PartialEq for ComponentData {
    fn eq(&self, other: &Self) -> bool {
        self.value.component_eq(other.value.as_ref())
    }
}

macro_rules! impl_component_downcast_type {
    ($type:ty, $key:literal) => {
        // SAFETY: This Foton-owned key uniquely identifies the concrete
        // component implementation within the process.
        unsafe impl DowncastType for $type {
            const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new($key);
        }
    };
}

impl_component_downcast_type!(DamageTypeComponent, "foton:item_component/damage_type");
impl_component_downcast_type!(
    AdventureModePredicate,
    "foton:item_component/adventure_mode_predicate"
);
impl_component_downcast_type!(LockCode, "foton:item_component/lock");
impl_component_downcast_type!(CustomData, "foton:item_component/custom_data");
impl_component_downcast_type!(CustomModelData, "foton:item_component/custom_model_data");
impl_component_downcast_type!(DyeColor, "foton:dye_color");
impl_component_downcast_type!(FoxVariant, "foton:fox_variant");
impl_component_downcast_type!(SalmonVariant, "foton:salmon_variant");
impl_component_downcast_type!(ParrotVariant, "foton:parrot_variant");
impl_component_downcast_type!(TropicalFishPattern, "foton:tropical_fish_pattern");
impl_component_downcast_type!(MooshroomVariant, "foton:mooshroom_variant");
impl_component_downcast_type!(RabbitVariant, "foton:rabbit_variant");
impl_component_downcast_type!(HorseVariant, "foton:horse_variant");
impl_component_downcast_type!(LlamaVariant, "foton:llama_variant");
impl_component_downcast_type!(AxolotlVariant, "foton:axolotl_variant");
impl_component_downcast_type!(DyedItemColor, "foton:item_component/dyed_item_color");
impl_component_downcast_type!(MapItemColor, "foton:item_component/map_item_color");
impl_component_downcast_type!(MapId, "foton:item_component/map_id");
impl_component_downcast_type!(FoodProperties, "foton:item_component/food");
impl_component_downcast_type!(
    SuspiciousStewEffects,
    "foton:item_component/suspicious_stew_effects"
);
impl_component_downcast_type!(
    WritableBookContent,
    "foton:item_component/writable_book_content"
);
impl_component_downcast_type!(
    WrittenBookContent,
    "foton:item_component/written_book_content"
);
impl_component_downcast_type!(DebugStickState, "foton:item_component/debug_stick_state");
impl_component_downcast_type!(Bees, "foton:item_component/bees");
impl_component_downcast_type!(EntityData, "foton:item_component/entity_data");
impl_component_downcast_type!(BlockEntityData, "foton:item_component/block_entity_data");
impl_component_downcast_type!(KineticWeapon, "foton:item_component/kinetic_weapon");
impl_component_downcast_type!(LodestoneTracker, "foton:item_component/lodestone_tracker");
impl_component_downcast_type!(MapDecorations, "foton:item_component/map_decorations");
impl_component_downcast_type!(FireworkExplosion, "foton:item_component/firework_explosion");
impl_component_downcast_type!(Fireworks, "foton:item_component/fireworks");
impl_component_downcast_type!(BlockItemStateProperties, "foton:item_component/block_state");
impl_component_downcast_type!(BlocksAttacks, "foton:item_component/blocks_attacks");
impl_component_downcast_type!(Consumable, "foton:item_component/consumable");
impl_component_downcast_type!(DeathProtection, "foton:item_component/death_protection");
impl_component_downcast_type!(ResolvableProfile, "foton:item_component/profile");
impl_component_downcast_type!(SeededContainerLoot, "foton:item_component/container_loot");
impl_component_downcast_type!(
    OminousBottleAmplifier,
    "foton:item_component/ominous_bottle_amplifier"
);
impl_component_downcast_type!(Enchantable, "foton:item_component/enchantable");
impl_component_downcast_type!(InstrumentComponent, "foton:item_component/instrument");
impl_component_downcast_type!(ArmorTrim, "foton:item_component/trim");
impl_component_downcast_type!(Recipes, "foton:item_component/recipes");
impl_component_downcast_type!(PotDecorations, "foton:item_component/pot_decorations");
impl_component_downcast_type!(PotionContents, "foton:item_component/potion_contents");
impl_component_downcast_type!(UseRemainder, "foton:item_component/use_remainder");
impl_component_downcast_type!(
    ChargedProjectiles,
    "foton:item_component/charged_projectiles"
);
impl_component_downcast_type!(BundleContents, "foton:item_component/bundle_contents");
impl_component_downcast_type!(ItemContainerContents, "foton:item_component/container");
impl_component_downcast_type!(
    SulfurCubeContent,
    "foton:item_component/sulfur_cube_content"
);
impl_component_downcast_type!(BannerPatternLayers, "foton:item_component/banner_patterns");
impl_component_downcast_type!(
    PaintingVariantComponent,
    "foton:item_component/painting_variant"
);
impl_component_downcast_type!(
    ProvidesTrimMaterial,
    "foton:item_component/provides_trim_material"
);
impl_component_downcast_type!(JukeboxPlayable, "foton:item_component/jukebox_playable");
impl_component_downcast_type!(
    ProvidesBannerPatterns,
    "foton:item_component/provides_banner_patterns"
);
impl_component_downcast_type!(DamageResistant, "foton:item_component/damage_resistant");
impl_component_downcast_type!(Repairable, "foton:item_component/repairable");
impl_component_downcast_type!(Tool, "foton:item_component/tool");
impl_component_downcast_type!(Weapon, "foton:item_component/weapon");
impl_component_downcast_type!(AttackRange, "foton:item_component/attack_range");
impl_component_downcast_type!(UseCooldown, "foton:item_component/use_cooldown");
impl_component_downcast_type!(UseEffects, "foton:item_component/use_effects");
impl_component_downcast_type!(ItemLore, "foton:item_component/lore");
impl_component_downcast_type!(Rarity, "foton:item_component/rarity");
impl_component_downcast_type!(TooltipDisplay, "foton:item_component/tooltip_display");
impl_component_downcast_type!(SwingAnimation, "foton:item_component/swing_animation");
impl_component_downcast_type!(
    MapPostProcessing,
    "foton:item_component/map_post_processing"
);
impl_component_downcast_type!(PiercingWeapon, "foton:item_component/piercing_weapon");
impl_component_downcast_type!(Equippable, "foton:item_component/equippable");
impl_component_downcast_type!(
    ItemAttributeModifiers,
    "foton:item_component/attribute_modifiers"
);
impl_component_downcast_type!(ItemEnchantments, "foton:item_component/enchantments");
impl_component_downcast_type!(
    RegistryReference<VillagerType>,
    "foton:item_component/villager_variant"
);
impl_component_downcast_type!(
    RegistryReference<WolfVariant>,
    "foton:item_component/wolf_variant"
);
impl_component_downcast_type!(
    RegistryReference<WolfSoundVariant>,
    "foton:item_component/wolf_sound_variant"
);
impl_component_downcast_type!(
    RegistryReference<PigVariant>,
    "foton:item_component/pig_variant"
);
impl_component_downcast_type!(
    RegistryReference<PigSoundVariant>,
    "foton:item_component/pig_sound_variant"
);
impl_component_downcast_type!(
    RegistryReference<CowVariant>,
    "foton:item_component/cow_variant"
);
impl_component_downcast_type!(
    RegistryReference<CowSoundVariant>,
    "foton:item_component/cow_sound_variant"
);
impl_component_downcast_type!(
    RegistryReference<ChickenVariant>,
    "foton:item_component/chicken_variant"
);
impl_component_downcast_type!(
    RegistryReference<ChickenSoundVariant>,
    "foton:item_component/chicken_sound_variant"
);
impl_component_downcast_type!(
    RegistryReference<ZombieNautilusVariant>,
    "foton:item_component/zombie_nautilus_variant"
);
impl_component_downcast_type!(
    RegistryReference<FrogVariant>,
    "foton:item_component/frog_variant"
);
impl_component_downcast_type!(
    RegistryReference<CatVariant>,
    "foton:item_component/cat_variant"
);
impl_component_downcast_type!(
    RegistryReference<CatSoundVariant>,
    "foton:item_component/cat_sound_variant"
);

#[cfg(test)]
mod tests {
    use super::ComponentData;

    #[test]
    fn typed_values_downcast_by_deterministic_key() {
        let value = ComponentData::new(17_i32);

        assert_eq!(value.downcast_ref::<i32>(), Some(&17));
        assert_eq!(value.downcast_ref::<bool>(), None);
    }

    #[test]
    fn equality_requires_the_same_concrete_type() {
        assert_eq!(ComponentData::new(17_i32), ComponentData::new(17_i32));
        assert_ne!(ComponentData::new(17_i32), ComponentData::new(17.0_f32));
    }
}
