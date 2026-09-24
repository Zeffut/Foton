//! # Foton Protocol Packet Traits
//!
//! This module contains the traits for the packets.
use std::{
    future::Future,
    io::{Cursor, Read, Write},
    num::NonZeroU32,
    pin::Pin,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use foton_utils::{
    FrontVec,
    codec::VarInt,
    serial::{ReadFrom, WriteTo},
};
use serde::Deserialize;
use tokio::{
    spawn,
    sync::Semaphore,
    task::spawn_blocking,
    time::{Instant, timeout_at},
};

use crate::utils::{ConnectionProtocol, MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, PacketError};

// These are the network read/write traits
/// A trait for packets sent from the server to the client.
pub trait ServerPacket: ReadFrom {
    /// Reads a packet from the given data.
    fn read_packet(data: &mut Cursor<&[u8]>) -> Result<Self, PacketError> {
        Self::read(data).map_err(PacketError::from)
    }
}

/// A trait for packets sent from the client to the server.
pub trait ClientPacket: WriteTo {
    /// Writes the packet to the given writer.
    ///
    /// # Errors
    /// - If the packet fails to write.
    /// - If the protocol is invalid.
    fn write_packet(
        &self,
        writer: &mut impl Write,
        protocol: ConnectionProtocol,
    ) -> Result<(), PacketError> {
        let packet_id = self
            .get_id(protocol)
            .ok_or(PacketError::InvalidProtocol(format!(
                "Invalid protocol {protocol:?}"
            )))?;
        VarInt(packet_id).write(writer)?;
        self.write(writer).map_err(PacketError::from)
    }

    /// Gets the ID of the packet for the given protocol.
    fn get_id(&self, protocol: ConnectionProtocol) -> Option<i32>;
}

/// Information about compression.
#[derive(Copy, Clone, Debug, Deserialize)]
pub struct CompressionInfo {
    /// The compression threshold used when compression is enabled.
    /// Its an `NonZeroU32` to allow for nullptr optimization in `Option<Self>` cases
    pub threshold: NonZeroU32,
    /// A value between `0..9`.
    /// `1` = Optimize for the best speed of encoding.
    /// `9` = Optimize for the size of data being encoded.
    pub level: i32,
}

impl Default for CompressionInfo {
    #[expect(
        clippy::unwrap_used,
        reason = "256 is a known nonzero compression threshold"
    )]
    fn default() -> Self {
        Self {
            threshold: NonZeroU32::new(256).unwrap(),
            level: 4,
        }
    }
}

/// Represents an encoded clientbound packet, optionally applying compression based on threshold and level.
///
/// # Packet Size Limits
/// - Maximum packet size: 2097151 bytes (2^21 - 1, max 3-byte `VarInt`)
/// - Maximum uncompressed size for compressed packets: 8388608 bytes (2^23)
/// - Length field must not exceed 3 bytes
///
/// # Packet Encoding Format
///
/// **Without Compression:**
/// ```text
/// [Length: VarInt]     Length of (Packet ID + Data)
/// [Packet ID: VarInt]  Protocol ID from packet report
/// [Data: Byte Array]   Packet payload
/// ```
///
/// **With Compression (size >= threshold):**
/// ```text
/// [Length: VarInt]     Length of (Data Length + compressed data)
/// [Data Length: VarInt] Length of uncompressed (Packet ID + Data)
/// [Compressed Data]    zlib compressed (Packet ID + Data)
/// ```
///
/// **With Compression (size < threshold):**
/// ```text
/// [Length: VarInt]     Length of (Data Length + uncompressed data)
/// [Data Length: VarInt] 0 to indicate uncompressed
/// [Packet ID: VarInt]  Protocol ID from packet report
/// [Data: Byte Array]   Uncompressed packet payload
/// ```
///
/// Compression is only applied when:
/// 1. Compression is enabled via Set Compression packet
/// 2. The uncompressed data length meets/exceeds the threshold
/// 3. The threshold is non-negative
#[derive(Clone)]
pub struct EncodedPacket {
    // This is optimized for reduces allocation
    /// The encoded data.
    pub encoded_data: Arc<FrontVec>,
}

