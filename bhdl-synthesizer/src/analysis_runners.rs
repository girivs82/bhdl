//! Analysis-runner methods for `NetlistGenerator`.
//!
//! Each `run_*` method wires the in-progress netlist + analysis
//! result into one of the sibling analysis crates
//! (`component_compatibility`, `design_pattern_recognition`,
//! `cross_component_optimization`, `design_rule_checker`,
//! `ml_component_selection`, `thermal_simulation`,
//! `cost_optimization`, `emi_emc_analysis`,
//! `reliability_analysis`, `predictive_analytics`,
//! `manufacturing_optimization`) and stashes the result back on
//! `self`. These methods are coupled to `NetlistGenerator`'s
//! fields but not to each other (apart from a few `estimate_*` /
//! `extract_*` helpers that stay together with their consumer),
//! so they form a natural extraction unit.
//!
//! Split out of `lib.rs` on 2026-05-26 to drop the central file
//! from 4552 → ~3150 LOC. Each runner is now its own incremental-
//! compile target; touching this file (a typical edit when tuning
//! a single analysis pass) no longer invalidates the rest of
//! `lib.rs` at the rmeta level.
//!
//! Visibility: methods are declared `pub(crate)` so the main
//! pipeline (`generate_from_ast_and_analysis_internal` in
//! `lib.rs`) can call them. They are deliberately NOT `pub` —
//! external callers should go through the top-level pipeline
//! entry points, not invoke individual analyses directly.

use super::*;

impl NetlistGenerator {
    
    /// Run component compatibility analysis on the generated netlist  
    pub(crate) async fn run_compatibility_analysis(&self, analysis: &AnalysisResult) -> Result<()> {
        use crate::component_compatibility::ComponentCompatibilityAnalyzer;
        use std::path::Path;
        
        info!("Running component compatibility analysis on {} components", self.netlist.instances.len());
        
        // Initialize the compatibility analyzer (try to use database if available)
        let analyzer = if let Some(ref db_path) = self.config.database_path {
            match ComponentCompatibilityAnalyzer::with_database(Path::new(db_path)).await {
                Ok(analyzer) => {
                    info!("Using real component database for compatibility analysis");
                    analyzer
                },
                Err(e) => {
                    warn!("Failed to connect to component database: {}. Using mock data.", e);
                    ComponentCompatibilityAnalyzer::new()
                }
            }
        } else {
            info!("No database path configured. Using mock compatibility data.");
            ComponentCompatibilityAnalyzer::new()
        };
        
        // Run the compatibility analysis
        match analyzer.analyze_compatibility(&self.netlist, analysis) {
            Ok(report) => {
                // Report compatibility results to the user
                info!("=== Component Compatibility Analysis Results ===");
                info!("Overall compatibility score: {:.1}%", report.overall_compatibility_score * 100.0);
                
                // Report power domain analysis
                if !report.power_domain_analysis.is_empty() {
                    info!("Power Domain Analysis:");
                    for (i, domain) in report.power_domain_analysis.iter().enumerate() {
                        info!("  {}. Domain '{}' ({:.1}V) - {} components, {:.1}A capacity", 
                              i + 1, domain.domain_name, domain.nominal_voltage, 
                              domain.connected_components.len(), domain.max_current);
                        
                        if !domain.compatibility_issues.is_empty() {
                            warn!("     {} compatibility issues found", domain.compatibility_issues.len());
                            for issue in &domain.compatibility_issues {
                                warn!("     - {}: {}", issue.title, issue.description);
                            }
                        }
                    }
                }
                
                // Report thermal analysis
                if !report.thermal_analysis.is_empty() {
                    info!("Thermal Analysis:");
                    for (i, zone) in report.thermal_analysis.iter().enumerate() {
                        info!("  {}. Zone '{}' - {:.2}W dissipation, max {:.1}°C", 
                              i + 1, zone.thermal_zone, zone.total_power_dissipation, zone.max_junction_temp);
                    }
                }
                
                // Report critical issues
                if !report.critical_issues.is_empty() {
                    warn!("Critical compatibility issues found:");
                    for issue in &report.critical_issues {
                        warn!("  - {}: {}", issue.title, issue.description);
                        warn!("    Recommended action: {}", issue.recommended_action);
                    }
                } else {
                    info!("No critical compatibility issues detected");
                }
                
                // Report optimization opportunities
                if !report.optimization_opportunities.is_empty() {
                    info!("Optimization opportunities identified:");
                    for opportunity in &report.optimization_opportunities {
                        info!("  - {}: {}", opportunity.title, opportunity.description);
                    }
                }
                
                info!("Component compatibility analysis completed successfully");
            },
            Err(e) => {
                warn!("Component compatibility analysis failed: {}", e);
                // Don't fail the entire synthesis - just log the warning
            }
        }
        
        Ok(())
    }
    
