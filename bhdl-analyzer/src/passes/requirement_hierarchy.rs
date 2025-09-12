//! Hierarchical requirement analysis and traceability
//! 
//! This module analyzes hierarchical safety requirements, validates decomposition,
//! checks ASIL inheritance, and generates traceability reports.

use std::collections::{HashMap, HashSet, VecDeque};
use bhdl_ast::{AstNode, Board, SourceFile, HasSatisfies};
use crate::types::Diagnostic;

/// Requirement level in the V-model hierarchy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequirementLevel {
    SafetyGoal,
    Functional,
    Technical,
    Implementation,
}

/// ASIL levels per ISO 26262
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ASILLevel {
    QM,
    ASIL_A,
    ASIL_B,
    ASIL_C,
    ASIL_D,
}

/// A node in the requirement hierarchy
#[derive(Debug, Clone)]
pub struct RequirementNode {
    pub id: String,
    pub level: RequirementLevel,
    pub description: Option<String>,
    pub asil: Option<ASILLevel>,
    pub derived_from: Vec<String>,    // Parent requirements
    pub decomposes_to: Vec<String>,   // Child requirements
    pub implemented_by: ImplementationDetails,
    pub verification: Option<String>,
    pub coverage: f64,
}

/// How a requirement is implemented
#[derive(Debug, Clone)]
pub enum ImplementationDetails {
    NotImplemented,
    ByComponents(Vec<String>),
    ByRequirements(Vec<String>), // Composed of other requirements
    BySubsystem(String, Vec<String>),
}

/// A complete traceability path from safety goal to implementation
#[derive(Debug, Clone)]
pub struct TraceabilityPath {
    pub safety_goal: String,
    pub functional_reqs: Vec<String>,
    pub technical_reqs: Vec<String>,
    pub implementations: Vec<String>,
}

/// Coverage metrics at different hierarchy levels
#[derive(Debug, Clone, Default)]
pub struct HierarchicalCoverage {
    pub safety_goals: HashMap<String, f64>,
    pub functional_reqs: HashMap<String, f64>,
    pub technical_reqs: HashMap<String, f64>,
    pub overall: f64,
}

/// The complete requirement hierarchy and traceability graph
#[derive(Debug, Clone)]
pub struct RequirementHierarchy {
    /// All requirements indexed by ID
    pub requirements: HashMap<String, RequirementNode>,
    
    /// Parent-child relationships (parent -> children)
    pub decomposition_tree: HashMap<String, Vec<String>>,
    
    /// Child-parent relationships (child -> parents)
    pub composition_tree: HashMap<String, Vec<String>>,
    
    /// Complete upward traces (requirement -> all ancestors)
    pub upward_traces: HashMap<String, Vec<String>>,
    
    /// Complete downward traces (requirement -> all descendants)
    pub downward_traces: HashMap<String, Vec<String>>,
    
    /// All complete paths from safety goals to implementations
    pub traceability_paths: Vec<TraceabilityPath>,
    
    /// Coverage metrics
    pub coverage: HierarchicalCoverage,
    
    /// Validation diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

impl RequirementHierarchy {
    pub fn new() -> Self {
        Self {
            requirements: HashMap::new(),
            decomposition_tree: HashMap::new(),
            composition_tree: HashMap::new(),
            upward_traces: HashMap::new(),
            downward_traces: HashMap::new(),
            traceability_paths: Vec::new(),
            coverage: HierarchicalCoverage::default(),
            diagnostics: Vec::new(),
        }
    }
    
    /// Build the hierarchy from AST
    pub fn build_from_ast(&mut self, source_file: &SourceFile) {
        // First pass: collect all requirement definitions
        self.collect_requirement_definitions(source_file);
        
        // Second pass: collect satisfies declarations
        self.collect_satisfies_declarations(source_file);
        
        // Build relationship trees
        self.build_relationship_trees();
        
        // Calculate all traces
        self.calculate_all_traces();
        
        // Build complete traceability paths
        self.build_traceability_paths();
        
        // Calculate coverage
        self.calculate_coverage();
        
        // Validate the hierarchy
        self.validate();
    }
    
