//! Execute Command support - custom BHDL-specific commands

use tower_lsp::lsp_types::*;
use tower_lsp::Client;
use serde_json::Value;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

/// Available BHDL commands
pub enum BhdlCommand {
    ValidateDesign,
    ShowComponentCount,
    ShowPinCount,
    AnalyzePowerDomains,
    FormatAllDocuments,
    GenerateSchematic,
}

impl BhdlCommand {
    pub fn as_str(&self) -> &'static str {
        match self {
            BhdlCommand::ValidateDesign => "bhdl.validateDesign",
            BhdlCommand::ShowComponentCount => "bhdl.showComponentCount",
            BhdlCommand::ShowPinCount => "bhdl.showPinCount",
            BhdlCommand::AnalyzePowerDomains => "bhdl.analyzePowerDomains",
            BhdlCommand::FormatAllDocuments => "bhdl.formatAllDocuments",
            BhdlCommand::GenerateSchematic => "bhdl.generateSchematicJson",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            BhdlCommand::ValidateDesign => "Validate BHDL Design",
            BhdlCommand::ShowComponentCount => "Show Component Count",
            BhdlCommand::ShowPinCount => "Show Pin Count",
            BhdlCommand::AnalyzePowerDomains => "Analyze Power Domains",
            BhdlCommand::FormatAllDocuments => "Format All BHDL Documents",
            BhdlCommand::GenerateSchematic => "Generate Schematic JSON",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "bhdl.validateDesign" => Some(BhdlCommand::ValidateDesign),
            "bhdl.showComponentCount" => Some(BhdlCommand::ShowComponentCount),
            "bhdl.showPinCount" => Some(BhdlCommand::ShowPinCount),
            "bhdl.analyzePowerDomains" => Some(BhdlCommand::AnalyzePowerDomains),
            "bhdl.formatAllDocuments" => Some(BhdlCommand::FormatAllDocuments),
            "bhdl.generateSchematicJson" => Some(BhdlCommand::GenerateSchematic),
            _ => None,
        }
    }
}

/// Execute a BHDL command
pub async fn execute_command(
    client: &Client,
    command: &str,
    arguments: Vec<Value>,
    text: Option<&str>,
) -> Option<Value> {
    let bhdl_command = BhdlCommand::from_str(command)?;

    match bhdl_command {
        BhdlCommand::ValidateDesign => {
            execute_validate_design(client, text).await
        }
        BhdlCommand::ShowComponentCount => {
            execute_show_component_count(client, text).await
        }
        BhdlCommand::ShowPinCount => {
            execute_show_pin_count(client, text).await
        }
        BhdlCommand::AnalyzePowerDomains => {
            execute_analyze_power_domains(client, text).await
        }
        BhdlCommand::FormatAllDocuments => {
            execute_format_all(client, arguments).await
        }
        BhdlCommand::GenerateSchematic => {
            execute_generate_schematic(client, text).await
        }
    }
}

/// Validate the current design
async fn execute_validate_design(
    client: &Client,
    text: Option<&str>,
) -> Option<Value> {
    let text = text?;

    let parse_result = parse(text);
    let parse_error_count = parse_result.errors().len();

    if parse_error_count > 0 {
        client.show_message(
            MessageType::ERROR,
            format!("Parse errors found: {} errors", parse_error_count),
        ).await;
        return Some(serde_json::json!({
            "success": false,
            "parse_errors": parse_error_count,
            "semantic_errors": 0,
        }));
    }

    // Extract diagnostic count in a scope to ensure non-Send types are dropped
    let diagnostic_count = {
        let source_file = SourceFile::cast(parse_result.syntax())?;
        let analysis_result = analyze(&source_file);
        analysis_result.diagnostics.len()
    }; // source_file and analysis_result dropped here

    if diagnostic_count > 0 {
        client.show_message(
            MessageType::WARNING,
            format!("Validation found {} diagnostic{}",
                diagnostic_count,
                if diagnostic_count == 1 { "" } else { "s" }
            ),
        ).await;
    } else {
        client.show_message(
            MessageType::INFO,
            "Validation passed: No errors or warnings found!",
        ).await;
    }

    Some(serde_json::json!({
        "success": diagnostic_count == 0,
        "parse_errors": 0,
        "semantic_errors": diagnostic_count,
    }))
}

