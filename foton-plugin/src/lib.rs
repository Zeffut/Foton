//! Hosting Bukkit-family plugins, which are JVM artifacts.
//!
//! A plugin is a jar compiled against `org.bukkit`. Running one means running a
//! JVM, and this crate is the only place in Foton that knows that. Everything
//! it exposes is inert until [`PluginHost::start`] is called, and nothing calls
//! it unless an operator asked for plugins.
//!
//! **The JVM is loaded, never linked.** Linking `libjvm` would make every Foton
//! server need a Java installation to start, plugins or not, which is exactly
//! the cost `design/plugin-compatibility.md` says an operator who runs no
//! plugins must not pay. So the entry point is resolved out of a shared library
//! at the moment it is wanted, and a server with no plugins never opens it.
//!
//! Class loading, reflection and the plugin lifecycle live on the Java side, in
//! `plugin-api/src/foton/`. That is not a shortcut: those are the things a JVM
//! is for, and writing them through JNI would be three times the code doing the
//! same job less clearly.

// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. The lint
// is scoped to `not(test)` on purpose -- a panicking test is how a test reports
// a failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic, clippy::unreachable, clippy::todo))]

use std::collections::BTreeSet;
use std::ffi::{CString, NulError, c_void};
use std::fs::{read, read_dir, symlink_metadata};
use std::io;
use std::mem;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Weak};
use std::time::Duration;

use foton_core::server::Server;
use foton_protocol::packet_traits::PacketTranslation;
use foton_utils::locks::SyncMutex;
use jni::objects::JString;
use jni::sys::{JNI_OK, JNI_VERSION_1_8, JavaVM as RawJavaVm, JavaVMInitArgs, JavaVMOption};
use jni::{JavaVM, errors::Error as JniError};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    sync::Semaphore,
    task::spawn_blocking,
    time::{Instant, timeout_at},
};

mod enchantment;
mod forward;
mod item_bridge;
mod item_components;
mod natives;
#[cfg(test)]
mod natives_live_api_tests;
mod packet_tap;
mod relay;
mod scoreboard_natives;
mod via;

/// The class the Java side exposes to this one.
const HOST_CLASS: &str = "foton/PluginHost";

/// The class whose methods Foton answers.
const NATIVE_CLASS: &str = "foton/Native";

/// The JVM lives until process exit, so the library containing its executing
/// code must remain mapped for that entire lifetime too.
static JVM_RUNTIMES: LazyLock<SyncMutex<Vec<libloading::Library>>> =
    LazyLock::new(|| SyncMutex::new(Vec::new()));

/// Limits connection setup calls that may be blocked inside a plugin's Netty initializer.
static VIA_OPEN_WORKERS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(32)));
const VIA_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const LIBRARY_MANIFEST: &str = include_str!("../../plugin-api/lib/manifest.txt");

