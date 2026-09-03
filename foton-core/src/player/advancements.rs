//! The player's half of the advancement engine.
//!
//! Vanilla parity: the parts of `PlayerAdvancements` that need a live player --
//! the reward grant, the chat announcement and the packets. The bookkeeping
//! itself is in [`crate::advancement::PlayerAdvancements`], which returns what
//! happened so no lock is ever held across a send.

use std::time::{SystemTime, UNIX_EPOCH};

use foton_protocol::packets::game::{CSelectAdvancementsTab, CSystemChat, CUpdateAdvancements};
use foton_registry::advancement::{AdvancementRewards, AdvancementType};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_game_rules::SHOW_ADVANCEMENT_MESSAGES;
use foton_utils::Identifier;
use foton_utils::translations;
use text_components::interactivity::HoverEvent;
use text_components::{Modifier as _, TextComponent};

use crate::advancement::{ADVANCEMENT_TREE, AwardOutcome, CriterionRef, TabSelection};
use crate::entity::Entity as _;
use crate::inventory::container::Container as _;

use super::Player;
use super::player_inventory::PlayerInventory;
use crate::event::{PlayerAdvancementCriterionGrantEvent, PlayerAdvancementDoneEvent};

/// The player inventory and the stacks that changed in it.
///
/// Vanilla parity: the two things `InventoryChangeTrigger` needs -- the whole
/// container, for the slot counts and the multi-predicate sweep, and the stack
/// a slot changed to, which a single-predicate criterion tests on its own.
pub struct InventoryChange {
    /// Every slot of the player inventory, in slot order.
    pub items: Vec<ItemStack>,
    /// What the slots that changed now hold.
    pub changed: Vec<ItemStack>,
}

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
        let advancement = ADVANCEMENT_TREE.node(node).advancement;
        let mut grant_event = PlayerAdvancementCriterionGrantEvent::new(
            self.uuid(),
            advancement.key.to_string(),
            criterion.to_owned(),
        );
        if let Some(server) = self.server.upgrade() {
            server.events.fire(&mut grant_event);
        }
        if grant_event.is_cancelled() {
            return AwardOutcome::default();
        }
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
        let advancement = ADVANCEMENT_TREE.node(node).advancement;
        let mut outcome = AwardOutcome::default();
        for criterion in advancement.criteria {
            let step = self.award_advancement_criterion(node, criterion.name);
            outcome.granted |= step.granted;
            outcome.completed |= step.completed;
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

    /// Every advancement with progress, in the shape the save file keeps.
    ///
    /// Vanilla parity: `PlayerAdvancements.asData`.
    #[must_use]
    pub fn saved_advancements(&self) -> Vec<(Identifier, Vec<(&'static str, i64)>)> {
        self.advancements.lock().save_data()
    }

    /// Replaces this player's progress with what was saved.
    ///
    /// Vanilla parity: `PlayerAdvancements.applyFrom` on a freshly built
    /// counter -- so it replaces rather than merges, and the client is sent a
    /// resetting first packet on the next tick.
    pub fn load_advancements(
        &self,
        saved: impl IntoIterator<Item = (Identifier, Vec<(String, i64)>)>,
    ) {
        let mut advancements = self.advancements.lock();
        advancements.reset();
        advancements.load(saved);
    }

    /// Forgets every advancement, for a player entering a domain they have
    /// never visited.
    pub fn reset_advancements(&self) {
        self.advancements.lock().reset();
    }

    /// The criteria of `trigger_id` this player could still be awarded.
    ///
    /// Vanilla parity: `PlayerAdvancements.getTriggerMapForType`, whose map is
    /// the set of criteria still listening for that trigger.
    #[must_use]
    pub fn pending_advancement_criteria(&self, trigger_id: &str) -> Vec<CriterionRef> {
        self.advancements.lock().pending(trigger_id)
    }

    /// Reads the inventory and reports which slots moved since the last call.
    ///
    /// Vanilla parity: the `lastSlots` comparison of
    /// `AbstractContainerMenu.triggerSlotListeners`, including its side effect
    /// -- the snapshot is updated here, so a slot is reported once per change.
    #[must_use]
    pub fn take_inventory_change(&self) -> Option<InventoryChange> {
        let inventory = self.inventory.lock();
        let mut last = self.last_seen_inventory.lock();

        let mut items = Vec::with_capacity(PlayerInventory::CONTAINER_SIZE);
        let mut changed = Vec::new();
        for slot in 0..PlayerInventory::CONTAINER_SIZE {
            let item = inventory.get_item(slot).clone();
            if !ItemStack::matches(&last[slot], &item) {
                last[slot] = item.clone();
                changed.push(item.clone());
            }
            items.push(item);
        }

        if changed.is_empty() {
            return None;
        }
        Some(InventoryChange { items, changed })
    }

    fn complete_advancement(&self, node: usize) {
        let advancement = ADVANCEMENT_TREE.node(node).advancement;
        self.grant_advancement_rewards(&advancement.rewards);
        let mut event = PlayerAdvancementDoneEvent::new(self.uuid(), advancement.key.to_string());
        if let Some(server) = self.server.upgrade() {
            server.events.fire(&mut event);
        }

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
        // Vanilla broadcasts this to the whole player list; Foton's death
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
        // Foton has no recipe book yet, so there is nothing to unlock into;
        // the criterion itself is still awarded, so the progress is not lost.
        if !rewards.loot.is_empty() || rewards.function.is_some() {
            log::warn!(
                "advancement reward carries loot or a function, which Foton does not grant yet"
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
