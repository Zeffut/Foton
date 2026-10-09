//! Objectives, scores and display slots of a domain scoreboard.
//!
//! Vanilla parity: `Scoreboard` for the model, and `ServerScoreboard` for the
//! packets. An objective is *tracked* when some display slot shows it; only
//! then does a client know it exists, so only then do its scores, look and
//! removal produce packets. Every change queues the packets vanilla would
//! broadcast, and the server sends them once per tick (see
//! [`Scoreboard::take_objective_updates`]).

use std::{collections::btree_map::Entry, mem};

use foton_protocol::packets::game::{
    CResetScore, CSetDisplayObjective, CSetObjective, CSetScore, DisplaySlot, NumberFormat,
    ObjectiveLook, ObjectiveMethod, ObjectiveRenderType,
};
use foton_utils::translations;
use text_components::{Modifier as _, TextComponent, interactivity::HoverEvent};

use super::{
    ObjectiveCriteria, ObjectiveState, PersistentScoreboard, ScoreHolder, Scoreboard,
    ScoreboardError, ScoreboardObjective, ScoreboardScore, ensure_holder_name,
    ensure_objective_exists, ensure_objective_name, ensure_writable_objective,
};

/// A packet a client needs to keep its copy of the scoreboard current.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScoreboardPacket {
    /// `ClientboundSetObjectivePacket`.
    Objective(CSetObjective),
    /// `ClientboundSetScorePacket`.
    Score(CSetScore),
    /// `ClientboundResetScorePacket`.
    Reset(CResetScore),
    /// `ClientboundSetDisplayObjectivePacket`.
    Display(CSetDisplayObjective),
}

/// Everything about one objective, as one coherent read.
///
/// Vanilla parity: `Objective`.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveData {
    /// `Objective.getName`.
    pub name: String,
    /// `Objective.getCriteria`.
    pub criteria: ObjectiveCriteria,
    /// `Objective.getDisplayName`.
    pub display_name: TextComponent,
    /// `Objective.getRenderType`.
    pub render_type: ObjectiveRenderType,
    /// `Objective.displayAutoUpdate`.
    pub display_auto_update: bool,
    /// `Objective.numberFormat`.
    pub number_format: Option<NumberFormat>,
}

impl ObjectiveData {
    /// `Objective.getFormattedDisplayName`: the display name in square
    /// brackets, hovering to the objective's name.
    #[must_use]
    pub fn formatted_display_name(&self) -> TextComponent {
        let name =
            self.display_name
                .clone()
                .hover_event(HoverEvent::show_text(TextComponent::plain(
                    self.name.clone(),
                )));
        translations::CHAT_SQUARE_BRACKETS
            .message([name])
            .component()
    }
}

fn objective_data(name: &str, state: &ObjectiveState) -> ObjectiveData {
    ObjectiveData {
        name: name.to_owned(),
        criteria: ObjectiveCriteria::by_name(&state.criteria)
            .unwrap_or_else(ObjectiveCriteria::dummy),
        display_name: state
            .display_name
            .clone()
            .unwrap_or_else(|| TextComponent::plain(name.to_owned())),
        render_type: state.render_type,
        display_auto_update: state.display_auto_update,
        number_format: state.number_format.clone(),
    }
}

impl PersistentScoreboard {
    fn is_displayed(&self, objective: &str) -> bool {
        self.display_slots.values().any(|shown| shown == objective)
    }

    /// `Scoreboard.getObjectiveDisplaySlotCount`.
    fn display_slot_count(&self, objective: &str) -> usize {
        self.slots_showing(objective).count()
    }

    fn slots_showing<'a>(&'a self, objective: &'a str) -> impl Iterator<Item = DisplaySlot> + 'a {
        DisplaySlot::VALUES.into_iter().filter(move |slot| {
            self.display_slots
                .get(slot.serialized_name())
                .is_some_and(|shown| shown == objective)
        })
    }

    fn look(&self, objective: &str) -> Option<ObjectiveLook> {
        let data = objective_data(objective, self.objectives.get(objective)?);
        Some(ObjectiveLook {
            display_name: data.display_name,
            render_type: data.render_type,
            number_format: data.number_format,
        })
    }