    /// Collect requirement definitions from the AST
    fn collect_requirement_definitions(&mut self, _source_file: &SourceFile) {
        // In a real implementation, we'd parse requirement definition blocks
        // For now, we'll create some example requirements based on satisfies blocks
        
        // This would normally parse:
        // safety_goal SG_BCM_001 { ... }
        // functional_requirement FSR_BCM_001 { ... }
        // technical_requirement TSR_BCM_001 { ... }
    }
    
    /// Collect satisfies declarations from boards and modules
    fn collect_satisfies_declarations(&mut self, source_file: &SourceFile) {
        for item in source_file.items() {
            if let Some(board) = Board::cast(item.syntax().clone()) {
                self.process_board_satisfies(&board);
            }
            // Also process modules when implemented
        }
    }
    
    /// Process satisfies block in a board
    fn process_board_satisfies(&mut self, board: &Board) {
        if let Some(satisfies_block) = board.satisfies_block() {
            for item in satisfies_block.items() {
                if let Some(req_id) = item.requirement_id() {
                    let req_id_str = req_id.text().to_string();
                    
                    // For now, use simple satisfies and enhance later
                    self.process_simple_satisfies(&req_id_str, &item);
                    
                    // Check if this looks like a hierarchical requirement
                    self.detect_and_process_hierarchy(&req_id_str, &item);
                }
            }
        }
    }
    
    /// Detect hierarchical requirements and process them
    fn detect_and_process_hierarchy(&mut self, req_id: &str, _item: &bhdl_ast::safety::SatisfiesItem) {
        // Infer hierarchy based on naming conventions
        let level = infer_requirement_level(req_id);
        
        // Update the requirement level
        if let Some(req) = self.requirements.get_mut(req_id) {
            req.level = level;
            
            // Try to infer parent relationships based on naming
            // E.g., TSR_PWR_MCU_001 might derive from FSR_PWR_MCU
            if req_id.starts_with("TSR_") {
                // Technical requirement - look for functional parent
                let parts: Vec<&str> = req_id.split('_').collect();
                if parts.len() >= 3 {
                    // Try FSR with same module prefix
                    let potential_parent = format!("FSR_{}_{}", parts[1], parts[2]);
                    req.derived_from.push(potential_parent);
                }
            } else if req_id.starts_with("FSR_") {
                // Functional requirement - look for safety goal parent
                let parts: Vec<&str> = req_id.split('_').collect();
                if parts.len() >= 2 {
                    // Try SG with same system prefix
                    let potential_parent = format!("SG_{}", parts[1]);
                    req.derived_from.push(potential_parent);
                }
            }
        }
    }
    
    /// Process a simple satisfies specification (fallback)
    fn process_simple_satisfies(&mut self, req_id: &str, item: &bhdl_ast::safety::SatisfiesItem) {
        let req = self.requirements.entry(req_id.to_string())
            .or_insert_with(|| RequirementNode {
                id: req_id.to_string(),
                level: infer_requirement_level(req_id),
                description: None,
                asil: None,
                derived_from: Vec::new(),
                decomposes_to: Vec::new(),
                implemented_by: ImplementationDetails::NotImplemented,
                verification: None,
                coverage: 100.0,
            });
        
        if let Some(spec) = item.satisfaction() {
            match spec {
                bhdl_ast::safety::SatisfiesSpec::Via(via) => {
                    // Use component_paths() to handle comma-separated list
                    req.implemented_by = ImplementationDetails::ByComponents(via.component_paths());
                }
                bhdl_ast::safety::SatisfiesSpec::Details(_) => {
                    // Handle detailed specifications
                }
            }
        }
    }
    
