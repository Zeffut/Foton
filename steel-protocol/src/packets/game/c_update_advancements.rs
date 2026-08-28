//! Clientbound update advancements packet -- what the advancement screen
//! draws and how far the player has got through it.
//!
//! Vanilla parity: `ClientboundUpdateAdvancementsPacket`, together with the
//! `AdvancementHolder`, `Advancement` and `DisplayInfo` stream codecs it
//! delegates to. Vanilla keeps the removals in a set and the progress in a
//! map; Steel keeps ordered vectors, so the same packet always encodes to the
//! same bytes and can be tested against them.

use std::io::{Result, Write};

use steel_macros::ClientPacket;
use steel_registry::advancement::{AdvancementProgress, AdvancementRef, DisplayInfo};
use steel_registry::packets::play::C_UPDATE_ADVANCEMENTS;
use steel_utils::Identifier;
use steel_utils::codec::VarInt;
use steel_utils::serial::{PrefixedWrite as _, WriteTo};
use text_components::TextComponent;
use text_components::resolving::TextResolutor;

/// Set when the advancement has a tab background, which only a root does.
const DISPLAY_HAS_BACKGROUND: i32 = 1;
/// Set when earning the advancement pops a toast.
const DISPLAY_SHOW_TOAST: i32 = 2;
/// Set when the advancement stays hidden until it is earned.
const DISPLAY_HIDDEN: i32 = 4;

/// One entry of the `added` list: an advancement plus the two text components
/// resolved for the player this packet is addressed to.
///
/// The resolved text is kept next to the definition rather than replacing it,
/// so the id, the parent, the icon, the layout and the requirements are still
/// read straight off the registry entry. Building one is the only way to pair
/// the two, which keeps the resolved text from drifting away from the
/// advancement it belongs to.
#[derive(Debug, Clone)]
pub struct AddedAdvancement {
    advancement: AdvancementRef,
    /// The display's `title` and `description`, resolved. Present exactly when
    /// the advancement has a display.
    resolved_text: Option<(TextComponent, TextComponent)>,
}

impl AddedAdvancement {
    /// Pairs `advancement` with its title and description resolved for
    /// `player`.
    #[must_use]
    pub fn new<T: TextResolutor>(advancement: AdvancementRef, player: &T) -> Self {
        let resolved_text = advancement.display.as_ref().map(|display| {
            (
                display.title.resolve(player),
                display.description.resolve(player),
            )
        });
        Self {
            advancement,
            resolved_text,
        }
    }

    /// The definition this entry describes.
    #[must_use]
    pub const fn advancement(&self) -> AdvancementRef {
        self.advancement
    }
}

/// Writes one `DisplayInfo` with its text already resolved.
///
/// Vanilla parity: `DisplayInfo.serializeToNetwork`. `announce_chat` is
/// deliberately not on the wire -- the server is the one that broadcasts that
/// message, and the client's decoder hard-codes `false` for it.
fn write_display(
    display: &DisplayInfo,
    title: &TextComponent,
    description: &TextComponent,
    writer: &mut impl Write,
) -> Result<()> {
    title.write(writer)?;
    description.write(writer)?;
    display.icon.template().write(writer)?;
    VarInt(display.advancement_type as i32).write(writer)?;

    let mut flags = 0;
    if display.background.is_some() {
        flags |= DISPLAY_HAS_BACKGROUND;
    }
    if display.show_toast {
        flags |= DISPLAY_SHOW_TOAST;
    }
    if display.hidden {
        flags |= DISPLAY_HIDDEN;
    }
    // Vanilla uses `writeInt` here, not `writeVarInt`: the flags are four
    // big-endian bytes even though they never exceed 7.
    flags.write(writer)?;

    if let Some(background) = display.background.as_ref() {
        background.write(writer)?;
    }
    display.x.write(writer)?;
    display.y.write(writer)
}

impl WriteTo for AddedAdvancement {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.advancement.key.write(writer)?;
        self.advancement.parent.write(writer)?;

        match (&self.advancement.display, &self.resolved_text) {
            (Some(display), Some((title, description))) => {
                true.write(writer)?;
                write_display(display, title, description, writer)?;
            }
            _ => false.write(writer)?,
        }

