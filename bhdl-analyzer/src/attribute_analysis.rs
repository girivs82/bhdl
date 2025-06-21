// Attribute dependency analysis for behavioral modeling
// Tracks dependencies between attributes and detects circular references

use std::collections::{HashMap, HashSet, VecDeque};
use bhdl_ast::attributes::{AttributeDecl, AttributeType, AttributeDependency};
use bhdl_ast::{SyntaxNode, BhdlLanguage, SyntaxKind};
use rowan::ast::AstNode;

/// Result of attribute dependency analysis
#[derive(Debug, Clone)]
pub struct AttributeAnalysisResult {
    /// All attributes found in the scope
    pub attributes: HashMap<String, AttributeInfo>,
    /// Dependency graph: attribute -> set of attributes it depends on
    pub dependencies: HashMap<String, HashSet<String>>,
    /// Attributes in dependency order (can be evaluated in this order)
    pub evaluation_order: Vec<String>,
    /// Circular dependency chains if any
    pub circular_dependencies: Vec<Vec<String>>,
    /// Mutable attributes (modified in when blocks)
    pub mutable_attributes: HashSet<String>,
}

/// Information about a single attribute
#[derive(Debug, Clone)]
pub struct AttributeInfo {
    pub name: String,
    pub attribute_type: AttributeType,
    pub dependencies: AttributeDependency,
    pub is_mutable: bool,
    pub decl: AttributeDecl,
}

/// Information about a when block
#[derive(Debug, Clone)]
pub struct WhenBlockInfo {
    /// Condition expression
    pub condition: String,
    /// Assignments in the when block
    pub assignments: HashMap<String, String>,
}

/// Analyzes attribute dependencies in a syntax tree
pub struct AttributeAnalyzer {
    attributes: HashMap<String, AttributeInfo>,
    dependencies: HashMap<String, HashSet<String>>,
    mutable_attributes: HashSet<String>,
}

impl AttributeAnalyzer {
    pub fn new() -> Self {
        Self {
            attributes: HashMap::new(),
            dependencies: HashMap::new(),
            mutable_attributes: HashSet::new(),
        }
    }
    
    /// Analyze all attributes in the given syntax tree
    pub fn analyze(&mut self, root: &SyntaxNode<BhdlLanguage>) -> AttributeAnalysisResult {
        // First pass: collect all attribute declarations
        self.collect_attributes(root);
        
        // Second pass: analyze when blocks for mutable attributes
        self.analyze_when_blocks(root);
        
        // Build dependency graph
        self.build_dependency_graph();
        
        // Detect circular dependencies
        let circular_dependencies = self.detect_circular_dependencies();
        
        // Compute evaluation order (topological sort)
        let evaluation_order = if circular_dependencies.is_empty() {
            self.topological_sort()
        } else {
            Vec::new() // Can't determine order with circular deps
        };
        
        AttributeAnalysisResult {
            attributes: self.attributes.clone(),
            dependencies: self.dependencies.clone(),
            evaluation_order,
            circular_dependencies,
            mutable_attributes: self.mutable_attributes.clone(),
        }
    }
    
    /// Collect all attribute declarations
    fn collect_attributes(&mut self, root: &SyntaxNode<BhdlLanguage>) {
        for node in root.descendants() {
            if let Some(attr_decl) = AttributeDecl::cast(node) {
                if let Some(name_token) = attr_decl.name() {
                    let name = name_token.text().to_string();
                    
                    // Determine attribute type
                    let attribute_type = if attr_decl.is_expression_attribute() {
                        let deps = attr_decl.referenced_attributes();
                        AttributeType::Expression(deps.clone())
                    } else {
                        AttributeType::Static(attr_decl.value()
                            .map(|v| v.syntax().text().to_string())
                            .unwrap_or_default())
                    };
                    
                    // Create dependency info
                    let dependencies = AttributeDependency {
                        attribute: name.clone(),
                        depends_on: attr_decl.referenced_attributes(),
                        pin_refs: attr_decl.referenced_pins(),
                        is_mutable: false, // Will be updated in second pass
                    };
                    
                    let info = AttributeInfo {
                        name: name.clone(),
                        attribute_type,
                        dependencies,
                        is_mutable: false,
                        decl: attr_decl,
                    };
                    
                    self.attributes.insert(name, info);
                }
            }
        }
    }
    
    /// Analyze when blocks to find mutable attributes
    fn analyze_when_blocks(&mut self, root: &SyntaxNode<BhdlLanguage>) {
        use bhdl_ast::behavioral::{find_when_blocks, find_mutable_attributes};
        
        // Find all attributes modified in when blocks
        let mutable_attrs = find_mutable_attributes(root);
        
        // Mark these attributes as mutable
        for attr_name in mutable_attrs {
            self.mutable_attributes.insert(attr_name.clone());
            
            // Update the attribute info if it exists
            if let Some(info) = self.attributes.get_mut(&attr_name) {
                info.is_mutable = true;
                info.dependencies.is_mutable = true;
                info.attribute_type = AttributeType::Mutable;
            }
        }
    }
    
    /// Build the dependency graph from collected attributes
    fn build_dependency_graph(&mut self) {
        for (name, info) in &self.attributes {
            let deps: HashSet<String> = info.dependencies.depends_on
                .iter()
                .filter(|dep| self.attributes.contains_key(*dep))
                .cloned()
                .collect();
            
            self.dependencies.insert(name.clone(), deps);
        }
    }
    
    /// Detect circular dependencies using DFS
    fn detect_circular_dependencies(&self) -> Vec<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycles = Vec::new();
        
        for attr in self.attributes.keys() {
            if !visited.contains(attr) {
                let mut path = Vec::new();
                if self.has_cycle_dfs(attr, &mut visited, &mut rec_stack, &mut path) {
                    cycles.push(path);
                }
            }
        }
        