/// Show component count
async fn execute_show_component_count(
    client: &Client,
    text: Option<&str>,
) -> Option<Value> {
    let text = text?;

    // Extract counts in a scope to ensure non-Send types are dropped
    let (board_count, entity_count, total_instances) = {
        let parse_result = parse(text);
        let source_file = SourceFile::cast(parse_result.syntax())?;
        let analysis_result = analyze(&source_file);

        let mut total_instances = 0;
        let mut board_count = 0;
        let mut entity_count = 0;

        // Count symbols
        for symbol in analysis_result.global_scope.iter() {
            match symbol.kind {
                bhdl_analyzer::symbol_table::SymbolKind::Board => board_count += 1,
                bhdl_analyzer::symbol_table::SymbolKind::Entity => entity_count += 1,
                bhdl_analyzer::symbol_table::SymbolKind::Instance => total_instances += 1,
                _ => {}
            }
        }

        // Count instances in definition scopes
        for (_ptr, scope) in &analysis_result.definition_scopes {
            for symbol in scope.iter() {
                if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Instance {
                    total_instances += 1;
                }
            }
        }

        (board_count, entity_count, total_instances)
    }; // All non-Send types dropped here

    client.show_message(
        MessageType::INFO,
        format!(
            "Design summary: {} boards, {} entities, {} component instances",
            board_count, entity_count, total_instances
        ),
    ).await;

    Some(serde_json::json!({
        "boards": board_count,
        "entities": entity_count,
        "instances": total_instances,
    }))
}

/// Show pin count
async fn execute_show_pin_count(
    client: &Client,
    text: Option<&str>,
) -> Option<Value> {
    let text = text?;

    // Extract pin counts in a scope to ensure non-Send types are dropped
    let (total_pins, virtual_pins) = {
        let parse_result = parse(text);
        let source_file = SourceFile::cast(parse_result.syntax())?;
        let analysis_result = analyze(&source_file);

        let mut total_pins = 0;
        let mut virtual_pins = 0;

        // Count pins in all scopes
        for symbol in analysis_result.global_scope.iter() {
            match symbol.kind {
                bhdl_analyzer::symbol_table::SymbolKind::Pin => total_pins += 1,
                bhdl_analyzer::symbol_table::SymbolKind::VirtualPin => {
                    total_pins += 1;
                    virtual_pins += 1;
                }
                _ => {}
            }
        }

        for (_ptr, scope) in &analysis_result.definition_scopes {
            for symbol in scope.iter() {
                match symbol.kind {
                    bhdl_analyzer::symbol_table::SymbolKind::Pin => total_pins += 1,
                    bhdl_analyzer::symbol_table::SymbolKind::VirtualPin => {
                        total_pins += 1;
                        virtual_pins += 1;
                    }
                    _ => {}
                }
            }
        }

        (total_pins, virtual_pins)
    }; // All non-Send types dropped here

    client.show_message(
        MessageType::INFO,
        format!(
            "Pin summary: {} total pins ({} physical, {} virtual)",
            total_pins,
            total_pins - virtual_pins,
            virtual_pins
        ),
    ).await;

    Some(serde_json::json!({
        "total": total_pins,
        "physical": total_pins - virtual_pins,
        "virtual": virtual_pins,
    }))
}

