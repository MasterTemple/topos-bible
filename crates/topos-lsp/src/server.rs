use std::{collections::HashMap, path::PathBuf};

use lsp_types::{
    CodeAction, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability, Command,
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    ExecuteCommandOptions, ExecuteCommandParams, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InlayHint, InlayHintLabel, InlayHintParams, Location, MarkupContent,
    MarkupKind, OneOf, Position, PublishDiagnosticsParams, Range, ReferenceParams,
    ServerCapabilities, SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Uri,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use topos_bible::{
    filter::{bible_filter::BibleFilter, filters::testament::TestamentFilter},
    matcher::{BibleMatch, BibleMatcher, LineIndex},
    segments::{
        Passage,
        autocomplete::{CompleteOptions, CompletionKind},
        formatter::{BookStyle, FormatOptions},
    },
};

use crate::{
    settings::{self, Configured, HoverField, InlayHints, ReferenceDiagnostics},
    workspace,
};

/// The command code actions run; its result is the matching locations
pub const SEARCH_COMMAND: &str = "topos.search";

/**
The command reformat actions use in unnamed documents: `{ "edits": [TextEdit] }` for the
editor to apply to the document the action came from
- A workspace edit names its document by URI, and an unnamed document's (`file://`) names no
  buffer an editor can find (Neovim makes a new one), so the edits go with the action instead
*/
pub const APPLY_EDITS_COMMAND: &str = "topos.applyEdits";

/// Whether a URI names no file (an unnamed buffer's `file://`)
pub fn is_unnamed(uri: &Uri) -> bool {
    workspace::uri_to_path(uri).is_none_or(|path| path.as_os_str().is_empty())
}

/// The kinds of workspace search, like the CLI's passage filters
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchMode {
    /// `-o`: references that name a verse of the passage (whole chapters don't count)
    ExplicitOverlap,
    /// `--any-overlap`: references that share any verse with it
    AnyOverlap,
    /// `--exact-overlap`: references that are exactly it (go-to-references)
    ExactOverlap,
    /// `-i`: references entirely inside it
    Inside,
}

impl SearchMode {
    const ALL: [SearchMode; 4] = [
        SearchMode::ExplicitOverlap,
        SearchMode::AnyOverlap,
        SearchMode::ExactOverlap,
        SearchMode::Inside,
    ];

    fn title(self, reference: &str) -> String {
        match self {
            SearchMode::ExplicitOverlap => format!("Search \"{reference}\" for explicit overlap"),
            SearchMode::AnyOverlap => format!("Search \"{reference}\" for any overlap"),
            SearchMode::ExactOverlap => format!("Search \"{reference}\" for exact overlap"),
            SearchMode::Inside => format!("Search inside \"{reference}\""),
        }
    }
}

/// The arguments of [`SEARCH_COMMAND`]
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchArguments {
    pub mode: SearchMode,
    /// The passage, written so it parses back (`John 3:16`)
    pub passage: String,
}

/// Open documents and the matcher, with one method per LSP request or notification
pub struct Server {
    matcher: BibleMatcher,
    /// How references are written: completions, hover, symbols, hints, and action titles
    format: FormatOptions,
    inlay_hints: InlayHints,
    reference_diagnostics: ReferenceDiagnostics,
    /// What the hover shows below the reference
    hover: Vec<HoverField>,
    /// Extensions searched in the workspace (empty for every text file)
    extensions: Vec<String>,
    documents: HashMap<Uri, String>,
    /// The workspace folders, for workspace-wide searches
    roots: Vec<PathBuf>,
    /// The editor's settings (initialization options, then the latest configuration)
    editor: Map<String, Value>,
    /// Each workspace file's references, reused while it is unchanged
    cache: workspace::Cache,
}

impl Server {
    pub fn new(matcher: BibleMatcher) -> Self {
        Self {
            matcher,
            format: FormatOptions::default(),
            inlay_hints: InlayHints::default(),
            reference_diagnostics: ReferenceDiagnostics::default(),
            hover: HoverField::DEFAULT.to_vec(),
            extensions: vec![],
            documents: HashMap::new(),
            roots: vec![],
            editor: Map::new(),
            cache: workspace::Cache::default(),
        }
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    /**
    Applies the config file and the editor's settings (`initializationOptions` or
    `workspace/didChangeConfiguration`'s `settings`, either may nest them under `topos`)
    - On an error the previous settings stay, and the error says why
    */
    pub fn configure(&mut self, editor: Value) -> Result<(), String> {
        let editor = settings::editor_layer(editor);
        let config = settings::config_layer(&editor)?;
        let Configured {
            matcher,
            format,
            inlay_hints,
            reference_diagnostics,
            hover,
            extensions,
        } = settings::settings(config, &editor)?.configure()?;
        self.matcher = matcher;
        self.format = format;
        self.inlay_hints = inlay_hints;
        self.reference_diagnostics = reference_diagnostics;
        self.hover = hover;
        self.extensions = extensions;
        self.editor = editor;
        // New data can change what every file contains
        self.cache.clear();
        Ok(())
    }

    /// The open documents (whose diagnostics change with the settings)
    pub fn documents(&self) -> Vec<Uri> {
        self.documents.keys().cloned().collect()
    }

    pub fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some([" ", ":", "-", ",", ";", "."].map(String::from).to_vec()),
                ..CompletionOptions::default()
            }),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            inlay_hint_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec![SEARCH_COMMAND.into()],
                ..ExecuteCommandOptions::default()
            }),
            ..ServerCapabilities::default()
        }
    }

    pub fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.documents.insert(doc.uri, doc.text);
    }

    /// Only full syncs are advertised, so the last change is the whole document
    pub fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents.insert(params.text_document.uri, change.text);
        }
    }

    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    pub fn completion(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let position = params.text_document_position;
        let text = self.documents.get(&position.text_document.uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(
            position.position.line as usize,
            position.position.character as usize,
        )?;
        let options = CompleteOptions {
            format: self.format.clone(),
            limit: Some(100),
        };
        let items = self
            .matcher
            .complete(text, cursor, &options)
            .into_iter()
            .enumerate()
            .map(|(idx, completion)| {
                let range = completion.edit.range;
                CompletionItem {
                    label: completion.label.clone(),
                    kind: Some(match completion.kind {
                        CompletionKind::Book => CompletionItemKind::FOLDER,
                        CompletionKind::Chapter => CompletionItemKind::MODULE,
                        CompletionKind::Verse => CompletionItemKind::REFERENCE,
                    }),
                    /*
                    What was typed, then the label (`jn 3:1 John 3:16`):
                    - Editors that match from the start (Neovim's own completion) see what was
                      typed, so `jn 3:1` still shows `John 3:16`
                    - Editors that match the word before the cursor (blink.cmp) find it in the
                      label, even against a list made a keystroke ago: with only what was
                      typed, `1 C`'s list filtered `Co` to nothing, so `1 Co` showed none
                    */
                    filter_text: Some(format!("{} {}", &text[range.clone()], completion.label)),
                    // Keep the order: chapters and verses are in reading order
                    sort_text: Some(format!("{idx:05}")),
                    text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit {
                        range: lsp_range(&index, range.start, range.end),
                        new_text: completion.edit.text,
                    })),
                    ..CompletionItem::default()
                }
            })
            .collect();
        // Incomplete: the editor asks again as you type, instead of filtering this list itself
        // (which filtered `1 Ti` against what was typed when it was made, `1 T`, and emptied it)
        Some(CompletionResponse::List(lsp_types::CompletionList {
            is_incomplete: true,
            items,
        }))
    }

    /**
    The document's diagnostics:
    - Warnings for references that do not exist, like `John 3:99` (code `missing`)
    - An information diagnostic on each reference: how it is written in the configured format,
      like `John 3:16` (code `reference`; a hint, or none, with `reference-diagnostics`)
    */
    pub fn diagnostics(&self, uri: &Uri) -> PublishDiagnosticsParams {
        let diagnostics = self.documents.get(uri).map_or_else(Vec::new, |text| {
            let index = LineIndex::new(text);
            let diagnostic = |range, severity, code: &str, message| Diagnostic {
                range,
                severity: Some(severity),
                code: Some(lsp_types::NumberOrString::String(code.into())),
                source: Some(String::from("topos")),
                message,
                ..Diagnostic::default()
            };
            let mut diagnostics: Vec<Diagnostic> = self
                .matcher
                .problems(text)
                .into_iter()
                .map(|problem| {
                    let range = lsp_range(&index, problem.bytes.start, problem.bytes.end);
                    diagnostic(
                        range,
                        DiagnosticSeverity::WARNING,
                        "missing",
                        problem.message,
                    )
                })
                .collect();
            let severity = match self.reference_diagnostics {
                ReferenceDiagnostics::Info => Some(DiagnosticSeverity::INFORMATION),
                ReferenceDiagnostics::Hint => Some(DiagnosticSeverity::HINT),
                ReferenceDiagnostics::Never => None,
            };
            if let Some(severity) = severity {
                for m in self.matcher.search(text) {
                    let bytes = m.location.bytes;
                    let range = lsp_range(&index, bytes.start, bytes.end);
                    diagnostics.push(diagnostic(range, severity, "reference", self.reference(&m)));
                }
                // In document order, so they read top to bottom
                diagnostics.sort_by_key(|d| (d.range.start.line, d.range.start.character));
            }
            diagnostics
        });
        PublishDiagnosticsParams::new(uri.clone(), diagnostics, None)
    }

    /// The document's diagnostics when the editor asks for them (pull diagnostics)
    pub fn pull_diagnostics(
        &self,
        params: lsp_types::DocumentDiagnosticParams,
    ) -> lsp_types::DocumentDiagnosticReportResult {
        let items = self.diagnostics(&params.text_document.uri).diagnostics;
        lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(
                lsp_types::RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: lsp_types::FullDocumentDiagnosticReport {
                        result_id: None,
                        items,
                    },
                },
            ),
        )
    }

    /// The reference at a position, with the document's line index
    fn reference_at(&self, uri: &Uri, position: Position) -> Option<(BibleMatch, LineIndex<'_>)> {
        let text = self.documents.get(uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(position.line as usize, position.character as usize)?;
        let found = self.matcher.search(text).into_iter().find(|m| {
            let bytes = m.location.bytes;
            bytes.start <= cursor && cursor <= bytes.end
        })?;
        Some((found, index))
    }

    /// The reference under the cursor, normalized, with what the `hover` setting lists about it
    /// (its OSIS id, book, genres, ...); or, on a reference that doesn't exist, what does
    /// (`John 3 has 36 verses`)
    pub fn hover(&self, params: HoverParams) -> Option<Hover> {
        let position = params.text_document_position_params;
        let uri = &position.text_document.uri;
        if let Some(hover) = self.problem_hover(uri, position.position) {
            return Some(hover);
        }
        let (found, index) = self.reference_at(uri, position.position)?;
        let bytes = found.location.bytes;
        let written = self
            .documents
            .get(uri)
            .and_then(|text| text.get(bytes.start..bytes.end))
            .unwrap_or_default();
        let title = self.reference(&found);
        let mut value = format!("**{title}**");
        let lines: Vec<String> = self
            .hover
            .iter()
            .filter_map(|&field| {
                let (label, detail) = self.hover_line(field, &found, written)?;
                // The full name or abbreviation is left out when it is the title already
                let repeats =
                    matches!(field, HoverField::Name | HoverField::Abbreviation) && detail == title;
                (!repeats).then(|| format!("- **{label}**: {detail}"))
            })
            .collect();
        if !lines.is_empty() {
            value.push_str("\n\n");
            value.push_str(&lines.join("\n"));
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(lsp_range(&index, bytes.start, bytes.end)),
        })
    }

    /// One hover line's label and text, or [`None`] when there is nothing to say
    fn hover_line(
        &self,
        field: HoverField,
        found: &BibleMatch,
        written: &str,
    ) -> Option<(&'static str, String)> {
        let data = self.matcher.data();
        let psg = &found.psg;
        let styled = |book| {
            FormatOptions {
                book,
                ..self.format.clone()
            }
            .passage(psg, data)
        };
        let versification = data.chapter_verses().get_chapter_verses(&psg.book);
        Some(match field {
            HoverField::Written => ("Written", format!("`{written}`")),
            HoverField::Name => ("Name", styled(BookStyle::Name)?),
            HoverField::Abbreviation => ("Abbreviation", styled(BookStyle::Abbreviation)?),
            HoverField::Osis => ("OSIS", format!("`{}`", psg.to_osis(data.books())?)),
            HoverField::Book => {
                let name = data.books().get_name(psg.book)?;
                let books = data.books().ids().count();
                let mut detail = format!("{name}, book {} of {books}", psg.book.0);
                if let Some(chapters) = versification.map(|v| v.get_chapter_count()) {
                    let plural = if chapters == 1 { "" } else { "s" };
                    detail.push_str(&format!(", {chapters} chapter{plural}"));
                }
                ("Book", detail)
            }
            HoverField::Testament => {
                let testament = if TestamentFilter::Old.contains(psg.book) {
                    "Old Testament"
                } else if TestamentFilter::New.contains(psg.book) {
                    "New Testament"
                } else {
                    return None;
                };
                ("Testament", testament.to_string())
            }
            HoverField::Genres => {
                let genres: Vec<&str> = data
                    .genres()
                    .iter()
                    .filter(|genre| genre.books().contains(&psg.book))
                    .map(|genre| genre.name())
                    .collect();
                let label = if genres.len() == 1 { "Genre" } else { "Genres" };
                (label, (!genres.is_empty()).then(|| genres.join(", "))?)
            }
            HoverField::Verses => {
                let mut verses = psg.verses(versification);
                verses.sort();
                verses.dedup();
                let ranges: Vec<String> = psg
                    .ranges(versification)
                    .iter()
                    .map(|range| {
                        let (start, end) = (range.start, range.end);
                        let cv = &self.format.chapter_verse;
                        let to = &self.format.range;
                        if end.verse == 0 {
                            start.chapter.to_string()
                        } else if start == end {
                            format!("{}{cv}{}", start.chapter, start.verse)
                        } else if start.chapter == end.chapter {
                            format!("{}{cv}{}{to}{}", start.chapter, start.verse, end.verse)
                        } else {
                            format!(
                                "{}{cv}{}{to}{}{cv}{}",
                                start.chapter, start.verse, end.chapter, end.verse
                            )
                        }
                    })
                    .collect();
                let count = if verses.is_empty() {
                    String::from("?")
                } else {
                    verses.len().to_string()
                };
                (
                    "Verses",
                    format!("{count} ({})", ranges.join(&self.format.chapter_separator)),
                )
            }
            HoverField::Bcv => {
                let keys: Vec<String> = psg
                    .bcv_ranges()
                    .into_iter()
                    .map(|range| match range.start() == range.end() {
                        true => format!("`{}`", range.start()),
                        false => format!("`{}-{}`", range.start(), range.end()),
                    })
                    .collect();
                ("BCV", keys.join(", "))
            }
            HoverField::Location => {
                let (start, end) = (found.location.start, found.location.end);
                // Columns are 1-based characters; the end is the last character's
                let last = end.char_column.saturating_sub(1).max(1);
                let detail = if start.line == end.line {
                    format!("line {}, columns {}-{last}", start.line, start.char_column)
                } else {
                    format!(
                        "line {}, column {} to line {}, column {last}",
                        start.line, start.char_column, end.line
                    )
                };
                ("Location", detail)
            }
        })
    }

    fn problem_hover(&self, uri: &Uri, position: Position) -> Option<Hover> {
        let text = self.documents.get(uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(position.line as usize, position.character as usize)?;
        let problem = self
            .matcher
            .problems(text)
            .into_iter()
            .find(|p| p.bytes.start <= cursor && cursor <= p.bytes.end)?;
        let value = match &problem.detail {
            Some(detail) => format!("**{}**\n\n{detail}", problem.message),
            None => format!("**{}**", problem.message),
        };
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(lsp_range(&index, problem.bytes.start, problem.bytes.end)),
        })
    }

    /// Every reference in the document, for outlines and pickers
    pub fn document_symbols(&self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
        let text = self.documents.get(&params.text_document.uri)?;
        let index = LineIndex::new(text);
        let symbols = self
            .matcher
            .search(text)
            .into_iter()
            .map(|m| {
                let bytes = m.location.bytes;
                let range = lsp_range(&index, bytes.start, bytes.end);
                #[allow(deprecated)]
                DocumentSymbol {
                    name: self.reference(&m),
                    detail: m.psg.to_osis(self.matcher.data().books()),
                    kind: SymbolKind::KEY,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                }
            })
            .collect();
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    /// A hint after each reference in the range: how it is written in the configured format
    /// (where that differs, by default), or its OSIS id
    pub fn inlay_hints(&self, params: InlayHintParams) -> Option<Vec<InlayHint>> {
        if self.inlay_hints == InlayHints::Never {
            return Some(vec![]);
        }
        let text = self.documents.get(&params.text_document.uri)?;
        let index = LineIndex::new(text);
        let hints = self
            .matcher
            .search(text)
            .into_iter()
            .filter_map(|m| {
                let bytes = m.location.bytes;
                let end = lsp_position(&index, bytes.end);
                let start = lsp_position(&index, bytes.start);
                if end < params.range.start || start > params.range.end {
                    return None;
                }
                let label = match self.inlay_hints {
                    InlayHints::Osis => m.psg.to_osis(self.matcher.data().books())?,
                    InlayHints::Changed if self.reference(&m) == text[bytes.start..bytes.end] => {
                        return None;
                    }
                    _ => self.reference(&m),
                };
                Some(InlayHint {
                    position: end,
                    label: InlayHintLabel::String(label),
                    kind: None,
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                })
            })
            .collect();
        Some(hints)
    }

    /**
    For the reference under the cursor:
    - Reformat it, if it isn't written the way completion would write it (and, when several
      references in the document are, reformat them all)
    - One search of the workspace per [`SearchMode`]
    */
    pub fn code_actions(&self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        let (found, index) = self.reference_at(uri, params.range.start)?;
        let reference = self.reference(&found);
        let passage = self.parseable(&found.psg);
        let mut actions = self.reformat_actions(uri, &found, &index);
        let searches = SearchMode::ALL
            .into_iter()
            .map(|mode| {
                let title = mode.title(&reference);
                let arguments = SearchArguments {
                    mode,
                    passage: passage.clone(),
                };
                CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.clone(),
                    command: Some(Command {
                        title,
                        command: SEARCH_COMMAND.into(),
                        arguments: Some(vec![serde_json::to_value(arguments).ok()?]),
                    }),
                    ..CodeAction::default()
                })
                .into()
            })
            .collect::<Option<Vec<CodeActionOrCommand>>>()?;
        actions.extend(searches);
        Some(actions)
    }

    /// `Reformat as "John 3:16"` for this reference, and for every reference in the document
    /// that is written differently, when there are several
    // `WorkspaceEdit::changes` is keyed by `Uri`, which clippy counts as a mutable key
    #[allow(clippy::mutable_key_type)]
    fn reformat_actions(
        &self,
        uri: &Uri,
        found: &BibleMatch,
        index: &LineIndex,
    ) -> Vec<CodeActionOrCommand> {
        let Some(text) = self.documents.get(uri) else {
            return vec![];
        };
        // Each reference written differently from the configured format, with its rewrite
        let rewrite = |m: &BibleMatch| {
            let bytes = m.location.bytes;
            let formatted = self.reference(m);
            (text[bytes.start..bytes.end] != formatted).then(|| TextEdit {
                range: lsp_range(index, bytes.start, bytes.end),
                new_text: formatted,
            })
        };
        let unnamed = is_unnamed(uri);
        let action = |title: String, edits: Vec<TextEdit>| {
            let (edit, command) = if unnamed {
                let arguments = serde_json::json!({ "edits": edits });
                let command = Command::new(
                    title.clone(),
                    APPLY_EDITS_COMMAND.into(),
                    Some(vec![arguments]),
                );
                (None, Some(command))
            } else {
                let changes = HashMap::from([(uri.clone(), edits)]);
                (Some(lsp_types::WorkspaceEdit::new(changes)), None)
            };
            CodeActionOrCommand::CodeAction(CodeAction {
                title,
                kind: Some(lsp_types::CodeActionKind::REFACTOR_REWRITE),
                edit,
                command,
                ..CodeAction::default()
            })
        };
        let mut actions = vec![];
        if let Some(edit) = rewrite(found) {
            actions.push(action(
                format!("Reformat as \"{}\"", edit.new_text),
                vec![edit],
            ));
        }
        let all: Vec<TextEdit> = self
            .matcher
            .search(text)
            .iter()
            .filter_map(rewrite)
            .collect();
        if all.len() > 1 {
            let title = format!("Reformat all {} references in this file", all.len());
            actions.push(action(title, all));
        }
        actions
    }

    /// Go to references: every reference in the workspace that is exactly the one under the
    /// cursor (`--exact-overlap`), however it is written
    pub fn references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let position = params.text_document_position;
        let (found, _) = self.reference_at(&position.text_document.uri, position.position)?;
        self.search(SearchMode::ExactOverlap, &self.parseable(&found.psg))
            .ok()
    }

    /// Runs [`SEARCH_COMMAND`], returning the locations it finds
    pub fn execute_command(&self, params: ExecuteCommandParams) -> Result<Vec<Location>, String> {
        if params.command != SEARCH_COMMAND {
            return Err(format!("unknown command {}", params.command));
        }
        let argument = params
            .arguments
            .into_iter()
            .next()
            .ok_or("topos.search needs { mode, passage }")?;
        let arguments: SearchArguments =
            serde_json::from_value(argument).map_err(|e| e.to_string())?;
        self.search(arguments.mode, &arguments.passage)
    }

    /// The references in the workspace that a search keeps, in file order
    pub fn search(&self, mode: SearchMode, passage: &str) -> Result<Vec<Location>, String> {
        let mut filter = BibleFilter::new(self.matcher.data().clone());
        match mode {
            SearchMode::ExplicitOverlap => filter.filter_explicit_overlap(passage),
            SearchMode::AnyOverlap => filter.filter_any_overlap(passage),
            SearchMode::ExactOverlap => filter.filter_exact_overlap(passage),
            SearchMode::Inside => filter.filter_inside(passage),
        }
        .map_err(|e| e.to_string())?;
        let query = filter.create_matcher();
        let mut files = workspace::references(
            &self.roots,
            &self.documents,
            &self.extensions,
            &self.matcher,
            &self.cache,
        );
        files.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        let locations = files
            .iter()
            .flat_map(|(uri, found)| {
                found
                    .iter()
                    .filter(|f| query.keeps(&f.passage))
                    .map(|f| Location::new(uri.clone(), f.range))
            })
            .collect();
        Ok(locations)
    }

    /// The reference in the configured format (OSIS references are written as OSIS ids)
    fn reference(&self, found: &BibleMatch) -> String {
        let data = self.matcher.data();
        let written = match self.format.book {
            BookStyle::Osis => found.psg.to_osis(data.books()),
            _ => self.format.passage(&found.psg, data),
        };
        written.unwrap_or_else(|| found.psg.segments.to_string())
    }

    /// The passage written so the filters can read it back, whatever the configured format
    fn parseable(&self, passage: &Passage) -> String {
        FormatOptions::default()
            .passage(passage, self.matcher.data())
            .unwrap_or_else(|| passage.segments.to_string())
    }
}