impl EncodedPacket {
    fn from_data_uncompressed(mut packet_data: FrontVec) -> Result<Self, PacketError> {
        let data_len = packet_data.len();
        let varint_size = VarInt::written_size(data_len as i32);

        let complete_len = varint_size + data_len;
        if complete_len > MAX_PACKET_SIZE {
            return Err(PacketError::TooLong(complete_len));
        }

        VarInt(data_len as i32).set_in_front(&mut packet_data, varint_size);

        Ok(Self {
            encoded_data: Arc::new(packet_data),
        })
    }

    fn from_packet_data_front(
        mut packet_data: FrontVec,
        compression: CompressionInfo,
    ) -> Result<Self, PacketError> {
        let data_len = packet_data.len();
        // We dont need any more size check to convert to i32 as MAX_PACKET_DATA_SIZE < i32::MAX
        if data_len + VarInt::MAX_SIZE * 2 > MAX_PACKET_DATA_SIZE {
            Err(PacketError::TooLong(data_len))?;
        }

        if data_len >= compression.threshold.get() as _ {
            let mut buf = FrontVec::new(10);
            let mut compressor =
                ZlibEncoder::new(&mut buf, Compression::new(compression.level as u32));

            compressor
                .write_all(&packet_data)
                .map_err(|e| PacketError::CompressionFailed(e.to_string()))?;
            compressor
                .finish()
                .map_err(|e| PacketError::CompressionFailed(e.to_string()))?;

            // compressed data cant be larger so we dont need to check the size again
            let varint_size = VarInt::written_size(data_len as i32);
            let full_len = varint_size + buf.len();
            let full_varint_size = VarInt::written_size(full_len as i32);

            VarInt(data_len as i32).set_in_front(&mut buf, varint_size);
            VarInt(full_len as i32).set_in_front(&mut buf, full_varint_size);
            log::trace!(
                "data length: {data_len}, full length: {full_len}, varint size: {varint_size}, full varint size: {full_varint_size}"
            );

            Ok(Self {
                encoded_data: Arc::new(buf),
            })
        } else {
            // Pushed before data:
            // Length of (Data Length) + length of compressed (Packet ID + Data)
            // 0 to indicate uncompressed

            let data_len_with_header = data_len + 1;
            let varint_size = VarInt::written_size(data_len_with_header as i32);

            VarInt(0).set_in_front(&mut packet_data, 1);
            VarInt(data_len_with_header as i32).set_in_front(&mut packet_data, varint_size);

            Ok(Self {
                encoded_data: Arc::new(packet_data),
            })
        }
    }

    /// Creates a new `EncodedPacket` from a bare packet.
    ///
    /// # Errors
    /// - If the packet fails to write.
    /// - If the packet fails to compress.
    pub fn from_bare<P: ClientPacket>(
        packet: P,
        compression: Option<CompressionInfo>,
        protocol: ConnectionProtocol,
    ) -> Result<Self, PacketError> {
        let buf = Self::write_vec(packet, protocol)?;
        Self::from_data(buf, compression)
    }

    /// Frames and optionally compresses an already serialized packet body.
    ///
    /// `packet_data` must contain exactly `[VarInt packet id][payload]`.
    pub fn from_packet_data(
        packet_data: &[u8],
        compression: Option<CompressionInfo>,
    ) -> Result<Self, PacketError> {
        if packet_data.is_empty() || packet_data.len() > MAX_PACKET_DATA_SIZE {
            return Err(PacketError::OutOfBounds);
        }
        let mut data = FrontVec::capacity(10, packet_data.len());
        data.extend_from_slice(packet_data);
        Self::from_data(data, compression)
    }

