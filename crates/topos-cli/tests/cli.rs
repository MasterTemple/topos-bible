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
        refs(&["-o", "Romans 9", "--exclude-overlap", "Romans 9:2"]),
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
            "total-count",
            "index"
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

#[test]
fn merge_and_remove_data() {
    let home = scratch("merge-data");
    std::fs::create_dir_all(home.join("topos")).unwrap();
    std::fs::write(
        home.join("merge.json"),
        r#"{ "books": [{ "book": "John", "abbreviations": ["jhn"] }, { "id": 67, "book": "Tobit", "abbreviation": "Tob" }] }"#,
    )
    .unwrap();
    std::fs::write(
        home.join("remove.json"),
        r#"{ "books": [{ "book": "Jude" }, { "book": "Romans", "abbreviations": ["rom"] }] }"#,
    )
    .unwrap();
    // Both can be set in config.toml (a list or one path)
    let config = format!(
        "merge-data = [{:?}]\nremove-data = {:?}\nmode = \"quickfix\"\n",
        home.join("merge.json"),
        home.join("remove.json")
    );
    std::fs::write(home.join("topos/config.toml"), config).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(args)
            .env("XDG_CONFIG_HOME", &home)
            .env("XDG_CACHE_HOME", home.join("cache"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let text = "Jhn 3:16, Tobit 1:1, Jude 5, Rom 8:28, Romans 8:28, Gen 1:1";
    let found = stdout(&run(&["--text", text]));
    assert_eq!(
        found, ":1:1: John 3:16\n:1:11: Tobit 1:1\n:1:40: Romans 8:28\n:1:53: Genesis 1:1\n",
        "{found}"
    );
    assert_eq!(stdout(&run(&["--list-books"])).lines().count(), 66);

    // A name for two books is an error that says which file and name
    std::fs::write(
        home.join("bad.json"),
        r#"{ "books": [{ "book": "John", "abbreviations": ["gen"] }] }"#,
    )
    .unwrap();
    let bad_path = home.join("bad.json");
    let bad = run(&["--text", "x", "--merge-data", bad_path.to_str().unwrap()]);
    assert_eq!(bad.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("bad.json") && stderr.contains("`gen` would mean both"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn path_options() {
    let dir = scratch("paths");
    let write = |path: &str, text: &[u8]| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("a.md", b"John 3:16\n");
    write("notes/b.txt", b"Romans 8:28\n");
    write("notes/deep/c.md", b"Genesis 1:1\n");
    write("ignored.md", b"Jude 5\n");
    write("skip.md", b"Ruth 1:1\n");
    write(".hidden.md", b"Psalm 23\n");
    write("bin.dat", b"Mark 1:1\x00\n");
    write(
        "big.md",
        format!("Acts 2:38\n{}", " ".repeat(4000)).as_bytes(),
    );
    write("none.md", b"no references here\n");
    write(".gitignore", b"ignored.md\n");
    write(".toposignore", b"skip.md\n");
    // .gitignore only counts inside a Git repository, like ripgrep (see --no-require-git)
    std::fs::create_dir_all(dir.join(".git")).unwrap();

    let run = |args: &[&str]| -> (Vec<String>, Option<i32>) {
        let output = Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(["--no-config", "--sort"])
            .args(args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let mut lines: Vec<String> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| line.trim_start_matches("./").to_string())
            .collect();
        lines.sort();
        (lines, output.status.code())
    };
    let files = |args: &[&str]| {
        let mut all = vec!["--files", "."];
        all.extend(args);
        run(&all).0
    };

    // The defaults skip ignored, hidden, and binary files
    let default = [
        "a.md",
        "big.md",
        "bin.dat",
        "none.md",
        "notes/b.txt",
        "notes/deep/c.md",
    ];
    assert_eq!(files(&[]), default);
    assert_eq!(
        run(&["-l", "."]).0,
        ["a.md", "big.md", "notes/b.txt", "notes/deep/c.md"]
    );
    assert_eq!(run(&["--files-without-match", "."]).0, ["none.md"]);

    assert!(files(&["--hidden"]).contains(&".hidden.md".to_string()));
    assert!(files(&["-."]).contains(&".hidden.md".to_string()));
    let no_ignore = files(&["--no-ignore"]);
    assert!(
        no_ignore.contains(&"ignored.md".to_string()) && no_ignore.contains(&"skip.md".to_string())
    );
    let no_vcs = files(&["--no-ignore-vcs"]);
    assert!(no_vcs.contains(&"ignored.md".to_string()) && !no_vcs.contains(&"skip.md".to_string()));
    write("extra-ignore", b"none.md\n");
    assert!(!files(&["--ignore-file", "extra-ignore"]).contains(&"none.md".to_string()));

    // -u stacks: no ignore files, then hidden files, then binary files
    assert!(files(&["-u"]).contains(&"ignored.md".to_string()));
    assert!(!files(&["-u"]).contains(&".hidden.md".to_string()));
    assert!(files(&["-uu"]).contains(&".hidden.md".to_string()));
    assert!(!run(&["-l", "-uu", "."]).0.contains(&"bin.dat".to_string()));
    assert!(run(&["-l", "-uuu", "."]).0.contains(&"bin.dat".to_string()));
    assert!(
        run(&["-l", "--binary", "."])
            .0
            .contains(&"bin.dat".to_string())
    );

    // Like ripgrep, globs override the ignore rules: a matching file is searched even if it is
    // hidden or ignored
    assert_eq!(
        files(&["--glob", "*.md", "--glob", "!big.md"]),
        [
            ".hidden.md",
            "a.md",
            "ignored.md",
            "none.md",
            "notes/deep/c.md",
            "skip.md"
        ]
    );
    assert_eq!(
        files(&["--glob", "!*.md"]),
        ["bin.dat", "extra-ignore", "notes/b.txt"]
    );
    assert_eq!(
        files(&["--iglob", "*.MD", "--max-depth", "1", "--no-ignore"]),
        [
            ".hidden.md",
            "a.md",
            "big.md",
            "ignored.md",
            "none.md",
            "skip.md"
        ]
    );
    assert_eq!(
        files(&["--exclude-ext", "md,dat"]),
        ["extra-ignore", "notes/b.txt"]
    );
    assert_eq!(files(&["-d", "0"]), Vec::<String>::new());
    assert!(!files(&["--max-filesize", "1K"]).contains(&"big.md".to_string()));
    assert!(files(&["--max-filesize", "1M"]).contains(&"big.md".to_string()));

    // Files named directly are always searched
    assert_eq!(run(&["-l", "ignored.md"]).0, ["ignored.md"]);
    // Exit codes: 1 when nothing is listed
    assert_eq!(run(&["-l", "none.md"]).1, Some(1));
    let bad = run(&["--max-filesize", "lots", "."]);
    assert_eq!(bad.1, Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn passage_filters() {
    let text = "John 3, John 2-4, John 3:14-18, John 2; 3:16, Jn 3:16, John 3:16-17";
    let run = |args: &[&str]| {
        let mut all = vec!["--no-config", "--text", text, "-m", "quickfix"];
        all.extend(args);
        stdout(&topos(&all, None))
            .lines()
            .map(|line| line.split(": ").nth(1).unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let all = [
        "John 3",
        "John 2-4",
        "John 3:14-18",
        "John 2; 3:16",
        "John 3:16",
        "John 3:16-17",
    ];
    assert_eq!(run(&["--any-overlap", "John 3:16"]), all);
    // The old name still works
    assert_eq!(run(&["--overlaps", "John 3:16"]), all);
    // -o: whole chapters don't count
    assert_eq!(
        run(&["-o", "John 3:16"]),
        ["John 3:14-18", "John 2; 3:16", "John 3:16", "John 3:16-17"]
    );
    // John 2; 3:16 also has chapter 2, so only Jn 3:16 is exactly John 3:16
    assert_eq!(run(&["--exact-overlap", "John 3:16"]), ["John 3:16"]);
    assert_eq!(run(&["--exact-overlap", "John 3:16-17"]), ["John 3:16-17"]);
    // Inclusions add up; exclusions win
    assert_eq!(
        run(&[
            "--exact-overlap",
            "John 3",
            "-i",
            "John 3:14-17",
            "--exclude-overlap",
            "John 3:15"
        ]),
        // John 3 is exact, but shares 3:15, and exclusions win
        ["John 3:16", "John 3:16-17"]
    );
    // The old name for --exclude-overlap: every reference here shares John 3:16
    assert_eq!(run(&["--outside", "John 3:16"]), Vec::<String>::new());

    // Old names work in config.toml too
    let configured = topos_with_config(
        Some("overlaps = \"John 3:16\"\noutside = \"John 2\"\nmode = \"quickfix\"\n"),
        &["--text", text],
        None,
    );
    assert_eq!(
        stdout(&configured).lines().count(),
        4,
        "{}",
        stdout(&configured)
    );
}

#[test]
fn passage_format_options() {
    let text = "John 3:16, 17, 18; Jude 1:5";
    let run = |config: Option<&str>, args: &[&str]| {
        let mut all = vec!["--text", text, "-m", "quickfix"];
        all.extend(args);
        stdout(&topos_with_config(config, &all, None))
            .lines()
            .map(|line| line.split(": ").nth(1).unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(run(None, &["--no-config"]), ["John 3:16,17,18", "Jude 1:5"]);
    assert_eq!(
        run(
            None,
            &[
                "--no-config",
                "--fmt-join-adjacent",
                "--fmt-chapter-in-single-chapter-books=false"
            ]
        ),
        ["John 3:16-18", "Jude 5"]
    );
    // The whole object, then single fields and -f on top of it
    let json = r#"{"join_adjacent": true, "book": "abbreviation", "chapter_verse": "."}"#;
    assert_eq!(
        run(None, &["--no-config", "--psg-fmt", json]),
        ["Jn 3.16-18", "Jude 1.5"]
    );
    assert_eq!(
        run(
            None,
            &[
                "--no-config",
                "--psg-fmt",
                json,
                "--fmt-chapter-verse",
                ":",
                "-f",
                "name"
            ]
        ),
        ["John 3:16-18", "Jude 1:5"]
    );

    // In config.toml: a table, and false for yes/no options
    let config = "psg-fmt = { join_adjacent = true, range = \"\u{2013}\" }\n\
                  fmt-chapter-in-single-chapter-books = false\n";
    assert_eq!(run(Some(config), &[]), ["John 3:16\u{2013}18", "Jude 5"]);
    // The command line still overrides the config
    assert_eq!(
        run(Some(config), &["--fmt-join-adjacent=false"]),
        ["John 3:16,17,18", "Jude 5"]
    );

    // Completions use the format too (with a separator wider than a byte)
    let completions = stdout(&topos_with_config(
        Some(config),
        &["--complete", "jn 3:16-"],
        None,
    ));
    assert_eq!(completions.lines().next(), Some("John 3:16\u{2013}17"));

    let bad = topos(
        &[
            "--no-config",
            "--text",
            "x",
            "--psg-fmt",
            r#"{"joins": true}"#,
        ],
        None,
    );
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown field `joins`"));
}

/// The shared completion cases (topos-lib's tests/cases/complete.txt), through --complete
#[test]
fn completion_cases() {
    let cases = include_str!("../../topos-lib/tests/cases/complete.txt");
    let mut failures = vec![];
    for line in cases
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter(|l| !l.starts_with("apply: "))
    {
        let (input, expected) = line.split_once(" =>").unwrap();
        let mut args = vec!["--no-config", "-m", "json"];
        if input.starts_with("[join] ") {
            args.push("--fmt-join-adjacent");
        }
        if input.starts_with("[abbreviation] ") {
            args.extend(["-f", "abbreviation"]);
        }
        let input = input
            .trim_start_matches("[join] ")
            .trim_start_matches("[abbreviation] ");
        args.extend(["--complete", input]);
        let labels: Vec<String> = stdout(&topos(&args, None))
            .lines()
            .map(|l| {
                let value: serde_json::Value = serde_json::from_str(l).unwrap();
                value["label"].as_str().unwrap().to_string()
            })
            .collect();
        let expected: Vec<&str> = expected
            .split(" | ")
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .collect();
        let ok = if expected.is_empty() {
            labels.is_empty()
        } else {
            labels.len() >= expected.len() && labels[..expected.len()] == expected[..]
        };
        if !ok {
            failures.push(format!(
                "{input:?}: expected {expected:?}, got {:?}",
                &labels[..labels.len().min(4)]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A single-chapter book's number is a verse in filters too (`-o "Jude 5"` is Jude 1:5)
#[test]
fn single_chapter_books_in_filters() {
    let run = |args: &[&str]| {
        let mut all = vec![
            "--no-config",
            "--text",
            "Jude 5, Jude 1:6",
            "-m",
            "quickfix",
        ];
        all.extend(args);
        stdout(&topos(&all, None))
    };
    assert_eq!(run(&["-o", "Jude 5"]), ":1:1: Jude 1:5\n");
    assert_eq!(run(&["--exact-overlap", "Jude 6"]), ":1:9: Jude 1:6\n");
}

/// A two-chapter EPUB whose spine starts at /6/2
fn write_epub(path: &std::path::Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let page = |body: &str| {
        format!(
            "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head><body><p id=\"p\">{body}</p></body></html>"
        )
    };
    let files = [
        ("mimetype", "application/epub+zip".to_string()),
        (
            "META-INF/container.xml",
            r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_string(),
        ),
        (
            "content.opf",
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata/><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#.to_string(),
        ),
        ("a.xhtml", page("See Jn 3:16.")),
        ("b.xhtml", page("And Romans 8:28")),
    ];
    for (name, text) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn epub_links() {
    let dir = scratch("epub-links");
    std::fs::create_dir_all(dir.join("Books")).unwrap();
    write_epub(&dir.join("Books/My Book.epub"));
    std::fs::write(dir.join("notes.md"), "John 1:1\n").unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_topos"))
            .args(["--no-config", "--sort"])
            .args(args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap()
    };
    // Only EPUBs, with paths relative to the directory searched
    assert_eq!(
        run(&["--epub-links", "wiki", "."]),
        "[[Books/My Book.epub#epubcfi(/6/2!/4/2,/1:4,/1:11)|John 3:16]]\n\
         [[Books/My Book.epub#epubcfi(/6/4!/4/2,/1:4,/1:15)|Romans 8:28]]\n"
    );
    assert_eq!(
        run(&["--epub-links", "markdown", "-f", "abbrev", "Books"]),
        "[Jn 3:16](My%20Book.epub#epubcfi%28/6/2!/4/2,/1%3A4,/1%3A11%29)\n\
         [Rom 8:28](My%20Book.epub#epubcfi%28/6/4!/4/2,/1%3A4,/1%3A15%29)\n"
    );
    // Assertions, and the CFI as the label in other modes
    let quickfix = run(&["--cfi-assertions", "-m", "quickfix", "Books/My Book.epub"]);
    assert_eq!(
        quickfix.lines().next(),
        Some("Books/My Book.epub: John 3:16 (epubcfi(/6/2[a]!/4/2[p],/1:4,/1:11))")
    );
    // JSON gets a link field; counts are unchanged
    let json = run(&["--epub-links", "wiki", "-m", "json", "."]);
    let first: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert_eq!(
        first["link"],
        "[[Books/My Book.epub#epubcfi(/6/2!/4/2,/1:4,/1:11)|John 3:16]]"
    );
    // Positions in an EPUB are in the book's text: its documents' text, a blank line apart
    let second: serde_json::Value = serde_json::from_str(json.lines().nth(1).unwrap()).unwrap();
    let place = |key: &str| second[key].to_string();
    assert_eq!(
        [
            "start_utf16",
            "end_utf16",
            "line",
            "utf16_column",
            "line_text"
        ]
        .map(place),
        ["18", "29", "3", "5", "\"And Romans 8:28\""]
    );
    assert_eq!(
        second["epub"],
        serde_json::json!({ "spine_index": 1, "cfi": "epubcfi(/6/4!/4/2,/1:4,/1:15)", "chapter": null })
    );
    assert_eq!(
        run(&["--epub-links", "wiki", "-m", "total-count", "."]),
        "2\n"
    );
    // Text isn't searched for links
    let output = topos(&["--no-config", "--epub-links", "wiki"], Some("John 3:16"));
    assert_eq!(output.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn index_entries() {
    use base64::Engine;
    use topos_bible_index::{EntryMessage, Unit, text_hash};

    let dir = scratch("index-entries");
    std::fs::create_dir_all(dir.join("Books")).unwrap();
    write_epub(&dir.join("Books/My Book.epub"));
    std::fs::write(dir.join("a.md"), "é Jn 3:16\nRom 8:28").unwrap();
    std::fs::write(dir.join("empty.md"), "nothing").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_topos"))
        .args(["--no-config", "--sort", "-m", "index", "."])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let lines: Vec<(String, EntryMessage)> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(value["entry"].as_str().unwrap())
                .unwrap();
            let path = value["path"].as_str().unwrap().to_string();
            (path, EntryMessage::decode(&bytes).unwrap())
        })
        .collect();
    // Every searched file, with or without references
    let paths: Vec<&str> = lines.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["./Books/My Book.epub", "./a.md", "./empty.md"]);

    let (_, note) = &lines[1];
    assert_eq!(note.unit, Unit::Utf16);
    assert_eq!(note.entry.stamp.size, 19);
    assert_eq!(
        note.entry.stamp.hash,
        Some(text_hash("é Jn 3:16\nRom 8:28"))
    );
    let refs: Vec<_> = note
        .entry
        .refs
        .iter()
        .map(|r| (r.start, r.len, r.line, r.column))
        .collect();
    assert_eq!(refs, [(2, 7, 1, 3), (10, 8, 2, 1)]);
    assert!(lines[2].1.entry.refs.is_empty());

    // An EPUB's details: its CFIs and the text around each reference
    let (_, book) = &lines[0];
    assert_eq!(book.entry.stamp.hash, None);
    assert_eq!(book.entry.refs.len(), 2);
    let detail = book.detail.as_ref().unwrap();
    assert_eq!(detail.refs[1].cfi, "epubcfi(/6/4!/4/2,/1:4,/1:15)");
    assert_eq!(
        (detail.refs[1].context.as_str(), detail.refs[1].at),
        ("And Romans 8:28", 4)
    );
    assert_eq!(
        book.entry.detail,
        Some(topos_bible_index::detail_name(
            "./Books/My Book.epub",
            &book.entry.stamp
        ))
    );
    let _ = std::fs::remove_dir_all(&dir);
}
