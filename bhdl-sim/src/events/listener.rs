//! Event listener and handler system

use std::collections::HashMap;
use std::sync::Arc;
use super::event::{SimulationEvent, EventType};

/// Listener ID type
pub type ListenerId = u32;

/// Event handler function type
pub type EventHandler = Arc<dyn Fn(&SimulationEvent) + Send + Sync>;

/// Event filter predicate
pub type EventFilter = Arc<dyn Fn(&SimulationEvent) -> bool + Send + Sync>;

/// Event listener
pub struct EventListener {
    /// Unique ID
    pub id: ListenerId,
    /// Name for debugging
    pub name: String,
    /// Handler function
    pub handler: EventHandler,
    /// Optional filter
    pub filter: Option<EventFilter>,
    /// Whether listener is enabled
    pub enabled: bool,
    /// Number of events handled
    pub event_count: u64,
}

impl EventListener {
    /// Create a new listener
    pub fn new(name: String, handler: EventHandler) -> Self {
        Self {
            id: 0, // Will be assigned by registry
            name,
            handler,
            filter: None,
            enabled: true,
            event_count: 0,
        }
    }
    
    /// Add a filter
    pub fn with_filter(mut self, filter: EventFilter) -> Self {
        self.filter = Some(filter);
        self
    }
    
    /// Check if event passes filter
    pub fn accepts(&self, event: &SimulationEvent) -> bool {
        if !self.enabled {
            return false;
        }
        
        if let Some(filter) = &self.filter {
            filter(event)
        } else {
            true
        }
    }
    
    /// Handle an event
    pub fn handle(&mut self, event: &SimulationEvent) {
        if self.accepts(event) {
            (self.handler)(event);
            self.event_count += 1;
        }
    }
}

/// Listener registry
pub struct ListenerRegistry {
    /// All listeners
    pub(crate) listeners: HashMap<ListenerId, EventListener>,
    /// Next listener ID
    next_id: u32,
    /// Listeners by event type (for optimization)
    type_listeners: HashMap<String, Vec<ListenerId>>,
}

impl ListenerRegistry {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            next_id: 1,
            type_listeners: HashMap::new(),
        }
    }
    
    /// Register a listener
    pub fn register(&mut self, mut listener: EventListener) -> ListenerId {
        let id = self.next_id;
        self.next_id += 1;
        listener.id = id;
        
        self.listeners.insert(id, listener);
        id
    }
    
    /// Unregister a listener
    pub fn unregister(&mut self, id: ListenerId) -> Option<EventListener> {
        self.listeners.remove(&id)
    }
    
    /// Enable/disable listener
    pub fn set_enabled(&mut self, id: ListenerId, enabled: bool) {
        if let Some(listener) = self.listeners.get_mut(&id) {
            listener.enabled = enabled;
        }
    }
    
    /// Get all listeners for an event
    pub fn get_listeners(&self, event: &SimulationEvent) -> Vec<&EventListener> {
        self.listeners.values()
            .filter(|l| l.accepts(event))
            .collect()
    }
    
    /// Get mutable listeners
    pub fn get_listeners_mut(&mut self, event: &SimulationEvent) -> Vec<ListenerId> {
        self.listeners.iter()
            .filter(|(_, listener)| listener.accepts(event))
            .map(|(id, _)| *id)
            .collect()
    }
    
    /// Clear all listeners
    pub fn clear(&mut self) {
        self.listeners.clear();
        self.type_listeners.clear();
    }
    
    /// Get listener count
    pub fn count(&self) -> usize {
        self.listeners.len()
    }
    
    /// Get listener statistics
    pub fn statistics(&self) -> ListenerStats {
        let total_events: u64 = self.listeners.values()
            .map(|l| l.event_count)
            .sum();
            
        let enabled_count = self.listeners.values()
            .filter(|l| l.enabled)
            .count();
            
        ListenerStats {
            total_listeners: self.listeners.len(),
            enabled_listeners: enabled_count,
            total_events_handled: total_events,
        }
    }
}

/// Listener statistics
#[derive(Debug, Clone)]
pub struct ListenerStats {
    pub total_listeners: usize,
    pub enabled_listeners: usize,
    pub total_events_handled: u64,
}

/// Common event filters
pub mod filters {
    use super::*;
    use crate::events::EventPriority;
    
    /// Filter by event type
    pub fn by_type<F>(predicate: F) -> EventFilter
    where
        F: Fn(&EventType) -> bool + Send + Sync + 'static
    {
        Arc::new(move |event| predicate(&event.event_type))
    }
    
    /// Filter by time range
    pub fn by_time_range(start: f64, end: f64) -> EventFilter {
        Arc::new(move |event| event.time >= start && event.time <= end)
    }
    
    /// Filter by priority
    pub fn by_priority_min(min_priority: EventPriority) -> EventFilter {
        Arc::new(move |event| event.priority <= min_priority)
    }
    
    /// Filter by source
    pub fn by_source(source: String) -> EventFilter {
        Arc::new(move |event| {
            event.source.as_ref() == Some(&source)
        })
    }
    
    /// Combine filters with AND
    pub fn and(f1: EventFilter, f2: EventFilter) -> EventFilter {
        Arc::new(move |event| f1(event) && f2(event))
    }
    
    /// Combine filters with OR
    pub fn or(f1: EventFilter, f2: EventFilter) -> EventFilter {
        Arc::new(move |event| f1(event) || f2(event))
    }
    
    /// Negate a filter
    pub fn not(filter: EventFilter) -> EventFilter {
        Arc::new(move |event| !filter(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::filters::*;
    use crate::events::{EventType, EventPriority};
    
    #[test]
    fn test_listener_registration() {
        let mut registry = ListenerRegistry::new();
        
        let handler = Arc::new(|_event: &SimulationEvent| {
            println!("Event handled");
        });
        
        let listener = EventListener::new("test".to_string(), handler);
        let id = registry.register(listener);
        
        assert_eq!(registry.count(), 1);
        registry.unregister(id);
        assert_eq!(registry.count(), 0);
    }
    
    #[test]
    fn test_event_filtering() {
        let filter = by_time_range(1.0, 2.0);
        
        let e1 = SimulationEvent::new(0.5, EventType::TimeAdvance { from: 0.0, to: 0.5 });
        let e2 = SimulationEvent::new(1.5, EventType::TimeAdvance { from: 1.0, to: 1.5 });
        let e3 = SimulationEvent::new(2.5, EventType::TimeAdvance { from: 2.0, to: 2.5 });
        
        assert!(!filter(&e1));
        assert!(filter(&e2));
        assert!(!filter(&e3));
    }
    
    #[test]
    fn test_filter_combination() {
        let f1 = by_time_range(1.0, 3.0);
        let f2 = by_priority_min(EventPriority::High);
        let combined = and(f1, f2);
        
        let event = SimulationEvent::new(2.0, EventType::TimeAdvance { from: 1.0, to: 2.0 })
            .with_priority(EventPriority::Normal);
            
        assert!(by_time_range(1.0, 3.0)(&event));
        assert!(!by_priority_min(EventPriority::High)(&event));
        assert!(!combined(&event));
    }
}