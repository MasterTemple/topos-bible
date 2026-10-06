//! Default options from `~/.config/topos/config.toml` (or `--config PATH`), added before the
//! command line's.
//!
//! Each key is a long option name and its value becomes that option's argument:
//!
//! ```toml
//! mode = "grouped"                    # --mode grouped
//! cache = true                        # --cache
//! exclude-book = ["Song of Solomon"]  # --exclude-book "Song of Solomon"
//! data = "~/bible/custom.json"        # --data ~/bible/custom.json (`~/` is expanded)
//! ```

use std::{ffi::OsString, fs, path::PathBuf};

use clap::CommandFactory;
use toml::Value;

use crate::args::Args;

/// `$XDG_CONFIG_HOME/topos/config.toml`, else `~/.config/topos/config.toml`
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".config")))?;
    Some(base.join("topos").join("config.toml"))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The `--config PATH` (or `--config=PATH`) on the command line
fn config_flag(args: &[OsString]) -> Option<PathBuf> {
    args.iter().enumerate().find_map(|(idx, arg)| {
        let arg = arg.to_str()?;
        if arg == "--config" {
            args.get(idx + 1).map(PathBuf::from)
        } else {
            arg.strip_prefix("--config=").map(PathBuf::from)
        }
    })
}

/// The command line with the config file's options inserted before the user's arguments
pub fn with_defaults(args: Vec<OsString>) -> Result<Vec<OsString>, String> {
    if args.iter().any(|arg| arg == "--no-config") {
        return Ok(args);
    }
    let path = match config_flag(&args) {
        // A config the user names must exist
        Some(path) if !path.exists() => {
            return Err(format!("{}: config file not found", path.display()));
        }
        Some(path) => path,
        None => match default_path().filter(|path| path.exists()) {
            Some(path) => path,
            None => return Ok(args),
        },
    };
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let defaults = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut args = args.into_iter();
    Ok(args
        .next()
        .into_iter()
        .chain(defaults)
        .chain(args)
        .collect())
}

/// Turns the TOML table into command line arguments
fn parse(text: &str) -> Result<Vec<OsString>, String> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let command = Args::command();
    let mut args = vec![];
    for (key, value) in table {
        let name = key.replace('_', "-");
        let known = command
            .get_arguments()
            .any(|arg| arg.get_long() == Some(name.as_str()));
        if !known || name == "no-config" || name == "config" {
            return Err(format!("unknown option `{key}`"));
        }
        let flag = OsString::from(format!("--{name}"));
        let values = match value {
            Value::Array(values) => values,
            value => vec![value],
        };
        for value in values {
            match value {
                Value::Boolean(true) => args.push(flag.clone()),
                Value::Boolean(false) => {}
                Value::String(s) => args.extend([flag.clone(), expand_home(&s)]),
                Value::Integer(n) => args.extend([flag.clone(), n.to_string().into()]),
                other => return Err(format!("unsupported value for `{key}`: {other}")),
            }
        }
    }
    Ok(args)
}

fn expand_home(value: &str) -> OsString {
    match (value.strip_prefix("~/"), home()) {
        (Some(rest), Some(home)) => home.join(rest).into_os_string(),
        _ => value.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_become_arguments() {
        let args = parse(
            "mode = \"grouped\"\ncache = true\nsort = false\nexclude_book = [\"Jude\", \"Ruth\"]\nafter-context = 2\n",
        )
        .unwrap();
        let args: Vec<_> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "--after-context",
                "2",
                "--cache",
                "--exclude-book",
                "Jude",
                "--exclude-book",
                "Ruth",
                "--mode",
                "grouped"
            ]
        );
    }

    #[test]
    fn rejects_unknown_options() {
        assert_eq!(
            parse("colour = \"always\"").unwrap_err(),
            "unknown option `colour`"
        );
        assert!(parse("no-config = true").is_err());
        assert!(parse("config = \"other.toml\"").is_err());
        assert!(parse("mode = [1.5]").is_err());
        assert!(parse("not toml").is_err());
    }
}