/// Why a plugin host could not start, or could not do its job.
#[derive(Debug, Error)]
pub enum PluginHostError {
    /// A statically linked (musl) binary has no dynamic loader to map a JVM with.
    #[error(
        "this Foton build is statically linked (musl) and cannot load a Java runtime, so it \
         cannot run plugins; use the glibc build (foton-linux-x86_64-gnu or \
         foton-linux-aarch64-gnu from the release assets), or unset FOTON_PLUGIN_DIRECTORY to \
         run without plugins"
    )]
    StaticBuild,
    /// No Java runtime was found where the configuration said one would be.
    #[error("no Java runtime at {0}: {1}")]
    NoRuntime(PathBuf, String),
    /// The library loaded but did not export the entry point every JVM has.
    #[error("{0} is not a Java runtime: it exports no JNI_CreateJavaVM")]
    NotARuntime(PathBuf),
    /// The runtime refused to start, and said so with a JNI status code.
    #[error("the Java runtime refused to start (JNI status {0})")]
    RuntimeRefused(i32),
    /// A path could not be handed to C because it contains a zero byte.
    #[error("a path cannot be passed to the Java runtime: {0}")]
    UnusablePath(#[from] NulError),
    /// The API jar the plugins are loaded against is not where it should be.
    #[error("the plugin API jar is missing at {0}; run dev/build-plugin-api.sh")]
    NoApiJar(PathBuf),
    /// A configured classpath directory could not be read completely.
    #[error("could not read plugin library directory {0}: {1}")]
    LibraryDirectory(PathBuf, io::Error),
    /// The pinned dependencies referenced by public API signatures are absent.
    #[error("the plugin API dependency directory is unavailable at {0}: {1}")]
    NoLibraryDirectory(PathBuf, #[source] io::Error),
    /// One expected runtime jar is absent from the configured directory.
    #[error("pinned runtime library is missing at {0}")]
    MissingRuntimeLibrary(PathBuf),
    /// A runtime jar is present without an entry in the authoritative manifest.
    #[error("runtime library {0} is not in the pinned manifest")]
    ExtraRuntimeLibrary(PathBuf),
    /// A runtime jar's bytes differ from the authoritative manifest.
    #[error("runtime library digest mismatch at {path}: expected {expected}, got {actual}")]
    RuntimeLibraryDigest {
        /// The runtime jar whose bytes did not match.
        path: PathBuf,
        /// The authoritative manifest digest.
        expected: String,
        /// The digest computed from the configured runtime jar.
        actual: String,
    },
    /// The checked-in manifest itself cannot describe an exact pinned set.
    #[error("the pinned runtime library manifest is invalid: {0}")]
    InvalidLibraryManifest(String),
    /// A runtime jar could not be read while validating its digest.
    #[error("runtime library cannot be read at {0}: {1}")]
    RuntimeLibraryIo(PathBuf, #[source] io::Error),
    /// Something went wrong on the Java side of the boundary.
    #[error("the plugin host failed: {0}")]
    Java(#[from] JniError),
    /// A bounded JVM worker could not complete a connection operation.
    #[error("the plugin network bridge failed: {0}")]
    JavaTask(String),
}

/// Where the pieces a plugin host needs are.
#[derive(Debug, Clone)]
pub struct PluginHostConfig {
    /// The directory of a JDK or JRE, as `JAVA_HOME` would name it.
    pub java_home: PathBuf,
    /// The jar `dev/build-plugin-api.sh` produces.
    pub api_jar: PathBuf,
    /// Jars a plugin expects the server to provide and does not ship.
    ///
    /// Not an afterthought: a plugin compiled against a Paper server assumes
    /// Gson and the rest are simply there, and the first real plugin tried
    /// against this failed on exactly that.
    ///
    /// Several, searched in order: the libraries Paper declares for its API,
    /// and the ones its server jar carries at run time -- JDBC drivers among
    /// them, which plugins use without shipping.
    pub library_directories: Vec<PathBuf>,
    /// Where plugin jars are found.
    pub plugin_directory: PathBuf,
    /// Maximum retained item snapshots and independent in-flight candidates.
    /// This is a cardinality limit, not a bound on retained bytes.
    pub item_snapshot_limit: NonZeroUsize,
}

impl PluginHostConfig {
    /// The pinned dependencies referenced by the public plugin API.
    #[must_use]
    pub fn resolved_library_directory(&self) -> PathBuf {
        self.library_directories
            .first()
            .cloned()
            .unwrap_or_else(|| {
                self.api_jar
                    .parent()
                    .and_then(Path::parent)
                    .unwrap_or_else(|| Path::new("."))
                    .join("lib")
            })
    }
    /// Initial operational limit; unrelated to Vanilla inventory sizes.
    pub const DEFAULT_ITEM_SNAPSHOT_LIMIT: NonZeroUsize = match NonZeroUsize::new(4096) {
        Some(limit) => limit,
        None => NonZeroUsize::MIN,
    };

    /// The shared library holding the runtime, for this platform's layout.
    fn runtime_library(&self) -> PathBuf {
        let name = if cfg!(target_os = "windows") {
            "bin/server/jvm.dll"
        } else if cfg!(target_os = "macos") {
            "lib/server/libjvm.dylib"
        } else {
            "lib/server/libjvm.so"
        };
        self.java_home.join(name)
    }

    /// Plugins Foton ships because their upstream builds cannot run on it.
    ///
    /// Beside the API jar, because they are built with it and against it:
    /// `dev/build-packetevents.sh` writes there. The host loads them before
    /// the plugin directory, and one of theirs wins a name collision.
    fn bundled_directory(&self) -> PathBuf {
        self.api_jar
            .parent()
            .map_or_else(|| PathBuf::from("bundled"), |parent| parent.join("bundled"))
    }

    /// Everything a plugin is allowed to see, in the order it is searched.
    ///
    /// Spelled out rather than globbed with `dir/*`: the invocation API does
    /// not expand that the way the `java` launcher does, and what a plugin can
    /// reach is worth deciding on purpose in any case.
    fn class_path(&self) -> Result<String, PluginHostError> {
        if !is_regular_file(&self.api_jar) {
            return Err(PluginHostError::NoApiJar(self.api_jar.clone()));
        }
        let mut entries = vec![jvm_path(&self.api_jar)];
        entries.extend(validated_jars_in(&self.resolved_library_directory())?);
        let pinned: BTreeSet<String> = pinned_libraries()?
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        for directory in self.library_directories.iter().skip(1) {
            for jar in jars_in(directory)? {
                // Pinned API/runtime types always come from the verified first directory.
                if Path::new(&jar)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_none_or(|name| !pinned.contains(name))
                {
                    entries.push(jar);
                }
            }
        }
        Ok(entries.join(if cfg!(target_os = "windows") {
            ";"
        } else {
            ":"
        }))
    }
}

/// Every jar directly inside a directory, sorted so a classpath is reproducible.
fn jars_in(directory: &Path) -> Result<Vec<String>, PluginHostError> {
    let entries = read_dir(directory)
        .map_err(|error| PluginHostError::LibraryDirectory(directory.to_owned(), error))?;
    let mut jars = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| PluginHostError::LibraryDirectory(directory.to_owned(), error))?;
        let kind = entry
            .file_type()
            .map_err(|error| PluginHostError::LibraryDirectory(directory.to_owned(), error))?;
        let path = entry.path();
        if kind.is_file() && path.extension().is_some_and(|extension| extension == "jar") {
            jars.push(jvm_path(&path));
        }
    }
    jars.sort();
    Ok(jars)
}

fn jvm_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// True only for a direct regular file. Runtime classpaths never follow links:
/// otherwise a bundle validated inside one directory can be swapped to point
/// outside it before the JVM opens the jar.
fn is_regular_file(path: &Path) -> bool {
    symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn reject_nested_jars(directory: &Path) -> Result<(), PluginHostError> {
    for entry in read_dir(directory)
        .map_err(|error| PluginHostError::NoLibraryDirectory(directory.to_owned(), error))?
    {
        let entry = entry
            .map_err(|error| PluginHostError::NoLibraryDirectory(directory.to_owned(), error))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| PluginHostError::RuntimeLibraryIo(path.clone(), error))?;
        if kind.is_symlink() || path.extension().is_some_and(|extension| extension == "jar") {
            return Err(PluginHostError::ExtraRuntimeLibrary(path));
        }
        if kind.is_dir() {
            reject_nested_jars(&path)?;
        }
    }
    Ok(())
}

fn validated_jars_in(directory: &Path) -> Result<Vec<String>, PluginHostError> {
    let entries = read_dir(directory)
        .map_err(|error| PluginHostError::NoLibraryDirectory(directory.to_owned(), error))?;
    let manifest = pinned_libraries()?;
    let expected_names: BTreeSet<String> =
        manifest.iter().map(|(name, _)| name.to_owned()).collect();
    let mut found_names = BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|error| PluginHostError::NoLibraryDirectory(directory.to_owned(), error))?
            .path();
        if symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_dir()) {
            reject_nested_jars(&path)?;
        }
        if path.extension().is_some_and(|extension| extension == "jar") {
            if !is_regular_file(&path) {
                return Err(PluginHostError::ExtraRuntimeLibrary(path));
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return Err(PluginHostError::ExtraRuntimeLibrary(path));
            };
            if !expected_names.contains(name) {
                return Err(PluginHostError::ExtraRuntimeLibrary(path));
            }
            found_names.insert(name.to_owned());
        }
    }

    let mut jars = Vec::with_capacity(manifest.len());
    for (name, expected) in manifest {
        let path = directory.join(&name);
        if !found_names.contains(&name) {
            return Err(PluginHostError::MissingRuntimeLibrary(path));
        }
        let bytes = match read(&path) {
            Ok(bytes) => bytes,
            Err(error) => return Err(PluginHostError::RuntimeLibraryIo(path, error)),
        };
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(PluginHostError::RuntimeLibraryDigest {
                path,
                expected,
                actual,
            });
        }
        jars.push(jvm_path(&path));
    }
    Ok(jars)
}

