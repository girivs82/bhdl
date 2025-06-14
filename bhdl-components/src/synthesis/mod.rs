//! Component synthesis module
//!
//! This module provides two-stage component synthesis:
//! 1. Spec-based selection from local database (fast, comprehensive)
//! 2. Live supplier lookup for shortlisted candidates (accurate, limited scope)
//!
//! Due to API limits (50 parts/request) and data volatility (daily price changes),
//! we avoid large-scale caching and focus on targeted real-time lookups.

pub mod engine;
pub mod matcher;
pub mod optimizer;
pub mod synthesizer;
pub mod selector;
pub mod two_stage;

// Re-export synthesis engines
pub use engine::SynthesisEngine;
pub use two_stage::{TwoStageSynthesizer, TwoStageConfig};

use crate::types::*;
use crate::database::ComponentDatabase;

/// Main component synthesizer (legacy interface - use SynthesisEngine for new code)
pub struct ComponentSynthesizer {
    engine: SynthesisEngine,
}

impl ComponentSynthesizer {
    pub fn new() -> Self {
        Self {
            engine: SynthesisEngine::new(),
        }
    }
    
    pub async fn synthesize(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
    ) -> anyhow::Result<SynthesisResult> {
        // Delegate to the new synthesis engine
        self.engine.synthesize_component(component_type, requirements, database).await
    }
}

impl Default for ComponentSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}