//! BHDL Language Server Protocol Implementation
//!
//! Provides IDE integration for BHDL with:
//! - Real-time diagnostics
//! - Intent function autocomplete
//! - Hover documentation
//! - Go to definition
//! - Syntax highlighting

use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_common::IntentRegistry;
use bhdl_stdlib::intents::register_stdlib_intents;

mod document;
mod completion;
mod diagnostics;
mod hover;

pub use document::DocumentStore;
pub use completion::provide_completions;
pub use diagnostics::convert_diagnostics;
pub use hover::provide_hover;

/// BHDL Language Server Backend
pub struct BhdlLanguageServer {
    client: Client,
    documents: Arc<RwLock<DocumentStore>>,
}

impl BhdlLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(DocumentStore::new())),
        }
    }

    /// Create a fresh intent registry (fast operation)
    fn create_intent_registry() -> IntentRegistry {
        let mut registry = IntentRegistry::new();
        register_stdlib_intents(&mut registry);
        registry
    }

    /// Analyze a document and publish diagnostics
    async fn analyze_document(&self, uri: Url, text: &str) {
        // Parse the document
        let parse_result = parse(text);

        // Collect parse errors
        let mut lsp_diagnostics = Vec::new();
        for error in parse_result.errors() {
            lsp_diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 0 },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                source: Some("bhdl-parser".to_string()),
                message: error.message.clone(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            });
        }

        // If parsing succeeded, run semantic analysis
        if lsp_diagnostics.is_empty() {
            if let Some(source_file) = SourceFile::cast(parse_result.syntax()) {
                let analysis_result = analyze(&source_file);

                // Convert analyzer diagnostics to LSP format
                lsp_diagnostics.extend(convert_diagnostics(&analysis_result.diagnostics));

                // Note: We don't store the full analysis result since it's not Send+Sync
                // For advanced features (go-to-definition, etc.), we would re-analyze on demand
            }
        }

        // Publish diagnostics to client
        self.client.publish_diagnostics(uri, lsp_diagnostics, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for BhdlLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "(".to_string(),
                        ",".to_string(),
                        " ".to_string(),
                        "f".to_string(), // for "for" keyword
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("bhdl".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::KEYWORD,
                                    SemanticTokenType::TYPE,
                                    SemanticTokenType::VARIABLE,
                                    SemanticTokenType::PARAMETER,
                                    SemanticTokenType::FUNCTION,
                                    SemanticTokenType::COMMENT,
                                    SemanticTokenType::NUMBER,
                                    SemanticTokenType::STRING,
                                ],
                                token_modifiers: vec![],
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "BHDL Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "BHDL Language Server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        // Store document
        let mut documents = self.documents.write().await;
        documents.open(uri.clone(), text.clone());
        drop(documents); // Release lock before analysis

        // Analyze document
        self.analyze_document(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.content_changes[0].text.clone();

        // Update document
        let mut documents = self.documents.write().await;
        documents.update(uri.clone(), text.clone());
        drop(documents); // Release lock before analysis

        // Analyze document
        self.analyze_document(uri, &text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        // Remove document
        let mut documents = self.documents.write().await;
        documents.close(uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            let registry = Self::create_intent_registry();
            let completions = provide_completions(
                &document.text,
                position,
                &registry,
            );
            return Ok(Some(CompletionResponse::Array(completions)));
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            let registry = Self::create_intent_registry();
            return Ok(provide_hover(&document.text, position, &registry));
        }

        Ok(None)
    }
}
