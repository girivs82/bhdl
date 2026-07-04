//! Test component model extraction from various sources
//! 
//! Demonstrates extracting SPICE models from:
//! - Symbol table (analyzer results)
//! - User-defined attributes
//! - Circuit context inference

use std::collections::HashMap;
use bhdl_spice::{
    ComponentModelExtractor, ExtractedModel, ModelSource,
    models::SpiceModel,
};

fn main() {
    println!("Component Model Extraction Test");
    println!("==============================\n");
    
    test_symbol_table_extraction();
    test_user_attribute_extraction();
    test_context_inference();
    test_model_creation();
}

fn test_symbol_table_extraction() {
    println!("1. Symbol Table Extraction");
    println!("--------------------------");
    
    let mut extractor = ComponentModelExtractor::new();
    
    // Simulate symbol table data from analyzer
    let mut symbol_data = HashMap::new();
    symbol_data.insert("component_type".to_string(), "resistor".to_string());
    symbol_data.insert("value".to_string(), "4.7k".to_string());
    symbol_data.insert("power".to_string(), "0.25W".to_string());
    symbol_data.insert("tolerance".to_string(), "5%".to_string());
    symbol_data.insert("package".to_string(), "0805".to_string());
    
    match extractor.extract_from_symbol_table("R1", &symbol_data) {
        Ok(model) => {
            print_extracted_model(&model);
        }
        Err(e) => {
            eprintln!("Failed to extract from symbol table: {}", e);
        }
    }
    
    println!();
}

fn test_user_attribute_extraction() {
    println!("2. User Attribute Extraction");
    println!("----------------------------");
    
    let mut extractor = ComponentModelExtractor::new();
    
    // User-defined SPICE attributes
    let mut user_attrs = HashMap::new();
    user_attrs.insert("spice_model".to_string(), "diode".to_string());
    user_attrs.insert("spice_is".to_string(), "1e-12".to_string());
    user_attrs.insert("spice_n".to_string(), "1.5".to_string());
    user_attrs.insert("spice_rs".to_string(), "10".to_string());
    user_attrs.insert("spice_vj".to_string(), "0.7".to_string());
    user_attrs.insert("spice_bv".to_string(), "50".to_string());
    user_attrs.insert("part_number".to_string(), "1N4148".to_string());
    
    match extractor.extract_from_user_attributes("D1", &user_attrs) {
        Ok(model) => {
            print_extracted_model(&model);
        }
        Err(e) => {
            eprintln!("Failed to extract from user attributes: {}", e);
        }
    }
    
    println!();
}

fn test_context_inference() {
    println!("3. Context-Based Inference");
    println!("--------------------------");
    
    let mut extractor = ComponentModelExtractor::new();
    
    // Infer from circuit connections
    let connections = vec![
        "VCC".to_string(),
        "LED1.anode".to_string(),
    ];
    
    let nearby_components = vec![
        "LED1".to_string(),
        "C1".to_string(),
        "U1".to_string(),
    ];
    
    match extractor.infer_from_context("R2", &connections, &nearby_components) {
        Ok(model) => {
            print_extracted_model(&model);
            println!("   Note: Low confidence due to inference");
        }
        Err(e) => {
            eprintln!("Failed to infer from context: {}", e);
        }
    }
    
    println!();
}

fn test_model_creation() {
    println!("4. SPICE Model Creation");
    println!("-----------------------");
    
    let mut extractor = ComponentModelExtractor::new();
    
    // Create models from different sources
    let test_cases = vec![
        // LED model
        {
            let mut attrs = HashMap::new();
            attrs.insert("spice_model".to_string(), "diode".to_string());
            attrs.insert("spice_type".to_string(), "led".to_string());
            attrs.insert("spice_vj".to_string(), "2.0".to_string());
            attrs.insert("spice_is".to_string(), "1e-15".to_string());
            attrs.insert("spice_n".to_string(), "2.0".to_string());
            attrs.insert("color".to_string(), "red".to_string());
            ("LED1", attrs)
        },
        // Capacitor model
        {
            let mut attrs = HashMap::new();
            attrs.insert("spice_model".to_string(), "capacitor".to_string());
            attrs.insert("spice_capacitance".to_string(), "100e-6".to_string());
            attrs.insert("spice_voltage_rating".to_string(), "16".to_string());
            attrs.insert("spice_esr".to_string(), "0.1".to_string());
            attrs.insert("type".to_string(), "electrolytic".to_string());
            ("C1", attrs)
        },
    ];
    
    for (name, attrs) in test_cases {
        println!("\nCreating SPICE model for {}:", name);
        
        match extractor.extract_from_user_attributes(name, &attrs) {
            Ok(extracted) => {
                match extractor.create_spice_model(&extracted) {
                    Ok(spice_model) => {
                        println!("  Model type: {:?}", spice_model.model_type());
                        println!("  Terminals: {}", spice_model.num_terminals());
                        println!("  Nonlinear: {}", spice_model.is_nonlinear());
                        println!("  Parameters:");
                        for (param, value) in spice_model.parameters() {
                            println!("    {}: {}", param, value);
                        }
                    }
                    Err(e) => {
                        eprintln!("  Failed to create SPICE model: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("  Failed to extract model: {}", e);
            }
        }
    }
}

fn print_extracted_model(model: &ExtractedModel) {
    println!("Extracted Model: {}", model.name);
    println!("  Source: {:?}", model.source);
    println!("  Type: {:?}", model.component_type);
    println!("  Confidence: {:.1}%", model.confidence * 100.0);
    
    if !model.parameters.is_empty() {
        println!("  Parameters:");
        for (key, value) in &model.parameters {
            println!("    {}: {}", key, value);
        }
    }
    
    if !model.attributes.is_empty() {
        println!("  Attributes:");
        for (key, value) in &model.attributes {
            println!("    {}: {}", key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extraction_confidence() {
        let mut extractor = ComponentModelExtractor::new();
        
        // Symbol table should have high confidence
        let mut symbol_data = HashMap::new();
        symbol_data.insert("component_type".to_string(), "resistor".to_string());
        symbol_data.insert("value".to_string(), "1k".to_string());
        
        let model = extractor.extract_from_symbol_table("R1", &symbol_data).unwrap();
        assert!(model.confidence > 0.8);
        
        // Inferred should have low confidence
        let model = extractor.infer_from_context("R2", &[], &[]).unwrap();
        assert!(model.confidence < 0.5);
    }
}