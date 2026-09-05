use super::*;
use crate::advancement::triggers;
use crate::event::PlayerPortalEvent;
use crate::portal::TeleportTransitionCause::{EndGateway, EndPortal, NetherPortal};

impl Player {
    fn apply_post_teleport_transition(&self, post_transition: &TeleportPostTransition) {
        for action in post_transition.actions() {
            match *action {
                TeleportPostAction::PlayPortalSound => {
                    self.send_packet(CLevelEvent::new(
                        level_events::SOUND_PORTAL_TRAVEL,
                        BlockPos::ZERO,
                        0,
                        false,
                    ));
                }
                TeleportPostAction::PlacePortalTicket(target) => {
                    let ticket_position = match target {
                        PortalTicketTarget::Destination => BlockPos::from(self.position()),
                        PortalTicketTarget::Block(pos) => pos,
                    };
                    self.get_world().place_portal_ticket(ticket_position);
                }
            }
        }
    }

    /// Applies an ordinary player transition that has already passed server world-change checks.
    /// Cross-domain player state is restored only by the domain-switch workflow.
    pub(crate) fn change_world_within_domain(
        self: &Arc<Self>,
        teleport_transition: &TeleportTransition,
    ) -> bool {
        let current_world = self.get_world();
        let current_position = self.position();
        let current_rotation = self.rotation();
        let current_velocity = self.velocity();
        let mut teleport_transition = teleport_transition.clone();
        if matches!(
            teleport_transition.cause,
            NetherPortal | EndPortal | EndGateway
        ) {
            let mut event = PlayerPortalEvent::new(
                self.uuid(),
                current_world.key.to_string(),
                current_position,
                current_rotation,
                teleport_transition.target_world.key.to_string(),
                teleport_transition.position,
                teleport_transition.rotation,
                teleport_transition.cause,
            );
            self.server().events().fire(&mut event);
            if event.is_cancelled() {
                return false;
            }
            let Some(target_key) = event.to_world().parse().ok() else {
                return false;
            };
            let Some(target_world) = self.server().worlds.get(&target_key) else {
                return false;
            };
            teleport_transition.target_world = target_world;
            teleport_transition.position = event.to_position();
            teleport_transition.rotation = event.to_rotation();
        }
        let new_world = Arc::clone(&teleport_transition.target_world);
        let new_world_key = new_world.key.clone();
        if current_world.domain() != new_world.domain() {
            tracing::error!(
                entity_id = self.id(),
                source_domain = current_world.domain(),
                target_domain = new_world.domain(),
                "Refusing player world change outside the domain-switch workflow"
            );
            return false;
        }
        let position = teleport_transition.resolved_position(current_position);
        let rotation = teleport_transition.resolved_rotation(current_rotation);
        let velocity =
            teleport_transition.resolved_velocity(current_velocity, current_rotation, rotation);
        self.set_portal_cooldown(teleport_transition.portal_cooldown);
        if !teleport_transition.as_passenger {
            self.stop_riding();
        }
        if Arc::ptr_eq(&current_world, &new_world) {
            if let Err(error) = self.teleport_with_velocity_packet(
                position,
                velocity,
                rotation,
                teleport_transition.position,
                teleport_transition.velocity,
                teleport_transition.rotation,
                teleport_transition.relatives,
            ) {
                log::error!(
                    "failed to commit same-world portal teleport for player {}: {error}",
                    self.id()
                );
            }
            self.reset_flying_ticks();
        } else {
            self.reset(new_world, ResetReason::WorldChange);
            if !self.spawn_with_velocity_packet(
                position,
                rotation,
                velocity,
                ResetReason::WorldChange,
                teleport_transition.position,
                teleport_transition.rotation,
                teleport_transition.velocity,
                teleport_transition.relatives,
            ) {
                return false;
            }
            // Vanilla: PlayerList.sendAllPlayerInfo -> inventoryMenu.sendAllDataToRemote
            self.send_inventory_to_remote();
            // Vanilla parity: `ServerPlayer.triggerDimensionChangeTriggers`,
            // which only runs when the level actually changed.
            // Not implemented: the `NETHER_TRAVEL` half. It needs
            // `enteredNetherPosition`, and Foton does not record where a player
            // entered the nether, so the distance it measures has no origin.
            triggers::world::changed_dimension(self, &current_world.key, &new_world_key);
        }
        self.apply_post_teleport_transition(&teleport_transition.post_transition);
        true
    }
}