    fn objective_packet(&self, objective: &str, add: bool) -> Option<ScoreboardPacket> {
        let look = self.look(objective)?;
        Some(ScoreboardPacket::Objective(CSetObjective {
            name: objective.to_owned(),
            method: if add {
                ObjectiveMethod::Add(look)
            } else {
                ObjectiveMethod::Change(look)
            },
        }))
    }

    fn removal_packet(objective: &str) -> ScoreboardPacket {
        ScoreboardPacket::Objective(CSetObjective {
            name: objective.to_owned(),
            method: ObjectiveMethod::Remove,
        })
    }

    fn display_packet(slot: DisplaySlot, objective: Option<&str>) -> ScoreboardPacket {
        ScoreboardPacket::Display(CSetDisplayObjective {
            slot,
            objective_name: objective.unwrap_or_default().to_owned(),
        })
    }

    fn score_packet(holder: &str, objective: &str, score: &ScoreboardScore) -> ScoreboardPacket {
        ScoreboardPacket::Score(CSetScore {
            owner: holder.to_owned(),
            objective_name: objective.to_owned(),
            score: score.value,
            display: score.display.clone(),
            number_format: score.number_format.clone(),
        })
    }

    /// The score packet for a change, if some slot makes clients track it.
    fn tracked_score_packet(
        &self,
        holder: &str,
        objective: &str,
        score: &ScoreboardScore,
    ) -> Option<ScoreboardPacket> {
        self.is_displayed(objective)
            .then(|| Self::score_packet(holder, objective, score))
    }

    /// Vanilla parity: `ServerScoreboard.getStartTrackingPackets`.
    fn start_tracking_packets(&self, objective: &str) -> Vec<ScoreboardPacket> {
        let mut packets = Vec::new();
        packets.extend(self.objective_packet(objective, true));
        packets.extend(
            self.slots_showing(objective)
                .map(|slot| Self::display_packet(slot, Some(objective))),
        );
        for (holder, scores) in &self.scores {
            if let Some(score) = scores.get(objective) {
                packets.push(Self::score_packet(holder, objective, score));
            }
        }
        packets
    }

    /// Vanilla parity: `Scoreboard.setDisplayObjective` as `ServerScoreboard`
    /// overrides it, returning the packets that broadcast the change.
    fn change_display(
        &mut self,
        slot: DisplaySlot,
        objective: Option<&str>,
    ) -> Vec<ScoreboardPacket> {
        let old = self.display_slots.get(slot.serialized_name()).cloned();
        let new_was_tracked = objective.is_some_and(|name| self.is_displayed(name));
        match objective {
            Some(name) => {
                self.display_slots
                    .insert(slot.serialized_name().to_owned(), name.to_owned());
            }
            None => {
                self.display_slots.remove(slot.serialized_name());
            }
        }

        let mut packets = Vec::new();
        if let Some(old) = old.as_deref()
            && Some(old) != objective
        {
            if self.display_slot_count(old) > 0 {
                packets.push(Self::display_packet(slot, objective));
            } else {
                packets.push(Self::removal_packet(old));
            }
        }
        if let Some(name) = objective {
            if new_was_tracked {
                packets.push(Self::display_packet(slot, Some(name)));
            } else {
                packets.extend(self.start_tracking_packets(name));
            }
        }
        packets
    }
}

impl Scoreboard {
    /// Adds a writable `dummy` objective whose display name is its name.
    ///
    /// # Errors
    ///
    /// Returns an error if the objective name is empty or already exists.
    pub fn add_objective(
        &self,
        name: impl Into<String>,
    ) -> Result<ScoreboardObjective, ScoreboardError> {
        self.insert_objective(name.into(), ObjectiveState::default())
    }

    /// Adds a `dummy` objective with explicit command mutability.
    ///
    /// # Errors
    ///
    /// Returns an error if the objective name is empty or already exists.
    pub fn add_objective_with_read_only(
        &self,
        name: impl Into<String>,
        read_only: bool,
    ) -> Result<ScoreboardObjective, ScoreboardError> {
        self.insert_objective(
            name.into(),
            ObjectiveState {
                read_only,
                ..ObjectiveState::default()
            },
        )
    }

