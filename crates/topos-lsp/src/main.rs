//! `topos-lsp` speaks the Language Server Protocol over stdin and stdout.

use std::error::Error;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Uri,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
        Notification as LspNotification, PublishDiagnostics,
    },
    request::{Completion, DocumentSymbolRequest, HoverRequest, Request as LspRequest},
};
use topos_bible::matcher::BibleMatcher;

use crate::server::Server;

mod server;

type AnyError = Box<dyn Error + Send + Sync>;

fn main() -> Result<(), AnyError> {
    let (connection, io_threads) = Connection::stdio();
    run(&connection)?;
    io_threads.join()?;
    Ok(())
}

fn run(connection: &Connection) -> Result<(), AnyError> {
    connection.initialize(serde_json::to_value(Server::capabilities())?)?;
    let mut server = Server::new(BibleMatcher::default());
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
            Message::Notification(notification) => {
                if let Some(uri) = notify(&mut server, notification) {
                    let diagnostics = server.diagnostics(&uri);
                    let notification =
                        Notification::new(PublishDiagnostics::METHOD.into(), diagnostics);
                    connection
                        .sender
                        .send(Message::Notification(notification))?;
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
}
