//! Runs the `topos` binary like a user would.

use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn topos(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_topos"))
        .args(args)
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
