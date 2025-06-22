//! Simulation metrics and statistics collection

pub mod collector;
pub mod stats;
pub mod report;
pub mod simple;

pub use collector::{MetricsCollector, MetricType, MetricValue};
pub use stats::{SimulationStats, ComponentStats, NetStats, PerformanceStats};
pub use report::{MetricsReport, ReportFormat, generate_report};