//! Checks on the plugin host.
//!
//! A JVM cannot be started twice in one process, so the tests that need one
//! are driven from a single place and the rest assert on what can be decided
//! without starting anything.

use crate::item_bridge as bridge_item_bridge;
use std::env;
use std::fs::{copy, create_dir_all, read_dir, remove_file, write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use super::HostLifecycle;
use super::{JVM_RUNTIMES, PluginHost, PluginHostConfig, PluginHostError, forward};
use foton_core::event::{EventBus, ServerTickEvent};
use foton_utils::Identifier;

fn config() -> PluginHostConfig {
    PluginHostConfig {
        java_home: PathBuf::from("/nowhere"),
        api_jar: PathBuf::from("/nowhere/foton-plugin-api.jar"),
        library_directories: Vec::new(),
        plugin_directory: PathBuf::from("/nowhere/plugins"),
        item_snapshot_limit: PluginHostConfig::DEFAULT_ITEM_SNAPSHOT_LIMIT,
    }
}

fn pinned_runtime_config() -> (tempfile::TempDir, PluginHostConfig) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let api_jar = directory.path().join("build/foton-plugin-api.jar");
    let libraries = directory.path().join("lib");
    create_dir_all(api_jar.parent().expect("the API jar has a parent"))
        .expect("the fixture build directory should exist");
    create_dir_all(&libraries).expect("the fixture library directory should exist");
    write(&api_jar, b"not really a jar").expect("the fixture API should write");
    let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugin-api/lib");
    for entry in read_dir(committed).expect("the committed library directory should exist") {
        let path = entry
            .expect("the committed entry should be readable")
            .path();
        if path.extension().is_some_and(|extension| extension == "jar") {
            copy(
                &path,
                libraries.join(path.file_name().expect("a committed jar has a name")),
            )
            .expect("the pinned jar should copy");
        }
    }
    let config = PluginHostConfig {
        api_jar,
        library_directories: vec![libraries],
        ..config()
    };
    (directory, config)
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
        library_directories: vec![PathBuf::from("/srv/foton/runtime-libraries")],
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
    let (_directory, config) = pinned_runtime_config();
    let class_path = config.class_path().expect("the pinned set should validate");

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
        [
            "foton-plugin-api.jar",
            "adventure-api-5.2.0.jar",
            "adventure-key-5.2.0.jar",
            "adventure-text-minimessage-5.2.0.jar",
            "adventure-text-logger-slf4j-5.2.0.jar",
            "adventure-text-serializer-plain-5.2.0.jar",
            "adventure-text-serializer-legacy-5.2.0.jar",
            "adventure-text-serializer-gson-5.2.0.jar",
            "adventure-text-serializer-json-5.2.0.jar",
            "adventure-text-serializer-commons-5.2.0.jar",
            "option-1.1.0.jar",
            "annotations-26.1.0.jar",
            "brigadier-1.3.10.jar",
            "gson-2.14.0.jar",
            "guava-33.6.0-jre.jar",
            "failureaccess-1.0.3.jar",
            "jspecify-1.0.0.jar",
            "error_prone_annotations-2.47.0.jar",
            "j2objc-annotations-3.1.jar",
            "joml-1.10.8.jar",
            "kotlin-stdlib-jdk8-1.8.20.jar",
            "kotlin-stdlib-jdk7-1.8.20.jar",
            "kotlin-stdlib-1.8.20.jar",
            "kotlin-stdlib-common-1.8.20.jar",
            "slf4j-api-2.0.17.jar",
            "snakeyaml-2.2.jar",
            "netty-common-4.2.15.Final.jar",
            "netty-buffer-4.2.15.Final.jar",
            "netty-transport-4.2.15.Final.jar",
            "netty-resolver-4.2.15.Final.jar",
            "netty-codec-base-4.2.15.Final.jar",
            "sqlite-jdbc-3.49.1.0.jar",
            "auto-service-annotations-1.1.1.jar",
            "maven-resolver-api-1.9.18.jar",
            "maven-resolver-spi-1.9.18.jar",
            "maven-resolver-util-1.9.18.jar",
            "maven-resolver-impl-1.9.18.jar",
            "maven-resolver-named-locks-1.9.18.jar",
            "maven-resolver-connector-basic-1.9.18.jar",
            "maven-resolver-provider-3.9.6.jar",
            "maven-model-3.9.6.jar",
            "maven-model-builder-3.9.6.jar",
            "maven-repository-metadata-3.9.6.jar",
            "maven-artifact-3.9.6.jar",
            "maven-builder-support-3.9.6.jar",
            "plexus-utils-3.5.1.jar",
            "plexus-interpolation-1.26.jar",
            "javax.inject-1.jar",
            "commons-lang3-3.20.0.jar",
        ],
        "the API jar leads, then the manifest order is stable"
    );
}

