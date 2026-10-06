//! Runs the `topos` binary like a user would.

use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

/// A directory for this test process (configs and caches never touch the real home)
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("topos-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn topos(args: &[&str], stdin: Option<&str>) -> Output {
    topos_with_config(None, args, stdin)
}

/// Runs with `config` as the default config file (or no config file)
fn topos_with_config(config: Option<&str>, args: &[&str], stdin: Option<&str>) -> Output {
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let home = scratch(&format!("config-{run}"));
    if let Some(config) = config {
        std::fs::create_dir_all(home.join("topos")).unwrap();
        std::fs::write(home.join("topos/config.toml"), config).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_topos"))
        .args(args)
        .env("XDG_CONFIG_HOME", &home)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn quickfix_from_stdin() {
    // The format the Neovim integration in the README parses
    let output = topos(
        &["-m", "quickfix"],
        Some("intro\nsee Rom 8:28 and Jude 5\n"),
    );
    assert_eq!(stdout(&output), ":2:5: Romans 8:28\n:2:18: Jude 1:5\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn text_and_filters() {
    let output = topos(
        &[
            "--text",
            "John 3:16, Rom 8:28",
            "-t",
            "new",
            "--exclude-book",
            "John",
            "-m",
            "quickfix",
        ],
        None,
    );
    assert_eq!(stdout(&output), ":1:12: Romans 8:28\n");

    let output = topos(
        &["--text", "John 3:16", "-f", "osis", "-m", "quickfix"],
        None,
    );
    assert_eq!(stdout(&output), ":1:1: John.3.16\n");
}

#[test]
fn json_lines() {
    let output = topos(&["--text", "x John 3:16", "-m", "json"], None);
    let value: serde_json::Value = serde_json::from_str(stdout(&output).trim()).unwrap();
    assert_eq!(value["reference"], "John 3:16");
    assert_eq!(value["osis"], "John.3.16");
    assert_eq!(value["text"], "John 3:16");
    assert_eq!(
        (value["line"].as_u64(), value["column"].as_u64()),
        (Some(1), Some(3))
    );

    // UTF-16 positions, book data, and segments (for editors and the Obsidian plugin).
    // `é` and the spaces are 1 UTF-16 unit each, and `📖` is 2
    let text = "é 📖 Jn 3:16-18; 5\nnext Rom 8:28";
    let output = topos(&["--text", text, "-m", "json"], None);
    let lines: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["book_id"], 43);
    assert_eq!(lines[0]["book"], "John");
    assert_eq!(
        (
            lines[0]["start_utf16"].as_u64(),
            lines[0]["end_utf16"].as_u64()
        ),
        (Some(5), Some(18))
    );
    assert_eq!(lines[0]["utf16_column"], 6);
    assert_eq!(lines[0]["line_text"], "é 📖 Jn 3:16-18; 5");
    assert_eq!(
        lines[0]["segments"],
        serde_json::json!([
            { "tag": "Verses", "start": { "chapter": 3, "verse": 16 }, "end": { "chapter": 3, "verse": 18 } },
            { "tag": "Chapters", "start": 5, "end": null },
        ])
    );
    assert_eq!(lines[1]["start_utf16"], 24);
    assert_eq!(lines[1]["line_text"], "next Rom 8:28");
}

#[test]
fn exit_codes() {
    assert_eq!(topos(&["--text", "nothing"], None).status.code(), Some(1));

    let unknown = topos(&["--text", "John 3:16", "-b", "Jhon"], None);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown book \"Jhon\""));

    let missing = topos(&["does-not-exist.md"], None);
    assert_eq!(missing.status.code(), Some(2));
}

