use super::*;
use crate::natives::TICK_THREAD;
use crate::{PluginHost, PluginHostConfig, forward::spawn_bridge_tests::live_test_server};
use foton_core::server::Server;
use foton_registry::data_components::{
    DataComponentPatch, components::CustomData, vanilla_components::CUSTOM_DATA,
};
use jni::{
    JNIEnv,
    objects::{JByteArray, JString, JValue},
};
use simdnbt::owned::NbtCompound;
use std::{
    env,
    error::Error,
    fs,
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Instant,
};
use tempfile::TempDir;
use tokio::runtime::{Handle, Id as RuntimeId, Runtime};
use tokio::task::try_id;
use tokio::time::sleep;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn start_host(scratch: &TempDir, server: &Arc<Server>) -> TestResult<PluginHost> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let java_home = env::var_os("JAVA_HOME").map_or_else(
        || {
            let output = Command::new("java")
                .args(["-XshowSettings:properties", "-version"])
                .output()
                .expect("java");
            String::from_utf8(output.stderr)
                .expect("properties")
                .lines()
                .find_map(|line| line.trim().strip_prefix("java.home = ").map(PathBuf::from))
                .expect("java.home")
        },
        PathBuf::from,
    );
    // Compile existing native declarations into an isolated JAR copy. The
    // prebuilt JAR can predate them; all protected files remain read-only.
    let api_jar = scratch.path().join("api.jar");
    fs::copy(root.join("plugin-api/build/foton-plugin-api.jar"), &api_jar)?;
    assert!(
        Command::new(java_home.join("bin/javac"))
            .args(["--release", "21", "-cp"])
            .arg(&api_jar)
            .arg("-d")
            .arg(scratch.path())
            .arg(root.join("plugin-api/src/foton/Native.java"))
            .status()?
            .success()
    );
    assert!(
        Command::new(java_home.join("bin/jar"))
            .arg("uf")
            .arg(&api_jar)
            .arg("-C")
            .arg(scratch.path())
            .arg("foton/Native.class")
            .status()?
            .success()
    );
    Ok(PluginHost::start(
        &PluginHostConfig {
            java_home,
            api_jar,
            library_directory: Some(root.join("plugin-api/lib")),
            plugin_directory: scratch.path().join("plugins"),
        },
        &Arc::downgrade(server),
    )?)
}

fn getter(
    env: &mut JNIEnv<'_>,
    name: &JString<'_>,
    pos: BlockPos,
    slot: Option<(u64, i32)>,
) -> TestResult<Option<Vec<u8>>> {
    let mut args = vec![
        JValue::Object(name),
        JValue::Int(pos.x()),
        JValue::Int(pos.y()),
        JValue::Int(pos.z()),
    ];
    let (method, signature) = if let Some((identity, slot)) = slot {
        args.extend([JValue::Long(identity as i64), JValue::Int(slot)]);
        ("brewingStandLiveItem", "(Ljava/lang/String;IIIJI)[B")
    } else {
        ("brewingStandSnapshot", "(Ljava/lang/String;III)[B")
    };
    let result = env
        .call_static_method("foton/Native", method, signature, &args)?
        .l()?;
    if result.is_null() {
        return Ok(None);
    }
    Ok(Some(env.convert_byte_array(JByteArray::from(result))?))
}

fn install_oversized(stand: &BrewingStandBlockEntity, case: &str) {
    let mut state = stand.state_snapshot();
    state.items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
    state.custom_name = None;
    state.lock = LockCode::NO_LOCK;
    match case {
        "snapshot-name" => {
            state.custom_name = Some(TextComponent::plain("x".repeat((2 << 20) + 1)));
        }
        "snapshot-lock" => {
            state.lock = LockCode::new(ItemPredicate::new(
                Some(RegistryHolderSet::Direct(vec![
                    &*vanilla_items::STONE;
                    300_000
                ])),
                IntBounds::ANY,
                DataComponentMatchers::ANY,
            ));
        }
        "live-text" => {
            let mut item = ItemStack::new(&vanilla_items::STONE);
            item.set(CUSTOM_NAME, TextComponent::plain("x".repeat((2 << 20) + 1)));
            state.items[0] = item;
        }
        _ => {
            let mut compound = NbtCompound::new();
            compound.insert("huge", "x".repeat((2 << 20) + 1));
            let mut item = ItemStack::new(&vanilla_items::STONE);
            item.set(
                CUSTOM_DATA,
                CustomData::try_from_compound(compound).expect("custom data"),
            );
            state.items[0] = item;
        }
    }
    assert!(stand.apply_state_snapshot(state));
}