/// Exercises the real JNI descriptors against the built Java API. Clean
/// Cargo-only checkouts do not have that generated jar yet, so build the API
/// before running this targeted integration check.
#[test]
fn login_attempt_lifecycle_crosses_the_rust_java_bridge() {
    let Some(java_home) = env::var_os("JAVA_HOME").map(PathBuf::from) else {
        eprintln!("skipping Rust/Java bridge check because JAVA_HOME is unset");
        return;
    };
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the plugin crate should be inside the repository")
        .to_owned();
    let api_jar = repository.join("plugin-api/build/foton-plugin-api.jar");
    if !api_jar.is_file() {
        eprintln!("skipping Rust/Java bridge check because the API jar is not built");
        return;
    }
    let plugins = tempfile::tempdir().expect("temporary plugin directory");
    let host = PluginHost::start(
        &PluginHostConfig {
            java_home,
            api_jar,
            library_directories: vec![repository.join("plugin-api/lib")],
            plugin_directory: plugins.path().to_owned(),
            item_snapshot_limit: PluginHostConfig::DEFAULT_ITEM_SNAPSHOT_LIMIT,
        },
        &Weak::new(),
    )
    .expect("the test JVM should start");

    bridge_item_bridge::jvm_tests::check(&host.vm);

    assert!(
        forward::login_lifecycle_bridge_check(&host.vm),
        "Rust login/abort/join calls did not reach the Java attempt lifecycle"
    );
    bridge_item_bridge::jvm_tests::check_terminal(&host);
    drop(host);
    assert!(
        !JVM_RUNTIMES.lock().is_empty(),
        "dropping the host must not unmap the process-lifetime JVM runtime"
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

#[test]
fn the_exact_pinned_runtime_library_set_is_accepted() {
    let (_directory, config) = pinned_runtime_config();

    let class_path = config.class_path().expect("the pinned set should validate");

    assert!(class_path.contains("adventure-api-5.2.0.jar"));
    assert!(class_path.contains("snakeyaml-2.2.jar"));
}

#[test]
fn a_missing_pinned_runtime_library_is_rejected_with_its_path() {
    let (_directory, config) = pinned_runtime_config();
    let missing = config
        .resolved_library_directory()
        .join("snakeyaml-2.2.jar");
    remove_file(&missing).expect("the temporary jar should be removable");

    let error = config
        .class_path()
        .expect_err("a missing pin must be rejected");

    assert!(
        error
            .to_string()
            .contains(missing.to_string_lossy().as_ref())
    );
    assert!(error.to_string().contains("missing"));
}

#[test]
fn an_extra_runtime_jar_is_rejected_with_its_path() {
    let (_directory, config) = pinned_runtime_config();
    let extra = config.resolved_library_directory().join("decoy.jar");
    write(&extra, b"decoy").expect("the decoy should write");

    let error = config
        .class_path()
        .expect_err("an extra jar must be rejected");

    assert!(error.to_string().contains(extra.to_string_lossy().as_ref()));
    assert!(error.to_string().contains("not in the pinned manifest"));
}

#[test]
fn a_tampered_runtime_jar_is_rejected_with_its_path_and_digests() {
    let (_directory, config) = pinned_runtime_config();
    let tampered = config
        .resolved_library_directory()
        .join("snakeyaml-2.2.jar");
    write(&tampered, b"tampered").expect("the temporary jar should be replaceable");

    let error = config
        .class_path()
        .expect_err("a changed digest must be rejected");
    let message = error.to_string();

    assert!(message.contains(tampered.to_string_lossy().as_ref()));
    assert!(message.contains("expected 1467931448a0817696ae2805b7b8b20"));
    assert!(message.contains("got d121be3103007b41edf96f8262925f8c"));
}

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

#[test]
fn nested_runtime_jars_cannot_bypass_the_primary_manifest() {
    let (_directory, config) = pinned_runtime_config();
    let nested = config.resolved_library_directory().join("nested");
    create_dir_all(&nested).expect("nested runtime fixture directory");
    write(nested.join("injected.jar"), b"not pinned").expect("nested runtime fixture jar");
    assert!(
        config.class_path().is_err(),
        "a nested jar must fail closed"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_runtime_pin_cannot_escape_the_validated_directory() {
    use std::os::unix::fs::symlink;
    let (directory, config) = pinned_runtime_config();
    let pin = config
        .resolved_library_directory()
        .join("sqlite-jdbc-3.49.1.0.jar");
    let outside = directory.path().join("outside.jar");
    copy(&pin, &outside).expect("copy pinned runtime fixture");
    remove_file(&pin).expect("replace fixture pin with a symlink");
    symlink(outside, &pin).expect("symlink runtime fixture pin");
    assert!(
        config.class_path().is_err(),
        "even matching symlink bytes must be rejected"
    );
}