        // Vanilla: `AdvancementRequirements.write`, a list of lists of strings.
        let groups = self.advancement.requirements.groups;
        VarInt(i32::try_from(groups.len()).unwrap_or(i32::MAX)).write(writer)?;
        for group in groups {
            VarInt(i32::try_from(group.len()).unwrap_or(i32::MAX)).write(writer)?;
            for name in *group {
                name.write_prefixed::<VarInt>(writer)?;
            }
        }

        self.advancement.sends_telemetry_event.write(writer)
    }
}

/// Adds, removes and re-scores the advancements one client knows about.
///
/// Vanilla parity: `ClientboundUpdateAdvancementsPacket`. Neither the criteria
/// nor the rewards travel: the client only needs the tree, the artwork and the
/// progress, and it rebuilds each advancement's requirements from the entry it
/// was sent.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_UPDATE_ADVANCEMENTS)]
pub struct CUpdateAdvancements {
    /// Whether the client throws away everything it already had first.
    pub reset: bool,
    /// The advancements the client is told about, in wire order.
    pub added: Vec<AddedAdvancement>,
    /// The advancements the client forgets.
    pub removed: Vec<Identifier>,
    /// The progress to apply, keyed by advancement.
    pub progress: Vec<(Identifier, AdvancementProgress)>,
    /// Whether the advancement screen can be opened at all. New in 26.2.
    pub show_advancements: bool,
}

impl CUpdateAdvancements {
    /// Builds the packet, resolving every added advancement's title and
    /// description for `player`.
    #[must_use]
    pub fn new<T: TextResolutor>(
        reset: bool,
        added: impl IntoIterator<Item = AdvancementRef>,
        removed: Vec<Identifier>,
        progress: Vec<(Identifier, AdvancementProgress)>,
        show_advancements: bool,
        player: &T,
    ) -> Self {
        Self {
            reset,
            added: added
                .into_iter()
                .map(|advancement| AddedAdvancement::new(advancement, player))
                .collect(),
            removed,
            progress,
            show_advancements,
        }
    }
}

impl WriteTo for CUpdateAdvancements {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.reset.write(writer)?;
        self.added.write(writer)?;
        self.removed.write(writer)?;
        self.progress.write(writer)?;
        // Easy to forget: 26.2 added this after the progress map.
        self.show_advancements.write(writer)
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::advancement::AdvancementProgress;
    use steel_registry::init_vanilla_registry;
    use steel_registry::vanilla_advancements::STORY_ROOT;
    use text_components::resolving::NoResolutor;

    use super::*;

    /// When `story/root`'s only criterion was met, in epoch milliseconds.
    const OBTAINED: i64 = 1_234;

    fn encoded(value: &impl WriteTo) -> Vec<u8> {
        let mut bytes = Vec::new();
        value
            .write(&mut bytes)
            .expect("writing to a Vec cannot fail");
        bytes
    }

    /// `story/root` with its one criterion met.
    fn story_root_progress() -> AdvancementProgress {
        let mut progress = AdvancementProgress::new();
        progress.update(STORY_ROOT.requirements);
        assert!(
            progress.grant("crafting_table", OBTAINED),
            "story/root's criterion is called crafting_table"
        );
        progress
    }

    /// One added advancement and one progress entry, as a player would get on
    /// login.
    fn story_root_packet(show_advancements: bool) -> CUpdateAdvancements {
        CUpdateAdvancements::new(
            true,
            [&STORY_ROOT],
            Vec::new(),
            vec![(STORY_ROOT.key.clone(), story_root_progress())],
            show_advancements,
            &NoResolutor,
        )
    }

