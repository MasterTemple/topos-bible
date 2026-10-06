use std::io::{self, IsTerminal, Write};

use serde_json::json;
use topos_lib::{
    data::bible_data::BibleData,
    segments::{Passage, Segment, verse_bounds::VerseBounds},
};

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
    total: usize,
}

impl Printer {
    pub fn new(args: &Args, data: BibleData) -> Self {
        let tty = io::stdout().is_terminal();
        let mode = match args.mode {
            _ if args.total_count => OutputMode::TotalCount,
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
            total: 0,
        }
    }

    /// JSON output includes each match's text
    pub fn needs_text(&self) -> bool {
        self.mode == OutputMode::Json
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
                // Hits are in order, so UTF-16 offsets are counted in one pass over the file
                let mut utf16 = Utf16Offsets::default();
                for hit in &file.hits {
                    let text = match (&file.text, &hit.bytes) {
                        (Some(text), Some(bytes)) => Some(&text[bytes.clone()]),
                        _ => None,
                    };
                    let (start_utf16, end_utf16, line_text) = match (&file.text, &hit.bytes) {
                        (Some(text), Some(bytes)) => (
                            Some(utf16.at(text, bytes.start)),
                            Some(utf16.at(text, bytes.end)),
                            Some(line_at(text, bytes.start)),
                        ),
                        _ => (None, None, None),
                    };
                    let value = json!({
                        "path": file.path,
                        "reference": self.reference(&hit.passage),
                        "osis": hit.passage.to_osis(self.data.books()),
                        "book_id": hit.passage.book.0,
                        "book": self.data.books().get_name(hit.passage.book),
                        "segments": segments_json(&hit.passage),
                        "line": hit.position.map(|(s, _)| s.line),
                        "column": hit.position.map(|(s, _)| s.column),
                        "utf16_column": hit.position.map(|(s, _)| s.utf16_column),
                        "end_line": hit.position.map(|(_, e)| e.line),
                        "end_column": hit.position.map(|(_, e)| e.column),
                        "start_byte": hit.bytes.as_ref().map(|b| b.start),
                        "end_byte": hit.bytes.as_ref().map(|b| b.end),
                        "start_utf16": start_utf16,
                        "end_utf16": end_utf16,
                        "line_text": line_text,
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
            OutputMode::TotalCount => self.total += file.hits.len(),
        }
    }

    /// Prints anything that waits for every result (the total and the table)
    pub fn finish(self) {
        if self.mode == OutputMode::TotalCount {
            emit(self.total.to_string());
            return;
        }
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

/// Converts increasing byte offsets in one text to UTF-16 offsets, counting each byte once
#[derive(Default)]
struct Utf16Offsets {
    byte: usize,
    utf16: usize,
}

impl Utf16Offsets {
    fn at(&mut self, text: &str, byte: usize) -> usize {
        if byte < self.byte {
            *self = Self::default();
        }
        self.utf16 += text[self.byte..byte].encode_utf16().count();
        self.byte = byte;
        self.utf16
    }
}

/// The line that contains a byte offset, without its line break
fn line_at(text: &str, byte: usize) -> &str {
    let start = text[..byte].rfind('\n').map_or(0, |i| i + 1);
    let end = text[byte..].find('\n').map_or(text.len(), |i| byte + i);
    text[start..end].trim_end_matches('\r')
}

/// Segments as written, in the same shape as the bindings' `PassageSegment`
fn segments_json(passage: &Passage) -> Vec<serde_json::Value> {
    passage
        .segments
        .iter()
        .map(|seg| {
            let cv = |chapter: u8, verse: u8| json!({ "chapter": chapter, "verse": verse });
            let (sc, sv, ec) = (
                seg.starting_chapter(),
                seg.starting_verse(),
                seg.ending_chapter(),
            );
            match (seg, seg.ending_verse()) {
                (Segment::ChapterVerse(_), _) => {
                    json!({ "tag": "Verses", "start": cv(sc, sv), "end": null })
                }
                (Segment::FullChapter(_), _) => {
                    json!({ "tag": "Chapters", "start": sc, "end": null })
                }
                (Segment::FullChapterRange(_), _) => {
                    json!({ "tag": "Chapters", "start": sc, "end": ec })
                }
                (_, end_verse) => json!({
                    "tag": "Verses",
                    "start": cv(sc, sv),
                    "end": cv(ec, end_verse.unwrap_or_default()),
                }),
            }
        })
        .collect()
}

/// Prints a line, exiting quietly if stdout is closed (like `topos | head`)
fn emit(line: String) {
    let mut out = io::stdout().lock();
    if writeln!(out, "{line}").is_err() {
        std::process::exit(0);
    }
}
