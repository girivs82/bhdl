//! Event dispatcher for routing events to listeners

use std::sync::{Arc, Mutex};
use crossbeam_channel::{Sender, Receiver, bounded};
use crate::error::SimulationResult;
use super::event::{SimulationEvent, EventType};
use super::queue::{EventQueue, EventId};
use super::listener::{ListenerRegistry, EventListener, ListenerId};

/// Event dispatcher configuration
#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    /// Maximum events in queue
    pub max_queue_size: usize,
    /// Channel buffer size
    pub channel_size: usize,
    /// Whether to log events
    pub log_events: bool,
    /// Whether to collect metrics
    pub collect_metrics: bool,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 10000,
            channel_size: 1000,
            log_events: false,
            collect_metrics: true,
        }
    }
}

/// Event dispatcher
pub struct EventDispatcher {
    /// Configuration
    config: DispatcherConfig,
    /// Event queue
    queue: Arc<Mutex<EventQueue>>,
    /// Listener registry
    listeners: Arc<Mutex<ListenerRegistry>>,
    /// Event channel for async dispatch
    event_tx: Sender<SimulationEvent>,
    event_rx: Receiver<SimulationEvent>,
    /// Metrics
    events_dispatched: u64,
    events_dropped: u64,
}

impl EventDispatcher {
    /// Create a new dispatcher
    pub fn new(config: DispatcherConfig) -> Self {
        let (tx, rx) = bounded(config.channel_size);
        
        Self {
            queue: Arc::new(Mutex::new(EventQueue::new(config.max_queue_size))),
            listeners: Arc::new(Mutex::new(ListenerRegistry::new())),
            event_tx: tx,
            event_rx: rx,
            config,
            events_dispatched: 0,
            events_dropped: 0,
        }
    }
    
    /// Schedule an event
    pub fn schedule(&self, event: SimulationEvent) -> SimulationResult<EventId> {
        let mut queue = self.queue.lock().unwrap();
        queue.schedule(event)
    }
    
    /// Schedule immediate event
    pub fn schedule_immediate(&self, event_type: EventType) -> SimulationResult<EventId> {
        let mut queue = self.queue.lock().unwrap();
        queue.schedule_immediate(event_type)
    }
    
    /// Schedule delayed event
    pub fn schedule_delayed(
        &self, 
        delay: f64, 
        event_type: EventType
    ) -> SimulationResult<EventId> {
        let mut queue = self.queue.lock().unwrap();
        queue.schedule_delayed(delay, event_type)
    }
    
    /// Cancel an event
    pub fn cancel(&self, event_id: EventId) -> SimulationResult<()> {
        let mut queue = self.queue.lock().unwrap();
        queue.cancel(event_id)
    }
    
    /// Register a listener
    pub fn register_listener(&self, listener: EventListener) -> ListenerId {
        let mut registry = self.listeners.lock().unwrap();
        registry.register(listener)
    }
    
    /// Unregister a listener
    pub fn unregister_listener(&self, id: ListenerId) {
        let mut registry = self.listeners.lock().unwrap();
        registry.unregister(id);
    }
    
    /// Process next event
    pub fn process_next(&mut self) -> Option<SimulationEvent> {
        let event = {
            let mut queue = self.queue.lock().unwrap();
            queue.next()
        };
        
        if let Some(ref event) = event {
            self.dispatch(event);
        }
        
        event
    }
    
    /// Process all events up to time
    pub fn process_until(&mut self, time: f64) -> Vec<SimulationEvent> {
        let mut processed = Vec::new();
        
        loop {
            let next_time = {
                let queue = self.queue.lock().unwrap();
                queue.next_time()
            };
            
            match next_time {
                Some(t) if t <= time => {
                    if let Some(event) = self.process_next() {
                        processed.push(event);
                    }
                }
                _ => break,
            }
        }
        
        processed
    }
    
    /// Dispatch event to listeners
    fn dispatch(&mut self, event: &SimulationEvent) {
        if self.config.log_events {
            tracing::debug!("Dispatching event: {}", event);
        }
        
        // Get listener IDs that accept the event
        let listener_ids = {
            let mut registry = self.listeners.lock().unwrap();
            registry.get_listeners_mut(event)
        };
        
        // Dispatch to each listener
        for id in listener_ids {
            let mut registry = self.listeners.lock().unwrap();
            if let Some(listener) = registry.listeners.get_mut(&id) {
                listener.handle(event);
            }
        }
        
        // Try to send to async channel
        if let Err(_) = self.event_tx.try_send(event.clone()) {
            self.events_dropped += 1;
            if self.config.log_events {
                tracing::warn!("Event channel full, dropping event");
            }
        }
        
        self.events_dispatched += 1;
    }
    
