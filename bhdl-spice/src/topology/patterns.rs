//! Circuit pattern definitions

#[derive(Debug, Clone)]
pub enum CircuitPattern {
    /// Series chain of nonlinear components (e.g., LED string)
    SeriesNonlinear {
        components: Vec<String>,
        count: usize,
    },
    
    /// Parallel array of similar components
    ParallelArray {
        components: Vec<String>,
        matched: bool,
    },
    
    /// Symmetric circuit structure
    Symmetric {
        groups: Vec<Vec<String>>,
    },
    
    /// Hierarchical/modular structure with weakly coupled blocks
    Hierarchical {
        blocks: Vec<CircuitBlock>,
    },
}

#[derive(Debug, Clone)]
pub struct CircuitBlock {
    pub components: Vec<String>,
    pub interface_nodes: Vec<String>,
    pub internal_nodes: Vec<String>,
}