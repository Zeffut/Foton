//! Text display entity.
//!
//! Vanilla parity: `Display.TextDisplay`. The hologram that servers used to
//! fake with an invisible named armor stand, promoted to a real entity: a
//! block of text that wraps at a chosen width, can be aligned, shadowed, made
//! see-through, and given its own background color.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entity_data::TextDisplayEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;
use uuid::Uuid;

use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData};
use crate::world::World;

/// Draws a drop shadow behind the glyphs.
///
/// Vanilla parity: `Display.TextDisplay.FLAG_SHADOW`.
pub const FLAG_SHADOW: i8 = 1;

/// Draws the text through blocks, like a glowing name tag.
///
/// Vanilla parity: `Display.TextDisplay.FLAG_SEE_THROUGH`.
pub const FLAG_SEE_THROUGH: i8 = 2;

/// Uses the viewer's own text-background color instead of `background`.
///
/// Vanilla parity: `Display.TextDisplay.FLAG_USE_DEFAULT_BACKGROUND`.
pub const FLAG_USE_DEFAULT_BACKGROUND: i8 = 4;

/// Alignment bit for left-aligned text.
///
/// Vanilla parity: `Display.TextDisplay.FLAG_ALIGN_LEFT`.
pub const FLAG_ALIGN_LEFT: i8 = 8;

/// Alignment bit for right-aligned text.
///
/// Vanilla parity: `Display.TextDisplay.FLAG_ALIGN_RIGHT`.
pub const FLAG_ALIGN_RIGHT: i8 = 16;

/// Wrap width in pixels before a line breaks.
///
/// Vanilla parity: `Display.TextDisplay.INITIAL_LINE_WIDTH`.
const INITIAL_LINE_WIDTH: i32 = 200;

/// Default background, a quarter-opaque black in ARGB.
///
/// Vanilla parity: `Display.TextDisplay.INITIAL_BACKGROUND`.
const INITIAL_BACKGROUND: i32 = 1_073_741_824;

/// Default opacity byte. `-1` is vanilla's "fully opaque" sentinel.
///
/// Vanilla parity: `Display.TextDisplay.INITIAL_TEXT_OPACITY`.
const INITIAL_TEXT_OPACITY: i8 = -1;

/// Where each wrapped line sits inside the block of text.
///
/// Vanilla parity: `Display.TextDisplay.Align`. Vanilla keeps this in two bits
/// of the style-flags byte rather than a field of its own, so the NBT name and
/// the synced byte disagree on shape and both have to be converted by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// Lines are centered on the entity.
    Center,
    /// Lines share a left edge.
    Left,
    /// Lines share a right edge.
    Right,
}

impl TextAlign {
    /// Reads the alignment out of a style-flags byte.
    ///
    /// Vanilla parity: `Display.TextDisplay.getAlign`. Left wins when both
    /// alignment bits are set, exactly as vanilla orders the checks.
    #[must_use]
    pub const fn from_flags(flags: i8) -> Self {
        if flags & FLAG_ALIGN_LEFT != 0 {
            Self::Left
        } else if flags & FLAG_ALIGN_RIGHT != 0 {
            Self::Right
        } else {
            Self::Center
        }
    }

    /// Returns the flag bits this alignment contributes to the style byte.
    #[must_use]
    const fn flag_bits(self) -> i8 {
        match self {
            Self::Center => 0,
            Self::Left => FLAG_ALIGN_LEFT,
            Self::Right => FLAG_ALIGN_RIGHT,
        }
    }

