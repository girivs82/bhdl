//! Component synthesis engine demonstration
//!
//! This example shows how to use the BHDL component synthesis engine to convert
//! component requirements into ranked component selections.

use bhdl_components::synthesis::SynthesisEngine;
use bhdl_components::types::*;
use bhdl_components::database::ComponentDatabase;
use std::path::Path;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    println!("BHDL Component Synthesis Engine Demo");
    println!("====================================");

    // Create synthesis engine
    let engine = SynthesisEngine::new();
    
    println!("\nSynthesis Engine Configuration:");
    let stats = engine.get_synthesis_stats();
    println!("  Version: {}", stats.matcher_version);
    println!("  Max candidates: {}", stats.max_candidates);
    println!("  Max alternatives: {}", stats.max_alternatives);

    // Set up a temporary database for demonstration
    let db_path = Path::new("demo_components.db");
    let database = ComponentDatabase::new(db_path).await?;

    // Demo 1: Synthesize a resistor
    println!("\n--- Demo 1: Resistor Synthesis ---");
    demo_resistor_synthesis(&engine, &database).await?;

    // Demo 2: Synthesize a capacitor
    println!("\n--- Demo 2: Capacitor Synthesis ---");
    demo_capacitor_synthesis(&engine, &database).await?;

    // Demo 3: Custom selection criteria
    println!("\n--- Demo 3: Custom Selection Criteria ---");
    demo_custom_criteria(&engine, &database).await?;

    // Demo 4: Application-specific optimization
    println!("\n--- Demo 4: Application-Specific Optimization ---");
    demo_application_optimization(&engine, &database).await?;

    println!("\nDemo completed successfully!");
    
    // Clean up demo database
    if db_path.exists() {
        std::fs::remove_file(db_path)?;
    }

    Ok(())
}

async fn demo_resistor_synthesis(engine: &SynthesisEngine, database: &ComponentDatabase) -> Result<()> {
    // Create requirements for a 1k ohm resistor
    let mut requirements = ComponentRequirements::resistor(1000.0, 0.25, 0.05, 100);
    requirements.max_unit_price = Some(0.50);
    requirements.max_lead_time_days = Some(30);
    requirements.package_type = Some("0603".to_string());

    println!("Requirements:");
    println!("  - Resistance: 1kΩ ±5%");
    println!("  - Power: 0.25W");
    println!("  - Package: 0603");
    println!("  - Quantity: 100");
    println!("  - Max price: $0.50");
    println!("  - Max lead time: 30 days");

    // Synthesize component
    let result = engine.synthesize_component("resistor", &requirements, database).await?;

    print_synthesis_result(&result);
    Ok(())
}

async fn demo_capacitor_synthesis(engine: &SynthesisEngine, database: &ComponentDatabase) -> Result<()> {
    // Create requirements for a 10µF capacitor
    let mut requirements = ComponentRequirements::capacitor(10e-6, 25.0, 0.20, 50);
    requirements.max_unit_price = Some(1.00);
    requirements.temperature_range = Some((-40.0, 85.0));
    requirements.application = ComponentApplication::PowerSupply;

    println!("Requirements:");
    println!("  - Capacitance: 10µF ±20%");
    println!("  - Voltage rating: 25V");
    println!("  - Temperature: -40°C to +85°C");
    println!("  - Application: Power Supply");
    println!("  - Quantity: 50");
    println!("  - Max price: $1.00");

    // Synthesize component
    let result = engine.synthesize_component("capacitor", &requirements, database).await?;

    print_synthesis_result(&result);
    Ok(())
}

async fn demo_custom_criteria(engine: &SynthesisEngine, database: &ComponentDatabase) -> Result<()> {
    let requirements = ComponentRequirements::resistor(10000.0, 0.125, 0.01, 1000);

    // Create cost-optimized criteria
    let cost_criteria = SelectionCriteria {
        price_weight: 0.7,
        availability_weight: 0.2,
        lead_time_weight: 0.05,
        spec_match_weight: 0.03,
        reliability_weight: 0.02,
    };

    println!("Requirements: 10kΩ precision resistor (1000 units)");
    println!("Selection criteria: Cost-optimized");
    println!("  - Price weight: 70%");
    println!("  - Availability weight: 20%");
    println!("  - Lead time weight: 5%");
    println!("  - Spec match weight: 3%");
    println!("  - Reliability weight: 2%");

    let result = engine.synthesize_with_criteria(
        "resistor", &requirements, &cost_criteria, database
    ).await?;

    print_synthesis_result(&result);
    Ok(())
}

async fn demo_application_optimization(engine: &SynthesisEngine, database: &ComponentDatabase) -> Result<()> {
    let mut requirements = ComponentRequirements::capacitor(100e-6, 50.0, 0.10, 20);
    requirements.application = ComponentApplication::PowerManagement;
    requirements.criticality = ComponentCriticality::Critical;

    println!("Requirements: 100µF power management capacitor");
    println!("  - Application: Power Management");
    println!("  - Criticality: Critical");
    println!("  - Voltage rating: 50V");
    println!("  - Quantity: 20");

    let result = engine.synthesize_component("capacitor", &requirements, database).await?;

    print_synthesis_result(&result);
    Ok(())
}

fn print_synthesis_result(result: &SynthesisResult) {
    println!("\nSynthesis Result:");
    println!("  Confidence: {:.1}%", result.confidence * 100.0);
    println!("  Success: {}", if result.is_successful() { "Yes" } else { "No" });
    
    if let Some(recommended) = &result.recommended {
        println!("\nRecommended Component:");
        println!("  Name: {}", recommended.component.name);
        println!("  Manufacturer: {}", 
                recommended.component.manufacturer.as_deref().unwrap_or("Unknown"));
        println!("  Part Number: {}", 
                recommended.component.part_number.as_deref().unwrap_or("Unknown"));
        println!("  Fitness Score: {:.3}", recommended.fitness_score);
        println!("  Selection Reason: {}", recommended.selection_reason);
        println!("  Unit Price: ${:.3}", recommended.supplier_choice.unit_price);
        println!("  Total Cost: ${:.2}", recommended.total_cost);
        println!("  Lead Time: {} days", 
                recommended.supplier_choice.lead_time_days.unwrap_or(0));
    }

    if result.alternatives.len() > 1 {
        println!("\nAlternatives ({}):", result.alternatives.len() - 1);
        for (i, alt) in result.alternatives.iter().skip(1).take(3).enumerate() {
            println!("  {}. {} - Score: {:.3} - ${:.3}", 
                    i + 2, 
                    alt.component.name, 
                    alt.fitness_score,
                    alt.supplier_choice.unit_price);
        }
    }

    if !result.synthesis_notes.is_empty() {
        println!("\nSynthesis Notes:");
        for note in &result.synthesis_notes {
            println!("  - {}", note);
        }
    }
}