    /// Adds an objective driven by `criteria`.
    ///
    /// Vanilla parity: `ScoreboardCommand.addObjective`, which gives the
    /// objective the criteria's default render type, no automatic display
    /// updates and no number format.
    ///
    /// # Errors
    ///
    /// Returns an error if the objective name is empty or already exists.
    pub fn create_objective(
        &self,
        name: impl Into<String>,
        criteria: &ObjectiveCriteria,
        display_name: TextComponent,
    ) -> Result<ScoreboardObjective, ScoreboardError> {
        self.insert_objective(
            name.into(),
            ObjectiveState {
                read_only: criteria.is_read_only(),
                criteria: criteria.name().to_owned(),
                display_name: Some(display_name),
                render_type: criteria.default_render_type(),
                ..ObjectiveState::default()
            },
        )
    }

    fn insert_objective(
        &self,
        name: String,
        objective: ObjectiveState,
    ) -> Result<ScoreboardObjective, ScoreboardError> {
        ensure_objective_name(&name)?;
        let mut state = self.state.write();
        if state.objectives.contains_key(&name) {
            return Err(ScoreboardError::DuplicateObjective(name));
        }
        let read_only = objective.read_only;
        state.objectives.insert(name.clone(), objective);
        drop(state);
        self.mark_dirty();
        Ok(ScoreboardObjective { name, read_only })
    }

    /// Returns an objective by name.
    #[must_use]
    pub fn objective(&self, name: &str) -> Option<ScoreboardObjective> {
        self.state
            .read()
            .objectives
            .get(name)
            .map(|objective| ScoreboardObjective {
                name: name.to_owned(),
                read_only: objective.read_only,
            })
    }

    /// Returns everything about an objective.
    #[must_use]
    pub fn objective_data(&self, name: &str) -> Option<ObjectiveData> {
        let state = self.state.read();
        Some(objective_data(name, state.objectives.get(name)?))
    }

    /// Returns every objective in name order.
    ///
    /// Vanilla parity: `Scoreboard.getObjectives`.
    #[must_use]
    pub fn objectives(&self) -> Vec<ObjectiveData> {
        self.state
            .read()
            .objectives
            .iter()
            .map(|(name, objective)| objective_data(name, objective))
            .collect()
    }

    /// Returns objective names in stable order.
    #[must_use]
    pub fn objective_names(&self) -> Vec<String> {
        self.state.read().objectives.keys().cloned().collect()
    }

