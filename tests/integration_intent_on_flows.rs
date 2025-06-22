// Integration test for intent on flow statements
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

#[test]
fn test_intent_on_flow_statements() {
    let test_bhdl = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Named flow with intent
    critical_path: @VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
    
    // Direct connection with intent
    @VCC -> Cap(100n).1 -> @GND for decoupling();
}
"#;
    
    // Parse
    let parse_result = parse(test_bhdl);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
    
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be a SourceFile");
    
    // Analyze
    let result = analyze(&source_file);
    
    // Check that analysis completes without critical errors
    let critical_errors: Vec<_> = result.diagnostics.iter()
        .filter(|d| d.message.contains("Undefined") && !d.message.contains("red"))
        .collect();
    assert!(critical_errors.is_empty(), "Critical analysis errors: {:?}", critical_errors);
    
    // Check flow tracking results
    assert_eq!(result.flow_tracking.flow_paths.len(), 2, "Should have 2 flow paths");
    
    // Check first flow path (critical_path)
    let first_flow = &result.flow_tracking.flow_paths[0];
    assert!(first_flow.intent.is_some(), "First flow should have intent");
    if let Some(intent) = &first_flow.intent {
        assert_eq!(intent.function_name, "delay");
        assert_eq!(intent.positional_args.len(), 1);
        assert_eq!(intent.positional_args[0], "3ms");
    }
    
    // Check second flow path
    let second_flow = &result.flow_tracking.flow_paths[1];
    assert!(second_flow.intent.is_some(), "Second flow should have intent");
    if let Some(intent) = &second_flow.intent {
        assert_eq!(intent.function_name, "decoupling");
        assert_eq!(intent.positional_args.len(), 0);
    }
}