/// Analyze power domains
async fn execute_analyze_power_domains(
    client: &Client,
    text: Option<&str>,
) -> Option<Value> {
    let text = text?;

    // Extract power/ground domains in a scope to ensure non-Send types are dropped
    let (power_domains, ground_domains) = {
        let parse_result = parse(text);
        let source_file = SourceFile::cast(parse_result.syntax())?;
        let analysis_result = analyze(&source_file);

        let mut power_domains = Vec::new();
        let mut ground_domains = Vec::new();

        // Find power and ground declarations
        // Power/ground domains are stored as Net symbols with NetAttribute metadata
        for symbol in analysis_result.global_scope.iter() {
            if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Net {
                if let Some(ref attrs) = symbol.net_attributes {
                    match attrs {
                        bhdl_analyzer::net_attributes::NetAttribute::PowerDomain { .. } => {
                            power_domains.push(symbol.name.clone());
                        }
                        bhdl_analyzer::net_attributes::NetAttribute::GroundDomain => {
                            ground_domains.push(symbol.name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        for (_ptr, scope) in &analysis_result.definition_scopes {
            for symbol in scope.iter() {
                if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::Net {
                    if let Some(ref attrs) = symbol.net_attributes {
                        let scoped_name = format!("{}.{}", scope.scope_name.as_ref().unwrap_or(&"?".to_string()), symbol.name);
                        match attrs {
                            bhdl_analyzer::net_attributes::NetAttribute::PowerDomain { .. } => {
                                power_domains.push(scoped_name);
                            }
                            bhdl_analyzer::net_attributes::NetAttribute::GroundDomain => {
                                ground_domains.push(scoped_name);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        (power_domains, ground_domains)
    }; // All non-Send types dropped here

    let message = if power_domains.is_empty() && ground_domains.is_empty() {
        "No power or ground domains found".to_string()
    } else {
        format!(
            "Power domains: {}\nGround domains: {}",
            if power_domains.is_empty() { "None".to_string() } else { power_domains.join(", ") },
            if ground_domains.is_empty() { "None".to_string() } else { ground_domains.join(", ") }
        )
    };

    client.show_message(MessageType::INFO, message).await;

    Some(serde_json::json!({
        "power_domains": power_domains,
        "ground_domains": ground_domains,
    }))
}

/// Format all documents (placeholder - would need workspace access)
async fn execute_format_all(
    client: &Client,
    _arguments: Vec<Value>,
) -> Option<Value> {
    client.show_message(
        MessageType::INFO,
        "Format all documents: Use editor's format command on each file",
    ).await;

    Some(Value::Bool(true))
}

/// Generate SchematicData JSON for the current document.
///
/// Requires netlist synthesis which is async and uses non-Send types (rowan AST).
/// Currently returns the schematic data by running parse → analyze → synthesize → extract
/// synchronously in a blocking closure to avoid Send issues with tower-lsp.
async fn execute_generate_schematic(
    client: &Client,
    text: Option<&str>,
) -> Option<Value> {
    let text = text?;
    let text_owned = text.to_string();

    // Run synthesis in a blocking task to avoid Send issues with rowan AST nodes.
    // The synthesizer uses async internally for database operations, so we create
    // a temporary runtime inside the blocking task.
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let parse_result = parse(&text_owned);
        if !parse_result.errors().is_empty() {
            return Err(format!("{} parse errors", parse_result.errors().len()));
        }

        let source_file = SourceFile::cast(parse_result.syntax())
            .ok_or_else(|| "Failed to cast SourceFile".to_string())?;
        let analysis = analyze(&source_file);

        // Create a temporary runtime for the async synthesizer
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("runtime_error: {}", e))?;

        let mut generator = bhdl_synthesizer::NetlistGenerator::new();
        let netlist = rt.block_on(generator.generate_from_ast_and_analysis(&source_file, &analysis))
            .map_err(|e| format!("synthesis_failed: {}", e))?;

        let data = bhdl_schematic::extract_schematic_data(&netlist, Some(&analysis), None)
            .map_err(|e| format!("extraction_failed: {}", e))?;

        serde_json::to_value(&data)
            .map_err(|e| format!("serialization_failed: {}", e))
    }).await;

    match result {
        Ok(Ok(json)) => Some(json),
        Ok(Err(e)) => {
            client.show_message(MessageType::ERROR, format!("Schematic generation failed: {}", e)).await;
            Some(serde_json::json!({ "error": e }))
        }
        Err(e) => {
            client.show_message(MessageType::ERROR, format!("Task failed: {}", e)).await;
            Some(serde_json::json!({ "error": format!("task_failed: {}", e) }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_as_str() {
        assert_eq!(BhdlCommand::ValidateDesign.as_str(), "bhdl.validateDesign");
        assert_eq!(BhdlCommand::ShowComponentCount.as_str(), "bhdl.showComponentCount");
        assert_eq!(BhdlCommand::GenerateSchematic.as_str(), "bhdl.generateSchematicJson");
    }

    #[test]
    fn test_command_from_str() {
        assert!(matches!(
            BhdlCommand::from_str("bhdl.validateDesign"),
            Some(BhdlCommand::ValidateDesign)
        ));
        assert!(matches!(
            BhdlCommand::from_str("bhdl.showComponentCount"),
            Some(BhdlCommand::ShowComponentCount)
        ));
        assert!(matches!(
            BhdlCommand::from_str("bhdl.generateSchematicJson"),
            Some(BhdlCommand::GenerateSchematic)
        ));
        assert!(BhdlCommand::from_str("invalid.command").is_none());
    }

    #[test]
    fn test_command_title() {
        assert_eq!(BhdlCommand::ValidateDesign.title(), "Validate BHDL Design");
        assert_eq!(BhdlCommand::ShowComponentCount.title(), "Show Component Count");
        assert_eq!(BhdlCommand::GenerateSchematic.title(), "Generate Schematic JSON");
    }
}
