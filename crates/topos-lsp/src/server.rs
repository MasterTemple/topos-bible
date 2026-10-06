use std::collections::HashMap;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Hover, HoverContents,
    HoverParams, HoverProviderCapability, MarkupContent, MarkupKind, OneOf, Position, Range,
    ServerCapabilities, SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Uri,
};
use topos_lib::{
    matcher::{BibleMatch, BibleMatcher, LineIndex},
    segments::autocomplete::{CompleteOptions, CompletionKind},
};

/// Open documents and the matcher, with one method per LSP request or notification
pub struct Server {
    matcher: BibleMatcher,
    documents: HashMap<Uri, String>,
}

impl Server {
    pub fn new(matcher: BibleMatcher) -> Self {
        Self {
            matcher,
            documents: HashMap::new(),
        }
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
            limit: Some(100),
            ..CompleteOptions::default()
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

    /// The reference under the cursor, normalized, with its OSIS id
    pub fn hover(&self, params: HoverParams) -> Option<Hover> {
        let position = params.text_document_position_params;
        let text = self.documents.get(&position.text_document.uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(
            position.position.line as usize,
            position.position.character as usize,
        )?;
        let found = self.matcher.search(text).into_iter().find(|m| {
            let bytes = m.location.bytes;
            bytes.start <= cursor && cursor <= bytes.end
        })?;
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

    fn reference(&self, found: &BibleMatch) -> String {
        let data = self.matcher.data();
        let book = data
            .books()
            .get_name(found.psg.book)
            .cloned()
            .unwrap_or_default();
        format!("{book} {}", found.psg.segments)
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
