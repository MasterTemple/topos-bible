//! Named queries from `queries.toml` in the topos config directory, used with `-q NAME`:
//!
//! ```toml
//! pauline = '--nt -g "Pauline Epistles"'
//! sermons = 'Sermons -o "John 1" --exclude-book Philemon'
//! gospels = ["-g", "Gospels"]           # or the arguments already split
//! ```
//!
//! `-q NAME` is replaced by the query's arguments where it appears, so options after it still
//! override the query's. Queries may use other queries. The file is read even with
//! `--no-config`, since `-q` asks for it explicitly.

use std::{collections::BTreeMap, ffi::OsString, fs, path::PathBuf};

use toml::Value;

/// `$XDG_CONFIG_HOME/topos/queries.toml`, else `~/.config/topos/queries.toml`
pub fn default_path() -> Option<PathBuf> {
    Some(crate::config::default_path()?.with_file_name("queries.toml"))
}

pub type Queries = BTreeMap<String, Vec<String>>;

/// Reads the queries file (an empty set when it doesn't exist)
pub fn load() -> Result<Queries, String> {
    let Some(path) = default_path().filter(|path| path.exists()) else {
        return Ok(Queries::new());
    };
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn parse(text: &str) -> Result<Queries, String> {
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
    table
        .into_iter()
        .map(|(name, value)| {
            let args = match value {
                Value::String(query) => shlex::split(&query)
                    .ok_or_else(|| format!("query `{name}`: unbalanced quotes"))?,
                Value::Array(values) => values
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => Ok(s),
                        other => Err(format!("query `{name}`: expected strings, found {other}")),
                    })
                    .collect::<Result<_, _>>()?,
                other => return Err(format!("query `{name}`: expected a string, found {other}")),
            };
            Ok((name, args))
        })
        .collect()
}

/// The query name given by `-q NAME`, `-qNAME`, `--query NAME`, or `--query=NAME` at `args[idx]`
/// (`None` when the name is missing), and how many arguments that took
fn query_at(args: &[OsString], idx: usize) -> Option<(Option<String>, usize)> {
    let arg = args[idx].to_str()?;
    if arg == "-q" || arg == "--query" {
        let name = args.get(idx + 1).map(|n| n.to_string_lossy().into_owned());
        return Some((name, 2));
    }
    let name = arg
        .strip_prefix("--query=")
        .or_else(|| arg.strip_prefix("-q").filter(|rest| !rest.is_empty()))?;
    Some((Some(name.to_string()), 1))
}

/// Replaces each `-q NAME` with that query's arguments (loading the queries file only when needed)
pub fn expand(args: Vec<OsString>) -> Result<Vec<OsString>, String> {
    let end = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let needs_queries = (0..end).any(|idx| query_at(&args, idx).is_some());
    if !needs_queries {
        return Ok(args);
    }
    expand_with(args, &load()?)
}

pub fn expand_with(args: Vec<OsString>, queries: &Queries) -> Result<Vec<OsString>, String> {
    expand_inner(args, queries, &mut vec![])
}

fn expand_inner(
    args: Vec<OsString>,
    queries: &Queries,
    using: &mut Vec<String>,
) -> Result<Vec<OsString>, String> {
    let mut out = Vec::with_capacity(args.len());
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == "--" {
            out.extend(args[idx..].iter().cloned());
            break;
        }
        let Some((value, taken)) = query_at(&args, idx) else {
            out.push(args[idx].clone());
            idx += 1;
            continue;
        };
        let Some(name) = value else {
            return Err("-q needs the name of a query".into());
        };
        let Some(query) = queries.get(&name) else {
            let known = queries.keys().map(String::as_str).collect::<Vec<_>>();
            return Err(match default_path() {
                _ if !known.is_empty() => {
                    format!("unknown query `{name}` (queries: {})", known.join(", "))
                }
                Some(path) => format!("unknown query `{name}`: define it in {}", path.display()),
                None => format!("unknown query `{name}`"),
            });
        };
        if using.contains(&name) {
            return Err(format!("query `{name}` uses itself"));
        }
        using.push(name);
        let inner = query.iter().map(OsString::from).collect();
        out.extend(expand_inner(inner, queries, using)?);
        using.pop();
        idx += taken;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str], queries: &str) -> Result<Vec<String>, String> {
        let queries = parse(queries)?;
        let args = args.iter().map(OsString::from).collect();
        Ok(expand_with(args, &queries)?
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect())
    }

    #[test]
    fn queries_expand_in_place() {
        let queries =
            "paul = '--nt -g \"Pauline Epistles\"'\nsermons = ['Sermons', '-q', 'paul']\n";
        assert_eq!(
            run(&["topos", "-q", "paul", "-m", "count"], queries).unwrap(),
            ["topos", "--nt", "-g", "Pauline Epistles", "-m", "count"]
        );
        assert_eq!(
            run(&["topos", "--query=sermons", "-qpaul"], queries).unwrap(),
            [
                "topos",
                "Sermons",
                "--nt",
                "-g",
                "Pauline Epistles",
                "--nt",
                "-g",
                "Pauline Epistles"
            ]
        );
        // After `--`, everything is a path
        assert_eq!(
            run(&["topos", "--", "-q"], queries).unwrap(),
            ["topos", "--", "-q"]
        );
    }

    #[test]
    fn query_mistakes() {
        assert_eq!(
            run(&["topos", "-q", "nope"], "paul = '-g paul'").unwrap_err(),
            "unknown query `nope` (queries: paul)"
        );
        assert_eq!(
            run(&["topos", "-q"], "").unwrap_err(),
            "-q needs the name of a query"
        );
        assert_eq!(
            run(&["topos", "-q", "a"], "a = '-q b'\nb = '-q a'").unwrap_err(),
            "query `a` uses itself"
        );
        assert_eq!(
            run(&["topos"], "a = '-g \"oops'").unwrap_err(),
            "query `a`: unbalanced quotes"
        );
    }
}
