//! `topos-lsp` speaks the Language Server Protocol over stdin and stdout.

use std::error::Error;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, MessageType, ShowMessageParams, Uri,
    notification::{
        DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
        Notification as LspNotification, PublishDiagnostics, ShowMessage,
    },
    request::{
        CodeActionRequest, Completion, DocumentDiagnosticRequest, DocumentSymbolRequest,
        ExecuteCommand, HoverRequest, InlayHintRequest, References, Request as LspRequest,
        WorkspaceDiagnosticRefresh,
    },
};
use topos_bible::matcher::BibleMatcher;

use crate::server::Server;

mod server;
mod settings;
mod workspace;

type AnyError = Box<dyn Error + Send + Sync>;

fn main() -> Result<(), AnyError> {
    let (connection, io_threads) = Connection::stdio();
    run(&connection)?;
    io_threads.join()?;
    Ok(())
}

/// Tells the editor something went wrong (like bad settings), without stopping the server
fn warn(connection: &Connection, message: String) -> Result<(), AnyError> {
    let params = ShowMessageParams {
        typ: MessageType::WARNING,
        message,
    };
    let notification = Notification::new(ShowMessage::METHOD.into(), params);
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    server: &Server,
    uri: &Uri,
) -> Result<(), AnyError> {
    let notification =
        Notification::new(PublishDiagnostics::METHOD.into(), server.diagnostics(uri));
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}

fn run(connection: &Connection) -> Result<(), AnyError> {
    let (id, init) = connection.initialize_start()?;
    let init: InitializeParams = serde_json::from_value(init).unwrap_or_default();
    let text_document = init.capabilities.text_document.as_ref();
    // Editors that ask for diagnostics (pull) get them for any buffer, even an unnamed one whose
    // `file://` URI a pushed notification couldn't be matched back to
    let pull = text_document.and_then(|t| t.diagnostic.as_ref()).is_some();
    let refresh = init
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.diagnostic.as_ref())
        .and_then(|d| d.refresh_support)
        .unwrap_or(false);
    let mut capabilities = Server::capabilities();
    if pull {
        capabilities.diagnostic_provider = Some(lsp_types::DiagnosticServerCapabilities::Options(
            lsp_types::DiagnosticOptions {
                identifier: Some(String::from("topos")),
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..lsp_types::DiagnosticOptions::default()
            },
        ));
    }
    connection.initialize_finish(
        id,
        serde_json::json!({
            "capabilities": capabilities,
            "serverInfo": { "name": "topos-lsp", "version": env!("CARGO_PKG_VERSION") },
        }),
    )?;
    let mut server = Server::new(BibleMatcher::default());
    let mut refreshes = 0;
    // The workspace folders, for go-to-references and searches
    #[allow(deprecated)]
    let roots = match &init.workspace_folders {
        Some(folders) => folders.iter().map(|f| f.uri.clone()).collect(),
        None => init.root_uri.iter().cloned().collect::<Vec<_>>(),
    };
    server.set_roots(roots.iter().filter_map(workspace::uri_to_path).collect());
    let options = init.initialization_options.unwrap_or_default();
    if let Err(err) = server.configure(options) {
        warn(connection, err)?;
    }
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                connection
                    .sender
                    .send(Message::Response(respond(&server, request)))?;
            }
            Message::Notification(notification)
                if notification.method == DidChangeConfiguration::METHOD =>
            {
                let Ok(params) =
                    serde_json::from_value::<DidChangeConfigurationParams>(notification.params)
                else {
                    continue;
                };
                match server.configure(params.settings) {
                    // New data can change which references exist
                    Ok(()) if pull && refresh => {
                        refreshes += 1;
                        let request = Request::new(
                            lsp_server::RequestId::from(format!("refresh-{refreshes}")),
                            WorkspaceDiagnosticRefresh::METHOD.into(),
                            serde_json::Value::Null,
                        );
                        connection.sender.send(Message::Request(request))?;
                    }
                    Ok(()) if pull => {}
                    Ok(()) => {
                        for uri in server.documents() {
                            publish_diagnostics(connection, &server, &uri)?;
                        }
                    }
                    Err(err) => warn(connection, err)?,
                }
            }
            Message::Notification(notification) => {
                if let Some(uri) = notify(&mut server, notification)
                    && !pull
                {
                    publish_diagnostics(connection, &server, &uri)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn respond(server: &Server, request: Request) -> Response {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        Completion::METHOD => params(request).map(|p| json(server.completion(p))),
        HoverRequest::METHOD => params(request).map(|p| json(server.hover(p))),
        DocumentSymbolRequest::METHOD => params(request).map(|p| json(server.document_symbols(p))),
        InlayHintRequest::METHOD => params(request).map(|p| json(server.inlay_hints(p))),
        CodeActionRequest::METHOD => params(request).map(|p| json(server.code_actions(p))),
        References::METHOD => params(request).map(|p| json(server.references(p))),
        ExecuteCommand::METHOD => params(request).and_then(|p| server.execute_command(p).map(json)),
        DocumentDiagnosticRequest::METHOD => {
            params(request).map(|p| json(server.pull_diagnostics(p)))
        }
        method => {
            let code = lsp_server::ErrorCode::MethodNotFound as i32;
            return Response::new_err(id, code, format!("unsupported request {method}"));
        }
    };
    match result {
        Ok(value) => Response::new_ok(id, value),
        Err(err) => Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, err),
    }
}

