//! Checks on the plugin host.
//!
//! A JVM cannot be started twice in one process, so the tests that need one
//! are driven from a single place and the rest assert on what can be decided
//! without starting anything.

use std::fs::write;
use std::path::PathBuf;
use std::sync::Weak;

use super::{PluginHostConfig, PluginHostError};

fn config() -> PluginHostConfig {
    PluginHostConfig {
        java_home: PathBuf::from("/nowhere"),
        api_jar: PathBuf::from("/nowhere/foton-plugin-api.jar"),
        library_directory: None,
        plugin_directory: PathBuf::from("/nowhere/plugins"),
    }
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

use super::PluginHost;
