//! Hierarchical reference designator generation
//!
//! This module handles generating reference designators that reflect
//! the module hierarchy, e.g., U1.R1 for a resistor inside module U1.

use std::collections::HashMap;
use anyhow::Result;
use bhdl_analyzer::types::AnalysisResult;
use bhdl_netlist::{Netlist, InstanceId, ModuleId};
use log::{debug, info};

/// Hierarchical reference designator generator
pub struct HierarchicalRefDesGenerator {
    /// Counter for each component type at each hierarchy level
    /// Key: "path.type" (e.g., "U1.R", "U1.U2.C")
    counters: HashMap<String, usize>,
    
    /// Map from instance ID to its hierarchical path
    instance_paths: HashMap<InstanceId, String>,
}

impl HierarchicalRefDesGenerator {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
            instance_paths: HashMap::new(),
        }
    }
    
    /// Generate hierarchical reference designators for all instances
    pub fn generate_refdes(
        &mut self,
        netlist: &mut Netlist,
        analysis: &AnalysisResult,
    ) -> Result<()> {
        info!("Generating hierarchical reference designators");
        
        // First pass: Build hierarchy tree
        let hierarchy = self.build_hierarchy(netlist)?;
        
        // Second pass: Generate reference designators
        self.assign_refdes_recursive(netlist, &hierarchy, "", &mut HashMap::new())?;
        
        // Third pass: Update component instances from analysis
        self.update_component_refdes(netlist, analysis)?;
        
        Ok(())
    }
    
    /// Build a hierarchy tree from the netlist
    fn build_hierarchy(&self, netlist: &Netlist) -> Result<HierarchyNode> {
        let mut root = HierarchyNode {
            module_id: netlist.top_level_module,
            instances: Vec::new(),
            children: HashMap::new(),
        };
        
        // Group instances by their parent module
        let mut instance_to_parent: HashMap<InstanceId, Option<ModuleId>> = HashMap::new();
        
        // Find all instances and their parent modules
        for (inst_id, instance) in &netlist.instances {
            // For now, assume all instances are at top level
            // In a full implementation, we'd track parent modules during synthesis
            instance_to_parent.insert(inst_id, netlist.top_level_module);
            
            if let Some(parent_id) = netlist.top_level_module {
                if Some(parent_id) == root.module_id {
                    root.instances.push(inst_id);
                }
            }
        }
        
        Ok(root)
    }
    
    /// Recursively assign reference designators
    fn assign_refdes_recursive(
        &mut self,
        netlist: &mut Netlist,
        node: &HierarchyNode,
        parent_path: &str,
        parent_refdes: &mut HashMap<String, usize>,
    ) -> Result<()> {
        // Process instances at this level
        for &inst_id in &node.instances {
            if let Some(instance) = netlist.instances.get_mut(inst_id) {
                if let Some(module) = netlist.modules.get(instance.definition) {
                    let base_type = self.get_component_type(&module.name);
                    
                    // Generate reference designator
                    let counter_key = if parent_path.is_empty() {
                        base_type.clone()
                    } else {
                        format!("{}.{}", parent_path, base_type)
                    };
                    
                    let count = self.counters.entry(counter_key.clone()).or_insert(0);
                    *count += 1;
                    
                    let refdes = format!("{}{}", base_type, count);
                    let full_refdes = if parent_path.is_empty() {
                        refdes.clone()
                    } else {
                        format!("{}.{}", parent_path, refdes)
                    };
                    
                    debug!("Assigning refdes {} to instance {}", full_refdes, instance.name);
                    
                    // Store the hierarchical path
                    self.instance_paths.insert(inst_id, full_refdes.clone());
                    
                    // Update instance name to hierarchical refdes
                    instance.name = full_refdes;
                    
                    // Recursively process child modules
                    if matches!(module.kind, bhdl_netlist::ModuleKind::Module) {
                        let child_path = if parent_path.is_empty() {
                            refdes
                        } else {
                            format!("{}.{}", parent_path, refdes)
                        };
                        
                        // TODO: Process children of this instance
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get component type prefix from module/component name
    fn get_component_type(&self, name: &str) -> String {
        // Handle both full names and variants
        let base_name = if let Some(pos) = name.find('_') {
            &name[..pos]
        } else {
            name
        };
        
        match base_name.to_lowercase().as_str() {
            "res" | "resistor" => "R",
            "cap" | "capacitor" => "C",
            "ind" | "inductor" => "L",
            "diode" | "led" => "D",
            "transistor" | "bjt" | "fet" | "mosfet" => "Q",
            "ic" | "opamp" | "regulator" => "U",
            "connector" => "J",
            "switch" => "SW",
            "crystal" | "xtal" => "Y",
            "transformer" => "T",
            "fuse" => "F",
            "tvs" | "tvsdio
e" => "D",
            _ => "U", // Default to U for unknown types
        }.to_string()
    }
    
    /// Update component instances from analysis with hierarchical names
    fn update_component_refdes(
        &mut self,
        netlist: &mut Netlist,
        analysis: &AnalysisResult,
    ) -> Result<()> {
        // Get the inferred components from analysis
        let component_context = &analysis.component_inference;
        
        // Map old flat names to hierarchical names
        let mut name_mapping: HashMap<String, String> = HashMap::new();
        
        // Process each inferred component
        for inferred in &component_context.inferred_components {
            if let Some(instance_name) = &inferred.instance_name {
                // Determine which module this component belongs to
                // This is a simplified approach - in reality we'd track during inference
                let hierarchical_name = instance_name.clone();
                
                name_mapping.insert(instance_name.clone(), hierarchical_name);
            }
        }
        
        // Update instance names in netlist
        // Note: This is where we'd apply the mapping if we had the component instances
        
        Ok(())
    }
}

/// Hierarchy node for building the instance tree
struct HierarchyNode {
    module_id: Option<ModuleId>,
    instances: Vec<InstanceId>,
    children: HashMap<String, HierarchyNode>,
}

/// Apply hierarchical reference designators after synthesis
pub fn apply_hierarchical_refdes(
    netlist: &mut Netlist,
    analysis: &AnalysisResult,
) -> Result<()> {
    let mut generator = HierarchicalRefDesGenerator::new();
    generator.generate_refdes(netlist, analysis)
}