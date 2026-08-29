//! Banner block entity.
//!
//! Vanilla parity: `BannerBlockEntity`. A banner's color is in the block, but
//! its pattern layers and its name are not -- they live here, and they are the
//! only reason a placed banner is anything but a colored sheet.
//!
//! Without this the loom's output is a dead end: a banner stamped with six
//! layers becomes a plain one the moment it is put down, and the item that
//! comes back when it is broken has lost them too.

use std::sync::{Arc, Weak};

use foton_registry::data_components::DataComponentMap;
use foton_registry::data_components::components::{BannerPatternLayer, BannerPatternLayers};
use foton_registry::data_components::vanilla_components::{BANNER_PATTERNS, CUSTOM_NAME};
use foton_registry::dye_color::DyeColor;
use foton_registry::registry::holder::RegistryHolder;
use foton_registry::vanilla_block_entity_types;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::{FromNbtTag as _, ToNbtTag as _};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase, ImplicitComponentInput};
use crate::world::World;

/// Banner block entity, shared by the standing and wall forms.
pub struct BannerBlockEntity {
    base: Arc<BlockEntityBase>,
    state: SyncMutex<BannerState>,
}

/// What a banner remembers beyond its block state.
struct BannerState {
    patterns: BannerPatternLayers,
    name: Option<TextComponent>,
}

impl Default for BannerState {
    fn default() -> Self {
        Self {
            patterns: BannerPatternLayers::empty(),
            name: None,
        }
    }
}

// SAFETY: This key is owned by Foton and uniquely identifies `BannerBlockEntity`.
unsafe impl DowncastType for BannerBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/banner");
}

impl BannerBlockEntity {
    /// Creates a banner block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: Arc::new(BlockEntityBase::new(
                &vanilla_block_entity_types::BANNER,
                level,
                pos,
                state,
            )),
            state: SyncMutex::new(BannerState::default()),
        }
    }

    /// Returns the pattern layers stamped on this banner.
    #[must_use]
    pub fn patterns(&self) -> BannerPatternLayers {
        self.state.lock().patterns.clone()
    }

    /// Returns the banner's custom name, if it was renamed.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.state.lock().name.clone()
    }
}

impl BlockEntity for BannerBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut layers = Vec::new();
        if let Some(list) = nbt_view.list("patterns")
            && let Some(compounds) = list.compounds()
        {
            for compound in compounds {
                // `BannerPatternLayer` only reads itself from a whole tag, and
                // a borrowed compound cannot be wrapped back into one, so the
                // two fields are read here instead.
                let layer = compound
                    .get("pattern")
                    .and_then(RegistryHolder::from_nbt_tag)
                    .zip(compound.get("color").and_then(DyeColor::from_nbt_tag));
                if let Some((pattern, color)) = layer {
                    layers.push(BannerPatternLayer::new(pattern, color));
                }
            }
        }

        let mut state = self.state.lock();
        state.patterns = BannerPatternLayers::new(layers);
        state.name = nbt_view
            .get("CustomName")
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        if !state.patterns.layers().is_empty() {
            let layers: Vec<NbtCompound> = state
                .patterns
                .layers()
                .iter()
                .filter_map(|layer| match layer.clone().to_nbt_tag() {
                    NbtTag::Compound(compound) => Some(compound),
                    _ => None,
                })
                .collect();
            nbt.insert("patterns", NbtList::Compound(layers));
        }
        if let Some(name) = &state.name {
            nbt.insert("CustomName", name.to_codec_nbt());
        }
    }

    /// Vanilla parity: `BannerBlockEntity.getUpdateTag`. The client draws the
    /// layers itself, so it needs them the moment the chunk arrives.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }

    /// Vanilla parity: `BannerBlockEntity.collectImplicitComponents`. This is
    /// what the `minecraft:banner_patterns` entry of every banner loot table
    /// reads, so it is the whole reason a broken banner is still the banner the
    /// loom made.
    fn collect_implicit_components(&self, components: &mut DataComponentMap) {
        let state = self.state.lock();
        components.set(BANNER_PATTERNS, Some(state.patterns.clone()));
        components.set(CUSTOM_NAME, state.name.clone());
    }

    /// Vanilla parity: `BannerBlockEntity.applyImplicitComponents`, which is
    /// what carries an item's layers onto the banner just placed.
    fn apply_implicit_components(&self, input: &ImplicitComponentInput<'_>) {
        let patterns = input.get_or_default(BANNER_PATTERNS, BannerPatternLayers::empty());
        let name = input.get(CUSTOM_NAME);
        let mut state = self.state.lock();
        state.patterns = patterns;
        state.name = name;
    }
}
