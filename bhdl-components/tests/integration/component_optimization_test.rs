//! Component optimization and alternative selection tests
//! 
//! Tests the advanced features of the component intelligence system:
//! - Alternative component suggestion based on requirements
//! - Cost optimization across different suppliers
//! - Package optimization for PCB constraints
//! - Performance-based component ranking

use anyhow::Result;
use std::collections::HashMap;
use tempfile::TempDir;
use tokio_test;

use bhdl_components::{
    database::ComponentDatabase,
    synthesis::{
        matcher::ComponentMatcher,
        optimizer::{ComponentOptimizer, SelectionCriteria, OptimizationGoal},
    },
    types::{
        Component, ComponentType, ComponentRequirements, QuantityRequirement,
        SupplierInfo, PriceBreak,
    },
};

#[tokio::test]
async fn test_component_alternative_optimization() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_optimization.db");
    
    let database = ComponentDatabase::new(&db_path).await?;
    
    println!("🎯 Testing component alternative optimization...");
    
    // Add a diverse set of resistor components with different characteristics
    add_resistor_alternatives(&database).await?;
    
    // Create requirements for a 10kΩ resistor
    let requirements = ComponentRequirements {
        component_type: ComponentType::Resistor,
        properties: vec![
            ("resistance".to_string(), "10k".to_string()),
            ("tolerance".to_string(), "5%".to_string()),
        ].into_iter().collect(),
        quantity: Some(QuantityRequirement::Exactly(1000)),
        max_cost_per_unit: Some(0.20),
        preferred_packages: vec!["0805".to_string(), "0603".to_string()],
        temperature_range: Some((-40.0, 85.0)),
        notes: Some("For digital pull-up resistors".to_string()),
    };
    
    // Test different optimization goals
    let optimization_goals = vec![
        OptimizationGoal::MinimizeCost,
        OptimizationGoal::MaximizeReliability,
        OptimizationGoal::OptimizeAvailability,
        OptimizationGoal::Balanced,
    ];
    
    let matcher = ComponentMatcher::new(database.clone());
    let optimizer = ComponentOptimizer::new();
    
    for goal in optimization_goals {
        println!("\n📊 Testing optimization goal: {:?}", goal);
        
        // Find candidate components
        let candidates = matcher.find_matching_components(&requirements).await?;
        println!("   Found {} candidate components", candidates.len());
        
        if candidates.is_empty() {
            continue;
        }
        
        // Add mock supplier data to components
        let mut components_with_suppliers = Vec::new();
        for (i, component) in candidates.into_iter().enumerate() {
            let supplier_info = create_mock_supplier_data(&component, i);
            components_with_suppliers.push((component, vec![supplier_info]));
        }
        
        // Create selection criteria
        let criteria = SelectionCriteria {
            optimization_goal: goal,
            max_cost_per_unit: requirements.max_cost_per_unit,
            preferred_packages: requirements.preferred_packages.clone(),
            required_quantity: requirements.quantity.clone().unwrap_or(QuantityRequirement::Exactly(1)),
            temperature_range: requirements.temperature_range,
            reliability_priority: match goal {
                OptimizationGoal::MaximizeReliability => 1.0,
                OptimizationGoal::Balanced => 0.5,
                _ => 0.2,
            },
            cost_priority: match goal {
                OptimizationGoal::MinimizeCost => 1.0,
                OptimizationGoal::Balanced => 0.5,
                _ => 0.2,
            },
            availability_priority: match goal {
                OptimizationGoal::OptimizeAvailability => 1.0,
                OptimizationGoal::Balanced => 0.5,
                _ => 0.2,
            },
        };
        
        // Optimize component selection
        let optimized_components = optimizer.optimize_component_selection(
            components_with_suppliers,
            &criteria,
        ).await?;
        
        println!("   Optimized to {} top options", optimized_components.len());
        
        // Show top 3 results
        for (i, option) in optimized_components.iter().take(3).enumerate() {
            println!("   {}. {} (Score: {:.2})", i + 1, option.component.name, option.score);
            println!("      Package: {}", option.component.properties.get("package").unwrap_or(&"Unknown".to_string()));
            println!("      Estimated cost: ${:.4}", option.cost_analysis.estimated_unit_cost);
            println!("      Total cost (1000): ${:.2}", option.cost_analysis.total_cost);
            if let Some(supplier) = option.supplier_options.first() {
                println!("      Best supplier: {} (Stock: {})", supplier.supplier_name, supplier.availability);
            }
        }
        
        // Verify optimization worked
        assert!(!optimized_components.is_empty(), "Optimization should return at least one option");
        
        // Verify ordering based on goal
        if optimized_components.len() > 1 {
            match goal {
                OptimizationGoal::MinimizeCost => {
                    let first_cost = optimized_components[0].cost_analysis.estimated_unit_cost;
                    let second_cost = optimized_components[1].cost_analysis.estimated_unit_cost;
                    assert!(first_cost <= second_cost, "Cost optimization should order by cost");
                }
                OptimizationGoal::OptimizeAvailability => {
                    if let (Some(first_supplier), Some(second_supplier)) = (
                        optimized_components[0].supplier_options.first(),
                        optimized_components[1].supplier_options.first()
                    ) {
                        assert!(first_supplier.availability >= second_supplier.availability,
                                "Availability optimization should order by stock");
                    }
                }
                _ => {} // Other goals are more complex to verify
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_package_constraint_optimization() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_package_opt.db");
    
    let database = ComponentDatabase::new(&db_path).await?;
    
    println!("📦 Testing package constraint optimization...");
    
    // Add components with various packages
    add_multi_package_components(&database).await?;
    
    // Test different package constraints
    let package_tests = vec![
        (vec!["0603".to_string()], "Small form factor"),
        (vec!["0805".to_string(), "0603".to_string()], "Preferred SMD packages"),
        (vec!["1206".to_string(), "2512".to_string()], "High power packages"),
        (vec!["THT".to_string()], "Through-hole only"),
    ];
    
    let matcher = ComponentMatcher::new(database.clone());
    let optimizer = ComponentOptimizer::new();
    
    for (preferred_packages, description) in package_tests {
        println!("\n📋 Testing: {} ({:?})", description, preferred_packages);
        
        let requirements = ComponentRequirements {
            component_type: ComponentType::Resistor,
            properties: vec![("resistance".to_string(), "1k".to_string())].into_iter().collect(),
            quantity: Some(QuantityRequirement::Exactly(100)),
            max_cost_per_unit: Some(0.50),
            preferred_packages: preferred_packages.clone(),
            temperature_range: None,
            notes: Some(description.to_string()),
        };
        
        let candidates = matcher.find_matching_components(&requirements).await?;
        println!("   Found {} candidates", candidates.len());
        
        // Check that candidates respect package constraints
        let mut package_distribution = HashMap::new();
        for component in &candidates {
            if let Some(package) = component.properties.get("package") {
                *package_distribution.entry(package.clone()).or_insert(0) += 1;
            }
        }
        
        println!("   Package distribution:");
        for (package, count) in &package_distribution {
            println!("     {}: {} components", package, count);
        }
        
        // Verify preferred packages are found
        let found_preferred = package_distribution.keys()
            .any(|pkg| preferred_packages.contains(pkg));
        
        if !candidates.is_empty() {
            assert!(found_preferred, "Should find components in preferred packages");
        }
        
        // Test optimization with package preferences
        if !candidates.is_empty() {
            let mut components_with_suppliers = Vec::new();
            for (i, component) in candidates.into_iter().take(5).enumerate() {
                let supplier_info = create_mock_supplier_data(&component, i);
                components_with_suppliers.push((component, vec![supplier_info]));
            }
            
            let criteria = SelectionCriteria {
                optimization_goal: OptimizationGoal::Balanced,
                max_cost_per_unit: requirements.max_cost_per_unit,
                preferred_packages: preferred_packages.clone(),
                required_quantity: requirements.quantity.clone().unwrap(),
                temperature_range: requirements.temperature_range,
                reliability_priority: 0.3,
                cost_priority: 0.4,
                availability_priority: 0.3,
            };
            
            let optimized = optimizer.optimize_component_selection(
                components_with_suppliers,
                &criteria,
            ).await?;
            
            println!("   Optimized to {} options", optimized.len());
            
            // Check that preferred packages are prioritized
            if let Some(best_option) = optimized.first() {
                if let Some(package) = best_option.component.properties.get("package") {
                    println!("   Best package: {}", package);
                    // Preferred packages should get higher scores
                    if preferred_packages.contains(package) {
                        println!("   ✅ Preferred package selected");
                    }
                }
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_cost_vs_performance_tradeoffs() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_tradeoffs.db");
    
    let database = ComponentDatabase::new(&db_path).await?;
    
    println!("⚖️  Testing cost vs performance tradeoffs...");
    
    // Add components with different cost/performance profiles
    add_performance_range_components(&database).await?;
    
    let optimizer = ComponentOptimizer::new();
    
    // Test scenarios with different budget constraints
    let budget_scenarios = vec![
        (0.05, "Ultra low cost"),
        (0.15, "Budget conscious"),
        (0.50, "Standard budget"),
        (2.00, "Premium budget"),
    ];
    
    for (max_cost, scenario) in budget_scenarios {
        println!("\n💰 Testing scenario: {} (max ${:.2})", scenario, max_cost);
        
        let requirements = ComponentRequirements {
            component_type: ComponentType::Capacitor,
            properties: vec![
                ("capacitance".to_string(), "10uF".to_string()),
                ("voltage_rating".to_string(), "25V".to_string()),
            ].into_iter().collect(),
            quantity: Some(QuantityRequirement::Exactly(100)),
            max_cost_per_unit: Some(max_cost),
            preferred_packages: vec!["0805".to_string(), "1206".to_string()],
            temperature_range: Some((-40.0, 85.0)),
            notes: Some(format!("Budget scenario: {}", scenario)),
        };
        
        let matcher = ComponentMatcher::new(database.clone());
        let candidates = matcher.find_matching_components(&requirements).await?;
        
        // Filter by budget
        let budget_candidates: Vec<_> = candidates.into_iter()
            .filter(|c| {
                // Mock cost calculation based on component name
                let estimated_cost = estimate_component_cost(&c.name);
                estimated_cost <= max_cost
            })
            .collect();
        
        println!("   Found {} components within budget", budget_candidates.len());
        
        if !budget_candidates.is_empty() {
            // Create mock supplier data and optimize
            let mut components_with_suppliers = Vec::new();
            for (i, component) in budget_candidates.into_iter().take(8).enumerate() {
                let supplier_info = create_mock_supplier_data(&component, i);
                components_with_suppliers.push((component, vec![supplier_info]));
            }
            
            let criteria = SelectionCriteria {
                optimization_goal: OptimizationGoal::MinimizeCost,
                max_cost_per_unit: Some(max_cost),
                preferred_packages: requirements.preferred_packages.clone(),
                required_quantity: requirements.quantity.clone().unwrap(),
                temperature_range: requirements.temperature_range,
                reliability_priority: 0.3,
                cost_priority: 0.7, // Emphasize cost for this test
                availability_priority: 0.2,
            };
            
            let optimized = optimizer.optimize_component_selection(
                components_with_suppliers,
                &criteria,
            ).await?;
            
            if let Some(best_option) = optimized.first() {
                println!("   Best option: {}", best_option.component.name);
                println!("   Cost: ${:.3} (Score: {:.2})", 
                        best_option.cost_analysis.estimated_unit_cost, 
                        best_option.score);
                
                // Verify cost constraint
                assert!(best_option.cost_analysis.estimated_unit_cost <= max_cost,
                       "Selected component should be within budget");
            }
        }
    }
    
    Ok(())
}

// Helper functions for creating test data

async fn add_resistor_alternatives(database: &ComponentDatabase) -> Result<()> {
    let resistors = vec![
        // Basic options
        ("R_10k_0805_5%", "10k", "0805", "5%", "0.125W", "Basic"),
        ("R_10k_0603_5%", "10k", "0603", "5%", "0.1W", "Basic"),
        ("R_10k_1206_5%", "10k", "1206", "5%", "0.25W", "Basic"),
        
        // Precision options
        ("R_10k_0805_1%", "10k", "0805", "1%", "0.125W", "Precision"),
        ("R_10k_0603_1%", "10k", "0603", "1%", "0.1W", "Precision"),
        
        // High power options
        ("R_10k_2512_1%", "10k", "2512", "1%", "1W", "High Power"),
        
        // Through-hole options
        ("R_10k_THT_5%", "10k", "THT", "5%", "0.25W", "Through Hole"),
    ];
    
    for (name, resistance, package, tolerance, power, category) in resistors {
        let mut properties = HashMap::new();
        properties.insert("resistance".to_string(), resistance.to_string());
        properties.insert("package".to_string(), package.to_string());
        properties.insert("tolerance".to_string(), tolerance.to_string());
        properties.insert("power_rating".to_string(), power.to_string());
        properties.insert("category".to_string(), category.to_string());
        
        let component = Component {
            id: 0,
            name: name.to_string(),
            component_type: ComponentType::Resistor.to_string(),
            part_number: Some(format!("OPT_{}", name)),
            manufacturer: Some("Test Optimization Corp".to_string()),
            description: Some(format!("{} resistor, {} package, {} category", resistance, package, category)),
            properties,
            package_info: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        database.insert_component(&component).await?;
    }
    
    Ok(())
}

async fn add_multi_package_components(database: &ComponentDatabase) -> Result<()> {
    let packages = vec!["0402", "0603", "0805", "1206", "2512", "THT"];
    let resistances = vec!["1k", "4.7k", "10k"];
    
    for resistance in &resistances {
        for package in &packages {
            let name = format!("R_{}_{}", resistance, package);
            let mut properties = HashMap::new();
            properties.insert("resistance".to_string(), resistance.to_string());
            properties.insert("package".to_string(), package.to_string());
            properties.insert("tolerance".to_string(), "5%".to_string());
            
            // Assign power rating based on package size
            let power = match *package {
                "0402" => "0.063W",
                "0603" => "0.1W",
                "0805" => "0.125W",
                "1206" => "0.25W",
                "2512" => "1W",
                "THT" => "0.25W",
                _ => "0.125W",
            };
            properties.insert("power_rating".to_string(), power.to_string());
            
            let component = Component {
                id: 0,
                name,
                component_type: ComponentType::Resistor.to_string(),
                part_number: Some(format!("PKG_{}_{}", resistance, package)),
                manufacturer: Some("Package Test Mfg".to_string()),
                description: Some(format!("{} resistor in {} package", resistance, package)),
                properties,
                package_info: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            
            database.insert_component(&component).await?;
        }
    }
    
    Ok(())
}

async fn add_performance_range_components(database: &ComponentDatabase) -> Result<()> {
    let capacitors = vec![
        // Budget options
        ("C_10uF_Basic", "10uF", "1206", "25V", "X5R", "Basic"),
        ("C_10uF_Economy", "10uF", "0805", "16V", "Y5V", "Economy"),
        
        // Standard options
        ("C_10uF_Standard", "10uF", "1206", "25V", "X7R", "Standard"),
        ("C_10uF_Standard_805", "10uF", "0805", "25V", "X7R", "Standard"),
        
        // Premium options
        ("C_10uF_Premium", "10uF", "1206", "50V", "X7R", "Premium"),
        ("C_10uF_Ultra", "10uF", "0805", "50V", "C0G", "Ultra Premium"),
        
        // High-end options
        ("C_10uF_Military", "10uF", "1206", "100V", "C0G", "Military Grade"),
        ("C_10uF_Space", "10uF", "1210", "100V", "C0G", "Space Grade"),
    ];
    
    for (name, capacitance, package, voltage, dielectric, grade) in capacitors {
        let mut properties = HashMap::new();
        properties.insert("capacitance".to_string(), capacitance.to_string());
        properties.insert("package".to_string(), package.to_string());
        properties.insert("voltage_rating".to_string(), voltage.to_string());
        properties.insert("dielectric".to_string(), dielectric.to_string());
        properties.insert("grade".to_string(), grade.to_string());
        
        let component = Component {
            id: 0,
            name: name.to_string(),
            component_type: ComponentType::Capacitor.to_string(),
            part_number: Some(format!("PERF_{}", name)),
            manufacturer: Some("Performance Test Corp".to_string()),
            description: Some(format!("{} capacitor, {} grade", capacitance, grade)),
            properties,
            package_info: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        database.insert_component(&component).await?;
    }
    
    Ok(())
}

fn create_mock_supplier_data(component: &Component, index: usize) -> SupplierInfo {
    // Create realistic but varied supplier data
    let base_cost = estimate_component_cost(&component.name);
    let stock_variation = (index % 5) as i32;
    
    SupplierInfo {
        supplier_name: "Mock Supplier".to_string(),
        supplier_part_number: format!("MOCK-{}-{:03}", component.name.chars().take(8).collect::<String>(), index),
        manufacturer_part_number: component.part_number.clone().unwrap_or_else(|| component.name.clone()),
        manufacturer: component.manufacturer.clone().unwrap_or_else(|| "Unknown".to_string()),
        availability: 1000 + stock_variation * 500,
        lead_time_days: Some(1 + (index % 3) as i32),
        moq: 1 + (index % 10) as i32,
        price_breaks: vec![
            PriceBreak {
                quantity: 1,
                unit_price: base_cost * (1.2 + index as f64 * 0.1),
                currency: "USD".to_string(),
            },
            PriceBreak {
                quantity: 100,
                unit_price: base_cost * (1.0 + index as f64 * 0.05),
                currency: "USD".to_string(),
            },
            PriceBreak {
                quantity: 1000,
                unit_price: base_cost * (0.9 + index as f64 * 0.02),
                currency: "USD".to_string(),
            },
        ],
        datasheet_url: Some(format!("https://example.com/datasheets/{}.pdf", component.name)),
        last_updated: chrono::Utc::now(),
    }
}

fn estimate_component_cost(name: &str) -> f64 {
    // Simple cost estimation based on component name patterns
    if name.contains("Premium") || name.contains("Ultra") {
        return 0.50;
    }
    if name.contains("Military") || name.contains("Space") {
        return 2.00;
    }
    if name.contains("Basic") || name.contains("Economy") {
        return 0.05;
    }
    if name.contains("0402") {
        return 0.08;
    }
    if name.contains("2512") || name.contains("THT") {
        return 0.25;
    }
    if name.contains("1%") {
        return 0.20;
    }
    
    0.12 // Default cost
}