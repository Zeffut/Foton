//! Login state packet handlers.

use std::sync::Arc;

use foton_core::player::GameProfile;
use foton_protocol::{
    packets::login::{CHello, CLoginCompression, CLoginFinished, SHello, SKey},
    utils::ConnectionProtocol,
};
use foton_utils::translations;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};
use sha1::Sha1;
use sha2::Digest;
use text_components::TextComponent;
use tokio::task::spawn_blocking;

use crate::{
    AuthError,
    authentication::{acquire_authentication_slot, mojang_authenticate_with_cancel},
    floodgate::resolve_floodgate_login,
    is_valid_player_name, offline_uuid, signed_bytes_be_to_hex,
    tcp_client::{ConnectionAction, ConnectionUpdate, JavaTcpClient},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoginKeyDecryptionError {
    InvalidKey,
    InvalidChallenge,
}

fn decrypt_login_key(
    private_key: RsaPrivateKey,
    packet: SKey,
    expected_challenge: [u8; 4],
) -> Result<[u8; 16], LoginKeyDecryptionError> {
    let mut rng = rand::rng();
    let challenge_response = private_key
        .decrypt_blinded(&mut rng, Pkcs1v15Encrypt, &packet.challenge)
        .map_err(|_| LoginKeyDecryptionError::InvalidKey)?;
    if challenge_response != expected_challenge {
        return Err(LoginKeyDecryptionError::InvalidChallenge);
    }

    let secret_key = private_key
        .decrypt_blinded(&mut rng, Pkcs1v15Encrypt, &packet.key)
        .map_err(|_| LoginKeyDecryptionError::InvalidKey)?;
    secret_key
        .try_into()
        .map_err(|_| LoginKeyDecryptionError::InvalidKey)
}

impl JavaTcpClient {
    /// Handles the hello packet during the login state.
    pub(crate) async fn handle_hello(&self, packet: SHello) -> ConnectionAction {
        // A Floodgate handshake derives its identity entirely from the
        // encrypted hostname payload, never from the client-supplied
        // `packet.name` below -- checked first so a Bedrock gamertag that
        // fails Java's ASCII-only name validation (it can hold characters a
        // Java name cannot) is never rejected before Floodgate gets a
        // chance to authenticate it. This is a sibling of the offline path
        // further down, not a replacement: every failure here is a hard
        // reject that never falls through to it, and a hostname with no
        // Floodgate payload at all falls straight through to the ordinary
        // login below, unaffected.
        let hostname = self.hostname.lock().clone();
        match resolve_floodgate_login(&hostname, self.address, &self.bedrock) {
            Ok(Some(profile)) => {
                let action = self.send_login_finished(&profile).await;
                let sequence_result = self.pre_play_state.lock().complete_login(profile);
                return match sequence_result {
                    Ok(()) => action,
                    Err(error) => self.reject_unexpected_packet(error).await,
                };
            }
            Ok(None) => {}
            Err(error) => {
                log::warn!(
                    "Client {} ({}) sent a Floodgate handshake that was refused: {error}",
                    self.id,
                    self.address
                );
                self.kick("Invalid player data".into()).await;
                return ConnectionAction::none();
            }
        }

        // The hello UUID is client supplied; only authentication or offline derivation is trusted.
        let requested_username = packet.name;
        if !is_valid_player_name(&requested_username) {
            self.kick("Invalid player name".into()).await;
            return ConnectionAction::none();
        }

        if self.server.config.encryption {
            let sequence_result = self.pre_play_state.lock().wait_for_key(requested_username);
            if let Err(error) = sequence_result {
                return self.reject_unexpected_packet(error).await;
            }

            let challenge: [u8; 4] = rand::random();
            self.challenge.store(challenge);

            self.send_bare_packet_now(CHello::new(
                String::new(),
                &self.server.key_store.public_key_der,
                challenge,
                self.server.config.online_mode,
            ))
            .await;
            return ConnectionAction::none();
        }

        let profile = GameProfile {
            id: offline_uuid(&requested_username),
            name: requested_username,
            properties: vec![],
            profile_actions: None,
        };
        let action = self.send_login_finished(&profile).await;
        let sequence_result = self.pre_play_state.lock().complete_login(profile);
        if let Err(error) = sequence_result {
            return self.reject_unexpected_packet(error).await;
        }
        action
    }

    /// Handles the key packet during the login state, used for encryption.
    #[expect(
        clippy::too_many_lines,
        reason = "the authentication permit must visibly cover the complete ordered RSA, cipher activation, session-server and login-state transition"
    )]
    pub(crate) async fn handle_key(&self, packet: SKey) -> ConnectionAction {
        let sequence_result = self.pre_play_state.lock().begin_authentication();
        let requested_username = match sequence_result {
            Ok(requested_username) => requested_username,
            Err(error) => return self.reject_unexpected_packet(error).await,
        };
        let challenge = self.challenge.load();

        let authentication_slot = tokio::select! {
            slot = acquire_authentication_slot() => slot,
            () = self.cancel_token.cancelled() => return ConnectionAction::none(),
        };
        let Some(authentication_slot) = authentication_slot else {
            self.kick("Authentication is temporarily busy".into()).await;
            return ConnectionAction::none();
        };
        let authentication_slot = Arc::new(authentication_slot);

        // Both attacker-controlled private-key operations run on Tokio's
        // blocking pool. The authentication permit remains in this async
        // frame while the job runs, so moving CPU work off a runtime worker
        // does not weaken the concurrency bound.
        let private_key = self.server.key_store.private_key.clone();
        let worker_authentication_slot = Arc::clone(&authentication_slot);
        let decryption = tokio::select! {
            result = spawn_blocking(move || {
                let _authentication_slot = worker_authentication_slot;
                decrypt_login_key(private_key, packet, challenge)
            }) => result,
            () = self.cancel_token.cancelled() => return ConnectionAction::none(),
        };
        let secret_key = match decryption {
            Ok(Ok(secret_key)) => secret_key,
            Ok(Err(LoginKeyDecryptionError::InvalidChallenge)) => {
                self.kick("Invalid challenge response".into()).await;
                return ConnectionAction::none();
            }
            Ok(Err(LoginKeyDecryptionError::InvalidKey)) => {
                self.kick("Invalid key".into()).await;
                return ConnectionAction::none();
            }
            Err(error) => {
                log::error!("Login RSA worker failed for client {}: {error}", self.id);
                self.kick("Authentication failed".into()).await;
                return ConnectionAction::none();
            }
        };

        let Ok(_) = self
            .connection_updates
            .send(ConnectionUpdate::EnableEncryption(secret_key))
        else {
            self.kick("Failed to send connection update".into()).await;
            return ConnectionAction::none();
        };

        tokio::select! {
            () = self.connection_updated.notified() => {}
            () = self.cancel_token.cancelled() => return ConnectionAction::none(),
        }

        let profile = if self.server.config.online_mode {
            let server_hash = &Sha1::new()
                .chain_update(secret_key)
                .chain_update(&self.server.key_store.public_key_der)
                .finalize();

            let server_hash = signed_bytes_be_to_hex(server_hash);

            let authentication = mojang_authenticate_with_cancel(
                &requested_username,
                &server_hash,
                self.server.config.auth_server.as_deref(),
                self.server.config.allow_insecure_auth_server,
                &self.cancel_token,
            )
            .await;
            match authentication {
                Ok(profile) => profile,
                Err(AuthError::Cancelled) => return ConnectionAction::none(),
                Err(error) => {
                    self.kick(match error {
                        AuthError::FailedResponse => TextComponent::translated(
                            translations::MULTIPLAYER_DISCONNECT_AUTHSERVERS_DOWN.msg(),
                        ),
                        AuthError::UnverifiedUsername => TextComponent::translated(
                            translations::MULTIPLAYER_DISCONNECT_UNVERIFIED_USERNAME.msg(),
                        ),
                        AuthError::InvalidAuthServer(auth_server) => {
                            log::error!(
                                "Invalid authentication server URL configured: {auth_server}"
                            );
                            TextComponent::translated(
                                translations::MULTIPLAYER_DISCONNECT_AUTHSERVERS_DOWN.msg(),
                            )
                        }
                        e => e.to_string().into(),
                    })
                    .await;
                    return ConnectionAction::none();
                }
            }
        } else {
            GameProfile {
                id: offline_uuid(&requested_username),
                name: requested_username,
                properties: vec![],
                profile_actions: None,
            }
        };
        drop(authentication_slot);

        // A duplicate UUID is settled at admission, not here: `queue_player_join`
        // kicks the session already holding it with
        // `multiplayer.disconnect.duplicate_login` and waits for it to leave,
        // which is what `PlayerList.disconnectAllPlayersWithProfile` does.
        // A duplicate *name* on a different UUID is still unhandled.

        let action = self
            .send_login_finished(&profile)
            .await
            .with_reader_encryption(secret_key);
        let sequence_result = self.pre_play_state.lock().complete_login(profile);
        if let Err(error) = sequence_result {
            return self.reject_unexpected_packet(error).await;
        }
        action
    }

    /// Sends the successful login response.
    ///
    pub(crate) async fn send_login_finished(&self, profile: &GameProfile) -> ConnectionAction {
        let mut action = ConnectionAction::none();
        if let Some(compression) = self.server.config.compression {
            // The threshold is a `NonZeroU32` and the packet field is an `i32`,
            // so a configured value above `i32::MAX` has no wire form at all.
            // Vanilla's `network-compression-threshold` is an int and cannot
            // reach that; clamping keeps a mistyped config from aborting the
            // server on the first login, which is what this used to do.
            self.send_bare_packet_now(CLoginCompression::new(
                i32::try_from(compression.threshold.get()).unwrap_or(i32::MAX),
            ))
            .await;
            self.compression.store(Some(compression));
            action = ConnectionAction::reader_compression(compression);
        }

        self.send_bare_packet_now(CLoginFinished::new(
            profile.into(),
            self.connection_session.session_id(),
        ))
        .await;

        action
    }

    /// Handles the login acknowledged packet and transitions to the configuration state.
    pub(crate) async fn handle_login_acknowledged(&self) -> ConnectionAction {
        let sequence_result = self.pre_play_state.lock().acknowledge_login();
        if let Err(error) = sequence_result {
            return self.reject_unexpected_packet(error).await;
        }
        self.protocol.store(ConnectionProtocol::Config);

        self.start_configuration().await;
        ConnectionAction::none()
    }
}

