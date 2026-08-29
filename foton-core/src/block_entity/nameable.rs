//! The custom name a container block entity carries.
//!
//! Vanilla parity: the `name` field of `BaseContainerBlockEntity`, which every
//! chest, barrel, furnace, hopper, dispenser, brewing stand, beacon and shulker
//! box inherits along with `Nameable`. Rust has no inheritance, so each of them
//! owns a [`BlockEntityName`] and forwards to it -- the same shape
//! [`ContainerLoot`] already uses for the other half of that hierarchy.
//!
//! It is not decoration. 68 of the 71 vanilla block loot tables that use
//! `minecraft:copy_components` copy `minecraft:custom_name`, so this is what
//! decides whether an anvil-named container comes back named.
//!
//! [`ContainerLoot`]: crate::block_entity::ContainerLoot

use foton_registry::data_components::{DataComponentMap, vanilla_components::CUSTOM_NAME};
use foton_utils::locks::SyncMutex;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;

use super::ImplicitComponentInput;

/// Vanilla parity: the `"CustomName"` tag `BaseContainerBlockEntity` reads and
/// writes. The skull and the copper golem statue use a lowercase key instead
/// and keep their own field.
const CUSTOM_NAME_TAG: &str = "CustomName";

/// The name an anvil gave a block entity before it was placed.
#[derive(Debug, Default)]
pub struct BlockEntityName {
    name: SyncMutex<Option<TextComponent>>,
}

impl BlockEntityName {
    /// Creates an unnamed block entity name slot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Vanilla parity: `Nameable.getCustomName`, which is `None` for a block
    /// entity nobody renamed.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.name.lock().clone()
    }

    /// Replaces the name.
    pub fn set_custom_name(&self, name: Option<TextComponent>) {
        *self.name.lock() = name;
    }

    /// Vanilla parity: `BaseContainerBlockEntity.getName`, the title the menu
    /// opens with. An unnamed container falls back to its block's own name.
    #[must_use]
    pub fn display_name(&self, default_name: TextComponent) -> TextComponent {
        self.custom_name().unwrap_or(default_name)
    }

    /// Vanilla parity: `parseCustomNameSafe(input, "CustomName")`.
    pub fn load(&self, nbt: &BorrowedNbtCompoundView<'_, '_>) {
        let name = nbt
            .get(CUSTOM_NAME_TAG)
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt);
        self.set_custom_name(name);
    }

    /// Vanilla parity: `output.storeNullable("CustomName", ...)`.
    ///
    /// `simdnbt`'s `insert` appends rather than replaces and readers take the
    /// first match, so any earlier value is dropped first.
    pub fn save(&self, nbt: &mut NbtCompound) {
        while nbt.remove(CUSTOM_NAME_TAG).is_some() {}
        if let Some(name) = self.custom_name() {
            nbt.insert(CUSTOM_NAME_TAG, name.to_codec_nbt());
        }
    }

    /// Vanilla parity: the `CUSTOM_NAME` line of
    /// `BaseContainerBlockEntity.collectImplicitComponents`. An unnamed
    /// container removes the component rather than leaving a stale one.
    pub fn collect_implicit_components(&self, components: &mut DataComponentMap) {
        components.set(CUSTOM_NAME, self.custom_name());
    }

    /// Vanilla parity: the `CUSTOM_NAME` line of
    /// `BaseContainerBlockEntity.applyImplicitComponents`.
    pub fn apply_implicit_components(&self, input: &ImplicitComponentInput<'_>) {
        self.set_custom_name(input.get(CUSTOM_NAME));
    }
}