        cycles
    }
    
    /// DFS helper for cycle detection
    fn has_cycle_dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());
        
        if let Some(deps) = self.dependencies.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if self.has_cycle_dfs(dep, visited, rec_stack, path) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    // Found a cycle - trim path to show only the cycle
                    if let Some(pos) = path.iter().position(|x| x == dep) {
                        *path = path[pos..].to_vec();
                    }
                    return true;
                }
            }
        }
        
        rec_stack.remove(node);
        path.pop();
        false
    }
    
    /// Topological sort using Kahn's algorithm
    fn topological_sort(&self) -> Vec<String> {
        // Create reverse dependency graph (who depends on this attribute)
        let mut reverse_deps: HashMap<String, HashSet<String>> = HashMap::new();
        for attr in self.attributes.keys() {
            reverse_deps.insert(attr.clone(), HashSet::new());
        }
        
        // Build reverse dependencies
        for (attr, deps) in &self.dependencies {
            for dep in deps {
                if let Some(rev_set) = reverse_deps.get_mut(dep) {
                    rev_set.insert(attr.clone());
                }
            }
        }
        
        // Calculate in-degrees (how many attributes this one depends on)
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for (attr, deps) in &self.dependencies {
            in_degree.insert(attr.clone(), deps.len());
        }
        
        // Queue for nodes with no dependencies
        let mut queue: VecDeque<String> = VecDeque::new();
        for (attr, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(attr.clone());
            }
        }
        
        let mut result = Vec::new();
        
        while let Some(attr) = queue.pop_front() {
            result.push(attr.clone());
            
            // For each attribute that depends on this one
            if let Some(dependents) = reverse_deps.get(&attr) {
                for dependent in dependents {
                    if let Some(count) = in_degree.get_mut(dependent) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }
        
        // If we didn't process all nodes, there's a cycle
        if result.len() != self.attributes.len() {
            Vec::new()
        } else {
            result
        }
    }
}

/// Check if an attribute reference would create a circular dependency
pub fn would_create_cycle(
    dependencies: &HashMap<String, HashSet<String>>,
    from_attr: &str,
    to_attr: &str,
) -> bool {
    // Check if adding edge from_attr -> to_attr would create a cycle
    // This means checking if there's already a path from to_attr to from_attr
    
    let mut visited = HashSet::new();
    let mut stack = vec![to_attr];
    
    while let Some(current) = stack.pop() {
        if current == from_attr {
            return true; // Found a path back
        }
        
        if visited.insert(current.to_string()) {
            if let Some(deps) = dependencies.get(current) {
                for dep in deps {
                    stack.push(dep);
                }
            }
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_circular_dependency_detection() {
        let mut analyzer = AttributeAnalyzer::new();
        
        // Create a simple circular dependency: a -> b -> c -> a
        analyzer.dependencies.insert("a".to_string(), ["b"].iter().cloned().collect());
        analyzer.dependencies.insert("b".to_string(), ["c"].iter().cloned().collect());
        analyzer.dependencies.insert("c".to_string(), ["a"].iter().cloned().collect());
        
        // Add attributes
        for name in &["a", "b", "c"] {
            analyzer.attributes.insert(
                name.to_string(),
                AttributeInfo {
                    name: name.to_string(),
                    attribute_type: AttributeType::Expression(vec![]),
                    dependencies: AttributeDependency {
                        attribute: name.to_string(),
                        depends_on: vec![],
                        pin_refs: vec![],
                        is_mutable: false,
                    },
                    is_mutable: false,
                    decl: AttributeDecl::cast(SyntaxNode::new_root(rowan::GreenNode::new(
                        SyntaxKind::ATTRIBUTE_DECL.into(),
                        vec![]
                    ))).unwrap(), // Dummy node for test
                },
            );
        }
        
        let cycles = analyzer.detect_circular_dependencies();
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].len() >= 3); // Should contain at least a, b, c
    }
    
    #[test]
    fn test_topological_sort() {
        let mut analyzer = AttributeAnalyzer::new();
        
        // Create dependencies: a -> b, b -> c, d -> c
        analyzer.dependencies.insert("a".to_string(), ["b"].iter().cloned().collect());
        analyzer.dependencies.insert("b".to_string(), ["c"].iter().cloned().collect());
        analyzer.dependencies.insert("d".to_string(), ["c"].iter().cloned().collect());
        analyzer.dependencies.insert("c".to_string(), HashSet::new());
        
        // Add attributes
        for name in &["a", "b", "c", "d"] {
            analyzer.attributes.insert(
                name.to_string(),
                AttributeInfo {
                    name: name.to_string(),
                    attribute_type: AttributeType::Expression(vec![]),
                    dependencies: AttributeDependency {
                        attribute: name.to_string(),
                        depends_on: vec![],
                        pin_refs: vec![],
                        is_mutable: false,
                    },
                    is_mutable: false,
                    decl: AttributeDecl::cast(SyntaxNode::new_root(rowan::GreenNode::new(
                        SyntaxKind::ATTRIBUTE_DECL.into(),
                        vec![]
                    ))).unwrap(), // Dummy node for test
                },
            );
        }
        
        let order = analyzer.topological_sort();
        
        // c should come before b and d
        // b should come before a
        let c_pos = order.iter().position(|x| x == "c").unwrap();
        let b_pos = order.iter().position(|x| x == "b").unwrap();
        let a_pos = order.iter().position(|x| x == "a").unwrap();
        let d_pos = order.iter().position(|x| x == "d").unwrap();
        
        assert!(c_pos < b_pos);
        assert!(c_pos < d_pos);
        assert!(b_pos < a_pos);
    }
}