    /// Returns the unframed, decompressed `[VarInt packet id][payload]` body.
    pub fn to_packet_data(
        &self,
        compression: Option<CompressionInfo>,
    ) -> Result<Vec<u8>, PacketError> {
        let bytes = self.encoded_data.as_slice();
        let mut frame = Cursor::new(bytes);
        let declared = VarInt::read(&mut frame)?.0;
        let declared = usize::try_from(declared).map_err(|_| PacketError::OutOfBounds)?;
        let frame_start = frame.position() as usize;
        if declared == 0 || declared != bytes.len().saturating_sub(frame_start) {
            return Err(PacketError::OutOfBounds);
        }
        let framed = &bytes[frame_start..];
        let data = if let Some(info) = compression {
            let mut compressed = Cursor::new(framed);
            let uncompressed = VarInt::read(&mut compressed)?.0;
            let uncompressed =
                usize::try_from(uncompressed).map_err(|_| PacketError::OutOfBounds)?;
            let start = compressed.position() as usize;
            if uncompressed == 0 {
                framed[start..].to_vec()
            } else {
                if uncompressed > MAX_PACKET_DATA_SIZE
                    || uncompressed < info.threshold.get() as usize
                {
                    return Err(PacketError::OutOfBounds);
                }
                let decoder = ZlibDecoder::new(&framed[start..]);
                let mut limited = decoder.take(uncompressed as u64 + 1);
                let mut output = Vec::with_capacity(uncompressed.min(8 * 1024));
                limited
                    .read_to_end(&mut output)
                    .map_err(|error| PacketError::DecompressionFailed(error.to_string()))?;
                if output.len() != uncompressed {
                    return Err(PacketError::DecompressionFailed(format!(
                        "expected {uncompressed} bytes, decoded {}",
                        output.len()
                    )));
                }
                output
            }
        } else {
            framed.to_vec()
        };
        if data.is_empty() || data.len() > MAX_PACKET_DATA_SIZE {
            return Err(PacketError::OutOfBounds);
        }
        Ok(data)
    }

    fn write_vec<P: ClientPacket>(
        packet: P,
        protocol: ConnectionProtocol,
    ) -> Result<FrontVec, PacketError> {
        let mut buf = FrontVec::new(6);
        packet.write_packet(&mut buf, protocol)?;
        Ok(buf)
    }

    fn from_data(buf: FrontVec, compression: Option<CompressionInfo>) -> Result<Self, PacketError> {
        if let Some(compression) = compression {
            Self::from_packet_data_front(buf, compression)
        } else {
            Self::from_data_uncompressed(buf)
        }
    }
}

/// Direction in which a Minecraft packet crosses the protocol translator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketDirection {
    /// Client to server.
    Serverbound,
    /// Server to client.
    Clientbound,
}

/// All packets emitted by one translator exchange, including synthetic packets.
#[derive(Debug, Default)]
pub struct TranslationBatch {
    /// Native-protocol packets to feed into Foton.
    pub serverbound: Vec<Vec<u8>>,
    /// Client-protocol packets to write to the socket.
    pub clientbound: Vec<Vec<u8>>,
}

/// Synchronous boundary implemented by an optional Java protocol platform.
pub trait PacketTranslator: Send + Sync {
    /// Transforms one packet and drains all resulting directions.
    fn exchange(
        &self,
        direction: PacketDirection,
        packet_data: &[u8],
    ) -> Result<TranslationBatch, PacketError>;

    /// Runs scheduled work and drains resulting packets.
    fn poll(&self) -> Result<TranslationBatch, PacketError>;

    /// Releases the translator's per-connection resources.
    fn close(&self) -> Result<(), PacketError>;
}

static TRANSLATION_WORKERS: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(32)));
#[cfg(not(test))]
const TRANSLATION_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const TRANSLATION_TIMEOUT: Duration = Duration::from_millis(100);

/// Serialized, async-safe owner of one connection's translator.
#[derive(Clone)]
pub struct PacketTranslation {
    inner: Arc<dyn PacketTranslator>,
    serial: Arc<Semaphore>,
    workers: Arc<Semaphore>,
    failed: Arc<AtomicBool>,
}

struct TranslationCallCompletion {
    failed: Arc<AtomicBool>,
    succeeded: bool,
}

impl Drop for TranslationCallCompletion {
    fn drop(&mut self) {
        if !self.succeeded {
            self.failed.store(true, Ordering::Release);
        }
    }
}