    /// Run design pattern recognition on the generated netlist
    pub(crate) fn run_pattern_recognition(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::design_pattern_recognition::DesignPatternRecognizer;
        
        info!("Running design pattern recognition on {} components", self.netlist.instances.len());
        
        let mut recognizer = DesignPatternRecognizer::new();
        
        // Recognize patterns in the netlist
        match recognizer.recognize_patterns(&self.netlist, analysis) {
            Ok(report) => {
                info!("Recognized {} circuit patterns", report.recognized_patterns.len());
                for pattern in &report.recognized_patterns {
                    info!("  - {} (confidence: {:.1}%)", pattern.pattern_name, pattern.confidence_score * 100.0);
                    if !pattern.matched_components.is_empty() {
                        info!("    Components: {} instances", pattern.matched_components.len());
                    }
                }
            },
            Err(e) => {
                warn!("Pattern recognition failed: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Run cross-component optimization
    pub(crate) fn run_cross_component_optimization(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::cross_component_optimization::CrossComponentOptimizer;
        
        info!("Running cross-component optimization");
        
        let mut optimizer = CrossComponentOptimizer::new();
        
        // Analyze coordination opportunities  
        // Note: Using empty behavioral models array as we don't have them yet
        let behavioral_models = Vec::new();
        match optimizer.analyze_coordination_opportunities(&self.netlist, &behavioral_models) {
            Ok(plan) => {
                info!("Found coordination plan with {} participants", plan.total_participants);
                
                // Execute coordinated optimization if there are participants
                if plan.total_participants > 0 {
                    // Create initial design parameters (would come from simulation in full implementation)
                    let initial_params = bhdl_simulation::DesignParameters::new();
                    match optimizer.execute_coordinated_optimization(&mut self.netlist, &initial_params) {
                        Ok(result) => {
                            info!("Cross-component optimization completed:");
                            info!("  - {} optimization phases executed", result.phase_results.len());
                            info!("  - Objectives met: {}", result.objectives_met);
                            
                            // Log phase results
                            for phase in &result.phase_results {
                                let total_objectives = phase.objectives_achieved.len();
                                info!("    Phase '{}': {} participants, {} objectives achieved",
                                      phase.phase_name, phase.participants_optimized, total_objectives);
                            }
                        },
                        Err(e) => {
                            warn!("Cross-component optimization execution failed: {}", e);
                        }
                    }
                }
            },
            Err(e) => {
                warn!("Cross-component opportunity analysis failed: {}", e);
            }
        }
        
        Ok(())
    }
    
    /// Run design rule checking
    pub(crate) fn run_design_rule_check(&self, analysis: &AnalysisResult) -> Result<()> {
        use crate::design_rule_checker::{DesignRuleChecker, IndustryStandard};
        
        info!("Running design rule check on netlist");
        
        // Use IPC-2221 as default standard
        let mut checker = DesignRuleChecker::new(IndustryStandard::IPC2221);
        let report = checker.run_checks(&self.netlist, analysis);
        // Surface violations in the synthesis/BOM output as a Markdown
        // section (they also go to the log). An engineer's report carries
        // the rule findings next to the sign-off — not buried in a log.
        if !report.violations.is_empty() {
            println!("\n## Design rule check\n");
            println!("| Rule | Severity | Finding | Suggested fix |");
            println!("|---|---|---|---|");
            for v in &report.violations {
                println!(
                    "| {} {} | {:?} | {} | {} |",
                    v.rule_id, v.rule_name, v.severity, v.description, v.fix_suggestion
                );
            }
            println!();
        }
        
        info!("DRC Results:");
        info!("  - Rules checked: {}", report.rules_checked);
        info!("  - Pass rate: {:.1}%", report.pass_rate);
        
        if report.critical_count > 0 {
            error!("  - {} CRITICAL violations found!", report.critical_count);
        }
        if report.error_count > 0 {
            error!("  - {} ERROR violations found!", report.error_count);
        }
        if report.warning_count > 0 {
            warn!("  - {} WARNING violations found", report.warning_count);
        }
        if report.info_count > 0 {
            info!("  - {} INFO messages", report.info_count);
        }
        
        if report.manufacturing_ready {
            info!("✅ Design is MANUFACTURING READY");
        } else {
            warn!("❌ Design is NOT manufacturing ready - fix critical and error violations");
        }
        
        Ok(())
    }
    
    /// Run ML-based component selection optimization
    pub(crate) fn run_ml_component_selection(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::ml_component_selection::{
            MLComponentSelector, ComponentRequirements, ComponentCategory,
            EnvironmentalConditions, DesignContext
        };
        
        info!("Running ML-based component selection optimization");
        
        let ml_selector = MLComponentSelector::new();
        let mut optimization_count = 0;
        let mut total_components = 0;
        
        // Process each component instance for potential optimization
        for (instance_id, instance) in &self.netlist.instances {
            total_components += 1;
            
            // Determine component category
            let category = self.determine_component_category(&instance.name);
            
            // Extract requirements from instance and analysis
            let requirements = self.extract_component_requirements(
                instance,
                &category,
                analysis
            )?;
            
            // Create design context
            let context = DesignContext {
                application_type: "General Purpose".to_string(),
                production_volume: 1000,
                target_cost: 100.0,
                regulatory_requirements: vec![],
            };
            
            // Run ML selection
            match ml_selector.select_component(&requirements, &context) {
                Ok(prediction) => {
                    if !prediction.recommended_components.is_empty() {
                        let best = &prediction.recommended_components[0];
                        
                        // Only suggest optimization if confidence is high
                        if best.score > 0.8 {
                            info!("  ML recommends {} for {} (score: {:.2})",
                                  best.part_number, instance.name, best.score);
                            
                            // Store recommendation for later application
                            // In production, would update the netlist or generate report
                            optimization_count += 1;
                            
                            // Log reasons
                            for reason in &best.reasons {
                                debug!("    - {}", reason);
                            }
                        }
                    }
                },
                Err(e) => {
                    debug!("ML selection failed for {}: {}", instance.name, e);
                }
            }
        }
        
        info!("ML component selection completed:");
        info!("  - {} components analyzed", total_components);
        info!("  - {} optimization opportunities found", optimization_count);
        
        if optimization_count > 0 {
            let optimization_rate = (optimization_count as f64 / total_components as f64) * 100.0;
            info!("  - Optimization potential: {:.1}%", optimization_rate);
        }
        
        Ok(())
    }
    
    /// Determine component category from instance name/type
    pub(crate) fn determine_component_category(&self, name: &str) -> crate::ml_component_selection::ComponentCategory {
        use crate::ml_component_selection::ComponentCategory;
        
        let name_lower = name.to_lowercase();
        
        if name_lower.contains("res") || name_lower.starts_with('r') {
            ComponentCategory::Resistor
        } else if name_lower.contains("cap") || name_lower.starts_with('c') {
            ComponentCategory::Capacitor
        } else if name_lower.contains("ind") || name_lower.starts_with('l') {
            ComponentCategory::Inductor
        } else if name_lower.contains("diode") || name_lower.starts_with('d') {
            ComponentCategory::Diode
        } else if name_lower.starts_with('q') || name_lower.contains("trans") {
            ComponentCategory::Transistor
        } else if name_lower.starts_with('u') {
            ComponentCategory::IC
        } else if name_lower.starts_with('j') || name_lower.contains("conn") {
            ComponentCategory::Connector
        } else if name_lower.starts_with('y') || name_lower.contains("xtal") {
            ComponentCategory::Crystal
        } else {
            ComponentCategory::IC // Default
        }
    }
    
    /// Extract component requirements from instance and analysis
    pub(crate) fn extract_component_requirements(
        &self,
        instance: &bhdl_netlist::Instance,
        category: &crate::ml_component_selection::ComponentCategory,
        analysis: &AnalysisResult,
    ) -> Result<crate::ml_component_selection::ComponentRequirements> {
        use crate::ml_component_selection::{ComponentRequirements, ComponentCategory, EnvironmentalConditions};
        use std::collections::HashMap;
        
        let mut electrical_specs = HashMap::new();
        
        // Extract electrical specifications from instance attributes
        for (key, value) in &instance.attributes {
            if let Ok(num_value) = value.parse::<f64>() {
                electrical_specs.insert(key.clone(), num_value);
            }
        }
        
        // Add default specs based on category
        match category {
            ComponentCategory::Resistor => {
                electrical_specs.entry("power_rating".to_string()).or_insert(0.25);
                electrical_specs.entry("tolerance".to_string()).or_insert(5.0);
            },
            ComponentCategory::Capacitor => {
                electrical_specs.entry("voltage_rating".to_string()).or_insert(16.0);
                electrical_specs.entry("tolerance".to_string()).or_insert(10.0);
            },
            _ => {}
        }
        
        Ok(ComponentRequirements {
            component_type: category.clone(),
            electrical_specs,
            environmental_conditions: EnvironmentalConditions {
                temperature_range: (-40.0, 85.0),
                humidity_range: (0.0, 95.0),
                vibration_level: "Standard".to_string(),
                altitude_max: 3000.0,
                chemical_exposure: vec![],
            },
            cost_target: None,
            size_constraints: None,
            reliability_requirements: None,
        })
    }
    
    /// Run thermal simulation and analysis
    pub(crate) fn run_thermal_simulation(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::thermal_simulation::{ThermalSimulator, AmbientConditions, BoardThermalProperties};
        use std::collections::HashMap;
        
        info!("Running thermal simulation on {} components", self.netlist.instances.len());
        
        let mut simulator = ThermalSimulator::new();
        
        // Set up simulation environment
        let ambient = AmbientConditions {
            temperature: 25.0,  // °C - typical room temperature
            humidity: 50.0,     // %RH
            pressure: 101.3,    // kPa - sea level
            altitude: 0.0,      // m
            enclosure_properties: None,
        };
        simulator.set_ambient_conditions(ambient);
        
        // Set up board properties (would come from PCB design in production)
        let board_props = BoardThermalProperties::default();
        simulator.set_board_properties(board_props);
        
        // Extract component list and load thermal models
        let component_names: Vec<String> = self.netlist.instances.keys()
            .map(|id| self.netlist.instances[id].name.clone())
            .collect();
        
        simulator.load_component_models(&component_names)?;
        
        // Estimate power dissipation for each component
        let power_map = self.estimate_component_power_dissipation(analysis)?;
        
        info!("Power dissipation estimates:");
        for (name, power) in &power_map {
            debug!("  {}: {:.3}W", name, power);
        }
        
        // Run thermal simulation
        match simulator.simulate(&power_map) {
            Ok(results) => {
                info!("Thermal simulation results:");
                info!("  - Components analyzed: {}", results.component_temperatures.len());
                info!("  - Thermal violations: {}", results.thermal_violations.len());
                info!("  - Hot spots identified: {}", results.hot_spots.len());
                
                // Report component temperatures
                for (name, temp) in &results.component_temperatures {
                    let status = if temp.thermal_margin > 10.0 {
                        "✓ OK"
                    } else if temp.thermal_margin > 0.0 {
                        "⚠ WARM"
                    } else {
                        "❌ HOT"
                    };
                    
                    info!("    {}: {:.1}°C junction, {:.1}°C margin {}",
                          name, temp.junction_temperature, temp.thermal_margin, status);
                }
                
                // Report violations
                if !results.thermal_violations.is_empty() {
                    warn!("Thermal violations detected:");
                    for violation in &results.thermal_violations {
                        warn!("  - {}: {:.1}°C exceeds {:.1}°C limit ({:?})",
                              violation.component_name,
                              violation.actual_value,
                              violation.limit_value,
                              violation.severity);
                    }
                }
                
                // Report hot spots
                if !results.hot_spots.is_empty() {
                    warn!("Hot spots detected:");
                    for hot_spot in &results.hot_spots {
                        warn!("  - {:.1}°C at ({:.1}, {:.1}) mm - {} - {:?}",
                              hot_spot.temperature,
                              hot_spot.position.0,
                              hot_spot.position.1,
                              hot_spot.root_cause,
                              hot_spot.severity);
                    }
                }
                
                // Show cooling recommendations
                if !results.cooling_recommendations.is_empty() {
                    info!("Cooling recommendations:");
                    for rec in &results.cooling_recommendations {
                        info!("  - {:?}: {} ({:.1}°C improvement, {:?} cost)",
                              rec.solution_type,
                              rec.description,
                              rec.estimated_improvement,
                              rec.implementation_cost);
                    }
                }
                
                // Show derating recommendations
                if !results.power_derating_recommendations.is_empty() {
                    info!("Power derating recommendations:");
                    for rec in &results.power_derating_recommendations {
                        info!("  - {}: Reduce to {:.2}W ({:.0}% derating)",
                              rec.component_name,
                              rec.recommended_power,
                              (1.0 - rec.derating_factor) * 100.0);
                    }
                }
                
                // Generate thermal report
                match simulator.export_thermal_report(&results) {
                    Ok(report) => {
                        debug!("Thermal analysis report:\n{}", report);
                    },
                    Err(e) => {
                        warn!("Failed to generate thermal report: {}", e);
                    }
                }
            },
            Err(e) => {
                error!("Thermal simulation failed: {}", e);
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    /// Estimate power dissipation for components
    pub(crate) fn estimate_component_power_dissipation(
        &self,
        analysis: &AnalysisResult,
    ) -> Result<HashMap<String, f64>> {
        let mut power_map = HashMap::new();
        
        // Extract power information from component instances
        for (_, instance) in &self.netlist.instances {
            let mut power = 0.0;
            
            // Check for explicit power attributes
            if let Some(power_str) = instance.attributes.get("power") {
                if let Ok(parsed_power) = power_str.parse::<f64>() {
                    power = parsed_power;
                }
            }
            
            // Estimate based on component type if no explicit power
            if power == 0.0 {
                power = self.estimate_power_by_type(&instance.name);
            }
            
            // Add power domain contributions
            power += self.estimate_power_from_domains(&instance.name, analysis);
            
            power_map.insert(instance.name.clone(), power);
        }
        
        Ok(power_map)
    }
    
    /// Estimate power by component type
    pub(crate) fn estimate_power_by_type(&self, component_name: &str) -> f64 {
        let name_lower = component_name.to_lowercase();
        
        if name_lower.starts_with('r') {
            0.001 // 1mW typical resistor
        } else if name_lower.starts_with('c') {
            0.0001 // 0.1mW typical capacitor
        } else if name_lower.starts_with('l') {
            0.0001 // 0.1mW typical inductor
        } else if name_lower.starts_with('d') {
            0.01 // 10mW typical diode
        } else if name_lower.starts_with('q') {
            0.1 // 100mW typical transistor
        } else if name_lower.starts_with('u') {
            0.25 // 250mW typical IC
        } else if name_lower.contains("led") {
            0.02 // 20mW typical LED
        } else {
            0.1 // 100mW default
        }
    }
    
    /// Estimate power contribution from power domains
    pub(crate) fn estimate_power_from_domains(&self, _component_name: &str, analysis: &AnalysisResult) -> f64 {
        // Check if component is connected to high-power domains
        // This would require connection analysis in production
        
        // For now, add small contribution based on power domain voltage
        let mut domain_power = 0.0;
        
        for (_domain_name, symbol) in analysis.global_scope.get_symbols() {
            if let Some(net_attr) = &symbol.net_attributes {
                if let Some(voltage) = net_attr.voltage() {
                    // Check for current in the net attributes (simplified)
                    // In production, would have proper current extraction method
                    if voltage > 3.0 {
                        domain_power += voltage * 0.01 * 0.01; // Very small estimated contribution
                    }
                }
            }
        }
        
        domain_power.min(1.0f64) // Cap at 1W
    }
    
    /// Run cost optimization with supplier data integration
    pub(crate) async fn run_cost_optimization(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::cost_optimization::{CostOptimizer, CostOptimizationConfig, SupplierClient, RateLimit, BackoffStrategy};
        use std::time::Duration;
        
        info!("Running cost optimization on {} components", self.netlist.instances.len());
        
        // Initialize cost optimizer if not already initialized
        if self.cost_optimizer.is_none() {
            let mut config = CostOptimizationConfig::default();
            config.enable_real_time_pricing = true;
            config.cache_pricing_hours = 4;
            config.parallel_supplier_queries = 5;
            config.include_shipping_costs = true;
            config.optimization_iterations = 30;
            
            let mut optimizer = CostOptimizer::with_config(config);
            
            // Add default suppliers (in production, these would come from configuration)
            let digikey = SupplierClient {
                supplier_name: "DigiKey".to_string(),
                api_endpoint: "https://api.digikey.com/v1/".to_string(),
                api_key: None, // Would be loaded from environment in production
                rate_limit: RateLimit {
                    requests_per_minute: 60,
                    burst_allowance: 10,
                    backoff_strategy: BackoffStrategy::Exponential(Duration::from_secs(1)),
                },
                availability_check: true,
                real_time_pricing: true,
                bulk_discount_support: true,
                lead_time_data: true,
            };
            
            let mouser = SupplierClient {
                supplier_name: "Mouser".to_string(),
                api_endpoint: "https://api.mouser.com/v1/".to_string(),
                api_key: None,
                rate_limit: RateLimit {
                    requests_per_minute: 100,
                    burst_allowance: 20,
                    backoff_strategy: BackoffStrategy::Linear(Duration::from_millis(500)),
                },
                availability_check: true,
                real_time_pricing: true,
                bulk_discount_support: true,
                lead_time_data: true,
            };
            
            let arrow = SupplierClient {
                supplier_name: "Arrow".to_string(),
                api_endpoint: "https://api.arrow.com/v1/".to_string(),
                api_key: None,
                rate_limit: RateLimit {
                    requests_per_minute: 30,
                    burst_allowance: 5,
                    backoff_strategy: BackoffStrategy::Fixed(Duration::from_secs(2)),
                },
                availability_check: true,
                real_time_pricing: false, // Batch pricing updates
                bulk_discount_support: true,
                lead_time_data: true,
            };
            
            // Add suppliers to optimizer
            optimizer.add_supplier(digikey).await.context("Failed to add DigiKey supplier")?;
            optimizer.add_supplier(mouser).await.context("Failed to add Mouser supplier")?;
            optimizer.add_supplier(arrow).await.context("Failed to add Arrow supplier")?;
            
            self.cost_optimizer = Some(optimizer);
            info!("Cost optimizer initialized with 3 suppliers");
        }
        
        // Run cost optimization
        if let Some(optimizer) = &mut self.cost_optimizer {
            match optimizer.optimize_component_costs(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Cost optimization results:");
                    info!("  - Original total cost: ${:.2}", results.original_cost.total);
                    info!("  - Optimized total cost: ${:.2}", results.optimized_cost.total);
                    info!("  - Total cost savings: ${:.2} ({:.1}%)", 
                          results.cost_savings, results.savings_percentage);
                    
                    // Report component-level savings
                    let significant_savings: Vec<_> = results.component_recommendations.iter()
                        .filter(|(_, rec)| rec.cost_change.abs() > 0.10) // Show savings > $0.10
                        .collect();
                    
                    if !significant_savings.is_empty() {
                        info!("  - Components with significant cost changes:");
                        for (instance_id, recommendation) in significant_savings.iter().take(10) {
                            let component_name = self.netlist.instances.get(**instance_id)
                                .map(|inst| inst.name.as_str())
                                .unwrap_or("unknown");
                            
                            let change_sign = if recommendation.cost_change >= 0.0 { "+" } else { "" };
                            info!("    • {}: {}{:.2} ({:.1}%) -> {}",
                                  component_name,
                                  change_sign,
                                  recommendation.cost_change,
                                  recommendation.cost_change_percentage,
                                  recommendation.recommended_component);
                        }
                        
                        if significant_savings.len() > 10 {
                            info!("    ... and {} more components with cost changes",
                                  significant_savings.len() - 10);
                        }
                    }
                    
                    // Report supplier consolidation
                    info!("  - Supplier optimization:");
                    info!("    • Suppliers: {} → {}",
                          results.supplier_consolidation.original_supplier_count,
                          results.supplier_consolidation.optimized_supplier_count);
                    info!("    • Consolidation savings: ${:.2}",
                          results.supplier_consolidation.consolidation_savings);
                    info!("    • Volume discount achieved: ${:.2}",
                          results.supplier_consolidation.volume_discount_achieved);
                    
                    // Report lifecycle risks
                    if !results.lifecycle_risks.is_empty() {
                        warn!("  - Lifecycle risks identified: {}", results.lifecycle_risks.len());
                        let high_risks = results.lifecycle_risks.iter()
                            .filter(|r| matches!(r.risk_level, crate::cost_optimization::RiskLevel::High | crate::cost_optimization::RiskLevel::Critical))
                            .count();
                        
                        if high_risks > 0 {
                            warn!("    • High/Critical risks: {} components", high_risks);
                        }
                    }
                    
                    // Show key findings and recommendations
                    if !results.optimization_summary.key_findings.is_empty() {
                        info!("  - Key findings:");
                        for finding in &results.optimization_summary.key_findings {
                            info!("    • {}", finding);
                        }
                    }
                    
                    if !results.optimization_summary.recommendations.is_empty() {
                        info!("  - Recommendations:");
                        for recommendation in &results.optimization_summary.recommendations {
                            info!("    • {}", recommendation);
                        }
                    }
                    
                    // Report optimization performance
                    info!("  - Optimization performance:");
                    info!("    • Iterations: {} (converged: {})",
                          results.optimization_summary.iterations_performed,
                          results.optimization_summary.convergence_achieved);
                    info!("    • Components analyzed: {}",
                          results.optimization_summary.components_analyzed);
                    info!("    • Alternatives evaluated: {}",
                          results.optimization_summary.alternatives_evaluated);
                    info!("    • Supplier queries: {}",
                          results.optimization_summary.supplier_queries_made);
                    info!("    • Time: {:.2}s",
                          results.optimization_summary.optimization_time_seconds);
                    
                    // Store results for later use (e.g., BOM generation)
                    // In production, this would be stored in the netlist or a separate structure
                    debug!("Cost optimization data available for BOM generation and procurement");
                },
                Err(e) => {
                    error!("Cost optimization failed: {}", e);
                    warn!("Continuing synthesis without cost optimization");
                    // Don't fail the entire synthesis due to cost optimization failure
                }
            }
        } else {
            error!("Cost optimizer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Cost optimizer initialization failed"));
        }
        
        Ok(())
    }
    
    /// Run EMI/EMC (Electromagnetic Interference/Electromagnetic Compatibility) analysis
    pub(crate) async fn run_emi_emc_analysis(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::emi_emc_analysis::{EMIEMCAnalyzer, EMIEMCConfig, EmissionStandard, ImmunityStandard};
        
        info!("Running EMI/EMC analysis on {} components", self.netlist.instances.len());
        
        // Initialize EMI/EMC analyzer if not already initialized
        if self.emi_emc_analyzer.is_none() {
            let mut config = EMIEMCConfig::default();
            
            // Configure analysis parameters
            config.target_standards = vec![
                EmissionStandard::CISPR22,   // Information Technology Equipment
                EmissionStandard::FCC15,     // US FCC Part 15
                EmissionStandard::IEC61000,  // General EMC Standard
            ];
            
            config.immunity_standards = vec![
                ImmunityStandard::IEC61000_4_2, // ESD Immunity
                ImmunityStandard::IEC61000_4_3, // Radiated RF Immunity
                ImmunityStandard::IEC61000_4_4, // Electrical Fast Transient
                ImmunityStandard::IEC61000_4_6, // Conducted RF
            ];
            
            config.frequency_range = (9_000.0, 1_000_000_000.0); // 9 kHz to 1 GHz
            config.analysis_resolution = 100_000.0; // 100 kHz resolution
            config.enable_prediction = true;
            config.enable_mitigation_suggestions = true;
            config.include_crosstalk_analysis = true;
            config.include_power_integrity = true;
            config.safety_margin = 6.0; // 6 dB safety margin
            
            let analyzer = EMIEMCAnalyzer::with_config(config);
            self.emi_emc_analyzer = Some(analyzer);
            
            info!("EMI/EMC analyzer initialized with {} emission standards and {} immunity standards",
                  3, 4);
        }
        
        // Run EMI/EMC analysis
        if let Some(analyzer) = &mut self.emi_emc_analyzer {
            match analyzer.analyze_emi_emc(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("EMI/EMC analysis results:");
                    
                    // Report emission compliance
                    info!("  - Emission Compliance:");
                    info!("    • Conducted emissions: {:?}", results.emission_compliance.conducted_emissions.overall_status);
                    info!("    • Radiated emissions: {:?}", results.emission_compliance.radiated_emissions.overall_status);
                    info!("    • Harmonic emissions: {:?}", results.emission_compliance.harmonic_emissions.overall_status);
                    info!("    • Emission hotspots found: {}", results.emission_compliance.emission_hotspots.len());
                    
                    // Report worst-case margins
                    let worst_conducted_margin = results.emission_compliance.conducted_emissions.worst_case_margin;
                    let worst_radiated_margin = results.emission_compliance.radiated_emissions.worst_case_margin;
                    
                    info!("    • Worst-case conducted margin: {:.1} dB", worst_conducted_margin);
                    info!("    • Worst-case radiated margin: {:.1} dB", worst_radiated_margin);
                    
                    if worst_conducted_margin < 0.0 || worst_radiated_margin < 0.0 {
                        warn!("    ⚠ Emission limits exceeded - mitigation required");
                    }
                    
                    // Report emission hotspots
                    if !results.emission_compliance.emission_hotspots.is_empty() {
                        info!("  - Emission Hotspots (top 5):");
                        for (i, hotspot) in results.emission_compliance.emission_hotspots.iter().take(5).enumerate() {
                            let component_name = self.netlist.instances.get(hotspot.component_id)
                                .map(|inst| inst.name.as_str())
                                .unwrap_or("unknown");
                            
                            info!("    {}. {} at {:.1} MHz: {:.1} dBμV ({:.1}% contribution)",
                                  i + 1,
                                  component_name,
                                  hotspot.emission_frequency / 1_000_000.0,
                                  hotspot.emission_level,
                                  hotspot.contribution_percentage);
                        }
                        
                        if results.emission_compliance.emission_hotspots.len() > 5 {
                            info!("    ... and {} more hotspots",
                                  results.emission_compliance.emission_hotspots.len() - 5);
                        }
                    }
                    
                    // Report immunity assessment
                    info!("  - Immunity Assessment:");
                    info!("    • Overall immunity level: {:.1} dBμV/m", 
                          results.immunity_assessment.susceptibility_analysis.overall_immunity_level);
                    info!("    • Protection effectiveness: {:.1}%", 
                          results.immunity_assessment.susceptibility_analysis.protection_effectiveness);
                    info!("    • Vulnerable components: {}", 
                          results.immunity_assessment.vulnerable_components.len());
                    
                    // Report vulnerable components
                    let high_risk_components = results.immunity_assessment.vulnerable_components.iter()
                        .filter(|v| matches!(v.risk_level, crate::emi_emc_analysis::RiskLevel::High | crate::emi_emc_analysis::RiskLevel::Critical))
                        .count();
                    
                    if high_risk_components > 0 {
                        warn!("    ⚠ {} components at high/critical risk for interference", high_risk_components);
                        
                        info!("    • High-risk components:");
                        for vulnerable in results.immunity_assessment.vulnerable_components.iter()
                            .filter(|v| matches!(v.risk_level, crate::emi_emc_analysis::RiskLevel::High | crate::emi_emc_analysis::RiskLevel::Critical))
                            .take(3) {
                            
                            info!("      - {} at {:.1} MHz (threshold: {:.1} dBμV/m)",
                                  vulnerable.component_name,
                                  vulnerable.susceptible_frequency / 1_000_000.0,
                                  vulnerable.immunity_threshold);
                        }
                    }
                    
                    // Report interference analysis
                    info!("  - Interference Analysis:");
                    info!("    • Internal interference sources: {}", 
                          results.interference_analysis.internal_interference.len());
                    info!("    • Crosstalk pairs analyzed: {}", 
                          results.interference_analysis.crosstalk_analysis.near_end_crosstalk.len());
                    info!("    • Power integrity issues: {}", 
                          results.interference_analysis.power_integrity_issues.len());
                    
                    let severe_interference = results.interference_analysis.internal_interference.iter()
                        .filter(|i| matches!(i.impact_severity, crate::emi_emc_analysis::ImpactSeverity::Severe | crate::emi_emc_analysis::ImpactSeverity::Critical))
                        .count();
                    
                    if severe_interference > 0 {
                        warn!("    ⚠ {} severe/critical interference issues detected", severe_interference);
                    }
                    
                    // Report worst-case crosstalk
                    if results.interference_analysis.crosstalk_analysis.worst_case_crosstalk > -30.0 {
                        warn!("    ⚠ Excessive crosstalk detected: {:.1} dB", 
                              results.interference_analysis.crosstalk_analysis.worst_case_crosstalk);
                    }
                    
                    // Report mitigation recommendations
                    info!("  - Mitigation Recommendations:");
                    info!("    • Total recommendations: {}", results.mitigation_recommendations.len());
                    
                    let critical_recs = results.mitigation_recommendations.iter()
                        .filter(|r| matches!(r.priority, crate::emi_emc_analysis::MitigationPriority::Critical))
                        .count();
                    
                    let high_recs = results.mitigation_recommendations.iter()
                        .filter(|r| matches!(r.priority, crate::emi_emc_analysis::MitigationPriority::High))
                        .count();
                    
                    info!("    • Critical priority: {}", critical_recs);
                    info!("    • High priority: {}", high_recs);
                    
                    if critical_recs > 0 || high_recs > 0 {
                        info!("    • Top recommendations:");
                        for (i, rec) in results.mitigation_recommendations.iter()
                            .filter(|r| matches!(r.priority, crate::emi_emc_analysis::MitigationPriority::Critical | crate::emi_emc_analysis::MitigationPriority::High))
                            .take(5)
                            .enumerate() {
                            
                            info!("      {}. [{}] {} - Effectiveness: {:.0}%",
                                  i + 1,
                                  match rec.priority {
                                      crate::emi_emc_analysis::MitigationPriority::Critical => "CRITICAL",
                                      crate::emi_emc_analysis::MitigationPriority::High => "HIGH",
                                      _ => "MEDIUM",
                                  },
                                  rec.description,
                                  rec.effectiveness * 100.0);
                        }
                    }
                    
                    // Report compliance summary
                    info!("  - Compliance Summary:");
                    info!("    • Overall compliance: {:?}", results.compliance_summary.overall_compliance);
                    info!("    • Standards passed: {}", results.compliance_summary.standards_passed.len());
                    info!("    • Standards failed: {}", results.compliance_summary.standards_failed.len());
                    info!("    • Estimated fix cost: {:?}", results.compliance_summary.estimated_fix_cost);
                    
                    // Report analysis performance
                    info!("  - Analysis Performance:");
                    info!("    • Components analyzed: {}", results.analysis_summary.components_analyzed);
                    info!("    • Nets analyzed: {}", results.analysis_summary.nets_analyzed);
                    info!("    • Frequencies analyzed: {}", results.analysis_summary.frequencies_analyzed);
                    info!("    • Analysis time: {:.2}s", results.analysis_summary.analysis_time_seconds);
                    info!("    • Prediction confidence: {:.1}%", results.analysis_summary.prediction_confidence * 100.0);
                    
                    // Summary status
                    match results.compliance_summary.overall_compliance {
                        crate::emi_emc_analysis::ComplianceLevel::Pass => {
                            info!("✅ EMI/EMC analysis: PASS - Circuit meets all EMC requirements");
                        },
                        crate::emi_emc_analysis::ComplianceLevel::PassWithMargin(_) => {
                            info!("✅ EMI/EMC analysis: PASS WITH MARGIN - Circuit exceeds EMC requirements");
                        },
                        crate::emi_emc_analysis::ComplianceLevel::Marginal(_) => {
                            warn!("⚠️ EMI/EMC analysis: MARGINAL - Circuit meets requirements but with limited margin");
                        },
                        crate::emi_emc_analysis::ComplianceLevel::Fail(_) => {
                            error!("❌ EMI/EMC analysis: FAIL - Circuit does not meet EMC requirements");
                        },
                    }
                    
                    // Store results for later use (e.g., compliance reporting)
                    debug!("EMI/EMC analysis data available for compliance documentation and design optimization");
                },
                Err(e) => {
                    error!("EMI/EMC analysis failed: {}", e);
                    warn!("Continuing synthesis without EMI/EMC analysis");
                    // Don't fail the entire synthesis due to EMI/EMC analysis failure
                }
            }
        } else {
            error!("EMI/EMC analyzer not initialized - this should not happen");
            return Err(anyhow::anyhow!("EMI/EMC analyzer initialization failed"));
        }
        
        Ok(())
    }

    /// Run reliability and lifecycle analysis
    pub(crate) async fn run_reliability_analysis(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::reliability_analysis::{ReliabilityAnalyzer, ReliabilityConfig};
        
        info!("Running reliability and lifecycle analysis on {} components", self.netlist.instances.len());
        
        // Initialize reliability analyzer if not already initialized
        if self.reliability_analyzer.is_none() {
            let mut config = ReliabilityConfig::default();
            
            // Configure analysis parameters
            config.analysis_period = 87600.0;  // 10 years in hours
            config.confidence_level = 0.95;    // 95% confidence
            config.enable_accelerated_testing = true;
            config.enable_physics_of_failure = true;
            config.enable_bayesian_analysis = false;
            config.enable_prognostics = true;
            config.temperature_cycling_enabled = true;
            config.burn_in_hours = 168.0;      // 1 week burn-in
            
            // Configure derating factors for conservative design
            config.derating_factors.voltage_derating = 0.8;  // 80% of maximum
            config.derating_factors.current_derating = 0.75; // 75% of maximum
            config.derating_factors.power_derating = 0.8;    // 80% of maximum
            config.derating_factors.temperature_derating = 10.0; // 10°C below maximum
            config.derating_factors.frequency_derating = 0.8; // 80% of maximum
            
            let analysis_period = config.analysis_period;
            let confidence_level = config.confidence_level;
            
            let analyzer = ReliabilityAnalyzer::with_config(config);
            self.reliability_analyzer = Some(analyzer);
            
            info!("Reliability analyzer initialized for {:.0}-year analysis with {}% confidence",
                  analysis_period / 8760.0, confidence_level * 100.0);
        }
        
        // Run reliability analysis
        if let Some(analyzer) = &mut self.reliability_analyzer {
            match analyzer.analyze_reliability(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Reliability analysis results:");
                    
                    // Report system-level reliability
                    info!("  - System Reliability:");
                    info!("    • Overall reliability: {:.4} ({:.2}%)", 
                          results.overall_system_reliability, 
                          results.overall_system_reliability * 100.0);
                    info!("    • Mean Time Between Failures: {:.0} hours ({:.1} years)", 
                          results.mean_time_between_failures, 
                          results.mean_time_between_failures / 8760.0);
                    info!("    • System failure rate: {:.2e} failures/hour", results.failure_rate);
                    
                    // Report component reliability summary
                    info!("  - Component Reliability:");
                    info!("    • Total components analyzed: {}", results.component_reliabilities.len());
                    
                    let low_reliability_count = results.component_reliabilities.values()
                        .filter(|c| c.reliability < 0.9)
                        .count();
                    
                    if low_reliability_count > 0 {
                        warn!("    ⚠ {} components with reliability < 90%", low_reliability_count);
                        
                        // Show worst components
                        let mut worst_components: Vec<_> = results.component_reliabilities.values().collect();
                        worst_components.sort_by(|a, b| a.reliability.partial_cmp(&b.reliability).unwrap());
                        
                        info!("    • Lowest reliability components:");
                        for (i, component) in worst_components.iter().take(3).enumerate() {
                            info!("      {}. {}: {:.3} ({:.1}% reliability)", 
                                  i + 1, 
                                  component.component_name, 
                                  component.reliability,
                                  component.reliability * 100.0);
                        }
                    }
                    
                    // Report critical components
                    info!("  - Critical Components:");
                    info!("    • Critical components identified: {}", results.critical_components.len());
                    
                    let single_point_failures = results.critical_components.iter()
                        .filter(|c| c.single_point_of_failure)
                        .count();
                    
                    if single_point_failures > 0 {
                        warn!("    ⚠ {} single points of failure detected", single_point_failures);
                    }
                    
                    // Show top critical components
                    if !results.critical_components.is_empty() {
                        info!("    • Top critical components:");
                        for (i, component) in results.critical_components.iter().take(5).enumerate() {
                            let impact_str = match component.failure_impact {
                                crate::reliability_analysis::ImpactSeverity::Catastrophic => "CATASTROPHIC",
                                crate::reliability_analysis::ImpactSeverity::Critical => "CRITICAL",
                                crate::reliability_analysis::ImpactSeverity::Marginal => "MARGINAL",
                                crate::reliability_analysis::ImpactSeverity::Negligible => "NEGLIGIBLE",
                            };
                            
                            info!("      {}. {} (Score: {:.2}, Impact: {}{})", 
                                  i + 1, 
                                  component.component_name,
                                  component.criticality_score,
                                  impact_str,
                                  if component.single_point_of_failure { ", SPOF" } else { "" });
                        }
                    }
                    
                    // Report failure predictions
                    info!("  - Failure Predictions:");
                    info!("    • Predicted failures in analysis period: {}", results.failure_predictions.len());
                    
                    let near_term_failures = results.failure_predictions.iter()
                        .filter(|f| f.predicted_failure_time < 8760.0) // Within 1 year
                        .count();
                    
                    if near_term_failures > 0 {
                        warn!("    ⚠ {} components predicted to fail within 1 year", near_term_failures);
                        
                        info!("    • Near-term failure predictions:");
                        for (i, prediction) in results.failure_predictions.iter()
                            .filter(|f| f.predicted_failure_time < 8760.0)
                            .take(3)
                            .enumerate() {
                            
                            info!("      {}. {} in {:.0} hours ({:.1} months, confidence: {:.0}%)",
                                  i + 1,
                                  prediction.component_name,
                                  prediction.predicted_failure_time,
                                  prediction.predicted_failure_time / (8760.0 / 12.0),
                                  prediction.prediction_confidence * 100.0);
                        }
                    }
                    
                    // Report lifecycle risks
                    info!("  - Lifecycle Risks:");
                    info!("    • Total lifecycle risks identified: {}", results.lifecycle_risks.len());
                    
                    let high_lifecycle_risks = results.lifecycle_risks.iter()
                        .filter(|r| matches!(r.risk_level, crate::reliability_analysis::RiskLevel::High | crate::reliability_analysis::RiskLevel::Critical))
                        .count();
                    
                    if high_lifecycle_risks > 0 {
                        warn!("    ⚠ {} high/critical lifecycle risks", high_lifecycle_risks);
                        
                        info!("    • High-priority lifecycle risks:");
                        for (i, risk) in results.lifecycle_risks.iter()
                            .filter(|r| matches!(r.risk_level, crate::reliability_analysis::RiskLevel::High | crate::reliability_analysis::RiskLevel::Critical))
                            .take(3)
                            .enumerate() {
                            
                            let risk_type_str = match risk.risk_type {
                                crate::reliability_analysis::LifecycleRiskType::ComponentObsolescence => "Obsolescence",
                                crate::reliability_analysis::LifecycleRiskType::SupplierDiscontinuation => "Supplier Risk",
                                crate::reliability_analysis::LifecycleRiskType::TechnologySupersession => "Technology",
                                crate::reliability_analysis::LifecycleRiskType::RegulatoryChange => "Regulatory",
                            };
                            
                            info!("      {}. {} - {} ({:.1} years): {}",
                                  i + 1,
                                  risk.component_name,
                                  risk_type_str,
                                  risk.time_horizon,
                                  risk.impact_description);
                        }
                    }
                    
                    // Report maintenance recommendations
                    info!("  - Maintenance Recommendations:");
                    info!("    • Total maintenance items: {}", results.maintenance_recommendations.len());
                    
                    let critical_maintenance = results.maintenance_recommendations.iter()
                        .filter(|m| matches!(m.priority, crate::reliability_analysis::MaintenancePriority::Critical))
                        .count();
                    
                    let high_maintenance = results.maintenance_recommendations.iter()
                        .filter(|m| matches!(m.priority, crate::reliability_analysis::MaintenancePriority::High))
                        .count();
                    
                    info!("    • Critical priority: {}", critical_maintenance);
                    info!("    • High priority: {}", high_maintenance);
                    
                    if critical_maintenance > 0 || high_maintenance > 0 {
                        info!("    • Priority maintenance items:");
                        for (i, maintenance) in results.maintenance_recommendations.iter()
                            .filter(|m| matches!(m.priority, crate::reliability_analysis::MaintenancePriority::Critical | crate::reliability_analysis::MaintenancePriority::High))
                            .take(5)
                            .enumerate() {
                            
                            let priority_str = match maintenance.priority {
                                crate::reliability_analysis::MaintenancePriority::Critical => "CRITICAL",
                                crate::reliability_analysis::MaintenancePriority::High => "HIGH",
                                _ => "MEDIUM",
                            };
                            
                            info!("      {}. [{}] {} - Every {:.0} hours ({:.1} months)",
                                  i + 1,
                                  priority_str,
                                  maintenance.component_name,
                                  maintenance.recommended_interval,
                                  maintenance.recommended_interval / (8760.0 / 12.0));
                        }
                    }
                    
                    // Report derating analysis
                    info!("  - Derating Analysis:");
                    info!("    • Overall derating compliance: {:.1}%", 
                          results.derating_analysis.overall_derating_compliance);
                    info!("    • Voltage derating: {:.1}% compliant", 
                          results.derating_analysis.voltage_derating_status.compliance_percentage);
                    info!("    • Current derating: {:.1}% compliant", 
                          results.derating_analysis.current_derating_status.compliance_percentage);
                    info!("    • Thermal derating: {:.1}% compliant", 
                          results.derating_analysis.thermal_derating_status.compliance_percentage);
                    
                    if results.derating_analysis.overall_derating_compliance < 90.0 {
                        warn!("    ⚠ Derating compliance below 90% - review component stress levels");
                    }
                    
                    // Report environmental impact
                    info!("  - Environmental Impact:");
                    info!("    • Temperature impact factor: {:.2}", results.environmental_impact.temperature_impact);
                    info!("    • Humidity impact factor: {:.2}", results.environmental_impact.humidity_impact);
                    info!("    • Vibration impact factor: {:.2}", results.environmental_impact.vibration_impact);
                    info!("    • Overall environmental factor: {:.2}", results.environmental_impact.overall_environmental_factor);
                    
                    if results.environmental_impact.overall_environmental_factor > 1.5 {
                        warn!("    ⚠ High environmental stress factor - consider environmental mitigation");
                    }
                    
                    // Report confidence intervals
                    info!("  - Statistical Confidence:");
                    info!("    • Reliability range: {:.3} - {:.3}", 
                          results.confidence_intervals.reliability_lower_bound,
                          results.confidence_intervals.reliability_upper_bound);
                    info!("    • MTBF range: {:.0} - {:.0} hours", 
                          results.confidence_intervals.mtbf_lower_bound,
                          results.confidence_intervals.mtbf_upper_bound);
                    
                    // Summary assessment
                    if results.overall_system_reliability > 0.95 {
                        info!("✅ Reliability analysis: EXCELLENT - System meets high reliability standards");
                    } else if results.overall_system_reliability > 0.90 {
                        info!("✅ Reliability analysis: GOOD - System meets reliability requirements");
                    } else if results.overall_system_reliability > 0.80 {
                        warn!("⚠️ Reliability analysis: MARGINAL - System reliability could be improved");
                    } else {
                        error!("❌ Reliability analysis: POOR - System reliability needs significant improvement");
                    }
                    
                    // Store results for later use (e.g., maintenance planning, lifecycle management)
                    debug!("Reliability analysis data available for maintenance planning and lifecycle management");
                },
                Err(e) => {
                    error!("Reliability analysis failed: {}", e);
                    warn!("Continuing synthesis without reliability analysis");
                    // Don't fail the entire synthesis due to reliability analysis failure
                }
            }
        } else {
            error!("Reliability analyzer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Reliability analyzer initialization failed"));
        }
        
        Ok(())
    }
    
    /// Run predictive analytics and machine learning integration
    pub(crate) async fn run_predictive_analysis(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::predictive_analytics::{PredictiveAnalyzer, PredictiveConfig};
        
        info!("Running predictive analytics and machine learning integration on {} components", self.netlist.instances.len());
        
        // Initialize predictive analyzer if not already initialized
        if self.predictive_analyzer.is_none() {
            let mut config = PredictiveConfig::default();
            
            // Enable key ML models
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ComponentSelection);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::PerformancePrediction);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::DesignCompletion);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::AnomalyDetection);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ParameterTuning);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ThermalPrediction);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::EMIPrediction);
            config.enabled_models.insert(crate::predictive_analytics::ModelType::ReliabilityPrediction);
            
            // Configure prediction parameters
            config.prediction_confidence_threshold = 0.8;
            config.max_prediction_time_ms = 30000; // 30 seconds max
            config.enable_explainable_ai = true;
            config.enable_uncertainty_quantification = true;
            config.enable_online_learning = false; // Off by default for stability
            
            let analyzer = PredictiveAnalyzer::with_config(config);
            self.predictive_analyzer = Some(analyzer);
            
            info!("Predictive analyzer initialized with ML algorithms: Random Forest, Gradient Boosting, SVM, Ensemble Methods");
        }
        
        // Run predictive analysis
        if let Some(analyzer) = &mut self.predictive_analyzer {
            match analyzer.analyze_predictive_insights(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Predictive analytics completed successfully:");
                    info!("  - Component recommendations: {}", results.component_recommendations.len());
                    info!("  - Performance predictions: {}", results.performance_predictions.len());
                    info!("  - Design completion suggestions: {}", results.design_completion_suggestions.len());
                    info!("  - Optimization opportunities: {}", results.optimization_opportunities.len());
                    info!("  - Risk assessments: {}", results.risk_assessments.len());
                    info!("  - Design pattern matches: {}", results.design_pattern_matches.len());
                    info!("  - Anomalies detected: {}", results.anomaly_detections.len());
                    
                    if results.component_recommendations.len() + results.optimization_opportunities.len() > 5 {
                        info!("✅ Predictive analysis: EXCELLENT - Multiple insights generated for design optimization");
                    } else if results.component_recommendations.len() + results.optimization_opportunities.len() > 2 {
                        info!("✅ Predictive analysis: GOOD - Several insights generated for improvement");
                    } else {
                        info!("✅ Predictive analysis: BASIC - Limited insights available with current data");
                    }
                    
                    // Store results for ML model training and future predictions
                    debug!("Predictive analytics data available for ML model improvement and future predictions");
                },
                Err(e) => {
                    error!("Predictive analytics failed: {}", e);
                    warn!("Continuing synthesis without predictive analytics");
                    // Don't fail the entire synthesis due to predictive analytics failure
                }
            }
        } else {
            error!("Predictive analyzer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Predictive analyzer initialization failed"));
        }
        
        Ok(())
    }
    
    /// Run manufacturing and assembly optimization (DFM/DFA)
    pub(crate) async fn run_manufacturing_optimization(&mut self, analysis: &AnalysisResult) -> Result<()> {
        use crate::manufacturing_optimization::{ManufacturingOptimizer, ManufacturingConfig};
        
        info!("Running manufacturing and assembly optimization on {} components", self.netlist.instances.len());
        
        // Initialize manufacturing optimizer if not already initialized
        if self.manufacturing_optimizer.is_none() {
            let mut config = ManufacturingConfig::default();
            
            // Configure based on production intent
            config.target_process = crate::manufacturing_optimization::ManufacturingProcess::SmallBatch;
            config.assembly_method = crate::manufacturing_optimization::AssemblyMethod::FullySMT;
            config.target_volume = crate::manufacturing_optimization::ProductionVolume::MediumVolume;
            config.quality_level = crate::manufacturing_optimization::QualityLevel::Standard;
            
            // Enable optimization features
            config.enable_panelization = true;
            config.enable_testpoint_generation = true;
            config.enable_component_consolidation = true;
            config.enable_placement_optimization = true;
            config.enable_routing_optimization = true;
            
            // Set targets
            config.target_yield = 0.95;
            config.max_board_layers = 4;
            
            let optimizer = ManufacturingOptimizer::with_config(config);
            self.manufacturing_optimizer = Some(optimizer);
            
            info!("Manufacturing optimizer initialized for small batch SMT production");
        }
        
        // Run manufacturing analysis
        if let Some(optimizer) = &mut self.manufacturing_optimizer {
            match optimizer.analyze_manufacturing(&self.netlist, analysis).await {
                Ok(results) => {
                    info!("Manufacturing optimization results:");
                    info!("  - DFM Score: {:.1}%", results.dfm_score * 100.0);
                    info!("  - DFA Score: {:.1}%", results.dfa_score * 100.0);
                    info!("  - Estimated Yield: {:.1}%", results.estimated_yield * 100.0);
                    info!("  - Unit Cost: ${:.2}", results.estimated_cost.total_unit_cost);
                    info!("  - Violations: {}", results.violations.len());
                    info!("  - Warnings: {}", results.warnings.len());
                    info!("  - Optimization Suggestions: {}", results.suggestions.len());
                    
                    // Report critical violations
                    let critical_violations = results.violations.iter()
                        .filter(|v| matches!(v.severity, crate::manufacturing_optimization::ViolationSeverity::Critical))
                        .count();
                    
                    if critical_violations > 0 {
                        error!("  ⚠ {} critical manufacturing violations found - design changes required", critical_violations);
                        for violation in results.violations.iter()
                            .filter(|v| matches!(v.severity, crate::manufacturing_optimization::ViolationSeverity::Critical))
                            .take(3) {
                            error!("    - {}: {}", violation.location, violation.description);
                        }
                    }
                    
                    // Report panelization if enabled
                    if let Some(panel) = &results.panelization {
                        info!("  - Panelization: {}x{} boards per panel, {:.1}% utilization",
                              panel.panel_layout.rows,
                              panel.panel_layout.columns,
                              panel.utilization * 100.0);
                    }
                    
                    // Report test coverage
                    info!("  - Test Coverage:");
                    info!("    • ICT: {:.1}%", results.test_coverage.in_circuit_test_coverage * 100.0);
                    info!("    • Boundary Scan: {:.1}%", results.test_coverage.boundary_scan_coverage * 100.0);
                    info!("    • Functional: {:.1}%", results.test_coverage.functional_test_coverage * 100.0);
                    
                    // Report assembly sequence
                    info!("  - Assembly Steps: {}", results.assembly_sequence.len());
                    let total_time: f64 = results.assembly_sequence.iter()
                        .map(|s| s.time_estimate)
                        .sum();
                    info!("    • Total assembly time: {:.1} minutes", total_time);
                    
                    // Report critical components
                    if !results.critical_components.is_empty() {
                        info!("  - Critical Components: {}", results.critical_components.len());
                        for component in results.critical_components.iter().take(3) {
                            info!("    • {}: {:?}", component.component_name, component.criticality_reason);
                        }
                    }
                    
                    // Overall assessment
                    if results.dfm_score > 0.9 && results.dfa_score > 0.9 {
                        info!("✅ Manufacturing optimization: EXCELLENT - Design ready for production");
                    } else if results.dfm_score > 0.8 && results.dfa_score > 0.8 {
                        info!("✅ Manufacturing optimization: GOOD - Minor improvements recommended");
                    } else if results.dfm_score > 0.7 && results.dfa_score > 0.7 {
                        warn!("⚠️ Manufacturing optimization: MODERATE - Several improvements needed");
                    } else {
                        error!("❌ Manufacturing optimization: POOR - Significant redesign recommended");
                    }
                    
                    // Store results for production planning
                    debug!("Manufacturing analysis data available for production planning and cost estimation");
                },
                Err(e) => {
                    error!("Manufacturing optimization failed: {}", e);
                    warn!("Continuing synthesis without manufacturing optimization");
                    // Don't fail the entire synthesis due to manufacturing optimization failure
                }
            }
        } else {
            error!("Manufacturing optimizer not initialized - this should not happen");
            return Err(anyhow::anyhow!("Manufacturing optimizer initialization failed"));
        }
        
        Ok(())
    }
}
