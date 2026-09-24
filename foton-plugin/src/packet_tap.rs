//! Foton's packet tap, answered by the Java side.
//!
//! This is what a packet library such as `PacketEvents` stands on. Foton hands
//! over every packet as protocol bytes, on the connection's own network task,
//! and `foton.PacketBridge` hands back what its listeners decided.
//!
//! The tap is installed only when the Java side asks for it, so a server whose
//! plugins never touch raw packets never pays for a crossing.

use std::ffi::c_void;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use foton_core::packet_tap::{PacketTap, TapOutcome, TapPhase, TapVerdict};
use foton_core::player::connection::PlayerConnection;
use jni::errors::Error as JniError;
use jni::objects::{GlobalRef, JByteArray, JClass, JString, JValue};
use jni::sys::{jboolean, jint};
use jni::{JNIEnv, JavaVM};
use uuid::Uuid;

use crate::natives;

/// The Java class that receives packets.
const BRIDGE: &str = "foton/PacketBridge";

/// `inbound` and `outbound` answer with one of these in their low bits.
const PASS: jint = 0;
const CANCEL: jint = 1;
const REWRITE: jint = 2;
/// Set by `outbound` when work waits for the packet to be written.
const AFTER_SEND: jint = 1 << 4;

/// Local references one crossing may create. Network threads stay attached
/// for their whole life, so every call runs in its own frame; without one,
/// each packet would leave its byte array pinned until the thread exits.
const FRAME: i32 = 8;

struct JavaPacketTap {
    vm: Arc<JavaVM>,
    bridge: GlobalRef,
}

impl JavaPacketTap {
    /// Runs `call` on this thread's JNI environment, inside a local frame, and
    /// clears any exception a listener threw so the next packet starts clean.
    ///
    /// Attached as a daemon: these are Tokio's worker threads, which outlive
    /// any one connection and must never keep the JVM from exiting.
    fn with_env<T>(
        &self,
        fallback: T,
        call: impl FnOnce(&mut JNIEnv<'_>, &JClass<'_>) -> Option<T>,
    ) -> T {
        let Ok(mut env) = self.vm.attach_current_thread_as_daemon() else {
            return fallback;
        };
        let class: &JClass<'_> = self.bridge.as_obj().into();
        let answer = env
            .with_local_frame(FRAME, |env| Ok::<_, JniError>(call(env, class)))
            .ok()
            .flatten();
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        answer.unwrap_or(fallback)
    }

    fn notify(&self, method: &str, signature: &str, args: &[JValue<'_, '_>]) {
        self.with_env((), |env, class| {
            env.call_static_method(class, method, signature, args)
                .ok()?;
            Some(())
        });
    }

    fn offer(
        &self,
        method: &str,
        connection: u64,
        phase: TapPhase,
        id: i32,
        payload: &[u8],
    ) -> (jint, Option<Vec<u8>>) {
        self.with_env((PASS, None), |env, class| {
            let bytes = env.byte_array_from_slice(payload).ok()?;
            let answer = env
                .call_static_method(
                    class,
                    method,
                    "(JII[B)I",
                    &[
                        JValue::Long(connection_id(connection)),
                        JValue::Int(phase_id(phase)),
                        JValue::Int(id),
                        JValue::Object(&bytes),
                    ],
                )
                .ok()?
                .i()
                .ok()?;
            if answer & 0b11 != REWRITE {
                return Some((answer, None));
            }
            let rewritten = env
                .call_static_method(class, "takeRewrite", "()[B", &[])
                .ok()?
                .l()
                .ok()?;
            let rewritten = env.convert_byte_array(JByteArray::from(rewritten)).ok()?;
            Some((answer, Some(rewritten)))
        })
    }
}

/// Connection ids are handed out from one counter and fit comfortably; a
/// Java `long` is signed, so the conversion wraps rather than failing.
#[expect(
    clippy::cast_possible_wrap,
    reason = "a connection id is an opaque key on both sides; only equality matters"
)]
const fn connection_id(connection: u64) -> i64 {
    connection as i64
}

const fn phase_id(phase: TapPhase) -> jint {
    match phase {
        TapPhase::Configuration => 0,
        TapPhase::Play => 1,
    }
}

fn verdict(answer: jint, rewritten: Option<Vec<u8>>) -> TapVerdict {
    match (answer & 0b11, rewritten) {
        (CANCEL, _) => TapVerdict::Cancel,
        (REWRITE, Some(payload)) => TapVerdict::Rewrite(payload),
        // A rewrite whose bytes never arrived is safer passed than dropped:
        // the listener saw the packet, and the client still gets it.
        _ => TapVerdict::Pass,
    }
}

impl PacketTap for JavaPacketTap {
    fn opened(&self, connection: u64, profile: Uuid, name: &str, address: SocketAddr) {
        self.with_env((), |env, class| {
            let profile = env.new_string(profile.to_string()).ok()?;
            let name = env.new_string(name).ok()?;
            let address = env.new_string(address.to_string()).ok()?;
            env.call_static_method(
                class,
                "opened",
                "(JLjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
                &[
                    JValue::Long(connection_id(connection)),
                    JValue::Object(&profile),
                    JValue::Object(&name),
                    JValue::Object(&address),
                ],
            )
            .ok()?;
            Some(())
        });
    }