/// Applies a document notification, returning the document whose diagnostics changed
fn notify(server: &mut Server, notification: Notification) -> Option<Uri> {
    let params = notification.params;
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(params).ok()?;
            let uri = p.text_document.uri.clone();
            server.did_open(p);
            Some(uri)
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(params).ok()?;
            let uri = p.text_document.uri.clone();
            server.did_change(p);
            Some(uri)
        }
        DidCloseTextDocument::METHOD => {
            let p: DidCloseTextDocumentParams = serde_json::from_value(params).ok()?;
            let uri = p.text_document.uri.clone();
            // Clears the document's diagnostics, since it is no longer open
            server.did_close(p);
            Some(uri)
        }
        _ => None,
    }
}

fn params<P: serde::de::DeserializeOwned>(request: Request) -> Result<P, String> {
    serde_json::from_value(request.params).map_err(|e| e.to_string())
}

fn json(value: impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use lsp_server::{Connection, Message, Notification, Request, RequestId};
    use serde_json::json;

    /// A whole session over an in-memory connection
    #[test]
    fn session() {
        let (client, server) = Connection::memory();
        let thread = std::thread::spawn(move || super::run(&server).unwrap());
        let send = |message: Message| client.sender.send(message).unwrap();
        let request = |id: i32, method: &str, params| {
            Message::Request(Request::new(RequestId::from(id), method.into(), params))
        };

        send(request(1, "initialize", json!({ "capabilities": {} })));
        let Message::Response(init) = client.receiver.recv().unwrap() else {
            panic!("expected the initialize response")
        };
        assert!(init.response_result.unwrap()["capabilities"]["completionProvider"].is_object());
        send(Message::Notification(Notification::new(
            "initialized".into(),
            json!({}),
        )));

        let uri = "file:///a.md";
        send(Message::Notification(Notification::new(
            "textDocument/didOpen".into(),
            json!({ "textDocument": { "uri": uri, "languageId": "markdown", "version": 1, "text": "Read Phil 4:1" } }),
        )));
        // Opening a document publishes its diagnostics
        let Message::Notification(diagnostics) = client.receiver.recv().unwrap() else {
            panic!("expected diagnostics")
        };
        assert_eq!(diagnostics.method, "textDocument/publishDiagnostics");
        send(request(
            2,
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 6 } }),
        ));
        let Message::Response(hover) = client.receiver.recv().unwrap() else {
            panic!("expected the hover response")
        };
        let value = hover.response_result.unwrap()["contents"]["value"].clone();
        assert_eq!(value, "**Philippians 4:1**\n\nOSIS: `Phil.4.1`");

        send(request(3, "shutdown", json!(null)));
        client.receiver.recv().unwrap();
        send(Message::Notification(Notification::new(
            "exit".into(),
            json!(null),
        )));
        thread.join().unwrap();
    }

    /// Editors that support pull diagnostics ask for them, and get none pushed
    #[test]
    fn pull_diagnostics() {
        let (client, server) = Connection::memory();
        let thread = std::thread::spawn(move || super::run(&server).unwrap());
        let send = |message: Message| client.sender.send(message).unwrap();
        let request = |id: i32, method: &str, params| {
            Message::Request(Request::new(RequestId::from(id), method.into(), params))
        };
        send(request(
            1,
            "initialize",
            json!({ "capabilities": { "textDocument": { "diagnostic": {} } } }),
        ));
        let Message::Response(init) = client.receiver.recv().unwrap() else {
            panic!("expected the initialize response")
        };
        let result = init.response_result.unwrap();
        assert_eq!(
            result["capabilities"]["diagnosticProvider"]["identifier"],
            "topos"
        );
        send(Message::Notification(Notification::new(
            "initialized".into(),
            json!({}),
        )));
        send(Message::Notification(Notification::new(
            "textDocument/didOpen".into(),
            json!({ "textDocument": { "uri": "file://", "languageId": "", "version": 1, "text": "Read jn 3:16" } }),
        )));
        // No diagnostics are pushed: the next message is the answer to this request
        send(request(
            2,
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": "file://" } }),
        ));
        let Message::Response(report) = client.receiver.recv().unwrap() else {
            panic!("expected the diagnostic report, not a pushed notification")
        };
        let report = report.response_result.unwrap();
        assert_eq!(report["kind"], "full");
        assert_eq!(report["items"][0]["message"], "John 3:16");

        send(request(3, "shutdown", json!(null)));
        client.receiver.recv().unwrap();
        send(Message::Notification(Notification::new(
            "exit".into(),
            json!(null),
        )));
        thread.join().unwrap();
    }
}
