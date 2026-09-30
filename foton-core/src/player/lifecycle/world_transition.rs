use super::*;
use crate::advancement::triggers;
use crate::event::{
    Event as _, PlayerChangedWorldEvent, PlayerPortalEvent, PlayerTeleportEvent, TeleportCause,
    TeleportPoint,
};
use crate::portal::TeleportTransitionCause::{
    Command, EndGateway, EndPortal, EnderPearl, NetherPortal, Unknown,
};
use crate::portal::{TeleportTransitionCause, WorldChangeRequest};

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

    /// Asks plugins about a teleport that is not a portal, and rewrites the
    /// transition to wherever they sent the player. False when refused.
    ///
    /// Paper parity: `ServerPlayer.teleport(TeleportTransition)` raises
    /// `PlayerTeleportEvent` for every cause but portals, which raise
    /// `PlayerPortalEvent` above, and respawns, which raise none. A refused
    /// pearl is spent without hurting its thrower; a refused `/tp` does not
    /// count the player as moved.
    fn ask_teleport(
        &self,
        transition: &mut TeleportTransition,
        current_position: DVec3,
        current_rotation: (f32, f32),
    ) -> bool {
        let cause = match transition.cause {
            EnderPearl => TeleportCause::EnderPearl,
            Command => TeleportCause::Command,
            Unknown => TeleportCause::Unknown,
            _ => return true,
        };
        let destination = TeleportPoint {
            world: transition.target_world.key.to_string(),
            position: transition.resolved_position(current_position),
            rotation: transition.resolved_rotation(current_rotation),
        };
        let origin = TeleportPoint {
            world: self.get_world().key.to_string(),
            position: current_position,
            rotation: current_rotation,
        };
        let mut event = PlayerTeleportEvent::new(self.uuid(), origin, destination.clone(), cause);
        self.fire_event(&mut event);
        if event.is_cancelled() {
            return false;
        }
        let to = event.to();
        if *to == destination {
            return true;
        }
        let Some(target_world) = to
            .world
            .parse()
            .ok()
            .and_then(|key| self.server().worlds.get(&key))
        else {
            return false;
        };
        // What a listener hands back is a place, not an offset: the position
        // and rotation stop being relative, the velocity keeps its meaning.
        let absolute = RelativeMovement::X
            | RelativeMovement::Y
            | RelativeMovement::Z
            | RelativeMovement::Y_ROT
            | RelativeMovement::X_ROT;
        transition.target_world = target_world;
        transition.position = to.position;
        transition.rotation = to.rotation;
        transition.relatives = RelativeMovement::new(transition.relatives.0 & !absolute);
        true
    }

    /// Sends the player to `to`, whose `PlayerTeleportEvent` has already been
    /// asked. False when the destination is not a loaded world of this domain
    /// or the move is refused.
    ///
    /// Within the world the move is made now. Into another world it is made
    /// at the tick's world-change step, with the destination chunk held
    /// loaded meanwhile as `/tp` holds it: moving a player between worlds
    /// from inside a listener would re-enter the world being ticked.
    pub fn teleport_announced(self: &Arc<Self>, to: &TeleportPoint) -> bool {
        let Some(world) = to
            .world
            .parse()
            .ok()
            .and_then(|key| self.server().worlds.get(&key))
        else {
            return false;
        };
        let current = self.get_world();
        if Arc::ptr_eq(&world, &current) {
            return self
                .teleport(to.position, to.rotation.0, to.rotation.1)
                .is_ok();
        }
        if world.domain() != current.domain() {
            return false;
        }
        world
            .chunk_map
            .place_teleport_ticket(ChunkPos::from_block_pos(BlockPos::from(to.position)));
        let transition = TeleportTransition {
            target_world: world,
            cause: TeleportTransitionCause::Plugin,
            position: to.position,
            rotation: to.rotation,
            velocity: DVec3::ZERO,
            relatives: RelativeMovement::NONE,
            portal_cooldown: self.portal_cooldown(),
            as_passenger: false,
            post_transition: TeleportPostTransition::do_nothing(),
        };
        let entity: SharedEntity = Arc::<Self>::clone(self);
        self.server()
            .queue_world_change(entity, WorldChangeRequest::Computed(transition));
        true
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
        if !self.ask_teleport(&mut teleport_transition, current_position, current_rotation) {
            return false;
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

            // After, not before: Bukkit's `PlayerChangedWorldEvent` reports a
            // move that has happened, which is why it carries the world left
            // behind and cannot be refused. The refusable hook is the teleport.
            let mut changed =
                PlayerChangedWorldEvent::new(Arc::clone(self), Arc::clone(&current_world));
            self.fire_event(&mut changed);
        }
        self.apply_post_teleport_transition(&teleport_transition.post_transition);
        true
    }
}
