//! Progressive activation strategy for series nonlinear circuits

use crate::{Circuit, AnalysisResult, ComponentModel, GlacierSolver};
use std::collections::HashMap;
use anyhow::{Result, anyhow};

/// Progressive activation strategy that activates components one by one
pub struct ProgressiveActivation {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
}

impl ProgressiveActivation {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
        }
    }
    
    pub fn add_model(&mut self, component_name: String, model: ComponentModel) {
        self.models.insert(component_name, model);
    }
    
    /// Solve using progressive activation
    pub fn solve(&self, components: &[String]) -> Result<AnalysisResult> {
        println!("Progressive Activation: Solving {} components in series", components.len());
        println!("  Components to activate: {:?}", components);
        
        let mut _previous_solution = None;
        
        // Progressively activate components
        for i in 1..=components.len() {
            println!("  Step {}: Activating components 1-{}", i, i);
            
            // Create modified circuit with only first i components active
            let mut modified_circuit = self.circuit.clone();
            let mut modified_models = self.models.clone();
            
            // Deactivate components beyond i by replacing with high resistance
            for j in i..components.len() {
                let comp_name = &components[j];
                // Replace with 10MΩ resistor to simulate "off" state
                modified_models.insert(
                    comp_name.clone(),
                    ComponentModel::Resistor {
                        resistance: 10e6,
                        tolerance: 5.0,
                        limits: Default::default(),
                    }
                );
            }
            
            // Solve subproblem with GLACIER - should work for simplified circuits
            let mut glacier = GlacierSolver::new(modified_circuit);
            for (name, model) in &modified_models {
                glacier.add_model(name.clone(), model.clone());
            }
            
            println!("    Trying GLACIER on simplified circuit (step {}/{})", i, components.len());
            match glacier.analyze() {
                Ok(solutions) if !solutions.is_empty() => {
                    println!("    GLACIER found {} solutions for step {}", solutions.len(), i);
                    
                    // Look for the best solution
                    let mut best_solution = None;
                    let mut best_voltage_ratio = 0.0;
                    let mut is_partial = false;
                    
                    for (start_ramp, end_ramp, gradient, result) in solutions {
                        // Check if this is a partial solution (marked by equal start and end)
                        if (start_ramp - end_ramp).abs() < 0.01 && end_ramp < 0.5 {
                            // This is a partial solution from a marginal circuit
                            is_partial = true;
                            best_solution = Some(result);
                            best_voltage_ratio = end_ramp;
                            println!("      Found partial solution at {:.0}% of supply voltage (marginal circuit)", 
                                     end_ramp * 100.0);
                        } else if end_ramp >= 0.8 {
                            // This solution reaches at least 80% of supply voltage
                            best_solution = Some(result);
                            best_voltage_ratio = end_ramp;
                            is_partial = false;
                            println!("      Found solution covering {:.0}%-{:.0}% of supply voltage", 
                                     start_ramp * 100.0, end_ramp * 100.0);
                            break;
                        } else if end_ramp > best_voltage_ratio {
                            // Keep track of the highest voltage solution we find
                            best_solution = Some(result);
                            best_voltage_ratio = end_ramp;
                        }
                    }
                    
                    if let Some(result) = best_solution {
                        if best_voltage_ratio < 0.8 && !is_partial {
                            println!("      ⚠️  Best solution only reaches {:.0}% of supply voltage", 
                                     best_voltage_ratio * 100.0);
                            
                            // For the final step, if we can't reach full voltage, that's an error unless it's a partial solution
                            if i == components.len() && best_voltage_ratio < 0.5 {
                                return Err(anyhow!("Could not find stable solution above 50% supply voltage at final step"));
                            }
                        }
                        
                        _previous_solution = Some(result.clone());
                        
                        // If this is the last step, we're done
                        if i == components.len() {
                            if is_partial {
                                println!("      ✓ Accepting partial solution for marginal circuit");
                            }
                            return Ok(result);
                        }
                    } else {
                        return Err(anyhow!("No valid solutions found at step {}", i));
                    }
                }
                Ok(_) => {
                    // Empty solutions vector - GLACIER couldn't find any stable regions
                    println!("      GLACIER found no stable regions. Trying direct Newton-Raphson at 100%...");
                    
                    // As a last resort, try a direct solve at 100% with a reasonable initial guess
                    // For series LEDs, assume each LED drops about 2V
                    match glacier.analyze_with_guidance(1.0, Some(2.0)) {
                        Ok(result) => {
                            println!("      ✓ Direct solve succeeded!");
                            _previous_solution = Some(result.clone());
                            
                            if i == components.len() {
                                return Ok(result);
                            }
                        }
                        Err(e) => {
                            return Err(anyhow!("GLACIER found no stable operating regions at step {}. Circuit may have insufficient voltage for {} active components. Direct solve also failed: {}", i, i, e));
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow!("GLACIER solver error at step {}: {}", i, e));
                }
            }
        }
        
        Err(anyhow!("Progressive activation failed"))
    }
}