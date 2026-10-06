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
        .env("XDG_CACHE_HOME", home.join("cache"))
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
    let dir = scratch("cache-test");
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    let file = dir.join("docs/a.txt");
    std::fs::write(&file, "See John 3:16\n").unwrap();

    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(["--cache", "-m", "quickfix"])
            .args(args)
            .arg(&file)
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap()
    };
    let first = run(&[]);
    assert!(first.ends_with(":1:5: John 3:16\n"), "{first}");
    assert_eq!(run(&[]), first);

    // Change the text but keep the size and modification time: a cached result shows the old
    // reference, which proves that other filters reuse the cached (unfiltered) results
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::fs::write(&file, "See Ruth 3:16\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(run(&["--nt"]).ends_with(":1:5: John 3:16\n"));
    assert!(run(&["-o", "John 3"]).ends_with(":1:5: John 3:16\n"));
    // Filters that rule the file out (by its books) find nothing
    assert_eq!(run(&["--ot"]), "");
    assert_eq!(run(&["-b", "Genesis"]), "");

    // A changed size or time searches the file again
    std::fs::write(&file, "Now Romans 8:28 instead\n").unwrap();
    assert!(run(&[]).ends_with(":1:5: Romans 8:28\n"));
    assert!(run(&["--nt"]).ends_with(":1:5: Romans 8:28\n"));

    // --clear-cache deletes it
    let cache = dir.join("cache/topos");
    assert!(cache.exists());
    Command::new(env!("CARGO_BIN_EXE_topos"))
        .arg("--clear-cache")
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .output()
        .unwrap();
    assert!(!cache.exists());
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

    let passages = topos(&["--text", text, "-b", "Genesis", "-i", "Romans 8"], None);
    assert_eq!(passages.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&passages.stderr)
            .contains("Romans 8 is in books that aren't searched, so nothing can match")
    );
}