    /// Returns the vanilla NBT name for this alignment.
    ///
    /// Vanilla parity: `Display.TextDisplay.Align.getSerializedName`.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// Parses a vanilla NBT alignment name.
    #[must_use]
    pub fn from_serialized_name(name: &str) -> Option<Self> {
        match name {
            "center" => Some(Self::Center),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

/// A text display entity.
///
/// Vanilla parity: `Display.TextDisplay`. Like its `BlockDisplay` sibling this
/// carries the subclass state only. The shared `Display` layer -- transformation,
/// billboard mode, brightness, view range, shadow, culling size and glow color --
/// exists in the synced data but is neither persisted nor interpolated here,
/// because Foton has no display render-state system; see `BlockDisplayEntity`
/// for the same gap.
#[entity_behavior(class = "TextDisplay")]
pub struct TextDisplayEntity {
    /// Common entity fields (id, uuid, position, etc.).
    base: EntityBase,
    /// Vanilla entity type registered for this implementation.
    entity_type: EntityTypeRef,
    /// Synced entity data for network serialization.
    entity_data: SyncMutex<TextDisplayEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `TextDisplayEntity`.
unsafe impl DowncastType for TextDisplayEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/text_display");
}

impl TextDisplayEntity {
    /// Creates a new text display entity.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(TextDisplayEntityData::new()),
        }
    }

    /// Creates a new text display entity with a specific UUID.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn with_uuid(
        entity_type: EntityTypeRef,
        id: i32,
        position: DVec3,
        uuid: Uuid,
        world: Weak<World>,
    ) -> Self {
        Self {
            base: EntityBase::with_uuid(id, uuid, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(TextDisplayEntityData::new()),
        }
    }

    /// Creates a text display entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(TextDisplayEntityData::new()),
        }
    }

    /// Gets a reference to the entity data for reading/modifying synced state.
    pub const fn entity_data(&self) -> &SyncMutex<TextDisplayEntityData> {
        &self.entity_data
    }

    /// Returns the text this display shows.
    #[must_use]
    pub fn text(&self) -> TextComponent {
        (**self.entity_data.lock().text.get()).clone()
    }

    /// Sets the text this display shows.
    pub fn set_text(&self, text: TextComponent) {
        self.entity_data.lock().text.set(Box::new(text));
    }

    /// Returns the wrap width in pixels.
    #[must_use]
    pub fn line_width(&self) -> i32 {
        *self.entity_data.lock().line_width.get()
    }

    /// Sets the wrap width in pixels.
    pub fn set_line_width(&self, line_width: i32) {
        self.entity_data.lock().line_width.set(line_width);
    }

    /// Returns the packed ARGB background color.
    #[must_use]
    pub fn background_color(&self) -> i32 {
        *self.entity_data.lock().background_color.get()
    }

    /// Sets the packed ARGB background color.
    pub fn set_background_color(&self, background_color: i32) {
        self.entity_data
            .lock()
            .background_color
            .set(background_color);
    }

    /// Returns the text opacity byte.
    #[must_use]
    pub fn text_opacity(&self) -> i8 {
        *self.entity_data.lock().text_opacity.get()
    }

    /// Sets the text opacity byte.
    pub fn set_text_opacity(&self, text_opacity: i8) {
        self.entity_data.lock().text_opacity.set(text_opacity);
    }

    /// Returns the packed style flags.
    #[must_use]
    pub fn style_flags(&self) -> i8 {
        *self.entity_data.lock().style_flags.get()
    }

    /// Sets the packed style flags.
    pub fn set_style_flags(&self, style_flags: i8) {
        self.entity_data.lock().style_flags.set(style_flags);
    }

    /// Returns where wrapped lines sit inside the block of text.
    #[must_use]
    pub fn align(&self) -> TextAlign {
        TextAlign::from_flags(self.style_flags())
    }
}

