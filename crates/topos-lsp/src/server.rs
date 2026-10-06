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
    filter::bible_filter::BibleFilter,
    matcher::{BibleMatch, BibleMatcher, LineIndex},
    segments::{
        Passage,
        autocomplete::{CompleteOptions, CompletionKind},
        formatter::{BookStyle, FormatOptions},
    },
};

use crate::{
    settings::{self, Configured, InlayHints},
    workspace,
};

/// The command code actions run; its result is the matching locations
pub const SEARCH_COMMAND: &str = "topos.search";

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
    /// Extensions searched in the workspace (empty for every text file)
    extensions: Vec<String>,
    documents: HashMap<Uri, String>,
    /// The workspace folders, for workspace-wide searches
    roots: Vec<PathBuf>,
    /// The editor's settings (initialization options, then the latest configuration)
    editor: Map<String, Value>,
}

impl Server {
    pub fn new(matcher: BibleMatcher) -> Self {
        Self {
            matcher,
            format: FormatOptions::default(),
            inlay_hints: InlayHints::default(),
            extensions: vec![],
            documents: HashMap::new(),
            roots: vec![],
            editor: Map::new(),
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
            extensions,
        } = settings::settings(config, &editor)?.configure()?;
        self.matcher = matcher;
        self.format = format;
        self.inlay_hints = inlay_hints;
        self.extensions = extensions;
        self.editor = editor;
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
                    label: completion.label,
                    kind: Some(match completion.kind {
                        CompletionKind::Book => CompletionItemKind::MODULE,
                        CompletionKind::Chapter => CompletionItemKind::FOLDER,
                        CompletionKind::Verse => CompletionItemKind::REFERENCE,
                    }),
                    // Filter by what was typed, so `jn 3:` still shows `John 3:16`
                    filter_text: Some(text[range.clone()].to_string()),
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
        Some(CompletionResponse::Array(items))
    }

    /// Warnings for references that do not exist, like `John 3:99`
    pub fn diagnostics(&self, uri: &Uri) -> PublishDiagnosticsParams {
        let diagnostics = self.documents.get(uri).map_or_else(Vec::new, |text| {
            let index = LineIndex::new(text);
            self.matcher
                .problems(text)
                .into_iter()
                .map(|problem| Diagnostic {
                    range: lsp_range(&index, problem.bytes.start, problem.bytes.end),
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some(String::from("topos")),
                    message: problem.message,
                    ..Diagnostic::default()
                })
                .collect()
        });
        PublishDiagnosticsParams::new(uri.clone(), diagnostics, None)
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

    /// The reference under the cursor, normalized, with its OSIS id
    pub fn hover(&self, params: HoverParams) -> Option<Hover> {
        let position = params.text_document_position_params;
        let (found, index) = self.reference_at(&position.text_document.uri, position.position)?;
        let bytes = found.location.bytes;
        let osis = found
            .psg
            .to_osis(self.matcher.data().books())
            .unwrap_or_default();
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}**\n\nOSIS: `{osis}`", self.reference(&found)),
            }),
            range: Some(lsp_range(&index, bytes.start, bytes.end)),
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

    /// For the reference under the cursor, one search of the workspace per [`SearchMode`]
    pub fn code_actions(&self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let (found, _) = self.reference_at(&params.text_document.uri, params.range.start)?;
        let reference = self.reference(&found);
        let passage = self.parseable(&found.psg);
        let actions = SearchMode::ALL
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
        Some(actions)
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
        let mut texts = workspace::texts(&self.roots, &self.documents, &self.extensions);
        texts.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        let mut locations = vec![];
        for (uri, text) in texts {
            let index = LineIndex::new(&text);
            for m in self.matcher.search(&text) {
                if query.keeps(&m.psg) {
                    let bytes = m.location.bytes;
                    locations.push(Location::new(
                        uri.clone(),
                        lsp_range(&index, bytes.start, bytes.end),
                    ));
                }
            }
        }
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

fn lsp_range(index: &LineIndex, start: usize, end: usize) -> Range {
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

    #[test]
    fn completes_with_utf16_ranges() {
        // `𝄞` is 2 UTF-16 units, so `jn` starts at character 3
        let (server, uri) = server("intro\n𝄞 jn 3:1");
        let Some(CompletionResponse::Array(items)) = server.completion(CompletionParams {
            text_document_position: at(&uri, 1, 9),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        }) else {
            panic!("no completions");
        };
        let first = &items[0];
        assert_eq!(first.label, "John 3:1");
        assert_eq!(first.filter_text.as_deref(), Some("jn 3:1"));
        let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &first.text_edit else {
            panic!("no edit");
        };
        assert_eq!(
            edit.range,
            Range::new(Position::new(1, 3), Position::new(1, 9))
        );
    }

    #[test]
    fn warns_about_references_that_do_not_exist() {
        let (server, uri) = server("Fine: John 3:16\nBad: John 3:99");
        let params = server.diagnostics(&uri);
        assert_eq!(params.diagnostics.len(), 1);
        let diagnostic = &params.diagnostics[0];
        assert_eq!(diagnostic.message, "John 3:99 does not exist");
        assert_eq!(
            diagnostic.range,
            Range::new(Position::new(1, 10), Position::new(1, 14))
        );
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
        assert_eq!(markup.value, "**Romans 8:28**\n\nOSIS: `Rom.8.28`");
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
}
