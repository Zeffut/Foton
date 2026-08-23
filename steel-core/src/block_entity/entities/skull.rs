//! Skull and head block entity.
//!
//! Vanilla parity: `SkullBlockEntity`. Which creature a skull came from is in
//! the block, but whose head it is, is not -- the profile lives here, and it
//! is the only thing that tells one player head from another.
//!
//! Without this every player head placed in the world would be a blank one,
//! and the head that comes back when it is broken would have forgotten who it
//! belonged to. The same entity also carries the sound identifier a player
//! head lends to the note block underneath it.

use std::str::FromStr as _;
use std::sync::{Arc, Weak};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use simdnbt::{FromNbtTag as _, ToNbtTag as _};
use steel_registry::data_components::vanilla_components::ResolvableProfile;
use steel_registry::vanilla_block_entity_types;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Skull block entity, shared by every skull and head, standing or on a wall.
pub struct SkullBlockEntity {
    base: Arc<BlockEntityBase>,
    state: SyncMutex<SkullState>,
}

/// What a skull remembers beyond its block state.
#[derive(Default)]
struct SkullState {
    /// The profile a player head wears.
    owner: Option<ResolvableProfile>,
    /// The sound a note block under this head plays instead of an instrument.
    note_block_sound: Option<Identifier>,
    /// The name an anvil gave the head before it was placed.
    name: Option<TextComponent>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SkullBlockEntity`.
unsafe impl DowncastType for SkullBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/skull");
}

impl SkullBlockEntity {
    /// Creates a skull block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: Arc::new(BlockEntityBase::new(
                &vanilla_block_entity_types::SKULL,
                level,
                pos,
                state,
            )),
            state: SyncMutex::new(SkullState::default()),
        }
    }

    /// Returns the profile this head wears.
    ///
    /// Vanilla parity: `SkullBlockEntity.getOwnerProfile`.
    #[must_use]
    pub fn owner_profile(&self) -> Option<ResolvableProfile> {
        self.state.lock().owner.clone()
    }

    /// Returns the sound a note block under this head should play.
    ///
    /// Vanilla parity: `SkullBlockEntity.getNoteBlockSound`, read by
    /// `NoteBlock.getCustomSoundId`. Steel's note block cannot use it yet: the
    /// sound packet only carries registered sound events, while vanilla wraps
    /// this identifier in a direct `Holder<SoundEvent>`. Inline sound events in
    /// `CSound` are the missing system.
    #[must_use]
    pub fn note_block_sound(&self) -> Option<Identifier> {
        self.state.lock().note_block_sound.clone()
    }

    /// Returns the skull's custom name, if it was renamed.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.state.lock().name.clone()
    }

    /// Replaces what the skull remembers.
    ///
    /// Vanilla parity: `SkullBlockEntity.applyImplicitComponents`, which is
    /// what carries a head item's profile onto the block it becomes.
    pub fn set_from_item(
        &self,
        owner: Option<ResolvableProfile>,
        note_block_sound: Option<Identifier>,
        name: Option<TextComponent>,
    ) {
        {
            let mut state = self.state.lock();
            state.owner = owner;
            state.note_block_sound = note_block_sound;
            state.name = name;
        }
        self.set_changed();
    }
}

impl BlockEntity for SkullBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();
        state.owner = nbt_view
            .get("profile")
            .and_then(ResolvableProfile::from_nbt_tag);
        state.note_block_sound = nbt_view
            .string("note_block_sound")
            .and_then(|value| Identifier::from_str(&value.to_str()).ok());
        state.name = nbt_view
            .get("custom_name")
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        if let Some(owner) = &state.owner {
            nbt.insert("profile", owner.clone().to_nbt_tag());
        }
        if let Some(sound) = &state.note_block_sound {
            nbt.insert("note_block_sound", sound.to_string());
        }
        if let Some(name) = &state.name {
            nbt.insert("custom_name", name.to_codec_nbt());
        }
    }

    /// Vanilla parity: `SkullBlockEntity.getUpdateTag`, which sends the whole
    /// saved compound. The client draws the face from the profile, so it needs
    /// it the moment the chunk arrives rather than after an interaction.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}