    /// Get event receiver for async processing
    pub fn event_receiver(&self) -> Receiver<SimulationEvent> {
        self.event_rx.clone()
    }
    
    /// Clear all events
    pub fn clear(&self) {
        let mut queue = self.queue.lock().unwrap();
        queue.clear();
    }
    
    /// Get current simulation time
    pub fn current_time(&self) -> f64 {
        let queue = self.queue.lock().unwrap();
        queue.current_time()
    }
    
    /// Check if there are pending events
    pub fn has_events(&self) -> bool {
        let queue = self.queue.lock().unwrap();
        !queue.is_empty()
    }
    
    /// Get dispatcher statistics
    pub fn statistics(&self) -> DispatcherStats {
        let queue_stats = {
            let queue = self.queue.lock().unwrap();
            queue.statistics()
        };
        
        let listener_stats = {
            let registry = self.listeners.lock().unwrap();
            registry.statistics()
        };
        
        DispatcherStats {
            events_dispatched: self.events_dispatched,
            events_dropped: self.events_dropped,
            queue_size: queue_stats.current_size,
            queue_peak: queue_stats.peak_size,
            total_events: queue_stats.total_events,
            current_time: queue_stats.current_time,
            listener_count: listener_stats.total_listeners,
            enabled_listeners: listener_stats.enabled_listeners,
        }
    }
}

/// Dispatcher statistics
#[derive(Debug, Clone)]
pub struct DispatcherStats {
    pub events_dispatched: u64,
    pub events_dropped: u64,
    pub queue_size: usize,
    pub queue_peak: usize,
    pub total_events: u64,
    pub current_time: f64,
    pub listener_count: usize,
    pub enabled_listeners: usize,
}

/// Async event processor
pub struct AsyncEventProcessor {
    receiver: Receiver<SimulationEvent>,
    handlers: Vec<Box<dyn Fn(&SimulationEvent) + Send>>,
}

impl AsyncEventProcessor {
    /// Create a new async processor
    pub fn new(receiver: Receiver<SimulationEvent>) -> Self {
        Self {
            receiver,
            handlers: Vec::new(),
        }
    }
    
    /// Add a handler
    pub fn add_handler<F>(&mut self, handler: F)
    where
        F: Fn(&SimulationEvent) + Send + 'static
    {
        self.handlers.push(Box::new(handler));
    }
    
    /// Run the processor
    pub async fn run(&mut self) {
        while let Ok(event) = self.receiver.recv() {
            for handler in &self.handlers {
                handler(&event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    
    #[test]
    fn test_event_dispatch() {
        let mut dispatcher = EventDispatcher::new(DispatcherConfig::default());
        let counter = Arc::new(AtomicU32::new(0));
        
        // Register listener
        let counter_clone = counter.clone();
        let handler = Arc::new(move |_event: &SimulationEvent| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });
        
        let listener = EventListener::new("counter".to_string(), handler);
        dispatcher.register_listener(listener);
        
        // Schedule and process events
        dispatcher.schedule_immediate(
            EventType::TimeAdvance { from: 0.0, to: 1.0 }
        ).unwrap();
        dispatcher.schedule_immediate(
            EventType::TimeAdvance { from: 1.0, to: 2.0 }
        ).unwrap();
        
        dispatcher.process_next();
        dispatcher.process_next();
        
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }
    
    #[test]
    fn test_process_until() {
        let mut dispatcher = EventDispatcher::new(DispatcherConfig::default());
        
        // Schedule events at different times
        dispatcher.schedule(
            SimulationEvent::new(1.0, EventType::TimeAdvance { from: 0.0, to: 1.0 })
        ).unwrap();
        dispatcher.schedule(
            SimulationEvent::new(2.0, EventType::TimeAdvance { from: 1.0, to: 2.0 })
        ).unwrap();
        dispatcher.schedule(
            SimulationEvent::new(3.0, EventType::TimeAdvance { from: 2.0, to: 3.0 })
        ).unwrap();
        
        // Process until time 2.5
        let processed = dispatcher.process_until(2.5);
        
        assert_eq!(processed.len(), 2);
        assert_eq!(processed[0].time, 1.0);
        assert_eq!(processed[1].time, 2.0);
    }
}