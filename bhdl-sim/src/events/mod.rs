//! Event system for simulation
//!
//! Provides event-driven simulation capabilities including:
//! - Event scheduling and dispatch
//! - Event listeners and handlers
//! - Event priorities and ordering
//! - Custom event types

pub mod event;
pub mod queue;
pub mod dispatcher;
pub mod listener;

pub use self::event::{SimulationEvent, EventType, EventPriority};
pub use self::queue::{EventQueue, EventId};
pub use self::dispatcher::EventDispatcher;
pub use self::listener::{EventListener, EventHandler, ListenerId};