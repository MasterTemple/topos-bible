//! Default options from `~/.config/topos/config.toml` (or `--config PATH`), added before the
//! command line's. On first use (without `--config` or `--no-config`), a commented
//! `config.toml` and a sample `queries.toml` are written there if they don't exist.
//!
//! Each key is a long option name and its value becomes that option's argument:
//!
//! ```toml
//! mode = "grouped"                    # --mode grouped
//! cache = true                        # --cache
//! exclude-book = ["Song of Solomon"]  # --exclude-book "Song of Solomon"
//! data = "~/bible/custom.json"        # --data ~/bible/custom.json (`~/` is expanded)
//! ```

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

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

/// The commented config file and sample queries written on first use
const DEFAULT_CONFIG: &str = include_str!("../defaults/config.toml");
const DEFAULT_QUERIES: &str = include_str!("../defaults/queries.toml");

/// Writes `config.toml` and `queries.toml` next to `config` when they don't exist, so every user
/// has documented files to start from. Existing files are never touched, and failing to write
/// (a read-only home, say) is not an error.
fn create_defaults(config: &Path) {
    let Some(dir) = config.parent() else { return };
    let queries = crate::queries::default_path();
    let missing: Vec<_> = [
        (Some(config.to_path_buf()), DEFAULT_CONFIG),
        (queries, DEFAULT_QUERIES),
    ]
    .into_iter()
    .filter_map(|(path, text)| Some((path?, text)))
    .filter(|(path, _)| !path.exists())
    .collect();
    if missing.is_empty() || fs::create_dir_all(dir).is_err() {
        return;
    }
    for (path, text) in missing {
        // create_new: never overwrite a file another process just wrote
        if let Ok(mut file) = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            let _ = std::io::Write::write_all(&mut file, text.as_bytes());
        }
    }
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
        None => match default_path()
            .inspect(|path| create_defaults(path))
            .filter(|path| path.exists())
        {
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