#[test]
fn named_queries() {
    let home = scratch("queries");
    std::fs::create_dir_all(home.join("topos")).unwrap();
    std::fs::write(
        home.join("topos/queries.toml"),
        "paul = '--nt -g \"Pauline Epistles\"'\ngospels = ['-g', 'Gospels']\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(args)
            .env("XDG_CONFIG_HOME", &home)
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let text = "John 3:16, Romans 8:28, Genesis 1:1";
    let paul = run(&[
        "--text",
        text,
        "--no-config",
        "-q",
        "paul",
        "-m",
        "quickfix",
    ]);
    assert_eq!(stdout(&paul), ":1:12: Romans 8:28\n");
    let gospels = run(&["--text", text, "-qgospels", "-m", "quickfix"]);
    assert_eq!(stdout(&gospels), ":1:1: John 3:16\n");

    let unknown = run(&["--text", text, "-q", "nope"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&unknown.stderr)
            .contains("unknown query `nope` (queries: gospels, paul)")
    );

    let list = run(&["--list-queries"]);
    assert_eq!(
        stdout(&list),
        "gospels\t-g Gospels\npaul\t--nt -g 'Pauline Epistles'\n"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn first_run_writes_default_files() {
    let home = scratch("first-run");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(args)
            .env("XDG_CONFIG_HOME", &home)
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    // --no-config leaves the config folder alone
    run(&["--text", "John 3:16", "--no-config"]);
    assert!(!home.join("topos").exists());

    let first = run(&[
        "--text",
        "John 3:16 and Romans 8:28",
        "-q",
        "pauline",
        "-m",
        "quickfix",
    ]);
    assert_eq!(stdout(&first), ":1:15: Romans 8:28\n");
    let config = std::fs::read_to_string(home.join("topos/config.toml")).unwrap();
    // The default config turns the cache on
    assert!(config.contains("\ncache = true"), "{config}");
    assert!(home.join("cache/topos").exists());
    assert_eq!(
        stdout(&run(&["--list-queries"])),
        "pauline\t--nt -g 'Pauline Epistles'\n"
    );

    // Existing files are kept
    std::fs::write(home.join("topos/queries.toml"), "mine = '-b John'\n").unwrap();
    run(&["--text", "x"]);
    assert_eq!(stdout(&run(&["--list-queries"])), "mine\t-b John\n");
    let _ = std::fs::remove_dir_all(&home);
}

/// Asks topos for completions the way bash's registration does
fn complete(words: &[&str], config_home: &std::path::Path) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_topos"))
        .arg("--")
        .args(words)
        .env("COMPLETE", "bash")
        .env("_CLAP_IFS", "\n")
        .env("_CLAP_COMPLETE_INDEX", (words.len() - 1).to_string())
        .env("_CLAP_COMPLETE_COMP_TYPE", "9")
        .env("_CLAP_COMPLETE_SPACE", "true")
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_CACHE_HOME", config_home.join("cache"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn shell_completions() {
    let home = scratch("complete");
    std::fs::create_dir_all(home.join("topos")).unwrap();
    std::fs::write(home.join("topos/queries.toml"), "paul = '-g pauline'\n").unwrap();
    let c = |words: &[&str]| complete(words, &home);

    assert_eq!(
        c(&["topos", "-m", ""]),
        [
            "auto",
            "grouped",
            "quickfix",
            "table",
            "json",
            "count",
            "total-count"
        ]
    );
    assert_eq!(c(&["topos", "-b", "1 co"]), ["1 Corinthians"]);
    // What bash passes for an unquoted word keeps its backslashes
    assert_eq!(
        c(&["topos", "--exclude-book", "song\\ of"]),
        ["Song of Solomon"]
    );
    assert_eq!(c(&["topos", "-b", "jn"]), ["Jonah", "John"]);
    assert!(c(&["topos", "-g", ""]).contains(&"Pauline Epistles".to_string()));
    assert_eq!(c(&["topos", "-t", ""]), ["old", "new"]);
    assert_eq!(c(&["topos", "-q", ""]), ["paul"]);
    assert_eq!(c(&["topos", "-o", "Ps 119:"]).len(), 176);
    assert_eq!(c(&["topos", "-i", "rom 8"]), ["Romans 8"]);
    assert!(c(&["topos", "--ex"]).contains(&"--exclude-book".to_string()));

    // The registration script lets readline quote candidates
    let script = Command::new(env!("CARGO_BIN_EXE_topos"))
        .env("COMPLETE", "bash")
        .output()
        .unwrap();
    let script = String::from_utf8(script.stdout).unwrap();
    assert!(script.contains("compopt -o filenames"), "{script}");
    assert!(script.contains("complete -o nospace"), "{script}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn no_cache_overrides_the_config() {
    let home = scratch("no-cache");
    std::fs::create_dir_all(home.join("topos")).unwrap();
    std::fs::write(home.join("topos/config.toml"), "cache = true\n").unwrap();
    std::fs::write(home.join("a.txt"), "John 3:16\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_topos"))
            .arg(home.join("a.txt"))
            .args(args)
            .env("XDG_CONFIG_HOME", &home)
            .env("XDG_CACHE_HOME", home.join("cache"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    run(&["--no-cache"]);
    assert!(!home.join("cache/topos").exists());
    // The last of --cache and --no-cache wins
    run(&["--no-cache", "--cache"]);
    assert!(home.join("cache/topos").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn complete_and_list_books() {
    let lines = |args: &[&str]| -> Vec<String> {
        let mut all = vec!["--no-config"];
        all.extend(args);
        stdout(&topos(&all, None))
            .lines()
            .map(str::to_string)
            .collect()
    };
    let books = lines(&["--complete"]);
    assert_eq!(books.len(), 66);
    assert_eq!(books[0], "Genesis");
    assert_eq!(lines(&["--complete", ""]), books);
    assert_eq!(lines(&["--list-books"]), books);
    assert_eq!(
        lines(&["--list-books", "-f", "abbreviation"])[..2],
        ["Gn", "Ex"]
    );
    assert_eq!(lines(&["--list-books", "-f", "osis"])[65], "Rev");
    assert_eq!(lines(&["--list-books", "--nt"]).len(), 27);

    let verses = lines(&["--complete", "John 3:"]);
    assert_eq!(verses.len(), 36);
    assert_eq!(verses[15], "John 3:16");
    assert_eq!(
        lines(&["--complete", "jn 3:16-"])[..2],
        ["John 3:16-17", "John 3:16-18"]
    );
    assert_eq!(lines(&["--complete", "jn 3:", "-f", "osis"])[0], "John.3.1");
    assert_eq!(
        lines(&["--complete", "jn 3:", "-f", "abbreviation"])[0],
        "Jn 3:1"
    );
    assert_eq!(
        lines(&["--complete", "John 3:", "-o", "John 3:16-17"]),
        ["John 3:16", "John 3:17"]
    );
    assert_eq!(
        lines(&["--complete", "Rom 8", "-m", "json"]),
        [r#"{"text":"Romans 8","label":"Romans 8","kind":"chapter"}"#]
    );
}