fn assert_live_contract(
    env: &mut JNIEnv<'_>,
    name: &JString<'_>,
    pos: BlockPos,
    stand: &BrewingStandBlockEntity,
) -> TestResult {
    let mut patch = DataComponentPatch::new();
    patch.set(CUSTOM_NAME, TextComponent::plain("raw zero"));
    patch.remove(ITEM_NAME);
    let mut raw = ItemStack::from_raw_parts(&vanilla_items::STONE, 0, patch);
    raw.set_opaque_nbt(Some("{opaque:1b}".into()));
    let mut state = stand.state_snapshot();
    state.items[0] = raw.clone();
    state.brew_time = 37;
    state.recipe_brew_time = 240;
    state.fuel = 11;
    state.custom_name = Some(TextComponent::plain("brewer"));
    assert!(stand.apply_state_snapshot(state));
    let identity = stand.identity();
    let bytes = getter(env, name, pos, None)?.ok_or("snapshot resolved")?;
    let decoded = decode_brewing_snapshot(&bytes).ok_or("snapshot decoded")?;
    assert_eq!(
        (
            decoded.identity,
            decoded.brew_time,
            decoded.recipe_brew_time,
            decoded.fuel
        ),
        (identity, 37, 240, 11)
    );
    assert_eq!(decoded.custom_name, Some(TextComponent::plain("brewer")));
    assert_eq!(decoded.items[0], raw);
    let bytes = getter(env, name, pos, Some((identity, 0)))?.ok_or("live item resolved")?;
    assert_eq!(read_brewing_item(&bytes), Some(raw));
    for slot in [
        Some((identity.wrapping_add(1), 0)),
        Some((identity, -1)),
        Some((identity, 5)),
    ] {
        assert!(getter(env, name, pos, slot)?.is_none());
    }
    Ok(())
}

fn assert_scalar_and_apply_allocations(
    env: &mut JNIEnv<'_>,
    name: &JString<'_>,
    pos: BlockPos,
    stand: &BrewingStandBlockEntity,
) -> TestResult {
    let state = env.new_string("minecraft:brewing_stand")?;
    let mut incoming = snapshot(vec![ItemStack::empty(); BREWING_STAND_SLOTS]);
    incoming.identity = stand.identity();
    let encoded = encode_brewing_snapshot(&incoming).ok_or("small incoming snapshot")?;
    let payload = env.byte_array_from_slice(&encoded)?;
    // The isolated child owns this tick thread for actual native mutation calls.
    *TICK_THREAD.lock() = Some(thread::current().id());
    let mut failures = Vec::new();
    for case in ["snapshot-name", "snapshot-lock", "snapshot-component"] {
        install_oversized(stand, case);
        let expected = stand.with_state(|view| format!("{}\u{001f}{}", view.brew_time, view.fuel));
        let scalar = allocation_counter::measure(|| {
            let value = env
                .call_static_method(
                    "foton/Native",
                    "brewingStandState",
                    "(Ljava/lang/String;III)Ljava/lang/String;",
                    &[
                        JValue::Object(name),
                        JValue::Int(pos.x()),
                        JValue::Int(pos.y()),
                        JValue::Int(pos.z()),
                    ],
                )
                .expect("actual scalar JNI call")
                .l()
                .expect("scalar result");
            let actual: String = env
                .get_string(&JString::from(value))
                .expect("scalar string")
                .into();
            assert_eq!(actual, expected);
        });
        eprintln!("task13 actual JNI scalar {case}: {scalar:?}");
        if scalar.bytes_max >= 1 << 20 {
            failures.push((case, "scalar", scalar.bytes_max));
        }
        let apply = allocation_counter::measure(|| {
            assert!(
                env.call_static_method(
                    "foton/Native",
                    "brewingStandApply",
                    "(Ljava/lang/String;IIILjava/lang/String;[BZZ)Z",
                    &[
                        JValue::Object(name),
                        JValue::Int(pos.x()),
                        JValue::Int(pos.y()),
                        JValue::Int(pos.z()),
                        JValue::Object(&state),
                        JValue::Object(&payload),
                        JValue::Bool(0),
                        JValue::Bool(0)
                    ],
                )
                .expect("actual apply JNI call")
                .z()
                .expect("apply boolean")
            );
        });
        eprintln!("task13 actual JNI apply {case}: {apply:?}");
        if apply.bytes_max >= 1 << 20 {
            failures.push((case, "apply", apply.bytes_max));
        }
        assert_eq!(getter(env, name, pos, None)?, Some(encoded.clone()));
    }
    assert!(
        failures.is_empty(),
        "scalar/apply cloned existing payloads: {failures:?}"
    );
    Ok(())
}

