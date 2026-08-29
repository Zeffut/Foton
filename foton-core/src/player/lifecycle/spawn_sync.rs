use foton_protocol::packets::game::{
    CEntityEvent, CPlayerInfoUpdate, CSetEntityData, CUpdateAttributes,
};
use foton_registry::vanilla_game_rules::{IMMEDIATE_RESPAWN, LIMITED_CRAFTING, REDUCED_DEBUG_INFO};
use foton_utils::entity_events::EntityStatus;

use super::{
    Arc, BlockBreakingManager, CContainerClose, CGameEvent, CRespawn, CSetDefaultSpawnPosition,
    CSetHeldSlot, CSetPassengers, DVec3, Entity, GameEventType, GameType, MenuRemovalStatus,
    MobEffectSyncChange, MobEffectSyncPacket, Player, RegistryEntry, RelativeMovement, ResetReason,
    World,
};

impl Player {
    /// Resets the player's transient state and prepares them for a new world.
    ///
    /// This is the shared "clean slate" path used by initial join, respawn, and
    /// world change. If the player is currently in a different world, they are
    /// removed from the old world first.
    ///
    /// Vanilla creates a fresh `ServerPlayer` for death and End-credits respawns,
    /// but reuses it for dimension changes. Foton reuses the same `Player` for
    /// every path, so this resets only the transient state appropriate to `reason`.
    pub(crate) fn reset(self: &Arc<Self>, new_world: Arc<World>, reason: ResetReason) {
        self.reset_inner_after(new_world, reason, false, || {});
    }

    /// Resets a player already detached from its source domain and restores target-domain state.
    pub(crate) fn reset_after_detached_domain_restore<F>(
        self: &Arc<Self>,
        new_world: Arc<World>,
        restore_state: F,
    ) where
        F: FnOnce(),
    {
        self.reset_inner_after(new_world, ResetReason::WorldChange, true, restore_state);
    }

    fn reset_inner_after<F>(
        self: &Arc<Self>,
        new_world: Arc<World>,
        reason: ResetReason,
        source_world_detached: bool,
        restore_state: F,
    ) where
        F: FnOnce(),
    {
        if reason != ResetReason::InitialJoin {
            assert_eq!(
                self.remove_all_menus(),
                MenuRemovalStatus::Complete,
                "player reset menu removal must run outside a menu callback"
            );
        }
        if matches!(reason, ResetReason::Respawn | ResetReason::EndCredits) {
            // Vanilla creates a fresh ServerPlayer and inventory menu for these paths.
            self.inventory_menu
                .lock()
                .behavior_mut()
                .reset_quick_craft();
        }

        let old_world = self.get_world();
        let switching_worlds = !Arc::ptr_eq(&old_world, &new_world);

        if switching_worlds {
            self.send_packet(CContainerClose { container_id: 0 });
            if !source_world_detached {
                old_world.remove_player_for_world_change(self);
            }
            self.set_world(new_world.clone());
        } else if !source_world_detached {
            old_world.chunk_map.remove_player(self);
        }

        self.set_client_loaded(false);
        self.set_velocity(DVec3::ZERO);
        self.movement.lock().reset_last_known_client_movement();
        self.set_on_ground(false);
        self.reset_entity_state();
        *self.block_breaking.lock() = BlockBreakingManager::new();

        restore_state();

        if reason != ResetReason::InitialJoin {
            // 0x01 = keep attributes, 0x02 = keep entity data
            let data_kept = reason.respawn_data_kept();

            self.send_packet(CRespawn {
                dimension_type: new_world.dimension_type.id() as i32,
                dimension_name: new_world.key.clone(),
                hashed_seed: new_world.obfuscated_seed(),
                gamemode: self.game_mode() as u8,
                previous_gamemode: nullable_game_mode_id(self.previous_game_mode()),
                is_debug: false,
                is_flat: new_world.is_flat,
                has_death_location: false,
                death_dimension_name: None,
                death_location: None,
                portal_cooldown_ticks: self.portal_cooldown(),
                sea_level: new_world.sea_level,
                data_kept,
            });
        }
    }

    /// Spawns the player into their current world at the given position.
    ///
    /// This is the shared "enter world" path used by initial join, respawn, and
    /// world change. Sends position sync, abilities, inventory, time, weather,
    /// and adds the player to the world as appropriate for the given reason.
    ///
    /// # Panics
    /// Panics if the `advance_time` gamerule is not a bool.
    #[must_use]
    pub(crate) fn spawn(
        self: &Arc<Self>,
        position: DVec3,
        rotation: (f32, f32),
        reason: ResetReason,
    ) -> bool {
        self.spawn_with_velocity(position, rotation, DVec3::ZERO, reason)
    }