    /// Removes an objective, its scores and its display slots.
    ///
    /// Vanilla parity: `Scoreboard.removeObjective`. Holders stay tracked
    /// even when this leaves them without a score.
    pub fn remove_objective(&self, objective: &ScoreboardObjective) -> bool {
        let mut state = self.state.write();
        if state.objectives.remove(objective.name()).is_none() {
            return false;
        }
        let mut packets = Vec::new();
        let slots: Vec<DisplaySlot> = state.slots_showing(objective.name()).collect();
        for slot in slots {
            packets.extend(state.change_display(slot, None));
        }
        for scores in state.scores.values_mut() {
            scores.remove(objective.name());
        }
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packets);
        true
    }

    fn change_objective(
        &self,
        objective: &ScoreboardObjective,
        change: impl FnOnce(&mut ObjectiveState),
    ) -> bool {
        let mut state = self.state.write();
        let Some(stored) = state.objectives.get_mut(objective.name()) else {
            return false;
        };
        change(stored);
        let packet = state
            .is_displayed(objective.name())
            .then(|| state.objective_packet(objective.name(), false))
            .flatten();
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packet);
        true
    }

    /// Vanilla parity: `Objective.setDisplayName`.
    pub fn set_objective_display_name(
        &self,
        objective: &ScoreboardObjective,
        display_name: TextComponent,
    ) -> bool {
        self.change_objective(objective, |stored| {
            stored.display_name = Some(display_name);
        })
    }

    /// Vanilla parity: `Objective.setRenderType`.
    pub fn set_objective_render_type(
        &self,
        objective: &ScoreboardObjective,
        render_type: ObjectiveRenderType,
    ) -> bool {
        self.change_objective(objective, |stored| stored.render_type = render_type)
    }

    /// Vanilla parity: `Objective.setDisplayAutoUpdate`.
    pub fn set_objective_display_auto_update(
        &self,
        objective: &ScoreboardObjective,
        display_auto_update: bool,
    ) -> bool {
        self.change_objective(objective, |stored| {
            stored.display_auto_update = display_auto_update;
        })
    }

    /// Vanilla parity: `Objective.setNumberFormat`.
    pub fn set_objective_number_format(
        &self,
        objective: &ScoreboardObjective,
        number_format: Option<NumberFormat>,
    ) -> bool {
        self.change_objective(objective, |stored| stored.number_format = number_format)
    }

    /// Returns the name of the objective a slot shows.
    ///
    /// Vanilla parity: `Scoreboard.getDisplayObjective`.
    #[must_use]
    pub fn display_objective(&self, slot: DisplaySlot) -> Option<String> {
        self.state
            .read()
            .display_slots
            .get(slot.serialized_name())
            .cloned()
    }

    /// Shows an objective in a slot, or empties the slot.
    ///
    /// Vanilla parity: `ServerScoreboard.setDisplayObjective`.
    pub fn set_display_objective(
        &self,
        slot: DisplaySlot,
        objective: Option<&ScoreboardObjective>,
    ) {
        let mut state = self.state.write();
        if objective.is_some_and(|objective| !state.objectives.contains_key(objective.name())) {
            return;
        }
        let packets = state.change_display(slot, objective.map(ScoreboardObjective::name));
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packets);
    }

    /// Returns the score entry for a holder and objective.
    #[must_use]
    pub fn score_entry(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
    ) -> Option<ScoreboardScore> {
        self.state
            .read()
            .scores
            .get(holder.name())
            .and_then(|scores| scores.get(objective.name()).cloned())
    }

    /// Returns the integer score for a holder and objective.
    #[must_use]
    pub fn score(&self, holder: &ScoreHolder, objective: &ScoreboardObjective) -> Option<i32> {
        self.score_entry(holder, objective)
            .map(|score| score.value())
    }

    /// Returns a holder's scores in objective-name order.
    ///
    /// Vanilla parity: `Scoreboard.listPlayerScores(ScoreHolder)`.
    #[must_use]
    pub fn holder_scores(&self, holder: &ScoreHolder) -> Vec<(String, i32)> {
        self.state
            .read()
            .scores
            .get(holder.name())
            .map_or_else(Vec::new, |scores| {
                scores
                    .iter()
                    .map(|(objective, score)| (objective.clone(), score.value))
                    .collect()
            })
    }

    /// Sets a holder's score, creating it if absent and preserving its lock.
    ///
    /// Vanilla parity: `ScoreAccess.set` of `Scoreboard.getOrCreatePlayerScore`:
    /// an objective with `displayautoupdate` copies the holder's display name
    /// into the score, and a new or changed score is sent to clients.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty holder, a missing objective, or a read-only objective.
    pub fn set_score(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
        value: i32,
    ) -> Result<(), ScoreboardError> {
        ensure_holder_name(holder.name())?;
        let mut state = self.state.write();
        ensure_writable_objective(&state, objective)?;
        let auto_update = ensure_objective_exists(&state, objective)?.display_auto_update;
        let scores = state.scores.entry(holder.name().to_owned()).or_default();
        let (score, mut changed) = match scores.entry(objective.name().to_owned()) {
            Entry::Vacant(entry) => (entry.insert(ScoreboardScore::new(value)), true),
            Entry::Occupied(entry) => (entry.into_mut(), false),
        };
        if auto_update
            && let Some(name) = holder.display_name()
            && score.display.as_ref() != Some(name)
        {
            score.display = Some(name.clone());
            changed = true;
        }
        if score.value != value {
            score.value = value;
            changed = true;
        }
        if !changed {
            return Ok(());
        }
        let score = score.clone();
        let packet = state.tracked_score_packet(holder.name(), objective.name(), &score);
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packet);
        Ok(())
    }

    /// Returns a score, creating a default one first if it is absent.
    ///
    /// Vanilla parity: `ScoreAccess.get` of `Scoreboard.getOrCreatePlayerScore`.
    /// Creating alone sends nothing; `/scoreboard players operation` relies on
    /// this to read a source it has never written.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty holder or a missing objective.
    pub fn score_or_create(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
    ) -> Result<i32, ScoreboardError> {
        ensure_holder_name(holder.name())?;
        let mut state = self.state.write();
        ensure_objective_exists(&state, objective)?;
        let scores = state.scores.entry(holder.name().to_owned()).or_default();
        let created = !scores.contains_key(objective.name());
        let value = scores.entry(objective.name().to_owned()).or_default().value;
        drop(state);
        if created {
            self.mark_dirty();
        }
        Ok(value)
    }

    /// Changes a score's trigger lock state.
    ///
    /// Vanilla parity: `ScoreAccess.lock` and `unlock`. A score created by the
    /// call is sent to clients that track the objective.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty holder or a missing objective.
    pub fn set_score_locked(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
        locked: bool,
    ) -> Result<(), ScoreboardError> {
        ensure_holder_name(holder.name())?;
        let mut state = self.state.write();
        ensure_objective_exists(&state, objective)?;
        let scores = state.scores.entry(holder.name().to_owned()).or_default();
        let packet = match scores.entry(objective.name().to_owned()) {
            Entry::Vacant(entry) => {
                let score = entry.insert(ScoreboardScore {
                    locked,
                    ..ScoreboardScore::default()
                });
                let score = score.clone();
                state.tracked_score_packet(holder.name(), objective.name(), &score)
            }
            Entry::Occupied(mut entry) => {
                if entry.get().locked == locked {
                    return Ok(());
                }
                entry.get_mut().locked = locked;
                None
            }
        };
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packet);
        Ok(())
    }

    /// Replaces the text drawn in place of a holder's name on one objective.
    ///
    /// Vanilla parity: `ScoreAccess.display`.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty holder or a missing objective.
    pub fn set_score_display(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
        display: Option<TextComponent>,
    ) -> Result<(), ScoreboardError> {
        self.change_score(holder, objective, |score| score.display = display)
    }

    /// Replaces a score's own number format.
    ///
    /// Vanilla parity: `ScoreAccess.numberFormatOverride`.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty holder or a missing objective.
    pub fn set_score_number_format(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
        number_format: Option<NumberFormat>,
    ) -> Result<(), ScoreboardError> {
        self.change_score(holder, objective, |score| {
            score.number_format = number_format;
        })
    }

    fn change_score(
        &self,
        holder: &ScoreHolder,
        objective: &ScoreboardObjective,
        change: impl FnOnce(&mut ScoreboardScore),
    ) -> Result<(), ScoreboardError> {
        ensure_holder_name(holder.name())?;
        let mut state = self.state.write();
        ensure_objective_exists(&state, objective)?;
        let scores = state.scores.entry(holder.name().to_owned()).or_default();
        let existing = scores.get(objective.name()).cloned();
        let score = scores.entry(objective.name().to_owned()).or_default();
        change(score);
        let score = score.clone();
        // Vanilla parity: `ScoreAccess.display` sends only a real change, or a
        // score that clients have not seen yet.
        let packet = (existing.as_ref() != Some(&score))
            .then(|| state.tracked_score_packet(holder.name(), objective.name(), &score))
            .flatten();
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packet);
        Ok(())
    }

    /// Removes one of a holder's scores.
    ///
    /// Vanilla parity: `Scoreboard.resetSinglePlayerScore`. A holder left with
    /// no score is forgotten entirely, which tells every client.
    pub fn reset_score(&self, holder: &ScoreHolder, objective: &ScoreboardObjective) {
        let mut state = self.state.write();
        let Some(scores) = state.scores.get_mut(holder.name()) else {
            return;
        };
        let removed = scores.remove(objective.name()).is_some();
        let packet = if scores.is_empty() {
            state.scores.remove(holder.name());
            Some(ScoreboardPacket::Reset(CResetScore {
                owner: holder.name().to_owned(),
                objective_name: None,
            }))
        } else if removed && state.is_displayed(objective.name()) {
            Some(ScoreboardPacket::Reset(CResetScore {
                owner: holder.name().to_owned(),
                objective_name: Some(objective.name().to_owned()),
            }))
        } else {
            None
        };
        drop(state);
        self.mark_dirty();
        self.queue_objective_packets(packet);
    }

    /// Removes every score of a holder.
    ///
    /// Vanilla parity: `Scoreboard.resetAllPlayerScores`.
    pub fn reset_holder(&self, holder: &ScoreHolder) {
        if self.state.write().scores.remove(holder.name()).is_none() {
            return;
        }
        self.mark_dirty();
        self.queue_objective_packets([ScoreboardPacket::Reset(CResetScore {
            owner: holder.name().to_owned(),
            objective_name: None,
        })]);
    }

    /// Takes the objective packets queued since the last call, oldest first.
    #[must_use]
    pub fn take_objective_updates(&self) -> Vec<ScoreboardPacket> {
        mem::take(&mut *self.objective_updates.lock())
    }

    /// What a client entering this scoreboard's domain is sent: for each
    /// objective some slot shows, the packets that start tracking it.
    ///
    /// Vanilla parity: the objective half of `PlayerList.updateEntireScoreboard`.
    #[must_use]
    pub fn objective_snapshot_packets(&self) -> Vec<ScoreboardPacket> {
        let state = self.state.read();
        let mut sent = Vec::new();
        let mut packets = Vec::new();
        for slot in DisplaySlot::VALUES {
            let Some(objective) = state.display_slots.get(slot.serialized_name()) else {
                continue;
            };
            if sent.contains(&objective) {
                continue;
            }
            sent.push(objective);
            packets.extend(state.start_tracking_packets(objective));
        }
        packets
    }

    /// What takes this scoreboard's objectives off a client leaving the domain.
    #[must_use]
    pub fn objective_removals(&self) -> Vec<ScoreboardPacket> {
        let state = self.state.read();
        let mut sent = Vec::new();
        for objective in state.display_slots.values() {
            if !sent.contains(&objective) {
                sent.push(objective);
            }
        }
        sent.into_iter()
            .map(|objective| PersistentScoreboard::removal_packet(objective))
            .collect()
    }

    fn queue_objective_packets(&self, packets: impl IntoIterator<Item = ScoreboardPacket>) {
        let mut queue = self.objective_updates.lock();
        queue.extend(packets);
    }
}

