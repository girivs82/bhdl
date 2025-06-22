//! Event queue implementation

use std::collections::{BinaryHeap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use crate::error::{SimulationResult, SimulationError};
use super::event::{SimulationEvent, EventType, EventPriority};

/// Event ID type
pub type EventId = u64;

/// Event queue for simulation
pub struct EventQueue {
    /// Priority queue of future events
    future_events: BinaryHeap<SimulationEvent>,
    /// Events for current time
    current_events: VecDeque<SimulationEvent>,
    /// Event ID generator
    next_id: AtomicU64,
    /// Current simulation time
    current_time: f64,
    /// Maximum queue size
    max_size: usize,
    /// Statistics
    total_events: u64,
    peak_size: usize,
}

impl EventQueue {
    /// Create a new event queue
    pub fn new(max_size: usize) -> Self {
        Self {
            future_events: BinaryHeap::new(),
            current_events: VecDeque::new(),
            next_id: AtomicU64::new(1),
            current_time: 0.0,
            max_size,
            total_events: 0,
            peak_size: 0,
        }
    }
    
    /// Schedule an event
    pub fn schedule(&mut self, mut event: SimulationEvent) -> SimulationResult<EventId> {
        // Check queue size
        if self.size() >= self.max_size {
            return Err(SimulationError::EventQueueFull(self.max_size));
        }
        
        // Assign ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        event.id = id;
        
        // Validate time
        if event.time < self.current_time {
            return Err(SimulationError::InvalidEventTime {
                event_time: event.time,
                current_time: self.current_time,
            });
        }
        
        // Add to appropriate queue
        if (event.time - self.current_time).abs() < 1e-15 {
            self.current_events.push_back(event);
        } else {
            self.future_events.push(event);
        }
        
        // Update statistics
        self.total_events += 1;
        self.peak_size = self.peak_size.max(self.size());
        
        Ok(id)
    }
    
    /// Schedule event at current time
    pub fn schedule_immediate(&mut self, event_type: EventType) -> SimulationResult<EventId> {
        let event = SimulationEvent::new(self.current_time, event_type);
        self.schedule(event)
    }
    
    /// Schedule event with delay
    pub fn schedule_delayed(
        &mut self, 
        delay: f64, 
        event_type: EventType
    ) -> SimulationResult<EventId> {
        if delay < 0.0 {
            return Err(SimulationError::InvalidEventTime {
                event_time: self.current_time + delay,
                current_time: self.current_time,
            });
        }
        
        let event = SimulationEvent::new(self.current_time + delay, event_type);
        self.schedule(event)
    }
    
    /// Cancel an event
    pub fn cancel(&mut self, event_id: EventId) -> SimulationResult<()> {
        // Note: This is inefficient for BinaryHeap
        // In production, would use a different data structure
        let mut temp = Vec::new();
        let mut found = false;
        
        // Check current events
        self.current_events.retain(|e| {
            if e.id == event_id {
                found = true;
                false
            } else {
                true
            }
        });
        
        if !found {
            // Check future events
            while let Some(event) = self.future_events.pop() {
                if event.id != event_id {
                    temp.push(event);
                } else {
                    found = true;
                }
            }
            
            // Restore events
            for event in temp {
                self.future_events.push(event);
            }
        }
        
        if found {
            Ok(())
        } else {
            Err(SimulationError::EventNotFound(event_id))
        }
    }
    
    /// Get next event
    pub fn next(&mut self) -> Option<SimulationEvent> {
        // First check current time events
        if let Some(event) = self.current_events.pop_front() {
            return Some(event);
        }
        
        // Then check future events
        if let Some(event) = self.future_events.pop() {
            self.current_time = event.time;
            
            // Move all events at this time to current queue
            while let Some(next) = self.future_events.peek() {
                if (next.time - self.current_time).abs() < 1e-15 {
                    if let Some(e) = self.future_events.pop() {
                        self.current_events.push_back(e);
                    }
                } else {
                    break;
                }
            }
            
            return Some(event);
        }
        
        None
    }
    
    /// Peek at next event without removing
    pub fn peek(&self) -> Option<&SimulationEvent> {
        self.current_events.front()
            .or_else(|| self.future_events.peek())
    }
    
    /// Get next event time
    pub fn next_time(&self) -> Option<f64> {
        self.peek().map(|e| e.time)
    }
    
    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.current_events.is_empty() && self.future_events.is_empty()
    }
    
    /// Get queue size
    pub fn size(&self) -> usize {
        self.current_events.len() + self.future_events.len()
    }
    
    /// Get current time
    pub fn current_time(&self) -> f64 {
        self.current_time
    }
    
    /// Clear all events
    pub fn clear(&mut self) {
        self.current_events.clear();
        self.future_events.clear();
    }
    
    /// Get queue statistics
    pub fn statistics(&self) -> EventQueueStats {
        EventQueueStats {
            current_size: self.size(),
            peak_size: self.peak_size,
            total_events: self.total_events,
            current_time: self.current_time,
        }
    }
}

