//! JNI adapter for the Paper channel hook consumed by `ViaVersion`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use foton_protocol::{
    packet_traits::{PacketDirection, PacketTranslation, PacketTranslator, TranslationBatch},
    utils::{MAX_PACKET_DATA_SIZE, PacketError},
};
use foton_utils::locks::SyncMutex;
use jni::{
    JNIEnv, JavaVM,
    errors::Error as JniError,
    objects::{GlobalRef, JByteArray, JObjectArray, JValue, JValueOwned},
};

const BRIDGE_CLASS: &str = "foton/network/FotonViaBridge";
const MAX_EXCHANGE_PACKETS: usize = 256;
const MAX_EXCHANGE_BYTES: usize = MAX_PACKET_DATA_SIZE * 4;

pub(crate) struct ViaTranslator {
    vm: Arc<JavaVM>,
    channel: SyncMutex<Option<GlobalRef>>,
    closed: AtomicBool,
}

impl ViaTranslator {
    pub(crate) fn open(vm: Arc<JavaVM>) -> Result<Option<PacketTranslation>, PacketError> {
        let mut env = vm
            .attach_current_thread()
            .map_err(|error| Self::jni_error("attach JVM", error))?;
        let channel = match env
            .call_static_method(
                BRIDGE_CLASS,
                "open",
                "()Lfoton/network/FotonViaChannel;",
                &[],
            )
            .and_then(JValueOwned::l)
        {
            Ok(channel) => channel,
            Err(error) => {
                Self::clear_exception(&mut env);
                return Err(Self::jni_error("open Via channel", error));
            }
        };
        if channel.is_null() {
            return Ok(None);
        }
        let channel = env
            .new_global_ref(channel)
            .map_err(|error| Self::jni_error("retain Via channel", error))?;
        drop(env);
        Ok(Some(PacketTranslation::new(Arc::new(Self {
            vm,
            channel: SyncMutex::new(Some(channel)),
            closed: AtomicBool::new(false),
        }))))
    }

    fn invoke(&self, method: &str, packet: Option<&[u8]>) -> Result<TranslationBatch, PacketError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(PacketError::ConnectionClosed);
        }
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|error| Self::jni_error("attach JVM", error))?;
        let channel = self.channel.lock();
        let channel = channel.as_ref().ok_or(PacketError::ConnectionClosed)?;
        let output = if let Some(packet) = packet {
            let packet = env
                .byte_array_from_slice(packet)
                .map_err(|error| Self::jni_error("copy packet into JVM", error))?;
            env.call_method(
                channel.as_obj(),
                method,
                "([B)[[B",
                &[JValue::Object(packet.as_ref())],
            )
        } else {
            env.call_method(channel.as_obj(), method, "()[[B", &[])
        };
        let output = match output.and_then(JValueOwned::l) {
            Ok(output) => output,
            Err(error) => {
                Self::clear_exception(&mut env);
                return Err(Self::jni_error("run Via pipeline", error));
            }
        };
        let output = JObjectArray::from(output);
        let count = usize::try_from(
            env.get_array_length(&output)
                .map_err(|error| Self::jni_error("read Via output length", error))?,
        )
        .map_err(|_| PacketError::OutOfBounds)?;
        if count > MAX_EXCHANGE_PACKETS {
            return Err(PacketError::TooLong(count));
        }

        let mut batch = TranslationBatch::default();
        let mut total_bytes = 0usize;
        for index in 0..count {
            let marked = env
                .get_object_array_element(&output, index as i32)
                .map_err(|error| Self::jni_error("read Via output packet", error))?;
            let marked = JByteArray::from(marked);
            let marked = env
                .convert_byte_array(&marked)
                .map_err(|error| Self::jni_error("copy Via output packet", error))?;
            let Some((&direction, packet)) = marked.split_first() else {
                return Err(PacketError::MalformedValue(
                    "Via emitted an empty marked packet".to_owned(),
                ));
            };
            total_bytes = total_bytes
                .checked_add(packet.len())
                .ok_or(PacketError::OutOfBounds)?;
            if total_bytes > MAX_EXCHANGE_BYTES || packet.len() > MAX_PACKET_DATA_SIZE {
                return Err(PacketError::TooLong(total_bytes));
            }
            match direction {
                0 => batch.serverbound.push(packet.to_vec()),
                1 => batch.clientbound.push(packet.to_vec()),
                _ => {
                    return Err(PacketError::MalformedValue(
                        "Via emitted an invalid packet direction".to_owned(),
                    ));
                }
            }
        }
        Ok(batch)
    }

    fn jni_error(context: &str, error: JniError) -> PacketError {
        PacketError::Other(format!("{context}: {error}"))
    }

    fn clear_exception(env: &mut JNIEnv<'_>) {
        if matches!(env.exception_check(), Ok(true)) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
    }
}

impl PacketTranslator for ViaTranslator {
    fn exchange(
        &self,
        direction: PacketDirection,
        packet_data: &[u8],
    ) -> Result<TranslationBatch, PacketError> {
        match direction {
            PacketDirection::Serverbound => self.invoke("serverbound", Some(packet_data)),
            PacketDirection::Clientbound => self.invoke("clientbound", Some(packet_data)),
        }
    }

    fn poll(&self) -> Result<TranslationBatch, PacketError> {
        self.invoke("poll", None)
    }

    fn close(&self) -> Result<(), PacketError> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|error| Self::jni_error("attach JVM", error))?;
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let mut channel = self.channel.lock();
        let Some(channel) = channel.take() else {
            return Ok(());
        };
        if let Err(error) = env.call_method(channel.as_obj(), "close", "()V", &[]) {
            Self::clear_exception(&mut env);
            drop(channel);
            return Err(Self::jni_error("close Via channel", error));
        }
        drop(channel);
        Ok(())
    }
}

impl Drop for ViaTranslator {
    fn drop(&mut self) {
        // Normal connection shutdown closes explicitly on a blocking worker.
        // This fallback matters when opening the channel exceeded its deadline
        // and the late worker result is dropped without reaching its caller.
        let _ = PacketTranslator::close(self);
    }
}