#[cfg(test)]
mod tests {
    use foton_protocol::packets::game::{
        CResetScore, CSetDisplayObjective, CSetObjective, DisplaySlot, NumberFormat,
        ObjectiveMethod, ObjectiveRenderType,
    };
    use text_components::TextComponent;

    use super::{ObjectiveCriteria, ScoreboardPacket};
    use crate::scoreboard::{PersistentScoreboard, ScoreHolder, Scoreboard};

    fn kinds(packets: &[ScoreboardPacket]) -> Vec<&'static str> {
        packets
            .iter()
            .map(|packet| match packet {
                ScoreboardPacket::Objective(CSetObjective {
                    method: ObjectiveMethod::Add(_),
                    ..
                }) => "add",
                ScoreboardPacket::Objective(CSetObjective {
                    method: ObjectiveMethod::Remove,
                    ..
                }) => "remove",
                ScoreboardPacket::Objective(_) => "change",
                ScoreboardPacket::Score(_) => "score",
                ScoreboardPacket::Reset(_) => "reset",
                ScoreboardPacket::Display(_) => "display",
            })
            .collect()
    }

    /// A client only knows an objective a slot shows, so scores written to any
    /// other objective must not be sent, and showing one starts with
    /// everything the client is missing.
    #[test]
    fn clients_hear_of_an_objective_only_while_a_slot_shows_it() {
        let scoreboard = Scoreboard::new();
        let kills = scoreboard.add_objective("kills").expect("objective");
        let steve = ScoreHolder::new("Steve");
        scoreboard.set_score(&steve, &kills, 3).expect("score");
        assert!(scoreboard.take_objective_updates().is_empty());

        scoreboard.set_display_objective(DisplaySlot::Sidebar, Some(&kills));
        let start = scoreboard.take_objective_updates();
        assert_eq!(kinds(&start), ["add", "display", "score"]);
        assert_eq!(start, scoreboard.objective_snapshot_packets());

        scoreboard.set_score(&steve, &kills, 4).expect("score");
        scoreboard.set_score(&steve, &kills, 4).expect("same score");
        assert_eq!(kinds(&scoreboard.take_objective_updates()), ["score"]);

        // A second slot showing it needs only the slot, not the objective again.
        scoreboard.set_display_objective(DisplaySlot::List, Some(&kills));
        assert_eq!(kinds(&scoreboard.take_objective_updates()), ["display"]);
        scoreboard.set_display_objective(DisplaySlot::Sidebar, None);
        let cleared = scoreboard.take_objective_updates();
        assert_eq!(kinds(&cleared), ["display"]);
        assert_eq!(
            cleared,
            [ScoreboardPacket::Display(CSetDisplayObjective {
                slot: DisplaySlot::Sidebar,
                objective_name: String::new(),
            })]
        );

        // The last slot letting go removes the objective from clients.
        scoreboard.set_display_objective(DisplaySlot::List, None);
        assert_eq!(kinds(&scoreboard.take_objective_updates()), ["remove"]);
        scoreboard.set_score(&steve, &kills, 5).expect("score");
        assert!(scoreboard.take_objective_updates().is_empty());
    }

    #[test]
    fn changing_or_removing_a_shown_objective_is_sent_but_an_unshown_one_is_not() {
        let scoreboard = Scoreboard::new();
        let shown = scoreboard.add_objective("shown").expect("objective");
        let hidden = scoreboard.add_objective("hidden").expect("objective");
        scoreboard.set_display_objective(DisplaySlot::BelowName, Some(&shown));
        let _ = scoreboard.take_objective_updates();

        assert!(scoreboard.set_objective_render_type(&hidden, ObjectiveRenderType::Hearts));
        assert!(scoreboard.take_objective_updates().is_empty());
        assert!(scoreboard.set_objective_render_type(&shown, ObjectiveRenderType::Hearts));
        assert_eq!(kinds(&scoreboard.take_objective_updates()), ["change"]);

        assert!(scoreboard.remove_objective(&hidden));
        assert!(scoreboard.take_objective_updates().is_empty());
        assert!(scoreboard.remove_objective(&shown));
        assert_eq!(kinds(&scoreboard.take_objective_updates()), ["remove"]);
        assert_eq!(scoreboard.display_objective(DisplaySlot::BelowName), None);
    }

    /// Vanilla forgets a holder once its last score goes, and tells everyone;
    /// a score that only one objective loses is announced for that objective.
    #[test]
    fn resetting_scores_tells_clients_what_was_forgotten() {
        let scoreboard = Scoreboard::new();
        let first = scoreboard.add_objective("first").expect("objective");
        let second = scoreboard.add_objective("second").expect("objective");
        scoreboard.set_display_objective(DisplaySlot::Sidebar, Some(&first));
        let steve = ScoreHolder::new("Steve");
        scoreboard.set_score(&steve, &first, 1).expect("score");
        scoreboard.set_score(&steve, &second, 2).expect("score");
        let _ = scoreboard.take_objective_updates();

        scoreboard.reset_score(&steve, &first);
        assert_eq!(
            scoreboard.take_objective_updates(),
            [ScoreboardPacket::Reset(CResetScore {
                owner: "Steve".to_owned(),
                objective_name: Some("first".to_owned()),
            })]
        );
        scoreboard.reset_score(&steve, &second);
        assert_eq!(
            scoreboard.take_objective_updates(),
            [ScoreboardPacket::Reset(CResetScore {
                owner: "Steve".to_owned(),
                objective_name: None,
            })]
        );
        assert!(scoreboard.tracked_holders().is_empty());
    }

    /// `displayautoupdate` copies an entity's display name into the score the
    /// first time it is written, and again when the name changes.
    #[test]
    fn display_auto_update_copies_the_holders_display_name() {
        let scoreboard = Scoreboard::new();
        let kills = scoreboard.add_objective("kills").expect("objective");
        let plain = ScoreHolder::new("Steve").with_display_name(TextComponent::plain("Steve"));
        scoreboard.set_score(&plain, &kills, 1).expect("score");
        assert_eq!(
            scoreboard
                .score_entry(&plain, &kills)
                .and_then(|score| score.display().cloned()),
            None
        );

        scoreboard.set_objective_display_auto_update(&kills, true);
        let named = ScoreHolder::new("Alex").with_display_name(TextComponent::plain("[A] Alex"));
        scoreboard.set_score(&named, &kills, 2).expect("score");
        assert_eq!(
            scoreboard
                .score_entry(&named, &kills)
                .and_then(|score| score.display().cloned()),
            Some(TextComponent::plain("[A] Alex"))
        );
    }

    /// Everything `/scoreboard` can set must come back after a restart, and a
    /// save from before any of it existed must still load.
    #[test]
    fn objective_state_survives_a_save_and_old_saves_still_load() {
        let scoreboard = Scoreboard::new();
        let criteria = ObjectiveCriteria::by_name("trigger").expect("criteria");
        let objective = scoreboard
            .create_objective("t", &criteria, TextComponent::plain("Trig"))
            .expect("objective");
        scoreboard.set_objective_render_type(&objective, ObjectiveRenderType::Hearts);
        scoreboard.set_objective_number_format(&objective, Some(NumberFormat::Blank));
        scoreboard.set_display_objective(DisplaySlot::TeamRed, Some(&objective));
        let steve = ScoreHolder::new("Steve");
        scoreboard.set_score(&steve, &objective, 9).expect("score");
        scoreboard
            .set_score_display(&steve, &objective, Some(TextComponent::plain("S")))
            .expect("display");
        scoreboard
            .set_score_number_format(
                &steve,
                &objective,
                Some(NumberFormat::Fixed(TextComponent::plain("nine"))),
            )
            .expect("format");

        let json = serde_json::to_string(&*scoreboard.state.read()).expect("save");
        let persistent: PersistentScoreboard = serde_json::from_str(&json).expect("load");
        let restored = Scoreboard::from_persistent(persistent).expect("valid");

        let data = restored.objective_data("t").expect("objective");
        assert!(data.criteria.is_trigger());
        assert_eq!(data.render_type, ObjectiveRenderType::Hearts);
        assert_eq!(data.number_format, Some(NumberFormat::Blank));
        assert_eq!(data.display_name, TextComponent::plain("Trig"));
        assert_eq!(
            restored.display_objective(DisplaySlot::TeamRed).as_deref(),
            Some("t")
        );
        let objective = restored.objective("t").expect("handle");
        let score = restored.score_entry(&steve, &objective).expect("score");
        assert_eq!(score.value(), 9);
        assert_eq!(score.display(), Some(&TextComponent::plain("S")));
        assert_eq!(
            score.number_format(),
            Some(&NumberFormat::Fixed(TextComponent::plain("nine")))
        );
        // The client of a restarted server is sent the shown objective at once.
        assert_eq!(
            kinds(&restored.objective_snapshot_packets()),
            ["add", "display", "score"]
        );

        let old = r#"{"objectives":{"kills":{"read_only":false}},"scores":{"Steve":{"kills":{"value":5,"locked":true}}},"teams":[],"holder_teams":{}}"#;
        let persistent: PersistentScoreboard = serde_json::from_str(old).expect("old shape");
        let restored = Scoreboard::from_persistent(persistent).expect("old save is valid");
        let kills = restored.objective("kills").expect("objective");
        assert_eq!(restored.score(&steve, &kills), Some(5));
        assert_eq!(
            restored
                .objective_data("kills")
                .map(|data| data.render_type),
            Some(ObjectiveRenderType::Integer)
        );
    }

    /// A slot saved for an objective that no longer exists is dropped rather
    /// than failing the whole scoreboard's load, as vanilla does.
    #[test]
    fn a_saved_slot_without_its_objective_is_dropped() {
        let saved =
            r#"{"objectives":{},"display_slots":{"sidebar":"gone","nonsense":"also_gone"}}"#;
        let persistent: PersistentScoreboard = serde_json::from_str(saved).expect("load");
        let restored = Scoreboard::from_persistent(persistent).expect("valid");
        assert_eq!(restored.display_objective(DisplaySlot::Sidebar), None);
    }
}