    #[must_use]
    pub(crate) fn spawn_with_velocity(
        self: &Arc<Self>,
        position: DVec3,
        rotation: (f32, f32),
        velocity: DVec3,
        reason: ResetReason,
    ) -> bool {
        self.spawn_with_velocity_packet(
            position,
            rotation,
            velocity,
            reason,
            position,
            rotation,
            velocity,
            RelativeMovement::NONE,
        )
    }

    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "packet-relative teleports must keep resolved and protocol values separate"
    )]
    pub(crate) fn spawn_with_velocity_packet(
        self: &Arc<Self>,
        position: DVec3,
        rotation: (f32, f32),
        velocity: DVec3,
        reason: ResetReason,
        packet_position: DVec3,
        packet_rotation: (f32, f32),
        packet_velocity: DVec3,
        relatives: RelativeMovement,
    ) -> bool {
        let world = self.get_world();

        // Set position and rotation
        self.base.set_position_local(position);
        self.set_rotation(rotation);
        self.set_old_position_to_current();
        self.movement.lock().reset_for_position_sync(position);

        // Teleport sync (sends CPlayerPosition, sets awaiting_teleport for ack)
        if let Err(error) = self.teleport_with_velocity_packet(
            position,
            velocity,
            rotation,
            packet_position,
            packet_velocity,
            packet_rotation,
            relatives,
        ) {
            panic!(
                "failed to synchronize player {} spawn position: {error}",
                self.id()
            );
        }
        self.reset_flying_ticks();

        self.send_spawn_state_packets(&world);

        // Force health/xp resync on next tick
        self.reset_sent_info();

        // Resend client context that is not fully covered by CLogin/CRespawn.
        self.server().resend_player_context(self);
        self.send_active_effects_for_self();

        // Add to world / re-enter chunk tracking
        match reason {
            ResetReason::InitialJoin | ResetReason::WorldChange => {
                if reason == ResetReason::WorldChange {
                    log::info!(
                        "Player {} changed world to {}",
                        self.gameprofile.name,
                        world.key
                    );
                }
                world.add_player(self.clone(), reason)
            }
            ResetReason::Respawn | ResetReason::EndCredits => {
                if world.players.get_by_entity_id(self.id()).is_none() {
                    return world.add_respawned_player(self.clone());
                }

                // Same world — re-enter chunk tracking
                world.chunk_map.remove_player(self);
                world.player_area_map.remove_by_entity_id(self.id());
                world.entity_tracker().on_player_leave(self);

                self.send_packet(CGameEvent {
                    event: GameEventType::LevelChunksLoadStart,
                    data: 0.0,
                });
                world.register_respawned_player_entity(self);
                true
            }
        }
    }

    fn send_spawn_state_packets(&self, world: &World) {
        self.send_abilities();
        self.publish_client_options();
        self.resend_attributes();
        self.resend_entity_data();
        self.rebroadcast_game_mode();
        self.send_client_enforced_game_rules(world);
        self.send_packet(CSetHeldSlot {
            slot: i32::from(self.inventory.lock().get_selected_slot()),
        });
        self.send_time_sync(world);
        self.send_packet(world.initialize_border_packet());
        self.send_default_spawn_position(world);
        self.send_weather_sync(world);
    }

    /// Hands the player their whole attribute map again.
    ///
    /// Vanilla has no equivalent call because it does not need one: a respawn
    /// builds a fresh `ServerPlayer`, and `restoreFrom`'s `assignBaseValues`
    /// dirties every attribute on that new map, so `ServerEntity` sends the lot
    /// on the next tick. Foton reuses the player and its attribute map, while
    /// the respawn packet still tells the client to throw its copy away -- so
    /// any attribute whose base value differs from the client's default and
    /// which happens not to change again is lost on the client for good. Sent
    /// on every spawn path rather than only on respawn, because a world change
    /// keeps its attributes and a resend there costs one packet.
    fn resend_attributes(&self) {
        let snapshots = self.living_base.attributes().lock().syncable_snapshots();
        if snapshots.is_empty() {
            return;
        }
        self.send_packet(CUpdateAttributes::new(self.id(), snapshots));
    }

    /// Hands the player their whole synchronized entity data again.
    ///
    /// The same story as the attributes above, and from the same bit: a
    /// `CRespawn` whose `data_kept` lacks 0x02 tells the client to throw its
    /// copy of this player's metadata away, which is what vanilla wants
    /// because its replacement `ServerPlayer` carries a fresh
    /// `SynchedEntityData` whose every non-default entry is dirty. Foton reuses
    /// the entry map, and an entry only travels when its value *changes*, so a
    /// field that already holds the right number goes quiet forever: skin
    /// parts, main hand, score, a parrot still on a shoulder. `pack_all` is
    /// exactly the "every non-default value" set the vanilla replacement would
    /// have sent. An initial join needs it too -- no `CRespawn` there, but no
    /// tracker either, because a player never tracks themselves.
    fn resend_entity_data(&self) {
        let values = self.pack_all_entity_data();
        if values.is_empty() {
            return;
        }
        self.send_packet(CSetEntityData::new(self.id(), values));
    }

    /// Tells the arriving player which rules their new world makes them keep.
    ///
    /// Vanilla parity: the `reducedDebugInfo`, `showDeathScreen` and
    /// `doLimitedCrafting` fields of `ClientboundLoginPacket`. Vanilla can
    /// afford to send those once and never again because its game rules belong
    /// to the server. Foton's belong to a world, `CRespawn` carries none of the
    /// three, and the client enforces all three by itself -- so a player who
    /// walked from a world that allows free crafting into one that does not
    /// went on crafting freely until they reconnected.
    fn send_client_enforced_game_rules(&self, world: &World) {
        self.send_packet(CGameEvent {
            event: GameEventType::LimitedCrafting,
            data: game_event_flag(world.get_game_rule(&LIMITED_CRAFTING)),
        });
        self.send_packet(CGameEvent {
            event: GameEventType::ImmediateRespawn,
            data: game_event_flag(world.get_game_rule(&IMMEDIATE_RESPAWN)),
        });
        self.send_packet(CEntityEvent {
            entity_id: self.id(),
            event: if world.get_game_rule(&REDUCED_DEBUG_INFO) {
                EntityStatus::ReducedDebugInfo
            } else {
                EntityStatus::FullDebugInfo
            },
        });
    }

    /// Tells everyone else what game mode this player is now in.
    ///
    /// Vanilla parity: the `broadcastAll` of
    /// `ServerPlayerGameMode.setGameModeForPlayer`. Foton keeps the game mode
    /// per domain and restores it with a plain setter, so a player who was
    /// creative in one domain and survival in another switched between them
    /// without anyone's tab list hearing about it.
    fn rebroadcast_game_mode(&self) {
        self.server()
            .broadcast_to_online(CPlayerInfoUpdate::update_game_mode(
                self.gameprofile.id,
                self.game_mode() as i32,
            ));
    }

    fn send_time_sync(&self, world: &World) {
        self.send_packet(world.time_sync_packet());
    }

    fn send_default_spawn_position(&self, world: &World) {
        if let Some(server) = self.server.upgrade() {
            match server.respawn_data_for_domain(world.domain()) {
                Ok(respawn_data) => {
                    self.send_packet(CSetDefaultSpawnPosition {
                        global_pos: respawn_data.global_pos,
                        yaw: respawn_data.yaw,
                        pitch: respawn_data.pitch,
                    });
                }
                Err(error) => {
                    log::error!(
                        "Failed to send default spawn position to player {}: {error}",
                        self.gameprofile.name
                    );
                }
            }
        }
    }

    fn send_weather_sync(&self, world: &World) {
        if !world.can_have_weather() || !world.is_raining() {
            return;
        }

        let (rain_level, thunder_level) = {
            let weather = world.weather.lock();
            (weather.rain_level, weather.thunder_level)
        };

        self.send_packet(CGameEvent {
            event: GameEventType::StartRaining,
            data: 0.0,
        });
        self.send_packet(CGameEvent {
            event: GameEventType::RainLevelChange,
            data: rain_level,
        });
        self.send_packet(CGameEvent {
            event: GameEventType::ThunderLevelChange,
            data: thunder_level,
        });
    }

    pub(in crate::player) fn passenger_ids_for_packet(entity: &dyn Entity) -> Vec<i32> {
        entity
            .passengers()
            .iter()
            .map(|passenger| passenger.id())
            .collect()
    }

    pub(crate) fn send_mob_effect_sync_packet(&self, packet: MobEffectSyncPacket) {
        match packet {
            MobEffectSyncPacket::Update(packet) => self.send_packet(packet),
            MobEffectSyncPacket::Remove(packet) => self.send_packet(packet),
        }
    }

    fn send_active_effects_for_self(&self) {
        for effect in self.living_base.active_mob_effects() {
            self.send_mob_effect_sync_packet(
                MobEffectSyncChange::Update {
                    effect,
                    blend_for_self: false,
                }
                .packet(self.id(), true),
            );
        }
    }

    pub(in crate::player) fn send_active_effects_for_vehicle(&self, vehicle: &dyn Entity) {
        let Some(living_vehicle) = vehicle.as_living_entity() else {
            return;
        };
        for effect in living_vehicle.active_mob_effects() {
            self.send_mob_effect_sync_packet(
                MobEffectSyncChange::Update {
                    effect,
                    blend_for_self: false,
                }
                .packet(vehicle.id(), false),
            );
        }
    }

    pub(crate) fn send_restored_vehicle_mount_sync(&self, vehicle: &dyn Entity) {
        self.send_active_effects_for_vehicle(vehicle);
        self.send_packet(CSetPassengers::new(
            vehicle.id(),
            Self::passenger_ids_for_packet(vehicle),
        ));
    }

    pub(in crate::player) fn remove_active_effects_for_vehicle(&self, vehicle: &dyn Entity) {
        let Some(living_vehicle) = vehicle.as_living_entity() else {
            return;
        };
        for effect in living_vehicle.active_mob_effects() {
            self.send_mob_effect_sync_packet(
                MobEffectSyncChange::Remove {
                    effect: effect.effect(),
                }
                .packet(vehicle.id(), false),
            );
        }
    }
}

pub(in crate::player) fn nullable_game_mode_id(game_mode: Option<GameType>) -> i8 {
    game_mode.map_or(-1, |game_mode| game_mode as i8)
}

/// Vanilla writes a boolean game event's value as 1.0 or 0.0.
const fn game_event_flag(enabled: bool) -> f32 {
    if enabled { 1.0 } else { 0.0 }
}