    /// Build decomposition and composition trees
    fn build_relationship_trees(&mut self) {
        // Collect the relationships first to avoid borrow issues
        let relationships: Vec<(String, Vec<String>)> = self.requirements
            .iter()
            .filter(|(_, req)| !req.decomposes_to.is_empty())
            .map(|(id, req)| (id.clone(), req.decomposes_to.clone()))
            .collect();
        
        for (req_id, children) in relationships {
            // Build decomposition tree (parent -> children)
            self.decomposition_tree.insert(req_id.clone(), children.clone());
            
            // Update composition tree and derived_from
            for child_id in children {
                self.composition_tree
                    .entry(child_id.clone())
                    .or_insert_with(Vec::new)
                    .push(req_id.clone());
                
                // Update child's derived_from
                if let Some(child_req) = self.requirements.get_mut(&child_id) {
                    if !child_req.derived_from.contains(&req_id) {
                        child_req.derived_from.push(req_id.clone());
                    }
                }
            }
        }
    }
    
    /// Calculate all upward and downward traces
    fn calculate_all_traces(&mut self) {
        for req_id in self.requirements.keys().cloned().collect::<Vec<_>>() {
            // Calculate upward traces (to ancestors)
            let upward = self.trace_upward(&req_id);
            self.upward_traces.insert(req_id.clone(), upward);
            
            // Calculate downward traces (to descendants)
            let downward = self.trace_downward(&req_id);
            self.downward_traces.insert(req_id.clone(), downward);
        }
    }
    
    /// Trace upward to all ancestor requirements
    fn trace_upward(&self, req_id: &str) -> Vec<String> {
        let mut ancestors = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        // Start with direct parents
        if let Some(parents) = self.composition_tree.get(req_id) {
            for parent in parents {
                if visited.insert(parent.clone()) {
                    queue.push_back(parent.clone());
                }
            }
        }
        
        // BFS to find all ancestors
        while let Some(current) = queue.pop_front() {
            ancestors.push(current.clone());
            
            if let Some(parents) = self.composition_tree.get(&current) {
                for parent in parents {
                    if visited.insert(parent.clone()) {
                        queue.push_back(parent.clone());
                    }
                }
            }
        }
        
        ancestors
    }
    
    /// Trace downward to all descendant requirements
    fn trace_downward(&self, req_id: &str) -> Vec<String> {
        let mut descendants = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        // Start with direct children
        if let Some(children) = self.decomposition_tree.get(req_id) {
            for child in children {
                if visited.insert(child.clone()) {
                    queue.push_back(child.clone());
                }
            }
        }
        
        // BFS to find all descendants
        while let Some(current) = queue.pop_front() {
            descendants.push(current.clone());
            
            if let Some(children) = self.decomposition_tree.get(&current) {
                for child in children {
                    if visited.insert(child.clone()) {
                        queue.push_back(child.clone());
                    }
                }
            }
        }
        
        descendants
    }
    
    /// Build complete traceability paths from safety goals to implementations
    fn build_traceability_paths(&mut self) {
        for (req_id, req) in &self.requirements {
            if req.level == RequirementLevel::SafetyGoal {
                let paths = self.build_paths_from_goal(req_id);
                self.traceability_paths.extend(paths);
            }
        }
    }
    
