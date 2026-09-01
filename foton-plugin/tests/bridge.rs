//! Does a call made in Java actually reach Foton?
//!
//! The unit tests decide what they can without a runtime. This one starts a
//! real one, because the thing most likely to be wrong is not the Rust and not
//! the Java but the seam: a native descriptor that disagrees with its Java
//! declaration compiles perfectly on both sides and fails the first time a
//! plugin calls it.
//!
//! It needs a Java installation and a built API jar. Both exist in this
//! repository's environment; where they do not, the test says what is missing
//! and passes, because a checkout without a JDK is not a broken checkout.

use std::env::var_os;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::Weak;

use foton_plugin::{PluginHost, PluginHostConfig};

/// The repository root, from this crate's manifest.
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// A Java installation, from the environment or the usual places.
fn java_home() -> Option<PathBuf> {
    if let Some(home) = var_os("JAVA_HOME").or_else(|| var_os("FOTON_JAVA_HOME")) {
        return Some(PathBuf::from(home));
    }
    let jvm = Path::new("/usr/lib/jvm");
    let entries = read_dir(jvm).ok()?;
    let mut homes: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("lib/server/libjvm.so").is_file())
        .collect();
    homes.sort();
    homes.pop()
}

/// A call that starts in Java comes back with Foton's answer.
///
/// `serverName` is the smallest native there is, which is the point: if the
/// registration, the descriptors and the class path are right, this returns
/// "Foton", and if any of the three is wrong it cannot.
#[test]
fn a_call_made_in_java_reaches_foton() {
    let Some(java_home) = java_home() else {
        println!("no Java installation found; the bridge was not exercised");
        return;
    };
    let api_jar = repo().join("plugin-api/build/foton-plugin-api.jar");
    if !api_jar.is_file() {
        println!("run dev/build-plugin-api.sh first; the bridge was not exercised");
        return;
    }

    let config = PluginHostConfig {
        java_home,
        api_jar,
        library_directory: Some(repo().join("plugin-api/lib")),
        plugin_directory: repo().join("plugin-api/build/no-plugins"),
    };

    // No server behind the handle: the natives answer as they would for one
    // that has shut down, which is a real case and the one that needs to not
    // crash.
    let host = match PluginHost::start(&config, &Weak::new()) {
        Ok(host) => host,
        Err(error) => panic!("the plugin host should start: {error}"),
    };

    let name = host
        .server_name_from_java()
        .expect("the Java side should be able to ask");

    assert_eq!(
        name, "Foton",
        "the answer came from Rust, so it should be Rust's"
    );
}
