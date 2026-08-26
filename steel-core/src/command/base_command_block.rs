//! The command store shared by the command block and its minecart.
//!
//! Vanilla parity: `net.minecraft.world.level.BaseCommandBlock`. Both a
//! `CommandBlockEntity` and a `MinecartCommandBlock` own one of these; it holds
//! the command text, the success count a comparator reads, the last output the
//! editor shows, and the game-time stamp that stops one block running twice in
//! a tick.
//!
//! What it does *not* own is where the command runs from, or how the result
//! reaches a client. A block reports its own position and resends itself; a
//! minecart reports wherever it has rolled to and writes synced data. Both of
//! those live with the owner, which passes them in.

use std::time::SystemTime;

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_utils::locks::SyncMutex;
use text_components::{Modifier as _, TextComponent};

/// The name a command block reports when it has none of its own.
///
/// Vanilla parity: `BaseCommandBlock.DEFAULT_NAME`.
const DEFAULT_NAME: &str = "@";

/// The game time meaning "this has never run".
///
/// Vanilla parity: `BaseCommandBlock.NO_LAST_EXECUTION`.
const NO_LAST_EXECUTION: i64 = -1;

/// The mutable half of a command block.
#[derive(Debug)]
struct CommandBlockState {
    command: String,
    success_count: i32,
    track_output: bool,
    last_output: Option<TextComponent>,
    custom_name: Option<TextComponent>,
    last_execution: i64,
    update_last_execution: bool,
}

impl CommandBlockState {
    const fn new() -> Self {
        Self {
            command: String::new(),
            success_count: 0,
            track_output: true,
            last_output: None,
            custom_name: None,
            last_execution: NO_LAST_EXECUTION,
            update_last_execution: true,
        }
    }
}

/// One command block's stored command and its results.
pub struct BaseCommandBlock {
    state: SyncMutex<CommandBlockState>,
}

impl BaseCommandBlock {
    /// Creates an empty command store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SyncMutex::new(CommandBlockState::new()),
        }
    }

    /// Returns the stored command.
    #[must_use]
    pub fn command(&self) -> String {
        self.state.lock().command.clone()
    }

    /// Stores a command and clears the success count.
    ///
    /// Vanilla parity: `BaseCommandBlock.setCommand`, which resets
    /// `successCount` so a comparator does not keep reading the old result.
    pub fn set_command(&self, command: String) {
        let mut state = self.state.lock();
        state.command = command;
        state.success_count = 0;
    }

    /// Returns what a comparator reads off this block.
    ///
    /// Vanilla parity: `BaseCommandBlock.getSuccessCount`.
    #[must_use]
    pub fn success_count(&self) -> i32 {
        self.state.lock().success_count
    }

    /// Sets the success count.
    pub fn set_success_count(&self, success_count: i32) {
        self.state.lock().success_count = success_count;
    }

    /// Returns whether this block keeps its last output.
    #[must_use]
    pub fn tracks_output(&self) -> bool {
        self.state.lock().track_output
    }

    /// Sets whether this block keeps its last output.
    pub fn set_track_output(&self, track_output: bool) {
        self.state.lock().track_output = track_output;
    }

    /// Returns the last output, or an empty component when there is none.
    ///
    /// Vanilla parity: `BaseCommandBlock.getLastOutput`.
    #[must_use]
    pub fn last_output(&self) -> TextComponent {
        self.state.lock().last_output.clone().unwrap_or_default()
    }

    /// Replaces the last output without notifying the owner.
    pub fn set_last_output(&self, last_output: Option<TextComponent>) {
        self.state.lock().last_output = last_output;
    }

    /// Returns the custom name, if one was set.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.state.lock().custom_name.clone()
    }

    /// Sets the custom name.
    pub fn set_custom_name(&self, custom_name: Option<TextComponent>) {
        self.state.lock().custom_name = custom_name;
    }

    /// Returns the name this block signs its output with.
    ///
    /// Vanilla parity: `BaseCommandBlock.getName`, which falls back to `@`.
    #[must_use]
    pub fn name(&self) -> TextComponent {
        self.custom_name()
            .unwrap_or_else(|| TextComponent::plain(DEFAULT_NAME))
    }

    /// Records one line of command output.
    ///
    /// Vanilla parity: `BaseCommandBlock.CloseableCommandBlockSource.sendSystemMessage`,
    /// which stamps the output with the wall-clock time so the editor shows
    /// when the command last spoke.
    ///
    /// Vanilla also calls `onUpdated` here, resending the block once per output
    /// line. Steel's owners publish once after the command instead: only the
    /// last line is kept, and the packet carries the whole block entity, so the
    /// state a client ends up with is the same.
    pub fn record_output(&self, message: &TextComponent) {
        if !self.tracks_output() {
            return;
        }

        let stamped =
            TextComponent::plain(format!("[{}] ", local_clock_time())).add_child(message.clone());
        self.set_last_output(Some(stamped));
    }

    /// Returns whether this block already ran on `game_time`.
    ///
    /// Vanilla parity: the `level.getGameTime() == this.lastExecution` guard of
    /// `performCommand`, which is what stops a chain looping on itself inside a
    /// single tick.
    #[must_use]
    pub fn already_ran_at(&self, game_time: i64) -> bool {
        self.state.lock().last_execution == game_time
    }

    /// Stamps this block as having run on `game_time`.
    pub fn mark_ran_at(&self, game_time: i64) {
        let mut state = self.state.lock();
        state.last_execution = if state.update_last_execution {
            game_time
        } else {
            NO_LAST_EXECUTION
        };
    }

    /// Writes the command block's fields.
    ///
    /// Vanilla parity: `BaseCommandBlock.save`, key for key.
    pub fn save(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("Command", state.command.clone());
        nbt.insert("SuccessCount", state.success_count);
        if let Some(custom_name) = &state.custom_name {
            nbt.insert("CustomName", custom_name.to_codec_nbt());
        }
        nbt.insert("TrackOutput", state.track_output);
        if state.track_output
            && let Some(last_output) = &state.last_output
        {
            nbt.insert("LastOutput", last_output.to_codec_nbt());
        }
        nbt.insert("UpdateLastExecution", state.update_last_execution);
        if state.update_last_execution && state.last_execution != NO_LAST_EXECUTION {
            nbt.insert("LastExecution", state.last_execution);
        }
    }

    /// Reads the command block's fields.
    ///
    /// Vanilla parity: `BaseCommandBlock.load`, including its defaults: a
    /// missing `TrackOutput` means true, and an untracked block forgets its
    /// output rather than keeping a stale one.
    pub fn load(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        state.command = nbt
            .string("Command")
            .map(|value| value.to_string())
            .unwrap_or_default();
        state.success_count = nbt.int("SuccessCount").unwrap_or(0);
        state.custom_name = component_at(nbt, "CustomName");
        state.track_output = nbt.byte("TrackOutput").is_none_or(|value| value != 0);
        state.last_output = if state.track_output {
            component_at(nbt, "LastOutput")
        } else {
            None
        };
        state.update_last_execution = nbt
            .byte("UpdateLastExecution")
            .is_none_or(|value| value != 0);
        state.last_execution = if state.update_last_execution {
            nbt.long("LastExecution").unwrap_or(NO_LAST_EXECUTION)
        } else {
            NO_LAST_EXECUTION
        };
    }
}

