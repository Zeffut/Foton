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

use std::ffi::{CString, NulError, c_void};
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Weak};

use foton_core::server::Server;
use jni::objects::JString;
use jni::sys::{JNI_OK, JNI_VERSION_1_8, JavaVM as RawJavaVm, JavaVMInitArgs, JavaVMOption};
use jni::{JavaVM, errors::Error as JniError};
use thiserror::Error;

mod forward;
mod natives;

/// The class the Java side exposes to this one.
const HOST_CLASS: &str = "foton/PluginHost";

/// The class whose methods Foton answers.
const NATIVE_CLASS: &str = "foton/Native";

/// Why a plugin host could not start, or could not do its job.
#[derive(Debug, Error)]
pub enum PluginHostError {
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
    /// Something went wrong on the Java side of the boundary.
    #[error("the plugin host failed: {0}")]
    Java(#[from] JniError),
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
    pub library_directory: Option<PathBuf>,
    /// Where plugin jars are found.
    pub plugin_directory: PathBuf,
}

impl PluginHostConfig {
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

    /// Everything a plugin is allowed to see, in the order it is searched.
    ///
    /// Spelled out rather than globbed with `dir/*`: the invocation API does
    /// not expand that the way the `java` launcher does, and what a plugin can
    /// reach is worth deciding on purpose in any case.
    fn class_path(&self) -> Result<String, PluginHostError> {
        if !self.api_jar.is_file() {
            return Err(PluginHostError::NoApiJar(self.api_jar.clone()));
        }
        let mut entries = vec![self.api_jar.to_string_lossy().into_owned()];
        if let Some(directory) = &self.library_directory {
            entries.extend(jars_in(directory));
        }
        Ok(entries.join(if cfg!(target_os = "windows") {
            ";"
        } else {
            ":"
        }))
    }
}

/// Every jar directly inside a directory, sorted so a classpath is reproducible.
fn jars_in(directory: &Path) -> Vec<String> {
    let Ok(entries) = read_dir(directory) else {
        return Vec::new();
    };
    let mut jars: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jar"))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    jars.sort();
    jars
}

/// A running Java runtime with Foton's plugin host inside it.
///
/// Dropping this does not stop the runtime: a JVM cannot be started twice in
/// one process, so tearing one down and expecting another would be a trap. Call
/// [`Self::disable_all`] to stop the plugins.
pub struct PluginHost {
    vm: Arc<JavaVM>,
    /// The loaded runtime, kept alive because the VM's code lives in it.
    ///
    /// Dropping this would unmap the library the JVM is executing from.
    _runtime: libloading::Library,
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
        let class_path = config.class_path()?;
        let library = config.runtime_library();

        // SAFETY: `Library::new` maps a shared object, and `get` looks a symbol
        // up in it. Both are unsafe because the file could be anything; here it
        // is a path an operator configured, and one that is not a Java runtime
        // fails to load or fails to export the symbol rather than doing
        // something surprising. `JNI_CreateJavaVM` is called with arguments
        // built immediately below and valid for the whole call.
        let (vm, runtime) = unsafe {
            let runtime = libloading::Library::new(&library)
                .map_err(|error| PluginHostError::NoRuntime(library.clone(), error.to_string()))?;
            let create: libloading::Symbol<'_, CreateJavaVm> =
                runtime
                    .get(b"JNI_CreateJavaVM\0")
                    .map_err(|_| PluginHostError::NotARuntime(library.clone()))?;

            let class_path = CString::new(format!("-Djava.class.path={class_path}"))?;
            let mut options = [JavaVMOption {
                optionString: class_path.as_ptr().cast_mut(),
                extraInfo: ptr::null_mut(),
            }];
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
            // SAFETY: the pointer came from a `JNI_CreateJavaVM` that reported
            // success, which is exactly what `from_raw` requires.
            let vm = JavaVM::from_raw(raw_vm)?;
            (vm, runtime)
        };

        natives::bind(server.clone());
        let host = Self {
            vm: Arc::new(vm),
            _runtime: runtime,
        };
        host.register_natives()?;
        // Foton's events reach plugins only once this is done, which is why it
        // happens before any plugin is loaded rather than after.
        if let Some(server) = server.upgrade() {
            forward::subscribe(&server, Arc::clone(&host.vm));
        }
        Ok(host)
    }

    /// Tells the runtime which Rust function answers each declared native.
    ///
    /// Done once, at start. A plugin that reaches Foton before this would get a
    /// `atalError` from the JVM rather than a wrong answer, which is the right
    /// way round but not a thing to rely on.
    fn register_natives(&self) -> Result<(), PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        let class = env.find_class(NATIVE_CLASS)?;
        env.register_native_methods(&class, &natives::bindings())?;
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

    /// Stops delivering Foton's events to plugins.
    ///
    /// Separate from [`Self::disable_all`] on purpose: a plugin being disabled
    /// should stop hearing about the world before it is asked to shut down, or
    /// its last moments are spent handling events for a server it is leaving.
    pub fn unsubscribe(server: &Arc<Server>) {
        forward::unsubscribe(server);
    }

    /// Disables every loaded plugin, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the Java side cannot be reached at all.
    pub fn disable_all(&self) -> Result<(), PluginHostError> {
        let mut env = self.vm.attach_current_thread()?;
        env.call_static_method(HOST_CLASS, "disableAll", "()V", &[])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