    /// The whole packet, byte for byte. The three opaque runs -- the two text
    /// components' NBT and the icon's item stack template -- are encoded on
    /// their own rather than spelled out, so the parts this packet is
    /// responsible for stay readable.
    #[test]
    fn an_added_advancement_encodes_exactly_as_vanilla_reads_it() {
        init_vanilla_registry();

        let display = STORY_ROOT.display.as_ref().expect("story/root is drawn");
        let title = encoded(&display.title.resolve(&NoResolutor));
        let description = encoded(&display.description.resolve(&NoResolutor));
        let icon = encoded(display.icon.template());
        let key = encoded(&STORY_ROOT.key);
        let background = encoded(
            display
                .background
                .as_ref()
                .expect("story/root has a tab background"),
        );

        let mut expected = Vec::new();
        expected.push(1); // reset
        expected.push(1); // one added advancement
        expected.extend_from_slice(&key);
        expected.push(0); // no parent: story/root is a root
        expected.push(1); // it has a display
        expected.extend_from_slice(&title);
        expected.extend_from_slice(&description);
        expected.extend_from_slice(&icon);
        expected.push(0); // frame: task, written as a var-int
        expected.extend_from_slice(&[0, 0, 0, 1]); // flags: a plain int, background only
        expected.extend_from_slice(&background);
        expected.extend_from_slice(&0.0f32.to_be_bytes()); // x
        expected.extend_from_slice(&1.75f32.to_be_bytes()); // y
        expected.push(1); // one requirement group
        expected.push(1); // holding one criterion
        expected.push(14);
        expected.extend_from_slice(b"crafting_table");
        expected.push(1); // sends_telemetry_event
        expected.push(0); // nothing removed
        expected.push(1); // one progress entry
        expected.extend_from_slice(&key);
        expected.push(1); // covering one criterion
        expected.push(14);
        expected.extend_from_slice(b"crafting_table");
        expected.push(1); // which was obtained
        expected.extend_from_slice(&OBTAINED.to_be_bytes());
        expected.push(1); // show_advancements

        assert_eq!(encoded(&story_root_packet(true)), expected);
    }

    /// The display flags are `writeInt`, not `writeVarInt`. A var-int would
    /// encode `story/root`'s flags as the single byte `1` and shift every
    /// following field three bytes earlier, which desynchronizes the whole
    /// packet rather than failing loudly.
    #[test]
    fn the_display_flags_occupy_four_bytes() {
        init_vanilla_registry();

        let display = STORY_ROOT.display.as_ref().expect("story/root is drawn");
        let title = encoded(&display.title.resolve(&NoResolutor));
        let description = encoded(&display.description.resolve(&NoResolutor));
        let icon = encoded(display.icon.template());
        let key = encoded(&STORY_ROOT.key);
        let background = encoded(
            display
                .background
                .as_ref()
                .expect("story/root has a tab background"),
        );

        // reset, added count, id, absent parent, present display, the three
        // opaque runs, and the one-byte frame var-int.
        let flags_at = 1 + 1 + key.len() + 1 + 1 + title.len() + description.len() + icon.len() + 1;
        let bytes = encoded(&story_root_packet(true));

        assert_eq!(&bytes[flags_at..flags_at + 4], &[0, 0, 0, 1]);
        // The background identifier follows the flags, and only because the
        // background bit was set.
        assert_eq!(
            &bytes[flags_at + 4..flags_at + 4 + background.len()],
            &background[..]
        );
    }

    /// 26.2 appended `show_advancements` after the progress map. Dropping it
    /// leaves the client one byte short at the very end of a packet that is
    /// otherwise correct.
    #[test]
    fn the_packet_ends_with_the_show_advancements_flag() {
        init_vanilla_registry();

        let shown = encoded(&story_root_packet(true));
        let hidden = encoded(&story_root_packet(false));

        assert_eq!(shown.len(), hidden.len());
        assert_eq!(shown[..shown.len() - 1], hidden[..hidden.len() - 1]);
        assert_eq!(shown.last(), Some(&1));
        assert_eq!(hidden.last(), Some(&0));
    }

    /// The removals and the progress are independent lists, and an empty one
    /// still costs its var-int length.
    #[test]
    fn an_empty_update_is_a_reset_flag_three_counts_and_the_screen_flag() {
        init_vanilla_registry();

        let packet =
            CUpdateAdvancements::new(false, [], Vec::new(), Vec::new(), true, &NoResolutor);

        assert_eq!(encoded(&packet), vec![0, 0, 0, 0, 1]);
    }

    /// Removals carry nothing but their identifiers.
    #[test]
    fn removals_are_written_as_bare_identifiers() {
        init_vanilla_registry();

        let key = STORY_ROOT.key.clone();
        let packet = CUpdateAdvancements::new(
            false,
            [],
            vec![key.clone()],
            Vec::new(),
            false,
            &NoResolutor,
        );

        let mut expected = vec![0, 0, 1];
        expected.extend_from_slice(&encoded(&key));
        expected.extend_from_slice(&[0, 0]);

        assert_eq!(encoded(&packet), expected);
    }
}
