//! Drive strength resolution for digital signals

use crate::circuit::state::{PinValue, DriveStrength, LogicLevel};
use std::collections::HashMap;

/// Resolves drive strength conflicts
pub struct DriveStrengthResolver {
    /// Drive strength priorities
    strength_priority: HashMap<DriveStrength, u8>,
    
    /// Conflict resolution strategy
    strategy: ResolutionStrategy,
    
    /// Metrics
    metrics: DriveMetrics,
}

/// Drive conflict information
#[derive(Debug, Clone)]
pub struct DriveConflict {
    pub net_name: String,
    pub drivers: Vec<DriveInfo>,
    pub resolved_value: PinValue,
    pub conflict_type: DriveConflictType,
}

/// Information about a driver
#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub pin_name: String,
    pub value: PinValue,
}

/// Type of drive conflict
#[derive(Debug, Clone, PartialEq)]
pub enum DriveConflictType {
    /// Multiple strong drivers
    StrongContention,
    /// Weak vs strong conflict
    StrengthMismatch,
    /// Bus contention
    BusContention,
    /// No conflict
    NoConflict,
}

/// Resolution strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolutionStrategy {
    /// Strongest wins
    StrongestWins,
    /// Wired-AND (all must be high)
    WiredAnd,
    /// Wired-OR (any high wins)
    WiredOr,
    /// Average voltage
    VoltageAverage,
}

/// Drive resolution metrics
#[derive(Debug, Default)]
struct DriveMetrics {
    resolutions_performed: usize,
    conflicts_detected: usize,
    resolution_time_ms: f64,
}

impl DriveStrengthResolver {
    /// Create a new drive strength resolver
    pub fn new(strategy: ResolutionStrategy) -> Self {
        let mut resolver = Self {
            strength_priority: HashMap::new(),
            strategy,
            metrics: DriveMetrics::default(),
        };
        
        // Set up default priorities
        resolver.strength_priority.insert(DriveStrength::None, 0);
        resolver.strength_priority.insert(DriveStrength::Weak, 1);
        resolver.strength_priority.insert(DriveStrength::Strong, 2);
        
        resolver
    }
    
    /// Set resolution strategy
    pub fn set_strategy(&mut self, strategy: ResolutionStrategy) {
        self.strategy = strategy;
    }
    
    /// Resolve drive conflicts on a net
    pub fn resolve_drives(
        &mut self,
        net_name: &str,
        drivers: Vec<DriveInfo>,
    ) -> DriveConflict {
        let start = std::time::Instant::now();
        self.metrics.resolutions_performed += 1;
        
        if drivers.is_empty() {
            return DriveConflict {
                net_name: net_name.to_string(),
                drivers: vec![],
                resolved_value: PinValue::default(),
                conflict_type: DriveConflictType::NoConflict,
            };
        }
        
        if drivers.len() == 1 {
            return DriveConflict {
                net_name: net_name.to_string(),
                drivers: drivers.clone(),
                resolved_value: drivers[0].value.clone(),
                conflict_type: DriveConflictType::NoConflict,
            };
        }
        
        // Detect conflict type
        let conflict_type = self.detect_conflict_type(&drivers);
        if matches!(conflict_type, DriveConflictType::StrongContention | DriveConflictType::BusContention) {
            self.metrics.conflicts_detected += 1;
        }
        
        // Resolve based on strategy
        let resolved_value = match self.strategy {
            ResolutionStrategy::StrongestWins => self.resolve_strongest(&drivers),
            ResolutionStrategy::WiredAnd => self.resolve_wired_and(&drivers),
            ResolutionStrategy::WiredOr => self.resolve_wired_or(&drivers),
            ResolutionStrategy::VoltageAverage => self.resolve_average(&drivers),
        };
        
        self.metrics.resolution_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        DriveConflict {
            net_name: net_name.to_string(),
            drivers,
            resolved_value,
            conflict_type,
        }
    }
    
    /// Detect type of conflict
    fn detect_conflict_type(&self, drivers: &[DriveInfo]) -> DriveConflictType {
        let strong_count = drivers.iter()
            .filter(|d| d.value.drive_strength == DriveStrength::Strong)
            .count();
        
        if strong_count > 1 {
            // Check if they're driving different values
            let strong_drivers: Vec<_> = drivers.iter()
                .filter(|d| d.value.drive_strength == DriveStrength::Strong)
                .collect();
            
            let all_same = strong_drivers.windows(2)
                .all(|w| w[0].value.logic_level == w[1].value.logic_level);
            
            if !all_same {
                return DriveConflictType::StrongContention;
            }
        }
        
        // Check for strength mismatch
        let strengths: Vec<_> = drivers.iter()
            .map(|d| d.value.drive_strength)
            .collect();
        
        if strengths.windows(2).any(|w| w[0] != w[1]) {
            return DriveConflictType::StrengthMismatch;
        }
        
        DriveConflictType::NoConflict
    }
    
