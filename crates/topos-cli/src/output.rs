use std::io::{self, IsTerminal, Write};

use serde_json::json;
use topos_lib::{data::bible_data::BibleData, segments::Passage};

use crate::{
    args::{Args, ColorChoice, OutputMode, ReferenceFormat},
    search::{FileHits, Hit},
};

const PATH: &str = "\x1b[35m";
const LINE: &str = "\x1b[32m";
const MATCH: &str = "\x1b[1;31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Prints results as they arrive (except tables, which are aligned at the end)
pub struct Printer {
    mode: OutputMode,
    color: bool,
    format: ReferenceFormat,
    options: topos_lib::segments::formatter::FormatOptions,
    data: BibleData,
    context: (usize, usize),
    rows: Vec<[String; 5]>,
    printed_group: bool,
}

impl Printer {
    pub fn new(args: &Args, data: BibleData) -> Self {
        let tty = io::stdout().is_terminal();
        let mode = match args.mode {
            OutputMode::Auto if tty => OutputMode::Grouped,
            OutputMode::Auto => OutputMode::Quickfix,
            mode => mode,
        };
        let color = match args.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => tty && std::env::var_os("NO_COLOR").is_none(),
        };
        Self {
            mode,
            color,
            format: args.format,
            options: args.format_options(),
            data,
            context: args.context_lines(),
            rows: vec![],
            printed_group: false,
        }
    }

    pub fn file(&mut self, file: &FileHits) {
        if file.hits.is_empty() {
            return;
        }
        let path = file
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        match self.mode {
            OutputMode::Auto | OutputMode::Grouped => self.grouped(&path, file),
            OutputMode::Quickfix => {
                for hit in &file.hits {
                    let reference = self.reference(&hit.passage);
                    let line = match (hit.position, &hit.label) {
                        (Some((start, _)), None) => {
                            format!("{path}:{}:{}: {reference}", start.line, start.column)
                        }
                        (Some((start, _)), Some(label)) => format!(
                            "{path}:{}:{}: {reference} ({label})",
                            start.line, start.column
                        ),
                        (None, label) => {
                            format!("{path}: {reference} ({})", label.as_deref().unwrap_or(""))
                        }
                    };
                    emit(line);
                }
            }
            OutputMode::Table => {
                for hit in &file.hits {
                    let (line, column) = hit
                        .position
                        .map(|(s, _)| (s.line.to_string(), s.column.to_string()))
                        .unwrap_or_default();
                    let label = hit.label.clone().unwrap_or_default();
                    let reference = self.reference(&hit.passage);
                    self.rows
                        .push([path.clone(), line, column, reference, label]);
                }
            }
            OutputMode::Json => {
                for hit in &file.hits {
                    let text = match (&file.text, &hit.bytes) {
                        (Some(text), Some(bytes)) => Some(&text[bytes.clone()]),
                        _ => None,
                    };
                    let value = json!({
                        "path": file.path,
                        "reference": self.reference(&hit.passage),
                        "osis": hit.passage.to_osis(self.data.books()),
                        "line": hit.position.map(|(s, _)| s.line),
                        "column": hit.position.map(|(s, _)| s.column),
                        "end_line": hit.position.map(|(_, e)| e.line),
                        "end_column": hit.position.map(|(_, e)| e.column),
                        "start_byte": hit.bytes.as_ref().map(|b| b.start),
                        "end_byte": hit.bytes.as_ref().map(|b| b.end),
                        "label": hit.label,
                        "text": text,
                    });
                    emit(value.to_string());
                }
            }
            OutputMode::Count if file.path.is_some() => {
                emit(format!("{path}:{}", file.hits.len()));
            }
            OutputMode::Count => emit(file.hits.len().to_string()),
        }
    }

    /// Prints anything that waits for every result (the table)
    pub fn finish(self) {
        if self.mode != OutputMode::Table || self.rows.is_empty() {
            return;
        }
        let has_labels = self.rows.iter().any(|row| !row[4].is_empty());
        let mut rows = vec![["File", "Line", "Col", "Reference", "Location"].map(String::from)];
        rows.push(["----", "----", "---", "---------", "--------"].map(String::from));
        rows.extend(self.rows);
        let columns = if has_labels { 5 } else { 4 };
        let widths: Vec<usize> = (0..columns)
            .map(|col| {
                rows.iter()
                    .map(|r| r[col].chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for row in rows {
            let cells: Vec<String> = (0..columns)
                .map(|col| format!("{:width$}", row[col], width = widths[col]))
                .collect();
            emit(format!("| {} |", cells.join(" | ")));
        }
    }

    fn reference(&self, passage: &Passage) -> String {
        let written = match self.format {
            ReferenceFormat::Osis => passage.to_osis(self.data.books()),
            _ => self.options.passage(passage, &self.data),
        };
        written.unwrap_or_else(|| passage.segments.to_string())
    }

    fn paint(&self, style: &str, text: &str) -> String {
        if self.color {
            format!("{style}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    fn grouped(&mut self, path: &str, file: &FileHits) {
        if self.printed_group {
            emit(String::new());
        }
        self.printed_group = true;
        if !path.is_empty() {
            emit(self.paint(PATH, path));
        }
        let (before, after) = self.context;
        let show_context = before + after > 0;
        for (idx, hit) in file.hits.iter().enumerate() {
            if show_context && idx > 0 {
                emit(String::from("--"));
            }
            let reference = self.paint(BOLD, &self.reference(&hit.passage));
            let label = hit
                .label
                .as_ref()
                .map(|l| format!(" ({l})"))
                .unwrap_or_default();
            match hit.position {
                Some((start, _)) => {
                    let line = self.paint(LINE, &start.line.to_string());
                    emit(format!("{line}:{}: {reference}{label}", start.column));
                }
                None => emit(format!("{reference}{label}")),
            }
            if show_context && let Some(text) = &file.text {
                self.context_lines(text, hit, before, after);
            }
        }
    }

    /// The lines around a hit, with the matched text highlighted
    fn context_lines(&self, text: &str, hit: &Hit, before: usize, after: usize) {
        let (Some((start, end)), Some(bytes)) = (hit.position, &hit.bytes) else {
            return;
        };
        let first = start.line.saturating_sub(before).max(1);
        let last = end.line + after;
        let mut offset = 0;
        for (idx, line) in text.split_inclusive('\n').enumerate() {
            let number = idx + 1;
            let line_start = offset;
            offset += line.len();
            if number < first {
                continue;
            }
            if number > last {
                break;
            }
            let content = line.trim_end_matches(['\n', '\r']);
            let in_match = (start.line..=end.line).contains(&number);
            let marker = if in_match { ':' } else { '-' };
            let content = if in_match {
                let from = bytes.start.saturating_sub(line_start).min(content.len());
                let to = bytes.end.saturating_sub(line_start).min(content.len());
                format!(
                    "{}{}{}",
                    &content[..from],
                    self.paint(MATCH, &content[from..to]),
                    &content[to..]
                )
            } else {
                content.to_string()
            };
            emit(format!(
                "  {}{marker} {content}",
                self.paint(LINE, &number.to_string())
            ));
        }
    }
}

/// Prints a line, exiting quietly if stdout is closed (like `topos | head`)
fn emit(line: String) {
    let mut out = io::stdout().lock();
    if writeln!(out, "{line}").is_err() {
        std::process::exit(0);
    }
}