fn pinned_libraries() -> Result<Vec<(String, String)>, PluginHostError> {
    let mut libraries = Vec::new();
    let mut names = BTreeSet::new();
    for (index, line) in LIBRARY_MANIFEST.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 4 || fields[3].len() != 64 {
            return Err(PluginHostError::InvalidLibraryManifest(format!(
                "line {} has the wrong shape",
                index + 1
            )));
        }
        let name = format!("{}-{}.jar", fields[1], fields[2]);
        if !names.insert(name.clone()) {
            return Err(PluginHostError::InvalidLibraryManifest(format!(
                "duplicate filename {name}"
            )));
        }
        libraries.push((name, fields[3].to_owned()));
    }
    if libraries.is_empty() {
        return Err(PluginHostError::InvalidLibraryManifest(
            "no libraries are pinned".to_owned(),
        ));
    }
    Ok(libraries)
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(digest.len() * 2);
    for byte in digest {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

/// A running Java runtime with Foton's plugin host inside it.
///
/// Dropping this does not stop the runtime: a JVM cannot be started twice in
/// one process, so tearing one down and expecting another would be a trap. Call
/// [`Self::disable_all`] to stop the plugins.
pub struct PluginHost {
    vm: Arc<JavaVM>,
    lifecycle: HostLifecycle<Server>,
}

/// The host-side association whose lifecycle must not keep a server alive.
///
/// `T` is generic only so the ownership and exactly-once behavior can be
/// exercised without constructing a complete server or JVM.
struct HostLifecycle<T> {
    server: SyncMutex<Weak<T>>,
    shutdown: AtomicBool,
}

impl<T> HostLifecycle<T> {
    const fn new() -> Self {
        Self {
            server: SyncMutex::new(Weak::new()),
            shutdown: AtomicBool::new(false),
        }
    }

    fn bind<U, S>(&self, server: &Weak<T>, unsubscribe: U, subscribe: S)
    where
        U: FnOnce(&Arc<T>),
        S: FnOnce(&Arc<T>),
    {
        let mut current = self.server.lock();
        if self.shutdown.load(Ordering::Acquire) {
            return;
        }
        if let Some(previous) = current.upgrade() {
            unsubscribe(&previous);
        }
        *current = server.clone();
        if let Some(server) = current.upgrade() {
            subscribe(&server);
        }
    }

    fn shutdown<U, D, E>(&self, unsubscribe: U, disable: D) -> Result<(), E>
    where
        U: FnOnce(&Arc<T>),
        D: FnOnce() -> Result<(), E>,
    {
        if self.shutdown.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        if let Some(server) = mem::take(&mut *self.server.lock()).upgrade() {
            unsubscribe(&server);
        }
        disable()
    }
}

/// The entry point every Java runtime exports.
type CreateJavaVm =
    unsafe extern "system" fn(*mut *mut RawJavaVm, *mut *mut c_void, *mut c_void) -> i32;

impl PluginHost {
    /// Starts a Java runtime and returns a host that can load plugins.
    ///
    /// # Errors
    ///
    /// Returns an error when no runtime is where the configuration said, when
    /// the API jar has not been built, or when the runtime refuses to start.
    pub fn start(
        config: &PluginHostConfig,
        server: &Weak<Server>,
    ) -> Result<Self, PluginHostError> {
        // Without this the musl loader fails with the bare "Dynamic loading
        // not supported", which names neither the cause nor the way out.
        if cfg!(target_env = "musl") {
            return Err(PluginHostError::StaticBuild);
        }
        let class_path = config.class_path()?;
        let library = config.runtime_library();

        // SAFETY: `Library::new` maps a shared object, and `get` looks a symbol
        // up in it. Both are unsafe because the file could be anything; here it
        // is a path an operator configured, and one that is not a Java runtime
        // fails to load or fails to export the symbol rather than doing
        // something surprising. `JNI_CreateJavaVM` is called with arguments
        // built immediately below and valid for the whole call.
        let vm = unsafe {
            let runtime = libloading::Library::new(&library)
                .map_err(|error| PluginHostError::NoRuntime(library.clone(), error.to_string()))?;
            let create: libloading::Symbol<'_, CreateJavaVm> =
                runtime
                    .get(b"JNI_CreateJavaVM\0")
                    .map_err(|_| PluginHostError::NotARuntime(library.clone()))?;

            let class_path = CString::new(format!("-Djava.class.path={class_path}"))?;
            let plugins_directory = CString::new(format!(
                "-Dfoton.plugins-directory={}",
                config.plugin_directory.display()
            ))?;
            let bundled_plugins = CString::new(format!(
                "-Dfoton.bundled-plugins={}",
                config.bundled_directory().display()
            ))?;
            // The process belongs to Foton, and so do its signals. Without
            // this, HotSpot installs its own SIGTERM and SIGINT handlers and
            // answers them by exiting the process on the spot: no world is
            // saved, no plugin is disabled, and the operator's `docker stop`
            // loses whatever the last autosave missed.
            let reduced_signals = CString::new("-Xrs")?;
            let mut options = [
                JavaVMOption {
                    optionString: class_path.as_ptr().cast_mut(),
                    extraInfo: ptr::null_mut(),
                },
                JavaVMOption {
                    optionString: plugins_directory.as_ptr().cast_mut(),
                    extraInfo: ptr::null_mut(),
                },
                JavaVMOption {
                    optionString: bundled_plugins.as_ptr().cast_mut(),
                    extraInfo: ptr::null_mut(),
                },
                JavaVMOption {
                    optionString: reduced_signals.as_ptr().cast_mut(),
                    extraInfo: ptr::null_mut(),
                },
            ];
            let mut args = JavaVMInitArgs {
                version: JNI_VERSION_1_8,
                nOptions: options.len().try_into().unwrap_or(0),
                options: options.as_mut_ptr(),
                ignoreUnrecognized: 0,
            };

            let mut raw_vm: *mut RawJavaVm = ptr::null_mut();
            let mut raw_env: *mut c_void = ptr::null_mut();
            let status = create(&raw mut raw_vm, &raw mut raw_env, (&raw mut args).cast());
            if status != JNI_OK {
                return Err(PluginHostError::RuntimeRefused(status));
            }
            // Pin the runtime before another fallible JNI operation can return:
            // a successful create call has already started process-lifetime
            // JVM threads that may execute code from this library.
            JVM_RUNTIMES.lock().push(runtime);
            // SAFETY: the pointer came from a `JNI_CreateJavaVM` that reported
            // success, which is exactly what `from_raw` requires.
            JavaVM::from_raw(raw_vm)?
        };

        item_bridge::initialize(config.item_snapshot_limit);
        let host = Self {
            vm: Arc::new(vm),
            lifecycle: HostLifecycle::new(),
        };
        if let Err(error) = host.register_natives() {
            if let Ok(store) = item_bridge::store() {
                store.close();
            }
            eprintln!("native registration failed: {error:?}");
            return Err(error);
        }
        let binding = host.vm.attach_current_thread().and_then(|mut env| {
            env.call_static_method(HOST_CLASS, "bindItemBridge", "()V", &[])
                .map(|_| ())
        });
        if let Err(error) = binding {
            if let Ok(store) = item_bridge::store() {
                store.close();
            }
            return Err(error.into());
        }
        host.bind_server(server);
        Ok(host)
    }

    /// Binds the already-running host to the fully initialized Foton server.
    ///
    /// This is deliberately separate from [`Self::start`]: Java plugins must
    /// receive `onLoad` before core registries are initialized, while natives
    /// that access gameplay state must only observe the completed server.
    pub fn bind_server(&self, server: &Weak<Server>) {
        natives::bind(server.clone());
        self.lifecycle.bind(server, forward::unsubscribe, |server| {
            forward::subscribe(server, Arc::clone(&self.vm));
        });
    }

    /// Opens a per-connection Via pipeline when a plugin registered Paper's hook.
    pub async fn open_packet_translation(
        &self,
    ) -> Result<Option<PacketTranslation>, PluginHostError> {
        let deadline = Instant::now() + VIA_OPEN_TIMEOUT;
        let worker = timeout_at(deadline, Arc::clone(&VIA_OPEN_WORKERS).acquire_owned())
            .await
            .map_err(|_| PluginHostError::JavaTask("opening a Via channel timed out".to_owned()))?
            .map_err(|_| PluginHostError::JavaTask("Via worker pool is closed".to_owned()))?;
        let vm = Arc::clone(&self.vm);
        let task = spawn_blocking(move || {
            let _worker = worker;
            via::ViaTranslator::open(vm)
        });
        timeout_at(deadline, task)
            .await
            .map_err(|_| PluginHostError::JavaTask("opening a Via channel timed out".to_owned()))?
            .map_err(|error| PluginHostError::JavaTask(error.to_string()))?
            .map_err(|error| PluginHostError::JavaTask(error.to_string()))
    }

    /// Tells the runtime which Rust function answers each declared native.
    ///
    /// Done once, at start. A plugin that reaches Foton before this would get a
    /// `atalError` from the JVM rather than a wrong answer, which is the right
    /// way round but not a thing to rely on.
    fn register_natives(&self) -> Result<(), PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        let class = env.find_class(NATIVE_CLASS)?;
        if let Err(error) = env.register_native_methods(&class, &natives::bindings()) {
            let _ = env.exception_describe();
            return Err(error.into());
        }
        Ok(())
    }

    /// Loads and enables every plugin in the configured directory.
    ///
    /// Returns how many were enabled. A plugin that fails is reported by the
    /// host and skipped: one bad jar must not take the others with it.
    ///
    /// # Errors
    ///
    /// Returns an error when the Java side cannot be reached at all.
    pub fn load_all(&self, directory: &Path) -> Result<i32, PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        let directory = env.new_string(directory.to_string_lossy().as_ref())?;
        let enabled = env
            .call_static_method(
                HOST_CLASS,
                "loadAll",
                "(Ljava/lang/String;)I",
                &[(&directory).into()],
            )?
            .i()?;
        Ok(enabled)
    }

    /// Loads plugin classes and invokes only their Bukkit `onLoad` phase.
    pub fn load_all_on_load(&self, directory: &Path) -> Result<i32, PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        let directory = env.new_string(directory.to_string_lossy().as_ref())?;
        Ok(env
            .call_static_method(
                HOST_CLASS,
                "loadAllOnLoad",
                "(Ljava/lang/String;)I",
                &[(&directory).into()],
            )?
            .i()?)
    }

    /// Invokes `onEnable` for all plugins loaded during the first phase.
    pub fn enable_all(&self) -> Result<i32, PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        Ok(env
            .call_static_method(HOST_CLASS, "enableAll", "()I", &[])?
            .i()?)
    }

    /// Asks the Java side what it thinks the server is called.
    ///
    /// Exists for the bridge test and for a first-run diagnostic: it is the
    /// shortest round trip through the seam, so an answer of anything but
    /// Foton's own name means the seam is wrong rather than the plugin.
    ///
    /// # Errors
    ///
    /// Returns an error when the Java side cannot be reached at all.
    pub fn server_name_from_java(&self) -> Result<String, PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        let value = env
            .call_static_method(NATIVE_CLASS, "serverName", "()Ljava/lang/String;", &[])?
            .l()?;
        let value: JString<'_> = value.into();
        Ok(env.get_string(&value)?.into())
    }

    /// Asks the Java API whether this caller is on Foton's game-tick thread.
    ///
    /// This small diagnostic crosses the same native boundary plugins use, so
    /// its integration test catches a Java declaration and JNI descriptor
    /// drifting apart.
    pub fn is_primary_thread_from_java(&self) -> Result<bool, PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        Ok(env
            .call_static_method(NATIVE_CLASS, "isPrimaryThread", "()Z", &[])?
            .z()?)
    }

    /// Stops Rust event delivery, then disables every loaded plugin newest first.
    ///
    /// Separate from [`Self::disable_all`] on purpose: a plugin being disabled
    /// should stop hearing about the world before it is asked to shut down, or
    /// its last moments are spent handling events for a server it is leaving.
    pub fn unsubscribe(server: &Arc<Server>) {
        forward::unsubscribe(server);
    }

    /// Marks an actual server shutdown before connections and plugins are drained.
    pub fn mark_stopping(&self) -> Result<(), PluginHostError> {
        let result = (|| {
            let mut env = self.vm.attach_current_thread()?;
            env.call_static_method(HOST_CLASS, "markStopping", "()V", &[])?;
            Ok(())
        })();
        if result.is_err() {
            Self::seal_item_operations();
        }
        result
    }

    /// Disables every loaded plugin, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the Java side cannot be reached at all.
    pub fn disable_all(&self) -> Result<(), PluginHostError> {
        let result = self
            .lifecycle
            .shutdown(forward::unsubscribe, || self.disable_java());
        if result.is_err() {
            Self::seal_item_operations();
        }
        result
    }

    fn disable_java(&self) -> Result<(), PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        env.call_static_method(HOST_CLASS, "disableAll", "()V", &[])?;
        Ok(())
    }

    /// Failure fallback: prevent further item work even when Java teardown failed.
    /// This does not make plugin teardown successful or synchronously drain work.
    pub fn seal_item_operations() {
        if let Ok(store) = item_bridge::store() {
            store.close();
        }
    }

    /// Outer async shutdown barrier, required before external world teardown.
    /// Java must first request terminal close after its invocation drain; on a
    /// failed handshake the caller must seal admission and report that failure.
    pub async fn drain_item_operations() {
        if let Ok(store) = item_bridge::store() {
            store.drained().await;
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod class_path_file_tests {
    #[cfg(unix)]
    use super::is_regular_file;
    use super::jars_in;
    use std::fs::{create_dir, write};

    #[test]
    fn classpath_uses_only_direct_regular_jars() {
        let temporary = tempfile::tempdir().expect("temporary classpath directory");
        let regular = temporary.path().join("regular.jar");
        write(&regular, b"jar").expect("regular test jar");
        create_dir(temporary.path().join("directory.jar")).expect("jar-shaped directory");

        assert_eq!(
            jars_in(temporary.path()).expect("readable classpath directory"),
            vec![regular.to_string_lossy().replace('\\', "/")]
        );
    }

    #[test]
    fn missing_classpath_directory_is_an_error() {
        let temporary = tempfile::tempdir().expect("temporary classpath parent");
        let missing = temporary.path().join("missing");

        assert!(jars_in(&missing).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn classpath_rejects_jar_symlinks() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary classpath directory");
        let target = temporary.path().join("target.bin");
        let link = temporary.path().join("linked.jar");
        write(&target, b"jar").expect("symlink target");
        symlink(&target, &link).expect("jar symlink");

        assert!(!is_regular_file(&link));
        assert!(
            jars_in(temporary.path())
                .expect("readable classpath directory")
                .is_empty()
        );
    }
}
