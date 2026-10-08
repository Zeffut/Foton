//! Command-line arguments.
//!
//! The flags exist because the installer needs two things the server cannot
//! otherwise tell it: what version it is, and a configuration directory without
//! a server left running to produce one. `import-anvil` is the one subcommand:
//! an offline job that fills the configured world storage from a vanilla world
//! and exits.

use std::path::PathBuf;

/// What `foton import-anvil` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub struct ImportArgs {
    /// The vanilla or Paper world folder to read.
    pub source: PathBuf,
    /// Dimensions to import; empty means every one found.
    pub dimensions: Vec<String>,
    /// The Foton world to import into, when it is not the one named like the dimension.
    pub world: Option<String>,
    /// Move an existing Foton world aside instead of refusing.
    pub replace: bool,
    /// Overrides the seed read from the source world.
    pub seed: Option<i64>,
    /// Leave out chunks still at an older `DataVersion` instead of refusing.
    pub skip_outdated: bool,
}

/// What the process was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the server. The default, and what every existing invocation does.
    Run,
    /// Print the version and exit.
    Version,
    /// Write the configuration files and exit.
    GenerateConfig,
    /// Import a vanilla world into the configured world storage, then exit.
    ImportAnvil(ImportArgs),
    /// An argument this binary does not understand.
    Unknown(String),
    /// A known subcommand used wrongly; carries what to tell the user.
    Invalid(String),
}

/// The usage text printed after a bad invocation.
pub const USAGE: &str = "usage: foton [--version] [--generate-config]\n       \
foton import-anvil <world folder> [--dimension <name>]... [--world <name>] [--seed <n>] [--replace] [--skip-outdated]";

/// Parses the arguments after the program name.
pub fn parse(mut args: impl Iterator<Item = String>) -> Action {
    let Some(arg) = args.next() else {
        return Action::Run;
    };
    match arg.as_str() {
        "--version" | "-V" => Action::Version,
        "--generate-config" => Action::GenerateConfig,
        "import-anvil" => parse_import(args),
        other => Action::Unknown(other.to_owned()),
    }
}

fn parse_import(mut args: impl Iterator<Item = String>) -> Action {
    let mut source = None;
    let mut dimensions = Vec::new();
    let mut world = None;
    let mut replace = false;
    let mut skip_outdated = false;
    let mut seed = None;

    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg, None),
        };
        match flag.as_str() {
            "--replace" => replace = true,
            "--skip-outdated" => skip_outdated = true,
            "--dimension" | "--world" | "--seed" => {
                let Some(value) = inline.or_else(|| args.next()) else {
                    return Action::Invalid(format!("{flag} needs a value"));
                };
                match flag.as_str() {
                    "--dimension" => dimensions.push(value),
                    "--world" => world = Some(value),
                    _ => match value.parse() {
                        Ok(parsed) => seed = Some(parsed),
                        Err(_) => {
                            return Action::Invalid(format!("{value} is not a valid seed"));
                        }
                    },
                }
            }
            unknown if unknown.starts_with("--") => {
                return Action::Invalid(format!("unknown option {unknown}"));
            }
            positional => {
                if source.replace(PathBuf::from(positional)).is_some() {
                    return Action::Invalid(
                        "only one world folder can be imported at a time".to_owned(),
                    );
                }
            }
        }
    }

    match source {
        Some(source) => Action::ImportAnvil(ImportArgs {
            source,
            dimensions,
            world,
            replace,
            seed,
            skip_outdated,
        }),
        None => Action::Invalid("import-anvil needs the world folder to read".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, ImportArgs, parse};
    use std::path::PathBuf;

    fn parsed(args: &[&str]) -> Action {
        parse(args.iter().map(|a| (*a).to_owned()))
    }

    #[test]
    fn no_arguments_starts_the_server() {
        assert_eq!(parsed(&[]), Action::Run);
    }

    #[test]
    fn version_is_recognized_in_both_spellings() {
        assert_eq!(parsed(&["--version"]), Action::Version);
        assert_eq!(parsed(&["-V"]), Action::Version);
    }

    #[test]
    fn generate_config_is_recognized() {
        assert_eq!(parsed(&["--generate-config"]), Action::GenerateConfig);
    }

    #[test]
    fn an_unknown_flag_is_reported_rather_than_ignored() {
        assert_eq!(parsed(&["--nope"]), Action::Unknown("--nope".to_owned()));
    }

    #[test]
    fn an_unknown_flag_does_not_silently_start_a_server() {
        // The failure that matters: a typo in an init script must not boot a
        // server nobody meant to start.
        assert_ne!(parsed(&["--generate-configs"]), Action::Run);
    }

    #[test]
    fn import_anvil_collects_its_options_in_either_spelling() {
        assert_eq!(
            parsed(&[
                "import-anvil",
                "maps/botw_nether",
                "--dimension",
                "nether",
                "--world=the_nether",
                "--seed",
                "-42",
                "--replace",
                "--skip-outdated",
            ]),
            Action::ImportAnvil(ImportArgs {
                source: PathBuf::from("maps/botw_nether"),
                dimensions: vec!["nether".to_owned()],
                world: Some("the_nether".to_owned()),
                replace: true,
                seed: Some(-42),
                skip_outdated: true,
            })
        );
    }

    #[test]
    fn import_anvil_without_a_folder_or_with_a_typo_never_runs_an_import() {
        for args in [
            &["import-anvil"][..],
            &["import-anvil", "w", "--replac"],
            &["import-anvil", "w", "--seed", "abc"],
            &["import-anvil", "w", "--dimension"],
            &["import-anvil", "a", "b"],
        ] {
            assert!(matches!(parsed(args), Action::Invalid(_)), "{args:?}");
        }
    }
}