    /// Resolve using strongest driver
    fn resolve_strongest(&self, drivers: &[DriveInfo]) -> PinValue {
        let strongest = drivers.iter()
            .max_by_key(|d| self.strength_priority.get(&d.value.drive_strength).unwrap_or(&0))
            .unwrap();
        
        // If multiple at same strength, average voltage
        let same_strength: Vec<_> = drivers.iter()
            .filter(|d| d.value.drive_strength == strongest.value.drive_strength)
            .collect();
        
        if same_strength.len() > 1 {
            let avg_voltage = same_strength.iter()
                .map(|d| d.value.voltage)
                .sum::<f64>() / same_strength.len() as f64;
            
            let mut result = strongest.value.clone();
            result.voltage = avg_voltage;
            
            // Logic level becomes unknown if different
            let all_same_logic = same_strength.windows(2)
                .all(|w| w[0].value.logic_level == w[1].value.logic_level);
            
            if !all_same_logic {
                result.logic_level = Some(LogicLevel::Unknown);
            }
            
            result
        } else {
            strongest.value.clone()
        }
    }
    
    /// Resolve using wired-AND logic
    fn resolve_wired_and(&self, drivers: &[DriveInfo]) -> PinValue {
        let all_high = drivers.iter()
            .all(|d| matches!(d.value.logic_level, Some(LogicLevel::High)));
        
        let mut result = drivers[0].value.clone();
        result.logic_level = Some(if all_high {
            LogicLevel::High
        } else {
            LogicLevel::Low
        });
        
        // Voltage is minimum of all drivers (AND behavior)
        result.voltage = drivers.iter()
            .map(|d| d.value.voltage)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
        
        result
    }
    
    /// Resolve using wired-OR logic
    fn resolve_wired_or(&self, drivers: &[DriveInfo]) -> PinValue {
        let any_high = drivers.iter()
            .any(|d| matches!(d.value.logic_level, Some(LogicLevel::High)));
        
        let mut result = drivers[0].value.clone();
        result.logic_level = Some(if any_high {
            LogicLevel::High
        } else {
            LogicLevel::Low
        });
        
        // Voltage is maximum of all drivers (OR behavior)
        result.voltage = drivers.iter()
            .map(|d| d.value.voltage)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
        
        result
    }
    
    /// Resolve by averaging voltages
    fn resolve_average(&self, drivers: &[DriveInfo]) -> PinValue {
        let avg_voltage = drivers.iter()
            .map(|d| d.value.voltage)
            .sum::<f64>() / drivers.len() as f64;
        
        let avg_current = drivers.iter()
            .map(|d| d.value.current)
            .sum::<f64>() / drivers.len() as f64;
        
        let mut result = drivers[0].value.clone();
        result.voltage = avg_voltage;
        result.current = avg_current;
        
        // Determine logic level from average voltage
        result.logic_level = Some(if avg_voltage > 2.4 {
            LogicLevel::High
        } else if avg_voltage < 0.8 {
            LogicLevel::Low
        } else {
            LogicLevel::Unknown
        });
        
        result
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &DriveMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = DriveMetrics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_driver(name: &str, voltage: f64, strength: DriveStrength, level: LogicLevel) -> DriveInfo {
        DriveInfo {
            pin_name: name.to_string(),
            value: PinValue {
                voltage,
                current: 0.0,
                impedance: 50.0,
                drive_strength: strength,
                logic_level: Some(level),
            },
        }
    }
    
    #[test]
    fn test_strongest_wins() {
        let mut resolver = DriveStrengthResolver::new(ResolutionStrategy::StrongestWins);
        
        let drivers = vec![
            create_driver("U1.OUT", 5.0, DriveStrength::Strong, LogicLevel::High),
            create_driver("U2.OUT", 0.0, DriveStrength::Weak, LogicLevel::Low),
        ];
        
        let conflict = resolver.resolve_drives("NET1", drivers);
        assert_eq!(conflict.resolved_value.voltage, 5.0);
        assert_eq!(conflict.resolved_value.logic_level, Some(LogicLevel::High));
        assert_eq!(conflict.conflict_type, DriveConflictType::StrengthMismatch);
    }
    
    #[test]
    fn test_strong_contention() {
        let mut resolver = DriveStrengthResolver::new(ResolutionStrategy::StrongestWins);
        
        let drivers = vec![
            create_driver("U1.OUT", 5.0, DriveStrength::Strong, LogicLevel::High),
            create_driver("U2.OUT", 0.0, DriveStrength::Strong, LogicLevel::Low),
        ];
        
        let conflict = resolver.resolve_drives("NET1", drivers);
        assert_eq!(conflict.resolved_value.voltage, 2.5); // Average
        assert_eq!(conflict.resolved_value.logic_level, Some(LogicLevel::Unknown));
        assert_eq!(conflict.conflict_type, DriveConflictType::StrongContention);
    }
    
    #[test]
    fn test_wired_and() {
        let mut resolver = DriveStrengthResolver::new(ResolutionStrategy::WiredAnd);
        
        let drivers = vec![
            create_driver("U1.OUT", 5.0, DriveStrength::Strong, LogicLevel::High),
            create_driver("U2.OUT", 4.8, DriveStrength::Strong, LogicLevel::High),
        ];
        
        let conflict = resolver.resolve_drives("NET1", drivers);
        assert_eq!(conflict.resolved_value.voltage, 4.8); // Minimum
        assert_eq!(conflict.resolved_value.logic_level, Some(LogicLevel::High));
    }
}