#[test]
fn brewing_union_actual_jni_getters_reject_without_deep_clones() -> TestResult {
    // One JVM per isolated process preserves the normal parallel test suite.
    const CHILD: &str = "FOTON_BREWING_JNI_ALLOCATION_CHILD";
    if env::var_os(CHILD).is_none() {
        let status = Command::new(env::current_exe()?)
            .args(["--exact", "natives::brewing_bridge_tests::brewing_union_jni::brewing_union_actual_jni_getters_reject_without_deep_clones", "--nocapture"])
            .env(CHILD, "1").status()?;
        assert!(status.success(), "actual JNI allocation child failed");
        return Ok(());
    }
    let (_storage, server) = live_test_server()?;
    let world = loaded_bridge_world();
    server
        .worlds
        .insert(world.key.clone(), Arc::clone(&world))?;
    server.attach_worlds();
    // Retain and drain every world even on a failed assertion.
    let _worlds: Vec<_> = server
        .worlds
        .snapshots()
        .iter()
        .map(|w| LoadedBridgeWorld::new(Arc::clone(w.world())))
        .collect();
    let scratch = tempfile::tempdir()?;
    let host = start_host(&scratch, &server)?;
    let mut env = host.vm.attach_current_thread()?;
    let name = env.new_string(world.key.to_string())?;
    let pos = BlockPos::new(3, 64, 3);
    assert!(world.set_block(
        pos,
        vanilla_blocks::BREWING_STAND.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let entity = world.get_block_entity(pos).ok_or("brewing stand")?;
    let stand = entity
        .downcast_ref::<BrewingStandBlockEntity>()
        .ok_or("brewing stand type")?;
    assert_live_contract(&mut env, &name, pos, stand)?;
    assert_scalar_and_apply_allocations(&mut env, &name, pos, stand)?;
    let mut failures = Vec::new();
    for case in [
        "snapshot-name",
        "snapshot-lock",
        "snapshot-component",
        "live-component",
        "live-text",
    ] {
        install_oversized(stand, case);
        let slot = case.starts_with("live").then_some((stand.identity(), 0));
        let stats = allocation_counter::measure(|| {
            assert!(
                getter(&mut env, &name, pos, slot)
                    .expect("actual JNI getter")
                    .is_none()
            );
        });
        eprintln!("union actual JNI {case}: {stats:?}");
        if stats.bytes_max >= 1 << 20 {
            failures.push((case, stats.bytes_max));
        }
    }
    host.disable_all()?;
    assert!(
        failures.is_empty(),
        "JNI getters cloned rejected state: {failures:?}"
    );
    Ok(())
}

fn assert_worker_failure_child(child_variable: &str) -> TestResult {
    // Capture the intentional worker panic; successful outer gates stay clean.
    let mut child = Command::new(env::current_exe()?)
            .args(["--exact", "natives::brewing_bridge_tests::brewing_union_jni::brewing_task13_teardown_exposes_actual_scheduling_failure", "--nocapture"])
            .env(child_variable, "1")
            .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            let output = child.wait_with_output()?;
            panic!(
                "actual worker teardown exceeded watchdog: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("task13 actual scheduling failure"),
        "actual scheduling injection did not run: {stderr}"
    );
    assert!(
        output.status.success(),
        "worker teardown child failed: {stderr}"
    );
    Ok(())
}

#[test]
fn brewing_task13_teardown_exposes_actual_scheduling_failure() -> TestResult {
    use std::sync::atomic::{AtomicBool, Ordering};
    const CHILD: &str = "FOTON_BREWING_WORKER_FAILURE_CHILD";
    static WORKER_RAN: AtomicBool = AtomicBool::new(false);
    if env::var_os(CHILD).is_none() {
        return assert_worker_failure_child(CHILD);
    }
    let world = loaded_bridge_world();
    let runtime = Arc::clone(&world.chunk_map.chunk_runtime);
    let tracker = world.chunk_map.task_tracker.clone();
    let weak = Arc::downgrade(&world);
    let caught = panic::catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(async { panic!("task13 caught test-thread assertion") });
    }));
    assert!(caught.is_err());
    assert_eq!(
        world.1.failures(),
        0,
        "caught test-thread panic attributed to worker"
    );
    let drained = Arc::new(AtomicBool::new(false));
    let drain_complete = Arc::clone(&drained);
    let alive = weak.clone();
    let cancel = world.chunk_map.cancel_token.clone();
    drop(tracker.spawn_on(
        async move {
            cancel.cancelled().await;
            sleep(Duration::from_millis(20)).await;
            assert!(
                alive.upgrade().is_some(),
                "world dropped before delayed worker drained"
            );
            drain_complete.store(true, Ordering::Release);
        },
        runtime.handle(),
    ));
    world
        .chunk_map
        .run_before_next_scheduling_epoch_for_test(|| {
            WORKER_RAN.store(true, Ordering::Release);
            panic!("task13 actual scheduling failure");
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !WORKER_RAN.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "scheduling injection timed out");
        world.chunk_map.advance_scheduling();
        thread::sleep(Duration::from_millis(1));
    }
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| drop(world)));
    // This uses the actual production worker's discarded JoinHandle, not a synthetic join.
    assert!(
        outcome.is_err(),
        "teardown silently accepted an actual tracked scheduling worker failure"
    );
    assert!(
        tracker.is_closed() && tracker.is_empty() && drained.load(Ordering::Acquire),
        "failure surfaced before draining"
    );
    assert!(
        weak.upgrade().is_none(),
        "world retained after completed teardown"
    );
    drop(runtime);

    assert_failure_during_unwind();
    Ok(())
}

