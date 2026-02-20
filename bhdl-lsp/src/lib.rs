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
mod definition;
mod references;
mod rename;
mod document_symbols;
mod semantic_tokens;
mod signature_help;
mod code_actions;
mod inlay_hints;
mod workspace_symbols;
mod folding_ranges;
mod call_hierarchy;
mod selection_range;
mod document_highlight;
mod code_lens;
mod document_link;
mod formatting;
mod on_type_formatting;
mod commands;

pub use document::DocumentStore;
pub use completion::provide_completions;
pub use diagnostics::convert_diagnostics;
pub use hover::provide_hover;
pub use definition::find_definition;
pub use references::find_references;
pub use rename::{prepare_rename, rename_symbol};
pub use document_symbols::provide_document_symbols;
pub use semantic_tokens::provide_semantic_tokens;
pub use signature_help::provide_signature_help;
pub use code_actions::provide_code_actions;
pub use inlay_hints::provide_inlay_hints;
pub use workspace_symbols::provide_workspace_symbols;
pub use folding_ranges::provide_folding_ranges;
pub use call_hierarchy::{prepare_call_hierarchy, incoming_calls, outgoing_calls};
pub use selection_range::provide_selection_range;
pub use document_highlight::provide_document_highlight;
pub use code_lens::provide_code_lens;
pub use document_link::provide_document_link;
pub use formatting::{format_document, format_range};
pub use on_type_formatting::on_type_formatting;

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
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![
                        "(".to_string(),
                        ",".to_string(),
                    ]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
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
                                    SemanticTokenType::KEYWORD,      // 0
                                    SemanticTokenType::TYPE,          // 1
                                    SemanticTokenType::VARIABLE,      // 2
                                    SemanticTokenType::PARAMETER,     // 3
                                    SemanticTokenType::FUNCTION,      // 4
                                    SemanticTokenType::COMMENT,       // 5
                                    SemanticTokenType::NUMBER,        // 6
                                    SemanticTokenType::STRING,        // 7
                                    SemanticTokenType::OPERATOR,      // 8
                                    SemanticTokenType::NAMESPACE,     // 9
                                ],
                                token_modifiers: vec![
                                    SemanticTokenModifier::DECLARATION,
                                    SemanticTokenModifier::DEFINITION,
                                    SemanticTokenModifier::READONLY,
                                ],
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".to_string(),
                    more_trigger_character: Some(vec!["}".to_string(), ";".to_string()]),
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        commands::BhdlCommand::ValidateDesign.as_str().to_string(),
                        commands::BhdlCommand::ShowComponentCount.as_str().to_string(),
                        commands::BhdlCommand::ShowPinCount.as_str().to_string(),
                        commands::BhdlCommand::AnalyzePowerDomains.as_str().to_string(),
                        commands::BhdlCommand::FormatAllDocuments.as_str().to_string(),
                    ],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
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

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut location) = find_definition(&document.text, position) {
                // Update the location URI to match the actual document
                location.uri = uri;
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
            }
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut locations) = find_references(&document.text, position, include_declaration) {
                // Update all location URIs to match the actual document
                for location in &mut locations {
                    location.uri = uri.clone();
                }
                return Ok(Some(locations));
            }
        }

        Ok(None)
    }

    async fn prepare_rename(&self, params: TextDocumentPositionParams) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(prepare_rename(&document.text, position));
        }

        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut workspace_edit) = rename_symbol(&document.text, position, &new_name) {
                // Update all URIs in the workspace edit to match the actual document
                if let Some(ref mut changes) = workspace_edit.changes {
                    // Remove the placeholder URI and replace with actual URI
                    if let Some(edits) = changes.remove(&Url::parse("file:///current_document").unwrap()) {
                        changes.insert(uri, edits);
                    }
                }
                return Ok(Some(workspace_edit));
            }
        }

        Ok(None)
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_document_symbols(&document.text));
        }

        Ok(None)
    }

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_semantic_tokens(&document.text));
        }

        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            let registry = Self::create_intent_registry();
            return Ok(provide_signature_help(&document.text, position, &registry));
        }

        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let diagnostics = params.context.diagnostics;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut actions) = provide_code_actions(&document.text, range, diagnostics) {
                // Update URIs in workspace edits to match the actual document
                for action_or_command in &mut actions {
                    if let CodeActionOrCommand::CodeAction(action) = action_or_command {
                        if let Some(ref mut edit) = action.edit {
                            if let Some(ref mut changes) = edit.changes {
                                // Replace placeholder URI with actual document URI
                                if let Some(edits) = changes.remove(&Url::parse("file:///current_document").unwrap()) {
                                    changes.insert(uri.clone(), edits);
                                }
                            }
                        }
                    }
                }
                return Ok(Some(actions));
            }
        }

        Ok(None)
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let range = params.range;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_inlay_hints(&document.text, range));
        }

        Ok(None)
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query;

        let documents = self.documents.read().await;
        let all_docs = documents.all_documents();
        drop(documents); // Release lock

        return Ok(provide_workspace_symbols(&query, &all_docs));
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_folding_ranges(&document.text));
        }

        Ok(None)
    }

    async fn prepare_call_hierarchy(&self, params: CallHierarchyPrepareParams) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut items) = prepare_call_hierarchy(&document.text, position) {
                // Update URIs to match actual document
                for item in &mut items {
                    item.uri = uri.clone();
                }
                return Ok(Some(items));
            }
        }

        Ok(None)
    }

    async fn incoming_calls(&self, params: CallHierarchyIncomingCallsParams) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let uri = params.item.uri.clone();

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut calls) = call_hierarchy::incoming_calls(&document.text, &params.item) {
                // Update URIs to match actual document
                for call in &mut calls {
                    call.from.uri = uri.clone();
                }
                return Ok(Some(calls));
            }
        }

        Ok(None)
    }

    async fn outgoing_calls(&self, params: CallHierarchyOutgoingCallsParams) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        let uri = params.item.uri.clone();

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            if let Some(mut calls) = call_hierarchy::outgoing_calls(&document.text, &params.item) {
                // Update URIs to match actual document
                for call in &mut calls {
                    call.to.uri = uri.clone();
                }
                return Ok(Some(calls));
            }
        }

        Ok(None)
    }

    async fn selection_range(&self, params: SelectionRangeParams) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions = params.positions;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_selection_range(&document.text, positions));
        }

        Ok(None)
    }

    async fn document_highlight(&self, params: DocumentHighlightParams) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_document_highlight(&document.text, position));
        }

        Ok(None)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_code_lens(&document.text));
        }

        Ok(None)
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(provide_document_link(&document.text, &uri));
        }

        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let options = params.options;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(format_document(&document.text, Some(options)));
        }

        Ok(None)
    }

    async fn range_formatting(&self, params: DocumentRangeFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let options = params.options;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(format_range(&document.text, range, Some(options)));
        }

        Ok(None)
    }

    async fn on_type_formatting(&self, params: DocumentOnTypeFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let ch = params.ch;
        let options = params.options;

        let documents = self.documents.read().await;
        if let Some(document) = documents.get(&uri) {
            return Ok(on_type_formatting(&document.text, position, &ch, options));
        }

        Ok(None)
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        let command = params.command;
        let arguments = params.arguments;

        // For most commands, we'll need the currently active document
        // Since LSP doesn't have a concept of "active document", we'll use the most recently opened/changed
        // For now, commands will work on the first document in the store if available
        let documents = self.documents.read().await;
        let all_docs = documents.all_documents();
        let text_owned = all_docs.get(0).map(|(_, doc)| doc.clone());
        drop(documents);

        let result = commands::execute_command(&self.client, &command, arguments, text_owned.as_deref()).await;
        Ok(result)
    }
}
