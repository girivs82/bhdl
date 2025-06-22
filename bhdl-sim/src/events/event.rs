//! Event types and definitions

use std::fmt;
use bhdl_netlist::{InstanceId, NetId};
use crate::circuit::PinValue;

/// Priority levels for events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPriority {
    /// Highest priority - system events
    Critical = 0,
    /// High priority - control events
    High = 1,
    /// Normal priority - most events
    Normal = 2,
    /// Low priority - optional events
    Low = 3,
}

impl Default for EventPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Types of simulation events
#[derive(Debug, Clone)]
pub enum EventType {
    /// Time advance event
    TimeAdvance { 
        from: f64, 
        to: f64 
    },
    
    /// Component evaluation event
    ComponentEvaluation {
        instance: InstanceId,
        scheduled_time: f64,
    },
    
    /// Pin value change event
    PinChange {
        instance: InstanceId,
        pin: String,
        old_value: PinValue,
        new_value: PinValue,
    },
    
    /// Net value change event
    NetChange {
        net: NetId,
        old_voltage: f64,
        new_voltage: f64,
    },
    
    /// Attribute change event
    AttributeChange {
        path: String,
        old_value: Option<String>,
        new_value: String,
    },
    
    /// Simulation state change
    StateChange {
        old_state: String,
        new_state: String,
    },
    
    /// Error event
    Error {
        source: String,
        message: String,
        recoverable: bool,
    },
    
    /// Breakpoint hit event
    BreakpointHit {
        breakpoint_id: u32,
        location: String,
    },
    
    /// Watchpoint triggered event
    WatchpointTriggered {
        watchpoint_id: u32,
        target: String,
        value: String,
    },
    
    /// User-defined event
    Custom {
        name: String,
        data: String,
    },
}

/// A simulation event
#[derive(Debug, Clone)]
pub struct SimulationEvent {
    /// Unique event ID
    pub id: u64,
    /// Simulation time when event occurs
    pub time: f64,
    /// Event priority
    pub priority: EventPriority,
    /// Type of event
    pub event_type: EventType,
    /// Source that generated the event
    pub source: Option<String>,
    /// Optional metadata
    pub metadata: Option<String>,
}

impl SimulationEvent {
    /// Create a new event
    pub fn new(time: f64, event_type: EventType) -> Self {
        Self {
            id: 0, // Will be assigned by queue
            time,
            priority: EventPriority::default(),
            event_type,
            source: None,
            metadata: None,
        }
    }
    
    /// Set priority
    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }
    
    /// Set source
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }
    
    /// Set metadata
    pub fn with_metadata(mut self, metadata: String) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    /// Check if event is an error
    pub fn is_error(&self) -> bool {
        matches!(self.event_type, EventType::Error { .. })
    }
    
    /// Check if event is critical
    pub fn is_critical(&self) -> bool {
        self.priority == EventPriority::Critical
    }
}

impl fmt::Display for SimulationEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:.9}s] {:?}", self.time, self.event_type)
    }
}

/// Event ordering for priority queue
impl PartialEq for SimulationEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.priority == other.priority
    }
}

impl Eq for SimulationEvent {}

impl PartialOrd for SimulationEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SimulationEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap is a max-heap, so we reverse the comparison to get min-heap behavior
        // First by time (earlier time should be popped first)
        match other.time.partial_cmp(&self.time) {
            Some(std::cmp::Ordering::Equal) => {
                // Then by priority (lower enum value = higher priority)
                // Critical (0) should be processed before Low (3)
                // For max-heap, we want Critical to be "greater" than Low
                other.priority.cmp(&self.priority)
            }
            Some(ord) => ord, // Reversed order for time (to make min-heap)
            None => std::cmp::Ordering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_ordering() {
        let e1 = SimulationEvent::new(1.0, EventType::TimeAdvance { from: 0.0, to: 1.0 })
            .with_priority(EventPriority::Normal);
            
        let e2 = SimulationEvent::new(1.0, EventType::TimeAdvance { from: 0.0, to: 1.0 })
            .with_priority(EventPriority::High);
            
        let e3 = SimulationEvent::new(0.5, EventType::TimeAdvance { from: 0.0, to: 0.5 })
            .with_priority(EventPriority::Normal);
            
        // For BinaryHeap (max-heap), greater values have higher priority
        // Earlier time (0.5) should have higher priority than later time (1.0)
        assert!(e3 > e1);
        
        // Same time, higher priority (lower enum value) should have higher priority
        assert!(e2 > e1);
    }
    
    #[test]
    fn test_event_builder() {
        let event = SimulationEvent::new(
            1e-9, 
            EventType::ComponentEvaluation {
                instance: InstanceId::default(),
                scheduled_time: 1e-9,
            }
        )
        .with_priority(EventPriority::High)
        .with_source("scheduler".to_string())
        .with_metadata("first evaluation".to_string());
        
        assert_eq!(event.time, 1e-9);
        assert_eq!(event.priority, EventPriority::High);
        assert_eq!(event.source, Some("scheduler".to_string()));
        assert_eq!(event.metadata, Some("first evaluation".to_string()));
    }
}