impl Default for BaseCommandBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads one text component out of a borrowed compound.
fn component_at(nbt: BorrowedNbtCompoundView<'_, '_>, key: &str) -> Option<TextComponent> {
    nbt.get(key)
        .map(|tag| tag.to_owned())
        .as_ref()
        .and_then(TextComponent::from_nbt)
}

/// Returns the local wall-clock time as `HH:MM:SS`.
///
/// Vanilla parity: the `DateTimeFormatter.ofPattern("HH:mm:ss")` of
/// `CloseableCommandBlockSource`.
///
/// Steel has no local-timezone source, so this is UTC. The stamp is cosmetic --
/// it only ever appears in the command block editor's output line.
fn local_clock_time() -> String {
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let day_seconds = seconds % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        day_seconds / 3600,
        (day_seconds % 3600) / 60,
        day_seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::*;

    fn reload(nbt: &NbtCompound) -> BaseCommandBlock {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let block = BaseCommandBlock::new();
        block.load((&borrowed).into());
        block
    }

    /// The command and the success count are what a comparator and the editor
    /// read, so both have to survive a chunk unload.
    #[test]
    fn the_command_and_its_success_count_round_trip() {
        let block = BaseCommandBlock::new();
        block.set_command("say hello".to_owned());
        block.set_success_count(3);

        let mut nbt = NbtCompound::new();
        block.save(&mut nbt);
        assert_eq!(
            nbt.string("Command").map(ToString::to_string),
            Some("say hello".to_owned())
        );
        assert_eq!(nbt.int("SuccessCount"), Some(3));

        let reloaded = reload(&nbt);
        assert_eq!(reloaded.command(), "say hello");
        assert_eq!(reloaded.success_count(), 3);
    }

    /// Setting a new command clears the old result. Without this a comparator
    /// beside a freshly retyped block would keep reading the previous run.
    #[test]
    fn typing_a_new_command_clears_the_success_count() {
        let block = BaseCommandBlock::new();
        block.set_success_count(7);
        block.set_command("say hello".to_owned());
        assert_eq!(block.success_count(), 0);
    }

    /// Vanilla's `getBooleanOr("TrackOutput", true)`: a block written before the
    /// key existed still tracks its output.
    #[test]
    fn a_block_saved_without_track_output_still_tracks_it() {
        assert!(reload(&NbtCompound::new()).tracks_output());
    }

    /// An untracked block drops its output on load rather than reviving a stale
    /// one, and never writes `LastOutput` in the first place.
    #[test]
    fn an_untracked_block_keeps_no_output() {
        let block = BaseCommandBlock::new();
        block.set_last_output(Some(TextComponent::plain("old")));
        block.set_track_output(false);

        let mut nbt = NbtCompound::new();
        block.save(&mut nbt);
        assert!(nbt.compound("LastOutput").is_none());

        let reloaded = reload(&nbt);
        assert!(!reloaded.tracks_output());
        assert_eq!(reloaded.last_output(), TextComponent::default());
    }

    /// Output is only recorded while tracking is on -- the editor's checkbox is
    /// what stops a chatty command filling the block's memory.
    #[test]
    fn output_is_dropped_while_tracking_is_off() {
        let block = BaseCommandBlock::new();
        block.set_track_output(false);
        block.record_output(&TextComponent::plain("noisy"));
        assert_eq!(block.last_output(), TextComponent::default());
    }

    /// The execution stamp is what stops a chain re-entering the same block in
    /// one tick.
    #[test]
    fn a_block_will_not_run_twice_on_the_same_game_time() {
        let block = BaseCommandBlock::new();
        assert!(!block.already_ran_at(40));
        block.mark_ran_at(40);
        assert!(block.already_ran_at(40));
        assert!(!block.already_ran_at(41));
    }
}
