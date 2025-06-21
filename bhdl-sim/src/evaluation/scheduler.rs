//! Dependency-based evaluation scheduler

use std::collections::{HashMap, HashSet, VecDeque};

/// Unique identifier for attributes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributeId(pub String);

/// Change in dependencies
#[derive(Debug, Clone)]
pub struct DependencyChange {
    pub attribute: AttributeId,
    pub old_deps: HashSet<AttributeId>,
    pub new_deps: HashSet<AttributeId>,
}

/// Schedules attribute evaluation based on dependencies
#[derive(Debug)]
pub struct EvaluationScheduler {
    /// Dependency graph: attribute -> attributes it depends on
    dependency_graph: HashMap<AttributeId, HashSet<AttributeId>>,
    
    /// Reverse dependency graph: attribute -> attributes that depend on it
    reverse_deps: HashMap<AttributeId, HashSet<AttributeId>>,
    
    /// Topologically sorted evaluation order
    evaluation_order: Vec<AttributeId>,
    
    /// Set of attributes that need re-evaluation
    dirty_set: HashSet<AttributeId>,
}

impl EvaluationScheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            dependency_graph: HashMap::new(),
            reverse_deps: HashMap::new(),
            evaluation_order: Vec::new(),
            dirty_set: HashSet::new(),
        }
    }
    
    /// Initialize with dependency information
    pub fn initialize(&mut self, dependencies: HashMap<String, HashSet<String>>) {
        // Clear existing data
        self.dependency_graph.clear();
        self.reverse_deps.clear();
        
        // Build dependency graphs
        for (attr, deps) in dependencies {
            let attr_id = AttributeId(attr);
            let dep_ids: HashSet<AttributeId> = deps.into_iter()
                .map(AttributeId)
                .collect();
            
            // Forward dependencies
            self.dependency_graph.insert(attr_id.clone(), dep_ids.clone());
            
            // Reverse dependencies
            for dep in &dep_ids {
                self.reverse_deps
                    .entry(dep.clone())
                    .or_default()
                    .insert(attr_id.clone());
            }
        }
        
        // Compute evaluation order
        self.compute_evaluation_order();
        
        // Initially all attributes are dirty
        self.dirty_set = self.dependency_graph.keys().cloned().collect();
    }
    
    /// Mark an attribute as needing re-evaluation
    pub fn mark_dirty(&mut self, attr: AttributeId) {
        // Mark this attribute as dirty
        self.dirty_set.insert(attr.clone());
        
        // Mark all attributes that depend on this one
        let dependents: Vec<AttributeId> = self.reverse_deps.get(&attr)
            .map(|deps| deps.iter().cloned().collect())
            .unwrap_or_default();
            
        for dep in dependents {
            self.mark_dirty(dep);
        }
    }
    
    /// Get the next batch of attributes to evaluate
    pub fn get_evaluation_batch(&mut self) -> Vec<AttributeId> {
        let mut batch = Vec::new();
        
        // Find attributes that are dirty and have no dirty dependencies
        for attr in &self.evaluation_order {
            if !self.dirty_set.contains(attr) {
                continue;
            }
            
            // Check if all dependencies have been evaluated
            let ready = if let Some(deps) = self.dependency_graph.get(attr) {
                deps.is_empty() || deps.iter().all(|dep| !self.dirty_set.contains(dep))
            } else {
                true
            };
            
            if ready {
                batch.push(attr.clone());
            }
        }
        
        // Remove from dirty set
        for attr in &batch {
            self.dirty_set.remove(attr);
        }
        
        batch
    }
    
    /// Update dependencies for an attribute
    pub fn update_dependencies(&mut self, changes: &[DependencyChange]) {
        for change in changes {
            // Update forward dependencies
            self.dependency_graph.insert(
                change.attribute.clone(),
                change.new_deps.clone()
            );
            
            // Update reverse dependencies
            // Remove old reverse deps
            for old_dep in &change.old_deps {
                if let Some(rev_deps) = self.reverse_deps.get_mut(old_dep) {
                    rev_deps.remove(&change.attribute);
                }
            }
            
            // Add new reverse deps
            for new_dep in &change.new_deps {
                self.reverse_deps
                    .entry(new_dep.clone())
                    .or_default()
                    .insert(change.attribute.clone());
            }
            
            // Mark as dirty since dependencies changed
            self.mark_dirty(change.attribute.clone());
        }
        
        // Recompute evaluation order
        self.compute_evaluation_order();
    }
    
    /// Check if there are attributes needing evaluation
    pub fn has_dirty_attributes(&self) -> bool {
        !self.dirty_set.is_empty()
    }
    
    /// Get all dirty attributes
    pub fn dirty_attributes(&self) -> &HashSet<AttributeId> {
        &self.dirty_set
    }
    
    /// Compute topological evaluation order
    fn compute_evaluation_order(&mut self) {
        self.evaluation_order.clear();
        
        // Kahn's algorithm for topological sort
        let mut in_degree: HashMap<AttributeId, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        
        // Calculate in-degrees - count how many dependencies each attribute has
        for (attr, deps) in &self.dependency_graph {
            in_degree.insert(attr.clone(), deps.len());
        }
        
        // Find nodes with no incoming edges
        for (attr, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(attr.clone());
            }
        }
        
        // Process nodes
        while let Some(attr) = queue.pop_front() {
            self.evaluation_order.push(attr.clone());
            
            // For each node that depends on this one, reduce its in-degree
            if let Some(dependents) = self.reverse_deps.get(&attr) {
                for dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }
        
        // Check for cycles
        if self.evaluation_order.len() < self.dependency_graph.len() {
            tracing::warn!("Circular dependencies detected in attribute graph");
        }
    }
}

impl Default for EvaluationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_dependencies() {
        let mut scheduler = EvaluationScheduler::new();
        
        let mut deps = HashMap::new();
        deps.insert("c".to_string(), ["a", "b"].iter().map(|s| s.to_string()).collect());
        deps.insert("b".to_string(), ["a"].iter().map(|s| s.to_string()).collect());
        deps.insert("a".to_string(), HashSet::new());
        
        scheduler.initialize(deps);
        
        // Should evaluate in order: a, b, c
        let batch1 = scheduler.get_evaluation_batch();
        assert_eq!(batch1.len(), 1);
        assert_eq!(batch1[0].0, "a");
        
        let batch2 = scheduler.get_evaluation_batch();
        assert_eq!(batch2.len(), 1);
        assert_eq!(batch2[0].0, "b");
        
        let batch3 = scheduler.get_evaluation_batch();
        assert_eq!(batch3.len(), 1);
        assert_eq!(batch3[0].0, "c");
    }
    
    #[test]
    fn test_mark_dirty() {
        let mut scheduler = EvaluationScheduler::new();
        
        let mut deps = HashMap::new();
        deps.insert("c".to_string(), ["b"].iter().map(|s| s.to_string()).collect());
        deps.insert("b".to_string(), ["a"].iter().map(|s| s.to_string()).collect());
        deps.insert("a".to_string(), HashSet::new());
        
        scheduler.initialize(deps);
        
        // Clear dirty set
        scheduler.dirty_set.clear();
        
        // Mark 'a' as dirty - should cascade to b and c
        scheduler.mark_dirty(AttributeId("a".to_string()));
        
        assert!(scheduler.dirty_set.contains(&AttributeId("a".to_string())));
        assert!(scheduler.dirty_set.contains(&AttributeId("b".to_string())));
        assert!(scheduler.dirty_set.contains(&AttributeId("c".to_string())));
    }
}