fn assert_failure_during_unwind() {
    // A background failure must not cause a second destructor panic during an
    // already-failing test; its workers must still be drained.
    use std::sync::atomic::{AtomicBool, Ordering};
    static WORKER_RAN: AtomicBool = AtomicBool::new(false);
    let world = loaded_bridge_world();
    let tracker = world.chunk_map.task_tracker.clone();
    let weak = Arc::downgrade(&world);
    world
        .chunk_map
        .run_before_next_scheduling_epoch_for_test(|| {
            WORKER_RAN.store(true, Ordering::Release);
            panic!("task13 actual scheduling failure during unwind");
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !WORKER_RAN.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "unwind scheduling injection timed out"
        );
        world.chunk_map.advance_scheduling();
        thread::sleep(Duration::from_millis(1));
    }
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let _world = world;
        panic!("task13 original test failure");
    }));
    assert!(outcome.is_err());
    assert!(tracker.is_closed() && tracker.is_empty());
    assert!(weak.upgrade().is_none());
}

// Test-only observation belongs to fixture lifetime, not production task scheduling.
// Tokio tasks are identified by runtime; Rayon work is identified by pool threads.
mod worker_failures {
    use super::*;
    use foton_utils::locks::SyncMutex;
    use rustc_hash::FxHashMap;
    use std::{
        sync::{
            OnceLock, Weak,
            atomic::{AtomicUsize, Ordering},
        },
        thread::ThreadId,
    };

