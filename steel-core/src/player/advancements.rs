//! The player's half of the advancement engine.
//!
//! Vanilla parity: the parts of `PlayerAdvancements` that need a live player --
//! the reward grant, the chat announcement and the packets. The bookkeeping
//! itself is in [`crate::advancement::PlayerAdvancements`], which returns what
//! happened so no lock is ever held across a send.

use std::time::{SystemTime, UNIX_EPOCH};

use steel_protocol::packets::game::{CSelectAdvancementsTab, CSystemChat, CUpdateAdvancements};
use steel_registry::advancement::{AdvancementRewards, AdvancementType};
use steel_registry::vanilla_game_rules::SHOW_ADVANCEMENT_MESSAGES;
use steel_utils::Identifier;
use steel_utils::translations;
use text_components::interactivity::HoverEvent;
use text_components::{Modifier as _, TextComponent};

use crate::advancement::{ADVANCEMENT_TREE, AwardOutcome, TabSelection};
use crate::entity::Entity as _;

use super::Player;

/// The moment an award is stamped with.
///
/// Vanilla parity: `CriterionProgress.grant`, which takes `Instant.now()`.
fn now_epoch_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

impl Player {
    /// Sends whatever the advancement screen still has to be told.
    ///
    /// Vanilla parity: the `this.advancements.flushDirty(this, true)` that ends
    /// `ServerPlayer.tick`. `show_advancements` is always true here for the
    /// same reason it is in vanilla: only `/advancement ... everything` ever
    /// passes false, to suppress a thousand toasts at once.
    pub(super) fn flush_dirty_advancements(&self) {
        let update = self.advancements.lock().flush_dirty();
        let Some(update) = update else {
            return;
        };
        self.send_packet(CUpdateAdvancements::new(
            update.reset,
            update.added,
            update.removed,
            update.progress,
            true,
            self,
        ));
    }

    /// Marks one criterion met, handing out the rewards and announcing the
    /// advancement if that finished it.
    ///
    /// Vanilla parity: `PlayerAdvancements.award`.
    pub fn award_advancement_criterion(&self, node: usize, criterion: &str) -> AwardOutcome {
        let outcome = self
            .advancements
            .lock()
            .award(node, criterion, now_epoch_millis());
        if outcome.completed {
            self.complete_advancement(node);
        }
        outcome
    }

    /// Marks one criterion unmet.
    ///
    /// Vanilla parity: `PlayerAdvancements.revoke`.
    pub fn revoke_advancement_criterion(&self, node: usize, criterion: &str) -> bool {
        self.advancements.lock().revoke(node, criterion)
    }

    /// Marks every criterion of one advancement met.
    pub fn award_advancement(&self, node: usize) -> AwardOutcome {
        let outcome = self.advancements.lock().award_all(node, now_epoch_millis());
        if outcome.completed {
            self.complete_advancement(node);
        }
        outcome
    }

    /// Marks every criterion of one advancement unmet.
    pub fn revoke_advancement(&self, node: usize) -> bool {
        self.advancements.lock().revoke_all(node)
    }

    /// Whether this player has finished an advancement.
    #[must_use]
    pub fn has_advancement(&self, node: usize) -> bool {
        self.advancements.lock().is_done(node)
    }

    fn complete_advancement(&self, node: usize) {
        let advancement = ADVANCEMENT_TREE.node(node).advancement;
        self.grant_advancement_rewards(&advancement.rewards);

        let Some(display) = advancement.display.as_ref() else {
            return;
        };
        if !display.announce_chat {
            return;
        }
        let world = self.get_world();
        if !world.get_game_rule(&SHOW_ADVANCEMENT_MESSAGES) {
            return;
        }

        let announcement = display
            .advancement_type
            .announcement()
            .message([
                self.display_name(),
                decorated_name(
                    display.advancement_type,
                    &display.title,
                    &display.description,
                ),
            ])
            .component();
        // Vanilla broadcasts this to the whole player list; Steel's death
        // messages already broadcast per world, and the two follow the same
        // rule so an announcement never crosses a domain boundary alone.
        world.broadcast_system_chat(CSystemChat {
            content: announcement,
            overlay: false,
        });
    }

    /// Hands out what finishing an advancement is worth.
    ///
    /// Vanilla parity: `AdvancementRewards.grant`. Vanilla's own advancement
    /// data only ever fills in `experience` and `recipes`; the loot and function
    /// rewards a datapack could add are logged rather than silently dropped.
    fn grant_advancement_rewards(&self, rewards: &AdvancementRewards) {
        if rewards.experience != 0 {
            self.give_experience_points(rewards.experience);
        }
        // The recipe rewards are what unlock a recipe in the recipe book.
        // Steel has no recipe book yet, so there is nothing to unlock into;
        // the criterion itself is still awarded, so the progress is not lost.
        if !rewards.loot.is_empty() || rewards.function.is_some() {
            log::warn!(
                "advancement reward carries loot or a function, which Steel does not grant yet"
            );
        }
    }

    /// Points the client's advancement screen at a tab it asked for.
    ///
    /// Vanilla parity: `ServerGamePacketListenerImpl.handleSeenAdvancements`
    /// plus `PlayerAdvancements.setSelectedTab`. An unknown tab is ignored
    /// outright, and a known-but-invalid one clears the selection.
    pub fn handle_seen_advancements(&self, tab: Option<Identifier>) {
        let Some(tab) = tab else {
            // Vanilla ignores `CLOSED_SCREEN` entirely.
            return;
        };
        let Some(node) = ADVANCEMENT_TREE.index_of(&tab) else {
            return;
        };
        let echo = self.advancements.lock().set_selected_tab(Some(node));
        let packet = match echo {
            None => return,
            Some(TabSelection::Cleared) => CSelectAdvancementsTab::none(),
            Some(TabSelection::Selected(tab)) => CSelectAdvancementsTab::select(tab),
        };
        self.send_packet(packet);
    }
}

/// The bracketed, colored, hoverable advancement name a chat announcement
/// carries.
///
/// Vanilla parity: `Advancement.decorateName`.
fn decorated_name(
    advancement_type: AdvancementType,
    title: &TextComponent,
    description: &TextComponent,
) -> TextComponent {
    let color = advancement_type.chat_color();
    let tooltip = title
        .clone()
        .color(color.clone())
        .add_child(TextComponent::const_plain("\n"))
        .add_child(description.clone());
    let hovered = title.clone().hover_event(HoverEvent::show_text(tooltip));
    translations::CHAT_SQUARE_BRACKETS
        .message([hovered])
        .component()
        .color(color)
}