impl Entity for TextDisplayEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Display.isIgnoringBlockTriggers`.
    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    /// Vanilla parity: `Display.TextDisplay.addAdditionalSaveData`.
    ///
    /// The shared `Display.addAdditionalSaveData` half is not written, matching
    /// `BlockDisplayEntity`.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        let data = self.entity_data.lock();
        let flags = *data.style_flags.get();
        nbt.insert("text", data.text.get().to_codec_nbt());
        nbt.insert("line_width", *data.line_width.get());
        nbt.insert("background", *data.background_color.get());
        nbt.insert("text_opacity", *data.text_opacity.get());
        drop(data);

        // Vanilla parity: `Display.TextDisplay.storeFlag`, one boolean per bit.
        nbt.insert("shadow", i8::from(flags & FLAG_SHADOW != 0));
        nbt.insert("see_through", i8::from(flags & FLAG_SEE_THROUGH != 0));
        nbt.insert(
            "default_background",
            i8::from(flags & FLAG_USE_DEFAULT_BACKGROUND != 0),
        );
        nbt.insert("alignment", TextAlign::from_flags(flags).serialized_name());
    }

    /// Vanilla parity: `Display.TextDisplay.readAdditionalSaveData`.
    ///
    /// Deviation: vanilla resolves selectors and scores in the saved component
    /// through `ComponentUtils.resolve` with a gamemaster command source. Foton
    /// has no component resolution context, so the component is restored as
    /// written.
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut data = self.entity_data.lock();
        data.line_width
            .set(nbt.int("line_width").unwrap_or(INITIAL_LINE_WIDTH));
        data.text_opacity
            .set(nbt.byte("text_opacity").unwrap_or(INITIAL_TEXT_OPACITY));
        data.background_color
            .set(nbt.int("background").unwrap_or(INITIAL_BACKGROUND));

        // Vanilla parity: `Display.TextDisplay.loadFlag`, absent means unset.
        let mut flags = 0_i8;
        flags |= load_flag(nbt, "shadow", FLAG_SHADOW);
        flags |= load_flag(nbt, "see_through", FLAG_SEE_THROUGH);
        flags |= load_flag(nbt, "default_background", FLAG_USE_DEFAULT_BACKGROUND);
        if let Some(align) = nbt
            .string("alignment")
            .and_then(|name| TextAlign::from_serialized_name(&name.to_str()))
        {
            flags |= align.flag_bits();
        }
        data.style_flags.set(flags);

        if let Some(text) = nbt
            .get("text")
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt)
        {
            data.text.set(Box::new(text));
        }
    }
}

/// Turns one saved boolean into its style-flag bit.
///
/// Vanilla parity: `Display.TextDisplay.loadFlag`.
fn load_flag(nbt: BorrowedNbtCompoundView<'_, '_>, key: &str, mask: i8) -> i8 {
    if nbt.byte(key).is_some_and(|value| value != 0) {
        mask
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use simdnbt::borrow::read_compound;

    use super::*;
    use crate::entity::{EntityBaseSaveData, EntityFireFreezeState};

    fn reload(entity: &TextDisplayEntity) -> TextDisplayEntity {
        let mut saved = NbtCompound::new();
        entity.save_additional(&mut saved);
        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let Ok(borrowed) = read_compound(&mut Cursor::new(bytes.as_slice())) else {
            panic!("saved text display NBT should reborrow");
        };
        let loaded = TextDisplayEntity::from_saved(
            &vanilla_entities::TEXT_DISPLAY,
            EntityBaseLoad {
                id: 2,
                position: DVec3::ZERO,
                uuid: Uuid::nil(),
                velocity: DVec3::ZERO,
                rotation: (0.0, 0.0),
                fall_distance: 0.0,
                fire_freeze: EntityFireFreezeState::new(),
                on_ground: false,
                save_data: EntityBaseSaveData::new(),
                world: Weak::new(),
            },
        );
        loaded.load_additional((&borrowed).into());
        loaded
    }

    #[test]
    fn a_saved_text_display_comes_back_with_its_text_layout_and_style_bits() {
        init_vanilla_registry();
        let entity = TextDisplayEntity::new(
            &vanilla_entities::TEXT_DISPLAY,
            1,
            DVec3::new(0.5, 65.0, 0.5),
            Weak::new(),
        );
        entity.set_text(TextComponent::plain("hello hologram"));
        entity.set_line_width(72);
        entity.set_background_color(0x40FF_00FF_u32 as i32);
        entity.set_text_opacity(96);
        entity.set_style_flags(FLAG_SHADOW | FLAG_SEE_THROUGH | FLAG_ALIGN_RIGHT);

        let loaded = reload(&entity);

        assert_eq!(loaded.text(), TextComponent::plain("hello hologram"));
        assert_eq!(loaded.line_width(), 72);
        assert_eq!(loaded.background_color(), 0x40FF_00FF_u32 as i32);
        assert_eq!(loaded.text_opacity(), 96);
        assert_eq!(
            loaded.style_flags(),
            FLAG_SHADOW | FLAG_SEE_THROUGH | FLAG_ALIGN_RIGHT
        );
        assert_eq!(loaded.align(), TextAlign::Right);
    }

    #[test]
    fn a_text_display_saved_with_no_style_reloads_on_vanilla_defaults() {
        init_vanilla_registry();
        let entity =
            TextDisplayEntity::new(&vanilla_entities::TEXT_DISPLAY, 1, DVec3::ZERO, Weak::new());

        let loaded = reload(&entity);

        assert_eq!(loaded.line_width(), INITIAL_LINE_WIDTH);
        assert_eq!(loaded.background_color(), INITIAL_BACKGROUND);
        assert_eq!(loaded.text_opacity(), INITIAL_TEXT_OPACITY);
        assert_eq!(loaded.style_flags(), 0);
        assert_eq!(loaded.align(), TextAlign::Center);
    }

    #[test]
    fn left_alignment_wins_over_right_because_vanilla_checks_it_first() {
        assert_eq!(
            TextAlign::from_flags(FLAG_ALIGN_LEFT | FLAG_ALIGN_RIGHT),
            TextAlign::Left
        );
    }
}
