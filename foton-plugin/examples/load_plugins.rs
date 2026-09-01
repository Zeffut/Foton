//! Starts a Java runtime and enables every plugin in a directory.
//!
//! The end-to-end check for the plugin host, kept as an example rather than a
//! test because it needs a Java installation, a built API jar and at least one
//! plugin jar — none of which a checkout has.
//!
//! ```text
//! FOTON_JAVA_HOME=$(dirname $(dirname $(readlink -f $(command -v java)))) \
//!   cargo run -p foton-plugin --example load_plugins -- ./plugins
//! ```

use std::env::{args_os, var_os};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use foton_plugin::{PluginHost, PluginHostConfig};

fn main() -> ExitCode {
    let Some(java_home) = var_os("FOTON_JAVA_HOME").map(PathBuf::from) else {
        eprintln!("set FOTON_JAVA_HOME to a JDK or JRE directory");
        return ExitCode::FAILURE;
    };
    let plugins = args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("plugins"), PathBuf::from);

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let config = PluginHostConfig {
        java_home,
        api_jar: repo.join("plugin-api/build/foton-plugin-api.jar"),
        library_directory: Some(repo.join("plugin-api/lib")),
        plugin_directory: plugins.clone(),
    };

    let host = match PluginHost::start(&config) {
        Ok(host) => host,
        Err(error) => {
            eprintln!("the plugin host did not start: {error}");
            return ExitCode::FAILURE;
        }
    };

    match host.load_all(&plugins) {
        Ok(enabled) => println!("--- {enabled} plugin(s) enabled from Rust ---"),
        Err(error) => {
            eprintln!("loading failed: {error}");
            return ExitCode::FAILURE;
        }
    }

    if let Err(error) = host.disable_all() {
        eprintln!("shutting down failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