    /// Build all paths from a safety goal to implementations
    fn build_paths_from_goal(&self, goal_id: &str) -> Vec<TraceabilityPath> {
        let mut paths = Vec::new();
        
        // Find all implementation-level descendants
        let empty_vec = Vec::new();
        let descendants = self.downward_traces.get(goal_id).unwrap_or(&empty_vec);
        let implementations: Vec<_> = descendants.iter()
            .filter(|id| {
                self.requirements.get(*id)
                    .map(|r| matches!(r.implemented_by, ImplementationDetails::ByComponents(_)))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        
        if !implementations.is_empty() {
            // Build a path for this goal
            let functional_reqs = descendants.iter()
                .filter(|id| {
                    self.requirements.get(*id)
                        .map(|r| r.level == RequirementLevel::Functional)
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            
            let technical_reqs = descendants.iter()
                .filter(|id| {
                    self.requirements.get(*id)
                        .map(|r| r.level == RequirementLevel::Technical)
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            
            paths.push(TraceabilityPath {
                safety_goal: goal_id.to_string(),
                functional_reqs,
                technical_reqs,
                implementations,
            });
        }
        
        paths
    }
    
    /// Calculate coverage metrics at all levels
    fn calculate_coverage(&mut self) {
        let mut safety_goal_coverage = HashMap::new();
        let mut functional_coverage = HashMap::new();
        let mut technical_coverage = HashMap::new();
        
        for (req_id, req) in &self.requirements {
            let coverage = self.calculate_requirement_coverage(req_id);
            
            match req.level {
                RequirementLevel::SafetyGoal => {
                    safety_goal_coverage.insert(req_id.clone(), coverage);
                }
                RequirementLevel::Functional => {
                    functional_coverage.insert(req_id.clone(), coverage);
                }
                RequirementLevel::Technical => {
                    technical_coverage.insert(req_id.clone(), coverage);
                }
                _ => {}
            }
        }
        
        // Calculate overall coverage
        let total_reqs = self.requirements.len() as f64;
        let implemented_reqs = self.requirements.values()
            .filter(|r| !matches!(r.implemented_by, ImplementationDetails::NotImplemented))
            .count() as f64;
        
        self.coverage = HierarchicalCoverage {
            safety_goals: safety_goal_coverage,
            functional_reqs: functional_coverage,
            technical_reqs: technical_coverage,
            overall: if total_reqs > 0.0 { (implemented_reqs / total_reqs) * 100.0 } else { 100.0 },
        };
    }
    
    /// Calculate coverage for a specific requirement (including decomposition)
    fn calculate_requirement_coverage(&self, req_id: &str) -> f64 {
        if let Some(req) = self.requirements.get(req_id) {
            match &req.implemented_by {
                ImplementationDetails::ByComponents(_) => req.coverage,
                ImplementationDetails::ByRequirements(child_reqs) => {
                    // Coverage is average of child requirements
                    if child_reqs.is_empty() {
                        0.0
                    } else {
                        let sum: f64 = child_reqs.iter()
                            .map(|child| self.calculate_requirement_coverage(child))
                            .sum();
                        sum / child_reqs.len() as f64
                    }
                }
                ImplementationDetails::BySubsystem(_, _) => 100.0, // Assume subsystem handles it
                ImplementationDetails::NotImplemented => 0.0,
            }
        } else {
            0.0
        }
    }
    
    /// Validate the requirement hierarchy
    fn validate(&mut self) {
        self.validate_complete_decomposition();
        self.validate_asil_inheritance();
        self.validate_no_orphans();
        self.validate_no_cycles();
    }
    
    /// Check that all high-level requirements are decomposed
    fn validate_complete_decomposition(&mut self) {
        for (req_id, req) in &self.requirements {
            if matches!(req.level, RequirementLevel::SafetyGoal | RequirementLevel::Functional) {
                if req.decomposes_to.is_empty() && 
                   matches!(req.implemented_by, ImplementationDetails::NotImplemented) {
                    self.diagnostics.push(Diagnostic {
                        message: format!(
                            "{:?} requirement {} has no decomposition or implementation",
                            req.level, req_id
                        ),
                        range: rowan::TextRange::empty(rowan::TextSize::from(0)),
                    });
                }
            }
        }
    }
    
    /// Verify ASIL inheritance rules
    fn validate_asil_inheritance(&mut self) {
        for (req_id, req) in &self.requirements {
            if let Some(req_asil) = &req.asil {
                // Check all parent requirements
                for parent_id in &req.derived_from {
                    if let Some(parent) = self.requirements.get(parent_id) {
                        if let Some(parent_asil) = &parent.asil {
                            if req_asil < parent_asil {
                                self.diagnostics.push(Diagnostic {
                                    message: format!(
                                        "Requirement {} has ASIL {:?} which is lower than parent {} with ASIL {:?}",
                                        req_id, req_asil, parent_id, parent_asil
                                    ),
                                    range: rowan::TextRange::empty(rowan::TextSize::from(0)),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Check for orphaned requirements
    fn validate_no_orphans(&mut self) {
        for (req_id, req) in &self.requirements {
            // Skip safety goals (they're the root)
            if req.level == RequirementLevel::SafetyGoal {
                continue;
            }
            
            // Check if requirement has no parents and no implementation
            if req.derived_from.is_empty() && 
               self.composition_tree.get(req_id).map(|p| p.is_empty()).unwrap_or(true) &&
               matches!(req.implemented_by, ImplementationDetails::NotImplemented) {
                self.diagnostics.push(Diagnostic {
                    message: format!(
                        "Requirement {} is orphaned (no parent requirements and no implementation)",
                        req_id
                    ),
                    range: rowan::TextRange::empty(rowan::TextSize::from(0)),
                });
            }
        }
    }
    
    /// Check for circular dependencies
    fn validate_no_cycles(&mut self) {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        
        for req_id in self.requirements.keys() {
            if !visited.contains(req_id) {
                if self.has_cycle(req_id, &mut visited, &mut rec_stack) {
                    self.diagnostics.push(Diagnostic {
                        message: format!("Circular dependency detected involving requirement {}", req_id),
                        range: rowan::TextRange::empty(rowan::TextSize::from(0)),
                    });
                }
            }
        }
    }
    
    /// DFS to detect cycles
    fn has_cycle(&self, req_id: &str, visited: &mut HashSet<String>, rec_stack: &mut HashSet<String>) -> bool {
        visited.insert(req_id.to_string());
        rec_stack.insert(req_id.to_string());
        
        if let Some(children) = self.decomposition_tree.get(req_id) {
            for child in children {
                if !visited.contains(child) {
                    if self.has_cycle(child, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(child) {
                    return true;
                }
            }
        }
        
        rec_stack.remove(req_id);
        false
    }
    
    /// Generate a markdown traceability report
    pub fn generate_traceability_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str("# Requirement Traceability Report\n\n");
        
        // Executive summary
        report.push_str("## Executive Summary\n\n");
        report.push_str(&format!("- Total Requirements: {}\n", self.requirements.len()));
        report.push_str(&format!("- Overall Coverage: {:.1}%\n", self.coverage.overall));
        report.push_str(&format!("- Safety Goals: {}\n", self.coverage.safety_goals.len()));
        report.push_str(&format!("- Functional Requirements: {}\n", self.coverage.functional_reqs.len()));
        report.push_str(&format!("- Technical Requirements: {}\n", self.coverage.technical_reqs.len()));
        report.push_str("\n");
        
        // Coverage by level
        report.push_str("## Coverage by Level\n\n");
        report.push_str("| Level | Count | Average Coverage |\n");
        report.push_str("|-------|-------|------------------|\n");
        
        let sg_avg = if !self.coverage.safety_goals.is_empty() {
            self.coverage.safety_goals.values().sum::<f64>() / self.coverage.safety_goals.len() as f64
        } else { 0.0 };
        report.push_str(&format!("| Safety Goals | {} | {:.1}% |\n", 
            self.coverage.safety_goals.len(), sg_avg));
        
        let fr_avg = if !self.coverage.functional_reqs.is_empty() {
            self.coverage.functional_reqs.values().sum::<f64>() / self.coverage.functional_reqs.len() as f64
        } else { 0.0 };
        report.push_str(&format!("| Functional | {} | {:.1}% |\n", 
            self.coverage.functional_reqs.len(), fr_avg));
        
        let tr_avg = if !self.coverage.technical_reqs.is_empty() {
            self.coverage.technical_reqs.values().sum::<f64>() / self.coverage.technical_reqs.len() as f64
        } else { 0.0 };
        report.push_str(&format!("| Technical | {} | {:.1}% |\n", 
            self.coverage.technical_reqs.len(), tr_avg));
        report.push_str("\n");
        
        // Traceability paths
        report.push_str("## Traceability Paths\n\n");
        for path in &self.traceability_paths {
            report.push_str(&format!("### {}\n", path.safety_goal));
            report.push_str("```\n");
            report.push_str(&format!("{} (Safety Goal)\n", path.safety_goal));
            for fr in &path.functional_reqs {
                report.push_str(&format!("  └─> {} (Functional)\n", fr));
            }
            for tr in &path.technical_reqs {
                report.push_str(&format!("      └─> {} (Technical)\n", tr));
            }
            for impl_req in &path.implementations {
                if let Some(req) = self.requirements.get(impl_req) {
                    if let ImplementationDetails::ByComponents(comps) = &req.implemented_by {
                        for comp in comps {
                            report.push_str(&format!("          └─> {} (Component)\n", comp));
                        }
                    }
                }
            }
            report.push_str("```\n\n");
        }
        
        // Issues found
        if !self.diagnostics.is_empty() {
            report.push_str("## Issues Found\n\n");
            for diag in &self.diagnostics {
                report.push_str(&format!("- ⚠️ {}\n", diag.message));
            }
            report.push_str("\n");
        }
        
        // Requirement details
        report.push_str("## Requirement Details\n\n");
        for (req_id, req) in &self.requirements {
            report.push_str(&format!("### {}\n", req_id));
            report.push_str(&format!("- Level: {:?}\n", req.level));
            if let Some(asil) = &req.asil {
                report.push_str(&format!("- ASIL: {:?}\n", asil));
            }
            if !req.derived_from.is_empty() {
                report.push_str(&format!("- Derived From: {}\n", req.derived_from.join(", ")));
            }
            if !req.decomposes_to.is_empty() {
                report.push_str(&format!("- Decomposes To: {}\n", req.decomposes_to.join(", ")));
            }
            match &req.implemented_by {
                ImplementationDetails::ByComponents(comps) => {
                    report.push_str(&format!("- Implemented By: {}\n", comps.join(", ")));
                }
                ImplementationDetails::ByRequirements(reqs) => {
                    report.push_str(&format!("- Composed Of: {}\n", reqs.join(", ")));
                }
                ImplementationDetails::BySubsystem(sys, _) => {
                    report.push_str(&format!("- Allocated To: {}\n", sys));
                }
                ImplementationDetails::NotImplemented => {
                    report.push_str("- **NOT IMPLEMENTED**\n");
                }
            }
            report.push_str(&format!("- Coverage: {:.1}%\n", req.coverage));
            report.push_str("\n");
        }
        
        report
    }
}

/// Infer requirement level from naming convention
fn infer_requirement_level(req_id: &str) -> RequirementLevel {
    if req_id.starts_with("SG_") || req_id.starts_with("SYS_") {
        RequirementLevel::SafetyGoal
    } else if req_id.starts_with("FSR_") || req_id.starts_with("FUNC_") {
        RequirementLevel::Functional
    } else if req_id.starts_with("TSR_") || req_id.starts_with("TECH_") {
        RequirementLevel::Technical
    } else {
        RequirementLevel::Implementation
    }
}

/// Analyze hierarchical requirements for a source file
pub fn analyze_requirement_hierarchy(source_file: &SourceFile) -> RequirementHierarchy {
    let mut hierarchy = RequirementHierarchy::new();
    hierarchy.build_from_ast(source_file);
    hierarchy
}