#[cfg(test)]
mod tests {
    use rsa::RsaPublicKey;

    use super::*;

    #[test]
    fn login_key_decryption_validates_both_ciphertexts_and_the_challenge() {
        let private_key = RsaPrivateKey::new(&mut rand::rng(), 1024)
            .expect("test RSA private key should generate");
        let public_key = RsaPublicKey::from(&private_key);
        let challenge = [1, 2, 3, 4];
        let secret = [7; 16];
        let packet = SKey::new(
            public_key
                .encrypt(&mut rand::rng(), Pkcs1v15Encrypt, &secret)
                .expect("test secret should encrypt"),
            public_key
                .encrypt(&mut rand::rng(), Pkcs1v15Encrypt, &challenge)
                .expect("test challenge should encrypt"),
        );

        assert_eq!(
            decrypt_login_key(private_key.clone(), packet.clone(), challenge),
            Ok(secret)
        );
        assert_eq!(
            decrypt_login_key(private_key.clone(), packet.clone(), [9; 4]),
            Err(LoginKeyDecryptionError::InvalidChallenge)
        );

        let malformed_key = SKey::new(vec![0], packet.challenge);
        assert_eq!(
            decrypt_login_key(private_key, malformed_key, challenge),
            Err(LoginKeyDecryptionError::InvalidKey)
        );
    }
}
