use std::{
    io::{self, IsTerminal, Write},
    path::{Component, Path, PathBuf},
};

use serde_json::json;
use topos_bible::segments::formatter::BookStyle;
use topos_bible::{
    data::bible_data::BibleData,
    segments::{Passage, Segment, verse_bounds::VerseBounds},
};
use topos_bible_formats::epub::{LinkStyle, epub_link};

use crate::{
    args::{Args, ColorChoice, OutputMode},
    search::{FileHits, Hit, is_epub},
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
    options: topos_bible::segments::formatter::FormatOptions,
    data: BibleData,
    context: (usize, usize),
    rows: Vec<[String; 5]>,
    printed_group: bool,
    total: usize,
    paths: Option<PathsOnly>,
    /// `--epub-links`
    links: Option<LinkStyle>,
    /// The paths searched, which links are relative to
    roots: Vec<PathBuf>,
}

/// Printing paths instead of references
#[derive(Clone, Copy)]
enum PathsOnly {
    /// `--files`: every file that would be searched
    Files,
    /// `-l`
    WithMatches,
    /// `--files-without-match`
    WithoutMatch,
}

impl Printer {
    pub fn new(args: &Args, data: BibleData, roots: Vec<PathBuf>) -> Self {
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
            options: args.format_options(),
            data,
            context: args.context_lines(),
            rows: vec![],
            printed_group: false,
            total: 0,
            paths: if args.files {
                Some(PathsOnly::Files)
            } else if args.files_with_matches {
                Some(PathsOnly::WithMatches)
            } else if args.files_without_match {
                Some(PathsOnly::WithoutMatch)
            } else {
                None
            },
            links: args.epub_links.map(LinkStyle::from),
            roots,
        }
    }

    /// JSON output includes each match's text, and index entries the text's hash
    pub fn needs_text(&self) -> bool {
        matches!(self.mode, OutputMode::Json | OutputMode::Index)
    }

    /// Prints a file's results; returns whether it counts as found (for the exit code)
    pub fn file(&mut self, file: &FileHits) -> bool {
        let path = file
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // --files, -l, and --files-without-match print only paths
        let list = match self.paths {
            Some(PathsOnly::Files) => true,
            Some(PathsOnly::WithMatches) => !file.hits.is_empty(),
            Some(PathsOnly::WithoutMatch) => file.hits.is_empty(),
            // Every searched file, so a file without references is known to have none
            None if self.mode == OutputMode::Index => {
                emit(index_line(file));
                return !file.hits.is_empty();
            }
            None => {
                if file.hits.is_empty() {
                    return false;
                }
                self.hits(&path, file);
                return true;
            }
        };
        if list {
            let _ = writeln!(io::stdout(), "{}", self.paint(PATH, &path));
        }
        list
    }

    /// The EPUB++ link to a hit, with `--epub-links` (for EPUBs, which have CFIs)
    fn link(&self, file: &FileHits, hit: &Hit) -> Option<String> {
        let style = self.links?;
        let path = file.path.as_deref().filter(|p| is_epub(p))?;
        let cfi = hit.label.as_deref()?;
        let path = link_path(path, &self.roots);
        Some(epub_link(&path, cfi, &self.reference(&hit.passage), style))
    }

    fn hits(&mut self, path: &str, file: &FileHits) {
        let path = path.to_string();
        let mode = self.mode;
        // Links replace the modes that print each reference, except JSON
        if self.links.is_some()
            && !matches!(
                mode,
                OutputMode::Json | OutputMode::Count | OutputMode::TotalCount
            )
        {
            for hit in &file.hits {
                if let Some(link) = self.link(file, hit) {
                    emit(link);
                }
            }
            return;
        }
        match mode {
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
                    let (start_utf16, end_utf16, line_text) =
                        match (&file.text, &hit.bytes, &hit.epub) {
                            (Some(text), Some(bytes), _) => (
                                Some(utf16.at(text, bytes.start)),
                                Some(utf16.at(text, bytes.end)),
                                Some(line_at(text, bytes.start)),
                            ),
                            // EPUBs: in the book's text (each content document's, a blank line
                            // apart), so positions are in book order
                            (_, _, Some(epub)) => (
                                Some(epub.text_utf16.start),
                                Some(epub.text_utf16.end),
                                Some(epub.line_text.as_str()),
                            ),
                            _ => (None, None, None),
                        };
                    let line = hit
                        .position
                        .map(|(s, _)| s.line)
                        .or(hit.epub.as_ref().map(|e| e.line));
                    let utf16_column = hit
                        .position
                        .map(|(s, _)| s.utf16_column)
                        .or(hit.epub.as_ref().map(|e| e.utf16_column));
                    let mut value = json!({
                        "path": file.path,
                        "reference": self.reference(&hit.passage),
                        "osis": hit.passage.to_osis(self.data.books()),
                        "book_id": hit.passage.book.0,
                        "book": self.data.books().get_name(hit.passage.book),
                        "segments": segments_json(&hit.passage),
                        "line": line,
                        "column": hit.position.map(|(s, _)| s.column),
                        "utf16_column": utf16_column,
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
                    if let (Some(epub), serde_json::Value::Object(map)) = (&hit.epub, &mut value) {
                        map.insert(
                            "epub".into(),
                            json!({
                                "spine_index": epub.spine_index,
                                "cfi": hit.label,
                                "chapter": epub.chapter,
                            }),
                        );
                    }
                    let value = match (value, self.link(file, hit)) {
                        (serde_json::Value::Object(mut map), Some(link)) => {
                            map.insert("link".into(), link.into());
                            serde_json::Value::Object(map)
                        }
                        (value, _) => value,
                    };
                    emit(value.to_string());
                }
            }
            OutputMode::Count if file.path.is_some() => {
                emit(format!("{path}:{}", file.hits.len()));
            }
            OutputMode::Count => emit(file.hits.len().to_string()),
            OutputMode::TotalCount => self.total += file.hits.len(),
            OutputMode::Index => emit(index_line(file)),
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
        let written = match self.options.book {
            BookStyle::Osis => passage.to_osis(self.data.books()),
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

/// A file's `-m index` line: its path, and its entry for topos-bible-index (base64)
fn index_line(file: &FileHits) -> String {
    use base64::Engine;
    use std::time::{SystemTime, UNIX_EPOCH};
    use topos_bible_index::{
        EntryMessage, EpubEntryBuilder, EpubRef, FileEntry, Reference, Stamp, Unit, detail_name,
        text_hash,
    };

    let path = file
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let millis = |time: SystemTime| {
        time.duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    };
    let metadata = file.path.as_ref().and_then(|p| std::fs::metadata(p).ok());
    let size = metadata
        .as_ref()
        .map(|m| m.len())
        .or(file.text.as_ref().map(|t| t.len() as u64))
        .unwrap_or_default();
    let mtime = metadata.and_then(|m| m.modified().ok()).map_or(0, millis);
    let written = millis(SystemTime::now());
    let is_book = file.path.as_deref().is_some_and(is_epub);
    let (entry, detail) = if is_book {
        let mut builder = EpubEntryBuilder::default();
        for hit in &file.hits {
            let Some(epub) = &hit.epub else {
                continue;
            };
            builder.push(EpubRef {
                passage: hit.passage.clone(),
                start: epub.text_utf16.start as u32,
                end: epub.text_utf16.end as u32,
                line: epub.line as u32,
                column: epub.utf16_column as u32,
                spine: epub.spine_index as u32,
                chapter: epub.chapter.as_deref(),
                cfi: hit.label.clone().unwrap_or_default(),
                line_text: &epub.line_text,
            });
        }
        let stamp = Stamp {
            size,
            mtime,
            hash: None,
        };
        let (entry, detail) = builder.finish(stamp, written, detail_name(&path, &stamp));
        // A book without references needs no details
        if entry.refs.is_empty() {
            (
                FileEntry {
                    detail: None,
                    ..entry
                },
                None,
            )
        } else {
            (entry, Some(detail))
        }
    } else {
        let text = file.text.as_deref();
        let mut utf16 = Utf16Offsets::default();
        let refs = file
            .hits
            .iter()
            .filter_map(|hit| {
                let (text, bytes, (start, _)) = (text?, hit.bytes.as_ref()?, hit.position?);
                let from = utf16.at(text, bytes.start) as u32;
                let to = utf16.at(text, bytes.end) as u32;
                Some(Reference {
                    passage: hit.passage.clone(),
                    start: from,
                    len: to - from,
                    line: start.line as u32,
                    column: start.utf16_column as u32,
                    section: None,
                })
            })
            .collect();
        let stamp = Stamp {
            size,
            mtime,
            hash: text.map(text_hash),
        };
        (FileEntry::new(stamp, written, refs), None)
    };
    let message = EntryMessage {
        path: path.clone(),
        unit: Unit::Utf16,
        entry,
        detail,
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(message.encode());
    json!({ "path": path, "entry": encoded }).to_string()
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

/// A file's path relative to the searched path it was found under, with forward slashes (a file
/// named on the command line is just its name)
fn link_path(path: &Path, roots: &[PathBuf]) -> String {
    let relative = roots
        .iter()
        .find_map(|root| {
            if path == root {
                path.file_name().map(Path::new)
            } else {
                path.strip_prefix(root)
                    .ok()
                    .filter(|p| !p.as_os_str().is_empty())
            }
        })
        .unwrap_or(path);
    let parts: Vec<_> = relative
        .components()
        .filter(|c| !matches!(c, Component::CurDir | Component::RootDir))
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    // An absolute path (outside every root) keeps its leading slash
    let root = if relative.has_root() { "/" } else { "" };
    format!("{root}{}", parts.join("/"))
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
/// Prints a line; when the reader has gone (`topos ... | head`), exits quietly instead of
/// panicking
pub fn emit(line: String) {
    let mut out = io::stdout().lock();
    if writeln!(out, "{line}").is_err() {
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_paths_are_relative_to_the_root() {
        let roots = [
            PathBuf::from("."),
            PathBuf::from("vault"),
            PathBuf::from("one.epub"),
        ];
        let path = |p: &str| link_path(Path::new(p), &roots);
        assert_eq!(path("./Books/Moby Dick.epub"), "Books/Moby Dick.epub");
        assert_eq!(path("vault/Books/a.epub"), "Books/a.epub");
        assert_eq!(path("one.epub"), "one.epub");
        let roots = [PathBuf::from("dir/two.epub")];
        assert_eq!(link_path(Path::new("dir/two.epub"), &roots), "two.epub");
        assert_eq!(link_path(Path::new("/b/c.epub"), &roots), "/b/c.epub");
    }
}