    fn playing(&self, connection: u64, player: Uuid, entity_id: i32) {
        self.with_env((), |env, class| {
            let player = env.new_string(player.to_string()).ok()?;
            env.call_static_method(
                class,
                "playing",
                "(JLjava/lang/String;I)V",
                &[
                    JValue::Long(connection_id(connection)),
                    JValue::Object(&player),
                    JValue::Int(entity_id),
                ],
            )
            .ok()?;
            Some(())
        });
    }

    fn closed(&self, connection: u64) {
        self.notify("closed", "(J)V", &[JValue::Long(connection_id(connection))]);
    }

    fn inbound(&self, connection: u64, phase: TapPhase, id: i32, payload: &[u8]) -> TapVerdict {
        let (answer, rewritten) = self.offer("inbound", connection, phase, id, payload);
        verdict(answer, rewritten)
    }

    fn outbound(&self, connection: u64, phase: TapPhase, id: i32, payload: &[u8]) -> TapOutcome {
        let (answer, rewritten) = self.offer("outbound", connection, phase, id, payload);
        TapOutcome {
            verdict: verdict(answer, rewritten),
            after_send: answer & AFTER_SEND != 0,
        }
    }

    fn sent(&self, connection: u64) {
        self.notify("sent", "(J)V", &[JValue::Long(connection_id(connection))]);
    }
}

/// The bridge class, resolved once. `FindClass` from a network thread sees
/// only the system class loader, which is where the API jar lives; resolving
/// it here, on a thread Java called in on, is simply cheaper.
static BRIDGE_CLASS: OnceLock<GlobalRef> = OnceLock::new();

/// `foton.Native.packetTapEnable`
extern "system" fn packet_tap_enable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    enable: jboolean,
) -> jboolean {
    let Some(server) = natives::server() else {
        return u8::from(false);
    };
    if enable == 0 {
        server.packet_taps.uninstall();
        return u8::from(true);
    }
    let bridge = if let Some(bridge) = BRIDGE_CLASS.get() {
        bridge.clone()
    } else {
        let Ok(class) = env.find_class(BRIDGE) else {
            return u8::from(false);
        };
        let Ok(global) = env.new_global_ref(class) else {
            return u8::from(false);
        };
        BRIDGE_CLASS.get_or_init(|| global).clone()
    };
    let Ok(vm) = env.get_java_vm() else {
        return u8::from(false);
    };
    server.packet_taps.install(Arc::new(JavaPacketTap {
        vm: Arc::new(vm),
        bridge,
    }));
    u8::from(true)
}

/// `foton.Native.packetTapSkipOutbound`
extern "system" fn packet_tap_skip_outbound(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    skip: jboolean,
) {
    if let Some(server) = natives::server() {
        server.packet_taps.set_outbound_skipped(id, skip != 0);
    }
}

/// `foton.Native.packetSend`
extern "system" fn packet_send(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    player: JString<'_>,
    id: jint,
    payload: JByteArray<'_>,
    silent: jboolean,
) -> jboolean {
    let Some(player) = natives::player(&mut env, &player) else {
        return u8::from(false);
    };
    let Ok(payload) = env.convert_byte_array(&payload) else {
        return u8::from(false);
    };
    let PlayerConnection::Java(connection) = &*player.connection else {
        return u8::from(false);
    };
    u8::from(connection.send_raw(id, &payload, silent != 0))
}

/// `foton.Native.packetReceive`
extern "system" fn packet_receive(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    player: JString<'_>,
    id: jint,
    payload: JByteArray<'_>,
    silent: jboolean,
) -> jboolean {
    let Some(server) = natives::server() else {
        return u8::from(false);
    };
    let Some(player) = natives::player(&mut env, &player) else {
        return u8::from(false);
    };
    let Ok(payload) = env.convert_byte_array(&payload) else {
        return u8::from(false);
    };
    let PlayerConnection::Java(connection) = &*player.connection else {
        return u8::from(false);
    };
    u8::from(connection.receive_raw(&server, id, payload, silent != 0))
}

/// The natives this module answers, for [`natives::bindings`].
pub(crate) fn bindings() -> Vec<jni::NativeMethod> {
    fn method(name: &str, signature: &str, pointer: *mut c_void) -> jni::NativeMethod {
        jni::NativeMethod {
            name: name.into(),
            sig: signature.into(),
            fn_ptr: pointer,
        }
    }

    vec![
        method("packetTapEnable", "(Z)Z", packet_tap_enable as *mut c_void),
        method(
            "packetTapSkipOutbound",
            "(IZ)V",
            packet_tap_skip_outbound as *mut c_void,
        ),
        method(
            "packetSend",
            "(Ljava/lang/String;I[BZ)Z",
            packet_send as *mut c_void,
        ),
        method(
            "packetReceive",
            "(Ljava/lang/String;I[BZ)Z",
            packet_receive as *mut c_void,
        ),
    ]
}