impl PacketTranslation {
    /// Wraps a translator implementation.
    #[must_use]
    pub fn new(inner: Arc<dyn PacketTranslator>) -> Self {
        Self {
            inner,
            serial: Arc::new(Semaphore::new(1)),
            workers: Arc::clone(&TRANSLATION_WORKERS),
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    fn with_workers(inner: Arc<dyn PacketTranslator>, workers: Arc<Semaphore>) -> Self {
        Self {
            inner,
            serial: Arc::new(Semaphore::new(1)),
            workers,
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn blocking<T, F>(&self, allow_failed: bool, operation: F) -> Result<T, PacketError>
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn PacketTranslator>) -> Result<T, PacketError> + Send + 'static,
    {
        if !allow_failed && self.failed.load(Ordering::Acquire) {
            return Err(PacketError::ConnectionClosed);
        }
        let deadline = Instant::now() + TRANSLATION_TIMEOUT;
        let serial = match timeout_at(deadline, Arc::clone(&self.serial).acquire_owned()).await {
            Ok(Ok(serial)) => serial,
            Ok(Err(_)) => return Err(PacketError::ConnectionClosed),
            Err(_) => {
                self.failed.store(true, Ordering::Release);
                return Err(Self::deadline_error());
            }
        };
        if !allow_failed && self.failed.load(Ordering::Acquire) {
            return Err(PacketError::ConnectionClosed);
        }
        let worker = match timeout_at(deadline, Arc::clone(&self.workers).acquire_owned()).await {
            Ok(Ok(worker)) => worker,
            Ok(Err(_)) => return Err(PacketError::ConnectionClosed),
            Err(_) => {
                self.failed.store(true, Ordering::Release);
                return Err(Self::deadline_error());
            }
        };
        let translator = Arc::clone(&self.inner);
        let failed = Arc::clone(&self.failed);
        // A JVM call cannot be forcibly interrupted safely. The owned worker
        // permit therefore moves into the closure and remains consumed until
        // even a timed-out call really returns; the connection is poisoned so
        // no second call can race the still-running Java pipeline.
        let mut task = spawn_blocking(move || {
            let _serial = serial;
            let _worker = worker;
            // Declared after both permits so it poisons the connection before
            // either permit can be released, including during unwinding.
            let mut completion = TranslationCallCompletion {
                failed,
                succeeded: false,
            };
            let result = operation(translator);
            completion.succeeded = result.is_ok();
            result
        });
        if let Ok(joined) = timeout_at(deadline, &mut task).await {
            match joined {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(error)) => {
                    self.failed.store(true, Ordering::Release);
                    Err(error)
                }
                Err(error) => {
                    self.failed.store(true, Ordering::Release);
                    Err(PacketError::Other(format!(
                        "protocol translation task failed: {error}"
                    )))
                }
            }
        } else {
            self.failed.store(true, Ordering::Release);
            let translator = Arc::clone(&self.inner);
            // JNI has no safe cancellation primitive. Keep observing the
            // detached worker and close its channel as soon as it eventually
            // returns, without holding this connection or its serial guard.
            drop(spawn(async move {
                let _ = task.await;
                let cleanup = spawn_blocking(move || translator.close());
                let _ = cleanup.await;
            }));
            Err(Self::deadline_error())
        }
    }

    fn deadline_error() -> PacketError {
        PacketError::Other("protocol translation exceeded its deadline".to_owned())
    }

    /// Transforms a packet outside the Tokio worker and game-tick threads.
    #[must_use]
    pub fn exchange(
        &self,
        direction: PacketDirection,
        packet_data: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<TranslationBatch, PacketError>> + Send + '_>> {
        Box::pin(self.blocking(false, move |translator| {
            translator.exchange(direction, &packet_data)
        }))
    }

    /// Polls scheduled translator output outside the Tokio worker threads.
    #[must_use]
    pub fn poll(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<TranslationBatch, PacketError>> + Send + '_>> {
        Box::pin(self.blocking(false, |translator| translator.poll()))
    }

    /// Closes the translator after all earlier exchanges on this connection.
    #[must_use]
    pub fn close(&self) -> Pin<Box<dyn Future<Output = Result<(), PacketError>> + Send + '_>> {
        Box::pin(self.blocking(true, |translator| translator.close()))
    }
}

#[cfg(test)]
mod translation_packet_tests {
    use std::{
        num::NonZeroU32,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc::{Receiver, Sender, channel},
        },
        time::Duration,
    };