#[test]
fn cache_reuses_and_refreshes_results() {
    let dir = std::env::temp_dir().join(format!("topos-cache-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    let file = dir.join("docs/a.txt");
    std::fs::write(&file, "See John 3:16\n").unwrap();

    let run = || {
        let output = Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(["--cache", "-m", "quickfix"])
            .arg(&file)
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap()
    };
    let first = run();
    assert!(first.ends_with(":1:5: John 3:16\n"), "{first}");
    assert_eq!(
        std::fs::read_dir(dir.join("cache/topos")).unwrap().count(),
        1
    );
    assert_eq!(run(), first);

    // A changed file is searched again (the size changes, so the mtime resolution does not matter)
    std::fs::write(&file, "Now Romans 8:28 instead\n").unwrap();
    assert!(run().ends_with(":1:5: Romans 8:28\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inside_overlaps_and_outside() {
    let text = "Rom 8:28; Rom 8:38-9:1; Rom 9:2";
    let refs = |args: &[&str]| {
        let mut all = vec!["--text", text, "-m", "quickfix"];
        all.extend(args);
        stdout(&topos(&all, None))
    };
    assert_eq!(refs(&["-i", "Romans 8"]), ":1:1: Romans 8:28\n");
    assert_eq!(
        refs(&["-o", "Romans 8"]),
        ":1:1: Romans 8:28\n:1:11: Romans 8:38-9:1\n"
    );
    assert_eq!(
        refs(&["-o", "Romans 9", "--outside", "Romans 9:2"]),
        ":1:11: Romans 8:38-9:1\n"
    );
}

#[test]
fn default_config_file() {
    let config = "format = \"osis\"\nmode = \"quickfix\"\n";
    let run = |args: &[&str]| {
        let mut all = vec!["--text", "John 3:16"];
        all.extend(args);
        stdout(&topos_with_config(Some(config), &all, None))
    };
    // The config applies, the command line overrides it, and --no-config ignores it
    assert_eq!(run(&[]), ":1:1: John.3.16\n");
    assert_eq!(run(&["-f", "name"]), ":1:1: John 3:16\n");
    assert_eq!(run(&["--no-config", "-m", "quickfix"]), ":1:1: John 3:16\n");

    let bad = topos_with_config(Some("colour = \"always\""), &["--text", "x"], None);
    assert_eq!(bad.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("config.toml: unknown option `colour`"),
        "{stderr}"
    );
}

#[test]
fn testament_shorthands_and_total_count() {
    let text = "Gen 1:1, John 3:16, Rom 8:28";
    let run = |args: &[&str]| {
        let mut all = vec!["--text", text];
        all.extend(args);
        stdout(&topos(&all, None))
    };
    assert_eq!(run(&["--nt", "--total-count"]), "2\n");
    assert_eq!(run(&["--ot", "-m", "total-count"]), "1\n");
    assert_eq!(run(&["--ot", "--nt", "--total-count"]), "3\n");
}

#[test]
fn explicit_config_file() {
    let dir = scratch("explicit-config");
    let config = dir.join("other.toml");
    std::fs::write(&config, "format = \"osis\"\nmode = \"quickfix\"\n").unwrap();
    let config = config.to_str().unwrap();

    let output = topos(&["--config", config, "--text", "John 3:16"], None);
    assert_eq!(stdout(&output), ":1:1: John.3.16\n");
    let equals = format!("--config={config}");
    assert_eq!(
        stdout(&topos(&[&equals, "--text", "John 3:16"], None)),
        ":1:1: John.3.16\n"
    );

    let missing = topos(
        &["--config", "/nonexistent/topos.toml", "--text", "x"],
        None,
    );
    assert_eq!(missing.status.code(), Some(2));
    let both = topos(&["--config", config, "--no-config", "--text", "x"], None);
    assert_eq!(both.status.code(), Some(2));
}

#[test]
fn ext_limits_walked_files() {
    let dir = scratch("ext");
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/a.md"), "John 3:16\n").unwrap();
    std::fs::write(dir.join("notes/b.TXT"), "Romans 8:28\n").unwrap();
    std::fs::write(dir.join("notes/c.html"), "<p>Genesis 1:1</p>\n").unwrap();
    let run = |args: &[&str]| {
        let mut args = args.to_vec();
        args.extend(["--no-config", "-m", "quickfix", "--sort"]);
        let output = Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(&args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        text.lines()
            .map(|line| line.rsplit(": ").next().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(run(&["."]), ["John 3:16", "Romans 8:28", "Genesis 1:1"]);
    assert_eq!(
        run(&[".", "--ext", "md,.txt"]),
        ["John 3:16", "Romans 8:28"]
    );
    assert_eq!(
        run(&[".", "--ext", "md", "--ext", "txt"]),
        ["John 3:16", "Romans 8:28"]
    );
    // A file named on the command line is searched whatever its extension
    assert_eq!(run(&["notes/c.html", "--ext", "md"]), ["Genesis 1:1"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn testaments_narrow_genres() {
    let text = "John 3:16, Romans 8:28, Genesis 1:1";
    let nt = topos(
        &[
            "--text",
            text,
            "--nt",
            "-g",
            "Pauline Epistles",
            "-m",
            "quickfix",
        ],
        None,
    );
    assert_eq!(stdout(&nt), ":1:12: Romans 8:28\n");
    let ot = topos(&["--text", text, "--ot", "-g", "Pauline Epistles"], None);
    assert_eq!(ot.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&ot.stderr).contains("nothing can match"));
}
