/// Net attributes for power domains and other net properties
use std::collections::HashMap;

/// Attributes that can be attached to nets
#[derive(Debug, Clone, PartialEq)]
pub enum NetAttribute {
    /// Power domain attributes
    PowerDomain {
        voltage: f64,
        tolerance: f64,
        max_current: f64,
        controllable: bool,
        enable_signal: Option<String>,
        startup_delay_ms: f64,
        sequence_priority: u32,
        dependencies: Vec<String>,
        /// Ordered stage names from `|> stage1 |> stage2` chain
        stages: Vec<String>,
    },
    /// Ground domain (0V reference)
    GroundDomain,
    /// Generic key-value attributes
    Generic(HashMap<String, String>),
}

impl NetAttribute {
    /// Create a new power domain attribute
    pub fn new_power_domain(voltage: f64, max_current: f64) -> Self {
        NetAttribute::PowerDomain {
            voltage,
            tolerance: 5.0, // 5% default
            max_current,
            controllable: true,
            enable_signal: None,
            startup_delay_ms: 1.0,
            sequence_priority: 100,
            dependencies: Vec::new(),
            stages: Vec::new(),
        }
    }
    
    /// Create a ground domain attribute
    pub fn new_ground_domain() -> Self {
        NetAttribute::GroundDomain
    }
    
    /// Check if this is a power-related attribute
    pub fn is_power_attribute(&self) -> bool {
        matches!(self, NetAttribute::PowerDomain { .. } | NetAttribute::GroundDomain)
    }
    
    /// Get voltage if this is a power domain
    pub fn voltage(&self) -> Option<f64> {
        match self {
            NetAttribute::PowerDomain { voltage, .. } => Some(*voltage),
            NetAttribute::GroundDomain => Some(0.0),
            _ => None,
        }
    }
    
    /// Set stage chain on a power domain
    pub fn set_stages(&mut self, new_stages: Vec<String>) {
        if let NetAttribute::PowerDomain { stages, .. } = self {
            *stages = new_stages;
        }
    }

    /// Get stage chain from a power domain (empty slice if not a power domain or no stages)
    pub fn stages(&self) -> &[String] {
        match self {
            NetAttribute::PowerDomain { stages, .. } => stages,
            _ => &[],
        }
    }

    /// Get max current if this is a power domain
    pub fn max_current(&self) -> Option<f64> {
        match self {
            NetAttribute::PowerDomain { max_current, .. } => Some(*max_current),
            NetAttribute::GroundDomain => Some(f64::INFINITY), // Ground can sink any current
            _ => None,
        }
    }
}