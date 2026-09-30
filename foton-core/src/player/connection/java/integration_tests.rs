//! Regressions for the packet tap sharing the hardened transport.

use std::env::temp_dir;
use std::sync::atomic::{AtomicUsize, Ordering};

use foton_protocol::packet_traits::PacketTranslator;
use tokio::fs::remove_dir_all;
use tokio::io::AsyncReadExt;
use tokio::runtime::Builder;
use uuid::Uuid;

use super::*;
use crate::packet_tap::TapOutcome;
use crate::permission::PermissionSubjectIndex;
use crate::server::test_server;
use crate::test_support::{TestPlayerBuilder, fresh_test_world};

#[derive(Default)]
struct RecordingBridge {
    outbound: SyncMutex<Vec<Vec<u8>>>,
    translated: SyncMutex<Vec<Vec<u8>>>,
    inbound: SyncMutex<Vec<Vec<u8>>>,
    sent: AtomicUsize,
}

impl PacketTap for RecordingBridge {
    fn opened(&self, _: u64, _: Uuid, _: &str, _: SocketAddr) {}
    fn playing(&self, _: u64, _: Uuid, _: i32) {}
    fn closed(&self, _: u64) {}

    fn inbound(&self, _: u64, _: TapPhase, _: i32, payload: &[u8]) -> TapVerdict {
        self.inbound.lock().push(payload.to_vec());
        TapVerdict::Cancel
    }

    fn outbound(&self, _: u64, _: TapPhase, _: i32, payload: &[u8]) -> TapOutcome {
        self.outbound.lock().push(payload.to_vec());
        TapOutcome {
            verdict: if payload == [2] {
                TapVerdict::Cancel
            } else {
                TapVerdict::Rewrite(vec![9])
            },
            after_send: true,
        }
    }

    fn sent(&self, _: u64) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }
}

impl PacketTranslator for RecordingBridge {
    fn exchange(
        &self,
        direction: PacketDirection,
        packet_data: &[u8],
    ) -> Result<TranslationBatch, PacketError> {
        assert_eq!(direction, PacketDirection::Clientbound);
        self.translated.lock().push(packet_data.to_vec());
        Ok(TranslationBatch {
            clientbound: vec![packet_data.to_vec()],
            serverbound: Vec::new(),
        })
    }

    fn poll(&self) -> Result<TranslationBatch, PacketError> {
        Ok(TranslationBatch::default())
    }

    fn close(&self) -> Result<(), PacketError> {
        Ok(())
    }
}

#[test]
fn tap_rewrites_before_translation_and_preserves_silent_and_cancelled_packets() {
    let world = fresh_test_world("packet_tap_integration");
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should initialize");
    runtime.block_on(async {
        let storage = temp_dir().join(format!("foton-packet-tap-{}", Uuid::new_v4()));
        let server = test_server(Arc::clone(&world), PermissionSubjectIndex::new(), &storage)
            .await
            .expect("test server should initialize");
        let player = TestPlayerBuilder::new(world, "TapTester", 1)
            .server(&server)
            .build();
        let bridge = Arc::new(RecordingBridge::default());
        server.packet_taps.install(bridge.clone());
        let (sender, mut receiver) = outbound_packet_channel_with_budget(1024);
        let (mut connection, mut client) = tests::test_java_connection(sender.clone()).await;
        connection.player = Arc::downgrade(&player);
        connection.translation = Some(PacketTranslation::new(bridge.clone()));
        let (translated, _translated_recv) = mpsc::channel(8);
        connection.translated_serverbound = Some(translated);

        let packet = |payload| {
            EncodedPacket::from_id_and_payload(play::C_SYSTEM_CHAT, &[payload], None)
                .expect("test packet should frame")
        };
        assert!(
            sender
                .try_send_chunk_batch(vec![packet(1), packet(2)])
                .is_ok()
        );
        let envelope = receiver.recv_envelope().await.expect("batch is queued");
        assert!(
            !connection
                .write_envelope(envelope)
                .await
                .expect("batch should write")
        );
        assert!(connection.send_raw(play::C_SYSTEM_CHAT, &[3], true));
        let envelope = receiver
            .recv_envelope()
            .await
            .expect("silent packet is queued");
        assert!(
            !connection
                .write_envelope(envelope)
                .await
                .expect("silent packet should write")
        );

        let mut expected = packet(9).encoded_data.as_slice().to_vec();
        expected.extend_from_slice(packet(3).encoded_data.as_slice());
        let mut wire = vec![0; expected.len()];
        timeout(Duration::from_secs(2), client.read_exact(&mut wire))
            .await
            .expect("wire read should finish")
            .expect("wire should be readable");
        assert_eq!(wire, expected);
        assert_eq!(*bridge.outbound.lock(), vec![vec![1], vec![2]]);
        assert_eq!(bridge.sent.load(Ordering::Relaxed), 1);
        assert_eq!(
            *bridge.translated.lock(),
            vec![
                packet(9)
                    .to_packet_data(None)
                    .expect("rewritten packet should encode"),
                packet(3)
                    .to_packet_data(None)
                    .expect("silent packet should encode")
            ],
        );

        // A translated native packet reaches the tap before the malformed
        // keep-alive could be decoded. Its cancellation must suppress that error.
        let inbound = RawPacket::new(play::S_KEEP_ALIVE, vec![7])
            .to_packet_data()
            .expect("inbound packet should encode");
        connection
            .process_translation_batch(
                TranslationBatch {
                    serverbound: vec![inbound],
                    clientbound: Vec::new(),
                },
                &player,
                &server,
            )
            .await
            .expect("tap cancellation should prevent decoding");
        assert_eq!(*bridge.inbound.lock(), vec![vec![7]]);
        assert!(!connection.receive_raw(&server, play::S_KEEP_ALIVE, vec![7], true));
        assert_eq!(
            bridge.inbound.lock().len(),
            1,
            "silent injection bypasses tap"
        );

        drop(connection);
        drop(player);
        drop(server);
        remove_dir_all(storage).await.expect("remove test storage");
    });
}

#[tokio::test]
async fn silent_packet_retains_budget_until_its_write_completes() {
    let (sender, mut receiver) = outbound_packet_channel_with_budget(8);
    let packet =
        EncodedPacket::from_id_and_payload(0, &[1, 2, 3], None).expect("test packet should frame");
    assert!(
        sender
            .try_send(OutboundPacket::Silent(packet.clone()))
            .is_ok()
    );
    let queued = receiver.recv().await.expect("silent packet is queued");
    assert!(matches!(
        sender.try_send(OutboundPacket::Silent(packet.clone())),
        Err(mpsc::error::TrySendError::Full(_)),
    ));
    drop(queued);
    assert!(sender.try_send(OutboundPacket::Silent(packet)).is_ok());
}
