//! Command-line arguments.
//!
//! Two flags, because the installer needs exactly two things the server
//! cannot otherwise tell it: what version it is, and a configuration
//! directory without a server left running to produce one.

/// What the process was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the server. The default, and what every existing invocation does.
    Run,
    /// Print the version and exit.
    Version,
    /// Write the configuration files and exit.
    GenerateConfig,
    /// An argument this binary does not understand.
    Unknown(String),
}

/// Parses the arguments after the program name.
pub fn parse(mut args: impl Iterator<Item = String>) -> Action {
    let Some(arg) = args.next() else {
        return Action::Run;
    };
    match arg.as_str() {
        "--version" | "-V" => Action::Version,
        "--generate-config" => Action::GenerateConfig,
        other => Action::Unknown(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, parse};

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
}