    use foton_utils::locks::SyncMutex;
    use tokio::{
        spawn as spawn_task,
        sync::Semaphore,
        task::spawn_blocking as wait_blocking,
        time::{sleep, timeout},
    };

    use super::{
        CompressionInfo, EncodedPacket, PacketDirection, PacketTranslation, PacketTranslator,
        TranslationBatch,
    };
    use crate::utils::PacketError;

    struct BlockingTranslator {
        calls: AtomicUsize,
        entered: SyncMutex<Option<Sender<()>>>,
        release: SyncMutex<Option<Receiver<()>>>,
        fail_first: bool,
    }

    struct FailingTranslator {
        closed: AtomicBool,
    }

    impl PacketTranslator for FailingTranslator {
        fn exchange(
            &self,
            _direction: PacketDirection,
            _packet_data: &[u8],
        ) -> Result<TranslationBatch, PacketError> {
            Err(PacketError::MalformedValue("broken Via output".to_owned()))
        }

        fn poll(&self) -> Result<TranslationBatch, PacketError> {
            Ok(TranslationBatch::default())
        }

        fn close(&self) -> Result<(), PacketError> {
            self.closed.store(true, Ordering::Release);
            Ok(())
        }
    }

    impl PacketTranslator for BlockingTranslator {
        fn exchange(
            &self,
            _direction: PacketDirection,
            _packet_data: &[u8],
        ) -> Result<TranslationBatch, PacketError> {
            if self.calls.fetch_add(1, Ordering::AcqRel) == 0
                && let Some(release) = self.release.lock().as_ref()
            {
                if let Some(entered) = self.entered.lock().take() {
                    let _ = entered.send(());
                }
                release.recv().map_err(|error| {
                    PacketError::Other(format!("test release channel closed: {error}"))
                })?;
                if self.fail_first {
                    return Err(PacketError::MalformedValue("broken Via output".to_owned()));
                }
            }
            Ok(TranslationBatch::default())
        }

        fn poll(&self) -> Result<TranslationBatch, PacketError> {
            self.calls.fetch_add(1, Ordering::AcqRel);
            Ok(TranslationBatch::default())
        }

        fn close(&self) -> Result<(), PacketError> {
            Ok(())
        }
    }

    fn immediate_translator() -> Arc<BlockingTranslator> {
        Arc::new(BlockingTranslator {
            calls: AtomicUsize::new(0),
            entered: SyncMutex::new(None),
            release: SyncMutex::new(None),
            fail_first: false,
        })
    }

    #[test]
    fn packet_data_round_trips_without_compression() {
        let data = [0xac, 0x02, 1, 2, 3, 4];
        let encoded = EncodedPacket::from_packet_data(&data, None).expect("frame packet body");
        assert_eq!(encoded.to_packet_data(None).expect("unframe packet"), data);
    }

    #[test]
    fn packet_data_round_trips_both_compression_representations() {
        let compression = CompressionInfo {
            threshold: NonZeroU32::new(16).expect("nonzero threshold"),
            level: 4,
        };
        for data in [vec![0, 1, 2], vec![0; 1024]] {
            let encoded = EncodedPacket::from_packet_data(&data, Some(compression))
                .expect("encode compressed protocol body");
            assert_eq!(
                encoded
                    .to_packet_data(Some(compression))
                    .expect("decode compressed protocol body"),
                data
            );
        }
    }

