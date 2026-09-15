//! Checks on the plugin host.
//!
//! A JVM cannot be started twice in one process, so the tests that need one
//! are driven from a single place and the rest assert on what can be decided
//! without starting anything.

use std::fs::{create_dir_all, write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use foton_core::event::{EventBus, ServerTickEvent};
use foton_utils::Identifier;

use super::{HostLifecycle, PluginHostConfig, PluginHostError};

fn config() -> PluginHostConfig {
    PluginHostConfig {
        java_home: PathBuf::from("/nowhere"),
        api_jar: PathBuf::from("/nowhere/foton-plugin-api.jar"),
        library_directory: None,
        plugin_directory: PathBuf::from("/nowhere/plugins"),
    }
}

/// An API jar moved as one unit with its pinned dependencies keeps working
/// without another hidden configuration value.
#[test]
fn resolved_library_directory_defaults_to_api_sibling_lib() {
    let config = PluginHostConfig {
        api_jar: PathBuf::from("/server/plugin-api/build/foton-plugin-api.jar"),
        ..config()
    };

    assert_eq!(
        config.resolved_library_directory(),
        PathBuf::from("/server/plugin-api/lib")
    );
}

/// Operators can place the pinned runtime jars elsewhere without the default
/// derived from the API jar taking precedence.
#[test]
fn resolved_library_directory_prefers_the_explicit_override() {
    let config = PluginHostConfig {
        api_jar: PathBuf::from("/server/plugin-api/build/foton-plugin-api.jar"),
        library_directory: Some(PathBuf::from("/srv/foton/runtime-libraries")),
        ..config()
    };

    assert_eq!(
        config.resolved_library_directory(),
        PathBuf::from("/srv/foton/runtime-libraries")
    );
}

/// A missing API jar is named, not discovered as a class-loading failure later.
///
/// The jar is produced by `dev/build-plugin-api.sh`, which a first-time
/// operator has no reason to have run. Finding out through a
/// `NoClassDefFoundError` deep inside someone else's plugin is the worst
/// available way to learn that.
#[test]
fn a_missing_api_jar_is_reported_before_anything_starts() {
    let Err(error) = PluginHost::start(&config(), &Weak::new()) else {
        panic!("nothing is at /nowhere, so nothing should have started");
    };

    assert!(
        matches!(error, PluginHostError::NoApiJar(_)),
        "expected the missing jar to be named, got {error}"
    );
    assert!(
        error.to_string().contains("dev/build-plugin-api.sh"),
        "the error should say how to produce it: {error}"
    );
}

/// The class path is built in a fixed order, so two runs load the same code.
///
/// A directory listing is not ordered, and a plugin that resolves a class from
/// whichever jar happened to come first would work or not depending on the
/// filesystem's mood.
#[test]
fn the_class_path_is_ordered() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let jar = directory.path().join("api.jar");
    write(&jar, b"not really a jar").expect("the fixture should write");
    for name in ["zebra.jar", "alpha.jar", "middle.jar", "ignored.txt"] {
        write(directory.path().join(name), b"x").expect("the fixture should write");
    }

    let config = PluginHostConfig {
        api_jar: jar,
        library_directory: Some(directory.path().to_owned()),
        ..config()
    };
    let class_path = config.class_path().expect("the jar exists");

    let names: Vec<&str> = class_path
        .split(if cfg!(target_os = "windows") {
            ';'
        } else {
            ':'
        })
        .filter_map(|entry| entry.rsplit('/').next())
        .collect();
    assert_eq!(
        names,
        ["api.jar", "alpha.jar", "api.jar", "middle.jar", "zebra.jar"],
        "the API jar leads, then the libraries in a fixed order"
    );
    assert!(
        !class_path.contains("ignored.txt"),
        "only jars belong on a class path: {class_path}"
    );
}

/// Public API signatures refer to classes in the pinned library set, so an
/// absent directory is a startup error rather than a later linkage surprise.
#[test]
fn a_missing_runtime_library_directory_is_reported_before_startup() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let api_jar = directory.path().join("build/foton-plugin-api.jar");
    create_dir_all(api_jar.parent().expect("the API jar has a parent"))
        .expect("the fixture build directory should exist");
    write(&api_jar, b"not really a jar").expect("the fixture should write");

    let config = PluginHostConfig {
        api_jar,
        ..config()
    };
    let error = config
        .class_path()
        .expect_err("the derived sibling lib directory is absent");

    assert!(
        matches!(error, PluginHostError::NoLibraryDirectory(..)),
        "expected the missing runtime library directory, got {error}"
    );
    assert!(
        error
            .to_string()
            .contains(directory.path().join("lib").to_string_lossy().as_ref()),
        "the startup error should name the missing directory: {error}"
    );
}

use super::PluginHost;

/// Rebinding must replace the owner's complete subscription set without
/// retaining the server through an `Arc` cycle.
#[test]
fn repeated_binding_does_not_duplicate_owner_subscriptions_and_is_weak() {
    let lifecycle = HostLifecycle::new();
    let events = Arc::new(EventBus::new());
    let owner = Identifier::from_foton("plugins");
    let resets = AtomicUsize::new(0);
    let subscriptions = AtomicUsize::new(0);

    for _ in 0..2 {
        lifecycle.bind(
            &Arc::downgrade(&events),
            |events| {
                resets.fetch_add(1, Ordering::Relaxed);
                events.forget(&owner);
            },
            |events| {
                subscriptions.fetch_add(1, Ordering::Relaxed);
                events.on::<ServerTickEvent, _>(owner.clone(), |_| {});
            },
        );
    }

    assert_eq!(events.listener_count::<ServerTickEvent>(), 1);
    assert_eq!(resets.load(Ordering::Relaxed), 1);
    assert_eq!(subscriptions.load(Ordering::Relaxed), 2);
    assert_eq!(
        Arc::strong_count(&events),
        1,
        "the host must store only Weak"
    );
}

/// Graceful shutdown and startup rollback share the same exactly-once gate.
#[test]
fn graceful_and_startup_failure_unsubscribe_and_disable_once() {
    for path in ["graceful shutdown", "startup failure"] {
        let lifecycle = HostLifecycle::new();
        let events = Arc::new(EventBus::new());
        let owner = Identifier::from_foton("plugins");
        lifecycle.bind(
            &Arc::downgrade(&events),
            |_| {},
            |events| events.on::<ServerTickEvent, _>(owner.clone(), |_| {}),
        );

        let java_disables = AtomicUsize::new(0);
        let rust_unsubscribes = AtomicUsize::new(0);
        for _ in 0..2 {
            lifecycle
                .shutdown(
                    |events| {
                        rust_unsubscribes.fetch_add(1, Ordering::Relaxed);
                        events.forget(&owner);
                    },
                    || {
                        java_disables.fetch_add(1, Ordering::Relaxed);
                        Ok::<(), ()>(())
                    },
                )
                .expect("the test disable action cannot fail");
        }

        assert_eq!(events.listener_count::<ServerTickEvent>(), 0, "{path}");
        assert_eq!(rust_unsubscribes.load(Ordering::Relaxed), 1, "{path}");
        assert_eq!(java_disables.load(Ordering::Relaxed), 1, "{path}");
    }
}