fn lsp_position(index: &LineIndex, offset: usize) -> Position {
    let position = index.position(offset);
    Position::new(position.line as u32 - 1, position.utf16_column as u32 - 1)
}

pub fn lsp_range(index: &LineIndex, start: usize, end: usize) -> Range {
    Range::new(lsp_position(index, start), lsp_position(index, end))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use std::path::Path;

    use lsp_types::{
        PartialResultParams, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
        WorkDoneProgressParams,
    };

    use serde_json::json;

    use super::*;

    fn server(text: &str) -> (Server, Uri) {
        let uri = Uri::from_str("file:///notes.md").unwrap();
        let mut server = Server::new(BibleMatcher::default());
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri.clone(), "markdown".into(), 1, text.into()),
        });
        (server, uri)
    }

    fn at(uri: &Uri, line: u32, character: u32) -> TextDocumentPositionParams {
        TextDocumentPositionParams::new(
            TextDocumentIdentifier::new(uri.clone()),
            Position::new(line, character),
        )
    }

    /// The shared completion cases (topos-lib's tests/cases/complete.txt), as an editor asks
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
            let (mut server, uri) = server(
                input
                    .trim_start_matches("[join] ")
                    .trim_start_matches("[abbreviation] "),
            );
            let format = serde_json::json!({
                "join_adjacent": input.starts_with("[join] "),
                "book": if input.starts_with("[abbreviation] ") { "abbreviation" } else { "name" },
            });
            server
                .configure(serde_json::json!({ "no-config": true, "psg-fmt": format }))
                .unwrap();
            let input = input
                .trim_start_matches("[join] ")
                .trim_start_matches("[abbreviation] ");
            let character = input.encode_utf16().count() as u32;
            let labels: Vec<String> = match server.completion(CompletionParams {
                text_document_position: at(&uri, 0, character),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: None,
            }) {
                Some(CompletionResponse::List(list)) => {
                    let items = list.items;
                    items.into_iter().map(|item| item.label).collect()
                }
                _ => vec![],
            };
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
                failures.push(format!("{input:?}: expected {expected:?}, got {labels:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// The shared `apply:` completion cases (`|` is the cursor), as an editor asks
    #[test]
    fn applied_completion_cases() {
        let cases = include_str!("../../topos-lib/tests/cases/complete.txt");
        let mut failures = vec![];
        for case in cases.lines().filter_map(|l| l.strip_prefix("apply: ")) {
            let (input, expected) = case.split_once(" => ").unwrap();
            let (input, expected) = (input.replace("\\n", "\n"), expected.replace("\\n", "\n"));
            let (input, expected) = (input.as_str(), expected.as_str());
            let character = input[..input.find('|').unwrap()].encode_utf16().count() as u32;
            let text = input.replacen('|', "", 1);
            let (server, uri) = server(&text);
            let first = match server.completion(CompletionParams {
                text_document_position: at(&uri, 0, character),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: None,
            }) {
                Some(CompletionResponse::List(list)) => list.items.into_iter().next(),
                _ => None,
            };
            let applied = first.and_then(|item| match item.text_edit? {
                lsp_types::CompletionTextEdit::Edit(edit) => {
                    let index = LineIndex::new(&text);
                    let start = index.offset_of_utf16(0, edit.range.start.character as usize)?;
                    let end = index.offset_of_utf16(0, edit.range.end.character as usize)?;
                    let mut applied = text.clone();
                    applied.replace_range(start..end, &edit.new_text);
                    Some(applied)
                }
                lsp_types::CompletionTextEdit::InsertAndReplace(_) => None,
            });
            if applied.as_deref() != Some(expected) {
                failures.push(format!("{input:?}: expected {expected:?}, got {applied:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn completes_with_utf16_ranges() {
        // `𝄞` is 2 UTF-16 units, so `jn` starts at character 3
        let (server, uri) = server("intro\n𝄞 jn 3:1");
        let Some(CompletionResponse::List(lsp_types::CompletionList {
            items,
            is_incomplete: true,
        })) = server.completion(CompletionParams {
            text_document_position: at(&uri, 1, 9),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        })
        else {
            panic!("no completions");
        };
        let first = &items[0];
        assert_eq!(first.label, "John 3:1");
        assert_eq!(first.filter_text.as_deref(), Some("jn 3:1 John 3:1"));
        let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &first.text_edit else {
            panic!("no edit");
        };
        assert_eq!(
            edit.range,
            Range::new(Position::new(1, 3), Position::new(1, 9))
        );
    }

    #[test]
    fn diagnostics_for_references_and_missing_verses() {
        let (mut server, uri) = server("Fine: jn 3:16\nBad: John 3:99");
        let summary = |server: &Server| -> Vec<(Option<DiagnosticSeverity>, String, Range)> {
            server
                .diagnostics(&uri)
                .diagnostics
                .into_iter()
                .map(|d| (d.severity, d.message, d.range))
                .collect()
        };
        assert_eq!(
            summary(&server),
            [
                (
                    Some(DiagnosticSeverity::INFORMATION),
                    String::from("John 3:16"),
                    Range::new(Position::new(0, 6), Position::new(0, 13))
                ),
                (
                    Some(DiagnosticSeverity::WARNING),
                    String::from("John 3:99 does not exist"),
                    Range::new(Position::new(1, 10), Position::new(1, 14))
                ),
            ]
        );
        // Codes tell them apart
        let codes: Vec<_> = server
            .diagnostics(&uri)
            .diagnostics
            .into_iter()
            .map(|d| d.code)
            .collect();
        assert_eq!(
            codes,
            [
                Some(lsp_types::NumberOrString::String("reference".into())),
                Some(lsp_types::NumberOrString::String("missing".into()))
            ]
        );

        server
            .configure(serde_json::json!({ "no-config": true, "reference-diagnostics": "hint" }))
            .unwrap();
        assert_eq!(summary(&server)[0].0, Some(DiagnosticSeverity::HINT));
        server
            .configure(serde_json::json!({ "no-config": true, "reference_diagnostics": "never" }))
            .unwrap();
        assert_eq!(summary(&server).len(), 1);
    }

    /// A workspace folder with notes on disk, plus one open document
    fn workspace(open: &str) -> (Server, Uri, PathBuf) {
        let dir = std::env::temp_dir().join(format!("topos-lsp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("My Notes")).unwrap();
        std::fs::write(
            dir.join("My Notes/a.md"),
            "Jn 3:16 again\nand John 3:14-18\n",
        )
        .unwrap();
        std::fs::write(dir.join("b.txt"), "John 3, John 2; 3:16, Rom 8:28").unwrap();
        std::fs::write(dir.join("image.png"), b"John 3:16\x00").unwrap();
        let uri = workspace::path_to_uri(&dir.join("open.md")).unwrap();
        let mut server = Server::new(BibleMatcher::default());
        server.set_roots(vec![dir.clone()]);
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri.clone(), "markdown".into(), 1, open.into()),
        });
        (server, uri, dir)
    }

    /// `file name: reference text` for each location
    fn found(dir: &Path, locations: &[Location]) -> Vec<String> {
        locations
            .iter()
            .map(|location| {
                let path = workspace::uri_to_path(&location.uri).unwrap();
                let name = path.strip_prefix(dir).unwrap().display().to_string();
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|_| String::from("See John 3:16 here"));
                let index = LineIndex::new(&text);
                let range = location.range;
                let start = index
                    .offset_of_utf16(range.start.line as usize, range.start.character as usize)
                    .unwrap();
                let end = index
                    .offset_of_utf16(range.end.line as usize, range.end.character as usize)
                    .unwrap();
                format!("{name}: {}", &text[start..end])
            })
            .collect()
    }

    #[test]
    fn searches_the_workspace() {
        let (server, uri, dir) = workspace("See John 3:16 here");
        // Go to references: exactly John 3:16, however it is written
        let references = server
            .references(ReferenceParams {
                text_document_position: at(&uri, 0, 6),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: lsp_types::ReferenceContext {
                    include_declaration: true,
                },
            })
            .unwrap();
        assert_eq!(
            found(&dir, &references),
            ["My Notes/a.md: Jn 3:16", "open.md: John 3:16"]
        );

        // One code action per search, for the reference under the cursor
        let actions = server
            .code_actions(CodeActionParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
                range: Range::new(Position::new(0, 8), Position::new(0, 8)),
                context: lsp_types::CodeActionContext::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .unwrap();
        let titles: Vec<_> = actions
            .iter()
            .map(|a| match a {
                CodeActionOrCommand::CodeAction(action) => action.title.clone(),
                CodeActionOrCommand::Command(command) => command.title.clone(),
            })
            .collect();
        assert_eq!(
            titles,
            [
                "Search \"John 3:16\" for explicit overlap",
                "Search \"John 3:16\" for any overlap",
                "Search \"John 3:16\" for exact overlap",
                "Search inside \"John 3:16\"",
            ]
        );

        // Running an action's command finds that search's references
        let run = |index: usize| {
            let CodeActionOrCommand::CodeAction(action) = &actions[index] else {
                panic!("not an action")
            };
            let command = action.command.clone().unwrap();
            let locations = server
                .execute_command(ExecuteCommandParams {
                    command: command.command,
                    arguments: command.arguments.unwrap(),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                })
                .unwrap();
            found(&dir, &locations)
        };
        assert_eq!(
            run(0),
            [
                "My Notes/a.md: Jn 3:16",
                "My Notes/a.md: John 3:14-18",
                "b.txt: John 2; 3:16",
                "open.md: John 3:16"
            ]
        );
        let any = run(1);
        assert!(any.contains(&"b.txt: John 3".to_string()), "{any:?}");
        assert_eq!(any.len(), 5);

        // The files' references are cached (the binary one too, as having none)
        assert_eq!(server.cache.len(), 3);
        let exact = |server: &Server| {
            let locations = server
                .search(SearchMode::ExactOverlap, "Romans 8:28")
                .unwrap();
            locations.len()
        };
        assert_eq!(exact(&server), 1);
        // Same size and time: the cache is used, so the edit isn't seen yet
        let b = dir.join("b.txt");
        let modified = std::fs::metadata(&b).unwrap().modified().unwrap();
        std::fs::write(&b, "John 3, John 2; 3:16, Rom 8:29").unwrap();
        let file = std::fs::File::options().write(true).open(&b).unwrap();
        file.set_modified(modified).unwrap();
        assert_eq!(exact(&server), 1);
        // A real change is read again
        std::fs::write(&b, "Nothing here now").unwrap();
        assert_eq!(exact(&server), 0);
        // Deleted files are forgotten
        std::fs::remove_file(&b).unwrap();
        exact(&server);
        assert_eq!(server.cache.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_format_completions_hints_and_symbols() {
        let (mut server, uri) = server("See jn 3:16, 17 and Jude 1:5");
        server
            .configure(serde_json::json!({
                "no-config": true,
                "topos": {
                    "no_config": true,
                    "psg-fmt": { "join_adjacent": true, "chapter_in_single_chapter_books": false },
                    "format": "abbreviation"
                }
            }))
            .unwrap();
        let hints = server
            .inlay_hints(InlayHintParams {
                work_done_progress_params: WorkDoneProgressParams::default(),
                text_document: TextDocumentIdentifier::new(uri.clone()),
                range: Range::new(Position::new(0, 0), Position::new(1, 0)),
            })
            .unwrap();
        let labels: Vec<_> = hints
            .iter()
            .map(|hint| match &hint.label {
                InlayHintLabel::String(label) => (hint.position, label.clone()),
                InlayHintLabel::LabelParts(_) => panic!("parts"),
            })
            .collect();
        assert_eq!(
            labels,
            [
                (Position::new(0, 15), String::from("Jn 3:16-17")),
                (Position::new(0, 28), String::from("Jude 5"))
            ]
        );
        // Unchanged references get no hint by default; "always" and "osis" show one
        server
            .configure(serde_json::json!({ "no-config": true, "inlay-hints": "osis" }))
            .unwrap();
        let hints = server
            .inlay_hints(InlayHintParams {
                work_done_progress_params: WorkDoneProgressParams::default(),
                text_document: TextDocumentIdentifier::new(uri.clone()),
                range: Range::new(Position::new(0, 0), Position::new(1, 0)),
            })
            .unwrap();
        assert_eq!(hints.len(), 2);

        // Bad settings are reported, and the previous settings stay
        let err = server
            .configure(serde_json::json!({ "no-config": true, "psg-fmt": { "joins": true } }))
            .unwrap_err();
        assert!(err.contains("joins"), "{err}");
    }

    #[test]
    fn reformats_and_explains_missing_verses() {
        let (server, uri) = server("See jn 3:16 and rom 8:28, John 1:1\nBad: John 3:99");
        let actions = |line: u32, character: u32| -> Vec<(String, Vec<TextEdit>)> {
            server
                .code_actions(CodeActionParams {
                    text_document: TextDocumentIdentifier::new(uri.clone()),
                    range: Range::new(
                        Position::new(line, character),
                        Position::new(line, character),
                    ),
                    context: lsp_types::CodeActionContext::default(),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                })
                .unwrap_or_default()
                .into_iter()
                .filter_map(|a| match a {
                    CodeActionOrCommand::CodeAction(action) => {
                        let edits = action
                            .edit
                            .and_then(|e| e.changes)
                            .map(|changes| changes.into_values().flatten().collect())?;
                        Some((action.title, edits))
                    }
                    CodeActionOrCommand::Command(_) => None,
                })
                .collect()
        };
        let on_jn = actions(0, 5);
        assert_eq!(on_jn.len(), 2);
        assert_eq!(on_jn[0].0, "Reformat as \"John 3:16\"");
        assert_eq!(
            on_jn[0].1,
            [TextEdit::new(
                Range::new(Position::new(0, 4), Position::new(0, 11)),
                "John 3:16".into()
            )]
        );
        // Every reference written differently (John 1:1 already is)
        assert_eq!(on_jn[1].0, "Reformat all 2 references in this file");
        assert_eq!(on_jn[1].1.len(), 2);
        // A reference already in the format gets no reformat for itself
        let on_john = actions(0, 28);
        assert_eq!(on_john.len(), 1);
        assert_eq!(on_john[0].0, "Reformat all 2 references in this file");

        // Hovering a verse that doesn't exist says what does
        let hover = server
            .hover(HoverParams {
                text_document_position_params: at(&uri, 1, 11),
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .unwrap();
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("no markup");
        };
        assert_eq!(
            markup.value,
            "**John 3:99 does not exist**\n\nJohn 3 has 36 verses"
        );
    }

    #[test]
    fn hovers_and_lists_symbols() {
        let (server, uri) = server("See Rom 8:28 and\nJude 5.");
        let hover = server
            .hover(HoverParams {
                text_document_position_params: at(&uri, 0, 6),
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .unwrap();
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("no markup");
        };
        assert_eq!(
            markup.value,
            "**Romans 8:28**\n\n\
             - **Abbreviation**: Rom 8:28\n\
             - **OSIS**: `Rom.8.28`\n\
             - **Book**: Romans, book 45 of 66, 16 chapters\n\
             - **Testament**: New Testament\n\
             - **Genres**: Pauline Epistles, Epistles\n\
             - **Verses**: 1 (8:28)\n\
             - **Location**: line 1, columns 5-12"
        );
        assert_eq!(
            hover.range,
            Some(Range::new(Position::new(0, 4), Position::new(0, 12)))
        );

        let Some(DocumentSymbolResponse::Nested(symbols)) =
            server.document_symbols(DocumentSymbolParams {
                text_document: TextDocumentIdentifier::new(uri),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
        else {
            panic!("no symbols");
        };
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Romans 8:28", "Jude 1:5"]);
    }

    /// The `hover` setting picks the lines and their order; a full name that is the title already
    /// is left out
    #[test]
    fn hover_lines_follow_the_setting() {
        let hover = |text: &str, settings: Value| {
            let (mut server, uri) = server(text);
            server.configure(settings).unwrap();
            let hover = server
                .hover(HoverParams {
                    text_document_position_params: at(&uri, 0, 1),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                })
                .unwrap();
            let HoverContents::Markup(markup) = hover.contents else {
                panic!("no markup");
            };
            markup.value
        };
        let settings = json!({ "no-config": true, "hover": "written,name,bcv,verses", "format": "abbreviation" });
        assert_eq!(
            hover("jn 3; 4:1-2", settings),
            "**Jn 3; 4:1-2**\n\n\
             - **Written**: `jn 3; 4:1-2`\n\
             - **Name**: John 3; 4:1-2\n\
             - **BCV**: `43003001-43003999`, `43004001-43004002`\n\
             - **Verses**: 38 (3:1-36; 4:1-2)"
        );
        let settings = json!({ "no-config": true, "hover": ["name", "testament"] });
        assert_eq!(
            hover("Gen 1:1", settings),
            "**Genesis 1:1**\n\n- **Testament**: Old Testament"
        );
        let settings = json!({ "no-config": true, "hover": [] });
        assert_eq!(hover("Gen 1:1", settings), "**Genesis 1:1**");
    }
}