    type Counters = Vec<Weak<AtomicUsize>>;
    #[derive(Default)]
    struct Observers {
        runtimes: FxHashMap<RuntimeId, Counters>,
        pool_threads: FxHashMap<ThreadId, Counters>,
    }

    fn observers() -> &'static SyncMutex<Observers> {
        static OBSERVERS: OnceLock<SyncMutex<Observers>> = OnceLock::new();
        OBSERVERS.get_or_init(|| {
            let previous = panic::take_hook();
            panic::set_hook(Box::new(move |info| {
                {
                    let observers = observers().lock();
                    // Exclude caught test-thread assertions inside Runtime::block_on.
                    let runtime = try_id().and_then(|_| Handle::try_current().ok());
                    let counters = runtime
                        .as_ref()
                        .and_then(|runtime| observers.runtimes.get(&runtime.id()))
                        .or_else(|| observers.pool_threads.get(&thread::current().id()));
                    if let Some(counters) = counters {
                        for counter in counters.iter().filter_map(Weak::upgrade) {
                            counter.fetch_add(1, Ordering::AcqRel);
                        }
                    }
                }
                // Preserve normal diagnostics and never call arbitrary hooks under our lock.
                previous(info);
            }));
            SyncMutex::new(Observers::default())
        })
    }

    pub(in crate::natives::brewing_bridge_tests) struct WorkerFailureObserver {
        runtime: RuntimeId,
        pool_threads: Vec<ThreadId>,
        counter: Arc<AtomicUsize>,
    }

    impl WorkerFailureObserver {
        pub(in crate::natives::brewing_bridge_tests) fn new(world: &World) -> Self {
            Self::for_workers(
                &world.chunk_map.chunk_runtime,
                &world.chunk_map.generation_pool,
            )
        }

        pub(in crate::natives::brewing_bridge_tests) fn for_workers(
            runtime: &Runtime,
            pool: &rayon::ThreadPool,
        ) -> Self {
            let counter = Arc::new(AtomicUsize::new(0));
            let runtime = runtime.handle().id();
            let pool_threads = pool.broadcast(|_| thread::current().id());
            let mut observers = observers().lock();
            observers
                .runtimes
                .entry(runtime)
                .or_default()
                .push(Arc::downgrade(&counter));
            for thread in &pool_threads {
                observers
                    .pool_threads
                    .entry(*thread)
                    .or_default()
                    .push(Arc::downgrade(&counter));
            }
            Self {
                runtime,
                pool_threads,
                counter,
            }
        }

        pub(in crate::natives::brewing_bridge_tests) fn failures(&self) -> usize {
            self.counter.load(Ordering::Acquire)
        }
    }

    impl Drop for WorkerFailureObserver {
        fn drop(&mut self) {
            let mut observers = observers().lock();
            let own = Arc::downgrade(&self.counter);
            if let Some(counters) = observers.runtimes.get_mut(&self.runtime) {
                counters.retain(|counter| !counter.ptr_eq(&own));
                if counters.is_empty() {
                    observers.runtimes.remove(&self.runtime);
                }
            }
            for thread in &self.pool_threads {
                if let Some(counters) = observers.pool_threads.get_mut(thread) {
                    counters.retain(|counter| !counter.ptr_eq(&own));
                    if counters.is_empty() {
                        observers.pool_threads.remove(thread);
                    }
                }
            }
        }
    }
}
pub(super) use worker_failures::WorkerFailureObserver;