/// Event queue statistics
#[derive(Debug, Clone)]
pub struct EventQueueStats {
    pub current_size: usize,
    pub peak_size: usize,
    pub total_events: u64,
    pub current_time: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_scheduling() {
        let mut queue = EventQueue::new(100);
        
        // Schedule events
        let e1 = EventType::TimeAdvance { from: 0.0, to: 1.0 };
        let e2 = EventType::TimeAdvance { from: 1.0, to: 2.0 };
        let e3 = EventType::TimeAdvance { from: 0.5, to: 1.5 };
        
        queue.schedule(SimulationEvent::new(1.0, e1)).unwrap();
        queue.schedule(SimulationEvent::new(2.0, e2)).unwrap();
        queue.schedule(SimulationEvent::new(0.5, e3)).unwrap();
        
        // Events should come out in time order
        assert_eq!(queue.next().unwrap().time, 0.5);
        assert_eq!(queue.next().unwrap().time, 1.0);
        assert_eq!(queue.next().unwrap().time, 2.0);
        assert!(queue.next().is_none());
    }
    
    #[test]
    fn test_immediate_events() {
        let mut queue = EventQueue::new(100);
        queue.current_time = 1.0;
        
        let e1 = EventType::TimeAdvance { from: 0.0, to: 1.0 };
        queue.schedule_immediate(e1.clone()).unwrap();
        queue.schedule_immediate(e1.clone()).unwrap();
        
        // Both should be at current time
        assert_eq!(queue.next().unwrap().time, 1.0);
        assert_eq!(queue.next().unwrap().time, 1.0);
    }
    
    #[test]
    fn test_event_cancellation() {
        let mut queue = EventQueue::new(100);
        
        let e1 = EventType::TimeAdvance { from: 0.0, to: 1.0 };
        let id = queue.schedule(SimulationEvent::new(1.0, e1)).unwrap();
        
        assert_eq!(queue.size(), 1);
        queue.cancel(id).unwrap();
        assert_eq!(queue.size(), 0);
    }
    
    #[test]
    fn test_priority_ordering() {
        let mut queue = EventQueue::new(100);
        
        // Same time, different priorities
        let e1 = SimulationEvent::new(1.0, EventType::TimeAdvance { from: 0.0, to: 1.0 })
            .with_priority(EventPriority::Low);
        let e2 = SimulationEvent::new(1.0, EventType::TimeAdvance { from: 0.0, to: 1.0 })
            .with_priority(EventPriority::Critical);
        let e3 = SimulationEvent::new(1.0, EventType::TimeAdvance { from: 0.0, to: 1.0 })
            .with_priority(EventPriority::Normal);
            
        queue.schedule(e1).unwrap();
        queue.schedule(e2).unwrap();
        queue.schedule(e3).unwrap();
        
        // Critical should come first
        assert_eq!(queue.next().unwrap().priority, EventPriority::Critical);
        assert_eq!(queue.next().unwrap().priority, EventPriority::Normal);
        assert_eq!(queue.next().unwrap().priority, EventPriority::Low);
    }
}