    #[test]
    fn packet_data_rejects_trailing_frame_bytes() {
        let data = [0, 1, 2];
        let encoded = EncodedPacket::from_packet_data(&data, None).expect("frame packet body");
        let mut corrupt = encoded.encoded_data.as_slice().to_vec();
        corrupt.push(9);
        let mut stored = foton_utils::FrontVec::new(0);
        stored.extend_from_slice(&corrupt);
        let corrupt = EncodedPacket {
            encoded_data: Arc::new(stored),
        };
        assert!(corrupt.to_packet_data(None).is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_future_keeps_the_connection_serial_permit() {
        let (entered_send, entered_recv) = channel();
        let (release_send, release_recv) = channel();
        let translator = Arc::new(BlockingTranslator {
            calls: AtomicUsize::new(0),
            entered: SyncMutex::new(Some(entered_send)),
            release: SyncMutex::new(Some(release_recv)),
            fail_first: false,
        });
        let translation = PacketTranslation::new(translator.clone());

        let first_translation = translation.clone();
        let first = spawn_task(async move {
            first_translation
                .exchange(PacketDirection::Serverbound, vec![0])
                .await
        });
        wait_blocking(move || entered_recv.recv())
            .await
            .expect("wait task should run")
            .expect("first translation should enter");
        first.abort();
        let _ = first.await;

        let second_translation = translation.clone();
        let second = spawn_task(async move {
            second_translation
                .exchange(PacketDirection::Serverbound, vec![0])
                .await
        });
        sleep(Duration::from_millis(20)).await;
        assert_eq!(translator.calls.load(Ordering::Acquire), 1);

        release_send
            .send(())
            .expect("first translation should still wait for release");
        timeout(Duration::from_secs(1), second)
            .await
            .expect("second translation should enter after the first returns")
            .expect("second task should not panic")
            .expect("second translation should succeed");
        assert_eq!(translator.calls.load(Ordering::Acquire), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queued_call_cannot_enter_after_the_first_call_fails() {
        let (entered_send, entered_recv) = channel();
        let (release_send, release_recv) = channel();
        let translator = Arc::new(BlockingTranslator {
            calls: AtomicUsize::new(0),
            entered: SyncMutex::new(Some(entered_send)),
            release: SyncMutex::new(Some(release_recv)),
            fail_first: true,
        });
        let translation = PacketTranslation::new(translator.clone());

        let first_translation = translation.clone();
        let first = spawn_task(async move {
            first_translation
                .exchange(PacketDirection::Serverbound, vec![0])
                .await
        });
        wait_blocking(move || entered_recv.recv())
            .await
            .expect("wait task should run")
            .expect("first translation should enter");
        let second_translation = translation.clone();
        let second = spawn_task(async move { second_translation.poll().await });
        sleep(Duration::from_millis(20)).await;
        assert_eq!(translator.calls.load(Ordering::Acquire), 1);

        release_send
            .send(())
            .expect("first translation should still wait for release");
        assert!(matches!(
            first.await.expect("first task should not panic"),
            Err(PacketError::MalformedValue(_))
        ));
        assert!(matches!(
            second.await.expect("second task should not panic"),
            Err(PacketError::ConnectionClosed)
        ));
        assert_eq!(translator.calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn worker_pool_wait_is_covered_by_the_translation_deadline() {
        let translator = immediate_translator();
        let translation = PacketTranslation::with_workers(translator, Arc::new(Semaphore::new(0)));

        let error = translation
            .exchange(PacketDirection::Serverbound, vec![0])
            .await
            .expect_err("an exhausted worker pool must time out");
        assert!(error.to_string().contains("deadline"));
        assert!(matches!(
            translation.poll().await,
            Err(PacketError::ConnectionClosed)
        ));
    }

    #[tokio::test]
    async fn translator_error_poisons_the_connection_before_another_call() {
        let translator = Arc::new(FailingTranslator {
            closed: AtomicBool::new(false),
        });
        let translation = PacketTranslation::new(translator.clone());

        assert!(matches!(
            translation
                .exchange(PacketDirection::Serverbound, vec![0])
                .await,
            Err(PacketError::MalformedValue(_))
        ));
        assert!(matches!(
            translation.poll().await,
            Err(PacketError::ConnectionClosed)
        ));
        translation
            .close()
            .await
            .expect("a poisoned translator must still release its channel");
        assert!(translator.closed.load(Ordering::Acquire));
    }
}
